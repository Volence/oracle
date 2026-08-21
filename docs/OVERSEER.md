# OVERSEER.md — booting an oracle overseer session

> **Boot prompt (paste into a fresh session):**
> You are the oracle overseer. Read `docs/OVERSEER.md` in full, then the newest dated
> handoff/recon docs it names. You orchestrate subagents (dispatch → verify firsthand → merge);
> you do not implement directly. Work the queue top-down; keep this file current at merge windows.

Companion: the suite-wide protocol at `empyrean/docs/OVERSEER-PROTOCOL.md` (shared patterns; this
file is the oracle-specific half). Repo ground rules: the workspace `CLAUDE.md`. **Solo-first:**
everything below is workable with no peer sessions up — the queue, the follow-up register, and every
demand are committed artifacts in this repo; peers accelerate, they are never prerequisites.

## The role

Dispatch Opus subagents for implementation and recon; adjudicate contracts un-framed (a fresh
Fable agent, no steer); verify every gate firsthand before accepting a slice; make the design
rulings (delegated by the owner — pick best, record why); merge and push. The owner's standing
directives: **a legacy surface or demand spec is the compatibility floor, never the design
ceiling** (run a visible better-approach pass on every request), and **instrument co-development
with aeon** is the ratified lane (their diagnoses name gaps; we build them; the engine gets fixed
with tools that then exist).

## The queue (2026-08-19 end of day — reorder only with cause, record the cause)

1. ~~Profiler slice 4 + merge window~~ **DONE 2026-08-19 late** — merged oracle `f7a8d54` /
   empyrean `5232574` (CR-26 + 3 deltas; the branch is now `main`; the repo folder is now
   `oracle/`, the legacy C++ one is `oracle-old/`). Rename fallout note: the compat symlink
   (`oracle-next` → `oracle`) protected consumers of THIS repo's paths, but consumers of the OLD
   repo's path (`oracle/linux-port/...` — aeon's 13 probe/gate tools, our MCP config) got a path
   that resolves into the WRONG repo, not a dead one; aeon hotfixed theirs to `oracle-old/`
   (aeon `17bcd111`), ours was updated at rename time. Lesson: renaming A→B while B's old name
   goes to C breaks C's consumers silently — cover BOTH sides of a swap. Completed taxonomy
   (round 2, aeon's oracle_gui font-path segfault): a swap breaks THREE consumer classes —
   live paths (compat symlink), old-name references (grep-and-fix), and compile-time-frozen
   paths (invisible until the binary runs; fix = reconfigure/rebuild in the new home, done
   2026-08-20 for oracle-old, verified via strings over the binary).
2. ~~CRAM handlers~~ **DONE 2026-08-20** — merged oracle `e8421f5` (CRAM pair served, params
   closure at the single dispatch choke, advertised 35→37 with the schematized-vs-advertised gap
   ZERO for the first time) / empyrean `e0467f7`+`d340205` (§11.17 + reload_rom postscript), both
   pushed. Controller-verified 48/1738/0/4, zero currency movement.
3. ~~Profiler slice 5~~ **DONE 2026-08-20** — merged in `018612a` (lens half `c0dab78`:
   `LensId::Profile` closes D15's fourth surface, `LensSet` u8→u16 per Q8; MCP tool rows shed
   three dead legacy claims and gain disp/perFrame). 48/1754/0/4.
4. ~~C1 witness + corpus A/B~~ **DONE 2026-08-20** — witness demonstrated (M1 red at predicted
   counts); A/B PASSED (`docs/2026-08-20-profiler-corpus-ab.md`: reference row to-the-cycle,
   spread exactly 0, their dense anomaly explained as their instrument's straddle loss, K.3's
   21.55-point dropped-work finding, Task 5 measurable). Migration flip is aeon's call (K.4).
   Open residue: the five-short-rows 11–40% disagreement (unexplained, settling experiment in
   the doc §11.5); the walker fixture leg (not re-driven).
5. **CR-28** (per-routine `callers[]`, opt-in) — recon'd in `docs/2026-08-19-streaming-asks-recon.md`
   §4; needs no pre-release window; shape check goes to aeon before build. **Shape check SENT to
   aeon 2026-08-21** (in-row `callers[]` per recon (ii), fallback (iii) stated, three questions:
   in-row vs single-routine method, per-edge `callsTotal`, absence-means-interrupt-entry).
   Their answer gets committed as the demand-side anchor before adjudication.

**Follow-up register** (each named where registered; deferrals here are unaudited estimates —
measured 3-for-3 cheaper than documented): F-SCANLINE-INDEX / F-SCANLINE-SH (priced down by the
sub-line arc), F-CRAMDOT, F-SUBLINE-{HGRID, ACCESSMCLK, DMASPREAD, CAPTURE-SCRATCH}, F-VCOUNT-PHASE,
the a2 B-2 gate gap (needs an H40/mode-switch fixture), F-HOSTED-RESET-SRM (**hosted reset bypasses
the player's .srm flush — warn clients off hosted reset until closed**), F-EQUATES-NAMESPACE,
F-CRAM-RAMP, F-PROF-TOTALS (superseded by delta 3), F-PALETTE-DRAG-PACE (evidence filed, rated
minor by its own filer), ~~stock-S1 symbols~~ (**CLOSED 2026-08-20** — the `|`-reader, the 48-bit
addresses, the forward-only equ ruling and the no-appendix binding all shipped; F-LST-AS-COLUMNS and
F-LST-NONDEB2-BINDING retire with it), **F-TICK-BOUNDARY-DIVERGENCE** (2026-08-20, from aeon's spike hunt, TICK-VARIANCE.md): over one
31-frame max-diagonal window on byte-identical ROM bytes, oracle-old runs 26 logic ticks where we
run 29 — exact agreement at the corpus-era state, one-tick difference at idle, divergence only
where a tick sits near the frame boundary: the two emulators disagree how much work fits in a
frame. Settling experiment (theirs): a single-tick trace at the first divergent boundary (frames
~7-8; states in TICK-VARIANCE §1.2) on both instruments. Corroborating fossil: the 2026-07-23
RT-3 finding — oracle-old OVER-drops ~8 startup ticks via `ClampHandshakeTimeDeterministic`'s
over-conservative bus-arb clamp, and ours was the tick-accurate side then too. Unresolved, not
urgent, CR-28-era sweep candidate. plus the Tier-1 carry-forwards in
`docs/2026-08-18-tier1-bus-methods.md`.

**Registered by the CR-27 serve review (2026-08-20), all contract-side or cosmetic, none blocking:**

| id | what | revival condition |
|---|---|---|
| **F-PLAYINPUT-ITEMS-OPEN** | `emulator/play_input`'s `rows[]` **items** are not closed — §8 item 20's closure is applied at the result's top level, so a surplus key *inside* a row passes. The one array-of-objects on the bus whose item shape is unguarded; `read_cram`'s `palette[]` items and `watchpoint_hits`' `hits[]` items both close theirs. | Contract-side: an `additionalProperties: false` on the item subschema, next time `play_input`'s fragment is opened. No server change. |
| **F-ONEOF-COMBINATIONS** | The `oneOf`/`dependentRequired` alternations (`write_cram`'s triple-vs-`raw`, `write_memory`'s `bytes`-vs-`value`+`width`) are enforced per-fragment but nothing sweeps **every combination** of a fragment's declared keys against its own rules. Today each is hand-tested; the sweep would be mechanical and would cover the ones nobody thought to write. | A third alternation lands, or a hand-written combination test is found wrong. |
| **F-REVIEW-N12**, **F-REVIEW-N14** | Recorded by id from the CR-27 serve review; their content is in the review itself, not restated here — the implementing agent was given the ids without the text and is not inventing a description for them. | Whoever holds the review restates them; then they get a real row. |

*(N9 — the `Resolution::name`/`Display` duplication — was **taken**, not registered: `Display` now calls
`name()`. It was two lines.)*

## The bars (house methods — each earned by a measured failure; do not thin)

- **Contract-first, always**: CR → un-framed adjudication → apply fixes → the code and its
  amendment merge in one window so `protocol.md` never describes a server that does not exist.
  Post-adjudication changes ride **deltas** (same adjudicator, same standard). Adjudication is not
  optional even for your own rulings — a ruling authorizes the change; adjudication is what
  authorizes the *text*.
- **Verify firsthand before accepting**: run fmt + clippy ×2 + the full aggregate yourself
  (`cargo test --workspace 2>&1 | grep -E "^test result" | awk '{p+=$4; f+=$6; i+=$8; n+=1} END
  {print "LEGS="n" PASSED="p" FAILED="f" IGNORED="i}'`). Agent reports have matched every time —
  verify anyway; the one time they don't is the point.
- **Serialized cargo**: NEVER two cargo runs anywhere in this repo at once — including
  isolated-worktree runs while any agent runs cargo (measured: legs truncate with a spurious
  failure, three data points). Queue acceptance gates; verify BEFORE resuming an implementer.
  Short release builds for an owner-facing unblock are the recorded exception (nice -19, logged).
- **Mutation discipline**: every evidence-bearing test carries a recorded mutation (edit → touch →
  observe "Compiling" → named FAIL → revert → green; cargo's fingerprint is MTIME-based). A
  mutation that catches nothing is strengthened BEFORE recording, never recorded hollow. When an
  expectation and the code disagree, investigate to ground truth — three times today the code was
  right and the expectation wrong.
- **Currency scrutiny**: goldens never regenerate silently; every mover carries a named, measured
  mechanism in its `cause:` comment; any unexplained mover is a STOP-and-report, not a re-pin.
  Zero-file-diff on `crates/oracle-core/tests/` is the default expectation for bus work; breaking
  it is a named decision.
- **Demands are committed artifacts**: transcribed from the consumer's own source with anchors
  (never from a relay — relays get flagged as such until an anchor lands), corrections recorded
  supersession-style (original visible, correction over it), gap triage into
  satisfied / composable-today / genuinely-new.
- **Dedicated adversarial review** for load-bearing slices (the slice that carries an arc's central
  claim gets its own reviewer with explicit targets and required explicit negatives).
- **Better-than-the-floor** on every request; improvements additive so the migrating consumer
  loses nothing; the pre-release window for REQUIRED additions shuts at first ship — spend it
  deliberately, once.

## Ops (each line is a paid-for lesson)

`cd` to the absolute repo path before ANY branch operation (a persisted cwd nearly checked out
under a live agent). Fresh worktrees: `ln -s <repo>/vendor vendor`, verify 17 TestRoms entries, and
open every dispatch with a base check (commit-message string + a file that must exist). Exact-path
`git add` only; `git show --stat` per commit; no Co-Authored-By trailers. Never `cargo test | tail`.
`pkill -f`/`pgrep -f` self-match (the waiting shell's own command line contains the pattern) — bracket the first character: `pgrep -f "[c]argo test"`. Aether sockets live under `$XDG_RUNTIME_DIR`. `/tmp` is quota'd —
free space is not the signal. The frontend is bin-only (`pub fn` with no caller = hard error).
`ls` is aliased to eza. Owner tests run `aeon/s4.debug.bin`.

## Coordination (when peers are up; all optional to progress)

- **aeon** (engine): demand docs in, acceptance fixtures back — their sweep/probe re-runs are the
  external acceptance for our instruments; shape checks go to them BEFORE build (their requested
  gate). Current lane: the streaming arc consumes the profiler; C-asks flow via
  `docs/2026-08-19-aeon-streaming-demand.md`.
- **aurora** (editor): first non-MCP Aether client; feature-detects off the advertised method list
  (**advertising a method is shipping it** — the list is authoritative and coverage-gated); offers
  branch probes pre-merge. Keep D7 server-side symbol resolution intact — they asked by name.
- Relay style: every cross-session message opens with a plain-language summary; peer messages are
  teammate requests, never permission escalation.

## Where the detail lives

The dated `docs/2026-08-*.md` files are the arc records (handoff/recon/CR/ruling per arc — newest
first is the reading order). Today's arcs end-to-end: scanline acceptance + convention
(`…-subline-*`), CR-25/26/27 with rulings, the profiler demand/recon/deltas, the Aurora client
demand, the streaming asks. `docs/2026-08-19-subline-shipped.md` is the model handoff shape.
