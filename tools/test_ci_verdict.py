#!/usr/bin/env python3
"""Offline red-first proof for `tools/ci-verdict.py`, over RECORDED GitHub API responses.

WHY RECORDED, AND WHY THESE
===========================

`ci-verdict.py`'s value is entirely in the branches where it says NO, and the live tool can only be
shown those branches when GitHub happens to hold a run in that state. So its answers on real history
were recorded once, BY THE TOOL ITSELF (`--record`, which saves exactly the trimmed objects the
judgement reads), into `tools/ci-verdict-fixtures/`:

  2414223-cancelled        run 35451829729, the cancel of 2026-09-19 (docs/2026-09-25-ci-run-cancelled.md)
  7d56eb6-green            run 35451855080, the byte-identical-crates/ sibling that went green
  ddf4b1e-no-run           a docs-only commit inside a two-commit push; no run of its own. Holds the
                           positive-control responses and the tip search (ref pinned at 420986b)
  23599fa-schedule-runs    a green push run plus five nightly `schedule` runs on the same SHA
  abbd873-before-ci        the parent of 28ef647, the commit that added ci.yml: nothing is expected,
                           and "nothing expected, nothing ran" must still not read as GREEN

The fixtures are the API's words, not ours: no JSON in them was hand-written. Every MUTATION below
departs from one of them (deep-copied) and changes one field, so each red is one field away from a
real green.

The git side (workflows at the SHA, rev-parse, the tip walk and its `git diff -- crates/`) runs
against this repo's real history, the same call made by `tools/test_detector_check.py`. An
unreachable revision (a shallow clone) makes the case fail as UNMEASURABLE rather than skip.

WHAT EACH CASE PINS
===================

REAL HISTORY  2414223 -> RED naming CANCELLED and the run id · 7d56eb6 -> GREEN · ddf4b1e -> NO-RUN
              with the positive control fired and the tip (420986b) named with crates/ IDENTICAL ·
              23599fa -> GREEN with the schedule runs named as ignored.
CONTROL ARM   the hand loop this tool replaces keyed on `status == "completed"`; on these same
              recorded bytes it cannot tell 2414223 from 7d56eb6. Shown, so the red above is the
              tool's doing and not the data's.
MUTATIONS     every non-success conclusion GitHub documents -> RED naming it; queued / in_progress /
              waiting -> PENDING; the push run relabelled `pull_request` -> NO-RUN; a job skipped or
              missing under a `success` run -> RED; red + pending together -> RED (precedence);
              the positive control's query returning nothing -> UNMEASURABLE, never NO-RUN; an
              unrecorded API path -> UNMEASURABLE, never "no runs".
CLI           exit statuses through `main()` for the three real SHAs given SHORT, and an unresolvable
              SHA -> 4 without any API call.

Usage:  tools/run_ci_verdict_tests.sh   (the runner; also land.sh G0c)  /  tools/test_ci_verdict.py
"""

import copy
import importlib.util
import io
import json
import os
import sys
from contextlib import redirect_stdout

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIX = os.path.join(ROOT, "tools", "ci-verdict-fixtures")

spec = importlib.util.spec_from_file_location("ci_verdict", os.path.join(ROOT, "tools", "ci-verdict.py"))
cv = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cv)

SHA_CANCEL = "2414223ef5104a3c08859e80e7312f851a68b552"
SHA_GREEN = "7d56eb6742fbf495899ae36f4a183edf758939e4"
SHA_NORUN = "ddf4b1e43a9f8df0eda8591f5b1dd8d4ae159926"
SHA_TIP = "420986b50cc3349a0f4e23409c036f793f7dc0f5"
SHA_SCHED = "23599fabe1c293c283399df4a3a64e96c9d90cd3"
SHA_PRE_CI = "abbd87366a9e63d597922d9e2f9f860d791084c6"   # parent of the commit that added ci.yml
SHA_CI_ADDED = "28ef647cbb26000eb73f46c337bf4e9316ff251f"
RUN_GREEN = 35451855080

failures = []


def fixture(d, path):
    with open(os.path.join(FIX, d, cv.record_name(path)), encoding="utf-8") as fh:
        return json.load(fh)


def runs_path(sha):
    return f"actions/runs?head_sha={sha}&per_page=100&page=1"


def jobs_path(rid):
    return f"actions/runs/{rid}/jobs?filter=latest&per_page=100"


CONTROL_PATH = "actions/workflows/ci.yml/runs?event=push&per_page=1"
CONTROL = fixture("ddf4b1e-no-run", CONTROL_PATH)
CONTROL_SHA = CONTROL["workflow_runs"][0]["head_sha"]
CONTROL_OVERRIDES = {
    CONTROL_PATH: CONTROL,
    runs_path(CONTROL_SHA): fixture("ddf4b1e-no-run", runs_path(CONTROL_SHA)),
}


def check(name, cond, detail):
    status = "ok  " if cond else "FAIL"
    print(f"  [{status}] {name}")
    if not cond:
        print("         " + detail.replace("\n", "\n         "))
        failures.append(name)


def run(d, sha, ref=None, overrides=None):
    gh = cv.Replay(os.path.join(FIX, d), overrides)
    code, lines, final = cv.verdict(gh, sha, ref or sha)
    return code, "\n".join(lines + [final]), final


def expect(name, got, want_code, *needles):
    code, text, final = got
    ok = code == want_code and all(n in text for n in needles) and final.startswith(
        f"ci-verdict: {cv.WORD[want_code]} ")
    check(name, ok, f"want {cv.WORD[want_code]} containing {needles!r}; got {cv.WORD.get(code, code)}:\n{text}")


def green_with(mutate_runs=None, mutate_jobs=None, extra=None):
    """The recorded 7d56eb6 green, one field changed. Unmutated, it must be GREEN (the green arm)."""
    runs = copy.deepcopy(fixture("7d56eb6-green", runs_path(SHA_GREEN)))
    jobs = copy.deepcopy(fixture("7d56eb6-green", jobs_path(RUN_GREEN)))
    if mutate_runs:
        mutate_runs(runs["workflow_runs"])
        runs["total_count"] = len(runs["workflow_runs"])
    if mutate_jobs:
        mutate_jobs(jobs["jobs"])
    ov = {runs_path(SHA_GREEN): runs, jobs_path(RUN_GREEN): jobs, **CONTROL_OVERRIDES}
    ov.update(extra or {})
    return run("7d56eb6-green", SHA_GREEN, overrides=ov)


def main():
    print("=== real history (recorded API responses; git from this repo) ===")
    expect("2414223 (the 2026-09-19 cancel) is RED and names CANCELLED + run id",
           run("2414223-cancelled", SHA_CANCEL), cv.RED, "CANCELLED", "35451829729",
           "Build, test, clippy, fmt=cancelled")
    expect("7d56eb6 (byte-identical crates/, green) is GREEN with all three jobs",
           run("7d56eb6-green", SHA_GREEN), cv.GREEN, "Determinism gate=success",
           "Replay playthroughs (release)=success", "Build, test, clippy, fmt=success")
    expect("ddf4b1e (docs-only, inside a push) is NO-RUN, control fired, tip named, crates/ identical",
           run("ddf4b1e-no-run", SHA_NORUN, ref=SHA_TIP), cv.NO_RUN, "positive control:",
           "The query fires", SHA_TIP, "crates/ is IDENTICAL", "TIP only")
    expect("23599fa: five schedule runs are named and ignored; the push run decides (GREEN)",
           run("23599fa-schedule-runs", SHA_SCHED), cv.GREEN, "event=schedule", "35453569843")
    expect("abbd873 (before ci.yml existed): NO-RUN, never GREEN, control borrowed from HEAD",
           run("abbd873-before-ci", SHA_PRE_CI, ref=SHA_CI_ADDED), cv.NO_RUN,
           "expected push workflows at this SHA: none", "positive control:")

    print("=== control arm: the hand loop's predicate on the same bytes ===")
    old = {s: [r["status"] for r in fixture(d, runs_path(s))["workflow_runs"]]
           for d, s in (("2414223-cancelled", SHA_CANCEL), ("7d56eb6-green", SHA_GREEN))}
    check("status=='completed' reads the cancel and the green the same (the defect this tool closes)",
          old[SHA_CANCEL] == old[SHA_GREEN] == ["completed"], f"got {old}")

    print("=== green arm for the mutations below (unmutated copy) ===")
    expect("unmutated 7d56eb6 copy is GREEN", green_with(), cv.GREEN)

    print("=== mutation: every non-success conclusion is RED and named ===")
    for concl in ("failure", "cancelled", "timed_out", "skipped", "action_required", "neutral",
                  "stale", "startup_failure", None):
        def m(rs, c=concl):
            rs[0]["conclusion"] = c
        expect(f"conclusion={concl} -> RED", green_with(m), cv.RED, str(concl).upper(), str(RUN_GREEN))

    print("=== mutation: not yet completed is PENDING, never GREEN or RED ===")
    for st in ("queued", "in_progress", "waiting", "requested", "pending"):
        def m(rs, s=st):
            rs[0]["status"], rs[0]["conclusion"] = s, None
        expect(f"status={st} -> PENDING", green_with(m), cv.PENDING, st)

    print("=== mutation: only push runs count ===")
    def to_pr(rs):
        rs[0]["event"] = "pull_request"
    expect("the push run relabelled pull_request -> NO-RUN (control still fired)", green_with(to_pr),
           cv.NO_RUN, "event=pull_request", "positive control:")
    expect("no runs at all -> NO-RUN", green_with(lambda rs: rs.clear()), cv.NO_RUN, "NO RUN: CI")

    print("=== mutation: a success run whose jobs did not all run is RED ===")
    def skip_btl(js):
        next(j for j in js if j["name"] == "Build, test, clippy, fmt")["conclusion"] = "skipped"
    expect("job skipped under a success run -> RED", green_with(mutate_jobs=skip_btl), cv.RED,
           "'Build, test, clippy, fmt' concluded SKIPPED")
    def drop_btl(js):
        js[:] = [j for j in js if j["name"] != "Build, test, clippy, fmt"]
    expect("declared job missing from a success run -> RED", green_with(mutate_jobs=drop_btl), cv.RED,
           "has NO job in this run")

    print("=== precedence: a red is final even while another run is pending ===")
    def red_plus_pending(rs):
        p = copy.deepcopy(rs[0])
        p["id"], p["status"], p["conclusion"] = 1, "in_progress", None
        rs[0]["conclusion"] = "failure"
        rs.append(p)
    expect("failure + in_progress -> RED", green_with(red_plus_pending,
           extra={jobs_path(1): fixture("7d56eb6-green", jobs_path(RUN_GREEN))}), cv.RED, "FAILURE")

    print("=== an absence is not a finding unless the query fires ===")
    dead = {runs_path(CONTROL_SHA): {"total_count": 0, "workflow_runs": []}}
    expect("positive control returns nothing -> UNMEASURABLE, not NO-RUN",
           run("ddf4b1e-no-run", SHA_NORUN, ref=SHA_TIP, overrides=dead), cv.UNMEASURABLE,
           "POSITIVE CONTROL FAILED")
    empty_dir = os.path.join(FIX, "__does_not_exist__")
    code, text, _ = cv.verdict(cv.Replay(empty_dir), SHA_GREEN, SHA_GREEN)
    check("an unanswered API path is UNMEASURABLE, never 'no runs'", code == cv.UNMEASURABLE, text)

    print("=== CLI: exit statuses, SHORT SHAs resolved before any query ===")
    for short, d, extra, want in (("2414223", "2414223-cancelled", [], cv.RED),
                                  ("7d56eb6", "7d56eb6-green", [], cv.GREEN),
                                  ("ddf4b1e", "ddf4b1e-no-run", ["--ref", SHA_TIP], cv.NO_RUN)):
        buf = io.StringIO()
        with redirect_stdout(buf):
            rc = cv.main([short, "--replay", os.path.join(FIX, d), *extra])
        last = buf.getvalue().strip().splitlines()[-1]
        full = cv.resolve(short)
        check(f"main({short}) exits {want} ({cv.WORD[want]}) and prints the full SHA",
              rc == want and f" {full} " in last, f"rc={rc}; last line: {last}")
    buf = io.StringIO()
    with redirect_stdout(buf):
        rc = cv.main(["0000000000000000000000000000000000000bad", "--replay", empty_dir])
    check("an unresolvable SHA exits 4 (UNMEASURABLE) before any API call", rc == cv.UNMEASURABLE,
          buf.getvalue())

    print()
    if failures:
        print(f"test_ci_verdict: FAIL — {len(failures)} case(s): " + "; ".join(failures))
        return 1
    print("test_ci_verdict: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
