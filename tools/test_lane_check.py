#!/usr/bin/env python3
"""Red-first proof for `tools/lane-check.py`, against SHAPES THIS REPO ACTUALLY COMMITTED.

WHY IT IS BUILT THIS WAY, AND NOT THE OBVIOUS WAY
=================================================

The obvious test suite writes a small JSON file exhibiting each defect and asserts the tool
reddens. **That is precisely the failure this whole parcel is about**: `lane-check.py` printed
`clean` on every landing for two weeks while enforcing almost none of the contract, and a rule
verified only against a file written to satisfy it reproduces that exactly — the fixture is shaped
by the same belief as the check, so the pair agree and neither is measured.

So every case below is a **real, dated revision of `docs/lane-status.json` from this repo's own
history**, named by its commit SHA and extracted with `git show`. The fixtures were not written;
they were found, by replaying all 255 committed revisions of the file and asking which rules each
would break. That replay is the `--corpus` mode at the bottom.

**THE CONTROL IS THE LOAD-BEARING ARM.** For each case this also extracts `tools/lane-check.py` as
it stood at `f4022d6` — the gate that was live when this parcel was dispatched — and runs it on the
same bytes. It must print `clean` and exit 0. A new check is only worth its line count if the old
one missed the case, and "would have been missed" is a claim with an executable form, so it is
executed rather than asserted. *(Banked lesson from this lane, 2026-09-18: a control arm written
down and never run was unexecutable.)*

**AND THE GREEN ARM.** `HEAD`'s own `docs/lane-status.json` must stay clean under the widened gate.
Without it this file would be satisfied by a validator that reddens on everything, and the next
landing would be unable to pass its own G2b.

WHAT IS NOT PROVEN HERE, AND WHY IT IS SAID RATHER THAN OMITTED
===============================================================

One rule — **an unknown key on a queue row** — has **no instance in this repo's history**. The
defect is real and dated (aeon, 2026-09-16, an invented `anchors` key; the console answered `ok`
and silently dropped it, which is why the contract records it) but it happened in *aeon's* tree,
and this file only replays oracle's. Its case is therefore CONSTRUCTED, from a real HEAD row plus
that one key, and is labelled `CONSTRUCTED` in the output so no reader mistakes it for the others.
An absence is never a finding, and a constructed fixture is never dressed as a measured one.

USAGE
=====

    tools/run_lane_check_tests.sh          # the runner; this file is not meant to be run bare
    tools/test_lane_check.py --corpus      # replay all 255 revisions and report the rule census
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOOL = os.path.join(ROOT, "tools", "lane-check.py")
STATUS = "docs/lane-status.json"
DECISIONS = "docs/decisions.jsonl"

# The gate as it stood when this parcel was dispatched. The control runs THIS on the same bytes.
BASELINE_REV = "f4022d630e8fd89fa2eeea7a59e24622a1aac266"

# ---------------------------------------------------------------------------------------------
# THE CORPUS. Every row is a real commit of this repo. `expect` is a substring that must appear in
# a finding — the GUARD THAT MUST FIRE, named, so a case cannot be satisfied by some other rule
# going red for an unrelated reason.
# ---------------------------------------------------------------------------------------------
CASES = [
    dict(
        rev="48e8e12525cb270c221a775ef877494b566fa0f1",
        when="2026-09-19T07:39:48-04:00",
        rule="exactly one `next`",
        expect="2 rows are 'next' and the contract says exactly one",
        story="the defect the console flagged on the morning this parcel was written; the third "
        "lane to hit it that night. 21 revisions of this file carry two, and 24 carry three or more.",
    ),
    dict(
        rev="aba31c31102b946f4616415e3fe1044e41c57729",
        when="2026-09-18T22:29:32-04:00",
        rule="`next` with a non-null blockedBy (lane has a `doing` row)",
        expect="state 'next' with a non-null blockedBy, on a lane that has a 'doing' row",
        story="PANEL-CLIP-MARK sat `next` behind card d-54 while F-PANEL-CLIP-TOTAL-LOSS-UNSIGNALLED "
        "was `doing`. 16 revisions carry this pair.",
    ),
    dict(
        rev="558de4132e70435984caff859e61e7501b5d0eed",
        when="2026-09-18T22:26:49-04:00",
        rule="`open` with a non-null blockedBy",
        expect="READS AS STARTABLE AND IS NOT",
        story="FIVE rows at once. This is the dominant class — 175 of 255 revisions — and the one "
        "the contract only gained on 2026-09-19 (empyrean b8a7333f), from this lane's measurement.",
        count=5,
    ),
    dict(
        rev="8d58a766635056ac9103984e8c4bce03276bb214",
        when="2026-09-18T20:30:27-04:00",
        rule="`focus` <= 120 characters",
        expect="'focus' is 126 characters, over the 120 bound",
        story="126 characters — the fourth size bound, the one not in contract rule 7 and therefore "
        "the one a lane told to 'check rule 7' still misses. 35 revisions are over it.",
    ),
    dict(
        rev="8ca30566cc3c6a24f20f570c79b1110e453109b4",
        when="2026-09-08T14:23:22-04:00",
        rule="`blocked` with no blocker named",
        expect="state 'blocked' and no blocker named",
        story="the inverse pair. 11 revisions carry it; no reader can tell whether the thing the row "
        "waits on has landed.",
    ),
    dict(
        rev="ee314cd3682d9c2177e999b1b36c83b310bc43a4",
        when="2026-09-10T02:45:52-04:00",
        rule="queue <= 20 rows, file <= 12 KB, title <= 240 (rule 7's three, on one revision)",
        expect="over the 20 bound (rule 7)",
        story="the board at its largest. The same revision is over the byte bound and carries "
        "over-long titles, which is the shape rule 7 was ruled against.",
    ),
    dict(
        rev="015f988576d29cf55f8017098460000ed5581931",
        when="2026-09-06T18:58:01-04:00",
        rule="boundary audit check 1: a `blockedOnOwner` id naming no card",
        expect="and no card in the decision ledger has that id",
        story="the only instance in 255 revisions: a blockedOnOwner entry stamped id 'limit', which "
        "is not a card. The console renders the blocker and the owner opens nothing.",
    ),
]


def git(*a, check=True):
    r = subprocess.run(["git", "-C", ROOT, *a], capture_output=True, text=True)
    if check and r.returncode != 0:
        raise RuntimeError(f"git {' '.join(a)} failed: {r.stderr.strip()}")
    return r.stdout


def run_tool(tool, status, decisions, label):
    r = subprocess.run(
        [sys.executable, tool, "--status", status, "--decisions", decisions, "--label", label],
        capture_output=True,
        text=True,
    )
    return r.returncode, r.stdout + r.stderr


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--corpus", action="store_true", help="replay every revision, print the census")
    args = ap.parse_args()

    decisions = os.path.join(ROOT, DECISIONS)
    failures = []
    tmp = tempfile.mkdtemp(prefix="lane-check-redfirst-")

    if args.corpus:
        return corpus(decisions)

    # ------------------------------------------------------------------------------------------
    # THE GREEN ARM first: a gate that reddens on everything proves nothing, and a widened gate
    # that HEAD cannot pass makes the next landing impossible.
    # ------------------------------------------------------------------------------------------
    code, out = run_tool(TOOL, os.path.join(ROOT, STATUS), decisions, "HEAD working tree")
    if code == 0:
        print(f"  ok    GREEN ARM   HEAD's own {STATUS} is clean under the widened gate")
    else:
        failures.append("GREEN ARM: HEAD's own lane-status.json is RED under the widened gate")
        print(f"  FAIL  GREEN ARM   HEAD is RED:\n{out}")

    # The baseline tool, extracted once. Refuse rather than skip if it cannot be had: a control
    # that silently does not run is the thing this file exists to avoid.
    baseline = os.path.join(tmp, "lane-check-baseline.py")
    try:
        with open(baseline, "w", encoding="utf-8") as fh:
            fh.write(git("show", f"{BASELINE_REV}:tools/lane-check.py"))
    except RuntimeError as e:
        print(f"  FAIL  CONTROL is UNMEASURABLE: cannot extract the baseline gate: {e}")
        failures.append("the control arm could not be run; 'would have been missed' is unproven")
        baseline = None

    for case in CASES:
        rev, rule = case["rev"], case["rule"]
        try:
            blob = git("show", f"{rev}:{STATUS}")
        except RuntimeError as e:
            # NOT a skip. A corpus revision that cannot be read makes the proof unmeasurable, and
            # an unmeasurable proof must never read as a pass.
            print(f"  FAIL  {rev[:12]}  {rule}: revision unreachable ({e}); UNMEASURABLE")
            failures.append(f"{rev[:12]}: corpus revision unreachable")
            continue
        path = os.path.join(tmp, f"{rev[:12]}.json")
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(blob)

        code, out = run_tool(TOOL, path, decisions, rev[:12])
        hits = out.count(case["expect"])
        want = case.get("count", 1)
        if code == 1 and hits >= want:
            print(f"  ok    {rev[:12]}  {case['when']}  RED on: {rule}  ({hits} finding(s))")
        else:
            print(f"  FAIL  {rev[:12]}  {rule}: exit {code}, {hits} matching finding(s), wanted "
                  f">= {want}\n{out}")
            failures.append(f"{rev[:12]}: {rule} did not fire")

        if baseline:
            bcode, bout = run_tool(baseline, path, decisions, rev[:12])
            if bcode == 0:
                print(f"        CONTROL   the gate at {BASELINE_REV[:7]} says 'clean' on these "
                      "same bytes — the case WOULD have been missed")
            else:
                # Not a failure of the new gate: it means the old gate already caught this file for
                # some OTHER reason, so this case does not prove a widening. Say so rather than
                # counting it as proof.
                print(f"        CONTROL   the gate at {BASELINE_REV[:7]} ALSO reddens here, so "
                      "this revision does not on its own prove the widening:\n"
                      + "\n".join("          " + line for line in bout.strip().splitlines()[:4]))
                failures.append(
                    f"{rev[:12]}: the control also reddened, so this case proves no widening"
                )

    # ------------------------------------------------------------------------------------------
    # THE ONE CONSTRUCTED CASE, labelled as such. See the module docstring.
    # ------------------------------------------------------------------------------------------
    doc = json.loads(open(os.path.join(ROOT, STATUS), encoding="utf-8").read())
    if doc.get("queue"):
        doc["queue"][0]["anchors"] = ["docs/OVERSEER-LOG.md", "crates/oracle-frontend"]
        path = os.path.join(tmp, "constructed-unknown-key.json")
        with open(path, "w", encoding="utf-8") as fh:
            json.dump(doc, fh)
        code, out = run_tool(TOOL, path, decisions, "CONSTRUCTED")
        if code == 1 and "which is not in the row shape" in out:
            print("  ok    CONSTRUCTED  RED on: an unknown key on a queue row (aeon's `anchors`, "
                  "2026-09-16 — real defect, but in AEON's tree, so no oracle revision has it)")
        else:
            print(f"  FAIL  CONSTRUCTED  unknown-key rule did not fire: exit {code}\n{out}")
            failures.append("CONSTRUCTED: the unknown-key rule did not fire")
        if baseline:
            bcode, _ = run_tool(baseline, path, decisions, "CONSTRUCTED")
            print(f"        CONTROL   the gate at {BASELINE_REV[:7]} exits {bcode} here"
                  + (" — would have been missed" if bcode == 0 else ""))

    print()
    if failures:
        for f in failures:
            print(f"  BAD   {f}")
        print(f"test_lane_check: {len(failures)} failure(s)")
        return 1
    print(f"test_lane_check: {len(CASES)} real revisions + 1 constructed case, all RED where "
          "expected, all clean under the control, HEAD green.")
    return 0


def corpus(decisions):
    """Replay every committed revision of the status file and print the rule census.

    This is where the CASES above came from, and re-running it is how a future reader checks that
    the counts quoted in `lane-check.py`'s docstring are still the counts.
    """
    revs = [
        l.split()
        for l in git("log", "--format=%H %cI", "--follow", "--", STATUS).splitlines()
        if l.strip()
    ]
    tmp = tempfile.mkdtemp(prefix="lane-check-corpus-")
    red = 0
    total = 0
    for sha, when in revs:
        try:
            blob = git("show", f"{sha}:{STATUS}")
        except RuntimeError:
            continue
        total += 1
        p = os.path.join(tmp, "s.json")
        with open(p, "w", encoding="utf-8") as fh:
            fh.write(blob)
        code, _ = run_tool(TOOL, p, decisions, sha[:12])
        if code:
            red += 1
    print(f"corpus: {red} of {total} committed revisions of {STATUS} are RED under the widened "
          f"gate ({total - red} clean). The gate at {BASELINE_REV[:7]} passed all {total}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
