//! **The schema harness's own tests** — contract §8 item 15 (D14).
//!
//! Four jobs, and only the first is the obvious one:
//!
//! 1. **Freshness.** The vendored schema is byte-identical to the contract's copy.
//! 2. **Coverage, reported and pinned.** The schema began as a SEED covering a minority of what this
//!    server advertised; it now has a `result` fragment for **every** advertised method, which is what
//!    `UNCOVERED_METHODS` being pinned *empty* says. The document's fragments therefore split exactly two
//!    ways, and both halves are named rather than counted: the ones this server advertises
//!    (`engine::METHODS`, derived) and the ones it does not (`SCHEMATIZED_NOT_ADVERTISED`, enumerated
//!    below). No number appears in this file that is not the length of a list printed beside it — the
//!    counts in the prose here went stale twice while every assertion stayed green, which is the whole
//!    argument for enumerating instead. The pins guard the two silent directions: a newly advertised
//!    method joining the unchecked pile, and a fragment arriving for a method nobody serves.
//! 3. **The divergence registry, reported and kept live.** Shapes where the server and the schema
//!    disagree are registered rather than silenced, each with its CR number, and the report prints
//!    beside the coverage split so nobody reads a green suite as "fully conformant". An entry that stops
//!    firing fails `every_registered_divergence_is_still_live`, so the list cannot rot after a ruling —
//!    which is exactly how the first two entries it held came to be deleted. It holds CR-16 today, found
//!    by turning item 20's closure on for the first time.
//! 4. **Anti-vacuity.** Proof that the validator *rejects*. This repo has twice shipped an assertion that
//!    passed while testing nothing — a volatility test that was a name grep, an assertion that passed with
//!    zero enqueues. A validator that accepts everything is exactly that failure wearing a library's
//!    clothes, and it would be invisible: the suite would be green and the wire unchecked. Since §8 item
//!    20 landed, this carries the closure's own control too: the working keyword is proven to accept a
//!    conformant reply and catch a surplus key, and the obvious-but-wrong one is proven to do neither.

mod common;

use common::schema::{
    check_incoming, check_incoming_strict, divergence_report, schemas, KNOWN_CONTRACT_DIVERGENCES,
};
use oracle_aether::engine::METHODS;
use serde_json::{json, Value};
use std::path::PathBuf;

// ---------------------------------------------------------------------------------------------------
// 1. Freshness
// ---------------------------------------------------------------------------------------------------

/// Where the upstream contract schema might live. `$AETHER_CONTRACT_SCHEMA` wins; otherwise walk up from
/// this crate and probe each ancestor for `empyrean/contract/schema/…`, which finds it both from a normal
/// checkout and from a `.claude/worktrees/…` worktree, whose depth differs.
fn upstream_schema_path() -> Result<PathBuf, Vec<PathBuf>> {
    if let Ok(p) = std::env::var("AETHER_CONTRACT_SCHEMA") {
        let p = PathBuf::from(p);
        return if p.is_file() { Ok(p) } else { Err(vec![p]) };
    }
    let mut tried = Vec::new();
    let mut dir: Option<&std::path::Path> = Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
    while let Some(d) = dir {
        let cand = d.join("empyrean/contract/schema/bus-protocol.schema.json");
        if cand.is_file() {
            return Ok(cand);
        }
        tried.push(cand);
        dir = d.parent();
    }
    Err(tried)
}

#[test]
fn the_vendored_schema_is_byte_identical_to_the_upstream_contract() {
    // WHY a vendored copy at all: the tests compile against a fixed schema, so the suite is hermetic and
    // reproducible. WHY this test: a hermetic copy is a copy that can rot silently. Byte-comparing it
    // against the contract makes a contract edit turn this suite red, which forces an explicit re-vendor
    // commit — and that commit is the auditable record of "we adopted contract revision X".
    let upstream = match upstream_schema_path() {
        Ok(p) => p,
        Err(tried) => {
            // NOT a silent skip. A missing `vendor` symlink once made whole conformance rows skip
            // unnoticed in this repo; the lesson was that "cannot check" must never look like "checked".
            // So the default is a loud failure naming every path tried, with a documented escape hatch
            // for a checkout that genuinely has no sibling contract repo.
            let msg = format!(
                "CANNOT VERIFY the vendored contract schema is fresh: no upstream copy found.\n\
                 Tried (in order):\n  {}\n\
                 Point AETHER_CONTRACT_SCHEMA at empyrean/contract/schema/bus-protocol.schema.json, \
                 or set AETHER_CONTRACT_OPTIONAL=1 to downgrade this to a warning.\n\
                 This does NOT pass silently on purpose (contract §8 item 15): a freshness check that \
                 cannot run must say so, or a stale vendored schema validates every message against \
                 last week's contract and the suite stays green.",
                tried
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n  ")
            );
            if std::env::var("AETHER_CONTRACT_OPTIONAL").is_ok() {
                eprintln!("WARNING: {msg}");
                return;
            }
            panic!("{msg}");
        }
    };

    let up = std::fs::read(&upstream).expect("read the upstream contract schema");
    let vendored = common::schema::VENDORED_SCHEMA.as_bytes();
    if up == vendored {
        return; // The ordinary case: we track the contract's default branch and it has not moved.
    }
    // Otherwise we may be tracking a revision that is real but not yet on the default branch. That is a
    // legitimate state — a server implements against an adjudicated amendment while the contract repo
    // finishes merging it — and it is NOT a licence to skip the check. See `TRACKED_REVISION`.
    match tracked_revision_check(&upstream, vendored) {
        Ok(note) => eprintln!("NOTE: {note}"),
        Err(why) => panic!(
            "the vendored schema does not match {} and is not a clean copy of the revision \
             PROVENANCE.md pins.\n{why}\n\
             Re-vendor it and update crates/oracle-aether/tests/contract/PROVENANCE.md — that commit is \
             the record of adopting a contract revision.",
            upstream.display()
        ),
    }
}

/// The contract revision the vendored copy tracks when it is **ahead of the contract's default branch**.
///
/// `None` means "we track the default branch" and the plain byte-comparison above is the whole test.
/// `Some((rev, branch))` means the vendored copy is a verbatim copy of `rev`, which is not yet merged.
///
/// Why this exists rather than a skip. The profiler surface implements an adjudicated amendment that is
/// still an unmerged draft in the contract repo, so upstream's working tree is the pre-amendment schema
/// and a plain byte-compare would be red for a reason that is not drift. Turning the check off for that
/// would give up the only thing that stops a vendored copy rotting silently. So instead the check gets
/// *stricter*: it must still find an exact upstream match, just at a **named revision**, and it
/// additionally demands that the default branch has not moved the schema since that revision branched.
/// The second condition is what preserves the original guarantee — if the contract edits the schema on
/// its default branch while we track a draft, this goes red exactly as it would have before.
///
/// It retires itself: the moment the draft merges, upstream's working tree matches the vendored bytes,
/// the early return above fires, and none of this code runs.
const TRACKED_REVISION: Option<(&str, &str)> = None;

/// Verify the vendored bytes against [`TRACKED_REVISION`] using the contract repo's own object store.
fn tracked_revision_check(upstream: &std::path::Path, vendored: &[u8]) -> Result<String, String> {
    let Some((rev, branch)) = TRACKED_REVISION else {
        return Err(
            "PROVENANCE.md pins no tracked revision, so the copy has simply drifted.".into(),
        );
    };
    // `upstream` is <repo>/contract/schema/bus-protocol.schema.json; the repo root is three up, and the
    // path within it is the same three components.
    let repo = upstream
        .ancestors()
        .nth(3)
        .ok_or("cannot locate the contract repo root above the schema path")?;
    let rel = "contract/schema/bus-protocol.schema.json";
    let git = |args: &[&str]| -> Result<Vec<u8>, String> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .map_err(|e| format!("running git in {}: {e}", repo.display()))?;
        if !out.status.success() {
            return Err(format!(
                "git {:?} in {} failed: {}",
                args,
                repo.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(out.stdout)
    };

    let at_rev = git(&["show", &format!("{rev}:{rel}")])?;
    if at_rev != vendored {
        return Err(format!(
            "the vendored copy is {} bytes and {rev} ({branch}) is {} — it is not a verbatim copy of \
             the revision PROVENANCE.md pins either.",
            vendored.len(),
            at_rev.len()
        ));
    }

    // The guard the early return would otherwise have given us: the default branch must not have moved
    // the schema since the tracked revision branched off it. If it has, we are behind on one line of
    // development while being ahead on another, and that is drift however it is spelled.
    let head = String::from_utf8(git(&["rev-parse", "HEAD"])?)
        .map_err(|e| e.to_string())?
        .trim()
        .to_string();
    let base = String::from_utf8(git(&["merge-base", &head, rev])?)
        .map_err(|e| e.to_string())?
        .trim()
        .to_string();
    let at_base = git(&["show", &format!("{base}:{rel}")])?;
    let upstream_now = std::fs::read(upstream).map_err(|e| e.to_string())?;
    if at_base != upstream_now {
        return Err(format!(
            "the contract's checked-out branch has changed the schema since {rev} ({branch}) branched \
             from {base}. The vendored copy tracks the draft and is now ALSO stale against the branch \
             the draft will merge into."
        ));
    }

    Ok(format!(
        "the vendored contract schema tracks {rev} ({branch}), an unmerged contract revision, and is a \
         verbatim copy of it; the checked-out branch has not touched the schema since that revision \
         branched. This note disappears when the revision merges."
    ))
}

// ---------------------------------------------------------------------------------------------------
// 2. Coverage — reported, and pinned so it cannot shrink
// ---------------------------------------------------------------------------------------------------

/// The methods we advertise that the SEED schema gives **no** `result` schema for.
///
/// Pinned deliberately. Two ways this list can be wrong, and the test catches both:
///
/// * a **newly advertised** method would join it silently, so a new op would arrive on the wire with
///   nothing checking its result shape and nothing saying so;
/// * a **newly schematized** method would leave it, and that has to be a deliberate edit here rather
///   than a number quietly improving.
///
/// The list did **not** move when `emulator/pixel_attribution` was implemented, and that is the
/// mechanism working rather than a gap: CR-10 put the fragment in the contract *first*, so the method
/// arrived already schematized and the covered count went 8 → 9 while this list stayed at 12.
///
/// **It is now empty, and it emptied the right way round.** Writing the 12 missing fragments was
/// explicitly not this harness's job — writing schemas from what this server emits would encode the
/// implementation as the contract, the exact inversion of "the contract leads" (§8). So they were raised
/// as CR-13, the contract ruled every key on its merits (`empyrean` `f309cc8`, `protocol.md` §11.5:
/// registered, restructured, or **struck**), and the 12 fragments arrived upstream. Coverage went 9 → 21
/// because the contract moved first, which is the only direction that counts — and 21 → 25 on 2026-08-15
/// for the same reason, when CR-11/CR-12's four fragments were ruled into the contract before a line of
/// handler was written.
///
/// **Those four numbers are a record of two moves in 2026-08, not a claim about today.** The advertised
/// list has grown well past 25 since, and every fragment that arrived with it arrived the same way round;
/// what the file asserts about the present is the *empty list* below and the enumerated one further down,
/// never a total. A count in a comment is exactly what went stale here twice — the second time while this
/// very paragraph sat beside a green assertion saying otherwise.
///
/// The pin stays, and it now guards the one remaining direction: a **newly advertised** method would join
/// this list silently, arriving on the wire with nothing checking its result shape and nothing saying so.
/// An empty expectation makes that failure loud on the first run.
const UNCOVERED_METHODS: &[&str] = &[];

#[test]
fn the_schema_covers_every_method_we_advertise_and_the_uncovered_list_is_pinned_empty() {
    let advertised: Vec<&str> = METHODS.iter().map(|m| m.name).collect();
    let schematized = schemas().methods_with_result();

    let mut covered: Vec<&str> = advertised
        .iter()
        .copied()
        .filter(|m| schematized.contains(m))
        .collect();
    let mut uncovered: Vec<&str> = advertised
        .iter()
        .copied()
        .filter(|m| !schematized.contains(m))
        .collect();
    covered.sort_unstable();
    uncovered.sort_unstable();

    // Schematized but not advertised: harmless in that direction — D4 makes the advertised list
    // authoritative — but worth printing, because it is the shape of a method we might be missing.
    let schema_only: Vec<&str> = schematized
        .iter()
        .copied()
        .filter(|m| !advertised.contains(m))
        .collect();

    println!("--- Aether wire-schema coverage (contract §8 item 15) ---");
    println!(
        "advertised methods: {}   result schema present: {}   absent: {}",
        advertised.len(),
        covered.len(),
        uncovered.len()
    );
    println!("  COVERED   ({}): {}", covered.len(), covered.join(", "));
    println!(
        "  UNCOVERED ({}): {}",
        uncovered.len(),
        uncovered.join(", ")
    );
    println!(
        "  schematized but not advertised ({}): {}",
        schema_only.len(),
        schema_only.join(", ")
    );
    println!(
        "  events with a params schema ({}): {}",
        schemas().events_with_params().len(),
        schemas().events_with_params().join(", ")
    );
    println!(
        "  => envelope coverage 100% of lines; result coverage {}/{} methods. \
         Every line is checked against `anyMessage`; a reply to an UNCOVERED method would get the \
         envelope and nothing more — there are {} of those.",
        covered.len(),
        advertised.len(),
        uncovered.len()
    );

    // The registry prints HERE, beside the coverage split, and not in a test of its own — because the
    // failure mode being guarded against is someone reading a green suite as "fully conformant". A
    // number that says what is checked has to sit next to the list of what is checked *differently*.
    println!(
        "  KNOWN CONTRACT DIVERGENCES ({}) — registered, ruling-pending, NOT conformant:",
        KNOWN_CONTRACT_DIVERGENCES.len()
    );
    for line in divergence_report() {
        println!("    {line}");
    }
    // The closing claim adapts to the registry, because a fixed sentence is how a report starts lying.
    // With divergences registered, green means only "nothing UNregistered". With none, the claim is
    // genuinely stronger — but it is still a claim about *shapes*, and D14 puts behaviour under the
    // prose, so it must not be read as conformance to §8 as a whole.
    if KNOWN_CONTRACT_DIVERGENCES.is_empty() {
        println!(
            "  => no registered divergences: every advertised method's replies match the contract's \
             own wire shapes, closed against their fragments (§8 item 20), so an unknown key is a red \
             test rather than a shape nobody sampled. This is NOT the same as conformance to §8: D14 \
             puts BEHAVIOUR under the prose, and the sharpest live example is that `reason: \"step\"` \
             for a completed run_frames would pass everything here (see the item-13 test below). Two \
             shape-level holes also remain, both pinned as open: `anyMessage` is a oneOf over both wire \
             directions, and the closure is top-level only."
        );
    } else {
        println!(
            "  => this server is NOT fully schema-conformant. A green suite means \"no UNREGISTERED \
             divergences\", which is a weaker and more useful claim — and since §8 item 20 landed it is \
             a much sharper one: every result is closed against its fragment, so an unknown key is a \
             red test rather than a shape nobody sampled."
        );
    }

    let mut expected_uncovered = UNCOVERED_METHODS.to_vec();
    expected_uncovered.sort_unstable();
    assert_eq!(
        uncovered, expected_uncovered,
        "the set of methods with no `result` schema changed.\n\
         If a method was ADDED to `engine::METHODS`, it joined the unchecked pile — add it to \
         UNCOVERED_METHODS deliberately, or (better) get a fragment into the contract schema first.\n\
         If a schema fragment was COMPLETED, remove that method from UNCOVERED_METHODS in the same \
         commit that re-vendors the schema."
    );
    assert_eq!(
        covered.len() + uncovered.len(),
        advertised.len(),
        "every advertised method is in exactly one bucket"
    );

    // **The other direction — and the decision the 58-fragment re-vendor owed it.**
    //
    // For most of this file's life `schema_only` was non-empty by design — fragments landed ahead of
    // their handlers, which is the order §8 item 20 wants — so it was printed and not asserted. Serving
    // the CRAM pair emptied it, and `assert!(schema_only.is_empty())` was written to stop an empty list
    // going back to non-empty silently. Its own comment said the rule was that a fragment ahead of its
    // handler "has to be a decision, taken in the commit that re-vendors". empyrean's §9
    // mechanical-completion pass added 21 such fragments, so this is that commit, and this is that
    // decision. It is written down here rather than in a doc because this is the assertion that will ask
    // the question again.
    //
    // **The decision: schematized-but-unadvertised is a legitimate steady state here, and the ones listed
    // below are not deferred work.** Three grounds, in order of weight.
    //
    //  1. **The contract says the fragment comes first.** §8 item 20 makes a fragment the *precondition*
    //     for a handler and not its record; the schema's own description says the CRAM pair was
    //     "schematized first on purpose" for exactly that reason. A gate that turns the contract's
    //     required order into a failure is punishing the artifact for being correct.
    //  2. **They are not this server's rows — yet.** §6 is the suite catalog, not our backlog. Every name
    //     still listed is served today by `oracle-old` and by no route in this process — no `METHODS`
    //     entry, no handler symbol, no cargo feature (this crate has no `[features]` at all), no runtime
    //     toggle. The 2026-08-22 dry run enumerated all seven routes that could produce a reply and found
    //     the blocker on each. Eight are governed by a capability flag this server publishes as `false`,
    //     which is the contract's own way of saying "not here".
    //
    //     *"Yet" is now load-bearing.* These 21 became this repo's acceptance contract — what the successor
    //     must serve before it can replace the legacy server — and the `step*` trio has since been served
    //     and removed from the list below. So the ground is "not served here today", never "never ours",
    //     and the set is expected to shrink one shipped handler at a time.
    //  3. **The failure it was written to catch is a different failure.** It was aimed at *our* rows
    //     sitting unserved after we accepted them (§11.13's, §11.14's). Nothing about these 21 was
    //     accepted here.
    //
    // **What it does NOT become: a printed count.** A report says "21" and goes green forever; a 22nd
    // fragment for a method we never serve — the exact original failure — would then print `22` and pass.
    // Worse, a bare `is_empty()` and a bare count share the vacuous shape this repo has been bitten by:
    // both are satisfied by a schema that failed to load, parsed to nothing, or was silently empty. `0`
    // and `not checked` are the same observation.
    //
    // So it becomes a **pinned set**, the shape `UNCOVERED_METHODS` above already uses, and it is
    // strictly louder than what it replaces in all three directions:
    //
    //   * a fragment arriving for one more unserved method → red (the original purpose, kept);
    //   * one of the listed names becoming served → red, forcing its removal in the commit that ships it,
    //     which is the half `is_empty()` could never have caught — and which is exactly what happened to
    //     the `step*` trio on 2026-08-22;
    //   * an empty or unparsed schema → red, because the expectation is a literal set of names and not
    //     zero. This is the property the brief demands: the check cannot be satisfied by not running.
    //
    // The list is a *decision record*, so it is literal on purpose — the same reasoning as
    // `UNCOVERED_METHODS`, where the point is that the number cannot quietly improve. What must not be
    // literal is any claim about the names, so the two claims the pin implies are re-derived below from
    // the schema and from `METHODS` rather than trusted: each pinned name has a fragment, and each is
    // genuinely unadvertised. A typo or a stale entry cannot survive that.
    const SCHEMATIZED_NOT_ADVERTISED: &[&str] = &[
        "emulator/audio_spectrum",
        "emulator/breakpoint_add",
        "emulator/breakpoint_clear",
        "emulator/breakpoint_list",
        "emulator/get_channel_states",
        "emulator/get_layer_states",
        "emulator/log_clear",
        "emulator/ping",
        "emulator/set_channel_enabled",
        "emulator/set_layer_enabled",
        // `emulator/step`, `emulator/step_out` and `emulator/step_over` were here until 2026-08-22, and
        // `emulator/run_to_scanline` left the same day — the first four of the 21 to leave the set by being
        // SERVED, which is the direction the pin's second bullet was written for and the one `is_empty()`
        // could never have caught. Each removal was forced by this assertion going red on the commit that
        // shipped the handler, not remembered afterwards. Seventeen remain — the names in this literal,
        // which is the only count worth having — and the three grounds above still hold for every one.
        "emulator/vgm_start",
        "emulator/vgm_status",
        "emulator/vgm_stop",
        "emulator/wait_for_break",
        "emulator/write_vram",
        "emulator/z80_read",
        "emulator/z80_write",
    ];

    let mut expected_schema_only = SCHEMATIZED_NOT_ADVERTISED.to_vec();
    expected_schema_only.sort_unstable();
    let mut schema_only_sorted = schema_only.clone();
    schema_only_sorted.sort_unstable();
    assert_eq!(
        schema_only_sorted, expected_schema_only,
        "the set of schematized-but-unadvertised methods changed.\n\
         A fragment ARRIVED for a method we do not serve: decide it in the re-vendor commit — serve it, \
         or add it here with the reason, exactly as the 21 above were decided.\n\
         A method LEFT the set because we now serve it: remove it here in the same commit that ships the \
         handler.\n\
         Neither is a conformance failure (D4 makes the advertised list authoritative); both are \
         decisions that must be taken deliberately rather than noticed later."
    );

    // The two claims the pin implies, re-derived rather than trusted — so a typo or a stale name in the
    // list above cannot sit there looking like a decision.
    for m in SCHEMATIZED_NOT_ADVERTISED {
        assert!(
            schematized.contains(m),
            "{m} is pinned as schematized-but-unadvertised, but the vendored schema has no fragment for \
             it — the pin names something that does not exist"
        );
        assert!(
            !advertised.contains(m),
            "{m} is pinned as UNadvertised but appears in engine::METHODS — this server serves it, so \
             the pin is stale and must be removed in the commit that shipped the handler"
        );
    }

    // **Every fragment in the document reaches the coverage split — the one blind spot the pins leave.**
    //
    // The first thing written here was `covered.len() + schema_only.len() == schematized.len()`, and it
    // was deleted for being **tautological**: those two sets partition `schematized` by construction, so
    // the identity holds however few fragments `schematized` contains. An assertion that cannot fail is
    // worse than no assertion, because it reads like coverage.
    //
    // The real gap is one level further back. `schematized` is built from fragments that declare a
    // `result`, and every bucket in this test derives from it — so a fragment **without** a `result` is
    // invisible to all of them. Trace it: it is not in `schematized`, therefore not in `covered`, not in
    // `uncovered` and not in `schema_only`, so the UNCOVERED pin, the schematized-but-unadvertised pin
    // and the per-name checks above are all satisfied while a fragment nobody has looked at sits in the
    // document. That is exactly the arrival this test exists to make loud.
    //
    // The other routes into `schematized` shrinking are already covered earlier and this check is
    // deliberately not a second guard on them: an *advertised* method losing its `result` trips the
    // UNCOVERED pin, and a *pinned* one losing it trips the pin — both were confirmed by making them
    // fail. What is left, and what this catches, is a fragment that is neither advertised nor pinned and
    // carries no `result`. Proven red by adding exactly that fragment to a scratch copy of the schema and
    // watching every assertion above stay green.
    //
    // The population is re-derived straight off the raw document — every non-`$` key under `methods` —
    // on every run, never a literal.
    let fragment_names: Vec<&str> = common::schema::schema_root()["methods"]
        .as_object()
        .expect("the vendored schema has a methods object")
        .keys()
        .filter(|k| !k.starts_with('$'))
        .map(String::as_str)
        .collect();
    let unsplit: Vec<&str> = fragment_names
        .iter()
        .copied()
        .filter(|n| !schematized.contains(n))
        .collect();
    assert!(
        unsplit.is_empty(),
        "{} fragments in the vendored schema declare no `result` and so reach neither bucket of this \
         coverage split: {unsplit:?}.\n\
         Every assertion in this test — the UNCOVERED pin and the schematized-but-unadvertised pin \
         alike — is blind to them. Either the fragment is incomplete upstream, or `methods_with_result` \
         is not reading the whole document.",
        unsplit.len()
    );
}

// ---------------------------------------------------------------------------------------------------
// 3. The divergence registry — kept live, so it cannot rot after a ruling
// ---------------------------------------------------------------------------------------------------

#[test]
fn every_registered_divergence_is_still_live() {
    // **The anti-rot property, and the reason the registry is a list of canonical MESSAGES rather than a
    // list of method names.** Each entry claims that the schema, as vendored, rejects a specific shape
    // this server puts on the wire. This test checks the claim both ways round:
    //
    //   * the schema must STILL reject it with no allowances — so when a CR is ruled on and the schema
    //     re-vendored, this goes red and the entry must be deleted in the same commit;
    //   * the harness must ACCEPT it with allowances — so the entry is actually doing the job it exists
    //     for, and is not a dead line of documentation next to an allowance that fires somewhere else.
    //
    // Liveness is keyed to the schema rather than to observed traffic on purpose. Counting live firings
    // would only see the messages the binary that owns the counter happened to produce — precisely the
    // sampling weakness that let CR-14 through a 33-message probe of a live server.
    //
    // The non-emptiness assertion that used to stand here is gone, deliberately: its own comment asked
    // for exactly that ("if the last divergence was ruled on and removed, delete this assertion"), CR-14's
    // ruling is the event it was written for, and a registry that is empty for a day should not have to
    // fake an entry to keep a test honest. The anti-vacuity job it was doing has moved somewhere it holds
    // whether the list is empty or not: `the_strict_closure_rejects_a_surplus_key_and_needs_the_…`
    // proves the validator still bites. (The list is not empty today — CR-16 landed the same afternoon.)
    for d in KNOWN_CONTRACT_DIVERGENCES {
        let (line, method) = (d.canonical)();

        let strict = check_incoming_strict(&line, method);
        assert!(
            strict.is_err(),
            "{} ({} {}) NO LONGER DIVERGES: the vendored schema now accepts the shape this entry was \
             registered for. That is good news — a ruling landed — but the entry is now a lie. Delete \
             it from KNOWN_CONTRACT_DIVERGENCES, delete its allowance in common::schema, and close the \
             CR in docs/2026-08-14-aether-change-requests.md.\n  canonical: {line}",
            d.cr,
            d.method,
            d.path
        );

        // A weak locator, and labelled as one rather than dressed up. For a divergence the schema
        // reports at a key (`/otherMatches`) this really does check the entry describes the right bug.
        // For one that falls out at the root `oneOf` (CR-15) the failure text embeds the whole instance,
        // so the key name is present either way and this proves little. The load-bearing assertions are
        // the two either side of it; this one only catches an entry whose path is outright unrelated.
        // An entry may name several keys (`"$.region, $.symbolDisp, $.caveat"`), and EVERY one of them
        // must show up — a divergence that listed three keys and tripped over one would otherwise pass
        // while two-thirds of it went unproven.
        let failures = strict.unwrap_err();
        for key in d.path.split(',').map(|k| k.trim().trim_start_matches("$.")) {
            assert!(
                failures.iter().any(|f| f.contains(key)),
                "{} ({} {}) diverges, but `{key}` appears nowhere in the failure — the entry may be \
                 describing one bug while its canonical message trips over another.\n  failures: {failures:#?}",
                d.cr,
                d.method,
                d.path
            );
        }

        check_incoming(&line, method).unwrap_or_else(|e| {
            panic!(
                "{} ({} {}) is registered but its allowance does not actually admit its own canonical \
                 message — the entry and the allowance have drifted apart.\n  failures: {e:#?}",
                d.cr, d.method, d.path
            )
        });
    }
}

#[test]
fn every_registered_divergence_names_a_real_change_request() {
    // A CR number that does not exist in the ledger makes the registry unauditable: the whole promise is
    // "registered, not silenced", and an entry pointing at nothing is silenced with extra steps.
    let ledger = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/2026-08-14-aether-change-requests.md"),
    )
    .expect("the change-request ledger must be readable from the crate");
    for d in KNOWN_CONTRACT_DIVERGENCES {
        assert!(
            ledger.contains(&format!("## {} —", d.cr)),
            "{} is registered in KNOWN_CONTRACT_DIVERGENCES but has no `## {} —` section in \
             docs/2026-08-14-aether-change-requests.md",
            d.cr,
            d.cr
        );
    }
}

// ---------------------------------------------------------------------------------------------------
// 4. Anti-vacuity controls — proof the validator rejects
// ---------------------------------------------------------------------------------------------------

/// Assert the harness rejects `line`, and that some failure message mentions `needle`.
///
/// Naming the field is the point: an error that says only "invalid" would not distinguish a validator
/// that found the planted defect from one that rejects everything, and the positive control below is
/// what rules the latter out.
fn rejects(line: &Value, method: Option<&str>, needle: &str) -> Vec<String> {
    match check_incoming(line, method) {
        Ok(()) => panic!(
            "THE VALIDATOR ACCEPTED A MESSAGE IT MUST REJECT (expected a failure mentioning \
             `{needle}`). A validator that accepts everything is a vacuous assertion: the suite would \
             be green and the wire unchecked.\n  line: {line}"
        ),
        Err(failures) => {
            assert!(
                failures.iter().any(|f| f.contains(needle)),
                "rejected, but no failure mentions `{needle}` — the validator may be failing for an \
                 unrelated reason, which would make this control prove nothing.\n  failures: {failures:#?}"
            );
            failures
        }
    }
}

/// A well-formed `emulator/read_memory` reply, used as the base for the planted defects below.
fn good_read_memory_reply() -> Value {
    // `region` is REQUIRED as of the CR-16 re-vendor (`empyrean` `d45dc87`, §11.6) — §6's row lists it
    // un-parenthesised, and the server has always emitted it. This fixture omitted it and so stopped
    // being conformant the moment the fragment declared it, which is the positive control doing its job
    // on itself: a "conformant reply" fixture that drifts from the contract silently weakens every
    // rejection control underneath it.
    json!({"jsonrpc":"2.0","id":7,"result":{
        "addr":"0x00FFA144","len":4,"bytes":"0x02600000","symbol":"Camera_X","region":"work RAM",
        "frame":601,"mclk":538008040,"running":false,"droppedEvents":0}})
}

#[test]
fn positive_control_a_conformant_message_is_accepted() {
    // Without this, every control below is satisfied by a validator that rejects unconditionally.
    check_incoming(&good_read_memory_reply(), Some("emulator/read_memory"))
        .expect("a conformant reply must pass");
    check_incoming(
        &json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
            "reason":"runFrames","pc":"0x00012A4C","frames":1,"deadlineReached":true,
            "frame":1,"mclk":896040,"running":false}}),
        None,
    )
    .expect("a conformant event must pass");
}

#[test]
fn control_a_reply_without_droppedevents_is_rejected() {
    // §2.3 / D17 / §8 item 18: present even at zero — "0 is the answer, not the absence of one".
    let mut line = good_read_memory_reply();
    line["result"]
        .as_object_mut()
        .unwrap()
        .remove("droppedEvents");
    rejects(&line, Some("emulator/read_memory"), "droppedEvents");
}

#[test]
fn control_a_numeric_checkpoint_id_is_rejected() {
    // A REGRESSION GUARD, not a synthetic one: this exact shape was live in this tree until 2026-08-15,
    // and it is the one thing the probe found the live wire actually violating (F1). §8 item 16 / D9
    // category 4: an opaque handle is a JSON string.
    //
    // Note this control also proves the *keying* works. The envelope alone accepts it — `replyFields`
    // says nothing about a key called `id` inside `result` — so it can only be caught by having reached
    // for `methods["emulator/checkpoint"].result`, which means `recv` correctly attributed the reply to
    // the request that asked for it.
    let line = json!({"jsonrpc":"2.0","id":3,"result":{
        "id":1,"bytes":262144,"frame":10,"mclk":8960400,"running":false,"droppedEvents":0}});
    check_incoming(&line, None).expect("the envelope alone accepts it — that is the point");
    rejects(&line, Some("emulator/checkpoint"), "id");
}

/// A well-formed `emulator/watchpoint_hits` reply carrying one **bus** hit, used as the base for the
/// planted defects below.
fn good_hits_reply(hit: Value) -> Value {
    json!({"jsonrpc":"2.0","id":11,"result":{
        "hits":[hit],"total":1,"returned":1,"limit":100,"truncated":false,
        "dropped":0,"seen":42,"matched":1,
        "frame":3,"mclk":2688120,"running":false,"droppedEvents":0}})
}

fn good_bus_hit() -> Value {
    json!({"watch":"w0","space":"bus","addr":"0x00FF8000","value":"0x00001234","size":2,
           "op":"write","fc":5,"via":"bus","pc":"0x000002A0","frame":1,"mclk":896040,"seq":0})
}

#[test]
fn control_a_bus_hit_carrying_old_and_a_vdp_hit_missing_it_are_both_rejected() {
    // **The structural presence rule, both directions.** `Watchpoints::on_event` builds every bus hit with
    // `old: 0` unconditionally — the 68000 bus event stream carries no prior value — so a bus hit that
    // reports `old` is asserting something false, and it would defeat the one exact per-write change test
    // this instrument offers. A VDP-internal write has the prior value and has no bus function code.
    check_incoming(
        &good_hits_reply(good_bus_hit()),
        Some("emulator/watchpoint_hits"),
    )
    .expect("the positive control first: a conformant bus hit must pass");

    let mut hit = good_bus_hit();
    hit["old"] = json!("0x00000000");
    rejects(
        &good_hits_reply(hit),
        Some("emulator/watchpoint_hits"),
        "old",
    );

    // And the mirror: a VDP hit must carry `old` and must not carry `fc`.
    let vdp = json!({"watch":"w0","space":"vram","addr":"0x00000100","value":"0x000000BE",
                     "old":"0x00000000","size":1,"op":"write","via":"direct","pc":"0x00000216",
                     "frame":0,"mclk":0,"seq":0});
    check_incoming(
        &good_hits_reply(vdp.clone()),
        Some("emulator/watchpoint_hits"),
    )
    .expect("a conformant VDP hit must pass");
    let mut missing_old = vdp.clone();
    missing_old.as_object_mut().unwrap().remove("old");
    rejects(
        &good_hits_reply(missing_old),
        Some("emulator/watchpoint_hits"),
        "old",
    );
    let mut with_fc = vdp;
    with_fc["fc"] = json!(0);
    rejects(
        &good_hits_reply(with_fc),
        Some("emulator/watchpoint_hits"),
        "fc",
    );
}

#[test]
fn control_a_numeric_watch_handle_is_rejected_wherever_it_appears() {
    // §8 item 16's mistake, generalised: this server shipped a numeric checkpoint handle once, and the
    // watch handle appears in five places. Two of them are checked here — the rest are covered
    // behaviourally by `tests/watchpoints.rs`.
    let mut hit = good_bus_hit();
    hit["watch"] = json!(0);
    rejects(
        &good_hits_reply(hit),
        Some("emulator/watchpoint_hits"),
        "watch",
    );

    let line = json!({"jsonrpc":"2.0","id":12,"result":{
        "watch":0,"space":"bus","addr":"0x00FF8000","len":2,"op":"write","mode":"record",
        "frame":0,"mclk":0,"running":false,"droppedEvents":0}});
    rejects(&line, Some("emulator/watchpoint_add"), "watch");
}

#[test]
fn control_a_hits_reply_missing_an_honesty_counter_is_rejected() {
    // §8 item 21: `seen`, `dropped` and `matched` are REQUIRED beside §2.4's `total`/`returned`/
    // `truncated`. `seen` is the one that matters most — without it a client cannot tell "this address is
    // never written" from "the recorder was not in the run" — so a reply that omits it is rejected rather
    // than read as a zero.
    for key in [
        "seen",
        "dropped",
        "matched",
        "total",
        "returned",
        "truncated",
    ] {
        let mut line = good_hits_reply(good_bus_hit());
        line["result"].as_object_mut().unwrap().remove(key);
        rejects(&line, Some("emulator/watchpoint_hits"), key);
    }
}

#[test]
fn control_a_watchpoint_stop_that_does_not_name_its_watch_is_rejected() {
    // Unlike CR-9's `buttons`/`port`, this rule HAS a discriminator in the event, so the schema enforces
    // both halves of it: a `watchpoint` stop must name its watch, and no other stop may.
    let missing = json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
        "reason":"watchpoint","pc":"0x000002A0","deadlineReached":false,
        "frame":1,"mclk":896040,"running":false}});
    rejects(&missing, None, "watch");

    let spurious = json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
        "reason":"runFrames","pc":"0x000002A0","frames":1,"deadlineReached":true,"watch":"w0",
        "frame":1,"mclk":896040,"running":false}});
    rejects(&spurious, None, "watch");
}

#[test]
fn control_buttons_without_port_is_rejected_and_that_is_all_the_schema_can_do() {
    // **CR-9's cost, made executable.** `dependentRequired` catches the half-attribution — a subscriber
    // told which buttons went down and not which pad would blame the wrong controller in a two-pad
    // session — and that is the ONLY half a schema can reach. The other half (present iff `press` drove
    // the advance) has no discriminator in the event, deliberately: `reason` names the stop condition and
    // never the driving method, so a press-driven advance and a `run_frames` one both read `runFrames`.
    // The second assertion below is that gap, asserted rather than assumed, so nobody later reads the
    // schema as enforcing more than it does. Its behavioural half is
    // `watchpoints::press_stops_carry_buttons_and_port_and_run_frames_does_not`.
    let half = json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
        "reason":"runFrames","pc":"0x000002A0","frames":2,"deadlineReached":true,
        "buttons":["start"],"frame":2,"mclk":1792080,"running":false}});
    rejects(&half, None, "port");

    let fabricated = json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
        "reason":"runFrames","pc":"0x000002A0","frames":2,"deadlineReached":true,
        "buttons":["start"],"port":0,"frame":2,"mclk":1792080,"running":false}});
    check_incoming(&fabricated, None).expect(
        "a run_frames stop wearing buttons/port is SCHEMA-VALID — the event carries no method \
         discriminator, so this rule is behavioural and is pinned in tests/watchpoints.rs",
    );
}

#[test]
fn control_a_stopped_event_with_an_unknown_reason_is_rejected() {
    // §3's reason enum is closed. A server may not widen it unilaterally (§8).
    let line = json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
        "reason":"frameAdvance","pc":"0x00012A4C","frame":1,"mclk":896040,"running":false}});
    rejects(&line, None, "reason");
}

#[test]
fn control_an_envelope_with_both_result_and_error_is_rejected() {
    // §2: success is signalled by the presence of `result`; an error response carries `error`. Both at
    // once tells a client two different things about one call.
    let line = json!({"jsonrpc":"2.0","id":9,
        "result":{"frame":1,"mclk":2,"running":false,"droppedEvents":0},
        "error":{"code":-32603,"message":"internal","data":{"frame":1,"mclk":2,"running":false,"droppedEvents":0}}});
    // This one names the keyword rather than a field, and necessarily so: `successResponse` and
    // `errorResponse` both close `additionalProperties`, so the message matches NEITHER arm and the
    // failure is reported at the root `oneOf` of `anyMessage`. Asserting on "oneOf" is asserting that
    // the envelope discriminant did its job.
    let failures = rejects(&line, None, "oneOf");
    assert!(
        failures.iter().any(|f| f.starts_with("anyMessage:")),
        "the failure must come from the envelope check: {failures:#?}"
    );
}

#[test]
fn control_an_error_without_data_is_rejected() {
    // §2 / §2.2 / §2.3: `error.data` is ALWAYS present, because it carries the stamp and the counter.
    // This is the arm of the funnel that reaches `$defs/errorObject` — it needs no separate dispatch,
    // `anyMessage` -> `errorResponse` -> `errorObject` gets there on its own.
    let line =
        json!({"jsonrpc":"2.0","id":11,"error":{"code":-32004,"message":"address out of range"}});
    rejects(&line, None, "oneOf");

    // And the stamp inside `data` is not optional either.
    let line = json!({"jsonrpc":"2.0","id":11,"error":{
        "code":-32004,"message":"address out of range","data":{"addr":"0x0","droppedEvents":0}}});
    rejects(&line, None, "oneOf");
}

#[test]
fn control_a_hex_field_that_is_not_hex_is_rejected() {
    // `$defs/hex` (D9 category 1) is a pattern, not just "a string": an address that lost its `0x` is a
    // client-side parse bug waiting to happen.
    let mut line = good_read_memory_reply();
    line["result"]["addr"] = json!("00FFA144");
    rejects(&line, Some("emulator/read_memory"), "addr");
}

// ---------------------------------------------------------------------------------------------------
// The blind spot, recorded as an executable fact
// ---------------------------------------------------------------------------------------------------

#[test]
fn the_schema_cannot_express_section_8_item_13_and_this_test_proves_it() {
    // Probe finding F2, promoted from a comment to an assertion so it cannot quietly stop being true.
    //
    // A completed `run_frames` mislabelled `reason:"step"` PASSES the schema, because `step` is a legal
    // member of the enum. §3 pins `step` as "one instruction, or one instruction-shaped unit … not the
    // value for a frame advance" — that is BEHAVIOUR, and D14 puts behaviour under the prose, not the
    // schema. So of the two mechanical conformance items in this arc the validator catches one (item 16,
    // the checkpoint id, controlled above) and is blind to the other by construction.
    //
    // The rule itself is asserted behaviourally in `tests/events.rs`
    // (`a_completed_run_frames_reports_runframes_not_step`, `a_press_reports_runframes_…`). If this test
    // ever starts FAILING, the schema grew a way to express item 13 and those tests have a mechanical
    // backstop they did not have before — which is good news, and means this test should be deleted and
    // the doc updated.
    let mislabelled = json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
        "reason":"step","pc":"0x00012A4C","frames":8,"deadlineReached":true,
        "frame":8,"mclk":7168320,"running":false}});
    assert!(
        check_incoming(&mislabelled, None).is_ok(),
        "the schema now rejects a mislabelled run_frames stop — see this test's comment"
    );
}

#[test]
fn the_null_id_allowance_is_exactly_as_wide_as_json_rpc_2_0_requires_and_no_wider() {
    // FOUND BY THIS VALIDATOR, not by the probe — the probe's 33-message run drove six error paths, all
    // of which had a parseable request and therefore a real id, so it never saw a null one.
    //
    // JSON-RPC 2.0 §5 MANDATES `"id": null` when the id could not be detected, and our server obeys:
    // three handshake tests (invalid JSON, a batch, an over-long line) all produce it. `$defs/id` was
    // `["integer","string"]` and rejected it — a spec bug under D14, raised as CR-15, and originally
    // carried here as a narrow harness allowance.
    //
    // **CR-15 was ruled and the contract amended the same day** (`empyrean` `protocol.md` §11.4), at
    // exactly this width: `errorResponse.id` is nullable, narrowed by an `if`/`then` to the two codes
    // decided before a request object exists. So the four fences below are no longer OUR fences — they
    // are the schema's, and this test now checks that the schema's width and
    // `is_json_rpc_undetectable_id_error`'s still agree. If the contract ever widens (a nullable id on a
    // code that answers a parsed request), (a) goes red here rather than being discovered by a client
    // that cannot correlate its own failures.
    let parse_error = json!({"jsonrpc":"2.0","id":null,"error":{
        "code":-32700,"message":"invalid JSON",
        "data":{"frame":0,"mclk":0,"running":false,"droppedEvents":0}}});
    assert!(common::schema::is_json_rpc_undetectable_id_error(
        &parse_error
    ));
    check_incoming(&parse_error, None)
        .expect("the schema must accept the shape JSON-RPC 2.0 mandates");
    // ...and the *strict* verdict too: with CR-15 retired there is no allowance left to lean on, so the
    // mandated shape must pass with no allowances in play at all. This is what proves the entry was
    // retired because the contract moved, not because the harness got more permissive.
    check_incoming_strict(&parse_error, None)
        .expect("no allowance should be needed for this shape any more");

    // Now the fence, four sides of it. Each of these must still be rejected.

    // (a) A null id on a code that answers a request we DID parse. There is a real id to echo, so a
    //     null there is a correlation bug, not the protocol's mandate. The schema's `if`/`then` is what
    //     rejects this now.
    let mut wrong_code = parse_error.clone();
    wrong_code["error"]["code"] = json!(-32602);
    assert!(!common::schema::is_json_rpc_undetectable_id_error(
        &wrong_code
    ));
    rejects(&wrong_code, None, "oneOf");

    // (b) A null id on a SUCCESS. Nothing in JSON-RPC 2.0 permits it: a result always answers a request
    //     whose id was read. `$defs/id` was deliberately left unwidened, which is what keeps this shut.
    let null_id_success = json!({"jsonrpc":"2.0","id":null,"result":{
        "frame":0,"mclk":0,"running":false,"droppedEvents":0}});
    assert!(!common::schema::is_json_rpc_undetectable_id_error(
        &null_id_success
    ));
    rejects(&null_id_success, None, "oneOf");

    // (c) Legalising the null id does not legalise anything else about the message. A parse error that
    //     lost its `data` (and so its stamp and `droppedEvents`, §2.2/§2.3) is still caught.
    let mut no_data = parse_error.clone();
    no_data["error"].as_object_mut().unwrap().remove("data");
    assert!(common::schema::is_json_rpc_undetectable_id_error(&no_data));
    rejects(&no_data, None, "oneOf");

    // (d) ...and so is one whose `data` lost `droppedEvents`.
    let mut no_dropped = parse_error.clone();
    no_dropped["error"]["data"]
        .as_object_mut()
        .unwrap()
        .remove("droppedEvents");
    rejects(&no_dropped, None, "oneOf");
}

#[test]
fn the_strict_closure_rejects_a_surplus_key_and_needs_the_unevaluated_keyword_to_do_it() {
    // **Contract §8 item 20, and its own anti-vacuity control.** Two claims, and the second is the one
    // that was got wrong upstream before it was got right — so it is reproduced here rather than trusted.
    //
    // Claim 1: a conformant reply passes, and the same reply with one invented key does not. This is the
    // whole of item 20: "an unknown key on the wire is a change request, never a shipment".
    let good = good_read_memory_reply();
    check_incoming(&good, Some("emulator/read_memory")).expect("a conformant reply must pass");

    let mut surplus = good.clone();
    surplus["result"]["stoppedAtVibes"] = json!(7);
    rejects(&surplus, Some("emulator/read_memory"), "stoppedAtVibes");

    // Claim 2, the mechanics: `additionalProperties: false` would have rejected the CONFORMANT reply, not
    // the surplus one, because in draft 2020-12 it sees only its own `properties` and never those an
    // adjacent `allOf` contributes — and every fragment pulls the stamp (§2.2) and `droppedEvents` (§2.3)
    // in through `allOf: [{"$ref": "#/$defs/replyFields"}]`. Asserting this here means the harness cannot
    // be "simplified" to the obvious keyword without a red test explaining why not.
    let mut fragment = common::schema::schema_root()["methods"]["emulator/read_memory"]["result"]
        .as_object()
        .expect("the fragment is an object")
        .clone();
    fragment.insert(
        "$schema".into(),
        json!("https://json-schema.org/draft/2020-12/schema"),
    );
    fragment.insert(
        "$defs".into(),
        common::schema::schema_root()["$defs"].clone(),
    );

    let mut wrong = fragment.clone();
    wrong.insert("additionalProperties".into(), json!(false));
    let wrong = jsonschema::validator_for(&Value::Object(wrong)).expect("compiles");
    let envelope_fields: Vec<String> = wrong
        .iter_errors(&good["result"])
        .map(|e| e.to_string())
        .collect();
    assert!(
        !envelope_fields.is_empty(),
        "additionalProperties:false was expected to reject the conformant reply — if it no longer does, \
         the fragments stopped composing their envelope through `allOf` and item 20's note is stale"
    );
    for f in ["frame", "mclk", "running", "droppedEvents"] {
        assert!(
            envelope_fields.iter().any(|e| e.contains(f)),
            "additionalProperties:false must reject `{f}` — that is the defect item 20 documents. \
             errors: {envelope_fields:#?}"
        );
    }

    let mut right = fragment;
    right.insert("unevaluatedProperties".into(), json!(false));
    let right = jsonschema::validator_for(&Value::Object(right)).expect("compiles");
    assert!(
        right.is_valid(&good["result"]),
        "unevaluatedProperties:false must ACCEPT the conformant reply — it is the keyword that sees \
         across applicators, which is the whole reason item 20 names it"
    );
    assert!(
        !right.is_valid(&surplus["result"]),
        "...and must still catch the surplus key"
    );
}

#[test]
fn othermatches_is_the_bounded_container_the_contract_pins_and_carries_no_cursor() {
    // **What is left of the CR-14 fence, re-aimed at the ruled shape.** The test this replaces asserted
    // the *un-ruled* shape under an allowance, which was the honest thing to do while a ruling was
    // pending and is the wrong thing to do now that one has landed (`empyrean` `f309cc8`, §4 + §2.4).
    //
    // The history is worth keeping because of how the defect was found: `otherMatches` is emitted only on
    // the partial-match paths, and the 33-message probe that opened this arc called `lookup_symbol` with a
    // name resolving to nothing, so it only ever drove the error path. Probe finding F1 — "the only
    // schema-level failure on the live wire is the checkpoint id" — was a floor, not a result. That is the
    // sampling weakness §8 item 20 replaces with a gate.
    let ok = json!({"jsonrpc":"2.0","id":3,"result":{
        "query":"Play","exact":false,
        "otherMatches":{"items":[{"name":"Player_2","addr":"0x00FF8D4A"}],
                        "total":2,"returned":1,"limit":5,"truncated":true},
        "frame":0,"mclk":0,"running":false,"droppedEvents":0}});
    check_incoming(&ok, Some("emulator/lookup_symbol"))
        .expect("the ruled container shape must pass");

    // A bare array of strings — what the schema asked for BEFORE the ruling — is now rejected. CR-14 was
    // the first divergence where the server's shape was the better one, and this is the ruling landing.
    let mut bare = ok.clone();
    bare["result"]["otherMatches"] = json!(["Player_2"]);
    rejects(&bare, Some("emulator/lookup_symbol"), "otherMatches");

    // §2.4 clause (a): losing `truncated` is losing the one field the whole rule exists for.
    let mut lost = ok.clone();
    lost["result"]["otherMatches"]
        .as_object_mut()
        .unwrap()
        .remove("truncated");
    rejects(&lost, Some("emulator/lookup_symbol"), "truncated");

    // §2.4 clause (b) / §8 item 16: `lookup_symbol` accepts no continuation param, so a token it emits is
    // one the client can never hand back. The schema spells this `"cursor": false` / `"nextCursor": false`
    // rather than leaving it to prose, and both spellings are fenced.
    for token in ["cursor", "nextCursor"] {
        let mut with_token = ok.clone();
        with_token["result"]["otherMatches"][token] = json!(1);
        rejects(&with_token, Some("emulator/lookup_symbol"), token);
    }

    // One item shape, closed. A branch-specific extra key is exactly the "which branch am I on?" problem
    // §4 struck, and `items[]` closes with `additionalProperties` legally — that subschema has no `allOf`
    // for the keyword to be blind past.
    let mut odd_item = ok.clone();
    odd_item["result"]["otherMatches"]["items"][0]["rawName"] = json!("Player_2");
    rejects(&odd_item, Some("emulator/lookup_symbol"), "rawName");

    // The rest of the result is still the schema's business. `addr` is `$defs/hex`.
    let mut bad_addr = ok.clone();
    bad_addr["result"]["addr"] = json!(4290743546_u64);
    rejects(&bad_addr, Some("emulator/lookup_symbol"), "addr");
}

#[test]
fn anymessage_is_bidirectional_so_a_stray_id_on_an_event_slips_through() {
    // A SECOND blind spot, found by building the controls rather than by reading the schema — it was
    // written as a control that must reject, and it passed.
    //
    // `anyMessage` is a `oneOf` over BOTH directions of the wire (`request`, `successResponse`,
    // `errorResponse`, `notification`). `$defs/notification` does carry `"not": {"required": ["id"]}`,
    // but a line with `jsonrpc` + `id` + `method` + `params` is a perfectly legal `$defs/request` — so
    // an event that grew an `id` is accepted, because from the schema's side of the fence it is
    // indistinguishable from a client request that happened to travel the wrong way.
    //
    // Consequence for THIS harness, which is the part that matters: `check_incoming` keys event-params
    // validation off "has no `id`", so such a line would also skip `events.<name>.params` entirely and
    // be checked by the envelope alone. The server does not do this today (`tests/events.rs` asserts a
    // notification carries no id, directly), and closing it in the schema would need `anyMessage` split
    // into a client-to-server and a server-to-client half — a contract change, not a server one, and so
    // out of this slice by §8's ban on unilateral invention. Recorded, not worked around.
    let stray = json!({"jsonrpc":"2.0","id":4,"method":"emulator/stopped","params":{
        "reason":"pause","pc":"0x0","frame":0,"mclk":0,"running":false}});
    assert!(
        check_incoming(&stray, None).is_ok(),
        "`anyMessage` gained a direction — see this test's comment, and tell the contract"
    );

    // The narrower half IS enforced, and this is what keeps the observation honest: the bad `reason`
    // is caught the moment the line is attributable as an event, i.e. once the `id` is gone.
    let mut as_event = stray.clone();
    as_event.as_object_mut().unwrap().remove("id");
    as_event["params"]["reason"] = json!("frameAdvance");
    assert!(check_incoming(&as_event, None).is_err());
}
