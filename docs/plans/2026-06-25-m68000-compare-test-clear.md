# oracle-next — 68000 compare/test/clear family (CMP, CMPM, CMPI, CMPA, TST, CLR, MOVEQ)

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:test-driven-development` — each behaviour gets a
> *failing test first*. Commits are sequential and dependent (N0→N6); each leaves the tree fully green before
> the next. **GROUND TRUTH IS THE VENDORED SingleStepTests STREAM.** The recipe shapes, flag formulas, cycle
> counts and orderings below were reconnoitred against the real vendored data (a 2-agent recon over CMP.*/
> CMPA.*/TST.*/CLR.*), but every exact cycle placement and bus-transaction ORDER is **TDD-discovered against
> the data — the SST stream wins, never weaken the assertion.** See *Anti-cheating rules*. Never modify
> `../oracle/`.

## Goal & scope

Add the **compare / test / clear / quick-load** integer family on the proven micro-op framework
(`m68000::{microop,ea,decode}`): **CMP**, **CMPM**, **CMPI**, **CMPA**, **TST**, **CLR**, **MOVEQ**. These are
among the most common instructions in real code and are almost entirely *reuse* of the existing `ea_src`/
`ea_dst` builders + a flag-only ALU. The structurally-new shapes are small: a **flag-only compare** (sets
N/Z/V/C, **preserves X**, writes nothing) and **CLR**'s **read-then-write** (it reads the EA, discards it, then
writes 0). Odd word/long EAs are already coverable via the execution-time address-error abort (built in the
exceptions push), so they are **in scope**, not deferred.

**KEY DATA FACT — the `CMP.<sz>.json` files are 3-way mixes.** Each contains CMP `<ea>,Dn` **+** CMPM
`(Ay)+,(Ax)+` **+** CMPI `#imm,<ea>`, all mislabeled "CMP" in the `name` field — **classify by opcode, never
by name.** Decoding all three fully covers those files. `CMPA.w/.l` are pure CMPA. (CMPM/CMPI are NOT separate
files; ADDQ/SUBQ/ADDI/SUBI/ANDI/ORI/EORI/plain-CMPI-file do **not exist** in this suite — only `*toCCR`/
`*toSR` immediate forms do, already handled or out of scope.)

**Deferred (not this push):** the logic family (AND/OR/EOR reg-forms — own files), ADDA/SUBA, NEG/NEGX/NOT/EXT/
SWAP/Scc/TAS, bit ops, shifts/rotates, MUL/DIV, MOVEM/LEA/PEA — all later grind pushes. The integration pivot.

## Flag & value formulas (verified statistically, 0 mismatches)

- **CMP / CMPM / CMPI** at size `s` computing `dst − src` (CMP: `Dn − <ea>`; CMPM: `(Ax) − (Ay)`; CMPI:
  `<ea> − #imm`): N/Z/V/C **exactly as `SUB`** at size `s`, but **X is PRESERVED** (never written), and **no
  write-back**. (So `AluOp::Cmp` = the `sub_*` flag computation with `(ccr & !CCR_X) | (live X)`.)
- **CMPA** at size `s` (w/l): `An(full 32) − src`, where `src = sign_extend16→32(word)` for `.w` or the full
  long for `.l`; computed at the **long boundary**; N/Z/V/C set, **X preserved**, no write. (`AluOp::Cmpa`
  sign-extends `b` internally when `size==Word`, mirroring `AluOp::MoveA`.)
- **TST**: `operand − 0` → N = msb(operand), Z = (operand==0), **V=0, C=0**, X preserved, no write. (Reuses
  `AluOp::Cmp` with `b = Operand::Zero`.)
- **CLR**: writes **0** to the EA/register; **Z=1, N=0, V=0, C=0, X preserved** — exactly `move_flags(0)`.
  (Reuses `AluOp::Move` with `a = Operand::Zero`; the write value is the parked 0.)
- **MOVEQ**: `Dn = sign_extend8(opcode & 0xFF)` (full 32); N = msb, Z = (value==0), V=0, C=0, X preserved.
  (Reuses `AluOp::Move` at `Size::Long` with `a = Operand::BranchDisp8`.)

## Opcode layouts (classify by opcode)

- **CMP** `1011 ddd 0SS mmm rrr` (0xB000/B040/B080, opmode SS=0/1/2 → b/w/l): `Dn − <ea>`, all 12 source modes
  (An-direct legal for **w/l only**, illegal for `.b`).
- **CMPM** `1011 xxx 1SS 001 yyy` (opmode bit2 set, 4/5/6 → b/w/l; EA mode field forced to 001): `(Ax) − (Ay)`,
  both `(An)+` post-increment reads — src @ `(Ay)+` **first**, then dst @ `(Ax)+`.
- **CMPI** `0000 1100 SS mmm rrr` (0x0Cxx, SS bits 7-6): `<ea> − #imm`; immediate is the **first** extension
  word(s) (`.l` = two words), then the EA's extension words; dest EA = **data-alterable** (read & discard, no
  write).
- **CMPA** `1011 aaa 0 11/111 mmm rrr` (0xB0C0 word / 0xB1C0 long): `An − src`, all 12 source modes.
- **TST** `0100 1010 SS mmm rrr` (0x4A00/4A40/4A80): data-alterable EA (Dn + memory-alterable; **no** An,
  PC-rel, #imm).
- **CLR** `0100 0010 SS mmm rrr` (0x4200/4240/4280): data-alterable EA; **read-then-write** (Dn-dest: no
  memory).
- **MOVEQ** `0111 ddd 0 dddddddd` (0x7000 | dn<<9 | imm8): `Dn ← sign_extend8(imm8)`.

## Recipe shapes (TDD-pinned to the data)

- **CMP `<ea>,Dn`:** `ea_src(src, size)` reads the operand; `Alu{Cmp, size, a: Dn(size), b: operand, dst:
  None}` (Dn is the minuend). Flag-only — no write. Register/#imm sources have no data read (only the trailing
  prefetch); memory sources reuse the `(k−1)PF→READ→PF` placement.
- **CMPM `(Ay)+,(Ax)+`:** read src @ `(Ay)+` (then `AdjustAddr Ay += step`), read dst @ `(Ax)+` (then
  `AdjustAddr Ax += step`), `Prefetch`, `Alu{Cmp, size, a: dst_read, b: src_read, dst: None}`. `.l` = two reads
  per operand + `Combine32` (src hi/lo, dst hi/lo).
- **CMPI `#imm,<ea>`:** capture the immediate (one ext word → `ImmWord`; `.l` → two ext words assembled like
  `#imm.l`), read the EA (data-alterable; discarded), `Alu{Cmp, size, a: ea_operand, b: immediate, dst:
  None}`. No write. Pin the immediate-then-EA prefetch interleave to the data.
- **CMPA `<ea>,An`:** `ea_src(src, size==w?Word:Long)` reads the source; `Alu{Cmpa, size, a: AddrReg(An), b:
  source, dst: None}` (Cmpa sign-extends `b` when `size==Word`, computes at Long). No write.
- **TST `<ea>`:** `ea_src(src, size)`; `Alu{Cmp, size, a: operand, b: Zero, dst: None}`. No write.
- **CLR `<ea>`:** `ea_dst(dst, size, make_alu = Alu{Move, size, a: Zero, dst: Scratch(1)})` — the existing RMW
  path: read EA (discarded), prefetch, `Move{Zero}` (sets flags + parks 0), write 0. `.l` reuses `ea_dst_long`
  (read hi@EA/lo@EA+2, write lo@EA+2/hi@EA — the existing reversed long-store order). Dn-dest: no `ea_dst` —
  `Alu{Move, size, a: Zero, dst: dn_dest(Dn,size)}` + prefetch (CLR.l Dn = 6 cyc, one trailing idle — pin it).
- **MOVEQ:** `Alu{Move, Long, a: BranchDisp8, dst: DataReg(Dn)}` + `Prefetch` (len 4).

## Vocabulary additions (introduced in the commit that first USES it — keep `-D warnings` clean)

| Item | Signature / semantics | First used |
|---|---|---|
| `AluOp::Cmp` | size-aware `dst − src` flags = `sub_*` N/Z/V/C with **X preserved** (`(sub_ccr & !CCR_X) \| (regs.sr & CCR_X)`); with `Dest::None` writes nothing. | N0 |
| `Dest::None` | flag-only — `exec_one`'s `Alu` arm sets the CCR and writes no register/scratch. | N0 |
| `AluOp::Cmpa` | `An(long) − src`, `src` sign-extended word→long when `size==Word` else full long; long-boundary N/Z/V/C, **X preserved**; flag-only (`Dest::None`). Mirrors `AluOp::MoveA`'s internal sign-extension. | N3 |

CLR reuses `AluOp::Move` + `Operand::Zero` (no new vocab); MOVEQ reuses `AluOp::Move` + `Operand::BranchDisp8`
(no new vocab); CMPM/CMPI reuse `Read`/`AdjustAddr`/`Combine32`/`ImmWord` (no new vocab). `MAX_OPS`/
`SCRATCH_SLOTS` should not need a bump (CMPM.l = 4 reads + Combine32×2 + Cmp + prefetch ≈ 9 ops; CMPI.l ≈ a
few more) — measure; bump only if a recipe exceeds 20/10 (document the worst).

## Clean vs deferred — the `covered()` strategy

The decisive subtlety: **`covered()` must classify the CMP.* files by opcode**, not by name. Add a small
`cmp_class(opcode) -> {Cmp, Cmpm, Cmpi, Cmpa, None}` helper and admit the classes as each commit lands. Odd
word/long EAs are **in scope** (the abort handles them — confirmed: every odd-EA case is a clean group-0
14-byte vector-3 frame, ~38% of word/long cases). The only deferrals are mode-scope/illegal forms **absent
from the data**: An-direct as a `.b` source (CMP.b has 0), and (until their commits) the not-yet-decoded
classes. TST/CLR admit exactly `{Dn,(An),(An)+,-(An),d16(An),d8(An,Xn),abs.w,abs.l}` (An/PC-rel/#imm are
absent). **Threshold:** raise `ran >=` to the measured true count per commit (raised, never lowered). Estimated
final ~**286,353** (current 189,573 + 12 files × 8065); **measure & pin at N6.**

## Commit plan (TDD, sequential, each fully green before the next)

Each commit: extend `tools/fetch-tests.sh` `FILES` + add the real sha256 to `tools/singlesteptests.sha256` +
extend the runner `FILES`; add vocabulary in the commit that uses it; decode arm + recipe; the `covered()`/
class filter; both-drivers agreement + a snapshot/restore anchor for the new shape; full gate; conventional
`feat(m68000):` commit ending in the `Co-Authored-By` line.

- [ ] **N0 — CMP `<ea>,Dn` + the flag-only shape.** Add `AluOp::Cmp` + `Dest::None`. Decode CMP (opmode 0/1/2),
  all source modes (An-direct w/l only). Vendor CMP.b/.w/.l; `covered()` admits the **Cmp** class only (CMPM/
  CMPI deferred to N1/N2). Anchors: a Dn-source, an An-source (w/l), a memory source (each size), an odd-EA
  case (address-error frame via the abort). Snapshot anchor.
- [ ] **N1 — CMPM `(Ay)+,(Ax)+`.** Two postinc reads (src@Ay first) + `Cmp`. Widen `covered()` to the **Cmpm**
  class. Anchors per size (`.l` = 4 reads). Snapshot anchor on `.l`.
- [ ] **N2 — CMPI `#imm,<ea>`.** Immediate (1 word / `.l` 2 words) + EA read (discard) + `Cmp`. Widen
  `covered()` to the **Cmpi** class. **CMP.* now fully covered.** Anchors per size + a memory-dest CMPI.
- [ ] **N3 — CMPA `<ea>,An`.** Add `AluOp::Cmpa`. Decode CMPA.w/.l (all source modes). Vendor CMPA.w/.l.
  Anchors: Dn-source (sign-extend, w), An-source, memory (w/l), #imm. Snapshot anchor.
- [ ] **N4 — TST `<ea>`.** `Cmp` vs `Zero`, flag-only. Vendor TST.b/.w/.l; `covered()` = the data-alterable
  set. Anchors: Dn, memory (each size). Snapshot anchor.
- [ ] **N5 — CLR `<ea>`.** Reuse `ea_dst`/`ea_dst_long` with `make_alu = Move{Zero}`; Dn-dest direct. Vendor
  CLR.b/.w/.l. Pin the **read-then-write** order + the CLR.l Dn 6-cyc idle + the `.l` reversed long-store.
  Anchors: CLR Dn (each size, incl `.l` 6-cyc), CLR (An) read-then-write, CLR -(An)/(An)+, an odd-EA CLR (the
  fault is the READ, low5=0x15). Snapshot anchor across a CLR (An).
- [ ] **N6 — MOVEQ.** Reuse `Move{BranchDisp8}` → `DataReg(Dn)` + prefetch. Vendor MOVE.q. Anchor `7cb5`
  (sign-extend). **Measure + pin the final threshold.** Snapshot anchor.

## Vendoring (pinned commit `e0d5ece9670205cc84a0101081837deb446f86a3`)

Add to `tools/singlesteptests.sha256` (real sha256 of the pinned `.gz`, already fetched + verified locally):

```
dbad35609f7b83407898a4e0fbc110737b340d486e60c2e25a3ea0360e3e7002  CMP.b.json.gz
2d89021348662b52869266874627a452b490a7c753165389996c1f4c1b8678e0  CMP.w.json.gz
a27630dbeddf4ef4452b74892a14a15a66dedbfae1ac5f23c404d335674bae0a  CMP.l.json.gz
4e48082041062cc0cdbdf78a6f4d34846e11118df78b2f0ce030c0cfb5449495  CMPA.w.json.gz
28c1f1d7e4547e2e2eb263e62400a76a87e2d08d3ebc7bb24a5a8dd971183f06  CMPA.l.json.gz
d93b43e1db22659b8f7e837841a25137f61a357aabb439dcab4ef604d670c9dc  TST.b.json.gz
2087a65f6fbb37d60ca7ee2bf4c24af84a41500a9856d4d204a4070fd627df76  TST.w.json.gz
11145aeb99a3bfee6628864bc20426a9d7ff38e797e125da8fefa5fb7f0480a6  TST.l.json.gz
2bad3ce4cdd234cbacc6ea2d1a1a1f607963c54f57804114cbc88413889d5b3d  CLR.b.json.gz
04dd3f8adf4a40591806a076a79347ad5ff7da88d28855c7a187f454d5d58442  CLR.w.json.gz
865f6cbc813e8e41958cf719637685201653b5b672d66e287b51921c32a0008e  CLR.l.json.gz
49eccdff108ff7f6dbf42117210b1223b4d67d392c1f84f23f9e838cc02bcf4d  MOVE.q.json.gz
```

## Verification protocol (every commit) & anti-cheating rules

Not done until, from the repo root: `cargo test -p oracle-core --test determinism_gate --test proptests`
green (most-guarded), `cargo fmt --all -- --check` clean, `cargo clippy --all-targets -- -D warnings` clean,
`cargo test --workspace` green; committed with a conventional `feat(m68000):` message ending in
`Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`. Read `.github/workflows/ci.yml`; run what CI runs.

**Anti-cheating (hard rules — a verifier checks these):**
- **SST is ground truth.** Fix the recipe/code to match the suite. NEVER weaken an assertion, broaden
  `covered()` to skip cases that should pass, lower the `ran >=` threshold below the true covered count,
  `#[ignore]`/`#[cfg]`-out a test, or comment out an assert. `covered()` may only defer documented classes
  (not-yet-decoded CMP-file classes between N0–N2; the absent illegal forms).
- **Classify the CMP.* files by OPCODE, not the `name` field** (the names all say "CMP.<sz>" but the files mix
  CMP + CMPM + CMPI). Mis-classifying inflates "An-direct" counts (those are CMPM) or "#imm" (those are CMPI).
- Both-drivers-agree and snapshot/restore-at-every-bus-boundary stay intact for every new form; the
  determinism gate stays the most-guarded job.
- No `Rc`/`RefCell`/`unsafe`/`HashMap`/floats in hashed state; `MicroState` stays fixed-size bincode.
  Never touch `../oracle/`.

## Risks

- **CMP-file 3-way classification** — the load-bearing subtlety; a shared `cmp_class(opcode)` used by both
  `decode` and `covered()` (or a debug-assert they agree) prevents drift.
- **CMPI immediate-then-EA ordering** (`.l` = two-word immediate before the EA ext words) — TDD-pin the
  prefetch interleave; do not assume from memory.
- **CMPA `.w` sign-extension** — `AluOp::Cmpa` sign-extends `b` internally (size-aware); pin against a Dn-source
  `.w` anchor whose source word has the high bit set.
- **CLR is read-then-write, not write-only** — it reuses the `ea_dst` RMW path (the read can itself fault →
  address error, low5=0x15); do NOT model it as write-only. CLR.l Dn has a 6-cyc trailing idle (≠ TST.l Dn 4).
- **MAX_OPS/SCRATCH_SLOTS** — measure the CMPM.l / CMPI.l recipes; bump only if needed (document the worst).
