# Toy Story's perspective floor, measured on the live machine

**For aeon, as the reference for the perspective floor.** Read from the owner's own running window at two
camera positions on 2026-09-05, entirely through pure reads: no pause, no write, no resume. His game was
frame 7019 before and after every read at position 1, and 7186 at position 2, `running: false` throughout.

## The answer to his question, in one sentence

**The convergence is drawn into fixed artwork and the whole plane scrolls, so the vanishing point travels
with the world; it is not re-anchored to the screen as the camera moves.**

He asked, of his own drawing: *"see how the left points to the left corner, right to right corner, middle
to center, but if you keep moving it continuously does that?"* Measured answer: **no, not continuously.**
That alignment is a property of where the camera happens to be, because the fan is painted into the floor
art and slides with it.

## What was measured, and how, since no VDP register is readable

This server serves no VDP register readback (see the ask at the end), so nothing below reads a register.
Every number is a measurement of observable behaviour or a byte read out of VRAM.

### 1. The fan is NOT per-line horizontal scroll

A line's horizontal scroll fixes where the 8 pixel cell grid falls on screen, and that is directly
observable: the screen `x` at which the winning plane B tile changes. A smooth perspective fan needs
**sub-cell** scroll differences between adjacent lines, so its phase would drift continuously down the
screen.

| position | frame | lines measured | distinct phases |
|---|---|---|---|
| 1 | 7019 | 48 (y=96 to 223) | **{4}** |
| 2 | 7186 | 20 | **{0}** |

**Constant down the screen at both positions, and shifted as a whole between them.** That is the signature
of a plane scrolled uniformly, not fanned per line.

### 2. The fan is in the art, and the art is unique per cell

Plane B tiles across one floor row are **consecutive**: at y=210, `965, 966, 967 … 978`. Every cell has its
own tile. There is no repeating pattern to wrap, which answers *"how does it avoid a visible wrap"*: the
floor is effectively a bitmap, so there is nothing to wrap.

Per floor cell-row, from the nametable:

| screen y | first tile indices | nametable row |
|---|---|---|
| 180 | 813, 814, 815, 816, 817 | `0xCD00` |
| 190 | 851, 852, 853, 854, 855 | `0xCD80` |
| 200 | 927, 928, 929, 930, 931 | `0xCE80` |
| 210 | 965, 966, 967, 968, 969 | `0xCF00` |
| 220 | 1003, 1004, 1005, 1006, 1007 | `0xCF80` |

**38 unique tiles per cell-row** (813 → 851 → 889 → 927 → 965 → 1003, a stride of 38), which is about one
screen width of unique art per row. Rows sit **128 bytes apart**, so the plane is **64 cells = 512 pixels
wide**, derived from row spacing rather than from register $10.

### 3. Static art, moving plane: the decisive comparison

Between the two camera positions, byte for byte over the whole of VRAM:

| region | bytes differing |
|---|---|
| floor tile art (tiles 813 to 1041) | **0** |
| plane B nametable (`0xC000` to `0xD000`) | **0** |
| everything below the floor art | 674 (sprite animation) |

**The floor's art and its map did not change by a single byte.** What changed is the scroll: the leftmost
visible plane B cell on row y=210 moved from plane column 3 to column 15, so the plane scrolled **12 cells,
96 pixels**, and the phase change of 4 puts the true figure at 92 or 100 pixels (the sub-cell part is not
separable without the scroll value itself).

So the game did **not** redraw the fan for the new camera position. If it had, the vanishing point would
stay under the screen centre; it does not.

### 4. Vertical scroll

VSRAM reads `0x0320, 0x0320, 0x0000, 0x0000 …`: only the first two entries are used, both **800**, and every
entry after is zero. **Whole-plane vertical scroll, both planes together. Not column vscroll.**

## ⚑ A candidate that looked right and was wrong, recorded because it nearly shipped

Searching VRAM by structure for a per-line scroll table found `0xCC00`: constant for the top 64 lines, then
a 32 line repeating sawtooth of exactly 2 per line on a baseline rising 38 every 32 lines. It looks exactly
like a floor table, and it would have made a convincing curve.

**It is the nametable.** The "smooth ramp" is consecutive tile indices, and the "period 38" is the 38 tiles
per cell-row above. It was rejected because it predicts a cell phase that varies line to line (5, 5, 5, 1 on
the lines tested) and the measured phase is a flat 4.

The structural search alone would have produced a fabricated curve that fitted the data it was derived from.
**The phase test is what refuted it, and it cost four probes.**

## What this means for building one

1. **No raster effect is required.** No per-line scroll, no line interrupt, no mid-frame register write.
2. **The cost is VRAM and authoring, not CPU.** 38 unique tiles per cell-row across five or more rows is
   roughly 190+ tiles of floor, and it must be streamed as the camera moves, since there is no repeat.
3. **The perspective is baked at author time**, so the vanishing point is a property of the artwork. Moving
   it means redrawing, which is exactly why it does not follow the camera here.
4. **A screen-anchored vanishing point is a different technique from this one.** If that is what is wanted,
   this ROM is not the reference for it.

## What could not be read, and the ask it generates

**No VDP register readback is served.** Every base and mode here was derived by structure or by observation:
the nametable base and plane width from row spacing and a unique tile-sequence match, the absence of a
per-line fan from cell phase, the vertical scroll mode from VSRAM's own shape. That worked, and it cost
several probes and one rejected candidate that a register read would have settled in one call.

Registers this pass wanted and could not read: **$04** (plane B nametable base), **$0D** (horizontal scroll
table base), **$10** (plane size), **$0B** (scroll modes). A CR follows, written from this list rather than
from a guess.
