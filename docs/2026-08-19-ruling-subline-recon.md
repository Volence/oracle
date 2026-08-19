# Ruling on the F-SCANLINE-SUBLINE recon's §G questions (2026-08-19, controller)

Applies to `docs/2026-08-19-subline-recon.md` (`d1c9ba0`). The recommended timing model — eager
resolve at line start, deferred segmented **decode** at line close — is **ADOPTED** as designed,
including both hard implementation constraints (one complete `on_scanline` row per line, segments
internal; journal coalescing by pixel-x) and the 8/10-mclk-per-pixel axis over the 422-position
H-counter grid. The two spot-verified load-bearing claims (`resolve_line` is index-domain and never
reads CRAM; `state_hash`/`export_state` hash no rendered pixel) held on firsthand inspection.

Standing owner directive applied throughout ([2026-08-19]): a demand spec or legacy surface is the
compatibility **floor, never the ceiling** — this design already exceeds the floor (the retained
`Vec<PixelResolution>` makes F-SCANLINE-INDEX/SH sink-extension-cheap; slice 1 discharges the hard
half of F-TRACE-VDPWRITE-MCLK), which is why it is adopted rather than sent back.

## Q1 — Authorized; vehicle = numbered CR + §11.15

The behaviour change is authorized (owner scheduled the item explicitly, 2026-08-19 morning).
Vehicle: **a numbered CR and a §11.15 amendment entry**, not plain prose. `112d683`'s prose-only
rationale was *"adds no behaviour"* — that reasoning now points the other way, and a client written
against the reference server's stated convention deserves the amendment log even though no wire
shape or fragment changes. Sequencing: the CR is drafted and adjudicated **while slices 1–3 (all
behaviour-neutral) proceed**; the empyrean prose correction merges in the same window as the arc's
oracle-next merge, so `contract/protocol.md` is never wrong on `main`. Q6's one-sentence
`pixel_attribution` divergence note rides the same CR (see Q6).

## Q2 — Re-pin approved, cross-check preserved

Both `color_1536` literals (`scanline_goldens.rs:130`, `conformance_roms.rs:83`) re-pin **together,
to one measured value**, in slice 5. The cross-check assertion between the two files is **preserved,
not dropped** — it is what keeps them honest. Mechanism goes in the `cause:` comment in the file's
own style, and the arc's handoff doc carries a per-ROM moved-goldens table (owner-visible, the
standing rule).

## Q3 — Flips allowed, per-row justified; unexplained movement is a stop

`IDENTICAL-TO-POST-HOC → LIVE-DIFFERS` flips are **allowed** for `direct_color_dma` and (if
measured) `cram_flicker`: a flip is a strictly better pin capturing the mechanism this corpus exists
to catch. Each flip carries its mechanism (which writes, which lines, which indices) beside the row.
**Any mover without an explained mechanism — including any of the 9 low-risk rows — is a bug
report, not a re-pin: stop the slice and surface it.**

## Q4 — Fold F-TRACE-VDPWRITE-MCLK in, as its own slice

In-arc, as **slice 1b** with its own tests and its own commit (never smuggled into slice 1's
diff). It retires a thrice-registered follow-up at the moment its hard half is already paid for,
and the `watchpoints.rs:819-821` caveat text that names the gap on the wire gets updated in the
same commit — check whether any pinned test asserts that caveat string before editing it.

## Q5 — Band + structure, band derived from source

The rewritten `a2` gate pins **structure** (a split row: uniform colour-A prefix, uniform colour-B
suffix, exactly one transition; full rows above; the two-ROM band swap retained) plus a **band** for
the transition column. The band is **derived in the test from source constants** (poll-loop cycle
cost × mclk-per-CPU-cycle ÷ mclk-per-pixel — invariant: expectations derived, never copied from the
recon's estimate or a single measurement), with the derivation in a comment. An exact column is
declined as a flake risk (128,005.71 CPU cycles/frame ⇒ phase drift is real). Determinism of the
column within one boot stays covered by the A1 gate.

## Q6 — Document only

`pixel_attribution`'s within-row divergence from `emulator/scanlines` gets **one sentence**, folded
into the Q1 CR (same vehicle, zero extra process). Closing the divergence is `F-SCANLINE-INDEX`,
which this arc has priced down but not scheduled.

## Execution order

1. Slices 1, 1b, 2 (dead-code + follow-up retirement) — one dispatch, per-slice commits.
2. CR draft + adjudication in parallel (empyrean prose diff prepared, not merged).
3. Slice 3 (neutrality) — own dispatch, own review; the arc's currency-neutrality claim lives here.
4. Slice 4 (behaviour + `a2` rewrite, same commit) — after the CR is adjudicated.
5. Slice 5 (measure, justify, re-pin) — TAGged checks resolved by running the suite, per-row rules
   above.
6. Slice 6 (contract merge + Aeon re-run: `flipX ≈ 222` and restated-A2 distinct pictures ≥ ~30 are
   the two acceptance numbers; ping `aeon-e0` for the same-session run they offered).
