# Push D — `export_state` v1 freeze (integration pivot D8)

**Status: PLAN, 2026-07-16.** Design of record: `docs/decisions/2026-07-15-integration-pivot-design.md`
§D8. The last push of the integration pivot. Follows Push C (system wiring — built the `export_state`
*currency* on the real CPU but did NOT freeze it). Recon: [[recon-export-state-v1]] in session memory.

## Goal

Freeze the `export_state` byte layout as **v1** and write its spec. This is the cross-backend differential
currency (oracle-next vs BlastEm-over-RSP, and oracle-next's own determinism gate). Freezing means: every
region's offset/size is pinned, a checked-in golden test makes any accidental drift a hard failure, and
`docs/export-state-v1.md` documents the layout + the version-bump rule so v2 is a deliberate, rare event.

v1 is frozen **exactly once**. The one open layout question (the Z80 region) is resolved *first* and lands
*before* the freeze, so the golden fixture pins the final layout.

## Recon findings (full detail: [[recon-export-state-v1]])

### The Z80-region decision — RESOLVED: live RAM + reserved register sub-block

Push C serialized the Z80 region as `Z80_RAM_SIZE` (0x2000) all-zeros. But `z80_ram` is **genuine
68000-reachable mutable state**: `bus.rs` `write8` maps `$A00000–$A0FFFF` (mirrored) → `z80_ram`. A ROM
writing `$A00000` mutated state invisible to the currency — the reviewer's flagged gap. Findings agree
with the reviewer's recommendation, so:

- **Emit the live `z80_ram` bytes** (0x2000).
- **Followed by a fixed zeroed 0x40 sub-block reserved for the future Z80 register file.** The Z80's full
  architectural register set (AF/BC/DE/HL + shadows + IX/IY/SP/PC + I/R + IFF1/IFF2/IM + halt + WZ) ≈ 0x20
  bytes; 0x40 is 2× margin, so filling it when the Z80 core lands is a **content** change (no version bump).

Cross-backend comparable: BlastEm's RSP `m` (read memory) command reads any 68k address; the harness's
`rsp.py read_mem(0xA00000, 0x2000)` reads Z80 RAM through the 68k window (README demos `read_mem(0xFF0000,4)`
for work RAM). This changes the Z80 region from **0x2000 zeros** to **0x2000 live + 0x40 reserved = 0x2040**;
it is the only layout change in the freeze.

### FM/PSG footprint sanity-check — KEEP 0x200 / 0x10

- **FM (YM2612) 0x200** = the 2-port × 0x100 addressable register-file space, exactly. Full internal FM
  state (24 operators' phase/EG accumulators, LFO, 2 timers) exceeds 0x200 and is NOT readable over RSP
  anyway (YM2612 registers are write-only on hardware) → the FM region can only ever be an
  oracle-next-internal reserve. A v2 bump is the expected path when the FM core lands (FM is second-to-last;
  only PSG follows).
- **PSG (SN76489) 0x10** = 4ch × (10-bit tone + 4-bit atten) + noise ctrl + latch byte + 16-bit LFSR ≈ 16
  bytes; right at the register+latch footprint. PSG is the **last** region → any future resize shifts nothing.

Both are register-file-scale reserves, honestly documented. Freezing them minimal-but-meaningful beats
padding with speculative zeros; a future enlargement is a clean v2 with no offset churn elsewhere.

### VDP region — already reserved at final size

`VRAM 0x10000 + CRAM 0x80 + VSRAM 0x50 + regs 24 = 0x100E8` (from `state_hash`). Fills without a bump when
the VDP lands. Not 68000-mutable now (VDP stub drops writes), so staying zeroed in export_state is correct;
VDP memory is covered by the Oracle-compatible `state_hash` for the live-Oracle differential.

## The frozen v1 layout (little-endian, as built)

| # | Region | Offset | Size (hex / dec) | Semantics |
|---|--------|--------|------------------|-----------|
| 0 | version | `0x00000` | 2 / 2 | `u16` LE, = 1 |
| 1 | m68k regs | `0x00002` | `0x4E` / 78 | d0–d7, a0–a6, usp, ssp, pc (LE `u32` ×18), sr (LE `u16`), prefetch[0], prefetch[1] (LE `u16` ×2) |
| 2 | work RAM | `0x00050` | `0x10000` / 65536 | live `$FF0000` work RAM |
| 3 | Z80 RAM | `0x10050` | `0x2000` / 8192 | **live** `$A00000` Z80 RAM |
| 4 | Z80 regs (reserved) | `0x12050` | `0x40` / 64 | zeroed; fills when the Z80 core lands (no bump) |
| 5 | VDP (reserved) | `0x12090` | `0x100E8` / 65768 | zeroed at final size: VRAM 0x10000 + CRAM 0x80 + VSRAM 0x50 + regs 24; fills when the VDP lands (no bump) |
| 6 | FM (reserved) | `0x22178` | `0x200` / 512 | zeroed; YM2612 register-file-scale; full internal state → v2 |
| 7 | PSG (reserved) | `0x22378` | `0x10` / 16 | zeroed; SN76489 register/latch-scale; full internal state → v2 |

**Total = `0x22388` = 140168 bytes.**

## v1 invariants (restated in the spec)

- **Instruction-boundary export only.** `run_frames` leaves the CPU quiesced at an instruction boundary;
  mid-instruction state is snapshot territory (bincode `snapshot`/`restore`), never export_state.
- **The master clock is NOT in the currency.** Timing divergences are xfail-manifest entries
  (`known_differences.py`), compared separately — never state divergences. mclk / `frame_boundary_mclk`
  are in the bincode snapshot but never in `export_state`.
- **Little-endian**, as built (matches the host and the SST vocabulary's LE serialization).
- **Version-bump rule:** bump `EXPORT_STATE_VERSION` when the byte **layout** changes — any region added,
  removed, reordered, or resized; any field's offset shifts; endianness changes. Do **not** bump when a
  reserved zeroed region fills with live bytes at unchanged size (a content change) — the designed path for
  the VDP + Z80-register reserves.

## Slices (TDD; full triplet + SST re-run per slice; one conventional commit each)

Only `System::export_state` changes — the CPU core `m68000/*` is untouched, so **SST is structurally
invariant** (re-run anyway per discipline). `determinism_gate`/`proptests` compare relatively (no hardcoded
value), so the layout change is safe there; only the new golden test pins absolute values.

- **D1 (feat)** — Z80 region goes live + reserved register sub-block. Add `EXPORT_Z80_RAM_LEN` (0x2000) +
  `EXPORT_Z80_REGS_PLACEHOLDER` (0x40); `export_state` emits `self.z80_ram` then the zeroed sub-block.
  TDD: a test mutates `z80_ram` through `mega_bus().write8($A00000,..)` and asserts the byte appears at the
  Z80-RAM offset in `export_state`, and that the 0x40 reserved sub-block is zeroed. Update the existing
  `export_state_has_the_fixed_layout_and_version` size math.
- **D2 (test + docs)** — the freeze: `crates/oracle-core/tests/export_state_v1.rs` golden-layout fixture
  (known SEED + testrom + N frames → hard-asserted total length, every region offset/size, **and a
  byte-exact `export_state_hash` constant**) + `docs/export-state-v1.md` (layout table, offsets, semantics,
  invariants, version-bump rule). Accidental drift → hard test failure.

## Anti-drift / anti-cheating

- The golden hash is captured from a green run and hard-asserted; if a future change shifts the layout the
  test fails loudly (must bump the version + regenerate the golden deliberately).
- Region offset/size asserts are written from the table above (independent of the code's arithmetic), so a
  constant typo that still sums to the same total is still caught.
- Threshold `ran >= 1_000_058` and the Oracle `state_hash` FNV layout are untouched.

## Out of scope (flagged, not started)

- After D ships, the **macro-RTC perf pass** trigger fires (audit policy 7) — a separate brief.
- The **Halted/double-fault wiring** (reviewer docket item) is a *separate* follow-up plan + commits (it
  touches the frozen `m68000/*` core), not part of this freeze.
