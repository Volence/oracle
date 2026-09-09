# The command palette's shape, and the strip above the picture

**Scoping only. No code was written for either half of this document, deliberately.**

Two items from the owner's own usability pass of 2026-09-09
(`empyrean origin/main:docs/2026-09-09-oracle-ux-owner-capture.md`) are questions rather than defects,
and each is written up here instead of being fixed:

* **§1, the palette's shape.** He liked the previous one. Deliverable: what the old one did that this one
  does not, and what I would build. An agent must not spend a parcel building a palette on the strength
  of this document; it exists so that the parcel, when it is opened, starts from an enumeration rather
  than from a memory.
* **§2, the strip above the picture.** *"the box above screen kind of looks bad too imo."* That is a pure
  look call and it is **his**. What I would change is written down and nothing was changed.

The five items that WERE fixed in the same pass are five commits on this branch and are not repeated
here.

---

## 1. The palette

### 1.1 His words

> "commands should open with tilde like the other one but I have to press this button and this pops up
> and there's no easy thing or way to use it imo. I really liked the command palette of our previous one."

Two asks. The tilde binding is shipped (`crates/oracle-player/src/palette.rs`, `TILDE_SHORTCUT`, added
beside `Ctrl+P` and the toolbar button, none of which were removed). The shape is this document.

### 1.2 What each surface actually is

**The old one** — `crates/oracle-frontend/src/commands.rs` (the registry) and
`crates/oracle-frontend/src/palette.rs` (the state machine).

* **42 registry rows**, 31 of them visible: 16 hand-written, 7 generated one per `LensId::ALL`, 4
  generated one per `LayerMask::targets()`, 4 audio-only; plus 11 hidden rows that bind a key without
  taking a line in the list (the `F1` reset alias and the ten direct slot keys).
* Every row is a **complete gesture with a human title**: `Pause / resume`, `Soft reset (SRAM kept)`,
  `Reload ROM from disk + reset`, `Dump watch hits to terminal`, `Hide / show sprites`. Choosing a row
  does the thing. Nothing is typed except the filter.
* Rows are **grouped** under seven headers in a fixed display order — `GAME`, `SAVE STATES`, `WATCH`,
  `LENSES`, `DISPLAY LAYERS`, `SPAWN OBJECTS (DEBUG, NOT SAVED)`, `SETTINGS` — so the list answers *what
  can this thing do* by category and not as one alphabet.
* A **hotkey column**, from the same rows, so the palette teaches the keyboard rather than competing
  with it. The registry is the single source of both: adding a command yields the palette entry and the
  binding together, and `hotkeys_unique` catches a collision.
* The filter is a case-insensitive **subsequence** match on the title (`ssl` finds `Save state to current
  slot`), hand-rolled, no crate.
* **Keyboard-complete**: `Up`/`Down` move the selection, `Enter` runs, `Esc` closes, typing filters. The
  mouse is never required.
* **Pickers.** A row can open a *second* list with its own title and its own filter — `Select save
  slot…`, `Open ROM…` (the folder listing, with the running image marked). `Esc` backs out to the command
  list and restores the outer query. This is how the old palette handled the arguments it had.
* Opened with **backtick or `Ctrl+P`**; backtick again or `Esc` closes. The game keeps running behind it.
* Pure state machine — `PaletteKey` in, `PaletteAction` out, no window, no I/O — and therefore fully
  tested headless.

**The current one** — `crates/oracle-player/src/palette.rs`.

* One list of **all 63 served methods**, borrowed from `oracle_aether::engine::METHODS` and filtered by a
  substring over the wire name and the registry summary.
* A row is a **wire name plus its summary**, and clicking it **loads the name into a text box**. It does
  not run.
* Two text boxes and a button: `method`, `params` (a JSON object typed by hand), `Run`.
* A refusal or a reply is rendered verbatim underneath, with a `remedy` keyed on `error.data.reason`.

### 1.3 The gap, itemised

This is the answer to "what did the old one do that this one does not". Each line is a thing that was
present and is now absent, not a preference.

| | old | current |
|---|---|---|
| a row is | a complete gesture | a name to be copied into a box |
| running one takes | one keystroke | a click, a JSON object, and a button |
| rows are named | in prose a person reads | in wire spelling (`emulator/write_memory`) |
| rows are organised | seven groups, fixed order | one flat list of 63 |
| keys are | taught, in a column, from the same table | absent |
| driven by | the keyboard end to end | the mouse, plus typing JSON |
| arguments come from | a picker over a bounded set | a JSON object the person composes |
| filter is | subsequence over the title | substring over name and summary |
| closing it | backtick or Esc | the window chrome |

**The one thing the current surface does better, and it must survive any rebuild:** its list is
*derived* from the registry rather than written down, so a method added to the engine appears on the next
build and one removed disappears. The old registry was a hand-written table and could go stale silently.
That property is not negotiable, and it is the constraint the design below is built around.

**Why this happened is not carelessness.** The current palette was built to make *every served
capability reachable from the window* — the standing default that a capability on the bus is reachable in
the window. Sixty-three methods with arbitrary parameters admit exactly one uniform surface, and that is
the one that shipped. The old palette had 42 hand-chosen gestures and no parameters at all. Neither is
wrong; they answer different questions, and he wants the answer to *"let me do the thing"*, not the
answer to *"enumerate the bus"*.

### 1.4 What I would build

**Two layers over one derived registry, with the gesture layer in front.**

1. **A gesture table.** Hand-authored rows in the old shape: a human title, a group, an optional key, and
   what it calls. Perhaps 25 to 35 of them, drawn from the methods the owner actually reaches for. This
   is the layer he is asking for.

   ⚑ **The table must be checked against `METHODS`, not trusted.** Every row names a method, and a test
   walks the table asserting each name is served by this build. That keeps the old shape's readability
   without buying back its silent staleness: a method renamed in the engine is a red test, never a row
   that quietly does nothing. This is the single most important line in this document.

2. **Generated rows, exactly as the old registry generated its own.** One per `LayerMask::targets()`, one
   per save slot, one per spawn archetype the current listing publishes. Generated rows cannot go stale
   by construction, and this is where most of the count comes from.

3. **Pickers for bounded arguments.** The old palette's `Picker` is the piece that made a parameterised
   command a gesture: a second list with its own filter, `Esc` backing out. The arguments worth one are
   the ones with a discoverable set — the save slot, the layer name, the archetype, the ROM file, an
   armed breakpoint. Those cover nearly every argument the owner types today.

4. **The raw form stays, one keystroke away.** Thirty gestures cannot cover sixty-three methods and
   should not pretend to; the method-and-JSON form is the honest fallback for the rest and is already
   built and tested. It becomes the palette's second mode rather than its only one, reached by a row
   (`Raw command…`) that is itself in the table. Nothing is deleted.

5. **Keyboard end to end**, because that is what "easy to use" meant in his sentence: `Up`/`Down`,
   `Enter`, `Esc`, typing filters, tilde toggles. Subsequence matching, lifted from
   `commands::subseq_match` — the function is 8 lines and its behaviour is what makes `ssl` find the save
   command.

6. **Group headers and a key column**, from the table, for the old one's reason: the palette is where a
   person learns the keys.

**Shape of the work.** The gesture table and the picker are the parcel; the renderer is mostly the list
furniture this crate already has. The one genuinely new decision is where a *gesture* lives, given that
`crate::ui::Tab` is for things you look at and this window already has controls for pause, step, reset,
ROM-open and spawn — the palette must not become a fourth spelling of those, it must *call* them.

**What I would NOT build:** a widget-per-parameter form generator. It means reading the vendored schema
at runtime for types, ranges and the `oneOf` alternatives half these methods carry, it has its own
staleness question, and the picker covers the arguments that matter for a fraction of the cost. This was
already the current palette's stated non-goal and it remains right.

---

## 2. Parked: the strip above the picture

> "Also the box above screen kind of looks bad too imo"

**Changed nothing. His call.** What follows is what I would propose if he asks, and why.

### 2.1 What it is

`Panels::screen_controls`, drawn above the picture in the Screen tab's vertical stack. In the order it
draws: the mask statement, the glass alarm, the spawn badge, the effects statement, the `layers:` row of
four checkboxes, the run-state line, the aspect row with the armed-watch count, the save-state slot row,
and the states note. **Nine rows, six of which appear and disappear** depending on what is on.

### 2.2 Why it looks bad, as a mechanism rather than a taste

* **It is a flat column of nine unrelated things.** Controls (checkboxes, buttons, slot stepper) and
  standing statements (mask, alarm, badge, effects, run state) are interleaved with no separation, so
  nothing tells the eye which lines it can click.
* **Its height is not stable.** Six of the nine are conditional, so the picture's top edge moves as facts
  come and go — the same complaint as his toast, one level up. Two of the six left the strip in this
  pass (the readout moved onto the glass, and the armed statement is now drawn there as well), so the
  strip is already shorter than the one he screenshotted, but the mechanism is untouched.
* **It spends the game view's own pixels**, which is the argument that already moved the spawn picker out
  of it into `Tab::Spawn` on his own instruction.

### 2.3 What I would change

1. **One control row, fixed height, always present**: aspect | layers | state slot. Three groups on one
   line with separators, so the strip a person clicks never changes shape.
2. **The statements come off the strip entirely and onto the glass**, where `armed_notice` and the click
   readout now are — the mask statement, the glass alarm and the effects statement are all claims about
   *this picture*, which is the argument that put the other two there. Colour and position carry the
   distinction between an alarm and a note.
3. **The spawn badge comes out of the strip**, because after this pass it is drawn on the picture too,
   and a standing statement drawn twice in one tab is one too many. This is the cheapest single change
   and it is the one I would do first.
4. **The states note and the run-state line stay off the picture**, because they are about the *machine*
   rather than about the picture, and go in a single-line status area under the control row.

That lands at two fixed rows above the picture instead of a column of up to nine, with nothing
conditional in the layout.

### 2.4 Why it is parked and not done

Because he called it a look, and because it overlaps the toolkit rebuild (d-25) he has already approved:
the stacked tab strips, the panel clutter and the drag-to-any-side request all land there, and doing the
strip here means paying for it twice. The five items fixed in this pass were fixed because they are wrong
regardless of what draws them. This one is a judgement about what a surface should look like, and the
person who has to look at it has not made it yet.
