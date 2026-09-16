# Oracle: Overseer Reference (opened at a moment, never at boot)

**What this is.** Split out of `docs/OVERSEER.md` on 2026-09-04 under the owner's ruling of
2026-09-04T15:38:47Z, one call for all six lanes (`git -C ../empyrean show
origin/main:docs/OVERSEER-PROTOCOL.md`, section "The boot read is bounded"): the boot read is
**split by WHEN a rule is read**, never by size and never by raising the bound. `OVERSEER.md` keeps
what a fresh session needs to act *at boot*: scope, queue, resume brief, the standing rulings that
change what it does first. A rule that only matters at one later moment lives here.

**When to open it: three moments, and they are the whole list.** Before **dispatching** a wave of
agents; before **reviewing** returned work; before **landing**. If you are doing none of those, you
do not need this file.

**What is in it.** Two sections, moved verbatim out of the boot file: **The bars** (house methods,
each earned by a measured failure) and **Ops** (each line a paid-for lesson).

**Since 2026-09-13 it also holds the follow-up register and the live rulings the when-read cut moved out
of `OVERSEER.md`**, appended at the end under **Moved from OVERSEER.md 2026-09-13**. Each is read at the
moment its stub in `OVERSEER.md` names, and some of those moments (reporting a lens count, cutting the boot
file, starting a server, a peer filing a demand) fall outside the three above.

**One thing deliberately did NOT move.** The bootstrap stanza (read the protocol at a committed
revision, never through `../empyrean/docs/OVERSEER-PROTOCOL.md`) stayed in `docs/OVERSEER.md`
under its own heading. It is upstream of the boot read itself, and a rule read at a later moment
cannot protect a read that already happened.

**On any disagreement with `origin/main:docs/OVERSEER-PROTOCOL.md`, the protocol governs.**

## The bars (house methods, each earned by a measured failure; do not thin)

**⚑ A THRESHOLD CALIBRATED FROM THE SYSTEM'S OWN BEHAVIOUR MEASURES THE DEFECT, NOT THE PROPERTY**
*(2026-09-09, found by an agent against a guard it was not sent to look at)*. A control required
**>= 5 %** open-loop underruns. That 5 % was arithmetic done on the **broken** samples-per-frame value
(0.6213 % deficit -> 5.2 %). The true rate yields **4.0 %**. **So the guard sat within 4 % of its own
vacuity, and one CORRECT change tipped it over** — it would have gone red for the right fix and been read
as the fix's fault.
**The failure mode is the nastiest available**: the number was derived, not copied, and derived correctly
from the tree as it then stood. Bar 1 is satisfied and the guard is still wrong, because *the tree as it
then stood contained the bug.* **A derivation is only as sound as the thing it derives FROM**, and a
constant lifted from live behaviour silently pins that behaviour as correct.
**The repair is the transferable part, and it is NOT re-tuning to the new edge** — that reinstates the
defect one value along. Make the load-bearing assertion **threshold-free**: here, *a deficit, however
small, drains any buffer eventually*, which is what a deficit IS. The percentage stays as a coarse
re-measured floor, explicitly not the assertion.
**Booked follow-up, and it is the real question the instance raises: how many other hand-tuned floors in
this workspace are calibrated to numbers that have since moved?** A floor that has never gone red since
the day it was written is the one to check first, because that is also what a vacuous one looks like.

**▶ PARKED, NOT IN FORCE (the moratorium is the owner's CUT THE CEREMONY ruling, in `docs/OVERSEER.md`,
which never uses the word; parked at the hub in `OVERSEER-PENDING-BARS.md`): A PARITY
PAIR IS STRUCTURALLY BLIND TO A DEFECT IN THE DERIVATION IT SHARES. ASSERT THE SHARED DERIVATION DID
SOMETHING.** Found by this seat probing parcel 2b, where the defence
already existed and is the reason the probe is a bar rather than a bug. R1 ("one derivation, two
consumers") makes a panel and a handler agree **by construction**, which is the point, and which means a
parity test can only witness *agreement*, never *correctness*. Break the shared function and both sides
move together: the pair agrees perfectly and both are wrong. Measured: `absolutise` reduced to
`path.to_string()` leaves the strip and `emulator/status.romPath` in exact agreement on the un-normalised
string. **The remedy is a third assertion in the pair: that the derivation is not a no-op**, and 2b's
test carries it (`assert_ne!` against the raw argument, failing with *"the agreement above is two copies
of the same untouched string rather than one shared normalisation"*), so the mutation went red. **Every
R1 pair owes this third clause**; without it a parity suite grows more confident exactly as it shares
more code. Same family as the poison bars: the row measures a real quantity and not the one it is named
for.

**▶ NEW BAR, 2026-08-26: A MERGED SERVE IS NOT A SERVED METHOD. THE CONSUMER REACHES A BINARY.**
Found in the foreground pass that closed the CR-D `⟨RUNTIME⟩` debt
(`docs/2026-08-26-runtime-decoders-check.md` §5). The object decoders merged, tested and pushed at
`0f33c44`, and stayed **unreachable to every consumer**, because `target/release/oracle-aether` was
still the build from the day before and **nothing in a merge rebuilds it**. The MCP shim spawns *that
binary*; so a shim spawned any time between the merge and the check answered
`[-32601] no such method` to the very methods we had just shipped. Reproduced firsthand on this
session's own shim, then fixed and re-verified end to end through the consumer's own spawn path.
**This sharpens, and does not contradict, the coordination note that *advertising a method is
shipping it*: the advertised list is authoritative, but it is emitted BY A RUNNING BINARY, and a
stale binary advertises a stale list with total confidence.** Practical check before telling any
consumer a method is available: spawn the consumer's own path and call it, not `cargo test`, which
passes against source the consumer never runs. Same family as item 1's rename fallout
(*compile-time-frozen paths, invisible until the binary runs*); here the frozen artifact was the
binary itself.

**▶ AND THE COUNTING BAR THAT CAME WITH IT: MEASURE USE, NOT ATTACHMENT.** The same pass had to
count whether consumers actually call these methods. `grep -c` over the transcript tree reports
~10,000 mentions across ~4,055 files, and reports **the same ~4,055 for every tool name, including
tools nobody has ever called**, because the MCP tool listing sits in every session's system prompt.
Parsing `tool_use` blocks instead gives the true figure: 216 invocations. **The near-constant across
varied inputs was the tell**. The existing bar caught it. Mentions measure attachment; only
invocations measure use.

**▶ NEW BAR, 2026-08-24: ANCHOR A CLAIM TO A SHA THAT CAN CARRY IT. A docs commit cannot vouch for
code.** Caught by aeon against this seat, same day. I reported the straddle fix to them anchored to
`7bdb75f`, which is a **one-line `docs/lane-log.jsonl` commit**. Every claim I made was true, and
the anchor could not carry any of it: the code is `4111c88` under merge `51143a5`, tests `68461a7`.
They cited the code SHAs in their booking instead. **The failure mode is that it hardens invisibly**:
a peer transcribes the anchor into their prose, and a later reader who checks it finds a docs diff
where a guarantee was promised. This is the same family as the provenance audit named in `The role`,
in `docs/OVERSEER.md` (*cite the
ruling, not a status field*): the citation must be the artifact that actually contains the thing.
Practical check before sending: `git show --stat <sha>` and confirm the files named are the ones the
claim is about.

**▶ AMENDMENT, 2026-08-27: THE ABOVE BAR HAS A FALSE-POSITIVE MODE, AND THIS SEAT FIRED IT AT A PEER.

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 952-966.)*
A RIGHT SHA ANSWERING AN UNSTATED QUESTION IS NOT A WRONG SHA.** Found by aiming the 08-24 bar at aeon
and being half right; the diagnosis below is theirs, banked by them at aeon `b64f6bcb` (verified here as
a reachable ancestor of their `origin/master`, docs SHA carrying docs).


**⚑ AND THE HALF THAT COST ME MORE THAN THE CATCH: RUN `--stat` ON THE SHA YOU PROPOSE, NOT ONLY ON THE
ONE YOU DOUBT.** This seat named a replacement anchor by inferring from a commit's subject line and it was
also a docs commit; on the one chain where it was measured, subject-line inference failed at two in three.
A subject line describes what a commit is *about*; `--stat` is the only thing that says what it *contains*.
*(The archaeology: `OVERSEER-LOG.md`, 2026-08-27.)*

**⚑ THE PROCESS LESSON, which aeon called out explicitly and which is why this was cheap: I sent it

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 987-989.)*
HEDGED (*"treat this as a reading, not a finding"*), and that is what made it worth sending.** It was
50% right (symptom yes, diagnosis and replacement no). Sent as a finding it would have cost the same
commands with friction and put a wrong diagnosis into their tree with my confidence attached; sent as a
reading it cost them three commands and produced a rule neither lane had. This is protocol bar 20's
hedging clause paying out in the direction people doubt it: **the hedge is not weaker, it is what let a
half-wrong flag be useful instead of expensive.**


**▶ AND THE SCOPE-MARKING BAR IT ARRIVED WITH, which is aeon's and is the more reusable half.** Their
mis-filed ask traced back to a sentence **in our own module docs** (*"`self_cycles` has no such
lag"*) that is true of routine rows and false of interrupt buckets and **did not mark which it
meant**. They carried it across the boundary; the sentence let them. Their framing, worth keeping
verbatim: *"a relayed premise inherits no more scrutiny than the claim it supports."* **A rule that
is true of one kind and silently false of another must say which at the point it is stated**, not in
a later paragraph a reader may never reach. Fixed at source in `profiler.rs`. Also theirs, and
sharp: they sorted the gap **from our wire schema** (no such key, therefore genuinely-new) rather
than **from the quantity they needed measured**, and a schema can only tell you whether a *name*
exists, so sorting from it lands in the expensive bucket by construction.

**▶ NEW BAR, 2026-08-24: `docs/lane-status.json` is the OVERSEER'S file. Never let a dispatched
agent edit it, and say so in the brief.** Earned the same day: the Q-PROF-STRADDLE agent did
excellent work and, closing out, marked its queue item `"state": "done"`, an enum the suite
contract does not define. The Dominion console **rejects the whole document on one bad enum**, so
that single word would have made this lane invisible on the owner's board for the second time in
one night. The agent could not have known: the valid states live in `empyrean/contract/LANE_STATUS.md`,
not in this repo, and nothing it could read locally would have told it. **The fix is structural, not
educational**: a live operational file that the console parses is not part of any work product, and
handing it to an agent puts a contract the agent cannot see in the path of a commit it must make.
Agents report their queue outcome *in their report*; the overseer transcribes it. Related: a
finished item **leaves** the queue: `done` is not a state, it is an absence.

- **Contract-first, always**: CR → un-framed adjudication → apply fixes → the code and its
  amendment merge in one window so `protocol.md` never describes a server that does not exist.
  Post-adjudication changes ride **deltas** (same adjudicator, same standard). Adjudication is not
  optional even for your own rulings: a ruling authorizes the change; adjudication is what
  authorizes the *text*.
- **⛑ CLIPPY LOCALLY IS NOT THE GATE, AND ON THIS MACHINE THE GATE CANNOT BE RUN AT ALL** *(measured 2026-09-16, this seat, on a landing that went CI-red after a green local verification)*. CI installs the **declared Rust FLOOR** (`Cargo.toml` `[workspace.package] rust-version`, today 1.96.0, read by `tools/rust-floor.sh` — deliberately, so the build is pinned rather than tracking `stable`). This machine has distro rust **1.98** and **no `rustup`**, so the floor's clippy is not installable here: a lint that fires at the floor and not at 1.98 is invisible locally **by construction**, and no amount of local care substitutes. Measured instance: `clippy::nonminimal_bool` on `!(self.vblank(line_start) || !self.display_enabled())` — red at 1.96, silent at 1.98, fixed by De Morgan at `0647d6f`. **Two consequences.** (1) Run CI's command verbatim, **`cargo clippy --all-targets -- -D warnings`**, not a `-q` variant without `-D warnings`: this seat ran the latter, read `EXIT=0`, and recorded "clippy clean" — a command that **cannot fail on a lint** reported as though it had passed one, which is the vacuous-green class this same seat had flagged in the save fingerprint an hour earlier. **The instrument bar applies to your own verification commands, not only to the agent's gates.** (2) Even run verbatim it is necessary and not sufficient here, so **a landing is not clean until CI's own run is read to completion** — `gh run view <id> --log-failed`, never the status field. Likely to hold in every Rust lane that pins a floor; relayed to the hub.
  ⛑ **CORRECTED SAME NIGHT, AGAINST THIS SEAT, AND THE CORRECTION IS THE USEFUL HALF: THIS REPO ALREADY HAD THE GATE AND I DID NOT RUN IT.** `tools/land.sh` is the landing script, and its G5 is `cargo clippy --workspace --all-targets --release -- -D warnings`; its HEADER already carried the `-D warnings` lesson, measured, before tonight (*"without it clippy exits 0 on every lint it finds, i.e. the gate cannot fire"*). I hand-typed my own gate list instead of running it, then re-derived its documented lesson the expensive way, off a red CI. **The rule is not "run CI's command verbatim" — it is RUN `tools/land.sh`, which exists so that nobody assembles the gate list from memory at 3am.** Hand-rolling also skipped G2b (the lane files the console parses), G3 (fast-forward) and G6 (the derived leg count), none of which happened to bite. **Sigil derived the same rule independently the same night on a different route, and their sentence is better than mine: `-D warnings` is the flag that turns clippy into an instrument with a verdict** — flagless, it prints findings AND exits 0, so the exit code is the only part of the output that looks like a verdict and is not one (hub rule 3c, empyrean `7345417`).
  ⛑ **AND THE GATE WOULD NOT HAVE CAUGHT THIS ONE — MEASURED, NOT ARGUED.** I put the pre-fix spelling back on disk and ran G5's exact command: **exit 0, zero mentions of `nonminimal_bool`.** G5 runs the local toolchain, so a floor-only lint is invisible to it exactly as it is to any hand-typed command. **So both halves are live and neither substitutes for the other: run `tools/land.sh` because the gate list is not yours to remember, AND read CI's own run to completion because a floor-only lint exists nowhere else on this machine.** A landing that skipped the script and then went red on a lint the script could not have caught is two independent defects that happen to share an incident; fixing only the one you tripped over leaves the other armed.
- **Verify firsthand before accepting**: run fmt + clippy ×2 + the full aggregate yourself
  (`cargo test --workspace 2>&1 | grep -E "^test result" | awk '{p+=$4; f+=$6; i+=$8; n+=1} END
  {print "LEGS="n" PASSED="p" FAILED="f" IGNORED="i}'`). Agent reports have matched every time.
  Verify anyway; the one time they don't is the point.
  ⚑ **And when you grep the same output for failing NAMES, anchor it `^test [^ ]+ \.\.\. FAILED`, never
  `^test .* FAILED`** *(aeon, 2026-09-09; reproduced here)*: cargo's own summary line
  `test result: FAILED. 11 passed; 1 failed` begins with `test ` and ends in `FAILED`, so the loose form
  matches it too and **one real failure reports as two** — at exactly the moment you are least inclined to
  re-read the output. Measured: loose matches 2 lines against a 1-failure log, strict matches 1. Not in any
  committed script here; it was in this seat's hand-typed verification commands all night, and never fired
  only because every run was green.
- **Serialized cargo**: NEVER two cargo runs anywhere in this repo at once, including
  isolated-worktree runs while any agent runs cargo (measured: legs truncate with a spurious
  failure, three data points). Queue acceptance gates; verify BEFORE resuming an implementer.
  Short release builds for an owner-facing unblock are the recorded exception (nice -19, logged).
- **⚑ RED-FIRST IS NECESSARY AND NOT SUFFICIENT: a poison can come back GREEN with the guard
  perfectly sound** *(2026-08-22, aurora; three green poisons in one parcel, none of them a bad
  guard)*. The three classes: (1) the row aimed at a branch **a pre-check makes unreachable**;
  (2) the row proving only *"it refused"*, which **two independent code paths** both satisfy, so
  deleting the guard under test leaves the *other* mechanism holding it green (the matcher clause,
  but the collision is two **paths** producing one observable, not two messages sharing a phrase);
  (3) **the row measuring the WRONG OBSERVABLE**: the fixture left the thing resolvable, so the
  catch site the test was *named after* was never entered. **Planting a violation could not have
  revealed the third; only asking whether it measured the right quantity could.**
  **⚑ THE TEST FOR WHETHER A SPLIT LIKE THIS IS REAL (aurora's, and it generalises past poisons):
  do the two classes have DIFFERENT FIXES?** A matcher collision is repaired by re-pointing at wording
  only that rule uses. Two-paths-one-observable **is not repaired by touching the matcher at all**:
  the matcher can be perfectly precise and the row still worthless; it is repaired by asserting
  **which path ran**. *A bar that cannot tell them apart sends you to the wrong repair*, which is the
  cost of collapsing them. **Their tell for the confusable pair: is the observable UNIQUE to the
  rule?** Unique → the assertion is too loose (matcher). Not unique → the assertion may be exact and
  still prove nothing (two paths).
  **Operational form, to be asked per assertion:** *if this row went green for a reason OTHER than
  the rule holding, what would that reason be?* Then check that specific reason, and report the
  alternative green-path considered and how it was ruled out. **A `None`/absent/empty on either side
  of a comparison must be LOUD, never green**; that is where all three hid, each reading as healthy.
- **Mutation discipline**: every evidence-bearing test carries a recorded mutation (edit → touch →
  observe "Compiling" → named FAIL → revert → green; cargo's fingerprint is MTIME-based). A
  mutation that catches nothing is strengthened BEFORE recording, never recorded hollow. When an
  expectation and the code disagree, investigate to ground truth. Three times today the code was
  right and the expectation wrong.
- **Currency scrutiny**: goldens never regenerate silently; every mover carries a named, measured
  mechanism in its `cause:` comment; any unexplained mover is a STOP-and-report, not a re-pin.
  Zero-file-diff on `crates/oracle-core/tests/` is the default expectation for bus work; breaking
  it is a named decision.
- **Demands are committed artifacts**: transcribed from the consumer's own source with anchors
  (never from a relay; relays get flagged as such until an anchor lands), corrections recorded
  supersession-style (original visible, correction over it), gap triage into
  satisfied / composable-today / genuinely-new.
- **⚑ A CONFIDENT MECHANISM FROM THIS SEAT IS A HYPOTHESIS, AND THE RECEIVER'S OWN ALREADY-RUN
  COMMAND OUTRANKS IT** *(2026-08-22, found by the sigil lane against themselves; proposed upstream
  to empyrean, which is where it belongs, so do not treat this entry as the rule's home)*. I sent a
  peer a confident mechanism for why three stale citations survived (*"they resolve into a different
  real repo and hand you a plausible wrong file"*). It was **wrong**: the leaves 404. My error was
  reusing a real lesson on an instance I had not measured. **Theirs was worse and is the durable
  half: they had ALREADY RUN the refuting command in the same session** (a directory listing and a
  probe that printed `No such file or directory`), **read the output, used it to conclude the cite
  was stale, and then wrote a row asserting my mechanism anyway**, because mine was a better-sounding
  story and arrived with a post-mortem attached.
  **Why this is a bar and not an anecdote: the second failure does not need a peer to be wrong; it
  only needs a peer to supply the frame.** A confident mechanism overwrites a measurement the
  receiver already holds, silently, and nothing in either session looks like a conflict because the
  measurement was never re-read.
  **▶ THE DELEGATION COROLLARY, which is the operative half for this seat and is strictly worse than
  the peer case.** A peer has standing to push back; **an agent has almost none.** Four of my stated
  facts were corrected today by agents who checked them, and every one of those was a *fact*, which
  is checkable. **A stated MECHANISM is far more dangerous than a stated fact**, because it explains
  the evidence rather than competing with it: an agent that measures something inconsistent with my
  mechanism will tend to reconcile the measurement *to* the story instead of reporting the conflict.
  **So: state mechanisms as hypotheses in briefs, explicitly labelled, and say in every dispatch that
  the agent's own command output outranks anything I asserted.** The instance that saved us here is
  the shape to demand: the README agent verified all three targets **individually before writing**,
  rather than performing the search-and-replace my framing implied.
  **Second clause, sigil's, and it prevents the over-correction:** when a correction lands, **check
  which half of the claim actually moved before discarding the whole thing.** The rename shape and
  the code-comment rule both survived my wrong mechanism intact, and retracting them along with it
  would have destroyed two sound rules to fix one bad sentence.
- **Dispatch ahead of a survey only when you can name what the survey could change ABOUT THAT
  PARCEL**, never on the argument that it changes nothing downstream *(2026-08-22; I asked
  empyrean to challenge the step-trio call and they ratified the instance, rejected the
  generalisation)*. The trio was sound: its fragments were final upstream, so the survey could only
  reorder what followed. The generalisation fails because **the survey's most valuable output was
  correcting nine of my own brief-facts, and that value is uncorrelated with whether the fragments
  were final.** Pricing is the stated reason to survey; fact-checking the controller is the one
  that actually pays, and it is exactly the one a "it can only reorder what follows" argument
  discards without noticing.
- **Never record an approval whose granting act you have not seen: cite the ruling, not a status
  field** *(2026-08-22, from empyrean, who found it in their own doc; see the Fable-seat audit in
  The role, in `OVERSEER.md`, which is this lane's instance)*. Boot docs are snapshots that age while logs
  accumulate, and an owner ruling lands in the middle where head-and-tail reading never sees it,
  so **grep the history for an item before putting it to the owner OR funding work off it.** Both
  directions are failures: re-asking a settled question wastes his time, and acting on a
  self-declared approval spends his money on a decision he never made. The nastiest form is a
  document's description of ITSELF hardening into an owner decision and then into an instruction
  *not to check*, inside the one file every cold session reads and nobody re-reads.
- **Dedicated adversarial review** for load-bearing slices (the slice that carries an arc's central
  claim gets its own reviewer with explicit targets and required explicit negatives).
- **Better-than-the-floor** on every request; improvements additive so the migrating consumer
  loses nothing; the pre-release window for REQUIRED additions shuts at first ship. Spend it
  deliberately, once.
- **A gate described for someone else to carry must name its ASSERTION, not its shape.** Earned
  2026-08-23 with aeon, on both sides in one exchange. Our reconciliation identity is a **loss**
  detector, not a correctness proof: a suppressed interrupt bucket *conserves* its cycles into
  `unattributedCycles`, so **the identity closes with that term arbitrarily large** and closure alone
  is satisfied by exactly the case the gate exists to catch; only the explicit `== 0` assertion
  fires. A peer booked the requirement as *"carries the identity check"*, having read the proof, and
  would have shipped a **correctly-described gate whose teeth were gone**: not a wrong gate, not a
  missing one, a gate whose shape a porter inherits with no reason to look under it. Note where this
  bit: inside the very booking written to argue that a mechanism beats a remembered rule. **The
  mechanism only beats the rule if the assertion survives transcription**, so when writing a gate
  into prose for a consumer, name the thing that fails, and re-derive rather than paraphrase when
  carrying someone else's.

**▶ NEW BAR, 2026-08-26: CHECK THE VINTAGE OF THE PROCESS, NOT THE VERSION OF THE FILE. A long-lived
interpreter is a stale artifact class, and no in-tree check can see it.** Found by this seat while
reaching for an unrelated ⟨RUNTIME⟩ debt; corroborated independently by aurora, sigil, seraph and
dominion within the hour. `oracle-old` `07314aa` (08-25 21:09) made the MCP shim spawn its own private
`oracle-aether` and stop dialling the well-known socket. **Every suite lane's shim process started
08-25 19:53-20:29 (before that commit), and Python reads its source at process start.** So all six
lanes were executing the pre-ruling version, wired straight into `/run/user/1000/oracle.sock`, held by
the OWNER'S live `oracle-frontend` player. Proven by socket-inode pairing for oracle and aeon; aeon had
already `reload_rom`'d his running window onto a worktree build at ~18:20Z in perfect good faith,
**on a banked note that said a fresh session gets a private instance**, a note that was true of the
file and false of the process.
**This is yesterday's *a merged serve is not a served method* bar on a NEW artifact class.** That one
names compiled binaries and compile-time-frozen paths. This is neither: the file on disk is correct,
the fix is merged AND pushed, `git log` looks finished, and the defect exists only in the memory of a
running process. **No sweep, no audit, no cold read of the tree can reach it**: the tree is right.
**⚑ AND THE REMEDY IS NOT THE OBVIOUS ONE: a `/clear` does NOT fix it; only a session relaunch does.**
The shim is spawned by the session process, so clearing the conversation leaves the same interpreter
running. Measured firsthand here, and this is the cheap corroboration worth copying: **this session was
`/clear`ed and its shim's start time did not move** (shim 287372 at 20:29:19, one second after its own
session process at 20:29:18). aurora reached the same conclusion from the *other* direction (that the
shim is on the process command line), which is bar 19's genuine corroboration rather than echo, because
neither derivation could have shared the other's parameter.
**aurora's one-command discriminator, adopted: `pgrep -P <shim-pid>`.** A post-fix shim owns a child
`oracle-aether` on a `/tmp/oracle-mcp-*` mkdtemp socket; a pre-fix one has no child. Both kinds appear
in a single `ss -lxp`/`pgrep` listing, so pre- and post-fix sessions are **visibly different in one
command** with nothing to reason about.
**The failure that nearly happened to three separate lanes' documentation, and it is the durable half:
aurora's own `OVERSEER.md` asserted the opposite**: *"`mcp__oracle__*` in this session SPAWNS A PRIVATE
EMULATOR by default — it is NOT the window the owner is watching"*, written that same day, correctly,
**from the file on disk**, and false for every interpreter older than 21:09. They fixed it at
`83fcb64`. **A claim about RUNNING STATE banked as though it were a property of the code** is the
perishability preamble's sharpest instance yet: the anchor was valid, the source was authoritative, and
the sentence was still wrong the moment it was written.
**Operational form: before trusting any tool that dials something, ask when its PROCESS started
relative to the fix you are relying on**, and write the vintage condition into the note, never the
conclusion alone.


**▶ NEW BAR, 2026-08-27: THE OPS LINE THAT IS NOT IN THE DISPATCH IS NOT IN THE DISPATCH. Carry the
worktree `vendor` symlink into every brief that will run cargo.** The Ops section below has said *"fresh
worktrees: `ln -s <repo>/vendor vendor`"* for weeks. It was **still missed on a dispatch this morning**,
because the brief is composed from the invariant block and the parcel's own grounding, and an Ops line
sitting in this file is not either of those. The agent lost time on a baseline that would not reproduce:
eight `save_state::tests::*` rows **panic** (not skip) on the missing vendored ROM, and the resulting
`exit 101` is indistinguishable at the aggregate line from two other causes this repo has recorded.
**The fix is structural, not educational**: an overseer who has read this file every session still omitted
it, so the rule is that the vendor line is part of the *brief template* for any cargo-running dispatch,
alongside the base check. Related and already booked: the same class as *a merged serve is not a served
method*; knowing a thing in the tree is not the thing reaching the process that needs it.


**▶ NEW OPS LINE, 2026-08-30: NEVER CITE THE TIP. CITE THE COMMIT THAT CARRIES THE ARTIFACT, EMITTED

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1196-1220.)*
FROM THE PATH.** Third instance of the anchor-class family against this seat, caught by the hub.


**The corrective is constructive, not verifying**, because `--stat`-after-the-fact is what the existing
bar already prescribes and it did not fire; I had no reason to doubt a hash I had watched go out:


**▶ NEW OPS LINE, 2026-08-30: A KILLED SUITE LEAVES A LOG THAT AGGREGATES CLEAN. COUNT THE LEGS, NOT THE

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1226-1252.)*
FAILURES.** Nearly quoted as a merge verdict by this seat.


**Corrective, and it is cheap because it is one more line in the same command:** a verification asserts
its own **completeness** before its verdict.


**▶ CORRECTION, 2026-08-30: OUR HEADLESS RECIPE'S "BOTH GUARDS" ARE ONE GUARD TWICE, AND IT IS THE

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1259-1285.)*
GUARD A PEER JUST MEASURED AS INEFFECTIVE.** Prompted by aurora's O36 finding (relayed by the hub);
the defect below is ours and was found by reading our own source, not theirs.


**▶ NEW OPS LINE, 2026-08-30: DO NOT COMMIT WHILE A VERIFICATION RUN IS IN FLIGHT. IT INVALIDATES THE

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1292-1312.)*
BUILD-ID GATE AND THE FAILURE LOOKS LIKE THE PARCEL'S.** Third instrumentation failure in one night, and
the only one that produced a red that was entirely mine.


**Corrective:** a verification prints `HEAD_AT_START` and `HEAD_AT_END` and **they must be equal for the
verdict to count**. Bank findings *after* the run, never during: the twelve minutes are not free time,
they are part of the measurement.


**▶ SCOPE CORRECTION, 2026-08-30: `screen_text` DOES NOT END AEON'S EYEBALL REQUESTS, AND THIS FILE SAID

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1319-1330.)*
IT WOULD.** Corrected by aeon against a claim this seat made to them, which was taken verbatim from our
own queue row.


**The durable shape, and it is why this is an ops line rather than a typo fix: the over-claim was in a
QUEUE TITLE, which is the one place nobody re-derives.** It was written when the item was a sketch,
inherited by every status file since, and finally exported across the fence with a seat's confidence
attached, where the only reader who could refute it happened to be the party it was about. **A queue
row's justification ages exactly like a precedent narrative and nothing re-reads it.** When an item's
design lands, re-read its own queue title against what was actually built.

**▶ F-ACCEPT-TABLE-CROSSCHECK-BLIND (registered 2026-08-30, emitter behaviour change, NEEDS A RULING).**

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1351-1358.)*
`tools/legacy_accept_table.py`'s axis-A/axis-B reconciliation adds to `claimed_lines` **before the row is
written**, so it is **structurally blind to a row-level drop**. Measured firsthand: with the four unguarded
`addr` rows dropped cleanly, `--fail-on-gap` prints `cross-check : AGREES`, `parse complete : yes` and
**exits 0**, while `UNGUARDED reads` silently falls **43 → 39**. ⚑ **So the tool's own headline safety line
is not evidence of the thing a reader takes it for**: it witnesses that every *access* was claimed, never
that every *row* survived. The 57-test suite catches this by named assertion; `--fail-on-gap` does not, and
`--fail-on-gap` is what a CONSUMER would wire into a gate. **Revival: aeon wiring the table into their gate;
they must be told the gate flag is weaker than the suite** (told 2026-08-30). Fix would be `--fail-on-gap`
independently verifying row presence against a source-derived expectation; that is an emitter behaviour
change and was correctly kept out of the hardening parcel.


## The landing checks — RUN THE LIST, do not compose it each time

*(Written 2026-09-09 after this seat ran three different check sets across three merges in ninety
minutes and reddened `main` with the one it shortened. The omission is invisible from inside a single
landing: the parcel that skipped a check had the HEAVIEST evidence of the three, an 87-leg suite run.)*

**On the MERGED tree, never branch-side, in this order. All five, every time, whatever the parcel:**

1. `cargo fmt --all -- --check` — exit code captured **outside any pipe**.
2. `cargo clippy --workspace --all-targets -- -D warnings` — **`--workspace` and `--all-targets`**, not
   per-crate and not lib-only. The lint that broke `main` was in a **test target** in a crate the parcel
   touched, and a `-p` scoped run is what an agent will report from its branch.
3. The tests. Full suite when code moved; a targeted run is acceptable **only** for a change proven to
   add no executable lines, and the proof is stated (for Rust: no non-comment additions **and no code
   fences**, since a fenced doc-comment becomes a doctest that runs).
4. **Completeness, not just green**: the leg count. It MOVES when a parcel adds a test target — expected
   86, got 87, reconciled as the new `toolchain_floor` target. **A moved count is an explanation or a
   problem; find out which.** Never read a capped, killed or absent run as a pass.
5. `git push`, then **verify `origin` actually moved** — the push is not the act, the remote moving is.

**Then, and this is the step that was missing entirely tonight: READ CI.** Four parcels landed on a
signal nobody looked at. `gh run list --limit 6 --json conclusion,headSha,status`. **A normal run here
takes 35-45 minutes** — measured, so a long one is not a stall; judging a duration against an assumption
nearly produced a false alarm about a stuck runner an hour after the false all-clear.
⚑ **A wait-for-CI loop that times out EXITS 0 and reads exactly like success.** Re-read the run list;
never trust the waiter's exit code, and never trust `cmd | head; echo $?`, which hands back `head`'s.

⚑ **If your own landing breaks something, test the explanation that makes it YOUR FAULT first.** See the
ops entry below; the structural story arrives first and costs the most.

## Ops (each line is a paid-for lesson)

**⚑ RE-VERIFY ON THE MERGED TREE MEANS THE SAME CHECKS EVERY TIME, AND THE VARIANCE IS INVISIBLE FROM
INSIDE A SINGLE LANDING** *(2026-09-09, this seat, caught by CI after four parcels had landed)*. Three
merges in ninety minutes: the citations parcel got fmt + check + clippy; the rust-floor parcel got fmt +
clippy + its own test; **the six-guard parcel got the full suite + fmt and NO CLIPPY.** It reddened `main`
on a `clippy::int_plus_one` in a test that parcel added. Each landing looked thorough **in isolation** —
the guard parcel's evidence was the *heaviest* of the three, an 87-leg suite run — and the omission is only
visible by comparing landings to each other, which nobody does. **Write the landing checks down and run
the list, or the set silently varies with what felt proportionate that hour.**

⚑ **AND THE HALF THAT IS WORTH MORE THAN THE OMISSION: THE FIRST DIAGNOSIS WAS SELF-EXCULPATING AND
WRONG.** This seat wrote — into a commit message and into a comment — that the lint fires on CI's 1.96
floor and not on a newer local clippy, i.e. a **toolchain skew**: structural, unpreventable, nobody's
fault, and consistent with the L-13 ruling banked hours earlier, which is exactly what made it feel
confirmed. **Measured, it is false: local clippy 0.1.98 flags it by default, exit 101, same lint, same
line**, proven by re-introducing the bad form as a positive control. **The gate that would have caught it
was one this seat SKIPPED, not one it lacked.**
**The durable rule: when your own work breaks something, check the explanation that makes it your fault
BEFORE the one that makes it structural.** The structural story is the one that arrives first, survives
scrutiny longest, and costs the most, because it sends the next session building tooling for a problem
that does not exist — here, a whole row proposing local-toolchain-matching apparatus for a skew that is
not there. Same family as bar 9's *"the tell is not the red build, it is WHO IS EXPECTED TO MOVE"*: a
diagnosis requiring work from nobody is the one to distrust.


**⚑ A RECEIVER WHO SILENTLY REPAIRS A BROKEN INSTRUCTION DESTROYS THE ONLY SIGNAL THAT WOULD HAVE
CORRECTED THE SENDER** *(2026-09-09, this seat's own defect, found when the hub self-reported the
instruction)*. The hub handed this seat `git -S '<text>' log -- <path>` as the way to read a standing
ruling firsthand. **It is malformed** — `-S` is a `log` option, not a `git` option, and it dies with
`unknown option: -S`. **This seat ran the CORRECT form** (`git log --oneline -S …`) without noticing,
got the right answer, and **said nothing**, so the hub went on to write the broken form repeatedly in
messages. It surfaced only because the hub audited its own prose.
**The generalisation, and it is the one worth keeping: a competent receiver is an ERROR-ABSORBING
SURFACE.** Silent repair looks like fluency and is indistinguishable, from the sender's side, from
the instruction having been correct. Same mechanism as `2>/dev/null` deleting the artifact that would
have corrected a reading, and as the protocol's `ls`/`eza` pair — the lane that saw the error re-ran
and was saved; the lane that suppressed it formed the false belief. **When you fix someone's command
to make it work, say that you did.** It costs one clause and it is the only channel by which the
sender can learn.

**⚑ AND THE MASK THAT HID IT: A FAILING COMMAND INSIDE A PIPE REPORTS THE PIPE'S STATUS, NOT ITS OWN.**
`git <bad> 2>&1 | head -3; echo "exit=$?"` prints **`exit=0`** while the command failed, because `$?` is
`head`'s. Bit this seat TWICE in one session — the other was `cargo check … | tail` reporting an EMPTY
status that would have read as a pass. **Capture the exit code outside any pipe** (redirect to a file,
then `echo $?`), or the status you quote belongs to the last thing in the chain rather than the thing
you are testing.


**⚑ RESOLVE A PEER'S DEFAULT BRANCH; NEVER ASSUME IT — AND THE ASSUMPTION FAILS SILENT** *(2026-09-09,
found by a dispatched agent against this seat's own brief; extended by the hub, who measured wider than
this seat did and was right to)*. Measured across all six repos: **aeon, sigil and AURORA are `master`;
empyrean, seraph and oracle are `main`**, and `git rev-parse --verify origin/main` **fails in all three
master repos**. This seat had been writing `git -C ../<repo> show origin/main:<path>` into dispatch briefs.
**The failure mode is the whole hazard: that read comes back EMPTY and reads as "nothing there", never as
an error** — bar 16(d)'s absence class, hosted in boilerplate, which is the worst possible host because
boilerplate is not re-read. **Correct form, which costs one command and cannot rot:**

```sh
B=$(git -C ../<repo> rev-parse --abbrev-ref origin/HEAD)   # e.g. origin/master
git -C ../<repo> show "$B:<path>"
```

⚑ **AURORA IS THE ONE THIS SEAT WOULD NOT HAVE CAUGHT**, because its own peer reads point at aeon and
sigil; the hub found it by enumerating all six rather than the two in the conversation. **Bar 14's
consumer-set rule arriving on a branch name.**

⚑ **AND THE HOST IS THE FINDING.** The shared `dispatching-empyrean-agents` skill was checked and carries
**zero** `origin/` occurrences, and this repo's own docs mention `origin/main` only for empyrean and
oracle, which genuinely are `main`. **So there was no file to correct: the wrong instruction existed only
in brief text composed fresh at each dispatch.** That is bar 20 (*mail is not part of the tree, so no tree
can surface a wrong claim made in mail*) arriving on **dispatch briefs**, which are the same artifact class
— authored once, read by one agent, never re-read, and invisible to every sweep this repo runs. A brief is
mail. **Anything a brief asserts twice belongs in THIS file, where a later dispatch will meet it.**



**2026-09-09, and the first is THIS SEAT'S defect in its own dispatch briefs.**

* ⚑ **A COMPLETED AGENT'S WORKTREE IS NOT IDLE.** Measured 2026-09-09 while considering a routine tidy of
  six merged parcels: the worktree of an agent that had reported done, been merged and been reported to the
  owner still had **live `zsh` processes with their cwd inside it**, the oldest started hours earlier, plus
  transient `sleep`s from its own polling loops. No build was running. **"The agent finished" is a statement
  about the agent, not about its directory** — and the CWD half of the shared-machine check is what catches
  it, because a cmdline sweep sees `sleep 30` and learns nothing.
  ⚠ **CORRECTED WITHIN THE HOUR, AND THE CORRECTION IS THE POINT. I wrote that aurora's tree showed "the
  identical shape, so this is the harness, not one agent". The shape was identical and the FACT WAS
  OPPOSITE**: aurora's processes belonged to an agent still working mid-parcel, not to a finished one. So
  this is **n=1, not two independent cases**, and "it is the harness" is unsupported. **The process shape is
  NOT diagnostic**; I extracted a pattern from one case and carried its conclusion onto the next without
  testing it, which is the twin of the extracted-number bar below.
  ⚑ **The discriminator aurora supplied — process shape tells you something is RUNNING, commit recency tells
  you whether it is WORKING — is necessary and NOT sufficient, measured against this tree the same hour.** A
  worktree INHERITS ITS BASE'S HISTORY, so `git log -1` in it answers about the BASE until the agent commits
  anything. My live agent's worktree reported *"12 minutes ago"* — the age of a commit **I** had just made on
  `main` — with **zero** unique commits and no agent output at all. Had I committed a minute earlier it would
  have read *"1 minute ago"* and looked maximally busy. And it does not separate the two cases either way:
  `rev-list --count <base>..HEAD` is **0** BOTH for a fresh worktree and for a merged one, which is the
  empty-range trap a third time tonight.
  **So no repo-side probe reliably answers "is this agent alive".** The authority is the harness — this
  lane's own `inFlight` and its task notifications. Use the repo probes to decide what is SAFE TO DELETE
  (they fail closed), never to conclude an agent has finished.
  ⚑ **NOR DOES THE AGENT'S OWN REPORT.** The same agent later stated *"cleanup verified — no stray
  processes remain in my worktree"*. Measured immediately after: **four**, two of them `sleep`s spawned
  **at 12:27, AFTER it reported done**, so its polling loop was still cycling as it declared the tree
  clear. It is not lying and it is not careless — **an agent cannot observe what outlives it**, which is
  the whole reason the peer rule says verify a peer's claims firsthand rather than reconciling verdicts.
  Do not forward an agent's cleanup claim; measure it.
  **Two consequences.** (1) Do not prune a merged parcel's worktree on the strength of the merge; run the
  check first. (2) **The count is unstable by construction** — the `sleep`s expire and are replaced, so two
  readings minutes apart give different PIDs and different totals. Enumerate and identify; a number here is
  not a fact, which is the amended rule's own point arriving on a new surface.
* ⚑ **`git merge-base --is-ancestor <branch> main` is TWO-VALUED and a fresh parcel branch reads as MERGED.**
  A branch just created at `main` with no commits yet is an ancestor of `main`, so a tidy driven by that
  predicate would delete the branch an agent is *currently working on*. Seen live: `parcel/bus-visible`
  listed as merged while its agent was running. Disambiguate with `git log <base>..<branch>` (unique
  commits) before believing it, per the protocol's own "empty range is two-valued" precedent.

* ⚑ **TELL EVERY AGENT WHERE TO PUT SCRATCH FILES, BY A RUN-UNIQUE PATH.** Two agents in this lane's
  2026-09-09 wave shared the session scratchpad and one **overwrote the other's `ledger.py` mid-run**.
  Harmless that time (the victim's rows were already committed) and it need not have been: a scratch
  collision corrupts silently and looks like your own bug. **The dispatch skill already warns that two
  agents told to build and never told where to put scratch files are each other's concurrent writer BY
  CONSTRUCTION — three briefs went out without the clause anyway.** Knowing a rule is not applying it;
  put the path in the brief.
* ⚑ **DO NOT COMMIT WHILE A SUITE IS READING THE TREE — and the reason is not the obvious one.**
  `oracle-aether`'s `the_compiled_in_build_id_still_names_this_tree` correctly catches a mid-run commit:
  the id is compiled in from the HEAD at build time, and committing moves HEAD out from under it.
  **The hazard is what that does to the RUN: without `--no-fail-fast` the failure aborted at 36 of 85
  targets, and an aborted run's tail is indistinguishable from a completed one.** Same family as the
  capped-run false green above, reached by a different door. Always `--no-fail-fast`, always check the
  target count, never read the tail.
  ⚠ **SCOPE, corrected by this seat against the agent that supplied the lesson: the suite is NOT "red on
  any dirty tree".** `dirty` is computed over a DECLARED path set — `oracle-aether/src`, its `Cargo.toml`
  and `build.rs`, `oracle-core/src` and its `Cargo.toml`, the workspace `Cargo.toml` and `Cargo.lock`
  (`build.rs::build_input_paths`). **`docs/` is not in it**, which matters daily because
  `docs/lane-status.json` is tracked and deliberately never committed, so this tree is permanently dirty
  by one file and the suite runs fine. **The over-broad form of this rule cost a real run: this seat
  killed a healthy suite on the strength of it.** A lesson stated wider than its measurement is how a
  correct finding produces a wrong action — and the fix is to name the scope, not to trust the summary.
* **A capped foreground run is a silent false green.** `cargo test --workspace` here exceeds ten minutes;
  a run killed at a harness cap aggregates a clean log and exits like a pass. Detach it, write your own
  end marker, poll your own log for that marker, and prove completeness with the TARGET COUNT (85 on
  2026-09-09) rather than the exit code.
* **A WRONG INSTRUMENT DOES NOT FAIL, IT ANSWERS.** Four instances in one day across three lanes and the
  hub: a grep on a wrong path returning `0` (reads exactly like "no such mechanism"); `/proc/<pid>/fd`
  returning 0 for both candidates because a listening unix socket's fd is `socket:[inode]` and never a
  path, where `ss -xlp` answered in one line; a peer reporting a number "unavailable" before looking for
  the instrument; and a 100,000 BYTE bound measured with a CHARACTER count, committing 107 B over while
  its own instrument printed "under". **A zero, an absence or a clean pass from an instrument you have
  not positively controlled is not evidence yet.**
* **`(deleted)` on `/proc/<pid>/exe` says the file was REPLACED since exec, not that the process is old.**
  The image is still readable and `strings` it settles what the process actually contains. This seat read
  the marker as a verdict and told the owner his window lacked fixes it was running.

**▶ `lane-status.json`: THE BOOT CURL VALIDATES THE FILE YOU WROTE AT BOOT AND NOTHING AFTER IT** (2026-08-30,
this seat, measured). I wrote `"state": "done"` on a landed row after a merge. **`done` is not in the
vocabulary** (`doing | next | open | blocked`; a landed row LEAVES the queue and its landing goes to
`lane-log.jsonl`). **One bad enum in one row rejects the WHOLE file**, so the owner's card for this lane went
dark, with every true thing in it, and stayed dark for about an hour. **Nothing about it is visible from
this side:** the file writes fine, `git` is happy, and the lane goes on reporting accurately to itself.
**The defect was not the word, it was that I ran the verification curl ONCE, at boot.** Every later write is
unverified unless re-checked, and I made a dozen. **Re-run the boot step's curl after ANY write to
`lane-status.json`**: it is two seconds and it is the only thing that can tell you.
⚑ **THIS RULE IS NOW CONTRACT AND EMPYREAN GOVERNS IT**: `contract/LANE_STATUS.md` §*"Verify after EVERY
write, not only at boot"*, at empyrean `origin/main` (verified here by content at `97c4f72`; the hub cited it
as *"commit after 1489413"*, which is a coordinate rather than a SHA, so it was resolved by reading the
section, not by trusting the pointer). **The text below is this repo's PRECEDENT NARRATIVE, not a second
copy of the rule**: on any disagreement the contract wins, and the rule is not to be restated here as it
drifts. Read it at a committed revision, never through `../empyrean/`.
⚑ **n=2, AND THE SECOND INSTANCE READS SHARPER THAN A REPEAT** (aurora, verified here: contract line at
empyrean `origin/main`, carrying commit `10c87ba`, a real contract+docs commit, `--stat`-checked, and this
time the hub emitted the SHA from git rather than naming a neighbouring coordinate). **sigil wrote `closed`
the same night, an hour apart, neither lane aware of the other, both having read the warning shortly
before.** ⚑ **The part worth more than the count: we reached for TWO DIFFERENT WORDS.** That is not two
lanes making the same slip; it is two lanes independently reaching for a terminal state **the vocabulary
does not have**, and picking different plausible names for it. **The error is INVITED by the design, not
merely permitted by it**: the natural word for a finished row does not exist, because the contract's answer
is that a finished row *leaves the queue* (correct, and not what a writer's hand reaches for). A rule
against `done` would not have caught `closed`, and a rule against both would not catch `complete`.
⚑ **The skill's own boot text warned about exactly this** (*"three lanes wrote `done`, which is not in the
vocabulary, in three days"*) and I did it anyway, which is the argument for the mechanism over the warning:
a rule you have read does not fire, a curl does. Found by the aurora lane reading the console, not by me:
**this lane cannot detect its own invisibility, so it depends on a peer looking.** Worth knowing when no
peers are up: the verdict is only ever one curl away, and nothing else will surface it.


**▶ NEW BAR, 2026-08-29: VALIDATE AN ARTIFACT AGAINST THE SCHEMA IT TARGETS BEFORE CALLING IT READY.

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1400-1426.)*
A "ready to merge" IS a completeness claim, and it is the one nobody thinks to check because it reads
as a status rather than an assertion.** Earned against this seat, on the same submission where it was
lecturing about unchecked residues.


**Corrective, and it is mechanical because vigilance already failed:** an artifact authored against a
schema is **run against that schema before it is handed over**, and the run's output is what the handover
cites, never "these conform". If the validator cannot be run from here, say so in the handover and name
what was not checked, rather than letting a confident cover note stand in for it.


**▶ NEW BAR, 2026-08-29: A TEST THAT ASSERTS WHAT YOU *ADDED* IS STRUCTURALLY BLIND TO WHAT YOU

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1432-1480.)*
*DISPLACED*, AND A FIXED-SIZE SURFACE MAKES EVERY ADDITION A DISPLACEMENT.** Earned on the
SCREEN-HONESTY parcel, against this seat's own green test.


**Correctives, in the order they are cheap.** (1) When adding to a fixed-width surface, assert on the
**whole** rendered string, not on the field you added; `assert_eq!(rendered, full)` is the form, and
it fails for the person who adds the *next* field too. (2) **Print it and look at it** before believing
a boolean; the arithmetic here was mine and was wrong twice before the probe settled it. (3) Ordering
is a design decision on any surface that truncates: the fields that answer *"is this window lying to
me"* go first, and that ordering wants its own test with an **anti-vacuity clause**: there must exist
a width that drops a late field while keeping an early one, or the ordering test passes on a line that
never truncates at all.


**▶ NEW BAR, 2026-08-27: A CLAIM IN OUR DOCS ABOUT A PEER'S FILE HAS A SHELF LIFE, AND NOTHING IN THIS

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1486-1504.)*
REPO CAN EVER TELL YOU IT HAS EXPIRED. MEASURED SHELF LIFE: FORTY MINUTES.** Third instance in one day,
which is what makes it a bar rather than three slips.


**The remedy, and it is cheap because it is the protocol's verified-at anchor pointed at our own
docs:** when a doc here asserts something about a sibling repo's file, **record the peer revision it
was read at, inline**: `(aeon `6e4751c3`, read 2026-08-27)`. That converts an unfalsifiable sentence
into a one-command currency check for the next reader, which is exactly what the three instances above
each lacked. **And before exporting any such claim across the fence, re-read the file at their tip**,
not the doc that quotes it.

**⚑ The sharpest form, from aeon's side of this one: a peer's warning about YOUR OWN tree is the class
you must verify before acting on, and it is the one that feels least like it needs checking**. It
arrives as help, about your own code, from someone with no motive to be wrong. They nearly briefed an
agent on our stale premise. **Our confident claim about their tree almost became their agent's
instruction**, which is the delegation corollary reaching one repo further than it was written for.


**▶ NEW BAR, 2026-08-27: DO NOT GREP A RELEASE BINARY FOR A SHORT STRING. THE OPTIMIZER INLINES IT AS

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1524-1553.)*
AN IMMEDIATE AND IT IS SIMPLY NOT THERE AS A CONTIGUOUS SEQUENCE.** Earned by nearly reporting a
stale-binary emergency to a peer who was about to make a decision on it.


**Operational form:** to ask whether a binary contains a symbol or wire key, (1) **spawn it and call
it** (the bar already says this and it is the only answer that cannot be fooled); (2) failing that,
grep the **debug** build, or the **8-byte prefix**; (3) never read a short-string absence in a release
binary as staleness. And if a static read contradicts a live measurement, **the live one wins**,
which is our own *the receiver's already-run command outranks a confident mechanism from this seat*,
arriving with the seat on the losing side.

**▶ AND THE ONE THAT COST MORE: A NUMBER OF OURS CAME BACK AS A PEER'S AND OUTRANKED OUR OWN

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1567-1597.)*
MEASUREMENT. FIRSTHAND VERIFICATION DID NOT PROTECT US; IT IS WHAT LAUNDERED IT.** Found by aeon
against this seat, same hour, and it is the reason the absence above went unbelieved for as long as it
did.


**▶ THE COMMIT-MESSAGE BAR NOW HAS TWO INSTANCES, AND BOTH FAILED BY THE SAME MECHANISM: A LINE WRAP.**

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1609-1628.)*
Protocol bar 23 (*a commit message is a claim about a diff, and nothing checks it*) came out of this
lane on 2026-08-23, from a scripted edit that **silently failed on a line wrap** while the shell let the
commit run anyway. On 2026-08-27 aeon produced the second instance in their own tree, hours after
banking the bar: a message asserting two changes, one of which *"matched nothing, because the sentence
wraps across two lines and my pattern assumed one"* (their `c136fc3c`, corrected at `95c39449`, both
verified here as reachable ancestors of their `origin/master`; they corrected the **record**, not the
history, since the first was public).


**Two corrections to how the bar is stated, both from their instance:**
1. **`;` is a rung BELOW the `&&` the bar already calls insufficient.** Bar 23 warns that `edit && commit`
   does not protect you, since a replace matching nothing still exits zero. Theirs was weaker still:
   the commit *"sat after the failed edit in the same block rather than behind it"*, so the exit status
   was **never consulted at all.** When a block does both, the commit must be `&&`-behind the edit *and*
   behind a verification, because `&&` alone is known-insufficient.
2. **Match on a short fragment that CANNOT wrap, or read the blob back.** A multi-word prose pattern is a
   bet that the author's wrapping matches yours. Prefer a distinctive short token, and then
   `git show <sha>:<path> | grep -c` the committed blob before writing the message. The assertion in the
   message and the check that earns it are separable, and the message is the cheap one.


**▶ STATUS: PROPOSED, ACCEPTED, QUEUED. DO NOT RE-PROPOSE IT.** The hub ledgered all three sharpenings
as **Q-23** in `empyrean:docs/OVERSEER.md`'s pending protocol queue, verified here firsthand on the
pushed blob at empyrean `e27362c` (which is `origin/main` itself; `grep -c '^Q-23\.'` = 1, and the entry
carries this lane's bar-21 self-discount as stated rather than dropping it). Per the owner's batching
rule it lands **inside bar 23's text as an amendment, not as a new bar**, in the next batched protocol
pass. **Nothing is owed by this lane.** The paragraph above stays as lane-local ops guidance and is
correct whether or not the protocol pass ever runs, but a session that reads *"proposed to empyrean"*
and re-sends it is spending a peer's attention on a closed item, which is the notify-on-the-dependency
bar failing from the other end.


**▶ NEW BAR, 2026-08-29: THE HEADLESS PLAYER RECIPE IN THIS FILE WAS INCOMPLETE, AND FOLLOWING IT PUTS A

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1659-1672.)*
WINDOW ON THE OWNER'S DESKTOP.** Measured firsthand while discharging the two window checks; it happened on
the first launch. The banked recipe (from the SCREEN-HONESTY parcel, above) is *"the player under `xvfb-run`
with `XDG_CONFIG_HOME` pointed at a scratch `player.conf`"*. **`minifb` prefers Wayland when
`WAYLAND_DISPLAY` is set, and every lane session on this box inherits `WAYLAND_DISPLAY=wayland-0`**, so
`DISPLAY=:91` was honoured by nothing, the log said `Wayland window`, and `python-xlib` found **zero windows
on the Xvfb**. The window was on his real screen. Killed inside the minute by recorded PID.
**Corrected recipe (both guards, because the failure is silent and lands on somebody else's screen):**
`env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE DISPLAY=:N target/release/oracle-frontend --x11 …`, with your
own `Xvfb :N` whose PID you recorded. **Then verify placement before driving anything**: enumerate windows
on the display you believe you own, and treat an empty list as the finding rather than as a slow start.
**Why the note was wrong in a way nothing could surface:** it was correct *for its author's purpose*: they
were reading a screenshot, and any window would do. It is the **isolation** claim that was never true, and
the note never said which of the two it was promising. Same class as *check the vintage of the process*: a
sentence true of the file and false of the situation. *(`xdotool` is absent on this machine; `python-xlib`
0.33 is present and gives XTEST, which is what drove the keystrokes. `import` from ImageMagick grabs the
screen; `scrot`/`xwd` are absent.)*


**▶ OPS LINE, 2026-09-08 (aeon's incident, `48c49d4e`, relayed by the hub; no oracle instance): NAME THE ACTOR IN A BRIEF'S FINISH CLAUSE.** An agent whose brief said *"never commit to master"* and also carried, in the passive, *"the branch is removed after landing, as expected"* **merged its own parcel to `origin/master`** and then reported the removal as the expected end state. The two clauses are consistent only if you already know who lands; with no actor named, the tidy clause reads as a description of a process the agent is inside rather than one performed on it, and it quietly licenses the thing the first clause forbids. **Write it actively — *the controller merges your branch and then deletes it* — never *the branch is removed*.** Checked here when the hazard arrived: both briefs in flight named the controller, and `origin/main` had not moved. Same family as the name-is-not-behaviour bar: a sentence carrying the shape of an instruction while naming nobody to carry it out.

**▶ OPS LINE, 2026-09-09 (measured here, prompted by aurora): A TEST THAT STARTS FROM A PLATFORM EVENT'S *PAYLOAD* CANNOT WITNESS THE EVENT.** The owner's drag-and-drop failed while every drop test passed. Cause, two layers, and the first is the general one: every test calls `decide_drop(&[PathBuf])` with **hand-built paths**, and the only code reading the real event, `dropped_paths(ctx)` (`rom_open.rs:281`), is in **no test at all**. So the suite begins one step *after* the step that never happens — it asserts what we do once we already have paths, which passes on any platform, **including one where the event is impossible by design** (winit 0.30.13 emits `DroppedFile` from the X11, macOS and Windows backends and from **zero** files in the Wayland backend; his desktop is Wayland). Second layer: our only headless UI harness (`crates/oracle-panels-spike/run.sh:23`) does `env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE DISPLAY=$DISP`, so every window-level measurement this repo has ever taken is an **X11** measurement and shares the blind spot by construction. **The payload-shaped test is the natural one to write and looks complete; it is exactly the shape that survives the capability being absent.** When a feature depends on a platform delivering something, name in the test doc what is NOT covered, and treat "we tested it" as a claim about the handler only.

**▶ MECHANISM, 2026-09-09, after THREE instances in one session and two reminders that did not take: THE `lane-status.json` WRITE GOES IN THE SAME ACTION AS THE STATE CHANGE, NEVER WHEN YOU NEXT SURFACE.** *(aeon's framing, relayed by the hub; adopted here because attention is the thing that kept failing.)* **Dispatching without updating `inFlight` is the same class as committing without checking the diff** — and both directions cost real work: aeon marked `atBoundary` true with `inFlight` empty, then dispatched, and the hub pushed it onto work it had been running eleven minutes; this lane's three were the mirror, a status that *outlived* the fact (a focus line ninety minutes stale, and an `awaiting` still asking the owner for an answer he had already given, **which caused the hub to ask him again**). **The board is the only artifact the owner and the hub can both see, so a lag makes both of them act on a state you have left.** Operationally: the same tool call that dispatches an agent, merges a branch, lands a parcel, or receives an owner answer also writes the status — one action, not two, and the console curl rides with it.

`cd` to the absolute repo path before ANY branch operation (a persisted cwd nearly checked out
under a live agent). Fresh worktrees: `ln -s <repo>/vendor vendor`, verify 17 TestRoms entries, and
open every dispatch with a base check (commit-message string + a file that must exist). Exact-path
`git add` only; `git show --stat` per commit; no Co-Authored-By trailers. Never `cargo test | tail`.
`pkill -f`/`pgrep -f` self-match (the waiting shell's own command line contains the pattern). Bracket the first character: `pgrep -f "[c]argo test"`. ⚠ **Bracketing is not enough when the SAME command carries the literal string elsewhere**: a heredoc writing a doc that quotes the socket path made `pkill -f "[o]rc-p/o.sock"` match its own shell and kill it mid-command (exit 144, 2026-08-26). Kill by the PID you recorded at launch, not by pattern, whenever the command also contains the text. Aether sockets live under `$XDG_RUNTIME_DIR`. `/tmp` is quota'd;
free space is not the signal. The frontend is bin-only (`pub fn` with no caller = hard error).
`ls` is aliased to eza. Owner tests run `aeon/s4.debug.bin`.
A probe socket must NOT live under the session scratchpad: that path exceeds `SUN_LEN` and the
server refuses with `cannot bind the Aether socket: path must be shorter than SUN_LEN`; use a short
`/tmp/<short>` dir. The MCP shim (`oracle-old/linux-port/mcp/oracle_mcp.py`) **SPAWNs its own
`oracle-aether` by default** (private `mkdtemp` socket, `ORACLE_ROM` default `aeon/s4.debug.bin`) and
**ATTACHes only when `$ORACLE_SOCKET`/`$EXODUS_SOCKET` is set**. It does NOT use empyrean's
`resolve_socket_path()`, so do not reason about the shim from that resolver (this seat did, and was
wrong about whose emulator it was talking to).


## Moved from OVERSEER.md 2026-09-06 (boot-read bound) — ops lessons and bars read at a moment, not at boot

*Each block below sat in the boot read as a dated registration. The rule is the durable half;
it is read before dispatching, before reviewing returned work, or before landing, never at boot.*

- **⚑ A PIPE REPLACES THE EXIT STATUS, AND IT BIT THIS SEAT TODAY TOO: n=2 in one day, two lanes.**
  aeon gave this back freely after piping their new script to `sed` and reading exit 0 on the **failure**
  path, while having cited that exact trap to other lanes all session. **We did the same thing hours
  earlier and did not notice:** `python3 tools/prove_doc_split.py … | tail -25` then `echo "EXIT=$?"`
  printed **`EXIT=0`** while the prover's real verdict was **`DISPROVED`**. We were saved only because that
  tool states its verdict in words and we read the words: **the exit code we printed and reported was
  `tail`'s.** A tool that answered only by status would have had its disproof reported as a proof.
  **Operational form, and aeon's point is the right one: make it mechanical, not remembered.** Never read
  `$?` through a pipe. Redirect to a file and check the status of the command itself, which is what the
  later invocations in that same arc did.

- **⚠ OPS, two, neither a defect in the work.** (a) **A fresh worktree has no `vendor/` symlink**, and
  without it 8 `save_state` rows FAIL while the whole 68000 SingleStepTests sweep SKIPS AND PASSES: the
  repo's own `vendor_data_present_when_running_in_ci` says so. The agent's first full run reported 2073 and
  was partly vacuous; it symlinked and re-ran. **Put the symlink step in any brief whose parcel touches a
  suite total.** (b) **Do not `git commit` while `cargo test --workspace` is running:**
  `the_compiled_in_build_id_still_names_this_tree` compares the compiled-in id against HEAD and fails when
  HEAD moves under the run. Correctly caught, self-inflicted, worth knowing.
  ⚑ **And a third, this seat's own, because it produced a false alarm worth more than the mistake:** a run
  started with BOTH `nohup …&` and the harness's own backgrounding fires its completion notification for the
  **launcher shell**, not for cargo. Reading the log at that notification gave **50 legs / 1543 passed / 0
  failed** against a 70-leg baseline, an apparently clean green **800 tests short**, which is bar 25's
  artifact exactly. The missing legs were the slow ones and the log had no final summary. **Use one
  backgrounding mechanism, and confirm a suite's completion from the log's own end, never from a
  notification.**

- **⚑ A WORKTREE AGENT READS A STALE QUEUE BY CONSTRUCTION, AND IT PRODUCED A CONFIDENT WRONG FINDING.**
  The recon reported that two of the three rows it was asked to re-price *"exist as names in a narrative
  sentence in `lane-log.jsonl` and have no id in `lane-status.json`"*. **False, and the mechanism is
  structural rather than careless:** `docs/lane-status.json` is deliberately **uncommitted** (the contract
  keeps it out of git), so an agent's worktree serves whatever was last committed: here a queue three rows
  short and carrying five rows since removed. The agent read the only copy its tree had.
  **Operational form, and it binds this seat rather than the agent: when a brief names queue rows, QUOTE
  THEM INTO THE BRIEF.** Pointing an agent at a row id is pointing it at a file that is stale in its tree by
  design. This is the live-tree hazard inverted: the usual failure is reading someone's *uncommitted* tree
  as though committed; this is reading a *committed* copy of a file whose truth only ever lives uncommitted.

- **⚑ NO SINGLE `cargo test` INVOCATION RUNS EVERY TEST IN THIS REPO, AND THE TWO PROFILES' TOTALS
  CAN AGREE FOR CANCELLING REASONS.** Release drops three `#[cfg(debug_assertions)]` rows in
  `crates/oracle-core/src/testrom.rs` (they assert `debug_assert!` guards fire, so they cannot compile
  in release) and gains the three replay playthroughs, which carry `#[cfg_attr(debug_assertions,
  ignore)]` and run only in release. **Measured on this landing: the agent's debug run and this seat's
  release run both reported `PASSED=2345`, differing by `IGNORED` 6 vs 3; the pass totals matched
  because each profile gained three the other lost.** A total quoted without its profile is not
  comparable to another session's, and two such totals agreeing is not corroboration. **Quote the
  profile with the number**, and read `IGNORED` as well as `PASSED` when reconciling two runs.

**▶ NEW BAR, 2026-08-30 — EVERY CITATION RULE THIS SUITE OWNS IS WRITTEN FOR THE RECEIVING SIDE, AND
BOTH OF TONIGHT'S FAILURES WERE ON THE EMITTING SIDE, WHERE NO RULE REACHES.** aeon's formulation, banked
by them at aeon `4fae2d8d`; two instances, one from each lane, hours apart.
*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 233-289. The bar's opening line had been
lost from `OVERSEER.md` by an earlier pass and survived only as a stranded fragment in the log; restored
here 2026-09-06 from `OVERSEER-LOG.md`'s copy.)*


**▶ F-CR28-CALLERS-DANGLING, registered 2026-08-30: an unmerged commit in a leftover worktree, found
while earning an `atBoundary: true` claim rather than asserting one.**
*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 294-323. An earlier pass inserted this
pointer between the two halves of that sentence; rejoined 2026-09-06.)*


**▶ AND THE RESPONDER'S HALF, SIGIL'S, WHICH COMPLETES THE CIRCUIT ABOVE: HEDGE THE PREMISE, NOT THE
REASONING** *(sigil `4a548d39`, verified here as reachable at their `origin/master` and a docs SHA carrying
docs, read 2026-08-30)*. Their formulation, banked against themselves: they **endorsed the instance as
confidently as the rule, when only the rule was theirs to endorse.** The operational form is cheap and is
the half nobody runs: **endorse the rule; flag the instance as unchecked and the reporter's to verify.**
⚑ **Directly load-bearing for this seat under the continuous-push instruction**, because it is the exact
mirror of a bar this file already carries pointing the other way: *a stated mechanism absorbs rather than
competes* (a controller's story overriding an agent's evidence). Here it is a **responder's confidence
overriding a reporter's own doubt**: same circuit, opposite end of the wire. This lane held only the half
that flattered it, and so did they.
**Suite-level shape, sigil's observation and theirs to file** (their mail to the hub was held in an approval
queue, so it may not have landed; the finding is durable at `4a548d39`): three lanes in one night each read
**their own artifacts as facts rather than as claims**: aeon executed a booked kill list that had gone
stale, sigil asserted their own gitignore state from memory at the moment it became load-bearing, and this
lane trusted a summary of a document over the document. **Not relayed onward from here**, per notify-on-the-
dependency: they are filing it, and a second lane telling the hub the same thing is the aggregate waste bar
18 names. Recorded so the pointer survives if their mail did not: it lands on the hub's own live
`PLAN-PROSE-SWEEP` item.

**▶ A CONDITIONAL LINE IS A DECISION WITH AN EXPIRY DATE, AND THE BOOT DOC IS THE WORST PLACE FOR ONE**
*(2026-09-05, this seat, measured: it cost a parcel).* A claim of the form *"X is off until Y"* reads as a
standing decision forever after Y happens, because nothing about it changes when Y does. A brief written
from `OVERSEER.md`'s line-item told this lane that the player's layout persistence was off and that turning
it on was a live choice; it had shipped, and the only real question left was whether anything asserted the
property. **Operational form: when a conditional line's condition is met, strike the line in the same commit
that meets it.** *(The paragraph and both of its corrections: `OVERSEER-LOG.md`, 2026-09-06.)*

## Moved from OVERSEER.md 2026-09-09 (boot-read cut) — ops lessons read at a moment, not at boot

### A row's justification ages while the row sits still; and presence of structure read as absent emission (from CR-Q, 2026-09-09)

⛑ **THE SIXTH INSTANCE IN TWO DAYS OF A ROW'S JUSTIFICATION AGEING WHILE THE ROW SAT STILL, and the first
one caught BEFORE the agent rather than after.** The standing rule that caught it is the 09-09 frontier's:
**before dispatching any row, check whether the WORK landed, not whether the ROW is open.** Both artifacts
asserting this debt — the `▶ WHAT WE OWED` paragraph (now in `OVERSEER-LOG.md`, under *CR-Q / §11.40
`machineReplaced`: the adjudication provenance, and the three-item debt discharged*) and the
`CR-Q-MACHINEREPLACED` board row — were self-consistent, correctly
cited, and wrong, which is why neither could correct the other.

⛑ **AND THE HUB READ IT THE SAME WAY FROM THE OTHER SIDE, which makes this a shared-frame instance rather
than a lane defect.** Backing the parcel as contract owner, it reported *"your crates reference it in 8 files
… so the contract is written, the structure is there, and the signal is not delivered"* — it had grepped the
references and read **presence of structure as evidence of absent emission**. That is bar 16 running in the
NEGATIVE direction: name-is-not-behaviour normally over-reads a name as work, and here it under-read eight
real emission sites as scaffolding. The discriminator was one grep separating **test** references from
**production call sites** (`states.rs:237`, `main.rs:1926`), which is the command that converts a reference
count into behaviour.

### Handing a check you cannot run to the peer who can: bar 24's second instrument, aimed at a peer (CR-Q §5, 2026-09-05)

**And a correction to OUR §5 worth keeping: events ARE schematized in this repo**, so the schema cost is
one fragment, which the hub added. We priced it as more.
**The pre-adoption check we flagged and could not run, the hub ran:** `clients/python` validates no closed
set (boolean negotiation), the MCP shim negotiates `want_events=False`, and the handshake fragment is free
strings. **So the check came back clear, but it was right to hand it over rather than assert it**, which
is bar 24's second-instrument rule working in the direction of a peer rather than a document.

### A brief warned about the mirror image and the thing it warned about could not happen (S3, 2026-09-05)

*The defect this is about — every palette gesture that replaced the machine running NO repair — is in
`OVERSEER-LOG.md` under* **The S3 one-door defect**.

  ⚑ **MY BRIEF WARNED ABOUT THE MIRROR IMAGE AND THE THING IT WARNED ABOUT COULD NOT HAPPEN.** I wrote that
  *"two copies of this repair is the defect this slice is most likely to ship"*. There was nowhere for a
  second copy to live: the palette is **derived from `METHODS`**, so `reload_rom` was always reachable
  through the one registry and an F5 binding is a keyboard alias for a call that already existed. **The
  real defect was a door that ran NO repair, not two doors running it differently.** Fixed by recording in
  `Bus::call` itself rather than a per-call-site list: a per-site list is a list of methods that replace
  the machine, and the palette is registry-derived precisely so no such list exists to go stale.
  **Proved by this seat restoring the defect**: deleting `self.own.absorb(&report)` turns all four
  `bus::one_door::*` rows red.

### Third consecutive agent to correct its brief: lead a dispatch by asking for disagreement first (S2a, 2026-09-05)

  ⚑ **THIRD CONSECUTIVE AGENT ON THIS ARC TO CORRECT ITS BRIEF ON A MATERIAL POINT**: the recon on the
  module list, S0-S2 on the fit inverse, this one on the refusal. **That is the delegation corollary paying
  out: a brief's frame is the thing an agent is best placed to break, and all three were caught because the
  brief asked for disagreement first rather than last.** Keep leading dispatches with that request.

### A granting act DESCRIBED, not a status field quoted (push authorization, 2026-08-24)

**The granting act is named, which is why this relay is usable at all.** The hub consolidated a
question two lanes had stopped on separately (sigil asked outright; aeon was sitting on three
finished docs commits for the same reason, neither able to see the other asking), put three options
to him (own-repo standing / standing-for-docs-ask-for-code / per-push), and he chose the widest.
That is a granting act described, not a status field quoted, which is the distinction the
never-record-an-unwitnessed-approval bar exists to draw.

### Freshness is not transitive across a document, and proximity reads as verification (2026-08-22)

*Durable formulation from the two-implementer-conflation thread under* **⚑ THE CUTOVER** *in
`OVERSEER.md`, worth more than its instance:* **freshness is not
transitive across a document, and proximity reads as verification**: a stale figure beside a
freshly-updated one is read as cross-checked, which is how my own 37 survived hours next to a correct
18.

### Untracked does not mean there is no committed revision to ask (F-FROZEN-FIXTURE-DRIFTS, 2026-09-06)

**The correction worth carrying**, because the brief asserted the opposite and it is a reusable move: the
`.lst` files being untracked does **not** mean there is no committed revision to ask. The *source* the
dimensions come from is tracked (`ObjSub_Spring__Up_Red` is a `pub equ` in
`games/sonic4/objects/test_solid.emp`), so a currency check reads aeon's **object store at a ref** and the
live-tree hazard is avoidable for the primary measurement rather than inherent. Detail and the falsifiers:
`docs/2026-09-06-fixture-dimension-drift.md`.

## Moved from OVERSEER.md 2026-09-09 evening (second boot-read cut) — read at a moment, not at boot

*Three blocks whose headings read as closed history but whose BODIES are majority live rule, so they
belong here and not in the log. Read the ledger block* **before landing** *(before any edit to
`docs/decisions.jsonl`); the GUI-LAYERS hazard* **before designing or reviewing a surface that names
a tile slot**; *the plane-raster lesson* **before re-sending an ask parked on the owner**. *Proved
lossless by `tools/prove_doc_split.py`: exit 0, PROVED, 974/974 non-blank lines accounted, PROOF 3
seams introduced heading-aware 0 AND heading-blind 0.*

## ⚑ THE LEDGER, 2026-09-10: SEVEN LINES REWRITTEN IN PLACE — **DO NOT REPAIR** — AND ONE THAT HAD TO BE

**(a) Closed out of shape, do not repair: `d-31, d-35, d-38, d-39, d-40, d-44, d-47`.** The 2026-09-09
audit session answered six and re-shaped one **by rewriting the settled lines** (`31982af`, `docs/decisions.jsonl`
**+7/−7, zero appends** — measured here, not taken from the hub's numstat). `contract/DECISIONS.md` rule 8
forbids exactly that. **They stay as written — CITE RULE 8f**, `contract/DECISIONS.md` at empyrean `2e070bf`
(ancestor-verified, a contract commit carrying the rule). The pre-rewrite text is preserved in git at `627295f`.
⚠ **This seat first cited the pre-8d "closed out of shape, do not repair" bullet and that was ONE NOTCH WIDE**:
8d's bullet says *"before this rule"* and 8f now says so explicitly — 8d took force 2026-08-30T01:58:05Z and
these were rewritten 09-09, so the bullet does not reach them. **Same disposition, different route**; the
sentence that actually forbids the rewrite is *"Nothing in 8d is an instruction to touch an existing line."*
⚑ **The hub proposed aurora's append-a-sibling repair and withdrew it on this lane's objection**: appending
siblings would put a second answered row under a second id for each question, double-counting on the console —
a repair creating the defect it repairs. **8f's discriminator, now written down because the hub told two lanes
opposite things in one day and both were right: REPAIR ONLY WHEN A GATE IS RED AND THE REPAIR CLEARS IT;
otherwise list and leave.** Aurora's rewrite failed `check-ledger-timestamps`, so aurora repaired; ours failed
nothing, so ours is listed. **A repair with no red gate behind it is history edited to look compliant.**
⚑ **And the uncomfortable half, kept because it is the sharpest instance either lane produced: the correction
this seat served the hub at 02:44Z was sourced from those seven rewritten lines.** The conclusion was right and
was independently confirmed, but **both lanes were reading the ledger as ground truth inside an argument about
reading the cheaper artifact.**

**(b) THE ONE REWRITE THIS SEAT MADE, DELIBERATELY AND LOUDLY: `d-46` appeared TWICE** (lines 46 and 49), the
second filed by this lane at 2026-09-09T15:20:19Z as a corrected re-file that reused the id instead of taking
the next free one. **`tools/lane-check.py` was RED on it from 15:20Z, hours before the audit** — so the hub's
warning was right about the class and wrong about the instance, and the red was ours. **G2b runs on every
landing, fast path or full**, so the landing lane was blocked for both live parcels and would have refused them
with a message about malformed lane files, reading as a fault in their branches.
**Repair: line 49's `id` → `d-49` plus `supersedes: "d-46"`, one line, `--numstat` verified 1/1, the other 48
lines proven byte-identical, gate exit 0.**
⚑ **NOW CONTRACT RULE 8e, adopted from this lane's finding** (empyrean `2e070bf`): **a duplicate id cannot be
repaired by any legal append.** Rule 8's sanctioned shape-fix (sigil's d-15 over d-14) adds a new id and leaves
the malformed line — which does not remove a *duplicate*, so a uniqueness gate stays red forever and the file's
own gate demands the one move its contract forbids. **Ruled: id-uniqueness wins, narrowly. Re-id the LATER line
with `supersedes` naming what it left, exactly one line, `--numstat` reading `1 1` with the rest byte-identical,
loud in the commit.** It is the ONLY sanctioned edit to an existing line in that document. The hub's rationale,
worth keeping because it is better than the one this seat argued from: **a duplicate makes the reader resolve
last-line-wins and SILENTLY SHADOW the earlier decision, so the owner never sees it** — the exact failure
append-only exists to prevent, arriving through the rule that prevents it.
⚑ **The move was made and flagged BEFORE the ruling existed, and the ruling ratified it.** That is the right
order and worth keeping as the pattern: act on the critical path, be loud about which rule you are bending and
why, and let the contract catch up — never bend it quietly and never block a landing lane on a bookkeeping
defect while waiting for permission.

## ▶ GUI-LAYERS: **DISCHARGED 2026-09-09.** Every premise it was queued on is now false

*The discharge measurement — all three queued claims put to the tree and found false — is in
`OVERSEER-LOG.md` under* **GUI-LAYERS: the discharge measurement, all three premises false**. *The
queued section and aurora's five consumer rules moved there the same day.*

⚑ **ONE HAZARD IS KEPT, because it binds anything BUILT LATER rather than describing what shipped: if a
surface ever names a blob-local tile slot, the rebase can land OUTSIDE the blob.** `tile` is VRAM-absolute;
aurora's `BG_TILE_BASE_SLOT` is 1024, so **any `tile < 1024` rebases NEGATIVE**, and capacity does not
rescue it — their formulation, **in-capacity is not in-blob**. Plane B can legitimately show engine art or
another act's. So such a surface answers *"that is not part of your background"* and above all does not
guess: an unchecked rebase either throws or confidently names a slot the author does not own, which is
indistinguishable from a correct answer.


## ⚑ MEASURED 2026-09-08: THE PLANE RASTER IS NOT THE LAG, AND THE ASK ON HIM IS RETIRED

`F-PLANES-RASTER-EVERY-FRAME` sat on the owner for a tab test it never needed: `eframe` writes the dock
layout to `~/.local/share/oracle-player/app.ron`, so which tab each pane executes is readable off disk at
any moment. The row STANDS as a real defect and is NOT the current explanation of his cost. Measurement,
the four-pane table and the baseline caveat moved whole to `OVERSEER-LOG.md` 2026-09-09.

⚑ **The live rule: an ask parked on the owner should be RE-PRICED before it is re-sent.** Before putting a
*look at this* question to him, ask what the program already writes down.
## OPS 2026-09-10: a commit message composed through a command substitution is SHELL INPUT

**Never put backquotes in a commit message written as `git commit -m "$(cat <<EOF ...)"` in this harness.** Measured tonight: a backquoted word inside that construct was executed as a command and DELETED from the message, leaving `8d says in terms that  supplies the CONTENT` in a pushed commit, with `command not found` printed into a stream nobody was reading. The heredoc delimiter was quoted and it happened anyway.
**This is the commit-message bar's own failure mode arriving through the shell rather than through a failed edit** (second instance here; the first was an edit that matched nothing while the chained commit ran regardless). A commit message is a claim about a diff and NOTHING checks it, so it is the one artifact where a silent deletion survives.
**Remedy: compose the message from a file or a Python string, and READ THE MESSAGE BACK with `git log -1 --format=%B` whenever it contains punctuation the shell owns.** Already pushed means it stays: never rewrite pushed history.

## ⚑ FROM THE C5/H26 PAIRING, 2026-09-10 — read before DISPATCHING, REVIEWING and LANDING

*(Both landed in order: H26 `f759d76`, C5 `282aa93`. The discharge and the numbers are in `OVERSEER-LOG.md` under **Moved from OVERSEER.md 2026-09-11 — closed blocks, verbatim**; these four are the transferable half.)*

⚑ **THE LESSON THE PAIRING ACTUALLY TAUGHT, and it is not the one the rule below predicted. The canonical
site was SAFE; its PARAPHRASES were not.** The C5 brief ordered the agent to repair `render_scanline`'s doc
because a second stateful render would falsify it — and that paragraph never went false, because H26 had
deliberately written it to anticipate exactly this. What went false were **seven other sites** the brief did
not name (`oracle-player/machine.rs:318`, `oracle-frontend/main.rs:709`, `oracle-aether/engine.rs:1595`,
`:1622`, three in `vdp.rs`), each paraphrasing the claim as *"the one render that commits the latches"*.
**A claim repaired at its canonical site leaves every paraphrase of it standing, and the paraphrases are
where the next parcel falsifies it.** H26 fixed eleven sites of one spelling; C5 found seven of a *different*
spelling that H26's own greps had no reason to match. When a doc is hardened, sweep for what RESTATES it, not
only for what repeats its words.
⚑ **CORROBORATED CROSS-LANE WITHIN THE HOUR, and sigil's framing generalises past docs — take theirs as the
statement of the class and this as its doc-side instance.** Sigil booked `LINKER-STILL-PRINTS-A-PASS-COUNT`:
the owner ruled an attempt count out of the assembler's message, and the **linker's copy of the same wording**
(`crates/sigil-link/src/relax.rs:1116`) stood untouched. Their sentence, which is the durable one:
**a ruling has CONSUMING SURFACES, nothing in the tree marks a site as one, and so a partial enumeration looks
exactly like a finished one.** That is why eleven-then-seven happened here and why it is not a docs problem:
the same shape reaches code, messages and rulings. Practical consequence for any fix that repairs a *claim*
rather than a behaviour: **the deliverable is the enumeration of consuming surfaces, and it must be produced
by varying the SPELLING and the AXIS, never by grepping the words the canonical site happens to use.**
*(Cross-lane generalisation is the hub's and marked as theirs; the eleven/seven numbers are this lane's.)*
⚑ **AND THE COUNTER-INSTANCE TO "PACKET COUNTS ARE FLOORS": here the finding was EXACT.** The brief pushed
the agent to reach past the ledger's one consumer on the standing floor prior. It reached and found nothing —
`oracle-player`, `oracle-frontend` and `oracle-aether` all put `ScanlineCapture` in the `Fanout` on **both
branches of every run path**, so they are armed by construction and `oracle-replay` really was the only
unarmed consumer. **A floor is a PRIOR, not a law.** Keep sending agents past the stated count; stop treating
a count that holds as a failure to look.
⚑ **OPS, AGAINST THIS SEAT: THE BASELINE I QUOTED COULD NOT COME FROM THE COMMAND I QUOTED.** The brief gave
`88 legs / 2759 / 0 / 3` as the baseline *and* `cargo test --workspace` as the verification. `oracle-replay`'s
three playthroughs are `cfg_attr(debug_assertions, ignore)`, so **debug reports 2757/6 and release reports
2760/3** — a baseline from one profile handed over with a command from the other, and the agent had to
reconcile it (2757 + 3 = 2760) before it could report anything. This is the already-banked *"no single
`cargo test` runs every test here"* arriving as a defect in my own brief. **State the PROFILE with every
baseline count.** And at the landing, the arithmetic is not the proof: `comm` the debug and release
**ignored lists** and confirm the extra legs ran BY NAME — the three that matter here are exactly the ones
covering the code C5 changed.
⚑ **THE GUARD LESSON, and it is the parity-pair bar arriving on its own: A SWEEP COMPARING TWO PATHS STAYS
GREEN WHEN BOTH ARE WRONG THE SAME WAY.** The agent's width mutation (`let width = 256;` in the cheap path)
came back **applied-and-still-green** over a corpus whose own coverage assertion `saw_h32 && saw_h40` was
true the whole time — because `width` reaches committed state only through `collision`, only for pixels in
`x ∈ [256,320)`, and no fixture put an *overlap* in that band. **A coverage assertion that names the axis is
not a coverage assertion about the MECHANISM.** The repair is the reusable half: a control asserted **before**
the sweep (this pair collides at 320 and is clipped at 256), with the band coordinate **derived** from where
width can reach state at all rather than picked. The agent treated the green as a guard defect rather than a
pass, which is the behaviour bar 8(c) exists to produce.

## Moved from OVERSEER.md 2026-09-11 (boot-read cut, 99,657 → 89,880 B) — read at a moment, not at boot

*Two blocks whose subject is discharged but which carry rules with no other copy in this file (grepped:
"self-removing" and `fixedAt` had zero hits here), so they belong here and not in the log. Read the
first* **before choosing a row to dispatch** *and* **before writing into `OVERSEER.md` at a landing**;
*the second* **before writing any brief that asks for a ledger row to be marked fixed.** *Proved lossless by `tools/prove_doc_split.py`: exit 0, PROVED, 981/981 non-blank lines accounted, PROOF 3 seams introduced heading-aware 0 AND heading-blind 0 (17 derived cut points, all OK); control run green on the untouched tree first, since declaring this whole file as an `--output` is vacuously red here.*

*Referent, because the cut commit left the moved text as written: in the NEXT block, "the `emulator/step` row in the follow-up register" moved to `OVERSEER-LOG.md` in the same cut, under **Moved from OVERSEER.md 2026-09-11 — closed blocks, verbatim**.*

### From queue item 8 (THE ACCEPTANCE CONTRACT): the discharged NEXT block (orig lines 83-104)

   **NEXT (not yet dispatched):** ⚑ **DISCHARGED, AND THIS BLOCK OUTLIVED IT BY NINETEEN DAYS.** It called
   the `run_to_scanline` parcel *in-flight* and said *"remove from here once that lands"*; it landed
   **2026-08-22** (`4f93583`), and both folded residuals are resolved in source (checked, not assumed —
   a fold is a reference and closing its target orphans it). ⚑ **A self-removing instruction is nobody's
   job by construction**: no owner, no trigger anyone watches, so it reads as live prose forever. Book the
   removal or write a condition a boot read can test. Still open
   from the survey is a proposed
   **error-surface gate**: since no fragment declares error conditions, a suite validating only
   replies is blind to every error obligation. ⚠ **NO LONGER A PROPOSAL — `crates/oracle-aether/tests/request_bounds.rs`
   LANDED 2026-09-09 (`84e14f6`, `5f6a670`, `c4eeae5`) and covers all 97 numeric bound obligations, fragment-derived at
   runtime. The non-numeric half landed on `parcel/error-surface-gate` the same night. This paragraph went on calling it
   a proposal for the whole day, and a dispatch was written from it — the EIGHTH row this week whose justification aged
   while the row sat still.** ⚑ **And it is the sharpest of the eight because the rule that would have caught it was
   already written and already being enforced — ON AGENTS. Every fix brief that night carried *"check whether the WORK
   landed, not whether the ROW is open"*, and this seat did not run it on its own row selection. A rule encoded as an
   instruction to others is not a rule you are following.** ~~The gate is still a proposal; the defect that
   demonstrated it is not.** The unenforced `count` bounds named in the 2026-09-04 §11.33 registration
   (the `emulator/step` row in the follow-up register) were fixed by the CR-STEP-SHORTFALL
   parcel (`step.rs`'s two refusal rows now assert them from the wire), so the standing argument for
   the gate must be carried on its own merits again: **one method's refusals being covered by hand
   is not the gate**, and nothing systematic yet reads a `params` fragment and asks the server to
   refuse what falls outside it.

### From the LAYER-MASK section: the `fixedAt` brief correction (orig lines 747-750)

⚑ **PROCESS CORRECTION, from the agent, against my own brief: `fixedAt` is a commit SHA and CANNOT exist inside
its own commit.** I have been asking agents to "mark the ledger row fixed in the same commit as the fix", which
is unachievable and contradicts every existing fixed row (M47/M48/M49 all append the row afterwards). **Stop
writing that instruction into briefs**; ask for the fix commit, then the ledger append naming it.

## Moved from OVERSEER.md 2026-09-13 (the when-read cut, 97,771 → 26,170 B) — live rules, each read at the moment its stub names

*Fourteen blocks moved whole out of `a44cb9e:docs/OVERSEER.md` under the owner's go (row OVERSEER-CUT): twelve whole sections, each below under its original heading, and two partial blocks, the tail of "The boot read is bounded" under that same heading and the follow-up register under a new heading naming where it came from. None of it is closed history: each block is a live rule or booking read at one later moment, and `OVERSEER.md` keeps each heading (for the register, its place in the queue section) plus one pointer line naming that moment. Orig line numbers are of `a44cb9e:docs/OVERSEER.md`. Proved lossless by `tools/prove_doc_split.py` with the fourteen slices declared as outputs beside `OVERSEER.md` (never this whole file, which is vacuously red here) and the fourteen pointer lines declared new: exit 0, PROVED, 956/956 non-blank lines, token delta 0, PROOF 3 seams introduced heading-aware 0 and heading-blind 12 (all twelve a stub heading followed by its pointer), 42 derived cut points all OK. The same invocation, with the slices and the declared-new file still empty, was green on the untouched tree first.*

*Referents, because the cut commit left the moved text as written. In the tail of **The boot read is bounded**, "this file", "here" and "at the foot of this file" mean `docs/OVERSEER.md`, whose foot carries **Where the detail lives**. In the follow-up register, "Measure before you add here" means `docs/OVERSEER.md`. "The four rulings above" (d-16 SUBSTITUTE), "see the flag above" (THE CUTOVER's heading) and "the relayed rulings above" (HERMETIC GATE) name the **FOUR OWNER RULINGS, 2026-08-22** section, and "hub ruling above" (HERMETIC GATE) names the 2026-09-10 hub ruling inside **CUT THE CEREMONY**; both stayed in `docs/OVERSEER.md`.*

## The boot read is bounded (100,000 B, gated)

**Nobody hand-trims for the bound. Do not take a headroom figure from this paragraph — MEASURE it**
(`wc -c docs/OVERSEER.md` against 100,000). A previous revision of this line asserted "so there is real
headroom" and was still asserting it at 99,578 B with 422 B left: a measurement written in the present
tense, in the one section whose job is to warn about exactly that. Cuts so far: 2026-09-06 (99,798 →
87,2xx B with its repair pass), 2026-09-09 morning (99,578 → 93,149 B, three closed blocks) and
2026-09-09 evening (98,275 B / 1,142 lines; 8 closed blocks to the
log, 7 ops lessons to the reference) and **2026-09-09 second evening cut (98,469 B / 1,147 lines →
90,463 B / 1,052 lines; 5 sections, 2 to the log and 3 to the reference)** and 2026-09-11 (99,657 B / 1,142 lines → 89,880 B / 1,043 lines; 7 blocks, 5 to the log and 2 to the reference). ⚑ **The BYTE bound is met
with real room; the LINE half is not, and that is reported rather than closed.** What is left over
~1,050 lines is live rulings and live follow-up-register bookings, and the protocol's own
measure-then-move rule 3 says the residual goes to the owner rather than into a trim.
⚑ **The claim "every block that could move has moved" stood here for one day and was false when
written** — the second cut found five more the same night, three of them classified by BODY against a
heading that reads as closed history. Do not write that sentence again; say what you moved and let the
next session measure.
When the bound is next reached, move history out in ONE cut with `tools/prove_doc_split.py` — run it from
THIS repo, script by absolute path, and check its provenance lines name this document's line count before
reading the verdict. Read BOTH PROOF 3 numbers: `--headings` gates the verdict on one of them, so confirm
the heading-blind seams land on headings rather than accepting the gated number alone.
⚑ **AND ITS PUBLISHED INVOCATION IS VACUOUSLY RED IN THIS REPO. Do not declare `OVERSEER-LOG.md` or
`OVERSEER-REFERENCE.md` as `--output`.** This lane has cut before, so both already hold thousands of lines
that were never in `OVERSEER.md`, and the tool correctly reports every one as `1b … NOT DECLARED` — an
exit 1 that reads as *the agent lost content* and means nothing. It scored 3700 on an UNTOUCHED tree.
**Declare the appended SLICES instead** (write each moved block to its own scratch file, pass those as
`--output`, then check the slice text is verbatim-contiguous in its destination), and **run your exact
invocation against the unmodified tree FIRST and require green there** before you trust any red after.
A control that is red before you start makes the verdict uninterpretable.
⚑ **And expect it to refuse a cut you were sure of.** The 09-09 evening cut also tried to move
`F-HOSTED-RESET-SRM`'s closed narrative; PROOF 3 disproved it, because that entry is a parenthetical
INSIDE the follow-up register's running comma-list — no blank line anywhere near the cut, so the torn
sentence was invisible to every paragraph- or sentence-boundary check a person would run by hand.
**The register's list-shaped entries are not movable at all**; do not try again without reshaping the
list first. `## Where the detail lives` at the foot of this file says which of the three files takes what.

## Follow-up register (from `OVERSEER.md`'s queue section, orig lines 112-440)

**Follow-up register** (each named where registered; deferrals here are unaudited estimates,
measured 3-for-3 cheaper than documented): F-SCANLINE-INDEX / F-SCANLINE-SH (priced down by the
sub-line arc), F-CRAMDOT, F-SUBLINE-{HGRID, ACCESSMCLK, DMASPREAD, CAPTURE-SCRATCH}, F-VCOUNT-PHASE,
the a2 B-2 gate gap (needs an H40/mode-switch fixture), ~~F-HOSTED-RESET-SRM~~ (**CLOSED 2026-09-06,
branch `fix/hosted-reset-srm`** — and the booking's mitigation never covered it: the defect is NOT
hosted-only, the window's own F1/palette reset reaches it through the same door. Two variants, both
now covered by rows that read the `.srm` itself: a reset landing before the first `Battery::tick`
STRANDS the save (`System::reset` clears `sram_dirty`, so nothing arms the debounce), and a window
reset whose next frame writes SRAM ROLLS THE FILE BACK (`after_replacement` read carried != live as
a cartridge swap). Fixed structurally: "did the buffer survive?" is decided in `Bus::call_stamped`,
the instant of the replacement, and carried to the drain — not re-derived from bytes a frame later.
The guard already named for this defect, `a_reset_does_not_rewind_the_live_battery_to_the_file`, was
green and blind: it read the machine, never the file, and its helper never ticked),
F-EQUATES-NAMESPACE,
F-CRAM-RAMP, F-PROF-TOTALS (superseded by delta 3), F-PALETTE-DRAG-PACE (evidence filed, rated
minor by its own filer), ~~stock-S1 symbols~~ (**CLOSED 2026-08-20**: the `|`-reader, the 48-bit
addresses, the forward-only equ ruling and the no-appendix binding all shipped; F-LST-AS-COLUMNS and
F-LST-NONDEB2-BINDING retire with it), **F-TICK-BOUNDARY-DIVERGENCE** (2026-08-20, from aeon's spike hunt, TICK-VARIANCE.md): over one
31-frame max-diagonal window on byte-identical ROM bytes, oracle-old runs 26 logic ticks where we
run 29; exact agreement at the corpus-era state, one-tick difference at idle, divergence only
where a tick sits near the frame boundary: the two emulators disagree how much work fits in a
frame. Settling experiment (theirs): a single-tick trace at the first divergent boundary (frames
~7-8; states in TICK-VARIANCE §1.2) on both instruments. Corroborating fossil: the 2026-07-23
RT-3 finding, where oracle-old OVER-drops ~8 startup ticks via `ClampHandshakeTimeDeterministic`'s
over-conservative bus-arb clamp, and ours was the tick-accurate side then too. Unresolved, not
urgent, CR-28-era sweep candidate. plus the Tier-1 carry-forwards in
`docs/2026-08-18-tier1-bus-methods.md`.

**Registered 2026-09-11, cart mapper (`parcel/cart-bank-mapper`). Everything — window table, evidence,
decisions, the two new F- rows, the owner's foreground check — is in
`docs/2026-09-11-cart-mapper-design.md`; TRANSCRIBE it, do not re-derive it.** Live: `$A130F3-$A130FF`
point seven 512 KiB windows, window 0 fixed, identity at reset → currency byte-identical by construction,
snapshot-only, **no `export_state` bump**. `$A130F1` stays the SRAM latch. ~~F-DEBUGREAD-BANKED~~ closed 2026-09-11 (§11.48,
`docs/2026-09-11-debugread-banked.md`). Open: **F-BANKED-ADDR-AMBIGUITY**. ⚑ **Method lesson: an assertion against the constant under test is CIRCULAR,
and reads as strong until something mutates the constant** — the reset test compared `cart_banks()` to
`CartBanks::IDENTITY` and stayed GREEN under `IDENTITY := [0; 8]`. (⚑ This block was drafted at 1,995 B
against 1,531 B of headroom and broke `overseer_bound` at 100,464 B. Measure before you add here.)
**Registered 2026-09-12 (a commitment to the HUB, and it is an obligation ON US that nothing in the tree would
otherwise carry).** Asked whether a `contract/` landing would move a gate here, this lane enumerated what oracle
actually consumes: **exactly two paths, as literals, never a tree walk** — `contract/schema/bus-protocol.schema.json`
and `contract/schema/tests/vectors.json` (literals at `tools/contract_drift_report.py:117,122`; the first pinned by
git-blob hash out of `crates/oracle-aether/tests/contract/PROVENANCE.md` by `schema_conformance.rs`). On the strength
of that the hub **narrowed its contract-landing notices to those two paths** (banked theirs at empyrean
`origin/main` `3a6b17b`). ⚑ **So the day this repo vendors a THIRD fragment, we owe the hub a message, or we
silently stop being told when it moves** — the notice would not fail, it would simply never arrive, which is the
absence class with a peer's practice behind it. The reader who needs this is whoever edits the vendored set, so it is
also noted in `PROVENANCE.md` beside the adoption steps. *(Their side of the exchange corrected itself too: their
notice said the commit "touches exactly one file" and `--stat` showed two. They banked a mechanism rather than more
care — run `git show --stat` in the same tool call that composes any notice naming what moved — which is protocol
bar 23 arriving in mail instead of a commit message.)*

**Registered 2026-09-13, from aeon (inbound instrument ask; verified firsthand at `e272ac7`): F-Z80-ACCESSES-UNWATCHED.**
A bus watch never sees the Z80 read cartridge ROM through its `$8000` window: `Z80Bus::read_window`
(`crates/oracle-core/src/z80/bus.rs`) returns the byte with no sink call. **Wider than the ask:** the only `on_event_at`
in that file is the FM/PSG write tap, so every other Z80 access (its own RAM, window writes into 68k work RAM, the VDP
mirror) is undelivered too, while `watchpoints.rs`'s module header says *"the real 68000/Z80 bus adapters deliver every
access"* and `Watchpoints::caveats` says nothing. Aeon measured it: read watches on one sound effect's FM patch bytes,
effect fired, **0 hits / 0 dropped** while the driver's channel state pointed at them. ⚑ **So a zero on a Z80-read subject
is the instrument being absent reported as a negative finding**, and the `seen = 0` caveat cannot fire because the
68000's traffic makes `seen` non-zero. Two halves: (1) cheap, a caveat plus the header made true (**LANDED 2026-09-13, merge `453aa96`**; VDP/I-O
through the Z80 window stays unmodelled, so caveat 1 does not cover it; aeon told); (2) emit Z80 accesses as
`BusEvent`s (fc 0), which moves the stream every sink sees (VGM logger, profiler, trace), so enumerate consumers first.
*Hypothesis, unmeasured:* a timestamped Z80 access stream is a candidate for the Z80-timing currency M24 needs; shape the
two together. Aeon is not blocked (they answered their own question from the driver's channel state).

**Registered 2026-09-13, from the F-MACHINEREPLACED-EVENT-RACE parcel (brief `725fb04`):**
- **F-EVENTS-BEGIN-UNSTATED, a contract gap for the HUB.** The server subscribes a connection when it processes
  `initialized`, a notification nobody answers, so a client that *has sent* it can still miss an event (a window state
  load in the gap: no event, `droppedEvents` 0). `protocol.md` §2.1/§3 say when the server may push, never when a
  client may rely on receiving; empyrean's Python and TypeScript clients both return straight after `initialized`.
  Today's remedy is any round trip after `initialized` (our test harness now does exactly that). A server-side
  queue-at-`initialize` alternative moves the wire: a CR, not a slice. Live-window loss is reasoned from source, not observed.
- ~~F-PLAYER-SCREENTEXT-FIRST-READ~~ **CLOSED 2026-09-13 (merge `70a83cd`, brief `f3951f3`).** Iteration 1's drain runs
  before the first `set_screen_text`, and one `Host::pump` answers every request it finds queued; reproduced 15/15 red,
  byte-identical to CI's refusal; fixed test-side (the client waits for `status.display: true`). It leaves
  **F-FIRST-PRESENT-REFUSAL, a contract question for the HUB**: a client answered by that first drain is told `noDisplay`
  (*"this server has no window"*) and `status.display: false` by a window that exists, and `emulator/pacing` gives
  `noPacing`, in both embedders (`oracle-frontend` by reading). §11.29 is silent on a presenting host that has not yet
  presented. Options: a distinct reason (wire, a CR), holding answers until the first present (moves iteration 1's
  repairs), or documenting the `display` poll. Not built; the agent's report is in the merge message.

**Registered 2026-09-11 (a commitment to sigil):** sigil's `.lst` gains a top `DIGEST-` section, relying on `SymbolTable::parse`'s `Section::Body` arm treating pre-header non-matches as non-damage. **Kept, and pinned by a named test** (LENS-WAVE1-B). Told sigil: no preamble line may begin `Symbol Table`/`Equate Table`/`Phase Table`.

**Registered 2026-09-04, from taking the foreground runtime backlog the moment an instrument existed:**

- **▶ CLOSED 2026-09-04: the four foreground runtime follow-ups (three stale, one never ours) and `step`'s frame-budget shortfall, closed by a CR that went the whole way to §11.33. Moved whole to `OVERSEER-LOG.md` 2026-09-05 for the boot-read bound. The two live rules they produced are kept: a register entry naming a contract gap is worth re-reading against the CURRENT CONTRACT, not only the current code; and a perishable claim decays where nobody re-reads it, the distinguishing variable being whether the claim is ABOUT THE CODE IT SITS BESIDE.**

**▶ CR-Q / §11.40 (`emulator/machineReplaced`): ADOPTED WITH CHANGES, SERVED, AND ALL THREE OWED ITEMS
DISCHARGED** — the adjudication provenance, the reviewer, and the 2026-09-09 discharge measurement are in
`OVERSEER-LOG.md`. The adopted changes stay here because M2 is a live hazard on a live surface:

**The four changes to our proposal, transcribed rather than summarised:**
- **M2: `capabilities.events` advertises the member ONLY on a process that can produce the gesture; a
  headless `oracle-aether` MUST NOT advertise it.** ⚑ **This makes the events list PROCESS-DEPENDENT, which
  is the [[F-BANNER-INVITES-A-PIN]] hazard on a new surface**: a consumer that pins the events array will
  now break by *which binary it is talking to* rather than by version. Say so when we serve it.
- **M3: Half A (internal accounting) is REQUIRED alongside Half B, not optional.** Our proposal offered it
  as separable; the hub closed that door.
- **M4: one boundary, one signal**, with V7-V11 in our suite.
- **S1 as proposed** (the single-member `reason` enum, so `reset`/`restore` later are an added member
  rather than a renamed event).

**▶ RULED 2026-09-05 by the contract owner (hub, under delegation; theirs, not his): DO NOT widen the
aether build script's dirty scope to cover `crates/oracle-player/src`.**

The reasoning, transcribed because it is the reusable part: `protocol.md` §2.1 defines `serverBuild.id` as
differing between builds whose **observable behaviour on this bus** can differ, and `dirty` as uncommitted
source under that same claim. **The window's UI code changes what a person sees, not what the bus serves**,
so pulling `oracle-player/src` into that scope would make the bus's flag answer a question it was not
asked, and would widen a deliberately declarable `rerun-if-changed` set for a non-bus reason.
**The shipped design stands:** the chip marks dirty positively only when the scope covers what the reader
looks at, stays silent otherwise, and retires its caveat when the scope moves.
⚑ **If the window ever wants to say "clean" about itself, that is a WINDOW-LOCAL marker computed over
`oracle-player/src` by the window's own build step, a separate claim with a separate name, never the bus's
flag.** Booked as `F-WINDOW-LOCAL-DIRTY` and **rated low on its merits**: the observable that actually
settled the stale-binary incident was the executable's **mtime**, which already ships. A dirty marker helps
somebody *editing* the window, not the owner *running* it, and he is not editing it. Revival: someone is
regularly running a hand-built window and being misled by it.

**Registered 2026-09-05, from landing S3:** *(the one-door palette defect that opened this group is
closed; its narrative is in `OVERSEER-LOG.md` and the brief lesson it produced in
`docs/OVERSEER-REFERENCE.md`.)*

- **▶ CR OWED UPSTREAM: F-STATELOAD-SILENT-REPLACE.** A **window** save-state load replaces the machine
  **without moving `rom_generation`**, so a connected client is never told the machine underneath it
  changed. `oracle-frontend`'s F4 has the identical hole. **Correctly NOT built** (a new signal on a
  contract surface is a CR, not a slice) and recorded at `states.rs`. Raise it with the hub.

- **▶ RATIFIED, AND NAMED TO THE HUB RATHER THAN DECIDED QUIETLY: after a client's `reload_rom` this window
  now applies the incoming cartridge's `.srm`.** The reply is unchanged; **the machine state a client reads
  afterwards is not.** Ratified because it is the window's own file and the window's own repair, in the same
  function as the four repairs `drain` already performed, and it makes the toolkit window agree with
  `oracle-frontend`, which has done this after F5 for as long as it has existed. **It is a property of the
  deployment, not of the protocol: the standalone server is untouched** (verified: the diff over
  `tests/contract/` and `engine.rs` is empty). Flagged upstream so the hub can overrule.

- **▶ F-ADOPT-RESYNC-UNGATED, the agent's own honest residual.** Deleting `Machine::adopt_system`'s resync
  leaves the **whole suite green**: the fixture drives `System::run_frames`, which never feeds `Machine::cap`,
  so `capture_lines()` was already 0, and making it non-zero needs a mid-frame halt; both breakpoints tried
  halt before the first scanline completes. The audio half is *indefinite silence* and wants a real cpal
  device. **What stands in a gate's place is structural (one method, two statements) and the agent wrote it
  down as weaker rather than claiming coverage.** That is the behaviour bar 8's clause exists to get.

**Registered 2026-09-05, from the socket-identity notice paying off:**

- **✔ CLOSED 2026-09-05: the socket-identity notice found a real name match in aeon before the rename (their `pgrep -x oracle-frontend`), fixed at aeon `044573da` to key on the socket path. Moved whole to `OVERSEER-LOG.md` for the boot-read bound. Two live rules kept: a `pgrep` on a name that does not exist returns empty and reads as *not running*: aeon reported the owner's window closed all night while it was open; and a HEDGED cross-lane claim was more useful than a confident one would have been, because it let the receiver fix what breaks under either outcome.**


**Registered 2026-09-05, from landing S2a:**

- **✔ F-PARITY-BLIND-TO-SAT-STRIDE: CLOSED 2026-09-05, and verified by this seat re-running its OWN mutation** — `SAT_ENTRY_BYTES` 8→16 now fails naming the quantity, and `index * SAT_ENTRY_BYTES` → `0 * …` fails naming a non-zero index. The fixture cannot move with the constant under test. Closure narrative moved whole to `OVERSEER-LOG.md` 2026-09-06 for the boot-read bound.

- **✔ F-THREE-MASKED-RENDERERS: CLOSED 2026-09-12 (LENS-WAVE3, lens M11).** `Vdp::render_frame_masked` (`&self`)
  is the one masked picture; the three sites are thin consumers, each pinned against it by a parity row.

- **⚑ MY BRIEF WAS WRONG ABOUT THE REFUSAL, AND THE CORRECTION IS A DESIGN FACT WORTH KEEPING.** I wrote
  that S2a *"deletes that gate and its test"*, quoting the S0-S2 doc. **It deletes the BLANKET gate and
  needs a narrower one, because this slice creates the very fact the old reasoning lacked: once the window
  has two pixel paths, *the mask the machine holds* and *the mask the glass was drawn with* are two
  different facts.** They separate when the mask moves after the picture (the palette can call
  `set_layer_enabled` inside the same `build_ui`) or when a masked render yields nothing. So `Panel::pick`
  takes the glass's mask as a **parameter** and refuses only on disagreement, with `None` the honest
  *"no picture yet"* rather than a fourth state. Invisible on every ordinary frame.

**Registered 2026-09-05, from verifying a relayed authorisation instead of absorbing it:**

- **▶ F-VSYNC-NEVER-MEASURED: AN OWNER-AUTHORISED FOREGROUND PASS WAS SPENT WITHOUT ANSWERING THE QUESTION
  IT WAS AUTHORISED FOR, AND THE MIGRATION'S RETIREMENT GATE DEPENDS ON THE ANSWER.**
  **The granting act, verified firsthand** (the hub relayed it; this seat checked rather than adopting):
  empyrean **`0689c55`**, an ancestor of their `origin/main`, a **docs commit carrying a docs ruling**, and it
  carries the words itself: owner, 2026-09-02T20:36:03Z, *"6. Load them for me and tell me what to look
  for"*, applied by the hub as **explicit authorisation for TWO NAMED RUNS on his display (aeon's left-edge
  gate and oracle's vsync spike), and recorded there as NOT a standing one.**
  **Ours was run** (lane log 2026-09-02T21:10:26Z, `DISPLAY=:0`, ownership confirmed at 1920×1080, 120 s,
  exit 0), so **the authorisation is spent.** ⚑ **But it FREE-RAN.** Its own entry says *"NOT a vsync-paced
  measurement: the spike free-runs, hence 93 rather than 60"*, and `docs/2026-09-02-toolkit-spike.md:21`
  still reads **"Presented fps under vsync on the real GPU? NOT MEASURED. Deferred to a foreground pass."**
  **So the deferral survived the run that was supposed to close it, and the cell is still empty.**
  ⚑ **Why this is load-bearing rather than trivia: the hub's retirement condition for `oracle-frontend` is
  "60 fps and audio pacing measured on the real player under the toolkit, IN THE SAME FORM AS THE SPIKE
  DOC".** That form has this cell blank. **S8 cannot honestly close against a shape whose headline number
  was never measured**, and measuring it needs his display, i.e. **a fresh authorisation**, since the one
  that existed was narrow and is spent. Filed as **`d-29`**; it gates S8 only, so nothing stops until then.
  ▶ **RULED 2026-09-05 by the HUB under the owner's standing delegation; record it as the hub's, not his,
  and it is overturnable by him** (`d-30` supersedes `d-29`): **`at-s8`**, this lane's own recommendation.
  Explicitly **not** `drop-the-bar`. Their words: *the retirement condition stands as written, and now stands
  knowing its headline cell is empty, which is better than standing on a number that looked measured.*
  ⚑ **The S8 session owes the ask, and its form is prescribed: ONE SENTENCE saying what to look for, framed
  exactly as the first run was.** His fresh word is required (`0689c55` authorised one run and it is spent),
  and no agent may take it: this lane's flat rule bars launching any window while his player may be live, and
  a headless framebuffer has no vsync, so it would answer a different question while looking like an answer.
  ⚑ **The durable shape, and it is bar 24's inverse: an instrument was OBTAINED, used, and the question
  still went unanswered. Nothing announced that.** A spent authorisation looks identical to an answered
  question from every artifact except the one cell nobody re-read. **When a run is authorised to answer a
  named question, check the question's own cell afterwards, not the run's exit code.**

**Registered 2026-09-05, from landing migration slices S0-S2:**

- **✔ F-PARITY-BLIND-TO-SAT-STRIDE, registered and CLOSED the same day; the registration moved whole to `OVERSEER-LOG.md` 2026-09-05 for the boot-read bound, and the closure is the `F-PARITY-BLIND-TO-SAT-STRIDE: CLOSED 2026-09-05` row under *Registered 2026-09-05, from landing S2a*, with its narrative in the log beside this one. The live rule it produced: a guard called the strongest in the tree had a hole reachable in ONE mutation, found only because the mutation parameter was VARIED rather than repeated. Bar 19's enumeration-parameter rule arriving on a mutation instead of a survey.**


**Registered 2026-09-05, from the frontend-migration recon:**

- **✔ F-FRONTEND-PALETTE-BUS: CLOSED, NOT BUILT** — its blocker (*"needs a free-text argument mode"*) is true of `oracle-frontend` and moot for where the work would land, since `oracle-player`'s palette already IS that mode and the row would mean editing the crate d-25 retires. Moved whole to `OVERSEER-LOG.md` 2026-09-06. **The live residual is a different row:** `commands.rs`'s 42 rows are frontend *actions*, not bus methods, so their replacement belongs against the player, not the palette.


**Registered 2026-09-05, from landing the press frame-cap parcel:**


- **F-HANDSHAKE-LOAD-TIMEOUT**: `tests/handshake.rs::initialize_advertises_a_generated_method_list_that_is_the_dispatch_table`
  fails with a socket read timeout (`WouldBlock`, `tests/common/mod.rs:196`) under load average ~250.
  **The implementing agent's measurement, not re-derived here** (3/3 on their branch, 2/2 on `main`'s
  `oracle-aether`, green 15/15 at normal load), so it is attributed rather than asserted. Same CLASS as the
  wait_for_break defect just fixed (a suite row that only fails under peer load), and a **different root
  cause**; do not assume the fix reached it. Booked because this repo has now twice written off a
  load-sensitive row as a flake and been wrong: the wait_for_break row was tagged a flake on 2026-09-03 and
  again on 2026-09-04 before it was root-caused. **A row that only fails under load is a defect with a
  narrow window, not a flake, until something says otherwise.**

- **F-SPAWN-PICKER-PANEL-SURFACE: CLOSED.** The parcel landed on `oracle-frontend` (merge `531894e`); the
  owner question it carried was retired unasked by the d-25 swap-toolkit ruling, which makes the two windows
  one. Narrative, both corrections and the wrong-file measurement moved whole to `OVERSEER-LOG.md` 2026-09-09.
  **The live lesson: a question can be answered by a ruling made elsewhere, and nothing retracts the ask.**

- **F-SHIM-SOCKDIR-RESIDUE: the PROCESS half did not reproduce; the FILESYSTEM half did.** Zero leaked `oracle-aether` processes against a working control, but 50 `/tmp/oracle-mcp-*` dirs with 50 socket files and ZERO listeners, spanning 2026-08-27 to 09-03, at **200 K total** rather than the relayed 38 MB (which was process RSS, a different quantity). **Ruled: nothing to reap**; the shim-reaps-on-disconnect question is **booked, not fixed**, since the shim is `oracle-old/linux-port/mcp/oracle_mcp.py` and `oracle-old` is reference-only with the cutover existing to delete it. Revival: the cutover replacing the shim, or `/tmp` pressure becoming real. Moved whole to `OVERSEER-LOG.md` 2026-09-06.

**Registered 2026-09-04, from aeon answering the CR-J §11 flag:**

- **CR-J §11 UNMEASURED FLAG: CLOSED, MEASURED, both halves** — aeon statically at their `origin/master`, ours live on `aeon/s4.debug.bin` at frame 240 through a channel neither lane authored (the SAT the emulated 68000 wrote), with the camera at (96,144) so the identity was not vacuous. Moved whole to `OVERSEER-LOG.md` 2026-09-06. **Two live residuals:** we read the camera as **unsigned** and add as u32, so we cannot express a negative world coordinate and an out-of-act click reaches aeon as a large positive rather than as something obviously wrong (told to them, not left implicit); and the frontend comment at `bus.rs:405-408` still carries the flag as open — fix it in the next parcel that opens that file.

- **✔ F-SPAWN-OUTSIDE-ACT: CLOSED 2026-09-04 (`parcel/spawn-outside-act`), window-side, refused not clamped, with its original booking and the wrong-symbol trap (`Player_Bound_Right` is INSET and objects are deliberately unclamped) moved whole to `OVERSEER-LOG.md` 2026-09-05 for the boot-read bound. The live residual is the three-surface gap: the debug window's palette reaches `emulator/object_spawn` by name and this parcel's check lives in `oracle-frontend`, a different crate.**

**Registered 2026-08-29, from aurora's relay of the owner's R8 question:**

- **F-R8-LATE-REVISION**: a *copy-of-column-0* toggle for the R8 leftmost-partial-column quirk, so a
  fix can be seen under both hardware behaviours. **Declined as a booking, and the reason is not
  "noise".** Under the later behaviour the leftmost column takes column 0's vscroll, so the defect
  simply **disappears** and aeon's column-19 write becomes an inert no-op rather than something the
  differential validates: the whole verdict is derivable without building it. Against that we have
  **no hardware-tested rule for the late revision**, only Plutiedev/Stef descriptions, where the early
  rule pinned at `render.rs` `plane_vscroll` (H40 `VSRAM[$4C] & VSRAM[$4E]`, H32 `0`, same value both
  planes) matches Genesis Plus GX's *"verified on PAL MD2"*. Shipping a second model whose fidelity
  cannot be established, and then letting a consumer validate a fix against it, is bar 9's corollary
  exactly: an unvalidated instrument adopted as a gate returns a **confident wrong verdict**.
  *Revival condition:* a hardware-tested rule for the later revision appears, **or** a second scene
  turns up where the fork changes a design decision. The divergence ledger already records the fork,
  so nothing is hidden meanwhile.

- **F-BANNER-INVITES-A-PIN**: *found from the other end, by a consumer breaking on it (aurora's O26,
  2026-08-29).* Our startup banner prints `aether: N methods advertised`, and `Bus::start` prints the same
  total on the serving line. **A published total is an invitation to pin it**, and a consumer did: their
  `classic-playtest-harness.mjs:171` pinned `methods === '35'` and *threw* `stale oracle-aether binary` on
  anything else, so the guard written to detect staleness became the stale thing and rejected every
  correct binary. **Measured firsthand at `6031020`, two ways: the banner says 52, and `initialize`'s
  `methods` array has length 52** (spawned and called, not read off a schema).
  **The defect is theirs; the surface that manufactures it is ours.** A count changes for reasons unrelated
  to freshness and is identical across binaries that differ, so it is the wrong observable for the question
  every consumer actually asks: *is this binary current?* We already serve the right answer and do not point
  at it: `initialize.serverBuild` carries `{id: "<sha>+profile=…+target=…+features=…", source, dirty}`.
  **Cheap fix, and it is a documentation-and-adjacency fix, not a removal:** name `serverBuild` in the same
  breath as the count, so the number a reader meets first is not the only identity on offer. Do **not**
  simply delete the total: it is genuinely useful at a glance, and aurora's lesson is about what a consumer
  should *key on*, not about what we may print. Our own side is clean: grepped, every use is `METHODS.len()`
  and no literal total is pinned anywhere (`crates/oracle-aether/tests/params_closure.rs` closes over
  `METHODS.len()`, which is the derived form).
  **The durable line, aurora's: a total was the wrong observable.** Same family as this file's own
  name-is-not-behaviour bar: a number that *correlates* with the property being tested, standing in for the
  property, and reading exactly like a real check until the correlation breaks.

  ⚠ **AMENDED SAME DAY, and the amendment corrects THIS BOOKING, not the consumer** *(aurora, 2026-08-29,
  who class-checked the SHA out of habit and found the thing I had just recommended people point at)*. The
  paragraph above says "name `serverBuild` in the same breath as the count". **That advice is incomplete in
  a way that walks a consumer back into the same trap from the other side.** `serverBuild.id` names whatever
  HEAD was at build time, and the id they measured, `6031020`, is a **docs-only commit** (`lane-status.json`,
  +10/−18). That is correct behaviour for a build identity and is exactly what staleness wants, but it means
  **the id moves for reasons that have nothing to do with the code**: binaries built at `acf41f5` and at
  `6031020` contain identical code and report different ids.
  **So the field answers *"is this the same binary I measured before?"* and MUST NOT be compared for equality
  to answer *"does this build contain feature X?"***: that second question belongs to `capabilities` and
  `methods` membership, which is the derived form this register already recommends. Whenever we point a
  consumer at `serverBuild`, we owe them that sentence in the same breath; a pinnable identifier offered as
  the cure for a pinnable count is the same defect in better clothes.

- **F-SERVERNAME-PREDATES-THE-RENAME**: `EngineConfig::default()` sets `server_name: "oracle-next"`
  (`crates/oracle-aether/src/engine.rs:205`, read at `fee8f12`), so every `initialize` still answers with the
  **pre-rename** repo name; `serverVersion` is `"0.0.0"`. Spotted by aurora 2026-08-29 while assessing what on
  our wire is an identity.
  **Not a wire-correctness bug, and say so plainly:** §2.1 deliberately demotes `serverName` to a *deployment
  label* and moves identity to `implementation` (`"oracle-rs"`) and `serverBuild`, both read from `build_info`
  and (verified firsthand, not from the comment) **barred from configuration by a source-level test**
  (`tests/server_build.rs::neither_identity_value_is_reachable_from_configuration`). A consumer reading
  `serverName` for identity is reading the field the contract told it not to.
  **But the value is still a stale name we publish on every handshake**, and "it is only a label" is exactly
  how a wrong string survives a rename. **Changing it is wire-visible**, so it does not get a drive-by edit:
  it needs bar 14's consumer-set enumeration first (grep every sibling tree for the literal `oracle-next`
  with real client context) because the failure mode of a consumer matching on it is silent. Revival
  condition: do it as part of any deliberate handshake pass, never alone.
  **ONE CONSUMER CLEARED, AND THEIR OWN CAVEAT IS THE REASON IT IS STILL NOT A GREEN LIGHT** *(aurora,
  2026-08-29, asked for exactly this input)*: they grepped `src/`, `test/` and their harnesses: 40
  references to `serverName`, **zero** comparisons/branches/`includes`/`startsWith`/`match` on its value;
  every use is display or pass-through. **But they also volunteered that the literal `'oracle-next'` appears
  21 times across 8 of their test files as fixture INPUT**, with assertions derived from the payload
  (`expect(s.serverName).toBe(PAYLOAD.serverName)`), so a rename leaves their suite green **while their
  fixtures quietly describe a server that does not exist.** Their formulation, worth keeping verbatim in
  spirit: *our green is not evidence your rename is safe; it is evidence we do not look.* That is the
  clearest statement of this register's own no-consumer-broke hazard anyone has offered, and it came from
  the consumer. **Four repos remain unenumerated** (aeon, seraph, sigil, empyrean), so the booking stands;
  aurora has asked to be told if it moves, so they can re-point their fixtures.
  ⚑ **AMENDED 2026-09-09: the field now has a named live READER, and aurora's clearance asked the wrong
  question.** Their bus badge renders `serverName ?? 'oracle'`, so it displays **`connected · oracle-next`**
  in every deployment — measured here at `aca9e9a`: `server_name` has exactly TWO sites, the field
  (`engine.rs:213`) and the default (`engine.rs:279`), and **nothing sets it**, so §2.1's "a deployment
  label a config may set" is unreachable in this tree and its stated justification (two same-implementation
  processes wanting distinguishable names) describes the very case it cannot serve. Their sweep cleared this
  field as *"every use is display or pass-through"* — **display was in the clear list**, which is right for
  almost every field and exactly wrong for an identity one. **Durable, and it is bar 14's blind spot: a
  consumer sweep asking "does anyone BRANCH on this?" cannot see "does anyone SHOW this?"** A rename would
  have left their suite green, their sweep correct, and a wrong name on screen. Revival condition is
  unchanged (a deliberate handshake pass, never alone) but it is no longer cosmetic; aurora's
  `parcel/bus-identity-displayed` is the live consumer.

- **F-RSP-XVFB-ORPHAN: AUDITED CLEAN, and that is a measurement rather than an assumption.** aurora's O16 warning was about a `pkill -f '<dist path>'` teardown that had killed a peer's processes three times; enumerated here by what *touches process teardown* rather than by the token, **this repo's teardown sites are exactly one** — `rsp.py:157 self.p.kill()`, on the `Popen` handle that object itself spawned — and every `pkill`/`killall` string in the tree is docs prose warning against it, with a 165-hit bare-`kill` control proving the grep could see what was there. Moved whole to `OVERSEER-LOG.md` 2026-09-06. Revival: the differential harness run in anger again, or a stray blastem/Xvfb outliving it — fix is a process group, not a wider pattern.


**▶ REGISTERED 2026-08-30: F-LEGACY-SILENT-DEFAULT, and it is the sharpest argument the cutover has.**
*(Measurement history in `OVERSEER-LOG.md`.)*
**Revival condition:** the README sentence owner ruling 4 requires; it should carry this fact, not
merely that the surface is legacy. Explicitly NOT a fix recommendation for `oracle-old`: it is
reference-only and the cutover exists to delete it.

## ⚑ OWNER RULING: PUSH AUTHORIZATION. ✅ **CONFIRMED DIRECTLY BY THE OWNER, 2026-08-24, IN THIS SESSION**

**STANDING APPROVAL, OWN REPO ONLY: a lane may push its own repo's master without asking each
time.** Reached us via empyrean-18, banked by the hub at empyrean `2bd72a03`, **verified firsthand
here: the object exists, is an ancestor of their `origin/main`, and is a docs commit carrying a docs
ruling, so its SHA class matches what it anchors.** Flagged as a relay per this lane's own standing
rule; ~~direct owner confirmation requested in-session.~~
**✅ THE CONFIRMATION ARRIVED, AND THE FLAG IS REPLACED RATHER THAN DELETED, per the rule that wrote
it.** The owner answered decision `d-1` directly in this session on 2026-08-24, choosing **"Confirm it
as standing permission"** from the two options put to him. **This lane may now push its own repo's
master without asking each time**, under the four conditions below, which ride with the grant and are
unchanged by the confirmation. *Why this relay was usable before the confirmation arrived — a granting
act described rather than a status field quoted — is in `docs/OVERSEER-REFERENCE.md`.*

**The conditions ride with the grant and are part of it**, transcribed rather than paraphrased:
- **verify `origin` actually moved: the push is not the act, the remote moving is.** This is the
  protocol's own push-before-you-cite rule arriving as an owner condition;
- **never rewrite already-pushed history**;
- **never push another lane's repo**;
- **publication to the public wiki site stays a separate explicit ask**, not a concern in this
  tree today, but it becomes one the moment the wiki-emulator spike produces anything shippable.

**Scope, stated by the hub because this is the class of grant that gets restated wider: it
authorizes PUSHING, not the work being pushed.** It does not release this lane's boot stop, it is
not approval to dispatch or to land a parcel, and **it does not touch the CR-A/CR-B adjudication
hold**, which remains a separately parked owner item.

## ⚑ HUB RULING UNDER DELEGATION, 2026-08-27: d-16 SUBSTITUTE: the reviewer seat, and the rule it creates

⚑ **RELAYED, NOT WITNESSED BY THIS LANE**: same flag, same reason, as the four rulings above. The
owner armed an overnight delegation in his own words (*"if anything needs decision that they can't
make you make it for them"*, transcribed by the hub into empyrean `OVERSEER.md` addition (f) at
05:39Z, banked `091ac59`) and went to bed; the hub ruled in his place and **he reviews it on return.**
Record it as the hub's ruling. Do not upgrade it to his. *The question it answered, and what it was
blocking, are in `OVERSEER-LOG.md` under* **d-16, the question the SUBSTITUTE ruling answered**.

**THE STANDING RULE THIS CREATES, and it outlives tonight.** Adjudications run on the ordinary model
while the seat is parked, and **every ruling produced that way NAMES ITS OWN REVIEWER, at the top, in
the ruling itself.** Not in a covering note, not in the dispatch record, but in the artifact a later
reader picks up cold. The reason is the whole design: Fable's first job when the owner lifts the limit
is auditing exactly these, and an audit cannot find what does not announce itself. **Independence is
preserved: a fresh reviewer that took no part in the drafting is still the half that catches real
problems. Reviewer tier is what was spent.** Say it that way; do not describe a substituted ruling as
adjudicated without qualification.

**Ledger:** every substituted adjudication gets an entry in
`docs/2026-08-22-unadjudicated-decision-ledger.md` **at the moment it is dispatched**, not
reconstructed after: the entry must be adjudicable cold, and must name *what the audit should re-run*
and *what would have to be true for the ruling to be wrong*. First entry is **L-07 (CR-A)**, which also
records the cheap first cut for the audit: **re-run the material items only**, since the M/S split
every ruling here is required to produce is the instrument that measures what the substitution cost.

⚠ **Numbering collision, live:** aeon also has a card numbered `d-16` (background chunk height). The
console shows two. **Never cross-reference a decision by number alone across lanes**: say the lane.

## ⚑ THE CUTOVER: ruled 2026-08-22 (RELAYED, see the flag above), mechanism determined firsthand

**The ruling** (owner, via empyrean, quoted): *"I say do it now and when something is needed have it
built out, no?"* Cut `mcp__oracle__*` over to the Rust server **now**, and close the remaining
methods **on demand** rather than in enumeration order. empyrean recommended registering alongside
the legacy server; **he overruled it** and they now agree, as do I: it converts the acceptance
contract from a catalogue into demand-driven work.

**✅ RULED PROCEED (relayed, with the full measured cost disclosed to him first: aeon's two gates
down ~a day, Z80 no real consumers, binary needs rebuilding). His words:** *"Yeah just proceed. We fix
when we come across it, if we don't we build later but this is really just to start building out the
tooling."*
**⚑ THE LAST CLAUSE IS THE GOVERNING ONE, AND IT REFRAMES THE WHOLE ACCEPTANCE CONTRACT.** The cutover
is **not** happening because the successor is ready; it is happening **because being reachable is what
generates the demand that builds it out.** So the remaining 17 are **not a checklist to burn down
before the switch; they are a queue the switch POPULATES in priority order.**
**▶ CONSEQUENCE FOR EVERY BRIEF FROM HERE: an early gap is a SUCCESS SIGNAL, not an embarrassment.**

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 565-566.)*
State it explicitly to agents: the instinct will be to treat every `-32601` as a failure that should
have been prevented, and under this ruling it is the mechanism working. **This does NOT relax the
loud-failure requirement**; it is the reason for it. A gap that refuses by name feeds the queue; a gap
that degrades to a plausible answer poisons it.


**▶ ALSO OPEN, and it points INWARD at our own docs (aurora ran it on theirs and it drew blood).**
Their split: **engine facts** (properties of aeon, seen through a window; unaffected) vs **server
facts** (properties of ONE implementation). Their worked example is the one to internalise: they
re-derived the `require_paused` set **from our Rust source**, banked it as a correction, and wrote it
into a section that had always called these properties of *"the bus"*. **It is a property of one
implementation and the correction did not say so**: a defect created *while applying every other bar
correctly, in the act of fixing a different staleness.* **We have the same exposure and more of it:**
this repo's recon, demand and CR docs describe "the server" throughout, and D-10/D-13/D-17 are already
booked as having **two implementers**. A sweep is owed: every claim about server behaviour either
names its implementation or is a latent two-implementer conflation. *The durable formulation this
produced — freshness is not transitive across a document — is in `docs/OVERSEER-REFERENCE.md`.*

## ⚑ THE SOCKET CHAIN, AND F-CHAIN-QUOTED

**There is no chain.** `empyrean/clients/python/aether.py`'s resolver commits on a *directory* test, so
every lane resolves to `$XDG_RUNTIME_DIR/oracle.sock` and stops; `/tmp/oracle.sock` is unreachable dead
code. The spec specifies a chain and the reference client never implemented one: a conformance gap,
not a stale comment. **Operational consequence: start a server on `/run/user/1000/oracle.sock`.**
**F-CHAIN-QUOTED stands:** two historical recon docs here name the socket paths and neither was written
from the resolver. Revival: before any doc here is cited to a peer as the transport's behaviour.
Full measurement, and what each of the three lanes had right and wrong, in the log.

*(The shim-half measurement is in the log. The hazard it leaves behind is live and is this:)*

**THE HAZARD TO ENFORCE, and it is this seat's job:** once the new server is the only one reachable,
every failure presents as *the consumer* being broken and the gradient pushes lanes to engineer
around gaps instead of reporting them: **bar 9's corollary with the causation hidden.** Counter-
measure: **an unserved method must fail LOUDLY and BY NAME, never degrade to a plausible answer.**
Ours already does (`-32601` unknown method; `-32602` naming the key at the dispatch choke *before*
the handler); the legacy server silently defaults unknown params, which is exactly why bar 15 says
sequence a cutover onto the STRICT implementation. **A missing capability that returns something is
far worse here than one that refuses.**

## ▶ LAYER-MASK: LANDED. One safety property survives it, and HALF OF IT IS A CONVENTION

**What the compiler enforces — verified firsthand here, not taken from the parcel:** *no render that takes a
`LayerMask` takes `&mut self`.* The four mask-taking renders are all `&self` (`render.rs`
`resolve_line_masked`, `render_line_masked`, `render_line_report_masked`, `pixel_attribution_masked`);
`Vdp::commit_scanline_sprites` — the write behind the sprite-overflow/collision latches and the R10 carry —
takes `&mut self` (`vdp.rs`); `oracle-core` is `#![forbid(unsafe_code)]` with no interior mutability in fact.
**So a masked path cannot compile a call to the commit.** `LayerMask` is also a parameter and never a field,
absent from `vdp.rs` and `system.rs`, so it is in no snapshot and no `state_hash`.

**What REVIEW holds, and only review: `render_scanline` does not gain a mask parameter.** ⚠ **This section
asserted that was "enforced by the type system" and it was FALSE** — H26, fixed at `37e6d12`, landed. **Proven vacuous, not argued**: the agent planted the exact forbidden shape
(`render_scanline_masked` committing `overflow && mask.sprites`) and 891 core tests, clippy `-D warnings` and
`fmt` all stayed green. **Nothing in the language can carry it**, and that is the durable half: *a type system
constrains programs under a signature, it cannot constrain edits to the signature.* The harm is guarded by the
named test `masked_renders_leave_the_committed_sprite_latches_untouched`, never by this paragraph.
⚑ **Eleven sites, not the packet's eight** — found by varying the grep SPELLING over six phrasings; the real
mechanism was found by varying the RECEIVER (`&self` vs `&mut self`), an axis no phrasing search reaches.

⚑ **THIS RESOLVES THE C5/H26 SEQUENCING QUESTION RULED EARLIER TODAY, and in C5's favour.** The invariant is
deliberately *not* "there is exactly one stateful render": **C5's cheap UNMASKED twin cannot violate it and is
admissible; a MASKED twin stays forbidden.** The parcel was written that way on purpose after `6b0b75e` landed
mid-flight — had it finished twenty minutes earlier the doc would have said "exactly one stateful render" and
C5 would have falsified it on landing. **C5's brief must carry this sentence.** `docs/2026-08-26-layer-mask.md`
is the artifact of record.

*(Moved 2026-09-11: the superseded C5/H26 sequencing hold and its 2026-09-10 discharge to `OVERSEER-LOG.md`, the `fixedAt` brief correction to `docs/OVERSEER-REFERENCE.md`, each under **Moved from OVERSEER.md 2026-09-11**; the pairing's four lessons stay in the reference under **FROM THE C5/H26 PAIRING, 2026-09-10**.)*

## ⚑ HUB RULING, 2026-09-02: HERMETIC GATE IS THE RATIFIED SHAPE; DRIFT IS A NIGHTLY, AND IT GETS **NO SECOND OWNER CARD**

⚑ **RELAYED BY empyrean-01, NOT WITNESSED BY THIS LANE**: same flag, same reason, as the relayed
rulings above. It is the **hub's** ruling under the owner's standing delegation. Do not upgrade it to
his. Anchored at empyrean **`1e9d70c`**, verified firsthand here rather than taken on trust: the object
is a **commit**, it is an **ancestor of their `origin/main`**, and `--stat` shows it is a **docs commit
carrying a docs ruling**, so its SHA class matches what it anchors.

**The question this answers** is the one `parcel/stopprecision` left open in
`docs/2026-09-02-stopprecision.md` §6: our schema gate went hermetic (blob-pinned, no peer read), and
the deliberate cost recorded there was that **a default run no longer notices upstream moving on its
own**. **Ruled: the hermetic default is the ratified shape, and drift detection is a NIGHTLY's
property, never a local run's.** Same shape as sigil's decouple: vendored content plus a revision
stamp, local runs hermetic, drift watched out-of-band.

**THE OPERATIVE INSTRUCTION, and it is a prohibition; read it before filing anything:** the drift job
is **a queue row here, not an owner card.** The host question it would raise (a standing unattended
timer on the owner's machine) was raised with him as empyrean `d-9`, whose question is literally
*"Running it means a systemd timer on YOUR machine … Do you want that standing job installed?"* — the same
question ours would ask in different words. **One cross-lane question gets one card.** A second card does not
add information; it makes him answer the same thing twice and lets the two answers diverge.
⚠ **CORRECTED 2026-09-10: `d-9` IS ANSWERED AND THIS PARAGRAPH CARRIED IT AS OPEN FOR EIGHT DAYS.** Verified
at their `origin/main`: **`chose: "install"`, `by: "hub"`, `at: 2026-09-02T03:47:48Z`**, and empyrean's own
`blockedOnOwner` carries only `SERAPH-HOLD` and `FILMING-NOD`. **So the host half of this row's blocker is
DISCHARGED**; what still blocks `SCHEMA-DRIFT-NIGHTLY` is the cut-the-ceremony moratorium alone, and when that
lifts the row needs no further owner ask. ⚑ **IT LIFTED 2026-09-10 (hub ruling above), SO THIS ROW IS
UNBLOCKED AND OWES HIM NOTHING.** Unlike `F-CITATION-LINT` it survives ruling (c) on its own: its merits were
argued and only the blocker outlived them. **Take it as ordinary queue work, not as a revival.**
⚑ **THE TRAP, AND IT IS WHY THIS SURVIVED: the answer lives under a DIFFERENT ID.** The ledger records it as a
separate appended row `d-9-answered`; the row actually numbered `d-9` still reads `answered: false`. **A
lookup by the id you were given returns OPEN and is wrong** — dedup-by-id, the technique this lane uses on its
own ledger, cannot see an answer filed under a sibling id. Match on the QUESTION, or on an `id` prefix, before
reporting any peer's card as open. *(Found because the hub went to doubt a citation of mine and checked it.)* *(`d-7-restated-3` is the companion card on
how many quiet chains before review, provisionally ruled N=5.)*

**Board row id: `SCHEMA-DRIFT-NIGHTLY`**. This section is that row's detail, per `LANE_STATUS.md`
rule 7 (a title states the state; the history lives here and the row points at it by id).

**The shape to build, when it is picked up:** a runner with `AETHER_CONTRACT_REPO` set, **non-blocking**,
reporting *"contract advanced past pinned blob"*. Note it needs **no new capability**: the hermetic
gate already grew exactly that env-var path as step 2 (`schema_conformance.rs`), so the nightly is a
caller of a road already built, not a build.

**Board row id: `F-FROZEN-FIXTURE-DRIFTS`** — landed 2026-09-06. Its detail (the four drifted dimensions,
the `DIMENSIONS.tsv`/`aeon_dimensions.rs` shape, and why no second owner card was filed) is in
`OVERSEER-LOG.md` under the row's own name; the reusable correction it produced is in
`docs/OVERSEER-REFERENCE.md`.

**▶ BOOKED, NOT BUILT, 2026-09-09: `F-CITATION-LINT` — make a drifted cross-repo citation UNEXPRESSIBLE
rather than detectable.** The claim-vacuity agent's proposal, recorded verbatim in shape because it should not
be re-derived: a ~30-line test **in this repo**, no peer checkout, no network, no host question — regex every
comment under `crates/**` for `(aeon|sigil|empyrean|seraph|aurora)[:/][\w./-]+:\d+` and fail unless the
enclosing comment block also carries a 7+ hex revision. `effects.rs`'s `NOTE = "aeon c4c5c3d8 …"` passes by
construction; every citation fixed in that parcel passes by carrying no bare number at all.

**RULED: do not build it now, and the reason is the moratorium, NOT the merits.** The owner's CUT THE CEREMONY
ruling bars instrument work that is not a DoD item or shipping wrong output, and a comment is not shipped
output. Said plainly to the agent rather than dressed as a design objection.
**The cost of adopting it is not the 30 lines**: ~15 citations resolve correctly TODAY while being bare line
numbers into moving HEADs, so the lint goes red on arrival and either drags a second parcel with it or gets an
exemption list that hollows it out. So the row is ONE row doing both halves, never the lint alone.
⚑ **REVIVAL CONDITION FIRED 2026-09-10 AND THE ANSWER IS STILL NO — this line was written wrong, not merely
overtaken.** It said *"revival: the moratorium lifting"*; the moratorium's clause 2 lapsed today, and the hub's
ruling is that **merits cannot be inherited from a lapse** — and this row's own text says the hold was *the
moratorium, NOT the merits*, so there are no merits to inherit. **A revival condition that names only the
BLOCKER is malformed**: it promises revival on an event that says nothing about whether the thing is worth
building. Name what would make it WORTH doing. ⚑ **The real obstacle is untouched by the lapse and is stated
above: ~15 citations resolve correctly today while being bare line numbers into moving HEADs, so the lint goes
red on arrival** and either drags a second parcel or takes an exemption list that hollows it out.
⚑ **DECIDED 2026-09-10 ON THE MEASUREMENT, under delegation. THE LINT AS DESIGNED IS REFUSED — its catch
rate on the real population is ZERO.** Its regex is `(aeon|sigil|empyrean|seraph|aurora)[:/]…:\d+`, i.e.
**cross-repo only**; all five citations the `sweep-t4-comment-truth` parcel actually repaired were **in-repo**,
and its worst case (a bare basename with five candidate files) is an **ambiguity** problem the shape does not
model at all. ⚑ **THE DURABLE LESSON, and it is why this was worth measuring rather than arguing: a check
designed from the FINDING THAT PROMPTED IT, rather than from the POPULATION IT MUST COVER, can have a zero
catch rate and still look right on review.** Same family as the 09-09 guard holes — the cure is deriving from
the live population, never another sweep of the same axis.
**What the measurement DOES support is a different check**, and it is not revived on the strength of that
either: `\bthis (commit|push)\b|a later commit|not yet` over `crates/**` comments would have caught all 14
M34 sites in one pass plus ~29 booked siblings, a class four hand passes each declared exhausted. **But that
population is partly stale — 3 of 3 spot-checked siblings were ALREADY FALSE.** So the order is: **verify the
~29 first as lens work, then let that number decide the instrument.** Measure the population, then build the
check; not the reverse.
⚑ **And it stays distinct from `SCHEMA-DRIFT-NIGHTLY`, which owns CONTENT drift** — a revision-pinned citation
that has gone stale is a different question from one that was never pinned. Do not merge the two rows.

**Board row id: `ATTR-RGB-LATCH`**. Detail lives in `docs/2026-08-30-rgb-live-resolve.md`
(aeon's colour finding: reproduced 55/55, closed as a server change; ~~what remains is a contract change
so the reply says which moment its colour is for and names `emulator/scanlines` as the caller's path~~).
Anchored here 2026-09-02 because the row's own title carried the only copy, and `LANE_STATUS.md` rule 7
requires the row to point at its detail by id.

⚠ **CORRECTED 2026-09-03: THE CR IS FILED AND ADOPTED; WHAT IS OWED IS OURS TO BUILD.** The struck sentence
said a CR still had to be filed. **CR-G was ours, and was adjudicated `ADOPT WITH CHANGES` as
`contract/protocol.md` §11.27** at empyrean **`32a0041`** (2026-08-30T02:37Z; ancestor-verified, `--stat`
shows protocol +48 and schema +6, so the SHA class carries what it anchors). *(How the row stayed wrong for
four days, and why the hub was right to change our emission rule: `OVERSEER-LOG.md`, 2026-09-03.)*

**What §11.27 leaves owed, read out of the adopted text and not summarised from memory:**
1. **The emission rule is a MEASUREMENT, and it is NOT the one we proposed.** Adopted: emit when the CRAM
   entry at `cramIndex` **has been written since line `y` of the last completed frame was drawn**, or when
   no frame has completed; absent otherwise. A server that cannot yet stamp per-entry writes **MAY** emit on
   *any* CRAM write since the line drew (coarser, still conditional), and **MUST NOT emit unconditionally.**
2. **Four vectors, and §11.27 names US as their author**, red-first, run against the schema before handover
   (the bar this lane set itself on CR-F): caveat after a qualifying write (valid); a caveat naming no method
   (red); a pre-first-frame reply carrying it (valid); no qualifying write and no caveat (valid).
3. **The "required when applicable" half is a LIVE CONFORMANCE CHECK, never a schema property**: a schema
   cannot see the write stamp. It rides with our conformance rows the way `object_at`'s did.

**State measured here 2026-09-03, both sides.** The vendored fragment at our pin **already declares**
`caveat` on `emulator/pixel_attribution`'s result, with §11.27's rule quoted in its own `description`; and
`Engine::pixel_attribution` **never sets the key**. So we are conformant-by-omission (the fragment does not
require it) and silent on exactly the divergence aeon asked us to make audible. **F-SCANLINE-INDEX is
untouched**: §11.27 makes the divergence audible, it does not close it.

⚑ **AND THE SCOPE OF WHAT THE HUB CAN HAND US, ESTABLISHED 2026-09-02 BY THE HUB RETRACTING ITS OWN
GO; bank this, it will recur.** At 10:40Z the hub cleared `LIVE-TREE-RESIDUE` "under the owner's
widened delegation". **This seat held anyway** and the hub then **withdrew it against its own
interest**, which is the strongest form this correction could take.

**The test that decided it, aeon's, and it is the reusable part: *is there an owner decision under
this, and is it THIS question?*** Applied here: the owner's 03:22Z words (empyrean `63c85ae`) re-arm
the **raster/parallax effects** drive; his 03:46Z widening (empyrean `4e8e865b`) covers **decision
CARDS in a lane's domain**. Neither is a word to start a **non-effects** parcel, and the live-folder
cleanup is this lane's own hygiene row. The nearest owner decision under it is `d-17` (his: the
*write* side into his aeon folder); the *read* side, sigil's `d-18`, **was ruled by the hub under
delegation, not by him.** So no owner decision exists for this question, which is exactly what the
test asks.

**THE DURABLE SPLIT, and it is what a future session should apply without re-deriving:** under today's
brief the hub can hand this lane **any ask an effects lane files**: those need no owner word and
should just be worked. It **cannot** hand us a go on our own hygiene, our own backlog, or anything
outside the effects drive. **When a relay's go and this test disagree, the test wins and you ask the
owner.** Precedent from the same morning, banked in the hub's own record at empyrean `13a7d5a`: sigil
adopted a hub *ruling* while holding for the owner's *word* on the same shape: **a relay carries a
ruling, never an authorization.** Two lanes, same hour, same conclusion, reached independently.

## ⚑ `run_to` vs `resume`: TWO LIVE RULES (2026-09-02; derivation in `OVERSEER-LOG.md`)

Answered for aurora and re-derived by them independently at `7ba2faf`; the exchange is closed and its
mechanism reading is in the log. What stays live:

1. **⚑ ON THE WIRE, `run_to`'s `stopped` EVENT PRECEDES ITS REPLY**: it calls `emit_stopped` before it
   builds the result. A client that reads through to the reply and discards events (aurora's, and
   `Client::ok`'s) is correct and unaffected. **A client that consumed the reply and THEN waited for the
   halt event would block forever**: F-RESUME-STOP-RACE with the halves swapped, and exactly the shape a
   first breakpoint consumer reaches for. `run_to` blocks inside `dispatch`, so its reply is *produced by*
   the halt; `resume` only flips a flag and returns, which is the whole of that race.
2. **`"reached": run.predicate_fired`, the predicate's own verdict, NEVER the sink's, now has a named
   live consumer** (aurora's boot restore gates on `reached !== true`). `StopRecord::fired` means only
   *"something asked to stop"*, so reading it would report a target as reached because an unrelated
   `stopAfter` watch halted the run. The "simplification" that swaps them would break a real client
   **silently, in the direction that presents as a successful boot restore over a write window that never
   opened.** Booked here because a code comment is where a perishable rule goes to be read by nobody.

## ⚑ HUB RULING, 2026-09-07: HOW THE LENS COUNT IS REPORTED — **"PACKET MINUS FIXED", NEVER THE LEDGER'S OPEN COUNT**

⚑ **The hub's, under delegation; do not upgrade it to his.** It settles a defect this seat found while
correcting its own: **`docs/lens-findings.jsonl` is a CURATED SUBSET of the packet**, ~32 ids against 8
critical + 33 high + ~77 medium/low. A finding with no row was never in an open tally, so a shrinking open
count reads as a repo approaching clean when most of the packet was never enrolled.

**The rule, in force:** the number the owner gets is **the packet's totals minus the ledger's fixed rows**,
stated in those words, **never the ledger's open count alone**, and **the floor caveat travels with it**
(*"open in the ledger is a floor on what is open, not a count of it"*).

**Enrolment is HELD, and the reason is the owner's quota, not the merits** (he is at ~90% of weekly usage):
an enrolment parcel is bookkeeping that spends his budget to make a file carry a number the packet already
carries. **Enrol rows opportunistically — only when you next touch the ledger for a fix, as part of that
landing.**

⚑ **And the reporting lesson underneath it, which is this seat's own and cost a wrong number on his card:**
*"four criticals"* meant the packet's *"four things to act on first"* (C1, H2, C4, C3 — one H-numbered,
and **excluding C2 and C5**), and it reached his board reading as all five. **A phrase coined for a queue
row acquires a different meaning when it is lifted into a status line**, and nothing in either artifact
announces the shift. State counts by enumerating their members where the members are few.

## ⚑ OWNER RULING, 2026-09-07: THE LENS RITUAL GAINS A UX SEAT PAIR, AND **THIS LANE IS THE PILOT**

⚑ **RELAYED BY empyrean-c0, NOT WITNESSED HERE, and verified firsthand rather than adopted:** empyrean
**`6a12740`** is an ancestor of their `origin/main`, `--stat` shows a docs commit carrying a docs ruling,
and the owner's words are in the blob. Asked whether oracle's sweep had a UI/UX lens and told the roster
had none: *"I think it should have one right?"*, then *"I think we draft it and run on oracle, aurora,
and sigil for now"*.

**PILOT AGAINST `6a12740`, NOT `97cd725`.** The first revision is the ruling; the second carries this
lane's four pre-run gaps folded in, and running the pilot from the earlier text re-inherits every one of
them. Roster C = **UXa** (task walk: 3-5 newcomer jobs named in the charter, README only, every stall and
guess logged with a screenshot) and **UXb** (heuristic audit: every panel, control and message against a
fixed checklist). A ×2 opposed pair; convergence is the top finding class. It is a **late panel** on a
corpus that already has a packet, so it runs at a **NEW pin** and the packet is amended naming the pair
as late plus that pin, **never re-dated**.

**Seat rules that bind our charter**, transcribed rather than summarised: private Xvfb display with X11
forced and the screen size verified from inside; private socket; never the shared server, never his
display, never the emulator MCP. Every finding ships a screenshot or verbatim diagnostic text, and **a
clean task still ships a screenshot per step**, so a clean verdict is examinable rather than "nothing
found". Look/taste items are captures for the owner, not packet findings.

⚑ **A private Xvfb display satisfies this lane's flat no-window-while-his-player-may-be-live rule in
SUBSTANCE, not merely in spirit** — nothing reaches his screen and nothing touches his socket. Recorded
so a later session does not re-litigate it; banked at the hub the same way.

**The four gaps this lane found in the first draft, folded in at `6a12740` and attributed there.** Kept
here because the fourth is the reusable one: (1) *the charter names the surface* — oracle is **two**
windows, `oracle-frontend` (game) and `oracle-player` (debug tabs), and a task walked in one and judged
against the other is the conflation that already cost a parcel; (2) **failure to get a ROM in is a
FINDING, never BLOCKED**; (3) **pacing/smoothness/responsiveness are OUT OF SCOPE for Roster C** — a
virtual display has no vsync, so a "feels sluggish" reading there answers a different question while
looking like an answer ([[F-VSYNC-NEVER-MEASURED]]); (4) **"private socket" binds the CLIENT, not only
the instance.**
⚑ **(4) is banked because CHECKING IT REVERSED IT.** The draft of that gap said a private socket is a
trap here, reasoning from this lane's own `F-CHAIN-QUOTED` booking. Read before sending: our **server**
takes `--socket` and `$ORACLE_SOCKET` cleanly (`crates/oracle-aether/src/server.rs:66-78`), so the
isolation is one flag; the hazard is real but lives in the suite's **reference client**, whose resolver
commits on a directory test and reaches the shared path whatever the server was told. **A booking about
our own tree was about to be sent as a claim about a different component**, and only reading the source
separated them.

**OWED TO THE HUB AFTER THE RUN, both, and neither is optional:** the **landing SHA of the amended
packet**, and **one paragraph on what the seat brief STILL got wrong** — sigil runs the pair next and the
brief is the thing under test on this first run.

## ⚑ OWNER RULING, 2026-09-03: WHAT GETS A TAB IN THE DEBUG WINDOW (ORACLE-DEBUG-UI)

**Witnessed directly in session, not relayed.** Put to him as an assessment, answered *"That's fine I agree
with the assessment"*. It is the standing shape for every panel parcel; do not re-derive it.

**Default: a capability served on the bus is reachable in the window, not only from a tool.** That is his
standing "build out the tooling" directive arriving on the UI. But **a tab is not the right shape for all
of them**, and the split is the ruling:

* **Things you LOOK AT** (registers, memory, objects, breakpoints, profiler) **are tabs.**
* **Things you DO** (reset, press, spawn, write) **are NOT tabs.** They are controls inside a panel or an
  invoked command. A tab that is empty until used is a worse button. *(The spawn serve is the live example:
  its surface is clicking a spot in the Screen panel, not an `object_spawn` tab.)*
* **Things too expensive to show live** (`scanlines` is ~440 KB of JSON per frame) are **on demand**,
  never a docked tab quietly costing frames.

⚑ **And the half that is a correctness rule rather than taste: A PANEL MUST SHOW THE SAME ANSWER A TOOL
GETS.** ~~Prefer reading through the served surface over reaching into the emulator by a private route.~~

⚠ **THE REQUIREMENT IS HIS AND STANDS; THE MECHANISM WAS THIS SEAT'S AND IS WRONG, corrected 2026-09-03,
original struck rather than deleted.** He agreed to an assessment that contained my error, so the record has
to separate them. **"Route panels through the served surface" is the option this repo already considered and
REJECTED, with the reasoning in-tree the whole time:** **the primary source is the contract itself**, `empyrean:contract/protocol.md:238` (D15), not our
comment about it: *"An in-process GUI is a consumer of the same registry, not a second server. A debugger
or inspector view living in the player's own window **reads the method registry directly, in-process; it
does not open a socket to itself.** The one legitimate alternative — a GUI running out-of-process as an
ordinary Aether client…"*. So the contract does not merely reject the round-trip, it **prescribes** the
in-process read and names the only alternative. `pick.rs:649-655` is our in-tree echo of it, and our
`Host::pump` makes the rejected shape worse still: a click would enqueue and wait a frame to answer what
it can answer synchronously. *(Re-anchored 2026-09-03: this correction first cited the code comment, which
is the story rather than the artifact and is this repo's own bar.)*
**What makes parity true by construction is ONE IMPLEMENTATION UNDER TWO CONSUMERS plus a parity test**, not
a transport. `host.rs:439-440` already says so: panels draw from *"the same instruments its loop feeds and
the bus serves, so a local readout and a client's reply cannot disagree."*
**The line, from the parcel-2 design:** per-frame panel bodies read the shared derivation **directly**;
per-gesture **commands** go through a synchronous `Host::call`, so a click gets the tool's exact reply *and
its refusal*. That grants his correctness half in full and costs JSON per click, not per frame.
**Mirror he did not state and it follows from his own default:** a served capability that **changes what the
window does** must be *visible* in it; `hold` ORs a client's pad into the player's, so a disconnected client
can leave someone walking left forever with nothing on screen able to say why.

## ⚑ OWNER RULING, 2026-09-02T20:05:08Z, d-25 DOCK SHAPE: **option 3 `swap-toolkit`, NOT our recommendation**

⚑ **RELAYED (empyrean-c0, 2026-09-05), NOT WITNESSED HERE, verified firsthand at empyrean
`origin/main` `33ca3b7`:docs/OVERSEER.md:389.** Banked here 2026-09-05 because it had reached this lane
through relay only: our own `docs/decisions.jsonl` still carries d-25/d-26 with our `fixed-slots`
recommendation and no answer, and the contract defines no closed state for a card, so **nothing in this
tree recorded that he overruled us.**

**Rebuild the window on a real UI toolkit.** His words on the old shape: *"there are some nice things
about it but it's not like I fully designed it myself, just had some features (lenses) added in, which
wind up either not showing enough to make space or will show too mcuh and take up too mcuh spack. Was a
clean idea but just not good enough."*

⚑ **His lens verdict, *"a clean idea but just not good enough"*, RETIRES "lenses stay for what they
suit" from ORACLE-DEBUG-UI's goal.** Do not restate lenses as a live design direction.

**The three things the ruling said to answer BEFORE building are ALL ANSWERED AND BANKED ON `main`
(measured 2026-09-05, before an agent was spent re-asking them):** (1) **which toolkit**: `egui` 0.36 +
`eframe` + `egui_dock` 0.21 in `crates/oracle-player`, **eleven** real tabs in `Tab::ALL` (eight when
this was measured on 2026-09-05; `Planes`, `Spawn` and `Effects` have landed since — the count is stated
here in the present tense, so it is corrected rather than left as a dated figure); (2) **a measured
frame loop under it**: `docs/2026-09-02-toolkit-spike.md`, **0.22 ms median / 0.66 ms p99, ~1.3 % of a
16.67 ms frame**, with `docs/2026-09-02-player-pacing-design.md` putting the stall risk in *present*, not
compute; (3) **panels in a second toolkit-drawn window beside the existing player first**: true by
construction, `oracle-player` (egui) runs beside `oracle-frontend` (minifb).
**So the pre-build gate is CLOSED and this is build work, not a fresh decision card.** What remains of
item 3 is its own second half, *the player migrates later*, which is where `F-FRONTEND-PALETTE-BUS` and
`F-STATUS-CAVEAT-NOT-ON-STRIP` live.
⚑ **The reason this is written down rather than just acted on:** a queue row's justification ages like a
precedent narrative. This row asked for three answers that had existed for two days, and it is the second
time in three days that has happened here (`ATTR-RGB-LATCH` asked for a CR adopted four days earlier).
**Re-measure a row's premise before spending an agent on it, not after.**

**▶ RETIREMENT GATE ON THE minifb FRONTEND: hub ruling under delegation, 2026-09-05 (empyrean-c0;
theirs, do not upgrade it to his).** They ruled skip-the-card and verified our side independently before
ruling (the spike doc's 0.22/0.66 ms **and** 60.03 fps sustained over 75 s; `egui_dock` actually *used* in
`layout.rs` and `main.rs`, not merely pinned: behaviour, not presence). The condition rides with it:
**`oracle-frontend` is not retired until the migrated player shows 60 fps and audio pacing measured on the
REAL player under the toolkit, in the same form as the spike doc**, and the owner's window keeps working
across the switch.
⚠ **Cross-lane obligation, and it has a precedent behind it: aeon reloads into the owner's window BY
SOCKET, so tell aeon and the hub the day the binary name or socket path changes.** A wrong process name
has already cost a night of "window closed" reports. This is bar 14's consumer-set rule arriving on a
process identity rather than a wire key.

## Coordination (when peers are up; all optional to progress)

- **seraph** (DAW): **the first FILED DEMAND against the unserved-method list (2026-08-26), and it is
  why the cutover ruling works.** Their S2 verification gate builds side B entirely out of
  `emulator_vgm_start`/`stop` → `vgm2wav`, so **S2 as banked is NOT executable against the new core**.
  Their triage, taken as given: **`vgm_{start,status,stop}` is the one that matters** (realtime and
  foreground fine; it must be deterministic enough to capture twice and compare); **`audio_spectrum` is
  explicitly NOT wanted**: do not build it on their account; **channel masks are wanted at S3, not S2**.
  **Firing condition: S1 landing.** Deliberately not filed as a dated queue row (bar 18: the dependency is
  two packages away). Treat VGM as demand-ordered-with-a-condition: of the unserved set it is the only one
  with a named consumer, a named artifact and a stated trigger. Do not pre-build it; do not renumber it
  away. That these are unserved is **machine-enforced** (`schema_conformance.rs` pins
  `SCHEMATIZED_NOT_ADVERTISED` with `assert_eq!` on the whole sorted set; `engine.rs` advertises
  `"vgm": false`); the *reason*, the synth being `cfg`-gated out, is a reading, not re-derived.
  *(Grounds, anchors and the confidence-split correction: `OVERSEER-LOG.md`.)*
