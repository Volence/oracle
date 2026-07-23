# Cartridge SRAM / battery-backed save — design recon (READ-ONLY)

**Date:** 2026-07-23
**Status:** recon complete → **APPROVED as design of record.** Every technical claim carries a `file:line` citation.
**Frontier ref:** `backlog-gameplay-accuracy` (owner's pick #1 = SRAM/battery save, "currency-aware design needed").

## APPROVAL / DECISIONS LOCKED (2026-07-23)

- **Overseer independently verified** the load-bearing citations before approval: `state_hash.rs:44` (VDP-only,
  four regions), `system.rs:310-365` (export_state tail = FM+PSG, no SRAM reserve), `export-state-v1.md:104-114`
  (region-add ⇒ version bump), `bus.rs` ROM decode of `$200000-$3FFFFF` + absent `$A130xx` + the Z80
  `busreq`/`running` even-byte latch precedent, and **Oracle's `OpStateHash`** hashing exactly VRAM/CRAM/VSRAM/regs
  (`oracle/linux-port/gui/ControlSocket.cpp:2340-2355`). All confirmed.
- **Owner decision 1 — validation:** build + unit-test the mechanism now (directed SRAM write/read/persist/reload
  tests); real save-cart end-to-end validation deferred until the owner supplies a ROM. No save-game ROM is
  vendored in-tree, so open question 2 stays parked — S0–S3 proceed on directed tests.
- **Owner decision 2 — go-live golden:** **authorized to proceed through S3**, including the single attributable
  `export_state` v1→v2 bump + one golden regen, with full independent overseer verification at that slice.
- **Forks ratified:** Fork 1 = hybrid detection (header RA + `$200000+` odd-byte fallback + map-nothing when
  neither RA nor `$A130F1` activity → existing goldens untouched); Fork 2 = OUT of `state_hash` (forced); Fork 3 =
  IN `export_state` as a fixed 64 KiB reserved tail at S3; Fork 4 = dirty-throttled autosave + save-on-quit
  (frontend); Fork 5 = odd-byte wiring, `sram[(a-base)>>1]`.
- **Execution:** overseer dispatches one worker per slice → independently verifies (fmt, clippy, the 5 currency
  goldens, m68000 zero-diff) → commits (no co-author trailer) → pushes when green. Open question 1 (`$A130F1` bit
  semantics) is pinned from a reference as the first task of S0.

SRAM is currently **entirely absent** from the core. A workspace-wide grep for
`sram|SRAM|A130|battery|.srm|save_ram|backupram` returns **zero** hits in either
`crates/oracle-core/src` or `crates/oracle-frontend/src` (only unrelated `vsram`,
`ym2612::TIMER_*`, and a VGM `header` matched). This is a greenfield feature.

---

## The currency verdict (the crux — read this first)

**SRAM must stay OUT of `state_hash`, and enters `export_state` only at a single deliberate,
attributable go-live slice that bumps `EXPORT_STATE_VERSION` to 2.**

Reasoning, all verified:

1. **`state_hash` is VDP-only, and that is a hard cross-emulator contract.** Ours hashes exactly
   VRAM → CRAM → VSRAM → the 24 VDP registers (`crates/oracle-core/src/state_hash.rs:44-78`), and
   Oracle's C++ reference `OpStateHash` hashes the identical four regions and nothing else
   (`oracle/linux-port/gui/ControlSocket.cpp:2340-2355` — it reads only `GetVRAMBuffer`/`GetCRAMBuffer`/
   `GetVSRAMBuffer`/`GetRegisterData`). SRAM is **not** in Oracle's hash. Therefore keeping the live-Oracle
   A/B differential (`crates/oracle-core/tests/oracle_differential.rs`) byte-identical **requires** SRAM
   stay out of `state_hash`. This is not a judgment call; including it would break parity by construction.

2. **`export_state` has no SRAM reserve, so adding SRAM is a layout change → version bump.** The frozen v1
   image is `version → m68k regs → work RAM → Z80 RAM → Z80 regs(reserved) → VDP → FM(reserved) →
   PSG(reserved)` (`crates/oracle-core/src/system.rs:310-365`; spec `docs/export-state-v1.md:25-36`). There
   is no reserved region sized for cartridge SRAM. The version-bump rule
   (`docs/export-state-v1.md:104-114`) is explicit: bump **iff a region is added, removed, reordered, or
   resized**. Adding SRAM adds a region → **bump to v2** and regenerate `export_state_v1.rs`'s golden in the
   same commit. (Contrast: the Z80-regs and VDP reserves went live at *unchanged size* = content change, no
   bump — SRAM has no such pre-carved slot to fill.)

3. **Only one currency golden must move; everything else stays byte-identical.** See §B7 table. The single
   attributable regen is `export_state_v1.rs` at the go-live slice — the exact precedent set by the Z80
   region-4 go-live ("one attributable golden regen").

One paragraph, plain: *The emulator guards three "currencies" with golden tests — determinism, a serialized
state snapshot (`export_state`), and a VDP-only fingerprint (`state_hash`) that must match the reference C++
emulator byte-for-byte. SRAM is persistent cartridge memory, so unlike the sound work it does touch state.
The safe plan is: SRAM never goes into the VDP fingerprint (the reference doesn't hash it, so matching it
forbids us from hashing it either); and SRAM enters the state snapshot only once, in a clearly-labelled
"go-live" commit that ticks the snapshot version from 1 to 2 and regenerates exactly one golden. Every other
golden — determinism, render frames, the Oracle differential, snapshot round-trips — stays untouched.*

---

## A. Memory-map / bus mechanics

### A1 — Address decode today; where SRAM lives

The 68000 map is decoded in `MegaDriveBus::mapped_byte` (`crates/oracle-core/src/bus.rs:302-347`) and its
write twin `store_byte` (`bus.rs:352-380`). The relevant arm:

- `0x00_0000..=0x3F_FFFF => ROM` (`bus.rs:305-308`) — **the entire first 4 MiB is ROM**, read-only, with
  reads past a short ROM's end returning open bus (`bus.rs:307`). Writes to this range are silently dropped
  (there is no `0x00_0000..` arm in `store_byte`, so it falls to `_ => {}` at `bus.rs:378`).

**Consequence:** the standard Genesis SRAM window **`$200000-$3FFFFF` overlaps the ROM region** and is
currently 100% decoded as ROM. On real carts SRAM is mapped into the upper part of this window *behind a
bank/enable latch* (`$A130F1`), overlaying ROM only while enabled. Adding SRAM means splitting the
`0x00_0000..=0x3F_FFFF` arm so that, **when SRAM is enabled and the address is in the cart's SRAM range**,
reads/writes hit the SRAM buffer instead of ROM.

### A2 — `$A130F1` (the TIME / bank-enable register) — ABSENT

There is **no** `$A130xx` handling anywhere. `$A130F1` falls through `mapped_byte`'s final `_ => None`
(`bus.rs:345`, open bus on read) and `store_byte`'s final `_ => {}` (`bus.rs:378`, dropped on write). Real
carts use `$A130F1` bit0 = SRAM-enable and bit1 = write-protect; the enable bit gates whether the
`$200000+` window shows SRAM or ROM.

**Precedent to model it on:** the Z80 BUSREQ (`$A11100`) and RESET (`$A11200`) latches were promoted from
drop-stubs to real one-bit latches threaded as `&mut bool` through the bus. Read side:
`bus.rs:331-337`; write side (even-byte-only latch): `bus.rs:363-367`; the backing fields + their
"not in `export_state`" doc comments: `crates/oracle-core/src/system.rs:77-88`; the split-borrow wiring:
`system.rs:250-269`. `$A130F1`'s enable/write-protect bits should be latched the same way — a small scalar
threaded through `MegaDriveBus`, **not** in `export_state`/`state_hash` (a bus-arbitration-class scalar,
exactly like `z80_busreq`).

### A3 — ROM storage / header parsing — no header is read

The ROM is a plain `System.rom: Vec<u8>` (`system.rs:64`), loaded raw by `System::load_rom(rom: Vec<u8>)`
(`system.rs:225-227`), read-only via `System::rom()` (`system.rs:230-232`). There is **no cartridge struct
and no header parser** — grep for `header|Cartridge|0x1B0|0x1B8` in `src/` matches only VGM/comment text.
The CPU reads the reset vectors ($0/$4) straight from ROM bytes, but **no code reads the SRAM header fields**
(`$1B0-1` "RA" magic + type byte, `$1B4-7` SRAM start, `$1B8-B` SRAM end). So a header-driven SRAM detection
(Fork 1a/1c) would be brand-new parsing logic, cleanly addable in `load_rom` or a small `cartridge` module.

The test fixture ROM (`crates/oracle-core/src/testrom.rs:1-45`) is a hand-authored 0x300-byte image with no
"RA" field — relevant because it is what the currency goldens boot (§B7).

### A4 — Odd/even byte mapping

The bus assembles words byte-by-byte from `mapped_byte(a)` + `mapped_byte(a+1)` (`bus.rs:512-517` for
`read16`) and scatters word writes to `store_byte(a, hi)` + `store_byte(a+1, lo)` (`bus.rs:534-535`). Byte
reads already distinguish even (UDS/high half) from odd (LDS/low half) — `bus.rs:559-562`. Real 8-bit
Genesis SRAM is wired to **only odd bytes** (occasionally only even), so the chip appears at every *other*
bus byte and its N bytes span a 2N address range. The natural model: give SRAM its own byte-addressed arm in
`mapped_byte`/`store_byte` that maps bus address → `sram[(a - sram_base) >> 1]` for the odd-byte wiring
(and returns open-bus/ROM for the unused parity). This is a per-byte concern and lives entirely inside those
two functions — the word/byte plumbing above needs no change.

---

## B. Currency interaction (exhaustive)

### B5 — Would `state_hash` change? Does Oracle include SRAM? → NO / NO

`state_hash` is computed only from VDP memory + registers (`state_hash.rs:44-78`); the `System` wrapper feeds
it `vdp.vram()/cram()/vsram()/regs()` (`system.rs:293-296` region). Adding a `System.sram` field does **not**
touch `state_hash` unless we deliberately add it to `StateHash::compute` — which we must **not** do, because
Oracle's `OpStateHash` excludes SRAM (`ControlSocket.cpp:2340-2355`). **Verdict: SRAM stays out of
`state_hash`; the Oracle A/B differential is unaffected and must stay byte-identical.**

### B6 — `export_state` layout, reserves, and how SRAM is added

Current layout (`system.rs:310-365`, spec `docs/export-state-v1.md:25-36`):

| # | Region | Size | Note |
|---|--------|------|------|
| 0 | version `u16` LE | 2 | `EXPORT_STATE_VERSION = 1` (`system.rs:37`) |
| 1 | m68k regs | 78 | `system.rs:41` |
| 2 | work RAM | 0x10000 | live |
| 3 | Z80 RAM | 0x2000 | live |
| 4 | Z80 regs | 0x40 | went live at unchanged size (`system.rs:42-45`) |
| 5 | VDP | 0x100E8 | went live at unchanged size |
| 6 | FM (reserved) | 0x200 | all-zero (`system.rs:46-50`) |
| 7 | PSG (reserved) | 0x10 | all-zero, tail (`system.rs:51-53`) |

There is **no SRAM reserve**. Adding SRAM (whether appended after PSG or inserted) is a region addition →
**layout change → `EXPORT_STATE_VERSION` bumps to 2**, and the golden constants in `export_state_v1.rs` are
regenerated **in the same commit** (rule: `docs/export-state-v1.md:104-114`). This is *not* the
content-change path the Z80/VDP reserves used — those filled pre-carved zeroed bytes at unchanged size
(`system.rs:353-355`, `docs/export-state-v1.md:69-71`); SRAM has no such slot.

Design choice for the region: SRAM size is cart-dependent (2–64 KiB). To keep the layout fixed I recommend a
**fixed 0x10000 (64 KiB) reserved tail region** appended after PSG (the max standard SRAM size), holding the
live SRAM bytes left-justified and zero-padded — one bump buys a stable v2 layout regardless of the specific
cart's SRAM size. Placing it at the **tail** means any future resize churns no other offset (the same
rationale FM/PSG cite, `system.rs:49/52`).

### B7 — Per-golden impact prediction

| Test | Currency used | Impact |
|------|---------------|--------|
| `oracle_differential.rs` | `state_hash` (VDP-only), absolute Oracle hashes (`:35-56`) | **Must stay byte-identical.** SRAM out of `state_hash` ⇒ untouched. |
| `state_hash.rs` unit goldens | `state_hash` (`:108-136`) | **Byte-identical** — SRAM never enters `compute`. |
| `golden_frames.rs` | VDP render output only | **Byte-identical** — no SRAM path. |
| `determinism_gate.rs` | `export_state_hash`, but **relative** (compares two fresh instances `a == b`, no absolute golden — `:26-40`) | **Stays green** at both skeleton and go-live: adding a deterministic SRAM field keeps instances identical. No regen. |
| `proptests.rs` | `export_state_hash`, **relative** (bulk==stepwise `:38`, snapshot round-trip `:46`) | **Stays green** — relative comparisons; a normal bincode field round-trips. No regen. |
| `export_state_v1.rs` | `export_state` + **absolute** `GOLDEN_HASH` + every offset literal (`:20-45`) | **THE one attributable regen.** At the go-live slice only: bump to v2, add region offset/size literals, regen `GOLDEN_HASH`. Skeleton slices (SRAM not yet in `export_state`) leave it byte-identical. |
| `io_controllers.rs`, `singlestep_*`, `watchpoints.rs` | none of the above | **Untouched.** |

**Snapshot/restore (`bincode`):** `System` derives `Encode/Decode` (`system.rs:56`); `snapshot`/`restore`
are self-describing bincode with **no golden blob on disk** (find for `*.bin`/`*.snapshot` under
`crates/oracle-core` returns none). Adding `sram: Vec<u8>` as a normal field round-trips automatically; the
relative round-trip tests (`system.rs:1217+`, `proptests.rs:46`) stay green. SRAM **should** ride the bincode
snapshot (it is real mutable state that must survive save-states/determinism), exactly as `z80_ram` does.

---

## C. Persistence architecture

### C8 — The frontend seam

The frontend owns all non-determinism and constructs the machine in `main`:
`std::fs::read(rom_path)` (`crates/oracle-frontend/src/main.rs:321`) → `System::new(0x5EED)`
(`main.rs:330`) → `sys.load_rom(rom)` (`main.rs:331`) → `sys.reset()` (`main.rs:332`) → per-frame run loop
(`main.rs:376+`). Quit is the `while window.is_open() && !Escape` exit (`main.rs:376`), with an existing
"on quit, flush" comment at `main.rs:495`.

**Load-SRAM-on-boot seam:** immediately after `load_rom`/before `reset` (`main.rs:331-332`), read
`<rom>.srm` next to the ROM (derive from `args.rom_path`) and call `sys.load_sram(&bytes)` if present.
**Save-SRAM seam:** in the run loop after each `run_frames(1)`, check `sys.sram_dirty()` and/or write once at
the quit path (`main.rs:495`). This mirrors how audio was added as a frontend-only concern
(`main.rs:361-362`) — zero core determinism surface.

### C9 — Minimal core API surface

Propose (all on `System`, mirroring `rom()`/`load_rom()` at `system.rs:225-232`):

- `fn load_sram(&mut self, bytes: &[u8])` — copy a `.srm` into the SRAM buffer on boot (truncate/zero-pad to
  the region size).
- `fn sram(&self) -> &[u8]` — the live SRAM bytes for the frontend to persist.
- `fn sram_dirty(&self) -> bool` + `fn clear_sram_dirty(&mut self)` — a throttle signal so the frontend
  writes only after a guest write, not every frame. (Dirty flag is a non-currency scalar, like
  `z80_frontier_mclk` — in the bincode snapshot for determinism, out of `export_state`/`state_hash`.)

That is the whole seam. The frontend does all file I/O; the core never touches the filesystem (preserving its
"deterministic, no-I/O" contract, `main.rs:7-9`).

---

## D. Forks (each with a recommendation)

### Fork 1 — SRAM presence / mapping detection
- (a) trust ROM header "RA" field; (b) always provide a standard `$200000+` window; (c) hybrid: header if
  the "RA" magic + range are valid, else fall back to a standard window.
- **Recommend (c) hybrid.** Parse `$1B0-1` "RA" magic, `$1B4-7` start, `$1B8-B` end (new logic in `load_rom`
  or a `cartridge` module — none exists today, §A3). If the magic is present and the range is sane, honor it
  (correct odd/even parity from the type byte at `$1B1`). Homebrew and mis-authored headers are common
  (§A3 notes even the vendored ROM has no "RA"), so fall back to a standard 64 KiB odd-byte window at the top
  of `$200000-$3FFFFF` when the header is absent/garbage. **Do not** map SRAM at all for a cart with no "RA"
  *and* no `$A130F1` activity — leave it pure ROM (currency-neutral for every existing golden ROM).

### Fork 2 — `state_hash` inclusion
- **Recommend OUT, non-negotiable.** Oracle's `OpStateHash` excludes SRAM (`ControlSocket.cpp:2340-2355`);
  including it would break the byte-for-byte A/B parity that `oracle_differential.rs` guards (§B5).

### Fork 3 — `export_state` inclusion + versioning
- **Recommend: IN, as a fixed 0x10000 reserved tail region, gated behind a single go-live slice that bumps
  to v2.** SRAM is architectural persistent state and belongs in the cross-backend currency, but it has no
  pre-carved reserve, so it is a genuine layout change (§B6). The golden in `export_state_v1.rs` regenerates
  **exactly once**, at that go-live slice — the "one attributable golden regen" precedent. Until then, SRAM
  lives only in the bincode snapshot (determinism-safe) and every currency golden stays byte-identical.

### Fork 4 — persistence trigger (frontend-only)
- **Recommend both: dirty-throttled autosave + save-on-quit.** Poll `sram_dirty()` in the run loop and
  debounce (e.g. write at most once per N frames when dirty), and always flush at the quit path
  (`main.rs:495`). This survives a crash mid-session yet avoids a per-frame file write. Pure frontend; no
  core determinism impact.

### Fork 5 — odd/even & size modelling
- **Recommend: model the odd-byte (default) wiring from the header type byte, buffer sized to the header
  range (2/8/32/64 KiB), stored in a fixed 64 KiB `Vec<u8>` region.** In `mapped_byte`/`store_byte`, an
  enabled SRAM access maps bus addr → `sram[(a - base) >> 1]` for odd-byte carts (§A4); the unused parity and
  the disabled state fall through to the existing ROM/open-bus arms. Word-wide plumbing is untouched.

---

## Open questions (could NOT verify — do not guess)

1. **`$A130F1` bit semantics beyond enable/write-protect for real target ROMs.** The bus has no `$A130xx`
   handling to check against (§A2); the exact enable protocol some carts use (e.g. write `$01` to enable)
   should be pinned from a reference (jgenesis/BlastEm or the ROM's own driver) before implementing — same
   discipline as the Z80 BUSREQ recon.
2. **Which target ROM the owner wants first** (Phantasy Star, Shining Force, Sonic 3 save…). SRAM range,
   size, and odd/even parity differ per cart; the first bring-up ROM determines the concrete header values to
   test against. No such ROM is vendored in-tree (the fixture ROM has no "RA" field, §A3).
3. **Whether BlastEm-over-RSP (the future `oracle-bus` differential) can read SRAM** for cross-backend
   compare. `export_state`'s Z80-RAM region notes RSP reads it via the 68k window
   (`docs/export-state-v1.md:58-61`); whether SRAM is similarly reachable over RSP `m` at `$200000+` when
   enabled is unverified and affects whether the v2 SRAM region is differential-comparable or
   determinism-only.
4. **EEPROM/serial-save carts** (a different protocol at `$200000`/`$A130F1` than parallel SRAM). Out of
   scope for the first slice; flagged so the header parser (Fork 1) does not misclassify an EEPROM cart as
   parallel SRAM.

---

## Proposed SLICE LADDER (currency-safe-first, mirroring prior phases)

1. **S0 — `$A130F1` latch (skeleton, currency-neutral).** Promote `$A130F1` from drop-stub to a real
   enable/write-protect latch threaded through `MegaDriveBus` (model: Z80 BUSREQ, §A2). No SRAM buffer yet;
   reads/writes to `$200000+` still hit ROM. **Every currency golden byte-identical** (no golden touches
   `$A130F1`). Pin the bit semantics from a reference first (open question 1).
2. **S1 — header "RA" parse + SRAM buffer, bincode-only (currency-neutral).** Add `System.sram: Vec<u8>` +
   `sram_dirty`, parse the header (Fork 1c), map enabled SRAM in `mapped_byte`/`store_byte` with odd-byte
   wiring (Fork 5). SRAM rides the **bincode snapshot** but is **NOT** in `export_state` or `state_hash`.
   Determinism gate + proptests stay green (relative); `export_state_v1.rs` byte-identical (SRAM not yet in
   the image); Oracle differential byte-identical. This is the "game can save within a session" slice.
3. **S2 — core API + frontend persistence (frontend-only).** Add `load_sram`/`sram`/`sram_dirty` (§C9); wire
   `.srm` load-on-boot + dirty-throttled/quit autosave in `oracle-frontend` (§C8, Fork 4). **Zero core
   currency surface** — pure frontend, like the audio slices. Saves now survive across launches.
4. **S3 — `export_state` go-live (THE one attributable currency slice).** Add the fixed 64 KiB SRAM tail
   region to `export_state`, **bump `EXPORT_STATE_VERSION` → 2**, and regenerate `export_state_v1.rs`'s
   golden + offset literals in the **same commit** (Fork 3). This is the single deliberate golden regen of
   the whole feature; all other goldens remain byte-identical. Gate it behind reference confirmation that
   SRAM is cross-backend comparable (open question 3) — if not, SRAM can legitimately stay bincode-only and
   S3 is deferred.
