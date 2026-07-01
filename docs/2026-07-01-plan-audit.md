# Plan audit — 2026-07-01 (Fable)

A full audit of the oracle-next plans (CHARTER, foundations, research-digest, the
cycle-granularity decision, the Phase-0 plan, and the 14 per-family build plans) against
the question: **is this best-in-class, and does it make sense?** Requested by the owner
before the Fable→Opus handoff; written for the agent that builds the rest.

## Verdict

**The plan is sound, internally coherent, and genuinely best-in-class on its four chosen
axes — by design, not by accident.** No shipping Genesis emulator combines what this
architecture gets for free: bit-exact determinism, mid-instruction snapshot/restore,
per-cycle bus-event streams, headless N-instance parallelism, and an agent-native MCP
surface. Exodus has the introspection but no determinism/headless; BlastEm is headless
but C and instruction-grained; Ares is clean but libco-bound; jgenesis is the modern-Rust
proof but instruction-stepped with no debug surface. The charter's deciding insight —
*for an agent-first debugger, the architecture is the product* — is correct, and the
built record backs it: the four non-negotiables are all mechanically enforced by gates,
not aspirations.

Accuracy is the one axis where the plan is (deliberately) only at parity for now:
SST-level CPU fidelity, scanline VDP. The charter's "MVP-debuggable, not VDPFIFOTesting"
line is the right call and must keep being defended against scope creep.

## What is right — endorse, do not relitigate

- **Rust, earned via jgenesis as prior art.** The borrow-checker objection was tested
  against a shipping codebase, not argued abstractly. The alternatives were disqualified
  for specific, verified reasons (libco determinism, Zig pre-1.0, C safety).
- **The single-definition micro-op + two-driver design** dissolves the feared two-path
  correctness surface — one recipe per opcode, drivers cannot diverge. The
  post-implementation addendum in the cycle-granularity brief is the strongest artifact
  in the whole record: it *falsified its own perf premise* with measurements (framework
  RTC ≈ 5.4× slower than atomic, not "near-baseline"), explained why, and identified a
  proven escape hatch (macro-inlined RTC) instead of papering over it.
- **The validation ladder is stricter than anything shipping emulators gate on**:
  determinism gate as the first CI job; 752,523 SST cases through *both* drivers,
  asserting regs/SR/RAM/prefetch/cycles *and* the per-cycle bus-transaction stream, with
  snapshot/restore exercised at every bus boundary.
- **Zero-cost instrumentation verified in code**: `BusEventSink for ()` is a no-op sink
  monomorphized away on the hot path (`bus.rs`) — the "event stream, not special cases"
  principle holds without a perf tax when nothing listens.
- **Interface reuse as a determinism firewall**: `oracle_mcp.py` unchanged, the bus
  protocol as the conformance gate, one deliberate divergence (screenshot→base64)
  recorded. Rejecting in-process PyO3 was right.
- **The build cadence itself** (data-grounded recon → dated plan doc → gated
  workflow with per-commit adversarial verifiers → owner self-verification) is a project
  asset. It has already caught a recon error (the shifts corrupt-data exclusion key) —
  the process is doing its job. Keep it verbatim.

## Findings (ordered by importance)

### 1. The declared #1 risk — the VDP — is the least-designed subsystem

Everything built so far got foundations-grade design first. The VDP, which every
document names the long pole and #1 schedule risk, exists only as bullet points.
Specifically: **the render-decode introspection API ("why is this pixel this color,
which sprites dropped on line N") is the product differentiator, and Phase 3 promises to
upgrade scanline→dot-accurate *behind that unchanged API*** — a promise that is only
cheap if the API is defined against decoded *semantics* (sprite-evaluation results,
per-line limit outcomes, priority/palette resolution) rather than renderer internals.

**Recommendation:** before any VDP (or even Z80) code, write a foundations-grade VDP
design doc: what state is latched at line start; the policy for mid-scanline
register/CRAM/VSRAM writes (the CRAM-dot class of effects — even if unrendered at first,
the *model* must have a place for them); the introspection API surface; which golden
tests are passable at scanline granularity (measure Exodus's real pass count first, as
foundations already says). This is the highest-leverage design work remaining and can
run in parallel with the 68000 tail. It is the critical path.

### 2. Timing ground truth is model-relative — decide the tiebreak now

SingleStepTests are MAME/ares-derived (foundations already flags this). The 752k
"0-mismatch" record proves fidelity to *that model*, not to silicon. Consequences:

- The nightly differential vs BlastEm **will** disagree on cycle counts somewhere.
  Decide now: **SST remains the tiebreak until Phase 3**; record divergences in the
  versioned xfail manifest instead of churning the core per-oracle.
- The **canonical `export_state` byte layout** — which foundations says to "freeze
  early" precisely because cross-backend hash equality is a footgun — is still not
  frozen. Freeze it at the integration pivot at the latest.

### 3. The uncovered 68000 dark corners are where emulators actually diverge

The grind has covered the synchronous instruction families superbly. What remains
uncovered is exactly the territory where SST is weakest and real-world divergence lives:
**trace** (no vendored case in any family sets the T bit — already flagged in the
handoff), **async interrupt delivery timing** (needs System integration),
**STOP/RESET/HALT**, **bus arbitration** (Z80 bus requests, DMA stealing 68k time), and
interrupt-at-instruction-boundary semantics. **Recommendation:** treat this cluster as
its own recon-grade push with hand-authored vectors plus differential-vs-BlastEm as the
gate — not as mop-up. It is the last place a "752k cases green" confidence could quietly
mislead.

### 4. The perf escape hatch gets harder the longer it waits — pin its trigger

The interpreter does ~45 M simple-ops/s (~60× real-time) — fine for MVP, not
best-in-class on the hot loop. Deferring the macro-inlined RTC fast path until the
micro-op vocabulary stabilizes was correct. But each family has *grown* the vocabulary
(Alu-returns-cycles with self-booked `self.cycles`, decode-time data-dependent timing,
the atomic indivisible-RMW bus mechanism, `Operand::ShiftCount`, the word-EA
shift-by-1 RMW) — every mechanism is something codegen must reproduce exactly, and
self-booking cycle arms are precisely the kind of subtlety codegen gets wrong.

**Recommendation:** (a) fix the trigger — run the perf pass right after 68000
completion + integration pivot, before the differential fleet / N-instance use cases
need the throughput; (b) the existing both-drivers-must-agree equivalence harness is
*exactly* the right codegen gate — keep it as the acceptance criterion; (c) maintain a
one-page vocabulary ledger (the mechanisms above, each with its plan-doc pointer) so the
perf pass doesn't rediscover them from 12k lines of decode.rs.

### 5. Burn down the tracked deferral debt before the pivot

The `(A7)` mode-2 plain-indirect deferral in the older families
(ADD/SUB/MOVE(A)/CMP(I)/NEG/NEGX/NOT — hundreds of vendored cases that would pass) is
tracked but should be un-deferred before the integration pivot so it doesn't fossilize.

### 6. Make license discipline a verifier-enforced hard rule for agents

The study-only tier (jgenesis GPL-3, BlastEm GPL-3, GPGX non-commercial) is documented,
but the build runs many implementation agents; an agent that casually fetches GPL source
mid-implementation is a clean-room contamination risk to the permissive core.
**Recommendation:** add to the workflow hard rules the adversarial verifiers already
enforce: implementation agents must not open study-only sources; behavior enters the
codebase only via tests and the recon docs.

### 7. Minor gaps — one line each

- **PAL** is implicitly out of scope (NTSC frame quantum hardcoded); state it explicitly
  as a non-goal so nothing silently assumes otherwise.
- **Snapshot format stability:** declare bincode snapshots version-locked to the commit
  (no compat promises) until Phase 2 — avoids accidental compat burden during churn.
- **Exodus as the VDP differential oracle** is dormant and (per this repo's own gate)
  not bit-reproducible — already mitigated by the canonical `export_state` currency, but
  budget manual triage time.
- **Z80 granularity asymmetry** (per-clock tick vs the 68k's bus-access quiesce) is
  deliberate and justified; record it in a one-page decision brief when Z80 starts so it
  doesn't read as an accident later.
- **wasm32** should be a CI compile-check (portability tripwire), not a deliverable.

### 8. The schedule estimates are stale — in the good direction

The charter's estimates (MVP ~4–6 months at side-project pace) predate the demonstrated
agent cadence: the entire shift/rotate + mul + div grind — six families, ~250k cases —
landed in roughly four days. Implementation is no longer the rate-limiter; **design
decisions are** (which is why finding 1 is the critical path). Pull the VDP design work
forward; the rest of the timeline compresses behind it.

## Recommended sequence for the next builder

1. **68000 tail** — load/store/misc cluster (MOVEM/LEA/PEA/LINK/UNLK/EXG/NOP; already
   recon-recommended in the handoff), then privileged moves + ABCD/SBCD/NBCD +
   ADDX/SUBX + MOVEP, per the proven cadence.
2. **VDP design doc** (finding 1) — in parallel with 1; it gates everything after.
3. **Exceptions/async cluster** (finding 3) as its own recon-grade push.
4. **(A7) mode-2 un-defer** (finding 5).
5. **Integration pivot** — unify `Bus68k` with `Bus`, retire `StubCpu`, wire `Cpu68000`
   into `System`; **freeze `export_state`** here (finding 2).
6. **Macro-RTC perf pass** (finding 4), gated on the equivalence harness.
7. Z80 → VDP per the charter's staged path.

Nothing in this audit changes the charter, the language, the architecture, or the
cycle-granularity decision — those are settled and correct. The findings are about
sequencing the remaining risk, not re-deciding the foundations.

## Addendum (same day): findings closed at the doc level

The owner asked Fable to fix up what could be fixed before handoff. Artifacts:

- **Finding 1 → `docs/2026-07-01-vdp-design.md`** (PROPOSED, for ratification): the
  scanline model, latch semantics, the render-decode introspection API + its stability
  contract, the VDP validation rungs, and the VDP build order (recon push first).
- **Findings 2, 4, 6, 7 → `docs/decisions/2026-07-01-audit-policies.md`** (PROPOSED):
  SST-as-tiebreak, `export_state` freeze at the pivot, the agent clean-room rule, PAL
  non-goal, snapshot version-locking, the wasm32 CI tripwire, the perf-pass trigger.
- **Finding 4 (ledger) → `docs/m68000-vocabulary-ledger.md`**: the mechanism inventory
  the macro-RTC codegen must reproduce, verified against the code.

Still open (build work, not doc work): finding 3 (exceptions/async recon push),
finding 5 ((A7) mode-2 un-defer), and the sequence in "Recommended sequence" above.
