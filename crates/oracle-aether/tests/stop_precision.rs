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
use oracle_aether::server::ServerHandle;
use oracle_core::testrom::{
    build_stop_precision, SP_BOUNDARIES, SP_BRANCH, SP_LOOP, SP_PROBE, SP_PROBE_POST_D0,
    SP_PROBE_PRE_D0, SP_STORE, SP_STORE_ADDR,
};
use serde_json::{json, Value};

/// The length of `addq.w #1, D0` in bytes — where an `afterCommit` stop armed at [`SP_PROBE`] would
/// report its `pc`. Derived from the two constants that bracket it, never written down as a number.
const PROBE_LEN: u32 = 0x0000_0208 - SP_PROBE;

fn armed(tag: &str) -> (ServerHandle, Client) {
    let h = spawn_with(tag, build_stop_precision(), 1024);
    let mut c = Client::connect(&h);
    c.handshake(true);
    (h, c)
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

/// Read lines until the next `emulator/stopped` — for the halts nobody's reply announces (a breakpoint
/// that ends a free run).
fn next_stopped(c: &mut Client) -> Value {
    loop {
        let v = c.recv();
        if v.get("method").and_then(Value::as_str) == Some("emulator/stopped") {
            return v["params"].clone();
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
        c.ok("emulator/resume", json!({}));
        let p = next_stopped(&mut c);
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
    let mut seen: Vec<String> = Vec::new();
    let mut note = |p: &Value| {
        let r = p["reason"].as_str().expect("reason").to_string();
        if !seen.contains(&r) {
            seen.push(r);
        }
    };

    settle(&mut c);
    let (p, _) = call_capturing_stop(&mut c, 740, "emulator/run_frames", json!({"frames": 1}));
    note(&p);
    let (p, _) = call_capturing_stop(&mut c, 741, "emulator/run_to_scanline", json!({"line": 40}));
    note(&p);
    let (p, _) = call_capturing_stop(
        &mut c,
        742,
        "emulator/run_to",
        json!({"addr": format!("0x{SP_LOOP:08X}")}),
    );
    note(&p);
    let (p, _) = call_capturing_stop(&mut c, 743, "emulator/step", json!({}));
    note(&p);
    c.ok("emulator/resume", json!({}));
    let (p, _) = call_capturing_stop(&mut c, 744, "emulator/pause", json!({}));
    note(&p);
    c.ok(
        "emulator/breakpoint_add",
        json!({"addr": format!("0x{SP_PROBE:08X}")}),
    );
    c.ok("emulator/resume", json!({}));
    note(&next_stopped(&mut c));
    c.ok("emulator/breakpoint_clear", json!({"all": true}));
    c.ok(
        "emulator/watchpoint_add",
        json!({"addr": format!("0x{SP_STORE_ADDR:08X}"), "len": 2, "stopAfter": 1}),
    );
    let (p, _) = call_capturing_stop(&mut c, 745, "emulator/run_frames", json!({"frames": 4}));
    note(&p);

    seen.sort();
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
        !seen.iter().any(|r| r == "entry"),
        "`entry` is in §3's enum and this server has no path that emits it — so rule 1's \"no more\" \
         half forbids it from appearing in the handshake map"
    );
}
