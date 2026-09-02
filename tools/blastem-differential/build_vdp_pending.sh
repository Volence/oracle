#!/usr/bin/env bash
# Assemble the VDP pending-toggle experiment ROM with the native ASL toolchain.
# Same conventions as build_rom.sh (aeon suite's asl/p2bin; name it with TOOLS,
# AEON_DIR or EMPYREAN_SUITE_ROOT — see aeon_tools.sh).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=aeon_tools.sh
. "$HERE/aeon_tools.sh"
TOOLS="$(resolve_aeon_tools)"

"$TOOLS/asl" -cpu 68000 -q -A -L -U \
    -olist "$HERE/vdp_pending.lst" -o "$HERE/vdp_pending.p" "$HERE/vdp_pending.asm"
"$TOOLS/p2bin" "$HERE/vdp_pending.p" "$HERE/vdp_pending.bin" >/dev/null

echo "built $HERE/vdp_pending.bin ($(stat -c%s "$HERE/vdp_pending.bin") bytes)"
echo "label addresses:"
grep -iE 'Done |GenHalt |Init ' "$HERE/vdp_pending.lst" | grep -iE ' C \|' || true
