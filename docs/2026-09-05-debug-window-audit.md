# The data-display audit: what every panel's data becomes

**Status:** the build order for the debug window's panels. One row per panel, each saying what it shows
now, how it reads badly, and the specific treatment it gets. Decidable by somebody who is not the author.

**The brief, in the owner's own words:**

> *"Can you do a nice audit to make the ui on this look nicer? just the data display for most things"*

and, from the day before, the specification:

> *"super clean and readable and pops in areas like this where it's just data that doesn't necessarily need
> like a visual built like a graph or piano roll"*

He is not asking for charts. He is asking for plain data to be typographically strong: readable at a
glance, with the important number popping, in panels that are just facts.

**Rules:** `docs/2026-09-05-debug-window-style.md` §3 (P1 to P10) and §3a (the bar). This page does not
restate them; it applies them and, in two places, **corrects them**.

**Sources.** Every line number below was re-derived at the revision this page landed on, and each was
checked to land on the construct it is cited for. The suite-filed copy of the style page is `empyrean`
`87a8d70:design/DEBUG_WINDOW_STYLE.md`.

---

## 0. Six corrections to the record, before anything is built on it

These are the things that were believed going in and are not true. Each cost a real amount of the audit to
establish, and each would have sent the next parcel somewhere wrong.

### 0.1 The style page's P2 check is narrower than P2

P2 says tabular data goes in a table with fixed columns, and names its check:

> *the panel contains no width-padded format specifier.* `grep -cE '\{:[<>^][0-9]+'`

**That check returns zero on the Pacing tab, which is the worst offender the owner photographed.** Its
padding lived in the string literal, not in the specifier:

```rust
ui.monospace(format!("frames emulated   {}", self.machine.frames()));
ui.monospace(format!("governor rebases  {}   <- stalls of a whole frame or more", ...));
```

Three spaces, hand-counted, doing a column's work. The rule was right and its stated check could not see
it. **The widened check is on the rendered value, not on the source:** no string a panel draws contains a
run of two spaces or a tab. `pacing.rs`'s `no_string_the_tab_draws_pads_itself_into_a_column` is that check
as a gate, and it is proven red against exactly the string above.

The check is also blind in a second direction: it is positional, so **named captures escape it**.
`ui.rs:756`'s `format!("{label:<18}{value}")` (the status strip, seven or more rows of label and value) and
`report.rs:169`'s `{count:>8}` are both padded specifiers the published regex does not match. The corrected
source check is `\{[a-zA-Z_]*:[<>^][0-9]+`.

### 0.2 `pacing.rs`, `stats.rs`, `machine.rs`, `device.rs`, `symbols.rs`, `battery.rs`, `report.rs`, `identity.rs`, `screen_pick.rs`, `planes.rs`, `states.rs`, `bus.rs` are not panels

Twelve of the seventeen files the brief lists as panels contain **no `ui.*` call at all**. They are models:
a governor, a frame-time series, the emulator wrapper, a CPAL device, a symbol loader, an SRAM file, a
bench report, a build chip, pick geometry, a nametable rasteriser, save slots, and the Aether transport.

The window's real unit is the **tab**, and there are nine of them (`ui.rs:68-93`), plus two strips that are
not tabs. Almost every `ui.*` call in the crate lives in `ui.rs`. **Auditing by file was the wrong axis**
and this page is organised by tab. The file column is kept so the brief's list can be reconciled.

Two consequences worth stating rather than discovering:

* **`stats.rs` and `report.rs` have no window surface at all.** `report::print` is `println!` only, called
  twice from `main.rs` at the end of a `--bench-*` run. P1 to P9 govern `ui.*` calls and do not bind them.
  P10 does, because it binds every tool and a person reads that terminal.
* **`bus.rs` is 3857 lines of transport and renders nothing.** Its only `egui` reference outside doc
  comments is a `#[cfg(test)]` pixel-sampling helper.

### 0.3 There is a live raw-JSON path on the screen today, and it is not the one the style page named

The style page names `ui.rs`'s `render` helper as "the one raw-JSON path that is still live". It was
latent: safe until a served key stopped being a scalar. **It is now closed** (§3 below).

The one that is live, right now, on the owner's screen, is `memory.rs:672`:

```rust
Answer::Ok(v) => format!("ok — {v}"),
```

`v` is a `serde_json::Value`, rendered by `Display`. `note_label` (`ui.rs:2291`) then puts the result in
`ui.monospace`. Every successful gesture that funnels through `Panels::issue` (`ui.rs:1244`) renders
literally `ok — {"breakpoint":"b3","addr":"0x00001234",...}`. That is **Breakpoints, Watchpoints, Profiler,
the Memory write cell and `memory_hash`** — five surfaces, one line. Three rules in one expression: **P1**
(raw `Value` by `Display`), **P3** (a JSON blob in the register face), **P10** (the em dash in `"ok — "`).

Two more sites in the same module do the same thing, found while checking this page's own citations:
`memory.rs:504` and `memory.rs:518` each interpolate a whole `Value` into a sentence on the address box,
and the second fires on **every prefix search**, which is not a rare path. So it is three sites, not one.

**This is the highest-value single fix in the audit and it is not the exemplar**, because it is a
three-expression change that needs the reply shapes enumerated, whereas the exemplar has to demonstrate a
whole treatment. It is item 1 of the build order. `ui.rs`'s `render` (§3) is the model for the fix: an
exhaustive match whose composite arms *state what arrived* rather than dumping it, plus a gate that walks
every `Value` variant.

### 0.4 `theme.rs` has shipped

The style page's §0 says *"there is no theming code at all"* and *"nothing here has been applied to any
panel"*. Both were true when it was written and neither is now. `theme::install` is wired at
`main.rs:1545` and `theme::dock_style` at `main.rs:1126`, and two tabs (Objects, Planes) already draw
through the shared furniture. The page's §5 sequencing step 1 is done; this page is step 2.

### 0.5 The Objects header is a spacing defect, not a structural one

Confirmed as the brief states it. `objects.rs` defines `Col` once (`objects.rs:110`) and `ui.rs`'s
`slot_table` (`ui.rs:2146`) draws header and body from that one definition, measuring widths with
`Painter::layout_no_wrap` against the face the theme actually installed. There is nothing structural to
fix. What remains is a look call (§4).

**The Profiler is the panel where the brief's structural claim is true**: `ui.rs:1757` writes the header's
five column widths and `ui.rs:1771` writes the body's, as two separate format strings that must agree by
hand.

### 0.6 The suite revision in the brief is an oracle SHA

`fde8d14` is a commit in **this** repo (*"style: P10 no dashes, and the owner's own paragraph as the bar"*),
not in `empyrean`. `git -C ../empyrean show fde8d14:...` cannot resolve. The empyrean commit that filed it
is `87a8d70`, whose subject says so: *"re-file the debug-window style page from oracle fde8d14"*.

---

## 1. The three shapes, and how to choose between them

Everything below assigns each region one of three treatments. They are the vocabulary; nothing else is
invented per panel.

| shape | when | how it is drawn |
|---|---|---|
| **Big-number readout** | The one or two numbers a person opens the tab to read. | `stat` / `stat_row` (`ui.rs:1908`). Value at the 20px `section` face in `text_hi` or a health colour, unit `Small` beside it, label `Small` and recessed **beneath** it. |
| **Labelled fact** | Supporting numbers and short values. | `fact_grid` (`ui.rs:1864`) or `health_grid` (`ui.rs:1961`) when the rows carry health. Label `Small` in `text_lo`, value in `text_hi`. |
| **Column table** | Rows of like-shaped data: slots, breakpoints, watch hits, routines, spaces. | `slot_table` + `Col` + `column_widths` + `table_cell` (`ui.rs:2065` to `2224`). Header row, hairline, zebra bands, numeric columns right-aligned, the unbounded name column last. |

Plus two that are not text:

* **A meter**, for a fact that is genuinely a fraction of something with a threshold on it. `meter`
  (`ui.rs:1990`). **The legend is not optional** and is not `Option` in the type either.
* **A picture**, for a fact with real spatial shape. Already correct on Screen and Planes.

And two that are states, not shapes:

* **A stated absence** (P6). A sentence, never a zero, never an omitted row. Where possible make it
  unrepresentable rather than merely required: `pacing::Audio` is an enum whose `Absent` arm carries a
  sentence and has no numbers in it at all, so no downstream code *can* render zeroes.
* **A refusal** (P4, P5). A whole-panel state, coloured from a carried boolean.

**The label-beneath-the-number order in the big readout is the whole trick.** A reader scanning a row of
these reads the numbers first and drops to a label only for the one that surprised them, which is the
opposite of what a `label   value` line does. That is what "pops" means where there is no graph to draw.

**Emphasis is colour and size and nothing else.** egui selects fonts by family and has no weight axis;
`RichText::strong()` only swaps in `Visuals::strong_text_color()`, which `theme.rs` sets to the family's
`text_hi`. There is no bold here to reach for, so do not write a treatment that assumes one.

---

## 2. The build order

Each item is one parcel. Ordered by value per unit of risk, not by tab order.

| # | parcel | why here |
|---|---|---|
| 1 | `memory.rs:672` `answer_line`, §0.3 | One expression, five surfaces, raw JSON on screen today. |
| 2 | **Pacing** (done, §3) | The exemplar. Owner-named, purely plain data, no controls to preserve. |
| 3 | **Profiler** (§4) | Owner-named. Two hand-agreeing format strings, five padded specifiers in one line, a whole prose clause in monospace, and two spec citations. |
| 4 | **Watchpoints** (§4) | The strongest P2 case in the crate: a virtualised log of seven-column rows drawn by `format!`. |
| 5 | **Breakpoints** (§4) | Same table furniture as 3 and 4; land the generalisation once and reuse. |
| 6 | **Registers** (§4) | Seven-plus label/value rows including the two alarm rows, all one weight. |
| 7 | **Memory** (§4) | Hex dump, gate table, and a truncation bug the padding is hiding. |
| 8 | **Planes** (§4) | One missing legend line. Cheap, and it closes the named failure mode. |
| 9 | **Screen strip and slots** (§4) | Slot occupancy is invisible for nine of ten slots. |
| 10 | **Objects** (§4) | Already the best table in the window. Look call only. |
| 11 | The P10 sweep and the P9 pair (§5) | Mechanical, and better done once than per parcel. |

**Parcels 3, 4 and 5 share one prerequisite**: generalising `column_widths` / `table_cell` / `text_w` /
`cell_face` off `objects::Col`. They already operate on `&[String]` cells and a `Col { head, numeric,
mono }` shape; only `objects::Field` (which `cell_colour` matches on) is object-specific. Lift the colour
decision to a caller-supplied closure and the four functions are generic as they stand.

---

## 3. The exemplar: Pacing (done)

`ui.rs:662` (was), now `pacing::Readout` plus `Panels::pacing`. This is the reference the rest are measured
against, so its structure matters as much as its look.

**What it showed.** Thirteen `ui.monospace(format!(..))` lines, hand-spaced. Broke **P2** (invisibly, §0.1),
**P3** (all thirteen lines are prose in the register face; not one is an address), and **P10** (`device
NONE — pacing is unmeasured, not fine`). It also restated `frames` and `rebases` a second time in the
status line at the bottom with nothing saying they were the same numbers.

**What it became.**

* `pacing::Readout` is the projection: `Stat`, `Fact`, `Meter`, and `Audio` as an **enum**. It holds no
  egui type.
* Three headline stats in a card: frames emulated, pictures drawn, worst late.
* A `governor` section: target period, rebases, early wakes, in a `health_grid`.
* An `audio` section: starved and producer drops as stats, rate/ring/latency as facts, and the ring as a
  meter with the low-water mark ticked and a legend beneath it.
* The status line demoted to a `Small` recessed footnote whose hover says it is the line published for
  `emulator/screen_text` and that its two counts are the same numbers as above.

**The three lessons the other sixteen inherit.**

1. **The projection holds no egui type, so it is testable without a window.** This window cannot be opened
   from an agent seat: the owner's copy is live and launching a second is barred. A panel whose correctness
   lives in its draw calls is a panel nothing can check. Every panel below gets a view type in its own
   module and a thin render in `ui.rs`.
2. **Health is decided beside the number, never inferred from its text downstream.** That is P5's principle
   generalised off refusals. Note `early wakes` is deliberately never a warning however large it gets: an
   early wake is the governor *doing its job*. A counter whose health rule differs from its neighbours' is
   exactly the judgement that must not be re-derived at the draw site.
3. **A threshold drawn on a picture is a claim about behaviour, and must be derived from the behaviour.**
   `the_meters_mark_is_where_the_policy_actually_changes_its_mind` does not compare the mark to a number.
   It asks `frames_to_run` what it actually does either side of the drawn line and requires the line to be
   where the answer changes. Retuning `RENDER_LOW_WATER_FRAMES` cannot leave a tick painted at the old
   fraction.

**Gates:** eight new tests in `pacing.rs`, three of them proven red-first against a quoted mutation.

---

## 4. Panel by panel

Each row: what it shows, how it reads badly, and what it becomes. Line numbers at `a8d87f9`.

### Profiler (`ui.rs:1674`, model `stopping::profiler`)

**Shows.** A `live_head` sentence; arm/disarm with two lens checkboxes; the frames-in-sample divisor; a
five-column table of the hottest routines (addr, cycles, self, stall, calls, name); a "further routines not
drawn" note.

**Reads badly.**
* `ui.rs:1757` and `ui.rs:1771` are **two separate format strings** carrying the same five column widths.
  Five padded specifiers in the body line, the densest in the crate. **P2**, and this is where the brief's
  "header and body disagree" claim is genuinely true.
* `ui.rs:1741` renders a whole prose clause in monospace: `"frames in sample (the divisor
  \`emulator/get_profiler_frames\` uses)   {}"`. **P3**, the worst instance in the window.
* `ui.rs:1700` and `ui.rs:1719` cite `§11.16` and `§11.18` in runtime strings. **P9**, two of the three
  genuine runtime violations in the crate.
* Four runtime em dashes: `ui.rs:1695, 1718, 1752, 1784`. **P10.**
* The name column is silently empty when `symbol` is `None`, rather than stating the absence the way
  `objects::NO_NAME` does. **P6-adjacent.**

**Becomes.** A near-literal reuse of `slot_table` once the table furniture is generalised. Columns: `addr`
(mono), `cycles` / `self` / `stall` (numeric, mono, right-aligned; these are machine cycle counts and keep
the face), `calls` (numeric), `name` (proportional, with a stated absence when unnamed). The
frames-in-sample number becomes a **big-number readout** beside `routine_count` and `open_frames`, because
the divisor is the fact every other number on the tab is relative to. `"hottest routines"` and `"top 24 of
340"` become `section(ui, "hottest routines", Some("top 24 of 340"), "emulator/get_profiler_frames")` in
three weights on one line, which is what `section` exists for. The two `§` references move into the doc
comments of the code that composes the strings; the *facts* they carry (arming resets the sample, disarming
retains it) stay, in the reader's terms.

### Watchpoints (`ui.rs:1444`, model `stopping::watches`)

**Shows.** `live_head`; an add row; a `seen / matched / dropped` line; instrument caveats; armed watches in
a scroll; the retained hit log, virtualised with `show_rows` and `stick_to_bottom`.

**Reads badly.**
* `ui.rs:1639` draws each hit as `format!("#{:<7} f{:<6} {} {:?} {:?} {:#X} pc {}", ..)`. Seven columns of
  like-shaped data with hand-rolled widths. **P2**, and the most exact match in the crate for the owner's
  "rows of like-shaped data".
* `ui.rs:1572` does the same for an armed watch, `{:<4}` plus two `{:?}` Debug-formatted enums.
* `ui.rs:1535` glues three labelled counts into one monospace sentence. **P3.**
* Four runtime em dashes: `ui.rs:1503, 1540, 1561, 1598`. **P10.**

**Becomes.** Two tables. The armed-watch table is small and takes the generalised furniture directly:
`handle` (mono), `space` (proportional, the plain word from `WATCH_SPACES`, never `{:?}`), `range` (mono),
`op` (proportional, "read" / "write" / "read and write" spelled out), `matched` (numeric, popping),
`stopAfter` (a stated "never" when `None`, per P6), `label` (proportional, recessed). The hit log **keeps
`show_rows`** and gets the same columns laid out per row inside the closure with `table_cell`, rather than
one `format!`: precompute the widths once outside `show_rows` so the virtualisation cost does not change.
`seen / matched / dropped` becomes three big-number readouts, since they are the three numbers that make a
negative finding readable and are the reason the tab is open.

### Breakpoints (`ui.rs:1271`, model `stopping::breakpoints`)

**Shows.** `live_head`; a halting alarm when a breakpoint is stopping the machine; an add row; a header
line and one row per breakpoint.

**Reads badly.**
* `ui.rs:1377` is a padded fake header, and `stopping.rs:150` (`BreakRow::summary`) is the padded body.
  **P2**, and this is the case the style page named at the stale line 138.
* The body blob mixes an address (legitimately mono) with the state word, the symbol name and the caller's
  free-text label (all prose). **P3.**
* Eleven runtime em dashes reach this tab, seven from `stopping.rs`'s `Live::sentence` and
  `Halting::headline` and four from the render. **P10.**
* A `None` symbol is omitted rather than stated. **P6-adjacent.**

**Becomes.** Columns: `id` (mono), `addr` (mono), `state` (proportional, and the fact carried by **colour**
rather than by the word alone: `text_hi` armed, `weak_text_color` disabled), `hits` (numeric, mono,
right-aligned, and this is the number that should pop, because it is the evidence a disabled breakpoint
ever fired), `symbol` (proportional, stated absence when `None`), `label` (proportional, recessed, since it
is caller metadata and not the headline fact). The halting alarm moves into a `card` so it has the same
visual weight as the Screen tab's standing readout: it is the same kind of thing and is currently a bare
`colored_label` in a stack of them.

### Registers (`ui.rs:728`)

**Shows.** The status strip (`StatusStrip::rows`, seven or more label/value pairs including the `held` and
`halting` alarm rows), then the 68000 register file in a two-column `Grid`, then a note about A7 and SP.

**Reads badly.**
* `ui.rs:756`: `ui.monospace(format!("{label:<18}{value}"))` over every strip row. **P2** in the named-capture
  form the published check misses (§0.1), and **P3**, since several values are whole sentences: `"none
  loaded (no --symbols, and no .lst beside the ROM)"` is not an address.
* The `halting` and `held` rows are deliberately placed first because they matter most (`ui.rs:2622`), and
  then rendered at exactly the same weight as `frame (emulated)`. The ordering carries the whole emphasis.
* **Its data modelling is the best in the window** and should not be touched: `symbol_count: Option<usize>`
  with a three-way match (`ui.rs:2630`) is exactly the P6 discipline. The defect is entirely presentational.

**Becomes.** `health_grid`, the type the exemplar already added, which is what the alarm rows need: label
`Small` in `text_lo`, value in `text_hi`, and `halting` / `held` in `WARNING` / `ERROR` from a carried
boolean. `romPath` and hex-shaped values keep the mono face; counts and sentences lose it. The register
file below is already a real `Grid` and is correct as it stands.

### Memory (`ui.rs:778`)

**Shows.** A space selector; an address box resolving hex or a symbol; a hex dump; a gated write cell; a
collapsible table of what every space accepts; a `memory_hash` range tool.

**Reads badly.**
* `memory.rs:672`, the live P1 (§0.3).
* `ui.rs:971`: `format!("{:<22} {}  {}", space.label(), if open {"WRITE"} else {"  —  "}, g.why())` over five
  spaces. **P2**, **P3**, and an em dash used as a glyph. **P10.**
* `ui.rs:886`'s hex dump `ScrollArea` has **no `.id_salt`**. **P7**, the only missing one in the crate.
* `note_label` (`ui.rs:2291`) puts every non-refused note in monospace, including prose-plus-address lines
  and the two further raw-`Value` sites below. **P3** and **P10**.
* **Two more raw-JSON sites, found while verifying this page's own citations:** `memory.rs:504`
  (`format!("the reply carried no \`addr\`: {v}")`) and `memory.rs:518` (`format!("{t:?} is not an exact
  name; the server answered a search instead: {v}")`). Both interpolate a whole `serde_json::Value` into a
  string a person reads, and both land in `Resolved::Rejected` on the address box. **P1**, twice, and the
  second is not even a rare path: it fires on every prefix search. So `answer_line` is three sites in one
  module, not one.

**On the hex dump, which is the one place P2's letter is wrong.** A hex dump is the one region where
monospace and column alignment are correct by nature, and P3 explicitly carves it out. But the *mechanism*
at `ui.rs:889` (`"{}  {:<47}  {}"`) is still worse than it needs to be, and it is **hiding a bug**: `{:<47}`
assumes sixteen bytes, so on a truncated final page (`v.truncated_to`, a real path) `row.hex()` is shorter
and the ASCII gutter drifts left of every row above it. **Treatment:** a three-column `Grid` with
`.striped(true)`, each cell individually monospace, the grid doing the alignment. That keeps the "hex dump
is a table of monospace cells" reading, gains zebra banding for free, and removes the drift.

The space-gate block becomes a real table: `space` (proportional), `write` (a `SUCCESS` / `text_lo`
indicator, not `"WRITE"` versus three padded spaces and a dash), `why` (`Small`, `text_lo`). `note_label`
takes an `addr: Option<u32>` on `memory::Line` so the caller composes prose in `Body` with the address
alone in `Monospace`.

### Planes (`ui.rs:496`)

**Shows.** A plane selector, two toggles, the rasterised plane as a square-aspect picture, and a `card` of
six facts plus the scroll notes.

**Reads badly.** **No P-rule is broken.** Zero padded specifiers, zero raw JSON, zero runtime dashes, zero
citations, `id_salt` present. Its `fact_grid` use is the reference pattern.

**One defect, and it is the named failure mode.** The picture draws a **checkerboard** for nibble-0
transparent pixels (`planes.rs:113, 197, 426`), and nothing in the window says so: `git grep -n
'checker|transparent|empty_a|empty_b' -- crates/oracle-player/src/ui.rs` returns zero. A person sees a
checkered region and has no way to know it means "nothing is drawn here" rather than dithered game art.
This is exactly *"what are the purple boxes"*: visual weight without a legend.

**Becomes.** One `Small` `text_lo` line near the picture (not only in the facts card, which can be pushed
below the picture on a narrow panel at `ui.rs:610`): the checkered squares are transparent, palette nibble
zero. One line, and it closes the failure the style page's §3a warns about.

### Screen: the control strip and the save slots (`ui.rs:302`, model `states.rs`)

**Shows.** The mask statement, the glass alarm, a spawn badge, four layer checkboxes, spawn-mode controls,
three aspect buttons, a save-state row, and the standing pick readout in a `card`.

**Reads badly.**
* `ui.rs:397`: `ui.monospace(format!("slot {slot} {}", "(occupied)"))`. **P3**: the parenthetical is prose.
* **Nine of the ten slots' occupancy is invisible.** The comment at `ui.rs:387` says "the occupancy dots",
  plural, but only the current slot's state is drawn anywhere in the crate. A person steps through ten
  slots blind to find a full one. This is "rows of like-shaped data" that does not exist as a list at all,
  and it is a missing *fact*, not a missing style.
* The glass alarm (*"THE PICTURE BELOW IS NOT THE MACHINE'S PICTURE"*) sits at the same visual weight as a
  routine spawn badge, separated only by colour.
* The pick readout is correct and the owner liked it (*"ah it's able to recognize points now"*). Do not
  touch it. `screen_pick.rs:1063` is its own P9/P10 gate and should be the model for the others.

**Becomes.** Ten small cells in one row: slot number in mono, occupancy carried by **fill** (`raised` for
occupied, `surface` for empty) and by the number's colour (`text_hi` occupied, `text_faint` empty), the
selected slot taking `theme::selection()` which the Objects table already uses for exactly this. Click to
select; keep the steppers beside it for keyboard parity. The parenthetical leaves the string entirely. The
glass alarm gets separation beyond colour, which is a look call (§6).

### Objects (`ui.rs:1028`)

**Shows.** The pool layout as a `fact_grid` in a `card`, a players section, the pool table, the rings
block, and one expanded slot.

**Reads badly.** Structurally, it does not. §0.5. It is the best table in the window: one `Col` definition
for header and body, widths measured against the installed face, numeric columns right-aligned, truncation
detected before drawing rather than inferred after, stated absences (`ABSENT`, `NO_NAME`, `NOT_PRESENT`)
with the reason on the hover.

**Becomes.** Nothing structural. A look call on the header spacing (§6), which is what the owner
photographed and the one thing a rendered frame is required to settle.

### Panels menu (`nav.rs:420`)

Not a data panel. Four runtime em dashes in hover text (`nav.rs:255, 256, 257, 439`). Its own test
(`nav.rs:900`) correctly excludes tooltips from an ASCII-glyph rule, which is a different rule from P10 and
does not exempt them. Four string edits, no redesign.

### Build chip and SRAM notice (`main.rs:1036`, `main.rs:1086`)

Two runtime em dashes each (`identity.rs:207, 229`, both reaching the hover; `battery.rs:309`, reaching the
swap notice's hover when a rescue write fails). `identity.rs:385` is a working P9 gate and a model. The
chip's revision hash is the one token worth `Monospace` inside otherwise-dim prose, per P3's "symbol names
as they appear in a listing".

---

## 5. The two sweeps

**P10.** Measured per file, runtime only, comments and tests triaged out by hand. The raw count and the
runtime count are different questions and only the second is work.

| file | raw | runtime | note |
|---|---|---|---|
| `nav.rs` | 76 | 4 | hover text |
| `stopping.rs` | 110 | 9 | `Live::sentence`, `Halting::headline` |
| `ui.rs` render, tabs 1239 to 1770 | 25 | 12 | three stopping tabs |
| `palette.rs` | 60 | 8 | includes the P9 line |
| `symbols.rs` | 22 | 7 | console only |
| `device.rs` | 15 | 6 | stderr only |
| `report.rs` | 11 | 6 | console only |
| `battery.rs` | 25 | 2 | one reaches a hover |
| `identity.rs` | 33 | 2 | both reach the hover |
| `states.rs` | 32 | 1 | console only |
| `memory.rs` | 53 | 6 | includes `"ok — {v}"` |
| `screen_pick.rs`, `planes.rs` | 57, 3 | 0, 0 | already gated |

**P9.** Three genuine runtime citations remain: `palette.rs:309` (`"(D15)"`, verified still live at this
revision and at the same line the style page gave), `ui.rs:1700` (`§11.16`), `ui.rs:1719` (`§11.18`). Every
other `§` hit in the crate is in a doc comment or a test assertion, where the reader *is* holding the
specification and the citation is correct.

**One more P7:** `palette.rs:337`'s `ScrollArea` has no `id_salt`, and `ui.rs:886`'s has none. Those are the
two in the crate.

---

## 6. Parked look calls

**Nothing in this document was seen.** The owner's window is live and launching a second is barred, so
every claim here is from source and every claim about *appearance* is a prediction. These are the questions
a rendered frame has to answer, and they are the reason this is an audit and not a verdict.

1. **Objects header spacing.** The thing the owner photographed and named. The columns are structurally
   sound and share one definition, so the fix is `COL_GUTTER` (14.0), the header's `Small` face, or both.
   *Question: with the theme now installed, do the Objects column names still run together, and is it the
   gap between columns or the header's size that reads wrong?*
2. **Pacing at its dock width.** Pacing sits in the right column at `0.68` split, stacked over Registers
   (`ui.rs:2675`), which is narrow. *Question: do three big-number stats fit on one line there, or do they
   wrap and need to become two rows of two?*
3. **The meter.** *Question: does the ring bar read as informative, or as decoration on a panel that did
   not need it? It is the one visual I invented and the easiest thing here to have got wrong.*
4. **Big number versus stat tile.** *Question: should a headline number be bare accent-coloured text at
   20px, or a small bordered tile? Both satisfy "pops"; they are different commitments and only a frame
   says which reads as clean data rather than overbuilt.*
5. **The glass alarm.** *Question: does "THE PICTURE BELOW IS NOT THE MACHINE'S PICTURE" get noticed in the
   instant it fires, with colour as its only distinction from the badge beside it?*
6. **The slot strip.** *Question: do ten cells fit the Screen control strip at a realistic window width
   without wrapping, and does the strip beat the stepper for someone who already knows their slot?*
7. **The Planes checker, once labelled.** *Question: at typical zoom on a busy commercial ROM, is the
   checker still distinguishable from the game's own dithered art?*
8. **`GOVERNOR OFF (control)`.** Reachable in a normal window via `--target-fps 0`, not only in bench mode.
   *Question: does it deserve a standing warning-coloured badge on the Pacing tab, or is that overkill for
   a flag only a bench operator passes?*
9. **The Watchpoints hit log's frame cost.** `show_rows` exists because the naive `show` measured 15.22 ms.
   *Question (a profiled frame, not a look): does per-cell layout inside the closure stay under budget?*

**Added by the spawn-picker parcel (2026-09-05), same rule: none of these were seen.**

10. **The picker's height in the Screen strip.** The list is a `ScrollArea` capped at `PICKER_MAX_H`
    (140 points) and it sits *above* the picture, so every point it takes is a point the game does not
    get. *Question: at a realistic window size, does 140 leave enough picture, and does the list read as
    part of the strip or as a second panel wedged into it?*
11. **The filter box.** *Question: on a build with a handful of archetypes, does a filter box read as
    clutter that should only appear past some number of rows? It earns itself on a 137-archetype listing
    and this box's fixture has three.*
12. **The auto-pause line's standing weight.** It is a `Small` recessed line in the ordinary case and
    `error_fg_color` when the machine was left moved. *Question: in the ordinary case, does a line that
    says the window paused and resumed the machine read as reassurance or as noise after the tenth
    spawn? The alarming arms must stay; the quiet one is the judgement call.*
13. **The selected row.** Selection is carried by `theme::selection()` fill and nothing else, which is
    what the Objects table does for the same job. *Question: on a monospace list of near-identical
    `ObjDef_` names, is fill alone enough to find the selected row at a glance, or does it want the
    accent on the text too?*

---

## 7. What would make this page wrong

Stated so the next reader can check rather than trust.

* **Line numbers in `ui.rs` rot within hours, and this page nearly shipped rotten.** The style page's
  `ui.rs:1460` was already `2070` when this audit started. Then the four parallel audits behind §4 read the
  crate at one revision, the exemplar landed and moved every line below `662` by up to 215, and the first
  draft of this page carried the pre-exemplar numbers. It was caught by re-running the greps before the
  landing, which is the rule immediately below this one working on the document that states it. Every
  citation here has since been checked against the line it names. **Re-derive with `git grep` and verify
  the line says what you think; never trust a number older than the last commit that touched the file.**
* The counts in §5 were triaged by hand from `/usr/bin/grep -nP '[\x{2014}\x{2013}]'`. The plain `grep`
  here prunes gitignored directories and returns a confident zero, and `ls` is aliased and errors on a path
  argument. Any absence claimed from a decorated tool is worthless without a positive control.
* `cargo clippy` exits 0 while printing lints. Only `-- -D warnings` fires. Everything claimed clean here
  was run in the deny form.

---

## Addendum, 2026-09-06: the window has ten tabs, and this page's stamp is unchanged

*Appended, and nothing above this line is edited. That is the point of the form rather than a courtesy:
this page is stamped to the revision its line numbers were derived at, and every citation above was
checked against the construct it names at that revision. An edit anywhere above would break that promise
for every reader who trusts the stamp; an append moves no earlier line, so the stamp keeps meaning what
it says.*

**What changed.** §0's paragraph on the window's real unit says there are **nine** tabs. There are now
**ten**. `Tab::Spawn` landed at `eddc13b`, *"the spawn picker stops taking the game view's height, and
becomes its own tab"*, on the owner's own reversal of the rule that had put the picker in the Screen
tab's control strip:

> *"the placement works well it seems! it just takes up a lot of space haha. Maybe it should be its own
> debug tool in the right panel instead of part of screens?"*

**The move, and what did not move with it.** The picker's rows are now `Panels::spawn` in `Tab::Spawn`,
and **arming and placing are untouched**: the click that places an object is still on the picture, in the
Screen tab, through the same `screen_pick::Panel::click`. What deliberately stayed in the Screen strip is
the pair that must be seen without going to look for it, the spawn badge and the run-state line, because
a standing statement that can be behind another tab in a dock leaf is not standing. `palette.rs`'s
tabs-versus-controls rule was amended in place rather than quietly broken: the rule is about **one-shot
gestures**, and a picker is a standing list you read.

**The stamp above is unchanged and still governs everything before this heading.** The page was landed at
`ec896b6` and its citations were re-derived at `86aa307` (2026-09-05), with `5452ba4` recording that the
picker had been built. Nothing in this addendum re-dates any of that. Two consequences for a reader:

* **Every line number above is still as of `86aa307` and several have since rotted**, which is §7's own
  first rule arriving on schedule. Two measured here rather than asserted: §0 cites `ui.rs:68-93` for the
  tab enum, which today spans `68` to `108`, and `Tab::ALL` is now `ui.rs:120` and declares `[Tab; 10]`.
  **Re-derive before citing any of them**, exactly as §7 says.
* **The tab count is the only claim this addendum corrects.** Nothing else above has been re-measured at
  this revision, so absence of a correction here is not a statement that a line still holds.

---

## Addendum, 2026-09-06: the Planes picture takes a click, and it answers a different question from the Screen tab's

*Appended under the same rule as the addendum above, and for the same reason: nothing before this heading
is edited, so the stamp keeps meaning what it says.*

**What changed.** `PLANES-PANEL-PICK` landed. The plane picture is drawn with `Sense::click()` instead of
`Sense::hover()`, and a click puts a standing four-part readout in a `card` at the **top of the side
column**, above the facts card §4's "Planes" entry describes. §4's description of that tab is now short by
that card; everything it says about the six facts, the scroll notes and the `fact_grid` reference pattern
still holds.

**The one thing about it that is a correctness claim rather than a layout choice.** The queue row asked
for *"the same answer clicking the game picture already gives"*, and that framing is wrong in a way that
two correct numbers would not fix:

* the **game picture**'s click asks *which layer won at this dot* (attribution, through scroll, priority
  and sprite order);
* the **plane picture**'s click asks *which cell of this map is this, and what word is in it* (a
  nametable read, on a plane drawn whole).

They legitimately disagree: the cell you click here can be off screen, covered by a sprite, or ranked
under the other plane. So the readout carries a sentence of its own about the screen, derived from
`planes::covered_mask` — the same mask the viewport outline is drawn from, so the words and the outline
cannot disagree — and it says on the covered case, once, whose question the other one is. What the two
surfaces may never disagree about is the nametable word, and that is asserted by
`the_viewer_and_pixel_attribution_agree_about_the_word` rather than assumed.

**No new number treatment.** §6's parked call 4 (bare accent text at 20px versus a small bordered tile)
is still parked and this parcel did not pre-empt it: the readout is prose at `strong_text_color`, a
`Small` recessed sentence, a monospace `weak_text_color` detail line, and `theme::WARNING` caveat lines.
Every one of those weights is already in use on the Screen tab's pick readout, which is the surface this
one is deliberately shaped after.

**§4's one named Planes defect is still open.** The checker legend — *"the checkered squares are
transparent, palette nibble zero"* — was not added by this parcel and is not affected by it. A click on a
transparent cell now reports its tile like any other, which is correct and is not a legend.

### Parked look calls, added by the plane-pick parcel (2026-09-06), same rule: none of these were seen

The owner's window was live, launching a second is barred, and a headless framebuffer answers a different
question while looking like an answer. Continuing §6's numbering.

14. **Whether the click registers at all.** The picture lives inside a `ScrollArea::both`, and the sense
    changed from `hover` to `click` under it. *Question: does a click on the plane picture produce a
    readout, and does the wheel still scroll a plane larger than the pane?* This is the same structural
    hole the queue already carries for the picker rows: an agent seat cannot press anything.
15. **The pick at a non-1.0 `pixels_per_point`.** `screen_pick::dot_at` takes `ppp` explicitly and is
    tested at 1.0, 1.5 and 2.0, and it inverts `Response::rect` rather than re-deriving the fit — but no
    test in this crate has ever inverted a rect that a scroll offset moved. *Question: on the owner's
    display, does the named cell match the cell under the cursor, at the top of the scroll and after
    scrolling a 128-cell plane sideways?*
16. **Four parts in a 260-point column.** The side column is allocated at 260 points wide when the pane is
    at least 560. The head is a full sentence, the screen line is another, and the detail line is
    monospace and long. *Question: does the readout read as one answer, or as a wall that pushes the six
    facts off the bottom?*
17. **The caveat's weight when it fires.** The armed-H-interrupt caveat is a `Small` `theme::WARNING` line
    and the picture's own `unestablished` note is already one, so two warning-coloured paragraphs can
    stand in the same column at once. *Question: do two of them read as one alarm and get skipped, or does
    the reader take them as the two different statements they are?*
18. **Whether the readout should sit under the picture instead.** It goes in the side column, which the
    narrow layout pushes **below** the picture. *Question: at a realistic dock width, is your click's
    answer where your eye already is?*
