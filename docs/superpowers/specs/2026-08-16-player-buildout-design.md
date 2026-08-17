# Player build-out — dev-first on-glass UI (design)

Date: 2026-08-16. Status: approved in brainstorm (owner), pending spec review.
Frontier item: ranked 5 in `docs/2026-08-15-handoff-conformance-and-item19.md` §7
("group by subsystem, separate views from settings, a command palette rather than a menu at ~30 items").

## 1. Goal and north star

**Dev-first, pleasant for both.** The primary user is the developer building the game; the debug
surface is a first-class citizen reached without ceremony. "Enjoyable to play" is delivered by the
play path never being janky (sound, pads, settings that stick, fast-forward) — not by hiding the
dev surface. There is no technical tension between the two; the only contested resource is
defaults, and defaults go to the developer.

Baseline (verified in `crates/oracle-frontend/src/`, 2026-08-16): one window, ~20 hotkeys in a
match-chain in `main.rs`, a non-interactive OSD (toasts, status line, PAUSED banner), click-a-pixel
-to-watch, minifb + own scaler, **no menu, no config file, nothing persisted** (volume/aspect/
deadzone reset or are hardcoded every launch).

## 2. Scope

All four surfaces plus one addition that emerged in design:

1. **Command palette** — the control-surface spine.
2. **Lenses** — small non-modal debug overlays on the glass.
3. **Views** — large dockable instruments (heatmap, piano roll, VRAM browser).
4. **Settings + persistence** — config file; settings edited through the palette.
5. **Play comforts** — fast-forward, full rebinding; fullscreen deferred with a named trigger.

Zero `oracle-core` changes anywhere in this arc. The currency gate holds by construction.

## 3. Layering and input model

Compositing order onto the presented frame, bottom to top:

1. **Game picture** — untouched retained framebuffer, as today.
2. **Lenses** — non-modal, pure display; game runs and takes input.
3. **Views** — docked panels (see §6); game keeps running at a playable size beside them.
4. **Command palette** — the only modal layer. While open it captures all keystrokes; the game
   keeps running behind it (dev-first: the watch ticker stays live while you type) but game input
   is swallowed.
5. **Toasts + status line** — top, exactly as today.

Input routing is a priority chain: palette open → palette eats everything; else bindings resolve
hotkeys; else game pad-sampling. Gamepads always reach the game (a pad cannot type).

Key decisions:

- **Palette opens on `` ` `` (backtick)**, alias `Ctrl+P`. The Quake-console key: top-left, unused,
  no conflict with game keys (arrows / A,S,D / Enter).
- **`Tab` = soft reset**, honoring Gens/Fusion muscle memory (owner request). `F1` remains an alias.
- **`Esc` closes the topmost UI layer** (palette, then focused view). It no longer quits. Quit =
  window close button or the `Quit` palette command.
- No other default hotkey changes. The palette lists hotkeys; it does not replace them.

## 4. The command registry

One table is the sole source of truth for every frontend action:

```
Command { id: "state.save", title: "Save state to slot N", group: SaveStates,
          hotkey: Some(F2), action: ... }
```

Groups: `Game / Save states / Lenses / Views / Watch / Settings`. Everything derives from the
table: the palette renders it (grouped, hotkey column visible — the list doubles as the
cheat-sheet), the hotkey dispatcher reads bindings from it (replacing the ~300-line if-chain in
`main.rs`), and the first-launch hint comes from it. Adding a command in one place yields the
hotkey, palette entry, and searchability for free.

Palette behavior:

- **Empty query = grouped, browsable full list** (the palette *is* the menu; typing only filters).
  Discoverability has three layers: status-line hint ("press ` for commands") teaches the palette
  exists; the empty list teaches what the emulator can do; the hotkey column teaches the fast path.
- **Matching**: hand-rolled case-insensitive subsequence match (~20 lines). No fuzzy-rank library.
- **Recents**: last 3 used commands float to the top of the empty-query list.
- **Pickers**: commands needing an argument (e.g. `Select slot…`) open a second list in the same
  box (slots 0–9 with occupied/empty markers). **No free-text arguments in v1.**
- **Do-what-I-mean over greying out**: `Step one frame` while unpaused pauses *and* steps. A
  command is absent only when it cannot exist (audio commands in a no-audio build).
- **Rendering**: existing 8×8 font, overlay scale rules, translucent panel, capped rows + scroll.

## 5. Lenses

A lens is a named, toggleable overlay redrawn from live emulator state each frame, anchored to the
picture like the overlay. Each lens auto-registers its toggle command. The active lens set
persists across relaunch.

Build order:

1. **Watch ticker** — bottom strip streaming newest watch hits (`w0 vram $4A00 ← 3F @f811`) plus
   armed count. Reads the existing hit ring non-destructively; `W`/`C`/click-to-watch unchanged.
2. **Video lenses** — sprite outlines (from the decoded sprite table); **hover** = the callout tag
   (`slot 12 · tile $4A0 · pal 2 · pri 1`) via the same pixel-attribution call the click uses
   (hover explains, click arms — unchanged); CRAM strip (4×16 live swatches).
3. **CPU chip** — small top-right readout: PC as symbol (`Sonic_Move+$1C` from the loaded `.lst`),
   SR, frame counter. Auto-shows while paused/stepping; can latch on; palette command expands to
   the full D0–D7/A0–A7 block. Without a `.lst`, shows raw hex PC.
4. **Audio meters** — ten bars (FM1–6, PSG1–4) with per-channel mute/solo. **Last**, timed to land
   when sound work resumes so a real ear is on it. Contract-gated: see §9.

Deferred from v1 lenses: nothing else. (Plane/VRAM browsing is a View, §6.)

## 6. Views — dockable large instruments

A **View** is a large instrument opened from the palette's Views group. Views dock; they do not
take over:

- A view docks to a **side** — right or bottom; each view has a natural default (piano roll →
  bottom, heatmap → right). The game rescales into the remaining space (the scaler already handles
  arbitrary sizes) and **stays genuinely playable** — the whole point is watching the instrument
  while playing the thing that triggers it.
- **Divider drags** to resize. Dock side switches by drag or palette command.
- At most **one vertical + one horizontal dock** at a time (flexible without hand-rolling a window
  manager).
- **Tabs**: a dock holds multiple views as tabs, one visible per dock, tab strip to switch
  (owner request, from the old oracle's docked popups).
- **Layout persists** per view: dock side, split ratio, tab membership, active tab.
- **The click rule is universal**: in any view, clicking a thing arms a watch on it, same as
  clicking the picture.
- `Esc` closes the focused view (consistent with §3).

The framework ships in this arc proven by **one flagship view**; the others follow as their own
slices so none is built shallow:

1. **Write heatmap (flagship, this arc)** — VRAM tile space as a grid, cells glowing by write
   recency/volume, backed by the shipped census-watch capability (its first real consumer).
   Answers "what is stomping my art?"; click the hot cell → watch armed on the culprit's target.
2. **Piano roll (next, timed with sound work)** — ten lanes (FM1–6, PSG1–3, noise), pitch-placed
   note bars scrolling, decoded live from the FM/PSG event tap (fnum→note math exists from the
   synth arc). Pairs with audio-meter mute/solo. Contract-gated: see §9.
3. **VRAM tile browser (after)** — pageable tile grid with selectable palette, plane maps; the
   in-window Exodus viewer. Needs its own paging-UI design pass.

## 7. Settings and persistence

**File**: `$XDG_CONFIG_HOME/oracle/player.conf` (fallback `~/.config/oracle/player.conf`). Flat
hand-parsed `key = value` lines (~60 lines of code; no TOML/serde dependency). App-global
preferences only; per-ROM files (`.srm`, `.state0–9`, `.lst`) stay ROM-adjacent as today.

Persisted: volume, mute, aspect, window scale, gamepad deadzone, key bindings, pad bindings,
active lens set, view layouts (§6), status-line latch.

Write policy: on change (debounced, same pattern as the `.srm` autosave) and on quit. A corrupt or
unreadable file never crashes: renamed to `.bak`, defaults load, a toast says so. Unknown keys are
ignored with a warning toast (forward compatibility).

**The settings UI is the palette's Settings group** — no separate settings screens ("views
separate from settings" per the recon). Direct commands for volume/aspect/scale; pickers for the
rest.

**Rebinding** (owner priority): **every game control (D-pad, A/B/C, Start) and every emulator
hotkey is rebindable, keyboard and pad** — free because a hotkey is a data field on a command.
Flow: `Rebind: <command>` → capture modal "press a key… (Esc cancels)". Conflict detection names
the current owner of a contested key and offers to steal it. `Reset all bindings to defaults`
command as the escape hatch. The **deadzone picker shows a live stick meter** while choosing —
deliberately closing the owner-owed "deadzone 0.5 is an unfelt guess" item by construction.

## 8. Play comforts

- **Fast-forward**: hold `F` (rebindable) → run as fast as possible, capped 8×. The audio ring's
  master-clock role suspends while held; audio drops what the ring can't take and pacing snaps
  back on release. A `»»` marker shows while active.
- **Fullscreen**: deferred with a named trigger. minifb has no runtime fullscreen call; the WM
  fullscreens the window today. The UI layer speaks "pixel buffer + input events," so a later
  winit shell swap (trigger: one-exe fan-game distribution, or owner wants in-app toggle) touches
  none of this arc's code.
- **Pad quick-menu** (Start-hold, ~6 items for couch play): registered later slice, not v1.

## 9. Architecture

New modules in `oracle-frontend` (this arc shrinks `main.rs`, currently 2,363 lines):

- `commands.rs` — registry + dispatcher (the hotkey if-chain becomes data).
- `bindings.rs` — raw key/pad event → logical action via the rebindable map.
- `palette.rs` — modal state machine (closed → filtering → picker → capture), matcher, renderer.
- `lens/` — `watch.rs`, `video.rs`, `cpu.rs`, `audio.rs` behind one `Lens` trait
  (`draw(&System, &mut Frame)`); read-only over core state.
- `view/` — dock/tab framework + `heatmap.rs` (flagship).
- `config.rs` — flat-file parse/serialize, XDG path, debounced writer.

**Data flow per iteration** (skeleton unchanged): input → bindings → palette eats-or-passes →
emulate N frames (audio-clocked, or fast-forward) → lenses/views draw → overlay/toasts → present.
The Aether bus pump stays where it is; lenses and socket clients read the same instruments (watch
ring, sprite table) — nothing double-sourced.

**Contract gates (item 19 / D15 parity)**: the watch, video, and CPU lenses and the heatmap
consume capabilities that already carry Aether contract rows (sprites CR-18, pixel attribution,
registers, watchpoint census CR-11/12). Two future pieces do NOT and are **gated on contract rows
before building**: the **audio meters** (channel-state/mute has no bus row) and the **piano roll**
(the FM/PSG event tap has no bus row). Both are late in the build order; the gate costs nothing
now but must not be forgotten. Planning must also verify census counts are readable back through
`watchpoint_hits` in the shape the heatmap needs.

## 10. Error handling

- Corrupt config → `.bak` + defaults + toast (§7).
- Missing `.lst` → CPU chip falls back to raw hex PC.
- No gamepad / no audio device → those command groups absent; existing graceful-fallback pattern.
- Palette/lens/view drawing never touches the retained native framebuffer (same rule the overlay
  obeys today) — a UI bug cannot corrupt emulation state or the bus-published frame.

## 11. Testing

House style — pure functions tested directly, no UI harness:

- Matcher: table-driven subsequence cases.
- Registry invariants: unique ids, no duplicate default bindings, every group non-empty.
- Config: round-trip, corrupt-file recovery, unknown-key tolerance.
- Palette state machine: key-sequence → expected state/selection.
- Bindings: conflict detection, steal, reset-to-defaults.
- Lens/view draws: render into a scratch buffer, assert pixels (existing overlay-test pattern).
- Fast-forward pacing: frames-per-iteration decision as a pure function of ring state + FF flag.

**Every evidence-bearing test is mutation-verified at writing time, one recorded line each**
(standing practice; 3-for-3 vacuous tests passing gates was the measured base rate).

## 12. Phasing (implementation slices, in order)

1. **S1 — registry + bindings + palette** (the spine; hotkey chain becomes data; no visual change
   for existing keys).
2. **S2 — config file** (persistence for what already exists: volume, mute, aspect, slot strip
   latch; then bindings).
3. **S3 — lenses: watch ticker, video, CPU chip.**
4. **S4 — views framework + heatmap** (docks, divider, tabs, persistence).
5. **S5 — comforts: fast-forward; full rebinding UX (capture modal, conflicts, deadzone meter).**
6. **Later slices, each its own pass**: piano roll (with sound work; contract row first), VRAM
   browser, audio meters (contract row first), pad quick-menu, winit shell swap (on trigger).

## 13. Deliberately not doing

- egui/toolkit UI, multi-window (rejected in brainstorm — on-glass hand-rolled chosen).
- In-app fullscreen now (needs winit; trigger named in §8).
- Free-floating draggable panels (docks + tabs give the customizability without a window manager).
- Free-text command arguments, fuzzy-rank scoring, MRU persistence beyond session.
- Any `oracle-core` change, any Aether contract change (except the two named gates when their
  slices arrive).
