#!/usr/bin/env python3
"""STOP x trace 2x2 differential + NOP controls, over BlastEm's GDB-remote stub.

Two T bits are in play for STOP: the start-T (the SR just before STOP) and the
loaded-T (the T bit of STOP's immediate). This sweeps the full 2x2 plus NOP
controls that validate the harness actually detects a trace.

Selection flows through a writable RAM control block (PC/PC-register writes are
unavailable over this stub); the ROM dispatches and its handlers record the
observable side effects. Fresh BlastEm session per cell.

Run:  python3 run_stop_trace.py         # build_rom.sh must have produced harness.bin
See docs/plans/2026-07-16-m68000-blastem-differential.md for the recorded results
and the known-differences ledger (known_differences.py).
"""
import os
import re
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, _HERE)
from rsp import RSP, watchdog

ROM = os.path.join(_HERE, "harness.bin")
LST = os.path.join(_HERE, "harness.lst")
SR_T1, SR_T0 = 0xA700, 0x2700           # start-T = 1 / 0 (S=1, mask=7 in both)
NAME = {0: "STOP#2700(load0)", 1: "STOP#A700(load1)", 2: "NOP-control"}


def labels():
    """Parse label -> address from the ASL listing symbol table."""
    out = {}
    with open(LST) as f:
        for m in re.finditer(r'\*?(\w+)\s*:\s*([0-9A-Fa-f]+)\s+C\b', f.read()):
            out[m.group(1)] = int(m.group(2), 16)
    return out


def safe_regs(r):
    try:
        g = r.cmd('g', 2.0)
        if not g or len(g) < 18 * 8:
            return None
        w = [int(g[i * 8:i * 8 + 8], 16) for i in range(18)]
        return {'sr': w[16] & 0xFFFF, 'pc': w[17]}
    except Exception:
        return None


def safe_mem(r, addr, ln):
    try:
        d = r.cmd(f'm{addr:x},{ln:x}', 2.0)
        return bytes.fromhex(d.decode()) if d else None
    except Exception:
        return None


def cell(L, sT, sel):
    r = RSP(ROM)
    try:
        r.wait_ready()
        r.write_mem(0xFF9000, bytes([sel]))                          # instruction selector
        r.write_mem(0xFF9002, (SR_T1 if sT else SR_T0).to_bytes(2, 'big'))  # start SR
        r.write_mem(0xFF8000, bytes(16))                             # clear markers
        for name in ('TraceHalt', 'GenHalt', 'FT0', 'FT1', 'FTN'):
            r.bp(L[name])
        rep, timed_out = r.cont_or_interrupt(3.0)
        marker = spc = ssr = fpc = fsr = -1
        if not timed_out:
            mark = safe_mem(r, 0xFF8000, 12)
            if mark:
                marker = int.from_bytes(mark[0:2], 'big')
                spc = int.from_bytes(mark[4:8], 'big')
                ssr = int.from_bytes(mark[8:10], 'big')
            reg = safe_regs(r)
            if reg:
                fpc, fsr = reg['pc'], reg['sr']
        if marker == 0xA909:
            outcome = "TRACE"
        elif marker == 0xDEAD:
            outcome = "OTHER-EXC"
        elif marker >= 0 and (marker & 0xFF00) == 0xFE00:
            outcome = "FELL-THROUGH"
        elif timed_out:
            outcome = "STOPPED"
        else:
            outcome = "?(m=%x)" % marker
        print("startT=%d %-16s -> %-13s | stackedPC=%08x stackedSR=%04x | reply=%s"
              % (sT, NAME[sel], outcome, spc & 0xFFFFFFFF, ssr & 0xFFFF, rep))
    finally:
        r.close()


def main():
    watchdog(220)
    L = labels()
    print("=== NOP controls (validate the harness detects a trace) ===")
    cell(L, 0, 2)   # start-T=0 NOP -> expect FELL-THROUGH
    cell(L, 1, 2)   # start-T=1 NOP -> expect TRACE
    print("=== STOP x trace 2x2 ===")
    for sT in (0, 1):
        for sel in (0, 1):
            cell(L, sT, sel)
    print("DONE")


if __name__ == '__main__':
    main()
