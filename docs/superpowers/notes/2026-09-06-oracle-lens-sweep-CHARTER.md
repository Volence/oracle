# Oracle full-repo lens sweep — CHARTER and seat rules

**Pinned review SHA: `d3ca871a3a553efd793d6dce15bad5fb8194fb98`** (`main`, clean tree, in sync with
`origin/main` at pin time). Every seat works read-only against this revision.

Authority: owner, verbatim 2026-09-06T07:23:14Z — *"alright, I'd really like to finish up the
raster/parallax and then run a full lens suite on everything"* (empyrean `origin/main`,
`docs/OVERSEER-LOG.md:6251`), reaching this lane under his 2026-09-04T15:48:58Z condition *"sigil and
oracle can start up again whenever we want"*. Trigger: EFFECTS-W1 closed (aeon `dad5c395`, verified an
ancestor of their `origin/master`). Protocol: aeon `61f22403:docs/superpowers/LENS_PROTOCOL.md`.

## Step 0 — standing findings

**No prior full-repo packet exists in this repo**, so this is a fresh panel rather than the protocol's
two-job re-verification. The one prior lens artifact is narrow: `docs/2026-08-17-player-s3-lenses.md`
(player S3 only), whose still-open follow-ups are `F-PALETTE-SCROLL`, `F-PALETTE-HINT`, the non-gamepad
deadzone literal at `main.rs:389`, and Spec-§7 residue. **Seats must not re-find those as new.**

There is **no `docs/DEFERRED_WORK.md` in this repo** and no DO-NOT-RE-LITIGATE section. This lane books
findings in `docs/lane-status.json`'s queue, `docs/OVERSEER.md`'s follow-up register, and
`docs/2026-09-06-queue-detail.md`. Reconciliation goes there, not to a file that does not exist.

## Corpus

All Rust under `crates/`: **196 files, 202,516 lines**, six crates — `oracle-core` (80,406),
`oracle-aether` (46,889), `oracle-player` (38,486), `oracle-frontend` (29,757), `oracle-replay` (6,009),
`oracle-panels-spike` (969).

**Out of scope, and therefore UNEXAMINED RATHER THAN CLEARED** — write that phrase into any finding that
touches them: `vendor/` (third-party test ROMs and fixtures), `oracle-old/` (a different repo, legacy
C++, reference only), `tools/*.py` except where a seat's own charter names one, and all `docs/`
prose except where a seat is chartered to check a claim against code.

## ⚑ ROSTER DEVIATION, stated rather than made quietly

The protocol says **do not improvise a smaller panel** — each ratified seat earned its keep. This sweep
runs the **full seat count**, but **six Roster B seats are SUBSTITUTED, not dropped**, because Roster B
was validated on sigil (an assembler) and six of its seats name subsystems an emulator does not have.
Measured at the pin, with working controls (`vdp` 84 files, `m68k|68000` 70 files, so the search sees
what is there): **`codegen` 0 files, `linker` 0 files, `relaxation` 0 files, `comptime` 1 incidental.**

**Why substitution and not omission, and why not a literal run:** a seat pointed at a subsystem that does
not exist returns *"nothing found"*, which is **indistinguishable from "examined and clean"** — this
repo's signature failure mode, applied to the sweep itself. A literal Roster B run would have produced
six clean-looking verdicts about nothing. Each substitution preserves the seat's *property class*
(correctness-per-target, layout integrity, inter-layer contract, iterative numerics, evaluator
semantics) and points it at the corpus where that property actually lives.

| Roster B seat | property class | substituted here |
|---|---|---|
| CGa / CGb | correctness per target | **CPU-A** 68000 semantics/flags · **CPU-B** Z80 + bus arbitration |
| LINK | placement/layout integrity | **STATE** save/restore, state-hash, checkpoint layout |
| IR | contract between passes | **PROTO** Aether method/schema contract across layers |
| RELAX | iterative/fixed-point numerics | **TIMING** clock, scheduling, sub-frame accounting |
| COMPTIME | evaluator semantics | **VDP** VDP/render semantics |

Unchanged from Roster B: GATE, TEST, FUZZ, SAFE, ARCH, ERR, CACHE, P1a, P1b, P2. From Roster A: A2, B1,
B2×2, V. **V is doubled** (Va/Vb) on this lane's own evidence: vacuity has been the highest-yield defect
class here all week.

## Seat rules — every seat, verbatim

1. **READ-ONLY.** No edits, no commits, no branches, no `git add`.
2. **NO EMULATOR MCP TOOLS EVER** (`mcp__oracle__*`) — they deadlock from background agents. Anything
   wanting runtime confirmation is **TAGGED** for the controller's foreground follow-up.
3. **DO NOT LAUNCH ANY WINDOW OR GUI.** The owner's own player window is live on his display right now
   (pid 2349009) and this lane has a flat rule against a second one. A headless framebuffer answers a
   different question while looking like an answer.
4. **DO NOT run `cargo test --workspace` or a full release build.** The machine is shared, the owner is
   actively using a window on it, other lanes run agents, and this repo has a booked row
   (`F-HANDSHAKE-LOAD-TIMEOUT`) that fails **only under load**. A targeted `cargo test -p <crate>
   <filter>` is fine when a finding needs it. If you need more, TAG it.
5. **Every claim needs a derivation the controller can redo in seconds** — `file:line`, a grep, or two
   lines of reasoning. **"Verified clean, and here is what I re-derived" is a welcome result. Inventing
   findings is the cardinal sin.**
6. **Report most-severe and most-uncertain first.** Keep verified separate from proposed. Close with
   **what you could NOT check and which instrument would**.
7. **BLOCKED** (missing context, unreadable pin, a charter that forces a worse answer): STOP on that
   item, say exactly why, continue with the rest. Never degrade the charter silently.
8. Rank each finding **CRITICAL / HIGH / MEDIUM** with a **LIVE-today vs latent-until-X** reachability tag.

## Three rules from today, in every seat brief

- **A finding resting on a harness RE-RUNS that harness and reports its state beside the finding.** A
  harness that has not run since it was written is not evidence.
- **Ask "could this sample have FAILED?", not "did it pass?"** A control that cannot fail measures
  nothing. This lane has hit that six-plus times this week, including twice today.
- **A red-first mutation must go red ON THE ROW UNDER TEST, not merely go red.** Landed evidence
  (`fa8fcbd`): two prescribed mutations went red on the *schema validator* because the fragment marked
  both fields required, so the proof would have looked rigorous and demonstrated nothing about the row
  being commissioned. A mutation caught by a different guard produces an identical artifact.

## Aftermath

**NO FIXES DURING THE SWEEP.** Seats stay read-only precisely so the report stays honest. Findings land
as rows; fixes are separate, owner-gated parcels.

---

## ⚑ LAUNCH RECORD — the panel was STAGED, not batched, and five seats are OWED

The protocol says *launch all seats in one batched message so they run concurrently.* **That was
attempted and the environment refused it**: this harness caps concurrent subagents at 20.

**17 seats launched** (first two batches complete, plus A2): CPU-A, CPU-B, GATE, TEST, FUZZ, SAFE,
TIMING, STATE, PROTO, ARCH, ERR, VDP, CACHE, P1a, P1b, P2, A2.

**5 seats REFUSED by the cap and OWED — they launch as capacity frees:**
**B1** (construct/idiom), **B2a** (duplication, code-first), **B2b** (duplication, data-first),
**Va** (vacuity, guard-first), **Vb** (vacuity, claim-first).

⚑ **These are NOT dropped, and the distinction is the whole point of writing this down.** Silently
running 17 of 22 and publishing the packet would be precisely the *improvised smaller panel* the
protocol forbids — and it would look identical to a complete sweep from the outside. **The packet does
not ship until all 22 have run, or until any seat that did not run is named in it as UNEXAMINED.**

Note which seats the cap took: **both vacuity seats and both duplication seats.** Vacuity is the
highest-yield class in this repo and duplication is the one with a live open finding. Had this been
allowed to pass unrecorded, the sweep would have skipped exactly the seats most likely to find
something, and the packet would have read as clean.
