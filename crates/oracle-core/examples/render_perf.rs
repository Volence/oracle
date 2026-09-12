//! Renderer throughput + picture-parity harness — the measurement half of a render hot-path change.
//!
//! `microop_perf.rs` does this for the 68000 interpreter; this is its counterpart for
//! [`oracle_core::vdp::Vdp::render_line`] (and therefore `resolve_line`, the per-dot compositor every
//! rendering path funnels through). A render optimization has two obligations and this tool serves both
//! from one boot, so the numbers and the picture describe the *same* frames:
//!
//! 1. **The picture must not change.** `--dump <file>` writes the **raw RGB bytes** of every active line of
//!    every frame rendered — not a hash, so a before/after `cmp` names the first differing byte rather than
//!    just saying "different". The per-frame FNV-1a digests are printed too, so a mismatch localizes to a
//!    frame without re-running.
//! 2. **The speedup must be measured.** The timing phase re-renders the settled frame `--reps` times and
//!    reports wall-clock ns/frame and Mpixel/s.
//!
//! A real ROM is the workload on purpose: the synthetic scenes in `tests/golden_frames.rs` pin the *models*
//! but exercise a handful of VDP configurations, while a booting game walks through display-off, window
//! bands, two-cell v-scroll, per-line h-scroll and heavy sprite lines across its frames. The default image is
//! this repo's own frozen `fixtures/aeon/s4.debug.bin`, so the workload is committed bytes.
//!
//! This is a **dev tool, not a gate artifact** — nothing in CI depends on it. The picture *gate* is
//! `tests/golden_frames.rs`; this tool is the wider net you run by hand across a change.
//!
//! Usage: `cargo run --release --example render_perf -- [--rom <path>] [--frames N] [--reps N] [--dump <file>]`
//! (defaults: the frozen `s4.debug.bin`, 120 frames, 200 timing reps, no dump).

use oracle_core::system::System;
// Active display height in lines — the core's own `ACTIVE_LINES`, the same one `tests/golden_frames.rs` and
// `tests/conformance_roms.rs` hash over, so a digest here is directly comparable with theirs in shape.
use oracle_core::vdp::ACTIVE_LINES;
use std::hint::black_box;
use std::io::Write;
use std::process::ExitCode;
use std::time::Instant;

#[path = "common/rom_source.rs"]
mod rom_source;

const FNV1A_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Render every active line of the current settled state, appending the RGB bytes to `out` (when the caller
/// wants them) and folding them into the running FNV-1a digest. One function so the hashed bytes and the
/// dumped bytes can never be a different set.
fn render_frame(sys: &System, out: Option<&mut Vec<u8>>, hash: &mut u64) -> usize {
    let mut sink = out;
    let mut pixels = 0usize;
    for line in 0..ACTIVE_LINES {
        for (r, g, b) in sys.vdp().render_line(line) {
            pixels += 1;
            for byte in [r, g, b] {
                *hash ^= byte as u64;
                *hash = hash.wrapping_mul(FNV1A_PRIME);
                if let Some(v) = sink.as_deref_mut() {
                    v.push(byte);
                }
            }
        }
    }
    pixels
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut rom_path = rom_source::frozen("s4.debug.bin");
    let mut frames = 120u64;
    let mut reps = 200u32;
    let mut dump: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        let next = |i: usize| args.get(i + 1).cloned();
        match args[i].as_str() {
            "--rom" => rom_path = next(i).unwrap_or_else(|| rom_path.clone()),
            "--frames" => frames = next(i).and_then(|s| s.parse().ok()).unwrap_or(frames),
            "--reps" => reps = next(i).and_then(|s| s.parse().ok()).unwrap_or(reps),
            "--dump" => dump = next(i),
            other => {
                eprintln!("unknown argument `{other}`");
                return ExitCode::FAILURE;
            }
        }
        i += 2;
    }

    let rom = match std::fs::read(&rom_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("cannot read ROM {rom_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    rom_source::announce(&rom_path, rom.len());

    let mut sys = System::new(0x1234_5678);
    sys.load_rom(rom);
    sys.reset();

    // Phase 1 — picture. One digest per frame, and (optionally) every rendered byte.
    let mut bytes: Option<Vec<u8>> = dump.as_ref().map(|_| Vec::new());
    let mut digests: Vec<u64> = Vec::with_capacity(frames as usize);
    let mut whole = FNV1A_OFFSET;
    for _ in 0..frames {
        sys.run_frames(1);
        let mut per_frame = FNV1A_OFFSET;
        render_frame(&sys, bytes.as_mut(), &mut per_frame);
        render_frame(&sys, None, &mut whole);
        digests.push(per_frame);
    }
    println!("frames rendered: {frames}");
    for (n, d) in digests.iter().enumerate() {
        println!("  frame {n:>4}: 0x{d:016x}");
    }
    println!("picture digest (all frames): 0x{whole:016x}");
    if let (Some(path), Some(v)) = (dump.as_ref(), bytes.as_ref()) {
        match std::fs::File::create(path).and_then(|mut f| f.write_all(v)) {
            Ok(()) => println!("dumped {} raw RGB bytes to {path}", v.len()),
            Err(e) => {
                eprintln!("cannot write {path}: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    // Phase 2 — throughput. The settled state is re-rendered `reps` times; `black_box` keeps the render
    // from being elided. Same code path as phase 1, so the timing describes the picture just verified.
    let mut sink = FNV1A_OFFSET;
    let mut pixels = 0usize;
    let t0 = Instant::now();
    for _ in 0..reps {
        pixels += render_frame(black_box(&sys), None, &mut sink);
    }
    let dt = t0.elapsed();
    black_box(sink);
    let per_frame = dt.as_secs_f64() / reps as f64;
    println!(
        "render: {reps} frames in {:.3} s  =  {:.3} ms/frame  ({:.2} Mpixel/s)",
        dt.as_secs_f64(),
        per_frame * 1e3,
        pixels as f64 / dt.as_secs_f64() / 1e6
    );
    ExitCode::SUCCESS
}
