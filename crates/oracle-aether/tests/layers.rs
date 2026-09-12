//! **The layer-mask pair** — `emulator/get_layer_states` (`protocol.md` §6 line 1136) and
//! `emulator/set_layer_enabled` (§6 line 1192), served 2026-08-26.
//!
//! Both fragments landed upstream final, so this is conformance work with no contract change behind it —
//! the `step*` trio's shape (`tests/step.rs`), one row at a time out of the schematized-not-advertised set.
//!
//! # What the schema checks for free, and what it cannot
//!
//! Every line a [`Client`] receives is validated against the vendored fragment closed with
//! `unevaluatedProperties: false` (`common::schema`), so a surplus key on either reply, a missing one of
//! `get_layer_states`' four, a `caveat` where the fragment declares one absent, or a `layer` outside the
//! enum all fail here without an assertion of their own.
//!
//! What that leaves is everything the validator is structurally blind to, and it is the whole of what makes
//! this feature right rather than merely well-shaped:
//!
//! * **A mask is not a blank.** `{"planeA": false}` and a picture full of backdrop is a conformant reply and
//!   a wrong answer. [`masking_a_layer_reveals_what_is_behind_it`] is the control.
//! * **A mask must not perturb the machine.** [`the_mask_is_not_machine_state`] pins that
//!   `emulator/state_hash` (framebuffer digest included) and `emulator/memory_hash` cannot see it.
//! * **A mask must not be lost.** [`the_mask_survives_reset_reload_rom_and_restore`] pins the other
//!   direction: the three calls that replace the machine leave the debugger's masks alone.
//! * **One mask, every surface.** [`one_mask_is_visible_on_every_surface_that_renders`] ties
//!   `emulator/screenshot`, `emulator/scanlines` and `emulator/pixel_attribution` to the same expected
//!   pixels, derived from the core renderer against a copy of the very `System` the server was handed.
//! * **The four names are the contract's.** [`the_mask_vocabulary_is_the_contract_fragments_own`] parses
//!   the vendored schema and compares, rather than trusting four string literals to have been typed right.
//!
//! # Where the expectations come from
//!
//! The fixture machine is built here, so every expected picture is computed by calling the **core**
//! renderer on an identical `System` before the server ever sees one — never read back off a reply and
//! re-asserted.

mod common;

use common::{spawn_system, Client};
use oracle_aether::host::{Host, HostConfig};
use oracle_core::render::LayerMask;
use oracle_core::system::System;
use oracle_core::vdp::Vdp;
use serde_json::{json, Value};
use std::collections::BTreeSet;

// -------------------------------------------------------------------------------------------------
// Fixture
// -------------------------------------------------------------------------------------------------

/// Pattern indices, one solid colour each, so a rendered dot names the layer that drew it.
const TILE_B: u16 = 0x11;
const TILE_A: u16 = 0x12;
const TILE_S: u16 = 0x13;
/// SAT base: reg 5 = $58 → `($58 & $7E) << 9` = $B000.
const SAT_BASE: u16 = 0xB000;

fn set_reg(v: &mut Vdp, reg: u8, val: u8) {
    v.control_write(0x8000 | (u16::from(reg) << 8) | u16::from(val), 0);
}

fn set_addr(v: &mut Vdp, code: u8, addr: u16) {
    v.control_write(((u16::from(code) & 0x03) << 14) | (addr & 0x3FFF), 0);
    v.control_write(((u16::from(code) >> 2) << 4) | (addr >> 14), 0);
}

fn write_vram(v: &mut Vdp, addr: u16, words: &[u16]) {
    set_addr(v, 0x01, addr);
    for w in words {
        v.data_write(*w);
    }
}

fn write_cram(v: &mut Vdp, index: u16, word: u16) {
    set_addr(v, 0x03, index * 2);
    v.data_write(word);
}

/// **The stack.** At screen (0,0): an opaque low-priority sprite over an opaque low-priority plane A cell
/// over an opaque low-priority plane B cell, with a backdrop colour distinct from all three.
///
/// Four distinguishable colours is the whole point — a mask implemented as a post-hoc blank and a mask
/// implemented as a fall-through are indistinguishable on a scene where the layer behind is the backdrop
/// colour anyway.
fn layered_machine() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    let v = sys.vdp_mut();
    v.vram_mut().fill(0);
    // Reg $01 FIRST: the mode-4 register mask discards writes above register 10 while M5 is clear, so an
    // $0C written ahead of it is silently dropped (the trap `tests/pixel_attribution.rs` documents).
    set_reg(v, 0x01, 0x74); // display on, mode 5, DMA enable
    set_reg(v, 0x0C, 0x81); // H40
    set_reg(v, 0x02, 0x30); // plane A nametable @ $C000
    set_reg(v, 0x03, 0x28); // window nametable @ $A000
    set_reg(v, 0x04, 0x07); // plane B nametable @ $E000
    set_reg(v, 0x05, 0x58); // SAT @ $B000
    set_reg(v, 0x07, 0x04); // backdrop = CRAM entry 4
    set_reg(v, 0x0B, 0x00); // full h + full v scroll
    set_reg(v, 0x0D, 0x20); // h-scroll table @ $8000
    set_reg(v, 0x0F, 0x02); // autoincrement 2
    set_reg(v, 0x10, 0x00); // 32x32 planes
    set_reg(v, 0x11, 0x00); // no window
    set_reg(v, 0x12, 0x00);

    // Solid patterns: nibble 1 / 2 / 3 respectively.
    write_vram(v, TILE_B * 32, &[0x1111; 16]);
    write_vram(v, TILE_A * 32, &[0x2222; 16]);
    write_vram(v, TILE_S * 32, &[0x3333; 16]);
    // Four distinct colours, written rather than left to power-on randomness.
    write_cram(v, 1, 0x000E); // red   — plane B
    write_cram(v, 2, 0x0E00); // blue  — plane A
    write_cram(v, 3, 0x00E0); // green — sprite
    write_cram(v, 4, 0x0EEE); // white — backdrop

    // The stack: a 4x4-cell block of each plane at the top-left (x 0-31, y 0-31), one behind the other.
    // H40's nametable row stride is 64 cells.
    for row in 0..4u16 {
        for col in 0..4u16 {
            let off = (row * 64 + col) * 2;
            write_vram(v, 0xE000 + off, &[TILE_B]);
            write_vram(v, 0xC000 + off, &[TILE_A]);
        }
    }
    // One 1x1-cell sprite at screen (0,0) — only `TILE_S` is filled, so a larger sprite would be
    // transparent past its first cell and the size would be a claim the VRAM does not back. Link 0 ends
    // the walk; the Y/X fields carry the +128 screen offset.
    write_vram(v, SAT_BASE, &[128, 0x0000, TILE_S, 128]);

    // **Somewhere each layer wins alone**, so a sweep that hides one layer at a time can actually move the
    // picture. Without this a mask that reached nothing would still satisfy an "unmasked equals unmasked"
    // control — the alternative green path a planted defect found in the core's own fixture.
    set_reg(v, 0x11, 0x88); // right window from x = 8 * 16 = 128
    for row in 0..4u16 {
        write_vram(v, 0xC000 + (row * 64 + 8) * 2, &[TILE_A]); // plane A alone, x 64-71
        write_vram(v, 0xE000 + (row * 64 + 12) * 2, &[TILE_B]); // plane B alone, x 96-103
        write_vram(v, 0xA000 + (row * 64 + 16) * 2, &[TILE_S]); // window, x 128-135
    }
    sys
}

/// Every maskable layer must be the visible winner somewhere in the frame — i.e. hiding any one of them,
/// alone, changes the picture. The precondition any sweep over the four names needs before its comparison
/// is evidence of anything.
fn assert_every_layer_is_visible_somewhere() {
    let (_, _, base) = expected_frame(LayerMask::ALL);
    for name in ["planeA", "planeB", "window", "sprites"] {
        let (_, _, hidden) = expected_frame(mask_without(name));
        assert_ne!(
            hidden, base,
            "fixture precondition: hiding {name} must change the picture — a comparison that cannot \
             move is not evidence"
        );
    }
}

fn client(handle: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(handle);
    c.handshake(false);
    c
}

/// A private path for a screenshot. **Never the default.**
///
/// `emulator/screenshot` with no `path` writes `$TMPDIR/oracle-frame-{frame}.png`, which is a function of
/// the emulated frame and of nothing else — so two tests in this binary that capture at the same frame
/// write and read the *same* file, in parallel, and the one that loses reads the other's picture. That is
/// not a hypothetical: it is what turned a green suite into an intermittently red one here, and it is
/// invisible when a single test file is run alone.
fn shot_path(tag: &str) -> String {
    std::env::temp_dir()
        .join(format!("lay-{tag}-{}.png", std::process::id()))
        .display()
        .to_string()
}

/// The vendored contract schema — the same bytes `common::schema` validates against.
fn schema() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/contract/bus-protocol.schema.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).expect("read the vendored schema"))
        .expect("the vendored schema parses")
}

fn set_of(v: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    v.into_iter().collect()
}

/// `LayerMask` with one wire-named layer switched off. The mapping is spelled once, here, and
/// [`the_mask_vocabulary_is_the_contract_fragments_own`] proves the four names are the fragment's.
fn mask_without(name: &str) -> LayerMask {
    let mut m = LayerMask::ALL;
    match name {
        "planeA" => m.plane_a = false,
        "planeB" => m.plane_b = false,
        "window" => m.window = false,
        "sprites" => m.sprites = false,
        other => panic!("no such mask target: {other}"),
    }
    m
}

// -------------------------------------------------------------------------------------------------
// The vocabulary
// -------------------------------------------------------------------------------------------------

/// **Derived, never copied.** Three sets must be the same four names, and each is read from its own
/// authority rather than typed here:
///
/// * `emulator/set_layer_enabled`'s `layer` enum, parsed out of the vendored fragment;
/// * `emulator/get_layer_states`' `result.required`, parsed out of the other fragment — §11.22 says the
///   setter's enum *is* the getter's key set, and this is that claim discharged by parse;
/// * the key set this server actually answers with, which is generated from the core's `Layer::ALL`.
///
/// Every set is asserted non-empty first: three empty sets are equal, and a schema that failed to parse or a
/// reply that came back `{}` would otherwise sail through as agreement.
#[test]
fn the_mask_vocabulary_is_the_contract_fragments_own() {
    let doc = schema();
    let setter: BTreeSet<String> = set_of(
        doc["methods"]["emulator/set_layer_enabled"]["params"]["properties"]["layer"]["enum"]
            .as_array()
            .expect("the setter fragment declares a `layer` enum")
            .iter()
            .map(|v| v.as_str().expect("enum values are strings").to_string()),
    );
    let getter: BTreeSet<String> = set_of(
        doc["methods"]["emulator/get_layer_states"]["result"]["required"]
            .as_array()
            .expect("the getter fragment declares required keys")
            .iter()
            .map(|v| v.as_str().expect("required names are strings").to_string()),
    );
    assert_eq!(setter.len(), 4, "the setter enum should name four layers");
    assert!(!getter.is_empty(), "the getter fragment required nothing");
    assert_eq!(
        setter, getter,
        "§11.22: the setter's enum IS the getter's key set, and the two fragments have drifted"
    );

    let h = spawn_system("lay-vocab", layered_machine(), 1024);
    let mut c = client(&h);
    let served: BTreeSet<String> = set_of(
        c.ok("emulator/get_layer_states", json!({}))
            .as_object()
            .expect("an object")
            .keys()
            .filter(|k| !ENVELOPE_KEYS.contains(&k.as_str()))
            .cloned(),
    );
    assert_eq!(
        served, setter,
        "the names this server generates from Layer::ALL are not the fragment's"
    );
    assert!(
        !served.contains("backdrop"),
        "the backdrop is a pixel-attribution layer, not a mask target — the fragment says so"
    );

    // **CR6** (§11.49, CR-V). `displayMask` is spelled in the same four names, read from the schema's one
    // `$defs` entry, and that enum equals the core's `LayerMask::targets()` in BOTH directions: a name the
    // core can hide that the enum refuses is a reply the schema rejects, and an enum name the core never
    // emits is a vocabulary nobody serves. One `BTreeSet` equality would say both, but not which one broke,
    // so each direction is its own assertion.
    let defs_enum: BTreeSet<String> = set_of(
        doc["$defs"]["displayMask"]["items"]["enum"]
            .as_array()
            .expect("the schema declares $defs/displayMask with an items enum (§11.49)")
            .iter()
            .map(|v| v.as_str().expect("enum values are strings").to_string()),
    );
    let core: BTreeSet<String> =
        set_of(LayerMask::targets().into_iter().map(|(n, _)| n.to_string()));
    assert_eq!(
        core.len(),
        4,
        "LayerMask::targets() should name four layers"
    );
    let core_not_enum: Vec<&String> = core.difference(&defs_enum).collect();
    let enum_not_core: Vec<&String> = defs_enum.difference(&core).collect();
    assert!(
        core_not_enum.is_empty(),
        "LayerMask::targets() names {core_not_enum:?}, which $defs/displayMask refuses"
    );
    assert!(
        enum_not_core.is_empty(),
        "$defs/displayMask admits {enum_not_core:?}, which LayerMask::targets() never names"
    );
    assert_eq!(
        defs_enum, setter,
        "§11.49: displayMask's enum is set_layer_enabled's `layer` enum"
    );
    // The three fragments that carry the key must point at this one entry, or the enum checked above is
    // not the one their replies are judged by.
    for method in [
        "emulator/state_hash",
        "emulator/screenshot",
        "emulator/scanlines",
    ] {
        assert_eq!(
            doc["methods"][method]["result"]["properties"]["displayMask"]["$ref"],
            json!("#/$defs/displayMask"),
            "{method}'s displayMask must be the one $defs entry this row checks, not a copied enum"
        );
    }
}

/// The four keys the *envelope* stamps on after the handler returns (§2.2 / D11, §2.3 / D17).
const ENVELOPE_KEYS: [&str; 4] = ["frame", "mclk", "running", "droppedEvents"];

// -------------------------------------------------------------------------------------------------
// The round trip
// -------------------------------------------------------------------------------------------------

/// Every layer toggles, the reply reports the state **after** the call, the getter agrees, and toggling one
/// layer leaves the other three exactly where they were.
#[test]
fn every_layer_round_trips_and_leaves_its_neighbours_alone() {
    let h = spawn_system("lay-trip", layered_machine(), 1024);
    let mut c = client(&h);
    let names: Vec<String> = c
        .ok("emulator/get_layer_states", json!({}))
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| !ENVELOPE_KEYS.contains(&k.as_str()))
        .cloned()
        .collect();
    assert_eq!(names.len(), 4, "four mask targets");

    for name in &names {
        let before = c.ok("emulator/get_layer_states", json!({}));
        assert_eq!(before[name], json!(true), "{name} starts drawn");

        let r = c.ok(
            "emulator/set_layer_enabled",
            json!({"layer": name, "enabled": false}),
        );
        assert_eq!(r["layer"], json!(name));
        assert_eq!(
            r["enabled"],
            json!(false),
            "the reply is the state AFTER the call"
        );

        let after = c.ok("emulator/get_layer_states", json!({}));
        assert_eq!(after[name], json!(false), "{name} is now hidden");
        for other in &names {
            if other != name {
                assert_eq!(
                    after[other], before[other],
                    "toggling {name} moved {other} as well"
                );
            }
        }

        let r = c.ok(
            "emulator/set_layer_enabled",
            json!({"layer": name, "enabled": true}),
        );
        assert_eq!(r["enabled"], json!(true));
        assert_eq!(
            c.ok("emulator/get_layer_states", json!({}))[name],
            json!(true),
            "{name} came back"
        );
    }
}

/// A setter call that changes nothing is still a success reporting the true state — an idempotent set is
/// not an error, and the reply must not claim a transition that did not happen.
#[test]
fn setting_a_layer_to_the_state_it_is_already_in_is_a_success() {
    let h = spawn_system("lay-idem", layered_machine(), 1024);
    let mut c = client(&h);
    for _ in 0..2 {
        let r = c.ok(
            "emulator/set_layer_enabled",
            json!({"layer": "window", "enabled": true}),
        );
        assert_eq!(r["enabled"], json!(true));
    }
    assert_eq!(
        c.ok("emulator/get_layer_states", json!({}))["window"],
        json!(true)
    );
}

// -------------------------------------------------------------------------------------------------
// Refusals
// -------------------------------------------------------------------------------------------------

fn message(err: &Value) -> String {
    err["message"].as_str().unwrap_or_default().to_string()
}

/// An unknown `layer` **value** is `-32602` naming the field and listing the accepted set — the house
/// spelling `parse_watch_space` established for the other enum-valued params on this bus.
///
/// `backdrop` is in the list on purpose: it is a real `Layer` in the core and a real value in
/// `pixel_attribution`'s enum, and it is *not* a mask target. `plane_a` is the legacy MCP's snake spelling
/// (the fragment's `$comment` calls it a D-33-class divergence, retired by replacing the server), and
/// `sprite` is `pixel_attribution`'s singular. Each is a plausible guess a real client would make.
#[test]
fn an_unknown_layer_is_refused_naming_the_field_and_the_accepted_set() {
    let h = spawn_system("lay-enum", layered_machine(), 1024);
    let mut c = client(&h);
    for bad in ["backdrop", "plane_a", "planeb", "sprite", "", "PLANEA"] {
        let e = c.err(
            "emulator/set_layer_enabled",
            json!({"layer": bad, "enabled": false}),
        );
        assert_eq!(e["code"], json!(-32602), "{bad:?} must be -32602: {e}");
        let m = message(&e);
        assert!(
            m.contains("`layer`"),
            "{bad:?}: the message must name the field it refused: {m}"
        );
        for name in ["planeA", "planeB", "window", "sprites"] {
            assert!(
                m.contains(name),
                "{bad:?}: the message must list {name} as accepted: {m}"
            );
        }
        assert_eq!(
            e["data"]["accepted"],
            json!(["planeB", "planeA", "window", "sprites"]),
            "{bad:?}: the accepted set must also arrive as a typed array (§2.4 rule 3)"
        );
    }
    // Nothing was applied by any of them.
    let s = c.ok("emulator/get_layer_states", json!({}));
    for name in ["planeA", "planeB", "window", "sprites"] {
        assert_eq!(s[name], json!(true), "a refused set must change nothing");
    }
}

/// A missing or mistyped required param is `-32602`, and `enabled` is never defaulted: a flag quietly read
/// as `false` would turn a malformed request into a layer disappearing.
#[test]
fn a_missing_or_mistyped_param_is_refused() {
    let h = spawn_system("lay-req", layered_machine(), 1024);
    let mut c = client(&h);
    for (params, needle) in [
        (json!({"enabled": false}), "`layer`"),
        (json!({"layer": "planeA"}), "`enabled`"),
        (json!({}), "`layer`"),
        (json!({"layer": 3, "enabled": false}), "`layer`"),
        (json!({"layer": "planeA", "enabled": "false"}), "`enabled`"),
        (json!({"layer": "planeA", "enabled": 0}), "`enabled`"),
        (json!({"layer": null, "enabled": false}), "`layer`"),
        (json!({"layer": "planeA", "enabled": null}), "`enabled`"),
    ] {
        let e = c.err("emulator/set_layer_enabled", params.clone());
        assert_eq!(e["code"], json!(-32602), "{params} must be -32602: {e}");
        assert!(
            message(&e).contains(needle),
            "{params}: the refusal must name {needle}: {}",
            message(&e)
        );
    }
    assert_eq!(
        c.ok("emulator/get_layer_states", json!({}))["planeA"],
        json!(true),
        "a refused set must change nothing"
    );
}

/// An undeclared **key** is refused by §2.5's params closure — a different check from the enum above, and
/// worth pinning here because the two failures are easy to confuse: this one names the key and lists the
/// method's accepted *params*, that one names the field and lists its accepted *values*.
#[test]
fn an_undeclared_param_key_is_refused_by_the_params_closure() {
    let h = spawn_system("lay-closed", layered_machine(), 1024);
    let mut c = client(&h);
    let e = c.err(
        "emulator/set_layer_enabled",
        json!({"layer": "planeA", "enabled": false, "plane": 1}),
    );
    assert_eq!(e["code"], json!(-32602));
    let m = message(&e);
    assert!(m.contains("`plane`"), "must name the offending key: {m}");
    assert!(
        m.contains("accepted params: enabled, layer"),
        "must list the method's params: {m}"
    );
    assert_eq!(e["data"]["unknownParams"], json!(["plane"]));

    let e = c.err("emulator/get_layer_states", json!({"layer": "planeA"}));
    assert_eq!(e["code"], json!(-32602));
    assert!(
        message(&e).contains("none, this method takes no params"),
        "the getter takes no params: {}",
        message(&e)
    );
}

// -------------------------------------------------------------------------------------------------
// A mask is not a blank
// -------------------------------------------------------------------------------------------------

fn winner_at(c: &mut Client, x: u16, y: u16) -> String {
    c.ok("emulator/pixel_attribution", json!({"x": x, "y": y}))["winner"]["layer"]
        .as_str()
        .expect("winner.layer is a string")
        .to_string()
}

fn rgb_at(c: &mut Client, x: u16, y: u16) -> Value {
    c.ok("emulator/pixel_attribution", json!({"x": x, "y": y}))["rgb"].clone()
}

/// **The believable-wrong-answer control, on the wire.** Peeling the stack one layer at a time must walk
/// sprite → plane A → plane B → backdrop, with a *different colour* at every step. A mask implemented as a
/// post-hoc blank would jump straight to the backdrop colour on the first call and would pass every schema
/// check on the way.
#[test]
fn masking_a_layer_reveals_what_is_behind_it() {
    let h = spawn_system("lay-peel", layered_machine(), 1024);
    let mut c = client(&h);

    let sprite_rgb = rgb_at(&mut c, 0, 0);
    assert_eq!(
        winner_at(&mut c, 0, 0),
        "sprite",
        "control: unmasked, the sprite owns (0,0)"
    );

    c.ok(
        "emulator/set_layer_enabled",
        json!({"layer": "sprites", "enabled": false}),
    );
    let a_rgb = rgb_at(&mut c, 0, 0);
    assert_eq!(
        winner_at(&mut c, 0, 0),
        "planeA",
        "sprites hidden → plane A, not the backdrop"
    );

    c.ok(
        "emulator/set_layer_enabled",
        json!({"layer": "planeA", "enabled": false}),
    );
    let b_rgb = rgb_at(&mut c, 0, 0);
    assert_eq!(
        winner_at(&mut c, 0, 0),
        "planeB",
        "sprites + plane A hidden → plane B, not the backdrop"
    );

    c.ok(
        "emulator/set_layer_enabled",
        json!({"layer": "planeB", "enabled": false}),
    );
    let back_rgb = rgb_at(&mut c, 0, 0);
    assert_eq!(
        winner_at(&mut c, 0, 0),
        "backdrop",
        "every maskable layer hidden → the backdrop, where the fall-through ends"
    );

    // Four steps, four different colours. Without this the four `winner` assertions above would still pass
    // against a renderer that reported the right layer and drew the wrong pixel.
    let seen = [&sprite_rgb, &a_rgb, &b_rgb, &back_rgb];
    for (i, x) in seen.iter().enumerate() {
        for (j, y) in seen.iter().enumerate() {
            if i != j {
                assert_ne!(
                    x, y,
                    "steps {i} and {j} of the peel drew the same colour: {x}"
                );
            }
        }
    }
}

/// A masked layer is **absent** from `candidates` rather than carrying a verdict the closed vocabulary has
/// no word for — and no surviving candidate is relabelled on its way out.
#[test]
fn a_masked_layer_is_not_a_candidate() {
    let h = spawn_system("lay-cand", layered_machine(), 1024);
    let mut c = client(&h);
    let full = c.ok("emulator/pixel_attribution", json!({"x": 0, "y": 0}));
    let n = full["candidates"].as_array().unwrap().len();
    assert_eq!(n, 4, "control: sprite + A slot + B + backdrop");

    c.ok(
        "emulator/set_layer_enabled",
        json!({"layer": "sprites", "enabled": false}),
    );
    let masked = c.ok("emulator/pixel_attribution", json!({"x": 0, "y": 0}));
    let cands = masked["candidates"].as_array().unwrap();
    assert_eq!(cands.len(), 3, "the masked layer left the list: {cands:?}");
    assert!(
        cands.iter().all(|c| c["layer"] != json!("sprite")),
        "a masked layer must not appear at all: {cands:?}"
    );
    assert_eq!(
        cands[0]["verdict"],
        json!("won"),
        "the head of the list is the layer that was drawn"
    );
    assert!(
        cands.iter().all(|c| c["verdict"] != json!("operator")),
        "no candidate may be relabelled a sprite operator because a sprite was masked: {cands:?}"
    );
}

// -------------------------------------------------------------------------------------------------
// One mask, every surface
// -------------------------------------------------------------------------------------------------

/// The expected framebuffer for `mask`, computed by the **core** renderer on a machine identical to the
/// one the server was handed.
fn expected_frame(mask: LayerMask) -> (usize, u16, Vec<(u8, u8, u8)>) {
    let sys = layered_machine();
    let (width, height) = sys.vdp().active_display();
    let mut fb = Vec::new();
    for line in 0..height {
        fb.extend_from_slice(&sys.vdp().render_line_masked(line, mask));
    }
    (width as usize, height, fb)
}

/// **`screenshot`, `scanlines` and `pixel_attribution` all show the same one mask.** A mask honoured by one
/// and ignored by another is the plausible partial answer this repo treats as worse than an unimplemented
/// method, and the three are only tied together by asserting all three against one expectation.
///
/// The expectation is the core's own masked render of an identical machine — not a reply read back and
/// re-asserted — so a mask that reached none of the three would fail every branch rather than agreeing with
/// itself.
#[test]
fn one_mask_is_visible_on_every_surface_that_renders() {
    let h = spawn_system("lay-surf", layered_machine(), 1024);
    let mut c = client(&h);
    c.ok(
        "emulator/set_layer_enabled",
        json!({"layer": "planeA", "enabled": false}),
    );
    let mask = mask_without("planeA");
    let (width, height, want) = expected_frame(mask);
    let (_, _, unmasked) = expected_frame(LayerMask::ALL);
    assert_every_layer_is_visible_somewhere();
    assert_ne!(
        want, unmasked,
        "fixture precondition: masking plane A must actually change the picture, or every \
         assertion below is green whether or not the mask reached anything"
    );

    // 1. scanlines — the rows are the masked render, hex for hex.
    let rows = c.ok("emulator/scanlines", json!({"startLine": 0, "count": 8}));
    for (i, row) in rows["rows"].as_array().unwrap().iter().enumerate() {
        let flat: Vec<u8> = want[i * width..(i + 1) * width]
            .iter()
            .flat_map(|&(r, g, b)| [r, g, b])
            .collect();
        assert_eq!(
            row["rgb"],
            json!(oracle_aether::hex::bytes(&flat)),
            "scanlines row {i} is not the masked render"
        );
    }

    // 2. screenshot — the PNG on disk is the masked frame, encoded.
    let shot = c.ok("emulator/screenshot", json!({"path": shot_path("surf")}));
    let path = shot["path"].as_str().expect("a path");
    let bytes = std::fs::read(path).expect("the screenshot exists");
    assert_eq!(
        bytes,
        oracle_aether::png::encode(&want, width as u32, u32::from(height)),
        "the screenshot PNG is not the masked frame"
    );
    assert_eq!(shot["width"], json!(width));
    let _ = std::fs::remove_file(path);

    // 3. pixel_attribution — the dot the mask changed, and one it did not.
    let (mx, my) = (0u16, 0u16);
    let want_px = want[my as usize * width + mx as usize];
    assert_eq!(
        rgb_at(&mut c, mx, my),
        json!({"r": want_px.0, "g": want_px.1, "b": want_px.2}),
        "pixel_attribution disagrees with the masked render at ({mx},{my})"
    );
    let (ux, uy) = (300u16, 200u16); // outside the 32x32 stack: nothing the mask touches
    let untouched = want[uy as usize * width + ux as usize];
    assert_eq!(
        unmasked[uy as usize * width + ux as usize],
        untouched,
        "fixture precondition: ({ux},{uy}) must be a dot the mask does not change"
    );
    assert_eq!(
        rgb_at(&mut c, ux, uy),
        json!({"r": untouched.0, "g": untouched.1, "b": untouched.2}),
        "the mask changed a dot it had no business changing"
    );
}

/// A masked read cannot use the latched raster frame (it was drawn unmasked), so it says so — `source:
/// "stateRender"` and a caveat that names the mask instead of claiming no frame has been drawn.
///
/// The run first is load-bearing: without it the fallback is taken for the ordinary reason and the test
/// would pass with the mask wired to nothing.
#[test]
fn a_masked_read_declares_that_it_is_not_the_raster_frame() {
    let h = spawn_system("lay-cav", layered_machine(), 1024);
    let mut c = client(&h);
    c.ok("emulator/run_frames", json!({"frames": 2}));

    let shot = c.ok("emulator/screenshot", json!({"path": shot_path("cav-a")}));
    assert_eq!(
        shot["source"],
        json!("raster"),
        "control: after a run, an unmasked capture is the frame the raster drew"
    );
    assert!(
        shot.get("caveat").is_none(),
        "control: a raster capture carries no caveat: {shot}"
    );
    assert_eq!(
        shot["displayMask"],
        json!([]),
        "control: an unmasked capture hides nothing and says so in the typed key (§11.49 item B): {shot}"
    );
    let _ = std::fs::remove_file(shot["path"].as_str().unwrap());

    c.ok(
        "emulator/set_layer_enabled",
        json!({"layer": "sprites", "enabled": false}),
    );
    for (method, params) in [
        ("emulator/screenshot", json!({"path": shot_path("cav-b")})),
        ("emulator/scanlines", json!({"startLine": 0, "count": 1})),
    ] {
        let r = c.ok(method, params);
        assert_eq!(
            r["source"],
            json!("stateRender"),
            "{method}: a masked read is not the raster frame"
        );
        // The typed half (§11.49 item B), and since that ruling the half that is contract: the key names
        // the one hidden layer. The caveat assertions below stay, as this server's informative behaviour
        // (§2.4 rule 3); the key does not replace them, it is what a client may branch on.
        assert_eq!(
            r["displayMask"],
            json!(["sprites"]),
            "{method}: displayMask must name the hidden layer: {r}"
        );
        let caveat = r["caveat"].as_str().unwrap_or_else(|| {
            panic!("{method}: a masked read must carry a caveat: {r}");
        });
        assert!(
            caveat.contains("mask") && caveat.contains("sprites"),
            "{method}: the caveat must say a mask is active and which layer: {caveat}"
        );
        assert!(
            !caveat.contains("has not drawn one yet") && !caveat.contains("no whole frame"),
            "{method}: the caveat must not blame a frame that WAS drawn: {caveat}"
        );
        if let Some(p) = r["path"].as_str() {
            let _ = std::fs::remove_file(p);
        }
    }

    // Clearing the mask puts the raster frame back — nothing was thrown away.
    c.ok(
        "emulator/set_layer_enabled",
        json!({"layer": "sprites", "enabled": true}),
    );
    let back = c.ok("emulator/screenshot", json!({"path": shot_path("cav-c")}));
    assert_eq!(
        back["source"],
        json!("raster"),
        "clearing the mask restores the latched raster frame"
    );
    assert_eq!(
        back["displayMask"],
        json!([]),
        "clearing the mask empties the key: {back}"
    );
    let _ = std::fs::remove_file(back["path"].as_str().unwrap());
}

// -------------------------------------------------------------------------------------------------
// The mask is not machine state
// -------------------------------------------------------------------------------------------------

/// ⚑ **The mask must not poison a hash.** `emulator/state_hash` is a determinism fingerprint of the
/// machine; `includeFramebuffer` extends it to the picture the machine drew. A debug layer toggle moving
/// either would make two identical machines disagree for a reason that has nothing to do with either.
///
/// The framebuffer digest is the sharp half — it is the one hash with a rendering in it — so the run first
/// is deliberate, and the `stateRender`-vs-`raster` provenance is asserted too: a digest that agreed only
/// because both sides fell back to the same post-hoc render would be agreement for the wrong reason.
#[test]
fn the_mask_is_not_machine_state() {
    let h = spawn_system("lay-hash", layered_machine(), 1024);
    let mut c = client(&h);
    c.ok("emulator/run_frames", json!({"frames": 2}));

    let before = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
    let mem_before = c.ok(
        "emulator/memory_hash",
        json!({"addr": "0x00FF0000", "len": 4096}),
    );
    assert_eq!(
        before["framebufferSource"],
        json!("raster"),
        "control: the digest below must be of the raster frame, not of a fallback render"
    );

    for name in ["planeA", "planeB", "window", "sprites"] {
        c.ok(
            "emulator/set_layer_enabled",
            json!({"layer": name, "enabled": false}),
        );
    }
    // The mask really is on — otherwise the comparison below is two identical unmasked machines.
    let shot = c.ok("emulator/screenshot", json!({"path": shot_path("hash")}));
    assert_eq!(
        shot["source"],
        json!("stateRender"),
        "precondition: the mask must be visible to the render surfaces"
    );
    let _ = std::fs::remove_file(shot["path"].as_str().unwrap());

    let after = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
    for key in [
        "vram",
        "cram",
        "vsram",
        "regs",
        "combined",
        "framebuffer",
        "framebufferSource",
    ] {
        assert!(
            before[key].is_string(),
            "control: state_hash must actually report `{key}`"
        );
        assert_eq!(
            after[key], before[key],
            "a display mask moved state_hash.{key} — the debugger's state has entered the machine's \
             fingerprint"
        );
    }
    // §11.49: the digest did not move, and the same reply says the screen is hiding all four layers.
    assert_eq!(
        before["displayMask"],
        json!([]),
        "control: nothing was hidden when the first digest was taken: {before}"
    );
    assert_eq!(
        names(&after["displayMask"], "state_hash under a four-layer mask"),
        set_of(LayerMask::targets().into_iter().map(|(n, _)| n.to_string())),
        "displayMask must name every hidden layer: {after}"
    );
    assert_eq!(
        c.ok(
            "emulator/memory_hash",
            json!({"addr": "0x00FF0000", "len": 4096})
        )["fnv1a64"],
        mem_before["fnv1a64"],
        "a display mask moved memory_hash — it cannot see memory at all"
    );
}

// -------------------------------------------------------------------------------------------------
// §11.49 (CR-V): `displayMask`, the typed key for "the screen is not the picture this digest is of"
// -------------------------------------------------------------------------------------------------

/// The five fingerprint keys `emulator/state_hash` always answers, and the keys a `framebuffer` brings
/// with it, both **read from the vendored fragment**: `result.required`, and `framebuffer` plus
/// `dependentRequired.framebuffer` (§11.49 D1 with R1). Loud if either is missing, so a schema that
/// stopped declaring them cannot turn the key-set assertions below into comparisons against nothing.
fn state_hash_key_sets() -> (BTreeSet<String>, BTreeSet<String>) {
    let doc = schema();
    let frag = &doc["methods"]["emulator/state_hash"]["result"];
    let strings = |v: &Value, what: &str| -> BTreeSet<String> {
        set_of(
            v.as_array()
                .unwrap_or_else(|| panic!("the state_hash fragment declares no {what}"))
                .iter()
                .map(|s| s.as_str().expect("key names are strings").to_string()),
        )
    };
    let five = strings(&frag["required"], "result.required");
    assert_eq!(
        five.len(),
        5,
        "state_hash's required keys should be the five fingerprints: {five:?}"
    );
    let mut with_fb = strings(
        &frag["dependentRequired"]["framebuffer"],
        "dependentRequired.framebuffer",
    );
    with_fb.insert("framebuffer".to_string());
    assert!(
        with_fb.contains("displayMask") && with_fb.contains("framebufferSource"),
        "§11.49 D1 and R1: displayMask and framebufferSource are demanded beside framebuffer: {with_fb:?}"
    );
    (five, with_fb)
}

/// A reply's own keys, the envelope's four subtracted.
fn method_keys(r: &Value) -> BTreeSet<String> {
    set_of(
        r.as_object()
            .expect("an object")
            .keys()
            .filter(|k| !ENVELOPE_KEYS.contains(&k.as_str()))
            .cloned(),
    )
}

/// A `displayMask` as a set, **after** asserting it is an array of strings with no name twice (the
/// fragment's `uniqueItems`, asserted here too so a set comparison cannot hide a duplicate).
fn names(v: &Value, what: &str) -> BTreeSet<String> {
    let arr = v
        .as_array()
        .unwrap_or_else(|| panic!("{what}: displayMask must be an array, got {v}"));
    let set = set_of(arr.iter().map(|s| {
        s.as_str()
            .unwrap_or_else(|| panic!("{what}: displayMask names are strings: {v}"))
            .to_string()
    }));
    assert_eq!(
        set.len(),
        arr.len(),
        "{what}: displayMask lists a layer twice: {v}"
    );
    set
}

/// **CR1 and CR4** (§11.49, CR-V, D1 and D3). The unmasked framebuffer reply carries `displayMask: []`
/// and no caveat; a reply with no framebuffer carries neither key, mask or no mask; and a set that
/// changes nothing changes nothing.
///
/// **What else could make this green?** A server that never emitted `displayMask` would pass both
/// no-framebuffer halves, so the framebuffer half asserts the key as a present, EMPTY array rather than
/// the absence of a non-empty one. A key set compared against a list typed here would pass against a
/// stale list, so both sets come out of the vendored fragment ([`state_hash_key_sets`]). And the masked
/// no-framebuffer reply would prove nothing if the mask were not in force when it was taken, so a
/// framebuffer reply at the same point is asserted to name the hidden layer first.
#[test]
fn an_unmasked_framebuffer_hash_carries_an_empty_display_mask_and_no_constant_caveat() {
    let (five, with_fb) = state_hash_key_sets();
    let all: BTreeSet<String> = five.union(&with_fb).cloned().collect();
    let h = spawn_system("lay-hbase", layered_machine(), 1024);
    let mut c = client(&h);
    c.ok("emulator/run_frames", json!({"frames": 2}));

    // CR4, unmasked: the five fingerprints and nothing else. No caveat (M18), no displayMask.
    let plain = c.ok("emulator/state_hash", json!({}));
    assert_eq!(
        method_keys(&plain),
        five,
        "CR4: state_hash {{}} is the five fingerprints and nothing else, no caveat and no displayMask: {plain}"
    );

    // CR1: a framebuffer brings exactly `framebufferSource` and `displayMask` with it, the list present
    // and EMPTY, and no caveat.
    let a = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
    assert_eq!(
        method_keys(&a),
        all,
        "CR1: a framebuffer reply is the five plus exactly what the fragment demands beside framebuffer: {a}"
    );
    assert_eq!(
        a["displayMask"],
        json!([]),
        "CR1: nothing is hidden, and the key says so rather than being absent: {a}"
    );
    assert!(a.get("caveat").is_none(), "CR1: no mask, no caveat: {a}");

    // A set that changes nothing must change nothing, displayMask included.
    c.ok(
        "emulator/set_layer_enabled",
        json!({"layer": "planeA", "enabled": true}),
    );
    let b = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
    for key in &all {
        assert!(
            a.get(key).is_some(),
            "control: state_hash must actually report `{key}`, or the comparison below is vacuous"
        );
        assert_eq!(
            a[key], b[key],
            "a no-op set_layer_enabled moved state_hash.{key}"
        );
    }

    // CR4, masked: a mask IS set, but nothing was hashed, so there is no unmasked picture to disclaim.
    c.ok(
        "emulator/set_layer_enabled",
        json!({"layer": "window", "enabled": false}),
    );
    let masked_fb = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
    assert_eq!(
        masked_fb["displayMask"],
        json!(["window"]),
        "precondition: the mask is in force at this point, or the reply below proves nothing: {masked_fb}"
    );
    let masked_plain = c.ok("emulator/state_hash", json!({}));
    assert_eq!(
        method_keys(&masked_plain),
        five,
        "CR4: with a mask set and no framebuffer, still the five and nothing else: {masked_plain}"
    );
}

/// **CR2 and CR3** (§11.49, CR-V). Under a mask the digest does not move, and `displayMask` names exactly
/// the hidden layers. The reference server also puts a caveat beside it (CR3), which §11.49 S2 keeps
/// informative: it pins this server's behaviour, not the contract's.
///
/// **What else could make this green?** A digest that agreed only because the mask never reached any
/// picture, so the mask is shown to reach both render surfaces first (the anti-vacuity precondition kept
/// from the row this replaces). A list that names every layer, so the expectation is the two layers set
/// here and the comparison is set EQUALITY, which also refuses the two left shown. And a key or caveat
/// that stuck after the mask was cleared, so the mask is cleared and both are asserted gone.
#[test]
fn a_framebuffer_hash_taken_under_a_mask_names_the_hidden_layers_in_a_typed_key() {
    let (five, _) = state_hash_key_sets();
    let h = spawn_system("lay-hcav", layered_machine(), 1024);
    let mut c = client(&h);
    c.ok("emulator/run_frames", json!({"frames": 2}));

    let before = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
    let rows_before = c.ok("emulator/scanlines", json!({}))["rows"].clone();
    assert_eq!(
        before["framebufferSource"],
        json!("raster"),
        "control: the digest must be of the raster frame, not of a fallback render"
    );
    assert!(
        before.get("caveat").is_none(),
        "CR3 control: no mask, no caveat: {before}"
    );

    let hide = ["planeA", "sprites"];
    for layer in hide {
        c.ok(
            "emulator/set_layer_enabled",
            json!({"layer": layer, "enabled": false}),
        );
    }

    // Anti-vacuity: the mask reached the picture on both render surfaces. Otherwise there is no
    // divergence to name and this row describes a situation that is not happening.
    let shot = c.ok("emulator/screenshot", json!({"path": shot_path("hcav")}));
    assert_eq!(
        shot["source"],
        json!("stateRender"),
        "precondition: the mask must be visible to the render surfaces"
    );
    let _ = std::fs::remove_file(shot["path"].as_str().unwrap());
    let rows_after = c.ok("emulator/scanlines", json!({}))["rows"].clone();
    assert_ne!(
        rows_after, rows_before,
        "precondition: the mask must change the rows emulator/scanlines serves"
    );

    let after = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));

    // CR2 (1): nothing the digests cover moved. The key is a disclosure, not a substitute for the rule.
    for key in five
        .iter()
        .map(String::as_str)
        .chain(["framebuffer", "framebufferSource"])
    {
        assert!(
            before[key].is_string(),
            "control: state_hash must report `{key}`"
        );
        assert_eq!(
            after[key], before[key],
            "a display mask moved state_hash.{key}: the debugger's state has entered the machine's fingerprint"
        );
    }

    // CR2 (2): displayMask names exactly the hidden layers.
    assert_eq!(
        names(&after["displayMask"], "masked state_hash"),
        set_of(hide.iter().map(|s| s.to_string())),
        "CR2: displayMask must name exactly the layers hidden, no more and no fewer: {after}"
    );

    // CR3, informative (§11.49 S2): the reference server's human twin of the key.
    let caveat = after["caveat"].as_str().unwrap_or_else(|| {
        panic!("CR3: the reference server emits a caveat beside a non-empty displayMask: {after}")
    });
    for layer in hide {
        assert!(
            caveat.contains(layer),
            "CR3: the caveat must name the hidden layer {layer}: {caveat}"
        );
    }
    for (shown, _) in LayerMask::targets()
        .into_iter()
        .filter(|(n, _)| !hide.contains(n))
    {
        assert!(
            !caveat.contains(shown),
            "CR3: the caveat named {shown}, which is NOT hidden: {caveat}"
        );
    }
    assert!(
        caveat.contains("UNMASKED"),
        "CR3: the caveat must say which picture the hash is of: {caveat}"
    );

    // Cleared: the same digest, an empty key, and no caveat. Both follow the mask in both directions.
    for layer in hide {
        c.ok(
            "emulator/set_layer_enabled",
            json!({"layer": layer, "enabled": true}),
        );
    }
    let cleared = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
    assert_eq!(cleared["framebuffer"], before["framebuffer"]);
    assert_eq!(
        cleared["displayMask"],
        json!([]),
        "clearing the mask empties the key: {cleared}"
    );
    assert!(
        cleared.get("caveat").is_none(),
        "clearing the mask retires the caveat: {cleared}"
    );
}

/// **§8 item 30**, the recipe the contract hands a client from outside the server (§11.49): hash with
/// `includeFramebuffer`, hide one layer, hash again, restore, hash a third time. All three digests are
/// equal and the second reply's `displayMask` is that one layer. Run here for **every** mask target, each
/// with its own anti-vacuity: between the first two hashes `emulator/scanlines` must differ, or the
/// fixture draws nothing on that layer and the row proves nothing about it.
#[test]
fn section_8_item_30_each_layer_hidden_alone_leaves_the_digest_where_it_was() {
    assert_every_layer_is_visible_somewhere();
    let h = spawn_system("lay-item30", layered_machine(), 1024);
    let mut c = client(&h);
    c.ok("emulator/run_frames", json!({"frames": 2}));
    let targets = LayerMask::targets();
    assert_eq!(targets.len(), 4, "the sweep should cover four mask targets");
    for (name, _) in targets {
        let first = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
        let rows1 = c.ok("emulator/scanlines", json!({}))["rows"].clone();
        c.ok(
            "emulator/set_layer_enabled",
            json!({"layer": name, "enabled": false}),
        );
        let second = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
        let rows2 = c.ok("emulator/scanlines", json!({}))["rows"].clone();
        assert_ne!(
            rows1, rows2,
            "anti-vacuity: hiding {name} must change what emulator/scanlines serves"
        );
        c.ok(
            "emulator/set_layer_enabled",
            json!({"layer": name, "enabled": true}),
        );
        let third = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
        for key in ["framebuffer", "framebufferSource"] {
            assert!(
                first[key].is_string(),
                "control: state_hash must report `{key}`"
            );
            assert_eq!(
                second[key], first[key],
                "hiding {name} moved state_hash.{key}"
            );
            assert_eq!(
                third[key], first[key],
                "restoring {name} moved state_hash.{key}"
            );
        }
        assert_eq!(
            first["displayMask"],
            json!([]),
            "{name}: before hiding: {first}"
        );
        assert_eq!(
            second["displayMask"],
            json!([name]),
            "{name}: the second reply's displayMask is that one layer: {second}"
        );
        assert_eq!(
            third["displayMask"],
            json!([]),
            "{name}: after restoring: {third}"
        );
    }
}

/// **CR7, player parity** (§11.49, CR-V). The player's palette hides a layer through `Host::set_layer`,
/// a one-line forward to `Engine::set_layer` (the same field `emulator/set_layer_enabled` moves), and
/// draws its standing badge from `LayerMask::hidden()`. The wire's `displayMask` on all three methods must
/// be that list, as served, over every one of the sixteen masks: one derivation, asserted so it stays one.
///
/// **What else could make this green?** Agreement only because both lists were empty, so all sixteen
/// masks are swept and fifteen are counted non-empty. A second hand-written list holding the same names
/// in another order, which is set-equal and still two derivations, so the comparison is on the list as
/// served, order included. And a wire list equal to a `hidden()` that itself disagreed with what was
/// asked, so `hidden()` is first held set-equal to the layers this loop switched off.
#[test]
fn cr7_a_mask_set_from_the_palette_is_the_wire_display_mask_on_every_surface() {
    let mut host = Host::new(HostConfig::default());
    let mut sys = layered_machine();
    let targets = LayerMask::targets();
    assert_eq!(targets.len(), 4, "the sweep should cover four mask targets");
    let path = shot_path("cr7");
    let mut nonempty = 0;
    for bits in 0u32..(1 << targets.len()) {
        let mut want = BTreeSet::new();
        for (i, (name, layer)) in targets.iter().enumerate() {
            let hide = bits & (1 << i) != 0;
            assert!(host.set_layer(*layer, !hide), "{name} is a mask target");
            if hide {
                want.insert(name.to_string());
            }
        }
        let badge = host.layers().hidden();
        assert_eq!(
            set_of(badge.iter().map(|s| s.to_string())),
            want,
            "the palette route hid {want:?} and the badge's source reads {badge:?}"
        );
        if !badge.is_empty() {
            nonempty += 1;
        }
        for (method, params) in [
            ("emulator/state_hash", json!({"includeFramebuffer": true})),
            ("emulator/screenshot", json!({"path": path})),
            ("emulator/scanlines", json!({"startLine": 0, "count": 1})),
        ] {
            let (res, _stamp) = host.call(&mut sys, method, &params);
            let r = res.unwrap_or_else(|e| panic!("{method} under mask {want:?}: {e:?}"));
            assert_eq!(
                r["displayMask"],
                json!(badge),
                "{method}: the wire's displayMask is not LayerMask::hidden() under {want:?}, so the \
                 two are not one derivation"
            );
        }
    }
    assert_eq!(nonempty, 15, "the sweep must exercise every non-empty mask");
    let _ = std::fs::remove_file(&path);
}

/// [`layered_machine`] in H32 (reg 12 = `$00`): the same stack, 256 pixels wide, for CR8's second width
/// (§11.49 M1). Nothing else changes, and CR8 re-measures its own precondition on this machine (the mask
/// must hide drawn content) rather than inheriting H40's.
fn layered_machine_h32() -> System {
    let mut sys = layered_machine();
    set_reg(sys.vdp_mut(), 0x0C, 0x00);
    sys
}

/// `oracle_core::testrom::build_cram_midframe(100)` booted: an H32 frame whose backdrop is repainted at
/// line 100, so its raster differs from any post-hoc render of the end-of-frame state.
fn midframe_machine() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build_cram_midframe(100));
    sys.reset();
    sys
}

/// FNV-1a 64-bit with the parameters `emulator/memory_hash` states (§6), written out here rather than
/// borrowed from `oracle_core::state_hash`: the fold under test is the server's, and a test that reused its
/// function would agree with it by construction. The published `"foobar"` vector pins the algorithm.
fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xCBF2_9CE4_8422_2325u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01B3)
    })
}

fn hash_hex(v: u64) -> String {
    format!("0x{v:016X}")
}

/// `emulator/scanlines`' active-display rows, decoded and concatenated: the input §11.49 R2 says
/// `framebuffer` folds (M1). The range is the one `scanlines` serves for `{}` ("through line 223", and
/// whatever an amendment raising that bound makes it), checked against the core constant `scanlines` is
/// bounded by and never against a typed `224`. Each row's width is checked against the reply's own
/// `mode`, which is the width after `scanlines`' normalization.
fn active_display_bytes(r: &Value, what: &str) -> (Vec<u8>, usize) {
    let lines = usize::from(oracle_core::vdp::ACTIVE_LINES);
    let rows = r["rows"].as_array().expect("rows is an array");
    assert_eq!(
        r["startLine"],
        json!(0),
        "{what}: scanlines {{}} must start at line 0"
    );
    assert_eq!(
        rows.len(),
        lines,
        "{what}: scanlines {{}} must serve the whole active display (oracle_core::vdp::ACTIVE_LINES = \
         {lines}); the fold cannot be measured over a stripe"
    );
    let width = match r["mode"].as_str() {
        Some("h40") => usize::from(oracle_core::render::active_width(true)),
        Some("h32") => usize::from(oracle_core::render::active_width(false)),
        m => panic!("{what}: mode must be h40 or h32, got {m:?}"),
    };
    let mut out = Vec::with_capacity(lines * width * 3);
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row["line"], json!(i), "{what}: rows are contiguous from 0");
        assert_eq!(
            row["width"],
            json!(width),
            "{what}: row {i} is not at the reply's mode width"
        );
        let hex = row["rgb"]
            .as_str()
            .and_then(|s| s.strip_prefix("0x"))
            .expect("rgb is 0x-prefixed hex");
        assert_eq!(
            hex.len(),
            width * 6,
            "{what}: row {i} is not width x 3 bytes"
        );
        for k in (0..hex.len()).step_by(2) {
            out.push(u8::from_str_radix(&hex[k..k + 2], 16).expect("hex digits"));
        }
    }
    (out, width)
}

/// The core's own render of `sys` under `mask`, flattened to `r,g,b` bytes: what an identical machine
/// draws, computed without the server.
fn core_frame_bytes(sys: &System, mask: LayerMask) -> Vec<u8> {
    let (_, height) = sys.vdp().active_display();
    (0..height)
        .flat_map(|line| sys.vdp().render_line_masked(line, mask))
        .flat_map(|(r, g, b)| [r, g, b])
        .collect()
}

fn first_difference(a: &[u8], b: &[u8]) -> Option<usize> {
    a.iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .or(if a.len() == b.len() {
            None
        } else {
            Some(a.len().min(b.len()))
        })
}

/// CR8's anti-vacuity half: a constructor for a fresh machine identical to the one served, and the layers
/// to hide on it.
type MaskedHalf<'a> = (fn() -> System, &'a [&'a str]);

/// **CR8, the row that decides whether §11.49 R2 is true** (M1). With no mask and both sources `raster`,
/// FNV-1a-64 over `scanlines`' active-display rows, decoded and concatenated, equals `framebuffer`. With
/// `masked = Some((fresh, hide))`, the same fold over the rows served under a mask that hides drawn
/// content differs from `framebuffer` (the anti-vacuity half), and equals the fold of the core's own
/// masked render of `fresh()`, so the difference is the mask and nothing else. Returns the unmasked
/// `scanlines` reply for a caller with a further claim about the frame.
///
/// If this fails, R2 is withdrawn by a delta ruling (§11.49 M1). The message carries what differs.
fn cr8(
    tag: &str,
    sys: System,
    frames: u64,
    want_mode: &str,
    masked: Option<MaskedHalf<'_>>,
) -> Value {
    assert_eq!(
        fnv1a64(b"foobar"),
        0x8594_4171_F739_67E8,
        "the test's own FNV-1a-64 is the published algorithm"
    );
    let h = spawn_system(tag, sys, 1024);
    let mut c = client(&h);
    c.ok("emulator/run_frames", json!({"frames": frames}));
    let hash = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
    let s = c.ok("emulator/scanlines", json!({}));
    assert_eq!(
        hash["framebufferSource"],
        json!("raster"),
        "{tag}: CR8 is stated for raster sources"
    );
    assert_eq!(
        s["source"],
        json!("raster"),
        "{tag}: CR8 is stated for raster sources"
    );
    assert_eq!(
        s["displayMask"],
        json!([]),
        "{tag}: the unmasked half has no mask"
    );
    assert_eq!(
        s["mode"],
        json!(want_mode),
        "{tag}: the fixture must be the width this row claims to measure"
    );
    let (bytes, width) = active_display_bytes(&s, tag);
    let fold = hash_hex(fnv1a64(&bytes));
    assert_eq!(
        json!(fold),
        hash["framebuffer"],
        "{tag}: §11.49 R2 does not hold. FNV-1a-64 over scanlines' {} rows at the {want_mode} width {width} \
         ({} bytes) is {fold}, and framebuffer is {}",
        s["rows"].as_array().map_or(0, Vec::len),
        bytes.len(),
        hash["framebuffer"]
    );

    let Some((fresh, hide)) = masked else {
        return s;
    };
    let mut mask = LayerMask::ALL;
    for name in hide {
        let (_, layer) = LayerMask::targets()
            .into_iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("{name} is not a mask target"));
        mask.set(layer, false);
    }
    let core_masked = core_frame_bytes(&fresh(), mask);
    assert_ne!(
        hash_hex(fnv1a64(&core_masked)),
        hash_hex(fnv1a64(&core_frame_bytes(&fresh(), LayerMask::ALL))),
        "{tag}: precondition: hiding {hide:?} must hide drawn content on this fixture"
    );
    for name in hide {
        c.ok(
            "emulator/set_layer_enabled",
            json!({"layer": name, "enabled": false}),
        );
    }
    let s2 = c.ok("emulator/scanlines", json!({}));
    assert_eq!(
        names(&s2["displayMask"], tag),
        set_of(hide.iter().map(|n| n.to_string())),
        "{tag}: the masked rows must say which layers they hide"
    );
    let (masked_bytes, _) = active_display_bytes(&s2, tag);
    let masked_fold = hash_hex(fnv1a64(&masked_bytes));
    assert_ne!(
        json!(masked_fold),
        hash["framebuffer"],
        "{tag}: anti-vacuity: the fold over rows served under a mask hiding {hide:?} equals framebuffer, so \
         the equality above cannot tell the masked picture from the unmasked one"
    );
    assert!(
        masked_bytes == core_masked,
        "{tag}: the masked rows are not the core's masked render (first difference at byte {:?} of {} vs {}), \
         so the difference above may be something other than the mask",
        first_difference(&masked_bytes, &core_masked),
        masked_bytes.len(),
        core_masked.len()
    );
    let hash2 = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
    assert_eq!(
        hash2["framebuffer"], hash["framebuffer"],
        "{tag}: the digest moved under the mask"
    );
    s
}

#[test]
fn cr8_the_framebuffer_digest_is_the_fold_of_the_scanlines_rows_at_h40() {
    cr8(
        "lay-cr8-h40",
        layered_machine(),
        2,
        "h40",
        Some((layered_machine, &["planeA", "sprites"])),
    );
}

#[test]
fn cr8_the_framebuffer_digest_is_the_fold_of_the_scanlines_rows_at_h32() {
    cr8(
        "lay-cr8-h32",
        layered_machine_h32(),
        2,
        "h32",
        Some((layered_machine_h32, &["planeA"])),
    );
}

/// CR8's equality half on a frame no post-hoc render could have drawn: an H32 raster whose backdrop was
/// repainted part-way down. Without this, the equality could hold only because both sides happened to be
/// the same whole-frame render of a static scene.
#[test]
fn cr8_holds_on_a_mid_frame_h32_raster() {
    let s = cr8("lay-cr8-mid", midframe_machine(), 6, "h32", None);
    let rows = s["rows"].as_array().expect("rows");
    assert_ne!(
        rows[40]["rgb"], rows[160]["rgb"],
        "precondition: the frame must carry its mid-frame backdrop change (line 100), or this row is the \
         static case again"
    );
}

/// **CR9** (§11.49 item B, M4). `screenshot`, `scanlines` and `state_hash` report one `displayMask` at one
/// machine point, `[]` when nothing is hidden. The first two applied it to their picture and the third
/// did not; the key means the same thing on all three (the debugger's mask at reply time).
#[test]
fn cr9_screenshot_scanlines_and_state_hash_report_one_display_mask() {
    let h = spawn_system("lay-cr9", layered_machine(), 1024);
    let mut c = client(&h);
    c.ok("emulator/run_frames", json!({"frames": 2}));
    let path = shot_path("cr9");
    let read = |c: &mut Client| -> Vec<(&'static str, Value)> {
        vec![
            (
                "emulator/state_hash",
                c.ok("emulator/state_hash", json!({"includeFramebuffer": true})),
            ),
            (
                "emulator/screenshot",
                c.ok("emulator/screenshot", json!({"path": path})),
            ),
            (
                "emulator/scanlines",
                c.ok("emulator/scanlines", json!({"startLine": 0, "count": 1})),
            ),
        ]
    };
    for (method, r) in read(&mut c) {
        assert_eq!(
            r["displayMask"],
            json!([]),
            "{method}: unmasked, the key is present and empty: {r}"
        );
    }
    let hide = ["planeB", "window"];
    for layer in hide {
        c.ok(
            "emulator/set_layer_enabled",
            json!({"layer": layer, "enabled": false}),
        );
    }
    let replies = read(&mut c);
    for (method, r) in &replies {
        assert_eq!(
            names(&r["displayMask"], method),
            set_of(hide.iter().map(|s| s.to_string())),
            "{method}: displayMask must name exactly the hidden layers: {r}"
        );
    }
    for pair in replies.windows(2) {
        assert_eq!(
            pair[0].1["displayMask"], pair[1].1["displayMask"],
            "{} and {} report different lists at one machine point",
            pair[0].0, pair[1].0
        );
    }
    let _ = std::fs::remove_file(&path);
}

/// The other direction: the three calls that replace the machine must **not** take the debugger's masks
/// with them. A session that silently lost its masks across a restore is a real failure and a quiet one.
#[test]
fn the_mask_survives_reset_reload_rom_and_restore() {
    let h = spawn_system("lay-keep", layered_machine(), 1024);
    let mut c = client(&h);

    let rom = std::env::temp_dir().join(format!("lay-keep-{}.bin", std::process::id()));
    std::fs::write(&rom, oracle_core::testrom::build()).expect("write the fixture ROM");

    c.ok(
        "emulator/set_layer_enabled",
        json!({"layer": "planeB", "enabled": false}),
    );
    c.ok(
        "emulator/set_layer_enabled",
        json!({"layer": "window", "enabled": false}),
    );
    let want = json!({"planeA": true, "planeB": false, "window": false, "sprites": true});

    let cp = c.ok("emulator/checkpoint", json!({}));
    let id = cp["id"].as_str().expect("a checkpoint handle").to_string();

    for (label, method, params) in [
        ("reset", "emulator/reset", json!({})),
        (
            "reload_rom",
            "emulator/reload_rom",
            json!({"path": rom.display().to_string()}),
        ),
        ("restore", "emulator/restore", json!({"id": id})),
    ] {
        c.ok(method, params);
        let s = c.ok("emulator/get_layer_states", json!({}));
        for name in ["planeA", "planeB", "window", "sprites"] {
            assert_eq!(
                s[name], want[name],
                "{label} changed the mask on {name} — a debugging session must not lose its masks \
                 when the timeline jumps"
            );
        }
    }
    let _ = std::fs::remove_file(&rom);
}

/// The mask is engine state, so it is **shared by every connection** to this server, exactly as the held
/// pad set and the checkpoints are. Pinned because the alternative — per-connection masks — is a plausible
/// reading that would make two clients disagree about what is on the glass.
#[test]
fn the_mask_is_the_servers_not_the_connections() {
    let h = spawn_system("lay-share", layered_machine(), 1024);
    let mut a = client(&h);
    let mut b = client(&h);
    a.ok(
        "emulator/set_layer_enabled",
        json!({"layer": "sprites", "enabled": false}),
    );
    assert_eq!(
        b.ok("emulator/get_layer_states", json!({}))["sprites"],
        json!(false),
        "a second connection sees the same mask"
    );
    assert_eq!(winner_at(&mut b, 0, 0), "planeA", "…and the same picture");
}

/// **The currency control.** With no mask set, every render surface must answer byte-for-byte what it
/// answered before this feature existed — the all-on mask is the same code path, not a parallel one.
#[test]
fn an_unmasked_server_renders_exactly_the_unmasked_picture() {
    // The equality below is only evidence if the picture could have differed: assert first that every one
    // of the four layers is visible somewhere, so a server passing the wrong mask would be caught.
    assert_every_layer_is_visible_somewhere();
    let h = spawn_system("lay-zero", layered_machine(), 1024);
    let mut c = client(&h);
    let (width, height, want) = expected_frame(LayerMask::ALL);

    let shot = c.ok("emulator/screenshot", json!({"path": shot_path("zero")}));
    assert_eq!(
        shot["source"],
        json!("stateRender"),
        "no frame drawn yet, so this is the post-hoc path — the one the mask shares"
    );
    let bytes = std::fs::read(shot["path"].as_str().unwrap()).expect("the screenshot exists");
    assert_eq!(
        bytes,
        oracle_aether::png::encode(&want, width as u32, u32::from(height)),
        "an unmasked capture is not the unmasked render"
    );
    let _ = std::fs::remove_file(shot["path"].as_str().unwrap());

    let px = want[0];
    assert_eq!(
        rgb_at(&mut c, 0, 0),
        json!({"r": px.0, "g": px.1, "b": px.2})
    );
}
