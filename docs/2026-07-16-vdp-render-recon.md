# VDP render-format recon-lite — the standard plane/scroll/tile byte formats

**Status: RECORDED 2026-07-16.** Companion to `docs/2026-07-16-vdp-recon.md`. That doc burned down the
design brief's **[recon]**-tagged *behavioral quirks* (R1–R12, incl. the R8 leftmost-column v-scroll rule
and the R9 window bug). This doc pins the **standard render byte-formats** the planes push (design brief
§6.3, `docs/2026-07-01-vdp-design.md` §3 steps 1–3) needs but which neither the brief nor the R1–R12 recon
writes down: nametable-entry layout, tile format, plane/window/scroll base-address registers, the h/v scroll
table layouts and modes, the scroll sign conventions, backdrop/display registers. The brief marks these
**[settled]** (standard documented VDP behavior) but does not give the bit layouts; the clean-room +
trace-to-pins rules require them pinned from a permitted source before code, so they are pinned here.

**Permitted sources only** (audit policy 3, identical to the R1–R12 recon): official Sega documentation
(Genesis Software Manual — segaretro.org / archive.org scans of Sega's own document), Plutiedev (including
the Kabuto hardware-notes mirror it hosts), SpritesMind forum hardware-test threads. **No emulator source
was opened** (BlastEm / jgenesis / Genesis Plus GX); no third-party docs outside the permitted classes
(genvdp.txt, rasterscroll.com, copetti.org, megacatstudios, huguesjohnson, chibiakumas were **excluded** —
where a fact appears only there it is treated as not-pinnable, not cited).

Items are numbered **RR1–RR7** (render-recon) to sit alongside R1–R12. Each states the pin, the evidence,
confidence, behavioral-vs-timing class, and the open remainder with its pin-vs-defer disposition.

---

## RR1 — Nametable / tilemap cell word format (§3 steps 2–3)

**PINNED.** Each plane cell is one 16-bit big-endian word:

| Bits | Field | Meaning |
|---|---|---|
| 15 | PRI | priority (1 = high) |
| 14–13 | PAL | palette line 0–3 (selects the CRAM row) |
| 12 | VF | vertical flip |
| 11 | HF | horizontal flip |
| 10–0 | tile | pattern index (0–2047), × 32 = VRAM byte address |

**Evidence**: Plutiedev "Tile ID flags" (plutiedev.com/tile-id) gives the assembler constants verbatim —
`HFLIP = $0800` (bit 11), `VFLIP = $1000` (bit 12), `PAL0..PAL3 = $0000/$2000/$4000/$6000` (bits 14–13),
`LOPRI/HIPRI = $0000/$8000` (bit 15); tile index in the low 11 bits. Corroborated by the Genesis Software
Manual scroll-screen pattern-name section. **Confidence**: high. **Classification**: behavioral (the exact
decode of every plane pixel). **Open remainder**: none.

## RR2 — Tile / pattern pixel format (§3 steps 2–3)

**PINNED — and already implemented** by `Vdp::tile_pixels` (verified byte-for-byte against this pin):

- 8×8 tile = **32 bytes** = 8 rows × 4 bytes; each byte packs **two 4-bit pixels**, the **high nibble is the
  left pixel**, proceeding left-to-right, rows top-to-bottom.
- **Colour index 0 = transparent** within a plane/sprite pixel (shows the layer beneath); index 0 is only
  opaque when it is the backdrop colour itself.
- A pixel's CRAM index = `PAL × 16 + nibble` (palette line from the cell word).

**Evidence**: Plutiedev "Tiles and palettes" (plutiedev.com/tiles-and-palettes) — *"every four bits is a
pixel … the leftmost pixel occupies the high nibble of the first byte in each row"*; *"The first colour is
always transparent, unless used as the background colour"*; four palettes × 16 colours × 32 bytes CRAM.
**Confidence**: high. **Classification**: behavioral. **Open remainder**: none (transparency's interaction
with shadow/highlight is R11 / push 5, out of scope here).

## RR3 — Plane / window nametable bases + plane size (§3 steps 2–3)

**PINNED** (64 KB VRAM mode; 128 KB mode is a design non-goal):

- **Plane A base** = `(reg $02 & 0x38) << 10` (bits 5–3 = SA15–13).
- **Plane B base** = `(reg $04 & 0x07) << 13` (bits 2–0 = SB15–13).
- **Window base** = `(reg $03 & 0x3E) << 10` (bits 5–1 = WD15–11); **in H40, bit 1 (WD11) must be 0** →
  effective mask `0x3C`.
- **Plane size** (reg $10): horizontal cells from bits 1–0 (HSZ), vertical cells from bits 5–4 (VSZ), with
  the documented valid set: `00→32, 01→64, 11→128` cells; combos 32×32, 64×32, 128×32, 32×64, 64×64,
  32×128. Cell = 8 px; a plane wraps modulo its pixel dimensions.
- A cell's word address = `base + (row_in_plane × plane_width_cells + col_in_plane) × 2`.

**Evidence**: Plutiedev "VDP register reference" (plutiedev.com/vdp-registers): `$82` SA15–13 bits 5–3;
`$83` WD15–11 bits 5–1, *"In H40 mode, bit 1 (WD11) must be 0"*; `$84` SB15–13 bits 2–0; `$90` HSZ bits
1–0 / VSZ bits 5–4 with the size table. Base-shift cross-checked against stock Sonic 2 register values
(reg $02 = `$30` → Plane A at `$C000`; reg $04 = `$07` → Plane B at `$E000`). **Confidence**: high on the
valid set. **Classification**: behavioral. **Open remainder**: the **invalid** HSZ/VSZ code `0b10` (and
oversized combos like 128×64) — Plutiedev lists only the valid set; hardware behaviour for `0b10` is not in a
permitted source → **model deterministically** (treat `0b10` as 64 cells, the numeric `((n==2)?64:...)`
clamp) and note it; no fixture uses it. *Recorded, not defer* — it is a model choice, flagged for the
golden-frame differential (push 5).

## RR4 — Backdrop colour, display enable, leftmost-column blank (§3 step 1)

**PINNED:**

- **Backdrop / background colour** = `reg $07 & 0x3F` — a **direct CRAM index** (bits 5–4 = palette line,
  bits 3–0 = colour). Every dot with no opaque layer resolves to this CRAM entry.
- **Display enable** = reg $01 bit 6 (DISP). When 0, the active area shows the **backdrop colour only** (no
  planes/sprites) — the standard "display disabled → blanked to backdrop" behaviour.
- **Leftmost-column blank** = reg $00 bit 5 (LCB): forces the **leftmost 8-px column** to the backdrop
  colour (games use it to hide the partial fine-scroll column). Applied after plane resolution.

**Evidence**: Plutiedev "VDP register reference": `$87` CPT1–0 bits 5–4 + COL3–0 bits 3–0; `$81` bit 6 DISP
*"1 to enable rendering"*; `$80` bit 5 LCB *"1 to blank the leftmost column (8px wide)"*. **Confidence**:
high. **Classification**: behavioral. **Open remainder**: none. (LCB is a small, cheap correctness item; it
is **in scope** for the planes push — a one-line post-pass — but if the reviewer scopes it out it moves to
push 5 with the rest of the output-stage table. Surfaced as a decision in the plan.)

## RR5 — Horizontal scroll: table, modes, sign (§3 step 2)

**PINNED:**

- **HScroll table base** = `(reg $0D & 0x3F) << 10` (bits 5–0 = HS15–10).
- **Entry format**: each entry is **two words** — **Scroll A first (offset +0), Scroll B second (+2)** —
  i.e. 4 bytes per entry (Sega manual "H SCROLL DATA TABLE": A-scrolling-quantity at offset 00, B at 02).
  The scroll value is the low 10 bits (`& 0x3FF`); the plane wraps modulo its pixel width, so masking to the
  plane width subsumes the field width.
- **Mode** (reg $0B bits 1–0 = HSCR/LSCR), byte offset into the table for display line `L`:
  - `00` **full**: `0` (one value for the whole screen).
  - `01` **"scroll eight lines then repeat"**: `(L & 7) × 4`.
  - `10` **cell / "scroll every tile"**: `(L & ~7) × 4` (one value held across each 8-line tile row).
  - `11` **line / "scroll every line"**: `L × 4`.
- **Sign**: an **increasing** horizontal scroll value shifts the plane to the **right** — the plane's
  pixel-generator "fetches another fresh tile at the left edge" as X-scroll increases (Kabuto). So a screen
  pixel at column `X` samples plane pixel `(X − hscroll) mod plane_width_px`.

**Evidence**: Plutiedev "VDP register reference" (`$8D` HS15–10; `$8B` the four HSCR mode descriptions
verbatim: *"00: full scroll / 01: scroll eight lines, then repeat / 10: scroll every tile / 11: scroll every
line"*); Genesis Software Manual "H SCROLL DATA TABLE" (A-then-B word order, per-line 4-byte layout shared
across modes); Kabuto hardware-notes (the fresh-tile-at-left-edge direction statement, hosted on Plutiedev).
**Confidence**: high on full/line/sign; **medium** on the exact table indexing of the two intermediate modes
(`01`, `10`) — the mode *descriptions* are permitted-pinned but the byte-offset formula is the standard
reading of "held for 8 lines" against the shared per-line table, not a verbatim formula. **Classification**:
behavioral. **Open remainder**: the `01`/`10` byte-offset formulas — **interim model as written**, flagged
for confirm-by-golden-differential in push 5 (fixtures exercise only `00`/`11`, which are fully pinned).

## RR6 — Vertical scroll: VSRAM, mode, sign (§3 step 2)

**PINNED:**

- **VSRAM** = 80 bytes (40 words), addresses `$00–$4F` (40-word part; the 64-word Model-2-VA4+ variant is a
  revision fork — recon R8 open remainder, ledgered).
- **Full mode** (reg $0B bit 2 = 0): **Scroll A = word 0 (`$00`), Scroll B = word 1 (`$02`)** — one value per
  plane for the whole screen.
- **2-cell mode** (reg $0B bit 2 = 1): one A/B pair per **16-px column**. Column `C` (0-based, left to right):
  `A = VSRAM[C×4 + 0]` (word), `B = VSRAM[C×4 + 2]`. 16 columns in H32, 20 in H40.
- **Sign**: an **increasing** vertical scroll value scrolls the plane **up** (view moves down into the plane).
  A screen pixel at row `Y` samples plane pixel row `(Y + vscroll) mod plane_height_px`.
- **Leftmost partial-column v-scroll (recon R8, cross-ref)**: when `hscroll % 16 ≠ 0` in 2-cell mode, the
  partial left column's v-scroll = `VSRAM[$4C] & VSRAM[$4E]` (H40; AND of the two last word entries — column
  19 A/B) / **fixed 0** (H32), the *same* value for both planes. Already pinned in
  `docs/2026-07-16-vdp-recon.md` R8; this doc just fixes the byte offsets (`$4C` = word 38 = column 19 A;
  `$4E` = word 39 = column 19 B — consistent with the 2-cell layout above).

**Evidence**: Genesis Software Manual (VSRAM 80 bytes `$00–$4F`; two-cell-unit per-column A/B; *"increasing
VSRAM values scroll upward"*); Plutiedev "VDP register reference" (`$8B` bit 2 VSCR *"0 = full; 1 = scroll
every two tiles"*); SpritesMind t=737 / Plutiedev hardware-issues (the VSRAM 40→64-word revision fork, R8).
**Confidence**: high on layout/sign; the A-before-B word order is the manual's + is self-consistent with the
R8 `$4C`/`$4E` offsets. **Classification**: behavioral. **Open remainder**: none new (R8's revision variance
is already ledgered).

## RR7 — Layer compositing + palette → CRAM index (§3, the planes-push subset)

**PINNED from the design brief §3** (design-authority, recorded so the planes push has an explicit in-scope
boundary). The full step-5 *priority* ordering is
`high-sprite > high-A > high-B > low-sprite > low-A > low-B > backdrop`. Per the owner's scope
(`priority / shadow-highlight are pushes 4–5 — do NOT pull them in`), **the priority *ordering* — including
the plane priority bit that lets B appear in front of A — is push 5**, together with **sprites (push 4)** and
**shadow/highlight (reg $0C bit 3, R11, push 5)**. The planes push composites by **transparency only**, in a
fixed layer order:

- Per dot, first **opaque** of: **plane A / window** → **plane B** → **backdrop**. "Opaque" = tile nibble ≠ 0
  (RR2). The priority bit is **decoded and reported** (RR1 → `PixelResolution.priority` / `render_line_report`)
  but does **not** affect ordering this push — push 5 slots the priority bit + sprites into the full step-5
  ordering behind the same report.
- The winning cell's colour → CRAM index `PAL × 16 + nibble` → RGB via the fixed integer ramp
  (`Vdp::cram_decoded` / `ramp3`, no floats). Backdrop = `reg $07 & 0x3F` (RR4).

**Evidence**: `docs/2026-07-01-vdp-design.md` §3 (ratified) + the push-3 scope brief. **Confidence**: high
(design-pinned). **Classification**: behavioral. **Open remainder**: the priority-bit ordering + sprite
entries + shadow/highlight operators — all push 4–5, deliberately deferred. Surfaced as plan decision 1
(push 3 = transparency compositing; the priority bit is decoded-but-not-ordered).

---

## RR8 — Sprite attribute table byte format + sprite render geometry (§3 step 4, the sprite push)

**PINNED** (added 2026-07-16 for the sprite push — VDP push 4). Companion to the behavioral R5 (SAT
cache update rules) / R10 (x=0 masking + per-line limits); this item pins the *standard render byte-formats*
the sprite walk + render need — the SAT entry layout, the sprite tile/attr word, the coordinate offsets, the
size encoding, the column-major multi-cell tile order, the SAT base register, and the link-walk termination —
none of which R5/R10 wrote down.

- **SAT entry = 8 bytes, big-endian words** (Plutiedev "Sprites"):

  | Offset | Size | Field | Layout |
  |---|---|---|---|
  | +0 | word | **Y coordinate** | bits 9–0 (`& 0x3FF`); **screen Y = `Y − 128`** |
  | +2 | byte | **size** | bits 3–2 = **width** cells−1 (HSZ), bits 1–0 = **height** cells−1 (VSZ) |
  | +3 | byte | **link** | bits 6–0 (`& 0x7F`) — next sprite index |
  | +4 | word | **tile / attributes** | the **same 16-bit format as a nametable cell (RR1)**: bit 15 priority, 14–13 palette, 12 vflip, 11 hflip, 10–0 tile |
  | +6 | word | **X coordinate** | bits 8–0 (`& 0x1FF`); **screen X = `X − 128`** |

- **Coordinate offset**: *"You need to add 128 to both to get the value to write in the table"* (Plutiedev) —
  so on-screen position = field − 128 for both axes (128 px of off-screen border each side, for smooth
  entry/exit and full-off-screen parking). Y is 10 bits (`0x3FF`, Sega manual — allows the interlace-double
  range and top-off-screen sprites); X is 9 bits (`0x1FF`, R10 / Natsumi).
- **Size encoding** (Plutiedev, verbatim): *"Bits 3-2 are the width of the sprite, bits 1-0 are the height of
  the sprite. Take the size in tiles then subtract one (i.e. `00` is 1 tile, `11` is 4 tiles)."* So
  `width_cells = (byte>>2 & 3) + 1`, `height_cells = (byte & 3) + 1`, each 1–4 cells (8–32 px).
- **Cached vs render-fetched half** (cross-ref R5): the VDP caches **`+0..+4` = Y (word) + size/link (word)**;
  **`+4..+8` = tile/attr (word) + X (word) are fetched from VRAM at render time.** Phase-1 evaluation reads
  ONLY the cache (so it never sees X — R10's "masking is a render-phase effect" corollary).
- **SAT base register** (reg $05, Plutiedev: *"Bits 15-9 of the sprite table address are put into bits 6-0 of
  this register"*): **base = `(reg $05 & 0x7F) << 9`** ($200 boundary). **H40 masks reg-5 bit 0** →
  `(reg $05 & 0x7E) << 9` ($400 boundary) — R5.
- **Multi-cell tile layout — column-major** (Plutiedev, verbatim): *"Tiles are arranged first vertically then
  horizontally."* For a `W×H`-cell sprite with base tile `T`, the cell at sprite-relative `(cx, cy)` (0-based,
  `cx` across, `cy` down) uses tile **`T + cx·H + cy`** (the index runs down each column before moving right).
  Flips (RR1): **hflip** mirrors the cell columns (`cx → W−1−cx`) *and* the within-cell px (`& 7 ^ 7`);
  **vflip** mirrors the cell rows (`cy → H−1−cy`) *and* the within-cell py.
- **Link walk / termination** (Plutiedev: *"slap the sprite number plus one … except for the last sprite,
  where you'll put 0 in it"*): the VDP starts at **sprite 0**, renders it, then jumps to the sprite named by
  its link field, and so on; the walk **terminates when a link is 0** (points back at sprite 0) or when the
  hardware **maximum of 80 (H40) / 64 (H32) sprites** has been parsed (the cap bounds a cyclic link list — it
  cannot hang). Sprites unreachable because the list terminated early are **link-cut** (never evaluated).
- **Per-line limits** (Plutiedev, cross-ref R10): H40 = **80** total / **20** per line / **320** sprite px per
  line; H32 = **64** / **16** / **256**.

**Evidence**: Plutiedev "Sprites" (plutiedev.com/sprites — the 8-byte entry table, the +128 offset, the size
byte layout verbatim, the column-major tile arrangement, the link-plus-one convention, the reg-5 base bits,
and both per-resolution limit sets); Plutiedev "Tile ID flags" (the +4 attribute word = the RR1 nametable
format); Sega Genesis Software Manual (Y 10-bit, sprite-list mechanics); recon R5 (cache half / base H40
mask) + R10 (X 9-bit, masking, limits). **Confidence**: high on the entry layout / size / column-major /
link / base / limits (Plutiedev verbatim); high on the attribute word (identical to RR1, an established
format). **Classification**: behavioral (the exact decode + geometry of every sprite pixel).

**Open remainder** — the two R5 sprite-push cells (both behavioral, both a *sub-case of the already-pinned
write-through rule*, so each gets a **deterministic interim model = the consistent extension of the pinned
rule**, flagged for the push-5 golden-frame differential — the same pin mechanism as R8/R9, **not** an xfail):

1. **Does the H40 reg-5 bit-0 mask apply to the write-through window compare?** — *interim: yes.* The cache
   window is computed from the **same masked base evaluation uses** (`(reg5 & 0x7E) << 9` in H40), so the
   cache tracks exactly the SAT the VDP will read. This is the consistent reading of R5 ("checked against the
   window computed from the current reg 5"). **Concrete defer reason**: the internal SAT cache is not
   observable over the ratified BlastEm GDB-RSP instrument (which exposes CPU + memory, not the VDP
   framebuffer or the internal cache); the only CPU-readable proxies (sprite overflow/collision status bits)
   don't discriminate a one-address window-base shift. Pinned instead by the push-5 golden-frame differential
   on `s4.bin` (rendered position of a stale-cache sprite is directly observable there).
2. **Do byte-wide / odd-address writes into the Y/size fields update the cache?** — *interim: yes, by
   construction.* The write-through is modeled **byte-granular** (every VRAM byte write — including the
   odd-address byte-swapped half — is checked against the window and mirrored), so a `move.b` into a cached
   byte updates exactly that cache byte. This is the faithful application of "every VRAM write is checked."
   **Concrete defer reason**: same instrument limit as (1); the differential surfaces it if a game relies on
   sub-word SAT pokes. (DMA fill/copy × cache stays deferred to push 6 — no DMA engine exists yet; interim
   per R5 = fill/copy steps hit the window compare like any write.)

---

## Summary scoreboard

| Item | Pin | Class | Remainder |
|---|---|---|---|
| RR1 nametable word | pinned | behavioral | none |
| RR2 tile format | pinned (impl matches) | behavioral | none |
| RR3 bases + plane size | pinned | behavioral | invalid HSZ/VSZ `0b10` → model choice |
| RR4 backdrop/DISP/LCB | pinned | behavioral | none |
| RR5 hscroll table/modes/sign | pinned (full/line/sign); medium on `01`/`10` indexing | behavioral | `01`/`10` byte offsets → interim, golden-diff push 5 |
| RR6 vscroll VSRAM/mode/sign | pinned | behavioral | none (R8 variance already ledgered) |
| RR7 layer compositing | design-pinned | behavioral | priority-bit ordering + sprites + S/H → pushes 4–5 |
| RR8 SAT format + sprite geometry | pinned (Plutiedev verbatim) | behavioral | R5 window-base H40 mask + odd-address SAT writes → interim, golden-diff push 5 |

Every render fact the planes/sprite pushes need is pinned from a permitted source or, where a permitted
verbatim formula is unavailable (the two intermediate hscroll modes, the invalid plane-size code) or a
behavioral sub-case is not instrument-observable (the two R5 cache-window cells), recorded as an explicit
deterministic interim model flagged for the golden-frame differential — never improvised silently.
