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
| `m68k_illegal` (`itest`) | Illegal / privileged / unimplemented encodings must trap to the right vector | **Yes** — no text; verdict is the backdrop word `CRAM[0..2]`: `$00E0` green = pass, `$000E` red = fail. 2 frames | **FAIL** — backdrop `$000E` | **Known bug K1** (`CMPI.B/.W #imm,An` executes instead of trapping). See below. |
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
| `m68k_opcode_sizes` (`m68k_opcode_sizes.bin`) | Per-opcode size/encoding sweep | Frame hash only — **not scraped**: at the pinned 120-frame budget the screen shows the ROM's font/pattern page, not a result page | `frame_hash=0xca81010c64f8e701` | Unscraped by choice. Finding its result page (a longer budget or an input) is deferred. |

Deliberately **not** vendored: the two TiTAN *Overdrive* mega-demos. They are the classic hardware-torture
payloads, but their verdict is human judgment on a moving picture, not a scrapeable pass/fail. Their Drive
ids are recorded in the header comment of `tools/fetch-testroms.sh`.

## Known bugs found during this recon — recorded, NOT fixed

These were found while building the harness. They are named here so the scorecard rows above are
*attributable*; fixing them is separate work (each will move a baseline row, deliberately).

### K1 — `CMPI.B/.W #imm,An` executes instead of trapping

*Symptom:* `m68k_illegal` (`itest`) ends red (`$000E`).

*Mechanism:* on the 68000 `CMPI`'s destination must be **data-alterable**; address-register direct (mode 1)
is not a legal `CMPI` destination at any size (An comparison is `CMPA`'s job), and mode 1 is illegal for
*every* byte-size EA. We decode it as a real instruction on two independent counts:

- `crates/oracle-core/src/m68000/ea.rs` — `src_seq`'s mode-1 arm (`(1, _) => SrcSeq { … }`) is **not
  size-gated**. Its own comment asserts "byte is illegal and never reaches here", which holds for the
  families that gate the size before calling, but is not enforced at this layer.
- `crates/oracle-core/src/m68000/decode.rs` — `cmpi_recipe` (line ~2715) dispatches straight into the EA
  machinery with **no data-alterable destination guard**, so mode 1 is accepted for `.b`/`.w`.

*Expected fix shape:* a data-alterable destination guard in `cmpi_recipe` (returning
`decode_time_exception_recipe(4)`, the mechanism already used there for the illegal cases it *does* catch),
and/or a size gate on the mode-1 arm. Both are `src/` changes → they move currency; out of scope for this
additive-only harness.

*Reference:* M68000PRM `CMPI` ("destination: data alterable"); M68000UM §6 vector 4.

### K2 — Z80 `$7F00-$7F1F` VDP-mirror reads return a constant `$FF`

*Symptom:* contributes to the `m68k_memory_test` `A00000-A0FFFF` row mismatch.

*Mechanism:* `crates/oracle-core/src/z80/bus.rs` — the read `match` falls through to `_ => 0xFF` for the
`$7F00-$7F1F` VDP-port mirror (the module header table already records this as *deferred: open bus / drop
(needs the `Vdp` borrow)*). Writes to the same window drop. Hardware mirrors the real VDP ports there.

*Why deferred:* wiring it up needs the `Vdp` borrow through the Z80 bus, which is a real architectural
change to the split-borrow bus, not a one-liner.

### K3 — div0 stacked PC — **UNDER ADJUDICATION, unresolved**

Recorded for completeness because it sits adjacent to the exception work `itest` exercises. The stacked PC
for the divide-by-zero (vector 5) trap is **under adjudication**; this ledger deliberately asserts **neither**
value. `m68k_illegal`'s single-bit red/green verdict cannot discriminate it, so no scorecard row depends on
it. Do not "resolve" it from this document.

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
