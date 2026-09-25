#!/usr/bin/env bash
# diff-all.sh : sample-level before/after comparison of every render, plus the differ's own control.
here="$(cd "$(dirname "$0")" && pwd)"
kit="$here/after/listen-kit"
cd "$here/renders" || exit 1
st=0
for s in boot30s legA ojz slide st liveojz; do
  echo "=== $s: before vs after"
  "$kit" diff "$s-before.wav" "$s-after.wav" || st=1
  echo
done
# CONTROL 1: a file against itself must read IDENTICAL.
echo "=== control: legA-before vs itself (must be IDENTICAL)"
"$kit" diff legA-before.wav legA-before.wav || st=1
# CONTROL 2: one sample changed by 1 LSB, at a known place, must be found at exactly that place.
python3 - <<'EOF'
b = bytearray(open("legA-before.wav", "rb").read())
i = 44 + 2 * 441000          # sample index 441000 = stereo frame 220500 = t 5.0000 s at 44.1 kHz
v = int.from_bytes(b[i:i+2], "little", signed=True)
b[i:i+2] = (v + 1 if v < 32767 else v - 1).to_bytes(2, "little", signed=True)
open("control-onesample.wav", "wb").write(b)
EOF
echo "=== control: legA-before vs a copy with sample 441000 moved by 1 LSB (must report index 441000, t = 5.0000 s)"
"$kit" diff legA-before.wav control-onesample.wav || st=1
rm -f control-onesample.wav
echo "diff-all exit=$st"
exit $st
