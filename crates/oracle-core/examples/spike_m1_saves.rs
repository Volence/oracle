//! spike: M1-FILL-OVER-TIME save-compatibility probe. Removed from the tip in the final spike commit.
//!
//! Built once on the baseline tree and once on the prototype, then the two binaries trade snapshots:
//!
//! * `fp` prints the save-state layout fingerprint exactly as `oracle_frontend::save_state` derives it
//!   (FNV-1a of `System::new(PROBE_SEED).snapshot()`).
//! * `save <rom> <mclk> <out> <frames>` boots, runs to `mclk`, writes the snapshot, then runs `frames`
//!   more and prints the `export_state_hash` it reaches.
//! * `load <snap> <frames>` restores (printing the refusal if there is one), runs `frames`, prints the hash.

use oracle_core::system::System;

const PROBE_SEED: u64 = 0x5A5E_57A7_E0B0_0B1E;

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

fn report(tag: &str, sys: &System) {
    let now = sys.scheduler().now();
    println!(
        "{tag}: mclk={now} dma_busy={} export_state_hash={:016x} vram_fnv={:016x}",
        sys.vdp().dma_busy(now),
        sys.export_state_hash(),
        fnv1a(sys.vdp().vram())
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("fp") => {
            let snap = System::new(PROBE_SEED).snapshot();
            println!("layout_fingerprint={:016x} probe_len={}", fnv1a(&snap), snap.len());
        }
        Some("save") => {
            let rom = std::fs::read(&args[1]).expect("rom");
            let mclk: u64 = args[2].parse().expect("mclk");
            let frames: u64 = args[4].parse().expect("frames");
            let mut sys = System::new(0x5EED);
            sys.load_rom(rom);
            sys.reset();
            sys.run_until(mclk);
            report("at-save", &sys);
            std::fs::write(&args[3], sys.snapshot()).expect("write");
            sys.run_frames(frames);
            report("after", &sys);
        }
        Some("load") => {
            let bytes = std::fs::read(&args[1]).expect("snap");
            let frames: u64 = args[2].parse().expect("frames");
            match System::restore(&bytes) {
                Err(e) => println!("restore REFUSED: {e}"),
                Ok(mut sys) => {
                    report("at-load", &sys);
                    sys.run_frames(frames);
                    report("after", &sys);
                }
            }
        }
        _ => eprintln!("usage: spike_m1_saves fp | save <rom> <mclk> <out> <frames> | load <snap> <frames>"),
    }
}
