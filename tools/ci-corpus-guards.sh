#!/usr/bin/env bash
# Run the six `vendor_data_present_when_running_in_ci` guards and make them VISIBLE in the log.
#
# WHY THIS EXISTS
#
# The guards were written so a missing corpus could not pass vacuously. For 46 days they could not run
# at all — CI was red for two unrelated reasons and `cargo test --workspace` never executed — so a
# permanently-red check camouflaged exactly the vacuity it existed to detect. Coming back from that,
# "the suite is green" is not enough: the log has to say the guards ran and held.
#
# `--nocapture` alone does not achieve that. libtest prints
#
#     test vendor_data_present_when_running_in_ci ... ok
#
# byte-identically whether the guard verified 708 files or returned on its first line because `CI` was
# unset. The NAME is not evidence. So each guard prints a `CORPUS GUARD <file>: OK — <what>` banner
# AFTER its last assertion, and this script counts them.
#
# The count is the part that matters. A bare `cargo test <filter>` that matches nothing exits 0 with
# "0 passed; 0 filtered out" — so renaming or deleting a guard would turn this step into a green run of
# nothing, which is the same defect one level up. Six banners or red.
set -uo pipefail
cd "$(dirname "$0")/.."

EXPECTED=6
LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT

echo "=== corpus guards (${EXPECTED} expected) ==="

# One invocation per crate; `--test` is repeatable within a crate.
cargo test -p oracle-core \
  --test singlestep_m68000 \
  --test singlestep_z80 \
  --test conformance_roms \
  --test scanline_goldens \
  --test scanline_capture \
  vendor_data_present_when_running_in_ci -- --nocapture 2>&1 | tee "$LOG"
core_status=${PIPESTATUS[0]}

cargo test -p oracle-aether \
  --test scanlines \
  vendor_data_present_when_running_in_ci -- --nocapture 2>&1 | tee -a "$LOG"
aether_status=${PIPESTATUS[0]}

if [[ $core_status -ne 0 || $aether_status -ne 0 ]]; then
  echo "corpus guards: FAILED (oracle-core=$core_status oracle-aether=$aether_status)" >&2
  exit 1
fi

ok=$(grep -c '^CORPUS GUARD .*: OK' "$LOG" || true)
skipped=$(grep -c '^CORPUS GUARD .*: SKIPPED' "$LOG" || true)

echo
echo "=== corpus guards: $ok verified, $skipped skipped (expected $EXPECTED verified) ==="

# Locally there is no `CI` env var and every guard correctly no-ops, so this script's contract is
# "under CI, six banners" — not "six banners always". Outside CI it reports and returns cleanly.
if [[ -z "${CI:-}" ]]; then
  echo "not running under CI (no CI env var) — the guards are no-ops by design; nothing to enforce"
  exit 0
fi

if [[ "$ok" -ne "$EXPECTED" ]]; then
  echo >&2
  echo "corpus guards: expected $EXPECTED 'CORPUS GUARD ...: OK' banners, saw $ok." >&2
  echo "A guard was renamed, deleted, or no-opped. An empty libtest filter exits 0, so this count" >&2
  echo "is the only thing between that and a green run of nothing." >&2
  exit 1
fi

echo "all $EXPECTED corpus guards ran and held"
