//! **The three `step*` rows** — `protocol.md` §6 lines 851-853, served 2026-08-22.
//!
//! These were the first three of the 21 fragments that describe methods this server did not serve, and
//! which the 2026-08-22 adoption turned into its acceptance contract. Their fragments landed upstream
//! final, so this is conformance work with no contract change behind it.
//!
//! # What the schema checks for free, and what it cannot
//!
//! Every line a [`Client`] receives is validated against the vendored fragment, closed with
//! `unevaluatedProperties: false` (`common::schema`, §8 item 15 / item 20). So the *shape* of every reply
//! and every event below is already the contract's own verdict rather than one typed here: a surplus key on
//! `emulator/step`'s result, a `caveat` the fragment declares absent, an address string where a
//! `$defs/symbolName` belongs, or a `stopped` carrying a `reason` outside the enum, all fail without any
//! assertion in this file.
//!
//! What that leaves for this file is everything the schema is structurally blind to, which is the whole of
//! the *behaviour* D14 puts under the prose:
//!
//! * **`reason: "step"` on all three.** §3 pins one stop condition across the three methods, so `step_over`
//!   and `step_out` get no reason of their own. `step` is a legal enum member for *any* stop, so the
//!   validator would accept it on a frame advance and accept `runFrames` here — the same blind spot
//!   `tests/events.rs` documents at length, pointed the other way.
//! * **That `step_over` is not a `step` in disguise.** The two have identical wire shapes on a call — one
//!   returns nothing and the other returns a `pc` the schema will accept whatever its value. Only running
//!   both from the same instruction and comparing where they land can tell them apart, which is
//!   [`step_over_a_call_runs_it_to_completion_while_step_enters_it`] and is the load-bearing test here.
//! * **That the machine advanced, and not merely the PC.** `System::step_instruction` moves the CPU without
//!   moving the master clock; a step built on it would return a perfectly conformant `pc` forever while the
//!   machine around it stood still. [`step_advances_the_whole_machine_not_just_the_cpu`] is the control.
//!
//! # Where the expectations come from
//!
//! Instruction addresses are derived from `oracle_core::testrom`'s own builder — the `ProfilerShape`
//! fixtures assemble `jsr`/`rts` at fixed, exported addresses precisely so a test can name them — and never
//! from a value this server was observed to produce. Each derivation is spelled out at its assertion.

mod common;

use common::{spawn_with, Client};
use oracle_core::testrom::{self, ProfilerShape, PROF_DISPATCH, PROF_LEAF, PROF_MID, PROF_TARGET};
use serde_json::{json, Value};

/// The three rows this file covers, so every sweep below is over the set rather than a sampled two.
const STEP_METHODS: [&str; 3] = ["emulator/step", "emulator/step_over", "emulator/step_out"];

fn hex(addr: u32) -> String {
    format!("0x{addr:08X}")
}

/// A server on the two-level profiler fixture: `MAIN` calls [`PROF_MID`], which calls [`PROF_LEAF`] twice
/// and returns. Chosen because its call tree is assembled at exported addresses, so "where should a
/// `step_over` land" has an answer read off `testrom.rs` rather than off this server.
fn two_level(tag: &str) -> (oracle_aether::server::ServerHandle, Client) {
    open(tag, ProfilerShape::TwoLevel)
}

fn open(tag: &str, shape: ProfilerShape) -> (oracle_aether::server::ServerHandle, Client) {
    let h = spawn_with(tag, testrom::build_profiler(shape), 1024);
    let mut c = Client::connect(&h);
    c.handshake(true);
    (h, c)
}

/// Drive the machine to `addr` and assert it arrived, so a test that means to start somewhere specific
/// fails at its setup instead of asserting about wherever it happened to be.
fn park_at(c: &mut Client, addr: u32) {
    let r = c.ok("emulator/run_to", json!({ "addr": hex(addr) }));
    assert_eq!(
        r["reached"],
        json!(true),
        "setup: the fixture never reached {}: {r}",
        hex(addr)
    );
    assert_eq!(r["pc"], json!(hex(addr)), "setup: parked at the wrong pc");
}

/// The `pc` `emulator/status` reports, as a `u32`.
fn pc_of(c: &mut Client) -> u32 {
    let s = c.ok("emulator/status", json!({}));
    let text = s["pc"].as_str().expect("status.pc is a hex string");
    u32::from_str_radix(text.trim_start_matches("0x"), 16).expect("status.pc parses")
}

/// Call `method`, collecting every notification the server pushes before the reply. Returns
/// `(result, events)`.
fn call_watching_events(c: &mut Client, method: &str, params: Value) -> (Value, Vec<Value>) {
    let id = 7700 + method.len() as i64;
    c.send_raw(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string());
    let mut events = Vec::new();
    loop {
        let v = c.recv();
        if v["id"] == json!(id) {
            assert!(v.get("error").is_none(), "{method} failed: {}", v["error"]);
            return (v["result"].clone(), events);
        }
        events.push(v);
    }
}

// ---------------------------------------------------------------------------------------------------
// The machine, not just the CPU
// ---------------------------------------------------------------------------------------------------

/// **The control against the wrong primitive.**
///
/// `System::step_instruction` steps the CPU over the real bus and returns the cycles it cost, and its own
/// doc records that it *"does not advance the master clock (the caller owns time)"* — no scheduler events
/// delivered, no Z80 catch-up, no IPL re-derive. A `step` built on it would hand back a `pc` that moved,
/// past a `$defs/hex` check, on a machine whose clock never ticked and whose interrupts can never arrive.
///
/// So the assertion is on `mclk`, which is the stamp D11 puts on every reply: it must move, and it must
/// move by a whole number of CPU cycles' worth of master clock. `MCLK_PER_CPU_CYCLE` is 7 and the shortest
/// 68000 instruction is 4 cycles, so any real step is at least 28 mclk; a step that advanced only the CPU
/// would leave the delta at exactly 0, which is the failure this pins.
#[test]
fn step_advances_the_whole_machine_not_just_the_cpu() {
    let (_h, mut c) = two_level("st-mclk");
    let before = c.ok("emulator/status", json!({}));
    let r = c.ok("emulator/step", json!({}));
    let delta = r["mclk"].as_u64().unwrap() - before["mclk"].as_u64().unwrap();
    assert!(
        delta >= oracle_core::system::MCLK_PER_CPU_CYCLE * 4,
        "a step must advance the master clock, not only the PC — mclk moved by {delta}"
    );
    assert_eq!(
        delta % oracle_core::system::MCLK_PER_CPU_CYCLE,
        0,
        "the clock advances in whole CPU cycles: {delta}"
    );
}

// ---------------------------------------------------------------------------------------------------
// emulator/step — the count
// ---------------------------------------------------------------------------------------------------

/// **`count` retires exactly that many instructions**, derived from the fixture's own assembly.
///
/// `testrom.rs` lays [`PROF_LEAF`] out as three instructions at known offsets — `nop` at `+0`, `nop` at
/// `+2`, `rts` at `+4` — so from `PROF_LEAF` the landing PC after *n* steps is `PROF_LEAF + 2n` for
/// `n <= 2`. Both are checked, so the test says "n steps retire n instructions" rather than "one step
/// works and the rest are assumed".
///
/// The third step is deliberately not asserted as an address: it retires the `rts`, and where that lands
/// is the caller's business rather than this fixture's arithmetic.
#[test]
fn step_count_retires_exactly_that_many_instructions() {
    let (_h, mut c) = two_level("st-count");
    park_at(&mut c, PROF_LEAF);
    let one = c.ok("emulator/step", json!({"count": 1}));
    assert_eq!(
        one["pc"],
        json!(hex(PROF_LEAF + 2)),
        "one step retires the `nop` at PROF_LEAF+0"
    );

    // And again from the top, asking for both at once: the same two instructions, one call.
    park_at(&mut c, PROF_LEAF);
    let two = c.ok("emulator/step", json!({"count": 2}));
    assert_eq!(
        two["pc"],
        json!(hex(PROF_LEAF + 4)),
        "two steps retire both `nop`s and stop before the `rts`"
    );
}

/// **An omitted `count` is one instruction** — and the default is *this server's*, because the contract
/// has none.
///
/// Audit D-02: §6's row states no default, no minimum above 0 and no ceiling, where every sibling count in
/// the catalog spells its bounds out. `1` is the only reading that matches the siblings and the word
/// "step", but it is a choice, so two conformant servers could disagree about `{}`. Pinned here so the
/// choice is visible and cannot drift silently while the defect is open upstream.
#[test]
fn an_omitted_count_is_one_instruction_and_that_default_is_ours_not_the_contracts() {
    let (_h, mut c) = two_level("st-default");
    park_at(&mut c, PROF_LEAF);
    let r = c.ok("emulator/step", json!({}));
    assert_eq!(
        r["pc"],
        json!(hex(PROF_LEAF + 2)),
        "an omitted count steps once (D-02: the contract states no default; this is ours)"
    );
}

/// **`count: 0` retires nothing, and is a real answer rather than an error.**
///
/// The fragment's `minimum` is `0`, so a zero count is a legal request and means what it says: a caller
/// establishing where the machine already is without moving it. Both halves are asserted — the PC must not
/// move *and* the clock must not either, because a "zero" step that still ran one instruction and returned
/// to the same PC would pass the first check alone on any loop.
#[test]
fn count_zero_retires_nothing_and_moves_no_clock() {
    let (_h, mut c) = two_level("st-zero");
    park_at(&mut c, PROF_LEAF);
    let before = c.ok("emulator/status", json!({}));
    let r = c.ok("emulator/step", json!({"count": 0}));
    assert_eq!(
        r["pc"],
        json!(hex(PROF_LEAF)),
        "count 0 must leave the pc where it was"
    );
    assert_eq!(
        r["mclk"], before["mclk"],
        "count 0 must not advance the machine either"
    );
}

/// `count` is refused when it is not a non-negative integer — the fragment types it `integer, minimum 0`,
/// and D9 makes a count a JSON number.
#[test]
fn a_negative_or_non_numeric_count_is_refused() {
    let (_h, mut c) = two_level("st-badcount");
    for bad in [json!(-1), json!("3"), json!(1.5), json!(null)] {
        let e = c.err("emulator/step", json!({ "count": bad }));
        assert_eq!(
            e["code"],
            json!(-32602),
            "count {bad} should be refused, got {e}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------
// step_over — the load-bearing distinction
// ---------------------------------------------------------------------------------------------------

/// **THE test of this parcel: `step_over` runs the call to completion; `step` enters it.**
///
/// A `step_over` that quietly behaved like a `step` would return the empty result its fragment requires,
/// emit a `stopped` with the pinned reason, and pass every schema check in this repo. Nothing about the
/// wire shape can tell the two apart, because the row has no result keys to disagree about. Only the
/// machine's position can.
///
/// The fixture makes that position exact. `testrom.rs` assembles [`PROF_MID`] as `jsr (LEAF).w` at `+0`,
/// a second `jsr (LEAF).w` at `+4`, and `rts` at `+8`. So from `PROF_MID`:
///
/// * `step` retires the `jsr` and lands **inside the callee**, at [`PROF_LEAF`];
/// * `step_over` runs the callee to its `rts` and lands on the **next instruction of the caller**, at
///   `PROF_MID + 4`.
///
/// Both are asserted from the same starting instruction, in the same test, so the pair is a difference
/// rather than two independent facts.
#[test]
fn step_over_a_call_runs_it_to_completion_while_step_enters_it() {
    let (_h, mut c) = two_level("st-over");

    park_at(&mut c, PROF_MID);
    let stepped = c.ok("emulator/step", json!({}));
    assert_eq!(
        stepped["pc"],
        json!(hex(PROF_LEAF)),
        "`step` over a jsr enters the callee"
    );

    park_at(&mut c, PROF_MID);
    c.ok("emulator/step_over", json!({}));
    assert_eq!(
        pc_of(&mut c),
        PROF_MID + 4,
        "`step_over` a jsr must land on the caller's NEXT instruction, having run the callee — \
         landing at PROF_LEAF would mean it is a `step` wearing another name"
    );
}

/// `step_over` on an instruction that is not a call is one instruction, which is the right answer rather
/// than a fallback: there is nothing to step over.
///
/// [`PROF_LEAF`] opens with two `nop`s, so stepping over the first lands on the second.
#[test]
fn step_over_a_non_call_is_exactly_one_instruction() {
    let (_h, mut c) = two_level("st-over-nop");
    park_at(&mut c, PROF_LEAF);
    c.ok("emulator/step_over", json!({}));
    assert_eq!(
        pc_of(&mut c),
        PROF_LEAF + 2,
        "stepping over a `nop` advances one instruction"
    );
}

// ---------------------------------------------------------------------------------------------------
// step_out
// ---------------------------------------------------------------------------------------------------

/// **`step_out` returns to the caller**, past the rest of the callee.
///
/// From [`PROF_LEAF`] — reached by the first `jsr` in [`PROF_MID`] — the frame's return address is the
/// instruction after that `jsr`, i.e. `PROF_MID + 4`. So a `step_out` must land there, having retired the
/// leaf's remaining `nop` and its `rts`.
///
/// Note what this rules out: landing at `PROF_LEAF + 2` (a plain step), and landing anywhere in `MAIN`
/// (unwinding one frame too many).
#[test]
fn step_out_returns_to_the_instruction_after_the_call() {
    let (_h, mut c) = two_level("st-out");
    park_at(&mut c, PROF_LEAF);
    c.ok("emulator/step_out", json!({}));
    assert_eq!(
        pc_of(&mut c),
        PROF_MID + 4,
        "step_out must return to PROF_MID's second call, exactly one frame up"
    );
}

/// **The `move.l addr,-(sp)` / `rts` dispatch idiom must not read as a return** — the W3 regression, in
/// `step_out`'s clothes.
///
/// [`PROF_DISPATCH`] pushes a target address and "returns" to it, landing at [`PROF_TARGET`] while leaving
/// the stack pointer exactly where it found it. A `step_out` that matched returns on a tolerant
/// `sp >= sp0` rule would fire on that `rts` and report `PROF_TARGET` — a confident, wrong answer that
/// looks entirely reasonable.
///
/// The honest answer is that `DISPATCH` has not returned: `TARGET` runs, calls the leaf, and *its* `rts`
/// unwinds the original return address. So `step_out` from `PROF_DISPATCH` must land back in the caller,
/// which is `MAIN` — above `PROF_TARGET` and nowhere near it.
#[test]
fn step_out_is_not_fooled_by_the_push_and_return_dispatch_idiom() {
    let (_h, mut c) = open("st-dispatch", ProfilerShape::Dispatch);
    park_at(&mut c, PROF_DISPATCH);
    c.ok("emulator/step_out", json!({}));
    let landed = pc_of(&mut c);
    assert_ne!(
        landed, PROF_TARGET,
        "the dispatch idiom's `rts` is a jump, not a return out of this frame — a tolerant \
         stack-depth rule stops here and is wrong"
    );
    // MAIN is the only caller of DISPATCH in this fixture, and it lives below every routine address, so
    // "returned to its caller" is exactly "landed before PROF_LEAF".
    assert!(
        landed < PROF_LEAF,
        "step_out must unwind to DISPATCH's real caller in MAIN, landed at {}",
        hex(landed)
    );
}

// ---------------------------------------------------------------------------------------------------
// The run-control state rule, and the params closure
// ---------------------------------------------------------------------------------------------------

/// **All three require a paused machine** — §6's run-control state rule reaches them through `step*`, and
/// a free-running machine MUST be refused with `-32005` and `data.reason = "machineRunning"`, never paused
/// implicitly to make the call succeed (§5, §8 item 12).
#[test]
fn all_three_refuse_a_free_running_machine_by_name() {
    let (_h, mut c) = two_level("st-running");
    c.ok("emulator/resume", json!({}));
    for m in STEP_METHODS {
        let e = c.err(m, json!({}));
        assert_eq!(
            e["code"],
            json!(-32005),
            "{m} must refuse while running: {e}"
        );
        assert_eq!(
            e["data"]["reason"],
            json!("machineRunning"),
            "{m}: `reason` is the discriminant clients branch on"
        );
    }
    // And the refusal really was a refusal: the machine is still running, not quietly paused by the
    // attempt. §5's ban is on resolving the wrong-state case implicitly, in either direction.
    assert_eq!(
        c.ok("emulator/status", json!({}))["running"],
        json!(true),
        "a refused step must not have changed the machine's mode"
    );
}

/// **The params closure reaches all three**, through the same dispatch choke as every other method
/// (§2.5 / §8 item 22): an undeclared key is `-32602`, the offending key is named, and the refusal
/// precedes the handler — so nothing stepped.
///
/// `tests/params_closure.rs` sweeps this over the whole table; it is repeated here on the three because
/// the *before the handler* half is specific to a method that would otherwise move the machine, and the
/// sweep cannot see it.
#[test]
fn an_undeclared_param_is_refused_before_anything_steps() {
    let (_h, mut c) = two_level("st-params");
    park_at(&mut c, PROF_LEAF);
    for m in STEP_METHODS {
        let e = c.err(m, json!({"notARealParamName": 1}));
        assert_eq!(e["code"], json!(-32602), "{m}: {e}");
        assert_eq!(
            e["data"]["unknownParams"],
            json!(["notARealParamName"]),
            "{m}"
        );
    }
    assert_eq!(
        pc_of(&mut c),
        PROF_LEAF,
        "a step refused for an unknown param must have stepped nothing"
    );
}

/// `count` is `emulator/step`'s only declared param, and it is declared on **no other** step row — the two
/// no-param rows must refuse it like any other unknown key rather than accepting it by family resemblance.
#[test]
fn count_is_not_a_step_family_param() {
    let (_h, mut c) = two_level("st-count-family");
    for m in ["emulator/step_over", "emulator/step_out"] {
        let e = c.err(m, json!({"count": 1}));
        assert_eq!(
            e["code"],
            json!(-32602),
            "{m} declares no params at all, so `count` is an unknown key: {e}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------
// The stopped event
// ---------------------------------------------------------------------------------------------------

/// **All three emit `emulator/stopped` with `reason: "step"`.**
///
/// §3 pins one stop reason across the three methods — *"`step` covers `step`, `step_over` and `step_out`
/// because those three share one stop condition"* — so neither of the two that return nothing gets a
/// reason of its own, and `reason` names the condition rather than the method that drove it.
///
/// **This assertion has no mechanical backstop**, exactly as `tests/events.rs` records for the mirror
/// case: `step` is a legal member of the schema's `reason` enum for *any* stop, so a `step_over` emitting
/// `runTo` — or emitting a reason of its own invention that happened to be in the enum — passes the
/// validator cleanly. D14 puts behaviour under the prose. Do not delete this on the grounds that the
/// events are schema-checked now; the schema cannot choose between two legal members.
#[test]
fn all_three_stop_for_the_reason_section_3_pins() {
    let (_h, mut c) = two_level("st-reason");
    for m in STEP_METHODS {
        park_at(&mut c, PROF_MID);
        let (_result, events) = call_watching_events(&mut c, m, json!({}));
        let stopped: Vec<&Value> = events
            .iter()
            .filter(|e| e["method"] == json!("emulator/stopped"))
            .collect();
        assert_eq!(
            stopped.len(),
            1,
            "{m} must emit exactly one stopped, saw {}: {events:?}",
            stopped.len()
        );
        let s = &stopped[0]["params"];
        assert_eq!(
            s["reason"],
            json!("step"),
            "{m}: §3 pins one reason across all three step methods"
        );
        // §3's params for a stop the caller can act on. `pc` is REQUIRED by the event fragment; the two
        // that return nothing depend on it entirely, which is audit D-03's cost made concrete.
        assert!(
            s["pc"].as_str().is_some_and(|p| p.starts_with("0x")),
            "{m}: the stopped event carries the pc: {s}"
        );
        assert_eq!(
            s["running"],
            json!(false),
            "{m}: a stopped event must not say the machine is running"
        );
    }
}

/// The two rows that return nothing return **exactly** nothing — `replyFields` and not one key more.
///
/// §6 writes `—` in both columns and the fragments carry no `properties` at all, so the whole reply is the
/// envelope: the machine stamp (§2.2) plus `droppedEvents` (§2.3). §8 item 20's closure already rejects a
/// surplus key on the wire; this pins the complement, that the four envelope fields are all present, so an
/// "empty" result cannot be an *absent* one.
#[test]
fn step_over_and_step_out_return_the_envelope_and_nothing_else() {
    let (_h, mut c) = two_level("st-empty");
    for m in ["emulator/step_over", "emulator/step_out"] {
        park_at(&mut c, PROF_LEAF);
        let r = c.ok(m, json!({}));
        let keys: Vec<&String> = r.as_object().expect("a result object").keys().collect();
        assert_eq!(
            keys,
            vec!["droppedEvents", "frame", "mclk", "running"],
            "{m}'s row has no result keys: the reply is the envelope alone"
        );
    }
}

// ---------------------------------------------------------------------------------------------------
// The symbol fields
// ---------------------------------------------------------------------------------------------------

/// **With no table loaded, `symbol` is ABSENT — never the address string.**
///
/// §4, quoted in the fragment: *"a server MUST NOT fall back to the address string"*. The failure this
/// pins is the friendly one — answering `"0x00000300"` in a field a client will pass back as a `symbol`
/// param and D7 promises will resolve. `symbolDisp` goes with it: a displacement from a symbol that was
/// never reported is a number about nothing.
#[test]
fn step_omits_the_symbol_fields_entirely_when_nothing_resolves() {
    let (_h, mut c) = two_level("st-nosym");
    let r = c.ok("emulator/step", json!({}));
    assert!(
        r.get("symbol").is_none(),
        "with no symbols loaded the field is absent, not an address string: {r}"
    );
    assert!(
        r.get("symbolDisp").is_none(),
        "and its displacement goes with it: {r}"
    );
    assert!(r.get("pc").is_some(), "`pc` is required regardless: {r}");
}

/// **With a table loaded, `symbol` is the bare label and `symbolDisp` is the number beside it.**
///
/// The listing names `Leaf` at [`PROF_LEAF`], so a step that lands at `PROF_LEAF + 2` must report
/// `symbol: "Leaf"` and `symbolDisp: 2` — never `"Leaf+$2"`, which is §4's displacement-inside-a-name
/// defect and the one `$defs/symbolName`'s pattern exists to reject.
///
/// The exact hit is asserted too: at `PROF_LEAF` itself the displacement is `0`, present rather than
/// omitted, because `0` is the answer and not the absence of one.
#[test]
fn step_reports_the_bare_label_and_the_displacement_beside_it() {
    let (_h, mut c) = two_level("st-sym");
    let lst = format!(
        "  Symbol Table (* = unused):\n\n Leaf : {:X} C |\n\n   1 symbols\n",
        PROF_LEAF
    );
    let dir = std::env::temp_dir().join(format!("oracle-st-sym-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("leaf.lst");
    std::fs::write(&path, lst).unwrap();
    c.ok(
        "emulator/load_symbols",
        json!({ "path": path.to_str().unwrap() }),
    );

    // Park one instruction short so the step lands ON the label: an exact hit, disp 0.
    park_at(&mut c, PROF_LEAF);
    let exact = c.ok("emulator/step", json!({"count": 0}));
    assert_eq!(exact["symbol"], json!("Leaf"), "{exact}");
    assert_eq!(
        exact["symbolDisp"],
        json!(0),
        "0 is an exact hit and is reported, not omitted: {exact}"
    );

    let displaced = c.ok("emulator/step", json!({"count": 1}));
    assert_eq!(
        displaced["symbol"],
        json!("Leaf"),
        "the BARE label — a `Leaf+$2` here would be §4's defect and the schema rejects it: {displaced}"
    );
    assert_eq!(
        displaced["symbolDisp"],
        json!(2),
        "the displacement is the number beside the name: {displaced}"
    );
    std::fs::remove_dir_all(&dir).ok();
}
