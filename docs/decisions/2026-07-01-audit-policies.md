# Decision brief: standing policies from the 2026-07-01 plan audit

**Status: RATIFIED 2026-07-01 (owner).** Small, cheap-to-adopt
policies that close findings 2, 4, 6, and 7 of `docs/2026-07-01-plan-audit.md`. None
changes the charter or architecture; each pre-decides a call that would otherwise be
made ad-hoc mid-build.

## 1. Timing ground truth + the tiebreak (finding 2)

SingleStepTests are MAME/ares-derived — our 752k-case record proves fidelity to *that
model*, not to silicon. **Policy: SST remains the single tiebreak for 68000 cycle/bus
behavior until Phase 3.** When the nightly differential vs BlastEm disagrees with SST
(it will, somewhere), the divergence goes into the versioned xfail manifest with a note
— the core is *not* churned per-oracle. Phase 3 revisits the manifest with better ground
truth (test ROMs on hardware, fx68k/microcode sources).

## 2. Freeze `export_state` at the integration pivot (finding 2)

The canonical `export_state` byte layout (the cross-backend differential currency —
foundations already rules out cross-backend hash equality) **must be frozen no later
than the integration pivot** (when `Cpu68000` enters `System`). It needs a version
field, fixed region order + sizes (the `state_hash` spec is the model), and a written
spec doc. Anything not yet emulated serializes as a fixed placeholder region so the
layout doesn't shift as chips land.

## 3. Clean-room rule for build agents (finding 6)

**Implementation and verification agents must never open study-only sources** —
jgenesis (GPL-3), BlastEm (GPL-3), Genesis Plus GX (non-commercial) — in any form
(WebFetch, clone, vendored copy). Behavior enters the codebase only via test vectors,
test ROMs, official/permissive documentation (Sega docs, Plutiedev, SpritesMind
threads), and *behavioral* differential experiments over the bus. This joins the hard
rules the adversarial verifiers already enforce (no weakened asserts, no threshold
lowering). Rationale: the permissive-core license posture is a charter commitment, and
agent workflows make accidental contamination one careless fetch away.

## 4. PAL is an explicit non-goal until further notice (finding 7)

The frame quantum, line count, and clocks are NTSC and may be hardcoded. Any PAL
support is a deliberate future decision, not an implicit TODO — nothing should carry
speculative PAL parameterization.

## 5. Snapshot format is version-locked to the commit until Phase 2 (finding 7)

bincode snapshots carry no cross-version compatibility promise while the state structs
churn. A snapshot is valid only for the commit that wrote it. Revisit (add a version
header + migration policy) when Phase 2 makes rewind/replay a daily-use feature.

## 6. wasm32 is a CI tripwire, not a deliverable (finding 7)

Add `cargo check --target wasm32-unknown-unknown -p oracle-core` to CI as a portability
tripwire (it fails the moment I/O or threads leak into the core). No wasm runtime work
until a consumer exists.

## 7. The macro-RTC perf pass has a fixed trigger (finding 4)

The macro-inlined run-to-completion fast path (the ratified escape hatch from the
cycle-granularity addendum) is scheduled **immediately after 68000 completion + the
integration pivot, before the differential fleet / N-instance workloads**. Its
acceptance gate is the existing both-drivers equivalence harness (identical final state
+ transaction stream + cycles across the full SST sweep). The micro-op vocabulary it
must reproduce is inventoried in `docs/m68000-vocabulary-ledger.md`.
