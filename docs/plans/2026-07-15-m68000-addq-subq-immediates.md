# m68000 Push 0 — ADDQ/SUBQ + ADDI/SUBI/ANDI/ORI/EORI (finish the vendored coverage)

**Plan of record for step 0 of the integration pivot** (`docs/decisions/2026-07-15-integration-pivot-design.md`,
Push 0). Closes the last unimplemented common instruction family — the `*Q` quick-immediate and `*I`
immediate-to-EA forms — which today **hide as opcode contaminants inside already-vendored SST files** (ADD/SUB/
AND/OR/EOR) and are silently skipped by `covered()`. ADDQ is among the most common 68000 instructions; a real
ROM panics `decode` within its first few hundred instructions without it. After this push every one of the 15
affected files is 100% covered and the SST suite threshold reaches **1,000,058** (all vendored cases minus the
2 corrupt ASL.b entries).

This is the **proven grind cadence** (recon → this plan → gated workflow → self-verify). The recon is done and
0-mismatch; see the "Recon findings" below and memory `recon-addq-immediates`.

## Goal / scope

Implement and un-skip, over the existing vendored data (NO new files to fetch):

| Mnemonic | Encoding | Lives in | Cases |
|---|---|---|---|
| ADDQ | `0101 qqq 0 ss mmm rrr` (0x5xxx, bit8=0, ss≠3) | ADD.b/w/l | 8265 |
| SUBQ | `0101 qqq 1 ss mmm rrr` (bit8=1, ss≠3) | SUB.b/w/l | 8264 |
| ADDI | `0000 0110 ss mmm rrr` (0x06xx) | ADD.b/w/l | 910 |
| SUBI | `0000 0100 ss mmm rrr` (0x04xx) | SUB.b/w/l | 897 |
| ANDI | `0000 0010 ss mmm rrr` (0x02xx) | AND.b/w/l | 1481 |
| ORI  | `0000 0000 ss mmm rrr` (0x00xx) | OR.b/w/l  | 1461 |
| EORI | `0000 1010 ss mmm rrr` (0x0Axx) | EOR.b/w/l | 2733 |

Total **+24,011** → **976,047 → 1,000,058**. Plus a **CI-fail-when-vendor-missing** guard (design-doc item).

**In scope:** all data-alterable destination modes (Dn + `(An)`/`(An)+`/`-(An)`/`d16(An)`/`d8(An,Xn)`/`abs.w`/
`abs.l`), plus An-direct for ADDQ/SUBQ word/long; odd word/long EAs (address errors the E3/E4 abort already
covers); plain `(A7)` mode-2 (clean, un-deferred like the recent families). **Out of scope:** decode totality /
illegal-EA gates / privilege (Push A); ADDQ/SUBQ byte-to-An (illegal, absent from data).

## Recon findings (0-mismatch, my own Python over the real data)

**Value + flags = the base ops, reused verbatim.** `value_mismatch=0` AND `flag_mismatch=0` on every
register-direct (mode 0) case across all seven mnemonics. ADDQ/SUBQ = `AluOp::Add`/`Sub` (full X/N/Z/V/C, X=C);
ADDI/SUBI same; ANDI/ORI/EORI = `AluOp::And`/`Or`/`Eor` (N/Z, V=C=0, **X preserved**). These AluOps are already
0-mismatch proven on the 0xD/0x9/0xC/0x8/0xB files — nothing new in the ALU.

**Immediate encoding** (the `*I` forms): byte = low byte of ONE extension word (high byte don't-care); word =
one extension word; long = TWO extension words (hi = `prefetch[1]`, lo = the next word). Identical capture to
`cmpi_recipe`.

**Timing (clean, non-address-error lengths — all extracted from the data):**

ADDQ/SUBQ:
- Dn (mode 0): b/w = **4**, l = **8**.
- An (mode 1): w = **8**, l = **6** ← **THE QUIRK: long is cheaper.** ADDQ.l→An = 6 (n2), NOT ADDA.l's 8 (n4)
  — verified ADDA.l Dn,An = 8 in the data, so the An long idle **cannot** reuse `adda_suba_recipe`. Byte→An is
  illegal (absent). NO flags.
- memory: (An)/(An)+ 12/12/**20**; -(An) 14/14/**22**; d16(An) 16/16/**24**; d8(An,Xn) 18/18/**26**; abs.w
  16/16/**24**; abs.l 20/20/**28** (b/w/l) — i.e. exactly `arith_dn_ea`'s `ea_dst` timing.

ADDI/SUBI/ANDI/ORI/EORI (= the `*Q` figure + 4 for the extra immediate fetch):
- Dn: b/w = **8**, l = **16**.
- memory: (An)/(An)+ 16/16/**28**; -(An) 18/18/**30**; d16(An) 20/20/**32**; d8(An,Xn) 22/22/**34**; abs.w
  20/20/**32**; abs.l 24/24/**36**.

The 50/52/54/56/58 (`*Q`) and 60/62/64/66 (`*I`) lengths are odd-EA address errors — in scope via E3/E4.

## Recipe shapes (all reuse proven machinery)

**New vocabulary — exactly one operand:** `Operand::Quick(u8)` — a decode-time constant `qqq==0 ? 8 : qqq`
(1-8), zero-extended, resolved as the literal `u8`. Mirrors `Operand::Zero`/`WordStep`/`ShiftCount`. That is the
*only* new vocab in the whole push (no new `AluOp`, `MicroOp`, or `Dest`).

### ADDQ/SUBQ (`addq_recipe(opcode, op, size)`, `op ∈ {Add,Sub}` chosen by bit 8)

`data = if qqq==0 {8} else {qqq}` where `qqq = (opcode>>9)&7`. Three destination arms by `mode = (opcode>>3)&7`:

- **Dn (mode 0):** `[Prefetch, Alu{op, size, a=dn_operand(reg,size), b=Quick(data), dst=dn_dest(reg,size)}]`,
  then `Internal{4}` iff `size==Long`. (Dn is the minuend `a` — correct for the non-commutative SUBQ.)
- **An (mode 1):** `[Prefetch, Alu{Adda/Suba, size, a=AddrReg(reg), b=Quick(data), dst=AddrReg(reg)},
  Internal{ if size==Word {4} else {2} }]`. Word = 8, long = 6. NO flags. (`op==Add → Adda`, `op==Sub → Suba`.)
  A byte size to An is never decoded (illegal; absent from data).
- **memory (2-7):** `ea_dst(mode, reg, size, |a| Alu{op, size, a, b=Quick(data), dst=Scratch(1)})` — byte/word
  via `ea_dst`, long via `ea_dst_long`. Identical to `arith_dn_ea` but the addend is `Quick` instead of
  `dn_operand`. Odd EA faults on the RMW read via E3/E4.

Decode arm (0x5xxx block, placed **before** the DBcc/Scc arms or guarded so it never sees `ss==3`): match
`opcode & 0xF000 == 0x5000 && (opcode>>6)&3 != 3` with dest in scope
(`mode==0 || (mode==1 && size!=Byte) || (2..=6).contains(&mode) || (mode==7 && reg<=1)`).

### ADDI/SUBI/ANDI/ORI/EORI (`imm_rmw_recipe(opcode, op, size)`)

The **CMPI immediate-capture idiom + `ea_dst` RMW writeback** (both proven). Reuses `cmpi_recipe`'s capture
block verbatim, then — instead of the discarded compare read — an `ea_dst`/`ea_dst_long` RMW with `b =
Scratch(imm_slot)`:

- **byte/word:** `[EaCalc{base=ImmWord, index=Zero, disp=Zero, dst=IMM_SLOT}, Prefetch, <ea_dst body: Read →
  Prefetch → Alu{op,size,a=<read/Dn>,b=Scratch(IMM_SLOT),dst=…} → Write>]`. For Dn dest the "read" arm is the
  register form (no memory access): `[…capture…, Alu{op,size,a=dn_operand,b=Scratch(IMM_SLOT),dst=dn_dest},
  Prefetch]` shaped to hit b/w=8, l=16.
- **long:** the `Combine32` hi/lo capture (HI before the refill, LO after), then `ea_dst_long` with
  `b=Scratch(IMM_SLOT)`.
- Flags follow `op`: Add/Sub = full add/sub (X=C); And/Or/Eor = logic (N/Z, V=C=0, X preserved).

The exact idle/prefetch placement of the capture↔RMW seam is pinned by the impl agent against the per-cycle
transaction stream (the runner's transaction gate is the check); expected structure = `cmpi`'s capture (one
extra prefetch, +4 cyc) followed by `arith_dn_ea`'s `ea_dst` stream (= the `*Q` memory figure).

Decode arm (group-0 block, after the bit-op arms): classify by high byte
(`(opcode>>8)&0xFF ∈ {0x06→Add, 0x04→Sub, 0x02→And, 0x00→Or, 0x0A→Eor}`), require `size = (opcode>>6)&3 != 3`
and a data-alterable dest (`mode==0 || is_dst_mem_mode(mode,reg)`). DISJOINT from CMPI (0x0C), bit-static
(0x08), BTST-dynamic (0x01xx, bit 8 set), MOVEP (bit 8 set), and the `*toSR`/`*toCCR` single points
(0x007C/0x003C/0x027C/0x023C/0x0A7C/0x0A3C — mode 7/4 `#imm`, excluded by the alterable-dest guard and living in
separate files). A single shared `imm_class(opcode) -> Option<(AluOp, Size)>` helper keeps decode and
`covered()` in agreement (the `cmp_class` pattern).

## `covered()` flips (the runner)

Add two admission arms to `covered()` and one classifier, mirroring the existing `and_or_in_scope`/`cmp_class`
style, and update the FILES-table comments:

- **ADDQ/SUBQ:** `opcode & 0xF000 == 0x5000 && (opcode>>6)&3 != 3` → in scope iff the dest is
  `mode==0 || (mode==1 && size!=Byte) || (2..=6).contains(&mode) || (mode==7 && reg<=1)`. (Placed so the
  Scc/DBcc `ss==3` arms above still win.)
- **ADDI/SUBI/ANDI/ORI/EORI:** `imm_class(opcode).is_some()` → in scope iff `mode==0 || is_dst_mem_mode(mode,reg)`.
- The old comments in the FILES table that assert "the `*I` cases are skipped cleanly" for ADD/SUB/AND/OR/EOR
  are rewritten to "now decoded and in scope" (the design doc calls these out as the false "no intra-class
  deferral" claims to correct).

Per-commit threshold (`ran >= N` at the runner's assert): C0 984312 · C1 992576 · C2 993486 · C3 994383 ·
C4 995864 · C5 997325 · **C6 1000058**.

## CI-fail-when-vendor-missing (C7)

Today CI (`.github/workflows/ci.yml`, `build-test-lint` job) runs `cargo test --workspace` **without fetching**
the gitignored vendor data, and each SST test `return`s on the first missing file → the suite skips vacuously
and CI passes green having run zero SST cases. Fix:
1. Add a `tools/fetch-tests.sh` step to the `build-test-lint` job (before the test step).
2. Replace the per-test skip guard: if `VENDOR_DIR` is missing **and** `std::env::var("CI").is_ok()` →
   `panic!` with the fetch instruction; else keep the current clean local skip. (Applies to every SST test
   function's top-of-body guard.)

This makes a fetch failure fail loudly in CI while preserving the friction-free local skip.

## Gated commit list (the workflow)

Each commit = fresh impl agent (TDD, full CI gate, ONE conventional `feat(m68000)`/`test(m68000)` commit) →
independent adversarial verifier (re-runs the whole gate, recomputes the per-file covered count from the JSON,
anti-cheating audit). Sequential/dependent. C0 lands `Operand::Quick` + the three ADDQ arms + decode + covered;
C1 reuses them (only `AluOp::Sub`/`Suba` + the bit-8 direction). C2 lands `imm_rmw_recipe` + `imm_class`;
C3–C6 reuse it (different `AluOp`). Anchor a mid-instruction snapshot/restore case for each genuinely new shape
(the ADDQ An arm, the ADDQ memory RMW, the `*I` capture+RMW long form).

| # | Commit | Threshold |
|---|---|---|
| C0 | `feat(m68000): ADDQ — quick-immediate add (Operand::Quick + Dn/An/mem arms)` | 984312 |
| C1 | `feat(m68000): SUBQ — the quick-immediate subtract twin of ADDQ` | 992576 |
| C2 | `feat(m68000): ADDI — immediate-to-EA add (cmpi capture + ea_dst RMW)` | 993486 |
| C3 | `feat(m68000): SUBI — the immediate-to-EA subtract twin of ADDI` | 994383 |
| C4 | `feat(m68000): ANDI — immediate-to-EA logical AND (logic flags)` | 995864 |
| C5 | `feat(m68000): ORI — the immediate-to-EA logical OR twin of ANDI` | 997325 |
| C6 | `feat(m68000): EORI — the immediate-to-EA logical XOR twin of ANDI` | 1000058 |
| C7 | `test(m68000): fail the SST suite loudly when vendor data is missing in CI` | 1000058 |

## Anti-cheating / invariants (the verifier enforces)

- `covered()` may only ADMIT the new opcodes; never weaken/remove/`#[ignore]`/`#[cfg]` an assert, never lower a
  `ran >=` below the true covered count, never add a parity filter (odd EAs pass via E3/E4). Threshold is raised
  each commit to the exact new count, never lowered.
- Both drivers agree on regs/SR/RAM/prefetch/cycles + the per-cycle transaction stream; snapshot/restore holds
  at every bus boundary for the new shapes. Every commit individually `cargo fmt`-clean.
- The ALU is reused, not reinvented: ADDQ/ADDI must produce byte-identical flags to the proven `AluOp::Add`
  (same for Sub/And/Or/Eor). No new flag code.
- Clean-room: impl/verify agents never open jgenesis/BlastEm/Genesis-Plus-GX. Behavior enters only via the
  vendored vectors + permissive docs. No `Co-Authored-By: Claude` trailer.

## Risks

- **The ADDQ.l→An = 6 quirk** (long cheaper than word) is the one non-obvious timing fact; a naive
  `adda_suba_recipe` reuse gives 8 and fails the cycle gate. Bespoke An arm with the explicit word→n4 / long→n2
  idle. Pinned above.
- **Decode collisions in the crowded group-0 space** — mitigated by classifying on the exact high byte + the
  data-alterable guard (disjoint from every existing group-0 arm, proven by the `cmp_class`/`and_or_in_scope`
  precedent) and a totality spot-check deferred to Push A.
- **The capture↔RMW seam ordering** for the `*I` forms is the only genuinely new bus-stream composition; the
  per-cycle transaction gate catches any mis-ordering, and the structure is `cmpi` (proven) + `ea_dst` (proven).
