//! **The watchpoint surface (`protocol.md` §6, CR-11 and CR-12), over the real wire.**
//!
//! Every line these tests receive is validated against the vendored contract schema by
//! [`common::schema`], and every result is *closed* against its fragment (§8 item 20) — so a surplus key
//! or a wrong JSON type fails here without anyone writing an assertion for it. What is asserted below is
//! the half a schema structurally cannot check: **behaviour**.
//!
//! Two of these are the ones the design would not let be waived, and they are marked where they sit:
//!
//! * [`a_write_count_is_not_a_measure_of_change`] — the aeon lesson (*97% of freq writes and 99% of TL
//!   writes were redundant re-writes of unchanged values*) as an executable fixture, so it cannot be
//!   re-learned.
//! * The bus/panel parity check lives in `src/host.rs`'s own tests
//!   (`the_bus_and_the_panel_read_one_instrument`), because it has to reach the instrument **from the
//!   host's side** — through the accessor the player's run loop uses — which no socket client can do.

#![cfg(unix)]

mod common;

use common::{spawn, spawn_system, spawn_with, Client};
use oracle_aether::server::ServerHandle;
use oracle_core::system::System;
use serde_json::{json, Value};

/// The address `testrom::build`'s VInt handler writes `$1234` to, once per frame — and which the main loop
/// never stirs, so it is a clean single-writer target.
const SENTINEL: &str = "0x00FF8000";

/// A server whose machine actually **takes** its vertical interrupt.
///
/// The fixture ROMs lower the CPU mask but never touch a VDP register, so at power-on IE0 (reg 1 bit 5) is
/// off and the VInt latch is set every frame without ever being gated into the IPL — the handler never runs
/// and the sentinel is never written. Enabling it here is what makes "one write per frame, forever" true,
/// and it is the same one-line pose `system.rs`'s own interrupt tests use.
fn vint_server(tag: &str, rom: Vec<u8>) -> ServerHandle {
    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();
    sys.vdp_mut().control_write(0x8120, 0); // reg 1 = $20 → IE0 (VINT enable)
    spawn_system(tag, sys, 1024)
}

fn armed(tag: &str) -> (ServerHandle, Client) {
    let h = vint_server(tag, oracle_core::testrom::build());
    let mut c = Client::connect(&h);
    c.handshake(true);
    (h, c)
}

/// Every hit the ring holds, unpaged (the fixtures here stay well inside one page unless they say
/// otherwise).
fn hits(c: &mut Client, params: Value) -> Value {
    c.ok("emulator/watchpoint_hits", params)
}

// ---------------------------------------------------------------------------------------------------
// 1. What a watch IS
// ---------------------------------------------------------------------------------------------------

/// **§8 item 16's mistake, refused at all five places the handle appears.** A watch handle is an opaque
/// string (D9 category 4): it cannot be an address — one address may carry several watches, and the same
/// number names four different things across the four spaces — and it cannot be an index, because ids are
/// never reused. This server shipped a *numeric* checkpoint handle once against a reasonable reading of
/// D9; the schema has always said string, and this pins that it is one everywhere here.
#[test]
fn the_watch_handle_is_an_opaque_string_everywhere_it_appears() {
    let (_h, mut c) = armed("wp-handle");
    let add = c.ok(
        "emulator/watchpoint_add",
        json!({"addr": SENTINEL, "len": 2, "label": "sentinel"}),
    );
    let handle = add["watch"].as_str().expect("1. add's result: a string");
    assert!(!handle.is_empty());
    assert!(
        add["watch"].as_u64().is_none(),
        "a handle that parses as a number invites the arithmetic D9 category 4 forbids"
    );

    c.ok("emulator/run_frames", json!({"frames": 3}));

    let list = c.ok("emulator/watchpoint_list", json!({}));
    assert_eq!(list["watches"][0]["watch"], json!(handle), "2. list's item");

    let got = hits(&mut c, json!({}));
    let first = &got["hits"][0];
    assert_eq!(
        first["watch"],
        json!(handle),
        "3. every hit names its watch"
    );

    let filtered = hits(&mut c, json!({"watch": handle}));
    assert_eq!(
        filtered["total"], got["total"],
        "4. the same handle, accepted as a filter"
    );

    // 5. …and handed back to the inverse.
    let cleared = c.ok("emulator/watchpoint_clear", json!({"watch": handle}));
    assert_eq!(cleared["removed"], json!(1));

    // A *number* is refused rather than helpfully coerced: accepting it would reward exactly the usage
    // D9 category 4 exists to forbid, and would keep working right up until the ids stop looking like
    // small integers.
    let e = c.err("emulator/watchpoint_clear", json!({"watch": 0}));
    assert_eq!(e["code"], json!(-32602));
}

/// **The cap is D13 rule 3, verbatim: refuse at it, never grow past it, never evict.**
///
/// The reason is sharper for watches than for checkpoints and the assertion at the end is that reason: a
/// silently-dropped watch produces a `seen`-positive, `matched`-zero reading, which is exactly what a
/// genuine negative finding looks like. A client would read "nothing writes here" off an instrument that
/// was never armed.
#[test]
fn the_watch_cap_is_refused_loudly_and_never_silently_grown_or_evicted() {
    let (_h, mut c) = armed("wp-cap");
    // The cap is advertised *before* a client can plan around it — that is what makes it a cap rather than
    // a surprise. Read on a second connection, because it is a property of the server and not of a session.
    let mut c2 = Client::connect(&_h);
    let init = c2.handshake(false);
    let cap = init["capabilities"]["watchpoints"]["maxWatches"]
        .as_u64()
        .expect("maxWatches is advertised") as usize;
    assert!(init["capabilities"]["watchpoints"]["supported"] == json!(true));

    let mut handles = Vec::new();
    for i in 0..cap {
        let r = c.ok(
            "emulator/watchpoint_add",
            json!({"addr": "0x00FF0000", "len": 1, "label": format!("w{i}")}),
        );
        handles.push(r["watch"].as_str().unwrap().to_string());
    }
    let e = c.err(
        "emulator/watchpoint_add",
        json!({"addr": "0x00FF0000", "len": 1}),
    );
    assert_eq!(e["code"], json!(-32005), "wrong right now, not malformed");
    assert_eq!(e["data"]["reason"], json!("watchCapReached"));
    assert_eq!(e["data"]["cap"], json!(cap));
    assert_eq!(e["data"]["count"], json!(cap));

    // Never grown past…
    let list = c.ok("emulator/watchpoint_list", json!({"limit": 4096}));
    assert_eq!(list["total"], json!(cap));
    // …and never evicted: every handle issued is still live, so none of them has quietly started meaning
    // nothing while a client was still holding it.
    let live: Vec<&str> = list["watches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["watch"].as_str().unwrap())
        .collect();
    for h in &handles {
        assert!(live.contains(&h.as_str()), "watch {h} was evicted");
    }
}

/// **`censusKey` without `mode: "census"` is `-32602`, and never silently ignored** — §5's refuse-and-name
/// ethos applied one level down. A param this bus quietly dropped would be a caller believing it asked for
/// a grouping it did not get, and reading the ungrouped answer as the grouped one.
#[test]
fn a_census_key_is_refused_without_census_mode_and_vice_versa() {
    let (_h, mut c) = armed("wp-census-key");
    let e = c.err(
        "emulator/watchpoint_add",
        json!({"addr": SENTINEL, "censusKey": "value"}),
    );
    assert_eq!(e["code"], json!(-32602));
    assert!(
        e["message"].as_str().unwrap().contains("censusKey"),
        "the refusal names the param: {}",
        e["message"]
    );
    // Not ignored: nothing was armed by the refused call.
    let list = c.ok("emulator/watchpoint_list", json!({}));
    assert_eq!(list["total"], json!(0));

    // And the other direction — a census with no key has nothing to group by.
    let e = c.err(
        "emulator/watchpoint_add",
        json!({"addr": SENTINEL, "mode": "census"}),
    );
    assert_eq!(e["code"], json!(-32602));
}

/// Deletion is **idempotent** (§6.1's rule, for §6.1's reason: an error a client must learn to swallow
/// teaches clients to swallow errors), and clearing a watch does **not** delete the hits it recorded —
/// a destructive clear would let one client erase another's evidence on a shared bus.
#[test]
fn clearing_is_idempotent_and_a_retired_handle_stays_legible_on_its_hits() {
    let (_h, mut c) = armed("wp-retire");
    let handle = c.ok(
        "emulator/watchpoint_add",
        json!({"addr": SENTINEL, "len": 2}),
    )["watch"]
        .as_str()
        .unwrap()
        .to_string();
    c.ok("emulator/run_frames", json!({"frames": 3}));
    let before = hits(&mut c, json!({}));
    assert!(before["total"].as_u64().unwrap() > 0, "the watch fired");

    assert_eq!(
        c.ok("emulator/watchpoint_clear", json!({"watch": &handle}))["removed"],
        json!(1)
    );
    // Idempotent: an unknown handle succeeds with removed: 0, never -32005.
    assert_eq!(
        c.ok("emulator/watchpoint_clear", json!({"watch": &handle}))["removed"],
        json!(0),
        "a second clear of the same handle"
    );
    assert_eq!(
        c.ok("emulator/watchpoint_clear", json!({"watch": "w9999"}))["removed"],
        json!(0),
        "a handle that was never issued"
    );

    // The hits survive, still naming the retired watch…
    let after = hits(&mut c, json!({}));
    assert_eq!(after["total"], before["total"], "hits are not deleted");
    assert_eq!(after["hits"][0]["watch"], json!(handle));
    // …and the handle is absent from `watchpoint_list`, which is how staleness is made loud without a new
    // counter: ids are never reused, so this test cannot give a false negative.
    let list = c.ok("emulator/watchpoint_list", json!({}));
    assert_eq!(list["total"], json!(0));
    // A retired handle still filters — that is *why* the hits were kept.
    let filtered = hits(&mut c, json!({"watch": &handle}));
    assert_eq!(filtered["total"], before["total"]);
}

// ---------------------------------------------------------------------------------------------------
// 2. What a watch TELLS YOU
// ---------------------------------------------------------------------------------------------------

/// ## ★ NON-WAIVABLE ★ — **a write count is not a measure of change.**
///
/// The finding this pins is not ours and cost somebody a feature: *97% of freq writes and 99% of TL writes
/// were redundant re-writes of unchanged values*, measured on a driver that re-asserts its registers every
/// frame. `matched` answers *"how often was this written"* and never *"did it change"*.
///
/// The fixture is two ROMs that write the **same address the same number of times** and differ only in what
/// they write. `testrom::build`'s VInt handler stores the constant `$1234`; `build_vint_counter`'s
/// increments. A raw write count cannot tell them apart — and the point is that it *does not*, here,
/// measured rather than argued. The `value` census can, and the assertion is that the two numbers the reply
/// carries make the redundancy ratio computable: `matched` writes carried `distinctKeys` distinct values,
/// so `1 - distinctKeys/matched` is 100% on one ROM and 0% on the other.
#[test]
fn a_write_count_is_not_a_measure_of_change() {
    /// Arm a `value` census on the sentinel word, run `frames`, and report `(matched, distinctKeys, census)`.
    fn measure(tag: &str, rom: Vec<u8>, frames: u64) -> (u64, u64, Vec<(u64, u64)>) {
        let h = vint_server(tag, rom);
        let mut c = Client::connect(&h);
        c.handshake(false);
        c.ok(
            "emulator/watchpoint_add",
            json!({"addr": SENTINEL, "len": 2, "mode": "census", "censusKey": "value"}),
        );
        c.ok("emulator/run_frames", json!({ "frames": frames }));
        let w = c.ok("emulator/watchpoint_list", json!({}))["watches"][0].clone();
        let census = w["census"]
            .as_array()
            .expect("a census watch reports one")
            .iter()
            .map(|e| (e["key"].as_u64().unwrap(), e["count"].as_u64().unwrap()))
            .collect();
        (
            w["matched"].as_u64().unwrap(),
            w["distinctKeys"].as_u64().unwrap(),
            census,
        )
    }

    const FRAMES: u64 = 20;
    let (same_matched, same_distinct, same_census) =
        measure("wp-redundant", oracle_core::testrom::build(), FRAMES);
    let (moved_matched, moved_distinct, _) = measure(
        "wp-changing",
        oracle_core::testrom::build_vint_counter(),
        FRAMES,
    );

    // Both ROMs wrote the same address the same number of times…
    assert!(
        same_matched >= 5,
        "the fixture wrote something: {same_matched}"
    );
    assert_eq!(
        same_matched, moved_matched,
        "the two ROMs must be indistinguishable BY WRITE COUNT — that is the whole fixture"
    );

    // …and the write count is 100% misleading about one of them.
    assert_eq!(
        same_distinct, 1,
        "every one of those {same_matched} writes stored the same value"
    );
    assert_eq!(
        same_census,
        vec![(0x1234, same_matched)],
        "one key, carrying the entire write count"
    );
    assert_eq!(
        moved_distinct, moved_matched,
        "and on the counter ROM every write really did move the value"
    );

    // The reported numbers make the redundancy ratio computable, which is the operational claim: a client
    // that reads `matched` alone learns nothing about movement, and a client that reads both learns
    // everything this aggregate can say.
    let redundancy = |m: u64, d: u64| 100 - (d * 100 / m);
    assert_eq!(redundancy(same_matched, same_distinct), 95);
    assert_eq!(redundancy(moved_matched, moved_distinct), 0);
}

/// **`old` is present if and only if `space` is not `bus`; `fc` if and only if it is.** Structural, not
/// stylistic: `Watchpoints::on_event` builds every bus hit with `old: 0` unconditionally, because the 68000
/// bus event stream carries no prior value — emitting that zero would assert something false, and it would
/// defeat the change measurement the test above exists for. A VDP-internal write has no bus function code,
/// and its CPU-vs-DMA attribution is `via`.
#[test]
fn old_is_present_iff_the_space_is_not_bus_and_fc_iff_it_is() {
    // --- bus: fc, no old ---
    {
        let (_h, mut c) = armed("wp-bus-hit");
        c.ok(
            "emulator/watchpoint_add",
            json!({"addr": SENTINEL, "len": 2}),
        );
        c.ok("emulator/run_frames", json!({"frames": 2}));
        let hit = hits(&mut c, json!({}))["hits"][0].clone();
        assert_eq!(hit["space"], json!("bus"));
        assert!(hit.get("fc").is_some(), "a bus hit names its master: {hit}");
        assert!(
            hit.get("old").is_none(),
            "a bus hit has no prior value to report and must not invent 0x0: {hit}"
        );
        assert_eq!(hit["via"], json!("bus"));
        assert_eq!(hit["op"], json!("write"));
        assert_eq!(hit["value"], json!("0x00001234"));
    }

    // --- a VDP-internal space: old, no fc ---
    {
        let h = spawn_with("wp-vram-hit", oracle_core::testrom::build_vram_poke(), 1024);
        let mut c = Client::connect(&h);
        c.handshake(false);
        c.ok(
            "emulator/watchpoint_add",
            json!({"space": "vram", "addr": "0x00000100", "len": 2}),
        );
        c.ok("emulator/run_frames", json!({"frames": 1}));
        let got = hits(&mut c, json!({}));
        assert_eq!(got["total"], json!(2), "one word poke = two byte captures");
        let hit = got["hits"][0].clone();
        assert_eq!(hit["space"], json!("vram"));
        assert!(
            hit.get("old").is_some(),
            "a VDP write carries the value it replaced: {hit}"
        );
        assert!(
            hit.get("fc").is_none(),
            "a VDP-internal write has no bus function code: {hit}"
        );
        // The CPU-vs-DMA answer no function code can give.
        assert_eq!(hit["via"], json!("direct"));
        assert_eq!(hit["value"], json!("0x000000BE"));
        assert_eq!(
            hit["pc"],
            json!("0x00000216"),
            "attributed to the instruction that drove it"
        );
    }
}

/// The `via` census — the one core change this arc earned, and the reason it was earned: an `fc` census
/// **provably cannot** answer CPU-vs-DMA on a VDP watch, because a VDP-internal write has no bus function
/// code and core hardwires it to 0 there. The two findings that settled `cram_flicker` (*"all CPU writes,
/// zero DMA"*) and `direct_color_dma` (*"99.997% DMA"*) are this group-by, both computed by hand.
#[test]
fn a_via_census_answers_cpu_versus_dma_on_a_vdp_watch() {
    let h = spawn_with("wp-via", oracle_core::testrom::build_vram_poke(), 1024);
    let mut c = Client::connect(&h);
    c.handshake(false);
    c.ok(
        "emulator/watchpoint_add",
        json!({"space": "vram", "addr": "0x00000100", "len": 2,
               "mode": "census", "censusKey": "via"}),
    );
    c.ok("emulator/run_frames", json!({"frames": 1}));
    let w = c.ok("emulator/watchpoint_list", json!({}))["watches"][0].clone();
    assert_eq!(w["censusKey"], json!("via"));
    // 0 = bus, 1 = direct (a CPU data-port write), 2 = DMA. This fixture is a pure CPU poke.
    assert_eq!(w["census"], json!([{"key": 1, "count": 2}]));
    assert_eq!(w["distinctKeys"], json!(1));
    assert_eq!(w["matched"], json!(2));
    // A `record`-mode watch reports none of the census keys — they are the absence of a census, not an
    // answer, and a client comparing `distinctKeys` across a mixed list would read them as findings.
    c.ok(
        "emulator/watchpoint_add",
        json!({"space": "vram", "addr": "0x00000100"}),
    );
    let list = c.ok("emulator/watchpoint_list", json!({}));
    let plain = list["watches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["mode"] == json!("record"))
        .expect("the record watch");
    assert!(plain.get("census").is_none());
    assert!(plain.get("distinctKeys").is_none());
    assert!(plain.get("keysCapped").is_none());
}

/// **`seen` is the structural negative control, and it is required on every hits read.**
///
/// `seen > 0` with `matched == 0` is a live instrument that found nothing. `seen == 0` is an instrument
/// that was never attached to the run, and a zero from it means nothing at all. Without the distinction a
/// client cannot tell "this address is never written" from "the recorder was not in the run" — and the
/// second is a real failure mode wherever the process that owns the loop is not the one that armed the
/// watch, which is precisely this server's hosted arrangement.
#[test]
fn seen_separates_a_live_instrument_that_found_nothing_from_one_that_never_ran() {
    let (_h, mut c) = armed("wp-seen");
    // Before any run: nothing has been offered to the recorder.
    let cold = hits(&mut c, json!({}));
    assert_eq!(cold["seen"], json!(0), "never attached");
    assert_eq!(cold["matched"], json!(0));
    assert_eq!(cold["dropped"], json!(0));
    assert_eq!(cold["total"], json!(0));
    assert_eq!(cold["truncated"], json!(false), "REQUIRED even when false");

    // An address this ROM never touches, watched across a real run.
    c.ok(
        "emulator/watchpoint_add",
        json!({"addr": "0x00FFFFF0", "len": 2}),
    );
    c.ok("emulator/run_frames", json!({"frames": 2}));
    let warm = hits(&mut c, json!({}));
    assert!(
        warm["seen"].as_u64().unwrap() > 0,
        "the recorder rode the run"
    );
    assert_eq!(
        warm["matched"],
        json!(0),
        "and found nothing — a real negative finding, distinguishable from the cold read above"
    );
}

/// **Reading hits does not consume them.** `hits()`, never `take_hits()`: a draining read is one client
/// stealing another's evidence on a shared bus — the same hazard §6.1 refuses for checkpoints — and it
/// would make a reply's own `total` unreproducible.
#[test]
fn reading_hits_is_non_destructive() {
    let (_h, mut c) = armed("wp-poll");
    c.ok(
        "emulator/watchpoint_add",
        json!({"addr": SENTINEL, "len": 2}),
    );
    c.ok("emulator/run_frames", json!({"frames": 3}));
    let first = hits(&mut c, json!({}));
    let second = hits(&mut c, json!({}));
    assert!(first["total"].as_u64().unwrap() > 0);
    assert_eq!(first["hits"], second["hits"], "two identical polls");
    assert_eq!(first["total"], second["total"]);
    assert_eq!(first["matched"], second["matched"]);
}

/// A hit's `frame`/`mclk` live **inside** `hits[]` and never at the top level, where §2.2's envelope stamp
/// would overwrite them with the machine's *current* coordinate — a silent wrong answer of exactly the
/// class D11 exists to prevent.
#[test]
fn a_hits_coordinate_is_not_shadowed_by_the_envelope_stamp() {
    let (_h, mut c) = armed("wp-stamp");
    c.ok(
        "emulator/watchpoint_add",
        json!({"addr": SENTINEL, "len": 2}),
    );
    c.ok("emulator/run_frames", json!({"frames": 5}));
    let got = hits(&mut c, json!({}));
    let machine_now = got["frame"].as_u64().expect("the envelope stamp");
    let hit_frame = got["hits"][0]["frame"].as_u64().expect("the hit's own");
    assert!(
        hit_frame < machine_now,
        "the first hit is at frame {hit_frame}, the machine is at {machine_now}"
    );
    assert!(got["hits"][0]["mclk"].as_u64().unwrap() < got["mclk"].as_u64().unwrap());
    // `first`/`last` use the same nested shape for the same reason.
    let w = c.ok("emulator/watchpoint_list", json!({}))["watches"][0].clone();
    assert!(w["first"]["frame"].as_u64().unwrap() <= w["last"]["frame"].as_u64().unwrap());
    assert!(w["first"]["seq"].as_u64().unwrap() < w["last"]["seq"].as_u64().unwrap());
}

/// **Three kinds of loss, three different numbers.** `dropped` is loss at *record* time (the ring was at
/// capacity), `truncated` is loss at *read* time, and `seq` is stable across ring drops so a gap in it
/// marks what the ring discarded. All three can be true at once, and this fixture makes them so.
#[test]
fn the_three_kinds_of_loss_are_three_different_numbers() {
    let (_h, mut c) = armed("wp-drop");
    // The main loop stirs 0x4000 words of work RAM per pass, so this overruns any ring within one frame.
    c.ok(
        "emulator/watchpoint_add",
        json!({"addr": "0x00FF0000", "len": 0x10000}),
    );
    c.ok("emulator/run_frames", json!({"frames": 2}));
    let got = hits(&mut c, json!({"limit": 10}));
    assert!(
        got["dropped"].as_u64().unwrap() > 0,
        "the ring discarded, and said so"
    );
    assert_eq!(got["truncated"], json!(true), "and the page is short too");
    assert_eq!(got["returned"], json!(10));
    assert!(
        got["total"].as_u64().unwrap() > 10,
        "more is held than paged"
    );
    assert!(
        got["matched"].as_u64().unwrap() > got["total"].as_u64().unwrap(),
        "matched counts accesses, total counts what the ring still holds"
    );
    // The first live hit's `seq` is *not* 0: the gap is what the drop left behind, which is why `seq` is
    // assigned to every matched access rather than to every stored one.
    assert!(got["hits"][0]["seq"].as_u64().unwrap() > 0);
}

/// The cursor invariant (§2.4 clause (c)): resuming from a cursor skips no live hit and repeats none.
#[test]
fn the_hits_cursor_neither_skips_nor_repeats_a_live_hit() {
    let (_h, mut c) = armed("wp-cursor");
    c.ok(
        "emulator/watchpoint_add",
        json!({"addr": SENTINEL, "len": 2}),
    );
    c.ok("emulator/run_frames", json!({"frames": 8}));
    let all = hits(&mut c, json!({}));
    let total = all["total"].as_u64().unwrap();
    assert!(total >= 4, "enough hits to page: {total}");

    let page1 = hits(&mut c, json!({"limit": 2}));
    assert_eq!(page1["truncated"], json!(true));
    let cursor = page1["cursor"].as_str().expect("more remain").to_string();
    // Arm another watch and run more frames between the two pages — the mutation the invariant is about.
    c.ok(
        "emulator/watchpoint_add",
        json!({"addr": "0x00FF8004", "len": 2}),
    );
    c.ok("emulator/run_frames", json!({"frames": 2}));

    let mut seen: Vec<u64> = page1["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["seq"].as_u64().unwrap())
        .collect();
    let mut cursor = Some(cursor);
    while let Some(cur) = cursor.take() {
        let page = hits(&mut c, json!({"cursor": cur, "limit": 3}));
        for h in page["hits"].as_array().unwrap() {
            let seq = h["seq"].as_u64().unwrap();
            assert!(!seen.contains(&seq), "seq {seq} delivered twice");
            seen.push(seq);
        }
        cursor = page["cursor"].as_str().map(str::to_string);
    }
    // Every hit that was live at the first request is in the union, and in order.
    let live_now: Vec<u64> = hits(&mut c, json!({"limit": 4096}))["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["seq"].as_u64().unwrap())
        .collect();
    for seq in live_now.iter().take(total as usize) {
        assert!(seen.contains(seq), "seq {seq} was skipped by the paging");
    }
    assert!(seen.windows(2).all(|w| w[0] < w[1]), "and stayed in order");
}

// ---------------------------------------------------------------------------------------------------
// 3. The stop condition, and CR-9
// ---------------------------------------------------------------------------------------------------

/// **`stopAfter` ends the run, and the `stopped` event names the watch that did it** — §6: *"the halt
/// always names its watch"*. `reason` stays `watchpoint`, the enum member §3 has always defined and no
/// catalogued method could previously produce; the identity of the watch rides as an additive param,
/// which is §11.7's house rule (`reason` is a vocabulary of *conditions*; anything naming which instance
/// fired is a param).
///
/// This is stop-on-condition and not an interactive break: the triggering instruction has fully committed
/// when the run ends, and the run was already bounded by its own caller.
#[test]
fn a_stop_after_watch_ends_the_run_and_the_stopped_event_names_it() {
    let (_h, mut c) = armed("wp-stopafter");
    let handle = c.ok(
        "emulator/watchpoint_add",
        json!({"addr": SENTINEL, "len": 2, "stopAfter": 2}),
    )["watch"]
        .as_str()
        .unwrap()
        .to_string();
    // `stopAfter` is echoed on the result, and listed, so a watch that can change a later capture's
    // outcome is visible rather than discovered.
    let list = c.ok("emulator/watchpoint_list", json!({}));
    assert_eq!(list["watches"][0]["stopAfter"], json!(2));

    c.send_raw(
        &json!({"jsonrpc":"2.0","id":900,"method":"emulator/run_frames","params":{"frames":60}})
            .to_string(),
    );
    let mut stopped = None;
    loop {
        let line = c.recv();
        if line["method"] == json!("emulator/stopped") {
            stopped = Some(line["params"].clone());
        }
        if line["id"] == json!(900) {
            break;
        }
    }
    let p = stopped.expect("a stopped event");
    assert_eq!(p["reason"], json!("watchpoint"), "not runFrames: {p}");
    assert_eq!(p["watch"], json!(handle), "the halt names its cause");
    assert_eq!(
        p["deadlineReached"],
        json!(false),
        "it ended on a condition, not on its bound"
    );
    assert!(
        p.get("frames").is_none(),
        "`frames` is required only for reason runFrames"
    );
    // It really did stop early: 60 frames were asked for and the machine is nowhere near them.
    let status = c.ok("emulator/status", json!({}));
    assert!(
        status["frameToken"].as_u64().unwrap() < 60,
        "the run was cut short at {}",
        status["frameToken"]
    );
    // And the watch matched at least its threshold, with the triggering access recorded.
    let got = hits(&mut c, json!({}));
    assert!(got["matched"].as_u64().unwrap() >= 2);
}

/// ## CR-9 — **`buttons`/`port` are present iff `emulator/press` drove the advance.**
///
/// The enforcement is deliberately asymmetric and this test is the half that cannot be mechanised. §3 was
/// widened so `reason: "runFrames"` names *the condition* — an exhausted frame count — and therefore covers
/// `press` as well as `run_frames`; the event carries no method discriminator, which is exactly why a
/// schema `if`/`then` cannot express "was this a press?". `dependentRequired` enforces the half that can be
/// (the two travel together, because a subscriber told which buttons went down and not which pad would
/// attribute the input to the wrong controller). The behavioural half is this assertion.
#[test]
fn press_stops_carry_buttons_and_port_and_run_frames_does_not() {
    let (_h, mut c) = armed("wp-cr9");

    let stop_params = |c: &mut Client, id: i64, method: &str, params: Value| -> Value {
        c.send_raw(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string());
        let mut stopped = None;
        loop {
            let line = c.recv();
            if line["method"] == json!("emulator/stopped") {
                stopped = Some(line["params"].clone());
            }
            if line["id"] == json!(id) {
                break;
            }
        }
        stopped.expect("a stopped event")
    };

    let p = stop_params(
        &mut c,
        901,
        "emulator/press",
        json!({"buttons": ["start", "a"], "frames": 2, "port": 1}),
    );
    assert_eq!(
        p["reason"],
        json!("runFrames"),
        "the CONDITION that ended the run, never the method that drove it"
    );
    assert_eq!(p["buttons"], json!(["start", "a"]));
    assert_eq!(p["port"], json!(1), "and never `buttons` without `port`");
    assert_eq!(p["frames"], json!(2));
    assert_eq!(p["deadlineReached"], json!(true));

    let p = stop_params(&mut c, 902, "emulator/run_frames", json!({"frames": 2}));
    assert_eq!(p["reason"], json!("runFrames"), "the same reason value");
    assert!(
        p.get("buttons").is_none() && p.get("port").is_none(),
        "no input was injected, so nothing is claimed about any: {p}"
    );
    // A subscriber MUST NOT read the absence of `buttons` as proof no input reached the machine — only
    // that *this advance* was not press-driven. `hold` is the counterexample, and it emits no stop at all.
    c.ok("emulator/hold", json!({"buttons": ["b"]}));
    let p = stop_params(&mut c, 903, "emulator/run_frames", json!({"frames": 1}));
    assert!(
        p.get("buttons").is_none(),
        "a held button is not a press-driven advance"
    );
}

/// A `watchpoint` stop and a `runFrames` stop are the only two a bounded advance can produce, and the
/// `watch` param's presence rule is enforced in **both** directions by the schema (unlike CR-9's pair, this
/// one has a discriminator in the event). This asserts the negative half behaviourally too.
#[test]
fn only_a_watchpoint_stop_carries_a_watch_param() {
    let (_h, mut c) = armed("wp-nowatch");
    c.send_raw(
        &json!({"jsonrpc":"2.0","id":904,"method":"emulator/run_frames","params":{"frames":1}})
            .to_string(),
    );
    loop {
        let line = c.recv();
        if line["method"] == json!("emulator/stopped") {
            assert_eq!(line["params"]["reason"], json!("runFrames"));
            assert!(
                line["params"].get("watch").is_none(),
                "a non-watchpoint stop must not name one: {}",
                line["params"]
            );
        }
        if line["id"] == json!(904) {
            break;
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// 4. Params, refused by name
// ---------------------------------------------------------------------------------------------------

/// The refusals §6 and §5 ask for, each naming what was wrong.
#[test]
fn malformed_arming_is_refused_and_named() {
    let (_h, mut c) = armed("wp-refuse");
    // A symbol names a 68000 address; a VDP-internal byte address has no symbol.
    let e = c.err(
        "emulator/watchpoint_add",
        json!({"space": "vram", "symbol": "anything"}),
    );
    assert_eq!(e["code"], json!(-32602));
    assert!(e["message"].as_str().unwrap().contains("symbol"));

    // A watch that can never match is refused rather than silently turned into a write watch: its reading
    // is `seen` positive and `matched` zero, which is what a genuine negative finding looks like.
    let e = c.err(
        "emulator/watchpoint_add",
        json!({"addr": SENTINEL, "read": false, "write": false}),
    );
    assert_eq!(e["code"], json!(-32602));

    // A range whose end is off the bus is refused, never clipped.
    let e = c.err(
        "emulator/watchpoint_add",
        json!({"addr": "0x00FFFFFF", "len": 2}),
    );
    assert_eq!(e["code"], json!(-32004));

    let e = c.err(
        "emulator/watchpoint_add",
        json!({"addr": SENTINEL, "space": "vdp"}),
    );
    assert_eq!(e["code"], json!(-32602));

    // A cursor this server never issued is a typo, and is named rather than answered with an
    // honest-looking empty page.
    let e = c.err("emulator/watchpoint_list", json!({"cursor": "w42"}));
    assert_eq!(e["code"], json!(-32602));
    let e = c.err("emulator/watchpoint_hits", json!({"watch": "w42"}));
    assert_eq!(e["code"], json!(-32602));
}

/// The op filter resolves to what §6 pins, and the result says which — so a caller that supplied neither
/// `read` nor `write` is told it got a write watch rather than left to assume.
#[test]
fn the_op_filter_defaults_to_write_and_the_result_says_so() {
    let (_h, mut c) = armed("wp-op");
    let cases = [
        (json!({}), "write"),
        (json!({"write": true}), "write"),
        (json!({"read": true}), "read"),
        (json!({"read": true, "write": true}), "any"),
    ];
    for (extra, want) in cases {
        let mut params = json!({"addr": SENTINEL});
        for (k, v) in extra.as_object().unwrap() {
            params[k] = v.clone();
        }
        let r = c.ok("emulator/watchpoint_add", params.clone());
        assert_eq!(r["op"], json!(want), "for {params}");
        assert_eq!(r["space"], json!("bus"), "the default space");
        assert_eq!(r["len"], json!(1), "the default length");
        assert_eq!(r["mode"], json!("record"), "the default mode");
        c.ok(
            "emulator/watchpoint_clear",
            json!({"watch": r["watch"].clone()}),
        );
    }
}

/// `all: true` retires every watch; `watch` and `all` together are refused rather than one silently
/// winning.
#[test]
fn clear_all_retires_every_watch() {
    let (_h, mut c) = armed("wp-clear-all");
    for _ in 0..3 {
        c.ok(
            "emulator/watchpoint_add",
            json!({"addr": "0x00FF0000", "len": 1}),
        );
    }
    let e = c.err(
        "emulator/watchpoint_clear",
        json!({"all": true, "watch": "w0"}),
    );
    assert_eq!(e["code"], json!(-32602));
    assert_eq!(
        c.ok("emulator/watchpoint_clear", json!({"all": true}))["removed"],
        json!(3)
    );
    assert_eq!(
        c.ok("emulator/watchpoint_list", json!({}))["total"],
        json!(0)
    );
}

/// The capability object is the discoverable half: a client must not have to arm a watch to learn which
/// spaces exist, how many watches it may hold, or how deep the ring is.
#[test]
fn the_capability_advertises_the_numbers_a_client_has_to_plan_around() {
    let h = spawn("wp-caps");
    let mut c = Client::connect(&h);
    let init = c.handshake(false);
    let w = &init["capabilities"]["watchpoints"];
    assert_eq!(w["supported"], json!(true));
    assert_eq!(w["spaces"], json!(["bus", "vram", "cram", "vsram"]));
    assert!(w["maxWatches"].as_u64().unwrap() >= 1);
    assert!(w["ringCap"].as_u64().is_some());
    // …and the four methods are in the authoritative list (D4), not merely in the catalog.
    let methods: Vec<&str> = init["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap())
        .collect();
    for m in [
        "emulator/watchpoint_add",
        "emulator/watchpoint_clear",
        "emulator/watchpoint_list",
        "emulator/watchpoint_hits",
    ] {
        assert!(methods.contains(&m), "{m} is not advertised");
    }
}
