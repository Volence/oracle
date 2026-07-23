//! Slice S4 end-to-end probe (NOT a CI test — the commercial ROM must never enter the repo). Boots the real
//! header-less battery cart `Sonic & Knuckles + Sonic The Hedgehog 3 (USA)` (blank `$1B0` header, saves via
//! the `$A130F1` access latch — `skdisasm/sonic3k.constants.asm:218`, `sonic3k.asm:344/15756`) and reports how
//! far it gets: does it boot, is the fallback SRAM map provisioned, does the game ENABLE `$A130F1`, does it
//! WRITE SRAM, and does a `.srm` image round-trip through a fresh `System`.
//!
//! Run: `cargo run --release -p oracle-core --example s3k_sram_probe [rom_path]`

use oracle_core::io::Pad;
use oracle_core::system::System;

const DEFAULT_ROM: &str =
    "/home/volence/sonic_hacks/Sonic & Knuckles + Sonic The Hedgehog 3 (USA).md";

/// Run `frames` one at a time, noting the first frame at which `$A130F1` enable and the first guest SRAM
/// write are observed. Returns `(first_enabled_frame, first_write_frame)`.
fn run_and_watch(
    sys: &mut System,
    start: u64,
    frames: u64,
    pad: Pad,
) -> (Option<u64>, Option<u64>) {
    sys.set_pad(0, pad);
    let mut first_enabled = None;
    let mut first_write = None;
    for i in 0..frames {
        sys.run_frames(1);
        let f = start + i;
        if first_enabled.is_none() && sys.sram_enabled() {
            first_enabled = Some(f);
        }
        if first_write.is_none() && sys.sram_used() {
            first_write = Some(f);
        }
    }
    (first_enabled, first_write)
}

fn nonzero_sram_bytes(sys: &System) -> usize {
    sys.sram().iter().filter(|&&b| b != 0).count()
}

fn main() {
    let rom_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_ROM.to_string());
    let rom = match std::fs::read(&rom_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("could not read ROM at {rom_path}: {e}");
            std::process::exit(1);
        }
    };
    println!("ROM: {rom_path} ({} bytes)", rom.len());
    let header_magic = &rom[0x1B0..0x1B2];
    println!(
        "$1B0 header magic: {:02X} {:02X} ({})",
        header_magic[0],
        header_magic[1],
        if header_magic == b"RA" {
            "\"RA\" present"
        } else {
            "no \"RA\" → header-less fallback path"
        }
    );

    let mut sys = System::new(0x5EED);
    sys.load_rom(rom.clone());
    sys.reset();
    // The base/end fields are private; the fallback map is $200001-$20FFFF (odd-byte) → a 32 KiB chip, and
    // sram().len() is the ground truth for what load_rom provisioned.
    println!(
        "booted OK. sram_present={} fallback_window=$200001-$20FFFF (odd-byte) buffer={} bytes",
        sys.sram_present(),
        sys.sram().len(),
    );

    // Phase 1: pure autoplay (no input) — SEGA screen, title, "SONIC 3" attract. S3K reads SRAM during boot
    // (to find saved games for the data-select), so $A130F1 should enable even without input.
    let (en1, wr1) = run_and_watch(&mut sys, 0, 1200, Pad::default());
    println!(
        "after 1200 autoplay frames: $A130F1 enabled={:?}, first SRAM write frame={:?}, sram_used={}, sram_dirty={}, non-zero SRAM bytes={}",
        en1,
        wr1,
        sys.sram_used(),
        sys.sram_dirty(),
        nonzero_sram_bytes(&sys),
    );

    // Phase 2: if no write yet, script a minimal input sequence to reach a save (press Start, then C several
    // times) — attract → data-select → begin, which S3K persists.
    if !sys.sram_used() {
        println!("no SRAM write during autoplay — scripting Start/C to reach a save…");
        let press = |sys: &mut System, start, n, pad| run_and_watch(sys, start, n, pad);
        let mut frame = 1200;
        for round in 0..8 {
            let (_e, w) = press(
                &mut sys,
                frame,
                20,
                Pad {
                    start: true,
                    ..Default::default()
                },
            );
            frame += 20;
            if w.is_some() {
                break;
            }
            let (_e2, w2) = press(
                &mut sys,
                frame,
                20,
                Pad {
                    c: true,
                    ..Default::default()
                },
            );
            frame += 20;
            if w2.is_some() {
                break;
            }
            let (_e3, w3) = press(&mut sys, frame, 40, Pad::default());
            frame += 40;
            if w3.is_some() {
                break;
            }
            println!(
                "  round {round}: still no write (frame {frame}), enabled={}",
                sys.sram_enabled()
            );
        }
        println!(
            "after scripted input: $A130F1 enabled={}, sram_used={}, sram_dirty={}, non-zero SRAM bytes={}",
            sys.sram_enabled(),
            sys.sram_used(),
            sys.sram_dirty(),
            nonzero_sram_bytes(&sys),
        );
    }

    // Phase 3: .srm round-trip. Save the live SRAM bytes to a temp file, build a FRESH System, load_rom +
    // load_sram, and assert the bytes match. This demonstrates the persistence path regardless of whether the
    // guest managed a save within the frame budget (an all-zero buffer round-trips too — the mechanism is what
    // is under test here).
    let saved = sys.sram().to_vec();
    let tmp = std::env::temp_dir().join("s3k_sram_probe.srm");
    std::fs::write(&tmp, &saved).expect("write temp .srm");
    let reloaded = std::fs::read(&tmp).expect("read temp .srm");
    let mut fresh = System::new(0x5EED);
    fresh.load_rom(rom);
    fresh.load_sram(&reloaded);
    let round_trips = fresh.sram() == saved.as_slice();
    println!(
        ".srm round-trip: wrote {} bytes to {}, reloaded into a fresh System → bytes match: {}",
        saved.len(),
        tmp.display(),
        round_trips,
    );
    let _ = std::fs::remove_file(&tmp);
    assert!(
        round_trips,
        "SRAM bytes must survive a save/load round-trip"
    );

    println!("\nSUMMARY:");
    println!("  boot: OK (no panic)");
    println!("  fallback SRAM provisioned: {}", sys.sram_present());
    println!(
        "  game enabled $A130F1: {}",
        sys.sram_enabled() || en1.is_some()
    );
    println!("  game wrote SRAM (sram_used): {}", sys.sram_used());
    println!("  .srm round-trip: {round_trips}");
}
