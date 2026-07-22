# FM `$A04000` status recon + design (DR-1b) — the YM2612 busy-flag that blocks Gunstar's render

**Status: RECON + DESIGN, 2026-07-22. Docs only — no code this slice.** The Z80 BUSREQ slice
(`docs/2026-07-22-z80-busreq-recon.md`, landed as `1ffed93`) resolved Gunstar's `$3090` freeze but exposed a
**second, independent blocker**: after booting past the BUSREQ spin, Gunstar hangs at `$66360` polling the
**YM2612 FM chip's busy flag** at `$A04000`. oracle-next maps `$A04000–$A04003` (the FM chip) as **Z80 RAM**,
so the game's write of an FM register address ≥ `$80` (observed: `$F3` in `z80_ram[0]`) leaves bit 7 set and
the busy-poll spins forever. A throwaway experiment already proved that the BUSREQ fix **plus** a not-busy FM
read boots Gunstar to its main loop, enables the display, and renders a fully non-black frame. This doc pins
the FM `$A04000` status semantics, **answers the load-bearing gate question empirically** (does any
frozen-currency fixture touch `$A04000–3`?), surfaces the write-side design decision, and gives the gated
plan.

**Permitted sources only** (audit policy 3): official Sega documentation, **Plutiedev**, SpritesMind hardware
threads. **No emulator source opened.** Primary in-situ evidence: the Gunstar disassembly (user's ROM, on disk
only, D5) and oracle-next's own committed tests.

Items are numbered **F1–F5**. Each states the pin, evidence, confidence, the behavioral-vs-timing class, and
the open remainder with its pin-vs-defer disposition.

---

## Part 1 — Pinned FM `$A04000` semantics

### F1 — The four addresses

**PINNED.** The YM2612 exposes two register banks to the 68000, each an address/data port pair:

| Address | Register |
|---|---|
| `$A04000` | Bank 0 **address** port (write) / **status** (read) |
| `$A04001` | Bank 0 **data** port (write) |
| `$A04002` | Bank 1 **address** port (write) |
| `$A04003` | Bank 1 **data** port (write) |

**Evidence**: Plutiedev "YM2612 registers": *"YM2612 has two sets of registers split in banks: bank 0 is
accessed at `$A04000/1` and bank 1 is accessed at `$A04002/3`."* Corroborated by the Genesis Software Manual
FM section. **Confidence**: high. **Classification**: behavioral. **Open remainder**: none.

### F2 — The status read (the load-bearing pin)

**PINNED.** A **read** of `$A04000` (the status is presented on all four addresses on real silicon; games read
`$A04000`) returns the YM2612 status byte:

| Bit | Field |
|---|---|
| 7 | **BUSY** — set briefly after a register write while the chip latches it; clears on its own |
| 1 | Timer B overflow |
| 0 | Timer A overflow |
| 6–2 | unused (read 0) |

A game writes an FM register by: write register number → address port, poll **bit 7** until clear, write value
→ data port (recon F3). oracle-next currently returns `z80_ram[0]` for a `$A04000` read (the RAM mirror), so
after a `$A04000` write of an FM register address ≥ `$80` the "status" reads back with bit 7 set → the
busy-poll (`btst #7,(a0) / bne`) never exits. **This is the DR-1b hang.**

**Evidence**: Plutiedev "YM2612 registers" (status bits 7/1/0); Gunstar in-situ at `$66360`:
`lea $A04000,a0 / btst #7,(a0) / bne` — the busy-wait, twice (before the address write and before the data
write), the textbook F3 sequence. **Confidence**: high. **Classification**: behavioral (the value the poll
reads). **Open remainder**: the timer-overflow bits (F5).

### F3 — The write sequence

**PINNED.** Program an FM register as: (1) write the register number to the address port (`$A04000`/`$A04002`),
(2) poll status bit 7 until clear, (3) write the value to the data port (`$A04001`/`$A04003`), (4) poll bit 7
again before the next write. **Evidence**: Plutiedev "YM2612 registers" write sequence; Gunstar `$66360`
routine (`btst #7 / bne` / `move.b d0,(a0)` / `nop` / `btst #7 / bne` / `move.b d1,(1,a0)`). **Confidence**:
high. **Classification**: behavioral. **Open remainder**: none.

### F4 — Busy timing: not-busy is observationally safe (the same argument as BUSREQ Z3)

**PINNED (behavioral) + timing deferred.** On hardware BUSY is asserted for a short, chip-clock-scaled window
after each write, then clears; **every consumer polls bit 7 until it clears** (F3). A model that reports
**not-busy always** (bit 7 = 0) is therefore observationally safe: the poll exits on its first read, and no
game can distinguish "was never busy" from "busy cleared in N cycles" — it only branches on the final value.
The busy duration is a **timing** property (deferrable), not behavioral. This is exactly the reasoning that
made the BUSREQ instant-grant model safe (recon Z3). **Confidence**: high (safety of the not-busy value).
**Classification**: value = behavioral, duration = timing. **Open remainder**: exact busy duration — deferred
(no boot/render consumer observes it; pin from the YM2612 datasheet when the FM core lands).

---

## Part 2 — The gate question, answered empirically

The overseer's load-bearing question: **do `s4.bin` or any frozen-currency fixture write to `$A04000–$A04003`
within the exported/rendered window?** If yes, changing how those writes are decoded (carve-out vs. keep in
`z80_ram`) could move a frozen currency, because **`$A04000–$A04003` alias `z80_ram[0..3]`** (the 8 KiB Z80 RAM
is mirrored across the 64 KiB window: `$A04000 & 0x1FFF = 0`, same byte `$A00000` uses; `$A04001→[1]`,
`$A04003→[3]`), and `z80_ram` **is** in `export_state`.

**Answer: no frozen-currency fixture reads or writes `$A04000–$A04003`. The FM decode change is
currency-neutral by construction.** Enumerated over all five committed gates:

| Gate | Fixture | Touches `$A04000–3`? | Why it's neutral |
|---|---|---|---|
| **export golden** (`export_state_v1`) | vendored `testrom::build()`, seed `0xD8…01`, 60 frames | **No** | The test *already asserts* the entire Z80 RAM region is **all-zero** ("this ROM never writes `$A00000`", lines 127–134). The RAM-stir ROM touches only work RAM. |
| **golden_frames** (7) | bus-less `Vdp` scenes | **No** | Pure `Vdp` built through the VDP ports + rendered; **never instantiates the bus**. No `$A0xxxx` path exists. |
| **SST** (1,000,058) | `FlatBus` | **No** | Runs the CPU against flat memory; `$A04000` has no special decode there at all. |
| **determinism_gate** | vendored `testrom::build()` | **No** | Same RAM-stir ROM; no `$A00000`/`$A04000` writes. |
| **oracle_differential** (3) | **captured static bytes** | **No** | Feeds pre-captured VDP bytes through FNV-1a; no `System`, no bus, no live ROM run. The "s4 engine ROM" in its header describes the *2026-06-24 capture provenance*, not a live run. |

**`s4.bin` is not in any frozen-currency gate.** It *does* write the FM (aeon `sound_api`), but it appears only
in the manual rung-2 validation and the differential-ROM sweep — neither a committed gate. So even though s4's
FM writes are real, they never enter the frozen currencies.

**Confidence**: high (each fixture inspected; the export-golden neutrality is backed by an *existing passing
assertion* that the Z80 RAM region is all-zero). **Open remainder**: the slice must still *prove* neutrality by
re-running all five gates byte-identical (Part 4) — construction-argument first, empirical proof second.

---

## Part 3 — The design decision (surfaced, not defaulted)

Because no frozen currency touches `$A04000–3`, **both** options below are currency-neutral. The decision is on
**correctness**, not gate-safety. Reads must change (return not-busy, F2/F4); the open question is **writes**.

**Option A — reads-only carve-out (minimal).** Intercept only `$A04000–3` *reads* → not-busy status; leave
writes landing in `z80_ram[0..3]` (unchanged).
- *Pro*: smallest diff; unblocks Gunstar (the hang is a read).
- *Con*: leaves the **FM↔Z80-RAM aliasing bug** — an FM register write still corrupts `z80_ram[0..3]`, which is
  *also* real Z80 RAM (`$A00000–3`). A game that writes the FM *and* uses those Z80-RAM bytes gets silent
  corruption. This is a latent divergence, not a fix.

**Option B — full carve-out (reads + writes) [RECOMMENDED].** Intercept `$A04000–3` entirely: reads →
not-busy status; writes → **dropped** (FM stub, no `z80_ram` store). `$A00000–3` (Z80 RAM) and `$A04000–3` (FM)
become correctly independent devices.
- *Pro*: **correct** — removes the aliasing corruption; this is what "model `$A04000` as the FM chip" means.
- *Con*: one extra write-side match arm (the side that *carried* the flagged gate risk — here **empirically
  nil**, Part 2).

**Recommendation: Option B.** The write-side gate risk the overseer flagged is real in principle but **zero in
this codebase** (no fixture writes `$A04000–3`), so there is no reason to accept Option A's correctness
compromise to dodge a non-existent risk. Option B is the actual fix; Option A is a patch over the symptom that
leaves a latent memory-corruption bug behind.

---

## Part 4 — Gated implementation plan

A single bounded slice in `crates/oracle-core/src/bus.rs`. **No `m68000/*`, no `system.rs`, no exported-state
layout change.**

### Design (Option B)

- **`mapped_byte`**: add `0xA0_4000..=0xA0_4003 => Some(0x00)` (not-busy, no timer overflow) **before** the
  `0xA0_0000..=0xA0_FFFF` Z80-RAM arm so it takes precedence over the mirror.
- **`store_byte`**: add `0xA0_4000..=0xA0_4003 => {}` (FM writes drop — do not fall through to the `z80_ram`
  store) **before** the Z80-RAM arm.
- Nothing else. `$A00000–3` (real Z80 RAM) is unaffected (different addresses); the export-state layout,
  `state_hash`, and the BUSREQ latch are untouched.

### Gates (ordered; each must pass before the next)

1. **Semantics unit tests** (new, `bus.rs`): a read of `$A04000`/`$A04001`/`$A04002`/`$A04003` returns `0x00`
   (bit 7 clear = not busy); a write to `$A04000` does **not** appear at `$A00000` (proves the carve-out broke
   the alias — write `$A04000 = $F3`, read `$A00000`, assert it is **not** `$F3`); the busy-poll idiom
   (`btst #7 / bne`) exits.
2. **Frozen-currency regression gate (hard):** re-run export golden, the 7 golden_frames, determinism, SST,
   and oracle_differential. All **byte-identical** (Part 2 predicts it by construction). Any diff is
   stop-and-investigate, not a re-baseline. Also re-run `export_state_captures_live_z80_ram` (writes `$A00000`,
   not `$A04000` — must still round-trip).
3. **Acceptance — Gunstar:** `boot_rom "…/Gunstar Heroes (USA).md" 1800` must boot **past** `$66360`, enable
   the display (`r01` bit 6 set), and render a **non-black** frame (throwaway-proven: PC → main loop `$450`,
   `r01 = $64`, 71 680/71 680 non-black). This is the confirming acceptance.
4. **Acceptance — TF4, diagnostic:** re-run TF4. If it now renders → DR-2 was also FM-gated, done. If still
   blank at `$0FF3xx` → its blocker is elsewhere (VDP-upload/render or a Z80-execution mailbox), routing it to
   the render thread or the full Z80 core. TF4's result refines the punch-list.

### Out of scope (named, not silent)

- **FM timer overflow flags (F5)** — status bits 0/1 stay `0` (no overflow); a consumer that *waits for* a
  timer overflow would hang, but no boot/render path does, and FM timers need the FM core. **Deferred.**
- **The broader Z80 memory map** — `$A06000` (bank register), `$A07F00–$A07F1F` (VDP mirror), `$A08000+` (bank
  window) are *also* wrongly mirrored as Z80 RAM today. DR-1b carves out **only** `$A04000–3` (the FM ports that
  block Gunstar). The rest is a separate Z80-map-correctness slice that lands with the Z80 core. **Deferred.**
- **The full Z80 / FM core** (execution, sound, real busy timing) — Z7. **Deferred.**

---

## Part 5 — Implementation outcome (2026-07-22, slice shipped for review)

Option B implemented per Part 4: `mapped_byte` gained `0xA0_4000..=0xA0_4003 => Some(0x00)` and `store_byte`
gained `0xA0_4000..=0xA0_4003 => {}`, both **before** the Z80-RAM arm. `bus.rs` only — no `system.rs`, no
export-state change.

**Gates:**
- **#1 semantics** — `fm_status_reads_not_busy_and_writes_do_not_alias_z80_ram` (bus.rs): all four ports read
  bit7 clear; a `$A04000 = $F3` write does **not** appear at `$A00000` (alias broken); `$A00000` Z80 RAM still
  round-trips. Watched it RED (FM write corrupted `$A00000`, read back `$F3`) then GREEN. Lib **596/596**.
- **#2 frozen currencies — byte-identical (currency-neutral by construction, confirmed):** export golden 3/3,
  7 golden_frames, determinism 2/2, oracle_differential 3/3, **SST 112/112**, plus
  `export_state_captures_live_z80_ram` ✓ (real Z80 RAM at `$A00000` still round-trips). fmt-clean, clippy-clean.
- **#3 Gunstar — RENDERS.** Boots past `$66360` to the main loop (`$450`), **display on** (`r01 = $64`),
  **fully non-black** (71 680/71 680), stable across 300/900/1800 frames. **DR-1 fully resolved (a + b).**
- **#4 TF4 — still blank** (PC `$0FF388/$0FF354`, the VDP-upload init loop; `r01 = $44`, 0 non-black). **Not
  FM-gated.** Its blocker is the render/DMA path or a Z80-execution mailbox — routes to the **DR-3 render
  thread or the full Z80 core**, not this family. TF4's result **picks the next rock**.

**Net:** DR-1 Gunstar is fully unblocked by two bounded bus-level slices (BUSREQ + FM-status), no Z80/FM core.
TF4 is confirmed a *different* root and moves to the render/Z80 investigation.

## Sources

- [Plutiedev — YM2612 registers](https://plutiedev.com/ym2612-registers) (address map, status bits, write sequence)
- Primary: Gunstar Heroes in-situ disassembly at `$66360` (user's ROM, D5); oracle-next's own committed gate
  tests (`export_state_v1.rs`, `golden_frames.rs`, `oracle_differential.rs`, `determinism_gate.rs`).
