# Oracle debug window — token mapping and panel spec

**Status:** draft for the hub to file beside `empyrean design/CHROME_SPEC.md`.
**Scope:** `crates/oracle-player` (egui/eframe/egui_dock). This page is the contract; **applying it is a
later parcel.** Nothing here has been applied to any panel.

**Sources, read at committed revisions** (never through a sibling working tree):
`empyrean` `origin/main` = `ea0f316` (2026-09-05) — `design/tokens.json` (`$meta.version` 0.2.0),
`design/CHROME_SPEC.md`, `contract/projects.json` → `projects[10].styleRequirement` on `ORACLE-DEBUG-UI`.
This repo read at `main` = `4135ced`.

---

## 0. The opening claim, and its one correction

**egui can consume `tokens.json` cleanly, and the design contract already names the artifact.**
`tokens.json`'s `$meta.description` says the file is *"One file, consumed three ways: a generated ImGui
style for Oracle, CSS variables for Aurora and Seraph (via @empyrean/ui), and later tokens.rs for
oracle-next."* This repo **is** `oracle-next`, renamed to `oracle` on 2026-08-19. The third branch is us,
and this page is its specification.

**The correction that belongs in the same breath:** the *"generated ImGui style for Oracle"* branch was
written for the **Dear ImGui** UI of the legacy C++ port, now `oracle-old/`. That is not this window.
This window is **egui 0.36.1**, which is a different library with a different style model. When the hub
re-issues `tokens.json`, the two Oracle-shaped branches should be re-worded as: *ImGui style for
`oracle-old` (frozen)*, and *`tokens.rs` for `oracle` (this page)*.

**Second correction, load-bearing for whoever applies this:** `surfaceFamilies.$default` is **`plum`**,
not `deep-space`. `color.base.*` and `color.text.*` mirror `deep-space` and are explicitly marked legacy
("Do not remove … until both apps migrate"). **The Rust theme must read `surfaceFamilies`, not
`color.base`/`color.text`** — otherwise Oracle ships the legacy family under a token name that is about to
change meaning. Family is user-selectable; the accent is not. **Oracle's accent is cyan `#38BDF8`**
(`color.accent.oracle.value`) and never changes with the family.

**Where the window is today:** there is **no theming code at all**. `grep -rn
'set_visuals|set_style|set_fonts|set_theme|Visuals::|style_mut|FontDefinitions'` over
`crates/oracle-player/` at `4135ced` returns **zero hits**, and `main.rs:1157` builds
`eframe::NativeOptions` with only `.with_inner_size(..)` and `.with_title(..)`. The window is running
egui's stock dark theme, unmodified. That is the whole of "ugly right now": this is a **greenfield
`theme.rs`**, not an edit to an existing palette.

---

## 1. Token → egui mapping

Every egui field below was read from the pinned source, not from memory:
`~/.cargo/registry/src/index.crates.io-*/egui-0.36.1/src/style.rs` and `.../src/context.rs`,
`epaint-0.36.1/src/{margin,corner_radius,shadow}.rs`, `egui_dock-0.21.1/src/style.rs`. Versions are pinned
in `Cargo.lock`: **egui 0.36.1, eframe 0.36.1, epaint 0.36.1, emath 0.36.1, egui_dock 0.21.1.**

> ⚠ **Naming, 0.36-specific.** `Rounding` does **not** exist; it is **`CornerRadius`** with `u8` fields
> `{nw, ne, sw, se}`. `Margin` fields are **`i8`** (`left/right/top/bottom`). `Shadow` is
> `{offset: [i8;2], blur: u8, spread: u8, color}`. Anything written against egui ≤0.31 will not compile.

Let `F` = the selected `surfaceFamilies` entry (default `plum`), `A` = `#38BDF8`.

### 1.1 Surfaces — `Visuals`

| token | egui 0.36.1 field | note |
|---|---|---|
| `F.void` | `Visuals::window_fill` | the frameless window's base; CHROME_SPEC "Base background: `void`" |
| `F.surface` | `Visuals::panel_fill` | every `CentralPanel`/`SidePanel`/`TopBottomPanel` |
| `F.field` | `Visuals::extreme_bg_color` | recessed input fill — CHROME_SPEC's "fields sit on recessed `field` fill". Also settable narrowly via `Visuals::text_edit_bg_color: Option<Color32>` |
| `F.raised` | `Visuals::faint_bg_color` | the zebra fill used by `Visuals::striped` tables |
| `F.raised` | `Visuals::code_bg_color` | code/hex backgrounds |
| `F.border` | `Visuals::window_stroke` (1px) | |
| `radius.md` (4px) | `Visuals::window_corner_radius`, `menu_corner_radius` | `CornerRadius::same(4)` |

### 1.2 Widgets — `Visuals::widgets: Widgets`

`Widgets` has exactly five `WidgetVisuals`: `noninteractive`, `inactive`, `hovered`, `active`, `open`.
`WidgetVisuals` = `{bg_fill, weak_bg_fill, bg_stroke, corner_radius, fg_stroke, expansion}`.
CHROME_SPEC's "crisp & bordered" personality maps as:

| state | `bg_fill` / `weak_bg_fill` | `bg_stroke` | `fg_stroke` |
|---|---|---|---|
| `noninteractive` | `F.surface` | 1px `F.border` | `F.text.base` |
| `inactive` (secondary button at rest) | `F.raised` | 1px `F.border` | `F.text.base` |
| `hovered` | `F.overlay` | 1px `F.borderStrong` | `F.text.hi` |
| `active` (pressed) | `F.overlay` | 1px `A` | `F.text.hi` |
| `open` (combo/menu open) | `F.overlay` | 1px `F.borderStrong` | `F.text.hi` |

- `corner_radius` on all five = `CornerRadius::same(4)` (`radius.md`; CHROME_SPEC "radius 4px on controls").
- `expansion` = `0.0` on all five. CHROME_SPEC says controls do not grow or shadow on hover; the state is
  carried by stroke and fill.
- **`bg_fill` vs `weak_bg_fill` is not cosmetic:** `Button` uses `weak_bg_fill`, while checkbox/radio
  bodies use `bg_fill`. Set both to the same value per row unless a deliberate difference is wanted.

### 1.3 Accent, selection, semantics

| token | egui field |
|---|---|
| `A` at `chrome.selectionAlpha` = 0.28 | `Visuals::selection.bg_fill` |
| `A` (1px) | `Visuals::selection.stroke` |
| `A` | `Visuals::hyperlink_color` |
| `color.semantic.warning` `#FBBF24` | `Visuals::warn_fg_color` |
| `color.semantic.error` `#F87171` | `Visuals::error_fg_color` |
| `A` | `Visuals::text_cursor.stroke.color` |

Use `Color32::from_rgba_unmultiplied(0x38, 0xBD, 0xF8, 71)` for the 0.28 selection (`0.28 × 255 ≈ 71`).
`success` `#34D399` and `info` `#38BDF8` have **no** `Visuals` slot — they are per-call-site
`ui.colored_label` colors, so they belong as `pub const` in `theme.rs`, not in `Visuals`.

### 1.4 Text — `Style::text_styles` and fonts

`Style::text_styles: BTreeMap<TextStyle, FontId>`. `TextStyle` in 0.36.1 is
`Small | Body | Monospace | Button | Heading | Name(Arc<str>)`. egui's stock sizes (`style.rs:1413
default_text_styles`) are Small 9, Body 13, Button 13, Heading 18, Monospace 13 — all Proportional except
Monospace. Map `type.scale`:

| `TextStyle` | FontId | token |
|---|---|---|
| `Small` | `FontId::new(10.0, Proportional)` | `2xs` (10px) — **note this raises egui's stock 9px** |
| `Body` | `FontId::new(13.0, Proportional)` | `base` (13px) |
| `Button` | `FontId::new(13.0, Proportional)` | `base` |
| `Heading` | `FontId::new(16.0, Proportional)` | `lg` (16px) — panel titles |
| `Monospace` | `FontId::new(11.0, Monospace)` | `xs` (11px) — "register dumps" is `xs`'s stated use |
| `Name("dense")` | `FontId::new(11.0, Proportional)` | `xs` — table cells |
| `Name("section")` | `FontId::new(20.0, Proportional)` | `xl` — section headers |

`type.weight` (400/500/600) has **no egui equivalent** — see §2.

### 1.5 Spacing and radius — `Style::spacing: Spacing`

`space` is a px scale (`1`=2px … `9`=48px), so it converts one-to-one; nothing here is `rem`.

| token | `Spacing` field | value |
|---|---|---|
| `space.2` | `item_spacing` | `Vec2::new(4.0, 4.0)` |
| `space.5` | `window_margin`, `menu_margin` | `Margin::same(12)` (**`i8`**) |
| `space.5`/`space.2` | `button_padding` | `Vec2::new(12.0, 4.0)` |
| `space.5` | `indent` | `12.0` |
| — | `interact_size` | `Vec2::new(0.0, 22.0)` — a 24px `chrome.statusbar` needs rows that fit it |
| `space.3` | `scroll.bar_width` | `6.0` |

Custom thin scrollbars (CHROME_SPEC "stock scrollbars are forbidden") are `Spacing::scroll: ScrollStyle`
— `{floating, bar_width, handle_min_length, bar_inner_margin, bar_outer_margin, foreground_color,
dormant_/active_/interact_ handle_opacity, fade, …}`. Set `floating: false`, `bar_width: 6.0`,
`foreground_color: true`, and the three handle opacities to `1.0` so the thumb is always the "one surface
step lighter" the spec asks for, rather than fading.

### 1.6 Frames, docks, panels

- `egui::containers::Frame` = `{inner_margin, fill, stroke, corner_radius, outer_margin, shadow}`. Panel
  bodies: `fill = F.surface`, `inner_margin = Margin::same(12)`, `stroke = 1px F.border`,
  `corner_radius = CornerRadius::same(4)`, **`shadow = Shadow::NONE`**.
- **egui_dock**: `egui_dock::Style::from_egui(&egui::Style)` derives the whole dock style from the theme,
  so the theme is set once and the dock follows. The three overrides worth making by hand afterwards, all
  in `egui_dock-0.21.1/src/style.rs`: `tab_bar.bg_fill = F.void`, `tab_bar.hline_color = F.border`, and
  the active tab's 2px accent underline (CHROME_SPEC "active tab gets a 2px accent underline") via
  `tab.active` / `TabInteractionStyle`. `Style::main_surface_border_stroke` = 1px `F.border`;
  `main_surface_border_rounding` = `CornerRadius::same(4)`.

### 1.7 Applying it

Dark-only: call `ctx.set_theme(egui::ThemePreference::Dark)` **and** install with
`ctx.set_style_of(egui::Theme::Dark, style)`. **Do not use bare `ctx.set_visuals` / `ctx.set_style`** —
`context.rs:2277` shows `set_visuals` writes only `self.theme()`'s style, so if the OS ever reports light
the window silently reverts to stock egui. Also set `Visuals::dark_mode = true`, which several widgets
branch on independently of which style slot they came from.

---

## 2. What egui 0.36.1 cannot express — and the policy for each

This section is the point of the page. Each item is a **decision already made here**, so the applying
parcel does not re-litigate it mid-work.

1. **Shadows.** `Shadow` exists (`window_shadow`, `popup_shadow`, `Frame::shadow`) but CHROME_SPEC says
   "no drop shadows on controls", and there is no CSS-like shadow token to convert anyway.
   **Policy:** `Visuals::window_shadow = Shadow::NONE` and `popup_shadow = Shadow::NONE`. Depth is carried
   by the surface ladder `void → surface → raised → overlay`, which is what the families are for.

2. **`rem`-based spacing.** There is none — `space.*` and `radius.*` are px, and egui points are
   1:1 with CSS px at `pixels_per_point = 1.0`. **Policy:** convert numerically, and never call
   `ctx.set_pixels_per_point` to "scale the UI"; DPI scaling is the OS's, and rescaling would desynchronise
   this window's px from the suite's.

3. **`motion.duration`'s three values.** egui has exactly **one** global `Style::animation_time: f32`
   (seconds). Three durations cannot be themed. **Policy:** `animation_time = 0.150` (`motion.quick`,
   the value that covers tab switches and collapse). `instant` (80ms) and `deliberate` (250ms) are
   per-call-site via `Context::animate_bool_with_time`. Record 80/150/250 as `pub const` in `theme.rs` so
   the numbers exist even where the theme cannot carry them.

4. **`motion.ease` cubic-beziers.** egui's animation helpers are linear/ease-out internally and take no
   easing curve. **Policy:** not expressible; drop it, and do not hand-roll an easing layer for a debug
   window. Note the omission in `theme.rs` so nobody "fixes" it later by accident.

5. **`prefers-reduced-motion`.** egui reads no such signal. **Policy:** ship a
   `--reduced-motion` flag (or a settings toggle) that sets `animation_time = 0.0`. Until then, state in
   the doc comment that Oracle does not honour the preference — an unstated gap is worse than a stated one.

6. **`type.weight` (400/500/600).** egui selects fonts by `FontFamily`, not by weight; there is no
   bold axis on a `FontId`. `RichText::strong()` sets a flag (`widget_text.rs:252`) whose only effect is
   the colour `Visuals::strong_text_color()`, which is *defined as* `widgets.active.text_color()`
   (`style.rs:1146`) — so §1.2's `active.fg_stroke = F.text.hi` **is** what `ui.strong()` renders. It does
   not embolden. **Policy:** express emphasis by **colour**
   (`text.base` → `text.hi`) and by **size step**, not by weight. If a real 600 is ever wanted, it costs a
   second `FontFamily::Name("ui-semibold")` with a second embedded font file — a separate decision.

7. **The actual typefaces.** `type.font.ui` names Inter, `type.font.mono` names JetBrains Mono. egui
   bundles Ubuntu-Light and Hack (`epaint-0.36.1/src/text/fonts.rs:513,527`) and has **no system-font
   lookup** — a font must be handed to `ctx.set_fonts` as bytes. **Policy:** shipping Inter + JetBrains
   Mono means vendoring two OFL font files into the repo and `include_bytes!`-ing them. That is a
   repo-weight and licensing call the owner should make; **until he does, the mapping is honoured in
   size, colour and family role, and the faces stay egui's stock pair.** The doc must not claim
   suite-font parity while Hack is rendering.

8. **The frameless window and its titlebar.** CHROME_SPEC requires no native decorations, a 36px
   accent-wash titlebar with a glowing mark, a 2px accent thread, and custom window controls.
   `main.rs:1157` currently builds a **decorated** window. `ViewportBuilder::with_decorations(false)`
   exists, but then move/resize/snap are this app's problem on X11 and Wayland. **Policy:** this is a
   parcel of its own, sequenced **after** the panels. Do the theme and the panels first; the chrome
   shell second. Say so rather than half-doing it — a frameless window that cannot be dragged is worse
   than a decorated one.

9. **Gradients** (`titlebar` wash, `thread` fade to 60%, `toolbar` bleed). egui has no gradient fill;
   `Frame::fill` is flat. **Policy:** when the chrome parcel lands, paint these with an explicit
   `epaint::Mesh` of vertex-coloured triangles (which egui does support) — not by approximating a
   gradient with stacked flat rects.

10. **The command palette, icon rail, inspector and output dock** (CHROME_SPEC "Layout grammar").
    egui_dock gives tabs and splits; it gives none of these region roles. **Policy:** out of scope for
    the style guide. Oracle's dock is a legitimate, different arrangement of the same regions; if the hub
    wants literal region parity it should say so as its own requirement, not be inferred from this page.

11. **Per-tool surface-family selection.** `surfaceFamilies` is user-selectable per app, but nothing in
    this repo persists a preference. **Policy:** `theme.rs` takes the family as a parameter and defaults
    to `plum`; wiring it to a persisted setting rides along with the existing `eframe/persistence`
    storage in `src/layout.rs` when someone asks for it.

---

## 3. Panel spec

Enforceable rules. Each says how a reviewer checks it. The hub's three rules are P1–P3.

**P1 — No raw JSON is rendered in a panel.**
No `serde_json::Value` (or `Map`/array) may reach a `ui.*` call as a value formatted by `Display`,
`Debug`, or `to_string()`. Server replies are projected into named Rust fields and rendered as facts.
*Check:* every `ui.*` argument that derives from a `Value` passes through a function that matches on
`Value::String`/number/bool exhaustively; no `{}` or `{:?}` on a `Value`, `Map` or `Vec<Value>`.
Grepping the panel body for `Value` and following each use is sufficient.

**P2 — Tabular data goes in a table with fixed columns.**
Rows of like-shaped data use a real column layout — `egui::Grid` (already available; `egui-0.36.1/src/grid.rs`)
or `egui_extras::TableBuilder` (**not currently a dependency** — `egui_extras` appears nowhere in
`Cargo.lock` at `4135ced`; adding it is the applying parcel's call). Header row present; every column
named. Striping via `Visuals::striped` + `faint_bg_color`.
*Check:* **the panel contains no width-padded format specifier.** `{:<12}`, `{:>3}`, `{:>7}` and friends
inside a `ui.monospace(format!(..))` are exactly the pseudo-table this rule outlaws. At `4135ced` there
are **9** such padded specifiers in `crates/oracle-player/src/ui.rs`, across 24 `ui.monospace(format!(..))`
sites — `grep -cE '\{:[<>^][0-9]+' crates/oracle-player/src/ui.rs` is the check, and it must reach 0 for
row data. (Padding inside a single self-contained `summary()` string is the same violation, just moved:
see `objects.rs:85`, `objects.rs:355`, `stopping.rs:138`.)

**P3 — Monospace only for addresses, hex, and code.**
`ui.monospace` / `TextStyle::Monospace` is reserved for machine addresses (`0x00FF8000`), hex byte runs,
register values, symbol names as they appear in a listing, and disassembly. Prose, labels, counts,
column headers and refusal sentences are `Body` or `Small`.
*Check:* read each `ui.monospace(..)` argument. If it would be equally readable in a proportional font,
it is a violation. Concretely, at `4135ced` the Pacing tab's `ui.monospace("frames emulated   {}")`
(`ui.rs:371-379`) is prose + a count and fails; the `addr` column of the object pool passes.

**P4 — A refusal is a whole-panel state, never an empty table.**
A panel that could not derive its subject renders the server's own code and message plus the remedy —
it does not render zero rows. The Objects tab already does this correctly (`Objects::Refused` /
`objects::refusal_text`, `objects.rs:503+`); the rule generalises it.
*Check:* every panel that reads a fallible source has a match arm with no table in it.

**P5 — A refusal's colour comes from the reply, not from the text.**
Error colouring uses `Visuals::error_fg_color` driven by a carried boolean, never by sniffing the string
for `"REFUSED"`. `ui.rs`'s `note_label` is the model.
*Check:* no `contains("REFUSED")`, no prefix matching on message prose, anywhere in a render path.

**P6 — An absent fact is a stated line, never a zero and never an omitted row.**
"not present", "this listing does not partition the table", "rings unavailable — <code> <message>" are
rows. `0` in place of an unknown is a defect, because `0` is a measurement.
*Check:* every `Option`/`Result` field in a panel's view type has a `None`/`Err` arm that emits text.

**P7 — One panel, one scroll position; scroll areas carry a stable `id_salt`.**
*Check:* every `ScrollArea` in a panel has an explicit `.id_salt("…")`.

**P8 — No panel composes a sentence about the server it lives inside.**
Server codes and messages appear verbatim; the panel may add remedies a human at this window can act on.
*Check:* refusal strings are `format!`s over `e.code`/`e.message`, not rewrites of them.

---

## 4. The before-case

The reference for "ugly" is the owner's capture, named by the hub in
`projects[10].styleRequirement`: **`empyrean docs/captures/2026-09-05-owner-oracle-player-objects-panel.png`**
— the Objects tab as he first saw it. **I did not open it** (this task is barred from viewing it); the
rules above are derived from the hub's description of it — *a raw JSON layout blob at the top with two
plain-text tables under it* — and from reading the code that produces those regions.

### ⚑ The JSON blob: already fixed, and this is the brief's stale premise

The brief for this page states the Objects tab is rendering a raw JSON blob **right now**. **At
`main` = `4135ced`, it is not.** The defect was fixed on **2026-09-04 00:54** by commit **`09a7e11`**
*("objects-panel: the header stops dumping JSON, and rings answer where they are asked")*, which:

- replaced `Pool`'s `layout: serde_json::Value` with named scalar fields (`engine`, `slot_count`,
  `slot_bytes`, `base_addr`, `detected_from`, `partition`) — `objects.rs:515-540`;
- added `Pool::layout_lines() -> Vec<String>` (`objects.rs:549`), the readable spelling;
- made the header render those lines (`ui.rs:727`, `for line in pool.layout_lines()`);
- and guarded it with `the_header_carries_the_layouts_facts_and_not_its_json` (`objects.rs:1673`), which
  asserts both halves: every fact survives, and no JSON punctuation does.

The exact string that used to appear across the top of the tab is recorded in the doc comment at
`objects.rs:515`: `{"baseAddr":"0x00FF8000","detectedBy":"symbol",…}`. That matches the hub's description
of the capture precisely.

**So the capture shows a build older than the running code.** The capture was banked into `empyrean` on
2026-09-05 10:02 (`fa416ea`), ~33 hours *after* the fix; the owner was running a binary built before
2026-09-04. **What a later parcel should do is confirm the owner rebuilds, not re-fix the header.**
The other half of his capture — *two plain-text tables* — **is still true at `4135ced`** and is exactly
what P2 and P3 target.

### The one raw-JSON path that is still live

`ui.rs:1460`'s helper is:

```rust
fn render(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
```

It is used for the expanded slot's fields (`ui.rs:865`, `ui.rs:870`). **Today it is safe**: every value
`DecodedRecord::to_json` emits is a scalar (`decoders.rs:670-704` — `x`/`y` numbers, `code`/`addr`/`bytes`
strings, `field_value` returns only `U`/`I`/`Hex` scalars), and `ui.rs:864` explicitly `continue`s past
the one composite key, `"fields"`. **But the `other =>` arm will print raw JSON the day any served key
becomes an object or array** — a nested `pools`, a per-field provenance map — with no test to catch it.
Per P1 this should be made an exhaustive match whose composite arms are unrepresentable or explicitly
rendered. A one-line note, not a bug on the owner's screen.

---

## 5. Sequencing

1. `crates/oracle-player/src/theme.rs` — §1 as code, family-parameterised, `plum` default, dark-only via
   `set_style_of(Theme::Dark, …)`; `egui_dock::Style::from_egui` plus the three overrides in §1.6.
2. Panels, one per parcel, against §3. Objects first — it is the capture's subject and it fails P2/P3.
3. The frameless chrome shell (§2 items 8 and 9), only after the panels.
4. Fonts (§2 item 7) whenever the owner rules on vendoring Inter/JetBrains Mono.

Look calls park with captures for the owner, per `styleRequirement.order`.
