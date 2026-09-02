//! **`stopPrecision` (`protocol.md` §2.1, §3, §6, §11.31) and its §8 item 24 proof, over the real wire.**
//!
//! Item 24 exists because §2.1's binding rule relates **two messages on one connection** — a declaration
//! in `initialize` and a value on `emulator/stopped` — and no schema fragment can hold that. *"A
//! declaration checked by nothing is the defect this key fixes, moved one level up."*
//!
//! ## The order these tests are in is the point
//!
//! Part A **measures**. It arms a stop at a known instruction in
//! [`oracle_core::testrom::build_stop_precision`] and asks the machine what actually happened, with no
//! reference to anything the server declares. §10.2 of CR-E and item 24 both name the alternative as this
//! amendment's own failure mode: *"a doc comment in a core saying it stops at instruction boundaries is a
//! claim about source text, and an `\"exact\"` declared on the strength of one is this amendment's own
//! failure mode committed by its implementer."* This server's `attribute()` carries exactly such a
//! comment (*"it halts at an instruction boundary before the instruction runs"*). Part A is why the
//! declaration does not rest on it.
//!
//! Part B is item 24 itself: it reads the declaration out of `initialize` and closes it against what
//! Part A's instrument sees.
//!
//! ## The instrument
//!
//! `build_stop_precision` loops over seven instruction boundaries with interrupts masked. Two of them are
//! probes:
//!
//! * [`SP_PROBE`] — `addq.w #1, D0`, whose **only** observable effect takes `D0.w` from `0` to `1`. A stop
//!   armed here reports `pc == SP_PROBE` with `D0.w == 0` if it is `exact`, and `pc == SP_PROBE + 2` with
//!   `D0.w == 1` if it is `afterCommit`. That is item 24's "single observable register effect".
//! * [`SP_STORE`] — `move.w D1, ($00FF8000).L`, the access an *access-armed* stop triggers on, whose
//!   commit is observable in memory: `mem16 == D1.w - 1` before it, `mem16 == D1.w` after.
//!
//! Every other boundary is covered by [`SP_BOUNDARIES`], whose rows are proven against the real CPU by a
//! unit test in `oracle-core` — so nothing here assumes the table, it inherits it.

#![cfg(unix)]

mod common;

use common::{spawn_with, Client};
use oracle_aether::engine::{StopPrecision, StopReason};
use oracle_aether::server::ServerHandle;
use oracle_core::testrom::{
    build_stop_precision, SP_BOUNDARIES, SP_BRANCH, SP_LOOP, SP_PROBE, SP_PROBE_POST_D0,
    SP_PROBE_PRE_D0, SP_STORE, SP_STORE_ADDR,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The length of `addq.w #1, D0` in bytes — where an `afterCommit` stop armed at [`SP_PROBE`] would
/// report its `pc`. Derived from the two constants that bracket it, never written down as a number.
const PROBE_LEN: u32 = 0x0000_0208 - SP_PROBE;

fn armed(tag: &str) -> (ServerHandle, Client) {
    let (h, c, _) = armed_with_handshake(tag);
    (h, c)
}

/// [`armed`], keeping the `initialize` result — the message §2.1's binding rule relates the events to.
fn armed_with_handshake(tag: &str) -> (ServerHandle, Client, Value) {
    let h = spawn_with(tag, build_stop_precision(), 1024);
    let mut c = Client::connect(&h);
    let hs = c.handshake(true);
    (h, c, hs)
}

fn hex_u32(v: &Value) -> u32 {
    let s = v
        .as_str()
        .unwrap_or_else(|| panic!("a hex string, got {v}"));
    u32::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16)
        .unwrap_or_else(|e| panic!("parsing {s}: {e}"))
}

/// The machine state every assertion below is written in terms of: where the PC says we are, the probe
/// register, the tick counter, and what the store published.
#[derive(Debug, Clone, Copy)]
struct Snap {
    pc: u32,
    d0: u16,
    d1: u16,
    mem: u16,
}

impl Snap {
    /// `mem16[SP_STORE_ADDR] - D1.w`, the store's commit discriminator: `0` once the store has committed
    /// this pass, `-1` while the tick has run and the store has not.
    fn store_delta(&self) -> i32 {
        self.mem.wrapping_sub(self.d1) as i16 as i32
    }
}

fn snapshot(c: &mut Client) -> Snap {
    let r = c.ok("emulator/registers", json!({}));
    let m = c.ok(
        "emulator/read_memory",
        json!({"addr": format!("0x{SP_STORE_ADDR:08X}"), "len": 2}),
    );
    let bytes = m["bytes"].as_str().expect("bytes");
    let raw = u32::from_str_radix(bytes.trim_start_matches("0x"), 16).expect("two bytes of hex");
    Snap {
        pc: hex_u32(&r["pc"]),
        d0: hex_u32(&r["d0"]) as u16,
        d1: hex_u32(&r["d1"]) as u16,
        mem: raw as u16,
    }
}

/// Send `method`, read every line until its reply, and return `(the last emulator/stopped params, the
/// reply's result)`. Written by hand rather than through [`Client::ok`] because `ok` skips events, and
/// the event is half the subject here.
fn call_capturing_stop(c: &mut Client, id: i64, method: &str, params: Value) -> (Value, Value) {
    c.send_raw(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string());
    let mut stopped = None;
    loop {
        let line = c.recv();
        if line["method"] == json!("emulator/stopped") {
            stopped = Some(line["params"].clone());
        }
        if line["id"] == json!(id) {
            assert!(
                line.get("error").is_none(),
                "{method} failed: {}",
                line["error"]
            );
            return (
                stopped.unwrap_or_else(|| panic!("{method} emitted no emulator/stopped")),
                line["result"].clone(),
            );
        }
    }
}

/// **`resume`, then the halt it runs into — read as one operation, because the two race.**
///
/// A breakpoint on this fixture's seven-instruction loop fires within microseconds of the resume, and the
/// `stopped` event is broadcast from the engine thread while the `resume` reply is written by the
/// connection thread. Either can reach the socket first. [`Client::ok`] reads through to the reply and
/// **discards** the events it passes, so the obvious spelling — `ok("emulator/resume")` then
/// `next_stopped` — throws the halt away roughly half the time and then blocks forever waiting for it.
/// Measured here at trial 4 of 8, after three clean passes; a single-shot test would have called it green.
///
/// So both lines are read before either is acted on. This is a property of the *harness*, not of the
/// server: the wire carried both messages in a legitimate order.
fn resume_and_wait_for_stop(c: &mut Client, id: i64) -> Value {
    c.send_raw(
        &json!({"jsonrpc":"2.0","id":id,"method":"emulator/resume","params":{}}).to_string(),
    );
    let mut stopped = None;
    let mut replied = false;
    loop {
        let line = c.recv();
        if line["method"] == json!("emulator/stopped") {
            stopped = Some(line["params"].clone());
        }
        if line["id"] == json!(id) {
            assert!(
                line.get("error").is_none(),
                "resume failed: {}",
                line["error"]
            );
            replied = true;
        }
        if replied {
            if let Some(p) = stopped.take() {
                return p;
            }
        }
    }
}

/// Put the machine in the fixture's steady state — past `main`, several passes into the loop — so `D0`,
/// `D1` and memory hold the values [`SP_BOUNDARIES`] describes rather than their power-on ones.
fn settle(c: &mut Client) {
    c.ok("emulator/run_frames", json!({"frames": 2}));
}

/// **The verdict a measurement yields.** Spelled as the contract's own three values so a measurement can
/// be compared directly against a declaration, plus the case where the machine's own state contradicts
/// the PC it reported — which is not a precision at all, it is a broken stop.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Measured {
    Exact,
    AfterCommit,
    /// The reported PC and the machine state are consistent with each other and with an instruction
    /// boundary, but the stop had no triggering address to be exact *about*.
    BoundaryConsistent,
}

/// Classify a stop that was armed at [`SP_PROBE`]: the single-observable-register-effect measurement item
/// 24 asks for, in one place so both parts of this file use the same instrument.
fn classify_probe_stop(s: Snap) -> Measured {
    if s.pc == SP_PROBE && s.d0 == SP_PROBE_PRE_D0 {
        Measured::Exact
    } else if s.pc == SP_PROBE + PROBE_LEN && s.d0 == SP_PROBE_POST_D0 {
        Measured::AfterCommit
    } else {
        panic!(
            "a stop armed at SP_PROBE ({SP_PROBE:#010X}) landed at {:#010X} with D0.w = {:#06X}. \
             Neither exact (pc == SP_PROBE, D0.w == {SP_PROBE_PRE_D0}) nor afterCommit \
             (pc == {:#010X}, D0.w == {SP_PROBE_POST_D0}): the PC does not describe the machine, which \
             is worse than an imprecise stop and is what `approximate` would have to be declared for.",
            s.pc,
            s.d0,
            SP_PROBE + PROBE_LEN
        )
    }
}

/// Classify a stop armed on the **access** at [`SP_STORE`]: the commit is visible in memory rather than
/// in a register, so the discriminator is `mem16 - D1.w`.
fn classify_store_stop(s: Snap) -> Measured {
    if s.pc == SP_STORE && s.store_delta() == -1 {
        Measured::Exact
    } else if s.pc == SP_BRANCH && s.store_delta() == 0 {
        Measured::AfterCommit
    } else {
        panic!(
            "a stop armed on the write at SP_STORE ({SP_STORE:#010X}) landed at {:#010X} with \
             mem16 - D1.w = {}. Neither exact (pc == SP_STORE, delta -1) nor afterCommit \
             (pc == {SP_BRANCH:#010X}, delta 0).",
            s.pc,
            s.store_delta()
        )
    }
}

/// **The check every addressless stop gets**: the reported PC must be one of the fixture's boundaries and
/// the machine state must be the state of a machine *about to execute* the instruction there.
///
/// This is a real assertion and it is also honestly weaker than the probe: `bra.s` has no observable
/// effect, so a stop misreported by one instruction across [`SP_BRANCH`] → [`SP_LOOP`] satisfies it. Every
/// other adjacent pair in the loop is separated. See [`SP_BOUNDARIES`].
fn assert_boundary_consistent(s: Snap, what: &str) -> Measured {
    let row = SP_BOUNDARIES
        .iter()
        .find(|(pc, _, _)| *pc == s.pc)
        .unwrap_or_else(|| {
            panic!(
                "{what}: reported pc {:#010X} is not an instruction boundary of the fixture loop. \
                 Interrupts are masked and the loop is closed, so a PC outside SP_BOUNDARIES means the \
                 stop did not land on an instruction boundary at all.",
                s.pc
            )
        });
    assert_eq!(
        s.d0, row.1,
        "{what}: at pc {:#010X} the instruction there has not executed, so D0.w must be {:#06X}; it is \
         {:#06X}. The reported PC does not describe this machine.",
        s.pc, row.1, s.d0
    );
    assert_eq!(
        s.store_delta(),
        row.2,
        "{what}: at pc {:#010X} mem16 - D1.w must be {}; it is {}.",
        s.pc,
        row.2,
        s.store_delta()
    );
    Measured::BoundaryConsistent
}

// ===================================================================================================
// Part A — the measurement. Nothing here reads a declaration.
// ===================================================================================================

/// How many times a repeatable stop is measured before the measurement is believed.
///
/// **Sampling cannot prove invariance and this constant does not pretend to.** §2.1 rule 3 is explicit:
/// *"a precision that varies **cannot be characterised by sampling**, so a client that has seen four
/// exact stops has learned nothing about the fifth"* — the legacy server's `[0, 0, -98, 0]` is the
/// measured case. What repetition buys is the cheap half: an intermittently early stop of the kind that
/// defect describes shows up in a handful of trials, and a single trial would not have looked for it.
/// The other half is structural and is argued in `docs/2026-09-02-stopprecision.md`, not here.
const TRIALS: usize = 8;

/// **`breakpoint`.** Arm at the probe, let the machine free-run into it, and read `D0` — [`TRIALS`]
/// times, since the defect §2.1 rule 3 legislates for is an *intermittently* early stop.
///
/// A breakpoint is the sharpest case the contract names: it is PC-armed, so it *has* a triggering address
/// and `exact` means `pc` IS that address with the instruction unexecuted.
///
/// **What this does not reach:** the *hosted* halt path. `Engine::halt_on_breakpoint` is called from two
/// run drivers — this engine's own free-run step, which a socket `resume` uses and this test therefore
/// measures, and the player window's loop by way of `Host::pump`. Both read the stopping `pc` from the
/// same `self.sys` at the same point, but only one of them is on the wire, so only one of them is
/// measured here. Registered in `docs/2026-09-02-stopprecision.md` rather than left implied.
#[test]
fn measured_breakpoint_precision() {
    let (_h, mut c) = armed("sp-m-bp");
    settle(&mut c);
    c.ok(
        "emulator/breakpoint_add",
        json!({"addr": format!("0x{SP_PROBE:08X}")}),
    );
    for trial in 0..TRIALS {
        let p = resume_and_wait_for_stop(&mut c, 600 + trial as i64);
        assert_eq!(p["reason"], json!("breakpoint"), "trial {trial}");
        let s = snapshot(&mut c);
        assert_eq!(
            hex_u32(&p["pc"]),
            s.pc,
            "trial {trial}: the event's pc is the machine's pc"
        );
        let m = classify_probe_stop(s);
        assert_eq!(
            m,
            Measured::Exact,
            "trial {trial}: MEASURED breakpoint precision {m:?}"
        );
    }
}

/// **`runTo`.** The other PC-armed stop, and the one whose `pc` a client reads from a *reply* (§2.1
/// rule 4), so it is measured on both the event and the result.
#[test]
fn measured_run_to_precision() {
    let (_h, mut c) = armed("sp-m-runto");
    settle(&mut c);
    for trial in 0..TRIALS {
        let (p, result) = call_capturing_stop(
            &mut c,
            700 + trial as i64,
            "emulator/run_to",
            json!({"addr": format!("0x{SP_PROBE:08X}")}),
        );
        assert_eq!(p["reason"], json!("runTo"), "trial {trial}");
        assert_eq!(result["reached"], json!(true), "trial {trial}");
        assert_eq!(
            hex_u32(&result["pc"]),
            hex_u32(&p["pc"]),
            "trial {trial}: the reply's pc and the event's pc are one stop"
        );
        assert_eq!(
            classify_probe_stop(snapshot(&mut c)),
            Measured::Exact,
            "trial {trial}"
        );
        // Leave the probe behind, or the next `run_to` is answered by the PC we are already sitting on.
        c.ok("emulator/step", json!({}));
    }
}

/// **`step`.** Steered so the step *lands* on the probe: `run_to` the loop head, then step the
/// `moveq #0, D0` that precedes it. Both halves are then observable in one register — the stepped
/// instruction committed (`D0.w == 0` is reachable no other way) and the instruction at the new `pc` has
/// not (`D0.w != 1`).
#[test]
fn measured_step_precision() {
    let (_h, mut c) = armed("sp-m-step");
    settle(&mut c);
    call_capturing_stop(
        &mut c,
        710,
        "emulator/run_to",
        json!({"addr": format!("0x{SP_LOOP:08X}")}),
    );
    let (p, result) = call_capturing_stop(&mut c, 711, "emulator/step", json!({}));
    assert_eq!(p["reason"], json!("step"));
    assert_eq!(hex_u32(&result["pc"]), hex_u32(&p["pc"]));
    let s = snapshot(&mut c);
    assert_eq!(
        s.pc, SP_PROBE,
        "one step from the loop head must land on the probe"
    );
    assert_eq!(
        s.d0, SP_PROBE_PRE_D0,
        "the stepped `moveq #0, D0` committed and the probe at pc has NOT run"
    );
    assert_eq!(
        assert_boundary_consistent(s, "step"),
        Measured::BoundaryConsistent
    );
}

/// **`watchpoint`.** Access-armed, on the store's write. §6 calls this stop *"with the triggering
/// instruction fully committed"*; this measures whether that is what the machine does.
#[test]
fn measured_watchpoint_precision() {
    let (_h, mut c) = armed("sp-m-wp");
    settle(&mut c);
    c.ok(
        "emulator/watchpoint_add",
        json!({"addr": format!("0x{SP_STORE_ADDR:08X}"), "len": 2, "stopAfter": 1}),
    );
    let (p, _) = call_capturing_stop(&mut c, 720, "emulator/run_frames", json!({"frames": 4}));
    assert_eq!(
        p["reason"],
        json!("watchpoint"),
        "the run ended on the watch"
    );
    let m = classify_store_stop(snapshot(&mut c));
    assert_eq!(
        m,
        Measured::AfterCommit,
        "MEASURED watchpoint precision: {m:?}"
    );
}

/// **`runFrames`, `runToScanline` and `pause`** — the three with no triggering address. §3: *"`pause`,
/// `entry` and `runFrames` have no triggering address, so their value is `\"exact\"` by definition"*, and
/// §11.31 says the same of `runToScanline` because *"the definition binds `pc`, not the line"*. What is
/// left to measure is the half that is not definitional: that the reported `pc` is an instruction
/// boundary and that the machine state is the one that PC implies.
#[test]
fn measured_addressless_stop_precision() {
    let (_h, mut c) = armed("sp-m-addressless");
    settle(&mut c);

    let (p, _) = call_capturing_stop(&mut c, 730, "emulator/run_frames", json!({"frames": 1}));
    assert_eq!(p["reason"], json!("runFrames"));
    let s = snapshot(&mut c);
    assert_eq!(hex_u32(&p["pc"]), s.pc);
    assert_boundary_consistent(s, "runFrames");

    let (p, _) = call_capturing_stop(
        &mut c,
        731,
        "emulator/run_to_scanline",
        json!({"line": 100}),
    );
    assert_eq!(p["reason"], json!("runToScanline"));
    let s = snapshot(&mut c);
    assert_eq!(hex_u32(&p["pc"]), s.pc);
    assert_boundary_consistent(s, "runToScanline");

    c.ok("emulator/resume", json!({}));
    let (p, _) = call_capturing_stop(&mut c, 732, "emulator/pause", json!({}));
    assert_eq!(p["reason"], json!("pause"));
    let s = snapshot(&mut c);
    assert_eq!(hex_u32(&p["pc"]), s.pc);
    assert_boundary_consistent(s, "pause");
}

/// **The negative half of the key-set rule, measured rather than assumed.** §2.1 rule 1 is *"no more, no
/// fewer"* in both directions, so before `entry` may be left out of the handshake map it has to be true
/// that this server never emits it. `emit_stopped` is called with a literal at every site, so the set is
/// enumerable from source; this is the runtime control on that reading — every reason observed across the
/// surface, collected, and checked against the enumeration.
#[test]
fn the_reasons_this_server_emits_are_the_seven_it_names() {
    let (_h, mut c) = armed("sp-m-reasons");
    let swept = sweep_every_reason(&mut c);
    let mut seen: Vec<&str> = swept.iter().map(|(r, _)| r.as_str()).collect();
    seen.sort_unstable();
    assert_eq!(
        seen,
        vec![
            "breakpoint",
            "pause",
            "runFrames",
            "runTo",
            "runToScanline",
            "step",
            "watchpoint"
        ],
        "the reasons this server can be driven to emit"
    );
    assert!(
        !seen.contains(&"entry"),
        "`entry` is in §3's enum and this server has no path that emits it — so rule 1's \"no more\" \
         half forbids it from appearing in the handshake map"
    );
}

/// Drive the surface until every reason has been seen once, returning `(reason, stopPrecision)` per
/// distinct reason. Shared by the emitted-set control above and the key-set test below, so the two
/// cannot disagree about what "the emitted set" means.
fn sweep_every_reason(c: &mut Client) -> Vec<(String, String)> {
    let mut seen: Vec<(String, String)> = Vec::new();
    let mut note = |p: &Value| {
        let r = p["reason"].as_str().expect("reason").to_string();
        let v = p["stopPrecision"]
            .as_str()
            .unwrap_or_else(|| panic!("§3: `stopPrecision` is REQUIRED on every stopped: {p}"))
            .to_string();
        if !seen.iter().any(|(n, _)| *n == r) {
            seen.push((r, v));
        }
    };
    settle(c);
    let (p, _) = call_capturing_stop(c, 850, "emulator/run_frames", json!({"frames": 1}));
    note(&p);
    let (p, _) = call_capturing_stop(c, 851, "emulator/run_to_scanline", json!({"line": 60}));
    note(&p);
    let (p, _) = call_capturing_stop(
        c,
        852,
        "emulator/run_to",
        json!({"addr": format!("0x{SP_LOOP:08X}")}),
    );
    note(&p);
    let (p, _) = call_capturing_stop(c, 853, "emulator/step", json!({}));
    note(&p);
    c.ok("emulator/resume", json!({}));
    let (p, _) = call_capturing_stop(c, 854, "emulator/pause", json!({}));
    note(&p);
    c.ok(
        "emulator/breakpoint_add",
        json!({"addr": format!("0x{SP_PROBE:08X}")}),
    );
    note(&resume_and_wait_for_stop(c, 860));
    c.ok("emulator/breakpoint_clear", json!({"all": true}));
    c.ok(
        "emulator/watchpoint_add",
        json!({"addr": format!("0x{SP_STORE_ADDR:08X}"), "len": 2, "stopAfter": 1}),
    );
    let (p, _) = call_capturing_stop(c, 855, "emulator/run_frames", json!({"frames": 4}));
    note(&p);
    seen
}

// ===================================================================================================
// Part B — contract §8 item 24. The declaration, closed against the instrument above.
// ===================================================================================================

/// The `stopPrecision` map out of an `initialize` result, parsed through the server's own three wire
/// spellings rather than through a fourth copy of the enum written here.
fn declared_map(hs: &Value) -> BTreeMap<String, StopPrecision> {
    let obj = hs
        .get("stopPrecision")
        .unwrap_or_else(|| {
            panic!(
                "no top-level `stopPrecision` in the initialize result. §2.1: it is a top-level key \
                 like `timingBasis` and `limits`, NOT under `capabilities` and NOT under `limits`. \
                 Keys present: {:?}",
                hs.as_object().map(|o| o.keys().collect::<Vec<_>>())
            )
        })
        .as_object()
        .expect("`stopPrecision` is an object mapping reason -> precision");
    obj.iter()
        .map(|(k, v)| {
            let s = v
                .as_str()
                .unwrap_or_else(|| panic!("{k}: not a string: {v}"));
            (
                k.clone(),
                StopPrecision::from_wire(s)
                    .unwrap_or_else(|| panic!("{k}: {s:?} is not one of §2.1's three values")),
            )
        })
        .collect()
}

/// What the machine was doing when the stop landed, in the form the item-24 assertion needs: which
/// instruction the stop was armed at, and the state read afterwards.
enum Evidence {
    /// Armed at [`SP_PROBE`] — a PC-armed stop with a single observable register effect.
    Probe(Snap),
    /// Armed on the access at [`SP_STORE`] — the commit is observable in memory.
    Store(Snap),
    /// No triggering address. Only the boundary invariant is available, and that is the whole of what
    /// `exact` can mean for a stop with nothing to be exact *about*.
    Addressless(Snap),
}

/// **The item-24 assertion, for one stop.**
///
/// Order follows the item's own wording. First the two riders — the reason has a map entry, and the
/// event's value is not weaker than the declaration. Then the register check, run against **the value
/// the event carried**: §2.1 rule 3 permits that to be stronger than the declaration, so it is the
/// stronger obligation to hold the server to, and checking the declaration alone would let a server that
/// declares `approximate` and emits `exact` past.
fn prove_item24(
    reason: &str,
    event: &Value,
    map: &BTreeMap<String, StopPrecision>,
    ev: Evidence,
) -> StopPrecision {
    let declared = *map.get(reason).unwrap_or_else(|| {
        panic!(
            "rider 1: `{reason}` was observed on emulator/stopped and has NO entry in the handshake \
             map — §2.1 rule 1's checkable half. Map: {:?}",
            map.keys().collect::<Vec<_>>()
        )
    });
    let on_wire = event["stopPrecision"].as_str().unwrap_or_else(|| {
        panic!(
            "§3: `stopPrecision` is REQUIRED on every emulator/stopped; this one has none: {event}"
        )
    });
    let carried = StopPrecision::from_wire(on_wire)
        .unwrap_or_else(|| panic!("{reason}: {on_wire:?} is not one of §2.1's three values"));
    assert!(
        carried.at_least_as_strong_as(declared),
        "rider 2: `{reason}` declared {declared:?} and the event carried {carried:?}, which is WEAKER. \
         §2.1 rule 3: a server may emit a stronger value than it declared; it may never emit a weaker \
         one."
    );

    match (carried, ev) {
        // "assert the PRE-trigger state for a declared `exact`"
        (StopPrecision::Exact, Evidence::Probe(s)) => {
            assert_eq!(
                classify_probe_stop(s),
                Measured::Exact,
                "`{reason}` says `exact`: the stop must be AT the armed instruction with \
                 `addq.w #1, D0` unexecuted — D0.w == {SP_PROBE_PRE_D0}"
            );
        }
        (StopPrecision::AfterCommit, Evidence::Probe(s)) => {
            assert_eq!(
                classify_probe_stop(s),
                Measured::AfterCommit,
                "`{reason}` says `afterCommit`: the armed instruction must have COMMITTED — D0.w == \
                 {SP_PROBE_POST_D0} at pc {:#010X}",
                SP_PROBE + PROBE_LEN
            );
        }
        (StopPrecision::Exact, Evidence::Store(s)) => {
            assert_eq!(
                classify_store_stop(s),
                Measured::Exact,
                "`{reason}` says `exact`: the store must NOT yet have committed — mem16 == D1.w - 1 at \
                 pc {SP_STORE:#010X}"
            );
        }
        // "…and the POST-trigger state for a declared `afterCommit`"
        (StopPrecision::AfterCommit, Evidence::Store(s)) => {
            assert_eq!(
                classify_store_stop(s),
                Measured::AfterCommit,
                "`{reason}` says `afterCommit`: the triggering store must have fully committed — \
                 mem16 == D1.w"
            );
        }
        // A stop with no triggering address. `exact` still means "the instruction at pc has not
        // executed", and that is exactly what the boundary table asserts.
        (StopPrecision::Exact | StopPrecision::AfterCommit, Evidence::Addressless(s)) => {
            assert_boundary_consistent(s, reason);
        }
        // "a declared `approximate` asserts only that the event carried the key" — which the `on_wire`
        // read above already did. Nothing further is provable, and pretending otherwise is the vacuity
        // this item exists to prevent.
        (StopPrecision::Approximate, _) => {}
    }
    carried
}

/// **§2.1 rule 1's checkable half, three ways.**
///
/// The handshake map's key set, the registry the server generates it from, and the set of reasons the
/// surface can actually be driven to emit must be one set. Two of the three come from
/// `engine::StopReason` — the point being that a hand-written expectation here would be the second list
/// rule 1 exists to forbid — and the third is collected at runtime, which closes the "no more"
/// direction the type system cannot.
#[test]
fn item24_the_handshake_map_key_set_is_the_registry_and_the_emitted_set() {
    let (_h, mut c, hs) = armed_with_handshake("sp-i24-keys");
    let map = declared_map(&hs);

    let mut from_map: Vec<&str> = map.keys().map(String::as_str).collect();
    let mut from_registry: Vec<&str> = StopReason::ALL.iter().map(|r| r.wire()).collect();
    from_map.sort_unstable();
    from_registry.sort_unstable();
    assert_eq!(
        from_map, from_registry,
        "the handshake map must be generated from StopReason::ALL — no more, no fewer"
    );

    // …and every value in it is the registry's, not a second opinion.
    for r in StopReason::ALL {
        assert_eq!(
            map.get(r.wire()),
            Some(&r.precision()),
            "`{}`'s declared precision must come from the registry",
            r.wire()
        );
    }

    // The runtime half: drive the surface and check nothing outside the map comes out — and that every
    // event carried a value at least as strong as the map's. `entry` is the case this guards: it is in
    // §3's enum, it is NOT in our map, and it must never appear.
    let observed = sweep_every_reason(&mut c);
    let mut observed_names: Vec<&str> = observed.iter().map(|(r, _)| r.as_str()).collect();
    observed_names.sort_unstable();
    assert_eq!(
        observed_names, from_registry,
        "every reason the surface can be driven to emit, and nothing else"
    );
    for (reason, on_wire) in &observed {
        let declared = map[reason];
        let carried = StopPrecision::from_wire(on_wire).expect("one of §2.1's three values");
        assert!(
            carried.at_least_as_strong_as(declared),
            "`{reason}` carried {carried:?}, weaker than the declared {declared:?}"
        );
    }
}

/// **§8 item 24 itself**: for every `reason` this server declares and can produce in the harness, arm a
/// stop at an instruction with a single observable register effect, halt, read that register, and hold
/// the server to the value it put on the wire.
///
/// **What it covers.** All seven declared reasons are produced. Four are armed at an instruction and get
/// the full pre/post discriminator: `breakpoint`, `runTo` and `step` at the `addq.w #1, D0` probe, and
/// `watchpoint` at the store. Three — `runFrames`, `runToScanline`, `pause` — have no triggering address
/// (§3: *"`pause`, `entry` and `runFrames` have no triggering address, so their value is `exact` by
/// definition"*), so what is checkable is the other half of the definition: the reported `pc` is an
/// instruction boundary and the machine is in the state of one about to execute the instruction there.
///
/// **What it provably cannot cover** is written down in `docs/2026-09-02-stopprecision.md` rather than
/// paraphrased here, so there is one copy of it.
#[test]
fn item24_every_declared_reason_is_proven_against_the_machine() {
    let (_h, mut c, hs) = armed_with_handshake("sp-i24-proof");
    let map = declared_map(&hs);
    settle(&mut c);

    let mut proven: BTreeMap<String, StopPrecision> = BTreeMap::new();

    // --- breakpoint: PC-armed at the probe ------------------------------------------------------
    c.ok(
        "emulator/breakpoint_add",
        json!({"addr": format!("0x{SP_PROBE:08X}")}),
    );
    let e = resume_and_wait_for_stop(&mut c, 799);
    assert_eq!(e["reason"], json!("breakpoint"));
    let v = prove_item24("breakpoint", &e, &map, Evidence::Probe(snapshot(&mut c)));
    proven.insert("breakpoint".into(), v);
    c.ok("emulator/breakpoint_clear", json!({"all": true}));

    // --- runTo: the other PC-armed stop, at the same probe --------------------------------------
    c.ok("emulator/step", json!({})); // step off the probe, or `run_to` answers from where we sit
    let (e, _) = call_capturing_stop(
        &mut c,
        800,
        "emulator/run_to",
        json!({"addr": format!("0x{SP_PROBE:08X}")}),
    );
    assert_eq!(e["reason"], json!("runTo"));
    let v = prove_item24("runTo", &e, &map, Evidence::Probe(snapshot(&mut c)));
    proven.insert("runTo".into(), v);

    // --- step: steered so the step LANDS on the probe -------------------------------------------
    call_capturing_stop(
        &mut c,
        801,
        "emulator/run_to",
        json!({"addr": format!("0x{SP_LOOP:08X}")}),
    );
    let (e, _) = call_capturing_stop(&mut c, 802, "emulator/step", json!({}));
    assert_eq!(e["reason"], json!("step"));
    let s = snapshot(&mut c);
    assert_eq!(
        s.pc, SP_PROBE,
        "one step from the loop head lands on the probe"
    );
    let v = prove_item24("step", &e, &map, Evidence::Probe(s));
    proven.insert("step".into(), v);

    // --- runFrames / runToScanline / pause: no triggering address --------------------------------
    let (e, _) = call_capturing_stop(&mut c, 803, "emulator/run_frames", json!({"frames": 1}));
    assert_eq!(e["reason"], json!("runFrames"));
    let v = prove_item24(
        "runFrames",
        &e,
        &map,
        Evidence::Addressless(snapshot(&mut c)),
    );
    proven.insert("runFrames".into(), v);

    let (e, _) = call_capturing_stop(
        &mut c,
        804,
        "emulator/run_to_scanline",
        json!({"line": 120}),
    );
    assert_eq!(e["reason"], json!("runToScanline"));
    let v = prove_item24(
        "runToScanline",
        &e,
        &map,
        Evidence::Addressless(snapshot(&mut c)),
    );
    proven.insert("runToScanline".into(), v);

    c.ok("emulator/resume", json!({}));
    let (e, _) = call_capturing_stop(&mut c, 805, "emulator/pause", json!({}));
    assert_eq!(e["reason"], json!("pause"));
    let v = prove_item24("pause", &e, &map, Evidence::Addressless(snapshot(&mut c)));
    proven.insert("pause".into(), v);

    // --- watchpoint: access-armed on the store ---------------------------------------------------
    c.ok(
        "emulator/watchpoint_add",
        json!({"addr": format!("0x{SP_STORE_ADDR:08X}"), "len": 2, "stopAfter": 1}),
    );
    let (e, _) = call_capturing_stop(&mut c, 806, "emulator/run_frames", json!({"frames": 4}));
    assert_eq!(e["reason"], json!("watchpoint"), "the watch ended the run");
    let v = prove_item24("watchpoint", &e, &map, Evidence::Store(snapshot(&mut c)));
    proven.insert("watchpoint".into(), v);

    // **The anti-vacuity check on this test itself.** Item 24 is "for EVERY reason it declares and can
    // produce"; a version of it that quietly proved four of seven would be green and would be the worst
    // outcome of this parcel. So the set proven above is closed against the declaration.
    let mut proven_names: Vec<&str> = proven.keys().map(String::as_str).collect();
    let mut declared_names: Vec<&str> = map.keys().map(String::as_str).collect();
    proven_names.sort_unstable();
    declared_names.sort_unstable();
    assert_eq!(
        proven_names, declared_names,
        "every DECLARED reason must be produced and proven here, or named as unproducible in \
         docs/2026-09-02-stopprecision.md — a declaration this test skipped is a declaration checked by \
         nothing, which is the defect item 24 exists to close"
    );
}
