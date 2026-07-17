# VDP push 6 plan: DMA + FIFO + wait states — the last must-have before real games display

> **For agentic workers:** implement this plan task-by-task (superpowers:executing-plans). Each slice is
> one gated commit: TDD, full gate, fmt + clippy `-D`, one commit per slice. Steps use checkbox syntax.

**Goal:** make the VDP FIFO real (4 entries, contents serialized), fill the `Bus68k` wait-cycle seam that has
returned zero since Push B (FIFO-full stall + the 68k→VDP DMA halt window), implement the three DMA modes
(68k→VDP transfer, VRAM fill, VRAM copy) with coarse Phase-2 timing, and land the `frame_report` DMA section —
so a real game's art-load-by-DMA works end to end (proved by upgrading `frame_dump` to load its art via DMA).

**Architecture:** the FIFO becomes a serialized 4-entry metadata ring (data + code/addr copy at enqueue) plus
a coarse drain clock; **VRAM/CRAM/VSRAM mutation stays applied at enqueue** (the scanline-latch approximation
absorbs the difference; a data-port read waits for the FIFO to drain anyway — R3), so the FIFO adds *timing*
and *snoop/fill-source contents*, not a deferred-write pipeline. DMA source reads (68k memory) live in
`MegaDriveBus` — the only `Bus68k` impl the `System` uses — which feeds words to the VDP and returns the coarse
wait through the existing channel. `FlatBus` (the SST harness bus) has no VDP and returns 0 forever, so the SST
corpus stays bit-identical.

**Tech stack:** Rust (`oracle-core`); bincode 2.x snapshots; the existing `MegaDriveBus`/`FlatBus`/`Bus68k`
wait-cycle channel (Push B, `bus.rs`); the `Vdp` struct (`vdp.rs`); the pure renderer (`render.rs`); the
BlastEm GDB-RSP differential harness (`tools/blastem-differential/`, the `vdp_pending` experiment template).

---

## Scope

**In** (design brief `docs/2026-07-01-vdp-design.md` §6 build-order item 6; recon `docs/2026-07-16-vdp-recon.md`
R3/R4/R5 + R1 deferred cells):

- The **real FIFO** (R3): 4 entries each = `{data, code, addr}` captured at enqueue; a coarse drain clock keyed
  to the pinned per-line slot budget; **FIFO-full stalls the 68k through the wait channel**; the pre-cache read
  buffer becomes slot-timed (read-with-nothing-cached stalls); the **CRAM/VSRAM snoop quirk** (undefined read
  bits sourced from the next-available FIFO entry — "4 writes ago").
- The **three DMA modes** (R4): **(a) 68k→VDP transfer** = a total bus-halt window through the wait channel,
  words through the FIFO; **(b) VRAM fill** = 68k keeps running, fill data from the last FIFO entry (top byte
  for VRAM; the "4-writes-ago" next-available entry for CRAM/VSRAM fills — the documented hardware bug),
  mid-fill data-port writes suspend + redirect (interim); **(c) VRAM copy** = FIFO-bypass, byte read + byte
  write per slot, half rate. **DMA-busy status** sets on the control-port setup write (Eke). Fill/copy run off
  the **live** code/address registers; the length/source registers (19–23) **mutate during a transfer**
  (correct, visible in both currencies — the golden fixture drives no VDP DMA so it is unaffected).
- **Coarse Phase-2 timing** (settled policy, brief §2): per-line external-slot counts from the pinned R3 table
  (16/18 active, 167/205 blanked, + refresh); Kabuto's 68k-cost formulas as corroboration. **Slot positions
  within a line stay deferred to Phase 3** — stated in the divergence ledger.
- **The R5 DMA-fill/copy × SAT-cache remainder** (rider): interim model — fill/copy steps hit the write-through
  window compare like any VRAM write. **Confirm or amend** (unit-test evidence; golden-frame evidence permitted
  but not required).
- **R1's two scheduled experiment cells** (rider): **mid-fill control-write semantics** and the **data-port-read
  toggle effect in non-lockup configs** — pin by a BlastEm experiment (probe ROM deposits results in work RAM;
  the RSP reads work RAM — the `vdp_pending` template). Record honestly: hardware itself is **intermittent** on
  three mid-fill cells — an intermittent cell is a documented limitation, not a pin.
- **`frame_report` DMA section** (design §4): transfers performed (source/dest/length/mode) + HINT/VINT/overflow
  rollup already present.
- **The DMA register/command byte formats** (regs 19–23, mode decode, trigger points) — pinned in a recon-lite
  doc addendum (the RR-series precedent: "the standard formats neither the design brief nor the R1–R12 recon
  wrote down").
- **`frame_dump` upgraded to load its art via DMA** — the end-to-end proof this push exists for.

**Out** (later / deferred, per the brief): slot-exact FIFO/DMA timing (Phase 3); VDPFIFOTesting suite (explicit
Phase-3 non-goal — charter line, hold it); the Z80/DMA bus-clash glitch behind the RAM-not-ROM rule (R4(d) — lands
with the Z80, nothing to model here); the M3 lightgun HL nuance; interlace mode 2. The CPU core `m68000/*` is
**frozen** and both frozen currencies stay **byte-identical** (see the invariant below).

---

## The load-bearing invariant: currency neutrality (the Push-4/5 precedent)

The new state (FIFO ring, drain clock, DMA-busy deadline, DMA in-flight counters) is **real hardware state** →
it becomes **serialized `Vdp` fields that round-trip snapshots**. This is safe *because both frozen currencies
read explicit regions, not a bincode of the whole struct*:

- **Oracle `state_hash`** (`state_hash.rs`) hashes exactly `vram / cram / vsram / regs`. The new fields are in
  none → all five fingerprints **byte-identical**.
- **`export_state`** (`system.rs`) serializes `version → m68k regs → work RAM → Z80 RAM → Z80 regs →
  (VRAM+CRAM+VSRAM+regs) → FM → PSG`. The new `Vdp` fields are in no region → the golden
  **`0x22F80ECF29ED3AD4`** holds **byte-identical**, and `export_state_v1`'s length/offset literals (derived
  from region *sizes*, unchanged) hold.
- The bincode **snapshot** of `Vdp` *does* grow — fine: not a frozen currency; every new field derives
  `bincode::Encode/Decode`; a per-field round-trip test + the existing proptest prove snapshot→restore equality.

**Two ways this push could still move a currency — both guarded:**

1. **The FIFO's VRAM mutation.** We keep VRAM/CRAM/VSRAM mutation **applied at enqueue** (immediate), exactly as
   today — the FIFO adds *timing + snoop contents*, not a deferred write. So slice B (fields) leaves
   VRAM/CRAM/VSRAM/regs byte-identical by construction. **Prove it:** the field-adding commit shows
   `export_state_v1`, `oracle_differential`, `determinism_gate`, **and both `golden_frames` scene sets** green
   with the existing constants — the isolated-commit proof (Push-4 precedent). If a diff moves any hash, a field
   leaked — stop.
2. **DMA mutating regs 19–23 / VRAM.** Real; visible in both currencies. Guarded because **the golden fixture
   (`testrom::build()`) and the SST corpus drive no VDP DMA** — so the currencies never see a DMA. The DMA
   slices (E–G) re-prove both goldens + `export_state_v1` green with the existing constants.

The new fields are a **v2 export-currency candidate** (whenever the cross-backend differential wants the FIFO /
DMA state in the frame image) — not this push.

---

## Ground rules (verifier-enforced)

- **`m68000/*` diff = 0 lines** across the whole push. The wait channel is consumed in `exec_one` already
  (Push B); no CPU changes. `git diff <base> -- crates/oracle-core/src/m68000/` must be empty at HEAD.
- **SST threshold exactly `ran >= 1_000_058`; harness untouched; `FlatBus` waits stay 0 forever.** `FlatBus`
  has no VDP and never triggers a DMA, so every SST stream + cycle count is bit-identical. **SST cadence:** since
  `m68000/*` is byte-identical the whole push, run the full SST sweep at **slice C** (the first commit that
  changes a wait return — proves `FlatBus`/SST bit-identical) **and at HEAD**; intermediate slices may skip it
  (state which trees when reporting). A `FlatBus`-returns-0 unit test guards the invariant directly.
- **Oracle `state_hash` layout + the export golden `0x22F80ECF29ED3AD4` byte-identical throughout — no golden
  regen.** `golden_frames.rs` scene hashes may change **only** with documented evidence (never silently); this
  push does **not** add or regen a golden scene (see Decision 5).
- Determinism gate + proptests + `export_state_v1` + `oracle_differential` + both `golden_frames` scene sets
  green at every slice; every commit fmt-clean; `clippy --all-targets -D warnings` (examples included);
  conventional commits, **no `Co-Authored-By` trailer**.
- **Clean-room absolute:** behavior enters only from the pinned recon (`docs/2026-07-16-vdp-recon.md` R1/R3/R4/R5
  + the recon-lite DMA-format addendum from permitted community docs) + the ratified design brief + the BlastEm
  instrument. **Never** emulator source. Never modify `../oracle/`.
- **No floats anywhere** (foundations rule). All slot/word/cycle math is integer.
- **Every new serialized field round-trips snapshots** (bincode + a round-trip test).
- **No dead code:** each new field is *used* in the slice that introduces it (clippy `-D` — the timing-skeleton
  push's FIFO-fields-omitted lesson).

---

## Design

### The FIFO as a coarse timing + contents model (not a deferred-write pipeline)

New serialized `Vdp` fields (all `bincode::Encode/Decode`, in neither frozen currency):

```rust
/// One FIFO slot (recon R3): the data word plus a copy of the command code/address registers as they were
/// when the write was enqueued. The physical slot RETAINS its data after the entry drains (fifo_len drops but
/// the bytes stay) — that stale data is what the CRAM/VSRAM snoop quirk and the fill data-source read.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, bincode::Encode, bincode::Decode)]
struct FifoEntry {
    data: u16,
    code: u8,
    addr: u16,
}

// added to struct Vdp:
    /// The 4-entry write FIFO (recon R3), a physical ring. `fifo_write` is the next slot to fill; the oldest
    /// pending entry is `fifo[(fifo_write - fifo_len) & 3]`; the "next-available" entry (about to be
    /// overwritten = written 4 writes ago) is `fifo[fifo_write]` — the snoop-quirk / CRAM-VSRAM-fill source.
    fifo: [FifoEntry; 4],
    fifo_write: u8,          // 0..=3, next slot to enqueue into
    fifo_len: u8,            // 0..=4 pending (not-yet-drained) entries — the coarse timing abstraction
    /// mclk up to which the FIFO has been drained. Advanced by `fifo_drain` at the pinned per-line slot rate.
    fifo_slot_clock: u64,
    /// mclk before which the DMA-busy status bit (status b1) reads set (recon R4 / Eke: set on the control-port
    /// setup write; a fill/copy runs the 68k in parallel, so a poll sees busy for the coarse transfer window).
    dma_busy_until: u64,
    /// The most recent completed DMA (introspection `frame_report`; recon R4). None until the first transfer.
    last_dma: Option<DmaRecord>,
```

**Why VRAM mutation stays at enqueue (the central simplification — Decision 1).** Applying the write when the
entry drains would build a deferred-write pipeline the currencies would have to chase. Instead we mutate
VRAM/CRAM/VSRAM at enqueue (as today) and let the FIFO carry only *timing* (`fifo_len`/`fifo_slot_clock`) and
*contents* (`fifo[..]` for snoop/fill-source). This is invisible to any 68k program: a data-port **read** waits
for the FIFO to be empty (R3), and rendering **latches at line start** (scanline model), so no observer can
distinguish "written at enqueue" from "written at drain" within Phase-2 granularity. Documented in the
divergence ledger.

**Drain model (coarse, integer, deterministic).** One external slot = one VRAM byte; a FIFO word to VRAM = 2
slots; CRAM/VSRAM = 1 word/slot (R3). Slots/line: `16` (H32 active) / `18` (H40 active) / `167` (H32 blanked) /
`205` (H40 blanked) — display-off or vblank lines are "blanked". So `mclk_per_slot(line) = MCLK_PER_LINE /
slots_per_line(line)` (integer; `MCLK_PER_LINE = 3420`). `fifo_drain(now)` pops entries whose slot-cost has
elapsed against `fifo_slot_clock`, advancing the clock; it consults the line type at `fifo_slot_clock` (coarse —
crossing an active↔blank boundary uses the type at the clock instant, which Phase 3 refines). See slice B.

**FIFO-full stall (the wait channel, slice C).** On a data-port write at `now`: drain up to `now`; if
`fifo_len == 4`, the 5th write stalls until the oldest drains — `wait_mclk = next_drain_mclk - now`; return
`wait_cpu = wait_mclk.div_ceil(MCLK_PER_CPU_CYCLE)` (=7) CPU cycles; then drain that one and enqueue. `MegaDriveBus`
threads this out of `vdp_write_word` → `write16`/`write8`. `FlatBus` never calls the VDP → 0.

### The three DMA modes (recon R4)

DMA source reads (68k memory) can only happen where 68k memory lives — in `MegaDriveBus`. The `Vdp` exposes a
`dma_request()` that `MegaDriveBus` drains after each control/data write and executes:

```rust
/// A pending DMA the 68k has just triggered (recon R4). The VDP arms it on the trigger write; `MegaDriveBus`
/// executes it (it owns the 68k source memory) and returns the coarse wait.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DmaRequest {
    /// (a) 68k→VDP: read `len` words from 68k `source` (byte address), feed each through the FIFO to the
    /// current data-port target + autoinc. Total 68k halt.
    Mem { source: u32, len: u16 },
    /// (b) VRAM fill: `len` writes of the fill byte to the current target, 68k keeps running.
    Fill { len: u16 },
    /// (c) VRAM copy: `len` byte read+write steps within VRAM, FIFO-bypass, 68k keeps running.
    Copy { len: u16 },
}
```

- **Mode decode** (recon-lite RD, slice A): reg 23 (`$17`) top bits — `0x` (bit 7 = 0) → **Mem**; `10` → **Fill**;
  `11` → **Copy**. `len = regs[20]<<8 | regs[19]` (words for Mem/Fill; bytes for Copy). `source =
  ((regs[23]&0x7F)<<16 | regs[22]<<8 | regs[21]) << 1` (Mem only; a 68k **word** address). CD5 (code bit 5) armed
  = DMA; the trigger:
  - **Mem & Copy trigger on the control-port second word** completing with CD5 set (the destination command
    write). Copy needs no 68k source.
  - **Fill triggers on the following data-port write** (the fill value); CD5 stays armed across it.
- **DMA-busy** (status b1) sets on the control-port setup write for all three (Eke). For Mem the 68k is halted the
  whole window, so it never observes busy set (consistent); for Fill/Copy `dma_busy_until = now + coarse_cost` so a
  polling 68k sees it for the transfer window.
- **Coarse cost (Decision 2):** integrate the per-line slot budget across the lines the transfer spans, from the
  current line, using each line's active/blank type — deterministic, integer. Mem/Fill = 1 word per (2 slots
  VRAM / 1 slot CRAM-VSRAM); Copy = half byte rate; fill loses one slot on its first line (the trigger write) — a
  documented coarse detail. Kabuto's `words×2.4+5.6` / `max(…, words×4.7−6)` formulas are recorded as
  corroboration only (no floats in the model — the slot-budget integration is the source of truth).
- **VRAM mutation is applied atomically at execution** for all three (the 68k can't read VRAM mid-DMA — reads
  are prohibited during a transfer and stall on a busy FIFO anyway); the busy window models the elapsed time.
  Mem/Fill route every word/byte through `write_vram_byte` → **SAT write-through fires** (R5 rider confirmed).
  Fill data source: last FIFO entry's top byte (VRAM) / the next-available "4-writes-ago" entry (CRAM/VSRAM bug).
  **regs 19–23 mutate** as the transfer runs (length → 0, source advanced) — faithful, currency-visible.
- **Mid-fill data-port write** (R4(b), interim): if a data-port write lands while `now < dma_busy_until` of a
  fill, the write enqueues, the fill resumes with the new data one location further. Modeled best-effort;
  flagged intermittent per the R1 experiment (slice B') + the divergence ledger.

### `DmaRecord` (introspection, `frame_report`)

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug, bincode::Encode, bincode::Decode)]
pub struct DmaRecord {
    pub mode: DmaMode,   // Mem | Fill | Copy
    pub source: u32,     // 0 for Fill/Copy-with-no-source
    pub dest: u16,       // the data-port target address at trigger
    pub len: u16,
    pub target: Target,  // Vram | Cram | Vsram
}
```

---

## Slices

Order: docs recon-lite → BlastEm experiment (pins the 2 R1 cells that inform later slices) → FIFO fields
(isolated, currency-neutral) → wait channel → snoop/read-buffer → the three DMA modes → `frame_report` →
`frame_dump`. Each slice = one commit, TDD, full gate, fmt + clippy `-D`.

---

### Slice A — recon-lite: DMA register/command byte formats

**Files:** Modify `docs/2026-07-16-vdp-render-recon.md` (append an **RD1–RD5** DMA-format section, mirroring the
RR-series). No code.

Pin from **permitted community docs only** (Plutiedev DMA-transfer page; Sega Genesis Technical Overview §7; Sega
Genesis Software Manual §DMA; Kabuto notes) — cite verbatim, clean-room:

- **RD1** register map: reg 19 `$13` = length low, 20 `$14` = length high; 21 `$15` = source low, 22 `$16` =
  source mid, 23 `$17` = source high + mode.
- **RD2** mode decode: reg 23 bit 7 = 0 → 68k→VDP (source bits = `regs[23]&0x7F`); `10` → VRAM fill; `11` → VRAM
  copy. Length is words (Mem/Fill) / bytes (Copy).
- **RD3** source: `((regs[23]&0x7F)<<16 | regs[22]<<8 | regs[21]) << 1` = a 68k word address (Mem only).
- **RD4** triggers: Mem & Copy on the control-port second word with CD5 set; Fill on the next data-port write.
- **RD5** the RAM-not-ROM rule (R4(d)) — verbatim, "nothing to model beyond the Z80 bus-clash (later)".

- [ ] **Step 1:** Write the RD1–RD5 section with verbatim citations and the interim/deferred flags.
- [ ] **Step 2:** `cargo fmt --check` (no code) and confirm no `.rs` touched: `git diff --name-only` = the doc.
- [ ] **Step 3:** Commit.

```bash
git add docs/2026-07-16-vdp-render-recon.md
git commit -m "docs(recon): pin the VDP DMA register/command byte formats (RD1-RD5)"
```

---

### Slice B' — BlastEm experiment: the two R1 cells (mid-fill control-write; data-port-read toggle)

**Files:** Create `tools/blastem-differential/vdp_dma_fill.asm` (+ assembled `.bin` + a `run_*.py` driver,
mirroring `vdp_pending`); Modify `docs/2026-07-16-vdp-recon.md` (fold results into R1's "Open remainder") +
`tools/blastem-differential/known_differences.py` (any recorded limitation).

Two cells, using the established pattern (probe ROM deposits results in work RAM; the RSP client reads work RAM):

1. **Data-port-read toggle effect in non-lockup configs** (R1 open remainder): arm a *read* command, then a
   data-port read, then observe whether the control toggle is cleared — in configs that do **not** hit the
   pinned lockup cell. High-confidence reproduction (the `vdp_pending` harness pattern).
2. **Mid-fill control-write semantics** (R4 open remainder): start a VRAM fill, issue a control write mid-fill,
   read back the resulting VRAM/address to see the redirect. **The recon records hardware is intermittent on
   three of these cells** — run each cell multiple times; an intermittent cell is recorded as a documented
   limitation, **not** pinned.

- [ ] **Step 1:** Author the probe ROM (`.asm`) for both cells; assemble to `.bin` (record sha256, as `vdp_pending`
      does). Author the `run_vdp_dma.py` RSP driver.
- [ ] **Step 2:** Run under `xvfb-run` + the RSP client. Capture the work-RAM results. Re-run the mid-fill cells
      several times to expose intermittency.
- [ ] **Step 3:** Fold the outcomes into `docs/2026-07-16-vdp-recon.md` (R1 + R4 open remainders → pinned or
      recorded-intermittent) and, for any intermittent/BlastEm-blind cell, add a `known_differences.py` entry.
- [ ] **Step 4:** `cargo fmt --check`; confirm no crate `.rs` changed.
- [ ] **Step 5:** Commit.

```bash
git add tools/blastem-differential/ docs/2026-07-16-vdp-recon.md
git commit -m "test(tools): BlastEm experiment — data-port-read toggle + mid-fill control-write cells"
```

> **If the experiment is infeasible in the timebox** (no xvfb/WM, as the frame-capture spike hit): record the
> negative result in the recon doc, keep the interim models (data-read clears the toggle like a data-write;
> mid-fill redirect off the live registers), and proceed — the interim models are already the implementation
> defaults. Surface this as a deviation in the report. Do **not** block the push on it.

---

### Slice B — the FIFO as serialized state (field-adding, currency-neutral, ISOLATED)

**Files:** Modify `crates/oracle-core/src/vdp.rs`.

Add `FifoEntry` + the FIFO fields + `dma_busy_until` + `last_dma`. Wire enqueue into `data_write`'s non-DMA
path, the drain model, and the DMA-busy status bit — **but keep VRAM mutation at enqueue and return no wait**,
so VRAM/CRAM/VSRAM/regs are byte-identical. Every field is used (no dead code): `fifo`/`fifo_write`/`fifo_len`
by enqueue + `fifo_snoop()`; `fifo_slot_clock` by `fifo_drain`; `dma_busy_until` by `status_word`; `last_dma`
by a new `pub fn last_dma()` reader (exercised by a test).

Key code:

```rust
fn slots_per_line(&self, mclk: u64) -> u64 {
    let blanked = self.vblank(mclk) || (self.regs[1] & 0x40) == 0; // vblank or display-off
    match (self.h40(), blanked) {
        (false, false) => 16,
        (true, false) => 18,
        (false, true) => 167,
        (true, true) => 205,
    }
}

fn fifo_drain(&mut self, now: u64) {
    if self.fifo_len == 0 {
        self.fifo_slot_clock = self.fifo_slot_clock.max(now); // idle FIFO tracks the clock forward
        return;
    }
    loop {
        if self.fifo_len == 0 {
            self.fifo_slot_clock = self.fifo_slot_clock.max(now);
            return;
        }
        let oldest = self.fifo[(self.fifo_write.wrapping_sub(self.fifo_len) & 3) as usize];
        let slots = match Self::target_of(oldest.code) {
            Target::Vram => 2,
            _ => 1,
        };
        let cost = slots * MCLK_PER_LINE / self.slots_per_line(self.fifo_slot_clock);
        if self.fifo_slot_clock + cost > now {
            return;
        }
        self.fifo_slot_clock += cost;
        self.fifo_len -= 1;
    }
}

fn fifo_enqueue(&mut self, data: u16) {
    self.fifo[self.fifo_write as usize] = FifoEntry { data, code: self.code, addr: self.addr };
    self.fifo_write = (self.fifo_write + 1) & 3;
    self.fifo_len = (self.fifo_len + 1).min(4);
}

/// The next-available FIFO entry (about to be overwritten = written 4 writes ago) — snoop / CRAM-VSRAM fill.
fn fifo_snoop(&self) -> FifoEntry {
    self.fifo[self.fifo_write as usize]
}
```

`data_write` (non-DMA arm) gains `self.fifo_drain(now); self.fifo_enqueue(w);` around the existing
`write_target(w); autoinc();` (so `data_write` now takes `now: u64`; `MegaDriveBus::vdp_write_word` already has
`now_mclk`). `status_word` gains `if mclk < self.dma_busy_until { s |= 1 << 1; }` (0 at power-on → no change).

- [ ] **Step 1:** Write failing tests: (a) `fifo_records_data_code_addr_at_enqueue` — a data write enqueues an
      entry whose `data/code/addr` match; (b) `fifo_and_dma_fields_survive_a_bincode_round_trip`; (c)
      `power_on_fifo_is_empty_and_busy_clear`; (d) `enqueue_still_writes_vram_immediately` (VRAM readback
      unchanged vs today).
- [ ] **Step 2:** Run — expect compile failure (fields/methods absent).
- [ ] **Step 3:** Implement the fields + `FifoEntry` + drain/enqueue/snoop + the status bit + `last_dma()`; thread
      `now` into `data_write` and its `MegaDriveBus` caller (`vdp_write_word` passes `self.now_mclk`).
- [ ] **Step 4:** `cargo test -p oracle-core --lib`; the four new tests pass.
- [ ] **Step 5 (the isolated-commit currency proof):** run **all** of:
      `cargo test -p oracle-core --test export_state_v1 --test oracle_differential --test determinism_gate
      --test golden_frames` — every hash green with the **existing** constants. `cargo fmt --check`;
      `cargo clippy --all-targets -- -D warnings`.
- [ ] **Step 6:** Commit.

```bash
git add crates/oracle-core/src/vdp.rs
git commit -m "feat(vdp): real FIFO as serialized state (contents + coarse drain clock), enqueue-immediate"
```

---

### Slice C — FIFO-full stall through the wait channel

**Files:** Modify `crates/oracle-core/src/vdp.rs` (`data_write` returns `u32` wait), `crates/oracle-core/src/bus.rs`
(`vdp_write_word` returns the wait; `write16`/`write8` thread it).

`data_write` returns wait CPU cycles: after `fifo_drain(now)`, if `fifo_len == 4` compute `wait_mclk =
next_drain_mclk(now) - now` and return `wait_mclk.div_ceil(MCLK_PER_CPU_CYCLE)`, draining the one entry before
enqueue; else 0. `MegaDriveBus::vdp_write_word` returns that; `write16`/`write8` return it in place of the
current `0` for VDP writes. **`FlatBus` is not touched** (it has no VDP path).

- [ ] **Step 1:** Failing tests (in `bus.rs`): (a) `fifo_full_write_returns_wait_cycles` — 5 rapid data writes at
      the *same* `now_mclk` (active-display line): the 5th returns `wait > 0`; (b)
      `fifo_writes_spaced_past_a_slot_do_not_stall` — writes spaced by ≥ the slot cost return 0; (c)
      `flatbus_vdp_absent_wait_is_zero` — a `FlatBus` never yields VDP wait (guards the SST invariant directly).
- [ ] **Step 2:** Run — expect failure (writes currently always return 0).
- [ ] **Step 3:** Implement the wait return + thread through `vdp_write_word`/`write16`/`write8`.
- [ ] **Step 4:** `cargo test -p oracle-core --lib`; new tests pass.
- [ ] **Step 5 (determinism with real stalls):** `cargo test -p oracle-core --test determinism_gate --test proptests`
      green — `run_frames(n) == n × run_frames(1)` now exercises real FIFO stalls (add a proptest ROM that bursts
      data-port writes if the existing proptest ROM does not stall; else note the existing coverage suffices).
- [ ] **Step 6 (the SST bit-identical proof — key commit):** `git diff <base> -- crates/oracle-core/src/m68000/`
      empty; `cargo test -p oracle-core --release --test singlestep_m68000` → `112 / ran >= 1_000_058`. Both
      goldens + `export_state_v1` green (unchanged). `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`.
- [ ] **Step 7:** Commit.

```bash
git add crates/oracle-core/src/vdp.rs crates/oracle-core/src/bus.rs
git commit -m "feat(vdp): FIFO-full stalls the 68k through the Bus68k wait channel (FlatBus stays 0)"
```

---

### Slice D — snoop quirk + slot-timed read buffer

**Files:** Modify `crates/oracle-core/src/vdp.rs` (`data_read` snoop + read-with-nothing-cached stall),
`crates/oracle-core/src/bus.rs` (`vdp_read_word` returns the read wait for `read16`/`read8`).

- **Snoop** (R3): a CRAM/VSRAM data-port read fills its **undefined** bits (the bits above the 9-bit CRAM /
  11-bit VSRAM masks) from `fifo_snoop().data`. VRAM reads are fully defined → unchanged. Behavioral, currency-safe
  (rendering + `state_hash`/`export_state` read the stored bytes directly, not via `data_read`).
- **Read buffer slot-timing** (R3): a data-port read waits for the write FIFO to drain first; a read with nothing
  pre-cached (no completed read command) stalls until an external slot. Return the wait via the read channel.

- [ ] **Step 1:** Failing tests: (a) `cram_read_snoops_undefined_bits_from_next_available_fifo_entry`; (b)
      `vram_read_is_fully_defined_no_snoop`; (c) `read_waits_for_a_nonempty_write_fifo` (read after a FIFO-loading
      burst returns `wait > 0`).
- [ ] **Step 2:** Run — expect failure.
- [ ] **Step 3:** Implement the snoop merge + read wait; thread through `vdp_read_word`/`read16`/`read8`.
- [ ] **Step 4:** `cargo test -p oracle-core --lib`; new tests pass. Both goldens + `export_state_v1` +
      `determinism_gate` green (reads don't touch currencies). fmt + clippy `-D`.
- [ ] **Step 5:** Commit.

```bash
git add crates/oracle-core/src/vdp.rs crates/oracle-core/src/bus.rs
git commit -m "feat(vdp): FIFO snoop quirk on CRAM/VSRAM reads + slot-timed read buffer"
```

---

### Slice E — DMA mode (a): 68k→VDP transfer (the total halt window)

**Files:** Modify `crates/oracle-core/src/vdp.rs` (arm/execute-step API + `DmaRequest`/`DmaRecord`/`DmaMode`),
`crates/oracle-core/src/bus.rs` (`MegaDriveBus` executes the source reads + returns the halt wait).

`control_write`'s second-word arm, when CD5 is set and the mode decodes to **Mem**, arms `DmaRequest::Mem`.
`MegaDriveBus`, after each control write, calls `self.vdp.take_dma_request()`; on `Mem` it loops `len` words:
read the source word from 68k memory (`mapped_byte` pair — ROM/RAM/Z80), call `self.vdp.dma_feed_word(word,
now)` (routes to the current target via `write_target` → `write_vram_byte` → **SAT write-through**, autoinc,
advances the source/length registers), accumulate the coarse cost, then return the total as wait. `dma_busy_until
= now + cost`.

- [ ] **Step 1:** Failing tests: (a) `mem_dma_copies_source_words_to_vram` (a `MegaDriveBus` driving a Mem DMA
      from a ROM region lands the bytes in VRAM at the dest, big-endian); (b) `mem_dma_updates_the_sat_cache`
      (R5 pin: a Mem DMA into the SAT window updates `sat_cache` — "any DMA that writes VRAM counts"); (c)
      `mem_dma_returns_a_halt_wait_from_the_slot_budget` (`wait > 0`, scales with `len`); (d)
      `mem_dma_advances_source_and_zeroes_length_registers` (regs 21–23 advanced, 19/20 → 0).
- [ ] **Step 2:** Run — expect failure.
- [ ] **Step 3:** Implement `DmaRequest`/`take_dma_request`/`dma_feed_word` + the `MegaDriveBus` Mem executor +
      cost integration.
- [ ] **Step 4:** `cargo test -p oracle-core --lib`; new tests pass.
- [ ] **Step 5 (currency re-proof — DMA touches VRAM+regs):** both goldens + `export_state_v1` +
      `oracle_differential` + `determinism_gate` green with existing constants (the fixtures drive no DMA). fmt +
      clippy `-D`.
- [ ] **Step 6:** Commit.

```bash
git add crates/oracle-core/src/vdp.rs crates/oracle-core/src/bus.rs
git commit -m "feat(vdp): DMA mode (a) 68k->VDP transfer — total halt window through the wait channel"
```

---

### Slice F — DMA mode (b): VRAM fill

**Files:** Modify `crates/oracle-core/src/vdp.rs`, `crates/oracle-core/src/bus.rs`.

CD5 armed + mode `10`: `control_write` records the pending fill; the **next data-port write** (the fill value)
triggers it. `data_write`'s DMA arm (currently a no-op placeholder) now: enqueue the value (so `fifo_snoop`/last
entry hold it), then run the fill — `len` writes of the fill byte to the current target via `write_vram_byte`
(SAT write-through), source = **last FIFO entry top byte** (VRAM) / **`fifo_snoop()` "4-writes-ago" entry**
(CRAM/VSRAM — the documented bug); 68k keeps running (`dma_busy_until = now + coarse_cost`, no wait returned);
regs 19/20 → 0, address advanced. **Mid-fill data-port write** (interim, R1 experiment-informed): a data write
while `now < dma_busy_until` redirects per the recorded model.

- [ ] **Step 1:** Failing tests: (a) `vram_fill_fills_the_target_with_the_top_byte`; (b)
      `cram_fill_uses_the_four_writes_ago_entry` (the R4 hardware-bug source); (c)
      `fill_sets_dma_busy_for_the_coarse_window_but_returns_no_wait` (68k keeps running; status b1 set until
      `dma_busy_until`); (d) `fill_updates_the_sat_cache_on_window_hits` (R5 rider confirmed).
- [ ] **Step 2:** Run — expect failure.
- [ ] **Step 3:** Implement the fill trigger + executor + busy window + the interim mid-fill redirect.
- [ ] **Step 4:** `cargo test -p oracle-core --lib`; new tests pass. Currencies re-proved green (fixtures no DMA).
      fmt + clippy `-D`.
- [ ] **Step 5:** Commit.

```bash
git add crates/oracle-core/src/vdp.rs crates/oracle-core/src/bus.rs
git commit -m "feat(vdp): DMA mode (b) VRAM fill — 68k runs, fill source per the R4 CRAM/VSRAM bug"
```

---

### Slice G — DMA mode (c): VRAM copy

**Files:** Modify `crates/oracle-core/src/vdp.rs`, `crates/oracle-core/src/bus.rs`.

CD5 armed + mode `11`: triggers on the control-port second word (no 68k source). Executes `len` byte
read+write steps within VRAM (source from regs 21/22 low, dest = live address), **bypassing the FIFO**, at
**half the fill byte rate** (R4(c)); 68k keeps running (`dma_busy_until` window). Each byte write routes through
`write_vram_byte` → SAT write-through (R5 rider).

- [ ] **Step 1:** Failing tests: (a) `vram_copy_moves_bytes_within_vram`; (b)
      `copy_runs_at_half_the_fill_byte_rate` (busy window ≈ 2× a same-length fill); (c)
      `copy_keeps_the_68k_running_no_wait`; (d) `copy_updates_the_sat_cache_on_window_hits`.
- [ ] **Step 2:** Run — expect failure.
- [ ] **Step 3:** Implement the copy trigger + FIFO-bypass byte executor + half-rate cost.
- [ ] **Step 4:** `cargo test -p oracle-core --lib`; new tests pass. Currencies re-proved. fmt + clippy `-D`.
- [ ] **Step 5:** Commit.

```bash
git add crates/oracle-core/src/vdp.rs crates/oracle-core/src/bus.rs
git commit -m "feat(vdp): DMA mode (c) VRAM copy — FIFO-bypass, half rate, 68k runs"
```

---

### Slice H — `frame_report` DMA section

**Files:** Modify `crates/oracle-core/src/render.rs` (or wherever `frame_report`/`render_line_report` live —
grep first) to add the DMA rollup; read `last_dma` from `Vdp`.

Expose the completed-transfer record(s): `frame_report().dma` = the source/dest/length/mode/target of the DMA(s)
performed. Minimum: the most-recent `DmaRecord` (`last_dma`); if `frame_report` already accumulates per-frame,
extend the accumulation.

- [ ] **Step 1:** Failing test: `frame_report_lists_the_dma_performed` — after driving a Mem DMA, the report
      names mode/source/dest/len/target.
- [ ] **Step 2:** Run — expect failure.
- [ ] **Step 3:** Implement the `frame_report` DMA field from `last_dma`.
- [ ] **Step 4:** `cargo test -p oracle-core --lib`; pass. Currencies unaffected (introspection only). fmt + clippy `-D`.
- [ ] **Step 5:** Commit.

```bash
git add crates/oracle-core/src/render.rs
git commit -m "feat(vdp): frame_report DMA section — transfers performed (source/dest/len/mode)"
```

---

### Slice I — `frame_dump` loads its art via DMA (the end-to-end proof)

**Files:** Modify `crates/oracle-core/examples/frame_dump.rs`.

Convert the tile/nametable art loads from data-port write loops to **DMA**: stage the tile bytes in work RAM
(or ROM), program regs 19–23 (length/source/mode), and trigger a **68k→VDP transfer** to VRAM — exactly what a
real game does. Keep at least one **VRAM fill** (e.g. clearing plane B) to exercise mode (b). The SAT may stay a
data-port write (or also move to DMA). **The rendered PPM must still show the same picture** (stripes + sprite
boxes + shadow/highlight) — the proof that art-load-by-DMA reaches the screen.

- [ ] **Step 1:** Rewrite the plane/tile loads as a DMA: add a `dma_to_vram(rom, dest, source, words, mode)`
      helper that emits the reg 19–23 writes + the control-port trigger command; move the striped nametable +
      solid tiles through it; clear plane B via a VRAM fill.
- [ ] **Step 2:** `cargo run --release --example frame_dump -- 2 /tmp/frame.ppm`; open the PPM (or check
      programmatically: 256×224, the expected colour census). Confirm the picture is unchanged from Push 5.
- [ ] **Step 3:** `cargo clippy --all-targets -- -D warnings` (examples included); `cargo fmt --check`.
- [ ] **Step 4:** Commit.

```bash
git add crates/oracle-core/examples/frame_dump.rs
git commit -m "feat(vdp): frame_dump loads its art via DMA like a real game (end-to-end proof)"
```

---

## Anti-cheating invariants (the verifier will check all of these)

1. **`m68000/*` diff = 0 lines** at HEAD (`git diff <base> -- crates/oracle-core/src/m68000/`).
2. **SST threshold literally `ran >= 1_000_058`; the `singlestep_m68000` harness file is untouched** (`git diff
   <base> -- crates/oracle-core/tests/singlestep_m68000.rs` empty). Full sweep green at slice C **and** HEAD.
3. **`FlatBus` VDP wait is provably 0** — a unit test asserts it; `FlatBus` has no VDP path and never triggers a
   DMA.
4. **`state_hash` layout + `export_state` golden `0x22F80ECF29ED3AD4` byte-identical throughout** — no
   regeneration. The field-adding slice B and the DMA slices E–G each re-prove both goldens + `export_state_v1`
   green with the existing constants (the isolated-commit proof).
5. **`golden_frames.rs` scene hashes unchanged** — this push adds **no** golden scene and regenerates **none**
   (Decision 5). If a scene hash moves, stop.
6. **Every new serialized field round-trips** (bincode round-trip test) and is in **neither** currency.
7. **No floats** anywhere (grep the diff for `f32`/`f64`/`.0` literals in cost math).
8. **No dead code** — each field/method used in its introducing slice (clippy `-D`).
9. **Clean-room** — behavior cites only the recon docs + design brief + BlastEm instrument; no emulator source;
   `../oracle/` untouched.
10. **No `Co-Authored-By` trailer**; every commit individually fmt-clean.

---

## Decisions surfaced (not defaulted — flag in the report; owner/reviewer may override)

1. **FIFO applies VRAM mutation at enqueue, not at drain** (the central simplification). Recommended: the
   scanline latch + read-waits-for-empty-FIFO make it unobservable within Phase-2 granularity, and it keeps
   slice B trivially currency-neutral. A drain-time pipeline is a Phase-3 refinement behind the same API.
2. **Coarse DMA/FIFO cost = per-line slot-budget integration** (active/blank per line), not a flat rate and not
   Kabuto's float formulas (no floats; formulas kept as corroboration only). Slot positions within a line stay
   deferred to Phase 3 — stated in the ledger.
3. **Fill/copy mutate VRAM atomically at execution + model elapsed time with a `dma_busy_until` window** (rather
   than stepping the transfer across scheduler events). Recommended: the 68k can't read VRAM mid-DMA, so only the
   busy window is observable; simpler and deterministic.
4. **R5 rider verdict: CONFIRM the interim model** — fill/copy steps route through `write_vram_byte` so they hit
   the SAT write-through window compare like any write (Mem-DMA-updates-cache is an explicit R5 pin; fill/copy is
   the "not stated in any source" remainder, and hitting the compare matches the general formulations).
   Confirmed by unit tests (slices E/F/G); **no golden regen** (golden-frame evidence permitted but not needed).
5. **No new golden scene for DMA.** Goldens stay frozen (hard constraint). DMA is proved by unit tests + the
   `frame_dump` visual + the currency re-proofs. Adding a DMA golden scene would force a documented hash regen we
   do not need — declined unless the reviewer wants a frozen DMA frame.
6. **Mid-fill data-port-write redirect is an interim model**, flagged intermittent per the R1 experiment (slice
   B') + the divergence ledger — an intermittent hardware cell is a documented limitation, not a pin.

---

## Risks

- **Currency leak** if a DMA path or the FIFO touches a hashed region under a fixture that exercises it.
  *Mitigation:* the golden fixture + SST corpus drive **no** VDP DMA; VRAM mutation stays at enqueue in slice B;
  re-prove both goldens + `export_state_v1` at slices B, C, E, F, G.
- **The wait channel perturbing SST.** *Mitigation:* `FlatBus` has no VDP and returns 0; `m68000/*` zero-diff;
  SST bit-identical proof at slice C.
- **`run_frames` determinism breaking under real stalls.** *Mitigation:* the absolute-frame-deadline carry
  (Push C) already absorbs overshoot; the proptest re-runs `run_frames(n) == n × run_frames(1)` with a
  FIFO-stalling ROM at slice C.
- **BlastEm experiment flakiness / infeasibility** (the frame-capture spike was negative). *Mitigation:* slice
  B' is timeboxed; the interim models are the implementation defaults, so a negative experiment does not block —
  record it honestly and proceed.
- **clippy `-D` dead-code** on new fields. *Mitigation:* each field is used in its introducing slice (the
  timing-skeleton FIFO-omission lesson).
- **DMA source reads crossing the open-bus / mapping edges** in `MegaDriveBus`. *Mitigation:* reuse the existing
  `mapped_byte` path (ROM/RAM/Z80/open-bus already handled); a test drives a Mem DMA from a ROM region.

---

## Self-review (done against the task brief)

- **Spec coverage:** FIFO real (B) ✓; wait-channel FIFO-full stall (C) ✓; read-buffer slot-timed + snoop (B/D) ✓;
  three DMA modes (E/F/G) ✓; DMA-busy on setup write (E/F/G) ✓; fill 4-writes-ago CRAM/VSRAM bug (F) ✓; mid-fill
  suspend/redirect (F, interim) ✓; copy FIFO-bypass half-rate (G) ✓; regs 19–23 mutate (E) ✓; coarse per-line
  budget, positions deferred (Design + Decision 2) ✓; R5 fill/copy×SAT rider (Decision 4, E/F/G tests) ✓; R1 two
  cells experiment (B') ✓; `frame_report` DMA section (H) ✓; `frame_dump` via DMA (I) ✓; recon-lite DMA formats
  (A) ✓. New serialized state round-trips + out of both currencies, proved at the field-adding commit (B) ✓;
  wait wiring shows determinism gate + proptests green (C) ✓.
- **Hard constraints mapped:** `m68000/*` zero-diff (invariant 1); SST exactly `ran >= 1_000_058`, harness
  untouched, `FlatBus` 0 (invariants 2/3); `state_hash` + export golden byte-identical, no regen (invariant 4);
  golden-frame hashes frozen (invariant 5); clean-room + no `../oracle/` (invariant 9); no floats (invariant 7);
  fmt + clippy `-D` per commit + conventional commits, no co-author (invariant 10); SST at key commits + HEAD
  with trees stated (ground rules).
