# The restamp A/B — what aeon's clamps actually moved

**Run 2026-08-30 by the oracle lane, for aeon's prove-then-restamp ruling (their d-14).**
Instrument: `replay_runner --restamp` (dry run; nothing was written anywhere).

## The two sides, both committed blobs

| side | ROM | sha256 | source |
|---|---|---|---|
| **new** | chain 188, `aeon_rev ec6a4791` | `951cf960…62707d` | sigil `e38295d2`, golden blob |
| **baseline** | chain 186, `aeon_rev def98ee5` | `75e9f4d4…19fcf7a` | our own `fixtures/aeon/s4.debug.bin` |

Listings bind to their images (the runner refuses one that does not). The chain-188 listing came from
aeon's tree with the **consistency joint re-verified at capture**: their on-disk ROM hashed byte-identical
to the golden blob at that moment.

## ⚑ THE HEADLINE: the clamps moved EXACTLY ONE checkpoint, and it is checkpoint 0

    stale on chain 188 :  {0, 18, 19, 20, 21, 22, 23, 24, 25, 26}   — 10 of 27
    stale on chain 186 :  {   18, 19, 20, 21, 22, 23, 24, 25, 26}   —  9 of 27
    ---------------------------------------------------------------------------
    MOVED BY THE CLAMPS:  {0}                                       —  1 of 27

| idx | ring | logic_tick | old | new |
|---|---|---|---|---|
| **0** | 0 | **2** | `1D375066` | `0D375066` |

**Checkpoint 0 is taken at `Logic_Tick` 2 — the earliest checkpoint in the stream**, deep inside the
window where `cam_col < 16`. **aeon's mechanism is SUPPORTED**, and supported on the detail it did not
choose: the act opens at `cam_col` 6, and the one checkpoint the clamps moved is the first one.

The nine deep checkpoints (18–26) are stale on **both** ROMs, with **byte-identical `old` and `new`
values on each side** — the same nine payloads, the same nine replacements. They are not sensitive to the
clamps.

## ⚠ WHY THE CONTROL DECIDED THIS, AND WHY THE RAW ANSWER WOULD HAVE BEEN A REFUTATION

**The raw chain-188 stale set contains nine movers deep into the run — at ticks 1154 through 1666, long
past the camera's column-16 crossing.** Reported on its own, that set meets aeon's own stated falsifier
word for word: *"if checkpoints deep into the run also moved, my mechanism is incomplete and the restamp
must not proceed on it."*

**It would have been the wrong verdict.** Those nine did not move; they were **already stale before the
clamps existed**. Only the differential separates *"this checkpoint disagrees with the fixture"* from
*"this checkpoint was changed by your parcel"*, and the fixture cannot tell you which — it records one
value and has no memory of why.

**Had the restamp proceeded on the raw set, ten checkpoints would have been re-stamped where one
legitimately moved** — and nine payloads whose staleness has an unexplained, older cause would have been
overwritten with fresh values, which is exactly the outcome the address-free fold exists to prevent: a
restamp that restores green and destroys the only claim the net makes.

## ⚑ THE SECOND FINDING, WHICH IS AEON'S AND IS OLDER: THE NET IS SILENTLY BROKEN AT 9 OF 27

**Chain 186 does not run the fixture green.** A plain (unstubbed) run against our frozen baseline exits
**2** and desyncs at `Logic_Tick` 1154, checkpoint 18.

**Nobody knew, and the reason is structural:** the two full-playthrough rows
(`the_standing_fixture_runs_green`, `the_slide_fixture_runs_green`) are **`#[ignore]`d** — ~34 s and ~49 s
of unoptimized emulation that *"does not belong in `cargo test --workspace`"*. Everything that runs by
default either **parses** the stream, **walks** it statically, or drives the **negative control** — and
none of those plays it through. So a stream that desyncs two thirds of the way in reads as a green suite
on both sides of the fence.

⚠ **This corrects a claim in this repo, made by me tonight, in the commit that merged the freeze:**
*"chain 186 is the last freeze whose embedded fixture is coherent — 13 passed / 0 failed."* **The figure
was real and the conclusion did not follow.** 13/0 was the *default* suite, which cannot see this. **The
pin itself is still right** — 186 is the last freeze whose checkpoint **0** is coherent, it is strictly
less broken than 188, and the decoupling the freeze bought is unaffected — **but it was pinned for a
reason that was never tested.** Same shape as everything else tonight: a true number answering a narrower
question than the one asked of it.

## What is NOT established

- **Why 18–26 are stale.** Unexplained, older than chain 186, and not measured further here. It is
  aeon's fixture and aeon's call whether it predates the clamps by one parcel or by many.
- **Whether `OjzSlide` (37 checkpoints) behaves the same.** Not run; `Ojz` is what the ruling needs.
- Nothing was written: `--restamp` without `--out` is a pure dry run, and this repo was reported by the
  runner's own guard as protected.
