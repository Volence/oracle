# Foundations decision brief

From the 2026-06-24 foundations research (5 angles + synthesis). This settles the
language and the core architecture; it leaves **one** deliberate open decision (cycle
granularity) to resolve empirically in Phase 0.

## Language: Rust — earned, not assumed

The borrow-checker objection ("aliased mutable state fights Rust") **does not survive
contact with prior art.** [jgenesis](https://github.com/jsgroth/jgenesis) (jsgroth) is a
shipping, cycle-accurate, from-scratch **Rust** Genesis emulator for exactly our chips
(68000/Z80/VDP/PSG/YM2612) that already realizes the intended design: one owning struct
holds all chips + memory; CPUs are generic over a `BusInterface` trait; a transient
`&mut MainBus` is threaded into `execute_instruction(&mut bus)`; **no `Rc`/`RefCell`/
`unsafe` on the hot path**; `bincode` Encode/Decode for snapshots; a debugger-hook trait
for instrumentation. The NES-style `Rc<RefCell<dyn Mapper>>` pain is absent because
Genesis carts are simple (generics/enums, not `dyn`).

**Why not the alternatives:**
- **C++** (Exodus/ares heritage) — disqualified for the greenfield core: its dominant
  idioms (thread-per-device, libco cooperative threading) are *proven* non-deterministic
  and snapshot-hostile (this repo's own gate showed Exodus isn't bit-reproducible).
- **C** (BlastEm) — right to *adopt* (it's the ~50× headless diff-oracle), wrong to
  *write* (no Serde/typed-bus/RAII; UB-prone).
- **Zig** — the strongest challenger (velocity, comptime opcode tables, clean wasm/FFI),
  but disqualified for a multi-year core by pre-1.0 churn and *no* aliased-pointer safety
  on exactly our shared-state pattern.

**jgenesis is a reference, not a fork.** It's GPL-3 and *instruction*-stepped; oracle-next is a
clean agent-first reimplementation (cycle-steppable, render-decode introspection). We
mine its architecture and reimplement clean, keeping oracle-core permissive and our
options open.

## Architecture

A no-I/O **`oracle-core`** crate:
- **Scheduler** owns the *sole* master clock (NTSC mclk ≈ 53.69 MHz; 68k = mclk/7,
  Z80 = mclk/15) as a min-heap of `(deadline_mclk, EventKind)` (VINT/HINT/scanline/DMA/
  sample) plus **one seeded** SplitMix64/PCG seeding power-on RAM/VRAM. Deterministic is
  the only mode.
- **One `System` struct** owns RAM/VRAM/CRAM/VSRAM, both CPUs, the sound chips, and the
  scheduler. Chips are generic over `&mut impl Bus`, borrowed per step via **split-borrow**
  (no `Rc`/`RefCell`/raw pointers). The whole machine is plain owned data deriving
  `Clone` + bincode — O(struct) snapshot, no pointer fixup.
- **Bus = typed protocol** emitting a `BusEvent` per access. Breakpoints, watchpoints,
  decoders, the profiler, and the VGM logger are **event-stream consumers**, not CPU
  special-cases.
- **Stepping:** Z80 + VDP **cycle-stepped** (floooh tick = one-clock FSM — the
  render-decode introspection engine); 68k **instruction-stepped fast path but
  quiescable to any cycle boundary on demand**. BlastEm-style sync-on-demand with a
  catch-up window (default one scanline = 3420 mclk); any debugger touch drains to a
  cycle boundary.
- **Crates:** `oracle-core` (no I/O, `Send`), `oracle-bus` (tokio `UnixListener`
  JSON-RPC, owns N core instances one-per-thread), `oracle-host` (optional wgpu GUI). The
  same `oracle-core` compiles to **wasm32** for deterministic replay.

### Handling shared mutable state (the plan)

Generic-bus-as-trait (jgenesis): the CPU owns no bus; each step it borrows a transient
`&mut SystemBus` via split-borrow, so only one `&mut` is live — monomorphized, zero
dispatch, no interior mutability. Re-entrant cross-chip writes (a 68k write landing in
the VDP mid-access; DMA reading 68k memory) go through **one explicit deferred-write /
command-buffer seam** (jgenesis's `MainBusWrites`): writes push to a pending queue,
`apply_writes()` drains after the access — which *also* makes transitions explicit and
serializable. Hard rules: no `Rc`/`RefCell`/`Cell` on the hot path; no floats in hashed
state (fixed-point audio); deterministic collections (no `HashMap` in state); zero
threads in core.

## MCP integration — the clean win

**`oracle-core` *is* a native Unix-socket bus server; `oracle_mcp.py` stays UNCHANGED.**
The MCP is already a pure bus client (Aether NDJSON JSON-RPC 2.0 / AF_UNIX). A Rust
server (`serde_json` + tokio `UnixListener`) reuses the ~50-op surface 1:1, mirroring
Exodus's response shapes plus structured errors, advertising live `methods[]`. The
process boundary is a **determinism firewall** (control plane runs *between* deterministic
steps) — so we **reject** in-process PyO3/ctypes, which would re-couple what the bus
decoupled and fight headless + N-instances.

**One protocol change first:** `screenshot` currently writes a PNG to disk and returns
`{path}`; change it to return **base64 PNG bytes** so the core is truly zero-I/O and
wasm-portable.

## Validation stack (reuse Oracle's ladder)

1. **Determinism gate (per-commit, the most-guarded job):** port `determinism_gate.py`
   (two fresh instances, `reset{run:false}`, `run_frames(1)+state_hash` loop,
   byte-identical) + proptests `run_frames(N) == N×run_frames(1)` and
   `snapshot/restore == identical hash`.
2. **CPU unit gates:** a runner over **pinned** SingleStepTests (680x0 + z80) asserting
   post regs/SR/RAM **and** the per-cycle bus-transaction stream (oracle-next's edge), with a
   versioned xfail manifest; + ZEXALL/ZEXDOC full-ROM gates.
3. **Bus-contract conformance:** assert each op's schema so `oracle_mcp.py` +
   `determinism_gate.py` drive oracle-next **unchanged**.
4. **Nightly differential** lockstep vs **BlastEm** (accuracy oracle) + **Exodus** (VDP
   oracle), diffing a **canonical `export_state`** byte layout — **not** cross-backend
   hash equality (FNV-1a is within-backend only; equality would report 100% false
   divergence). Bisect to the diverging instruction.
5. **VDP ladder** golden-frames (VDPFIFOTesting / Sprite-Masking / CRAM-dot / 240p) —
   measure Exodus's *real* pass count first.
6. **Golden-frame + TAS-replay** on `s4.bin`; audio hashed as the VGM **register stream**
   (not PCM); Nuked-OPN2 as FM golden.
7. **cargo-fuzz** over `Arbitrary` bus-op streams (no panic / valid state).

## The one open decision: cycle granularity

jgenesis's 68k is **instruction**-stepped; oracle-next wants "every cycle a valid 68k break/
snapshot point." A truly cycle-stepped 68000 is much more code and slower (and may
threaten the parallel N-instance perf budget); the hybrid (instruction fast-path +
on-demand FSM-quiesce) adds a two-path correctness surface that must agree. **Resolve
empirically: prototype one opcode both ways in Phase 0 before committing.**

## First build steps (ordered)

1. `oracle-core` skeleton: Scheduler (master-clock min-heap + one seeded RNG), `System`
   owning RAM/VRAM/CRAM/VSRAM + a `Bus` trait emitting typed `BusEvent` via a split-borrow
   `SystemBus`, proven on a stub chip; derive `Clone` + bincode + an FNV-1a `state_hash`
   **byte-compatible with `ControlSocket.cpp`** (vram/cram/vsram/regs/combined).
2. **Determinism gate before any real chip** (port `determinism_gate.py` + the proptests)
   as the first CI job.
3. 68000 instruction-stepped, generic over `Bus`, gated on pinned SingleStepTests/680x0;
   **prototype one opcode both instruction-stepped and FSM-quiesced** to settle cycle
   granularity.
4. Z80 (floooh cycle-stepped) gated on SingleStepTests/z80 + ZEXALL/ZEXDOC; then the VDP
   as a cycle-stepped FSM producing render-decode views.
5. `oracle-bus` (tokio `UnixListener` JSON-RPC) mirroring the Aether handshake + core op
   profile with Exodus-identical shapes; `screenshot`→base64; run `oracle_mcp.py` +
   `determinism_gate.py` against it **unchanged** as the conformance gate.
6. Differential harness (lockstep vs BlastEm + Exodus, canonical `export_state` diff
   currency) + nightly cargo-fuzz.

## Biggest risks

- **Cycle granularity** — the big unresolved scope/perf call (prototype first).
- **Determinism is a single point of failure** — any `HashMap` order, `f32` audio
  nondeterminism, thread scheduling, or unseeded RAM silently voids `state_hash`. Enforce
  no-floats/deterministic-collections/zero-threads from commit one.
- **Split-borrow + deferred-write seam shape the `System` layout early** — get them wrong
  and you face a painful refactor or slide back to `Rc`/`RefCell` (which also breaks
  `Send`). Design them up front.
- **Cross-backend hash equality is a footgun** — define the canonical `export_state` diff
  currency; freeze its byte layout early.
- **Test-suite realities** — SingleStepTests are MAME/ares-derived (not cycle ground
  truth); VDP test ROMs self-report via framebuffer goldens; vendor them into CI.
