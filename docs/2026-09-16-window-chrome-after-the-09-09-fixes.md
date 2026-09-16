# The debug window's chrome, one week after the 2026-09-09 pass

**What this is.** `OWNER-UX-GROUP-B`'s re-derivation and its captures. The board sent this row out as
*three of the owner's ten complaints remain*. **Two of the three were fixed on 2026-09-09 and are on disk
at this parcel's base commit `c13e97b`.** This page is the evidence for that, the one thing the parcel
changed, and the two look calls that are genuinely still the owner's to make.

Everything quoted from the owner is verbatim from `empyrean` `origin/main:docs/2026-09-09-oracle-ux-owner-capture.md`.

---

## 0. The naming defect, first, because it is in the row's own id

The board calls this row **`OWNER-UX-GROUP-B`**. In the capture these three items are the hub's **Group A**
(§3, *"Group A, already covered by a decision he has taken"* — the clutter, the stacked tab strips, the
drag request). The capture's **Group B** is a different and disjoint list: spawn mode stealing clicks, the
toast shifting the picture, the subtype labels, the selection highlight, the Objects headers, tilde, the
palette shape.

So the row id names the wrong group. Neither name is adopted here; the items are cited by capture section
(§1.1, §1.4, §1.7) throughout, which is the only citation that cannot drift.

---

## 1. The premise, checked. It does not hold.

The board's reason for the row being takeable is that an earlier seat *"wrongly parked them as things the
window rebuild would fix"*. That sentence is true about **2026-09-09 at 13:29** and stops being true
**27 minutes later**.

`docs/lane-log.jsonl` has both entries:

| at | headline |
|---|---|
| `2026-09-09T13:29:21Z` | Three of his ten complaints were parked on a rebuild that has already happened, and dragging panels already works. |
| `2026-09-09T13:56:29Z` | **The panel menu now says the tabs can be dragged, because they always could — and the bulk-close button is gone.** |

The board's clause was built on the **first** entry and the work landed in the **second**. Two commits
carry it, both ancestors of this parcel's base:

* `ad7bc78` *nav: the panel menu says the tabs can be dragged, because they always could*
* `4f31f0d` *dock: the close-all button leaves every tab strip, and the drag settings get written down*

### §1.7 — *"I think being able to drag these would be nicer"*

**Live, and not new.** `main.rs` sets `.draggable_tabs(true)` and
`.allowed_splits(egui_dock::AllowedSplits::All)` explicitly (`crates/oracle-player/src/main.rs:1311-1312`),
and did so by upstream default before that. The discoverability answer — the thing that was actually
missing — shipped as `nav::ARRANGE_DRAG` (`crates/oracle-player/src/nav.rs:199-201`), drawn under an
`arranging` heading in the `panels` menu, pinned by
`nav.rs:1104`'s `the_menu_says_the_tabs_can_be_dragged_and_offers_the_way_back_beside_it`.

⚑ **This does NOT inherit the Wayland blind spot, and that is worth saying because the ROM file-drop did.**
The 2026-09-09 ops line (`docs/OVERSEER-REFERENCE.md:739`) is about `winit`'s `DroppedFile`, which the
Wayland backend never emits. A tab drag is not a platform DND event: `egui_dock` drives it from
`ctx.is_being_dragged` over ordinary pointer input (`show/leaf.rs:409-411`), and a tab dropped clear of the
drop icons becomes an **`egui::Window` inside the same viewport**, not an OS window
(`show/window_surface.rs:23-29`, `create_window`). So nothing in §1.7 rests on a platform event that
Wayland withholds. **What is still not covered:** every shot below is X11 on a private Xvfb, so this is an
argument from source, not a measurement on his compositor.

### §1.4 — *"If multiple are open it's really hard as well"*

The capture describes *"three tab strips … each with its own collapse arrow and close button"*.

* **The close-all button is gone.** `.show_leaf_close_all_buttons(false)` (`main.rs:1357`). Shot 1 confirms
  it on the glass: no control at any strip's right edge.
* **The collapse arrow stays and is now named** — `nav::ARRANGE_COLLAPSE` — deliberately, because it is the
  one existing remedy for the complaint (fold the strips you are not reading) and `Tree::set_collapsed`
  being `pub(crate)` makes the arrow its own only undo.

### §1.1 — *"all the pannels are kind of difficult to navvigate/use"*

The navigation half was answered earlier still, by the `panels` menu (`nav.rs`, merged `541c872`
2026-09-04): one list naming all eleven tabs, open **and** close, with a reset row. Shot 3 shows the
button surviving at 421 points of window width.

The *typography* half of "cluttered" — dense hex, run-together headers, long prose between tables — is
`DATA-DISPLAY-AUDIT`'s row (`docs/2026-09-05-debug-window-audit.md`), not this one, and nothing here
touches it.

---

## 2. The one thing this parcel changed

**A number in our own source was wrong by roughly half, and it is the number the remaining look call turns
on.** `main.rs`'s chrome block said `egui_dock` draws *"the per-tab `x` on the active tab"*.

It does not. `show/leaf.rs:431` computes

```rust
let show_close_button = self.show_close_buttons && closeable;
```

**inside** `for tab_index in 0..tabs_len` (`:402`), with no reference to `is_active`. The ✕ is on
**every** tab. `nav.rs:69` had it right the whole time — *"`egui_dock` draws a ✕ on every tab
(`DockArea::show_close_buttons` defaults to `true`)"* — so the crate asserted both things and the wrong
one sat at the decision site.

Read as written, removing close-all left one arrow and one ✕ per leaf: four leaves, **eight** controls.
What `initial_dock()` actually draws is one arrow per **leaf** and one ✕ per **tab** — 4 leaves, 11 tabs,
**fifteen** controls, eleven of them the ✕. Count them in shot 1.

The comment is corrected in place, with the screenshot cited as the witness. **No behaviour changed**: the
right answer to fifteen controls is a look call and it is the owner's, below.

---

## 3. The captures

All three taken on a **private Xvfb at `:79`**, geometry no real monitor has, `WAYLAND_DISPLAY` and
`XDG_SESSION_TYPE` unset, `XDG_DATA_HOME` redirected inside the worktree so the owner's
`~/.local/share/oracle-player/app.ron` was neither read nor written, and no socket (`--aether`,
`--socket` and `ORACLE_SOCKET` all withheld — the status line in shot 1 says *"not serving"*). Each run
printed `display ownership CONFIRMED` from `main.rs`'s own `--expect-screen` guard before drawing.

⚑ **`xdotool` is not installed on this machine, so no click or drag was synthesised for any shot.** Every
shot is the default layout as it opens. Nothing below is a claim about what a gesture does.

| shot | file | what it shows |
|---|---|---|
| 1 | `img/2026-09-16-window-chrome-1443x907.png` | The default layout. Four tab strips, no close-all anywhere, **eleven ✕ glyphs**, four collapse arrows. Three strips stacked in the right column — the §1.4 condition, still true. |
| 2 | `img/2026-09-16-window-chrome-911x709.png` | The same layout in a 911-point window. The right column is 32 % of it (~291 pt): every strip's titles are cut mid-word (`Profile…`), and the Pacing tiles are cut (`61.0…`). |
| 3 | `img/2026-09-16-window-chrome-421x601.png` | 421 points. The `panels` button still on the glass — `nav`'s *"its label is on the glass at all times"* holds here, though by its **position** in the bar rather than by any rule, since the bar clips everything after `pause`. |

---

## 4. The two look calls, which are his

Neither is coded. Both have a stated recommendation.

### 4.1 Eleven ✕ glyphs — remove the per-tab close button?

`DockArea::show_close_buttons(false)` is one line. The 2026-09-09 parcel weighed this control against three
criteria and kept it: **bulk-destructive** (no — it closes one panel), **unlabelled where used** (yes — no
tooltip, pointing-hand cursor only), **capability available and labelled elsewhere** (yes — the `panels`
menu closes by name with hover text). It failed one of three and stayed.

**Recommendation: remove it.** The argument that has changed since is arithmetic, not taste: it is eleven
controls, not four, and its one surviving justification — being the *ordinary* close — is exactly what the
`panels` menu already is, by name, with an undo (`reset to the default layout`) the ✕ does not have.
`egui_dock` also keeps middle-click-to-close on the tab title independently (`show/leaf.rs:589`), so the
gesture does not disappear with the glyph.

**Against:** it is one click versus two, on the panel already in front.

### 4.2 Three tab strips stacked in the right column — the half `4f31f0d` deliberately left

That commit's own message separates the item into two halves and takes one: *"What he is looking at is part
dock chrome (ours, via the `DockArea` builder) and part `ui::initial_dock`'s choice of … vertically-stacked
leaves (also ours). They have different answers and only the first is taken here."* The second half was
never taken and is the live remainder of §1.4.

`ui::initial_dock` (`ui.rs:4935-4969`) splits the picture off at 0.68 and then stacks **three** leaves down
the right column: `[Pacing, Spawn, Effects]`, `[Registers, Memory, Objects]`, `[Breakpoints, Watchpoints,
Profiler]`. Each stacking argument in that function is individually good and the sum is what he
photographed.

**Recommendation: put it to him with shot 2 rather than guess.** Each candidate — fewer leaves with more
tabs each, a wider right column, a second column instead of a third row — trades a different thing he
cares about, and the function's existing comments show every pairing was already argued from his own past
instructions. This is the case the brief reserves for him.

---

## 5. Left open

* **`F-NAV-COLLAPSED-LEAF` prose vs its booking.** `nav.rs` described it as *"real and still open"* while
  `docs/decisions.jsonl` d-31 was answered `leave-it` on 2026-09-09T23:18:15Z and the id is absent from
  `docs/lane-status.json`'s queue. Corrected here to say what is true of each: the **limitation** stands,
  the **booking** is closed pending the owner actually hitting it.
* **Content clipped horizontally in a narrow pane** (shot 2). Whether that is this row's or
  `DATA-DISPLAY-AUDIT`'s is a boundary call, and the boundary this parcel was given puts panel-internal
  layout on the audit. Not taken, named here so it is not re-found.
* **Nothing was confirmed by a gesture.** No click, no drag, no menu opened. `xdotool` is absent and this
  parcel did not invent a way around it.
