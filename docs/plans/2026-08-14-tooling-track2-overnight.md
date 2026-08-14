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
2. **`BusEvent` gains no fields** — conclusion unchanged, **stated reason corrected 2026-08-14.**
   This plan originally justified it as "a new field poisons the `Vec<BusEvent>` recording asserts".
   That is folklore, and the design pass measured it: **2 breaking assertions in 1 file plus 12
   mechanical literal edits, zero exhaustive destructurings.** The real and stronger reason is that
   **three of the four emission sites do not know the values** — `MegaDriveBus::emit` (`bus.rs:563`)
   holds only `now_mclk`, and the phase-0 `SystemBus` (`bus.rs:190/201`) has no clock at all — so a
   `pc`/`frame` field would be a sentinel in the 22 unit tests that build a bus with no `System`.
   The established extension pattern remains a **defaulted forwarder method** (`on_event_at`,
   `ebebe8e`); attribution belongs in a type layered *above* `BusEvent`. See
   `docs/2026-08-14-trace-recorder-design.md` §2-§4.
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

### S3 — Aether transport  ▸ SHIPPED (see "S3 as shipped" below)
`empyrean/contract/protocol.md` verbatim — NDJSON JSON-RPC over `AF_UNIX` (mode 0600),
`initialize`/`initialized` with a **generated** method list, push events. Gated on S1's combinator.
Must honour recon §4's three non-negotiables from the start: every reply carries `{frame, mclk,
running}`; every array bounded/cursored with an explicit truncation flag; anything approximate carries
a `caveat`. Retrofitting the timestamp later is far more expensive than honouring it now.

Also a hard requirement, from a real incident: **a slow or dead client must never wedge the
emulator.** `aeon/docs/BUGS.md:494-551` records a frozen repro frame *"lost to an emulator
control-socket hang before the sprite table could be dumped"* — a hang in the debug transport
destroyed irreplaceable evidence.

### S4 — the trace recorder  ▸ DESIGNED (`27e3d14`), not implemented
Design: `docs/2026-08-14-trace-recorder-design.md`. **Verdict: build it at roughly a quarter of the
implied size**, as four additive changes to `watchpoints.rs` — which is already a filtered,
attributed, bounded, record-time recorder missing only `mclk`, ids/labels, and aggregation. The
forwarder-vs-field question is settled (a richer type layered *above* `BusEvent`), though not for the
reason this plan originally gave — see constraint 2 above.

The design pass **refuted five claims in the recon doc that commissioned it** (all re-verified
firsthand; see that doc's ERRATA banner). It also unbundles three cheaper items that were wrongly
lumped in, and records an honest shortfall: of the six sinks it fully replaces one, partially replaces
one, and correctly declines three. Notably the "near-duplicate pair" cited as motivating evidence
would be **untouched** — their real need is a promoted `ScanlineCapture` plus an `on_frame_boundary`
hook that exists nowhere in the tree, ~60 lines, with *stronger* duplication evidence than half the
recorder work.

Three open questions left for the owner rather than silently picked: `F-TRACE-MASTER` (is
function-code master attribution wanted at all, given its justifying episode does not hold up?),
`F-TRACE-PAL` (every timestamp is silently NTSC), and whether scanline capture should go first.

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

- **00:5x** — S1, S2 and the S4 design pass dispatched in parallel isolated worktrees.
- **02:2x** — **all three died on transient API 529s** within two minutes of each other, during
  startup. No work lost (none had begun). Backed off four minutes and re-dispatched **staggered**
  rather than simultaneously; all three then survived. Worth remembering: three simultaneous spawns
  may have contributed, and a stagger costs minutes while a mass failure costs the batch.
- **03:1x** — S4 design landed (`27e3d14`), refuting five claims in the recon doc that commissioned
  it. All five re-verified firsthand before acceptance; corrections committed (`a333ce1`).
- **03:5x** — **S1 and S2 both landed, verified, and merged.** Details below.

### S1 as shipped — `be6f341`, merged
`BusEventSink::stop_requested()` defaulted `false`; the loop asks once per step, after the existing
`on_step_boundary` stamp and **before** the instruction commits, so the machine is never left
mid-instruction and `record.pc` is the instruction that has *not* run — classic breakpoint semantics.
`run_until_with_sink`/`run_frames_with_sink` now return `StopRecord { reason, pc, frame, mclk }` with
`StopReason::{SinkRequested, DeadlineReached}` — fired-vs-timed-out is unambiguous by construction,
the defect the recon doc flagged in the sibling. `System::run_until_stop(max_frames, predicate)` is
the closure form. `bus::Fanout<A, B>` plus impls for `&mut S` and `Option<S>`; the hand-written
`AudioAndWatch` is now a type alias over it, safe because `AudioSink` overrides none of the differing
hooks.

**Overseer-verified independently:** fmt 0, clippy 0 warnings (both feature settings), currency
suites green, **no golden regenerated**, and the conformance scorecard re-run byte-identical
(`vdp_port_access` 16/0/16, memtest 13/13) — so the two converted budgets changed no verdict.

**★ The agent pushed back on this plan's brief, with a measurement, and was right.** Told nine budgets
were convertible, it converted two and stopped: instrumenting `m68k_bcd` over its 700-frame budget
shows VDP traffic on frames **0, 6 and 530** — 523 frames of total silence while it computes, then it
prints. Any "the screen went quiet" predicate stops 523 frames before the answer exists and reads a
blank row as a verdict: exactly the confidently-wrong-answer class §5 says to design against. Each
remaining budget needs its own measurement. It also found `k4_openbus_probe`'s frame loop is **not** a
stop-on-condition workaround — it wraps each frame in `catch_unwind` for ROMs hitting deferred Z80
opcodes, and converting it would delete the panic guard.

### S2 as shipped — `642d77e` (3 commits), merged
`crates/oracle-core/src/symbols.rs` — pure parser, `&str` in, no filesystem, no new dependencies.
Name→address, address→nearest-preceding+displacement, prefix search, the `$`-scope tree,
`validate_against_rom`. Frontend `symbol_file.rs` does the file half; watch-hit PCs are symbolised
(raw hex always retained) and symbols are **re-read and re-validated on F5**, which is the actual D7
stale-symbol scenario.

**Real-file evidence, run and read back:** 2,129 symbols / 54 modules; **279 RAM symbols where Aeon's
own `s4budget.py` reports 0**; `Player_1` listed `$FFFF8CFA` = bus `$FF8CFA`; PC `$000216` resolves to
**`EntryPoint.warm_boot+$2`** — the scope tree working on real data.

**★ A fourth trap this plan did not list, and it is the one that silently kills every RAM lookup:**
`FFFF8CFA` is a 32-bit spelling of a **24-bit** bus address. The 68000 drives 24 lines and our bus
decodes work RAM at `$E00000–$FFFFFF`, so the listing's `FFFF8CFA` *is* the machine's `$FF8CFA`. Match
a PC against the raw value and you find nothing, every time. Corollary the agent also caught:
nearest-preceding must not cross an address space, or a RAM query resolves to the last ROM symbol
+15 MB. Nuance on trap 2: `C`-for-everything is a property of Aeon's current *source*, not the format
— sigil's emitter does support `-` for equates.

**Two real bugs its own adversarial self-review caught, both on real data:** (1) locating the module
positionally invented **34 phantom modules and misfiled 125 symbols** in `s4.debug.lst`, because
macro-scoped names put the macro instance outside the module — `s4.lst` contains none of that shape,
so validating against one artifact validated one artifact; (2) the refusal was **fail-open** — deleting
the `EndOfRom` row (and truncation cuts from the end) downgraded a correct refusal to "loading it
unverified", after which 1,775/1,811 shared symbols named the wrong address.

**`deb2` finding (investigation only, as instructed):** real, and **overseer-verified to the byte** —
offset 659952, `s4.bin` 696836 bytes, difference **36884**, magic `de b2 04 02` exactly there. It
works as a shape fingerprint but is **a filter, not a proof**: `demo.lst` and `demo.debug.lst` both
declare `EndOfRom : 11224` while sharing 1,197 symbols at differing addresses, so
`validate_against_rom` is deliberately three-state. Rejected alternatives, all checked: the ROM header
is byte-identical between s4 shapes; the `.lst` carries no date/version/hash; names are not
ASCII-searchable in the appendix. **The clean fix is producer-side** — strengthening cross-repo Ask 2.

### S3 as shipped — `crates/oracle-aether`
**Transport, handshake and event machinery: complete. Methods: a thin 16, every name verbatim from
`protocol.md` §6.** New workspace crate, per the architecture call — the core's charter is
"deterministic, no-I/O" with a one-crate dependency list, so threads/sockets/JSON live outside it.
Verified: the `Cargo.lock` diff adds exactly one entry (`oracle-aether → oracle-core, serde_json`) and
`oracle-core`'s own entry is untouched; no new third-party crate entered the graph, because `serde_json`
was already there as a core dev-dependency.

**Wire conformance.** NDJSON JSON-RPC 2.0 over `AF_UNIX`; §7.1's path resolution
(`$ORACLE_SOCKET` → `$EXODUS_SOCKET` → `$XDG_RUNTIME_DIR/oracle.sock` → `/tmp/oracle.sock`); mode 0600
set *and re-read to verify*, refusing to serve otherwise; a live server on the path is refused while a
stale socket file is reclaimed; batches refused `-32600`; notifications never answered; a 1 MiB line
ceiling that drains rather than buffers, so refusing one over-long message does not desync the framing.
The §5 error table is implemented, `-32012` (no symbols) and `-32013` (not found) distinct.

**D4 made literal.** `engine::METHODS` is the function-pointer dispatch table *and* the advertised list.
Drift is impossible in **both** directions, not just the sibling's advertised-but-stale direction, and a
test asserts the identity plus that every advertised name really dispatches.

**The three non-negotiables are structural, not per-handler.** The stamp is merged after the handler
returns and overwrites any same-named key, so a handler cannot omit or shadow it — it rides on success
`result`, on `error.data`, and on every event's `params`. Arrays go through one
bounded/cursored/truncation-flagged wrapper. Approximate answers carry `caveat`: a debug read that
bypasses the bus, a whole-frame render that is not scanline-accurate, `state_hash`'s VDP-only coverage, a
nearest-preceding symbol match, a symbol listing that could not be bound to the ROM, and a `run_to` that
ended on its bound rather than its condition.

**★ The slow-client property is a test, not a claim.** A subscriber that completes the handshake and then
never reads another byte, with the queue deliberately shrunk to 4, while a second client drives 600
events: the driver's slowest single request and the machine's frame count are both asserted, and the dead
client is shown its own `droppedEvents` total when it finally reads — the loss is visible, never silent.
The mechanism is that **the emulator thread never writes to a socket at all**: it broadcasts into bounded
per-connection queues that drop oldest-first, and a blocking write only ever happens on that connection's
own writer thread.

**★ `checkpoint` was deliberately NOT shipped**, and this is the one place the brief was not delivered as
written. §9 explicitly defers save-state ops and §8 forbids inventing them, so the slice ships catalogued
`emulator/state_hash` instead and raises `emulator/checkpoint` as **CR-2** with a concrete proposed
schema. Six change requests and six recorded ambiguities are drafted (not filed, per question b) in
`docs/2026-08-14-aether-change-requests.md` — the sharpest being CR-4: §6 specifies `run_to` with **no
bound and no fired-vs-timed-out result**, which is simultaneously the hang that destroyed a frozen repro
frame and the ambiguous-success defect the core's own `StopReason` exists to prevent.

**Currency: neutral by construction and verified so** — no core file was touched, no golden regenerated,
and the conformance scorecard re-ran byte-identical (`vdp_port_access` page1 9/0/9, cumulative 16/0/16).
