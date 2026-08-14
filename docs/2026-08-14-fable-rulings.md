# Rulings on the open tooling/contract decisions (Fable, 2026-08-14)

**Commissioned by the owner** to adjudicate the decisions queued at the end of the 2026-08-14 overnight
arc. Every load-bearing claim below was re-verified firsthand against the tree before ruling, per this
project's standing rule about restated figures (the verification log is at the end). Nothing outside
this file was modified.

---

## Summary for a non-specialist

Six decisions, six rulings, none deferred:

| # | Question | Ruling in one line |
|---|---|---|
| **A** | Trace timestamps are silently NTSC | **Stamp them and label them.** Add a machine-readable `timing_basis` field now (it's a constant string today, ~free). Do not refuse to stamp; do not rely on prose caveats alone. |
| **B** | Chip-level "who drove this bus access" attribution | **Keep the shipped caveat, drop the feature.** The episode that justified it was checked and doesn't hold. Build nothing more unless a real hunt is blocked by its absence. |
| **C** | The six contract change requests | **Adopt all six.** Each fixes a concrete defect hit while implementing the contract verbatim. Two small wording corrections noted below. |
| **D** | Save-states over the wire (`checkpoint`) | **Amend the contract now and ship it.** This is the single biggest agent-productivity item on the board (one call instead of forty). Minimum schema below, with three rules the draft didn't cover. |
| **E** | "The cross-repo asks aren't mine to send" | **Your reframing is correct — just do them.** One person owns every repo; filing a change request in your own contract repo is editing your own spec. The discipline worth keeping is *sequencing* (write the contract first, then implement), not *separation of parties*. |
| **F** | Priority of the cheap unbundled work | **1) arm-at-power-on, 2) scanline capture + frame hook, 3) size/parity filter, 4) expose the latches, 5) defer the tuple census.** Nothing on the list should never be built, but three things adjacent to it should stay unbuilt. |

The overall shape: the overnight work was cautious in exactly the right places (it declined to invent
`checkpoint`, it declined to file cross-repo changes without a ruling). Those cautions were correct *as
agent defaults* and are now *resolved by this ruling* — the owner can simply proceed.

---

## A. `F-TRACE-PAL` — the silently-NTSC timestamps

**Ruling: keep stamping, and add a machine-readable timing-basis field to the trace report and to the
Aether `initialize` result. Do not refuse to stamp. Do not settle for a prose caveat.**

**What it means in practice.** `TraceReport` gains something like `timing_basis: "ntsc"` (or, better, a
tiny struct: `{standard: "ntsc", mclk_per_frame: 896040, lines_per_frame: 262}` so a consumer never has
to look the numbers up). The Aether server advertises the same thing once in `initialize`. Today it is a
constant, because the machine *is* NTSC-only (`MCLK_PER_FRAME = 896_040`, `system.rs:24`, verified) —
so this is a few lines and zero risk.

**Reasoning.**
- The three options are not symmetric. *Refusing to stamp* is wrong because the stamps are **correct**
  — the emulated machine genuinely is NTSC; there is no "unknown basis" case in the current core.
  *Caveat-only* is the weakest form of the design's own rule: agents and scripts read fields, not prose,
  and a caveat string can't be branched on. *A basis field* is the only option that is both honest today
  and still correct the day PAL arrives.
- The design doc's own cost argument is decisive and I verified its premise: "recording it now is free;
  retrofitting it after agents have cached 'frame 601' is not." Once downstream tooling stores frame
  coordinates, an unlabeled basis becomes an unfixable ambiguity in *other people's data*.
- Precision note the open question glossed over: "NTSC" alone is under-specified — the constant is
  specifically 262 lines × 3420 mclk (H32 line timing). Carry the numbers, not just the label, and the
  field can never drift from the scheduler's actual arithmetic.

**Does PAL support later change this?** No — it's the point. When PAL lands, region becomes machine
state and the field goes from constant to live; every consumer that already reads it keeps working, and
every consumer that ignored it was NTSC-only anyway. The ruling is the same in both worlds, which is
what makes it safe to decide now.

**What would change my mind.** If a survey showed no consumer ever persists frame coordinates across
sessions, caveat-only would suffice. That is contradicted by the existing corpus (magic frame budgets,
docs full of cached frame numbers), so I don't expect it.

> **EXECUTED 2026-08-14.** `system::TimingBasis { standard: TimingStandard, mclk_per_frame,
> lines_per_frame }`, both numbers derived from `MCLK_PER_FRAME`/`MCLK_PER_LINE` and tied to `vdp`'s
> geometry by `const _: () = assert!(…)`, so the reported basis and the scheduler's arithmetic cannot
> disagree. Exposed as `System::timing_basis()` (the accessor that goes live under PAL without changing
> shape), `Watchpoints::timing_basis()` (the trace-report surface), and `timingBasis` in the Aether
> `initialize` result.
>
> **One correction to this ruling's wording:** `TraceReport` does not exist. It is a *sketch* in the
> trace-recorder design's §11 ("Output shape"); what shipped on 2026-08-14 is the four additive changes to
> `watchpoints.rs`, whose report surface is `Watchpoints`' accessors + `WatchReport`. The basis therefore
> landed as `Watchpoints::timing_basis()`. Consequence worth knowing: because `Watchpoints` is never handed
> the machine, its basis is the NTSC constant rather than a value read off `System`. That is correct while
> the core is NTSC-only (an integration test asserts `wp.timing_basis() == sys.timing_basis()` so the two
> cannot silently diverge), and the PAL-day fix is to plumb the machine's basis into the recorder — the
> accessor's signature, and every consumer, are unaffected.

---

## B. `F-TRACE-MASTER` — function-code master attribution

**Ruling: adopt the design's option (c), which is already shipped — document the `fc = 0` conflation
and attach a caveat to any master-flavoured query over the PSG ports. Do not build option (a) (emit
true `fc` at the window tap) and do not build option (b) (a `master` field on `BusEvent`). Remove
"master attribution" from any roadmap as a feature; it is a documented limitation.**

**What it means in practice.** Nothing further to build. `Watchpoints::caveats` already reports the
PSG-port conflation (verified: `watchpoints.rs:29,181,438,684`; the conflation itself is deliberate and
documented at `bus.rs:716-728` — a 68000 PSG write through the Z80 window is reshaped to look exactly
like the Z80's own write so the VGM logger and synth unify the two paths). That stays as-is.

**Reasoning.**
- The justifying episode was checked against the primary record and **refuted**: the sound-silence hunt
  classified on address, was explicitly `fc`-agnostic, and the 68k was never a suspect
  (`docs/2026-07-22-phase-rt-design.md:145-148, :444`). A feature whose founding evidence dissolved on
  inspection has no evidence — this project's own "no episode, no feature" rule applies.
- Option (a) is worse than nothing: it is currency-gated *and* it would break a deliberate, documented
  design (the VGM/synth path unification at `bus.rs:1594-1595`) to serve a need nobody has demonstrated.
- The one real defect the investigation surfaced — that on `$7F11` both master signals are wrong
  simultaneously — is a one-port hole, and the caveat makes it impossible to be silently wrong about it.
  That is the correct end state: the failure mode was never "we lack the data", it was "we might draw a
  conclusion the data can't support", and the caveat closes exactly that.
- **Bias check, since the brief asked:** this is *not* the recorder bias protecting its own. Master
  attribution *is* a recording feature, and it is being declined on the same evidentiary standard that
  declined breakpoints. The bias is being applied as a standard, not as dogma.

**What would change my mind.** One real hunt — ours or an Aeon developer's — that stalls because it
cannot distinguish a 68k-window PSG write from a Z80-native one. If that happens, option (a) behind a
currency check is the right fix (fix the source, not the struct), and the caveat already marks the
exact spot.

---

## C. The six Aether change requests

**Ruling: adopt all six into `empyrean/contract/protocol.md`.** None is speculative; each was hit while
implementing the contract verbatim, and (per ruling E) adopting them is the owner editing the owner's
own spec. Per-CR rulings, with reasoning where it isn't obvious:

**CR-1 (no `stopped` reason for a completed `run_frames`) — ADOPT, with one spelling correction.**
The gap is real: a completed bounded run is none of the seven listed reasons, and the shipped
workaround (`reason: "step"`) is a knowing mislabel. But the CR proposes adding `frames` to the enum;
the enum's existing values are `runTo`, `runToScanline` — the consistent spelling is **`runFrames`**.
Also make the additive `frames`/`deadlineReached` params normative while you're in there.

**CR-2 (`checkpoint` has no op and §9 defers it) — ADOPT.** Full ruling under D.

**CR-3 (no error code for "wrong machine state") — ADOPT** as proposed: `-32005 | invalid state for
this operation`, `data.reason` machine-readable. The alternative — implicitly pausing on the client's
behalf — is a silent mode change, the exact class of surprise this bus exists to prevent, and `-32600`
genuinely is the wrong code (the envelope is fine; the timing is wrong). Cheap, correct, no downside.

**CR-4 (`run_to` unbounded, no fired-vs-timed-out result) — ADOPT; this is the sharpest of the six.**
The contract as written specifies a call that can hang forever and whose result (`target`, an echo of
the input) cannot distinguish success from surrender. Both defects have body counts on record: the
unbounded hang is the same failure shape that destroyed an irreplaceable frozen repro frame
(`aeon/docs/BUGS.md:494-551`), and ambiguous-success is the defect the core's own
`StopReason::{SinkRequested, DeadlineReached}` was built to refuse. Make `maxFrames` (default 600) and
`reached: bool` normative for `run_to`, `run_to_scanline`, and any future wait-shaped op. A contract
that specifies a footgun *is the source of truth for a footgun*; fix the source.

**CR-5 (no reply carries a machine timestamp) — ADOPT** as an envelope-level normative field in §2:
every result, every `error.data`, every event's params carries `{frame, mclk, running}`, both clocks
emulated. This was the recon's single worst finding about the sibling's surface ("an agent stitching
four reads into one conclusion may be reading four different machine states with no way to detect it"),
and oracle-next already implements it structurally. One consequence to accept with eyes open: the
moment this is normative, the sibling C++ Oracle is non-conformant. That is fine — the roadmap already
says oracle-next inherits the bus role, and the sibling is on the way out. Grandfather it or don't; do
not weaken the contract to protect a server being retired.

**CR-6 (`romReloaded` vs `rom_reloaded`) — ADOPT the contract's spelling; fix Aurora's spec.**
Verified firsthand: `protocol.md` §3 defines `emulator/romReloaded`; Aurora's approved spec subscribes
to `emulator/rom_reloaded` (`aurora/docs/specs/2026-07-03-aether-client-playtest-design.md:35`). The
contract wins on both grounds available: it declares itself the source of truth, *and* camelCase is
the envelope's own convention everywhere else (`protocolVersion`, `frameToken`, `runTo`). The fix is
one line in Aurora's document. And the drafted ask is right to refuse emitting both spellings —
a server that accepts both makes the drift permanent and invisible. Until the Aurora doc is edited,
an approved client is subscribed to an event that will never arrive; that is a live bug, so this is
the most time-sensitive item in this entire ruling set.

**What would change my mind (whole section).** Only CR-5 has a real trade-off (sibling conformance),
addressed above. If the sibling were expected to live another year as a peer server, CR-5 would want a
capability flag (`stamped: true`) instead of a bare mandate. It isn't, so it doesn't.

---

## D. `checkpoint` / save-states over the wire

**Ruling: amend the contract now (accept CR-2) and ship it in oracle-aether.** The implementer's
refusal to invent it unilaterally was correct process; the deferral itself was a June scheduling note,
not a considered rejection, and the facts have changed: the capability now exists, is O(struct), and is
covered by a determinism property test (`System::snapshot`/`restore`, `system.rs:554/560`, verified).

**What it means in practice.** An agent gets to "the level-2 boss" once, calls `checkpoint`, and every
subsequent experiment starts from `restore` — one call instead of forty scripted-input calls per
iteration. This is the largest single reduction in agent round-trips available anywhere in the catalog.

**Minimum schema.** CR-2's proposed four methods are right and complete — adopt them as drafted:

| Method | params | result |
|---|---|---|
| `emulator/checkpoint` | `label?` | `id`, `frame`, `mclk`, `bytes` |
| `emulator/restore` | `id` | `frame`, `mclk` |
| `emulator/checkpoint_list` | `cursor?`, `limit?` | bounded array of `{id, label?, frame, mclk, bytes}` |
| `emulator/checkpoint_drop` | `id` \| `all` | `removed` |

Server-assigned ids (as drafted — no cross-client collisions). Plus **three rules the draft did not
pin down, which should go in the contract text**:

1. **Checkpoints are volatile: in-memory, per-server-session, never persisted to disk.** The snapshot
   format is bincode of the live struct — it is version-fragile across builds *by design*, and writing
   it to disk would silently create a durability contract the format doesn't promise. If persistent
   save-states are ever wanted, that is a separate, versioned artifact and a separate CR.
2. **`restore` restores the whole machine, ROM included.** State it explicitly, so restoring across a
   `reload_rom` is understood to bring the *old* ROM back rather than being refused or half-applied.
3. **A cap on total checkpoint count/bytes, refused loudly** (`-32005` per CR-3 or a dedicated code,
   with the cap in `error.data`) — "refuse loud, never clamp silently" is already this contract's rule.

**Bias check, since the brief asked.** The archaeology lists rewind/save-state as zero-use despite
being a charter promise — so isn't shipping this exactly the charter-promise failure mode? No, for a
population reason the recon itself identifies: the zero-use corpus is *us debugging the emulator*;
`checkpoint` serves *agents driving a game*, a workload that barely existed until the Aether socket
landed this week. It is still a bet on that new population, so the standing usage policy applies: ship
the thin version, watch whether it gets used, and let usage justify anything fancier (named persistent
slots, auto-checkpointing, etc. — none of which is in this schema, deliberately).

**What would change my mind.** If snapshots turned out to be large enough that a handful wedges the
server's memory (they shouldn't be — the machine's whole state is a few MB dominated by ROM), rule 3's
cap handles it; if even that failed, restore-to-file-descriptor designs exist, but nothing suggests
we need one.

---

## E. The two cross-repo asks — and the framing itself

**Ruling: your reframing is correct, and it was the right thing to question. Send both asks — or more
precisely, stop thinking of them as "asks" and treat them as work items in your own suite. This also
retroactively simplifies C and D: those are not petitions either; they are edits to your own spec,
gated only by your own review.**

**What it means in practice.**
- **Ask 1 collapses into CR-6** (ruled above): edit one line in Aurora's spec. Do it first; it is a
  live bug (an approved client subscribed to a nonexistent event).
- **Ask 2 (`s4.build.json` from `sigil build`)**: file it against sigil/empyrean and schedule it.
  Verified firsthand: `empyrean/CLAUDE.md` already says to ship it *"now rather than waiting"* — so
  this is not even a new decision; it is executing a decision the suite already made and never actioned.
  The overnight evidence made it stronger (the `deb2` fingerprint is provably a filter, not a proof:
  the demo pair share an identical `EndOfRom` while 1,197 shared symbols moved), and the stale-symbol
  bug class it kills has now bitten three times.

**Reasoning — what the old framing got right and wrong.** "Outward-facing, not mine to initiate" was
the correct *default for an unsupervised overnight agent*: an agent should not edit a contract repo
mid-implementation on its own authority, and that restraint did real work this arc (it is why
`checkpoint` was raised as a CR instead of invented, which is why D is now easy to rule on). But as a
*standing state* it was over-framed: there is no other team. The contract's value is that it is the
written source of truth and that changes to it are recorded and reviewed — none of which requires the
filer and the owner to be different people. **Keep the sequencing discipline (contract first, then
implementation; agents raise CRs, the owner accepts them). Drop the fiction of a foreign counterparty.**

**How it changes C and D.** It converts them from "drafts awaiting a third party" into a short
owner-review-and-commit pass over `empyrean/contract/protocol.md`. Practically: accept the six CRs
(with the CR-1 spelling fix), add the checkpoint section, fix the Aurora line, and the entire queue of
contract questions from this arc is closed in one sitting.

**What would change my mind.** If the suite ever gains a second contributor, reinstate the full
change-request formality — the process is already written down, which is exactly why relaxing it now
is safe.

---

## F. Priority for the cheap unbundled work

**Ruling — build in this order:**

**1. C1 arm-at-power-on (~5 lines) — first, and treat it as a bug fix, not a feature.**
Verified: `System::reset` hardcodes the unit sink (`self.step_cpu(&mut ())`, now at `system.rs:404` —
the doc's `:361` has drifted but the claim holds), so the reset-vector fetches — the first bus accesses
of the machine's life — are invisible to *every possible instrument*, and all eight bespoke sinks in
the tree attach post-reset. This exact gap already produced a retracted verdict once (the mis-armed
capture that "diverged at melody index 0, a garbage comparison"). It is the cheapest item, and it is
load-bearing for the trustworthiness of everything else on this list — an aggregate over a mis-armed
capture returns a plausible number, not an error. `reset_with_sink` + the atomic `boot_with_sink`
constructor, exactly as the design sketches.

> **EXECUTED 2026-08-14, and the ~5-line estimate held for the fix itself** (`reset` → `reset_with_sink`
> + delegate + `boot_with_sink` = 4 changed/added lines of logic). The slice around it is larger and
> deliberately so: `is_pristine_power_on()` — C1's *"the API must expose that check"* half, which this
> ruling's item 1 does not mention and the design doc left open as `F-TRACE-POWERON-CHECK` — plus the
> tests. **Finding: our power-on anchor is all-zero**, not the `PC=0xFFFFFFFF, SP=0xFFFFFFFF, SR=0xFFFF`
> the recon quotes; those are the *sibling* Oracle's values, so a check ported on those literals would be
> wrong here. `F-TRACE-POWERON-CHECK` is closed by the predicate.

**2. `F-SCANLINE-CAPTURE` + `on_frame_boundary` (~60 lines) — second.**
The duplication evidence is the strongest in the whole corpus (two sinks with byte-identical method
bodies written 9 hours apart, same author, same day), the hook retires what the census calls the
single largest source of duplicated bookkeeping (every sink that needs frame structure currently
infers it from magic line numbers), and it directly serves the audience the recon says per-scanline
work is *for* — Aeon developers, who independently rediscovered the frame-latched-CRAM trap three
times. This is the item the trace recorder's motivating evidence actually pointed at.

> **EXECUTED 2026-08-14, and the ~60-line estimate held** (`crates/oracle-core/src/scanline_capture.rs` =
> 117 lines of type + impl, `on_frame_boundary` = 1 trait method + 3 forwarders + 1 call site; both ad-hoc
> sinks deleted). **The one non-obvious call: the boundary is end-of-active-display (the line-224 `Scanline`
> event), not top-of-frame.** `run_frames` deadlines land exactly on a frame-boundary mclk and the run loop
> tests `now < deadline` *before* popping due events, so the line-0 event of the frame after the last is
> never delivered inside a run — which is why a 1-frame run delivers 224 scanlines and not 225. Placing the
> hook at line 0 would have silently orphaned the final frame of every run and moved the `color_1536`
> per-scanline `frame_hash` currency. Pinned by a test that runs the deleted magic-line sink as an oracle
> and asserts byte-identical retained pixels.

**3. `F-TRACE-SIZEFILTER` (~8 lines) — third.** Four real episodes, no currency surface, and it was
only left out because the design honorably refused to smuggle in a filter its spec hadn't sanctioned.
Sanctioned now. Do it in the same sitting as item 2 if convenient.

**4. `F-TRACE-EXPOSE-LATCHES` — fourth.** Read-only accessors for `z80_busreq` / `z80_running` / the FM
address latch. Three separate sinks currently shadow-reconstruct hardware state the machine already
holds, and the K4 refutation showed this — not more census primitives — is what most of `K4Probe`
actually needs. It ranks below 1–3 only because its payoff arrives on the *next* hunt rather than
fixing anything broken today.

**5. `F-TRACE-TUPLEKEY` — defer; do not build now.** Its one justifying episode (T16) used
VDP-internal probe results as keys, not bus-event fields — so the evidence, examined closely, does not
actually support building it *on this seam*. It stays a registered follow-up awaiting a bus-shaped
episode. Deferring is cheap: it is a small additive extension whenever one appears.

**Should any never be built?** None of these five. But three adjacent things should stay dead unless
new evidence appears, and it is worth saying so plainly so they aren't re-funded by momentum: the
`master` field on `BusEvent` (ruling B), min/max & percentile aggregation (no episode, ever), and the
bus-legality detector (called "highest strategic value" in a 2026-07-20 wishlist; two years of hunts
never needed it once — the recon is right that this one deserves a pause before anyone funds it again).

**One item outside the given list:** `F-TRACE-VDPWRITE-MCLK` (per-write timestamps on VDP writes) is
the change that answers sub-scanline CRAM questions directly instead of by hand arithmetic. It touches
`vdp.rs`'s capture path, so it is correctly its own slice — schedule it after 1–4, ahead of any
further aggregation work.

**What would change my mind.** If the Aeon replay-runner work (the recon's "highest-leverage
engine-facing item") starts before this list is done, items 2–4 should yield to it — a working CI gate
for the engine outranks instrument polish. Item 1 yields to nothing; it's a five-line hole under every
instrument the project owns.

---

## Verification log (what was checked firsthand for this ruling)

- `MCLK_PER_FRAME = 896_040`, NTSC-only, `system.rs:24` — confirmed.
- `System::reset` hardcodes the unit sink — confirmed at `system.rs:404` (doc anchor `:361` drifted).
- `System::snapshot`/`restore` exist (`system.rs:554/560`) with round-trip tests — confirmed.
- Aurora's approved spec subscribes to `emulator/rom_reloaded` (its line 35) vs the contract's
  `emulator/romReloaded` (§3) — both spellings confirmed in their respective files.
- The `$7F11` fc/address conflation is real, deliberate, and documented (`bus.rs:716-728, 1594-1595`);
  `Watchpoints::caveats` already reports it — confirmed.
- The refutation of the master-attribution episode (`docs/2026-07-22-phase-rt-design.md:145-148, :444`,
  addr-classified and fc-agnostic) — confirmed via the design doc's quoted primary text.
- `empyrean/CLAUDE.md` blesses shipping `s4.build.json` "now rather than waiting" — confirmed.
- The shipped watchpoints API (`WatchMode`/`CensusKey`/`stop_after`/`caveats`) and the `oracle-aether`
  crate (with `emulator/state_hash`, no `checkpoint`) — confirmed in the tree.
- `protocol.md` §3 reason enum, §6 `run_to | addr|symbol | target`, §9 save-state deferral, §8
  no-invention rule — confirmed in the contract text.
- Repo state: HEAD `a8c61a0`, clean tree, arc pushed — confirmed.
