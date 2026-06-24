# oracle-next

The **next-generation Oracle engine** — a from-scratch, **agent-first** Sega Genesis /
Mega Drive debugging emulator core in **Rust**. Not a separate product: when finished it
*becomes* Oracle (Exodus engine retires; the MCP / bus protocol / harness carry over
unchanged). "oracle-next" is the dev name; shipped, it's just **Oracle**.

Side project — the **current Oracle (Exodus) stays the daily driver** for the Sonic-4 hack
until oracle-next earns the role. (Don't confuse this with `megaforge`/`empyrean` — that's
the bus connector between tools, a separate thing.)

> **New session? Start here.** Everything needed to continue lives in these files + the
> project memory — this does not depend on the originating conversation.

## Read first (in order)

1. **`CHARTER.md`** — vision, the decision (fresh agent-first core, not Ares/not a fork),
   the four non-negotiables, the bootstrapped approach, the staged path, honest
   effort/risk, and the relationship to Oracle.
2. **`docs/foundations.md`** — the settled language (**Rust**) + core architecture + the
   MCP/bus integration + the validation ladder + the one open call (**cycle granularity**)
   + the ordered first build steps.
3. **`docs/research-digest.md`** — the evidence base (emulator landscape, Exodus
   separability, BlastEm + Ares spikes, the from-scratch synthesis, license-tiered reuse).

## Status (2026-06-24)

Research + foundations **decided**. Language = Rust (earned against Zig/C++/C; jgenesis is
the architectural proof). **Ready to start Phase 0 (build).**

## Immediate next step — Phase 0 (test-first)

1. Cargo workspace + **`oracle-core`** skeleton: Scheduler (master-clock min-heap + one
   seeded RNG), the `System` struct owning RAM/VRAM/CRAM/VSRAM, a `Bus` trait emitting a
   typed `BusEvent` via a split-borrow `SystemBus`; derive `Clone` + bincode + an FNV-1a
   `state_hash` **byte-compatible with Oracle's `ControlSocket.cpp`** (vram/cram/vsram/
   regs/combined).
2. **Determinism gate before any real chip** — port `determinism_gate.py` + proptests
   (`run_frames(N) == N×run_frames(1)`, `snapshot/restore == identical hash`) as the first
   CI job.
3. 68000 instruction-stepped, generic over `Bus`, gated on pinned SingleStepTests/680x0 —
   **prototype one opcode both instruction-stepped and FSM-quiesced** to settle cycle
   granularity empirically.

(Then: Z80 + VDP cycle-stepped → `oracle-bus` so `oracle_mcp.py` drives oracle-next unchanged →
differential harness vs BlastEm/Exodus. Full ordering in `docs/foundations.md`.)

## Key references

- **Architecture proof (study, do NOT fork — GPL-3):** jgenesis.
- **Differential-test oracles:** BlastEm (accuracy), Exodus (VDP).
- **Black-box component:** ymfm (BSD) for FM audio. Nuked-OPN2 (LGPL) isolated, optional.
- **The Oracle repo** (`../oracle/`) is the reference op surface + bus protocol, the
  source of the MIT 68000/Z80 cores to bootstrap, and the differential-test harness.
