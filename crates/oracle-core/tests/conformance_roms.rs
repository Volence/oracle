//! Test-ROM conformance harness — a **non-gating instrument with a pinned baseline**.
//!
//! Boots the vendored corpus of well-known Mega Drive test ROMs (`tools/fetch-testroms.sh`) headlessly
//! against the real `System`, scrapes each ROM's own verdict, and compares the WHOLE scorecard against a
//! pinned baseline in a single `assert_eq!` so one diff shows every changed ROM at once.
//!
//! **This harness does NOT gate on "everything passes."** Per `CHARTER.md` the launch target is
//! *MVP-debuggable*, NOT "passes VDPFIFOTesting"; accuracy is an asymptote. Several of these ROMs fail
//! today for documented reasons (the K1 illegal-decode hole, the Z80 `$7F00` VDP-mirror stub and the
//! K4 open-bus model were all fixed 2026-08-02 — `m68k_illegal` is green and `m68k_memory_test` is
//! 12/13 since). The remaining failures are
//! **recorded, not fixed** — the baseline below is
//! a *photograph of today*, and this test fires only on a **regression from it** (or an unannounced
//! improvement, which is equally worth seeing). Every entry, its reference, and its expected-fail reason
//! is written up in `docs/2026-07-25-testrom-conformance.md`.
//!
//! Currency note: this file is additive-only test code. It touches no `oracle-core/src/` semantics, so the
//! frozen golden state hashes (`determinism_gate`, `export_state_v1`) are unmoved by construction.
//!
//! ## How a verdict is scraped
//!
//! Most of these ROMs print their result as text through the VDP. There is no OCR: the text ROMs use an
//! ASCII-ordered font, so a plane-A nametable cell's low 11 bits are `font_base + ASCII` and the screen can
//! be read straight out of VRAM. The font base is **hardcoded per ROM** (see `Rom::font_base`) rather than
//! auto-detected, so a decode change shows up as garbled text (a visible regression) instead of being
//! silently re-derived. Two ROMs need other channels: `m68k_illegal` signals via the backdrop colour, and
//! `vdp_sprite_masking` draws its per-test verdict as a 32x8 glyph (PASS / FAIL / tick / cross) that is
//! classified by hashing the rendered pixels — its nametable cells are identical for the tick and cross
//! cases, so only the framebuffer can tell them apart.
//!
//! **Which framebuffer, for that one ROM, is itself a measurement** (2026-09-18, `POSTHOC-CARRY`). The
//! glyphs are hashed out of a `ScanlineCapture` — the pixels the VDP emitted during the run — and hashed a
//! second time out of the post-hoc `Vdp::render_line`, and the row records both: the live verdicts, then
//! the complete list of glyphs the re-render disagrees about. The two differ on exactly one glyph today,
//! because `render_line` re-seeds R10 sprite masking from the end-of-frame `sprite_dot_overflow_carry`
//! instead of advancing it per line, and one of this ROM's nine tests is a test *of that carry*. Reading
//! that re-render as a verdict is what made `6=FAIL` stand, as a defect that was not there, for this row's
//! entire life to that point — pinned 2026-07-25, corrected 2026-09-18, eight weeks (the "thirteen months"
//! this line said until 2026-09-18 is not possible: the repository's first commit is 2026-06-24);
//! carrying both readings is what makes the next such divergence announce itself. See
//! `the_glyph_scrape_reads_the_live_render_path`.
//!
//! **And that ROM is scraped in BOTH its screen modes** (2026-09-18, `TESTROM-H40-HALF`). It runs its nine
//! tests in H32 and again in H40 — different sprite-per-line, dots-per-line and sprites-per-frame limits,
//! which four of the nine tests are tests *of* — and the harness had only ever measured the power-on half.
//! The row now carries both, and the mode each half is in is **proved off the captured picture** before a
//! single glyph is read (`ProvenScreen`): a toggle that silently failed would otherwise re-scrape the H32
//! screen and print nine perfectly plausible H40 verdicts.
//!
//! If the vendored ROM is missing, that ROM skips cleanly (run `tools/fetch-testroms.sh`).

use oracle_core::bus::{BusEvent, BusEventSink, BusOp, Fanout};
use oracle_core::io::Pad;
use oracle_core::scanline_capture::{Retain, ScanlineCapture};
use oracle_core::system::System;
use oracle_core::vdp::ACTIVE_LINES;
use std::path::Path;

const VENDOR_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/TestRoms");

/// Power-on RNG seed. Fixed so the whole scorecard is reproducible (seeded power-on RAM is a
/// non-negotiable of the core; a different seed is a different — still deterministic — machine).
const SEED: u64 = 0x1234_5678;

/// Every ROM `tools/fetch-testroms.sh` vendors. Keep in sync with that script: the count guard below
/// asserts the pinned baseline covers exactly this list, so a ROM can never be quietly dropped.
const ROMS: &[&str] = &[
    "color_1536",
    "cram_flicker",
    "direct_color_dma",
    "fm_test",
    "gfx_joystick",
    "io_sample",
    "m68k_bcd",
    "m68k_illegal",
    "m68k_memory_test",
    "m68k_opcode_sizes",
    "shadow_highlight",
    "vcounter",
    "vdp_port_access",
    "vdp_sprite_masking",
    "vdp_test_register",
    "window_distortion",
    "window_test",
];

// ---------------------------------------------------------------------------------------------------
// The pinned baseline
// ---------------------------------------------------------------------------------------------------

/// Today's recorded outcome for every ROM. **A diff here is the whole point of this test.** Changing an
/// entry is a deliberate, evidenced amendment (a fixed bug, a model refinement) — never a silent regen;
/// update `docs/2026-07-25-testrom-conformance.md` in the same change.
const BASELINE: &[(&str, &str)] = &[
    (
        // Re-pinned 2026-08-03 to the per-scanline capture (Limitation L1 narrowed). The old end-of-frame
        // hash was 0x96b9c93c4f3dd325 — a picture with FOUR distinct colours, because CRAM at end-of-frame
        // holds only the last of the mid-scanline rewrites. The capture hash below is the ROM's actual
        // ~1400-colour gradient. Both verified by eye as PPM dumps; see the ledger's L1 section.
        //
        // Re-pinned again 2026-08-19 (F-SCANLINE-SUBLINE slice 4, was 0x917371f07409cb25): CRAM landings are
        // now resolved to the pixel inside the row they land on, so a row shows the palette evolving across
        // its own width instead of one snapshot of it. Measured mechanism: 515 value-changing in-active-window
        // CRAM writes in the hashed frame, to indices 4-7, over active lines 48..221 — all four sampled by
        // the picture. **This literal must stay equal to `scanline_goldens.rs`'s** — that agreement is a
        // live cross-check that the two harnesses share a byte layout and run shape, so the two move together
        // to one measured value or not at all.
        //
        // Re-pinned 2026-09-08 (C2, the FIFO double-charge; was 0x9ae4acc58d2a382d). MECHANISM, measured on
        // this branch over the 120 hashed frames: the ROM performs **11 data-port writes into a full FIFO**,
        // and **5 of them are a second-or-later stalling write inside the SAME 68000 instruction** — the
        // shape `Vdp::data_write_at` used to bill from the instruction's frozen `now` rather than from the
        // drain instant the CPU had already been held to, re-charging **1,239 mclk (~177 CPU cycles)** of
        // hold that was already in the instruction's cost. Removing that changes those instructions' cycle
        // counts, which moves `now_mclk` for everything after them, which relocates this ROM's mid-scanline
        // CRAM landings — and since `F-SCANLINE-SUBLINE` slice 4 those landings are resolved to a *pixel*
        // inside the row they land on, so a few cycles of shift is visible in the picture. This ROM is the
        // ONLY row in the corpus that moved: the other 16 scorecard lines, including every pass/fail verdict
        // (`vdp_port_access` 16/0/16, `m68k_memory_test` 13/13, `vdp_sprite_masking`), are byte-identical.
        "color_1536",
        "VISUAL-BASELINE frame_hash=0x87ecf46f3cd54fda (per-scanline capture)",
    ),
    (
        // Re-adjudicated 2026-08-03 under the per-scanline capture (ledger L1a). The HASH is unchanged —
        // the capture is byte-identical to the end-of-frame frame (224/224 lines, one distinct colour) —
        // only the reason moved: "border-only rendering" was wrong. The ROM leaves the screen blank and
        // hammers palette indices 4 and 36 (never index 0, so it is not a border-colour demo) 16x per
        // active line; what it demonstrates is the CRAM-write artefact at the beam position, which is
        // sub-scanline and which we do not model anywhere. Follow-up F-CRAMDOT.
        //
        // Re-measured 2026-08-19 under F-SCANLINE-SUBLINE slice 4: hash unchanged, and now for a *proven*
        // reason rather than a predicted one. The slice resolves CRAM landings to a pixel, so this ROM's
        // 2,692 value-changing in-active-window writes per hashed frame do split its rows — but they are all
        // to indices 4 and 36, and the picture samples only index 0, so every segment decodes the same
        // colour. Sub-scanline CRAM is now modelled; the CRAM *dot* (the value painted at the beam position
        // regardless of the resolved index) still is not, and that is what this ROM tests. F-CRAMDOT stands.
        //
        // Same caveat as `direct_color_dma` above: the load-bearing measurement is
        // **`scanline_goldens.rs`'s live-vs-post-hoc verdict**, which stayed IDENTICAL-TO-POST-HOC. This
        // row's own hash is a post-hoc `render_line` sweep and is structurally immune to the slice.
        "cram_flicker",
        "NOT-RENDERABLE (CRAM-write artefact, sub-scanline) frame_hash=0x815bb645bc46a325",
    ),
    (
        // Re-adjudicated 2026-08-03 (ledger L1b), and re-measured 2026-08-19 under F-SCANLINE-SUBLINE
        // slice 4 — **hash unchanged, but the 2026-08-03 reason was retired by that slice and is replaced
        // here**. It used to read "the 44,352 CRAM words land inside ONE inter-line window in our model",
        // which is precisely the assumption slice 4 removes: those words now land *inside* line 1 and are
        // resolved to a pixel. The picture still does not move, for a reason that had to be measured rather
        // than assumed: the whole burst shares one master clock (decision C-6, one DMA = one landing), the
        // journal coalesces it to a single landing at pixel 82, and that surviving landing's word is the
        // value index 0 **already held** at line 1's start — net effect over the line, in the hashed frame,
        // is 0x0000 -> 0x0000. So the row does split, and both spans decode to the same colour.
        //
        // Which measurement carries that claim: **`scanline_goldens.rs`'s live-vs-post-hoc verdict for this
        // ROM** (still IDENTICAL-TO-POST-HOC after the slice), not the hash on this row. This row is scraped
        // through `scrape_visual` -> post-hoc `render_line`, which is structurally blind to anything
        // sub-frame — it could not have moved whatever the emitter did, so it is evidence of nothing here.
        "direct_color_dma",
        "NOT-RENDERABLE (sub-scanline CRAM) frame_hash=0xed40dc4a6c4fc325",
    ),
    ("fm_test", "VISUAL-BASELINE frame_hash=0xed40dc4a6c4fc325"),
    (
        "gfx_joystick",
        "VISUAL-BASELINE frame_hash=0xba8ce06075e4e163",
    ),
    ("io_sample", "PASS port1=JOYPAD port2=JOYPAD"),
    (
        "m68k_bcd",
        "PASS abcd/sbcd/nbcd 0 value errors, 0 flag errors",
    ),
    ("m68k_illegal", "PASS backdrop=$00E0 (green)"),
    (
        // K4-1 (2026-08-02): arbiter open-bus flavor — `400000-7FFFFF` + `A11200` rows green (was 4/13).
        // K4-2 (2026-08-02): $A11100 residue + reset-folded grant bit — both `A11100` rows green.
        // K4-3 (2026-08-02): Z80-window gating + word duplication + $A06000-$A07EFF=$FF (and the
        // bank-canary alias fix) — the three `A0xxxx` window rows green.
        // K4-4 (2026-08-02): the I/O block ignores A0 (registers answer both byte lanes) — row green.
        // K4-5 (2026-08-02): status bits 10-15 float with the residue — the row's open-bus half is
        // now exact (0290 -> 4E90); it stayed red on the status LOW byte only.
        // See docs/2026-08-02-k4-openbus-design.md and the K4 ledger section.
        // Status-low-byte fixes (2026-08-02): ODD forced 0 outside interlace (reference toggle rule
        // `interlace && !odd`, Oracle S315-5313_Timing.cpp:1103) + VBlank bit forced set while the
        // display is disabled (Oracle S315-5313_General.cpp:2345-2351 "hardware tests have confirmed";
        // memtest's C00004 reads land mid active scan at line 27 with reg 1 = $04) — row green, 13/13.
        "m68k_memory_test",
        "13/13 rows match the ROM's hardware reference",
    ),
    (
        "m68k_opcode_sizes",
        // Re-pinned 2026-08-02 with the K1 illegal-decode fix: the ROM plots the live decode map, and
        // the newly-trapping encodings move a handful of its cells (476 px in the 0x0/4/8/C/E pages).
        "VISUAL-BASELINE frame_hash=0x5436cda5786ea450",
    ),
    (
        "shadow_highlight",
        "VISUAL-BASELINE frame_hash=0x428e03aa61cc0285",
    ),
    ("vcounter", "VISUAL-BASELINE frame_hash=0x294957c8001b9f93"),
    (
        // A1 (2026-08-03): live FIFO EMPTY/FULL status flags. The Results counts were UNCHANGED, but
        // T16 "FIFO Wait States" went 26/80 → 62/80 verdict bytes green (every group's first-probe
        // word now matches). The rest of T16 needs DMA-through-FIFO (slice A3) + discrete per-line
        // access-slot scheduling — see docs/2026-07-25-testrom-conformance.md.
        // A2 (2026-08-03): control-port / code-register edges. Page 2 went 9/7 → 11/5 — T13 "Register
        // Writes and Code Reg" and T10 "Partial CP Writes" flipped to pass. Page 1 is untouched (its
        // three failures, T3/T4/T6, are other slices). T12 "Register Write Mode4 Mask" was left a named
        // residual here, believed to move frozen currency — that belief was later measured and proved
        // FALSE; see the T12 note at the end of this comment.
        // A3a (2026-08-03): 68k→VDP DMA payload words now occupy physical FIFO slots. Page 1 went
        // 6/3 → 7/2 — T3 "DMA Transfer using FIFO" flipped to pass (its 16 observations are pure
        // undefined-bit FIFO snoop reads).
        // A3b (2026-08-03): the DMA-fill trigger is applied as a normal full-word write before the fill
        // runs, and the fill engine writes its byte to `address ^ 1`. Page 1 went 7/2 → 8/1 — T4 "DMA Fill
        // FIFO Usage" flipped to pass (its last two wrong words, 8 and 13, were exactly those two bugs).
        // NOTE the A3 design note predicted this would move `export_state_v1::GOLDEN_HASH`; it does NOT —
        // that prediction mis-identified `testrom::build_pad_poll` (which does DMA-fill VRAM, and is used
        // only by the io/watchpoint tests) as the golden fixture. The golden fixture is `testrom::build`,
        // which never touches the VDP. Verified: every currency gate is byte-identical across this slice.
        // A4 (2026-08-03): the undocumented 8-bit VRAM read (CD = %001100) returns `vram[address ^ 1]` in
        // the low byte and the next-available FIFO entry's high byte in the high byte. Page 1 went 8/1 →
        // **9/0** — T6 "8-bit VRAM Read target 01100" flipped to pass, completing page 1. Cumulative
        // 13/3 → 14/2; page 2's two residuals were T12 and T16.
        // T12 (2026-08-03): in Mode 4 (reg 1 bit 2 = M5 clear) only the eleven SMS registers 0-10 are
        // writable — writes above 10 are discarded. Cumulative 14/2 → **15/1**; page 1 unchanged at 9/0
        // (T12 is a page-2 test). The parked claim that this moves `export_state_v1::GOLDEN_HASH` and the
        // `golden_frames` scenes was the *same* `build_pad_poll`/`build` mis-identification as above and is
        // FALSE: measured, every frozen constant came back byte-identical once the fixtures that declared
        // Mode 4 while programming Mode-5 registers were corrected to declare M5 (test-only, 8 sites; see
        // docs/2026-08-03-decision2-premise-recheck.md). The `> 10` boundary is extrapolated from a hedged
        // source — the ROM pins register 15 only — registered as follow-up F-M4REGS in the ledger.
        // T16 (2026-08-03, slices T16/S1 + T16/S2): the last one. **This ROM now passes 16/16.** Two
        // independent causes, neither of them the "Phase 3 per-line DMA cost" deferral this residual had
        // been filed under: (S1) the external access slots within an active line are irregularly spaced —
        // Kabuto's published per-line access pattern — and our uniform spacing let the /DTACK-stall
        // phase-lock park the ROM's probe just past a drain boundary on every one of its 2048 retries,
        // costing groups 2/3/5/6/8 [62/80 → 72/80 verdict bytes]; (S2) a finished 68k→VDP DMA leaves words
        // *pending* in the FIFO, because the DMA unit's job ends when the last word is pushed into the FIFO
        // rather than when it reaches VRAM, so the resuming 68k sees FULL → partial → EMPTY, which is
        // groups 9/10 [72/80 → **80/80**]. S2 reverses A3a's deliberate ring-store-only choice and thereby
        // answers design question Q1 of docs/2026-08-03-a3-dma-fifo-design.md. Cumulative 15/1 → **16/0**;
        // page 1 unchanged at 9/0 (T16 is a page-2 test). Every other row here — including the two
        // frame_hash rows measurably sensitive to FIFO stall timing, m68k_opcode_sizes and
        // window_distortion — and every frozen currency constant is byte-identical across both slices.
        // The genuinely large piece (integrating `Vdp::dma_cost` across the lines a transfer spans) is NOT
        // needed for T16 and stays deferred; see the ledger's S1/S2 addenda.
        //
        // WIDENED 2026-09-12 (VDP-PORT-ACCESS-FULL-ROM). "16/0/16, complete" was true of the two pages this
        // scraper read, never of the ROM: VDPFIFOTesting runs 122 tests over 22 pages. The row now comes
        // from the one shared whole-ROM run (`port_access_run`), which reads the tally the ROM prints at the
        // end of every page. It keeps page 1 and page 2 (the numbers every entry above cites, unchanged
        // under the new paging) and adds the last page. Which 46 tests fail, and why, is pinned per test by
        // `vdp_port_access_full_rom_verdicts` (`PORT_ACCESS_FAILING`) and written up in
        // docs/2026-09-12-vdp-port-access-full-rom.md.
        //
        // 2026-09-12 (DMA-SRC-ADVANCE, cause A3): a fill and a copy now advance source registers 21/22 by
        // their length. Tests 28 and 29 flip; all 22 pages 76/46/122 → 78/44/122. Pages 1 and 2 unchanged.
        // A1 (2026-09-12, VSRAM-DECODE): the VSRAM address is 7 bits (wraps at $80), writes to $50-$7F are
        // discarded, and reads there return the VSRAM read latch, which the committed render feeds. All
        // 22 pages 76/46 → **112/10**: tests 23, 74-95's eight VSRAM fills and the copy matrix 96-122 flip
        // to pass; test 20 still fails (6/12 words, its A2 half); pages 1 and 2 are unchanged.
        // A1 + A3 merged (2026-09-12), measured on the merged tree: **114/8/122**, failing 20 27 31-34 36 38.
        // 2026-09-12 (DMA-SRC-128K, cause A2): a 68k→VDP transfer's source wraps inside its own 128 KB page,
        // and register 23 never takes the carry. Tests 27 and 20 flip; all 22 pages 114/8/122 → **116/6/122**,
        // failing 31 32 33 34 36 38. Test 20 is a JOINT flip: all six of its remaining wrong words were A2's,
        // but two more had already gone with A1 (8/12 → 6/12 → 0/12), so it took both. Pages 1 and 2 unchanged.
        // A4 (2026-09-12, FILL-BUSY-ARM): DMA-busy reads set from a fill's CONTROL write, not only from its
        // trigger — `Vdp::dma_busy` is now "a fill is armed (CD5 + register 23 = Fill) OR the transfer window
        // is open". Tests 36 and 38 flip; on the pre-A2 branch all 22 pages went **114/8 → 116/6**, failing
        // 20 27 31 32 33 34. Pages 1 and 2 unchanged, and no other scorecard row moved.
        // A5 (2026-09-12, FILL-TGT; the body is `Vdp::fill_step` since M1): `Vdp::run_fill`'s body shares `code_names_a_write_target` with the
        // two data-port write paths, so a fill armed on a code that names no write target runs — address,
        // length, source registers 21/22, busy window — and writes nowhere. Test 34 flips; on the pre-A2
        // branch all 22 pages went **116/6 → 117/5**, failing 20 27 31 32 33. Tests 4, 28, 29 and 72-95
        // unmoved. Follow-up F-FILLTGT retired.
        // A2 + A4 + A5 merged (2026-09-12), measured on the merged tree: **119/3/122**, failing 31 32 33 —
        // M1 (FILL-OVER-TIME) alone, the size-L design item that was then out of scope. Pages 1 and 2
        // stay 9/0/9 and 16/0/16.
        // M1 (2026-09-15, FILL-RUN — parcel P1 of docs/2026-09-14-m1-fill-over-time-design.md): a DMA fill
        // is a process the VDP advances lazily on the external-slot clock the FIFO already runs on, and each
        // step pulls its write target and data out of the FIFO ring (Nemesis, *VDP Internals* p.4), so a
        // data-port write made while a fill is running changes what the rest of the fill writes and consumes
        // no length count. Tests 31, 32 and 33 flip; all 22 pages 119/3/122 → **122/0/122**, and
        // PORT_ACCESS_FAILING is now EMPTY — this ROM passes every one of its 122 tests. Pages 1 and 2
        // unchanged at 9/0/9 and 16/0/16, and **no other row of this scorecard moved**: the 16 visual
        // baselines, the two PASS rows and the sprite-masking glyphs are byte-identical across the parcel.
        "vdp_port_access",
        "page1 pass/fail/total=9/0/9; pages1+2 cumulative=16/0/16; all 22 pages cumulative=122/0/122",
    ),
    (
        // **Re-pathed 2026-09-18 (POSTHOC-CARRY): `6=FAIL` → `6=PASS`, and F-POSTHOC-STALE-CARRY is
        // CLOSED.** No emulator behaviour changed; the instrument did. The nine verdict glyphs are now
        // classified from a `ScanlineCapture` of the last completed frame — the pixels the VDP emitted —
        // instead of from the post-hoc `Vdp::render_line`, and the post-hoc reading is kept and reported in
        // the bracket as the control arm.
        //
        // MECHANISM, re-derived from the tree rather than taken from the 2026-08-15 note. `Vdp::sprite_line`
        // seeds its R10 x=0 masking with `seen_nonzero = self.sprite_dot_overflow_carry()` and never writes
        // it back; `Vdp::render_scanline` commits the new carry after each line, `Vdp::render_line` does not.
        // So a pure re-render seeds EVERY line from the carry as it stands after the run. Measured on this
        // ROM at its idle stop: that settled carry is `false`, while live the walk commits `true` on line 87
        // and holds it through line 94 (line 95 is drawn with the seed still `true` and commits `false` —
        // the mask suppressed the sprites, so there was no overflow to carry). Lines 88-95, x 216..247, are
        // therefore drawn with the mask armed live and unarmed post-hoc: eight lines, exactly the 32x8
        // rectangle this scraper reads for test 6 — MASK S1 ON DOT OVERFLOW, i.e. the ROM's test OF the
        // carry. The whole frame differs nowhere else (8 differing lines out of 224, measured), and the ROM
        // makes zero VDP accesses after frame 7, so nothing mid-frame is involved.
        //
        // The two blockers F-POSTHOC-STALE-CARRY named are settled, by measurement, not by argument:
        // (a) **every glyph constant survives the substrate change with no re-derivation.** Eight glyphs are
        // bit-identical through both paths, and test 6's live hash is `0x66094bba88cb93ed` — the `PASS`
        // literal, which was itself pinned from post-hoc pixels. `block_hash` has exactly one caller, this
        // ROM, so "every ROM and glyph the scraper classifies" is these nine; the other 16 rows scrape the
        // nametable, CRAM or a whole-frame hash and cannot be affected. (b) **which frame**: the last frame
        // the run COMPLETED before the idle stop (index 14; the stop fires mid-frame 15). The old comment's
        // "stopping mid-frame is irrelevant to it" was true of a re-render and is false of a capture, so it
        // is replaced by three loud guards in `sprite_masking_glyphs` — exactly one complete frame, at least
        // one frame completed, and the captured frame strictly later than the ROM's last VDP write.
        //
        // The previous warning — "do not fix the emulator until the post-hoc render says PASS" — is still
        // true and is now STRUCTURAL rather than prose. Doing that would empty `[post-hoc path differs: …]`,
        // which moves this row and fails `the_glyph_scrape_reads_the_live_render_path`. So would losing the
        // live path. The bracket is a positive statement in both directions: it says `[post-hoc path
        // agrees]` when there is nothing to report, so an absent disagreement can never read as an
        // unmeasured one.
        //
        // What did NOT move: no file under `crates/oracle-core/src/` was touched, and the other 16 rows of
        // this scorecard — plus `determinism_gate`, `export_state_v1`, `golden_frames` and
        // `scanline_goldens` — are byte-identical across this parcel.
        //
        // History. 2026-08-15 (the per-scanline survey) found the artefact and left the row standing on
        // purpose. 2026-09-16 (SPRITE-MID-CUT, ledger row P1) flipped test 3 `TICK/CROSS` → `TICK/TICK` when
        // the per-line pixel budget began cutting inside the straddling sprite; that glyph hash matched the
        // already-pinned `TICK_TICK`, so blocker (a) was never put to the test then. It is now.
        // ## The H40 half (2026-09-18, `TESTROM-H40-HALF`)
        //
        // This ROM runs its nine tests in TWO screen modes, and until today the harness measured the
        // power-on one. The row now carries both halves of one boot, separated by ` | `: H32 to idle, the
        // mode toggle, H40 to idle. One row rather than two BASELINE entries, deliberately —
        // `baseline_covers_every_rom` asserts a ROM/row bijection, and a regression in EITHER mode is a
        // diff on this single line either way, so the second entry would buy nothing and cost the guard.
        //
        // What the H40 half adds that the H32 half could not: the two modes have different sprite limits
        // (per line 20 vs 16, per line in dots 320 vs 256, per frame 80 vs 64), and tests 1, 2, 3 and 9
        // are tests OF those limits. Nine verdicts that had never been read now are.
        //
        // The H40 verdicts come out identical to the H32 ones, including the `6=` live/post-hoc split —
        // the stale-carry artefact reproduces in H40 for the same reason it does in H32 — so the bracket
        // is carried per half and the control arm requires the divergence in both.
        //
        // Every coordinate in the H40 segment was derived in H40, not scaled from H32 (see
        // `Screen::glyph_x`): the verdict rectangle is x 216..248 in both modes because the ROM writes
        // the verdict cells at plane columns 27-30 regardless of mode rather than right-aligning them, so
        // in H40 they sit nine cells in from the right edge. Confirmed off the H40 framebuffer: all nine
        // glyphs' lit pixels fall in x 216..=246, and x 248..319 is blank on all nine rows. The mode
        // LABEL is likewise measured — it is `ProvenScreen`'s, taken from the width the captured frame
        // ended on, not a string literal; the old row's `"H32:"` was a literal that could not be wrong.
        //
        // The toggle is `C`, which is the button the ROM's own code reads even though its text names
        // `Start`. Q1 is closed by that finding, not worked around: the ROM never writes `$A10009`
        // anywhere in its image, so TH is never driven low and bit 5 of `$A10003` is `C`. Full evidence
        // in `docs/2026-09-18-h40-half.md`; nothing under `crates/oracle-core/src/` was touched.
        "vdp_sprite_masking",
        "H32: 1=TICK/TICK 2=TICK/TICK 3=TICK/TICK 4=PASS 5=PASS 6=PASS 7=PASS 8=PASS 9=TICK/TICK \
         [post-hoc path differs: 6=FAIL] | \
         H40: 1=TICK/TICK 2=TICK/TICK 3=TICK/TICK 4=PASS 5=PASS 6=PASS 7=PASS 8=PASS 9=TICK/TICK \
         [post-hoc path differs: 6=FAIL]",
    ),
    (
        "vdp_test_register",
        "VISUAL-BASELINE frame_hash=0x4a49cfea306e928b",
    ),
    (
        "window_distortion",
        "VISUAL-BASELINE frame_hash=0x5102219d295b4e2c",
    ),
    (
        "window_test",
        "VISUAL-BASELINE frame_hash=0x4efcfda475af0d12",
    ),
];

// ---------------------------------------------------------------------------------------------------
// Harness plumbing
// ---------------------------------------------------------------------------------------------------

fn rom_path(name: &str) -> String {
    format!("{VENDOR_DIR}/{name}.bin")
}

/// Boot a vendored ROM headlessly, or `None` (with a `SKIP:` note) if it has not been fetched.
fn boot(name: &str) -> Option<System> {
    let path = rom_path(name);
    let Ok(rom) = std::fs::read(&path) else {
        eprintln!("SKIP: {path} not found — run tools/fetch-testroms.sh");
        return None;
    };
    let mut sys = System::new(SEED);
    sys.load_rom(rom);
    sys.reset();
    Some(sys)
}

/// The one FNV-1a byte layout every framebuffer hash here uses: r, g, b per pixel, in the caller's pixel
/// order. Shared by [`frame_hash`] and [`frame_hash_scanline`] so an end-of-frame hash and a per-scanline
/// hash of the same picture are directly comparable (they cannot drift into different layouts).
fn fnv1a_rgb(mut h: u64, px: &[(u8, u8, u8)]) -> u64 {
    for &(r, g, b) in px {
        for byte in [r, g, b] {
            h ^= byte as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

const FNV1A_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a over the whole active framebuffer (the `golden_frames.rs` idiom). A *self-consistency* pin: it
/// captures what the current model draws, nothing more.
fn frame_hash(sys: &System) -> u64 {
    let mut h = FNV1A_OFFSET;
    for line in 0..ACTIVE_LINES {
        h = fnv1a_rgb(h, &sys.vdp().render_line(line));
    }
    h
}

/// Run the ROM under the shared per-scanline capture sink (Limitation L1) in `LastFrame` retention — the
/// last complete frame's active lines, each as the VDP rendered it *during* the run, so mid-frame CRAM
/// rewrites are visible here and structurally invisible to [`frame_hash`], which reads only the end-of-frame
/// palette. Hashed in the SAME byte layout as [`frame_hash`] (line-major, r/g/b per pixel over lines
/// 0..=223), so the two hashes name the same kind of thing and a row can be moved from one to the other with
/// the difference being only *when* the pixels were read.
///
/// The retention (and the frame boundary that drives it) lives in
/// [`oracle_core::scanline_capture::ScanlineCapture`] — this file used to hand-roll it from magic line
/// comparisons (`F-SCANLINE-CAPTURE`). The sink is state-neutral (`tests/scanline_capture.rs`), so the run
/// itself is the plain run.
fn frame_hash_scanline(sys: &mut System, frames: u64) -> u64 {
    let mut cap = ScanlineCapture::new(Retain::LastFrame);
    sys.run_frames_with_sink(frames, &mut cap);
    let width = sys.vdp().render_line(0).len();
    assert_eq!(
        cap.pixels().len(),
        width * ACTIVE_LINES as usize,
        "capture must hold exactly one complete frame of active lines"
    );
    fnv1a_rgb(FNV1A_OFFSET, cap.pixels())
}

// ---------------------------------------------------------------------------------------------------
// Stop conditions — the ROM's own "I am done" signal, instead of a hand-tuned frame budget
// ---------------------------------------------------------------------------------------------------
//
// Every budget in this file was tuned by hand to "comfortably more than the ROM needs", because the run
// loop could not be interrupted. `BusEventSink::stop_requested` (2026-08-14) makes the ROM's own signal
// expressible, so a scraper can say *what it is waiting for* and keep the old number only as a bound.
//
// Two are converted here as proof of the seam; the rest are a separate, reviewable change. Both keep their
// original number as the fallback bound and both `assert!(record.fired())`, so the condition is proven to
// have actually happened — a predicate that quietly never fires would otherwise degrade to the old budget
// and look identical.

/// Whether `addr` is inside the VDP port block `$C00000-$C0001F` — the data port (`$C00000-3`), the control
/// port (`$C00004-7`) and the HV counter (`$C00008-F`). Every 68000-driven change to VDP state passes
/// through it: VRAM/CRAM/VSRAM writes via the data port, register writes and DMA triggers via the control
/// port. The wider `$C00000-$DFFFFF` mirror range is deliberately not matched — it is not needed by the one
/// ROM this is used on (measured), and the `assert!(record.fired())` at the call site turns a miss into a
/// loud failure rather than a wrong early stop.
fn is_vdp_port(addr: u32) -> bool {
    addr & 0xFF_FFE0 == 0x00C0_0000
}

/// **`m68k_illegal`'s verdict channel.** The ROM has no text at all: it paints the backdrop blue (`$0E00`)
/// while the illegal/privileged sweep runs and overwrites CRAM index 0 with green (`$00E0`) or red (`$000E`)
/// when it is done. Stop on that write — the verdict *is* the condition.
///
/// Deliberately verdict-agnostic: it stops on red as well as green, so converting this scraper cannot turn a
/// future FAIL into a timeout that reads as INDETERMINATE.
#[derive(Default)]
struct BackdropVerdict {
    seen: bool,
}

impl BusEventSink for BackdropVerdict {
    fn on_event(&mut self, _event: BusEvent) {}

    fn wants_vdp_writes(&self) -> bool {
        true
    }

    fn on_vdp_write(&mut self, w: oracle_core::vdp::VdpWrite) {
        // CRAM index 0 is the backdrop; `addr` is its byte address, and the ROM writes it as a word.
        if w.target == oracle_core::vdp::VdpTarget::Cram && w.addr < 2 {
            let v = w.new & 0xFFFF;
            self.seen |= v == 0x00E0 || v == 0x000E;
        }
    }

    fn stop_requested(&self) -> bool {
        self.seen
    }
}

/// **"The ROM has stopped drawing."** Stops once the VDP port block has been *busy at least once* and then
/// completely silent for [`Self::QUIET_FRAMES`] consecutive frames.
///
/// The "busy at least once" latch is not optional: several of these ROMs are silent for the first few frames
/// while they set themselves up, and a naive idle detector stops on that.
///
/// **This condition is only valid for a ROM whose activity is measured to be contiguous**, and it is applied
/// to exactly one (`vdp_sprite_masking`) for that reason. `m68k_bcd` is the counter-example that kept it
/// narrow: instrumented over its full 700-frame budget it touches the VDP on frames 0, 6 and **530** —
/// 523 frames of total silence in the middle while it computes, then it prints its results. An idle detector
/// there stops before the answer exists and produces a confident wrong reading, which is exactly the failure
/// mode the tooling recon (§5) says these instruments must be designed against. `vdp_sprite_masking` was
/// measured the same way over its full 300-frame budget: activity on frames 0-7 contiguously and **not one
/// VDP access on frames 8-299**, so the picture the scraper hashes is final long before the old budget ends.
#[derive(Default)]
struct VdpIdle {
    frame: u64,
    busy_seen: bool,
    last_busy_frame: u64,
    idle_frames: u64,
}

impl VdpIdle {
    /// Consecutive silent frames that count as "done". Generous relative to the measured behaviour (the ROM
    /// it is used on goes silent forever after frame 7), so it is not a re-tuned magic number in disguise.
    const QUIET_FRAMES: u64 = 8;
}

impl BusEventSink for VdpIdle {
    fn on_event(&mut self, event: BusEvent) {
        if event.op == BusOp::Write && is_vdp_port(event.addr) {
            self.busy_seen = true;
            self.last_busy_frame = self.frame;
            self.idle_frames = 0;
        }
    }

    fn on_step_boundary(&mut self, _pc: u32, frame: u64) {
        if frame > self.frame {
            self.frame = frame;
            if self.busy_seen {
                self.idle_frames = frame - self.last_busy_frame;
            }
        }
    }

    fn stop_requested(&self) -> bool {
        self.busy_seen && self.idle_frames >= Self::QUIET_FRAMES
    }
}

/// FNV-1a over one rendered rectangle (used to classify `vdp_sprite_masking`'s verdict glyphs).
fn block_hash(sys: &System, x0: usize, x1: usize, y0: u16, y1: u16) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for line in y0..y1 {
        let px = sys.vdp().render_line(line);
        for (r, g, b) in px.iter().take(x1).skip(x0) {
            for byte in [*r, *g, *b] {
                h ^= byte as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    h
}

/// [`block_hash`]'s **live** twin: the same FNV-1a over the same rectangle, but read out of a
/// [`ScanlineCapture`]'s pixels — the picture the VDP emitted during the run — instead of re-rendered
/// afterwards. `px` is one complete frame, line-major, `width` pixels per line.
///
/// Byte-for-byte the same layout as [`block_hash`] on purpose: the two are compared directly (that
/// comparison IS the control arm in `sprite_masking_glyphs`), so a hash from one and a hash from the other
/// must name the same kind of thing, and the four pinned glyph constants must classify either substrate.
fn block_hash_live(
    px: &[(u8, u8, u8)],
    width: usize,
    x0: usize,
    x1: usize,
    y0: u16,
    y1: u16,
) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for line in y0..y1 {
        let row = &px[line as usize * width..(line as usize + 1) * width];
        for (r, g, b) in row.iter().take(x1).skip(x0) {
            for byte in [*r, *g, *b] {
                h ^= byte as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    h
}

/// `vdp_sprite_masking`'s four verdict glyphs, pinned from rendered pixels (Limitation L3).
///
/// **These are substrate-independent** — verified, not assumed, by `POSTHOC-CARRY` (2026-09-18) when the
/// scraper moved from the post-hoc render to the live capture: all four survived the move with **no
/// re-derivation**. Eight of the nine glyphs are bit-identical through both paths, and the ninth (test 6)
/// hashes live to exactly the [`PASS`] literal below, which was pinned from post-hoc pixels. That was the
/// blocker `F-POSTHOC-STALE-CARRY` named — "the constants are themselves pinned from post-hoc pixels, so
/// the defect could reproduce one layer down" — and it is now settled by measurement.
///
/// [`TICK_CROSS`] currently classifies **nothing**: test 3's second sub-case was the only cross in the
/// corpus and it flipped to a tick when the mid-sprite pixel-budget cut landed (2026-09-16, ledger P1). It
/// is kept because it is a *classifier*, not a pin — a regression that reintroduces a cross must read
/// `TICK/CROSS` rather than `UNKNOWN-GLYPH` — but it is worth knowing that no run exercises it today.
const TICK_TICK: u64 = 0xb498_5631_5ac3_a445;
const TICK_CROSS: u64 = 0xa126_fa46_503f_8e4d;
const PASS: u64 = 0x6609_4bba_88cb_93ed;
const FAIL: u64 = 0x1f88_0fb1_901c_cfe5;

/// Classify one 32x8 verdict rectangle. An unrecognised hash is reported as `UNKNOWN-GLYPH(0x…)` rather
/// than guessed at or dropped — loud, per L3, because any palette or renderer change invalidates the pins.
fn glyph_label(h: u64) -> String {
    match h {
        TICK_TICK => "TICK/TICK".to_string(),
        TICK_CROSS => "TICK/CROSS".to_string(),
        PASS => "PASS".to_string(),
        FAIL => "FAIL".to_string(),
        other => format!("UNKNOWN-GLYPH(0x{other:016x})"),
    }
}

/// Read the on-screen text out of the plane-A nametable, one `String` per cell row.
///
/// Plane A base = `(R2 & $38) << 10`; plane pitch from R16; visible columns from the rendered line width
/// (256 = H32, 320 = H40). A cell's low 11 bits are `font_base + ASCII` for every text ROM here; anything
/// outside printable ASCII becomes a space, so rows compare by their text content after [`squeeze`].
fn text_rows(sys: &System, font_base: u16) -> Vec<String> {
    let vdp = sys.vdp();
    let base = ((vdp.regs()[2] as usize) & 0x38) << 10;
    let pitch = match vdp.regs()[16] & 0x03 {
        0 => 32,
        1 => 64,
        _ => 128,
    };
    let cols = vdp.render_line(0).len() / 8;
    let rows = (ACTIVE_LINES / 8) as usize;
    (0..rows)
        .map(|row| {
            (0..cols)
                .map(|col| {
                    let off = base + (row * pitch + col) * 2;
                    let cell = u16::from_be_bytes([vdp.vram()[off], vdp.vram()[off + 1]]);
                    let c = (cell & 0x07FF).wrapping_sub(font_base);
                    if (0x21..0x7F).contains(&c) {
                        c as u8 as char
                    } else {
                        ' '
                    }
                })
                .collect()
        })
        .collect()
}

/// Collapse runs of whitespace so a scraped row compares by content, not by column padding.
fn squeeze(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------------------------------
// Per-ROM verdict scrapers
// ---------------------------------------------------------------------------------------------------

/// `bcd-verifier-u1` — exhaustive ABCD/SBCD/NBCD value+flag verification. Reports one row per mnemonic
/// with the value-error and flag-error counts; `$00000 $00000` is a clean sweep. Settles well before
/// frame 700 (the sweep is long, hence the frame budget).
fn scrape_m68k_bcd(sys: &mut System) -> String {
    sys.run_frames(700);
    let rows = text_rows(sys, 0x000);
    let mut bad = Vec::new();
    for op in ["abcd", "sbcd", "nbcd"] {
        let row = rows
            .iter()
            .map(|r| squeeze(r))
            .find(|r| r.starts_with(op))
            .unwrap_or_else(|| format!("{op} <row not found>"));
        if row != format!("{op} $00000 $00000") {
            bad.push(row);
        }
    }
    if bad.is_empty() {
        "PASS abcd/sbcd/nbcd 0 value errors, 0 flag errors".to_string()
    } else {
        format!("FAIL {}", bad.join("; "))
    }
}

/// `memtest_68k` — reads every non-lockup address range twice and prints the two words it read, followed
/// by the ROM's OWN built-in real-hardware reference in parentheses (`?` = wildcard nibble). Row shape:
/// `ADDR : A B (EA EB)`. Most mismatches here are the absent open-bus model.
fn scrape_m68k_memory_test(sys: &mut System) -> String {
    sys.run_frames(30);
    let rows = text_rows(sys, 0x100);
    let mut total = 0usize;
    let mut matched = 0usize;
    let mut mismatched = Vec::new();
    for row in rows.iter().map(|r| squeeze(r)) {
        let Some((label, rest)) = row.split_once(" : ") else {
            continue;
        };
        let f: Vec<&str> = rest
            .split_whitespace()
            .map(|t| t.trim_matches(['(', ')']))
            .collect();
        if f.len() != 4 {
            continue;
        }
        total += 1;
        let hit = |got: &str, want: &str| {
            got.len() == want.len()
                && got
                    .chars()
                    .zip(want.chars())
                    .all(|(g, w)| w == '?' || g == w)
        };
        if hit(f[0], f[2]) && hit(f[1], f[3]) {
            matched += 1;
        } else {
            mismatched.push(label.trim().to_string());
        }
    }
    if mismatched.is_empty() {
        format!("{matched}/{total} rows match the ROM's hardware reference")
    } else {
        format!(
            "{matched}/{total} rows match the ROM's hardware reference; mismatch: {}",
            mismatched.join(", ")
        )
    }
}

/// `Multitap - IO Sample Program` — probes both controller ports and names the detected device. A
/// three-button pad must read back as `JOYPAD` on port 1 and port 2.
fn scrape_io_sample(sys: &mut System) -> String {
    sys.run_frames(160);
    let rows = text_rows(sys, 0x000);
    let found: Vec<String> = rows
        .iter()
        .map(|r| squeeze(r))
        .filter(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_uppercase() || c == ' '))
        .filter(|r| r.contains("JOYPAD"))
        .collect();
    if found.len() == 2 {
        "PASS port1=JOYPAD port2=JOYPAD".to_string()
    } else {
        format!("FAIL {} port(s) report JOYPAD (want 2)", found.len())
    }
}

/// `itest` — sweeps illegal / privileged / unimplemented encodings and checks each traps correctly. No
/// text: the verdict is the backdrop colour — blue `$0E00` while the sweep runs, then green `$00E0` =
/// pass, red `$000E` = fail. A completed sweep (all 11529 table entries trapping) settles by ~frame 9;
/// the old 2-frame budget dated from when the ROM failed fast (K1) and never reached the verdict write.
///
/// **Converted from a frame budget to a stop condition (2026-08-14).** `run_frames(20)` is now
/// "run until the ROM writes its verdict, and give up after 20 frames" — the 20 is a bound, not a guess at
/// how long the sweep takes. Measured: the ROM writes CRAM index 0 three times — `$0000` and `$0E00` (blue,
/// running) on frame 0, then `$00E0` (green) on frame **9** — so the stop fires at 9 and the 11 frames after
/// it were always dead time.
fn scrape_m68k_illegal(sys: &mut System) -> String {
    let mut verdict = BackdropVerdict::default();
    let stop = sys.run_frames_with_sink(20, &mut verdict);
    assert!(
        stop.fired(),
        "m68k_illegal wrote no backdrop verdict within 20 frames (stopped at {stop:?}) — the scrape below \
         would be reading a mid-sweep screen, so fail loudly instead"
    );
    let cram = sys.vdp().cram();
    let backdrop = u16::from_be_bytes([cram[0], cram[1]]);
    match backdrop {
        0x00E0 => "PASS backdrop=$00E0 (green)".to_string(),
        0x000E => "FAIL backdrop=$000E (red); pass would be $00E0 (green)".to_string(),
        other => format!("INDETERMINATE backdrop=${other:04X} (want $00E0 green / $000E red)"),
    }
}

/// `SpriteMaskingTestRom` — nine sprite-masking / dot-overflow tests, verdict drawn at the right edge as a
/// 32x8 glyph. Tests 4-8 print PASS/FAIL; tests 1-3 and 9 print a pair of tick/cross marks (one per
/// sub-case). The tick and cross share identical nametable cells, so the glyphs are classified by hashing
/// the rendered pixels — the framebuffer is the only channel that distinguishes them.
///
/// **Run in BOTH of the ROM's screen modes** (2026-09-18, `TESTROM-H40-HALF`). This ROM boots in H32 and
/// re-runs all nine tests in H40 when the mode is toggled; from this row's first pin (2026-07-25) until
/// 2026-09-18 the harness measured the power-on half only, so half of what the ROM tests — the H40 per-line
/// sprite limit (20, not 16) and dot limit (320, not 256) — was never scraped at all. The scrape is now two halves of one boot:
/// H32 to idle, the toggle, H40 to idle, each half [`ProvenScreen`]-gated and each contributing its own
/// segment to the row.
///
/// The toggle is driven by `C`, not the `Start` the ROM's own on-screen text names. That was open question
/// Q1 and it is now **answered**: the ROM's wait routine at `$0002FA` writes `#$00` to the *Data* register
/// `$A10003` and then spins on `btst #5,$A10003`, and the string `$A10009` — the P1 *Control* (direction)
/// register — does not occur anywhere in the ROM's 256 KB image. TH is therefore never configured as an
/// output, never driven low, and floats high on the port pull-up (recon IO3), where bit 5 is `C`. It is a
/// bug in the ROM, not in this core; `vdp_port_access`, which does write `$A10009`, is advanced by `Start`
/// through this same harness. See `docs/2026-09-18-h40-half.md`.
///
/// ## The pixels come from the LIVE render path, and the post-hoc one is kept as the control
///
/// (2026-09-18, `POSTHOC-CARRY`; closes `F-POSTHOC-STALE-CARRY`.) This scraper used to read
/// [`block_hash`] only — `Vdp::render_line`, a *pure* re-render of a line against whatever state the VDP
/// holds after the run. That path re-seeds R10 sprite masking from `sprite_dot_overflow_carry` **as it
/// stands at the end of the frame** on every line, instead of advancing it line by line the way
/// `Vdp::render_scanline` does during the run. For eight of this ROM's nine glyphs the two substrates are
/// bit-identical; for test 6 — `MASK S1 ON DOT OVERFLOW`, i.e. the ROM's test *of the carry itself* — they
/// are not, and the post-hoc reading was a `FAIL` the machine never drew.
///
/// So the glyphs are now classified from a [`ScanlineCapture`] of the last **completed** frame — the
/// pixels the VDP actually emitted — and the post-hoc classification is computed alongside and **reported
/// in the row** as `[post-hoc path differs: …]` / `[post-hoc path agrees]`. That bracket is the control
/// arm: it makes a future divergence between the two render paths a scorecard diff instead of something
/// that arrives disguised as a ROM's verdict, in either direction —
///
/// * the live path collapsing back to post-hoc (a lost capture, a re-pathed `block_hash`) empties the
///   bracket **and** flips `6=` to `FAIL`;
/// * the post-hoc path being "fixed" until it agrees — the trap the old row comment warned about in prose —
///   empties the bracket while `6=` stays `PASS`;
/// * a *new* disagreement, on any glyph, adds an entry.
///
/// Each of those moves the single scorecard assert. `the_glyph_scrape_reads_the_live_render_path` below is
/// the non-vacuity half: it proves the two substrates are still distinguishable here, so the choice between
/// them is load-bearing rather than decorative.
fn scrape_vdp_sprite_masking(sys: &mut System) -> String {
    sprite_masking_halves(sys)
        .iter()
        .map(sprite_masking_row)
        .collect::<Vec<_>>()
        .join(" | ")
}

/// One of the ROM's two screen modes, named by its **display width in pixels** — the one property of the
/// mode that the picture itself carries, so it can be measured rather than believed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Screen {
    H32,
    H40,
}

impl Screen {
    /// The active display width. H32 = 32 cells x 8px, H40 = 40 cells x 8px.
    const fn width(self) -> usize {
        match self {
            Screen::H32 => 256,
            Screen::H40 => 320,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Screen::H32 => "H32",
            Screen::H40 => "H40",
        }
    }

    /// The x-range of the 32x8 verdict rectangle **in this mode**, half-open.
    ///
    /// Both entries are derived from that mode's own picture, independently — H40 is **not** the H32
    /// number scaled, copied, or assumed, and it is worth saying explicitly that it comes out equal:
    ///
    /// * **H32** (the original, 2026-07-25): nametable row 6 carries exactly four non-zero cells past the
    ///   test's name text, at plane columns 27, 28, 29, 30 (`$010E $010F $010C $010D`); 27 x 8 = 216 and
    ///   31 x 8 = 248.
    /// * **H40** (2026-09-18): the same recon repeated on the H40 screen. The nametable's verdict cells
    ///   are at plane columns **27, 28, 29, 30 again** — the ROM does not right-align the verdicts, so in
    ///   a 40-column screen they sit 9 cells in from the right edge rather than at it. Corroborated off
    ///   the framebuffer, which is the reading that does not depend on a scroll assumption: in the H40
    ///   capture every lit pixel of every one of the nine verdict glyphs falls in x 216..=246, with the
    ///   72 pixels from 248 to 319 blank on all nine rows.
    const fn glyph_x(self) -> (usize, usize) {
        match self {
            Screen::H32 => (216, 248),
            Screen::H40 => (216, 248),
        }
    }
}

/// A [`Screen`] that has been **measured off the picture the run actually produced**, and the only ticket
/// into the glyph scrape.
///
/// ## Why this is a type and not an assert
///
/// The failure this whole H40 half exists to avoid is silent: if the toggle does not take, the second half
/// re-scrapes the *first* half's screen, every one of the nine glyphs classifies, and the row reads
/// `H40: 1=TICK/TICK …` — nine plausible verdicts for a mode the run never entered. An `assert!` next to
/// the scrape would catch that today and stop catching it the first time someone reorders the function.
///
/// So the proof *gates* the scrape structurally: [`establish`](Self::establish) is the only constructor,
/// it refuses loudly, and [`sprite_masking_classify`] cannot be called without the value it returns. An
/// unproven mode is not a thing this file can represent. That is the same shape as the
/// `[post-hoc path …]` bracket — agreement stated positively, so an absent disagreement can never read as
/// an unmeasured one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ProvenScreen(Screen);

impl ProvenScreen {
    /// The only way to make one. `want` is the mode the caller set the machine up for, `measured` is the
    /// width the **captured frame ended on** (`CompletedFrame::width`, never a post-hoc re-query of the
    /// chip — after a mode switch the chip answers for the *next* frame), and `must_differ_from` is the
    /// mode the previous half of the run proved, if any.
    ///
    /// Both checks are load-bearing and neither implies the other:
    ///
    /// * `measured == want.width()` catches a toggle that did not take, a toggle that took in the wrong
    ///   direction, and a capture read at the wrong width.
    /// * `measured != previous` catches the case where `want` itself was wrong — if a future edit set both
    ///   halves up for the same mode, the first check would pass twice and the row would carry two
    ///   readings of one screen under two labels.
    fn establish(want: Screen, measured: usize, must_differ_from: Option<ProvenScreen>) -> Self {
        assert_eq!(
            measured,
            want.width(),
            "the captured frame is {measured}px wide, but this half of the run was set up for {} \
             ({}px). The mode toggle did not take, so every glyph below would be read off the OTHER \
             mode's screen and would classify perfectly — nine plausible verdicts for a screen the run \
             never entered. Check the `C` press in `sprite_masking_halves` (and note the ROM's on-screen \
             text names the wrong button — see the Q1 answer in docs/2026-09-18-h40-half.md).",
            want.label(),
            want.width()
        );
        if let Some(prev) = must_differ_from {
            assert_ne!(
                prev.0,
                want,
                "both halves of this run were set up for {} — the scorecard would carry two readings of \
                 one screen under two labels",
                want.label()
            );
        }
        Self(want)
    }

    const fn screen(self) -> Screen {
        self.0
    }
}

/// One half of the run: the mode it was **proved** to be in, and the nine verdict glyphs read through both
/// substrates.
struct SpriteMaskingHalf {
    screen: ProvenScreen,
    live: Vec<String>,
    post_hoc: Vec<String>,
}

/// Both halves of this ROM's run, in order: H32 (power-on) then H40 (after the toggle).
///
/// The H32 half is byte-for-byte the run it always was — same boot, same seed, same sink, same stop — so
/// the H32 segment of the row is unmoved by adding the H40 one. The toggle and the second run happen
/// strictly after the H32 half's post-hoc reading has been taken, because that reading is of
/// `sys`'s *current* VDP state and the toggle changes it.
///
/// **The press.** `C`, held for two frames on port 1 and then released. Measured: at the H32 idle stop the
/// ROM is parked at pc `$316`, the "wait for bit 5 to go low" half of its wait routine, so it sees the
/// press on the first frame of it; the toggle is edge-shaped, not level-shaped (holding `C` for ten frames
/// toggles once, not ten times), and the release matters because the routine's *other* half waits for bit
/// 5 to come back high before it will accept anything again. Two frames rather than one is slack, not a
/// tuned constant: the assert in [`ProvenScreen::establish`] is what says whether it worked.
fn sprite_masking_halves(sys: &mut System) -> [SpriteMaskingHalf; 2] {
    let h32 = sprite_masking_half(sys, Screen::H32, None);
    sys.set_pad(
        oracle_core::io::PadPort::P1,
        Pad {
            c: true,
            ..Default::default()
        },
    );
    sys.run_frames(2);
    sys.set_pad(oracle_core::io::PadPort::P1, Pad::default());
    let h40 = sprite_masking_half(sys, Screen::H40, Some(h32.screen));
    [h32, h40]
}

/// Run the ROM until it stops drawing, prove the mode off the captured picture, then classify the nine
/// verdict glyphs through **both** substrates: `(live, post-hoc)`, test 1..=9 in order.
///
/// ## Which frame the live reading is of, and what that costs
///
/// [`VdpIdle`] stops the run *mid-frame* (at the first instruction boundary after the quiet window), so
/// there is no live picture of "the stop point" to read — a per-scanline capture only has whole frames.
/// The decision is therefore: **the last frame the run completed before the stop**, which
/// `Retain::LastFrame` latches at the frame boundary. Measured for this ROM: the stop fires at frame 15,
/// the captured frame is index 14, and the ROM's last VDP access of any kind is in frame 7.
///
/// The cost is that the old convention no longer holds. The 2026-08-14 idle-stop conversion could say
/// "`block_hash` re-renders from live VDP state, so stopping mid-frame is irrelevant to it" — true of a
/// post-hoc re-render, false of a capture. Four guards replace that sentence, and all four are loud:
///
/// * the capture must hold **exactly** one complete frame of active lines (a torn or short capture is a
///   real failure mode — an early draft of `scanline_goldens.rs` hashed a fragment and got four confident
///   garbage hashes). `CompletedFrame` is what enforces this now, and it is stricter than the
///   `pixels().len() == width * ACTIVE_LINES` assert it replaces: that assert compared the capture's
///   length against a width re-queried from the chip, which after a mode switch is the *next* frame's
///   width, so in the H40 half it would have compared 224 lines of 320px against 224 x 320 and agreed
///   for a reason unrelated to the picture. `completed_frame` refuses a log shorter than a frame and
///   refuses a tail whose per-line widths do not sum to the pixels held;
/// * **no line of the captured frame is a different width from the frame itself.** This is the
///   non-vacuous heir of the old length assert and the one that can see a tear *across* a mode switch:
///   `CompletedFrame::pixels` pads a short line with black and cuts a long one, so a frame half-drawn in
///   H32 and half in H40 reads as a clean 320-wide picture with a black left-hand gutter, and hashes
///   confidently. Measured today: every line of the H32 capture is 256px and every line of the H40
///   capture is 320px;
/// * the run must have completed at least one frame at all;
/// * the captured frame must be **strictly later than the last frame in which the ROM touched the VDP**,
///   so the picture scraped is one the ROM had already finished drawing. If this ROM ever starts writing
///   later than the quiet window allows, that fires instead of the scrape quietly reading a stale frame.
///
/// Attaching the capture does not change the machine: `wants_scanlines` moves the run loop from
/// `Vdp::advance_scanline` to `Vdp::render_scanline`, which `render.rs`'s
/// `the_cheap_scanline_advance_leaves_the_same_machine` pins to be `PartialEq`-identical in every `Vdp`
/// field. Measured here as well: with and without the capture attached the run stops at the same record
/// (frame 15, pc `$316`, mclk 13,440,630) and the post-hoc frame hash is byte-identical.
fn sprite_masking_half(
    sys: &mut System,
    want: Screen,
    previous: Option<ProvenScreen>,
) -> SpriteMaskingHalf {
    // Converted from a frame budget to a stop condition (2026-08-14): "run until the ROM stops drawing,
    // and give up after 300 frames". Measured over the full old budget, every VDP access this ROM makes is
    // on frames 0-7 and there is not one on frames 8-299, so the screen is final by frame 7 and the stop
    // fires at 15 (7 + QUIET_FRAMES). Measured again for the H40 half (2026-09-18): the press lands in
    // frame 15, the ROM redraws over frames 15-22, and the stop fires at frame 30 on the captured frame 29.
    let mut sink = Fanout::new(VdpIdle::default(), ScanlineCapture::new(Retain::LastFrame));
    let stop = sys.run_frames_with_sink(300, &mut sink);
    assert!(
        stop.fired(),
        "vdp_sprite_masking never went idle within 300 frames in its {} half (stopped at {stop:?}) — the \
         idle condition no longer describes this ROM, so fail loudly rather than silently falling back to \
         the old budget",
        want.label()
    );
    let Fanout { a: idle, b: cap } = sink;
    let frame = cap.completed_frame(ACTIVE_LINES as usize).unwrap_or_else(|| {
        panic!(
            "the {} half completed no whole frame of {ACTIVE_LINES} active lines, so there is no live \
             picture to classify — the capture logged {} lines holding {} pixels. The idle stop must \
             always sit well after a frame boundary.",
            want.label(),
            cap.lines().len(),
            cap.pixels().len()
        )
    });
    let line_widths: Vec<usize> = frame.rows().map(|r| r.len()).collect();
    refuse_a_ragged_frame(&line_widths, frame.width(), want.label());

    // THE GATE. Nothing below may read a pixel until the mode the picture is in has been established from
    // the picture itself; `sprite_masking_classify` takes the `ProvenScreen` this line either produces or
    // panics trying, so there is no path to a glyph that skips it.
    let screen = ProvenScreen::establish(want, frame.width(), previous);

    let index = cap.last_frame_index().expect(
        "the run stopped before completing a single frame, so there is no live picture to classify — the \
         idle stop must always sit well after the ROM's last frame boundary",
    );
    assert!(
        index > idle.last_busy_frame,
        "the {} half's captured frame ({index}) is not strictly later than the last frame in which this \
         ROM touched the VDP ({}), so the glyphs would be read off a picture the ROM had not finished \
         drawing",
        want.label(),
        idle.last_busy_frame
    );

    let px: Vec<(u8, u8, u8)> = frame.pixels().collect();
    let (live, post_hoc) = sprite_masking_classify(screen, &px, sys);
    SpriteMaskingHalf {
        screen,
        live,
        post_hoc,
    }
}

/// Refuse a captured frame whose lines are not all the frame's own width.
///
/// The failure mode this exists for is a frame **torn across a mode switch**: lines drawn before the R12
/// write are 256px and the rest 320px, and `CompletedFrame::pixels` pads the short ones with black and cuts
/// the long ones, so such a frame hands back a clean 320x224 rectangle with a black gutter and hashes
/// confidently. It is the non-vacuous heir of the `pixels().len() == width * ACTIVE_LINES` assert this
/// parcel retired (that one compared the capture against a width re-queried from the *chip*, which after a
/// mode switch answers for the next frame).
///
/// A free function with its own test (`a_ragged_captured_frame_is_refused`) rather than an inline assert,
/// and the reason is worth recording: **this ROM cannot produce a torn retained frame**, so the guard is
/// not red-provable through the run. The tear can only happen in the frame the R12 write lands in, that
/// write is itself a VDP write, and `VdpIdle` will not stop until 8 frames after the last VDP write — so
/// the torn frame is always at least nine frames older than the retained one. Structurally unreachable
/// today, and a guard whose subject is unreachable is exactly the kind that rots into decoration. So the
/// subject is supplied directly instead.
fn refuse_a_ragged_frame(line_widths: &[usize], width: usize, mode: &str) {
    let ragged: Vec<usize> = line_widths
        .iter()
        .copied()
        .filter(|&w| w != width)
        .collect();
    assert!(
        ragged.is_empty(),
        "the {mode} half's captured frame is ragged: {} of its {} lines are not {width}px wide ({:?}). \
         `CompletedFrame::pixels` would pad or cut those lines and hand back a confident rectangle anyway, \
         so a frame torn across a mode switch must fail here instead.",
        ragged.len(),
        line_widths.len(),
        {
            let mut u = ragged.clone();
            u.sort_unstable();
            u.dedup();
            u
        }
    );
}

/// Classify the nine verdict glyphs of one half-run through both substrates: `(live, post-hoc)`, test
/// 1..=9 in order. Takes a [`ProvenScreen`] rather than a width because the mode has to have been
/// established from the machine before any of these pixels means anything — see that type's docs.
fn sprite_masking_classify(
    screen: ProvenScreen,
    px: &[(u8, u8, u8)],
    sys: &System,
) -> (Vec<String>, Vec<String>) {
    let (x0, x1) = screen.screen().glyph_x();
    let width = screen.screen().width();
    // Cell rows 6..=14, one per test, 8 lines each. Derived in both modes from the picture: the nine
    // verdict glyphs occupy exactly lines 48..=119 and every one of them is 8-line-aligned at 48 + 8k.
    let live = (6u16..15)
        .map(|row| glyph_label(block_hash_live(px, width, x0, x1, row * 8, row * 8 + 8)))
        .collect();
    let post_hoc = (6u16..15)
        .map(|row| glyph_label(block_hash(sys, x0, x1, row * 8, row * 8 + 8)))
        .collect();
    (live, post_hoc)
}

/// Render one half's segment of the scorecard row: the **proved** mode label, the live verdicts, then the
/// **complete list** of glyphs the post-hoc path reads differently. The bracket is never omitted — "the
/// paths agree" is stated positively, so an absent disagreement can never be confused with an unmeasured
/// one — and the label comes from a [`ProvenScreen`], so neither can an unmeasured mode.
fn sprite_masking_row(half: &SpriteMaskingHalf) -> String {
    let SpriteMaskingHalf {
        screen,
        live,
        post_hoc,
    } = half;
    let mut out = vec![format!("{}:", screen.screen().label())];
    for (n, label) in live.iter().enumerate() {
        out.push(format!("{}={label}", n + 1));
    }
    let diffs: Vec<String> = live
        .iter()
        .zip(post_hoc)
        .enumerate()
        .filter(|(_, (l, p))| l != p)
        .map(|(n, (_, p))| format!("{}={p}", n + 1))
        .collect();
    out.push(if diffs.is_empty() {
        "[post-hoc path agrees]".to_string()
    } else {
        format!("[post-hoc path differs: {}]", diffs.join(" "))
    });
    out.join(" ")
}

/// The purely-visual ROMs: no machine-readable verdict, so the frame hash is a **regression baseline
/// only** — it says "the picture did not change", never "the picture is right". `note` carries a
/// NOT-RENDERABLE caveat where our capture model structurally cannot show what the ROM demonstrates.
fn scrape_visual(sys: &mut System, note: &str) -> String {
    sys.run_frames(120);
    format!("{note}frame_hash=0x{:016x}", frame_hash(sys))
}

/// `color_1536` — the 1536-colour trick: CRAM is rewritten *mid-scanline*, so the picture exists only while
/// the frame is being drawn. Captured per-scanline (Limitation L1, narrowed 2026-08-03): the end-of-frame
/// framebuffer showed 4 distinct colours (a black + flat-grey rectangle on the backdrop), the per-scanline
/// capture shows the real ~1400-colour gradient the ROM demonstrates. Still a regression baseline only — the
/// ROM prints no verdict — but now a hash of the right picture.
fn scrape_color_1536(sys: &mut System) -> String {
    format!(
        "VISUAL-BASELINE frame_hash=0x{:016x} (per-scanline capture)",
        frame_hash_scanline(sys, 120)
    )
}

// ---------------------------------------------------------------------------------------------------
// The scorecard
// ---------------------------------------------------------------------------------------------------

fn scrape(name: &str) -> Option<String> {
    if name == "vdp_port_access" {
        // Read from the one shared whole-ROM run rather than booting the ROM a second time.
        return scrape_vdp_port_access();
    }
    let mut sys = boot(name)?;
    Some(match name {
        "m68k_bcd" => scrape_m68k_bcd(&mut sys),
        "m68k_memory_test" => scrape_m68k_memory_test(&mut sys),
        "io_sample" => scrape_io_sample(&mut sys),
        "m68k_illegal" => scrape_m68k_illegal(&mut sys),
        "vdp_sprite_masking" => scrape_vdp_sprite_masking(&mut sys),
        "cram_flicker" => scrape_visual(
            &mut sys,
            "NOT-RENDERABLE (CRAM-write artefact, sub-scanline) ",
        ),
        "direct_color_dma" => scrape_visual(&mut sys, "NOT-RENDERABLE (sub-scanline CRAM) "),
        "color_1536" => scrape_color_1536(&mut sys),
        _ => scrape_visual(&mut sys, "VISUAL-BASELINE "),
    })
}

/// The instrument. Builds the whole scorecard and compares it to [`BASELINE`] in ONE assert, so a diff
/// lists every ROM whose outcome moved.
#[test]
fn testrom_conformance_scorecard() {
    let mut scorecard: Vec<(&str, String)> = Vec::new();
    for name in ROMS {
        if let Some(outcome) = scrape(name) {
            scorecard.push((name, outcome));
        }
    }

    let ran = scorecard.len();
    let present = ROMS
        .iter()
        .filter(|n| Path::new(&rom_path(n)).exists())
        .count();
    eprintln!("conformance: {ran}/{} ROMs ran", ROMS.len());
    for (name, outcome) in &scorecard {
        eprintln!("  {name:<20} {outcome}");
    }

    // Count guard: every vendored ROM present on disk MUST have produced a row. A ROM that boots into a
    // panic-free nothing, or a scraper that silently bails, cannot hide behind the soft-skip path.
    assert_eq!(
        ran, present,
        "{present} vendored ROMs are on disk but only {ran} produced a scorecard row"
    );

    if ran == 0 {
        eprintln!("SKIP: no vendored test ROMs at all — run tools/fetch-testroms.sh");
        return;
    }

    let expected: Vec<(&str, String)> = BASELINE
        .iter()
        .filter(|(n, _)| scorecard.iter().any(|(s, _)| s == n))
        .map(|(n, o)| (*n, squeeze(o)))
        .collect();
    let actual: Vec<(&str, String)> = scorecard.iter().map(|(n, o)| (*n, squeeze(o))).collect();

    assert_eq!(
        actual, expected,
        "test-ROM conformance moved. This harness is NON-GATING: a diff is information, not \
         automatically a failure to 'fix'. Confirm the change is intended, then update BASELINE and \
         docs/2026-07-25-testrom-conformance.md together."
    );
}

/// Structural guard: the pinned baseline must cover exactly the ROM list (which mirrors
/// `tools/fetch-testroms.sh`). Without this, dropping a ROM from `ROMS` would shrink the scorecard AND
/// the filtered baseline in lockstep and still pass.
#[test]
fn baseline_covers_every_rom() {
    assert_eq!(
        BASELINE.len(),
        ROMS.len(),
        "BASELINE has {} entries for {} ROMs",
        BASELINE.len(),
        ROMS.len()
    );
    for name in ROMS {
        assert!(
            BASELINE.iter().any(|(n, _)| n == name),
            "ROM {name} has no pinned baseline entry"
        );
    }
    let mut sorted = ROMS.to_vec();
    sorted.sort_unstable();
    assert_eq!(
        sorted, ROMS,
        "keep ROMS sorted so the scorecard diff is stable"
    );
}

/// **The control arm: render the verdict both ways and require them to agree — and say so when they do
/// not.** (`POSTHOC-CARRY`, 2026-09-18.)
///
/// This is the one line nobody ever ran. From the first golden in this tree (`golden_frames.rs`,
/// 2026-07-16) to 2026-09-18, every golden read the *post-hoc*
/// `Vdp::render_line`; the 2026-08-15 per-scanline survey found that six of seventeen ROMs draw a picture
/// that re-render cannot reproduce, and for `vdp_sprite_masking` the gap was not a hash but **a recorded
/// verdict** — `6=FAIL`, indistinguishable in the output from a real emulator defect, because an artefact
/// and a failure are the same glyph. The scorecard row now carries both readings (see
/// [`sprite_masking_row`]), so any movement in their relationship is a scorecard diff. This test is what
/// stops that row from decaying into a string nobody can read:
///
/// 1. **The two substrates are still distinguishable here.** If they were not, pinning the live one would
///    be decoration and the row's bracket would be a tautology. Measured today: exactly one glyph differs.
/// 2. **The row is oriented live-in-the-body, post-hoc-in-the-bracket.** Built from labels this test
///    measures itself, not copied from [`BASELINE`], so swapping the two substrates fails here even if
///    someone re-pinned the row to match.
/// 3. **Neither reading is `UNKNOWN-GLYPH`.** A rectangle nobody can classify must never be reported as an
///    agreement — "couldn't measure" is not "the same".
///
/// A mutation that re-paths the live classifier back to `render_line` fails assertions 1 and 2; a mutation
/// that breaks the live carry-seeding in `Vdp::sprite_line` fails 1, 2 and the scorecard row together.
/// If the **post-hoc** path ever legitimately catches up — a stateful re-render, a carry replayed from
/// line 0 — assertion 1 is the thing that fires, and that is the correct outcome: it is a real change to
/// the instrument and must be re-derived and re-pinned with evidence, never quietly absorbed.
#[test]
fn the_glyph_scrape_reads_the_live_render_path() {
    let Some(mut sys) = boot("vdp_sprite_masking") else {
        eprintln!("SKIP: vdp_sprite_masking not vendored");
        return;
    };
    let halves = sprite_masking_halves(&mut sys);

    // (0) The two halves are two DIFFERENT screens. Without this the three checks below could all hold
    // twice over one mode — which is exactly what a toggle that silently failed would look like.
    // `ProvenScreen::establish` already refuses that inside the run; this restates it where the test's own
    // premise ("both of the ROM's modes are measured") is stated, so the premise cannot rot separately.
    assert_eq!(
        halves.len(),
        2,
        "this ROM runs its nine tests in two screen modes"
    );
    assert_ne!(
        halves[0].screen, halves[1].screen,
        "both halves proved the same screen mode, so only one of the ROM's two modes is measured"
    );
    assert_eq!(halves[0].screen.screen(), Screen::H32, "power-on is H32");
    assert_eq!(
        halves[1].screen.screen(),
        Screen::H40,
        "the toggle gives H40"
    );

    for half in &halves {
        let mode = half.screen.screen().label();
        let SpriteMaskingHalf { live, post_hoc, .. } = half;
        assert_eq!(live.len(), 9, "this ROM has nine verdict glyphs");
        assert_eq!(post_hoc.len(), live.len());

        // (3) loud on unmeasurable, before anything is compared.
        for (n, (l, p)) in live.iter().zip(post_hoc).enumerate() {
            for (which, label) in [("live", l), ("post-hoc", p)] {
                assert!(
                    !label.starts_with("UNKNOWN-GLYPH"),
                    "{mode} test {} classifies as {label} through the {which} path — the four pinned \
                     glyph hashes no longer describe what this ROM draws (Limitation L3). Re-derive them \
                     from the run and re-pin BASELINE; do NOT let an unclassifiable rectangle read as an \
                     agreement.",
                    n + 1
                );
            }
        }

        // (1) non-vacuity: the choice of substrate has to be able to change the answer. Required in BOTH
        // modes — the H40 half is a second, independent instance of the same divergence, so a fix that
        // quietly repaired one mode's carry seeding and not the other's would fire here.
        let differing: Vec<usize> = (0..live.len())
            .filter(|&n| live[n] != post_hoc[n])
            .collect();
        assert!(
            !differing.is_empty(),
            "in {mode} the live and post-hoc render paths now classify all nine glyphs identically, so \
             this scraper's choice of substrate no longer carries any information. On 2026-09-18 exactly \
             one differed IN EACH MODE (test 6, MASK S1 ON DOT OVERFLOW: live PASS vs post-hoc FAIL, the \
             stale sprite_dot_overflow_carry). Either the live capture stopped being live — check \
             block_hash_live and the ScanlineCapture wiring — or the post-hoc path genuinely caught up, \
             which is a real instrument change: re-derive this guard and the BASELINE row together, with \
             the evidence."
        );

        // (2) orientation: the segment states the LIVE verdicts and reports the post-hoc reading as the
        // exception, under the mode label the machine proved.
        let row = sprite_masking_row(half);
        assert!(
            row.starts_with(&format!("{mode}: ")),
            "the segment must be labelled with the mode its pixels were measured in: {row}"
        );
        for (n, label) in live.iter().enumerate() {
            assert!(
                row.contains(&format!("{}={label}", n + 1)),
                "the {mode} segment does not state test {}'s LIVE verdict ({label}): {row}",
                n + 1
            );
        }
        let bracket: Vec<String> = differing
            .iter()
            .map(|&n| format!("{}={}", n + 1, post_hoc[n]))
            .collect();
        assert!(
            row.ends_with(&format!("[post-hoc path differs: {}]", bracket.join(" "))),
            "the {mode} segment must end by naming every glyph the post-hoc path reads differently ({}): \
             {row}",
            bracket.join(" ")
        );
        eprintln!(
            "CONTROL ARM the_glyph_scrape_reads_the_live_render_path [{mode}]: OK — 9 glyphs classified \
             through both substrates, {} differing ({})",
            differing.len(),
            bracket.join(" ")
        );
    }
}

/// [`refuse_a_ragged_frame`] must actually refuse, and must accept a uniform frame.
///
/// Both halves matter. The accept half is what stops the guard from being tightened into something that
/// fires on the real run (which would look like a broken ROM, not a broken guard); the refuse half is the
/// gate, and it is tested here rather than through the run because the run **cannot** produce its subject —
/// see that function's docs for why the tear is structurally nine frames too old to be retained.
///
/// The widths used are the real ones: 256 = H32, 320 = H40, and the ragged case is the shape a frame torn
/// across the mode switch takes — the lines before the R12 write at the old width, the rest at the new one.
#[test]
fn a_ragged_captured_frame_is_refused() {
    let lines = ACTIVE_LINES as usize;

    // Accept: a whole H32 frame, and a whole H40 frame. These are the two shapes the real run produces,
    // measured (every line of the H32 capture is 256px, every line of the H40 capture is 320px).
    refuse_a_ragged_frame(&vec![256; lines], 256, "H32");
    refuse_a_ragged_frame(&vec![320; lines], 320, "H40");

    // Refuse: torn across the switch at line 87, which is the shape the toggle frame actually has (the R12
    // write lands mid-frame). `width` is what the frame ENDED on, so the 88 short lines are the ragged ones.
    let mut torn = vec![256usize; 88];
    torn.extend(std::iter::repeat_n(320usize, lines - 88));
    let panicked = std::panic::catch_unwind(|| refuse_a_ragged_frame(&torn, 320, "H40"));
    let msg = panicked.expect_err(
        "a frame torn across the mode switch must be refused, not padded into a rectangle",
    );
    let msg = msg
        .downcast_ref::<String>()
        .map(String::as_str)
        .unwrap_or("<non-string panic>");
    for want in ["ragged", "88 of its 224 lines", "320px wide", "[256]"] {
        assert!(
            msg.contains(want),
            "the refusal must name what it saw: {want:?} missing from {msg:?}"
        );
    }
    eprintln!("GATE a_ragged_captured_frame_is_refused: OK — uniform 256 and 320 frames accepted, a frame torn at line 88 refused");
}

/// CI guard: the vendored test ROMs MUST be present under CI, so a fetch failure fails LOUDLY instead of
/// every ROM skipping cleanly and the scorecard passing VACUOUSLY. Locally (no `CI` env var) this is a
/// no-op — fetching is optional for dev and the per-ROM `SKIP` guards keep the suite friendly.
///
/// The `CORPUS GUARD ...: OK` banner is printed only after every assertion has held — a name in a green log
/// is not evidence, because libtest prints `... ok` identically for a guard that returned on line one. See
/// `singlestep_m68000.rs`'s guard and `tools/ci-corpus-guards.sh`.
#[test]
fn vendor_data_present_when_running_in_ci() {
    if std::env::var_os("CI").is_none() {
        // local dev: the per-ROM SKIP guards handle an absent vendor dir
        println!("CORPUS GUARD conformance_roms: SKIPPED (no CI env var; local dev)");
        return;
    }
    assert!(
        Path::new(VENDOR_DIR).exists(),
        "CI: vendored test-ROM dir {VENDOR_DIR} is missing — tools/fetch-testroms.sh must run before \
         the test job (a missing dir makes the whole scorecard skip and pass vacuously)"
    );
    for name in ROMS {
        let p = rom_path(name);
        assert!(
            Path::new(&p).exists(),
            "CI: vendored test ROM {p} is missing — tools/fetch-testroms.sh did not fetch the full corpus"
        );
    }
    println!(
        "CORPUS GUARD conformance_roms: OK — {} pinned test ROMs present under {VENDOR_DIR}",
        ROMS.len()
    );
}

// ---------------------------------------------------------------------------------------------------
// VDPFIFOTesting (`vdp_port_access`), the WHOLE ROM: one shared run, its own result records, its own verdicts
// ---------------------------------------------------------------------------------------------------

/// How many tests `vdp_port_access` runs before it prints its last page and loops back to test 1 (its main
/// list at ROM `$030C..$0D66` ends with a `$FFFF` page marker and `bra $02DA`). A run that never gets its
/// `Results:` line to this total is a failure, never a smaller check.
const PORT_ACCESS_TESTS: u32 = 122;

/// One result record the ROM writes to work RAM for each of its tests.
struct PortAccessRecord {
    /// 1-based test number. The ROM numbers tests by list position (`$FFFF00`, `d5` in the display routine),
    /// so this is the record's ordinal in the list, which the decoder counts the same way.
    number: usize,
    /// The screen page the ROM's display routine shows this record on (1-based); see [`port_access_records`].
    page: usize,
    title: String,
    /// The word after the title: non-zero when the record carries an expected-value table.
    has_expected: bool,
    /// The record's data length in bytes (word 2, signed as the display routine tests it).
    data_len: i16,
    /// The ROM's expected-value bytes (copied from its own table), empty when `has_expected` is false.
    expected: Vec<u8>,
    /// What the VDP answered, `data_len` bytes.
    actual: Vec<u8>,
}

/// Big-endian words of a byte string (a trailing odd byte becomes a word's high half).
fn be_words(b: &[u8]) -> Vec<u16> {
    b.chunks(2)
        .map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]))
        .collect()
}

impl PortAccessRecord {
    fn expected_words(&self) -> Vec<u16> {
        be_words(&self.expected)
    }

    fn actual_words(&self) -> Vec<u16> {
        be_words(&self.actual)
    }

    /// **The verdict the ROM itself counts**, transcribed from its display routine. `$0F9E` prints the
    /// record; its compare loop (`$0FEA..$1020`, 32 bytes per call of `$1092`) walks `data_len` bytes of the
    /// actual data against a second pointer, and any byte that differs colours the cell red (`$4100`) and
    /// sets `d4`; at `$1024` a clear `d4` counts a pass (`$FFFF10`) and a set one a fail (`$FFFF12`). The
    /// second pointer is the expected table when `has_expected` is set (`$0FDE..$0FE4`); otherwise it is the
    /// actual data itself (`$1092` sees `a0 == a1` and prints every cell neutral, `$0100`), so **a record
    /// with no expected table always counts as a pass**: it is shown, not judged. A `data_len` of zero or
    /// less skips the loop (`$0FEA`, `beq`/`bmi`) and also counts as a pass.
    fn rom_pass(&self) -> bool {
        !self.has_expected || self.data_len <= 0 || self.expected == self.actual
    }

    /// How many table words differ (0 for a record the ROM does not judge).
    fn wrong_words(&self) -> usize {
        if !self.has_expected {
            return 0;
        }
        self.expected_words()
            .iter()
            .zip(self.actual_words())
            .filter(|(e, a)| **e != *a)
            .count()
    }
}

/// Walk the result records `vdp_port_access` keeps from `$FF0000`, assigning each to the page the ROM shows
/// it on. Layout and paging are read from the ROM's display routine (`$0D8A..$0F9C`), which walks the same
/// list after every test:
///
/// * a record is word `rows` (how many screen rows it takes), word `title_len`, word `data_len`, the title
///   bytes, word `has_expected`, then `data_len` bytes of expected values copied from the ROM's own table
///   (when `has_expected` is non-zero) and `data_len` bytes of what the VDP answered;
/// * `$0000` ends the list; `$8000` (`$0E6E`) and `$FFFF` (`$0E50`, which also cancels the ROM's "run every
///   page" flag) each take one word and end the page;
/// * a page holds at most `$15` = 21 rows (`$0E16..$0E1C`): a record that would overflow it starts the next
///   page instead.
///
/// The existing M22 decoder skipped four bytes on `$FFFF` and called word 0 the test number; the ROM takes
/// two bytes there, and word 0 is the row count (the display routine adds it to `d6`). Neither mattered to
/// M22, because the only `$FFFF` in the list is its last word. Both are now read as the ROM reads them, and
/// [`port_access_run`] checks the page assignment against the tally the ROM prints at the end of every page.
fn port_access_records(sys: &System) -> Vec<PortAccessRecord> {
    let ram = sys.ram();
    let word = |o: usize| u16::from_be_bytes([ram[o], ram[o + 1]]);
    let mut out = Vec::new();
    let (mut o, mut page, mut rows_on_page) = (0usize, 1usize, 0u16);
    while o + 6 <= ram.len() {
        match word(o) {
            0x0000 => break,
            0x8000 | 0xFFFF => {
                o += 2;
                page += 1;
                rows_on_page = 0;
            }
            rows => {
                assert!(
                    rows <= 0x15,
                    "record at $FF{o:04X} claims {rows} rows; a page holds 21, so the layout is misread"
                );
                if rows_on_page + rows > 0x15 {
                    page += 1;
                    rows_on_page = 0;
                }
                rows_on_page += rows;
                let title_len = word(o + 2) as usize;
                let data_len = word(o + 4) as i16;
                let n = data_len.max(0) as usize;
                let title = String::from_utf8_lossy(&ram[o + 6..o + 6 + title_len])
                    .trim_end()
                    .to_string();
                let p = o + 6 + title_len;
                let has_expected = word(p) != 0;
                let body = p + 2;
                let (expected, actual) = if has_expected {
                    (
                        ram[body..body + n].to_vec(),
                        ram[body + n..body + 2 * n].to_vec(),
                    )
                } else {
                    (Vec::new(), ram[body..body + n].to_vec())
                };
                out.push(PortAccessRecord {
                    number: out.len() + 1,
                    page,
                    title,
                    has_expected,
                    data_len,
                    expected,
                    actual,
                });
                o = body + if has_expected { 2 * n } else { n };
            }
        }
    }
    out
}

/// The `Results: ( P/ F/ T)` tally the ROM prints at the end of every page, read off the screen.
fn port_access_printed_tally(sys: &System) -> Option<(u32, u32, u32)> {
    let row = text_rows(sys, 0x000)
        .iter()
        .map(|r| squeeze(r))
        .find(|r| r.starts_with("Results:"))?;
    let nums: Vec<u32> = row
        .trim_start_matches("Results:")
        .trim()
        .trim_matches(['(', ')'])
        .split('/')
        .map(|t| t.trim().parse().ok())
        .collect::<Option<Vec<_>>>()?;
    (nums.len() == 3).then(|| (nums[0], nums[1], nums[2]))
}

/// Whether the 68000 is parked in the ROM's own wait-for-a-button code: the end-of-page loop
/// (`$0F46..$0F62`) or the pad routines it calls (`$10E8..$1168`: wait for release, wait for a press,
/// debounce, wait for release). The ROM enters it only after printing a page's `Results:` line
/// (`$0E74..$0F1A`); no test calls it.
fn port_access_waiting(sys: &System) -> bool {
    let pc = sys.cpu_regs().pc;
    (0x0F46..0x0F66).contains(&pc) || (0x10E8..0x116A).contains(&pc)
}

/// One whole run of `vdp_port_access`, every page, shared by every test that reads it.
struct PortAccessRun {
    /// The ROM's printed tally at each page's end, in page order: cumulative (passed, failed, total).
    page_tallies: Vec<(u32, u32, u32)>,
    records: Vec<PortAccessRecord>,
    frames: u64,
}

/// Boot `vdp_port_access` and let it run all 22 pages, pressing `Start` each time the ROM parks in its
/// wait-for-a-button loop, which is its own "this page is done" signal ([`port_access_waiting`]). The
/// printed tally is captured at every one of those stops. Returns `None` (with `boot`'s `SKIP:` note) when
/// the ROM is not vendored.
///
/// Loud, never smaller: the ROM must reach [`PORT_ACCESS_TESTS`] within the frame bound, it must print a
/// tally whenever it waits, the decoder must find one record per test, and **the decoder's own page split
/// and verdicts must reproduce every page's printed tally**. That last check is what makes the decoded
/// records trustworthy: a misread layout, a wrong page boundary or a wrong verdict rule changes at least
/// one of 22 cumulative counts the ROM computed itself.
fn run_port_access() -> Option<PortAccessRun> {
    let mut sys = boot("vdp_port_access")?;
    const POLL: u64 = 4;
    // Measured need: 617 frames. The bound is 8x that, so a stall fails in seconds, not minutes.
    const BOUND: u64 = 5_000;
    let mut frames = 0u64;
    let mut page_tallies = Vec::new();
    loop {
        sys.run_frames(POLL);
        frames += POLL;
        if port_access_waiting(&sys) {
            let tally = port_access_printed_tally(&sys).unwrap_or_else(|| {
                panic!(
                    "vdp_port_access is waiting for a button (PC ${:06X}, frame {frames}) but prints no \
                     Results line: the paging model no longer describes this ROM",
                    sys.cpu_regs().pc
                )
            });
            page_tallies.push(tally);
            if tally.2 >= PORT_ACCESS_TESTS {
                break;
            }
            sys.set_pad(
                oracle_core::io::PadPort::P1,
                Pad {
                    start: true,
                    ..Default::default()
                },
            );
            sys.run_frames(5);
            sys.set_pad(oracle_core::io::PadPort::P1, Pad::default());
            frames += 5;
        }
        assert!(
            frames < BOUND,
            "vdp_port_access never reported {PORT_ACCESS_TESTS} tests: {} page stops recorded, the last \
             tally {:?}, after {frames} frames. The paging model no longer describes this ROM, so nothing \
             that reads this run would be measured",
            page_tallies.len(),
            page_tallies.last()
        );
    }
    let records = port_access_records(&sys);
    let &(passed, failed, total) = page_tallies.last().expect("at least one page");
    assert_eq!(
        total, PORT_ACCESS_TESTS,
        "the last page's tally is the whole ROM"
    );
    assert_eq!(records.len(), total as usize, "one result record per test");
    assert_eq!(passed + failed, total, "the ROM's own tally must add up");
    // Every page's printed cumulative tally, recomputed from the decoded records.
    let pages = records.last().map_or(0, |r| r.page);
    let recomputed: Vec<(u32, u32, u32)> = (1..=pages)
        .map(|pg| {
            let upto: Vec<&PortAccessRecord> = records.iter().filter(|r| r.page <= pg).collect();
            let p = upto.iter().filter(|r| r.rom_pass()).count() as u32;
            (p, upto.len() as u32 - p, upto.len() as u32)
        })
        .collect();
    assert_eq!(
        recomputed, page_tallies,
        "the decoder's page split and verdicts must reproduce the tally the ROM printed at the end of every \
         page, or it is reading the wrong bytes"
    );
    Some(PortAccessRun {
        page_tallies,
        records,
        frames,
    })
}

/// The one shared run. Every reader of this ROM's results goes through here, so the ROM boots once per
/// test binary however many tests read it (the scorecard row, the copy tables, the verdict pin).
fn port_access_run() -> Option<&'static PortAccessRun> {
    static RUN: std::sync::OnceLock<Option<PortAccessRun>> = std::sync::OnceLock::new();
    RUN.get_or_init(run_port_access).as_ref()
}

/// `VDPFIFOTesting`'s scorecard row: the ROM's own printed tally after page 1, after page 2, and after the
/// last page, read from the shared run.
fn scrape_vdp_port_access() -> Option<String> {
    let run = port_access_run()?;
    let t = |i: usize| {
        let (p, f, n) = run.page_tallies[i];
        format!("{p}/{f}/{n}")
    };
    Some(format!(
        "page1 pass/fail/total={}; pages1+2 cumulative={}; all {} pages cumulative={}",
        t(0),
        t(1),
        run.page_tallies.len(),
        t(run.page_tallies.len() - 1)
    ))
}

fn hex_words(w: &[u16]) -> String {
    w.iter()
        .map(|x| format!("{x:04x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// **VRAM copy DMA reads AND writes the opposite byte lane — pinned by the ROM's own hardware tables.**
///
/// `vdp_port_access` (VDPFIFOTesting) has **122** tests over 22 pages; the scorecard used to stop after
/// page 2 (16 tests), so it never reached the 28 that read back a VRAM copy's destination image. This
/// test reads them from the shared whole-ROM run ([`port_access_run`]) and compares, record by record,
/// what the ROM's expected-value tables say against what the VDP answered:
///
/// * **Test 26, "DMA Copy Length Reg Update"** (table at ROM `$9B24`): four copies from `$9000` to `$8000`
///   with autoinc 1, of `$0201`, `$0003`, `$0201` and `$0200` bytes, with the destination's tails read
///   back. Source bytes repeat `01 23 45 67 89 AB CD EF`. The 3-byte copy must read back `0123 0067`,
///   i.e. `$8002` untouched and `$8003 = $67`: step 2 read `$9003` and wrote `$8003`. The 513-byte copies
///   end `… 0023 0000`: step 512 read `$9201` and wrote `$8201`. All four records are asserted whole.
/// * **Tests 96-122, the copy matrix** (titles at ROM `$E0F2`, tables at ROM `$E4BE` + 32 × case): odd
///   and even sources and destinations, lengths 9 and 10, autoincrements 0/1/2/4, overlapping copies, and
///   every CD3-CD0 value. Each record is 16 words; the LAST EIGHT are the destination read back after the
///   copy (ROM `$EB82..$EBD0`), and those are asserted. The FIRST EIGHT are not about the copy. They are
///   VSRAM word 0 and CRAM word 0 read alternately (commands `$00000010` and `$00000020`), with CRAM writes
///   between the pairs (ROM `$EA2C..$EB6E`). **Corrected 2026-09-12:** M22 called them a post-copy read-path
///   behaviour. They differ only in the VSRAM halves, because the test's own 64-word VSRAM load (`$E87C`)
///   hits our 80-byte VSRAM wrap and overwrites word 0. That is cause A1 in
///   `docs/2026-09-12-vdp-port-access-full-rom.md`, and [`vdp_port_access_full_rom_verdicts`] pins it, so it
///   is not asserted here. **Fixed 2026-09-12 (VSRAM-DECODE):** with the 7-bit decode the load no longer
///   reaches word 0, and all 27 records now pass whole under the ROM's own verdict (the pin above).
///
/// What would make this green for a reason other than the rule holding: (1) records not found (a paging
/// or layout change) — guarded by the exact title set and the count of 27; (2) an `expected` that is
/// really the `actual` read twice — guarded because the decoder's verdicts must reproduce every page tally
/// the ROM prints itself ([`run_port_access`]); (3) the aligned controls (tests 100 and 107) pass under
/// every model — which is why the odd cases are asserted too, and they fail with the read half or the write
/// half removed (mutation record in the F-COPYXOR entry).
#[test]
fn vdp_port_access_copy_dma_matches_the_roms_own_tables() {
    let Some(run) = port_access_run() else {
        return; // SKIP printed by `boot`; under CI `vendor_data_present_when_running_in_ci` fails instead
    };
    let recs = &run.records;

    // Every mismatch is collected first and reported together, so one red shows test 26 AND the matrix.
    let mut wrong: Vec<String> = Vec::new();

    let len_reg = recs
        .iter()
        .find(|r| r.title == "DMA Copy Length Reg Update")
        .expect("test 26 'DMA Copy Length Reg Update' record");
    let (exp, act) = (len_reg.expected_words(), len_reg.actual_words());
    assert_eq!(exp.len(), 16, "test 26 carries a 16-word table");
    if act != exp {
        // Words 0-3: $81FC..$8203 after the $201-byte copy (words 2-3 are $8200..$8203, where the last
        // step lands). Words 4-7: $8000..$8007 after the 3-byte copy. Words 8-15: the same two probes after
        // the second $201-byte copy and the $200-byte copy.
        wrong.push(format!(
            "test 26 'DMA Copy Length Reg Update' (ROM table $9B24): want {} got {}",
            hex_words(&exp),
            hex_words(&act)
        ));
    }

    let matrix: Vec<&PortAccessRecord> = recs
        .iter()
        .filter(|r| {
            [
                "DMA Copy 9000 to 8",
                "DMA Copy 9001 to 8",
                "DMA Copy 8000 to 8",
                "DMA Copy 8001 to 8",
            ]
            .iter()
            .any(|p| r.title.starts_with(p))
        })
        .collect();
    assert_eq!(matrix.len(), 27, "the copy matrix is tests 96-122");
    for r in &matrix {
        let (exp, act) = (r.expected_words(), r.actual_words());
        assert_eq!(exp.len(), 16, "{}: a 16-word table", r.title);
        if act[8..] != exp[8..] {
            wrong.push(format!(
                "{} (destination image, ROM $E4BE table): want {} got {}",
                r.title,
                hex_words(&exp[8..]),
                hex_words(&act[8..])
            ));
        }
    }

    assert!(
        wrong.is_empty(),
        "{} VRAM-copy image(s) differ from VDPFIFOTesting's hardware tables. The rule they pin: step i reads \
         the source at `(source + i) ^ 1` and writes the destination at `(dest + i*inc) ^ 1`.\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// **Today's failing set for the whole of `vdp_port_access`, by test number and title.** This is a record
/// of today. It is **not** a list of accepted failures.
///
/// Each entry is `(test number, title, words that differ from the ROM's table, words in the table)`. It is
/// derived from the ROM's own verdicts ([`PortAccessRecord::rom_pass`], the display routine's byte compare),
/// and [`run_port_access`] checks those verdicts against the tally the ROM prints at the end of each of its
/// 22 pages. Nothing here was typed from an inventory: on a mismatch the test prints today's list in this
/// exact form.
///
/// The owner's standing rule governs how to read it: **timing unknowns may be deferred; behaviour unknowns
/// get pinned from a reference and fixed, never waved through as expected failures.** Every entry below is
/// sorted in `docs/2026-09-12-vdp-port-access-full-rom.md` into one of six causes. Five are behaviour (A)
/// causes, and each of those is a queue row to fix. The sixth is one unmodelled mechanism (M):
///
/// * **A1, VSRAM address decode.** The address is 7 bits (it wraps at `$80`), writes to `$50-$7F` are
///   discarded, and reads from `$50-$7F` return the VSRAM read latch. We wrapped at 80 bytes instead
///   (`% VSRAM_SIZE`), so a 64-word VSRAM load overwrote words 0-23. Tests 23, 74-95 (the eight VSRAM
///   fills), 96-122 (the copy matrix's first eight words are VSRAM reads of word 0), and two words of 20.
///   **Fixed 2026-09-12 (VSRAM-DECODE)**: `Vdp::vsram_byte` and the VSRAM read latch; 76/46 → 112/10.
/// * **A2, the 68k-to-VDP DMA source wraps inside its 128 KB page** (register 23 never takes a carry). Tests
///   27 and the other half of 20. **Fixed 2026-09-12 (DMA-SRC-128K)**: both pass and are no longer listed;
///   20 is a joint flip, since its other two words per half needed A1.
/// * **A3, fill and copy advance the DMA source registers 21/22 by their length.** Tests 28 and 29.
///   **Fixed 2026-09-12 (DMA-SRC-ADVANCE)**: both pass and are no longer listed.
/// * **A4, DMA busy reads set from the fill command's control write**, not only once the fill is
///   triggered. Tests 36 and 38. **Fixed 2026-09-12 (FILL-BUSY-ARM)**: `Vdp::dma_busy` is now "a fill is
///   armed OR the transfer window is open"; both pass and are no longer listed.
/// * **A5, a fill whose code names no write target writes nothing** (it closes follow-up F-FILLTGT). Test 34.
///   **Fixed 2026-09-12 (FILL-TGT)**: the fill body (`Vdp::run_fill` then; `Vdp::fill_step` since M1) shares `code_names_a_write_target` with the two
///   port-write paths; the fill still walks its address, consumes its length and advances registers 21/22.
///   It passes and is no longer listed.
/// * **M1, a fill that runs over time.** Ours completed inside its trigger write, so a data-port write made
///   during a running fill never changed the fill byte. Tests 31, 32 and 33.
///   **Fixed 2026-09-15 (M1 FILL-RUN, parcel P1)**: a fill is a process the VDP advances on the one
///   external-slot clock it shares with the FIFO, and each step reads its data and write target out of the
///   FIFO ring rather than from a copy of the trigger word, so a mid-fill data-port write changes what the
///   remaining steps write — and consumes no length count. All three pass and the set below is **empty**;
///   all 22 pages 119/3/122 → **122/0/122**, pages 1 and 2 unchanged at 9/0/9 and 16/0/16. Design:
///   `docs/2026-09-14-m1-fill-over-time-design.md` (§1.3 derives the three tails by hand from the ROM's
///   own code, before the model was built).
///
/// **The list is empty, and that is a claim, not an absence.** Every one of VDPFIFOTesting's 122 tests
/// passes. A flip in either direction fails naming the test that moved — an empty pin makes the "NOW FAILS"
/// arm the only one that can fire, which is exactly the guard a green suite wants. So does a failing test
/// whose wrong-word count moves. Update this list only together with that document and
/// `docs/2026-07-25-testrom-conformance.md`.
const PORT_ACCESS_FAILING: &[(usize, &str, usize, usize)] = &[];

/// **The whole of VDPFIFOTesting, pinned test by test from the ROM's own verdicts.** See
/// [`PORT_ACCESS_FAILING`] for what the pinned set means and how to read it: it records today and accepts
/// nothing.
///
/// What would make this green for a reason other than the pin holding, and what rules each out:
/// (1) **fewer tests read** (a paging or layout change): [`run_port_access`] fails unless the ROM's printed
/// tally reaches 122 and the decoder finds one record per test. (2) **A verdict rule that is wrong but
/// agrees with the pin** (for example, an `expected` that is really the `actual` read twice): the decoder's
/// page split and verdicts must reproduce all 22 cumulative tallies the ROM printed itself. (3) **A missing
/// ROM**: the test prints `SKIP` and returns. Under CI `vendor_data_present_when_running_in_ci` fails
/// instead, and locally the `vdp_port_access: 122 records` line below is the proof that the run happened.
#[test]
fn vdp_port_access_full_rom_verdicts() {
    let Some(run) = port_access_run() else {
        return; // SKIP printed by `boot`; under CI `vendor_data_present_when_running_in_ci` fails instead
    };
    let &(passed, failed, total) = run.page_tallies.last().expect("a tally");
    eprintln!(
        "vdp_port_access: {} records, {} pages, {} frames; the ROM prints {passed}/{failed}/{total}",
        run.records.len(),
        run.page_tallies.len(),
        run.frames,
    );
    // The inventory, one line per test (`--nocapture` shows it). The tables in
    // docs/2026-09-12-vdp-port-access-full-rom.md are re-derivable from these lines.
    for r in &run.records {
        eprintln!(
            "REC\t{}\t{}\t{}\t{}\t{}\t{}/{}\tE={}\tA={}",
            r.number,
            r.page,
            r.title,
            if r.rom_pass() { "PASS" } else { "FAIL" },
            r.data_len,
            r.wrong_words(),
            r.expected_words().len(),
            hex_words(&r.expected_words()),
            hex_words(&r.actual_words())
        );
    }

    let got: Vec<(usize, &str, usize, usize)> = run
        .records
        .iter()
        .filter(|r| !r.rom_pass())
        .map(|r| {
            (
                r.number,
                r.title.as_str(),
                r.wrong_words(),
                r.expected_words().len(),
            )
        })
        .collect();
    assert_eq!(
        got.len(),
        failed as usize,
        "the failing set must be exactly the failures the ROM printed"
    );
    if got == PORT_ACCESS_FAILING {
        return;
    }

    // Name every test that moved, in both directions, before printing today's list in paste-ready form.
    let mut moved: Vec<String> = Vec::new();
    for g in &got {
        match PORT_ACCESS_FAILING.iter().find(|w| w.0 == g.0) {
            None => moved.push(format!(
                "test {} '{}' NOW FAILS ({}/{} words off the ROM's table)",
                g.0, g.1, g.2, g.3
            )),
            Some(w) if *w != *g => moved.push(format!(
                "test {} '{}' still fails, but {}/{} words are off the ROM's table (pinned: '{}', {}/{})",
                g.0, g.1, g.2, g.3, w.1, w.2, w.3
            )),
            Some(_) => {}
        }
    }
    for w in PORT_ACCESS_FAILING {
        if !got.iter().any(|g| g.0 == w.0) {
            moved.push(format!("test {} '{}' NOW PASSES", w.0, w.1));
        }
    }
    let today: Vec<String> = got
        .iter()
        .map(|g| format!("    ({}, {:?}, {}, {}),", g.0, g.1, g.2, g.3))
        .collect();
    panic!(
        "vdp_port_access's per-test verdicts moved (the ROM prints {passed}/{failed}/{total}):\n  {}\n\
         A flip is a fix or a regression. Confirm which, then update PORT_ACCESS_FAILING together with \
         docs/2026-09-12-vdp-port-access-full-rom.md. Today's list:\n{}",
        moved.join("\n  "),
        today.join("\n")
    );
}
