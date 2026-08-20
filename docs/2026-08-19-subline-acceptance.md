# Sub-line acceptance: adoption clause 3 discharged — CR-25 fully adopted (2026-08-19)

Addendum to `docs/2026-08-19-subline-shipped.md` §7. The demand side executed the acceptance
protocol (clause 3 of §11.15's adoption condition, empyrean `d72513c`) against `oracle-aether`
built at `ff9e784`, sweep driver unchanged in form. **Verdict: PASS on both acceptance numbers;
the model is not falsified.** Reported by the Aeon overseer, 2026-08-19; measurements theirs.

## The two numbers

- **flipX is a measurement.** Row-100 fixture, spin 4: **flipX = 219**, inside the pinned honesty
  band [205, 225] (prediction ≈ 222). Neither falsifier (0 / 319) observed anywhere. The flip
  marches with N — 186 → 195 → 199 → 219 → 219 → 221 → 230 → 240 → 246 over N = 0..8 — and their
  fit over 16 moving flip points gives **0.849 px/cyc** vs the arithmetic 0.875, inside the
  instruction-granularity bounds. Caveat theirs: their ROM has moved several parcels past the
  prediction's 1b-era basis (dispatch-chain and SR parcels touched the pre-burst path), so the
  in-band agreement is slightly better than the raw 219-vs-222 reads.
- **Distinct pictures: 19 of a possible 20 at step 3** over N ∈ 0..57 (their driver has no step-1
  mode), against the pre-change 4 — the step difference disclosed on both sides, and at step 3 the
  ceiling is 20. **The spec's LITERAL A2 now passes**: N = 0 vs N = 17 differ by 102 columns on
  row 100 (x 186..319) — the exact check that was structurally impossible on the atomic-row
  instrument.

`source == "raster"` was asserted throughout and never fired; repeat runs were byte-identical
(A1-style determinism held on the new instrument).

## Confirmed in passing

The morning's row-indexing question closes the way `docs/2026-08-19-aeon-acceptance-results.md`
predicted it would once sub-line landed: HInt-fixture boundaries now appear **in** the landing row
as mid-row flips, and their row-101-vs-100 disagreement with the GUI oracle is collapsing.

## Their side's follow-on (recorded, not ours)

The sweep driver's boundary-crossing analysis and solver-fit sections are atomic-era logic — on the
new instrument they misread split rows as 23 "boundaries" and print a spurious NO-GO. Aeon is
booking a sub-line mode on their side; the payoff they name: the blanking window's EARLY edge
becomes directly measurable for the first time (previously derived), tightening their anchor's
standard error and re-opening their parked 4-word-burst question with a better instrument.

## Adoption-condition status (§11.15, empyrean `d72513c`)

| clause | state |
|---|---|
| 1 — rewritten two-timings gate, band derived from source | green in-suite since `d1cd2d9`/`5cd87c7` |
| 2 — zero-mid-line-write byte-identity | green in-suite (slice 3 poison, re-proven in slice 4) |
| 3 — demand-side sweep re-run | **discharged 2026-08-19, PASS — this document** |
| 4 — watch-hit clock gate | green in-suite since `01866a7`, mutation-checked |

**CR-25's adoption condition is fully satisfied. The arc is closed.**

## Follow-up (2026-08-19, later): the upgraded instrument closes their raster chain — five-quantity corroboration

Aeon's sweep driver gained its sub-line mode (aeon `6a9ba181`) and immediately closed their entire
raster-timing chain by direct measurement. Every figure below is also a precision statement about
this renderer's landing model — independent physical quantities recovered through §11.15's
`x = floor(d/p)` convention:

| quantity | measured | reference | agreement |
|---|---|---|---|
| blanking early edge | N = 16.028 ± 0.070 | — | **first observation on any instrument** |
| blanking late edge | N = 28.267 ± 0.076 | — | — |
| window width | 122.39 ± 1.07 cyc | arithmetic 122.86 | 0.44 s.e. |
| px/cyc | 0.8740 ± 0.0027 | 0.875 (the definition) | sub-cycle |
| line period | 488.51 ± 0.25 cyc | 488.57 | sub-cycle |

Their shipped anchor confirmed at 0.88 s.e.; their parked 4-word-burst ceiling CLOSED as a clean
refusal (early slack −0.28 cyc against a 1.41 bar, consistent across four sweeps) — "the better
instrument turned a marginal maybe into a verdict."

Two instrument notes, recorded not asked: a multi-word burst's flipX brackets the FIRST entry's
landing (consistent with the instruction-granular one-landing pin; their edge fixtures now use
single-word bursts; `F-SUBLINE-ACCESSMCLK` remains the refinement for the residual ~1-cycle
art-sampling bias they midpoint-correct); and their five atomic-era anchor fixtures reproduce
their verdicts to the pixel across the convention change — the amendment was backward-compatible
exactly as CR-25 argued.
