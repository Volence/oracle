#!/usr/bin/env bash
# Assemble the VDP DMA-fill experiment ROM with the native ASL toolchain.
# Same conventions as build_rom.sh (aeon suite's asl/p2bin; override TOOLS).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
TOOLS="${TOOLS:-$HERE/../../../aeon/tools}"

"$TOOLS/asl" -cpu 68000 -q -A -L -U \
    -olist "$HERE/vdp_dma_fill.lst" -o "$HERE/vdp_dma_fill.p" "$HERE/vdp_dma_fill.asm"
"$TOOLS/p2bin" "$HERE/vdp_dma_fill.p" "$HERE/vdp_dma_fill.bin" >/dev/null

echo "built $HERE/vdp_dma_fill.bin ($(stat -c%s "$HERE/vdp_dma_fill.bin") bytes)"
echo "label addresses:"
grep -iE 'Done |GenHalt |Init ' "$HERE/vdp_dma_fill.lst" | grep -iE ' C \|' || true
