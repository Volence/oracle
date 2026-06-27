#!/usr/bin/env bash
# Fetch the pinned SingleStepTests 680x0 suite (https://github.com/SingleStepTests/680x0).
# Reproducible by construction: a pinned commit SHA + a sha256 manifest. Output lands in the
# gitignored vendor/ dir; the 68000 test runner reads it (and skips cleanly if it is absent).
set -euo pipefail

PIN="e0d5ece9670205cc84a0101081837deb446f86a3"
BASE="https://raw.githubusercontent.com/SingleStepTests/680x0/${PIN}/68000/v1"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$HERE/../vendor/ProcessorTests/68000/v1"
CHECKSUMS="$HERE/singlesteptests.sha256"

# Files needed by the 68000 slice. Extend as opcode coverage grows.
FILES=(ADD.w SUB.w ADD.b SUB.b ADD.l SUB.l MOVE.w MOVE.b MOVE.l MOVEA.w MOVEA.l Bcc BSR JMP JSR RTS DBcc RTR TRAP RTE TRAPV CHK ANDItoSR ORItoSR EORItoSR RESET CMP.b CMP.w CMP.l CMPA.w CMPA.l TST.b TST.w TST.l CLR.b CLR.w CLR.l MOVE.q ADDA.w ADDA.l SUBA.w SUBA.l AND.b AND.w AND.l OR.b OR.w OR.l EOR.b EOR.w EOR.l NEG.b NEG.w NEG.l NEGX.b NEGX.w NEGX.l NOT.b NOT.w NOT.l EXT.w EXT.l SWAP Scc TAS BTST BCHG BCLR BSET ASL.b ASL.w ASL.l ASR.b ASR.w ASR.l LSL.b LSL.w LSL.l LSR.b LSR.w LSR.l ROL.b ROL.w ROL.l ROR.b ROR.w ROR.l ROXL.b ROXL.w ROXL.l ROXR.b ROXR.w ROXR.l MULU MULS)

mkdir -p "$OUT"
for f in "${FILES[@]}"; do
  echo "fetching $f.json.gz"
  curl -fsSL -o "$OUT/$f.json.gz" "$BASE/$f.json.gz"
done

echo "verifying checksums"
( cd "$OUT" && sha256sum -c "$CHECKSUMS" )

for f in "${FILES[@]}"; do
  gunzip -kf "$OUT/$f.json.gz"
done

echo "vendored to $OUT"
