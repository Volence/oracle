#!/usr/bin/env bash
# Runner for the lane-file validator's own red-first proof.
#
# THIS IS THE RUNNER THAT EXECUTES tools/test_lane_check.py AND tools/ci-parity.py's self-check.
#
# `tools/lane-check.py` is run by `tools/land.sh` at G2b and prints "lane-check: clean" on every
# landing. On 2026-09-19 it was measured to enforce almost none of the contract it was believed to
# enforce, so that line was a green about a question nobody had asked. Widening it is only worth
# anything if the new rules fire on the shapes that were actually getting through — which is what
# this proves, against real dated revisions of this repo's own board, with the PREVIOUS gate run on
# the same bytes as a control.
#
# NOT WIRED INTO A BLOCKING PATH, the same call and the same reasoning as
# tools/run_contract_drift_tests.sh, tools/run_doc_split_tests.sh and tools/run_accept_table_tests.sh:
# CI here builds Rust and these are Python instruments. It is cheap (a few seconds) and it reads only
# this repo's own git objects — never a peer's tree, never the network.
#
# ⚑ It DOES read this repo's history, so it is not hermetic in the tempdir sense the other three are.
#   That is deliberate and is the whole point: a hermetic fixture is a file written to satisfy the
#   check, which is the exact defect being fixed. If a revision is unreachable (a shallow clone), the
#   test FAILS as UNMEASURABLE rather than skipping — an absence is never a finding.
#
# Usage:  tools/run_lane_check_tests.sh
#         PYTHON=python3.12 tools/run_lane_check_tests.sh
#
# Exit 0 only if every case reddens where expected, every control stays clean, and HEAD stays green.

set -uo pipefail
cd "$(dirname "$0")/.."
PYTHON="${PYTHON:-python3}"

rc=0

echo "=== lane-check red-first (real committed shapes; control = the gate at f4022d6) ==="
"$PYTHON" tools/test_lane_check.py || rc=1

echo
echo "=== ci-parity red-first (four drifts, on a COPY of the workflow; ci.yml is never written) ==="
"$PYTHON" tools/test_ci_parity.py || rc=1

echo
echo "=== ci-parity self-check (is the CI coverage map current against the real ci.yml?) ==="
"$PYTHON" tools/ci-parity.py || rc=1

echo
echo "=== the rules lane-check does NOT enforce (printed, never implied) ==="
"$PYTHON" tools/lane-check.py --gaps || rc=1

echo
if [ "$rc" -eq 0 ]; then
    echo "run_lane_check_tests: PASS"
else
    echo "run_lane_check_tests: FAIL"
fi
exit "$rc"
