# Test-ROM conformance ledger

**The ROM-level analogue of `tools/blastem-differential/known_differences.py` and
`docs/2026-07-16-vdp-pixel-known-differences.md`.** `crates/oracle-core/tests/conformance_roms.rs` boots a
vendored corpus of well-known Mega Drive test ROMs (`tools/fetch-testroms.sh`) headlessly, scrapes each
ROM's own verdict, and compares the whole scorecard against a pinned baseline in one `assert_eq!`.

**This instrument is NON-GATING.** Per `CHARTER.md`, the launch target is *MVP-debuggable*, **not** "passes
VDPFIFOTesting" — accuracy is an asymptote, not a day-one milestone. The harness therefore fails on a
**regression from the recorded baseline**, never on "all must pass". Several ROMs fail today for reasons
enumerated below; those failures are *recorded*, not fixed. A scorecard diff is **information**: confirm the
change is intended, then amend `BASELINE` and this ledger in the same change. Never regen silently.

Harness facts, so a reader can reproduce a row by hand:

- Power-on seed `0x12345678`; `System::new(seed)` → `load_rom` → `reset` → `run_frames(n)`; pads injected
  after `reset` via `set_pad`.
- Text ROMs use an ASCII-ordered font: a plane-A nametable cell's low 11 bits are `font_base + ASCII`, and
  plane A's base is `(R2 & $38) << 10`. The font base is **hardcoded per ROM** in the harness, never
  auto-detected, so a decode change garbles the text (a visible regression) instead of silently re-deriving.
- `frame_hash` = FNV-1a over the whole 224-line framebuffer (the `tests/golden_frames.rs` idiom). It is a
  **self-consistency pin only**: it says "the picture did not change", never "the picture is right".
- Frame budgets are per-ROM settle times found empirically; they are generous, not tight.
- `crates/oracle-core/examples/testrom_probe.rs` is the recon tool used to derive every row here (plane text
  dump, framebuffer ASCII dump, tile dump, verdict-block hashes, scripted pad presses).

## Scorecard (recorded 2026-07-25)

| ROM (local name) | What it tests | Automatable? | Recorded result | Expected-fail reason / reference |
|---|---|---|---|---|
| `m68k_bcd` (`bcd-verifier-u1`) | Exhaustive ABCD / SBCD / NBCD value **and** flag verification, including the undefined-flag cases | **Yes** — three scraped text rows, font base `$000`, 700 frames | **PASS** — `abcd`/`sbcd`/`nbcd` all `$00000 $00000` (0 value errors, 0 flag errors) | — (corroborates the SST BCD coverage on real silicon-derived vectors) |
| `io_sample` (`Multitap - IO Sample Program`) | Controller-port device detection (the TH-handshake ID protocol) on both ports | **Yes** — both ports must print `JOYPAD`, font base `$000`, 160 frames | **PASS** — port1 = `JOYPAD`, port2 = `JOYPAD` | — (matches `docs/2026-07-17-io-recon.md` IO1–IO6) |
| `m68k_illegal` (`itest`) | Illegal / privileged / unimplemented encodings must trap to the right vector | **Yes** — no text; verdict is the backdrop word `CRAM[0..2]`: blue `$0E00` while running, `$00E0` green = pass, `$000E` red = fail. 20 frames (the full sweep settles ~frame 9) | **PASS** — backdrop `$00E0` (green) since the 2026-08-02 K1 fix (was FAIL `$000E`) | **K1 — RESOLVED & FIXED.** See below. |
| `m68k_memory_test` (`memtest_68k`) | Reads every non-lockup address range twice; prints what it read **and** the ROM's own built-in real-hardware reference (`?` = wildcard nibble) | **Yes** — per-row compare, font base `$100`, 30 frames | **13 / 13 rows match** since the 2026-08-02 status-low-byte fixes (K4 slices took it 4/13 → 12/13; the row-11 residual was ODD-outside-interlace + VBlank-forced-while-display-disabled, both reference-corroborated semantics bugs — see the K4 ledger addendum) | **K4 — RESOLVED & FIXED**; row-11 status low byte also RESOLVED (adjudicated semantics, not timing — see below). |
| `vdp_port_access` (`VDPFIFOTesting`) | 16 VDP port-access tests over two pages: FIFO size/behaviour, DMA-via-FIFO, byteswapping, partial CP writes, register-write masking, read-target switching, FIFO wait states | **Yes** — scrapes the on-screen `Results: ( P/ F/ T)` line; page 1 auto-runs (settles ~frame 42), `Start` advances to page 2 (~480 more frames) | **page 1 = 7 pass / 2 fail / 9**; **pages 1+2 cumulative = 12 pass / 4 fail / 16** (was 9/7/16 — slice A2 flipped T13 and T10, slice A3a flipped T3; Test 16's byte matrix had already improved 54→18 red cells with the A1 live FIFO flags — see notes below) | The harness remains non-gating on this ROM (the CHARTER line stands, and `conformance_roms.rs`'s header still states it); what changed on 2026-08-03 is that the owner asked for this ROM's rows to be worked, so they are now being fixed slice-by-slice (A1 = live FIFO EMPTY/FULL flags; A2 = control-port / code-register edges; A3a = DMA payload words through the FIFO ring, all 2026-08-03). Per-test today — page 1: T1 T2 T3 T5 T7 T8 T9 pass, **T4 T6 fail**; page 2: T10 T11 T13 T14 T15 pass, **T12 T16 fail**. **A3a flipped T3 "DMA Transfer using FIFO"** (see the A3a note below); **T4 "DMA Fill FIFO Usage"** is a named residual whose fix (A3b) is designed and evidenced but **PARKED for an owner ruling** because it moves `export_state_v1::GOLDEN_HASH` — see `docs/2026-08-03-a3-dma-fifo-design.md` §4.1 and `docs/plans/2026-08-03-PARKED-owner-ruling.md`. **A2 flipped T13 "Register Writes and Code Reg" and T10 "Partial CP Writes"** (see the A2 note below), and left **T12 "Register Write Mode4 Mask"** as a named residual: in Mode 4 (reg 1 bit 2 = M5 clear) only the eleven SMS registers 0–10 are writable, and the one-line fix in `Vdp::write_register` **moves `export_state_v1::GOLDEN_HASH` and the `golden_frames` scenes**, because nearly every fixture here — `testrom.rs`'s golden ROM included, its `reg 1 = $50` leaving M5 clear — programs registers 11+ while still in Mode 4. Held for an owner ruling; the spec is pinned as an `#[ignore]`d unit test (`vdp::tests::mode4_ignores_register_writes_above_ten`). **T16 "FIFO Wait States" remains FAIL but is now 62/80 verdict bytes green (was 26/80):** all 10 groups' first-probe word matches (FULL `$0100` / EMPTY `$0200` / partial `$0000` per each group's config), as do the drained-state probes in groups 1–8. Remaining reds: groups 9–10's stream probes need DMA words to occupy the FIFO as **pending** entries (the ROM triggers a 68k→VRAM DMA mid-group and expects the stream to see FULL/partial) — **slice A3a landed DMA-through-FIFO ring *contents* and left T16 unmoved at 62/80**, confirming these groups need *occupancy*, which our synchronous DMA model deliberately does not produce (design Q1; see the A3a note below) — and groups 2/3/5/6/8's stream-FULL probe needs **discrete per-line access-slot scheduling** (the write-6 stall phase-locks the CPU to the drain clock; with our uniform slot spacing — an H40 VRAM word costs 380 mclk, i.e. 2 slots of 190 mclk each — the post-insert probe deterministically lands just past a drain boundary every retry, where hardware's irregular slot gaps let it still catch FULL) — the "Phase 3 per-line DMA cost" deferral, not flag mechanics. |
| `vdp_sprite_masking` (`SpriteMaskingTestRom`) | 9 sprite masking / per-line / per-frame / dot-overflow tests | **Yes** — verdict is a 32×8 glyph at the right edge, classified by **rendered-pixel hash** (its nametable cells are identical for the tick and cross cases, so only the framebuffer discriminates). 300 frames, settles ~frame 8 | `1=TICK/TICK 2=TICK/TICK 3=TICK/CROSS 4=PASS 5=PASS 6=FAIL 7=PASS 8=PASS 9=TICK/TICK` — **2 failures**: test 3's second sub-case (MAX SPRITE DOTS – COMPLEX) and test 6 (MASK S1 ON DOT OVERFLOW) | Expected: both are the **mid-sprite pixel-budget cut** interim model, ledger row **P1** in `docs/2026-07-16-vdp-pixel-known-differences.md` (we spend budget per whole sprite; hardware cuts mid-sprite at the exact dot). Open question **Q1** below on the H32/H40 toggle. |
| `color_1536` (`TEST1536.BIN`) | 1536-colour trick — CRAM rewritten mid-scanline | Frame hash only — **per-scanline capture** (the only row that uses it) | `frame_hash=0x917371f07409cb25` (per-scanline capture; was `0x96b9c93c4f3dd325` end-of-frame, re-pinned 2026-08-03) | **Limitation L1 — NARROWED**: the row now hashes the picture the ROM actually draws. Still a regression pin, not a verdict (the ROM prints none). |
| `cram_flicker` (`cram flicker.bin`) | The artefact from writing CRAM during active display | **NOT-RENDERABLE** — the artefact is the write itself appearing at the beam position, which we do not model | `frame_hash=0x815bb645bc46a325` (unchanged; re-adjudicated 2026-08-03) | Structural: **CRAM-write artefact, sub-scanline** — reason narrowed from "border-only rendering", which the evidence disproved (see **L1a**). Follow-up **F-CRAMDOT**. |
| `direct_color_dma` (`Direct-Color-DMA.bin`) | Direct-colour DMA — CRAM streamed per pixel during active display | **NOT-RENDERABLE** — sub-scanline CRAM | `frame_hash=0xed40dc4a6c4fc325` (unchanged; re-adjudicated 2026-08-03) | Structural: needs a per-pixel CRAM timeline — writes carry no h-position, and the mclk is instruction-granular (see **L1b**). Follow-up **F-CRAMDOT**. |
| `shadow_highlight` (`Shadow-Highlight Test Program #2`) | Shadow / highlight operator output | Frame hash only (visual judgment) | `frame_hash=0x428e03aa61cc0285` | Ledger row **P8** (S/H DAC calibration) governs any pixel-level divergence. |
| `window_test` (`Window Test by Fonzie`) | Window plane placement / clipping | Frame hash only | `frame_hash=0x4efcfda475af0d12` | — |
| `window_distortion` (`Window distortion bug.BIN`) | The hardware window-plane distortion bug (left window + fine h-scroll) | Frame hash only | `frame_hash=0x5102219d295b4e2c` | Ledger row **P5** (R9 window-bug sub-tile alignment) governs the exact reused-tile offset. |
| `vdp_test_register` (`DisableRegTestROM.bin`) | The undocumented VDP test register (`$1F`) display-disable bits | Frame hash only | `frame_hash=0x4a49cfea306e928b` | We do not model the test register; the hash is a "did the picture move" pin, not a verdict. |
| `gfx_joystick` (`Graphics & Joystick Sampler`) | Basic graphics + joystick sampler (a smoke ROM) | Frame hash only | `frame_hash=0xba8ce06075e4e163` | — |
| `fm_test` (`FM Test by DevSter`) | YM2612 FM tone output | Frame hash only — **audio, not video**; the harness pins the (blank) screen so the ROM at least boots and runs | `frame_hash=0xed40dc4a6c4fc325` (identical to `direct_color_dma`: both are all-black screens) | Audio verdict is out of scope here — the sound stack has its own instrument (VGM capture / A-B vs Oracle, `docs/2026-07-23-rt3-oracle-ab-findings.md`). |
| `vcounter` (`vctest.bin`) | V-counter / HV-counter behaviour across the frame | Frame hash only — **not scraped**: the ROM draws its results in a proportional font that is not an ASCII-ordered nametable, so the text-scrape path does not apply | `frame_hash=0x294957c8001b9f93` | Unscraped by choice, not by failure. Scraping it needs a glyph table; deferred. |
| `m68k_opcode_sizes` (`m68k_opcode_sizes.bin`) | Per-opcode size/encoding sweep | Frame hash only — **not scraped**: at the pinned 120-frame budget the screen shows the ROM's font/pattern page, not a result page | `frame_hash=0x5436cda5786ea450` (re-pinned 2026-08-02 with the K1 fix; was `0xca81010c64f8e701` — 476 px moved across the `$0/4/8/C/E` opcode pages, the newly-trapping encodings) | Unscraped by choice. Finding its result page (a longer budget or an input) is deferred. |

Deliberately **not** vendored: the two TiTAN *Overdrive* mega-demos. They are the classic hardware-torture
payloads, but their verdict is human judgment on a moving picture, not a scrapeable pass/fail. Their Drive
ids are recorded in the header comment of `tools/fetch-testroms.sh`.

## Known bugs found during this recon — recorded, NOT fixed

These were found while building the harness. They are named here so the scorecard rows above are
*attributable*; fixing them is separate work (each will move a baseline row, deliberately).

### K1 — illegal encodings execute instead of trapping — **RESOLVED & FIXED (2026-08-02)**

*Symptom (was):* `m68k_illegal` (`itest`) ends red (`$000E`).

*Mechanism:* on the 68000 `CMPI`'s destination must be **data-alterable**; address-register direct (mode 1)
is not a legal `CMPI` destination at any size (An comparison is `CMPA`'s job), and mode 1 is illegal for
*every* byte-size EA. We decoded it as a real instruction on two independent counts (`src_seq`'s
un-size-gated mode-1 arm in `ea.rs`; `cmpi_recipe`'s missing data-alterable destination guard in
`decode.rs`). Fixing those two exposed the rest of the class: running `itest`'s own 11529-entry opcode
table against the decoder enumerated every remaining illegal-but-executing family.

*Fixed (all routed through the existing `decode_time_exception_recipe(4)` illegal path; group-1 semantics,
stacked PC = the offending instruction):*

- `ea.rs src_seq` — mode 1 (An-direct source) now size-gated: byte → ILLEGAL. Flips `AND.b/OR.b/ADD.b/
  SUB.b/CMP.b An,Dn` (`$C008/$8008/$D008/$9008/$B008` + all dn/rn), `TST.b An`, `BTST #n,An`.
- `ea.rs move_emit_source` — same byte gate for MOVE: `MOVE.b An,<ea>` (`$1008` family) now traps.
- `decode.rs cmpi_recipe` — data-alterable destination guard: mode 1 and 7/2-4 ILLEGAL at every size
  (`$0C08/$0C48/$0C88`, `$0C3A/$0C3B/$0C3C` + word/long forms). The long path was already gated —
  byte/word now match it.
- `decode.rs` CMPI dispatch — size field SS=3 (`$0CC0-$0CFF`, the 68020 CAS space) → ILLEGAL (previously
  fell into the `.l` arm; `$0CE2` = `CMPI.?? -(A2)` was the encoding that derailed `itest`'s own table
  pointer once the sweep got past `$0C08`).
- `ea.rs ea_tst` — TST's EA is data-alterable only: An / PC-relative / `#imm` ILLEGAL at every size
  (`TST.w/.l An`, `TST.x d16(PC)/d8(PC,Xn)/#imm` — 68020 legalized some of these; the 68000 traps).
- `decode.rs chk_recipe` / `mul_recipe` / `div_recipe` — An-direct source ILLEGAL (data-addressable only):
  `CHK An,Dn`, `MULU/MULS An,Dn`, `DIVU/DIVS An,Dn`.
- `decode.rs arith_ea_dn` — AND/OR sources are data-addressable only: `AND.w/.l An,Dn`, `OR.w/.l An,Dn`
  ILLEGAL at every size (ADD/SUB/CMP keep their legal word/long An sources).
- `decode.rs` shift arm — memory-form shifts require bit 11 = 0; `$E8C0-$EFFF` (the 68020 bit-field
  space, BFTST/…) previously decoded as memory shifts (the type field reads bits 10-9 only) → ILLEGAL.

*Verification:* `itest`'s full opcode table (11529 words at ROM `$4FC`) replayed against the decoder =
**11529/11529 trap to vector 4, 0 execute**; the ROM itself now completes its sweep and goes **green
(`$00E0`) at ~frame 9** (the scraper budget was raised from 2 to 20 frames — the old budget dated from the
fail-fast red). Unit pins in `decode.rs` (`k1_*` tests: every family + legal neighbors incl. MOVEP, which
shares the dynamic-bit-op mode-1 space). SST v1 is structurally blind to the whole class (zero An-direct /
PC-rel-dest / SS=3 / bit-field cases vendored — verified per family) so the full sweep is unmoved.
`m68k_opcode_sizes` (which plots the live decode map) moved its visual pin with the fix
(`0xca81010c64f8e701 → 0x5436cda5786ea450`, 476 px across the `$0/4/8/C/E` pages) — deliberate, attributable.

*Reference:* M68000PRM per-instruction "Effective Addressing Mode" tables; M68000UM §6 vector 4.

### K2 — Z80 `$7F00-$7F1F` VDP-mirror reads return a constant `$FF` — **RESOLVED & FIXED (2026-08-02, Z80 side)**

Fixed: the Z80-side status/HV window is now routed to the live `Vdp`. `crates/oracle-core/src/z80/bus.rs`
carries the `Vdp` split-borrow (threaded from `System::catch_up_z80` exactly like the `Ym2612`, read at the
Z80's own frontier mclk), and:

- **`$7F04-$7F07`** is a REAL control-port status read — `Vdp::control_read_status`, so it has the same
  side effects as a 68k `$C00004` read (clears the control-port write-toggle, the pinned recon-vdp
  semantic) and the same byte-lane split (even = status high byte, odd = low byte).
- **`$7F08-$7F0F`** reads the live HV counter (`Vdp::hv_counter_read`, side-effect-free; even = V,
  odd = H).
- **`$7F00-$7F03` (data port) is deliberately UNCHANGED** — still `$FF`, because a Z80 read of the VDP
  data port locks up real hardware (the `vdp-dataport-read-lockup` known-difference, recon R1 in
  `docs/2026-07-16-vdp-recon.md`); we return open bus instead of modeling the hang. Pinned by the
  `vdp_data_mirror_stays_open_bus_with_no_side_effects` test.

Unit pins in `z80/bus.rs` (`vdp_status_mirror_*`, `vdp_hv_mirror_*`, `vdp_data_mirror_*`). Blast radius
measured before the change: **0** Z80 reads in `$7F00-$7F1F` across all four currency suites and the
`s4.soundtest.bin` VGM-capture boot (600 frames); **298,333** reads (all `$7F05`, a status-low-byte poll)
from `fm_test` in this corpus. No scorecard row moved (fm_test's end-of-frame visual pin does not depend on
the poll result); all frozen currencies byte-identical.

*The 68k-side half — RESOLVED (2026-08-02, K4-6):* the 68k window now routes `$A07F00+` (15-bit-masked)
through the **same** shared reader (`z80/bus.rs::vdp_mirror_read`) — live status at `$7F04-$7F07`
(side-effecting, `open_bus = 0` per the K2 pin), live HV at `$7F08-$7F0F`, `$FF` for the data port (the
same ledgered lockup known-difference) and `$7F10+`. Z80 *writes* to `$7F00-$7F1F` (other than the PSG
tap at `$7F11`, which the 68k window now also taps) still drop, and `$7F10-$7F1F` reads stay open bus
(write-only region on hardware).

### K3 — div0 stacked PC — **RESOLVED & FIXED (2026-08-02)**

Adjudicated and fixed: zero divide (vector 5) is a **group-2** exception, so the stacked PC is the
**next instruction's address** (`instruction_pc + 2 × the recipe's Prefetch count`), not the instruction
start our core previously stacked (pinned from the sole SST DIVU div0 sample — that sample is wrong;
emulator-generated, contradicting M68000UM §6.2.4, a BlastEm GDB-RSP hardware probe, Oracle's
`DIVU.h`/`DIVS.h`, and SST's own TRAP/TRAPV/CHK internal consistency). Fix in
`crates/oracle-core/src/m68000/microop.rs` (both `Divu`/`Divs` div0 arms); the SST sample is a documented
exclusion in `tests/singlestep_m68000.rs` (`covered()` + the
`divu_div0_stacks_next_instruction_pc_known_difference` pin). Value-only — timing/bus stream unchanged; no
scorecard row moved (`m68k_illegal`'s single-bit verdict cannot discriminate it, and no vendored ROM
executes a div0 on its scored path — measured 0 hits across the whole scorecard run).

### K4 — open-bus model — **RESOLVED & FIXED (2026-08-02, slices K4-0..K4-5 landed; memtest 4/13 → 12/13)**

Unmapped / write-only / partially-decoded addresses returned fixed values instead of the residue the real
bus leaves floating. Root-caused and designed in `docs/2026-08-02-k4-openbus-design.md`: the latch
(`last_bus_word`) already existed and already carried hardware-exact prefetch residue — the *consumers*
were wrong outside the VDP region. Blast radius measured first (K4-0, zero src changes):
`docs/2026-08-02-k4-0-hit-table.md` — gate fixtures 0 hits, frozen currencies untouched by construction.

**Row-by-row account (memtest hardware column):** rows 1 (`400000-7FFFFF`), 9 (`A11200`) — K4-1; rows
7/8 (`A11100` ×2) — K4-2; rows 2/3/5 (the `A0xxxx` window) — K4-3; row 6 (`A10000-A1001F`) — K4-4;
row 11 (`C00004-C00007`) — K4-5 fixed **the open-bus half exactly** (`0290`→`4E90`; the floating upper
6 bits now carry the residue, `4E..` ✓) but the row stays red on the **status low byte** (`90` vs
hardware `88`), a pre-existing non-open-bus gap outside the K4 design's rule (STOP-condition (c)
discipline — reported, not folded in): our status bit 4 reports the raw odd-frame toggle where the
Sega manual says ODD reads 0 outside interlace mode, and bit 3 (VBlank) depends on the frame phase at
the ROM's read instant (timing-model, deferrable class). Rows 4/10/12/13 passed before K4.
K4 final: **12/13**, every remaining bit the open-bus model owns is hardware-exact. The row-11
status-low-byte residual was then adjudicated and fixed the same day — **13/13** (addendum below).

Slice log (each row-flip amends the `BASELINE` in the same commit):

- **K4-1** — arbiter open-bus flavor (`$400000-$7FFFFF` gap + write-only `$A11200` reads = residue high
  byte, low byte driven `$00`), plus the byte-read latch lane-merge (replacing the `b * 0x0101` smear).
  memtest `400000-7FFFFF` + `A11200` rows green → **6/13**. No other scorecard row moved (per the K4-0
  table, nothing else touches these addresses).
- **K4-2** — `$A11100` is partially decoded: the arbiter drives only the grant bit (word bit 8) + the
  low byte `$00`; bits 9-15 float with the residue, and the readable bit folds Z80 RESET in
  (1 = unavailable while reset asserted — hardware row 7 `4F00` + Exodus `MDBusArbiter.cpp:444`).
  Both `A11100` rows green → **8/13**; rest of the scorecard byte-identical. Game risk pre-adjudicated
  in the K4-0 table (ristar's single under-reset read = a bounded 16-poll `dbeq`) and verified after:
  ristar\@900f and gunstar\@420f/900f boot renders + PCs **pixel-identical** to the pre-K4 baselines.
- **K4-3** — the 68k-side Z80 window (`$A00000-$A0FFFF`) is gated on `busreq && reset-released`
  (closed → arbiter open bus / dropped writes; memtest row 2), word reads mirror the single 8-bit
  result into both halves (row 3 `F3F3`; `MDBusArbiter.cpp:489-495`), and `$A06000-$A07EFF` reads
  `$FF` through the open window (row 5). **Plus a pre-existing corruption fix the row-3 debug
  uncovered:** a 68k write to `$A06000-$A07FFF` (Z80-side bank-register/unused region — never RAM)
  used to alias into `z80_ram` through the `& $1FFF` mirror — memtest's single `$FF` bank-canary
  write (PC `$34C2`, f11) overwrote the loaded Z80 program's first byte, which is why row 3 read
  `FFED` instead of `F3ED` *before* any gating existed. Those writes now drop from RAM's view.
  → **11/13**; rest of the scorecard byte-identical. Verified beyond the scorecard: 7-game boot
  render A/B all pixel-identical (drivers upload through this window holding BUSREQ, per K4-0's
  zcR!/zcW! = 0), and the `s4.soundtest.bin` VGM capture still carries the full ~5.9k-write score
  (18,530-byte VGM at 600 frames, key-ons through frame 599).
  *Ledgered follow-ups (not regressions):* the 68k-side path to the real serial bank latch (the
  Z80-side latch at `$6000` is live), the window's true 15-bit address masking (`$A08000+` currently
  still mirrors RAM 8-KiB-wise), and 68k-side `$A07F00+` VDP-mirror routing (K2's deferred half).
  **All three closed by K4-6 below (2026-08-02).**
- **K4-4** — the I/O block (`$A10000-$A1001F`) does not decode A0 (Exodus
  `AddressDiscardLowerBitCount="1"`): each odd register answers BOTH byte lanes (`$A10000` reads the
  version byte, word reads are the register duplicated — row 6 `A0A0`). Reads only; even-byte
  *writes* still drop (unexercised by the corpus — the K4-0 table's ubiquitous `ioW` = 3 is the
  boot-time `tst.w $A10008`-family over still-zero registers, value-unchanged by the mirror).
  → **12/13**; `io_sample`/`gfx_joystick` and the rest byte-identical; sonic3k/s2rev01/ristar boot
  A/B pixel-identical.
- **K4-5** — the VDP status word drives only its low 10 lines (`StatusRegisterMask = 0x03FF`,
  `S315-5313_Ports.cpp:1163-1170`); bits 10-15 float with the open-bus residue.
  `Vdp::control_read_status(open_bus, mclk)` (the `data_read_at` plumbing pattern); the 68k bus passes
  the latch, internal callers and the Z80-side `$7F04` mirror pass 0 (K2 behavior byte-identical —
  the Z80-side bus stays out of K4 scope). Row 11's open-bus half exact (`4E90`); the row's remaining
  status-low-byte delta is the pre-existing gap ledgered above. **No other scorecard row moved** —
  despite VDPFIFOTesting's 348k and TF4's 677k status reads (the corpus consumes low bits only).
  Adjudicated beyond the scorecard: 7-game boot A/B pixel-identical (incl. TF4, the heaviest status
  consumer), `s4.soundtest.bin` VGM capture **byte-identical** K4-4 → K4-5.

- **K4-6 (2026-08-02)** — the 68k-side Z80 window **completed**: the window's address is masked to
  15 bits (`$A08000-$A0FFFF` behaves as `$A00000-$A07FFF`; MDBusArbiter.cpp:304 — Charles MacDonald's
  hardware tests) and decoded per the Z80's own local bus map, one source of truth: `$0000-$3FFF` Z80
  RAM; `$4000-$5FFF` the YM2612's full select span (ports = low 2 bits; memtest row
  `A04000-A05FFF = 0000` — the FM carve-out keeps its K4-3 answer-regardless-of-ownership pin).
  *Known asymmetry (deferred):* the **Z80-side** decode deliberately stays `$4000-$4003` (`$4004-$5FFF`
  reads `$FF` / drops there) — that span is unpinned by any test ROM from the Z80 side and has zero
  corpus evidence (no driver touches `$4004+`), so widening it would move sound-currency surface for no
  gain; the two sides intentionally differ until Z80-side evidence exists;
  `$6000-$60FF` = one serial tick of the **same** 9-bit bank latch the Z80's `$6000` write loads
  (`z80/bus.rs::bank_latch_tick` — one register, two paths; only memtest's `$FF` canary exercises it,
  probe column `bkW`); `$6100-$7EFF` `$FF`; `$7F00-$7FFF` the **same** VDP-mirror reader the Z80 uses
  (`vdp_mirror_read`, K2's 68k-side half — see the K2 entry) with the Z80-shaped PSG write tap at
  `$7F11` (probe `vpW` = 0 corpus-wide). **Q4 RESOLVED — word writes land ONE byte:** the probe's
  `wwW!` column found real ROMs exercising it (Gunstar Heroes + Alien Soldier: a 4096-word even-address
  sweep clearing all 8 KiB of Z80 RAM; `m68k_opcode_sizes`: 21 real data words), so the rule was
  adjudicated and implemented — only the HIGH byte lands, at the (even) target address. Provenance:
  the reference arbiter's implemented code (MDBusArbiter.cpp:496-501, even address →
  `data.GetUpperHalf()`, one byte written; its TODO admits their own hardware tests never confirmed
  it), Genesis Plus GX's implemented `z80_write_word` (`mem68k.c`: stores `data >> 8` only), Plutiedev
  ("you must use byte accesses when touching Z80 RAM, word accesses won't work"), and the read side of
  the same one-8-bit-cycle mechanism being hardware-pinned by memtest row 3 (`F3F3`). No direct
  hardware test of the write side exists in any source we hold — recorded honestly here. Rider: the
  one-byte rule makes `m68k_opcode_sizes`' word-uploaded Z80 program execute its half-landed stream
  (as hardware does), which reached two deferred-panic undocumented-Z80 classes; those are now the
  pinned NONI/prefix-ignored behaviors (see the z80 commit — ED holes = 8T no-ops, DD/FD prefix
  ignored on non-HL opcodes; Vectorman's `FD FF` boot panic un-stuck as a side effect, 26 → 900/900
  frames). **Scorecard byte-identical** (memtest stays 12/13, `m68k_opcode_sizes` visual pin
  unmoved); currencies byte-identical; s4 VGM capture sha-identical; gunstar + sonic3k 900-frame boot
  renders sha-identical.

**Open questions carried from the design's §6 (still open):** Q1 — the physical mechanism of the
arbiter's low-byte-`$00` (adopted as the memtest-pinned empirical rule; the C++ reference retains the
full word instead); Q5 — board-revision stability of the reference
values (unknowable offline; the vendored ROM's column is the pinned ground truth); Q6 — region-wide
extrapolation of the arbiter flavor to untested gaps (`$A10020-$A10FFF`, `$A11000`, `$A130xx` reads,
`$A14000` — still full-latch retention, applied only where evidenced). Q4 and the K4-3 ledgered
follow-ups (68k-side bank-latch path, true 15-bit window masking, 68k-side `$A07F00+` VDP routing)
are **closed by K4-6** above.

### Row-11 status-low-byte addendum (2026-08-02) — adjudicated SEMANTICS, fixed, memtest 13/13

The residual after K4-5 was the status LOW byte: hardware `4E88` (bit 3 VBlank SET, bit 4 ODD CLEAR)
vs our `4E90` (bit 3 clear, bit 4 set). Both halves turned out to be **behavioral bugs with direct
reference corroboration — NOT read-instant timing**, so the deferrable-timing escape hatch was not
taken:

- **ODD (bit 4) outside interlace.** We toggled the odd-frame flag unconditionally every VInt; the ROM
  runs with reg $0C = $81 (LSM bits 2:1 = 0, interlace OFF), and hardware reads bit 4 = 0. The
  reference implements this at the toggle point, not the read: `oddFlagSet = interlaceIsEnabled &
  !oddFlagSet` (Oracle `Devices/315-5313/S315-5313_Timing.cpp:1103` in `AdvanceHVCounters`, repeated at
  :1181 in `AdvanceHVCountersOneStep`; interlace-enable = reg $0C bit 1, `_interlaceEnabledCached =
  data.GetBit(1)`, `S315-5313_Ports.cpp:1883`). Fix in `Vdp::raise_vint` — the stored flag is forced 0
  while interlace is off. No corpus ROM enables interlace on its scored path, and the renderer never
  consumes the flag (it only feeds status bit 4), so no other row could move — and none did.
- **VBlank (bit 3) at the read instant.** Measured with a throwaway `on_event_at` instrument: memtest's
  three row-11 reads land at mclk 9,949,478 / 9,949,646 / 9,949,814 = frame 11, **line 27** — mid
  active scan, nowhere near the vblank window, with **reg 1 = $04 (display DISABLED)**; the ROM only
  enables the display (reg 1 $04→$44) at mclk 11,198,824, *after* the sweep. Hardware still reads
  VBlank SET because **the VBlank status bit is forced set while the display is disabled** — Oracle:
  `vblankFlag |= !_displayEnabledCached` with the comment "although not mentioned in the official
  documentation, hardware tests have confirmed that the VBlank flag is always forced to set when the
  display is disabled" (`Devices/315-5313/S315-5313_General.cpp:2345-2351`). So the delta was a missing
  status-semantics rule, not a whole-boot phase difference: at line 27 the read instant is ~170 lines
  from either vblank boundary, far beyond any plausible per-instruction timing skew. Fix in
  `Vdp::status_word` (bit 3 only — `vblank()` itself stays a pure timing function; the renderer and
  goldens are untouched).

With both: low byte `$90`→`$88`, row green, **memtest 13/13**. Scorecard otherwise byte-identical
(io_sample, VDPFIFOTesting and every VISUAL-BASELINE hash unchanged); all four currency suites green.
One boundary note stays open (not exercised by any pinned test): our vblank window covers lines
224..=261 while Oracle's `vblankClearedPoint = 0x1FF` clears the flag on line 261 (V28 NTSC,
`V28NtscNoIntScanSettingsStatic`, `S315-5313_Timing.cpp:250`) — memtest cannot discriminate (its only
line-261 read happens with the display disabled, where the forced-set rule dominates).

### A2 addendum (2026-08-03) — control-port / code-register edges; T13 + T10 fixed, T12 held

Slice A2 of the FIFO/scanline arc. Both pins come from `vendor/TestRoms/vdp_port_access.bin`'s own
**embedded expected-value tables** (each test's 36-byte name string is followed 36 bytes later by its
expected words; the ROM loads both with literal `lea`s, so the addresses are the ROM's, not guesses), each
corroborated by public hardware documentation. Scorecard movement: `vdp_port_access` page 2 went
**9/7/16 → 11/5/16**; every other row, and all four frozen currency suites, byte-identical.

**T13 "Register Writes and Code Reg"** (name `$22D6`, table `$22FA` =
`0123 4567 ffff ffff ffff ffff f923 fd67 ffff ffff f923 fd67`). The observation group at ROM `$23B4` sets a
VRAM-write command, writes register 15, then writes `$0123`/`$4567` — and hardware reads back the *old*
`$FFFF`s. So the register write leaves the data port dead. *Mechanism pinned:* a first control word always
latches CD1-CD0 from its bits 15-14, and the `$8xxx` register form's bits 15-14 are `10`, leaving
CD3-CD0 = `xx10`, which names no target. Charles MacDonald, *genvdp.txt* 1.5f: "Writing to a VDP register
will clear the code register. Games that rely on this are Golden Axe II … and Sonic 3D." **It is not a full
clear:** the same test's fifth/sixth groups (ROM `$24EC`) write a register and then a *first-half-only*
control word, and the following writes land on the previously latched **VSRAM** target — so CD5-CD2 survive
(table words 8-11). A13-A0 is left alone on the register form: unobserved by the ROM, and MacDonald records
the address side as unknown ("It is not known if the address register is cleared as well").

**T10 "Partial CP Writes"** (name `$FC86`, table `$FCAA`; 14 words, 13 of which already matched). The single
red word was #7, from ROM `$FFEA`: a full CRAM-**read** command, then a first-half-only word (CD1-CD0 ← 01),
leaving CD3-CD0 = `1001` — CD0 set, so it looks like a write, but `1001` is not in genvdp.txt's code table
(`0000` VRAM read / `0001` VRAM write / `0011` CRAM write / `0100` VSRAM read / `0101` VSRAM write / `1000`
CRAM read). Hardware discards it; we were writing to VRAM. Fixed by gating `write_target` on
CD3-CD0 ∈ {`0001`,`0011`,`0101`} — the write half of MacDonald's "You cannot write data after setting up a
read operation, or read data after setting up a write operation. The write or read is ignored." The word is
still enqueued into the FIFO and the address still auto-increments (unobserved by the ROM either way; kept
so no FIFO/timing behaviour moves).

**T12 "Register Write Mode4 Mask" — RESIDUAL, not fixed.** Table `$20EC`; sequence at ROM `$2244` sets
reg 1 = `$40` (M5 clear → Mode 4), writes `$8F04` (reg 15 = 4), restores reg 1 = `$44`, then streams three
words: hardware lands them contiguously, i.e. the autoincrement is still **2**, i.e. the mode-4 register
write never happened. Kabuto's hardware notes: "All registers except for the 10(?) SMS registers are
disabled." The fix is one line in `Vdp::write_register` (`if regs[1] & 0x04 == 0 && reg > 10 { return }`)
and it does make T12 pass — but **it moves frozen currency**: `crates/oracle-core/src/testrom.rs`'s golden
ROM programs reg 1 = `$50`, which leaves M5 **clear**, and then writes registers 11/12/13/15/16, so
`export_state_v1::GOLDEN_HASH` and the `golden_frames` scenes both move (locally it also reddened 52 unit
tests whose fixtures never set M5). Held for an owner ruling. The spec is preserved as an `#[ignore]`d unit
test, `vdp::tests::mode4_ignores_register_writes_above_ten`, next to a NOT MODELLED note in
`write_register`.

Sources: [genvdp.txt 1.5f, Charles MacDonald](http://jiggawatt.org/genvdp.txt) ·
[Kabuto's hardware notes (Plutiedev mirror)](https://plutiedev.com/mirror/kabuto-hardware-notes).

### A3a addendum (2026-08-03) — DMA payload words occupy FIFO slots; T3 fixed, T4 parked

Slice A3a of the FIFO/scanline arc, implementing the test-3 half of
`docs/2026-08-03-a3-dma-fifo-design.md`. Scorecard movement: `vdp_port_access` page 1 went
**6/3/9 → 7/2/9**, cumulative **11/5/16 → 12/4/16**; every other row and **all frozen currency suites
byte-identical with their existing constants** (A3a touches only `fifo`/`fifo_write`, neither of which is
in `state_hash` or `export_state` — currency-neutral by construction, and confirmed by running every gate).

**T3 "DMA Transfer using FIFO"** (name `$5DE8`, expected table `$5E0C` =
`c800 c800 c000 c000 d800 d800 d111 d111 e800 e800 e000 e000 f800 f800 f111 f111`; DMA source payload
`AAAA BBBB CCCC DDDD EEEE FFFF` at `$5DDC`). Despite the name, the test is **not** about write
interleaving: it never reads its DMA destination. All 16 observations are CRAM/VSRAM reads at address 0 of
a zeroed memory, so every expected bit comes from the **undefined-bit FIFO snoop** (Nemesis, *VDP
Internals*: undefined bits "are actually initialized to the content on the next available FIFO entry (the
one containing the data written to control port four writes ago)"). Decoding the table bottom-up says one
sentence: after the 6-word DMA the physical ring holds the last four **payload** words with the write
cursor parked on `$CCCC`, and each intervening CRAM `$FFFF` write walks the cursor one slot. We held the
last four **marker** words instead, producing `3000 3000 3000 3000 4000 …` — reproduced exactly by the new
`bus.rs` replay test on its first (red) run, which is the cross-check that the decode chain is right.

*Mechanism pinned:* a 68k→VDP DMA's payload words enter the same physical 4-slot FIFO a CPU data-port
write uses. Nemesis, same thread: a DMA "will read a value from external memory using the DMA source
address register and **add it to the FIFO** using the current command code and incremented command address
registers". Corroborated by Kabuto: "When writing a value to the VDP's data port (**or the VDP does that
internally through DMA**) both value and current address are appended to its internal FIFO." Implemented by
splitting `fifo_enqueue` into `fifo_store` (ring + write cursor) and `fifo_enqueue` (`fifo_store` + pending
count), and having `dma_write_word` call `fifo_store`.

**Deliberately NOT bumping the pending count (design Q1) — and what it costs.** Our Mem DMA runs
synchronously inside one bus access and bills its time through `dma_cost` + the returned halt wait, so
counting payload words as pending would leave phantom entries no clock has advanced past — a spurious
/DTACK stall on the next data-port write in every DMA-using ROM. **Measured consequence:** T16 "FIFO Wait
States" stayed at **62/80** verdict bytes green — A3a moved it *not at all*. That is now a concrete answer
to Q1 rather than a hypothetical: T16's groups 9-10 stream probes need FIFO **occupancy** during a DMA, not
merely ring **contents**, so they cannot go green until DMA words are modelled as pending (which in turn
needs a non-synchronous DMA, or an occupancy model decoupled from the drain clock). Recorded here as the
named blocker for T16's remaining reds; the rest of T16 still needs discrete per-line access-slot
scheduling, as noted in the A1 entry.

**T4 "DMA Fill FIFO Usage" — RESIDUAL, parked, not attempted.** Expected table `$DC54`; we differ at
exactly words 8 and 13 (`1212` vs `1234`, `0000` vs `0012`) — precisely the design's §4.3 prediction, and
its snoop half (words 0-7) already passes. The fix (the fill trigger applied as a normal word write, and
fill bytes written to `address ^ 1`) is designed and evidenced in
`docs/2026-08-03-a3-dma-fifo-design.md` §3.4-3.5, but it **moves `export_state_v1::GOLDEN_HASH`** because
`testrom.rs`'s golden fixture zeroes VRAM with a DMA fill whose last byte (`vram[$FFFF]`) is currently left
at its power-on random value. Held for an owner ruling — see
`docs/plans/2026-08-03-PARKED-owner-ruling.md`. Slice A3b carries it.

**How the per-test and verdict-byte numbers above are measured (so a later slice can tell whether it moved
them).** The committed harness scrapes only the ROM's aggregate `Results: (P/F/T)` line, so the finer
figures — per-test PASS/FAIL and T16's "62/80 verdict bytes" — come from a throwaway probe, re-created per
slice and never committed. Two equivalent techniques, both used during the 2026-08-03 arc:

1. *Screen decode.* The ROM renders each observed word as hex glyphs and colours them per byte against its
   own expected table: palette ink `$0040` (green) = byte matched hardware, `$000C` (red) = mismatch. Decode
   plane A's nametable **and** the per-cell palette bits (the harness's `text_rows`, `conformance_roms.rs`
   ~line 203, shows the nametable half). "62/80" is the green-byte count over T16's 10 groups × 4 words × 2
   bytes.
2. *RAM record scrape.* Each test builds a record in a RAM buffer — name, then expected words, then observed
   words — before the render pass diffs them. Locating those records gives exact per-word verdicts without
   any glyph decoding, and is the easier of the two.

Page 2 is reached by: run 60 frames, `set_pad(0, Pad { start: true, ..Default::default() })`, run 5 frames,
release, run 535 frames. Expected-value tables live in the ROM itself: each test's `lea`-loaded 36-byte
name string is followed 36 bytes later by its 16 expected words — those tables are the authoritative
hardware answers and were the basis for every behavioural pin in this arc.

Sources: [Nemesis, VDP Internals p.3](https://gendev.spritesmind.net/forum/viewtopic.php?t=1291&start=43) ·
[Kabuto's hardware notes (Plutiedev mirror)](https://plutiedev.com/mirror/kabuto-hardware-notes).

## Limitations of the instrument itself

**L1 — end-of-frame capture — NARROWED 2026-08-03.** The harness's default capture reads the framebuffer
*after* `run_frames(n)` completes, so any effect that exists only mid-frame is lost. That default is
unchanged (every other row still hashes the end-of-frame picture and every one of those hashes is
byte-identical across this change), but the harness now also has a **per-scanline capture** path:
`FrameCapture` opts into `BusEventSink::wants_scanlines` and retains the last complete frame of lines as
the `Scanline` event renders them, and `frame_hash_scanline` hashes that in the *same* FNV-1a byte layout
as `frame_hash` (both go through `fnv1a_rgb`), so the two are directly comparable.

`color_1536` is upgraded to it and is the clean demonstration of what the limitation cost. Both
framebuffers were dumped as PPM and inspected by eye:

* **End-of-frame** (`0x96b9c93c4f3dd325`): **4 distinct colours** in the whole frame — a dark-green
  patterned backdrop with a rectangle in the middle that is flat black on the left ~2/3 and flat grey on
  the right ~1/3. CRAM at end-of-frame holds only the *last* of the ROM's mid-scanline rewrites, so the
  rectangle collapses to two solid blocks.
* **Per-scanline** (`0x917371f07409cb25`): **~1400 distinct colours** — the rectangle is the ROM's actual
  colour field: twelve vertical colour columns (dark red / olive / indigo / teal, then red / green / cyan /
  violet, then pastel peach / lavender / mint / white) each ramping through many horizontal bands, and the
  whole ramp repeated twice down the frame. This is the 1536-colour trick.

**What remains.** The two NOT-RENDERABLE rows are *not* fixed by per-line capture. Both were probed under
the capture sink on 2026-08-03 and re-adjudicated below (L1a, L1b): they share **one** root, and it is not a
capture-time limitation at all. They stay end-of-frame pins, with their reasons narrowed.

### L1a — `cram_flicker`: the CRAM-write artefact, not the border

Probed under the scanline sink (120 frames, the harness's own budget and seed):

* The per-scanline frame is **byte-identical** to the end-of-frame frame — same hash
  `0x815bb645bc46a325`, **224 of 224 lines identical**, and the whole frame is **one distinct colour**
  (verified by eye as a PPM: solid black).
* The active display is **empty**. Plane A base is `$0000`, every pixel resolves to the backdrop, and
  `R7 = $00` → backdrop = CRAM index 0 = `$0000`.
* The ROM writes CRAM **16 times per active line, on every active line** (4,528 word-writes in the last
  frame; 420,386 over 120 frames), **all CPU writes, zero DMA**, cycling `$000E` / `$00E0` / `$0E00`
  round-robin into exactly **two entries: index 4 and index 36**.

That last fact is the adjudication. The earlier reason on this row — "border-only rendering" — does not
survive it: a border-colour demo would hammer **index 0** (the backdrop is the border), and this ROM never
touches index 0. It writes two ordinary palette entries that **no on-screen pixel references**, on a screen
it deliberately leaves blank. What the ROM demonstrates is therefore the **CRAM-write artefact** itself (the
written colour appearing on the display at the beam position of the write) — a *sub-scanline* effect at the
write's h-position, which we do not model anywhere, in the active area or the border. Per-line capture
cannot help, and **rendering the border would not help either**: with no dot model there would still be
nothing to draw.

Row reason narrowed accordingly: *border-only rendering* → *CRAM-write artefact, sub-scanline*. Same root as
L1b. (Pinning the exact on-hardware appearance would need a reference capture — out of scope for this
instrument; nothing here depends on it, since the blocker is on our side either way.)

### L1b — `direct_color_dma`: sub-scanline CRAM, two concrete blockers

Probed the same way:

* Per-scanline frame **byte-identical** to end-of-frame — hash `0xed40dc4a6c4fc325`, 224/224 lines
  identical, **one distinct colour** (solid black by eye).
* The ROM's CRAM traffic is **99.997% DMA** (4,923,072 of 4,923,206 writes over 120 frames) and lands
  **entirely on CRAM index 0** — the backdrop — with the address never advancing. **44,352 words per
  frame** = 198 × 224, i.e. ~198 colours per line for all 224 active lines. This is the direct-colour
  technique exactly: the *picture* is the sequence of values passing through CRAM[0], one per pixel slot.
* In our model **all 44,352 of a frame's writes land inside a single inter-line window** (they bucket into
  one `Scanline` boundary), so there is nothing for a per-line sampler to sample.

The two blockers, precisely:

1. **CRAM writes carry no h-position.** `Vdp::write_target`'s CRAM arm (`crates/oracle-core/src/vdp.rs`,
   the `Target::Cram` branch) stores the masked word and captures a `VdpWrite` — target, address, old, new,
   size, via — and **no time**. A write leaves no record of *when* inside the line it happened.
2. **Even a timestamp would be instruction-granular.** `System::step_cpu`
   (`crates/oracle-core/src/system.rs`) samples `let now = self.scheduler.now()` **once**, before
   `cpu.step`, and hands that single value to `MegaDriveBus` as `now_mclk`; every VDP access the
   instruction makes sees it. For this ROM that is decisive: `MegaDriveBus::run_mem_dma`
   (`crates/oracle-core/src/bus.rs`) runs the **whole** transfer loop in one pass at that one `now_mclk`,
   so a frame's entire colour stream is emitted at a single instant of model time.

Rendering this ROM therefore needs a per-pixel (not per-line) CRAM timeline, which means a timestamped CRAM
write **and** sub-instruction clock advance through a DMA. That is a real engine change, not a harness one.

### Named follow-ups (not implemented here)

* **F-CRAMDOT — sub-scanline CRAM timeline.** Timestamp CRAM writes with an h-position and advance the
  clock through a DMA body so writes distribute across the line. Unblocks **both** rows above. Touches the
  VDP write path and the DMA loop, i.e. it is currency-relevant — needs its own design pass.
* **F-BORDER — border / overscan rendering.** We render the 224-line active area only. Independent of
  F-CRAMDOT and, per L1a, **not** what either remaining row needs; recorded so the gap stays visible.

**L2 — frame budgets are settle-time guesses.** They were found empirically per ROM and are generous, but a
timing change that slows a ROM past its budget shows up as a scorecard diff, not as a timeout. Read a diff on
a text ROM as "did it get slower?" before reading it as "did it get wrong?".

**L3 — verdict-glyph hashing is pixel-exact.** `vdp_sprite_masking`'s TICK/CROSS/PASS/FAIL classification
hashes rendered pixels, so any palette or rendering change turns a known label into
`UNKNOWN-GLYPH(0x…)`. That is by design (loud, not silent), but it means the four glyph hashes are a second
pin that must be re-derived if the renderer legitimately changes.

## Open questions

**Q1 — the `vdp_sprite_masking` H32/H40 toggle responds to `C`, not `Start`.** The ROM's on-screen text says
`PRESS START BUTTON TO SWITCH BETWEEN H40 AND H32 MODE`. In our core, `Start` does not toggle it; `C` does.
This is **recorded, not fixed** — it could be an input-mapping/TH-protocol subtlety on our side, or the ROM's
text could be stale relative to its build. The harness therefore exercises H32 only. Resolving it needs a
cross-emulator A/B on this exact ROM (the natural instrument is the existing BlastEm differential rig).

**Q2 — `vcounter` and `m68k_opcode_sizes` are unscraped.** Both are automatable in principle; neither yields
its verdict through the ASCII-nametable path the other text ROMs share. Deferred, not blocked.

## How to amend a row

1. Reproduce with `cargo test -p oracle-core --test conformance_roms -- --nocapture` (the scorecard prints
   before the assert).
2. Investigate with `cargo run -p oracle-core --example testrom_probe -- vendor/TestRoms/<rom>.bin <frames>
   [font_base_hex]` (env: `SCREEN=<step>`/`SCRX0`, `RAW_ROW`, `TILES=<hex>,<count>`, `BLOCKS`,
   `PRESS`/`PRESS_AT`/`PRESS_LEN`).
3. Update `BASELINE` **and** this ledger in the same change, with the evidence for why the row moved.
