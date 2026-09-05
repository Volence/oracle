# Frontend migration S0-S2 — the calls made, and what S2a inherits

**Date:** 2026-09-05 · **Branch:** `frontend-migration-s0-s2`
**Plan:** `docs/2026-09-05-frontend-migration-recon.md` §3, slices S0, S1, S2.
**Out of scope by instruction:** S2a (the display mask reaching the picture). §8's consequence is honoured
literally — **S1 ships masked-off-only and says so**, in `crates/oracle-player/src/screen_pick.rs`'s own
module doc and in the refusal a person reads on the glass.

**No windowed binary was launched. No emulator MCP tool was touched.** Everything below is `cargo check`,
`cargo test`, `cargo clippy` and `cargo fmt`.

---

## 0. What the recon got wrong

The recon corrected three of this repo's own booked measurements, so it earns the same treatment. Three
things in it are wrong or incomplete; the first is the one that changed this parcel's shape.

### 0.1 §3.0(b)'s module list is not a closed set — six of its nine cannot move

§3.0(b) recommends *"add `crates/oracle-frontend/src/lib.rs` exporting `config`, `save_state`, `sram_file`,
`symbol_file`, `symbol_watch`, `rom_browser`, `present`, `pick`, `commands`."* Measured by enumerating every
`crate::` path in `crates/oracle-frontend/src/`:

| Module | `crate::` edges out | Movable at S0? |
|---|---|---|
| `present` | `crate::font` | yes, with `font` |
| `pick` | none | yes |
| `font` | none | yes |
| `spawn` | none in code (three in doc links) | yes |
| `save_state` | `crate::sram_file` | yes, as a set with `sram_file`/`symbol_file` |
| `symbol_watch` | none | yes |
| `config` | **`crate::gamepad_default_deadzone`** — an item defined in `main.rs` | **no** |
| `commands` | `crate::lens` → `crate::overlay` → `crate::spawn` + `crate::screen_text` | **no** |
| `rom_browser` | `crate::commands`, `crate::palette` → `crate::overlay` | **no** |

`config`, `commands` and `rom_browser` each reach code that stays in the binary, and two of the three reach
**items defined in `main.rs` itself** (`crate::gamepad_default_deadzone`; and transitively `crate::main`,
`crate::blit_masked`, `crate::notify` through `drain`/`bus`). Moving them means first cutting an edge back
into the run loop, which is S3-S6's work. Attempting the full list at S0 would have turned an XS slice into
the whole migration.

**So S0 shipped the closed subset S1/S2 actually consume:** `audio`, `font`, `pick`, `present`, `spawn`. The
`save_state`/`sram_file`/`symbol_file`/`symbol_watch` set is *also* closed and was deliberately left for S3,
where it is needed — moving it now would be churn in `main.rs` with no consumer.

### 0.2 §5's S0 risk row understates the fix and overstates the residual

§5 prices S0's named seam as *"two crates compiling one file twice if it is refused; a lib without the
`oracle-aether` edge silently deletes `pick.rs`'s `bus_parity` guard."* Both halves are right, and the second
is the trap this parcel was warned about — but the recon presents them as risks to be *carried*. They are
both closed by one structural choice: **the lib target is a `lib.rs` inside `oracle-frontend`, not a new
crate.** A lib target inherits its crate's dependency list, so the `oracle-aether` edge is not something S0
had to remember to add; it is not possible for S0 to have dropped it. See §1.2 for how that was proven rather
than asserted, and for the tripwire that makes it stay true.

### 0.3 §3.2's "changing the fit invalidates S1's inverse" does not arise under egui

§3.2's stated risk is *"it interacts with S1: change the fit and the inverse must change with it… re-run
S1's identity test at the end of S2, or do them as one slice."* That is exactly right for `oracle-frontend`,
where `present::window_to_native` is handed a rect the frontend **re-derived** from the window size, and a
fit change that missed the inverse would leave two derivations disagreeing.

Under egui it should not arise, and the reason is structural rather than lucky: `Response::rect` is **the
rect egui actually laid the image out in**, so S1's inverse reads the drawn geometry instead of re-deriving
it. S1 is therefore written with the fit and the inverse as two functions that do not share a derivation at
all — `fit` decides a size, `dot_at` inverts whatever was drawn. **Whether that holds is S2's measurement,
recorded in §3 when S2 lands**; the slices are still ordered S1→S2 with S1's identity test re-run at the
end, because "did not arise" is a thing you establish by checking.

---

## 1. S0 — the lib target

### 1.1 What landed

* `crates/oracle-frontend/src/lib.rs` — a lib target exporting `audio`, `font`, `pick`, `present`, `spawn`.
* `crates/oracle-frontend/Cargo.toml` — `minifb`, `x11-dl` and `raw-window-handle` are now **optional**,
  behind a new default-on `window` feature, and an explicit `[[bin]]` carries `required-features =
  ["window"]`. `gilrs` was already optional.
* `crates/oracle-frontend/src/main.rs` — the five moved modules are `pub(crate) use`d at the crate root
  instead of `mod`-declared, so every `crate::present` / `crate::font` / `crate::pick` / `crate::spawn` path
  in `overlay.rs`, `palette.rs`, `screen_text.rs`, `lens/*` and `bus.rs` resolves exactly as before.
* `crates/oracle-player/` — depends on `oracle-frontend` with `default-features = false, features =
  ["audio", "aether"]`, and the `#[path = "../../oracle-frontend/src/audio.rs"] mod audio;` include is gone.
* `crates/oracle-frontend/src/spawn.rs` — the **spawn choreography** moved here from `bus.rs`: a `Caller`
  trait (dispatch one method; resolve one symbol) plus `archetypes`, `act_bounds` and `place`. See §1.5.

### 1.2 The `window` feature is the answer to the objection that blocked this edge

`oracle-player/src/main.rs` carried a standing argument against exactly this dependency: *"giving
`oracle-frontend` a `lib` target drags `minifb`, `x11-dl` and `gilrs` into this crate's graph to reach one
file."* That was true and is now false, because those three are optional and off in the player's edge.
Measured, not assumed:

```
cargo tree -p oracle-player -e normal | grep -c '<crate> v'
  minifb  0
  gilrs   0
  x11-dl  1   <- winit -> egui-winit/eframe, NOT the S0 edge (cargo tree -i x11-dl)
```

and the two standing "no toolkit downstream" gates are untouched:
`cargo tree -p oracle-core -e normal | grep -icE 'egui|eframe|wgpu|winit'` = **0**, same for
`-p oracle-frontend` = **0**.

### 1.3 ⚑ Proving `bus_parity` survived — same rows, not a subset

The brief's trap: a lib carved out without the `oracle-aether` edge deletes `pick.rs`'s
`#[cfg(feature = "aether")] mod bus_parity` and stays green. Three separate things establish it did not.

**(a) Set equality of the whole test name list, not a count.** `cargo test -p oracle-frontend -- --list`,
piped through `grep ': test$' | sort`, was captured *before* the change and again after. `diff` of the two
files is **empty**: 350 rows before, 350 rows after, byte-identical names. A count alone could not
distinguish "the same 350" from "9 lost and 9 gained"; the sorted set diff can, and does.

**(b) The nine rows named, and the target they now run in.**
`cargo test -p oracle-frontend --lib pick::tests::bus_parity` → **9 passed, 0 failed**, running
`unittests src/lib.rs`:

```
pick::tests::bus_parity::a_named_tile_states_its_space_and_never_another_models_slot
pick::tests::bus_parity::the_answer_says_a_layer_is_hidden_and_says_it_only_then
pick::tests::bus_parity::the_bus_refuses_a_dot_the_panel_would_still_answer
pick::tests::bus_parity::the_panel_and_the_bus_agree_on_plane_cells_and_on_the_backdrop
pick::tests::bus_parity::the_panel_and_the_bus_agree_under_every_mask_that_changes_the_answer
pick::tests::bus_parity::the_panel_and_the_bus_carry_the_same_colour_caveat
pick::tests::bus_parity::the_panel_and_the_bus_name_the_same_sprite_tile_and_sat_entry
pick::tests::bus_parity::the_panel_answers_at_the_clock_it_is_given_not_the_vdps
pick::tests::bus_parity::the_panel_stays_silent_when_the_colour_cannot_have_changed
```

**(c) The rows are live, proven by mutation.** `pick.rs:153`'s `tile_range` was mutated **on disk** to
`let lo = ((u32::from(tile) + 1) * TILE_BYTES) & VRAM_MASK;` (`git diff --stat` showed the file changed),
and the guard went red with an address-level message:

```
the panel arms $0220 but the bus names "0x00000200" — the two have DRIFTED
2 failed: ..._name_the_same_sprite_tile_and_sat_entry, ..._agree_on_plane_cells_and_on_the_backdrop
```

The line was then restored and `git diff --stat crates/oracle-frontend/src/pick.rs` was **empty** —
restoration to the committed baseline, verified rather than assumed — and the nine rows went green again.

**(d) A tripwire for the next time.** `crates/oracle-frontend/tests/lib_target_keeps_the_bus_parity_edge.rs`
is one address-level parity row that lives in `tests/`, so it links the **lib target** and names
`oracle_frontend::pick` and `oracle_aether::engine::Engine` in one compilation unit. If a later slice moves
`pick` into a crate that cannot see `oracle-aether`, this stops compiling — which converts the silent
failure into a loud one. It is deliberately **one dot** rather than a copy of the sprite sweep: duplicating
`bus_parity` would be two implementations of one claim, drifting. Red-first proven with the same mutation
(`the panel arms $0AC0 and the bus names "0x00000AA0"`), then restored and re-run green.

### 1.4 `F-FRONTEND-PALETTE-BUS` — closed, and the replacement row's text

§2.1's ruling is adopted unchanged: **close it, do not build it.** Its blocker (*"it needs a free-text
argument mode the current design lacks"*) is a true statement about `oracle-frontend`'s `Cmd`-is-`Copy`
state machine and a dissolved one about `oracle-player`, whose palette has been two `TextEdit::singleline`s
over `METHODS` since it shipped.

Closing it takes on the debt §2.1 names, so here is **the replacement row, booked against the player**, in
the form `docs/lane-status.json`'s `queue` takes. (Written here as a paragraph rather than as an edit to
that file: the file is deliberately uncommitted and this worktree's copy is stale, so an edit from here
would be written against a queue that no longer exists.)

> **`F-PLAYER-PALETTE-NO-ACTIONS`** — *The player's palette lists served methods and no player actions.*
> `oracle-player/src/palette.rs` is built by filtering `METHODS`, so everything the **window** does rather
> than the **machine** is unreachable from it. `oracle-frontend/src/commands.rs::registry()` yields **42**
> rows in a default build and they are frontend actions, not bus methods: pause, step, reset, save state,
> load state, 10 slot selects, volume up/down, mute, audio filter, aspect, status line, 7 lens toggles, 4
> layer toggles, ROM browser, spawn mode, quit. 29 carry a hotkey and 31 are visible. Migration slices
> S3-S6 must land each of them somewhere in the player — a palette row, a nav-menu item, or a hotkey — or
> they are silently dropped when `oracle-frontend` retires, and nothing today would notice.
> **The completeness gate is `commands::registry()` itself, not a list written from memory**: it is a
> headless function, `oracle-frontend` now has a lib target, and a test in the player can walk it and assert
> every row is claimed or explicitly waived. Four rows close for free at S2a: the `ToggleLayer` family is
> generated from the core's `LayerMask::targets()` and maps straight onto `emulator/set_layer_enabled`,
> which the player's palette already reaches.
> *Blocked on:* nothing. *Closes when:* S6 lands with that walk-the-registry test green.

Note the row deliberately does **not** ask for `Cmd::BusMethod` in `oracle-frontend`. §2.1's third reason
stands: adding a free-text mode to a crate scheduled for retirement is a migration bought twice.

### 1.5 The spawn choreography moved with the module, and that is the point of S0

`oracle-frontend/src/bus.rs` held `Bus::archetypes`, `Bus::act_bounds`, `Bus::read_u16` and `Bus::spawn_at`
— roughly 130 lines carrying the world join, the `worldSource`-refused-rather-than-guessed rule, the
resolve-by-name-every-time rule, and the `F-SPAWN-OUTSIDE-ACT` act-bounds gate with its refused-not-clamped
ruling. S1 needs every one of those in the player.

Copying them would have been the failure the lib target exists to prevent, and this repo has already paid
for it once: CR-K re-pointed `reload_rom` at one implementation so *"the two cannot answer one client
differently about one file in one millisecond"* (`docs/lane-log.jsonl:109`). The stakes are higher here,
because this choreography is what stands between a click and an **acked-then-silently-culled** spawn.

So the bodies moved into `spawn` and the seam is a two-method `spawn::Caller` trait: `call` (dispatch a
served method synchronously, hand back the tool's own reply or its own refusal) and `address_of` (resolve
one symbol, fresh). Neither window's `Bus` type is nameable from `spawn`, and neither needs to be — the two
windows host their `Host` differently (`oracle-frontend` pumps, `oracle-player` does not), and this is the
part they genuinely share. `oracle-frontend/src/bus.rs` keeps a private `HostCaller` adapter and three
one-line delegations; `Bus::act_bounds` was **deleted** rather than kept as a delegation, because after the
move nothing in that crate called it and a public method with no caller is a second surface to keep in step.

The move is behaviour-preserving by construction (the bodies are the same text) and covered by the frontend's
existing spawn tests, which did not change and did not move.

---

## 2. S1 — click-picking and the spawn picker on the Screen tab

### 2.1 What landed

`crates/oracle-player/src/screen_pick.rs` (new) holds everything about what a click *means*; `ui.rs`'s
`Panels::screen` holds only where the picture goes and what the pointer did. `bus.rs` gained one accessor,
`Bus::layers()`, delegating to `Host::layers()`.

The `Response` that `ui.rs:210` used to discard is what `docs/OVERSEER.md:298` correctly identified as the
reason this tab could not receive a click. It now carries the two things minifb never told `oracle-frontend`
— **where the image actually landed** and **where the pointer was** — in the same space.

The rect is allocated explicitly (`allocate_exact_size` + `Rect::from_center_size` + `Image::paint_at` +
`Ui::interact`) rather than through `centered_and_justified`, because the whole gesture rests on the rect
being the *picture's*: a justified layout may hand a widget more room than it asked for, and a click
inverted against a rect a few points wider than the picture is an offset nothing on screen explains.

### 2.2 Two routes out of one gesture, and D15 is the reason for both

* **Resolving the dot is a read** and goes straight to the core: `pick::resolve` over the VDP the loop
  owns. `pick.rs`'s module doc argues that out and `bus_parity` holds it to it.
* **Arming the watch, and spawning, are per-gesture commands** and go through `Bus::call` → `Host::call` —
  `emulator/watchpoint_clear` for the retire, `emulator/watchpoint_add` for the arm, and
  `spawn::place`'s three calls for a placement. Synchronous, in-process, no socket. The point of going
  through the server is that a click gets the tool's exact reply *and its exact refusal*: the watch cap's
  `watchCapReached` and all five §11.32 refusals arrive whole and are shown whole.

Retiring uses the **handles this panel holds**, never `{all: true}`, because a blanket clear would take a
socket client's watches with it — the shared-instrument hazard `oracle-frontend` learned the hard way.

### 2.3 ⚑ Masked-off-only, and why it is a refusal rather than a caveat

§8's instruction, honoured literally. The player has no `blit_masked` — `bus.rs`'s own `framebuffer` doc
already says so — so a bus-set mask changes the bus's answers and not this window's picture. Under a mask
the tab cannot satisfy `pick::resolve`'s *"the panel describes the picture"* invariant either way round:
resolve **with** the mask and the answer describes a picture nobody is looking at (carrying a
`planeA hidden, so this is the masked picture` clause that is false of this glass); resolve **without** it
and the answer is right about the picture but silently disagrees with what `emulator/pixel_attribution`
would tell a client about the same dot in the same instant — the exact drift `bus_parity` exists to
prevent, arriving through the one path that guard cannot see.

So **while any layer is hidden the click is refused**, in a sentence that names the hidden layers (read off
`LayerMask::hidden()`, never listed), names the slice that fixes it, and names both ways out. A caveat was
considered and rejected: a caveat on an answer still gives the answer, and the failure mode here is a
confident wrong tile address, not a missing disclaimer.

The gate is read **before** anything is resolved or retired, so a refused click leaves the previously armed
watch exactly where it was — asserted, not intended, by
`a_masked_off_refusal_changes_nothing_on_the_machine`.

### 2.4 The tests, and what each would catch

Nine rows in `screen_pick::tests`, `cargo test -p oracle-player screen_pick`:

| Row | Catches |
|---|---|
| `the_click_inverse_goes_through_pixels_per_point_at_every_scale` | the §3.1 risk: a points/pixels conversion that is invisible at 1.0 and wrong on the owner's display. Swept at ppp 1.0/1.5/2.0, expectations derived as `floor(n * ppp / k)`. |
| `the_fit_and_the_click_inverse_are_inverses` | the fit and the inverse drifting. Forward direction is `present::native_rect_to_window`, so the two halves of one blit are asserted against each other. |
| `a_pointer_off_the_picture_resolves_to_nothing` | a clamp instead of a rejection — which would arm the corner tile every time somebody clicked the letterbox. Both edges, plus the first dot inside each, so it is a boundary and not a blanket refusal. |
| `a_degenerate_scale_answers_nothing` / `a_degenerate_panel_yields_no_picture` | NaN and zero geometry naming a dot or panicking. |
| `the_masked_off_only_refusal_names_the_layers_the_mask_hides` | a refusal with layer names written into it rather than read off the mask; swept over every `LayerMask::targets()` entry, with the all-shown control asserted beside it. |
| `nothing_is_refused_while_every_layer_is_shown` | the gate becoming a wall. |
| **`a_click_arms_the_clicked_tile_on_the_machine_and_the_next_click_replaces_it`** | ★ the load-bearing one — see below. |
| `a_masked_off_refusal_changes_nothing_on_the_machine` | the gate firing *after* the retire. |

**★ The load-bearing row asserts on the machine, not on the reply**, which is the gate shape
`docs/lane-log.jsonl:103` records as the only one that caught a fabricated-success mutation. It drives
`Panel::click` end to end and then reads the result back **off the shared instrument through
`emulator/watchpoint_list`** — so a panel that composed a lovely sentence and armed nothing fails. Its
expectations are derived (`A_TILE * 32`; the backdrop register's entry × 2), not pasted back from a run.
Its **anti-vacuity clause** is the second click: a backdrop click must leave a CRAM entry and no VRAM watch,
`assert_ne!` against the first answer, because a `click` that became a no-op after the first would satisfy
"there is a watch" forever and satisfy nothing here. The empty-instrument control is taken first, while it
is still unambiguous.

*(Red-first proofs: see §2.6.)*

### 2.5 The spawn picker, and the standing badge

Spawn mode is a **control**, not a tab, on the Screen tab's own strip: arm (which lists the archetypes
through `emulator/lookup_symbol`'s prefix search **now**, never cached, because a stale archetype name
spawns the wrong thing rather than failing to spawn), cycle, disarm. The click branches spawn-then-pick
exactly as `oracle-frontend`'s run loop does, and for the reason it states: the two are the same gesture and
only one of them can have it.

The **badge** is drawn above the picture on every frame the mode is armed and names the archetype. That is
`oracle-frontend`'s rule carried over as a correctness requirement rather than as decoration, and it is above
the picture rather than below for the reason the halting alarm is on the top bar rather than in a tab: a
standing statement that can be cropped out of view is not standing.

The **choreography is not reimplemented** — it is `oracle_frontend::spawn::place`, the same function
`oracle-frontend` calls, reached through a `PlayerCaller` adapter. See §1.5.

One thing this window had to supply that the other already had: `spawn::Refusal::remedy` formats *"press
{k} to pause this window"* for the `machineRunning` refusal. `oracle-frontend` passes the key its registry
bound; this window has a **button**, so it passes `ui::PAUSE_LABEL` — derived from the label constant, not
transcribed, which is the rule the frontend's version states.

### 2.6 Red-first proofs

*(Pending — recorded below once run against the committed baseline.)*

---

## 3. S2 — the three aspect modes

*(See §3.1-3.3.)*

---

## 4. What S2a inherits

*(See §4.)*
