#!/usr/bin/env bash
# Fetch the pinned SingleStepTests z80 suite (https://github.com/SingleStepTests/z80).
# Reproducible by construction: a pinned commit SHA + a sha256 manifest. Output lands in the
# gitignored vendor/ dir; the z80 test runner reads it (and skips cleanly if it is absent).
#
# NOTE: this is external TEST DATA (expected {initial -> final} register+RAM state per opcode),
# NOT emulator source. It is the z80 analog of tools/fetch-tests.sh (the 68000 SST corpus). The
# Z80 core itself is written from scratch, clean-room; this is only the answer key we grade it
# against. The corpus also carries a per-cycle bus trace ("cycles") which the instruction-atomic
# core does not reproduce — the runner gates on "final" register+RAM state and ignores "cycles".
#
# Layout differs from the 68000 set: files are UNCOMPRESSED .json under v1/, named by opcode hex;
# prefixed opcodes contain a space (e.g. "cb 40.json", "dd cb __ 06.json").
set -euo pipefail

PIN="ebe1875d48f374bcfd4b505d8eb8ee751568b5f7"
BASE="https://raw.githubusercontent.com/SingleStepTests/z80/${PIN}/v1"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$HERE/../vendor/ProcessorTests/z80/v1"
CHECKSUMS="$HERE/singlesteptests-z80.sha256"

# Opcodes needed by the z80 slice. Extend as opcode coverage grows — same model as the 68000
# fetch script's FILES list. This batch completes the whole UN-PREFIXED base table: every opcode
# 0x00-0xFF except the four prefix bytes 0xCB/0xDD/0xED/0xFD (those dispatch into the CB/DD/ED/FD
# tables, which land in the later prefix-group slices). This latest addition is the branch/stack
# control flow — DJNZ/JR/JR cc (0x10,0x18,0x20,0x28,0x30,0x38), JP/JP cc, CALL/CALL cc, RET/RET cc,
# RST, PUSH/POP across 0xC0-0xFF, plus EXX (0xD9) — filling the remaining base-table holes.
FILES=()
for i in $(seq 0 255); do
  case $i in 203|221|237|253) continue;; esac                       # skip 0xCB/0xDD/0xED/0xFD prefix bytes
  FILES+=("$(printf '%02x' "$i")")
done

mkdir -p "$OUT"
for f in "${FILES[@]}"; do
  echo "fetching $f.json"
  # URL-encode the space in prefixed names; keep the space in the local filename (matches the repo).
  url_f="${f// /%20}"
  curl -fsSL -o "$OUT/$f.json" "$BASE/$url_f.json"
done

if [[ -f "$CHECKSUMS" ]]; then
  echo "verifying checksums"
  ( cd "$OUT" && sha256sum -c "$CHECKSUMS" )
else
  echo "no manifest yet — generating $CHECKSUMS from this fetch (first run; commit it to pin)"
  ( cd "$OUT" && sha256sum -- *.json > "$CHECKSUMS" )
fi

echo "vendored to $OUT"
