# oracle-next — 68000 ADDA/SUBA (address arithmetic) + AND/OR/EOR (logic)

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:test-driven-development` — each behaviour gets a
> *failing test first*. Commits are sequential and dependent (L0→L4); each leaves the tree fully green before
> the next. **GROUND TRUTH IS THE VENDORED SingleStepTests STREAM.** The recipe shapes, flag formulas, cycle
> counts and orderings below were reconnoitred against the real vendored data (an adversarial Python recon
> over all 13 files, 0-mismatch statistical verification of every load-bearing claim — see *Recon findings*),
> but every exact cycle placement and bus-transaction ORDER is **TDD-discovered against the data — the SST
> stream wins, never weaken the assertion.** See *Anti-cheating rules*. Never modify `../oracle/`.

## Goal & scope

Add two more of the most common integer families on the proven micro-op framework
(`m68000::{microop,ea,decode}`):

- **ADDA / SUBA** — `An = An ± src`, **NO flags**, `.w` sign-extends the source word to long. Address
  arithmetic, structurally **MOVEA ±** (reuse the MOVEA/CMPA source machinery + a new no-flag An-write ALU).
- **AND / OR / EOR** — bitwise logic, `.b/.w/.l`. AND/OR in **both directions** (`<ea>,Dn` and `Dn,<ea>`);
  EOR in `Dn,<ea>` **only** (there is no `EOR <ea>,Dn` — that opcode space is CMP). Sets N=msb / Z=(result==0),
  clears V/C, **PRESERVES X**, never computes X.

This is **overwhelmingly reuse**. The structurally-new pieces are tiny: two no-flag An-write ALU ops
(`Adda`/`Suba`, mirroring `MoveA`), three logic ALU ops sharing one flag shape (`And`/`Or`/`Eor`), one
trailing-idle tweak for ADDA.w/SUBA.w, and one register-dest arm for `EOR Dn,Dn`. Odd word/long EAs are
already coverable via the E3/E4 execution-time address-error abort, so they are **in scope**, not deferred.

**KEY DATA FACT — the AND/OR/EOR files are CONTAMINATED with ANDI/ORI/EORI.** Each logic file mixes the
genuine register-form opcode (`AND` 0xC, `OR` 0x8, `EOR` 0xB nibble) with the **dedicated immediate-form
`*I` opcode** in the group-0 space (`ANDI` 0x02xx, `ORI` 0x00xx, `EORI` 0x0Axx) — 5675 cases total. `*I` is a
**separate instruction this push does not implement** (parallel to how the `CMP.*` files mixed CMP/CMPM/CMPI).
**Classify by OPCODE**, admit only the genuine register form, and these `*I` cases are skipped cleanly
(never decoded). They are a future dedicated `ANDI/ORI/EORI` push, NOT a mode-scope deferral. (`ADDA.*`/
`SUBA.*` files are 100% pure — 0 contaminants.)

**Deferred (not this push):** the `*I` immediate family (ANDI/ORI/EORI, 5675 vendored cases skipped here);
the plain `(A7)` mode-2 indirect form (the pre-existing residual carve-out shared with ADD/SUB/MOVE/CMP — its
`(A7)+`/`-(A7)` siblings ARE in scope); NEG/NEGX/NOT/EXT/SWAP/Scc/TAS; bit ops; shifts/rotates; MUL/DIV;
MOVEM/LEA/PEA/LINK/etc. The integration pivot.

## Recon findings (adversarial Python recon, 0-mismatch verification)

- **AND/OR/EOR flag formula is IDENTICAL across all three families and all sizes** (verified bit-exact on
  every clean case, 0 mismatches): `N = msb(result at size)`, `Z = (result == 0 at size)`, `V = 0`, `C = 0`,
  `X = preserved`. One shared flag shape; the three differ ONLY in the bit operation (`&` / `|` / `^`).
- **ADDA/SUBA touch NO flags** (`final.sr == initial.sr`, 0 mismatches across all 4 files / 32260 cases).
  `.w` sign-extends src word→long then adds at the long boundary into the full An; `.l` adds full 32. An is
  written full-32. Files are 100% pure ADDA/SUBA (opmode 3/7 only).
- **Cycle timing (per source mode, address-error cases excluded):**
  - **ADDA.w / SUBA.w** = the **MOVEA.w** source-fetch bus stream **+ a uniform trailing `n4` idle** for
    *every* source mode (verified: ADDA.w[mode] = MOVEA.w[mode] + 4, all 12 modes).
  - **ADDA.l / SUBA.l** = **byte-for-byte identical to `ADD.l <ea>,Dn`** (lengths + idle placement): trailing
    `n4` for register-direct (Dn/An) and `#imm`, trailing `n2` for every memory mode. So they reuse
    `ea_src_long` (the ADD.l reader) *verbatim* — its n4/n2 trailing idle is already built in.
  - **AND/OR `<ea>,Dn`** = `ADD <ea>,Dn` byte-for-byte (word & long), MINUS the illegal An-direct (mode 1)
    source (absent from the data). **AND/OR `Dn,<ea>`** (alterable-memory dest) = `ADD Dn,<ea>` byte-for-byte.
  - **EOR `Dn,<ea>`** (memory dest) = `ADD Dn,<ea>` byte-for-byte. **`EOR Dn,Dn`** (mode 0, register dest):
    `.b`/`.w` = 4 cyc (no idle), **`.l` = 8 cyc with a trailing `n4`** (the register-register long idle).
- **No ABCD/EXG/SBCD contamination** in AND/OR (opmode 4/5/6 mode-000/001 count = 0). **No CMPM contamination**
  in EOR (opmode 4/5/6 mode-001 count = 0). The decoder must still *reserve* those corners (mode 000/001 of
  0xC/0x8 opmode-4/5/6 = ABCD/EXG/SBCD; 0xB opmode-4/5/6 mode-001 = CMPM, handled by the existing `cmp_class`).
- **AND/OR `<ea>,Dn` DO contain `#imm` (7/4) and PC-relative (7/2, 7/3) sources** — `ea_src` already handles
  all of these; nothing new needed. (EOR has no `#imm`/PC source — it is `Dn,<ea>` only.)
- **Odd word/long EAs**: 29042 cases across the 10 word/long files, all clean group-0 14-byte address-error
  frames (two independent signatures agree 100%). **In scope — must PASS** via the E3/E4 abort (no parity
  filter). Byte ops never fault.
- **Plain `(A7)` mode-2 indirect**: 1877 cases across the 13 files — DEFERRED (precedent-consistent residual,
  not an odd-address case). The `(A7)+`/`-(A7)` siblings stay in scope.

## Flag & value formulas (verified statistically, 0 mismatches)

- **AND / OR / EOR** at size `s`, `result = a OP b` (OP = `&`/`|`/`^`): `N = msb(result)`, `Z = (result == 0)`,
  `V = 0`, `C = 0`, **X PRESERVED**. Implemented as `move_flags(a OP b, s)` (which masks to size + computes
  N/Z and clears V/C) with the live X re-injected: `(ccr_nz | (regs.sr & CCR_X))`. The size-masked result is
  written back (low8/low16/full32, or parked in scratch for a memory dest).
- **ADDA / SUBA** at size `s` (w/l): `An = An ± b`, where `b = sign_extend16→32(src word)` for `.w` (mirroring
  `MoveA`) or the full long for `.l`; computed at the **long boundary**; **NO flags**; An written full-32.
  (`AluOp::Adda`/`Suba` resolve `a = An` and `b = src`, sign-extend `b` when `size == Word`, and write An via
  `Dest::AddrReg` — exactly the no-flag early-return shape of `AluOp::MoveA`, but `a ± b` instead of a copy.)

## Opcode layouts (classify by opcode — admit only the genuine register form)

- **ADDA** `1101 aaa s11 mmm rrr` (opmode 3 = `.w` → `0xD0C0`; opmode 7 = `.l` → `0xD1C0`): `An = An + src`,
  all 12 source modes (An-direct LEGAL — it is address arithmetic). Masks: `opcode & 0xF1C0 == 0xD0C0` /
  `== 0xD1C0`.
- **SUBA** `1001 aaa s11 mmm rrr` (`0x90C0` `.w` / `0x91C0` `.l`): `An = An − src`. Masks `== 0x90C0` /
  `== 0x91C0`.
- **AND `<ea>,Dn`** `1100 ddd 0SS mmm rrr` (opmode 0/1/2 = b/w/l → `0xC000`/`0xC040`/`0xC080`): `Dn = Dn & <ea>`.
  Source = data modes; **An-direct (mode 1) ILLEGAL** (absent).
- **AND `Dn,<ea>`** `1100 ddd 1SS mmm rrr` (opmode 4/5/6 → `0xC100`/`0xC140`/`0xC180`): `<ea> = <ea> & Dn`,
  alterable-memory dest (`is_dst_mem_mode` — modes 2..6, abs.w/abs.l; mode 000/001 = ABCD/EXG, excluded).
- **OR** `1000 ...` — same two directions with base `0x8000` (`0x8000`/`0x8040`/`0x8080` and `0x8100`/`0x8140`/
  `0x8180`). Mode 000/001 of the `Dn,<ea>` direction = SBCD/PACK (excluded).
- **EOR `Dn,<ea>`** `1011 ddd 1SS mmm rrr` (opmode 4/5/6 → `0xB100`/`0xB140`/`0xB180`): `<ea> = <ea> ^ Dn`.
  Dest = `Dn` (mode 0, register) OR alterable memory (2..6, abs.w/abs.l). **Mode 001 = CMPM** (handled by the
  existing `cmp_class` arm, which runs first in dispatch — so the EOR arm only sees mode != 001). There is
  **no `EOR <ea>,Dn`** (opmode 0/1/2 in 0xB is CMP).
- **ANDI/ORI/EORI** (`0x02xx`/`0x00xx`/`0x0Axx`, group-0 immediate, NOT `0x?C` to-CCR/SR): **not decoded** this
  push — classified out by opcode (high nibble 0), skipped in `covered()`.

## Recipe shapes (TDD-pinned to the data)

- **ADDA/SUBA `<ea>,An`:** `ea_src(src, size, make_alu)` with `make_alu(operand) = Alu{op: Adda|Suba, size,
  a: AddrReg(dst_An), b: operand, dst: AddrReg(dst_An)}`. For **`.w`** append a uniform trailing `Internal{4}`
  (the ADDA.w/SUBA.w idle — like `ea_cmpa`'s `n2` but `n4`). For **`.l`** append **nothing** — `ea_src` routes
  to `ea_src_long`, whose built-in n4(register/#imm)/n2(memory) trailing idle already matches ADDA.l/SUBA.l
  exactly. (Shared `adda_suba_recipe(opcode, op, size)`, parameterized by `AluOp` — ADDA and SUBA differ only
  in the op.)
- **AND/OR `<ea>,Dn`:** `arith_ea_dn(opcode, AluOp::And|Or, size)` **verbatim** — already parameterized by
  `AluOp`, builds `ea_src` + `Alu{op, a: Dn, b: operand, dst: Dn}`.
- **AND/OR/EOR `Dn,<ea>` (memory dest):** `arith_dn_ea(opcode, AluOp::And|Or|Eor, size)` **verbatim** — the
  `ea_dst`/`ea_dst_long` RMW path with `Alu{op, a: mem, b: Dn, dst: Scratch(1)}` (logic is commutative, so the
  memory-minuend order is fine).
- **EOR `Dn,Dn` (mode 0, register dest):** its own arm (no memory, like `clr_recipe`'s mode-0 path):
  `[Prefetch, Alu{Eor, size, a: Dn_dest, b: Dn_src, dst: dn_dest(Dn_dest,size)}]`, plus `Internal{4}` when
  `size == Long` (EOR.l Dn,Dn = 8 cyc). `Dn_dest` = the EA reg (bits 2-0); `Dn_src` = bits 11-9.

## Vocabulary additions (introduced in the commit that first USES it — keep `-D warnings` clean)

| Item | Signature / semantics | First used |
|---|---|---|
| `AluOp::Adda` | no-flags An write: `An = An + b`, `b` sign-extended word→long when `size==Word` else full long, long boundary; writes `Dest::AddrReg`. Mirrors `AluOp::MoveA`'s no-flag early-return. | L0 |
| `AluOp::Suba` | as `Adda` but `An = An − b`. | L1 |
| `AluOp::And` | logic flags: `result = a & b`; `move_flags(result, size)` N/Z + clear V/C, **X preserved**; size-masked write-back. | L2 |
| `AluOp::Or` | as `And` with `a \| b`. | L3 |
| `AluOp::Eor` | as `And` with `a ^ b`. | L4 |

`arith_ea_dn`/`arith_dn_ea` are **already `AluOp`-parameterized** (no change). The `Adda`/`Suba` and
`And`/`Or`/`Eor` exec arms reuse existing helpers (`sign_extend16`, `move_flags`, `addr_reg_set`) — no new
flag helper. `MAX_OPS`/`SCRATCH_SLOTS` unchanged (the heaviest new recipe, ADDA.l `#imm.l` ≈ 8 ops, is well
under 20/10 — measure; bump only if exceeded).

## Clean vs deferred — the `covered()` strategy

Add three genuine-opcode predicates (`adda_suba_in_scope` base 0xD/0x9, `and_or_in_scope` base 0xC/0x8,
`eor_in_scope`) that admit ONLY the genuine register form and defer the plain `(A7)` mode-2. Verified counts
(my predicate reproduces the recon's independent count exactly — 0 discrepancy):

| File | in-scope covered | File | in-scope covered | File | in-scope covered |
|---|---|---|---|---|---|
| ADDA.w | 7935 | AND.b | 7391 | OR.l | 7399 |
| ADDA.l | 7935 | AND.w | 7419 | EOR.b | 7026 |
| SUBA.w | 7934 | AND.l | 7417 | EOR.w | 6999 |
| SUBA.l | 7971 | OR.b | 7453 | EOR.l | 7012 |
|  |  | OR.w | 7402 | **TOTAL** | **+97293** |

- Odd word/long EAs are **in scope** (the E3/E4 abort installs the group-0 14-byte frame — they PASS; no
  parity filter).
- The only intra-family deferral is the plain `(A7)` mode-2 indirect (`mode==2 && reg==7`), consistent with
  ADD/SUB/MOVE/CMP. `(A7)+`/`-(A7)` are in scope.
- The `*I` (ANDI/ORI/EORI) cases are classified out (high nibble 0 ≠ the genuine 0xC/0x8/0xB) → skipped, not
  decoded. They are a future opcode, not a deferral of *this* family.
- **Threshold:** raise `ran >=` to **383008** (current 285715 + 97293). Each commit raises it to its measured
  true count (raised, never lowered) — measure & pin at each step; the L4 total is 383008.

## Commit plan (TDD, sequential, each fully green before the next)

Each commit: extend `tools/fetch-tests.sh` `FILES` + add the real sha256 to `tools/singlesteptests.sha256` +
extend the runner `FILES`; add vocabulary in the commit that uses it; decode arm + recipe; the `covered()`/
genuine-opcode predicate; both-drivers agreement + a snapshot/restore anchor for the new shape; full gate;
conventional `feat(m68000):` commit ending in the `Co-Authored-By` line.

- [ ] **L0 — ADDA.w / ADDA.l.** Add `AluOp::Adda` (no-flag An write, `.w` sign-extends, long boundary).
  Decode `0xD0C0`/`0xD1C0`. `adda_suba_recipe` (ea_src; `.w` trailing `n4`; `.l` reuses `ea_src_long`'s
  built-in idle). Vendor ADDA.w/.l. `covered()` admits the genuine ADDA opcode (defer `(A7)` m2). Anchors:
  Dn-source `.w` (sign-extend, high bit set), An-source, memory `.w`/`.l`, `#imm.l`, an odd-EA case
  (address-error frame). Snapshot anchor.
- [ ] **L1 — SUBA.w / SUBA.l.** Add `AluOp::Suba` (mirror Adda). Decode `0x90C0`/`0x91C0`. Reuse
  `adda_suba_recipe`. Vendor SUBA.w/.l. Anchors mirror L0. Snapshot anchor.
- [ ] **L2 — AND.b / AND.w / AND.l (both directions).** Add `AluOp::And` (logic flags via `move_flags` +
  preserve X). Decode `0xC000`/`0xC040`/`0xC080` (`<ea>,Dn` → `arith_ea_dn`) and `0xC100`/`0xC140`/`0xC180`
  (`Dn,<ea>` → `arith_dn_ea`, `is_dst_mem_mode` guard). Vendor AND.b/.w/.l; `covered()` = `and_or_in_scope`
  (base 0xC; exclude An-source, ABCD/EXG corner, `*I` contaminant, `(A7)` m2). Anchors: `<ea>,Dn` (Dn/memory/
  `#imm` source, each size), `Dn,<ea>` (memory dest, each size), an odd-EA case. Snapshot anchor.
- [ ] **L3 — OR.b / OR.w / OR.l (both directions).** Add `AluOp::Or`. Decode base `0x8000`. Reuse
  `arith_ea_dn`/`arith_dn_ea`. Vendor OR.b/.w/.l; `covered()` = `and_or_in_scope` (base 0x8). Anchors mirror
  L2. Snapshot anchor.
- [ ] **L4 — EOR.b / EOR.w / EOR.l (`Dn,<ea>` only).** Add `AluOp::Eor`. Decode `0xB100`/`0xB140`/`0xB180`
  (mode 001 already taken by the CMPM arm). `eor_recipe`: mode-0 `Dn,Dn` register arm (`.l` trailing `n4`) +
  memory via `arith_dn_ea`. Vendor EOR.b/.w/.l; `covered()` = `eor_in_scope`. **Measure + pin the final
  threshold 383008.** Anchors: `EOR Dn,Dn` (each size, incl `.l` 8-cyc), `EOR Dn,(An)` RMW (each size), an
  odd-EA case. Snapshot anchor.

## Vendoring (pinned commit `e0d5ece9670205cc84a0101081837deb446f86a3`)

Add to `tools/singlesteptests.sha256` (real sha256 of the pinned `.gz`, already fetched + verified locally):

```
628b1b0da75be4da9b10b5c69266c5f2353c96d167a682f4a38e1a7d459bfc26  ADDA.w.json.gz
f613eaf7bf738c82b5ae294736a21f2dedd49fed8a5e477c38097f9f0a4761ce  ADDA.l.json.gz
9810f5df82ee6b241a6f8361984c220bcc72674b38088f18266f72509833f95f  SUBA.w.json.gz
38731811dd3269ffb28551585f811a77235cb05345c15106aee52d1f7cb2697c  SUBA.l.json.gz
eefde37e4ff72baa2d6589ee3466e457eb3d5c805fd54f54c26b31ed0e8b0fc3  AND.b.json.gz
5c6948557518dc6a3f61a46045cd8e4a49dfa97855e7aab2d4445dd0196ca685  AND.w.json.gz
ca235bbbd43fffe88021bac01a74c67cc82dc40d081cce21e80c6b834d596c58  AND.l.json.gz
252d8d0ac2031e044825073c3fc646b23bbc4c0b11d4ba8f09ef951aac28df1c  OR.b.json.gz
43fce2a92b02afa6bff3b111f3d058f6b3c4ac4a5b08f3b8537415d7b5bf1af1  OR.w.json.gz
55d175215525f6eec412cf616ecbdcc573136ca5c1d2ae4f896358288a2b6a42  OR.l.json.gz
058ad15e4d7faa7b822032bdacf915c014d51c2695bd24c2261f1373d400c035  EOR.b.json.gz
2e2c6259e590f3dd11f64ad5c2c6ac8d8e26c819b89dee4b74e5f6e977194490  EOR.w.json.gz
e2eb6bf479b8ddf74bb48a49f028050e65a39e376ee1a403096994f09108b206  EOR.l.json.gz
```

## Verification protocol (every commit) & anti-cheating rules

Not done until, from the repo root: `cargo test -p oracle-core --test determinism_gate --test proptests`
green (most-guarded), `cargo fmt --all -- --check` clean, `cargo clippy --all-targets -- -D warnings` clean,
`cargo test --workspace` green; committed with a conventional `feat(m68000):` message ending in
`Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`. Read `.github/workflows/ci.yml`; run what CI runs.

**Anti-cheating (hard rules — a verifier checks these):**
- **SST is ground truth.** Fix the recipe/code to match the suite. NEVER weaken an assertion, broaden
  `covered()` to skip cases that should pass, lower the `ran >=` threshold below the true covered count,
  `#[ignore]`/`#[cfg]`-out a test, or comment out an assert. `covered()` may only skip the documented
  deferrals (the `*I` immediate opcode — a different instruction; the plain `(A7)` mode-2 form).
- **Classify by OPCODE, not the `name` field** (the AND/OR/EOR files mix the genuine register form with the
  `*I` immediate opcode). Admitting `*I` cases would route them to `todo!()` (no decode arm) — they MUST be
  classified out.
- Both-drivers-agree and snapshot/restore-at-every-bus-boundary stay intact for every new form; the
  determinism gate stays the most-guarded job.
- No `Rc`/`RefCell`/`unsafe`/`HashMap`/floats in hashed state; `MicroState` stays fixed-size bincode.
  Never touch `../oracle/`.

## Risks

- **`*I` contamination** — the load-bearing new subtlety (parallel to the CMP 3-way mix). The genuine-opcode
  predicate (`high nibble == 0xC/0x8/0xB`) is the pin; admitting a group-0 `*I` case panics at `todo!()`.
- **ADDA.l/SUBA.l idle is NOT uniform** — it is `ea_src_long`'s built-in n4(reg/#imm)/n2(memory) split (=
  ADD.l), NOT a uniform trailing idle like ADDA.w. Do NOT append an idle for `.l`. TDD-pin against an `#imm.l`
  ADDA (n4) AND a memory-mode ADDA.l (n2).
- **ADDA.w/SUBA.w sign-extension** — `AluOp::Adda`/`Suba` sign-extend `b` internally when `size==Word`; pin
  against a Dn-source `.w` anchor whose source word has the high bit set (negative addend).
- **EOR `Dn,Dn` (mode 0)** is register-dest with NO memory (and `.l` carries a trailing n4); do NOT route it
  through `ea_dst` (memory-only). AND/OR have no register-dest `Dn,<ea>` form (mode 000/001 = ABCD/EXG).
- **AND/OR An-direct source** is illegal (`AND An,Dn` absent) — exclude mode 1 in `and_or_in_scope` (the
  decode `arith_ea_dn` arm relies on `covered()` to never feed it mode 1, exactly like the ADD.b precedent).
