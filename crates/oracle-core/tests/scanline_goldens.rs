//! Per-scanline golden coverage — the **live** picture, pinned alongside (never instead of) the post-hoc
//! hashes in `conformance_roms.rs` and `golden_frames.rs`.
//!
//! ## The blind spot this closes
//!
//! The machine renders every scanline during the run ([`oracle_core::vdp::Vdp::render_scanline`], driven from
//! `System::run`) and throws the RGB away unless a sink opts in via `wants_scanlines`. Every frozen golden we
//! had instead called the *post-hoc* [`oracle_core::vdp::Vdp::render_line`], which re-renders a line after the
//! fact against whatever state the VDP happens to hold at the end of the run. Surveying the whole vendored
//! corpus turned up **three** distinct mechanisms by which that re-render shows a picture the machine never
//! put on screen — all three of them invisible to the pre-existing goldens:
//!
//! 1. **Mid-frame state changes** — raster splits, per-line palette cycling, water lines, a window boundary
//!    moved partway down the screen. A post-hoc render sees only the *last* value of every register and CRAM
//!    word, so it draws the whole frame in the final palette. (`color_1536`, `window_distortion`, `io_sample`,
//!    `m68k_opcode_sizes`.)
//! 2. **Per-line carried sprite state.** `render_scanline` seeds the R10 sprite-masking rule from
//!    `sprite_dot_overflow_carry` and commits the new carry for the next line; `render_line` re-seeds from
//!    whatever the carry holds *now* — normally the last line of the frame. `render_line`'s sibling
//!    `report_rgb` already documents this ("re-resolving after `render_scanline` would be wrong as well as
//!    wasteful: the committed dot-overflow carry would reseed the R10 masking and could change the sprites"),
//!    but nothing measured how far apart the two paths actually were. (`vdp_sprite_masking`.)
//! 3. **Vblank updates that postdate the frame.** A ROM that touches the VDP only between the last active line
//!    and the next frame leaves the post-hoc render describing the state *after* the update, while the frame
//!    actually drawn used the state before it. Nothing is mid-frame here and no sprite state is involved — the
//!    re-render is simply of the wrong moment. (`shadow_highlight`, which animates on a measured 4-frame
//!    cycle with 18 vblank-only VDP writes per frame and zero writes during active display.)
//!
//! The frontend hit class 1 for real (Sonic 3 & Knuckles' underwater palette split drew in the above-water
//! palette) and was fixed by blitting a [`ScanlineCapture`]. The goldens were never converted, so a change that
//! broke raster effects could land with every gate green.
//!
//! ## What is pinned here
//!
//! One row per vendored test ROM. Each ROM is booted once, run [`FRAMES`] frames with a `Retain::LastFrame`
//! capture riding along, and the last complete frame is hashed **as the VDP drew it**. That live hash is then
//! compared to a post-hoc hash of the same machine, and the row records which of two things is true:
//!
//! - `LIVE-DIFFERS frame_hash=0x…` — the two paths disagree, and the live picture's hash is pinned. These are
//!   the rows that carry the new coverage.
//! - `IDENTICAL-TO-POST-HOC` — the two paths agree byte for byte, so there is nothing new to pin and **no
//!   hash literal is recorded**. Duplicating the post-hoc hash here would double the amendment burden without
//!   adding coverage; the *equality* is the assertion, and it is still machine-checked, so a ROM that starts
//!   diverging shows up as a diff instead of as nothing.
//!
//! Pinning rule, same as everywhere else in this tree: a literal here changes only as a deliberate, evidenced
//! amendment — never a silent regen.
//!
//! ## Why this cannot pass vacuously
//!
//! - A `LIVE-DIFFERS` row asserts, structurally, that the live hash is **not** the post-hoc hash. If the live
//!   path ever collapses back to post-hoc behaviour the row flips to `IDENTICAL-TO-POST-HOC` and the single
//!   scorecard assert fires — before the pinned hash is even consulted. (Measured: replacing the capture with
//!   the post-hoc render fails all six pinned rows.)
//! - [`live_frame_hash`] asserts the capture handed back exactly one complete frame of active lines and that
//!   the run delivered the frame boundaries it was asked for. A short or torn capture fails loudly rather than
//!   hashing a partial picture — which is a real failure mode, not a hypothetical: an early draft of this
//!   survey attached a capture *after* a mid-frame stop and silently hashed a fragment.
//! - The count guard below requires every ROM present on disk to have produced a row, and
//!   [`vendor_data_present_when_running_in_ci`] requires the whole corpus to be present under `CI`, so a
//!   missing-artifact run fails instead of skipping to a green pass.
//!
//! ## Currency
//!
//! Additive-only. This file pins nothing that `conformance_roms.rs`, `golden_frames.rs`, `determinism_gate` or
//! `export_state_v1` already pin, and touches no `oracle-core/src/` semantics. The question of whether the
//! post-hoc goldens should now be *retired* is deliberately left open for the owner — see the report in
//! `docs/2026-08-15-scanline-golden-coverage.md`.

use oracle_core::scanline_capture::{Retain, ScanlineCapture};
use oracle_core::system::System;
use std::path::Path;

const VENDOR_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/TestRoms");

/// Power-on RNG seed — the same fixed seed `conformance_roms.rs` uses, so a row here and a row there describe
/// the same machine.
const SEED: u64 = 0x1234_5678;

const ACTIVE_LINES: u16 = 224;

/// One settle budget for every ROM. This instrument is **not** a mirror of the conformance scorecard — it asks
/// a single question ("does the live picture differ from the post-hoc one?") and does not need each ROM's
/// hand-tuned verdict budget to ask it. 120 frames is the budget the scorecard's visual rows already settle
/// under, and every divergence recorded below was verified to hold across adjacent budgets or to be explained
/// by a measured cause (see the report).
const FRAMES: u64 = 120;

/// Every ROM `tools/fetch-testroms.sh` vendors, in the same order as `conformance_roms.rs::ROMS`.
const ROMS: &[&str] = &[
    "color_1536",
    "cram_flicker",
    "direct_color_dma",
    "fm_test",
    "gfx_joystick",
    "io_sample",
    "m68k_bcd",
    "m68k_illegal",
    "m68k_memory_test",
    "m68k_opcode_sizes",
    "shadow_highlight",
    "vcounter",
    "vdp_port_access",
    "vdp_sprite_masking",
    "vdp_test_register",
    "window_distortion",
    "window_test",
];

// ---------------------------------------------------------------------------------------------------
// The pinned baseline
// ---------------------------------------------------------------------------------------------------

/// The live per-scanline outcome for every ROM at [`FRAMES`]. **A diff here is the whole point of this test.**
///
/// The `cause:` note on each `LIVE-DIFFERS` row is the *measured* reason the two paths disagree — instrumented
/// by splitting every VDP-port bus write into "during active display" vs "during vblank", and by replaying the
/// stateful render path over a settled machine to isolate the sprite carry. Not a guess: the first draft of
/// this file attributed `shadow_highlight` to the sprite carry, and the replay control disproved it.
const BASELINE: &[(&str, &str)] = &[
    (
        // cause (mechanism 1): 645 VDP-port writes per frame *during active display* — 387 of them register
        // writes (R1/R19) and the rest CRAM — starting at line 48. The 1536-colour trick rewrites CRAM while
        // the frame is being drawn: the live picture holds ~1400 distinct colours, the post-hoc one holds 4.
        // This hash equals the one `conformance_roms.rs` already pins for this ROM — that row was converted to
        // the capture on 2026-08-03 and is the ONE case where the blindness had already been recognised (as
        // Limitation L1, for this ROM alone). Its agreement here is a cross-check that this file's byte layout
        // and run shape match the existing harness's.
        //
        // RE-PINNED 2026-08-19, F-SCANLINE-SUBLINE slice 4 (was 0x917371f07409cb25). MECHANISM, measured on
        // this branch: in the hashed frame (119) this ROM performs **515 value-changing CRAM writes inside
        // the active-display window** — indices 4, 5, 6 and 7, ~129 each — spread over 131 active lines from
        // 48 to 221, and those four indices are among the six the picture samples (0,1,4,5,6,7). Until this
        // slice each row decoded against ONE snapshot of those entries; now each row is decoded in segments
        // and shows them evolving across its own width. That is not an incidental side effect — a row that
        // changes colour part-way across IS the 1536-colour trick, so this hash is the first one this corpus
        // has held that actually contains the effect the ROM exists to demonstrate. The colour count is
        // unchanged in kind (still ~1400 vs the post-hoc 4); the gradient is now correct *within* each row
        // as well as between rows.
        "color_1536",
        "LIVE-DIFFERS frame_hash=0x9ae4acc58d2a382d",
    ),
    ("cram_flicker", "IDENTICAL-TO-POST-HOC"),
    ("direct_color_dma", "IDENTICAL-TO-POST-HOC"),
    ("fm_test", "IDENTICAL-TO-POST-HOC"),
    ("gfx_joystick", "IDENTICAL-TO-POST-HOC"),
    (
        // cause (mechanism 1): 328 VRAM writes per frame during active display — the ROM redraws its
        // nametable while the beam is past the text it is rewriting, so 14 lines (120-126, 168-174) were drawn
        // from the previous frame's tiles and the post-hoc render shows the new ones.
        "io_sample",
        "LIVE-DIFFERS frame_hash=0xe5e133a2b8f9fe93",
    ),
    ("m68k_bcd", "IDENTICAL-TO-POST-HOC"),
    ("m68k_illegal", "IDENTICAL-TO-POST-HOC"),
    ("m68k_memory_test", "IDENTICAL-TO-POST-HOC"),
    (
        // cause (mechanism 1): ~34 VRAM writes per frame during active display — the ROM plots its live decode
        // map cell by cell, so the 6 lines it reached after the beam had passed them (58-63) are a frame behind
        // in the picture that was actually drawn.
        "m68k_opcode_sizes",
        "LIVE-DIFFERS frame_hash=0xfb9783a5ab564eb4",
    ),
    (
        // cause (mechanism 3): **zero** VDP writes during active display and 18 per frame during vblank. All
        // 224 lines differ, and the picture cycles with a measured period of 4 frames. NOT the sprite carry:
        // replaying the stateful render path over the settled machine reproduces the POST-HOC hash, not the
        // live one, and the carry reads `false` after every frame. The post-hoc render is simply of the wrong
        // moment — one vblank update later than the frame it claims to describe.
        "shadow_highlight",
        "LIVE-DIFFERS frame_hash=0xfd6f02e7574d67f5",
    ),
    ("vcounter", "IDENTICAL-TO-POST-HOC"),
    ("vdp_port_access", "IDENTICAL-TO-POST-HOC"),
    (
        // cause (mechanism 2): **zero** VDP writes in either phase — this ROM makes none at all after frame 7 —
        // and yet 8 lines differ (88-95, x >= 216). Proven to be the stale `sprite_dot_overflow_carry`: driving
        // the STATEFUL path over the settled machine in line order reproduces the live hash EXACTLY, while the
        // pure path does not. The divergence is squarely on this ROM's own subject matter, and the harness's
        // glyph classifier reads a different verdict for test 6 through the two paths (post-hoc FAIL, live
        // PASS). `conformance_roms.rs`'s pinned row is deliberately NOT touched — see F-POSTHOC-STALE-CARRY.
        "vdp_sprite_masking",
        "LIVE-DIFFERS frame_hash=0xce1c5a0559088d5d",
    ),
    ("vdp_test_register", "IDENTICAL-TO-POST-HOC"),
    (
        // cause (mechanism 1): exactly ONE VDP-port write during active display per frame — R17 (window H
        // position) set to $00 at line 111, restored to $07 in vblank — and the 112 lines below it differ. A
        // one-write raster split, and the tightest demonstration in the corpus that a post-hoc render cannot
        // see a mid-frame register change: one write, 112 wrong lines, and no other row of any golden moves.
        "window_distortion",
        "LIVE-DIFFERS frame_hash=0xdf5bae342cc03667",
    ),
    ("window_test", "IDENTICAL-TO-POST-HOC"),
];

// ---------------------------------------------------------------------------------------------------
// Harness plumbing
// ---------------------------------------------------------------------------------------------------

fn rom_path(name: &str) -> String {
    format!("{VENDOR_DIR}/{name}.bin")
}

fn boot(name: &str) -> Option<System> {
    let path = rom_path(name);
    let Ok(rom) = std::fs::read(&path) else {
        eprintln!("SKIP: {path} not found — run tools/fetch-testroms.sh");
        return None;
    };
    let mut sys = System::new(SEED);
    sys.load_rom(rom);
    sys.reset();
    Some(sys)
}

/// FNV-1a-64 over RGB bytes in the caller's pixel order — byte-for-byte the layout `conformance_roms.rs` and
/// `golden_frames.rs` use, so a hash here and a hash there name the same kind of thing and the two can be
/// compared directly.
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

/// Run `frames` frames with a per-scanline capture attached and hash the last complete frame **as the VDP drew
/// it**. The guards are the anti-vacuity spine of this file: a capture that hands back anything other than one
/// full frame of active lines, or a run that did not complete the frames it was asked for, fails here rather
/// than quietly hashing a fragment.
fn live_frame_hash(sys: &mut System, frames: u64) -> u64 {
    let mut cap = ScanlineCapture::new(Retain::LastFrame);
    sys.run_frames_with_sink(frames, &mut cap);
    let width = sys.vdp().render_line(0).len();
    assert_eq!(
        cap.frames_completed(),
        frames,
        "the capture must see every frame boundary of the run"
    );
    assert_eq!(
        cap.pixels().len(),
        width * ACTIVE_LINES as usize,
        "the capture must hold exactly one complete frame of active lines ({width}x{ACTIVE_LINES}), not a \
         partial or torn one"
    );
    fnv1a_rgb(FNV1A_OFFSET, cap.pixels())
}

/// Hash the same frame the way every pre-existing golden does: by re-rendering each line **after the run**
/// from whatever state the VDP now holds. Kept here only as the control that gives `LIVE-DIFFERS` its meaning.
fn post_hoc_frame_hash(sys: &System) -> u64 {
    let mut h = FNV1A_OFFSET;
    for line in 0..ACTIVE_LINES {
        h = fnv1a_rgb(h, &sys.vdp().render_line(line));
    }
    h
}

/// Boot, run, and classify one ROM: does the live picture differ from the post-hoc one, and if so what is it?
fn measure(name: &str) -> Option<String> {
    let mut sys = boot(name)?;
    let live = live_frame_hash(&mut sys, FRAMES);
    let post_hoc = post_hoc_frame_hash(&sys);
    Some(if live == post_hoc {
        "IDENTICAL-TO-POST-HOC".to_string()
    } else {
        format!("LIVE-DIFFERS frame_hash=0x{live:016x}")
    })
}

// ---------------------------------------------------------------------------------------------------
// The scorecard
// ---------------------------------------------------------------------------------------------------

/// The instrument. Builds the whole scorecard and compares it to [`BASELINE`] in ONE assert, so a diff lists
/// every ROM whose live-vs-post-hoc relationship moved.
#[test]
fn scanline_golden_scorecard() {
    let mut scorecard: Vec<(&str, String)> = Vec::new();
    for name in ROMS {
        if let Some(row) = measure(name) {
            scorecard.push((name, row));
        }
    }

    let ran = scorecard.len();
    let present = ROMS
        .iter()
        .filter(|n| Path::new(&rom_path(n)).exists())
        .count();
    eprintln!("scanline goldens: {ran}/{} ROMs ran", ROMS.len());
    for (name, row) in &scorecard {
        eprintln!("  {name:<20} {row}");
    }

    // Count guard: every vendored ROM on disk MUST have produced a row, so a ROM that boots into a panic-free
    // nothing cannot hide behind the soft-skip path.
    assert_eq!(
        ran, present,
        "{present} vendored ROMs are on disk but only {ran} produced a scorecard row"
    );

    if ran == 0 {
        eprintln!("SKIP: no vendored test ROMs at all — run tools/fetch-testroms.sh");
        return;
    }

    let expected: Vec<(&str, String)> = BASELINE
        .iter()
        .filter(|(n, _)| scorecard.iter().any(|(s, _)| s == n))
        .map(|(n, o)| (*n, o.to_string()))
        .collect();

    assert_eq!(
        scorecard, expected,
        "the live per-scanline picture moved. A `LIVE-DIFFERS` row that became `IDENTICAL-TO-POST-HOC` means \
         the live path stopped seeing mid-frame state or per-line sprite carry — that is a REGRESSION of the \
         very coverage this file exists for, not a hash to re-pin. A changed hash is a changed picture: \
         confirm it is intended, then amend BASELINE and docs/2026-08-15-scanline-golden-coverage.md together."
    );
}

/// Structural guard: the pinned baseline must cover exactly the ROM list. Without this, dropping a ROM would
/// shrink the scorecard AND the filtered baseline in lockstep and still pass.
#[test]
fn baseline_covers_every_rom() {
    assert_eq!(
        BASELINE.len(),
        ROMS.len(),
        "BASELINE has {} entries for {} ROMs",
        BASELINE.len(),
        ROMS.len()
    );
    for name in ROMS {
        assert!(
            BASELINE.iter().any(|(n, _)| n == name),
            "ROM {name} has no pinned baseline entry"
        );
    }
    let mut sorted = ROMS.to_vec();
    sorted.sort_unstable();
    assert_eq!(
        sorted, ROMS,
        "keep ROMS sorted so the scorecard diff is stable"
    );
}

/// The harness actually depends on the pixels: flipping a single bit of one captured pixel changes the live
/// hash. Guards against a degenerate hash (a constant, an empty capture, a length-only digest) that would make
/// every pinned value above vacuous — the analogue of `golden_frames.rs::golden_frame_hash_discriminates`.
///
/// Uses `window_distortion` because its divergence is the tightest in the corpus (one mid-frame R17 write,
/// 112 affected lines) and the perturbed pixel sits on line 111, right at its raster split.
#[test]
fn the_live_hash_depends_on_the_pixels() {
    let Some(mut sys) = boot("window_distortion") else {
        eprintln!("SKIP: window_distortion not vendored");
        return;
    };
    let mut cap = ScanlineCapture::new(Retain::LastFrame);
    sys.run_frames_with_sink(FRAMES, &mut cap);
    let width = sys.vdp().render_line(0).len();
    let base = fnv1a_rgb(FNV1A_OFFSET, cap.pixels());

    let mut perturbed = cap.pixels().to_vec();
    perturbed[111 * width + 200].0 ^= 1;
    assert_ne!(
        base,
        fnv1a_rgb(FNV1A_OFFSET, &perturbed),
        "the live frame hash must depend on the captured pixels"
    );
    assert_eq!(
        base,
        live_frame_hash(&mut boot("window_distortion").unwrap(), FRAMES),
        "and it must be reproducible from a fresh boot"
    );
}

/// At least one ROM must carry live coverage. A baseline that drifted to all-`IDENTICAL-TO-POST-HOC` would
/// still pass the scorecard assert above while pinning nothing at all — this file would then be pure cost.
#[test]
fn the_baseline_actually_pins_live_coverage() {
    let pinned = BASELINE
        .iter()
        .filter(|(_, o)| o.starts_with("LIVE-DIFFERS"))
        .count();
    assert!(
        pinned >= 6,
        "only {pinned} ROMs carry a pinned live hash; 6 were measured to diverge on 2026-08-15, and losing \
         one silently would mean losing the coverage this file was added for"
    );
}

/// CI guard: the vendored test ROMs MUST be present under CI, so a fetch failure fails LOUDLY instead of every
/// ROM skipping cleanly and the scorecard passing VACUOUSLY. Locally (no `CI` env var) this is a no-op.
#[test]
fn vendor_data_present_when_running_in_ci() {
    if std::env::var_os("CI").is_none() {
        return;
    }
    assert!(
        Path::new(VENDOR_DIR).exists(),
        "CI: vendored test-ROM dir {VENDOR_DIR} is missing — tools/fetch-testroms.sh must run before the \
         test job (a missing dir makes the whole scorecard skip and pass vacuously)"
    );
    for name in ROMS {
        let p = rom_path(name);
        assert!(
            Path::new(&p).exists(),
            "CI: vendored test ROM {p} is missing — tools/fetch-testroms.sh did not fetch the full corpus"
        );
    }
}
