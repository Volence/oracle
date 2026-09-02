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
Fable agent, no steer — recorded 2026-08-21 as cost-questioned-and-ratified by the owner, on the
reasoning that the ruling is where one judgment becomes permanent contract text, so the smartest
model sits there and nowhere in the bulk work. ⚠ **PROVENANCE, audited 2026-08-22 after empyrean
flagged the class:** that ratification is recorded in `7fd201d`'s prose and commit message and
**nowhere else — no citation of the granting act exists.** It is *stronger* than empyrean's
parallel case (their string was a spec's own self-declared status field; this one carries a
rationale responsive to a cost objection, which is the shape of a real exchange) and it is *weaker
than a cited ruling*, which is the only thing that settles it. Git authorship proves nothing here:
every commit in this repo carries the owner's identity whoever wrote it. **Standing rule adopted
from empyrean: never record an approval whose granting act you have not seen — cite the ruling,
not a status field.** Treat the seat as owner-confirmable, not owner-confirmed. **The action it was
used to justify survives the doubt anyway** — declining to spend a premium-model budget without the
owner is his call whether or not a prior ratification exists — but the *justification* was
over-claimed, twice to him and once across the fence, and is corrected here); verify every gate
firsthand before accepting a slice; make the design
rulings (delegated by the owner — pick best, record why); merge and push. The owner's standing
directives: **a legacy surface or demand spec is the compatibility floor, never the design
ceiling** (run a visible better-approach pass on every request), and **instrument co-development
with aeon** is the ratified lane (their diagnoses name gaps; we build them; the engine gets fixed
with tools that then exist).

## The boot read is bounded, and this file was split to meet it (2026-09-02)

Closed history lives in **`docs/OVERSEER-LOG.md`**, which is NOT read at boot; this file is the live
half. The governing rule is `origin/main:docs/OVERSEER-PROTOCOL.md`, *"The boot read is bounded"* —
read it there, never this summary. The split was proved lossless by line-multiset before committing.
**A ruling that must survive a rotation goes here, never only into the log.**


## The queue (2026-08-19 end of day — reorder only with cause, record the cause)

*(Items 1-7 are closed and moved to the log. Item 8 keeps its live tail below; its closed sub-arcs
moved with them.)*

8. **▶ OPEN — THE ACCEPTANCE CONTRACT.** The definite list of what the successor must serve before it
   replaces the legacy C++ server. **Re-derive the membership, never transcribe it** — the machine-
   enforced source is `SCHEMATIZED_NOT_ADVERTISED` in `crates/oracle-aether/tests/schema_conformance.rs`,
   asserted as a whole sorted set, so it cannot drift silently. Board row `ACCEPT-16`. The arc's closed
   history (survey, CR-A, trio, CR-B) is in the log.

   **NEXT (not yet dispatched):** open from the survey and **not lost**: stale prose at
   `schema_conformance.rs:6,222` and the `resolve_target` `oneOf` divergence (**both folded into
   the in-flight `run_to_scanline` parcel** — remove from here once that lands), and a proposed
   **error-surface gate** — since no fragment declares error conditions, a suite validating only
   replies is blind to every error obligation.
   **FOREGROUND runtime follow-ups, never a subagent** (the emulator MCP deadlocks from background
   agents): the `step` frame-budget truncation; the `write_vram` SAT-cache desync; and **two new
   ones from CR-B** — the tail wrap (`z80_write {addr:"0x3FFC", bytes:<8>}` then read `$0000`;
   predicted from source: bytes 5–8 land at `$0000–$0003`) and the silent `len` clamp
   (`z80_read {len:10000}` → `8192`, no error). ⚠ **ATTEMPTED 2026-08-22 evening and correctly
   ABANDONED, not deferred for convenience:** my MCP client found no socket in my own
   `$XDG_RUNTIME_DIR` (`Errno 2` — a *failing lookup*, which says nothing about the world, bar
   16(d)). A `pgrep` showed an emulator IS live, but it is **another lane's harness** with its own
   `XDG_RUNTIME_DIR=/tmp/oracle-harness-4av2i47x` running `aeon/s4.debug.bin`. Writing into a
   peer's harness Z80 RAM to demonstrate a wrap bug is the shared-machine hazard itself. These stay
   open until a lane-owned instance exists; **the CR does not depend on them** — it stands on the
   source read, and the runtime pass would only upgrade it from derived to demonstrated.
   **AEON OBLIGATION — SCOPE WAS WRONG, and the correction makes it bigger.** Item 7 recorded it
   as a dated heads-up before serving `emulator/wait_for_break`, because their gates send
   `timeout_ms`. **The survey found it covers THREE methods, not one, and I verified it firsthand
   at `origin/master` (not their working tree):** both scripts run an **arm → wait → clear** flow —
   `raster_source_gate.py:161/168/173` and `snapshot_poison_gate.py:62/64/68` call
   `emulator/breakpoint_add {addr}` → `emulator/wait_for_break {timeout_ms}` →
   `emulator/breakpoint_clear {all:true}`.
   **Consequence, and it is the load-bearing one: the migration CANNOT be piecemeal.** Serving
   `wait_for_break` alone would leave their flow with nothing to arm — so `wait_for_break` and the
   breakpoint trio ship as ONE parcel or the notice is worthless. The `timeout_ms` spelling was
   never the whole exposure; it was the part visible from a param grep.
   **This also gives the obligation a live reader BEFORE any date exists.** Their call sites bet on
   a specific breakpoint shape — `{addr: "0x…"}` to arm, `{all: true}` to clear, i.e. **address-
   keyed, no handles** — and **CR-A (D-13) is about to decide exactly that handle discipline.**
   Their input window is *now, before adjudication*, not when we ship. Note also
   `raster_source_gate.py:33`: under `deterministic=True` the legacy server answers `breakpoint_add`
   with a "det-mode stop" behaviour — a documented interaction our fragments say nothing about.
   The **date** still waits on the survey's pricing of that parcel; the **design consultation**
   does not, and holding it until a date existed would have consulted them after the ruling.
   If this session ends first, **the next one owes both**.


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

**Registered 2026-08-29, from aurora's relay of the owner's R8 question:**

- **F-R8-LATE-REVISION** — a *copy-of-column-0* toggle for the R8 leftmost-partial-column quirk, so a
  fix can be seen under both hardware behaviours. **Declined as a booking, and the reason is not
  "noise".** Under the later behaviour the leftmost column takes column 0's vscroll, so the defect
  simply **disappears** and aeon's column-19 write becomes an inert no-op rather than something the
  differential validates — the whole verdict is derivable without building it. Against that we have
  **no hardware-tested rule for the late revision**, only Plutiedev/Stef descriptions, where the early
  rule pinned at `render.rs` `plane_vscroll` (H40 `VSRAM[$4C] & VSRAM[$4E]`, H32 `0`, same value both
  planes) matches Genesis Plus GX's *"verified on PAL MD2"*. Shipping a second model whose fidelity
  cannot be established, and then letting a consumer validate a fix against it, is bar 9's corollary
  exactly: an unvalidated instrument adopted as a gate returns a **confident wrong verdict**.
  *Revival condition:* a hardware-tested rule for the later revision appears, **or** a second scene
  turns up where the fork changes a design decision. The divergence ledger already records the fork,
  so nothing is hidden meanwhile.

- **F-BANNER-INVITES-A-PIN** — *found from the other end, by a consumer breaking on it (aurora's O26,
  2026-08-29).* Our startup banner prints `aether: N methods advertised`, and `Bus::start` prints the same
  total on the serving line. **A published total is an invitation to pin it**, and a consumer did: their
  `classic-playtest-harness.mjs:171` pinned `methods === '35'` and *threw* `stale oracle-aether binary` on
  anything else — so the guard written to detect staleness became the stale thing and rejected every
  correct binary. **Measured firsthand at `6031020`, two ways: the banner says 52, and `initialize`'s
  `methods` array has length 52** (spawned and called, not read off a schema).
  **The defect is theirs; the surface that manufactures it is ours.** A count changes for reasons unrelated
  to freshness and is identical across binaries that differ, so it is the wrong observable for the question
  every consumer actually asks — *is this binary current?* We already serve the right answer and do not point
  at it: `initialize.serverBuild` carries `{id: "<sha>+profile=…+target=…+features=…", source, dirty}`.
  **Cheap fix, and it is a documentation-and-adjacency fix, not a removal:** name `serverBuild` in the same
  breath as the count, so the number a reader meets first is not the only identity on offer. Do **not**
  simply delete the total — it is genuinely useful at a glance, and aurora's lesson is about what a consumer
  should *key on*, not about what we may print. Our own side is clean: grepped, every use is `METHODS.len()`
  and no literal total is pinned anywhere (`crates/oracle-aether/tests/params_closure.rs` closes over
  `METHODS.len()`, which is the derived form).
  **The durable line, aurora's: a total was the wrong observable.** Same family as this file's own
  name-is-not-behaviour bar — a number that *correlates* with the property being tested, standing in for the
  property, and reading exactly like a real check until the correlation breaks.

  ⚠ **AMENDED SAME DAY, and the amendment corrects THIS BOOKING, not the consumer** *(aurora, 2026-08-29,
  who class-checked the SHA out of habit and found the thing I had just recommended people point at)*. The
  paragraph above says "name `serverBuild` in the same breath as the count". **That advice is incomplete in
  a way that walks a consumer back into the same trap from the other side.** `serverBuild.id` names whatever
  HEAD was at build time — and the id they measured, `6031020`, is a **docs-only commit** (`lane-status.json`,
  +10/−18). That is correct behaviour for a build identity and is exactly what staleness wants, but it means
  **the id moves for reasons that have nothing to do with the code**: binaries built at `acf41f5` and at
  `6031020` contain identical code and report different ids.
  **So the field answers *"is this the same binary I measured before?"* and MUST NOT be compared for equality
  to answer *"does this build contain feature X?"*** — that second question belongs to `capabilities` and
  `methods` membership, which is the derived form this register already recommends. Whenever we point a
  consumer at `serverBuild`, we owe them that sentence in the same breath; a pinnable identifier offered as
  the cure for a pinnable count is the same defect in better clothes.

- **F-SERVERNAME-PREDATES-THE-RENAME** — `EngineConfig::default()` sets `server_name: "oracle-next"`
  (`crates/oracle-aether/src/engine.rs:205`, read at `fee8f12`), so every `initialize` still answers with the
  **pre-rename** repo name; `serverVersion` is `"0.0.0"`. Spotted by aurora 2026-08-29 while assessing what on
  our wire is an identity.
  **Not a wire-correctness bug, and say so plainly:** §2.1 deliberately demotes `serverName` to a *deployment
  label* and moves identity to `implementation` (`"oracle-rs"`) and `serverBuild`, both read from `build_info`
  and — verified firsthand, not from the comment — **barred from configuration by a source-level test**
  (`tests/server_build.rs::neither_identity_value_is_reachable_from_configuration`). A consumer reading
  `serverName` for identity is reading the field the contract told it not to.
  **But the value is still a stale name we publish on every handshake**, and "it is only a label" is exactly
  how a wrong string survives a rename. **Changing it is wire-visible**, so it does not get a drive-by edit:
  it needs bar 14's consumer-set enumeration first — grep every sibling tree for the literal `oracle-next`
  with real client context — because the failure mode of a consumer matching on it is silent. Revival
  condition: do it as part of any deliberate handshake pass, never alone.
  **ONE CONSUMER CLEARED, AND THEIR OWN CAVEAT IS THE REASON IT IS STILL NOT A GREEN LIGHT** *(aurora,
  2026-08-29, asked for exactly this input)*: they grepped `src/`, `test/` and their harnesses — 40
  references to `serverName`, **zero** comparisons/branches/`includes`/`startsWith`/`match` on its value;
  every use is display or pass-through. **But they also volunteered that the literal `'oracle-next'` appears
  21 times across 8 of their test files as fixture INPUT**, with assertions derived from the payload
  (`expect(s.serverName).toBe(PAYLOAD.serverName)`) — so a rename leaves their suite green **while their
  fixtures quietly describe a server that does not exist.** Their formulation, worth keeping verbatim in
  spirit: *our green is not evidence your rename is safe; it is evidence we do not look.* That is the
  clearest statement of this register's own no-consumer-broke hazard anyone has offered, and it came from
  the consumer. **Four repos remain unenumerated** (aeon, seraph, sigil, empyrean), so the booking stands;
  aurora has asked to be told if it moves, so they can re-point their fixtures.

- **F-RSP-XVFB-ORPHAN** — *audited into existence by a peer's warning, and the audit came back clean on
  the thing they warned about.* aurora relayed their O16 finding 2026-08-29: 28 of their harnesses tore
  down with `pkill -f '<dist path>'`, an argv pattern that matched **other sessions' processes** and had
  killed a peer's Electron mid-run three times. **It does not apply here, and that is a measurement, not
  an assumption.** Enumerated by what *touches process teardown* rather than by the token (protocol bar
  8): the executable surface is 16 `.sh`/`.py` files plus every `.rs`; spawn sites are five
  (`rsp.py:43`, and four `Command::new("git")` that are `output()`-style and never outlive the call);
  **teardown sites are exactly one — `rsp.py:157 self.p.kill()`, on the `Popen` handle that object
  itself spawned.** Ownership by construction: no pattern, no process name. Every `pkill`/`killall`
  string in this repo is in **docs prose warning against it** (5 in `docs/`, plus stale copies inside
  three dead `.claude/worktrees/`), zero in code. The control run (165 bare `kill` hits) confirms the
  grep could see what was there, so the empty result is a measurement rather than a broken pattern.
  ⚠ **The one residue their audit did surface here, and it is the OPPOSITE failure — sent to them
  hedged, as a reading rather than a finding, because I have not run it.** `rsp.py` spawns through
  `xvfb-run -a`, so `self.p` is the **wrapper's** pid, not BlastEm's. `close()` sends the RSP `k`
  (kill) to BlastEm first and that is the normal path, but when the stub is wedged — the case the
  watchdog exists for — the backstop `SIGKILL` lands on a shell that cannot trap it, so BlastEm and
  Xvfb may be left **orphaned**. That endangers nobody else's session (a stranger surviving, never a
  stranger dying), which is why it is a register entry and not a hazard notice. **Revival condition:**
  the differential harness is run in anger again, or a stray `blastem`/`Xvfb` is found outliving it —
  fix is a process group (`start_new_session=True` + `killpg`), not a wider pattern.
  ⚑ **STRENGTHENED 2026-08-30 by aurora's measurement (aurora `055bff40`), which supplies the consequence
  this booking lacked.** `/usr/bin/xvfb-run` **has no trap**, so a SIGTERM to the wrapper skips all of its
  cleanup — and each skipped cleanup leaks an X lock, a socket **and a tempdir**, where **every leaked lock
  permanently burns a display number**, so every later run on the box scans longer. My entry had the
  orphaned-process half and read the cost as "a stray process survives"; the real cost compounds across the
  machine and outlives the session that caused it. Their portable fix: reap the wrapper's **children** by
  PID first, then the wrapper.
  *Two honest scope notes.* (a) aurora **retracted** the suite-wide version of this (aurora `81ebf173`) —
  they fingerprinted the leak to their own harness, so nothing here is currently leaking; measured at the
  time: `/tmp/.X*-lock` = 0, `/tmp/.X11-unix` = 0. (b) **This lane nearly reported "no xvfb-run here" on a
  grep that had errored on shell globbing.** Run correctly there are four hits — `rsp.py:10`/`:41` and
  `nightly_differential.py:145`/`:217`. Second instance in one night of bar 16(d) at this seat: **a failing
  command and an empty world print the same thing**, and only a positive control separates them.
  *Not applicable to the player-window fixtures* (`docs/2026-08-29-window-runtime-checks.md`): those run
  their own `Xvfb :N` and kill it by recorded PID, so the wrapper is out of the loop entirely — which was
  an accident of needing a known `DISPLAY` for XTEST, not foresight, and is recorded that way.

**▶ NEW BAR, 2026-08-30 — EVERY CITATION RULE THIS SUITE OWNS IS WRITTEN FOR THE RECEIVING SIDE, AND
BOTH OF TONIGHT'S FAILURES WERE ON THE EMITTING SIDE, WHERE NO RULE REACHES.** aeon's formulation, banked
by them at aeon `4fae2d8d`; two instances, one from each lane, hours apart.

**The pair.** This lane sent sigil a confident wrong claim about **its own tree** (the `.lst` recipe
"residue", retracted above). aeon reported a commit hash **typed from memory of a commit they had made
four minutes earlier**, which resolved in no object store anywhere. Different artifacts, one mechanism.

**Why no existing rule catches either.** `--stat` what you are handed; check the SHA's class; verify the
anchor; do not trust the paraphrase — **every one of them presupposes an incoming artifact.** There is no
incoming artifact when the claim is about your own work, so the whole apparatus is structurally
inapplicable. aeon's diagnosis of *why* it goes unchecked is the sharp part: **a claim about someone
else's tree feels like a claim and gets verified; a claim about your own feels like recall.**

**And the amplifier, which is this lane's half:** a careful peer reasoning soundly on your wrong premise
makes the error look **corroborated rather than caught**. Their care is what hardens it. Same circuit as
the 52-method number that went out from here, came back as a peer's, and outranked our own measurement.

**Their operational form, for hashes:** emit every SHA from the command that proves it, in the same
invocation as the thing it anchors. **Ours is about claims rather than hashes, and is this:** a statement
about our own tree that is going OUT — to a peer, to the owner, into a doc — is read out of the file at
send time, or it is sent hedged. Not because it is likely wrong, but because *nothing downstream can
check it*, and the more competent the receiver the more thoroughly it will be built upon.

⚑ **AND THE PATTERN BEHIND THE INSTANCES, 2026-08-30 — THIS SEAT KEEPS READING A SOUND OBSERVATION AT ONE
NOTCH TOO WIDE A SCOPE.** Three in a night is a habit, not luck, so it is booked as one entry rather than
three slips:

| observed (true) | asserted (too wide) | what settled it |
|---|---|---|
| no socket in `/run/user/1000` | *"the socket chain is EMPTY, no lane can reach any emulator"* | reading the resolver — one path, chosen on a directory test |
| a grep for `xvfb-run` returned nothing | *"this repo has no xvfb-run site"* | the grep had **errored** on shell globbing; four real hits |
| `ls /tmp/.X*-lock \| wc -l` = 0 | *"the box has zero leaked locks"* | `ls` is aliased to `eza`, which **exited**; `find` says 108 |
| two `cargo` processes running | *"two cargo runs in THIS repo"* — the serialized-cargo hazard | `/proc/<pid>/cwd`: the other is in `.sigil-clamps`, a sigil worktree, under aeon's `refreeze --attest`. Different target dir, no lock contention, rule not engaged |

**The shape is constant: the measurement is right, the SCOPE is asserted rather than measured, and the
wider claim is the one that gets said out loud.** Each was settled by exactly one command — read the
function, run the grep correctly, use `find`, read `/proc/<pid>/cwd`. **Two of the four are the SAME
mechanism** (a command that failed, with its error suppressed or swallowed by an alias, counted as an
empty world), which is bar 16(d) and which this file already carried when all three happened.
**Operational form, and it is narrower than "be careful": before stating a scope — *no lane*, *this repo*,
*the box*, *nowhere* — name the command that establishes THAT WORD, not the one that established the
observation.** They are rarely the same command, and the second one is usually cheap.

⚑ **THE INSTANCE, SAME NIGHT, AND IT IS A VERIFICATION I REPORTED AS CLEAN.** aeon found a blocker with no
decision card; this lane ran the same check over its own board and told a peer *"no missing cards here"*.
**That emitted claim had two independent defects, and a peer found each — neither was found here.**
1. **Wrong enumeration parameter.** It read blockers from `blockedOnOwner` only, and `AUDIO-DULL`'s owner
   blocker was sitting in a queue row's `blockedBy` **free text**, where the enumeration could not see it.
   The correct form enumerates owner-blocking claims from `blockedOnOwner` **and** every `blockedBy`
   string, because prose is where one hides. (aurora, 2026-08-30.)
2. **Wrong data shape.** It built `{id: entry}` over `docs/decisions.jsonl` — the dict-by-id that
   empyrean `52519fd` now forbids outright, ledger tooling being **line-addressed**. Measured here after
   the amendment landed: 20 lines, 20 distinct ids, **0 entries dropped** — so the answer was right and
   the method was wrong, and it was *guaranteed* to break at the first 8c closure, which appends
   duplicate content by design. **A check that is correct by luck reports exactly like one that is
   correct**, which is why the amendment is a shape rule rather than advice.
**Both defects were in a claim about our OWN board, sent outward, where nothing downstream could check
it** — the bar above, arriving on the verification written to enforce a neighbouring bar. Nothing was
committed carrying the bad shape (only two docs mention the ledger; no scripts), so this is a habit note,
not a repair. **Do not transcribe the contract clause itself** — read `contract/DECISIONS.md` there.

**▶ F-CR28-CALLERS-DANGLING, registered 2026-08-30 — an unmerged commit in a leftover worktree, found
while earning an `atBoundary: true` claim rather than asserting one.**

Branch **`cr28-callers`** holds **one commit not on `main`** — `22d57ca` *"docs: CR-28 ruling applied —
ADOPT WITH CHANGES, M1-M7 and S1-S4"*, a **418+/145− revision of `docs/2026-08-21-cr28-callers.md`**
whose blob **differs from `main`'s copy of the same path**. Its own message ends *"Nothing merged,
nothing pushed"*, which is an agent's honest close-out, not a verdict on whether the controller wanted it.

**Deliberately NOT merged and NOT deleted.** Queue item 5 records CR-28 as fully done — ruling
adjudicated, applied and served on 08-21 — so this is *probably* a superseded intermediate. **Probably is
not knowledge**, and merging a docs revision into a closed arc on a guess is worse than leaving it.

**Why it is registered rather than mentioned:** the two sibling worktrees (`parcel/gui-layers`,
`profiler-shortrow-residual`) are genuinely merged — **zero-ahead AND ancestors of `main`, both
conditions checked**, because zero-ahead alone is the two-valued reading bar 16(a) was written for. This
one is neither, and a dangling branch is invisible to every reader who does not run `git worktree list`.

**Revival condition:** anyone reopening CR-28, or the next session that prunes worktrees. Resolve by
diffing `22d57ca:docs/2026-08-21-cr28-callers.md` against `main:` and deciding whether the revision was
superseded by the applied ruling or dropped by accident — then merge it or delete the branch **with the
reason recorded**, so the next reader is not asked the same question a third time.


**Registered 2026-08-29 by the two window checks** (`docs/2026-08-29-window-runtime-checks.md` — both
gates discharged; the `LIVE-OBJECTS-CARD` sequencing blocker is cleared):

| id | what | revival condition |
|---|---|---|
| **F-WINDOW-BUS-FRAME-OFFBYONE** | ✅ **DIAGNOSED 2026-08-30, and the registered reason below was WRONG.** ~~*A completed-vs-presenting convention difference would explain it and would not be a defect.*~~ **It is a real, accumulating divergence.** Bus `frame` is DERIVED from the clock (`engine.rs:2223`, `now() / MCLK_PER_FRAME`); the window's `F` is a COUNTER bumped after every run iteration whether or not a frame completed (`main.rs:1929`). A breakpoint stopping mid-frame is a **permanent +1** that never self-corrects, and a **state load diverges them without bound** in the other direction (`main.rs:1571` prints *"frame counter continues at {frame}"* while the restored clock rewinds). `engine.rs:2237` had already named "a UI counter" as the thing it refused to serve — the window IS that counter. **DO NOT JOIN THESE NUMBERS**; `emulator/screen_text`'s fragment carries the disclaimer in its own description so no consumer tries. **Fixing the counter is a SEPARATE item and deliberately not bundled** — it is a behaviour change and a contract ruling must not be made contingent on one. | **RULED 2026-08-30 (L-08, `docs/2026-08-22-unadjudicated-decision-ledger.md`): RELABEL, do not sync the counter.** The engine had already settled it from the other end — `engine.rs:2349-2351` refused to serve a UI counter and named the cost (*three hand-rolled realignments*), so syncing ours is that refusal re-litigated from the losing side. A still-incrementing `F` at a breakpoint halt is how a person sees the render loop is alive while the machine is not; a clock-derived number freezes there. What is left is the WORDING, for whichever parcel touches the status line. The *joining* hazard is already closed by the fragment. |
| **F-SCREEN-TEXT-PALETTE-LENS** | `emulator/screen_text` serves **three of the five** adopted `kind`s — `titleBar`, `statusLine`, `toast`. `palette` and `lens` are not served (reasons in the module doc); both are **additive and need no contract change**. Separately, the layer badge and the PAUSED banner have **no `kind` in the adopted enum at all**, so they cannot be reported without a contract amendment. | A consumer asking for palette/lens text, or a CR that widens the enum. |
| **F-STOPPREC-HOSTED-HALT** | 🔴 **REGISTERED 2026-09-02 (`parcel/stopprecision`).** §8 item 24's proof measures the breakpoint halt on the **socket** free-run driver only. `Engine::halt_on_breakpoint` has two callers — that one and the player window's loop through `Host::pump` — and the hosted one is not reachable from a socket client, so its `stopPrecision: "exact"` is inferred from sharing one function with the measured path rather than measured. Both read the stopping `pc` from the same `self.sys` at the same point, which is why the inference is reasonable and why it is still an inference. | Needs a host-side test in the shape of `host.rs`'s `the_bus_and_the_panel_read_one_instrument`, which reaches the instrument from the host's side. Bounded. Revive when the player's breakpoint path is next touched, or sooner if a consumer reads a window-driven halt as exact. |
| **F-RESUME-STOP-RACE** | 🔴 **REGISTERED 2026-09-02 (`parcel/stopprecision`), and it was found only because a test repeated.** `c.ok("emulator/resume")` followed by `next_stopped(c)` RACES: the halt's `stopped` event is broadcast from the engine thread while the `resume` reply is written by the connection thread, and `Client::ok` reads through to the reply **discarding every event it passes**. When the halt wins, the event is thrown away and the test blocks to its 20 s socket timeout. Measured on a seven-instruction fixture at **trial 4 of 8, after three clean passes — a single-shot test would have called it green.** Fixed inside `tests/stop_precision.rs` (`resume_and_wait_for_stop`, which reads both lines before acting on either); the same spelling is still live in `tests/breakpoints.rs` and `tests/watchpoints.rs`, where it has not been observed failing because those fixtures take longer to reach the breakpoint. | Lift `resume_and_wait_for_stop` into `tests/common` and use it at every `resume`-then-wait site. Out of `parcel/stopprecision`'s scope. Revive on the next flake in either file, or when either is next edited. **⚑ AMENDED 2026-09-02 — IT HAS AN EXTERNAL TRIGGER NOW, AND A COMMITMENT RIDES WITH IT.** aurora enumerated their client and **cannot hit this today** — one non-test `onEvent` consumer, which discards the event; nothing awaits an event anywhere; every sequencing point gates on a reply (their `44f17ca8`, re-verified by them against our `7ba2faf`). **That "no" is scoped to code they have not written yet:** the first thing anyone builds on `breakpoint_add` is arm → resume → wait-to-be-told-it-hit, which is exactly this shape. **So the revival condition is now also: aurora (or any client) starting a breakpoint consumer — and the fix has to be IN before that lands, not after.** **⚑ STANDING COMMITMENT, booked here under bar 20's sending half because it was made in mail and mail is not part of the tree:** this lane told aurora to ping before they write any wait loop, and undertook to tell them **whether this fix has landed yet**. A `/clear` must not lose that — if they ping and this row is still 🔴, the honest answer is *not yet, and here is the shape to avoid* (the inverse caveat in the `run_to` section below: the `stopped` event precedes the reply, so take-the-reply-then-wait blocks forever). |
| **F-STEPOUT-SLOW-CLIENTS** | `emulator/step_out` with `{}` takes **~92 s in a debug build** on this box (600-frame bound, per-instruction sink), measured directly. Test clients use a **20 s** socket read timeout, so the all-method sweeps (`handshake::…advertises_a_generated_method_list…`, `methods.rs`) are **timing-marginal under load** — observed failing twice in isolation on a busy box and passing in both full runs. **Pre-existing, not the `screen_text` parcel's, and it will bite again.** | Any flake in those sweeps — read this row before diagnosing it as new. |

**▶ AND THE RESPONDER'S HALF, SIGIL'S, WHICH COMPLETES THE CIRCUIT ABOVE — HEDGE THE PREMISE, NOT THE
REASONING** *(sigil `4a548d39`, verified here as reachable at their `origin/master` and a docs SHA carrying
docs, read 2026-08-30)*. Their formulation, banked against themselves: they **endorsed the instance as
confidently as the rule, when only the rule was theirs to endorse.** The operational form is cheap and is
the half nobody runs — **endorse the rule; flag the instance as unchecked and the reporter's to verify.**
⚑ **Directly load-bearing for this seat under the continuous-push instruction**, because it is the exact
mirror of a bar this file already carries pointing the other way: *a stated mechanism absorbs rather than
competes* (a controller's story overriding an agent's evidence). Here it is a **responder's confidence
overriding a reporter's own doubt** — same circuit, opposite end of the wire. This lane held only the half
that flattered it, and so did they.
**Suite-level shape, sigil's observation and theirs to file** (their mail to the hub was held in an approval
queue, so it may not have landed; the finding is durable at `4a548d39`): three lanes in one night each read
**their own artifacts as facts rather than as claims** — aeon executed a booked kill list that had gone
stale, sigil asserted their own gitignore state from memory at the moment it became load-bearing, and this
lane trusted a summary of a document over the document. **Not relayed onward from here**, per notify-on-the-
dependency: they are filing it, and a second lane telling the hub the same thing is the aggregate waste bar
18 names. Recorded so the pointer survives if their mail did not — it lands on the hub's own live
`PLAN-PROSE-SWEEP` item.

**▶ REGISTERED 2026-08-30 — F-LEGACY-SILENT-DEFAULT, and it is the sharpest argument the cutover has.**
`docs/2026-08-30-legacy-silent-default.md`. The legacy C++ server (`oracle-old`,
`linux-port/gui/ControlSocket.cpp`) validates **no parameter at all**: `getInt(k, d = 0)` at `:130`
returns the default on absent key, unparseable string (`catch (...)`) and unhandled type, and there is
**no unknown-key rejection anywhere in the file** (verified as a genuine absence under a positive
control, not read off empty output). ⚑ **CORRECTED same day: the family is FOUR accessors and 63 call sites, not one and 34**
(`get` str `:119`/18 sites, `getInt` `:130`/23, `getU32` `:152`/11, `getBool` `:156`/11) — the original
count enumerated by too narrow an ALPHABET and agreed with itself, **bar 19 turned on this seat**, and it
was aeon's wider `get*("key")` sweep, run for their own purposes, that surfaced it (bar 21: the
discriminator fired by accident again). ⚑ **AND CLOSED AT 64 (2026-08-30, second pass): 63 accessor sites + `ParseButtons` `:1576`, the one
read that type-checks — found by aeon varying the parameter ON PURPOSE (the first deliberate invocation
this lane has seen), and the population is now COMPLETE rather than a running total** (59 `const JsonObj&`
signatures swept; the only raw-`json` touches in the file are inside the four accessors plus `ParseButtons`,
and `JsonObj` exposes nothing else). A misspelled `buttons` presses NOTHING and returns success.
⚑ **Plus a type-gap of our own finding: `has()` `:117` is satisfied by any present non-null value while
`getBool` `:156` accepts only `"true"`/`"1"`/`"yes"`, so `{"enabled":"on"}` passes the explicit guard, reads
false, and `*flag = !on` MUTES the layer the caller asked to enable** — partially mitigated because the reply
echoes its own decision. **And the near-miss is banked with it:** the three unguarded-looking `getBool`
sites are in fact guarded, and this seat nearly reported a silent inversion that does not exist for the
missing-key case — caught by reading the lines around the cited line (bar 11, on our own finding).
⚑⚑ **AND THE SIX WERE THE WRONG SIX — RETRACTED 2026-08-30, found by aeon, verified here against the
enclosing blocks.** FOUR of them (`:348`, `:615`, `:739`, `:782`) sit inside `if (req.has(...))` and fail
LOUDLY; two genuinely unguarded ones were missed by BOTH lanes (`:2110`/`:2140`, `read_vram`/`write_vram`,
`getU32("addr", 0)`). True set = **{read_vram.addr, write_vram.addr, z80_read.addr, z80_write.addr}**.
**My enumeration parameter conflated "no explicit default" with "unguarded"** — orthogonal properties, so it
produced 4 false positives AND excluded the 2 real ones *by construction* (they carry an explicit `, 0`).
**Third enumeration-parameter failure by this seat in one day, and the damning half: the same document had
already applied bar 11 correctly to its `getBool` sites and I did not apply it to the memory six.**
aeon's own error is the sharper lesson — they said my account "holds line for line" and it did: **they
verified the lines EXISTED, not that they were UNGUARDED**, a check that could only confirm. Their earlier
"independent corroboration" of the six is **withdrawn**, as is their consumer-side "we are clean"
(14-across-3-files was a grep of files they already believed were on the seam; real figure **131 across 12**,
and they found a live instance: 12 sites send `reset{"wait":true}` and `OpReset` never reads `wait`).
⚑⚑ **AND THE CANONICAL EXAMPLE WAS BACKWARDS — swap it in anything quoting it.** `"0xZZZZ"` is the
**SAFE** end: no valid hex digit after the prefix, `stoll` throws, `catch (...)` returns the default.
**The dangerous shape is a VALID PREFIX + GARBAGE** — `"0x12ZZ"` → **18**, `"12abc"` → **12**. aeon
measured it by compiling the accessor rather than reasoning about `stoll`'s contract; **reproduced here
independently** (transcribed string arm, `g++ -std=c++17`, sentinel default). Nothing we said about
`"0xZZZZ"` was false — it *does* resolve to the default and *does* defeat the absence guards — **the
defect was presenting the class's mildest member as its canonical case.** And the partial parse is worse
**in kind**: a defaulted `0` is a consistent wrong value someone may learn to recognise; address **18**
is plausible, arbitrary, and looks like data.
⚑ **WHAT SURVIVES AND IS THE DURABLE PART: the guards cover ABSENCE, NEVER TYPE.** `has()` passes any
present non-null value and `getInt`'s `stoll` throws into `catch (...)` → 0, so `{"addr":"0xZZZZ"}` defeats
all four guards. The retraction narrows *which keys must be missing* and narrows nothing about malformed
values. Headline holds of **`write_vram`/`z80_write`**, not `write_memory`.
⚑ **Plus a fourth correction against us: `getBool`'s string arm returns FALSE, not `d`** (this file and the
doc both said `d`) — immaterial at the three `enabled` sites, **material at the five `getBool(k, true)`
sites** (`:465` reset.run, `:871` watchpoint_add.write, `:1366` reload_rom.reset, `:1377` reload_rom.wait,
`:1710` hold.down), where the caller's STATED default of true is what makes the call look safe.
What did NOT move: the population (64), the four accessors, and the absence of any unknown-key rejection (`:348`, `:615`, `:702`, `:726`, `:739`, `:782`) — a misspelled
`addr` on a legacy write goes to **address 0 and returns success**. ⚑ **The `mcp__oracle__*` surface
still reaches this server**, so every lane debugging through MCP is on this path.
**Provenance, and it is the instructive half:** this arrived as aeon's *aside* — a claim about OUR tree,
in MAIL, which is bar 20's exact shape — while they were acknowledging an unrelated signal. It was
verified here rather than banked, and their version **understated it**: they named one key and one site
(`:894`, `timeout_ms`, 30000, confirmed real), where the defect is the accessor and is 34 sites wide.
A peer's passing remark about our own code was worth more than the thing they wrote it to explain.
**Revival condition:** the README sentence owner ruling 4 requires — it should carry this fact, not
merely that the surface is legacy. Explicitly NOT a fix recommendation for `oracle-old`: it is
reference-only and the cutover exists to delete it.

**Registered by the CR-27 serve review (2026-08-20), all contract-side or cosmetic, none blocking:**

| id | what | revival condition |
|---|---|---|
| **F-PLAYINPUT-ITEMS-OPEN** | `emulator/play_input`'s `rows[]` **items** are not closed — §8 item 20's closure is applied at the result's top level, so a surplus key *inside* a row passes. The one array-of-objects on the bus whose item shape is unguarded; `read_cram`'s `palette[]` items and `watchpoint_hits`' `hits[]` items both close theirs. | Contract-side: an `additionalProperties: false` on the item subschema, next time `play_input`'s fragment is opened. No server change. |
| **F-ONEOF-COMBINATIONS** | The `oneOf`/`dependentRequired` alternations (`write_cram`'s triple-vs-`raw`, `write_memory`'s `bytes`-vs-`value`+`width`) are enforced per-fragment but nothing sweeps **every combination** of a fragment's declared keys against its own rules. Today each is hand-tested; the sweep would be mechanical and would cover the ones nobody thought to write. | A third alternation lands, or a hand-written combination test is found wrong. |
| **F-REVIEW-N12**, **F-REVIEW-N14** | Recorded by id from the CR-27 serve review; their content is in the review itself, not restated here — the implementing agent was given the ids without the text and is not inventing a description for them. | Whoever holds the review restates them; then they get a real row. |

*(N9 — the `Resolution::name`/`Display` duplication — was **taken**, not registered: `Display` now calls
`name()`. It was two lines.)*

## ⚑ OWNER RULING — PUSH AUTHORIZATION — ✅ **CONFIRMED DIRECTLY BY THE OWNER, 2026-08-24, IN THIS SESSION**

**STANDING APPROVAL, OWN REPO ONLY: a lane may push its own repo's master without asking each
time.** Reached us via empyrean-18, banked by the hub at empyrean `2bd72a03` — **verified firsthand
here: the object exists, is an ancestor of their `origin/main`, and is a docs commit carrying a docs
ruling, so its SHA class matches what it anchors.** Flagged as a relay per this lane's own standing
rule; ~~direct owner confirmation requested in-session.~~
**✅ THE CONFIRMATION ARRIVED, AND THE FLAG IS REPLACED RATHER THAN DELETED, per the rule that wrote
it.** The owner answered decision `d-1` directly in this session on 2026-08-24, choosing **"Confirm it
as standing permission"** from the two options put to him. **This lane may now push its own repo's
master without asking each time**, under the four conditions below, which ride with the grant and are
unchanged by the confirmation.

*Worth keeping, because it is the only evidence the precaution was ever worth its cost: the relay was
accurate in substance the whole time.* Holding it as unwitnessed cost this lane one evening of
unpushed docs and cost the suite nothing, while the alternative — acting on a relayed authorization —
is the failure this repo booked twice on 2026-08-22. **A precaution that turns out to have been
unnecessary is the only kind that ever gets tested.** ⚠ **The 2026-08-22 four-ruling block below is a
SEPARATE relay and STAYS FLAGGED** — he confirmed the push grant and nothing else.

**The granting act is named, which is why this relay is usable at all.** The hub consolidated a
question two lanes had stopped on separately (sigil asked outright; aeon was sitting on three
finished docs commits for the same reason, neither able to see the other asking), put three options
to him — own-repo standing / standing-for-docs-ask-for-code / per-push — and he chose the widest.
That is a granting act described, not a status field quoted, which is the distinction the
never-record-an-unwitnessed-approval bar exists to draw.

**The conditions ride with the grant and are part of it**, transcribed rather than paraphrased:
- **verify `origin` actually moved — the push is not the act, the remote moving is.** This is the
  protocol's own push-before-you-cite rule arriving as an owner condition;
- **never rewrite already-pushed history**;
- **never push another lane's repo**;
- **publication to the public wiki site stays a separate explicit ask** — not a concern in this
  tree today, but it becomes one the moment the wiki-emulator spike produces anything shippable.

**Scope, stated by the hub because this is the class of grant that gets restated wider: it
authorizes PUSHING, not the work being pushed.** It does not release this lane's boot stop, it is
not approval to dispatch or to land a parcel, and **it does not touch the CR-A/CR-B adjudication
hold**, which remains a separately parked owner item.

## ⚑ FOUR OWNER RULINGS, 2026-08-22 — **RELAYED, NOT WITNESSED BY THIS LANE**

Reached us via empyrean-73, quoting the owner's own words in their session. **Flagged as a relay
per this repo's own rule** (*never record an approval whose granting act you have not seen*) — which
was written earlier the same day, after that shape failed twice across two lanes. Quoted words with
a named source are far stronger than a status field and are still not a witnessed act. **Direct
owner confirmation requested in-session; replace this flag with the confirmation, do not delete it.**

1. **Wiki-emulator spike: APPROVED, and my flagged divergence resolved — in the direction that
   corrects empyrean, not us.** The approval was **real all along**; the spec's self-declared
   "Approved design" was factually correct, and empyrean's correction to me was wrong on the fact.
   ⚠ **Keep both halves:** an unverifiable claim turning out true does **not** retroactively make
   recording it without a citation correct — *we got away with one.* Owner: *"Yes I did but I was
   trying to save fable use so I never had an agent start. I can now if it wants with opus but just
   be careful and if we get stuck don't push."* **Authorised on Opus, with two conditions in his own
   words** — a spike, not a commitment: **report the wall rather than engineering around it.**
   Escalate to empyrean rather than burning a week proving feasibility that was meant to be cheap.
   **Not reprioritised above the acceptance parcels.**
2. **Fable seat: HOLD — with a new obligation that is better than either option I offered.**
   ⚠ **CLOSED AS AN OWNER ITEM 2026-08-22 — STOP LISTING IT AS PARKED.** Asked whether to fund the
   seat long-term he answered *"Idk what you want for this"*, and the asking lane recorded that as
   **their badly-formed question, not his indecision** — the right way round. **No decision is needed
   today: hold stands, the ledger IS the mechanism, and the question returns naturally when the limit
   lifts.** Note this also retires the provenance worry above by superseding it: there is now a live
   cited ruling on the seat, so the unwitnessed 2026-08-21 ratification is **correct and moot**.
   Owner:
   *"keep careful record of what's done without fable so when our limit is no longer up the first
   thing it can do is make sure we made the correct decisions without it."* **Fable's FIRST job when
   the limit lifts is auditing exactly those decisions**, so the gap becomes a queue rather than a
   hole. ▶ **Ledger created: `docs/2026-08-22-unadjudicated-decision-ledger.md`** (L-01…L-06). Each
   entry must be adjudicable **cold** — verdict, alternatives, evidence at the time, and *what would
   have to be true for it to be wrong.* An entry recording only the verdict is useless to the audit
   it exists for. **Every future unadjudicated call gets an entry at the moment it is made**, not
   reconstructed later. Note: this supersedes the unwitnessed 2026-08-21 ratification audited above
   — there is now a live cited ruling on the seat, so that correction stands as *correct and
   superseded*.
3. **▶ THE MOST CONSEQUENTIAL, and it is aimed at this lane.** Owner: *"Oracle - let's make sure
   anything not going for the new oracle does and tell it to make sure to tell the oracle agent to
   build out any tools these other suite items/agents might need, that's how we're getting robust."*
   Two halves: **(a)** anything still pointed at the legacy C++ server should be moving to the new
   core — **the acceptance contract is the vehicle and is effectively blessed as the priority**;
   **(b) this lane is the SUITE'S TOOL-BUILDER.** empyrean is telling every lane to send named
   instrument asks here rather than working around gaps. **Inbound capability asks are first-class
   queue items, not interruptions** — his stated reason is *"that's how we're getting robust"*. This
   extends the existing aeon co-development lane from one peer to all of them.
4. **READMEs: make every suite repo's README accurate.** *"Doesn't have to be super in depth."*
   Ours must say plainly that **the MCP surface still reaches the legacy C++ server** — the fact
   most likely to mislead a reader, and the one this lane independently flagged in the status
   roll-up before the directive arrived.

> ▶ **BOOTING INTO THE CUTOVER? READ `docs/2026-08-22-cutover-handoff.md` FIRST.** It is written for
> the session that boots *after* the owner flips the config and relaunches every lane — what the
> rebuilt binary at `12cc17e` guarantees, the 17 remaining, why a `-32601` is a success signal, and
> what to do first when lanes report gaps. This section is the record; that file is the instructions.

## ⚑ HUB RULING UNDER DELEGATION, 2026-08-27 — d-16 SUBSTITUTE: the reviewer seat, and the rule it creates

⚑ **RELAYED, NOT WITNESSED BY THIS LANE** — same flag, same reason, as the four rulings above. The
owner armed an overnight delegation in his own words (*"if anything needs decision that they can't
make you make it for them"*, transcribed by the hub into empyrean `OVERSEER.md` addition (f) at
05:39Z, banked `091ac59`) and went to bed; the hub ruled in his place and **he reviews it on return.**
Record it as the hub's ruling. Do not upgrade it to his.

**The question (d-16):** the premium independent-reviewer seat was parked days ago when it blocked
nothing. It had come to block three items — OVERLAY-STATE and CR-A, five of the sixteen unserved
methods among them. **Ruled: SUBSTITUTE.**

**THE STANDING RULE THIS CREATES, and it outlives tonight.** Adjudications run on the ordinary model
while the seat is parked, and **every ruling produced that way NAMES ITS OWN REVIEWER, at the top, in
the ruling itself.** Not in a covering note, not in the dispatch record — in the artifact a later
reader picks up cold. The reason is the whole design: Fable's first job when the owner lifts the limit
is auditing exactly these, and an audit cannot find what does not announce itself. **Independence is
preserved — a fresh reviewer that took no part in the drafting is still the half that catches real
problems. Reviewer tier is what was spent.** Say it that way; do not describe a substituted ruling as
adjudicated without qualification.

**Ledger:** every substituted adjudication gets an entry in
`docs/2026-08-22-unadjudicated-decision-ledger.md` **at the moment it is dispatched**, not
reconstructed after — the entry must be adjudicable cold, and must name *what the audit should re-run*
and *what would have to be true for the ruling to be wrong*. First entry is **L-07 (CR-A)**, which also
records the cheap first cut for the audit: **re-run the material items only**, since the M/S split
every ruling here is required to produce is the instrument that measures what the substitution cost.

⚠ **Numbering collision, live:** aeon also has a card numbered `d-16` (background chunk height). The
console shows two. **Never cross-reference a decision by number alone across lanes** — say the lane.

## ⚑ THE CUTOVER — ruled 2026-08-22 (RELAYED, see the flag above), mechanism determined firsthand

**The ruling** (owner, via empyrean, quoted): *"I say do it now and when something is needed have it
built out, no?"* — cut `mcp__oracle__*` over to the Rust server **now**, and close the remaining
methods **on demand** rather than in enumeration order. empyrean recommended registering alongside
the legacy server; **he overruled it** and they now agree, as do I: it converts the acceptance
contract from a catalogue into demand-driven work.

**✅ RULED PROCEED (relayed, with the full measured cost disclosed to him first — aeon's two gates
down ~a day, Z80 no real consumers, binary needs rebuilding). His words:** *"Yeah just proceed. We fix
when we come across it, if we don't we build later but this is really just to start building out the
tooling."*
**⚑ THE LAST CLAUSE IS THE GOVERNING ONE, AND IT REFRAMES THE WHOLE ACCEPTANCE CONTRACT.** The cutover
is **not** happening because the successor is ready — it is happening **because being reachable is what
generates the demand that builds it out.** So the remaining 17 are **not a checklist to burn down
before the switch; they are a queue the switch POPULATES in priority order.**
**▶ CONSEQUENCE FOR EVERY BRIEF FROM HERE: an early gap is a SUCCESS SIGNAL, not an embarrassment.**
State it explicitly to agents — the instinct will be to treat every `-32601` as a failure that should
have been prevented, and under this ruling it is the mechanism working. **This does NOT relax the
loud-failure requirement**; it is the reason for it. A gap that refuses by name feeds the queue; a gap
that degrades to a plausible answer poisons it.

**The mechanism, the precondition and the measured day-one breakage are in the log.** What stays live
is the clause above and one hazard, restated below where it belongs.

**▶ ALSO OPEN, and it points INWARD at our own docs (aurora ran it on theirs and it drew blood).**
Their split: **engine facts** (properties of aeon, seen through a window — unaffected) vs **server
facts** (properties of ONE implementation). Their worked example is the one to internalise: they
re-derived the `require_paused` set **from our Rust source**, banked it as a correction — and wrote it
into a section that had always called these properties of *"the bus"*. **It is a property of one
implementation and the correction did not say so** — a defect created *while applying every other bar
correctly, in the act of fixing a different staleness.* **We have the same exposure and more of it:**
this repo's recon, demand and CR docs describe "the server" throughout, and D-10/D-13/D-17 are already
booked as having **two implementers**. A sweep is owed — every claim about server behaviour either
names its implementation or is a latent two-implementer conflation.
*Durable formulation from the same thread, worth more than its instance:* **freshness is not
transitive across a document, and proximity reads as verification** — a stale figure beside a
freshly-updated one is read as cross-checked, which is how my own 37 survived hours next to a correct
18.

## ⚑ THE SOCKET CHAIN, AND F-CHAIN-QUOTED

**There is no chain.** `empyrean/clients/python/aether.py`'s resolver commits on a *directory* test, so
every lane resolves to `$XDG_RUNTIME_DIR/oracle.sock` and stops; `/tmp/oracle.sock` is unreachable dead
code. The spec specifies a chain and the reference client never implemented one — a conformance gap,
not a stale comment. **Operational consequence: start a server on `/run/user/1000/oracle.sock`.**
**F-CHAIN-QUOTED stands:** two historical recon docs here name the socket paths and neither was written
from the resolver. Revival: before any doc here is cited to a peer as the transport's behaviour.
Full measurement, and what each of the three lanes had right and wrong, in the log.

**OPERATIONAL CONSEQUENCE for the d-4 parcel: start the server on `/run/user/1000/oracle.sock`.** That
is what every lane resolves to. Unlinking the stale `/tmp/oracle.sock` (aurora's suggestion) is **not
required** for any consumer using the reference client, since it is unreachable; it may still matter
for a client with its own resolution, which is aurora's to determine and not mine to touch.

*(The shim-half measurement is in the log. The hazard it leaves behind is live and is this:)*

**THE HAZARD TO ENFORCE, and it is this seat's job:** once the new server is the only one reachable,
every failure presents as *the consumer* being broken and the gradient pushes lanes to engineer
around gaps instead of reporting them — **bar 9's corollary with the causation hidden.** Counter-
measure: **an unserved method must fail LOUDLY and BY NAME, never degrade to a plausible answer.**
Ours already does (`-32601` unknown method; `-32602` naming the key at the dispatch choke *before*
the handler); the legacy server silently defaults unknown params, which is exactly why bar 15 says
sequence a cutover onto the STRICT implementation. **A missing capability that returns something is
far worse here than one that refuses.**

## ⚑ SIGIL CYCLE DUMPER — my join objection is REFUTED; two requirements survive it (2026-08-24)

**Do not re-raise the join objection.** I priced the differential as blocked on *who supplies the
opcode-to-key join*, reasoning that sigil's `instr_cycles` is keyed on mnemonic + size + EA category
and holds no opcode (true, and derived here without contact). **The conclusion over-reached.**
Verified firsthand at sigil `origin/master` **`4b02eb07`** — their committed revision, not their live
worktree: `m68k_decode.rs::decode_one`, `m68k.rs:180 pub struct Instruction { mnemonic, size, ops }`,
`m68k_cycles.rs::instr_cycles`, and `sigil-frontend-emp/src/m68k_cycles.rs:130 fn classify(op:
&CodeOperand, …)`. **They own both halves.** The real gap is one adapter between two of their own
types, inside one repo. *My premise was right and I turned "the mapping lives in your decoder" into a
cross-repo ownership problem without checking whether the decoder existed — it had landed the week
before, in the very parcel whose Capstone precedent I had just praised in the same message.*

**TWO REQUIREMENTS THAT BIND OUR DUMPER, both cheap up front and awkward retrofitted:**
1. **⚑ BRANCH OUTCOME PER EXECUTION, not just a cycle count.** `CycleCost::Branch { taken, not_taken,
   exact }` is **outcome-keyed** (verified at `m68k_cycles.rs:103-109`), so a measured count cannot be
   compared to a `Branch` row unless the dump says which way the branch went. **This is a real change
   to what would otherwise have been built.**
2. **The assertion comes from the DATA, which beats my framing.** Rows carry `exact: bool`, doc'd at
   `:92-93` verbatim: *"`exact: false` marks a MAXIMUM over a data-dependent execution — sound as a
   ceiling, unusable as an equality."* So the gate is `measured <= modeled` on inexact rows and `==`
   on exact ones, **read off the flag rather than chosen**. My "≤ direction only" was right and was a
   convention; theirs is a property. *This satisfies our own name-the-assertion bar out of the data
   instead of by fiat, which is the stronger form of it.*

**THE NUMBER THAT DECIDES WHETHER TO BUILD AT ALL, sigil's own and stated against their interest:**
`CycleCost::Unmodeled` exists (`:112`), so the differential's domain is **partial by construction**.
The gate is **what fraction of a real ROM's instruction stream is modeled**. Unmeasured; theirs to
measure; **it comes before either lane spends a parcel.** A differential over 20% of the stream is a
different proposition from one over 95%.

**Status: still `blocked` on `no filed ask exists`** — correctly, and sigil is filing one. The
consumer (Spec 2 cycle budgets, `SIGIL_SPEC2_LANGUAGE.md` S2-D7(c)) is **deferred at the spec freeze**,
so there is no sigil-side consumer this week either.

**⚑ THE COVERAGE NUMBER HAS THREE BUCKETS, NOT TWO — sigil's correction to this seat's booking,
2026-08-26, and it changes what the dumper would be FOR.** I had booked the gate as "what fraction of a
real ROM's instruction stream is modeled", i.e. modelled-vs-unmodelled. They corrected it against their
own tree: because `exact: false` marks a **ceiling** rather than an equality, a routine can carry a
`@budget(cycles: N)` bound while being permanently unable to carry `@cycles_exact`. So the honest split
is **exact-modelled / ceiling-only / unmodelled**, and the three answer different questions — *"a corpus
that is 80% modelled but mostly ceiling-only supports a budget checker and not an equality checker"*.
**That distinction is precisely what decides whether either lane should build this at all**, and a
two-bucket number would have looked like an answer while hiding it. The comparison target differs per
bucket, so our dumper's assertion direction is per-row and not per-run — which composes with the two
requirements already booked above rather than replacing them. They also settled, firsthand, that the
cycle model is **entirely static** (`cycle_budget.rs` walks an evaluated `CodeBuf` against the cost
tables and the shared `Cfg`), so the coverage measurement needs no emulator and was never blocked by
the shim hazard above. CYCLE-ASK stays correctly gated on their owner picking the measurement up.
⚠ **STATUS CORRECTED BY SIGIL THE SAME DAY, against their own contribution — the three buckets are a
PREDICTION, not a measurement, and the paragraph above overstated them.** They have run no corpus
measurement. The split is a *consequence of `CycleCost`'s own doc comment* (`exact: false` = "a MAXIMUM
over a data-dependent execution — sound as a ceiling, unusable as an equality"), read while checking
this seat's two requirements. **So it is the shape to measure IN, never a result to build on: if the
corpus turns out to have a negligible ceiling-only middle, the sharpening was true and irrelevant, and
the original two-bucket question was the right shape after all.** Do not let a later session cite the
three buckets as a finding about any ROM.
*Their framing of where the value actually came from, kept because it is the more useful lesson and it
is narrower than the headline: a precise question about their own tree sent them to read their own type
definition, where the answer was already written down. That is bar 12 arriving between repos —* the
rule was in the contract they already owned.*


## ▶ LAYER-MASK — LANDED. One safety property survives it and must not be "finished".

**`render_scanline` — the one render that commits sprite-overflow/collision latches and the R10 carry —
takes no mask and has no masked twin, so "a display mask cannot perturb emulation" is enforced by the
type system. Do not add a mask parameter to it.** Design calls and the resume path are in the log;
`docs/2026-08-26-layer-mask.md` is the artifact of record.

## ▶ QUEUED — GUI-LAYERS: the player window's layer toggles + click-an-object

**Recommended to the owner 2026-08-26, not yet picked.** The bus can now hide layers; `oracle-frontend`
cannot. It draws its own window and `pick.rs` resolves attribution **unmasked**, so a bus-set mask
changes the bus's answers and not the picture — and `pick.rs`'s *"this panel and
`emulator/pixel_attribution` must never disagree"* invariant is now **conditional on no mask being
set.** This is also the natural home for the owner's unqueued *click an object and be told what it is*.

**⚑ CONSUMER DESIGN INPUT, solicited from aurora BEFORE shaping the parcel and adopted — their
priority order, kept.** *(Asked for deliberately: they are the editor and the only lane that would
consume this. Cheaper before the design than after.)*
1. **The answer must name its own subject, in a sentence.** Their expensive lesson the same day: they
   shipped a band lens that highlighted 1,244 cells, entirely correct, and the owner's reaction was
   *"what are the purple boxes"* — not *that's wrong* but ***what is that***. A feature that works
   perfectly and communicates nothing. So the top line is prose a person reads; the verdict enum stays
   underneath for tools. `[planeB:won, backdrop:lostToPriority]` is the right data and the wrong answer.
2. **Return identity a client can JOIN ON. ⚑ ALREADY SATISFIED — do not build it.** They asked for the
   nametable word at the dot; `pixel_attribution.cell` already returns it decoded (`tile`, `tileAddr`,
   `palette`, `hflip`, `vflip`, `priority`), iff the winner is planeA/planeB/window. **Verified live at
   `d285ecb`**, not read off the schema: `(160,100) → tile 1066, tileAddr 0x8540, palette 2, vflip`.
   **`tile` is VRAM-ABSOLUTE** (`tileAddr == tile*32`, checked: 1066×32 = 0x8540), so aurora rebases by
   `BG_TILE_BASE_SLOT` for blob-local — their model's space, and the direction their injector already
   goes. Their warning is the durable part: **an index whose space is unstated is a transpose bug
   waiting to happen**; the fragment states the space in the field's own description, which is where it
   belongs. Filed as *satisfied*, not *genuinely-new* — the triage exists to catch exactly this.
   **⚠ AND THE HAZARD ON THE OTHER SIDE OF THAT JOIN, found by aurora against this seat's own sample
   dots — if OUR panel ever names a blob slot, it inherits this check.** The rebase can land **outside
   the blob**, and `BG_TILE_BASE_SLOT = 1024` (verified firsthand as a literal at aurora
   `origin/master`), so **any `tile < 1024` rebases NEGATIVE.** Worked on the two dots this lane
   sampled: `1066 → 42`, inside their 320-tile blob; **`1456 → 432`, outside it** — and *not rescued by
   capacity*, since 432 < 448. **Their durable formulation: in-capacity is not in-blob.** *(Base slot is
   ours firsthand; the 448 capacity and the 320-tile blob are their measurements, not re-derived here.)*
   Plane B can legitimately be showing engine art, another act's art, or a slot past the blob's end —
   the corpus-ROM sample is not a defect, it is proof the class is reachable. **So a click-to-identify
   surface must answer *"that is not part of your background"* for those — not index, and above all not
   guess: an unchecked rebase either throws or confidently names a slot the author does not own, which
   is indistinguishable from a correct answer.** That is point 3's loud-on-unmeasurable rule arriving on
   the join instead of on the mask, which is what makes it a class rather than two tips.
3. **Assert the conditional invariant rather than noting it.** A rule with an unasserted precondition is
   this workspace's recurring defect; their form is **loud-on-unmeasurable beats a plausible answer** —
   their layout harness answers *"COULD NOT MEASURE A FIT"* under a planted defect rather than "fits".
   So while a mask is set the panel SAYS so in the answer rather than quietly describing a picture that
   is not on screen, and the human-facing line carries it, not only the wire caveat.
4. They will honour the framebuffer-digest ruling and not fingerprint a masked view.
5. **A lens must state that it is on, persistently, and this is a CORRECTNESS requirement rather than
   polish** — their point, and it had not been on this seat's list. A mask that changes the picture with
   no standing on-screen statement is the unlabelled-highlight defect one level up: *the author will
   forget, and then read a masked picture as the real one.* Their canvas palette treats colour as a
   language deliberately; a toggle that fights that is worse than none.

*(The three 2026-08-30 parcels, the restamp A/B and REPLAY-NET-BLIND-3 are closed and in the log.
What was left open by them is here:)*

**Open, booked, not started:** `F-ACCEPT-TABLE-RAWSTRING`; `README-LEGACY-WARNING`; `FRAME-LABEL`;
`PLAYER-POLISH`; `OVERLAY-STATE` (never run against a real window; waits until the owner is away);
`ACCEPT-16`; `WIKI-SPIKE`; and **`d-20`, the dull sound — the owner's taste call, untouched.**
Residue from parcel 2, deliberately out of scope: `tools/aether_smoke.py` and several
`crates/oracle-core/examples/*` still read aeon's **live** tree and pin `symbolCount == 2129`
against it — the same dependency `fixtures/aeon/` exists to remove, in the places the freeze did not
reach.

## ⚑ HUB RULING, 2026-09-02 — HERMETIC GATE IS THE RATIFIED SHAPE; DRIFT IS A NIGHTLY, AND IT GETS **NO SECOND OWNER CARD**

⚑ **RELAYED BY empyrean-01, NOT WITNESSED BY THIS LANE** — same flag, same reason, as the relayed
rulings above. It is the **hub's** ruling under the owner's standing delegation. Do not upgrade it to
his. Anchored at empyrean **`1e9d70c`**, verified firsthand here rather than taken on trust: the object
is a **commit**, it is an **ancestor of their `origin/main`**, and `--stat` shows it is a **docs commit
carrying a docs ruling** — so its SHA class matches what it anchors.

**The question this answers** is the one `parcel/stopprecision` left open in
`docs/2026-09-02-stopprecision.md` §6: our schema gate went hermetic (blob-pinned, no peer read), and
the deliberate cost recorded there was that **a default run no longer notices upstream moving on its
own**. **Ruled: the hermetic default is the ratified shape, and drift detection is a NIGHTLY's
property, never a local run's.** Same shape as sigil's decouple — vendored content plus a revision
stamp, local runs hermetic, drift watched out-of-band.

**THE OPERATIVE INSTRUCTION, and it is a prohibition — read it before filing anything:** the drift job
is **a queue row here, not an owner card.** The host question it would raise (a standing unattended
timer on the owner's machine) is **already open with him as empyrean `d-9`** — verified firsthand at
their `origin/main`, and its question is literally *"Running it means a systemd timer on YOUR machine
… Do you want that standing job installed?"*, which is the same question ours would ask in different
words. **One cross-lane question gets one card.** A second card does not add information; it makes him
answer the same thing twice and lets the two answers diverge. *(`d-7-restated-3` is the companion card
— how many quiet chains before review — provisionally ruled N=5.)*

**Board row id: `SCHEMA-DRIFT-NIGHTLY`** — this section is that row's detail, per `LANE_STATUS.md`
rule 7 (a title states the state; the history lives here and the row points at it by id).

**The shape to build, when it is picked up:** a runner with `AETHER_CONTRACT_REPO` set, **non-blocking**,
reporting *"contract advanced past pinned blob"*. Note it needs **no new capability** — the hermetic
gate already grew exactly that env-var path as step 2 (`schema_conformance.rs`), so the nightly is a
caller of a road already built, not a build.

**Also carried in the same message, both banked:** our landing is recorded upstream with attribution
(**2026 passed / 0 failed / 6 ignored** cited as *our* measurement, not re-derived by them — correct
attribution discipline); and **F-RESUME-STOP-RACE was relayed to aurora** as the suite's outbound
client, which is the right destination — that register entry names `tests/breakpoints.rs` and
`tests/watchpoints.rs` as still carrying the racy spelling, and aurora writes clients that will hit the
same read-through-discarding-events shape. **No reply was requested and none is owed.**

⚠ **The one thing verified against MY OWN interest, because the relay asserted it and this seat's bar
says a claim about your own tree gets read out of the file:** the blob identity is **content-addressed
and therefore not talkable-into-agreeing** — our vendored
`crates/oracle-aether/tests/contract/bus-protocol.schema.json` is `125d17f03ac33872…` at our `HEAD`,
and `git rev-parse 82982b7:contract/schema/bus-protocol.schema.json` in empyrean returns **the same
blob id**. Byte identity by construction, checked in both trees, neither read from a working file.

**Board row id: `ATTR-RGB-LATCH`** — detail lives in `docs/2026-08-30-rgb-live-resolve.md`
(aeon's colour finding: reproduced 55/55, closed as a server change; what remains is a contract change
so the reply says which moment its colour is for and names `emulator/scanlines` as the caller's path).
Anchored here 2026-09-02 because the row's own title carried the only copy, and `LANE_STATUS.md` rule 7
requires the row to point at its detail by id.

⚑ **AND THE SCOPE OF WHAT THE HUB CAN HAND US, ESTABLISHED 2026-09-02 BY THE HUB RETRACTING ITS OWN
GO — bank this, it will recur.** At 10:40Z the hub cleared `LIVE-TREE-RESIDUE` "under the owner's
widened delegation". **This seat held anyway** and the hub then **withdrew it against its own
interest**, which is the strongest form this correction could take.

**The test that decided it, aeon's, and it is the reusable part: *is there an owner decision under
this, and is it THIS question?*** Applied here: the owner's 03:22Z words (empyrean `63c85ae`) re-arm
the **raster/parallax effects** drive; his 03:46Z widening (empyrean `4e8e865b`) covers **decision
CARDS in a lane's domain**. Neither is a word to start a **non-effects** parcel, and the live-folder
cleanup is this lane's own hygiene row. The nearest owner decision under it is `d-17` (his — the
*write* side into his aeon folder); the *read* side, sigil's `d-18`, **was ruled by the hub under
delegation, not by him.** So no owner decision exists for this question, which is exactly what the
test asks.

**THE DURABLE SPLIT, and it is what a future session should apply without re-deriving:** under today's
brief the hub can hand this lane **any ask an effects lane files** — those need no owner word and
should just be worked. It **cannot** hand us a go on our own hygiene, our own backlog, or anything
outside the effects drive. **When a relay's go and this test disagree, the test wins and you ask the
owner.** Precedent from the same morning, banked in the hub's own record at empyrean `13a7d5a`: sigil
adopted a hub *ruling* while holding for the owner's *word* on the same shape — **a relay carries a
ruling, never an authorization.** Two lanes, same hour, same conclusion, reached independently.

## ⚑ 2026-09-02 — `run_to` DOES **NOT** SHARE `resume`'S THREAD SPLIT. Measured from source, answering aurora.

**The question, theirs (relayed via the hub, 2026-09-02):** their Build-&-Run boot restore awaits the
`emulator/run_to` reply and gates on `reached`, assuming the machine is **already halted** when that
reply lands. If `run_to` had `resume`'s split, their next call would land on a running machine. They
correctly noted it would fail **loudly** (`write_memory` is behind `require_paused`, so it refuses by
name) — a diagnosis cost, not a corruption risk. **Answer: their assumption holds, and structurally.**

**The mechanism, read at `2a7fd82`, and it is one thread doing two things in order.** `engine_loop`
(`server.rs`, spawned at the `engine thread` builder) handles one `EngineMsg::Call` at a time as
`engine.dispatch(&method, &params)` **then** `reply.send(CallResult { .. })`. So:

- **`run_to` blocks INSIDE `dispatch`.** It `require_paused`es, sets `self.running = true`, calls
  `advance_until(max_frames, |pc, _| pc == target)` — which does not return until the target, the
  bound, a breakpoint or a `stopAfter` watch ends the run — sets `self.running = false`, emits
  `stopped`, and only then builds its result map. **`reply.send` therefore cannot execute before the
  halt: the reply is PRODUCED BY the halt, not merely correlated with it.**
- **`resume` does not block at all.** Its whole body is
  `Ok(json!({"wasRunning": self.set_free_run(true)}))` — it flips a flag and returns. The halt happens
  later, on a subsequent `free_run_step()` in the loop's `None` branch, and `emit_stopped` broadcasts
  from the engine thread **after** the `resume` reply was already sent. That gap is the whole of
  F-RESUME-STOP-RACE.

**⚑ THE CAVEAT THEY DID NOT ASK FOR, AND IT IS THE INVERSE OF THEIR WORRY — worth more than the
answer.** `run_to` calls `emit_stopped` **before** it builds its reply, so **on the wire the `stopped`
event PRECEDES the `run_to` reply.** A client reading through to the reply and discarding events (what
aurora does, and what `Client::ok` does) is correct and unaffected. **A client that consumed the reply
and THEN waited for the halt event would block forever** — the same read-ordering hazard as
F-RESUME-STOP-RACE with the two halves swapped. This is exactly the shape a first breakpoint consumer
reaches for, so it is recorded before anyone writes one.

⚠ **SCOPE, MEASURED RATHER THAN ASSERTED** (this seat's own booked habit of reading a sound observation
one notch too wide): the above is the **socket/free-run driver**, which is the path aurora reaches. The
**hosted** path — the player window through `Host::pump` — is a different driver and is already
registered as `F-STOPPREC-HOSTED-HALT`, where the halt is *inferred* from sharing one function with the
measured path rather than measured. Nothing here upgrades that.

**Their side is clean and it is a real enumeration, not a did-not-find:** `AetherClient.onEvent` has
exactly one non-test consumer in their tree (`bridge.ts`, which refreshes a badge and discards the
event), nothing awaits an event anywhere, and every sequencing point gates on a **reply**. **Their
perishable half, stated by them and adopted here: the day they build a breakpoint consumer is the day
our server-side fix has to be in.** That is now a board row rather than a note.

⚑ **AND THE RETURN LEG, 2026-09-02 — `run_to.reached` NOW HAS A NAMED LIVE CONSUMER, WHICH IS WHAT
PROTECTS IT FROM A FUTURE TIDY-UP.** aurora re-read the body order at `7ba2faf` themselves rather than
adopting our answer (confirming it an ancestor of our `origin/main` first), and their stated reason is
the sharp one: *a claim about another repo's tree is the one class of claim nothing in my tree could
ever contradict.* That is bar 20's receiving side run correctly, and it is why the answer is now
corroborated rather than merely believed.

**The part that comes back to us as an obligation.** Their boot restore gates on `reached !== true`.
So `"reached": run.predicate_fired` — **the predicate's own verdict, never the sink's** — is no longer
a defensive design choice explained in a comment; **a real client's write window depends on it.** The
comment at the site already says why (`StopRecord::fired` means only "*something* asked to stop", so
reading it would report a target as reached because an unrelated `stopAfter` watch halted the run).
**Booked here because this file's own bar says a code comment is where a perishable rule goes to be
read by nobody** — and the "simplification" that swaps `predicate_fired` for `fired` would now break a
named consumer silently, in the direction that presents as a successful boot restore over a write
window that never opened.

*They also noted it was an uncited joint in their own code: they had verified they read a reply rather
than an event, and never asked whether the field they read could be true for the wrong reason. Bar 8's
cheap frame-changer — the load-bearing step nobody cited — arriving on a consumer's side.*

## The bars (house methods — each earned by a measured failure; do not thin)

**▶ NEW BAR, 2026-08-26 — A MERGED SERVE IS NOT A SERVED METHOD. THE CONSUMER REACHES A BINARY.**
Found in the foreground pass that closed the CR-D `⟨RUNTIME⟩` debt
(`docs/2026-08-26-runtime-decoders-check.md` §5). The object decoders merged, tested and pushed at
`0f33c44` — and stayed **unreachable to every consumer**, because `target/release/oracle-aether` was
still the build from the day before and **nothing in a merge rebuilds it**. The MCP shim spawns *that
binary*; so a shim spawned any time between the merge and the check answered
`[-32601] no such method` to the very methods we had just shipped. Reproduced firsthand on this
session's own shim, then fixed and re-verified end to end through the consumer's own spawn path.
**This sharpens, and does not contradict, the coordination note that *advertising a method is
shipping it*: the advertised list is authoritative, but it is emitted BY A RUNNING BINARY, and a
stale binary advertises a stale list with total confidence.** Practical check before telling any
consumer a method is available: spawn the consumer's own path and call it — not `cargo test`, which
passes against source the consumer never runs. Same family as item 1's rename fallout
(*compile-time-frozen paths, invisible until the binary runs*); here the frozen artifact was the
binary itself.

**▶ AND THE COUNTING BAR THAT CAME WITH IT — MEASURE USE, NOT ATTACHMENT.** The same pass had to
count whether consumers actually call these methods. `grep -c` over the transcript tree reports
~10,000 mentions across ~4,055 files — and reports **the same ~4,055 for every tool name, including
tools nobody has ever called**, because the MCP tool listing sits in every session's system prompt.
Parsing `tool_use` blocks instead gives the true figure: 216 invocations. **The near-constant across
varied inputs was the tell** — the existing bar caught it. Mentions measure attachment; only
invocations measure use.

**▶ NEW BAR, 2026-08-24 — ANCHOR A CLAIM TO A SHA THAT CAN CARRY IT. A docs commit cannot vouch for
code.** Caught by aeon against this seat, same day. I reported the straddle fix to them anchored to
`7bdb75f` — which is a **one-line `docs/lane-log.jsonl` commit**. Every claim I made was true, and
the anchor could not carry any of it: the code is `4111c88` under merge `51143a5`, tests `68461a7`.
They cited the code SHAs in their booking instead. **The failure mode is that it hardens invisibly** —
a peer transcribes the anchor into their prose, and a later reader who checks it finds a docs diff
where a guarantee was promised. This is the same family as the provenance audit above (*cite the
ruling, not a status field*): the citation must be the artifact that actually contains the thing.
Practical check before sending: `git show --stat <sha>` and confirm the files named are the ones the
claim is about.

**▶ AMENDMENT, 2026-08-27 — THE ABOVE BAR HAS A FALSE-POSITIVE MODE, AND THIS SEAT FIRED IT AT A PEER.
A RIGHT SHA ANSWERING AN UNSTATED QUESTION IS NOT A WRONG SHA.** Found by aiming the 08-24 bar at aeon
and being half right; the diagnosis below is theirs, banked by them at aeon `b64f6bcb` (verified here as
a reachable ancestor of their `origin/master`, docs SHA carrying docs).

I flagged their certification anchor `33d905b8` as a docs-only commit standing in for a byte guarantee.
**The observation was right and the diagnosis was wrong.** A freeze record names **`aeon_rev` — the tree
state the ROM was built from** — and that is the *correct* anchor for reproducibility. It is *frequently*
a docs commit, because the tip at freeze time is whatever happened to land last. Nothing was false.

**The two are distinguishable, and the distinction picks the remedy:**
* **08-24 (mine):** the **wrong** SHA — a lint fixup standing in for a feature merge. `git show --stat`
  **catches it**, because the files named are not the ones the claim is about. Remedy: **swap the SHA.**
* **08-27 (theirs):** the **right** SHA for an **unstated question**. `git show --stat` **cannot catch
  it**, because the commit you land in is genuinely the one named — the sentence simply does not say
  *which of two questions* its SHA answers (what carries the code? vs. what tree was it frozen from?).
  Remedy: **a label, not a swap** — `code <SHA> · frozen at aeon_rev <SHA> / sigil <SHA>`.
**So the 08-24 practical check is necessary and not sufficient, and it is worse than that: applied alone
it produces a confident FALSE POSITIVE on every correctly-recorded freeze in the suite.** Ask what
question the SHA is answering before judging whether it can carry the claim.

**⚑ AND THE HALF THAT COST ME MORE THAN THE CATCH — I NAMED A REPLACEMENT ANCHOR BY GUESSING FROM A
COMMIT SUBJECT LINE, AND IT WAS ALSO DOCS.** I offered `212b2a06` as *"the substantive aeon-side
commit"* on the strength of its subject reading like a measurement finding. `--stat` says it is **a
single 509-line doc, zero code**. The real code anchor is **`cbd04ba8`** (`engine/objects/sprites.emp`
+121, `engine/ram.emp` +20, `tools/test_sprite_owner.py` +281; 532 insertions) — all three verified
firsthand here. **Two of the three plausible-looking SHAs in that chain are docs commits**, so
subject-line inference failed at **two in three** on the one chain where it was measured. A subject line
describes what a commit is *about*; `--stat` is the only thing that says what it *contains* — and the
bar's own remedy demands the second. **Run `--stat` on the SHA you propose, not only on the one you
doubt.**

**⚑ THE PROCESS LESSON, which aeon called out explicitly and which is why this was cheap: I sent it
HEDGED — *"treat this as a reading, not a finding"* — and that is what made it worth sending.** It was
50% right (symptom yes, diagnosis and replacement no). Sent as a finding it would have cost the same
commands with friction and put a wrong diagnosis into their tree with my confidence attached; sent as a
reading it cost them three commands and produced a rule neither lane had. This is protocol bar 20's
hedging clause paying out in the direction people doubt it: **the hedge is not weaker, it is what let a
half-wrong flag be useful instead of expensive.**

⚠ **Corrects a claim already committed in this repo:** `docs/lane-log.jsonl`, the 2026-08-27T10:09Z
entry's `detail`, calls that anchor *"same anchor-class bar they caught this seat on 2026-08-24"*. It is
not the same bar. That file is append-only, so this paragraph is the correction of record.

**▶ AND THE SCOPE-MARKING BAR IT ARRIVED WITH, which is aeon's and is the more reusable half.** Their
mis-filed ask traced back to a sentence **in our own module docs** — *"`self_cycles` has no such
lag"* — that is true of routine rows and false of interrupt buckets and **did not mark which it
meant**. They carried it across the boundary; the sentence let them. Their framing, worth keeping
verbatim: *"a relayed premise inherits no more scrutiny than the claim it supports."* **A rule that
is true of one kind and silently false of another must say which at the point it is stated**, not in
a later paragraph a reader may never reach. Fixed at source in `profiler.rs`. Also theirs, and
sharp: they sorted the gap **from our wire schema** (no such key, therefore genuinely-new) rather
than **from the quantity they needed measured** — and a schema can only tell you whether a *name*
exists, so sorting from it lands in the expensive bucket by construction.

**▶ NEW BAR, 2026-08-24 — `docs/lane-status.json` is the OVERSEER'S file. Never let a dispatched
agent edit it, and say so in the brief.** Earned the same day: the Q-PROF-STRADDLE agent did
excellent work and, closing out, marked its queue item `"state": "done"` — an enum the suite
contract does not define. The Dominion console **rejects the whole document on one bad enum**, so
that single word would have made this lane invisible on the owner's board for the second time in
one night. The agent could not have known: the valid states live in `empyrean/contract/LANE_STATUS.md`,
not in this repo, and nothing it could read locally would have told it. **The fix is structural, not
educational** — a live operational file that the console parses is not part of any work product, and
handing it to an agent puts a contract the agent cannot see in the path of a commit it must make.
Agents report their queue outcome *in their report*; the overseer transcribes it. Related: a
finished item **leaves** the queue — `done` is not a state, it is an absence.

> ⚠ **READ THE PROTOCOL AT A COMMITTED REVISION, NOT THROUGH THE PATH** (seraph's rule, empyrean
> `origin/main` — the most upstream rule in that document): `../empyrean/docs/OVERSEER-PROTOCOL.md`
> is **one peer's live working tree**, so booting by path delivers the suite's shared contract by
> reading somebody's uncommitted directory. Use
> `git -C ../empyrean fetch -q origin && git -C ../empyrean show origin/main:docs/OVERSEER-PROTOCOL.md`.
> Correct citation discipline applied to a bad source produces a **more** convincing artifact, not a
> less convincing one. **This session's own boot is the measured case**: it read the file by path
> while empyrean held sixteen unpushed commits, and got the right bytes only because their worktree
> happened to be clean at that minute — 59 lines landed in that path minutes later, and the file
> reached **422 lines by day's end against the 245-line snapshot the session booted on**. Right
> answer, by timing luck, with nothing in the output saying so.
>
> The commentary on protocol bars 8-15 moved to the log: they are read at boot from the protocol
> itself, at a committed revision, which is the only copy that cannot drift. The stanza above stays
> because the protocol's own bootstrap exception sanctions it.


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
- **⚑ RED-FIRST IS NECESSARY AND NOT SUFFICIENT — a poison can come back GREEN with the guard
  perfectly sound** *(2026-08-22, aurora; three green poisons in one parcel, none of them a bad
  guard)*. The three classes: (1) the row aimed at a branch **a pre-check makes unreachable**;
  (2) the row proving only *"it refused"*, which **two independent code paths** both satisfy — so
  deleting the guard under test leaves the *other* mechanism holding it green (the matcher clause,
  but the collision is two **paths** producing one observable, not two messages sharing a phrase);
  (3) **the row measuring the WRONG OBSERVABLE** — the fixture left the thing resolvable, so the
  catch site the test was *named after* was never entered. **Planting a violation could not have
  revealed the third; only asking whether it measured the right quantity could.**
  **⚑ THE TEST FOR WHETHER A SPLIT LIKE THIS IS REAL — aurora's, and it generalises past poisons:
  do the two classes have DIFFERENT FIXES?** A matcher collision is repaired by re-pointing at wording
  only that rule uses. Two-paths-one-observable **is not repaired by touching the matcher at all** —
  the matcher can be perfectly precise and the row still worthless; it is repaired by asserting
  **which path ran**. *A bar that cannot tell them apart sends you to the wrong repair*, which is the
  cost of collapsing them. **Their tell for the confusable pair: is the observable UNIQUE to the
  rule?** Unique → the assertion is too loose (matcher). Not unique → the assertion may be exact and
  still prove nothing (two paths).
  **Operational form, to be asked per assertion:** *if this row went green for a reason OTHER than
  the rule holding, what would that reason be?* — then check that specific reason, and report the
  alternative green-path considered and how it was ruled out. **A `None`/absent/empty on either side
  of a comparison must be LOUD, never green**; that is where all three hid, each reading as healthy.
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
- **⚑ A CONFIDENT MECHANISM FROM THIS SEAT IS A HYPOTHESIS, AND THE RECEIVER'S OWN ALREADY-RUN
  COMMAND OUTRANKS IT** *(2026-08-22, found by the sigil lane against themselves; proposed upstream
  to empyrean, which is where it belongs — do not treat this entry as the rule's home)*. I sent a
  peer a confident mechanism for why three stale citations survived (*"they resolve into a different
  real repo and hand you a plausible wrong file"*). It was **wrong** — the leaves 404. My error was
  reusing a real lesson on an instance I had not measured. **Theirs was worse and is the durable
  half: they had ALREADY RUN the refuting command in the same session** — a directory listing and a
  probe that printed `No such file or directory` — **read the output, used it to conclude the cite
  was stale, and then wrote a row asserting my mechanism anyway**, because mine was a better-sounding
  story and arrived with a post-mortem attached.
  **Why this is a bar and not an anecdote: the second failure does not need a peer to be wrong — it
  only needs a peer to supply the frame.** A confident mechanism overwrites a measurement the
  receiver already holds, silently, and nothing in either session looks like a conflict because the
  measurement was never re-read.
  **▶ THE DELEGATION COROLLARY, which is the operative half for this seat and is strictly worse than
  the peer case.** A peer has standing to push back; **an agent has almost none.** Four of my stated
  facts were corrected today by agents who checked them — and every one of those was a *fact*, which
  is checkable. **A stated MECHANISM is far more dangerous than a stated fact**, because it explains
  the evidence rather than competing with it: an agent that measures something inconsistent with my
  mechanism will tend to reconcile the measurement *to* the story instead of reporting the conflict.
  **So: state mechanisms as hypotheses in briefs, explicitly labelled, and say in every dispatch that
  the agent's own command output outranks anything I asserted.** The instance that saved us here is
  the shape to demand — the README agent verified all three targets **individually before writing**,
  rather than performing the search-and-replace my framing implied.
  **Second clause, sigil's, and it prevents the over-correction:** when a correction lands, **check
  which half of the claim actually moved before discarding the whole thing.** The rename shape and
  the code-comment rule both survived my wrong mechanism intact, and retracting them along with it
  would have destroyed two sound rules to fix one bad sentence.
- **Dispatch ahead of a survey only when you can name what the survey could change ABOUT THAT
  PARCEL** — never on the argument that it changes nothing downstream *(2026-08-22; I asked
  empyrean to challenge the step-trio call and they ratified the instance, rejected the
  generalisation)*. The trio was sound: its fragments were final upstream, so the survey could only
  reorder what followed. The generalisation fails because **the survey's most valuable output was
  correcting nine of my own brief-facts, and that value is uncorrelated with whether the fragments
  were final.** Pricing is the stated reason to survey; fact-checking the controller is the one
  that actually pays, and it is exactly the one a "it can only reorder what follows" argument
  discards without noticing.
- **Never record an approval whose granting act you have not seen — cite the ruling, not a status
  field** *(2026-08-22, from empyrean, who found it in their own doc; see the Fable-seat audit in
  The role above, which is this lane's instance)*. Boot docs are snapshots that age while logs
  accumulate, and an owner ruling lands in the middle where head-and-tail reading never sees it —
  so **grep the history for an item before putting it to the owner OR funding work off it.** Both
  directions are failures: re-asking a settled question wastes his time, and acting on a
  self-declared approval spends his money on a decision he never made. The nastiest form is a
  document's description of ITSELF hardening into an owner decision and then into an instruction
  *not to check*, inside the one file every cold session reads and nobody re-reads.
- **Dedicated adversarial review** for load-bearing slices (the slice that carries an arc's central
  claim gets its own reviewer with explicit targets and required explicit negatives).
- **Better-than-the-floor** on every request; improvements additive so the migrating consumer
  loses nothing; the pre-release window for REQUIRED additions shuts at first ship — spend it
  deliberately, once.
- **A gate described for someone else to carry must name its ASSERTION, not its shape.** Earned
  2026-08-23 with aeon, on both sides in one exchange. Our reconciliation identity is a **loss**
  detector, not a correctness proof: a suppressed interrupt bucket *conserves* its cycles into
  `unattributedCycles`, so **the identity closes with that term arbitrarily large** and closure alone
  is satisfied by exactly the case the gate exists to catch — only the explicit `== 0` assertion
  fires. A peer booked the requirement as *"carries the identity check"*, having read the proof, and
  would have shipped a **correctly-described gate whose teeth were gone**: not a wrong gate, not a
  missing one, a gate whose shape a porter inherits with no reason to look under it. Note where this
  bit: inside the very booking written to argue that a mechanism beats a remembered rule. **The
  mechanism only beats the rule if the assertion survives transcription** — so when writing a gate
  into prose for a consumer, name the thing that fails, and re-derive rather than paraphrase when
  carrying someone else's.

**▶ NEW BAR, 2026-08-26 — CHECK THE VINTAGE OF THE PROCESS, NOT THE VERSION OF THE FILE. A long-lived
interpreter is a stale artifact class, and no in-tree check can see it.** Found by this seat while
reaching for an unrelated ⟨RUNTIME⟩ debt; corroborated independently by aurora, sigil, seraph and
dominion within the hour. `oracle-old` `07314aa` (08-25 21:09) made the MCP shim spawn its own private
`oracle-aether` and stop dialling the well-known socket. **Every suite lane's shim process started
08-25 19:53–20:29 — before that commit — and Python reads its source at process start.** So all six
lanes were executing the pre-ruling version, wired straight into `/run/user/1000/oracle.sock`, held by
the OWNER'S live `oracle-frontend` player. Proven by socket-inode pairing for oracle and aeon; aeon had
already `reload_rom`'d his running window onto a worktree build at ~18:20Z in perfect good faith,
**on a banked note that said a fresh session gets a private instance** — a note that was true of the
file and false of the process.
**This is yesterday's *a merged serve is not a served method* bar on a NEW artifact class.** That one
names compiled binaries and compile-time-frozen paths. This is neither: the file on disk is correct,
the fix is merged AND pushed, `git log` looks finished, and the defect exists only in the memory of a
running process. **No sweep, no audit, no cold read of the tree can reach it** — the tree is right.
**⚑ AND THE REMEDY IS NOT THE OBVIOUS ONE: a `/clear` does NOT fix it; only a session relaunch does.**
The shim is spawned by the session process, so clearing the conversation leaves the same interpreter
running. Measured firsthand here, and this is the cheap corroboration worth copying: **this session was
`/clear`ed and its shim's start time did not move** (shim 287372 at 20:29:19, one second after its own
session process at 20:29:18). aurora reached the same conclusion from the *other* direction — that the
shim is on the process command line — which is bar 19's genuine corroboration rather than echo, because
neither derivation could have shared the other's parameter.
**aurora's one-command discriminator, adopted: `pgrep -P <shim-pid>`.** A post-fix shim owns a child
`oracle-aether` on a `/tmp/oracle-mcp-*` mkdtemp socket; a pre-fix one has no child. Both kinds appear
in a single `ss -lxp`/`pgrep` listing, so pre- and post-fix sessions are **visibly different in one
command** with nothing to reason about.
**The failure that nearly happened to three separate lanes' documentation, and it is the durable half:
aurora's own `OVERSEER.md` asserted the opposite** — *"`mcp__oracle__*` in this session SPAWNS A PRIVATE
EMULATOR by default — it is NOT the window the owner is watching"* — written that same day, correctly,
**from the file on disk**, and false for every interpreter older than 21:09. They fixed it at
`83fcb64`. **A claim about RUNNING STATE banked as though it were a property of the code** is the
perishability preamble's sharpest instance yet: the anchor was valid, the source was authoritative, and
the sentence was still wrong the moment it was written.
**Operational form: before trusting any tool that dials something, ask when its PROCESS started
relative to the fix you are relying on** — and write the vintage condition into the note, never the
conclusion alone.


**▶ NEW BAR, 2026-08-27 — THE OPS LINE THAT IS NOT IN THE DISPATCH IS NOT IN THE DISPATCH. Carry the
worktree `vendor` symlink into every brief that will run cargo.** The Ops section below has said *"fresh
worktrees: `ln -s <repo>/vendor vendor`"* for weeks. It was **still missed on a dispatch this morning**,
because the brief is composed from the invariant block and the parcel's own grounding — and an Ops line
sitting in this file is not either of those. The agent lost time on a baseline that would not reproduce:
eight `save_state::tests::*` rows **panic** (not skip) on the missing vendored ROM, and the resulting
`exit 101` is indistinguishable at the aggregate line from two other causes this repo has recorded.
**The fix is structural, not educational** — an overseer who has read this file every session still omitted
it, so the rule is that the vendor line is part of the *brief template* for any cargo-running dispatch,
alongside the base check. Related and already booked: the same class as *a merged serve is not a served
method* — knowing a thing in the tree is not the thing reaching the process that needs it.


**▶ NEW OPS LINE, 2026-08-30 — NEVER CITE THE TIP. CITE THE COMMIT THAT CARRIES THE ARTIFACT, EMITTED
FROM THE PATH.** Third instance of the anchor-class family against this seat, caught by the hub.

I sent the hub `d5baac7` as CR-H's anchor. **It is a `docs/lane-status.json`-only commit.** The CR is
carried by **`d907fae`**. Both verified with `--stat` after the fact; the hub caught it before I did.

**The mechanism, and it is new — the two previous instances do not describe it.** 08-24 was the *wrong*
SHA (a lint fixup for a feature merge) and 08-27 was the *right* SHA for an unstated question. **This is
neither: I cited the SHA I had just pushed.** I committed the CR, then committed a status update on top,
then pushed, then quoted the push's result — so the tip was the status commit, and the *act of being
diligent about pushing before citing* is what put the wrong object in my hand. The push-before-you-cite
rule and the cite-the-carrying-commit rule pull in opposite directions at exactly this moment, and
nothing warns you.

**The corrective is constructive, not verifying**, because `--stat`-after-the-fact is what the existing
bar already prescribes and it did not fire — I had no reason to doubt a hash I had watched go out:

```sh
git log -1 --format=%h -- docs/proposed/2026-08-30-cr-h-screen-text.md   # the commit that carries it
```

**Never type or paste the output of `git push` / `rev-parse HEAD` as an anchor for an artifact.** Ask the
path which commit carries it. That form cannot produce this error, where checking can only catch it.

⚠ **And the reason it was cheap: the bad anchor lived ONLY IN MAIL.** `grep -rn d5baac7 docs/` returns
nothing — no in-tree reader could ever have met the contradiction, exactly as protocol bar 20 describes,
and the recipient was the only party able to catch it. **They did, which is the argument for citing a SHA
the receiver can actually resolve rather than one that merely exists.**


**▶ NEW OPS LINE, 2026-08-30 — A KILLED SUITE LEAVES A LOG THAT AGGREGATES CLEAN. COUNT THE LEGS, NOT THE
FAILURES.** Nearly quoted as a merge verdict by this seat.

A merged-tree verification was **killed at 46 of 61 legs** — cause unidentified (no rotation was due, no
peer announced a `pkill`, and this box has a recorded history of both). The log it left behind is the
hazard: `grep -E "^test result" | awk` over it prints a **confident `LEGS=46 … FAILED=0`**, which is
exactly the shape of a healthy result and is three quarters of a suite. **Nothing in the aggregate line
says how many legs there should have been**, so the one number that would reveal the truncation is the
one the house aggregate does not carry.

**This is the green-log-and-absent-run bar arriving on a PARTIAL run rather than an absent one**, and it
is worse than the absent case: an absent run leaves nothing to misread, while this leaves a page of
genuine passes. Every one of those 46 legs really did pass. The log is not lying; it is answering a
narrower question than the one being asked of it.

**Corrective, and it is cheap because it is one more line in the same command:** a verification asserts
its own **completeness** before its verdict —

```sh
LEGS=$(grep -cE '^test result' "$LOG")
[ "$LEGS" -lt 61 ] && echo "INCOMPLETE: $LEGS legs, expected 61 — NOT A VERDICT"
```

and the runner prints an explicit `CARGO_EXIT=` and `COMPLETE` marker, so **the absence of the marker is
itself readable**. Never take an aggregate from a log whose run you did not watch terminate.
⚠ Compounding factor recorded because it hid the first instance tonight: an earlier verification of mine
ended in `grep -c 'SKIP:'`, which **exits 1 when it finds zero matches** — the desired outcome — so the
harness reported the whole command as failed while the suite was genuinely green. **One command reported
failure on success, and hours later another reported success on a partial run.** A pipeline's exit status
and its subject's verdict are different facts; make the command say which one it is reporting.


**▶ CORRECTION, 2026-08-30 — OUR HEADLESS RECIPE'S "BOTH GUARDS" ARE ONE GUARD TWICE, AND IT IS THE
GUARD A PEER JUST MEASURED AS INEFFECTIVE.** Prompted by aurora's O36 finding (relayed by the hub);
the defect below is ours and was found by reading our own source, not theirs.

The banked recipe reads `env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE DISPLAY=:N … --x11`, and its prose
calls these **"both guards, because the failure is silent and lands on somebody else's screen"**.
**They are not two guards.** `--x11` is `force_x11`, and `main.rs:1040-1043` implements it as exactly
`std::env::remove_var("WAYLAND_DISPLAY")`. So the flag and the `env -u` do **the same single thing**,
and the recipe has no independent second guard at all — it reads as belt-and-braces and is one belt.

⚠ **And aurora measured that mechanism failing.** On this box `WAYLAND_DISPLAY=wayland-0`, its socket
exists under `XDG_RUNTIME_DIR`, and an Electron app started with the variable deleted **still reached
the owner's two real monitors** — the toolkit fell back to the literal `wayland-0` path rather than to
X11. **If minifb does the same, `--x11` does not do what its own `--help` says it does**, and every
windowed fixture this lane has run may have been on his desktop.

**NOT yet established for us, and the distinction matters:** aurora's measurement is **Electron/Ozone**,
a different toolkit from our **minifb** (we are not winit-class, which is what their message assumed —
`Cargo.toml:10`). Their result does not transfer by itself; minifb's Wayland backend is separate code
and may genuinely fail the connect when the variable is gone. **So this is a live hypothesis about our
player, not a finding about it.**

**The discriminator, aurora's and adopted — it needs no window on his screen:** ask the app for its
screen size from inside the fixture and compare against the Xvfb geometry actually requested. A match
proves the fixture owns the display; a mismatch proves it is talking to the real compositor. **Do NOT
measure the windowed case while the owner is logged in.**

**Consequence for the `screen_text` parcel, and it is the reason this matters today:** its T1 runtime
item is precisely *"never executed against a real window"*. **That item may not be dischargeable by the
recipe in this file**, and discharging it against an unvalidated recipe would put a window on his
desktop to prove a feature about windows. Settle the discriminator first, or leave T1 open and say so.


**▶ NEW OPS LINE, 2026-08-30 — DO NOT COMMIT WHILE A VERIFICATION RUN IS IN FLIGHT. IT INVALIDATES THE
BUILD-ID GATE AND THE FAILURE LOOKS LIKE THE PARCEL'S.** Third instrumentation failure in one night, and
the only one that produced a red that was entirely mine.

A merged-tree run came back **61/1988/1**, failing `server_build::the_compiled_in_build_id_still_names_this_tree`
— *"the compiled-in build id names a different commit than HEAD"*. **It was right.** The binary was
compiled at `8e88e2b`; I then landed two `docs/OVERSEER.md`-only commits **while the suite was running**,
so HEAD had moved by the time the assertion read it. Re-run on a stable tree: **61/1989/0/6, exit 0,
`HEAD_AT_START == HEAD_AT_END`.**

**Why this is worth a line rather than a shrug: the gate exists to catch a stale build-script product,
and a docs commit is exactly the change a person feels safe making during a long run** — it touches no
code, it cannot affect a test, and it is the natural way to use twelve idle minutes. The failure it
produces names a *build-script caching defect*, which is a plausible and completely wrong diagnosis; a
session that trusted the message would have gone looking at `cargo:rerun-if-changed` declarations.

**Corrective:** a verification prints `HEAD_AT_START` and `HEAD_AT_END` and **they must be equal for the
verdict to count**. Bank findings *after* the run, never during — the twelve minutes are not free time,
they are part of the measurement.

*Tonight's three instrumentation failures, kept together because the pattern is the point: one command
reported **failure on success** (`grep -c` exiting 1 on the desired zero matches); one reported **success
on a partial run** (a killed suite aggregating clean at 46 of 61 legs); one reported **a real failure
with a misleading cause** (this). In all three the suite itself was honest and the harness around it was
not. **The instrument that reports on the instrument is the one nobody tests.***


**▶ SCOPE CORRECTION, 2026-08-30 — `screen_text` DOES NOT END AEON'S EYEBALL REQUESTS, AND THIS FILE SAID
IT WOULD.** Corrected by aeon against a claim this seat made to them, which was taken verbatim from our
own queue row.

The OVERLAY-STATE row read *"the item that would stop aeon asking you to eyeball things"*, and I repeated
it to them as a headline. **It is wrong. `screen_text` reads the emulator's own CHROME — status line,
toasts, palette, lens, title bar — and every eyeball request aeon has outstanding is GAME PIXELS**: the
right-edge price of a column borrow, a background wrap, colour bands. **No `kind` in the adopted enum can
see any of them.** Their words, and the reason they sent it: they would otherwise have planned a parcel
around a capability the tool does not have.

**Where it genuinely helps, per aeon: the booked SCENE-READOUT row** — the owner counts button presses to
know which of twenty effects is on screen. **If that readout is drawn as debugger chrome, `screen_text`
reads it and the row is cheap; if it is drawn as game graphics, the tool cannot see it. Which one is
UNMEASURED**, and aeon has written it into their booking as a thing to check *before* planning around it
rather than assuming the convenient half.

**The durable shape, and it is why this is an ops line rather than a typo fix: the over-claim was in a
QUEUE TITLE, which is the one place nobody re-derives.** It was written when the item was a sketch,
inherited by every status file since, and finally exported across the fence with a seat's confidence
attached — where the only reader who could refute it happened to be the party it was about. **A queue
row's justification ages exactly like a precedent narrative and nothing re-reads it.** When an item's
design lands, re-read its own queue title against what was actually built.

**▶ F-ACCEPT-TABLE-CROSSCHECK-BLIND (registered 2026-08-30, emitter behaviour change, NEEDS A RULING).**
`tools/legacy_accept_table.py`'s axis-A/axis-B reconciliation adds to `claimed_lines` **before the row is
written**, so it is **structurally blind to a row-level drop**. Measured firsthand: with the four unguarded
`addr` rows dropped cleanly, `--fail-on-gap` prints `cross-check : AGREES`, `parse complete : yes` and
**exits 0**, while `UNGUARDED reads` silently falls **43 → 39**. ⚑ **So the tool's own headline safety line
is not evidence of the thing a reader takes it for** — it witnesses that every *access* was claimed, never
that every *row* survived. The 57-test suite catches this by named assertion; `--fail-on-gap` does not, and
`--fail-on-gap` is what a CONSUMER would wire into a gate. **Revival: aeon wiring the table into their gate
— they must be told the gate flag is weaker than the suite** (told 2026-08-30). Fix would be `--fail-on-gap`
independently verifying row presence against a source-derived expectation; that is an emitter behaviour
change and was correctly kept out of the hardening parcel.

⚑ **AND THE LESSON ABOUT THE VERIFIER, WHICH IS THIS SEAT'S: A POISON THAT CRASHES IS THE EASY VERSION.**
My dropped-row poison returned `None` from `_record`, which crashed `hazard_views` and produced
`setUpClass` errors — I read "exit 1" as caught. The **non-crashing** form of the *same* failure
(`continue` before the row is written) went through the pre-hardening suite with **one bare `KeyError`
against a synthetic fixture and nothing against real source**. Both reproduced firsthand at the merge.
**A poison must be built to survive its own blast** — if it takes the program down, you have measured that
the program crashes, not that the suite noticed. The agent built the harder variant unprompted after being
told the easy one, which is the deviation-with-evidence this file's bars ask for.

## Ops (each line is a paid-for lesson)

**▶ `lane-status.json` — THE BOOT CURL VALIDATES THE FILE YOU WROTE AT BOOT AND NOTHING AFTER IT** (2026-08-30,
this seat, measured). I wrote `"state": "done"` on a landed row after a merge. **`done` is not in the
vocabulary** (`doing | next | open | blocked`; a landed row LEAVES the queue and its landing goes to
`lane-log.jsonl`). **One bad enum in one row rejects the WHOLE file**, so the owner's card for this lane went
dark — with every true thing in it — and stayed dark for about an hour. **Nothing about it is visible from
this side:** the file writes fine, `git` is happy, and the lane goes on reporting accurately to itself.
**The defect was not the word, it was that I ran the verification curl ONCE, at boot.** Every later write is
unverified unless re-checked, and I made a dozen. **Re-run the boot step's curl after ANY write to
`lane-status.json`** — it is two seconds and it is the only thing that can tell you.
⚑ **THIS RULE IS NOW CONTRACT AND EMPYREAN GOVERNS IT** — `contract/LANE_STATUS.md` §*"Verify after EVERY
write, not only at boot"*, at empyrean `origin/main` (verified here by content at `97c4f72`; the hub cited it
as *"commit after 1489413"*, which is a coordinate rather than a SHA, so it was resolved by reading the
section, not by trusting the pointer). **The text below is this repo's PRECEDENT NARRATIVE, not a second
copy of the rule** — on any disagreement the contract wins, and the rule is not to be restated here as it
drifts. Read it at a committed revision, never through `../empyrean/`.
⚑ **n=2, AND THE SECOND INSTANCE READS SHARPER THAN A REPEAT** (aurora, verified here: contract line at
empyrean `origin/main`, carrying commit `10c87ba` — a real contract+docs commit, `--stat`-checked, and this
time the hub emitted the SHA from git rather than naming a neighbouring coordinate). **sigil wrote `closed`
the same night, an hour apart, neither lane aware of the other, both having read the warning shortly
before.** ⚑ **The part worth more than the count: we reached for TWO DIFFERENT WORDS.** That is not two
lanes making the same slip — it is two lanes independently reaching for a terminal state **the vocabulary
does not have**, and picking different plausible names for it. **The error is INVITED by the design, not
merely permitted by it**: the natural word for a finished row does not exist, because the contract's answer
is that a finished row *leaves the queue* — correct, and not what a writer's hand reaches for. A rule
against `done` would not have caught `closed`, and a rule against both would not catch `complete`.
⚑ **The skill's own boot text warned about exactly this** (*"three lanes wrote `done`, which is not in the
vocabulary, in three days"*) and I did it anyway, which is the argument for the mechanism over the warning:
a rule you have read does not fire, a curl does. Found by the aurora lane reading the console, not by me —
**this lane cannot detect its own invisibility, so it depends on a peer looking.** Worth knowing when no
peers are up: the verdict is only ever one curl away, and nothing else will surface it.



**▶ NEW BAR, 2026-08-29 — VALIDATE AN ARTIFACT AGAINST THE SCHEMA IT TARGETS BEFORE CALLING IT READY.
A "ready to merge" IS a completeness claim, and it is the one nobody thinks to check because it reads
as a status rather than an assertion.** Earned against this seat, on the same submission where it was
lecturing about unchecked residues.

**The instance.** This lane authored 11 contract vectors for CR-F, verified *programmatically* that
every case cited its clause, wrote a README handing them over as ready, and shipped them. **Nine of the
eleven could not have passed**: every result case carried `"layout": {}` and `$defs.decoderLayout`
requires five fields. The applying lane filled them in. One command — running the cases against the
schema — would have caught it, and **the schema was readable at a committed revision the whole time and
this lane had already read other parts of it.**

**Why it is a bar and not a slip: the same document argued that an unrecorded residue "reads as guarded
to everyone who sees a green schema run", while containing vectors that could not have produced a green
run at all.** The rigour was spent entirely on the interesting half (which rules are testable, which are
behavioural and therefore not) and none on the mechanical half. That is the failure mode of a lane that
has been reasoning well for hours: **scrutiny follows what looks like an argument, and a field you filled
in as a placeholder does not look like one** — the provenance-feeling-material bar arriving on your own
output instead of on a citation.

**Corrective, and it is mechanical because vigilance already failed:** an artifact authored against a
schema is **run against that schema before it is handed over**, and the run's output is what the handover
cites — never "these conform". If the validator cannot be run from here, say so in the handover and name
what was not checked, rather than letting a confident cover note stand in for it.

**Second defect, same submission, different class: a change introduced between two of your OWN artifacts
needs a named delta.** CR-F as filed said `owner.raw` is served *always*; the fragment written three
hours later makes it absent when `kind == "unavailable"`. That is the better rule and it was adopted —
but it changed a filed artifact silently in a second one, and the receiving lane had to catch it and ask
for it to be noted. **An improvement introduced without a flag is indistinguishable from an
inconsistency**, and the cost lands on the reader who has to work out which it was. Name the delta at
submission time; it costs a sentence.

**▶ NEW BAR, 2026-08-29 — A TEST THAT ASSERTS WHAT YOU *ADDED* IS STRUCTURALLY BLIND TO WHAT YOU
*DISPLACED*, AND A FIXED-SIZE SURFACE MAKES EVERY ADDITION A DISPLACEMENT.** Earned on the
SCREEN-HONESTY parcel, against this seat's own green test.

**The instance.** Adding an `AETHER ON/OFF` field to the F3 status line came with a test asserting the
field survives truncation. It passed. It was also true and useless: `fit` cuts the line from the right
with **no ellipsis and no error**, so the test proved the new field was present while three older ones
— aspect, native resolution, frame counter — had been pushed off the glass. Measured rather than
reasoned about, by printing the fitted string at each real window size: **34 characters available at
224px, 448px, 672px and 896px alike**, because the font scale tracks the picture height, so the budget
in *characters* is very nearly constant no matter how large the window. The line wanted 51.

**The part that makes it a bar rather than a slip: the surface was ALREADY over budget and nobody
knew.** Before this parcel the line ran to 41 characters, so `F1234` had never been visible at any
size and the resolution was being cut mid-number to `320X2`. A field that silently truncates cannot
report its own overflow, and no test asserted on the *whole* line — every test asserted on the field
its author had just added. That is bar 16(d)'s absence surface on a **positive** artifact: a status
line that is present, legible, and quietly missing its right-hand third.

**Correctives, in the order they are cheap.** (1) When adding to a fixed-width surface, assert on the
**whole** rendered string, not on the field you added — `assert_eq!(rendered, full)` is the form, and
it fails for the person who adds the *next* field too. (2) **Print it and look at it** before believing
a boolean; the arithmetic here was mine and was wrong twice before the probe settled it. (3) Ordering
is a design decision on any surface that truncates: the fields that answer *"is this window lying to
me"* go first, and that ordering wants its own test with an **anti-vacuity clause** — there must exist
a width that drops a late field while keeping an early one, or the ordering test passes on a line that
never truncates at all.

**And the fix that bought the room back is worth stealing: the status line now draws one font step
smaller than the rest of the overlay** (`Overlay::status_font_scale`). A toast is a *message* and wants
reading across the room; a status line is a **readout** consulted deliberately by someone who pressed
F3. One step down roughly halves its width cost, and the whole 51-character line now fits at every size
from 448px up — including the frame counter, which had never been visible. At 224px there is no step
left to drop and it still truncates; that floor is **asserted in the test** rather than papered over, so
a change that makes it worse fails instead of passing quietly.

**Second finding from the same parcel, on the silent arm.** The bug being fixed was that `Bus::start`
printed nothing when no `--aether` was given — an absence, which is why it was never noticed. **No test
over that file could have caught its removal either**: a unit test cannot read `println!`, and a test
that shelled out to grep its own stdout would be testing the harness. So the guard is structural — the
`if let Some(..) else` became a `match`, and **deleting the quiet arm is now a compile error.** Where
the observable is unreachable to a test, reach for the type system before settling for a comment. The
wording is pinned separately as a constant, and the two builds' twins pin a shared opening so
`bus.rs` and `bus_stub.rs` cannot start describing one state in two vocabularies.

**Runtime is what closed the wiring, and it was not optional.** Poisoning `filter: filter_label(..)`
back to the core identifier left the entire suite green — the tests call the function, and the call
site sits inside `setup_audio`, which needs a real output device. That half was settled by running the
player under `xvfb-run` with `XDG_CONFIG_HOME` pointed at a scratch `player.conf` carrying
`status_line = on` (which is how you get the F3 line up with no keyboard, and without touching the
owner's config), then reading the screenshot. All three readings confirmed on the glass: `AETHER OFF`,
`AETHER ON` under `--socket`, and `AUDIO RAW` under `ORACLE_CONSOLE_FILTER=off`. **The test's own doc
now says which half it does not cover**, rather than leaving a later reader to assume it covers both.

**▶ NEW BAR, 2026-08-27 — A CLAIM IN OUR DOCS ABOUT A PEER'S FILE HAS A SHELF LIFE, AND NOTHING IN THIS
REPO CAN EVER TELL YOU IT HAS EXPIRED. MEASURED SHELF LIFE: FORTY MINUTES.** Third instance in one day,
which is what makes it a bar rather than three slips.

**The instance, dated end to end because the dating is the argument.** `docs/2026-08-27-breakpoints.md`
§7b landed at **03:11:37** stating that aeon's `evict_witness.py:97` reads snake_case
`timeout_reached` and would silently misreport a timeout. **It was TRUE.** aeon fixed it at
**03:51:24** (`6e4751c3`, verified here as a code commit on their `origin/master`). The claim was then
**re-cited three times inside this repo** (`bp-disclose-recon.md` ×3, `lane-log.jsonl`) and **exported
to aeon at ~13:00 as a live exposure**, where they *"had the brief half-written"* for an agent before
opening the file by luck. **Not one link in that chain re-read the source.** Every re-citation was a
faithful copy of a sentence that had been true.

**The other two instances the same day**: the ROM-disappearance *mechanism* asserted about their build
(invented, not observed), and the 52-method figure that went out from here, was banked there, and came
back to outrank our own measurement. **Different shapes, one property: a statement about a peer's tree
living in our tree, where no reader of ours can meet the contradiction.**

**Why the existing bars do not cover it.** *Verify firsthand* is satisfied — the author did read the
file. *Check the SHA class* is satisfied. *Re-read at send time* (protocol bar 22) is the closest and
is written for **peer status files**, which announce their own staleness with a timestamp; **our own
committed prose does not**, and reads as settled fact precisely because it is in our tree and we wrote
it. **A doc has no `updatedAt`.**

**The remedy, and it is cheap because it is the protocol's verified-at anchor pointed at our own
docs:** when a doc here asserts something about a sibling repo's file, **record the peer revision it
was read at, inline** — `(aeon `6e4751c3`, read 2026-08-27)`. That converts an unfalsifiable sentence
into a one-command currency check for the next reader, which is exactly what the three instances above
each lacked. **And before exporting any such claim across the fence, re-read the file at their tip** —
not the doc that quotes it.

**⚑ The sharpest form, from aeon's side of this one: a peer's warning about YOUR OWN tree is the class
you must verify before acting on, and it is the one that feels least like it needs checking** — it
arrives as help, about your own code, from someone with no motive to be wrong. They nearly briefed an
agent on our stale premise. **Our confident claim about their tree almost became their agent's
instruction**, which is the delegation corollary reaching one repo further than it was written for.


**▶ NEW BAR, 2026-08-27 — DO NOT GREP A RELEASE BINARY FOR A SHORT STRING. THE OPTIMIZER INLINES IT AS
AN IMMEDIATE AND IT IS SIMPLY NOT THERE AS A CONTIGUOUS SEQUENCE.** Earned by nearly reporting a
stale-binary emergency to a peer who was about to make a decision on it.

Chasing whether `target/release/oracle-aether` was current, I grepped it for `timeoutReached`,
`oracle-rs` and `serverBuild`. **All absent.** I re-ran it four ways — `grep -a`, `LC_ALL=C grep -a`,
`strings | grep`, `strings -a -n 4 | grep` — plus a raw byte `bytes.find()` in Python that removes both
tools from the question. **All five agreed on absence, with a negative control returning zero and
positive controls returning hits.** Every check this file demands, and the conclusion was still wrong.

**aeon settled it by spawning the binary and reading the wire: all three strings are served.** Then the
mechanism, measured rather than guessed — the prediction was that an 8-byte immediate leaves the first
eight bytes contiguous and orphans the rest:

| literal | len | release, whole | release, 8-byte prefix | debug, whole |
|---|---|---|---|---|
| `oracle-rs` | 9 | **0** | 1 | 1 |
| `oracle-next` | 11 | **0** | 1 | 1 |
| `serverBuild` | 11 | **0** | 1 | 1 |
| `timeoutReached` | 14 | **0** | 2 | 1 |
| `profile=release` | 15 | 1 | 1 | 0 |

**Short literals are materialized as 8-byte `mov` immediates in the optimized build; only the first
eight bytes survive as a searchable run.** The 15-byte control stays whole, and **every debug build on
this machine carries all of them** — which is the discriminator to reach for.

**Why this is a bar and not a curiosity: it fails in the FALSE-ALARM direction, and it attacks this
lane's own cheap shortcut.** Our 2026-08-26 bar — *a merged serve is not a served method; spawn the
consumer's own path and call it* — is correct, and grepping the shipped binary is the tempting cheap
substitute for it. **That substitute manufactures confident evidence of staleness in a binary that is
perfectly current.** It is bar 16(d)'s absence surface with the sharpest possible teeth: an absence
reproduced by five instruments still is not evidence, because all five shared one frame — *that a
string literal in the source exists as that string in the artifact.* **Nobody records that as a choice,
so re-running cannot vary it** (bar 19 exactly).

**Operational form:** to ask whether a binary contains a symbol or wire key, (1) **spawn it and call
it** — the bar already says this and it is the only answer that cannot be fooled; (2) failing that,
grep the **debug** build, or the **8-byte prefix**; (3) never read a short-string absence in a release
binary as staleness. And if a static read contradicts a live measurement, **the live one wins** —
which is our own *the receiver's already-run command outranks a confident mechanism from this seat*,
arriving with the seat on the losing side.

**▶ AND THE ONE THAT COST MORE — A NUMBER OF OURS CAME BACK AS A PEER'S AND OUTRANKED OUR OWN
MEASUREMENT. FIRSTHAND VERIFICATION DID NOT PROTECT US; IT IS WHAT LAUNDERED IT.** Found by aeon
against this seat, same hour, and it is the reason the absence above went unbelieved for as long as it
did.

The chain: **this lane** measured *52 methods advertised, `capabilities.breakpoints: true`* and told
aeon. aeon banked it in **their** `docs/OVERSEER.md` rung-2 note. I then **read that number firsthand
out of their committed blob** and used it as *independent corroboration* to discount my own static
finding — reasoning that a binary missing its handshake identity could not report 52. **The
corroboration was our own claim wearing another lane's confidence.** Bar 19's echo-versus-corroboration
at its purest: there were never two derivations, there was one, and it had gone round a circuit.

⚠ **And the detail that makes it worse rather than better, verified at their `origin/master`:** the
sentence beside it — the *41*-method reading — is explicitly labelled *"Measured, both binaries as
shipped"*, i.e. **theirs**. The 52 carries **no attribution at all** (*"the same server reports 52
methods…"*). So the number I trusted sat in a peer's tree, in a paragraph whose neighbouring figure
announced its own provenance, silently missing its own. aeon's own account was that they had attributed
it in their docs and dropped it only in the message; **at the revision I read, the durable record does
not carry it either** — offered to them hedged, because it changes the remedy.

**Which is the durable half: `verify firsthand` does not reach this.** Reading a committed blob
confirms the **transcription**, never the **claim** — and this suite's primary defense is firsthand
verification, so the failure rides in on the exact discipline meant to stop it. **Provenance does not
survive transcription into another lane's document**, and the reader cannot recover it, because a
faithfully-copied number looks identical to an independently-measured one. So the remedy is not only
*say whose measurement it is when you repeat it in a message* — it is **the attribution must survive
into the durable record**, since that is what peers read firsthand and trust.
**Before letting any peer's figure outrank your own measurement, ask where it came from originally.**

*⚑ Keep the sting, because the tidy lesson is wrong: **the laundered number was pointing at the TRUTH
and this seat's correctly-executed measurement was pointing at a FALSEHOOD.** The binary really was
current. Had I discounted the circular 52 and trusted my own five-instrument absence, I would have
raised a false staleness alarm at a peer mid-decision. Two defective inputs happened to cancel. So
resist *"trust your own measurement over a peer's number"* — the actual rule is that **a number with no
provenance and an absence with no live control are both unfinished**, and the fix for each is the same:
go and look at the running thing.*


**▶ THE COMMIT-MESSAGE BAR NOW HAS TWO INSTANCES, AND BOTH FAILED BY THE SAME MECHANISM: A LINE WRAP.**
Protocol bar 23 (*a commit message is a claim about a diff, and nothing checks it*) came out of this
lane on 2026-08-23, from a scripted edit that **silently failed on a line wrap** while the shell let the
commit run anyway. On 2026-08-27 aeon produced the second instance in their own tree, hours after
banking the bar — a message asserting two changes, one of which *"matched nothing, because the sentence
wraps across two lines and my pattern assumed one"* (their `c136fc3c`, corrected at `95c39449`, both
verified here as reachable ancestors of their `origin/master`; they corrected the **record**, not the
history, since the first was public).

**Different repo, different operator, different tooling, SAME mechanism** — which by bar 19's test is
corroboration rather than echo, because neither derivation could have shared the other's parameter. So
the durable statement is sharper than the bar's own: **the dominant failure mode of a scripted edit in
these repos is prose that WRAPS defeating a pattern that assumed one line.** Every lane here is
docs-heavy and every doc wraps, so this is the common case, not the corner.

**Two corrections to how the bar is stated, both from their instance:**
1. **`;` is a rung BELOW the `&&` the bar already calls insufficient.** Bar 23 warns that `edit && commit`
   does not protect you, since a replace matching nothing still exits zero. Theirs was weaker still —
   the commit *"sat after the failed edit in the same block rather than behind it"*, so the exit status
   was **never consulted at all.** When a block does both, the commit must be `&&`-behind the edit *and*
   behind a verification, because `&&` alone is known-insufficient.
2. **Match on a short fragment that CANNOT wrap, or read the blob back.** A multi-word prose pattern is a
   bet that the author's wrapping matches yours. Prefer a distinctive short token, and then
   `git show <sha>:<path> | grep -c` the committed blob before writing the message — the assertion in the
   message and the check that earns it are separable, and the message is the cheap one.

*Their framing, kept because it is the honest half: they banked the bar and produced its textbook
instance twenty minutes later. **Rehearsal is not protection** — which this suite's protocol already says
about SHAs, arriving here in a different field.*

**▶ STATUS: PROPOSED, ACCEPTED, QUEUED — DO NOT RE-PROPOSE IT.** The hub ledgered all three sharpenings
as **Q-23** in `empyrean:docs/OVERSEER.md`'s pending protocol queue, verified here firsthand on the
pushed blob at empyrean `e27362c` (which is `origin/main` itself; `grep -c '^Q-23\.'` = 1, and the entry
carries this lane's bar-21 self-discount as stated rather than dropping it). Per the owner's batching
rule it lands **inside bar 23's text as an amendment, not as a new bar**, in the next batched protocol
pass. **Nothing is owed by this lane.** The paragraph above stays as lane-local ops guidance and is
correct whether or not the protocol pass ever runs — but a session that reads *"proposed to empyrean"*
and re-sends it is spending a peer's attention on a closed item, which is the notify-on-the-dependency
bar failing from the other end.


**▶ NEW BAR, 2026-08-29 — THE HEADLESS PLAYER RECIPE IN THIS FILE WAS INCOMPLETE, AND FOLLOWING IT PUTS A
WINDOW ON THE OWNER'S DESKTOP.** Measured firsthand while discharging the two window checks; it happened on
the first launch. The banked recipe (from the SCREEN-HONESTY parcel, above) is *"the player under `xvfb-run`
with `XDG_CONFIG_HOME` pointed at a scratch `player.conf`"*. **`minifb` prefers Wayland when
`WAYLAND_DISPLAY` is set, and every lane session on this box inherits `WAYLAND_DISPLAY=wayland-0`** — so
`DISPLAY=:91` was honoured by nothing, the log said `Wayland window`, and `python-xlib` found **zero windows
on the Xvfb**. The window was on his real screen. Killed inside the minute by recorded PID.
**Corrected recipe — both guards, because the failure is silent and lands on somebody else's screen:**
`env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE DISPLAY=:N target/release/oracle-frontend --x11 …`, with your
own `Xvfb :N` whose PID you recorded. **Then verify placement before driving anything**: enumerate windows
on the display you believe you own, and treat an empty list as the finding rather than as a slow start.
**Why the note was wrong in a way nothing could surface:** it was correct *for its author's purpose* — they
were reading a screenshot, and any window would do. It is the **isolation** claim that was never true, and
the note never said which of the two it was promising. Same class as *check the vintage of the process*: a
sentence true of the file and false of the situation. *(`xdotool` is absent on this machine; `python-xlib`
0.33 is present and gives XTEST, which is what drove the keystrokes. `import` from ImageMagick grabs the
screen; `scrot`/`xwd` are absent.)*

`cd` to the absolute repo path before ANY branch operation (a persisted cwd nearly checked out
under a live agent). Fresh worktrees: `ln -s <repo>/vendor vendor`, verify 17 TestRoms entries, and
open every dispatch with a base check (commit-message string + a file that must exist). Exact-path
`git add` only; `git show --stat` per commit; no Co-Authored-By trailers. Never `cargo test | tail`.
`pkill -f`/`pgrep -f` self-match (the waiting shell's own command line contains the pattern) — bracket the first character: `pgrep -f "[c]argo test"`. ⚠ **Bracketing is not enough when the SAME command carries the literal string elsewhere** — a heredoc writing a doc that quotes the socket path made `pkill -f "[o]rc-p/o.sock"` match its own shell and kill it mid-command (exit 144, 2026-08-26). Kill by the PID you recorded at launch, not by pattern, whenever the command also contains the text. Aether sockets live under `$XDG_RUNTIME_DIR`. `/tmp` is quota'd —
free space is not the signal. The frontend is bin-only (`pub fn` with no caller = hard error).
`ls` is aliased to eza. Owner tests run `aeon/s4.debug.bin`.
A probe socket must NOT live under the session scratchpad — that path exceeds `SUN_LEN` and the
server refuses with `cannot bind the Aether socket: path must be shorter than SUN_LEN`; use a short
`/tmp/<short>` dir. The MCP shim (`oracle-old/linux-port/mcp/oracle_mcp.py`) **SPAWNs its own
`oracle-aether` by default** (private `mkdtemp` socket, `ORACLE_ROM` default `aeon/s4.debug.bin`) and
**ATTACHes only when `$ORACLE_SOCKET`/`$EXODUS_SOCKET` is set** — it does NOT use empyrean's
`resolve_socket_path()`, so do not reason about the shim from that resolver (this seat did, and was
wrong about whose emulator it was talking to).

## Coordination (when peers are up; all optional to progress)

- **seraph** (DAW): **the first FILED DEMAND against the unserved-method list, shaped and dated
  2026-08-26 — and it is the reason the cutover ruling works.** Their S2 verification gate
  (`plans/2026-07-03-s2-verification-gate.md:10-16`; grounds anchor **`a02c77a5`**, which supersedes the
  `9b2c5a77` first cited — both verified here as reachable ancestors of their `origin/main`; the S2
  conclusion hardened rather than moved, so nothing banked off the earlier one needs changing) builds
  **side B entirely out of `emulator_vgm_start`/`stop` → `vgm2wav`**, so **S2 as banked is NOT
  executable against the new core** — VGM capture is not one instrument among several there, it is the
  whole of side B. Their triage, taken as given: **`vgm_{start,status,stop}` is the one that matters**
  (realtime and foreground is fine; what it must be is deterministic enough to capture twice and
  compare); **`audio_spectrum` is explicitly NOT wanted** — their compare is scripted seraph-side
  against the rendered WAV, so do not build it on their account; **channel masks are wanted at S3, not
  S2**, and they declined to charge us the synth-into-the-bus-server cost yet. **Firing condition: S1
  landing**, when a compiled blob exists to capture. They deliberately did NOT file it as a dated queue
  item today, citing bar 18 — the dependency is two packages away and a board entry for a consumer that
  does not exist yet is a cost with no reader. **Treat VGM as demand-ordered-with-a-condition rather
  than unqueued**: when the acceptance list is next triaged, VGM is the only one of the eighteen with a
  named consumer, a named artifact, and a stated trigger. Do not pre-build it; do not renumber it away.
  ⚑ **CONFIDENCE SPLIT, corrected by seraph against this seat's own over-hedge and verified here at
  `main`: the THAT is machine-enforced; only the WHY is a reading.** That these six are unserved is not a
  booked opinion — `schema_conformance.rs:403` pins `SCHEMATIZED_NOT_ADVERTISED` at **18 entries** and
  asserts the whole sorted set with `assert_eq!` (a subset check would not do it), all six audio names
  among them; and `engine.rs:1415` independently advertises **`"vgm": false`** in the handshake
  capabilities, which the comment tells clients to branch on instead of the version integer. Two
  independent places. **What remains unverified is the MECHANISM** — that the channel pair is unserved
  *because* the synth is `#[cfg(feature)]`-gated out of the bus server, and that `vgm.rs` is ungated;
  both came from the acceptance survey's unverified batch and neither has been re-derived. Protocol bar
  10 exactly: a gate's verdict and its stated reason are separately checkable, and this seat had hedged
  the verdict on the strength of doubting the reason. *Bar 19 clears the agreement as corroboration
  rather than echo: our derivation came from a survey agent reading the tree, theirs from enumerating
  the asserted test literal and the capability block. They also declined to lean on `audio_spectrum`
  grepping only to test files — consistent with unserved, but an absence, and the claim rests on list
  membership instead.*
- **aeon** (engine): ⚑ **THEIR CORRECTION TO THIS SEAT'S EXPOSURE RANKING, 2026-08-27, taken.** When the
  shim's ROM-freshness banner landed this seat reasoned about who it would reach by **condition rate** —
  who most often runs against a rebuilt ROM — and answered "aeon". They ran the positive control and
  **nothing in their tools goes through the MCP shim at all**: every gate reaches the emulator through
  `tools/aether_instance.py` → `BusClient`, which spawns the Rust `oracle-aether` directly. So the lane
  that trips the *condition* most often has **zero exposure to the change**. **Their durable form:
  exposure needs BOTH the condition rate AND the transport — rank consumers by who parses the shim's
  output, and do not assume nobody does.** Banked aeon `c115d98c`. This is a general defect in how this
  seat reasons about blast radius: *who hits the condition* and *who sees the output* are different
  populations, and the first is the one that comes to mind.
- **aeon** (engine): demand docs in, acceptance fixtures back — their sweep/probe re-runs are the
  external acceptance for our instruments; shape checks go to them BEFORE build (their requested
  gate). Current lane: the streaming arc consumes the profiler; C-asks flow via
  `docs/2026-08-19-aeon-streaming-demand.md`.
- **aurora** (editor): ✅ **OBLIGATION DISCHARGED 2026-08-27, and they consumed it the same night.**
  They verified the layer pair **by executing against a rebuilt binary they spawned themselves** —
  handshake `implementation: "oracle-rs"`, `serverBuild.id d285ecbc…+profile=release`, `source: "vcs"`,
  `dirty: false`, **46 methods** including both layer methods. (Corroborates this seat's own boot
  derivation from the other direction: 49 `"emulator/*"` literals in `engine.rs` − 3 events = 46.)
  **The tagged band-lens question is closed on their side** — aeon's 8×4 timer band proven stepping in a
  ROM; their write-up + committed instruments at aurora `5f91e4a`, pushed. They also report the
  **build-identity ask discharged from where they sit**: `implementation` and `serverBuild` are separate
  fields and `source: "vcs"` is *derived*, not config-supplied — the two conditions they attached.
  ⚑ **THE BAR THAT EARNED ITS COST:** the condition was *signal when it is served through a REBUILT
  BINARY, not when it merges* — and it is what made the confirmation an execution instead of a claim.
  ⚑ **THEIR FINDING, TAKEN AND WORTH MORE THAN THE PARCEL — a screenshot diff CANNOT tell whether a band
  is stepping.** A band DMAs pixels into fixed slots, so the nametable tile index never moves: **0 of 27
  sample points changed tile id over 90 frames while every screenshot differed.** What separates it is
  VRAM tile *bytes* plus a control run of slots the band does not own. Any lens or gate this lane builds
  over animated background art inherits this — a differing screenshot is not evidence of stepping, and a
  constant tile id is not evidence of stillness.
  ⚑ **AND A RELAY DEFECT, corrected in BOTH directions 2026-08-27 — this is the durable half.** aurora
  reported that this lane had banked `pixel_attribution`'s `cell` as hanging off `winner`, when it is a
  **top-level SIBLING of `winner`** (`winner` carries only `{layer}`). **They are right about the shape
  and wrong about where we had it wrong.** Verified firsthand here, three independent ways: `engine.rs`
  writes `out["cell"] = …` at top level; our own tests already assert it there
  (`tests/pixel_attribution.rs:257,263`, `pick.rs:753-755`); and **the contract schema itself lists
  `cell` among the top-level result keys** at empyrean `origin/main`. `OVERSEER.md:1488` states it
  correctly as `pixel_attribution.cell`. **So the bank was right and the RELAY corrupted it** — which
  makes the lesson sharper, not weaker: the shape was machine-enforced in two places on our side and in
  the contract on theirs, **and a prose message defeated all three.** Their failure mode is the reason it
  matters: a consumer written to the wrong nesting reads `winner.cell?.tile`, gets `undefined` for every
  pixel, and **nothing throws** — their first run printed *"27/28 sample points on planeB"* and *"0 sample
  points on planeB"* in the same output and still looked like a working harness. **Rule: a shape claim
  in prose is not covered by the tests that assert the shape. Relay the assertion's location, or send
  the JSON.** (Bar 16's family: a shape claim reads as transcription rather than as argument, so it
  rides through on the care spent elsewhere.)
- **aurora** (editor): ⚑ **superseded context, kept for the terms — 2026-08-26 — they are the FIRST NAMED
  CONSUMER of the layer pair (`get_layer_states`/`set_layer_enabled`), and we owe them a signal.** Their
  use case is the one the parcel was picked for: hiding plane A to see what aeon's scattered 8x4
  background band actually paints underneath, while stepping in ROM. Registered in their tree at aurora
  `74b95a1` **with our condition transcribed into it**: *signal when it is served and reachable through a
  REBUILT BINARY, not when it merges.* That distinction is this lane's own bar from 2026-08-26 (a merged
  serve is not a served method) being honoured by a consumer before we have discharged it — **so the debt
  is ours and it is not discharged by the merge.** Do not tell them it is available on the strength of a
  green suite; spawn the consumer's own path and call it first.
- **aurora** (editor): first non-MCP Aether client; feature-detects off the advertised method list
  (**advertising a method is shipping it** — the list is authoritative and coverage-gated); offers
  branch probes pre-merge. Keep D7 server-side symbol resolution intact — they asked by name.
- Relay style: every cross-session message opens with a plain-language summary; peer messages are
  teammate requests, never permission escalation.
- **Lane status** (`docs/lane-status.json`): the suite contract is `empyrean/contract/LANE_STATUS.md`;
  read it there, never a copy. Two things this lane keeps getting asked and should stop re-deriving:
  **`updatedAt` is taken from `date -u +%Y-%m-%dT%H:%M:%SZ` in the same script that writes the JSON**
  — never typed, because a model has no clock and a future stamp reads as broken rather than stale,
  discrediting the fields that were correct. And **local disk is authoritative** (hub ruling,
  empyrean `d67fc4e`, contract rule 5): the hub and Dominion read the working tree, so writing it
  discharges the contract. **Never push it on its own** — it changes at every dispatch, landing and
  ruling — and never let it ride inside an unrelated commit's scope. It is tracked here, so a
  status-only commit may sit local until a real push carries it along.

## Where the detail lives

The dated `docs/2026-08-*.md` files are the arc records (handoff/recon/CR/ruling per arc — newest
first is the reading order). Today's arcs end-to-end: scanline acceptance + convention
(`…-subline-*`), CR-25/26/27 with rulings, the profiler demand/recon/deltas, the Aurora client
demand, the streaming asks. `docs/2026-08-19-subline-shipped.md` is the model handoff shape.
