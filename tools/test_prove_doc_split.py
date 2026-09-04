#!/usr/bin/env python3
"""Tests for tools/prove_doc_split.py.

An instrument that cannot fail is worse than no instrument, because its presence
and its absence read the same.  prove_doc_split.py is about to become four other
lanes' gate, so the point of this suite is not "does it say yes on a good split"
-- it is "does it say NO, by name, on each way a split goes wrong".

Every fixture here is synthetic and built in a tempdir.  Nothing reads a real
OVERSEER.md: that file moves, and a test pinned to it would rot into a skip or a
false red the next time a lane edits a heading.

The failure modes covered, one class each:

  CleanSplitControl        a good split is GREEN.  Without this control the whole
                           suite could pass by refusing everything.
  MidSentenceTruncation    the 2026-09-02 failure, reproduced: every line
                           accounted for, token counts equal, and a bullet cut in
                           half across the two files.  PROOF 1 and PROOF 2 stay
                           green; PROOF 3 must go red and quote the seam.
  DroppedLine              a line vanishes -> PROOF 1 red, naming it.
  ReorderedLine            a line is present but out of sequence -> only the
                           order-preserving check can see it.  The test asserts
                           the multiset checks stayed green, which is the whole
                           argument for the subsequence property.
  UndeclaredAddition       a line appears that was never declared new -> red.
  AmbiguousDeclaredNew     a declared-new line whose text duplicates a real
                           original line.  Green when honest, red when the
                           original copy was actually dropped.  Naive
                           remove-by-content gets both of these wrong.
  MarkdownTableRowAtACut   a markdown table row is the last line kept before a
                           cut.  Nothing is torn, so the verdict must be GREEN:
                           a row ends in '|' and so could never end a sentence,
                           which made every such correct cut a false DISPROVED.
  TornSentenceBesideATable the control on that fix.  A genuinely torn sentence
                           next to a table must STILL be red -- an over-broad
                           "not a sentence" would re-hide the 09-02 defect.
  VerdictCarriesBothHeadingNumbers
                           the verdict states the heading-aware AND the
                           heading-blind figure and which one gated, so a lane
                           reading only the gated number cannot miss the other.
  Unmeasurable             a missing / unreadable / undecidable input FAILS with
                           a named cause and exit 2 -- never a skip, never a 0,
                           never rendered as "0 problems found".
  GitRevisionOriginal      'rev:path' really is read out of git.
  ProvenanceIdentifiesTheTree
                           two runs that measured DIFFERENT documents must not
                           print the same provenance -- a green whose subject a
                           reader cannot name is the failure class this whole
                           instrument exists to end.
  RunnerIsWired            something in the tree actually runs this file.

Run via: tools/run_doc_split_tests.sh
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOOL = Path(__file__).resolve().parent / "prove_doc_split.py"
RUNNER = Path(__file__).resolve().parent / "run_doc_split_tests.sh"


# ---------------------------------------------------------------------------
# The base document and its honest split.  Ground truth is fixed by
# construction: this text is the whole world these tests live in.
# ---------------------------------------------------------------------------

ORIGINAL = """\
# Handbook

This document explains the thing.

## Boot rules

Read these first.

- The first rule is long enough to wrap, and so it ends with a comma,
  continuing on this second line to a full stop.

The boot section closes here.

## Reference rules

Look these up on demand.

- A reference rule, stated completely on one line.

## Boot appendix

The last boot paragraph.
"""

CLEAN_BOOT = """\
# Handbook

This document explains the thing.

## Boot rules

Read these first.

- The first rule is long enough to wrap, and so it ends with a comma,
  continuing on this second line to a full stop.

The boot section closes here.

## Boot appendix

The last boot paragraph.
"""

CLEAN_REF = """\
## Reference rules

Look these up on demand.

- A reference rule, stated completely on one line.
"""

CUT_LINE = "- The first rule is long enough to wrap, and so it ends with a comma,"
TAIL_LINE = "  continuing on this second line to a full stop."


class ToolCase(unittest.TestCase):
    """Runs the tool as a real subprocess, so exit status is the real thing."""

    def setUp(self):
        self._td = tempfile.TemporaryDirectory()
        self.d = Path(self._td.name)
        self.addCleanup(self._td.cleanup)

    def write(self, name, text):
        p = self.d / name
        p.write_text(text, encoding="utf-8")
        return p

    def run_tool(self, *args, expect=None, cwd=None):
        r = subprocess.run([sys.executable, str(TOOL), *[str(a) for a in args]],
                           capture_output=True, text=True,
                           cwd=None if cwd is None else str(cwd))
        if expect is not None:
            self.assertEqual(
                r.returncode, expect,
                f"expected exit {expect}, got {r.returncode}\n"
                f"--- stdout ---\n{r.stdout}\n--- stderr ---\n{r.stderr}")
        return r

    def split(self, boot=CLEAN_BOOT, ref=CLEAN_REF, original=ORIGINAL,
              new=None, extra=()):
        o = self.write("ORIG.md", original)
        b = self.write("boot.md", boot)
        f = self.write("ref.md", ref)
        args = ["--original", o, "--output", b, "--output", f, "--headings"]
        if new is None:
            args.append("--no-new")
        else:
            n = self.write("new.md", new)
            args += ["--new", n]
        args += list(extra)
        return args


# ---------------------------------------------------------------------------


class CleanSplitControl(ToolCase):
    """The control.  A suite that only proves reds can be passed by an
    instrument that refuses everything."""

    def test_clean_split_is_proved(self):
        r = self.run_tool(*self.split(), expect=0)
        self.assertIn("VERDICT: PROVED", r.stdout)
        self.assertNotIn("FAILED:", r.stdout)

    def test_clean_split_derives_its_cut_points(self):
        r = self.run_tool(*self.split(), expect=0)
        # Derived from the alignment, never hardcoded: the two files diverge
        # twice and boot.md re-joins across the reference block once.
        self.assertIn("DIVERGENCE", r.stdout)
        self.assertIn("JUNCTION", r.stdout)
        self.assertIn("failing the predicate: 0", r.stdout)
        self.assertNotIn("[BAD]", r.stdout)

    def test_both_heading_numbers_are_always_reported(self):
        """--headings chooses which number gates; it never hides the other."""
        r = self.run_tool(*self.split(), expect=0)
        self.assertIn("[heading-aware]", r.stdout)
        self.assertIn("[heading-blind]", r.stdout)
        # ... and the flattering one is the one marked as gating.
        aware = [l for l in r.stdout.splitlines() if "heading-aware" in l]
        self.assertTrue(any("<= GATE" in l for l in aware), r.stdout)

    def test_three_way_split_is_proved(self):
        """The instrument is not two-file-shaped: the partition is k-way."""
        a = self.write("a.md", "# Handbook\n\nThis document explains the thing.\n")
        b = self.write("b.md", "## Boot rules\n\nRead these first.\n\n"
                               + CUT_LINE + "\n" + TAIL_LINE + "\n\n"
                               "The boot section closes here.\n")
        c = self.write("c.md", "## Reference rules\n\nLook these up on demand.\n\n"
                               "- A reference rule, stated completely on one line.\n\n"
                               "## Boot appendix\n\nThe last boot paragraph.\n")
        o = self.write("ORIG.md", ORIGINAL)
        r = self.run_tool("--original", o, "--output", a, "--output", b,
                          "--output", c, "--no-new", "--headings", expect=0)
        self.assertIn("VERDICT: PROVED", r.stdout)


class MidSentenceTruncation(ToolCase):
    """The 2026-09-02 failure, reproduced.

    A bullet's first line goes to one file and its continuation to the other.
    Line accounting is perfect.  Token accounting is perfect.  md5 of the
    concatenation would be perfect.  The sentence is in two pieces.
    """

    def fixture(self):
        boot = CLEAN_BOOT.replace(TAIL_LINE + "\n", "")
        ref = TAIL_LINE + "\n\n" + CLEAN_REF
        return self.split(boot=boot, ref=ref)

    def test_proof_3_goes_red(self):
        r = self.run_tool(*self.fixture(), expect=1)
        self.assertIn("VERDICT: DISPROVED", r.stdout)
        self.assertIn("PROOF 3", "\n".join(
            l for l in r.stdout.splitlines() if l.startswith("  FAILED:")))

    def test_the_seam_is_named_not_merely_counted(self):
        r = self.run_tool(*self.fixture(), expect=1)
        self.assertIn("INTRODUCED SEAM", r.stdout)
        self.assertIn(CUT_LINE, r.stdout,
                      "the half-sentence left behind must be quoted, not counted")
        self.assertIn("EDGE CUT", r.stdout)
        self.assertIn(TAIL_LINE.strip(), r.stdout,
                      "the orphaned continuation must be quoted too")

    def test_the_older_checks_would_have_passed_this(self):
        """This is why PROOF 3 exists.  If PROOF 1 or PROOF 2 could see this
        cut, the 09-02 split would have been caught by its own proof."""
        r = self.run_tool(*self.fixture(), expect=1)
        self.assertIn("1a ORIGINAL lines ABSENT after split: 0", r.stdout)
        self.assertIn("1b lines present but NOT DECLARED   : 0", r.stdout)
        self.assertIn("DELTA vs original                   : 0", r.stdout)
        self.assertIn("tokens LOST                         : 0", r.stdout)


class DroppedLine(ToolCase):
    def fixture(self):
        boot = CLEAN_BOOT.replace("The boot section closes here.\n\n", "")
        return self.split(boot=boot)

    def test_dropped_line_is_red_and_named(self):
        r = self.run_tool(*self.fixture(), expect=1)
        self.assertIn("VERDICT: DISPROVED", r.stdout)
        self.assertIn("ABSENT: 'The boot section closes here.'", r.stdout)
        self.assertIn("PROOF 1a: 1 original line(s) absent", r.stdout)

    def test_dropped_line_also_fails_the_partition(self):
        r = self.run_tool(*self.fixture(), expect=1)
        self.assertIn("order-preserving partition FAILED", r.stdout)
        self.assertIn("cut points not derivable", r.stdout,
                      "a failed alignment must say the cut list is untrustworthy "
                      "rather than print one anyway")


class ReorderedLine(ToolCase):
    """Present, but out of sequence.  A multiset check passes this.  That is
    exactly why the order-preserving subsequence property matters."""

    def fixture(self):
        boot = (CLEAN_BOOT
                .replace("This document explains the thing.", "\x00")
                .replace("Read these first.", "This document explains the thing.")
                .replace("\x00", "Read these first."))
        self.assertIn("Read these first.", boot)
        return self.split(boot=boot)

    def test_reorder_is_red(self):
        r = self.run_tool(*self.fixture(), expect=1)
        self.assertIn("VERDICT: DISPROVED", r.stdout)
        self.assertIn("PROOF 1d", r.stdout)
        self.assertIn("order-preserving partition FAILED", r.stdout)

    def test_only_the_order_check_can_see_it(self):
        r = self.run_tool(*self.fixture(), expect=1)
        self.assertIn("1a ORIGINAL lines ABSENT after split: 0", r.stdout)
        self.assertIn("1b lines present but NOT DECLARED   : 0", r.stdout)
        self.assertIn("tokens LOST                         : 0", r.stdout)
        self.assertIn("tokens GAINED undeclared            : 0", r.stdout)
        fails = [l for l in r.stdout.splitlines() if l.startswith("  FAILED:")]
        self.assertTrue(all("PROOF 1d" in l or "PROOF 3" in l for l in fails),
                        f"a multiset-visible failure leaked in: {fails}")

    def test_the_misplaced_line_is_named(self):
        r = self.run_tool(*self.fixture(), expect=1)
        self.assertIn("could not be placed in any output", r.stdout)
        self.assertIn("This document explains the thing.", r.stdout)


class UndeclaredAddition(ToolCase):
    def fixture(self):
        ref = CLEAN_REF + "\nAn entirely new sentence nobody declared.\n"
        return self.split(ref=ref)

    def test_undeclared_line_is_red_and_named(self):
        r = self.run_tool(*self.fixture(), expect=1)
        self.assertIn("VERDICT: DISPROVED", r.stdout)
        self.assertIn("UNDECLARED: 'An entirely new sentence nobody declared.'",
                      r.stdout)
        self.assertIn("PROOF 1b", r.stdout)

    def test_declaring_it_makes_it_green(self):
        """The positive control for the check above: the same line, declared,
        is accepted.  Otherwise 'red' could just mean 'refuses additions'."""
        ref = CLEAN_REF + "\nAn entirely new sentence nobody declared.\n"
        args = self.split(ref=ref, new="An entirely new sentence nobody declared.\n")
        r = self.run_tool(*args, expect=0)
        self.assertIn("VERDICT: PROVED", r.stdout)


class AmbiguousDeclaredNew(ToolCase):
    """A declared-new line whose text also occurs in the original.

    Removing declared-new lines from the outputs BY CONTENT (the draft's
    approach) strips every copy, including the original's, and then reports the
    original's copy as absent.  Deciding it by search gets both cases right.
    """

    DOC = "# T\n\nAlpha ends here.\n\n---\n\nBeta ends here.\n"

    def test_honest_duplicate_is_green(self):
        o = self.write("ORIG.md", self.DOC)
        a = self.write("a.md", "# T\n\nAlpha ends here.\n\n---\n")
        b = self.write("b.md", "---\n\nBeta ends here.\n")
        n = self.write("new.md", "---\n")
        r = self.run_tool("--original", o, "--output", a, "--output", b,
                          "--new", n, "--headings", expect=0)
        self.assertIn("VERDICT: PROVED", r.stdout)

    def test_dropping_the_original_copy_is_still_red(self):
        """Same declaration, but the original's own '---' really is gone.  The
        surviving copy must not be allowed to stand in for it."""
        o = self.write("ORIG.md", self.DOC)
        a = self.write("a.md", "# T\n\nAlpha ends here.\n")
        b = self.write("b.md", "---\n\nBeta ends here.\n")
        n = self.write("new.md", "---\n")
        r = self.run_tool("--original", o, "--output", a, "--output", b,
                          "--new", n, "--headings", expect=1)
        self.assertIn("VERDICT: DISPROVED", r.stdout)
        self.assertIn("DECLARED-NOT-USED", r.stdout)


class DerivedCutPoints(ToolCase):
    """The cut points derived from the alignment are not a prettier restatement
    of the seam analysis -- they see cuts the seam analysis structurally cannot.

    A paragraph seam only exists where a BLANK LINE does.  A file that joins two
    non-adjacent original lines with no blank between them has no seam there at
    all, and a sentence broken across that join is invisible to the whole
    PROOF 3 seam family.  The alignment knows the join happened, so the derived
    cut check catches it.  This fixture is built so the seam family reports
    nothing whatsoever and the cut check reports five BAD joins.
    """

    DOC = ("# Doc\n"
           "\n"
           "## Prose\n"
           "\n"
           "Runs across three lines and the first ends with a comma,\n"
           "the middle line continues it and also ends with a comma,\n"
           "and the third line finishes it.\n"
           "\n"
           "## Ref\n"
           "\n"
           "Reference text.\n")

    def fixture(self):
        o = self.write("ORIG.md", self.DOC)
        # boot takes original lines 1, 3, 5; ref takes 2, 4, 6, 7. Both streams
        # are ascending, so the partition is valid -- the prose sentence is
        # simply torn between them at joins that carry no blank line.
        a = self.write("boot.md",
                       "# Doc\n"
                       "\n"
                       "Runs across three lines and the first ends with a comma,\n"
                       "and the third line finishes it.\n")
        b = self.write("ref.md",
                       "## Prose\n"
                       "the middle line continues it and also ends with a comma,\n"
                       "## Ref\n"
                       "\n"
                       "Reference text.\n")
        return ["--original", o, "--output", a, "--output", b,
                "--no-new", "--headings"]

    def test_the_seam_family_is_blind_to_this_cut(self):
        """Stated as an assertion so that if the seam analysis ever grows to
        cover it, this test says so instead of quietly overlapping."""
        r = self.run_tool(*self.fixture(), expect=1)
        self.assertNotIn("SEAMS INTRODUCED BY THE SPLIT", r.stdout)
        self.assertNotIn("EDGE CUT", r.stdout)
        self.assertIn("seams INTRODUCED by the split: 0", r.stdout)

    def test_the_derived_cuts_catch_it_and_name_the_joins(self):
        r = self.run_tool(*self.fixture(), expect=1)
        self.assertIn("[BAD] JUNCTION", r.stdout)
        self.assertIn("[BAD] DIVERGENCE", r.stdout)
        self.assertIn("failing the predicate: 5", r.stdout)
        self.assertIn("derived cut point(s) fail the", r.stdout)
        self.assertIn("Runs across three lines and the first ends with a comma,",
                      r.stdout)

    def test_line_and_token_accounting_are_perfect_here(self):
        """The 09-02 signature again, at a different granularity: nothing is
        missing, nothing is added, and the prose is still in pieces."""
        r = self.run_tool(*self.fixture(), expect=1)
        self.assertIn("1a ORIGINAL lines ABSENT after split: 0", r.stdout)
        self.assertIn("tokens LOST                         : 0", r.stdout)
        self.assertIn("RESULT: PASS", r.stdout)


class MarkdownTableRowAtACut(ToolCase):
    """The sigil lane's false DISPROVED, reproduced.

    A markdown table row ends in '|'.  '|' is neither stripped as trailing
    decoration nor terminal punctuation, so a CONTENT row could never end a
    sentence anywhere in any document, and the separator-row escape (the
    fullmatch on pure markup) does not cover it, because a content row is not
    pure markup.  At a cut the pairing is new, so the ORIGINAL cross-check does
    not subtract it either: a correct cut whose last kept line was a table row
    was always DISPROVED.

    The asymmetry is the tell that this was an oversight rather than a rule:
    begins_sentence()'s STARTERS already contains "|", so the identical row on
    the AFTER side of the same seam was always accepted.

    WHY SKIPPING TABLE ROWS LOSES NOTHING: a table damaged by a split loses
    LINES, and line loss is PROOF 1's job (a mangled cell is PROOF 2's).
    PROOF 3 exists for PROSE torn mid-sentence, and a table row has no sentence
    to tear.  That is the argument against "restoring" this check later.
    """

    # The literal line from sigil 7fb76ba:docs/OVERSEER.md:104.  A paraphrase
    # would not prove the reported instance closed, so the row is pinned here
    # and a test below asserts the fixture really carries it.
    SIGIL_ROW = "| *(none)* | — | — | — | — |"

    DOC = ("# Doc\n"
           "\n"
           "## Table section\n"
           "\n"
           "| a | b | c | d | e |\n"
           "|---|---|---|---|---|\n"
           + SIGIL_ROW + "\n"
           "\n"
           "## Moved section\n"
           "\n"
           "Body of the moved section, a complete sentence.\n")

    def fixture(self, headings=False):
        o = self.write("ORIG.md", self.DOC)
        # A clean cut: the whole table stays, the whole moved section leaves.
        # Nothing is torn; every line is present exactly once.
        b = self.write("boot.md",
                       "# Doc\n"
                       "\n"
                       "## Table section\n"
                       "\n"
                       "| a | b | c | d | e |\n"
                       "|---|---|---|---|---|\n"
                       + self.SIGIL_ROW + "\n")
        f = self.write("ref.md",
                       "## Moved section\n"
                       "\n"
                       "Body of the moved section, a complete sentence.\n")
        args = ["--original", o, "--output", b, "--output", f, "--no-new"]
        if headings:
            args.append("--headings")
        return args

    def test_the_fixture_carries_the_literally_reported_row(self):
        """Guards the fixture itself: the reported instance, not a paraphrase."""
        self.assertIn("| *(none)* | — | — | — | — |", self.DOC)

    def test_the_reported_case_is_proved(self):
        r = self.run_tool(*self.fixture(), expect=0)
        self.assertIn("VERDICT: PROVED", r.stdout)
        self.assertNotIn("FAILED:", r.stdout)

    def test_neither_the_edge_nor_the_derived_cut_flags_the_row(self):
        """Both of the places the predicate is applied to a BEFORE side."""
        r = self.run_tool(*self.fixture(), expect=0)
        self.assertNotIn("EDGE CUT", r.stdout)
        self.assertNotIn("[BAD]", r.stdout)
        self.assertIn("failing the predicate: 0", r.stdout)

    def test_it_is_proved_under_headings_too(self):
        """The rescue is not a side effect of --headings: no heading is
        involved on the before side at all."""
        r = self.run_tool(*self.fixture(headings=True), expect=0)
        self.assertIn("VERDICT: PROVED", r.stdout)

    def test_the_after_side_was_already_accepted(self):
        """States the asymmetry as an assertion, so a later edit that drops
        '|' from STARTERS is caught here rather than in a lane's gate."""
        sys.path.insert(0, str(TOOL.parent))
        import prove_doc_split as P
        self.assertTrue(P.begins_sentence(self.SIGIL_ROW))
        self.assertTrue(P.make_ends_sentence(False)(self.SIGIL_ROW))


class TornSentenceBesideATable(ToolCase):
    """The control on that fix: the suppression must not be over-broad.

    The 2026-09-02 defect -- a bullet truncated mid-sentence with its
    continuation in the other file -- must STILL be caught when it happens next
    to a table.  If "not a sentence" is allowed to grow, that defect becomes
    invisible again, which is the one way this fix could do harm.
    """

    DOC = ("# Doc\n"
           "\n"
           "## Table section\n"
           "\n"
           "| a | b |\n"
           "|---|---|\n"
           "| *(none)* | — |\n"
           "\n"
           "- The rule is long enough to wrap, and so it ends with a comma,\n"
           "  continuing on this second line to a full stop.\n"
           "\n"
           "## Moved section\n"
           "\n"
           "Body of the moved section, a complete sentence.\n")

    def fixture(self):
        o = self.write("ORIG.md", self.DOC)
        b = self.write("boot.md",
                       "# Doc\n"
                       "\n"
                       "## Table section\n"
                       "\n"
                       "| a | b |\n"
                       "|---|---|\n"
                       "| *(none)* | — |\n"
                       "\n"
                       "- The rule is long enough to wrap, and so it ends with a comma,\n")
        f = self.write("ref.md",
                       "  continuing on this second line to a full stop.\n"
                       "\n"
                       "## Moved section\n"
                       "\n"
                       "Body of the moved section, a complete sentence.\n")
        return ["--original", o, "--output", b, "--output", f, "--no-new"]

    # The truncated half.  Named once, because the assertions below must quote
    # the whole flagged line and not merely find this text somewhere in stdout.
    TORN = "- The rule is long enough to wrap, and so it ends with a comma,"
    TAIL = "  continuing on this second line to a full stop."

    def test_the_torn_bullet_is_still_red_and_quoted(self):
        r = self.run_tool(*self.fixture(), expect=1)
        self.assertIn("VERDICT: DISPROVED", r.stdout)
        self.assertIn(self.TORN, r.stdout)
        self.assertIn(self.TAIL.strip(), r.stdout)

    def test_the_BEFORE_side_is_what_flags_it(self):
        """This is the assertion that bites on an over-broad fix.

        The earlier draft of this class asserted only that the torn text
        appeared in stdout and that the run was red.  Both survive a poisoned
        `is_table_row` that returns True for EVERY line: the run still reds off
        the AFTER side (ref.md starts with a lowercase continuation) and the
        derived cut still PRINTS the torn line, as '[OK ]'.  So the class was
        passing for the wrong reason and could not have caught the very
        regression it exists to catch.  Proven by poison, 2026-09-04.

        The assertion that actually bites is the boot.md EDGE CUT, which names
        the BEFORE side: under that poison the edge count drops 2 -> 1 and this
        line is gone.  The derived cut stays [BAD] even poisoned, because the
        AFTER side of that same join fails independently -- so it is asserted
        as context, not relied on.  The class below removes that crutch
        entirely.
        """
        r = self.run_tool(*self.fixture(), expect=1)
        self.assertIn(f"EDGE CUT [boot.md] ends mid-sentence: {self.TORN!r}",
                      r.stdout)
        self.assertIn("[BAD] DIVERGENCE", r.stdout)
        self.assertIn("failing the predicate: 1", r.stdout)

    def test_a_tear_only_the_before_side_can_see_is_still_red(self):
        """The strongest form of the control: the BEFORE side is the ONLY
        reason this split is red.

        Same tear, but the orphaned continuation begins with a capital, so
        begins_sentence() accepts it and the after side reports nothing.  Every
        red here comes from the sentence predicate applied to a line that sits
        in a document full of table rows.  Poison is_table_row to return True
        for every line and this fixture goes PROVED -- the 09-02 defect,
        invisible again -- so this test is what stands between the two.
        """
        doc = ("# Doc\n"
               "\n"
               "## Table section\n"
               "\n"
               "| a | b |\n"
               "|---|---|\n"
               "| *(none)* | — |\n"
               "\n"
               "- The rule is long enough to wrap, and so it ends with a comma,\n"
               "  Sonic then continues on this second line to a full stop.\n")
        o = self.write("ORIG2.md", doc)
        b = self.write("boot2.md",
                       "# Doc\n"
                       "\n"
                       "## Table section\n"
                       "\n"
                       "| a | b |\n"
                       "|---|---|\n"
                       "| *(none)* | — |\n"
                       "\n"
                       "- The rule is long enough to wrap, and so it ends with a comma,\n")
        f = self.write("ref2.md",
                       "  Sonic then continues on this second line to a full stop.\n")
        r = self.run_tool("--original", o, "--output", b, "--output", f,
                          "--no-new", expect=1)
        self.assertIn("VERDICT: DISPROVED", r.stdout)
        self.assertIn(f"EDGE CUT [boot2.md] ends mid-sentence: {self.TORN!r}",
                      r.stdout)
        self.assertIn("failing the predicate: 1", r.stdout)
        # ... and nothing was flagged on the after side, so there is no other
        # check that could have carried this red.
        self.assertNotIn("starts mid-sentence", r.stdout)

    def test_the_older_proofs_are_blind_to_it_here_too(self):
        """The 09-02 signature again: lines and tokens all account for."""
        r = self.run_tool(*self.fixture(), expect=1)
        self.assertIn("1a ORIGINAL lines ABSENT after split: 0", r.stdout)
        self.assertIn("tokens LOST                         : 0", r.stdout)

    def test_the_table_row_in_the_same_document_is_not_what_reds_it(self):
        """Every cut quoted must be about the bullet, never about the table."""
        r = self.run_tool(*self.fixture(), expect=1)
        quoted = [l for l in r.stdout.splitlines()
                  if "EDGE CUT" in l or l.strip().startswith(("before:", "after :"))]
        self.assertTrue(quoted, r.stdout)
        self.assertFalse([l for l in quoted if "*(none)*" in l],
                         "the table row must not appear as a flagged cut:\n"
                         + "\n".join(quoted))


class VerdictCarriesBothHeadingNumbers(ToolCase):
    """Defect 2: the verdict was gated on ONE of two printed numbers.

    A lane reading only the gated figure can accept a cut whose other flagged
    sites are not heading-adjacent and never learn there were more.  So the
    verdict itself carries BOTH figures and says which gated and why.
    """

    DOC = ("# Doc\n\n## A\n\nBody A ends here.\n\n## B\n\nBody B ends here.\n"
           "\n## C\n\nBody C ends here.\n\n## D\n\nBody D ends here.\n")

    def fixture(self, headings):
        o = self.write("ORIG.md", self.DOC)
        # boot takes original non-blank lines 1,2,6,8,9; ref takes 3,4,5,7.
        # That puts two heading-to-heading seams in boot that the original
        # never had: heading-BLIND flags both, heading-AWARE flags neither.
        b = self.write("boot.md",
                       "# Doc\n\n## A\n\n## C\n\n## D\n\nBody D ends here.\n")
        f = self.write("ref.md",
                       "Body A ends here.\n\n## B\n\nBody B ends here.\n"
                       "\nBody C ends here.\n")
        args = ["--original", o, "--output", b, "--output", f, "--no-new"]
        if headings:
            args.append("--headings")
        return args

    def verdict_note(self, out):
        lines = out.splitlines()
        got = [l for l in lines
               if "heading-aware" in l and "heading-blind" in l and "GATED" in l]
        self.assertEqual(len(got), 1,
                         "expected exactly one verdict-adjacent line carrying "
                         "both figures and naming the gate; got:\n" + out)
        idx = lines.index(got[0])
        verdicts = [i for i, l in enumerate(lines) if l.startswith("VERDICT:")]
        self.assertEqual(len(verdicts), 1, out)
        self.assertLessEqual(abs(idx - verdicts[0]), 1,
                             "the note must sit beside the VERDICT line, not "
                             "hundreds of lines above it")
        return got[0]

    def test_both_values_are_stated_beside_the_verdict(self):
        r = self.run_tool(*self.fixture(headings=True), expect=0)
        note = self.verdict_note(r.stdout)
        self.assertIn("heading-aware 0", note)
        self.assertIn("heading-blind 2", note)

    def test_the_note_says_which_gated_and_why(self):
        r = self.run_tool(*self.fixture(headings=True), expect=0)
        note = self.verdict_note(r.stdout)
        self.assertIn("heading-aware GATED", note)
        self.assertIn("--headings", note)

    def test_the_ungated_figure_is_the_larger_one_and_is_not_hidden(self):
        """The whole point: PROVED, and the reader still sees the 2."""
        r = self.run_tool(*self.fixture(headings=True), expect=0)
        self.assertIn("VERDICT: PROVED", r.stdout)
        self.assertIn("heading-blind 2", self.verdict_note(r.stdout))

    def test_the_same_two_values_appear_when_the_other_number_gates(self):
        """Without --headings the blind number gates; both figures are still
        carried, with the same values, and the note names the other gate."""
        r = self.run_tool(*self.fixture(headings=False), expect=1)
        note = self.verdict_note(r.stdout)
        self.assertIn("heading-aware 0", note)
        self.assertIn("heading-blind 2", note)
        self.assertIn("heading-blind GATED", note)


class Unmeasurable(ToolCase):
    """A thing this instrument could not measure must never render as 0, and
    must never render as a plain red either: exit 2, named cause, empty
    stdout."""

    def test_missing_original(self):
        b = self.write("boot.md", CLEAN_BOOT)
        f = self.write("ref.md", CLEAN_REF)
        r = self.run_tool("--original", self.d / "nope.md", "--output", b,
                          "--output", f, "--no-new", expect=2)
        self.assertEqual(r.stdout, "",
                         "a partial report would be read as a partial proof")
        self.assertIn("UNMEASURABLE", r.stderr)
        self.assertIn("nope.md", r.stderr)

    def test_missing_output(self):
        o = self.write("ORIG.md", ORIGINAL)
        b = self.write("boot.md", CLEAN_BOOT)
        r = self.run_tool("--original", o, "--output", b,
                          "--output", self.d / "gone.md", "--no-new", expect=2)
        self.assertEqual(r.stdout, "")
        self.assertIn("gone.md", r.stderr)

    def test_missing_declared_new_file(self):
        o = self.write("ORIG.md", ORIGINAL)
        b = self.write("boot.md", CLEAN_BOOT)
        f = self.write("ref.md", CLEAN_REF)
        r = self.run_tool("--original", o, "--output", b, "--output", f,
                          "--new", self.d / "absent-frag.md", expect=2)
        self.assertEqual(r.stdout, "")
        self.assertIn("absent-frag.md", r.stderr)

    def test_directory_given_as_a_document(self):
        b = self.write("boot.md", CLEAN_BOOT)
        f = self.write("ref.md", CLEAN_REF)
        r = self.run_tool("--original", self.d, "--output", b, "--output", f,
                          "--no-new", expect=2)
        self.assertIn("is a directory", r.stderr)

    def test_unreadable_file(self):
        o = self.write("ORIG.md", ORIGINAL)
        b = self.write("boot.md", CLEAN_BOOT)
        f = self.write("ref.md", CLEAN_REF)
        os.chmod(b, 0o000)
        self.addCleanup(os.chmod, b, 0o600)
        if os.access(b, os.R_OK):        # running as root: the mode means nothing
            self.skipTest("cannot make a file unreadable as this user")
        r = self.run_tool("--original", o, "--output", b, "--output", f,
                          "--no-new", expect=2)
        self.assertEqual(r.stdout, "")
        self.assertIn("cannot read", r.stderr)

    def test_non_utf8_file(self):
        o = self.write("ORIG.md", ORIGINAL)
        b = self.d / "boot.md"
        b.write_bytes(b"\xff\xfe\x00binary not text\x00")
        f = self.write("ref.md", CLEAN_REF)
        r = self.run_tool("--original", o, "--output", b, "--output", f,
                          "--no-new", expect=2)
        self.assertIn("not UTF-8 text", r.stderr)

    def test_new_declaration_is_mandatory(self):
        """Neither --new nor --no-new must not silently mean 'no new lines':
        that would make a forgotten declaration read as a STRICTER proof."""
        o = self.write("ORIG.md", ORIGINAL)
        b = self.write("boot.md", CLEAN_BOOT)
        f = self.write("ref.md", CLEAN_REF)
        r = self.run_tool("--original", o, "--output", b, "--output", f,
                          expect=2)
        self.assertIn("exactly one of --new", r.stderr)

    def test_new_and_no_new_together_is_refused(self):
        o = self.write("ORIG.md", ORIGINAL)
        b = self.write("boot.md", CLEAN_BOOT)
        f = self.write("ref.md", CLEAN_REF)
        n = self.write("new.md", "whatever\n")
        r = self.run_tool("--original", o, "--output", b, "--output", f,
                          "--new", n, "--no-new", expect=2)
        self.assertIn("exactly one of --new", r.stderr)

    def test_state_cap_is_reported_not_silently_accepted(self):
        """Undecidable ambiguity is an exit 2 with a named cause, never a
        green.  Forced with an absurd cap on an otherwise clean split."""
        o = self.write("ORIG.md", self.DOC_AMBIG)
        a = self.write("a.md", "x\n\n---\n\n---\n\n---\n\ny\n")
        b = self.write("b.md", "---\n\n---\n\n---\n\nz\n")
        n = self.write("new.md", "---\n---\n")
        r = self.run_tool("--original", o, "--output", a, "--output", b,
                          "--new", n, "--state-cap", "1", expect=2)
        self.assertIn("ambiguity exceeded", r.stderr)

    DOC_AMBIG = "x\n\n---\n\n---\n\n---\n\ny\n\n---\n\nz\n"


class GitRevisionOriginal(ToolCase):
    """'rev:path' must really go through git.  Reading 'the original' off the
    working tree is how a proof ends up comparing the change to itself."""

    def make_repo(self):
        g = self.d / "repo"
        g.mkdir()
        def run(*a):
            subprocess.run(["git", "-C", str(g), *a], check=True,
                           capture_output=True)
        run("init", "-q")
        run("config", "user.email", "t@example.invalid")
        run("config", "user.name", "t")
        (g / "doc.md").write_text(ORIGINAL, encoding="utf-8")
        run("add", "doc.md")
        run("commit", "-qm", "original")
        # Now DESTROY the working-tree copy, so a green can only come from git.
        (g / "doc.md").write_text("# totally different\n", encoding="utf-8")
        return g

    def test_original_is_read_from_the_commit_not_the_worktree(self):
        g = self.make_repo()
        b = self.write("boot.md", CLEAN_BOOT)
        f = self.write("ref.md", CLEAN_REF)
        r = self.run_tool("--original", "HEAD:doc.md", "--repo", g,
                          "--output", b, "--output", f, "--no-new",
                          "--headings", expect=0)
        self.assertIn("original : git HEAD:doc.md", r.stdout)
        self.assertIn("VERDICT: PROVED", r.stdout)

    def test_bad_revision_is_named_not_swallowed(self):
        g = self.make_repo()
        b = self.write("boot.md", CLEAN_BOOT)
        f = self.write("ref.md", CLEAN_REF)
        r = self.run_tool("--original", "no-such-rev:doc.md", "--repo", g,
                          "--output", b, "--output", f, "--no-new", expect=2)
        self.assertEqual(r.stdout, "")
        self.assertIn("no-such-rev:doc.md", r.stderr)
        self.assertIn("git", r.stderr)

    def test_a_bare_missing_path_says_it_is_not_a_git_spelling(self):
        b = self.write("boot.md", CLEAN_BOOT)
        f = self.write("ref.md", CLEAN_REF)
        r = self.run_tool("--original", "nowhere.md", "--output", b,
                          "--output", f, "--no-new", expect=2)
        self.assertIn("not a git revision spelling", r.stderr)


class ProvenanceIdentifiesTheTree(ToolCase):
    """The provenance must name WHICH FILES WERE READ, not which arguments were
    typed.

    Measured on 2026-09-04, both real: this tool run from an oracle checkout
    against oracle's already-split OVERSEER.md, and run from aeon's repo against
    aeon's unsplit one, printed the same three provenance lines --
    ``original : git main:docs/OVERSEER.md (repo .)`` -- and both exited 0.  One
    of those runs measured the document under test; the other measured a
    different repository's already-finished work.  A lane pasting either
    transcript as its evidence produces a green that a reader cannot attribute
    to a tree.  That is a confident verdict about an unknown subject, which is
    the entire failure class this instrument exists to end.

    So the tests below do not ask "does an absolute path appear" -- that is
    satisfied by any absolute path, including the SAME one printed twice, which
    is precisely the regression.  They run the tool twice against two different
    trees holding same-named documents, with identical argv, and require the two
    transcripts to differ BY NAMING THEIR OWN TREE.
    """

    ORIG = "# Handbook {m}\n\nAlpha sentence here.\n\n## Ref\n\nBeta sentence here.\n"
    BOOT = "# Handbook {m}\n\nAlpha sentence here.\n"
    REF = "## Ref\n\nBeta sentence here.\n"

    def make_tree(self, name, marker):
        """A directory holding a document, its split, and nothing tree-specific
        in the FILE NAMES -- only in the paths and the content."""
        t = self.d / name
        t.mkdir()
        (t / "ORIG.md").write_text(self.ORIG.format(m=marker), encoding="utf-8")
        (t / "boot.md").write_text(self.BOOT.format(m=marker), encoding="utf-8")
        (t / "ref.md").write_text(self.REF, encoding="utf-8")
        return t.resolve()

    def init_repo(self, path, marker):
        def run(*a):
            subprocess.run(["git", "-C", str(path), *a], check=True,
                           capture_output=True)
        run("init", "-q")
        run("config", "user.email", "t@example.invalid")
        run("config", "user.name", "t")
        (path / "doc.md").write_text(self.ORIG.format(m=marker), encoding="utf-8")
        run("add", "doc.md")
        run("commit", "-qm", "original")
        return path

    @staticmethod
    def inputs_block(stdout):
        """Everything up to the first proof heading: the part a lane pastes."""
        out = []
        for line in stdout.splitlines():
            if line.startswith("PROOF 1"):
                break
            out.append(line)
        return "\n".join(out)

    @staticmethod
    def paths_in(text):
        """Whole path tokens.  ``assertIn(str(parent), text)`` is satisfied by a
        printed CHILD path, so a report naming only `/repo/nested/deeper` would
        pass an assertion that `/repo` was named.  Comparing complete tokens is
        what makes 'the toplevel is stated' mean it."""
        return set(re.findall(r"/[^\s'\"(),;]+", text))

    @staticmethod
    def field(stdout, prefix):
        for line in stdout.splitlines():
            if line.strip().startswith(prefix):
                return line.strip()
        return ""

    # -- plain-path documents ------------------------------------------------

    def test_same_argv_against_two_trees_yields_distinguishable_provenance(self):
        a = self.make_tree("alpha", "A")
        b = self.make_tree("beta", "B")
        argv = ["--original", "ORIG.md", "--output", "boot.md",
                "--output", "ref.md", "--no-new", "--headings", "--repo", "."]
        ra = self.run_tool(*argv, cwd=a, expect=0)
        rb = self.run_tool(*argv, cwd=b, expect=0)

        # Both are honest, lossless splits: the verdict cannot be what tells
        # them apart, which is exactly why the provenance has to.
        self.assertIn("VERDICT: PROVED", ra.stdout)
        self.assertIn("VERDICT: PROVED", rb.stdout)

        ia, ib = self.inputs_block(ra.stdout), self.inputs_block(rb.stdout)
        self.assertNotEqual(
            ia, ib,
            "two runs measuring different documents printed identical "
            f"provenance:\n{ia}")
        self.assertIn(str(a), ia)
        self.assertNotIn(str(b), ia,
                         "tree alpha's report names tree beta")
        self.assertIn(str(b), ib)
        self.assertNotIn(str(a), ib,
                         "tree beta's report names tree alpha")

    def test_each_output_is_reported_by_resolved_path(self):
        a = self.make_tree("alpha", "A")
        b = self.make_tree("beta", "B")
        argv = ["--original", "ORIG.md", "--output", "boot.md",
                "--output", "ref.md", "--no-new", "--headings"]
        ra = self.run_tool(*argv, cwd=a, expect=0)
        rb = self.run_tool(*argv, cwd=b, expect=0)
        for r, mine, theirs in ((ra, a, b), (rb, b, a)):
            outs = [l for l in r.stdout.splitlines()
                    if l.strip().startswith("output   :")]
            self.assertEqual(len(outs), 2, r.stdout)
            for line in outs:
                self.assertIn(str(mine), line,
                              "an output line that does not name its tree is "
                              "the same line for every tree")
                self.assertNotIn(str(theirs), line)
            self.assertIn(str(mine / "boot.md"), r.stdout)
            self.assertIn(str(mine / "ref.md"), r.stdout)

    def test_declared_new_files_are_reported_by_resolved_path(self):
        a = self.make_tree("alpha", "A")
        b = self.make_tree("beta", "B")
        for t in (a, b):
            (t / "ref.md").write_text(
                self.REF + "\nA declared new sentence.\n", encoding="utf-8")
            (t / "new.md").write_text("A declared new sentence.\n",
                                      encoding="utf-8")
        argv = ["--original", "ORIG.md", "--output", "boot.md",
                "--output", "ref.md", "--new", "new.md", "--headings"]
        ra = self.run_tool(*argv, cwd=a, expect=0)
        rb = self.run_tool(*argv, cwd=b, expect=0)
        la = self.field(ra.stdout, "declared new :")
        lb = self.field(rb.stdout, "declared new :")
        self.assertNotEqual(la, lb, f"identical declared-new provenance: {la}")
        self.assertIn(str(a / "new.md"), la)
        self.assertIn(str(b / "new.md"), lb)

    # -- git rev:path originals ---------------------------------------------

    def test_a_git_original_names_the_tree_it_was_read_from(self):
        """The measured defect itself: identical `rev:path`, identical `--repo`
        spelling, two repositories, two different documents."""
        a = self.init_repo(self.make_tree("alpha", "A"), "A")
        b = self.init_repo(self.make_tree("beta", "B"), "B")
        argv = ["--original", "HEAD:doc.md", "--output", "boot.md",
                "--output", "ref.md", "--no-new", "--headings", "--repo", "."]
        ra = self.run_tool(*argv, cwd=a, expect=0)
        rb = self.run_tool(*argv, cwd=b, expect=0)
        la = self.field(ra.stdout, "original :")
        lb = self.field(rb.stdout, "original :")
        # The git spelling is identical in both -- so it cannot be doing the
        # distinguishing, and the line must carry the tree as well.
        self.assertIn("git HEAD:doc.md", la)
        self.assertIn("git HEAD:doc.md", lb)
        self.assertNotEqual(
            la, lb,
            f"two repositories, one provenance line: {la}")
        self.assertIn(str(a), la)
        self.assertNotIn(str(b), la)
        self.assertIn(str(b), lb)
        self.assertNotIn(str(a), lb)

    def test_repo_is_reported_resolved_never_as_the_dot_that_was_typed(self):
        a = self.init_repo(self.make_tree("alpha", "A"), "A")
        r = self.run_tool("--original", "HEAD:doc.md", "--output", "boot.md",
                          "--output", "ref.md", "--no-new", "--headings",
                          "--repo", ".", cwd=a, expect=0)
        block = self.inputs_block(r.stdout)
        self.assertIn(str(a), block)
        self.assertNotIn("(repo .)", block,
                         "the literal argument is not an identity: every run "
                         "everywhere prints '.'")

    def test_a_subdirectory_repo_reports_the_git_tree_it_actually_used(self):
        """`--repo` is not necessarily the repository: `git -C` searches upward.
        Two checkouts at different paths are common on this machine and
        worktrees make it commonplace, so the run states the toplevel git
        actually read from, not merely the directory it was pointed at."""
        a = self.init_repo(self.make_tree("alpha", "A"), "A")
        sub = a / "nested" / "deeper"
        sub.mkdir(parents=True)
        r = self.run_tool("--original", "HEAD:doc.md",
                          "--output", a / "boot.md", "--output", a / "ref.md",
                          "--no-new", "--headings", "--repo", sub, expect=0)
        block = self.inputs_block(r.stdout)
        self.assertIn(str(sub), self.paths_in(block),
                      "the directory given must still show")
        self.assertIn(str(a), self.paths_in(block),
                      "the git toplevel actually read from must be stated, as "
                      "itself and not merely as a prefix of the subdirectory")
        self.assertIn(str(a), self.paths_in(self.field(r.stdout, "original :")),
                      "the original line alone must identify its tree, because "
                      "that is the line a lane quotes")

    def test_a_non_git_repo_directory_is_labelled_not_silently_blank(self):
        """No git tree is a fact about the run, not a reason to print nothing:
        a blank would read as 'the repo line is missing' to a lane comparing
        transcripts."""
        a = self.make_tree("alpha", "A")
        probe = subprocess.run(["git", "-C", str(a), "rev-parse",
                                "--show-toplevel"], capture_output=True)
        if probe.returncode == 0:
            # The tempdir landed inside somebody's checkout; say so rather than
            # asserting a fact that is not true of this run.
            self.skipTest(f"tempdir {a} is inside a git tree")
        r = self.run_tool("--original", "ORIG.md", "--output", "boot.md",
                          "--output", "ref.md", "--no-new", "--headings",
                          "--repo", ".", cwd=a, expect=0)
        block = self.inputs_block(r.stdout)
        self.assertIn(str(a), block)
        self.assertIn("not a git working tree", block)


class RunnerIsWired(unittest.TestCase):
    """A test nothing runs is the defect this parcel exists to fix."""

    def test_runner_exists_and_names_this_suite(self):
        self.assertTrue(RUNNER.exists(), f"{RUNNER} is missing")
        self.assertTrue(os.access(RUNNER, os.X_OK), f"{RUNNER} is not executable")
        self.assertIn("test_prove_doc_split.py", RUNNER.read_text(encoding="utf-8"))

    def test_tool_is_executable(self):
        self.assertTrue(os.access(TOOL, os.X_OK), f"{TOOL} is not executable")


if __name__ == "__main__":
    unittest.main(verbosity=2)
