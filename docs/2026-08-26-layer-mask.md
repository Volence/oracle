# The layer mask — `emulator/get_layer_states` + `emulator/set_layer_enabled`

Served 2026-08-26 on branch `parcel/layer-mask`. Both fragments (`protocol.md` §6 lines 1136 and 1192,
amended by §11.22) were already vendored and final, so this is pure conformance: no change request, no
contract movement. Two more names leave `schema_conformance.rs`'s `SCHEMATIZED_NOT_ADVERTISED` set by
being **served**, which is the direction that pin was written for — its assertion went red on the commit
that shipped the handlers and forced the removal.

## What a mask is here

A **display** mask, and nothing else. `LayerMask` (`crates/oracle-core/src/render.rs`) is a *parameter*,
never a field: no `Vdp` and no `System` holds one. The engine holds the authoritative mask
(`Engine::layers`), beside the watchpoints, and hands it to the pure `&self` renders.

Three properties fall out of that placement, and none of them could be had any other way:

| Property | Why it holds |
|---|---|
| Not in `state_hash` / `memory_hash` | The mask is in no bincode snapshot and no hash input. `state_hash {includeFramebuffer}` additionally passes `LayerMask::ALL` **explicitly**, so even the picture digest is of the unmasked frame. |
| Survives `reset` / `reload_rom` / `restore` | All three replace `self.sys`; none touches `self.layers`. |
| Cannot perturb emulation | `Vdp::render_scanline` — the one render that commits the sprite-overflow / collision latches and the R10 dot-overflow carry — takes **no mask argument and has no masked twin**. There is nothing to thread, so no caller can reach chip state through a mask. `System::run` is byte-for-byte unchanged. |

## The three design calls worth re-reading

1. **A masked layer is not a candidate.** The mask reaches exactly one place, `Vdp::rr9_winner`'s candidate
   tests, where a masked layer is skipped exactly where a transparent one is. Whatever is behind it wins,
   and the fall-through still ends at the backdrop. Blanking the winner afterwards would paint backdrop
   over dots plane B was visible at — the believable-wrong-answer failure, and it looks right on a
   screenshot of a simple scene.

2. **The mask decides what is drawn, never how a surviving pixel looks.** `sh_state` is handed the
   *unmasked* pixels. R11's shadow/highlight default is derived from the planes' priority bits, so
   re-deriving it post-mask would darken plane B the moment a high-priority plane A above it was hidden —
   masking one layer changing the colour of another.

3. **Masking `window` falls through to plane B, not to plane A.** The window and plane A share one
   rendering slot; inside the window span the hardware fetches the window's cell and plane A is never
   sampled there. Substituting plane A would *synthesise* a picture the hardware cannot produce rather
   than remove one from it. This keeps one rule ("the next candidate wins") instead of a special case, and
   keeps the mask strictly subtractive — an invariant pinned dot by dot.

## The surfaces

`emulator/screenshot`, `emulator/scanlines` and `emulator/pixel_attribution` all read the same mask.

* `Engine::framebuffer` now takes its mask **explicitly**, so each call site states which picture it wants.
  There is deliberately no zero-argument version to fall into.
* A masked read **cannot use the latched raster frame** — that frame was composited during the run by
  `render_scanline`, which takes no mask. So it answers `source: "stateRender"` with a caveat naming the
  masked layers. Each row's pre-existing caveat text is byte-identical for the case it was written for;
  the mask text is additive, and takes precedence when a mask is set because it is then *the* reason the
  read is post-hoc. Clearing the mask puts the raster frame straight back — nothing is discarded.
* `pixel_attribution` reports **what was drawn**: the post-mask winner, `rgb` equal to the masked render's
  pixel, and the masked layer **absent from `candidates`**. The list means "every layer that could have
  shown", and a masked one could not; the closed `verdict` vocabulary also has no word for it
  (`lostToPriority` names a reason that did not happen, `transparent` misreports opaque art, `operator`
  means a sprite operator), and inventing one would be a unilateral contract change. `minItems: 1` already
  admits short lists.

## The vocabulary is derived

`mask_key` is an **exhaustive match on the core's `Layer`** — a new variant cannot compile until it
declares whether a mask reaches it — and `mask_targets()` is the single source for the getter's key set,
the setter's accepted values, the refusal message and the caveat.
`layers.rs::the_mask_vocabulary_is_the_contract_fragments_own` parses the vendored schema and proves that
set equals **both** fragments' (§11.22's "the setter's enum IS the getter's key set", discharged by parse
rather than by reading), and that the server's own generated key set equals it too. `backdrop` is `None`:
a pixel-attribution layer, not a mask target, exactly as the fragment's `$comment` says.

## Refusals

No fragment on this bus declares an error condition, so the whole error surface is prose. An unknown
`layer` **value** is `-32602` in the `parse_watch_space` house spelling: name the field, list the accepted
set, carry it as a typed array in `error.data`.

The §2.5 params closure could **not** have produced that refusal, and that is not a gap: `unknown_params`
is a closure over param *keys*, and `layer` is a declared key with an out-of-enum value. The two failures
are pinned separately so they cannot be confused.

## Known gaps

* **The player GUI has no layer-mask surface.** `oracle-frontend` draws its own window and its pick panel
  calls `Vdp::pixel_attribution` (unmasked) directly, so in the hosted arrangement a mask set over the bus
  changes the bus's answers and not the window's. `pick.rs`'s "this panel and `emulator/pixel_attribution`
  must never disagree" invariant is therefore now conditional on no mask being set — its test still holds
  because it runs on a default (all-on) engine. A GUI toggle plus a masked `pick::resolve` is the obvious
  follow-up; it is named here rather than left as an omission.
* **Not exercised against a real ROM.** Every gate here runs on hand-posed VDP fixtures and the synthetic
  test ROM. A live pass — mask each layer on a real game frame and look at the screenshot — is foreground
  work.
