//! `emulator/sprites` — `protocol.md` §6 (VRAM / CRAM / layers), adopted as CR-18
//! (`docs/2026-08-15-cr18-sprite-table.md`, ruled in `docs/2026-08-16-ruling-cr18.md`, §11.10).
//!
//! Every test here is a **wire** round trip, so `common::Client::recv` validates each received line
//! against the vendored contract schema on the way past — driving the method *is* the schema-conformance
//! pin, and it cannot be forgotten.
//!
//! The three cases the ruling made adoption **conditional** on are the first three below: an H32 reply
//! carrying `parsedMax: 64`, a `limit`-truncated reply carrying all three counters, and the exact key set
//! (§8 item 20's closure, which the schema's own `result` cannot catch because it has no
//! `additionalProperties: false`).
//!
//! The rest pin the four normative behaviours §11.10 states: slot order and `total: 80`, `parsedMax`
//! reported rather than derived, `cacheDivergence` present even when false, and a pure read that emits no
//! `caveat` on a running machine.

mod common;

use common::{spawn_system, Client};
use oracle_core::system::System;
use oracle_core::vdp::Vdp;
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// SAT base: reg 5 = $58 → `($58 & $7E) << 9` = $B000 in H40, and `($58 & $7F) << 9` = $B000 in H32 too,
/// so the two fixtures below differ in mode and in nothing else.
const SAT_BASE: u16 = 0xB000;
const BASE_TILE: u16 = 0x123;

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

/// One sprite in slot 0: 2x3 cells at screen (72, 128), palette 2, hflip, priority, link 3.
///
/// `h40` picks the mode, and it is the only difference between the two fixtures — which is what makes the
/// `parsedMax` test a controlled comparison rather than two unrelated machines.
fn machine(h40: bool) -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    let v = sys.vdp_mut();
    v.vram_mut().fill(0);
    // Reg $01 FIRST: the mode-4 register mask discards writes to registers above 10 while M5 is clear, so
    // an $0C written ahead of it is silently dropped and the fixture comes up H32 while claiming H40.
    set_reg(v, 0x01, 0x74); // display on, mode 5, DMA enable
    set_reg(v, 0x0C, if h40 { 0x81 } else { 0x00 });
    set_reg(v, 0x05, 0x58); // SAT base $B000
    set_reg(v, 0x0F, 0x02); // autoincrement 2
                            // Y=0x0100 (screen 128), size 2x3 → bits 3-2 = w-1 = 1, bits 1-0 = h-1 = 2 → 0x06;
                            // link 3; attr = priority | palette 2 | hflip | tile $123; X=0x00C8 (screen 72).
    let attr: u16 = 0x8000 | (2 << 13) | (1 << 11) | BASE_TILE;
    write_vram(v, SAT_BASE, &[0x0100, (0x06 << 8) | 3, attr, 0x00C8]);
    sys
}

fn sprites(c: &mut Client, params: Value) -> Value {
    c.ok("emulator/sprites", params)
}

// -------------------------------------------------------------------------------------------------
// The three conditions the ruling attached to adoption
// -------------------------------------------------------------------------------------------------

/// **Adoption condition 1.** The whole point of the field: in H32 the hardware parses 64 of the 80 slots,
/// and the reply says so instead of presenting 16 non-sprites as sprites.
#[test]
fn parsed_max_is_eighty_in_h40_and_sixty_four_in_h32() {
    let h = spawn_system("sprites-h40", machine(true), 64);
    let mut c = Client::connect(&h);
    c.handshake(false);
    let r = sprites(&mut c, json!({}));
    assert_eq!(r["parsedMax"], json!(80), "H40 parses 80 slots");
    assert_eq!(
        r["total"],
        json!(80),
        "and all 80 are still returned — parsedMax is a cap, not a clip"
    );

    let h = spawn_system("sprites-h32", machine(false), 64);
    let mut c = Client::connect(&h);
    c.handshake(false);
    let r = sprites(&mut c, json!({}));
    assert_eq!(r["parsedMax"], json!(64), "H32 parses 64 slots");
    assert_eq!(
        r["returned"], 80,
        "the 16 slots past the cap are RETURNED anyway: they are really in VRAM, and clipping them \
         would hide bytes the caller asked about"
    );
}

/// **Adoption condition 2.** A truncated page carries all three counters, `truncated` included — §2.4
/// clause (a) requires it even when it would be obvious.
#[test]
fn a_limit_truncates_and_carries_every_counter() {
    let h = spawn_system("sprites-limit", machine(true), 64);
    let mut c = Client::connect(&h);
    c.handshake(false);
    let r = sprites(&mut c, json!({"limit": 8}));
    assert_eq!(r["returned"], 8);
    assert_eq!(r["limit"], 8);
    assert_eq!(r["truncated"], json!(true));
    assert_eq!(
        r["total"],
        json!(80),
        "total is the TABLE's size, never the page's and never parsedMax"
    );
    assert_eq!(r["sprites"].as_array().unwrap().len(), 8);

    // The complete page reports the same three keys with truncated=false, not by omitting them.
    let full = sprites(&mut c, json!({"limit": 80}));
    assert_eq!(full["truncated"], json!(false));
    assert_eq!(full["returned"], 80);
}

/// **Adoption condition 3.** The exact key set. The schema's `result` has no `additionalProperties:
/// false` (it cannot: the envelope stamp arrives through `allOf`), so a surplus key passes the validator
/// and only this assertion catches it — the §8 item-20 closure, applied at the one place it binds.
#[test]
fn the_key_set_is_exact() {
    let h = spawn_system("sprites-keys", machine(true), 64);
    let mut c = Client::connect(&h);
    c.handshake(false);
    let r = sprites(&mut c, json!({}));
    let got: BTreeSet<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
    // The stamp (§D11) plus this row's six.
    let want: BTreeSet<&str> = [
        "sprites",
        "satBase",
        "parsedMax",
        "total",
        "returned",
        "limit",
        "truncated",
        "frame",
        "mclk",
        "running",
        "droppedEvents",
    ]
    .into_iter()
    .collect();
    assert_eq!(got, want, "no surplus keys, and none missing");

    let entry = &r["sprites"][0];
    let got: BTreeSet<&str> = entry
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let want: BTreeSet<&str> = [
        "index",
        "x",
        "y",
        "widthCells",
        "heightCells",
        "link",
        "baseTile",
        "palette",
        "hflip",
        "vflip",
        "priority",
        "cacheDivergence",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        got, want,
        "per-entry key set is exact — in particular NO satAddr, which is satBase + index*8 and was \
         struck as a derivable key"
    );
}

// -------------------------------------------------------------------------------------------------
// The normative behaviours §11.10 pins
// -------------------------------------------------------------------------------------------------

/// Slot order, index-ascending — pinned because the SAT's *other* reading is link-ordered, which makes
/// this the one place two conformant servers could differ defensibly. The fixture's slot 0 links to 3, so
/// a link-ordered server would put slot 3 second and this test would catch it.
#[test]
fn the_table_is_in_slot_order_never_link_order() {
    let h = spawn_system("sprites-order", machine(true), 64);
    let mut c = Client::connect(&h);
    c.handshake(false);
    let r = sprites(&mut c, json!({}));
    let list = r["sprites"].as_array().unwrap();
    assert_eq!(list.len(), 80);
    for (i, s) in list.iter().enumerate() {
        assert_eq!(s["index"], json!(i), "entry {i} must be slot {i}");
    }
    assert_eq!(
        list[0]["link"],
        json!(3),
        "slot 0 links to 3, so this fixture can tell slot order from link order"
    );
}

/// The decoded fields, against the values written into the fixture's attribute entry.
#[test]
fn the_decode_matches_what_was_written() {
    let h = spawn_system("sprites-decode", machine(true), 64);
    let mut c = Client::connect(&h);
    c.handshake(false);
    let r = sprites(&mut c, json!({}));
    let s = &r["sprites"][0];
    assert_eq!(s["y"], json!(128), "screen Y = Yfield - 128");
    assert_eq!(s["x"], json!(72), "screen X = Xfield - 128");
    assert_eq!(s["widthCells"], json!(2));
    assert_eq!(s["heightCells"], json!(3));
    assert_eq!(s["baseTile"], json!(BASE_TILE), "the spelling is baseTile");
    assert_eq!(s["palette"], json!(2));
    assert_eq!(s["hflip"], json!(true));
    assert_eq!(s["vflip"], json!(false));
    assert_eq!(s["priority"], json!(true));
    assert_eq!(
        r["satBase"],
        json!("0x0000B000"),
        "satBase is a hex string (D9 category 1), emitted once instead of eighty satAddrs"
    );
}

/// `cacheDivergence` is present on **every** entry, `false` included. A field that appeared only in the
/// unusual case would be a field nobody reads.
#[test]
fn cache_divergence_is_present_even_when_false() {
    let h = spawn_system("sprites-coherent", machine(true), 64);
    let mut c = Client::connect(&h);
    c.handshake(false);
    let r = sprites(&mut c, json!({}));
    for (i, s) in r["sprites"].as_array().unwrap().iter().enumerate() {
        assert!(
            s.get("cacheDivergence").is_some(),
            "entry {i} must carry cacheDivergence"
        );
    }
    assert_eq!(
        r["sprites"][0]["cacheDivergence"],
        json!(false),
        "cache and VRAM agree in a fixture that wrote the SAT through the data port"
    );
}

/// And it reports `true` when the cached half really has gone stale: moving the SAT base leaves the
/// cached Y/size/link describing the old table while X and the attribute word are read from the new one.
#[test]
fn cache_divergence_reports_a_stale_cache() {
    let mut sys = machine(true);
    let v = sys.vdp_mut();
    // A second, different table at $A000, then point reg 5 at it without refilling the cache.
    write_vram(v, 0xA000, &[0x0140, (0x0F << 8) | 9, 0x4567, 0x0100]);
    set_reg(v, 0x05, 0x50); // ($50 & $7E) << 9 = $A000

    let h = spawn_system("sprites-stale", sys, 64);
    let mut c = Client::connect(&h);
    c.handshake(false);
    let r = sprites(&mut c, json!({}));
    assert_eq!(
        r["satBase"],
        json!("0x0000A000"),
        "the base moved, so the VRAM half is read from the new table"
    );
    assert_eq!(
        r["sprites"][0]["cacheDivergence"],
        json!(true),
        "the cached Y/size/link still describe the old table — the difference between 'the game wrote \
         the sprite' and 'the VDP will draw the sprite'"
    );
}

/// A pure read: it answers a free-running machine rather than refusing with `-32005`, and it emits no
/// `caveat` — the stamp's `running` is the contract's whole answer to a torn sample.
#[test]
fn it_answers_a_running_machine_and_emits_no_caveat() {
    let h = spawn_system("sprites-running", machine(true), 64);
    let mut c = Client::connect(&h);
    c.handshake(false);
    c.ok("emulator/resume", json!({}));
    let r = sprites(&mut c, json!({}));
    assert_eq!(r["running"], json!(true), "answered while free-running");
    assert!(
        r.get("caveat").is_none(),
        "no caveat: §2.4 rule 4 makes its absence a decision, and `running` already says the sample \
         may be torn"
    );
    assert_eq!(r["total"], json!(80));
}

/// `limit` is bounded at the table's own size in both directions — a page that could never be filled is
/// a policy wearing a count's name.
#[test]
fn limit_is_bounded_at_the_tables_own_size() {
    let h = spawn_system("sprites-limit-bounds", machine(true), 64);
    let mut c = Client::connect(&h);
    c.handshake(false);
    for bad in [json!(0), json!(81), json!("8"), json!(-1)] {
        let e = c.err("emulator/sprites", json!({ "limit": bad }));
        assert_eq!(
            e["code"], -32602,
            "limit {bad} must be refused, not clamped"
        );
    }
    let ok = sprites(&mut c, json!({"limit": 1}));
    assert_eq!(ok["returned"], 1);
    assert_eq!(ok["truncated"], json!(true));
}
