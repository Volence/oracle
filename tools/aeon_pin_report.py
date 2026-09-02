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


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--sigil", default=None, help="path to the sigil checkout")
    ap.add_argument("--ref", default="origin/master",
                    help="the TIP ref to ask the currency question at (default origin/master)")
    ap.add_argument("--fetch", action="store_true",
                    help="update the local mirror of that ref first (network; off by default, "
                         "because a reporter should not mutate anything you did not ask it to)")
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
        return 0
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
        return 0
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
        return 0
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
    print("\nExiting 0 — report only.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
