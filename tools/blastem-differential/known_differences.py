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
