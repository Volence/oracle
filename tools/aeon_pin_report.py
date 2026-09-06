#!/usr/bin/env python3
"""Has aeon's build moved past our frozen pin?  A REPORTER, never a gate.

WHY THIS IS NOT A TEST
----------------------
The obvious shape for this — a default-suite test that reads sigil's golden at ``origin/master`` and
fails when it differs from our frozen copy — is wrong twice over:

1. It reintroduces the sibling-checkout dependency that ``fixtures/aeon/`` exists to remove.  Our
   suite would stop passing in a fresh clone, on CI, and on any machine without sigil beside us.
2. It makes our build go red because *someone else* moved.  When a gate goes red, "the consumer is
   broken" is a conclusion that requires work from nobody except the consumer — and the whole
   gradient then pushes toward bending our side until it goes green.  That is precisely how a pin
   gets moved to make a red test pass, which ``fixtures/aeon/PROVENANCE.md`` forbids in terms.

So this is something you *read*.  It always exits 0.  Nothing calls it from a gate.

WHICH QUESTION IT ASKS
----------------------
The **currency** question — *"has it moved?"* — which must be asked at **TIP**, never at the pinning
revision.  Re-pointing a drift check at the revision the pin was taken from makes it vacuous: a
pinned blob equals itself forever, so it would pass for the wrong reason and never once detect the
thing it exists for.

The complementary **recovery** question — *"are the bytes here the bytes we recorded?"* — is asked at
the pinning revision, is a fact about this repository alone, and therefore IS a gate:
``crates/oracle-replay/tests/aeon_pin.rs``.

WHAT IT CANNOT ANSWER, AND SAYS SO
----------------------------------
sigil's golden directory carries the ROMs and **zero ``.lst`` listings at any revision checked**.  So
for every listing row there is no upstream artifact to compare against and the currency of those rows
is *not measurable from sigil at all*.  Those print as UNMEASURABLE.  They are never rendered as
agreement, and never counted toward a clean result.

THE LISTING ROWS' CURRENCY, ASKED A DIFFERENT WAY (added for F-FROZEN-FIXTURE-DRIFTS)
------------------------------------------------------------------------------------
Four of the six pinned artifacts are ``.lst`` files, and the paragraph above says their currency is
unmeasurable *as bytes*.  That was read for a while as "unmeasurable", full stop, and it is not: what
cannot be compared is the **bytes**.  The **shapes** can be, and the shapes are what consumers read.

So the DIMENSION CURRENCY section below asks, per row of ``fixtures/aeon/DIMENSIONS.tsv``: *does aeon
publish a dimension our frozen listing has none of?*  It reports from two sources, kept separate
because they are not equally trustworthy:

1. **aeon's OBJECT STORE at a ref** (``git grep`` at ``--aeon-ref``, default ``origin/master``) — the
   primary.  A committed revision, so no working tree is opened and a mid-edit save in that lane
   cannot move this answer.  It is a **PROXY**: it sees a token in tracked *source*, not an entry in a
   *built listing*.  Both error directions are real and are printed with the result — a name mentioned
   only in a comment counts as published, and a name the assembler generates counts as absent.
2. **aeon's LIVE BUILD TREE**, only if the located checkout happens to have the ``.lst`` on disk — the
   authoritative count, and the only place an exact number exists, because *no repo in the suite
   tracks these listings*.  It is a working-tree read and is labelled as one, stamped with the file's
   size and mtime, and never printed without that stamp.  It is secondary for exactly that reason.

If neither source is available the rows print UNMEASURABLE.  **They are never rendered as "no drift".**

HOW IT REACHES SIGIL, AND WHY IT STAYS AT TIP
---------------------------------------------
Every byte it compares comes out of sigil's **object store** — ``git rev-parse``/``git cat-file`` at
``--ref`` — so sigil's *working tree* is never opened and a mid-edit save in that lane cannot move
this report.  That mechanism was already right; what was wrong until 2026-09-02 was how the checkout
was *located*: a home literal ``/home/volence/sonic_hacks/sigil`` closed the candidate list, and the
sibling guess before it (``dirname(REPO)/sigil``) resolved to nothing from a linked worktree.  Both
are replaced by the precedence in empyrean ``contract/SUITE_PATHS.md`` at ``38f6df4`` — ``--sigil``,
``SIGIL_DIR`` (``ORACLE_SIGIL_DIR`` as a transition alias), ``EMPYREAN_SUITE_ROOT/sigil``, a marker
walk, then a refusal naming all of them — and the resolved path prints with the step that answered.

It is **not** pinned, and must not be.  Re-pointing a currency check at the revision the pin was taken
from makes it vacuous: a pinned blob equals itself by construction, so it would pass forever while
detecting nothing.  Fix the mechanism, keep the question.

Usage:  python3 tools/aeon_pin_report.py [--sigil DIR] [--ref REF] [--fetch]
                                        [--aeon DIR] [--aeon-ref REF]
"""

import argparse
import datetime
import hashlib
import os
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PIN = os.path.join(REPO, "fixtures", "aeon", "PIN.tsv")
GOLDEN = "crates/sigil-harness/golden"
HEADER = ["file", "sha256", "bytes", "chain", "sigil_freeze", "aeon_rev", "authority", "upstream"]


def read_pin(path):
    """Parse PIN.tsv into a list of dicts.  Same file the gate reads; same 8-column contract."""
    rows = []
    header_seen = False
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.rstrip("\n")
            if not line.strip() or line.startswith("#"):
                continue
            f = line.split("\t")
            if len(f) != len(HEADER):
                sys.exit("PIN.tsv: expected %d tab-separated columns, got %d in %r"
                         % (len(HEADER), len(f), line))
            if not header_seen:
                if f != HEADER:
                    sys.exit("PIN.tsv header changed: %r" % (f,))
                header_seen = True
                continue
            rows.append(dict(zip(HEADER, f)))
    if not rows:
        sys.exit("PIN.tsv lists no artifacts")
    return rows


def git(sigil, *args, binary=False):
    """Run one git command in the sigil checkout.  Returns None on failure — never raises.

    Failures must surface as UNMEASURABLE, not as a crash and not as a silent empty result.  (A
    pipeline that hashes a failed command's empty output returns e3b0c442…, the sha256 of nothing,
    which reads as a perfectly plausible artifact hash.  Every read here is checked instead.)
    """
    p = subprocess.run(["git", "-C", sigil] + list(args),
                       capture_output=True, check=False)
    if p.returncode != 0:
        return None
    return p.stdout if binary else p.stdout.decode("utf-8", "replace").strip()


def is_checkout(path):
    """A git checkout has a `.git` — a directory in a normal clone, a *file* in a linked worktree."""
    return bool(path) and os.path.exists(os.path.join(path, ".git"))


def suite_root_from(anchor):
    """Walk up from `anchor` to the first directory holding a `sigil` checkout.

    Deliberately **not** ``git rev-parse --git-common-dir``.  That command returns three different
    shapes — ``.git`` at a main checkout's root, an absolute path from a linked worktree's
    subdirectory, and a *relative* ``../../.git`` from a MAIN-checkout subdirectory — and trimming its
    answer lexically is how sigil walked onto the wrong directory.  That failure is invisible to
    agents, who run in linked worktrees, while the suite runs from the main checkout: the two return
    different shapes.  A marker walk asks the filesystem the question it actually has and needs none
    of it.

    It also replaces ``os.path.dirname(REPO)``, which had the same bug from the other side: ``REPO``
    is this file's own two-levels-up, so from a linked worktree at ``<repo>/.claude/worktrees/<name>``
    its parent is ``<repo>/.claude/worktrees`` and the sibling guess landed nowhere.  Every ancestor
    is tried here instead, so the worktree case walks past ``.claude`` and finds the suite root.
    """
    cur = os.path.abspath(anchor)
    while True:
        if is_checkout(os.path.join(cur, "sigil")):
            return cur
        parent = os.path.dirname(cur)
        if parent == cur:
            return None
        cur = parent


def locate_sigil(explicit):
    """Resolve a sigil checkout.  Returns ``(path_or_None, step, tried)``.

    Precedence is empyrean ``contract/SUITE_PATHS.md`` at ``38f6df4``: the explicit argument, the
    checkout variable, the suite root joined with the repo's directory name, derivation, then refuse
    "naming what was looked for and where.  Never a home literal, and never a silent fallback to the
    live tree."  The home literal ``/home/volence/sonic_hacks/sigil`` that used to close this list is
    gone; ``ORACLE_SIGIL_DIR`` is kept as a transition alias, and ``SIGIL_DIR`` is the ratified name.

    A variable that is **set but wrong** is a hard error at its own step rather than a fall-through:
    a wrong value is evidence of a wrong environment, and the next step would hide it.

    Note what this resolver is for.  Locating the checkout is the only thing it does; every byte this
    script then reads comes out of that checkout's *object store* at a ref (``git cat-file``), never
    its working tree.  That is why a derivation step is legitimate here and is refused elsewhere in
    this repo: what a derived path can hand you is a tree whose revision moves under a run, and this
    script never reads a tree.
    """
    tried = []
    if explicit:
        if is_checkout(explicit):
            return explicit, "0-argument", tried
        tried.append("--sigil %s -> no .git there" % explicit)
        return None, None, tried

    for var in ("SIGIL_DIR", "ORACLE_SIGIL_DIR"):
        val = os.environ.get(var)
        if val is None:
            tried.append("$%s (a path to the sigil checkout) — not set" % var)
            continue
        if is_checkout(val):
            return val, "1-env-checkout:%s" % var, tried
        tried.append("$%s=%s -> no .git there (set but wrong is a hard error, not a "
                     "reason to keep looking)" % (var, val))
        return None, None, tried

    root = os.environ.get("EMPYREAN_SUITE_ROOT")
    if root is None:
        tried.append("$EMPYREAN_SUITE_ROOT/sigil — EMPYREAN_SUITE_ROOT not set")
    else:
        cand = os.path.join(root, "sigil")
        if is_checkout(cand):
            return cand, "2-suite-root", tried
        tried.append("$EMPYREAN_SUITE_ROOT=%s -> %s has no .git" % (root, cand))
        return None, None, tried

    derived = suite_root_from(REPO)
    if derived is not None:
        return os.path.join(derived, "sigil"), "3-derived", tried
    tried.append("derivation: no ancestor of %s contains sigil/.git" % REPO)
    return None, None, tried


def chain_number_at(sigil, rev):
    """Derive the freeze-chain number: it is the count of ``[[entry]]`` blocks in provenance.toml.

    Derived, not transcribed.  Verified at three revisions when this was written:
    5af70797 -> 186, 39c34fd2 -> 189, origin/master 3ad7ed02 -> 189.
    """
    blob = git(sigil, "cat-file", "-p", "%s:%s/provenance.toml" % (rev, GOLDEN), binary=True)
    if blob is None:
        return None, None, None
    text = blob.decode("utf-8", "replace")
    n = sum(1 for line in text.splitlines() if line.rstrip() == "[[entry]]")
    name, aeon_rev = None, None
    for line in text.splitlines():
        s = line.strip()
        if s.startswith("name = "):
            name = s.split("=", 1)[1].strip().strip('"')
        elif s.startswith("aeon_rev = "):
            aeon_rev = s.split("=", 1)[1].strip().strip('"')
    return n, name, aeon_rev


DIMENSIONS = os.path.join(REPO, "fixtures", "aeon", "DIMENSIONS.tsv")
DIM_HEADER = ["file", "probe", "arg", "frozen", "upstream", "relied_on_by"]


def read_dimensions(path):
    """Parse DIMENSIONS.tsv.  Same file the hermetic gate reads; same 6-column contract.

    Returns None (with nothing printed) when the file is absent, so an older checkout that predates
    the manifest degrades to "this section is not available here" rather than to a crash.
    """
    if not os.path.exists(path):
        return None
    rows, header_seen = [], False
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.rstrip("\n")
            if not line.strip() or line.startswith("#"):
                continue
            f = line.split("\t")
            if len(f) != len(DIM_HEADER):
                sys.exit("DIMENSIONS.tsv: expected %d tab-separated columns, got %d in %r"
                         % (len(DIM_HEADER), len(f), line))
            if not header_seen:
                if f != DIM_HEADER:
                    sys.exit("DIMENSIONS.tsv header changed: %r" % (f,))
                header_seen = True
                continue
            rows.append(dict(zip(DIM_HEADER, f)))
    return rows


def locate_aeon(explicit):
    """Resolve an aeon checkout.  Returns ``(path_or_None, step, tried)``.

    Same precedence as :func:`locate_sigil`, per empyrean ``contract/SUITE_PATHS.md``.

    ⚑ The variable is **``AEON_DIR``**, never ``ORACLE_AEON_DIR``.  That contract is explicit that a
    variable naming *a directory of artifacts* (``ORACLE_AEON_DIR``, which oracle's tests point at a
    frozen copy) "keeps its own name; it is not an alias of ``AEON_DIR``".  Reading a checkout out of
    the artifact variable would resolve to ``fixtures/aeon`` by default — a directory with no ``.git``
    — and the whole section would report UNMEASURABLE for a reason that has nothing to do with aeon.
    """
    tried = []
    if explicit:
        if is_checkout(explicit):
            return explicit, "0-argument", tried
        tried.append("--aeon %s -> no .git there" % explicit)
        return None, None, tried

    val = os.environ.get("AEON_DIR")
    if val is None:
        tried.append("$AEON_DIR (a path to the aeon CHECKOUT, not ORACLE_AEON_DIR) — not set")
    elif is_checkout(val):
        return val, "1-env-checkout:AEON_DIR", tried
    else:
        tried.append("$AEON_DIR=%s -> no .git there (set but wrong is a hard error)" % val)
        return None, None, tried

    root = os.environ.get("EMPYREAN_SUITE_ROOT")
    if root is None:
        tried.append("$EMPYREAN_SUITE_ROOT/aeon — EMPYREAN_SUITE_ROOT not set")
    else:
        cand = os.path.join(root, "aeon")
        if is_checkout(cand):
            return cand, "2-suite-root", tried
        tried.append("$EMPYREAN_SUITE_ROOT=%s -> %s has no .git" % (root, cand))
        return None, None, tried

    derived = suite_root_from(REPO)
    if derived is not None and is_checkout(os.path.join(derived, "aeon")):
        return os.path.join(derived, "aeon"), "3-derived", tried
    tried.append("derivation: no ancestor of %s contains both sigil/.git and aeon/.git" % REPO)
    return None, None, tried


def upstream_publishes(aeon, ref, path, token):
    """How many tracked files under `path` mention `token`, at `ref`, read from the OBJECT STORE.

    Returns ``(count, None)`` or ``(None, reason)``.  A `git grep` that matches nothing exits 1, which
    is indistinguishable from a failure by return code alone — so a nonzero exit is disambiguated by
    re-asking whether the path exists at that ref at all.  Without that step "the path is gone" and
    "the token is absent" would print the same, and one of those is a measurement while the other is
    a broken question.
    """
    p = subprocess.run(["git", "-C", aeon, "grep", "-l", "-F", token, ref, "--", path],
                       capture_output=True, check=False)
    if p.returncode == 0:
        return len([x for x in p.stdout.decode("utf-8", "replace").splitlines() if x.strip()]), None
    # Exit 1 with no output is a real "no match" — but only if the path is actually there.
    probe = subprocess.run(["git", "-C", aeon, "ls-tree", "-r", "--name-only", ref, "--", path],
                           capture_output=True, check=False)
    if probe.returncode != 0 or not probe.stdout.strip():
        return None, "%s holds no tracked files at %s" % (path, ref)
    if p.returncode == 1:
        return 0, None
    return None, "git grep failed (exit %d)" % p.returncode


def live_listing_dimension(aeon, lst, probe, arg):
    """The authoritative count, read from aeon's LIVE BUILD TREE — or ``(None, reason)``.

    ⚑ This is a working-tree read, and it is the only place an exact number exists: no repo in the
    suite tracks these `.lst` files.  Every caller prints the stamp this returns alongside the number,
    so the answer can never be mistaken for an object-store fact.

    Deliberately a text scan and not our parser: this script is not the gate, has no Rust available to
    it, and a rough count that is labelled rough is worth more here than no number at all.  The
    authoritative parser-side measurement of the FROZEN side is `crates/oracle-core/tests/aeon_dimensions.rs`.
    """
    path = os.path.join(aeon, lst)
    try:
        st = os.stat(path)
    except OSError:
        return None, "%s not built in that checkout" % lst
    stamp = "%d bytes, mtime %s" % (
        st.st_size,
        datetime.datetime.fromtimestamp(st.st_mtime).strftime("%Y-%m-%dT%H:%M:%S"))
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            text = fh.read()
    except OSError as e:
        return None, "%s unreadable: %s" % (lst, e)

    if probe == "equate_prefix_count":
        n = sum(1 for l in text.splitlines() if l.startswith("EQU " + arg))
    elif probe == "symbol_prefix_count":
        n = len({l.split(":", 1)[0].strip() for l in text.splitlines()
                 if l.startswith(" " + arg) and ":" in l})
    elif probe == "symbol_present":
        n = int(any(l.startswith(" " + arg + " :") for l in text.splitlines()))
    elif probe == "equate_rows":
        n = None
        for l in text.splitlines():
            s = l.strip()
            if s.endswith(" equates") and s.split()[0].isdigit():
                n = int(s.split()[0])
        if n is None:
            return None, "%s has no `N equates` trailer" % lst
    elif probe == "phase_count":
        n = None
        for l in text.splitlines():
            if l.startswith("PHASE-COUNT "):
                n = int(l.split()[1])
            elif l.startswith("PHASE COUNT "):
                n = int(l.split()[2])
        if n is None:
            return ("absent", stamp) if "Phase Table" not in text else (0, stamp)
    else:
        return None, "no live probe for `%s`" % probe
    return n, stamp


def report_dimension_currency(aeon_arg, aeon_ref):
    """The section F-FROZEN-FIXTURE-DRIFTS asked for: has aeon published a shape our pin lacks?"""
    print("\n" + "=" * 78)
    print("DIMENSION CURRENCY — the .lst rows, asked as SHAPES rather than bytes")
    print("=" * 78)

    dims = read_dimensions(DIMENSIONS)
    if dims is None:
        print("UNMEASURABLE: fixtures/aeon/DIMENSIONS.tsv is not present in this checkout.")
        print("Nothing was compared. This is NOT 'no drift'.")
        return
    askable = [d for d in dims if d["upstream"] != "-" and d["arg"] != "-"]
    print("%d of %d manifest rows name a tracked upstream origin and are askable here."
          % (len(askable), len(dims)))
    print("The other %d (`equate_rows`, `phase_count`) have no single tracked source file to grep,"
          % (len(dims) - len(askable)))
    print("so the object store cannot answer them; the live-tree column below still can.")

    aeon, step, tried = locate_aeon(aeon_arg)
    if aeon is None:
        print("\nUNMEASURABLE: no aeon checkout found. Consulted, in order:")
        for t in tried:
            print("    %s" % t)
        print("This is NOT 'the shapes are current'. Nothing was compared.")
        print("Pass --aeon DIR, or set AEON_DIR, to measure.")
        return
    print("\naeon checkout: %s   [step=%s]" % (aeon, step))

    tip = git(aeon, "rev-parse", aeon_ref)
    if tip is None:
        print("UNMEASURABLE: %s does not resolve in that checkout. Nothing was compared."
              % aeon_ref)
        print("(aeon's default branch is `master`; there is no `main`.)")
        return
    when = git(aeon, "log", "-1", "--format=%cI", tip)
    print("OBJECT STORE at %s = %s   committed %s" % (aeon_ref, tip[:12], when or "?"))
    print("  ^ PROXY: a token in tracked SOURCE, not an entry in a built listing. A name that appears")
    print("    only in a comment counts as published; a name the assembler generates counts as absent.")

    live_note = None
    if os.path.isdir(aeon):
        live_note = ("LIVE TREE at %s — a WORKING TREE read, not the object store. Authoritative "
                     "counts,\n  but the directory can be mid-edit and is not a revision. Each "
                     "number is stamped." % aeon)
        print("  " + live_note)

    drift, same, unmeasurable = [], 0, 0
    print("\nPER-ROW:")
    for d in dims:
        ident = "%s:%s(%s)" % (d["file"], d["probe"], d["arg"]) if d["arg"] != "-" \
            else "%s:%s" % (d["file"], d["probe"])
        ours = d["frozen"]

        src, src_why = (None, "no tracked upstream origin recorded")
        if d["upstream"] != "-" and d["arg"] != "-":
            src, src_why = upstream_publishes(aeon, tip, d["upstream"], d["arg"])
        live, live_why = live_listing_dimension(aeon, d["file"], d["probe"], d["arg"])

        if src is None and live is None:
            unmeasurable += 1
            print("  %-46s UNMEASURABLE — store: %s; live: %s" % (ident, src_why, live_why))
            continue

        ours_absent = ours in ("0", "absent")
        live_absent = live in (0, "absent", None)
        srcs = "%d file(s)" % src if src is not None else "n/a"
        lives = str(live) if live is not None else "n/a"

        # Two kinds of divergence, and the second is the one the row's own headline example is.
        #
        #   ABSENT HERE     — we have none of it and upstream has some. Coverage is synthetic.
        #   COUNT DIFFERS   — we have some and upstream has a different number. This is "five objects
        #                     where the real one had six", which hid a fault by exactly one entry. A
        #                     rule that only fired on zero would print that case as agreement, which
        #                     is the shape that made it invisible the first time.
        #
        # COUNT DIFFERS is asked of the LIVE number only: the store column counts files mentioning a
        # token, which is not the same quantity as the manifest's and must never be differenced
        # against it.
        if ours_absent and ((live is not None and not live_absent) or (src or 0) > 0):
            drift.append(ident)
            print("  %-46s ⚠ ABSENT HERE, PUBLISHED UPSTREAM" % ident)
            print("  %-46s   ours=%s  store=%s  live=%s" % ("", ours, srcs, lives))
            print("  %-46s   blind: %s" % ("", d["relied_on_by"]))
        elif live is not None and not ours_absent and str(live) != ours:
            drift.append(ident)
            print("  %-46s ⚠ COUNT DIFFERS (ours %s, upstream %s)" % (ident, ours, lives))
            print("  %-46s   store=%s" % ("", srcs))
            print("  %-46s   partly blind: %s" % ("", d["relied_on_by"]))
        else:
            same += 1
            print("  %-46s ours=%-7s store=%-9s live=%s" % (ident, ours, srcs, lives))

    print("\nSUMMARY: %d rows where upstream publishes a shape our pin lacks, %d without that gap, "
          "%d unmeasurable." % (len(drift), same, unmeasurable))
    if drift:
        print("A drift row is NOT a defect and NOT a reason to move the pin on its own. It is the")
        print("input to a decision, and its `blind:` column names the coverage that is synthetic")
        print("until the pin moves. Moving the pin is PROVENANCE.md, 'Moving the pin'.")
    if unmeasurable:
        print("The unmeasurable rows are NOT evidence of agreement. They were not compared at all.")


def _finish(args):
    """The one exit path, so the byte half's early returns cannot silently drop the shape half.

    Every ``return`` in the byte-currency section goes through here. They are all UNMEASURABLE cases —
    no sigil checkout, an unresolvable ref, an unreadable provenance — and none of them says anything
    about aeon's SHAPES, which are measured from a different checkout entirely. Returning early on
    them used to skip the dimension section with no line printed, which is the "a decorated tool's
    failure is indistinguishable from its empty result" shape: a reader would see a sigil complaint
    and no drift report, and read the absence as nothing to report.
    """
    report_dimension_currency(args.aeon, args.aeon_ref)
    print("\nExiting 0 — report only.")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--sigil", default=None, help="path to the sigil checkout")
    ap.add_argument("--ref", default="origin/master",
                    help="the TIP ref to ask the currency question at (default origin/master)")
    ap.add_argument("--fetch", action="store_true",
                    help="update the local mirror of that ref first (network; off by default, "
                         "because a reporter should not mutate anything you did not ask it to)")
    ap.add_argument("--aeon", default=None,
                    help="path to the aeon CHECKOUT (not ORACLE_AEON_DIR, which names an artifact "
                         "directory); $AEON_DIR is the env spelling")
    ap.add_argument("--aeon-ref", default="origin/master",
                    help="the ref to read aeon's tracked source at (default origin/master; aeon "
                         "has no `main`)")
    args = ap.parse_args()

    print("=" * 78)
    print("AEON PIN CURRENCY REPORT — REPORT ONLY.  This script never fails a build; it exits 0")
    print("whatever it finds.  It asks the CURRENCY question ('has it moved?') and therefore asks it")
    print("at TIP.  The recovery question ('are our bytes our bytes?') is the gate in")
    print("crates/oracle-replay/tests/aeon_pin.rs.")
    print("=" * 78)

    rows = read_pin(PIN)

    print("\nOUR PIN (fixtures/aeon/PIN.tsv):")
    for r in rows:
        print("  %-15s chain %-4s sigil %-9s aeon_rev %s  [%s]"
              % (r["file"], r["chain"], r["sigil_freeze"], r["aeon_rev"][:8], r["authority"]))
    chains = sorted({r["chain"] for r in rows}, key=int)
    if len(chains) > 1:
        print("  ^ MIXED PIN across chains %s. Per PROVENANCE.md this is a dated gap awaiting"
              % " and ".join(chains))
        print("    artifacts that do not exist upstream — not a permanent property of the design.")

    # ---- locate sigil ----
    sigil, step, tried = locate_sigil(args.sigil)
    if sigil is None:
        print("\nUNMEASURABLE: no sigil checkout found. Consulted, in order:")
        for t in tried:
            print("    %s" % t)
        print("This is NOT 'the pin is current'. Nothing was compared. Pass --sigil DIR to measure.")
        return _finish(args)
    print("\nsigil checkout: %s   [step=%s]" % (sigil, step))
    print("Read through its OBJECT STORE only — `git rev-parse` / `git cat-file` at a ref. The working")
    print("tree is never opened, so a mid-edit save in that lane cannot change what this reports.")

    if args.fetch:
        remote = args.ref.split("/")[0] if "/" in args.ref else "origin"
        if git(sigil, "fetch", "--quiet", remote) is None:
            print("  (fetch failed — reading the local mirror as it stands)")

    tip = git(sigil, "rev-parse", args.ref)
    if tip is None:
        print("\nUNMEASURABLE: %s does not resolve in that checkout. Nothing was compared." % args.ref)
        return _finish(args)
    when = git(sigil, "log", "-1", "--format=%cI", tip)
    age = ""
    if when:
        try:
            dt = datetime.datetime.fromisoformat(when)
            days = (datetime.datetime.now(dt.tzinfo) - dt).total_seconds() / 86400.0
            age = "  (%.1f days old)" % days
        except ValueError:
            pass
    print("TIP  %s = %s   committed %s%s" % (args.ref, tip, when or "?", age))
    if not args.fetch:
        print("     ^ this is your LOCAL mirror of %s. It can itself be behind the real remote;" % args.ref)
        print("       re-run with --fetch to update it before believing an 'agrees' below.")

    tip_chain, tip_name, tip_aeon = chain_number_at(sigil, tip)
    if tip_chain is None:
        print("\nUNMEASURABLE: could not read %s/provenance.toml at tip. Nothing was compared." % GOLDEN)
        return _finish(args)
    print("TIP chain %d — %s (aeon_rev %s)"
          % (tip_chain, tip_name or "?", (tip_aeon or "?")[:8]))

    # ---- per-row currency ----
    print("\nPER-FILE CURRENCY (each row asked independently, so this survives the pin becoming"
          "\nun-mixed without a rewrite):")
    agree = differ = unmeasurable = 0
    for r in rows:
        if r["upstream"] == "-":
            unmeasurable += 1
            print("  %-15s UNMEASURABLE — sigil freezes no counterpart for this artifact."
                  % r["file"])
            print("  %-15s              (its golden set carries zero .lst at any revision checked)"
                  % "")
            continue
        blob = git(sigil, "cat-file", "-p", "%s:%s" % (tip, r["upstream"]), binary=True)
        if blob is None:
            unmeasurable += 1
            print("  %-15s UNMEASURABLE — %s does not exist at tip." % (r["file"], r["upstream"]))
            continue
        got = hashlib.sha256(blob).hexdigest()
        if got == r["sha256"]:
            agree += 1
            print("  %-15s AGREES with tip (chain %s pin, tip chain %d) — %d bytes, %s"
                  % (r["file"], r["chain"], tip_chain, len(blob), got[:16] + "…"))
        else:
            differ += 1
            print("  %-15s DIFFERS from tip." % r["file"])
            print("  %-15s   ours (chain %s) %s  %s bytes"
                  % ("", r["chain"], r["sha256"][:16] + "…", r["bytes"]))
            print("  %-15s   tip  (chain %d) %s  %d bytes"
                  % ("", tip_chain, got[:16] + "…", len(blob)))
            if int(r["bytes"]) == len(blob):
                print("  %-15s   ⚠ SAME LENGTH, different bytes. Byte-count-neutral is not"
                      % "")
                print("  %-15s     byte-identical — never compare these by size."
                      % "")

    print("\nSUMMARY: %d agree, %d differ, %d unmeasurable, over %d pinned artifacts."
          % (agree, differ, unmeasurable, len(rows)))
    if differ:
        print("A DIFFER is not a defect on our side and not a reason to touch anything. It is the")
        print("input to a deliberate decision to move the pin — PROVENANCE.md, 'Moving the pin'.")
    if unmeasurable:
        print("The unmeasurable rows are NOT evidence of agreement. They were not compared at all.")
    return _finish(args)


if __name__ == "__main__":
    sys.exit(main())
