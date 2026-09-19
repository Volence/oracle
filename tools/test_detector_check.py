#!/usr/bin/env python3
"""Red-first proof for `tools/detector-check.py`, against SHAPES THIS LANE ACTUALLY PRODUCED.

WHY IT IS BUILT THIS WAY
========================

⚑ **A detector tested only against a case written to satisfy it is the defect it exists to catch.**
So every case below is transcribed from a dated instance in this lane's own record — the two cohort
documents of 2026-09-19 and the bars in `docs/OVERSEER-REFERENCE.md` — and each row says where.
Rows are labelled:

* `REAL`      — the bytes come out of `git show <rev>:<path>`. Nothing was written for the test.
* `DERIVED`   — the field values are transcriptions of a real sentence or a real invocation from a
                named document, placed into the declaration form. The defect is dated; the framing
                is this file's.
* `CONSTRUCTED` — no instance of the defect exists in this repo's record. Said outright, in the
                output, so no reader mistakes it for the others.

**THE GREEN ARM.** Every `docs/*.md` at the working tree must come back clean. Without it this file
would be satisfied by a checker that reddens on everything, and the next landing could not pass its
own G2c.

**WHAT THERE IS NO CONTROL ARM FOR, AND WHY IT IS SAID RATHER THAN OMITTED.**
`tools/test_lane_check.py` runs the PREVIOUS gate on the same bytes, so "the old one missed it" is
executed rather than asserted. There is no previous gate here: `detector-check.py` is the first
thing in this repo that reads these declarations, and the thing it replaces is **prose in
`docs/OVERSEER-REFERENCE.md`**, which has no executable form. The honest statement of the control is
therefore historical and not runnable: all three bars were already banked as prose on 2026-09-19 and
all three habits were broken **after** they were banked — the false-absence bar explicitly so
("I had banked 'an absence is never a finding' hours before the second and third"). That is the
measurement standing in for a control arm, and it is a weaker instrument than the one
`test_lane_check.py` has. Naming it beats implying a control that was never run.

USAGE
=====

    tools/run_detector_check_tests.sh     # the runner; this file is not meant to be run bare
"""

import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOOL = os.path.join(ROOT, "tools", "detector-check.py")

# The base this parcel branched from: the last revision at which NEITHER cohort doc carried a
# declarations block. Case 1 reads its real bytes.
BASE_REV = "74c3579"

HEADER = "# scratch\n\n**Date:** 2026-09-19 · **Kind:** investigation.\n\n"


def block(body):
    return HEADER + "```detectors\n" + body.strip("\n") + "\n```\n"


# A declaration set that passes everything, so each case below can vary exactly one thing. The
# values are cohort 2's real ones (docs/2026-09-19-reds-0909-cohort.md).
GOOD_ABSENCE = """ABSENCE: UNDETERMINED = 0 of 7 runs
  instrument: grep -c '<pattern>' <run>.log
  positive:   grep -c 'noDisplay' 34752339602.log -> 1
  negative:   grep -c 'noDisplay' 35051831548.log -> 0
  scope:      gh run list --limit 500 returned 500 runs, oldest 2026-09-04T01:01:13Z
  contains:   2026-09-09 lies five days inside the oldest bound; 7 of 63 runs that day failed
"""
GOOD_HEURISTIC = """HEURISTIC: "the sibling CI jobs were green, therefore not INFRA" — does not transfer
  from:    docs/2026-09-19-reds-without-a-cause.md
  assumes: that every job runs the failing step
  checked: 35f81ca:.github/workflows/ci.yml:63 "System libraries for the frontend/player build"
"""
GOOD_CANNOT = """CANNOT-TEST: "this lint fires on the CI floor and not on this machine" — DISPROVED
  attempted: cargo clippy --all-targets -- -D warnings on a two-file scratch crate -> exit 101
  cost:      20 seconds, two files, no toolchain manager
  prior:     2999687
"""
GOOD = GOOD_ABSENCE + "\n" + GOOD_HEURISTIC + "\n" + GOOD_CANNOT


def swap(base, old, new):
    assert old in base, f"fixture drifted: {old!r} not in the good block"
    return base.replace(old, new, 1)


# ---------------------------------------------------------------------------------------------
# THE CORPUS. `expect` is the substring that must appear in a finding — THE GUARD THAT MUST FIRE,
# named, so a case cannot be satisfied by some other rule reddening for an unrelated reason.
# ---------------------------------------------------------------------------------------------
CASES = [
    dict(
        label="REAL",
        habit="all three",
        name="an investigation doc with no declarations at all",
        origin=f"the two cohort docs as committed at {BASE_REV} — the state of the tree when this "
        f"parcel was dispatched",
        rev=BASE_REV,
        path="docs/2026-09-19-reds-0909-cohort.md",
        expect="carries no ```detectors block",
    ),
    dict(
        label="REAL",
        habit="all three",
        name="the other investigation doc, same state",
        origin=f"docs/2026-09-19-reds-without-a-cause.md at {BASE_REV}",
        rev=BASE_REV,
        path="docs/2026-09-19-reds-without-a-cause.md",
        expect="carries no ```detectors block",
    ),
    dict(
        label="DERIVED",
        habit="1 — false absence",
        name="the control controls a different instrument than the one that produced the zero",
        origin="OVERSEER-REFERENCE.md, the false-absence bar, instance (1): a CI waiter on "
        "`gh run list --limit 8` whose SHA had fallen out of the window. The zeros in all three "
        "instances came out of `gh`; the only thing ever controlled was a `grep`.",
        text=block(
            swap(
                GOOD,
                "  instrument: grep -c '<pattern>' <run>.log",
                "  instrument: gh run list --limit 8 --json headSha,status",
            )
        ),
        expect="controls a DIFFERENT INSTRUMENT",
    ),
    dict(
        label="DERIVED",
        habit="1 — false absence",
        name="a positive control that returned zero",
        origin="OVERSEER-REFERENCE.md, instance (2): a sweep regex that reported "
        '"log expired or not a test" for eight runs whose logs were fully retrievable. The regex '
        "was never shown to fire on a log that had the thing.",
        text=block(
            swap(
                GOOD,
                "grep -c 'noDisplay' 34752339602.log -> 1",
                "grep -c 'noDisplay' 34752339602.log -> 0",
            )
        ),
        expect="THE CONTROL DID NOT FIRE",
    ),
    dict(
        label="DERIVED",
        habit="1 — false absence",
        name="a correct control pair and no window — the case a control pair alone cannot catch",
        origin="OVERSEER-REFERENCE.md, instance (3): a survey reporting \"logs gone\" for three "
        "runs of 1683/1675/1692 lines, because `gh run list --limit 200` had been pushed past them "
        "by the night's own commits. An extractor control fires happily here and the zero is still "
        "wrong; only the enumerated window catches it (docs/2026-09-19-reds-0909-cohort.md, "
        '"The window, proved rather than assumed").',
        text=block(
            swap(
                GOOD,
                "  scope:      gh run list --limit 500 returned 500 runs, oldest 2026-09-04T01:01:13Z\n",
                "",
            )
        ),
        expect="missing `scope`",
    ),
    dict(
        label="REAL",
        habit="1 — false absence",
        name="an absence with no negative control",
        origin="docs/2026-09-19-reds-0909-cohort.md, the RULE 3 sweep: \"not one of the seven run "
        'ids appears anywhere in docs/ or in any commit message". That grep has a positive control '
        "in the document (two SHAs DO appear, at lane-log.jsonl:184-185) and no negative one.",
        text=block(
            swap(
                GOOD,
                "  negative:   grep -c 'noDisplay' 35051831548.log -> 0\n",
                "",
            )
        ),
        expect="missing `negative`",
    ),
    dict(
        label="DERIVED",
        habit="2 — borrowed heuristic",
        name="a heuristic inherited without checking its topology",
        origin="docs/2026-09-19-reds-without-a-cause.md ruled INFRA = 0 partly on sibling-green "
        "and never opened the workflow; ci.yml:63 at 35f81ca is where that would have been settled.",
        text=block(swap(GOOD, "  checked: 35f81ca:", "  checked: not-checked — 35f81ca:")),
        expect="topology was not checked in THIS investigation",
    ),
    dict(
        label="DERIVED",
        habit="2 — borrowed heuristic",
        name="a citation that points at the wrong line",
        origin="the same real citation, moved off the step it is supposed to prove. `ci.yml` at "
        "35f81ca is a live file that keeps changing; a citation by line number rots silently.",
        text=block(
            swap(GOOD, ".github/workflows/ci.yml:63 ", ".github/workflows/ci.yml:6 ")
        ),
        expect="but that line reads",
    ),
    dict(
        label="DERIVED",
        habit="2 — borrowed heuristic",
        name="a citation to a revision that does not exist",
        origin="the same citation with a plausible-looking SHA. The class is dated in this lane: "
        "tools/lane-check.py's docstring records two decision cards stamped `d-31`/`d-32` by "
        "assuming the next free number instead of measuring, and both ids were already taken.",
        text=block(swap(GOOD, "  checked: 35f81ca:", "  checked: 35f81cb:")),
        expect="does not resolve in this repository",
    ),
    dict(
        label="DERIVED",
        habit="3 — cannot be tested here",
        name="an impossibility claim whose `attempted` names an intention, not a result",
        origin="docs/2026-09-19-reds-without-a-cause.md, Event A, verbatim: \"The floor lint cannot "
        'be run here at any price short of installing a toolchain manager." The next night the same '
        "class of lint reproduced in twenty seconds.",
        text=block(
            swap(
                GOOD,
                "  attempted: cargo clippy --all-targets -- -D warnings on a two-file scratch crate -> exit 101",
                "  attempted: installing a toolchain manager was judged too expensive to be worth it",
            )
        ),
        expect="names no result",
    ),
    dict(
        label="DERIVED",
        habit="3 — cannot be tested here",
        name="a `prior` naming a revision that does not exist",
        origin="the correction this lane refused twice is `2999687`, a real commit. A `prior` field "
        "filled from memory rather than from a grep is the same defect one step later.",
        text=block(swap(GOOD, "  prior:     2999687", "  prior:     2999688")),
        expect="does not exist in this repository",
    ),
    dict(
        label="CONSTRUCTED",
        habit="all three",
        name="a declaration answering `none` with no reason",
        origin="NO INSTANCE IN THIS REPO'S RECORD. The defect is the checkbox failure mode this "
        "block is most likely to decay into, and it is guarded pre-emptively. Constructed, and "
        "labelled so, because an absence is never a finding and a constructed fixture is never "
        "dressed as a measured one.",
        text=block(GOOD_ABSENCE + "\nHEURISTIC: none\n\n" + GOOD_CANNOT),
        expect="answers `none` with no reason",
    ),
]


def run_tool(*paths):
    p = subprocess.run(
        [sys.executable, TOOL, *paths], capture_output=True, text=True, cwd=ROOT
    )
    return p.returncode, p.stdout + p.stderr


def main():
    rc = 0
    tmp = tempfile.mkdtemp(prefix="detector-check-")

    print(f"THE TOOL: {os.path.relpath(TOOL, ROOT)}")
    print(f"{len(CASES)} red cases, then the green arm.\n")

    for i, case in enumerate(CASES, 1):
        path = os.path.join(tmp, f"case{i:02d}.md")
        if "rev" in case:
            p = subprocess.run(
                ["git", "-C", ROOT, "show", f"{case['rev']}:{case['path']}"],
                capture_output=True,
                text=True,
            )
            if p.returncode != 0:
                # A revision this test cannot reach is UNMEASURABLE, never a skip.
                print(
                    f"[{i:02d}] UNMEASURABLE  {case['name']}\n"
                    f"     {case['rev']}:{case['path']} is unreachable "
                    f"(shallow clone?); an absence is never a finding"
                )
                rc = 1
                continue
            body = p.stdout
        else:
            body = case["text"]
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(body)

        got_rc, out = run_tool(path)
        fired = case["expect"] in out
        ok = got_rc != 0 and fired
        print(f"[{i:02d}] {'RED  ' if ok else 'MISS '} {case['label']:<11} {case['name']}")
        print(f"     habit {case['habit']}")
        print(f"     from: {case['origin']}")
        print(f"     guard: {case['expect']!r}")
        if not ok:
            rc = 1
            print(f"     ---- EXPECTED RED ON THAT GUARD; got exit {got_rc} ----")
            for line in out.splitlines():
                print(f"     | {line}")
        print()

    print("=== the green arm: every docs/*.md in the working tree ===")
    got_rc, out = run_tool()
    for line in out.splitlines():
        print(f"  {line}")
    if got_rc != 0:
        print("  ---- THE GREEN ARM IS RED; the gate would refuse this repo's own docs ----")
        rc = 1

    print()
    print("test_detector_check: " + ("PASS" if rc == 0 else "FAIL"))
    return rc


if __name__ == "__main__":
    sys.exit(main())
