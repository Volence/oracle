#!/usr/bin/env bash
# Assemble the differential harness ROM with the native ASL toolchain.
#
# The harness ROM (harness.asm) is a clean-room instrument authored for this
# differential; see README.md. It reuses the aeon suite's native `asl`/`p2bin`
# (no Wine); name it with TOOLS, AEON_DIR or EMPYREAN_SUITE_ROOT — see
# aeon_tools.sh for why there is no default.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=aeon_tools.sh
. "$HERE/aeon_tools.sh"
TOOLS="$(resolve_aeon_tools)"

"$TOOLS/asl" -cpu 68000 -q -A -L -U \
    -olist "$HERE/harness.lst" -o "$HERE/harness.p" "$HERE/harness.asm"
"$TOOLS/p2bin" "$HERE/harness.p" "$HERE/harness.bin" >/dev/null

echo "built $HERE/harness.bin ($(stat -c%s "$HERE/harness.bin") bytes)"
echo "label addresses:"
grep -iE 'FT0 |FT1 |FTN |TraceHalt|GenHalt|Init |TraceH |GenericH' "$HERE/harness.lst" \
    | grep -iE ' C \|' || true
