#!/usr/bin/env python3
"""VDP DMA-fill + data-port-read experiment, over BlastEm's GDB-remote stub.

Two cells for the DMA + FIFO push (recon R4(b) / R1), driven exactly like
run_vdp_pending.py (BlastEm 0.6.2 as a black-box instrument under xvfb).

Cell 0 (VRAM fill baseline, R4(b)): an 8-byte VRAM fill of $EE at $0400. Reads
VRAM $0400/$0402/$0406 back — all $EEEE confirms the fill writes the top byte
to consecutive addresses (autoinc 1) under DMA-enable. This pins the concrete
behavior slice F implements.

Cell 1 (data-port read while a WRITE command is armed, R1 open cell): recon R1
pins this as a hardware lockup. A watchdog timeout (no done marker) => BlastEm
models the lockup as a hang, consistent with the pin; a returned value is
recorded honestly. Low-exposure cell — recorded, not forced into a pin.

Run:  ./build_vdp_dma_fill.sh && python3 run_vdp_dma_fill.py
"""
import os
import re
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, _HERE)
from rsp import RSP, watchdog

ROM = os.path.join(_HERE, "vdp_dma_fill.bin")
LST = os.path.join(_HERE, "vdp_dma_fill.lst")
NAME = {0: "VRAM fill baseline", 1: "data-read (write-armed)"}


def labels():
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


def cell(L, sel):
    r = RSP(ROM)
    try:
        r.wait_ready()
        r.write_mem(0xFF9000, bytes([sel]))
        r.write_mem(0xFF8000, bytes(0x12))  # clear observables + done marker
        for name in ("Done", "GenHalt"):
            r.bp(L[name])
        _rep, timed_out = r.cont_or_interrupt(6.0)
        v0400 = v0402 = v0406 = dread = marker = -1
        mem = None if timed_out else safe_mem(r, 0xFF8000, 0x12)
        if mem:
            v0400 = int.from_bytes(mem[0:2], "big")
            v0402 = int.from_bytes(mem[2:4], "big")
            v0406 = int.from_bytes(mem[4:6], "big")
            dread = int.from_bytes(mem[8:10], "big")
            marker = int.from_bytes(mem[0x10:0x12], "big")
        if timed_out or marker == -1:
            status = "HUNG"
        elif marker == 0xC0DE:
            status = "ok"
        elif marker == 0xDEAD:
            status = "EXCEPTION"
        else:
            status = "INCOMPLETE"
        if sel == 0:
            filled = v0400 == 0xEEEE and v0402 == 0xEEEE and v0406 == 0xEEEE
            verdict = (
                "VRAM fill wrote $EE to all 8 bytes (top-byte source, autoinc 1)"
                if filled
                else "fill did NOT match — see raw values"
            )
        else:
            verdict = (
                "HUNG => lockup confirmed (BlastEm models the data-read-while-write-armed hang)"
                if status == "HUNG"
                else f"did NOT hang; data-read returned {dread:04x} (recorded, not a pin)"
            )
        print(
            "sel=%d %-24s -> %-9s | $0400=%04x $0402=%04x $0406=%04x dread=%04x | %s"
            % (sel, NAME[sel], status, v0400 & 0xFFFF, v0402 & 0xFFFF,
               v0406 & 0xFFFF, dread & 0xFFFF, verdict)
        )
    finally:
        r.close()


def main():
    watchdog(120)
    L = labels()
    print("=== VDP DMA fill + data-port-read (BlastEm 0.6.2 black-box instrument) ===")
    for sel in (0, 1):
        cell(L, sel)
    print("DONE")


if __name__ == "__main__":
    main()
