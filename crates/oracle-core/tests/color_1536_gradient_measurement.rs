//! **Measurement artifact, not a gate.** This file exists to answer one question that a frame hash
//! structurally cannot: after the C2 VDP-stall fix (`02d6282`) moved `color_1536`'s pinned
//! `frame_hash` (re-pinned in `3e863a6`), is the captured picture still the ~1400-colour gradient the
//! ROM exists to demonstrate, or has it collapsed toward the 4 colours the *post-hoc* framebuffer
//! shows?
//!
//! It asserts nothing about the picture's content — it *reports*. It reuses the exact capture path
//! the pinned harnesses use (`ScanlineCapture` in `Retain::LastFrame` over 120 frames, the same
//! FNV-1a byte layout), so the numbers here describe the same pixels the pinned hash covers, and it
//! prints that hash so the correspondence can be checked rather than assumed.
//!
//! It writes a P6 PPM of the captured frame so a human can look at it. Output directory comes from
//! `C1536_OUT` (default: the system temp dir); nothing is written into the repo.
//!
//! Run with: `cargo test -p oracle-core --test color_1536_gradient_measurement -- --nocapture`

use oracle_core::scanline_capture::{Retain, ScanlineCapture};
use oracle_core::system::System;
use std::collections::HashSet;
use std::io::Write;

const VENDOR_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/TestRoms");
const SEED: u64 = 0x1234_5678;
const ACTIVE_LINES: u16 = 224;
const FRAMES: u64 = 120;

/// Byte-for-byte the layout `conformance_roms.rs::fnv1a_rgb` uses.
fn fnv1a_rgb(mut h: u64, px: &[(u8, u8, u8)]) -> u64 {
    for &(r, g, b) in px {
        for byte in [r, g, b] {
            h ^= byte as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

const FNV1A_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

#[test]
fn measure_color_1536_gradient() {
    let path = format!("{VENDOR_DIR}/color_1536.bin");
    let Ok(rom) = std::fs::read(&path) else {
        panic!("MEASUREMENT BLOCKED: {path} not present — run tools/fetch-testroms.sh");
    };
    let mut sys = System::new(SEED);
    sys.load_rom(rom);
    sys.reset();

    // The pinned capture path, verbatim.
    let mut cap = ScanlineCapture::new(Retain::LastFrame);
    sys.run_frames_with_sink(FRAMES, &mut cap);
    let width = sys.vdp().render_line(0).len();
    assert_eq!(
        cap.pixels().len(),
        width * ACTIVE_LINES as usize,
        "capture must hold exactly one complete frame of active lines"
    );
    let px = cap.pixels();

    let live_hash = fnv1a_rgb(FNV1A_OFFSET, px);

    // The post-hoc picture, for the 4-colour contrast the ROM's comment names.
    let mut posthoc: Vec<(u8, u8, u8)> = Vec::with_capacity(px.len());
    for line in 0..ACTIVE_LINES {
        posthoc.extend_from_slice(&sys.vdp().render_line(line));
    }
    let posthoc_hash = fnv1a_rgb(FNV1A_OFFSET, &posthoc);

    let distinct = |s: &[(u8, u8, u8)]| s.iter().copied().collect::<HashSet<_>>().len();

    println!("== color_1536 gradient measurement ==");
    println!("width={width} lines={ACTIVE_LINES} frames={FRAMES}");
    println!("LIVE frame_hash    = 0x{live_hash:016x}");
    println!("POSTHOC frame_hash = 0x{posthoc_hash:016x}");
    println!("DISTINCT_LIVE      = {}", distinct(px));
    println!("DISTINCT_POSTHOC   = {}", distinct(&posthoc));

    // Per-scanline distinct counts: every line, plus a named sample so a collapse confined to some
    // rows cannot hide inside a healthy total.
    let mut per_line: Vec<usize> = Vec::with_capacity(ACTIVE_LINES as usize);
    for y in 0..ACTIVE_LINES as usize {
        per_line.push(distinct(&px[y * width..(y + 1) * width]));
    }
    println!("PERLINE_ALL={per_line:?}");
    for y in [
        0usize, 32, 47, 48, 64, 96, 112, 128, 160, 192, 210, 221, 223,
    ] {
        println!("PERLINE y={y:3} distinct={}", per_line[y]);
    }
    let nonflat = per_line.iter().filter(|&&c| c > 1).count();
    println!(
        "PERLINE summary: min={} max={} lines_with_more_than_one_colour={nonflat}",
        per_line.iter().min().unwrap(),
        per_line.iter().max().unwrap()
    );

    // Write the picture so a human can glance at it, and so two revisions can be diffed pixelwise.
    let out_dir =
        std::env::var("C1536_OUT").unwrap_or_else(|_| std::env::temp_dir().display().to_string());
    let tag = std::env::var("C1536_TAG").unwrap_or_else(|_| "unknown".to_string());
    let ppm = format!("{out_dir}/color_1536_{tag}.ppm");
    let mut f = std::fs::File::create(&ppm).expect("cannot create PPM");
    write!(f, "P6\n{width} {ACTIVE_LINES}\n255\n").unwrap();
    let mut raw = Vec::with_capacity(px.len() * 3);
    for &(r, g, b) in px {
        raw.extend_from_slice(&[r, g, b]);
    }
    f.write_all(&raw).unwrap();
    f.flush().unwrap();
    println!("PPM_PATH={ppm}");
}
