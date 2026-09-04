//! `emulator/pixel_attribution` — `protocol.md` §6 (VRAM / CRAM / layers), adopted as CR-10
//! (`docs/2026-08-15-pixel-attribution-bus-method.md`, ruled in `docs/2026-08-15-fable-ruling-attribution.md`).
//!
//! Every test here is a **wire** round trip, and that is not decoration: `common::Client::recv` validates
//! each received line against the vendored contract schema, so simply driving the method through the
//! client is the schema-conformance pin (§5.3 test 9) — it needs no assertion of its own and it cannot be
//! forgotten.
//!
//! What is pinned, and why each one:
//!
//! * **the exact key set** — the ruling's condition 4. No surplus keys, of the kind the wire probe's F4
//!   found on ten existing methods. The schema's `result` has no `additionalProperties: false`, so a
//!   surplus key would pass the validator; this is the assertion that catches it.
//! * **`rgb == render_line(y)[x]`** — the core pins this internally; here it must survive serialization,
//!   which is where a channel-order bug would live.
//! * **bounds** — `-32004` with `width`/`height` in `error.data`, and `width`/`height` on success.
//! * **blanked-but-valid still answers** — display off, and the leftmost-column blank at `x < 8`.
//! * **the candidate bound** — 3 or 4 live, exactly 1 blanked, over a randomised sweep.
//! * **the sprite path end-to-end** — `sprite.tile` is the pattern the *renderer* drew from, for every
//!   dot of a 3x2 and a 2x3 sprite under all four flip combinations.
//! * **no pause required** — it answers a free-running machine, and says so in the stamp.
//!
//! The parity invariant of §5.3 test 1 — the panel and the bus agreeing — is **not** here, and cannot be:
//! `oracle-frontend` depends on `oracle-aether`, so a test in this crate cannot reach `pick::resolve`
//! without a dependency cycle. It lives in `crates/oracle-frontend/src/pick.rs` instead, where both sides
//! are in scope; see the note there.

mod common;

use common::{spawn_system, Client};
use oracle_core::rng::SplitMix64;
use oracle_core::system::System;
use oracle_core::vdp::Vdp;
use serde_json::{json, Value};
use std::collections::BTreeSet;

// -------------------------------------------------------------------------------------------------
// Fixtures
// -------------------------------------------------------------------------------------------------

/// The sprite's base pattern index.
const BASE_TILE: u16 = 0x10;
/// SAT base: reg 5 = $58 → `($58 & $7E) << 9` = $B000.
const SAT_BASE: u16 = 0xB000;
/// Screen position of the fixture sprite, both axes.
const SPRITE_AT: u16 = 64;

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

/// A machine whose VDP shows one sprite of `w_cells x h_cells` at (64, 64), with a distinguishable
/// pattern behind each cell.
///
/// Deliberately the **same fixture shape** as `oracle-frontend`'s `pick.rs`: pattern `BASE_TILE + n` is
/// solid colour nibble `n + 1`, so the rendered CRAM index *names the pattern the renderer drew from*.
/// That is what lets the sprite test below check the reported `sprite.tile` against the core's own
/// renderer rather than restating the column-major addressing on both sides — the check that caught a
/// row-major mix-up once already.
///
/// Both plane bases sit at $0000 over zeroed VRAM, so every plane cell is pattern 0 (also zeroed) and
/// therefore transparent, leaving the sprite the winner at every dot of its box.
fn machine_with_sprite(w_cells: u8, h_cells: u8, hflip: bool, vflip: bool) -> System {
    assert!(
        usize::from(w_cells) * usize::from(h_cells) <= 15,
        "the fixture gives each cell a unique colour nibble (1..=15)"
    );
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    let v = sys.vdp_mut();
    v.vram_mut().fill(0);
    // Reg $01 FIRST, and the order is load-bearing: the mode-4 register mask discards writes to
    // registers above 10 while M5 (reg $01 bit 2) is clear, so an $0C written ahead of it is silently
    // dropped and the fixture comes up H32 while its comment claims H40.
    set_reg(v, 0x01, 0x74); // display on, mode 5, DMA enable
    set_reg(v, 0x0C, 0x81); // H40 — before the SAT writes; the cache window depends on it
    set_reg(v, 0x05, 0x58); // SAT base $B000
    set_reg(v, 0x07, 0x00); // backdrop = CRAM 0
    set_reg(v, 0x0F, 0x02); // autoincrement 2
    set_reg(v, 0x10, 0x00); // 32x32 planes

    for n in 0..16u16 {
        let nib = u16::from((n as u8 % 15) + 1);
        let word = (nib << 12) | (nib << 8) | (nib << 4) | nib;
        write_vram(v, (BASE_TILE + n) * 32, &[word; 16]);
    }
    let attr: u16 = (u16::from(vflip) << 12) | (u16::from(hflip) << 11) | BASE_TILE;
    let size = u16::from(((w_cells - 1) << 2) | (h_cells - 1));
    write_vram(
        v,
        SAT_BASE,
        &[
            SPRITE_AT + 128,
            size << 8, // size high byte, link 0 low — link 0 ends the walk
            attr,
            SPRITE_AT + 128,
        ],
    );
    sys
}

/// A machine with one opaque plane-A cell at the top-left, and no sprites on screen.
fn machine_with_plane_cell() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    let v = sys.vdp_mut();
    v.vram_mut().fill(0);
    set_reg(v, 0x01, 0x74); // display on, mode 5 — before $0C; see machine_with_sprite
    set_reg(v, 0x0C, 0x81); // H40
    set_reg(v, 0x02, 0x30); // plane A nametable @ $C000
    set_reg(v, 0x04, 0x07); // plane B nametable @ $E000
    set_reg(v, 0x05, 0x58); // SAT @ $B000 — empty, so sprite 0's Y is 0-128, off-screen
    set_reg(v, 0x07, 0x25); // backdrop = CRAM entry $25
    set_reg(v, 0x0F, 0x02);
    set_reg(v, 0x10, 0x00);
    // Plane A cell (0,0) → pattern $055, palette 1, hi-pri; the pattern is solid nibble 3.
    write_vram(v, 0xC000, &[(1 << 15) | (1 << 13) | 0x055]);
    write_vram(v, 0x055 * 32, &[0x3333; 16]);
    sys
}

/// Connect, handshake, and return the client.
fn client(handle: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(handle);
    c.handshake(false);
    c
}

fn attribution(c: &mut Client, x: u16, y: u16) -> Value {
    c.ok("emulator/pixel_attribution", json!({"x": x, "y": y}))
}

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

/// The four keys the *envelope* stamps on after the handler returns (§2.2 / D11, §2.3 / D17). The
/// handler must never emit these itself, and the assertions below subtract them rather than listing
/// them among the method's own keys.
const ENVELOPE_KEYS: &[&str] = &["frame", "mclk", "running", "droppedEvents"];

fn method_keys(result: &Value) -> BTreeSet<String> {
    let mut k = keys(result);
    for e in ENVELOPE_KEYS {
        k.remove(*e);
    }
    k
}

// -------------------------------------------------------------------------------------------------
// The ruling's condition 4 — EXACTLY the schematized keys, no surplus
// -------------------------------------------------------------------------------------------------

/// **The ruling's condition 4, pinned.** The schema's `result` deliberately has no
/// `additionalProperties: false` (an envelope stamp is merged into it), so a surplus key would sail
/// through the validator — exactly the F4 finding the wire probe made against ten existing methods.
/// This is the assertion that closes it, at every level of the reply.
///
/// **Why every set below runs a frame first, added 2026-09-03 with §11.27's `caveat`.** These fixtures
/// pose a VDP by hand and were previously queried with the machine still at its power-on anchor — where
/// §11.27's *"or when no frame has completed"* arm applies, so every reply here would carry a `caveat`
/// and this test would be pinning the key set of the **disclosing** shape only. One `run_frames` puts
/// each fixture past its first frame, and the test ROM writes no CRAM
/// (`the_caveat_is_absent_when_nothing_wrote_the_entry_since_its_line_drew` is the row that measures
/// that), so the sets below are the ordinary, quiet key sets — `caveat` genuinely absent rather than
/// subtracted. The disclosing shape is pinned in its own section further down, and the pre-first-frame
/// arm by `a_reply_before_the_first_completed_frame_discloses_and_still_answers`.
#[test]
fn the_reply_carries_exactly_the_schematized_keys_and_no_surplus() {
    let h = spawn_system("pa-keys", machine_with_sprite(2, 2, false, false), 1024);
    let mut c = client(&h);
    c.ok("emulator/run_frames", json!({"frames": 1}));
    c.ok("emulator/pause", json!({}));

    // --- a sprite dot: the common keys + `sprite`, and no `cell`.
    let r = attribution(&mut c, SPRITE_AT + 3, SPRITE_AT + 3);
    assert_eq!(
        method_keys(&r),
        set(&[
            "x",
            "y",
            "width",
            "height",
            "winner",
            "cramIndex",
            "cramAddr",
            "rgb",
            "state",
            "candidates",
            "sprite",
        ]),
        "sprite reply: {r:#}"
    );
    assert_eq!(keys(&r["winner"]), set(&["layer", "spriteIndex"]));
    assert_eq!(keys(&r["rgb"]), set(&["r", "g", "b"]));
    assert_eq!(
        keys(&r["sprite"]),
        set(&[
            "index",
            "x",
            "y",
            "widthCells",
            "heightCells",
            "baseTile",
            "palette",
            "hflip",
            "vflip",
            "priority",
            "satAddr",
            "tile",
            "tileAddr",
        ]),
        "`cacheDivergence` is present only when true, and this fixture is coherent: {r:#}"
    );
    for cand in r["candidates"].as_array().expect("candidates") {
        let mut k = keys(cand);
        k.remove("spriteIndex"); // present iff the candidate IS the sprite
        assert_eq!(
            k,
            set(&["layer", "opaque", "priority", "cramIndex", "verdict"]),
            "candidate: {cand:#}"
        );
    }
    // The envelope's own four keys are present, and were applied by the server rather than the handler.
    for e in ENVELOPE_KEYS {
        assert!(r.get(*e).is_some(), "the envelope must stamp `{e}`: {r:#}");
    }

    // --- a plane dot: the common keys + `cell`, and no `sprite`.
    let h2 = spawn_system("pa-keys-plane", machine_with_plane_cell(), 1024);
    let mut c2 = client(&h2);
    c2.ok("emulator/run_frames", json!({"frames": 1}));
    c2.ok("emulator/pause", json!({}));
    let r = attribution(&mut c2, 2, 2);
    assert_eq!(r["winner"]["layer"], json!("planeA"), "{r:#}");
    assert_eq!(
        method_keys(&r),
        set(&[
            "x",
            "y",
            "width",
            "height",
            "winner",
            "cramIndex",
            "cramAddr",
            "rgb",
            "state",
            "candidates",
            "cell",
        ]),
        "plane reply: {r:#}"
    );
    assert_eq!(keys(&r["winner"]), set(&["layer"]), "no spriteIndex here");
    assert_eq!(
        keys(&r["cell"]),
        set(&["tile", "tileAddr", "palette", "hflip", "vflip", "priority"])
    );

    // --- a backdrop dot: neither `cell` nor `sprite`.
    let r = attribution(&mut c2, 300, 200);
    assert_eq!(r["winner"]["layer"], json!("backdrop"), "{r:#}");
    assert_eq!(
        method_keys(&r),
        set(&[
            "x",
            "y",
            "width",
            "height",
            "winner",
            "cramIndex",
            "cramAddr",
            "rgb",
            "state",
            "candidates",
        ]),
        "backdrop reply: {r:#}"
    );
}

// -------------------------------------------------------------------------------------------------
// §5.3 test 2 — the colour on the glass survives serialization
// -------------------------------------------------------------------------------------------------

/// `rgb` is the colour the renderer actually puts at the dot. The core pins this internally against
/// `render_line`; the point *here* is that it survives the wire, which is where a channel-order bug
/// (r/b swapped) would live and where the core's own test could never see it.
#[test]
fn rgb_and_cram_index_equal_what_render_line_produces_at_the_same_dot() {
    let sys = machine_with_sprite(3, 2, false, false);
    let reference = sys.vdp().clone();
    let h = spawn_system("pa-rgb", sys, 1024);
    let mut c = client(&h);

    let mut rng = SplitMix64::new(0xC0FFEE);
    for _ in 0..64 {
        let x = (rng.next_u64() % 320) as u16;
        let y = (rng.next_u64() % 224) as u16;
        let r = attribution(&mut c, x, y);
        let (rr, gg, bb) = reference.render_line(y)[usize::from(x)];
        assert_eq!(
            (r["rgb"]["r"].as_u64(), r["rgb"]["g"].as_u64(), r["rgb"]["b"].as_u64()),
            (Some(u64::from(rr)), Some(u64::from(gg)), Some(u64::from(bb))),
            "({x},{y}): the wire's rgb must equal render_line's — a channel swap lives exactly here"
        );
        let attr = reference.pixel_attribution(x, y);
        assert_eq!(r["cramIndex"], json!(attr.cram_index), "({x},{y})");
        assert_eq!(
            r["cramAddr"],
            json!(format!("0x{:08X}", u32::from(attr.cram_index) * 2)),
            "({x},{y}): cramAddr is index*2, as a hex string (D9 category 1)"
        );
    }
}

// -------------------------------------------------------------------------------------------------
// §5.3 test 3 — the bounds decision (§3.5)
// -------------------------------------------------------------------------------------------------

/// A dot outside the active display is **refused**, not answered with a plausible backdrop.
///
/// The core is deliberately total there; on a wire that totality is a silent wrong answer, because the
/// client cannot tell a nonexistent dot's backdrop from a real one. The refusal carries the bound, so a
/// client learns it from the failure — and `width`/`height` are on every success too, so it never has to
/// provoke one.
#[test]
fn a_dot_outside_the_active_display_is_refused_with_the_bound_attached() {
    let h = spawn_system("pa-bounds", machine_with_plane_cell(), 1024);
    let mut c = client(&h);

    // The last real dot answers.
    let r = attribution(&mut c, 319, 223);
    assert_eq!(r["width"], json!(320));
    assert_eq!(r["height"], json!(224));

    for (x, y, why) in [
        (320u16, 0u16, "one past the right edge in H40"),
        (0, 224, "one past the bottom"),
        (511, 511, "the widest the schema allows a param to be"),
    ] {
        let e = c.err("emulator/pixel_attribution", json!({"x": x, "y": y}));
        assert_eq!(e["code"], json!(-32004), "{why}: {e:#}");
        assert_eq!(e["data"]["width"], json!(320), "{why}: {e:#}");
        assert_eq!(e["data"]["height"], json!(224), "{why}: {e:#}");
    }

    // A coordinate outside the schema's own param range is a *different* failure, and stays one:
    // -32602 says "that is not a coordinate", -32004 says "that coordinate is not on this display".
    let e = c.err("emulator/pixel_attribution", json!({"x": 512, "y": 0}));
    assert_eq!(e["code"], json!(-32602), "{e:#}");
    let e = c.err("emulator/pixel_attribution", json!({"x": "0x10", "y": 0}));
    assert_eq!(
        e["code"],
        json!(-32602),
        "a coordinate is a number (D9): {e:#}"
    );
    let e = c.err("emulator/pixel_attribution", json!({"y": 0}));
    assert_eq!(e["code"], json!(-32602), "`x` is required: {e:#}");
}

/// The bound is the **active** width, so it moves with the mode — H32 refuses at 256 where H40 answered.
/// A client that cached 320 from an H40 reply and kept sweeping would otherwise be reading 64 columns of
/// invented backdrop.
#[test]
fn the_width_bound_tracks_the_h32_h40_mode() {
    let mut sys = machine_with_plane_cell();
    set_reg(sys.vdp_mut(), 0x0C, 0x00); // H32
    let h = spawn_system("pa-h32", sys, 1024);
    let mut c = client(&h);

    let r = attribution(&mut c, 255, 0);
    assert_eq!(r["width"], json!(256), "H32 is 256 wide: {r:#}");
    assert_eq!(r["height"], json!(224));
    let e = c.err("emulator/pixel_attribution", json!({"x": 256, "y": 0}));
    assert_eq!(e["code"], json!(-32004), "{e:#}");
    assert_eq!(e["data"]["width"], json!(256), "{e:#}");
}

// -------------------------------------------------------------------------------------------------
// §5.3 test 4 — blanked, but valid
// -------------------------------------------------------------------------------------------------

/// Two dots that look like the refusal case and are **not**: the display being off, and the
/// leftmost-column blank at `x < 8`. Both dots exist, and the backdrop genuinely is what is shown.
/// Refusing either would be answering a real question with an error.
#[test]
fn a_blanked_dot_still_answers_with_the_backdrop() {
    // (a) display off.
    let mut sys = machine_with_plane_cell();
    set_reg(sys.vdp_mut(), 0x01, 0x34); // mode 5, DMA — display-enable bit cleared
    let h = spawn_system("pa-blank", sys, 1024);
    let mut c = client(&h);
    let r = attribution(&mut c, 2, 2);
    assert_eq!(r["winner"]["layer"], json!("backdrop"), "{r:#}");
    assert_eq!(r["cramIndex"], json!(0x25), "the backdrop register: {r:#}");
    let cands = r["candidates"].as_array().expect("candidates");
    assert_eq!(cands.len(), 1, "a blanked dot has one candidate: {r:#}");
    assert_eq!(cands[0]["layer"], json!("backdrop"));
    assert_eq!(cands[0]["verdict"], json!("won"));

    // (b) the leftmost-column blank, with the display ON — x < 8 is blanked, x >= 8 is not.
    let mut sys = machine_with_plane_cell();
    set_reg(sys.vdp_mut(), 0x00, 0x20); // reg $00 bit 5: blank the leftmost column
                                        // Give plane A an opaque cell at column 1 too, so the x >= 8 half of the comparison has a winner.
    write_vram(sys.vdp_mut(), 0xC002, &[(1 << 13) | 0x055]);
    let h = spawn_system("pa-lcb", sys, 1024);
    let mut c = client(&h);
    let r = attribution(&mut c, 3, 2);
    assert_eq!(
        r["winner"]["layer"],
        json!("backdrop"),
        "x<8 is blanked: {r:#}"
    );
    assert_eq!(r["candidates"].as_array().unwrap().len(), 1, "{r:#}");
    let r = attribution(&mut c, 9, 2);
    assert_eq!(r["winner"]["layer"], json!("planeA"), "x>=8 is not: {r:#}");
    assert!(r["candidates"].as_array().unwrap().len() >= 3, "{r:#}");
}

// -------------------------------------------------------------------------------------------------
// §5.3 test 5 — the structural candidate bound `candidates` rests on
// -------------------------------------------------------------------------------------------------

/// The reason `candidates` needs no cursor: the list is bounded **by construction** at 4 — at most one
/// flattened sprite pixel, the plane-A slot, plane B, and the backdrop — not by a server policy a client
/// would have to page around. 3 or 4 for a live dot, exactly 1 for a blanked one.
#[test]
fn the_candidate_list_is_three_or_four_live_and_exactly_one_blanked() {
    let h = spawn_system("pa-cands", machine_with_sprite(3, 2, false, false), 1024);
    let mut c = client(&h);
    let mut rng = SplitMix64::new(0x5EED_5EED);
    let mut saw_four = false;
    let mut saw_three = false;
    for _ in 0..200 {
        let x = (rng.next_u64() % 320) as u16;
        let y = (rng.next_u64() % 224) as u16;
        let r = attribution(&mut c, x, y);
        let n = r["candidates"].as_array().expect("candidates").len();
        assert!(
            (3..=4).contains(&n),
            "({x},{y}) live dot: {n} candidates: {r:#}"
        );
        saw_four |= n == 4;
        saw_three |= n == 3;
        // Exactly one `won`, and it is the winner the reply names.
        let won: Vec<&Value> = r["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["verdict"] == json!("won"))
            .collect();
        assert_eq!(won.len(), 1, "({x},{y}): {r:#}");
        assert_eq!(won[0]["layer"], r["winner"]["layer"], "({x},{y}): {r:#}");
        assert_eq!(won[0]["cramIndex"], r["cramIndex"], "({x},{y}): {r:#}");
    }
    assert!(saw_four, "the sweep must cross the sprite (4 candidates)");
    assert!(saw_three, "and miss it too (3 candidates)");

    // The blanked end of the same bound.
    let mut sys = machine_with_sprite(3, 2, false, false);
    set_reg(sys.vdp_mut(), 0x01, 0x34); // display off
    let h = spawn_system("pa-cands-off", sys, 1024);
    let mut c = client(&h);
    for _ in 0..32 {
        let x = (rng.next_u64() % 320) as u16;
        let y = (rng.next_u64() % 224) as u16;
        let r = attribution(&mut c, x, y);
        assert_eq!(r["candidates"].as_array().unwrap().len(), 1, "({x},{y})");
    }
}

// -------------------------------------------------------------------------------------------------
// §5.3 tests 6 and 7 — the sprite path, end to end
// -------------------------------------------------------------------------------------------------

/// **The sprite pin.** For every dot of a sprite, under every flip combination, the `sprite.tile` on the
/// wire is the pattern the *core's renderer* actually drew that dot from — checked through the fixture's
/// unique solid colours, so the CRAM index names the pattern and the column-major addressing is never
/// restated on both sides of the assertion. (3,2) and (2,3) are the pair that catch a row-major mix-up.
#[test]
fn the_reported_sprite_tile_is_the_one_the_renderer_drew_from() {
    for (w, h_cells) in [(1u8, 1u8), (3, 2), (2, 3), (4, 1), (1, 4)] {
        for (hflip, vflip) in [(false, false), (true, false), (false, true), (true, true)] {
            let handle = spawn_system(
                "pa-sprite",
                machine_with_sprite(w, h_cells, hflip, vflip),
                1024,
            );
            let mut c = client(&handle);
            for dy in 0..usize::from(h_cells) * 8 {
                for dx in 0..usize::from(w) * 8 {
                    let (x, y) = (SPRITE_AT + dx as u16, SPRITE_AT + dy as u16);
                    let r = attribution(&mut c, x, y);
                    assert_eq!(r["winner"]["layer"], json!("sprite"), "({x},{y}): {r:#}");
                    assert_eq!(r["winner"]["spriteIndex"], json!(0), "({x},{y}): {r:#}");
                    let named = r["sprite"]["tile"].as_u64().unwrap_or_else(|| {
                        panic!("{w}x{h_cells} hflip={hflip} vflip={vflip} ({x},{y}): the dot is inside the sprite, so a tile must be reported: {r:#}")
                    }) as u16;
                    // The renderer's own answer: pattern `BASE_TILE + n` is solid nibble `n + 1`.
                    let nibble = (r["cramIndex"].as_u64().unwrap() % 16) as u16;
                    assert_eq!(
                        nibble,
                        named - BASE_TILE + 1,
                        "{w}x{h_cells} hflip={hflip} vflip={vflip} at ({x},{y}): named tile \
                         ${named:03X} but the renderer drew colour nibble {nibble}: {r:#}"
                    );
                    assert_eq!(
                        r["sprite"]["tileAddr"],
                        json!(format!("0x{:08X}", u32::from(named) * 32)),
                        "({x},{y})"
                    );
                    assert_eq!(r["sprite"]["baseTile"], json!(BASE_TILE), "({x},{y})");
                    assert_eq!(
                        r["sprite"]["satAddr"],
                        json!(format!("0x{SAT_BASE:08X}")),
                        "sprite 0's entry is at the SAT base"
                    );
                    assert_eq!(r["sprite"]["widthCells"], json!(w), "({x},{y})");
                    assert_eq!(r["sprite"]["heightCells"], json!(h_cells), "({x},{y})");
                    assert_eq!(r["sprite"]["hflip"], json!(hflip), "({x},{y})");
                    assert_eq!(r["sprite"]["vflip"], json!(vflip), "({x},{y})");
                }
            }
        }
    }
}

/// **§5.3 test 7, and the honest form of it.** The design pins that `sprite.tile` is *absent, not
/// invented*, when the winning sprite's box no longer contains the dot.
///
/// Through the bus that state is **unreachable**, and saying so is worth more than a test that pretends
/// otherwise: attribution re-derives the scanline on the call, and `sprites_decoded` reads the same live
/// cache + VRAM the sprite walk did, within the same handler invocation. There is no interval for the SAT
/// to move in. So the pin here is the *positive* one — the bus never reports a sprite winner without a
/// tile — and the absent branch stays exercised where it can be: `sprite_tile_at`'s own tests in
/// `oracle-frontend`'s `pick.rs` (`a_dot_outside_the_sprite_has_no_tile`), which drive it with a dot
/// outside the box directly rather than through a machine.
///
/// This is flagged for review rather than worked around silently: if the handler ever grows a cached or
/// deferred render, this test is the one that should be replaced by a real absence case.
#[test]
fn a_sprite_winner_always_carries_its_tile_because_the_read_is_atomic() {
    let mut sys = machine_with_sprite(4, 1, false, false);
    // Diverge the SAT cache from VRAM: move the SAT base without rewriting the cache, so the cached
    // Y/size/link and the VRAM X/attr come from different places — the stale-cache state.
    let v = sys.vdp_mut();
    write_vram(
        v,
        0xB800,
        &[SPRITE_AT + 128, 0x0300, BASE_TILE + 4, SPRITE_AT + 128],
    );
    set_reg(v, 0x05, 0x5C); // SAT base $B800

    let h = spawn_system("pa-sat", sys, 1024);
    let mut c = client(&h);
    let mut found = 0;
    for dx in 0..32u16 {
        let r = attribution(&mut c, SPRITE_AT + dx, SPRITE_AT + 3);
        if r["winner"]["layer"] == json!("sprite") {
            assert!(
                r["sprite"]["tile"].is_u64(),
                "a sprite winner must carry the tile it drew from: {r:#}"
            );
            assert!(r["sprite"]["tileAddr"].is_string(), "{r:#}");
            // The stale-cache state, made visible — and present *only* when true, which is why the
            // key-set test above (a coherent fixture) must not see this key at all.
            assert_eq!(
                r["sprite"]["cacheDivergence"],
                json!(true),
                "the reg-5 move without a cache rewrite is exactly the divergence flag's case: {r:#}"
            );
            assert_eq!(
                r["sprite"]["satAddr"],
                json!("0x0000B800"),
                "the SAT entry address follows the live reg-5 base: {r:#}"
            );
            found += 1;
        }
    }
    assert!(found > 0, "the fixture must actually show the sprite");
}

// -------------------------------------------------------------------------------------------------
// §5.3 test 8 — a pure read needs no pause
// -------------------------------------------------------------------------------------------------

/// **It is a pure read** (§3.3): it must answer a free-running machine, and the envelope's
/// `running: true` is what tells the client the sample is live. Gating it on a pause would be a new rule
/// — §6's run-control rule names the ops that *mutate the timeline*, and this mutates nothing.
#[test]
fn it_answers_a_free_running_machine_and_the_stamp_says_so() {
    let h = spawn_system("pa-running", machine_with_plane_cell(), 1024);
    let mut c = client(&h);
    c.ok("emulator/resume", json!({}));
    let r = attribution(&mut c, 2, 2);
    assert_eq!(
        r["running"],
        json!(true),
        "a live sample must be labelled one (D11): {r:#}"
    );
    assert!(r["frame"].is_u64() && r["mclk"].is_u64(), "{r:#}");
    // And it is still a plain success — no -32005, no implicit pause on the way.
    assert_eq!(r["winner"]["layer"], json!("planeA"), "{r:#}");
    assert_eq!(
        c.ok("emulator/status", json!({}))["running"],
        json!(true),
        "the read must not have paused the machine as a side effect"
    );
}

// ---------------------------------------------------------------------------------------------------
// `rgb` resolves from LIVE state, and that is the contract — not a defect
// ---------------------------------------------------------------------------------------------------

/// ⚑ **THE ANTI-FIX PIN. This method MUST NOT read a framebuffer, and this row exists to stop the next
/// person "fixing" it — because this seat tried, on 2026-08-30, and got as far as a green suite.**
///
/// The story is worth the words. aeon reported that `cramIndex` is right while `rgb` reports a different
/// frame's colour on a ROM that repaints CRAM mid-frame; they lost an hour to it and it produced a
/// confident wrong finding about their engine. It reproduces exactly — on `color_1536.bin`, 55 of 55
/// sampled rows. A fix reading the latched raster made all 220 sampled dots agree.
///
/// **The contract forbids that fix in terms**, `protocol.md` §11.3: *"A server answers by resolving the
/// scanline from live VDP state … and **MUST NOT read a framebuffer**"*, and then, about this exact
/// disagreement: *"**This is not a defect in either method and a server MUST NOT try to paper over it**;
/// a client that needs the two to agree needs a per-scanline capability — `emulator/scanlines`."* The
/// divergence is a **registered follow-up, F-SCANLINE-INDEX**, not a bug.
///
/// So the behaviour below is correct-by-contract, and the real defect aeon hit is **discoverability**:
/// nothing in the reply points at the reconciliation path. That is a CR against the fragment, not a
/// server change — see `docs/2026-08-30-rgb-live-resolve.md`.
///
/// **Updated 2026-09-03**, when that CR came back ratified as §11.27. The behaviour below is unchanged
/// and the anti-fix pin still means what it meant; what is new is the last assertion — the reply now
/// **says** the picture may disagree, on this exact scenario, which is the one that produced the CR.
/// The pin and the disclosure are complements, not alternatives: `rgb` stays live *and* the divergence
/// stops being silent. If a future session ever makes this row green by reporting the latched raster,
/// it will have deleted the reason the caveat exists at the same time.
#[test]
fn rgb_resolves_against_live_state_and_the_row_must_not_read_a_framebuffer() {
    let h = spawn_system("pa-rgb-live", machine_with_plane_cell(), 1024);
    let mut c = client(&h);

    // Draw a whole frame, so a latched raster EXISTS to be wrongly reachable.
    c.ok("emulator/run_frames", json!({"frames": 1}));
    c.ok("emulator/pause", json!({}));
    let before = attribution(&mut c, 4, 4);
    let index = before["cramIndex"].as_u64().expect("cramIndex");

    // Repaint that entry. The picture on screen is unchanged — nothing has been drawn since.
    c.ok(
        "emulator/write_cram",
        json!({"line": index / 16, "index": index % 16, "r": 7, "g": 0, "b": 7}),
    );

    let after = attribution(&mut c, 4, 4);
    assert_eq!(
        after["cramIndex"], before["cramIndex"],
        "the index is a property of the resolve and does not move: {after}"
    );
    assert_ne!(
        after["rgb"], before["rgb"],
        "rgb MUST follow live CRAM. If this row ever goes green by reporting the latched raster, someone \
         has made the method read a framebuffer — which §11.3 forbids in terms, and which silently breaks \
         the guarantee that this method answers about the VDP's state NOW rather than about a frame"
    );
    assert_eq!(
        after["rgb"],
        json!({"r": 255, "g": 0, "b": 255}),
        "and it is the newly painted colour, not merely a different one: {after}"
    );
    // §11.27: the divergence this row exists to protect is now AUDIBLE. `before` was quiet (a frame
    // had drawn and nothing had written the entry since), `after` is not, and the difference is the
    // write — which is what makes this an assertion about the rule rather than about the key.
    assert!(
        before.get("caveat").is_none(),
        "before the repaint the entry was quiet, or the assertion below witnesses nothing: {before}"
    );
    assert!(
        after["caveat"]
            .as_str()
            .is_some_and(|c| c.contains("emulator/scanlines")),
        "§11.27: this is THE scenario the clause was written for — aeon's `cramIndex` right and `rgb` \
         from another moment — so the reply must disclose it and name the path: {after}"
    );
}

// ---------------------------------------------------------------------------------------------------
// §11.27 — the reply says out loud when its colour may not be the one on the glass
// ---------------------------------------------------------------------------------------------------
//
// **These are the "required when applicable" half of §11.27, and they can only live here.** The ruling
// says so in terms: a schema cannot see a write stamp, so the four wire vectors
// (`docs/proposed/2026-08-30-cr-g-vectors.json`, merged upstream) can only pin that `caveat` is a string
// and that disclosure never displaces `rgb`. A reply *with* the caveat and a reply *without* it are both
// valid documents. Which one a server owes is a live conformance question, and these are the rows that
// ask it — exactly as §11.26's two did for `object_at`.
//
// ⚑ **The vacuity that has to be designed out.** A caveat test that never arranges a qualifying write
// proves only that the key is absent — which is also what a server that never emits produces, i.e. the
// state this repo was in before today. So the rows below come in **pairs over one fixture at one dot**,
// differing in the write stamp and in nothing else: same machine, same frame count, same coordinate,
// same winner, same `cramIndex`. If the pair ever agrees, the emission rule is inert.

/// **The absence half, and the anti-vacuity control for everything below.**
///
/// A completed frame exists, and nothing has written the displayed entry since line `y` of it drew, so
/// §11.27's rule says *absent* — "no write since the line drew means the glass cannot differ" is a
/// measurement, and this is the measurement coming back negative.
///
/// It is also what stops `caveat` becoming structural. §2.4's advisory names the failure precisely: a
/// caveat present on every reply is documentation wearing signal's clothes, and `emulator/read_memory`'s
/// constant string is the in-tree example. §11.27 raises it to a MUST NOT — an unconditional caveat is
/// non-conformant, not merely unhelpful — because both engines in this suite rebuild CRAM every vblank,
/// which would make "a completed frame exists" fire on every reply after the first.
///
/// **The fixture's silence is measured, not assumed.** `machine_with_plane_cell` poses VRAM and the
/// registers by hand and writes **no CRAM at all** — the winning colour is an untouched entry — and
/// `oracle_core::testrom` writes none either, which was checked by probe across 1, 2 and 5 frames before
/// this row was written. So the stamp here is genuinely "never written", not "written early enough".
/// The row that proves this fixture *can* produce the other answer is its pair,
/// `a_cram_write_after_the_line_drew_makes_the_reply_say_so_and_name_the_path`, which reaches the
/// disclosing state from this identical setup with one `emulator/write_cram`.
#[test]
fn the_caveat_is_absent_when_nothing_wrote_the_entry_since_its_line_drew() {
    let h = spawn_system("pa-cav-quiet", machine_with_plane_cell(), 1024);
    let mut c = client(&h);
    c.ok("emulator/run_frames", json!({"frames": 1}));
    c.ok("emulator/pause", json!({}));

    let r = attribution(&mut c, 2, 2);
    assert_eq!(r["winner"]["layer"], json!("planeA"), "{r:#}");
    assert!(
        r.get("caveat").is_none(),
        "a completed frame exists and NOTHING wrote this entry since its line drew, so §11.27's rule \
         says absent. A caveat here is the unconditional shape the clause forbids in terms — the \
         `read_memory` constant-string failure §2.4 names — and it would make every row below vacuous: \
         {r:#}"
    );
}

/// **The disclosure half — §11.27's first vector, live.** Same fixture, same dot, same completed frame
/// as the row above; the ONLY difference is that the displayed palette entry has since been repainted.
///
/// This is the case that cost aeon an hour: `cramIndex` right, `rgb` a colour the glass never showed,
/// and no way to tell from the reply. §11.3 keeps `rgb` live — the fix is forbidden, see
/// `rgb_resolves_against_live_state_and_the_row_must_not_read_a_framebuffer` — so the reply's job is to
/// **say so**, and to say **where to go instead**.
///
/// Both of oracle's two properties from the CR are asserted: the path is named (not merely the hazard),
/// and disclosure arrives **beside** the datum rather than instead of it (`rgb` still present, still the
/// live colour — a reply that answered with a caveat and no `rgb` is one of the ruling's red vectors).
#[test]
fn a_cram_write_after_the_line_drew_makes_the_reply_say_so_and_name_the_path() {
    let h = spawn_system("pa-cav-write", machine_with_plane_cell(), 1024);
    let mut c = client(&h);
    c.ok("emulator/run_frames", json!({"frames": 1}));
    c.ok("emulator/pause", json!({}));

    let before = attribution(&mut c, 2, 2);
    assert!(
        before.get("caveat").is_none(),
        "the pair's control: this fixture is quiet before the write, or the assertion after it \
         witnesses nothing: {before:#}"
    );
    let index = before["cramIndex"].as_u64().expect("cramIndex");

    c.ok(
        "emulator/write_cram",
        json!({"line": index / 16, "index": index % 16, "r": 7, "g": 0, "b": 7}),
    );

    let after = attribution(&mut c, 2, 2);
    assert_eq!(
        after["cramIndex"], before["cramIndex"],
        "the pair must differ in the WRITE STAMP and nothing else — a different index would mean a \
         different dot was measured: {after:#}"
    );
    let caveat = after["caveat"]
        .as_str()
        .unwrap_or_else(|| panic!("§11.27: a write after the line drew MUST disclose: {after:#}"));
    assert!(
        caveat.contains("emulator/scanlines"),
        "§11.27 pins TWO properties and this is the second: the caveat names the reconciliation \
         PATH, not only the hazard. aeon already had the workaround; a caller who does not must be \
         sent somewhere, or the caveat costs an hour and saves none. Got: {caveat}"
    );
    assert!(
        after.get("rgb").is_some(),
        "disclosure rides BESIDE the datum, never instead of it — answering with a caveat and no \
         `rgb` is a red vector in the ruling's own set: {after:#}"
    );
}

/// **The rule is PER ENTRY, and this is the row that proves we implemented the precise form rather than
/// the coarse one §11.27 also permits.**
///
/// A server that cannot stamp per-entry writes MAY emit on *any* CRAM write since the line drew. Oracle
/// can, so it does not: repainting entry X must leave the dot displaying entry Y silent. Under the
/// coarse rule this test fails at the first assertion, which is exactly what makes it a measurement of
/// which rule is in force rather than a restatement of the one above.
///
/// The second half is the pair's other end over the *same* connection and the same dot: repaint the
/// entry that IS displayed, and the same reply discloses. So the two assertions differ only in **which
/// entry the write landed on** — not in the fixture, the frame count, the dot, or the payload.
#[test]
fn the_caveat_is_per_entry_so_repainting_an_unrelated_colour_stays_silent() {
    let h = spawn_system("pa-cav-perentry", machine_with_plane_cell(), 1024);
    let mut c = client(&h);
    c.ok("emulator/run_frames", json!({"frames": 1}));
    c.ok("emulator/pause", json!({}));

    let shown = attribution(&mut c, 2, 2)["cramIndex"]
        .as_u64()
        .expect("cramIndex");
    // Some entry that is NOT the one on the glass at this dot. Same palette line where possible, so the
    // write is as adjacent as it can be without being the subject.
    let other = if shown.is_multiple_of(16) {
        shown + 1
    } else {
        shown - 1
    };
    assert_ne!(shown, other, "the control must poke a DIFFERENT entry");

    c.ok(
        "emulator/write_cram",
        json!({"line": other / 16, "index": other % 16, "r": 7, "g": 7, "b": 0}),
    );
    let r = attribution(&mut c, 2, 2);
    assert!(
        r.get("caveat").is_none(),
        "entry {other} was repainted; entry {shown} is what this dot shows and it has not been \
         touched since its line drew. §11.27's rule is about the entry at `cramIndex`, so this reply \
         must stay silent. A caveat here means the coarse any-CRAM-write fallback is in force, which \
         is conformant but is NOT what this server claims to implement: {r:#}"
    );

    // …and the same dot, same connection, now with the write on the entry that IS displayed.
    c.ok(
        "emulator/write_cram",
        json!({"line": shown / 16, "index": shown % 16, "r": 7, "g": 0, "b": 7}),
    );
    let r = attribution(&mut c, 2, 2);
    assert!(
        r.get("caveat").is_some(),
        "and the other end of the pair: the DISPLAYED entry was repainted, so the same dot must now \
         disclose. Without this half the row above is equally consistent with a server that never \
         emits at all: {r:#}"
    );
}

/// **§11.27's third vector, live: a reply before any frame has completed carries the caveat.**
///
/// §11.3 requires this method to answer a machine that has rendered nothing since power-on — *"a machine
/// that has rendered nothing since power-on answers as well as one mid-level"* — and that answer is
/// real. What it is not is comparable with a picture, because there is no picture. §11.27 folds this into
/// the same key rather than inventing a second one, and the honest statement is about which moment the
/// row answers for.
///
/// The complement is the row above it in the file: one `run_frames` on this identical fixture removes
/// the caveat. So the pair differs in whether a frame completed and in nothing else.
#[test]
fn a_reply_before_the_first_completed_frame_discloses_and_still_answers() {
    let h = spawn_system("pa-cav-preframe", machine_with_plane_cell(), 1024);
    let mut c = client(&h);

    let r = attribution(&mut c, 2, 2);
    let caveat = r["caveat"].as_str().unwrap_or_else(|| {
        panic!("§11.27: with no completed frame there is nothing to agree with, and the reply says so: {r:#}")
    });
    assert!(
        caveat.contains("emulator/scanlines"),
        "this arm names the path too — a caller here needs the same signpost: {caveat}"
    );
    // The whole answer is still an answer. §11.3's power-on sentence is the point: a caveat is not an
    // excuse for a thinner reply.
    assert_eq!(r["winner"]["layer"], json!("planeA"), "{r:#}");
    assert!(
        r.get("rgb").is_some() && r.get("cramIndex").is_some(),
        "disclosure beside the datum, here too: {r:#}"
    );
}
