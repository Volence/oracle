#!/usr/bin/env python3
"""Compare two VGM captures as register-write triple sequences (timing-independent).

Parses the VGM command stream, extracting YM2612 (0x52/0x53) and PSG (0x50) register
writes; strips all wait commands. Reports per-chip counts, DAC ($2A) vs melody split,
and how far the two melody-write sequences agree order-for-order.
"""
import sys, struct

def parse(path):
    data = open(path, "rb").read()
    assert data[:4] == b"Vgm ", f"{path}: not a VGM (got {data[:4]!r})"
    # data offset field at 0x34 is relative to 0x34; older/1.50 files may have 0 -> data at 0x40.
    doff = struct.unpack_from("<I", data, 0x34)[0]
    start = (0x34 + doff) if doff else 0x40
    i = start
    n = len(data)
    writes = []  # (chip, port, reg, value)
    waits = 0
    while i < n:
        c = data[i]
        if c == 0x66:  # end
            break
        elif c == 0x50:  # PSG
            writes.append(("PSG", 0, 0, data[i+1])); i += 2
        elif c == 0x52:  # YM2612 port 0
            writes.append(("YM", 0, data[i+1], data[i+2])); i += 3
        elif c == 0x53:  # YM2612 port 1
            writes.append(("YM", 1, data[i+1], data[i+2])); i += 3
        elif c == 0x61:  # wait nnnn
            waits += struct.unpack_from("<H", data, i+1)[0]; i += 3
        elif c == 0x62:  # wait 735
            waits += 735; i += 1
        elif c == 0x63:  # wait 882
            waits += 882; i += 1
        elif 0x70 <= c <= 0x7F:  # wait (n+1)
            waits += (c & 0xF) + 1; i += 1
        elif 0x80 <= c <= 0x8F:  # YM2612 port0 $2A write from data bank, then wait n
            writes.append(("YM", 0, 0x2A, 0)); waits += (c & 0xF); i += 1  # value from PCM bank (opaque)
        elif c == 0x67:  # data block: 0x67 0x66 tt ss ss ss ss  (+ ss bytes)
            size = struct.unpack_from("<I", data, i+3)[0]; i += 7 + size
        elif c == 0xE0:  # seek PCM
            i += 5
        elif c == 0x4F or c == 0x51:  # GG stereo / YM2413 (unused here) 2 operands
            i += 2
        else:
            # Unknown command; try to skip conservatively (most 0xNN with 2 operands).
            i += 1
    return writes, waits

def split(ws):
    dac = [w for w in ws if w[0] == "YM" and w[2] == 0x2A]
    melody = [w for w in ws if not (w[0] == "YM" and w[2] == 0x2A)]
    return melody, dac

def main():
    ours_p, orac_p = sys.argv[1], sys.argv[2]
    ours, ours_w = parse(ours_p)
    orac, orac_w = parse(orac_p)
    om, od = split(ours)
    rm, rd = split(orac)
    print(f"OURS   : {len(ours):6d} writes  ({len(om)} melody + {len(od)} DAC)  waits={ours_w}")
    print(f"ORACLE : {len(orac):6d} writes  ({len(rm)} melody + {len(rd)} DAC)  waits={orac_w}")
    # Compare melody sequences order-for-order over the common prefix.
    common = min(len(om), len(rm))
    match = 0
    first_mismatch = None
    for k in range(common):
        if om[k] == rm[k]:
            match += 1
        elif first_mismatch is None:
            first_mismatch = k
    print(f"\nMELODY sequence: common prefix = {common}; exact-position matches = {match} "
          f"({100*match/common:.1f}%)")
    if first_mismatch is not None:
        k = first_mismatch
        print(f"first divergence at melody index {k}:")
        print(f"  ours  [{k-1}..{k+2}]: {om[max(0,k-1):k+3]}")
        print(f"  oracle[{k-1}..{k+2}]: {rm[max(0,k-1):k+3]}")
    else:
        print("melody prefixes IDENTICAL over the full common length.")
    # Multiset comparison (order-independent) as a second lens.
    from collections import Counter
    co, cr = Counter(om), Counter(rm)
    only_ours = sum((co - cr).values()); only_orac = sum((cr - co).values())
    print(f"\nMELODY multiset: {only_ours} writes only-in-ours, {only_orac} only-in-oracle "
          f"(of {len(om)}/{len(rm)})")

    # NOTES-ONLY: exclude the per-frame timer-control heartbeat ($24-$27), which is a control
    # write whose interleaving with notes is timing-sensitive. Does the musical note stream match deeper?
    def notes(seq):
        return [w for w in seq if not (w[0] == "YM" and 0x24 <= w[2] <= 0x27)]
    on, rn = notes(om), notes(rm)
    c2 = min(len(on), len(rn)); m2 = 0; fm2 = None
    for k in range(c2):
        if on[k] == rn[k]:
            m2 += 1
        elif fm2 is None:
            fm2 = k
    print(f"\nNOTES-ONLY (excl. $24-$27): common={c2}; exact matches={m2} ({100*m2/c2:.1f}%)")
    if fm2 is not None:
        print(f"  first notes divergence at index {fm2}:")
        print(f"    ours  : {on[max(0,fm2-1):fm2+3]}")
        print(f"    oracle: {rn[max(0,fm2-1):fm2+3]}")
    else:
        print("  note prefixes IDENTICAL over the full common length.")

if __name__ == "__main__":
    main()
