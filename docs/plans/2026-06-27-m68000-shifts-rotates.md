# oracle-next — 68000 shifts / rotates (ASL / ASR / LSL / LSR / ROL / ROR / ROXL / ROXR)

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:test-driven-development` — each behaviour gets a
> *failing test first*. Commits are sequential and dependent (S0→S7); each leaves the tree fully green before
> the next. **GROUND TRUTH IS THE VENDORED SingleStepTests STREAM.** Every flag/value/timing formula below was
> derived and **0-mismatch verified against the real vendored `.json.gz` for all 24 files** (193,558 / 193,560
> cases — exhaustive Python; the only 2 misses are provably-corrupt SST entries, see *The two corrupt ASL.b
> entries*). Every cycle placement and bus-transaction ORDER is **TDD-pinned against the data — the SST stream
> wins, never weaken the assertion.** See *Anti-cheating rules*. Never modify `../oracle/`.

## Goal & scope

Add the eight shift/rotate ops on the proven micro-op framework (`m68000::{microop,ea,decode}`). Each op has
**three forms**, classified by opcode bits:

- **Register, immediate count** (`1110 ccc d ss 0 tt rrr`, bit 5 = 0): shift `Dn` (bits 2-0) by a count
  `ccc` (bits 11-9) **if `ccc != 0` else 8** — so **1-8**. `.b/.w/.l` per `ss` (bits 7-6 = 00/01/10).
- **Register, Dn count** (`… 1 tt rrr`, bit 5 = 1): count = `D[ccc] & 63` (**0-63**, mod 64) — a **live `Dn`
  read at decode** (the Scc-`n2` / DBcc-counter / bit-ops `pos>=16` precedent). `.b/.w/.l`.
- **Memory shift by 1** (`1110 0TTd 11 mmm rrr`, bits 7-6 = **11**): **WORD only**, count always 1. Lives ONLY
  in the **`.w` files** (the `.b`/`.l` files have NO memory form). Data-alterable EA: `(An)`/`(An)+`/`-(An)`/
  `d16(An)`/`d8(An,Xn)` (2-6) + `abs.w`/`abs.l` (7/0, 7/1), **including `(A7)`/`(A7)+`/`-(A7)`**. `TT` (bits
  10-9) = 00 AS / 01 LS / 10 ROX / 11 RO.

Direction: **bit 8 = 1 LEFT, 0 RIGHT** (both register and memory forms). Type: register **bits 4-3**, memory
**bits 10-9** — 00 AS / 01 LS / 10 ROX / 11 RO. **Every file is 100% PURE** for its op+size (no contamination,
no MOVEP/MULU mix — `0xExxx` is a dedicated opcode space).

**Flags (the surface of this push — most flag-subtle family yet):**
- **N** = msb(result), **Z** = (result == 0) — always, all ops.
- **C** = the last bit shifted out (exact per-op formulas below). **ASR carry quirk: C = 0 when `cnt > n`**
  (NOT the sign bit).
- **X = C** for **ASL/ASR/LSL/LSR/ROXL/ROXR**. **ROL/ROR do NOT touch X** (X preserved).
- **V** is set **only by ASL** (sign bit changed at any point during the shift); **V = 0 for all 7 others**.
- **ROXL/ROXR thread X** through an `(n+1)`-bit rotate.
- **Zero count** (`cnt == 0`, possible only in the Dn form): value unchanged; V = 0; **X unchanged**; **C = 0
  for AS/LS/RO**, **C = X for ROX** (C set to the incoming X); N/Z from the unchanged operand.

**Deferred (not this push):** MULU/MULS + DIVU/DIVS; MOVEM/LEA/PEA/LINK/etc; ABCD/SBCD/NBCD; ADDX/SUBX; the
remaining exceptions; the integration pivot. Also NOT in scope: shifting on a `(d8,PC,Xn)`/`#imm` operand
(memory shift is only the data-alterable set — never PC-relative / immediate).

## Recon findings (exhaustive, 0-mismatch verification)

All formulas reproduce the vendored `final` registers + `final.sr` + the written value **bit-exact** on every
non-corrupt case across all 24 files (`vendor/` is gitignored; reproducible from the sha256 manifest). Verified
both via a bit-by-bit simulator AND an independent closed-form model — they agree on all 193,558 cases.

- **Forms / counts.** 24 files, 8065 cases each, pure per op+size. `.b/.l` = register-only (imm + Dn, no
  memory, no faults). `.w` = register + memory-shift-by-1 (≈1000 odd-EA address-error cases per `.w`, in scope
  via the E3/E4 abort — see *Memory shift*). Immediate counts span **1-8**; Dn counts span **0-63**.
- **C / X / V / N / Z** — see *Flag & value formulas* (the verified closed forms).
- **The ASR carry quirk** (the one model trap): for `ASR`, `C = bit(cnt-1)` when `1 <= cnt <= n`, else **0**
  ("last bit shifted out of the OPERAND" — beyond the operand's own bits the carry is 0, IDENTICAL to LSR,
  even though the *value* sign-extends). A naive "C = sign for `cnt > n`" mismatches 1642/1063/1031 ASR.b/w/l
  cases.
- **ASL V** (the other trap): set iff the sign bit changes at ANY point during the shift. The clean closed
  form (verified): for `cnt >= n`, `V = (x != 0)` (NOT `x != 0 && x != mask` — `x == mask` shifts a 0 in and
  the sign DOES change → V=1); for `cnt < n`, `V = (top != 0 && top != topmask)` where `top = x &
  (((1<<(cnt+1))-1) << (n-1-cnt))` (the top `cnt+1` bits are not all-equal).
- **ROXL/ROXR thread X.** Treat `{X:operand}` as an `(n+1)`-bit register (X above the msb), rotate by
  `cnt % (n+1)`; the final bit ejected into X is both the new X and C. **`cnt == 0` → C = X, X unchanged.**
- **ROL/ROR never touch X.** C = the last bit rotated out; `cnt == 0` → C = 0, X unchanged.
- **Timing (uniform across ALL 8 ops):** **register** (imm AND Dn, identical base) = **`.b/.w` 6 + 2·cnt**,
  **`.l` 8 + 2·cnt**. Decode-time data-dependent for the Dn form (cnt = `D[ccc]&63`, up to 63 → up to 126 idle
  cyc). **Memory shift-by-1** (word, all ops identical): `(An)` 12, `(An)+` 12, `-(An)` 14, `d16(An)` 16,
  `d8(An,Xn)` 18, `abs.w` 16, `abs.l` 20.
- **Counts.** Every file 8065 in scope **EXCEPT `ASL.b` = 8063** (2 corrupt entries excluded). Total in scope
  **= 193,558 → threshold 526705 → 720263** (+193,558).

### The two corrupt ASL.b entries (EXCLUDE exactly these 2)

`ASL.b` cases `e502 [ASL.b Q, D2] 1583` (d2 `cdfb7fbe`→`2e5e4304`) and `e502 [ASL.b Q, D2] 1761` (d2
`417c7e7d`→`6461d390`) are **internally self-contradictory**: their transaction stream is
`[prefetch-refill@4, idle@6]` — a register-only `ASL.b #2,D2` with **NO memory access** (10 cyc) — which by
construction CANNOT change D2's upper 24 bits, yet `final.d2` is full-register garbage. No shift/rotate/unary/
binary transform maps init→final, and the two scramble *differently* (no shared transform). They are identical
in the SST repo's current HEAD (stable, not a transient glitch — a baked-in generator bug). A **correct** 68000
produces `cdfb7ff8` / `417c7ef4`. **Including them would force a WRONG implementation**; excluding them is the
opposite of under-coverage. `covered()` excludes exactly these 2 (keyed on `opcode == 0xE502 && d2 ∈
{0xcdfb7fbe, 0x417c7e7d}`) → **`ASL.b` in scope = 8063**, the only file not 8065. The S0 agent MUST assert the
exclusion removes **precisely 2** cases (a count check) and that `ASL.b` runs exactly **8063**.

## Flag & value formulas (verified, 0 mismatches — Rust-ready)

Implement eight `AluOp`s (`Asl/Asr/Lsl/Lsr/Rol/Ror/Roxl/Roxr`). The exec resolves `a` = the operand (already
size-masked: `DataRegLowN`/scratch), `b` = the count source, `size` → `n = 8/16/32`. Let `mask = (1<<n)-1`,
`signbit = 1<<(n-1)`, `x = a & mask`, `cnt = (b_resolved & 63)`, `xin = (sr >> 4) & 1`. Return `(result, ccr)`;
the shared write-back `regs.sr = (regs.sr & 0xFF00) | ccr` + the `dst` write follow (as for every other Alu).
**Guard all Rust shifts** — shifting a `u32` by `>= 32` is UB; branch on `cnt >= n` / `r == 0`.

- **ASL**: `res = if cnt < n { (x << cnt) & mask } else { 0 }`; `C = if cnt == 0 {0} else if cnt <= n {(x >>
  (n-cnt)) & 1} else {0}`; **X = C**; **V** = (see *ASL V* above); `cnt == 0` → C=0, X kept, V=0.
- **LSL**: as ASL but **V = 0** always.
- **LSR**: `res = if cnt < n { x >> cnt } else { 0 }`; `C = if cnt==0 {0} else if cnt <= n {(x >> (cnt-1)) &
  1} else {0}`; **X = C**; V = 0.
- **ASR**: `res = if cnt >= n { if x & signbit != 0 { mask } else { 0 } } else { (x >> cnt) | (if x & signbit
  != 0 { (mask << (n-cnt)) & mask } else { 0 }) }`; `C = if cnt==0 {0} else if cnt <= n {(x >> (cnt-1)) & 1}
  else {0}` (**the `cnt > n` → 0 quirk**); **X = C**; V = 0.
- **ROL** (no X): `r = cnt % n`; `res = if cnt==0 || r==0 { x } else { ((x << r) | (x >> (n-r))) & mask }`;
  `C = if cnt==0 {0} else (x >> ((n - (cnt % n)) % n)) & 1`; **X PRESERVED**; V = 0.
- **ROR** (no X): `r = cnt % n`; `res = if cnt==0 || r==0 { x } else { ((x >> r) | (x << (n-r))) & mask }`;
  `C = if cnt==0 {0} else (x >> ((cnt-1) % n)) & 1`; **X PRESERVED**; V = 0.
- **ROXL** (thread X): `cnt==0` → `res = x, C = xin, X = xin (unchanged), V = 0`. Else: `per = n+1`,
  `eff = cnt % per`, `comb = ((xin << n) | x)` in `per` bits; `comb = if eff==0 {comb} else { ((comb << eff) |
  (comb >> (per-eff))) & ((1<<per)-1) }`; `res = comb & mask`; **C = X = (comb >> n) & 1**; V = 0.
- **ROXR** (thread X): `cnt==0` → same as ROXL's zero case. Else: same `comb`; `comb = if eff==0 {comb} else {
  ((comb >> eff) | (comb << (per-eff))) & ((1<<per)-1) }`; `res = comb & mask`; **C = X = (comb >> n) & 1`; V=0.

`ccr` assembly: `N` if `res & signbit`, `Z` if `res == 0`, `V` per above, `C`/`X` per above. For **ROL/ROR**
re-inject the live X: `ccr = (regs.sr & CCR_X) | N | Z | C`. For the others `X = C`: `ccr = N | Z | V | (if c
{ CCR_C | CCR_X } else { 0 })`. The system byte (SR bits 8-15) is preserved by the shared write-back.

A bit-by-bit simulation (`steps = min(cnt, n+1)` for shifts; `((cnt-1) % period) + 1` for rotates) is an
equally-valid implementation and was used as the independent cross-check — pick whichever reads cleanest, but
TDD-pin the closed-form anchors either way.

## Opcode layouts (classify by opcode — files pure per op)

`0xExxx` is a dedicated, currently-unused opcode space (the dispatch's flat `if`-chain has no `0xE` arm). All
shift opcodes share `opcode >> 12 == 0xE`. Within it:

- **Register** (`1110 ccc d ss ir tt rrr`): `bits 7-6 = ss != 11`. `ir` = bit 5 (0 imm / 1 Dn). `tt` = bits
  4-3 (00 AS / 01 LS / 10 ROX / 11 RO). `d` = bit 8 (1 left / 0 right). `ccc` = bits 11-9, `rrr` = bits 2-0.
- **Memory** (`1110 0TTd 11 mmm rrr`): `bits 7-6 == 11`. `TT` = bits 10-9 (00 AS / 01 LS / 10 ROX / 11 RO),
  `d` = bit 8, `mmm rrr` = the EA.

Per-op opcode identity = `(type, direction)`: ASL = AS/left, ASR = AS/right, LSL = LS/left, LSR = LS/right,
ROXL = ROX/left, ROXR = ROX/right, ROL = RO/left, ROR = RO/right. The decode arm matches `0xE` + the op's
`(type, dir)` (register via bits 4-3 + bit 8; memory via bits 10-9 + bit 8 when bits 7-6 == 11) and routes to
the op's `AluOp`. (Only the op-files loaded in a given commit are ever decoded, so a not-yet-added op's
opcodes never reach decode — see *Commit plan*.)

## Recipe shapes (TDD-pinned to the data)

A single shared `shift_recipe(opcode, op: AluOp, regs: &Registers)` (modeled on `bit_recipe`):

- **Register** (`bits 7-6 != 11`): count `cnt` = decode-time — `ccc != 0 ? ccc : 8` (imm, bit 5 = 0) /
  `regs.d[ccc] & 63` (Dn, bit 5 = 1). The count **operand** `b` = `Operand::ShiftCount(cnt as u8)` (imm) /
  `Operand::DataRegFull(ccc)` (Dn; the exec masks `& 63`). Recipe:
  ```
  [ Prefetch,
    Alu { op, size, a: dn_src(rrr, size), b, dst: dn_dest(rrr, size) },
    Internal { cycles: (base - 4) + 2*cnt } ]      // base = 6 (.b/.w) / 8 (.l)
  ```
  where `dn_src` = `DataRegLow8`/`DataRegLow16`/`DataRegFull` and `dn_dest` = `DataRegLow8`/`DataRegLow16`/
  `DataReg` per size (the existing `dn_dest` helper; add a `dn_src` twin or inline). Observed stream =
  `[refill-read @4, idle @(2 or 4 + 2·cnt)]` — Prefetch first, then idle.
- **Memory** (`bits 7-6 == 11`, `.w` only, `cnt = 1`): the **word** `ea_dst` RMW (read word → shift1 → write
  word), byte-for-byte CLR.w/NEG.w's memory path:
  ```
  ea_dst(&mut buf, mode, reg, Size::Word, |operand| Alu {
      op, size: Size::Word, a: operand, b: Operand::ShiftCount(1), dst: Dest::Scratch(1) })
  ```
  Odd EA → a **read** address-error (SSW low5 = 0x15), the E3/E4 abort, exactly like NEG.w/CLR.w. No register
  `+2` (memory timing is fixed per EA mode).

The decode-time count is the load-bearing data dependency (the dynamic Dn-count is a live `Dn` — `decode(regs)`
reads it for the idle, exactly like `bit_recipe`'s `pos >= 16`). `MAX_OPS`/`SCRATCH_SLOTS` unchanged (heaviest
recipe ≈ memory `abs.l` RMW ≈ 8 ops; register ≈ 3 ops — measure, bump only if exceeded). `Internal.cycles` is
already `u16` (handles 2·63 = 126).

## Vocabulary additions (introduced in the commit that first USES it — keep `-D warnings` clean)

| Item | Signature / semantics | First used |
|---|---|---|
| `Operand::ShiftCount(u8)` | A decode-time immediate count (1-8 imm / 1 memory), resolved as the literal value; the exec masks `& 63`. Mirrors `Operand::Zero`/`WordStep` (constant operands). | S0 |
| `AluOp::Asl` | left arith shift: res `(x<<cnt)`, C = bit(n-cnt), X = C, **V = sign-changed**. | S0 |
| `AluOp::Asr` | right arith shift: sign-extending res, C = bit(cnt-1) (`cnt>n` → 0), X = C, V = 0. | S1 |
| `AluOp::Lsl` | left logical: res `(x<<cnt)`, C = bit(n-cnt), X = C, V = 0. | S2 |
| `AluOp::Lsr` | right logical: res `(x>>cnt)`, C = bit(cnt-1), X = C, V = 0. | S3 |
| `AluOp::Rol` | rotate left (no X): C = last bit out, **X preserved**, V = 0. | S4 |
| `AluOp::Ror` | rotate right (no X): C = last bit out, **X preserved**, V = 0. | S5 |
| `AluOp::Roxl` | rotate left through X: thread `{X:op}`, C = X = ejected, V = 0; `cnt=0` → C=X. | S6 |
| `AluOp::Roxr` | rotate right through X: thread `{X:op}`, C = X = ejected, V = 0; `cnt=0` → C=X. | S7 |

`shift_recipe` + `dn_src` (the operand-by-size helper) are introduced in S0 and reused verbatim S1-S7 (only the
`AluOp` + the decode `(type, dir)` arm differ). No new EA machinery — register is a 3-op recipe, memory reuses
the word `ea_dst`.

## Clean vs deferred — the `covered()` strategy

Add a `shift_covered(opcode)` predicate (dispatched from `covered()` for `opcode >> 12 == 0xE`) admitting the
in-scope EA set, plus the 2-case ASL.b corrupt exclusion (needs `ini` — `covered(opcode, ini)` already passes
it):

- **Register** (`bits 7-6 != 11`): every register shift is in scope — `Dn` operand, imm or Dn count, all
  counts. No memory access → no faults. (An-direct etc. do not arise — the operand is always `Dn`, bits 2-0.)
- **Memory** (`bits 7-6 == 11`, `.w` only): admit the data-alterable set `(An)`/`(An)+`/`-(An)`/`d16(An)`/
  `d8(An,Xn)` (2-6) + `abs.w`/`abs.l` (7/0, 7/1), **including `(A7)` mode-2** (a clean word RMW — NO deferral,
  like CLR.w/NEG.w; odd EAs are address errors the E3/E4 abort covers — NO parity filter).
- **Corrupt exclusion** (S0): `if opcode == 0xE502 && (d2 == 0xcdfb7fbe || d2 == 0x417c7e7d) { return false }`.

In-scope counts (exhaustively recomputed from the vendored JSON by opcode — NO carve-out beyond the 2 corrupt):
every file **8065** EXCEPT **`ASL.b` 8065 − 2 = 8063**. Per op (3 sizes): **ASL = 24193**, every other op =
**24195**. **Threshold: raise `ran >=` from 526705 to 720263** (+193,558) in cumulative steps; each commit
raises it to its measured true count (raised, never lowered).

## Commit plan (TDD, sequential, each fully green before the next)

Eight per-op commits — each op is one coherent `AluOp` + flag rule + `(type, dir)` decode arm + 3 files. S0
builds the shared `shift_recipe` + `Operand::ShiftCount` + `dn_src` + the word-RMW memory shift + the corrupt
exclusion, then ASL (the hardest — it owns V). S1-S7 reuse `shift_recipe` verbatim. Each commit: extend
`tools/fetch-tests.sh` `FILES` + add the 3 sha256 lines to `tools/singlesteptests.sha256` + add the 3 files to
the runner `FILES`; add the `AluOp` (+ S0 the `Operand`); decode arm + the exec arm; the `covered()` predicate;
both-drivers agreement + a snapshot/restore anchor for each new shape; full gate; conventional `feat(m68000):`
commit ending in the `Co-Authored-By` line.

- [ ] **S0 — ASL (`.b/.w/.l`, register imm + Dn + memory).** Build `shift_recipe`, `Operand::ShiftCount`,
  `dn_src`, the register `[Prefetch, Alu, Internal{idle}]` recipe (decode-time cnt), the **word `ea_dst`
  memory-shift** path, `AluOp::Asl` (with **V = sign-changed**), the `0xE` dispatch arm for AS/left, the
  `shift_covered` predicate + the **2-case corrupt exclusion** (assert exactly 2 removed, `ASL.b` runs 8063).
  Threshold → **550898** (+24193). Anchors: register imm `.b`/`.w`/`.l` (cnt 1-8, timing 6+2·cnt / 8+2·cnt);
  **register Dn-count with cnt=0** (zero-count: value kept, C=0, X kept, V=0, 6/8 cyc) AND a large Dn count
  (e.g. 40, timing 6+80); an **ASL.w that sets V** (sign changed) and one that does not; `.w` memory `(An)`
  shift-by-1 and `abs.l`; an `(A7)` mode-2 memory case; an odd-EA `.w` memory address-error; both-drivers +
  snapshot.
- [ ] **S1 — ASR (`.b/.w/.l`).** Add `AluOp::Asr` (sign-extending value; **C = bit(cnt-1), `cnt>n` → 0**; V=0;
  X=C). Decode arm AS/right. Reuse `shift_recipe`. Threshold → **575093** (+24195). Anchors: ASR with `cnt <=
  n` (C = bit(cnt-1)) AND **`cnt > n` (C = 0 — the quirk, esp. a negative operand where the naive sign rule
  would set C)**; Dn cnt=0; `.w` memory `(An)`/`-(An)`; an `(A7)` m2; both-drivers + snapshot.
- [ ] **S2 — LSL (`.b/.w/.l`).** Add `AluOp::Lsl` (= ASL value/C but **V = 0**). Decode arm LS/left. Threshold
  → **599288** (+24195). Anchors: LSL `cnt < n` and `cnt >= n` (C = 0, res = 0); Dn cnt=0; `.w` memory; an
  `(A7)` m2; both-drivers + snapshot.
- [ ] **S3 — LSR (`.b/.w/.l`).** Add `AluOp::Lsr` (zero-fill; C = bit(cnt-1); V=0; X=C; **N always 0** for
  cnt>=1). Decode arm LS/right. Threshold → **623483** (+24195). Anchors: LSR `cnt < n` / `cnt >= n`; Dn
  cnt=0; `.w` memory; an `(A7)` m2; both-drivers + snapshot.
- [ ] **S4 — ROL (`.b/.w/.l`).** Add `AluOp::Rol` (rotate; **X PRESERVED**; C = last bit out; V=0). Decode arm
  RO/left. Threshold → **647678** (+24195). Anchors: ROL `cnt % n != 0` and `cnt % n == 0` with `cnt != 0`
  (value unchanged, C from the formula); **Dn cnt=0 (C=0, X kept)**; verify X is untouched; `.w` memory; an
  `(A7)` m2; both-drivers + snapshot.
- [ ] **S5 — ROR (`.b/.w/.l`).** Add `AluOp::Ror` (rotate; **X PRESERVED**; C = last bit out; V=0). Decode arm
  RO/right. Threshold → **671873** (+24195). Anchors: ROR `cnt % n != 0` / `== 0`; Dn cnt=0; X untouched; `.w`
  memory; an `(A7)` m2; both-drivers + snapshot.
- [ ] **S6 — ROXL (`.b/.w/.l`).** Add `AluOp::Roxl` (thread X through an `(n+1)`-bit rotate; C = X = ejected;
  V=0). Decode arm ROX/left. Threshold → **696068** (+24195). Anchors: ROXL with X=0 and X=1 incoming
  (different results); **Dn cnt=0 (C = X, X unchanged)**; a `cnt` that wraps the `(n+1)` period; `.w` memory;
  an `(A7)` m2; both-drivers + snapshot.
- [ ] **S7 — ROXR (`.b/.w/.l`) — FINAL.** Add `AluOp::Roxr` (thread X right; C = X = ejected; V=0). Decode arm
  ROX/right. **Pin the final threshold 720263** (+24195). Anchors: ROXR X=0/X=1; Dn cnt=0 (C = X); period
  wrap; `.w` memory; an `(A7)` m2; both-drivers + snapshot.

## Vendoring (pinned commit `e0d5ece9670205cc84a0101081837deb446f86a3`)

Add to `tools/singlesteptests.sha256` (real sha256 of the pinned `.gz`, already fetched + verified locally),
3 lines per commit (`<op>.b`/`.w`/`.l`):

```
21d7221ca6179353a18ea59f19d17c7d59d1992f839aeafcf826e1dda98d4970  ASL.b.json.gz
9d4f7ab6f68003c93be8a589a80919d844bd452390225219cd6df8e851db3e31  ASL.w.json.gz
f9301adc58cfde24e0285a267de12ca161ebb6166c0384b6c625ee6bf862596d  ASL.l.json.gz
183d573c65392e3634ea50f5330ed98204d8c88dba33ade2fef250c8d95b3e46  ASR.b.json.gz
847dd822010a8c11f89bcff2acd89aacb920f8c5f268227ab8847408f7fa0f7b  ASR.w.json.gz
3b79584985a6b5b35de447d79c7caf1475bc579111dcbda62f09b340a2f8e422  ASR.l.json.gz
9bf1bee8f77baf3993a8704b57c010aa896fc3b7f29001eee0a0e4b15c8656af  LSL.b.json.gz
ccbc4bca24c4ab831cb7c0a1e0ffbc3f50d6e7259c91d5b763b808da3fe3ed66  LSL.w.json.gz
9b54a13533b773dc1a740f90ffb20aaa9200d0b0ac05db4f76de69de1edf9b7e  LSL.l.json.gz
b25d7ddad9ed202526997d47090b8cce25739a07a3687e870527cbafb7e16c87  LSR.b.json.gz
701a39b8d5cf8238f583359446f7ed853c3c4c7fa72720c2eea5f81caca5672d  LSR.w.json.gz
1917f7611ee89494c4546df5eacaee198fa474e41bfe57a75d4866f17456774d  LSR.l.json.gz
c2880bc43001f0cb0ee1c340a9e449f7749d3a6ba2584c4d72923f6525dafa2b  ROL.b.json.gz
41dac7384de365281846a7fd9d2fb92deddb1b1a1b1e1530de9a7c2d33ff1aa4  ROL.w.json.gz
87eea840832f0f1f9e1a30e37bf93aa8cb46dd9942b65ef244c92c6e5e94f144  ROL.l.json.gz
4a20a9efe3ad9dcb41a7fb4462b888baa7332360f058d60f9293713bae6b8f4c  ROR.b.json.gz
a9ca4ff30335ca913eba474e4660fa929858353b195792e181cd6723afb12d9b  ROR.w.json.gz
ea5d0b70a6b0fb757cc9b8069167c37e12e0a7a2ac39a74834268eb63d84aa1c  ROR.l.json.gz
8056117e27161ff6a3a9c896d03571dac2244457b80fc0b690ed3c0516169d04  ROXL.b.json.gz
f5534472b194f08051b0dfd458fa1a13ead0e23b767ce2069898b247d8feab31  ROXL.w.json.gz
edec7f9ce774d4ed77547cf126c2c0be01dfc6ef262d018deca326238cff3b82  ROXL.l.json.gz
6471d65223cf0dfd51a74b6ca062be0f02ace4722195d81e4b5575a1a7e9b32f  ROXR.b.json.gz
51a48dea07e8c0c3a2170c3c2198ace23d769e4e7307115ca38127e78c57485b  ROXR.w.json.gz
9c7c22c445278ee35038bb94114a0ab5666bcf2be283bc37044dd982cf35021a  ROXR.l.json.gz
```

## Verification protocol (every commit) & anti-cheating rules

Not done until, from the repo root: `cargo test -p oracle-core --test determinism_gate --test proptests`
green (most-guarded), `cargo fmt --all -- --check` clean, `cargo clippy --all-targets -- -D warnings` clean,
`cargo test --workspace` green; committed with a conventional `feat(m68000):` message ending in
`Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`. Read `.github/workflows/ci.yml`; run what CI runs.
The SST integration test sweeps ALL covered families (now ~600 s with shifts — use a **600000 ms** timeout; do
not assume it hung).

**Anti-cheating (hard rules — a verifier checks these):**
- **SST is ground truth.** Fix the code to match the suite. NEVER weaken an assertion, broaden `covered()` to
  skip cases that should pass, lower the `ran >=` threshold below the true covered count, `#[ignore]`/`#[cfg]`
  -out a test, or comment out an assert. The ONLY documented exclusion is the **2 corrupt ASL.b entries**
  (proven self-contradictory — a correct CPU cannot match them; the exclusion must remove *exactly 2* and is
  keyed on the exact opcode+d2). Everything else is 100% in scope (incl `(A7)` mode-2 and odd-EA address
  errors).
- **Classify by OPCODE** (`0xExxx`; register vs memory by bits 7-6; type by bits 4-3 (reg) / 10-9 (mem);
  direction by bit 8).
- **The ASR carry quirk** (`C = 0` for `cnt > n`) and **ASL's V** (sign-changed) are load-bearing — pin both a
  `cnt <= n` and a `cnt > n` ASR anchor, and a V-set and V-clear ASL anchor.
- **ROL/ROR must NOT touch X**; **ROXL/ROXR thread X** (and `cnt == 0` → C = X). Pin an X=0 vs X=1 ROX anchor.
- **The register count is decode-time** — imm from the opcode (`ccc`/8), Dn from the **live `Dn` read at
  decode** (the idle = 2·cnt). Do NOT hardcode; read `regs`. Pin a `cnt = 0` and a large-count anchor.
- Both-drivers-agree and snapshot/restore-at-every-bus-boundary stay intact for every new form; the
  determinism gate stays the most-guarded job. No `Rc`/`RefCell`/`unsafe`/`HashMap`/floats in hashed state;
  `MicroState` stays fixed-size bincode. Never touch `../oracle/`.

## Risks

- **ASL's V** — the subtlest flag. Use the verified closed form (`cnt >= n` → `V = (x != 0)`, NOT
  `x != 0 && x != mask`) or the bit-sim; pin a V-set and a V-clear `.w` anchor.
- **The ASR `cnt > n` carry = 0** — a naive "last bit out = sign for over-shift" mismatches 1642 ASR.b cases.
  Pin a negative-operand `cnt > n` ASR anchor (C must be 0, not 1).
- **ROXL/ROXR X-threading** — the `(n+1)`-bit rotate; `cnt == 0` → C = X (not 0). Guard the Rust shift
  (`per - eff` can equal `per` when `eff == 0`; branch). Pin X=0 vs X=1.
- **Decode-time Dn-count** — the idle (2·cnt) reads the live `Dn`; ccc == rrr (count reg == operand reg) is
  legal (both resolved before the write-back). Pin a `cnt = 0` (zero-count special case) and a `cnt = 63` (126
  idle cyc) anchor.
- **Rust shift UB** — `u32 << 32` / `>> 32` panics in debug; branch on `cnt >= n` / `r == 0` / `eff == 0`.
- **The 2 corrupt ASL.b entries** — exclude EXACTLY 2 (assert the count); do not broaden the key. `ASL.b` is
  the only file at 8063.
- **Memory shift is `.w` only** (the `.b`/`.l` files have no memory form) and odd-EA = a **read**
  address-error (low5 = 0x15) via E3/E4 — reuse NEG.w/CLR.w's word `ea_dst` path verbatim.
