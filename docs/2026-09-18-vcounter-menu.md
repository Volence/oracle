# `vcounter` — driving the menu, and the false reason that kept it unscraped

**Parcel `TESTROM-VCOUNTER-MENU`, 2026-09-18.** Branch `parcel/vcounter-menu`.
Companion to `docs/2026-07-25-testrom-conformance.md` (the scorecard ledger) and
`docs/2026-09-18-h40-half.md` (the parcel whose `ProvenScreen` pattern this one reuses).

No file under `crates/oracle-core/src/` was touched. This is an instrument change.

---

## 1. The premise this parcel was given was FALSE, and correcting it is part of the deliverable

The ledger said of this ROM, from its first pin on 2026-07-25 until today:

> Frame hash only — **not scraped**: the ROM draws its results in a proportional font that is not an
> ASCII-ordered nametable, so the text-scrape path does not apply

and open question **Q2** added:

> Both are automatable in principle; neither yields its verdict through the ASCII-nametable path the other
> text ROMs share. Deferred, not blocked.

Both statements are wrong about this ROM. Measured, in one command:

```
$ cargo run -p oracle-core --example testrom_probe -- vendor/TestRoms/vcounter.bin 200 100
width=320 regs2=10
--- plane A base $4000 plane_w=64 cols=40
01|..V counter test program................|
02|..by Charles MacDonald..................|
05|..0. Mode 4 (256x192)...................|
...
16|..Press START to run a test.............|
18|..Press A to toggle scanning 262 or ....|
21|..Lines to scan: 262....................|
```

That third argument is the font base: `$100`. It is **the same base `m68k_memory_test` already used**, read
through the very same `text_rows`. The text is ordinary ASCII-ordered nametable text; there is no
proportional font, and there is no glyph table to build. Eight weeks of frame hashes stood in place of a
verdict because a note said the door was locked.

**The real blocker, and it is a different one.** This ROM is **menu-driven**: it prints nine mode choices and
a scan-length toggle and does nothing at all until a button is pressed. Nothing in the harness drove a menu
for it. So the work was navigation, not glyph archaeology — which is why this parcel is a page of button
presses and proofs rather than a font decoder.

Both claims are corrected in the ledger in this same change: the row's "how it is scraped" column, and Q2,
which shrinks to `m68k_opcode_sizes` alone (a separate parcel, deliberately untouched here).

---

## 2. The ROM, measured

Everything below was established with `testrom_probe`. Two probe features were added for it and both are
documented in the ledger's "How to amend a row": `PRESSES=<at>:<btn>:<len>,…` (a press *script*, d-pad
included — a menu needs more than one button) and `PRIO_ROWS=1` (the nametable's priority bits per row).

### 2.1 The mode cursor is a priority-bit highlight, not text

The menu lists nine items and shows no cursor in the text grid. `RAW_ROW=8` explains why:

```
RAW row 5: 0000 0000 0130 012E 0120 014D ...
RAW row 8: 8000 8000 8133 812E 8120 814D ...   <-- bit 15 set across the whole row
RAW row 9: 0000 0000 0134 012E 0120 014D ...
```

The selected line is highlighted by setting the nametable cell's **priority bit** on every cell of that row
and nowhere else. `PRIO_ROWS=1` reads that channel directly. Measured:

| action | highlighted cell row | menu item |
|---|---|---|
| power-on | 8 | **3** (Mode 5 320x224) |
| `up` x1 | 7 | 2 |
| `down` x1 | 9 | 4 |
| `up` x3 | 5 | 0 |
| `up` x5 | 5 | 0 — **clamps**, does not wrap |
| `down` x5 | 13 | 8 |
| `down` x7 | 13 | 8 — clamps |
| `up` held 30 frames | 7 | 2 — **edge-shaped**, one move per press |
| `up` held 1 frame | 7 | 2 — one frame is enough |

So items 0..=8 are cell rows 5..=13, the cursor moves one row per press, it clamps at both ends, and the
input is edge-triggered. That last pair of rows is why a press in this harness is two frames held and two
released: it is slack, not a tuned number — every press is followed by a wait on the effect it was supposed
to have.

### 2.2 `A` toggles the scan length; `Start` runs; `Start` again exits

* `A` on the menu flips the ROM's own `Lines to scan: 262` line to `312` and back.
* `Start` runs the selected test and lands on a result screen headed
  `Page: 0/2  Reg: 8C81 8144  Format:0`, with `Press START to exit`, `Press A to format for IM2 values`,
  `Press B for the previous page`, `Press C for the next page`.
* `C` pages forward; on the last page it does nothing.
* `Start` on the result screen **exits back to the menu**, and both the cursor row and the `A` toggle
  survive the round trip (measured: `down`, `Start`, `Start` leaves the cursor on row 9; `A`, `Start`,
  `Start` leaves `Lines to scan: 312`).

That last fact is why the whole grid — nine modes x two scan lengths — is driven from **one boot**.

### 2.3 The result screen is a table of the counter words, and the ROM names its own registers

The `Reg:` field is the pair of **control-port register-write words** the ROM used for the run, so it is the
ROM's own statement of which mode it ran. Measured, one row per menu item:

| item | label on screen | `Reg:` | reg 12 | reg 1 |
|---|---|---|---|---|
| 0 | Mode 4 (256x192) | `8C00 8140` | `$00` | `$40` |
| 1 | Mode 4 (256x192, interlace 1) | `8C02 8140` | `$02` | `$40` |
| 2 | Mode 4 (256x192, interlace 2) | `8C06 8140` | `$06` | `$40` |
| 3 | Mode 5 (320x224) | `8C81 8144` | `$81` | `$44` |
| 4 | Mode 5 (320x448, interlace 1) | `8C83 8144` | `$83` | `$44` |
| 5 | Mode 5 (320x448, interlace 2) | `8C87 8144` | `$87` | `$44` |
| 6 | Mode 5 (320x240, PAL) | `8C81 814C` | `$81` | `$4C` |
| 7 | Mode 5 (320x480, interlace 1+PAL) | `8C83 814C` | `$83` | `$4C` |
| 8 | Mode 5 (320x480, interlace 2+PAL) | `8C87 814C` | `$87` | `$4C` |

Reg 12 bit 7 = RS1 (H40), bits 2-1 = LSM1/LSM0 (interlace). Reg 1 bit 6 = display enable, bit 3 = M2/V30
(240-line), bit 2 = M5. The ROM's "PAL" items are **V30**, not a region change: it sets reg 1 bit 3.

**The nine pairs are pairwise distinct**, which is what makes the `Reg:` field a complete discriminator over
the menu — and `the_vcounter_mode_table_discriminates_the_nine_modes` asserts that rather than trusting it.

The table itself: 16 cell rows from cell row 7, 8 columns of `XXXX ` five characters wide, filled
**column-major**. Column-major is a measurement and the page that settles it is the last one: with a
262-line scan page 2 holds six values and they sit in column 0, rows 7..=12 — row-major would have put them
in row 7, columns 0..=5. Re-confirmed on the 312-line scan, whose page 2 fills columns 0, 1, 2 and then
eight rows of column 3 (56 values). Page totals: 128 + 128 + 6 = 262, and 128 + 128 + 56 = 312 — the scan
length the menu printed, which is the harness's table-length gate.

---

## 3. What the ROM reports, and the verdict

All nine modes, both scan lengths, 18 readings from one boot in **428 frames**:

```
262-line scan: modes[0-8]=$00-$EA(235),$E5-$FF(27)                 [recon-R2 NTSC-V28 match 9/9]
312-line scan: modes[0-8]=$00-$EA(235),$E5-$FF(27),$00-$31(50)     [recon-R2 NTSC-V28 match 9/9]
```

### 3.1 Item 3 is a real pass

`docs/2026-07-16-vdp-recon.md` §R2 pins the NTSC V28 V counter as `0x00–0xEA` then `0xE5–0xFF`
(235 + 27 = 262 lines), the jump being `0xEA → 0xE5`. That is character-for-character what the ROM printed
for menu item 3, the one V28 NTSC mode on its list. The comparison is made against `vc_r2_ntsc_v28`, which
**restates R2's progression instead of calling `Vdp::v_counter`** — deliberately, so that a change to the
model moves this row rather than moving the expectation along with it.

### 3.2 The other eight agree by being UNMODELLED — recorded, not fixed

This is a **limitation pinned as a baseline**, in the same way `vdp_port_access` carried its 46 failures for
months. Derived from the tree, not from a guess:

* `Vdp::v_counter` is `fn v_counter(&self, mclk: u64) -> u8` and its body reads **no register at all** — it
  is `(mclk % MCLK_PER_FRAME) / MCLK_PER_LINE` remapped. So reg 12's LSM bits and reg 1's M4 and M2/V30
  bits cannot move it, by construction.
* `ACTIVE_LINES` (224) is documented in `vdp.rs` as "**the only vertical mode this core models**", and the
  same doc comment says V30 "would also have to move `LINES_PER_FRAME`, which this core fixes at NTSC's
  262".
* Interlace exists in the model (`Vdp::interlace_enabled`, reg 12 bit 1) but only to drive the ODD status
  bit; it touches no vertical timing.

So eight of the nine readings are the V28 sequence answering a question about a mode the core does not
have. The row says so in its own text (`V28-ONLY MODEL: only mode 3 is a V28 mode, so the other 8 agree by
being unmodelled`) so that nine identical lines can never read as nine passes. **Nothing was fixed to make
a verdict pass**, per this instrument's charter.

### 3.3 Why the 312-line half is worth its keep

A scan longer than one frame wraps, and the wrap point is the frame's **total length** — the one quantity in
this ROM that the 262-line scan cannot show. `$00-$31(50)` says the scan ran 50 lines into the next frame:
262 lines per frame, read off the ROM rather than out of `LINES_PER_FRAME`. A PAL/V30 frame is 313 lines, so
that segment is precisely where items 6-8 would separate from the rest if this core ever grew V30 — which is
the whole reason to carry them.

### 3.4 Scope: what was excluded, and why nothing was

**Nothing was excluded.** The reasoning, since the brief asked for it rather than a maximal sweep:

* The navigation is identical for all nine items — one extra keypress each — so the marginal cost of the
  eight unmodelled modes is a keypress and one line of grouped output, not nine scorecard rows.
* They are *reachable and complete*: every one selects, runs, prints its own register pair and a full-length
  table. There is no unmodelled-machine wall that makes a reading impossible, only one that makes eight
  readings **the same**, which is itself the finding.
* They are the only thing in the corpus that would **announce** an interlace or V30 vertical-timing model
  arriving, correct or not: the row's grouping splits the moment one of them differs.
* An excluded mode with a stated reason is a result; an excluded mode that silently never ran is the hole
  this row was opened to close. Grouping the identical readings costs nothing and hides nothing, because
  `the_vcounter_row_measures_nine_established_modes` asserts the set of 18 established (item, scan length)
  pairs is exactly the whole grid.

One thing *is* deliberately not read: the result screen's own `A` key, which re-formats the words for IM2.
`Format:0` — the raw counter word — is asserted, so a future reading in the other format cannot slip in
unnoticed. Re-formatting is a display choice about values this row already has, not a second measurement.

---

## 4. The gate: `ProvenVcMode`

The failure this row must not have is the one `ProvenScreen` was built for a parcel earlier: **a verdict
scraped from a state that was never established**. If a selection does not take — a lost press, a cursor
clamped at the end of the list, an `A` toggle flipped the wrong way — the run reads the *previous* mode's
result screen, every field parses, the table decodes, and the row prints nine plausible verdicts for tests
that never ran.

So the proof is a type, and it **gates** rather than accompanies: `ProvenVcMode::establish` is the only
constructor, it refuses loudly, and `vc_classify` — the only thing that turns raw counter words into the
row — cannot be called without the value it returns. An unestablished mode is not representable.

Four machine-read facts, in the order the gate checks them:

1. **The ROM says it programmed this item's registers.** The `Reg:` pair off its own result screen, against
   the table in §2.3, whose nine entries are pairwise distinct. Checked first because it is the ROM's own
   record of what ran and names the mode that *actually* ran in the failure message.
2. **The cursor was on this item's row**, read out of the priority bits immediately before `Start` — and
   every single step that put it there was verified one press at a time (`vc_select` waits for the highlight
   to land on the next row, so a lost or doubled press fails at that step rather than at the end).
3. **The menu's printed scan length is the one asked for.**
4. **The table is that many entries long.** `Lines to scan: 262` with 256 values means a page was missed,
   and a short table still reads as a clean ascending sequence.

Plus `Format:0`, above.

Two supporting refusals, both loud: `vc_page_values` rejects a page with a **hole** (a blank cell inside a
column, or a filled column after a short one) rather than compacting it away into a shorter table that still
looks clean; and every wait in the navigation names what it was waiting for when it gives up.

**One measured surprise worth recording.** The result screen's header appears *before* the table under it is
finished being drawn: reading page 0 as soon as the header parsed yielded **212 of 262** values. Each page is
therefore waited on until it holds its own full count — derived from the scan length the ROM itself printed
— rather than until the page number changes. Gate 4 is what caught that, on the first run.

---

## 5. Red-first proofs

Baseline for restore: commit `20c201c`, tree clean, suite green (9 passed) before each mutation and restored
from the commit after it. Each mutation was written to disk, shown with `git diff --stat` plus the mutated
line, and the guard that actually fired was read off the panic — not merely "the suite went red".

| # | mutation (on disk) | guard that FIRED | evidence |
|---|---|---|---|
| 1 | `vc_select` returns 0 immediately — the cursor never moves | **`ProvenVcMode::establish`'s `Reg:` pair assert** | `says it ran with reg12=$81 reg1=$44, but item 0 (M4-256x192) is $00/$40` — it names the power-on mode, which is exactly the "read the previous screen" failure |
| 2 | the `vc_press` inside `vc_select` is removed (the wait is left in place) | **`vc_select`'s per-step effect wait** | `one press to move the menu cursor from row 8 to row 7 did not happen within 30 frames (pc $001572)` |
| 3 | `vc_set_scan_len` never presses `A`, and its own effect-wait is deleted so it cannot be the thing that catches it | **`establish`'s scan-length assert** | `menu said it would scan Some(262) lines, not 312` — and it fired on the first item of the 312 pass, i.e. after nine clean 262-line readings |
| 4 | the last result page is never read (`0..max_page`) | **`establish`'s table-length assert** | `printed 256 values for a 262-line scan — a page was missed` |
| 5 | `vc_r2_ntsc_v28` drops the `0xEA → 0xE5` jump | **both** the control arm's `matched > 0` assert *and* the scorecard row | `not one of vcounter's 18 readings matches the recon-R2 NTSC V28 sequence`; and the row printed `[recon-R2 NTSC-V28 match 0/9]` twice, failing `testrom_conformance_scorecard` |
| 6 | item 6 is given item 3's register pair | **`the_vcounter_mode_table_discriminates_the_nine_modes`** (and, independently, the `Reg:` gate, because the table then disagrees with the machine) | `items 3 (M5-320x224) and 6 (M5-320x240-PAL) share the register pair $81/$44` |
| 7 | `VC_TABLE_ROW0` 7 → 6 (the table is read one cell row too high) | **`vc_page_values`' hole refusal** | `result page has a hole: column 0 row 1 holds 0x0000 but an earlier row of that column is blank` |

None of the seven was a compile error; each ran the test binary and reached the named assertion. Mutation 1
compiled with an `unreachable statement` **warning** only, and the test executed (0.12 s) before failing.

---

## 6. Open, and not done here

* **`m68k_opcode_sizes`** — the other half of Q2. Untouched by instruction; Q2 now names it alone.
* **A `vcounter` verdict for interlace, V30 or Mode 4 vertical timing** requires the core to model more than
  one vertical mode (`LINES_PER_FRAME`, `ACTIVE_LINES` and `Vdp::v_counter` move together, as `vdp.rs`
  already says). That is a model arc, not an instrument change, and this parcel deliberately did not start
  it. When it happens, this row is the thing that will show it: the 312-line segment splits on frame length
  and the 262-line segment on the counter's shape.
* **The V counter's sub-line phase** stays an open R2 item (we increment at the line boundary; hardware
  increments mid-line at H `$84→$85` / `$A4→$A5`). This ROM samples once per line and so cannot see it —
  worth stating, because it means a 9/9 match here is not a claim about phase.
