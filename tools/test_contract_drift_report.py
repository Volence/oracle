#!/usr/bin/env python3
"""Tests for tools/contract_drift_report.py.

An instrument that cannot fail is worse than no instrument, because its presence and its absence read
the same.  This one exists to answer a question nothing else in the repo answers — *has the contract
repo moved past our pin?* — so the point of this suite is not "does it say SAME when nothing moved".
It is:

  * does it say DRIFTED, by name, on each way the peer can move; and
  * does it say UNMEASURABLE, by name, on each way the measurement can fail, and NEVER render that as
    SAME, as zero, or as "no drift".

WHY THE SECOND HALF IS THE LOAD-BEARING HALF, AND WHY IT IS HERE RATHER THAN IN THE BACKTEST
-------------------------------------------------------------------------------------------
`contract_drift_report.py --backtest` replays the shipped detector over the contract repo's REAL
history and measures 78/78 on the first-parent line of `origin/main`.  That measurement has a hole it
states itself: `git log --full-history -M --diff-filter=R` over `contract/schema/` is EMPTY across the
whole history, so **the rename/delete class has zero instances upstream** and the backtest's 100% says
nothing at all about it.  A population that lacks a class cannot measure a detector's behaviour on that
class.  So the classes history does not supply are constructed here, one fixture each.

Every fixture is a synthetic git repository built in a tempdir.  Nothing here reads
`/home/volence/sonic_hacks/empyrean`: that repo moves under a run — the hub commits to it hourly — and
a test pinned to it would rot into a false red or, worse, a green that measured a different revision
than the one it names.  That is the same hazard `F-SCHEMA-READS-LIVE-EMPYREAN` registered, one layer
out.

`$EMPYREAN_DIR` and `$EMPYREAN_SUITE_ROOT` are scrubbed from the environment of every test that does
not set them deliberately.  A resolver suite that inherits the developer's environment tests the
developer's machine (`empyrean/contract/SUITE_PATHS.md`: a row that owns the unset path must be
"constructed rather than ambient").

The failure modes covered, one class each:

  CleanControl              nothing moved -> SAME.  Without this control the suite could pass by
                            refusing everything, which is how a detector reaches 100% recall and 0%
                            usefulness.
  ContentDrift              the peer wrote new bytes -> DRIFTED, with the changed leaf path named.
  SemanticallyNullDrift     bytes moved, the parsed document did not (empyrean 47e77ec, the one real
                            instance in 97 historical events) -> DRIFTED, labelled semantically null.
  RenamedUpstream           the path moved upstream -> UNMEASURABLE.  ZERO instances in real history;
                            this fixture is the only coverage that class has.
  DeletedUpstream           the path was removed -> UNMEASURABLE.  Likewise zero instances upstream.
  RefDoesNotResolve         no origin/main in the peer (unfetched clone) -> UNMEASURABLE, and the
                            message says how to fix it.
  NoCheckoutFound           no peer at all -> refusal naming every candidate tried.
  SetButWrongIsHardError    $EMPYREAN_DIR pointing at a non-checkout stops there; it does NOT fall
                            through to the suite root, which would hide a wrong environment.
  FetchFailureIsStamped     a peer with no reachable remote -> the fetch failure is named and every
                            verdict is stamped as being against a local mirror.
  StaleMirrorIsStamped      --no-fetch says so, and says the tip's COMMIT date is not its FETCH date.
  MissingPinMarker          PROVENANCE.md lost a pin -> the report refuses; it does not find no drift.
  BacktestCounts            the backtest's own arithmetic, against a fixture whose event count is
                            fixed by construction rather than copied from a run.
  BacktestNoOpMerge         a merge that re-resolves the path to its first parent's blob -> counted as
                            touch-no-change and NOT as a false positive.  Real history has zero of
                            these -- STRUCTURALLY, because git records a path in a commit's diff only
                            when its blob or mode changes -- so upstream can never supply a non-empty
                            denominator for that rate and this fixture is its ONLY measured coverage.
  BacktestEmptyPopulation   a path with no history -> "the backtest cannot run on it", never 100%.
"""

import argparse
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stdout

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import contract_drift_report as mod  # noqa: E402

SCHEMA_PATH = "contract/schema/bus-protocol.schema.json"
VECTORS_PATH = "contract/schema/tests/vectors.json"


# ---------------------------------------------------------------------------------------------------
# Fixture plumbing
# ---------------------------------------------------------------------------------------------------


def git(repo, *args, check=True):
    p = subprocess.run(
        ["git", "-C", repo] + list(args), capture_output=True, check=False, text=True
    )
    if check and p.returncode != 0:
        raise AssertionError("git %s failed in %s: %s" % (" ".join(args), repo, p.stderr))
    return p.stdout.strip()


def write(repo, rel, text):
    full = os.path.join(repo, rel)
    os.makedirs(os.path.dirname(full), exist_ok=True)
    with open(full, "w", encoding="utf-8") as fh:
        fh.write(text)


def init_peer(root):
    """A minimal contract repo carrying both vendored paths, with `origin/main` set.

    `refs/remotes/origin/main` is written directly rather than by cloning: the tool reads a
    remote-tracking ref, and what it must handle is that ref existing, not a network.
    """
    repo = os.path.join(root, "empyrean")
    os.makedirs(repo)
    git(repo, "init", "-q", "-b", "main")
    git(repo, "config", "user.email", "t@example.invalid")
    git(repo, "config", "user.name", "test")
    return repo


def commit(repo, message):
    git(repo, "add", "-A")
    git(repo, "commit", "-q", "-m", message)
    git(repo, "update-ref", "refs/remotes/origin/main", "HEAD")
    return git(repo, "rev-parse", "HEAD")


def blob_of(repo, rev, path):
    return git(repo, "rev-parse", "%s:%s" % (rev, path))


def provenance(root, schema_blob, vectors_blob, revision, schema_bytes=1, vectors_bytes=1,
               drop=None):
    """A PROVENANCE.md carrying only the six markers the tool parses."""
    lines = [
        "# fixture provenance",
        "",
        "    pin.revision = %s" % revision,
        "    pin.blob     = %s" % schema_blob,
        "    pin.bytes    = %d" % schema_bytes,
        "",
        "    pin.vectors.revision = %s" % revision,
        "    pin.vectors.blob     = %s" % vectors_blob,
        "    pin.vectors.bytes    = %d" % vectors_bytes,
    ]
    if drop:
        lines = [ln for ln in lines if not ln.strip().startswith("pin.%s " % drop)
                 and not ln.strip().startswith("pin.%s=" % drop)]
    p = os.path.join(root, "PROVENANCE.md")
    with open(p, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")
    return p


def run_report(peer, prov_path, fetch=False, ref="origin/main", empyrean=None):
    """Call the shipped `report()` in-process and capture what a reader would see."""
    args = argparse.Namespace(
        empyrean=empyrean if empyrean is not None else peer,
        ref=ref,
        fetch=fetch,
        fetch_timeout=5,
        json=False,
        backtest=False,
        backtest_json=None,
    )
    buf = io.StringIO()
    old = mod.PROVENANCE
    mod.PROVENANCE = prov_path
    try:
        with redirect_stdout(buf):
            rc = mod.report(args)
    finally:
        mod.PROVENANCE = old
    return rc, buf.getvalue()


def run_backtest(peer, ref="origin/main"):
    args = argparse.Namespace(
        empyrean=peer, ref=ref, fetch=False, fetch_timeout=5, json=False,
        backtest=True, backtest_json=None,
    )
    buf = io.StringIO()
    with redirect_stdout(buf):
        rc = mod.backtest(args)
    return rc, buf.getvalue()


class Scrubbed(unittest.TestCase):
    """Base class: no test inherits the developer's suite-path environment."""

    def setUp(self):
        self._saved = {k: os.environ.pop(k, None) for k in ("EMPYREAN_DIR", "EMPYREAN_SUITE_ROOT")}
        self.tmp = tempfile.TemporaryDirectory()
        self.root = self.tmp.name
        self.addCleanup(self.tmp.cleanup)

    def tearDown(self):
        for k, v in self._saved.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v


# ---------------------------------------------------------------------------------------------------
# The three verdicts
# ---------------------------------------------------------------------------------------------------


class TestVerdicts(Scrubbed):
    def build(self, schema_text='{"a": 1}\n', vectors_text='{"cases": []}\n'):
        peer = init_peer(self.root)
        write(peer, SCHEMA_PATH, schema_text)
        write(peer, VECTORS_PATH, vectors_text)
        rev = commit(peer, "add the contract")
        return peer, rev

    def test_clean_control_nothing_moved_is_SAME(self):
        """The control.  A suite of red-only rows can be satisfied by a detector that refuses all."""
        peer, rev = self.build()
        prov = provenance(
            self.root, blob_of(peer, rev, SCHEMA_PATH), blob_of(peer, rev, VECTORS_PATH), rev
        )
        rc, out = run_report(peer, prov)
        self.assertEqual(rc, 0)
        self.assertIn("SUMMARY: 2 same, 0 DRIFTED, 0 unmeasurable", out)
        self.assertNotIn("DRIFTED\n", out.replace("0 DRIFTED", ""))

    def test_content_drift_is_named_with_the_changed_leaf_path(self):
        peer, rev0 = self.build()
        pinned_schema = blob_of(peer, rev0, SCHEMA_PATH)
        pinned_vectors = blob_of(peer, rev0, VECTORS_PATH)
        write(peer, SCHEMA_PATH, '{"a": 2}\n')
        commit(peer, "the peer moves on")
        prov = provenance(self.root, pinned_schema, pinned_vectors, rev0)
        rc, out = run_report(peer, prov)
        self.assertEqual(rc, 0, "a reporter never fails a build")
        self.assertIn("DRIFTED", out)
        self.assertIn("SUMMARY: 1 same, 1 DRIFTED, 0 unmeasurable", out)
        self.assertIn("$.a", out, "the delta must name the leaf path that moved")
        self.assertIn("1 changed", out)

    def test_bytes_moved_but_the_document_did_not_is_drift_labelled_semantically_null(self):
        """empyrean 47e77ec, the one instance in 97 historical events, reproduced.

        It is real drift for a byte-pinned copy — our gate hashes bytes — and it is NOT a shape
        change.  Reporting it without that label sends a reader hunting for something that is not
        there; suppressing it would be a miss.
        """
        peer, rev0 = self.build()
        pinned_schema = blob_of(peer, rev0, SCHEMA_PATH)
        pinned_vectors = blob_of(peer, rev0, VECTORS_PATH)
        write(peer, SCHEMA_PATH, '{\n    "a": 1\n}\n')  # same document, different bytes
        commit(peer, "reformat, content-identical")
        prov = provenance(self.root, pinned_schema, pinned_vectors, rev0)
        rc, out = run_report(peer, prov)
        self.assertIn("DRIFTED", out)
        self.assertIn("the parsed documents are IDENTICAL", out)
        self.assertIn("semantically null", out)
        self.assertIn("still real drift", out)


# ---------------------------------------------------------------------------------------------------
# UNMEASURABLE — the classes real history supplies ZERO instances of
# ---------------------------------------------------------------------------------------------------


class TestUnmeasurable(Scrubbed):
    def build(self):
        peer = init_peer(self.root)
        write(peer, SCHEMA_PATH, '{"a": 1}\n')
        write(peer, VECTORS_PATH, '{"cases": []}\n')
        rev = commit(peer, "add the contract")
        return peer, rev, blob_of(peer, rev, SCHEMA_PATH), blob_of(peer, rev, VECTORS_PATH)

    def assert_not_rendered_as_agreement(self, out):
        self.assertIn("UNMEASURABLE", out)
        self.assertNotIn("SUMMARY: 2 same, 0 DRIFTED, 0 unmeasurable", out)
        self.assertIn("NOT", out)

    def test_renamed_upstream_is_UNMEASURABLE_not_SAME(self):
        """ZERO instances in the peer's entire history, so the backtest cannot cover this."""
        peer, rev0, sblob, vblob = self.build()
        os.makedirs(os.path.join(peer, "contract/schema/v2"), exist_ok=True)
        git(peer, "mv", SCHEMA_PATH, "contract/schema/v2/bus-protocol.schema.json")
        commit(peer, "move the schema")
        prov = provenance(self.root, sblob, vblob, rev0)
        rc, out = run_report(peer, prov)
        self.assertEqual(rc, 0)
        self.assert_not_rendered_as_agreement(out)
        self.assertIn("RENAME or a DELETE", out)
        self.assertIn("SUMMARY: 1 same, 0 DRIFTED, 1 unmeasurable", out)

    def test_deleted_upstream_is_UNMEASURABLE_not_SAME(self):
        peer, rev0, sblob, vblob = self.build()
        git(peer, "rm", "-q", VECTORS_PATH)
        commit(peer, "drop the vectors")
        prov = provenance(self.root, sblob, vblob, rev0)
        rc, out = run_report(peer, prov)
        self.assert_not_rendered_as_agreement(out)
        self.assertIn("SUMMARY: 1 same, 0 DRIFTED, 1 unmeasurable", out)

    def test_ref_that_does_not_resolve_is_UNMEASURABLE_and_says_how_to_fix_it(self):
        """The likely real case: a clone whose `origin/main` was never fetched."""
        peer, rev0, sblob, vblob = self.build()
        git(peer, "update-ref", "-d", "refs/remotes/origin/main")
        prov = provenance(self.root, sblob, vblob, rev0)
        rc, out = run_report(peer, prov)
        self.assertEqual(rc, 0)
        self.assertIn("UNMEASURABLE: origin/main does not resolve", out)
        self.assertIn("NOTHING WAS COMPARED", out)
        self.assertIn("This is not 'no drift'", out)
        # Assert on the VERDICT line, not on the word: "SAME" also appears in the stale-mirror
        # caveat ("A SAME below is 'same as our mirror'"), and a bare `assertNotIn("SAME")` fails on
        # the caveat while proving nothing about any verdict.
        self.assertFalse(
            [ln for ln in out.splitlines() if ln.strip().startswith("SAME —")],
            "no artifact may be given a verdict when the ref did not resolve",
        )
        self.assertNotIn("SUMMARY:", out, "there is no summary of a measurement that did not happen")

    def test_no_peer_checkout_refuses_by_naming_every_candidate(self):
        prov = provenance(self.root, "a" * 40, "b" * 40, "c" * 40)
        empty = os.path.join(self.root, "nowhere")
        os.makedirs(empty)
        saved_repo = mod.REPO
        mod.REPO = empty  # nothing above a tempdir holds an `empyrean` checkout
        try:
            rc, out = run_report(peer=None, prov_path=prov, empyrean="")
        finally:
            mod.REPO = saved_repo
        self.assertEqual(rc, 0)
        self.assertIn("UNMEASURABLE: no empyrean checkout found", out)
        self.assertIn("$EMPYREAN_DIR", out)
        self.assertIn("$EMPYREAN_SUITE_ROOT/empyrean", out)
        self.assertIn("derivation:", out)
        self.assertIn("This is NOT 'no drift'", out)


# ---------------------------------------------------------------------------------------------------
# The resolver — empyrean contract/SUITE_PATHS.md at 38f6df4
# ---------------------------------------------------------------------------------------------------


class TestResolver(Scrubbed):
    def test_set_but_wrong_is_a_hard_error_at_its_own_step(self):
        """SUITE_PATHS.md: a wrong value is evidence of a wrong environment; the next step hides it.

        Constructed so that step 2 WOULD have succeeded: a valid `empyrean` sits under the suite
        root.  If the resolver fell through, it would return that path and the test would not notice
        a wrong `$EMPYREAN_DIR` at all.
        """
        peer = init_peer(self.root)
        write(peer, SCHEMA_PATH, "{}\n")
        commit(peer, "c")
        bogus = os.path.join(self.root, "not-a-checkout")
        os.makedirs(bogus)
        os.environ["EMPYREAN_DIR"] = bogus
        os.environ["EMPYREAN_SUITE_ROOT"] = self.root
        path, step, tried = mod.locate_empyrean(None)
        self.assertIsNone(path, "a set-but-wrong variable must not fall through to the suite root")
        self.assertIsNone(step)
        self.assertTrue(any("EMPYREAN_DIR" in t and "no .git" in t for t in tried), tried)
        self.assertFalse(any("EMPYREAN_SUITE_ROOT" in t for t in tried),
                         "the next step must not even be tried: %r" % (tried,))

    def test_precedence_order_is_argument_then_env_then_suite_root(self):
        peer = init_peer(self.root)
        write(peer, SCHEMA_PATH, "{}\n")
        commit(peer, "c")
        path, step, _ = mod.locate_empyrean(peer)
        self.assertEqual((path, step), (peer, "0-argument"))
        os.environ["EMPYREAN_DIR"] = peer
        path, step, _ = mod.locate_empyrean(None)
        self.assertEqual((path, step), (peer, "1-env-checkout:EMPYREAN_DIR"))
        del os.environ["EMPYREAN_DIR"]
        os.environ["EMPYREAN_SUITE_ROOT"] = self.root
        path, step, _ = mod.locate_empyrean(None)
        self.assertEqual((path, step), (peer, "2-suite-root"))

    def test_the_marker_walk_finds_a_peer_from_a_linked_worktree_depth(self):
        """Step 3 from `<repo>/.claude/worktrees/<name>`, which is where agents actually run.

        `os.path.dirname(REPO)` — the obvious sibling guess — lands on `.claude/worktrees` here and
        finds nothing.  This is the configuration that guess is wrong in, so it is the one tested.
        """
        peer = init_peer(self.root)
        write(peer, SCHEMA_PATH, "{}\n")
        commit(peer, "c")
        deep = os.path.join(self.root, "oracle", ".claude", "worktrees", "agent-x")
        os.makedirs(deep)
        # The sibling guess this walk replaces, shown failing on the same fixture.
        self.assertFalse(
            mod.is_checkout(os.path.join(os.path.dirname(deep), "empyrean")),
            "dirname(REPO) lands on .claude/worktrees, which holds no peer",
        )
        saved = mod.REPO
        mod.REPO = deep
        try:
            path, step, _ = mod.locate_empyrean(None)
        finally:
            mod.REPO = saved
        self.assertEqual(step, "3-derived")
        self.assertEqual(os.path.realpath(path), os.path.realpath(peer))


# ---------------------------------------------------------------------------------------------------
# Freshness of the mirror
# ---------------------------------------------------------------------------------------------------


class TestMirrorFreshness(Scrubbed):
    def build(self):
        peer = init_peer(self.root)
        write(peer, SCHEMA_PATH, '{"a": 1}\n')
        write(peer, VECTORS_PATH, '{"cases": []}\n')
        rev = commit(peer, "add the contract")
        prov = provenance(
            self.root, blob_of(peer, rev, SCHEMA_PATH), blob_of(peer, rev, VECTORS_PATH), rev
        )
        return peer, prov

    def test_no_fetch_stamps_every_verdict_as_being_about_a_mirror(self):
        peer, prov = self.build()
        _, out = run_report(peer, prov, fetch=False)
        self.assertIn("reading the LOCAL MIRROR", out)
        self.assertIn("NOT 'same as what the peer publishes'", out)
        self.assertIn("the tip's COMMIT date is not the mirror's FETCH date", out)

    def test_a_failed_fetch_is_named_and_never_folded_into_the_result(self):
        """A peer with no remote configured: `git fetch origin` fails.

        The verdicts still print — a failed fetch does not make the local mirror unreadable — but
        they are stamped, and the failure is named rather than swallowed.
        """
        peer, prov = self.build()
        _, out = run_report(peer, prov, fetch=True)
        self.assertIn("FETCH FAILED", out)
        self.assertIn("arbitrarily far behind", out)
        self.assertIn("NOT 'same as what the peer publishes'", out)


# ---------------------------------------------------------------------------------------------------
# The sidecar
# ---------------------------------------------------------------------------------------------------


class TestPinSidecar(Scrubbed):
    def test_a_missing_pin_marker_refuses_rather_than_reporting_no_drift(self):
        peer = init_peer(self.root)
        write(peer, SCHEMA_PATH, "{}\n")
        write(peer, VECTORS_PATH, "{}\n")
        rev = commit(peer, "c")
        prov = provenance(
            self.root,
            blob_of(peer, rev, SCHEMA_PATH),
            blob_of(peer, rev, VECTORS_PATH),
            rev,
            drop="vectors.blob",
        )
        with self.assertRaises(SystemExit) as cm:
            run_report(peer, prov)
        self.assertIn("vectors.blob", str(cm.exception))
        self.assertIn("not a report that finds no drift", str(cm.exception))

    def test_the_real_sidecar_parses_and_both_pins_name_one_revision(self):
        """Not a fixture: the repo's own PROVENANCE.md, which the report reads by default.

        This is the one row that touches a real file, and it touches THIS repo's file, never a
        peer's.  `schema_conformance.rs` step 0 asserts the same-revision property; if this row and
        that gate ever disagree, one of them is parsing the sidecar wrongly.
        """
        pins = mod.read_pins()
        self.assertEqual(len(pins["blob"]), 40)
        self.assertEqual(len(pins["vectors.blob"]), 40)
        self.assertEqual(pins["revision"], pins["vectors.revision"])
        self.assertEqual(int(pins["bytes"]), os.path.getsize(
            os.path.join(mod.REPO, ARTIFACT_LOCAL[0])))
        self.assertEqual(int(pins["vectors.bytes"]), os.path.getsize(
            os.path.join(mod.REPO, ARTIFACT_LOCAL[1])))


ARTIFACT_LOCAL = [local for _, _, local in mod.ARTIFACTS]


# ---------------------------------------------------------------------------------------------------
# The backtest's own arithmetic
# ---------------------------------------------------------------------------------------------------


class TestBacktest(Scrubbed):
    def test_counts_match_a_fixture_whose_event_count_is_fixed_by_construction(self):
        """N is set here, not read off a run, so a miscount cannot be absorbed by updating a number."""
        n_changes = 5
        peer = init_peer(self.root)
        write(peer, SCHEMA_PATH, '{"a": 0}\n')
        write(peer, VECTORS_PATH, '{"cases": []}\n')
        commit(peer, "add")
        for i in range(1, n_changes + 1):
            write(peer, SCHEMA_PATH, '{"a": %d}\n' % i)
            commit(peer, "change %d" % i)
        rc, out = run_backtest(peer)
        self.assertEqual(rc, 0)
        self.assertIn("FIRED %d / %d content changes, MISSED 0" % (n_changes, n_changes), out)
        self.assertIn("catch rate: 100.0%", out)
        self.assertIn("vacuity control fired: 0", out)

    def test_a_merge_that_re_resolves_the_same_blob_is_not_a_false_positive(self):
        """ZERO instances in the peer's real history, so this fixture is that class's only coverage.

        A merge whose result for the path equals its FIRST parent's blob is not a content change and
        must not be counted as one.  Built explicitly: a side branch changes another file, and the
        merge leaves the schema exactly as the first parent had it.
        """
        peer = init_peer(self.root)
        write(peer, SCHEMA_PATH, '{"a": 0}\n')
        write(peer, VECTORS_PATH, '{"cases": []}\n')
        commit(peer, "add")
        base = git(peer, "rev-parse", "HEAD")
        git(peer, "checkout", "-q", "-b", "side")
        write(peer, "unrelated.txt", "side\n")
        commit(peer, "side work")
        git(peer, "checkout", "-q", "main")
        write(peer, SCHEMA_PATH, '{"a": 1}\n')
        commit(peer, "main changes the schema")
        first_parent_blob = blob_of(peer, "HEAD", SCHEMA_PATH)
        git(peer, "merge", "-q", "--no-ff", "-m", "merge side", "side")
        git(peer, "update-ref", "refs/remotes/origin/main", "HEAD")
        self.assertEqual(
            blob_of(peer, "HEAD", SCHEMA_PATH),
            first_parent_blob,
            "fixture is wrong if the merge moved the schema",
        )
        self.assertNotEqual(first_parent_blob, blob_of(peer, base, SCHEMA_PATH))
        _, out = run_backtest(peer)
        # The whole point of this fixture: upstream's history gives the literal false-positive class
        # a denominator of ZERO — structurally, since git records a path in a diff only when its blob
        # or mode moves — so the tool prints RATE UNMEASURED there and refuses to be quoted as
        # precision. Here the denominator is 1, built on purpose, and the rate IS measured.
        # Scope to the SCHEMA path's section. The same fixture's vectors path has no merge and so
        # legitimately prints RATE UNMEASURED; asserting over the whole output would conflate the
        # two paths, which is the very confusion the split-by-path report exists to prevent.
        schema_section = out.split("PATH: " + SCHEMA_PATH)[1].split("PATH: " + VECTORS_PATH)[0]
        self.assertIn("flagged 0 of 1 such commits", schema_section)
        self.assertNotIn(
            "RATE UNMEASURED",
            schema_section,
            "with a real touch-no-change commit present the denominator is not empty",
        )
        # ...and the vectors path in the SAME run, which has none, must say the opposite.
        vectors_section = out.split("PATH: " + VECTORS_PATH)[1]
        self.assertIn("flagged 0 of 0 such commits", vectors_section)
        self.assertIn("RATE UNMEASURED", vectors_section)
        self.assertIn("path TOUCHED, blob unchanged", out)
        # The merge is reached only by --full-history; the first-parent walk sees one real change.
        self.assertIn("FIRED 1 / 1 content changes, MISSED 0", out)

    def test_an_empty_population_reports_that_it_did_not_run(self):
        """The F-CITATION-LINT shape: zero events is UNMEASURED, and must never print as 100%."""
        peer = init_peer(self.root)
        write(peer, "unrelated.txt", "x\n")
        commit(peer, "no contract here at all")
        _, out = run_backtest(peer)
        self.assertIn("the backtest cannot run on it", out)
        self.assertIn("NO EVENTS MEASURED", out)
        self.assertNotIn("catch rate:            100.0%", out)

    def test_the_backtest_refuses_loudly_when_the_peer_cannot_be_found(self):
        saved = mod.REPO
        empty = os.path.join(self.root, "nowhere")
        os.makedirs(empty)
        mod.REPO = empty
        try:
            _, out = run_backtest("")
        finally:
            mod.REPO = saved
        self.assertIn("The backtest DID NOT RUN", out)
        self.assertIn("not a catch rate of 100%", out)


# ---------------------------------------------------------------------------------------------------
# The detector itself, in isolation
# ---------------------------------------------------------------------------------------------------


class TestClassify(unittest.TestCase):
    def test_three_answers_and_None_is_never_agreement(self):
        self.assertEqual(mod.classify("a" * 40, "a" * 40), mod.SAME)
        self.assertEqual(mod.classify("a" * 40, "b" * 40), mod.DRIFTED)
        self.assertEqual(mod.classify("a" * 40, None), mod.UNMEASURABLE)
        self.assertNotEqual(mod.classify("a" * 40, None), mod.SAME)

    def test_the_delta_summary_survives_a_non_json_blob(self):
        lines = mod.describe_delta(b"\xff\xfe not json", b"also not json")
        self.assertTrue(any("byte comparison only" in ln for ln in lines))
        self.assertTrue(any("bytes 13 -> 13" in ln or "bytes" in ln for ln in lines))


if __name__ == "__main__":
    unittest.main(verbosity=2)
