#!/usr/bin/env python3
"""VDP control-port pending-toggle experiment, over BlastEm's GDB-remote stub.

VDP recon R1's open cell: which accesses clear the control port's
first/second-write toggle? Permitted docs pin the data-port-write clear
(Kabuto) and are silent on status reads / HV reads. Per cell, the ROM
(vdp_pending.asm) arms the toggle, applies one probe, then writes an ambiguous
word whose interpretation (second word vs first word) routes a $BBBB sentinel
to VRAM $0200 (toggle still armed) or $0300 (toggle cleared). Fresh BlastEm
session per cell.

Run:  ./build_vdp_pending.sh && python3 run_vdp_pending.py
"""
import os
import re
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, _HERE)
from rsp import RSP, watchdog

ROM = os.path.join(_HERE, "vdp_pending.bin")
LST = os.path.join(_HERE, "vdp_pending.lst")
NAME = {
    0: "control (no probe)",
    1: "status read",
    2: "HV counter read",
    3: "data-port write",
}


def labels():
    """Parse label -> address from the ASL listing symbol table."""
    out = {}
    with open(LST) as f:
        for m in re.finditer(r"\*?(\w+)\s*:\s*([0-9A-Fa-f]+)\s+C\b", f.read()):
            out[m.group(1)] = int(m.group(2), 16)
    return out


def safe_mem(r, addr, ln):
    try:
        d = r.cmd(f"m{addr:x},{ln:x}", 2.0)
        return bytes.fromhex(d.decode()) if d else None
    except Exception:
        return None


def verdict(v0200, v0300, v0202, sel):
    if sel == 3:
        if v0200 == 0xCCCC and v0300 == 0xBBBB:
            return "CLEARS the toggle"
        if v0200 == 0xCCCC and v0202 == 0xBBBB:
            return "does NOT clear (BBBB landed at the autoincremented $0202)"
        return "UNEXPECTED"
    if v0200 == 0xBBBB and v0300 == 0x2222:
        return "does NOT clear (ambiguous word completed the armed command)"
    if v0200 == 0x1111 and v0300 == 0xBBBB:
        return "CLEARS the toggle (ambiguous word re-armed as a first word)"
    return "UNEXPECTED"


def cell(L, sel):
    r = RSP(ROM)
    try:
        r.wait_ready()
        r.write_mem(0xFF9000, bytes([sel]))
        r.write_mem(0xFF8000, bytes(0x12))  # clear observables + done marker
        for name in ("Done", "GenHalt"):
            r.bp(L[name])
        rep, timed_out = r.cont_or_interrupt(5.0)
        v0100 = v0200 = v0300 = v0202 = probe = marker = -1
        mem = None if timed_out else safe_mem(r, 0xFF8000, 0x12)
        if mem:
            v0100 = int.from_bytes(mem[0:2], "big")
            v0200 = int.from_bytes(mem[2:4], "big")
            v0300 = int.from_bytes(mem[4:6], "big")
            v0202 = int.from_bytes(mem[6:8], "big")
            probe = int.from_bytes(mem[8:10], "big")
            marker = int.from_bytes(mem[0x10:0x12], "big")
        status = "ok" if marker == 0xC0DE else ("EXCEPTION" if marker == 0xDEAD else "INCOMPLETE")
        print(
            "sel=%d %-18s -> %-8s | $0100=%04x $0200=%04x $0300=%04x $0202=%04x probe=%04x | %s"
            % (sel, NAME[sel], status, v0100 & 0xFFFF, v0200 & 0xFFFF, v0300 & 0xFFFF,
               v0202 & 0xFFFF, probe & 0xFFFF,
               verdict(v0200, v0300, v0202, sel) if status == "ok" else "no verdict")
        )
    finally:
        r.close()


def main():
    watchdog(220)
    L = labels()
    print("=== VDP control-port pending-toggle: what clears the first-write state? ===")
    print("(BlastEm 0.6.2 as black-box instrument; sentinel $0100=AAAA $0200=1111 $0300=2222)")
    for sel in (0, 1, 2, 3):
        cell(L, sel)
    print("DONE")


if __name__ == "__main__":
    main()
