# `export_state` — v1 (frozen) + v2 (SRAM go-live)

**Status: v1 FROZEN, 2026-07-16 (integration pivot D8); v2 SHIPPED 2026-07-23 (SRAM go-live, slice S3).**
`EXPORT_STATE_VERSION = 2`. The v1 layout below is retained as historical; v2 is v1 plus a single appended
64 KiB SRAM tail region (see [## v2 — SRAM region](#v2--sram-region) at the end). Every offset 0–7 is
unchanged; only the total length grows and the version field reads `2`.

`export_state` is oracle-next's **cross-backend differential currency**: a flat, versioned, little-endian
byte image of the machine's architectural state, captured at an instruction boundary. It is what the
determinism gate hashes and what a lockstep BlastEm-over-RSP differential compares against. It is distinct
from:

- the **bincode `snapshot`/`restore`** (the full internal `System`, including mid-instruction CPU
  micro-state, the scheduler, and the master clock — used for save states and snapshot-restore testing), and
- the Oracle-compatible **`state_hash`** (an FNV hash of VDP memory + registers only, byte-for-byte
  identical to Oracle's `OpStateHash`, kept for the live-Oracle differential).

`export_state` is defined and produced by `System::export_state`; `System::export_state_hash` is its
FNV-1a-64 hash (an independent hasher — it does **not** touch the frozen Oracle `state_hash` layout). The
golden-layout test `crates/oracle-core/tests/export_state_v1.rs` pins this layout; any accidental drift is a
hard test failure.

## Layout

Byte order is **little-endian** throughout (matches the host and the SingleStepTests serialization). Regions
are contiguous and gap-free, in this fixed order:

| # | Region | Offset | Size (hex / dec) | Contents |
|---|--------|--------|------------------|----------|
| 0 | version | `0x00000` | `0x2` / 2 | `u16` LE = `EXPORT_STATE_VERSION` (currently 1) |
| 1 | m68k regs | `0x00002` | `0x4E` / 78 | see below |
| 2 | work RAM | `0x00050` | `0x10000` / 65536 | live `$FF0000–$FFFFFF` 68000 work RAM |
| 3 | Z80 RAM | `0x10050` | `0x2000` / 8192 | **live** `$A00000` 8 KiB Z80 RAM |
| 4 | Z80 regs (reserved) | `0x12050` | `0x40` / 64 | all-zero; reserved for the future Z80 register file |
| 5 | VDP | `0x12090` | `0x100E8` / 65768 | **live** VRAM `0x10000` + CRAM `0x80` + VSRAM `0x50` + regs 24 (state_hash order) |
| 6 | FM (reserved) | `0x22178` | `0x200` / 512 | all-zero; YM2612 register-file scale |
| 7 | PSG (reserved) | `0x22378` | `0x10` / 16 | all-zero; SN76489 register/latch scale |

**Total = `0x22388` = 140168 bytes.**

### Region 1 — m68k registers (78 bytes)

In order, each little-endian:

| Field | Type | Bytes |
|-------|------|-------|
| `d0`–`d7` | `u32` × 8 | 32 |
| `a0`–`a6` | `u32` × 7 | 28 |
| `usp` | `u32` | 4 |
| `ssp` | `u32` | 4 |
| `pc` | `u32` | 4 |
| `sr` | `u16` | 2 |
| `prefetch[0]`, `prefetch[1]` | `u16` × 2 | 4 |

`a7` is not stored separately — the active stack pointer is whichever of `usp`/`ssp` the current `sr`
supervisor bit selects, and both are present. `prefetch` is the 68000's two-word prefetch queue (part of the
architectural state at an instruction boundary). This is exactly the SingleStepTests register vocabulary.

### Regions 2–3, 5 — live memory

Work RAM, Z80 RAM, and the VDP region are **live** bytes copied straight from `System`. The Z80 RAM is
68000-reachable at `$A00000` (a store there is real mutable state), so it is in the currency and is
cross-backend comparable — BlastEm's RSP `m` command reads it through the 68k window
(`read_mem(0xA00000, 0x2000)`). The VDP region is the four Oracle-hashed regions (VRAM → CRAM → VSRAM →
registers, the `state_hash` order) at the frozen sizes; it went live filling the previously-zeroed reserve at
unchanged size — the designed v1 *content* change (no version bump, per the rule below).

### Regions 4, 6–7 — reserved

Zeroed placeholders for chips not yet emulated in this pivot. Two flavors:

- **Reserved at generous-margin size** (region 4, Z80 regs): when the chip lands, its state fills the existing
  zeroed bytes. That is a *content* change, **not** a layout change — it does **not** bump the version. (Region
  5, VDP, was such a reserve and has now filled — the designed content change.)
- **Reserved at register-file scale** (regions 6/7, FM/PSG): sized to the chip's addressable register file,
  not its full internal state. The full YM2612/SN76489 internal state (envelope/phase accumulators, LFO,
  timers, LFSR) exceeds these and, in the FM case, is not even readable over BlastEm's RSP (YM2612 registers
  are write-only on hardware). When those cores land, enlarging the region **is** a layout change and bumps
  to v2. Both are tail regions, so a resize churns no other offset.

## Invariants

- **Instruction-boundary export only.** `run_frames` leaves the CPU quiesced at an instruction boundary, so
  `export_state` never captures mid-instruction micro-state. Mid-instruction state is snapshot territory
  (bincode `snapshot`/`restore`), never `export_state`.
- **The master clock is NOT in the currency.** `mclk` / `frame_boundary_mclk` and all scheduler timing live
  in the bincode snapshot but never in `export_state`. Timing divergences between backends are compared
  **separately** as xfail-manifest entries (`tools/blastem-differential/known_differences.py`) — never as
  state divergences. Two backends that reach the same architectural state via different cycle counts agree on
  `export_state` and are recorded as a timing difference, not a state divergence.
- **Little-endian**, as built.

## Deliberate exclusions (v1)

Two pieces of real bus-level state are **deliberately not** in the currency:

- **`last_bus_word` (the open-bus latch).** Mutable micro-architectural state (open-bus reads sample it),
  present in the bincode snapshot, excluded here: cross-backend open-bus models legitimately differ (ours is
  a documented placeholder), and same-seed determinism runs agree on it regardless. If open-bus behavior ever
  causes a real divergence, it surfaces downstream as an architectural difference (a register or RAM byte) —
  which is exactly what the currency compares.
- **Z80 BUSREQ/RESET arbitration state.** Not stored state at all in this pivot (reads report a constant
  bus-granted/reset-released; writes are accepted and dropped — `bus.rs`). There is nothing to serialize.
  When these become real latches, their natural home is region 4's reserve — a content change at unchanged
  size, no version bump.

## Version-bump rule

Bump `EXPORT_STATE_VERSION` (and regenerate the golden constants in `export_state_v1.rs` in the **same
commit**) **iff the byte layout changes**:

- a region is added, removed, reordered, or resized;
- any field's offset shifts;
- the endianness changes.

Do **not** bump when a reserved zeroed region fills with live bytes at unchanged size — that is a content
change, and it is the designed path (the VDP region 5 filled this way; the Z80-register reserve is next).

## Anti-drift guard

`crates/oracle-core/tests/export_state_v1.rs` pins, from a fixed seed + the vendored test ROM + a fixed
frame count: the total length, every region's offset/size (as independent literals, not recomputed from the
production constants), the per-region semantics, and a byte-exact `export_state_hash`. A silent layout change
fails the test loudly.

## v2 — SRAM region

**Shipped 2026-07-23 (slice S3, the SRAM feature's one deliberate currency-boundary change).** v2 appends
exactly one region to the tail of the v1 image; nothing before it moves.

| # | Region | Offset | Size (hex / dec) | Contents |
|---|--------|--------|------------------|----------|
| 8 | SRAM | `0x22388` | `0x10000` / 65536 | live cartridge SRAM bytes, **left-justified, zero-padded** to a fixed 64 KiB (all-zero when the cart has no SRAM) |

**New total = `0x32388` = 205704 bytes** (v1 `0x22388` + `0x10000`).

- **Why a version bump (not a content-fill).** Unlike the VDP and Z80-register reserves — which went live by
  filling *pre-carved* zeroed bytes at unchanged size (a content change, no bump) — SRAM had **no** reserved
  slot in v1. Adding the region is a genuine layout change, so per the version-bump rule it bumps
  `EXPORT_STATE_VERSION` 1→2 and regenerates the `export_state_v1.rs` golden (`GOLDEN_HASH`, offsets, total)
  in the **same commit**. This is the single attributable golden regen of the whole SRAM feature.
- **Fixed 64 KiB, regardless of cart.** Real cartridge SRAM is 2–64 KiB and cart-dependent. Reserving the
  standard maximum (64 KiB) keeps the layout stable across every cart; the live bytes (`System.sram`, sized to
  the detected header range, empty when `!sram_present`) are written left-justified and the remainder
  zero-filled. Being the tail region, any future resize churns no other offset (the same rationale FM/PSG
  cite).
- **Raw byte lane only.** The region holds only the SRAM chip's byte contents. The `$A130F1`
  enable/write-protect latch, the `sram_dirty` throttle flag, and the base/end/odd map stay **bincode-only**
  (real state that rides the snapshot for determinism, but not architectural currency) — exactly the split
  used for `z80_busreq` and the other bus-arbitration scalars.
- **SRAM stays OUT of `state_hash`.** Oracle's `OpStateHash` hashes VDP memory + registers only and excludes
  SRAM, so the live-Oracle A/B differential (`oracle_differential.rs`) forbids us from adding SRAM to
  `state_hash`. Fork 2 of the design recon is non-negotiable: v2 touches `export_state` alone; `state_hash` is
  byte-identical.
- **Currency scope.** The v2 SRAM region is currently **determinism-gated only** (the same-seed determinism
  gate + snapshot round-trips exercise it). It becomes cross-backend-comparable only once the BlastEm-over-RSP
  differential path can read `$200000+` SRAM through the 68k window (design recon open question 3) — until
  then it is a determinism-only region, not a cross-backend one.
