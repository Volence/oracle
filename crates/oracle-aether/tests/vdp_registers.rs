//! **`emulator/read_vdp_registers`** — `protocol.md` §6 line 1388, unblocked and served 2026-09-05.
//!
//! Not a new method. The row has been catalogued since the catalog was written and sat in the schema's
//! BLOCKED set under audit D-20, because its own text spelled the result with two literal ellipses and
//! left `raw[]`'s length unstated. §11.41 (CR-R) removed both by **striking** — `decoded{}` is gone,
//! `status` is reduced to its `raw` word — so the fragment became writable with nothing invented, and the
//! handler became servable.
//!
//! # What the schema checks for free, and what it structurally cannot
//!
//! Every line a [`Client`] receives is validated against the vendored fragment closed with
//! `unevaluatedProperties: false` (`common::schema`, §8 item 15 / item 20). So a `raw[]` of 23 or 25
//! entries, an **unprefixed** hex byte, a `status` carrying a second key, a resurrected `decoded{}` and a
//! `caveat` the fragment declares absent all fail here **without an assertion in this file** — the first
//! three on the published fragment's own `minItems`/`maxItems`, `pattern` and `status`'s
//! `unevaluatedProperties: false`; the last two on item 20's closure, which lives in the harness and
//! never in the published artifact.
//!
//! ⚑ **Two things this paragraph used to claim that are NOT true, corrected 2026-09-05 rather than left
//! standing.** They are the exact failure mode this file exists to avoid — prose asserting coverage the
//! assertions do not have — so they are named instead of quietly edited out.
//!
//! 1. It said a **lower-case** hex byte fails here. It does not. The fragment's pattern is
//!    `^0x[0-9A-Fa-f]{2}$` (and `{4}` for `status.raw`), which is case-INSENSITIVE, and this file's own
//!    `decode_raw` checks the prefix, the length and `from_str_radix`, all three case-blind. A server
//!    emitting `"0x4a"` passes every row here and the schema besides. Whether upper case is even an
//!    obligation is unsettled — D9 category 1 does not state a case — so this is recorded as a
//!    NON-obligation rather than an uncovered one, and nothing is asserted about it either way.
//! 2. It credited the contract's **five vectors** with proving all five of those refusals bite. They
//!    prove two of them. The five are: a `params` pass on `{}`, a `params` fail on `{"reg": 4}`, a
//!    `result` pass, a `result` fail on 23 entries, and a `result` fail on a `status` key beyond `raw`
//!    (§11.41's own list, and `vectors.json`). The `decoded{}` and `caveat` refusals hold — through
//!    item 20's closure and `$defs/replyFields`, which declares the stamp and `droppedEvents` and no
//!    `caveat` — but no vector witnesses them, and `the_contracts_own_vectors_pass_and_fail_exactly_as_\
//!    declared` in `tests/schema_conformance.rs` is the anti-vacuity evidence for the two it covers and
//!    for nothing else.
//!
//! What that leaves is §8 **item 29**, which is the whole of what makes this method right rather than
//! merely well-shaped, and which no shape check can reach:
//!
//! > *A server advertising `emulator/read_vdp_registers` MUST answer it without moving the machine:
//! > calling it any number of times on a paused machine leaves `emulator/state_hash.combined`
//! > byte-identical, leaves the control-port write-pending toggle in the state it was in (a two-word
//! > control command begun before the call still completes as one after it), and leaves the
//! > sprite-overflow and collision latches set if they were set.*
//!
//! ⚑ **Item 29 was CLARIFIED upstream on 2026-09-05, on a finding from this very serve, and the
//! clarification is satisfied here already.** The added sentence: *"The three clauses are INDEPENDENT
//! and a harness must assert all three: the hash clause alone cannot see a violation, because
//! `state_hash` covers VRAM, CRAM, VSRAM and the register bytes and the toggle and latches live in none
//! of those (oracle's serve proved it: swapping the peek for a status-port read left the hash row
//! green)."* It also names game-agnostic recipes for the other two — write one control word so the
//! toggle is pending, call, write the second and assert the command completed as one; and with both
//! latches set, call twice and assert both bits still read set in `status.raw`. Those are the two rows
//! below, and each calls the method **four** times where the recipe asks for two. Nothing needed
//! extending; this note exists so a reader comparing the file against the amended text does not have to
//! re-derive that.
//!
//! A reply that corrupted the machine on its way out is perfectly conformant to the fragment. The three
//! rows below are the ones that can fail, on three separate fixtures, which is what "independent" means
//! in practice:
//!
//! * [`the_write_pending_toggle_survives_the_call`] — the sharpest, because it catches the single most
//!   likely implementation mistake, calling the `&mut self` `Vdp::control_read_status` where the `&self`
//!   `Vdp::status_word` was meant. That mistake is silent, timeline-corrupting, and would make the game's
//!   next control word parse as a fresh first word.
//! * [`the_sprite_overflow_and_collision_latches_survive_the_call`] — the same rule at a second site,
//!   which fails independently: a real `$C00004` read clears both.
//! * [`the_machine_does_not_move`] — item 29 asserted directly and with no game knowledge at all: call
//!   twice, hash, compare.
//!
//! # Where the expectations come from
//!
//! Every expected number is computed from the **core** — the `Vdp` this test posed, or
//! `oracle_core::render::plane_size`, or `oracle_core::state_hash::fnv1a_bytes` — on a machine built
//! here, and never read off a reply and re-asserted. The fixture ROM's instruction addresses are
//! constants of this file's own assembler, so "where should the PC be after five steps" has an answer
//! read off the layout rather than off the server.

mod common;

use common::{spawn_system, spawn_with, Client};
use oracle_core::render::{plane_size, LayerMask};
use oracle_core::state_hash::{fnv1a_bytes, REG_COUNT};
use oracle_core::system::System;
use oracle_core::vdp::Vdp;
use serde_json::{json, Value};

const METHOD: &str = "emulator/read_vdp_registers";

fn client(h: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(h);
    c.handshake(false);
    c
}

/// The reply's `raw[]`, decoded back to the 24 bytes it publishes.
///
/// Strict on purpose: the entries are asserted to be `0x` + exactly two hex digits *here* as well as in
/// the fragment, because this helper is what every value assertion below reads through, and a decoder
/// that quietly accepted `"4"` would let a wrong wire spelling pass every one of them.
fn decode_raw(reply: &Value) -> Vec<u8> {
    let arr = reply["raw"].as_array().expect("`raw` is an array");
    assert_eq!(
        arr.len(),
        REG_COUNT,
        "the fragment pins raw[] at REG_COUNT entries"
    );
    arr.iter()
        .enumerate()
        .map(|(i, v)| {
            let s = v.as_str().unwrap_or_else(|| panic!("raw[{i}] is a string"));
            let digits = s
                .strip_prefix("0x")
                .unwrap_or_else(|| panic!("raw[{i}] = {s:?} does not carry the 0x prefix (D9)"));
            assert_eq!(digits.len(), 2, "raw[{i}] = {s:?} is not two hex digits");
            u8::from_str_radix(digits, 16).unwrap_or_else(|_| panic!("raw[{i}] = {s:?} is not hex"))
        })
        .collect()
}

fn status_word(reply: &Value) -> u16 {
    let s = reply["status"]["raw"]
        .as_str()
        .expect("status.raw is a string");
    let digits = s
        .strip_prefix("0x")
        .unwrap_or_else(|| panic!("status.raw = {s:?} does not carry the 0x prefix"));
    assert_eq!(digits.len(), 4, "status.raw = {s:?} is not four hex digits");
    u16::from_str_radix(digits, 16).expect("status.raw is hex")
}

// ---------------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------------

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

/// **Toy Story's floor configuration**, posed by hand from `docs/2026-09-05-toystory-floor-recon.md`.
///
/// That recon ran against the owner's live window on a commercial ROM this repo does not hold, so the
/// machine cannot be reproduced. What CAN be reproduced, and is what the recon actually cost four probes
/// and one rejected candidate to establish, is the **register configuration** it derived the long way:
/// plane B's nametable at `0xC000` (inferred from a unique-tile-sequence match) and a 64-cell plane width
/// (inferred from nametable rows sitting 128 bytes apart). Both are one register each.
///
/// So the fixture sets those two registers and then **proves they mean what the recon said they mean**,
/// by rendering: the tile written at `0xC000` must be the dot at screen (0,0), and the tile written 128
/// bytes further on must be the dot at screen (0,8). A 32-cell plane would put row 1 at `0xC040` and show
/// backdrop there instead, so the second assertion is a real discrimination and not a restatement.
const TOY_TILE_ROW0: u16 = 0x11;
const TOY_TILE_ROW1: u16 = 0x12;
/// Reg `$04` = 6: plane B nametable base = `(6 & 0x07) << 13` = `0xC000` (recon RR3).
const TOY_REG_04: u8 = 0x06;
/// Reg `$10` = 1: HSZ = 1, VSZ = 0 — a 64 x 32 cell plane.
const TOY_REG_10: u8 = 0x01;
const TOY_PLANE_B_BASE: u16 = 0xC000;

fn toystory_machine() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    let v = sys.vdp_mut();
    v.vram_mut().fill(0);
    // Reg $01 first: while M5 is clear the register mask discards writes above register 10, so a $10
    // written ahead of it is silently dropped. (The trap `tests/pixel_attribution.rs` documents.)
    set_reg(v, 0x01, 0x74); // display on, mode 5, DMA enable
    set_reg(v, 0x0C, 0x81); // H40
    set_reg(v, 0x02, 0x20); // plane A nametable @ $8000, out of the way
    set_reg(v, 0x03, 0x28); // window nametable @ $A000
    set_reg(v, 0x04, TOY_REG_04);
    set_reg(v, 0x05, 0x58); // SAT @ $B000 (empty)
    set_reg(v, 0x07, 0x04); // backdrop = CRAM entry 4
    set_reg(v, 0x0B, 0x00); // full h + full v scroll
    set_reg(v, 0x0D, 0x20); // h-scroll table @ $8000
    set_reg(v, 0x0F, 0x02); // autoincrement 2
    set_reg(v, 0x10, TOY_REG_10);
    set_reg(v, 0x11, 0x00); // no window
    set_reg(v, 0x12, 0x00);

    write_vram(v, TOY_TILE_ROW0 * 32, &[0x1111; 16]);
    write_vram(v, TOY_TILE_ROW1 * 32, &[0x2222; 16]);
    write_cram(v, 1, 0x000E); // red
    write_cram(v, 2, 0x0E00); // blue
    write_cram(v, 4, 0x0EEE); // white — backdrop, distinct from both tiles

    // Cell (0,0) and cell (0,1) of plane B. The second offset is 64 cells * 2 bytes = 128, which is the
    // row spacing the recon measured and attributed to a 64-cell plane.
    write_vram(v, TOY_PLANE_B_BASE, &[TOY_TILE_ROW0]);
    write_vram(v, TOY_PLANE_B_BASE + 128, &[TOY_TILE_ROW1]);
    sys
}

/// A machine with the sprite-overflow and sprite-collision status latches **set**, through the VDP's own
/// commit path (`Vdp::commit_scanline_sprites`, the function the renderer calls) rather than by reaching
/// into the fields. Both are sticky until a `$C00004` status read clears them, which is exactly the
/// clearing this method must not do.
fn latched_machine() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    sys.vdp_mut().commit_scanline_sprites(false, true, true);
    sys
}

// ---------------------------------------------------------------------------------------------------
// The split-control-word fixture ROM — the machine for the write-pending toggle row
// ---------------------------------------------------------------------------------------------------

/// PC of `move.w #$4100,(a0)`: the **first** word of a two-word VRAM-write command. Arms the toggle.
const SPLIT_FIRST_WORD_PC: u32 = 0x0000_0214;
/// PC of `move.w #$0000,(a0)`: the **second** word. Where the machine sits while the method is called.
const SPLIT_SECOND_WORD_PC: u32 = 0x0000_0218;
/// Instructions from reset to [`SPLIT_SECOND_WORD_PC`]: two `lea`s and three `move.w`s.
const SPLIT_STEPS_TO_ARMED: u64 = 5;
/// The VRAM byte address the completed command targets, and the word it writes there.
const SPLIT_TARGET: u32 = 0x0100;
const SPLIT_WORD: u16 = 0xBEEF;

fn put_word(rom: &mut [u8], at: usize, w: u16) {
    rom[at..at + 2].copy_from_slice(&w.to_be_bytes());
}

fn put_long(rom: &mut [u8], at: usize, v: u32) {
    rom[at..at + 4].copy_from_slice(&v.to_be_bytes());
}

/// **A ROM that splits a two-word VDP control command across the call under test.**
///
/// It points `a0` at the control port and `a1` at the data port, enables mode 5, sets autoincrement 2,
/// and then issues the two halves of a VRAM-write command at `$0100` as two **separate** instructions,
/// followed by the data write. Stepping to [`SPLIT_SECOND_WORD_PC`] leaves the machine with the
/// write-pending toggle **armed** and one instruction still to go, which is the state §8 item 29 names.
///
/// The discrimination this buys: if the call clears the toggle, `$0000` is re-read as a *first* word
/// (CD1-CD0 = 00, A13-A0 = 0), the command targets `$0000` instead of `$0100`, and `SPLIT_WORD` never
/// reaches `SPLIT_TARGET`. Nothing else in the ROM can produce that outcome.
fn split_control_word_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x230];
    put_long(&mut rom, 0x0, 0x00FF_FFFE); // reset SSP
    put_long(&mut rom, 0x4, 0x0000_0200); // reset PC

    put_word(&mut rom, 0x200, 0x41F9); // lea $00C00004,a0   — the control port
    put_long(&mut rom, 0x202, 0x00C0_0004);
    put_word(&mut rom, 0x206, 0x43F9); // lea $00C00000,a1   — the data port
    put_long(&mut rom, 0x208, 0x00C0_0000);
    put_word(&mut rom, 0x20C, 0x30BC); // move.w #$8174,(a0) — reg 1: display on, M5, DMA enable
    put_word(&mut rom, 0x20E, 0x8174);
    put_word(&mut rom, 0x210, 0x30BC); // move.w #$8F02,(a0) — reg 15: autoincrement 2
    put_word(&mut rom, 0x212, 0x8F02);
    put_word(&mut rom, SPLIT_FIRST_WORD_PC as usize, 0x30BC); // move.w #$4100,(a0)
    put_word(&mut rom, SPLIT_FIRST_WORD_PC as usize + 2, 0x4100);
    put_word(&mut rom, SPLIT_SECOND_WORD_PC as usize, 0x30BC); // move.w #$0000,(a0)
    put_word(&mut rom, SPLIT_SECOND_WORD_PC as usize + 2, 0x0000);
    put_word(&mut rom, 0x21C, 0x32BC); // move.w #$BEEF,(a1) — the data write
    put_word(&mut rom, 0x21E, SPLIT_WORD);
    put_word(&mut rom, 0x220, 0x60FE); // bra.s *
    rom
}

// ---------------------------------------------------------------------------------------------------
// Shape and values — the half the schema mostly covers, with the joins it cannot
// ---------------------------------------------------------------------------------------------------

/// The reply publishes the machine's **own** register file, byte for byte, in index order.
///
/// The expectation is the `Vdp` this test posed — read out of an identical `System` built here — and not
/// a table of literals that would drift the moment the fixture changed.
#[test]
fn the_reply_is_the_machines_own_register_file_in_index_order() {
    let expected: Vec<u8> = toystory_machine().vdp().regs().to_vec();
    let h = spawn_system("vdpreg-file", toystory_machine(), 1024);
    let mut c = client(&h);

    let r = c.ok(METHOD, json!({}));
    let got = decode_raw(&r);

    assert_eq!(got.len(), REG_COUNT);
    assert_eq!(
        got, expected,
        "raw[] must be the register file the core holds, index-ordered 0 to 23"
    );
    // The one register that is not the same in two consecutive slots, so an off-by-one in the ordering
    // could not pass the vector comparison above by coincidence.
    assert_eq!(got[0x04], TOY_REG_04);
    assert_eq!(got[0x10], TOY_REG_10);
}

/// **The cross-surface join, and the reason `REG_COUNT` is not a literal anywhere in the handler.**
///
/// `emulator/state_hash.regs` is FNV-1a-64 over the 24 register bytes, and it is frozen currency: it is
/// byte-compatible with Oracle's `OpStateHash` and the differential harness compares it. So decoding this
/// reply's `raw[]` and hashing it must reproduce that fingerprint exactly. If the two ever disagreed, one
/// of them would be lying about the same 24 bytes — and this is the only assertion in the suite that can
/// tell which surfaces are reading the same array.
#[test]
fn the_published_registers_hash_to_the_frozen_state_hash_currency() {
    let h = spawn_system("vdpreg-join", toystory_machine(), 1024);
    let mut c = client(&h);

    let bytes = decode_raw(&c.ok(METHOD, json!({})));
    // `state_hash::hex` rather than a `format!` typed here: the wire spelling of a fingerprint is the
    // currency's, and a second copy of it is a place for the two to drift.
    let hashed = oracle_core::state_hash::hex(fnv1a_bytes(&bytes));
    let regs = c.ok("emulator/state_hash", json!({}))["regs"]
        .as_str()
        .expect("state_hash.regs")
        .to_string();

    assert_eq!(
        hashed, regs,
        "raw[] hashed FNV-1a must equal state_hash.regs — the two surfaces publish one array"
    );
}

/// `status.raw` is the VDP's status word at the machine's own now, computed by the non-mutating
/// accessor. Derived from the core on an identical machine, at the same instant the server would use.
#[test]
fn the_status_word_is_the_chips_own() {
    let sys = toystory_machine();
    let expected = sys.vdp().status_word(sys.scheduler().now());

    let h = spawn_system("vdpreg-status", toystory_machine(), 1024);
    let mut c = client(&h);
    assert_eq!(status_word(&c.ok(METHOD, json!({}))), expected);
}

// ---------------------------------------------------------------------------------------------------
// §8 item 29 — the peek rows, the half that can fail
// ---------------------------------------------------------------------------------------------------

/// **The sharpest row: a two-word control command begun before the call still completes as one after it.**
///
/// This is item 29's own worked example and it catches the single most likely implementation mistake —
/// calling `Vdp::control_read_status` (which takes `&mut self` and clears the toggle, the FIFO's read
/// path and both sprite latches) where `Vdp::status_word` (`&self`) was meant.
///
/// The fixture ROM issues the two halves of a VRAM-write command as separate instructions. Stepping to
/// [`SPLIT_SECOND_WORD_PC`] leaves the toggle armed; the method is then called twice; the remaining two
/// instructions complete the command and write [`SPLIT_WORD`] through the data port. If the toggle had
/// been cleared, `$0000` would be re-read as a *first* control word and the write would land at `$0000`,
/// so `SPLIT_TARGET` would still be zero.
#[test]
fn the_write_pending_toggle_survives_the_call() {
    let h = spawn_with("vdpreg-toggle", split_control_word_rom(), 1024);
    let mut c = client(&h);

    let stepped = c.ok("emulator/step", json!({"count": SPLIT_STEPS_TO_ARMED}));
    assert_eq!(
        stepped["pc"],
        json!(format!("0x{SPLIT_SECOND_WORD_PC:08X}")),
        "fixture precondition: {SPLIT_STEPS_TO_ARMED} steps must land on the SECOND control word, with \
         the toggle armed by the first — if this moved, the ROM layout changed and the row below is \
         asserting nothing"
    );

    // The call under test, twice: item 29 says "any number of times".
    c.ok(METHOD, json!({}));
    c.ok(METHOD, json!({}));

    // The second control word, then the data write.
    c.ok("emulator/step", json!({"count": 2}));

    let vram = c.ok(
        "emulator/read_vram",
        json!({"addr": format!("0x{SPLIT_TARGET:08X}"), "len": 2}),
    );
    assert_eq!(
        vram["bytes"],
        json!(format!("0x{SPLIT_WORD:04X}")),
        "the two-word command did not complete as one: reading the registers cleared the control-port \
         write-pending toggle, so the second word was parsed as a fresh first word and the data write \
         went somewhere else. This is §8 item 29's own example, and the cause is calling the &mut self \
         control_read_status where the &self status_word was meant."
    );
}

/// **The same rule at a second site, and it fails independently.** A real `$C00004` read clears the
/// sprite-overflow (bit 6) and collision (bit 5) latches. This method clears nothing, so both bits must
/// still be set on the second call.
#[test]
fn the_sprite_overflow_and_collision_latches_survive_the_call() {
    const OVERFLOW: u16 = 1 << 6;
    const COLLISION: u16 = 1 << 5;

    let h = spawn_system("vdpreg-latch", latched_machine(), 1024);
    let mut c = client(&h);

    let first = status_word(&c.ok(METHOD, json!({})));
    assert_eq!(
        first & (OVERFLOW | COLLISION),
        OVERFLOW | COLLISION,
        "fixture precondition: both latches must be set before the rule can be tested"
    );

    for call in 2..=4 {
        let s = status_word(&c.ok(METHOD, json!({})));
        assert_eq!(
            s & (OVERFLOW | COLLISION),
            OVERFLOW | COLLISION,
            "call {call} lost a sticky status latch: a $C00004 read clears these and this method must \
             not (§8 item 29)"
        );
    }
}

/// **Item 29 asserted directly, with no game knowledge at all: call twice, hash, compare.**
///
/// `state_hash.combined` is one FNV-1a stream over VRAM + CRAM + VSRAM + the registers, so it moves if
/// the call touched any of them. Four calls rather than two, because "any number of times" is the rule
/// and a handler that mutated on first use only would pass a two-call check.
#[test]
fn the_machine_does_not_move() {
    let h = spawn_system("vdpreg-still", toystory_machine(), 1024);
    let mut c = client(&h);

    let before = c.ok("emulator/state_hash", json!({}))["combined"].clone();
    for _ in 0..4 {
        c.ok(METHOD, json!({}));
    }
    let after = c.ok("emulator/state_hash", json!({}))["combined"].clone();

    assert_eq!(
        before, after,
        "state_hash.combined moved across four calls on a paused machine (§8 item 29)"
    );
    assert!(before.is_string(), "the control read a real fingerprint");
}

/// **Never refused on a free-running machine** (§11.41 M2), on the `read`/`read_cram`/`sprites`/
/// `pixel_attribution`/`scanlines` precedent. The reply still carries the D11 stamp, which the schema
/// checks; what is asserted here is that the stamp says the machine was *running*, so this is not the
/// paused path wearing a different name.
#[test]
fn it_is_not_refused_on_a_free_running_machine() {
    let h = spawn_system("vdpreg-free", toystory_machine(), 1024);
    let mut c = client(&h);

    c.ok("emulator/resume", json!({}));
    let r = c.ok(METHOD, json!({}));
    assert_eq!(
        r["running"],
        json!(true),
        "the call was answered, but on a machine the stamp says was paused — the row's freedom from \
         §6's run-control state rule is untested unless the machine really is running"
    );
    assert_eq!(decode_raw(&r).len(), REG_COUNT);
    c.ok("emulator/pause", json!({}));
}

// ---------------------------------------------------------------------------------------------------
// The refusals
// ---------------------------------------------------------------------------------------------------

/// **A guessed selection param is refused, not silently ignored** (§2.5, §8 item 22, §11.41 M4).
///
/// The row takes no params. Without this, a client that reaches for `{"reg": 4}` gets the whole file back
/// and believes its filter worked — the confidently-wrong answer §4 exists to prevent, and the same
/// failure §11.17 was written for.
#[test]
fn a_guessed_selection_param_is_refused() {
    let h = spawn_system("vdpreg-closed", toystory_machine(), 1024);
    let mut c = client(&h);

    for bad in [
        json!({"reg": 4}),
        json!({"space": "vdp"}),
        json!({"index": 0}),
    ] {
        let e = c.err(METHOD, bad.clone());
        assert_eq!(e["code"], json!(-32602), "{bad} must be -32602");
    }
    // The control: the empty object is accepted, so the refusals above are about the key and not about
    // params being rejected wholesale.
    c.ok(METHOD, json!({}));
}

/// **Read only** (§11.41 M4): there is no write counterpart, and `-32601` stands.
#[test]
fn there_is_no_write_counterpart() {
    let h = spawn_system("vdpreg-readonly", toystory_machine(), 1024);
    let mut c = client(&h);
    let e = c.err("emulator/write_vdp_registers", json!({}));
    assert_eq!(e["code"], json!(-32601));
}

// ---------------------------------------------------------------------------------------------------
// The regression the recon paid for
// ---------------------------------------------------------------------------------------------------

/// **One call settles what `docs/2026-09-05-toystory-floor-recon.md` derived the long way.**
///
/// That pass could read no register, so it recovered plane B's nametable base by matching a unique
/// tile sequence in VRAM, and the plane's 64-cell width from nametable rows sitting 128 bytes apart. It
/// cost four probes and one candidate that *"looks exactly like a floor table"* and was the nametable —
/// refuted only by a phase test. Its own closing line names `$04` and `$10` as two of the four registers
/// it wanted and could not read.
///
/// The fixture poses those two registers as the recon reported them, then does two things a
/// value-comparison alone would not:
///
/// 1. **Proves the decode against the renderer**, so `raw[0x04] = 0x06` really does mean a nametable at
///    `0xC000` on this machine, and `raw[0x10] = 0x01` really does mean a 128-byte row stride. A 32-cell
///    plane would put row 1 at `0xC040` and show the backdrop at screen (0,8) instead.
/// 2. **Derives the width from `oracle_core::render::plane_size`**, the same function the renderer uses,
///    rather than restating `(reg >> 2 & 3) + 1` in a second place where it could drift.
#[test]
fn toy_storys_floor_registers_are_settled_by_one_call() {
    // Step 1, before the server exists: the renderer's own verdict on this configuration.
    let sys = toystory_machine();
    let row0 = sys.vdp().render_line_masked(0, LayerMask::ALL);
    let row1 = sys.vdp().render_line_masked(8, LayerMask::ALL);
    let backdrop = *sys
        .vdp()
        .render_line_masked(0, LayerMask::ALL)
        .last()
        .expect("a rendered line");
    assert_ne!(
        row0[0], backdrop,
        "the cell written at 0xC000 must be visible at (0,0) — otherwise reg $04 does not name that base \
         and the readback below is being compared against a claim nothing backs"
    );
    assert_ne!(
        row1[0], backdrop,
        "the cell written 128 bytes past 0xC000 must be visible at (0,8) — this is what a 64-cell row \
         stride means, and a 32-cell plane would show backdrop here"
    );
    assert_ne!(
        row0[0], row1[0],
        "the two rows must draw different tiles, or the stride assertion above could not have failed"
    );

    // Step 2: the same two facts, from one call.
    let h = spawn_system("vdpreg-toystory", toystory_machine(), 1024);
    let mut c = client(&h);
    let raw = decode_raw(&c.ok(METHOD, json!({})));

    let plane_b_base = (u32::from(raw[0x04] & 0x07)) << 13;
    assert_eq!(
        plane_b_base,
        u32::from(TOY_PLANE_B_BASE),
        "raw[0x04] must yield plane B's nametable base — the recon's four-probe answer, in one read"
    );

    let (width_cells, _height_cells) = plane_size(raw[0x10]);
    assert_eq!(
        width_cells, 64,
        "raw[0x10] must yield the 64-cell plane width the recon inferred from a 128-byte row spacing"
    );
    assert_eq!(
        u32::from(width_cells) * 2,
        128,
        "and 64 cells is the 128-byte nametable row stride the recon actually measured"
    );
}

// ---------------------------------------------------------------------------------------------------
// The advertisement
// ---------------------------------------------------------------------------------------------------

/// The row is advertised, and its declared param set is empty — the §6 row's own dash.
///
/// `tests/params_closure.rs` already ties every advertised method's `params` to its fragment by parse;
/// this is the narrower claim that the row is in `METHODS` at all, so a serve that was reverted while
/// the schema stayed would be caught here rather than only in the coverage pin.
#[test]
fn the_row_is_advertised_with_no_params() {
    let spec = oracle_aether::engine::METHODS
        .iter()
        .find(|m| m.name == METHOD)
        .expect("emulator/read_vdp_registers is advertised");
    assert!(
        spec.params.is_empty(),
        "the §6 row's params column is a dash; {:?} would be an invention",
        spec.params
    );
}
