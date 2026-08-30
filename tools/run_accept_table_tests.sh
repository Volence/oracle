#!/usr/bin/env bash
# Runner for the legacy accept-table tool.
#
# This is the runner named in the handoff: tools/run_accept_table_tests.sh.
# It executes the test suite AND regenerates the table with --fail-on-gap, so
# a parse that has quietly stopped covering the source fails here rather than
# shipping a short table that reads as a complete one.
#
# Deliberately NOT wired into .github/workflows: the consumer validates the
# emitted table against their own independent read BEFORE it becomes a gate.
# Once an unvalidated instrument is a gate, every failure it produces presents
# as the consumer being broken.
#
# Usage:  tools/run_accept_table_tests.sh [--emit PATH]
#
# Exit 0 only if every test passes AND the parse of the real source is complete
# AND the two independent enumerations agree.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
py="${PYTHON:-python3}"
emit=""
if [[ "${1:-}" == "--emit" ]]; then
    emit="${2:?--emit needs a path}"
fi

echo "== unit tests =========================================================="
# The suite's own exit status decides, not a grep of its output: a tail excerpt
# once hid 16 failures behind a merged "green" in this workspace. `set -e` is
# suspended across the pipeline so the explicit diagnostic below actually runs.
set +e
"$py" -m unittest discover -s "$here" -p 'test_legacy_accept_table.py' -v 2>&1 \
    | tee "${TMPDIR:-/tmp}/accept_table_tests.$$.log"
status=${PIPESTATUS[0]}
set -e
tail -3 "${TMPDIR:-/tmp}/accept_table_tests.$$.log"
rm -f "${TMPDIR:-/tmp}/accept_table_tests.$$.log"
if [[ $status -ne 0 ]]; then
    echo "FAIL: unit tests did not pass (exit $status) -- see the full log above" >&2
    exit 1
fi

echo
echo "== accept-table generation (gaps are fatal) ============================"
if [[ -n "$emit" ]]; then
    "$py" "$here/legacy_accept_table.py" --fail-on-gap --out "$emit"
    echo "wrote $emit"
    "$py" "$here/legacy_accept_table.py" --fail-on-gap --format summary
else
    "$py" "$here/legacy_accept_table.py" --fail-on-gap --format summary
fi

echo
echo "ALL GREEN"
