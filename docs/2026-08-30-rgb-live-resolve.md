# `pixel_attribution.rgb` — aeon's finding is real, the fix is FORBIDDEN, and the defect is discoverability

**Date:** 2026-08-30 · **By:** oracle overseer, foreground · **Outcome:** no server change; an anti-fix pin,
a doc note, and a CR to draft

## 0. The short version

aeon reported that `emulator/pixel_attribution` returns the right `cramIndex` and the **wrong `rgb`** on a
ROM that repaints CRAM mid-frame. **It reproduces exactly.** I built the obvious fix, verified it against
the reproduction, and then found the contract forbids it **in terms**. The fix is reverted. What survives
is a pin that stops the next person doing what I did, a doc note aeon asked for, and a CR for the half that
is genuinely broken.

**The behaviour is correct-by-contract. The defect is that nothing tells a caller so.**

## 1. Reproduced, by a route neither of us shared

aeon: a 71,680-pixel sweep on `s4.debug.bin` (their raster band demo, frame 186), cross-referenced against
`emulator/screenshot`'s PNG. 702 in-band pixels reported the base colour while the framebuffer held the
three authored band colours.

Here, before touching anything: `vendor/TestRoms/color_1536.bin`, 90 frames, paused, comparing
`pixel_attribution` against **`emulator/scanlines`** rather than a PNG.

```
checked 55 rows at x=100 -> MISMATCHES: 55
  y=  4 attribution=(0, 72, 0)  raster=(0, 54, 0)  cramIndex=0
  …
```

Same `cramIndex`, different colour, every row. **Different ROM, different instrument, different lane** — so
this is corroboration rather than echo (bar 19: the enumeration parameters genuinely differ).

I also confirmed the mechanism from our source independently of both measurements — `render.rs:1897`,
`rgb: self.cram_rgb_state(winner.cram_index, winner.state)`, where `cram_index` comes from the resolved
line and the colour comes from **live** CRAM.

## 2. The fix I built, and why it is gone

`Engine::latched_raster_pixel` — take the pixel from `last_frame` (the raster the machine actually drew,
the same source `screenshot` and `scanlines` serve) when no layer mask is set. Verified against the
reproduction: **220 dots across 55 rows × 4 columns, 0 mismatches**, up from 55-of-55 wrong.

It is reverted, because `contract/protocol.md` §11.3 says, of this method:

> *"A server answers by resolving the scanline from live VDP state — VRAM, CRAM, the registers, the sprite
> table — and **MUST NOT read a framebuffer**."*

and, about this exact disagreement:

> *"a whole-frame-state read disagrees with the picture **whether the machine is running or not** … **This
> is not a defect in either method and a server MUST NOT try to paper over it**; a client that needs the two
> to agree needs a per-scanline capability — `emulator/scanlines`."*

and names it as already booked: *"closing that divergence is a registered follow-up
(**F-SCANLINE-INDEX**), not a defect in either method."*

**F-SCANLINE-INDEX is in this repo's own follow-up register**, and this seat read it at boot the same day.

⚠ **How close this came to shipping, recorded because that is the useful part.** The fix compiled, the
reproduction went green, the full suite passed, and I had already told aeon my "current lean" was to do
exactly this. Nothing in the code, the tests, or the reproduction would have stopped it. **The only thing
that stopped it was reading the method's own contract text before merging** — and I only did that because I
went looking for whether `caveat` was available on the fragment, i.e. for an unrelated reason. That is
bar 8's cheap frame-changer arriving by luck rather than by discipline, which per bar 21 makes it a
coincidence with a good track record, not a practice.

**And the near-miss is bar 9 exactly, from the inside:** the instrument (attribution) could not reach the
subject (what was drawn), and I reconfigured the subject until it could. It produced better-looking data —
220 agreeing dots — and *nothing about it announced itself*. It is the textbook shape and I was in it.

## 3. What is actually wrong, and it is aeon's own framing

The contract's answer to "I need the colour that was drawn" is **`emulator/scanlines`** — which is,
independently, exactly the workaround aeon arrived at (`cramIndex` from attribution, colour from the
raster). So the mechanism works. What failed is that **nothing in the reply says any of this.**

aeon's cost was not the wrong value. In their words: *"my run had `cramIndex` right and `rgb` wrong in the
same object, and there was no way to tell from the object alone. A caller with one instrument cannot detect
disagreement between two fields that never disagree out loud."*

**That is the defect, it is real, and it is contract surface rather than server behaviour**: this fragment
declares no `caveat` (19 of 64 do), so a server cannot say it. §8's invention ban means we do not add one
unilaterally.

**CR to draft — `pixel_attribution` gains a `caveat`**, emitted when the divergence is *possible* (a
completed frame exists to disagree with), naming `emulator/scanlines` as the reconciliation path. Two
properties worth writing as properties:

1. **It must not be a heuristic that claims to detect mid-frame CRAM writes.** A flag that fires only when
   we think a raster program ran will be wrong in both directions; the honest statement is about which
   moment this row answers for, which is always true.
2. **It must name the path, not just the hazard.** aeon had the workaround already; a caller who does not
   needs to be sent somewhere, or the caveat costs an hour and saves none.

## 4. What landed

* **The anti-fix pin** — `rgb_resolves_against_live_state_and_the_row_must_not_read_a_framebuffer`
  (`tests/pixel_attribution.rs`). Asserts `rgb` **follows live CRAM** after a repaint, and says in its own
  body why a green-by-latched-raster is the defect. **Recorded mutation:** apply the framebuffer read →
  the row fails naming §11.3. It exists because this seat tried the fix and got a green suite.
* **The truncation note aeon asked for** (`render.rs::intensity`). `step * 255 / 14` truncates, so `$0224`
  is `(72, 36, 36)` and not the `(73, 36, 36)` a rounding formula gives. Comparing our output against a
  `round()`-based reference scores **correct** pixels as mismatches by one unit — small enough to read as
  noise, large enough to fail an equality check. No behaviour change; aeon explicitly did not ask for one.

## 5. Owed

* Tell aeon: their report is right, their workaround is the contract's own prescribed path, and the fix
  they ranked first is forbidden — so the thing they ranked *second* (name the moment) is what gets built.
* Draft the caveat CR.
* **Do not re-attempt the framebuffer fix.** If a future session believes attribution should agree with the
  picture, the contract change is the work, not the server change — and F-SCANLINE-INDEX is where it lives.
