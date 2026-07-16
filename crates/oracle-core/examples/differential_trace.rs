//! Instruction-boundary architectural-state tracer for the BlastEm-over-the-bus nightly differential.
//!
//! Boots a ROM on the real 68000, single-steps `N` instructions, and prints one JSON object per
//! instruction boundary (the state *after* that instruction) to stdout — the oracle-next side of the
//! differential. The Python driver runs the same ROM in BlastEm over the RSP stub and compares.
//!
//! Only architectural state is emitted (d0–d7, a0–a7 with a7 = the active stack pointer, pc, sr). Timing
//! is deliberately absent — SST-model cycles vs BlastEm cycles legitimately diverge and are xfail-manifest
//! entries, not state divergences (integration-pivot D8).
//!
//! Usage: `differential_trace <rom.bin> <n_steps>`

use oracle_core::system::System;
use std::io::{self, Write};

fn main() {
    let mut args = std::env::args().skip(1);
    let rom_path = args
        .next()
        .expect("usage: differential_trace <rom.bin> <n_steps>");
    let n_steps: usize = args
        .next()
        .expect("usage: differential_trace <rom.bin> <n_steps>")
        .parse()
        .expect("n_steps must be a non-negative integer");

    let rom = std::fs::read(&rom_path).expect("could not read ROM file");
    // Seed 0: a fixed, documented power-on RAM pattern. The differential ROMs are register-only, so the
    // initial RAM contents never influence the trace — but pin the seed for reproducibility.
    let mut sys = System::new(0);
    sys.load_rom(rom);
    sys.reset();

    let out = io::stdout();
    let mut w = io::BufWriter::new(out.lock());
    // Emit the post-reset state as step 0 so the two sides agree on the starting point (PC at the ROM's
    // reset vector, SR = 0x2700).
    emit(&mut w, 0, &sys);
    for i in 1..=n_steps {
        sys.step_instruction();
        emit(&mut w, i, &sys);
    }
    w.flush().unwrap();
}

/// Print one instruction-boundary state as a JSON object (one line).
fn emit<W: Write>(w: &mut W, step: usize, sys: &System) {
    let r = sys.cpu_regs();
    // a7 is the active stack pointer (selected by S) — the value BlastEm's g-packet reports for a7.
    let a7 = if r.sr & 0x2000 != 0 { r.ssp } else { r.usp };
    write!(w, "{{\"step\":{step},\"pc\":{},\"sr\":{}", r.pc, r.sr).unwrap();
    for (i, d) in r.d.iter().enumerate() {
        write!(w, ",\"d{i}\":{d}").unwrap();
    }
    for (i, a) in r.a.iter().enumerate() {
        write!(w, ",\"a{i}\":{a}").unwrap();
    }
    writeln!(w, ",\"a7\":{a7}}}").unwrap();
}
