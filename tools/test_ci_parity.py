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

THE SEVEN DRIFTS, in two directions, and each is a thing that has happened to a checklist somewhere
==================================================================================================

Drifts 1 to 4 are **CI moving away from `land.sh`**. Drifts 5 to 7 are **`land.sh` moving away from
its own map** — the direction a first draft of this file did not test, and the one where the map
becomes the very thing it exists to prevent: a written claim of coverage that nothing checks.
⚑ **Mutation 5 caught a real hole rather than confirming a design.** The `local` proof first
searched land.sh with only comment lines stripped, and a land.sh whose clippy gate had silently
lost `-D warnings` still PASSED, because the gate `echo`s its own command as a banner one line
above. Display lines are stripped now. It was found by running the mutation, not by reading the
filter.


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

5. **`land.sh` STOPS RUNNING A STEP THE MAP SAYS IT RUNS** — `-D warnings` dropped from the debug
   clippy gate, which is measured instance 2 pointed the other way.
6. **A WHOLE SCRIPT GATE GOES** — `ci-corpus-guards.sh` no longer invoked, while the header still
   describes it at length. This is the likeliest real shape: the prose outlives the call.
7. **THE VENDOR VERIFICATION GOES** — `verify-vendor.sh` no longer invoked.

Plus a **GREEN ARM**: the unmutated copy must exit 0, or the reds above prove only that the tool
reddens on everything.

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


def make_copy(tmp, text, land=None):
    """A scratch tree with a workflow and a land.sh. Neither real file is ever written.

    `land.sh` is copied in because `ci-parity.py` now proves its RUN claims against it; a tree
    without one would fail every RUN row for want of a file, which would look like seven findings
    and be one missing fixture.
    """
    root = os.path.join(tmp, "tree")
    shutil.rmtree(root, ignore_errors=True)
    d = os.path.join(root, ".github", "workflows")
    os.makedirs(d)
    os.makedirs(os.path.join(root, "tools"))
    with open(os.path.join(d, "ci.yml"), "w", encoding="utf-8") as fh:
        fh.write(text)
    if land is None:
        land = subprocess.run(
            ["git", "-C", ROOT, "show", "HEAD:tools/land.sh"], capture_output=True, text=True
        ).stdout
    with open(os.path.join(root, "tools", "land.sh"), "w", encoding="utf-8") as fh:
        fh.write(land)
    return root


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

    def case(name, text, want, story, land=None):
        root = make_copy(tmp, text, land)
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

    # ---- 5-7. land.sh DRIFTS AWAY FROM ITS OWN MAP -------------------------------------------
    base_land = subprocess.run(
        ["git", "-C", ROOT, "show", "HEAD:tools/land.sh"], capture_output=True, text=True
    ).stdout
    if not base_land:
        print("  FAIL  cannot read the committed tools/land.sh; drifts 5-7 are UNMEASURABLE")
        failures.append("land.sh drift cases unmeasurable")
        base_land = None

    land_mutations = [
        (
            "5 land.sh LOSES `-D warnings` from the debug clippy gate",
            'if cargo clippy --all-targets -- -D warnings > "$RUN_DIR/clippy-debug.log" 2>&1; then',
            'if cargo clippy --all-targets > "$RUN_DIR/clippy-debug.log" 2>&1; then',
            "cargo clippy --all-targets -- -D warnings",
        ),
        (
            "6 land.sh STOPS INVOKING ci-corpus-guards.sh (its header still describes it)",
            'if ./tools/ci-corpus-guards.sh > "$RUN_DIR/corpus-guards.log" 2>&1; then',
            "if true; then",
            "./tools/ci-corpus-guards.sh",
        ),
        (
            "7 land.sh STOPS INVOKING verify-vendor.sh",
            'if ./tools/verify-vendor.sh > "$RUN_DIR/verify-vendor.log" 2>&1; then',
            "if true; then",
            "./tools/verify-vendor.sh",
        ),
    ]
    for name, old_s, new_s, want_text in land_mutations:
        if base_land is None:
            break
        if old_s not in base_land:
            print(f"  FAIL  {name}: the anchor no longer matches tools/land.sh; UNMEASURABLE")
            failures.append(f"{name}: anchor stale")
            continue
        # The header still QUOTES the command; that is the point of the case.
        mutated = base_land.replace(old_s, new_s, 1)
        assert want_text in mutated, "the fixture must keep the prose and lose the call"
        root = make_copy(tmp, base, mutated)
        code, out = run(root)
        if code == 1 and "MAP CLAIMS MORE THAN land.sh DOES" in out and want_text in out:
            print(f"  ok    {name}: exit 1  [MAP CLAIMS MORE THAN land.sh DOES]")
        else:
            print(f"  FAIL  {name}: exit {code}\n" + "\n".join("        " + l for l in out.strip().splitlines()[:8]))
            failures.append(name)

    print()
    if failures:
        print(f"test_ci_parity: {len(failures)} failure(s): {', '.join(failures)}")
        return 1
    print("test_ci_parity: green arm clean; all seven drifts redden with the guard named — four "
          "where CI moves, three where land.sh does.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
