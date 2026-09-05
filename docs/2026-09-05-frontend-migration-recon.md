# Migrating the game window onto the toolkit — recon and plan

**Date:** 2026-09-05 · **Branch:** `recon/frontend-migration` · **Base:** `1c2639e`
**Scope:** recon and design only. **Nothing was rebuilt, no window was launched, no emulator MCP tool was
touched.** Every claim below is source-read at `1c2639e` in this worktree, or quoted from a named doc.

This is the second half of the owner's d-25 ruling (`docs/OVERSEER.md:1009`). The first half —
*rebuild the debug window on a real UI toolkit* — shipped as `crates/oracle-player` (egui 0.36 / eframe /
egui_dock 0.21, eight tabs in `ui::Tab::ALL`, `ui.rs:102-112`). The half that remains is the one the
ruling itself deferred: **the game window, `crates/oracle-frontend`, is still a hand-drawn `minifb`
window, and it migrates onto the toolkit later.** This document prices that.

---

## 0. Bottom line

| | |
|---|---|
| **The headline** | **The migration is far smaller than the crate sizes suggest, and it is not a port of the window — it is a port of ~14 human-facing features into a window that already exists.** `oracle-player` has already independently rebuilt every *architectural* thing `oracle-frontend` does: the picture as a texture, aspect-fit, the pacing, the input latch, the bus hosting, the drain-and-repair, `emulator/screen_text`, the hold/pad merge, the picture handed to `emulator/screenshot`, layout persistence. §1.7 is the list. |
| **What is genuinely missing** | Save states, `.srm`, F5 ROM reload, the ROM browser, `player.conf`, gamepad, the overlay/toast layer, symbol watches, click-picking, the spawn picker, the window icon/WM class, the two non-square `Aspect` modes, the volume/mute/filter controls, and — the one nobody had booked as a migration item — **a display mask that reaches the picture at all**. §1.2, §1.6, §3. |
| **The recommended first slice** | **S1 — click-picking on the Screen tab** (with S2/S2a close behind it as one arc). It is the seam everyone is most worried about, it is the row the owner has an open question on, and it turns out to be the *cheapest* thing on the list: `present::window_to_native` is already a pure function over an arbitrary rect, and egui hands you the rect and the click position on the `Response` the Screen tab currently throws away (`ui.rs:210`). §3.1. |
| **Rows re-measured** | **`F-FRONTEND-PALETTE-BUS` should be CLOSED, not built** — its premise is dissolved by the migration and its stated cost (a free-text argument mode) is already paid in `oracle-player/src/palette.rs`. §2.1. |
| **A premise correction** | The brief says *"three rows in the queue describe this seam."* **Only one does.** `F-STATUS-CAVEAT-NOT-ON-STRIP` and `F-FRONTEND-NO-STATUS` exist as names in `docs/lane-log.jsonl:109` and **have no `id` anywhere in `docs/lane-status.json`'s `queue`** (19 ids at `1c2639e`, enumerated in §2.2). They were booked in a narrative and never tracked. |
| **A cost correction** | `docs/OVERSEER.md:298`'s `F-SPAWN-PICKER-PANEL-SURFACE` prices the panels surface as needing *"an egui-rect→native-dot mapping invented from scratch."* **It does not exist from scratch; it exists as `present::window_to_native`** (`present.rs:203`), which already takes an arbitrary `Rect`. §2.3. |
| **The thing I am least sure about** | Whether the *new* per-frame surfaces this migration adds — the overlay/toast layer, picking hover, layer toggles — fit in the margin. Not whether the eight panels fit: **they were measured, a real 14.8 ms stall was found in them, and it was fixed** (`docs/2026-09-03-debug-panels-design.md` §5.7.2). §7.1 states the residual honestly, and it is narrower than I first wrote it. |
| **Not measured here** | No build was run. This worktree has **no `target/` directory**, so `cargo check` would compile the toolkit's 466-package graph cold on the owner's live machine for a docs-only change. Declared, not hidden. |

---

## 1. What `oracle-frontend` actually is today

Enumerated by *what touches the window*, per the brief — not by what a grep for `minifb` finds.

### 1.1 The event loop

`crates/oracle-frontend/src/main.rs:1480` — `while window.is_open() && running { … }`, one poll loop,
one `Window::new` in the whole crate (`main.rs:1259`), on the main thread, **zero `thread::spawn` in
`main.rs`** (cpal and Aether spawn their own, and neither touches the window). This is the spike's §6
correction and it still holds at `1c2639e`.

Per-iteration ordering, read off the loop body:

1. `window.get_size()` → `present::dest_rect(...)` — geometry re-derived every iteration because the
   window is resizable (`main.rs:1482-1483`).
2. Mouse-edge detect → picking / spawn (`main.rs:1493-1560`).
3. Keys → palette → commands → pad.
4. `audio::frames_to_run` decides **0, 1 or 2** emulated frames from audio-ring occupancy
   (`main.rs:2081-2091`).
5. Emulate with `run_frames_with_sink` + `ScanlineCapture` — the picture is assembled *during* the run,
   line by line, not read back afterwards (`main.rs` module doc, "Pixels").
6. Aether pump, budgeted at 4 ms (`main.rs:2217`).
7. Overlay/lens composite, blit, `window.update_with_buffer(&screen, win_w, win_h)` (`main.rs:2493`).

**The master clock is the audio device, not the window** — `minifb::set_target_fps(60)` is a coarse
governor that the ring finely trims, because the limiter's `sleep(target − elapsed)`-then-restart shape
runs 59.54–59.63 fps, a permanent 0.62 % sample deficit (`main.rs` module doc, "Pacing").
`oracle-player` reproduces both layers deliberately (`docs/2026-09-02-player-pacing-design.md` §1-2), so
**this is the one architectural property that does not need porting — it has already been ported.**

### 1.2 Presentation

`present.rs` computes the destination rectangle itself and hands minifb a 1:1 `ScaleMode::Stretch`
present, because minifb "never tells the caller where it put the image" (`present.rs:1-17`). Three
things fall out, and all three are stated in that header: the window is resizable, the click inverse is
an *exact* inverse of the blit rather than a re-derivation, and **overlays draw at window resolution**
rather than being scaled up with the game.

`Aspect` (`present.rs:29-37`) has three modes: `Tv` (4:3, the **default**, because a Mega Drive does not
have square pixels and H32 is stretched wider rather than pillarboxed), `Square`, `Integer`.

Composited into that same `Vec<u32>` buffer, in Z order, are **13 distinct surfaces**: six lens panels
drawn from seven `LensId`s (`lens/mod.rs:47` = `Watch, Cpu, CpuRegs, Sprites, Cram, Hover, Profile` —
`Cpu`/`CpuRegs` share one panel, compact vs expanded; *the `main.rs` module doc says "five read-only
overlays" and is stale*); the palette's command list and its picker panel (`palette.rs:341`); and six
from `overlay.rs` (2311 lines) — status line with the 10-cell save-slot strip, layer badge, `PAUSED`
banner, toasts, spawn badge. Plus the crosshair, which is the one thing drawn in *native* pixels, as an
XOR into a scratch copy (`main.rs:786`). All of it uses a self-contained 5×7 bitmap font (`font.rs`,
~96 glyphs, uppercase folded at draw time) that exists solely because minifb has no text rendering.

**Two properties of this layer are timing, not drawing, and both are migration hazards.**
`ov.tick(paused)` (`main.rs:2466`) counts **presented** frames, so `TOAST_FRAMES = 150`,
`FADE_FRAMES = 30` and `PAUSED_BANNER_DWELL_FRAMES = 12` are all in presented-frame units — and egui
repaints *on demand*, so the same constants mean different wall-clock durations there. And **overlays
must never write into the retained `buf`**: a 0-frame or paused iteration re-presents it forever, so ink
accumulates. That rule is stated four separate times in this crate (`main.rs:1362-1365`, `:2424-2425`,
`overlay.rs:20-23`, `lens/mod.rs:8-10`) because `draw_crosshair` learned it the hard way.

**There is also a second, mutually-exclusive pixel path.** `blit_masked` (`main.rs:725`) re-derives the
picture from VDP state via `render_line_masked` whenever a display layer is hidden, and it is entered
**from inside `drain::drain`** (`drain.rs:230`) — so a *bus client* can switch the window's rendering
path, at the cost of every mid-frame palette effect. The window announces it with the layer badge and
`pick.rs` mirrors it in a caveat. **`oracle-player` has no equivalent** (agent-verified: `Bus` has no
`layers()` accessor). §3.2a books it.

### 1.3 Input

Keyboard bindings are enumerated in the `main.rs` module doc table (lines 30-52) and are the ones the
owner's hands know: arrows = D-pad, `A`/`S`/`D` = A/B/C, `Enter` = Start, `Space` = pause, `.` = step,
backtick or Ctrl+P = palette, `Tab`/`F1` = reset, `F5` = ROM reload, `F2`/`F4` = save/load state,
`F6`/`F7`/`0`-`9` = slots, `-`/`=`/`M` = volume, `F` = audio filter, `F3` = status line, `W`/`C` = watch
dump/clear, `Esc` = close palette. 67 `Key::` references in `commands.rs`.

Gamepads (`gamepad.rs`, feature-gated `gilrs`) drive P1 and P2, hotplugged, **OR'd per button with the
keyboard** so the keyboard never goes dead. `oracle-player` has no gamepad support at all
(`docs/2026-09-02-player-pacing-design.md` §6: "the brief asks for keyboard at minimum").

The minifb-specific surface is small and named: `get_mouse_down(MouseButton::Left)`,
`get_mouse_pos(MouseMode::Discard)`, `is_key_down`, `get_size`, `is_open`, `update_with_buffer`,
`set_target_fps`. Plus `icon.rs`, which reaches through `raw-window-handle` into minifb's own X11
`Display` to call `XSetClassHint` (`icon.rs:22,166`) — that one is genuinely minifb-shaped and is
replaced, not ported (§3.7).

### 1.4 Click-picking

`pick.rs` (1314 lines) is **almost entirely window-independent**: it resolves a native dot through
`Vdp::pixel_attribution`, `sprites_decoded`, `sat_base` and `oracle_core::render::sprite_tile_at`, and
arms watches. It carries three properties that must survive the migration and are asserted in its own
tests:

* **`resolve` takes a `LayerMask` and there is no unmasked twin** (`pick.rs:35-41`) — the panel describes
  the *picture*, so once a display mask could hide a layer, an unmasked attribution stopped doing that.
* **The colour caveat is `oracle_core::render::cram_divergence_caveat`, the same function the wire uses**
  (`pick.rs:45-57`), guarded by `bus_parity::the_panel_and_the_bus_carry_the_same_colour_caveat`.
* **D15: the panel resolves in-process and does not ask the bus** (`pick.rs:231`, `:718`, `:1132`).

The window-coupled part of picking is exactly three calls and one pure function, at
`main.rs:1493-1502`:

```rust
let mouse_down = window.get_mouse_down(MouseButton::Left);
let clicked = mouse_down && !prev_mouse_down;
if clicked && !palette.is_open() {
    if let Some((mx, my)) = window.get_mouse_pos(MouseMode::Discard) {
        if let Some((x, y)) = present::window_to_native(mx, my, view, width, HEIGHT) {
```

and `window_to_native` (`present.rs:203`) is `fn(f32, f32, Rect, usize, usize) -> Option<(u16,u16)>` —
a pure function over an **arbitrary rect**, not over a window. `spawn.rs`'s click-to-place branches in
front of the watch pick at `main.rs:1512`, because "the two are the same gesture and only one of them
can have it — which is precisely why the mode owes a standing statement that it is on."

### 1.5 Its own command surface

`commands.rs` (655 lines) is a headless registry: `registry() -> Vec<CommandInfo>` at `commands.rs:158`.
**It yields 42 rows in a default build**, from 24 construction sites: 16 literal (`:159-241`), 1 hidden
F1 reset alias (`:243`), 10 generated `SlotSelect(n)` rows keyed `0`-`9` (`:268-289`, both arrays typed
by `SLOT_COUNT` so a slot without a key is a *compile* error), 7 `ToggleLens` rows from `LensId::ALL`
(`:294-301`), **4 `ToggleLayer` rows generated from the core's own `LayerMask::targets()`**
(`:311-318` — nothing here transcribes a layer name, so the palette cannot offer a layer the bus lacks),
and 4 audio rows (`:321-352`). **29 carry a hotkey; 31 are visible.**
*The two prior docs say "~25" and "30"; both undercount, because both were counting literal rows and
missing the four generated families.*

`palette.rs` (1069 lines) is a pure state machine — `PaletteKey` in, `PaletteAction` out, testable
headless. **Argument model: there is none.** `Cmd` is a flat 22-variant `Copy` enum whose only payloads
are a `usize` index (`SlotSelect(n)`, `RomEntry(n)`), both hidden from the list; the one "argument"
mechanism is `Picker` (`palette.rs:79`), a secondary list of pre-built concrete rows used by exactly two
things (the slot list, the ROM browser), each of which resolves back to a fixed `Cmd` variant.

**Zero references to `METHODS`, `Host::call` or the served vocabulary** — `METHODS` appears in this
crate exactly once, inside an error message in `bus.rs:137`. That is the whole content of
`F-FRONTEND-PALETTE-BUS`, and §2.1 rules on it.

### 1.6 What is not window-coupled — and would survive any toolkit unchanged

`config.rs` (934, `player.conf`), `save_state.rs` (629), `sram_file.rs` (349), `symbol_file.rs` (256),
`symbol_watch.rs` (534), `rom_browser.rs` (363), `bus.rs` (1853) / `bus_stub.rs` (297), `drain.rs`
(1173), `audio.rs` (853), `gamepad.rs` (468), `pick.rs` minus the three calls above, `present.rs` minus
its minifb doc references. That is **~7,600 lines that move by relocation, not by rewriting.**

**But there is a hard mechanical constraint on relocating them: neither crate has a lib target.**
`crates/oracle-frontend/src/lib.rs` and `crates/oracle-player/src/lib.rs` both do not exist; both are
bin-only. So nothing in `oracle-frontend` can be `use`d from `oracle-player`. **The tree already has
the workaround and it is load-bearing:** `crates/oracle-player/src/main.rs:46` is

```rust
#[path = "../../oracle-frontend/src/audio.rs"]
mod audio;
```

— byte-for-byte inclusion of the frontend's audio module, at the frontend's own `ringbuf 0.4` / `cpal
0.15`. §3.0 rules on whether to keep using that or to grow a real lib.

### 1.7 ⚑ What `oracle-player` already holds — the finding that resizes this whole task

Measured at `1c2639e`, and this is the reason the plan below is short:

| Responsibility | `oracle-frontend` | `oracle-player` | Verified at |
|---|---|---|---|
| Picture on the glass | blit into `Vec<u32>` | `egui::TextureHandle`, `TextureOptions::NEAREST`, re-uploaded only when a frame ran | `main.rs:594,674-678`; `ui.rs:199-212` |
| Aspect fit | `present::dest_rect`, 3 modes | square-pixel fit only, inline, 13 lines | `ui.rs:205-208` |
| Pacing | audio ring + `set_target_fps` | audio ring + own monotonic governor | `pacing.rs`; pacing design §2 |
| Audio | `audio.rs` | the identical file via `#[path]` | `main.rs:46` |
| Keyboard → pad | `poll_pad` | same binding, same latch, driven by `wants_keyboard_input()` | pacing design §3.1 |
| Aether hosting | `--aether` / `--socket` | `--aether` / `--socket`, three-state, same default path, refuses a live bind | `main.rs:112-121` |
| Drain + repair | `drain.rs` | `bus::drain`, one function, acts on its own `PumpReport` | `main.rs:582-588` |
| `emulator/screen_text` | `screen_text.rs` | `screen.rs` + `nav.rs` + `ui.rs`, published after `build_ui` | `main.rs:613-637` |
| Picture → `emulator/screenshot` | yes | yes | `bus.rs:532-546` |
| `emulator/hold` pad merge | yes | yes, both directions | `bus.rs:568-591` |
| Status line | F3 overlay | a real `StatusStrip` with `romPath`/`frame`/`frames_run`/`symbolCount`/`symbolAtPc`/`held`/`aether` | `ui.rs:1425-1481` |
| Layout persistence | n/a | **ON** — `eframe/persistence` + `egui_dock/serde`, RON | `Cargo.toml`; `layout.rs` |
| Palette over `METHODS` | **no** | **yes**, Ctrl+P, in-process `Bus::call` → `Host::call` | `palette.rs:32,309` |

Both binaries host `oracle_aether::Host` and **neither overrides `server_name`** (`engine.rs:167,208` —
the only three `server_name` hits in `crates/`). So both answer `serverName: "oracle-next"` and
`implementation: "oracle-rs"` (`build_info.rs:29`). §6 turns on that.

---

## 2. The booked rows, re-measured before pricing

Per constraint D and per `docs/OVERSEER.md:1035-1038` — *"re-measure a row's premise before spending an
agent on it, not after."*

### 2.1 `F-FRONTEND-PALETTE-BUS` — **CLOSE IT. Do not build it.**

Its recorded title: *"The GAME window has its own command line, but it can only run the window's own 30
hand-written actions — it cannot reach anything the emulator serves. … Not a row to add: it needs a
free-text argument mode the current design lacks."* (`docs/lane-status.json`, `queue`.) Its origin is
`docs/2026-09-03-debug-panels-design.md:1711-1721`.

Three independent reasons, each sufficient:

1. **Its subject is the crate the ruling retires.** The row is a defect *in `oracle-frontend`'s
   palette*. After the migration there is no second palette; the game window's palette **is**
   `oracle-player/src/palette.rs`, which is built by filtering `METHODS` and dispatches through
   `Bus::call` → `Host::call` (`palette.rs:32`, `:309`).
2. **Its stated cost is already paid.** The row says the blocker is "a free-text argument mode the
   current design lacks" — that is a true statement about `oracle-frontend`'s `Cmd`-is-`Copy` state
   machine (§1.5). **`oracle-player`'s palette is exactly that free-text mode and has been since it
   shipped:** two `TextEdit::singleline`s, `method` (`palette.rs:319`, which doubles as the filter) and
   `params` (`palette.rs:327`, hint `"{} — a JSON object, or empty for none"`), with `parse_args`
   (`palette.rs:161`) treating empty as `{}` and quoting `serde_json`'s own line-and-column error
   verbatim on bad input. Its refusals are tested at `palette.rs:759,765`. A per-parameter form was
   considered and rejected on the record (`palette.rs:74-80`).
3. **Building it costs work that is then deleted.** Adding `Cmd::BusMethod` + a free-text mode to a
   crate scheduled for retirement is the definition of a migration you buy twice.

**What must not be lost when it closes.** The row is closed *by the migration*, so closing it takes on a
debt: the **42** registry rows in `commands.rs` are **frontend actions, not bus methods** — pause, step,
reset, save/load state, 10 slot selects, volume, mute, audio filter, aspect, status line, 7 lens
toggles, 4 layer toggles, ROM browser, spawn mode, quit. `oracle-player`'s palette lists *served methods
only*. So closing this row means slices S3-S6 below must land those actions somewhere in the player
(palette rows, nav menu, or hotkeys) or they are silently dropped. **Recommendation: close
`F-FRONTEND-PALETTE-BUS` and book one replacement row — "the player's palette lists methods but no
player actions" — against the player.** Note the four `ToggleLayer` rows are the *only* frontend action
that is already a served method (`emulator/set_layer_enabled`), so those four close for free the moment
§3.2a lands.

### 2.2 `F-STATUS-CAVEAT-NOT-ON-STRIP` and `F-FRONTEND-NO-STATUS` — **neither is in the queue.**

They appear exactly once in this tree, inside a narrative sentence in `docs/lane-log.jsonl:109`
(2026-09-04, CR-K serve): *"Their other three bookings stand: F-STATUS-CAVEAT-NOT-ON-STRIP,
F-FRONTEND-NO-STATUS, F-MCP-SYMBOL-FRESHNESS-NO-BANNER."* `docs/lane-status.json` at `1c2639e` has 19
queue ids and **none of them is either name**:

```
F-SPAWN-OUTSIDE-ACT, F-SPAWN-PICKER-PANEL-SURFACE, F-HANDSHAKE-LOAD-TIMEOUT, F-NAV-COLLAPSED-LEAF,
SERVE-EQUATES, SCHEMA-DRIFT-NIGHTLY, VRAM-WRITE-RULE-CR, ORACLE-SANITY-WEEKLY, S1-DIALECT-FIXTURE,
Y-SIGN-COVERAGE, F-ACCEPT-TABLE-RAWSTRING, ACCEPT-16, WIKI-SPIKE, OVERLAY-STATE, PEER-CLAIMS-SWEEP,
CR-STEP-SHORTFALL, REGISTER-WHICH-SERVER-SWEEP, ERROR-SURFACE-GATE, F-FRONTEND-PALETTE-BUS
```

(`docs/lane-status.json`'s `updatedAt` is `2026-09-04T10:25:08Z`, five hours *before* the log entry that
books them, which is consistent with the omission being a missed write rather than a deletion. The file
also contains `F-HANDSHAKE-LOAD-TIMEOUT`, booked 09-05, so `updatedAt` is itself stale.)

Ruling on each, from source:

* **`F-STATUS-CAVEAT-NOT-ON-STRIP` — the migration does NOT touch it; it survives and gets worse.**
  The subject is `oracle-player`'s `StatusStrip`, which has `rom_path`, `rom_bytes`, `frame`,
  `frames_run`, `symbol_count`, `symbol_at_pc`, `held`, `aether` (`ui.rs:1435-1481`) and **no field for
  `emulator/status`'s `caveat`**, which CR-K added to the wire. The only `caveat` in `ui.rs` is
  `view.caveats` at `:1041`, which belongs to the Watchpoints panel and is a different thing. After the
  migration this strip is the *only* status surface in the suite's only window, so the gap stops being
  one of two and becomes the one. **Book it properly, against `oracle-player`, and do it before or with
  S7.**
* **`F-FRONTEND-NO-STATUS` — the migration CLOSES it, structurally.** The frontend's F3 line is composed
  from local state; it never calls `emulator/status` (`overlay.rs:307` names the method only in a doc
  comment about the layer badge and the run-state banner). The player's strip derives its rows from the
  *same functions* `emulator/status` answers with — `System::rom().len()`, `mclk / MCLK_PER_FRAME`,
  `oracle_aether::engine::symbol_at`, `engine::absolutise` — and two tests hold it to that
  (`ui.rs:2506`, `:2623`). Retiring the frontend removes the surface the row is about.

### 2.3 `F-SPAWN-PICKER-PANEL-SURFACE` — the migration **dissolves the question the owner was asked.**

`docs/OVERSEER.md:298-317` is the trap on this seam and it has already caught one brief. Its content, as
written: the owner's tab ruling says spawn's surface is *"clicking a spot in the Screen panel"*; there
are two windows; SPAWN-PICKER landed on `oracle-frontend`; the brief that dispatched it conflated the
two, and the agent refused to half-build across the seam. It ends: *"Needs ONE WORD FROM THE OWNER … If
the game window, this is closed today. If the panels window, it is a fresh parcel."*

**After the migration there is one window, and it is the panels window with the game picture in a tab.
Both answers become the same answer.** That is the strongest single argument for doing S1 first: it
retires an open owner question rather than waiting on it.

Two corrections to that entry, both measured here:

1. **The measurement it cites is of the wrong file.** It says *"`crates/oracle-player/src/screen.rs`
   (541 lines) has ZERO pointer interaction."* True, and irrelevant: `oracle-player/src/screen.rs` is
   **not the Screen tab** — it is the `emulator/screen_text` glyph/readback model (`screen.rs:2`,
   `:137-170`, all about `epaint::text::Glyph::uv_rect`). The Screen *tab* is
   `ui.rs:199-212`, `Panels::screen`. The names collide across the two crates
   (`oracle-frontend/src/screen_text.rs` ≈ `oracle-player/src/screen.rs`) and that is what the read
   fell into. The conclusion — the Screen tab cannot receive a click today — is still **correct**:
   `ui.rs:210` is `ui.add(egui::Image::new(tex).fit_to_exact_size(src * scale));`, and the `Response`
   that `add` returns is **discarded**.
2. **Its cost estimate is too high.** It prices the panels surface as *"an egui-rect→native-dot mapping
   invented from scratch plus its own standing indicator."* The mapping is not invented from scratch:
   `present::window_to_native(mx, my, rect, src_w, src_h)` (`present.rs:203-222`) is already generic
   over a rect and is the *exact inverse of the blit* rather than a re-derivation (`present.rs:200`).
   Under egui the rect is handed to you (`Response::rect`) and so is the click position
   (`Response::interact_pointer_pos()`), both in the same space. **The standing indicator is real new
   work; the mapping is an adapter.**

---

## 3. The migration plan

### 3.0 Two decisions the whole plan rests on

**(a) Two processes, never two windows in one process.** The spike's §6 is explicit and its reasoning
survives: `eframe::run_native` wraps `winit::EventLoop::run`, which consumes `self` and must own the
main thread; two display connections in one process is materially worse on Wayland; the pacing model has
one master (the audio ring) and any per-iteration egui cost is charged to the iteration that must
produce 735 samples. **This is not a problem for constraint E, because the two windows are already two
binaries.** `oracle-frontend` and `oracle-player` are separate `[[bin]]`-less crates with separate
`main`s and separate `System`s. "Both windows keep working through the migration" costs nothing
architectural — it is the status quo. The only shared resource is the default socket path, and §6
covers it.

**(b) Give `oracle-frontend` a lib target, and move code by `mod` rather than by copy.** Today the only
sharing mechanism is `#[path]` inclusion (`oracle-player/src/main.rs:46`), which works but is a
compile-time copy: two crates compile the same file twice with no shared type identity. Every slice
below moves 300-1200 lines. **Recommendation: add `crates/oracle-frontend/src/lib.rs` exporting
`config`, `save_state`, `sram_file`, `symbol_file`, `symbol_watch`, `rom_browser`, `present`, `pick`,
`commands`, and have `oracle-player` depend on it.** Cost: one `lib.rs`, `pub` on ~10 modules, and
`main.rs` switching from `mod x;` to `use oracle_frontend::x;`. Benefit: the frontend keeps working
(constraint E) while the player consumes the *same* code, so the two windows cannot drift apart during
the migration — which is the failure mode a copy-based migration guarantees. **Alternative if the lib
is refused:** keep `#[path]`, and accept that every module is compiled twice and that a bug fixed in one
window is fixed in both only by luck of the include. I do not recommend it.

*(Bin-only is a measured fact, not an assumption: no `lib.rs` exists in either crate, and
`docs/lane-log.jsonl:103` records an agent correcting a prior brief on exactly this — "oracle-frontend's
`Refusal` cannot generalise (bin-only crate, no lib target)". The same end state is already booked
independently: `oracle-player/src/main.rs:38-40` says "extracting a shared `oracle-audio` crate is the
right end state and is a parcel-2 line item, but it means editing `oracle-frontend`.")*

⚑ **One thing the lib must not drop, and it is easy to drop silently.** `pick.rs`'s
`#[cfg(feature = "aether")] mod bus_parity` (`pick.rs:724+`) can only exist in a crate that can see
**both** the panel and `oracle_aether::engine::Engine` at once — it builds an engine whose VDP is
byte-identical to the one the panel is handed, dispatches `emulator/pixel_attribution`, and asserts
address-level agreement for every dot of four sprite shapes under all four flip combinations, plus the
mask rows and the colour caveat, reading the mask and `now_mclk` **back off the engine** so the test
cannot keep the two sides in step itself. It is the strongest correctness guarantee in the frontend.
`oracle-player` already depends on `oracle-aether`, so the guard survives a move into the player — **but
a lib crate carved out without that dependency edge would delete it and stay green.** Whatever S0
produces must keep `pick.rs` and the aether edge in the same compilation unit.

**(c) The seven lenses are not ported as lenses.** The owner retired them:
*"a clean idea but just not good enough"*, and `docs/OVERSEER.md:1021-1022` records that this
**retires "lenses stay for what they suit" as a live design direction.** Mapping them onto what already
exists:

| Lens | Disposition |
|---|---|
| `Watch` (ticker) | already a shipped tab — `Tab::Watchpoints` |
| `Cpu`, `CpuRegs` | already a shipped tab — `Tab::Registers` |
| `Profile` | already a shipped tab — `Tab::Profiler` |
| `Cram` (palette strip) | genuinely absent; a small new panel, or drop |
| `Sprites` (outlines) | **must stay a picture overlay** — it marks things *in* the picture |
| `Hover` (callout) | **must stay a picture overlay** — it is the hover half of picking |

So four of seven are already done or droppable, and **only two are irreducibly picture-coupled.** Both
of those need `present::forward_map` (`present.rs:224`), the forward map that exists precisely because
"lenses that mark things *in the picture* need this."

### 3.1 S1 — click-picking and the spawn picker on the Screen tab **(do this first)**

**What.** Make `ui::Panels::screen` keep the `Response` it currently discards; call
`.interact(Sense::click())`; on a click, convert `Response::rect` to a `present::Rect` and call
`present::window_to_native` with `interact_pointer_pos()`; branch spawn-mode-then-pick exactly as
`main.rs:1512` does; render the spawn badge as a standing indicator on the tab.

**Why first.** (i) It closes `F-SPAWN-PICKER-PANEL-SURFACE` by dissolving its question (§2.3). (ii) It
is the seam the OVERSEER entry flags as the trap, so proving it cheap early removes the fear from every
later slice. (iii) It needs no new state, no file I/O and no new dependency. (iv) It is the one gesture
the owner has actually asked about.

**Proof it worked.** A test asserting `window_to_native` over an egui-shaped rect agrees with the
frontend's own inverse on the same geometry; `pick.rs`'s existing `bus_parity` guard run from the player
(it is a pure function of the VDP, so it ports verbatim); and — the load-bearing one — an assertion on
the *machine*, not on the reply: a spawn click at a known dot leaves the expected bytes in object RAM,
with an `assert_ne!` anti-vacuity clause, which is the exact gate shape `docs/lane-log.jsonl:103`
records as the only one that caught a fabricated-success mutation.

**Risk.** Low. One named seam: egui's `Rect` is `f32` in *points*, `present::Rect` is `usize` in
*pixels*. On a HiDPI display those differ by `pixels_per_point`, and getting it wrong is a picking
offset that is invisible at 1.0 scaling and wrong on the owner's actual display. **The conversion must
go through `ctx.pixels_per_point()` and must be tested at a non-1.0 value.**

### 3.2 S2 — the three aspect modes, and the picture as a first-class view

**What.** Replace `ui.rs:205-208`'s inline square-pixel fit with `present::dest_rect(w, h, src_w,
src_h, aspect)` and expose `Aspect::{Tv, Square, Integer}`. `Tv` is the frontend's **default** and the
player does not have it, so the player is today showing the owner a geometrically wrong picture by the
frontend's own standard.

**Proof.** `present.rs`'s existing `dest_rect` tests, run from the player; plus a test that the
picking inverse composed with the forward map is identity at each of the three modes — which is the
property S1's correctness actually rests on.

**Risk.** Low, but it interacts with S1: change the fit and the inverse must change with it. **Do S1 and
S2 in that order and re-run S1's identity test at the end of S2**, or do them as one slice.

### 3.2a S2a — the display mask must reach the picture

**What.** `oracle-frontend` has two pixel paths (§1.2): `blit_capture` normally, and `blit_masked`
whenever a layer is hidden, switched from inside `drain.rs:230`. `oracle-player` has neither the second
path nor a `Bus::layers()` accessor, so **a bus-set display mask changes the bus's answers and not the
player's picture.** That is `docs/OVERSEER.md`'s queued `GUI-LAYERS` item arriving as a migration
blocker: `pick.rs`'s masked-`resolve` invariant (§1.4) is *"the panel describes the picture"*, and it is
unsatisfiable in a window whose picture ignores the mask. **S1's picking is only correct once S2a
lands**, so if they are separated, S1 must be masked-off-only and say so.

**Also the cheapest four rows of §2.1's debt:** the frontend's four `ToggleLayer` palette rows are
generated from the core's `LayerMask::targets()` and map straight onto `emulator/set_layer_enabled`,
which the player's palette already reaches. Four checkboxes on the Screen tab, as
`docs/2026-09-03-debug-panels-design.md` §2.1 already books them.

**⚑ The constraint that is not negotiable here.** `render_scanline` — the one render that commits
sprite-overflow/collision latches and the R10 carry — **takes no mask and has no masked twin, so "a
display mask cannot perturb emulation" is enforced by the type system** (`docs/OVERSEER.md:789-793`).
The masked path is `render_line_masked`, a separate function, and the migration adds no mask parameter
to `render_scanline`. The cost of that separation is real and already accepted: the masked path is a
post-hoc re-render and loses every mid-frame palette effect, which is why the window announces it.

**Proof.** A test that a mask set through `emulator/set_layer_enabled` changes the uploaded
`ColorImage`, plus `pick.rs`'s existing masked bus-parity rows run from the player.
**Risk.** Medium — it is the one slice that changes what is on the glass for reasons the operator did
not directly ask for, and the announcement (the layer badge) has to come with it or the picture just
silently loses colour effects.

### 3.3 S3 — save states, `.srm`, F5 ROM reload

**What.** `save_state.rs` (629), `sram_file.rs` (349), and the F1/F5 reset-and-reload choreography from
`main.rs`. The choreography is the part that is not a file move: the module doc at `main.rs:159-175`
enumerates five things that must happen in order (flush the pending `.srm` *first*, re-apply it after,
re-derive the ROM fingerprint, rebuild the audio sink, re-read and re-validate the symbol table) and
each one exists because something was silently lost without it.

**Proof.** The frontend's own tests for these modules move with them (12 `save_state::tests::*` rows,
which the spike's gate confirms need the `vendor` symlink). Plus one new ordering test per hazard in
that list of five.

**Risk. Medium, and this is the first genuinely risky slice.** The player's `bus::drain` already does
some of this repair for *client-driven* reloads (`main.rs:582-588` and `bus.rs:696-719` name the capture,
the audio ring and the symbol cache). A window-driven F5 must go through the **same** door or the two
paths will diverge — and the CR-K parcel's own lesson (`docs/lane-log.jsonl:109`) is that `reload_rom`
was deliberately re-pointed at one implementation so "the two cannot answer one client differently about
one file in one millisecond." **Named seam: F5 must call the drain's repair path, not a second copy.**

### 3.4 S4 — `player.conf`

**What.** `config.rs` (934), nine persisted keys, debounced writes, `.bak` recovery, unknown-key
carry-through (`F-CONFIG-UNKNOWN-KEYS`).

**Named seam, and it is a real one.** The player already persists state — but through
`eframe`'s storage (RON, `layout.rs`), not through `player.conf`. **Two persistence mechanisms in one
process, with `scale` and window geometry plausibly in both**, is how a setting gets written by one and
read by the other. Decide explicitly: either `player.conf` keeps everything except the dock layout, or
everything moves into eframe storage and `player.conf` is migrated once and retired. **Do not let this
be decided by whichever slice lands first.**

**Proof.** A round-trip test through both stores asserting no key is writable from both.

### 3.5 S5 — the overlay layer

**What.** Toasts, the `PAUSED` banner, the spawn badge, the status line. In egui these are not a
composited pixel layer — they are `egui::Area`s over the Screen tab plus rows on the existing top bar.

**Named seam: `emulator/screen_text`.** Both windows serve it, from their own composed strings
(frontend `screen_text.rs`; player `screen.rs` + `main.rs:613-637`). Every new overlay string in the
player **must** reach `screen::snapshot`, or the method quietly starts under-reporting what is on the
glass — and the player's version is gated on `is_serving()` and published *after* `build_ui`
specifically so a client never reads text describing an unpresented frame. A toast added without
threading it through `build_ui`'s return value is a silent regression that no test of the toast alone
can see.

**And the readback needs a new surface kind, not just new strings.** `oracle-frontend`'s
`screen_text.rs` emits `Kind::{TitleBar, StatusLine, Toast}` with both `text` (source) and `rendered`
(post-truncation), so `rendered != text` *is* "it was cut". `oracle-player`'s `screen::snapshot`
(`screen.rs:264`) emits **exactly two** surfaces — `titleBar` and `statusLine`. **Porting toasts means
adding the `Toast` kind to the player's snapshot**, or every toast is invisible to
`emulator/screen_text` while being plainly visible to a human, which is the precise disagreement the
method exists to prevent.

**Also note the glyph trap, already paid for and easy to re-break:** `screen::Glyphs` does **not** use
`epaint`'s `Fonts::has_glyph`, because on egui 0.36 it calls `A` undrawable in monospace and `▶`
undrawable in proportional on a build that draws both (`screen.rs:145-163`). Any new overlay text goes
through `Glyphs::drawable`, never through `has_glyph`.

**And the clock changes underneath the constants.** §1.2: the frontend's toast TTL and paused dwell are
counted in **presented frames** (`TOAST_FRAMES = 150`, `FADE_FRAMES = 30`,
`PAUSED_BANNER_DWELL_FRAMES = 12`). The player repaints on demand via `request_repaint_after(tick.wait)`
(`main.rs:1052`), so a paused, idle player repaints at whatever the governor asks for and a toast timed
in frames means something different. **Re-express these in `Duration`, not in frames** — a mechanical
change that is invisible until someone pauses and wonders why the toast never leaves.

**Risk.** Medium. The visual work is easy; the readback contract and the clock are where the defects
live, and neither is visible in a screenshot.

### 3.6 S6 — gamepad, ROM browser, symbol watches, the remaining palette rows

**What.** `gamepad.rs` (468, `gilrs`, P1+P2 hotplug, OR'd with keyboard), `rom_browser.rs` (363),
`symbol_watch.rs` (534), and the ~30 `commands.rs` rows that §2.1 says must not be dropped when
`F-FRONTEND-PALETTE-BUS` closes.

**Risk.** Low each, but this is the long tail and it is where a migration stalls at "90 % done" for a
month. **Recommendation: land it as one slice with an explicit checklist derived from `commands.rs`'s
registry rather than from memory** — the registry is the cheat-sheet, by its own design
(`main.rs:53-54`), so it is also the completeness gate.

### 3.7 S7 — window identity, and the measurement that opens the gate

**What.** Window icon and WM class. `icon.rs` (246) reaches into minifb's X11 `Display` via `x11-dl`;
under eframe this is `ViewportBuilder::with_icon` plus `with_app_id` (Wayland) — a rewrite, not a port,
and `assets/oracle.desktop` + `assets/install-desktop.sh` already exist for the Wayland route.

**⚑ This slice is the one place the migration is a straight win rather than a cost.** On Wayland
minifb's `set_icon` is `unimplemented!()` — **it panics** — and no app id can be set at all, which is
the entire reason `--x11` exists and the reason `main.rs:1265` does
`std::env::remove_var("WAYLAND_DISPLAY")` before the window opens, under a load-bearing ordering comment
("nothing in this process has touched the display yet"). The owner's session is **KDE Plasma on
Wayland** (spike §2.1). So `icon.rs` (246 lines), `x11-dl`, `raw-window-handle`, the embedded
`assets/oracle-icon.argb` and the `--x11` flag are all **deleted, not ported**, and a panic path on the
owner's own compositor goes with them. Dropping `minifb` drops all three direct dependencies at once —
`raw-window-handle` and `x11-dl` were promoted to direct *only* so `icon.rs` could reach through it.

**And then the gate measurement.** See §4.

### 3.8 S8 — retirement

Delete `crates/oracle-frontend`, or reduce it to the lib the player consumes. **Not before §4's
measurement passes**, and not before §6's notifications have gone out.

---

## 4. The retirement gate, and where the measurement is taken

The hub's condition, banked at `docs/OVERSEER.md:1040-1046`: **`oracle-frontend` is not retired until
the migrated player shows 60 fps and audio pacing measured on the REAL player under the toolkit, in the
same form as the spike doc, and the owner's window keeps working across the switch.**

**The measurement is taken after S7 and before S8, on the fully-featured player** — not after S1, and
not on a stripped build. The reason is specific: the spike measured 0.22 ms median / 0.66 ms p99 with
*two placeholder panels* (`docs/2026-09-02-player-pacing-design.md` §3.2 — "Registers (placeholder) —
20 monospace rows … there to *cost* what a real panel costs"). After S1-S6 the same frame carries eight
real panels, a picture, an overlay layer, and the 4 ms Aether pump. **The number that matters is the one
taken with all of it on.**

**The form it must take**, copied from the spike and the pacing doc so it is not re-invented:

* `--mode bench-cpu`, 75 s, real audio device at gain 0.0 — the display-independent pass. Report
  emulated fps, median frame period, steady-state audio starvations, producer drops, **each run
  reported separately and never averaged** (pacing design §4.1).
* `--mode bench-window`, 75 s — the real stack.
* `--dock every-tab` **and** `--bench-arm`, both, or the panel-cost half of the measurement is a lie:
  `egui_dock` draws only a leaf's *active* tab, so three panels sharing a pane cost one body; and the
  three stopping panels are empty until something is armed (`oracle-player/src/main.rs:100-113`).
* A control run: the same binary with the governor off (`--target-fps 0`), which the pacing doc
  reproduces at 324.3 fps and 23.3 M discarded samples. **An absence needs its control.**
* Every timing figure ships with a wall-clock uptime and the machine's condition, stated rather than
  claimed (pacing design §4.0).

**The instrument that makes this safe to run while the owner is at the machine** already exists and must
be reused rather than re-derived: `crates/oracle-panels-spike/run.sh` starts its own
`Xvfb :77 -screen 0 1281x803x24`, strips `WAYLAND_DISPLAY`/`XDG_SESSION_TYPE`, and passes
`--expect-screen 1281x803`; **the guard is not the environment but the binary asking the toolkit for its
own monitor size on the first frame and `exit(2)`ing on a mismatch before drawing anything** (spike
§2.1). `oracle-player` already carries `--expect-screen`. The geometry is deliberately a size no real
monitor is, so the check is a discriminator rather than a coincidence.

**⚠ And the one thing this instrument structurally cannot answer, carried forward from the spike:**
presented fps under vsync on the real GPU. Xvfb has no vsync and llvmpipe is not the machine's GPU
(spike §4.1, pacing design §4.4, both TAGGED for a foreground pass). **The gate's "60 fps" must
therefore be stated as 60 *emulated* fps with a bounded audio ring — which is what both prior documents
actually measured — or the foreground pass must be run by the owner.** See §7.2.

---

## 5. Cost and risk summary

| Slice | Lines moved | Cost | Risk | The named seam |
|---|---|---|---|---|
| S0 lib target | ~30 new, ~10 `pub` | XS | Low | Two crates compiling one file twice if it is refused; a lib without the `oracle-aether` edge silently deletes `pick.rs`'s `bus_parity` guard |
| S1 picking + spawn | ~150 new | S | Low | egui points vs pixels: `pixels_per_point` |
| S2 aspect | ~40 | XS | Low | Changing the fit invalidates S1's inverse |
| S2a display mask reaches the picture | ~120 | S | **Medium** | S1's "the panel describes the picture" invariant is unsatisfiable without it; `render_scanline` must gain no mask |
| S3 save states / `.srm` / F5 | ~1,000 moved | M | **Medium-high** | F5 must go through `bus::drain`'s repair, not a second copy |
| S4 `player.conf` | ~950 moved | M | **Medium** | Two persistence stores; decide the split before either slice lands |
| S5 overlay | ~600 rewritten | M | **Medium** | Every string must reach `screen::snapshot` (which needs a new `Toast` kind), or `screen_text` under-reports; frame-counted TTLs must become `Duration`s |
| S6 gamepad / browser / watches / palette rows | ~1,400 moved | M | Low | Completeness — gate on `commands::registry()`, not on memory |
| S7 identity + gate measurement | ~250 **deleted**, ~20 new | XS | Low | Wayland `app_id` vs X11 `WM_CLASS` — and this slice removes a Wayland panic path rather than porting one |
| S8 retirement | deletion | XS | — | §6 must have gone out first |

**The two risks that are not in any one slice:**

1. **`oracle-frontend` is still receiving features.** `git log --oneline -- crates/oracle-frontend/src/`
   shows 31 commits since 2026-08-25, the newest on 2026-09-04 (`21c99ac`, `ac43e14`, `70860cf` — the
   spawn picker and the §11.27 colour caveat, all landed *this week*). A migration against a moving
   target re-ports. **Decide explicitly at S0 whether the frontend is frozen to bug-fixes.** If it is
   not frozen, S0's shared lib is no longer a nicety — it is the only thing that keeps a feature landed
   in one window from being absent in the other.
2. **egui is a fast-moving dependency.** The spike's §7 records six compile errors from a 0.32→0.36
   API shift (`App::update` → `App::ui`, `Context::run` → `run_ui`, `TopBottomPanel::top` → `Panel::top`,
   panels take `.show(ui, …)`, no `NativeOptions::vsync`), and 233 new crates in the graph. Budget that
   churn on every bump.

---

## 6. ⚠ Cross-lane obligation — and it is narrower than it looks

`docs/OVERSEER.md:1047-1051`: *"aeon reloads into the owner's window BY SOCKET, so tell aeon and the hub
the day the binary name or socket path changes. A wrong process name has already cost a night of
'window closed' reports."*

**What does NOT change, measured here so nobody spends a day on it:**

* **The socket path.** Both binaries resolve the same default chain — `$ORACLE_SOCKET`, `$EXODUS_SOCKET`,
  `$XDG_RUNTIME_DIR/oracle.sock`, `/tmp/oracle.sock` (`oracle-player/src/main.rs:135-138`), and the
  player carries the frontend's three-state nesting *unchanged and deliberately* (`main.rs:112-121`).
* **The handshake identity.** `server_name` has exactly three references in `crates/`
  (`engine.rs:167`, `:208`, `:2627`); **neither binary overrides it.** Both answer
  `serverName: "oracle-next"`, `implementation: "oracle-rs"` (`build_info.rs:29`). aeon's
  `tools/aether_instance.py` asserts on precisely those two fields, in two rungs, and **both rungs pass
  unchanged after the migration.**

**What DOES change, and is therefore the whole of the notification:**

1. **The process name.** `oracle-frontend` → `oracle-player`. Anything discriminating by `ps`/`pgrep`
   breaks. (This is also the shared-machine hazard `docs/OVERSEER.md`'s `F-SHIM-SOCKDIR-RESIDUE` entry
   warns about from the other direction — reaping by pattern once nearly killed this lane's own
   in-flight parcel.)
2. **The launch command.** Different flags: the player requires `--rom PATH` and takes
   `--mode`/`--size`/`--dock`/`--bench-arm`; the frontend takes `--scale`/`--aspect`/`--x11`.
3. **Stale references in peer trees.** `aeon/tools/evict_witness.py:37,51` hardcodes
   `/run/user/1000/oracle.sock` and its docstring still says *"Requires one running `oracle_gui`"* —
   which is the legacy C++ binary, retired. That comment is already wrong; the migration makes it wrong
   in a second way.

**Recommendation: send the notification at S0, not at S8.** It costs nothing to say "the game window is
migrating to `oracle-player`; the socket path and the handshake are unchanged; the process name and the
launch flags change" months early, and the failure mode it prevents is a night of false "window closed"
reports.

---

## 7. What I could NOT determine, and the instrument each needs

Per the brief, this section is as valuable as the plan. Nothing here is padding and nothing is omitted.

### 7.1 ⚑ Whether the surfaces this migration *adds* fit in the margin — **the thing I am least sure about**

**First, a correction to my own draft, and it is the constraint-D lesson landing on me.** I initially
wrote that the eight real panels "have never been measured at 60 Hz beside a running game." **That is
false.** `docs/2026-09-03-debug-panels-design.md` §5.7.1 measures four panel configurations at 4500
samples each, and §5.7.2 splits the result. It found a **real 14.8 ms stall** — the Watchpoints hit log
at `ui-build` **15.220 ms median**, driving the player to 51.6 emulated fps, 1754 governor rebases, 1353
steady starvations and **10.6 seconds of silence in a 75-second run** — and repaired it by virtualising
the list rather than by moving `Machine` to a thread, back to **0.573 ms**. The parts were then checked
against the whole to 0.084 ms. **So the panels are measured, a failure of exactly the feared kind was
found in them, and it was fixed.** Quoting a stale worry as an open one is the failure this document's
§2 exists to prevent, and I nearly committed it.

**What remains genuinely open is narrower.** Every one of those measurements was taken on today's
surface set. The migration adds per-frame work that has never been in a measured frame: the overlay/toast
layer (S5), a hover callout that resolves attribution under the cursor *every frame* (§3.5, the frontend
does this at `main.rs:2493-2496`), the masked pixel path (S2a, a full re-render rather than a capture
read), and four layer checkboxes. The hover callout is the one I would bet on being the problem: it is
per-frame, not per-gesture, and `docs/2026-09-03-debug-panels-design.md` §5.8.1's rule is that
`bus-pump` stays trivial precisely because everything expensive is per-gesture.

Two further facts sharpen the question rather than settle it: the default dock has **four leaves**, so
only four of the eight bodies run on a given frame (`ui.rs:1695`, and `nav.rs:12-18` corrects an earlier
"six" claim) — which is why `--dock every-tab` exists and why a measurement without it measures half the
panels; and the player deliberately runs `RENDER_LOW_WATER_FRAMES = 2` against the frontend's
`LOW_WATER_FRAMES = 1` (`pacing.rs:70-87,113`), buying ~32 ms of stall margin for ~17 ms of latency.
The margin is real but it has already been spent once.

**The instrument:** `oracle-player --mode bench-window --secs 75 --dock every-tab --bench-arm` under
`crates/oracle-panels-spike/run.sh`'s Xvfb-plus-`--expect-screen` guard, run **before and after each of
S2a and S5**, and compared as a delta the way §5.7.2 does — never as a single absolute number. Every
flag already exists; nothing needs building. **I did not run it** — constraint A forbids launching a
windowed binary from a background agent, and this worktree has no `target/`, so it would also be a cold
466-package build on the owner's live machine. **TAGGED for foreground follow-up.** If a new surface
does eat the margin, the escape hatch is already designed and named: `Machine::step` is the
deliberately toolkit-free seam and moves behind a frame channel on its own thread
(`pacing.rs:89-99` — *"if a debug panel is ever measured to stall the UI thread, the fix is to put
`Machine` behind a frame channel on its own thread — not to raise the low-water mark again"*).

### 7.2 Presented fps under vsync on the real GPU

Unresolved since the spike (§4.1) and re-tagged by the pacing design (§4.4). Xvfb has no vsync and
llvmpipe is not the machine's GPU, so **no headless instrument can answer it** — this is a reach limit,
not an omission. It matters for the retirement gate specifically: §4's gate says "60 fps", and what the
existing instruments measure is 60 *emulated* fps with a bounded ring. **The instrument is the owner's
own display, in the foreground.** Until then the gate must be worded as emulated-fps-plus-audio, or it
is claiming something nothing has measured.

### 7.3 Input latency, resize, DPI and multi-monitor under eframe

Spike §4.3: "not exercised headless." DPI is not academic here — §3.1's one named risk is exactly the
points-vs-pixels conversion, and a picking offset caused by `pixels_per_point` is invisible at 1.0 and
wrong on the owner's actual `4764x1600` display. **Instrument: a foreground run on the real display with
a known dot clicked and its reported coordinate checked.** Cannot be done headless because Xvfb's
scaling is chosen by the harness, so a test there would confirm the harness rather than the display.

### 7.4 Whether the frontend's `--x11` escape hatch is still needed

`main.rs:1250-1257` carries `--x11` "because that path is the fragile one" (spike §6.2). Whether eframe
on this machine's Wayland session needs an equivalent, and whether `with_app_id` gets the WM class right
where `XSetClassHint` currently does, is a **run-time property of the session**, not readable from
source. **Instrument: launching the player on the owner's live Wayland session and reading the WM
class.** Explicitly out of bounds for this task.

### 7.5 What the owner actually wants the game picture to *be*

I can price "the picture becomes a dockable tab" (it already is one) and I can price "the picture
becomes its own always-visible viewport" (eframe supports a second viewport, cost unpriced). **I cannot
determine which he wants, and the difference is not cosmetic** — a tab can be closed, hidden behind
another tab in the same leaf, or made 200 px tall by a drag, and a game window that can be accidentally
hidden by the debugger is a different product. `docs/OVERSEER.md:298`'s open question (*"which window
did he mean?"*) is adjacent to this but not the same question. **Instrument: one word from the owner.
Recommend folding it into the same read-back that is already parked in `lane-status.json`'s `awaiting`.**

### 7.6 Two things I deliberately did not verify by running

* **No `cargo check` / `cargo test` was run.** Cold worktree, no `target/`, docs-only change, owner on
  the machine. Every source claim above is a read, and every one carries its `file:line` so it is
  checkable without a build. Stated rather than implied.
* **No emulator MCP tool was touched** (constraint B). Nothing above needed one; where a live machine
  would have been the better instrument it is tagged in this section rather than attempted.

### 7.7 One thing this document asserts on someone else's measurement

Every performance number in this document is **quoted, not reproduced here**, and attributed at each
use: the spike's 0.22/0.66 ms and 60.03 fps; the pacing doc's 60.037 emulated fps, 16.666 ms median,
324.3 fps governor-off control, 23.3 M discarded samples, 283/256 ms render stalls survived; and the
panels design's §5.7 configuration matrix (`ui-build` 15.220 → 0.573 ms, 51.6 fps, 1353 starvations,
10.6 s of silence, the parts-to-whole check at 0.084 ms) and §5.8.1's `bus-pump` worst of 0.169 ms
against a 4 ms budget. The hub verified the spike independently before ruling
(`docs/OVERSEER.md:1041-1043`); **this document adds no verification of its own to any of them**, and
§7.1 records that I initially mis-stated the panels-design measurements as absent before checking.

---

## 8. The recommended order, in one line

**S0 (lib target + freeze decision + notify aeon) → S1 (picking) → S2 (aspect) → S2a (display mask) →
S3 (save states) → S4 (config) → S5 (overlay) → S6 (long tail) → S7 (identity + the gate measurement) →
S8 (retire).**

S1, S2 and S2a are one arc and are best landed as one parcel or three same-week parcels: S2 changes the
fit S1 inverts, and S2a is what makes S1's *"the panel describes the picture"* invariant true rather
than conditional. If they must be separated, S1 ships masked-off-only and says so in its own doc.

Close `F-FRONTEND-PALETTE-BUS` at S0 with a replacement row booked against the player. Book
`F-STATUS-CAVEAT-NOT-ON-STRIP` into the queue properly — it is not in it — and land it with S7.
`F-FRONTEND-NO-STATUS` and `F-SPAWN-PICKER-PANEL-SURFACE` close on their own as S1 and S8 land.
