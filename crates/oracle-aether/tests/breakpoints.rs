//! **The breakpoint surface (`protocol.md` §6, §11.21 — CR-BP), over the real wire.**
//!
//! Every line these tests receive is validated against the vendored contract schema by
//! [`common::schema`] and *closed* against its fragment (§8 item 20), so a surplus key or a wrong JSON
//! type fails here without anyone writing an assertion for it — including on the `emulator/stopped`
//! events, where the `breakpoint`-iff-`reason: "breakpoint"` rule is schema-enforced. What is asserted
//! below is the half a schema structurally cannot check.
//!
//! Three of these are the ones that would have made this parcel not worth shipping if they failed, and
//! they are marked where they sit:
//!
//! * [`a_breakpoint_actually_halts_a_free_running_machine`] — a surface that reports success and arms
//!   nothing is worse than an unimplemented one.
//! * [`a_wait_does_not_stall_another_client`] — `wait_for_break` is poll-shaped on an async transport,
//!   and a naive implementation stalls every other client *including the one that would end the wait*.
//! * [`resuming_from_a_breakpoint_address_makes_progress`] — the legacy server's defect 1, which three
//!   `aeon` tools carry a hand-written workaround for.

#![cfg(unix)]

mod common;

use common::{spawn, Client};
use oracle_aether::server::ServerHandle;
use serde_json::{json, Value};
use std::time::Instant;

/// A PC the fixture ROM executes constantly: `move.w (A0), D0`, the head of `testrom::build`'s inner
/// stirring loop, which runs 0x4000 times per outer pass and never stops. Read from `testrom.rs` rather
/// than measured, so a ROM change breaks this loudly instead of silently un-arming the fixture.
const HOT_PC: &str = "0x0000020E";

/// A PC the fixture ROM never reaches: the head of `ILLEGAL_H`, reachable only through vector 4, which
/// this ROM's main loop cannot take. The negative control — a breakpoint here must never fire.
const COLD_PC: &str = "0x00000280";

fn armed(tag: &str) -> (ServerHandle, Client) {
    let h = spawn(tag);
    let mut c = Client::connect(&h);
    c.handshake(false);
    (h, c)
}

/// Read lines until the `emulator/stopped` this test is waiting for. Panics through `recv` on a hung
/// transport, which is the right failure: a stop that never arrives is the bug.
fn next_stopped(c: &mut Client) -> Value {
    loop {
        let v = c.recv();
        if v.get("method").and_then(Value::as_str) == Some("emulator/stopped") {
            return v["params"].clone();
        }
    }
}

fn add(c: &mut Client, params: Value) -> String {
    c.ok("emulator/breakpoint_add", params)["breakpoint"]
        .as_str()
        .expect("a breakpoint handle")
        .to_string()
}

fn list(c: &mut Client) -> Value {
    c.ok("emulator/breakpoint_list", json!({}))
}

/// The one row for `handle` in a `breakpoint_list` page, or `None` once it has been cleared.
fn row(list: &Value, handle: &str) -> Option<Value> {
    list["breakpoints"]
        .as_array()
        .expect("breakpoints[]")
        .iter()
        .find(|b| b["breakpoint"] == json!(handle))
        .cloned()
}

// ---------------------------------------------------------------------------------------------------
// 1. Identity is the handle (§11.21 design choice 1)
// ---------------------------------------------------------------------------------------------------

/// §6: *"a second `breakpoint_add` at an address that already has one is **not** a duplicate error and
/// **not** an idempotent echo — it is a second breakpoint"*, each with *"its own `enabled` and `hits`"*.
///
/// This is the whole amendment in one fixture: the address-keyed surface it replaced could not have
/// passed it, because it had nowhere to put the second entry.
#[test]
fn one_address_carries_several_breakpoints() {
    let (_h, mut c) = armed("bp-two-at-one");
    let a = add(&mut c, json!({"addr": HOT_PC, "label": "first"}));
    let b = add(&mut c, json!({"addr": HOT_PC, "label": "second"}));
    assert_ne!(
        a, b,
        "a re-add at an occupied address must issue a NEW handle"
    );

    let l = list(&mut c);
    assert_eq!(l["total"], json!(2), "both must be held, not merged");
    assert_eq!(row(&l, &a).expect("first")["label"], json!("first"));
    assert_eq!(row(&l, &b).expect("second")["label"], json!("second"));

    // …and they are independent objects, not two names for one.
    c.ok(
        "emulator/breakpoint_set_enabled",
        json!({"breakpoint": b, "enabled": false}),
    );
    let l = list(&mut c);
    assert_eq!(row(&l, &a).expect("first")["enabled"], json!(true));
    assert_eq!(row(&l, &b).expect("second")["enabled"], json!(false));
}

/// §11.21: a handle is *"server-assigned, **never reused**, so a stale handle resolves to nothing rather
/// than to someone else's breakpoint"*. `clear {all}` is the hardest case for that promise, because it is
/// where a naive counter would be tempted to rewind.
#[test]
fn a_handle_is_never_reused() {
    let (_h, mut c) = armed("bp-never-reused");
    let first = add(&mut c, json!({"addr": HOT_PC}));
    assert_eq!(
        c.ok("emulator/breakpoint_clear", json!({"all": true}))["removed"],
        json!(1)
    );
    let second = add(&mut c, json!({"addr": HOT_PC}));
    assert_ne!(
        first, second,
        "a handle issued after a clear-all must not be one already handed out"
    );
    // And the stale one resolves to NOTHING — never to the live breakpoint at the same address, which is
    // the silent mis-target the amendment exists to prevent.
    assert_eq!(
        c.ok("emulator/breakpoint_clear", json!({"breakpoint": first}))["removed"],
        json!(0)
    );
    assert_eq!(list(&mut c)["total"], json!(1), "the live one is untouched");
}

// ---------------------------------------------------------------------------------------------------
// 2. `enabled` has exactly one writer (§11.21 design choice 2, audit D-13)
// ---------------------------------------------------------------------------------------------------

/// §6: *"A disabled breakpoint keeps its handle, its label and its `hits`, and does not halt. `hits`
/// counts firings while enabled and is never reset by this surface."*
#[test]
fn set_enabled_carries_hits_across_the_toggle() {
    let (_h, mut c) = armed("bp-toggle-hits");
    let bp = add(&mut c, json!({"addr": HOT_PC, "label": "keep me"}));
    // Earn a hit, so "carried across" is a claim about a non-zero number.
    c.ok("emulator/resume", json!({}));
    c.ok("emulator/wait_for_break", json!({"timeoutMs": 5000}));
    let hits = row(&list(&mut c), &bp).expect("live")["hits"]
        .as_u64()
        .expect("hits");
    assert!(hits >= 1, "the breakpoint must have fired at least once");

    let off = c.ok(
        "emulator/breakpoint_set_enabled",
        json!({"breakpoint": bp, "enabled": false}),
    );
    assert_eq!(off["enabled"], json!(false));
    assert_eq!(
        off["hits"],
        json!(hits),
        "disabling must not reset the count — a client wanting a fresh one clears and re-adds"
    );
    let r = row(&list(&mut c), &bp).expect("still held");
    assert_eq!(
        r["label"],
        json!("keep me"),
        "a disabled bp keeps its label"
    );
    assert_eq!(r["hits"], json!(hits));
}

/// §6 states the asymmetry and its reason: `set_enabled` on a handle the server does not hold *"refuses
/// with `-32005 {reason:'unknownBreakpoint'}` (a client that thinks it is toggling something must learn
/// it is toggling nothing)"*, while `clear` *"succeeds with `removed: 0`"*.
///
/// One fixture for both, because the value is in the **contrast**: two tests could each pass while the
/// pair had been made uniform in the wrong direction.
#[test]
fn a_toggle_refuses_what_a_clear_forgives() {
    let (_h, mut c) = armed("bp-unknown-handle");
    let stale = add(&mut c, json!({"addr": HOT_PC}));
    c.ok("emulator/breakpoint_clear", json!({"breakpoint": stale}));

    let e = c.err(
        "emulator/breakpoint_set_enabled",
        json!({"breakpoint": stale, "enabled": true}),
    );
    assert_eq!(e["code"], json!(-32005));
    assert_eq!(e["data"]["reason"], json!("unknownBreakpoint"));

    assert_eq!(
        c.ok("emulator/breakpoint_clear", json!({"breakpoint": stale}))["removed"],
        json!(0),
        "a delete that finds nothing has reached its goal"
    );
}

// ---------------------------------------------------------------------------------------------------
// 3. The cap (§11.21 design choice 3)
// ---------------------------------------------------------------------------------------------------

/// §6: *"There is a cap, advertised as `limits.maxBreakpoints`"*, and at it `breakpoint_add` *"MUST fail
/// with `-32005` carrying `{"reason":"breakpointCapReached","cap":n,"count":n}` and MUST NOT silently
/// grow past the advertised number."*
///
/// The cap is read from the handshake rather than written down here: a test that hard-codes 32 stops
/// testing the contract the moment the number is configured differently, and it is the *advertisement*
/// that §11.21 makes normative — a cap a client can only discover by hitting it is the defect.
#[test]
fn the_advertised_cap_is_the_cap_that_is_enforced() {
    let h = spawn("bp-cap");
    let mut c = Client::connect(&h);
    let init = c.handshake(false);
    assert_eq!(
        init["capabilities"]["breakpoints"],
        json!(true),
        "the family is served, so the boolean capability must say so"
    );
    assert!(
        init["methods"]
            .as_array()
            .expect("methods[]")
            .contains(&json!("emulator/breakpoint_set_enabled")),
        "§11.21: the PRESENCE of breakpoint_set_enabled in `methods` is how a client tells the handle \
         shape from the pre-amendment address shape"
    );
    let cap = init["limits"]["maxBreakpoints"]
        .as_u64()
        .expect("limits.maxBreakpoints is REQUIRED once the family is advertised");

    for _ in 0..cap {
        add(&mut c, json!({"addr": HOT_PC}));
    }
    let e = c.err("emulator/breakpoint_add", json!({"addr": HOT_PC}));
    assert_eq!(e["code"], json!(-32005));
    assert_eq!(e["data"]["reason"], json!("breakpointCapReached"));
    assert_eq!(e["data"]["cap"], json!(cap));
    assert_eq!(e["data"]["count"], json!(cap));
    assert_eq!(
        list(&mut c)["total"],
        json!(cap),
        "MUST NOT silently grow past the advertised number"
    );
}

// ---------------------------------------------------------------------------------------------------
// 4. `all` is the one deliberately shared verb (§11.21 design choice 5)
// ---------------------------------------------------------------------------------------------------

/// §6: *"`breakpoint_clear {all:true}` removes every breakpoint on the server, **including other
/// clients'** — that is the one deliberately shared verb."* Two connections, because one cannot
/// distinguish "shared" from "mine".
#[test]
fn clear_all_reaches_another_clients_breakpoints() {
    let h = spawn("bp-clear-all-shared");
    let mut mine = Client::connect(&h);
    mine.handshake(false);
    let mut theirs = Client::connect(&h);
    theirs.handshake(false);

    let ours = add(&mut mine, json!({"addr": HOT_PC}));
    let bp = add(&mut theirs, json!({"addr": COLD_PC}));
    assert_eq!(list(&mut mine)["total"], json!(2), "one bus, one set");

    assert_eq!(
        theirs.ok("emulator/breakpoint_clear", json!({"all": true}))["removed"],
        json!(2),
        "`all` is not scoped to the connection that calls it"
    );
    let l = list(&mut mine);
    assert_eq!(l["total"], json!(0));
    assert!(row(&l, &ours).is_none() && row(&l, &bp).is_none());
}

// ---------------------------------------------------------------------------------------------------
// 5. The surface arms something — the failure that would make this parcel not worth shipping
// ---------------------------------------------------------------------------------------------------

/// **A breakpoint reported as added must actually halt the machine**, and the `stopped` event must name
/// the handle that caused it (§11.21's M2 clarification (ii): `breakpoint` is REQUIRED whenever `reason`
/// is `breakpoint`).
///
/// The free-run path is the one under test on purpose: `resume` → `wait_for_break` is the idiom every
/// live consumer of this surface uses, and it is the only path where the halt has to be raised by the
/// engine's own loop rather than by a bounded run a client asked for.
#[test]
fn a_breakpoint_actually_halts_a_free_running_machine() {
    let h = spawn("bp-halts-free-run");
    let mut events = Client::connect(&h);
    events.handshake(true);
    let mut c = Client::connect(&h);
    c.handshake(false);

    // **Both armed before the resume, deliberately.** Arming while running is legal (§6) and is asserted
    // in `the_whole_surface_is_legal_while_the_machine_runs` — but doing it *here* would race the halt: on
    // a fixture whose breakpoint address executes every few microseconds, the first firing lands before a
    // second `breakpoint_add` can be dispatched, and the "every enabled breakpoint increments" claim below
    // would be measuring the scheduler rather than the rule.
    let bp = add(&mut c, json!({"addr": HOT_PC, "label": "inner"}));
    let hot = add(
        &mut c,
        json!({"addr": HOT_PC, "label": "second at one address"}),
    );
    c.ok("emulator/resume", json!({}));

    let r = c.ok("emulator/wait_for_break", json!({"timeoutMs": 5000}));
    assert!(
        r.get("timeoutReached") != Some(&json!(true)),
        "the machine must actually have stopped: {r}"
    );
    assert_eq!(
        r["running"],
        json!(false),
        "the stamp must agree that the machine is halted — every reading a client takes next depends \
         on it"
    );
    assert_eq!(
        r["pc"],
        json!(HOT_PC),
        "a breakpoint halts BEFORE the instruction at its address executes"
    );

    let stopped = next_stopped(&mut events);
    assert_eq!(stopped["reason"], json!("breakpoint"));
    assert_eq!(stopped["pc"], json!(HOT_PC));
    assert_eq!(
        stopped["breakpoint"],
        json!(bp),
        "§6 pins the named handle to the EARLIEST-ADDED enabled breakpoint at the address"
    );

    // "every enabled breakpoint at that address increments its `hits`", though the event names one.
    let l = list(&mut c);
    assert_eq!(row(&l, &bp).expect("live")["hits"], json!(1));
    assert_eq!(
        row(&l, &hot).expect("live")["hits"],
        json!(1),
        "the second breakpoint at the same address counts the same firing"
    );
}

/// §6: *"A disabled breakpoint … does not halt."* The negative control for the fixture above — without
/// it, an implementation that halts on every armed address regardless of `enabled` passes everything.
#[test]
fn a_disabled_breakpoint_does_not_halt() {
    let (_h, mut c) = armed("bp-disabled-silent");
    let bp = add(&mut c, json!({"addr": HOT_PC, "enabled": false}));
    c.ok("emulator/resume", json!({}));
    let r = c.ok("emulator/wait_for_break", json!({"timeoutMs": 250}));
    assert_eq!(
        r["timeoutReached"],
        json!(true),
        "a disabled breakpoint at a constantly-executed PC must not stop the machine: {r}"
    );
    assert_eq!(row(&list(&mut c), &bp).expect("live")["hits"], json!(0));
}

/// **The legacy server's defect 1.** `BusEventSink::on_step_boundary` fires again for the stopping PC
/// when the caller resumes, so a sink without a resume-PC latch halts at the same instruction forever:
/// *"the sweep arm ran 24 iterations against ONE frozen tick"*
/// (`aeon/tools/parallax_hscroll_probe.py`).
///
/// Asserted on the emulated clock, not on the hit count: a server that never advanced would still report
/// a rising `hits`, which is exactly the shape that made the defect survive as long as it did.
#[test]
fn resuming_from_a_breakpoint_address_makes_progress() {
    let (_h, mut c) = armed("bp-resume-progress");
    let bp = add(&mut c, json!({"addr": HOT_PC}));
    c.ok("emulator/resume", json!({}));
    let first = c.ok("emulator/wait_for_break", json!({"timeoutMs": 5000}));
    let mclk0 = first["mclk"].as_u64().expect("the stamp's mclk");

    // Resume from the breakpoint's own address, with no `step` in front of it.
    c.ok("emulator/resume", json!({}));
    let second = c.ok("emulator/wait_for_break", json!({"timeoutMs": 5000}));
    assert!(
        second.get("timeoutReached") != Some(&json!(true)),
        "the second arrival must be reached: {second}"
    );
    let mclk1 = second["mclk"].as_u64().expect("the stamp's mclk");
    assert!(
        mclk1 > mclk0,
        "the machine must ADVANCE between two stops at one address — {mclk0} -> {mclk1} is the frozen \
         tick the workaround in aeon's tools exists to route around"
    );
    assert_eq!(
        row(&list(&mut c), &bp).expect("live")["hits"],
        json!(2),
        "two arrivals, two firings — the resume repeat must not be counted as a third"
    );
}

// ---------------------------------------------------------------------------------------------------
// 6. `wait_for_break` and the transport
// ---------------------------------------------------------------------------------------------------

/// **The named failure mode for this parcel.** `wait_for_break` is poll-shaped on an async transport; a
/// handler that slept on the engine thread would stall every other client — including the one that would
/// tell it to stop — and would guarantee its own timeout, because the engine thread's free-run step is
/// what advances the machine toward the break.
///
/// One client waits on a breakpoint that can never fire; a second must be served **while that wait is
/// outstanding**. The bound is deliberately loose (a quarter of the wait): this asserts "not serialised
/// behind the wait", not a latency figure, so it cannot go red on a loaded box.
#[test]
fn a_wait_does_not_stall_another_client() {
    let h = spawn("bp-wait-concurrency");
    let mut waiter = Client::connect(&h);
    waiter.handshake(false);
    let mut other = Client::connect(&h);
    other.handshake(false);

    // A breakpoint the fixture ROM can never reach, so the wait runs its full budget.
    add(&mut waiter, json!({"addr": COLD_PC}));
    waiter.ok("emulator/resume", json!({}));

    let started = Instant::now();
    waiter.send_raw(
        &json!({"jsonrpc":"2.0","id":900,"method":"emulator/wait_for_break",
                "params":{"timeoutMs":2000}})
        .to_string(),
    );

    // …and now, with that wait outstanding, the OTHER client must be answered.
    let s = other.ok("emulator/status", json!({}));
    let served_after = started.elapsed();
    assert!(
        s.get("pc").is_some(),
        "the concurrent request must get a real answer, not an empty one"
    );
    assert!(
        served_after.as_millis() < 500,
        "a second client was served only after {served_after:?} — the wait is serialising the bus, \
         which is the exact failure this method is most likely to introduce"
    );

    // The waiter still gets its own honest answer.
    let reply = waiter.recv_response();
    assert_eq!(reply["id"], json!(900));
    let r = &reply["result"];
    assert_eq!(
        r["timeoutReached"],
        json!(true),
        "a break that never fired is `timeoutReached: true` — true means SURRENDERED (D12's named \
         exemption, 2026-08-27)"
    );
    assert!(
        r.get("pc").is_none(),
        "no `pc` when the wait timed out with the machine still running — a PC sampled from a moving \
         machine names an instruction that has already gone"
    );
    assert!(
        r["waitedMs"].as_u64().expect("waitedMs") >= 1500,
        "waitedMs must be the wall clock actually spent, not the budget echoed back: {r}"
    );
    assert!(
        started.elapsed().as_millis() >= 1900,
        "the wait must genuinely have waited, or the fixture above proves nothing"
    );
}

/// §11.24 audit D-07: `timeoutMs` is *"≥0, def 30000, ≤300000, **refused above**"* — refused, never
/// clamped. And refused *promptly*: a five-minute sleep in front of a `-32602` would be the transport
/// hazard wearing a refusal's clothes.
#[test]
fn a_timeout_past_the_ceiling_is_refused_and_refused_at_once() {
    let (_h, mut c) = armed("bp-timeout-ceiling");
    c.ok("emulator/resume", json!({}));
    let started = Instant::now();
    let e = c.err("emulator/wait_for_break", json!({"timeoutMs": 300_001}));
    assert_eq!(e["code"], json!(-32602));
    assert!(
        started.elapsed().as_millis() < 500,
        "the refusal must not be preceded by a wait: {:?}",
        started.elapsed()
    );
}

/// §11.24 audit D-07: *"`0` polls once and returns."* On a running machine that is the one honest way to
/// ask "has it broken yet?" without blocking anything at all.
#[test]
fn a_zero_timeout_polls_once_and_returns() {
    let (_h, mut c) = armed("bp-zero-timeout");
    add(&mut c, json!({"addr": COLD_PC}));
    c.ok("emulator/resume", json!({}));
    let started = Instant::now();
    let r = c.ok("emulator/wait_for_break", json!({"timeoutMs": 0}));
    assert_eq!(r["timeoutReached"], json!(true));
    assert!(
        started.elapsed().as_millis() < 400,
        "a zero timeout must not wait: {:?}",
        started.elapsed()
    );
}

/// **The live-consumer finding, pinned.** Three tools in the `aeon` tree send `timeout_ms` (snake_case)
/// where the row says `timeoutMs`. §2.5's params closure refuses it, which is the wanted outcome — a
/// loud refusal rather than a silently-defaulted 30-second wait — and there is deliberately **no alias**:
/// two spellings for one parameter is how a vocabulary rots.
///
/// Pinned as a test so that "we serve the contract spelling" stays a property rather than a decision
/// somebody remembers.
#[test]
fn the_snake_case_spelling_is_refused_rather_than_aliased() {
    let (_h, mut c) = armed("bp-snake-case");
    let started = Instant::now();
    let e = c.err("emulator/wait_for_break", json!({"timeout_ms": 6000}));
    assert_eq!(e["code"], json!(-32602));
    assert!(
        started.elapsed().as_millis() < 500,
        "an unknown param must be refused before any wait: {:?}",
        started.elapsed()
    );
}

// ---------------------------------------------------------------------------------------------------
// 7. `breakpoint_list` is a §2.4 paged collection
// ---------------------------------------------------------------------------------------------------

/// §2.4 clause (a): `total`/`returned`/`truncated` are required even when the page is complete, and
/// `total` is the whole set rather than the page. Clause (b): a `cursor` is emitted only when more
/// remain, and this row accepts one so it may emit one.
#[test]
fn the_list_pages_and_the_cursor_walks_the_whole_set() {
    let (_h, mut c) = armed("bp-paging");
    let mut all = Vec::new();
    for _ in 0..5 {
        all.push(add(&mut c, json!({"addr": HOT_PC})));
    }

    let complete = list(&mut c);
    assert_eq!(complete["total"], json!(5));
    assert_eq!(complete["returned"], json!(5));
    assert_eq!(
        complete["truncated"],
        json!(false),
        "REQUIRED even when false (§2.4 clause (a))"
    );
    assert!(
        complete.get("cursor").is_none(),
        "no cursor when nothing remains"
    );

    let mut seen = Vec::new();
    let mut pages = 0;
    let mut page = c.ok("emulator/breakpoint_list", json!({"limit": 2}));
    loop {
        pages += 1;
        assert_eq!(page["total"], json!(5), "`total` is the SET, not the page");
        let items = page["breakpoints"].as_array().expect("breakpoints[]");
        // **The page ceiling is a ceiling.** Without this the whole loop passes against a server that
        // ignores `limit` and returns everything on page one — it would still visit each handle exactly
        // once, which is all the rest of this fixture checks.
        assert!(
            items.len() <= 2,
            "`limit: 2` must bound the page: got {} rows",
            items.len()
        );
        assert_eq!(page["returned"], json!(items.len()));
        for b in items {
            seen.push(b["breakpoint"].as_str().expect("handle").to_string());
        }
        let Some(cursor) = page.get("cursor").and_then(Value::as_str) else {
            assert_eq!(
                page["truncated"],
                json!(false),
                "the last page is not truncated"
            );
            break;
        };
        assert_eq!(page["truncated"], json!(true));
        let cursor = cursor.to_string();
        page = c.ok(
            "emulator/breakpoint_list",
            json!({"limit": 2, "cursor": cursor}),
        );
    }
    assert_eq!(seen, all, "paging must visit every breakpoint exactly once");
    assert_eq!(
        pages, 3,
        "5 breakpoints at 2 a page is three pages, not one"
    );
}

// ---------------------------------------------------------------------------------------------------
// 8. Refusals that name the real mistake
// ---------------------------------------------------------------------------------------------------

/// The `addr`-XOR-`symbol` alternation the fragment enforces mechanically, and the two symbol codes §6
/// names: `-32012` when no symbols are loaded, `-32013` when the symbol is unknown. This fixture's server
/// has no symbol table, so it pins the first.
#[test]
fn add_refuses_the_shapes_the_fragment_forbids() {
    let (_h, mut c) = armed("bp-add-refusals");
    let both = c.err(
        "emulator/breakpoint_add",
        json!({"addr": HOT_PC, "symbol": "Anything"}),
    );
    assert_eq!(both["code"], json!(-32602));

    let neither = c.err("emulator/breakpoint_add", json!({}));
    assert_eq!(neither["code"], json!(-32602));

    let no_symbols = c.err("emulator/breakpoint_add", json!({"symbol": "Anything"}));
    assert_eq!(
        no_symbols["code"],
        json!(-32012),
        "no symbols loaded is its own code, not a generic bad-param"
    );

    // D9 category 4: a handle is opaque, and computing on it is what the string spelling forbids.
    let numeric = c.err(
        "emulator/breakpoint_set_enabled",
        json!({"breakpoint": 0, "enabled": true}),
    );
    assert_eq!(numeric["code"], json!(-32602));

    // A toggle whose argument may be omitted is a toggle whose caller cannot tell which way it went.
    let bp = add(&mut c, json!({"addr": HOT_PC}));
    let no_state = c.err(
        "emulator/breakpoint_set_enabled",
        json!({"breakpoint": bp.clone()}),
    );
    assert_eq!(no_state["code"], json!(-32602));

    // `breakpoint` and `all` are alternatives, and passing both is a request that names two different
    // intents — refused rather than silently resolved to one of them.
    let both_clear = c.err(
        "emulator/breakpoint_clear",
        json!({"breakpoint": bp, "all": true}),
    );
    assert_eq!(both_clear["code"], json!(-32602));
}

/// §6: *"Not subject to the run-control state rule"* — *"arming, toggling and clearing mutate an
/// observer, not the timeline, and are legal while running."* A server that required a paused machine
/// would force every client into a pause/arm/resume dance, which is the machine-state change §5 forbids
/// a server to make on a caller's behalf.
#[test]
fn the_whole_surface_is_legal_while_the_machine_runs() {
    let (_h, mut c) = armed("bp-legal-while-running");
    c.ok("emulator/resume", json!({}));
    let bp = add(&mut c, json!({"addr": COLD_PC}));
    c.ok(
        "emulator/breakpoint_set_enabled",
        json!({"breakpoint": bp.clone(), "enabled": false}),
    );
    list(&mut c);
    c.ok("emulator/breakpoint_clear", json!({"breakpoint": bp}));
    assert_eq!(
        c.ok("emulator/status", json!({}))["running"],
        json!(true),
        "and none of it paused the machine behind the caller's back"
    );
}
