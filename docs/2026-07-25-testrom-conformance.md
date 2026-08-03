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
| `vdp_port_access` (`VDPFIFOTesting`) | 16 VDP port-access tests over two pages: FIFO size/behaviour, DMA-via-FIFO, byteswapping, partial CP writes, register-write masking, read-target switching, FIFO wait states | **Yes** — scrapes the on-screen `Results: ( P/ F/ T)` line; page 1 auto-runs (settles ~frame 42), `Start` advances to page 2 (~480 more frames) | **page 1 = 6 pass / 3 fail / 9**; **pages 1+2 cumulative = 9 pass / 7 fail / 16** | Expected: the VDP timing skeleton is an interim model (`docs/2026-07-16-vdp-recon.md`; the FIFO/wait-state rows are exactly the "Phase 3 per-line DMA cost" deferral). CHARTER explicitly does not gate on this ROM. |
| `vdp_sprite_masking` (`SpriteMaskingTestRom`) | 9 sprite masking / per-line / per-frame / dot-overflow tests | **Yes** — verdict is a 32×8 glyph at the right edge, classified by **rendered-pixel hash** (its nametable cells are identical for the tick and cross cases, so only the framebuffer discriminates). 300 frames, settles ~frame 8 | `1=TICK/TICK 2=TICK/TICK 3=TICK/CROSS 4=PASS 5=PASS 6=FAIL 7=PASS 8=PASS 9=TICK/TICK` — **2 failures**: test 3's second sub-case (MAX SPRITE DOTS – COMPLEX) and test 6 (MASK S1 ON DOT OVERFLOW) | Expected: both are the **mid-sprite pixel-budget cut** interim model, ledger row **P1** in `docs/2026-07-16-vdp-pixel-known-differences.md` (we spend budget per whole sprite; hardware cuts mid-sprite at the exact dot). Open question **Q1** below on the H32/H40 toggle. |
| `color_1536` (`TEST1536.BIN`) | 1536-colour trick — CRAM rewritten mid-scanline | Frame hash only | `frame_hash=0x96b9c93c4f3dd325` | **Limitation L1**: end-of-frame capture. This ROM renders correctly only with per-scanline capture; the end-of-frame framebuffer cannot show it. |
| `cram_flicker` (`cram flicker.bin`) | CRAM-dot / border artefacts from writing CRAM during active display | **NOT-RENDERABLE** — border-only rendering; the effect lives outside our active-area framebuffer | `frame_hash=0x815bb645bc46a325` | Structural: we render the 224-line active area, not the border where the artefact appears. |
| `direct_color_dma` (`Direct-Color-DMA.bin`) | Direct-colour DMA — CRAM streamed per pixel during active display | **NOT-RENDERABLE** — sub-scanline CRAM | `frame_hash=0xed40dc4a6c4fc325` | Structural: same root as L1, one level finer (needs sub-scanline CRAM state, not just per-line). |
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

*Still deferred (ledgered here):* the **68k-side** view of the same region — our 68k bus maps all of
`$A00000-$A0FFFF` to the Z80-RAM mirror, so a 68k read of `$A07F00+` does not reach the VDP ports. That is
part of the `m68k_memory_test` `A00000-A0FFFF` row (alongside K4) and stays a recorded gap. Z80 *writes*
to `$7F00-$7F1F` (other than the PSG tap at `$7F11`) still drop, and `$7F10-$7F1F` reads stay open bus
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

**Open questions carried from the design's §6 (still open):** Q1 — the physical mechanism of the
arbiter's low-byte-`$00` (adopted as the memtest-pinned empirical rule; the C++ reference retains the
full word instead); Q4 — Z80-window word *writes* (Exodus TODO says only one byte should land; we
still store both; untested by memtest, 0 corpus hits); Q5 — board-revision stability of the reference
values (unknowable offline; the vendored ROM's column is the pinned ground truth); Q6 — region-wide
extrapolation of the arbiter flavor to untested gaps (`$A10020-$A10FFF`, `$A11000`, `$A130xx` reads,
`$A14000` — still full-latch retention, applied only where evidenced). Plus the K4-3 ledgered
follow-ups (68k-side bank-latch path, true 15-bit window masking, 68k-side `$A07F00+` VDP routing).

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

## Limitations of the instrument itself

**L1 — end-of-frame capture.** The harness captures the framebuffer *after* `run_frames(n)` completes, so
any effect that exists only mid-frame is lost. `color_1536` is the clean demonstration: it renders correctly
only with per-scanline capture. Rows marked NOT-RENDERABLE (`cram_flicker`, `direct_color_dma`) are the
stronger form of the same limitation. A per-scanline capture mode would upgrade these three rows from
"frame hash of the wrong thing" to real verdicts.

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
