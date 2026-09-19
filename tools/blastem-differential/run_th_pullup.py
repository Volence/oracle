#!/usr/bin/env python3
"""Undriven-TH pull direction, over BlastEm's GDB-remote stub (the INDEPENDENT arm).

`docs/2026-07-17-io-recon.md` IO3 pins that a wire configured as input with
nothing driving it floats HIGH. Our own core implements that rule, so asking our
core is a consistency check and not evidence -- an expectation derived from the
thing under test cannot fail. This runs the same ROM image (`th_pullup.bin`,
built by `build_th_pullup.py`) through a second, independently written model.

What this arm CAN prove: that a model nobody here wrote agrees (or does not).
What it CANNOT prove: hardware. Two models can inherit the same documentation
error, and BlastEm has already been caught with a blind spot in this very rig
(STOP x trace -- see README). Treat a disagreement as the interesting result.

Run:  ./build_th_pullup.py && python3 run_th_pullup.py
"""
import os
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, _HERE)
from rsp import RSP, watchdog  # noqa: E402
from build_th_pullup import assemble  # noqa: E402

ROM = os.path.join(_HERE, "th_pullup.bin")

LABEL = {
    0: ("$A10003 P1 Data  PRISTINE", "THE QUESTION: $FF = pull HIGH, $B3 = pull LOW"),
    1: ("$A10009 P1 Ctrl  PRISTINE", "premise: Control untouched at $00"),
    2: ("$A10005 P2 Data  PRISTINE", "the same question, second port"),
    3: ("$A1000B P2 Ctrl  PRISTINE", "premise: Control untouched at $00"),
    4: ("$A10001 Version", "the I/O block answers, and not with $FF"),
    5: ("$A10003 latch=$40 ctrl=$00", "$FF = pull HIGH or latch echo; $B3 = pull LOW"),
    6: ("$A10003 ctrl=$40 latch=$40", "CONTROL, TH DRIVEN high: $FF under every hypothesis"),
    7: ("$A10003 ctrl=$40 latch=$00", "CONTROL, TH DRIVEN low: $B3 under every hypothesis"),
    8: ("$A10003 back to ctrl=$00", "$FF = pull HIGH; $B3 = pull LOW or latch echo"),
    9: ("$A10009 ctrl read back", "expect $00"),
}


def verdict(o):
    """Read the three-way discriminator off observables +0, +5, +8. See build_th_pullup.py."""
    p0, p5, p8 = o[0], o[5], o[8]
    if (p0, p5, p8) == (0xFF, 0xFF, 0xFF):
        return "PULL-UP HIGH (IO3 as pinned)"
    if (p0, p5, p8) == (0xB3, 0xB3, 0xB3):
        return "PULL-DOWN LOW -- IO3 IS WRONG, STOP AND REPORT"
    if (p0, p5, p8) == (0xB3, 0xFF, 0xB3):
        return "input pin ECHOES THE LATCH (says nothing about the pull)"
    return "UNRECOGNISED PATTERN -- not one of the three modelled hypotheses"


def main():
    watchdog(200)
    fresh, labels, _ = assemble()
    with open(ROM, "rb") as f:
        on_disk = f.read()
    if on_disk != fresh:
        print("REFUSED: %s does not match a fresh assembly; run ./build_th_pullup.py" % ROM)
        return 2

    print("=== undriven-TH pull direction: what does $A10003 read with Control at $00? ===")
    print("(BlastEm 0.6.2 as a black-box instrument, nothing held on the pad)")

    r = RSP(ROM)
    try:
        r.wait_ready()
        # Poison the observables so a byte we never wrote cannot read as a measurement.
        r.write_mem(0x00FF8000, bytes([0x5A] * 0x12))
        for name in ("Done", "GenHalt"):
            r.bp(labels[name])
        _, timed_out = r.cont_or_interrupt(8.0)
        mem = None if timed_out else r.read_mem(0x00FF8000, 0x12)
    finally:
        r.close()

    if mem is None:
        print("DID NOT MEASURE: the ROM never reached its done marker (timed_out=%s)." % timed_out)
        print("This is an absence, not a finding.")
        return 1
    marker = int.from_bytes(mem[0x10:0x12], "big")
    if marker != 0xC0DE:
        what = "an exception vector was taken" if marker == 0xDEAD else "the run is incomplete"
        print("DID NOT MEASURE: done marker is $%04X (%s)." % (marker, what))
        print("This is an absence, not a finding.")
        return 1

    o = list(mem[0:10])
    for i in range(10):
        what, means = LABEL[i]
        print("  +%d  %-26s = $%02X  %s" % (i, what, o[i], means))

    bad = []
    if o[1] != 0x00 or o[3] != 0x00:
        bad.append("PREMISE FAILED: a Control register did not power on at $00 (+1=$%02X +3=$%02X); the "
                   "'untouched at $00' reading below is not what it claims" % (o[1], o[3]))
    if o[6] != 0xFF:
        bad.append("CONTROL FAILED: TH driven HIGH read $%02X, not $FF -- the model does not present an "
                   "all-released pad, so the main cell's $FF (if any) may mean nothing" % o[6])
    if o[7] != 0xB3:
        bad.append("CONTROL FAILED: TH driven LOW read $%02X, not $B3 -- a non-$FF byte was never shown to "
                   "be observable through this path, so $FF elsewhere is not a measurement" % o[7])
    for line in bad:
        print("  !! " + line)

    print("\nVERDICT (from +0/+5/+8 = $%02X/$%02X/$%02X): %s" % (o[0], o[5], o[8], verdict(o)))
    if bad:
        print("WITH THE CONTROLS FAILING ABOVE, that verdict is NOT evidence.")
    print("DONE")
    return 0


if __name__ == "__main__":
    sys.exit(main())
