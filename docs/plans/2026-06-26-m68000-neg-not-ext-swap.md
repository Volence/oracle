# oracle-next — 68000 NEG / NEGX / NOT + EXT + SWAP (single-operand ops)

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:test-driven-development` — each behaviour gets a
> *failing test first*. Commits are sequential and dependent (G0→G3); each leaves the tree fully green before
> the next. **GROUND TRUTH IS THE VENDORED SingleStepTests STREAM.** The flag formulas, value formulas and
> in-scope counts below were derived and **0-mismatch verified against the real vendored data** (exhaustive
> check over every mode-0 case — see *Recon findings*); every exact cycle placement and bus-transaction ORDER
> is **TDD-discovered against the data — the SST stream wins, never weaken the assertion.** See *Anti-cheating
> rules*. Never modify `../oracle/`.

## Goal & scope

Add the single-operand integer family group on the proven micro-op framework
(`m68000::{microop,ea,decode}`):

- **NEG.{b,w,l}** — `dst = 0 − dst`. Full flags (it is literally a `0 − dst` subtraction). Data-alterable
  destination (`Dn` or alterable memory); RMW for memory.
- **NEGX.{b,w,l}** — `dst = 0 − dst − X`. SUBX-style flags: **Z is STICKY** (only ever cleared), `X = C =`
  borrow, `V` = subtract-from-zero overflow. The one genuinely-subtle op.
- **NOT.{b,w,l}** — `dst = ~dst`. Logic flags (`N=msb`, `Z=zero`, `V=0`, `C=0`, **X preserved**) — the exact
  `And`/`Or`/`Eor` shape.
- **EXT.w / EXT.l** — sign-extend `Dn.b→Dn.w` (`.w`) / `Dn.w→Dn.l` (`.l`). `Dn`-only. Logic flags. 4 cyc.
- **SWAP** — swap the two 16-bit halves of `Dn`. `Dn`-only. Logic flags on the full 32-bit result. 4 cyc.

This is **overwhelmingly reuse**. The structurally-new pieces are small: three unary ALU ops
(`Neg`/`Negx`/`Not`), two `Dn`-only transform ops (`Ext`/`Swap`), and the observation that the
memory-destination NEG/NEGX/NOT recipe is **`clr_recipe`'s read-then-write RMW with the read operand USED**
(the `ea_dst` closure receives the just-read value; CLR discards it, we transform it). Odd word/long EAs are
already coverable via the E3/E4 execution-time address-error abort, so they are **in scope**, not deferred.

**KEY DATA FACT — every one of the 12 files is 100% PURE** (one opcode each, 8065 cases each; verified:
`op & 0xFFC0` is a single value per NEG/NEGX/NOT file, `op & 0xFFF8` a single value per EXT/SWAP file). There
is **NO `*I`-style contamination** this push (unlike AND/OR/EOR). Classify by opcode as always, but there is
no immediate-form intruder to carve out.

**Deferred (not this push):** the plain `(A7)` mode-2 indirect form (the pre-existing residual carve-out
shared with ADD/SUB/MOVE/CMP/CLR/AND/OR/EOR — its `(A7)+`/`-(A7)` siblings ARE in scope); Scc/TAS; bit ops;
shifts/rotates; MUL/DIV; MOVEM/LEA/PEA/LINK/etc; ABCD/SBCD/NBCD; ADDX/SUBX; the remaining exceptions; the
integration pivot.

## Recon findings (exhaustive, 0-mismatch verification over all mode-0 cases)

All four formulas reproduce the vendored `final` registers + `final.sr` **bit-exact with 0 mismatches** across
every mode-0 (`Dn`) case (NEG ~1300/size, NEGX ~1285/size, NOT ~1325/size, EXT/SWAP all 8065). The flag
formula is mode-independent, so mode-0 verification fixes it for all modes; the EA/bus machinery for the
memory modes is the already-proven `ea_dst`/`ea_dst_long` path + the address-error abort.

- **NEG** `res = (0 − d) & mask`: `N = msb(res)`, `Z = (res == 0)`, **`V = (d & res & signbit) != 0`** (set
  only when `d == sign-min`), **`C = X = (d != 0)`** (borrow). This is byte-identical to `AluOp::Sub` with
  `a = 0, b = d` (NEG is `0 − d`) — verified.
- **NEGX** `res = (0 − d − X_in) & mask`: `N = msb(res)`, **`Z = Z_in AND (res == 0)`** (STICKY — cleared when
  `res != 0`, otherwise unchanged; the multi-precision idiom), `V = (d & res & signbit) != 0`,
  **`C = X = NOT(d == 0 AND X_in == 0)`** (borrow of `0 − d − X_in`). Depends on the *incoming* X and Z.
- **NOT** `res = (~d) & mask`: `N = msb(res)`, `Z = (res == 0)`, `V = 0`, `C = 0`, **X preserved**. Identical
  to the `And`/`Or`/`Eor` logic shape — `move_flags(res, size)` with X re-injected `(ccr_nz | (sr & CCR_X))`.
- **EXT.w** `Dn.w = sign_extend8→16(Dn.b)` (high word of Dn preserved): `N = bit15`, `Z = (word == 0)`,
  `V = 0`, `C = 0`, X preserved. **EXT.l** `Dn.l = sign_extend16→32(Dn.w)` (full 32): `N = bit31`,
  `Z = (long == 0)`, `V = 0`, `C = 0`, X preserved.
- **SWAP** `Dn = (Dn >> 16) | (Dn << 16)` (full 32): `N = bit31`, `Z = (res == 0)`, `V = 0`, `C = 0`, X
  preserved.
- **Modes**: NEG/NEGX/NOT destinations are **data-alterable** — `Dn` (mode 0) + alterable memory (modes 2..6,
  `abs.w` 7/0, `abs.l` 7/1). **No An (mode 1), no PC-relative (7/2,7/3), no immediate (7/4)** (a destination
  must be alterable — none present in the data). EXT/SWAP are **`Dn`-only (mode 0)**.
- **Cycle timing** (clean, non-fault): register `.b/.w = 4` cyc, register `.l = 6` cyc (trailing `n2`); memory
  RMW = the standard single-operand read-then-write (same as CLR): `(An)`/`(An)+` `= 12`(.b/.w)/`20`(.l),
  `−(An)` `= 14/22`, `d16(An)` `= 16/24`, `d8(An,Xn)` `= 18/26`, `abs.w` `= 16/24`, `abs.l` `= 20/28`.
  EXT.w/EXT.l/SWAP `= 4` cyc (one prefetch). Odd word/long EAs → the group-0 14-byte address-error frame
  (~50 cyc) — **in scope**, must PASS via the abort (no parity filter). Byte ops never fault.
- **Plain `(A7)` mode-2 indirect**: present (~150–182 cases per word/long-capable NEG/NEGX/NOT file) —
  DEFERRED (precedent-consistent residual). `(A7)+`/`-(A7)` siblings stay in scope. EXT/SWAP have no memory →
  no `(A7)` cases.

## Flag & value formulas (verified, 0 mismatches)

Implement NEG/NEGX/NOT as **unary** `AluOp`s — the exec arm resolves a single source operand `a` (the `Dn`
value for mode 0, or the scratch holding the read EA value for memory), ignores `b`, and writes the
size-masked result to `dst` (`dn_dest(reg,size)` for register, `Scratch(1)` for memory, exactly as CLR):

- **`AluOp::Neg`**: `let d = a & mask; res = (0u32.wrapping_sub(d)) & mask`. Flags: reuse the existing
  subtract flag path (NEG ≡ `sub_{b,w,l}(0, d)`) — N/Z/V/C and `X = C`. (If cleanest, the exec arm may simply
  delegate to the same `sub_*` helper the `AluOp::Sub` arm uses with `lhs = 0`.)
- **`AluOp::Negx`**: `let xin = (regs.sr >> 4) & 1; let zin = (regs.sr >> 2) & 1; res = (0 − d − xin) & mask`.
  `N = msb`, **`Z = if res == 0 { zin } else { 0 }`** (sticky), `V = (d & res & signbit) != 0`,
  `C = X = if d == 0 && xin == 0 { 0 } else { 1 }`. Pin against cases with `X_in = 1` AND `Z_in = 1`.
- **`AluOp::Not`**: `res = (!d) & mask`; `move_flags(res, size)` for N/Z + clear V/C, then re-inject X:
  `(ccr_nz | (regs.sr & CCR_X))`. (Structurally identical to the `Eor` arm.)
- **`AluOp::Ext`** (`Dn`-only): `size == Word` → `res = sign_extend16(sign_extend8(a) )` low word, write
  `DataRegLow16`; `size == Long` → `res = sign_extend16(a)` full 32, write `DataReg`. Flags = `move_flags(res
  at result size)` + X preserved. (Reuse `sign_extend16`; add an 8→32 sign-extend inline for the byte input.)
- **`AluOp::Swap`** (`Dn`-only, size ignored / always 32): `res = (a >> 16) | (a << 16)`; flags = N=bit31,
  Z=(res==0), V=C=0, X preserved; write `DataReg`.

## Opcode layouts (classify by opcode — all files pure)

- **NEG** `0100 0100 ss mmm rrr` (`0x4400` .b / `0x4440` .w / `0x4480` .l), mask `opcode & 0xFFC0`.
- **NEGX** `0100 0000 ss mmm rrr` (`0x4000` / `0x4040` / `0x4080`), mask `0xFFC0`.
- **NOT** `0100 0110 ss mmm rrr` (`0x4600` / `0x4640` / `0x4680`), mask `0xFFC0`.
  All three: destination = data-alterable (mode 0 `Dn`, or `is_dst_mem_mode` memory). Defer plain `(A7)` m2.
- **EXT.w** `0100 1000 10 000 rrr` (`0x4880`), **EXT.l** `0100 1000 11 000 rrr` (`0x48C0`),
  **SWAP** `0100 1000 01 000 rrr` (`0x4840`): mask `opcode & 0xFFF8` (mode is fixed `000` = `Dn`; the low 3
  bits are the register). The `0xFFF8` mask isolates the `Dn` encodings from the neighbours in `0x48xx`
  (`PEA`/`MOVEM`, which have mode ≥ 2 → different bits 5-3) — those stay undecoded (a future push).

## Recipe shapes (TDD-pinned to the data)

- **NEG/NEGX/NOT `<ea>`:** a shared `neg_family_recipe(opcode, AluOp, size)` mirroring `clr_recipe`:
  - mode 0 (`Dn`): `[Prefetch, Alu{op, size, a: <Dn source for size>, b: Zero, dst: dn_dest(reg,size)},
    (+ Internal{2} when size == Long)]` → `.b/.w` = 4 cyc, `.l` = 6 cyc.
  - memory (`is_dst_mem_mode`): `ea_dst(&mut buf, mode, reg, size, |operand| Alu{op, size, a: operand,
    b: Zero, dst: Scratch(1)})` — the read EA value is the `operand` the closure receives (CLR discards it;
    here it is the unary source). `.l` routes through `ea_dst_long` (reversed long store) automatically.
- **EXT.w / EXT.l:** `[Prefetch, Alu{op: Ext, size, a: <Dn>, b: Zero, dst: DataRegLow16(reg) | DataReg(reg)}]`
  — 4 cyc, no idle. The `dst` and result width follow `size` (Word writes the low word, Long the full 32).
- **SWAP:** `[Prefetch, Alu{op: Swap, size: Long, a: DataRegFull(reg), b: Zero, dst: DataReg(reg)}]` — 4 cyc.

The `Dn` source operand for a unary op follows size: `DataRegLow8`/`DataRegLow16`/`DataRegFull` (`.b/.w/.l`).
`MAX_OPS`/`SCRATCH_SLOTS` unchanged (the heaviest recipe, NEG.l `abs.l` RMW, is ~9 ops, well under 20/10 —
measure; bump only if exceeded).

## Vocabulary additions (introduced in the commit that first USES it — keep `-D warnings` clean)

| Item | Signature / semantics | First used |
|---|---|---|
| `AluOp::Neg` | unary `res = (0 − a) & mask`; subtract flags N/Z/V/C, `X = C`; size-masked write-back. | G0 |
| `AluOp::Negx` | unary `res = (0 − a − X_in) & mask`; `Z` STICKY (`Z_in & (res==0)`), V=`(a&res&sb)`, `C=X`=borrow. | G1 |
| `AluOp::Not` | unary `res = (~a) & mask`; `move_flags` N/Z + clear V/C, **X preserved**. | G2 |
| `AluOp::Ext` | `Dn`-only sign-extend (byte→word for `.w`, word→long for `.l`); logic flags, X preserved. | G3 |
| `AluOp::Swap` | `Dn`-only 16-bit halfword swap (full 32); logic flags on bit31/zero, X preserved. | G3 |

The unary ops resolve operand `a` and ignore `b` (pass `Operand::Zero` as `b`). NEG may delegate to the
existing `sub_*` helpers (`lhs = 0`); NOT to the `move_flags` + X-reinject shape. No new EA machinery — the
memory path is `ea_dst`/`ea_dst_long` verbatim. `dn_dest` already gives the size-masked register write-back.

## Clean vs deferred — the `covered()` strategy

Add genuine-opcode predicates (`neg_family_in_scope` for the `0xFFC0` NEG/NEGX/NOT triple, `ext_swap_in_scope`
for the `0xFFF8` `Dn`-only ops) admitting the genuine opcode and deferring only the plain `(A7)` mode-2.
In-scope counts (exhaustively recomputed from the vendored JSON — defer = `(mode==2 && reg==7)`):

| File | in-scope | defer (A7)m2 | File | in-scope | defer | File | in-scope | defer |
|---|---|---|---|---|---|---|---|---|
| NEG.b | 7915 | 150 | NEGX.b | 7917 | 148 | NOT.b | 7901 | 164 |
| NEG.w | 7893 | 172 | NEGX.w | 7893 | 172 | NOT.w | 7894 | 171 |
| NEG.l | 7917 | 148 | NEGX.l | 7883 | 182 | NOT.l | 7899 | 166 |
| EXT.w | 8065 | 0 | EXT.l | 8065 | 0 | SWAP | 8065 | 0 |

- Odd word/long EAs are **in scope** (the E3/E4 abort installs the group-0 14-byte frame — they PASS).
- The only intra-family deferral is the plain `(A7)` mode-2 indirect (`mode==2 && reg==7`), consistent with
  every prior memory-dest family. `(A7)+`/`-(A7)` are in scope. EXT/SWAP have no deferrals.
- **Threshold:** raise `ran >=` from **383008** in cumulative steps to **478315** (current + 95307). Each
  commit raises it to its measured true count (raised, never lowered) — measure & pin at each step.

## Commit plan (TDD, sequential, each fully green before the next)

Each commit: extend `tools/fetch-tests.sh` `FILES` + add the real sha256 to `tools/singlesteptests.sha256` +
extend the runner `FILES`; add vocabulary in the commit that uses it; decode arm(s) + recipe; the
`covered()`/genuine-opcode predicate; both-drivers agreement + a snapshot/restore anchor for the new shape;
full gate; conventional `feat(m68000):` commit ending in the `Co-Authored-By` line.

- [ ] **G0 — NEG.b / NEG.w / NEG.l.** Add `AluOp::Neg` (unary `0 − a`, subtract flags, `X=C`). Decode
  `0x4400`/`0x4440`/`0x4480` (data-alterable dest: mode 0 + `is_dst_mem_mode`). `neg_family_recipe` (mode-0
  register arm `.l`+n2; memory via `ea_dst` with the read operand as the unary source). Vendor NEG.b/.w/.l.
  `covered()` admits the genuine NEG opcode (defer `(A7)` m2; odd EAs in scope). Threshold → **406733**
  (+23725). Anchors: `Dn` `.b/.w/.l` (incl. a `d == sign-min` overflow `V=1` case and a `d == 0` →
  `Z=1,C=0` case), `(An)` memory `.w`/`.l` RMW, an odd-EA case (address-error frame), both-drivers +
  snapshot.
- [ ] **G1 — NEGX.b / NEGX.w / NEGX.l.** Add `AluOp::Negx` (unary `0 − a − X`, **sticky Z**, `X=C`=borrow).
  Decode `0x4000`/`0x4040`/`0x4080`. Reuse `neg_family_recipe`. Vendor NEGX.b/.w/.l. Threshold → **430426**
  (+23693). Anchors: a `Dn` case with `X_in=1 && Z_in=1 && res!=0` (sticky Z stays from `Z_in`? — pin it: Z
  becomes 0 since res!=0) AND a `res==0` case (Z = `Z_in`), a borrow `C=X` case, `(An)` memory `.l` RMW, an
  odd-EA case, both-drivers + snapshot.
- [ ] **G2 — NOT.b / NOT.w / NOT.l.** Add `AluOp::Not` (unary `~a`, logic flags via `move_flags` + preserve
  X). Decode `0x4600`/`0x4640`/`0x4680`. Reuse `neg_family_recipe`. Vendor NOT.b/.w/.l. Threshold →
  **454120** (+23694). Anchors: `Dn` each size (X-preservation pin: enter with `X=1`, confirm X kept, V/C
  cleared), `(An)` memory `.w` RMW, an odd-EA case, both-drivers + snapshot.
- [ ] **G3 — EXT.w / EXT.l + SWAP.** Add `AluOp::Ext` + `AluOp::Swap` (`Dn`-only, logic flags). Decode
  `0x4880`/`0x48C0` (EXT, mask `0xFFF8`) and `0x4840` (SWAP, mask `0xFFF8`). `ext_recipe`/`swap_recipe`
  (`[Prefetch, Alu{…}]`, 4 cyc). Vendor EXT.w/EXT.l/SWAP; `covered()` = `ext_swap_in_scope`. **Pin the final
  threshold 478315** (+24195). Anchors: EXT.w (byte high-bit set → sign-extends to `0xFFxx`, high word of Dn
  preserved), EXT.l (word high-bit set), SWAP (distinct halves, N from bit31), each with the X-preserve pin;
  both-drivers + snapshot.

## Vendoring (pinned commit `e0d5ece9670205cc84a0101081837deb446f86a3`)

Add to `tools/singlesteptests.sha256` (real sha256 of the pinned `.gz`, already fetched + verified locally):

```
a7ea4a898adbca4d3e30e685b981ae8bb372151989759c0daf757a23be6f3f56  NEG.b.json.gz
b9d639cd66c2b3c4ccbbe6f15dbe10abbe9a37c9077490d8f2dd2d48f5d1d938  NEG.w.json.gz
134bcdc2b9c64507aa9a4eadef2bce05d301e0786d674dcbf1d258a364adc208  NEG.l.json.gz
222d166b6e00183e34452b7c2a1c11a92e42ba28049a05037c24defe21e94e0c  NEGX.b.json.gz
24a70df2ea1aca88dae4e41e607d45d990fb0c0b3387937c4ef5f6f803090c9e  NEGX.w.json.gz
9d7241f64ea8dea6aa6ae0d504b5c8ec1cdee62cac800c39ce93a6dac196eabf  NEGX.l.json.gz
86e8aa00e45bdf808ece613c86909711c5b4e74b8a7b1682977e608139c13f05  NOT.b.json.gz
52ec03e4a8a638b147e12fc76b9f23f28fa5db988f9a5ae89b45f0b80e9dcbeb  NOT.w.json.gz
7d3fc66804af15d7a684df12e7d402095d8b5f6b17ffc2e0ff99333c524fba48  NOT.l.json.gz
0d356458aae39fd86e99050caf74b0281b86610627ab7089087b00e25a739890  EXT.w.json.gz
28f0c3159982a48ec0ad5c49e986e94c6e77c7c153cae908861c7ab5f62df054  EXT.l.json.gz
12adf389965d7f63992cc6e6991f7455297b23afe38dffd475b4829dbac32bf9  SWAP.json.gz
```

## Verification protocol (every commit) & anti-cheating rules

Not done until, from the repo root: `cargo test -p oracle-core --test determinism_gate --test proptests`
green (most-guarded), `cargo fmt --all -- --check` clean, `cargo clippy --all-targets -- -D warnings` clean,
`cargo test --workspace` green; committed with a conventional `feat(m68000):` message ending in
`Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`. Read `.github/workflows/ci.yml`; run what CI runs.
The SST integration test sweeps ALL covered families (~350+ s) — use a generous timeout; do not assume it hung.

**Anti-cheating (hard rules — a verifier checks these):**
- **SST is ground truth.** Fix the recipe/code to match the suite. NEVER weaken an assertion, broaden
  `covered()` to skip cases that should pass, lower the `ran >=` threshold below the true covered count,
  `#[ignore]`/`#[cfg]`-out a test, or comment out an assert. `covered()` may only skip the documented
  deferral (the plain `(A7)` mode-2 form). There is NO contaminant opcode to skip this push (all files pure).
- **Classify by OPCODE.** All 12 files are pure single-opcode, so the predicate is the opcode mask + the
  `(A7)` m2 deferral + (for NEG/NEGX/NOT) the data-alterable-dest admission (mode 0 or `is_dst_mem_mode`).
- **NEGX is the load-bearing subtlety** — sticky Z and X-borrow-in. Pin against `X_in`/`Z_in`-varying cases;
  do NOT compute Z as a plain `res==0` (that breaks the multi-precision contract on `res==0 && Z_in==0`).
- Both-drivers-agree and snapshot/restore-at-every-bus-boundary stay intact for every new form; the
  determinism gate stays the most-guarded job.
- No `Rc`/`RefCell`/`unsafe`/`HashMap`/floats in hashed state; `MicroState` stays fixed-size bincode. Never
  touch `../oracle/`.

## Risks

- **NEGX sticky Z + X-in** — the only genuinely-new flag semantics. `Z_final = Z_in AND (res==0)` (NOT
  `res==0`), `X_in` participates in the value AND the borrow. TDD-pin against cases with `X_in=1`,
  `Z_in=1`, and a `res==0` case. (0-mismatch verified in recon, but it is the easiest thing to get subtly
  wrong.)
- **NEG/NEGX/NOT memory is the CLR read-then-write RMW** (read EA, THEN write the transformed value) — reuse
  `ea_dst`/`ea_dst_long` exactly; the read operand is the closure arg. An odd EA faults on the READ (the E3
  abort), like CLR. Do NOT invent a new write-only path.
- **EXT/SWAP mask is `0xFFF8` (mode-0 only)** — the `0x48xx` space also holds PEA/MOVEM (mode ≥ 2). The
  `0xFFF8` mask matches only the `Dn` encodings; do NOT use `0xFFC0` (that would swallow PEA/MOVEM).
- **EXT result width follows size** — EXT.w writes the low WORD (high word of Dn preserved → `DataRegLow16`),
  EXT.l writes the FULL 32 (`DataReg`). Flags are on the result width (bit15 vs bit31).
- **NEG ≡ Sub(0, d)** is a verified equivalence — fine to delegate to `sub_*`, but the operand order is
  `lhs = 0, rhs = d` (so the borrow/overflow come out right); pin a `d == sign-min` `V=1` case.
