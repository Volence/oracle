#!/usr/bin/env python3
"""Prove that a prose document was split into several files WITHOUT LOSS.

Why this exists
---------------
A document split that is proved by "line counts add up / md5 of the concatenation
matches / diff exits 0" can be simultaneously correct and useless.  A split
elsewhere in this suite passed exactly those three checks and had still cut 34
sentences in half, because every cut fell INSIDE the unit each check counted.

So this instrument proves three independent things, at three different
granularities, and a green requires all three:

  PROOF 1  STRUCTURE   Every non-blank line of the original appears in exactly
                       one output, no output line is unaccounted for, and each
                       output's stream is an ORDER-PRESERVING SUBSEQUENCE of the
                       original.  The outputs together must partition the
                       original's non-blank stream.  A multiset check alone
                       would accept a file whose lines were shuffled; the
                       subsequence property is what refuses that.

  PROOF 2  TOKENS      Whitespace-token counts, before vs after, with the delta
                       itemised.  Catches loss *inside* a line, which PROOF 1
                       (which compares whole lines) reports only as a swap of
                       one line for another.

  PROOF 3  SENTENCES   At every paragraph seam of every output file, does the
                       text before still END a sentence, and does the text after
                       still BEGIN one?  Run over the ORIGINAL too, and only
                       seams the split INTRODUCED are counted against it -- prose
                       that already read that way is not this split's doing.
                       Additionally, the exact cut points are DERIVED from the
                       PROOF 1 alignment (never hardcoded line numbers) and each
                       is put to the same predicate.

Exit status
-----------
  0  proved     every proof passed
  1  disproved  the instrument measured the split and found a real failure
  2  unmeasured an input was missing, unreadable, or too ambiguous to align

2 is deliberately distinct from 1.  "I could not measure this" must never render
as a 0, and must never render as a plain 1 either, because a lane fixing a red
needs to know whether it is fixing its document or fixing its invocation.

Usage
-----
  tools/prove_doc_split.py \
      --original main:docs/OVERSEER.md \
      --output docs/OVERSEER.md --output docs/OVERSEER-REFERENCE.md \
      --new /tmp/new-headings.md \
      --headings

`--original` accepts a git revision spelling (``rev:path``) as well as a plain
path.  A lane's pre-split original is normally a committed blob, and reading it
out of git rather than off disk is what keeps the proof honest: a working-tree
copy of "the original" can have been edited by the very change under test.

Exactly one of `--new FILE...` / `--no-new` is required.  Defaulting to "no new
lines" would let an invocation that forgot to declare its new material read as a
stricter proof than the one actually run.

Run the tests with: tools/run_doc_split_tests.sh
"""

from __future__ import annotations

import argparse
import collections
import re
import subprocess
import sys
from pathlib import Path

# --------------------------------------------------------------------------
# Input resolution.  Every failure here raises Unmeasurable, which becomes an
# exit 2 with a named cause on stderr and NOTHING on stdout.
# --------------------------------------------------------------------------


class Unmeasurable(Exception):
    """An input could not be read, or the alignment could not be decided."""


def read_source(spec: str, repo: Path) -> tuple[str, str]:
    """Return (text, provenance) for a path or a ``rev:path`` git spelling."""
    p = Path(spec)
    if p.exists():
        if p.is_dir():
            raise Unmeasurable(f"{spec}: is a directory, not a document")
        try:
            return p.read_text(encoding="utf-8"), f"file {p}"
        except UnicodeDecodeError as exc:
            raise Unmeasurable(f"{spec}: not UTF-8 text ({exc})") from exc
        except OSError as exc:
            raise Unmeasurable(f"{spec}: cannot read ({exc})") from exc
    if ":" in spec:
        try:
            r = subprocess.run(
                ["git", "-C", str(repo), "show", spec],
                capture_output=True, check=False)
        except OSError as exc:
            raise Unmeasurable(f"{spec}: cannot run git ({exc})") from exc
        if r.returncode != 0:
            err = r.stderr.decode("utf-8", "replace").strip()
            raise Unmeasurable(
                f"{spec}: no such file on disk, and "
                f"`git -C {repo} show {spec}` failed (exit {r.returncode}): {err}")
        try:
            return r.stdout.decode("utf-8"), f"git {spec} (repo {repo})"
        except UnicodeDecodeError as exc:
            raise Unmeasurable(f"{spec}: git blob is not UTF-8 text ({exc})") from exc
    raise Unmeasurable(
        f"{spec}: no such file, and it is not a git revision spelling "
        f"(a git spelling contains ':', as in 'main:docs/OVERSEER.md')")


def lines_of(text: str) -> list[str]:
    return text.split("\n")


def nb(ls):
    """Non-blank lines.  Blank-line placement is formatting, not content."""
    return [l for l in ls if l.strip()]


def toks(ls):
    return [t for l in ls for t in l.split()]


# --------------------------------------------------------------------------
# PROOF 1b -- order-preserving partition.
#
# The question: is the original's non-blank stream an INTERLEAVING of the
# outputs' non-blank streams, once the declared-new lines are removed from the
# outputs?  If yes, then simultaneously:
#   * every original line lands somewhere        (nothing dropped)
#   * no original line lands twice               (nothing duplicated)
#   * every output line is either an original    (nothing invented)
#     line or a declared-new one
#   * within each output the original lines keep (nothing reordered)
#     their original relative order
#
# Declared-new lines are NOT removed by naive text matching.  A new line whose
# text happens to coincide with a real original line ('---', a repeated heading)
# would then be deleted from the wrong place.  Instead each output line is
# classified once:
#     FORCED_NEW  text occurs among the declared-new fragments and NOT in the
#                 original -> it can only be new; skip it deterministically
#     AMBIGUOUS   text occurs in both -> the search branches on it
#     MATCHABLE   text occurs only in the original -> it must match
# The search then decides, rather than the tool guessing.
# --------------------------------------------------------------------------

FORCED_NEW, AMBIGUOUS, MATCHABLE = 0, 1, 2

DEFAULT_STATE_CAP = 20000


class AlignmentFailure(Exception):
    def __init__(self, message, detail_lines):
        super().__init__(message)
        self.detail_lines = detail_lines


def _classify(streams, new_texts, orig_texts):
    out = []
    for s in streams:
        row = []
        for line in s:
            in_new = line in new_texts
            in_orig = line in orig_texts
            if in_new and not in_orig:
                row.append(FORCED_NEW)
            elif in_new and in_orig:
                row.append(AMBIGUOUS)
            else:
                row.append(MATCHABLE)
        out.append(row)
    return out


def align(orig_nb, streams, new_texts, state_cap=DEFAULT_STATE_CAP):
    """Align the outputs against the original.

    Returns (owner, skipped) where ``owner[i]`` is the index of the output file
    holding original line ``i``, and ``skipped[j]`` is the list of positions in
    output ``j`` that were consumed as declared-new.

    Raises AlignmentFailure (a measured red) or Unmeasurable (ambiguity blew
    past the cap -- reported, never silently accepted).
    """
    k = len(streams)
    orig_texts = set(orig_nb)
    cls = _classify(streams, new_texts, orig_texts)

    def normalize(state):
        cur = list(state)
        for j in range(k):
            c = cur[j]
            while c < len(streams[j]) and cls[j][c] == FORCED_NEW:
                c += 1
            cur[j] = c
        return tuple(cur)

    def closure(seed_states):
        """Expand over declared-new skips (deterministic for FORCED_NEW,
        branching for AMBIGUOUS)."""
        seen, stack, out = set(), [normalize(s) for s in seed_states], set()
        while stack:
            s = stack.pop()
            if s in seen:
                continue
            seen.add(s)
            out.add(s)
            if len(seen) > state_cap:
                raise Unmeasurable(
                    f"alignment ambiguity exceeded the {state_cap}-state cap. "
                    f"Too many declared-new lines share text with the original "
                    f"for the partition to be decided. Narrow --new, or raise "
                    f"--state-cap deliberately.")
            for j in range(k):
                c = s[j]
                if c < len(streams[j]) and cls[j][c] == AMBIGUOUS:
                    stack.append(normalize(s[:j] + (c + 1,) + s[j + 1:]))
        return out

    start = tuple(0 for _ in range(k))
    states = closure({start})
    # levels[i] maps a state reached after consuming orig_nb[:i] to
    # (predecessor state, output index that took orig_nb[i-1]).
    levels = [{s: (None, None) for s in states}]

    for i, line in enumerate(orig_nb):
        nxt = {}
        for s in states:
            for j in range(k):
                c = s[j]
                if (c < len(streams[j]) and cls[j][c] != FORCED_NEW
                        and streams[j][c] == line):
                    for u in closure({s[:j] + (c + 1,) + s[j + 1:]}):
                        nxt.setdefault(u, (s, j))
            if len(nxt) > state_cap:
                raise Unmeasurable(
                    f"alignment ambiguity exceeded the {state_cap}-state cap at "
                    f"original line {i + 1}. Raise --state-cap deliberately if "
                    f"this document really is that ambiguous.")
        if not nxt:
            detail = [f"original line {i + 1} could not be placed in any output "
                      f"at its turn:",
                      f"    ORIGINAL {i + 1}: {line!r}"]
            for s in sorted(states)[:4]:
                for j in range(k):
                    c = s[j]
                    at = streams[j][c] if c < len(streams[j]) else "<end of file>"
                    detail.append(f"    output[{j}] is at its line {c + 1}: {at!r}")
                break
            detail.append("    -> the line is absent from every output, or it is "
                          "present but OUT OF ORDER (already passed, or still "
                          "ahead of a line that should have followed it).")
            raise AlignmentFailure(
                f"order-preserving partition FAILED at original line {i + 1}",
                detail)
        states = set(nxt)
        levels.append(nxt)

    accepting = [s for s in states
                 if all(s[j] == len(streams[j]) for j in range(k))]
    if not accepting:
        detail = ["every original line was placed, but at least one output has "
                  "leftover lines that are neither original nor declared new:"]
        s = sorted(states)[0]
        for j in range(k):
            for c in range(s[j], len(streams[j])):
                detail.append(f"    UNACCOUNTED output[{j}] line {c + 1}: "
                              f"{streams[j][c]!r}")
        raise AlignmentFailure(
            "order-preserving partition FAILED: output lines left over", detail)

    # Walk one accepting alignment back to a per-original-line owner.
    state = accepting[0]
    owner = [None] * len(orig_nb)
    for i in range(len(orig_nb), 0, -1):
        prev, j = levels[i][state]
        owner[i - 1] = j
        state = prev
    consumed = [set() for _ in range(k)]
    # Re-walk forwards to recover which output positions were matches.
    cursors = [0] * k
    for i, line in enumerate(orig_nb):
        j = owner[i]
        while streams[j][cursors[j]] != line or cls[j][cursors[j]] == FORCED_NEW:
            cursors[j] += 1
        consumed[j].add(cursors[j])
        cursors[j] += 1
    skipped = [[c for c in range(len(streams[j])) if c not in consumed[j]]
               for j in range(k)]
    return owner, skipped


# --------------------------------------------------------------------------
# PROOF 3 -- the sentence-boundary predicate.
# --------------------------------------------------------------------------

TRAIL = "*_`\"')]}>» "
STARTERS = ("#", "-", "*", ">", "|", "```",
            "1.", "2.", "3.", "4.", "5.", "6.", "7.", "8.", "9.")


def make_ends_sentence(heading_aware: bool):
    def ends_sentence(s: str) -> bool:
        t = s.rstrip()
        if heading_aware and t.lstrip().startswith("#"):
            # A markdown heading is a title, not a sentence. It has no terminal
            # punctuation by convention and cannot be "cut in half", so scoring
            # it as an unfinished sentence manufactures a failure. The
            # heading-BLIND number is still reported alongside, so choosing this
            # flag never quietly improves the result without saying so.
            return True
        while t and t[-1] in TRAIL:
            t = t[:-1]
        if not t:
            return True                       # pure markup (```/---) is not prose
        if re.fullmatch(r"[-=|#>` ]+", t):
            return True
        return t[-1] in ".!?:;"
    return ends_sentence


def begins_sentence(s: str) -> bool:
    t = s.strip()
    if not t or t.startswith(STARTERS):
        return True
    u = t.lstrip("*_`\"'([⚠▶✅⚑ ")
    if not u:
        return True
    c = u[0]
    return not (c.isalpha() and c.islower())


def seams(ls):
    """Every paragraph seam: (last non-blank before a blank run, first after)."""
    out, i, n = [], 0, len(ls)
    while i < n:
        if ls[i].strip() == "":
            j = i
            while j < n and ls[j].strip() == "":
                j += 1
            k = i - 1
            while k >= 0 and ls[k].strip() == "":
                k -= 1
            if k >= 0 and j < n:
                out.append((ls[k], ls[j]))
            i = j
        else:
            i += 1
    return out


def introduced_seams(orig_lines, out_lines_list, ends_sentence):
    """Seams flagged in the outputs, minus those already flagged in the original."""
    def flagged(ls):
        return [p for p in seams(ls)
                if not ends_sentence(p[0]) or not begins_sentence(p[1])]
    orig_flagged = collections.Counter(flagged(orig_lines))
    after = collections.Counter()
    per_file = []
    for ls in out_lines_list:
        f = flagged(ls)
        per_file.append(f)
        after += collections.Counter(f)
    return list((after - orig_flagged).elements()), orig_flagged, per_file


def edge_failures(orig_lines, out_lines_list, ends_sentence):
    """A file that STARTS mid-sentence or ENDS mid-sentence is cut, too, and no
    internal seam reports it.  Only counted where the original's own first/last
    line is not the one in question."""
    o = nb(orig_lines)
    if not o:
        return []
    bad = []
    for idx, ls in enumerate(out_lines_list):
        s = nb(ls)
        if not s:
            continue
        if s[0] != o[0] and not begins_sentence(s[0]):
            bad.append((idx, "starts mid-sentence", s[0]))
        if s[-1] != o[-1] and not ends_sentence(s[-1]):
            bad.append((idx, "ends mid-sentence", s[-1]))
    return bad


def derive_cuts(orig_nb, owner, names):
    """The cut points, DERIVED from the alignment -- never hardcoded.

    Two families, and both are real cuts:
      DIVERGENCE  original adjacency i -> i+1 broken because the two lines went
                  to different files.
      JUNCTION    new adjacency created inside one file, because it holds
                  original lines a and b with b > a+1 and nothing between.
    """
    cuts = []
    for i in range(len(orig_nb) - 1):
        if owner[i] != owner[i + 1]:
            cuts.append((f"DIVERGENCE {names[owner[i]]} keeps original line "
                         f"{i + 1} -> {names[owner[i + 1]]} takes {i + 2}",
                         orig_nb[i], orig_nb[i + 1]))
    last = {}
    for i, j in enumerate(owner):
        if j in last and i > last[j] + 1:
            a = last[j]
            cuts.append((f"JUNCTION {names[j]} joins original line {a + 1} "
                         f"straight to {i + 1}", orig_nb[a], orig_nb[i]))
        last[j] = i
    return cuts


# --------------------------------------------------------------------------


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(
        prog="prove_doc_split.py",
        description="Prove a prose document was split losslessly.")
    ap.add_argument("--original", required=True, metavar="PATH|REV:PATH",
                    help="the pre-split document; a git 'rev:path' spelling is "
                         "preferred, so the proof reads a committed blob")
    ap.add_argument("--output", required=True, action="append", metavar="PATH",
                    help="a post-split file (repeat for each)")
    ap.add_argument("--new", action="append", default=[], metavar="PATH",
                    help="a file of lines declared NEW in the split (repeat)")
    ap.add_argument("--no-new", action="store_true",
                    help="assert the split introduced NO new lines at all")
    ap.add_argument("--headings", action="store_true",
                    help="treat a markdown heading as a complete sentence for "
                         "PROOF 3 (both numbers are printed either way)")
    ap.add_argument("--repo", default=".", metavar="DIR",
                    help="repository for git 'rev:path' resolution (default .)")
    ap.add_argument("--state-cap", type=int, default=DEFAULT_STATE_CAP,
                    help=argparse.SUPPRESS)
    ap.add_argument("--max-report", type=int, default=20, metavar="N",
                    help="cap on itemised failures printed per category")
    args = ap.parse_args(argv)

    if bool(args.new) == bool(args.no_new):
        ap.error("give exactly one of --new FILE (repeatable) or --no-new; "
                 "an undeclared default would let a forgotten declaration read "
                 "as a stricter proof than the one actually run")

    repo = Path(args.repo)
    orig_text, orig_prov = read_source(args.original, repo)
    outs, out_prov = [], []
    for spec in args.output:
        t, p = read_source(spec, repo)
        outs.append(lines_of(t))
        out_prov.append(p)
    new_lines = []
    new_prov = []
    for spec in args.new:
        t, p = read_source(spec, repo)
        new_lines += lines_of(t)
        new_prov.append(p)

    orig = lines_of(orig_text)
    names = [Path(s).name or s for s in args.output]
    W = max([len(n) for n in names] + [10])

    print("=" * 92)
    print("INPUTS")
    print("=" * 92)
    print(f"  original : {orig_prov}")
    for n, p in zip(names, out_prov):
        print(f"  output   : {n} <- {p}")
    if new_prov:
        for p in new_prov:
            print(f"  declared new : {p}")
    else:
        print("  declared new : NONE (--no-new)")

    o = nb(orig)
    streams = [nb(x) for x in outs]
    a = nb(new_lines)

    failures = []

    # ---------------- PROOF 1 ----------------
    print()
    print("=" * 92)
    print("PROOF 1 - STRUCTURE: line accounting, then order-preserving partition")
    print("=" * 92)
    print(f"  original non-blank lines            : {len(o)}")
    for n, s in zip(names, streams):
        print(f"  {n:<{W}} non-blank lines : {len(s)}")
    print(f"  declared NEW non-blank lines        : {len(a)}")
    total = sum(len(s) for s in streams)
    print(f"  outputs - new  (must == original)   : {total - len(a)}")

    co = collections.Counter(o)
    cn = collections.Counter()
    for s in streams:
        cn += collections.Counter(s)
    ca = collections.Counter(a)
    missing = co - cn
    extra = cn - co
    unexplained = extra - ca
    overdeclared = ca - extra

    def itemise(label, counter):
        n = sum(counter.values())
        for l in list(counter.elements())[:args.max_report]:
            print(f"      {label}: {l!r}")
        if n > args.max_report:
            print(f"      ... and {n - args.max_report} more")
        return n

    nmissing = sum(missing.values())
    print(f"  1a ORIGINAL lines ABSENT after split: {nmissing}")
    itemise("ABSENT", missing)
    nunex = sum(unexplained.values())
    print(f"  1b lines present but NOT DECLARED   : {nunex}")
    itemise("UNDECLARED", unexplained)
    nover = sum(overdeclared.values())
    print(f"  1c declared NEW but already present : {nover}")
    itemise("PRE-EXISTING", overdeclared)

    if nmissing:
        failures.append(f"PROOF 1a: {nmissing} original line(s) absent after the split")
    if nunex:
        failures.append(f"PROOF 1b: {nunex} line(s) appeared that were never declared new")
    if nover:
        failures.append(f"PROOF 1c: {nover} declared-new line(s) already existed in the original")

    owner = None
    print()
    print("  1d order-preserving partition: is the original an interleaving of")
    print("      the outputs (declared-new lines removed)?  A multiset check")
    print("      alone would accept a file whose lines were shuffled.")
    try:
        owner, skipped = align(o, streams, set(a), state_cap=args.state_cap)
    except AlignmentFailure as exc:
        print(f"      RESULT: FAIL - {exc}")
        for line in exc.detail_lines:
            print(f"      {line}")
        failures.append(f"PROOF 1d: {exc}")
    else:
        print("      RESULT: PASS - every original line sits in exactly one")
        print("              output, in its original relative order.")
        for j, name in enumerate(names):
            held = sum(1 for x in owner if x == j)
            print(f"        {name:<{W}} holds {held} original line(s) "
                  f"+ {len(skipped[j])} declared-new")
        placed = collections.Counter()
        for j in range(len(streams)):
            for c in skipped[j]:
                placed[streams[j][c]] += 1
        if placed != ca:
            short = ca - placed
            over = placed - ca
            print("      NOTE: the alignment's new-line set differs from the "
                  "declaration:")
            itemise("DECLARED-NOT-USED", short)
            itemise("USED-NOT-DECLARED", over)
            failures.append("PROOF 1d: declared-new set does not match the "
                            "lines the alignment had to treat as new")

    # ---------------- PROOF 2 ----------------
    print()
    print("=" * 92)
    print("PROOF 2 - TOKENS: whitespace-token accounting (catches loss INSIDE a line)")
    print("=" * 92)
    to = toks(orig)
    tstreams = [toks(x) for x in outs]
    ta = toks(new_lines)
    print(f"  original tokens                     : {len(to)}")
    for n, t in zip(names, tstreams):
        print(f"  {n:<{W}} tokens          : {len(t)}")
    tt = sum(len(t) for t in tstreams)
    print(f"  declared NEW tokens                 : {len(ta)}")
    print(f"  outputs - new  (must == original)   : {tt - len(ta)}")
    delta = tt - len(ta) - len(to)
    print(f"  DELTA vs original                   : {delta}")
    mo = collections.Counter(to)
    mn = collections.Counter()
    for t in tstreams:
        mn += collections.Counter(t)
    lost = mo - mn
    gained = (mn - mo) - collections.Counter(ta)
    nlost, ngain = sum(lost.values()), sum(gained.values())
    print(f"  tokens LOST                         : {nlost}")
    itemise("LOST TOKEN", lost)
    print(f"  tokens GAINED undeclared            : {ngain}")
    itemise("GAINED TOKEN", gained)
    if nlost:
        failures.append(f"PROOF 2: {nlost} token(s) lost")
    if ngain:
        failures.append(f"PROOF 2: {ngain} undeclared token(s) gained")

    # ---------------- PROOF 3 ----------------
    print()
    print("=" * 92)
    print("PROOF 3 - SENTENCES: boundary predicate, run from BOTH sides of every seam")
    print("=" * 92)
    print("  At each paragraph seam: does the text BEFORE still end a sentence,")
    print("  and does the text AFTER still begin one?  Seams already flagged in")
    print("  the ORIGINAL are pre-existing prose style, not this split's doing,")
    print("  and are subtracted.")
    print()
    gate_ends = make_ends_sentence(args.headings)
    results = {}
    for mode, ha in (("heading-aware", True), ("heading-blind", False)):
        f = make_ends_sentence(ha)
        intro, orig_flagged, per_file = introduced_seams(orig, outs, f)
        edges = edge_failures(orig, outs, f)
        results[ha] = (intro, orig_flagged, per_file, edges)
        mark = "  <= GATE" if ha == args.headings else ""
        print(f"  [{mode:<13}] flagged in original: "
              f"{sum(orig_flagged.values()):<4} "
              f"seams INTRODUCED by the split: {len(intro):<4} "
              f"file-edge cuts: {len(edges)}{mark}")
    print()
    print("  Both numbers are printed on every run.  --headings only chooses")
    print("  which one gates; it can never quietly improve the reported result.")

    intro, orig_flagged, per_file, edges = results[args.headings]
    if intro:
        print()
        print(f"  SEAMS INTRODUCED BY THE SPLIT: {len(intro)}")
        for p in intro[:args.max_report]:
            print("      INTRODUCED SEAM:")
            print(f"        before: {p[0]!r}")
            print(f"        after : {p[1]!r}")
        if len(intro) > args.max_report:
            print(f"      ... and {len(intro) - args.max_report} more")
        failures.append(f"PROOF 3: {len(intro)} sentence seam(s) introduced by the split")
    if edges:
        print()
        print(f"  FILE-EDGE CUTS: {len(edges)}")
        for idx, why, line in edges[:args.max_report]:
            print(f"      EDGE CUT [{names[idx]}] {why}: {line!r}")
        failures.append(f"PROOF 3: {len(edges)} file-edge cut(s)")

    print()
    print("  Cut points DERIVED from the PROOF 1 alignment (no hardcoded line")
    print("  numbers), each put to the same predicate:")
    if owner is None:
        print("      NOT DERIVABLE - the alignment failed, so there is no")
        print("      trustworthy cut list. This is reported, never skipped.")
        failures.append("PROOF 3: cut points not derivable (alignment failed)")
    else:
        cuts = derive_cuts(o, owner, names)
        bad = 0
        for label, before, after in cuts:
            ok = gate_ends(before) and begins_sentence(after)
            bad += 0 if ok else 1
            print(f"    [{'OK ' if ok else 'BAD'}] {label}")
            print(f"          before: {before[:88]!r}")
            print(f"          after : {after[:88]!r}")
        print(f"  derived cut points: {len(cuts)}   failing the predicate: {bad}")
        if bad:
            failures.append(f"PROOF 3: {bad} derived cut point(s) fail the "
                            f"sentence-boundary predicate")

    # ---------------- VERDICT ----------------
    print()
    print("=" * 92)
    if failures:
        print(f"VERDICT: DISPROVED - {len(failures)} failing check(s)")
        for f in failures:
            print(f"  FAILED: {f}")
        print("=" * 92)
        return 1
    print("VERDICT: PROVED - the split is lossless by all three proofs")
    print("=" * 92)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Unmeasurable as exc:
        print(f"UNMEASURABLE: {exc}", file=sys.stderr)
        sys.exit(2)
