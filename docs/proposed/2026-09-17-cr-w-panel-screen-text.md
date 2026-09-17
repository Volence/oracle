# CR-W, `emulator/screen_text` reads the debug window's panels: one `panel` kind, named by its tab, harvested from what the toolkit actually painted

**Raised by:** oracle lane, 2026-09-17. Closes the queue row `F-PANELS-INVISIBLE-TO-SCREEN-TEXT`
(`docs/2026-09-06-queue-detail.md:71`; `docs/lane-status.json:86`, state `next`). Drafted by a research parcel that
changed no code, ran no emulator and could not open the window.
**Target:** `contract/protocol.md` **§11.50** (next free: the last heading is §11.49, and `git grep '11\.50'` finds
nothing in empyrean `origin/main` or oracle `HEAD`). **§6 amended in two places:** the `emulator/screen_text` blockquote
gains two bullets, and its row's result summary gains `panel?`. **Schema:** one new `$defs` entry, the surface item gains
one enum value, one key and an `if`/`then`/`else`, and four description strings change. **One droppable rider (R1)** adds
one `initialize.capabilities` key. One optional §8 item (31).
**Reviewer:** to be named by the adjudicator in the ruling, per the 2026-08-27 substituted-reviewer rule.

**Revisions everything below was read at.**
- **Contract:** empyrean `origin/main` **`610d0c70b3414dffd66995a8f707c5f8eaa6aae4`**, printed by `git rev-parse` after
  `git fetch`, read only as `git show origin/main:<path>`. The brief's premises were measured at `fedb881a`;
  `git log fedb881a..origin/main -- contract/` is empty, so every contract line cited here holds at both.
- **Server:** oracle `349298d` (this branch's base). **Toolkit:** `egui-0.36.1`, `epaint-0.36.1`, `egui_dock-0.21.1`
  from `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`, the versions `crates/oracle-player/Cargo.toml:30-32`
  pins.
- **Siblings swept**, each at its pushed ref: aeon `origin/master` `d6ec0c111`, sigil `origin/master` `bb9acf605`,
  aurora `origin/master` `e760b693b`, seraph `origin/main` `6688e12b0`, empyrean `origin/main` `610d0c70b`. Dominion
  (`36f606833`) and oracle-old (`1eb09a989`) have no remote; their committed `HEAD` was swept.

**How the letter was found.** `git grep -n -E 'CR-W\b' <ref>` finds nothing in empyrean, oracle, aeon, aurora, sigil or
seraph at the refs above. The same command with `CR-V\b` finds 30 lines in empyrean and 70 in oracle, so the grep can see
a hit. The closest string, `CR-WV-1..3` in `docs/2026-08-27-write-vram.md`, is a different family and does not match
`CR-W\b`. **CR-W.**

---

## 0. What the adjudicator is asked to rule on

| Part | What it does | Can be taken alone? |
|---|---|---|
| **Is it a CR at all** (§3) | Tests §11.47 against this change. Finding: the kind, the name key and the reading rule are a CR; three adjacent items are not. | Asks for a ruling on the reading. |
| **Core** | `kind` gains `panel`; a `panel` surface carries a REQUIRED `panel` key (its tab title); `text`/`rendered` of a panel are defined against what was painted, row- and run-aligned. | Yes. |
| **Core, normative text** | Which panels are reported (drawn bodies only), the empty-panel rule, the scrolled-out rule, the no-join extension, and an unknown-`kind` rule for clients. | Needs the core. |
| **Rider R1** (droppable) | `initialize.capabilities.screenTextKinds`: the kinds this deployment can put on the glass. | Yes. |
| **§8 item 31** (optional) | A game-agnostic recipe for "a panel surface exists exactly when its body was drawn". | Yes. |
| **Booked, not proposed** | Tooltips and popups (no kind); a "the list continues below" signal; reading a hidden tab. | Asks for finding ids. |

**No existing reply changes** for any client under any part: a reply with no panel drawn is byte-identical to today's
(vector case 2). The lockstep is oracle's own serve with its re-vendor, the standing adoption condition.

## 1. Where the booking, the brief and this repo's own earlier decision are wrong or incomplete, first

1. **This repo already decided NOT to report panels, in writing, and this CR reverses that.** The player's own module
   doc (`crates/oracle-player/src/screen.rs:10-37`) refuses panels on three arguments. The reversal has to answer each,
   not step past them:
   - *"A snapshot listing all eleven would report text nobody can see."* **Agreed, and kept.** The CR reports drawn
     bodies only (§5.2). It was an argument against *all eleven*, not against *the drawn ones*.
   - *"What a panel body actually reveals depends on the pane's pixel height and its scroll offset, computed inside
     egui's painting loop. Restating them here is precisely the drift `oracle-frontend`'s rule 2 forbids."* **True of
     restating, and this CR restates nothing.** It reads the painted output after the fact (§2.3): the clip rectangle,
     the elision flag and the glyphs are the renderer's own inputs, so there is no second layout to drift. That seam
     did not exist in the design space the module doc considered.
   - *"Six of the eleven panels have another reader anyway."* **True of the values and irrelevant to the complaints.**
     The queue row's cost is "ask the owner to look", and what he is asked about is how a panel READS: a cut cell, a
     hollow box, a wrong sentence, an empty state, a refusal worded badly, the click readout's words. No served row
     carries any of that. The values stay on their rows, and §5.2 makes reading them off a panel a named error.
2. **The booking says "not what any panel says". Nearly true.** `statusLine` already carries text that lives in panels'
   neighbourhood: `ui.rs:5647-5968` (`Transport`, the top bar) draws 11 of the file's text sites, and they are served
   today (§2.1). Nothing inside a dock pane is.
3. **Oracle's own register already classes panel-like text as needing NO contract change.** `docs/OVERSEER-LOG.md:2057`
   (`F-SCREEN-TEXT-PALETTE-LENS`) says `palette` and `lens` are *"additive and need no contract change"*, and
   `screen.rs:59` says *"The player's lenses are its eleven panels."* So serving panel bodies as `kind: "lens"` today
   is a reading this lane has already written down. §3 tests it and finds it permitted but inadequate.
4. **The fragment's `rendered` description is already false for a served surface.** The schema says `rendered` is *"a
   prefix of `text` today"*; `oracle-frontend`'s toasts end a cut message in `…` (`crates/oracle-frontend/src/screen_text.rs:63-66`),
   which is not a prefix. Harmless because it says "today", but a panel cell elided by `egui::Label::truncate` does the
   same, so the core rewords it (§5.3).
5. **The table furniture's truncation test is a prediction, and the CR must not use it.** `table_cell`
   (`ui.rs:4229`) computes `cut = text_w(..) > w` BEFORE drawing, to decide whether to attach a hover. The brief calls
   that "detects truncation before drawing". It is a second derivation of what `Label::truncate` then does, and nothing checks that the two
   agree (both are widths measured separately). The toolkit's own answer is `Galley::elided` (`epaint-0.36.1/src/text/text_layout_types.rs:751`),
   read after layout. `truncated` for a panel derives from the painted glyphs (§5.3), never from `cut`.

## 2. Recon, measured from the tree

### 2.1 How the panels draw text

All eleven panel bodies are dispatched from one function, `impl egui_dock::TabViewer for Panels`'s `ui`
(`crates/oracle-player/src/ui.rs:256-277`), a `match` over `Tab` with one arm per variant (`Tab::ALL`, `ui.rs:135`, 11
entries). No other production file draws panel text. A per-file sweep of `crates/oracle-player/src/*.rs`, each cut at
its first `#[cfg(test)]`/`#[cfg(all(test` line and grepped for egui usage, found egui in `ui.rs` (403 lines), `input.rs`
(29), `rom_open.rs` (23), `palette.rs` (22), `machine.rs` (12), `planes.rs` (9), `screen.rs` and `theme.rs` (6 each),
`main.rs` and `screen_pick.rs` (1 each); a second grep for text-drawing calls in the non-`ui.rs` files found them only in
`palette.rs` and `rom_open.rs`, whose drawing is two modal windows (`palette.rs:360` and `rom_open.rs:660`, both
`egui::Window`, not dock tabs). `main.rs`'s cut at its line 540 falls before `build_ui` (`main.rs:1115`), which draws
the top bar and is already served as `statusLine`.

**The production cut.** `ui.rs`'s first top-level test module is `mod transport_tests` at line 5970 (printed by
`grep -n '^mod \|^#\[cfg' crates/oracle-player/src/ui.rs`); every top-level item below it is a test module, which the
file's own gate `the_owner_is_the_only_production_reader_of_the_fit` asserts. Lines 1-5968 were copied to a scratch file
and counted.

**The counting command** (a line counts once however many calls it holds; comment lines excluded):

```sh
RE='\.label\(|\.monospace\(|\.strong\(|\.weak\(|\.heading\(|\.small\(|\.colored_label\(|\.button\(|\.small_button\(|\.selectable_label\(|\.checkbox\(|RichText::new\(|Label::new\(|Button::new\(|\.on_hover_text\(|\.on_disabled_hover_text\(|painter\(\)\.text\(|\.collapsing\(|CollapsingHeader::new|TextEdit::|ComboBox::'
sed -n "$START,${END}p" ui_prod.rs | grep -v '^\s*//' | grep -cE "$RE"
```

| Region of `ui.rs` production (lines) | Text-drawing lines |
|---|---|
| **Whole file, 1-5968** | **307** |
| `Transport` / top bar, 5647-5968 (already served as `statusLine`) | 11 |
| Tab dispatch and picture fit, 232-574 | 1 |
| **Ad hoc inside the `Panels` methods, 591-2836** | **212** |
| &nbsp;&nbsp;effects 63, watchpoints 25, spawn 21, memory 21, screen_controls 18, breakpoints 18, objects 14, subtypes 10, profiler 10, rings 8, `live_head` 3, planes 1, screen 0, pacing 0, registers 0 | |
| **Inside shared free helpers, 2837-5646** | **83** |
| &nbsp;&nbsp;`plane_side_column` 14, `stat` 6, `readout_card` 6, `plane_choice_controls` 5, `table_cell` 5, `subtype_list` 5, `fact_grid` 4, `health_grid` 4, `pacing_tab` 4, `preview_card` 4, `section` 4, `effects_list` 4, `note_label` 4, `registers_tab` 3, `meter` 3, `select_list` 3, `header_cell` 2, `plane_image` 2, `slot_cells` 1 | |

The helper rows were produced by running the same command over each function's line span (the span from its `fn` line
to the line before the next top-level `fn`). `pacing` and `registers` show 0 because they delegate whole to
`pacing_tab` and `registers_tab`. Separately counted with `painter.galley`/`layout_job`: `overlay_block`
(`ui.rs:3585-3626`) paints the Screen tab's on-picture readout straight to the painter as galleys, and `slot_cells`
(`ui.rs:4348`) calls `painter.text`, so two of the text paths are not widgets at all.

**Call sites of the helpers** (same scratch file, comment lines and the `fn` line excluded,
`grep -cE "(^|[^a-z_:.])<name>\("`): `card` 16, `section` 11, `note_label` 6, `fact_grid` 5, `health_grid` 4,
`stat_row` 4, `table` 4, `control_table` 2, `slot_table` 2, and one each for `hex_table`, `gate_table`, `readout_card`,
`preview_card`, `select_list`, `subtype_list`, `effects_list`, `registers_tab`, `pacing_tab`.

**What the numbers decide.**
- **A helper seam covers 83 of 295 panel-side text lines (28%).** The helpers that know any row/column structure
  (`table_cell` 5, `header_cell` 2, `fact_grid` 4, `health_grid` 4) are **15 lines**. Everything else, 212 lines in the
  panel methods plus the non-structural helpers, is ad hoc.
- **So capturing at the helpers is not "a handful of sites"; it is a sweep of ~212 ad-hoc sites plus two painter
  paths**, and every future `ui.label` added to a panel silently escapes it. That is the shape this repo has paid for
  before (a guard that covers only what its author thought to vary).
- **A structured (rows/cells) form could be structured for tables and fact grids only.** The other ~80% of the text has
  no structure to report, so option (c) would ship two shapes inside one surface (§4).

### 2.2 Which panels are drawn on a given present

- **`egui_dock` runs one body per leaf, and none for a collapsed leaf.** `egui_dock-0.21.1/src/widgets/dock_area/show/leaf.rs:1330`:
  `if !collapsed && let Some(tab) = tabs.get_mut(active.0)`. A hidden tab (not the active one in its leaf) and a
  collapsed leaf never reach `TabViewer::ui`.
- **Fully collapsed is reachable:** `crates/oracle-player/src/layout.rs:276` prints `fully_collapsed` and
  `collapsed_leaves` for a saved layout. A window can be drawing zero panel bodies.
- **Inside a drawn body, off-view widgets usually paint nothing.** `egui::Label` paints only
  `if ui.is_rect_visible(response.rect)` (`egui-0.36.1/src/widgets/label.rs:284`), `Button` likewise
  (`widgets/button.rs:373`). A folded `CollapsingHeader` (the Objects tab's *"where these addresses come from"*,
  `ui.rs:1978-1990`) runs no body.

### 2.3 The seam this recon found: the painted output, not the widgets

Everything a panel puts on the glass as text passes through one data structure egui exposes: the **paint list** of
the layer the body draws into.
- `Context::graphics` gives read access to the pass's `GraphicLayers` during the pass
  (`egui-0.36.1/src/context.rs:1044`); `GraphicLayers::get(layer_id)` returns its `PaintList`
  (`layers.rs:204`); `PaintList::next_idx` and `all_entries` are public (`layers.rs:121`, `:186`); `ShapeIdx` is a
  public `usize` newtype (`layers.rs:109`). So `TabViewer::ui` can note `ui.layer_id()` and the paint list's
  `next_idx()` before a body and after it, and the shapes between are **exactly that tab's**, with no geometry guess.
  (`Plugin::output_hook`, `plugin.rs:44`, sees the whole `FullOutput` after the pass if the in-pass read proves awkward.)
- Each entry is a `ClippedShape` carrying its **clip rectangle**, and a text entry is an `epaint::TextShape` whose
  `galley` carries: `Galley::text()` = *"the full, non-elided text of the input job"* (`text_layout_types.rs:1012`),
  i.e. the SOURCE; `rows[].glyphs[].chr` = the characters laid out (`Row::text`, `:951`), i.e. the RENDERED run,
  elision mark included; `elided` (`:751`); and each glyph's `uv_rect`, which is the exact input `screen::Glyphs`
  already compares to find hollow boxes (`screen.rs:128-240`, `Glyphs` at `:166`).
- The test module `pacing_headline_tests` already walks `ClippedShape` → `Shape::Text` → `galley.text()`
  (`ui.rs:7411-7431`) to assert what reached the screen. The instrument exists; it has never been pointed at the wire.

**What this buys, stated as the recon's finding rather than a design preference:** one hook at `TabViewer::ui` covers
all 296 non-top-bar lines (295 panel-side plus the dispatch line), the two painter paths, every future `ui.label`, and TextEdit contents and hint text, and it
answers "which panels", "what is clipped", "what was elided" and "which glyph is a box" from the renderer's own inputs.
**Unverified by this seat and named for a spike (Q3):** that each body's text lands contiguously in `ui.layer_id()`'s
paint list (popups, combo boxes and tooltips go to their own `Area` layers, which is the intended exclusion) and that
the in-pass read sees the shapes before `end_pass` drains them.

## 3. Is this a CR? §11.47 tested

§11.47 ruled that a run added INSIDE an existing surface (`statusLine`) is not a CR, because *"Nothing in the fragment
constrains which runs a surface may contain, and adding one is not a change in shape."* Tested against each piece:

| Piece | CR? | Why |
|---|---|---|
| **A panel reported as a new surface** | **Yes, as proposed.** | A `panel` value is outside the closed enum (`bus-protocol.schema.json:7090-7096`, vendored copy, byte-equal to upstream by `cmp`). §11.29's CR-H vectors pin the enum closed with a red case whose `why` reads *"Adding a surface must be a contract edit, not a silent drift."* |
| **The same, spelled `kind: "lens"` with no name** | **Not a CR, by §11.47's reasoning.** | The fragment never defines `lens`; oracle's own prose identifies the player's lenses with its panels (`screen.rs:59`) and its register calls lens serving additive (`OVERSEER-LOG.md:2057`). **But it is inadequate:** the surface item is closed (`additionalProperties: false`, schema `:7080`), so no key can say WHICH panel, and a client would have to read the panel's identity out of `text`, the parse §2.4 rule 3 and §11.29's no-join clause exist to prevent. It would also pre-empt this ruling. **Not recommended; named so the hub can rule it out explicitly.** |
| **A name key** | **Yes.** | Closed item. |
| **"Only drawn bodies", "scrolled out is not truncation", row/run alignment** | **Yes, as normative text.** | They define what `text` and `rendered` mean for the new kind. §11.47 extended the no-join clause by ruling even though no fragment moved; these are that kind of text. |
| **The no-join clause reaching panels** | **Ruling, not shape.** | §11.47's precedent: *"the part that needed a ruling rather than a nod"*. |
| **Serving the player's command palette window as `palette`** | **Not a CR.** | An enumerated kind for exactly that surface. Oracle's to build; out of this CR's scope. |
| **Fixing `F-PLAYER-SCREENTEXT-CLIP`** (the top bar's `rendered == text`) with the same paint harvest | **Not a CR.** | Server behaviour within §11.29's existing definition of `rendered`. |
| **Harvesting from the paint list at all** | **Not a CR.** | Implementation; it keeps CR-H §7's snapshot-per-present rule (§6.6). |

**Finding: part of the surrounding work is not a CR (the last three rows), and the whole of the ask is not avoidable
without one.** The only no-CR route to panel text, `lens` with no name, loses the one fact a reader needs first.

## 4. Shape options, with costs

| Option | Wire | Cost | Verdict |
|---|---|---|---|
| **(a) one `panel` kind + a REQUIRED `panel` name** | enum +1 value; item +1 key, tied by `if`/`then`/`else` | Enum widening is not additive for a validator closed over the old set (protocol.md's own sentence at the `entryKind` paragraph, `:2075`); only oracle vendors the schema (§7). Cell-level truncation is located by diffing two aligned strings, not by a typed flag per cell. | **Recommended.** |
| **(a2) as (a), one surface per visual LINE** | same | A drawn panel with no text yields no surface, so "on screen and blank" and "not on screen" become the same artifact, the exact defect §11.29 refused an empty list for. The 64-surface cap (`engine.rs:784`) is exceeded by one Objects table; raising it makes result-level `truncated: true` reachable on a method with no params to narrow by, which §2.4 (b) says a client answers by narrowing. | Lost. |
| **(b) one kind per tab** (`panelObjects`, …) | enum +11 today | Puts one window's tab vocabulary into the contract: every new tab is an amendment, and `layout.rs`'s `LAYOUT_VERSION` churn becomes contract churn. `oracle-frontend` has no tabs, so the values are meaningless to the other embedder. Buys nothing (a) does not: a client filters on a string either way. | Lost (vector 9 pins it red). |
| **(c) structured rows/cells** (`rows[][]`, or `runs[]{text,rendered,truncated,unrenderable}`) | new nested shape on one kind | §2.1: structure exists at 15 lines of 295; the other ~80% would be one-cell rows, i.e. two shapes in one surface. Per-cell `unrenderable` and `truncated` multiply JSON by roughly the cell count. A generic client that prints every surface's `rendered` stops working for this kind. `runs[]` is additive LATER for clients (not for stale validators, §7) if a consumer proves the aligned-string diff insufficient. | Lost now; booked as the named upgrade path (§12). |
| **(d1) `lens` + no name** | none | §3: identity only by parsing text. | Lost; permitted. |
| **(d2) `lens` + a name key** | item +1 key, no enum change | Avoids the enum widening. But `lens` would then carry two reading rules: the frontend's fixed overlays, whose `text` is the whole composed readout, and a scrolling pane, whose `text` is only the painted runs (§5.3). The kind is where a client learns how to read `text`. | **Strongest runner-up** (Q1). |
| **(d3) a separate method** (`emulator/panel_text {panel}`) | new method, params | It can narrow (§2.4 b), but it cannot serve a hidden tab without composing a body on demand, which CR-H §7 refuses by name; so it serves the same drawn set in a second non-atomic call, splitting "what does the window say" in two. | Lost. |
| **(d4) the helper seam instead of the paint list** (a shape-neutral alternative) | none | §2.1: 212 ad-hoc lines unreached, silent escape for every new label, and `cut` is a prediction (§1.5). | Lost; the paint list is the seam. |

## 5. The proposal

### 5.1 Schema (exactly what was patched and measured in §8)

```json
"$defs": { "screenTextKind": {
  "enum": ["statusLine", "toast", "palette", "lens", "titleBar", "panel"],
  "description": "The closed set of emulator/screen_text surface kinds (§11.29, widened by §11.50 CR-W). One definition, referenced by the surface item and by initialize.capabilities.screenTextKinds, so the two lists cannot drift." } }

"emulator/screen_text".result.properties.surfaces.items:
  "properties": {
    "kind":  { "$ref": "#/$defs/screenTextKind", "description": "Which surface. `titleBar` is drawn by the window manager, not the overlay, which is why the method is not named overlay_*. `panel` (§11.50, CR-W) is one debug-window panel whose body was drawn this present; its `panel` key names it. A client that meets a value it does not know MUST keep the other surfaces and MAY show the unknown one's `rendered` as plain text (§11.50)." },
    "panel": { "type": "string", "minLength": 1, "description": "Present if and only if `kind` is `panel`: the panel's name as its own tab bar draws it (itself text on the glass). Free text, NOT an enum: the contract does not carry one window's tab vocabulary (§11.50, CR-W)." },
    "text":     { "description": "The SOURCE string the player composed. For `panel`: the sources of the runs with at least one glyph on the glass, runs on one visual row joined by U+0009 and rows by U+000A, a run's own TAB/LF folded to a space. NOT the panel's whole content: the served row the panel renders is (§11.50)." },
    "rendered": { "description": "What is actually on the glass after the player's fit, elision or clipping. Not necessarily a prefix of `text`: an elided run ends in the toolkit's elision mark. Compare to `text` for the honest reading; `truncated` is a convenience, not the guard. For `panel`, row k / run j of `rendered` is the glass rendering of row k / run j of `text` (§11.50)." },
    ... },
  "if":   { "properties": { "kind": { "const": "panel" } }, "required": ["kind"] },
  "then": { "required": ["panel"] },
  "else": { "not": { "required": ["panel"] } }
```

(`text` and `rendered` keep `"type": "string"`; only their descriptions change.) The fragment's `$comment` gains
*"and the debug window's drawn panels (§11.50)"* after *"palette/lens readouts"*. The §6 row becomes
`` `surfaces[]{kind,panel?,text,rendered,truncated,unrenderable[]}` ``, `*(§11.29, §11.50)*`.

### 5.2 §6 text, drafted verbatim (two bullets appended to the `emulator/screen_text` blockquote)

> - **Panels** *(added §11.50, CR-W)*. A presenting host whose window docks debug panels reports each panel whose
>   **body it drew on this present** as one `panel` surface, named by `panel`: the title that panel's own tab bar
>   draws. A panel whose body was not drawn (a tab behind another in its pane, a collapsed pane) has **no** surface,
>   because this method reports the glass and a hidden panel's text is not on it. A drawn panel with no text on the
>   glass is **present** with `text` and `rendered` both `""`, never omitted, so *not on screen* and *on screen and
>   blank* stay different artifacts. Panel surfaces follow every surface drawn behind them, in the order the window
>   drew the panels. For a panel, `text` is the source of every run (a label, a cell, a button caption, a text box's
>   contents or hint) with at least one glyph on the glass, and `rendered` is the glyphs the toolkit laid out for those
>   runs after elision and clipping, the elision mark included. **Both strings are joined identically:** runs on one
>   visual row, left to right, by U+0009; rows, top to bottom, by U+000A; a run's own TAB or LF is folded to a space in
>   both. So row *k* run *j* of `rendered` is the glass rendering of row *k* run *j* of `text`, and a client finds the
>   cut run by comparing them. The joins are the server's and are not text on the glass. **Rows scrolled out of view
>   are in neither string and are not truncation:** a panel surface is what the panel shows, never the panel's content.
>   A client that wants the content calls the served row the panel renders.
>   **The no-join clause applies to panels with full force.** Almost every panel is a rendering of a bus row, which
>   makes it the most tempting join this method has: a number read off a panel is a snapshot of a rendering, taken at
>   a present, possibly cut by the window's fit, and never a source. Ask the bus.
> - **A kind a client does not know** *(added §11.50)*. A client MUST NOT fail a reply over an unrecognised `kind`. It
>   keeps every other surface, and MAY show the unrecognised one's `rendered` as plain text. (A validator closed over
>   an older `$defs/screenTextKind` will refuse the reply; that is §8 item 20's closure doing its job on a stale copy,
>   and the remedy is re-vendoring, not a version bump. §11.50's compatibility note.)

### 5.3 Why each rule is the one written

- **Drawn bodies only.** §11.29's first normative bullet: `rendered` exists because source-only *"is structurally
  blind to the whole defect class, reporting text that is not on screen as though it were"*. A hidden tab's text is
  exactly that. `screen.rs:10-25` makes the same argument and this CR keeps it.
- **`text` is the source of the PAINTED runs, not of the panel.** For `statusLine` and `toast`, `text` is one composed
  message, and everything cut from it is truncation. A panel has no single composed message; the only honest source
  set is the runs that reached the glass, because egui does not paint (and so does not lay out, `label.rs:284`) the
  rest. Calling an Objects table's unscrolled rows "truncated" would make `truncated` true on nearly every panel reply
  and mean nothing. So the scroll case is carried by prose and by pointing at the served row, and Q4 books the typed
  signal this leaves out.
- **Row/run alignment instead of structure.** It is the cheapest form that answers "which cell was cut" without a
  second shape (§4 c). The TAB/LF fold costs `text` a byte-for-byte claim to be the source for a run that contains its
  own TAB or LF; without it, a wrapped multi-paragraph label would break every row index after it. Q7 offers the
  hub the other side.
- **A wrapped run stays one run.** `rendered` joins a run's glyph rows without inserting breaks the source did not
  have, so a label that wraps but loses nothing has `rendered == text` and `truncated: false` (conformance row W4).
  Wrapping is not truncation.
- **REQUIRED name, free text.** Required because a panel surface without identity is (d1). Free text because an enum
  is (b). The tab title is itself glass text (`ui.rs:149-170`, `Tab::title`, one function feeding both the tab bar and
  the nav), so the key reports a string on the screen rather than an internal id.

### 5.4 Rider R1 (droppable): `initialize.capabilities.screenTextKinds`

```json
"screenTextKinds": { "type": "array", "items": { "$ref": "#/$defs/screenTextKind" }, "uniqueItems": true,
  "description": "§11.50 (CR-W) rider R1: the emulator/screen_text kinds THIS DEPLOYMENT can put on the glass, order not significant; [] on a headless server. Absent means the server predates the key, never 'none'. Lets a client tell 'no panel is drawn' from 'this server does not report panels'." }
```

**Why:** a reply with no panel surface means "every pane is collapsed" on the player and "panels are not reported"
on `oracle-frontend` or on a pre-CR-W player (vector case 2 is both). That is the blank-versus-absent defect one level
up, and `initialize` is where D5 puts deployment facts. It also answers a question already open without it: the
frontend serves `titleBar`, `statusLine`, `toast` and the player `titleBar`, `statusLine`, and a client today cannot
tell whether a missing `toast` means "no toast" or "never toasts". **Cost:** one per-deployment list (§11.46: the
handshake is process-scoped, and this is a process property). **Drop it if** the hub prefers clients to read
`implementation`/`serverBuild`, which says who answered but not what that build can draw.

### 5.5 Optional §8 item 31

> 31. **A panel surface exists exactly when its body is drawn** *(§11.50, CR-W)*. Game-agnostic recipe, on a presenting
>     host with at least two panels in one pane: read `screen_text`; exactly one of the pane's panels has a surface.
>     Make the other active (a window gesture); read again; the names swap. Collapse the pane; neither has a surface and
>     the reply still succeeds. *Anti-vacuity:* the first read must carry at least one non-empty panel `text`, or the
>     host reports no panels at all and the swap proves nothing.

## 6. Interaction with the existing rules

1. **The reply cap.** `MAX_SCREEN_SURFACES = 64` (`engine.rs:784`) is a server policy constant, not contract. Per-panel
   granularity adds at most one surface per drawn leaf: 11 with every tab in its own leaf (`--dock every-tab`,
   `ui.rs:5524`). Title, status line and 11 panels is 13, under 64 with room; no cap change. **Reply bytes** are bounded
   structurally by the glass: text only exists where glyphs fit on the window. No new policy bound, so §2.4 (a) gains
   no obligation.
2. **`unrenderable[]`.** Unchanged in rule, better in mechanism: measured per glyph from the `uv_rect` the painted
   galley will sample, in the font that actually laid it out, rather than per run family (`screen.rs:265-306`). Still
   computed over `text`, so a box in an elided tail is still reported, the frontend's choice
   (`oracle-frontend/src/screen_text.rs:75-80`). The three-state rule (`screen.rs:247-253`: only a measured `false`
   reaches the list) carries over unchanged.
3. **The no-join clause.** Extended by §5.2's last sentences. §11.47's third reason (the bus describing itself) does
   not apply; its first two (rendered, snapshot) apply more strongly than to any existing surface, because panels
   render bus rows by design and `section` (`ui.rs:3892`) even prints the method name beside the numbers.
4. **`truncated` versus `rendered` for a table cell cut with a hover.** Settled by CR-H's own two-string design, not by a
   new rule: `text` holds the whole cell (the galley's source), `rendered` holds the glyphs with `…`, `truncated`
   derives true in the handler (`engine.rs:3939-3942`, *"Derived, never carried"*). The hover is a separate `Area`
   layer, on the glass only while the pointer rests there, and is not part of the panel surface; its content is the
   cell's `text` anyway. `table_cell`'s pre-draw `cut` is not consulted (§1.5).
5. **`noDisplay`.** Unchanged. No window, no panels: the refusal already covers it, and CR-W adds no reason string.
   A window with every pane collapsed succeeds with no panel surface (case 2), which R1 disambiguates.
6. **Snapshot per present, never compose on demand** (CR-H §7). Kept: the harvest reads what this present painted and
   is pushed through the same `Host::set_screen_text` seam at the same point (`main.rs:1011-1016`). It runs no panel
   body a second time and touches no `System`.
7. **`F-FIRST-PRESENT-REFUSAL`** (`docs/OVERSEER-REFERENCE.md:1251`). A separate contract question: what a presenting
   host that has not yet presented should answer. **This CR's shape does not depend on it.** Whatever it rules, panels
   ride the same first push; the refusal or its replacement happens before any surface of any kind exists. Named, not
   solved.

## 7. Compatibility and the consumer sweep

**Method.** `git -C <repo> grep -n -E 'screen_text|screenText|emulator_screen_text|"statusLine"|titleBar|unrenderable' <ref> -- . ':!*.schema.json'`
at each ref in the header, hits read in context.

| Repo @ ref | Hits | Consumer? |
|---|---|---|
| aeon @ `d6ec0c111` | 9 | **No.** Docs (`docs/OVERSEER-LOG.md:191-204`, `docs/OVERSEER-REFERENCE.md:383`); the `set_screen_text` in `docs/superpowers/specs/2026-07-02-screens-hud-design.md:241` is aurora's screens-HUD tool, an unrelated name. |
| sigil @ `bb9acf605` | 8 | **No.** All `unrenderable` in `ledger_gate` (sigil's own ledger lines). |
| aurora @ `e760b693b` | 2 | **No.** A decisions line and a scratch UX plan; neither calls the method. |
| seraph @ `6688e12b0`, dominion `36f606833`, oracle-old `1eb09a989` | 0 | No. Oracle-old's MCP bridge (`linux-port/mcp/`) has no `screen_text` tool, so no model reads this method today. |
| empyrean @ `610d0c70b` | 67 | The contract, its 6 `screen_text` vectors (`contract/schema/tests/vectors.json`) and docs. No client code: its Python and TypeScript clients do not name the method. |
| **oracle** @ `349298d` | code in 13 files | **The only consumer, and the only tree vendoring the schema** (§11.49 found the same). Lockstep, all oracle's: re-vendor; `oracle-player/src/main.rs:2512-2513` asserts `total: 2` and `returned: 2` and moves when panels appear; `oracle-player/src/screen.rs` tests assert `snapshot` returns 2; `oracle-aether/tests/screen_text.rs`'s CR-H vector test stays green (its `pausedBanner` red is still outside the enum). `oracle-frontend` produces a subset of kinds (`screen_text.rs:46-50`) and is untouched. |

**What a client that does not know `panel` must do:** §5.2's second bullet, keep the other surfaces. **A client
validating against a vendored schema closed over five kinds** refuses a reply carrying `panel` (the same mechanism
§11.49's registration measured: 17 of oracle's wire tests went red on a re-vendor alone). The sweep found one such
client, oracle's own, which lands serve and re-vendor together. **Versioning:** none. D5 bumps `protocolVersion` only
for envelope changes; discovery is R1 (or `methods` plus `implementation` without it).

## 8. Vectors, with the verdicts measured

`docs/proposed/2026-09-17-cr-w-panel-screen-text-vectors.json`, **16 cases** in CR-H's shape (full documents, stamp
inline). **Provenance:** every `screen_text` document is **hand-built**; the column heads are
`stopping.rs:169-194`'s, the no-label marker `stopping.rs:346`'s, the tab titles `Tab::title`'s, and case 5 is CR-H's
live refusal copied. CR-H's bar applies: real replies replace cases 1-4 before the kind is served.

**How they were run.** empyrean `610d0c70`'s `contract/protocol.md`, `bus-protocol.schema.json`, both document schemas,
`tests/vectors.json` and `tests/validate_contract_schema.py` (plus `docs/AURORA_REGIONS_SCHEMA.md`, which it checks)
were taken by `git show` into a scratch tree. The upstream gate ran first. Two patched schemas were built by script
(**core**: §5.1; **core+R1**: §5.1 and §5.4). Each case was judged against all three with the validator's own
`standalone`/closure composition (Draft 2020-12, `jsonschema` 4.26.0); case 5 (an error document, which empyrean's G3
has no arm for, as with CR-H) against `$defs/errorObject`. Then each variant's gate was run whole with its cases
appended (core: the 10 non-error core cases; core+R1: all 15 non-error cases).

**Whole-gate results:** upstream **GREEN** (126 pass, 187 red, 77 closure); core **GREEN** (130 / 193 / 81);
core+R1 **GREEN** (132 / 196 / 83).

| # | Case | Expect | upstream | core | core+R1 |
|---|---|---|---|---|---|
| 1 | title, bar, Registers, Breakpoints with one elided cell | pass | RED (`'panel' is not one of` the enum) | pass | pass |
| 2 | every leaf collapsed: title and bar only | pass | pass | pass | pass |
| 3 | a drawn panel with no text, `""` | pass | RED (enum) | pass | pass |
| 4 | typed U+6F22 in the add box, `unrenderable` names it | pass | RED (enum) | pass | pass |
| 5 | the `noDisplay` refusal, unchanged | pass | pass | pass | pass |
| 6 | `panel` kind, no `panel` key | fail | RED (enum) | **RED** (`'panel' is a required property`) | RED |
| 7 | `panel` key on `statusLine` | fail | RED (enum, on the other surface) | **RED** (`else`/`not`) | RED |
| 8 | `panel: ""` | fail | RED (enum) | **RED** (`should be non-empty`) | RED |
| 9 | option (b) spelling `panelBreakpoints` | fail | RED (enum) | RED (enum) | RED |
| 10 | option (c) `rows` on a panel | fail | RED (enum) | **RED** (`'rows' was unexpected`) | RED |
| 11 | panel without `truncated` | fail | RED (enum) | **RED** (`'truncated' is a required property`) | RED |
| 12 | R1: player, `["titleBar","statusLine","panel"]` | pass | pass | pass | pass |
| 13 | R1: headless, `[]` | pass | pass | pass | pass |
| 14 | R1: duplicate kinds | fail | pass | pass | **RED** (non-unique) |
| 15 | R1: `["tooltip"]` | fail | pass | pass | **RED** (enum) |
| 16 | R1: `true` | fail | pass | pass | **RED** (not array) |

**What the columns prove, and what they do not.** Cases 1, 3 and 4 are refused upstream and admitted by the patch, so
the patch is what admits the kind. **The core reds (6-11) are also red upstream**, for a weaker reason (the enum): unlike
CR-V's reds, they do not prove on their own that the patch produced them. The core column's messages do: each is the
rule the case names, not the enum. R1's reds pass without R1, which is why they are tagged and drop with it. Cases 12
and 13 pass upstream because `capabilities` is open and item 20's closure is top-level only (`protocol.md:2429-2438`),
so they prove nothing about admission and are kept as documentation of the two meanings of the list.

**Deliberately not vectors** (server behaviour a schema cannot see): the conformance rows below.

## 9. Conformance rows for the implementing parcel

- **W1, drawn set.** With two tabs sharing a leaf, exactly the active one has a surface; switching swaps them;
  collapsing the leaf removes both and the reply still succeeds (§8 item 31's recipe, in-process).
- **W2, name.** Every `panel` value equals `Tab::title` of a tab whose body ran this present.
- **W3, alignment.** `text.split('\n').len() == rendered.split('\n').len()`, and per row the `'\t'` counts are equal,
  on every panel surface of a live reply, including one with an elided cell.
- **W4, no false truncation.** A panel whose runs are all unelided and unclipped has `rendered == text`, including one
  label long enough to wrap and one source string containing a TAB (after the fold). *Anti-vacuity:* the wrapped label
  must be shown to occupy two glyph rows.
- **W5, elision.** A table cell narrowed until `Galley::elided` is true yields `truncated: true`, and its run in
  `rendered` ends in `…`. The oracle is `elided`, never `table_cell`'s `cut`.
- **W6, clipping.** A run half outside its clip rectangle appears in `text` whole and in `rendered` as the visible
  glyphs; a run wholly outside appears in neither.
- **W7, boxes.** A TextEdit holding U+6F22 names it in `unrenderable`; the same panel with `A` names nothing (the
  control `screen.rs`'s own glyph test already uses).
- **W8, blank.** A drawn panel that paints no text is present with `""` (the Screen tab with a picture and its strip
  collapsed to nothing is the candidate; if no real panel reaches it, say so rather than build a fake one).
- **W9, R1.** The player advertises `["titleBar","statusLine","panel"]` (plus `palette` if it has served it by then);
  headless `oracle-aether` advertises `[]`; `oracle-frontend` advertises what `screen_text.rs::Kind` holds. Each list is
  derived from the producer's own kind type, not typed twice.
- **W10, cost.** The harvest's time per present is measured under `--bench` with `--dock every-tab` (the crate's own
  instrument for panel cost), gated on `is_serving` as the push already is. Recorded, not asserted against a guess.

## 10. Implementation sketch (a map, not a patch)

- **`crates/oracle-aether`:** `ScreenSurfaceKind::Panel` (`engine.rs:1660-1682`), `ScreenSurface` gains
  `panel: Option<String>` with a constructor that makes the kind/name pairing unrepresentable when violated (the
  `PacingFacts` enum precedent, `engine.rs:1718`); the handler emits `panel` when present. R1: a capability built
  from the producer's declared kinds (§11.46: process-scoped).
- **`crates/oracle-player/src/ui.rs`:** in `TabViewer::ui` (`:256`), note `ui.layer_id()` and the paint list's
  `next_idx()` around the `match`, and push `(Tab, layer, start, end)` into a per-present list the loop owns.
- **`crates/oracle-player/src/screen.rs`:** a `panels(ctx, spans, probe)` beside `snapshot`, walking
  `ctx.graphics(|g| g.get(layer))` entries in each span: per `TextShape`, source `galley.text()`, rendered glyph `chr`s
  inside the clip rectangle, `elided`, per-glyph box test through the existing `Glyphs` references; group runs into rows
  by glyph-row top, sort by left edge; fold TAB/LF; join. The module doc's refusal paragraph (`:10-37`) is rewritten to
  §1.1's answers. Q3's spike comes first.
- **`crates/oracle-player/src/main.rs:1011-1016`:** append the panel surfaces after `snapshot`'s two; move
  `:2512-2513`'s pinned `2`.
- **Re-vendor** from the ruling commit; replace vector cases 1-4 with real replies.

## 11. Open questions (not guessed)

- **Q1, hub: new kind or `lens` + name?** §4 (d2). The recommendation is a new kind because the reading rule for `text`
  differs; if the hub reads §11.29's `lens` as "debug readouts, however drawn", (d2) is the smaller wire change and the
  §5.2 bullets move to `lens` with a name key.
- **Q2, controller, a LOOK (tagged): does the harvested text match what the owner sees?** No seat can open the window.
  Once built, one panel reply set beside his screen (a cut cell, a scrolled table, a collapsed pane) is the check. Only
  he can do it.
- **Q3, spike before building (not a look):** does each tab body's text land contiguously in `ui.layer_id()`'s paint
  list, and is it readable before `end_pass` drains it? If not: `Plugin::output_hook` on `FullOutput`, attributing by
  clip rectangle inside each body's recorded rect. Options, not guesses; the shape of this CR does not change either way.
- **Q4, hub (booked, not proposed): a typed "the panel continues beyond its view" signal.** `F-UXB-22` is this defect
  class on the frontend palette (*"the list scrolls and looks complete when it is not"*). Options: (i) nothing, the served
  row is the content (this CR); (ii) a per-panel `clipped: boolean`, which needs every `ScrollArea` in the body
  instrumented (`egui_dock` owns the outer one, `leaf.rs:1390`, and does not hand its output to `TabViewer`). Suggested
  finding id: `F-PANEL-SCROLL-UNSTATED`.
- **Q5, hub (booked): tooltips and popups.** A hover (including `table_cell`'s truncation hover) and a combo-box list
  are text on the glass while open and have no kind. Suggested id: `F-SCREEN-TEXT-TRANSIENT-AREAS`. The player's command
  palette and open-ROM windows need no CR (`palette`), and are oracle's to serve.
- **Q6, owner (tagged): should an agent be able to read a HIDDEN tab?** This CR cannot, by design: it would need a
  method that changes the window under him (activate a tab) before reading. The queue row's complaints are about
  panels he is looking at, so they are drawn; if he wants the other kind, that is a separate CR with his say-so.
- **Q7, hub: the TAB/LF fold.** Alternative: no fold, and alignment holds only for runs without their own TAB/LF, with
  W3 relaxed. The fold is recommended because a guarantee that holds "usually" is not one a client can build on.
- **`F-FIRST-PRESENT-REFUSAL`** stays open and independent (§6.7).

## 12. What would make this recommendation wrong

- **Q3 fails in both forms**, so panel text cannot be attributed to a tab from the painted output. Then the seam falls
  back to the helpers plus a sweep of ~212 ad-hoc sites (§2.1), the cost this CR is priced against, and (a2) per-line or
  a narrower "tables only" scope might be the honest first step.
- **A consumer needs per-cell structure more than aligned strings.** If the first two panel complaints an agent works
  from need a cell's column name or per-cell `unrenderable`, (c)'s `runs[]` is the better shape, and taking it later
  costs a second re-vendor (only oracle vendors).
- **The hub reads `lens` broadly** (Q1). Then (d2) wins on wire size.
- **The harvest is expensive on the frame budget** (W10) in a way `is_serving` gating does not contain. Then a
  "harvest only while a client has asked within N presents" gate reopens CR-H's mid-session-connect argument
  (`host.rs:413-418`) and needs its own answer.
- **A consumer outside the seven swept trees switches on `kind` with a closed match that panics on an unknown value.**
  §5.2's rule tells it what to do; the sweep cannot see out-of-tree scripts.
