#!/usr/bin/env bash
# THE LANDING COMMAND. One name, one run, one verdict.
#
# ============================================================================================
# WHY THIS EXISTS
# ============================================================================================
#
# Until this file, a landing in this repo was a set of checks chosen by hand at the time. There
# is a `.github/workflows/ci.yml`, but it is a *remote* gate on a repo whose pushes are the thing
# we are trying to make safe, it runs the DEBUG suite, and nothing local ever consulted it. So in
# practice every landing here was recalled rather than executed.
#
# Sigil had a named script and drifted off it for eleven parcels, which is how two faults reached
# its main copy, one of them a clippy red that sat there an afternoon. A checklist is precisely
# the artifact that drifts, so this is a command and not a checklist. Type its name; it decides.
#
#   ./tools/land.sh                   # run every gate, then push the tested SHA to origin/main
#   ./tools/land.sh --no-push         # run every gate and stop; report what a push WOULD do
#   ./tools/land.sh --no-debug-suite  # skip D4-D6, and SAY SO in the report (see CI PARITY below)
#   ./tools/land.sh --remote R --branch B
#
# ============================================================================================
# CI PARITY — WHAT THIS COMMAND RUNS THAT CI RUNS, AND WHAT IT DOES NOT  (added 2026-09-19)
# ============================================================================================
#
# ⚑ **The paragraph below this one used to be the whole of what this file said about CI, and a
#   landing was read as a prediction of CI anyway.** Measured three times on 2026-09-19
#   (`docs/2026-09-19-reds-without-a-cause.md`, `docs/2026-09-19-reds-0909-cohort.md`):
#
#     1. A landing green here — 17 gates, 3027 passed — went RED on CI minutes later, on
#        `panel_attribution::tests::a_run_on_no_reported_row_…`, run `35418036061`. G7 ran the suite
#        in RELEASE; CI's `Test` step runs it in DEBUG, and the assertion only exists under
#        `debug_assertions`. **This header already said the profiles differ. A prediction was
#        inferred from the green anyway** — which is the finding: a gate's stated caveat does not
#        survive contact with its own green line.
#     2. `docs/lane-log.jsonl` at `2026-09-16T03:26:06Z` records *"clippy exit 0"* on a day CI failed
#        on a clippy `nonminimal_bool`. The invocation that was run had no `-D warnings`.
#     3. `docs/lane-log.jsonl:184` records a real full-suite green from a command that cannot fail a
#        clippy gate at all.
#
#   All three are one defect: **a gate believed to check something it does not.** The fix has two
#   halves and only one of them is affordable to run every time.
#
# **THE INVENTORY (G0), and it is the half that cannot rot.** `tools/ci-parity.py` reads
# `.github/workflows/` and requires every `run:` step of every push-triggered workflow to be
# CLASSIFIED — run here, subsumed by a gate here, or named as not run — keyed by job and step name
# and pinned by a digest of the step's body. **Adding a step to `ci.yml`, or editing one, reddens
# the next landing** until a human says which it is. It maps rather than executes because the
# inventory contains `sudo apt-get install`, `actions/cache/restore` and `$GITHUB_OUTPUT`, whose
# local execution is meaningless or destructive; what is derived is the OBLIGATION, not the command.
# **`G0` exiting 0 does not mean this command runs what CI runs. It means the difference is written
# down**, and the summary at the end prints it.
#
# **THE EXECUTION, and what it cost.** Measured on this workstation, warm tree, 2026-09-19:
#
#   ⚑ THE TIMINGS BELOW CARRY NO DECIMAL POINT AND THAT IS NOT A STYLE CHOICE. The first draft of
#     this table wrote a fraction of a second, as a decimal, on the floor-reading row. The guard in
#     `crates/oracle-core/tests/toolchain_floor.rs` scans live files for a version-shaped number
#     within sixty characters of a toolchain keyword, and that row names the floor script, so it
#     read the timing as a claimed compiler version and went red. **D5 caught it, on the very first
#     end-to-end run of the arm this same commit adds** — and the release suite could not have,
#     because D5 runs before G7. Keep the numbers here integral. (The same applies to any sentence
#     ABOUT this, which is why this paragraph states no figure.)
#
#     gate  CI step                                            profile   measured
#     G0    (the inventory itself)                             —         under 1 s  NEW
#     G0b   ./tools/rust-floor.sh                              —         under 1 s  NEW
#     G1b   ./tools/verify-vendor.sh                           —         2 s      NEW
#     D1    cargo clippy --all-targets -- -D warnings          debug     19 s     NEW
#     D2    ./tools/ci-corpus-guards.sh                        debug     7 s      NEW
#     D3    cargo test -p oracle-core --test determinism_gate
#             --test proptests -- --nocapture                  debug     31 s     NEW
#     D4    (the debug leg count, derived; its build is D5's)  debug     in D5    NEW
#     D5    cargo test --workspace                             debug     731 s    NEW
#     D6    (D5's ran-to-the-end)                              —         under 1 s  NEW
#
#   `D1` to `D6` are **the debug arm** — one block, named for what distinguishes it, executed
#   straight after G5 rather than woven into the release numbering. They are the gates CI runs and
#   this command did not.
#
#   G0/G0b/G1b and D1 to D3 total under a minute against a ~25-minute landing, so they are
#   unconditional and there was nothing to trade. **Every one of them is something CI has always run
#   and this command never did** — including `verify-vendor.sh`, which matters more here than on a
#   runner: a worktree that symlinks a sibling's `vendor/` inherits whatever that sibling last
#   fetched, and G1 only ever checked that the directories were non-empty. D2 is the sharpest of
#   them: G1 printed a note TELLING the reader they could run the corpus guards, and never ran them.
#
# ⚑ **THE PRICE, MEASURED RATHER THAN GUESSED, and it came out the opposite way round to the guess.**
#   On this workstation, warm tree, 2026-09-19: **the debug suite is 731 s / 12 m 11 s, 91 legs,
#   3058 passed / 0 failed / 10 ignored.** The whole debug arm is **790 s, about 13 minutes**, on a
#   landing whose release half is ~25. So parity costs **roughly half again**, not the doubling the
#   dispatching brief allowed for — debug builds far faster than release and the suite's slower
#   execution does not make that back. (The same ordering CI uses; D1's clippy run and D5's test run
#   disagree about fingerprints, so D5 rebuilds after it, and the 731 s INCLUDES that rebuild.)
#
# ⚑ **SO D5 IS ON BY DEFAULT.** It is the step that went red on run 35418036061, on a landing this
#   command had called green; at thirteen minutes it is not worth being clever about. `--no-debug-
#   suite` skips D4 to D6 (D1 to D3 are seconds and always run); the report and the `VERDICT` file
#   then name them as gates that did not run — **a landing report that says which gates it did NOT
#   run is strictly better than one that implies it ran them all.**
#
# ⚑ **AND THE DEBUG ARM GETS ITS OWN G8.** D6 holds D5 to the same two-independent-counts standard
#   G8 holds G7 to. A debug arm without it would carry exactly the silent-false-green property G8
#   exists to remove, one profile over — a 13-minute run killed at 60 legs of 91 aggregates clean.
#
# **WHY RELEASE IS KEPT AS WELL, rather than replaced by the debug arm that matches CI.** Neither
# profile subsumes the other: debug compiles `debug_assertions` code that release does not, and
# release RUNS the three replay playthroughs that debug ignores (`#[cfg_attr(debug_assertions,
# ignore)]`). Dropping G5/G7 to buy parity cheaply would weaken a gate to make a different gate
# affordable, which is the trade this file exists to refuse.
#
# **WHAT IS STILL NOT RUN HERE, named rather than left to be discovered** — `tools/ci-parity.py
# --gaps-only` prints this on every landing, and the summary reprints it:
#
#   * `Fetch vendored corpora` (ABSENT) — ~726 MB and 838 HTTP requests. G1 requires the corpus to be
#     present and G1b re-verifies its bytes against the same pinned manifests the fetch scripts
#     write, so what CI proves by fetching, a landing proves by verifying.
#   * `Name the frozen aeon pin` (DIFFERS) — CI names the pin in debug, G6b names it in release.
#   * `System libraries … (alsa, udev)` (SETUP) — a landing must not run `apt`.
#   * and the debug suite itself (D4-D6) whenever `--no-debug-suite` was passed.
#
# ⚑ IT TAKES ABOUT 25 MINUTES AND CANNOT BE RUN IN THE FOREGROUND FROM AN AGENT SEAT.
#    An agent's Bash tool caps a foreground command at ~2 minutes, so a foreground run here is
#    KILLED partway through, and a killed run's log aggregates clean: it is a silent false green,
#    which is the one failure this whole script exists to prevent. Measured 2026-09-05, foreground,
#    killed at the cap. From an agent seat, detach and poll for a marker you wrote yourself:
#
#      # Write a RUN-UNIQUE WRAPPER that cd's here and calls THIS script in place:
#      #     #!/usr/bin/env bash
#      #     cd <worktree> && ./tools/land.sh >> "$LOG" 2>&1
#      #     echo "LAND-EXIT=$? AT $(date -Is)" >> "$LOG"
#      setsid nohup "$RUN/runner.sh" </dev/null >/dev/null 2>&1 &
#      # then poll $LOG for the LAND-EXIT marker. NEVER infer the verdict from a tail:
#      # this script prints its own verdict token, and the marker proves the run REACHED it.
#
#    ⚑ DO NOT COPY THIS SCRIPT ITSELF TO THE SCRATCHPAD. The run-unique-path rule exists
#      because bash reads a script incrementally by byte offset, so a concurrent writer to a
#      shared path resumes your execution inside the new bytes. It applies to the WRAPPER, and
#      applying it to this file breaks the run: line 169 derives ROOT from BASH_SOURCE, so a
#      copy living in /tmp makes ROOT the scratchpad's parent and every gate then measures the
#      wrong tree. Measured 2026-09-05: a scratchpad copy reported the scratchpad as the repo
#      root in its own header. This file is version-controlled and has one writer per worktree,
#      so it is not the concurrent-writer hazard the rule is about.
#
#    ⚑ A FRESH WORKTREE HAS NO vendor/ AND G1 REFUSES ON SIGHT. That is the guard working, not
#      a misconfiguration: without vendor/ the SingleStepTests sweep skips and passes vacuously.
#      From a worktree root, share the main checkout's fetch rather than re-downloading it:
#          ln -s /home/volence/sonic_hacks/oracle/vendor vendor
#      `vendor` is gitignored (.gitignore:7), so the symlink never dirties the tree G2 checks.
#
# ============================================================================================
# WHAT IT RUNS, AND WHAT IT REFUSES
# ============================================================================================
#
# Gates (in cost order, cheapest first, so a red is loud in seconds rather than in half an hour).
# A gate marked **[CI]** runs a command `.github/workflows/ci.yml` runs; `tools/ci-parity.py` is what
# keeps that claim true, and G0 refuses if it has gone stale. See CI PARITY above.
#
#   G0  CI parity inventory  [CI]  every `run:` step of every push-triggered workflow must be
#                                  classified in `tools/ci-parity.py`. An added, edited or removed
#                                  CI step reddens here, so this command cannot drift away from CI
#                                  in silence. Under a second.
#   G0b declared Rust floor  [CI]  `./tools/rust-floor.sh`, the same script all three CI jobs run to
#                                  pick their toolchain. It exits 1 on an unreadable floor, which on
#                                  a runner fails the step; nothing local ran it, so a broken floor
#                                  declaration was a CI-only red.
#   G1  vendor precondition        a fresh worktree has no `vendor/` symlink, and without it the
#                                  SingleStepTests sweep SKIPS AND PASSES VACUOUSLY. Its failure
#                                  mode is a silent green — exactly what a human-read checklist is
#                                  worst at and a script is best at. We also export `CI=1`, which
#                                  arms the six vacuity guards the suite already carries — since
#                                  2026-09-06 all SIX are named `vendor_data_present_when_running_
#                                  in_ci` tests (conformance_roms, scanline_goldens,
#                                  singlestep_m68000, singlestep_z80, scanline_capture, and
#                                  oracle-aether scanlines). The last two used to be inline-only
#                                  "skip locally, NEVER under CI" refusals; they still are, and a
#                                  standalone named guard was added BESIDE each so all six can be
#                                  reached by a name filter and shown in a log (`tools/ci-corpus-
#                                  guards.sh`, and the CI step of the same name). Those assert
#                                  against the test files' OWN ROM and opcode manifests, which is a
#                                  stronger statement than any path check this script could
#                                  hand-write.
#   G2  clean tree (a)             refuse a dirty tree BEFORE anything runs, listing the paths.
#                                  `docs/lane-status.json` is the one tolerated path — see below.
#   G2b lane files                 `docs/lane-status.json`, `docs/lane-log.jsonl` and (since
#                                  2026-09-06) `docs/decisions.jsonl` are parsed by a console that is
#                                  not in this repo, and until this gate NOTHING here validated them:
#                                  `git grep` finds the first two names in two files
#                                  (`crates/oracle-aether/tests/hosted.rs`, `src/server.rs`) and both
#                                  are doc-comment mentions. So a malformed entry landed clean and was
#                                  discovered by the owner's card going dark. `tools/lane-check.py`
#                                  parses every log line, checks the status document's shape and its
#                                  `state` vocabulary, refuses a future timestamp, and holds the
#                                  decision ledger's ids to being unique and its supersede links to
#                                  naming cards that exist. It runs on the WORKING TREE (what the
#                                  console reads) and on the COMMITTED blob (what the push
#                                  publishes), because the carve-out below lets those differ for
#                                  exactly one of the files.
#   G1b vendor bytes         [CI]  `./tools/verify-vendor.sh`, the same script CI runs
#                                  unconditionally on a cache hit and a miss alike. G1 only ever
#                                  checked that the directories were non-empty, which cannot see a
#                                  truncated corpus or one fetched from a different pin — and in a
#                                  worktree that symlinks a sibling's `vendor/`, the bytes belong to
#                                  whatever that sibling last fetched. 2 s.
#   G3  fast-forward               the tested SHA must be a descendant of the remote branch, so a
#                                  landing can never rewrite pushed history.
#   G4  cargo fmt --all --check  [CI]
#   G5  cargo clippy --workspace --all-targets --release -- -D warnings
#                                  `-D warnings` is not decoration. Without it clippy exits 0 on
#                                  every lint it finds, i.e. the gate cannot fire — measured: the
#                                  same `clippy::needless_return` that reddens G5 leaves the flagless
#                                  command at exit 0. The repo's own CI already denies warnings; a
#                                  local gate that did not would be weaker than the thing it is
#                                  meant to make unnecessary.
#   ---- THE DEBUG ARM (D1-D6), executed here, straight after G5. It is what CI runs. ----
#   D1  cargo clippy --all-targets -- -D warnings   (DEBUG)  [CI]
#                                  CI's exact clippy command, and it is NOT the same lint set as
#                                  G5: `cfg(debug_assertions)` code is compiled in one profile and
#                                  out of the other, so a lint inside a debug-only block is
#                                  invisible to the release pass. Both are kept; neither subsumes
#                                  the other. 19 s.
#   D2  corpus guards        [CI]  `./tools/ci-corpus-guards.sh`, the same script as CI's step of
#                                  that name, six `CORPUS GUARD ...: OK` banners or red. ⚑ Until
#                                  2026-09-19 G1 printed a note TELLING the reader they could run
#                                  this, and never ran it — a gate's advice standing in for the
#                                  gate, which is this parcel's subject in one line. 7 s.
#   D3  determinism gate     [CI]  `cargo test -p oracle-core --test determinism_gate --test
#                                  proptests -- --nocapture`, in DEBUG, byte-identical to CI's most
#                                  guarded job — the one every other CI job `needs:`. It had no
#                                  local counterpart at all. 31 s.
#   D4  debug leg count            derived exactly as G6 derives the release one, and for the same
#                                  reason; its `--no-run` build is the one D5 reuses.
#   D5  cargo test --workspace   (DEBUG)  [CI]
#                                  **THE MEASURED GAP.** CI's `Test` step, verbatim. Run
#                                  35418036061 went red here on a landing this command had called
#                                  green. 731 s / 91 legs measured. ON BY DEFAULT;
#                                  `--no-debug-suite` skips D4-D6 and the report says so in words.
#   D6  debug ran-to-the-end       G8's standard, applied to D5. Without it the debug arm carries
#                                  the silent-false-green property G8 exists to remove.
#   ---- end of the debug arm; the release gates resume ----
#   G6  expected leg count         DERIVED, never hardcoded — see the derivation section below.
#                                  It runs AFTER clippy so that its build is the one the suite
#                                  reuses; clippy and test disagree about workspace fingerprints,
#                                  and whichever runs second rebuilds them.
#   G7  cargo test --workspace --release
#                                  NOT `-p X -p Y`, which differs by feature unification: under
#                                  `--workspace`, oracle-player's `oracle-core/synth` edge unifies
#                                  onto oracle-core and 57 synth tests come with it. And release,
#                                  not debug: the three replay playthroughs are
#                                  `#[cfg_attr(debug_assertions, ignore = ...)]`, so only a release
#                                  run executes them.
#                                  The doc-bound gate (`overseer_bound.rs`) and the schema
#                                  conformance gate (`schema_conformance.rs`) are already workspace
#                                  test targets, so this runs them. They need no separate
#                                  invocation; what they need is proof they RAN, which is G8.
#   G8  ran-to-the-end             the load-bearing clause. Twice on 2026-09-05 a suite here was
#                                  killed partway and its log aggregated perfectly clean — once at
#                                  56 legs of 75 with zero failures, which is indistinguishable
#                                  from success in every summary except the leg count. Aeon's
#                                  version of the same finding: a matching md5 on a run that never
#                                  finished was the most convincing artifact of the night.
#                                  So: two independent counts of the log must BOTH equal the
#                                  derived expectation, and the aggregate failure count must be 0.
#   G9  HEAD did not move (c)
#   G10 tree still clean
#
# Then, and only then, the push:
#
#   (e) it pushes the TESTED SHA BY NAME, never the branch tip. This is the operative clause, not
#       (c). Refusing on a moved HEAD makes the failure loud; naming the SHA makes it impossible,
#       and only the second survives being tired.
#   (d) it reads the remote SHA before and after and says IN WORDS whether the push did anything,
#       because `git push` exits 0 on already-up-to-date and its exit code therefore cannot tell
#       "pushed" from "did nothing".
#   (b) on ANY refusal it pushes nothing AND then goes and looks: it re-reads the remote and states
#       that the ref did not move. A claim that we did not push is worth less than a measurement.
#
# It does NOT merge, does NOT commit, and does NOT write the lane log. Those stay deliberate acts.
#
# ============================================================================================
# THE `docs/lane-status.json` CARVE-OUT (the one thing (a) is relaxed for)
# ============================================================================================
#
# That file is tracked, is the lane's status board, and is edited continuously — a naive dirty
# check would refuse every landing this repo will ever attempt. It is tolerated, by name, and
# always PRINTED rather than passed over in silence. Two things make that safe rather than merely
# convenient:
#
#   1. Nothing compiled reads it, and that is checked rather than remembered: the suite compiles
#      only Rust, and `git grep -l lane-status -- '*.rs'` lists every Rust source that so much as
#      names the file. An empty answer is the condition this carve-out rests on; a non-empty one
#      means re-examine the carve-out before trusting it. (Scripts do name it — this one, to carve
#      it out, and `tools/lane-check.py`, to validate it — but neither feeds the suite's measurement.)
#   2. Because of (e) we push a COMMIT by name, so whatever the file says in the working tree is
#      never what reaches the remote. The tested tree and the pushed tree are the same object.
#
# Every other path — tracked or untracked — still refuses. The carve-out is one literal string,
# not a pattern, so it cannot quietly widen.
#
# ============================================================================================
# THE VALIDATING FAST PATH (G4 to G8 skipped, G2b made stricter instead)
# ============================================================================================
#
# A landing that publishes nothing but lane bookkeeping still costs 25 minutes of suite. That is
# the whole reason the fast path exists, and it is approved on one condition: **it must VALIDATE,
# not merely skip.**
#
#   * **The gate computes the path set; an author never declares it.** It is
#     `git diff --name-only $REMOTE_BEFORE $TESTED_SHA` — the NET difference between the tree the
#     remote already carries and the tree this push would publish. A flag or a commit-message
#     convention would be a claim; this is a measurement, and it cannot be wrong in the author's
#     favour.
#   * The fast path is taken **only** when that set is a non-empty subset of exactly
#     `{docs/lane-log.jsonl, docs/lane-status.json, docs/decisions.jsonl}`. Anything else runs the
#     full suite, **including any other file under `docs/`** — a design page is prose to us and
#     evidence to a peer, and "docs are safe" is exactly the reasoning that widens a carve-out until
#     it means nothing.
#   * The argument it rests on: those three files are read by no compiled thing here (the same fact
#     the carve-out above rests on), so G4 to G8 would be measuring, byte for byte, the tree the
#     remote already carried when that SHA landed.
#   * **What it does instead is more than it skips.** G2b runs on every landing, fast or full, and
#     it is the first thing in this repo ever to check these three files at all.
#
#   ⚑ **`docs/decisions.jsonl` joined the set on 2026-09-06, on the hub's ruling, and it joined the
#   VALIDATOR in the same commit.** That order is the whole permission: the file is in scope because
#   `tools/lane-check.py` now checks it to the same standard as the other two (every line parses, the
#   corpus-derived required keys, no future `at`), plus two rules about the ids that link the cards —
#   every id appears exactly once, and every non-null `supersedes` names an id that exists and is not
#   the card's own. Those two came from a defect that had already happened: two cards were filed under
#   ids that were already taken, and one of them superseded an id that then named two different cards.
#   **It is the validator's scope, not a carve-out**, and a landing that widened the set without
#   widening the checker would be the second thing rather than the first.
#
# It changes nothing about G1, G2, G2b, G3, G9, G10 or the push: the tested SHA is still pushed by
# name, the remote is still read back, and a dirty or moved tree still refuses.
#
# ============================================================================================
# HOW THE EXPECTED LEG COUNT IS DERIVED  (G4)
# ============================================================================================
#
# A "leg" is one `Running .../Doc-tests ...` unit of a `cargo test` run. The count must be derived
# at run time, so that adding a crate or a test file updates it by itself: an expected count that
# is wrong in the safe direction is a gate that can never fire, which is the entire failure mode
# this command exists to prevent, and one wrong the other way is a gate nobody can land through.
#
# The naive derivation — count lib/bin/test targets from `cargo metadata --no-deps` and add one
# doc-test leg per lib-bearing package — gives 69 + 4 = 73 against a measured 75. The missing 2 are
# `oracle-core`'s `motion_run` and `ab_compare` EXAMPLES, which carry `test = true` in
# `crates/oracle-core/Cargo.toml` and are therefore run by `cargo test` like any other target. An
# example is invisible to a target-kind headcount that only looks at lib/bin/test, which is why the
# naive number was short by exactly two.
#
# Rather than patch the headcount with an examples clause and hope the next surprise is also an
# example, this asks cargo itself:
#
#   * the RUNNABLE legs come from `cargo test --workspace --release --no-run --message-format=json`,
#     counting the distinct executables cargo built with `profile.test == true`. That is cargo's own
#     target selection and its own feature resolution — including `required-features` (the
#     `oracle-frontend` bin needs `window`; `synth_render` needs `synth`) — rather than this script's
#     second reading of the manifests.
#   * the DOC-TEST legs come from `cargo metadata --no-deps`: one per lib target with
#     `doctest == true`. Those legs have no executable, so they are the one part cargo will not hand
#     us as an artifact.
#
# It is not a free step: the `--no-run` build is the compile the suite needs anyway, so G4 warms the
# cache that G7 then reuses.
#
# ============================================================================================
set -uo pipefail

REMOTE=origin
BRANCH=main
DO_PUSH=1
# G7d, CI's debug suite. ON by default: it is the arm that predicts CI, and the one that caught
# nothing here because it never ran. Skipping it is a choice a reader of the report can SEE.
DO_DEBUG_SUITE=1

while [ $# -gt 0 ]; do
    case "$1" in
        --remote)  REMOTE="${2:?--remote needs a value}"; shift 2 ;;
        --branch)  BRANCH="${2:?--branch needs a value}"; shift 2 ;;
        --no-push) DO_PUSH=0; shift ;;
        --no-debug-suite) DO_DEBUG_SUITE=0; shift ;;
        # Print the WHOLE header comment, however long it grows. A fixed line range is the same
        # class of stale copy as everything else this file now guards against: the range was
        # '2,120p' and the header had passed 200 lines.
        -h|--help) command awk 'NR>1 && /^#/ {print; next} NR>1 {exit}' "$0"; exit 0 ;;
        *) echo "land: unknown argument '$1' (see --help)" >&2; exit 2 ;;
    esac
done

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" || exit 2
cd "$ROOT" || exit 2

# The suite's own vendor guards are no-ops without this. See G1.
export CI=1

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
RUN_DIR="$ROOT/target/land/$STAMP"
mkdir -p "$RUN_DIR" || exit 2

# --------------------------------------------------------------------------------------------
# reporting
# --------------------------------------------------------------------------------------------
FAILURES=()
# Set for real at the FAST PATH block. Initialised here because the VERDICT writer and the summary
# both read it, and `set -u` turns an early refusal into an unbound-variable crash otherwise.
FAST=0
CI_GAPS=""
# ⚑ How far the debug arm ACTUALLY got: 0 none, 1 D1-D3, 2 D1-D6. Set where the gates run, never
#   inferred from `$DO_DEBUG_SUITE` -- a refusal at G0, G1 or G2b happens BEFORE D1 exists, and a
#   VERDICT that read the flag would there record `debug_arm=D1-D6 ran` for a run that reached no
#   D gate at all. (Found by reading the refusal path of this very change; it is the same defect
#   the change exists to remove, one artifact smaller.)
DEBUG_ARM=0
D_LEGS_HEADER=""
D_EXPECTED_LEGS=""
D_PASS=""
D_FAIL=0
D_IGN=""
TESTED_SHA=""
REMOTE_BEFORE=""
# Whether G3 has actually read the remote yet. An empty `REMOTE_BEFORE` means two different things
# and only one of them is "no such ref"; see `verify_remote_unmoved`.
REMOTE_READ=0

hr()   { echo "------------------------------------------------------------------------------"; }
pass() { echo "  PASS  $*"; }
fail() { echo "  RED   $*"; FAILURES+=("$*"); }
note() { echo "        $*"; }

# The VERDICT file is what a polling agent reads instead of the log, so the gaps go IN IT. A
# machine-read end marker that records only what passed reproduces, in a smaller artifact, exactly
# the belief this parcel removes: that a green names its coverage.
write_verdict_ci_parity() {
    case "$DEBUG_ARM" in
        2) echo "debug_arm=D1-D6 ran, in the profile CI uses" ;;
        1) echo "debug_arm=D1-D3 ran; D4-D6 did NOT: cargo test --workspace in DEBUG, which is the Test step of CI, was not run$([ "$DO_DEBUG_SUITE" = 0 ] && echo ' (--no-debug-suite)')" ;;
        *) echo "debug_arm=NONE ran$([ "$FAST" = 1 ] && echo ' (fast path: lane bookkeeping only)' || echo ' (this run ended before the debug arm)')" ;;
    esac
    # Same per-run list as the report; see the note beside it. `$CI_GAPS` (captured at G0, before
    # any D gate could have run) is deliberately NOT what goes in the marker.
    #
    # ⚑ FILTERED TO THE THREE CLASSIFICATION WORDS, and that is not tidying. This function also
    #   runs from `finish_red`, INCLUDING on a G0 refusal — the refusal that means ci-parity itself
    #   is unhappy — and on that path the tool prints its `BAD ...` findings and a `N finding(s)`
    #   summary alongside the gaps. Unfiltered, those became `ci_gap=` rows: a machine-read marker
    #   naming gaps that are not gaps, on the one run where a reader most needs the marker to be
    #   exact. Anything the tool says that is not a classified row belongs in the log, which
    #   `finish_red` has already printed in full.
    local ran=""
    [ "$DEBUG_ARM" = 2 ] && ran="D5"
    ./tools/ci-parity.py --gaps-only --ran "$ran" 2>/dev/null \
        | command grep -E '^[[:space:]]*(DIFFERS|ABSENT|CONDITIONAL)[[:space:]]' \
        | while IFS= read -r l; do
            echo "ci_gap=$(printf '%s' "$l" | command sed -e 's/^ *//')"
        done
}

# (b), and it is a measurement rather than a claim: whenever we end without pushing, go and read
# the remote back and say what it is.
verify_remote_unmoved() {
    local now
    now="$(git ls-remote "$REMOTE" "refs/heads/$BRANCH" 2>/dev/null | command awk '{print $1}')"
    hr
    echo "(b) PUSHED NOTHING — verifying that against the remote rather than asserting it:"
    # ⚑ A refusal BEFORE G3 has never read the remote, so `$REMOTE_BEFORE` is empty for want of a
    # measurement rather than for want of a ref — and the comparison below would then read a perfectly
    # ordinary ref as one that MOVED, which is a false alarm on the loudest line this script prints.
    # Measured 2026-09-05 on a G2b refusal, which is the gate that made an early refusal common.
    if [ "$REMOTE_READ" = 0 ]; then
        note "this run refused before it read $REMOTE/$BRANCH, so there is no 'before' to compare."
        note "$REMOTE/$BRANCH now            : ${now:-<no such ref>}"
        note "nothing from this run reached any remote: no push was attempted."
        return
    fi
    note "$REMOTE/$BRANCH before the run : ${REMOTE_BEFORE:-<no such ref>}"
    note "$REMOTE/$BRANCH now            : ${now:-<no such ref>}"
    if [ "$now" = "$REMOTE_BEFORE" ]; then
        note "the ref did not move. Nothing from this run reached the remote."
    else
        note "*** THE REF MOVED AND THIS RUN DID NOT PUSH IT. Someone else did. Investigate. ***"
    fi
    if [ -n "$TESTED_SHA" ] && [ "$now" = "$TESTED_SHA" ]; then
        note "note: the remote already carried the tested SHA before this run began; that is not"
        note "      this run having pushed it (the ref is unchanged from 'before' above)."
    fi
}

finish_red() {
    echo
    hr
    echo "LANDING REFUSED — $((${#FAILURES[@]})) gate(s) red:"
    local f
    for f in "${FAILURES[@]}"; do echo "  * $f"; done
    [ -n "$REMOTE_BEFORE$TESTED_SHA" ] && verify_remote_unmoved
    hr
    {
        echo "verdict=RED"
        echo "tested_sha=$TESTED_SHA"
        for f in "${FAILURES[@]}"; do echo "failure=$f"; done
        write_verdict_ci_parity
    } > "$RUN_DIR/VERDICT"
    echo "land: RED. Run artifacts in $RUN_DIR (end marker: $RUN_DIR/VERDICT)."
    exit 1
}

echo "=============================================================================="
echo "land.sh — $STAMP — $ROOT"
echo "  remote/branch : $REMOTE/$BRANCH"
echo "  push          : $([ "$DO_PUSH" = 1 ] && echo yes || echo 'no (--no-push)')"
echo "  debug suite   : $([ "$DO_DEBUG_SUITE" = 1 ] && echo 'yes (G7d, CI parity)' || echo 'NO (--no-debug-suite) — the report will name it as a gate that did not run')"
echo "  run artifacts : $RUN_DIR"
echo "=============================================================================="

# --------------------------------------------------------------------------------------------
# G0  CI parity inventory
#
# FIRST, and it costs a fifth of a second. Its job is not to run a gate but to establish that the
# rest of this file's claims about CI are still true: `tools/ci-parity.py` reads the workflows and
# requires every `run:` step of every push-triggered one to be classified, pinned by a digest of the
# step body. An added step, an edited step or a deleted step reddens HERE rather than on a runner
# forty minutes after a green landing.
#
# ⚑ Its exit 0 means the difference between this command and CI is WRITTEN DOWN. It does not mean
#   there is none. The gaps it names are printed here and again in the summary.
# --------------------------------------------------------------------------------------------
hr; echo "G0  CI parity inventory (derived from .github/workflows, classified in tools/ci-parity.py)"
if ./tools/ci-parity.py --report > "$RUN_DIR/ci-parity.log" 2>&1; then
    pass "G0 $(command tail -1 "$RUN_DIR/ci-parity.log")"
else
    fail "G0 the CI coverage map is STALE: CI runs a step this command cannot account for"
    command cat "$RUN_DIR/ci-parity.log"
    finish_red
fi
# Captured now so the summary can reprint it after twenty-five minutes of suite, when nobody is
# going to scroll back for it.
# The STATIC list, for the banner here: every gap plus every CONDITIONAL step, because at G0 no D
# gate has run and nothing yet knows which way this landing will go. The report at the END
# recomputes it with `--ran`, and THAT one is the per-run measurement. Kept separate on purpose —
# this line is "what could be missing", the other is "what was".
CI_GAPS="$(./tools/ci-parity.py --gaps-only 2>/dev/null)"
note "CI steps this command may not run (statically; the report at the end measures this run):"
printf '%s\n' "$CI_GAPS" | while IFS= read -r l; do [ -n "$l" ] && note "$l"; done

# --------------------------------------------------------------------------------------------
# G0b  the declared Rust floor  [CI: "Read the declared Rust floor", all three jobs]
#
# CI reads it three times to pick its toolchain, and `v=$(./tools/rust-floor.sh)` under `bash -e`
# fails the step on an empty or failing read. Nothing local ran it, so an unreadable floor
# declaration was a CI-only red — and it is the one gate in this whole file that costs nothing.
# --------------------------------------------------------------------------------------------
hr; echo "G0b declared Rust floor (./tools/rust-floor.sh — the same script all three CI jobs run)"
if RUST_FLOOR="$(./tools/rust-floor.sh 2>"$RUN_DIR/rust-floor.err")" && [ -n "$RUST_FLOOR" ]; then
    pass "G0b the floor reads as $RUST_FLOOR"
    note "this landing's toolchain: $(rustc --version 2>/dev/null || echo 'rustc not on PATH')"
else
    fail "G0b ./tools/rust-floor.sh produced no floor; on CI this fails the step in all three jobs"
    command cat "$RUN_DIR/rust-floor.err"
    finish_red
fi

# --------------------------------------------------------------------------------------------
# G1  vendor precondition
# --------------------------------------------------------------------------------------------
hr; echo "G1  vendor precondition"
VENDOR_OK=1
for need in vendor/ProcessorTests/68000/v1 vendor/ProcessorTests/z80/v1 vendor/TestRoms; do
    if [ -d "$ROOT/$need" ]; then
        n="$(command find -L "$ROOT/$need" -maxdepth 1 -type f | command wc -l)"
        if [ "$n" -eq 0 ]; then
            fail "G1 vendor: $need exists but is EMPTY"; VENDOR_OK=0
        else
            pass "G1 $need ($n files)"
        fi
    else
        fail "G1 vendor: $need is MISSING"; VENDOR_OK=0
    fi
done
if [ "$VENDOR_OK" = 0 ]; then
    note "A fresh worktree has no vendor/. Without it the SingleStepTests sweep SKIPS and the"
    note "suite passes VACUOUSLY. Fix, from the repo root:"
    note "    ln -s /home/volence/sonic_hacks/oracle/vendor vendor      # a worktree: share the fetch"
    note "    ./tools/fetch-tests.sh && ./tools/fetch-z80-tests.sh && ./tools/fetch-testroms.sh"
    finish_red
fi
note "CI=1 exported: the suite's six in-built vacuity guards are armed (six named guard tests, two"
note "of which also keep their original inline skip-refusal), so a present-but-INCOMPLETE vendor"
note "corpus reddens from inside too. They are RUN, by name, at D2 — see the note there about what"
note "this line used to be."

# --------------------------------------------------------------------------------------------
# G1b  vendor BYTES  [CI: "Verify vendored corpora against their pinned manifests"]
#
# The same script CI runs, unconditionally, on a cache hit and a miss alike. G1 above answers "is
# there a corpus"; this answers "is it the corpus we pinned", and those are different questions —
# a truncated download, a corrupt archive, or a fetch from a different upstream pin all satisfy G1.
#
# It matters MORE here than on a runner. The documented way to give a worktree a corpus is
# `ln -s .../oracle/vendor vendor` (see the header), so the bytes a landing measures belong to
# whatever the main checkout last fetched — a state no runner can be in and no gate here could see.
# Two seconds, measured.
# --------------------------------------------------------------------------------------------
hr; echo "G1b vendor bytes (./tools/verify-vendor.sh — CI's own script, against the pinned manifests)"
if ./tools/verify-vendor.sh > "$RUN_DIR/verify-vendor.log" 2>&1; then
    pass "G1b the corpus matches all three pinned sha256 manifests"
    note "$(command tail -1 "$RUN_DIR/verify-vendor.log")"
else
    fail "G1b the vendored corpus does NOT match its pinned manifests; the suite below would measure the wrong bytes"
    command tail -25 "$RUN_DIR/verify-vendor.log"
    finish_red
fi

# --------------------------------------------------------------------------------------------
# G2  clean tree  (aurora (a))
# --------------------------------------------------------------------------------------------
hr; echo "G2  clean tree"
TOLERATED_DIRTY="docs/lane-status.json"

collect_dirt() {   # -> BLOCKING[], TOLERATED[]
    BLOCKING=(); TOLERATED=()
    local line path
    while IFS= read -r line; do
        [ -z "$line" ] && continue
        path="${line:3}"
        path="${path##* -> }"          # renames: keep the destination
        path="${path%\"}"; path="${path#\"}"
        if [ "$path" = "$TOLERATED_DIRTY" ]; then TOLERATED+=("$line"); else BLOCKING+=("$line"); fi
    done < <(git status --porcelain)
}

collect_dirt
if [ "${#TOLERATED[@]}" -gt 0 ]; then
    note "tolerated dirty path (named carve-out; nothing compiled reads it, and (e) means its"
    note "working-tree content never reaches the remote):"
    for l in "${TOLERATED[@]}"; do note "    $l"; done
fi
if [ "${#BLOCKING[@]}" -gt 0 ]; then
    fail "G2 the tree is dirty (${#BLOCKING[@]} path(s)); a landing must test the tree it pushes"
    for l in "${BLOCKING[@]}"; do note "    $l"; done
    finish_red
fi
pass "G2 no blocking modifications"

TESTED_SHA="$(git rev-parse HEAD)"
BRANCH_NAME="$(git rev-parse --abbrev-ref HEAD)"
note "tested SHA : $TESTED_SHA  (on $BRANCH_NAME)"

# --------------------------------------------------------------------------------------------
# G2b  the lane files the console reads
#
# Runs on EVERY landing, fast path or full. Nothing in this repo validated these two files before
# this gate, so a malformed entry landed clean and was found by the owner's board going dark.
# --------------------------------------------------------------------------------------------
hr; echo "G2b lane files (docs/lane-status.json, docs/lane-log.jsonl, docs/decisions.jsonl)"
LANE_STATUS="docs/lane-status.json"
LANE_LOG="docs/lane-log.jsonl"
LANE_DECISIONS="docs/decisions.jsonl"
LANE_OK=1

# (i) the working tree: the copy the console actually reads, and the one the G2 carve-out lets
#     differ from the commit.
if ./tools/lane-check.py --status "$ROOT/$LANE_STATUS" --log "$ROOT/$LANE_LOG" \
        --decisions "$ROOT/$LANE_DECISIONS" \
        --label "working tree" > "$RUN_DIR/lane-worktree.log" 2>&1; then
    pass "G2b working tree: $(command tail -1 "$RUN_DIR/lane-worktree.log")"
else
    fail "G2b the lane files in the working tree are malformed"
    command cat "$RUN_DIR/lane-worktree.log"
    LANE_OK=0
fi

# (ii) the committed blobs: what (e) actually publishes. The two are the same file whenever the
#      carve-out is not in play, and the check costs milliseconds either way.
git show "$TESTED_SHA:$LANE_STATUS"    > "$RUN_DIR/lane-status.committed.json" 2>/dev/null || true
git show "$TESTED_SHA:$LANE_LOG"       > "$RUN_DIR/lane-log.committed.jsonl"  2>/dev/null || true
git show "$TESTED_SHA:$LANE_DECISIONS" > "$RUN_DIR/decisions.committed.jsonl" 2>/dev/null || true
if ./tools/lane-check.py \
        --status    "$RUN_DIR/lane-status.committed.json" \
        --log       "$RUN_DIR/lane-log.committed.jsonl" \
        --decisions "$RUN_DIR/decisions.committed.jsonl" \
        --label "as committed at ${TESTED_SHA:0:12}" > "$RUN_DIR/lane-committed.log" 2>&1; then
    pass "G2b committed: $(command tail -1 "$RUN_DIR/lane-committed.log")"
else
    fail "G2b the lane files AS COMMITTED are malformed; this is what a push would publish"
    command cat "$RUN_DIR/lane-committed.log"
    LANE_OK=0
fi
[ "$LANE_OK" = 0 ] && finish_red

# ⚑ G2b's green bounds only what `lane-check.py` checks, and on 2026-09-19 that was measured to be
#   far less than it was believed to be — it printed "clean" over two `next` rows, five `open` rows
#   with blockers, and a 126-character `focus`, all in one night. It is wider now, and the rules it
#   still does NOT enforce are printed here rather than left to be assumed. The `clean` line above
#   names the CONTRACT REVISION those rules were copied from, so this log says which contract was
#   being enforced without anyone opening the validator.
./tools/lane-check.py --gaps 2>/dev/null | while IFS= read -r l; do note "$l"; done

# --------------------------------------------------------------------------------------------
# G3  fast-forward
# --------------------------------------------------------------------------------------------
hr; echo "G3  fast-forward onto $REMOTE/$BRANCH"
REMOTE_BEFORE="$(git ls-remote "$REMOTE" "refs/heads/$BRANCH" 2>/dev/null | command awk '{print $1}')"
REMOTE_READ=1
if [ -z "$REMOTE_BEFORE" ]; then
    pass "G3 $REMOTE/$BRANCH does not exist yet; any push creates it"
elif [ "$REMOTE_BEFORE" = "$TESTED_SHA" ]; then
    pass "G3 $REMOTE/$BRANCH is already at the tested SHA"
elif git merge-base --is-ancestor "$REMOTE_BEFORE" "$TESTED_SHA"; then
    pass "G3 tested SHA is a descendant of $REMOTE_BEFORE"
else
    fail "G3 tested SHA $TESTED_SHA is NOT a descendant of $REMOTE/$BRANCH ($REMOTE_BEFORE) — a push would rewrite pushed history"
    finish_red
fi

# --------------------------------------------------------------------------------------------
# THE FAST PATH — computed here, never declared by an author
#
# The path set is the NET diff between what the remote already carries and what this push would
# publish. See the header section for the argument and for why every other path under docs/ still
# runs the full suite.
# --------------------------------------------------------------------------------------------
hr; echo "FAST PATH  what this landing would publish"
FAST=0
FASTPATH_ALLOWED="docs/lane-log.jsonl docs/lane-status.json docs/decisions.jsonl"
if [ -z "$REMOTE_BEFORE" ]; then
    note "no such remote ref yet, so there is no 'already carried' tree to compare against"
elif [ "$REMOTE_BEFORE" = "$TESTED_SHA" ]; then
    note "the remote already carries the tested SHA; nothing is being published"
else
    LANDED_PATHS="$(git diff --name-only "$REMOTE_BEFORE" "$TESTED_SHA")"
    LANDED_N="$(printf '%s\n' "$LANDED_PATHS" | command grep -c . )"
    note "git diff --name-only $REMOTE_BEFORE $TESTED_SHA  ->  $LANDED_N path(s)"
    printf '%s\n' "$LANDED_PATHS" | while IFS= read -r p; do [ -n "$p" ] && note "    $p"; done
    if [ "$LANDED_N" -gt 0 ]; then
        OUTSIDE=0
        while IFS= read -r p; do
            [ -z "$p" ] && continue
            case " $FASTPATH_ALLOWED " in
                *" $p "*) ;;
                *) OUTSIDE=$((OUTSIDE + 1)) ;;
            esac
        done <<EOF
$LANDED_PATHS
EOF
        if [ "$OUTSIDE" -eq 0 ]; then
            FAST=1
        else
            note "$OUTSIDE path(s) outside { $FASTPATH_ALLOWED }"
        fi
    fi
fi
if [ "$FAST" = 1 ]; then
    pass "FAST PATH taken: this landing publishes lane bookkeeping and nothing else"
    note "G2b has already VALIDATED both files, in the working tree and as committed. G4 to G8"
    note "would measure, byte for byte, the tree $REMOTE/$BRANCH already carries."
else
    pass "FULL SUITE: this landing publishes something the suite has to measure"
fi

if [ "$FAST" = 0 ]; then
# --------------------------------------------------------------------------------------------
# G4  fmt
# --------------------------------------------------------------------------------------------
hr; echo "G4  cargo fmt --all --check"
if cargo fmt --all --check > "$RUN_DIR/fmt.log" 2>&1; then
    pass "G4 formatting clean"
else
    fail "G4 cargo fmt --all --check is RED"
    command head -40 "$RUN_DIR/fmt.log"
    finish_red
fi

# --------------------------------------------------------------------------------------------
# G5  clippy
# --------------------------------------------------------------------------------------------
hr; echo "G5  cargo clippy --workspace --all-targets --release -- -D warnings"
if cargo clippy --workspace --all-targets --release -- -D warnings > "$RUN_DIR/clippy.log" 2>&1; then
    pass "G5 clippy clean under -D warnings"
else
    fail "G5 cargo clippy is RED"
    command grep -E '^(error|warning)' "$RUN_DIR/clippy.log" | command head -40
    finish_red
fi

# ============================================================================================
# THE DEBUG ARM — D1 to D6. THIS IS WHAT CI RUNS.
#
# Everything above and below this block is the release profile. CI's `build-test-lint` job and its
# `determinism-gate` job are DEBUG, and that difference is not cosmetic: it is why a landing green
# here went red on run 35418036061 minutes later. See the CI PARITY section of the header for the
# three measured instances and for the price.
#
# Placed here, straight after G5, on this file's own cost-order rule: D1 to D3 are 57 seconds
# between them and D5 is 731 s against G7's ~25 minutes, so a debug-only red is loud in a minute
# rather than after the release suite has run.
# ============================================================================================

# --------------------------------------------------------------------------------------------
# D1  clippy, DEBUG  [CI: "Clippy (deny warnings)"]
#
# CI's command verbatim, and it is a DIFFERENT LINT SET from G5 rather than a weaker spelling of
# it: `cfg(debug_assertions)` code is compiled in this profile and out of the release one, so a
# lint inside a debug-only block cannot be seen by G5 and one inside a release-only block cannot be
# seen here. Both are kept for that reason.
#
# Note the missing `--workspace`: this is CI's exact string. At the workspace root cargo's default
# member selection is the workspace, so it selects the same crates; the difference that matters is
# the profile, and copying the command rather than improving it is the point of a parity gate.
# --------------------------------------------------------------------------------------------
hr; echo "D1  cargo clippy --all-targets -- -D warnings   (DEBUG — CI's exact command)"
if cargo clippy --all-targets -- -D warnings > "$RUN_DIR/clippy-debug.log" 2>&1; then
    pass "D1 debug clippy clean under -D warnings"
else
    fail "D1 cargo clippy (DEBUG) is RED — this is the profile CI lints in"
    command grep -E '^(error|warning)' "$RUN_DIR/clippy-debug.log" | command head -40
    finish_red
fi

# --------------------------------------------------------------------------------------------
# D2  corpus guards  [CI: "Corpus guards (name them in the log)"]
#
# ⚑ THE SHARPEST OF THE NEW GATES, because of what stood here before it. G1 printed:
#       "To see them by name: ./tools/ci-corpus-guards.sh"
#   — a gate TELLING its reader to run the check, in a file whose entire premise is that a
#   checklist is the artifact that drifts. CI has run this script on every push since 2026-09-06.
#
# The script's own contract: six `CORPUS GUARD ...: OK` banners or red, because a libtest filter
# that matches nothing exits 0 with "0 passed". It needs `CI` set, which G1 exports.
# --------------------------------------------------------------------------------------------
hr; echo "D2  corpus guards (./tools/ci-corpus-guards.sh — CI's own script; six banners or red)"
if ./tools/ci-corpus-guards.sh > "$RUN_DIR/corpus-guards.log" 2>&1; then
    pass "D2 $(command grep -c 'CORPUS GUARD .*: OK' "$RUN_DIR/corpus-guards.log") guard(s) verified and NAMED in $RUN_DIR/corpus-guards.log"
else
    fail "D2 the corpus guards are RED: the vendored corpora are incomplete, and every suite green below would be vacuous"
    command tail -30 "$RUN_DIR/corpus-guards.log"
    finish_red
fi

# --------------------------------------------------------------------------------------------
# D3  determinism gate + invariant proptests, DEBUG  [CI: determinism-gate job]
#
# CI's most-guarded job: `build-test-lint` and `replay-playthroughs` both `needs:` it, so on a
# runner nothing else even starts unless this holds. It had NO local counterpart — not a weaker
# one, none — so the one CI job a landing could most cheaply rehearse was the one it never touched.
# `--nocapture`, as CI runs it, because these print what they proved.
# --------------------------------------------------------------------------------------------
hr; echo "D3  determinism gate + invariant proptests (DEBUG, --nocapture — CI's most-guarded job)"
cargo test -p oracle-core --test determinism_gate --test proptests -- --nocapture \
    > "$RUN_DIR/determinism-debug.log" 2>&1
DET_STATUS=$?
if [ "$DET_STATUS" -eq 0 ]; then
    pass "D3 determinism holds in debug ($(command grep -cE '^test result: ' "$RUN_DIR/determinism-debug.log") leg(s))"
    DEBUG_ARM=1
else
    fail "D3 the determinism gate is RED in debug (exit $DET_STATUS). On CI nothing else runs at all when this fails"
    command tail -40 "$RUN_DIR/determinism-debug.log"
    finish_red
fi

if [ "$DO_DEBUG_SUITE" = 1 ]; then
# --------------------------------------------------------------------------------------------
# D4  the DEBUG leg count, derived
#
# Derived, never hardcoded, for the reason G6 gives at length — and derived SEPARATELY from G6's
# release number rather than reused, because the two profiles do not run the same set: the three
# replay playthroughs are `#[cfg_attr(debug_assertions, ignore)]`, and `required-features` can
#
# ⚑ THE SETS DIFFER EVEN WHEN THE COUNTS DO NOT, and this comment first claimed otherwise. It said
#   "91 debug legs against 75 release legs" — 75 taken from this file's own older header rather
#   than from a run. Measured on the landing of this commit: **both derive 91**. The reason to
#   derive separately is therefore NOT that the numbers differ today; it is that the SELECTIONS do,
#   and nothing keeps them equal. A shared expectation would be a coincidence that a new
#   debug-ignored test silently ends, in the direction that makes D6 unable to fire.
#
# Its `--no-run` build is the one D5 reuses, exactly as G6's warms G7.
# --------------------------------------------------------------------------------------------
hr; echo "D4  expected DEBUG leg count (derived, never hardcoded)"
cargo test --workspace --no-run --message-format=json \
    > "$RUN_DIR/norun-debug.json" 2> "$RUN_DIR/norun-debug.err"
DNORUN_STATUS=$?
if [ "$DNORUN_STATUS" -ne 0 ]; then
    fail "D4 the debug --no-run build failed (status $DNORUN_STATUS); see $RUN_DIR/norun-debug.err"
    command tail -40 "$RUN_DIR/norun-debug.err"
    finish_red
fi
D_EXEC_LEGS="$(jq -r 'select(.reason == "compiler-artifact")
                      | select(.profile.test == true)
                      | select(.executable != null)
                      | .executable' "$RUN_DIR/norun-debug.json" | command sort -u | command wc -l)"
D_DOC_LEGS="$(cargo metadata --no-deps --format-version 1 \
              | jq '[.packages[].targets[] | select(.kind | index("lib")) | select(.doctest == true)] | length')"
if ! [ "$D_EXEC_LEGS" -gt 0 ] 2>/dev/null || ! [ "$D_DOC_LEGS" -gt 0 ] 2>/dev/null; then
    fail "D4 could not derive a debug leg count (exec=$D_EXEC_LEGS doc=$D_DOC_LEGS)"
    finish_red
fi
D_EXPECTED_LEGS=$((D_EXEC_LEGS + D_DOC_LEGS))
pass "D4 expected debug legs = $D_EXPECTED_LEGS  ($D_EXEC_LEGS test executables + $D_DOC_LEGS doc-test legs)"

# --------------------------------------------------------------------------------------------
# D5  the suite, DEBUG  [CI: "Test"]
#
# **THE STEP THIS WHOLE PARCEL IS ABOUT.** `cargo test --workspace`, no profile flag, exactly as
# CI's `Test` step spells it. Measured 2026-09-19: 731 s, 91 legs, 3058 passed / 0 failed / 10
# ignored on a warm tree.
# --------------------------------------------------------------------------------------------
hr; echo "D5  cargo test --workspace   (DEBUG — CI's exact command; expect $D_EXPECTED_LEGS legs; ~12 min)"
echo "    started $(date -u +%Y-%m-%dT%H:%M:%SZ)"
DSUITE_T0=$(date +%s)
cargo test --workspace 2>&1 | command tee "$RUN_DIR/suite-debug.log"
DSUITE_STATUS=${PIPESTATUS[0]}          # never $? through a pipe
DSUITE_T1=$(date +%s)
echo "    finished $(date -u +%Y-%m-%dT%H:%M:%SZ)  ($((DSUITE_T1 - DSUITE_T0)) s, exit $DSUITE_STATUS)"
if [ "$DSUITE_STATUS" -eq 0 ]; then
    pass "D5 cargo test (debug) exited 0"
else
    fail "D5 cargo test (DEBUG) exited $DSUITE_STATUS — this is the command CI runs, so CI will be red"
fi

# --------------------------------------------------------------------------------------------
# D6  the debug arm ran to the END
#
# G8's clause, applied to D5, and not optional: a 13-minute run killed at 60 legs of 91 aggregates
# perfectly clean, which is the exact artifact G8 was written after twice seeing.
# --------------------------------------------------------------------------------------------
hr; echo "D6  ran-to-the-end (debug)"
D_LEGS_HEADER="$(command grep -cE '^[[:space:]]{1,10}(Running|Doc-tests) ' "$RUN_DIR/suite-debug.log")"
D_LEGS_RESULT="$(command grep -cE '^test result: ' "$RUN_DIR/suite-debug.log")"
read -r D_PASS D_FAIL D_IGN < <(command awk '
    /^test result: /{
        for (i = 1; i <= NF; i++) {
            if ($(i+1) ~ /^passed/)  p += $i;
            if ($(i+1) ~ /^failed/)  f += $i;
            if ($(i+1) ~ /^ignored/) g += $i;
        }
    }
    END { printf "%d %d %d\n", p, f, g }' "$RUN_DIR/suite-debug.log")
note "legs by 'Running'/'Doc-tests' header : $D_LEGS_HEADER"
note "legs by 'test result:' line          : $D_LEGS_RESULT"
note "expected                             : $D_EXPECTED_LEGS"
note "aggregate (debug)                    : $D_PASS passed, $D_FAIL failed, $D_IGN ignored"
if [ "$D_LEGS_HEADER" -eq "$D_EXPECTED_LEGS" ] && [ "$D_LEGS_RESULT" -eq "$D_EXPECTED_LEGS" ]; then
    pass "D6 both counts equal the derived expectation — the debug run reached the end"
else
    fail "D6 debug leg count MISMATCH (header $D_LEGS_HEADER, result $D_LEGS_RESULT, expected $D_EXPECTED_LEGS): the debug run did NOT reach the end"
fi
if [ "$D_FAIL" -eq 0 ]; then
    pass "D6 debug aggregate failures = 0"
    DEBUG_ARM=2
else
    fail "D6 $D_FAIL test failure(s) in the debug aggregate"
    command grep -E '^(failures:|    [a-z_].*::)' "$RUN_DIR/suite-debug.log" | command head -30
fi

else
    # ⚑ `skipped`, never `0`. A zero here would be a MEASUREMENT of a suite that never ran, which is
    # the same untruth the fast path's own accounting refuses one page down and G8 refuses one page
    # up. The summary and the VERDICT file read these names.
    D_LEGS_HEADER=skipped
    D_EXPECTED_LEGS=skipped
    D_PASS=skipped; D_FAIL=0; D_IGN=skipped
    hr; echo "D4-D6 SKIPPED (--no-debug-suite)"
    note "cargo test --workspace (DEBUG) was NOT run. It is the command CI's 'Test' step runs and"
    note "the one that went red on run 35418036061 after a green landing here, so this landing"
    note "makes NO claim about whether CI will pass. D1 to D3 did run."
fi

# --------------------------------------------------------------------------------------------
# G6  derive the expected leg count
#
# AFTER clippy, deliberately. `cargo clippy` and `cargo test` disagree about the workspace crates'
# fingerprints, so whichever runs second rebuilds them. Measured on the red-first run that had this
# block before clippy: G7 re-compiled oracle-aether, oracle-frontend and oracle-player after G6 had
# already built them. In this order the `--no-run` build below is the one the suite then reuses.
# --------------------------------------------------------------------------------------------
hr; echo "G6  expected leg count (derived, never hardcoded)"
cargo test --workspace --release --no-run --message-format=json \
    > "$RUN_DIR/norun-artifacts.json" 2> "$RUN_DIR/norun.err"
NORUN_STATUS=$?
if [ "$NORUN_STATUS" -ne 0 ]; then
    fail "G6 the --no-run build failed (status $NORUN_STATUS); see $RUN_DIR/norun.err"
    command tail -40 "$RUN_DIR/norun.err"
    finish_red
fi
EXEC_LEGS="$(jq -r 'select(.reason == "compiler-artifact")
                    | select(.profile.test == true)
                    | select(.executable != null)
                    | .executable' "$RUN_DIR/norun-artifacts.json" | command sort -u | command wc -l)"
DOC_LEGS="$(cargo metadata --no-deps --format-version 1 \
            | jq '[.packages[].targets[] | select(.kind | index("lib")) | select(.doctest == true)] | length')"
if ! [ "$EXEC_LEGS" -gt 0 ] 2>/dev/null || ! [ "$DOC_LEGS" -gt 0 ] 2>/dev/null; then
    fail "G6 could not derive a leg count (exec=$EXEC_LEGS doc=$DOC_LEGS)"
    finish_red
fi
EXPECTED_LEGS=$((EXEC_LEGS + DOC_LEGS))
pass "G6 expected legs = $EXPECTED_LEGS  ($EXEC_LEGS test executables cargo will run + $DOC_LEGS doc-test legs)"

# --------------------------------------------------------------------------------------------
# G6b  name the frozen aeon pin
#
# `crates/oracle-replay/tests/aeon_pin.rs` states the rule this gate satisfies, in terms:
#
#     libtest CAPTURES a passing test's stdout, so under a plain `cargo test` the banner below is
#     printed and then swallowed [...] So the chain is named by running this file with --nocapture
#     as its own step: the `Name the frozen aeon pin` step in .github/workflows/ci.yml, and inside
#     tools/replay_playthroughs.sh. [...] If you add a THIRD place the suite runs, name the pin
#     there too, or that run's green says nothing about which build it passed against.
#
# This script IS that third place — the landing command, the run whose green publishes a SHA — and it
# was the one place that did not name the pin. Measured before this block:
#     $ grep 'nocapture\|aeon_pin' tools/land.sh   ->   no matches
# G7 below RUNS aeon_pin like any other test and cannot SHOW it: `test ... ok` reads identically
# whether it hashed six artifacts or returned on its first line, and every landing's log therefore
# recorded a green with no statement about which aeon build produced it.
#
# --release and placed AFTER G6, deliberately: G6's `--no-run` build has just produced this exact test
# binary, so the step reuses it. Measured at 0.32 s wall on a warm tree, which is what a naming step
# should cost.
#
# A red pin REFUSES rather than accumulating, because that is what the pin means. G7's green would be a
# statement about bytes this repo can no longer name, and 25 minutes of it is 25 minutes spent earning
# an unattributable result. Same call `tools/replay_playthroughs.sh` makes one gate earlier.
# --------------------------------------------------------------------------------------------
hr; echo "G6b name the frozen aeon pin (--nocapture; the suite would swallow it)"
cargo test --release -p oracle-replay --test aeon_pin -- --nocapture 2>&1 \
    | command tee "$RUN_DIR/aeon-pin.log" | command sed -n '/FROZEN AEON PIN/,/^$/p'
PIN_STATUS=${PIPESTATUS[0]}          # never $? through a pipe
if [ "$PIN_STATUS" -ne 0 ]; then
    fail "G6b the frozen aeon pin gate is RED (exit $PIN_STATUS): every green below would be a claim about bytes this repo cannot name"
    command tail -40 "$RUN_DIR/aeon-pin.log"
    finish_red
fi
# Exiting 0 is not the same as having NAMED the pin, and naming is this step's entire job. If the
# banner is absent the run measured nothing about the chain; refusing beats rendering an unmeasured
# chain as a silent pass, which is the failure one directory over that this gate was copied from.
if ! command grep -q 'FROZEN AEON PIN' "$RUN_DIR/aeon-pin.log"; then
    fail "G6b aeon_pin exited 0 but printed no 'FROZEN AEON PIN' banner: this run names no chain, so nothing below could be attributed to a build"
    command tail -40 "$RUN_DIR/aeon-pin.log"
    finish_red
fi
pass "G6b the aeon chain is named above; every leg of G7 ran against those bytes"

# --------------------------------------------------------------------------------------------
# G7  the suite
# --------------------------------------------------------------------------------------------
hr; echo "G7  cargo test --workspace --release   (expect $EXPECTED_LEGS legs; ~20-30 min)"
echo "    started $(date -u +%Y-%m-%dT%H:%M:%SZ)"
SUITE_T0=$(date +%s)
cargo test --workspace --release 2>&1 | command tee "$RUN_DIR/suite.log"
SUITE_STATUS=${PIPESTATUS[0]}          # never $? through a pipe
SUITE_T1=$(date +%s)
echo "    finished $(date -u +%Y-%m-%dT%H:%M:%SZ)  ($((SUITE_T1 - SUITE_T0)) s, exit $SUITE_STATUS)"
if [ "$SUITE_STATUS" -eq 0 ]; then
    pass "G7 cargo test exited 0"
else
    fail "G7 cargo test exited $SUITE_STATUS"
fi

# --------------------------------------------------------------------------------------------
# G8  it ran to the END
# --------------------------------------------------------------------------------------------
hr; echo "G8  ran-to-the-end"
LEGS_HEADER="$(command grep -cE '^[[:space:]]{1,10}(Running|Doc-tests) ' "$RUN_DIR/suite.log")"
LEGS_RESULT="$(command grep -cE '^test result: ' "$RUN_DIR/suite.log")"
read -r T_PASS T_FAIL T_IGN < <(command awk '
    /^test result: /{
        for (i = 1; i <= NF; i++) {
            if ($(i+1) ~ /^passed/)  p += $i;
            if ($(i+1) ~ /^failed/)  f += $i;
            if ($(i+1) ~ /^ignored/) g += $i;
        }
    }
    END { printf "%d %d %d\n", p, f, g }' "$RUN_DIR/suite.log")

note "legs by 'Running'/'Doc-tests' header : $LEGS_HEADER"
note "legs by 'test result:' line          : $LEGS_RESULT"
note "expected                             : $EXPECTED_LEGS"
note "aggregate (release)                  : $T_PASS passed, $T_FAIL failed, $T_IGN ignored"

if [ "$LEGS_HEADER" -eq "$EXPECTED_LEGS" ] && [ "$LEGS_RESULT" -eq "$EXPECTED_LEGS" ]; then
    pass "G8 both counts equal the derived expectation — the run reached the end"
else
    fail "G8 leg count MISMATCH (header $LEGS_HEADER, result $LEGS_RESULT, expected $EXPECTED_LEGS): this run did NOT reach the end. A clean aggregate over a short run is indistinguishable from success except HERE."
fi
if [ "$T_FAIL" -eq 0 ]; then
    pass "G8 aggregate failures = 0"
else
    fail "G8 $T_FAIL test failure(s) in the aggregate"
    command grep -E '^(failures:|    [a-z_].*::)' "$RUN_DIR/suite.log" | command head -30
fi

else
    # The fast path's own accounting. These names are what the summary and the VERDICT file read,
    # and they say `skipped` rather than a number, because a `0` here would be a measurement of a
    # suite that never ran — the same untruth G8 exists to catch one page up. The debug arm's four
    # are here too: the fast path skips D1-D6 along with G4-G8, and a landing that published only
    # lane bookkeeping must not report a debug total it did not take.
    LEGS_HEADER=skipped
    EXPECTED_LEGS=skipped
    T_PASS=skipped; T_FAIL=0; T_IGN=skipped
    D_LEGS_HEADER=skipped
    D_EXPECTED_LEGS=skipped
    D_PASS=skipped; D_FAIL=0; D_IGN=skipped
fi

# --------------------------------------------------------------------------------------------
# G9/G10  the tree under the run  (aurora (c))
# --------------------------------------------------------------------------------------------
hr; echo "G9  HEAD did not move under the run"
HEAD_AFTER="$(git rev-parse HEAD)"
if [ "$HEAD_AFTER" = "$TESTED_SHA" ]; then
    pass "G9 HEAD is still $TESTED_SHA"
else
    fail "G9 HEAD MOVED under the run: tested $TESTED_SHA, now $HEAD_AFTER. Nothing tested the tip."
fi

echo "G10 tree still clean"
collect_dirt
if [ "${#BLOCKING[@]}" -eq 0 ]; then
    pass "G10 no blocking modifications appeared during the run"
else
    fail "G10 the tree was modified during the run (${#BLOCKING[@]} path(s))"
    for l in "${BLOCKING[@]}"; do note "    $l"; done
fi

[ "${#FAILURES[@]}" -gt 0 ] && finish_red

# --------------------------------------------------------------------------------------------
# the push  (aurora (d) and (e))
# --------------------------------------------------------------------------------------------
hr
echo "ALL GATES GREEN for $TESTED_SHA"
if [ "$FAST" = 1 ]; then
    note "FAST PATH: lane bookkeeping only, VALIDATED by G2b. The suite was not run and this line"
    note "does not claim it was."
    note "published paths: $(printf '%s ' $LANDED_PATHS)"
else
    note "profile=release  legs=$LEGS_HEADER/$EXPECTED_LEGS  $T_PASS passed, $T_FAIL failed, $T_IGN ignored"
    note "profile=debug    legs=$D_LEGS_HEADER/$D_EXPECTED_LEGS  $D_PASS passed, $D_FAIL failed, $D_IGN ignored   <- the profile CI runs"
fi

# --------------------------------------------------------------------------------------------
# WHAT THIS LANDING DID NOT RUN.
#
# Printed on every green, last, where a reader's eye ends up — and NOT only on a red, because the
# reader who needs it is the one about to infer a CI prediction from a green line. That inference
# is the measured defect (three instances, 2026-09-19); the header's caveat had been there all
# along and did not prevent it, because nobody reads a header at the end of a 25-minute run.
# --------------------------------------------------------------------------------------------
hr
echo "GATES CI RUNS THAT THIS LANDING DID NOT — read this before predicting CI:"
# ⚑ RECOMPUTED HERE, not reused from G0. One CI step (`Test`) is CONDITIONAL: D5 runs it verbatim
#   unless --no-debug-suite, so whether it belongs in this list is a fact about THIS RUN and not
#   about the map. Passing `--ran` is what makes the list a measurement; a static list would either
#   claim a gap that was covered or, far worse, omit one that was not. Without `--ran` every
#   CONDITIONAL row counts as a gap, which is the safe direction.
RAN_GATES=""
[ "$DEBUG_ARM" = 2 ] && RAN_GATES="D5"
printf '%s\n' "$(./tools/ci-parity.py --gaps-only --ran "$RAN_GATES" 2>/dev/null)" \
    | while IFS= read -r l; do [ -n "$l" ] && note "$l"; done
if [ "$FAST" = 1 ]; then
    note "FAST PATH: G4-G8 and the whole debug arm D1-D6 were skipped. This landing publishes only"
    note "  lane bookkeeping, and it measured none of the code CI will build."
elif [ "$DEBUG_ARM" != 2 ]; then
    note "AND: D4-D6 were skipped (--no-debug-suite). 'cargo test --workspace' in DEBUG — CI's"
    note "  'Test' step, the one that went red on run 35418036061 after a green landing here — did"
    note "  NOT run. This landing makes no claim about whether CI will pass."
else
    note "the debug arm D1-D6 RAN, so the three CI steps most likely to differ from a release-only"
    note "  landing (clippy, the determinism job, the workspace suite) were measured in CI's profile."
fi
note "G2b's validator also names the contract rules it does not enforce; see its output above."

if [ "$DO_PUSH" = 0 ]; then
    hr
    echo "--no-push: stopping before the push. It WOULD have run:"
    note "    git push $REMOTE $TESTED_SHA:refs/heads/$BRANCH"
    verify_remote_unmoved
    {
        echo "verdict=GREEN-NOPUSH"
        echo "tested_sha=$TESTED_SHA"
        echo "legs=$LEGS_HEADER/$EXPECTED_LEGS"
        echo "debug_legs=$D_LEGS_HEADER/$D_EXPECTED_LEGS"
        echo "gates=$([ "$FAST" = 1 ] && echo fastpath-lane-files-only || echo full)"
        write_verdict_ci_parity
    } > "$RUN_DIR/VERDICT"
    echo "land: GREEN, not pushed. End marker: $RUN_DIR/VERDICT"
    exit 0
fi

hr
echo "PUSH — by tested SHA, not by branch tip  (aurora (e))"
note "    git push $REMOTE $TESTED_SHA:refs/heads/$BRANCH"
if ! git push "$REMOTE" "$TESTED_SHA:refs/heads/$BRANCH"; then
    fail "push: git push failed"
    finish_red
fi

REMOTE_AFTER="$(git ls-remote "$REMOTE" "refs/heads/$BRANCH" 2>/dev/null | command awk '{print $1}')"
hr
echo "(d) DID THE PUSH DO ANYTHING? — git push exits 0 on already-up-to-date, so this is read back:"
note "before : ${REMOTE_BEFORE:-<no such ref>}"
note "after  : ${REMOTE_AFTER:-<no such ref>}"
if [ "$REMOTE_AFTER" != "$TESTED_SHA" ]; then
    fail "push: $REMOTE/$BRANCH is $REMOTE_AFTER, NOT the tested SHA $TESTED_SHA"
    finish_red
elif [ -z "$REMOTE_BEFORE" ]; then
    VERDICT=GREEN-CREATED
    SUMMARY="GREEN. The push CREATED $REMOTE/$BRANCH at the tested SHA."
    echo "    THE PUSH CREATED $REMOTE/$BRANCH at the tested SHA."
elif [ "$REMOTE_BEFORE" = "$REMOTE_AFTER" ]; then
    # The summary line has to say this too. A run that ends "GREEN and pushed" over a push that did
    # nothing is the same class of untruth (d) exists to prevent, one line further down the page.
    VERDICT=GREEN-NOOP
    SUMMARY="GREEN, and it pushed NOTHING — $REMOTE/$BRANCH already carried the tested SHA."
    echo "    THE PUSH DID NOTHING: $REMOTE/$BRANCH was already at the tested SHA before this run."
else
    VERDICT=GREEN-PUSHED
    SUMMARY="GREEN and pushed. $REMOTE/$BRANCH moved to the tested SHA."
    echo "    THE PUSH MOVED $REMOTE/$BRANCH from $REMOTE_BEFORE to the tested SHA."
fi

hr
{
    echo "verdict=$VERDICT"
    echo "tested_sha=$TESTED_SHA"
    echo "gates=$([ "$FAST" = 1 ] && echo fastpath-lane-files-only || echo full)"
    echo "legs=$LEGS_HEADER/$EXPECTED_LEGS"
    echo "debug_legs=$D_LEGS_HEADER/$D_EXPECTED_LEGS"
    echo "totals=release $T_PASS passed, $T_FAIL failed, $T_IGN ignored"
    echo "totals_debug=debug $D_PASS passed, $D_FAIL failed, $D_IGN ignored"
    write_verdict_ci_parity
    echo "remote_before=${REMOTE_BEFORE:-none}"
    echo "remote_after=$REMOTE_AFTER"
} > "$RUN_DIR/VERDICT"
echo "land: $SUMMARY End marker: $RUN_DIR/VERDICT"
exit 0
