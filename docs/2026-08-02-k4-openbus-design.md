# K4 — 68k open-bus model: recon + design (2026-08-02)

Status: **DESIGN — not yet implemented.** Slices K4-0..K4-5 below; K4-0 (instrument, zero src
changes) is a hard prerequisite before any behavior flips.
Origin: read-only recon pass over oracle-next + the Oracle C++ reference, adjudicated against the
memtest_68k ROM's inline real-hardware reference column (our pinned ground truth for this work).
Context: bug K4 from docs/2026-07-25-testrom-conformance.md (memtest_68k 4/13 rows).

## 0. Executive summary

The surprise of this recon: **we already have an open-bus latch, and it already carries
hardware-exact prefetch residue.** `MegaDriveBus` threads a `last_bus_word: u16` through every
access (`crates/oracle-core/src/bus.rs:258`, backed by `system.rs:86`), prefetch refills go through
`Bus68k::read16` (`m68000/microop.rs:2643-2658`) and therefore update that latch, and the memtest
row that is *pure* full-word open bus — `C00018-C0001F` expecting `4E71 4E71` — **already passes**
(one of the 4 passing rows). K4 is therefore not "add an open-bus latch"; it is "the latch's
*consumers* are wrong for everything that isn't the VDP region": partially-decoded arbiter
registers ($A11100/$A11200) return clean constants, the Z80 window is never gated, the I/O block
returns 0 for even bytes, and the VDP status word never mixes residue into its floating upper 6
bits. Disassembling the memtest ROM (byte/byte/word read triplets, NOP-padded) plus its inline
hardware column pins one new empirical rule the C++ reference does **not** model: **arbiter-answered
regions return the residue's high byte with the low byte driven to $00** (`4E00`, `4F00`), while
VDP-answered regions retain the full word (`4E71`). The whole fix is pure functions of state that
already exists (`last_bus_word`, `z80_busreq`, `z80_running`) — **no new System fields, no
export_state version bump** — and the four frozen currencies are structurally insulated (§5).

## 1. Current architecture survey

### How a 68k read flows today

- The CPU core talks to the `Bus68k` trait (`crates/oracle-core/src/m68000/bus68k.rs:49-64`):
  `read16/write16/read8/write8/tas`, each carrying the function code and returning wait cycles.
  The SST harness uses `FlatBus` (`bus68k.rs:68`), the machine uses `MegaDriveBus`.
- `MegaDriveBus` (`crates/oracle-core/src/bus.rs:251-306`) is a split-borrow adapter built per CPU
  step in `System::step_cpu` (`system.rs:795`, constructed at `system.rs:820`; also `system.rs:487`
  for the peek path). It borrows `last_bus_word: &'a mut u16` (`bus.rs:258`) alongside `z80_busreq`
  (`bus.rs:264`), `z80_running` (`bus.rs:272`), SRAM latches, VDP, IO, FM.
- Reads decode per **byte** through `mapped_byte` (`bus.rs:365-417`): `Some(byte)` = a device drove
  the lane, `None` = open bus. `read16` (`bus.rs:591-623`) assembles two mapped bytes, **latches
  the word** (`bus.rs:616-617`), or returns `*last_bus_word` unchanged on `None` (`bus.rs:619`).
  `read8` (`bus.rs:640-665`) returns the UDS/LDS half of the latch for open bus
  (`bus.rs:658-662`) and latches `byte * 0x0101` for mapped bytes — self-labeled "placeholder
  open-bus rule" (`bus.rs:656`). Writes latch the written value (`bus.rs:636`, `bus.rs:679`).
  VDP data-port reads already receive the latch as an `open_bus` argument (`bus.rs:506-507` →
  `vdp.data_read_at(open_bus, mclk)`) — the plumbing precedent for the status-read fix.

### What unmapped / partially-decoded reads return today

- `$400000-$7FFFFF` and every gap → `None` → full 16-bit latch (`bus.rs:414-415`).
  Hardware wants high-byte-only residue (`4E00`).
- `$A11100` → constant `0x00`/`0x01` full byte (`bus.rs:401`), odd byte constant 0 (`bus.rs:402`);
  formula is busreq-only. Hardware: `4F00` then `4E00` — bit 8 = "bus unavailable" (folding in
  Z80 RESET), bits 9-15 = residue, low byte $00.
- `$A11200` → constant readback of `z80_running` (`bus.rs:406-407`). Hardware: reads are
  **undriven** (always `4E00` regardless of the latch — the memtest row 9 reference does not change
  across the reset toggles, and the reference arbiter drives no lines on a reset-register read,
  `oracle/Devices/MD1600IO/MDBusArbiter.cpp:448-452`).
- `$A00000-$A0FFFF` → always Z80 RAM `& 0x1FFF` (`bus.rs:386`), never gated on bus ownership,
  word reads return two distinct bytes.
- `$A10000-$A1001F` even bytes → constant 0 (`bus.rs:396`). Hardware: `A0A0` — the register byte
  answers both bytes/lanes.
- `$C00004` status → `status_word` with bits 10-15 always 0 (`vdp.rs:376-400`, via
  `control_read_status` `vdp.rs:758`). Hardware: bits 10-15 float (residue).

### Prefetch tracking and layering — is plumbing needed?

**No new plumbing is needed.** The prefetch queue is `regs.prefetch: [u16; 2]`
(`m68000/registers.rs:33-34`); the `MicroOp::Prefetch` arm refills it with
`bus.read16(pc+4, fc_program)` (`microop.rs:2654`), which lands in `MegaDriveBus::read16` and
updates `last_bus_word` like any other word read. Because recipes order the IRC refill before the
operand read (the `EaCalc, Prefetch, …, Read` shape, `microop.rs:211`), the latch holds the *next
instruction's* word at operand-read time, exactly like hardware. **In-repo proof:** the
`C00018-C0001F` memtest row passes today with `4E71 4E71` — the NOP after the reading instruction,
i.e. hardware-exact residue through our existing order.

## 2. Reference behavior (Oracle C++ / Exodus port)

The reference models open bus as **per-bit tri-state retention**:

- Every device read replies with an `accessMask` = "which data lines I drove". The M68000 core
  holds `_lastReadBusData` and merges:
  `_lastReadBusData = (_lastReadBusData & ~mask) | (value & mask)` — byte, word, and long paths
  (`oracle/Devices/M68000/M68000.cpp:2113-2196`, comment at 2113 explains the tri-state rationale).
  Byte reads return the UDS/LDS half of the merged word (2140-2147). Unmapped addresses return a
  default result whose mask drives nothing (`oracle/System/BusInterface.cpp:2438-2444`), so the CPU
  keeps the full previous word.
- The bus arbiter declares exactly which bits it drives: Z80BUSREQ read drives **only bit 0** of
  its interface, `data.SetBit(0, reset || !busreq || !busgrant)` (`MDBusArbiter.cpp:442-447`) —
  note **reset is folded into the readable bit**; Z80RESET reads drive **nothing**
  (`MDBusArbiter.cpp:448-452`); the Z80 bankswitch reads return `0xFFFF` ("hardware tests",
  `MDBusArbiter.cpp:422-437`). The system XML maps the arbiter's bit onto data line 8
  (`DataLineMapping="[08]"`, `oracle/Data/Modules/Sega Mega Drive 1600.xml:314-316`) — i.e. the
  word-read BUSREQ bit is bit 8.
- The Z80 window: forwarded only when `!reset && busgrant` (`MDBusArbiter.cpp:482` write side;
  read side same gate), address masked to 15 bits, and **word reads mirror the single 8-bit result
  into both halves** (`MDBusArbiter.cpp:489-495` — "word-wide access to the Z80 memory space is
  not possible").
- VDP status read drives **only the low 10 lines** (`accessMask = StatusRegisterMask = 0x03FF`,
  `oracle/Devices/315-5313/S315-5313_Ports.cpp:1163-1170`, `IS315_5313.h:34`); unused VDP ports and
  the test register drive nothing (`S315-5313_Ports.cpp:1201-1211`).

**What the reference does NOT capture:** the memtest hardware column's low-byte-$00 pattern.
Exodus's unmapped/arbiter reads retain the full previous word, and its prefetch is a single-word
lookahead (admitted TODO, `oracle/Devices/M68000/M68000.h:21-22`), so it would print e.g. `4F71`,
not `4F00`, at `$A11100`. This design keeps Exodus's *architecture* (drive-mask merge) but pins
*values* to the ROM's inline hardware column.

**docs/reference/ confirmed 68000-only:** `docs/reference/README.md` lists only Yacht.txt and
M68000UM.pdf — no Mega Drive open-bus primary source in-tree. The memtest ROM's inline reference
column (plus the Exodus hardware-test comments above) is the ground truth for this work.

## 3. The 13 memtest rows, decoded

The ROM's read idiom per row (disassembled at file offset `0x34d0-0x3638`): `move.b addr,(a5)+` /
`nop` / `move.b addr+1,(a5)+` / `nop` / `move.w addr,(a5)+` / `nop`. Printed `A` = the two byte
reads packed `[even:odd]`, `B` = the word read. Residue at every operand read = the trailing NOP
(`$4E71`) via the IRC refill. Init (offset `0x200-0x232`): release Z80 reset (`$100→$A11200`),
assert BUSREQ (`$100→$A11100`, **held forever**), poll bit 0, then copy an 8 KiB Z80 program
(`F3 ED 56 31 …` at ROM `0x270`) into `$A00000+`. Rows 2/3 and 7/8 are separated by
`move.w #$0/#$100,$A11200` toggles (offsets `0x34e8`, `0x350a`, `0x3574`, `0x3596`).

| # | Row | HW ref (A B) | We pass? | Rule that produces the reference |
|---|---|---|---|---|
| 1 | `400000-7FFFFF` | `4E00 4E00` | ✗ (we give `4E71`-shaped) | **Arbiter open bus**: `(residue & 0xFF00)`; odd byte lane reads $00 |
| 2 | `A00000-A0FFFF` (Z80 reset **asserted**, busreq held) | `4E00 4E00` | ✗ | Z80 window **closed** when reset asserted → arbiter open bus |
| 3 | `A00000-A03FFF` (reset released) | `F3ED F3F3` | ✗ (we give `F3ED F3ED`) | Bytes already right; **word read duplicates the even byte** (`F3F3`) |
| 4 | `A04000-A05FFF` | `0000 0000` | ✓ | YM2612 status 0 (`bus.rs:384`) |
| 5 | `A06000-A07EFF` | `FFFF FFFF` | ✗ (we give Z80-RAM mirror) | Z80-side bank register / unused reads return `$FF` (`MDBusArbiter.cpp:422-437`) |
| 6 | `A10000-A1001F` | `A0A0 A0A0` | ✗ (we give `00A0`) | I/O regs ignore A0 (Exodus `AddressDiscardLowerBitCount="1"`, XML:312): even byte = same reg, word = byte duplicated |
| 7 | `A11100` (reset asserted) | `4F00 4F00` | ✗ | bit8 = `!(busreq && reset_released)` = 1, bits 9-15 = residue, low byte $00 |
| 8 | `A11100` (reset released) | `4E00 4E00` | ✗ | same, bit8 = 0 |
| 9 | `A11200` | `4E00 4E00` | ✗ | reads undriven → arbiter open bus (no reset readback) |
| 10 | `C00000-C00003` | `1122 1122` | ✓ | data-port pre-cache model |
| 11 | `C00004-C00007` | `4E88 4E88` | ✗ | status = `(residue & 0xFC00) \| (status & 0x03FF)`; `4E71&FC00=4C00`, status `0x288` (FIFO-empty+F+VB — our `status_word` bits already produce this) |
| 12 | `C00008-C0000F` | `C0?? C0??` | ✓ | HV counter fully driven |
| 13 | `C00018-C0001F` | `4E71 4E71` | ✓ | full-residue open bus — **already exact** |

The consistent split: rows answered by the **arbiter/IO side** show low byte $00 + high-byte
residue; rows answered by the **VDP** show full-word retention. A defensible physical story
(arbiter-answered cycles drive D0-D7 low, leave D9-15 floating, drive specific bits like BUSREQ on
D8) — but it is an *empirical* rule pinned to the ROM's column, not a documented mechanism (Q1).

## 4. Proposed design

**State:** none new. All rules are pure functions of `last_bus_word` + `z80_busreq` +
`z80_running`, all already threaded and bincode-snapshotted (`system.rs:86-98`). ⇒ **no
export_state layout change, no version bump** (rule: `docs/export-state-v1.md:107-116` — bump on
layout change only).

**Core change — replace the single `None → full latch` fallback with two open-bus flavors** in
`MegaDriveBus`:

```text
fn open_word(&self, a: u32) -> u16 {
    // VDP-answered region: full tri-state retention (row 13, already proven).
    // Arbiter/cart-time regions ($400000-$7FFFFF, $A10000-$A1FFFF gaps): hi-residue | 0x00.
}
```

`read16` open-bus arm returns `open_word(a)`; `read8` returns its UDS/LDS half (odd → $00 in
arbiter regions falls out for free). Per-register rules:

- `$A11100`: word = `(residue & 0xFE00) | (bit << 8)`, byte-even = `(residue_hi & !1) | bit`,
  byte-odd = `$00`, with `bit = !(z80_busreq && z80_running)` (adds the reset term, per
  `MDBusArbiter.cpp:444` + rows 7/8). Bit 0/8 remains driven, so real `btst #0` spins are
  untouched.
- `$A11200`: delete the readback arm (`bus.rs:406-407`) → arbiter open bus. The write latch
  (`bus.rs:454`) is unchanged.
- Z80 window `$A00000-$A0FFFF`: gate on `z80_busreq && z80_running`; closed → arbiter open bus
  (reads) / drop (writes). Open: byte access as today; **word read = even byte duplicated**;
  `$A06000-$A07EFF` → `$FF`. (Z80-side `$7F00` mirror = K2, explicitly out of scope.)
- `$A10000-$A1001F`: decode `io_reg(a | 1)`-style (mirror the odd register onto the even byte,
  incl. `$A10000` → version); word read = byte duplicated.
- VDP status: `control_read_status(open_bus: u16, mclk)` returning
  `(s & 0x03FF) | (open_bus & 0xFC00)` — same signature pattern as `data_read_at` (`bus.rs:506`,
  `vdp.rs:923`); the ~5 internal call sites (`vdp.rs:1358,1551,2048,2059`) pass 0
  (behavior-identical for those tests).
- Latch update on partially-driven reads: merge driven bits (Exodus's rule, `M68000.cpp:2138`) —
  equivalent to latching the returned merged word, so `read16`'s existing
  `*last_bus_word = value` works unchanged. Optionally fix the `b * 0x0101` byte-read smear
  (`bus.rs:656`) to a lane-merge — nearly unobservable (a prefetch intervenes before any
  subsequent open-bus read), do it for honesty and let the instrument confirm.

**What deliberately does NOT change:** ROM-past-end in `$000000-$3FFFFF` stays full-latch
(untested by memtest; pinned by the `rom_past_the_end_is_open_bus` unit test `bus.rs:952-964`);
TAS/IntAck paths; FlatBus/SST; Z80-side bus.

**Frozen-currency exposure — structurally near-zero, but verify:**

- `determinism_gate`: self-relative (two runs of the same build, `tests/determinism_gate.rs:37-46`)
  — cannot move on a deterministic change.
- `export_state_v1`: its fixture is `testrom::build()` (`tests/export_state_v1.rs:52-56`), which
  touches **only ROM vectors + work RAM** (`src/testrom.rs:77-126`) — no affected address is ever
  read ⇒ golden hash byte-identical.
- `golden_frames`: drives the `Vdp` API directly, no bus, no status reads
  (`tests/golden_frames.rs:1-40`).
- `oracle_differential`: hashes captured byte arrays only (`tests/oracle_differential.rs:1-12`).
- The **non-frozen** but pinned surfaces that WILL move, deliberately: the conformance `BASELINE`
  memtest row (`tests/conformance_roms.rs:100-104`) per slice, possibly other scorecard rows
  (games in the corpus that read `$C00004`/`$A11100`), and the `bus.rs` unit tests pinning the old
  constants (`bus.rs:1054-1135` — e.g. the busreq test must now release reset before expecting
  bit 0 = 0, which is exactly what real init code does).

## 5. Slice plan (each independently gated; instrument first)

- **Slice K4-0 — instrument, zero src changes.** A test-side `BusEventSink` (BusEvent already
  carries op/addr/size/value, `bus.rs:43-49`) that classifies "reads whose value would change under
  the K4 rules" ($400000-$7FFFFF; $A0 window while it *would* be closed — reconstruct busreq/reset
  from the write stream; $A10000-1F even/word; $A11100/01; $A11200/01; $C00004/6) and counts them
  per ROM across: the two gate fixture ROMs, the vendored conformance corpus, and the differential
  game corpus (the `docs/2026-07-22-differential-rom-findings.md` rig). Deliverable: a per-ROM hit
  table in the slice write-up. Expected: 0 hits for gate fixtures (proves currencies
  frozen-by-construction); nonzero for memtest and possibly `$C00004` in games (measure which bits
  games consume).
- **Slice K4-1 — arbiter open-bus flavor: `$400000-$7FFFFF` + `$A11200`.** Rows 1, 9 go green.
  Unit tests pin the `hi|00` shape; amend `BASELINE` + `docs/2026-07-25-testrom-conformance.md`
  (K4 section) same commit; run all four currency suites (expected unchanged).
- **Slice K4-2 — `$A11100` residue + reset-folded bit.** Rows 7, 8. Update the busreq unit test to
  release reset first (documented as the hardware-semantics change it is). Re-check game corpus
  boots (Gunstar DR-1 busreq spins) via K4-0 instrument + scorecard.
- **Slice K4-3 — Z80 window gating + word-duplication + `$A06000-$A07EFF = $FF`.** Rows 2, 3, 5.
  Highest game risk (sound drivers): K4-0 must show which corpus ROMs touch the window while
  closed before flipping; write-drop is gated behind the same check.
- **Slice K4-4 — I/O block byte mirroring.** Row 6.
- **Slice K4-5 — VDP status upper-6 merge.** Row 11. After this: expected **13/13**, one
  attributable `BASELINE` amendment per slice, `A00000-A0FFFF` row no longer blocked on K2.

## 6. Open questions / risks

1. **Mechanism of the low-byte-$00** (arbiter drives D0-D7 low vs. lane decay): unresolved from
   any in-tree source; Exodus models full retention instead. *Recommendation:* adopt the
   arbiter-drives-$00 rule pinned to the memtest column (our stated ground truth), document it as
   an experiment-pinned divergence from the reference, revisit only on contrary test-ROM evidence.
2. **Reset-folded BUSREQ bit could hang a game** that polls `$A11100` before ever releasing
   `$A11200`. Hardware (row 7) and Exodus both say the bit reads 1 then; correct games release
   first. *Recommendation:* K4-0 counts "A11100 read while `z80_running == false`" across the
   corpus before K4-2 lands.
3. **`tst.w $C00004` idioms**: bit 15 becomes residue-dependent after K4-5; games moving the
   status word through flags could branch differently. *Recommendation:* K4-0 counts status reads
   per game; if a boot-path game misbehaves, that is a real-hardware-faithful behavior to debug,
   not to mask — but land K4-5 last for bisectability.
4. **Z80-window word *writes*** (Exodus TODO says only one byte lands,
   `MDBusArbiter.cpp:496-501`): untested by memtest. *Recommendation:* defer; instrument for word
   writes to `$A0xxxx` in the corpus first.
5. **Board-revision stability of the reference values** (VA0-VA7): unknowable offline.
   *Recommendation:* treat the vendored ROM's column as the pinned ground truth per the ledger's
   charter; note in the ledger.
6. **Extrapolating the arbiter rule to untested gaps** (`$A10020-$A10FFF`, `$A11000`, `$A130xx`
   reads, `$A14000`): *Recommendation:* apply only where evidenced in slices; flag the
   region-wide extrapolation as a follow-up decision.
