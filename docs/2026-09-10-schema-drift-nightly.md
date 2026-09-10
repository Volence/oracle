# SCHEMA-DRIFT-NIGHTLY — a contract-currency reporter, and the backtest that entitled it to ship

**Date:** 2026-09-10 · **Branch:** `parcel/schema-drift-nightly` · **Status:** shipped, non-gating

---

## The gap this fills, in one paragraph

`crates/oracle-aether/tests/schema_conformance.rs` proves the two vendored artifacts hash to the blobs
`contract/PROVENANCE.md` pins. That is **self-consistency** — a fact about this repository alone. It is
hermetic on purpose and it can never notice the contract repo moving on. `F-SCHEMA-READS-LIVE-EMPYREAN`
(2026-09-02) removed the walk into empyrean's working tree, correctly: the old shape went red when the
hub saved mid-edit and — the half that matters — **would have gone green against a change no other lane
could see**. `PROVENANCE.md` recorded the cost in its own words: *"a default local run no longer notices
upstream moving on its own."* This parcel is that notice, out of band.

---

## What shipped

| file | what it is |
|---|---|
| `tools/contract_drift_report.py` | the reporter (and its `--backtest` harness). **Always exits 0.** |
| `tools/test_contract_drift_report.py` | 20 rows, all hermetic (synthetic git repos in tempdirs) |
| `tools/run_contract_drift_tests.sh` | the named runner for those rows |
| `tools/replay_playthroughs.sh` | one non-gating call added, beside the existing `aeon_pin_report.py` one |
| `crates/oracle-aether/tests/contract/PROVENANCE.md` | the sidecar now names its currency owner |

### The question it asks, and where

*Is what we vendored still what the peer publishes?* — asked at the peer's **`origin/main` tip**, never
at `pin.revision`. Both vendored paths are resolved with `git rev-parse <rev>:<path>` in the contract
repo's **object store**; its working tree is never opened. The checkout is located by
`empyrean/contract/SUITE_PATHS.md`'s precedence at `38f6df4` — `--empyrean`, `$EMPYREAN_DIR`,
`$EMPYREAN_SUITE_ROOT/empyrean`, a marker walk, then a refusal naming every candidate tried — and the
step that answered prints before anything is compared. There is no home literal and no fallback to a
live tree.

Three verdicts, not two: `SAME`, `DRIFTED`, `UNMEASURABLE`. The third is never rendered as either of the
others, as zero, or as green.

---

## THE BACKTEST — the deliverable

This lane refused a sibling row (`F-CITATION-LINT`) on the morning of 2026-09-10 after measuring its
catch rate against the real population at **zero**: it had been designed from the single finding that
prompted it rather than from the population it had to cover, and it looked entirely reasonable until
someone measured it. This is the same species of check, so it got the same test **before** it shipped.

`--backtest` replays the **shipped `classify()`** — not a re-implementation of it — over the contract
repo's real history of both vendored paths.

### Primary: the first-parent line of `origin/main`, which is what a nightly actually experiences

| path | revisions | window | content-change events | fired | missed | catch rate |
|---|---|---|---|---|---|---|
| `contract/schema/bus-protocol.schema.json` | 57 | 2026-06-18 .. 2026-09-09 | 56 | **56** | **0** | **100.0%** |
| `contract/schema/tests/vectors.json` | 23 | 2026-08-22 .. 2026-09-06 | 22 | **22** | **0** | **100.0%** |
| **total** | **80** | | **78** | **78** | **0** | **100.0%** |

### Secondary: `--full-history`, side branches and merges included

97 content-change events, **97 fired, 0 missed**, over 100 commits walked (73 schema + 27 vectors).

### False positives

**0**, over **0** touch-no-change commits. Neither path has a single commit in its entire history that
touched it without moving its blob — no merge re-resolved either path to a blob equal to its first
parent's, and there are no mode-only changes. That class exists in principle, has zero instances here,
and is therefore covered by a constructed fixture rather than claimed as measured.

### The vacuity control

The same comparison asked at `pin.revision` instead of at tip — i.e. a drift check pointed at the
revision the pin was taken from — fires on **0 of 97** events. A pinned blob equals itself forever. This
is the number that says the 100% above is not an artifact of the method.

### What the fires would have said

| | schema | vectors |
|---|---|---|
| changed the parsed document | 71 | 25 |
| changed **bytes only** (parsed documents identical) | **1** | 0 |

The one is empyrean `47e77ec`, *"schema: restore raw UTF-8 (content-identical), and correct 11.45's
owes-nothing claim"*. Our pin is on **bytes**, so this is real drift and suppressing it would be a miss;
it is also not a shape change, so the report labels it *semantically null* rather than sending a reader
hunting for something that is not there.

### What the backtest does NOT cover, stated rather than implied

`git log --full-history -M --diff-filter=R` over `contract/schema/` returns **empty** across the entire
history: **the rename/delete class has zero upstream instances.** A population that lacks a class cannot
measure a detector on that class, so 100% is a statement about the classes that occurred. The
rename/delete branch — and the empty-population, missing-ref, no-peer, failed-fetch and missing-pin
branches — are covered by constructed rows in `tools/test_contract_drift_report.py` instead.

### Cost

Backtest **0.75 s** total; the report itself **0.03 s** (`--no-fetch`) or **0.74 s** with a real network
fetch. Measured at load averages between 6.09 and 13.78 on a machine running several concurrent agents.
A nightly can afford this by roughly four orders of magnitude. **The comparison carries no timeout** —
deliberately: a subprocess that is merely slow under load must never be converted into a false "no
drift". Only the network fetch has one (`--fetch-timeout`, default 120 s), and when it fires it prints
`TIMED OUT` by name and the run is stamped as being against a stale mirror.

---

## The stale-mirror decision

`origin/main` in a local checkout is a **mirror** and can be arbitrarily far behind the real remote. A
report that compared against a week-old mirror and printed "no drift" would be wrong in the one
direction that matters. The choice made:

* **`--fetch` is ON by default** (`--no-fetch` disables it) — a nightly can afford the network, and a
  currency question asked of a stale mirror is not the question.
* A fetch that fails or times out is **named**, and every verdict in that run is stamped as being
  against the local mirror rather than against what the peer publishes.
* The mirror's age prints either way: the tip commit's own date **and** the mirror's last-written time
  (`.git/FETCH_HEAD` mtime). These are different facts, and a stale mirror reports a plausible-looking
  recent commit date while still being stale.
* The in-runner call site below passes `--no-fetch`, because a courtesy report inside another runner
  must not reach the network. The scheduled nightly fetches.

---

## Where it is wired, and what happens on drift

1. **`tools/replay_playthroughs.sh`**, in the existing `REPORT ONLY — does not affect exit status`
   block, immediately after `python3 tools/aeon_pin_report.py || true`. The call is
   `python3 tools/contract_drift_report.py --no-fetch || true`, placed **after** `status` is captured
   and **before** `exit $status`, so it cannot change that script's verdict.
2. **`tools/run_contract_drift_tests.sh`** runs the reporter's own 20 rows. It is **not** in CI and not
   in any blocking path — same placement and same reasoning as `tools/run_doc_split_tests.sh`.

**It is in no blocking path.** It is not called from `tools/land.sh`, from any `.github/workflows/`
file, or from any `cargo test` target. It **always exits 0**, so it cannot turn an existing green red
under any input, including a peer that has drifted, a peer that does not exist, and a `PROVENANCE.md`
with a missing marker.

**On drift it prints** — and prints only: the path, the pinned blob and its revision, the current blob
and the tip it was resolved at, a `bytes N -> M (+D)` line, and a leaf-path delta (added / removed /
changed, capped at 12 names per class) in the same idiom the sidecar's **Current copy** tables use.
Then a standing instruction: a DRIFTED row is the input to a deliberate re-vendor per
`PROVENANCE.md`'s recipe, **never** a reason to edit the vendored bytes — a locally-corrected copy is a
copy whose blob check is worthless.

---

## What scheduling it needs — for the controller, not created here

**No cron job, systemd timer or scheduled workflow was created by this parcel.** Creating a recurring
job is the controller's call. What it would need:

* **Command:** `cd <oracle checkout> && python3 tools/contract_drift_report.py --json`
  (fetching is the default; `--json` gives the scheduler something to alert on).
* **Cadence:** daily, and the peer's own history is the argument. `git log --first-parent --format=%cs`
  over the schema path yields **19 distinct calendar days** across the whole 2026-06-18..2026-09-09
  window — but **12 of those 19 fall in the last 20 days** (2026-08-21 onward), so the recent rate is a
  move roughly every other day. At that rate a weekly cadence would routinely report a delta several
  amendments deep and lose the attribution that makes it actionable. *(This bullet first claimed "57 of
  ~84 days" — 57 is the revision count, not a day count. Measured and corrected before landing.)*
* **Where:** on this machine, next to sigil's existing nightly timers. **Not** a GitHub-hosted runner: a
  hosted runner has no `empyrean` checkout, so every run there would correctly print `UNMEASURABLE`
  forever, and a permanently-unmeasurable nightly is indistinguishable from a broken one.
* **Alert condition:** `counts.drifted > 0` **or** `measurable == false`. The second half is not
  optional — a nightly that alerts only on drift treats "could not measure" as "no drift", which is the
  failure this whole file exists to refuse.
* **Exit code:** always 0, by design. A scheduler must read the JSON, never the exit status.

---

## How it was verified

* `tools/run_contract_drift_tests.sh` — **20 rows, 20 green, 0.23 s.** Predicted 20 before the first
  run; 20 collected.
* `cargo test -p oracle-aether --test schema_conformance` (**debug** profile) — **25 passed, 0 failed,
  0 ignored**, 0.72 s, after the `PROVENANCE.md` edit. Predicted 25 from `grep -c '^#\[test\]'` before
  running; 25 ran.
* **Red-first, four mutations, each quoted back from disk before the run** (an unapplied mutation and a
  correctly restored baseline both print `ok`, so the disk was checked every time):

  | # | mutation | rows red |
  |---|---|---|
  | M1 | `classify` returns `SAME` when the blob could not be resolved | 3 (both rename/delete rows + the unit row) |
  | M2 | `classify` never returns `DRIFTED` | 5 (both drift rows, both backtest rows, the unit row) |
  | M3 | a set-but-wrong `$EMPYREAN_DIR` falls through to the suite root | 1 |
  | M4 | the missing-ref branch prints "no drift found" instead of `UNMEASURABLE` | 1 |

  **M4's first attempt did not apply** (a string-escape mismatch) and the suite printed `OK` — the
  unapplied-mutation collision, caught only because the disk grep for the marker came back empty. It was
  re-applied and went red. Under M2 the clean control stayed **green**, which is the argument for having
  it: a suite of red-only rows is satisfied by a detector that refuses everything.

  Baseline restored from the **committed** tree (`git checkout HEAD -- <file>`) after each, verified by
  `grep -c MUTATION` returning 0 and the suite returning to 20 green.

---

## Two defects the rows found in the tool itself

1. **`read_pins(path=PROVENANCE)`** — a default argument evaluated once at import. Tests that pointed
   the module at a fixture sidecar were silently served the real one, so **seven rows asserted against
   the live repo's pin** while their names claimed a fixture. They failed, which is how it was found;
   had the fixture pin happened to match, they would have passed for the wrong reason indefinitely.
2. **The `DRIFTED` branch read `git.last_error` after both `cat-file` calls**, so a failure on the first
   was overwritten by the success of the second and the report printed, literally, `could not read one
   of the blobs to summarise the delta: None`. A diagnostic that says "could not read: None" has lost
   the only thing it exists to carry.

And one in this document's source: the reporter's own docstring claimed the vacuity control finds
`0 of 39 real drift events` — **39 was a placeholder from the design sketch, written before the
backtest ran**, and it would have read as a measurement. It is 97. Numbers in that file now come from a
run or they do not appear.

---

## Premise of the brief found false

The dispatch asked for doc sync in `PROVENANCE.md` **and/or `docs/DEFERRED_WORK.md`**. There is no
`docs/DEFERRED_WORK.md` in this repo — that file is **aeon's**, cited here only from the outside
(`docs/2026-08-19-aeon-streaming-demand.md:223`, `docs/2026-08-23-prof-straddle-mechanism.md:20`). The
sync landed in `PROVENANCE.md` and in this file.
