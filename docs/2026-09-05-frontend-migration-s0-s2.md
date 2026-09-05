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
things in it are wrong or incomplete; the first is the one that changed this parcel's shape. Everything
else in it that this parcel touched held up — the `file:line` claims checked out, `present::window_to_native`
really is generic over a rect, `ui.rs:210` really did discard the `Response`, and `oracle-player` really has
no masked pixel path (its own `Bus::framebuffer` doc says so).

### 0.1 §3.0(b)'s module list is not a closed set — three of its nine cannot move at all

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

Under egui it did not arise, and the reason is structural rather than lucky: `Response::rect` is **the rect
egui actually laid the image out in**, so S1's inverse reads the drawn geometry instead of re-deriving it.
S2 replaced `fit`'s body — a float square scale became `present::dest_rect` under three `Aspect` modes —
and **`dot_at` needed no edit at all**. S1's identity row was re-run at the end of S2 and now sweeps all
three modes without its assertion changing, which is what turns "did not arise" into a measurement.

The ordering S1→S2 was still worth keeping, because that is how you find out.

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

**Red-first proofs: §2.6.** Three mutations, each shown applied on disk, each restored from the
committed baseline with `git checkout --` on an otherwise-clean tree and re-run green.

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

**Mutation 1 — the points→pixels conversion dropped.** `screen_pick.rs:133`
`let dx = (pos.x - image.min.x) * ppp;` → `let dx = pos.x - image.min.x;`:

```
test screen_pick::tests::the_click_inverse_goes_through_pixels_per_point_at_every_scale ... FAILED
test screen_pick::tests::the_fit_and_the_click_inverse_are_inverses ... FAILED
test result: FAILED. 7 passed; 2 failed
```

This is the recon's one named §3.1 risk, and note what the failure proves: it is **invisible at 1.0** and
caught only because the rows sweep 1.5 and 2.0 as well.

**Mutation 2 — the mask gate's early `return` deleted**, so the gate reports and does not stop:

```
test screen_pick::tests::a_masked_off_refusal_changes_nothing_on_the_machine ... FAILED
  assertion `left == right` failed: a refused click must leave the instrument untouched
test result: FAILED. 8 passed; 1 failed
```

**Mutation 3 — nothing is armed on the machine**, the arm loop's iterator sliced to `&p.targets[..0]`
while the description is still composed. This is the fabricated-success shape: the panel says a true
sentence about what it resolved and changes nothing.

```
test screen_pick::tests::a_click_arms_the_clicked_tile_on_the_machine_and_the_next_click_replaces_it ... FAILED
  assertion `left == right` failed: a plane click must arm exactly the 32-byte pattern of tile $055 in VRAM
   left: []
  right: [("vram", "0x00000AA0")]
test screen_pick::tests::a_masked_off_refusal_changes_nothing_on_the_machine ... FAILED
  assertion `left == right` failed: the precondition: one watch is armed
test result: FAILED. 7 passed; 2 failed
```

**And one mutation that was NOT strong enough, recorded because it is the more useful finding.** The first
attempt at mutation 3 kept the `watchpoint_add` call and dropped only `self.armed.push(h)`. That went red —
but on `assert_eq!(panel.armed_count(), 1)`, i.e. on the panel's own bookkeeping, with the machine-level
assertion still passing because the watch really had been armed. A weaker mutation than intended found a
weaker assertion than intended, and it would have been easy to write that up as "the machine-level row is
live". It was replaced with the one above, which removes the effect rather than the record.

---

## 3. S2 — the three aspect modes

### 3.1 What landed

`screen_pick::fit`'s body became `present::dest_rect(w_px, h_px, src_w, src_h, aspect)` — **the frontend's
own fit**, the same integer arithmetic and the same exact reduced-ratio derivation, rather than a second
implementation of 4:3 in the player. `Panel::aspect` selects it; the Screen tab's strip carries three
`selectable_label`s named by `Aspect::name()`, so the two windows cannot spell a mode differently.

**The player was showing a geometrically wrong picture by the frontend's own standard.** `Aspect::Tv` is
the frontend's default because a Mega Drive does not have square pixels, and the player had only the square
fit.

### 3.2 The one thing that is easy to get backwards, and the row that pins it

`Tv` is not "the wide one". 320x224 reduces to **10:7 ≈ 1.429**, which is *wider* than 4:3 ≈ 1.333 — so at
H40 the television picture is **narrower** than square pixels, and at H32 (8:7 ≈ 1.143) it is wider. The
first draft of `the_default_aspect_is_the_television_one_and_the_modes_differ` asserted `tv` was
proportionally wider and **went red on its first run**, which is how this was found rather than shipped.
The row now asserts the two ratios as integer identities (`tv.x * 3 == tv.y * 4`, `sq.x * 7 == sq.y * 10`)
and then asserts the ordering **both ways**, at H40 and at H32, so it is a measurement of the ratio rather
than of one box.

### 3.3 ⚑ `Integer` is a claim about the PIXEL grid, so the fit is computed in pixels

`fit` takes `pixels_per_point` even though it returns points. "The largest whole scale at which no row or
column is duplicated unevenly" is meaningless in points: computed in points at `ppp = 1.25`, integer mode
duplicates every fourth row while calling itself sharp. So the panel size is converted to device pixels,
`dest_rect` runs there, and the result is converted back.

`integer_mode_is_whole_in_pixels_not_in_points` is the gate, swept at ppp 1.0/1.25/1.5/2.0 and both native
widths, asserting both axes are whole multiples **and that they are the same multiple**. Red-first, with the
mutation shown on disk — `fit`'s body computing in points (`avail.x.floor()`, result returned unscaled):

```
test screen_pick::tests::integer_mode_is_whole_in_pixels_not_in_points ... FAILED
  assertion `left == right` failed: ppp=1.25 320x224: 1200 px wide is not whole
test result: FAILED. 12 passed; 1 failed
```

Note that the other twelve rows stayed green under that mutation, including the identity round trip — which
is exactly why this row exists separately: a points-space fit is *self-consistent*, so nothing that checks
the fit against its own inverse can see it.

### 3.4 `the_fit_is_the_frontends_own_dest_rect`

A row asserting that at `ppp = 1.0` the points `fit` returns are `dest_rect`'s own pixels, for every mode
and both widths at three panel sizes. Its only job is to notice if somebody ever inlines a ratio formula
here instead of calling the frontend's function — the drift that S0's whole shape exists to prevent, in the
one place it would be most tempting.

---

## 4. What S2a inherits

S2a is *"the display mask must reach the picture"*. This parcel deliberately did not build it, and left it
these five things.

**1. A refusal to delete, and a test that goes with it.** `Panel::masked_off_only` and its `pick` gate are
the whole of the masked-off-only concession, plus `the_masked_off_only_refusal_names_the_layers_the_mask_hides`
and `a_masked_off_refusal_changes_nothing_on_the_machine`. When the picture honours the mask, delete the
gate and those two rows; **nothing else in `screen_pick.rs` changes**, because `Panel::pick` already passes
`bus.layers()` into `pick::resolve` rather than `LayerMask::ALL`. That was deliberate: the line is correct
unchanged the moment the picture catches up.

**2. `Bus::layers()` already exists.** It landed with S1 because the gate needed to read the mask to refuse.
S2a needs the same accessor to draw with it, and it is a lend from `Host::layers()`, not a mirror — so a
socket client's `emulator/set_layer_enabled` and a window checkbox move one `LayerMask`, and there is
nothing on this side to drift.

**3. ⚑ `render_scanline` must gain no mask, and this parcel added none.** `docs/OVERSEER.md:789-793`: the
one render that commits sprite-overflow/collision latches and the R10 carry **takes no mask and has no
masked twin**, which is what makes *"a display mask cannot perturb emulation"* enforced by the type system
rather than by discipline. The masked path is `render_line_masked`, a separate function and a post-hoc
re-render that loses every mid-frame palette effect. That cost is real, already accepted, and is why the
frontend announces the masked path with a badge — **so S2a owes the badge as well as the pixels**, or the
picture silently loses colour effects.

**4. Where the mask has to reach.** In `oracle-frontend` the switch is inside `drain::drain` (`drain.rs:230`)
so a *bus client* can change the window's rendering path. The player's equivalent seam is
`Machine::adopt_frame` / the capture the loop feeds — and note `bus.rs`'s `framebuffer` doc, which
deliberately does **not** mask a client-driven frame today *because* this window masks nothing: *"masking
here would make a client-driven frame the only one that honoured `emulator/set_layer_enabled` — one window,
two rules for what it is showing."* S2a must move both paths together or re-create that split.

**5. Four palette rows close for free.** §2.1's debt names the frontend's four `ToggleLayer` commands as the
only frontend actions that are already served methods (`emulator/set_layer_enabled`), generated from the
core's own `LayerMask::targets()`. Four checkboxes on the Screen tab — beside the aspect selector S2 just
put there — close them the moment the mask reaches the picture.

**And one thing S2a does not inherit, because it is answered.** The recon's §3.2 worry that the fit and the
inverse must move together does not apply here (§0.3): `dot_at` inverts the drawn rect. S2a can change what
is *in* the picture without touching either.

---

## 5. Left open

* **`F-STATUS-CAVEAT-NOT-ON-STRIP` is still not in the queue.** §2.2 measured that it and
  `F-FRONTEND-NO-STATUS` are named only in `docs/lane-log.jsonl:109` and have no `id` in
  `lane-status.json`'s `queue`. This parcel did not book it, because `lane-status.json` is deliberately
  uncommitted and this worktree's copy is stale — an edit from here would be written against a queue that
  no longer exists. It should be booked against `oracle-player` and landed with S7.
* **Nothing here was seen on a screen.** Constraint A forbids launching a windowed binary from a background
  agent, so every claim in §2 and §3 is a `cargo test` result. The two things that genuinely need the
  owner's own display are unchanged from the recon's §7.3: that the picking offset is right at his actual
  `pixels_per_point` (the tests pin the arithmetic at 1.0/1.25/1.5/2.0, which is the most a headless
  harness can do — a test under Xvfb would confirm the harness's scaling rather than the display's), and
  that the three aspect modes look right rather than merely measure right. **TAGGED for a foreground pass.**
* **The spawn placement path is exercised only to its refusals.** `spawn::place`'s happy path needs a
  running game with `Camera_X`/`Camera_Y`/`Level_Width`/`Level_Height` in the loaded listing and an act
  initialised; the testrom has none of that, so a unit test there would assert the *"this build cannot turn
  a click into a world position"* refusal, which is a real row but not the one that matters. The
  choreography is unchanged code shared with `oracle-frontend`, which is the argument for why this is
  acceptable rather than an argument that it is covered. **TAGGED: a foreground click-to-place on
  `s4.debug.bin` is the instrument.**
* **The per-frame cost of the new surfaces is not measured**, per the recon's §7.1. S1 adds no per-frame
  work of the kind that section worries about — the pick and the spawn are per-*gesture*, and the Screen
  tab's per-frame additions are a badge, three `selectable_label`s and a text line — but "adds no
  measurable cost" is a claim and this parcel did not measure it. The instrument exists and is named in
  §7.1; it needs a foreground run.
* **The Screen tab's new strings do not reach `emulator/screen_text`** — and neither does any other panel
  body's text, which is the status quo rather than a regression this parcel introduced: `build_ui` collects
  `screen::Run`s from the top bar and the palette only, so all eight panel bodies are already invisible to
  the readback. It is worth stating because the recon's §3.5 names exactly this seam for S5 and the surface
  just got bigger. If the readback is ever extended to panel bodies, the badge and the refusal line are the
  two that matter most, because they are the two a person would quote when reporting a problem.
* **The hover callout and sprite outlines** — the two of `oracle-frontend`'s seven lenses §3.0(c) says are
  irreducibly picture-coupled — are not built. They need `present::forward_map`
  (`present::native_rect_to_window`), which is now in the lib and is already used by S1's identity test, so
  the seam is open.
