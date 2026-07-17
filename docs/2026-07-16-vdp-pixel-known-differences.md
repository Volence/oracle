# VDP pixel known-differences ledger

**The frame-level analogue of `tools/blastem-differential/known_differences.py`.** When a cross-emulator or
`s4.bin`/Exodus golden-frame differential eventually runs (design brief §5 rung 2/5), the framebuffer will
diverge from hardware at a small, *enumerated* set of dots — each an interim model chosen deliberately because
a permitted source did not pin the exact sub-case, or a documented scanline approximation. This ledger names
every one so the differential **attributes** the divergence instead of false-alarming, and so a future
refinement has a checklist. Nothing here is a bug; each row is a recorded model with a pin mechanism.

The golden-frame regression tests (`crates/oracle-core/tests/golden_frames.rs`) pin the *current* model of
each row as a self-consistency hash — a scene whose framebuffer depends on that model. A self-consistency pin
locks the model against silent drift; it does **not** by itself prove the model matches hardware. Confirming
(or amending) a row against hardware is the "confirm-when" column.

| # | Interim model | Recon | Why divergent-by-design | Golden scene that locks it | Confirm-when |
|---|---|---|---|---|---|
| P1 | **No mid-sprite pixel-budget cut.** A sprite that overshoots the per-line 320/256-px budget draws **fully**; the *next* on-line sprite is the one dropped (`DroppedPixelBudget`). Hardware cuts mid-sprite at the exact dot the budget runs out. | R10 / RR8 | The scanline model spends budget per whole sprite (push-4 must-ledger); the mid-sprite dot is a slot-granular effect. | `scene_no_mid_sprite_cut` (nine 32-px sprites = 288 px > 256) | s4.bin/Exodus golden frame with a budget-straddling sprite, or a 240p sprite-limit ROM. |
| P2 | **R5 SAT-cache window base — H40 bit-0 mask.** The write-through window base is `(reg5 & 0x7E) << 9` in H40 (bit 0 masked), `(reg5 & 0x7F) << 9` in H32. | R5 / RR8 (open remainder 1) | The exact H40 masking of the *window* base (vs evaluation base) is not instrument-observable over the RSP stub; modeled as the consistent extension of the pinned evaluation mask. | `scene_r5_cache_window` (reg5 = `$59`, H40 → masks to `$B000`) | s4.bin golden frame or a cache-mixing ROM (Bloodlines-style) captured on hardware/Exodus. |
| P3 | **R5 byte-granular SAT write-through.** Odd-address VRAM byte writes into the cached half update the matching cache byte (the write-through is per byte, not per word). | R5 / RR8 (open remainder 2) | Sub-word SAT pokes are not discriminable via the CPU-readable overflow/collision proxies. | `scene_r5_cache_window` (odd-address poke into entry-0 Y) | s4.bin golden frame with a byte-granular SAT write, or a hardware cache probe. |
| P4 | **R8 partial-column v-scroll extent.** In 2-cell v-scroll with `hscroll & 15 != 0`, the leftmost **16-px** column uses `VSRAM[$4C] & VSRAM[$4E]` (H40) / fixed 0 (H32); the rest scroll per-column. | R8 (model choice) | Eke's Model-2 rule is pinned, but the exact partial-column *extent* (16 px) and cross-revision variance are ledgered. | `scene_r8_partial_column` (H32, plane-B hscroll 4) | s4.bin golden frame at a 2-cell-scroll boundary, or the cross-revision hardware note. |
| P5 | **R9 window-bug sub-tile alignment.** Left window + plane-A `hscroll & 15 != 0`: the first 16 px right of the boundary reuse the window's **last-column tile**, sampled at plane A's fine-scroll offset. The tile identity is pinned (official); the exact sub-tile alignment is interim. | R9 (official; sub-tile open) | The reused-tile *sub-pixel* offset is the R9 open remainder. | `scene_r9_window_bug` (left window WHP=3, plane-A hscroll 5) | s4.bin golden frame across a left-window boundary. |
| P6 | **mode-01 / mode-10 h-scroll byte offsets.** Per-line (`01`) offset `(line & 7) * 4`; per-cell-row (`10`) offset `(line & !7) * 4`. | RR5 (medium confidence on the two intermediate modes) | A permitted *verbatim* formula for the two intermediate indexing modes was unavailable; modeled from the table structure. | `scene_hscroll_modes` (mode 10, per-band h-scroll) | s4.bin golden frame using per-cell/per-line h-scroll (Sonic water/HUD splits). |
| P7 | **Invalid plane size `0b10` clamp.** HSZ/VSZ code `2` (`0b10`), absent from the permitted valid set, is clamped deterministically to 64 cells. | RR3 (open remainder) | No permitted source gives hardware behaviour for the invalid code; deterministic model, no fixture uses it. | (none — no scene sets it; recorded model) | A hardware probe of the invalid size code, if ever needed. |
| P8 | **Shadow/highlight DAC calibration.** Intensity uses our fixed integer ramp (`Normal Min→Max`, `Shadow Min→½Max`, `Highlight ½Max→Max` on a shared 0..14 quantization), not the measured DAC output levels. | R11.5 (deferred remainder) | The introspection API reports CRAM values + our fixed ramp; exact DAC levels are a rendering-calibration deferral. | every S/H scene (`scene_priority_sh`) | Measured DAC levels (SpritesMind t=2188) if calibrated RGB is ever required. |

## How the differential consumes this

A future frame-level differential (cross-emulator or s4.bin/Exodus) should treat a divergence at a dot as
**expected** iff it maps to a row above — the dot lies in the region the row describes (a budget-straddling
sprite, the leftmost 16-px 2-cell column, the 16-px right-of-window-boundary band, an invalid-size plane, an
operator/S/H dot for P8, …). Any divergence that does **not** map to a row is a real finding. When a row is
confirmed against hardware, either the model is validated (delete the row's "interim" caveat) or it is amended
with evidence (a deliberate golden-hash regen in `golden_frames.rs`, never silent).

## BlastEm frame-capture feasibility

Upgrading a self-consistency pin to a cross-emulator confirmation needs a BlastEm framebuffer for the same
fixture. That feasibility is a bounded spike this push — see `tools/blastem-differential/frame_capture_spike.md`
for the findings. If a usable capture path exists, the affected rows move from "self-consistency" toward
"confirm-when" satisfied; if not, the rows stand as self-consistency pins (the honest state), and the s4.bin
golden-frame rung (a real ROM) remains the eventual confirmation.
