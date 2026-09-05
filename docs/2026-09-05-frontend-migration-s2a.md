# Frontend migration S2a — the display mask reaches the picture

**Date:** 2026-09-05 · **Branch:** `worktree-agent-a69a3e74841ca3df9`
**Plan:** `docs/2026-09-05-frontend-migration-recon.md` §3.2a · **Inherits:**
`docs/2026-09-05-frontend-migration-s0-s2.md` §4.
**Rides along:** `F-PARITY-BLIND-TO-SAT-STRIDE` (`docs/OVERSEER.md`).

**No windowed binary was launched. No emulator MCP tool was touched.** Everything below is `cargo check`,
`cargo test`, `cargo clippy` and `cargo fmt`.

---

## 0. What the brief and the plan got wrong, and what I got wrong

### 0.1 The brief's §5 sentence *"S2a deletes that gate and its test"* is one row short, and the deletion is not the whole of the concession

Three rows go, not two. `docs/2026-09-05-frontend-migration-s0-s2.md` §4 names
`the_masked_off_only_refusal_names_the_layers_the_mask_hides` and
`a_masked_off_refusal_changes_nothing_on_the_machine`; `nothing_is_refused_while_every_layer_is_shown`
went with them, because its whole body was the all-shown control for a gate that no longer exists. Its
assertion is not lost — it is the first two lines of
`the_standing_mask_statement_names_the_layers_the_mask_hides`, where the same control now guards the
statement instead of the gate.

**More consequentially: deleting the gate outright would have been wrong.** §4's item 1 says *"when the
picture honours the mask, delete the gate and those two rows; **nothing else in `screen_pick.rs`
changes**, because `Panel::pick` already passes `bus.layers()` into `pick::resolve`."* That reasoning is
sound about the ordinary frame and it drops a fact the slice creates: **once this window has two pixel
paths, "the mask the machine holds" and "the mask the picture on the glass was drawn with" become two
different facts.** They agree on every ordinary frame. They separate when the mask moves *after* the
picture was made — the palette can call `emulator/set_layer_enabled` during the same `build_ui` that drew
the picture — and when a masked render produces nothing. In that window `pick::resolve`'s *"the panel
describes the picture"* is still unsatisfiable, and `bus.layers()` is the wrong argument.

So `Panel::pick` takes the glass's mask as a **parameter** and refuses when it disagrees with the bus's.
That is a much narrower gate than S1's — invisible on every ordinary frame, where the two are equal — and
it is the same shape S1's was: read before anything is resolved *or retired*.

### 0.2 The recon's §3.2a *"Proof"* is satisfiable by a test that proves nothing

§3.2a asks for *"a test that a mask set through `emulator/set_layer_enabled` changes the uploaded
`ColorImage`."* Taken literally that is satisfied by asserting a flag, or by asserting the image is
*some* different image. The rows here assert on **named dots**: the plane dot must become the value the
backdrop dot already had, with the two asserted **different from each other first**, because "the dot is
red afterwards" witnesses nothing if it was red to begin with.

### 0.3 ⚠ My own: a row whose doc claimed to pin an ordering it could not see

`bus::masked_picture::a_client_driven_frame_is_masked_as_well` said in its own doc that it pinned the
ordering inside `drain` — that the masked re-render sits after `screen_changed`'s adoption. **Measured:
moving the re-render ahead of the adoption on disk left it green, and the whole suite green.** It could
not have been red: the row performs the adoption itself with `Machine::adopt_frame`, so the drain it
exercises sees `screen_changed == false` and only ever runs one of the two things whose order was
supposedly under test.

It is renamed to what it does measure (`an_unmasked_picture_on_the_glass_is_masked_by_the_next_drain`),
its doc records the negative measurement, and
`bus::pumped::a_client_driven_frame_reaches_the_glass_already_masked` is the row that pins the ordering:
a real client over a real socket, and the assertion is on `Machine::image_mask` — the field the two
orderings write in the two different orders. Commit `0e74d70` carries this correction on its own.

**The general form:** a mutation that stays green is a runner-or-gate defect, not a pass, *and the gate
half of that is easy to write off when the row is new and its name reads right.* The variable that
decided it here was mutating the **ordering** rather than the **effect** — the effect mutation (mutation
A below) reddened all three rows, and stopping there would have left the ordering claim standing on a
row that could not see it.

### 0.4 ⚠ My own, second: a false alarm on the loudest surface the tab has

The first draft wrote the standing alarm as `screen_mask != Some(bus.layers())` and drew it
unconditionally. On a freshly launched player, before any picture exists, `screen_mask` is `None` and the
bus's mask is `LayerMask::ALL` — so the alarm fired, in error red, announcing that a picture that was not
there disagreed with an empty mask. **A false alarm on the loudest surface a tab has is the fastest way
to teach a reader to ignore the real one.** `glass_alarm` now answers `None` for "no picture", and
`the_standing_alarm_fires_on_a_disagreement_and_never_before_the_first_frame` asserts the absence rather
than arguing it away. A *click* in that state is a different question and is still refused, in a sentence
that says there is no picture rather than describing a mask.

---

## 1. What landed

### 1.1 The pixel path

* **`Machine::render_masked(mask) -> bool`** — this window's `blit_masked`. A post-hoc re-render through
  `Vdp::render_line_masked`, line by line, into the same `egui::ColorImage` the uploader hands egui.
  Returns `false` and leaves the retained picture alone when the render produces no dots at all.
* **`bus::drain`** reads `Bus::layers()` at the **end** of the drain and, when `!is_all()`, calls it.
  `Drained::masked_picture` reports it.
* **`Loop::upload`** now also runs when the drain replaced the picture (`Drained::picture` /
  `masked_picture`), which was missing: both change `Machine::image` on an iteration that emulated
  nothing, and an early wake would have left the previous picture on the glass.

**Position, and both halves of it are load-bearing.** *After* the pump, so a socket client's
`set_layer_enabled` reaches the glass on the very iteration it arrives rather than a frame late. *After*
`screen_changed`'s `adopt_frame`, so a client-driven frame is masked too — one window, one rule for what
it is showing, which is the split `Bus::framebuffer`'s doc used to have to warn about. Inside `drain`
rather than in the loop, because it is the same kind of thing as the four repairs above it and a repair
the caller may decline to mention is the shape `PLAYER-SERVE` already shipped once.

It is keyed on **no `PumpReport` field**, and that is deliberate: the mask is engine state that survives
every reload and restore, so the question each iteration answers is *"is a mask set right now"*, not
*"did one just change"*. Keying it on a change flag is how a window ends up unmasked again after a
`reset` it did not think was relevant.

### 1.2 ⚑ `render_scanline` gained no mask, and none may ever be added

`docs/OVERSEER.md`'s LAYER-MASK entry: it is the one render that commits the sprite-overflow and
collision latches and the R10 carry, so *"a display mask cannot perturb emulation"* is enforced by the
**type system** rather than by discipline. Nothing in this slice touches `oracle-core`.

The capture it produces cannot be masked after the fact either, and that is not a shortcut missed: its
rows are decoded colours with the **losing layers already discarded**, so "mask" applied there could only
mean "paint over", and painting the backdrop over dots plane B was visible at is the believable-wrong-
answer this whole surface exists to avoid. Hence a re-render — and hence its cost, which is real and
already accepted on the other window: it reads whatever CRAM holds *now*, so **every mid-frame palette
effect is absent for as long as a mask is set** (S3K's underwater split is the loud one). The bus makes
the same trade and announces it as `source: "stateRender"`. This window has to say it too, which is §1.3.

### 1.3 The lens says it is on, persistently, on the human-facing surface

`screen_pick::mask_statement` is drawn above the picture on **every** frame a mask is set and on none
where it is not. It names the hidden layers off `LayerMask::hidden()` — the same derivation the wire's
caveat and `pick`'s clause use, so it cannot name a layer the mask does not hide and cannot miss one —
and it names the re-render cost from §1.2.

This is the consumer's ruling banked in `docs/OVERSEER.md`'s GUI-LAYERS entry, point 5, and it is a
correctness requirement rather than polish: *the author will forget, and then read a masked picture as
the real one.* A toast cannot carry it, because toasts expire and the mask does not. It is above the
picture rather than below for the reason the halting alarm is on the top bar: a standing statement that
can be cropped out of view is not standing.

### 1.4 Loud on unmeasurable

`Machine::image_mask` records the mask the retained picture was drawn under, written in the same
statement as every write to `image`. `Loop::upload` latches it into `tex_mask` in the same statement that
binds the pixels; `Panels::screen_mask` carries it to the tab. Where it disagrees with `Bus::layers()`:

* the tab draws `screen_pick::glass_alarm`'s sentence, in error red, naming **both** masks;
* a click is refused by `Panel::pick`'s gate, in a sentence naming both, before anything is resolved or
  retired.

Two functions rather than one expression in two places, so the alarm and the refusal cannot disagree
about one frame.

### 1.5 Four palette rows close

The Screen tab grows one checkbox per `LayerMask::targets()` entry, generated from the core's own
vocabulary rather than typed here, dispatched through `emulator/set_layer_enabled` via `Bus::call` →
`Host::call` (D15: in-process, no socket, and the tool's own refusal shown whole). Those are the four
`ToggleLayer` rows `F-PLAYER-PALETTE-NO-ACTIONS` names as *"the only frontend actions that were already a
served method"*, and `docs/2026-09-05-frontend-migration-s0-s2.md` §4 item 5 says they close here.

The checkbox shows the **bus's** mask, not the glass's, because it is a control and must report the state
it writes; §1.3's line is what reports the glass.

---

## 2. The ride-along: `F-PARITY-BLIND-TO-SAT-STRIDE`

`pick.rs`'s `bus_parity` is nine rows asserting address-level agreement between the panel and
`emulator/pixel_attribution` — the strongest correctness guard in that crate — and mutating
`SAT_ENTRY_BYTES: u32 = 8` to `16` left all nine GREEN.

**Reproduced firsthand before fixing.** `git checkout HEAD~1 -- crates/oracle-frontend/src/pick.rs`, the
mutation applied on disk and quoted back (`const SAT_ENTRY_BYTES: u32 = 16;`), the nine rows run: **9
passed, 0 failed.**

Blind for two reasons, and both had to close:

* every parity case asserted `spriteIndex == 0`, so `index * SAT_ENTRY_BYTES` was multiplied by zero;
* the rows asserted `p.targets[1].lo` and never `.hi`, which is the one place the stride survives at
  index 0 (`sat_lo + SAT_ENTRY_BYTES - 1`).

The fix:

* **`vdp_with_sprite_at_index(decoys, …)`** puts `decoys` off-screen SAT entries ahead of the real sprite
  and links them to it — the only way to make a *later* SAT index the winner, since the walk starts at 0
  and follows `link`. The decoys are off-screen in **Y**, so they are absent from every scanline's sprite
  list rather than merely invisible, and their X is off the right edge: an on-screen `x = 0` sprite is a
  *mask* sprite on the hardware, and a decoy that suppressed the sprite it exists to precede would be a
  fixture that silently measured nothing.
* **`bus_sat_stride()`** measures the stride **off the wire**, at a non-zero index, against the core's
  own `sat_base()`. Nothing in the test spells `8`. It asserts `index > 0` explicitly — at index 0 it
  would divide by zero, and a fixture that quietly came up at 0 again would make the measurement vacuous.
* the sweep runs at `decoys` 0, 1 and 5, asserts `spriteIndex == decoys`, and asserts
  `targets[1].hi == lo + stride - 1`: the panel's SAT range **length** against the **bus's** own entry
  spacing, so the two implementations of one constant are held to each other rather than each to itself.
* `SAT_FIXTURE_STRIDE` is written `4 * 2` and deliberately does **not** reuse `SAT_ENTRY_BYTES`. Reusing
  the constant under test would move the fixture and the code under test together, which is the shape
  that let the mutation live in the first place.

**Proof it is fixed, both halves, each shown applied on disk and restored from a committed baseline:**

| Mutation | Result |
|---|---|
| `SAT_ENTRY_BYTES: u32 = 8` → `16` | **RED** — *"the panel arms $B000-$B00F, which is 16 bytes, but the bus spaces its SAT entries 8 bytes apart — the two have DRIFTED about the entry size"* |
| `u32::from(index) * SAT_ENTRY_BYTES` → `0 * SAT_ENTRY_BYTES` | **RED** — *"(64,64) at SAT index 1: the panel arms $B000 but the bus names "0x0000B008" — the two have DRIFTED"* |

The two mutations are complementary on purpose: the first is caught by the new `.hi` assertion at index
0, the second by the new non-zero index at `.lo`. Neither alone would have proven both halves live.

---

## 3. Red-first proofs for S2a

Every mutation was applied on disk (`git diff --stat` naming the file, and the mutated line quoted back),
run, then restored with `git checkout HEAD -- <path>` on a tree whose only difference was the mutation,
and re-run green.

| # | Mutation | Result |
|---|---|---|
| A | `drain`: `out.masked_picture = machine.render_masked(layers)` → `= true` (report the repair, perform none — the fabricated-success shape) | **RED**, 3 rows: *"with plane A hidden and nothing behind it, the cell it was drawing must fall through to the backdrop — the picture on the glass still shows plane A"* |
| B | `drain`: the masked re-render moved **ahead** of `screen_changed`'s adoption | **RED** (after §0.3's fix; **green before it**) — *"the adopted frame reached the glass UNMASKED … the client's own frame is the one frame that ignores the mask"* |
| C | `Panel::pick`: the gate's early `return` deleted, so it reports and does not stop | **RED**, 2 rows: *"a refused click must leave the instrument untouched"* |
| D | `Panel::pick`: `pick::resolve(…, mask, …)` → `LayerMask::ALL` | **RED** — `a_click_resolves_under_the_mask_the_picture_was_drawn_with` |
| E | `Panel::set_layer`: compose the success sentence, call the server not at all | **RED** — `every_layer_toggle_goes_through_the_served_method` |
| F | `mask_statement`: *"a display mask is set"* instead of the layer names | **RED** — *"the statement must name planeB"* |
| G | `Machine::render_masked`: render with `LayerMask::ALL`, ignoring its argument | **RED**, 2 rows |
| H | `glass_alarm`: the `None` case treated as a disagreement — §0.4's false alarm restored | **RED** — `the_standing_alarm_fires_on_a_disagreement_and_never_before_the_first_frame` |

**⚠ One ops mistake, recorded because it nearly cost the evidence.** Mutation H was first run against an
*uncommitted* `glass_alarm`, so restoring with `git checkout HEAD -- <path>` discarded the fix along with
the mutation. Nothing was lost (it was re-applied from this session's own edits) but the rule exists for
exactly this: **the mutation and the restore must both be against a committed baseline, and the way to
guarantee that is to commit the fix first.** The table above is the re-run, from `ac118d4`.

---

## 4. Left open

* **The per-frame cost of the masked path is not measured.** Recon §7.1 names it: while a mask is set the
  drain performs a full 320×224 re-render **every iteration**, which is what `oracle-frontend` does and
  which has never been in a measured `oracle-player` frame beside eight panels. The instrument exists and
  needs no building — `oracle-player --mode bench-window --secs 75 --dock every-tab --bench-arm` under
  `crates/oracle-panels-spike/run.sh`, compared as a delta before and after, with a mask set. Constraint A
  forbids launching a windowed binary from a background agent. **TAGGED for a foreground pass.** If it
  does eat the margin, the mitigation is obvious and unbuilt: re-render only when the VDP or the mask has
  moved, rather than on every iteration.
* **Nothing here was seen on a screen.** Two things genuinely need the owner's display and both belong on
  `F-EYES-ON-PICKING`'s list rather than a new row: **(a)** that a masked picture looks like the masked
  picture — the tests assert named dots, not that a person recognises the result; **(b)** that the three
  new standing surfaces (the `HIDDEN:` line, the disagreement alarm, the four checkboxes) sit legibly
  above the picture at his `pixels_per_point` without pushing the game off the tab.
* **The Screen tab's new strings do not reach `emulator/screen_text`** — the status quo rather than a
  regression, since `build_ui` collects runs from the top bar and the palette only. It is worth restating
  because the surface just grew again and because these two in particular are what a person would quote
  when reporting a problem: the `HIDDEN:` line and the disagreement alarm. Recon §3.5 books the seam for
  S5.
* **`F-STATUS-CAVEAT-NOT-ON-STRIP` is still not in the queue**, unchanged from S0-S2's §5. Not booked from
  here: `docs/lane-status.json` is deliberately uncommitted and this worktree's copy is stale.
* **The masked path and `emulator/screenshot` are two implementations of one picture.** `Engine::framebuffer`
  and `Machine::render_masked` both loop `render_line_masked` over `ACTIVE_LINES`/`HEIGHT`, in two crates,
  and `oracle-frontend::blit_masked` is a third. They agree today and nothing asserts that they must. It
  is the `sprite_tile_at` situation one field over, and the same answer would apply — one derivation in
  `oracle-core` under all three consumers. Not taken here because it is not this slice's, and it is
  cheap: worth a row.
