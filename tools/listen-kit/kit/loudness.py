#!/usr/bin/env python3
"""loudness.py WAV... : per-file RMS and the fraction of one-second windows that are not silent."""
import array, math, sys

for path in sys.argv[1:]:
    b = open(path, "rb").read()
    x = array.array("h", b[44:])
    rate = int.from_bytes(b[24:28], "little")
    win = rate * 2
    loud = []
    for w in range(0, len(x), win):
        seg = x[w:w + win]
        r = math.sqrt(sum(v * v for v in seg) / max(len(seg), 1))
        loud.append(r >= 32.768)
    total = math.sqrt(sum(v * v for v in x) / max(len(x), 1))
    secs = [i for i, l in enumerate(loud) if l]
    span = f"{secs[0]}-{secs[-1]}" if secs else "none"
    print(f"{path}: RMS {total:.1f}, non-silent seconds {sum(loud)} of {len(loud)} (span {span})")
