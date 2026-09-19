#!/usr/bin/env python3
"""The landing gate's coverage map against CI — derived from `.github/workflows/`, never recalled.

WHY THIS EXISTS
===============

`tools/land.sh` is the landing command and it was believed to be a local rehearsal of CI. **It is
not, and the gap was measured three times on 2026-09-19** (`docs/2026-09-19-reds-without-a-cause.md`,
`docs/2026-09-19-reds-0909-cohort.md`):

1. A landing was green under `land.sh` (17 gates, 3027 passed) and **RED on CI** minutes later —
   `panel_attribution::tests::a_run_on_no_reported_row_...`, run `35418036061`. `land.sh` runs the
   suite in **release** (G7) and CI's `Test` step runs it in **debug**; the failing assertion only
   exists under `debug_assertions`. `land.sh`'s own header already said the profiles differ, and a
   prediction about CI was inferred from its green anyway.
2. `docs/lane-log.jsonl` at `2026-09-16T03:26:06Z` records *"clippy exit 0"* on a day CI failed on a
   clippy `nonminimal_bool`: the invocation that was run lacked `-D warnings`, so it could not fail.
3. `docs/lane-log.jsonl:184` records a real full-suite green from a command that cannot fail a clippy
   gate at all.

Every one of those is the same defect: **a gate believed to check something it does not.** The
expensive half (running everything CI runs) is priced in `land.sh`'s header. **This file is the
cheap half, and it is the one that cannot rot**: it reads the workflows and requires that every step
CI runs be CLASSIFIED here — run locally, subsumed by a local gate, or *named as not run*.

WHY IT MAPS THE STEPS RATHER THAN EXECUTING THEM
================================================

The obvious design is "derive `land.sh`'s command list from `ci.yml` so drift is impossible". It was
rejected on a reading of the actual steps, and the reason is in the inventory: `sudo apt-get install`,
`actions/cache/restore`, `echo "toolchain=$v" >> "$GITHUB_OUTPUT"` and `actions/checkout` are
**runner-environment** steps whose local execution is either meaningless or destructive. A derivation
that ran them verbatim would be wrong, and one that filtered them would need exactly the judgement
this table records.

So what is derived is the **obligation**, not the command: the set of steps that must be accounted
for. Adding a step to `ci.yml`, or editing one, reddens the next landing until a human says which of
the four things it is. That is drift-proof in the direction drift actually travels — CI grows a gate,
`land.sh` does not hear about it — while leaving the judgement where judgement belongs.

THE CLASSIFICATION VOCABULARY, and only the first two count as coverage
======================================================================

* `RUN`      — `land.sh` runs this same command, or invokes the same script. One code path.
* `STRONGER` — `land.sh` runs a variant that SUBSUMES it (e.g. `--workspace` where CI omits it).
               The difference is stated, so "stronger" is a claim a reader can check rather than
               a word.
* `DIFFERS`  — `land.sh` runs something RELATED that does **not** subsume it. This is the class that
               caused all three measured instances: release-vs-debug reads as coverage and is not.
               A `DIFFERS` row is a GAP and is printed in the landing report as one.
* `ABSENT`   — no local equivalent at all, with the reason.
* `SETUP`    — a runner-environment step that is not a gate (checkout, toolchain install, apt, cache).
               Named rather than filtered silently, because "it is only setup" is a judgement and the
               next reader is entitled to disagree with it.

⚑ `DIFFERS` and `ABSENT` are not failures of this tool. **A landing report that names the gates it
did not run is strictly better than one that implies it ran them all**, and this file is what makes
that sentence true rather than aspirational.

HOW A STEP IS KEYED, AND WHY EDITS REDDEN
=========================================

Key: `<workflow>:<job>/<step name>`. Step names repeat ACROSS jobs (`Read the declared Rust floor`
appears in three), so the job is part of the key; within a job they are unique.

Value carries a `sha` — the sha256 of the step's `run:` body. **An edited step therefore reddens**
even though its name is unchanged, which is the case a name-only map would miss: a step that gains
`--release`, or loses `-D warnings`, is a different gate wearing the same label. That is instance 2
above, in the other direction.

`uses:` steps are keyed the same way and matched against `KNOWN_ACTIONS` rather than pinned by
digest, because their body is a version tag we do not control. An unrecognised action reddens.

SCOPE: WHICH WORKFLOWS A LANDING CAN AFFECT
===========================================

A workflow is in scope if it is triggered by `push` or `pull_request` — those are the ones a landing
sets off. `.github/workflows/nightly-differential.yml` is `schedule` + `workflow_dispatch`, so a
landing cannot turn it red and it is reported as out of scope BY NAME. It is still enumerated: a
workflow that silently gained a `push:` trigger would otherwise become an unwatched gate.

USAGE
=====

    tools/ci-parity.py                 # the gaps, and the exit status (0 = the map is current)
    tools/ci-parity.py --report        # the full table, every step and its classification
    tools/ci-parity.py --gaps-only     # just the DIFFERS/ABSENT lines, for a report footer

Exit 0 when every in-scope step is classified and unchanged; 1 otherwise. **Exit 0 does NOT mean
`land.sh` runs what CI runs** — it means the difference is written down. Read the gaps.
"""

import argparse
import hashlib
import os
import sys

try:
    import yaml
except ImportError:  # pragma: no cover - the message is the whole value here
    sys.exit(
        "ci-parity: PyYAML is not installed, so the workflows cannot be read and this gate "
        "cannot say anything. `pip install pyyaml` (or apt install python3-yaml). Refusing "
        "rather than reporting an empty inventory, which would read as parity."
    )

WORKFLOW_DIR = ".github/workflows"

# Setup actions that carry no gate. An action outside this set reddens: a new `uses:` could be a
# gate (a security scanner, a coverage threshold) and we would not otherwise hear about it.
KNOWN_ACTIONS = {
    "actions/checkout@v4": "clone the tree; locally the tree IS the tree",
    "dtolnay/rust-toolchain@master": "install the declared floor; locally the installed toolchain is used",
    "Swatinem/rust-cache@v2": "runner build cache; locally target/ is the cache",
    "actions/cache/restore@v4": "runner corpus cache; locally vendor/ is fetched or symlinked once",
    "actions/cache/save@v4": "runner corpus cache; locally vendor/ is fetched or symlinked once",
}

# ---------------------------------------------------------------------------------------------
# THE MAP. One entry per `run:` step of every push-triggered workflow.
#
# `sha` is the first 12 hex of sha256 over the step's `run:` body, exactly as YAML yields it. When a
# step is edited this stops matching and the gate says so; re-read the step, decide what it is now,
# and update `sha` in the same edit as the words. Do NOT update `sha` alone — that is the one move
# that turns this file back into a stale copy.
# ---------------------------------------------------------------------------------------------
MAP = {
    # ---- determinism-gate ---------------------------------------------------------------------
    "ci.yml:determinism-gate/Read the declared Rust floor": dict(
        sha="45d764df8b06",
        kind="RUN",
        gate="G0b",
        note="land.sh runs ./tools/rust-floor.sh and refuses on an empty or failing read, which is "
        "the same failure this step has (`v=$(...)` under `bash -e`). Before 2026-09-19 nothing "
        "local ran it, so a broken floor declaration was a CI-only red.",
    ),
    "ci.yml:determinism-gate/Determinism gate + invariant proptests": dict(
        sha="8196c8670dd2",
        kind="RUN",
        gate="G6d",
        note="byte-identical command, debug profile, --nocapture. It is CI's most-guarded job "
        "(everything else `needs:` it), and it was the one CI job with no local counterpart at all.",
    ),
    # ---- build-test-lint ----------------------------------------------------------------------
    "ci.yml:build-test-lint/Read the declared Rust floor": dict(
        sha="45d764df8b06", kind="RUN", gate="G0b", note="same step, same script; see above."
    ),
    "ci.yml:build-test-lint/System libraries for the frontend/player build (alsa, udev)": dict(
        sha="331636b04a87",
        kind="SETUP",
        gate=None,
        note="`sudo apt-get install libasound2-dev libudev-dev`. A runner provisioning step; this "
        "workstation already carries both (`alsa-sys`/`libudev-sys` build here). land.sh must NOT "
        "run apt from a landing. If the local build ever fails on a missing .pc file, that is this "
        "step's local form and the fix is to install it once, not to gate on it.",
    ),
    "ci.yml:build-test-lint/Format": dict(
        sha="810e75f0d3de",
        kind="RUN",
        gate="G4",
        note="`cargo fmt --all -- --check` vs land.sh's `cargo fmt --all --check`; cargo forwards "
        "both spellings to the same rustfmt invocation. Profile-independent.",
    ),
    "ci.yml:build-test-lint/Clippy (deny warnings)": dict(
        sha="192f29c22f65",
        kind="RUN",
        gate="G5b",
        note="DEBUG clippy, `--all-targets -- -D warnings`. land.sh's G5 is the RELEASE variant and "
        "is NOT the same lint set: `cfg(debug_assertions)` code is compiled in one and out of the "
        "other, so a lint inside a debug-only block is invisible to G5. G5b runs CI's exact command "
        "and G5 stays as the stronger-in-the-other-direction release pass. Measured 2026-09-19: "
        "19 s on a warm tree.",
    ),
    "ci.yml:build-test-lint/Fetch vendored corpora (cache miss only)": dict(
        sha="187d20abe4cf",
        kind="ABSENT",
        gate=None,
        note="the three fetch scripts, ~726 MB and 838 HTTP requests. A landing must not re-download "
        "them; G1 instead requires vendor/ to be present and non-empty, and the step below "
        "re-verifies its bytes against the same pinned manifests the fetch scripts write. So what "
        "CI proves by fetching, a landing proves by verifying — see G1b.",
    ),
    "ci.yml:build-test-lint/Verify vendored corpora against their pinned manifests": dict(
        sha="7960477d461a",
        kind="RUN",
        gate="G1b",
        note="`./tools/verify-vendor.sh`, the same script, one code path. Until 2026-09-19 G1 only "
        "checked that the vendor directories were non-empty, which cannot see a truncated or "
        "wrong-pin corpus — and a worktree that symlinks a sibling's vendor/ inherits whatever that "
        "sibling last fetched. Measured: 2 s.",
    ),
    "ci.yml:build-test-lint/Name the frozen aeon pin": dict(
        sha="6e7185dab9bd",
        kind="DIFFERS",
        gate="G6b",
        note="CI names the pin in DEBUG; land.sh's G6b names it in RELEASE. Same test, same banner, "
        "different build. Left as DIFFERS rather than adding a fourth invocation: the gate's job is "
        "to NAME the chain and both do, and when --debug-suite runs, the debug suite executes this "
        "test as one of its legs. What is genuinely unrun in a release-only landing is the debug "
        "build of aeon_pin — 1 s, and it fails only if the pin data is unreadable, which the release "
        "run already proves.",
    ),
    "ci.yml:build-test-lint/Corpus guards (name them in the log)": dict(
        sha="6667c60ed428",
        kind="RUN",
        gate="G6c",
        note="`./tools/ci-corpus-guards.sh`, the same script. land.sh's G1 printed a note TELLING the "
        "reader they could run it and never ran it — the shape this whole parcel is about. It needs "
        "CI=1, which land.sh exports at G1. Measured: 7 s.",
    ),
    "ci.yml:build-test-lint/Test": dict(
        sha="ec4556a17852",
        kind="DIFFERS",
        gate="G7d",
        note="**THE MEASURED GAP.** `cargo test --workspace` in DEBUG. land.sh's G7 is the same "
        "selection in RELEASE, and release does not subsume debug in either direction: debug "
        "compiles `debug_assertions` code (which is how run 35418036061 went red on a landing that "
        "was green here) and release runs the three replay playthroughs that debug ignores. G7d runs "
        "CI's exact command and is ON BY DEFAULT; `--no-debug-suite` skips it and the report then "
        "names it as a gap rather than implying it ran.",
    ),
    # ---- replay-playthroughs --------------------------------------------------------------------
    "ci.yml:replay-playthroughs/Read the declared Rust floor": dict(
        sha="45d764df8b06", kind="RUN", gate="G0b", note="same step, same script; see above."
    ),
    "ci.yml:replay-playthroughs/Full replay playthroughs against the frozen pin": dict(
        sha="d161683ffc55",
        kind="STRONGER",
        gate="G7",
        note="`./tools/replay_playthroughs.sh` runs the three playthroughs in release. G7's "
        "`cargo test --workspace --release` RUNS those same three tests as ordinary legs — they are "
        "`#[cfg_attr(debug_assertions, ignore)]`, not `#[ignore]`, so a release workspace run "
        "executes them — alongside every other test in the workspace. The one thing the script adds "
        "is its pin-currency report, which is REPORT ONLY and cannot change an exit status (its own "
        "header says so), and G6b names the pin separately.",
    ),
}


def load_workflows(root):
    """Every workflow, with its triggers and its steps. Parsed, never grepped."""
    d = os.path.join(root, WORKFLOW_DIR)
    out = []
    if not os.path.isdir(d):
        return out
    for name in sorted(os.listdir(d)):
        if not name.endswith((".yml", ".yaml")):
            continue
        with open(os.path.join(d, name), encoding="utf-8") as fh:
            doc = yaml.safe_load(fh)
        if not isinstance(doc, dict):
            continue
        # YAML 1.1 reads a bare `on:` key as the boolean True. Both spellings, always.
        trig = doc.get("on", doc.get(True))
        if isinstance(trig, dict):
            triggers = sorted(trig.keys())
        elif isinstance(trig, list):
            triggers = sorted(str(t) for t in trig)
        elif trig is None:
            triggers = []
        else:
            triggers = [str(trig)]
        out.append((name, triggers, doc.get("jobs") or {}))
    return out


def digest(body):
    return hashlib.sha256(body.encode("utf-8")).hexdigest()[:12]


def audit(root):
    """-> (rows, findings). A row is (key, kind, gate, note); a finding is a sentence."""
    findings = []
    rows = []
    seen_keys = set()
    in_scope_files = []

    for wf, triggers, jobs in load_workflows(root):
        landing_triggers = [t for t in triggers if t in ("push", "pull_request")]
        if not landing_triggers:
            rows.append(
                (
                    f"{wf}",
                    "OUT-OF-SCOPE",
                    None,
                    f"triggers are {triggers or ['none']}; a landing cannot set this workflow off, "
                    "so it is not a landing gate. Enumerated anyway: a workflow that gained a "
                    "`push:` trigger would otherwise become an unwatched gate.",
                )
            )
            continue
        in_scope_files.append(wf)
        for jid, job in jobs.items():
            for st in job.get("steps") or []:
                name = st.get("name")
                if "run" in st:
                    key = f"{wf}:{jid}/{name}"
                    if name is None:
                        findings.append(
                            f"{wf}:{jid}: a `run:` step has no `name:`, so it cannot be keyed here. "
                            "Name it in ci.yml."
                        )
                        continue
                    seen_keys.add(key)
                    got = digest(st["run"])
                    entry = MAP.get(key)
                    if entry is None:
                        findings.append(
                            f"UNCLASSIFIED CI STEP {key!r} (body sha {got}). CI runs it and this map "
                            "does not say whether a landing does. Add an entry to tools/ci-parity.py "
                            "saying RUN / STRONGER / DIFFERS / ABSENT / SETUP and why — a landing "
                            "that cannot name a CI step cannot claim to rehearse CI."
                        )
                        continue
                    if entry["sha"] != got:
                        findings.append(
                            f"CI STEP CHANGED {key!r}: the map records body sha {entry['sha']} and "
                            f"the workflow now has {got}. The label is the same and the gate is not. "
                            "Re-read the step, re-decide its classification, and update BOTH the "
                            "words and the sha in the same edit."
                        )
                        continue
                    rows.append((key, entry["kind"], entry.get("gate"), entry["note"]))
                else:
                    uses = st.get("uses")
                    key = f"{wf}:{jid}/uses {uses}"
                    seen_keys.add(key)
                    if uses in KNOWN_ACTIONS:
                        rows.append((key, "SETUP", None, KNOWN_ACTIONS[uses]))
                    else:
                        findings.append(
                            f"UNRECOGNISED ACTION {uses!r} in {wf}:{jid}. An action can be a gate "
                            "(a scanner, a coverage floor), so it is not waved through. Add it to "
                            "KNOWN_ACTIONS with what it does, or classify it."
                        )

    stale = sorted(k for k in MAP if k not in seen_keys)
    for k in stale:
        findings.append(
            f"STALE MAP ENTRY {k!r}: this file classifies a CI step that no longer exists in the "
            "workflows. A map describing a CI that is gone is the copy problem this tool exists to "
            "prevent; delete the entry, or fix the key if the step was renamed."
        )
    return rows, findings


ORDER = {"DIFFERS": 0, "ABSENT": 1, "RUN": 2, "STRONGER": 3, "SETUP": 4, "OUT-OF-SCOPE": 5}


def main():
    ap = argparse.ArgumentParser(description="CI-step coverage map for tools/land.sh")
    ap.add_argument("--root", default=None, help="repo root (default: this script's parent)")
    ap.add_argument("--report", action="store_true", help="print every step, not only the gaps")
    ap.add_argument("--gaps-only", action="store_true", help="print only DIFFERS/ABSENT lines")
    args = ap.parse_args()

    root = args.root or os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    rows, findings = audit(root)

    gaps = [r for r in rows if r[1] in ("DIFFERS", "ABSENT")]

    if not args.gaps_only:
        shown = rows if args.report else gaps
        if args.report:
            shown = sorted(rows, key=lambda r: (ORDER.get(r[1], 9), r[0]))
            print("CI STEP INVENTORY — derived from .github/workflows, classified in tools/ci-parity.py")
            print("-" * 94)
        for key, kind, gate, note in shown:
            head = f"  {kind:<12} {key}"
            if gate:
                head += f"   [land.sh {gate}]"
            print(head)
            if args.report:
                for line in _wrap(note, 88):
                    print(f"                 {line}")
        if not args.report:
            print(f"  ({len(rows)} CI steps classified; {len(gaps)} are gaps)")
    else:
        for key, kind, gate, note in gaps:
            print(f"  {kind:<8} {key}" + (f"   [nearest local gate: {gate}]" if gate else ""))

    if findings:
        print()
        for f in findings:
            print(f"  BAD   {f}")
        print(f"ci-parity: {len(findings)} finding(s) — the map is NOT current")
        return 1
    print(
        f"ci-parity: the map is current ({len(rows)} steps classified, {len(gaps)} named as gaps). "
        "This says the difference is WRITTEN DOWN, not that there is none."
    )
    return 0


def _wrap(text, width):
    words, line, out = text.split(), "", []
    for w in words:
        if len(line) + len(w) + 1 > width:
            out.append(line)
            line = w
        else:
            line = f"{line} {w}".strip()
    if line:
        out.append(line)
    return out


if __name__ == "__main__":
    sys.exit(main())
