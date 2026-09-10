#!/usr/bin/env python3
"""Has the contract repo moved past the blobs `PROVENANCE.md` pins?  A REPORTER, never a gate.

WHY THIS EXISTS
---------------
`crates/oracle-aether/tests/schema_conformance.rs` proves that the two vendored artifacts
(`bus-protocol.schema.json`, `vectors.json`) hash to the blobs `PROVENANCE.md` pins.  That is
**self-consistency**: it is a fact about this repository alone, it is hermetic by design, and it can
never notice the contract repo moving on.  It answers *"are our bytes our bytes?"* — the **recovery**
question.

Nothing answered the **currency** question — *"is what we vendored still what the peer publishes?"* —
after `F-SCHEMA-READS-LIVE-EMPYREAN` (2026-09-02) removed the walk into the peer's working tree.  That
removal was right and is not being undone here.  What it left behind was recorded honestly at the time
(`contract/PROVENANCE.md`, "What was given up"): *"a default local run no longer notices upstream moving
on its own."*  This script is that notice, out of band.

WHY IT IS NOT A TEST
--------------------
Same two reasons as `tools/aeon_pin_report.py`, whose shape this follows deliberately:

1. A gate would reintroduce a peer dependency into a suite that is hermetic on purpose.  On a machine
   with no `empyrean/` checkout there is nothing to compare, and "cannot measure" must never be spelled
   as a failing build any more than it may be spelled as a pass.
2. A gate goes red because *someone else* moved.  The whole gradient then pushes toward bending our
   side until it is green — which for a pin means moving the pin to silence a red, exactly what
   `PROVENANCE.md`'s re-vendor recipe exists to prevent.

So this is something you *read*.  **It always exits 0.**  Nothing calls it from a gate.

WHICH QUESTION IT ASKS, AND AT WHICH REVISION
---------------------------------------------
The currency question, and therefore at the peer's **`origin/main` tip** — never at `pin.revision`.
Re-pointing a drift check at the revision the pin was taken from makes it vacuous: `pin.revision`
resolves those paths to the pinned blobs by construction and forever, so such a check would pass for
the wrong reason and detect nothing.  (Measured, not asserted: `--backtest` runs that same-revision
comparison as a control and it fires on **0 of 97** real drift events, against 97 of 97 for the form
this file ships.  That "97" was written "39" here before the backtest ran — a placeholder from the
design sketch that survived into prose and would have read as a measurement.  Numbers in this file
come from a run or they do not appear.)

HOW IT REACHES THE PEER
-----------------------
Every byte comes out of the contract repo's **object store** — `git rev-parse` / `git cat-file` at a
named revision — so its working tree is never opened and a mid-edit save in that lane cannot move this
answer.  That is `empyrean/contract/SUITE_PATHS.md` at `38f6df4`: *"A gate that proves a vendored copy
of a peer's CONTENT is fresh reads the peer through git objects at a named revision, never through the
peer's working tree."*  The checkout is *located* by that file's precedence (`--empyrean`,
`$EMPYREAN_DIR`, `$EMPYREAN_SUITE_ROOT/empyrean`, a marker walk, then a refusal naming every candidate
tried), and the step that answered is printed before anything is compared.

THE STALE-MIRROR CASE, AND THE CHOICE MADE ABOUT IT
---------------------------------------------------
`origin/main` in a local checkout is a **mirror**.  It can be arbitrarily far behind the real remote,
and a report that compared against a week-old mirror and printed "no drift" would be wrong in the one
direction that matters.  The choice here:

* `--fetch` is **ON by default** (`--no-fetch` disables it), because a nightly can afford the network
  and a currency question asked of a stale mirror is not the question.
* A fetch that fails or times out is **named**, and every verdict in that run is then stamped
  `vs a mirror last fetched <when>`.  It is never folded into a clean result.
* The mirror's age is printed **whether or not** the fetch succeeded — the fetch timestamp
  (`.git/FETCH_HEAD` mtime) and the tip commit's own date — so a reader can see what the verdict is
  a verdict about.

TIMEOUTS
--------
The **fetch** has one (`--fetch-timeout`, default 120 s) because it touches the network and a hung
network call would hang the nightly.  The **comparison** deliberately has none: this machine runs many
agents and a subprocess that is merely slow under load must not be converted into a false "no drift".
A timeout that fires is printed by name as TIMED OUT and counted as unmeasurable, never as agreement.

Usage:
    python3 tools/contract_drift_report.py [--empyrean DIR] [--ref REF] [--no-fetch] [--json]
    python3 tools/contract_drift_report.py --backtest [--backtest-json FILE]
"""

import argparse
import datetime
import json
import os
import subprocess
import sys
import time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PROVENANCE = os.path.join(
    REPO, "crates", "oracle-aether", "tests", "contract", "PROVENANCE.md"
)

# The two vendored artifacts, each as (pin-marker prefix, path inside the contract repo, local copy).
# These three facts travel together; a fourth artifact would be one more row here and nothing else.
ARTIFACTS = [
    (
        "",  # pin.revision / pin.blob / pin.bytes
        "contract/schema/bus-protocol.schema.json",
        "crates/oracle-aether/tests/contract/bus-protocol.schema.json",
    ),
    (
        "vectors.",  # pin.vectors.revision / pin.vectors.blob / pin.vectors.bytes
        "contract/schema/tests/vectors.json",
        "crates/oracle-aether/tests/contract/vectors.json",
    ),
]

# Verdicts.  Three, not two: "could not measure" is a first-class answer and is never rendered as
# either of the other two.  This is the whole point of the file.
SAME = "SAME"
DRIFTED = "DRIFTED"
UNMEASURABLE = "UNMEASURABLE"


# ---------------------------------------------------------------------------------------------------
# The pin sidecar
# ---------------------------------------------------------------------------------------------------


def read_pins(path=None):
    """Parse the six `pin.*` markers out of PROVENANCE.md.

    Same markers `tests/schema_conformance.rs` parses, same "missing is loud" rule: a sidecar that lost
    its pin is a report that cannot run, not a report that says everything is fine.

    `path=None` resolves the module global at CALL time, deliberately.  Written first as
    `path=PROVENANCE`, a default argument evaluated once at import — so a test that pointed the module
    at a fixture sidecar was silently served the real one, and seven rows asserted against the live
    repo's pin while claiming to assert against a fixture.  Found by those rows going red, which is
    what they are for.
    """
    if path is None:
        path = PROVENANCE
    try:
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
    except OSError as e:
        sys.exit("cannot read %s: %s" % (path, e))
    pins = {}
    for line in text.splitlines():
        t = line.strip()
        if not t.startswith("pin."):
            continue
        if "=" not in t:
            continue
        key, val = t.split("=", 1)
        key, val = key.strip()[len("pin."):], val.strip()
        if val:
            pins[key] = val
    required = []
    for prefix, _, _ in ARTIFACTS:
        required += [prefix + k for k in ("revision", "blob", "bytes")]
    missing = [k for k in required if k not in pins]
    if missing:
        sys.exit(
            "PROVENANCE.md carries no `pin.%s = …` marker(s). The sidecar IS the pin, so a missing\n"
            "marker is a report that cannot run, not a report that finds no drift."
            % ", pin.".join(missing)
        )
    return pins


# ---------------------------------------------------------------------------------------------------
# Locating the peer — empyrean contract/SUITE_PATHS.md at 38f6df4, the precedence every resolver shares
# ---------------------------------------------------------------------------------------------------


def is_checkout(path):
    """A git checkout has a `.git` — a directory in a normal clone, a *file* in a linked worktree."""
    return bool(path) and os.path.exists(os.path.join(path, ".git"))


def suite_root_from(anchor):
    """Walk up from `anchor` to the first directory holding an `empyrean` checkout.

    Deliberately **not** `git rev-parse --git-common-dir`, for the reason `SUITE_PATHS.md` records
    against it and `tools/aeon_pin_report.py` repeats: it returns three different shapes (`.git` at a
    main checkout's root, an absolute path from a linked worktree's subdirectory, a *relative*
    `../../.git` from a main-checkout subdirectory), and trimming its answer lexically is how sigil
    walked onto the wrong directory.  A marker walk asks the filesystem the question it actually has.

    This matters here more than usual: agents run from `<repo>/.claude/worktrees/<name>`, so
    `dirname(REPO)` — the obvious sibling guess — lands on `<repo>/.claude/worktrees` and finds
    nothing.  Every ancestor is tried.
    """
    cur = os.path.abspath(anchor)
    while True:
        if is_checkout(os.path.join(cur, "empyrean")):
            return cur
        parent = os.path.dirname(cur)
        if parent == cur:
            return None
        cur = parent


def locate_empyrean(explicit):
    """Resolve the contract checkout.  Returns `(path_or_None, step, tried)`.

    A variable that is **set but wrong** is a hard error at its own step, not a fall-through: a wrong
    value is evidence of a wrong environment, and the next step would hide it (`SUITE_PATHS.md`).

    Locating the checkout is all this does.  Every byte read afterwards comes from that checkout's
    object store at a named revision, never from its working tree — which is why a derivation step is
    legitimate here and refused for reference-dependent measurement elsewhere.
    """
    tried = []
    if explicit:
        if is_checkout(explicit):
            return explicit, "0-argument", tried
        tried.append("--empyrean %s -> no .git there" % explicit)
        return None, None, tried

    val = os.environ.get("EMPYREAN_DIR")
    if val is None:
        tried.append("$EMPYREAN_DIR (a path to the empyrean checkout) — not set")
    elif is_checkout(val):
        return val, "1-env-checkout:EMPYREAN_DIR", tried
    else:
        tried.append(
            "$EMPYREAN_DIR=%s -> no .git there (set but wrong is a hard error, not a reason to "
            "keep looking)" % val
        )
        return None, None, tried

    root = os.environ.get("EMPYREAN_SUITE_ROOT")
    if root is None:
        tried.append("$EMPYREAN_SUITE_ROOT/empyrean — EMPYREAN_SUITE_ROOT not set")
    else:
        cand = os.path.join(root, "empyrean")
        if is_checkout(cand):
            return cand, "2-suite-root", tried
        tried.append("$EMPYREAN_SUITE_ROOT=%s -> %s has no .git" % (root, cand))
        return None, None, tried

    derived = suite_root_from(REPO)
    if derived is not None:
        return os.path.join(derived, "empyrean"), "3-derived", tried
    tried.append("derivation: no ancestor of %s contains empyrean/.git" % REPO)
    return None, None, tried


# ---------------------------------------------------------------------------------------------------
# Reading the peer's object store
# ---------------------------------------------------------------------------------------------------


class Git:
    """`git -C <repo>`, with failures surfaced rather than swallowed.

    Returns `None` on failure and records why.  A pipeline that treats a failed command's empty output
    as data is how "could not measure" becomes a plausible-looking answer; every read here is checked.

    **No timeout on reads.** See the module docstring: this machine runs many agents concurrently, and
    a read that is merely slow under load must not become a false "no drift".
    """

    def __init__(self, repo):
        self.repo = repo
        self.last_error = None

    def __call__(self, *args, binary=False, timeout=None):
        self.last_error = None
        try:
            p = subprocess.run(
                ["git", "-C", self.repo] + list(args),
                capture_output=True,
                check=False,
                timeout=timeout,
            )
        except subprocess.TimeoutExpired:
            self.last_error = "TIMED OUT after %ss: git %s" % (timeout, " ".join(args))
            return None
        except OSError as e:
            self.last_error = "could not run git: %s" % e
            return None
        if p.returncode != 0:
            self.last_error = (
                p.stderr.decode("utf-8", "replace").strip()
                or "git %s exited %d" % (" ".join(args), p.returncode)
            )
            return None
        return p.stdout if binary else p.stdout.decode("utf-8", "replace").strip()

    def ok(self, *args):
        """Run for the exit status alone (`merge-base --is-ancestor`)."""
        try:
            return (
                subprocess.run(
                    ["git", "-C", self.repo] + list(args),
                    capture_output=True,
                    check=False,
                ).returncode
                == 0
            )
        except OSError:
            return False


def blob_at(git, rev, path):
    """The blob id the peer resolves `<rev>:<path>` to, or `None` with `git.last_error` set.

    `rev-parse <rev>:<path>` reads the object store.  It never opens the working tree, which is the
    property `F-SCHEMA-READS-LIVE-EMPYREAN` cost us and `SUITE_PATHS.md` ratified.
    """
    out = git("rev-parse", "%s:%s" % (rev, path))
    if out is None or len(out) != 40:
        return None
    return out


# ---------------------------------------------------------------------------------------------------
# THE DETECTOR.  One function, three answers, and the backtest measures THIS — not a re-implementation
# of it, which is how a detector gets measured green and ships blind.
# ---------------------------------------------------------------------------------------------------


def classify(pinned_blob, current_blob):
    """`SAME`, `DRIFTED`, or `UNMEASURABLE`.

    `current_blob is None` means the peer could not resolve that path at that revision — deleted,
    renamed, or an object we cannot read.  That is `UNMEASURABLE`, and the one rule this file has is
    that `UNMEASURABLE` is never rendered as `SAME`, as zero, or as green.
    """
    if current_blob is None:
        return UNMEASURABLE
    return SAME if current_blob == pinned_blob else DRIFTED


# ---------------------------------------------------------------------------------------------------
# A readable delta, for when it HAS drifted
# ---------------------------------------------------------------------------------------------------


def leaves(node, prefix="$"):
    """Flatten a JSON document to `{leaf path: scalar}`.

    `PROVENANCE.md`'s own idiom for describing a schema delta ("flattening both copies to leaf paths
    and differencing the sets"), so the report speaks the sidecar's language.
    """
    out = {}
    if isinstance(node, dict):
        for k, v in node.items():
            out.update(leaves(v, "%s.%s" % (prefix, k)))
    elif isinstance(node, list):
        for i, v in enumerate(node):
            out.update(leaves(v, "%s[%d]" % (prefix, i)))
    else:
        out[prefix] = node
    return out


def describe_delta(old_bytes, new_bytes):
    """A human summary of what moved between two JSON blobs.  Returns a list of lines.

    Handles the case the history actually contains: bytes that changed while the parsed document did
    not (empyrean `47e77ec`, *"schema: restore raw UTF-8 (content-identical)"*).  That is real drift
    for a byte-pinned copy and is reported as such — and it is reported as *semantically null*, so a
    reader is not sent hunting for a shape change that is not there.
    """
    lines = ["bytes %d -> %d (%+d)" % (len(old_bytes), len(new_bytes), len(new_bytes) - len(old_bytes))]
    try:
        old = json.loads(old_bytes.decode("utf-8"))
        new = json.loads(new_bytes.decode("utf-8"))
    except (UnicodeDecodeError, ValueError) as e:
        lines.append("could not parse one side as JSON (%s) — byte comparison only" % e)
        return lines
    if old == new:
        lines.append(
            "the parsed documents are IDENTICAL: this is an encoding/whitespace change, "
            "semantically null. It is still real drift for a byte-pinned copy."
        )
        return lines
    lo, ln = leaves(old), leaves(new)
    added = sorted(set(ln) - set(lo))
    removed = sorted(set(lo) - set(ln))
    changed = sorted(k for k in set(lo) & set(ln) if lo[k] != ln[k])
    lines.append(
        "leaf paths: %d added, %d removed, %d changed" % (len(added), len(removed), len(changed))
    )
    for label, keys in (("+", added), ("-", removed), ("~", changed)):
        for k in keys[:12]:
            lines.append("  %s %s" % (label, k))
        if len(keys) > 12:
            lines.append("  %s …and %d more" % (label, len(keys) - 12))
    return lines


# ---------------------------------------------------------------------------------------------------
# The report
# ---------------------------------------------------------------------------------------------------


def mirror_age(repo, ref):
    """`(fetch_stamp_or_None, tip_commit_iso_or_None)` — what this verdict is a verdict ABOUT.

    `origin/main` is a local mirror.  Its tip's *commit* date is not its *fetch* date: a mirror
    fetched a week ago still reports whatever commit date it holds, which reads as recent.  Both are
    printed.
    """
    stamp = None
    for cand in ("FETCH_HEAD", os.path.join("refs", "remotes", *ref.split("/"))):
        p = os.path.join(repo, ".git", cand)
        if os.path.exists(p):
            stamp = datetime.datetime.fromtimestamp(
                os.path.getmtime(p), datetime.timezone.utc
            ).isoformat()
            break
    return stamp


def report(args):
    out = []

    def say(line=""):
        out.append(line)
        print(line)

    say("=" * 92)
    say("CONTRACT DRIFT REPORT — REPORT ONLY. This script never fails a build; it exits 0 whatever")
    say("it finds. It asks the CURRENCY question ('has the peer moved past our pin?') and therefore")
    say("asks it at the peer's TIP. The RECOVERY question ('are our bytes our bytes?') is the gate in")
    say("crates/oracle-aether/tests/schema_conformance.rs, which never skips and needs no peer.")
    say("=" * 92)

    pins = read_pins()
    result = {"tool": "contract_drift_report", "ref": args.ref, "artifacts": [], "measurable": None}

    say("\nOUR PIN (crates/oracle-aether/tests/contract/PROVENANCE.md):")
    for prefix, contract_path, _ in ARTIFACTS:
        say(
            "  %-46s blob %s  %s bytes  @ %s"
            % (
                contract_path,
                pins[prefix + "blob"][:12] + "…",
                pins[prefix + "bytes"],
                pins[prefix + "revision"][:12],
            )
        )
    revs = {pins[p + "revision"] for p, _, _ in ARTIFACTS}
    if len(revs) != 1:
        say(
            "  ⚠ The two pins name DIFFERENT revisions (%s). schema_conformance step 0 asserts they"
            % ", ".join(sorted(r[:12] for r in revs))
        )
        say("    are equal, so that gate is already red; this report is downstream of it.")

    repo, step, tried = locate_empyrean(args.empyrean)
    if repo is None:
        say("\nUNMEASURABLE: no empyrean checkout found. Consulted, in order:")
        for t in tried:
            say("    %s" % t)
        say("")
        say("This is NOT 'no drift'. NOTHING WAS COMPARED. Pass --empyrean DIR, or set $EMPYREAN_DIR")
        say("or $EMPYREAN_SUITE_ROOT, to measure.")
        result["measurable"] = False
        result["reason"] = "no empyrean checkout located"
        result["tried"] = tried
        return finish(args, result, out)

    say("\ncontract checkout: %s   [step=%s]" % (repo, step))
    say("Read through its OBJECT STORE only — `git rev-parse` at a named revision. The working tree is")
    say("never opened, so a mid-edit save in that lane cannot change what this reports")
    say("(F-SCHEMA-READS-LIVE-EMPYREAN; empyrean contract/SUITE_PATHS.md at 38f6df4).")
    result["repo"] = repo
    result["step"] = step

    git = Git(repo)

    # --- the mirror, and how fresh it is -----------------------------------------------------------
    fetched = None
    if args.fetch:
        remote = args.ref.split("/")[0] if "/" in args.ref else "origin"
        t0 = time.time()
        if git("fetch", "--quiet", remote, timeout=args.fetch_timeout) is None:
            say(
                "\n⚠ FETCH FAILED (%s). Every verdict below is against the LOCAL MIRROR as it stands,"
                % git.last_error
            )
            say("  which may be arbitrarily far behind the real remote. A SAME below is therefore")
            say("  'same as our mirror', NOT 'same as what the peer publishes'.")
            result["fetch"] = {"attempted": True, "ok": False, "error": git.last_error}
        else:
            fetched = True
            say("\nfetched %s in %.1fs (timeout %ss)" % (remote, time.time() - t0, args.fetch_timeout))
            result["fetch"] = {"attempted": True, "ok": True, "seconds": round(time.time() - t0, 1)}
    else:
        say("\n--no-fetch: reading the LOCAL MIRROR of %s without refreshing it." % args.ref)
        say("  A SAME below is 'same as our mirror', NOT 'same as what the peer publishes'.")
        result["fetch"] = {"attempted": False}

    tip = git("rev-parse", args.ref)
    if tip is None:
        say("\nUNMEASURABLE: %s does not resolve in %s." % (args.ref, repo))
        say("  git said: %s" % git.last_error)
        say("  NOTHING WAS COMPARED. This is not 'no drift'. An unfetched or missing remote-tracking")
        say("  ref is the likely cause; `git -C %s fetch origin` and re-run." % repo)
        result["measurable"] = False
        result["reason"] = "%s does not resolve: %s" % (args.ref, git.last_error)
        return finish(args, result, out)

    when = git("log", "-1", "--format=%cI", tip)
    stamp = mirror_age(repo, args.ref)
    say("\nTIP  %s = %s" % (args.ref, tip))
    say("     tip commit dated %s" % (when or "?"))
    say("     mirror last written %s%s" % (stamp or "?", "  (just fetched)" if fetched else ""))
    if not fetched:
        say("     ^ the tip's COMMIT date is not the mirror's FETCH date. A stale mirror reports a")
        say("       plausible-looking recent commit and is still stale.")
    result["tip"] = tip
    result["tip_committed"] = when
    result["mirror_written"] = stamp

    # --- the detector, once per artifact -----------------------------------------------------------
    say("\nPER-ARTIFACT CURRENCY (each asked independently — the two files move independently):")
    counts = {SAME: 0, DRIFTED: 0, UNMEASURABLE: 0}
    for prefix, contract_path, local_path in ARTIFACTS:
        pinned = pins[prefix + "blob"]
        current = blob_at(git, tip, contract_path)
        verdict = classify(pinned, current)
        counts[verdict] += 1
        row = {
            "path": contract_path,
            "local": local_path,
            "pinned_blob": pinned,
            "pinned_bytes": int(pins[prefix + "bytes"]),
            "pinned_revision": pins[prefix + "revision"],
            "current_blob": current,
            "verdict": verdict,
        }
        if verdict == UNMEASURABLE:
            say("\n  %s" % contract_path)
            say("    UNMEASURABLE — %s does not resolve this path." % tip[:12])
            say("    git said: %s" % git.last_error)
            say("    A path that vanished upstream is a RENAME or a DELETE, both of which are drift")
            say("    of a kind this comparison cannot summarise. It is NOT agreement and NOT zero.")
            row["error"] = git.last_error
        elif verdict == SAME:
            say("\n  %s" % contract_path)
            say("    SAME — %s at %s, which is pin.%sblob." % (pinned[:12] + "…", args.ref, prefix))
        else:
            say("\n  %s" % contract_path)
            say("    ⚑ DRIFTED")
            say("      pinned  %s  at revision %s" % (pinned, pins[prefix + "revision"]))
            say("      current %s  at %s = %s" % (current, args.ref, tip))
            # Capture each error as it happens. Reading `git.last_error` after BOTH calls reports
            # whatever the SECOND one left there — which, when the second succeeded, is `None`, and
            # the report printed the literal string "None" as its reason. A diagnostic that says
            # "could not read: None" is a diagnostic that lost the thing it exists to carry.
            old = git("cat-file", "blob", pinned, binary=True)
            old_err = git.last_error
            new = git("cat-file", "blob", current, binary=True)
            new_err = git.last_error
            if old is None or new is None:
                say(
                    "      (could not read one of the blobs to summarise the delta: pinned=%s "
                    "current=%s)" % (old_err or "ok", new_err or "ok")
                )
                say("      The blob ids above are still the answer; only the summary is missing.")
            else:
                for line in describe_delta(old, new):
                    say("      %s" % line)
                row["delta"] = describe_delta(old, new)
            moved_at = git(
                "log", "-1", "--format=%H %cI %s", tip, "--", contract_path
            )
            if moved_at:
                say("      last commit to write this path at %s: %s" % (args.ref, moved_at))
                row["last_write"] = moved_at
        result["artifacts"].append(row)

    # --- is the pin still merged? ------------------------------------------------------------------
    pin_rev = pins["revision"]
    if git("rev-parse", "--verify", "--quiet", pin_rev + "^{commit}") is None:
        say("\n⚠ pin.revision %s is NOT AN OBJECT in %s." % (pin_rev[:12], repo))
        say("  The pin names a commit this checkout has never seen. Unmeasurable, not agreement.")
        result["pin_merged"] = None
    elif git.ok("merge-base", "--is-ancestor", pin_rev, args.ref):
        say("\npin.revision %s is an ancestor of %s — the pin names something merged." % (pin_rev[:12], args.ref))
        result["pin_merged"] = True
    else:
        say("\n⚑ pin.revision %s is NOT an ancestor of %s." % (pin_rev[:12], args.ref))
        say("  The vendored copy tracks a revision that is not on the contract's default branch.")
        result["pin_merged"] = False

    # --- summary -----------------------------------------------------------------------------------
    say("\n" + "-" * 92)
    say(
        "SUMMARY: %d same, %d DRIFTED, %d unmeasurable, over %d pinned artifacts, at %s = %s"
        % (counts[SAME], counts[DRIFTED], counts[UNMEASURABLE], len(ARTIFACTS), args.ref, tip[:12])
    )
    if counts[DRIFTED]:
        say("")
        say("A DRIFTED row is not a defect on our side and not a reason to touch anything today. It is")
        say("the input to a deliberate re-vendor — PROVENANCE.md, 'To re-vendor', which moves all six")
        say("pin markers and both Current copy tables in the same commit as the serve that needs them.")
        say("Do NOT edit the vendored bytes to make anything agree: a locally-corrected copy is a copy")
        say("whose blob check is worthless.")
    if counts[UNMEASURABLE]:
        say("")
        say("The unmeasurable rows were NOT compared at all. They are not evidence of agreement.")
    result["measurable"] = True
    result["counts"] = {k.lower(): v for k, v in counts.items()}
    return finish(args, result, out)


def finish(args, result, out):
    if args.json:
        print("\n--- json ---")
        print(json.dumps(result, indent=2, sort_keys=True))
    return 0


# ---------------------------------------------------------------------------------------------------
# THE BACKTEST
#
# Why it exists: this lane refused a sibling check (`F-CITATION-LINT`) on 2026-09-10 after measuring
# its catch rate against the real population at ZERO.  It had been designed from the single finding
# that prompted it rather than from the population it had to cover, and it looked entirely reasonable
# until someone measured it.  This is the same species of check, so it gets the same test BEFORE it
# ships.
#
# The population is real and enumerable: every commit in the contract repo's history that touched
# either vendored path.  For each, the detector above is run with `pin = the blob before` against
# `ref = that commit` — the same `classify()` the report calls, not a re-implementation of it.
# ---------------------------------------------------------------------------------------------------


def backtest(args):
    print("=" * 92)
    print("BACKTEST — the detector above, replayed over the contract repo's REAL history of the two")
    print("vendored paths. Every event below actually happened. `classify()` is the shipped function.")
    print("=" * 92)

    repo, step, tried = locate_empyrean(args.empyrean)
    if repo is None:
        print("\nUNMEASURABLE: no empyrean checkout found. Consulted, in order:")
        for t in tried:
            print("    %s" % t)
        print("\nThe backtest DID NOT RUN. That is not a catch rate of 100%; it is no measurement.")
        return 0
    print("\ncontract checkout: %s   [step=%s]" % (repo, step))
    git = Git(repo)
    tip = git("rev-parse", args.ref)
    if tip is None:
        print("\nUNMEASURABLE: %s does not resolve (%s). The backtest DID NOT RUN." % (args.ref, git.last_error))
        return 0
    print("ref: %s = %s" % (args.ref, tip))

    everything = {"ref": args.ref, "tip": tip, "paths": []}
    grand = {}

    for _, contract_path, _ in ARTIFACTS:
        print("\n" + "=" * 92)
        print("PATH: %s" % contract_path)
        print("=" * 92)

        t0 = time.time()
        simplified = (git("log", tip, "--format=%H", "--", contract_path) or "").split()
        full = (git("log", tip, "--full-history", "--format=%H", "--", contract_path) or "").split()
        first_parent = (
            git("log", tip, "--first-parent", "--format=%H", "--", contract_path) or ""
        ).split()
        if not simplified:
            print("  no history for this path at %s — the backtest cannot run on it." % args.ref)
            continue
        first_iso = git("log", "-1", "--format=%cI", simplified[-1])
        last_iso = git("log", "-1", "--format=%cI", simplified[0])
        print(
            "\nPOPULATION: %d revisions on %s's FIRST-PARENT line touch this path — this is the"
            % (len(first_parent), args.ref)
        )
        print("            sequence a nightly watching that branch actually experiences, and it is")
        print("            the population the catch rate below is computed over.")
        print(
            "            (%d with default history simplification, %d with --full-history, which"
            % (len(simplified), len(full))
        )
        print("             reaches side-branch commits no tip-watcher ever sees; both are walked)")
        print("            window %s .. %s" % (first_iso, last_iso))
        print("            renames: none — `git log --full-history -M --diff-filter=R` over")
        print("            contract/schema/ returns EMPTY across the whole history, so the")
        print("            rename/delete class is UNEXERCISED by this population. It is covered")
        print("            instead by a constructed row, not by this backtest. Said, not implied.")

        # Classify every commit in the FULL history — merges included, because "a merge re-resolving
        # the same blob" is one of the false positives this backtest was asked to look for and it can
        # only appear in the full walk.  The headline numbers come from the first-parent walk.
        def walk(commits):
            out = []
            for c in commits:
                after = blob_at(git, c, contract_path)
                parents = (git("log", "-1", "--format=%P", c) or "").split()
                before = blob_at(git, parents[0], contract_path) if parents else None
                out.append({"commit": c, "before": before, "after": after, "parents": len(parents)})
            return out

        fp_rows = walk(first_parent)
        fp_events = [
            r
            for r in fp_rows
            if r["before"] is not None and r["after"] is not None and r["before"] != r["after"]
        ]
        fp_fired = [r for r in fp_events if classify(r["before"], r["after"]) == DRIFTED]
        print(
            "\nPRIMARY MEASUREMENT (first-parent line of %s): FIRED %d / %d content changes, "
            "MISSED %d" % (args.ref, len(fp_fired), len(fp_events), len(fp_events) - len(fp_fired))
        )
        print(
            "  catch rate: %s"
            % (
                "%.1f%%" % (100.0 * len(fp_fired) / len(fp_events))
                if fp_events
                else "n/a — ZERO EVENTS, which is UNMEASURED, not 100%"
            )
        )
        print("\nSECONDARY: the same replay over --full-history (side branches and merges included),")
        print("           broken out by class, because that is where a merge re-resolving the same")
        print("           blob — one of the false positives this was asked to look for — can appear:")

        rows = walk(full)

        added = [r for r in rows if r["before"] is None and r["after"] is not None]
        vanished = [r for r in rows if r["before"] is not None and r["after"] is None]
        touched_nochange = [
            r for r in rows if r["before"] is not None and r["after"] is not None and r["before"] == r["after"]
        ]
        events = [
            r for r in rows if r["before"] is not None and r["after"] is not None and r["before"] != r["after"]
        ]

        # --- the measurement -----------------------------------------------------------------------
        fired = [r for r in events if classify(r["before"], r["after"]) == DRIFTED]
        missed = [r for r in events if classify(r["before"], r["after"]) != DRIFTED]
        # False positives: a commit with NO content change that the detector nevertheless calls drift.
        fp = [r for r in touched_nochange if classify(r["before"], r["after"]) == DRIFTED]
        # Vanished-path commits: the detector must call these UNMEASURABLE and never SAME.
        vanished_verdicts = {}
        for r in vanished:
            v = classify(r["before"], r["after"])
            vanished_verdicts[v] = vanished_verdicts.get(v, 0) + 1

        print("\nEVENT CLASSES over the %d commits walked:" % len(rows))
        print("  %3d  content CHANGED (blob before != blob after)  <- the population to catch" % len(events))
        print("  %3d  path TOUCHED, blob unchanged (merge re-resolve / mode change / no-op)" % len(touched_nochange))
        print("  %3d  path ADDED (no prior blob, so no pin could have existed)" % len(added))
        print("  %3d  path VANISHED at that commit (rename or delete)" % len(vanished))

        print("\nDETECTOR RESULT, replayed:")
        print("  FIRED   %d / %d real content changes" % (len(fired), len(events)))
        print("  MISSED  %d / %d" % (len(missed), len(events)))
        rate = (100.0 * len(fired) / len(events)) if events else float("nan")
        print("  catch rate: %s" % ("%.1f%%" % rate if events else "n/a — zero events in the window"))
        print("  FALSE POSITIVES (flagged with no content change): %d / %d such commits"
              % (len(fp), len(touched_nochange)))
        if vanished:
            print("  vanished-path commits classified as: %s"
                  % ", ".join("%s=%d" % kv for kv in sorted(vanished_verdicts.items())))
            if vanished_verdicts.get(SAME):
                print("  ⚑ A vanished path was rendered SAME. That is the defect this file forbids.")

        # --- the vacuity control -------------------------------------------------------------------
        # Ask the same question at pin.revision instead of at tip.  If the detector still fires, the
        # measurement above is measuring something other than what it claims.
        control_fired = 0
        for r in events:
            if classify(r["after"], blob_at(git, r["commit"], contract_path)) == DRIFTED:
                control_fired += 1
        print("\n  CONTROL (the vacuous form — comparing each revision's blob against ITSELF, i.e. what")
        print("  a drift check pointed at pin.revision would do): fired %d / %d." % (control_fired, len(events)))
        print("  Zero is the expected and correct answer, and it is why this check asks at TIP.")

        # --- what the fires would have SAID --------------------------------------------------------
        null_semantic, real_semantic, unparsable = 0, 0, 0
        samples = []
        for r in fired:
            old = git("cat-file", "blob", r["before"], binary=True)
            new = git("cat-file", "blob", r["after"], binary=True)
            if old is None or new is None:
                unparsable += 1
                continue
            try:
                if json.loads(old.decode("utf-8")) == json.loads(new.decode("utf-8")):
                    null_semantic += 1
                    subj = git("log", "-1", "--format=%h %s", r["commit"])
                    samples.append(("semantically null", subj))
                else:
                    real_semantic += 1
            except (UnicodeDecodeError, ValueError):
                unparsable += 1
        print("\n  OF THE %d FIRES, by what actually changed:" % len(fired))
        print("    %3d changed the parsed document (a real shape/value change)" % real_semantic)
        print("    %3d changed BYTES ONLY — the parsed documents are identical (encoding/whitespace)."
              % null_semantic)
        print("        These are real drift for a byte-pinned copy, and the report labels them as")
        print("        semantically null so nobody hunts for a shape change that is not there.")
        if unparsable:
            print("    %3d could not be parsed on one side (reported as byte-comparison only)" % unparsable)
        for label, subj in samples[:5]:
            print("        e.g. %s: %s" % (label, subj))

        print("\n  wall clock for this path: %.1fs" % (time.time() - t0))

        everything["paths"].append(
            {
                "path": contract_path,
                "revisions_first_parent": len(first_parent),
                "first_parent_events": len(fp_events),
                "first_parent_fired": len(fp_fired),
                "revisions_simplified": len(simplified),
                "revisions_full_history": len(full),
                "window": [first_iso, last_iso],
                "content_change_events": len(events),
                "fired": len(fired),
                "missed": len(missed),
                "catch_rate_pct": round(rate, 1) if events else None,
                "touched_no_change": len(touched_nochange),
                "false_positives": len(fp),
                "added": len(added),
                "vanished": len(vanished),
                "vanished_verdicts": vanished_verdicts,
                "control_fired_at_own_revision": control_fired,
                "fires_semantic": real_semantic,
                "fires_bytes_only": null_semantic,
                "fires_unparsable": unparsable,
            }
        )
        for k, v in everything["paths"][-1].items():
            if isinstance(v, int):
                grand[k] = grand.get(k, 0) + v

    print("\n" + "=" * 92)
    print("BACKTEST TOTALS across both paths")
    print("=" * 92)
    fpe = grand.get("first_parent_events", 0)
    print("  PRIMARY — first-parent line of %s (what a nightly experiences):" % args.ref)
    print("    content-change events: %d" % fpe)
    print("    fired:                 %d" % grand.get("first_parent_fired", 0))
    print("    missed:                %d" % (fpe - grand.get("first_parent_fired", 0)))
    print(
        "    catch rate:            %s"
        % (
            "%.1f%%" % (100.0 * grand.get("first_parent_fired", 0) / fpe)
            if fpe
            else "n/a — NO EVENTS MEASURED"
        )
    )
    print("  SECONDARY — --full-history (side branches and merges included):")
    print("  content-change events: %d" % grand.get("content_change_events", 0))
    print("  fired:                 %d" % grand.get("fired", 0))
    print("  missed:                %d" % grand.get("missed", 0))
    ev = grand.get("content_change_events", 0)
    print("  catch rate:            %s"
          % ("%.1f%%" % (100.0 * grand.get("fired", 0) / ev) if ev else "n/a — NO EVENTS MEASURED"))
    print("  false positives:       %d (over %d touch-no-change commits)"
          % (grand.get("false_positives", 0), grand.get("touched_no_change", 0)))
    print("  vacuity control fired: %d (must be 0)" % grand.get("control_fired_at_own_revision", 0))
    if ev == 0:
        print("\n⚑ ZERO EVENTS. The catch rate is not 100%, it is UNMEASURED. Refuse the check.")
    everything["totals"] = grand
    if args.backtest_json:
        with open(args.backtest_json, "w", encoding="utf-8") as fh:
            json.dump(everything, fh, indent=2, sort_keys=True)
        print("\nwrote %s" % args.backtest_json)
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--empyrean", help="path to the contract checkout (highest precedence)")
    ap.add_argument("--ref", default="origin/main", help="peer revision to ask at (default origin/main)")
    ap.add_argument("--no-fetch", dest="fetch", action="store_false", help="do not refresh the mirror")
    ap.add_argument("--fetch-timeout", type=int, default=120, metavar="S")
    ap.add_argument("--json", action="store_true", help="also emit a machine-readable summary")
    ap.add_argument("--backtest", action="store_true", help="replay the detector over the peer's history")
    ap.add_argument("--backtest-json", metavar="FILE")
    ap.set_defaults(fetch=True)
    args = ap.parse_args()
    if args.backtest:
        return backtest(args)
    return report(args)


if __name__ == "__main__":
    sys.exit(main())
