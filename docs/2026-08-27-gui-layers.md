# GUI-LAYERS — the player window honours the layer mask

Built 2026-08-27 on branch `parcel/gui-layers`, off `main` at `ad8e5a5`. Closes the first of the two gaps
`docs/2026-08-26-layer-mask.md` named under **Known gaps**: *"the player GUI has no layer-mask surface."*

The second gap — *not exercised against a real ROM* — is **still open**, and this parcel does not close it.
Nothing here has been seen on a screen by the person who built it; see **What could not be verified**.

## The defect, precisely

`oracle-frontend` draws its own window from a `ScanlineCapture`, and `pick.rs::resolve` called
`Vdp::pixel_attribution` — unmasked. So in the hosted arrangement a mask set over the socket changed the
bus's answers and not the picture, and `pick.rs`'s stated invariant

> this panel and the `emulator/pixel_attribution` bus method must never disagree

was **conditional on no mask being set, with the precondition unasserted** — its test still passed because it
ran on a default all-on engine. That shape (a rule whose precondition nobody checks) is the recurring defect
this workspace keeps rediscovering, so the parcel's centre of gravity is the assertion, not the toggle.

## A. One mask, not two

The mask lives on `Engine::layers` and nothing copies it.

* `oracle-core` gained the **vocabulary**: `Layer::mask_key()`, `LayerMask::targets()`, `LayerMask::hidden()`
  — verbatim moves of `oracle-aether::engine`'s `mask_key` / `mask_targets` / `masked_layer_names`. They moved
  because the window now needs the same four names, and a frontend spelling a layer differently from the wire
  is the item-19 drift class wearing a label's clothes. `oracle-aether/tests/layers.rs::the_mask_vocabulary_is_the_contract_fragments_own`
  still pins that vocabulary against the vendored contract fragment — it now pins the **core's**, which is
  strictly more coverage than it had, and it is what makes the palette's four rows contract-derived rather
  than transcribed.
* `Engine::layers()` / `set_layer()` → `Host::layers()` / `set_layer()` → `Bus::layers()` / `set_layer()`.
  Exactly the `watchpoints_mut` precedent, one surface over: one instrument, two consumers, nothing to drift
  apart *from*. A client's `emulator/set_layer_enabled` and a palette toggle move the same field.
* In the `--no-default-features` build there is no engine, so `bus_stub::Bus` owns a `LayerMask` — the same
  core type, and the only one that exists in that build. Same argument the stub's `Watchpoints` is made with:
  the panel predates the bus, so its state must exist in both builds, and owning it there is what keeps the
  run loop one shape.

**There is no frontend-side notion of hidden layers anywhere.** That was the parcel's first constraint and it
is met by construction rather than by discipline.

## B. Toggles, and the standing statement

**Toggles.** `Cmd::ToggleLayer(Layer)` — the core's `Layer`, not a frontend enum of the same shape — with one
palette row per `LayerMask::targets()` under a new `DISPLAY LAYERS` group. Its own group rather than a corner
of `LENSES` on purpose: a lens draws *over* the picture, a layer toggle changes *what the picture is*, and a
user hunting for "why is my background gone" should not have to know they are the same kind of thing, because
they are not. Palette-only, no hotkeys, for the reason the lens toggles are palette-only (every obvious key is
taken) and because a mask is set deliberately and then left set.

`CommandInfo::title` became a `Cow<'static, str>` so these four rows can be **built** from the mask names
rather than transcribing them. Every other row is still a borrowed literal and allocates nothing.

**Not persisted.** `cfg` carries the lens set and the status-line flag; it deliberately does not carry the
mask. A mask that survived a restart is the forgot-it-was-on failure with *no memory of it at all*, and a
standing badge could only re-raise the question rather than answer it. A mask lasts for a session — which is
also what the engine-side placement already gave us for free (survives reset/reload/restore, dies with the
process).

**The picture.** `blit_masked` re-derives the frame from VDP state through `Vdp::render_line_masked`, the same
call `emulator/screenshot` reaches under a mask. It cannot use the captured frame, and that is not a shortcut
missed: the capture's rows were composited by `render_scanline` (the render that commits the sprite-overflow
and collision latches, which is why it takes no mask), so what it leaves behind is decoded colours with the
losing layers already discarded — "mask" applied to those bytes could only mean *paint over*, and painting the
backdrop over dots plane B was visible at is the believable-wrong-answer this whole surface avoids.

`mask.is_all()` gates it, so with nothing hidden not one line of it runs and the presented picture is
byte-for-byte the captured frame the loop has always shown.

**The cost, stated because it is visible.** A masked picture is a post-hoc read of whatever CRAM holds now, so
every mid-frame palette effect `blit_capture` exists to preserve (S3K's underwater split) is gone for as long
as a mask is set. Same trade the bus makes and announces as `source: "stateRender"`. The toggle's terminal
line says so; clearing the mask restores the captured frame on the next completed frame.

**The badge — a correctness requirement, adopted from the consumer lane's point 5.** An unconditional
`HIDDEN: planeA planeB …` panel, amber, right-aligned inside the **F3 status band**. Three properties, each
chosen against a specific failure:

| Property | Against what |
|---|---|
| Drawn on every frame the mask is non-default, behind no flag | A toast expires and the mask does not; the author forgets, then reads a masked picture as the machine's |
| Names the hidden layers, in the wire's own words | "A mask is set" sends the reader hunting; `HIDDEN: planeB` does not — and the words are the ones they can type into the palette |
| Never truncated (scale steps to 1, then the badge is dropped) | `HIDDEN: plan` names a layer that does not exist — `PAUSED_WORD`'s rule ("PAU is not a pause indicator") on a longer string |

**Why the status band.** It is the one strip of the picture every lens already clears unconditionally
(`cpu.rs::top_of` starts at `status_band().y + h`), so no lens has to learn about the badge and none can be
dimmed by it — the interference the CPU chip's `paused_banner_rect` dodge exists for. The band's only other
tenant is the status line, which grows from the left; it is handed a **shortened width** whenever the badge is
showing, so the two are exclusive by pixel arithmetic rather than by redraw order. A status line allowed to
run under the badge would be a wrong readout painted underneath the sentence saying the picture is wrong.

## C. The invariant, asserted

`pick::resolve(vdp, x, y, mask)`. The mask is a parameter with **no unmasked twin to fall into**, exactly as
`Engine::framebuffer(mask)` is and for the same stated reason: each call site says which picture it means.

The answer shape follows the consumer lane's point 1 (their expensive lesson: a correct 1,244-cell highlight
whose reception was *"what are the purple boxes"*). `Pick` gained a `headline` — a sentence a person reads —
and `description` is that sentence followed by the structured detail that was always there:

```
That dot is plane A, drawn from VRAM-absolute tile $055.
   (planeA + sprites hidden, so this is the masked picture, not the machine's)
```

`[planeB:won, backdrop:lostToPriority]` is the right data and the wrong answer; the enum keeps its place
underneath.

**The ruling on other tools' models is honoured and re-stated at the point of use.** Every tile index this
panel names is `VRAM-absolute`, said in the answer, and nothing rebases into anyone else's blob. `TILE_SPACE`
carries the reason: the failure of a rebase is not a throw but a confident wrong slot — *in-capacity is not in
blob* — and it is indistinguishable from a right answer. **No disagreement to record**; the ruling matched
what the panel already had reason to do.

### The gates, and what each rules out

Every row below was proven red first, on the poison named, with the assertion quoted.

| Gate | Poison planted | It said |
|---|---|---|
| `pick::…::the_panel_and_the_bus_agree_under_every_mask_that_changes_the_answer` | `resolve` back to `pixel_attribution` (the pre-parcel unmasked call) | `hiding ["sprites"]: the bus says planeA and the panel armed $0200-$021F — the two have DRIFTED` (left `(512,543)` = the sprite's tile, right `(2720,2751)` = plane A's) |
| `overlay::…::a_hidden_layer_is_stated_on_screen_for_as_long_as_it_is_hidden` | badge drawn only `if self.showing_status()` (the obvious "put it in the status line") | `a hidden layer must be stated on screen even with the status line off and every toast expired` |
| `overlay::…::the_status_line_never_runs_under_the_badge` | the `badge_w` subtraction dropped | `320x224: the status line ran under the badge (2156 pixels in Rect { x: 97, y: 4, w: 219, h: 11 })` |
| `commands::…::every_mask_target_gets_a_visible_toggle_and_nothing_else_does` | one layer filtered out of the registration loop | ``no palette row for the `window` layer`` |
| `main::…::the_masked_picture_is_the_cores_masked_render_and_differs_from_the_unmasked_one` | `blit_masked` renders at `LayerMask::ALL` | `the masked and unmasked pictures are identical at (0,0) — either blit_masked ignored its mask, or the fixture's dot does not change under one and COULD NOT MEASURE anything` |
| `host::…::the_window_and_the_socket_move_one_mask` | `Host::layers` answers `LayerMask::ALL` instead of the engine's | `a client hid sprites and the window's accessor did not see it — the two are not one mask`, `left: []` |

**The green-poison question, per assertion — what else could make this pass with the rule broken:**

* *The parity row.* The pre-existing fixture could not catch a masked/unmasked split at all: its planes are
  transparent, so hiding plane A changes nothing and an unmasked panel keeps agreeing with a masked bus **by
  coincidence**. That is the sound-guard-green-poison shape exactly. So `vdp_with_four_answers` builds one dot
  at `(70,70)` where a sprite covers an opaque plane-A cell covering an opaque plane-B cell over a non-zero
  backdrop — **four** different right answers at one dot, one per mask. Further ruled out: the bus side going
  unmasked (the row asserts `r["winner"]["layer"]` against the fixture's expectation first, so a bus that
  ignored the mask fails before the panel is consulted); a stale engine mask (`e.layers().hidden()` is compared
  against what `set_layer_enabled` was told, as a *set* — `hidden()` answers in `Layer::ALL` order, which is a
  property of the core's enumeration and not of the caller's switching order); a silent refusal (`dispatch`
  panics with the layer named); and an empty answer (`p.targets[0]` indexes, so an empty pick panics rather
  than vacuously passing). It also checks the **prose**, because a range alone cannot tell plane A's tile from
  a sprite that happened to draw from it.
* *The badge rows.* Asserted after every toast has aged out **and** with F3 off — the two states in which "the
  user was told" is otherwise pure assertion. The negative half (`LayerMask::ALL` ⇒ zero ink) is asserted too,
  or a badge that was simply always painted would pass. The overlap row draws the status line **alone** over a
  known non-zero ground and looks for its ink in the badge's columns; drawing the badge as well would paint
  those columns itself and hide the thing under test. It also panics `COULD NOT MEASURE at {w}x{h}` if the
  badge has no form at that size, rather than skipping.
* *The registry row.* Its expectation is `LayerMask::targets()`, not four literals, so it is "the window
  offers exactly what the wire accepts". Both directions: a missing row is a feature nobody can reach, an
  extra row is a toggle that would call `set_layer` with something the mask refuses (the backdrop is the live
  candidate — it is a `Layer` and is not a target), and every registered payload is checked to have a
  `mask_key`.
* *The host row.* It drives **both crossings against each other** — a window-side `set_layer` read back
  through the served `emulator/get_layer_states`, and a client-side `emulator/set_layer_enabled` read back
  through `Host::layers()`. Either crossing alone passes against a half-copy, and that is not a hypothetical:
  under the poison above (getter copied, setter still delegating) the **window→socket crossing stayed green**
  and only socket→window fired. Two crossings is what closes it. The refusal path is asserted too — a
  `set_layer(Backdrop, …)` must return `false` *and leave the mask equal to what it was*, rather than
  pretending to have applied.
* *The picture row.* A `blit_masked` that ignored its mask would still match `render_line_masked(line, ALL)`
  on any scene where nothing is hidden, **and** on a scene where the hidden layer happened to be transparent.
  So the fixture hides a layer that is opaque and winning, and the row asserts the two pictures **differ**
  before asserting which one the window shows. CRAM is written explicitly, because power-on CRAM here is all
  black and the first draft of this row failed its own `COULD NOT MEASURE` guard for exactly that reason —
  the guard earned its place before the row was believed.

### The numbers

```
cargo test --workspace --no-fail-fast   exit 0
LEGS=56 PASSED=1880 FAILED=0 IGNORED=6      (baseline on main: LEGS=56 PASSED=1870 FAILED=0 IGNORED=6)
```

`+10`, accounted for one by one: the three `pick::bus_parity` rows, the three `overlay` rows, the two
`commands` rows, the one `main` row, and the one `host` row — the ten in the table above. No leg moved, so
no test binary appeared or vanished.

The workspace aggregate runs **default features only**, so the stub build was run separately:
`cargo test -p oracle-frontend --no-default-features` → `227 passed; 0 failed; 1 ignored` (the 32-row
`bus_parity` module is `#[cfg(feature = "aether")]` and is correctly absent there).

`cargo clippy --workspace --all-targets` and `cargo clippy -p oracle-frontend --no-default-features
--all-targets`: zero warnings, zero errors. `cargo fmt --all` applied.

**Currency: `git diff main -- crates/oracle-core/tests/` is empty.** No golden regenerated, none touched.

**No socket was dialled.** `/run/user/1000/oracle.sock` was never contacted and no server was spawned —
`Engine` and `Host` are both drivable in-process, so every bus assertion here goes through `Engine::dispatch`
or `Host::pump` directly. No emulator MCP call was made.

## D. Click-to-identify — **BLOCKED**, on two independent grounds

> ⚑ **SETTLED 2026-08-27 by `docs/2026-08-27-obj-join-recon.md` (merged `f1e3484`), which CORRECTS this
> section twice. Read it before acting on anything below.** The verdict — no non-heuristic derivation
> exists — **stands**, but this section's stated *reason* is wrong and its ground-2 pricing is wrong.
> **(1) Ground 1's reason.** This section says recovering the mapping "means replaying the engine's own
> sprite-building order … which is game code we do not model", implying infeasibility. **A replay IS
> feasible** — all four inputs resolve in the listing. The correct reason to refuse is that a replay
> **cannot detect its own divergence**, so it yields a confident wrong object name with no signal. That
> distinction matters because the wrong reason invites the cheaper partial replays somebody will propose
> next, and the right one rules them out too. **(2) Ground 2's asymmetry does not exist.** `mod pick` and
> `mod symbol_file` are both **ungated** in `oracle-frontend/src/main.rs` (the `#[cfg(feature = "aether")]`
> nearby guards `mod bus`), so with the decode in `oracle-core` the bus crate is not involved at all and
> there is no feature-gated second behaviour. Verified firsthand at the merge. **(3) Also corrected:**
> object records are `$50` bytes, not the 64 this section's brief assumed, and `sprite_piece_count` is a
> *pre-walk prediction* rather than "a count, not a range" — it is nonzero for objects that emitted
> nothing, which is worse. **The killing argument for refusing a nearest-object guess is one this section
> did not have: rings are the most-clicked sprite class in a Sonic act and rings are not objects at all**
> — they emit from a flat buffer outside the object pool, so every nearest-object answer there is wrong,
> silently.


Landed: nothing. Recommendation: **its own parcel, and the cheap half is not the hard half.**

**Ground 1 — the join does not exist, anywhere.** The ask is "when a *sprite* wins the dot, say what game
object it is". `pick.rs` has the winning **SAT index**; `emulator/object_list` / `object_slot` have **object
slots**. Nothing in this repo maps one to the other — verified by grep over `crates/oracle-aether/src`:
`sprite_piece_count` (object field `$25`) is the only field that even mentions sprites, and it is a *count*,
not a range. Recovering which SAT entries an object owns means replaying the engine's own sprite-building
order (the display-list walk that fills the SAT), which is game code we do not model. Any position- or
art-tile-based match is a **heuristic**, and a heuristic here produces a confident wrong object name —
precisely the failure class the parcel's own ruling names, one level up from the blob rebase. So this is not
"plumbing we did not get to"; it is an unanswered modelling question.

**Ground 2 — the decode is on the wrong side of a crate boundary, and moving it is a real refactor.**
`decoders.rs` is 771 lines depending on `crate::hex`, `crate::rpc::{code, RpcError}` and `serde_json`. Sharing
it with `pick.rs` means splitting a pure decode (core-able) from its JSON/RPC skin, and `oracle-core`'s
dependency list is deliberately tiny and I/O-free. There is also an asymmetry the brief did not anticipate:
`oracle-aether` is an **optional** dependency of `oracle-frontend`, so a `--no-default-features` player could
not identify objects at all — a feature-gated panel answer is a second behaviour, not a second build.

**And above all: no second copy was made.** The brief's instruction stands as written and is the right one.

**Recommendation, in order.** (1) Settle the join first, on paper, as a modelling question — is there a
non-heuristic derivation of SAT-run → object slot, and if not, is "which object is *nearest* this sprite, and
we say so" an answer worth shipping *labelled as* an inference? (2) Only if (1) yields something, do the crate
split: `oracle-core::objects` for the pure decode, `oracle-aether` keeping the `Value` skin, `pick.rs` reading
the core half. (2) without (1) buys a shared decoder with nothing to ask it.

## What could not be verified — needs eyes on a window

I cannot see the window and neither can the dispatcher. Every claim above is from headless assertions and
build output. **Not observed, and not claimed:**

* That the badge is legible, well-placed, or the right size at any real window size. The geometry is asserted
  in pixels (inside the picture, holding the whole text, not overlapped by the status line at 320x224 /
  640x448 / 960x672); how it *looks* is unmeasured.
* That the masked picture is recognisable — that hiding plane A on a real game frame shows the background you
  expect. The dot-for-dot equality with `render_line_masked` is asserted; "and that render is right for a
  Sonic frame" is `docs/2026-08-26-layer-mask.md`'s still-open second gap, untouched here.
* That the palette rows read well, that `DISPLAY LAYERS` lands in a sensible place in the list, or that the
  toggle feels responsive.
* Any frame-rate cost of `blit_masked` (224 post-hoc line renders per present while a mask is set). It is
  bounded and only paid while masked, but it has not been timed.

⟨RUNTIME⟩ **The foreground pass this wants**, for whoever has the screen: launch `s4.debug.bin`, open the
palette, hide each layer in turn, and confirm (a) the picture changes as the mask says, (b) the badge is up
the whole time and names the right layers, (c) clicking a dot where a hidden layer used to be names what is
*now* showing and says a mask is on, and (d) `emulator/set_layer_enabled` over a socket moves the same badge.
(d) is the one an automated test cannot reach: the parity gates prove the panel and the bus agree in-process,
but a hosted `Host` with a live socket driving the window is a runtime arrangement.

## Corrections to the brief

* **The base-commit check is stale.** The brief said `git log -1 --format=%s` on `main` "must mention the
  layer switches being served/gated/driven". `main` is `ad8e5a5` *"the join's other side: in-capacity is not
  in-blob…"*; the layer-switch commit is `1b16057`, **three commits back**. `main` had advanced by two commits
  since the brief was written. I did not stop: the substantive precondition (the switches are served) holds at
  `1b16057`, and `crates/oracle-frontend/src/pick.rs` exists. Worth knowing that a `log -1` gate on a moving
  branch expires within a day.
* **The `pick.rs:501` and `pick.rs:30` line numbers were right** at `ad8e5a5` and are not after this parcel —
  the same perishability the shared protocol's precedent-preamble warns about, arriving on a brief instead of
  on a bar.
* **`docs/lane-status.json` was not touched**, as instructed. Queue outcome to transcribe: **GUI-LAYERS —
  A/B/C landed on `parcel/gui-layers`; D BLOCKED (own parcel, join unmodelled).**
