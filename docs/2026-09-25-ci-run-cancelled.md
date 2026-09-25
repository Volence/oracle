# F-CI-RUN-CANCELLED-UNEXPLAINED: why run 35451829729 was cancelled

**Date:** 2026-09-25 · **Branch:** `parcel/ci-run-cancelled` · **Base:** `d1cdc48`
**Kind:** investigation. Nothing in CI or tooling was changed. Every fact below comes from the GitHub
API, from job logs fetched here, or from the tree. None is copied from the booking in `docs/OVERSEER.md`.

---

## The answer in plain words

**The job was stopped by a cancel request. It did not time out, its runner was not lost, and its code
was not broken.** The request came from outside the job. GitHub wrote its generic "someone asked this to
stop" message, and the runner then shut the job down cleanly. **Who sent the request cannot be found
from here.** The API does not record who cancels a run. No script, workflow or agent session on this box
issued one. That leaves two causes: the owner cancelling it in the web UI, or GitHub cancelling it on
its own side. The owner's GitHub security log is the one place that might tell them apart, and only he
can open it.

**Could this hide a red?** Not by itself. GitHub reports the run as `cancelled`, never as `success`.
A red could only be hidden by a reader that treats "completed" as "passed". No committed tool in this
repo reads CI verdicts at all (see §4). The one reader is the overseer's hand-written wait loop, and on
2026-09-19 it handled the cancel correctly.

---

## 1. What the run actually did

`gh api repos/Volence/oracle/actions/runs/35451829729`: `event=push`, `head_sha=2414223ef51…`,
`run_attempt=1`, `status=completed`, `conclusion=cancelled`, created `15:26:44Z`, updated `15:47:10Z`.
The `actor` and `triggering_actor` fields are both `Volence`, but they name the **pusher**. The API has
no field that names who cancelled.

`…/runs/35451829729/jobs`, per step:

| Job | Result | Window |
|---|---|---|
| Determinism gate | success | 15:26:46 to 15:28:03 |
| Replay playthroughs (release) | success | 15:28:05 to 15:28:33 |
| Build, test, clippy, fmt | **cancelled** | 15:28:05 to 15:47:09 (19 m 04 s) |

In the cancelled job, every step through *Corpus guards* was `success`. The two cache-miss steps were
`skipped`, because the vendor cache hit. **`Test` ran 15:29:34 to 15:47:07, 17 m 33 s, and ended
`cancelled`.** After that, *Post Run rust-cache* was `skipped`, while *Post Run checkout* and *Complete
job* were `success`.

**Where the test run stood** (from the job log, `gh api --allow-escape-sequences
repos/Volence/oracle/actions/jobs/105920393415/logs`, 3038 lines after ANSI stripping):
- 70 `test result:` blocks, all `ok`. `grep -c -E '\.\.\. FAILED|test result: FAILED'` returned 0.
- `singlestep_m68000` started at 15:35:23 (113 tests). 112 had printed `ok` by 15:38:33
  (`tst_anchors_match_singlesteptests`).
- Then 8 m 34 s passed with no output until `15:47:07.048Z ##[error]The operation was canceled.`
- The runner then ran post-job cleanup and printed `Terminate orphan process: pid (3861) (cargo)` and
  `… (singlestep_m68000-6cc033f171e2ee7f)`.

**The silence is normal.** In the two green runs pushed alongside it, the one test that was still running
(`add_sub_match_singlesteptests`) was the last of the 113 to finish, and it ran silently for a long time
after `tst_anchors` printed:

| Run | `tst_anchors … ok` | `add_sub … ok` | Silent tail | Binary total |
|---|---|---|---|---|
| 7d56eb6 (job 105920465234) | 15:37:14 | 15:52:23 | 15 m 09 s | 1060.51 s |
| 17e5c6b (job 105920482341) | 15:38:57 | 16:02:30 | 23 m 33 s | 1604.87 s |
| **2414223 (cancelled)** | 15:38:33 | *(cut at 15:47:07)* | 8 m 34 s so far | n/a |

So the cancel landed partway through the longest single test in the suite. That test was on course to
pass: `7d56eb6` carries `crates/` byte-identical to `2414223`, and it passed. **What the cancel left
unrun**, counted from the green `7d56eb6` log: the rest of `add_sub_match_singlesteptests`, then 27 more
result blocks (`singlestep_z80` onward, including 4 doc-test groups) holding 1,139 tests. Those 1,139
never ran on `2414223`, and they were proven only on the byte-identical `7d56eb6`.

## 2. Candidate causes, each tested

| Candidate | Verdict | Evidence |
|---|---|---|
| Job `timeout-minutes` | **Ruled out** | `ci.yml` sets no `timeout-minutes` anywhere (grep: 0 keys; the only hit is the comment at `2414223:.github/workflows/ci.yml:170` saying so). GitHub's default is 360 min, and the job ran 19 min. **Positive control, in this repo:** the one real timeout on record (nightly run 32227172318, job 95989136976, 2026-08-19, ran 6 h 01 m) carries the annotation **`The job has exceeded the maximum execution time of 6h0m0s`** *and* `The operation was canceled.` The job in question carries **only** the second. |
| Runner shutdown / lost communication | **Ruled out** | `grep -c` in the cancelled log: `shutdown signal` 0, `lost communication` 0. The runner was alive to run post-steps and kill the orphan `cargo`, which a lost runner cannot do. |
| `concurrency` group (any workflow) | **Ruled out** | `grep -n -i concurrency .github/workflows/*.yml` returns 0 lines across both workflows (`ci.yml`, `nightly-differential.yml`), and neither calls a reusable workflow (`uses:` only on `actions/*`, `dtolnay/*`, `Swatinem/*`). Empirically too: at 15:26:46Z and 15:27:11Z, `17e5c6b` and `7d56eb6` were pushed to the same ref, overlapped this run, and both finished `success`. Nothing superseded anything. |
| GitHub cancelling on a newer push of the same ref | **Ruled out** | GitHub does that only inside a `concurrency` group with `cancel-in-progress` (see the row above). Of 220 CI runs 2026-09-12..25, almost all overlapped a later push on `main`, and this is the only one cancelled. |
| A code defect (hang) | **Ruled out** | See §1: the silent tail matches the green runs, and byte-identical `crates/` went green in `7d56eb6`. |
| Our tooling or an agent issued the cancel | **Not found** | No committed code in `oracle`, `dominion` or `empyrean` calls a cancel endpoint. No Claude transcript or shell history has a cancel command (details in the detectors block). The overseer's session at 15:27-15:48Z was blocked on a background waiter (`byee1zymz`) that only *polls* `gh run list` and *prints* `gh run view --json conclusion,jobs`. |
| Manual cancel in the web UI, or GitHub-side | **Cannot be told apart from here** | `gh api orgs/Volence/audit-log` returns 404, because this is a user-owned repo, not an org. `repos/Volence/oracle/events` holds only `PushEvent`s (100 of 100, oldest 2026-09-18T19:31:48Z, so the window covers the cancel), and the Events API does not carry workflow cancels. The run's `/timing` shows `run_duration_ms=1226000` and nothing else. |

**A distribution, not one sample** (API, every CI run created 2026-09-12..2026-09-25: 234 runs, 220 of
them `CI`):

| *Build, test, clippy, fmt* conclusion | n | `Test` step min / median / p90 / max | job max |
|---|---|---|---|
| success | 211 | 13.3 / 36.2 / 42.0 / 55.4 min | 56.8 min |
| failure | 8 | n/a | 37.6 min |
| cancelled | **1 (this one)** | 17.6 min | 19.1 min |

The cancelled run stopped at **less than half the median**, so it did not run long. It is the only
cancel in two weeks. In the repo's whole history (`runs?status=cancelled`: `total_count=4`), the other
three were the 6 h nightly timeout above and two 08-26 runs that were cancelled with **zero jobs**, a
different shape from this one.

## 3. How close is the job to a timeout?

It is not close to any timeout, because the job sets none. The default is 360 min, and the slowest green
job in two weeks took 56.8 min, about 16 % of it. **The comment at `ci.yml:170` is now stale.** It says
*"Set one once the first green run says what the number is,"* and 211 green runs have since said it.
**Proposed, not made:** `timeout-minutes: 90` on `build-test-lint`. That is about 1.6× the observed
max, and it would stop a real hang at 90 min instead of 6 h. It is **not** a fix for this row: a timeout
adds a way for a job to be cancelled and removes none. Setting it is a CI-policy call for the overseer.
If made, it would show up in G0 (`tools/ci-parity.py`) as an edited step body.

## 4. Can our tooling read `cancelled` as green?

**No committed tool reads a CI verdict at all.** `git grep -n -I -E "gh run|gh api|conclusion|actions/runs"
-- tools/ .github/` found only docstrings and comments, in `tools/detector-check.py`,
`tools/test_detector_check.py`, `tools/replay_playthroughs.sh` and `tools/aeon_pin_report.py`.
`tools/land.sh` is a local gate and never queries GitHub (`git grep -c "gh run" -- tools/land.sh`
returned 0; control: `… -- tools/detector-check.py` returned 3).

The verdict is read by hand, by the overseer, with a pattern like the one used on 2026-09-19:

```
until [ "$(gh run list --limit 60 --json headSha,status --jq '[.[]|select(.headSha|startswith("2414223"))|.status]|first')" = "completed" ]; do sleep 90; done
gh run view $RID --json conclusion,jobs ...
```

The loop waits on `status`, then **prints** `conclusion`. So a cancelled run reaches a human as
`conclusion=cancelled`, and on 09-19 the human caught it. **Two real hazards remain in the pattern.**
Neither one is the `cancelled`-reads-green hazard:

1. **"Completed" and "passed" are one careless word apart.** Nothing mechanical separates them, so a
   reader who skims for "completed" gets it wrong. This is the risk the row exists for, and it is still
   guarded only by care.
2. **The brief's "filter silence" is a short-SHA exact match, now explained.** `gh run list --commit
   2414223` returns **0**, and `gh run list --commit 2414223ef5104a3c08859e80e7312f851a68b552` returns
   **1**. The REST filter behaves the same way (`runs?head_sha=2414223` gives `total_count=0`; the full
   SHA gives 1). The filter wants the full 40-character SHA and **silently answers 0 to a short one**.
   The `--limit 60 … startswith` workaround brings back the window failure instead: once more than 60
   runs land after the SHA, it reads `null`, never `completed`, and the loop spins forever.

**Proposed, not made:** a committed `tools/ci-verdict.sh <sha>`. It would resolve the SHA to its full
form with `git rev-parse`, query `runs?head_sha=<full>` (no window), and exit **0 only if every run for
that SHA has `conclusion == success`**. It would exit non-zero, *naming* the conclusion, on `cancelled`,
`failure`, `timed_out`, `skipped`, `action_required` or `neutral`. It would exit loudly on zero runs
found, with the full-SHA query shown to fire on a known SHA in the same invocation. That turns hazard 1
into a mechanical rule and removes hazard 2. It is not made here: it is new tooling, not a fix to
anything ours that caused this row, and it needs its own red-first proof (bar 7).

## 5. What would make the next one determinable

- **The canceller:** the owner can look at github.com/settings/security-log, filtered to the minutes
  around the event. *Unverified:* this investigation could not open that page, so it has not confirmed
  that a personal account's log records workflow cancels. No `gh api` endpoint reaches it for a
  user-owned repo (the org audit-log endpoint returns 404).
- **Faster triage:** `tools/ci-verdict.sh` above would reject a `cancelled` landing at once and name it,
  so the substitute-SHA check (`7d56eb6` here) would start without anyone first noticing a missing green.
- A step that prints elapsed time is **not** needed. The API's per-step timestamps plus libtest's
  `has been running for over 60 seconds` lines were enough to place the cancel to the second.

---

## Method declarations

```detectors
ABSENCE: the cancelled job's annotations carry no timeout message (so the cancel was not a timeout)
  instrument: gh api repos/Volence/oracle/check-runs/105920393415/annotations  | grep -c "exceeded the maximum execution time" -> 0
  positive:   gh api repos/Volence/oracle/check-runs/95989136976/annotations  | grep -c "exceeded the maximum execution time" -> 1
  negative:   gh api repos/Volence/oracle/check-runs/105920393373/annotations  | grep -c "exceeded the maximum execution time" -> 0
  scope:      all annotations on the one cancelled job (3 returned: node-20 warning, "The operation was canceled.", ubuntu-26 notice)
  contains:   the subject IS the enumerated job; the positive is this repo's only recorded timeout (nightly run 32227172318, 6 h 01 m)

ABSENCE: no Claude session or shell on this box issued a run cancel
  instrument: grep -l -r -E "gh run cancel|runs/[0-9]+/cancel|force-cancel" --include=*.jsonl ~/.claude/projects  (plus the same pattern over ~/.zsh_history -> 0)
  positive:   grep -c -E "gh run cancel|runs/[0-9]+/cancel|force-cancel" ~/.claude/projects/-home-volence-sonic-hacks-oracle/2225e2bd-78c4-43ef-b6e5-fcde06dcea9d/subagents/agent-a01ad01a2b4b12bcf.jsonl -> 7
  negative:   grep -c -E "gh run cancel|runs/[0-9]+/cancel|force-cancel" ~/.claude/projects/-home-volence-sonic-hacks-oracle/0ce9719e-9e30-4a14-8dfb-4dd864947709.jsonl -> 0
  scope:      all 5391 *.jsonl transcripts under ~/.claude/projects, including subagents; 1 file matched, and it is this investigation's own transcript (the positive). Boundary: this box only; a cancel from the GitHub web UI or another machine is outside it
  contains:   the overseer session that owned the landing (0ce9719e) has 21 lines timestamped 2026-09-19T15:4x and its tool calls 15:26-15:49Z are enumerated in §2; it is the negative above

ABSENCE: no committed tool in this repo reads a CI conclusion, so none can read `cancelled` as green
  instrument: git grep -n -I -E "gh run|gh api|conclusion|actions/runs" -- tools/ .github/  (every hit a docstring or comment)
  positive:   git grep -c "gh run" -- tools/detector-check.py -> 3
  negative:   git grep -c "gh run" -- tools/land.sh -> 0
  scope:      git ls-files tools .github = 60 tracked files at d1cdc48
  contains:   tools/land.sh, the landing gate the brief names, is inside tools/ and is the negative

ABSENCE: no other CI run in the two-week window was cancelled
  instrument: gh api "repos/Volence/oracle/actions/runs?status=cancelled&per_page=100" -q .total_count -> 4 (one in window: this run)
  positive:   gh api "repos/Volence/oracle/actions/runs?status=success&per_page=1" -q .total_count -> 434
  negative:   gh api "repos/Volence/oracle/actions/runs?status=cancelled&created=2026-09-20..2026-09-25" -q .total_count -> 0
  scope:      the repo's whole run history (998 runs total); the 2026-09-12..25 distribution was fetched separately, page by page, 234 runs
  contains:   this run (created 2026-09-19T15:26:44Z) appears in the status=cancelled listing by id

HEURISTIC: an unset `timeout-minutes` means GitHub's 360-minute default applies, so a 19-minute job cannot have timed out
  from:    GitHub Actions' documented default job timeout, and the workflow's own comment asserting it
  assumes: no job-level or step-level timeout-minutes exists at the landing SHA, and GitHub's default was not changed for this repo
  checked: 2414223:.github/workflows/ci.yml:170 "No `timeout-minutes` is set here deliberately"

CANNOT-TEST: who issued the cancel (owner in the web UI vs GitHub-side) cannot be determined from this box
  attempted: gh api orgs/Volence/audit-log -> 404; gh api repos/Volence/oracle/events -> 100 PushEvent only; gh api .../runs/35451829729 and /timing -> no canceller field
  cost:      not measured — the remaining instrument is the owner's own security-log page, which needs his browser session and his eyes
  prior:     docs/OVERSEER.md l.779-782, the booking, which recorded the cause as unexplained ("the log gives no reason"); no earlier claim about the canceller's identity (git grep -i "security.log|audit.log|cancelled_by|who cancel" -- docs/ -> 0)
```

The date filter in the fourth declaration was itself checked to fire: the same query with
`created=2026-09-19..2026-09-19` returns 1, which is this run.

---

## Where the brief was wrong

1. **"Appended note on the register row in `docs/OVERSEER-REFERENCE.md`": there is no such row.**
   `git grep -n "F-CI-RUN-CANCELLED" docs/` finds it in `docs/OVERSEER.md:786` (the booking) and in
   `docs/lane-status.json:35` (the queue row). It is not in `OVERSEER-REFERENCE.md`, whose follow-up
   register (line 1475 on) was checked. Both files that hold it are the overseer's, so this item was
   **not carried out.** The overseer should point the row at this doc.
2. **"`docs/OVERSEER-REFERENCE.md` around F-METHOD-ROWS-FROM-THE-REDS": that id is not in that file.**
   It is in `docs/OVERSEER.md` and `docs/lane-log.jsonl`. The detector convention itself sits at
   `OVERSEER-REFERENCE.md` ~l.90-105 and in the `tools/detector-check.py` docstring, and that is what
   this doc follows.
3. **"A ~19-minute run is suspiciously close to a round number."** The job ran 19 m 04 s and the `Test`
   step 17 m 33 s. Neither is a configured bound, and the annotation rules out a timeout in any case (§2).
4. **"`--commit 2414223` returned NOTHING … a filter's silence is not an absence."** That is true, but
   the silence is fully explained: the filter is an exact match on the full 40-character SHA (§4).
5. **"Check `tools/land.sh` / whatever reads CI for landings."** Nothing committed reads CI (§4). The
   reader is an ad-hoc loop in the overseer's session.
