# OVERSEER.md: booting an oracle overseer session

> **Boot prompt (paste into a fresh session):**
> You are the oracle overseer. Read `docs/OVERSEER.md` in full, then the newest dated
> handoff/recon docs it names. You orchestrate subagents (dispatch → verify firsthand → merge);
> you do not implement directly. Work the queue top-down; keep this file current at merge windows.

Companion: the suite-wide protocol at `empyrean/docs/OVERSEER-PROTOCOL.md` (shared patterns; this
file is the oracle-specific half). Repo ground rules: the workspace `CLAUDE.md`. **Solo-first:**
everything below is workable with no peer sessions up: the queue, the follow-up register, and every
demand are committed artifacts in this repo; peers accelerate, they are never prerequisites.

## The role

Dispatch Opus subagents for implementation and recon; adjudicate contracts un-framed (a fresh
Fable agent, no steer). **The seat is on HOLD, and that is a live owner ruling, not this seat's
call**: see owner ruling 2 below, and the substituted-reviewer rule under the 2026-08-27 hub ruling
for how adjudications run meanwhile. *(The 2026-08-22 provenance audit of the seat's original
ratification is closed and moot by that ruling's own words; moved verbatim to `OVERSEER-LOG.md`
2026-09-03. The standing rule it produced, never record an approval whose granting act you have
not seen, is live, and lives in the bars, now in `docs/OVERSEER-REFERENCE.md`.)* Verify every gate firsthand before accepting a slice;
make the design rulings (delegated by the owner: pick best, record why); merge and push. The owner's standing
directives: **a legacy surface or demand spec is the compatibility floor, never the design
ceiling** (run a visible better-approach pass on every request), and **instrument co-development
with aeon** is the ratified lane (their diagnoses name gaps; we build them; the engine gets fixed
with tools that then exist).

## The boot read is bounded (100,000 B, gated)

Closed history is in **`docs/OVERSEER-LOG.md`**, not read at boot. Governing rule:
`origin/main:docs/OVERSEER-PROTOCOL.md`. **A live ruling goes here, never only in the log.**

**Split by WHEN a rule is read, not by size** *(owner, 2026-09-04T15:38:47Z: one call for all six
lanes; the bound stays at 100,000 B and is never raised)*. This file is what you need to act **at
boot**. The house bars and the ops lessons live in **`docs/OVERSEER-REFERENCE.md`**. Open it at
the moment it applies: **before dispatching** a wave of agents, **before reviewing** returned work,
and **before landing**. It is not part of the boot read.
**Nobody hand-trims for the bound. Do not take a headroom figure from this paragraph — MEASURE it**
(`wc -c docs/OVERSEER.md` against 100,000). A previous revision of this line asserted "so there is real
headroom" and was still asserting it at 99,578 B with 422 B left: a measurement written in the present
tense, in the one section whose job is to warn about exactly that. Cuts so far: 2026-09-06 (99,798 →
87,2xx B with its repair pass), 2026-09-09 morning (99,578 → 93,149 B, three closed blocks) and
2026-09-09 evening (98,275 B / 1,142 lines; 8 closed blocks to the
log, 7 ops lessons to the reference) and **2026-09-09 second evening cut (98,469 B / 1,147 lines →
90,463 B / 1,052 lines; 5 sections, 2 to the log and 3 to the reference)**. ⚑ **The BYTE bound is met
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

## The queue (2026-08-19 end of day; reorder only with cause, record the cause)

*(Items 1-7 are closed and moved to the log. Item 8 keeps its live tail below; its closed sub-arcs
moved with them.)*

8. **▶ OPEN: THE ACCEPTANCE CONTRACT.** The definite list of what the successor must serve before it
   replaces the legacy C++ server. **Re-derive the membership, never transcribe it**: the machine-
   enforced source is `SCHEMATIZED_NOT_ADVERTISED` in `crates/oracle-aether/tests/schema_conformance.rs`,
   asserted as a whole sorted set, so it cannot drift silently. Board row `ACCEPT-16`. The arc's closed
   history (survey, CR-A, trio, CR-B) is in the log.

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
   **FOREGROUND runtime follow-ups: ALL FOUR CLOSED 2026-09-04** (three stale, one never ours), together with
   `step`'s frame-budget shortfall. The measurement, and the 2026-08-22 runtime attempt correctly ABANDONED
   rather than deferred, are in `OVERSEER-LOG.md`; the two live rules they produced are in the register below.
   **AEON OBLIGATION: SCOPE WAS WRONG, and the correction makes it bigger.** Item 7 recorded it
   as a dated heads-up before serving `emulator/wait_for_break`, because their gates send
   `timeout_ms`. **The survey found it covers THREE methods, not one, and I verified it firsthand
   at `origin/master` (not their working tree):** ⚠ **STALE AS OF 2026-09-04 AND IT COST A MIS-RANKING.
   RE-MEASURED AT THEIR `origin/master`: `raster_source_gate.py` has ZERO `wait_for_break` hits, and
   `snapshot_poison_gate.py`'s single hit is a COMMENT saying `emulator/run_to` replaced the arm/resume/wait
   triple.** The live call sites are `tools/evict_witness.py`, `tools/parallax_hscroll_probe.py`,
   `tools/raster_frame_epoch_probe.py` and the `aether_instance.py` client seam, none in the effects-gate
   lane. The original text below was true when written and is kept because a session that cites it must see
   that a verified-firsthand booking about a peer's tree still expired: ~~both scripts run an **arm → wait →
   clear** flow: `raster_source_gate.py:161/168/173` and `snapshot_poison_gate.py:62/64/68` call~~
   `emulator/breakpoint_add {addr}` → `emulator/wait_for_break {timeout_ms}` →
   `emulator/breakpoint_clear {all:true}`.
   **Consequence, and it is the load-bearing one: the migration CANNOT be piecemeal.** Serving
   `wait_for_break` alone would leave their flow with nothing to arm, so `wait_for_break` and the
   breakpoint trio ship as ONE parcel or the notice is worthless. The `timeout_ms` spelling was
   never the whole exposure; it was the part visible from a param grep.
   **This also gives the obligation a live reader BEFORE any date exists.** Their call sites bet on
   a specific breakpoint shape (`{addr: "0x…"}` to arm, `{all: true}` to clear, i.e. **address-
   keyed, no handles**), and **CR-A (D-13) is about to decide exactly that handle discipline.**
   Their input window is *now, before adjudication*, not when we ship. Note also
   `raster_source_gate.py:33`: under `deterministic=True` the legacy server answers `breakpoint_add`
   with a "det-mode stop" behaviour, a documented interaction our fragments say nothing about.
   The **date** still waits on the survey's pricing of that parcel; the **design consultation**
   does not, and holding it until a date existed would have consulted them after the ruling.
   If this session ends first, **the next one owes both**.


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

**Registered 2026-09-04, from reading the ADOPTED §11.33 text instead of the relay's summary of it:**

- **▶ CLOSED 2026-09-04: `emulator/step` did not enforce either of `count`'s bounds while its own comment claimed it transcribed them; handler, comment and test were mutually consistent and all three wrong. Moved whole to `OVERSEER-LOG.md` 2026-09-05 (boot-read bound). The live rule it produced is the entry below.**
- **⚑ AND IT IS THE FIRST DEMONSTRATED INSTANCE OF A BLINDNESS THIS FILE HAD ONLY PROPOSED.** The
  acceptance section carries *"a proposed **error-surface gate**: since no fragment declares error
  conditions, a suite validating only replies is blind to every error obligation."* **This is that, with a
  measurement attached.** A params fragment describes what a conformant CLIENT sends; **a server's duty to
  REFUSE what falls outside it is behaviour a document schema structurally cannot see.** Our conformance
  suite is green, the fragment is correctly vendored, the bound is correctly written, and the server has
  ignored it for ten days: **every artifact healthy, the obligation unmet.**
  **The gate is no longer a proposal looking for a justification; it has a demonstrated defect it would have
  caught.** Price it against this instance when it is picked up, and do not let it be re-argued from first
  principles: the argument is now an observation.

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

- **▶ F-THREE-MASKED-RENDERERS, the agent's finding and worth a row.** `Engine::framebuffer`,
  `Machine::render_masked` and `oracle-frontend::blit_masked` are **three implementations of one masked
  picture across three crates**, agreeing today with **nothing asserting they must**. Same class as
  `sprite_tile_at` before it moved into `oracle-core`, and the same answer applies. Not urgent; it becomes
  urgent the moment one of them is edited.

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

- **F-TMP-RESIDUE: DID NOT REPRODUCE** — 9,675 `/tmp/oracle_config_save_load_*` dirs relayed, **four** measured here minutes later, the name attributable to nothing in the suite, and nothing available could distinguish a wrong count from a reap between the two reads. Recorded as a caught relay rather than a finding, and not passed to the hub as a shared-machine hazard. Moved whole to `OVERSEER-LOG.md` 2026-09-06.

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

## ⚑ OWNER RULING, 2026-09-02T18:20:42Z: CUT THE CEREMONY. **IT OUTRANKS EVERY BAR IN THIS FILE.**

Verified at empyrean **`90554f2`**. ⚠ **Relayed here 09-02 and never banked: zero occurrences in this
file or the log until 09-03, so the 09-03 session spent hours on apparatus it forbids. The absence is the
failure.**

Owner, asked *"did we do something beurocratic to slow things down?"*: *"Yes please cut anyything that's
arbitrarily slowing us down without like an actual good reason please … as long as it's correct and stuff
and hitting our goal, that should be what we mainly care about."*

⚑ **THE SCOPE CLAUSE BELOW IS A LANE/HUB INFERENCE, NOT HIS WORDS, AND ITS CONDITION WAS MET 2026-09-06.**
His quote carries **no expiry and no project scope**; the "few weeks on this project" clause is his MOTIVE.
Empyrean labels the expiry as *the hub's application*; this file carried it in the register reserved for his
verbatim text — the seraph-hold defect, second file, opposite direction.
⚑ **RULED BY THE HUB 2026-09-10 under standing delegation (empyrean `origin/main`), overturnable by him on
read-back. (a)** The boot-doc-growth prohibition **has lapsed**, by the hub's terms not his. **(b)** It does
**not lift into permission**: the undated preference stands, so a new bar is now **governed by his test** —
*does it arbitrarily slow us down without an actual good reason* — rather than forbidden. **(c) Each hold that
rested on it is RE-DECIDED ON ITS OWN MERITS**, neither auto-revived nor auto-kept. Applied below:
`SCHEMA-DRIFT-NIGHTLY` survives (merits argued, only the blocker outlived them); `F-CITATION-LINT` does not
(its own text says the hold was *the moratorium, NOT the merits*, so there is nothing to inherit).
⚑ **CORRECTION ACCEPTED, against this seat:** I proposed treating it as in force *because in-force is what
every lane observes*. **Circular** — universal observance of a lapsed rule is the SYMPTOM, not evidence
against it. **When a rule's only remaining support is that everyone still obeys it, that is the finding.**
⚑ **AND I THEN COMMITTED THE MORNING'S OWN LESSON:** fixed this at its canonical site and left its
paraphrases standing one screen away (two revival conditions, both firing on the lapse). **A claim repaired
at its canonical site leaves its paraphrases standing** — I banked that at 08:1xZ and broke it at 09:0xZ.

*Scoped, per that inference, to EFFECTS-W1:*
* **No new process bars, no rulings about rules, no boot-doc growth.** New bars go to
  `docs/OVERSEER-PENDING-BARS.md` PARKED, not into force. The protocol pass waits.
* **A correction is ONE LINE in the lane log. No story.**
* **DoD items and the bug tier only**: no cross-lane audits, no instrument or ledger work, no re-measuring
  a peer's numbers, unless it blocks a DoD item or ships wrong output.
* **Status files, decision cards and lane logs are written once in the accepted shape and not polished.**
* **The boot-read gate stays, but nobody hand-trims for it**: over the bound, move history out in ONE cut
  and carry on.
* **"Correct" is unchanged:** a landing still builds, passes the lane's own tests, and shows on screen or
  in a witness. What is cut is certifying things that are not the feature, and record-keeping about the
  record-keeping.

## ⚑ OWNER RULING, 2026-09-03T05:21:01Z: **REPORT TO THE HUB WHENEVER YOU FINISH OR STOP**

⚑ **RELAYED BY empyrean-01, NOT WITNESSED BY THIS LANE**: same flag, same reason, as the relayed
rulings below. **Verified firsthand rather than taken on the relay's word**, which is what makes it
usable: empyrean **`f04afe3`** is an ancestor of their `origin/main`, `--stat` shows it is a docs commit
carrying a docs ruling (so its SHA class matches what it anchors), and the owner's words are present in
the blob at that revision. His words: *"tell the agents any time theyy finish work or stop to report to
you please, loosk like aeon's stopped right now"*.

**Standing, every lane: a landing, a boundary, a block, an owner question, or a dispatched agent
returning (anything that leaves nothing running) gets ONE message to the hub saying what landed (SHA
emitted from git output, never typed) or why you stopped, and what you need.** Going quiet with nothing
running is the state he named.

⚠ **This does NOT license the aggregate waste bar 18 exists for.** The trigger is *finishing or
stopping*, not *changing something*: a pin, a correction, or an interesting finding still needs a named
reader before it is sent. The two rules compose: report your own state unconditionally; relay a *fact* to
a peer only when you can name their dependency on it.

## ⚑ OWNER RULING, 2026-09-09: **SAY WHEN YOU NEED A CONTEXT CLEAR.** The sibling of the rule above

⚑ **RELAYED BY empyrean-cd, NOT WITNESSED BY THIS LANE**, and flagged per this repo's own rule; the owner's
words as transcribed: *"Remind the agents to let us know when they need a clear."* Banked here rather than
only in the log because it changes what a session does at a boundary. **Verify the granting act at
empyrean's `origin/main` when convenient and replace this flag with the confirmation; do not delete it.**

**The rule, all lanes: do not silently run down to a compaction or drift through one.** When a clear would
help, say so in the same breath as your status — **in the message to the hub AND in `lane-status.json`'s
`awaiting`** — together with **what a fresh session needs to resume.**

⚑ **And the resumption anchor is a FILE AT A COMMITTED SHA, never a summary.** That is the whole of why
this is a rule and not a courtesy: a summary is written by the session that is about to stop, is the one
artifact its successor cannot check, and is exactly the shape bar 20 names — a claim that lives only in a
message, with no reader who can meet the contradiction. A SHA the successor can `git show` is checkable;
*"I was in the middle of the lens highs"* is not.

**It binds this lane harder than most, because this lane runs several seats at once**: an overseer's own
context is the one thing not banked in the repo, and every dispatched agent's brief was composed from it.

⚑ **The honest failure mode to guard against is the opposite of the obvious one.** The risk is not asking
for a clear too late out of stubbornness; it is that **a session near its limit is the least able to judge
that it is** — the same reason `updatedAt` must come from the clock rather than from your own sense of the
time. So report a *measurement* rather than a feeling, and let the owner decide.

⚠ **AMENDED WITHIN THE HOUR, TWICE, AND THE SECOND CORRECTION IS AGAINST THIS SEAT.** The first form said
to report *"what fraction of the window is gone"*. **There is no such measurement available here, and I
reported one anyway: "about 9 percent".** Two separate defects, and the second is the one that generalises:

1. **(aurora's catch) The counter is not monotonic.** They watched `total_tokens` RESET UPWARD mid-session,
   ~13.65M back to 15.0M, and refused to derive a percentage from it. Correct: a percentage off a
   non-monotonic counter has a confident shape and no meaning, **which is the exact artifact this rule
   exists to prevent, so producing one to satisfy the rule defeats the rule.**
2. **(this seat's, worse, and it holds even if the counter were perfect) IT MEASURES THE WRONG QUANTITY.**
   `total_tokens` is a session **budget remaining**. *"How full is my context window"* is a different
   question, and budget spent is not window occupancy — long tool output is persisted to files rather than
   held, and the window is summarised on its own schedule. **I took a budget figure, renamed it occupancy,
   and divided.** Name-is-not-behaviour applied to a counter.

**So the rule is REWRITTEN rather than exempted, because the thing that actually decides the question is
measurable and neither number was it.** *"Would a clear cost anything?"* is not a question about occupancy
at all — it is **"is anything load-bearing living only in my head?"**, which is exactly what `atBoundary`
already encodes and which is fully checkable: uncommitted work, an agent holding a branch, a ruling not yet
in `OVERSEER.md`, a decision taken and not written down.

**Report, in this order:** (a) **what is unbanked** — measured, itemised, and the half that actually
decides it; (b) a budget figure **only if you have one, named as budget and never as occupancy**, with
aurora's caveat that it has been seen to move upward; (c) **if the number is not available, say so, and
never substitute the feeling it would have replaced.** Loud on unmeasurable, in the one place where the
temptation is to produce a plausible number because two peers just did.

## ⚑ FOUR OWNER RULINGS, 2026-08-22: **RELAYED, NOT WITNESSED BY THIS LANE**

Reached us via empyrean-73, quoting the owner's own words in their session. **Flagged as a relay
per this repo's own rule** (*never record an approval whose granting act you have not seen*), which
was written earlier the same day, after that shape failed twice across two lanes. Quoted words with
a named source are far stronger than a status field and are still not a witnessed act. **Direct
owner confirmation requested in-session; replace this flag with the confirmation, do not delete it.**

1. **Wiki-emulator spike: APPROVED, and my flagged divergence resolved, in the direction that
   corrects empyrean, not us.** The approval was **real all along**; the spec's self-declared
   "Approved design" was factually correct, and empyrean's correction to me was wrong on the fact.
   ⚠ **Keep both halves:** an unverifiable claim turning out true does **not** retroactively make
   recording it without a citation correct: *we got away with one.* Owner: *"Yes I did but I was
   trying to save fable use so I never had an agent start. I can now if it wants with opus but just
   be careful and if we get stuck don't push."* **Authorised on Opus, with two conditions in his own
   words**. A spike, not a commitment: **report the wall rather than engineering around it.**
   Escalate to empyrean rather than burning a week proving feasibility that was meant to be cheap.
   **Not reprioritised above the acceptance parcels.**
2. **Fable seat: HOLD, with a new obligation that is better than either option I offered.**
   ⚠ **CLOSED AS AN OWNER ITEM 2026-08-22. STOP LISTING IT AS PARKED.** Asked whether to fund the
   seat long-term he answered *"Idk what you want for this"*, and the asking lane recorded that as
   **their badly-formed question, not his indecision**: the right way round. **No decision is needed
   today: hold stands, the ledger IS the mechanism, and the question returns naturally when the limit
   lifts.** Note this also retires the provenance worry above by superseding it: there is now a live
   cited ruling on the seat, so the unwitnessed 2026-08-21 ratification is **correct and moot**.
   Owner:
   *"keep careful record of what's done without fable so when our limit is no longer up the first
   thing it can do is make sure we made the correct decisions without it."* **Fable's FIRST job when
   the limit lifts is auditing exactly those decisions**, so the gap becomes a queue rather than a
   hole. ▶ **Ledger created: `docs/2026-08-22-unadjudicated-decision-ledger.md`** (L-01…L-06). Each
   entry must be adjudicable **cold**: verdict, alternatives, evidence at the time, and *what would
   have to be true for it to be wrong.* An entry recording only the verdict is useless to the audit
   it exists for. **Every future unadjudicated call gets an entry at the moment it is made**, not
   reconstructed later. Note: this supersedes the unwitnessed 2026-08-21 ratification audited above:
   there is now a live cited ruling on the seat, so that correction stands as *correct and
   superseded*.
3. **▶ THE MOST CONSEQUENTIAL, and it is aimed at this lane.** Owner: *"Oracle - let's make sure
   anything not going for the new oracle does and tell it to make sure to tell the oracle agent to
   build out any tools these other suite items/agents might need, that's how we're getting robust."*
   Two halves: **(a)** anything still pointed at the legacy C++ server should be moving to the new
   core: **the acceptance contract is the vehicle and is effectively blessed as the priority**;
   **(b) this lane is the SUITE'S TOOL-BUILDER.** empyrean is telling every lane to send named
   instrument asks here rather than working around gaps. **Inbound capability asks are first-class
   queue items, not interruptions**: his stated reason is *"that's how we're getting robust"*. This
   extends the existing aeon co-development lane from one peer to all of them.
4. **READMEs: make every suite repo's README accurate.** *"Doesn't have to be super in depth."*
   Ours must say plainly that **the MCP surface still reaches the legacy C++ server**: the fact
   most likely to mislead a reader, and the one this lane independently flagged in the status
   roll-up before the directive arrived.

> ▶ **BOOTING INTO THE CUTOVER? READ `docs/2026-08-22-cutover-handoff.md` FIRST.** It is written for
> the session that boots *after* the owner flips the config and relaunches every lane: what the
> rebuilt binary at `12cc17e` guarantees, the 17 remaining, why a `-32601` is a success signal, and
> what to do first when lanes report gaps. This section is the record; that file is the instructions.

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

⚑ **PROCESS CORRECTION, from the agent, against my own brief: `fixedAt` is a commit SHA and CANNOT exist inside
its own commit.** I have been asking agents to "mark the ledger row fixed in the same commit as the fix", which
is unachievable and contradicts every existing fixed row (M47/M48/M49 all append the row afterwards). **Stop
writing that instruction into briefs**; ask for the fix commit, then the ledger append naming it.

⚑ **DISCHARGED 2026-09-10: BOTH LANDED, IN ORDER, AND THE SEQUENCING PAID OUT.** H26 at merge `f759d76`,
then C5 at merge `282aa93` (fix `470091d`) — `Vdp::advance_scanline`, the cheap unmasked stateful twin, is
now in the tree beside `render_scanline` and the invariant is intact **because H26 was written to admit it**.
The replay is **2.58×/2.59× faster by median** (2548→989 ms and 2556→987 ms over 1176 frames, two interleaved
sessions); the discarded raster was **~61 % of the replay's whole wall clock**. `d-44` discharged: split
delivered *and* the number he asked for. Guarded by `the_cheap_scanline_advance_leaves_the_same_machine`
(whole-`Vdp` `PartialEq` after each line, 14 fixtures). Zero currency movement.
⚑ **Four lessons this pairing produced — the paraphrase sweep, the floor-is-a-prior counter-instance,
the profile-with-every-baseline ops defect against this seat, and the two-paths-wrong-the-same-way guard
case — are in `docs/OVERSEER-REFERENCE.md`, because each is read BEFORE DISPATCHING, REVIEWING or
LANDING and none of them at boot.**

⚑ *(Superseded, kept because a session citing the hold must see it was correct when made.)* ~~**RULED
2026-09-10, SEQUENCING: C5 AND H26 TOUCH THIS ONE FUNCTION FROM OPPOSITE ENDS AND MUST NOT RUN
CONCURRENTLY. H26 first, C5 after it lands.**~~ H26 (`d-47` answered `structural`) is deciding whether this
very claim gets a real mechanism — plausibly by making the no-mask signature structurally locked. C5
(`d-44` answered `split`) splits that same call so the cheap path stops building a full attributed report
to obtain three status bits, i.e. **it proposes exactly the "twin" the claim above says does not exist.**
Neither is wrong; designed in parallel they would each be correct against a tree the other is changing,
and the merge would resolve cleanly while the safety property quietly stopped being true — the failure
this section exists to prevent, arriving through the fix rather than through an edit. **The C5 brief must
carry H26's outcome**: a cheap twin that renders no picture is not a mask parameter, but whether it may
exist at all is H26's ruling to make first. *(Held while the hub had already said "take C5"; the hazard is
visible from the source and was not visible from the board.)*

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

**Also carried in that same message and both banked; moved whole to `OVERSEER-LOG.md` 2026-09-06:** our landing recorded upstream with correct attribution (their number cited as *ours*, not re-derived), and **F-RESUME-STOP-RACE relayed to aurora** as the suite's outbound client, which is the right destination — no reply was requested and none is owed. With them, the content-addressed check that verified our vendored `bus-protocol.schema.json` against empyrean's blob id **in both trees, neither read from a working file**, which is why a relayed claim about our own tree was safe to accept.

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

## ⚠ Bootstrap: read the protocol at a COMMITTED revision (stays here on purpose)

*This stanza did not move to `docs/OVERSEER-REFERENCE.md` with the bars around it, and must not:
it is upstream of the boot read itself, and a rule filed in a file you open later cannot protect a
read that has already happened. The protocol's own preamble sanctions each repo carrying exactly
this one stanza.*

> ⚠ **READ THE PROTOCOL AT A COMMITTED REVISION, NOT THROUGH THE PATH** (seraph's rule, empyrean
> `origin/main`; the most upstream rule in that document): `../empyrean/docs/OVERSEER-PROTOCOL.md`
> is **one peer's live working tree**, so booting by path delivers the suite's shared contract by
> reading somebody's uncommitted directory. Use
> `git -C ../empyrean fetch -q origin && git -C ../empyrean show origin/main:docs/OVERSEER-PROTOCOL.md`.
> Correct citation discipline applied to a bad source produces a **more** convincing artifact, not a
> less convincing one. **This session's own boot is the measured case**: it read the file by path
> while empyrean held sixteen unpushed commits, and got the right bytes only because their worktree
> happened to be clean at that minute; 59 lines landed in that path minutes later, and the file
> reached **422 lines by day's end against the 245-line snapshot the session booted on**. Right
> answer, by timing luck, with nothing in the output saying so.
>
> The commentary on protocol bars 8-15 moved to the log: they are read at boot from the protocol
> itself, at a committed revision, which is the only copy that cannot drift. The stanza above stays
> because the protocol's own bootstrap exception sanctions it.

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

## Where the detail lives

The dated `docs/2026-0[89]-*.md` files are the arc records (handoff/recon/CR/ruling per arc; newest
first is the reading order). The 08-* arcs end-to-end: scanline acceptance + convention
(`…-subline-*`), CR-25/26/27 with rulings, the profiler demand/recon/deltas, the Aurora client
demand, the streaming asks. `docs/2026-08-19-subline-shipped.md` is the model handoff shape.

**Three files, split by WHEN each is read, and a section is classified by its CONTENT, not its heading:**

* **`docs/OVERSEER.md`** (this file) is the boot read, bounded at 100,000 B. It holds scope, the queue,
  any resume brief, and the standing rulings that change what a session does FIRST.
* **`docs/OVERSEER-REFERENCE.md`** holds the bars and the ops lessons: not read at boot, opened
  before dispatching, before reviewing returned work, and before landing.
* **`docs/OVERSEER-LOG.md`** holds closed history, append-only, newest last: not read at boot, read by
  `tail`/`grep` when a particular night or a moved entry is in question. **A live ruling goes in
  `OVERSEER.md`, never only in the log.**
