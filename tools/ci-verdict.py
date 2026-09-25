#!/usr/bin/env python3
"""Did CI pass on this commit? One SHA in, one verdict out, and the exit status IS the verdict.

WHY THIS EXISTS
===============

Until this file, nothing committed in this repo read a CI verdict (`docs/2026-09-25-ci-run-cancelled.md`
§4). The overseer read it by hand with a loop like:

    until [ "$(gh run list --limit 60 --json headSha,status \\
              --jq '[.[]|select(.headSha|startswith("2414223"))|.status]|first')" = "completed" ]
    do sleep 90; done
    gh run view $RID --json conclusion,jobs

Three defects in that loop, each one measured, and each one closed here:

1. **"Completed" is not "passed".** The loop waits on `status` and then *prints* `conclusion`, so the
   difference between a green and a cancelled run (run 35451829729 on `2414223`, 2026-09-19) was
   carried by a human reading a word. Here it is the exit status: **0 only when every run for the SHA
   concluded `success`**, and every other conclusion is named.
2. **A short SHA silently matches nothing.** `gh run list --commit 2414223` returns 0 runs and the full
   40-character SHA returns 1; the REST `head_sha` filter behaves the same. So the SHA is resolved
   with `git rev-parse` before any query, and the query is by full SHA.
3. **`--limit 60` + `startswith` is a window.** Once 60 runs land past the SHA the loop reads `null`
   and spins forever. The query here is `actions/runs?head_sha=<full>` — no window.

And one hazard that loop never faced but a verdict tool must: **"no run for this SHA" is an absence**,
and an absence is only a finding when the query that reported it is shown to fire. Every no-run
verdict is therefore made alongside a POSITIVE CONTROL in the same invocation: the newest push run
of the same workflow is looked up and its own SHA is fed back through the exact same `head_sha`
query. If that does not come back with that run, the verdict is UNMEASURABLE, not NO-RUN.

WHICH RUNS ARE EXPECTED — DERIVED, NEVER RECALLED
=================================================

The expected set is every workflow file **at the SHA being judged** (`git show <sha>:.github/...`)
whose `on:` includes `push`, read with `tools/ci-parity.py`'s own `trigger_map`, so the two tools
cannot disagree about what "push-triggered" means. Reading the workflows at the SHA rather than in
the working tree matters: a commit from before a workflow existed must not be refused for lacking its
run. For each expected workflow the jobs are derived too (their display `name:`), and a run that
concluded `success` while a job the workflow declares is absent or not `success` is RED — a job that
did not run is a gate that did not run.

Only `event == push` runs count. `pull_request` runs of the same SHA are named and ignored: they test
a merge commit, not this SHA.

A push trigger with `branches`/`paths`/`tags` filters may legitimately not run for a given push. That
is not decidable from here without the pushed ref and diff, so a missing run of a FILTERED workflow is
still refused, and the message says the absence may be legitimate. (Neither of this repo's workflows
has such a filter today; the branch exists so adding one cannot turn this tool quietly wrong.)

EXIT STATUS — the contract
==========================

    0  GREEN         every expected push workflow has a run, every run and every job concluded success
    1  RED           some run or job concluded anything else — cancelled, failure, timed_out, skipped,
                     action_required, neutral, stale, startup_failure — named with run id and workflow
    2  PENDING       nothing is red yet, and something is still queued/in progress
    3  NO-RUN        an expected workflow has no push run for this SHA (the positive control fired)
    4  UNMEASURABLE  the question could not be asked: git or gh failed, or the positive control did
                     not fire. Never read as any of the above.
    64 usage error

Precedence when several apply: RED > UNMEASURABLE > NO-RUN > PENDING > GREEN. A red is final, so it
is reported even while another workflow is still running.

⚑ **A multi-commit push runs CI on the tip only.** So a commit in the middle of a push has no run of
its own, and that is the commonest NO-RUN. On NO-RUN this tool walks forward from the SHA along
`--ref` (default `origin/main`) to the first descendant that HAS a push run, names it, gives its
verdict, and prints `git diff --stat <sha> <tip> -- crates/`, so "was the code under test the same
code?" is answered on the spot. That descendant's green is evidence about the SHA **only if** that
diff is empty — the tool says which, and never converts it into a GREEN for the SHA asked about.

USAGE
=====

    tools/ci-verdict.py <sha>                 # one query, one verdict
    tools/ci-verdict.py <sha> --wait          # poll while PENDING (and briefly while NO-RUN, since a
                                              # run appears some seconds after a push) until final
    tools/ci-verdict.py <sha> --wait --deadline 150 --interval 60
    tools/ci-verdict.py <sha> --record DIR    # also save every API response (the offline test's input)

The last line of output is always `ci-verdict: <VERDICT> <full sha> — ...`, carrying every job's
conclusion.

Its offline proof is `tools/test_ci_verdict.py`, run by `tools/run_ci_verdict_tests.sh` and by
`tools/land.sh` at G0c.
"""

import argparse
import importlib.util
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

GREEN, RED, PENDING, NO_RUN, UNMEASURABLE, USAGE = 0, 1, 2, 3, 4, 64
WORD = {GREEN: "GREEN", RED: "RED", PENDING: "PENDING", NO_RUN: "NO-RUN",
        UNMEASURABLE: "UNMEASURABLE", USAGE: "USAGE"}
PRECEDENCE = [RED, UNMEASURABLE, NO_RUN, PENDING, GREEN]


WORKFLOW_DIR = ".github/workflows"
TIP_SEARCH_LIMIT = 40


class Unmeasurable(Exception):
    """The question could not be asked. Never converted into a verdict about CI."""


def _load_ci_parity():
    spec = importlib.util.spec_from_file_location("ci_parity", os.path.join(ROOT, "tools", "ci-parity.py"))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


# ------------------------------------------------------------------------------------------------
# git and gh. Both go through one small object each so the offline test can substitute recorded
# API responses without touching the logic that judges them.
# ------------------------------------------------------------------------------------------------
def git(*args, check=True):
    r = subprocess.run(["git", "-C", ROOT, *args], capture_output=True, text=True)
    if check and r.returncode != 0:
        raise Unmeasurable(f"git {' '.join(args)} failed (exit {r.returncode}): {r.stderr.strip()}")
    return r


class Gh:
    """Live GitHub API through `gh api`. `--record DIR` saves each response under its path."""

    def __init__(self, repo, record=None):
        self.repo = repo
        self.record = record

    def api(self, path):
        full = f"repos/{self.repo}/{path}"
        r = subprocess.run(["gh", "api", full], capture_output=True, text=True)
        if r.returncode != 0:
            raise Unmeasurable(f"gh api {full} failed (exit {r.returncode}): {r.stderr.strip()}")
        data = trim(json.loads(r.stdout))
        if self.record:
            os.makedirs(self.record, exist_ok=True)
            with open(os.path.join(self.record, record_name(path)), "w", encoding="utf-8") as fh:
                json.dump(data, fh, indent=1, sort_keys=True)
        return data


class Replay:
    """Recorded API responses (written by `--record`). A path that was not recorded is UNMEASURABLE,
    never an empty answer — an empty answer is exactly the silence this tool refuses to trust."""

    def __init__(self, directory, overrides=None):
        self.dir = directory
        self.overrides = overrides or {}
        self.calls = []

    def api(self, path):
        self.calls.append(path)
        if path in self.overrides:
            return self.overrides[path]
        f = os.path.join(self.dir, record_name(path))
        if not os.path.exists(f):
            raise Unmeasurable(f"no recorded response for {path!r} in {self.dir}")
        with open(f, encoding="utf-8") as fh:
            return json.load(fh)


RUN_KEYS = ("id", "name", "path", "event", "status", "conclusion", "head_sha", "head_branch",
            "run_attempt", "created_at", "updated_at", "workflow_id", "html_url")
JOB_KEYS = ("id", "run_id", "run_attempt", "name", "status", "conclusion", "started_at", "completed_at")
STEP_KEYS = ("number", "name", "status", "conclusion", "started_at", "completed_at")


def trim(data):
    """Keep only the fields a verdict reads. Applied to LIVE responses too, before judging, so a
    recorded response and a live one are the same object to `judge` — the offline test then exercises
    exactly what the live path sees, not a richer or poorer copy."""
    if isinstance(data, dict) and "workflow_runs" in data:
        data = dict(total_count=data.get("total_count"),
                    workflow_runs=[{k: r.get(k) for k in RUN_KEYS} for r in data["workflow_runs"]])
    elif isinstance(data, dict) and "jobs" in data:
        data = dict(total_count=data.get("total_count"),
                    jobs=[dict({k: j.get(k) for k in JOB_KEYS},
                               steps=[{k: s.get(k) for k in STEP_KEYS} for s in (j.get("steps") or [])])
                          for j in data["jobs"]])
    return data


def record_name(path):
    return path.replace("/", "__").replace("?", "~").replace("&", "+").replace("=", "-") + ".json"


def repo_slug():
    r = subprocess.run(["gh", "repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"],
                       capture_output=True, text=True, cwd=ROOT)
    if r.returncode != 0 or not r.stdout.strip():
        raise Unmeasurable(f"cannot resolve the GitHub repo (gh repo view): {r.stderr.strip()}")
    return r.stdout.strip()


def resolve(sha):
    r = git("rev-parse", "--verify", "--quiet", f"{sha}^{{commit}}", check=False)
    if r.returncode != 0 or len(r.stdout.strip()) != 40:
        raise Unmeasurable(
            f"{sha!r} does not resolve to one commit here (git rev-parse --verify). Fetch first, or "
            "pass a longer prefix; an unresolved SHA must never reach the API, which answers a "
            "short or unknown SHA with a silent zero."
        )
    return r.stdout.strip()


# ------------------------------------------------------------------------------------------------
# What SHOULD have run: derived from the workflows as they were at the SHA.
# ------------------------------------------------------------------------------------------------
def expected_workflows(sha):
    """-> [{path, name, jobs: {key: display name or None}, filters: [..]}] for push-triggered files."""
    import yaml  # ci-parity refuses loudly if missing; so do we (via its import)

    cp = _load_ci_parity()
    ls = git("ls-tree", "--name-only", sha, "--", WORKFLOW_DIR + "/")
    out = []
    for path in sorted(ls.stdout.split()):
        if not path.endswith((".yml", ".yaml")):
            continue
        doc = yaml.safe_load(git("show", f"{sha}:{path}").stdout)
        if not isinstance(doc, dict):
            continue
        trig = cp.trigger_map(doc)
        if "push" not in trig:
            continue
        cfg = trig["push"] if isinstance(trig["push"], dict) else {}
        filters = sorted(k for k in cfg if k in ("branches", "branches-ignore", "paths",
                                                   "paths-ignore", "tags", "tags-ignore"))
        jobs = {}
        for key, job in (doc.get("jobs") or {}).items():
            name = (job or {}).get("name", key)
            # A templated name (`${{ matrix.os }}`) or a matrix job cannot be matched by string.
            if not isinstance(name, str) or "${{" in name or (job or {}).get("strategy"):
                name = None
            jobs[key] = name
        out.append(dict(path=path, name=doc.get("name", path), jobs=jobs, filters=filters))
    return out


# ------------------------------------------------------------------------------------------------
# THE JUDGEMENT. Pure: expected workflows + raw API run/job lists in, (code, lines) out.
# ------------------------------------------------------------------------------------------------
def judge(expected, runs, jobs_by_run):
    """-> (code, detail lines, summary fragments, missing workflow paths)."""
    lines, summary, missing = [], [], []
    codes = set()
    push = [r for r in runs if r.get("event") == "push"]
    for r in runs:
        if r.get("event") != "push":
            lines.append(f"  ignored: run {r['id']} ({r.get('name')}) event={r.get('event')} — not a push run, so "
                         "not part of this verdict")
    want = {w["path"]: w for w in expected}
    for w in expected:
        mine = [r for r in push if r.get("path") == w["path"]]
        if not mine:
            codes.add(NO_RUN)
            missing.append(w["path"])
            why = ""
            if w["filters"]:
                why = (f" — its push trigger is filtered ({', '.join(w['filters'])}), so this absence "
                       "MAY be legitimate; check the pushed ref/paths by hand")
            lines.append(f"  NO RUN: {w['name']} ({w['path']}) has no push run for this SHA{why}")
            summary.append(f"{w['name']}: no run")
    for r in push:
        w = want.get(r.get("path"))
        tag = f"{r.get('name')} run {r['id']} (attempt {r.get('run_attempt', 1)}, {r.get('head_branch')})"
        if w is None:
            lines.append(f"  note: {tag} is from {r.get('path')}, which is not a push workflow at this "
                         "SHA; judged anyway")
        status, concl = r.get("status"), r.get("conclusion")
        jobs = jobs_by_run.get(r["id"], [])
        jtxt = ", ".join(f"{j['name']}={j.get('conclusion') or j.get('status')}" for j in jobs) or "no jobs"
        if status != "completed":
            codes.add(PENDING)
            lines.append(f"  PENDING: {tag} status={status}; jobs: {jtxt}")
            summary.append(f"{r.get('name')} run {r['id']} {status} [{jtxt}]")
            continue
        if concl != "success":
            codes.add(RED)
            lines.append(f"  RED: {tag} concluded {str(concl).upper()}; jobs: {jtxt}")
            summary.append(f"{r.get('name')} run {r['id']} {str(concl).upper()} [{jtxt}]")
            continue
        bad = []
        for j in jobs:
            if j.get("conclusion") != "success":
                bad.append(f"job {j['name']!r} concluded {str(j.get('conclusion')).upper()}")
        if w is not None:
            seen = {j["name"] for j in jobs}
            for key, name in w["jobs"].items():
                if name is None:
                    lines.append(f"  note: job {key!r} has a templated/matrix name; its presence is "
                                 "not checked by name")
                elif name not in seen:
                    bad.append(f"declared job {name!r} ({key}) has NO job in this run")
        if not jobs:
            bad.append("the run lists no jobs at all")
        if bad:
            codes.add(RED)
            lines.append(f"  RED: {tag} concluded success, but " + "; ".join(bad) + f"; jobs: {jtxt}")
            summary.append(f"{r.get('name')} run {r['id']} success-but-{'/'.join(bad)} [{jtxt}]")
        else:
            lines.append(f"  green: {tag} concluded success; jobs: {jtxt}")
            summary.append(f"{r.get('name')} run {r['id']} success [{jtxt}]")
    if not expected and not push:
        codes.add(NO_RUN)
        lines.append("  NO RUN: this SHA has no push-triggered workflow AND no push run")
    code = next(c for c in PRECEDENCE if c in codes) if codes else GREEN
    return code, lines, summary, missing


# ------------------------------------------------------------------------------------------------
# Fetching.
# ------------------------------------------------------------------------------------------------
def runs_for(gh, sha):
    out, page = [], 1
    while True:
        d = gh.api(f"actions/runs?head_sha={sha}&per_page=100&page={page}")
        if "workflow_runs" not in d or "total_count" not in d:
            raise Unmeasurable(f"unexpected runs response for {sha}: keys {sorted(d)}")
        out += d["workflow_runs"]
        if len(out) >= d["total_count"] or not d["workflow_runs"]:
            return out
        page += 1


def jobs_for(gh, run_id):
    d = gh.api(f"actions/runs/{run_id}/jobs?filter=latest&per_page=100")
    if "jobs" not in d:
        raise Unmeasurable(f"unexpected jobs response for run {run_id}: keys {sorted(d)}")
    return d["jobs"]


def positive_control(gh, workflow_path):
    """Prove the `head_sha` query fires: the newest push run of this workflow must come back when
    its own SHA is asked about. -> a line describing the control. Raises Unmeasurable if not."""
    base = os.path.basename(workflow_path)
    d = gh.api(f"actions/workflows/{base}/runs?event=push&per_page=1")
    newest = (d.get("workflow_runs") or [None])[0]
    if newest is None:
        raise Unmeasurable(f"positive control impossible: {workflow_path} has never had a push run, "
                           "so no SHA is known for which the query must fire")
    got = runs_for(gh, newest["head_sha"])
    if not any(r["id"] == newest["id"] for r in got):
        raise Unmeasurable(
            f"POSITIVE CONTROL FAILED: head_sha={newest['head_sha']} did not return its own run "
            f"{newest['id']} — the query that reported NO RUN is not shown to fire")
    return (f"  positive control: head_sha={newest['head_sha']} (newest push run of {base}) -> "
            f"{len(got)} run(s), including {newest['id']}. The query fires; the absence above is real.")


def tip_search(gh, sha, ref, expected_paths):
    """On NO-RUN: the first descendant of `sha` along `ref` that has a push run of a missing workflow."""
    lines = []
    if git("rev-parse", "--verify", "--quiet", f"{ref}^{{commit}}", check=False).returncode != 0:
        return [f"  tip search skipped: --ref {ref!r} does not resolve here (fetch it, or pass --ref)"]
    if git("merge-base", "--is-ancestor", sha, ref, check=False).returncode != 0:
        remote = git("branch", "-r", "--contains", sha, check=False).stdout.split()
        lines.append(f"  {sha[:12]} is not an ancestor of {ref}"
                     + (f"; remote refs containing it: {', '.join(remote)}" if remote else
                        "; NO remote-tracking ref contains it (after your last fetch) — it may never "
                        "have been pushed, in which case no run can exist"))
        return lines
    desc = git("rev-list", "--ancestry-path", "--reverse", f"{sha}..{ref}").stdout.split()
    for cand in desc[:TIP_SEARCH_LIMIT]:
        runs = [r for r in runs_for(gh, cand) if r.get("event") == "push" and r.get("path") in expected_paths]
        if not runs:
            continue
        concl = ", ".join(f"{r.get('name')} run {r['id']} {r.get('status')}/{r.get('conclusion')}" for r in runs)
        subj = git("log", "-1", "--format=%s", cand).stdout.strip()
        lines.append(f"  the next descendant WITH a push run (the likely tip of the push that carried "
                     f"{sha[:12]}): {cand}  {subj[:70]}")
        lines.append(f"    its runs: {concl}")
        stat = git("diff", "--stat", sha, cand, "--", "crates/").stdout.rstrip()
        if stat:
            lines.append(f"    crates/ DIFFERS between {sha[:12]} and {cand[:12]} — that run is NOT evidence "
                         f"for this SHA's code:  git diff --stat {sha[:12]} {cand[:12]} -- crates/")
            lines += ["      " + l for l in stat.splitlines()[-6:]]
        else:
            lines.append(f"    crates/ is IDENTICAL between {sha[:12]} and {cand[:12]} (git diff --stat "
                         f"{sha[:12]} {cand[:12]} -- crates/ is empty): that run tested the same code. "
                         f"Ask for its verdict:  tools/ci-verdict.py {cand[:12]}")
        return lines
    lines.append(f"  no descendant of {sha[:12]} on {ref} within {TIP_SEARCH_LIMIT} commits has a push "
                 f"run ({len(desc)} descendant(s) searched up to the limit)")
    return lines


def verdict(gh, sha, ref="origin/main"):
    """One full evaluation. -> (code, lines, final line)."""
    lines = []
    try:
        expected = expected_workflows(sha)
        runs = runs_for(gh, sha)
        jobs = {r["id"]: jobs_for(gh, r["id"]) for r in runs if r.get("event") == "push"}
        code, jl, summary, missing = judge(expected, runs, jobs)
        lines += [f"  expected push workflows at this SHA: "
                  + (", ".join(f"{w['name']} ({w['path']})" for w in expected) or "none")]
        lines += jl
        if code == NO_RUN or missing:
            lines.append("  ⚑ a multi-commit push runs CI on its TIP only; a commit inside a push has no "
                         "run of its own")
            ctl_path = missing[0] if missing else (expected[0]["path"] if expected else None)
            if ctl_path is None:
                # Nothing was expected at this SHA (it predates the workflows). The control still has
                # to fire, so it borrows a push workflow from HEAD.
                head = expected_workflows("HEAD")
                if not head:
                    raise Unmeasurable("no push-triggered workflow at this SHA or at HEAD, so no query "
                                       "can be shown to fire")
                ctl_path = head[0]["path"]
            lines.append(positive_control(gh, ctl_path))
            lines += tip_search(gh, sha, ref, {w["path"] for w in expected} or {ctl_path})
    except Unmeasurable as e:
        return UNMEASURABLE, lines + [f"  UNMEASURABLE: {e}"], f"ci-verdict: UNMEASURABLE {sha} — {e}"
    return code, lines, f"ci-verdict: {WORD[code]} {sha} — " + ("; ".join(summary) or "no runs")


def main(argv=None):
    ap = argparse.ArgumentParser(description="Exit 0 only if every push-triggered CI run for the SHA "
                                 "concluded success. See the module docstring for the exit codes.")
    ap.add_argument("sha")
    ap.add_argument("--wait", action="store_true", help="poll while PENDING until a final verdict")
    ap.add_argument("--deadline", type=float, default=150.0, metavar="MIN",
                    help="--wait gives up (exit 2, PENDING) after this many minutes (default 150)")
    ap.add_argument("--interval", type=float, default=60.0, metavar="SEC", help="--wait poll interval")
    ap.add_argument("--grace", type=float, default=3.0, metavar="MIN",
                    help="--wait keeps polling a NO-RUN this long, since a run appears seconds after "
                    "a push (default 3)")
    ap.add_argument("--ref", default="origin/main", help="where to look for the push tip on NO-RUN")
    ap.add_argument("--repo", default=None, help="owner/name (default: gh repo view)")
    ap.add_argument("--record", default=None, metavar="DIR", help="save every API response to DIR")
    ap.add_argument("--replay", default=None, metavar="DIR", help="answer from recorded responses")
    try:
        args = ap.parse_args(argv)
    except SystemExit as e:
        return USAGE if e.code else 0
    try:
        sha = resolve(args.sha)
        gh = Replay(args.replay) if args.replay else Gh(args.repo or repo_slug(), args.record)
    except Unmeasurable as e:
        print(f"ci-verdict: UNMEASURABLE {args.sha} — {e}")
        return UNMEASURABLE
    t0 = time.monotonic()
    blips = 0
    while True:
        code, lines, final = verdict(gh, sha, args.ref)
        waited = (time.monotonic() - t0) / 60
        # Under --wait a single failed query (a network blip an hour into a wait) is retried, up to
        # three in a row; a fourth is the verdict. Without --wait it is reported at once.
        blips = blips + 1 if code == UNMEASURABLE else 0
        keep = args.wait and waited < args.deadline and (
            code == PENDING or (code == NO_RUN and waited < args.grace)
            or (code == UNMEASURABLE and blips <= 3))
        if not keep:
            break
        print(f"[{time.strftime('%H:%M:%S')}] {waited:5.1f} min  {final}", flush=True)
        time.sleep(args.interval)
    for l in lines:
        print(l)
    if args.wait and code == PENDING:
        print(f"  --wait deadline of {args.deadline:g} min reached; the run is STILL going. Not a verdict.")
    print(final)
    return code


if __name__ == "__main__":
    sys.exit(main())
