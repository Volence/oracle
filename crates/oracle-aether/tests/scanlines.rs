//! `emulator/scanlines` — `protocol.md` §6 (VRAM / CRAM / layers), adopted as CR-24
//! (`docs/2026-08-18-cr24-scanlines.md`, ruled in `docs/2026-08-18-ruling-cr24.md`, §11.14).
//!
//! Every test is a wire round trip, so each reply is validated against the vendored contract schema on the
//! way past — and for this row that validation is load-bearing rather than decorative: the fragment ties
//! `mode` to `rows[].width` to the *exact* `rgb` length with an `if`/`then`, in both directions, so a reply
//! whose hex string is one pixel short is rejected before any assertion here runs.
//!
//! The file is shaped by the ruling's R1, which replaced an unexecutable adoption clause with two suite
//! gates. Both are here, and they are the reason the rest of the file is allowed to be ordinary:
//!
//! * **Gate (i), the two-timings poison** — [`a2_two_timings_differ_and_the_boundary_moves`]. A shape check
//!   passes against a server that answers with a post-hoc render of end-of-frame state, which is
//!   structurally blind to exactly the mid-frame effects this method exists to see. Two ROMs that differ
//!   *only* in the scanline at which they repaint the backdrop is the discriminator: a blind server returns
//!   the same picture for both. `oracle_core::testrom::build_cram_midframe` is that fixture.
//! * **Gate (ii), live vs post-hoc on one machine** — [`restore_drops_the_frame_and_the_same_machine_answers_staterender`].
//!   `color_1536` is the corpus's pinned LIVE-DIFFERS ROM (`crates/oracle-core/tests/scanline_goldens.rs`),
//!   so the same machine point read twice — once with a retained frame, once after `restore` dropped it —
//!   must disagree. That test doubles as the R4 invalidation pin.
//!
//! A1 (determinism) is [`a1_determinism_three_boots_byte_identical`]: three separate servers, one ROM, the
//! same rows byte for byte.

mod common;

use common::{spawn_with, Client};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const ACTIVE_LINES: usize = 224;

/// The vendored conformance corpus, addressed the way `crates/oracle-core/tests/scanline_goldens.rs`
/// addresses it — same constant, so the two files cannot drift apart on where the ROMs live.
const VENDOR_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/TestRoms");

fn vendored(name: &str) -> Option<Vec<u8>> {
    let p: PathBuf = Path::new(VENDOR_DIR).join(format!("{name}.bin"));
    std::fs::read(p).ok()
}

fn client(h: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(h);
    c.handshake(false);
    c
}

/// Boot `rom` on its own server, advance `frames`, and hand back a connected client. Every test that needs
/// a *drawn* frame goes through here: without at least one completed frame the answer is `stateRender`, and
/// a gate that accepted that would be measuring the wrong instrument (the fragment's own MUST).
fn booted(tag: &str, rom: Vec<u8>, frames: u64) -> (oracle_aether::server::ServerHandle, Client) {
    let h = spawn_with(tag, rom, 1024);
    let mut c = client(&h);
    c.ok("emulator/run_frames", json!({ "frames": frames }));
    (h, c)
}

fn rows_of(r: &Value) -> &Vec<Value> {
    r["rows"].as_array().expect("rows is an array")
}

fn rgb_of(r: &Value, line: usize) -> String {
    let start = r["startLine"].as_u64().expect("startLine") as usize;
    rows_of(r)[line - start]["rgb"]
        .as_str()
        .expect("rgb is a string")
        .to_string()
}

/// A whole row of one colour, spelled the way the handler must spell it: `0x` + `RRGGBB` per pixel,
/// uppercase. The mid-frame fixture paints nothing but backdrop, so every row is one of these two.
fn flat_row(width: usize, hex: &str) -> String {
    format!("0x{}", hex.repeat(width))
}

const BLACK: &str = "000000";
const WHITE: &str = "FFFFFF";

fn keys(v: &Value) -> BTreeSet<String> {
    v.as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>()
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| (*s).to_string()).collect()
}

/// The four keys the *envelope* stamps on after the handler returns (§2.2 / D11, §2.3 / D17) — subtracted
/// rather than listed among the method's own keys, as `pixel_attribution.rs` does.
const ENVELOPE_KEYS: &[&str] = &["frame", "mclk", "running", "droppedEvents"];

fn method_keys(result: &Value) -> BTreeSet<String> {
    let mut k = keys(result);
    for e in ENVELOPE_KEYS {
        k.remove(*e);
    }
    k
}

// -------------------------------------------------------------------------------------------------
// shape
// -------------------------------------------------------------------------------------------------

/// The requested rows come back, contiguous and ascending from `startLine`, each one a full row of pixels.
#[test]
fn the_happy_path_returns_the_requested_rows() {
    let (_h, mut c) = booted("sl-happy", oracle_core::testrom::build(), 3);
    let r = c.ok("emulator/scanlines", json!({"startLine": 10, "count": 3}));
    assert_eq!(r["startLine"], json!(10), "the reply echoes its start line");
    let width = match r["mode"].as_str().expect("mode") {
        "h40" => 320usize,
        "h32" => 256,
        m => panic!("mode must be h40 or h32, got {m}"),
    };
    let rows = rows_of(&r);
    assert_eq!(rows.len(), 3, "rows.length == count");
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(
            row["line"],
            json!(10 + i),
            "rows are contiguous and line-ascending from startLine"
        );
        assert_eq!(row["width"], json!(width), "every row is mode's width");
        let rgb = row["rgb"].as_str().expect("rgb");
        assert_eq!(
            rgb.len(),
            width * 6 + 2,
            "rgb is `0x` + 3 bytes per pixel — {width} px is {} digits",
            width * 6
        );
        assert!(rgb.starts_with("0x"), "rgb carries the D9 prefix");
    }
    assert!(
        matches!(r["source"].as_str(), Some("raster") | Some("stateRender")),
        "source names which instrument answered"
    );
}

/// Both params default, and the defaults are the whole active display — the reading a caller who wants
/// "the picture" gets without having to know 224.
#[test]
fn defaults_cover_the_whole_active_display() {
    let (_h, mut c) = booted("sl-defaults", oracle_core::testrom::build(), 2);
    let r = c.ok("emulator/scanlines", json!({}));
    assert_eq!(r["startLine"], json!(0), "startLine defaults to 0");
    let rows = rows_of(&r);
    assert_eq!(
        rows.len(),
        ACTIVE_LINES,
        "count defaults to through line 223"
    );
    assert_eq!(rows[0]["line"], json!(0));
    assert_eq!(rows[ACTIVE_LINES - 1]["line"], json!(223));
}

/// **Refused, never clipped.** A clipped range returns fewer rows than asked for and the caller has no way
/// to notice; `-32602` is the fragment's own spelling, mechanically enforced for the two single-param
/// bounds and prose-only for the sum (no static schema can see it), so the server must enforce all three.
#[test]
fn bounds_are_refused_never_clipped() {
    let (_h, mut c) = booted("sl-bounds", oracle_core::testrom::build(), 2);
    for (params, why) in [
        (
            json!({"startLine": 224}),
            "startLine past the last active line",
        ),
        (json!({"count": 0}), "a count of zero returns nothing"),
        (json!({"count": 225}), "a count past the whole display"),
        (
            json!({"startLine": 200, "count": 25}),
            "the sum crosses 224 — the prose-only bound",
        ),
        (json!({"startLine": -1}), "a negative line is not a line"),
        (
            json!({"startLine": "10"}),
            "counts are numbers, not hex (D9)",
        ),
    ] {
        let e = c.err("emulator/scanlines", params.clone());
        assert_eq!(e["code"], json!(-32602), "{why}: {params}");
    }

    // And the refusal really is a refusal: the request that straddles the bound returns no rows at all,
    // rather than the 24 that would fit.
    let v = c.call("emulator/scanlines", json!({"startLine": 200, "count": 25}));
    assert!(
        v.get("result").is_none(),
        "a refused range must carry no result — clipping to 24 rows would be silent data loss"
    );
}

/// **A pure read**: §6's run-control state rule does not apply, and a server MUST NOT refuse this on a
/// free-running machine — exactly as `read`, `sprites` and `pixel_attribution` are, and mirroring
/// `memory_hash.rs`'s test of the same pin. The envelope's `running: true` is the contract's whole answer
/// to a torn sample, so it is asserted here too: a reply that answered by quietly pausing first would
/// satisfy the call and violate the rule (§8 item 12 — never pause or resume implicitly to make a call
/// succeed), and `running` is what tells the two apart.
#[test]
fn it_answers_a_free_running_machine() {
    let (_h, mut c) = booted(
        "sl-freerun",
        oracle_core::testrom::build_cram_midframe(100),
        4,
    );
    c.ok("emulator/resume", json!({}));
    let r = c.ok("emulator/scanlines", json!({"startLine": 0, "count": 4}));
    assert_eq!(rows_of(&r).len(), 4, "a free-running machine still answers");
    assert_eq!(
        r["running"],
        json!(true),
        "the machine is still running — the read must not have paused it to succeed"
    );
}

/// The exact key set (no surplus), and `caveat` **absent** on a raster reply. The harness already closes
/// every result at the TOP level (`common::schema` applies `unevaluatedProperties: false` in `Client::recv`),
/// so a stray top-level key is rejected on the wire before this runs. What this test alone covers: the
/// `caveat` ⇔ `source` tie (deliberately left mechanically unenforced in the fragment, ruling disposition
/// (a) — `caveat` is a declared property, so the validator admits it on a raster reply) and the per-row
/// key sets inside `rows[]`, which the top-level closure does not descend into.
#[test]
fn the_key_set_is_exact() {
    let (_h, mut c) = booted("sl-keys", oracle_core::testrom::build_cram_midframe(100), 4);
    let r = c.ok("emulator/scanlines", json!({"startLine": 0, "count": 2}));
    assert_eq!(
        r["source"],
        json!("raster"),
        "a run of 4 frames retains one"
    );
    assert_eq!(
        method_keys(&r),
        set(&["startLine", "mode", "source", "rows"]),
        "exactly the schematized keys — and no `caveat` on a raster reply"
    );
    for row in rows_of(&r) {
        assert_eq!(
            keys(row),
            set(&["line", "width", "rgb"]),
            "rows carry exactly line/width/rgb"
        );
    }
}

// -------------------------------------------------------------------------------------------------
// A1 — determinism
// -------------------------------------------------------------------------------------------------

/// **Acceptance A1.** Three separate boots of the same ROM, driven identically, produce byte-identical
/// rows. Not "similar" and not "hash-equal in aggregate": the full display, compared as the serialized row
/// array, because a readback whose bytes wander between boots cannot be the substrate for a golden.
#[test]
fn a1_determinism_three_boots_byte_identical() {
    let mut replies: Vec<Value> = Vec::new();
    for i in 0..3 {
        let (_h, mut c) = booted(
            &format!("sl-a1-{i}"),
            oracle_core::testrom::build_cram_midframe(100),
            6,
        );
        let r = c.ok("emulator/scanlines", json!({}));
        assert_eq!(r["source"], json!("raster"), "boot {i} must answer live");
        replies.push(r["rows"].clone());
    }
    let first = serde_json::to_string(&replies[0]).unwrap();
    for (i, r) in replies.iter().enumerate().skip(1) {
        assert_eq!(
            serde_json::to_string(r).unwrap(),
            first,
            "boot {i}'s rows differ from boot 0's — the readback is not deterministic"
        );
    }
    assert_eq!(
        replies[0].as_array().unwrap().len(),
        ACTIVE_LINES,
        "the comparison covered the whole display, not a stripe of it"
    );
}

// -------------------------------------------------------------------------------------------------
// A2 — suite gate (i), the two-timings poison
// -------------------------------------------------------------------------------------------------

/// **Acceptance A2, and the adoption condition's suite gate (i).** Two ROMs identical but for the scanline
/// at which they repaint the backdrop. Four things are asserted, and each closes a different way of passing
/// vacuously:
///
/// * `source == "raster"` — a `stateRender` reply passes every shape check in this file and is
///   structurally blind to mid-frame effects, so the fragment makes this check a MUST for liveness gates.
/// * **within one frame**, a row above the boundary differs from a row below it — a post-hoc render draws
///   the whole frame in the last colour written, so this alone fails a blind server.
/// * **the landing row is SPLIT** — uniform colour-A prefix, uniform colour-B suffix, exactly one
///   transition, at a column inside a band derived below. This is the `F-SCANLINE-SUBLINE` poison: a
///   line-atomic renderer draws that row wholly in one colour and cannot pass it, whichever colour it picks.
/// * **between the two ROMs**, the rows in the band between the two boundaries are swapped — so a server
///   that somehow captured *a* raster but ignored the timing still fails.
///
/// The first **fully** recoloured row is `line + 1`: the write lands part-way across line `line`, which has
/// already been resolved, so that row splits and the next is the first wholly-B one. An off-by-one in the
/// handler's row slicing still shows up here as a boundary in the wrong place.
#[test]
fn a2_two_timings_differ_and_the_boundary_moves() {
    let (_ha, mut a) = booted("sl-a2-a", oracle_core::testrom::build_cram_midframe(50), 6);
    let (_hb, mut b) = booted("sl-a2-b", oracle_core::testrom::build_cram_midframe(150), 6);
    let ra = a.ok("emulator/scanlines", json!({}));
    let rb = b.ok("emulator/scanlines", json!({}));

    for (tag, r) in [("50", &ra), ("150", &rb)] {
        assert_eq!(
            r["source"],
            json!("raster"),
            "ROM {tag}: a stateRender reply cannot see mid-frame effects at all — the gate is void \
             without this"
        );
    }
    let width = 256usize; // the fixture programs H32 (reg 12 = $8C00)
    assert_eq!(ra["mode"], json!("h32"));
    let (black, white) = (flat_row(width, BLACK), flat_row(width, WHITE));

    // Within ROM A's single frame: both halves are visible at once. This is the mid-frame effect itself.
    assert_ne!(
        rgb_of(&ra, 40),
        rgb_of(&ra, 160),
        "one frame must carry both backdrops — a post-hoc render carries only the last one"
    );

    // The landing rows are split, and the first FULLY recoloured row is line + 1.
    let xa = assert_split_row(&rgb_of(&ra, 50), width, "ROM 50, line 50");
    let xb = assert_split_row(&rgb_of(&rb, 150), width, "ROM 150, line 150");
    assert_eq!(
        rgb_of(&ra, 51),
        white,
        "ROM 50: line 51 is the first WHOLLY colour-B row"
    );
    assert_eq!(
        rgb_of(&rb, 151),
        white,
        "ROM 150: first wholly-B row at 151"
    );
    assert_eq!(
        rgb_of(&ra, 49),
        black,
        "ROM 50: line 49 is still wholly colour A"
    );
    assert_eq!(rgb_of(&rb, 149), black, "ROM 150: line 149 wholly A");

    // The transition column sits inside the band the fixture's own instruction stream implies (see
    // `landing_band`). Both ROMs run the identical poll loop, so both land in the same band.
    let (lo, hi) = landing_band(width);
    for (tag, x) in [("ROM 50", xa), ("ROM 150", xb)] {
        assert!(
            (lo..=hi).contains(&x),
            "{tag}: the backdrop write landed at pixel {x}, outside the derived band {lo}..={hi}. \
             That band comes from the fixture's own instruction stream, so a miss means either the \
             mclk -> pixel mapping or the poll-loop derivation is wrong — do NOT widen the band to fit."
        );
    }

    // Between the ROMs: every row in the band between the two boundaries is swapped. Line 150 is excluded —
    // it is ROM 150's own split row — so the band runs 51..=149 and line 150 is covered by the split checks.
    for line in 51..=149 {
        assert_eq!(
            rgb_of(&ra, line),
            white,
            "ROM 50: line {line} is past its boundary"
        );
        assert_eq!(
            rgb_of(&rb, line),
            black,
            "ROM 150: line {line} is before its boundary"
        );
    }
    // The two rows the equality list below has to drop are each ROM's *other* row: line 150 is ROM B's
    // split row but is wholly uniform in ROM A, and line 50 is the mirror. Dropping them from the equality
    // list without asserting them anywhere would leave a hole exactly where the split lives, so they are
    // pinned here as uniform — which is also a second, independent statement that only ONE row per ROM
    // splits.
    assert_eq!(
        rgb_of(&ra, 150),
        white,
        "ROM 50: line 150 is far past its boundary and wholly colour B — only ROM 150 splits there"
    );
    assert_eq!(
        rgb_of(&rb, 50),
        black,
        "ROM 150: line 50 is far above its boundary and wholly colour A — only ROM 50 splits there"
    );

    // Outside the band the two agree, which is what makes the disagreement inside it a *timing* difference
    // rather than two unrelated pictures. Line 50 is ROM A's split row and line 150 is ROM B's, so neither
    // belongs in an equality list; 49 and 151 are their wholly-uniform neighbours and do.
    for line in [0usize, 25, 49, 151, 200, 223] {
        assert_eq!(
            rgb_of(&ra, line),
            rgb_of(&rb, line),
            "line {line} is outside the band and must match"
        );
    }
}

/// Assert `row` is a **split row** — a uniform prefix, a uniform suffix, exactly one transition — and
/// return the transition column. The three facts together are what a line-atomic renderer cannot produce:
/// it emits a uniform row, which has zero transitions.
fn assert_split_row(row: &str, width: usize, tag: &str) -> usize {
    let px: Vec<&str> = row
        .strip_prefix("0x")
        .expect("rgb starts 0x")
        .as_bytes()
        .chunks(6)
        .map(|c| std::str::from_utf8(c).expect("hex"))
        .collect();
    assert_eq!(px.len(), width, "{tag}: row width");
    assert_eq!(px[0], BLACK, "{tag}: the row opens on colour A");
    assert_eq!(px[width - 1], WHITE, "{tag}: and closes on colour B");
    let transitions: Vec<usize> = (1..width).filter(|&i| px[i] != px[i - 1]).collect();
    assert_eq!(
        transitions.len(),
        1,
        "{tag}: EXACTLY one A->B transition — {} found. More than one means the journal was applied \
         out of order or more than once; zero means the row is uniform, i.e. line-atomic.",
        transitions.len()
    );
    let x = transitions[0];
    assert!(
        px[..x].iter().all(|&p| p == BLACK) && px[x..].iter().all(|&p| p == WHITE),
        "{tag}: uniform colour-A prefix and uniform colour-B suffix"
    );
    x
}

/// The band the backdrop write's landing column must fall in — **derived from the fixture's own source, not
/// from a measurement or the design note's estimate** (ruling Q5).
///
/// `build_cram_midframe`'s poll block is four instructions, and the 68000 costs are:
///
/// | instruction | cycles |
/// |---|---|
/// | `move.w (a3),d0`  — read the HV counter | 8 |
/// | `lsr.w #8,d0`     — V into the low byte | 6 + 2x8 = 22 |
/// | `cmpi.b #line,d0` | 8 |
/// | `bcs .top`        — taken 10 / not taken 8 | 10 / 8 |
///
/// So one *spinning* iteration is `POLL_ITERATION` = 48 cycles, and the iteration that finally sees
/// `V >= line` costs 46 before falling through. The previous read saw `V = line - 1` exactly one iteration
/// earlier, so **the successful read begins somewhere in the first `POLL_ITERATION` cycles of the line** —
/// that jitter is the band's width. From that read to the start of the CRAM data write is `POST_POLL`: the
/// 46 above plus the 20-cycle `move.l #cmd,(a0)` control write that precedes it.
///
/// The write is stamped at its instruction **start** (follow-up F-SUBLINE-ACCESSMCLK), so the landing offset
/// into the line is `[POST_POLL, POST_POLL + POLL_ITERATION)` CPU cycles, converted at
/// `MCLK_PER_CPU_CYCLE` and divided by this mode's mclk-per-pixel.
///
/// Pinning an exact column instead would be a flake: a frame is `896_040 / 7 = 128_005.71` CPU cycles, not
/// an integer, so the loop's phase relative to the line drifts frame to frame. Determinism of the column
/// *within* one boot is covered by the A1 gate, which is where it belongs.
///
/// **What this band does and does not discriminate**, measured rather than assumed — an earlier draft of
/// this note claimed more than the arithmetic supports and is corrected here. The measured column is 53 and
/// the band is 46..=79, so the band catches a **gross** error in the mclk→pixel derivation: forgetting
/// `MCLK_PER_CPU_CYCLE` entirely, a factor-of-two axis mistake, or a landing that drifts out of the poll
/// loop's own window. It does **not** discriminate either named pixel-axis decision at this fixture's mode:
///
/// * **B-1** (the 8/10-mclk axis versus `h_counter`'s 422/342 grid) — at H32 the two agree *exactly*
///   (`3420 / 342 == 2560 / 256 == 10`), so no H32 fixture can tell them apart. B-1's ~33-mclk disagreement
///   is an H40-only question, carried as follow-up **F-SUBLINE-HGRID**.
/// * **B-2** (the axis coming from the resolved row's own mode, not a live register read) — substituting
///   H40's 8 mclk/px here maps column 53 to 66, which is still inside 46..=79, and every other assertion in
///   this gate is column-agnostic. Discriminating B-2 would need a landing past roughly px 136, i.e. an
///   H40 fixture or a mid-line mode switch. **Named gap**, alongside F-SUBLINE-HGRID: B-2 is implemented
///   and unit-tested at core level, but no wire-level gate pins it.
fn landing_band(width: usize) -> (usize, usize) {
    const POLL_ITERATION: u64 = 8 + 22 + 8 + 10;
    const POST_POLL: u64 = (8 + 22 + 8 + 8) + 20;
    let mclk_per_pixel = oracle_core::vdp::MCLK_PER_ACTIVE / width as u64;
    let lo = POST_POLL * oracle_core::system::MCLK_PER_CPU_CYCLE / mclk_per_pixel;
    let hi =
        (POST_POLL + POLL_ITERATION) * oracle_core::system::MCLK_PER_CPU_CYCLE / mclk_per_pixel;
    (lo as usize, hi as usize)
}

// -------------------------------------------------------------------------------------------------
// suite gate (ii) + R4 — restore drops the frame, and the fallback really is a different picture
// -------------------------------------------------------------------------------------------------

/// **Suite gate (ii) and the R4 invalidation pin, in one test.** `restore` replaces the machine, so the
/// retained frame belongs to a machine that is no longer there and the contract drops it — the next read
/// answers `stateRender` + `caveat` until a new frame completes.
///
/// The ROM is `color_1536`, the corpus's pinned LIVE-DIFFERS row: it rewrites CRAM ~645 times per frame
/// during active display, so the live picture holds ~1400 colours and the post-hoc render holds 4. That is
/// what makes this more than a flag check — the same machine point, read twice, must produce **different
/// pixels**. (`cram_flicker` is pinned IDENTICAL-TO-POST-HOC and would make this pass vacuously; it is not
/// a substitute.)
#[test]
fn restore_drops_the_frame_and_the_same_machine_answers_staterender() {
    let Some(rom) = vendored("color_1536") else {
        // Mirrors `scanline_goldens.rs`: skip locally when the corpus is absent, but never under CI, where a
        // fetch failure must fail loudly instead of leaving the gate green and empty.
        assert!(
            std::env::var_os("CI").is_none(),
            "CI: vendored test ROM color_1536.bin is missing from {VENDOR_DIR} — \
             tools/fetch-testroms.sh must run before the test job, or this gate passes vacuously"
        );
        eprintln!("SKIP: {VENDOR_DIR}/color_1536.bin not present");
        return;
    };
    let (_h, mut c) = booted("sl-restore", rom, 12);

    let live = c.ok("emulator/scanlines", json!({}));
    assert_eq!(
        live["source"],
        json!("raster"),
        "a run of 12 frames retains one — without this the comparison below is stateRender vs stateRender"
    );
    assert!(
        live.get("caveat").is_none(),
        "no caveat on a raster reply — it is emitted on the fallback only"
    );

    let cp = c.ok("emulator/checkpoint", json!({}));
    let id = cp["id"].clone();
    c.ok("emulator/restore", json!({ "id": id }));

    let after = c.ok("emulator/scanlines", json!({}));
    assert_eq!(
        after["source"],
        json!("stateRender"),
        "restore drops the retained frame — a server MUST NOT serve a frame drawn by a machine that is \
         no longer there"
    );
    assert!(
        after.get("caveat").is_some(),
        "the fallback declares itself (§2.4 rule 4) — a provenance nobody stated is worse than a wrong one"
    );
    assert_eq!(
        after["mode"], live["mode"],
        "the width class did not change across the restore"
    );

    let (a, b) = (rows_of(&live), rows_of(&after));
    assert_eq!(a.len(), b.len(), "both replies cover the whole display");
    let differing = a
        .iter()
        .zip(b)
        .filter(|(x, y)| x["rgb"] != y["rgb"])
        .count();
    assert!(
        differing > 0,
        "color_1536 is the pinned LIVE-DIFFERS ROM: the live raster and the post-hoc render of the same \
         machine point MUST disagree. Zero differing rows means the raster path collapsed into the \
         fallback and every other test here would still pass."
    );
}
