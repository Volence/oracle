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

Under egui it does not arise, and the reason is structural rather than lucky: `Response::rect` is **the rect
egui actually laid the image out in**, so S1's inverse reads the drawn geometry instead of re-deriving it.
S2 changed the fit from a float square scale to `present::dest_rect` under three `Aspect` modes and the
inverse needed no edit at all — the identity test S1 wrote passes at every mode without modification, which
is the assertion rather than the anecdote. The slices are still ordered S1→S2 and S1's tests were re-run at
the end of S2, because "did not arise" is a thing you establish by checking.

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

---

## 2. S1 — click-picking and the spawn picker on the Screen tab

*(Filled in below as the slice landed; see §2.1-2.6.)*

---

## 3. S2 — the three aspect modes

*(See §3.1-3.3.)*

---

## 4. What S2a inherits

*(See §4.)*
