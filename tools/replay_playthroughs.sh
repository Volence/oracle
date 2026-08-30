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
# EXIT STATUS is the test run's. The pin report at the end is REPORT ONLY and cannot change it.

set -uo pipefail
cd "$(dirname "$0")/.."

# Name the pin first, with --nocapture: libtest swallows a passing test's stdout, so without this the
# banner would exist and never be read on a green run. A green below is a statement about THESE bytes.
cargo test -p oracle-replay --test aeon_pin -- --nocapture 2>&1 | sed -n '/FROZEN AEON PIN/,/^$/p'

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
