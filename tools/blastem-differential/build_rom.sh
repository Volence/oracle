#!/usr/bin/env bash
# Assemble the differential harness ROM with the native ASL toolchain.
#
# The harness ROM (harness.asm) is a clean-room instrument authored for this
# differential; see README.md. It reuses the aeon suite's native `asl`/`p2bin`
# (no Wine) — override TOOLS if your layout differs.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
TOOLS="${TOOLS:-$HERE/../../../aeon/tools}"

"$TOOLS/asl" -cpu 68000 -q -A -L -U \
    -olist "$HERE/harness.lst" -o "$HERE/harness.p" "$HERE/harness.asm"
"$TOOLS/p2bin" "$HERE/harness.p" "$HERE/harness.bin" >/dev/null

echo "built $HERE/harness.bin ($(stat -c%s "$HERE/harness.bin") bytes)"
echo "label addresses:"
grep -iE 'FT0 |FT1 |FTN |TraceHalt|GenHalt|Init |TraceH |GenericH' "$HERE/harness.lst" \
    | grep -iE ' C \|' || true
