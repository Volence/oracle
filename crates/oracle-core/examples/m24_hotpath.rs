//! **SPIKE (M24 hot-path A/B), NOT PROPOSED FOR MERGE.** Removed from the tip in the parcel's final `spike:`
//! commit.
//!
//! Times `run_frames` over the frozen aeon fixture (a released Z80 running its sound driver, so
//! `catch_up_z80`'s gated-on loop is hot), and prints an `export_state` digest so two builds can be compared
//! for identity as well as speed. Usage: `m24_hotpath [frames] [reps] [armed]`.
//!
//! The baseline binary (X) was built from this file minus the `armed` mode, against the core at `5ee7d6a`;
//! the timed null-sink loop (`System::new(0x51)`, `load_rom`, `reset`, `run_frames`) is the same text in
//! both. `armed` exercises the proposed hook with a counting sink (the positive control).

use oracle_core::bus::{BusEvent, BusEventSink, Z80Access};
use oracle_core::system::System;
use std::time::Instant;

struct CountZ80(u64);

impl BusEventSink for CountZ80 {
    fn on_event(&mut self, _event: BusEvent) {}
    fn wants_z80_accesses(&self) -> bool {
        true
    }
    fn on_z80_access(&mut self, _access: Z80Access) {
        self.0 += 1;
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let frames: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(600);
    let reps: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);
    let armed = args.next().as_deref() == Some("armed");
    let rom = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/aeon/s4.debug.bin"
    ))
    .expect("UNMEASURABLE: fixtures/aeon/s4.debug.bin unreadable");
    let mut per = Vec::with_capacity(reps);
    let mut digest = 0u64;
    let mut z80_accesses = 0u64;
    for _ in 0..reps {
        let mut s = System::new(0x51);
        s.load_rom(rom.clone());
        s.reset();
        let t = Instant::now();
        if armed {
            let mut c = CountZ80(0);
            s.run_frames_with_sink(frames, &mut c);
            z80_accesses = c.0;
        } else {
            s.run_frames(frames);
        }
        per.push(t.elapsed().as_nanos() as f64 / frames as f64);
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in &s.export_state() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        assert!(
            digest == 0 || digest == h,
            "UNMEASURABLE: two reps of one build disagree"
        );
        digest = h;
    }
    per.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    println!(
        "frames={frames} reps={reps} armed={armed} ns_per_frame min={:.0} median={:.0} max={:.0} \
         export_fnv={digest:016x} z80_accesses={z80_accesses}",
        per[0],
        per[reps / 2],
        per[reps - 1]
    );
}
