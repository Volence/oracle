//! **`emulator/object_list`, `emulator/player_state`, `emulator/object_slot`** — `protocol.md` §6's ⚙
//! decoder group, schematized 2026-08-26 by §11.25 (CR-D), served here.
//!
//! Every reply goes past the vendored schema on the way in (`common::Client`), closed with item 20's
//! `unevaluatedProperties`, so an unknown key is a red test rather than a shape nobody sampled. What this
//! file adds on top is the half a fragment cannot express — §11.25 lists five such obligations — plus the
//! two structural conditionals the adjudication kept catching.
//!
//! # The layout is never written down, and these tests prove it by moving it
//!
//! `derive` reads the pool out of symbols: the base from `Object_RAM`/`Player_1`, the **stride** from
//! `Player_2 - Player_1`, the **count** from `Object_RAM_End`, the partition from the three boundary
//! marks. So the tests build listings rather than fixtures, and the load-bearing evidence is that the same
//! ROM answers differently under two listings — a hardcoded base cannot do that. `sst.emp` dates the
//! current `$50` record to a 2026-08-05 fold from `$52`, so a decoder that pinned it was wrong three weeks
//! ago; [`a_record_stride_the_catalogue_was_not_written_for_is_refused`] is that fold arriving again and
//! being caught instead of silently mis-decoded.

mod common;

use common::{spawn_system, Client};
use oracle_core::system::System;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------------------------------
// The layout under test — every number DERIVED from a named source, none copied from a neighbour
// ---------------------------------------------------------------------------------------------------

/// `sst.emp` at aeon `f4896139`: `pub struct Sst (size: $50)`. The server does **not** hold this number;
/// it measures the stride from two adjacent slot symbols and cross-checks it against the size its field
/// catalogue was written for. The tests hold it because they are the ones building the listing.
const SST: u32 = 0x50;

// `engine/system/constants.emp:78-90`.
const NUM_PLAYERS: u32 = 2;
const NUM_DYNAMIC: u32 = 40;
const NUM_SYSTEM: u32 = 8;
const NUM_EFFECTS: u32 = 16;
/// `NUM_TOTAL_SLOTS`, computed the way `constants.emp` computes it rather than typed as 66.
const NUM_TOTAL: u32 = NUM_PLAYERS + NUM_DYNAMIC + NUM_SYSTEM + NUM_EFFECTS;

/// `Player_1` per the demand doc. Arbitrary as far as the server is concerned — which is the point, and
/// [`the_base_address_follows_the_listing_and_is_not_written_into_the_server`] moves it to prove so.
const BASE: u32 = 0x00FF_8DB0;

/// `objcodebase.emp:4-6`: *"The object code bank starts at `$10000` (ObjCodeBase)"*.
const OBJ_CODE_BASE: u32 = 0x0001_0000;

/// `ram.emp:612-618`'s declaration order, as `(name, address)` rows for a listing.
///
/// The addresses are **computed from `base` and `stride`**, never listed: a table of literals would let a
/// test agree with a server that had the same literals baked in, which is exactly the property under test.
fn pool_rows(base: u32, stride: u32) -> Vec<(String, u32)> {
    let player = base;
    let dynamic = player + NUM_PLAYERS * stride;
    let system = dynamic + NUM_DYNAMIC * stride;
    let effect = system + NUM_SYSTEM * stride;
    let end = effect + NUM_EFFECTS * stride;
    vec![
        ("Object_RAM".into(), player),
        ("Player_1".into(), player),
        ("Player_2".into(), player + stride),
        ("Dynamic_Slots".into(), dynamic),
        ("System_Slots".into(), system),
        ("Effect_Slots".into(), effect),
        ("Object_RAM_End".into(), end),
        ("ObjCodeBase".into(), OBJ_CODE_BASE),
    ]
}

// ---------------------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------------------

fn machine() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    sys
}

fn client(h: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(h);
    c.handshake(false);
    c
}

/// An AS-dialect symbol listing, in the spelling `emulator/load_symbols` already parses elsewhere in this
/// suite. The declared count is computed from the rows so the table loads intact.
fn listing(rows: &[(String, u32)]) -> String {
    let mut s = String::from("  Symbol Table (* = unused):\n\n");
    for (name, addr) in rows {
        s.push_str(&format!(" {name} : {addr:X} C |\n"));
    }
    s.push_str(&format!("\n{:>4} symbols\n", rows.len()));
    s
}

/// Write a listing to disk and load it.
///
/// **The filename is unique per call, not per tag**, and that is a bug fix rather than tidiness: several
/// tests load the same `full` layout, `cargo test` runs them in parallel in one process, and a shared
/// path meant two threads writing and reading one file at once. That produced failures in tests the
/// mutation under investigation had nothing to do with — a flaky suite reports the wrong thing twice
/// over, once as a false red and once as a red nobody trusts.
fn load_listing(c: &mut Client, tag: &str, rows: &[(String, u32)]) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("oracle-objdec-{}-{tag}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{tag}.lst"));
    std::fs::write(&path, listing(rows)).unwrap();
    c.ok(
        "emulator/load_symbols",
        json!({"path": path.to_str().unwrap()}),
    );
}

/// The ordinary case: the full aeon pool at [`BASE`] with a `$50` stride.
fn load_full_layout(c: &mut Client, tag: &str) {
    load_listing(c, tag, &pool_rows(BASE, SST));
}

/// Zero the whole object table, so "active" means "this test wrote a code word here" and nothing else.
///
/// **Not decoration.** `System::new(seed)` does not promise zeroed work RAM, and a slot that reads active
/// because of a fill pattern would make every `total` in this file a number nobody chose.
fn zero_pool(c: &mut Client, base: u32, stride: u32) {
    let span = (NUM_TOTAL * stride) as usize;
    let chunk = 4096usize; // `limits.maxWriteLen`
    let mut off = 0usize;
    while off < span {
        let n = chunk.min(span - off);
        let bytes = format!("0x{}", "00".repeat(n));
        c.ok(
            "emulator/write_memory",
            json!({"addr": format!("0x{:06X}", base + off as u32), "bytes": bytes}),
        );
        off += n;
    }
}

/// Poke one whole record. `rec` is `(offset, width, value)` triples in the record's own coordinates.
fn poke_slot(c: &mut Client, base: u32, stride: u32, slot: u32, rec: &[(u32, u32, u32)]) {
    let addr = base + slot * stride;
    for (off, width, value) in rec {
        c.ok(
            "emulator/write_memory",
            json!({
                "addr": format!("0x{:06X}", addr + off),
                "value": value,
                "width": width,
            }),
        );
    }
}

/// A minimal live object: a non-zero `code_addr` at `$00` and a position.
fn live_object(code: u32, x_px: u32, y_px: u32) -> Vec<(u32, u32, u32)> {
    vec![
        (0x00, 2, code), // code_addr — 0 is the empty-slot sentinel
        (0x02, 2, x_px), // x_pos, integer half
        (0x06, 2, y_px), // y_pos, integer half
    ]
}

/// Assert a method is advertised, so a refusal test cannot go green because the row is not served.
///
/// This is the alternative green path every refusal assertion in this file has: `-32601` and `-32012` are
/// both "an error came back", and a suite that only checked "an error came back" would pass on a build
/// that serves none of these rows — which is the exact failure §8 item 23 makes a conformance clause.
fn assert_advertised(h: &oracle_aether::server::ServerHandle) {
    // A connection of its own: `initialize` is once per connection, and re-handshaking the caller's would
    // turn this control into a `-32600` that hides whatever it was meant to prove.
    let mut probe = Client::connect(h);
    let hs = probe.handshake(false);
    let methods: Vec<&str> = hs["methods"]
        .as_array()
        .expect("methods is an array")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for m in [
        "emulator/object_list",
        "emulator/player_state",
        "emulator/object_slot",
    ] {
        assert!(methods.contains(&m), "{m} is not advertised");
    }
}

// ---------------------------------------------------------------------------------------------------
// 1. Advertisement — §8 item 23 makes `methods` a warranty
// ---------------------------------------------------------------------------------------------------

/// The three rows are advertised **and** dispatch, and `capabilities.objectDecoders` says so.
///
/// The flag is `true` iff **at least one** ⚙ row is in `methods` (S4's pin): under an "all three" reading
/// a build that dropped one row would advertise `false` while serving two, whose wire signature is
/// identical to a smaller server.
#[test]
fn the_three_decoder_rows_are_advertised_and_the_capability_agrees() {
    let h = spawn_system("objdec-advert", machine(), 64);
    let mut c = Client::connect(&h);
    let hs = c.handshake(false);
    assert_eq!(
        hs["capabilities"]["objectDecoders"],
        json!(true),
        "objectDecoders must report that THIS BUILD has the handlers"
    );
    let methods: Vec<&str> = hs["methods"]
        .as_array()
        .expect("methods is an array")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let rows = [
        "emulator/object_list",
        "emulator/player_state",
        "emulator/object_slot",
    ];
    for m in rows {
        assert!(methods.contains(&m), "{m} missing from the advertised list");
    }
    // Advertised is a warranty, not an advertisement: every one of them must answer something other than
    // `-32601`. Without symbols they answer `-32012`, which is a dispatch that happened.
    for m in rows {
        let e = c.err(m, json!({"slot": 0}));
        assert_ne!(
            e["code"],
            json!(-32601),
            "{m} is advertised but answers `no such method`"
        );
    }
}

// ---------------------------------------------------------------------------------------------------
// 2. Refuse rather than guess — §11.25's first hardening
// ---------------------------------------------------------------------------------------------------

/// **No symbols, no decode.** `-32012`, not a hardcoded base.
///
/// The legacy server falls back to a literal address at two call sites, which is the confidently-wrong
/// shape with no `binding` field to reveal it. This asserts the code exactly, and asserts advertisement
/// separately, so "an error came back" cannot stand in for "the right refusal came back".
#[test]
fn every_decoder_row_refuses_without_a_symbol_table() {
    let h = spawn_system("objdec-nosyms", machine(), 64);
    let mut c = client(&h);
    assert_advertised(&h);
    for (m, p) in [
        ("emulator/object_list", json!({})),
        ("emulator/player_state", json!({})),
        ("emulator/object_slot", json!({"slot": 0})),
    ] {
        let e = c.err(m, p);
        assert_eq!(
            e["code"],
            json!(-32012),
            "{m} must refuse with NO_SYMBOLS_LOADED, got {e}"
        );
        assert!(
            e["message"].as_str().unwrap().contains("guessed base"),
            "{m}: the refusal must say what it is refusing to do, got {e}"
        );
    }
}

/// A table that loads but names no object pool is refused, and the reply says which symbols were missing.
///
/// The alternative green path is "no table at all" — the previous test's case — so this one proves a table
/// really is loaded by resolving a symbol out of it first.
#[test]
fn a_symbol_table_that_names_no_object_pool_is_refused_and_names_what_is_missing() {
    let h = spawn_system("objdec-wrongsyms", machine(), 64);
    let mut c = client(&h);
    load_listing(&mut c, "probe", &[("Probe".to_string(), 0x00FF_0600)]);
    // The control: a table IS loaded, so the refusal below is not the no-table branch wearing a disguise.
    let probe = c.ok("emulator/lookup_symbol", json!({"name": "Probe"}));
    assert_eq!(probe["addr"], json!("0x00FF0600"));

    let e = c.err("emulator/object_list", json!({}));
    assert_eq!(e["code"], json!(-32012), "{e}");
    let missing: Vec<&str> = e["data"]["missingSymbols"]
        .as_array()
        .expect("error.data.missingSymbols is an array")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        !missing.is_empty(),
        "an empty missing-list would make this assertion vacuous: {e}"
    );
    for want in ["Object_RAM", "Player_1", "Player_2", "Object_RAM_End"] {
        assert!(
            missing.contains(&want),
            "{want} did not resolve but is not named in error.data.missingSymbols: {missing:?}"
        );
    }
    assert_eq!(e["data"]["engine"], json!("aeon-sst"));
}

/// **The `$52` → `$50` fold, arriving again.**
///
/// `sst.emp`'s own comment dates the current record size to a 2026-08-05 fold that shrank it from `$52`,
/// and `core.emp:52` still carries the stale sentence. A server that measured the stride but kept a field
/// catalogue written for the other size would read `anim` out of `subtype` and report it as a datum. So a
/// stride that disagrees with the catalogue is a refusal, and both numbers are in the message.
#[test]
fn a_record_stride_the_catalogue_was_not_written_for_is_refused() {
    let h = spawn_system("objdec-stride", machine(), 64);
    let mut c = client(&h);
    const PRE_FOLD: u32 = 0x52;
    load_listing(&mut c, "prefold", &pool_rows(BASE, PRE_FOLD));
    // Control: this listing is well-formed — every symbol resolves — so the refusal below is about the
    // stride and not about a missing name.
    assert_eq!(
        c.ok("emulator/lookup_symbol", json!({"name": "Player_2"}))["addr"],
        json!(format!("0x{:08X}", BASE + PRE_FOLD))
    );

    let e = c.err("emulator/object_list", json!({}));
    assert_eq!(e["code"], json!(-32012), "{e}");
    let msg = e["message"].as_str().unwrap();
    assert!(
        msg.contains("$52"),
        "the measured stride must be named: {msg}"
    );
    assert!(
        msg.contains("$50"),
        "the size the catalogue was written for must be named: {msg}"
    );
    assert_eq!(
        e["data"]["missingSymbols"],
        json!([]),
        "nothing was missing — an empty list here is the finding, not an absence: {e}"
    );
}

// ---------------------------------------------------------------------------------------------------
// 3. `layout` — REQUIRED on every reply, and derived per reply
// ---------------------------------------------------------------------------------------------------

/// The layout is read out of the listing: base, stride, count and the pool partition.
///
/// Every expectation here is **computed** from `SST` and the `constants.emp` pool sizes, so a server that
/// had the same literals baked in could still fail
/// [`the_base_address_follows_the_listing_and_is_not_written_into_the_server`], which moves them.
#[test]
fn the_layout_is_derived_from_the_listing_and_carries_how_it_was_detected() {
    let h = spawn_system("objdec-layout", machine(), 64);
    let mut c = client(&h);
    load_full_layout(&mut c, "full");
    zero_pool(&mut c, BASE, SST);

    let r = c.ok("emulator/object_list", json!({}));
    let l = &r["layout"];
    assert_eq!(l["engine"], json!("aeon-sst"));
    // P2: HOW the layout was chosen is part of the answer, not just WHAT it is.
    assert_eq!(l["detectedBy"], json!("symbol"));
    assert_eq!(
        l["detectedFrom"],
        json!("Object_RAM"),
        "the reply must name the symbol that actually answered"
    );
    assert_eq!(l["slotBytes"], json!(SST));
    assert_eq!(l["slotCount"], json!(NUM_TOTAL));
    assert_eq!(l["baseAddr"], json!(format!("0x{BASE:08X}")));

    let pools = l["pools"].as_array().expect("pools is an array");
    assert_eq!(
        pools.len(),
        4,
        "one entry per declared pool, got {:?}",
        l["pools"]
    );
    let expect = [
        ("player", 0, NUM_PLAYERS),
        ("dynamic", NUM_PLAYERS, NUM_DYNAMIC),
        ("system", NUM_PLAYERS + NUM_DYNAMIC, NUM_SYSTEM),
        (
            "effect",
            NUM_PLAYERS + NUM_DYNAMIC + NUM_SYSTEM,
            NUM_EFFECTS,
        ),
    ];
    for (i, (name, first, count)) in expect.iter().enumerate() {
        assert_eq!(pools[i]["name"], json!(name), "pool {i}");
        assert_eq!(pools[i]["firstSlot"], json!(first), "pool {name} firstSlot");
        assert_eq!(pools[i]["slotCount"], json!(count), "pool {name} slotCount");
    }
    // Contiguous and covering, checked rather than eyeballed: the last pool must end at slotCount.
    let last = pools.last().unwrap();
    assert_eq!(
        last["firstSlot"].as_u64().unwrap() + last["slotCount"].as_u64().unwrap(),
        u64::from(NUM_TOTAL)
    );

    // Every row carries it, not just this one.
    let p = c.ok("emulator/player_state", json!({}));
    assert_eq!(p["layout"], *l, "player_state must carry the same layout");
    let s = c.ok("emulator/object_slot", json!({"slot": 0}));
    assert_eq!(s["layout"], *l, "object_slot must carry the same layout");
}

/// **The load-bearing evidence that nothing is hardcoded**: the same ROM, two listings, two answers.
///
/// A committed fixture in aeon's own tree gives `Dynamic_Slots : FFFF8DC2` while the demand doc gives
/// `Player_1 = $FF8DB0`, and those two cannot describe one build — which is the whole argument for a
/// `layout` descriptor in one pair of numbers. A server carrying either literal would answer the same
/// thing twice here.
#[test]
fn the_base_address_follows_the_listing_and_is_not_written_into_the_server() {
    let h = spawn_system("objdec-move", machine(), 64);
    let mut c = client(&h);

    load_listing(&mut c, "at-base", &pool_rows(BASE, SST));
    let first = c.ok("emulator/object_list", json!({}))["layout"].clone();
    assert_eq!(first["baseAddr"], json!(format!("0x{BASE:08X}")));

    // A different build: the whole table slid by $400, exactly the way a 36-byte insertion once slid a RAM
    // block by +$24 inside one session (D7's founding incident).
    const MOVED: u32 = BASE + 0x400;
    load_listing(&mut c, "moved", &pool_rows(MOVED, SST));
    let second = c.ok("emulator/object_list", json!({}))["layout"].clone();
    assert_eq!(
        second["baseAddr"],
        json!(format!("0x{MOVED:08X}")),
        "the base must follow the listing, not a literal in the server"
    );
    assert_ne!(
        first["baseAddr"], second["baseAddr"],
        "if these agree the base is not coming from the symbols at all"
    );
    // Slot addresses move with it, so `addr` is genuinely `base + slot * stride` and not a second literal.
    let s = c.ok("emulator/object_slot", json!({"slot": 3}));
    assert_eq!(s["addr"], json!(format!("0x{:08X}", MOVED + 3 * SST)));
}

/// `pools` is OPTIONAL, and its absence is the honest answer — but `player_state` cannot answer without it.
///
/// Two findings in one arrangement, and they are different findings: `object_list` needs no partition and
/// still works, while `player_state`'s whole question is "which slots are players".
#[test]
fn pools_are_omitted_when_the_boundary_symbols_are_absent_and_player_state_then_refuses() {
    let h = spawn_system("objdec-nopools", machine(), 64);
    let mut c = client(&h);
    let rows: Vec<(String, u32)> = pool_rows(BASE, SST)
        .into_iter()
        .filter(|(n, _)| n != "Dynamic_Slots" && n != "System_Slots" && n != "Effect_Slots")
        .collect();
    assert_eq!(rows.len(), 5, "the four kept rows plus ObjCodeBase");
    load_listing(&mut c, "nopools", &rows);
    zero_pool(&mut c, BASE, SST);

    let r = c.ok("emulator/object_list", json!({}));
    assert!(
        r["layout"].get("pools").is_none(),
        "pools must be omitted rather than partitioned from a guess: {}",
        r["layout"]
    );
    // Loud control: the rest of the layout is there, so `pools` is absent for its own reason and not
    // because the whole reply is empty.
    assert_eq!(r["layout"]["slotCount"], json!(NUM_TOTAL));
    assert_eq!(r["layout"]["baseAddr"], json!(format!("0x{BASE:08X}")));

    let e = c.err("emulator/player_state", json!({}));
    assert_eq!(e["code"], json!(-32012), "{e}");
    let missing: Vec<&str> = e["data"]["missingSymbols"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        missing,
        vec!["Dynamic_Slots", "System_Slots", "Effect_Slots"],
        "the refusal must name the boundary symbols that were absent"
    );
}

// ---------------------------------------------------------------------------------------------------
// 4. `emulator/object_list`
// ---------------------------------------------------------------------------------------------------

/// Presence **is** activity: an empty slot is omitted, so slot numbers are sparse and `total` counts
/// active objects rather than the table's size.
///
/// One deliberate divergence from `emulator/sprites`, which pins `total` to the table's size because every
/// slot there is an item. Here the table's size lives in `layout.slotCount`, and the two must differ in
/// this reply or the divergence is not being tested.
#[test]
fn object_list_returns_only_active_slots_and_total_counts_them() {
    let h = spawn_system("objdec-list", machine(), 64);
    let mut c = client(&h);
    load_full_layout(&mut c, "full");
    zero_pool(&mut c, BASE, SST);

    let empty = c.ok("emulator/object_list", json!({}));
    assert_eq!(
        empty["objects"].as_array().unwrap().len(),
        0,
        "a zeroed pool has no active slots"
    );
    assert_eq!(empty["total"], json!(0));
    assert_eq!(
        empty["truncated"],
        json!(false),
        "REQUIRED even when false (§2.4 clause (a))"
    );

    poke_slot(&mut c, BASE, SST, 2, &live_object(0x2A18, 1024, 320));
    poke_slot(&mut c, BASE, SST, 5, &live_object(0x0100, 96, 656));
    let r = c.ok("emulator/object_list", json!({}));
    let objs = r["objects"].as_array().unwrap();
    assert_eq!(objs.len(), 2, "two slots were poked: {}", r["objects"]);
    assert_eq!(r["total"], json!(2));
    assert_eq!(r["returned"], json!(2));
    assert_eq!(r["truncated"], json!(false));
    // Sparse and ascending — slot 3 and 4 are empty and simply do not appear.
    assert_eq!(objs[0]["slot"], json!(2));
    assert_eq!(objs[1]["slot"], json!(5));
    // `total` is NOT the table's size. If these were equal the divergence from `sprites` would be untested.
    assert_ne!(
        r["total"], r["layout"]["slotCount"],
        "total counts active objects; the table's size is layout.slotCount"
    );
    assert_eq!(r["layout"]["slotCount"], json!(NUM_TOTAL));

    // P1: every item is checkable against another instrument on the same bus.
    assert_eq!(objs[0]["addr"], json!(format!("0x{:08X}", BASE + 2 * SST)));
    assert_eq!(objs[1]["addr"], json!(format!("0x{:08X}", BASE + 5 * SST)));
    assert_eq!(objs[0]["code"], json!("0x2A18"));
    assert_eq!(objs[0]["x"], json!(1024));
    assert_eq!(objs[0]["y"], json!(320));

    // No `active` on this row: a flag that would always be true is a field nobody reads.
    assert!(objs[0].get("active").is_none(), "{}", objs[0]);
}

/// `limit` is refused above the pool, never clamped — and echoed so a caller can tell a default from its
/// own request.
#[test]
fn object_list_limit_is_echoed_truncates_and_is_refused_above_the_pool() {
    let h = spawn_system("objdec-limit", machine(), 64);
    let mut c = client(&h);
    load_full_layout(&mut c, "full");
    zero_pool(&mut c, BASE, SST);
    poke_slot(&mut c, BASE, SST, 2, &live_object(0x2A18, 1, 2));
    poke_slot(&mut c, BASE, SST, 3, &live_object(0x2A18, 3, 4));

    // The default is the structural bound: this server advertises no `limits.maxObjectSlots`, and the
    // fragment says an absent one leaves `limit` bounded only by `layout.slotCount`.
    let all = c.ok("emulator/object_list", json!({}));
    assert_eq!(all["limit"], json!(NUM_TOTAL));
    assert_eq!(all["returned"], json!(2));
    assert_eq!(all["truncated"], json!(false));

    let one = c.ok("emulator/object_list", json!({"limit": 1}));
    assert_eq!(one["limit"], json!(1), "the ceiling actually applied");
    assert_eq!(one["returned"], json!(1));
    assert_eq!(
        one["total"],
        json!(2),
        "total is the whole population, not the page"
    );
    assert_eq!(one["truncated"], json!(true));
    assert_eq!(one["objects"].as_array().unwrap().len(), 1);

    // Refused, never clamped. A clamped page hands back fewer rows than were asked for with nothing on the
    // wire saying so.
    for bad in [0u64, u64::from(NUM_TOTAL) + 1] {
        let e = c.err("emulator/object_list", json!({"limit": bad}));
        assert_eq!(e["code"], json!(-32602), "limit {bad}: {e}");
    }
    // And the boundary itself is accepted, so the refusal is off-by-none.
    let edge = c.ok("emulator/object_list", json!({"limit": NUM_TOTAL}));
    assert_eq!(edge["limit"], json!(NUM_TOTAL));
}

// ---------------------------------------------------------------------------------------------------
// 5. The `active: false` conditional — the case adjudication kept catching
// ---------------------------------------------------------------------------------------------------

/// **An empty slot reports `active: false` and omits every decoded key.**
///
/// This is M7, and it is the defect the delta ruling had to unwind: the first ruling required all five
/// core keys unconditionally on `object_slot`, which refuses the honest answer for an empty slot. Emitting
/// `x: 0` instead would report bytes the game never wrote — the uninitialised-byte-as-datum shape §11.25
/// rule (3) forbids one level down for `fields`.
///
/// `bytes` is in the forbidden set deliberately, so the test asks with `includeBytes: true` **and** a
/// `fields` list: if either escaped the conditional the reply would be refused by the fragment.
#[test]
fn an_empty_object_slot_is_active_false_and_carries_no_decoded_key() {
    let h = spawn_system("objdec-empty", machine(), 64);
    let mut c = client(&h);
    load_full_layout(&mut c, "full");
    zero_pool(&mut c, BASE, SST);

    let r = c.ok(
        "emulator/object_slot",
        json!({"slot": 3, "includeBytes": true, "fields": ["anim", "status"]}),
    );
    // The slot facts and the layout survive — this is what makes the absences below meaningful rather
    // than the whole reply being empty.
    assert_eq!(r["slot"], json!(3));
    assert_eq!(r["addr"], json!(format!("0x{:08X}", BASE + 3 * SST)));
    assert_eq!(
        r["active"],
        json!(false),
        "false is the answer, not the absence of one"
    );
    assert!(
        r.get("layout").is_some(),
        "layout is REQUIRED on every reply"
    );

    for k in ["x", "y", "code", "name", "nameDisp", "fields", "bytes"] {
        assert!(
            r.get(k).is_none(),
            "`{k}` is present on an inactive slot — an empty record's bytes are bytes the game never \
             wrote: {r}"
        );
    }
}

/// The occupied case, so the conditional is proven in **both** directions on this row.
#[test]
fn an_occupied_object_slot_hoists_the_item_keys_beside_active_true() {
    let h = spawn_system("objdec-occupied", machine(), 64);
    let mut c = client(&h);
    load_full_layout(&mut c, "full");
    zero_pool(&mut c, BASE, SST);
    poke_slot(&mut c, BASE, SST, 7, &live_object(0x2A18, 1024, 320));

    let r = c.ok("emulator/object_slot", json!({"slot": 7}));
    assert_eq!(r["active"], json!(true));
    assert_eq!(r["slot"], json!(7));
    assert_eq!(r["addr"], json!(format!("0x{:08X}", BASE + 7 * SST)));
    assert_eq!(r["x"], json!(1024));
    assert_eq!(r["y"], json!(320));
    assert_eq!(r["code"], json!("0x2A18"));
    // `role` is declared on the player item only; emitting it here would be refused by item 20's closure.
    assert!(r.get("role").is_none(), "{r}");
    // Not asked for, so not present — the default reply's key set is fully enumerated by the fragment.
    assert!(r.get("fields").is_none(), "{r}");
    assert!(r.get("bytes").is_none(), "{r}");
}

/// A slot past the pool is `-32602` with the bound in `error.data`, never clamped.
///
/// §11.25 records that the contract is split here — `pixel_attribution` answers `-32004` for a
/// structurally identical refusal — and this family follows `scanlines`. The legacy server's `-32004`
/// agrees with `pixel_attribution` and is deliberately not followed.
#[test]
fn a_slot_past_the_pool_is_refused_with_the_bound() {
    let h = spawn_system("objdec-oob", machine(), 64);
    let mut c = client(&h);
    load_full_layout(&mut c, "full");

    let e = c.err("emulator/object_slot", json!({"slot": NUM_TOTAL}));
    assert_eq!(e["code"], json!(-32602), "not -32004: {e}");
    assert_eq!(e["data"]["slotCount"], json!(NUM_TOTAL));
    assert_eq!(e["data"]["slot"], json!(NUM_TOTAL));
    // Off-by-none: the last legal slot answers.
    let last = c.ok("emulator/object_slot", json!({"slot": NUM_TOTAL - 1}));
    assert_eq!(last["slot"], json!(NUM_TOTAL - 1));

    // `slot` is REQUIRED — this row addresses a slot.
    let missing = c.err("emulator/object_slot", json!({}));
    assert_eq!(missing["code"], json!(-32602), "{missing}");
}

// ---------------------------------------------------------------------------------------------------
// 6. `emulator/player_state`
// ---------------------------------------------------------------------------------------------------

/// Inactive player slots are **returned** with `active: false`, and carry no decoded key.
///
/// "Player 2 is not present" is the answer to the question asked; a client must not have to infer it from
/// an array's length against a bound it joins from elsewhere. This is M2 — the defect that made the CR's
/// first fragment refuse every reply from a one-player game.
#[test]
fn player_state_returns_inactive_slots_and_says_so() {
    let h = spawn_system("objdec-players", machine(), 64);
    let mut c = client(&h);
    load_full_layout(&mut c, "full");
    zero_pool(&mut c, BASE, SST);
    poke_slot(&mut c, BASE, SST, 0, &live_object(0x0100, 96, 656));

    let r = c.ok("emulator/player_state", json!({"fields": ["anim"]}));
    let ps = r["players"].as_array().unwrap();
    assert_eq!(
        ps.len(),
        NUM_PLAYERS as usize,
        "one entry per player SLOT, inactive included: {}",
        r["players"]
    );

    assert_eq!(ps[0]["active"], json!(true));
    assert_eq!(ps[0]["slot"], json!(0));
    assert_eq!(ps[0]["addr"], json!(format!("0x{BASE:08X}")));
    assert_eq!(ps[0]["x"], json!(96));
    assert_eq!(ps[0]["y"], json!(656));
    assert_eq!(ps[0]["code"], json!("0x0100"));
    assert!(ps[0]["fields"].is_object(), "{}", ps[0]);

    assert_eq!(ps[1]["active"], json!(false));
    assert_eq!(ps[1]["slot"], json!(1));
    assert_eq!(ps[1]["addr"], json!(format!("0x{:08X}", BASE + SST)));
    for k in ["x", "y", "code", "name", "nameDisp", "fields", "bytes"] {
        assert!(
            ps[1].get(k).is_none(),
            "`{k}` on an absent player: {}",
            ps[1]
        );
    }
    assert!(r.get("layout").is_some());
    // Structurally bounded, so §2.4 clause (d) gives it neither companions nor a cursor.
    for k in ["total", "returned", "limit", "truncated", "cursor"] {
        assert!(r.get(k).is_none(), "player_state must not carry `{k}`: {r}");
    }
}

/// **`role` survives inactivity** (the delta ruling's M5), and is omitted rather than guessed when the
/// listing names the slot ambiguously.
///
/// The label is the slot's, not the occupant's — and `layout.pools` carries pool names, not per-slot
/// roles, so forbidding `role` on an empty slot would delete the answer rather than displace it. This
/// server derives it from the symbol that names the slot's base address, so the two branches are: exactly
/// one symbol there (answer it) and several (omit it, because choosing would be a guess).
#[test]
fn role_is_the_slots_own_symbol_survives_inactivity_and_is_omitted_when_ambiguous() {
    let h = spawn_system("objdec-role", machine(), 64);
    let mut c = client(&h);
    // `Object_RAM` and `Player_1` name the same address, so slot 0 is ambiguous while slot 1 is not.
    load_full_layout(&mut c, "full");
    zero_pool(&mut c, BASE, SST);

    let r = c.ok("emulator/player_state", json!({}));
    let ps = r["players"].as_array().unwrap();
    assert_eq!(ps.len(), NUM_PLAYERS as usize);
    assert_eq!(ps[0]["active"], json!(false));
    assert!(
        ps[0].get("role").is_none(),
        "two symbols name slot 0's address; picking one would be a guess: {}",
        ps[0]
    );
    // M5 in one line: an INACTIVE slot still carries its label.
    assert_eq!(ps[1]["active"], json!(false));
    assert_eq!(
        ps[1]["role"],
        json!("Player_2"),
        "role is a fact about the slot, so it survives active:false: {}",
        ps[1]
    );

    // Drop the ambiguity and slot 0 answers too — which proves the omission above was about ambiguity and
    // not about slot 0 being special.
    let rows: Vec<(String, u32)> = pool_rows(BASE, SST)
        .into_iter()
        .filter(|(n, _)| n != "Object_RAM")
        .collect();
    load_listing(&mut c, "unambiguous", &rows);
    let r2 = c.ok("emulator/player_state", json!({}));
    let ps2 = r2["players"].as_array().unwrap();
    assert_eq!(ps2[0]["role"], json!("Player_1"), "{}", ps2[0]);
    // And `detectedFrom` follows the same evidence: `Object_RAM` is gone, so `Player_1` answered.
    assert_eq!(r2["layout"]["detectedFrom"], json!("Player_1"));
}

// ---------------------------------------------------------------------------------------------------
// 7. `fields` — a typed-open map, and nothing decoded into names
// ---------------------------------------------------------------------------------------------------

/// An unknown `fields` name is `-32602` **before any decode**, and every offender is named at once.
#[test]
fn an_unknown_fields_name_is_refused_and_all_of_them_are_named() {
    let h = spawn_system("objdec-badfield", machine(), 64);
    let mut c = client(&h);
    load_full_layout(&mut c, "full");

    let e = c.err(
        "emulator/object_list",
        json!({"fields": ["anim", "ground_speed", "notAField"]}),
    );
    assert_eq!(e["code"], json!(-32602), "{e}");
    assert_eq!(
        e["data"]["unknownFields"],
        json!(["ground_speed", "notAField"]),
        "every offender, not just the first — and `anim`, which IS declared, must not be listed"
    );
    // `ground_speed` is a `PlayerV` overlay name. It is refused because the overlay window has ten
    // declared interpretations in one build, so no server can say which one is live for a given slot.
    let msg = e["message"].as_str().unwrap();
    assert!(msg.contains("aeon-sst"), "the layout must be named: {msg}");

    // The same refusal on the other two rows: one catalogue, three doors.
    for (m, p) in [
        ("emulator/player_state", json!({"fields": ["nope"]})),
        (
            "emulator/object_slot",
            json!({"slot": 0, "fields": ["nope"]}),
        ),
    ] {
        let e = c.err(m, p);
        assert_eq!(e["code"], json!(-32602), "{m}: {e}");
        assert_eq!(e["data"]["unknownFields"], json!(["nope"]), "{m}");
    }
}

/// **The map is open in its keys and closed in its value types**, and nothing is decoded into names.
///
/// `status` is the test's spine: the legacy server emits `{"raw": n, "bits": [...]}` from a table whose
/// entries are invented and whose two branches disagree on the spelling of the same concept. §11.25
/// refuses that on every row — a set-bits list carries strictly less than the byte beside it, because it
/// cannot express a clear bit — so `status` must arrive as a number and nothing else.
#[test]
fn fields_values_are_scalars_typed_per_the_layout_and_never_decoded_into_names() {
    let h = spawn_system("objdec-fields", machine(), 64);
    let mut c = client(&h);
    load_full_layout(&mut c, "full");
    zero_pool(&mut c, BASE, SST);
    poke_slot(
        &mut c,
        BASE,
        SST,
        4,
        &[
            (0x00, 2, 0x2A18),      // code_addr
            (0x0A, 2, 0xFFC0),      // x_vel: -64 in 8.8, signed
            (0x10, 4, 0x0000_A1C4), // mappings: a ROM pointer
            (0x18, 1, 3),           // anim
            (0x1E, 1, 0x06),        // status: two bits set
            (0x23, 1, 9),           // mapping_frame
        ],
    );

    let r = c.ok(
        "emulator/object_slot",
        json!({"slot": 4, "fields": ["anim", "mapping_frame", "status", "x_vel", "mappings", "code_addr"]}),
    );
    let f = r["fields"].as_object().expect("fields is an object");
    assert_eq!(f.len(), 6, "one key per requested field: {f:?}");

    // D9 category 2 — counts and scalars as numbers.
    assert_eq!(f["anim"], json!(3));
    assert_eq!(f["mapping_frame"], json!(9));
    // Raw, and a NUMBER. Not `{raw, bits}`, not a list of names.
    assert_eq!(
        f["status"],
        json!(6),
        "status must be the byte at the offset the layout names, uninterpreted"
    );
    // Signed, because a velocity that cannot be negative is not a velocity.
    assert_eq!(f["x_vel"], json!(-64));
    // D9 category 1 — address-shaped fields as hex strings, at the field's own width.
    assert_eq!(f["mappings"], json!("0x0000A1C4"));
    assert_eq!(f["code_addr"], json!("0x2A18"));

    // The openness is bounded: every value is a scalar. An object here is exactly the legacy's decoded-bit
    // shape, and the fragment refuses it by construction.
    for (k, v) in f {
        assert!(
            v.is_number() || v.is_string() || v.is_boolean(),
            "`{k}` is not a scalar: {v}"
        );
    }

    // Asked for none is still asking: an empty map, not an absent key.
    let none = c.ok("emulator/object_slot", json!({"slot": 4, "fields": []}));
    assert_eq!(none["fields"], json!({}), "{none}");
    // Not asked at all: no key.
    let unasked = c.ok("emulator/object_slot", json!({"slot": 4}));
    assert!(unasked.get("fields").is_none(), "{unasked}");
}

/// No overlay name is in the catalogue — `sst_custom` (`$30-$4F`) is addressable but not live.
///
/// The CR measured ten declared overlays of that one 32-byte window in a single build, five of which put a
/// different type on the word at `$30` (`ground_speed: i16`, `player: u16` — an SST *pointer* —
/// `steps_remaining`, `timer`, `half_height`). Which one applies is decided by which routine owns the
/// slot, at run time, so any name for it would be §11.25 rule (3)'s uninitialised byte as a datum.
#[test]
fn no_overlay_field_name_is_offered() {
    let h = spawn_system("objdec-overlay", machine(), 64);
    let mut c = client(&h);
    load_full_layout(&mut c, "full");
    // Control: a real field name from the same catalogue IS accepted, so the refusals below are about the
    // names and not about `fields` being broken.
    c.ok(
        "emulator/object_slot",
        json!({"slot": 0, "fields": ["anim"]}),
    );
    for overlay in [
        "ground_speed",
        "player",
        "steps_remaining",
        "timer",
        "half_height",
        "sst_custom",
    ] {
        let e = c.err(
            "emulator/object_slot",
            json!({"slot": 0, "fields": [overlay]}),
        );
        assert_eq!(e["code"], json!(-32602), "{overlay} was accepted: {e}");
    }
}

// ---------------------------------------------------------------------------------------------------
// 8. `x`/`y`, `bytes`, and `name`
// ---------------------------------------------------------------------------------------------------

/// `x`/`y` are the **signed integer half** of the 16.16 position, in world pixels.
///
/// Three properties in one arrangement: it is the high word (not the low), it is signed (an object one
/// pixel left of the origin must not read as 65535), and the sub-pixel half is still reachable — through
/// `fields`, which is what the contract means by not carrying a 16.16 raw on the wire.
#[test]
fn x_and_y_are_the_signed_integer_half_of_the_position() {
    let h = spawn_system("objdec-coords", machine(), 64);
    let mut c = client(&h);
    load_full_layout(&mut c, "full");
    zero_pool(&mut c, BASE, SST);
    poke_slot(
        &mut c,
        BASE,
        SST,
        1,
        &[
            (0x00, 2, 0x0100),      // code_addr
            (0x02, 4, 0xFFFF_8000), // x_pos: integer -1, fraction $8000
            (0x06, 4, 0x0140_C000), // y_pos: integer 320, fraction $C000
        ],
    );

    let r = c.ok(
        "emulator/object_slot",
        json!({"slot": 1, "fields": ["x_pos", "y_pos"]}),
    );
    assert_eq!(r["x"], json!(-1), "signed, and the HIGH word: {r}");
    assert_eq!(r["y"], json!(320));
    // The raw 16.16 is still reachable by name, so nothing is lost by carrying pixels on the wire.
    assert_eq!(r["fields"]["x_pos"], json!(0xFFFF_8000u32));
    assert_eq!(r["fields"]["y_pos"], json!(0x0140_C000u32));
}

/// `includeBytes` returns the whole record verbatim, at the layout's own stride.
#[test]
fn include_bytes_returns_the_whole_record_at_the_layouts_stride() {
    let h = spawn_system("objdec-bytes", machine(), 64);
    let mut c = client(&h);
    load_full_layout(&mut c, "full");
    zero_pool(&mut c, BASE, SST);
    poke_slot(&mut c, BASE, SST, 6, &live_object(0x2A18, 1, 2));

    let r = c.ok(
        "emulator/object_slot",
        json!({"slot": 6, "includeBytes": true}),
    );
    let b = r["bytes"].as_str().expect("bytes is a hex string");
    assert!(b.starts_with("0x"), "{b}");
    assert_eq!(
        b.len(),
        2 + (SST as usize) * 2,
        "the whole record, `slotBytes` long — {} bytes, not {}",
        SST,
        (b.len() - 2) / 2
    );
    // The bytes really are this slot's: the code word leads and the position follows.
    assert!(b.starts_with("0x2A18"), "{b}");
    assert_eq!(r["layout"]["slotBytes"], json!(SST));
}

/// `name` is the **bare, round-tripping** label for `code`, resolved through `ObjCodeBase` — and both
/// `name` and `nameDisp` are omitted together when nothing resolves.
///
/// §11.25's second hardening against the legacy server, which strips a `_Main` suffix so the name it
/// reports resolves to nothing. `code_addr` is an *offset* (`objcodebase.emp`: *"Every object routine's
/// code_addr is `label - ObjCodeBase`"*), so without that symbol there is no address to resolve — and the
/// keys are then omitted, never `""`.
#[test]
fn name_is_the_bare_label_for_code_and_round_trips_or_is_omitted() {
    let h = spawn_system("objdec-name", machine(), 64);
    let mut c = client(&h);
    let mut rows = pool_rows(BASE, SST);
    rows.push(("Obj_Ring_Main".into(), OBJ_CODE_BASE + 0x2A18));
    load_listing(&mut c, "named", &rows);
    zero_pool(&mut c, BASE, SST);
    poke_slot(&mut c, BASE, SST, 2, &live_object(0x2A18, 0, 0));
    // Two bytes past the label: `nameDisp` carries the displacement, and the NAME must not.
    poke_slot(&mut c, BASE, SST, 3, &live_object(0x2A1A, 0, 0));

    let exact = c.ok("emulator/object_slot", json!({"slot": 2}));
    assert_eq!(exact["name"], json!("Obj_Ring_Main"));
    assert_eq!(exact["nameDisp"], json!(0));
    // §4 made operable: the reported name resolves back to the same symbol.
    let back = c.ok(
        "emulator/lookup_symbol",
        json!({"name": exact["name"].as_str().unwrap()}),
    );
    assert_eq!(
        back["addr"],
        json!(format!("0x{:08X}", OBJ_CODE_BASE + 0x2A18))
    );

    let displaced = c.ok("emulator/object_slot", json!({"slot": 3}));
    assert_eq!(displaced["name"], json!("Obj_Ring_Main"));
    assert_eq!(displaced["nameDisp"], json!(2));
    assert!(
        !displaced["name"].as_str().unwrap().contains('+'),
        "a displacement is never inside a name string (§4): {displaced}"
    );

    // Without `ObjCodeBase` the offset cannot become an address — so BOTH keys go, and nothing is faked.
    let no_base: Vec<(String, u32)> = rows
        .into_iter()
        .filter(|(n, _)| n != "ObjCodeBase")
        .collect();
    load_listing(&mut c, "nobase", &no_base);
    let r = c.ok("emulator/object_slot", json!({"slot": 2}));
    assert_eq!(r["active"], json!(true), "the slot is still occupied: {r}");
    assert_eq!(r["code"], json!("0x2A18"), "code is still the raw datum");
    assert!(r.get("name").is_none(), "never the empty string: {r}");
    assert!(
        r.get("nameDisp").is_none(),
        "a displacement with no name: {r}"
    );
}

// ---------------------------------------------------------------------------------------------------
// 9. Run-control — these are pure reads
// ---------------------------------------------------------------------------------------------------

/// None of the three is subject to §6's run-control state rule.
///
/// A pure read, exactly as `read`/`sprites`/`pixel_attribution`/`scanlines`: the envelope's `running` is
/// the contract's whole answer to a torn sample, and requiring a pause would make the one instrument for
/// "what is on screen right now" unusable while the game runs.
#[test]
fn the_decoder_rows_answer_while_the_machine_is_running() {
    let h = spawn_system("objdec-running", machine(), 64);
    let mut c = client(&h);
    load_full_layout(&mut c, "full");
    zero_pool(&mut c, BASE, SST);
    c.ok("emulator/resume", json!({}));

    for (m, p) in [
        ("emulator/object_list", json!({})),
        ("emulator/player_state", json!({})),
        ("emulator/object_slot", json!({"slot": 0})),
    ] {
        let v: Value = c.call(m, p);
        assert!(
            v.get("error").is_none(),
            "{m} must not require a paused machine: {}",
            v["error"]
        );
    }
    c.ok("emulator/pause", json!({}));
}
