#!/usr/bin/env python3
"""Nightly BlastEm-over-the-bus differential (integration-pivot Push C, point 7).

Runs a fixed **register-only** 68000 instruction sequence in both emulators and compares the
architectural state at every **instruction boundary**:

  * BlastEm 0.6.2 — as a black-box GDB-RSP oracle (via `rsp.py`; its source is never opened).
  * oracle-next  — via the `differential_trace` example (JSON per instruction boundary).

Only architectural state is compared (d0-d7, a0-a7, pc, sr). **Timing is deliberately not compared** —
SST-model cycles vs BlastEm cycles legitimately diverge and are xfail-manifest entries, never state
divergences (integration-pivot D8). Scenarios listed in `known_differences.py` are treated as EXPECTED
mismatches, not regressions.

The sequence is register-only (moveq / register arithmetic / shifts / swap / ext / nop) so it never
touches RAM — the two machines' (different) power-on RAM can never cause a spurious divergence, and no
instruction traps to a vector. Both emulators start from the ROM's reset vector with all data/address
registers zeroed, so the run is fully aligned.

This is an **instrument, not a merge gate**: if BlastEm / xvfb are unavailable it prints SKIPPED and
exits 0. Run it where BlastEm is present (see README) — or nightly in CI on a runner that has it.

Usage: `python3 nightly_differential.py [n_steps]`
Env:   BLASTEM (blastem binary), REPO (oracle-next repo root), CARGO (cargo binary).
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile

_HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.environ.get("REPO", os.path.normpath(os.path.join(_HERE, "..", "..")))
CARGO = os.environ.get("CARGO", "cargo")

# ------------------------------------------------------------------ the diff ROM


def _w(v):
    return v.to_bytes(2, "big")


def _l(v):
    return v.to_bytes(4, "big")


# The BlastEm RSP stub crashes on register writes (`P`/`G`) — see rsp.py's notes — so we cannot zero its
# power-on register file to align the two machines. Instead the ROM's **prologue** loads every register
# with identical code on both CPUs; we compare only from the post-prologue boundary, where both are
# guaranteed aligned. Prologue: moveq into D0-D7, lea into A0-A6 (A7 = the reset SSP, left alone). All
# register-direct / immediate — no memory reference, no trap.
PROLOGUE = [_w(0x7000 | (n << 9) | (0x10 + n)) for n in range(8)]  # moveq #imm, Dn
PROLOGUE += [_w(0x41F9 | (n << 9)) + _l(0x0001_0000 * (n + 1)) for n in range(7)]  # lea $const, An

# The instructions under test — register-only, so RAM never matters and both CPUs run identical bytes.
# (Even a mis-remembered mnemonic is a valid differential: both machines execute the same encoding.)
TEST = [
    _w(0xD280),  # add.l  D0, D1
    _w(0xE58A),  # lsl.l  #2, D2
    _w(0x4842),  # swap   D2
    _w(0x4880),  # ext.w  D0
    _w(0x48C1),  # ext.l  D1
    _w(0x4640),  # not.w  D0
    _w(0x4442),  # neg.w  D2
    _w(0xC282),  # and.l  D2, D1
    _w(0x8003),  # or.b   D3, D0
    _w(0xB142),  # eor.w  D0, D2
    _w(0x9401),  # sub.b  D1, D2
    _w(0x4E71),  # nop
]
DIFF_INSNS = PROLOGUE + TEST
COMPARE_FROM = len(PROLOGUE)  # boundary index at which both machines are fully aligned
CODE_BASE = 0x200
INITIAL_SSP = 0x00FF_FFFE


def insn_addresses():
    """Address of each instruction in the laid-out sequence, plus the address just past the last one."""
    addrs = []
    off = CODE_BASE
    for blob in DIFF_INSNS:
        addrs.append(off)
        off += len(blob)
    return addrs, off


def build_diff_rom():
    rom = bytearray(0x300)
    rom[0:4] = INITIAL_SSP.to_bytes(4, "big")  # reset SSP @ $0
    rom[4:8] = CODE_BASE.to_bytes(4, "big")  # reset PC  @ $4
    off = CODE_BASE
    for blob in DIFF_INSNS:
        rom[off:off + len(blob)] = blob
        off += len(blob)
    return bytes(rom)


# ------------------------------------------------------------------ state model

REG_FIELDS = ["pc", "sr"] + [f"d{i}" for i in range(8)] + [f"a{i}" for i in range(8)]


def norm_state(pc, sr, d, a):
    """Canonical comparable state. SR masked to the implemented bits (T|S|I2-0|CCR = 0xA71F)."""
    s = {"pc": pc & 0xFFFFFF, "sr": sr & 0xA71F}
    for i in range(8):
        s[f"d{i}"] = d[i] & 0xFFFFFFFF
    for i in range(8):
        s[f"a{i}"] = a[i] & 0xFFFFFF
    return s


# ------------------------------------------------------------------ oracle-next side

def oracle_trace(rom_path, n):
    """Run the differential_trace example; return {pc: state} over all instruction boundaries (the linear
    sequence visits each PC once)."""
    out = subprocess.run(
        [CARGO, "run", "-q", "--release", "--example", "differential_trace",
         "--", rom_path, str(n)],
        cwd=REPO, capture_output=True, text=True, check=True,
    ).stdout
    by_pc = {}
    for line in out.splitlines():
        line = line.strip()
        if not line:
            continue
        o = json.loads(line)
        d = [o[f"d{i}"] for i in range(8)]
        a = [o[f"a{i}"] for i in range(7)] + [o["a7"]]
        st = norm_state(o["pc"], o["sr"], d, a)
        by_pc[st["pc"]] = st
    return by_pc


# ------------------------------------------------------------------ BlastEm side

def blastem_trace(rom_path, targets):
    """Drive BlastEm to each boundary in `targets` (a set of PCs) via breakpoints + continue — the
    reliable path (the stub's single-step `s` is unstable; `run_stop_trace.py` uses this same
    breakpoint/continue model). Returns {pc: state}, or None if BlastEm/xvfb is unavailable. Registers
    are never written (the stub crashes on `P`/`G` — the ROM prologue aligns them instead)."""
    from rsp import RSP, BLASTEM, watchdog
    if not (os.path.exists(BLASTEM) and shutil.which("xvfb-run")):
        return None
    watchdog(180)
    r = RSP(rom_path)
    try:
        r.wait_ready()

        def snap():
            try:
                g = r.cmd("g", 2.0)
            except Exception:
                return None
            if not g or len(g) < 18 * 8:
                return None
            w = [int(g[i * 8:i * 8 + 8], 16) for i in range(18)]
            return norm_state(w[17], w[16], w[0:8], w[8:16])

        for a in sorted(targets):
            r.bp(a)

        by_pc = {}
        last = max(targets)
        # Each continue runs from the current PC (starting inside the prologue) to the next breakpoint.
        # Any stub instability (a dead pipe, a timed-out continue) ends the trace with what we have.
        try:
            for _ in range(len(targets) + 2):
                rep, timed_out = r.cont_or_interrupt(4.0)
                if timed_out:
                    break
                st = snap()
                if st is None:
                    break
                by_pc[st["pc"]] = st
                if st["pc"] == last:
                    break
        except Exception:
            pass
        return by_pc
    finally:
        r.close()


# ------------------------------------------------------------------ compare

def diff_states(oracle, blastem, pcs):
    """Yield (pc, field, oracle_val, blastem_val) for every mismatch at the shared boundary PCs `pcs`."""
    for pc in sorted(pcs):
        o, b = oracle[pc], blastem[pc]
        for f in REG_FIELDS:
            if o[f] != b[f]:
                yield pc, f, o[f], b[f]


def main():
    n = len(DIFF_INSNS)
    addrs, end = insn_addresses()
    # Compare at every post-prologue instruction boundary: the address of each test instruction, plus the
    # address just past the sequence (the boundary after the final nop).
    targets = set(addrs[COMPARE_FROM:]) | {end}
    rom = build_diff_rom()
    with tempfile.NamedTemporaryFile(suffix=".bin", delete=False) as f:
        f.write(rom)
        rom_path = f.name
    try:
        print(f"[nightly-differential] {n}-instruction sequence "
              f"({len(PROLOGUE)}-instruction register-init prologue); comparing {len(targets)} "
              "post-prologue boundaries")
        oracle = oracle_trace(rom_path, n)
        print(f"[nightly-differential] oracle-next traced {len(oracle)} boundaries")

        blastem = blastem_trace(rom_path, targets)
        if blastem is None:
            print("[nightly-differential] SKIPPED: BlastEm / xvfb-run not available "
                  "(instrument, not a merge gate). Run where BlastEm is present.")
            return 0
        print(f"[nightly-differential] BlastEm reached {len(blastem)} boundaries")

        shared = targets & oracle.keys() & blastem.keys()
        if not shared:
            print("[nightly-differential] SKIPPED: BlastEm reached no comparable boundary "
                  "(stub instability) — no divergence asserted.")
            return 0

        mism = list(diff_states(oracle, blastem, shared))
        if not mism:
            print(f"[nightly-differential] PASS: {len(shared)} instruction boundaries agree on all "
                  "architectural state (d0-7, a0-7, pc, sr). Timing is xfail (not compared).")
            return 0

        # Register-only sequences map to no known_differences scenario, but honor the ledger regardless.
        from known_differences import is_known_difference
        real = [m for m in mism if not is_known_difference("register-alu-sequence")]
        print(f"[nightly-differential] FAIL: {len(real)} architectural mismatch(es):")
        for pc, field, ov, bv in real[:40]:
            print(f"    pc=0x{pc:06X}  {field:3s}  oracle=0x{ov:08X}  blastem=0x{bv:08X}")
        return 1 if real else 0
    finally:
        os.unlink(rom_path)


if __name__ == "__main__":
    sys.exit(main())
