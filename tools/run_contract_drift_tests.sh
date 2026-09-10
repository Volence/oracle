#!/usr/bin/env bash
# Runner for the contract-drift reporter's own test suite.
#
# THIS IS THE RUNNER THAT EXECUTES tools/test_contract_drift_report.py.
#
# `tools/contract_drift_report.py` is the only thing in this repo that can notice the contract repo
# moving past our pin. An instrument that cannot fail is worse than no instrument, because its
# presence and its absence read the same -- and this one's whole value is in the branch where it says
# UNMEASURABLE rather than "no drift". So these tests are not decoration around the tool: they are
# what entitles anyone to read its output.
#
# NOT WIRED INTO A BLOCKING PATH, and that is deliberate on two levels:
#
#   * the DRIFT CHECK itself must block nobody -- it goes red because a PEER moved, and a gate with
#     that property puts the whole gradient behind bending our side until it is green, which for a
#     pin means moving the pin to silence a red. `contract_drift_report.py` always exits 0.
#   * these TESTS are hermetic (every fixture is a synthetic git repo in a tempdir; nothing reads
#     /home/volence/sonic_hacks/empyrean) and would be safe in CI, but CI here builds Rust and this
#     is a Python instrument. Same call and same reasoning as tools/run_doc_split_tests.sh and
#     tools/run_accept_table_tests.sh. If that changes, add a job that runs THIS script rather than
#     re-deriving the command in YAML -- there must be exactly one code path between "what I ran"
#     and "what CI runs".
#
# Usage:  tools/run_contract_drift_tests.sh
#         PYTHON=python3.12 tools/run_contract_drift_tests.sh
#
# Exit 0 only if every test passes.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
py="${PYTHON:-python3}"
log="$(mktemp "${TMPDIR:-/tmp}/contract_drift_tests.XXXXXX.log")"
trap 'rm -f "$log"' EXIT

# Scrub the suite-path variables. A resolver suite that inherits the caller's environment tests the
# caller's machine; the rows that own the unset path must be constructed, not ambient
# (empyrean contract/SUITE_PATHS.md, "A resolver's OWN checkout is observed, not resolved").
unset EMPYREAN_DIR EMPYREAN_SUITE_ROOT

echo "== tools/test_contract_drift_report.py ================================="
# The suite's own exit status decides, never a grep of its output: a tail excerpt once hid 16
# failures behind a merged "green" in this workspace. `set -e` is suspended across the pipeline so
# the diagnostic below actually runs instead of the shell exiting first.
set +e
"$py" -m unittest discover -s "$here" -p 'test_contract_drift_report.py' -v 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
set -e

echo
echo "== aggregate ==========================================================="
ran=$(grep -cE '^test_[A-Za-z0-9_]+ \(' "$log" || true)
echo "tests executed : ${ran}"
grep -E '^(FAIL|ERROR): ' "$log" | sed 's/^/  /' || true
tail -3 "$log"

if [[ $status -ne 0 ]]; then
    echo "FAIL: tools/test_contract_drift_report.py did not pass (exit $status)" >&2
    exit 1
fi

# A suite that collected nothing exits 0 from unittest. That green would say "the instrument is
# validated" while validating nothing -- the exact failure this instrument exists to refuse.
if [[ "${ran}" -lt 1 ]]; then
    echo "FAIL: the discovery pattern matched no tests -- a vacuous green" >&2
    exit 1
fi

echo
echo "ALL GREEN (${ran} tests)"
