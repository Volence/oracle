#!/usr/bin/env bash
# The replay net's full playthroughs — the runner that actually executes them.
#
# WHY THIS EXISTS
#
# The three playthroughs in `crates/oracle-replay/tests/replay_real_artifacts.rs` are the only tests
# that run Aeon's embedded replay stream end to end, and they were `#[ignore]`d behind a `--ignored`
# flag nobody types. That is how the fixture pin they read was allowed to go stale under a green suite.
#
# They now carry `#[cfg_attr(debug_assertions, ignore)]` instead, so:
#   * `cargo test` (debug)   — skipped, as before: ~183 s of unoptimized emulation in the default
#                              suite is a suite people revert.
#   * `cargo test --release` — they RUN. No flag to remember.
#
# This script is that release command, and the `replay-playthroughs` job in .github/workflows/ci.yml
# runs the same thing. Either is the answer to "what runs them?".
#
# EXIT STATUS is the test run's, unless the frozen-pin GATE below is red — in which case the
# playthroughs never run, because their green would be unattributable. The pin CURRENCY report at the
# end is a different thing entirely: REPORT ONLY, and it cannot change the exit status.

set -uo pipefail
cd "$(dirname "$0")/.."

# Name the pin first, with --nocapture: libtest swallows a passing test's stdout, so without this the
# banner would exist and never be read on a green run. A green below is a statement about THESE bytes.
#
# ⚑ AND ITS VERDICT IS READ, which until 2026-09-07 it was not. `set -uo pipefail` carries no `-e`, and
# the statement after the pipeline was an `echo`, which overwrites `$?` before anything looked at it.
# Measured, with one nibble of `fixtures/aeon/PIN.tsv` flipped so `aeon_pin` is RED:
#
#     $ bash tools/replay_playthroughs.sh
#     === replay playthroughs (release) ===        <- the FIRST line of output
#     ...
#     SCRIPT EXIT CODE WITH A RED aeon_pin: 0
#
# Not one word about the pin, and a zero. Doubly silent, because the chain banner is printed by the
# very test that panics, so the `sed` range selected nothing either — the failure was indistinguishable
# from a run where the pin simply had nothing to say. The one CI job whose stated purpose is "this green
# is about THESE bytes" could not fail on a broken pin.
#
# `-e` is deliberately NOT the fix, and that was checked rather than assumed. Two things below rely on a
# non-zero exit CONTINUING: the playthrough run's `status=$?` — this file's documented contract is that
# the exit status is the test run's, and under `-e` a red playthrough would skip the timing line, the
# currency report and `exit $status` outright — and the `|| true` on the reporter. So the verdict is read
# where it is produced, with `${PIPESTATUS[0]}` (never `$?` through a pipe), the same shape `land.sh`
# already uses for its suite leg.
PIN_LOG="$(mktemp)"
trap 'rm -f "$PIN_LOG"' EXIT
cargo test -p oracle-replay --test aeon_pin -- --nocapture 2>&1 \
    | tee "$PIN_LOG" | sed -n '/FROZEN AEON PIN/,/^$/p'
PIN_STATUS=${PIPESTATUS[0]}
if [ "$PIN_STATUS" -ne 0 ]; then
    echo
    echo "*** REFUSED: the frozen aeon pin gate is RED (exit $PIN_STATUS). The playthroughs below were"
    echo "*** NOT run. Their green would have been a statement about bytes this repo can no longer"
    echo "*** name, which is the one thing this step exists to prevent."
    echo
    tail -40 "$PIN_LOG"
    exit "$PIN_STATUS"
fi
# A zero exit is not the same as having NAMED the pin, and this step's whole job is the naming. If the
# banner is absent the run measured nothing about the chain — say so and refuse, rather than render an
# unmeasured chain as a silent pass.
if ! grep -q 'FROZEN AEON PIN' "$PIN_LOG"; then
    echo
    echo "*** REFUSED: aeon_pin exited 0 but printed no 'FROZEN AEON PIN' banner, so this run names no"
    echo "*** chain and the green below could not be attributed to any build. Either the banner moved"
    echo "*** or --nocapture stopped reaching it; fix the naming, do not delete this check."
    echo
    tail -40 "$PIN_LOG"
    exit 1
fi

echo "=== replay playthroughs (release) ==="
date -u +'started %Y-%m-%dT%H:%M:%SZ'
start=$(date +%s)

cargo test --release -p oracle-replay --test replay_real_artifacts -- --nocapture
status=$?

end=$(date +%s)
date -u +'finished %Y-%m-%dT%H:%M:%SZ'
echo "wall clock: $((end - start)) s"

# Non-gating: whether aeon has moved past our pin is a question for a reader, not a build. A gate that
# reddens because someone ELSE moved puts the whole gradient behind bending our side until it passes.
echo
echo "=== pin currency (REPORT ONLY — does not affect exit status) ==="
python3 tools/aeon_pin_report.py || true

exit $status
