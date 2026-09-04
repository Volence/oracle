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

use common::{spawn_with, spawn_with_frame_budget, Client};
use oracle_core::testrom::{self, ProfilerShape, PROF_DISPATCH, PROF_LEAF, PROF_MID, PROF_TARGET};
use serde_json::{json, Value};

/// The three rows this file covers, so every sweep below is over the set rather than a sampled two.
const STEP_METHODS: [&str; 3] = ["emulator/step", "emulator/step_over", "emulator/step_out"];

/// The fixture the frame-budget rows run on. The same shape [`two_level`] uses, named separately because
/// those rows spawn their own servers (they need a budget [`two_level`] cannot set) and must be reading
/// the *identical* ROM for the replica comparison in
/// [`a_bounded_step_reports_the_instructions_it_actually_retired`] to mean anything.
const SHORTFALL_SHAPE: ProfilerShape = ProfilerShape::TwoLevel;

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

/// **An omitted `count` is one instruction, and since §11.24 the contract says so too.**
///
/// This used to be a *choice*: audit D-02 recorded that §6's row stated no default, no minimum above 0 and
/// no ceiling, where every sibling count in the catalog spells its bounds out, so two conformant servers
/// could disagree about `{}`. §11.24 (`empyrean` `0a4313e`, 2026-08-25) closed D-02 and wrote
/// `≥1 / default 1 / ≤1000000` into the fragment. The behaviour asserted here did not move; what moved is
/// that it is now **transcribed** rather than invented, and this row is the transcription's witness.
#[test]
fn an_omitted_count_is_one_instruction_which_is_now_the_fragments_own_default() {
    let (_h, mut c) = two_level("st-default");
    park_at(&mut c, PROF_LEAF);
    let r = c.ok("emulator/step", json!({}));
    assert_eq!(
        r["pc"],
        json!(hex(PROF_LEAF + 2)),
        "an omitted count steps once (§11.24 wrote `default 1` into the fragment)"
    );
    assert_eq!(
        r["stepped"],
        json!(1),
        "and the default is what `stepped` reports, not the absent param"
    );
}

/// **`count: 0` is REFUSED, not obeyed** — the fragment's floor is `1` and a value outside is never
/// clamped.
///
/// ⚑ **This row asserted the opposite until 2026-09-04, and it was right when it was written.** Until
/// §11.24 the fragment really did say `minimum: 0`, so this test's ancestor
/// (`count_zero_retires_nothing_and_moves_no_clock`) pinned a zero step as *"a real answer — a caller
/// establishing where the machine already is, without moving it"*. `0a4313e` replaced that floor with `1`
/// and the description with *"Zero was refused rather than clamped because the two servers disagreed on it
/// and a step of nothing is a status call spelt wrong"*; the re-vendor landed here and neither the handler
/// nor this test moved with it, for ten days, all three mutually consistent and all three wrong.
///
/// So the assertion is inverted, and the second half of the old one is kept and put to a new use: a
/// refusal must also leave the machine **untouched**, because a server that refused after running would
/// pass a code check alone.
#[test]
fn count_zero_is_refused_because_the_fragments_floor_is_one() {
    let (_h, mut c) = two_level("st-zero");
    park_at(&mut c, PROF_LEAF);
    let before = c.ok("emulator/status", json!({}));
    let e = c.err("emulator/step", json!({"count": 0}));
    assert_eq!(
        e["code"],
        json!(-32602),
        "count 0 is below the fragment's `minimum: 1` and must be refused, never clamped: {e}"
    );
    let after = c.ok("emulator/status", json!({}));
    assert_eq!(
        after["pc"],
        json!(hex(PROF_LEAF)),
        "a refused step must not have moved the pc"
    );
    assert_eq!(
        after["mclk"], before["mclk"],
        "a refused step must not have advanced the machine either"
    );
}

/// **`count` above the fragment's ceiling is REFUSED, not clamped** — the other end of the same bound.
///
/// `maximum: 1000000` arrived with §11.24 alongside the floor, and the fragment's description binds a
/// server to a refusal at *both* ends: *"a value outside is REFUSED with `-32602`, never clamped"*. This
/// server accepted `1000001` and ran it for ten days.
///
/// The ceiling is written out here rather than read from the vendored JSON on purpose. A test that parsed
/// the fragment for its expectation would agree with the fragment however the fragment changed, which is
/// the same shape as asserting a reply against itself — the value below is the contract's number, and a
/// re-vendor that moved it should make this row red and force a reading of the new text.
///
/// The largest **accepted** value is asserted in the same row as the anti-vacuity control: without it,
/// a handler that refused every `count` at all would pass the refusal half perfectly. That control runs on
/// a **one-frame** engine ([`common::spawn_with_frame_budget`]) because `1000000` instructions against the
/// default budget is 600 frames of emulation per assertion; the request is accepted and bounded, which is
/// exactly what a legal-but-large `count` is supposed to do.
#[test]
fn a_count_above_the_fragments_ceiling_is_refused_not_clamped() {
    let h = spawn_with_frame_budget(
        "st-ceiling",
        testrom::build_profiler(SHORTFALL_SHAPE),
        1024,
        1,
    );
    let mut c = Client::connect(&h);
    c.handshake(true);
    let before = c.ok("emulator/status", json!({}));
    for bad in [json!(1_000_001u64), json!(2_000_000u64), json!(u64::MAX)] {
        let e = c.err("emulator/step", json!({ "count": bad }));
        assert_eq!(
            e["code"],
            json!(-32602),
            "count {bad} is above the fragment's `maximum: 1000000` and must be refused: {e}"
        );
    }
    let after = c.ok("emulator/status", json!({}));
    assert_eq!(
        after["mclk"], before["mclk"],
        "a refused step must not have advanced the machine"
    );
    // The control: 1000000 is INSIDE the bound, so it must be accepted. A handler that refused
    // everything would satisfy the loop above and fail here.
    let r = c.ok("emulator/step", json!({"count": 1_000_000u64}));
    assert!(
        r.get("pc").is_some(),
        "the ceiling value itself is a legal request: {r}"
    );
}

/// `count` is refused when it is not a positive integer — the fragment types it
/// `integer, minimum 1, maximum 1000000`, and D9 makes a count a JSON number.
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
// `stepped` — §11.33 (CR-STEP-SHORTFALL)
// ---------------------------------------------------------------------------------------------------

/// **A completed step reports `stepped` equal to `count`, anchored on where the machine actually is.**
///
/// The expectation is derived from `testrom.rs`, not from the reply: [`PROF_LEAF`] is assembled as two
/// `nop`s (2 bytes each) followed by an `rts`, so from `PROF_LEAF` a two-instruction step must land at
/// `PROF_LEAF + 4`. That address is the witness `stepped` is checked against — a `stepped` that agreed
/// with the handler's own arithmetic and nothing else would be two spellings of one belief.
#[test]
fn a_completed_step_reports_stepped_equal_to_count() {
    let (_h, mut c) = two_level("st-stepped-exact");
    park_at(&mut c, PROF_LEAF);
    let r = c.ok("emulator/step", json!({"count": 2}));
    assert_eq!(
        r["pc"],
        json!(hex(PROF_LEAF + 4)),
        "two steps from PROF_LEAF retire both `nop`s (testrom.rs assembles them at +0 and +2)"
    );
    assert_eq!(
        r["stepped"],
        json!(2),
        "the run was not bounded, so `stepped` is the whole count: {r}"
    );
}

/// **THE row this parcel exists for: a step the frame budget cuts short says how far it actually got, and
/// the number is checked against the machine rather than against the server that reported it.**
///
/// §11.33: *"a server that bounds a step MUST emit `stepped` whenever it is less than `count`"*, and
/// *"`stepped` is never greater than `count`; the schema cannot express that across params and result, and
/// the gate does not check it, so the server must."* Neither half is visible to the conformance suite —
/// `stepped` is optional, so a server emitting nothing stays green, and a cross-field bound between a
/// `params` object and a `result` object has nowhere to live in a per-method fragment.
///
/// **Why the assertion is not "the reply agrees with the reply".** A row that checked `stepped` against
/// anything this handler computed would pass just as happily if both sides were stubbed, because they
/// would be one side. So the number is fed back to a **second, independently spawned machine** — same ROM,
/// same seed, same reset, but a frame budget large enough that its own step is *not* bounded — and that
/// machine is asked to retire exactly `stepped` instructions. If the first server told the truth, the
/// replica must arrive at the identical `pc` **and the identical `mclk`**, because the two executed the
/// same instruction stream from the same state. Nothing about that agreement is authored by the reply
/// under test: it is the emulated CPU's own answer to "run this many instructions".
///
/// `mclk` is asserted as well as `pc` because the fixture spins on the V counter, and a spin loop makes
/// `pc` alias — several different retire counts land on the same instruction. The clock does not alias.
///
/// **Anti-vacuity.** The run must genuinely fall short: `stepped < count` is asserted, so a build where
/// the budget never bound would fail here rather than pass silently. The one-frame budget is what makes
/// that reachable in milliseconds; at the engine's real 600-frame clamp the same event needs minutes.
#[test]
fn a_bounded_step_reports_the_instructions_it_actually_retired() {
    const ASKED: u64 = 1_000_000;

    // The server under test: one frame of budget, so `ASKED` cannot possibly retire inside it.
    let bounded = spawn_with_frame_budget(
        "st-short-bounded",
        testrom::build_profiler(SHORTFALL_SHAPE),
        1024,
        1,
    );
    let mut bc = Client::connect(&bounded);
    bc.handshake(true);
    let (short, events) = call_watching_events(&mut bc, "emulator/step", json!({ "count": ASKED }));

    let stepped = short["stepped"]
        .as_u64()
        .unwrap_or_else(|| panic!("a bounded step MUST carry `stepped` (§11.33): {short}"));
    assert!(
        stepped >= 1,
        "the fragment types `stepped` as `minimum: 1`: {short}"
    );
    assert!(
        stepped < ASKED,
        "ANTI-VACUITY: this row is only a shortfall test if the run really fell short. \
         stepped={stepped}, count={ASKED} — if these are equal the frame budget never bound and \
         the assertion below could not fail: {short}"
    );

    // The event channel's own account of the same fact, which is what §11.33 says a reply-only client
    // could never see. It must agree that the bound is what ended the run.
    let stopped = events
        .iter()
        .find(|e| e["method"] == json!("emulator/stopped"))
        .unwrap_or_else(|| panic!("a step emits `stopped`: {events:?}"));
    assert_eq!(
        stopped["params"]["deadlineReached"],
        json!(true),
        "the run ended on its frame bound, which is the condition `stepped < count` reports: {stopped}"
    );

    // The independent witness: a second machine, told to retire exactly the number the first reported,
    // on a budget that will not bound it. Where it lands is the CPU's answer, not the reply's.
    let replica = spawn_with_frame_budget(
        "st-short-replica",
        testrom::build_profiler(SHORTFALL_SHAPE),
        1024,
        600,
    );
    let mut rc = Client::connect(&replica);
    rc.handshake(true);
    let echoed = rc.ok("emulator/step", json!({ "count": stepped }));
    assert_eq!(
        echoed["stepped"],
        json!(stepped),
        "the replica's own step was not bounded, so it retired the whole count: {echoed}"
    );
    assert_eq!(
        echoed["pc"], short["pc"],
        "retiring exactly `stepped` instructions from the same reset state must reach the same \
         instruction the bounded run stopped at — a fabricated `stepped` lands somewhere else"
    );
    assert_eq!(
        echoed["mclk"], short["mclk"],
        "...and at the same master clock, which is the half a spin loop cannot alias"
    );
}

/// **`stepped` is `emulator/step`'s key alone.** §11.33: *"`step_over` and `step_out` take no count and
/// stop on a condition, so a shortfall there is the condition not firing"* — the two rows are explicitly
/// **not amended**, and their fragments do not declare the key.
///
/// The trap this guards is a one-line one: all three rows return through `Engine::halt_result`, so setting
/// `stepped` there instead of in `step` would put it on all three. The schema would catch it — both
/// fragments close `result` with `unevaluatedProperties: false` — but only as an opaque validation failure
/// somewhere in the file, so the intent is asserted here in its own words.
#[test]
fn stepped_is_not_a_step_family_result_key() {
    let (_h, mut c) = two_level("st-stepped-family");
    park_at(&mut c, PROF_MID);
    for m in ["emulator/step_over", "emulator/step_out"] {
        let r = c.ok(m, json!({}));
        assert!(
            r.get("stepped").is_none(),
            "{m} is NOT amended by §11.33 and its fragment declares no `stepped`: {r}"
        );
    }
    // The control: the row that IS amended does carry it, so this test cannot pass by the server having
    // dropped the key everywhere.
    let r = c.ok("emulator/step", json!({}));
    assert!(
        r.get("stepped").is_some(),
        "`emulator/step` is the row §11.33 amends and must carry the key: {r}"
    );
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

/// **The two rows that returned nothing now return the halt** — `pc` on top of the envelope, and nothing
/// else (§11.24, closing audit D-03).
///
/// This test used to assert the opposite, and the flip is the amendment. §6 wrote `—` in both columns for
/// these two while `emulator/step` returned `pc`; §11.24 ruled the asymmetry away, on the ground that the
/// three share **one stop condition** and both servers already computed the PC and discarded it. So the
/// key set is the envelope (the machine stamp, §2.2, plus `droppedEvents`, §2.3) **plus `pc`**.
///
/// §8 item 20's closure already rejects a surplus key on the wire and the fragment's `required: ["pc"]`
/// rejects its absence; what this pins is the complement neither can state — that the envelope fields are
/// all still there, so a result carrying `pc` cannot have quietly lost the stamp.
///
/// No symbol pair here on purpose: `two_level` loads no listing, and the symbol fields have their own
/// tests below. That is also why the expected key set is exact rather than a subset.
#[test]
fn step_over_and_step_out_return_the_halt_pc_and_the_envelope() {
    let (_h, mut c) = two_level("st-empty");
    for m in ["emulator/step_over", "emulator/step_out"] {
        park_at(&mut c, PROF_LEAF);
        let r = c.ok(m, json!({}));
        let keys: Vec<&String> = r.as_object().expect("a result object").keys().collect();
        assert_eq!(
            keys,
            vec!["droppedEvents", "frame", "mclk", "pc", "running"],
            "{m}'s row returns emulator/step's result since §11.24: pc, plus the envelope"
        );
        // Derived from the machine rather than pinned: the reported PC must be the one the machine
        // actually halted at, which is the whole content of the amendment. A constant here would pass
        // against a handler that returned a plausible-looking address it never read.
        let regs = c.ok("emulator/registers", json!({}));
        assert_eq!(
            r["pc"], regs["pc"],
            "{m} must report the PC the machine stopped at, in the same hex spelling the rest of the \
             bus uses (D9 category 1)"
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
///
/// **The exact hit used to be spelled `count: 0`** — a step of nothing, read for its symbol fields — which
/// is precisely what the fragment calls *"a status call spelt wrong"* and what §11.24 refused. It is now
/// reached by stepping the `jsr` at [`PROF_MID`], which lands *on* the label. Same two assertions, one
/// fewer superseded request.
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

    // Park one instruction short so the step lands ON the label: an exact hit, disp 0. `PROF_MID + 0` is
    // `jsr (LEAF).w`, so one step from there enters the callee at `PROF_LEAF` exactly.
    park_at(&mut c, PROF_MID);
    let exact = c.ok("emulator/step", json!({"count": 1}));
    assert_eq!(exact["pc"], json!(hex(PROF_LEAF)), "setup: {exact}");
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
