//! Phase-1 root-cause diagnostic (sound-driver silence): instrument the 68k↔Z80-RAM channel and the
//! FM/PSG init write stream for `s4.soundtest.bin`.
//!
//! Pure instrumentation — boots a ROM *file*, runs N frames with a caller-owned [`BusEventSink`], and
//! reports (1) every 68k→Z80-RAM-window write (`$A00000-$A0FFFF`, where the SMPS sound queue lives), with
//! frame/addr/value/size/fc; (2) an fc-attribution tally of the FM (`$4000-$4003`/`$A04000-$A04003`) and PSG
//! (`$7F11`/`$C00011`) writes (fc 0 = Z80 master, 5/6 = 68000 master); (3) the first 64 bytes of Z80 RAM at
//! the end of the run. Touches no `src/` state; uses only the public API.
//!
//! **This example is now configuration, not a sink.** It used to hand-roll 81 lines of `BusEventSink`: a
//! `Rec` struct that re-declared `BusEvent` field-for-field plus a copied frame, a frame latch, two
//! address-classifier free functions, three unbounded `Vec<Rec>`, and three `BTreeMap<u8, u64>` fc tallies
//! — with bounding applied only at print time. All of it is now `Watchpoints` configuration: twelve
//! `add`-calls (three instruments × two address windows × [`WatchMode::Record`] + `Census(Fc)`)
//! and no `BusEventSink` impl at all (`docs/2026-08-14-trace-recorder-design.md` §9). The instrument gained
//! what the hand-rolled version never
//! had: a per-access `mclk`, record-time bounding with a drop count, the `seen` negative control, and the
//! master-attribution caveat on the PSG port that the hand-rolled fc tally silently got wrong.
//!
//! Usage: `cargo run --release -p oracle-core --example diag_soundqueue -- [rom.bin] [frames]`

use oracle_core::system::System;
use oracle_core::watchpoints::{
    CensusKey, Watch, WatchHit, WatchId, WatchMode, WatchOp, Watchpoints,
};
use std::collections::BTreeMap;
use std::ops::RangeInclusive;

const DEFAULT_ROM: &str = "/home/volence/sonic_hacks/aeon/s4.soundtest.bin";
const DEFAULT_FRAMES: u64 = 600;
/// Hit-ring capacity. Bounding is now record-time rather than print-time, so this is the honest memory
/// bound of the run; `dropped` reports any loss instead of it being invisible.
const HIT_CAP: usize = 1 << 18;

/// One logical instrument: a set of address ranges watched twice over — once in `Record` mode (the event
/// log) and once as `Census(Fc)` (the master tally). Two ranges are needed wherever one chip answers at two
/// windows (the FM at `$4000`/`$A04000`, the PSG at `$7F11`/`$C00011`), and the ids keep them distinguishable.
struct Group {
    log: Vec<WatchId>,
    fc: Vec<WatchId>,
}

impl Group {
    fn register(wp: &mut Watchpoints, label: &str, ranges: &[RangeInclusive<u32>]) -> Self {
        let mut log = Vec::new();
        let mut fc = Vec::new();
        for r in ranges {
            log.push(wp.add(Watch::bus(r.clone(), WatchOp::Write, label)));
            fc.push(
                wp.add(
                    Watch::bus(r.clone(), WatchOp::Write, format!("{label}.fc"))
                        .mode(WatchMode::Census(CensusKey::Fc)),
                ),
            );
        }
        Self { log, fc }
    }

    /// Every recorded write in this group, in the order the machine made them (the shared ring is ordered by
    /// `seq`, so filtering it by this group's watch ids preserves that order exactly).
    fn hits(&self, wp: &Watchpoints) -> Vec<WatchHit> {
        wp.hits()
            .iter()
            .filter(|h| self.log.contains(&h.watch))
            .copied()
            .collect()
    }

    /// The fc tally, folded back over this group's windows.
    fn fc_tally(&self, wp: &Watchpoints) -> BTreeMap<u8, u64> {
        let mut out = BTreeMap::new();
        for id in &self.fc {
            for (k, n) in wp.watch(*id).unwrap().census.unwrap_or_default() {
                *out.entry(k as u8).or_insert(0) += n;
            }
        }
        out
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let rom_path = args.next().unwrap_or_else(|| DEFAULT_ROM.to_string());
    let frames: u64 = args
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_FRAMES);

    let rom = match std::fs::read(&rom_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("cannot read ROM {rom_path}: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "ROM {rom_path}: {} bytes, running {frames} frames",
        rom.len()
    );

    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();

    let mut wp = Watchpoints::new(HIT_CAP);
    // The FM ports alias *inside* the Z80-RAM window but are the YM2612, not Z80 RAM — so the Z80-RAM watch
    // is the window with those four bytes peeled out, expressed as two ranges rather than as an `if` in a
    // hand-written sink.
    let fm = Group::register(&mut wp, "fm", &[0x4000..=0x4003, 0xA0_4000..=0xA0_4003]);
    let psg = Group::register(&mut wp, "psg", &[0x7F11..=0x7F11, 0xC0_0011..=0xC0_0011]);
    let z80_ram = Group::register(
        &mut wp,
        "z80ram",
        &[0xA0_0000..=0xA0_3FFF, 0xA0_4004..=0xA0_FFFF],
    );
    sys.run_frames_with_sink(frames, &mut wp);

    // ---- 68k -> Z80-RAM-window writes ------------------------------------------------------------------
    let z80_ram_writes = z80_ram.hits(&wp);
    println!("\n=== 68k->Z80-RAM-window writes ($A00000-$A0FFFF, FM ports excluded) ===");
    println!("count: {}", z80_ram_writes.len());
    println!("fc tally: {:?}", z80_ram.fc_tally(&wp));
    for r in &z80_ram_writes {
        let nz = if r.value != 0 { "  <== NON-ZERO" } else { "" };
        println!(
            "  frame={:4} {:?} addr={:06X} size={:?} value={:04X} fc={}{}",
            r.frame, r.op, r.addr, r.size, r.value, r.fc, nz
        );
    }

    // ---- FM fc attribution -----------------------------------------------------------------------------
    let fm_writes = fm.hits(&wp);
    println!("\n=== FM writes ($4000-$4003 / $A04000-$A04003) ===");
    println!(
        "count: {}   fc tally: {:?}",
        fm_writes.len(),
        fm.fc_tally(&wp)
    );
    let show = fm_writes.len().min(32);
    for r in &fm_writes[..show] {
        println!(
            "  frame={:4} addr={:06X} size={:?} value={:02X} fc={}",
            r.frame, r.addr, r.size, r.value, r.fc
        );
    }
    if fm_writes.len() > show {
        println!("  ... ({} more)", fm_writes.len() - show);
    }

    // ---- PSG fc attribution ----------------------------------------------------------------------------
    let psg_writes = psg.hits(&wp);
    println!("\n=== PSG writes ($7F11 / $C00011) ===");
    println!(
        "count: {}   fc tally: {:?}",
        psg_writes.len(),
        psg.fc_tally(&wp)
    );
    let show = psg_writes.len().min(32);
    for r in &psg_writes[..show] {
        println!(
            "  frame={:4} addr={:06X} size={:?} value={:02X} fc={}",
            r.frame, r.addr, r.size, r.value, r.fc
        );
    }
    if psg_writes.len() > show {
        println!("  ... ({} more)", psg_writes.len() - show);
    }

    // ---- Z80 RAM dump ----------------------------------------------------------------------------------
    let z80 = sys.z80_ram();
    println!("\n=== Z80 RAM first 64 bytes at end of run ===");
    for (i, chunk) in z80[..64.min(z80.len())].chunks(16).enumerate() {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02X}")).collect();
        println!("  {:04X}: {}", i * 16, hex.join(" "));
    }
    // The SMPS mailbox lives at Z80 offset $1F00 (68k $A01F00): ping/$1F00, sample/$1F01, music/$1F02,
    // sfx/$1F03. A non-zero music byte at end = the driver NEVER consumed the queued song.
    println!("\n=== SMPS mailbox at end of run (Z80 $1F00-$1F07) ===");
    for (off, &b) in z80[0x1F00..=0x1F07].iter().enumerate() {
        let i = 0x1F00 + off;
        let label = match i {
            0x1F00 => " (SND_REQ_PING)",
            0x1F01 => " (SND_REQ_SAMPLE)",
            0x1F02 => " (SND_REQ_MUSIC)",
            0x1F03 => " (SND_REQ_SFX)",
            _ => "",
        };
        println!("  z80_ram[{i:04X}] = {b:02X}{label}");
    }

    // Scan the whole 8 KiB for any non-zero byte (the queue may sit anywhere).
    let nz: Vec<(usize, u8)> = z80
        .iter()
        .enumerate()
        .filter(|(_, &b)| b != 0)
        .map(|(i, &b)| (i, b))
        .collect();
    println!("\nZ80 RAM non-zero bytes: {} total", nz.len());
    for (i, b) in nz.iter().take(64) {
        println!("  z80_ram[{i:04X}] = {b:02X}");
    }
    if nz.len() > 64 {
        println!("  ... ({} more)", nz.len() - 64);
    }

    // ---- What the hand-rolled sink could not report -----------------------------------------------------
    println!("\n=== instrument report ===");
    println!(
        "seen: {}   matched: {}   recorded: {}   dropped: {}",
        wp.seen(),
        wp.matched(),
        wp.hits().len(),
        wp.dropped()
    );
    for r in wp.watches() {
        let first = r.first.map(|s| s.mclk).unwrap_or_default();
        let last = r.last.map(|s| s.mclk).unwrap_or_default();
        println!(
            "  #{:<2} {:<8} {:?} ${:06X}-${:06X} {:?}  matched={:<6} mclk {}..{}",
            r.id.0,
            r.label,
            r.mode,
            r.range.start(),
            r.range.end(),
            r.op,
            r.matched,
            first,
            last
        );
    }
    for c in wp.caveats() {
        println!("  CAVEAT: {c}");
    }
}
