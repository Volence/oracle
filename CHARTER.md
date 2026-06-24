# Charter — a best-in-class, agent-first Genesis / Mega Drive debugging emulator

> **`oracle-next` is the next-generation Oracle *engine*** — a from-scratch Rust rewrite of
> the emulator core. It is not a separate product: when it's finished it *becomes* Oracle
> (the current Exodus-based engine retires; the MCP / bus protocol / harness carry over
> unchanged). "oracle-next" is the dev name; the shipped name is just **Oracle**.
> (Not to be confused with `megaforge`/`empyrean` — that's the bus connector, a different thing.)
> Status: **side project**, kicked off 2026-06-24. Exploration → MVP.
> **Current Oracle (Exodus) remains the daily driver** for the Sonic-4 hack and keeps
> getting upgrades as needed; oracle-next does not replace it until it earns the role
> (and it doubles as a differential-test oracle for the new engine).

## Why this exists

Oracle (Exodus + an MCP server exposing ~50 debug ops) works well, but a 2026-06-24
foundation review found that **no off-the-shelf Genesis emulator is best-in-class on
all four axes we care about**: deep debug introspection, hardware accuracy, clean
modern codebase, and speed/headless-parallel. Building a fresh, agent-first core is
the only path where all four are **designed in** rather than bolted on.

## The decision

Build a **fresh, agent-first emulator core — the smart bootstrapped way** (not a pure
greenfield rewrite, not a fork of Ares). Chosen over the alternatives we evaluated:

- **Build on Ares** (~2–4 weeks to Oracle-parity, accurate *today*, ISC, very active —
  all proven by spike) — rejected as the *foundation* because its **libco
  cooperative-threading** puts live chip state on C stacks, which fights the
  cheap-snapshot / break-and-inspect-anywhere that is this project's entire point.
  Ares stays a **differential-test oracle** and a reference for its clean core/host
  separation.
- **Stay on Exodus / run a hybrid** — fine for Oracle today, but not best-in-class on
  modern-code / headless / speed.

The deciding insight: **for an agent-first *debugger*, the architecture *is* the
product.** Cheap total snapshot/hash/rewind and break-at-any-cycle come for free from a
cycle-stepped, fully-serializable core, and fight every existing core's architecture.

## Non-negotiables (architectural intent, day one)

1. **Determinism by construction** — one time source + seeded power-on RAM, no
   wall-clock / syscalls in the hot loop. `run_frames` + `state_hash` bit-exact.
2. **Introspection as first-class** — every cycle a valid break/snapshot point; plus
   *render-decode* introspection (why is this pixel this color, which sprites dropped
   on line N, diff CRAM frame A vs B). This is the unique agent-first value.
3. **Headless + parallel** — the core does zero I/O; N instances across threads safe by
   construction.
4. **Clean, modern, instrumentable** — one `System` struct owning all state; the bus as
   a typed protocol; instrumentation as an event stream, not special cases.

Accuracy is the fifth goal, but it is an **asymptote, not a day-one milestone** (see
Staged path). The charter's launch target is **MVP-debuggable**, NOT "passes
VDPFIFOTesting."

## Approach (bootstrapped, not greenfield)

- **Language:** **Rust — decided** (see `docs/foundations.md`; earned against Zig/C++/C,
  with jgenesis as proof the architecture ships). Cycle-stepped explicit-state-machine
  chips; one `System` struct owns all state; a central scheduler owns the sole clock +
  RNG seed; chips generic over `&mut impl Bus` (split-borrow, no `Rc`/`RefCell`/`unsafe`
  on the hot path); all state bincode-serializable.
- **CPUs:** bootstrap from the **MIT-licensed Exodus 68000 / Z80 cores** (already in the
  Oracle repo, already Linux-ported) — re-architected for tick-stepping. Gate on
  SingleStepTests + ZEXALL first: the lowest-risk, mechanically-verifiable subsystem.
- **VDP:** **scanline-first** (handles ~99% of Sonic-hack content); defer the
  cycle-exact FIFO model behind a stable introspection API.
- **Audio:** **black-box `ymfm`** (BSD) for FM, SN76489 PSG from public docs. Synthesis
  off by default in headless runs.
- **Interface:** reuse Oracle's **MCP op surface + bus protocol** — don't re-derive the
  interface we already know works. The new core becomes another backend behind the
  same contract.
- **Differential testing from day one:** Oracle's deterministic `state_hash` vs.
  BlastEm / GPGX / Exodus / Ares. This is the lever that can compress the historically
  multi-year accuracy tail (prior solo authors lacked 3–4 accurate diff-oracles + a
  deterministic harness).

## Staged path

- **Phase 0 — Architecture + cores.** Lock the invariants. Tick-stepped Z80 (~1.5k LOC)
  + 68000 (bootstrapped from Exodus MIT, re-architected for bus-order). **Gate on
  SingleStepTests + ZEXALL before moving on.**
- **Phase 1 — MVP debuggable + deterministic.** Scanline VDP with always-coherent
  VRAM/CRAM/VSRAM/register state; sprite priority + dual per-line sprite limits +
  per-line H-scroll + window plane; object/sprite decoders. Wire the Oracle MCP. Prove
  bit-exact `state_hash`. `ymfm` audio. **Target: boots + correctly renders the Sonic-4
  hack, fully introspectable.** No FIFOTesting required.
- **Phase 2 — Usable daily.** Audio register-tap → VGM + channel-state decode; PSG;
  coarse FIFO/DMA-stall accounting; **render-decode introspection** (the differentiator);
  stand up differential testing; pass Nemesis sprite + 240p tests.
- **Phase 3 — Accuracy hardening (ongoing asymptote).** Upgrade scanline → dot-accurate
  *behind the unchanged introspection API*; tighten FIFO / VRAM-slot / DMA timing; add
  Nuked-OPN2 as an optional cycle-accurate FM backend; TAS-replay regression. **Never
  blocks daily use.**

## Honest effort + risk

- **MVP-debuggable:** ~1–3 months full-time / ~4–6 months at side-project pace.
- **Daily-usable:** ~6–12 months full-time.
- **Mature accuracy:** 2–3+ years (asymptote). **The VDP timing model is the long pole
  and the #1 schedule risk.**
- **Top risks:** the open-ended VDP accuracy tail; scope-creep on accuracy (the charter
  must hold the line at MVP, not VDPFIFOTesting); sustained side-project bandwidth;
  license discipline (ymfm BSD spine, Nuked LGPL *isolated*, never fold GPL /
  non-commercial into a permissive core); determinism leaks (any hidden global,
  wall-clock read, or unseeded RNG silently breaks `state_hash`).

## Relationship to Oracle

- Oracle stays the **daily driver** for the Sonic-4 hack and keeps getting upgrades.
- Oracle is also this project's **reference** (proven op surface + bus protocol), its
  **differential-test oracle**, and the source of the **MIT CPU cores**.
- This project is **reversible and low-risk**: if it stalls, Oracle loses nothing.

## Open questions

- **Cycle granularity (the one big unresolved call):** jgenesis's 68k is
  instruction-stepped, but oracle-next wants "every cycle a valid 68k break/snapshot point." A
  truly cycle-stepped 68000 is more code + slower; the hybrid (instruction fast-path +
  on-demand FSM-quiesce) adds a two-path correctness surface. **Resolve empirically —
  prototype one opcode both ways in Phase 0.**
- The split-borrow `SystemBus` shape and the one deferred-write seam must be designed up
  front (they fix the `System` layout); the canonical `export_state` diff-currency byte
  layout must be frozen early.

The language + architecture are settled — see `docs/foundations.md`. Evidence base for
the whole charter is in `docs/research-digest.md`.
