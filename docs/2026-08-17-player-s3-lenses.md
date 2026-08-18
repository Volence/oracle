# Player S3 shipped — five read-only lenses (2026-08-17, overnight)

Branch `player-s3-lenses`, 19 commits `6b54d2b^..da62d42` (15 code, 4 plan/handoff docs) on top of S2
(`docs/2026-08-17-player-s2-config.md`), cut from `743a5b5`.
Spec: `docs/superpowers/specs/2026-08-16-player-buildout-design.md` §5, §9, §11.
Plan: `docs/superpowers/plans/2026-08-17-player-s3-lenses.md`.

## What shipped

Five overlays redrawn from live machine state each frame, drawn into the *window* buffer between
the blit and the palette — over the picture, under the palette and the toasts. All read-only over
core state; **`crates/oracle-core/` is a zero-byte diff for the whole slice**.

- **Watch ticker** — bottom strip: the newest hits formatted to read as one instrument with the
  `W`-key log, plus the armed and dropped counts. Non-destructive `hits()` only; `take_hits()`
  would let switching a lens on delete a socket client's evidence.
- **CPU chip** — top-right: PC resolved through the same `resolve_within(_, MAX_SYMBOL_DISPLACEMENT)`
  the watch log uses (raw hex without a `.lst`), SR with the supervisor letter and interrupt mask,
  and the run loop's own frame counter. **Auto-shows while paused**, in amber, even with every lens
  off. `cpu_regs` expands it to D0-D7/A0-A7 (A0-A7 via `addr_reg`, so A7 follows the supervisor bit)
  at one font scale smaller.
- **CRAM strip** — top-left, one text row below the status row: 64 live swatches, 4×16 in CRAM order.
- **Sprite outlines** — boxes on the sprites the hardware actually **link-walks**, in game pixels,
  clipped per edge, mapped to the glass by the blit's own forward map. Drawn beneath the panels.
- **Hover callout** — names what is under the cursor (plane tile, sprite slot, backdrop). Hover
  **explains**; click still **arms** — the two never trade jobs.

Plus the supporting pieces: `present::native_rect_to_window` (the blit's forward map, ceiling form —
the only one that round-trips); `lens::FrameCtx`/`Models`/`models()`/`draw()`, the one call the run
loop makes; the `lens/` module (`mod.rs`, `watch.rs`, `cpu.rs`, `video.rs`); a `Group::Lenses`
palette group with one auto-registered toggle per `LensId` (six rows — the register block registers
its own); and the seventh config key, `lenses`.

**F-CONFIG-UNKNOWN-KEYS is closed at both levels.** The seventh key forced S2's adjudicated
reversal, so an unrecognised config **key** is now kept verbatim and written back — and the same bug
one level down (an unrecognised **lens name** inside `lenses`) was found inside the commit that
fixed the first one, with a doc comment claiming it followed the very rule it broke. Both are
preserved now, warnings collapse to one line per category, and a remnant can never shadow a known
key on re-parse.

Gates at merge: fmt clean; clippy `-D warnings` **0 in both feature variants**; frontend
**227 passed / 0 failed** (default) and **199 passed / 0 failed** (`--no-default-features`), 1
ignored in each (`write_presentation_screenshots`, needs `ORACLE_SHOT_ROM`); **full workspace suite
EXIT=0 — 36 legs, 1549 passed, 0 failed, 4 ignored**; release build clean; core diff
`743a5b5..HEAD -- crates/oracle-core/` **0 bytes**. (Figures as of the hover fix; the pre-review
figures were 224/196 across 18 commits.)

> Note for whoever repeats the gate: the plan's literal `git diff m68000-microop-framework..HEAD --
> crates/oracle-core/` stopped being the right command mid-slice, because `m68000-microop-framework`
> advanced under us with real core work from a concurrent session (`2c210e8` HINT bookkeeping,
> `2275b82` `sh_probe`). That diff is now non-empty in the *other* direction — it shows main's
> additions as deletions. **The gate that means what it says is merge-base-to-HEAD**, i.e.
> `git diff $(git merge-base m68000-microop-framework HEAD)..HEAD -- crates/oracle-core/`, and that
> is the 0 recorded above. Re-merge main before reading anything into the raw two-dot form.

## ☐ OWNER-OWED smoke checklist — EXTENDED (S1's and S2's are still unrun)

Everything in `docs/2026-08-17-player-s1-palette.md` and `docs/2026-08-17-player-s2-config.md` is
still owed. The owner has now seen the lenses on real glass **once** and reported two bugs — amber
boxes over empty picture, and the register block eating 41% of the screen — both fixed here
(`cd6a145`, `8384c45`). A third, of the same family, came out of the whole-branch review rather than
the owner's screen: the overlay was **erasing glyphs from the CPU chip** at the default window size
(`da62d42`). A fourth — the same seam again, this time the status line and the `PAUSED` banner
garbling the *hover callout* — was closed straight after. The list below is partly exercised but
nowhere near complete.

- toggle each of the five lenses from the palette's LENSES group → each appears, and the toast names
  it (there are **no default hotkeys** — the palette is the only way in this slice);
- quit and relaunch → the same lens set returns. ✔ **Observed firsthand in a live session** by
  the reviewer, who saw `lenses = cpu_regs,sprites,cram` in `~/.config/oracle/player.conf` after a
  relaunch. Recorded as an observation with its provenance rather than a bare ✔, so a later reader
  can tell it from an assumption — the rest of this list has no such artifact behind it;
- hand-add a junk key to `~/.config/oracle/player.conf`, launch, toggle a lens to force a save, quit
  → **the junk key is still there**. This is the F-CONFIG-UNKNOWN-KEYS reversal on real glass and
  the one item here that protects a user's file rather than a pixel;
- expand the CPU registers at a large window (the owner's 896×672 → font scale 3) → the chip clears
  the CRAM strip and leaves the picture readable;
- confirm sprite outlines sit **only** on sprites that are actually drawn — no boxes floating over
  empty picture (the `cd6a145` regression, and the reason the walk is the source);
- drag the window to a deliberately **non-integer** size → outlines still land *on* their sprites.
  This is the forward map's entire reason for existing and the one thing no unit test can settle;
- hover a plane tile, a sprite and the backdrop → the callout names each;
- pause with every lens off → the CPU chip appears on its own, in amber;
- **at the default window size**, read the chip's `PC` line character by character against the
  terminal's own watch log → every glyph is there. The bug this replaces rendered `PC $001234` as
  `PC $00_234`, which is not a visibly damaged readout, it is a *plausible wrong one*;
- pause with the register block expanded at the default window size → the `PAUSED` banner and the
  chip do not touch, and every `D`/`A` register reads as eight hex digits;
- **with the F3 status line up, hover a tile with the cursor in the top-left corner** → the callout
  steps below the status line rather than under it, and every glyph of the tile number is there.
  Then pause and do it again → it steps below the `PAUSED` banner too, but only when it would
  otherwise land on it.

## Registered follow-ups

- **F-SPRITE-WALK-ROW (new, and it is owed).** The player now *renders* the sprite link walk. An
  earlier ruling deferred the walk "with a trigger: it gets an Aether contract row when something
  renders it" — **that trigger has fired**, and item 19 / D15 parity now wants a §6 row. Already
  recorded in `docs/2026-08-17-aeon-switchover-gap-list.md` (on `m68000-microop-framework`, not on
  this branch). Anchor: `lens/mod.rs` `models()`, the `render_line_report(0).sprites` read.
- **The compact chip's width is a live option.** Both forms are sized to `CHIP_COLUMNS = 24`, so the
  compact chip is now 147 device px at `px = 1` where it used to be ~69 — 61% of a 240-px picture.
  That is deliberate (see ruling 6 below) but untested against the owner's eye. Reverting is one
  line: relax `chip_width(px).max(fixed_width(lines, px))` in `lens/cpu.rs` back toward the widest
  line for the compact form only.
- **A hover memo**, if profiling ever shows it matters — deliberately not built (ruling 7).
  Anchor: `models()`'s hover arm.
- **Exporting the S/H-aware CRAM conversion**, if the strip ever needs to match shadow/highlight
  regions. Anchor: `cram_rgb_state` (private, `render.rs`) vs `Vdp::cram_decoded()`.
- **F-PALETTE-SCROLL** (S1, still open) — full scroll UI: truncation indicator, page keys; the
  picker list still paints top-down without scroll. Anchors: `palette.rs` draw break vs `move_sel`,
  and the picker branch of `draw`.
- **F-PALETTE-HINT** (S1, still open) — the startup banner string is hardcoded (`main.rs:908`);
  spec §4 wants it derived from the registry. Needs a decision, not just a refactor: the palette's
  own open key is not a registry row.
- **The non-gamepad deadzone literal** (S2, still open) — a bare `0.5` at `main.rs:389` with no
  compile-time tie to `gamepad::STICK_DEADZONE`. Feature-variant drift risk; commented in place.
- **Spec-§7 residue** (S2, still open) — no palette commands for aspect/scale yet; in-session hand
  edits to the config file are overwritten by any autosave.
- **F-CONFIG-COMMENTS (new, registered not fixed).** `Config::serialize` drops comment lines, while
  the file header it writes says hand edits are fine. Pre-existing S2 behaviour and *not* a
  regression — but S3 made it far more reachable, because **every lens toggle now schedules a save**,
  so a user who annotates their config loses the annotations the first time they press a lens
  command. The unknown-*key* preservation this slice shipped is the same problem's other half, and
  the same seam solves it. Anchors: `config.rs`'s `parse` (comment lines skipped) and `serialize`
  (writes known keys plus `unknown`).
- ~~**F-HOVER-UNDER-STATUS**~~ **CLOSED** the moment it was registered, and the classification that
  registered it was wrong. It was filed as "accepted, same class as toasts covering the ticker".
  That is the wrong class: a covered ticker line is **occlusion** — visibly covered, and the reader
  knows they are not seeing it — whereas a callout reading `tile $0_3` is **interference**, and
  `$03` is a perfectly valid answer. Transience makes it *worse*, not better: a callout is summoned
  deliberately to answer one question, read once, and acted on, so nobody re-reads it to catch the
  lie. `draw_hover` now flips clear of the band the same way it already flips off the picture's
  edge, keeping the callout beside its dot wherever it can.
- ~~**F-CONFIG-UNKNOWN-KEYS**~~ **CLOSED** in this slice, at both levels. Do not carry it forward.

## Rulings a successor must not silently reverse

1. **No default hotkeys for lens toggles.** Every obvious key is taken; binding one fails the
   existing `hotkeys_unique` invariant (mutation-verified). Rebinding is a later slice's job, and
   `lens_toggles_bind_no_keys_yet` pins the decision so it cannot erode by accident.
2. **No `Lens` trait**, despite spec §9 naming one. A model/draw fn pair per lens instead, because
   the lenses read genuinely different things — a hit ring, a register file, a palette array, a
   mouse dot — and a `draw(&System, &mut Frame)` trait would force every lens to re-read `System`
   inside the draw path, which is the opposite of the split every house test relies on. A deliberate
   deviation, recorded in the plan's self-review, not an oversight.
3. **Outlines draw beneath the panels.** The panels' content is opaque on purpose, so a hairline
   across one is *interference*, not occlusion: a missing glyph is visibly missing, but a `$3F` with
   a stroke through it reads as `$8F` and a swatch with one blended row reports a colour CRAM never
   held. Silent wrongness in a readout is the failure this slice keeps guarding against. The
   outlines pay almost nothing: at `PANEL_ALPHA = 190` an outline under a panel is dimmed, not
   erased, and the position it exists to convey survives.
4. **Draw order is deliberately NOT `LensId::ALL` order.** Layering is changed by moving an arm in
   `draw()`, **never** by reordering `ALL`, which is the registration order the config file's
   `lenses` value is written in — reordering it would rewrite every user's file spelling.
5. **Outlines follow the link walk, not the SAT table.** `sprites_decoded()` returns all 80 entries;
   the hardware only displays what is reachable by walking `link` from slot 0, and every other slot
   holds stale bytes. Outlining the table outlined ghosts — the owner saw this on real glass. Source
   is the already-public `render_line_report(0).sprites`; **no core change**. Line 0 suffices because
   reachability is a property of the link *fields* (frame-level state) and the per-line `outcome` is
   discarded on purpose — filtering to `Rendered` is the tempting wrong fix and would blank nearly
   every box.
6. **The CPU chip uses a fixed 24-column width for BOTH forms**, never "the widest line". Sizing to
   the widest line is what let a fifty-glyph PC symbol grow the panel to the full width of the
   picture; it would also gut the compact chip, whose whole purpose is the PC, down to the width of
   `SR $2700 S7`. The PC truncates into that width **from the front** (`fit_tail`) because the
   leading module path is the half that never changes while you watch the PC move. Registered above
   as a live option.
7. **No hover memo.** A memo keyed on the dot alone would show a stale callout on a *running*
   machine — a lens that lies, which is worse than a lens that costs a scanline resolve. And
   `set_target_fps(60)` means a paused iteration is a real frame anyway, so the memo would not even
   buy the paused case much. The free guards are kept: lens off, palette eating the mouse, cursor
   outside the picture (`window_to_native` returning `None` *is* the hit test).
8. **The CRAM strip has no degrading form**, unlike the CPU chip. A clipped strip would report three
   palette lines as four; a picture too small draws nothing at all.
9. **The CRAM strip shows the Normal ramp in shadow/highlight regions.** `cram_decoded()` is pinned
   by a core test to the renderer's decode at `PixelState::Normal`, and the S/H-aware
   `cram_rgb_state` is private. Accepted and documented at the module doc rather than papered over;
   exporting the private conversion for a swatch strip would be core surface this slice deliberately
   does not add.

## Branch lore — this cost real time to learn

- **`oracle-frontend` is bin-only.** There is no `[lib]` target, so a `pub fn` — or a struct field —
  with no *non-test* reader is a hard `dead_code` error under `clippy --all-targets -D warnings`.
  This **voided Task 1** (the forward map, built and gated and reverted, landing later with its first
  consumer), **merged Tasks 2-4** into one spine commit since each supplied the next one's callers,
  and defeated **three separate attempts** to hoist the shared sprite decode into a
  `Models.decoded_sprites` field ahead of its reader. **Land production code and its caller together.**
- **cargo's fingerprint is mtime-based.** A revert that restores an older timestamp makes cargo reuse
  the stale binary with **no "Compiling" line** — which corrupts mutation evidence in *both*
  directions (a mutation appearing to survive, or a restored line appearing to fail). `touch` after
  every write **and** every revert. A 6-mutation contamination sweep of earlier work was run this way
  and came back clean.
- **Vacuous-test shapes measured this slice** — a dozen-plus found, several of them in the plan's own
  authored test code. Worth reading as a checklist before writing the next assertion:
  a bound derived from the thing being mutated; a translucent black panel invisible against an
  all-zero scratch buffer (**fill with `0x0012_3456`, never `0`**); **white on white** (an `INFO`
  outline over `INFO` glyphs blends to white, so a layering mutation survived until the fixture
  sprites carried the priority bit); containment that does not pin position; membership that does not
  pin pairing (all 64 CRAM colours present, transposed, and useless); a boundary probe that misses
  the non-boundary symptom; assertions pinned at a single font scale (`margin = (2*px).max(4)` is 4
  at both px 1 and px 2, so a hardcoded 4 was identity everywhere the module looked); a clause
  vacuous because a coordinate happens to be zero; a self-cancelling expectation (a seen-array sized
  by `ALL.len()`, so a variant missing from `ALL` also shrank the check); a model built with no draw
  arm and an author who dutifully declared `draws_yet => false`; and a test passing for an unrelated
  reason — the H32 SAT cache mirrors only 64 entries, so an 80-slot "parse cap" fixture was measuring
  clipping, not the cap.
- **A green suite cannot distinguish a passing test from a test that asserts a value against
  itself.** Binding measured ink bounds to the same names as the expected tuple turned an assertion
  into `assert_eq!(got, got)`; it was caught by **clippy's unused-variable error, not by the test
  run**, and the scale row it was supposed to verify had until then never been verified at all.
  Every anti-vacuity technique on this branch — hand-written expectations, two-background opacity
  detection, opportunity counters — assumes the assertion compares two *independently derived*
  things, and **that assumption is itself unchecked**. Nothing in a test suite verifies it. Read new
  assertions for it explicitly.
- **A removed vacuous assertion can come back in the next edit.** A `left > area.x + area.w / 2`
  clause — a width claim wearing an anchoring costume — was identified and deleted on this branch,
  and then *reintroduced by the same author* three commits later in a new anchoring test, where it
  was again false for a correctly-anchored panel (at px 4 the chip is 588 px of a 700 px picture).
  Deleting an instance does not teach the shape; only writing the shape down does, which is what
  this list is for.
- **Test the *seam*, not just the layers.** Nothing on this branch drew a lens and the overlay into
  the same buffer — no test under `lens/` constructed an `Overlay` at all — so two real bugs shipped
  through a fully green suite, one of them at the **default** window size. Layer-local tests cannot
  see a layer-crossing bug however good they are, and every module involved was individually
  well-tested. `the_overlay_never_extinguishes_a_lens_glyph` closes it; the one-line offsets it
  forced are the cheap part. It earned its keep immediately: the *first* run after the status-line
  fix, with the sweep extended to a top-left hover dot, found a third instance nobody had predicted
  — the `PAUSED` banner garbling the hover callout — in the same run that confirmed the fix.
- **A follow-up ledger's line anchors rot silently.** Both `main.rs` anchors in this document were
  correct when S2 wrote them and were pushed ~25 lines down by the spine, then carried forward
  unchecked. Same lesson as the provenance one in a new place: **re-resolve an anchor when you copy
  it**, or cite a symbol name instead of a line number.
- **Provenance claims need checking as carefully as behavioural ones.** Three subagent reports
  carried an attribution that failed on inspection, and one over-claim survived into a commit
  message: `8384c45`'s prose said each half of the CPU-chip fix failed the collision test alone.
  Reverting only the scale drop leaves 287 px of clearance and it passes. The commit's mutation
  *table* was right; the sentence around it was not, and the in-code comment has been corrected in
  place (`da62d42`) rather than the history rewritten.

## Review-loop record (this slice)

The two-stage review paid for itself every round. Caught and fixed before merge: a ticker whose five
draw tests could not see its own panel; a `the_model_takes_the_newest_rows` test that never called
`model()`; register label→value pairing unpinned, so swapping both loops (every register wrong under
the right name) passed all 13 tests; `CpuRegs × paused` untested, so collapsing the register block at
the one moment it exists for passed all 32 lens tests; an expanded panel that *bailed* when it did
not fit, so turning on more information removed all of it; a CRAM draw test asserting membership
rather than pairing; a lens-arm witness with an escape hatch (a model built, no arm, `draws_yet =>
false`) closed by destructuring `Models` so a new field is a compile error at that line; the whole
sprite-outline source (SAT → link walk) and its first cap fixture, which was vacuous and produced
exactly the number it asserted; a PC truncated from the wrong end, invisible to every geometry test;
and a collision fixture whose symbol was too short, so the test only failed when both halves of the
fix were reverted together. Then, from the whole-branch review: the overlay erasing CPU-chip glyphs
at two window sizes, with no test anywhere able to see it; `margin = (2*px).max(4)` unpinned in
`cpu.rs` *and* `watch.rs` (hardcoding 4 left all 224 tests green in both), the lesson `1407d07` had
already learned for `video.rs` and applied to one module of three; and two test-local `margin_of(px)`
helpers that were second copies of the production formula, so they moved with the bug they were
supposed to catch. A self-inflicted tautology was also caught mid-fix — binding the measured ink
bounds to the same names as the expected tuple made `assert_eq!(got, got)` — by **clippy's
unused-variable error, not by the suite**, which is worth remembering about what a green run proves.
Then, from the reversal round: F-HOVER-UNDER-STATUS re-classified from "accepted" to a bug and
fixed, on the argument that occlusion and interference are not the same risk; a *fourth* instance of
the overlay seam surfaced by the extended harness within a minute of the third being fixed; a
vacuity guard that counted the wrong thing (callouts still sharing the banner's rows — zero by
construction once the fix landed, so it fired on correct code and had to be re-pointed at *dots
inside the banner*, an opportunity measure that owes nothing to the code under test); and two
anchoring tests whose fixtures were too small to exercise what they claimed, so a px-3 leg silently
measured a degraded chip instead of the register block.

Every evidence-bearing test in the slice carries a mutation line in its commit body — **153 of them**
across the code commits, including several recorded as *survivors found and closed* rather than as
clean kills, plus **three** recorded contamination sweeps re-running earlier commits' mutations with
explicit mtime bumps (all still failed; none had regressed to vacuous).

## Next

S4/S5 per spec — S5 owns key rebinding, which is what unblocks default hotkeys for the lenses. The
`lenses` config value is already forward-compatible in both directions (unknown keys and unknown lens
names both survive a save), which is what makes an older build reading a newer build's file safe over
those two slices.
