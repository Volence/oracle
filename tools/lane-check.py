#!/usr/bin/env python3
"""Validate the lane files the Dominion console reads.

WHY THIS EXISTS
===============

`docs/lane-status.json` and `docs/lane-log.jsonl` are this lane's board and its history, and they are
parsed by a console that is not in this repo. Until this file **nothing in this repo validated them**:
`git grep` finds both names in exactly two places, `crates/oracle-aether/tests/hosted.rs` and
`src/server.rs`, and both are doc-comment mentions. So a malformed entry lands clean, and the way it is
discovered is the owner's card going dark.

That has happened. `docs/OVERSEER-REFERENCE.md` records it twice: a queue row marked `"state": "done"`,
which is not in the vocabulary, and **one bad enum rejects the WHOLE document**, so every true thing in
it went dark with the bad word. Three lanes across the suite wrote `done` in three days. Nothing on this
side can see it: the file writes fine and `git` is happy.

WHAT IT CHECKS, AND WHERE EACH RULE COMES FROM
=============================================

* **Every line of the log parses as a JSON object.** One unparsable line is the whole file to a reader
  that loads it strictly.
* **`at`, `headline`, `matters` on every log entry.** Derived from the corpus rather than invented: all
  137 entries at the revision this was written carry exactly those three, plus `detail` on 133 and
  `refs`/`project` on a few. `detail` is therefore NOT required.
* **The status document's eight top-level keys.** Stable across every one of the last twelve revisions
  that touched the file.
* **Every queue row carries `id`, `title` and `state`, and `state` is one of `doing | next | open |
  blocked`.** ⚑ The vocabulary is the CONTRACT's (`empyrean contract/LANE_STATUS.md`) and this repo's
  copy of it is a precedent narrative in `docs/OVERSEER-REFERENCE.md`. It is written out here rather
  than read from a sibling working tree, because a gate that reads a peer's live checkout measures
  whatever that peer happens to be editing. **On any disagreement the contract wins and this list is
  what changes.**
* **No future `updatedAt` and no future `at`.** A timestamp ahead of now is a typo'd year or a wrong
  clock, and either way the board is claiming to know something it cannot.

THE THIRD FILE: `docs/decisions.jsonl`
======================================

Added 2026-09-06 on the hub's ruling, in the same breath that put the file on `tools/land.sh`'s fast
path. **It is this validator's scope rather than a carve-out**, and that framing is the reason the fast
path was allowed to widen at all: a file that skips the suite has to be checked by something, and this
is the something.

The required keys are derived from the corpus, exactly as the log's three were: all 33 entries at the
revision this was written carry `id`, `at`, `question`, `options` and `recommend`. `supersedes` is on 32,
`detail` on 31, `refs` on 23, `answered` on 4 and `because` on 2, so none of those five is required.

⚑ **Two checks here are not shape checks, and they exist because of a defect that had already happened.**
The hub filed two cards an hour before this landed, stamped them `d-31` and `d-32` by assuming the next
free number instead of measuring, and both ids were already taken. The new `d-32` then declared
`supersedes: "d-31"` while **`d-31` named two different cards**, so a rotated session following the
supersede chain would have landed on yesterday's card about something else entirely.

* **Every `id` appears exactly once.** The file is append-only and its ids are its only handles, so a
  duplicate is silent at write time and survives every other check here. The finding names both lines.
* **Every non-null `supersedes` names an id that exists, and never the card's own.** A chain that leads
  nowhere is worse than no chain: a reader follows it and stops, and cannot tell a missing card from a
  typo.

**The failure is invisible to the person who causes it**, which is the whole argument for putting it in a
gate instead of in somebody's habits.

It is deliberately stricter than "the console will accept it" in three places (`size`, duplicate queue
ids, and the two rules above): these are our own files, being stricter about them costs nothing, and the
failure it prevents is one nobody at this seat can see.

USAGE
=====

    tools/lane-check.py --status docs/lane-status.json --log docs/lane-log.jsonl \
                        --decisions docs/decisions.jsonl
    tools/lane-check.py --status <file> --label "as committed at <sha>"

Any argument may be omitted to check only the others. Exit 0 when clean, 1 when anything is wrong;
every finding is printed with the file and, where there is one, the line or row it is about.
"""

import argparse
import datetime as dt
import json
import sys

# The queue's `state` vocabulary. See the module docstring: the contract governs it, this is a copy, and
# a landed row LEAVES the queue rather than becoming a fifth state.
STATES = ("doing", "next", "open", "blocked")

# Sizes this lane writes. Stricter than the console needs; see the docstring.
SIZES = ("S", "M", "L")

# The status document's shape, stable across every revision that touched it.
STATUS_KEYS = (
    "atBoundary",
    "awaiting",
    "blockedOnOwner",
    "focus",
    "inFlight",
    "nextBoundary",
    "queue",
    "updatedAt",
)

# What every log entry carries, derived from the corpus and not from taste.
LOG_KEYS = ("at", "headline", "matters")

# What every decision card carries, derived the same way. See the docstring for the counts that put
# `supersedes`, `detail`, `refs`, `answered` and `because` outside this set.
DECISION_KEYS = ("id", "at", "question", "options", "recommend")


def parse_stamp(where, value, findings):
    """An RFC 3339 UTC stamp that is not in the future, or a finding saying why it is not."""
    if not isinstance(value, str):
        findings.append(f"{where}: is {type(value).__name__}, not a timestamp string")
        return None
    text = value.replace("Z", "+00:00")
    try:
        when = dt.datetime.fromisoformat(text)
    except ValueError:
        findings.append(f"{where}: {value!r} is not an RFC 3339 timestamp")
        return None
    if when.tzinfo is None:
        findings.append(f"{where}: {value!r} carries no UTC offset, so it names no instant")
        return None
    now = dt.datetime.now(dt.timezone.utc)
    if when > now:
        findings.append(
            f"{where}: {value!r} is in the FUTURE (now is {now.isoformat(timespec='seconds')}). "
            "A board cannot be updated later than now; check the year and the clock"
        )
    return when


def check_log(path, label, findings):
    try:
        text = open(path, encoding="utf-8").read()
    except OSError as e:
        findings.append(f"{label}: cannot be read: {e}")
        return
    if not text.strip():
        findings.append(f"{label}: is empty")
        return
    for n, line in enumerate(text.splitlines(), start=1):
        if not line.strip():
            findings.append(f"{label}:{n}: blank line; a JSONL file is one object per line")
            continue
        try:
            entry = json.loads(line)
        except json.JSONDecodeError as e:
            findings.append(f"{label}:{n}: does not parse as JSON: {e}")
            continue
        if not isinstance(entry, dict):
            findings.append(f"{label}:{n}: is a {type(entry).__name__}, not an object")
            continue
        for key in LOG_KEYS:
            if key not in entry:
                findings.append(f"{label}:{n}: has no {key!r}")
            elif not isinstance(entry[key], str) or not entry[key].strip():
                findings.append(f"{label}:{n}: {key!r} is not a non-empty string")
        if isinstance(entry.get("at"), str):
            parse_stamp(f"{label}:{n}: at", entry["at"], findings)


def check_decisions(path, label, findings):
    """The decision ledger: the shape of a card, and the two rules about the ids that link them.

    Two passes, because the second rule is about the file and not about a line: a `supersedes` may point
    at a card written above it or below it, and a one-pass check would call a forward reference dangling.
    """
    try:
        text = open(path, encoding="utf-8").read()
    except OSError as e:
        findings.append(f"{label}: cannot be read: {e}")
        return
    if not text.strip():
        findings.append(f"{label}: is empty")
        return

    cards = []
    for n, line in enumerate(text.splitlines(), start=1):
        if not line.strip():
            findings.append(f"{label}:{n}: blank line; a JSONL file is one object per line")
            continue
        try:
            entry = json.loads(line)
        except json.JSONDecodeError as e:
            findings.append(f"{label}:{n}: does not parse as JSON: {e}")
            continue
        if not isinstance(entry, dict):
            findings.append(f"{label}:{n}: is a {type(entry).__name__}, not an object")
            continue
        cards.append((n, entry))

    # Pass one: the shape of each card, and where each id was first seen.
    first_seen = {}
    for n, entry in cards:
        for key in DECISION_KEYS:
            if key not in entry:
                findings.append(f"{label}:{n}: has no {key!r}")
        for key in ("id", "question"):
            if key in entry and (not isinstance(entry[key], str) or not entry[key].strip()):
                findings.append(f"{label}:{n}: {key!r} is not a non-empty string")
        # ⚑ `recommend` is NOT a string, and assuming it was is what a first draft of this did. Measured:
        # 31 of the 33 cards carry an object (`{key, because}`) and 2 carry a bare sentence. So the rule
        # is that it is present and says something, and the shape is the ledger's business rather than
        # this gate's. A stricter check here would have gone red on 31 true cards.
        if "recommend" in entry:
            rec = entry["recommend"]
            empty = (isinstance(rec, str) and not rec.strip()) or (
                isinstance(rec, (dict, list)) and not rec
            )
            if rec is None or empty:
                findings.append(
                    f"{label}:{n}: 'recommend' is empty; a card with no recommendation asks the "
                    "owner to do the work of forming one"
                )
        if "options" in entry and not (
            isinstance(entry["options"], list) and entry["options"]
        ):
            findings.append(
                f"{label}:{n}: 'options' is not a non-empty array; a card with nothing to choose "
                "between is not a decision"
            )
        if isinstance(entry.get("at"), str):
            parse_stamp(f"{label}:{n}: at", entry["at"], findings)

        rid = entry.get("id")
        if isinstance(rid, str) and rid.strip():
            if rid in first_seen:
                # ⚑ Both lines, and the id. The writer of the second one cannot see the first, which is
                # the whole reason this is a gate: a finding that named only "a duplicate" would send
                # them looking for it by hand through an append-only file.
                findings.append(
                    f"{label}:{n}: id {rid!r} is ALREADY the id of the card on line "
                    f"{first_seen[rid]}. The ids are this file's only handles and it is append-only, "
                    "so two cards under one id makes every reference to it ambiguous. Measure the "
                    "highest id in the file rather than assuming the next free number"
                )
            else:
                first_seen[rid] = n

    # Pass two: the links between them.
    for n, entry in cards:
        sup = entry.get("supersedes")
        if sup is None or "supersedes" not in entry:
            continue
        rid = entry.get("id")
        if not isinstance(sup, str) or not sup.strip():
            findings.append(
                f"{label}:{n}: 'supersedes' is neither null nor a non-empty string"
            )
            continue
        if isinstance(rid, str) and sup == rid:
            findings.append(
                f"{label}:{n}: card {rid!r} supersedes ITSELF, which is a chain with no end in it"
            )
            continue
        if sup not in first_seen:
            findings.append(
                f"{label}:{n}: card {rid!r} supersedes {sup!r}, and no card in this file has that "
                "id. A reader following the chain stops here and cannot tell a missing card from a "
                "typo"
            )


def check_status(path, label, findings):
    try:
        text = open(path, encoding="utf-8").read()
    except OSError as e:
        findings.append(f"{label}: cannot be read: {e}")
        return
    try:
        doc = json.loads(text)
    except json.JSONDecodeError as e:
        findings.append(f"{label}: does not parse as JSON: {e}")
        return
    if not isinstance(doc, dict):
        findings.append(f"{label}: is a {type(doc).__name__}, not an object")
        return

    for key in STATUS_KEYS:
        if key not in doc:
            findings.append(f"{label}: has no {key!r}, which the board's shape requires")
    if "atBoundary" in doc and not isinstance(doc["atBoundary"], bool):
        findings.append(f"{label}: 'atBoundary' is not a boolean")
    if "updatedAt" in doc:
        parse_stamp(f"{label}: updatedAt", doc["updatedAt"], findings)

    queue = doc.get("queue")
    if not isinstance(queue, list):
        if "queue" in doc:
            findings.append(f"{label}: 'queue' is not an array")
        return
    seen = set()
    for i, row in enumerate(queue):
        where = f"{label}: queue[{i}]"
        if not isinstance(row, dict):
            findings.append(f"{where}: is a {type(row).__name__}, not an object")
            continue
        rid = row.get("id")
        if not isinstance(rid, str) or not rid.strip():
            findings.append(f"{where}: has no usable 'id'")
        else:
            where = f"{label}: queue[{i}] {rid}"
            if rid in seen:
                findings.append(f"{where}: duplicate id; two rows cannot be one row")
            seen.add(rid)
        if not isinstance(row.get("title"), str) or not row["title"].strip():
            findings.append(f"{where}: has no usable 'title'")
        state = row.get("state")
        if state is None:
            findings.append(f"{where}: has no 'state'")
        elif state not in STATES:
            findings.append(
                f"{where}: state {state!r} is not in the vocabulary {STATES}. "
                "One bad enum rejects the WHOLE document, so this row would take every other row "
                "off the board with it. A finished row LEAVES the queue; its landing goes to "
                "docs/lane-log.jsonl"
            )
        size = row.get("size")
        if size is not None and size not in SIZES:
            findings.append(f"{where}: size {size!r} is not one of {SIZES}")
        blocked = row.get("blockedBy")
        if blocked is not None and not isinstance(blocked, str):
            findings.append(f"{where}: 'blockedBy' is neither null nor a string")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--status", help="path to a lane-status.json to check")
    ap.add_argument("--log", help="path to a lane-log.jsonl to check")
    ap.add_argument("--decisions", help="path to a decisions.jsonl to check")
    ap.add_argument(
        "--label",
        default="",
        help="what to call these files in the findings (e.g. 'as committed at <sha>')",
    )
    args = ap.parse_args()
    if not args.status and not args.log and not args.decisions:
        ap.error("nothing to check: pass --status, --log, --decisions, or any combination")

    findings = []
    suffix = f" ({args.label})" if args.label else ""
    if args.status:
        check_status(args.status, f"lane-status.json{suffix}", findings)
    if args.log:
        check_log(args.log, f"lane-log.jsonl{suffix}", findings)
    if args.decisions:
        check_decisions(args.decisions, f"decisions.jsonl{suffix}", findings)

    if findings:
        for f in findings:
            print(f"  BAD   {f}")
        print(f"lane-check: {len(findings)} finding(s){suffix}")
        return 1
    print(f"lane-check: clean{suffix}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
