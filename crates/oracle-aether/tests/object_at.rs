//! `emulator/object_at` — `protocol.md` §11.26 (CR-F, ADOPT WITH CHANGES).
//!
//! One click, one answer, every failure named. Every test here is a **wire** round trip, so
//! `common::Client::recv` validates each reply against the vendored contract schema — driving the method
//! at all is the schema-conformance pin, and it cannot be forgotten.
//!
//! **Two of these rows are the OBJECT-AT-CONFORMANCE pair, and they exist because a document schema
//! structurally cannot see them.** The applied fragment says so in its own `$comment`: both
//! `{kind:"unavailable"}` and `{kind:"none", raw:"0x0000"}` are valid *documents*, so nothing in the
//! schema can tell a server that answered "no owner" from one that answered "no table". They are:
//!
//! * [`a_build_without_the_owner_table_says_unavailable_not_none`] — the release-ROM shape. A click on a
//!   shipped build must say *this build has no owner table*, never *nothing here is an object*.
//! * [`addresses_are_resolved_again_after_a_rom_swap_never_remembered`] — M3's normative re-resolve.
//!
//! The rest pin the five outcomes, the two halves' independence, and the sentinel guard.

mod common;

use common::{spawn_system, Client};
use oracle_core::system::System;
use oracle_core::vdp::Vdp;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------------------------------
// Fixture — a machine showing one sprite, and a listing describing where things live
// ---------------------------------------------------------------------------------------------------

const BASE_TILE: u16 = 0x10;
const SAT_BASE: u16 = 0xB000;
/// Screen position of the fixture sprite, both axes. A dot inside its box is the sprite's.
const SPRITE_AT: u16 = 64;
/// A dot inside the sprite.
const DOT: (u16, u16) = (70, 70);

/// `sst.emp`: `pub struct Sst (size: $50)`. Held here because this test builds the listing; the server
/// measures it from two adjacent slot symbols and never holds it.
const SST: u32 = 0x50;
const NUM_TOTAL: u32 = 2 + 40 + 8 + 16;
const BASE: u32 = 0x00FF_8DB0;
const OBJ_CODE_BASE: u32 = 0x0001_0000;

/// Work RAM homes for the fixture's own tables. Arbitrary to the server — which is the point: it must
/// read them out of the listing, and [`addresses_are_resolved_again_after_a_rom_swap_never_remembered`]
/// moves them to prove it.
const SPRITE_OWNER: u32 = 0x00FF_E1EE;
const CAMERA_X: u32 = 0x00FF_A604;
const CAMERA_Y: u32 = 0x00FF_A608;

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

/// A machine showing one 2x2 sprite at [`SPRITE_AT`], over transparent planes.
///
/// Same fixture shape as `pixel_attribution.rs` and `pick.rs` deliberately: pattern `BASE_TILE + n` is a
/// solid colour nibble, both plane bases sit over zeroed VRAM so every plane cell is transparent, and the
/// sprite therefore wins at every dot of its box.
fn machine_with_sprite() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    let v = sys.vdp_mut();
    v.vram_mut().fill(0);
    // Reg $01 FIRST: the mode-4 register mask drops writes above register 10 while M5 is clear, so an
    // $0C written ahead of it is silently discarded and the fixture comes up H32.
    set_reg(v, 0x01, 0x74);
    set_reg(v, 0x0C, 0x81);
    set_reg(v, 0x05, 0x58);
    set_reg(v, 0x07, 0x00);
    set_reg(v, 0x0F, 0x02);
    set_reg(v, 0x10, 0x00);
    for n in 0..16u16 {
        let nib = u16::from((n as u8 % 15) + 1);
        let word = (nib << 12) | (nib << 8) | (nib << 4) | nib;
        write_vram(v, (BASE_TILE + n) * 32, &[word; 16]);
    }
    write_vram(
        v,
        SAT_BASE,
        &[SPRITE_AT + 128, 0x0500, BASE_TILE, SPRITE_AT + 128],
    );
    sys
}

/// A machine with no sprites at all — the SAT is empty, so sprite 0's Y puts it off-screen and the
/// backdrop wins every dot.
fn machine_with_no_sprite() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    let v = sys.vdp_mut();
    v.vram_mut().fill(0);
    set_reg(v, 0x01, 0x74);
    set_reg(v, 0x0C, 0x81);
    set_reg(v, 0x05, 0x58);
    set_reg(v, 0x07, 0x25);
    set_reg(v, 0x0F, 0x02);
    set_reg(v, 0x10, 0x00);
    sys
}

/// Object-pool rows, addresses **computed** from base and stride rather than listed: a table of literals
/// would let this test agree with a server holding the same literals.
fn pool_rows(base: u32, stride: u32) -> Vec<(String, u32)> {
    vec![
        ("Object_RAM".into(), base),
        ("Player_1".into(), base),
        ("Player_2".into(), base + stride),
        ("Object_RAM_End".into(), base + NUM_TOTAL * stride),
        ("ObjCodeBase".into(), OBJ_CODE_BASE),
    ]
}

/// The full debug-shape listing: the object pool, both cameras, and the owner table.
fn debug_rows(base: u32, owner: u32, cam: (u32, u32)) -> Vec<(String, u32)> {
    let mut r = pool_rows(base, SST);
    r.push(("Camera_X".into(), cam.0));
    r.push(("Camera_Y".into(), cam.1));
    r.push(("Sprite_Owner".into(), owner));
    r
}

/// The RELEASE shape: cameras present **and moved**, owner table absent entirely.
///
/// Not invented — this is the measured difference between `s4.lst` and `s4.debug.lst` recorded in
/// §11.26's table: `Camera_X` `FFFFA576` vs `FFFFA604`, `Sprite_Owner` absent from release.
fn release_rows(base: u32) -> Vec<(String, u32)> {
    let mut r = pool_rows(base, SST);
    r.push(("Camera_X".into(), 0x00FF_A576));
    r.push(("Camera_Y".into(), 0x00FF_A57A));
    r
}

fn listing(rows: &[(String, u32)]) -> String {
    let mut s = String::from("  Symbol Table (* = unused):\n\n");
    for (name, addr) in rows {
        s.push_str(&format!(" {name} : {addr:X} C |\n"));
    }
    s.push_str(&format!("\n{:>4} symbols\n", rows.len()));
    s
}

/// Unique filename per call: these run in parallel in one process, and a shared path means two threads
/// writing and reading one file.
fn load_listing(c: &mut Client, tag: &str, rows: &[(String, u32)]) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("oracle-objat-{}-{tag}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{tag}.lst"));
    std::fs::write(&path, listing(rows)).unwrap();
    c.ok(
        "emulator/load_symbols",
        json!({"path": path.to_str().unwrap()}),
    );
}

fn client(h: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(h);
    c.handshake(false);
    c
}

fn poke_word(c: &mut Client, addr: u32, value: u32) {
    c.ok(
        "emulator/write_memory",
        json!({"addr": format!("0x{addr:06X}"), "value": value, "width": 2}),
    );
}

/// Arm the owner table entry for SAT slot 0, and a camera, on a paused machine.
fn arm(c: &mut Client, owner_word: u32, cam: (u32, u32), cam_at: (u32, u32)) {
    c.ok("emulator/pause", json!({}));
    poke_word(c, SPRITE_OWNER, owner_word);
    poke_word(c, cam_at.0, cam.0);
    poke_word(c, cam_at.1, cam.1);
}

fn at(c: &mut Client, dot: (u16, u16)) -> Value {
    c.ok("emulator/object_at", json!({"x": dot.0, "y": dot.1}))
}

// ---------------------------------------------------------------------------------------------------
// The five outcomes
// ---------------------------------------------------------------------------------------------------

/// The whole-answer case: a sprite owned by a live slot, with both halves available.
#[test]
fn a_sprite_owned_by_a_live_slot_names_the_slot_and_the_world_point() {
    let h = spawn_system("objat-whole", machine_with_sprite(), 64);
    let mut c = client(&h);
    load_listing(
        &mut c,
        "debug",
        &debug_rows(BASE, SPRITE_OWNER, (CAMERA_X, CAMERA_Y)),
    );
    // Slot 1's SST address, low word — DERIVED from the base and stride this listing declares.
    let slot1_word = (BASE + SST) & 0xFFFF;
    arm(&mut c, slot1_word, (96, 144), (CAMERA_X, CAMERA_Y));

    let r = at(&mut c, DOT);
    assert_eq!(
        r["dot"],
        json!({"x": DOT.0, "y": DOT.1}),
        "params echoed: {r}"
    );
    assert_eq!(r["winner"]["layer"], "sprite", "{r}");
    assert_eq!(r["winner"]["spriteIndex"], 0, "{r}");
    assert_eq!(r["owner"]["kind"], "object", "{r}");
    assert_eq!(r["owner"]["slot"], 1, "the owner word names slot 1: {r}");
    assert_eq!(r["owner"]["raw"], format!("0x{slot1_word:04X}"), "{r}");
    assert_eq!(r["worldSource"], "camera", "{r}");
    // act-world = UNBIASED camera + dot, the arithmetic §11.26 M3 pins.
    assert_eq!(
        r["world"],
        json!({"x": 96 + i64::from(DOT.0), "y": 144 + i64::from(DOT.1)}),
        "{r}"
    );
    assert!(r.get("layout").is_some(), "a ⚙ row carries its layout: {r}");
}

/// ⚑ **The ring sentinel, and it is the row that matters most.**
///
/// `DrawRings` stamps a bare `move.w #1`, never an address. A server that rebased `0x0001` would divide a
/// garbage offset and **confidently name the wrong object** — a wrong answer that looks exactly like a
/// right one. On the first real screen this was tried on, two of the three sprites were rings, so this is
/// the common case rather than a corner.
#[test]
fn the_ring_sentinel_is_not_rebased_into_a_slot() {
    let h = spawn_system("objat-ring", machine_with_sprite(), 64);
    let mut c = client(&h);
    load_listing(
        &mut c,
        "debug",
        &debug_rows(BASE, SPRITE_OWNER, (CAMERA_X, CAMERA_Y)),
    );
    arm(&mut c, 0x0001, (0, 0), (CAMERA_X, CAMERA_Y));

    let r = at(&mut c, DOT);
    assert_eq!(r["owner"]["kind"], "ring", "{r}");
    assert_eq!(
        r["owner"]["raw"], "0x0001",
        "the word is served for audit: {r}"
    );
    assert!(
        r["owner"].get("slot").is_none(),
        "slot is present iff kind == object; a rebased sentinel is the defect this row exists for: {r}"
    );
}

/// The mask sentinel, `0x0002` from `InsertSpriteMasks`. Same guard, different value.
#[test]
fn the_mask_sentinel_is_not_rebased_into_a_slot() {
    let h = spawn_system("objat-mask", machine_with_sprite(), 64);
    let mut c = client(&h);
    load_listing(
        &mut c,
        "debug",
        &debug_rows(BASE, SPRITE_OWNER, (CAMERA_X, CAMERA_Y)),
    );
    arm(&mut c, 0x0002, (0, 0), (CAMERA_X, CAMERA_Y));

    let r = at(&mut c, DOT);
    assert_eq!(r["owner"]["kind"], "mask", "{r}");
    assert_eq!(r["owner"]["raw"], "0x0002", "{r}");
    assert!(r["owner"].get("slot").is_none(), "{r}");
}

/// `none` — the table exists and this entry is `0x0000`. Nothing stamped this sprite this frame.
#[test]
fn an_unstamped_entry_is_none_and_still_carries_its_raw_word() {
    let h = spawn_system("objat-none", machine_with_sprite(), 64);
    let mut c = client(&h);
    load_listing(
        &mut c,
        "debug",
        &debug_rows(BASE, SPRITE_OWNER, (CAMERA_X, CAMERA_Y)),
    );
    arm(&mut c, 0x0000, (0, 0), (CAMERA_X, CAMERA_Y));

    let r = at(&mut c, DOT);
    assert_eq!(r["owner"]["kind"], "none", "{r}");
    assert_eq!(
        r["owner"]["raw"], "0x0000",
        "`raw` is absent ONLY for unavailable; `none` read a word and must show it: {r}"
    );
}

/// ⚑ **OBJECT-AT-CONFORMANCE row 1 — the release-ROM shape.**
///
/// A schema cannot check this: `{kind:"unavailable"}` and `{kind:"none", raw:"0x0000"}` are both valid
/// documents, so only a live check can tell a server that answered *no owner* from one that answered
/// *no table*. Merged, a caller cannot distinguish "this build cannot answer" from "the answer is no" —
/// and since a screen of rings legitimately produces `none` for every sprite, the merged shape lets a
/// picker silently report an empty world.
#[test]
fn a_build_without_the_owner_table_says_unavailable_not_none() {
    let h = spawn_system("objat-release", machine_with_sprite(), 64);
    let mut c = client(&h);
    // A RELEASE listing: no `Sprite_Owner` at all.
    load_listing(&mut c, "release", &release_rows(BASE));
    c.ok("emulator/pause", json!({}));

    let r = at(&mut c, DOT);
    assert_eq!(
        r["owner"]["kind"], "unavailable",
        "a shipped build has no owner table; answering `none` would claim the table answered: {r}"
    );
    assert!(
        r["owner"].get("raw").is_none(),
        "no table means there was no word to read — a `0x0000` here would be indistinguishable from a \
         real unstamped entry: {r}"
    );
    assert!(r["owner"].get("slot").is_none(), "{r}");
    // The halves are independent: the camera resolved on this same listing, so the world half answers.
    assert_eq!(
        r["worldSource"], "camera",
        "a build that can answer one half answers it, rather than refusing both: {r}"
    );
}

/// ⚑ **OBJECT-AT-CONFORMANCE row 2 — M3's normative re-resolve.**
///
/// Every address is resolved by symbol, per loaded build, on every call. This is the half with **no loud
/// failure**: unlike `Sprite_Owner`, whose absence announces itself, `Camera_X`/`Camera_Y` exist in both
/// build shapes and simply MOVE — so a server that remembered an address would read a plausible number
/// from the wrong place and land every click silently in the wrong spot.
///
/// The test swaps the listing under a running server and moves every address, then asserts the answer
/// followed. A cached address fails here and cannot fail anywhere else.
#[test]
fn addresses_are_resolved_again_after_a_rom_swap_never_remembered() {
    let h = spawn_system("objat-reresolve", machine_with_sprite(), 64);
    let mut c = client(&h);

    // Build one.
    load_listing(
        &mut c,
        "debug",
        &debug_rows(BASE, SPRITE_OWNER, (CAMERA_X, CAMERA_Y)),
    );
    arm(
        &mut c,
        (BASE + SST) & 0xFFFF,
        (96, 144),
        (CAMERA_X, CAMERA_Y),
    );
    let first = at(&mut c, DOT);
    assert_eq!(first["owner"]["slot"], 1, "{first}");
    assert_eq!(first["world"]["x"], 96 + i64::from(DOT.0), "{first}");

    // Build two: EVERY address moves — the pool, the owner table, and both cameras — and the values at
    // the new homes differ from the old ones, so a stale read is a wrong answer rather than a lucky one.
    let base2 = BASE + 0x400;
    let owner2 = SPRITE_OWNER + 0x40;
    let cam2 = (CAMERA_X + 0x80, CAMERA_Y + 0x80);
    load_listing(&mut c, "debug2", &debug_rows(base2, owner2, cam2));
    poke_word(&mut c, owner2, (base2 + 2 * SST) & 0xFFFF);
    poke_word(&mut c, cam2.0, 500);
    poke_word(&mut c, cam2.1, 600);

    let second = at(&mut c, DOT);
    assert_eq!(
        second["owner"]["slot"], 2,
        "the owner table was re-read at its NEW address: {second}"
    );
    assert_eq!(
        second["world"],
        json!({"x": 500 + i64::from(DOT.0), "y": 600 + i64::from(DOT.1)}),
        "the cameras were re-read at their NEW addresses — the half with no loud failure: {second}"
    );
}

// ---------------------------------------------------------------------------------------------------
// The two halves are independent, and the winner mirrors pixel_attribution
// ---------------------------------------------------------------------------------------------------

/// An absent camera reports `worldSource: "unavailable"` and omits `world` — while the owner half, which
/// resolved, still answers. §11.26 M3's independence, in the direction the release row does not cover.
#[test]
fn an_absent_camera_omits_world_while_the_owner_half_still_answers() {
    let h = spawn_system("objat-nocam", machine_with_sprite(), 64);
    let mut c = client(&h);
    // Owner table present, cameras absent.
    let mut rows = pool_rows(BASE, SST);
    rows.push(("Sprite_Owner".into(), SPRITE_OWNER));
    load_listing(&mut c, "nocam", &rows);
    arm(&mut c, 0x0001, (0, 0), (CAMERA_X, CAMERA_Y));

    let r = at(&mut c, DOT);
    assert_eq!(r["worldSource"], "unavailable", "{r}");
    assert!(
        r.get("world").is_none(),
        "`world` is omitted, never zeroed — a zero here is a coordinate a client would use: {r}"
    );
    assert_eq!(
        r["owner"]["kind"], "ring",
        "the owner half resolved and must still answer: {r}"
    );
}

/// A non-sprite winner carries no `spriteIndex`, and — the owner table being present — answers `none`.
#[test]
fn a_non_sprite_winner_has_no_sprite_index_and_owns_nothing() {
    let h = spawn_system("objat-plane", machine_with_no_sprite(), 64);
    let mut c = client(&h);
    load_listing(
        &mut c,
        "debug",
        &debug_rows(BASE, SPRITE_OWNER, (CAMERA_X, CAMERA_Y)),
    );
    arm(&mut c, 0xDEAD, (0, 0), (CAMERA_X, CAMERA_Y));

    let r = at(&mut c, DOT);
    assert_ne!(r["winner"]["layer"], "sprite", "{r}");
    assert!(
        r["winner"].get("spriteIndex").is_none(),
        "spriteIndex is present iff the winner is a sprite: {r}"
    );
    assert_eq!(r["owner"]["kind"], "none", "{r}");
    assert_eq!(
        r["owner"]["raw"], "0x0000",
        "no sprite means no entry was read; the table still exists, so this is `none`: {r}"
    );
}

/// An owner word that is not on a record boundary is refused rather than named. The word is still served
/// as `raw`, so the caller can audit exactly what we saw.
#[test]
fn an_owner_word_off_a_record_boundary_is_not_named_as_a_slot() {
    let h = spawn_system("objat-misaligned", machine_with_sprite(), 64);
    let mut c = client(&h);
    load_listing(
        &mut c,
        "debug",
        &debug_rows(BASE, SPRITE_OWNER, (CAMERA_X, CAMERA_Y)),
    );
    // One byte into slot 0's record: a real-looking address that names no record.
    arm(&mut c, (BASE & 0xFFFF) + 1, (0, 0), (CAMERA_X, CAMERA_Y));

    let r = at(&mut c, DOT);
    assert_eq!(
        r["owner"]["kind"], "none",
        "a word that is not a record address must not be divided into a slot index: {r}"
    );
    assert!(r["owner"].get("slot").is_none(), "{r}");
    assert_eq!(
        r["owner"]["raw"],
        format!("0x{:04X}", (BASE & 0xFFFF) + 1),
        "{r}"
    );
}

// ---------------------------------------------------------------------------------------------------
// Group membership and bounds
// ---------------------------------------------------------------------------------------------------

/// A ⚙ decoder-group member: no listing, `-32012`, not a guessed base.
#[test]
fn without_symbols_the_row_refuses_with_the_decoder_groups_own_code() {
    let h = spawn_system("objat-nosym", machine_with_sprite(), 64);
    let mut c = client(&h);
    let e = c.err("emulator/object_at", json!({"x": DOT.0, "y": DOT.1}));
    assert_eq!(
        e["code"], -32012,
        "the decoder group's code, inherited rather than restated: {e}"
    );
}

/// The active bound is `-32004` with the display in `error.data` — `pixel_attribution`'s exact treatment,
/// which is the point of sharing one parser.
#[test]
fn a_dot_outside_the_active_display_is_refused_with_the_display_size() {
    let h = spawn_system("objat-bounds", machine_with_sprite(), 64);
    let mut c = client(&h);
    load_listing(
        &mut c,
        "debug",
        &debug_rows(BASE, SPRITE_OWNER, (CAMERA_X, CAMERA_Y)),
    );
    let e = c.err("emulator/object_at", json!({"x": 400, "y": 10}));
    assert_eq!(e["code"], -32004, "{e}");
    assert!(e["data"]["width"].is_number(), "{e}");
    assert!(e["data"]["height"].is_number(), "{e}");
}

/// The two rows answer the SAME dot with the same winner. This is the anti-drift pin behind sharing one
/// coordinate parser: if the spaces ever diverge, these disagree.
#[test]
fn object_at_and_pixel_attribution_answer_one_dot_the_same_way() {
    let h = spawn_system("objat-parity", machine_with_sprite(), 64);
    let mut c = client(&h);
    load_listing(
        &mut c,
        "debug",
        &debug_rows(BASE, SPRITE_OWNER, (CAMERA_X, CAMERA_Y)),
    );
    c.ok("emulator/pause", json!({}));

    let a = at(&mut c, DOT);
    let p = c.ok(
        "emulator/pixel_attribution",
        json!({"x": DOT.0, "y": DOT.1}),
    );
    assert_eq!(
        a["winner"], p["winner"],
        "one dot, one winner — the two rows share a space and must not drift: {a} vs {p}"
    );
}
