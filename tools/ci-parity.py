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
* `CONDITIONAL` — `land.sh` runs this same command, but a flag can skip it. It counts as coverage
               only on a run that took it, so `--gaps-only --ran <gates>` is how a report says
               which way this one went. The report must never state it statically.
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

⚑ **AND THE CLAIM IN THE OTHER DIRECTION IS CHECKED TOO, because otherwise this file is itself the
copy it exists to prevent.** A `RUN` or `CONDITIONAL` row asserts *`land.sh` runs this*. Nothing
made that true: someone could drop `-D warnings` from `land.sh`'s clippy gate and this map would go
on saying `RUN`, which is instance 2 of the measured defect pointed the other way and committed
inside the instrument built to catch it. So each such row carries `local` — the exact text that must
appear in `tools/land.sh` — and it is searched **outside comment lines only**, because this file's
sibling quotes every one of these commands in its header and a comment must not be able to satisfy
a proof obligation.

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
    tools/ci-parity.py --gaps-only     # just the gap lines, for a report footer
    tools/ci-parity.py --gaps-only --ran D5   # ... treating D5's CONDITIONAL row as covered

Exit 0 when every in-scope step is classified and unchanged; 1 otherwise. **Exit 0 does NOT mean
`land.sh` runs what CI runs** — it means the difference is written down. Read the gaps.
"""

import argparse
import hashlib
import os
import re
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
        local="./tools/rust-floor.sh",
        note="land.sh runs ./tools/rust-floor.sh and refuses on an empty or failing read, which is "
        "the same failure this step has (`v=$(...)` under `bash -e`). Before 2026-09-19 nothing "
        "local ran it, so a broken floor declaration was a CI-only red.",
    ),
    "ci.yml:determinism-gate/Determinism gate + invariant proptests": dict(
        sha="8196c8670dd2",
        kind="RUN",
        gate="D3",
        local="cargo test -p oracle-core --test determinism_gate --test proptests -- --nocapture",
        note="byte-identical command, debug profile, --nocapture. It is CI's most-guarded job "
        "(everything else `needs:` it), and it was the one CI job with no local counterpart at all.",
    ),
    # ---- build-test-lint ----------------------------------------------------------------------
    "ci.yml:build-test-lint/Read the declared Rust floor": dict(
        sha="45d764df8b06", kind="RUN", gate="G0b", local="./tools/rust-floor.sh",
        note="same step, same script; see above."
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
        local="cargo fmt --all --check",
        note="`cargo fmt --all -- --check` vs land.sh's `cargo fmt --all --check`; cargo forwards "
        "both spellings to the same rustfmt invocation. Profile-independent.",
    ),
    "ci.yml:build-test-lint/Clippy (deny warnings)": dict(
        sha="192f29c22f65",
        kind="RUN",
        gate="D1",
        local="cargo clippy --all-targets -- -D warnings",
        note="DEBUG clippy, `--all-targets -- -D warnings`. land.sh's G5 is the RELEASE variant and "
        "is NOT the same lint set: `cfg(debug_assertions)` code is compiled in one and out of the "
        "other, so a lint inside a debug-only block is invisible to G5. D1 runs CI's exact command "
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
        local="./tools/verify-vendor.sh",
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
        "to NAME the chain and both do, and when the debug arm runs, D5 executes this "
        "test as one of its legs. What is genuinely unrun in a release-only landing is the debug "
        "build of aeon_pin — 1 s, and it fails only if the pin data is unreadable, which the release "
        "run already proves.",
    ),
    "ci.yml:build-test-lint/Corpus guards (name them in the log)": dict(
        sha="6667c60ed428",
        kind="RUN",
        gate="D2",
        local="./tools/ci-corpus-guards.sh",
        note="`./tools/ci-corpus-guards.sh`, the same script. land.sh's G1 printed a note TELLING the "
        "reader they could run it and never ran it — the shape this whole parcel is about. It needs "
        "CI=1, which land.sh exports at G1. Measured: 7 s.",
    ),
    "ci.yml:build-test-lint/Test": dict(
        sha="ec4556a17852",
        kind="CONDITIONAL",
        gate="D5",
        local="cargo test --workspace 2>&1",
        note="**THE MEASURED GAP.** `cargo test --workspace` in DEBUG. land.sh's G7 is the same "
        "selection in RELEASE, and release does not subsume debug in either direction: debug "
        "compiles `debug_assertions` code (which is how run 35418036061 went red on a landing that "
        "was green here) and release runs the three replay playthroughs that debug ignores. D5 runs "
        "CI's exact command and is ON BY DEFAULT; `--no-debug-suite` skips it and the report then "
        "names it as a gap rather than implying it ran.",
    ),
    # ---- replay-playthroughs --------------------------------------------------------------------
    "ci.yml:replay-playthroughs/Read the declared Rust floor": dict(
        sha="45d764df8b06", kind="RUN", gate="G0b", local="./tools/rust-floor.sh",
        note="same step, same script; see above."
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


def trigger_map(doc):
    """`{trigger: its config or None}` for one parsed workflow document.

    The one place a workflow's `on:` is read. `tools/ci-verdict.py` imports it to decide which
    workflows a push must have run, so this tool and that one cannot disagree about what "push-
    triggered" means. The config is kept (not just the key) because a `push:` with `branches:` or
    `paths:` filters may legitimately not run, and the verdict tool has to be able to say so.
    """
    # YAML 1.1 reads a bare `on:` key as the boolean True. Both spellings, always.
    trig = doc.get("on", doc.get(True))
    if isinstance(trig, dict):
        return {str(k): v for k, v in trig.items()}
    if isinstance(trig, list):
        return {str(t): None for t in trig}
    if trig is None:
        return {}
    return {str(trig): None}


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
        triggers = sorted(trigger_map(doc).keys())
        out.append((name, triggers, doc.get("jobs") or {}))
    return out


def digest(body):
    return hashlib.sha256(body.encode("utf-8")).hexdigest()[:12]


_LAND_CODE = None


# Lines that DISPLAY text rather than run it. Both of these were measured to defeat the check
# before they were stripped: land.sh's header quotes every command it runs, at length, and each
# gate also `echo`s its own command as a banner. **A quotation must not be able to discharge a
# proof that the command is RUN** — the first draft of this check passed a land.sh whose clippy
# gate had lost `-D warnings`, because the banner one line above still said the words. That is the
# exact defect this file exists to catch, committed inside the catcher, and it was found by
# mutating land.sh rather than by reading the filter.
_DISPLAY = re.compile(r"^\s*(hr;\s*)?(echo|printf|note|pass|fail|command\s+echo)\b")


def _land_code(root):
    """tools/land.sh with comment and display lines removed.

    ⚑ This is a FILTER OVER TEXT, not a parse of shell. It cannot tell a command inside a dead
    `if false` branch from a live one, and a command spelled across a line continuation will not
    match. Both failures are in the safe direction — a false RED that a human resolves by looking —
    and neither is silent. What it does buy is that the `RUN` claim cannot be discharged by prose.
    """
    global _LAND_CODE
    if _LAND_CODE is None:
        try:
            with open(os.path.join(root, "tools", "land.sh"), encoding="utf-8") as fh:
                _LAND_CODE = "\n".join(
                    l
                    for l in fh.read().splitlines()
                    if not l.lstrip().startswith("#") and not _DISPLAY.match(l)
                )
        except OSError:
            _LAND_CODE = ""
    return _LAND_CODE


def _local_missing(root, entry):
    """The text a RUN/CONDITIONAL row promises land.sh contains, if it is absent."""
    if entry["kind"] not in ("RUN", "CONDITIONAL"):
        return None
    want = entry.get("local")
    if not want:
        return "<no `local` declared, so the RUN claim is unprovable>"
    return None if want in _land_code(root) else want


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
                    missing = _local_missing(root, entry)
                    if missing:
                        findings.append(
                            f"MAP CLAIMS MORE THAN land.sh DOES for {key!r}: it is classified "
                            f"{entry['kind']} at gate {entry.get('gate')}, but {missing!r} does not "
                            "appear on any non-comment line of tools/land.sh. Either the gate was "
                            "changed and the map was not, or the classification was wrong when it "
                            "was written. A map that asserts coverage it cannot see is the copy "
                            "this file exists to prevent."
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


ORDER = {"DIFFERS": 0, "CONDITIONAL": 1, "ABSENT": 2, "RUN": 3, "STRONGER": 4, "SETUP": 5,
         "OUT-OF-SCOPE": 6}


def main():
    ap = argparse.ArgumentParser(description="CI-step coverage map for tools/land.sh")
    ap.add_argument("--root", default=None, help="repo root (default: this script's parent)")
    ap.add_argument("--report", action="store_true", help="print every step, not only the gaps")
    ap.add_argument("--gaps-only", action="store_true", help="print only the gap lines")
    ap.add_argument(
        "--ran",
        default="",
        metavar="GATES",
        help="comma-separated gate ids that DID run on this landing; a CONDITIONAL row whose gate "
        "is named here is covered and is left out of the gap list. Without it every CONDITIONAL "
        "row counts as a gap, which is the safe direction: a report may understate its coverage "
        "and must never overstate it.",
    )
    args = ap.parse_args()
    ran = {g.strip() for g in args.ran.split(",") if g.strip()}

    root = args.root or os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    rows, findings = audit(root)

    gaps = [
        r
        for r in rows
        if r[1] in ("DIFFERS", "ABSENT")
        or (r[1] == "CONDITIONAL" and r[2] not in ran)
    ]

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
    # `--gaps-only` is consumed verbatim by `land.sh` into its report footer and its VERDICT file,
    # so it emits the gap lines and NOTHING else. A summary line appended there arrives in the
    # VERDICT as a bogus `ci_gap=` row, which is a machine-read artifact claiming a gap that is not
    # one — small, and exactly the class of untruth this parcel is about.
    if not args.gaps_only:
        print(
            f"ci-parity: the map is current ({len(rows)} steps classified, {len(gaps)} named as "
            "gaps). This says the difference is WRITTEN DOWN, not that there is none."
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
