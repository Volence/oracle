# `F-FROZEN-FIXTURE-DRIFTS` — the call, and what would have to be true for it to be wrong

**2026-09-06.** Row: *"our frozen copies of your game's symbol file drift away from the real one, and
the drift is exactly where new faults live … Worth a check that notices when the two have diverged in a
dimension the tests actually rely on."*

Written because this lane keeps a ledger of calls made without an independent reviewer. Everything below
was measured firsthand in this worktree; where a brief or a subagent supplied a number, it was
re-derived before use, and the two places that changed under re-derivation are named.

---

## 1. The premise I was handed, and the one part of it that is false

I was told: *the `.lst` files are not tracked in aeon, therefore there is no committed upstream revision
to diff against, therefore any drift check must read a peer's live working tree.*

The first clause is true and I verified it (`git -C ../aeon ls-files --error-unmatch s4.lst` fails
against a working control of 1519 tracked files; sigil tracks zero `s4.lst`/`demo.lst` either — its
`.lst` hits are unrelated probe fixtures under `docs/`).

**The conclusion does not follow, and this is the load-bearing correction.** The listings are untracked,
but *the source the dimensions come from is tracked*. `ObjSub_Spring__Up_Red` is not an assembler
invention; it is `pub equ ObjSub_Spring__Up_Red = spring_subtype(SPRING_DIR_UP, 0)` in
`games/sonic4/objects/test_solid.emp`, committed. So a currency check **can** read a committed revision
through aeon's object store, exactly as `aeon_pin_report.py` already does for sigil, and the live-tree
hazard is avoidable for the primary measurement rather than inherent.

That reframing is why the shape below has two sources instead of one.

## 2. The call

**A hermetic gate for recovery; a non-gating reporter for currency. Both driven by one new manifest.**

This is not a new pattern; it is the pattern this repo already ratified one level down:

| | bytes | shapes |
|---|---|---|
| manifest | `fixtures/aeon/PIN.tsv` | `fixtures/aeon/DIMENSIONS.tsv` *(new)* |
| recovery gate (hermetic, in-suite) | `crates/oracle-replay/tests/aeon_pin.rs` | `crates/oracle-core/tests/aeon_dimensions.rs` *(new)* |
| currency reporter (out-of-band, exits 0) | `tools/aeon_pin_report.py` | same file, new section *(new)* |

The split follows `SCHEMA-DRIFT-NIGHTLY` (`docs/OVERSEER.md`, hub ruling anchored at empyrean
`1e9d70c`): *the hermetic default is the ratified shape, and drift detection is a nightly's property,
never a local run's.* A gate that reddens because aeon moved would put the whole gradient behind bending
our side until it passes — which is how a pin gets moved to make a red test green, the one thing
`PROVENANCE.md` forbids in terms.

### What the hermetic half actually buys, since it cannot see aeon at all

This was the part I expected to be ceremony and it is not. Three properties:

1. **It measures through `SymbolTable`, the API consumers call, never by re-grepping the file.** A
   dimension present in the bytes but no longer reachable through the parser is exactly as invisible to
   a consumer as one that was never there. This paid on its first run: my grep-derived manifest recorded
   `demo*.lst` as having 0 `ObjDef_`; the parser said 1 (`ObjDef_DemoBox`) and was right — I had never
   measured demo and had assumed from the s4 files.
2. **It makes the blind spots countable and printed.** Sixteen of twenty-four probed dimensions are
   absent from the frozen listings. Before this, that fact existed only as one `loud()` note inside one
   test (`symbols_real_lst.rs`'s phase test, which is scrupulous about its own vacuity) and nowhere at
   all for `ObjSub_`, `ObjDef_` or `Level_Width`.
3. **It fails when the pin moves and the manifest does not** — the dimension-resolution form of the
   failure a byte pin structurally cannot see, because in that failure the bytes are exactly the bytes
   we recorded.

### The one property without which the whole hermetic half is theatre

Most manifest rows record `0`. **A `0` from a broken probe and a `0` from an absent dimension are the
same artifact**, and the first would certify the second forever. So `every_probe_kind_can_report_presence`
runs each probe against a synthetic listing that *does* carry the dimension, at the manifest's own `arg`
values.

This is not hypothetical. Mutation **M2** below breaks `equate_prefix_count` to always return `0`:
`manifest_matches_the_frozen_listings` stays **green** and prints *"24 rows over 4 listings, all matching
the frozen bytes"*, because the manifest records `0` for every `ObjSub_` row and the dead probe returns
`0`. Only the positive control catches it. A version of this work without that test would have shipped a
manifest whose most important column could not be trusted, with a green run asserting otherwise.

## 3. Red-first evidence

Four mutations, parameter varied deliberately (a repeated parameter is how a guard called the strongest
in this tree kept a hole). Each applied to a **committed** baseline, quoted back from disk, then restored
with `git checkout HEAD -- <path>` from a tree verified clean.

| # | mutation | applied line, from disk | result |
|---|---|---|---|
| M1 | manifest integer no longer matches bytes | `s4.lst\tequate_rows\t-\t724\t…` | **RED** `manifest_matches_the_frozen_listings` — *"manifest says 724, the frozen bytes give 723"*. Other two green. |
| M2 | probe always returns 0 | `"equate_prefix_count" => Some(0), // MUTANT M2` | **RED** `every_probe_kind_can_report_presence` only — *"cannot see a dimension that IS present in the control listing"*. **Manifest test stayed GREEN.** |
| M3 | strip the stake from an absent row | `s4.debug.lst\tequate_prefix_count\tObjSub_\t0\t…\t-` | **RED** `absent_dimensions_are_recorded_with_who_relies_on_them` only. |
| M4 | the `Option` path, not the integer path | `s4.debug.lst\tphase_count\t-\t6\t…` | **RED** `manifest_matches_the_frozen_listings` — *"manifest says 6, the frozen bytes give absent"*. |
| M5 | a pinned listing with **no rows at all** (all six `demo.debug.lst` rows deleted) | manifest rows for that file: 0 | **RED** — *"these frozen listings have no row in DIMENSIONS.tsv and are measured for nothing: demo.debug.lst"*. Added after M1–M4, when the gate was found to iterate the manifest and never ask the directory. |

Reporter controls (it has no assertions, so its failure mode is a *quiet* one): `--aeon` at a
non-checkout, an unresolvable `--aeon-ref`, and `--sigil` at a non-checkout. The first two print
`UNMEASURABLE … This is NOT 'the shapes are current'`; the third proved a defect in my own first draft
(the byte half's early returns dropped the entire new section with no line printed) and now routes
through `_finish()`.

Runner: `cargo test -p oracle-core --test aeon_dimensions`, which the default `cargo test --workspace`
selector picks up as an integration test of `oracle-core`. The reporter's new section rides the runner
that already called it, `tools/replay_playthroughs.sh` (which invokes `aeon_pin_report.py || true`).

**The blind-spot inventory needs no extra CI step**, and that was measured rather than assumed. The
`aeon_pin.rs` precedent needs one because libtest swallows a *passing* test's stdout; this file uses the
house `loud()` pattern on fd 2, which the capture does not touch. Confirmed in the M1 run, which had no
`--nocapture` and still printed the full sixteen-row inventory.

**Whole-suite state at the tip of this work**: `cargo test --workspace`, debug profile, default selector
(not `--all-targets`): **2556 passed, 0 failed, 6 ignored** across 78 suites. `cargo clippy --workspace
--all-targets -- -D warnings` exit 0. `cargo fmt --all -- --check` exit 0. The `vendor/` symlink was
created before any of it — without it eight `save_state` rows fail while the whole 68000 sweep skips and
passes, so a total taken without it would not mean what it says.

## 4. What I measured, and the two numbers I corrected

Frozen (`fixtures/aeon/`) against aeon's build, all re-derived here:

| dimension | frozen | aeon |
|---|---|---|
| `ObjSub_` equates (`s4.lst`, `s4.debug.lst`) | 0 | 8 |
| `ObjDef_` archetypes (`s4.lst`) | 3 | 4 |
| `ObjDef_` archetypes (`s4.debug.lst`) | 5 | 6 |
| `Phase Table` (`s4*.lst`) | absent | `PHASE-COUNT 6` |
| `Phase Table` (`demo*.lst`) | absent | `PHASE-COUNT 2` |
| `N equates` (`s4.lst` / `s4.debug.lst`) | 723 / 724 | 768 / 769 |
| `Level_Width`, `Level_Height` | absent | `$FFFFEA70`, `$FFFFEA72` |

**Correction 1.** My brief offered `ObjDef_` line-hit counts of 6 vs 8 in `s4.lst`, correctly flagged as
not an archetype count. The archetype counts are **3 vs 4** (`s4.lst`) and **5 vs 6** (`s4.debug.lst`),
the missing one being `ObjDef_Spring` — the archetype the spring picker is about. The 5-vs-6 figure is
the row's own "five objects where the real one had six", now reproduced from the bytes.

**Correction 2.** `demo*.lst` `ObjDef_` is 1, not 0. Caught by the gate on its first run, not by me.

**Addition.** `Level_Width` / `Level_Height` were not in the row and are the sharpest entry in the table,
because there is a *recorded field sighting* behind them: `crates/oracle-aether/tests/symbol_freshness.rs`
documents a session that trusted `symbolsDropped: false` and got `-32013 no symbol named or prefixed
Level_Width` for a symbol sitting in the listing on disk — and filed a false defect report against the
peer lane. The frozen fixture could not have caught that then and cannot now. Adding it forced a probe
that is not a namespace count (`symbol_present`), which is itself worth knowing: a manifest that could
only count prefixes would have missed the one dimension with a field sighting attached.

## 5. What would have to be true for this to be wrong

Stated as falsifiers, not as caveats, so a later session can check them rather than re-argue them.

1. **If a byte-level currency source for the listings appears, the live-tree column should go.** The
   secondary source in the reporter reads aeon's working tree, which can be mid-edit. I accepted that
   *only* because it is non-gating, stamped with size and mtime, labelled as a working-tree read, and is
   the only place an exact count exists. If sigil ever freezes `.lst` goldens, or aeon commits its
   listings, that justification evaporates and the column should be replaced, not kept alongside.
2. **If the object-store proxy's error rate turns out to matter, the primary is wrong.** It greps a
   token in tracked *source*. A name that appears only in a comment counts as published; a name the
   assembler generates counts as absent. Today it agrees with the live measurement everywhere it can be
   compared. If a drift row is ever raised by the proxy and contradicted by the live column, believe the
   live column and demote the proxy to presence-only.
3. **If the manifest starts being edited to make the gate green, the design has failed.** The gate's
   red says "the pin moved and the manifest did not", and the correct response is to move the manifest
   *and re-read the blind-spot rows*, because a dimension that appeared may now deserve real coverage.
   The failure mode is a session that bumps a number to get green and never reads the second half of
   that sentence. Nothing in the code can prevent this; the failure message says it in words, which is
   the most that is available.
4. **If nobody ever runs the reporter, the currency half is worth nothing.** It is non-gating by ruling,
   so its only guarantee of being read is a human or a nightly running it. `SCHEMA-DRIFT-NIGHTLY` is
   explicit that the standing-timer question is already open with the owner as empyrean `d-9` and that
   **one cross-lane question gets one card** — so this work deliberately files no second card. If that
   nightly never lands, this section is a tool you have to remember, and that is a known, accepted cost
   rather than an oversight.
5. **If the dimensions chosen are not the ones that matter, the manifest is decoration.** They were
   derived from what consuming code actually reads — `spawn::ARCHETYPE_PREFIX` and `SUBTYPE_PREFIX`
   reaching `with_prefix`/`equates_with_prefix` at `engine.rs:6702`/`6813`, `objreq.rs`'s
   `LEVEL_*_SYMBOL`, the `is_intact` equate trailer, the phase parse — not from a list I was handed. The
   manifest is deliberately cheap to extend: one row, and the positive control forces a matching case.

## 6. Where the honest answer is smaller than the row implies

The row asks for "a check that notices when the two have diverged". **The hermetic gate cannot notice
that, ever, and no hermetic check can** — divergence is a two-sided question and one side is another
lane's. What the gate does is strictly weaker and still worth having: it pins our side of the comparison
so that the *reporter's* answer is meaningful, and it makes our side's blind spots countable. The
noticing itself is the reporter's, and the reporter is non-gating by ruling. Anyone reading this expecting
`cargo test` to go red when aeon publishes a new namespace should stop expecting that: it is not an
omission, it is the ratified shape.

## 7. Open, and why

* **`equate_rows` and `phase_count` have no tracked upstream origin** (`upstream = -`), so the object
  store cannot answer them and only the live column can. A phase table comes from `phase` directives
  spread across many `engine/**` files and an equate trailer is a whole-build total; neither reduces to
  one grep. Left open deliberately rather than faked with a wide path.
* **No pin move.** Everything here is instrumentation. Moving `fixtures/aeon/` to a build carrying these
  dimensions is a separate, deliberate decision under `PROVENANCE.md`'s "Moving the pin", and it would
  want the spring-picker and `Level_*` coverage rewritten against real bytes in the same change — which
  is the actual prize behind this row and is not claimed here.
