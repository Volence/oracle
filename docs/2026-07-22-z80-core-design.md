# Z80 CPU core — design recon (execution model, interleave determinism, ZEX gate, first slice)

**Status: RECON + DESIGN, 2026-07-22. Docs only — no code this slice.** This is the Z80-core design recon the
sound-stack sequencing recon (`docs/2026-07-22-sound-stack-recon.md`) named as the next gated step. It takes
that recon's verdict as settled — **Z80 CPU core first, held-in-reset skeleton as the first landable slice,
ZEXDOC/ZEXALL as the Oracle-independent gate** — and designs the core itself: how instructions execute under
the CHARTER's serializable-state constraint, how the 68000 and Z80 interleave **deterministically** (the
load-bearing question S14 flagged as the #1 risk), the exact `Z80` struct, the ZEX harness, the Z80-side bus
map, the named scope boundaries, and the refined first-slice spec.

**Permitted sources only** (audit policy 3, identical to the VDP/IO/BUSREQ/FM/sequencing recons): the **Zilog
Z80 CPU User Manual** (UM008, opcode/flag/interrupt/reset semantics), the **ZEXDOC/ZEXALL** exerciser and its
distributed documentation, **Plutiedev** (Z80 usage, banking, YM2612/PSG ports), and **SpritesMind** hardware
threads. **No emulator source was opened** — not BlastEm, not Ares/GPGX/jgenesis, not the C++ Oracle, not any
`z80.c`/`z80.rs`. The 68000 core in `crates/oracle-core/src/m68000/` is used as an in-repo **design
precedent** (how it holds mid-instruction state serializably, how it fits `run_until`), not as Z80 source.

Grounding read: `CHARTER.md`, `crates/oracle-core/src/system.rs`, `scheduler.rs`, `bus.rs`,
`m68000/microop.rs` + `registers.rs`, `docs/export-state-v1.md`, `docs/foundations.md`, the two Z80/FM bus
recons, and the sequencing recon.

Items are numbered **ZC1–ZC14**, grouped by the seven deliverables. Each pins the call, the evidence class,
the confidence, and the open remainder with its pin-vs-defer disposition.

---

## 1. Execution model / decode strategy (ZC1–ZC3)

### ZC1 — Recommendation: instruction-atomic decode-execute with a fully-serializable struct, **not** the 68000 micro-op recipe framework

**PINNED (design call — argued, not defaulted to symmetry).** The 68000 core is written as one resumable
micro-op recipe per opcode (`MicroState`: a fixed `[MicroOp; 40]` cursor + `step`/`scratch`/flags, all bincode
`Encode`/`Decode`), driven two ways over one shared interpreter — a run-to-completion fast path
(`recipe.run_to_completion`, what `System::step_cpu` actually calls) and a step-one-micro-op quiesce
(`step_micro_op`). It carries that machinery for two reasons that **do not transfer to the Z80**:

1. **Cycle-exact bus ordering under SST.** The 68000 is gated on SingleStepTests, which checks the *per-cycle
   bus transaction stream* (prefetch order, RMW timing, exception stacking order). That forces a micro-op
   decomposition so each bus access is an addressable boundary. **The Z80's gate is ZEXDOC/ZEXALL, which checks
   *architectural results at instruction boundaries* via CRCs over the register file — not a cycle-exact bus
   trace.** Nothing in the Z80 gate requires sub-instruction bus-access boundaries.
2. **Mid-instruction snapshot for the debugger.** The 68000 is the CPU the agent breaks on; "every cycle a
   valid break point" (CHARTER non-negotiable 2) is a debugger feature for the *main* CPU.

The Z80 is the **sound-driver host**, a much simpler ISA (no supervisor mode, no prefetch queue, ~1.5k LOC per
`foundations.md`), and its instructions are short (4–23 T-states ≈ a handful of mclk). The recommendation is
therefore a **straightforward decode-execute**: read the opcode (consuming prefix bytes), dispatch to a handler
that reads its operands, computes, writes back, updates flags, and books the T-state cost — all as ordinary
Rust that **returns at the instruction boundary**, leaving every byte of live state in the `Z80` struct. This is
exactly the shape of the 68000's `run_to_completion` fast path, minus the micro-op cursor that the Z80 does not
need.

**Confidence**: high. **Classification**: architecture. **Open remainder**: whether sub-instruction Z80 breaks
are ever wanted (ZC3).

### ZC2 — The instruction-boundary contract (how this satisfies the CHARTER without a micro-op cursor)

**PINNED.** The CHARTER's binding constraint is *serializability with no live chip state on host/call stacks*
(the exact reason Ares's libco was rejected). The contract:

> **Between Z80 instructions, the entire Z80 is captured by the `Z80` struct + the shared `z80_ram`/68k-bus
> buffers. A Z80 instruction executes atomically inside one `step()` call — it never yields, never spawns a
> coroutine, and holds no state on the Rust call stack across a snapshot boundary, because a snapshot is only
> ever taken *between* `step()` calls (at an instruction boundary), never inside one.**

This is the identical contract the 68000 already honors on its fast path: `System::run_frames` /
`export_state` are explicitly *instruction-boundary only* (`system.rs`: "run_frames leaves the CPU quiesced at
an instruction boundary, so this never captures mid-instruction state"). The Z80 rides the same guarantee. A
mid-`step()` snapshot is structurally impossible because `step()` is synchronous, non-yielding, and runs to the
next boundary before returning — the same way an ordinary function holds locals on the stack only for its own
duration. No `Rc`/`RefCell`/`unsafe`/coroutine; all persistent state is `#[derive(bincode::Encode, Decode)]`
struct fields.

**Confidence**: high. **Classification**: architecture (CHARTER compliance). **Open remainder**: none.

### ZC3 — Sub-instruction breakpoints: deferred, with a named upgrade path that mirrors `MicroState`

**PINNED (scope).** CHARTER non-negotiable 2 says "every cycle a valid break/snapshot point." For the Z80 the
recommended model makes **every instruction boundary** a valid break/snapshot point, not every T-state. This is
a deliberate, honest narrowing, justified because:

- **No consumer needs sub-instruction Z80 breaks.** The debugger breaks on the 68000; the Z80 is the sound
  host. The VGM register-tap (Phase RT) observes *completed* port writes, which are instruction-boundary
  events. The gate (ZEX) is instruction-boundary. Nothing downstream can observe a half-executed Z80 opcode.
- **The upgrade path is named and cheap if ever needed.** If a future need arises (e.g. cycle-exact
  Z80↔68k bus contention, S14's deferred timing item), the Z80 gains a resumable cursor **exactly mirroring
  the 68000's `MicroState`**: a small serializable `inflight: Option<Z80MicroState>` holding the decoded
  operation + a `step` index, with the same "single definition, two drivers" discipline. Because all Z80 state
  already lives in the struct, adding this is additive, not a rewrite.

Recommending instruction-atomic now (with the micro-op cursor as a named future refinement) is the same call
the cycle-granularity decision (`docs/decisions/2026-06-24-cycle-granularity.md`) reached for the 68000 fast
path: pay for sub-instruction granularity only where a consumer observes it.

**Confidence**: high. **Classification**: scope. **Open remainder**: sub-instruction timing (deferred, ZC12).

### ZC3b — Decoder organization: prefix state, not a flat 256-way table

**PINNED (design).** The Z80 opcode space is a base table plus five prefix groups. The decoder is organized as
a small **prefix-accumulating front end** feeding per-table dispatch, all inside one non-yielding `decode_execute`:

| Prefix | Role | Decode note |
|---|---|---|
| (none) | base 256 opcodes | direct dispatch |
| `CB` | rotate/shift/bit/set/res on r/(HL) | second byte selects op |
| `ED` | extended: block LDIR/CPIR/INIR/OTIR, 16-bit `ADC/SBC HL`, `IN/OUT (C)`, `LD A,I/R`, `RETI/RETN`, `IM n` | second byte selects op; undefined `ED xx` = documented NOP |
| `DD` | HL→**IX** override for the *next* opcode | sets an index-override state, then re-enters decode |
| `FD` | HL→**IY** override | same, IY |
| `DDCB`/`FDCB` | indexed bit/shift ops | **fetch order is `DD`/`FD`, `CB`, `d` (displacement), then the opcode byte** — the displacement precedes the final opcode, a decode quirk that must be honored |

The front end reads bytes one at a time, tracking two pieces of decode state: an **index-register override**
(`None`/`IX`/`IY`, set by `DD`/`FD`, consumed by the following opcode; `(HL)`→`(IX+d)`/`(IY+d)`, `H`/`L`→
`IXH`/`IXL` for the undocumented halves) and whether a `CB`/`ED` table applies. `DD`/`FD` chains collapse
(each `DD`/`FD` restarts the override; a run of them is legal and each is one M1 fetch). This organization keeps
the base/CB/ED handlers written **once** and reused under the index override, rather than duplicating tables per
prefix. The DDCB/FDCB path is the one place the fetch order is irregular (displacement before opcode) and is
called out here so the implementation reads `d` at the right point.

**Confidence**: high (the prefix structure is fully specified in the Z80 UM008 opcode chapter). **Classification**:
architecture. **Open remainder**: the *undocumented* DD/FD/DDCB variants (IXH/IXL ops, `SLL`, the DDCB
"copy-to-register" side effects) are ZEXALL scope (ZC11), not the first executing version.

---

## 2. 68k↔Z80 interleave determinism — the load-bearing design (ZC4–ZC7)

### ZC4 — The model: 68000-anchored **run-ahead-to-deadline** with an absolute Z80 frontier

**PINNED (the central call).** The scheduler owns the sole clock in mclk; the 68000 runs at mclk/7, the Z80 at
mclk/15 (`scheduler.rs`, `foundations.md`). Today `System::run_until` is a 68000-driven loop: pop due events →
step the 68000 one instruction → `advance(cycles × 7)` → re-derive IPL, until `now >= deadline`. The Z80 fits
this **without changing the anchor**: the 68000 instruction stream defines the timeline; **the Z80 chases the
68000's clock**.

Add one serialized scalar to `System`: `z80_frontier_mclk: u64` — the absolute mclk up to which the Z80 has
been simulated (its next-instruction boundary). The augmented loop iteration is a **fixed total order**:

```
while now < deadline:
    deliver due scheduler events            (unchanged)
    step the 68000 one instruction          (unchanged; advances `now` by cycles×7)
    catch the Z80 up to the new `now`:       (NEW)
        while z80_gated_on()  and  z80_frontier_mclk < now:
            t = z80.step(&mut Z80Bus)        // whole Z80 instruction, returns T-states
            z80_frontier_mclk += t * 15      // the ONE Z80-cycle→mclk site (×15)
        if not z80_gated_on():
            z80_frontier_mclk = now          // held in reset/bus-granted: track time, run nothing
    re-derive IPL                            (unchanged)
```

`z80_gated_on()` = `z80_running && !z80_busreq` (ZC6). The Z80 runs whole instructions until its frontier
reaches or passes the 68000's current `now`, then stops — the instruction that crosses `now` finishes just past
it, and the overshoot is carried in `z80_frontier_mclk` into the next iteration. This is the **identical
absolute-deadline + bounded-overshoot pattern** the 68000 frame loop already uses and tests
(`frame_boundary_mclk`, `run_frames_n_equals_n_times_one`, `overshoot_never_accumulates`), reused verbatim for
the Z80's frontier. There is no separate Z80 event loop, no min-heap of two CPUs, no run-ahead beyond `now`.

**Confidence**: high. **Classification**: architecture (determinism). **Open remainder**: exact sub-cycle
contention ordering (deferred timing, ZC12).

### ZC5 — Why this is bit-reproducible and snapshot-safe (the failure modes, named and precluded)

**PINNED.** The determinism gate demands bit-for-bit reproducibility across runs and across snapshot/restore.
The model precludes each known failure mode by construction:

- **Non-deterministic run-ahead.** *Precluded:* the interleave is a **pure function of state** — a fixed order
  (events → 68000 step → Z80 catch-up-to-`now` → IPL) with the Z80 bounded by the 68000's clock, never by
  wall-clock, thread scheduling, or a free-running "run ahead as far as convenient." Two runs from the same
  seed execute the identical sequence of 68000 and Z80 instructions in the identical order. (Contrast the
  thread-per-device / libco idioms `foundations.md` calls out as *proven* non-deterministic.)
- **Floating cycle debt across snapshots.** *Precluded:* the Z80's position is the **absolute**
  `z80_frontier_mclk`, a serialized `System` field (bincode `Encode`/`Decode`, like `frame_boundary_mclk` and
  `z80_busreq`). A snapshot captures it exactly; a restore resumes the chase from the exact same frontier. No
  relative "cycles owed" counter lives outside the snapshot.
- **Mid-instruction state on the host stack at a snapshot.** *Precluded:* snapshots are taken between
  `run_until` iterations / between `step()` calls (ZC2); the Z80 is at an instruction boundary with all state
  in the struct. `z80.step()` is non-yielding.
- **Backlog explosion on reset-release.** *Precluded:* while gated off (held in reset or bus-granted to the
  68000), `z80_frontier_mclk` is advanced to `now` each iteration (it runs *nothing* but does not fall behind).
  When a game later releases reset, the Z80 resumes from the current `now` with **zero** accumulated backlog —
  it does not suddenly replay millions of skipped instructions. This is both correct (a reset Z80 executes
  nothing and has no notion of "catching up") and essential to keeping run time bounded.

**Confidence**: high. **Classification**: architecture (determinism). **Open remainder**: none.

### ZC6 — Gating on BUSREQ/RESET mid-frame

**PINNED.** The Z80 may step only when it is both **released from reset** and **not** frozen by a 68000 bus
grant. Two latches, both already partly present in `bus.rs`:

- `z80_busreq` — **already a real latch** (DR-1a, `bus.rs`): `true` = 68000 has requested/holds the Z80 bus.
- `z80_reset` — **promote from stub to a real latch** this slice. Today `$A11200` reads a constant `0` and
  drops writes (`bus.rs:307`, `store_byte` fall-through). Per BUSREQ recon Z1/Z4: `$A11200` bit0 `1` = release
  reset (Z80 runs), `0` = assert reset (held). **Power-on = reset asserted** — real hardware holds the Z80 in
  reset until the 68000 releases it (Plutiedev "Using the Z80"). To avoid the reset-polarity foot-gun, store
  the latch as a positive `z80_running: bool` (power-on `false`; `$A11200` bit0 write sets it), or name it
  `z80_reset_released`. `z80_gated_on() = z80_running && !z80_busreq`.

`z80_busreq`/`z80_running` ride the bincode snapshot (determinism) but are **not** in `export_state`
(bus-arbitration scalars, like `last_bus_word`; export-state-v1 §"Z80 BUSREQ/RESET arbitration state"). The
gate is evaluated every `run_until` iteration, so a mid-frame `stopZ80`/`startZ80` or reset toggle takes effect
at the next 68000 instruction boundary — instruction-granular, consistent with the ratified sync-on-demand
model.

**Confidence**: high. **Classification**: behavioral (the gate value) + architecture. **Open remainder**: the
Z4 "reset ⇒ auto-grant-bus" refinement stays deferred (no acceptance consumer; BUSREQ recon Z4).

### ZC7 — Deterministic resolution of shared-state accesses

**PINNED.** Two memory regions are touched by both CPUs; the fixed interleave order (ZC4) makes every access
unambiguous:

- **68000 → Z80 RAM (`$A00000–$A0FFFF`) and the FM/PSG ports.** Games access Z80 RAM only while holding the bus
  (`z80_busreq` asserted / Z80 in reset), i.e. exactly when the Z80 is **gated off and executing nothing**
  (ZC6). So there is no concurrent Z80 write to race with: the 68000's access reads/writes the shared
  `z80_ram` buffer at its own instruction boundary, deterministically. A pathological 68000 access to Z80 RAM
  *without* holding the bus is a hardware bus conflict; we model it as taking effect on the shared buffer at
  the 68000 boundary (the Z80 catch-up runs afterward and sees the result) — deterministic regardless.
- **Z80 → 68k bank window (`$8000–$FFFF`) reaching ROM/RAM/VDP.** The Z80 catch-up runs *after* the 68000's
  instruction for this iteration has completed and `now` has advanced, so the Z80 sees the post-step state of
  RAM/VDP at `now`. Because the order is fixed (68000 instruction, then Z80 chase), the Z80's view is a pure
  function of state — never "whichever thread got there first." Cross-master writes through the bank window use
  the **same split-borrow fields** the `MegaDriveBus` borrows (rom/ram/vdp/io), so there is no aliasing seam.

The one thing deliberately *not* modeled is **sub-cycle bus contention timing** (the ~3.3 Z80 / ~11 68000-cycle
average interleave delay) — a timing property with no acceptance consumer, deferred in BUSREQ recon Z3 and
sound-stack S14, and named again in ZC12. Its absence cannot change any *value* the fixed order produces, only
the (unobserved) cycle at which a contended access would have landed.

**Confidence**: high (value determinism). **Classification**: behavioral value = pinned; sub-cycle latency =
timing, deferred. **Open remainder**: contention timing (ZC12).

---

## 3. State representation (ZC8–ZC9)

### ZC8 — The `Z80` struct

**PINNED.** All fields bincode `Encode`/`Decode` + `Clone`/`PartialEq`, mirroring `Registers`/`Cpu68000`.
Register pairs are stored as `u16` (with byte accessors) so AF/BC/DE/HL and their shadows round-trip exactly.

```
struct Z80 {
    // Main register file
    af: u16, bc: u16, de: u16, hl: u16,     // A|F, B|C, D|E, H|L
    // Alternate (shadow) file — swapped by EX AF,AF' and EXX
    af2: u16, bc2: u16, de2: u16, hl2: u16,
    // Index + control
    ix: u16, iy: u16, sp: u16, pc: u16,
    i: u8,                                   // interrupt vector base (IM 2)
    r: u8,                                   // memory-refresh counter (bit 7 preserved on inc)
    // Interrupt / halt state
    iff1: bool, iff2: bool,                  // interrupt enable flip-flops
    im: u8,                                  // interrupt mode 0/1/2
    halted: bool,                            // HALT executed; waiting for INT/reset
    int_pending: bool,                       // /INT asserted (VDP vblank), not yet taken
    // Undocumented-flag support — RESERVED, zero until ZEXALL scope (ZC11)
    wz: u16,                                 // MEMPTR (drives YF/XF of BIT n,(HL) etc.)
    q: u8,                                   // last-flag-write tracker (SCF/CCF undoc flags)
}
```

The `wz`/`q` fields are present in the struct from the start (so the snapshot layout never changes) but are
**inert** in the first executing version, which targets ZEXDOC (documented flags only, ZC11). Flag bit layout
in `F`: `S Z 5 H 3 P/V N C` (bits 7..0); bits 5/3 (`YF`/`XF`) are the undocumented pair, written by ZEXALL-scope
ops only. `z80_frontier_mclk` and the `z80_running`/`z80_busreq` latches live in `System` (ZC4/ZC6), **not** in
`Z80` — the struct is pure architectural + interrupt state, so the ZEX harness (ZC10) can drive a bare `Z80`
with no `System`.

**Confidence**: high (Z80 UM008 register model). **Classification**: architecture. **Open remainder**: whether
`q` is one byte or a richer last-flags snapshot — settled at ZEXALL time (ZC11).

### ZC9 — Two serializations: the bincode snapshot vs. the 0x40 export-golden region — both checked

**PINNED.** There are two distinct currencies, and the `Z80` struct must satisfy both:

1. **Bincode snapshot** (`System::snapshot`) — the whole `Z80` struct rides it, unconstrained in size, for
   determinism/rewind. Trivially satisfied by the derives.
2. **Export-golden region 4** (`export_state`, offset `0x12050`, **size `0x40` = 64 bytes**, currently all
   zero). This is a *fixed hand-laid byte layout* I design, distinct from bincode. Enumerate the architectural
   register file in a fixed little-endian order:

   | Bytes | Field |
   |---|---|
   | 8 | AF, BC, DE, HL |
   | 8 | AF', BC', DE', HL' |
   | 4 | IX, IY |
   | 4 | SP, PC |
   | 2 | I, R |
   | 1 | IFF1·IFF2·IM packed |
   | 1 | HALT flag |
   | 2 | WZ (optional; ZEXALL scope) |
   | **30** | **total** |

   **30 bytes ≤ 64** — the reserved `0x40` has >2× margin, confirming the memory note and `system.rs`'s
   "sized (0x40) with 2× margin over the Z80's ~0x20-byte architectural register set." Filling it is a
   **content change at unchanged size → no version bump** (export-state-v1 rule; the VDP-region-5 precedent).

**Reset-state bytes — the only thing that can move the export golden (S9).** The Z80 reset (UM008 §"RESET")
*defines* PC=0, I=0, R=0, IFF1=IFF2=0, IM=0; SP and the main/index registers are **architecturally undefined**
(emulators vary; common power-on conventions are SP=`$FFFF`, AF=`$FFFF`). Because every committed fixture holds
the Z80 in reset (ZC13), region 4 is filled *from the reset-state register file*, so its bytes are whatever the
reset model pins:

- **All-zero reset model** → region 4 stays all-zero → the export golden **does not move** at go-live.
- **Hardware-faithful model** (SP=AF=`$FFFF`, else `$FFFF`/`0`) → region 4 is non-zero → the golden **moves
  once** — a legitimate one-time regen in the attributable Z-live slice, **not** a version bump.

**Recommendation:** pin the **defined** bits to their reset values (PC=I=R=0, IFF=0, IM=0, HALT=0) and, for the
undefined registers, **choose all-zero** — it keeps the export golden frozen through go-live (cleaner
attributability, one less golden churn) and is a legitimate reset model (undefined = we may pick). If the
overseer prefers hardware-faithful `$FFFF` fills, that is equally correct and costs exactly one documented
regen. Either way the choice is **pinned deliberately and not allowed to drift** (S9). The first slice keeps
region 4 zeroed regardless (ZC13), so this only binds at Z-live.

**Confidence**: high (fit + bump rule); the reset-fill value is a deliberate judgment call surfaced for the
overseer. **Classification**: architecture + currency. **Open remainder**: final reset-fill value (decided at
Z-live).

---

## 4. ZEXDOC/ZEXALL validation harness (ZC10–ZC11)

> **UPDATE 2026-07-22 — gate pivoted ZEXDOC → SingleStepTests/z80 (overseer decision, supersedes ZC10/ZC11's
> harness choice; the rest of this doc stands).** When provisioning the ZEXDOC `.com` binary it turned out to be
> absent from disk with no fetch tooling, while the **SingleStepTests z80 corpus is fetchable via the exact
> pinned+checksummed mechanism the 68000 SST currency already uses** (`tools/fetch-z80-tests.sh`, mirroring
> `tools/fetch-tests.sh`; data gitignored under `vendor/ProcessorTests/z80/v1`; committed `bc1dedb`). SST-z80 is
> the better-fitting Oracle-independent gate: same harness pattern as `singlestep_m68000.rs`, per-opcode
> granularity (1000 vectors/opcode, `FILES` list grows incrementally exactly like the 68000 set), and it carries
> `wz`/`q` so it covers **documented AND undocumented** flags in one corpus — subsuming ZEXDOC's *and* ZEXALL's
> roles without a two-stage `.com` ladder. The instruction-atomic model (ZC1/ZC3) still holds: the runner gates
> on `final` register+RAM state and **ignores the per-cycle `cycles` bus trace** our atomic core does not
> reproduce (the same reason the Z80 needs no `MicroState`). ZC10's structural-isolation argument (a Z80-only
> harness that never instantiates `System`, so it cannot move any frozen currency) carries over unchanged — the
> SST-z80 runner is isolated identically. The ZEXDOC→ZEXALL *ladder* (ZC11) is replaced by a single corpus with
> a documented-vs-all-flags assertion toggle. ZEXDOC/ZEXALL remain a possible later cross-check if ever sourced.

### ZC10 — The harness: a Z80-only flat 64 KiB bus + a BDOS trap, the analog of SST's `FlatBus`

**PINNED.** ZEXDOC/ZEXALL are CP/M `.COM` exercisers: they load at `$0100` in a flat 64 KiB Z80 address space,
drive every instruction against thousands of operand vectors, and CRC the resulting register state against a
built-in expected CRC, printing `OK`/`ERROR` per test group via CP/M BDOS calls. The harness needs **no
`System`, no scheduler, no Genesis map** — exactly as SST drives `Cpu68000::step` over a hand-built `FlatBus`
with no `System` (sequencing recon S7). Minimal pieces:

1. **A flat 64 KiB RAM `Z80TestBus`** implementing the Z80's `read8`/`write8`/`in`/`out` — plain array, no
   banking, no ports (the exercisers do not use real I/O).
2. **Load** the `.COM` image at `$0100`; set `PC=$0100`, `SP=$FFFF` (or the CP/M stack the image expects).
3. **A BDOS stub via PC-trap** (no real CP/M): before each `step()`, if `PC == $0005` (BDOS entry), service
   function `C=2` (print char in `E`) and `C=9` (print `$`-terminated string at `DE`) into a captured output
   buffer, then `RET`. If `PC == $0000` (CP/M warm boot), the run is done.
4. **Run** `step()` in a loop until the `$0000` exit; assert the captured output contains the all-`OK` banner
   and **no** `ERROR`/CRC-mismatch line. This is the gate.

The harness is a `#[cfg(test)]` runner over a vendored (or user-supplied, skip-if-absent per D5) `zexdoc.com`/
`zexall.com`, structurally isolated from `System` — so it can never move any of the five frozen currencies,
identically to how SST is isolated (S7). It is the mechanically-verifiable, Oracle-independent first gate.

**Confidence**: high (the ZEX BDOS-trap harness is the standard, documented method). **Classification**:
validation. **Open remainder**: vendoring vs. skip-if-absent for the `.com` files — a build-policy detail, D5
already covers the skip-if-absent path.

### ZC11 — ZEXDOC gates the first executing version; ZEXALL is the deferred accuracy follow-up

**PINNED (gate ladder, tied to scope ZC3b/ZC14).** ZEXDOC checks **documented** flag behavior; ZEXALL checks
**all** flags including the undocumented `YF`/`XF` (bits 5/3) and the undocumented opcodes (`IXH`/`IXL` halves,
`SLL`, the DDCB register-copy side effects). The Z80's F register and a handful of ops (`BIT n,(HL)`, block
ops, `SCF`/`CCF`) set the undocumented bits from `WZ`/`Q`, which the first version leaves inert (ZC8).

- **First executing version → ZEXDOC is the gate.** Documented flags + the documented opcode set. This is what
  a real SMPS/sound driver needs (drivers are overwhelmingly unlikely to depend on undocumented behavior), so
  ZEXDOC unblocks driver validation.
- **ZEXALL → deferred follow-up.** Undocumented opcodes + `YF`/`XF` propagation + `WZ`/`Q`, landed once the
  documented core is green. This is the harder accuracy gate and matches `foundations.md`'s "ZEXALL/ZEXDOC
  full-ROM gates" and CHARTER Phase 0's "gate on SingleStepTests + ZEXALL."

**Confidence**: high. **Classification**: validation + scope. **Open remainder**: none — the split is the
scope boundary (ZC14).

---

## 5. Bus/memory map from the Z80's side (ZC12)

### ZC12 — The `Z80Bus` split-borrow adapter

**PINNED.** A second CPU-facing adapter alongside `MegaDriveBus`, borrowing the same `System` fields
(split-borrow, no `Rc`/`RefCell`), presenting the Z80's own address space (Z80 UM008 has no memory map — this is
the *Genesis* Z80 map, pinned from Plutiedev "Using the Z80"/"Z80 banking" + the PSG/YM2612 pages):

| Z80 address | Target | Behavior |
|---|---|---|
| `$0000–$1FFF` | Z80 RAM (8 KiB) | the shared `z80_ram` buffer (same bytes the 68000 sees at `$A00000`) |
| `$2000–$3FFF` | Z80 RAM mirror | 8 KiB mirrored (mask `& 0x1FFF`) |
| `$4000` / `$4001` | YM2612 bank 0 | `$4000` = address latch, `$4001` = data — **write → `BusEvent` (Phase RT VGM stream)**; read = FM status (not-busy stub, reuses the 68k-side model) |
| `$4002` / `$4003` | YM2612 bank 1 | address / data (bank-1 registers) — same tap |
| `$6000` | bank register | **serial load**: each write shifts bit 0 of the byte into the 9-bit bank latch (LSB-first); after 9 writes the bank is set |
| `$7F11` | PSG (SN76489) | **write → `BusEvent` (Phase RT VGM stream)**; the SN76489 is write-only |
| `$7F00–$7F1F` | VDP mirror | Z80-side VDP port window (rarely used; can stall the Z80 on hardware — model minimally, route to the same `Vdp`) |
| `$8000–$FFFF` | 68k bank window | 32 KiB into the 68k space: `bus68k_addr = (bank << 15) | (z80_addr & 0x7FFF)`; reaches ROM/RAM/VDP via the shared fields |

**FM/PSG writes become the `BusEvent` stream the sequencing recon wants (S13/Tier 1).** Every `$4000–$4003`
and `$7F11` write on the `Z80Bus` emits a `BusEvent { op: Write, addr, size, value }` to the attached
`BusEventSink` — the *same* instrumentation channel `run_frames_with_sink` already threads. The RT-tap phase's
FM/PSG register files and VGM logger are pure `BusEventSink` consumers; no new mechanism. The 68k-side
`$A04000–$A04003` FM writes (Gunstar boot) decode to the **same** register file — one chip, two windows (S13).

**bank window vs. Z80-RAM aliasing:** the FM ports at `$4000–$4003` must be decoded **before** any RAM mirror
(same ordering discipline the 68k side already uses so `$A04000` is not aliased to `z80_ram[0..3]`, `bus.rs`
F1/DR-1b). The `$8000+` window is a 68k access, so it routes through the existing 68k read/write helpers, not a
Z80-RAM store.

**Confidence**: high (Plutiedev map). **Classification**: behavioral (the map) + architecture (the adapter).
**Open remainder**: exact `$7F00–$7F1F` VDP-mirror stall semantics and the precise 9-bit bank shift edge cases
— minor, pinnable from Plutiedev/SpritesMind when a driver exercises them; not on the first-slice path.

---

## 6. Scope boundaries — named, not silent (ZC13-scope / ZC14)

### ZC14 — In vs. out for the first executing version, each pinned to a reference

**IN (first executing version, ZEXDOC gate):**
- The full **documented** opcode set: base, `CB`, `ED`, `DD`/`FD` (documented IX/IY forms), `DDCB`/`FDCB`
  (documented indexed bit/shift). Pin: Z80 UM008 opcode chapter.
- **Documented flags** `S Z H P/V N C`. Pin: UM008 §"Flags"; ZEXDOC.
- **Interrupt modes IM 0/1/2, IFF1/IFF2, HALT**, and the **`/INT` line** (VDP raises Z80 vblank IRQ once per
  frame; the driver's timing depends on it). The `/INT` acceptance mechanism (mode-2 vector fetch via `I`,
  mode-1 `$0038`, `HALT` wake) is *in* the core; it is *wired to the VDP* when the core goes live. Pin: UM008
  §"Interrupt Response" + VDP recon (Z80 vblank IRQ).
- **Deterministic 68k↔Z80 interleave** (ZC4) — not deferrable; it is this recon's core deliverable.

**OUT (deferred, each pinned):**
- **Undocumented opcodes** (`IXH`/`IXL`, `SLL`, undefined `ED`, DDCB register-copy variants) → ZEXALL
  follow-up. Pin: UM008 undocumented behavior + ZEXALL. (Real drivers unlikely to use them — S14.)
- **Undocumented flags `YF`/`XF` (bits 5/3) + `WZ`/MEMPTR + `Q`** → ZEXALL follow-up; the fields exist inert in
  the struct now (ZC8) so no later layout change. Pin: the "undocumented Z80" community notes + ZEXALL.
- **Exact `R`-register increment edge cases** (per-M1 increment with bit 7 preserved is *in*; the precise count
  across interrupts/block ops is refined later) — **not ZEX-gated** (ZEX does not CRC `R`). Pin: UM008 §"R".
- **Sub-instruction Z80↔68000 bus-contention timing / wait states** (~3.3 Z80 / ~11 68000 cycles) → deferred
  timing, no acceptance consumer. Pin: SpritesMind bus-timing threads + BUSREQ recon Z3 / S14.
- **NMI** — the Genesis has no standard Z80 NMI source; unused. Pin: Plutiedev (Z80 `/NMI` unconnected).
- **FM/PSG *synthesis*** (envelopes, DAC, LFSR) → Phase SY tail, off by default (CHARTER). The Z80Bus only
  *taps* the writes (ZC12). Pin: sound-stack S2/S13.

**Confidence**: high. **Classification**: scope. **Open remainder**: none — the deferrals are the boundary.

---

## 7. The first landable slice, concretely (ZC13)

### ZC13 — Slice "Z-skeleton": the Z80 wired + runnable, held in reset, currency-neutral

**PINNED (refines sound-stack S3).** The direct analog of the VDP timing-skeleton push (wire everything, keep
`export_state` emitting zeros until a later attributable go-live).

**What it adds:**
- **New module** `crates/oracle-core/src/z80/` (`mod.rs` = the `Z80` struct ZC8 + decode-execute skeleton;
  `bus.rs` = the `Z80Bus` adapter ZC12). The opcode handlers can be a *stub* this slice (the point is the
  wiring + the reset gate; the full opcode set is the next slice, Z-execute) — or land the documented set here
  and gate it, overseer's call on slice size. Minimum viable skeleton: struct + `Z80Bus` + `step()` signature +
  the reset gate.
- **`System` fields:** `z80: Z80`, `z80_frontier_mclk: u64`, and promote `z80_reset`/`z80_running` to a real
  latch (power-on `false` = reset asserted). All bincode-serialized.
- **`bus.rs`:** promote `$A11200` from the constant-0/drop stub to a real write-catching latch (`z80_running =
  (byte & 1) != 0`); `$A11200` read reports the latch. `$A11100` `z80_busreq` is already real (DR-1a).
- **`run_until`:** the ZC4 catch-up block (Z80 chases `now` when gated on; advances the frontier to `now` when
  gated off). Since nothing releases reset in any fixture, the catch-up loop body runs **zero** times.
- **`export_state`:** region 4 (Z80 regs, `0x40`) **stays all-zero** this slice — do **not** flip live yet.

**Why it is currency-neutral (S8, by construction):** every committed fixture leaves the Z80 in reset
(`z80_running == false`) — none writes `$A11200` bit0 = 1 (the export testrom, the determinism RAM-stir ROM,
the golden_frames `Vdp`-only scenes, oracle_differential's static bytes, and SST's `FlatBus` all boot without
releasing the Z80; the export test already asserts the whole Z80-RAM region is zero). So `z80_gated_on()` is
never true → the Z80 executes **zero** instructions in every gate → `z80_ram` and the Z80 registers are
unchanged, and region 4 stays zero. SST is unaffected *by construction* (no `System`/Z80 in its harness). All
five frozen currencies stay byte-identical — proven empirically by re-running all five, construction-argument
first (the FM/BUSREQ-slice discipline).

**How it is validated out-of-band:** ZEXDOC on the ZC10 flat-bus harness (once opcodes land in Z-execute) +
a manual "release reset via `$A11200`, upload a real driver, run N frames, introspect `z80_ram`/registers"
harness — exactly as the 68000 is validated out-of-band by SST while the frozen currencies never run it there.

**The SECOND slice ("Z-execute"):** the full documented opcode set (ZC3b/ZC14) behind the serializable struct,
gated **ZEXDOC then ZEXALL**. Still currency-neutral in the committed gates (no fixture releases reset). Then
**"Z-live"** flips region 4 live + regenerates the golden in its own attributable commit (with the ZC9
reset-fill value pinned), and **Phase RT** lands the FM+PSG register-tap + VGM stream off the `Z80Bus`
`BusEvent`s (S2/S10 — golden does not even move, zero writes at the testrom capture point).

**Confidence**: high. **Classification**: sequencing. **Open remainder**: the Z-skeleton vs. Z-execute split
point (how much opcode work rides the first commit) is the overseer's slice-size call.

---

## Summary (the overseer's four asks)

1. **Execution model.** **Instruction-atomic decode-execute with a fully-serializable `Z80` struct — not the
   68000's micro-op recipe framework.** The recipe machinery exists to serve SST's cycle-exact bus trace and
   the main-CPU debugger's mid-instruction breaks; the Z80's gate (ZEX) is instruction-boundary and the Z80 is
   the sound host, so neither driver transfers. Instruction-boundary snapshot contract (ZC2) satisfies the
   CHARTER with no host-stack state; sub-instruction breaks are deferred with a named `MicroState`-mirroring
   upgrade path (ZC3). Decoder = a prefix-accumulating front end (CB/ED/DD/FD/DDCB/FDCB) feeding reused
   per-table handlers under an index-register override (ZC3b).

2. **Interleave determinism (load-bearing).** **68000-anchored run-ahead-to-deadline**: the 68000 instruction
   stream defines the timeline; the Z80 chases `now` via an **absolute, serialized `z80_frontier_mclk`** (the
   ×15 site), in a **fixed total order** (events → 68000 step → Z80 catch-up-to-`now` → IPL) — the same
   absolute-deadline + bounded-overshoot pattern the 68000 frame loop already uses. Gated on
   `z80_running && !z80_busreq`; while gated off the frontier tracks `now` so reset-release carries **zero**
   backlog. All four failure modes (non-deterministic run-ahead, floating cycle debt, host-stack mid-instruction
   state, backlog explosion) are precluded by construction (ZC5). Shared state (68000↔Z80 RAM, Z80↔68k window)
   resolves unambiguously because the order is a pure function of state (ZC7); sub-cycle contention timing is
   the one named deferral.

3. **ZEX gate.** A Z80-only flat 64 KiB `Z80TestBus` + a PC-trap BDOS stub (functions 2/9), structurally
   isolated from `System` like SST's `FlatBus` (ZC10). **ZEXDOC gates the first executing version** (documented
   flags/opcodes — what a real driver needs); **ZEXALL is the deferred accuracy follow-up** (undocumented
   opcodes + `YF`/`XF` + `WZ`/`Q`, whose fields exist inert now so no later layout churn) (ZC11).

4. **First slice.** **"Z-skeleton": `Z80` struct + `Z80Bus` + the ZC4 mclk/15 catch-up in `run_until` + a real
   `z80_reset`/`z80_running` latch (power-on = reset asserted), held in reset.** Currency-neutral by
   construction — no committed fixture releases the Z80, so it executes zero instructions and region 4 stays
   zeroed; validate out-of-band (ZEXDOC + a manual driver-run harness). Second slice "Z-execute" (full opcode
   set, ZEXDOC→ZEXALL); then "Z-live" (flip region 4 + one attributable golden regen, ZC9 reset-fill pinned);
   then Phase RT (FM+PSG tap off the `Z80Bus` `BusEvent`s). The `0x40` export region holds the 30-byte
   register file with >2× margin — filling it is a content change, no version bump (ZC9).

## Sources

- **Zilog Z80 CPU User Manual (UM008)** — register model, opcode tables + prefix groups (CB/ED/DD/FD/DDCB/FDCB),
  flag semantics, interrupt modes IM 0/1/2 + IFF1/IFF2 + HALT + `/INT` response, RESET state (PC/I/R/IFF/IM
  defined; SP + main/index undefined).
- **ZEXDOC / ZEXALL** exerciser + its distributed docs — CP/M `.COM` layout (`$0100`), BDOS calls
  (`CALL $0005`, functions 2/9), CRC-per-test model; ZEXDOC = documented flags, ZEXALL = all incl. `YF`/`XF`.
- **Plutiedev** — [Using the Z80](https://plutiedev.com/using-the-z80) (BUSREQ/RESET, reset ordering, memory
  map), Z80 banking (`$6000` serial bank register, `$8000+` 68k window), [YM2612](https://plutiedev.com/ym2612-registers)
  (`$4000–$4003`), [PSG](https://plutiedev.com/psg-chip) (`$7F11`).
- **SpritesMind** — Genesis 68K bus timing / Z80↔68k contention (the deferred sub-cycle timing).
- **oracle-next in-repo (precedent, not Z80 source):** `m68000/microop.rs` (`MicroState` serializable
  mid-instruction cursor, `run_to_completion` fast path, "single definition / two drivers"),
  `m68000/registers.rs` (serializable register file shape), `system.rs` (`run_until` loop, `frame_boundary_mclk`
  absolute-deadline overshoot, reserved `EXPORT_Z80_REGS_PLACEHOLDER = 0x40`), `bus.rs` (`z80_busreq` real
  latch, `$A11200` stub to promote, split-borrow adapter shape, FM-before-RAM decode order), `scheduler.rs`
  (mclk clock, 68k=/7 Z80=/15), `docs/export-state-v1.md` (region-4 reserve + no-bump-on-content-fill rule),
  `docs/foundations.md` + `CHARTER.md` (serializable-no-host-stack non-negotiable, ZEXALL/ZEXDOC gate,
  synthesis off by default), the sound-stack + BUSREQ + FM recons.
```
