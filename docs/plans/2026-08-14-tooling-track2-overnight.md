# Track 2 — overnight arc (2026-08-14)

**Authorisation.** Owner shipped track 1, said *"please continue, I'm going to bed so feel free to
work for a while past track 2 so a lot gets done overnight"*, and went to sleep. That is a broad
mandate to proceed, but it is **not** an answer to the three open questions in
`docs/2026-08-14-tooling-frontier-recon.md` §7. Those are recorded below with **provisional**
answers, each chosen to be cheap to reverse. Anything expensive to reverse, or that reaches outside
this repo, is deliberately NOT done — see "Explicitly not done".

**Design of record:** `docs/2026-08-14-tooling-frontier-recon.md`. Read it first; this plan only
records execution.

---

## Standing constraints for every slice in this arc

1. **Currency is a hard gate.** No golden may be regenerated. A moved golden means the model is
   wrong, not that the baseline needs updating (the T16 precedent: the *correct* model was cheaper in
   currency than the crude one). If a hash moves: STOP, report, do not regenerate.
2. **`BusEvent` gains no fields.** It derives `Eq` and is recorded into `Vec<BusEvent>` by tests
   asserting exact sequences. The established pattern for extending the seam is a **defaulted
   forwarder method** (`on_event_at`, `ebebe8e`), not a field.
3. **The no-instrumentation path stays bit-identical.** §2a of the recon doc records three separate
   investigations damaged by instrumentation that perturbed scheduling. Anything added here must be
   provably inert when unused.
4. **C1–C4 (recon §5) apply to every capture/trace facility**: atomic arm-at-power-on, deterministic
   *emulated* frame identity, a cheap negative-control mode, and no residual instrument state.
5. **Verification is the overseer's, not the implementer's.** Every gate is re-run independently
   before merge. This session already caught a self-reported `exit 0` on a run that had actually
   failed, so the rule is load-bearing rather than ceremonial.

---

## Provisional answers to the three open questions

| # | Question | Provisional answer | Why it is safe to reverse |
|---|---|---|---|
| a | Does the `deb2` in-ROM appendix decoder earn its cost now? | **No — `.lst` first.** Investigate `deb2` only far enough to judge whether its presence/offset/size works as a *build fingerprint* without decoding the packed names. | Investigation is free; the decoder is additive later and nothing built now depends on its absence. |
| b | Raise the two cross-repo asks (`s4.build.json`; `emulator/rom_reloaded` vs `romReloaded`)? | **Not overnight.** Drafted for the owner to send, not sent. | Filing a change request against another team's contract is outward-facing and not mine to initiate unprompted. |
| c | How much of the 53-method catalog do we commit to? | **Conform on the wire, subset the methods.** Implement the transport/handshake/event shape verbatim, then a thin method set. The catalog is a target, not a menu to swallow whole. | The handshake advertises a *generated* method list, so adding methods later is a capability-flag change, not a breaking one — which is exactly why the contract designed it that way. |

---

## Slices

### S1 — sink stop signal → `run_until(predicate)` + fan-out combinator  ▸ dispatched
The recon doc's cheapest high-value item: `run_until_with_sink` already calls `on_step_boundary` on
every CPU step (`system.rs:723`) but nothing can interrupt the loop, which is why nine hand-tuned
magic frame budgets and two polling loops exist. Adds a **defaulted** stop signal (existing sinks
compile untouched, default preserves today's behaviour), a bounded predicate-driven run that cannot
hang, and an unambiguous fired-vs-timed-out outcome. Plus the generic fan-out combinator the recon
doc says to build *before* the tool surface, since the only existing fan-out is hand-written for one
pair (`AudioAndWatch`).

Proof-of-value requirement: convert **two** of the nine magic budgets, not all nine — two demonstrates
the API, a mass rewrite is a separate reviewable change.

### S2 — sigil `.lst` symbol loader  ▸ dispatched
Highest single-feature leverage (recon §6 item 2). Pure parser in the core (charter: no I/O — the
caller reads the file), lookup in both directions including **address → nearest symbol + displacement**,
and the `$module$Proc$local` scope tree that resolves a PC to `EntryPoint.wait_dma` rather than
"somewhere after `EntryPoint`" — a differentiator the sibling cannot match, available for free from
the name mangling. Three documented traps must be handled or the answers are silently wrong: 8-digit
`FFFFxxxx` RAM addresses, a type column that is `C` for 100% of entries and discriminates nothing, and
per-shape binding that makes `s4.debug.lst` against `s4.bin` a wrong answer to be **refused**.

Tests use in-repo synthetic fixtures; the aeon repo is read-only and CI must not depend on it.

### S3 — candidate, not yet dispatched: Aether transport
`empyrean/contract/protocol.md` verbatim — NDJSON JSON-RPC over `AF_UNIX` (mode 0600),
`initialize`/`initialized` with a **generated** method list, push events. Gated on S1's combinator.
Must honour recon §4's three non-negotiables from the start: every reply carries `{frame, mclk,
running}`; every array bounded/cursored with an explicit truncation flag; anything approximate carries
a `caveat`. Retrofitting the timestamp later is far more expensive than honouring it now.

Also a hard requirement, from a real incident: **a slow or dead client must never wedge the
emulator.** `aeon/docs/BUGS.md:494-551` records a frozen repro frame *"lost to an emulator
control-socket hang before the sprite table could be dumped"* — a hang in the debug transport
destroyed irreplaceable evidence.

### S4 — candidate: the trace recorder (recon §3 item 1, recurrence 11)
The most-recurring hand-rolled capability, and the one that has been re-implemented six times. **Higher
risk than it looks**: attribution wants to live *in* the event, which collides with constraint 2 above.
Needs a design pass on the forwarder-vs-field question before implementation, so it is not being
dispatched blind overnight.

---

## Explicitly NOT done overnight

- **No cross-repo change requests filed** (question b). Drafted only.
- **No golden regenerated**, under any circumstance.
- **No `deb2` decoder** — investigation only.
- **No window-layer rebuild.** The owner chose "features first, rebuild later", and the rebuild should
  be informed by what a debug UI actually needs — which is exactly what this arc is still discovering.
- **No mass rewrite of the nine magic frame budgets.** Two, as proof.
- **Nothing depending on hardware nobody has confirmed.** The track-1 owner-owed checks (gamepad
  buttons, volume/mute, F1/F5/M in a real window) remain open and are not assumed working.

---

## Status log

- **00:5x** — S1 and S2 dispatched in parallel isolated worktrees. Independent, different files.
- (updated as slices land)
