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
    // `maxProfilerCallers` rides with the other two, and its PRESENCE is doing more work than theirs: it
    // is the caller lens's capability signal (§11.18), so a client branches on it rather than on a version
    // integer. A server without the lens omits it and refuses `set_profiler{callers:true}` by name.
    for k in [
        "maxProfilerRoutines",
        "maxProfilerFrames",
        "maxProfilerCallers",
    ] {
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
    // **The window is the most-recent TAIL, and the wire says which frames those are.** Asserted against
    // the full reply's own rows rather than against a computed number: a `take(frames)` in place of the
    // `skip(len - frames)` would serve the OLDEST two and pass every count assertion above and below it.
    let full_frames: Vec<u64> = rows.iter().map(|r| u64_of(r, "frame")).collect();
    let two_frames: Vec<u64> = c.ok("emulator/get_profiler_frames", json!({"frames": 2}))
        ["perFrame"]["items"]
        .as_array()
        .expect("perFrame.items")
        .iter()
        .map(|r| u64_of(r, "frame"))
        .collect();
    assert_eq!(
        two_frames,
        full_frames[full_frames.len() - 2..].to_vec(),
        "a bounded window is the LAST rows of the ring, not the first: full {full_frames:?}"
    );

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

/// **A sample longer than the ring says so.** The ring is bounded by `limits.maxProfilerFrames`, so a
/// longer sample has already dropped its oldest rows — and `total` taken from the ring's own occupancy
/// would then equal `returned` and report `truncated: false` on a reply missing most of the sample. The
/// honest denominator is the sample's frame count, which is what makes the pair answer §2.4 clause (a)'s
/// question: *does the client have everything?*
#[test]
fn a_sample_longer_than_the_ring_reports_the_frames_it_dropped() {
    let (_h, mut c, init) = booted("prof-ring-overflow", default_shape());
    let cap = init["limits"]["maxProfilerFrames"]
        .as_u64()
        .expect("the cap is advertised");

    let r = arm_run_read(&mut c, cap + 5, true);
    let n = u64_of(&r, "frameCount");
    assert!(n > cap, "the sample must outrun the ring: {n} vs {cap}");

    let ring = &r["perFrame"];
    assert_eq!(
        ring["total"].as_u64(),
        Some(n),
        "`total` is the sample's frames, not the ring's occupancy: {ring}"
    );
    assert_eq!(
        ring["returned"].as_u64(),
        Some(cap),
        "the ring held its bound and no more: {ring}"
    );
    assert_eq!(
        ring["truncated"],
        json!(true),
        "and the reply says the client does NOT have everything: {ring}"
    );
    // The rows it kept are the most recent ones — the same tail rule the bounded window follows.
    let items = ring["items"].as_array().expect("perFrame.items");
    let last = u64_of(items.last().expect("a row"), "frame");
    let first = u64_of(&items[0], "frame");
    assert_eq!(
        last - first + 1,
        cap,
        "the kept rows are one contiguous run of the ring's depth: {first}..={last}"
    );
}

/// **A timeline jump drops the sample and keeps the arming** (the N4 ruling).
///
/// `restore` is the case that proves the rule rather than merely illustrating it: the checkpoint's machine
/// has its own stack, so an in-flight shadow stack would be waiting for returns that can never come and
/// would mis-attribute the restored machine's returns to the old machine's frames. The measurement
/// therefore restarts — while `enabled`/`perFrame`, which are the *client's instruction* and not the
/// machine's state, survive, because a client that rewinds to a checkpoint wants to measure what happens
/// next and has no way to predict a silent disarm.
#[test]
fn a_restore_restarts_the_sample_and_keeps_the_arming() {
    let (_h, mut c, _init) = booted("prof-restore", default_shape());
    let cp = c.ok("emulator/checkpoint", json!({}));
    let id = cp["id"].as_str().expect("an id").to_string();

    let before = arm_run_read(&mut c, 6, true);
    assert!(
        u64_of(&before, "frameCount") > 0 && u64_of(&before, "sampleCycles") > 0,
        "there is a real sample to lose: {before}"
    );

    c.ok("emulator/restore", json!({"id": id}));

    let state = c.ok("emulator/get_profiler", json!({}));
    assert_eq!(
        state["enabled"],
        json!(true),
        "the arming is the client's instruction and survives the jump: {state}"
    );
    assert_eq!(
        state["perFrame"],
        json!(true),
        "including the ring, so the next sample has the shape that was asked for: {state}"
    );
    assert_eq!(
        u64_of(&state, "framesRecorded"),
        0,
        "but the measurement restarts: {state}"
    );

    let after = c.ok("emulator/get_profiler_frames", json!({}));
    assert_eq!(u64_of(&after, "frameCount"), 0);
    assert_eq!(u64_of(&after, "sampleCycles"), 0);
    assert_eq!(
        after["routines"]["total"],
        json!(0),
        "and no rows survive it"
    );
    assert!(
        after.get("perFrame").is_some(),
        "the ring is still armed, and therefore still present: {after}"
    );

    // And it measures again from the new timeline, so the reset is a restart rather than a stop.
    c.ok("emulator/run_frames", json!({"frames": 4}));
    let again = c.ok("emulator/get_profiler_frames", json!({}));
    assert!(
        u64_of(&again, "frameCount") > 0 && u64_of(&again, "sampleCycles") > 0,
        "the restored timeline is being measured: {again}"
    );
}

/// The same rule for the other two timeline jumps, because "replaces the machine" is the property that
/// matters and `reset` has it too — it is not a run and the sample it would keep is the previous
/// machine's.
#[test]
fn a_reset_restarts_the_sample_and_keeps_the_arming() {
    let (_h, mut c, _init) = booted("prof-reset-jump", default_shape());
    let before = arm_run_read(&mut c, 5, false);
    assert!(
        u64_of(&before, "frameCount") > 0,
        "a sample to lose: {before}"
    );

    c.ok("emulator/reset", json!({}));

    let state = c.ok("emulator/get_profiler", json!({}));
    assert_eq!(state["enabled"], json!(true), "still armed: {state}");
    assert_eq!(u64_of(&state, "framesRecorded"), 0, "restarted: {state}");
    let after = c.ok("emulator/get_profiler_frames", json!({}));
    assert_eq!(u64_of(&after, "sampleCycles"), 0, "{after}");

    c.ok("emulator/run_frames", json!({"frames": 3}));
    assert!(
        u64_of(
            &c.ok("emulator/get_profiler_frames", json!({})),
            "sampleCycles"
        ) > 0,
        "and the machine after the reset is measured"
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

/// **`name`/`disp` on the wire, from a real listing.** Every other test here boots symbol-less, which
/// pins only the absent half of "present together, absent together" — and a rule with one half tested is a
/// rule half tested.
///
/// The listing names the fixture's leaf at its exact entry address and puts the entry point a few bytes
/// **below** the main routine, so one row resolves at displacement 0 and the other at a real non-zero
/// offset — which is what makes `disp` a computed figure here rather than a constant that happens to be
/// right. Both halves are asserted:
/// the name is the **bare label** (§4's rule — `$defs/symbolName` refuses a `+$hex` composite by pattern,
/// so a composite would fail validation on the way in rather than reach this assertion), and `disp` is an
/// integer beside it rather than baked into the string.
#[test]
fn a_row_carries_its_bare_label_and_an_integer_displacement() {
    let (_h, mut c, _init) = booted("prof-symbols", default_shape());

    // Written per-test rather than shared: the path carries the tag, so two tests cannot race on one file.
    let lst = std::env::temp_dir().join(format!(
        "ae-prof-symbols-{}-{:?}.lst",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(
        &lst,
        "\
  Symbol Table (* = unused):
  --------------------------

 EntryPoint : 1FC C |
 Prof_Leaf : 300 C |

    2 symbols
    0 unused symbols
",
    )
    .expect("write the listing");
    c.ok(
        "emulator/load_symbols",
        json!({"path": lst.to_str().expect("utf-8")}),
    );

    let r = arm_run_read(&mut c, 6, false);
    let rows = r["routines"]["items"].as_array().expect("routines.items");
    assert!(!rows.is_empty(), "need rows: {r}");

    let mut named = 0;
    for row in rows {
        assert_eq!(
            row.get("name").is_some(),
            row.get("disp").is_some(),
            "name and disp travel together: {row}"
        );
        let Some(name) = row.get("name").and_then(Value::as_str) else {
            continue;
        };
        named += 1;
        assert!(
            ["EntryPoint", "Prof_Leaf"].contains(&name),
            "the label comes from the listing, not from the address: {row}"
        );
        assert!(
            !name.contains('+') && !name.contains('$'),
            "the BARE label — a `name+$hex` composite is a different field's job: {row}"
        );
        assert!(
            row["disp"].is_u64(),
            "`disp` is an integer beside the name, not baked into it: {row}"
        );
    }
    assert!(named >= 2, "both listed labels should have resolved: {r}");

    // The leaf's row is keyed by its exact entry address, so it resolves at displacement 0 — the case that
    // proves the displacement is computed rather than defaulted.
    let leaf = rows
        .iter()
        .find(|r| r["addr"] == json!("0x00000300"))
        .unwrap_or_else(|| panic!("the fixture's leaf is a row: {r}"));
    assert_eq!(leaf["name"], json!("Prof_Leaf"), "{leaf}");
    assert_eq!(leaf["disp"], json!(0), "exactly on the label: {leaf}");

    // …and the main-loop row sits inside `EntryPoint`, so its displacement is real and non-zero.
    let inside = rows
        .iter()
        .find(|r| r["name"] == json!("EntryPoint"))
        .unwrap_or_else(|| panic!("the main loop resolves to the entry point: {r}"));
    assert!(
        u64_of(inside, "disp") > 0,
        "a row inside a routine carries the offset it is at: {inside}"
    );

    let _ = std::fs::remove_file(&lst);
}

// --- the small witnesses ---------------------------------------------------------------------------------

/// **`get_profiler.routineCount` is the number `get_profiler_frames` puts in `routines.total`.** Two counts
/// of one thing is how the legacy surface got its divisor wrong, and §11.16 already pins the frame count
/// this way; the row count is the other pair and it costs one assertion to keep honest.
#[test]
fn the_two_row_counts_are_one_number() {
    let (_h, mut c, _init) = booted("prof-rowcount", default_shape());
    let sample = arm_run_read(&mut c, 6, false);
    let state = c.ok("emulator/get_profiler", json!({}));
    assert_eq!(
        state["routineCount"], sample["routines"]["total"],
        "the instrument's row count and the sample's are the same number: {state} vs {}",
        sample["routines"]
    );
    assert!(
        u64_of(&state, "routineCount") > 0,
        "and it is not vacuously zero: {state}"
    );

    // `total` is the sample's, not the page's: bounding the list must not move it.
    let one = c.ok("emulator/get_profiler_frames", json!({"top": 1}));
    assert_eq!(
        one["routines"]["total"], sample["routines"]["total"],
        "a bounded page still reports how many rows the sample has"
    );
}

/// **A clean sample carries no `caveat`** — §2.4's advisory, on the wire, in the direction the fixtures can
/// reach. The other direction (a caveat that appears, naming only the counter that fired) is unreachable
/// from any fixture ROM — both counters are stack-recovery events, and probing all eight `ProfilerShape`s
/// including 30,000-deep recursion produced zero of each — so it is pinned as a unit test on the sentence
/// itself, `engine::tests::the_caveat_appears_only_when_there_is_something_to_say_and_names_only_that`.
#[test]
fn a_clean_sample_carries_no_caveat() {
    let (_h, mut c, _init) = booted("prof-clean", default_shape());
    let r = arm_run_read(&mut c, 6, true);
    assert_eq!(
        u64_of(&r, "abandonedFrames"),
        0,
        "the fixture is clean: {r}"
    );
    assert_eq!(u64_of(&r, "depthExceeded"), 0, "…in both counters: {r}");
    assert!(
        r.get("caveat").is_none(),
        "so there is nothing to say, and nothing is said: {r}"
    );
}

/// **A server nobody has armed answers both reads**, and answers them with the degenerate state rather than
/// an error or a silence. This is the first thing any client does, and it is the one call sequence no other
/// test here performs: every other test arms first.
#[test]
fn a_never_armed_server_answers_both_reads() {
    let (_h, mut c, _init) = booted("prof-fresh", default_shape());

    let state = c.ok("emulator/get_profiler", json!({}));
    assert_eq!(state["enabled"], json!(false), "nothing armed it: {state}");
    assert_eq!(state["perFrame"], json!(false), "{state}");
    assert_eq!(u64_of(&state, "framesRecorded"), 0, "{state}");
    assert_eq!(u64_of(&state, "routineCount"), 0, "{state}");

    let r = c.ok("emulator/get_profiler_frames", json!({}));
    assert_eq!(u64_of(&r, "frameCount"), 0);
    assert_eq!(u64_of(&r, "sampleCycles"), 0);
    assert_eq!(
        r["perFrameExact"],
        json!(true),
        "nothing was divided, so nothing was lost: {r}"
    );
    assert_eq!(r["routines"]["total"], json!(0), "{r}");
    assert!(
        r.get("perFrame").is_none(),
        "the ring was never armed, so it is absent rather than empty: {r}"
    );
    // And running the machine changes none of it: an unarmed instrument records nothing.
    c.ok("emulator/run_frames", json!({"frames": 3}));
    assert_eq!(
        u64_of(
            &c.ok("emulator/get_profiler_frames", json!({})),
            "frameCount"
        ),
        0,
        "a disarmed profiler is not a quietly-armed one"
    );
}

// --- the caller lens (§11.18 / CR-28) --------------------------------------------------------------------

/// Arm with the lens, run, read.
fn arm_run_read_callers(c: &mut Client, frames: u64) -> Value {
    c.ok(
        "emulator/set_profiler",
        json!({"enabled": true, "callers": true}),
    );
    c.ok("emulator/run_frames", json!({"frames": frames}));
    c.ok("emulator/get_profiler_frames", json!({}))
}

/// The four row keys the lens adds, which arrive **as a set** or not at all.
const CALLER_KEYS: [&str; 4] = [
    "callers",
    "callersTotal",
    "callersReturned",
    "callersTruncated",
];

/// A fixture whose VBlank handler is reached from **both** interrupt causes, so at least one row has more
/// than one edge — without which every "sum of edges" assertion below would be a sum of one term.
fn two_cause_shape() -> oracle_core::testrom::ProfilerShape {
    oracle_core::testrom::ProfilerShape::Interrupts {
        hint: true,
        vint: true,
    }
}

fn envelope(result: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "result": result})
}

/// **Arming, echoing, and the one refusal a lens-implementing server may NOT make.**
///
/// The echo is capability-conditional rather than REQUIRED, which is the shape §11.16's expired
/// pre-release licence forced: this server advertises `limits.maxProfilerCallers`, so it MUST carry the
/// echo, and a server that does not advertise the cap MUST omit it — absence therefore means *no caller
/// lens on this server* and never *the lens is off*. Adoption clause 1's third bound is the last assertion
/// here: `set_profiler{callers: false}` is **accepted**, not refused, on a server that has the lens.
#[test]
fn the_lens_arms_and_is_echoed_on_both_state_replies() {
    let (_h, mut c, _init) = booted("prof-callers-arm", default_shape());

    // Off unless asked, on both replies.
    let plain = c.ok("emulator/set_profiler", json!({"enabled": true}));
    assert_eq!(
        plain["callers"],
        json!(false),
        "the lens exists and is off — `false`, not an absence: {plain}"
    );
    assert_eq!(
        c.ok("emulator/get_profiler", json!({}))["callers"],
        json!(false),
        "and the state read agrees"
    );

    let armed = c.ok(
        "emulator/set_profiler",
        json!({"enabled": true, "callers": true}),
    );
    assert_eq!(armed["callers"], json!(true), "{armed}");
    assert_eq!(
        armed["perFrame"],
        json!(false),
        "one lens, not both: {armed}"
    );
    let state = c.ok("emulator/get_profiler", json!({}));
    assert_eq!(
        state["callers"],
        json!(true),
        "the third arming fact, on the state read: {state}"
    );

    // **Every arming flag resets together.** A second arm starts a fresh sample under exactly the lenses
    // that call names, so arming without `callers` turns the lens off rather than leaving it running.
    c.ok("emulator/run_frames", json!({"frames": 4}));
    let re = c.ok("emulator/set_profiler", json!({"enabled": true}));
    assert_eq!(
        re["callers"],
        json!(false),
        "a second arm discards the in-flight sample AND its lenses: {re}"
    );

    // Adoption clause 1: `callers: false` is ACCEPTED on a server that implements the lens. It is only an
    // undeclared key — and only then a -32602 — on a server that does not.
    let off = c.ok(
        "emulator/set_profiler",
        json!({"enabled": true, "callers": false}),
    );
    assert_eq!(off["callers"], json!(false), "accepted, not refused: {off}");

    // Disarming RETAINS, and the echo keeps describing the sample that is still readable.
    c.ok(
        "emulator/set_profiler",
        json!({"enabled": true, "callers": true}),
    );
    c.ok("emulator/run_frames", json!({"frames": 4}));
    let disarmed = c.ok("emulator/set_profiler", json!({"enabled": false}));
    assert_eq!(
        disarmed["callers"],
        json!(true),
        "the retained sample still has caller data, so the echo still says so: {disarmed}"
    );
    let after = c.ok("emulator/get_profiler_frames", json!({}));
    assert!(
        after["routines"]["items"][0].get("callers").is_some(),
        "…and reading it after the disarm still serves the edges: {after}"
    );
}

/// **THE CENTRAL CLAIM: a reply nobody armed the lens for is BYTE-IDENTICAL to a never-armed server's.**
///
/// The adoption condition's clause 5, and the one an always-on accumulator would break first. Two servers,
/// same fixture, same frame count, armed the same way except that one names `callers: false` explicitly
/// and the other has never heard of the key — serialised and compared as **bytes**, not field by field,
/// because a field-by-field comparison only checks the fields somebody thought to list.
///
/// Beneath it, the direct half: no row carries any of the four keys, asserted against the pre-amendment
/// row key set spelled out here rather than against "no key starting with callers".
#[test]
fn an_unarmed_reply_is_byte_identical_to_a_never_armed_servers() {
    let sample = |tag: &str, arm: Value| -> Value {
        let (_h, mut c, _init) = booted(tag, default_shape());
        c.ok("emulator/set_profiler", arm);
        c.ok("emulator/run_frames", json!({"frames": 6}));
        c.ok("emulator/get_profiler_frames", json!({}))
    };
    let never = sample("prof-bytes-never", json!({"enabled": true}));
    let explicit = sample(
        "prof-bytes-false",
        json!({"enabled": true, "callers": false}),
    );

    assert!(
        u64_of(&never, "sampleCycles") > 0
            && !never["routines"]["items"]
                .as_array()
                .expect("items")
                .is_empty(),
        "the comparison must be over a reply with content: {never}"
    );
    assert_eq!(
        serde_json::to_string(&never).expect("serialise"),
        serde_json::to_string(&explicit).expect("serialise"),
        "naming the lens and declining it must produce the same bytes as never naming it"
    );

    // The pre-amendment row shape, spelled out. A key added to a row later shows up here as a failure
    // rather than as a silent widening of "everything else".
    const PRE_AMENDMENT_ROW_KEYS: [&str; 9] = [
        "addr",
        "cycles",
        "cyclesSelf",
        "stallCycles",
        "calls",
        "cyclesTotal",
        "cyclesSelfTotal",
        "stallCyclesTotal",
        "callsTotal",
    ];
    for row in never["routines"]["items"].as_array().expect("items") {
        let keys: Vec<&str> = row
            .as_object()
            .expect("a row is an object")
            .keys()
            .map(String::as_str)
            .collect();
        for k in &keys {
            assert!(
                PRE_AMENDMENT_ROW_KEYS.contains(k) || ["name", "disp"].contains(k),
                "an unarmed row carries only what it carried before the lens: {k} in {row}"
            );
        }
    }
}

/// **The two normative sums, with `==`, guarded by `callersTruncated: false`.**
///
/// Every invocation has exactly one caller, so a row's edges *partition* it: their `callsTotal` sum to the
/// row's `callsTotal` and their `cyclesSelfTotal` to the row's `cyclesSelfTotal`. Both sides undivided,
/// which is the whole reason the edge carries undivided partners — a divided sum falls short by up to one
/// unit per edge and reads exactly like agreement (§11.16's *quiet gap*, one level down).
///
/// The inclusive figure is deliberately **not** summed: it double-counts by construction, which is why the
/// contract states the cycle sum on self.
///
/// Anti-vacuity is explicit. A reply whose every row has exactly one edge would satisfy both sums without
/// testing anything, so the fixture is the two-cause one and the test asserts that at least one row really
/// carried more than one edge.
#[test]
fn a_rows_edges_sum_to_it_exactly_in_both_normative_figures() {
    let mut multi_edge_rows = 0;
    for (tag, shape) in [
        ("prof-sums-two-cause", two_cause_shape()),
        ("prof-sums-leaf", default_shape()),
        (
            "prof-sums-tree",
            oracle_core::testrom::ProfilerShape::TwoLevel,
        ),
    ] {
        let (_h, mut c, _init) = booted(tag, shape);
        let r = arm_run_read_callers(&mut c, 6);
        let rows = r["routines"]["items"].as_array().expect("routines.items");
        assert!(!rows.is_empty(), "{tag}: need rows: {r}");

        for row in rows {
            for k in CALLER_KEYS {
                assert!(
                    row.get(k).is_some(),
                    "{tag}: the four keys arrive as a SET on an armed row — {k} missing: {row}"
                );
            }
            let edges = row["callers"].as_array().expect("callers is an array");
            assert_eq!(
                u64_of(row, "callersReturned"),
                edges.len() as u64,
                "{tag}: `callersReturned` restates the array's length: {row}"
            );
            assert!(
                !edges.is_empty(),
                "{tag}: every armed row acquires at least one edge — an empty list is a defect in the \
                 accountant, not an ordinary answer: {row}"
            );
            if edges.len() > 1 {
                multi_edge_rows += 1;
            }
            assert_eq!(
                row["callersTruncated"],
                json!(false),
                "{tag}: this fixture must fit under the cap or the sums below are only bounds: {row}"
            );
            for key in ["callsTotal", "cyclesSelfTotal"] {
                assert_eq!(
                    edges.iter().map(|e| u64_of(e, key)).sum::<u64>(),
                    u64_of(row, key),
                    "{tag}: the edges' {key} sum EXACTLY to the row's — undivided on both sides, so \
                     this is an == and not a bound: {row}"
                );
            }
            // Ordered by `cycles` descending, one level down from the rows' own rule.
            let cycles: Vec<u64> = edges.iter().map(|e| u64_of(e, "cycles")).collect();
            let mut sorted = cycles.clone();
            sorted.sort_unstable_by(|a, b| b.cmp(a));
            assert_eq!(
                cycles, sorted,
                "{tag}: a bounded edge list must be the expensive end: {row}"
            );
        }
    }
    assert!(
        multi_edge_rows > 0,
        "at least one row must have had MORE THAN ONE edge, or both sums are sums of one term"
    );
}

/// **An interrupt-entered edge is keyed by CAUSE, on the wire, and the two causes stay apart.**
///
/// The fixture points *both* vectors at one handler, which is exactly when an accountant keying by handler
/// address cannot tell the causes apart at all — so one row, two edges, `hint` and `vint`. That is the
/// property the four-value enum exists for and the one a single collapsing `interrupt` value could not
/// express: these two spellings **join the edge to the bucket it came from by name**.
///
/// The demand side's own acceptance is the last assertion: an interrupt-entered edge is distinguishable
/// from a mainline one without asking us — a two-value membership test, which is what the split cost them
/// in place of one equality.
#[test]
fn interrupt_entered_edges_are_keyed_hint_and_vint_distinctly() {
    let (_h, mut c, _init) = booted("prof-callers-cause", two_cause_shape());
    let r = arm_run_read_callers(&mut c, 8);
    let rows = r["routines"]["items"].as_array().expect("routines.items");

    let kinds: Vec<String> = rows
        .iter()
        .flat_map(|row| row["callers"].as_array().expect("callers"))
        .filter_map(|e| e.get("entryKind").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    assert!(
        kinds.iter().any(|k| k == "hint") && kinds.iter().any(|k| k == "vint"),
        "one handler address, two acknowledged causes, two distinctly keyed edges: {kinds:?} in {r}"
    );

    // …and a MAINLINE edge to be distinguishable from, which is the branch this lens was built for. It
    // needs a second fixture: the two-cause shape has no body at all, which is exactly what makes its
    // bucket assertions unambiguous. The membership test the demand side is left with — `entryKind` in
    // {hint, vint} — is the one they named as their acceptance, and it runs without asking us anything.
    let (_h2, mut c2, _i2) = booted("prof-callers-mainline", default_shape());
    let m = arm_run_read_callers(&mut c2, 6);
    let mainline: Vec<&Value> = m["routines"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .flat_map(|row| row["callers"].as_array().expect("callers"))
        .filter(|e| e.get("callerAddr").is_some())
        .collect();
    assert!(
        !mainline.is_empty(),
        "a mainline edge names its caller and carries no kind: {m}"
    );
    for e in &mainline {
        assert!(
            e.get("entryKind").is_none(),
            "…and is therefore NOT in {{hint, vint}} by a plain membership test: {e}"
        );
    }
    // The bucket a `hint`/`vint` edge names is on the same reply, by the same word — which is the join a
    // collapsing value could not offer.
    for cause in ["hint", "vint"] {
        assert!(
            r["interrupts"][cause].is_object(),
            "the edge's kind names a bucket that is right there: {}",
            r["interrupts"]
        );
    }
}

/// **`entryKind` is REQUIRED exactly when `callerAddr` is absent, and FORBIDDEN when it is present.**
///
/// A biconditional the fragment enforces with `if`/`then`/`else`, so it is proven the way a closure has to
/// be proven — by messages that **fail**. Both directions, doctored out of a real reply that passed on the
/// way in, because a closure nobody has watched reject is a closure nobody has tested.
#[test]
fn the_entry_kind_biconditional_is_enforced_in_both_directions() {
    // `CallsLeaf` is the one fixture that produces BOTH shapes in one reply: the leaf's edge names its
    // caller, and the row that IS the frame the sample opened on carries `entryKind: "root"` with no
    // address. Doctoring one reply rather than two keeps the control (it passed on the way in) shared.
    let (_h, mut c, _init) = booted("prof-callers-bicond", default_shape());
    let r = arm_run_read_callers(&mut c, 8);
    common::schema::check_incoming(&envelope(&r), Some("emulator/get_profiler_frames"))
        .expect("the undoctored reply is conformant");

    // Find one edge of each kind to doctor — asserted rather than assumed, so a reply that stopped
    // producing one of them fails here instead of silently skipping half the test.
    let (mut with_addr, mut with_kind) = (None, None);
    for (i, row) in r["routines"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .enumerate()
    {
        for (j, e) in row["callers"]
            .as_array()
            .expect("callers")
            .iter()
            .enumerate()
        {
            if e.get("callerAddr").is_some() {
                with_addr.get_or_insert((i, j));
            } else {
                with_kind.get_or_insert((i, j));
            }
        }
    }
    let (ai, aj) = with_addr.expect("a mainline edge to doctor");
    let (ki, kj) = with_kind.expect("an interrupt-entered edge to doctor");

    // Direction 1: an edge that names its caller may NOT also carry a kind.
    let mut both = r.clone();
    both["routines"]["items"][ai]["callers"][aj]["entryKind"] = json!("root");
    let failures =
        common::schema::check_incoming(&envelope(&both), Some("emulator/get_profiler_frames"))
            .expect_err("an edge carrying BOTH callerAddr and entryKind must be refused");
    assert!(
        !failures.is_empty(),
        "the refusal must say something: {failures:?}"
    );

    // Direction 2: an edge with no caller address may NOT omit the kind — that is the absence meaning
    // three things again, which is the defect this key was added to end.
    let mut neither = r.clone();
    neither["routines"]["items"][ki]["callers"][kj]
        .as_object_mut()
        .expect("an edge is an object")
        .remove("entryKind");
    common::schema::check_incoming(&envelope(&neither), Some("emulator/get_profiler_frames"))
        .expect_err("an edge with neither callerAddr nor entryKind must be refused");
}

/// **Each of the four enum values is accepted on its own, and the collapsing spelling is refused.**
///
/// `"interrupt"` is the value this amendment **declined**, and it is the one a later drafter is likeliest
/// to reintroduce — so it is pinned as a refusal rather than left to prose. `depthCap` is checked here and
/// only here: the accumulator cannot reach it (a push is refused only *at* the depth cap, and the only way
/// back below the cap is a pop, after which the frame on top is one that really was tracked — see
/// `oracle_core::profiler::Profiler`'s `depth_capped`), so this is the one place its wire spelling can be
/// exercised at all.
#[test]
fn each_entry_kind_is_accepted_and_the_collapsing_spelling_is_not() {
    let (_h, mut c, _init) = booted("prof-callers-enum", two_cause_shape());
    let r = arm_run_read_callers(&mut c, 8);

    // An interrupt-entered edge — the one shape that legally carries a kind and no address.
    let (ri, ei) = r["routines"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .enumerate()
        .find_map(|(i, row)| {
            row["callers"]
                .as_array()
                .expect("callers")
                .iter()
                .position(|e| e.get("entryKind").is_some())
                .map(|j| (i, j))
        })
        .expect("an edge with an entryKind to re-spell");

    for kind in ["hint", "vint", "root", "depthCap"] {
        let mut doctored = r.clone();
        doctored["routines"]["items"][ri]["callers"][ei]["entryKind"] = json!(kind);
        common::schema::check_incoming(&envelope(&doctored), Some("emulator/get_profiler_frames"))
            .unwrap_or_else(|f| panic!("{kind} is one of the four this contract ships: {f:?}"));
    }
    let mut collapsed = r.clone();
    collapsed["routines"]["items"][ri]["callers"][ei]["entryKind"] = json!("interrupt");
    common::schema::check_incoming(&envelope(&collapsed), Some("emulator/get_profiler_frames"))
        .expect_err(
            "`interrupt` is the collapsing spelling this amendment overruled — widening the enum later \
             is not additive, so the value a ruling removed must be refused now",
        );
}

/// **The four `callers*` keys arrive as a SET, and a half-served lens is refused.**
///
/// Adoption clause 2, proven in both directions by messages that fail: a row carrying `callers` without
/// `callersTruncated`, a row carrying `callersTotal` alone, and — the direction that matters for an
/// unarmed reply — **each** of the four doctored one at a time onto a reply that was never armed for the
/// lens. `dependentRequired` ties all four together, so any one of them alone is a refusal.
#[test]
fn the_arming_set_is_all_four_keys_or_none() {
    let (_h, mut c, _init) = booted("prof-callers-set", default_shape());

    // Armed: dropping one of the four out of a passing reply must be refused.
    let armed = arm_run_read_callers(&mut c, 6);
    common::schema::check_incoming(&envelope(&armed), Some("emulator/get_profiler_frames"))
        .expect("the undoctored armed reply is conformant");
    for drop in CALLER_KEYS {
        let mut doctored = armed.clone();
        doctored["routines"]["items"][0]
            .as_object_mut()
            .expect("a row is an object")
            .remove(drop);
        let failures = common::schema::check_incoming(
            &envelope(&doctored),
            Some("emulator/get_profiler_frames"),
        )
        .expect_err(&format!(
            "a row serving the lens without {drop} is a half-served lens and must be refused"
        ));
        assert!(
            failures.iter().any(|m| m.contains(drop)),
            "the refusal must name {drop} rather than failing for an unrelated reason: {failures:?}"
        );
    }

    // Unarmed: adding any ONE of the four to a reply that carries none of them must be refused too.
    c.ok("emulator/set_profiler", json!({"enabled": true}));
    c.ok("emulator/run_frames", json!({"frames": 6}));
    let unarmed = c.ok("emulator/get_profiler_frames", json!({}));
    common::schema::check_incoming(&envelope(&unarmed), Some("emulator/get_profiler_frames"))
        .expect("the unarmed reply is conformant");
    for (key, value) in [
        ("callers", json!([])),
        ("callersTotal", json!(0)),
        ("callersReturned", json!(0)),
        ("callersTruncated", json!(false)),
    ] {
        let mut doctored = unarmed.clone();
        doctored["routines"]["items"][0][key] = value;
        common::schema::check_incoming(
            &envelope(&doctored),
            Some("emulator/get_profiler_frames"),
        )
        .expect_err(&format!(
            "{key} alone on an unarmed row must be refused — absence and a lone key must not both be \
             ways of saying the lens is off"
        ));
    }
}

/// **A per-edge `stallCycles` is refused by the closed fragment.**
///
/// The requesting client *declined* a per-edge stall figure on measured grounds, so its absence is a
/// demand-side decision rather than an oversight — and the fragment records the decision by **barring** the
/// key (`additionalProperties: false` on the edge shape) rather than by merely not declaring it. A
/// negative control, because every other assertion in this file checks that something arrived.
#[test]
fn a_per_edge_stall_figure_is_refused() {
    let (_h, mut c, _init) = booted("prof-callers-nostall", default_shape());
    let r = arm_run_read_callers(&mut c, 6);
    common::schema::check_incoming(&envelope(&r), Some("emulator/get_profiler_frames"))
        .expect("the undoctored reply is conformant");
    assert!(
        r["routines"]["items"][0]["callers"][0]
            .get("stallCycles")
            .is_none(),
        "the server does not emit one: {}",
        r["routines"]["items"][0]
    );

    for key in ["stallCycles", "stallCyclesTotal"] {
        let mut doctored = r.clone();
        doctored["routines"]["items"][0]["callers"][0][key] = json!(0);
        let failures = common::schema::check_incoming(
            &envelope(&doctored),
            Some("emulator/get_profiler_frames"),
        )
        .expect_err(&format!(
            "an edge carrying {key} must be REFUSED, not tolerated"
        ));
        assert!(
            failures.iter().any(|m| m.contains(key)),
            "the refusal must name {key}: {failures:?}"
        );
    }
}

/// **`topCallers` bounds each row's list independently, is refused above the cap, and is refused when the
/// lens is not armed.**
///
/// The refusal pair adoption clause 1 names. Above `limits.maxProfilerCallers` it is `-32602`, **refused
/// and not clamped** — the `top` precedent, which exists because the legacy surface clamped and a caller
/// could not tell a full list from a clipped one. Against a sample not armed for callers it is `-32005`
/// (`callersNotArmed`), the `frames`/`perFrameNotArmed` rule with the lens changed and nothing else: a
/// parameter that cannot affect the answer is worse than one that is rejected.
///
/// And `callersTotal` does **not** move when the list is clipped, which is what makes it the true count of
/// distinct callers rather than the count that survived a ceiling — the cap is a reply bound, not a
/// retention bound.
#[test]
fn top_callers_bounds_refuses_above_the_cap_and_refuses_when_unarmed() {
    let (_h, mut c, init) = booted("prof-topcallers", two_cause_shape());
    let full = arm_run_read_callers(&mut c, 8);

    // The row with more than one edge is the only one a bound can say anything about.
    let (idx, total) = full["routines"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .enumerate()
        .find_map(|(i, row)| {
            let n = u64_of(row, "callersTotal");
            (n > 1).then_some((i, n))
        })
        .unwrap_or_else(|| panic!("need a row with more than one caller to bound: {full}"));

    let one = c.ok("emulator/get_profiler_frames", json!({"topCallers": 1}));
    let row = &one["routines"]["items"][idx];
    assert_eq!(row["callersReturned"], json!(1), "bounded to one: {row}");
    assert_eq!(
        u64_of(row, "callersTotal"),
        total,
        "…and the TOTAL is unmoved: the cap decides how many are sent, not how many were kept: {row}"
    );
    assert_eq!(
        row["callersTruncated"],
        json!(true),
        "and it says so: {row}"
    );
    assert!(
        row["callers"].get("cursor").is_none() && row.get("callersCursor").is_none(),
        "no cursor: this method accepts no continuation param (§2.4 clause b): {row}"
    );
    assert!(
        row.get("callersLimit").is_none(),
        "and no per-row limit: the applied ceiling is one number for the whole reply: {row}"
    );
    // The kept edge is the expensive end.
    assert_eq!(
        row["callers"][0]["cycles"], full["routines"]["items"][idx]["callers"][0]["cycles"],
        "a truncated edge list keeps the most expensive edges"
    );

    // Refused above the cap, never clamped.
    let cap = init["limits"]["maxProfilerCallers"]
        .as_u64()
        .expect("the cap is advertised");
    let e = c.err(
        "emulator/get_profiler_frames",
        json!({"topCallers": cap + 1}),
    );
    assert_eq!(e["code"], json!(-32602), "refused, never clamped: {e}");

    // Refused with a reason when the sample was not armed for callers.
    c.ok("emulator/set_profiler", json!({"enabled": true}));
    c.ok("emulator/run_frames", json!({"frames": 4}));
    let e = c.err("emulator/get_profiler_frames", json!({"topCallers": 2}));
    assert_eq!(e["code"], json!(-32005), "{e}");
    assert_eq!(
        e["data"]["reason"],
        json!("callersNotArmed"),
        "the discriminant a client branches on, not the message: {e}"
    );
    // …and the sample itself still answers. The refusal is about the parameter, not the read.
    c.ok("emulator/get_profiler_frames", json!({}));
}

/// **A caller's name is the bare label and its displacement an integer beside it** — §4's rule, one level
/// down, with the caveat that makes this field different from the row's.
///
/// The edge whose caller is the frame the sample *opened* on is keyed at whatever PC retired first, which
/// is **mid-routine**. Its `callerAddr` is real and is where the call came from, but it is not an entry
/// point — so it resolves at a **non-zero `callerDisp`**, which is the ordinary answer here rather than a
/// failed lookup. A client that assumes every caller address resolves like a row key mis-renders exactly
/// this one edge per sample, which is why the contract states it and why this test pins it.
#[test]
fn a_caller_carries_its_bare_label_and_the_inferred_root_carries_a_displacement() {
    let (_h, mut c, _init) = booted("prof-callers-symbols", default_shape());
    let lst = std::env::temp_dir().join(format!(
        "ae-prof-callers-{}-{:?}.lst",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(
        &lst,
        "\
  Symbol Table (* = unused):
  --------------------------

 EntryPoint : 1FC C |
 Prof_Leaf : 300 C |

    2 symbols
    0 unused symbols
",
    )
    .expect("write the listing");
    c.ok(
        "emulator/load_symbols",
        json!({"path": lst.to_str().expect("utf-8")}),
    );

    let r = arm_run_read_callers(&mut c, 6);
    let leaf = r["routines"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|row| row["addr"] == json!("0x00000300"))
        .unwrap_or_else(|| panic!("the fixture's leaf is a row: {r}"));
    let edge = &leaf["callers"][0];

    assert_eq!(
        edge["callerName"],
        json!("EntryPoint"),
        "the leaf is called from the main loop, named by the listing: {edge}"
    );
    let name = edge["callerName"].as_str().expect("a name");
    assert!(
        !name.contains('+') && !name.contains('$'),
        "the BARE label — the composite is the client's to render: {edge}"
    );
    assert!(
        u64_of(edge, "callerDisp") > 0,
        "the sample opened mid-routine, so its address is real and is NOT an entry point: {edge}"
    );
    assert!(
        edge.get("entryKind").is_none(),
        "it names a caller, so it carries no kind — the biconditional, on the wire: {edge}"
    );

    // The other sense of *root*, on the same reply and not the same fact: the row that IS the opening
    // frame has an edge saying no call into it was ever observed.
    let root_edges: Vec<&Value> = r["routines"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .flat_map(|row| row["callers"].as_array().expect("callers"))
        .filter(|e| e["entryKind"] == json!("root"))
        .collect();
    assert_eq!(
        root_edges.len(),
        1,
        "exactly one frame was opened on rather than called into: {root_edges:?}"
    );
    assert!(
        root_edges[0].get("callerAddr").is_none(),
        "and `root` means there is no address to give, never a fabricated 0x0: {}",
        root_edges[0]
    );

    let _ = std::fs::remove_file(&lst);
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
