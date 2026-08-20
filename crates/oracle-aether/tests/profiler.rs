//! The profiler surface — `protocol.md` §6 (profiler), adopted as CR-26 (§11.16) with its three
//! adjudicated deltas.
//!
//! Every test here is a **wire round trip**, so each reply is validated against the vendored contract
//! fragments on the way past — open, and then closed with `unevaluatedProperties: false` per §8 item 20.
//! For this family that validation carries real weight rather than being ceremony: the fragments make nine
//! result keys REQUIRED, tie `budgetPct` and `budgetPctOmitted` into an exclusive pair with an
//! `anyOf` + `not`, and require `cyclesSelf` on an interrupt bucket — a key added by the delta ruling
//! precisely so the reconciliation identity §6 tells a client to compute is *computable from a reply*.
//!
//! The gates the adoption condition names live here:
//!
//! * **The reconciliation identity, computed from a reply** —
//!   [`the_identity_closes_when_computed_from_the_wire`]. Not from the internal `Report`, which is how the
//!   in-tree gate closed while the fragment still refused the key the sum needs; from the JSON, using only
//!   keys a client can see. Since delta 3 its primary form is asserted with `==` and **no condition
//!   attached**, over the undivided `*Total` figures.
//! * **The pair invariant** — [`every_divided_figure_has_an_undivided_partner_that_bounds_it`]: each of the
//!   four divided figures equals its `*Total` partner over `frameCount`, on every row and both buckets,
//!   which is what makes a total one accumulator read twice rather than a second measurement.
//! * **The negative control** — [`the_undivided_set_is_refused_on_a_per_frame_row`]: the one place in this
//!   file that checks a field did *not* arrive.
//! * **Determinism, three boots byte-identical** — [`three_boots_are_byte_identical`]. Aeon's spread-0 bar
//!   expressed as this suite's gate.

mod common;

use common::{spawn_with, Client};
use serde_json::{json, Value};

/// Boot a fixture on its own server and hand back a connected client.
fn booted(
    tag: &str,
    shape: oracle_core::testrom::ProfilerShape,
) -> (oracle_aether::server::ServerHandle, Client, Value) {
    let h = spawn_with(tag, oracle_core::testrom::build_profiler(shape), 64);
    let mut c = Client::connect(&h);
    let init = c.handshake(false);
    (h, c, init)
}

/// The fixture this file profiles unless a test needs another: one VBlank per frame and a leaf called
/// three times, so both a routine row and an interrupt bucket have something in them.
fn default_shape() -> oracle_core::testrom::ProfilerShape {
    oracle_core::testrom::ProfilerShape::CallsLeaf { k: 3 }
}

/// Arm, run, read — the three-call sequence §11.16 promises works with no sleep and no extra frame.
fn arm_run_read(c: &mut Client, frames: u64, per_frame: bool) -> Value {
    c.ok(
        "emulator/set_profiler",
        json!({"enabled": true, "perFrame": per_frame}),
    );
    c.ok("emulator/run_frames", json!({"frames": frames}));
    c.ok("emulator/get_profiler_frames", json!({}))
}

fn u64_of(v: &Value, key: &str) -> u64 {
    v[key]
        .as_u64()
        .unwrap_or_else(|| panic!("{key} must be a non-negative integer, got {}", v[key]))
}

/// Sum one numeric key across every routine row **and** both interrupt buckets — the left-hand side of the
/// reconciliation identity, over whichever spelling of "self cycles" is being reconciled.
///
/// Factored out because there are two spellings, and delta 3 landed the second: `"cyclesSelfTotal"` is the
/// undivided term the identity closes on exactly and unconditionally, `"cyclesSelf"` the divided one whose
/// reconstruction needs a `× frameCount` and a `perFrameExact` branch. One traversal serves both, so the
/// exact assertion is one line rather than a second copy of this walk that can drift from the first.
fn sum_over_rows_and_buckets(r: &Value, key: &str) -> u64 {
    let rows: u64 = r["routines"]["items"]
        .as_array()
        .expect("routines.items")
        .iter()
        .map(|x| u64_of(x, key))
        .sum();
    let buckets: u64 = ["hint", "vint"]
        .iter()
        .map(|k| u64_of(&r["interrupts"][k], key))
        .sum();
    rows + buckets
}

// --- the surface exists ------------------------------------------------------------------------------

/// The capability flips, the limits are advertised, and the three methods are in the method list.
///
/// Advertising IS shipping: `schema_conformance.rs`'s `UNCOVERED_METHODS` is pinned empty and computed
/// from this very list, so a method advertised without a fragment turns that suite red. These three are
/// safe to advertise only because the fragments landed first.
#[test]
fn the_handshake_advertises_the_profiler_and_its_limits() {
    let h = spawn_with(
        "prof-init",
        oracle_core::testrom::build_profiler(default_shape()),
        64,
    );
    let mut c = Client::connect(&h);
    let init = c.handshake(false);

    assert_eq!(
        init["capabilities"]["profiler"],
        json!(true),
        "the capability a client branches on (D5), not the version integer"
    );
    for k in ["maxProfilerRoutines", "maxProfilerFrames"] {
        assert!(
            init["limits"][k].as_u64().is_some_and(|n| n > 0),
            "{k} is REQUIRED once the methods are advertised — a cap a client can only learn by hitting \
             it costs work to discover; limits: {}",
            init["limits"]
        );
    }
    let methods: Vec<&str> = init["methods"]
        .as_array()
        .expect("methods is an array")
        .iter()
        .map(|m| m.as_str().unwrap_or_default())
        .collect();
    for m in [
        "emulator/set_profiler",
        "emulator/get_profiler",
        "emulator/get_profiler_frames",
    ] {
        assert!(
            methods.contains(&m),
            "{m} must be advertised; got {methods:?}"
        );
    }
}

/// Arm, run, read in three consecutive calls — no sleep, no poll, no extra frame for the arm to take.
/// And none of the three is refused for the machine's run state: the sample's edges are frame boundaries,
/// not the instant the command landed, so the free-running case below is exactly as well defined.
#[test]
fn arming_is_synchronous_and_never_refused_for_run_state() {
    let (_h, mut c, _init) = booted("prof-sync", default_shape());

    let armed = c.ok("emulator/set_profiler", json!({"enabled": true}));
    assert_eq!(armed["enabled"], json!(true));
    assert_eq!(armed["perFrame"], json!(false), "off unless asked");

    c.ok("emulator/run_frames", json!({"frames": 4}));
    let state = c.ok("emulator/get_profiler", json!({}));
    assert_eq!(state["enabled"], json!(true));
    assert!(
        u64_of(&state, "framesRecorded") > 0,
        "the arm took effect on the very next run: {state}"
    );

    // The two counts MUST agree when no frames ran between the calls — the legacy surface had two that
    // could differ, and only one of them was ever the divisor.
    let sample = c.ok("emulator/get_profiler_frames", json!({}));
    assert_eq!(
        state["framesRecorded"], sample["frameCount"],
        "get_profiler.framesRecorded and get_profiler_frames.frameCount are the same number"
    );

    // Free-running: still not refused. All three are pure with respect to the run state.
    c.ok("emulator/resume", json!({}));
    c.ok("emulator/set_profiler", json!({"enabled": true}));
    c.ok("emulator/get_profiler", json!({}));
    c.ok("emulator/get_profiler_frames", json!({}));
}

/// Arming **resets**; disarming **retains**. There is no resume in this revision, so a second arm discards
/// an in-flight sample — and a client that arms, runs, disarms and reads must still get its sample.
#[test]
fn arming_resets_and_disarming_retains() {
    let (_h, mut c, _init) = booted("prof-reset", default_shape());
    arm_run_read(&mut c, 4, false);

    // Disarm, then read: the sample survives, and reading clears nothing.
    let off = c.ok("emulator/set_profiler", json!({"enabled": false}));
    assert_eq!(off["enabled"], json!(false));
    let after = c.ok("emulator/get_profiler_frames", json!({}));
    let kept = u64_of(&after, "frameCount");
    assert!(kept > 0, "disarm retains the sample: {after}");
    let again = c.ok("emulator/get_profiler_frames", json!({}));
    assert_eq!(
        u64_of(&again, "frameCount"),
        kept,
        "and reading it does not clear it"
    );

    // A disarmed instrument does not keep recording.
    c.ok("emulator/run_frames", json!({"frames": 3}));
    assert_eq!(
        u64_of(
            &c.ok("emulator/get_profiler_frames", json!({})),
            "frameCount"
        ),
        kept,
        "a disarmed profiler records nothing"
    );

    // Re-arming discards it.
    c.ok("emulator/set_profiler", json!({"enabled": true}));
    assert_eq!(
        u64_of(
            &c.ok("emulator/get_profiler_frames", json!({})),
            "frameCount"
        ),
        0,
        "arming RESETS — a second arm discards the in-flight sample"
    );
}

// --- the identity ------------------------------------------------------------------------------------

/// **The reconciliation identity, computed from a reply.**
///
/// The primary form, since delta 3, needs no caveat and no arithmetic:
///
/// ```text
/// Σ routines[].cyclesSelfTotal + Σ interrupts[].cyclesSelfTotal + unattributedCycles == sampleCycles
/// ```
///
/// — asserted **unconditionally**, with `==`, on whatever sample the fixture happens to produce. No
/// `× frameCount`, no `perFrameExact` branch, and every term a REQUIRED key of the reply. That is the whole
/// point of the undivided set: before it, a suite could only gate the identity on a fixture engineered to
/// divide evenly.
///
/// The divided view's reconstruction is kept beneath it as the **secondary** check, because it is still
/// what a client reading only the per-frame figures can compute, and its weakness is worth pinning:
///
/// ```text
/// (Σ routines[].cyclesSelf + Σ interrupts[].cyclesSelf) × frameCount + unattributedCycles == sampleCycles
/// ```
///
/// exact when `perFrameExact`, and otherwise floored per summed figure — short by at most `frameCount − 1`
/// each and NEVER over. Both directions are asserted.
///
/// This is the D-M1 lesson made executable. The in-tree gate closed while the fragment still refused
/// `cyclesSelf` on a bucket, because that gate reads the internal `Report` — whose buckets always carried
/// the field. The contract told a client to compute a sum and then rejected every reply that permitted it,
/// and nothing noticed, because nothing computed it *from a reply*. This does.
#[test]
fn the_identity_closes_when_computed_from_the_wire() {
    let (_h, mut c, _init) = booted("prof-identity", default_shape());
    let r = arm_run_read(&mut c, 6, false);

    let frame_count = u64_of(&r, "frameCount");
    assert!(frame_count > 0, "a sample of no frames proves nothing: {r}");

    // Every term below is a key of the reply. Nothing is read from the server's internals.
    let rows = r["routines"]["items"].as_array().expect("routines.items");
    let divided_self = sum_over_rows_and_buckets(&r, "cyclesSelf");
    let unattributed = u64_of(&r, "unattributedCycles");
    let sample = u64_of(&r, "sampleCycles");

    // The whole list must be present for the sum to mean anything — a truncated `routines` would make the
    // reconstruction fall short for a reason that has nothing to do with truncation of the divisions.
    assert_eq!(
        r["routines"]["truncated"],
        json!(false),
        "this sample must fit in one reply or the sum below is not the sample's"
    );

    // PRIMARY: the undivided identity, exact, with no condition attached to it at all.
    let exact_self = sum_over_rows_and_buckets(&r, "cyclesSelfTotal");
    assert_eq!(
        exact_self + unattributed,
        sample,
        "the undivided identity closes exactly and unconditionally (perFrameExact is {}): \
         {exact_self} + {unattributed} vs {sample}",
        r["perFrameExact"]
    );

    // SECONDARY: what a client reading only the divided figures can reconstruct, and its bound.
    let reconstructed = divided_self * frame_count + unattributed;
    if r["perFrameExact"] == json!(true) {
        assert_eq!(
            reconstructed, sample,
            "perFrameExact, so the reconstruction closes on the nose: \
             {divided_self} x {frame_count} + {unattributed} vs {sample}"
        );
    } else {
        // Floored per summed figure: one unit per row and per bucket, each worth up to frameCount - 1.
        let terms = rows.len() as u64 + 2;
        let slack = terms * (frame_count - 1);
        assert!(
            reconstructed <= sample && sample - reconstructed <= slack,
            "the reconstruction may fall SHORT by at most {slack} ({terms} figures x {} each) and never \
             over: got {reconstructed} vs {sample}",
            frame_count - 1
        );
    }
}

/// An empty sample is answered, not refused, and its degenerate identity is `0 + 0 + 0 == 0`.
#[test]
fn an_empty_sample_is_a_reply_and_not_an_error() {
    let (_h, mut c, _init) = booted("prof-empty", default_shape());
    c.ok("emulator/set_profiler", json!({"enabled": true}));
    let r = c.ok("emulator/get_profiler_frames", json!({}));

    assert_eq!(u64_of(&r, "frameCount"), 0);
    assert_eq!(u64_of(&r, "sampleCycles"), 0);
    assert_eq!(u64_of(&r, "totalCycles"), 0);
    assert_eq!(u64_of(&r, "unattributedCycles"), 0);
    assert_eq!(
        r["perFrameExact"],
        json!(true),
        "there was nothing to divide, so nothing was lost"
    );
    assert_eq!(r["routines"]["total"], json!(0));
    assert_eq!(r["routines"]["items"].as_array().map(Vec::len), Some(0));

    // There are no rows here, but **both buckets are always present**, so an empty sample does have
    // carriers for the undivided set — they carry all-zero pairs. That is why the pair invariant is scoped
    // by `frameCount > 0` rather than by an absence of carriers that is not true of buckets.
    for k in ["hint", "vint"] {
        for f in UNDIVIDED {
            assert_eq!(
                u64_of(&r["interrupts"][k], f),
                0,
                "an empty sample's {k} bucket carries {f} as 0, not as an absence: {}",
                r["interrupts"][k]
            );
        }
    }
    assert_eq!(
        sum_over_rows_and_buckets(&r, "cyclesSelfTotal") + u64_of(&r, "unattributedCycles"),
        u64_of(&r, "sampleCycles"),
        "the degenerate identity, in the same unconditional form: 0 + 0 == 0"
    );
}

// --- the undivided set (delta 3) -------------------------------------------------------------------------

/// The four undivided fields, and the divided partner each one bounds.
const PAIRS: [(&str, &str); 4] = [
    ("cycles", "cyclesTotal"),
    ("cyclesSelf", "cyclesSelfTotal"),
    ("stallCycles", "stallCyclesTotal"),
    ("calls", "callsTotal"),
];

/// Just the undivided half of [`PAIRS`], for the places that only need the names.
const UNDIVIDED: [&str; 4] = [
    "cyclesTotal",
    "cyclesSelfTotal",
    "stallCyclesTotal",
    "callsTotal",
];

/// **Every divided figure has an undivided partner, and the partner bounds its truncation.**
///
/// `divided == total / frameCount` under integer division whenever `frameCount > 0`, equivalently
///
/// ```text
/// divided × frameCount ≤ total < (divided + 1) × frameCount
/// ```
///
/// Checked for all four pairs on every routine row **and** on both interrupt buckets, across three fixture
/// shapes — a leaf called three times a frame, a VBlank-only sample, and a DMA stall, the last so the
/// `stallCycles` pair is not checked only against zeroes. Asserted in both spellings: the division and the
/// two-sided bound are the same statement, and a server that satisfied one without the other would be
/// emitting a partner that is not the number it was divided from.
///
/// The relation is what makes a total more than a second number on the wire: it says the pair came from one
/// accumulator, so a client may mix the two views in one calculation.
#[test]
fn every_divided_figure_has_an_undivided_partner_that_bounds_it() {
    let shapes = [
        ("prof-pairs-leaf", default_shape()),
        (
            "prof-pairs-vint",
            oracle_core::testrom::ProfilerShape::Interrupts {
                hint: false,
                vint: true,
            },
        ),
        (
            "prof-pairs-stall",
            oracle_core::testrom::ProfilerShape::Stall {
                kind: oracle_core::testrom::StallKind::Dma,
            },
        ),
    ];

    // Anti-vacuity: a walk over rows that are all zero would pass every assertion below without testing
    // anything, so the pairs actually exercised are counted and the count is asserted.
    let mut non_zero_pairs = 0_u32;
    let mut truncated_pairs = 0_u32;

    for (tag, shape) in shapes {
        let (_h, mut c, _init) = booted(tag, shape);
        let r = arm_run_read(&mut c, 6, false);
        let n = u64_of(&r, "frameCount");
        assert!(n > 0, "{tag}: a sample of no frames proves nothing: {r}");

        let mut carriers: Vec<(String, &Value)> = vec![
            ("interrupts.hint".to_string(), &r["interrupts"]["hint"]),
            ("interrupts.vint".to_string(), &r["interrupts"]["vint"]),
        ];
        let rows = r["routines"]["items"].as_array().expect("routines.items");
        for (i, row) in rows.iter().enumerate() {
            carriers.push((format!("routines[{i}] {}", row["addr"]), row));
        }

        for (what, carrier) in carriers {
            for (divided_key, total_key) in PAIRS {
                let divided = u64_of(carrier, divided_key);
                let total = u64_of(carrier, total_key);
                assert_eq!(
                    divided,
                    total / n,
                    "{tag} {what}: {divided_key} must be {total_key} / frameCount ({total} / {n}), \
                     not a separately accumulated figure: {carrier}"
                );
                assert!(
                    divided * n <= total && total < (divided + 1) * n,
                    "{tag} {what}: {total_key} must bound {divided_key}'s truncation — \
                     {divided} x {n} <= {total} < {} : {carrier}",
                    (divided + 1) * n
                );
                if total > 0 {
                    non_zero_pairs += 1;
                }
                if divided * n != total {
                    truncated_pairs += 1;
                }
            }
        }
    }

    assert!(
        non_zero_pairs >= 8,
        "the walk must have had real figures to check, not a table of zeroes: {non_zero_pairs}"
    );
    assert!(
        truncated_pairs > 0,
        "and at least one pair must actually have been truncated by the division — otherwise the bound \
         is only ever checked at its equality case, which is the case a bug would not break"
    );
}

/// **The negative control: the undivided set is refused on a `perFrame[]` row.**
///
/// Every other assertion in this file checks that a field *arrived*. This one checks that it did not arrive
/// where it does not belong — the bound delta 3 states in prose (per-frame rows are whole-frame totals with
/// no per-routine breakdown, so they are already undivided and a total there would be a second spelling of
/// the same number) and which the fragment enforces with `additionalProperties: false` on the row shape.
///
/// Proven by validation rather than by inspection: a **real** reply, doctored four ways, one field at a
/// time, must be rejected by the same closed fragment `Client::recv` puts every line through.
#[test]
fn the_undivided_set_is_refused_on_a_per_frame_row() {
    let (_h, mut c, _init) = booted("prof-negctl", default_shape());
    let r = arm_run_read(&mut c, 4, true);
    assert!(
        !r["perFrame"]["items"]
            .as_array()
            .expect("perFrame.items")
            .is_empty(),
        "the control needs a row to doctor: {r}"
    );

    // The reply as it came off the wire already passed; that is this control's positive half, and it is
    // re-asserted here so a validator that stopped rejecting anything could not hide behind it.
    let envelope = |result: &Value| json!({"jsonrpc": "2.0", "id": 1, "result": result});
    common::schema::check_incoming(&envelope(&r), Some("emulator/get_profiler_frames"))
        .expect("the undoctored reply is conformant");

    for f in UNDIVIDED {
        let mut doctored = r.clone();
        doctored["perFrame"]["items"][0][f] = json!(1);
        let failures = common::schema::check_incoming(
            &envelope(&doctored),
            Some("emulator/get_profiler_frames"),
        )
        .expect_err(&format!(
            "a perFrame[] row carrying {f} must be REFUSED — the four undivided fields belong to the \
             routine row and the interrupt bucket only"
        ));
        assert!(
            failures.iter().any(|m| m.contains(f)),
            "the refusal must name {f} rather than failing for some unrelated reason: {failures:?}"
        );
    }
}

// --- the containers and their refusals -----------------------------------------------------------------

/// `top` bounds the rows, is ordered most-expensive-first, and is **refused above the cap, never clamped**
/// — the legacy surface clamped, so a caller could not tell a full list from a clipped one.
#[test]
fn top_bounds_the_rows_and_is_refused_rather_than_clamped() {
    let (_h, mut c, _init) = booted("prof-top", default_shape());
    let full = arm_run_read(&mut c, 6, false);
    let total = u64_of(&full["routines"], "total");
    assert!(total >= 2, "need a few rows to bound: {}", full["routines"]);

    let one = c.ok("emulator/get_profiler_frames", json!({"top": 1}));
    assert_eq!(one["routines"]["returned"], json!(1));
    assert_eq!(one["routines"]["total"], json!(total));
    assert_eq!(one["routines"]["truncated"], json!(true));
    assert!(
        one["routines"].get("cursor").is_none(),
        "no cursor: the method accepts no continuation param (§2.4 clause b)"
    );
    // The kept row is the expensive end, not an arbitrary slice.
    let biggest = full["routines"]["items"][0]["cycles"].clone();
    assert_eq!(
        one["routines"]["items"][0]["cycles"], biggest,
        "a truncated list keeps the most expensive rows"
    );

    let cap = _init["limits"]["maxProfilerRoutines"]
        .as_u64()
        .expect("the cap is advertised");
    let e = c.err("emulator/get_profiler_frames", json!({"top": cap + 1}));
    assert_eq!(e["code"], json!(-32602), "refused, never clamped: {e}");
}

/// `frames` is **refused with `-32005 perFrameNotArmed`** when the ring was not armed. A parameter that
/// cannot affect the answer is worse than one that is rejected — and this refusal is about the
/// *instrument's* state, so the run-state exemption that protects arm/disarm/read does not reach it.
#[test]
fn frames_without_the_ring_is_refused_and_says_why() {
    let (_h, mut c, _init) = booted("prof-noring", default_shape());
    arm_run_read(&mut c, 4, false);

    let e = c.err("emulator/get_profiler_frames", json!({"frames": 2}));
    assert_eq!(e["code"], json!(-32005), "{e}");
    assert_eq!(
        e["data"]["reason"],
        json!("perFrameNotArmed"),
        "the discriminant a client branches on, not the message: {e}"
    );

    // The same call is fine once the ring is armed, and it is refused for the instrument's state even
    // while the machine is paused — this is not the run-state rule wearing a different name.
    let armed = arm_run_read(&mut c, 4, true);
    assert!(
        armed.get("perFrame").is_some(),
        "the ring is present once armed: {armed}"
    );
    c.ok("emulator/get_profiler_frames", json!({"frames": 2}));
}

/// The per-frame ring is **opt-in and absent otherwise**, undivided, and bounded by the same policy cap it
/// advertises. Its absence is the signal: a ring of zeroes would be indistinguishable from a sample with
/// no frames in it.
#[test]
fn the_per_frame_ring_is_opt_in_undivided_and_bounded() {
    let (_h, mut c, _init) = booted(
        "prof-ring",
        oracle_core::testrom::ProfilerShape::Interrupts {
            hint: false,
            vint: true,
        },
    );

    let without = arm_run_read(&mut c, 5, false);
    assert!(
        without.get("perFrame").is_none(),
        "absent unless armed: {without}"
    );

    let with = arm_run_read(&mut c, 5, true);
    let ring = &with["perFrame"];
    let rows = ring["items"].as_array().expect("perFrame.items");
    assert_eq!(
        rows.len() as u64,
        u64_of(&with, "frameCount"),
        "one row per counted frame: {ring}"
    );
    // Undivided: the rows ARE the sample, decomposed.
    let summed: u64 = rows.iter().map(|r| u64_of(r, "cycles")).sum();
    assert_eq!(
        summed,
        u64_of(&with, "sampleCycles"),
        "per-frame rows are undivided and sum to the sample"
    );
    for row in rows {
        assert!(
            u64_of(row, "vintCycles") > 0,
            "this fixture takes one VBlank a frame: {row}"
        );
        assert_eq!(u64_of(row, "hintCycles"), 0, "and no HBlank: {row}");
    }
    // Bounded by `frames`, refused-not-clipped above the cap.
    let two = c.ok("emulator/get_profiler_frames", json!({"frames": 2}));
    assert_eq!(two["perFrame"]["returned"], json!(2));
    assert_eq!(two["perFrame"]["truncated"], json!(true));
    let cap = _init["limits"]["maxProfilerFrames"]
        .as_u64()
        .expect("the cap is advertised");
    let e = c.err("emulator/get_profiler_frames", json!({"frames": cap + 1}));
    assert_eq!(
        e["code"],
        json!(-32602),
        "refused above the ring depth: {e}"
    );
}

/// `budgetPct` is derived from the machine's own `timingBasis`, and exactly one of the pair is present.
/// A hardcoded NTSC constant is wrong by ~16% the moment the machine is PAL, so the derivation is checked
/// against the basis the same handshake advertised rather than against a number written here.
#[test]
fn budget_pct_is_derived_from_the_advertised_timing_basis() {
    let h = spawn_with(
        "prof-budget",
        oracle_core::testrom::build_profiler(default_shape()),
        64,
    );
    let mut c = Client::connect(&h);
    let init = c.handshake(false);
    let mclk_per_frame = init["timingBasis"]["mclkPerFrame"]
        .as_u64()
        .expect("the basis is advertised");

    let r = arm_run_read(&mut c, 6, false);
    assert!(
        r.get("budgetPctOmitted").is_none(),
        "the basis did not change, so the figure is derivable: {r}"
    );
    let pct = r["budgetPct"].as_f64().expect("budgetPct is a number");

    // The contract's own derivation: mclkPerFrame over the master clocks per CPU cycle.
    let cycles_per_frame = mclk_per_frame / 7;
    let expected = u64_of(&r, "totalCycles") as f64 * 100.0 / cycles_per_frame as f64;
    assert!(
        (pct - expected).abs() < 1e-6,
        "budgetPct must come from the advertised basis: {pct} vs {expected}"
    );
    // A machine whose CPU is never halted sits near 100 — the sanity check the contract describes.
    assert!(
        (50.0..=150.0).contains(&pct),
        "a plausible share of one frame's budget, not a percentage of something else: {pct}"
    );
}

// --- rows ----------------------------------------------------------------------------------------------

/// A row is keyed by a canonical 24-bit entry address, and `name`/`disp` are **absent together** when no
/// symbols are loaded — a server must not fall back to the address string, because a client cannot tell a
/// symbol spelled like an address from a lookup that failed.
#[test]
fn rows_are_canonical_addresses_and_name_and_disp_travel_together() {
    let (_h, mut c, _init) = booted("prof-rows", default_shape());
    let r = arm_run_read(&mut c, 6, false);
    let rows = r["routines"]["items"].as_array().expect("items");
    assert!(!rows.is_empty(), "need rows: {r}");

    for row in rows {
        let addr = row["addr"].as_str().expect("addr is a hex string");
        assert!(
            addr.starts_with("0x") && u32::from_str_radix(&addr[2..], 16).is_ok(),
            "addr is D9 category 1 hex: {addr}"
        );
        let n = u32::from_str_radix(&addr[2..], 16).unwrap();
        assert_eq!(
            n & 0xFF00_0000,
            0,
            "masked to the 24 address lines the 68000 drives: {addr}"
        );
        assert_eq!(
            row.get("name").is_some(),
            row.get("disp").is_some(),
            "name and disp are absent together, present together: {row}"
        );
        // No symbols are loaded on this server, so neither may appear at all.
        assert!(
            row.get("name").is_none(),
            "nothing resolved, so no name is invented: {row}"
        );
        // Ordered most-expensive-first.
        assert!(
            u64_of(row, "cyclesSelf") <= u64_of(row, "cycles"),
            "self is part of inclusive: {row}"
        );
        assert!(
            u64_of(row, "stallCycles") <= u64_of(row, "cycles"),
            "stall is a SUBSET of cycles, not a quantity beside them: {row}"
        );
    }
    let cycles: Vec<u64> = rows.iter().map(|r| u64_of(r, "cycles")).collect();
    let mut sorted = cycles.clone();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(
        cycles, sorted,
        "ordered by cycles descending, so a truncated list is the expensive end"
    );
}

// --- determinism -----------------------------------------------------------------------------------------

/// **Three boots, byte-identical.** Aeon's spread-0 bar expressed as this suite's gate: the consumer this
/// surface exists for compares figures with `==` across runs, so anything that varies boot to boot — a
/// `HashMap` iteration order, a wall clock, an uninitialised accumulator — makes the instrument useless to
/// them however accurate a single reading is.
#[test]
fn three_boots_are_byte_identical() {
    let mut replies: Vec<String> = Vec::new();
    for i in 0..3 {
        let (_h, mut c, _init) = booted(&format!("prof-det-{i}"), default_shape());
        let r = arm_run_read(&mut c, 6, true);
        assert!(
            u64_of(&r, "sampleCycles") > 0,
            "boot {i} must have measured something"
        );
        replies.push(serde_json::to_string(&r).expect("serialise"));
    }
    for (i, r) in replies.iter().enumerate().skip(1) {
        assert_eq!(
            r, &replies[0],
            "boot {i}'s profiler reply differs from boot 0's — the instrument is not deterministic"
        );
    }
}
