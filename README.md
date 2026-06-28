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
   the four non-negotiables, the staged path, honest effort/risk, the relationship to Oracle.
2. **`docs/foundations.md`** — the settled language (**Rust**) + core architecture + the
   MCP/bus integration + the validation ladder + the ordered build steps.
3. **`docs/decisions/`** + **`docs/plans/`** — the resolved cycle-granularity call, and one
   data-grounded plan per instruction-family push (the running build record).
4. **`docs/research-digest.md`** — the evidence base (emulator landscape, the from-scratch
   synthesis, license-tiered reuse).

## Status (2026-06-28) — Phase 0, deep into the 68000 core

The foundation is built and the **68000 CPU core is most of the way through its grind**;
the rest of the machine (VDP, Z80, audio, MCP wiring) is not started yet.

**Done & gate-green:**
- **Core skeleton** — `Scheduler` (the sole master clock + one seeded RNG + an event heap),
  `System` (owns RAM/VRAM/CRAM/VSRAM/VDP-regs + the scheduler; `Clone` + bincode
  `snapshot`/`restore`), a typed `Bus` + `BusEvent` stream via a split-borrow `SystemBus`,
  and an FNV-1a `state_hash` **byte-compatible with Oracle's `ControlSocket.cpp`**.
- **Determinism gate** (the gating CI job) + property tests: `run_frames(N) ==
  N×run_frames(1)`, and snapshot/restore == identical hash.
- **Cycle-granularity call resolved** (`docs/decisions/2026-06-24-cycle-granularity.md`):
  the single-definition hybrid — each opcode is one resumable micro-op sequence with a
  run-to-completion fast path and a step-one-micro-op quiesce; default quiesce granularity
  = bus access.
- **68000 micro-op core** — the framework (`m68000::{microop, ea, decode, bus68k,
  exception}`) is proven. The arithmetic, logic, shift/rotate, bit, **multiply/divide**,
  compare/move, flow-control, and exception instruction families are implemented and
  validated against the pinned **SingleStepTests/680x0** suite: **752,523 covered test
  cases**, each run through **both drivers** (run-to-completion *and* cycle-stepped),
  checked on registers/SR/RAM/prefetch/cycles **and** the per-cycle bus-transaction stream,
  with snapshot/restore exercised at every bus boundary.

**Not yet:**
- The real `Cpu68000` isn't wired into `System` yet (still a `StubCpu` placeholder) — that
  integration is the next inflection after the remaining 68000 families.
- VDP, Z80, audio, and the Oracle MCP/bus wiring are unstarted.

## Build & test

```sh
# Build
cargo build

# Fetch the pinned SingleStepTests vectors (gitignored; pinned to commit e0d5ece, sha256-verified).
# Needed for the 68000 SST integration tests; the runner skips cleanly if they are absent.
tools/fetch-tests.sh

# The determinism gate (the most-guarded job) + property tests
cargo test -p oracle-core --test determinism_gate --test proptests

# Lint + format (CI runs these with -D warnings)
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check

# Full gate — includes the SingleStepTests sweep through both drivers (~500s; be patient, it is not hung)
cargo test --workspace
```

## What's next

1. **Finish the 68000 instruction set** — the load/store/misc cluster (MOVEM, LEA/PEA,
   LINK/UNLK/EXG/NOP), the privileged moves, ABCD/SBCD/NBCD, ADDX/SUBX, MOVEP; the remaining
   exceptions (illegal/line-A/line-F, trace) and async-interrupt delivery. Same proven
   cadence each push: data-grounded recon → plan → gated build (impl agent → adversarial
   verifier per commit) → self-verified full gate.
2. **Integration pivot** — retire `StubCpu`, wire `Cpu68000` into `System` (reset + a memory
   map + graceful illegal-instruction handling). This is the step that lets the core execute
   a real ROM.
3. **Z80 + VDP** — a tick-stepped Z80, then a scanline-first VDP (planes, scroll, sprites
   with dual per-line limits, priority, H/V interrupts, DMA) toward the **Phase-1 MVP**:
   boots and renders the Sonic-4 hack, fully introspectable. The VDP timing model is the long
   pole and the #1 schedule risk (see `CHARTER.md`).

## Key references

- **Architecture proof (study, do NOT fork — GPL-3):** jgenesis.
- **Differential-test oracles:** BlastEm (accuracy), Exodus (VDP).
- **Black-box component:** ymfm (BSD) for FM audio. Nuked-OPN2 (LGPL) isolated, optional.
- **The Oracle repo** (`../oracle/`) is the reference op surface + bus protocol, the
  source of the MIT 68000/Z80 cores to bootstrap, and the differential-test harness.
