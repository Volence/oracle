# VDP push 2 plan: the timing skeleton

**Status: PLANNED 2026-07-16.** The first VDP implementation push (design brief §6.2,
`docs/2026-07-01-vdp-design.md` as amended today): h/v counter FSM, status bits, HINT/VINT +
scheduler events **including the IPL-deassert path** (the standing set_ipl docket item),
control/data ports, VRAM/CRAM/VSRAM access, and the `export_state` VDP region going live.
This alone lets the 68000 run ROM loops that poll status — the integration pivot's natural
companion. Recon is complete: every behavioral fact below is pinned in
`docs/2026-07-16-vdp-recon.md` (cited as R1–R12); no new recon is needed to build this.

**Scope guard.** In: the `Vdp` struct, timing counters, ports, interrupts, memories, the
introspection-facing state. Out (later pushes, per the brief's build order): rendering
(pushes 3–5), DMA + coarse FIFO budgeting (push 6 — the FIFO here is a data-model
placeholder that drains immediately, documented as such), the Z80 /INT line (no Z80 core),
interlace, the debug register. The CPU core `m68000/*` is **frozen** — everything lands via
the existing seams (`MegaDriveBus`, the `Bus68k` wait channel, the scheduler, `set_ipl`).

## Ground rules (unchanged, verifier-enforced)

- SST threshold exactly `ran >= 1_000_058`; harness untouched (`FlatBus` has no VDP — the
  CPU core cannot tell).
- Oracle `state_hash` FNV layout **frozen** — its region sizes (VRAM `0x10000`, CRAM `0x80`,
  VSRAM `0x50`, 24 regs) are exactly what `Vdp` owns; the hash must be byte-identical across
  the refactor (slice 1 proves it).
- `export_state` v1: filling region 5 is the **designed content change — NO version bump**
  (`docs/export-state-v1.md`); the golden-hash regeneration is **its own attributable
  commit** (slice 5).
- Determinism gate + proptests + golden green at every slice; every commit fmt-clean;
  clippy `-D warnings`; conventional commits, no co-author trailer.
- Clean-room: behavior enters only from the recon doc's pinned facts (permitted sources +
  the recorded BlastEm experiment) — never emulator source.

## Design

### The `Vdp` struct (new module `crates/oracle-core/src/vdp.rs`)

Plain owned data, `Clone` + bincode + `PartialEq`, a field of `System` (foundations rules:
no floats, no HashMap, no threads):

```rust
pub struct Vdp {
    // The four Oracle-hashed regions, moved from System's loose fields (sizes frozen):
    vram: Vec<u8>,          // 0x10000
    cram: Vec<u8>,          // 0x80 (stored in Oracle's byte layout)
    vsram: Vec<u8>,         // 0x50
    regs: [u8; 24],
    // Control-port state (R1):
    code: u8,               // live CD5..CD0
    addr: u16,              // live A15..A0 (the auto-incrementing address IS this register)
    pending: bool,          // first/second-write toggle
    read_buffer: u16,       // the pre-cache read buffer (R3), modeled as data
    fifo: [FifoEntry; 4],   // data-model placeholder: serialized, drains immediately this push
    fifo_len: u8,
    // Interrupt state (R12 + R7):
    vint_pending: bool,
    hint_pending: bool,
    hint_counter: u8,
    // Status latches owned here; vblank/hblank/PAL derive from mclk at read time:
    sprite_overflow: bool,  // consumed by push 4
    sprite_collision: bool, // consumed by push 4
    odd_frame: bool,
}
```

Attribution/render output stays **derived, not state** (design §1) — nothing render-related
serializes.

### Timing FSM: pure functions of mclk (granularity C)

No incremental counter stepping: NTSC V28 geometry is fixed (audit policy 4 — NTSC
hardcoded), so `line(mclk) = (mclk % 896_040) / 3420` and the in-line position is
`mclk % 3420`. The **readable** HV counter applies the pinned jump tables (R2):

- H: H32 `0x00–0x93` → `0xE9–0xFF`; H40 `0x00–0xB6` → `0xE4–0xFF` (the 8-bit read is the
  top 8 of the 9-bit counter — map `mclk % 3420` across 342/422 positions).
- V: `0x00–0xEA` → `0xE5–0xFF`; vblank status sets at the `0xDF→0xE0` transition; hblank
  set/clear at the pinned H anchors (R2). These anchors are the *only* place the H↔mclk
  phase is fixed — one private helper owns the mapping so the anchors are pinned in one spot.
- M3 (reg 0 bit 1): latch the HV value at the moment M3 is set; reads return the latch
  while set (interim model — the real trigger is the HL pin, which nothing asserts; noted
  in the recon doc's open remainder).

### Scheduler wiring (existing `EventKind`, no new plumbing)

A self-rescheduling **`Scanline`** event at every line start (3420-mclk cadence) does the
per-line housekeeping in `System::deliver_event` → `Vdp::on_line_start(line)`:

- HINT counter (R7): lines 225–261 reload from reg 10; lines 0..=224 decrement; on
  underflow reload and schedule **`HInt`** at `line_start + H=$A6/$86 offset` — the `HInt`
  delivery sets `hint_pending`.
- Line 224's start schedules **`VInt`** at the `H=$02` offset — its delivery sets
  `vint_pending` (+ `odd_frame` toggle). `FrameEnd` stays housekeeping.

After any event delivery *and* after every CPU step, System re-derives the IPL latch:
`cpu.set_ipl(vdp.ipl())` (below). Events are delivered at instruction boundaries (ratified
sync-on-demand); worst-case delivery lag is one instruction (~1,050 mclk for the DIV/RESET
outliers — the integration-pivot budget note). Delivery *order* is deterministic (BTreeMap
deadlines), so state evolution is deterministic regardless of lag.

### Interrupts + the IPL-deassert path (R12 — closes the docket item)

```rust
impl Vdp {
    /// Combinational IPL from the two pending latches AND their enable bits (R12).
    pub fn ipl(&self) -> u8 {
        if self.vint_pending && self.regs[1] & 0x20 != 0 { 6 }
        else if self.hint_pending && self.regs[0] & 0x10 != 0 { 4 }
        else { 0 }
    }
    /// The 68k interrupt-acknowledge: clear exactly the acknowledged level's latch (R12).
    pub fn acknowledge(&mut self, level: u8) { ... }
}
```

- **`MegaDriveBus` observes the IACK**: `IntAck` already drives `bus.read16(addr, fc=7)`
  (`microop.rs:2796` — zero CPU changes). In `MegaDriveBus::read16`, `fc == 7` → decode the
  level from the address (`0xFFFFFFF1 | (level << 1)`), call `vdp.acknowledge(level)`,
  return open bus (the CPU discards it — autovector). `FlatBus` ignores fc=7 reads as
  today (SST bit-identical).
- After the step returns, `System` recomputes `cpu.set_ipl(vdp.ipl())` — the acknowledged
  latch is gone, so **a delivered VInt no longer re-fires after RTE**. Enable-bit writes
  mid-step are also picked up by the same recompute (the Counting-Cafe re-assert case works
  because a *pending* latch + re-enabled bit re-raises IPL at the next boundary — the 68k's
  one-instruction latency falls out of instruction-boundary sampling for free).
- Pending latches are cleared by **nothing else** (not status reads, not enable clears, not
  frame boundaries, not display on/off) — R12, hardware-pinned.

### Ports (R1) — the VDP-stub rows of `MegaDriveBus` replaced wholesale

`MegaDriveBus` gains `vdp: &mut Vdp` and `now_mclk: u64` (split-borrowed from `System` like
every other field; `System::step_cpu`/`mega_bus` pass them). `$C00000–$C0000F`:

- **Control write ($C00004/6)**: `pending` clear + top bits `10` → register write
  (`regs[n & 0x1F]`, n ≥ 24 ignored, mode-5 set assumed per audit policy; CD5 gated by the
  DMA-enable bit per R1); `pending` clear otherwise → word 1: `code[1:0]`+`addr[13:0]`
  apply **immediately** (no latch), `pending = true`; `pending` set → word 2:
  `code[5:2]`+`addr[15:14]`, `pending = false`. (DMA-triggering CD5 codes are push-6; until
  then a CD5 command latches state and does nothing — documented placeholder.)
- **Control read (status)**: builds the live status word — PAL=0, DMA=0 (push 6),
  FIFO empty=1/full=0 (placeholder drain), `vint_pending` as bit 7's F readback
  (conservative no-side-effect pin, R12 remainder), sprite overflow/collision latches,
  odd-frame, vblank/hblank from mclk — **and clears `pending`** (the experiment pin,
  instrument-sourced).
- **Data write ($C00000/2)**: clears `pending`; routes by `code` to VRAM/CRAM/VSRAM with
  the address register semantics + autoincrement (reg 15) after each access. CRAM/VSRAM
  writes mask to their 9/11-bit significant layouts exactly as Oracle's byte layout stores
  them (the state_hash currency defines the stored form). VRAM odd-address byte-swap:
  verify against the Kabuto notes at implementation time and pin with a unit test (flagged
  in recon as an implementation-time check, not an open item).
- **Data read**: clears `pending`; returns via the read-buffer model (pre-cache refill —
  immediate this push, slot-timed in push 6); a read with a *write* code set is the pinned
  lockup cell (R1) → deterministic modeled outcome: return open bus, set a serialized
  `latched_fault` debug flag, never hang the host. Documented divergence-ledger entry
  (hardware hangs; we must stay debuggable).
- **HV counter read ($C00008)**: the R2 jump-table value; does **not** touch `pending`
  (experiment pin).
- The `VDP_STATUS` stub constant and its bus.rs rows are deleted (the "replaced wholesale"
  note in bus.rs comes due).

Writes apply immediately through `&mut Vdp` — with one bus master there is nothing to
defer; the deferred-write seam stays reserved for later masters (decision recorded, matches
the pivot's "no other master is live" note).

### Wait-cycle channel

Untouched this push: `FlatBus` and `MegaDriveBus` keep returning 0 extra cycles (FIFO never
fills under immediate drain). The channel is *the* seam push 6 fills with FIFO-full stalls
and DMA bus-halt windows (R3/R4). Stated here so nobody wires ad-hoc stalls early.

## Slicing (gated commits, one per slice, full gate each)

1. **`Vdp` struct extraction (hash-neutral refactor).** Move `vram/cram/vsram/vdp_regs`
   from `System` fields into `System.vdp`; `state_hash()`/`export_state()` read through.
   **Every existing hash is byte-identical** (golden `0x19A0538130972951` unchanged, since
   region 5 still exports zeros this slice — the refactor is provably invisible).
   `feat(vdp): the Vdp struct — move the Oracle-hashed regions out of System`.
2. **Timing FSM + HV port + status vblank/hblank.** The mclk→h/v pure functions, the R2
   jump tables + anchors, M3 latch, status word assembly (placeholder FIFO bits), HV read
   wired into `MegaDriveBus`. Table-driven tests from the pinned progressions (both widths,
   the 0xEA→0xE5 V jump, the four hblank/vblank anchor transitions).
   `feat(vdp): h/v counters, HV port, status timing bits (recon R2)`.
3. **Control/data ports + memories.** The live code/addr registers, the full toggle rule
   set (the four experiment cells as unit tests), register writes, VRAM/CRAM/VSRAM
   read/write + autoincrement + the lockup-cell policy; VDP-stub rows deleted. Tests:
   toggle matrix, address/code splits across word 1/2, autoinc widths, CRAM/VSRAM masking,
   readback round-trips, snapshot round-trip of mid-command state (pending toggle
   serialized).
   `feat(vdp): control/data ports — live address/code registers + memories (recon R1)`.
4. **Interrupts + IPL deassert.** `hint_counter` machinery + Scanline/HInt/VInt scheduling,
   the two pending latches, `ipl()`, `acknowledge()` via the fc=7 IACK hook, the
   post-step/post-event `set_ipl` recompute. Tests: the R7 schedule table (reg10=N → lines
   N, 2N+1, …; reg10=0 → every line 0..=224; line-224 HINT; vblank reloads), **the docket
   test: a delivered VInt is taken once and does NOT re-fire after RTE** (test-ROM ISR
   counts entries across 2 frames), enable-drop/re-assert (Counting-Cafe shape),
   both-pending L6→L4 cascade, latch-survives-status-read, snapshot round-trips of pending
   state. `feat(vdp): HINT/VINT pending latches + IACK-driven IPL deassert (recon R7/R12)`.
5. **`export_state` region 5 goes live + golden regeneration.** Emit
   vram/cram/vsram/regs (in the region-5 layout: VRAM `0x10000` + CRAM `0x80` + VSRAM
   `0x50` + regs 24) at unchanged offsets/sizes — **no version bump** (the designed content
   change); regenerate the golden hash in `export_state_v1.rs` in this same commit and
   nothing else. `test(oracle-core): export_state VDP region goes live (v1 content change,
   golden regenerated)`.

Slices 2–3 commute; 4 needs 2 (line timing) + 3 (enable-bit register writes); 5 last.
Estimated new-vocab risk is lowest-of-any-push so far: no CPU vocabulary, no recipes — the
frozen core is untouched by construction (only `bus.rs`, `system.rs`, `scheduler.rs`
consumers, the new `vdp.rs`).

## Introspection ops (design §4) — what this push owes the API

The timing skeleton predates rendering, so of the §4 surface only the state-shaped
primitives land here: `tile_pixels(index)` (pure VRAM decode) and `cram_decoded()` (CRAM →
RGB at the fixed ramp) as `Vdp` methods with unit tests — the wire protocol wrapping stays
with the Oracle-parity op work (out of scope, noted so the API doesn't accrete as "a later
layer", per the brief's closing rule). `render_line_report`/`pixel_attribution` land with
their pipeline stages (pushes 3–5).

## Anti-cheating / invariants

- SST: 112 tests / `ran >= 1_000_058`, re-run per slice; `m68000/*` diff is empty across
  the whole push (verifier greps it).
- The Oracle `state_hash` value is asserted unchanged across slice 1 (the refactor commit
  runs the oracle_differential + golden suites before/after).
- The golden export hash changes **only** in slice 5, which contains **only** the emission
  change + the regenerated constant (attributability).
- Behavioral facts trace to R1–R12 pins; the two instrument-sourced pins (status-read
  toggle clear; F-bit readback no-side-effect) are flagged in code comments pointing at the
  recon doc.
- New serialized fields all round-trip snapshots (proptest + per-slice unit tests).

## Risks

- **The System-field refactor touching the two frozen currencies** (state_hash,
  export_state). Mitigation: slice 1 is refactor-only and both goldens must hold
  bit-exactly; any change there fails loudly.
- **Event-delivery lag vs. pinned H positions**: HInt/VInt pending set up to one
  instruction late. Accepted by the ratified sync-on-demand model; the divergence ledger
  gets one entry (in-line interrupt position is timing, not state — the xfail-manifest
  policy covers differential noise).
- **The lockup cell** (data read with write code): any deviation from "hardware hangs" is
  a deliberate divergence — must be ledger-documented, not silent.
- **Scanline-event flood** (262/frame): BTreeMap churn is measurable but tiny;
  `microop_perf` is the standing instrument if it ever shows up (policy-7 amendment's T1
  trigger covers the escalation path).

## Decisions surfaced (not defaulted)

1. **R8 model choice** (made under delegated authority, recorded in the design amendment,
   needs owner ratification eventually — not blocking this push since rendering is push 3):
   pin Eke's deterministic Model-2 rule (H40 = AND of the last two VSRAM entries; H32 = 0)
   and ledger the cross-revision variance.
2. **Instrument-sourced pin** (status read clears the control-port toggle): pinned from the
   BlastEm experiment because permitted docs are silent — same standing as the STOP×trace
   pin (owner precedent: flagged, revisit if a better oracle contradicts).
3. **Status-read F-bit readback**: modeled with no side effect on the pending latch
   (hardware-supported for the latch; the readback *value* nuance is a recon remainder) —
   conservative, revisit via the nightly differential once it hashes VDP state.
4. **Immediate port-write application** (no deferred-write queue while the 68k is the only
   master) — recorded above; the seam survives for the Z80/DMA era.
