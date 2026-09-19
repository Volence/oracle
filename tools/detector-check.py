#!/usr/bin/env python3
"""Validate the METHOD DECLARATIONS an investigation doc carries — the three detectors.

WHY THIS EXISTS
===============

On 2026-09-19 this lane dispositioned 17 unexplained CI reds across two cohorts
(`docs/2026-09-19-reds-without-a-cause.md`, `docs/2026-09-19-reds-0909-cohort.md`) and, in the
course of it, measured three method defects **in its own work that night**:

1. **A survey that trusted its own empty result.** Three false absences in one night — a CI waiter
   on `--limit 8` whose SHA had fallen out of the window; a sweep regex reporting *"log expired"*
   for eight retrievable runs; a survey reporting **"logs gone"** for three runs carrying
   1683/1675/1692 lines. Every one was after *"an absence is never a finding"* had been banked.
2. **A heuristic reused where it did not apply.** Cohort 1 ruled INFRA = 0 partly because the two
   sibling CI jobs were green. Cohort 2 **checked instead of inheriting** and found it does not
   transfer: `ci.yml:63` at `35f81ca` puts the failing `apt-get` step in one job **and nowhere
   else**, so sibling greenness was uninformative.
3. **"That cannot be tested here", asserted twice and disproved.** A floor-toolchain lint was
   written off as unreproducible on this box; it reproduced **in twenty seconds on a two-file
   scratch crate** — and the tree had already corrected the same claim ten days earlier at
   `2999687`, *"correct my own diagnosis: there was no toolchain skew, I skipped clippy."*

All three were already banked in `docs/OVERSEER-REFERENCE.md` **as prose**, and prose is what they
were broken in. The measured lesson of that same night is that a structural check beat a documented
rule three times over (`tools/lane-check.py` now refuses state defects that prose did not prevent).

⚑ **So each habit gets a DETECTOR — a thing a reader can look for and FAIL TO FIND — rather than a
rule phrased as judgement.** *"Verify, don't adopt"* would have passed every artifact it was meant
to catch. A missing field does not.

THE ARTIFACT
============

An investigation doc carries one fenced block tagged ```detectors``, with at least one declaration
under each of the three keys. A declaration is either an explicit `none` with a reason, or a claim
line followed by indented fields:

    ```detectors
    ABSENCE: <the zero this document reports>
      instrument: <the exact invocation that produced the zero>
      positive:   <the SAME instrument, on a case known to contain the thing> -> <n>
      negative:   <the SAME instrument, on a case known to lack it> -> <n>
      scope:      <what the instrument enumerated, WITH ITS BOUNDARY>
      contains:   <how the subject is shown to lie inside that boundary>

    HEURISTIC: none — no heuristic was carried in from a prior investigation

    CANNOT-TEST: <the impossibility claim>
      attempted: <the command actually run> -> <result>
      cost:      <the MEASURED price of the cheapest attempt considered>
      prior:     <rev or docs path where this claim was made before, or `not-searched`>
    ```

**Why `scope` and `contains` exist, and why a control pair alone would not have caught habit 1.**
Of the three false absences, only two were extractor failures. The first was a WINDOW failure: a
`gh run list --limit 8` whose subject had fallen outside it. A positive control on a case inside the
window fires happily and the zero is still wrong. Cohort 2 got this right by a different move —
*"the window, proved rather than assumed"*: `--limit 500`, oldest run returned 2026-09-04, five days
older than the cohort. `scope`/`contains` is that move, made a field.

WHAT IS EXECUTABLE HERE, AND WHAT IS NOT
========================================

Enforced, mechanically:

* **Presence.** A doc declaring `**Kind:** investigation` must carry the block, with all three keys.
* **Shape.** Every declaration is one of the three keys; every field is a known field of that key;
  no key is missing a required field; no field is empty; a `none` answer carries a reason.
* **Polarity** — `positive` must end `-> n` with **n > 0** and `negative` with **n = 0**. A control
  that did not fire is not a control, and a control pasted with the wrong numbers is worse.
* **Same instrument** — `instrument`, `positive` and `negative` must share their leading command
  token. This is the check aimed at last night's actual failure: the zeros came out of
  `gh run list --limit 200`, and nothing that was ever controlled was that command.
* **The citation resolves** — `checked: <rev>:<path>:<line> "<text>"` is executed: the blob is read
  at that revision and `<text>` must occur on that line. A rotted or invented citation reddens.
* **`prior` resolves** — a revision must exist (`git cat-file -e`), a path must exist.

NOT enforced, and said plainly rather than implied (`--gaps` prints this list, and `land.sh` prints
it on every landing, for the same reason `lane-check.py --gaps` does):

* **Whether a doc that reports a zero declares it at all.** There is no prose parser here. The
  trigger is a doc SAYING `**Kind:** investigation`, which is self-selecting: delete the line and
  the gate has nothing to hold. Nothing in this repo detects an undeclared absence.
* **Whether the counts are true.** This re-runs nothing. `positive: ... -> 1` is checked for being a
  positive number, never for being the number that command returns.
* **Whether `assumes` names the real topology**, whether `cost` was measured rather than guessed,
  whether `attempted: none` was a defensible choice. Those are judgement, they are printed as notes
  so a reviewer meets them, and no tool here rules on them.

USAGE
=====

    tools/detector-check.py                       # every docs/*.md
    tools/detector-check.py docs/foo.md           # named files
    tools/detector-check.py --label "working tree"
    tools/detector-check.py --gaps                # what this gate does NOT check
"""

import argparse
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

FENCE_OPEN = re.compile(r"^```detectors\s*$")
FENCE_CLOSE = re.compile(r"^```\s*$")
# ⚑ ANCHORED TO THE START OF A LINE, and that is not cosmetic: the first draft matched anywhere and
#   immediately fired on `docs/OVERSEER-REFERENCE.md`, which MENTIONS `**Kind:** investigation` inside
#   backticks while documenting this very trigger. A marker that cannot be quoted is a marker that
#   punishes the file explaining it. The real ones sit in a doc's front-matter stanza, at column 0.
KIND_INVESTIGATION = re.compile(r"^\*\*Kind:\*\*\s*investigation", re.IGNORECASE | re.MULTILINE)

KEY_RE = re.compile(r"^(ABSENCE|HEURISTIC|CANNOT-TEST):\s*(.*)$")
FIELD_RE = re.compile(r"^\s{2,}([a-z][a-z-]*):\s*(.*)$")
COUNT_RE = re.compile(r"->\s*(\d+)\s*$")
# `checked: <rev>:<path>:<line> "<text>"`
CITE_RE = re.compile(r'^([0-9a-fA-F]{7,40}):(\S+):(\d+)\s+"(.+)"\s*$')
REV_RE = re.compile(r"^[0-9a-fA-F]{7,40}$")

# A `none` answer: the word, then a dash, then a reason. The reason is the point — an unadorned
# `none` is the judgement-shaped answer this whole file exists to refuse.
NONE_RE = re.compile(r"^none\s*[—\-–:]\s*(.{10,})$", re.IGNORECASE)

# Answers that are honest non-answers inside a field. They pass shape and are PRINTED as notes.
UNATTEMPTED = re.compile(
    r"^(none|not[- ](searched|run|measured|checked|done)|unknown)\b", re.IGNORECASE
)

REQUIRED = {
    "ABSENCE": ("instrument", "positive", "negative", "scope", "contains"),
    "HEURISTIC": ("from", "assumes", "checked"),
    "CANNOT-TEST": ("attempted", "cost", "prior"),
}

# Contract rules that are real and deliberately NOT enforced here. Printed by `--gaps` and by
# land.sh, because a green from this tool bounds only what this tool checks.
GAPS = [
    "a doc that reports a zero and does NOT say `**Kind:** investigation` is never asked for a block",
    "the counts in `positive`/`negative` are checked for polarity, never re-run",
    "`scope`/`contains` are checked for being non-empty, never for bounding the subject",
    "`assumes` is not checked against the workflow it describes; only `checked` is executed",
    "`cost` is not checked for being measured rather than guessed",
    "`attempted: none` passes shape; it is printed as a note and no tool rules on it",
]


def git(*args):
    """Run git in ROOT. Returns (rc, stdout)."""
    p = subprocess.run(
        ["git", "-C", ROOT, *args], capture_output=True, text=True, errors="replace"
    )
    return p.returncode, p.stdout


def lead_token(value):
    """The leading command token of an invocation, with any `-> n` tail removed."""
    v = COUNT_RE.sub("", value).strip()
    # Strip a leading `$ ` prompt if someone pastes one.
    if v.startswith("$ "):
        v = v[2:].strip()
    return v.split()[0] if v.split() else ""


def parse_block(lines, start, findings, where):
    """Parse the declarations inside one ```detectors fence. Returns a list of declarations."""
    decls = []
    cur = None
    for off, raw in enumerate(lines[start:], start=start):
        line = raw.rstrip("\n")
        if FENCE_CLOSE.match(line):
            return decls, off
        if not line.strip():
            continue
        m = KEY_RE.match(line)
        if m:
            cur = dict(key=m.group(1), claim=m.group(2).strip(), fields={}, line=off + 1)
            decls.append(cur)
            continue
        m = FIELD_RE.match(line)
        if m:
            if cur is None:
                findings.append(
                    f"{where}:{off + 1}: a field before any of "
                    f"ABSENCE/HEURISTIC/CANNOT-TEST: {line.strip()!r}"
                )
                continue
            name, value = m.group(1), m.group(2).strip()
            if name in cur["fields"]:
                findings.append(
                    f"{where}:{off + 1}: {cur['key']} names `{name}` twice; "
                    f"a field answered twice is a field nobody reads"
                )
            cur["fields"][name] = (value, off + 1)
            continue
        findings.append(
            f"{where}:{off + 1}: not a declaration and not a field: {line.strip()!r} "
            f"(a declaration is `ABSENCE:`/`HEURISTIC:`/`CANNOT-TEST:`; a field is two-space "
            f"indented `name: value`)"
        )
    findings.append(f"{where}:{start}: the ```detectors block is never closed")
    return decls, len(lines)


def check_absence(d, where, findings, notes):
    f = d["fields"]
    tokens = {}
    for name in ("instrument", "positive", "negative"):
        if name not in f:
            continue
        value, ln = f[name]
        tokens[name] = lead_token(value)
        if name == "instrument":
            continue
        m = COUNT_RE.search(value)
        if not m:
            findings.append(
                f"{where}:{ln}: ABSENCE `{name}` does not end in `-> <count>`; a control "
                f"without the count it returned is a claim that a control was run"
            )
            continue
        n = int(m.group(1))
        if name == "positive" and n == 0:
            findings.append(
                f"{where}:{ln}: ABSENCE `positive` returned 0 — THE CONTROL DID NOT FIRE, "
                f"so the zero being reported is not evidence of anything"
            )
        if name == "negative" and n != 0:
            findings.append(
                f"{where}:{ln}: ABSENCE `negative` returned {n}, not 0 — an instrument that "
                f"fires on a case known to LACK the thing cannot distinguish absence from noise"
            )
    have = {k: v for k, v in tokens.items() if v}
    if len(have) == 3 and len(set(have.values())) != 1:
        findings.append(
            f"{where}:{d['line']}: ABSENCE controls a DIFFERENT INSTRUMENT than the one that "
            f"produced the zero — instrument `{tokens['instrument']}`, positive "
            f"`{tokens['positive']}`, negative `{tokens['negative']}`. This is the exact shape of "
            f"2026-09-19: the zeros came out of `gh run list --limit 200` and the controls were greps."
        )


def check_heuristic(d, where, findings, notes):
    f = d["fields"]
    if "checked" not in f:
        return
    value, ln = f["checked"]
    if UNATTEMPTED.match(value):
        findings.append(
            f"{where}:{ln}: HEURISTIC `checked` is {value!r} — a borrowed heuristic whose "
            f"topology was not checked in THIS investigation is the 2026-09-19 defect verbatim"
        )
        return
    m = CITE_RE.match(value)
    if not m:
        findings.append(
            f"{where}:{ln}: HEURISTIC `checked` must be `<rev>:<path>:<line> \"<text>\"` so it can "
            f"be executed; got {value!r}"
        )
        return
    rev, path, lineno, text = m.group(1), m.group(2), int(m.group(3)), m.group(4)
    rc, out = git("show", f"{rev}:{path}")
    if rc != 0:
        findings.append(
            f"{where}:{ln}: HEURISTIC `checked` cites `{rev}:{path}`, which does not resolve "
            f"in this repository"
        )
        return
    body = out.splitlines()
    if lineno < 1 or lineno > len(body):
        findings.append(
            f"{where}:{ln}: HEURISTIC `checked` cites line {lineno} of `{rev}:{path}`, "
            f"which has {len(body)} lines"
        )
        return
    if text not in body[lineno - 1]:
        findings.append(
            f"{where}:{ln}: HEURISTIC `checked` quotes {text!r} at `{rev}:{path}:{lineno}`, "
            f"but that line reads {body[lineno - 1].strip()!r}"
        )


def check_cannot_test(d, where, findings, notes):
    f = d["fields"]
    if "attempted" in f:
        value, ln = f["attempted"]
        if UNATTEMPTED.match(value):
            notes.append(
                f"{where}:{ln}: CANNOT-TEST `attempted` is {value!r} — an impossibility claim with "
                f"no attempt behind it. Shape passes; nothing here rules on it."
            )
        elif not COUNT_RE.search(value) and "->" not in value:
            findings.append(
                f"{where}:{ln}: CANNOT-TEST `attempted` names no result; write "
                f"`<command> -> <what it did>` so a reader can tell an attempt from an intention"
            )
    if "cost" in f:
        value, ln = f["cost"]
        if UNATTEMPTED.match(value):
            notes.append(
                f"{where}:{ln}: CANNOT-TEST `cost` is {value!r} — the price that decided it was "
                f"asserted, not measured."
            )
    if "prior" in f:
        value, ln = f["prior"]
        token = value.split()[0].rstrip(",;") if value.split() else ""
        if UNATTEMPTED.match(value):
            notes.append(
                f"{where}:{ln}: CANNOT-TEST `prior` is {value!r} — this lane's own record was not "
                f"searched for an earlier statement of the same claim."
            )
        elif REV_RE.match(token):
            rc, _ = git("cat-file", "-e", f"{token}^{{commit}}")
            if rc != 0:
                findings.append(
                    f"{where}:{ln}: CANNOT-TEST `prior` names revision `{token}`, which does not "
                    f"exist in this repository"
                )
        elif "/" in token or token.endswith(".md"):
            if not os.path.exists(os.path.join(ROOT, token)):
                findings.append(
                    f"{where}:{ln}: CANNOT-TEST `prior` names path `{token}`, which does not exist"
                )
        else:
            findings.append(
                f"{where}:{ln}: CANNOT-TEST `prior` must be a revision, a repo path, or "
                f"`not-searched — <why>`; got {value!r}"
            )


CHECKERS = {
    "ABSENCE": check_absence,
    "HEURISTIC": check_heuristic,
    "CANNOT-TEST": check_cannot_test,
}


def check_doc(path, findings, notes):
    where = os.path.relpath(path, ROOT)
    try:
        with open(path, "r", encoding="utf-8") as fh:
            lines = fh.read().splitlines()
    except OSError as exc:
        findings.append(f"{where}: cannot be read: {exc}")
        return 0

    body = "\n".join(lines)
    is_investigation = bool(KIND_INVESTIGATION.search(body))

    decls = []
    i = 0
    blocks = 0
    while i < len(lines):
        if FENCE_OPEN.match(lines[i]):
            blocks += 1
            got, end = parse_block(lines, i + 1, findings, where)
            decls.extend(got)
            i = end + 1
            continue
        i += 1

    if blocks == 0:
        if is_investigation:
            findings.append(
                f"{where}: says `**Kind:** investigation` and carries no ```detectors block. "
                f"The three method declarations (ABSENCE, HEURISTIC, CANNOT-TEST) are required of "
                f"an investigation — see docs/OVERSEER-REFERENCE.md, The bars."
            )
        return 0
    if blocks > 1:
        findings.append(
            f"{where}: {blocks} ```detectors blocks. One artifact, not two that disagree."
        )

    seen = set()
    for d in decls:
        key = d["key"]
        seen.add(key)
        m = NONE_RE.match(d["claim"])
        if m:
            if d["fields"]:
                findings.append(
                    f"{where}:{d['line']}: {key} answers `none` and then carries fields; "
                    f"it is one or the other"
                )
            notes.append(f"{where}:{d['line']}: {key} declared none — {m.group(1)}")
            continue
        if d["claim"].lower().startswith("none"):
            findings.append(
                f"{where}:{d['line']}: {key} answers `none` with no reason. `none` is a claim; "
                f"write `none — <why>`."
            )
            continue
        if not d["claim"]:
            findings.append(f"{where}:{d['line']}: {key} states no claim")
        required = REQUIRED[key]
        for name in required:
            if name not in d["fields"]:
                findings.append(
                    f"{where}:{d['line']}: {key} is missing `{name}` "
                    f"(required: {', '.join(required)})"
                )
            elif not d["fields"][name][0]:
                findings.append(f"{where}:{d['fields'][name][1]}: {key} `{name}` is empty")
        for name, (_, ln) in d["fields"].items():
            if name not in required:
                findings.append(
                    f"{where}:{ln}: {key} has no field `{name}` "
                    f"(known: {', '.join(required)})"
                )
        CHECKERS[key](d, where, findings, notes)

    for key in REQUIRED:
        if key not in seen:
            findings.append(
                f"{where}: the ```detectors block never declares {key}. All three are answered or "
                f"the block is silent about a habit that has already cost this lane a night."
            )
    return len(decls)


def print_gaps():
    for g in GAPS:
        print(f"detector-check does NOT check: {g}")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("paths", nargs="*", help="markdown files; default every docs/*.md")
    ap.add_argument("--label", default="", help="a label for the summary line")
    ap.add_argument("--gaps", action="store_true", help="print what this tool does NOT check")
    ap.add_argument("--quiet-notes", action="store_true", help="suppress note lines")
    args = ap.parse_args()

    if args.gaps:
        print_gaps()
        return 0

    paths = args.paths
    if not paths:
        docs = os.path.join(ROOT, "docs")
        paths = sorted(
            os.path.join(docs, n) for n in os.listdir(docs) if n.endswith(".md")
        )

    findings, notes = [], []
    decls = 0
    for p in paths:
        decls += check_doc(p, findings, notes)

    if not args.quiet_notes:
        for n in notes:
            print(f"note: {n}")
    for f in findings:
        print(f"detector-check: {f}")

    label = f" ({args.label})" if args.label else ""
    if findings:
        print(
            f"detector-check{label}: {len(findings)} finding(s) over {len(paths)} doc(s), "
            f"{decls} declaration(s), {len(notes)} note(s)"
        )
        return 1
    print(
        f"detector-check{label}: clean — {len(paths)} doc(s), {decls} declaration(s), "
        f"{len(notes)} note(s)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
