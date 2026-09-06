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
#   ./tools/land.sh              # run every gate, then push the tested SHA to origin/main
#   ./tools/land.sh --no-push    # run every gate and stop; report what a push WOULD do
#   ./tools/land.sh --remote R --branch B
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
# Gates (in cost order, cheapest first, so a red is loud in seconds rather than in half an hour):
#
#   G1  vendor precondition        a fresh worktree has no `vendor/` symlink, and without it the
#                                  SingleStepTests sweep SKIPS AND PASSES VACUOUSLY. Its failure
#                                  mode is a silent green — exactly what a human-read checklist is
#                                  worst at and a script is best at. We also export `CI=1`, which
#                                  arms the six vacuity guards the suite already carries — four
#                                  named `vendor_data_present_when_running_in_ci` tests
#                                  (conformance_roms, scanline_goldens, singlestep_m68000,
#                                  singlestep_z80) plus two inline "skip locally, NEVER under CI"
#                                  refusals (oracle-aether scanlines.rs, oracle-core
#                                  scanline_capture.rs). Those assert against the test files' OWN
#                                  ROM and opcode manifests, which is a stronger statement than any
#                                  path check this script could hand-write.
#   G2  clean tree (a)             refuse a dirty tree BEFORE anything runs, listing the paths.
#                                  `docs/lane-status.json` is the one tolerated path — see below.
#   G2b lane files                 `docs/lane-status.json` and `docs/lane-log.jsonl` are parsed by a
#                                  console that is not in this repo, and until now NOTHING here
#                                  validated them: `git grep` finds both names in two files
#                                  (`crates/oracle-aether/tests/hosted.rs`, `src/server.rs`) and both
#                                  are doc-comment mentions. So a malformed entry landed clean and was
#                                  discovered by the owner's card going dark. `tools/lane-check.py`
#                                  parses every log line, checks the status document's shape and its
#                                  `state` vocabulary, and refuses a future timestamp. It runs on the
#                                  WORKING TREE (what the console reads) and on the COMMITTED blob
#                                  (what the push publishes), because the carve-out below lets those
#                                  two differ for exactly one of the files.
#   G3  fast-forward               the tested SHA must be a descendant of the remote branch, so a
#                                  landing can never rewrite pushed history.
#   G4  cargo fmt --all --check
#   G5  cargo clippy --workspace --all-targets --release -- -D warnings
#                                  `-D warnings` is not decoration. Without it clippy exits 0 on
#                                  every lint it finds, i.e. the gate cannot fire — measured: the
#                                  same `clippy::needless_return` that reddens G5 leaves the flagless
#                                  command at exit 0. The repo's own CI already denies warnings; a
#                                  local gate that did not would be weaker than the thing it is
#                                  meant to make unnecessary.
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
#   1. Nothing compiled reads it. `git grep lane-status -- '*.rs' '*.py' '*.sh'` is empty, so its
#      working-tree content cannot change what the suite measures.
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
#     `{docs/lane-log.jsonl, docs/lane-status.json}`. Anything else runs the full suite, **including
#     any other file under `docs/`** — a design page is prose to us and evidence to a peer, and
#     "docs are safe" is exactly the reasoning that widens a carve-out until it means nothing.
#   * The argument it rests on: those two files are read by no compiled thing here (the same fact
#     the carve-out above rests on), so G4 to G8 would be measuring, byte for byte, the tree the
#     remote already carried when that SHA landed.
#   * **What it does instead is more than it skips.** G2b runs on every landing, fast or full, and
#     it is the first thing in this repo ever to check these two files at all.
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

while [ $# -gt 0 ]; do
    case "$1" in
        --remote)  REMOTE="${2:?--remote needs a value}"; shift 2 ;;
        --branch)  BRANCH="${2:?--branch needs a value}"; shift 2 ;;
        --no-push) DO_PUSH=0; shift ;;
        -h|--help) command sed -n '2,120p' "$0"; exit 0 ;;
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
TESTED_SHA=""
REMOTE_BEFORE=""
# Whether G3 has actually read the remote yet. An empty `REMOTE_BEFORE` means two different things
# and only one of them is "no such ref"; see `verify_remote_unmoved`.
REMOTE_READ=0

hr()   { echo "------------------------------------------------------------------------------"; }
pass() { echo "  PASS  $*"; }
fail() { echo "  RED   $*"; FAILURES+=("$*"); }
note() { echo "        $*"; }

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
    } > "$RUN_DIR/VERDICT"
    echo "land: RED. Run artifacts in $RUN_DIR (end marker: $RUN_DIR/VERDICT)."
    exit 1
}

echo "=============================================================================="
echo "land.sh — $STAMP — $ROOT"
echo "  remote/branch : $REMOTE/$BRANCH"
echo "  push          : $([ "$DO_PUSH" = 1 ] && echo yes || echo 'no (--no-push)')"
echo "  run artifacts : $RUN_DIR"
echo "=============================================================================="

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
note "CI=1 exported: the suite's six in-built vacuity guards are armed (4 named guard tests + 2"
note "inline skip-refusals), so a present-but-INCOMPLETE vendor corpus reddens from inside too."

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
hr; echo "G2b lane files (docs/lane-status.json, docs/lane-log.jsonl)"
LANE_STATUS="docs/lane-status.json"
LANE_LOG="docs/lane-log.jsonl"
LANE_OK=1

# (i) the working tree: the copy the console actually reads, and the one the G2 carve-out lets
#     differ from the commit.
if ./tools/lane-check.py --status "$ROOT/$LANE_STATUS" --log "$ROOT/$LANE_LOG" \
        --label "working tree" > "$RUN_DIR/lane-worktree.log" 2>&1; then
    pass "G2b working tree: $(command tail -1 "$RUN_DIR/lane-worktree.log")"
else
    fail "G2b the lane files in the working tree are malformed"
    command cat "$RUN_DIR/lane-worktree.log"
    LANE_OK=0
fi

# (ii) the committed blobs: what (e) actually publishes. The two are the same file whenever the
#      carve-out is not in play, and the check costs milliseconds either way.
git show "$TESTED_SHA:$LANE_STATUS" > "$RUN_DIR/lane-status.committed.json" 2>/dev/null || true
git show "$TESTED_SHA:$LANE_LOG"    > "$RUN_DIR/lane-log.committed.jsonl"  2>/dev/null || true
if ./tools/lane-check.py \
        --status "$RUN_DIR/lane-status.committed.json" \
        --log    "$RUN_DIR/lane-log.committed.jsonl" \
        --label "as committed at ${TESTED_SHA:0:12}" > "$RUN_DIR/lane-committed.log" 2>&1; then
    pass "G2b committed: $(command tail -1 "$RUN_DIR/lane-committed.log")"
else
    fail "G2b the lane files AS COMMITTED are malformed; this is what a push would publish"
    command cat "$RUN_DIR/lane-committed.log"
    LANE_OK=0
fi
[ "$LANE_OK" = 0 ] && finish_red

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
FASTPATH_ALLOWED="docs/lane-log.jsonl docs/lane-status.json"
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
    # The fast path's own accounting. These four names are what the summary and the VERDICT file
    # read, and they say `skipped` rather than a number, because a `0` here would be a measurement
    # of a suite that never ran — the same untruth G8 exists to catch one page up.
    LEGS_HEADER=skipped
    EXPECTED_LEGS=skipped
    T_PASS=skipped; T_FAIL=0; T_IGN=skipped
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
fi

if [ "$DO_PUSH" = 0 ]; then
    hr
    echo "--no-push: stopping before the push. It WOULD have run:"
    note "    git push $REMOTE $TESTED_SHA:refs/heads/$BRANCH"
    verify_remote_unmoved
    { echo "verdict=GREEN-NOPUSH"; echo "tested_sha=$TESTED_SHA"; echo "legs=$LEGS_HEADER/$EXPECTED_LEGS"; echo "gates=$([ "$FAST" = 1 ] && echo fastpath-lane-files-only || echo full)"; } > "$RUN_DIR/VERDICT"
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
    echo "totals=release $T_PASS passed, $T_FAIL failed, $T_IGN ignored"
    echo "remote_before=${REMOTE_BEFORE:-none}"
    echo "remote_after=$REMOTE_AFTER"
} > "$RUN_DIR/VERDICT"
echo "land: $SUMMARY End marker: $RUN_DIR/VERDICT"
exit 0
