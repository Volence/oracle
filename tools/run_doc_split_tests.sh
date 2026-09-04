#!/usr/bin/env bash
# Runner for the document-split proof instrument.
#
# THIS IS THE RUNNER THAT EXECUTES tools/test_prove_doc_split.py.
#
# prove_doc_split.py is meant to be adopted as a shared gate by lanes other than
# this one. An unvalidated instrument adopted as a gate returns a confident wrong
# verdict, and its red then presents as the consumer being broken. So the tests
# are not optional decoration around the tool: they are the thing that entitles
# anyone to trust it, and they must be runnable in one command from any lane's
# checkout of this repo.
#
# Not wired into .github/workflows: CI here builds Rust, and the split this tool
# proves is a one-off event per lane, not a per-push invariant. The same call is
# made in tools/run_accept_table_tests.sh, for the same reason. If that changes,
# add a job that runs THIS script rather than re-deriving the command in YAML --
# there must be exactly one code path between "what I ran" and "what CI runs".
#
# Usage:  tools/run_doc_split_tests.sh
#         PYTHON=python3.12 tools/run_doc_split_tests.sh
#
# Exit 0 only if every test passes.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
py="${PYTHON:-python3}"
log="$(mktemp "${TMPDIR:-/tmp}/doc_split_tests.XXXXXX.log")"
trap 'rm -f "$log"' EXIT

echo "== tools/test_prove_doc_split.py ======================================="
# The suite's own exit status decides, never a grep of its output: a tail
# excerpt once hid 16 failures behind a merged "green" in this workspace.
# `set -e` is suspended across the pipeline so the diagnostic below actually
# runs instead of the shell exiting first.
set +e
"$py" -m unittest discover -s "$here" -p 'test_prove_doc_split.py' -v 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
set -e

echo
echo "== aggregate ==========================================================="
# Aggregate totals with names, not a tail excerpt: a run that reports only its
# last three lines cannot distinguish 0 failures from 40.
ran=$(grep -cE '^test_[A-Za-z0-9_]+ \(' "$log" || true)
echo "tests executed : ${ran}"
grep -E '^(FAIL|ERROR): ' "$log" | sed 's/^/  /' || true
tail -3 "$log"

if [[ $status -ne 0 ]]; then
    echo "FAIL: tools/test_prove_doc_split.py did not pass (exit $status)" >&2
    exit 1
fi

# A suite that collected nothing exits 0 from unittest. That green would say
# "the instrument is validated" while validating nothing, which is the exact
# failure mode this whole parcel is about.
if [[ "${ran}" -lt 1 ]]; then
    echo "FAIL: the discovery pattern matched no tests -- a vacuous green" >&2
    exit 1
fi

echo
echo "ALL GREEN (${ran} tests)"
