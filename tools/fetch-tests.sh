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
FILES=(ADD.w SUB.w ADD.b SUB.b ADD.l SUB.l MOVE.w MOVE.b MOVE.l MOVEA.w MOVEA.l Bcc BSR JMP JSR RTS DBcc)

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
