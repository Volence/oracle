//! **`color_1536`'s gradient guard** — the property the pinned `frame_hash` structurally cannot check.
//!
//! ## Why this exists
//!
//! `color_1536` is the 1536-colour ROM: it rewrites CRAM *mid-scanline*, so the picture it demonstrates
//! exists only while the frame is being drawn. It prints no verdict, so the corpus pins it as a frozen
//! `frame_hash` in two harnesses (`conformance_roms.rs`, `scanline_goldens.rs`). A frame hash is
//! *self-consistent by construction*: whatever the model draws, the hash describes it, and a re-pin makes
//! the new picture the new truth. When the C2 VDP-stall fix (`02d6282`) moved this row and `3e863a6`
//! re-pinned it, nothing in the tree could distinguish "the gradient shifted by a couple of pixels" from
//! "the gradient collapsed and we enshrined the wreckage". Earlier re-pins of this row were checked by eye
//! as PPM dumps; that check left no artifact and does not run again.
//!
//! This file is that check, made mechanical. It asserts the picture's **character** — how many distinct
//! colours the live capture holds, and that the live capture is drastically richer than the post-hoc
//! re-render — which is exactly the axis a hash is blind along. It deliberately does **not** pin an exact
//! colour count: that would duplicate the hash exactly and break on every legitimate timing shift.
//!
//! ## What it asserts
//!
//! A shape check up front, then three content properties evaluated together and reported in one assert:
//!
//! 0. The capture handed back exactly one complete frame of active lines (a torn capture must fail loudly,
//!    not be counted).
//! 1. `FLOOR` — the live picture holds at least [`GRADIENT_COLOUR_FLOOR`] distinct colours; see that
//!    constant for the derivation from measured numbers.
//! 2. `SAME-PICTURE` — the live picture and the post-hoc re-render are **not** the same picture. If they
//!    ever become equal, someone has pointed the capture at the post-hoc path and the per-scanline
//!    apparatus is a no-op.
//! 3. `RATIO` — the live picture is **drastically richer** than the post-hoc one, at least
//!    [`LIVE_OVER_POSTHOC_RATIO`]x as many distinct colours.
//!
//! 1-3 are collected rather than asserted in a line, because they are not independent in failure — measured,
//! not assumed: *both* red-first mutations below break all three at once, and with sequential `assert!`s the
//! first one fires and hides the other two. A failure here names every property that broke.
//!
//! ## Measured, both sides of the C2 fix
//!
//! | | `8ca3056` (pre-fix) | `e6c432b` (post-fix) |
//! |---|---|---|
//! | live `frame_hash` | `0x9ae4acc58d2a382d` | `0x87ecf46f3cd54fda` |
//! | distinct colours, live capture | 1407 | 1407 |
//! | distinct colours, post-hoc | 4 | 4 |
//! | pixels differing between the two revisions | — | 24 of 71680 (0.034%), in columns 64-65 only |
//!
//! The fix relocated colour boundaries by one or two pixels at the first mid-line CRAM boundary and changed
//! the colour count not at all. That is what a timing shift looks like here, and it is the headroom these
//! thresholds are sized against.
//!
//! ## Red-first
//!
//! Proven able to fail, with two mutations that vary a different parameter — *when* the picture is captured
//! versus *which path* it comes from. Each was applied on disk against this file as committed and reverted
//! with `git checkout --` afterwards; both runs exited 101.
//!
//! - **Mutation A, the captured moment** (`FRAMES` 120 -> 1: capture frame 0, before the ROM has begun its
//!   CRAM rewrites). A genuine collapse, not an edit to a threshold — the picture really does hold almost
//!   nothing. Observed **1** distinct colour, both hashes `0x815bb645bc46a325`, and the frame is still H32
//!   (`width=256`) because the ROM has not switched to H40 yet.
//! - **Mutation B, the source path** (feed the post-hoc re-render in where the live capture's pixels go).
//!   This is the exact blindness the per-scanline capture was built to close. Observed the two pictures
//!   byte-identical at `0x96b9c93c4f3dd325` with **4** colours.
//!
//! **Both mutations fired all three properties, 3 of 3.** That was not the expectation going in — the first
//! draft of this file predicted A would trip only `FLOOR` — and it is a fact about the machine rather than a
//! gap in the mutations: any picture flat enough to miss the floor is also, here, equal to the post-hoc
//! re-render. It is also precisely why the properties are collected. Under the first draft's sequential
//! `assert!`s the floor fired first and `SAME-PICTURE`/`RATIO` were never witnessed failing at all, which
//! would have shipped two assertions nobody had ever seen go red.
//!
//! ## Not a hash, not a replacement for one
//!
//! Additive. This pins no literal that `conformance_roms.rs` or `scanline_goldens.rs` already pins, and it
//! touches nothing under `crates/*/src/`. It also still *reports*: the hashes, the colour counts, the
//! per-line distinct profile, and a P6 PPM of the captured frame (directory from `C1536_OUT`, default the
//! system temp dir; nothing is written into the repo) so a human can still look at the picture.
//!
//! Run with `-- --nocapture` to see the numbers; the assertions run either way.

use oracle_core::scanline_capture::{Retain, ScanlineCapture};
use oracle_core::system::System;
use oracle_core::vdp::ACTIVE_LINES;
use std::collections::HashSet;
use std::io::Write;

const VENDOR_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/TestRoms");
const SEED: u64 = 0x1234_5678;
const FRAMES: u64 = 120;

/// The minimum number of distinct colours the live capture must hold for the picture to still *be* the
/// 1536-colour gradient. **Derived, not chosen:**
///
/// - Measured on both sides of the C2 fix: **1407** distinct colours live, **4** post-hoc. Those two numbers
///   are the whole spread this threshold has to separate.
/// - 256 sits **64x above** the post-hoc 4, so a collapse toward the flat end cannot squeak past it — it
///   would have to miss by nearly two orders of magnitude.
/// - 256 is **18% of** the measured 1407, so the picture may shed 82% of its colours before this trips. The
///   real timing change this file was written for moved 24 pixels of 71680 and **zero** colours, so the
///   headroom against legitimate motion is enormous.
/// - Corroboration from the hardware rather than from our own numbers: Genesis CRAM holds 64 entries, so any
///   single palette snapshot bounds a frame at 64 distinct colours (192 with shadow/highlight). 256 is above
///   every such bound, which makes clearing this floor positive evidence that CRAM was rewritten *during*
///   the frame — which is the trick the ROM exists to demonstrate.
const GRADIENT_COLOUR_FLOOR: usize = 256;

/// How many times richer the live capture must be than the post-hoc re-render. Measured ratio is 1407/4 =
/// ~351x, so 8x leaves ~44x of headroom while still firing instantly if the capture is ever pointed at the
/// post-hoc path (where the ratio is exactly 1).
const LIVE_OVER_POSTHOC_RATIO: usize = 8;

/// Byte-for-byte the layout `conformance_roms.rs::fnv1a_rgb` uses, so a hash printed here is directly
/// comparable to the pinned literals there and in `scanline_goldens.rs`.
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

fn distinct(px: &[(u8, u8, u8)]) -> usize {
    px.iter().copied().collect::<HashSet<_>>().len()
}

#[test]
fn color_1536_keeps_its_gradient() {
    let path = format!("{VENDOR_DIR}/color_1536.bin");
    let Ok(rom) = std::fs::read(&path) else {
        panic!("GUARD BLOCKED: {path} not present — run tools/fetch-testroms.sh");
    };
    let mut sys = System::new(SEED);
    sys.load_rom(rom);
    sys.reset();

    // The capture path the pinned harnesses use, verbatim.
    let mut cap = ScanlineCapture::new(Retain::LastFrame);
    sys.run_frames_with_sink(FRAMES, &mut cap);
    let width = sys.vdp().render_line(0).len();
    assert_eq!(
        cap.pixels().len(),
        width * ACTIVE_LINES as usize,
        "capture must hold exactly one complete frame of active lines"
    );
    let live: Vec<(u8, u8, u8)> = cap.pixels().to_vec();

    // The post-hoc picture: the same machine re-rendered after the fact, which sees only the final CRAM.
    let mut posthoc: Vec<(u8, u8, u8)> = Vec::with_capacity(live.len());
    for line in 0..ACTIVE_LINES {
        posthoc.extend_from_slice(&sys.vdp().render_line(line));
    }

    let live_hash = fnv1a_rgb(FNV1A_OFFSET, &live);
    let posthoc_hash = fnv1a_rgb(FNV1A_OFFSET, &posthoc);
    let live_colours = distinct(&live);
    let posthoc_colours = distinct(&posthoc);

    // ---- report (visible with --nocapture; the assertions below run regardless) ----
    println!("== color_1536 gradient guard ==");
    println!("width={width} lines={ACTIVE_LINES} frames={FRAMES}");
    println!("LIVE frame_hash    = 0x{live_hash:016x}");
    println!("POSTHOC frame_hash = 0x{posthoc_hash:016x}");
    println!("DISTINCT_LIVE      = {live_colours}");
    println!("DISTINCT_POSTHOC   = {posthoc_colours}");

    let mut per_line: Vec<usize> = Vec::with_capacity(ACTIVE_LINES as usize);
    for y in 0..ACTIVE_LINES as usize {
        per_line.push(distinct(&live[y * width..(y + 1) * width]));
    }
    println!("PERLINE_ALL={per_line:?}");
    for y in [
        0usize, 32, 47, 48, 64, 96, 112, 128, 160, 192, 210, 221, 223,
    ] {
        println!("PERLINE y={y:3} distinct={}", per_line[y]);
    }
    println!(
        "PERLINE summary: min={} max={}",
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
    let mut raw = Vec::with_capacity(live.len() * 3);
    for &(r, g, b) in &live {
        raw.extend_from_slice(&[r, g, b]);
    }
    f.write_all(&raw).unwrap();
    f.flush().unwrap();
    println!("PPM_PATH={ppm}");

    // ---- the guard ----
    //
    // Every property is EVALUATED, then one assert reports all violations at once (the
    // `conformance_roms.rs` idiom). Sequential asserts would let the first failure shadow the rest: a
    // mutation that points the capture at the post-hoc path collapses the colour count *and* makes the two
    // pictures equal, and with `assert!` in a line the floor fires and the reader never learns that the
    // live-vs-post-hoc checks would have caught it too. Here a failure names every property that broke,
    // which is also what a future maintainer needs in order to tell a shifted gradient from a lost one.
    let mut violations: Vec<String> = Vec::new();

    // (a) The gradient itself. A hash cannot see this: it moves the same way for a two-pixel boundary shift
    // and for a total collapse, and a re-pin makes either one the new truth.
    if live_colours < GRADIENT_COLOUR_FLOOR {
        violations.push(format!(
            "FLOOR: the live capture holds only {live_colours} distinct colours, below the floor of \
             {GRADIENT_COLOUR_FLOOR}. This ROM exists to draw a ~1400-colour gradient by rewriting CRAM \
             mid-scanline; a count this low means the gradient has collapsed toward the flat post-hoc \
             picture ({posthoc_colours} colours here)."
        ));
    }

    // (b) and (c) The reason this ROM is captured per scanline at all. If the two paths ever agree, the
    // capture is no longer capturing anything and the per-scanline coverage downstream is vacuous.
    if live_hash == posthoc_hash {
        violations.push(format!(
            "SAME-PICTURE: the live per-scanline capture and the post-hoc re-render produced the SAME \
             picture (0x{live_hash:016x}). color_1536's whole point is that they differ — the post-hoc \
             path sees only the end-of-frame palette. Equality means the capture is reading the post-hoc \
             path."
        ));
    }
    if live_colours < posthoc_colours.saturating_mul(LIVE_OVER_POSTHOC_RATIO) {
        violations.push(format!(
            "RATIO: the live capture ({live_colours} colours) is not drastically richer than the post-hoc \
             re-render ({posthoc_colours} colours) — expected at least {LIVE_OVER_POSTHOC_RATIO}x. \
             Measured on the C2 branches the ratio was ~351x (1407 vs 4)."
        ));
    }

    assert!(
        violations.is_empty(),
        "color_1536 no longer holds the gradient it exists to demonstrate \
         ({} of 3 properties failed):\n  - {}\n\nIf a pinned frame_hash moved at the same time, DO NOT \
         re-pin it — the picture is wrong, not merely different. PPM of what was actually drawn: {ppm}",
        violations.len(),
        violations.join("\n  - ")
    );
}
