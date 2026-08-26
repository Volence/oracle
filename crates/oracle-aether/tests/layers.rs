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

    // A 4x4-cell block of each plane at the top-left, so a whole 32x32 px corner is layered.
    for row in 0..4u16 {
        for col in 0..4u16 {
            let off = (row * 32 + col) * 2;
            write_vram(v, 0xE000 + off, &[TILE_B]);
            write_vram(v, 0xC000 + off, &[TILE_A]);
        }
    }
    // One 4x4-cell sprite (32x32 px) at screen (0,0), every cell the same solid pattern; link 0 ends the
    // walk. Y/X fields carry the +128 screen offset.
    write_vram(
        v,
        SAT_BASE,
        &[
            128, 0x0F00, // size: (4-1)<<2 | (4-1) = $0F in the high byte; link 0
            TILE_S, 128,
        ],
    );
    sys
}

fn client(handle: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(handle);
    c.handshake(false);
    c
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
        message(&e).contains("none — this method takes no params"),
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
    let shot = c.ok("emulator/screenshot", json!({}));
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

    let shot = c.ok("emulator/screenshot", json!({}));
    assert_eq!(
        shot["source"],
        json!("raster"),
        "control: after a run, an unmasked capture is the frame the raster drew"
    );
    assert!(
        shot.get("caveat").is_none(),
        "control: a raster capture carries no caveat: {shot}"
    );
    let _ = std::fs::remove_file(shot["path"].as_str().unwrap());

    c.ok(
        "emulator/set_layer_enabled",
        json!({"layer": "sprites", "enabled": false}),
    );
    for (method, params) in [
        ("emulator/screenshot", json!({})),
        ("emulator/scanlines", json!({"startLine": 0, "count": 1})),
    ] {
        let r = c.ok(method, params);
        assert_eq!(
            r["source"],
            json!("stateRender"),
            "{method}: a masked read is not the raster frame"
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
    let back = c.ok("emulator/screenshot", json!({}));
    assert_eq!(
        back["source"],
        json!("raster"),
        "clearing the mask restores the latched raster frame"
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
    let shot = c.ok("emulator/screenshot", json!({}));
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
    assert_eq!(
        c.ok(
            "emulator/memory_hash",
            json!({"addr": "0x00FF0000", "len": 4096})
        )["fnv1a64"],
        mem_before["fnv1a64"],
        "a display mask moved memory_hash — it cannot see memory at all"
    );
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
    let h = spawn_system("lay-zero", layered_machine(), 1024);
    let mut c = client(&h);
    let (width, height, want) = expected_frame(LayerMask::ALL);

    let shot = c.ok("emulator/screenshot", json!({}));
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
