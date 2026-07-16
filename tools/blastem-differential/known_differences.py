#!/usr/bin/env python3
"""Known-differences ledger for the BlastEm-over-the-bus differential.

The future nightly differential (Push C) imports this so it does NOT false-alarm
on cells where oracle-next intentionally diverges from BlastEm 0.6.2 because
BlastEm is a blind or limited instrument there. Each entry is a *documented*
divergence with the reference that governs the oracle-next behavior instead.

Format: a list of dicts. A differential harness should, for any (scenario) whose
key matches an entry, treat a BlastEm/oracle-next mismatch as EXPECTED (skip /
xfail), not a regression.
"""

KNOWN_DIFFERENCES = [
    {
        "id": "stop-x-trace",
        "scenario": "STOP with the trace bit set in the loaded SR (immediate)",
        "blastem_says": "STOPPED for all four (start-T x loaded-T) cells — BlastEm "
                        "0.6.2 does not model trace-on-STOP at all (it failed even the "
                        "(start-T=1, loaded-T=1) cell where the uniform-UM and loaded-T "
                        "rules agree; the NOP controls trace correctly, so the harness "
                        "detector is sound — this is an instrument blind spot).",
        "oracle_next_pins": "loaded-T rule (trace preempts stop): STOP with T set in the "
                            "LOADED SR services a trace exception instead of entering "
                            "Stopped; stacked PC = post-STOP (next instruction); pushed "
                            "SR = the loaded SR (T set). The two diagonal cells "
                            "(start-T=0/loaded-T=1 -> TRACE; start-T=1/loaded-T=0 -> "
                            "STOPPED) discriminate loaded-T from start-T.",
        "reference": "M68000 PRM STOP description ('The immediate operand is copied into "
                     "the entire status register ... A trace exception will occur if the "
                     "trace bit is set when the STOP instruction is encountered'); "
                     "M68000UM Sec 6.3.8. Owner decision 2026-07-16.",
        "action": "EXPECT-MISMATCH on STOP+trace cells; do not flag.",
    },
    {
        "id": "vdp-dataport-read-lockup",
        "scenario": "A VDP data-port READ issued while a WRITE command is armed (CD0 = 1)",
        "blastem_says": "On real hardware this HANGS the 68k until reset (Nemesis t=1291 / "
                        "Mask of Destiny t=2036: 'setup a write and then try to read -> the "
                        "68K will hang until the machine is reset').",
        "oracle_next_pins": "A deterministic modeled outcome: return the open-bus word and "
                            "latch Vdp.latched_fault (a debug flag); the host NEVER hangs. "
                            "The emulator must stay debuggable.",
        "reference": "docs/2026-07-16-vdp-recon.md R1 (the lockup cell). "
                     "VDP timing-skeleton push, slice 3.",
        "action": "EXPECT-MISMATCH: hardware hangs; we produce a debuggable deterministic "
                  "result. Do not flag.",
    },
    {
        "id": "vdp-interrupt-inline-position",
        "scenario": "The exact in-line (sub-scanline) mclk at which HINT/VINT pending is set",
        "blastem_says": "Sets the pending flag at the precise pinned H position within the "
                        "line (H=$02 for VINT, H=$A6/$86 for HINT).",
        "oracle_next_pins": "Events are delivered at 68k instruction boundaries (the ratified "
                            "sync-on-demand model), so HINT/VINT pending can be set up to one "
                            "instruction late (~1,050 mclk worst case, the DIV/RESET outliers). "
                            "The in-line interrupt position is TIMING, not state; delivery ORDER "
                            "stays deterministic (BTreeMap deadlines), so state evolution is "
                            "unaffected.",
        "reference": "docs/2026-07-16-vdp-recon.md R6/R7; "
                     "docs/plans/2026-07-16-vdp-timing-skeleton.md (Risks). "
                     "VDP timing-skeleton push, slice 4.",
        "action": "EXPECT-MISMATCH on cycle-exact interrupt timing; architectural state agrees.",
    },
    {
        "id": "vdp-hv-read-v-midline-increment",
        "scenario": "An HV-counter read ($C00008) in the back portion of a scanline: the V byte",
        "blastem_says": "The readable V counter increments MID-line, at H 0x84->0x85 (H32) / "
                        "0xA4->0xA5 (H40) — recon R2's pinned anchor — so a read after that "
                        "point on line N returns V = N+1.",
        "oracle_next_pins": "V is derived from the line number (mclk / 3420) — a line-boundary "
                            "increment. Reads after the mid-line anchor return V = N, one less "
                            "than hardware for that window. The H byte is mclk-exact and agrees. "
                            "Sub-line V position is TIMING per R2's classification (the jump "
                            "VALUES are behavioral and match; the in-line increment position is "
                            "not); a game that stores such a read makes this visible as a "
                            "one-off RAM/register delta — attribute here, do not flag.",
        "reference": "docs/2026-07-16-vdp-recon.md R2 (the V-increment anchor + the "
                     "timing classification). VDP timing-skeleton push, slice 2 "
                     "(worker deviation 5; ledgered by the reviewer).",
        "action": "EXPECT-MISMATCH on V reads landing between the mid-line anchor and the "
                  "line end; H reads and all other state agree.",
    },
]


def is_known_difference(scenario_id):
    return any(e["id"] == scenario_id for e in KNOWN_DIFFERENCES)


if __name__ == '__main__':
    for e in KNOWN_DIFFERENCES:
        print(f"[{e['id']}] {e['scenario']}")
        print(f"    BlastEm:     {e['blastem_says']}")
        print(f"    oracle-next: {e['oracle_next_pins']}")
        print(f"    reference:   {e['reference']}")
        print(f"    action:      {e['action']}")
