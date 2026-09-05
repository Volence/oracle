# Toy Story's perspective floor, measured on the live machine

**For aeon, as the reference for the perspective floor.** Read from the owner's own running window at two
camera positions on 2026-09-05, entirely through pure reads: no pause, no write, no resume. His game was
frame 7019 before and after every read at position 1, and 7186 at position 2, `running: false` throughout.

## ⚠ THE CONCLUSION IS SUSPENDED, 2026-09-05, AND THE ERROR IS MINE

**Do not build from the verdict this document originally carried.** It said the convergence is painted into
fixed art, the plane scrolls uniformly, and the vanishing point travels with the world. **The first
measurement offered in support of that is a null instrument, and the owner's own observation contradicts
the conclusion.**

### What the owner saw, which a rigid translation cannot produce

His words: *"as you move the plank on the right is pointed out right, then when you pass it in the center it
points down, and when it pass and it's on left it points and angles left. Just 2d flat art couldn't change
perspective like this as you move."* He is right on the physics. Scrolling a painted image moves a plank; it
cannot change that plank's **angle**.

### The null instrument, stated plainly because it was this document's lead evidence

The constant 8 pixel cell phase down the screen (4 everywhere at position 1, 0 everywhere at position 2) was
offered as the first of three measurements settling the painted reading. **It settles nothing between the
two hypotheses.** A per line or per strip scroll whose entries differ by WHOLE CELLS produces exactly the
same constant phase, because a multiple of 8 changes no sub cell phase at all. The original text even said
so as a caveat and then concluded past it, which is the defect: a limitation stated and then not carried
into the verdict. *(Raised independently by aeon and by the hub.)*

### What still stands

1. **Floor tile art and the plane B nametable are byte identical across the two camera positions**, while the
   plane scrolled. So the fan is drawn once and is never redrawn for a new camera. That is unaffected.
2. **38 unique consecutive tiles per cell row, no repeating pattern.** Unaffected.
3. **VSRAM is whole plane**, `0x0320, 0x0320`, then zeroes. Unaffected.

### What was measured after the challenge, and it constrains both stories

Per floor row, which nametable column sits at screen x=0 (correcting for the vertical scroll of 800, which
the first pass got wrong):

| | rows above the floor | floor rows y=180 to 220 | floor movement between positions |
|---|---|---|---|
| position 1 | column 0 | column 3, every row | |
| position 2 | column 0 | column 15, every row | 12 cells, 96 px |

- **Adjacent floor rows are NOT sheared**, at 8 pixel resolution across 8 line row spacing, at both positions.
  Combined with the constant phase (which does exclude sub cell differences), no fan is visible **between the
  rows sampled**.
- **But the floor and the region above it scroll by DIFFERENT amounts**, and by different amounts again
  between positions (the floor moved 12 cells, the upper region moved none). **That proves a per line or per
  region horizontal scroll table exists.**

### What is not established, and is the reason this is suspended rather than rewritten

**The scroll table has not been located.** Four search signatures over the two full VRAM captures failed to
find it: structural smoothness, the between position change signature, the within frame two region
signature, and both the per line (stride 4) and per cell (stride 32) storage layouts. A candidate found by
shape alone was already rejected once in this document for being the nametable, and no further candidate
will be adopted on shape.

So the honest state is: **a scroll table demonstrably exists and its contents are unknown.** The question
aeon poses is the right one and is answerable only from the table's LINE TO LINE DELTAS, not its absolute
values: all zero means uniform, all multiples of 8 means a whole cell shear, any non multiple of 8 means a
sub cell fan. (For contrast, aeon's own floor reads deltas of -1 and -2 per row.)

**This is settled in one call by `emulator/read_vdp_registers`, whose serve is in flight under §11.41:
register `$0B` names the mode and `$0D` names the table's base.** That is the same gap this document's last
section already raised, arriving as the thing that blocks the answer rather than merely slowing it.

**The owner should not be asked to move again.** Both captures are held and the window is untouched.

## ▶ THE TEST THAT REOPENS THIS, mechanical, no interpretation

*Agreed with the hub 2026-09-05. Run it the moment `emulator/read_vdp_registers` is served (§11.41). It
needs NO further frame and NO further play from the owner: two pure reads on the Toy Story socket, then
arithmetic on the two full VRAM captures already held under `docs/data/2026-09-05-toystory/`.*

1. `raw[0x0B]` gives the scroll modes: **bits 0-1 are HSCR** (whole plane, per cell strip, or per line) and
   **bit 2 is VSCR**. **If HSCR reads whole plane, the thread closes as painted and nothing else is needed.**
2. `raw[0x0D] & 0x3F`, shifted left 10, gives the **H-scroll table base**.
3. Index **both held captures** at that base and read the plane B entries for the floor lines, per line or
   per 8-line strip as the mode dictates.
4. Report the **LINE TO LINE DELTAS**, per position, against outcomes registered in advance:
   - **all zero** -> uniform scroll, painted only, settled;
   - **nonzero and growing with row depth** -> a per-row correction on top of the painted fan, which is what
     the owner describes watching;
   - and separately, whether the two positions' entries differ **by a constant** (translation) or **by a
     per-row amount** (a correction that tracks the camera).

**The absolute values carry a whole-frame offset and say nothing about shear; only the deltas do.** For
contrast, aeon's own floor reads deltas of -1 and -2 px per row over 71 of 71 floor rows.

**The raster read and the column test then either agree with this or the disagreement is itself the
finding.** Given how much of this document has already been withdrawn, the disagreement is the more
valuable outcome and should not be smoothed.

### ⚑ What this document has already cost, recorded so the next reader discounts it correctly

Four claims made here or in messages about it were withdrawn by their own author, each because a measurement
was reported as reaching further than the instrument could:
1. **The cell phase** offered as settling "not per line scroll". Null between uniform scroll and a whole
   cell shear, and the caveat was written and then dropped from the verdict.
2. **The VSRAM tail** offered as establishing whole plane MODE. It excludes a per column FAN and says
   nothing about the mode.
3. **Every state-resolved measurement** offered against mid-frame writes. `pixel_attribution` and a
   `stateRender` scanline read resolve one VDP state for every line by construction, so a mid-frame fan
   would produce exactly the results obtained and could not be detected by any of them.
4. **The column lookup** behind "uniform scroll, all rows shifted the same". It used a first-match index
   into a nametable row where **about 21 of 64 tiles repeat**, so the columns were not established. Redone
   by matching a unique multi-cell run.

The art-side vanishing point of "plane x 141" is **also withdrawn**: the separator colour was chosen by a
rarest-index heuristic that picked a different index per row, so no gap can be computed against it.

**The common shape, and it is the durable lesson: the reach of the instrument was repeatedly reported as
the reach of the evidence.** Three of the four were caught by peers, the fourth by the author. A reader
should treat every remaining number here as carrying that risk until the mechanical test above replaces it.

## What was measured, and how, since no VDP register is readable

This server serves no VDP register readback (see the ask at the end), so nothing below reads a register.
Every number is a measurement of observable behaviour or a byte read out of VRAM.

### 1. ~~The fan is NOT per-line horizontal scroll~~ ⚠ STRUCK: THIS SECTION IS A NULL INSTRUMENT

A line's horizontal scroll fixes where the 8 pixel cell grid falls on screen, and that is directly
observable: the screen `x` at which the winning plane B tile changes. A smooth perspective fan needs
**sub-cell** scroll differences between adjacent lines, so its phase would drift continuously down the
screen.

| position | frame | lines measured | distinct phases |
|---|---|---|---|
| 1 | 7019 | 48 (y=96 to 223) | **{4}** |
| 2 | 7186 | 20 | **{0}** |

~~**Constant down the screen at both positions, and shifted as a whole between them.** That is the signature
of a plane scrolled uniformly, not fanned per line.~~
⚠ **The measurement is real; the inference is void.** A per line or per strip scroll differing by WHOLE
CELLS gives the identical constant phase, so this excludes only a SUB CELL fan and says nothing about a
whole cell one. Do not cite this section as evidence for uniform scroll.

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

So the game did **not** redraw the fan for the new camera position. **That much stands.**
⚠ ~~If it had, the vanishing point would stay under the screen centre; it does not.~~ **STRUCK:** the second
clause does not follow. A fan drawn once can still be sheared afterwards by a per line scroll, which is what
the owner describes watching, and what the suspended conclusion at the top is waiting on.

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
