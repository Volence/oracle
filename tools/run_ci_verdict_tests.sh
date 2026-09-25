#!/usr/bin/env bash
# Runner for tools/ci-verdict.py's offline red-first proof.
#
# THIS IS THE RUNNER THAT EXECUTES tools/test_ci_verdict.py. `tools/land.sh` also runs that file at
# G0c, on every landing (it is offline and under a second, so unlike the other tools' self-tests it
# costs a landing nothing).
#
# Offline: it replays GitHub API responses the tool recorded from real runs
# (tools/ci-verdict-fixtures/) and reads this repo's own git history for the workflows at each SHA
# and the tip walk. It never touches the network. A revision that is unreachable (a shallow clone)
# fails as UNMEASURABLE rather than skipping.
#
# The LIVE tool is not run here, because its answer depends on GitHub's state at the moment; its
# real-history verdicts are recorded in the commit that added it.
#
# Usage:  tools/run_ci_verdict_tests.sh
#         PYTHON=python3.12 tools/run_ci_verdict_tests.sh

set -uo pipefail
cd "$(dirname "$0")/.."
PYTHON="${PYTHON:-python3}"

rc=0
echo "=== ci-verdict red-first (recorded real runs + one-field mutations; offline) ==="
"$PYTHON" tools/test_ci_verdict.py || rc=1

echo
echo "=== ci-parity self-check (ci-verdict imports its trigger_map; the map must still be current) ==="
"$PYTHON" tools/ci-parity.py || rc=1

echo
if [ "$rc" -eq 0 ]; then
    echo "run_ci_verdict_tests: PASS"
else
    echo "run_ci_verdict_tests: FAIL"
fi
exit "$rc"
