# Per-scanline golden coverage — closing the post-hoc-render blind spot

**Date:** 2026-08-15
**Change:** additive test coverage only (`crates/oracle-core/tests/scanline_goldens.rs`). **No existing pinned
literal moved** — not in `conformance_roms.rs`, not in `golden_frames.rs`, not in `determinism_gate` or
`export_state_v1`. No file under `crates/oracle-core/src/` was touched.

---

## 1. The blind spot

The machine renders every scanline live during the run (`System::run` → `Vdp::render_scanline`) and **discards
the RGB** unless a sink opts in via `BusEventSink::wants_scanlines`. A separate pure path,
`Vdp::render_line`, re-renders a line *after the fact* against whatever state the VDP currently holds.

Every frozen golden sampled the post-hoc path. The frontend had the same bug and it bit for real — Sonic 3 &
Knuckles' underwater palette split drew in the above-water palette — and was fixed on 2026-08-14 (`aba49a3`) by
blitting a `ScanlineCapture`. The goldens were never converted, so **the pinned hashes remained blind to
exactly the class of effect the window had been blind to.**

The `color_1536` row (Limitation L1, 2026-08-03) had already been converted, but it was treated as *one ROM's*
quirk rather than as a general property of the instrument.

## 2. What the survey found

Every ROM in `vendor/TestRoms` (17 of 17) was booted at the harness's seed (`0x1234_5678`) and run 120 frames.
The last complete frame was hashed twice from the *same* run: once from the live per-scanline capture, once
from the post-hoc re-render, in an identical FNV-1a byte layout.

**Six ROMs discriminate; eleven do not.**

| ROM | live vs post-hoc | lines differing | measured cause |
|---|---|---|---|
| `color_1536` | **DIFFERS** | 224/224 | mech. 1 — 645 VDP-port writes per frame during active display (387 register, rest CRAM), from line 48. Live picture ~1400 colours; post-hoc 4. **2026-08-19:** verdict unchanged, hash re-pinned to `0x9ae4acc58d2a382d` — the live rows are now segmented at each CRAM landing (515 value-changing in-active-window writes per hashed frame, indices 4–7, active lines 48–221), so the picture is correct *within* a row as well as between rows. |
| `shadow_highlight` | **DIFFERS** | 224/224 | mech. 3 — **zero** active-display writes, 18 vblank writes per frame; 4-frame animation cycle. |
| `window_distortion` | **DIFFERS** | 112/224 | mech. 1 — **exactly one** active-display write per frame: R17 (window H position) → `$00` at line 111, restored to `$07` in vblank. |
| `io_sample` | **DIFFERS** | 14/224 | mech. 1 — 328 active-display VRAM writes per frame; the ROM redraws its nametable behind the beam. |
| `vdp_sprite_masking` | **DIFFERS** | 8/224 | mech. 2 — zero writes of any kind; stale `sprite_dot_overflow_carry`. |
| `m68k_opcode_sizes` | **DIFFERS** | 6/224 | mech. 1 — ~34 active-display VRAM writes per frame as the ROM plots its decode map. |
| `cram_flicker` | identical | 0 | screen is blank; the effect it demonstrates is sub-scanline (see L1a). |
| `direct_color_dma` | identical | 0 | whole frame's CRAM DMA lands in one inter-line window (see L1b). |
| `fm_test`, `gfx_joystick`, `m68k_bcd`, `m68k_illegal`, `m68k_memory_test`, `vcounter`, `vdp_port_access`, `vdp_test_register`, `window_test` | identical | 0 | static screens; nothing mid-frame, no vblank animation, no sprite carry. |

A golden that is byte-identical to the post-hoc one adds amendment cost without coverage, so **the eleven
identical rows pin no hash literal.** They record the *equality* instead, which is still machine-checked: a ROM
that starts diverging shows up as a diff rather than as nothing.

### The three mechanisms

The survey turned up three distinct ways the post-hoc render shows a picture that was never on screen. Only
the first was anticipated:

1. **Mid-frame state changes.** Raster splits, per-line palette cycling, water lines. The post-hoc render sees
   only the *final* value of every register and CRAM word.
2. **Per-line carried sprite state.** `render_scanline` seeds the R10 sprite-masking rule from
   `sprite_dot_overflow_carry` and commits the new carry for the next line. `render_line` re-seeds from
   whatever the carry holds *now*. `report_rgb`'s own doc comment already says re-resolving after
   `render_scanline` "would be wrong as well as wasteful: the committed dot-overflow carry would reseed the R10
   masking and could change the sprites" — but nothing had ever measured how far apart the two paths were.
3. **Vblank updates that postdate the frame.** A ROM that touches the VDP only *between* the last active line
   and the next frame leaves the post-hoc render describing state one update newer than the frame it claims to
   describe. Nothing mid-frame, no sprite state — the re-render is simply of the wrong moment.

Mechanism 3 was not on anyone's list, and it is the broadest of the three: it needs no raster trick at all,
only a ROM that does its VDP work in vblank, which is *most* ROMs. It does not bite the eleven identical rows
only because their screens are static.

**Method note (a correction to my own first draft).** `shadow_highlight` was initially attributed to mechanism
2. That was wrong, and the control that caught it was: replay the *stateful* path (`render_scanline`) over the
settled machine in line order and see whether it reproduces the live frame. For `vdp_sprite_masking` it
reproduces the live hash **exactly**; for `shadow_highlight` it reproduces the **post-hoc** hash, and the carry
reads `false` after every frame. Splitting bus writes into active-display vs vblank phases then showed
`shadow_highlight`'s 18 writes per frame all land in vblank. The Fable adjudicator flagged the assumption
before the measurement did — "one latch can't obviously repaint every line".

## 3. The finding worth the owner's attention

**`vdp_sprite_masking` test 6 reads `FAIL` through the post-hoc path and `PASS` through the live path.**

The harness classifies that ROM's nine verdict glyphs by hashing rendered pixels (ticks and crosses have
identical nametable cells, so only the framebuffer discriminates). It renders them with `block_hash`, which
calls the post-hoc `render_line`. Classifying all nine glyphs under both paths, at the harness's own stop
condition:

```
test 1: post-hoc=TICK/TICK    live=TICK/TICK
test 2: post-hoc=TICK/TICK    live=TICK/TICK
test 3: post-hoc=TICK/CROSS   live=TICK/CROSS
test 4: post-hoc=PASS         live=PASS
test 5: post-hoc=PASS         live=PASS
test 6: post-hoc=FAIL         live=PASS      <-- DIFFERS
test 7: post-hoc=PASS         live=PASS
test 8: post-hoc=PASS         live=PASS
test 9: post-hoc=TICK/TICK    live=TICK/TICK
```

Eight of nine agree. The one that disagrees is test 6, **MASK S1 ON DOT OVERFLOW** — precisely the behaviour
governed by the carry, and its 8 divergent lines (88-95, x ≥ 216) are exactly the rectangle `block_hash` reads
for row 11. The ROM makes **zero** VDP accesses after frame 7, so no mid-frame effect is involved.

### The headline is the mis-attribution, not the string

`docs/2026-07-25-testrom-conformance.md` (the `vdp_sprite_masking` row) records **two** failures and credits
**both** to the mid-sprite pixel-budget cut interim model — ledger row **P1** in
`docs/2026-07-16-vdp-pixel-known-differences.md`. On this evidence P1 owns **at most one** of them (test 3's
second sub-case). Test 6 appears to be a measurement artefact of the instrument, not an emulator inaccuracy.

That reframes the priority of the P1 follow-up work itself, which is worth more than the row flip.

### Why nothing was changed

The scorecard row is **not** amended, and neither is the P1 ledger row. Reasons:

* The assignment explicitly reserved "should the post-hoc goldens be retired?" for the owner and said to
  escalate rather than expand scope. Switching that scraper *is* the reserved decision.
* The conformance harness is documented as **non-gating** — "a diff is information, not automatically a failure
  to fix" — so a wrong pin blocks nothing and breaks no build. The cost of waiting is ≈ zero.
* Option "flip the string" is not the one-line change it looks like. The four glyph constants
  (`TICK_TICK`/`TICK_CROSS`/`PASS`/`FAIL`) are themselves literals pinned *from post-hoc pixels*, and
  `block_hash` re-renders from settled state at any stop point — which is why the idle-stop conversion could
  say "stopping mid-frame is irrelevant to it". A live-path scrape needs a frame-aligned capture and a decision
  about *which* frame. That is an instrument redesign, done overnight, on a decision the owner already claimed.
* **The additive coverage already records the truth.** `scanline_goldens.rs` pins `vdp_sprite_masking`'s live
  frame as new, separate currency. The repository now holds both readings side by side with the explanation of
  why they differ — strictly better evidence for the owner than a flipped row, because the disagreement itself
  is preserved as an exhibit.

The real hazard of leaving it — a future session "fixing" the emulator until the *post-hoc* render says PASS,
i.e. breaking correct live carry-seeding to satisfy a broken instrument — is defused by documentation at the
point of use. A comment now sits on the `BASELINE` row itself.

### Registered follow-up

**F-POSTHOC-STALE-CARRY** — see the ledger's named-follow-ups section.

## 4. Non-vacuity

This project has been burned by an assertion that passed with zero enqueues, so each guard was demonstrated,
not asserted.

**Structural guard — a `LIVE-DIFFERS` row cannot pass if the live path collapses to post-hoc.** Each such row
records, as its own text, that the live hash is *not* the post-hoc hash. Verified by experiment: replacing
`live_frame_hash`'s body with the post-hoc computation flips **all six** pinned rows to
`IDENTICAL-TO-POST-HOC` and fails the scorecard —

```
color_1536 … window_test:  all 17 rows -> IDENTICAL-TO-POST-HOC
test result: FAILED. 3 passed; 1 failed
```

**Pixel sensitivity — each pinned hash depends on the pixels, not just on the differ/identical flag.**
Verified by experiment: XOR-ing one bit of one pixel (line 111, x 200) changes **every** pinned hash and fails
the scorecard —

| ROM | pinned | with one bit flipped |
|---|---|---|
| `color_1536` | `0x917371f07409cb25` † | `0x60cc41b2e871b204` |
| `io_sample` | `0xe5e133a2b8f9fe93` | `0x26dbf0aacccf8e6a` |
| `m68k_opcode_sizes` | `0xfb9783a5ab564eb4` | `0x9e09d3469fa45e55` |
| `shadow_highlight` | `0xfd6f02e7574d67f5` | `0xc5b0baf26cb2fd40` |
| `vdp_sprite_masking` | `0xce1c5a0559088d5d` | `0x0a68e5a2f17d748c` |
| `window_distortion` | `0xdf5bae342cc03667` | `0x13720bb63ffcf066` |

That experiment is retained permanently, in-tree, as `the_live_hash_depends_on_the_pixels`.

† **`color_1536` was re-pinned to `0x9ae4acc58d2a382d` on 2026-08-19** (`F-SCANLINE-SUBLINE` slice 4 — CRAM
landings are now resolved to a pixel *inside* the row, so the row shows the palette evolving across its own
width). The pair above is the **2026-08-15 measurement against the then-current pin** and is kept as the
historical record; the flipped-bit column has deliberately **not** been re-derived, because inventing a
number nobody measured is exactly what this table exists to prevent. The property the table demonstrates —
that a pinned hash depends on the pixels — is unchanged, and is the one still enforced in-tree by
`the_live_hash_depends_on_the_pixels` (which perturbs `window_distortion`, whose pin did not move).

**Capture-shape guard.** `live_frame_hash` asserts the capture handed back exactly one complete frame of active
lines and that the run delivered every frame boundary. This is not a hypothetical failure mode: an early draft
of the survey attached a capture *after* a mid-frame stop and silently hashed a fragment, producing four
"identical" glyph hashes that were pure garbage. The assert is what makes that loud.

**Artifact guard.** `scanline_golden_scorecard` requires every ROM present on disk to have produced a row, and
`vendor_data_present_when_running_in_ci` fails under `CI` if the corpus is missing — so a fetch failure cannot
turn into a vacuous green.

**Coverage guard.** `the_baseline_actually_pins_live_coverage` requires at least six `LIVE-DIFFERS` rows, so the
file cannot silently decay into pure cost.

**Independent cross-check** (values as of 2026-08-15; both literals moved together to `0x9ae4acc58d2a382d`
on 2026-08-19 and the cross-check was preserved, which is the whole point of it). `color_1536`'s live hash
computed here — `0x917371f07409cb25` — equals the value
`conformance_roms.rs` independently pins for that ROM through a separate code path. The byte layout and run
shape of the new instrument agree with the existing one.

## 5. Gates

| gate | result |
|---|---|
| `cargo test --workspace` at `b51dcb9` (baseline) | 1247 passed / 0 failed / 28 legs |
| `cargo test --workspace` with this change | 1252 passed / 0 failed / 29 legs (+5 tests, +1 leg) |
| `cargo clippy --all-targets --workspace` | zero warnings |
| `cargo fmt --all --check` | clean |
| existing pinned literals | unchanged — `git diff` on `conformance_roms.rs` is comment-only; `golden_frames.rs` untouched |

## 6. Open for the owner

1. **Should the post-hoc goldens be retired?** Deliberately left open. The two instruments now sit side by
   side, so the decision can be made with evidence rather than under time pressure. Note that retiring them is
   not free: the eleven `IDENTICAL-TO-POST-HOC` rows would lose their (cheap) equality check, and
   `golden_frames.rs` has no live path at all — its scenes are static `Vdp` fixtures with no machine to run, so
   per-scanline capture is not merely unimplemented there but not meaningful.
2. **F-POSTHOC-STALE-CARRY** — whether to switch `vdp_sprite_masking`'s scraper to the live path and amend the
   row to `6=PASS`, and whether the P1 ledger row should be corrected to claim one failure instead of two.
3. **Should `frame_hash` grow a vblank-phase caveat?** Mechanism 3 means *any* end-of-frame hash of a ROM that
   updates in vblank describes a moment one update later than the frame. Six rows of the existing scorecard are
   plain `frame_hash` visual baselines; `shadow_highlight` is now known to be one of the affected ones.
