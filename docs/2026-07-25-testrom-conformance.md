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
| `m68k_memory_test` (`memtest_68k`) | Reads every non-lockup address range twice; prints what it read **and** the ROM's own built-in real-hardware reference (`?` = wildcard nibble) | **Yes** — per-row compare, font base `$100`, 30 frames | **4 / 13 rows match** — mismatches: `400000-7FFFFF`, `A00000-A0FFFF`, `A00000-A03FFF`, `A06000-A07EFF`, `A10000-A1001F`, `A11100` (×2), `A11200`, `C00004-C00007` | Mostly **known gap K4** (no open-bus model). `A00000-A0FFFF` also folds in **K2** (Z80 `$7F00` mirror). |
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

### K2 — Z80 `$7F00-$7F1F` VDP-mirror reads return a constant `$FF`

*Symptom:* contributes to the `m68k_memory_test` `A00000-A0FFFF` row mismatch.

*Mechanism:* `crates/oracle-core/src/z80/bus.rs` — the read `match` falls through to `_ => 0xFF` for the
`$7F00-$7F1F` VDP-port mirror (the module header table already records this as *deferred: open bus / drop
(needs the `Vdp` borrow)*). Writes to the same window drop. Hardware mirrors the real VDP ports there.

*Why deferred:* wiring it up needs the `Vdp` borrow through the Z80 bus, which is a real architectural
change to the split-borrow bus, not a one-liner.

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

### K4 — no open-bus model (root cause of most `memtest_68k` mismatches)

Unmapped / write-only / partially-decoded addresses return a fixed value in our bus instead of the last value
left floating on the bus by the previous cycle. `memtest_68k`'s reference column shows hardware returning
prefetch-derived residue (e.g. `4E00`, `4E71`, `4F00` — fragments of the ROM's own instruction stream) where
we return zeros or `FFxx`. This single gap explains the `400000-7FFFFF`, `A11100` (×2), `A11200`,
`A10000-A1001F` and `C00004-C00007` rows. It is a bus-level model, additive to the existing typed-bus
protocol, and would move a golden-hash currency — hence recorded, not fixed here.

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
