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

THE 2026-09-19 WIDENING: A GATE BELIEVED TO CHECK WHAT IT DID NOT
=================================================================

`land.sh` runs this at G2b and prints **"lane-check: clean"** on every landing. Measured that morning:
it validated structure and the `state` vocabulary and **enforced none of the contract's checkable
rules about what the rows SAY**. Every one of the following was caught by a human or a peer that
night rather than by this tool, and **every one of them is a real, dated shape in this repo's own
history** — which is what the new checks were proven red-first against, rather than against files
written to satisfy them:

| Rule | A revision of `docs/lane-status.json` that has it |
|---|---|
| exactly one `next` | `48e8e12` (2026-09-19T07:39Z, two) — 21 revisions have 2, nine have 3, six have 4, one has 5, eight have 6 |
| `next` + non-null `blockedBy` | `aba31c3` (2026-09-18T22:29Z, `PANEL-CLIP-MARK`) — 16 revisions |
| `open` + non-null `blockedBy` | `558de41` (2026-09-18T22:26Z, **five rows at once**) — 175 revisions |
| `blocked` with no blocker named | 11 revisions |
| `focus` ≤ 120 | `8d58a76` (2026-09-18T20:30Z, 126 chars) — 35 revisions |
| `title` ≤ 240 | 133 revisions |
| `queue` ≤ 20 rows | 14 revisions |
| whole file ≤ 12 KB | 8 revisions |

**231 of the 255 committed revisions break at least one of these rules. The gate as it stood at
`f4022d6` reddened on 37 of the same 255 — on none of these rules, and on 16 of them for a rule the
contract does not have** (see `STATUS_KEYS`; that divergence is corrected in the same commit). The
remaining 21 are a `size: "XS"` outside the vocabulary, which the old gate did catch. `HEAD` at
`f4022d6` is clean under the widened gate, which is why widening it does not block the next landing.
Re-derive all of this with `tools/test_lane_check.py --corpus`.

⚑ **The "37" is here because the first draft of that corpus report PRINTED "the gate at f4022d6
passed all 255" as a sentence rather than measuring it** — an unmeasured claim inside the tooling
built to remove unmeasured claims. Measuring it is what surfaced the `STATUS_KEYS` divergence.

The counts are the argument for the widening: the dominant class is `open` + `blockedBy` at 175,
exactly as the contract predicts — the two older forbidden pairs are one row each by construction
while `open` is unbounded, so the defect accumulates there.

⚑ **One item on the dispatching brief's list was already enforced and is left alone: a future
`updatedAt`.** `parse_stamp` has flagged it since this file was written. Reported rather than
silently "fixed", because a rule added twice is the copy problem in miniature.

WHERE THESE RULES WERE READ FROM, AND THE COPY PROBLEM
=======================================================

This file keeps a COPY of rules whose home is the contract, and **a carried copy is correct only on
the day it is written** — which is the defect class this whole parcel is about. It cannot be avoided
(a gate that read a peer's live working tree would measure whatever that lane happens to be editing,
and `contract/SUITE_PATHS.md` forbids it), so it is made AUDITABLE instead: the copy names the exact
revision it was taken from, and `land.sh` prints that revision on every landing, so the log of any
run says which contract the gate was enforcing.

    CONTRACT_REPO = empyrean          CONTRACT_PATH = contract/LANE_STATUS.md
    CONTRACT_REV  = 819f59b6f55c47799274adb4fe2e4c2a99613d0d   (committed 2026-09-19T08:04:45-04:00)
    CONTRACT_BLOB = a20be8cbb20b1ff4c71e2afa7807bcd854c1b9ff   (74,917 bytes)

To see everything the contract has said since this copy was taken — rather than re-deriving the rules
by reading 814 lines again:

    git -C ../empyrean fetch -q origin
    git -C ../empyrean diff 819f59b6f5 origin/main -- contract/LANE_STATUS.md

`tools/lane-check.py --contract-drift` runs the blob half of that for you. It is a REPORTER and
always exits 0, for the reason `tools/contract_drift_report.py` states at length: a gate that goes
red because a PEER moved puts the whole gradient behind bending our side until it is green, which for
a pinned copy means silencing the red by moving the pin.

Each new rule below carries the contract's own words as an anchor, so a reader can grep the contract
for the phrase instead of trusting this paraphrase.

WHAT THIS GATE DOES NOT CHECK, STATED RATHER THAN LEFT TO BE DISCOVERED
=======================================================================

`--gaps` prints this list, and `land.sh` prints it in the landing report. A green here bounds only
what is checked, and the contract says so in those words about a different reader
(*"a green from a validator bounds only what the validator checks"*).

* **Boundary audit check 2** — *every card behind a blocker is actually blocking*. Needs to know
  whether the question is still live; nothing in the files says.
* **Boundary audit check 3** — *no queue row carries an owner blocker in `blockedBy` prose*. The
  contract rules this one out of automation explicitly: *"Check 3 is a read, not a grep"* — quoted
  history matches the same words and a substring check cannot read a negation.
* **Rule 3's moving clock** — a fresh `updatedAt` on a `focus` that stopped being true. The contract:
  *"the tell is not available in the file"*.
* **Rule 6b** — an `id` carrying a mutable number. No way to tell a count from a name.
* **Rule 8's cold-successor test** — whether the `next` row is one a fresh session could BEGIN. The
  contract's own warning is that the failure mode is a *plausible* refill, which is by construction
  not detectable here.
* **`focus` written for the owner** (rule 1), `title` written for the owner, an id that can be
  grepped in the repo's queue doc — all judgement.
* **`project` ids against `contract/projects.json`** — that file is the peer's, and reading it on the
  landing path would be the live-working-tree read the copy note above rules out.

⚑ **And the trap this file is the subject of: a rule added here is a rule believed to be enforced.**
The list above exists so the belief has a boundary written next to it.

USAGE
=====

    tools/lane-check.py --status docs/lane-status.json --log docs/lane-log.jsonl \
                        --decisions docs/decisions.jsonl
    tools/lane-check.py --status <file> --label "as committed at <sha>"
    tools/lane-check.py --gaps               # the contract rules this tool does NOT enforce
    tools/lane-check.py --contract-drift     # has the contract moved past the copy? (always exits 0)

Any argument may be omitted to check only the others. Exit 0 when clean, 1 when anything is wrong;
every finding is printed with the file and, where there is one, the line or row it is about. Notes
(`NOTE`) are printed and do NOT affect the exit status; see `NOTES, AND WHY THEY ARE NOT FINDINGS`
at `check_status`.
"""

import argparse
import datetime as dt
import json
import os
import subprocess
import sys

# ---------------------------------------------------------------------------------------------
# THE COPY, AND WHERE IT CAME FROM. See "WHERE THESE RULES WERE READ FROM" in the module docstring.
# These are printed by `land.sh` on every landing, so a run's log names the contract it enforced.
# ---------------------------------------------------------------------------------------------
CONTRACT_PATH = "contract/LANE_STATUS.md"
CONTRACT_REV = "819f59b6f55c47799274adb4fe2e4c2a99613d0d"
CONTRACT_BLOB = "a20be8cbb20b1ff4c71e2afa7807bcd854c1b9ff"
CONTRACT_COMMITTED = "2026-09-19T08:04:45-04:00"

# The queue's `state` vocabulary. See the module docstring: the contract governs it, this is a copy, and
# a landed row LEAVES the queue rather than becoming a fifth state.
STATES = ("doing", "next", "open", "blocked")

# Sizes this lane writes. Stricter than the console needs; see the docstring.
SIZES = ("S", "M", "L")

# ---------------------------------------------------------------------------------------------
# THE FOUR SIZE BOUNDS. Contract rule 7 carries three (`title` 240, `queue` 20 rows, file 12 KB) and
# the `focus` cell carries the fourth (120). They are together here BECAUSE the contract's own
# warning is that they get split: *"NAMING ONE OF THESE THREE BOUNDS TO A LANE CREATES FALSE
# COVERAGE FOR THE OTHER TWO"*, and then, one paragraph later, *"AND THERE IS A FOURTH BOUND THAT IS
# NOT IN THIS RULE"* — the false-coverage clause applying to itself, because it named three.
#
# So: one constant, four entries, checked in one loop. A future reader adding a fifth adds it here
# and every caller gets it. `focus` at 126 is a real shape this repo committed (8d58a76, a47790d).
# ---------------------------------------------------------------------------------------------
MAX_FOCUS_CHARS = 120
MAX_TITLE_CHARS = 240
MAX_QUEUE_ROWS = 20
MAX_STATUS_BYTES = 12 * 1024

# The queue row's field set. The contract, rule 8: *"the row shape is `id`, `title`, `state`, `size`,
# `blockedBy`, `project`"* — and the finding that put it there is why an unknown key is a RED and not
# a shrug: aeon added an `anchors` key, *"Dominion picks named fields, so an unknown key is silently
# dropped"*, and the console answered `ok` with no `stateProblem`. **A fail-soft reader passing an
# unknown key is indistinguishable from a reader that understood it.** The write looked accepted.
ROW_KEYS = ("id", "title", "state", "size", "blockedBy", "project")

# Derived from the corpus exactly as LOG_KEYS and DECISION_KEYS were, by replaying all 255 committed
# revisions of docs/lane-status.json: `what` and `since` on 332 of 332 `blockedOnOwner` entries
# (`id` on 327, so it is NOT required), `what`/`where`/`agent` on 224 of 224 `inFlight` entries.
BLOCKED_ON_OWNER_KEYS = ("what", "since")
IN_FLIGHT_KEYS = ("what", "where", "agent")

# Contract rules that are real and are deliberately NOT enforced here. Printed by `--gaps` and by
# land.sh, because a green from this tool bounds only what this tool checks.
UNCHECKED_RULES = (
    ("boundary audit check 2", "every card behind a blocker is ACTUALLY still blocking — needs to know whether the question is live; nothing in the files says"),
    ("boundary audit check 3", "no owner blocker in `blockedBy` prose — the contract rules this out of automation itself: \"Check 3 is a read, not a grep\""),
    ("rule 3, the moving clock", "a fresh `updatedAt` stamped on a `focus` that stopped being true — \"the tell is not available in the file\""),
    ("rule 6b", "an `id` carrying a mutable number (oracle's LENS-LEFT-113) — a count and a name look identical here"),
    ("rule 8, the cold-successor test", "whether the `next` row is one a fresh session could BEGIN — the contract's warning is that the failure mode is a PLAUSIBLE refill"),
    ("rules 1 and the `title` cell", "written for the OWNER rather than for a peer — judgement, in both fields"),
    ("`project` ids", "checked against the peer's contract/projects.json — reading a peer's tree on the landing path is what SUITE_PATHS forbids"),
)

# The status document's shape. Eight keys appear; SIX are required and two are not, and the split
# is the CONTRACT's rather than this file's.
#
# ⚑ THIS LIST USED TO REQUIRE ALL EIGHT, AND THAT WAS THE COPY OUTRUNNING ITS AUTHORITY. The
#   contract's field table calls `atBoundary` *"Optional"*, and of `awaiting` it says: *"Omitting
#   the field is a third answer, 'did not say', and is read differently from `null`."* A gate that
#   required them made that third answer unsayable here -- and it is not hypothetical: **16 of this
#   board's 255 committed revisions omit both** (2026-08-23T04:04Z through 2026-08-24T00:46Z), so
#   the widened gate would have reddened on this lane's own history for obeying the contract.
#
#   Found by measuring a sentence before writing it. The first draft of this comment KEPT the
#   strictness and justified it with "all 255 revisions carry both fields"; the count is 239. The
#   rule this file states at the top -- *"On any disagreement the contract wins and this list is
#   what changes"* -- then decides it, and what changed is the code and not the paragraph.
#
#   The two optional keys are still TYPE-checked when present (see `check_status`), and their
#   absence draws no note: the contract's governing law for this document is that a validator
#   demanding a field be filled will get it filled, and "did not say" is exactly the answer a nudge
#   would destroy.
STATUS_KEYS = (
    "blockedOnOwner",
    "focus",
    "inFlight",
    "nextBoundary",
    "queue",
    "updatedAt",
)

# Present-or-absent is the lane's call; the contract says so of both. Listed rather than left
# implicit so a reader can see that their omission from STATUS_KEYS is a decision.
STATUS_KEYS_OPTIONAL = ("awaiting", "atBoundary")

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
        return set()
    if not text.strip():
        findings.append(f"{label}: is empty")
        return set()

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

    # The id set, so `check_status` can run boundary-audit check 1 against it. Returned rather than
    # re-parsed: one read of the ledger, one answer about what is in it.
    return set(first_seen)


def check_status(path, label, findings, notes=None, decision_ids=None):
    """The board.

    NOTES, AND WHY THEY ARE NOT FINDINGS
    ------------------------------------
    One rule here is reported and does not fail: **a queue with no `next` row at all.** The contract
    says *"Exactly one item should be `next`"*, so two is plainly wrong — but the rule immediately
    above it in the same document is the governing law of the whole table: **"A VALIDATOR THAT
    DEMANDS A FIELD BE FILLED WILL GET IT FILLED"**, and the instance it is written from is *this
    exact check*: aurora's gate demanded a non-empty `next` and got a row whose `blockedBy` named
    the owner, *"because that was the cheapest way to green"*.

    So a gate that reddens on zero `next` rows would reproduce, in this repo, the defect the
    contract records that check causing in another. `hub_check` already flags `0 next rows` and the
    contract calls that flag *"the backstop, not the mechanism"*. A NOTE is the honest shape: it is
    in the landing log where a reader sees it, and it cannot be bought off with a lie that passes.
    """
    if notes is None:
        notes = []
    try:
        raw = open(path, "rb").read()
        text = raw.decode("utf-8")
    except OSError as e:
        findings.append(f"{label}: cannot be read: {e}")
        return
    except UnicodeDecodeError as e:
        findings.append(f"{label}: is not valid UTF-8: {e}")
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

    # ----------------------------------------------------------------------------------------
    # The FOURTH size bound, and the one most often missed because it is not in rule 7 with the
    # other three. This repo committed it at 126 twice (8d58a76, a47790d, 2026-09-18T20:2xZ) and
    # 35 revisions in total.
    # ----------------------------------------------------------------------------------------
    focus = doc.get("focus")
    if isinstance(focus, str) and len(focus) > MAX_FOCUS_CHARS:
        findings.append(
            f"{label}: 'focus' is {len(focus)} characters, over the {MAX_FOCUS_CHARS} bound. "
            "It is the one line the OWNER reads to remember where he is, and the bound is what "
            "keeps it that. The detail belongs in the lane's own docs"
        )

    # Rule 7's third bound. Measured on the whole file as committed, in BYTES — a character count
    # would understate a file carrying non-ASCII, and these files do.
    if len(raw) > MAX_STATUS_BYTES:
        findings.append(
            f"{label}: the file is {len(raw)} bytes, over the {MAX_STATUS_BYTES} bound (rule 7). "
            "Dominion reads all six lanes' files on every hub check and a fresh session reads its "
            "own at boot, so every reader pays for every night a row carried its history"
        )

    # `blockedOnOwner` and `inFlight`: shape from the corpus, plus boundary-audit check 1.
    for field, required in (
        ("blockedOnOwner", BLOCKED_ON_OWNER_KEYS),
        ("inFlight", IN_FLIGHT_KEYS),
    ):
        val = doc.get(field)
        if field in doc and not isinstance(val, list):
            findings.append(f"{label}: {field!r} is not an array (it may be [], never absent)")
            continue
        for i, entry in enumerate(val or []):
            if not isinstance(entry, dict):
                findings.append(f"{label}: {field}[{i}] is a {type(entry).__name__}, not an object")
                continue
            for key in required:
                if key not in entry:
                    findings.append(f"{label}: {field}[{i}] has no {key!r}")

    # ----------------------------------------------------------------------------------------
    # BOUNDARY AUDIT, CHECK 1: *"Every `blockedOnOwner` id has a card"*. Only runs when the ledger
    # was also passed — `land.sh` passes all three files, so it runs on every landing. Oracle's own
    # instance is what put check 1 in the contract; the enumeration half of the rule (draw from
    # `blockedBy` prose too) is NOT automated and is in UNCHECKED_RULES, because that half is prose.
    # ----------------------------------------------------------------------------------------
    if decision_ids is not None:
        for i, entry in enumerate(doc.get("blockedOnOwner") or []):
            if not isinstance(entry, dict):
                continue
            cid = entry.get("id")
            if isinstance(cid, str) and cid.strip() and cid not in decision_ids:
                findings.append(
                    f"{label}: blockedOnOwner[{i}] names card {cid!r} and no card in the decision "
                    "ledger has that id. The console renders the blocker and the owner opens the "
                    "card; a blocker pointing at nothing spends his attention and returns nothing"
                )

    queue = doc.get("queue")
    if not isinstance(queue, list):
        if "queue" in doc:
            findings.append(f"{label}: 'queue' is not an array")
        return

    # Rule 7's second bound.
    if len(queue) > MAX_QUEUE_ROWS:
        findings.append(
            f"{label}: 'queue' holds {len(queue)} rows, over the {MAX_QUEUE_ROWS} bound (rule 7). "
            "Keep the handful a fresh session could actually start; the rest is booked in the "
            "repo's own queue doc"
        )

    seen = set()
    n_next, n_doing = 0, 0
    next_rows = []
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
        title = row.get("title")
        if not isinstance(title, str) or not title.strip():
            findings.append(f"{where}: has no usable 'title'")
        elif len(title) > MAX_TITLE_CHARS:
            # Rule 7's first bound. 133 of this repo's 255 committed revisions carry one.
            findings.append(
                f"{where}: 'title' is {len(title)} characters, over the {MAX_TITLE_CHARS} bound "
                "(rule 7). A title states what the row gets HIM and where it stands; the history "
                "of how it got there goes in the repo's own queue doc, reachable by this row's id"
            )
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

        # ------------------------------------------------------------------------------------
        # An UNKNOWN KEY. The console fail-softs on it and answers `ok`, which is why this is a
        # gate: *"a fail-soft reader passing an unknown key is indistinguishable from a reader
        # that understood it"* — the lane's write reads as accepted and the field is dropped.
        # ------------------------------------------------------------------------------------
        unknown = sorted(k for k in row if k not in ROW_KEYS)
        if unknown:
            findings.append(
                f"{where}: carries {unknown}, which is not in the row shape {list(ROW_KEYS)}. "
                "Dominion picks named fields, so an unknown key is SILENTLY DROPPED and the "
                "console still answers ok — the write reads as accepted and the content is gone. "
                "Anchors go compactly in 'title' and in full in the repo's own queue doc"
            )

        has_blocker = isinstance(blocked, str) and blocked.strip()
        if state == "doing":
            n_doing += 1
        elif state == "next":
            n_next += 1
            next_rows.append((where, has_blocker))
        elif state == "open" and has_blocker:
            # ------------------------------------------------------------------------------
            # ADDED TO THE CONTRACT 2026-09-19 (empyrean b8a7333f) FROM THIS LANE'S OWN
            # MEASUREMENT, and it is the dominant class: 175 of this repo's 255 committed
            # revisions carry at least one, against 16 for the `next` pair. The contract says
            # why — the two older pairs are one row each by construction while `open` is
            # unbounded, *"so the defect accumulates there and nowhere else"*, and therefore
            # *"a sweep for this is keyed on `open`, never on `next`"*.
            # ------------------------------------------------------------------------------
            findings.append(
                f"{where}: state 'open' with blockedBy {blocked!r}. 'open' means available to "
                "pick, so this row READS AS STARTABLE AND IS NOT — the direction that costs, "
                "because a startable-looking row gets reasoned from: a successor dispatches "
                "against it and a sweep counts it as capacity. A row whose blocker is real is "
                "'blocked' with the blocker visible"
            )
        elif state == "blocked" and not has_blocker:
            findings.append(
                f"{where}: state 'blocked' and no blocker named. No reader can tell whether the "
                "thing it waits on has landed, so the row is unauditable and stays parked"
            )

    # ----------------------------------------------------------------------------------------
    # EXACTLY ONE `next`. Two is a finding; zero is a NOTE — see this function's docstring for why
    # the asymmetry is deliberate rather than an omission.
    # ----------------------------------------------------------------------------------------
    if n_next > 1:
        findings.append(
            f"{label}: {n_next} rows are 'next' and the contract says exactly one "
            f"({', '.join(w.rsplit(' ', 1)[-1] for w, _ in next_rows)}). A list where several "
            "things are next is a list that does not help him choose. Retire and promote in ONE "
            "write: two 'next' rows appear when a new one is booked in the same write that "
            "retires the old, and the outgoing and incoming never meet"
        )
    elif n_next == 0 and queue:
        notes.append(
            f"{label}: no row is 'next'. The contract wants exactly one, and rule 8 is about this "
            "exact moment — a dispatch consumes 'next' and nothing in the act of dispatching "
            "prompts you to refill it. Reported and NOT failed on purpose: the governing law of "
            "the contract's own table is that a validator demanding a field be filled will get it "
            "filled, and that law was written from this very check reddening in another lane"
        )

    # ----------------------------------------------------------------------------------------
    # `next` WITH A BLOCKER — and the contract's EXCEPTION, which a naive rule would get wrong:
    # *"except on a lane with no `doing` row, which is answering a different question ('what I
    # would take when the hold lifts') and is the field at its most useful; seraph's held F50 is
    # the reference case."* So the pair is only a contradiction when something IS running.
    # ----------------------------------------------------------------------------------------
    for where, has_blocker in next_rows:
        if has_blocker and n_doing > 0:
            findings.append(
                f"{where}: state 'next' with a non-null blockedBy, on a lane that has a 'doing' "
                "row. The rule was never 'have a next', it is 'have a next you can BEGIN', and "
                "the gap between them is exactly wide enough to fit a lie that passes. (On a lane "
                "with no 'doing' row this pair is legitimate and this check does not fire.)"
            )


def print_gaps():
    """The contract rules this tool does not enforce. See the module docstring."""
    print(
        f"lane-check enforces a COPY of {CONTRACT_PATH} at empyrean {CONTRACT_REV[:12]} "
        f"({CONTRACT_COMMITTED}). A green bounds only what it checks. NOT checked:"
    )
    for name, why in UNCHECKED_RULES:
        print(f"  UNCHECKED  {name}: {why}")


def contract_drift(empyrean_dir):
    """Has the contract moved past the copy? A REPORTER; it always returns 0.

    The reasoning is `tools/contract_drift_report.py`'s, verbatim in effect: a gate that reddens
    because a PEER moved puts the whole gradient behind bending our side until it is green, which
    for a pinned copy means moving the pin to silence the red. And it reads the peer through git
    OBJECTS at a named revision, never through its working tree, per contract/SUITE_PATHS.md.
    """
    if not os.path.isdir(os.path.join(empyrean_dir, ".git")):
        print(
            f"lane-check --contract-drift: no git repo at {empyrean_dir!r}, so the contract is "
            "UNMEASURABLE from here. That is not a verdict on the copy."
        )
        return 0

    def git(*a):
        r = subprocess.run(
            ["git", "-C", empyrean_dir, *a], capture_output=True, text=True
        )
        return r.stdout.strip() if r.returncode == 0 else None

    tip = git("rev-parse", f"origin/main:{CONTRACT_PATH}")
    if tip is None:
        print(
            "lane-check --contract-drift: could not resolve "
            f"origin/main:{CONTRACT_PATH} (no such ref, or it is not fetched). UNMEASURABLE."
        )
        return 0
    print(f"  copy taken from : {CONTRACT_REV[:12]}  blob {CONTRACT_BLOB[:12]}  ({CONTRACT_COMMITTED})")
    print(f"  peer origin/main: {(git('rev-parse', 'origin/main') or '?')[:12]}  blob {tip[:12]}")
    if tip == CONTRACT_BLOB:
        print("  the contract has NOT moved since this copy was taken.")
        return 0
    n = git("rev-list", "--count", f"{CONTRACT_REV}..origin/main", "--", CONTRACT_PATH)
    print(
        f"  THE CONTRACT HAS MOVED ({n or '?'} commit(s) touching it since the copy). Read the "
        "diff before trusting any rule here:"
    )
    print(f"    git -C {empyrean_dir} diff {CONTRACT_REV[:10]} origin/main -- {CONTRACT_PATH}")
    print("  Reported, not failed: a peer moving must never redden this lane's landing.")
    return 0


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
    ap.add_argument(
        "--gaps",
        action="store_true",
        help="print the contract rules this tool does NOT enforce, and exit",
    )
    ap.add_argument(
        "--contract-drift",
        nargs="?",
        const="../empyrean",
        default=None,
        metavar="EMPYREAN_DIR",
        help="report whether the contract has moved past the copy these rules were read from "
        "(a REPORTER: always exits 0)",
    )
    args = ap.parse_args()
    if args.gaps:
        print_gaps()
        return 0
    if args.contract_drift is not None:
        return contract_drift(args.contract_drift)
    if not args.status and not args.log and not args.decisions:
        ap.error(
            "nothing to check: pass --status, --log, --decisions, --gaps, --contract-drift, "
            "or any combination"
        )

    findings = []
    notes = []
    suffix = f" ({args.label})" if args.label else ""
    # The ledger FIRST, so its id set is available to boundary-audit check 1 on the board. When no
    # ledger is passed, `decision_ids` stays None and that check does not run rather than running
    # against an empty set and calling every card missing.
    decision_ids = None
    if args.decisions:
        decision_ids = check_decisions(args.decisions, f"decisions.jsonl{suffix}", findings)
    if args.status:
        check_status(
            args.status, f"lane-status.json{suffix}", findings, notes, decision_ids
        )
    if args.log:
        check_log(args.log, f"lane-log.jsonl{suffix}", findings)

    for n in notes:
        print(f"  NOTE  {n}")
    if findings:
        for f in findings:
            print(f"  BAD   {f}")
        print(f"lane-check: {len(findings)} finding(s){suffix}")
        return 1
    # The contract revision is on the clean line too, so a landing log says which contract the gate
    # was enforcing without anyone opening this file.
    tail = f", {len(notes)} note(s)" if notes else ""
    print(f"lane-check: clean{suffix} [contract {CONTRACT_REV[:10]}{tail}]")
    return 0


if __name__ == "__main__":
    sys.exit(main())
