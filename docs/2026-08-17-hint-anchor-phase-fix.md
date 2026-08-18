# The HINT anchor-phase fix — why the OJZ water line was all-or-nothing

**Date:** 2026-08-17 · **Status:** fixed, reviewed, committed · **Verdict:** oracle-next bug; aeon correct.

## Symptom (owner-reported, reproduced)

In oracle-next, aeon's OJZ (Oracle Jungle Zone) act-1 water line never rendered as a mid-screen
boundary: the screen was either fully bright (dry) or fully dark (shadow + water tint), flipping
all at once when the camera crossed the water region — and plane B visibly jumped down when it
flipped. The reference oracle draws the anchored boundary tracking world Y 224, dims below it,
and only goes whole-screen once the line scrolls off the top (the intended S3K
`Water_full_screen_flag` behavior, which the frame-top "ship" replay reproduces correctly in
both emulators).

## Root cause

The HINT line-counter bookkeeping (decrement / underflow / reload from reg 10) ran at scanline
**start** (`Vdp::on_line_start`, driven by the Scanline event), with only the pending-latch
raise delayed to the pinned H anchor. Recon R7 (docs/2026-07-16-vdp-recon.md) pins the
underflow/flag at **H = $A6 (H40) / $86 (H32)** — ~79% through the line — and "writing reg 10
does not load the live counter; the value takes effect at the next reload" (TmEE,
reload-on-event). R7's exposure column even named "mid-line-write" as the risk.

Aeon's raster tier arm-chains reg 10 from inside the HInt handler (the classic S3K waterline
idiom): each fire's handler writes the gap to the next fire. Those writes land mid-line — after
our line-start reload had already grabbed the stale value. With the schedule's initial reg10=0
the counter re-reloaded 0 every line, so records meant for fire lines {0, 1, 79, 221} cascaded
onto lines {0, 1, 2, 3}: `$8C89` (S/H on) + the water palette swap applied from the top of every
frame (whole-screen "shadow mode"), and the line-222 plane-B vscroll split applied from line ~3
(the BG jump).

Probe evidence (crates/oracle-core/examples/sh_probe.rs, aeon s4.debug.bin, spawn state,
water line = screen 80): before the fix, reg-10 arms observed at lines 1/2/3 with reg12=$89 at
line 3; after, arms at lines 1/2 and **reg12=$89 at line 80**, park at 221, VBlank restore —
and the live per-scanline frame shows the anchored boundary (row brightness 75.9 → 8 across
lines 80→86), matching the reference.

## Fix

`EventKind::HInt` is now scheduled unconditionally every line at `deadline + hint_offset()`,
and the bookkeeping (`Vdp::hint_anchor_tick`, the renamed `on_line_start`, body unchanged) runs
*inside* that event, recovering its line from the event's own deadline. Constant-reg-10
behavior is identical (same fire lines, same pending-raise instant); the only change is that
reg-10 writes in the first ~79% of a line are visible to that line's reload — the hardware
ordering.

## Verification

- New system-level pin: `system::tests::hint_reg10_rewrite_after_line_start_is_seen_by_that_lines_anchor_reload`
  — proven to FAIL on the old phasing ("no HINT for K lines after the mid-line re-arm to K")
  and pass on the fix. The vdp-level test pins the reload-reads-live-reg10 half.
- Full workspace suite green (1463/0), goldens, scanline goldens, determinism gate, SST suites —
  **zero currency movement**, by construction and by measurement.
- Adversarial review verified: exact line recovery from the deadline (both anchor offsets
  < MCLK_PER_LINE and below the H-counter jumps), deterministic event ordering, savestate
  round-trip soundness (same-version), negligible cost (one extra event/line).

## Side findings

- Aeon's "S/H CONTENT FINDING" comment (games/sonic4/data/effects/ojz_effects.emp:194-205,
  claiming the art is all high-priority so S/H has nothing to dim) is stale: the built ROM's
  plane-B nametable is entirely low-priority and plane A is mixed, so the band is plainly
  visible. Flagged for an aeon-side comment/docs fix.
- Sub-line nicety, not pursued: the water fire's `$8C89` lands ~25% into the boundary line, so
  hardware would flip S/H mid-line; our per-line renderer applies it from the next line
  (boundary at 81 instead of mid-80). Same class as the R11 DAC-calibration deferral.
- Cross-version savestates (pre-fix snapshot into post-fix code) can skip or double one
  counter tick at the restore line; same-version round-trips are exact. Release-note line only.
