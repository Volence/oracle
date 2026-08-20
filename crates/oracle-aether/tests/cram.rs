//! `emulator/read_cram` and `emulator/write_cram` — `protocol.md` §6 (VRAM / CRAM / layers), specified
//! by §11.17 (CR-27, `docs/2026-08-19-cr27-cram-params.md`, ruled in `docs/2026-08-19-ruling-cr27.md`).
//!
//! Both rows were catalogued from the legacy socket in this document's first draft and neither had ever
//! been served. Every reply here is validated against the vendored schema on the way past — open, and
//! then **closed** with `unevaluatedProperties: false` per §8 item 20 — by `common::schema`, so the shape
//! assertions below are about *meaning* rather than about key presence.
//!
//! The adoption gates the CR names are the shape of this file: the two happy paths, one refusal per
//! bound, the pause gate on the write and its deliberate absence on the read, a cross-instrument tie, and
//! the two **standing properties** of a poke — that it never reaches the watch surface, and that it does
//! not repaint a frame already drawn.

mod common;

use common::{spawn_system, Client};
use oracle_core::system::System;
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn machine() -> System {
    machine_with(oracle_core::testrom::build())
}

fn machine_with(rom: Vec<u8>) -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();
    sys
}

fn client(h: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(h);
    c.handshake(false);
    c
}

/// The four keys the envelope stamps on after the handler returns (§2.2 / D11, §2.3 / D17) — subtracted
/// rather than listed among the method's own keys, as `scanlines.rs` and `pixel_attribution.rs` do.
const ENVELOPE_KEYS: &[&str] = &["frame", "mclk", "running", "droppedEvents"];

fn method_keys(result: &Value) -> BTreeSet<String> {
    let mut k: BTreeSet<String> = result
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect();
    for e in ENVELOPE_KEYS {
        k.remove(*e);
    }
    k
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| (*s).to_string()).collect()
}

/// Poke one entry and return the reply. The machine is paused in every test that uses this.
fn poke(c: &mut Client, line: u64, index: u64, colour: Value) -> Value {
    let mut p = json!({"line": line, "index": index});
    let obj = p.as_object_mut().unwrap();
    for (k, v) in colour.as_object().expect("a colour object") {
        obj.insert(k.clone(), v.clone());
    }
    c.ok("emulator/write_cram", p)
}

fn entry_of(palette: &Value, line: u64, index: u64) -> Value {
    palette
        .as_array()
        .expect("palette is an array")
        .iter()
        .find(|e| e["line"] == json!(line) && e["index"] == json!(index))
        .unwrap_or_else(|| panic!("no entry for line {line} index {index}"))
        .clone()
}

// ---------------------------------------------------------------------------------------------------
// read_cram — shape
// ---------------------------------------------------------------------------------------------------

/// The two answers, and the tie between them. `line` given → 16 entries and the echo; `line` omitted →
/// 64 entries and **no** `line` key, because its presence is what tells a client which answer arrived.
/// The fragment enforces the tie in both directions; this pins that we emit the side of it we claim to.
#[test]
fn one_line_echoes_and_the_whole_palette_does_not() {
    let h = spawn_system("cram-shape", machine(), 64);
    let mut c = client(&h);

    let one = c.ok("emulator/read_cram", json!({"line": 2}));
    assert_eq!(method_keys(&one), set(&["line", "palette"]));
    assert_eq!(one["line"], json!(2), "the echo says which answer this is");
    assert_eq!(one["palette"].as_array().unwrap().len(), 16);

    let all = c.ok("emulator/read_cram", json!({}));
    assert_eq!(
        method_keys(&all),
        set(&["palette"]),
        "`line` is ABSENT, not null, when the whole palette was asked for"
    );
    assert_eq!(all["palette"].as_array().unwrap().len(), 64);
}

/// Entries are line-ascending then index-ascending and contiguous, every entry carries its own
/// `(line, index)` — the single-line answer included, so one entry on its own addresses the same cell
/// through `write_cram` — and `cramAddr` is `(line * 16 + index) * 2` at every one of the 64.
#[test]
fn entries_are_contiguous_and_self_addressing() {
    let h = spawn_system("cram-order", machine(), 64);
    let mut c = client(&h);

    let all = c.ok("emulator/read_cram", json!({}));
    for (i, e) in all["palette"].as_array().unwrap().iter().enumerate() {
        let line = i / 16;
        let index = i % 16;
        assert_eq!(e["line"], json!(line), "entry {i} is out of order");
        assert_eq!(e["index"], json!(index), "entry {i} is out of order");
        assert_eq!(
            e["cramAddr"],
            json!(format!("0x{:08X}", i * 2)),
            "entry {i}: cramAddr is (line*16 + index)*2"
        );
    }

    // The single-line answer carries `line` on every entry too — that is what makes an entry hand
    // straight back to `write_cram` without the client remembering which request it came from.
    let one = c.ok("emulator/read_cram", json!({"line": 3}));
    for (index, e) in one["palette"].as_array().unwrap().iter().enumerate() {
        assert_eq!(e["line"], json!(3));
        assert_eq!(e["index"], json!(index));
        assert_eq!(e["cramAddr"], json!(format!("0x{:08X}", (48 + index) * 2)));
    }
}

/// An entry is the **stored** colour, and `raw` and the three components are the same nine bits read two
/// ways. Pinned against a palette this test itself established, because an assertion that still passes
/// when the handler returns a fixed palette is vacuous — the CR's own mutation requirement.
#[test]
fn an_entry_is_the_stored_colour_in_both_spellings() {
    let h = spawn_system("cram-stored", machine(), 64);
    let mut c = client(&h);

    // Establish a palette with distinguishable entries, one per channel plus a mixed one.
    let planted = [
        (0u64, 0u64, 7u64, 0u64, 0u64, "0x000E"),
        (0, 1, 0, 7, 0, "0x00E0"),
        (0, 2, 0, 0, 7, "0x0E00"),
        (1, 5, 2, 4, 6, "0x0C84"),
        (3, 15, 7, 7, 7, "0x0EEE"),
    ];
    for (line, index, r, g, b, _) in planted {
        poke(&mut c, line, index, json!({"r": r, "g": g, "b": b}));
    }

    let all = c.ok("emulator/read_cram", json!({}));
    for (line, index, r, g, b, raw) in planted {
        let e = entry_of(&all["palette"], line, index);
        assert_eq!(e["raw"], json!(raw), "line {line} index {index}: raw word");
        assert_eq!(e["r"], json!(r), "line {line} index {index}: r");
        assert_eq!(e["g"], json!(g), "line {line} index {index}: g");
        assert_eq!(e["b"], json!(b), "line {line} index {index}: b");
        // No 8-bit expansion on this row, deliberately (F-CRAM-RAMP): the displayed colour is
        // pixel_attribution's `rgb`, and this catalog has never pinned an intensity ramp.
        assert_eq!(
            method_keys(&e),
            set(&["line", "index", "cramAddr", "raw", "r", "g", "b"]),
            "no expanded 8-bit triple, no cramIndex"
        );
    }
}

/// **The cross-instrument tie.** `emulator/read` at `space: "cram"` reads the same array through an
/// entirely separate path; the two must agree byte for byte, or one of them is describing a palette the
/// machine does not have. (`cram_rgb_matches_cram_decoded` is the same idea inside the core.)
#[test]
fn read_cram_agrees_with_emulator_read_at_the_same_address() {
    let h = spawn_system("cram-tie", machine(), 64);
    let mut c = client(&h);
    poke(&mut c, 2, 9, json!({"raw": 0x0A46}));

    let e = entry_of(
        &c.ok("emulator/read_cram", json!({"line": 2}))["palette"],
        2,
        9,
    );
    let addr = e["cramAddr"].as_str().unwrap().to_string();
    let raw = c.ok(
        "emulator/read",
        json!({"space": "cram", "addr": addr, "len": 2}),
    );
    assert_eq!(
        raw["bytes"],
        json!("0x0A46"),
        "the join key addresses the same two bytes on the other instrument"
    );
    assert_eq!(e["raw"], json!("0x0A46"));
}

/// A **pure read**: not refused on a free-running machine, on the `read`/`sprites`/`pixel_attribution`/
/// `scanlines` precedent. D11's stamp is the whole answer to a torn palette sample — and the reply says
/// so itself, which is why `running` is asserted rather than just the absence of an error.
#[test]
fn read_cram_is_not_gated_on_a_paused_machine() {
    let h = spawn_system("cram-ungated", machine(), 64);
    let mut c = client(&h);
    c.ok("emulator/resume", json!({}));

    let r = c.ok("emulator/read_cram", json!({}));
    assert_eq!(r["running"], json!(true), "answered mid-run, and says so");
    assert_eq!(r["palette"].as_array().unwrap().len(), 64);
}

/// `line` out of range is `-32602` — **refused, never clipped**. A server that clamped 4 to 3 would hand
/// back a plausible sixteen entries describing the wrong line.
#[test]
fn read_cram_refuses_a_line_outside_the_four() {
    let h = spawn_system("cram-line-oob", machine(), 64);
    let mut c = client(&h);
    for bad in [json!(4), json!(99), json!(-1), json!("0x2")] {
        let e = c.err("emulator/read_cram", json!({"line": bad}));
        assert_eq!(e["code"], json!(-32602), "line {bad} must be refused");
        assert!(
            e["message"].as_str().unwrap().contains("line"),
            "the refusal must name the offending param: {}",
            e["message"]
        );
    }
}

// ---------------------------------------------------------------------------------------------------
// write_cram — the colour, the bounds, the gate
// ---------------------------------------------------------------------------------------------------

/// Both spellings land the **same** colour, and the reply is self-describing: `line` and `index` echoed
/// beside `cramAddr` and `value`, all four REQUIRED. `value` is the word actually stored.
#[test]
fn the_two_colour_spellings_land_the_same_word_and_the_reply_says_where() {
    let h = spawn_system("cram-spellings", machine(), 64);
    let mut c = client(&h);

    let by_triple = poke(&mut c, 1, 4, json!({"r": 6, "g": 2, "b": 4}));
    assert_eq!(
        method_keys(&by_triple),
        set(&["line", "index", "cramAddr", "value"]),
        "all four REQUIRED, and no caveat — the fragment declares it absent"
    );
    assert_eq!(by_triple["line"], json!(1));
    assert_eq!(by_triple["index"], json!(4));
    assert_eq!(by_triple["cramAddr"], json!("0x00000028"), "(1*16+4)*2");
    assert_eq!(by_triple["value"], json!("0x084C"), "BBB- GGG- RRR-");

    // Clear it, then reach the identical word through `raw`.
    poke(&mut c, 1, 4, json!({"raw": 0}));
    let by_raw = poke(&mut c, 1, 4, json!({"raw": 0x084C}));
    assert_eq!(by_raw["value"], by_triple["value"]);
    assert_eq!(by_raw["cramAddr"], by_triple["cramAddr"]);

    // And the round trip closes through the other method.
    let e = entry_of(
        &c.ok("emulator/read_cram", json!({"line": 1}))["palette"],
        1,
        4,
    );
    assert_eq!(e["raw"], json!("0x084C"));
    assert_eq!(
        (e["r"].clone(), e["g"].clone(), e["b"].clone()),
        (json!(6), json!(2), json!(4))
    );
}

/// **Exactly one colour spelling.** All four bad shapes are `-32602`, and none of them writes anything —
/// the refusal precedes any effect. The sentinel is what makes that second half real rather than assumed:
/// a cell left at its reset value is indistinguishable from a cell a refused write happened to zero.
#[test]
fn the_colour_alternation_refuses_all_four_bad_shapes_before_writing() {
    let h = spawn_system("cram-alternation", machine(), 64);
    let mut c = client(&h);
    poke(&mut c, 0, 7, json!({"raw": 0x0EEE}));

    let bad = [
        (json!({"r": 1, "g": 2, "b": 3, "raw": 0x0100}), "both"),
        (json!({}), "neither"),
        (json!({"r": 1}), "a lone component"),
        (json!({"r": 1, "g": 2}), "two of three"),
        (
            json!({"r": 1, "g": 2, "raw": 0x0100}),
            "partial triple beside raw",
        ),
    ];
    for (colour, why) in bad {
        let mut p = json!({"line": 0, "index": 7});
        for (k, v) in colour.as_object().unwrap() {
            p.as_object_mut().unwrap().insert(k.clone(), v.clone());
        }
        let e = c.err("emulator/write_cram", p);
        assert_eq!(e["code"], json!(-32602), "{why} must be refused");
    }

    let entry = entry_of(
        &c.ok("emulator/read_cram", json!({"line": 0}))["palette"],
        0,
        7,
    );
    assert_eq!(
        entry["raw"],
        json!("0x0EEE"),
        "the sentinel survived every refusal — no write happened"
    );
}

/// A `raw` outside the chip's `$0EEE` mask is **refused, never masked**. This is the one bound the schema
/// cannot express (its `maximum: 3822` is the coarse half), so it is the one most likely to be
/// "helpfully" masked by a later edit — and a reply reporting a value the caller did not send is the
/// silent mutation this bus refuses everywhere else.
#[test]
fn an_out_of_mask_raw_is_refused_not_masked() {
    let h = spawn_system("cram-mask", machine(), 64);
    let mut c = client(&h);
    poke(&mut c, 0, 1, json!({"raw": 0x0246}));

    // Each of these has at least one bit outside 0x0EEE; the first three are *within* the schema's
    // numeric maximum, so only the exact-mask rule catches them.
    for bad in [0x0001u64, 0x0010, 0x0EEF, 0x1000, 0xFFFF] {
        let e = c.err(
            "emulator/write_cram",
            json!({"line": 0, "index": 1, "raw": bad}),
        );
        assert_eq!(e["code"], json!(-32602), "raw {bad:#06X} must be refused");
        assert!(
            e["message"].as_str().unwrap().contains("0x0EEE"),
            "the refusal must name the mask so it is also the fix: {}",
            e["message"]
        );
    }
    let entry = entry_of(
        &c.ok("emulator/read_cram", json!({"line": 0}))["palette"],
        0,
        1,
    );
    assert_eq!(
        entry["raw"],
        json!("0x0246"),
        "nothing was masked and stored"
    );
}

/// `line`, `index` and each component are refused outside their ranges — never clipped.
#[test]
fn write_cram_refuses_every_out_of_range_coordinate() {
    let h = spawn_system("cram-bounds", machine(), 64);
    let mut c = client(&h);
    let colour = json!({"r": 1, "g": 1, "b": 1});

    for (params, why) in [
        (json!({"line": 4, "index": 0}), "line 4"),
        (json!({"line": 0, "index": 16}), "index 16"),
        (json!({"index": 0}), "line missing"),
        (json!({"line": 0}), "index missing"),
    ] {
        let mut p = params;
        for (k, v) in colour.as_object().unwrap() {
            p.as_object_mut().unwrap().insert(k.clone(), v.clone());
        }
        let e = c.err("emulator/write_cram", p);
        assert_eq!(e["code"], json!(-32602), "{why} must be refused");
    }

    for bad in [json!(8), json!(255), json!(-1)] {
        let e = c.err(
            "emulator/write_cram",
            json!({"line": 0, "index": 0, "r": bad, "g": 0, "b": 0}),
        );
        assert_eq!(e["code"], json!(-32602), "component {bad} must be refused");
    }
}

/// **The pause gate** — `-32005` with `data.reason = "machineRunning"`, per §6's run-control state rule.
/// Demand-side confirmed rather than symmetry with `write_memory`: the unpaused case fails for engine
/// reasons anyway (a composed-per-frame palette pipeline overwrites the write within the frame), and the
/// paused machine is where the method earns its keep.
#[test]
fn write_cram_needs_a_paused_machine() {
    let h = spawn_system("cram-gate", machine(), 64);
    let mut c = client(&h);
    c.ok("emulator/resume", json!({}));

    let e = c.err(
        "emulator/write_cram",
        json!({"line": 0, "index": 0, "raw": 0x0EEE}),
    );
    assert_eq!(e["code"], json!(-32005));
    assert_eq!(e["data"]["reason"], json!("machineRunning"));

    // …and the read half is NOT gated, in the same session, so this is a statement about the pair.
    c.ok("emulator/read_cram", json!({}));

    c.ok("emulator/pause", json!({}));
    c.ok(
        "emulator/write_cram",
        json!({"line": 0, "index": 0, "raw": 0x0EEE}),
    );
}

// ---------------------------------------------------------------------------------------------------
// The two standing properties of a poke (§6, stated there rather than as a per-reply caveat)
// ---------------------------------------------------------------------------------------------------

/// **A poke is never offered to the watch surface** — the end-to-end half of that property.
///
/// A hit's `pc` names the instruction that drove the access and a poke has none to name; since §11.15 a
/// captured CRAM write also carries a landing clock a poke cannot supply.
///
/// # What this test does NOT prove, and where the real pin lives
///
/// It cannot distinguish the seam's design from the run lifecycle, and it took a mutation to notice.
/// `capture_armed` is set only for the duration of a `System::run` and cleared at the end, while
/// `write_cram` requires a **paused** machine — so on the wire a poke cannot reach a watch recorder no
/// matter what `Vdp::poke_cram` does. Mutating that function to call `capture` leaves this test green.
///
/// The property is therefore pinned on the primitive, with the recorder explicitly armed, by
/// `oracle_core::vdp::tests::poke_cram_never_captures_even_with_the_recorder_armed` — *that* is the test
/// which catches a later "simplification" into `write_target`. This one pins the composite a client
/// actually observes, which is worth having and is not the same claim.
///
/// The anti-vacuity control below is still doing work: it establishes that the watch is live and
/// matching real guest CRAM writes, so "the counters did not move" is a statement about the poke rather
/// than about a watch that never matched anything. That is why the fixture is `build_cram_midframe` — the
/// plain test ROM never writes CRAM at all (measured: `seen` 89,604, `matched` 0).
#[test]
fn a_poke_is_never_offered_to_the_watch_surface() {
    let h = spawn_system(
        "cram-watch",
        machine_with(oracle_core::testrom::build_cram_midframe(100)),
        1024,
    );
    let mut c = client(&h);
    c.ok(
        "emulator/watchpoint_add",
        json!({"space": "cram", "addr": "0x00000000", "len": 64}),
    );
    c.ok("emulator/run_frames", json!({"frames": 3}));

    let before = c.ok("emulator/watchpoint_hits", json!({}));
    let seen = before["seen"].as_u64().expect("seen");
    let matched = before["matched"].as_u64().expect("matched");
    assert!(
        seen > 0,
        "the recorder must have ridden the run, or this test proves nothing"
    );
    assert!(
        matched > 0,
        "the ROM must drive real CRAM writes through this watch, or the assertion below is vacuous \
         (seen={seen}, matched={matched})"
    );

    // The machine is paused after run_frames. Poke every entry the watch covers.
    for index in 0..16 {
        poke(&mut c, 0, index, json!({"raw": 0x0EEE}));
    }

    let after = c.ok("emulator/watchpoint_hits", json!({}));
    assert_eq!(
        after["matched"], before["matched"],
        "a poke matched a cram watch — it went through the port path"
    );
    assert_eq!(
        after["seen"], before["seen"],
        "a poke was OFFERED to the recorder — `seen` moved even though nothing matched"
    );

    // And the pokes really did land, so the two assertions above are about the watch surface rather
    // than about a write that silently did nothing.
    let e = entry_of(
        &c.ok("emulator/read_cram", json!({"line": 0}))["palette"],
        0,
        15,
    );
    assert_eq!(e["raw"], json!("0x0EEE"), "the pokes landed");
}

/// **A poke does not repaint a frame already drawn.** `emulator/scanlines` reports the retained frame's
/// colours until the machine advances, while anything that re-derives from live state changes at once —
/// the same retained-versus-re-derived split §11.3 and §11.14 already document.
///
/// Both halves are asserted, because only the pair is a statement: the rendered rows unchanged is the
/// property, and `read_cram` changing is what proves the write happened at all.
#[test]
fn a_poke_does_not_repaint_a_frame_already_drawn() {
    let h = spawn_system("cram-retained", machine(), 1024);
    let mut c = client(&h);
    c.ok("emulator/run_frames", json!({"frames": 2}));

    let before = c.ok("emulator/scanlines", json!({"startLine": 100, "count": 4}));
    assert_eq!(
        before["source"],
        json!("raster"),
        "a real drawn frame, not a state render"
    );

    // Repaint the whole palette to a colour nothing on that frame could have used.
    for line in 0..4 {
        for index in 0..16 {
            poke(&mut c, line, index, json!({"r": 7, "g": 0, "b": 7}));
        }
    }

    let after = c.ok("emulator/scanlines", json!({"startLine": 100, "count": 4}));
    assert_eq!(
        after["rows"], before["rows"],
        "the retained frame was repainted — scanlines must report the frame as it was drawn"
    );
    assert_eq!(after["source"], json!("raster"));

    // The re-derived side moved, which is what makes the assertion above a property and not a no-op.
    let e = entry_of(
        &c.ok("emulator/read_cram", json!({"line": 0}))["palette"],
        0,
        0,
    );
    assert_eq!(e["raw"], json!("0x0E0E"), "CRAM itself changed immediately");

    // One frame later the raster has seen the new palette, so the retention above was about the
    // *retained* frame rather than about the poke never taking effect.
    c.ok("emulator/run_frames", json!({"frames": 1}));
    let advanced = c.ok("emulator/scanlines", json!({"startLine": 100, "count": 4}));
    assert_ne!(
        advanced["rows"], before["rows"],
        "advancing the machine must show the new palette"
    );
}

/// The three-boot determinism pattern: the same ROM, booted three times, answers identically. A palette
/// read that varied run to run would make every assertion above an accident of one seed.
#[test]
fn three_boots_answer_identically() {
    let mut seen: Vec<Value> = Vec::new();
    for boot in 0..3 {
        let h = spawn_system(&format!("cram-det-{boot}"), machine(), 64);
        let mut c = client(&h);
        c.ok("emulator/run_frames", json!({"frames": 3}));
        let all = c.ok("emulator/read_cram", json!({}));
        poke(&mut c, 2, 2, json!({"r": 5, "g": 3, "b": 1}));
        let after = c.ok("emulator/read_cram", json!({"line": 2}));
        seen.push(json!({"all": all["palette"], "after": after["palette"]}));
    }
    assert_eq!(seen[0], seen[1], "boot 0 and boot 1 disagree");
    assert_eq!(seen[1], seen[2], "boot 1 and boot 2 disagree");
}
