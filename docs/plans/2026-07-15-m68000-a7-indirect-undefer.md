# oracle-next — 68000 `(A7)` mode-2 plain-indirect un-defer (coverage cleanup)

Author: session · 2026-07-15 · branch `m68000-microop-framework` (HEAD `10ac30a`).
No recon workflow / no data fetch: this is a **test-scope predicate flip only** — no new recipe, no new
vocab, no new vendored files. See the `recon-scc-tas` + `phase0-status` memories for the precedent that
motivates it.

## Context — the SST grind is finished; this is the last CPU-level loose end

The full 124-file SingleStepTests 680x0 suite is covered (threshold **970278**, ItoCCR was the last file).
The ONE residual intra-family carve-out that has ridden along since the 2026-06-25 EA-machinery push is the
**plain `(A7)` mode-2 indirect** (`mode == 2 && reg == 7`) on the *older* families. It was deferred out of
early caution and the convention propagated; the newer families (CLR/TST/Scc/TAS/bit-ops/shifts/MUL/DIV/…)
all cover it and pass. This push flips the residual to `covered` on the older families so the whole suite is
100% in scope with no mode carve-out anywhere.

## Why this is safe (no new behaviour — decode already handles it)

- The deferral lives **only in the test's `covered()` predicate**, never in `decode`/`ea.rs`. The plain-indirect
  EA arms (`(2, _) =>`) resolve the address as `regs.addr_reg(reg)` **uniformly for reg 0–7** — there is no A7
  guard, no panic, no special path. A7 routes to `ssp`/`usp` via `addr_reg`; every SST case is supervisor so
  A7 = SSP.
- Plain `(A7)` is **structurally simpler** than its in-scope siblings: `(A7)+`/`-(A7)` need the byte-step-by-2
  rule (`step_bytes`) to keep SP even; plain `(A7)` has no auto-(in/de)crement at all. Those harder siblings are
  already in scope and passing, as is every odd-A7 address-error case (the E3/E4 abort installs the group-0
  frame). So plain `(A7)` exercises nothing new.
- Precedent, pinned to data: `recon-scc-tas` established NEG.b's `(A7)` m2 is byte-identical in structure to
  CLR.b's `(A7)` m2, which passes. CLR/TST/Scc/TAS/bit-ops/shifts cover `(A7)` m2 with 0 mismatch.

Empirical guarantee: each `run_case(t)` asserts regs/SR/RAM/prefetch/cycles + the per-cycle transaction stream
and snapshot/restore at every bus boundary. If any flipped `(A7)` case is wrong, the run **panics** with the
exact mismatch — a silent pass is impossible.

## Scope — the families/sites still deferring plain `(A7)` mode-2

All in `crates/oracle-core/tests/singlestep_m68000.rs`. Each flips `reg != 7` → in-scope (folding mode 2 into
the adjacent alterable-memory arm), or removes the `if …mode==2 && reg==7 {return false}` guard:

1. **MOVE** (`move_in_scope`) — source AND dest, both sizes (the two `return false` guards).
2. **MOVEA** (`movea_in_scope`) — source.
3. **CMP** (`cmp_in_scope`) — `Cmp` source + `Cmpi` dest arm.
4. **CMPA** — the `CmpClass::Cmpa` arm in `covered()`.
5. **AND / OR** (`and_or_in_scope`) — `<ea>,Dn` source + `Dn,<ea>` dest.
6. **EOR** (`eor_in_scope`) — `Dn,<ea>` dest.
7. **NEG / NEGX / NOT** — the three `0x44/40/46xx` arms in `covered()`.
8. **ADD / SUB** — all six form arms (.w/.b/.l × `Dn,<ea>`/`<ea>,Dn`).
9. **ADDA / SUBA** — the two `An = An ± src` arms.

The newer families need **no change** (already in scope). Update the now-stale prose comments that say "A7
mode-2 deferred" / "the pre-existing `(A7)` mode-2 plain-indirect deferral" to state it is now covered.

## Verification protocol & anti-cheating

1. Flip the predicates + fix comments.
2. Run the SST sweep (`add_sub_match_singlesteptests`) in the **background** (compile+run > 10 min foreground
   cap) with `--nocapture`; a temporary `eprintln!` of the running total captures the new count. Confirm GREEN
   (all flipped `(A7)` cases pass) → set `ran >=` to the **new true count** (raise, never lower) and rewrite the
   threshold comment. Remove the temp print.
3. Full gate MYSELF (not agent word): `cargo test -p oracle-core --test determinism_gate --test proptests`;
   `cargo fmt --all -- --check`; `cargo clippy --all-targets -- -D warnings`; `cargo test --workspace` (SST
   sweep ~600-850s — 600000 ms+ timeout / background).

**Hard rules:** SST is ground truth — this only RAISES coverage; no assert weakened, no `#[ignore]`/`#[cfg]`,
no threshold lowered. **No** family's genuine deferral is touched: the illegal `An`-direct byte source (MOVE.b/
ADD.b/AND `<ea>,Dn` mode 1), PC-rel/`#imm` as a *destination*, `An`-direct as an alterable dest — all stay out.
Only `mode == 2 && reg == 7` flips. Odd-EA address errors remain in scope via E3/E4 (no parity filter). Commit
`feat(m68000): …` with a body; **NO `Co-Authored-By: Claude` trailer**; the commit stays individually fmt-clean.

## Commit plan

One commit (a pure coverage flip, no recipe work):
- [ ] **Un-defer plain `(A7)` mode-2 across MOVE/MOVEA/CMP/CMPI/CMPA/AND/OR/EOR/NEG/NEGX/NOT/ADD/SUB/ADDA/SUBA.**
  Flip the predicates + comments; raise `ran >=` to the measured count (was 970278); update the threshold
  comment with the delta. Optionally add one or two anchor assertions on a flipped `(A7)` case (e.g. an `ADD.w
  (A7),Dn` and a `NEG.w (A7)`) — nice-to-have; the existing `(An)` anchors already prove the shape.

## Risks (very low)
- **Clippy arm-merge:** folding `2 => reg != 7` into `2..=6 => true` may let clippy suggest further merges — keep
  arms clean and re-run `clippy -D warnings` before commit.
- **A hidden genuine reason** a specific family deferred `(A7)` m2 (beyond convention). Mitigated: `run_case`
  panics on any mismatch, per-family — I'd see exactly which family/case and can re-defer it with a documented
  reason rather than shipping a weakened assert. Not expected (decode is A7-uniform; siblings already pass).
