#!/usr/bin/env python3
"""Red-first proof for `tools/ci-parity.py`: the four ways CI can drift away from the landing gate.

WHY A TEST AND NOT A READING
============================

`ci-parity.py`'s entire value is the branch where it says NO. It runs on every landing and, on the
tree it was written against, it exits 0 — so on a green tree its output is indistinguishable from a
stub that prints a reassuring sentence. **That is the shape this parcel exists to remove**, and it
would be self-refuting to ship the fix for it with the fix unproven.

`.github/workflows/ci.yml` is READ-ONLY to this parcel by its brief, so every mutation below is made
on a COPY of the workflow tree in a tempdir, and `ci-parity.py --root` is pointed at the copy. The
real workflow is never written. The copy is made with `git show`, from the committed blob, so the
baseline each mutation departs from is a committed object rather than whatever is on disk.

THE FOUR DRIFTS, and each is a thing that has happened to a checklist somewhere
==============================================================================

1. **A STEP IS ADDED.** CI grows a gate and the landing command never hears about it. This is the
   direction drift actually travels and the reason the map is derived rather than recalled.
2. **A STEP IS EDITED IN PLACE.** The name is unchanged and the gate is not. Measured in this repo:
   `docs/lane-log.jsonl` at `2026-09-16T03:26:06Z` records *"clippy exit 0"* from an invocation that
   had lost `-D warnings` and therefore could not fail — so the mutation used here is exactly that,
   dropping `-D warnings` from CI's clippy step.
3. **A STEP IS REMOVED.** The map is then describing a CI that is gone, which is the stale-copy
   defect this file's sibling spends its docstring on.
4. **AN UNKNOWN ACTION APPEARS.** A `uses:` step can be a gate (a scanner, a coverage floor). Waving
   one through because it is "just an action" is a judgement, so it is made explicit.

Plus a **GREEN ARM**: the unmutated copy must exit 0, or the four reds above prove only that the
tool reddens on everything.

Usage:  tools/run_lane_check_tests.sh  (the runner)  /  tools/test_ci_parity.py
"""

import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOOL = os.path.join(ROOT, "tools", "ci-parity.py")
WF = ".github/workflows/ci.yml"


def run(root):
    r = subprocess.run(
        [sys.executable, TOOL, "--root", root], capture_output=True, text=True
    )
    return r.returncode, r.stdout + r.stderr


def make_copy(tmp, text):
    d = os.path.join(tmp, "tree", ".github", "workflows")
    shutil.rmtree(os.path.join(tmp, "tree"), ignore_errors=True)
    os.makedirs(d)
    with open(os.path.join(d, "ci.yml"), "w", encoding="utf-8") as fh:
        fh.write(text)
    return os.path.join(tmp, "tree")


def main():
    baseline = subprocess.run(
        ["git", "-C", ROOT, "show", f"HEAD:{WF}"], capture_output=True, text=True
    )
    if baseline.returncode != 0:
        print(f"  FAIL  cannot read the committed {WF}; this proof is UNMEASURABLE")
        return 1
    base = baseline.stdout
    tmp = tempfile.mkdtemp(prefix="ci-parity-redfirst-")
    failures = []

    def case(name, text, want, story):
        root = make_copy(tmp, text)
        code, out = run(root)
        ok = (code == 1 and want in out) if want else (code == 0)
        print(f"  {'ok  ' if ok else 'FAIL'}  {name}: exit {code}" + (f"  [{want}]" if want else ""))
        if not ok:
            print("        " + story)
            print("\n".join("        " + l for l in out.strip().splitlines()[:8]))
            failures.append(name)

    # ---- GREEN ARM -------------------------------------------------------------------------
    case("GREEN ARM (unmutated copy of the committed ci.yml)", base, None,
         "the map must be current against the real workflow, or every red below is meaningless")

    # ---- 1. a step is ADDED ------------------------------------------------------------------
    added = base.replace(
        "      - name: Format\n        run: cargo fmt --all -- --check\n",
        "      - name: Format\n        run: cargo fmt --all -- --check\n"
        "      - name: Coverage floor\n        run: cargo llvm-cov --fail-under-lines 80\n",
        1,
    )
    assert added != base, "the anchor for mutation 1 no longer matches the workflow"
    case("1 STEP ADDED (a coverage floor CI would enforce and a landing would not)",
         added, "UNCLASSIFIED CI STEP",
         "a new CI gate must redden the next landing until someone classifies it")

    # ---- 2. a step is EDITED IN PLACE --------------------------------------------------------
    edited = base.replace(
        "        run: cargo clippy --all-targets -- -D warnings\n",
        "        run: cargo clippy --all-targets\n",
        1,
    )
    assert edited != base, "the anchor for mutation 2 no longer matches the workflow"
    case("2 STEP EDITED (`-D warnings` dropped — the real 2026-09-16 shape)",
         edited, "CI STEP CHANGED",
         "same label, different gate; a name-keyed map would call this covered")

    # ---- 3. a step is REMOVED ----------------------------------------------------------------
    removed = base.replace(
        "      - name: Corpus guards (name them in the log)\n"
        "        run: ./tools/ci-corpus-guards.sh\n",
        "",
        1,
    )
    assert removed != base, "the anchor for mutation 3 no longer matches the workflow"
    case("3 STEP REMOVED (the corpus guards step)", removed, "STALE MAP ENTRY",
         "a map describing a CI that is gone is the stale copy this tool exists to prevent")

    # ---- 4. an UNKNOWN ACTION ----------------------------------------------------------------
    action = base.replace(
        "      - uses: Swatinem/rust-cache@v2\n",
        "      - uses: Swatinem/rust-cache@v2\n"
        "      - uses: some-org/secret-scanner@v1\n",
        1,
    )
    assert action != base, "the anchor for mutation 4 no longer matches the workflow"
    case("4 UNKNOWN ACTION (a scanner that could be a gate)", action, "UNRECOGNISED ACTION",
         "an action is not waved through on the grounds that it is an action")

    print()
    if failures:
        print(f"test_ci_parity: {len(failures)} failure(s): {', '.join(failures)}")
        return 1
    print("test_ci_parity: green arm clean; all four drifts redden with the guard named.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
