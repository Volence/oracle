//! **`emulator/run_to_scanline`** — `protocol.md` §6 line 855, served 2026-08-22.
//!
//! The fourth of the 21 fragments that describe methods this server did not serve, and which the
//! 2026-08-22 adoption turned into its acceptance contract. Its fragment landed upstream final, so this is
//! conformance work with no contract change behind it.
//!
//! # What the schema checks for free, and what it cannot
//!
//! Every line a [`Client`] receives is validated against the vendored fragment, closed with
//! `unevaluatedProperties: false` (`common::schema`, §8 item 15 / item 20). So a `pc` key smuggled onto the
//! result, a `line` outside 0-511 echoed back, a missing `reached`, or a `stopped` carrying a `reason`
//! outside the enum all fail without any assertion in this file.
//!
//! What that leaves is everything the schema is structurally blind to, and on this method that is nearly
//! all of it — because **`reached: true` beside a `line` echo is a shape the validator accepts no matter
//! where the machine actually stopped**. A handler that ran one frame and claimed success would pass every
//! structural check in the harness. That believable wrong answer is what this file exists to make
//! impossible, and it is why every positive test below reads the machine's own master clock back and
//! recomputes the line from it rather than trusting the echo.
//!
//! # Where the expectations come from
//!
//! * The line at any instant is `(mclk % MCLK_PER_FRAME) / MCLK_PER_LINE` — the VDP's own two constants
//!   (`oracle_core::vdp`), the same arithmetic `System::deliver_event` uses to label its `Scanline` events,
//!   and `mclk` is the stamp D11 puts on every reply. Nothing here is a number this server was observed to
//!   produce.
//! * The 0-511 bound is **parsed out of the vendored fragment**, never typed, so a contract that moves it
//!   moves this file's expectation with it.
//! * `LINES_PER_FRAME` (262) is read from `oracle_core::vdp`, so the unreachable-target tests below stop
//!   being about "300" and start being about "past the end of the frame", which is what they mean.

mod common;

use common::schema::schema_root;
use common::{spawn, Client};
use oracle_core::vdp::{LINES_PER_FRAME, MCLK_PER_FRAME, MCLK_PER_LINE};
use serde_json::{json, Value};

/// The line the raster is on, recomputed from the reply's own stamp.
///
/// This is the measurement the whole file turns on: the server's `line` echo is its *input*, not its
/// answer, and only the clock says where it really stopped.
fn line_of(reply: &Value) -> u64 {
    let mclk = reply["mclk"]
        .as_u64()
        .expect("every reply carries the machine stamp (D11)");
    (mclk % MCLK_PER_FRAME) / MCLK_PER_LINE
}

fn mclk_of(reply: &Value) -> u64 {
    reply["mclk"].as_u64().expect("every reply carries `mclk`")
}

/// The `line` bound the contract states, parsed from the vendored fragment rather than transcribed.
fn contract_line_bound() -> (u64, u64) {
    let p = &schema_root()["methods"]["emulator/run_to_scanline"]["params"]["properties"]["line"];
    (
        p["minimum"]
            .as_u64()
            .expect("the fragment states a minimum"),
        p["maximum"]
            .as_u64()
            .expect("the fragment states a maximum"),
    )
}

fn connect(tag: &str) -> (oracle_aether::server::ServerHandle, Client) {
    let h = spawn(tag);
    let mut c = Client::connect(&h);
    c.handshake(true);
    (h, c)
}

/// Call `method`, collecting every notification the server pushes before the reply.
fn call_watching_events(c: &mut Client, method: &str, params: Value) -> (Value, Vec<Value>) {
    let id = 8800 + method.len() as i64;
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
// It stops where it was asked to, and the answer is measured rather than echoed
// ---------------------------------------------------------------------------------------------------

/// **The named failure mode: `reached: true` from a run that stopped somewhere else.**
///
/// Nothing in the schema can catch it — `{line: 100, reached: true, maxFrames: 600}` is a conformant reply
/// whatever the machine did — so the assertion is on the clock. `mclk` is recomputed into a line by the
/// VDP's own constants and must equal the target *exactly*: this core stops at the first instruction
/// boundary at or after the line's start, and the fixture's instructions are tens of mclk against a
/// 3420-mclk line, so an off-by-one line is a real defect and not jitter.
///
/// Swept over four targets that are four different things — the first line of the frame, an active line, the
/// first line of vblank (`ACTIVE_LINES`), and a blanking line no `on_scanline` can ever deliver — because a
/// handler built on the rendered-row hook would pass on the two active ones and fail on the other two.
#[test]
fn it_stops_on_the_line_it_was_asked_for_and_the_clock_says_so() {
    let (_h, mut c) = connect("rts-lands");
    for target in [0_u64, 100, 224, 261] {
        let r = c.ok("emulator/run_to_scanline", json!({ "line": target }));
        assert_eq!(
            r["reached"],
            json!(true),
            "line {target} occurs every frame and must be reachable inside the default bound: {r}"
        );
        assert_eq!(r["line"], json!(target), "the target is echoed: {r}");
        assert_eq!(
            line_of(&r),
            target,
            "run_to_scanline({target}) reported reached but the clock says the raster is on line {}: {r}",
            line_of(&r)
        );
    }
}

/// **Blanking is reachable, and that is the half the pixel hook cannot serve.**
///
/// `on_scanline` fires only for `line < 224` and lags a line; `on_line_start` is the hook this method is
/// built on precisely so a target in vblank works. Lines 224-261 are exactly those, and `emulator/scanlines`
/// — the readback method — bounds itself at 0-223 for the same reason. Separated from the sweep above so a
/// regression that lost blanking names itself.
#[test]
fn a_target_in_vertical_blanking_is_reached() {
    let (_h, mut c) = connect("rts-vblank");
    let last = LINES_PER_FRAME - 1;
    for target in [224_u64, 240, last] {
        let r = c.ok("emulator/run_to_scanline", json!({ "line": target }));
        assert_eq!(
            r["reached"],
            json!(true),
            "line {target} is in vblank and must be reachable: {r}"
        );
        assert_eq!(
            line_of(&r),
            target,
            "stopped on the wrong blanking line: {r}"
        );
    }
}

/// **The condition is the NEXT start of the line, not "the raster is already there".**
///
/// The level-versus-edge trap: a handler that fires when the current line already matches would return
/// `reached: true` twice while advancing the machine only once, and a caller stepping frame by frame on one
/// raster coordinate — the obvious use — would be wedged forever on the second call.
///
/// The expectation is derived, not observed: two consecutive stops on the same line are one frame apart, so
/// the second `mclk` minus the first must be `MCLK_PER_FRAME` give or take the instruction the stop lands
/// after. The bound on that slack is one line, which is 3420 mclk and far larger than any instruction this
/// fixture executes — so it cannot absorb a whole-frame error in either direction.
#[test]
fn calling_it_twice_on_one_line_advances_a_frame_each_time() {
    let (_h, mut c) = connect("rts-edge");
    let first = c.ok("emulator/run_to_scanline", json!({ "line": 120 }));
    let second = c.ok("emulator/run_to_scanline", json!({ "line": 120 }));
    assert_eq!(
        second["reached"],
        json!(true),
        "the second call must fire too: {second}"
    );
    assert_eq!(
        line_of(&second),
        120,
        "the second call must land on 120 as well: {second}"
    );
    let delta = mclk_of(&second) - mclk_of(&first);
    assert!(
        delta.abs_diff(MCLK_PER_FRAME) < MCLK_PER_LINE,
        "two stops on the same line are one frame ({MCLK_PER_FRAME} mclk) apart; this pair is {delta} \
         apart, so one of them did not advance"
    );
}

/// **The machine really runs**, rather than the handler computing where the raster would have been.
///
/// The clock must move by at least the distance to the target, and the `stopped` event's `pc` must be a
/// legal instruction address. A handler that only did arithmetic would satisfy every echo test above.
#[test]
fn the_run_advances_the_machine() {
    let (_h, mut c) = connect("rts-advances");
    let before = c.ok("emulator/status", json!({}));
    let r = c.ok("emulator/run_to_scanline", json!({ "line": 200 }));
    let moved = mclk_of(&r) - mclk_of(&before);
    assert!(
        moved >= MCLK_PER_LINE,
        "the raster cannot reach a new line without the clock moving at least one line: {moved} mclk"
    );
}

// ---------------------------------------------------------------------------------------------------
// The bound, and what `reached: false` owes the caller (D12)
// ---------------------------------------------------------------------------------------------------

/// **`reached: false` on a reachable line the bound did not reach, and the caveat D12 asks for in words.**
///
/// Constructed rather than waited for. `run_frames`-shaped advances are anchored on the last whole frame
/// boundary crossed, so a one-frame run from line 200 covers lines 201-261 and no more: line 100 is behind
/// the raster and cannot come round again inside `maxFrames: 1`. Raising the bound to 2 reaches it, and that
/// pair is the test — one number changes, the answer flips, so neither outcome can be an accident of the
/// fixture.
///
/// The caveat is not merely asserted present: it must contain the word `NOTHING`, which is the specific
/// discharge D12 names ("nothing about the machine's state follows from where it stopped"), and it must
/// **not** claim the line is impossible, because line 100 is perfectly possible and a caller told otherwise
/// would stop raising the bound.
#[test]
fn a_reachable_line_the_bound_missed_answers_false_with_a_caveat() {
    let (_h, mut c) = connect("rts-deadline");
    let park = c.ok("emulator/run_to_scanline", json!({ "line": 200 }));
    assert_eq!(
        line_of(&park),
        200,
        "setup: never parked on line 200: {park}"
    );

    let missed = c.ok(
        "emulator/run_to_scanline",
        json!({ "line": 100, "maxFrames": 1 }),
    );
    assert_eq!(
        missed["reached"],
        json!(false),
        "line 100 is behind the raster and one frame's advance ends at the frame boundary: {missed}"
    );
    assert_eq!(
        missed["maxFrames"],
        json!(1),
        "the bound actually applied is echoed: {missed}"
    );
    let caveat = missed["caveat"].as_str().unwrap_or_default();
    assert!(
        caveat.contains("NOTHING"),
        "D12: a reached:false reply SHOULD say in words that nothing about the machine's state follows \
         from where it stopped. Caveat was {caveat:?}"
    );
    assert!(
        !caveat.contains("cannot occur in this video mode"),
        "line 100 is reachable — telling the caller it cannot occur sends them to stop raising maxFrames. \
         Caveat was {caveat:?}"
    );
}

/// The other half of the pair above: the same request with one more frame of budget fires.
#[test]
fn the_same_line_one_frame_later_is_reached() {
    let (_h, mut c) = connect("rts-deadline-2");
    let park = c.ok("emulator/run_to_scanline", json!({ "line": 200 }));
    assert_eq!(
        line_of(&park),
        200,
        "setup: never parked on line 200: {park}"
    );
    let hit = c.ok(
        "emulator/run_to_scanline",
        json!({ "line": 100, "maxFrames": 2 }),
    );
    assert_eq!(
        hit["reached"],
        json!(true),
        "two frames must cover line 100: {hit}"
    );
    assert_eq!(line_of(&hit), 100, "and it must actually be on 100: {hit}");
    assert!(
        hit.get("caveat").is_none(),
        "a caveat that is always present is documentation wearing signal's clothes (§2.4's advisory): {hit}"
    );
}

/// **A line the contract allows and this video mode cannot produce.**
///
/// `LINES_PER_FRAME` is 262 and the fragment's `maximum` is 511, so 262-511 are legal requests that can
/// never fire. Served rather than refused — narrowing a span the contract states would make one conformant
/// server refuse what another accepts — and the whole obligation lands on the caveat, which is why `caveat`
/// is declared on this row at all.
///
/// The target is derived from `LINES_PER_FRAME`, not typed: `LINES_PER_FRAME` itself is the first
/// unreachable line by construction, and the fragment's own maximum is the last.
#[test]
fn a_line_this_video_mode_cannot_produce_answers_false_and_says_why() {
    let (_h, mut c) = connect("rts-unreachable");
    let (_min, max) = contract_line_bound();
    for target in [LINES_PER_FRAME, LINES_PER_FRAME + 38, max] {
        let r = c.ok(
            "emulator/run_to_scanline",
            json!({ "line": target, "maxFrames": 1 }),
        );
        assert_eq!(
            r["reached"],
            json!(false),
            "line {target} is past the end of a {LINES_PER_FRAME}-line frame and cannot fire: {r}"
        );
        assert_eq!(r["line"], json!(target), "the target is still echoed: {r}");
        let caveat = r["caveat"].as_str().unwrap_or_default();
        assert!(
            caveat.contains("cannot occur in this video mode"),
            "an unreachable line owes the caller the reason, not a bare deadline — a caller told only that \
             the bound ran out will raise maxFrames forever. Caveat was {caveat:?}"
        );
        // The *last reachable* line, not the height: 262 would be satisfied by the caveat merely echoing
        // the target on the first sweep value, which is a matcher that proves nothing. 261 appears in no
        // target here, so only a caveat that really states the reachable range can contain it.
        assert!(
            caveat.contains(&(LINES_PER_FRAME - 1).to_string()),
            "the caveat must name the last reachable line so the caller can pick a target that works. \
             Caveat was {caveat:?}"
        );
        assert!(
            caveat.contains("NOTHING"),
            "D12's discharge is owed here too. Caveat was {caveat:?}"
        );
    }
}

/// **The bound is echoed, and the default is the contract's 600.**
///
/// D12 fixes the default; the result's `maxFrames` is "the bound actually applied", so an omitted param and
/// a stated one must be distinguishable from the reply alone.
#[test]
fn the_bound_actually_applied_is_reported() {
    let (_h, mut c) = connect("rts-bound");
    let default = c.ok("emulator/run_to_scanline", json!({ "line": 10 }));
    assert_eq!(
        default["maxFrames"],
        json!(600),
        "D12's default is 600 and the reply reports the bound actually applied: {default}"
    );
    let stated = c.ok(
        "emulator/run_to_scanline",
        json!({ "line": 10, "maxFrames": 3 }),
    );
    assert_eq!(
        stated["maxFrames"],
        json!(3),
        "a stated bound is echoed as given: {stated}"
    );
}

// ---------------------------------------------------------------------------------------------------
// The wire shape the fragment pins, including the defect it pins
// ---------------------------------------------------------------------------------------------------

/// **D-04, transcribed rather than repaired: no `pc` on the result.**
///
/// `run_to`'s result carries `pc`, `symbol` and `symbolDisp`; this row's carries none of them, so a caller
/// that ran to a scanline cannot learn where the 68000 stopped without a second call. The asymmetry is
/// registered upstream as a defect and this server reports it. Pinned here because a well-meaning later
/// edit that "fixed" it would put a key on the wire no contract text describes — CR-13's whole subject.
#[test]
fn the_result_carries_no_pc_which_is_the_registered_defect_not_an_oversight() {
    let (_h, mut c) = connect("rts-d04");
    let r = c.ok("emulator/run_to_scanline", json!({ "line": 50 }));
    for absent in ["pc", "symbol", "symbolDisp", "target", "frames"] {
        assert!(
            r.get(absent).is_none(),
            "`{absent}` is not on this row (D-04 records the `pc` half as a defect to raise, not to patch \
             locally): {r}"
        );
    }
    let keys: Vec<&str> = ["line", "reached", "maxFrames"]
        .into_iter()
        .filter(|k| r.get(*k).is_some())
        .collect();
    assert_eq!(
        keys,
        ["line", "reached", "maxFrames"],
        "the three required keys are all present: {r}"
    );
}

/// **The stop is announced as its own condition.**
///
/// §3's enum has had `runToScanline` since before the method existed, and `reason` names the stop
/// *condition* rather than the method — so `runTo` here would be the knowing mislabel §8 item 13 names.
/// `deadlineReached` is `reached`'s complement for stream consumers and §6 says the two are never both true;
/// both directions are checked, because a handler that hard-coded either would pass with only one.
#[test]
fn the_stopped_event_names_the_condition_and_complements_reached() {
    let (_h, mut c) = connect("rts-events");
    let (hit, hit_events) =
        call_watching_events(&mut c, "emulator/run_to_scanline", json!({"line": 30}));
    let stopped: Vec<&Value> = hit_events
        .iter()
        .filter(|e| e["method"] == json!("emulator/stopped"))
        .collect();
    assert_eq!(
        stopped.len(),
        1,
        "exactly one stopped per run: {hit_events:?}"
    );
    assert_eq!(
        stopped[0]["params"]["reason"],
        json!("runToScanline"),
        "the condition, not the method and not the nearest-looking value"
    );
    assert_eq!(hit["reached"], json!(true));
    assert_eq!(
        stopped[0]["params"]["deadlineReached"],
        json!(false),
        "§6: reached and deadlineReached are never both true, and never both false on a run that ended"
    );

    let (missed, missed_events) = call_watching_events(
        &mut c,
        "emulator/run_to_scanline",
        json!({"line": LINES_PER_FRAME, "maxFrames": 1}),
    );
    assert_eq!(missed["reached"], json!(false));
    let stopped: Vec<&Value> = missed_events
        .iter()
        .filter(|e| e["method"] == json!("emulator/stopped"))
        .collect();
    assert_eq!(
        stopped[0]["params"]["deadlineReached"],
        json!(true),
        "the run ended on its bound, and the stream has to be able to see that"
    );
}

// ---------------------------------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------------------------------

/// **§6's run-control state rule: a free-running machine is `-32005 machineRunning`, never an implicit
/// pause.**
#[test]
fn it_refuses_a_free_running_machine_by_reason_not_by_message() {
    let (_h, mut c) = connect("rts-running");
    c.ok("emulator/resume", json!({}));
    let e = c.err("emulator/run_to_scanline", json!({ "line": 10 }));
    assert_eq!(
        e["code"],
        json!(-32005),
        "the request is wrong right now: {e}"
    );
    assert_eq!(
        e["data"]["reason"],
        json!("machineRunning"),
        "`reason` is the discriminant clients branch on: {e}"
    );
}

/// **The 0-511 bound is the fragment's, and out of range is refused rather than clipped.**
///
/// The bound is parsed from the vendored fragment, so a contract amendment moves this test with it. Both
/// edges are checked in both directions: the last legal value is accepted (a bound off by one downward is
/// silent otherwise) and the first illegal one refused.
#[test]
fn the_line_bound_is_the_contracts_and_out_of_range_is_refused() {
    let (_h, mut c) = connect("rts-bounds");
    let (min, max) = contract_line_bound();
    let ok = c.ok(
        "emulator/run_to_scanline",
        json!({ "line": max, "maxFrames": 1 }),
    );
    assert_eq!(
        ok["line"],
        json!(max),
        "the fragment's maximum is a legal request: {ok}"
    );
    let lo = c.ok("emulator/run_to_scanline", json!({ "line": min }));
    assert_eq!(
        lo["reached"],
        json!(true),
        "the fragment's minimum is a legal request: {lo}"
    );

    let e = c.err("emulator/run_to_scanline", json!({ "line": max + 1 }));
    assert_eq!(e["code"], json!(-32602), "past the contract's bound: {e}");
    let msg = e["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains(&max.to_string()) && msg.contains(&(max + 1).to_string()),
        "the refusal names the bound and the value refused, so the caller need not guess: {msg:?}"
    );
}

/// **`line` is required, and it is a JSON integer (D9 category 2), never a hex string.**
///
/// The hex-string case is the one worth pinning: every *address* on this bus is a `0x…` string, so a client
/// generalising from `run_to` will reach for one here — and a server that accepted it would be inventing a
/// spelling the fragment does not carry.
#[test]
fn line_is_required_and_is_a_number() {
    let (_h, mut c) = connect("rts-line-type");
    let missing = c.err("emulator/run_to_scanline", json!({}));
    assert_eq!(missing["code"], json!(-32602));
    assert!(
        missing["message"]
            .as_str()
            .unwrap_or_default()
            .contains("`line`"),
        "the refusal names the missing param: {missing}"
    );
    for bad in [json!("0x64"), json!(-1), json!(1.5), json!(true)] {
        let e = c.err("emulator/run_to_scanline", json!({ "line": bad }));
        assert_eq!(
            e["code"],
            json!(-32602),
            "`line` = {bad} must be refused: {e}"
        );
    }
}

/// **The legacy `max_frames` spelling is refused by name, not silently ignored.**
///
/// D-33 measured four camelCase/snake_case conflicts between the legacy server and the contract, and
/// `maxFrames` vs `max_frames` is one of them — landing on this method and on `wait_for_break`. §2.5's params
/// closure is what makes the migration safe: a client that sends the old spelling is told, rather than
/// silently given the 600-frame default and a different answer.
#[test]
fn the_legacy_snake_case_bound_is_named_in_the_refusal() {
    let (_h, mut c) = connect("rts-legacy");
    let e = c.err(
        "emulator/run_to_scanline",
        json!({ "line": 10, "max_frames": 2 }),
    );
    assert_eq!(e["code"], json!(-32602));
    assert_eq!(
        e["data"]["unknownParams"],
        json!(["max_frames"]),
        "the offending key is machine-readable, not buried in prose: {e}"
    );
    let msg = e["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("maxFrames"),
        "and the accepted spelling is in the message, which is the whole migration: {msg:?}"
    );
}
