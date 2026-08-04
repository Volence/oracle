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
| `vdp_port_access` (`VDPFIFOTesting`) | 16 VDP port-access tests over two pages: FIFO size/behaviour, DMA-via-FIFO, byteswapping, partial CP writes, register-write masking, read-target switching, FIFO wait states | **Yes** — scrapes the on-screen `Results: ( P/ F/ T)` line; page 1 auto-runs (settles ~frame 42), `Start` advances to page 2 (~480 more frames) | **page 1 = 9 pass / 0 fail / 9 (complete)**; **pages 1+2 cumulative = 16 pass / 0 fail / 16 — this ROM is COMPLETE** (was 9/7/16 — slice A2 flipped T13 and T10, slice A3a flipped T3, slice A3b flipped T4, slice A4 flipped T6, slice T12 flipped T12, slices T16/S1 + T16/S2 flipped T16; Test 16's byte matrix had already improved 54→18 red cells with the A1 live FIFO flags — see notes below) | The harness remains non-gating on this ROM (the CHARTER line stands, and `conformance_roms.rs`'s header still states it); what changed on 2026-08-03 is that the owner asked for this ROM's rows to be worked, so they are now being fixed slice-by-slice (A1 = live FIFO EMPTY/FULL flags; A2 = control-port / code-register edges; A3a = DMA payload words through the FIFO ring; A3b = the DMA-fill trigger write + `address ^ 1` fill bytes; A4 = the 8-bit VRAM read target, all 2026-08-03). Per-test today — page 1: **T1-T9 all pass**; page 2: **T10-T16 all pass**. **A4 flipped T6 "8-bit VRAM Read target 01100"** — CD `%001100` returns `vram[address ^ 1]` in the low byte and the next-available FIFO entry's high byte in the high byte (see the A4 addendum below); it is currency-neutral and changed no existing test. **A3a flipped T3 "DMA Transfer using FIFO"** and **A3b flipped T4 "DMA Fill FIFO Usage"** (see the A3a/A3b notes below). A3b was expected to move `export_state_v1::GOLDEN_HASH` and **does not** — the design note's §4.1 mis-identified the golden fixture (it is `testrom::build`, which never touches the VDP; the DMA-filling ROM is `testrom::build_pad_poll`, used only by the io/watchpoint tests). Every currency gate is byte-identical across A3b. **A2 flipped T13 "Register Writes and Code Reg" and T10 "Partial CP Writes"** (see the A2 note below), and left **T12 "Register Write Mode4 Mask"** as a named residual, **which slice T12 has now FIXED (2026-08-03)**: in Mode 4 (reg 1 bit 2 = M5 clear) only the eleven SMS registers 0–10 are writable, and writes above 10 are discarded (one line in `Vdp::write_register`). **CORRECTION — the reason this was held was false.** The claim that the fix "moves `export_state_v1::GOLDEN_HASH` and the `golden_frames` scenes" rested on the *same* fixture mis-identification as A3b's: it cited `testrom.rs`'s `reg 1 = $8150`, which lives in `testrom::build_pad_poll` (used only by `io_controllers`, `watchpoints` and the `pad_probe` example — no frozen currency), whereas `export_state_v1` loads `testrom::build`, which drives no VDP port at all. Re-measured: **no frozen constant moves.** 66 tests did go red, every one the same fixture defect — fixtures that write `reg 1 = $40` (M5 clear) and then program registers 11+, i.e. they intend Mode 5 but never declare it, a machine state that cannot exist on hardware. Declaring M5 in them (test-only, 8 sites) brought all frozen hashes back **byte-identical**. See `docs/2026-08-03-decision2-premise-recheck.md` and the T12 addendum below. The spec test `vdp::tests::mode4_ignores_register_writes_above_ten` is no longer `#[ignore]`d. **Evidence caveat:** the `> 10` boundary is extrapolated from a hedged source — follow-up **F-M4REGS**. **T16 "FIFO Wait States" PASSES — all 80/80 verdict bytes green (was 26/80, then 62/80, then 72/80).** Two independent causes, fixed in two slices, and *neither* was the "Phase 3 per-line DMA cost" deferral this residual had been filed under. **T16/S1** gave the FIFO drain the *positions* of the external access slots within an active display line instead of a uniform per-line rate, flipping groups 2/3/5/6/8 [62→72]. **T16/S2** made a finished 68k→VDP DMA leave words **pending** in the FIFO — the DMA unit's job ends when the last word is pushed into the FIFO, not when it reaches VRAM — flipping groups 9/10 [72→**80**]; that reverses A3a's deliberate ring-store-only choice and answers design question Q1 of `docs/2026-08-03-a3-dma-fifo-design.md`, which A3a had explicitly deferred to here. (**A3a landed DMA-through-FIFO ring *contents* and left T16 unmoved at 62/80** — which is exactly what identified groups 9/10 as needing *occupancy* rather than contents.) See the S1 and S2 addenda below. |
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

**T12 "Register Write Mode4 Mask" — RESIDUAL, not fixed.**
> **SUPERSEDED 2026-08-03 — T12 is now FIXED; see the T12 addendum below.** The "it moves frozen
> currency" sentence in this paragraph is **false** and was retracted by measurement: it cites
> `testrom::build_pad_poll` (no frozen currency), not the `export_state_v1` golden fixture
> `testrom::build`, which drives no VDP port at all. Nothing moved. The 52 red unit tests were real but
> were all fixtures declaring Mode 4 while programming Mode-5 registers. The `#[ignore]` is gone.
> The mechanism described below is correct and is what shipped.

Table `$20EC`; sequence at ROM `$2244` sets
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

> **RESOLVED 2026-08-03 by slice T16/S2 (addendum below).** The diagnosis above was exactly right; the
> parenthetical guess about the *remedy* was not. Occupancy needed neither a non-synchronous DMA nor an
> occupancy model decoupled from the drain clock — it needed the drain clock **anchored at the transfer's
> end instant**, which is two lines and makes the pending entries real rather than phantom. `dma_write_word`
> now calls `fifo_enqueue`. T16 62/80 → 80/80 across S1+S2, `vdp_port_access` 15/1/16 → **16/0/16**, with
> every frozen currency constant byte-identical.

**T4 "DMA Fill FIFO Usage" — RESIDUAL at A3a, fixed by A3b (below).** Expected table `$DC54`; we differed at
exactly words 8 and 13 (`1212` vs `1234`, `0000` vs `0012`) — precisely the design's §4.3 prediction, and
its snoop half (words 0-7) already passed.

### A3b addendum (2026-08-03) — the DMA-fill trigger write and `address ^ 1`; T4 fixed

Slice A3b, implementing the test-4 half of `docs/2026-08-03-a3-dma-fifo-design.md` (§3.4 change 3, §3.5
change 4). Scorecard movement: `vdp_port_access` page 1 went **7/2/9 → 8/1/9**, cumulative
**12/4/16 → 13/3/16**; every other row byte-identical, and **all frozen currency suites green with their
existing constants**.

**T4 "DMA Fill FIFO Usage"** (name `$DC30`, expected table `$DC54` =
`0000 0000 0000 0000 0000 0000 1000 1000 1234 1212 1212 1212 1212 0012 0000 0000`). The 16 words split in
half: words 0-7 are VSRAM-read snoop probes (already green — the fill's trigger takes exactly one FIFO slot
and the fill's replicated bytes take none), words 8-15 are the settled VRAM image at `$8000`. Two pins:

* **P2 — the trigger is an ordinary write.** The data-port write that fires a pending fill is not swallowed:
  it is completed as a normal write to the current target (VRAM: MSB → `address`, LSB → `address ^ 1`) and
  the address then auto-increments. Nemesis, *VDP Internals* (SpritesMind): "When a DMA Fill operation is
  pending, and you perform a data port write, that data port write is completed as normal… That pending
  write is then pulled out of the FIFO, and processed as a normal FIFO write." Only a full word write can
  put the trigger's LSB `$34` at `$8001`, and only the autoincrement stops the fill's first step from
  overwriting it again — both forced by the table.
* **P3 — fill bytes land at `address ^ 1`.** Mask of Destiny, *Is DMA Fill buggy?* (SpritesMind): "MSB of
  the word in the FIFO is written DMA length times to address ^ 1"; Eke, same thread: "VRAM byte writes
  (used by VRAM fill and copy DMA) actually occur to VRAM address ^ 1 so you can get unexpected results
  depending on start address, DMA length and increment alignments." With `$8000`, autoinc 1, length 10 the
  ten steps run over `$8001..$800A` and write `{$8000} ∪ {$8002..$8009} ∪ {$800B}` — `$800A` skipped,
  `$800B` written. That union *is* words 8-15 of the table.

**Correction to the design note (and to Decision 1 of the parked-ruling doc): `GOLDEN_HASH` does NOT move.**
Design §4.1 asserted that `crates/oracle-core/src/testrom.rs:255-263` is the golden fixture and clears VRAM
with a `$FFFF`-byte DMA fill, so `vram[$FFFF]` kept a power-on `$3B` that the fix would zero. Those lines
are in **`testrom::build_pad_poll`**, a *different* fixture used only by `io_controllers.rs`,
`watchpoints.rs` and the `pad_probe` example. The `export_state_v1` golden fixture is **`testrom::build`** —
the RAM-stirring ROM at `$200` that never writes a VDP port at all (probed directly: after the golden's 60
frames every VDP register is `$00` and VRAM is untouched power-on noise, 254 zero bytes out of 65536).
`GOLDEN_HASH` stays `0xBF5D_1E1A_A727_143B`, unchanged, and A3b landed with **zero currency movement**.
The pad-poll fixture's VRAM does gain one zeroed byte at `$FFFF`, which is outside every plane/SAT window,
so its rendered frame and its watchpoint counts are unchanged (both suites green unmodified).

**Existing tests rewritten** (all three were asserting the buggy image, exactly as design §4.3 predicted):
`bus::tests::vram_fill_fills_the_target_with_the_top_byte` (`$0100..$0109` is now
`EE AA EE EE EE EE EE EE <untouched> EE`), `bus::tests::fill_updates_the_sat_cache_on_window_hits`
(`sat_cache[0..4]` = `77 AA 77 77`; the point of the test — fill bytes hit the write-through window — is
unchanged), and `vdp::tests::armed_captures_a_dma_fill_with_via_dma` (capture addresses `$0201 $0200 $0203
$0202`). `vdp::tests::cram_fill_uses_the_four_writes_ago_entry` is untouched and still green, which is the
guard that the trigger change did not leak into the CRAM/VSRAM fill data source.

**Not changed, deliberately (design Q2):** `run_copy` still writes at `address`, not `address ^ 1`, even
though Eke's quote covers copy too — no test in the vendored suite exercises it, and an unevidenced change
there could move visual baselines with nothing to justify it. Registered as follow-up **F-COPYXOR** in
*Named follow-ups* below.

**Invalid-target guard, and the asymmetry it exposes.** Design §3.4 shows the trigger write unconditional;
it shipped guarded by `matches!(code & 0x0F, 0x1 | 0x3 | 0x5)`, the same invalid-target rule the non-DMA
data-port path uses (T10's pin), because `target_of` falls back `_ => Vram` and would otherwise let a fill
armed on a no-write-target code scribble VRAM. `Vdp::run_fill`'s body still resolves its target through
that same fallback, so the two halves of the fill path now disagree for an invalid code — pre-existing, not
covered by any ROM, and deliberately left alone here. Registered as follow-up **F-FILLTGT** below. The
guard admitting CRAM/VSRAM means a `$23`/`$25` fill is primed too, which is extrapolated from a VRAM-only
pin — ruled deliberate and registered as **F-FILLPRIME** below.

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

### A4 addendum (2026-08-03) — the 8-bit VRAM read target; T6 fixed, page 1 complete

Slice A4. Scorecard movement: `vdp_port_access` page 1 went **8/1/9 → 9/0/9 (page 1 is now complete)**,
cumulative **13/3/16 → 14/2/16**; every other row byte-identical, and **all frozen currency suites green
with their existing constants** (`export_state_v1::GOLDEN_HASH`, `oracle_differential`, `golden_frames`,
`determinism_gate`, `singlestep_m68000`). No field was added to `state_hash` or `export_state`.

**T6 "8-bit VRAM Read target 01100."** Name string at ROM `$DEB0`, expected table at ROM `$DED4` — both
loaded by the ROM's own literal `lea`s (`00DF04: lea.l $deb0.l,a1` / `00DF18: lea.l $ded4.l,a1`), which is
the ground truth for the offsets, exactly as in A3. The table is

```
9922 9944 bb66 bb88 ddaa ddcc 12ee 1234 ff22 ff11 ff44 ff33 ff11 ff44 ff33 ff66
```

The test body was disassembled at `$DF28..$E0F0` (capstone `CS_ARCH_M68K` / `CS_MODE_M68K_000` over the
flat image). It writes eight marker words `1122 3344 5566 7788 99AA BBCC DDEE 1234` to VRAM `$8000` at
autoinc 2, then makes six groups of reads under control second-word `$0032` — CD3-CD0 = `1100`, the
undocumented **8-bit VRAM read** — separated by single ring-advancing CRAM writes of `$FFFF`. Groups 1-4
read at `$8000/$8004/$8008/$800C` with autoinc 2; then reg 15 is set to 1 and groups 5-6 read four words
each at `$8000` and at the **odd** address `$8001`.

Three clauses, each forced by the table:

* **Low byte = `vram[address ^ 1]`.** Group 5 is decisive: autoinc 1 from `$8000` over the image
  `11 22 33 44` returns `$22 $11 $44 $33`, i.e. bytes `$8001 $8000 $8003 $8002`. A plain `vram[address]`
  would return `$11 $22 $33 $44`. This is the *same* lane swap A3b pinned for the fill engine's byte
  writes — Eke, *Is DMA Fill buggy?* (SpritesMind): "VRAM byte writes (used by VRAM fill and copy DMA)
  actually occur to VRAM address ^ 1". Group 6 (odd start `$8001`) is the independent confirmation.
* **High byte = the next-available FIFO entry's high byte** — the same stale-contents snoop the CRAM/VSRAM
  undefined bits already read, making code `$0C` the third snooping target. Nemesis, *VDP Internals*
  (SpritesMind): undefined result bits "are actually initialized to the content on the next available FIFO
  entry (the one containing the data written to control port four writes ago)". After the eight marker
  writes the ring holds the last four with the cursor parked on `$99AA`; each group's `$FFFF` CRAM write
  advances it one slot, and the observed high byte walks `$99 → $BB → $DD → $12 → $FF` in lockstep. Both
  reads *within* a group return the same high byte, so **a read does not advance the cursor** — only
  writes do.
* **Autoincrement is normal.** Groups 1-4 step by 2 and groups 5-6 by 1, matching reg 15 with no
  byte-read-specific adjustment.

Verified before implementing: a standalone model of exactly these three clauses reproduces all sixteen ROM
words byte-identically, and the pre-fix red run of the replay test returned `1122 3344 5566 7788 …` — full
words, matching the independent A0 screen probe. Predicted-from-source + independently-observed agreement
is the cross-check that the whole decode chain (CD decode, `^ 1` lane, snoop cursor) is right.

**Shape of the fix.** `is_vram_byte_read(code) == (code & 0x0F == 0x0C)` is a predicate on the *read* path
only — deliberately not a new `Target` variant. Code `$0C` still decodes to `Target::Vram` for the write
path (where its low nibble names no valid write target, so a data write under it is accepted into the FIFO
and steps the address but reaches no memory — the A2 rule, and T5 "FIFO Write to invalid target" stays
green) and for the FIFO drain-cost model. The pre-cache (`read_target`) fetches the single byte at
`address ^ 1` into the low half; `data_read` merges the snoop's high byte at read time, alongside the
existing CRAM/VSRAM merges. **No existing test changed.**

**Named follow-up: F-SNOOPWHEN** (registered in the follow-up registry below) — the pre-cache/consume seam
is unpinned in both directions: *when* the snoop word is sampled, and what happens when the command code
changes between the two instants. See the registry entry for the full statement and the settling experiment.

**A4 review pass (same day).** Two behavioural notes from the review, both landed in the follow-up commit:

* `read_target`'s byte arm originally stored a **fabricated zero** in the buffer's high half. That leaked
  through the code-mismatch path above (arm `$0C`, then any `$8xxx` register write makes the code `$0E`, so
  `data_read` does not merge) and returned `$00XX` where pre-A4 the emulator returned the real VRAM word —
  an observable read-path change outside the slice's evidence. The buffer now keeps the **real** VRAM high
  byte and `data_read` masks it away, which is behaviour-identical for `$0C` (the only case the ROM pins)
  and restores the pre-A4 result on the mismatch path. Pinned by
  `vdp::tests::eight_bit_vram_read_buffer_degrades_to_the_plain_word_when_the_code_changes`.
* The FIFO drain-cost model is genuinely unchanged for `$0C` — `target_of`'s `_ => Vram` fallback still
  charges it a VRAM word's two slots — and that is now asserted rather than left to inspection.

**Page 2's two residuals at the time of A4** were T12 "Register Write Mode4 Mask" (then believed to move
`GOLDEN_HASH` + the `golden_frames` scenes — parked on an owner ruling; **that belief was wrong, and T12 is
now fixed — see the T12 addendum below**) and T16 "FIFO Wait States" (needs DMA-word FIFO *occupancy*,
design Q1, plus discrete per-line access-slot scheduling).

Sources: [Nemesis, VDP Internals p.3](https://gendev.spritesmind.net/forum/viewtopic.php?t=1291&start=43) ·
[Is DMA Fill buggy?](https://gendev.spritesmind.net/forum/viewtopic.php?t=2663).

### T12 addendum (2026-08-03) — the Mode-4 register mask; T12 fixed, T16 is the last residual

Slice T12. Scorecard movement: `vdp_port_access` cumulative **14/2/16 → 15/1/16**; page 1 unchanged at
**9/0/9** (T12 is a page-2 test). Every other `BASELINE` row byte-identical, and **all frozen currency
byte-identical with their existing constants** — `export_state_v1::GOLDEN_HASH`, `oracle_differential`,
the six `golden_frames` scene hashes, `determinism_gate`, `singlestep_m68000`, and every other
`VISUAL-BASELINE frame_hash=` row. No `state_hash` / `export_state` field added; no version bump.

**The behaviour.** In Mode 4 — reg 1 bit 2 = M5 **clear**, the SMS mode — only the eleven SMS registers
0-10 are writable; a write to any register above 10 is discarded. T12 (name `$20C8`, expected table
`$20EC`, sequence `$2244`) sets reg 15 = 4 inside a Mode-4 window and reads back an autoincrement still at
its Mode-5 value. Source: [Kabuto's hardware notes](https://plutiedev.com/mirror/kabuto-hardware-notes),
"All registers except for the 10(?) SMS registers are disabled." The fix is one line in
`Vdp::write_register`, placed after the `reg >= REG_COUNT` guard.

**Evidence honesty — the boundary is extrapolated.** The ROM pins **register 15 and nothing else**; the
`> 10` rule generalises Kabuto's own hedged phrasing (the "(?)" is his). Masking 11-14 and 16-23 — the DMA
registers 19-23 included — is therefore inference, not a hardware answer. Shipped uniform because the
alternative is inventing an equally unevidenced special case, and registered as follow-up **F-M4REGS**
below with the experiment that would settle it.

**Why this was parked, and why the parking reason was false.** `docs/plans/2026-08-03-PARKED-owner-ruling.md`
and this ledger both held that the fix moves `export_state_v1::GOLDEN_HASH` and the `golden_frames` scenes.
Re-measured in throwaway trees before any code was written
(`docs/2026-08-03-decision2-premise-recheck.md`) and confirmed here: **it moves neither.** The claim cited
`testrom.rs`'s `reg 1 = $8150`, which is inside `testrom::build_pad_poll` — consumed only by
`io_controllers`, `watchpoints` and the `pad_probe` example, none of them frozen currency — while
`export_state_v1` loads `testrom::build`, which drives no VDP port at all (all 24 registers read `$00`
after 60 frames). This is the **same** mis-identification that sank A3b's predicted `GOLDEN_HASH` move.
Two for two: *"a test goes red"* and *"the currency moves"* are different events, and only measurement
tells them apart.

**What did go red: 66 tests, all one fixture defect.** **57 in the lib suite, measured firsthand with the
fix applied and the fixtures untouched** — `render::tests` 45, `bus::tests` 5, `vdp::tests` 4,
`z80::bus::tests` 2, `system::tests` 1. (The premise re-check measured 53; the extra four are A4's T6
tests, which postdate it.) The remaining 9 are integration — `golden_frames` 6, `watchpoints` 2,
`io_controllers` 1 — a count carried from the re-check rather than re-measured here, since their fixtures
were repaired in the same pass; what *was* verified directly is that all 9 pass afterwards.
Every one was a fixture that writes `reg 1 = $40`/`$00`/`$50` (M5 clear = Mode 4) and then programs H40,
the window, plane bases, autoincrement or DMA — Mode-5-only state. **They were configuring a machine that
cannot exist on hardware**, and the mask simply made that visible. The repair is test-only and mechanical:
declare M5 in the five `fresh()`-style helpers, keep bit 2 set in the ten explicit reg-1 writes. After it,
all 705 lib tests and all eight integration targets pass with **every pinned constant untouched** — which
is the real proof the diagnosis was right, since a genuine behavioural regression could not restore a
frozen hash byte-for-byte.

`testrom::build_vram_poke` needed no change: its only register-11+ write is autoinc, and it performs a
single word poke whose two bytes land regardless of the autoincrement.

**T16 "FIFO Wait States" is now the sole remaining `vdp_port_access` failure.**

(Superseded 2026-08-03 by slices T16/S1 and T16/S2, below: **T16 passes and this ROM is 16/0/16.**)

Sources: [Kabuto's hardware notes (Plutiedev mirror)](https://plutiedev.com/mirror/kabuto-hardware-notes) ·
the ROM's own expected table at `$20EC`.

### T16/S1 addendum (2026-08-03) — intra-line access-slot positions; groups 2/3/5/6/8 fixed

Slice T16/S1. Scorecard movement: **none** — `vdp_port_access` stays `page1 9/0/9; cumulative 15/1/16`,
because T16 is still FAIL on groups 9/10. What moved is T16's verdict-byte count, **62/80 → 72/80**. Every
other conformance row, **every** `VISUAL-BASELINE frame_hash=` row, and **all frozen currency suites are
byte-identical with their existing constants** (`export_state_v1::GOLDEN_HASH`, `oracle_differential`,
`golden_frames`, `determinism_gate`, `singlestep_m68000` 113/113). No field was added to `state_hash` or
`export_state`. Design and measurements: `docs/2026-08-03-t16-slot-scheduling-recon.md`.

**CORRECTION — "a genuinely larger piece of work" was wrong, and this ledger said it.** The scorecard row
above and `docs/plans/2026-08-03-PARKED-owner-ruling.md:207-209` both filed T16's remaining reds under the
long-standing **"Phase 3 per-line DMA cost"** deferral. (`docs/plans/2026-08-03-fifo-scanline-arcs.md` does
*not* — it says only "fixes T16" and targets 16/16; it was cited here in error and the citation is
withdrawn.) That conflated three separate things:

* **(A) intra-line slot positions** — *missing*, and the whole of groups 2/3/5/6/8. **This slice.** ~50
  lines of table-driven integer code in `Vdp::next_active_slot` / `Vdp::entry_drain_cost`.
* **(B) post-DMA FIFO occupancy** — *missing deliberately* (A3a §3.3, design question Q1), and the whole of
  groups 9/10. Two lines. Not slot scheduling at all. Slice T16/S2.
* **(C) per-line cost integration** — the actual Phase-3 deferral: `Vdp::dma_cost` samples `slots_per_line`
  **once**, at the transfer's start instant, so a DMA that starts in vblank and runs into active display is
  mis-billed by up to ~11×. **T16 does not need it**, it changes DMA elapsed time for every DMA-using ROM,
  and that is where the visual-baseline risk actually lives. It stays deferred (recon §3.4 "S3").

**What the ROM asks.** T16's expected table is 80 bytes — 40 words, 10 groups × 4 — at ROM **`$ED10`**, found
via the record builder's own literal `lea`s (`00ED70: lea.l $ecec.l,a1` = the 36-byte name string
"FIFO Wait States"; `00ED84: lea.l $ed10.l,a1`; `00ED6A: move.w #$50,d6` states the 80-byte length). Per
group the words are probe 1, then the first FULL / first PARTIAL / first EMPTY seen in the 16-word status
stream that follows the group's *inserted operation*; `$ffff` = "never observed in 2048 retries". Groups
1-8 share a stimulus — a VRAM-write command to `$8000` then **six** data-port writes into a 4-deep FIFO —
and differ only in what is inserted between the two probes (nothing / one control word / a control pair /
a control pair plus data writes / a control pair plus a data-port *read*).

**The mechanism, measured.** The sixth write /DTACK-stalls, which **phase-locks the CPU to the drain
clock**: whatever h/v position a retry starts at, the 68k resumes exactly at a drain instant. With uniform
slot spacing the post-insert probe then lands at a *fixed* offset past the next drain instant on every
retry — 15 mclk for a 20-cycle inserted control word, 71 mclk for a 28-cycle control pair. 1815 of 1815
active-display retries produced the identical miss. The ROM's 2048 retries buy hardware 2048 different slot
phases; they bought us 2048 copies of one phase.

**The fix, and its source.** External access slots are not evenly spread across a line. Kabuto's hardware
notes publish the per-line access pattern as a string:

```text
H40: Hssss AsaaBsbb ((A~aaBSbb)*3 AraaBSbb)*5 ~~ s*23 ~ s*11        (210 accesses)
H32: Hssss AsaaBsbb ((A~aaBSbb)*3 AraaBSbb)*4 ~~ s*13 ~ s*13 ~      (171 accesses)
```

`~` = external (CPU/DMA) slot, `r` = VRAM refresh. Expanding them gives exactly **18 `~` + 5 `r`** (H40)
and **16 `~` + 4 `r`** (H32) — which reproduces the Sega *Genesis Technical Overview* DMA-capacity table's
18/16 figures (already pinned at `docs/2026-07-16-vdp-recon.md:109`) from an independent source. The H40
gap sequence is `(8,8,16)×4 | 8,8,15 | 1 | 24 | 26` accesses — the fifth render group's last gap is 15, not
16, because the pair at 173/174 begins one access early, and written that way it sums to the line's 210
accesses, which is the arithmetic cross-check. The wrap-around gap of **26** matches TascoDLX's
measured figure (SpritesMind t=851 — the manual's 16-slot maximum gap is wrong; "the largest gap is
actually 26 slots"), which is independent corroboration that Kabuto's string transcribes Nemesis's
logic-analyser measurements. `Vdp::entry_drain_cost` now walks that schedule on active lines, so a VRAM
word drains in anywhere from **260** mclk (a drain beginning *on* a slot instant and taking a render
group's two close slots, `t30 - t14` = 488 − 228) to ~814 (one beginning just past the line's last slot, so
both its slots come from the next line) instead of an invariant 380. **The per-line total is
unchanged by construction** — the table has exactly 18/16 entries — so this redistributes drains within a
line, it does not add or remove capacity. That is why it is currency-free where a crude uniform fudge is
not: the recon measured that an arbitrary `entry_drain_cost += 80` reaches the same 72/80 *and* moves two
visual baselines (`m68k_opcode_sizes`, `window_distortion`), while the real table moves none.

`vdp::tests::active_slot_gaps_follow_the_published_pattern` re-expands both strings and checks the
shipped index constants against them, plus the external/refresh counts — a transcription typo would
otherwise surface three layers downstream as a moved frame hash.

**Acceptance tests.** `bus::tests::vdpfifo_t16_*` replay T16's own port sequences through `MegaDriveBus`
(instruction costs 20 / 16 / 20 CPU cycles, each corroborated by the measured 140 / 112 / 140 mclk gaps in
the recon's `BusEventSink` trace) and sweep the retry across a scanline the way the ROM's 2048 retries do.
Group 1 (nothing inserted) and group 7 (an inserted data-port read → the FIFO drains, so `$ffff ffff` is
*correct*) are the two controls: both passed before this slice and still pass.

Sources: [Kabuto's hardware notes (Plutiedev mirror)](https://plutiedev.com/mirror/kabuto-hardware-notes) ·
[Nemesis / TascoDLX / Eke, VRAM access timing (t=851)](https://gendev.spritesmind.net/forum/viewtopic.php?t=851) ·
[Sega Genesis Technical Overview (1991)](https://segaretro.org/images/1/18/GenesisTechnicalOverview.pdf) ·
the ROM's own expected table at `$ED10`.

### T16/S2 addendum (2026-08-03) — post-DMA FIFO occupancy; T16 passes, this ROM is complete

Slice T16/S2. Scorecard movement: `vdp_port_access` **`cumulative 15/1/16 → 16/0/16`** (page 1 unchanged at
`9/0/9` — T16 is a page-2 test). T16's verdict bytes **72/80 → 80/80**. Every other conformance row, **every**
`VISUAL-BASELINE frame_hash=` row — including `m68k_opcode_sizes` `0x5436cda5786ea450` and
`window_distortion` `0x5102219d295b4e2c`, the two rows measurably sensitive to FIFO stall timing — and
**all frozen currency suites are byte-identical with their existing constants** (`export_state_v1::GOLDEN_HASH`,
`oracle_differential`, `golden_frames`, `determinism_gate`, `singlestep_m68000` 113/113). No field was added
to `state_hash` or `export_state`.

**This closes design question Q1 of `docs/2026-08-03-a3-dma-fifo-design.md`, by reversing A3a's choice.**
A3a fed DMA payload words through `Vdp::fifo_store` — the bare ring write — so a word took a physical FIFO
slot (which VDPFIFOTesting test 3 pins, through the undefined-bit snoop) **without** bumping the pending
count. Its stated reason: our Mem DMA runs synchronously inside the triggering bus access and bills the
whole transfer through `Vdp::dma_cost` plus the returned halt wait, so pending entries would be phantoms no
clock had advanced past — a spurious /DTACK stall on the next data-port write in every DMA-using ROM, for no
test-3 or test-4 benefit. A3a was right to defer it and right to name it Q1.

**What the ROM says.** T16 groups 9 and 10 (expected table ROM `$ED10` words 33-40, `0200 0100 0000 0200`
and `0000 0100 0000 0200`) fire a 68k→VRAM DMA *between* their two probes and require the resuming 68k to
observe **FULL → partial → EMPTY**. Group 9 uses an 8-word DMA from an empty FIFO; group 10 a 3-word DMA
onto a FIFO already holding three words — which is the case that pins saturation at 4 rather than "the
transfer leaves exactly `count` entries". With the bare ring store the CPU saw EMPTY on every read and both
groups reported the `$ffff` "never observed in 2048 retries" sentinel.

**Why hardware behaves that way.** The DMA unit's job ends when the last word is pushed *into the FIFO*, not
when it reaches VRAM. Nemesis, *VDP Internals*: a DMA "will read a value from external memory using the DMA
source address register and **add it to the FIFO** using the current command code and incremented command
address registers" — the same sentence A3a quoted for the ring store. So a transfer completes with up to
four words still queued, and the CPU that resumes can see them.

**The phantom-stall objection is answered, not overruled.** Two changes, both in `vdp.rs`:

* `Vdp::dma_write_word` calls `fifo_enqueue` instead of `fifo_store` — the payload word is pending, and
  `fifo_len` saturates at 4 as it does for CPU writes.
* `Vdp::dma_complete` anchors `fifo_slot_clock` at `busy_until` — the transfer's **end** instant — while
  entries remain pending. Without this the residual would be measured against a slot clock still sitting at
  the transfer's *start* and would appear to have drained the moment the 68k resumed, which is precisely
  A3a's objection. With it the entries are not phantom: they are the words that genuinely have not reached
  VRAM, the stall they produce is the real one, and the residual drains from the end of the transfer
  exactly as groups 9/10 observe. `bus.rs`'s `run_mem_dma` needed no edit — it already passes `now + cost`.

**Carried forward.** The total halt stays at `count × slots × rate` while up to four of those words are now
*also* accounted as pending, so the last four are billed twice; physically the DMA unit should release the
bus about four slots earlier. The ROM cannot see it (it measures FIFO state, not transfer duration) and
shortening the halt would change DMA timing for every ROM. Registered as follow-up **F-DMAHALT**.

**Test rewritten, not deleted.** `vdp::tests::mem_dma_ring_store_does_not_add_pending_entries` existed
specifically to guard the A3a choice this slice reverses. It is replaced by its inverse,
`vdp::tests::mem_dma_leaves_the_fifo_full`, citing ROM `$ED10` groups 9/10 in its comment, plus
`vdp::tests::post_dma_fifo_drains_from_the_transfer_end` for the clock anchor. The acceptance tests are
`bus::tests::vdpfifo_t16_group9_a_finished_dma_leaves_the_fifo_full` and
`…_group10_a_short_dma_onto_a_partial_fifo_also_leaves_it_full`, which replay the ROM's own port sequences
through `MegaDriveBus`.

**Manual re-check — OWED, then DISCHARGED CLEAN (overseer, 2026-08-03).** The DR-1/DR-2/DR-3 differential
ROMs (Gunstar Heroes, Thunder Force IV, Batman — `docs/2026-07-22-differential-rom-findings.md`) have no
automated conformance row and are all heavy DMA users. S2 changes what the FIFO looks like immediately after
**every** DMA, so they had to be run by hand; they are not vendored, so the implementing slice could not do
it. The overseer ran the A/B directly: `examples/boot_rom`, 600 frames each, built at `5cac0fa` (pre-T16)
and at the T16 merge, comparing `ppm` / `vram` / `cram` / `vsram` / `ram` / `z80` per ROM. **All 18
comparisons byte-identical.** S1+S2 are confirmed inert on the three ROMs the DR-1/2/3 slices exist to
support. **Re-run after the review pass** (`631dbd0` changed `dma_complete`'s anchor to `.max()`, a real
behaviour change, so the earlier discharge no longer covered the shipped code): same three ROMs, same
method, against the same `5cac0fa` baseline — **all 18 comparisons byte-identical again.**

Sources: [Nemesis, VDP Internals p.3](https://gendev.spritesmind.net/forum/viewtopic.php?t=1291&start=43) ·
the ROM's own expected table at `$ED10`.

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
* **F-COPYXOR — does VRAM *copy* also write to `address ^ 1`?** (Registered 2026-08-03 by slice A3b;
  design question Q2 of `docs/2026-08-03-a3-dma-fifo-design.md`.) A3b changed the **fill** engine to write
  its byte at `address ^ 1` (`Vdp::run_fill`, VRAM arm), pinned by VDPFIFOTesting test 4's own expected
  table at ROM `$DC54`. Eke's SpritesMind quote says the quirk covers copy as well — "VRAM byte writes
  (used by VRAM fill **and copy** DMA) actually occur to VRAM address ^ 1" — but **no ROM in
  `vendor/TestRoms/` exercises DMA copy**, so `Vdp::run_copy` was deliberately left writing at `address`.
  Changing it on that citation alone would move visual baselines with no test to justify or bound the
  move. Also unresolved in the same breath: whether the copy's *source* byte read is likewise `^ 1`.
  Needs a ROM or a BlastEm instrument before it lands, not a forum quote. Currency-relevant (it touches
  the VRAM write path used by every copy-using game).
* **F-FILLTGT — the fill path's two target decodes disagree on an invalid code.** (Registered 2026-08-03
  by slice A3b; pre-existing asymmetry that A3b's guard made visible, deliberately NOT fixed there.)
  **Code anchor: `Vdp::code_names_a_write_target` in `crates/oracle-core/src/vdp.rs`** — grep that name to
  find both sides of this. The fill *trigger* write in `Vdp::apply_data_write` is guarded by that predicate
  — the same invalid-target rule the non-DMA data-port path uses (pinned by VDPFIFOTesting test 10, ROM
  `$FCAA` word 7) — so a code naming no write target takes its FIFO slot and steps the address but reaches
  no memory. The fill *body* in `Vdp::run_fill` still resolves its target through `Vdp::target()`, whose
  `target_of` falls back `_ => Vram` for any unrecognised low nibble. Net effect: a fill armed on a
  no-write-target code has its trigger write suppressed yet its fill body still scribbles VRAM. One of the
  two decodes is wrong; the ROM does not cover the case, so which one is unpinned. Resolve by pinning the
  invalid-code fill behaviour from a ROM or an instrument, then making both paths share one decode.
* **F-FILLPRIME — CRAM/VSRAM fill priming is extrapolated from a VRAM-only ROM pin.** (Registered
  2026-08-03 by slice A3b; design question Q3. Code anchor: `Vdp::code_names_a_write_target`, and the
  as-shipped pin `vdp::tests::fill_trigger_primes_a_cram_fill_target`.) A3b made the fill's trigger
  data-port write land in memory instead of being swallowed. The guard admits all three write targets, so a
  fill armed with code `$23`/`$25` now writes its trigger word into CRAM/VSRAM as well. **That half is
  extrapolated, not pinned:** VDPFIFOTesting test 4's expected table (ROM `$DC54`) exercises a VRAM fill
  only, and Nemesis's statement of the rule — "that data port write is completed as normal" — names no
  target. **No ROM in `vendor/TestRoms/` exercises a `$23`/`$25` fill at all.**
  *Owner ruling, 2026-08-03: keep the uniform behaviour.* The clause the ROM pins is "the trigger is
  completed as a *normal full-word write*"; target selection is orthogonal to that clause, and the normal
  write path already handles all three targets. Restricting to VRAM would mean **adding** a special case
  with no evidence of its own, which is a larger unevidenced step than applying a pinned rule consistently.
  (Deliberately a different call from **F-COPYXOR**: there the evidence points *at* a change we declined
  for want of a ROM that exercises it; here the evidence pins a general rule and only its reach is open.)
  *What would settle it:* a ROM or BlastEm instrument that arms a CRAM or VSRAM fill and reads the armed
  entry back — if the entry holds the trigger word, the current behaviour is right; if it is untouched, the
  priming write is VRAM-only and the guard needs a `Target::Vram` clause. Low practical risk (a CRAM/VSRAM
  fill is already a documented hardware-bug path nothing sane uses), but genuinely unverified.
* **F-M4REGS — which registers the Mode-4 mask actually covers.** (Registered 2026-08-03 by slice T12.
  Code anchor: the mask in `Vdp::write_register`, `crates/oracle-core/src/vdp.rs`, and the pin
  `vdp::tests::mode4_ignores_register_writes_above_ten`.) As shipped, with M5 clear (reg 1 bit 2 = Mode 4)
  **every** register above 10 is discarded — 11-14, 15, and 16-23, the DMA registers 19-23 included.
  **What the ROM pins is register 15 only:** VDPFIFOTesting test 12 (`$20C8`, expected table `$20EC`,
  sequence `$2244`) writes reg 15 = 4 inside a Mode-4 window and observes the autoincrement still at its
  Mode-5 value. The rest of the range rests entirely on Kabuto's hardware notes, which **hedge the
  boundary in the sentence itself**: "All registers except for the **10(?)** SMS registers are disabled."
  The question mark is the author's. So `reg > 10` is a uniform-rule inference from a hedged secondary
  source, not a hardware pin, and the DMA registers in particular are masked on no direct evidence at all.
  *Why it shipped uniform anyway:* narrowing to "register 15 only" would mean **inventing** a special case
  the evidence does not support either, and every published description of Mode 4 describes the SMS
  register file as 0-10 — the same call as **F-FILLPRIME** (apply the pinned rule consistently rather than
  add an unevidenced exception). *What would settle it:* a ROM or a BlastEm instrument that, in Mode 4,
  writes each register 11-23 to a value distinguishable from its Mode-5 content, returns to Mode 5, and
  reads back the effect — a DMA register is the sharpest probe, since arming a DMA with a length written
  while in Mode 4 is directly observable. Low practical risk (real software sets M5 before programming any
  Mode-5 register), but it is the one part of this slice that is extrapolation rather than a ROM answer.
* **F-SNOOPWHEN — the read pre-cache/consume seam is unpinned.** (Registered 2026-08-03 by slice A4's
  review pass. **Code anchors: `Vdp::read_target` and `Vdp::data_read` in `crates/oracle-core/src/vdp.rs`**
  — the two instants are the pre-cache fill and the snoop merge; the as-shipped pin is
  `vdp::tests::eight_bit_vram_read_buffer_degrades_to_the_plain_word_when_the_code_changes`.) A read command
  fills the buffer when it completes, but `data_read` decides *at consume time*, from the live `self.code`,
  which bits are undefined and what to substitute. Two things about that seam are unpinned:
  1. **When is the snoop word sampled?** We read `fifo_snoop_word()` at consume time, consistent with the
     CRAM/VSRAM merges. Sampling at pre-cache time is equally consistent with the citations. VDPFIFOTesting
     test 6 never performs a FIFO write between arming the read and consuming it — and neither does any
     other test in `vendor/TestRoms/` — so no expected table can distinguish the two.
  2. **What if the code changes between the two instants?** A2's own pinned rule makes this reachable: any
     `$8xxx` register write latches CD1-CD0 from its top bits `10`, so arming the 8-bit VRAM read `$0C` and
     then writing a register leaves code `$0E`, for which `is_vram_byte_read` is false and no merge fires.
     Hardware might re-derive the whole result from the new code, keep the armed code, or something else
     again. **Our choice is "preserve the pre-A4 behaviour"** — the buffer holds the real full word, so the
     mismatch path returns the plain VRAM read rather than a value A4 invented. That is a conservative
     default where evidence is absent, *not* a hardware pin.
  *What would settle it:* a probe (ROM or BlastEm instrument) that arms each snooping read code, then
  between arming and reading (a) enqueues a FIFO write, and (b) clobbers the code register, and reads the
  result back. Both sub-questions fall out of the same experiment. Behavioural only — the seam is not in
  either currency.
* **F-REPLAYFN — factor a shared `VdpReplay` test harness.** (Registered 2026-08-03 by slice A4's review
  pass; the reviewer reversed an earlier deferral and judged it now factorable, and the owner scheduled it
  separately rather than in A4's follow-up commit because T12 was in flight and T16 lands next, both adding
  replays that a broad test refactor would collide with.) **Code anchor: the three
  `bus::tests::vdpfifo_t{3,4,6}_*` replay tests in `crates/oracle-core/src/bus.rs`.** The evidence for the
  refactor, recorded so the next slice does not have to rediscover it: all three share (a) the same
  `MdMem::new` + `now_mclk = 250 * MCLK_PER_LINE` + `sink` + `observed` preamble, (b) byte-identical `ctrl`
  and `data` closures, (c) 14 `observed.push(bus.read16(0xC0_0000, 5).0)` sites between them, and (d) the
  ring-advancing `$FFFF` CRAM-write triplet (`ctrl $C020`, `ctrl` second word, `data $FFFF`) common to all
  three. `MegaDriveBus::new` takes only `&mut` borrows, so a helper needs no borrow gymnastics. Tests only —
  no production code, no currency.
  *Amended 2026-08-03 by slice T16 — the scope grew, and partly shrank.* T16 added **six** more replays
  (`bus::tests::vdpfifo_t16_group*`), so the anchor is now the nine `bus::tests::vdpfifo_t{3,4,6,16}_*`
  tests. Two pieces of the eventual harness already exist and the refactor should adopt rather than
  re-invent them: `vdp_port_write` / `vdp_port_read`, which perform one port access **and fold the returned
  /DTACK wait into `now_mclk`** the way `System::step_cpu` does, and `t16_classify`, which replaced three
  copy-pasted "first FULL / PARTIAL / EMPTY" closures. The wait-folding matters beyond tidiness: T16's
  review found that the older fixed-`now_mclk` fixtures let the FIFO drain clock run ahead of the bus
  clock, which is a state no real 68k can reach, and `vdpfifo_t3_dma_payload_walks_the_fifo_ring` was
  converted to the helpers for exactly that reason. Any harness must keep that property.

* **F-SLOTGRID — an access slot's mclk position is a uniform grid, and hardware's is not.** (Registered
  2026-08-03 by slice T16/S1; recon question Q1. **Code anchor: `Vdp::next_active_slot` in
  `crates/oracle-core/src/vdp.rs`** — the line `k * MCLK_PER_LINE / accesses`.) S1 places access *k* at
  `k × 3420 / total`, i.e. it assumes the line's 210 (H40) / 171 (H32) accesses are evenly spaced. They are
  not: 210 × 16 mclk = 3360 ≠ 3420, because EDCLK slows around hsync. Eke (SpritesMind t=851): "EDCLK is
  not always MCLK/5 during HSYNC, it's actually variating between MCLK/5 and MCLK/4 around HSYNC"; TmEE's
  nesdev timing table describes the H40 line as "30 slow pixels, 4 medium pixels, and 386 fast pixels".
  Both descriptions total 3420 and disagree on the microstructure, and no source found reconciles them. The
  *ordering and count* of slots — which is what T16 measures — is unaffected; only their exact instants
  are. **This is the choice `docs/2026-08-03-t16-slot-scheduling-recon.md` §4.4 flags as the one whose
  variants must re-prove currency-neutrality**: a different mapping could plausibly move
  `m68k_opcode_sizes` or `window_distortion`, the two frame-hash rows measurably sensitive to FIFO stall
  timing. *What would settle it:* a source that pins the per-pixel clock division across the H40 line, or a
  BlastEm instrument timing a full-FIFO stall at known h positions near hsync. Implement it as a
  position→mclk function and re-run every currency gate; do **not** regenerate a baseline if one moves.
* **F-BLANKSLOT — blanked lines keep the aggregate-rate model.** (Registered 2026-08-03 by slice T16/S1;
  recon question Q3. **Code anchor: the early return in `Vdp::entry_drain_cost`,
  `crates/oracle-core/src/vdp.rs`**, and the pin `vdp::tests::blanked_lines_keep_the_aggregate_slot_rate`.)
  S1 gives *positions* to active-display drains but leaves vblank / display-off drains on the old
  `slots × 3420 / slots_per_line` rate. That is an acknowledged inconsistency in the model, not an
  oversight: on a blanked line nearly every access is an external slot so positions carry little
  information, and it is the path every real game's bulk VDP traffic takes — the highest-blast-radius place
  to change. *What would settle it:* Kabuto's notes do not publish a display-off pattern string; Mask of
  Destiny's measured refresh cadence (see **F-BLANKREFRESH**) is the only positional datum found. Needs a
  measured blanked-line pattern before it lands, and it belongs with the deferred per-line `dma_cost`
  integration (recon §3.4 "S3"), not on its own.
  *Rider, registered 2026-08-03 by the T16 review — the same boundary seen from the other side.*
  **`Vdp::next_active_slot` wraps into the next line using the active table unconditionally**, without
  asking whether that line is active: a drain starting past the last slot of line 223 is charged as if line
  224 were still displaying, when it is the first vblank line on which nearly every access is external.
  `Vdp::entry_drain_cost` makes the mirror-image simplification, picking the active/blanked branch once from
  the drain's *start* instant. Both are one-line consequences of "a drain is costed against a single line's
  model", and both should be fixed together with the blanked-line positions rather than piecemeal.
* **F-BLANKREFRESH — 205/167 vs 204/166 blanked-line slots.** (Registered 2026-08-03 by slice T16/S1;
  recon question Q4. **Code anchor: `Vdp::slots_per_line` in `crates/oracle-core/src/vdp.rs`.**) The Sega
  manual's DMA-capacity table gives 205 (H40) / 167 (H32) external slots on a blanked line, corroborated by
  TascoDLX ("on V-blank lines it's all external access slots except for the usual refresh slots (H32:
  167+4=171 ; H40: 205+5=210)") and confirmed by Nemesis. Mask of Destiny reports a *measured* extra
  refresh slot with the display off — "In H40 mode, there are 6 refresh slots when the display is off.
  There is one refresh slot every 32 slots starting at slot 37" — which gives 204, and 166 by analogy for
  H32 (whose display-off refresh positions he says he never determined). **No source adjudicates.** We keep
  the currently pinned 205/167. The recon measured the swap as currency-free on today's fixtures either
  way, so this is a correctness question with no gate pressure behind it. *What would settle it:* a
  hardware or BlastEm measurement of maximum DMA throughput on a display-off line, which differs by exactly
  one word per line between the two models.
* **F-H32SLOTS — no ROM exercises the H32 slot table.** (Registered 2026-08-03 by slice T16/S1; recon
  question Q6. **Code anchor: `Vdp::H32_ACTIVE_SLOTS` in `crates/oracle-core/src/vdp.rs`.**) T16 runs in
  H40 — measured: its drain cost was the H40 380 mclk. The H32 indices are derived from Kabuto's H32
  pattern string by the same expansion as H40's, and `vdp::tests::active_slot_gaps_follow_the_published_pattern`
  now checks both tables against the re-expanded strings, so a *transcription* error is caught. What is
  **not** covered is whether the H32 string itself is right, and no vendored ROM would notice.
  `vdp_sprite_masking` is the only H32 row we exercise and it is not FIFO-timing sensitive. *What would
  settle it:* run VDPFIFOTesting in H32 — but see Open question Q1 below, the H32/H40 toggle on the sprite
  ROM already behaves oddly for us, and VDPFIFOTesting offers no H32 mode at all. A BlastEm A/B on a
  synthetic H32 full-FIFO stall is the realistic instrument.
* **F-FIFOLAT — the FIFO's pipeline latency is not modeled.** (Registered 2026-08-03 by slice T16/S1;
  recon question Q2. **Code anchor: `Vdp::entry_drain_cost` / `Vdp::fifo_drain` in
  `crates/oracle-core/src/vdp.rs`** — our drain begins at the first slot strictly after the write.) Mask of
  Destiny: "There's also some latency involved in the FIFO, so there's a delay (2 or 3 slots IIRC) between
  when a word gets written to the FIFO and when the first byte gets written to VRAM even when the display
  is off." The hedge is his. This is pre-existing (the uniform model had zero latency too), but S1 makes it
  directly adjacent: with real slot positions, "which slot does a word start draining on" is now a
  meaningful question. T16 cannot distinguish the two — it observes FIFO *occupancy*, not VRAM — so nothing
  in the suite would move. *What would settle it:* a ROM or BlastEm instrument that writes one word and
  reads the destination back at a known slot offset. Do not add it on the forum hedge alone; it would shift
  every active-display drain and is a currency risk.

* **F-SLOTTABLE — the slot instants are divided out on every probe instead of being a const table.**
  (Registered 2026-08-03 by the T16 review. **Code anchor: the `// F-SLOTTABLE` comment inside
  `Vdp::next_active_slot`, `crates/oracle-core/src/vdp.rs`.**) The loop recomputes
  `k * MCLK_PER_LINE / accesses` per candidate slot — up to 18 (H40) integer divisions per call, and a VRAM
  entry costs two calls, so up to 36 divisions per drained entry. `entry_drain_cost` sits on the drain path
  of **every data-port write and every status read in every ROM**, which is as hot as core code gets here.
  Storing the mclk instants directly as a `const [u64; 18]` / `[u64; 16]` would be both faster and more
  readable (the reader sees 228, 358, 488 … rather than an index needing mental arithmetic). Deferred, not
  done, for one reason: the current form keeps the *published access indices* in the source, which is what
  `vdp::tests::active_slot_gaps_follow_the_published_pattern` checks against Kabuto's re-expanded pattern
  strings — the slice's main defence against a transcription typo. A precomputed table must therefore keep
  that test meaningful by asserting the derivation (indices → instants) rather than replacing it. Purely an
  optimisation and a readability change: the values are identical by construction, so it must land with
  every currency gate byte-identical, and any movement means the derivation is wrong. Note it interacts with
  **F-SLOTGRID** — if the mclk mapping changes, the table changes with it, so do F-SLOTGRID first if both
  are on the board.
* **F-DMAHALT — the last four words of a DMA are accounted twice.** (Registered 2026-08-03 by slice T16/S2;
  recon question Q5. **Code anchors: `Vdp::dma_cost` and `Vdp::dma_complete` in
  `crates/oracle-core/src/vdp.rs`, and `MegaDriveBus::run_mem_dma` in `crates/oracle-core/src/bus.rs`.**)
  S2 leaves the 68k halt at the full `count × slots × rate` *and* leaves up to four words pending in the
  FIFO afterwards, so those words are billed both inside the halt and again as post-transfer drain.
  Physically the DMA unit should release the bus roughly four slots earlier, since its job ends when the
  last word enters the FIFO. VDPFIFOTesting test 16 cannot see the difference — it measures FIFO *state*,
  not transfer duration — so nothing in the suite constrains it either way, and shortening the halt changes
  DMA elapsed time for **every** DMA-using ROM, which is a real currency risk. Shipped deliberately as a
  small, documented over-count rather than an unmeasured change. *What would settle it:* a ROM or BlastEm
  instrument that times a DMA's 68k halt directly (e.g. counting a free-running timer across the transfer)
  rather than inspecting the FIFO afterwards. Naturally belongs with the deferred per-line `dma_cost`
  integration, since both are about how a transfer's elapsed time is computed.

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
