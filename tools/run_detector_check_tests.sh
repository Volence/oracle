#!/usr/bin/env bash
# Runner for the method-declaration detector's own red-first proof.
#
# THIS IS THE RUNNER THAT EXECUTES tools/test_detector_check.py.
#
# `tools/detector-check.py` is run by `tools/land.sh` at G2c and prints "detector-check: clean" on
# every landing. That line bounds only what the tool checks, and the tool checks SHAPE and
# CITATIONS — never whether a stated count is true and never whether a doc that reports a zero
# declared it at all. `--gaps` is printed below and by land.sh for exactly that reason.
#
# NOT WIRED INTO A BLOCKING PATH, the same call and the same reasoning as
# tools/run_lane_check_tests.sh, tools/run_contract_drift_tests.sh, tools/run_doc_split_tests.sh
# and tools/run_accept_table_tests.sh: CI here builds Rust and this is a Python instrument. It is
# cheap (under a second) and it reads only this repo's own git objects — never a peer's tree, never
# the network.
#
# ⚑ It DOES read this repo's history (74c3579, 35f81ca, 2999687), so it is not hermetic in the
#   tempdir sense. That is deliberate: a hermetic fixture is a file written to satisfy the check,
#   which is the exact defect this parcel exists to catch. If a revision is unreachable (a shallow
#   clone), the case reports UNMEASURABLE and the run FAILS rather than skipping — an absence is
#   never a finding.
#
# Usage:  tools/run_detector_check_tests.sh
#         PYTHON=python3.12 tools/run_detector_check_tests.sh
#
# Exit 0 only if every case reddens on ITS NAMED GUARD and the green arm stays clean.

set -uo pipefail
cd "$(dirname "$0")/.."
PYTHON="${PYTHON:-python3}"

rc=0

echo "=== detector-check red-first (real and derived shapes from this lane's 2026-09-19 record) ==="
"$PYTHON" tools/test_detector_check.py || rc=1

echo
echo "=== the rules detector-check does NOT enforce (printed, never implied) ==="
"$PYTHON" tools/detector-check.py --gaps || rc=1

echo
if [ "$rc" -eq 0 ]; then
    echo "run_detector_check_tests: PASS"
else
    echo "run_detector_check_tests: FAIL"
fi
exit "$rc"
