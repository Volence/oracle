//! SingleStepTests runner for the 68000 micro-op framework.
//!
//! Drives the pinned, vendored SingleStepTests data (`tools/fetch-tests.sh`) for every covered `ADD`/`SUB`
//! case in **word, byte and long** sizes — `Dn,<ea>` (alterable-memory destination: (An) / (An)+ / -(An) /
//! d16(An) / d8(An,Xn) / abs.w / abs.l) and `<ea>,Dn` (register destination) for the source modes Dn / An
//! (word/long) / (An) / (An)+ / -(An) / d16(An) / d8(An,Xn) / abs.w / abs.l / d16(PC) / d8(PC,Xn) / #imm —
//! plus the full `MOVE` family (`MOVE.b`/`MOVE.w`/`MOVE.l`, the EA→EA composition) — and asserts post
//! regs/SR/RAM/prefetch, the cycle count, **and** the per-cycle bus-transaction stream (byte-granular for
//! `.b`; two word accesses per `.l` operand — hi then lo for a read; for a long memory write the order is
//! per-instruction: an `ADD.l`/`SUB.l` RMW and a `MOVE.l -(An)` predec store write lo then hi, while every
//! other `MOVE.l` memory dest writes hi then lo), through *both* framework drivers (run-to-completion fast
//! path and the step-one-micro-op quiesce path), which must also agree with each other.
//!
//! Also drives `Bcc`/`BRA` (the conditional/unconditional PC-relative branch, `0x6xxx`, cc != 1 — cc == 1 is
//! `BSR`, a later commit): the condition is resolved at decode time, emitting the taken or not-taken linear
//! recipe, so the variable cycle counts (byte not-taken 8, word not-taken 12, taken 10 both forms) emerge
//! naturally and both drivers stay in agreement.
//!
//! Versioned scope manifest: **odd-address word/long accesses are now IN scope** (E4 — the execution-time
//! address-error abort installs the group-0 14-byte vector-3 frame, so an odd word/long EA, an odd branch /
//! jump / return target, and an odd popped PC/return-address all PASS unchanged; byte accesses never fault).
//! Still deferred (genuinely-unimplemented / non-address-error mode-scope decisions, NOT odd-address cases):
//! the `A7` form of the older `(An)` (mode 2) memory access (a pre-existing mode-scope deferral — its `(A7)+`/
//! `-(A7)` siblings and every other A7 form ARE in scope), `An`-direct as a byte source (`ADD.b An,Dn` is
//! illegal), and the remaining EA modes / sizes (see [`covered`]). The auto-(in/de)crement `(A7)+`/`-(A7)`
//! forms are in scope for both sizes (word steps 2; byte steps 2 for A7 to keep the SP even). If the vendor
//! data is missing, the test skips cleanly (run `tools/fetch-tests.sh`).

use oracle_core::m68000::bus68k::{FlatBus, Transaction, TxKind};
use oracle_core::m68000::decode::{cmp_class, CmpClass};
use oracle_core::m68000::microop::{Cpu68000, Size, Step};
use oracle_core::m68000::registers::Registers;
use serde_json::Value;
use std::path::Path;

const VENDOR_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../vendor/ProcessorTests/68000/v1"
);

/// Mnemonic files driven by the current decode. Extend as opcode coverage grows (keep in sync with
/// `tools/fetch-tests.sh`).
const FILES: &[&str] = &[
    "ADD.w.json",
    "SUB.w.json",
    "ADD.b.json",
    "SUB.b.json",
    "ADD.l.json",
    "SUB.l.json",
    "MOVE.w.json",
    "MOVE.b.json",
    "MOVE.l.json",
    "MOVEA.w.json",
    "MOVEA.l.json",
    "Bcc.json",
    "BSR.json",
    "JMP.json",
    "JSR.json",
    "RTS.json",
    "DBcc.json",
    "RTR.json",
    "TRAP.json",
    "RTE.json",
    "TRAPV.json",
    "CHK.json",
    "ANDItoSR.json",
    "ORItoSR.json",
    "EORItoSR.json",
    "RESET.json",
    // The CMP.* files are 3-WAY MIXES (CMP <ea>,Dn + CMPM (Ay)+,(Ax)+ + CMPI #imm,<ea>), all mislabeled
    // "CMP.<sz>" in `name` — classified by OPCODE via `cmp_class`. N0 added the Cmp class, N1 the Cmpm class,
    // N2 the Cmpi class — so these files are now FULLY covered (CMPA is its own file). The only intra-class
    // deferral is the pre-existing `(A7)` mode-2 plain-indirect mode-scope convention.
    "CMP.b.json",
    "CMP.w.json",
    "CMP.l.json",
    // CMPA.w / CMPA.l (`1011 aaa 0 11/111 mmm rrr`, opmode 3/7) — pure CMPA files (not a mix). N3 decodes the
    // flag-only address compare `An − <ea>` (An the minuend, full 32; `.w` source sign-extended word→long).
    // All 12 source modes in scope except the pre-existing `(A7)` mode-2 plain-indirect convention.
    "CMPA.w.json",
    "CMPA.l.json",
    // TST.b / TST.w / TST.l (`0100 1010 SS mmm rrr`, 0x4A00/4A40/4A80) — the flag-only test `<ea> − 0`. N4
    // decodes the data-alterable EA set {Dn, (An), (An)+, -(An), d16(An), d8(An,Xn), abs.w, abs.l} (An-direct /
    // PC-relative / #imm are not data-alterable and are absent). Every case in scope (no `(A7)` mode-2 deferral
    // — TST never writes, so the plain `(A7)` indirect is clean); odd word/long EAs are address errors the
    // E3/E4 abort covers.
    "TST.b.json",
    "TST.w.json",
    "TST.l.json",
    // CLR.b / CLR.w / CLR.l (`0100 0010 SS mmm rrr`, 0x4200/4240/4280) — clear the data-alterable EA to 0
    // (Z=1/N=0/V=0/C=0, X PRESERVED = move_flags(0)). N5 decodes the data-alterable EA set {Dn, (An), (An)+,
    // -(An), d16(An), d8(An,Xn), abs.w, abs.l}. CLR is a READ-then-WRITE (it reads the EA, discards it, then
    // writes 0) — it reuses the `ea_dst`/`ea_dst_long` RMW path, so the odd-EA case faults on the READ (low5 =
    // 0x15), covered by the E3/E4 abort. NO `(A7)` mode-2 deferral parity beyond the pre-existing convention.
    "CLR.b.json",
    "CLR.w.json",
    "CLR.l.json",
    // MOVE.q (`0111 ddd 0 dddddddd`, 0x7000 | dn<<9 | imm8) — MOVEQ: load a sign-extended 8-bit immediate into
    // the FULL 32 bits of Dn (N = msb, Z = value == 0, V/C cleared, X PRESERVED). N6 decodes the whole opcode
    // space (bit 8 = 0 — the only legal form, and every vendored case has bit 8 clear). Every case is in scope:
    // a single flag-ALU + the trailing FC-6 queue refill (length 4), no operand fetch (the value is the opcode's
    // own low byte), no EA modes, no odd-address sub-cases.
    "MOVE.q.json",
    // ADDA.w / ADDA.l (`1101 aaa s11 mmm rrr`, opmode 3 = .w / 7 = .l = 0xD0C0 / 0xD1C0) — address arithmetic
    // `An = An + src`, NO flags (SR untouched). L0 decodes both sizes: `.w` sign-extends the source word→long
    // before the add (mirroring MOVEA.w / CMPA.w), `.l` adds the full 32. All 12 source modes in scope
    // (An-direct legal — it is address arithmetic; odd word/long source EAs are address errors the E3/E4 abort
    // covers, no parity filter) except the pre-existing `(A7)` (mode 2) plain-indirect deferral (its `(A7)+` /
    // `-(A7)` siblings ARE in scope). Files are 100% pure ADDA (no contaminants).
    "ADDA.w.json",
    "ADDA.l.json",
    // SUBA.w / SUBA.l (`1001 aaa s11 mmm rrr`, opmode 3 = .w / 7 = .l = 0x90C0 / 0x91C0) — address arithmetic
    // `An = An − src`, NO flags (SR untouched), a near-exact mirror of ADDA. L1 decodes both sizes: `.w`
    // sign-extends the source word→long before the subtract (mirroring MOVEA.w / CMPA.w), `.l` subtracts the
    // full 32. All 12 source modes in scope (An-direct legal — it is address arithmetic; odd word/long source
    // EAs are address errors the E3/E4 abort covers, no parity filter) except the pre-existing `(A7)` (mode 2)
    // plain-indirect deferral (its `(A7)+` / `-(A7)` siblings ARE in scope). Files are 100% pure SUBA (no
    // contaminants).
    "SUBA.w.json",
    "SUBA.l.json",
    // AND.b / AND.w / AND.l (`1100 ddd 0SS mmm rrr` = 0xC000/40/80 `<ea>,Dn`, `1100 ddd 1SS mmm rrr` =
    // 0xC100/40/80 `Dn,<ea>`) — bitwise AND in BOTH directions. L2 decodes both: `<ea>,Dn` (Dn = Dn & <ea>,
    // source = data modes, An-direct mode 1 ILLEGAL/absent) reuses `arith_ea_dn` verbatim; `Dn,<ea>` (<ea> =
    // <ea> & Dn, alterable-memory dest 2..6/abs.w/abs.l, mode 000/001 = ABCD/EXG reserved) reuses `arith_dn_ea`
    // verbatim. Sets N = msb / Z = (result == 0), clears V/C, PRESERVES X. **These files are CONTAMINATED with
    // ANDI** (the `0x02xx` immediate opcode, high nibble 0 — a DIFFERENT instruction NOT implemented this push):
    // `covered()` classifies by OPCODE (high nibble == 0xC), admitting ONLY the genuine register form so the
    // ANDI cases are skipped cleanly (never decoded). All source/dest modes in scope except the pre-existing
    // `(A7)` (mode 2) plain-indirect deferral; odd word/long EAs are address errors the E3/E4 abort covers.
    "AND.b.json",
    "AND.w.json",
    "AND.l.json",
    // OR.b / OR.w / OR.l (`1000 ddd 0SS mmm rrr` = 0x8000/40/80 `<ea>,Dn`, `1000 ddd 1SS mmm rrr` =
    // 0x8100/40/80 `Dn,<ea>`) — bitwise OR in BOTH directions. L3 decodes both: `<ea>,Dn` (Dn = Dn | <ea>,
    // source = data modes, An-direct mode 1 ILLEGAL/absent) reuses `arith_ea_dn` verbatim; `Dn,<ea>` (<ea> =
    // <ea> | Dn, alterable-memory dest 2..6/abs.w/abs.l, mode 000/001 = SBCD/PACK reserved) reuses `arith_dn_ea`
    // verbatim. Identical to AND in every respect except the bit op (`|` vs `&`) and the base nibble (0x8 vs
    // 0xC): sets N = msb / Z = (result == 0), clears V/C, PRESERVES X (the new `AluOp::Or`). **These files are
    // CONTAMINATED with ORI** (the `0x00xx` immediate opcode, high nibble 0 — a DIFFERENT instruction NOT
    // implemented this push): `covered()` classifies by OPCODE (`and_or_in_scope`: high nibble == 0x8),
    // admitting ONLY the genuine register form so the ORI cases are skipped cleanly (never decoded). All
    // source/dest modes in scope except the pre-existing `(A7)` (mode 2) plain-indirect deferral; odd word/long
    // EAs are address errors the E3/E4 abort covers.
    "OR.b.json",
    "OR.w.json",
    "OR.l.json",
    // EOR.b / EOR.w / EOR.l (`1011 ddd 1SS mmm rrr` = 0xB100/40/80, opmode 4/5/6 = b/w/l) — bitwise `<ea> =
    // <ea> ^ Dn`, the `Dn,<ea>` direction ONLY (opmode 0/1/2 of 0xB is CMP, not `EOR <ea>,Dn`). L4 decodes the
    // data-register dest (mode 000 = `Dn,Dn`, its own no-memory arm — `.l` carries a trailing n4, so `EOR.l
    // Dn,Dn` = 8 cyc) and the alterable-memory dest (modes 2..6/abs.w/abs.l) via `arith_dn_ea` VERBATIM (EOR
    // Dn,<ea> = ADD Dn,<ea> byte-for-byte). **Mode field 001 = CMPM** (a DIFFERENT instruction handled by the
    // `cmp_class` arm FIRST in dispatch — the EOR arm never sees mode 001). Same MOVE flag shape as AND/OR (N =
    // msb / Z = (result == 0), V/C cleared, X PRESERVED) via the new `AluOp::Eor`. **These files are CONTAMINATED
    // with EORI** (the `0x0Axx` immediate opcode, high nibble 0 — a DIFFERENT instruction NOT implemented this
    // push): `covered()` classifies by OPCODE (`eor_in_scope`: high nibble == 0xB), admitting ONLY the genuine
    // register form so the EORI cases are skipped cleanly (never decoded). All dest modes in scope except the
    // pre-existing `(A7)` (mode 2) plain-indirect deferral; odd word/long EAs are address errors the E3/E4 abort
    // covers.
    "EOR.b.json",
    "EOR.w.json",
    "EOR.l.json",
    // NEG.b / NEG.w / NEG.l (`0100 0100 SS mmm rrr`, 0x4400/4440/4480, SS bits 7-6 = b/w/l) — negate the
    // data-alterable EA: `res = (0 − d) & mask` with FULL SUBTRACT flags (NEG ≡ `Sub(0, d)`): N = msb(res),
    // Z = (res == 0), V = (d == sign-min) overflow, C = X = (d != 0) borrow (`AluOp::Neg`). G0 decodes the
    // data-alterable EA set {Dn (0), (An) (2), (An)+ (3), -(An) (4), d16(An) (5), d8(An,Xn) (6), abs.w (7/0),
    // abs.l (7/1)}; An-direct / PC-relative / #imm are not data-alterable and are absent. NEG is a READ-then-WRITE
    // (it reads the EA, NEGATES it, then writes it back) — it reuses the `ea_dst`/`ea_dst_long` RMW path (the SAME
    // as CLR, but the read operand is the unary source instead of discarded), so an odd word/long EA address-errors
    // on the READ (low5 = 0x15), covered by the E3/E4 abort (no parity filter). The only intra-family deferral is
    // the pre-existing `(A7)` (mode 2) plain-indirect form (its `(A7)+` / `-(A7)` siblings ARE in scope). Files are
    // 100% pure NEG (no contaminants). Per-file true counts: NEG.b 7915 + NEG.w 7893 + NEG.l 7917 = +23725.
    "NEG.b.json",
    "NEG.w.json",
    "NEG.l.json",
    // NEGX.b / NEGX.w / NEGX.l (`0100 0000 SS mmm rrr`, 0x4000/4040/4080, SS bits 7-6 = b/w/l) — negate-with-extend
    // the data-alterable EA: `res = (0 − d − X_in) & mask` with SUBX-style flags: N = msb(res), **Z is STICKY —
    // `Z_final = Z_in AND (res == 0)`** (NEGX only ever CLEARS Z, never sets it — the multi-precision idiom),
    // V = `(d & res & signbit) != 0`, C = X = NOT(d == 0 AND X_in == 0) borrow (`AluOp::Negx`, where
    // `X_in = (sr >> 4) & 1` and `Z_in = (sr >> 2) & 1` feed BOTH the value and the borrow). G1 decodes the same
    // data-alterable EA set as NEG {Dn (0), (An) (2), (An)+ (3), -(An) (4), d16(An) (5), d8(An,Xn) (6), abs.w
    // (7/0), abs.l (7/1)} via the SHARED `neg_family_recipe` (only the `AluOp` exec differs — the recipe shape is
    // identical); An-direct / PC-relative / #imm are not data-alterable and are absent. NEGX is a READ-then-WRITE
    // (it reads the EA, transforms it, then writes it back), so an odd word/long EA address-errors on the READ
    // (low5 = 0x15), covered by the E3/E4 abort (no parity filter). The only intra-family deferral is the
    // pre-existing `(A7)` (mode 2) plain-indirect form (its `(A7)+` / `-(A7)` siblings ARE in scope). Files are
    // 100% pure NEGX (no contaminants). Per-file true counts: NEGX.b 7917 + NEGX.w 7893 + NEGX.l 7883 = +23693.
    "NEGX.b.json",
    "NEGX.w.json",
    "NEGX.l.json",
    // NOT.b / NOT.w / NOT.l (`0100 0110 SS mmm rrr`, 0x4600/4640/4680, SS bits 7-6 = b/w/l) — bitwise-complement
    // the data-alterable EA: `res = (~d) & mask` with LOGIC flags (the SAME MOVE flag shape as AND/OR/EOR):
    // N = msb(res), Z = (res == 0), **V = 0, C = 0, X PRESERVED** (re-injected `ccr_nz | (sr & CCR_X)`, never
    // computed) via the new `AluOp::Not`. G2 decodes the same data-alterable EA set as NEG/NEGX {Dn (0), (An)
    // (2), (An)+ (3), -(An) (4), d16(An) (5), d8(An,Xn) (6), abs.w (7/0), abs.l (7/1)} via the SHARED
    // `neg_family_recipe` (only the `AluOp` exec differs — `~a` instead of a subtraction; the recipe shape is
    // identical); An-direct / PC-relative / #imm are not data-alterable and are absent. NOT is a READ-then-WRITE
    // (it reads the EA, complements it, then writes it back), so an odd word/long EA address-errors on the READ
    // (low5 = 0x15), covered by the E3/E4 abort (no parity filter). The only intra-family deferral is the
    // pre-existing `(A7)` (mode 2) plain-indirect form (its `(A7)+` / `-(A7)` siblings ARE in scope). Files are
    // 100% pure NOT (no contaminants). Per-file true counts: NOT.b 7901 + NOT.w 7894 + NOT.l 7899 = +23694.
    "NOT.b.json",
    "NOT.w.json",
    "NOT.l.json",
    // EXT.w / EXT.l (`0100 1000 1S 000 rrr`, 0x4880 .w / 0x48C0 .l, mask `opcode & 0xFFF8`) — sign-extend the
    // `Dn`-only source whose result WIDTH follows the size: EXT.w sign-extends the low BYTE to 16 bits and writes
    // the LOW WORD (the high word of Dn is PRESERVED), N = bit15 / Z = (word == 0); EXT.l sign-extends the low
    // WORD to 32 bits and writes the FULL 32, N = bit31 / Z = (long == 0). Both: V = 0, C = 0, X PRESERVED (LOGIC
    // flags) via the new unary `AluOp::Ext`. SWAP (`0100 1000 01 000 rrr`, 0x4840, mask `opcode & 0xFFF8`) — swap
    // the two 16-bit halves of Dn on the FULL 32 bits (size always Long), LOGIC flags on bit31 / zero via the new
    // unary `AluOp::Swap`. G3 decodes all three with mask `0xFFF8` (mode FIXED 000 = `Dn`; the low 3 bits the
    // register) — NOT `0xFFC0`, which would swallow the PEA/MOVEM neighbours in 0x48xx (mode ≥ 2, reserved this
    // push). `Dn`-only — NO memory, NO fault, 4 cyc each (one Prefetch, no idle). `covered()` = `ext_swap_in_scope`
    // (the `0xFFF8` mask match, NO deferral — every case is `Dn`-direct). Files are 100% pure (no contaminants).
    // Per-file true counts: EXT.w 8065 + EXT.l 8065 + SWAP 8065 = +24195.
    "EXT.w.json",
    "EXT.l.json",
    "SWAP.json",
    // Scc `<ea>` (`0101 cccc 11 mmm rrr`, opcode & 0xF0C0 == 0x50C0 — EXCL the 0x50C8 DBcc mode-001 form) —
    // the conditional byte set: write 0xFF if the condition `cc` (bits 11-8) is TRUE else 0x00, with NO flags
    // (`final.sr == initial.sr`). The condition is resolved at DECODE time (like Bcc/DBcc/TRAPV) via
    // `condition_true(cc, sr)`; cc 0 = T (always 0xFF) / cc 1 = F (always 0x00) are BOTH legal. C0 decodes the
    // full data-alterable EA set {Dn (0), (An) (2 — incl the clean `(A7)` mode-2 indirect), (An)+ (3), -(An)
    // (4), d16(An) (5), d8(An,Xn) (6), abs.w (7/0), abs.l (7/1)} via the new no-flag `MicroOp::SetByte` — the
    // `Dn` arm is `[Prefetch, SetByte (+ Internal(2) ONLY when the condition is true)]` (FALSE = 4 cyc, TRUE =
    // 6), the memory arm is byte-for-byte CLR's read-then-write RMW (`ea_dst` with `Size::Byte`) but writing
    // the conditional constant with NO flags. Byte-only → NO odd-EA faults, NO `(A7)` mode-2 deferral (Scc =
    // CLR's exact byte RMW; CLR covers `(A7)` m2 and passes). The Scc.json file is 100% PURE (one opcode, no
    // contaminant). Per-mode true counts: 1280 / 1253 / 1303 / 1302 / 1320 / 1295 / 159 / 153 = 8065 (the WHOLE
    // file in scope — no deferral).
    "Scc.json",
    // TAS `Dn` (`0100 1010 11 mmm rrr`, opcode & 0xFFC0 == 0x4AC0) — the indivisible test-and-set, REGISTER
    // form this commit (`mode == 0` only; the memory modes are the atomic RMW, a later commit). Read the byte
    // → set N = bit7 / Z = (byte == 0), clear V/C, PRESERVE X → write `byte | 0x80` (the flags are on the READ
    // byte, the written value is `read | 0x80` — DISTINCT). The 0x4AC0 space is the SS == 3 (`& 0xC0 == 0xC0`)
    // sub-case of the 0x4A00 TST space, excluded from the TST arm via `& 0xC0 != 0xC0` — no conflict. New
    // vocabulary: the unary `AluOp::Tas` (flags via `move_flags` over the INPUT byte + X-reinject, write
    // `a | 0x80`); the `tas_recipe` Dn arm is `[Prefetch, Alu]` (4 cyc, no memory). `covered()` admits TAS
    // mode 0 ONLY this commit (the not-yet-decoded memory cases are SKIPPED before decode). The TAS.json file
    // is 100% PURE (one opcode); TAS mode 0 (Dn) = 1237 (TAS memory = 6828 is a later commit).
    "TAS.json",
    // BTST `<ea>` (dynamic `0000 ddd 1 00 mmm rrr` = 0x01xx / static `0000 1000 00 mmm rrr` = 0x08xx) — test a
    // single bit, setting ONLY Z = NOT(bit); X/N/V/C + the SR system byte all PRESERVED. READ-ONLY (no write).
    // B0 decodes BOTH forms via the new `AluOp::Btst` + the bit-number operand (dynamic = `D[(opcode>>9)&7]` /
    // static = the captured `prefetch[1]` ext word, cmpi-style). The bit width follows the operand: a `Dn`
    // operand is 32-bit (mod 32, `Size::Long`), a memory/`#imm`/PC-relative operand is 8-bit (mod 8,
    // `Size::Byte`). The FULL read-only source set is in scope (no deferral): dynamic Dn (0) + (An)/(An)+/-(An)/
    // d16(An)/d8(An,Xn) (2-6) + abs.w/abs.l/d16(PC)/d8(PC,Xn)/#imm (7/0..7/4); static the same MINUS #imm
    // (7/0..7/3). An-direct (mode 1 = MOVEP) is absent. The plain `(A7)` mode-2 indirect is COVERED (a clean
    // byte read, like CLR/TST — NO deferral). Byte memory → NO odd-EA faults (no parity filter). The BTST.json
    // file is 100% PURE (one op-type): dynamic 7185 + static 880 = 8065 (the WHOLE file in scope).
    "BTST.json",
    // BCHG `<ea>` (dynamic `0000 ddd 1 01 mmm rrr` = 0x01xx / static `0000 1000 01 mmm rrr` = 0x08xx, tt bits
    // 7-6 == 01) — test then TOGGLE a single bit (`operand ^= 1<<pos`), setting ONLY Z = NOT(the PRE-modify
    // bit); X/N/V/C + the SR system byte all PRESERVED. A read-modify-WRITE to a data-alterable destination. B1
    // decodes BOTH forms via the new `AluOp::Bchg` (Btst + toggle) + the shared `bit_recipe`. The bit width
    // follows the dest: a `Dn` dest is 32-bit (mod 32, `Size::Long`, FULL-32 write with one bit flipped), a
    // memory dest is 8-bit (mod 8, `Size::Byte`, byte RMW). The register `+2` is a DECODE-TIME `pos >= 16`
    // decision (the dynamic bit number is a live `Dn`); memory has NO `+2`. The FULL in-scope EA set per op is
    // covered (no deferral): Dn (0) + data-alterable memory (2-6, 7/0, 7/1). The plain `(A7)` mode-2 indirect is
    // COVERED (a clean byte RMW, like CLR/TST — NO deferral). Byte memory → NO odd-EA faults (no parity filter).
    // The BCHG.json file is 100% PURE (one op-type): dynamic 7173 + static 892 = 8065 (the WHOLE file in scope).
    "BCHG.json",
    // BCLR `<ea>` (dynamic `0000 ddd 1 10 mmm rrr` = 0x01xx / static `0000 1000 10 mmm rrr` = 0x08xx, tt bits
    // 7-6 == 10) — test then CLEAR a single bit (`operand &= !(1<<pos)`), setting ONLY Z = NOT(the PRE-clear
    // bit); X/N/V/C + the SR system byte all PRESERVED. A read-modify-WRITE to a data-alterable destination. B2
    // decodes BOTH forms via the new `AluOp::Bclr` (Btst + clear) reusing the shared `bit_recipe` — IDENTICAL to
    // BCHG EXCEPT the register base idle is `n4` (BCLR is 8/10 cyc, 2 slower than BCHG/BSET's 6/8). The bit width
    // follows the dest: a `Dn` dest is 32-bit (mod 32, `Size::Long`, FULL-32 write with one bit cleared), a
    // memory dest is 8-bit (mod 8, `Size::Byte`, byte RMW). The register `+2` is a DECODE-TIME `pos >= 16`
    // decision (the dynamic bit number is a live `Dn`); memory has NO `+2` (identical to BCHG, fixed byte). The
    // FULL in-scope EA set per op is covered (no deferral): Dn (0) + data-alterable memory (2-6, 7/0, 7/1). The
    // plain `(A7)` mode-2 indirect is COVERED (a clean byte RMW, like CLR/TST — NO deferral). Byte memory → NO
    // odd-EA faults (no parity filter). The BCLR.json file is 100% PURE: dynamic 7166 + static 899 = 8065.
    "BCLR.json",
    // BSET `<ea>` (dynamic `0000 ddd 1 11 mmm rrr` = 0x01xx / static `0000 1000 11 mmm rrr` = 0x08xx, tt bits
    // 7-6 == 11) — test then SET a single bit (`operand |= 1<<pos`), setting ONLY Z = NOT(the PRE-set bit);
    // X/N/V/C + the SR system byte all PRESERVED. A read-modify-WRITE to a data-alterable destination. B3 (the
    // FINAL bit-op) decodes BOTH forms via the new `AluOp::Bset` (Btst + set) reusing the shared `bit_recipe` —
    // IDENTICAL to BCHG (the register base idle is `n2`, the SAME as BCHG — BSET is 6/8 cyc, NOT BCLR's 8/10).
    // The bit width follows the dest: a `Dn` dest is 32-bit (mod 32, `Size::Long`, FULL-32 write with one bit
    // set), a memory dest is 8-bit (mod 8, `Size::Byte`, byte RMW). The register `+2` is a DECODE-TIME `pos >=
    // 16` decision (the dynamic bit number is a live `Dn`); memory has NO `+2` (identical to BCHG, fixed byte).
    // The FULL in-scope EA set per op is covered (no deferral): Dn (0) + data-alterable memory (2-6, 7/0, 7/1).
    // The plain `(A7)` mode-2 indirect is COVERED (a clean byte RMW, like CLR/TST — NO deferral). Byte memory →
    // NO odd-EA faults (no parity filter). The BSET.json file is 100% PURE: dynamic 7099 + static 966 = 8065.
    "BSET.json",
    // ASL.b / ASL.w / ASL.l (`0xExxx`, AS/left) — the FOUNDATIONAL shift/rotate op (S0): arithmetic shift
    // LEFT. Three forms, classified by OPCODE: REGISTER immediate-count (`1110 ccc d ss 0 00 rrr`, bit 5 = 0,
    // count `ccc != 0 ? ccc : 8` = 1-8), REGISTER `Dn`-count (`… 1 00 rrr`, bit 5 = 1, count `D[ccc] & 63` =
    // 0-63, read LIVE at decode), MEMORY shift-by-1 (`1110 0 00 1 11 mmm rrr`, bits 7-6 == 11, WORD only — the
    // `.b`/`.l` files have NO memory form). ASL owns the **V** flag (the sign bit changed at ANY point during
    // the shift); C = the last bit shifted out (`bit(n-cnt)`, 0 once `cnt > n`), **X = C**, **N** = msb / **Z**
    // = (res == 0). ZERO COUNT (`cnt == 0`, only the `Dn` form): value unchanged, V = 0, C = 0, **X PRESERVED**.
    // New vocabulary: `Operand::ShiftCount(u8)` (the decode-time literal count, mirroring `Operand::Zero`/
    // `WordStep`) + `AluOp::Asl` + the shared `shift_recipe` (register `[Prefetch, Alu, Internal{(base-4)+
    // 2*cnt}]`, base 6 `.b`/`.w` / 8 `.l` → `6+2*cnt` / `8+2*cnt`; memory the CLR.w/NEG.w word `ea_dst` RMW).
    // Register timing is DECODE-TIME data-dependent (the idle `2*cnt` reads the live `Dn`, up to 63 → 126 idle).
    // Memory shift-by-1 (word): (An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l 20; an odd EA
    // address-errors on the READ (low5 = 0x15, the E3/E4 abort), exactly like CLR.w/NEG.w. The FULL in-scope EA
    // set is covered: every register shift (no memory → no fault) + (for `.w`) the data-alterable memory set
    // (2-6, 7/0, 7/1) INCLUDING the clean `(A7)` mode-2 indirect (NO deferral, NO parity filter). The ONLY
    // exclusion anywhere in the family is `ASL.b`'s 2 PROVABLY-CORRUPT, self-contradictory entries (opcode
    // 0xE502 — a register-only `ASL.b #2,D2` with NO memory access, yet final.d2 is full-register garbage no
    // shift can produce; a correct CPU gives cdfb7ff8 / 417c7ef4). Keyed precisely on opcode 0xE502 + the
    // corrupt INITIAL d2 AND the corrupt FINAL d2 (the same initial d2 ALSO appears in 2 LEGIT cases a correct
    // CPU matches — so the final-d2 leg is REQUIRED to isolate EXACTLY the 2 corrupt). Per-file true counts:
    // ASL.b 8063 (8065 - 2 corrupt) + ASL.w 8065 + ASL.l 8065 = +24193. ASL.b is the ONLY file not 8065.
    "ASL.b.json",
    "ASL.w.json",
    "ASL.l.json",
    // ASR.b / ASR.w / ASR.l (`0xExxx`, AS/right) — arithmetic shift RIGHT (S1): the sign-EXTENDING right
    // shift. Same three forms / classification / `shift_recipe` as ASL (only the AluOp + the AS/right decode
    // arm differ — direction bit 8 == 0, type AS bits 4-3 (register) / 10-9 (memory) == 0). Value: the vacated
    // top bits are filled with the operand's sign bit (`cnt >= n` → all-sign-bits). C = the last bit shifted
    // out of the OPERAND — `bit(cnt-1)` for `1 <= cnt <= n`, else **0** (THE ASR CARRY QUIRK: `cnt > n` → C=0,
    // NOT the sign bit, even though the value still sign-extends — a naive "last bit out = sign for over-shift"
    // mismatches 1642 ASR.b cases); **X = C**; **V = 0** always (ASR never sets V — only ASL owns V); **N** =
    // msb / **Z** = (res == 0). ZERO COUNT (`cnt == 0`, only the `Dn` form): value unchanged, V = 0, C = 0,
    // **X PRESERVED**. Reuses `Operand::ShiftCount` + the shared `shift_recipe` VERBATIM (register `[Prefetch,
    // Alu, Internal{(base-4)+2*cnt}]`, base 6 `.b`/`.w` / 8 `.l`; memory the CLR.w/NEG.w word `ea_dst` RMW).
    // Timing identical to ASL: register `.b`/`.w` = 6 + 2*cnt, `.l` = 8 + 2*cnt; memory shift-by-1 (word):
    // (An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l 20; an odd EA address-errors on the
    // READ (low5 = 0x15, the E3/E4 abort). The FULL in-scope EA set is covered (every register shift + the
    // `.w` data-alterable memory set INCL the clean `(A7)` mode-2 indirect, NO deferral / NO parity filter).
    // NO corrupt entries (only ASL.b has the 2 self-contradictory cases). Per-file true counts: ASR.b 8065 +
    // ASR.w 8065 + ASR.l 8065 = +24195. All three files are 100% PURE for their op+size.
    "ASR.b.json",
    "ASR.w.json",
    "ASR.l.json",
    // LSL.b / LSL.w / LSL.l (`0xExxx`, LS/left) — logical shift LEFT (S2): IDENTICAL to ASL's value and carry,
    // with the SOLE difference that **V is FORCED to 0** (a logical shift never tracks the sign change — only
    // ASL owns V). Same three forms / classification / `shift_recipe` as ASL/ASR (only the AluOp + the LS/left
    // decode arm differ — direction bit 8 == 1, type LS bits 4-3 (register) / 10-9 (memory) == 1). Value:
    // `res = (x << cnt) & mask` when `cnt < n`, else 0 (an over-shift clears the register). C = the last bit
    // shifted out of the OPERAND — `bit(n-cnt)` for `1 <= cnt <= n`, else 0; **X = C**; **V = 0** ALWAYS (the
    // only difference from ASL); **N** = msb / **Z** = (res == 0). ZERO COUNT (`cnt == 0`, only the `Dn` form):
    // value unchanged, V = 0, C = 0, **X PRESERVED**. Reuses `Operand::ShiftCount` + the shared `shift_recipe`
    // VERBATIM (register `[Prefetch, Alu, Internal{(base-4)+2*cnt}]`, base 6 `.b`/`.w` / 8 `.l`; memory the
    // CLR.w/NEG.w word `ea_dst` RMW). Timing identical to ASL/ASR: register `.b`/`.w` = 6 + 2*cnt, `.l` = 8 +
    // 2*cnt; memory shift-by-1 (word): (An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l 20;
    // an odd EA address-errors on the READ (low5 = 0x15, the E3/E4 abort). The FULL in-scope EA set is covered
    // (every register shift + the `.w` data-alterable memory set INCL the clean `(A7)` mode-2 indirect, NO
    // deferral / NO parity filter). NO corrupt entries (only ASL.b has the 2 self-contradictory cases).
    // Per-file true counts: LSL.b 8065 + LSL.w 8065 + LSL.l 8065 = +24195. All three files are 100% PURE.
    "LSL.b.json",
    "LSL.w.json",
    "LSL.l.json",
    // LSR.b / LSR.w / LSR.l (`0xExxx`, LS/right) — logical shift RIGHT (S3): the ZERO-FILL right shift (contrast
    // ASR, which sign-EXTENDS). Same three forms / classification / `shift_recipe` as ASL/ASR/LSL (only the
    // AluOp + the LS/right decode arm differ — direction bit 8 == 0, type LS bits 4-3 (register) / 10-9
    // (memory) == 1). Value: `res = x >> cnt` when `cnt < n`, else 0 (an over-shift clears the register). C =
    // the last bit shifted out of the OPERAND — `bit(cnt-1)` for `1 <= cnt <= n`, else 0 (same form as ASR's
    // carry; with no sign, `cnt > n` → 0 is natural); **X = C**; **V = 0** always; **N** = msb(res) — ALWAYS 0
    // for any `cnt >= 1` (the msb is zero-filled); **Z** = (res == 0). ZERO COUNT (`cnt == 0`, only the `Dn`
    // form): value unchanged, V = 0, C = 0, **X PRESERVED**, N/Z from the unchanged operand (so N CAN be 1 —
    // it is NOT forced to 0). Reuses `Operand::ShiftCount` + the shared `shift_recipe` VERBATIM (register
    // `[Prefetch, Alu, Internal{(base-4)+2*cnt}]`, base 6 `.b`/`.w` / 8 `.l`; memory the CLR.w/NEG.w word
    // `ea_dst` RMW). Timing identical to ASL/ASR/LSL: register `.b`/`.w` = 6 + 2*cnt, `.l` = 8 + 2*cnt; memory
    // shift-by-1 (word): (An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l 20; an odd EA
    // address-errors on the READ (low5 = 0x15, the E3/E4 abort). The FULL in-scope EA set is covered (every
    // register shift + the `.w` data-alterable memory set INCL the clean `(A7)` mode-2 indirect, NO deferral /
    // NO parity filter). NO corrupt entries (only ASL.b has the 2 self-contradictory cases). Per-file true
    // counts: LSR.b 8065 + LSR.w 8065 + LSR.l 8065 = +24195. All three files are 100% PURE.
    "LSR.b.json",
    "LSR.w.json",
    "LSR.l.json",
    // ROL.b / ROL.w / ROL.l (`0xExxx`, RO/left) — rotate LEFT (S4): a plain bit-rotate that does NOT pass
    // through X — contrast ROXL, which threads X through an (n+1)-bit rotate (S6). Same three forms /
    // classification / `shift_recipe` as ASL/ASR/LSL/LSR (only the AluOp + the RO/left decode arm differ —
    // direction bit 8 == 1, type RO bits 4-3 (register) / 10-9 (memory) == 3). Value: `r = cnt % n`; `res =
    // x` when `cnt == 0 || r == 0` (a whole-register rotation leaves the value unchanged), else `((x << r) |
    // (x >> (n - r))) & mask`. C = the last bit rotated out — `(x >> ((n - (cnt % n)) % n)) & 1` for `cnt !=
    // 0`, else 0 (a zero count clears C). **X is PRESERVED** (ROL/ROR never touch X — re-inject the live X),
    // NOT set to C. **V = 0** always. **N** = msb(res), **Z** = (res == 0). ZERO COUNT (`cnt == 0`, only the
    // `Dn` form): value unchanged, V = 0, C = 0, **X PRESERVED**, N/Z from the unchanged operand. cnt a
    // NONZERO multiple of n (`r == 0`, e.g. ROL.b #8): value unchanged but C comes from the formula (= the
    // operand's low bit region). Reuses `Operand::ShiftCount` + the shared `shift_recipe` VERBATIM (register
    // `[Prefetch, Alu, Internal{(base-4)+2*cnt}]`, base 6 `.b`/`.w` / 8 `.l`; memory the CLR.w/NEG.w word
    // `ea_dst` RMW). Timing identical to ASL/ASR/LSL/LSR: register `.b`/`.w` = 6 + 2*cnt, `.l` = 8 + 2*cnt;
    // memory shift-by-1 (word): (An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l 20; an odd
    // EA address-errors on the READ (low5 = 0x15, the E3/E4 abort). The FULL in-scope EA set is covered (every
    // register shift + the `.w` data-alterable memory set INCL the clean `(A7)` mode-2 indirect, NO deferral /
    // NO parity filter). NO corrupt entries (only ASL.b has the 2 self-contradictory cases). Per-file true
    // counts: ROL.b 8065 + ROL.w 8065 + ROL.l 8065 = +24195. All three files are 100% PURE.
    "ROL.b.json",
    "ROL.w.json",
    "ROL.l.json",
    // ROR.b / ROR.w / ROR.l (`0xExxx`, RO/right) — rotate RIGHT (S5): ROL's right-direction twin, a plain
    // bit-rotate that does NOT pass through X — contrast ROXR, which threads X through an (n+1)-bit rotate (S7).
    // Same three forms / classification / `shift_recipe` as ASL/ASR/LSL/LSR/ROL (only the AluOp + the RO/right
    // decode arm differ — direction bit 8 == 0, type RO bits 4-3 (register) / 10-9 (memory) == 3). Value: `r =
    // cnt % n`; `res = x` when `cnt == 0 || r == 0` (a whole-register rotation leaves the value unchanged), else
    // `((x >> r) | (x << (n - r))) & mask`. C = the last bit rotated out — `(x >> ((cnt - 1) % n)) & 1` for
    // `cnt != 0`, else 0 (a zero count clears C). **X is PRESERVED** (ROL/ROR never touch X — re-inject the live
    // X), NOT set to C. **V = 0** always. **N** = msb(res), **Z** = (res == 0). ZERO COUNT (`cnt == 0`, only the
    // `Dn` form): value unchanged, V = 0, C = 0, **X PRESERVED**, N/Z from the unchanged operand. cnt a NONZERO
    // multiple of n (`r == 0`, e.g. ROR.b #8): value unchanged but C comes from the formula (= the operand's
    // high bit region). Reuses `Operand::ShiftCount` + the shared `shift_recipe` VERBATIM (register `[Prefetch,
    // Alu, Internal{(base-4)+2*cnt}]`, base 6 `.b`/`.w` / 8 `.l`; memory the CLR.w/NEG.w word `ea_dst` RMW).
    // Timing identical to ASL/ASR/LSL/LSR/ROL: register `.b`/`.w` = 6 + 2*cnt, `.l` = 8 + 2*cnt; memory
    // shift-by-1 (word): (An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l 20; an odd EA
    // address-errors on the READ (low5 = 0x15, the E3/E4 abort). The FULL in-scope EA set is covered (every
    // register shift + the `.w` data-alterable memory set INCL the clean `(A7)` mode-2 indirect, NO deferral /
    // NO parity filter). NO corrupt entries (only ASL.b has the 2 self-contradictory cases). Per-file true
    // counts: ROR.b 8065 + ROR.w 8065 + ROR.l 8065 = +24195. All three files are 100% PURE.
    "ROR.b.json",
    "ROR.w.json",
    "ROR.l.json",
    // ROXL.b / ROXL.w / ROXL.l (`0xExxx`, ROX/left) — rotate LEFT THROUGH X (S6): the FIRST X-threading rotate.
    // Unlike ROL/ROR (which leave X untouched) and ASL/ASR/LSL/LSR (which set X = C from the value), ROXL treats
    // `{X:operand}` as an `(n+1)`-bit register — X sits ABOVE the msb — and rotates it left by `cnt % (n+1)`; the
    // final bit ejected into X is BOTH the new X and C, so the result depends on the INCOMING X. Same three forms
    // / classification / `shift_recipe` as ASL/ASR/LSL/LSR/ROL/ROR (only the AluOp + the ROX/left decode arm
    // differ — direction bit 8 == 1, type ROX bits 4-3 (register) / 10-9 (memory) == 2). Value: `per = n + 1`,
    // `eff = cnt % per`; `comb = ((xin << n) | x)` in `per` bits, rotated left by `eff` (the wider `u64` so the
    // `.l` 33-bit case does not overflow), `res = comb & mask`. **C = X = (comb >> n) & 1** (the bit ejected into
    // X). **V = 0** always. **N** = msb(res), **Z** = (res == 0). ZERO COUNT (`cnt == 0`, only the `Dn` form):
    // value UNCHANGED, **C = X (the incoming X — NOT 0), X UNCHANGED**, V = 0, N/Z from the unchanged operand. A
    // cnt that wraps the `(n+1)` PERIOD (e.g. cnt = 9 for `.b` → eff = 9 % 9 = 0) returns the value to its start.
    // Reuses `Operand::ShiftCount` + the shared `shift_recipe` VERBATIM (register `[Prefetch, Alu,
    // Internal{(base-4)+2*cnt}]`, base 6 `.b`/`.w` / 8 `.l`; memory the CLR.w/NEG.w word `ea_dst` RMW). Timing
    // identical to every shift/rotate: register `.b`/`.w` = 6 + 2*cnt, `.l` = 8 + 2*cnt; memory shift-by-1
    // (word): (An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l 20; an odd EA address-errors on
    // the READ (low5 = 0x15, the E3/E4 abort). The FULL in-scope EA set is covered (every register shift + the
    // `.w` data-alterable memory set INCL the clean `(A7)` mode-2 indirect, NO deferral / NO parity filter). NO
    // corrupt entries (only ASL.b has the 2 self-contradictory cases). Per-file true counts: ROXL.b 8065 +
    // ROXL.w 8065 + ROXL.l 8065 = +24195. All three files are 100% PURE.
    "ROXL.b.json",
    "ROXL.w.json",
    "ROXL.l.json",
    // ROXR.b / ROXR.w / ROXR.l (`0xExxx`, ROX/right) — rotate RIGHT THROUGH X (S7, the FINAL shift/rotate op):
    // ROXL's right-direction twin. Unlike ROL/ROR (which leave X untouched) and ASL/ASR/LSL/LSR (which set X = C
    // from the value), ROXR treats `{X:operand}` as an `(n+1)`-bit register — X sits ABOVE the msb — and rotates
    // it right by `cnt % (n+1)`; the final bit ejected into X is BOTH the new X and C, so the result depends on
    // the INCOMING X. Same three forms / classification / `shift_recipe` as ASL/ASR/LSL/LSR/ROL/ROR/ROXL (only
    // the AluOp + the ROX/right decode arm differ — direction bit 8 == 0, type ROX bits 4-3 (register) / 10-9
    // (memory) == 2). Value: `per = n + 1`, `eff = cnt % per`; `comb = ((xin << n) | x)` in `per` bits, rotated
    // right by `eff` (the wider `u64` so the `.l` 33-bit case does not overflow), `res = comb & mask`. **C = X =
    // (comb >> n) & 1** (the bit ejected into X). **V = 0** always. **N** = msb(res), **Z** = (res == 0). ZERO
    // COUNT (`cnt == 0`, only the `Dn` form): value UNCHANGED, **C = X (the incoming X — NOT 0), X UNCHANGED**, V
    // = 0, N/Z from the unchanged operand. A cnt that wraps the `(n+1)` PERIOD (e.g. cnt = 9 for `.b` → eff = 9 %
    // 9 = 0) returns the value to its start. Reuses `Operand::ShiftCount` + the shared `shift_recipe` VERBATIM
    // (register `[Prefetch, Alu, Internal{(base-4)+2*cnt}]`, base 6 `.b`/`.w` / 8 `.l`; memory the CLR.w/NEG.w
    // word `ea_dst` RMW). Timing identical to every shift/rotate: register `.b`/`.w` = 6 + 2*cnt, `.l` = 8 +
    // 2*cnt; memory shift-by-1 (word): (An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l 20; an
    // odd EA address-errors on the READ (low5 = 0x15, the E3/E4 abort). The FULL in-scope EA set is covered
    // (every register shift + the `.w` data-alterable memory set INCL the clean `(A7)` mode-2 indirect, NO
    // deferral / NO parity filter). NO corrupt entries (only ASL.b has the 2 self-contradictory cases). Per-file
    // true counts: ROXR.b 8065 + ROXR.w 8065 + ROXR.l 8065 = +24195. All three files are 100% PURE.
    "ROXR.b.json",
    "ROXR.w.json",
    "ROXR.l.json",
];

fn u32f(v: &Value, key: &str) -> u32 {
    v.get(key)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("missing field {key}")) as u32
}

fn build_regs(ini: &Value) -> Registers {
    let mut d = [0u32; 8];
    for (i, slot) in d.iter_mut().enumerate() {
        *slot = u32f(ini, &format!("d{i}"));
    }
    let mut a = [0u32; 7];
    for (i, slot) in a.iter_mut().enumerate() {
        *slot = u32f(ini, &format!("a{i}"));
    }
    let pf = ini["prefetch"].as_array().unwrap();
    Registers {
        d,
        a,
        usp: u32f(ini, "usp"),
        ssp: u32f(ini, "ssp"),
        pc: u32f(ini, "pc"),
        sr: u32f(ini, "sr") as u16,
        prefetch: [
            pf[0].as_u64().unwrap() as u16,
            pf[1].as_u64().unwrap() as u16,
        ],
    }
}

fn build_bus(ini: &Value) -> FlatBus {
    let mut bus = FlatBus::new();
    for pair in ini["ram"].as_array().unwrap() {
        let p = pair.as_array().unwrap();
        bus.poke(p[0].as_u64().unwrap() as u32, p[1].as_u64().unwrap() as u8);
    }
    bus
}

fn expected_transactions(t: &Value) -> Vec<Transaction> {
    let mut out = Vec::new();
    for tr in t["transactions"].as_array().unwrap() {
        let arr = tr.as_array().unwrap();
        let kind = match arr[0].as_str().unwrap() {
            "r" => TxKind::Read,
            "w" => TxKind::Write,
            "t" => TxKind::Tas, // the atomic TAS read-modify-write (ONE locked bus cycle; value = the written byte)
            _ => continue,      // 'n' idle cycles etc. — not memory transactions
        };
        // The transaction array is [kind, cycles, fc, addr, size_token, value]; index 4 is the size token
        // (".w" / ".b"), index 5 the value. A byte access is byte-granular on the bus and records the single
        // on-bus byte as its value.
        let size = match arr[4].as_str().unwrap() {
            ".b" => Size::Byte,
            ".w" => Size::Word,
            other => panic!("unexpected size token {other}"),
        };
        out.push(Transaction {
            kind,
            fc: arr[2].as_u64().unwrap() as u8,
            addr: arr[3].as_u64().unwrap() as u32,
            size,
            value: arr[5].as_u64().unwrap() as u16,
        });
    }
    out
}

fn assert_final(t: &Value, regs: &Registers, bus: &FlatBus) {
    let name = t["name"].as_str().unwrap_or("?");
    let fin = &t["final"];
    for i in 0..8 {
        assert_eq!(regs.d[i], u32f(fin, &format!("d{i}")), "d{i} [{name}]");
    }
    for i in 0..7 {
        assert_eq!(regs.a[i], u32f(fin, &format!("a{i}")), "a{i} [{name}]");
    }
    assert_eq!(regs.usp, u32f(fin, "usp"), "usp [{name}]");
    assert_eq!(regs.ssp, u32f(fin, "ssp"), "ssp [{name}]");
    assert_eq!(regs.pc, u32f(fin, "pc"), "pc [{name}]");
    assert_eq!(regs.sr, u32f(fin, "sr") as u16, "sr [{name}]");
    let pf = fin["prefetch"].as_array().unwrap();
    assert_eq!(
        regs.prefetch,
        [
            pf[0].as_u64().unwrap() as u16,
            pf[1].as_u64().unwrap() as u16
        ],
        "prefetch [{name}]"
    );
    for pair in fin["ram"].as_array().unwrap() {
        let p = pair.as_array().unwrap();
        let addr = p[0].as_u64().unwrap() as u32;
        let val = p[1].as_u64().unwrap() as u8;
        assert_eq!(bus.peek(addr), val, "ram[{addr:#x}] [{name}]");
    }
}

/// The MOVE size of this opcode, or `None` if it is not a (non-`MOVEA`) `MOVE` the framework covers. Layout
/// `00 SS RRR MMM mmm rrr`: bits 15-14 = 00, the size field (bits 13-12) is `01` byte / `11` word / `10`
/// long; `dst_mode == 1` (`MOVEA`) is excluded (a later commit, and byte MOVEA is illegal).
fn move_size(opcode: u16) -> Option<Size> {
    if (opcode >> 6) & 7 == 1 {
        return None; // MOVEA — a later commit
    }
    match (opcode >> 12) & 0xF {
        0b0011 => Some(Size::Word),
        0b0001 => Some(Size::Byte),
        0b0010 => Some(Size::Long),
        _ => None,
    }
}

/// Whether the framework covers this `MOVE.b`/`MOVE.w`/`MOVE.l` case. Source = all 12 EA modes (byte
/// excludes `An`-direct — `MOVE.b An,<ea>` is illegal; word and long allow it); destination = `Dn` plus the
/// alterable-memory modes (`(An)`/`(An)+`/`-(An)`/`d16(An)`/`d8(An,Xn)`/`abs.w`/`abs.l`); `dst_mode == 1`
/// (`MOVEA`) is a later commit. For **word/long**, a memory access (source OR destination) to an odd EA
/// raises an address error → xfail; for **byte** there is NO odd-address error (byte EAs may be odd) → no
/// parity filter. The `(A7)` (mode 2) form stays xfail (the prior convention) for both source and dest, all
/// sizes.
fn move_covered(opcode: u16) -> bool {
    move_size(opcode).is_some()
}

/// The MOVE-specific mode-scope filter (called once `move_covered` matches). **No parity filter** — E4 made
/// odd word/long EAs (source OR destination) coverable: the execution-time address-error abort installs the
/// group-0 14-byte frame, so an odd MOVE access PASSES unchanged. The only remaining deferrals are
/// mode-scope: the supported source / destination modes, the illegal `MOVE.b An,<ea>` byte-source, and the
/// `(A7)` (mode-2) plain-indirect form (a pre-existing non-address-error mode-scope deferral — its `(A7)+`/
/// `-(A7)` siblings ARE in scope).
fn move_in_scope(opcode: u16) -> bool {
    let byte = move_size(opcode).expect("move_covered gates move_size") == Size::Byte;
    let dst_reg = (opcode >> 9) & 7;
    let dst_mode = (opcode >> 6) & 7;
    let src_mode = (opcode >> 3) & 7;
    let src_reg = opcode & 7;
    // Supported source modes: Dn (0, always legal) + (for word/long only) An-direct (1, illegal `MOVE.b
    // An,<ea>`) + (An)/(An)+/-(An)/d16(An)/d8(An,Xn) (2..=6) + abs.w/abs.l/d16(PC)/d8(PC,Xn)/#imm (7/0..=4).
    // Supported dest modes: Dn (0) + alterable memory (2..=6, abs.w/abs.l).
    let src_ok = src_mode == 0
        || (src_mode == 1 && !byte)
        || (2..=6).contains(&src_mode)
        || (src_mode == 7 && src_reg <= 4);
    let dst_ok = dst_mode == 0
        || (2..=6).contains(&dst_mode)
        || (dst_mode == 7 && (dst_reg == 0 || dst_reg == 1));
    if !src_ok || !dst_ok {
        return false;
    }
    // (A7) mode-2 plain-indirect form stays deferred (pre-existing mode-scope convention, NOT an odd-address
    // case) — source and destination, both sizes.
    if src_mode == 2 && src_reg == 7 {
        return false;
    }
    if dst_mode == 2 && dst_reg == 7 {
        return false;
    }
    true
}

/// The MOVEA size of this opcode, or `None` if it is not a `MOVEA` the framework covers. Layout
/// `00 SS RRR 001 mmm rrr`: bits 15-14 = 00, `dst_mode` (bits 8-6) == 1 (An), and the size field (bits
/// 13-12) is `11` word / `10` long. Byte MOVEA (size `01` with `dst_mode == 1`) is ILLEGAL → `None`.
fn movea_size(opcode: u16) -> Option<Size> {
    if (opcode >> 14) != 0 || (opcode >> 6) & 7 != 1 {
        return None; // not a MOVE-family opcode, or dst_mode != 1 (plain MOVE)
    }
    match (opcode >> 12) & 3 {
        0b11 => Some(Size::Word),
        0b10 => Some(Size::Long),
        _ => None, // byte MOVEA is illegal
    }
}

/// Whether the framework covers this `MOVEA.w`/`MOVEA.l` case (called once `movea_size` matches). Source =
/// all 12 EA modes (`An`-direct is a legal MOVEA source); destination is always `An` (a register write — no
/// memory access). **No parity filter** — E4 made an odd word/long memory source coverable (the address-error
/// abort installs the 14-byte frame; the auto-increment register bump is committed before the faulting read,
/// pinned to the data). The only remaining deferral is the `(A7)` (mode-2) plain-indirect source (a
/// pre-existing non-address-error mode-scope convention).
fn movea_in_scope(opcode: u16) -> bool {
    let src_mode = (opcode >> 3) & 7;
    let src_reg = opcode & 7;
    // Source modes: Dn (0), An (1), (An)/(An)+/-(An)/d16(An)/d8(An,Xn) (2..=6), abs.w/abs.l/d16(PC)/d8(PC,Xn)/
    // #imm (7/0..=4). All are legal MOVEA sources (An-direct included for both sizes).
    let src_ok = src_mode == 0
        || src_mode == 1
        || (2..=6).contains(&src_mode)
        || (src_mode == 7 && src_reg <= 4);
    if !src_ok {
        return false;
    }
    // (A7) mode-2 plain-indirect source stays deferred (pre-existing mode-scope convention, NOT odd-address).
    if src_mode == 2 && src_reg == 7 {
        return false;
    }
    true
}

/// Whether this opcode is a `Bcc`/`BRA` the framework covers (`0110 cccc dddddddd`, 0x6xxx; cc != 1 — cc ==
/// 1 is `BSR`, a later commit, and is excluded). `Bcc.json` carries `BRA` (0x60xx) + cc 2..=15; the cc == 1
/// `BSR` cases live in `BSR.json` (not this file).
fn bcc_covered(opcode: u16) -> bool {
    opcode >> 12 == 0b0110 && (opcode >> 8) & 0xF != 1
}

/// Whether this opcode is a `DBcc` the framework covers (`0101 cccc 11001 rrr`, opcode & 0xF0F8 == 0x50C8 —
/// the `An`-direct (mode 001) special case of the `Scc` opcode space; only this exact form is DBcc, every
/// other mode is `Scc`, which is NOT implemented). `DBcc.json` carries the full DBcc family.
fn dbcc_covered(opcode: u16) -> bool {
    opcode & 0xF0F8 == 0x50C8
}

/// Whether this opcode is a `BSR` the framework covers (`0110 0001 dddddddd`, 0x61xx; cc == 1 — the BSR
/// encoding, decoded as its own arm). `BSR.json` carries `BSR.b`/`BSR.w` plus the 35 cases of `0x61FF` — on
/// the 68000 the `disp8 == 0xFF` byte form is displacement −1, so the target `pc + 1` is **odd** → an
/// address error; E4's abort handles it, so every `0x61FF` case is now in scope (no separate deferral). Every
/// `BSR` form is always-taken; an odd target (including `0x61FF`) is an address error the E4 abort covers, so
/// there is no parity filter.
fn bsr_covered(opcode: u16) -> bool {
    opcode & 0xFF00 == 0x6100
}

/// Whether this opcode is a `JMP <control ea>` the framework covers (`0100 1110 11 mmm rrr`, 0x4EC0 | ea):
/// the seven 68000 control addressing modes — `(An)` 010, `(d16,An)` 101, `(d8,An,Xn)` 110, `abs.w` 111/0,
/// `abs.l` 111/1, `(d16,PC)` 111/2, `(d8,PC,Xn)` 111/3. (`JMP` accepts no data-register / `(An)+` / `-(An)` /
/// `#imm` modes — those encodings are illegal and never appear in `JMP.json`.)
fn jmp_covered(opcode: u16) -> bool {
    if opcode & 0xFFC0 != 0x4EC0 {
        return false;
    }
    let mode = (opcode >> 3) & 7;
    let reg = opcode & 7;
    matches!(mode, 2 | 5 | 6) || (mode == 7 && matches!(reg, 0..=3))
}

/// Whether this opcode is a `JSR <control ea>` the framework covers (`0100 1110 10 mmm rrr`, 0x4E80 | ea):
/// the SAME seven 68000 control addressing modes as `JMP` — `(An)` 010, `(d16,An)` 101, `(d8,An,Xn)` 110,
/// `abs.w` 111/0, `abs.l` 111/1, `(d16,PC)` 111/2, `(d8,PC,Xn)` 111/3 — only the opcode prefix differs from
/// `JMP` (0x4EC0). (`JSR` accepts no data-register / `(An)+` / `-(An)` / `#imm` modes — those encodings are
/// illegal and never appear in `JSR.json`.)
fn jsr_covered(opcode: u16) -> bool {
    if opcode & 0xFFC0 != 0x4E80 {
        return false;
    }
    let mode = (opcode >> 3) & 7;
    let reg = opcode & 7;
    matches!(mode, 2 | 5 | 6) || (mode == 7 && matches!(reg, 0..=3))
}

/// Whether this opcode is an `RTS` the framework covers (`0x4E75` — the sole RTS encoding; `RTS.json` carries
/// only `0x4E75`).
fn rts_covered(opcode: u16) -> bool {
    opcode == 0x4E75
}

/// Whether this opcode is an `RTR` the framework covers (`0x4E77` — the sole RTR encoding; `RTR.json` carries
/// only `0x4E77`).
fn rtr_covered(opcode: u16) -> bool {
    opcode == 0x4E77
}

/// Whether this opcode is an `RTE` the framework covers (`0x4E73` — the sole RTE encoding; `RTE.json` carries
/// only `0x4E73`).
fn rte_covered(opcode: u16) -> bool {
    opcode == 0x4E73
}

/// Whether this opcode is a `TRAP #n` the framework covers (`0100 1110 0100 nnnn`, 0x4E40 | n — the 16-point
/// block 0x4E40..=0x4E4F). `TRAP.json` carries all 16 vectors. Every vendored case is fully in scope: they
/// all start in supervisor mode (S=1, T=0) with an even SSP and an even handler address (length 34, no
/// address-error sub-cases), so there is no parity/scope filter to apply.
///
/// CAVEAT (correctness-only, NOT gate-validated): because every case is already supervisor with T clear, the
/// supervisor-entry transform (`EnterException`: set S, clear T, switch A7 user→supervisor via the S-bit
/// routing) is exercised *structurally* by every frame push but is a **no-op on the data** — the user→
/// supervisor (usp→ssp) switch and the T-clear are implemented to spec but cannot be distinguished from a
/// no-op by any vendored TRAP case (the same honest caveat the plan records for the *toSR / privilege paths).
fn trap_covered(opcode: u16) -> bool {
    opcode & 0xFFF0 == 0x4E40
}

/// Whether this opcode is a `TRAPV` the framework covers (`0x4E76` — the sole TRAPV encoding; `TRAPV.json`
/// carries only `0x4E76`). Every vendored case is fully in scope: V=0 → no trap (a single prefetch, length 4);
/// V=1 → the standard 6-byte exception frame to vector 7 with a LEADING prefetch (length 34). All start in
/// supervisor mode with an even SSP and an even handler address (no address-error sub-cases), so there is no
/// parity/scope filter to apply. Same correctness-only caveat as TRAP: the S/T/A7 supervisor-entry transform
/// is structurally exercised on the trap path but a no-op on the (always-supervisor) data.
fn trapv_covered(opcode: u16) -> bool {
    opcode == 0x4E76
}

/// Whether this opcode is a `CHK <ea>,Dn` the framework covers (`0100 ddd 110 mmm rrr`, opcode & 0xF1C0 ==
/// 0x4180). The bounds-check reads a word from the source EA, then traps to vector 6 (the standard 6-byte
/// frame) if `Dn.w < 0` or `Dn.w > bound`. All 11 legal source modes are in scope — `Dn` (0),
/// `(An)`/`(An)+`/`-(An)`/`d16(An)`/`d8(An,Xn)` (2..=6), `abs.w`/`abs.l`/`d16(PC)`/`d8(PC,Xn)`/`#imm` (7/0..=4);
/// `An`-direct (mode 1) is illegal for CHK and never appears in `CHK.json`. **No parity filter** — an odd
/// source-EA word read is an address error the E3/E4 abort covers (the 14-byte vector-3 frame), so odd EAs
/// PASS unchanged. Unlike the older ADD/SUB/MOVE families CHK has **no `(A7)` mode-2 deferral**: its `(A7)`
/// plain-indirect bound read is in scope (it is a plain word read like any other — the pre-existing mode-2 A7
/// convention was never applied to CHK).
fn chk_covered(opcode: u16) -> bool {
    if opcode & 0xF1C0 != 0x4180 {
        return false;
    }
    let mode = (opcode >> 3) & 7;
    let reg = opcode & 7;
    matches!(mode, 0 | 2 | 3 | 4 | 5 | 6) || (mode == 7 && reg <= 4)
}

/// Whether this opcode is one of the privileged immediate-to-SR logic ops the framework covers — `ANDItoSR`
/// (`0x027C`), `ORItoSR` (`0x007C`), `EORItoSR` (`0x0A7C`), each the sole encoding in its file. Every vendored
/// case starts in **supervisor** mode (the legal, SR-modifying path), so all are in scope.
///
/// CAVEAT (correctness-only, NOT gate-validated): the user-mode privilege-violation entry is implemented to
/// spec but never appears in the vendored data (every case is supervisor). What IS gate-exercised — and is the
/// load-bearing pin — is the **mid-instruction FC switch**: an `AND`/`EOR` that clears S makes the two
/// re-prefetch reads run under the NEW (user) function code FC=2 instead of FC=6, pinned per case by the
/// transaction stream.
fn to_sr_covered(opcode: u16) -> bool {
    matches!(opcode, 0x027C | 0x007C | 0x0A7C)
}

/// Whether this opcode is a `RESET` (`0x4E70`, the sole encoding; `RESET.json` carries only `0x4E70`). Every
/// vendored case is in scope: all supervisor, length 132 (`n4` + `n124` reset-line idle + one queue refill),
/// no register state change beyond the prefetch queue. The user-mode privilege-violation entry is
/// correctness-only (not gated).
fn reset_covered(opcode: u16) -> bool {
    opcode == 0x4E70
}

/// Whether the framework covers this case out of the 3-way `CMP.*` mix — admitting **only the `Cmp` class**
/// this commit (`CMP <ea>,Dn`, `1011 ddd 0SS mmm rrr`, opmode 0/1/2 = b/w/l). The CMPM and CMPI cases in the
/// same files are deferred to N1/N2 (they classify as [`CmpClass::Cmpm`]/[`CmpClass::Cmpi`] → not covered →
/// skipped cleanly, never decoded). Classification is by OPCODE (`cmp_class`), never the misleading `name`.
///
/// All 12 source modes are in scope (An-direct is legal for w/l, illegal/absent for `.b`); an odd word/long
/// source EA is an address error the E3/E4 abort covers (no parity filter). The only deferral is the `(A7)`
/// (mode-2) plain-indirect source — the pre-existing non-address-error mode-scope convention shared with
/// ADD/SUB (its `(A7)+`/`-(A7)` siblings ARE in scope).
fn cmp_in_scope(opcode: u16) -> bool {
    match cmp_class(opcode) {
        // CMP `<ea>,Dn`: all source modes in scope, except the `(A7)` mode-2 plain-indirect (the pre-existing
        // mode-scope convention shared with ADD/SUB; its `(A7)+`/`-(A7)` siblings ARE in scope).
        CmpClass::Cmp => {
            let mode = (opcode >> 3) & 7;
            let reg = opcode & 7;
            !(mode == 2 && reg == 7)
        }
        // CMPM `(Ay)+,(Ax)+` (N1): both operands are post-increment reads — no `(A7)` mode-2 exclusion applies
        // (the `(A7)+` form is in scope, A7 steps by 2 for byte); odd word/long EAs are address errors the
        // E3/E4 abort covers (no parity filter).
        CmpClass::Cmpm => true,
        // CMPI `#imm,<ea>` (N2): the data-alterable destination EA modes only — `Dn` (0, no memory access),
        // `(An)` (2), `(An)+` (3), `-(An)` (4), `d16(An)` (5), `d8(An,Xn)` (6), `abs.w` (7/0), `abs.l` (7/1).
        // `An`-direct (illegal for CMPI), PC-relative and `#imm` are NOT data-alterable and are absent from the
        // data. The `(A7)` (mode 2) plain-indirect form follows the same pre-existing mode-scope deferral as
        // Cmp/ADD/SUB (its `(A7)+`/`-(A7)` siblings ARE in scope); odd word/long EAs are address errors the
        // E3/E4 abort covers (no parity filter).
        CmpClass::Cmpi => {
            let mode = (opcode >> 3) & 7;
            let reg = opcode & 7;
            match mode {
                0 => true,                         // Dn-direct (no memory access)
                2 => reg != 7,                     // (An) — A7 mode-2 deferred
                3 | 4 => true,                     // (An)+ / -(An)
                5 | 6 => true,                     // d16(An) / d8(An,Xn)
                7 if reg == 0 || reg == 1 => true, // abs.w / abs.l
                _ => false,                        // An-direct / PC-rel / #imm: absent / illegal
            }
        }
        // CMPA (its own decode arm) and non-CMP opcodes — not covered by this CMP-file dispatch.
        _ => false,
    }
}

/// Whether the framework covers this genuine register-form `AND`/`OR` case — the load-bearing classifier for
/// the **CONTAMINATED** `AND.*`/`OR.*` files (each mixes the genuine register opcode with the dedicated `ANDI`/
/// `ORI` immediate opcode in the group-0 space, `0x02xx`/`0x00xx`, high nibble 0). Classifying **by OPCODE**
/// (high nibble `0xC` for AND / `0x8` for OR), this admits ONLY the genuine register form so the `*I` cases —
/// a DIFFERENT instruction not implemented this push — are skipped cleanly (never decoded, never reaching the
/// `todo!()`). `false` for any non-`0xC`/`0x8` opcode (so a group-0 `ANDI`/`ORI` falls through to the rest of
/// `covered()`, which also rejects it).
///
/// - **`<ea>,Dn`** (opmode 0/1/2 = b/w/l): source = data modes. **An-direct (mode 1) is ILLEGAL/absent** (the
///   `AND An,Dn` encoding does not exist — excluded for all sizes, the `arith_ea_dn` decode arm relies on this).
///   The `(A7)` (mode 2) plain-indirect source is the pre-existing mode-scope deferral (its `(A7)+`/`-(A7)`
///   siblings ARE in scope); odd word/long EAs are address errors the E3/E4 abort covers (no parity filter).
/// - **`Dn,<ea>`** (opmode 4/5/6 = b/w/l): alterable-memory dest only (`(An)` 2, `(An)+` 3, `-(An)` 4,
///   `d16(An)` 5, `d8(An,Xn)` 6, `abs.w` 7/0, `abs.l` 7/1). **Mode 000/001 = ABCD/EXG** (a DIFFERENT
///   instruction) is reserved/excluded. The `(A7)` (mode 2) plain-indirect dest follows the same deferral.
/// - **opmode 3/7** = MULU/MULS (not AND/OR) → not covered.
fn and_or_in_scope(opcode: u16) -> bool {
    let high = opcode >> 12;
    if high != 0xC && high != 0x8 {
        return false; // not the genuine AND/OR register form (e.g. ANDI/ORI group-0 immediate opcode)
    }
    let mode = (opcode >> 3) & 7;
    let reg = opcode & 7;
    match (opcode >> 6) & 7 {
        // <ea>,Dn (opmode 0/1/2 = b/w/l): An-direct (mode 1) ILLEGAL; data modes only.
        0..=2 => match mode {
            0 => true,     // Dn-direct (no memory access)
            1 => false,    // An-direct illegal/absent (AND An,Dn does not exist)
            2 => reg != 7, // (An) — A7 mode-2 deferred
            3 | 4 => true, // (An)+ / -(An)
            5 | 6 => true, // d16(An) / d8(An,Xn)
            7 => reg <= 4, // abs.w / abs.l / d16(PC) / d8(PC,Xn) / #imm
            _ => false,
        },
        // Dn,<ea> (opmode 4/5/6 = b/w/l): alterable-memory dest only; mode 000/001 = ABCD/EXG reserved.
        4..=6 => match mode {
            2 => reg != 7,                     // (An) — A7 mode-2 deferred
            3 | 4 => true,                     // (An)+ / -(An)
            5 | 6 => true,                     // d16(An) / d8(An,Xn)
            7 if reg == 0 || reg == 1 => true, // abs.w / abs.l
            _ => false,                        // mode 0/1 = ABCD/EXG; mode 7 reg>=2 not alterable
        },
        // opmode 3/7 = MULU/MULS — a different instruction, not AND/OR.
        _ => false,
    }
}

/// Whether the framework covers this genuine register-form `EOR` case — the load-bearing classifier for the
/// **CONTAMINATED** `EOR.*` files (each mixes the genuine register opcode, high nibble `0xB`, with the dedicated
/// `EORI` immediate opcode in the group-0 space, `0x0Axx`, high nibble 0). Classifying **by OPCODE** (high
/// nibble `0xB`), this admits ONLY the genuine register form so the `EORI` cases — a DIFFERENT instruction not
/// implemented this push — are skipped cleanly (never decoded, never reaching the `todo!()`). `false` for any
/// non-`0xB` opcode (so a group-0 `EORI` falls through to the rest of `covered()`, which also rejects it).
///
/// EOR exists ONLY in the `Dn,<ea>` direction (opmode 4/5/6 = b/w/l). The destination is either a **data
/// register** (mode 000 = `Dn,Dn`) or **alterable memory** (`(An)` 2, `(An)+` 3, `-(An)` 4, `d16(An)` 5,
/// `d8(An,Xn)` 6, `abs.w` 7/0, `abs.l` 7/1). **Mode field 001 = CMPM** (a DIFFERENT instruction classified out
/// of EOR — it is a `CmpClass::Cmpm` opcode handled by `cmp_in_scope`, not here) and is excluded. The `(A7)`
/// (mode 2) plain-indirect dest is the pre-existing mode-scope deferral (its `(A7)+`/`-(A7)` siblings ARE in
/// scope); odd word/long EAs are address errors the E3/E4 abort covers (no parity filter). opmode 0/1/2 = CMP /
/// 3/7 = CMPA (not EOR) → not covered.
fn eor_in_scope(opcode: u16) -> bool {
    if opcode >> 12 != 0xB {
        return false; // not the genuine EOR register form (e.g. EORI group-0 immediate opcode)
    }
    match (opcode >> 6) & 7 {
        // Dn,<ea> (opmode 4/5/6 = b/w/l): data-register dest (mode 0) or alterable memory; mode 001 = CMPM.
        4..=6 => {
            let mode = (opcode >> 3) & 7;
            let reg = opcode & 7;
            match mode {
                0 => true,                         // Dn-direct register dest (no memory access)
                2 => reg != 7,                     // (An) — A7 mode-2 deferred
                3 | 4 => true,                     // (An)+ / -(An)
                5 | 6 => true,                     // d16(An) / d8(An,Xn)
                7 if reg == 0 || reg == 1 => true, // abs.w / abs.l
                _ => false,                        // mode 1 = CMPM; mode 7 reg>=2 not alterable
            }
        }
        // opmode 0/1/2 = CMP, opmode 3/7 = CMPA — not EOR.
        _ => false,
    }
}

/// Whether the framework covers this case (else it is an xfail for this push). `ADD`/`SUB` in word, byte and
/// long sizes, each in two forms — `Dn,<ea>` (memory dest; word ADD=0xD140/SUB=0x9140, byte ADD=0xD100/
/// SUB=0x9100, long ADD=0xD180/SUB=0x9180) and `<ea>,Dn` (register dest; word ADD=0xD040/SUB=0x9040, byte
/// ADD=0xD000/SUB=0x9000, long ADD=0xD080/SUB=0x9080). **No parity filter** — E4 made odd word/long EAs
/// coverable (the execution-time address-error abort installs the group-0 14-byte vector-3 frame, so an odd
/// access PASSES unchanged; the auto-(in/de)crement register bump is committed before the faulting read,
/// pinned to the data). The only remaining deferrals are mode-scope: `An`-direct as a byte source/dest
/// (`ADD.b An,Dn` is illegal), the older `(An)` (mode 2) `A7` form (a pre-existing non-address-error
/// mode-scope convention — its `(A7)+`/`-(A7)` siblings ARE in scope), and not-yet-implemented EA modes.
/// Whether the framework covers this `0xExxx` shift/rotate case (the EA-scope half — the 2 corrupt ASL.b
/// entries are handled separately in [`covered`], which has the final state). Classified by OPCODE:
///
/// - **Register** (bits 7-6 != 11): EVERY register shift is in scope — the operand is always `Dn` (bits 2-0),
///   the count an immediate (1-8) or a live `Dn` (mod 64), all sizes. No memory access → no faults, no
///   deferral.
/// - **Memory shift-by-1** (bits 7-6 == 11, `.w` only): the data-alterable set `(An)` (2), `(An)+` (3),
///   `-(An)` (4), `d16(An)` (5), `d8(An,Xn)` (6), `abs.w` (7/0), `abs.l` (7/1) — INCLUDING the clean `(A7)`
///   mode-2 indirect (`mode == 2 && reg == 7`, NO deferral, like CLR.w/NEG.w). Odd EAs are address errors the
///   E3/E4 abort covers (NO parity filter). `An`-direct / PC-relative / `#imm` are not data-alterable and are
///   absent.
fn shift_covered(opcode: u16) -> bool {
    if (opcode >> 6) & 3 != 3 {
        return true; // register shift — always in scope (Dn operand, imm or Dn count, all sizes)
    }
    let mode = (opcode >> 3) & 7;
    let reg = opcode & 7;
    matches!(mode, 2..=6) || (mode == 7 && (reg == 0 || reg == 1))
}

fn covered(opcode: u16, ini: &Value, fin: &Value) -> bool {
    // MOVE (`00 SS RRR MMM mmm rrr`, dst_mode != 1) — its own EA→EA mode-scope filter (no parity).
    if move_covered(opcode) {
        return move_in_scope(opcode);
    }
    // MOVEA.w / MOVEA.l (`00 SS RRR 001 mmm rrr`, dst_mode == 1) — its own source-EA mode-scope filter
    // (destination is always An, a register write). Byte MOVEA is illegal → not covered.
    if movea_size(opcode).is_some() {
        return movea_in_scope(opcode);
    }
    // Bcc / BRA (`0110 cccc dddddddd`, 0x6xxx; cc != 1) — every case in scope (not-taken fall-through and
    // both even/odd taken targets; an odd target is an address error the E4 abort covers). cc == 1 is BSR.
    if bcc_covered(opcode) {
        return true;
    }
    // DBcc (`0101 cccc 11001 rrr`, opcode & 0xF0F8 == 0x50C8) — every case in scope (fall-through and both
    // even/odd taken targets; an odd target is an address error the E4 abort covers). Only the 0x50C8
    // An-direct form is DBcc (every other mode is Scc, not implemented → not covered by `dbcc_covered`).
    if dbcc_covered(opcode) {
        return true;
    }
    // BSR (`0110 0001 dddddddd`, 0x61xx; cc == 1) — every case in scope (always-taken, both even/odd targets;
    // the 0x61FF byte form is displacement −1 → odd target pc+1, an address error the E4 abort covers).
    if bsr_covered(opcode) {
        return true;
    }
    // JMP `<control ea>` (0x4EC0 | ea) — the seven control modes, every (even/odd) target in scope (an odd
    // target is an address error the E4 abort covers).
    if jmp_covered(opcode) {
        return true;
    }
    // JSR `<control ea>` (0x4E80 | ea) — the SAME seven control modes as JMP, every (even/odd) target in
    // scope. The JSR recipe pushes a 32-bit return address (the reload splits around the push); on an odd
    // target the faulting program fetch precedes the push (SSP −14, no push), pinned to the data.
    if jsr_covered(opcode) {
        return true;
    }
    // RTS (`0x4E75`) — every popped target in scope (an odd popped 32-bit return address is an address error
    // the E4 abort covers; SSP ends −10). No EA, no flags.
    if rts_covered(opcode) {
        return true;
    }
    // RTR (`0x4E77`) — like RTS but pops a saved CCR word first; every popped target in scope (an odd popped
    // return address is an address error the E4 abort covers, stacking the CCR-restored SR; SSP ends −8).
    if rtr_covered(opcode) {
        return true;
    }
    // RTE (`0x4E73`) — return from exception: pop the 6-byte frame (SR + 32-bit PC), restore the full SR (may
    // switch S/T), pop SP by 6 while still supervisor, then reload at the popped PC. Every popped PC in scope:
    // an odd popped PC is an address error the E4 abort covers (the faulting reload runs under the RESTORED
    // mode's FC, so a returning-to-user frame stacks the user-mode SR and SSW low5=0x1A — pinned to the data).
    if rte_covered(opcode) {
        return true;
    }
    // TRAP #n (`0100 1110 0100 nnnn`, 0x4E40 | n) — the standard 6-byte exception entry. Every vendored case
    // is in scope (all supervisor, even SSP/handler, length 34); the S/T/A7 transform is structurally
    // exercised but a no-op on the data (see `trap_covered`'s caveat).
    if trap_covered(opcode) {
        return true;
    }
    // TRAPV (`0x4E76`) — trap on overflow, resolved at decode time on the V flag. Every vendored case is in
    // scope (V=0 no-trap len 4 / V=1 trap len 34, all supervisor with even SSP/handler); the trap path runs the
    // same standard 6-byte frame as TRAP but with a LEADING prefetch, and the S/T/A7 transform is structurally
    // exercised but a no-op on the data (see `trapv_covered`'s caveat).
    if trapv_covered(opcode) {
        return true;
    }
    // CHK `<ea>,Dn` (0100 ddd 110 mmm rrr, opcode & 0xF1C0 == 0x4180) — bounds-check trap to vector 6. Every
    // case is in scope across all 11 source modes (no-trap, Dn<0 / Dn>bound trap; odd source EAs are address
    // errors the E3/E4 abort covers). `An`-direct is illegal for CHK (never appears); no `(A7)` mode-2 deferral.
    if chk_covered(opcode) {
        return true;
    }
    // ANDItoSR / ORItoSR / EORItoSR (0x027C / 0x007C / 0x0A7C) — the privileged immediate-to-SR logic ops.
    // Every vendored case is supervisor (the legal SR-modifying path, all length 20); the mid-instruction FC
    // switch (S cleared → the two re-prefetch reads run under FC2 instead of FC6) IS gate-exercised, while the
    // user-mode privilege-violation entry is correctness-only (see `to_sr_covered`'s caveat).
    if to_sr_covered(opcode) {
        return true;
    }
    // RESET (0x4E70) — assert the reset line for 124 cycles (length 132: n4 + n124 + one queue refill). Every
    // vendored case is supervisor; the user-mode privilege-violation entry is correctness-only (not gated).
    if reset_covered(opcode) {
        return true;
    }
    // CMP `<ea>,Dn` + CMPM `(Ay)+,(Ax)+` + CMPI `#imm,<ea>` (the Cmp/Cmpm/Cmpi classes of the 3-way CMP.* mix,
    // classified by OPCODE). N0 added Cmp, N1 Cmpm, N2 Cmpi — so the CMP.* files are now FULLY covered (CMPA is
    // its own file/decode arm). Odd word/long EAs are address errors the E3/E4 abort covers; the per-class
    // scope is in `cmp_in_scope`.
    if matches!(
        cmp_class(opcode),
        CmpClass::Cmp | CmpClass::Cmpm | CmpClass::Cmpi
    ) {
        return cmp_in_scope(opcode);
    }
    // CMPA `<ea>,An` (`1011 aaa 0 11/111 mmm rrr`, opmode 3/7 = .w/.l) — its own CMPA.w/.l files / decode arm.
    // All 12 source modes in scope (An-direct legal; odd word/long source EAs are address errors the E3/E4
    // abort covers — no parity filter) except the pre-existing `(A7)` (mode 2) plain-indirect deferral (its
    // `(A7)+`/`-(A7)` siblings ARE in scope). Classified by OPCODE via `cmp_class`.
    if matches!(cmp_class(opcode), CmpClass::Cmpa) {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return !(mode == 2 && reg == 7);
    }
    // TST `<ea>` (`0100 1010 SS mmm rrr`, 0x4A00/4A40/4A80, SS != 3) — the flag-only test `<ea> − 0`. The
    // data-alterable EA set is in scope: Dn (0), (An) (2), (An)+ (3), -(An) (4), d16(An) (5), d8(An,Xn) (6),
    // abs.w (7/0), abs.l (7/1). An-direct (1) / PC-relative (7/2, 7/3) / #imm (7/4) are NOT data-alterable and
    // are absent from the data. NO `(A7)` mode-2 deferral — TST never writes, so the plain `(A7)` indirect read
    // is clean; odd word/long EAs are address errors the E3/E4 abort covers (no parity filter). SS == 3 is TAS.
    if opcode & 0xFF00 == 0x4A00 && opcode & 0xC0 != 0xC0 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return match mode {
            0 => true,                         // Dn-direct (no memory access)
            2..=4 => true,                     // (An) / (An)+ / -(An)
            5 | 6 => true,                     // d16(An) / d8(An,Xn)
            7 if reg == 0 || reg == 1 => true, // abs.w / abs.l
            _ => false, // An-direct / PC-rel / #imm: absent / not data-alterable
        };
    }
    // CLR `<ea>` (`0100 0010 SS mmm rrr`, 0x4200/4240/4280, SS != 3) — clear the data-alterable EA to 0
    // (Z=1/N=0/V=0/C=0, X PRESERVED). The data-alterable EA set is in scope: Dn (0), (An) (2), (An)+ (3),
    // -(An) (4), d16(An) (5), d8(An,Xn) (6), abs.w (7/0), abs.l (7/1). An-direct (1) / PC-relative (7/2, 7/3) /
    // #imm (7/4) are NOT data-alterable and are absent from the data. CLR is a READ-then-WRITE (it reuses the
    // `ea_dst`/`ea_dst_long` RMW path), so an odd word/long EA address-errors on the READ (low5 = 0x15), the
    // E3/E4 abort covers it (no parity filter). NO `(A7)` mode-2 deferral — the plain `(A7)` indirect RMW is
    // clean (the read/write both hit the active A7). SS == 3 (0x42C0) is illegal on the 68000 (not CLR).
    if opcode & 0xFF00 == 0x4200 && opcode & 0xC0 != 0xC0 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return match mode {
            0 => true,                         // Dn-direct (no memory access)
            2..=4 => true,                     // (An) / (An)+ / -(An)
            5 | 6 => true,                     // d16(An) / d8(An,Xn)
            7 if reg == 0 || reg == 1 => true, // abs.w / abs.l
            _ => false, // An-direct / PC-rel / #imm: absent / not data-alterable
        };
    }
    // NEG `<ea>` (`0100 0100 SS mmm rrr`, 0x4400/4440/4480, SS != 3) — negate the data-alterable EA (`res =
    // 0 − d`, full subtract flags). The data-alterable EA set is in scope: Dn (0), (An) (2 — minus the `(A7)`
    // mode-2 deferral), (An)+ (3), -(An) (4), d16(An) (5), d8(An,Xn) (6), abs.w (7/0), abs.l (7/1). An-direct (1) /
    // PC-relative (7/2, 7/3) / #imm (7/4) are NOT data-alterable and are absent. NEG is a READ-then-WRITE (it
    // reuses the `ea_dst`/`ea_dst_long` RMW path), so an odd word/long EA address-errors on the READ (low5 =
    // 0x15), the E3/E4 abort covers it (no parity filter). The ONE intra-family deferral is the plain `(A7)`
    // mode-2 indirect (`mode == 2 && reg == 7`), the pre-existing precedent-consistent residual — its `(A7)+` /
    // `-(A7)` siblings ARE in scope. SS == 3 (0x44C0) is MOVE-to-CCR, not NEG.
    if opcode & 0xFF00 == 0x4400 && opcode & 0xC0 != 0xC0 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return match mode {
            0 => true,                         // Dn-direct (no memory access)
            2 => reg != 7, // (An) — A7 mode-2 deferred (plain-indirect residual)
            3 | 4 => true, // (An)+ / -(An)
            5 | 6 => true, // d16(An) / d8(An,Xn)
            7 if reg == 0 || reg == 1 => true, // abs.w / abs.l
            _ => false,    // An-direct / PC-rel / #imm: absent / not data-alterable
        };
    }
    // NEGX `<ea>` (`0100 0000 SS mmm rrr`, 0x4000/4040/4080, SS != 3) — negate-with-extend the data-alterable EA
    // (`res = 0 − d − X_in`, sticky Z + X-in borrow). IDENTICAL EA scope to NEG (it reuses `neg_family_recipe`):
    // the data-alterable EA set is in scope — Dn (0), (An) (2 — minus the `(A7)` mode-2 deferral), (An)+ (3),
    // -(An) (4), d16(An) (5), d8(An,Xn) (6), abs.w (7/0), abs.l (7/1). An-direct (1) / PC-relative (7/2, 7/3) /
    // #imm (7/4) are NOT data-alterable and are absent. NEGX is a READ-then-WRITE, so an odd word/long EA
    // address-errors on the READ (low5 = 0x15), the E3/E4 abort covers it (no parity filter). The ONE
    // intra-family deferral is the plain `(A7)` mode-2 indirect (`mode == 2 && reg == 7`), the pre-existing
    // residual — its `(A7)+` / `-(A7)` siblings ARE in scope. SS == 3 (0x40C0) is MOVE-from-SR, not NEGX.
    if opcode & 0xFF00 == 0x4000 && opcode & 0xC0 != 0xC0 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return match mode {
            0 => true,                         // Dn-direct (no memory access)
            2 => reg != 7, // (An) — A7 mode-2 deferred (plain-indirect residual)
            3 | 4 => true, // (An)+ / -(An)
            5 | 6 => true, // d16(An) / d8(An,Xn)
            7 if reg == 0 || reg == 1 => true, // abs.w / abs.l
            _ => false,    // An-direct / PC-rel / #imm: absent / not data-alterable
        };
    }
    // NOT `<ea>` (`0100 0110 SS mmm rrr`, 0x4600/4640/4680, SS != 3) — bitwise-complement the data-alterable EA
    // (`res = ~d`, LOGIC flags: N = msb / Z = (res == 0), V = 0, C = 0, X PRESERVED). IDENTICAL EA scope to
    // NEG/NEGX (it reuses `neg_family_recipe`): the data-alterable EA set is in scope — Dn (0), (An) (2 — minus
    // the `(A7)` mode-2 deferral), (An)+ (3), -(An) (4), d16(An) (5), d8(An,Xn) (6), abs.w (7/0), abs.l (7/1).
    // An-direct (1) / PC-relative (7/2, 7/3) / #imm (7/4) are NOT data-alterable and are absent. NOT is a
    // READ-then-WRITE, so an odd word/long EA address-errors on the READ (low5 = 0x15), the E3/E4 abort covers it
    // (no parity filter). The ONE intra-family deferral is the plain `(A7)` mode-2 indirect (`mode == 2 && reg ==
    // 7`), the pre-existing residual — its `(A7)+` / `-(A7)` siblings ARE in scope. SS == 3 (0x46C0) is
    // MOVE-to-SR, not NOT.
    if opcode & 0xFF00 == 0x4600 && opcode & 0xC0 != 0xC0 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return match mode {
            0 => true,                         // Dn-direct (no memory access)
            2 => reg != 7, // (An) — A7 mode-2 deferred (plain-indirect residual)
            3 | 4 => true, // (An)+ / -(An)
            5 | 6 => true, // d16(An) / d8(An,Xn)
            7 if reg == 0 || reg == 1 => true, // abs.w / abs.l
            _ => false,    // An-direct / PC-rel / #imm: absent / not data-alterable
        };
    }
    // EXT.w / EXT.l (`0100 1000 1S 000 rrr`, 0x4880 .w / 0x48C0 .l) + SWAP (`0100 1000 01 000 rrr`, 0x4840) —
    // mask `opcode & 0xFFF8` (mode FIXED 000 = `Dn`; the low 3 bits the register). EVERY case is `Dn`-direct, so
    // there is NO deferral (no memory → no `(A7)` mode-2 form, no odd-EA fault): the whole opcode space of each
    // file is in scope. The `0xFFF8` mask isolates the `Dn` encodings from the PEA/MOVEM neighbours in 0x48xx
    // (mode ≥ 2, reserved this push) — `0xFFC0` would wrongly admit them. The files are 100% pure (no
    // contaminants). Classified by OPCODE (the `0xFFF8` mask match).
    if matches!(opcode & 0xFFF8, 0x4880 | 0x48C0 | 0x4840) {
        return true;
    }
    // Scc `<ea>` (`0101 cccc 11 mmm rrr`, opcode & 0xF0C0 == 0x50C0) — the conditional byte set (0xFF if cc
    // TRUE else 0x00, NO flags). Classified by OPCODE. The 0x50C8 mode-001 DBcc form is consumed by
    // `dbcc_covered` ABOVE (which returns true and so never reaches here); the data-alterable match below also
    // excludes mode 1. The data-alterable EA set is FULLY in scope: Dn (0), (An) (2 — incl the clean `(A7)`
    // mode-2 indirect, NO deferral: Scc = CLR's exact byte RMW), (An)+ (3), -(An) (4), d16(An) (5), d8(An,Xn)
    // (6), abs.w (7/0), abs.l (7/1). An-direct (1) is DBcc / not data-alterable; PC-relative (7/2, 7/3) / #imm
    // (7/4) are not data-alterable and are absent. Byte-only → NO odd-EA address-error faults (no parity
    // filter). The Scc.json file is 100% PURE (one opcode, no contaminant).
    if opcode & 0xF0C0 == 0x50C0 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return match mode {
            0 => true,                         // Dn-direct (no memory access)
            2..=4 => true,                     // (An) [incl (A7) m2] / (An)+ / -(An)
            5 | 6 => true,                     // d16(An) / d8(An,Xn)
            7 if reg == 0 || reg == 1 => true, // abs.w / abs.l
            _ => false, // An-direct (DBcc, handled above) / PC-rel / #imm: absent / not data-alterable
        };
    }
    // TAS `<ea>` (`0100 1010 11 mmm rrr`, opcode & 0xFFC0 == 0x4AC0) — the indivisible test-and-set. The
    // FULL data-alterable EA set is in scope (BOTH the `Dn` register form AND the atomic-RMW memory forms):
    // Dn (0), (An) (2 — INCL the clean `(A7)` mode-2 indirect, NO deferral: the atomic `[t@A7, prefetch]`),
    // (An)+ (3), -(An) (4), d16(An) (5), d8(An,Xn) (6), abs.w (7/0), abs.l (7/1). An-direct (1) / PC-rel
    // (7/2, 7/3) / #imm (7/4) are not data-alterable and are absent. Classified by OPCODE. The 0x4AC0 space
    // is the SS == 3 (`& 0xC0 == 0xC0`) sub-case of the 0x4A00 TST space (excluded from the TST arm above via
    // `& 0xC0 != 0xC0`). Byte-only → NO odd-EA faults (no parity filter). The TAS.json file is 100% PURE (one
    // opcode): Dn (mode 0) = 1237 + memory = 6828 = 8065 (the WHOLE file in scope — no deferral).
    if opcode & 0xFFC0 == 0x4AC0 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return match mode {
            0 => true,                         // Dn-direct (no memory access)
            2..=4 => true,                     // (An) [incl (A7) m2] / (An)+ / -(An)
            5 | 6 => true,                     // d16(An) / d8(An,Xn)
            7 if reg == 0 || reg == 1 => true, // abs.w / abs.l
            _ => false, // An-direct / PC-rel / #imm: absent / not data-alterable
        };
    }
    // BTST `<ea>` — test a single bit, Z = NOT(bit), READ-ONLY (X/N/V/C + the SR system byte preserved).
    // Classified by OPCODE: the DYNAMIC form (`0000 ddd 1 00 mmm rrr`, mask `0xF1C0 == 0x0100`, tt bits 7-6 ==
    // 00) admits the FULL read-only source set — Dn (0), (An)/(An)+/-(An)/d16(An)/d8(An,Xn) (2-6), abs.w/abs.l/
    // d16(PC)/d8(PC,Xn)/#imm (7/0..7/4); the STATIC form (`0000 1000 00 mmm rrr`, mask `0xFF00 == 0x0800`, tt ==
    // 00) admits the same set MINUS #imm (7/0..7/3 — #imm is not a static operand and is absent). An-direct
    // (mode 1 = MOVEP) is absent. The plain `(A7)` mode-2 indirect is COVERED (a clean byte read, like CLR/TST —
    // NO deferral, NO `reg != 7` carve-out). Byte memory → NO odd-EA address-error faults (no parity filter).
    // The opcode spaces 0x01xx (dynamic, bit 8 set) and 0x08xx (static) are disjoint from CMPI (0x0Cxx, bit 8
    // clear) and the `*toSR` single points (bit 8 clear). The BTST.json file is 100% PURE (one op-type): dynamic
    // 7185 + static 880 = 8065 (the WHOLE file in scope).
    if opcode & 0xF1C0 == 0x0100 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return mode == 0 || (2..=6).contains(&mode) || (mode == 7 && reg <= 4);
    }
    if opcode & 0xFF00 == 0x0800 && (opcode >> 6) & 3 == 0 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return mode == 0 || (2..=6).contains(&mode) || (mode == 7 && reg <= 3);
    }
    // BCHG `<ea>` — test then TOGGLE a single bit, Z = NOT(the PRE-modify bit), a read-modify-WRITE (X/N/V/C +
    // the SR system byte preserved). Classified by OPCODE (`tt` bits 7-6 == 01): the DYNAMIC form (mask
    // `0xF1C0 == 0x0140`) and the STATIC form (mask `0xFF00 == 0x0800`, tt == 01) BOTH admit the FULL in-scope
    // EA set — Dn (0) + data-alterable memory (An)/(An)+/-(An)/d16(An)/d8(An,Xn) (2-6), abs.w/abs.l (7/0, 7/1).
    // `An`-direct (mode 1 = MOVEP) / PC-relative / `#imm` are NOT alterable and absent. The plain `(A7)` mode-2
    // indirect is COVERED (a clean byte RMW, like CLR/TST — NO deferral, NO `reg != 7` carve-out). Byte memory
    // → NO odd-EA address-error faults (no parity filter). The 0x01xx/0x08xx spaces are disjoint from CMPI
    // (0x0Cxx) and the BTST forms (tt == 00). The BCHG.json file is 100% PURE: dynamic 7173 + static 892 = 8065.
    if opcode & 0xF1C0 == 0x0140 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return mode == 0 || (2..=6).contains(&mode) || (mode == 7 && (reg == 0 || reg == 1));
    }
    if opcode & 0xFF00 == 0x0800 && (opcode >> 6) & 3 == 1 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return mode == 0 || (2..=6).contains(&mode) || (mode == 7 && (reg == 0 || reg == 1));
    }
    // BCLR `<ea>` — test then CLEAR a single bit, Z = NOT(the PRE-clear bit), a read-modify-WRITE (X/N/V/C + the
    // SR system byte preserved). Classified by OPCODE (`tt` bits 7-6 == 10): the DYNAMIC form (mask
    // `0xF1C0 == 0x0180`) and the STATIC form (mask `0xFF00 == 0x0800`, tt == 10) BOTH admit the FULL in-scope
    // EA set — Dn (0) + data-alterable memory (An)/(An)+/-(An)/d16(An)/d8(An,Xn) (2-6), abs.w/abs.l (7/0, 7/1).
    // `An`-direct (mode 1 = MOVEP) / PC-relative / `#imm` are NOT alterable and absent. The plain `(A7)` mode-2
    // indirect is COVERED (a clean byte RMW, like CLR/TST — NO deferral, NO `reg != 7` carve-out). Byte memory
    // → NO odd-EA address-error faults (no parity filter). The 0x01xx/0x08xx spaces are disjoint from CMPI
    // (0x0Cxx) and the BTST/BCHG forms (tt == 00/01). The BCLR.json file is 100% PURE: dynamic 7166 + static
    // 899 = 8065 (the WHOLE file in scope).
    if opcode & 0xF1C0 == 0x0180 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return mode == 0 || (2..=6).contains(&mode) || (mode == 7 && (reg == 0 || reg == 1));
    }
    if opcode & 0xFF00 == 0x0800 && (opcode >> 6) & 3 == 2 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return mode == 0 || (2..=6).contains(&mode) || (mode == 7 && (reg == 0 || reg == 1));
    }
    // BSET `<ea>` — test then SET a single bit, Z = NOT(the PRE-set bit), a read-modify-WRITE (X/N/V/C + the SR
    // system byte preserved). Classified by OPCODE (`tt` bits 7-6 == 11): the DYNAMIC form (mask
    // `0xF1C0 == 0x01C0`) and the STATIC form (mask `0xFF00 == 0x0800`, tt == 11) BOTH admit the FULL in-scope
    // EA set — Dn (0) + data-alterable memory (An)/(An)+/-(An)/d16(An)/d8(An,Xn) (2-6), abs.w/abs.l (7/0, 7/1).
    // `An`-direct (mode 1 = MOVEP) / PC-relative / `#imm` are NOT alterable and absent. The plain `(A7)` mode-2
    // indirect is COVERED (a clean byte RMW, like CLR/TST — NO deferral, NO `reg != 7` carve-out). Byte memory
    // → NO odd-EA address-error faults (no parity filter). The 0x01xx/0x08xx spaces are disjoint from CMPI
    // (0x0Cxx) and the BTST/BCHG/BCLR forms (tt == 00/01/10). The BSET.json file is 100% PURE: dynamic 7099 +
    // static 966 = 8065 (the WHOLE file in scope).
    if opcode & 0xF1C0 == 0x01C0 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return mode == 0 || (2..=6).contains(&mode) || (mode == 7 && (reg == 0 || reg == 1));
    }
    if opcode & 0xFF00 == 0x0800 && (opcode >> 6) & 3 == 3 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return mode == 0 || (2..=6).contains(&mode) || (mode == 7 && (reg == 0 || reg == 1));
    }
    // ADD/SUB. No parity filter (odd word/long EAs are address errors the E4 abort covers); the only
    // mode-scope deferrals are the `(A7)` (mode 2) plain-indirect form (`reg != 7`) and the illegal `An`-direct
    // byte source (mode 1). `mode` 3/4 are `(An)+`/`-(An)` (the auto-(in/de)crement bump is committed before the
    // faulting access, matching the data); 5/6 are `d16(An)`/`d8(An,Xn)`; 7/reg the abs / PC-relative / #imm.
    // <op>.w Dn,<ea> — word memory destination.
    if opcode & 0xF1C0 == 0xD140 || opcode & 0xF1C0 == 0x9140 {
        let mode = (opcode >> 3) & 7;
        let reg = (opcode & 7) as usize;
        return match mode {
            2 => reg != 7,                     // (An) — A7 mode-2 deferred
            3 | 4 => true,                     // (An)+ / -(An)
            5 | 6 => true,                     // d16(An) / d8(An,Xn)
            7 if reg == 0 || reg == 1 => true, // abs.w / abs.l
            _ => false,                        // other alterable-memory dest modes: out of slice
        };
    }
    // <op>.w <ea>,Dn — word register destination.
    if opcode & 0xF1C0 == 0xD040 || opcode & 0xF1C0 == 0x9040 {
        let mode = (opcode >> 3) & 7;
        let reg = (opcode & 7) as usize;
        return match mode {
            0 | 1 => true,         // Dn / An direct (no memory access; A7 source legal)
            2 => reg != 7,         // (An) — A7 mode-2 deferred
            3 | 4 => true,         // (An)+ / -(An)
            5 | 6 => true,         // d16(An) / d8(An,Xn)
            7 if reg <= 4 => true, // abs.w / abs.l / d16(PC) / d8(PC,Xn) / #imm
            _ => false,
        };
    }
    // <op>.b Dn,<ea> — byte memory destination (no odd-address error for byte; `An`-direct is not a dest).
    if opcode & 0xF1C0 == 0xD100 || opcode & 0xF1C0 == 0x9100 {
        let mode = (opcode >> 3) & 7;
        let reg = (opcode & 7) as usize;
        return match mode {
            2 => reg != 7,                     // (An) — A7 mode-2 deferred
            3 | 4 => true,                     // (An)+ / -(An) (byte step 1, or 2 for A7)
            5 | 6 => true,                     // d16(An) / d8(An,Xn)
            7 if reg == 0 || reg == 1 => true, // abs.w / abs.l
            _ => false,
        };
    }
    // <op>.b <ea>,Dn — byte register destination. `An`-direct (mode 1) is EXCLUDED (`ADD.b An,Dn` illegal).
    if opcode & 0xF1C0 == 0xD000 || opcode & 0xF1C0 == 0x9000 {
        let mode = (opcode >> 3) & 7;
        let reg = (opcode & 7) as usize;
        return match mode {
            0 => true,             // Dn direct
            1 => false,            // An-direct illegal for byte
            2 => reg != 7,         // (An) — A7 mode-2 deferred
            3 | 4 => true,         // (An)+ / -(An)
            5 | 6 => true,         // d16(An) / d8(An,Xn)
            7 if reg <= 4 => true, // abs.w / abs.l / d16(PC) / d8(PC,Xn) / #imm
            _ => false,
        };
    }
    // <op>.l Dn,<ea> — long memory destination (`An`-direct is not a dest).
    if opcode & 0xF1C0 == 0xD180 || opcode & 0xF1C0 == 0x9180 {
        let mode = (opcode >> 3) & 7;
        let reg = (opcode & 7) as usize;
        return match mode {
            2 => reg != 7,                     // (An) — A7 mode-2 deferred
            3 | 4 => true,                     // (An)+ / -(An)
            5 | 6 => true,                     // d16(An) / d8(An,Xn)
            7 if reg == 0 || reg == 1 => true, // abs.w / abs.l
            _ => false,
        };
    }
    // <op>.l <ea>,Dn — long register destination. `An`-direct (mode 1) is LEGAL for long (`ADD.l An,Dn`).
    if opcode & 0xF1C0 == 0xD080 || opcode & 0xF1C0 == 0x9080 {
        let mode = (opcode >> 3) & 7;
        let reg = (opcode & 7) as usize;
        return match mode {
            0 | 1 => true,         // Dn / An direct (no memory access)
            2 => reg != 7,         // (An) — A7 mode-2 deferred
            3 | 4 => true,         // (An)+ / -(An)
            5 | 6 => true,         // d16(An) / d8(An,Xn)
            7 if reg <= 4 => true, // abs.w / abs.l / d16(PC) / d8(PC,Xn) / #imm.l
            _ => false,
        };
    }
    // MOVEQ (`0111 ddd 0 dddddddd`, 0x7000 | dn<<9 | imm8, bit 8 = 0) — load a sign-extended 8-bit immediate
    // into the FULL 32 bits of Dn (N = msb / Z = value == 0, V/C cleared, X PRESERVED). The whole opcode space
    // (bit 8 = 0) is in scope: there are no EA modes (the immediate is the opcode's own low byte), no memory
    // access (a single FC-6 queue refill, length 4), and no odd-address sub-cases. Bit 8 set is illegal on the
    // 68000 and absent from the data.
    if opcode & 0xF100 == 0x7000 {
        return true;
    }
    // ADDA `<ea>,An` (`1101 aaa s11 mmm rrr`, opmode 3 = .w (0xD0C0) / 7 = .l (0xD1C0)) — `An = An + src`, NO
    // flags. All 12 source modes in scope (An-direct LEGAL — it is address arithmetic; odd word/long source EAs
    // are address errors the E3/E4 abort covers, no parity filter) except the pre-existing `(A7)` (mode 2)
    // plain-indirect deferral. The ADDA.w/.l files are 100% pure (no contaminants). Classified by OPCODE.
    if opcode & 0xF1C0 == 0xD0C0 || opcode & 0xF1C0 == 0xD1C0 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return match mode {
            0 | 1 => true,         // Dn / An direct (no memory access; An source legal)
            2 => reg != 7,         // (An) — A7 mode-2 deferred
            3 | 4 => true,         // (An)+ / -(An)
            5 | 6 => true,         // d16(An) / d8(An,Xn)
            7 if reg <= 4 => true, // abs.w / abs.l / d16(PC) / d8(PC,Xn) / #imm
            _ => false,
        };
    }
    // SUBA `<ea>,An` (`1001 aaa s11 mmm rrr`, opmode 3 = .w (0x90C0) / 7 = .l (0x91C0)) — `An = An − src`, NO
    // flags, a near-exact mirror of ADDA. All 12 source modes in scope (An-direct LEGAL — it is address
    // arithmetic; odd word/long source EAs are address errors the E3/E4 abort covers, no parity filter) except
    // the pre-existing `(A7)` (mode 2) plain-indirect deferral. The SUBA.w/.l files are 100% pure (no
    // contaminants). Classified by OPCODE.
    if opcode & 0xF1C0 == 0x90C0 || opcode & 0xF1C0 == 0x91C0 {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        return match mode {
            0 | 1 => true,         // Dn / An direct (no memory access; An source legal)
            2 => reg != 7,         // (An) — A7 mode-2 deferred
            3 | 4 => true,         // (An)+ / -(An)
            5 | 6 => true,         // d16(An) / d8(An,Xn)
            7 if reg <= 4 => true, // abs.w / abs.l / d16(PC) / d8(PC,Xn) / #imm
            _ => false,
        };
    }
    // AND `<ea>,Dn` (0xC000/40/80) + `Dn,<ea>` (0xC100/40/80) and OR `<ea>,Dn` (0x8000/40/80) + `Dn,<ea>`
    // (0x8100/40/80) — the genuine register forms, classified by OPCODE (high nibble 0xC for AND / 0x8 for OR).
    // The AND.*/OR.* files MIX this with the ANDI/ORI immediate opcode (`0x02xx`/`0x00xx`, high nibble 0) — a
    // DIFFERENT instruction NOT decoded this push; `and_or_in_scope` returns false for it (high nibble != 0xC
    // and != 0x8), so the *I cases are skipped cleanly. Source = data modes (An-direct mode 1 ILLEGAL/absent);
    // dest = alterable memory (mode 000/001 = ABCD/EXG for AND, SBCD/PACK for OR, reserved). All in scope except
    // the pre-existing `(A7)` mode-2 plain-indirect deferral; odd word/long EAs are address errors the E3/E4
    // abort covers. The single predicate covers both families (only the base nibble differs).
    if and_or_in_scope(opcode) {
        return true;
    }
    // EOR `Dn,<ea>` (0xB100/40/80, opmode 4/5/6) — the genuine register form, classified by OPCODE (high nibble
    // 0xB). The EOR.* files MIX this with the EORI immediate opcode (`0x0Axx`, high nibble 0) — a DIFFERENT
    // instruction NOT decoded this push; `eor_in_scope` returns false for it (high nibble != 0xB), so the EORI
    // cases are skipped cleanly. Dest = data register (mode 000 = `Dn,Dn`) or alterable memory (2..6/abs.w/abs.l).
    // Mode field 001 = CMPM (a `CmpClass::Cmpm` opcode handled by the `cmp_class` block above — but the EOR.*
    // files have NO mode-001 cases). All in scope except the pre-existing `(A7)` mode-2 plain-indirect deferral;
    // odd word/long EAs are address errors the E3/E4 abort covers. There is NO `EOR <ea>,Dn` (opmode 0/1/2 = CMP).
    if eor_in_scope(opcode) {
        return true;
    }
    // ASL `<ea>` (`0xExxx`, AS/left) — the foundational shift/rotate op (S0). `shift_covered` admits every
    // register shift and the data-alterable memory set (`.w` shift-by-1, INCL the clean `(A7)` mode-2
    // indirect — NO deferral; odd EAs are address errors the E3/E4 abort covers). The ONLY exclusion in the
    // whole shift/rotate family: `ASL.b`'s 2 PROVABLY-CORRUPT, self-contradictory entries. The opcode is a
    // register-only `ASL.b #2,D2` (0xE502, NO memory access) that CANNOT change D2's upper 24 bits, yet
    // final.d2 is full-register garbage no shift/rotate/unary/binary transform produces (a baked-in SST
    // generator bug, identical in the repo's HEAD; a correct CPU gives cdfb7ff8 / 417c7ef4). The key needs
    // BOTH the corrupt INITIAL d2 AND the corrupt FINAL d2: the same initial d2 (cdfb7fbe / 417c7e7d) ALSO
    // appears in 2 LEGIT cases whose final.d2 IS cdfb7ff8 / 417c7ef4 (a correct CPU matches them — they MUST
    // run), so keying on the initial d2 alone would wrongly skip 2 passing cases (ASL.b 8061, not 8063).
    // The final-d2 leg isolates EXACTLY the 2 corrupt → ASL.b in scope = 8063 (the only file not 8065).
    if opcode >> 12 == 0xE {
        if opcode == 0xE502
            && matches!(u32f(ini, "d2"), 0xcdfb_7fbe | 0x417c_7e7d)
            && matches!(u32f(fin, "d2"), 0x2e5e_4304 | 0x6461_d390)
        {
            return false;
        }
        return shift_covered(opcode);
    }
    false // other forms (not-yet-implemented modes): out of slice this push
}

/// Run one covered case through both drivers, asserting they match the suite and each other.
fn run_case(t: &Value) {
    let ini = &t["initial"];
    let length = t["length"].as_u64().unwrap() as u32;
    let expected = expected_transactions(t);

    // Driver 1 — run-to-completion (the default fast path).
    let mut cpu = Cpu68000::new(build_regs(ini));
    let mut bus = build_bus(ini);
    let cycles = cpu.run_instruction(&mut bus);
    assert_eq!(cycles, length, "cycle count [{}]", t["name"]);
    assert_final(t, &cpu.regs, &bus);
    assert_eq!(bus.log, expected, "transactions [{}]", t["name"]);

    // Driver 2 — step-one-micro-op (the quiesce path); must agree with the suite and driver 1.
    let mut cpu_step = Cpu68000::new(build_regs(ini));
    let mut bus_step = build_bus(ini);
    cpu_step.start_instruction();
    let cycles_step = loop {
        if let Step::Done(c) = cpu_step.step_micro_op(&mut bus_step) {
            break c;
        }
    };
    assert_eq!(
        cycles_step, cycles,
        "step-driver cycle count [{}]",
        t["name"]
    );
    assert_eq!(
        cpu_step.regs, cpu.regs,
        "step-driver final regs [{}]",
        t["name"]
    );
    assert_eq!(
        bus_step.log, bus.log,
        "step-driver transactions [{}]",
        t["name"]
    );
}

#[test]
fn add_sub_match_singlesteptests() {
    let mut ran = 0usize;
    for fname in FILES {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();

        let mut file_ran = 0usize;
        for t in &data {
            let ini = &t["initial"];
            let opcode = ini["prefetch"][0].as_u64().unwrap() as u16;
            if !covered(opcode, ini, &t["final"]) {
                continue;
            }
            run_case(t);
            file_ran += 1;
        }
        // S0 — the ONLY documented exclusion in the whole shift/rotate family: `ASL.b`'s 2 PROVABLY-CORRUPT,
        // self-contradictory entries (opcode 0xE502, a register-only `ASL.b #2,D2` with full-register-garbage
        // final.d2 no shift can produce). Independently re-count the corrupt pair (the same precise key
        // `covered()` uses — opcode + the corrupt INITIAL d2 + the corrupt FINAL d2) and ASSERT the exclusion
        // removes EXACTLY 2 (the initial-d2-only key would wrongly catch 2 LEGIT cases sharing that initial
        // d2) and that `ASL.b` runs EXACTLY 8063 (8065 − 2). NO broadening — exactly 2, the only carve-out.
        if *fname == "ASL.b.json" {
            let corrupt = data
                .iter()
                .filter(|t| {
                    t["initial"]["prefetch"][0].as_u64().unwrap() as u16 == 0xE502
                        && matches!(u32f(&t["initial"], "d2"), 0xcdfb_7fbe | 0x417c_7e7d)
                        && matches!(u32f(&t["final"], "d2"), 0x2e5e_4304 | 0x6461_d390)
                })
                .count();
            assert_eq!(
                corrupt, 2,
                "ASL.b corrupt exclusion must isolate EXACTLY 2 self-contradictory entries (no broadening)"
            );
            assert_eq!(
                file_ran, 8063,
                "ASL.b must run EXACTLY 8063 covered cases (8065 - 2 corrupt)"
            );
        }
        eprintln!("  {fname}: {file_ran} covered cases passed");
        ran += file_ran;
    }

    assert!(
        ran >= 720_263,
        "expected 720263 covered cases — S7 adds ROXR.b / ROXR.w / ROXR.l (`0xExxx`, ROX/right): ROXR.b 8065 + \
         ROXR.w 8065 + ROXR.l 8065 = +24195 over S6's 696068 (NO corrupt entries — only ASL.b has the 2). This \
         is the FINAL shift/rotate commit (all eight ASL/ASR/LSL/LSR/ROL/ROR/ROXL/ROXR ops now loaded). \
         ROXR is rotate RIGHT THROUGH X — ROXL's right-direction twin. It treats the X:operand pair as an \
         `(n+1)`-bit register (X above the msb) and rotates it right by `cnt % (n+1)`; the final bit ejected \
         into X is BOTH \
         the new X and C, so the result depends on the INCOMING X (unlike ROL/ROR, which leave X untouched, or \
         ASL/ASR/LSL/LSR, which set X = C from the value). It reuses `Operand::ShiftCount` + the shared \
         `shift_recipe` + `dn_*` VERBATIM (only the AluOp + the ROX/right decode arm differ — direction bit 8 == \
         0, type ROX bits 4-3 (register) / 10-9 (memory) == 2). Value: `per = n + 1`, `eff = cnt % per`; `comb = \
         ((xin << n) | x)` in `per` bits, rotated right by `eff` (a wider `u64` so the `.l` 33-bit case does not \
         overflow `u32`), `res = comb & mask`. C = X = `(comb >> n) & 1` (the bit ejected into X); V = 0 always; \
         N = msb(res), Z = (res == 0). ZERO COUNT (`cnt == 0`, only the `Dn` form): value UNCHANGED, C = X (the \
         INCOMING X — NOT 0), X UNCHANGED, V = 0, N/Z from the unchanged operand. A cnt that wraps the `(n+1)` \
         PERIOD (e.g. cnt = 9 for `.b` → eff = 9 % 9 = 0) returns the value to its start. Timing identical to \
         every shift/rotate: register `.b`/`.w` = 6 + 2*cnt, `.l` = 8 + 2*cnt; memory rotate-by-1 (word): \
         (An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l 20; an odd EA address-errors on the \
         READ (the E3/E4 abort). The FULL in-scope EA set is covered (`shift_covered`): every register shift + \
         the `.w` data-alterable memory set INCL the clean `(A7)` mode-2 indirect (NO deferral, NO parity \
         filter). All three ROXR files are 100% PURE for their op+size (only ROX/right decodes the ROXR \
         opcodes). Prior baseline — S6 adds ROXL.b / ROXL.w / ROXL.l (`0xExxx`, ROX/left): ROXL.b 8065 + \
         ROXL.w 8065 + ROXL.l 8065 = +24195 over S5's 671873 (NO corrupt entries — only ASL.b has the 2). \
         ROXL is rotate LEFT THROUGH X — the FIRST X-threading rotate. It treats the X:operand pair as an \
         `(n+1)`-bit register (X above the msb) and rotates it left by `cnt % (n+1)`; the final bit ejected \
         into X is BOTH \
         the new X and C, so the result depends on the INCOMING X (unlike ROL/ROR, which leave X untouched, or \
         ASL/ASR/LSL/LSR, which set X = C from the value). It reuses `Operand::ShiftCount` + the shared \
         `shift_recipe` + `dn_*` VERBATIM (only the AluOp + the ROX/left decode arm differ — direction bit 8 == \
         1, type ROX bits 4-3 (register) / 10-9 (memory) == 2). Value: `per = n + 1`, `eff = cnt % per`; `comb = \
         ((xin << n) | x)` in `per` bits, rotated left by `eff` (a wider `u64` so the `.l` 33-bit case does not \
         overflow `u32`), `res = comb & mask`. C = X = `(comb >> n) & 1` (the bit ejected into X); V = 0 always; \
         N = msb(res), Z = (res == 0). ZERO COUNT (`cnt == 0`, only the `Dn` form): value UNCHANGED, C = X (the \
         INCOMING X — NOT 0), X UNCHANGED, V = 0, N/Z from the unchanged operand. A cnt that wraps the `(n+1)` \
         PERIOD (e.g. cnt = 9 for `.b` → eff = 9 % 9 = 0) returns the value to its start. Prior baseline — S5 \
         adds ROR.b / ROR.w / ROR.l (`0xExxx`, RO/right): ROR.b 8065 + \
         is rotate RIGHT — ROL's right-direction twin, a plain bit-rotate that does NOT pass through X \
         (contrast ROXR, which threads X — S7). It reuses `Operand::ShiftCount` + the shared `shift_recipe` + \
         `dn_*` VERBATIM (only the AluOp + the RO/right decode arm differ — direction bit 8 == 0, type RO bits \
         4-3 (register) / 10-9 (memory) == 3). Value: `r = cnt % n`; `res = x` when `cnt == 0 || r == 0` (a \
         whole-register rotation leaves the value unchanged), else `((x >> r) | (x << (n - r))) & mask`. C = \
         the last bit rotated out — `(x >> ((cnt - 1) % n)) & 1` for `cnt != 0`, else 0 (a zero count is the \
         ONLY way ROR clears C — a nonzero multiple of n with `r == 0` still takes C from the formula); X is \
         PRESERVED (ROL/ROR never touch X — re-inject the live X, NEVER set X = C); V = 0 always; N = \
         msb(res), Z = (res == 0). ZERO COUNT (`cnt == 0`, only the `Dn` form): value unchanged, V = 0, C = \
         0, X PRESERVED, N/Z from the unchanged operand. Timing identical to ASL/ASR/LSL/LSR/ROL: register \
         `.b`/`.w` = 6 + 2*cnt, `.l` = 8 + 2*cnt; memory rotate-by-1 (word): (An)/(An)+ 12, -(An) 14, \
         d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l 20; an odd EA address-errors on the READ (the E3/E4 \
         abort). The FULL in-scope EA set is covered (`shift_covered`): every register shift + the `.w` \
         data-alterable memory set INCL the clean `(A7)` mode-2 indirect (NO deferral, NO parity filter). All \
         three ROR files are 100% PURE for their op+size (only RO/right decodes the ROR opcodes — ROXL/ROXR \
         land S6-S7). Prior baseline — S4 adds ROL.b / ROL.w / ROL.l (`0xExxx`, RO/left): ROL.b 8065 + \
         ROL.w 8065 + ROL.l 8065 = +24195 over S3's 623483 (NO corrupt entries — only ASL.b has the 2). ROL \
         is rotate LEFT — a plain bit-rotate that does NOT pass through X (contrast ROXL, which threads X — \
         S6). It reuses `Operand::ShiftCount` + the shared `shift_recipe` + `dn_*` VERBATIM (only the AluOp + \
         the RO/left decode arm differ — direction bit 8 == 1, type RO bits 4-3 (register) / 10-9 (memory) == \
         3). Value: `r = cnt % n`; `res = x` when `cnt == 0 || r == 0` (a whole-register rotation leaves the \
         value unchanged), else `((x << r) | (x >> (n - r))) & mask`. C = the last bit rotated out — `(x >> \
         ((n - (cnt % n)) % n)) & 1` for `cnt != 0`, else 0 (a zero count is the ONLY way ROL clears C — a \
         nonzero multiple of n with `r == 0` still takes C from the formula); X is PRESERVED (ROL/ROR never \
         touch X — re-inject the live X, NEVER set X = C); V = 0 always; N = msb(res), Z = (res == 0). ZERO \
         COUNT (`cnt == 0`, only the `Dn` form): value unchanged, V = 0, C = 0, X PRESERVED, N/Z from the \
         unchanged operand. Timing identical to ASL/ASR/LSL/LSR: register `.b`/`.w` = 6 + 2*cnt, `.l` = 8 + \
         2*cnt; memory rotate-by-1 (word): (An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l \
         20; an odd EA address-errors on the READ (the E3/E4 abort). The FULL in-scope EA set is covered \
         (`shift_covered`): every register shift + the `.w` data-alterable memory set INCL the clean `(A7)` \
         mode-2 indirect (NO deferral, NO parity filter). All three ROL files are 100% PURE for their op+size \
         (only RO/left decodes the ROL opcodes — ROR/ROXL/ROXR land S5-S7). Prior baseline — S3 adds LSR.b / \
         LSR.w / LSR.l (`0xExxx`, LS/right): LSR.b 8065 + \
         LSR.w 8065 + LSR.l 8065 = +24195 over S2's 599288 (NO corrupt entries — only ASL.b has the 2). LSR \
         is logical shift RIGHT — the ZERO-FILL right shift (contrast ASR, which sign-EXTENDS). It reuses \
         `Operand::ShiftCount` + the shared `shift_recipe` + `dn_*` VERBATIM (only the AluOp + the LS/right \
         decode arm differ — direction bit 8 == 0, type LS bits 4-3 (register) / 10-9 (memory) == 1). Value: \
         `res = x >> cnt` when `cnt < n`, else 0 (an over-shift clears the register). C = the last bit shifted \
         out of the OPERAND — `bit(cnt-1)` for `1 <= cnt <= n`, else 0 (same form as ASR's carry; with no sign, \
         `cnt > n` → 0 is natural); X = C; V = 0 always; N = msb(res) — ALWAYS 0 for any `cnt >= 1` (the msb is \
         zero-filled), Z = (res == 0). ZERO COUNT (`cnt == 0`, only the `Dn` form): value unchanged, V = 0, \
         C = 0, X PRESERVED (the shift never ran), N/Z from the unchanged operand (so N CAN be 1 — NOT forced \
         to 0). Timing identical to ASL/ASR/LSL: register `.b`/`.w` = 6 + 2*cnt, `.l` = 8 + 2*cnt; memory \
         shift-by-1 (word): (An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l 20; an odd EA \
         address-errors on the READ (the E3/E4 abort). The FULL in-scope EA set is covered (`shift_covered`): \
         every register shift + the `.w` data-alterable memory set INCL the clean `(A7)` mode-2 indirect (NO \
         deferral, NO parity filter). All three LSR files are 100% PURE for their op+size (only LS/right \
         decodes the LSR opcodes — ROL/ROR/ROXL/ROXR land S4-S7). Prior baseline — S2 adds LSL.b / LSL.w / \
         LSL.l (`0xExxx`, LS/left): LSL.b 8065 + \
         LSL.w 8065 + LSL.l 8065 = +24195 over S1's 575093 (NO corrupt entries — only ASL.b has the 2). LSL \
         is logical shift LEFT — IDENTICAL to ASL's value and carry with V FORCED to 0 (a logical shift never \
         tracks the sign change; only ASL owns V). It reuses `Operand::ShiftCount` + the shared `shift_recipe` \
         + `dn_*` VERBATIM (only the AluOp + the LS/left decode arm differ — direction bit 8 == 1, type LS \
         bits 4-3 (register) / 10-9 (memory) == 1). Value: `res = (x << cnt) & mask` when `cnt < n`, else 0 \
         (an over-shift clears the register). C = the last bit shifted out of the OPERAND — `bit(n-cnt)` for \
         `1 <= cnt <= n`, else 0; X = C; V = 0 ALWAYS (the ONLY difference from ASL — LSL does NOT compute the \
         sign-changed V); N = msb(res), Z = (res == 0). ZERO COUNT (`cnt == 0`, only the `Dn` form): value \
         unchanged, V = 0, C = 0, X PRESERVED (the shift never ran). Timing identical to ASL/ASR: register \
         `.b`/`.w` = 6 + 2*cnt, `.l` = 8 + 2*cnt; memory shift-by-1 (word): (An)/(An)+ 12, -(An) 14, d16(An) \
         16, d8(An,Xn) 18, abs.w 16, abs.l 20; an odd EA address-errors on the READ (the E3/E4 abort). The \
         FULL in-scope EA set is covered (`shift_covered`): every register shift + the `.w` data-alterable \
         memory set INCL the clean `(A7)` mode-2 indirect (NO deferral, NO parity filter). All three LSL files \
         are 100% PURE for their op+size (only LS/left decodes the LSL opcodes — LSR/ROL/ROR/ROXL/ROXR land \
         S3-S7). Prior baseline — S1 adds ASR.b / ASR.w / ASR.l (`0xExxx`, AS/right): ASR.b 8065 + \
         ASR.w 8065 + ASR.l 8065 = +24195 over S0's 550898 (NO corrupt entries — only ASL.b has the 2). ASR \
         is arithmetic shift RIGHT, the sign-EXTENDING right shift; it reuses `Operand::ShiftCount` + the \
         shared `shift_recipe` + `dn_*` VERBATIM (only the AluOp + the AS/right decode arm differ — direction \
         bit 8 == 0, type AS bits 4-3 (register) / 10-9 (memory) == 0). Value: the vacated top bits are filled \
         with the operand's sign bit (`cnt >= n` → all-sign-bits: `mask` if negative else 0; `0 < cnt < n` → \
         `(x >> cnt)` OR the top `cnt` sign-fill bits). C = the last bit shifted out of the OPERAND — \
         `bit(cnt-1)` for `1 <= cnt <= n`, else 0 (THE ASR CARRY QUIRK: `cnt > n` → C = 0, NOT the sign bit — \
         even though the value still sign-extends to all-sign-bits; a naive 'last bit out = sign for \
         over-shift' rule mismatches 1642 ASR.b cases); X = C; V = 0 ALWAYS (ASR never sets V — only ASL owns \
         V); N = msb(res), Z = (res == 0). ZERO COUNT (`cnt == 0`, only the `Dn` form): value unchanged, V = \
         0, C = 0, X PRESERVED (the shift never ran). Timing identical to ASL: register `.b`/`.w` = 6 + 2*cnt, \
         `.l` = 8 + 2*cnt; memory shift-by-1 (word): (An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w \
         16, abs.l 20; an odd EA address-errors on the READ (the E3/E4 abort). The FULL in-scope EA set is \
         covered (`shift_covered`): every register shift + the `.w` data-alterable memory set INCL the clean \
         `(A7)` mode-2 indirect (NO deferral, NO parity filter). All three ASR files are 100% PURE for their \
         op+size (only AS/right decodes the ASR opcodes — LSL/LSR/ROL/ROR/ROXL/ROXR land S2-S7). \
         Prior baseline — S0 (the FOUNDATIONAL shift/rotate commit) adds ASL.b / ASL.w / ASL.l \
         (`0xExxx`, AS/left): ASL.b 8063 + ASL.w 8065 + ASL.l 8065 = +24193 over B3's 526705 (ASL.b is the \
         ONLY file not 8065 — it has 2 PROVABLY-CORRUPT entries excluded). ASL is arithmetic shift LEFT, the \
         one shift that owns the **V** flag (the sign bit changed at ANY point during the shift). Three forms, \
         classified by OPCODE: REGISTER immediate-count (`1110 ccc d ss 0 00 rrr`, bit 5 = 0, count `ccc != 0 \
         ? ccc : 8` = 1-8), REGISTER `Dn`-count (`… 1 00 rrr`, bit 5 = 1, count `D[ccc] & 63` = 0-63 read LIVE \
         at decode — `ccc == rrr` legal), MEMORY shift-by-1 (`1110 0 00 1 11 mmm rrr`, bits 7-6 == 11, WORD \
         only — the `.b`/`.l` files have NO memory form). Value `res = (x << cnt) & mask` when `cnt < n` (n = \
         8/16/32) else 0; C = the last bit shifted out (`bit(n-cnt)` for `1 <= cnt <= n`, else 0 — `cnt > n` → \
         C = 0), **X = C**; **V** (closed form, 0-mismatch verified): `cnt >= n` → `V = (x != 0)` (`x == mask` \
         shifts a 0 into the sign, so it DOES change → V=1, NOT `x != 0 && x != mask`), `cnt < n` → the top \
         `cnt+1` bits are not all-equal; **N** = msb(res), **Z** = (res == 0). ZERO COUNT (`cnt == 0`, only the \
         `Dn` form): value unchanged, V = 0, C = 0, **X PRESERVED** (the shift never ran — re-inject the live \
         X), N/Z from the unchanged operand. New vocabulary: `Operand::ShiftCount(u8)` (the decode-time literal \
         count, mirroring `Operand::Zero`/`WordStep`) + `AluOp::Asl` + the shared `shift_recipe` (modelled on \
         `bit_recipe`): the register arm is `[Prefetch, Alu {{ op, size, a: dn_src(rrr,size), b, dst: \
         dn_dest(rrr,size) }}, Internal {{ (base-4) + 2*cnt }}]` with base = 6 (`.b`/`.w`) / 8 (`.l`) → total \
         `6 + 2*cnt` / `8 + 2*cnt` (the leading Prefetch refill is the 4; the idle's `2*cnt` is the DECODE-TIME \
         count — the immediate literal, or the LIVE `D[ccc] & 63`, so the register recipe length depends on \
         `regs`, exactly like Scc's true/false n2, DBcc's counter and the bit-ops' `pos >= 16`). The count \
         operand `b` = `Operand::ShiftCount` (imm / memory literal) / `Operand::DataRegFull(ccc)` (the dynamic \
         `Dn`-count, the exec masks `& 63`). The memory arm is byte-for-byte CLR.w/NEG.w's WORD `ea_dst` RMW \
         (read the word → shift-by-1 → write the word, `b = ShiftCount(1)`, `Dest::Scratch(1)`); an odd EA \
         address-errors on the READ (low5 = 0x15, the E3/E4 abort), exactly like NEG.w/CLR.w. Register timing \
         (imm AND Dn, identical base): `.b`/`.w` = 6 + 2*cnt, `.l` = 8 + 2*cnt (up to 8 + 2*63 = 134). Memory \
         shift-by-1 (word): (An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l 20. The FULL \
         in-scope EA set is covered (`shift_covered`): every register shift (no memory → no fault, no deferral) \
         + (for `.w`) the data-alterable memory set Dn-absent — (An) (2, INCL the clean `(A7)` mode-2 indirect, \
         NO deferral, like CLR.w/NEG.w), (An)+ (3), -(An) (4), d16(An) (5), d8(An,Xn) (6), abs.w (7/0), abs.l \
         (7/1); An-direct / PC-rel / #imm are not data-alterable and are absent. Odd word EAs are address \
         errors the E3/E4 abort covers (NO parity filter). The ONLY exclusion in the whole shift/rotate family \
         is `ASL.b`'s 2 PROVABLY-CORRUPT, self-contradictory entries: opcode 0xE502 (a register-only `ASL.b \
         #2,D2` with NO memory access, so it CANNOT change D2's upper 24 bits) whose final.d2 is full-register \
         garbage no shift/rotate/unary/binary transform produces (a baked-in SST generator bug, identical in \
         the repo's HEAD; a correct CPU gives cdfb7ff8 / 417c7ef4). Keyed PRECISELY on opcode 0xE502 + the \
         corrupt INITIAL d2 (cdfb7fbe / 417c7e7d) AND the corrupt FINAL d2 (2e5e4304 / 6461d390) — the same \
         initial d2 ALSO appears in 2 LEGIT cases whose final.d2 IS cdfb7ff8 / 417c7ef4 (a correct CPU matches \
         them, they MUST run), so the final-d2 leg is REQUIRED to isolate EXACTLY the 2 corrupt (initial d2 \
         alone would wrongly skip 2 passing cases → 8061). The runner asserts the exclusion removes EXACTLY 2 \
         and ASL.b runs EXACTLY 8063 (8065 − 2). The ASL.* files are 100% PURE for their op+size (`0xExxx` is a \
         dedicated opcode space; only AS/left decodes this commit — ASR/LSL/LSR/ROL/ROR/ROXL/ROXR land S1-S7). \
         Prior baseline — B3 (the FINAL bit-op) adds BSET `<ea>` (its own BSET.json file, \
         dynamic `0000 ddd 1 11 mmm rrr` = opcode & 0xF1C0 == 0x01C0 / static `0000 1000 11 mmm rrr` = \
         opcode & 0xFF00 == 0x0800 with tt bits 7-6 == 11): BSET 8065 = +8065 over B2's 518640 (the WHOLE \
         file in scope — no contaminant, no deferral). BSET tests then SETS a single bit (`operand |= \
         1<<pos`), setting ONLY Z = NOT(the PRE-set bit); X/N/V/C AND the SR system byte are ALL PRESERVED. \
         Z is from the bit BEFORE the set (the read value), not after. The bit width follows the DEST: a \
         `Dn` dest is 32-bit (`pos = b mod 32`, `Size::Long`, the FULL 32-bit register written with one bit \
         set), a memory dest is 8-bit (`pos = b mod 8`, `Size::Byte`, the byte RMW with one bit set). \
         New vocabulary: `AluOp::Bset` (Btst + the set write `a | (1<<pos)`). It reuses the shared \
         `bit_recipe` VERBATIM, IDENTICAL to BCHG (the register base idle is `n2`, the SAME as BCHG — BSET is \
         6/8 cyc, NOT BCLR's 8/10) — `reg_base = 2`. The `Dn` dest shape is `[Prefetch, Alu, Internal(2), \
         (+Internal(2) iff DECODE-TIME pos>=16)]`; the `pos>=16` `+2` is the LOAD-BEARING subtlety (the bit \
         number is read at decode — the live `Dn` for dynamic / the captured `prefetch[1]` for static — so the \
         REGISTER recipe length depends on `regs`, exactly like Scc's true/false n2 and DBcc's counter). \
         Dynamic BSET Dn = 6 (pos<16) / 8 (pos>=16); static = 10 / 12. Memory (2-6, 7/0, 7/1, `Size::Byte`) is \
         IDENTICAL to BCHG — the NEG-family read→modify→write RMW via `ea_dst` byte (read the byte, refill, the \
         bit-set `Alu` into `Scratch(1)`, write back) — NO register `+2` (byte/mod-8 timing is FIXED per \
         mode): (An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l 20 (dynamic); static = +4. \
         The static form prepends `[EaCalc(ImmWord → scratch), Prefetch]` (capture the bitnum, then the refill) \
         and routes the EA's own ext words AFTER (mirrors `cmpi_recipe`). The FULL in-scope EA set is covered — \
         Dn (0) + data-alterable memory (2-6, 7/0, 7/1); An-direct (mode 1 = MOVEP) / PC-rel / #imm are not \
         alterable and absent. The plain `(A7)` mode-2 indirect is COVERED (a clean byte RMW, like CLR/TST — NO \
         deferral, NO `reg != 7` carve-out). Byte memory → NO odd-EA address-error faults (no parity filter). \
         Classified by OPCODE (the masks above; the 0x01xx/0x08xx spaces are disjoint from CMPI 0x0Cxx and the \
         BTST/BCHG/BCLR forms tt == 00/01/10). Per-form true counts: dynamic 7099 + static 966 = 8065 (the \
         WHOLE file in scope). The BSET.json file is 100% PURE (one op-type, no contaminant). \
         Prior baseline — B2 adds BCLR `<ea>` (its own BCLR.json file, dynamic \
         `0000 ddd 1 10 mmm rrr` = opcode & 0xF1C0 == 0x0180 / static `0000 1000 10 mmm rrr` = \
         opcode & 0xFF00 == 0x0800 with tt bits 7-6 == 10): BCLR 8065 = +8065 over B1's 510575 (the WHOLE \
         file in scope — no contaminant, no deferral). BCLR tests then CLEARS a single bit (`operand &= \
         !(1<<pos)`), setting ONLY Z = NOT(the PRE-clear bit); X/N/V/C AND the SR system byte are ALL PRESERVED. \
         Z is from the bit BEFORE the clear (the read value), not after. The bit width follows the DEST: a \
         `Dn` dest is 32-bit (`pos = b mod 32`, `Size::Long`, the FULL 32-bit register written with one bit \
         cleared), a memory dest is 8-bit (`pos = b mod 8`, `Size::Byte`, the byte RMW with one bit cleared). \
         New vocabulary: `AluOp::Bclr` (Btst + the clear write `a & !(1<<pos)`). It reuses the shared \
         `bit_recipe` VERBATIM, IDENTICAL to BCHG EXCEPT the register base idle is `n4` (BCLR is 8/10 cyc, 2 \
         slower than BCHG/BSET's 6/8) — `reg_base = 4`. The `Dn` dest shape is `[Prefetch, Alu, Internal(4), \
         (+Internal(2) iff DECODE-TIME pos>=16)]`; the `pos>=16` `+2` is the LOAD-BEARING subtlety (the bit \
         number is read at decode — the live `Dn` for dynamic / the captured `prefetch[1]` for static — so the \
         REGISTER recipe length depends on `regs`, exactly like Scc's true/false n2 and DBcc's counter). \
         Dynamic BCLR Dn = 8 (pos<16) / 10 (pos>=16); static = 12 / 14. Memory (2-6, 7/0, 7/1, `Size::Byte`) is \
         IDENTICAL to BCHG — the NEG-family read→modify→write RMW via `ea_dst` byte (read the byte, refill, the \
         bit-clear `Alu` into `Scratch(1)`, write back) — NO register `+2` (byte/mod-8 timing is FIXED per \
         mode): (An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l 20 (dynamic); static = +4. \
         The static form prepends `[EaCalc(ImmWord → scratch), Prefetch]` (capture the bitnum, then the refill) \
         and routes the EA's own ext words AFTER (mirrors `cmpi_recipe`). The FULL in-scope EA set is covered — \
         Dn (0) + data-alterable memory (2-6, 7/0, 7/1); An-direct (mode 1 = MOVEP) / PC-rel / #imm are not \
         alterable and absent. The plain `(A7)` mode-2 indirect is COVERED (a clean byte RMW, like CLR/TST — NO \
         deferral, NO `reg != 7` carve-out). Byte memory → NO odd-EA address-error faults (no parity filter). \
         Classified by OPCODE (the masks above; the 0x01xx/0x08xx spaces are disjoint from CMPI 0x0Cxx and the \
         BTST/BCHG forms tt == 00/01). Per-form true counts: dynamic 7166 + static 899 = 8065 (the WHOLE file \
         in scope). The BCLR.json file is 100% PURE (one op-type, no contaminant). \
         Prior baseline — B1 adds BCHG `<ea>` (its own BCHG.json file, dynamic \
         `0000 ddd 1 01 mmm rrr` = opcode & 0xF1C0 == 0x0140 / static `0000 1000 01 mmm rrr` = \
         opcode & 0xFF00 == 0x0800 with tt bits 7-6 == 01): BCHG 8065 = +8065 over B0's 502510 (the WHOLE \
         file in scope — no contaminant, no deferral). BCHG tests then TOGGLES a single bit (`operand ^= \
         1<<pos`), setting ONLY Z = NOT(the PRE-modify bit); X/N/V/C AND the SR system byte are ALL PRESERVED. \
         Z is from the bit BEFORE the toggle (the read value), not after. The bit width follows the DEST: a \
         `Dn` dest is 32-bit (`pos = b mod 32`, `Size::Long`, the FULL 32-bit register written with one bit \
         flipped), a memory dest is 8-bit (`pos = b mod 8`, `Size::Byte`, the byte RMW with one bit flipped). \
         New vocabulary: `AluOp::Bchg` (Btst + the toggle write `a ^ (1<<pos)`). The shared `bit_recipe` (which \
         BCLR/BSET reuse) has two dest shapes: `Dn` (mode 0, `Size::Long`) `[Prefetch, Alu, Internal(base), \
         (+Internal(2) iff DECODE-TIME pos>=16)]` — base = `n2` for BCHG; the `pos>=16` `+2` is the LOAD-BEARING \
         subtlety (the bit number is read at decode — the live `Dn` for dynamic / the captured `prefetch[1]` \
         for static — so the REGISTER recipe length depends on `regs`, exactly like Scc's true/false n2 and \
         DBcc's counter). Dynamic BCHG Dn = 6 (pos<16) / 8 (pos>=16); static = 10 / 12. Memory (2-6, 7/0, 7/1, \
         `Size::Byte`) is the NEG-family read→modify→write RMW via `ea_dst` byte (read the byte, refill, the \
         bit-toggle `Alu` into `Scratch(1)`, write back) — NO register `+2` (byte/mod-8 timing is FIXED per \
         mode): (An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l 20 (dynamic); static = +4. \
         The static form prepends `[EaCalc(ImmWord → scratch), Prefetch]` (capture the bitnum, then the refill) \
         and routes the EA's own ext words AFTER (mirrors `cmpi_recipe`). The FULL in-scope EA set is covered — \
         Dn (0) + data-alterable memory (2-6, 7/0, 7/1); An-direct (mode 1 = MOVEP) / PC-rel / #imm are not \
         alterable and absent. The plain `(A7)` mode-2 indirect is COVERED (a clean byte RMW, like CLR/TST — NO \
         deferral, NO `reg != 7` carve-out). Byte memory → NO odd-EA address-error faults (no parity filter). \
         Classified by OPCODE (the masks above; the 0x01xx/0x08xx spaces are disjoint from CMPI 0x0Cxx and the \
         BTST forms tt == 00). Per-form true counts: dynamic 7173 + static 892 = 8065 (the WHOLE file in \
         scope). The BCHG.json file is 100% PURE (one op-type, no contaminant). \
         Prior baseline — B0 adds BTST `<ea>` (its own BTST.json file, dynamic \
         `0000 ddd 1 00 mmm rrr` = opcode & 0xF1C0 == 0x0100 / static `0000 1000 00 mmm rrr` = \
         opcode & 0xFF00 == 0x0800 with tt bits 7-6 == 00): BTST 8065 = +8065 over C2's 494445 (the WHOLE \
         file in scope — no contaminant, no deferral). BTST tests a single bit, setting ONLY Z = NOT(bit); \
         X/N/V/C AND the SR system byte are ALL PRESERVED (`ccr = (sr & (X|N|V|C)) | (Z if bit==0)`). It is \
         READ-ONLY (`Dest::None`, no write — BCHG/BCLR/BSET add the write in later commits). The bit width \
         follows the operand: a `Dn` operand is 32-bit (`pos = b mod 32`, `Size::Long`), a memory/`#imm`/\
         PC-relative operand is 8-bit (`pos = b mod 8`, `Size::Byte`) — the `Alu` size field carries this. The \
         bit number `b`: DYNAMIC = `D[(opcode>>9)&7]` (`Operand::DataRegFull`, always live, no capture); STATIC \
         = the `prefetch[1]` ext word, captured into a scratch slot BEFORE the refill shifts it out (the \
         cmpi-style interleave) and fed as `Operand::Scratch`. New vocabulary: `AluOp::Btst` (the bit-test \
         flag-only op) + the bit-number operand. The `btst_recipe` has three operand shapes (timing pinned to \
         the vendored BTST stream; static = dynamic + 4): `Dn` (mode 0, `Size::Long`, trailing `Internal(2)` \
         bit-test idle) — dynamic `[Prefetch, Alu, Internal(2)]` = 6 cyc FIXED (NO `pos>=16` +2 — that variance \
         is ONLY the RMW trio; BTST is read-only) / static 10; `#imm` (7/4, dynamic only, `Size::Byte`, the \
         immediate read BEFORE the refills then the trailing idle) `[Alu(a=ImmWord), Prefetch, Prefetch, \
         Internal(2)]` = 10; memory/PC-relative (2-6, 7/0..7/3, `Size::Byte`, NO trailing idle) via `ea_src` \
         byte read → `Alu` on the just-read scratch (reuses `ea_src` verbatim — it already covers d16(PC)/\
         d8(PC,Xn)/#imm). The static form prepends `[EaCalc(ImmWord → scratch), Prefetch]` (capture the bitnum, \
         then the refill that consumes it) and routes the EA's own ext words AFTER (mirrors `cmpi_recipe`). \
         Memory cost (dynamic): (An)/(An)+ 8, -(An) 10, d16(An) 12, d8(An,Xn) 14, abs.w 12, abs.l 16, d16(PC) \
         12, d8(PC,Xn) 14, #imm 10; static = dynamic + 4. The FULL read-only source set is in scope — dynamic \
         Dn (0) + (An)/(An)+/-(An)/d16(An)/d8(An,Xn) (2-6) + abs.w/abs.l/d16(PC)/d8(PC,Xn)/#imm (7/0..7/4); \
         static the same MINUS #imm (7/0..7/3). An-direct (mode 1 = MOVEP) is absent. The plain `(A7)` mode-2 \
         indirect is COVERED (a clean byte read, like CLR/TST — NO deferral, NO `reg != 7` carve-out). Byte \
         memory → NO odd-EA address-error faults (no parity filter). Classified by OPCODE (the masks above; the \
         0x01xx/0x08xx spaces are disjoint from CMPI 0x0Cxx and the `*toSR` points, all bit-8-clear). Per-form \
         true counts: dynamic 7185 + static 880 = 8065 (the WHOLE file in scope). The BTST.json file is 100% \
         PURE (one op-type, no contaminant). \
         Prior baseline — C2 adds TAS `<ea>` MEMORY (its own TAS.json file, \
         `0100 1010 11 mmm rrr` = opcode & 0xFFC0 == 0x4AC0): the atomic-RMW memory forms = 6828 = +6828 over \
         C1's 487617 (which covered TAS `Dn` mode 0 = 1237; the WHOLE 8065-case file is now in scope — Dn 1237 \
         + memory 6828, no deferral). TAS memory is the INDIVISIBLE read-modify-write the SST stream models as \
         ONE `'t'` transaction (NOT a separate `'r'`+`'w'` pair): a single locked bus cycle (10 cyc = read 4 + \
         the indivisible modify 2 + write 4) reads the byte → sets N = bit7 / Z = (byte == 0), clears V/C, \
         PRESERVES X → writes `byte | 0x80` (the flags are on the READ byte, the written value `read | 0x80` is \
         DISTINCT). New vocabulary: `TxKind::Tas` (the `'t'` bus token, value = the WRITTEN byte) + \
         `Bus68k::tas(addr, fc) -> u8` (read `orig`, write `orig | 0x80`, log ONE `Tas` transaction, return \
         `orig`) + `MicroOp::TasRmw {{ addr }}` (the atomic 10-cyc RMW micro-op, one bus access = one quiesce \
         boundary) + the `ea_tas` builder (mirrors `ea_dst`'s seven EA arms but emits ONE `TasRmw` in place of \
         `Read, Prefetch, Alu, Write`, the trailing `Prefetch` AFTER it). Memory cost = CLR + 2 cyc everywhere: \
         (An)/(An)+ 14, -(An) 16, d16(An) 18, d8(An,Xn) 20, abs.w 18, abs.l 22. The data-alterable EA set is \
         FULLY in scope — Dn (0), (An) (2 — INCL the clean `(A7)` mode-2 indirect, NO deferral: the atomic \
         `[t@A7, prefetch]`), (An)+ (3), -(An) (4), d16(An) (5), d8(An,Xn) (6), abs.w (7/0), abs.l (7/1) — \
         per-mode true counts 1237 / 1280 / 1382 / 1259 / 1340 / 1248 / 147 / 172 = 8065; An-direct (1) / \
         PC-rel / #imm are not data-alterable and absent. Byte-only → NO odd-EA address-error faults (no parity \
         filter). The TAS.json file is 100% PURE (one opcode, no contaminant). \
         Prior baseline — C0 adds Scc `<ea>` (its own Scc.json file, `0101 cccc 11 mmm rrr` = \
         opcode & 0xF0C0 == 0x50C0, EXCL the 0x50C8 DBcc mode-001 form): Scc 8065 = +8065 over the prior 478315 \
         (the WHOLE file in scope — no contaminant, no deferral). Scc writes 0xFF if the condition `cc` (bits \
         11-8) is TRUE else 0x00, with NO flags (`final.sr == initial.sr`); the condition is resolved at DECODE \
         time (like Bcc/DBcc/TRAPV) via `condition_true(cc, sr)`, and cc 0 = T (always 0xFF) / cc 1 = F (always \
         0x00) are BOTH legal. New vocabulary: the no-flag byte constant write `MicroOp::SetByte {{ value: u8, \
         dst: Dest }}` (the analog of `LoadImm`, generalized to a `Dest` — into `Dest::DataRegLow8` it preserves \
         the upper 24 bits, into `Dest::Scratch` it parks the byte for the trailing memory Write; it touches NO \
         CCR bit, unlike `AluOp::Move` which CLR uses because CLR DOES set flags). `scc_recipe` resolves the \
         condition at decode and bakes the constant into `SetByte`: the `Dn`-direct arm is `[Prefetch, SetByte \
         (+ Internal(2) ONLY when the condition is TRUE)]` (FALSE = 4 cyc, TRUE = 6 — the ONLY true/false timing \
         difference, `Dn`-only), the memory arm is byte-for-byte `clr_recipe`'s read-then-write RMW (`ea_dst` \
         with `Size::Byte`) but writing the conditional constant with NO flags — condition-independent and \
         byte-IDENTICAL to CLR ((An)/(An)+ 12, -(An) 14, d16(An) 16, d8(An,Xn) 18, abs.w 16, abs.l 20). The \
         data-alterable EA set is FULLY in scope — Dn (0), (An) (2 — incl the clean `(A7)` mode-2 indirect, NO \
         deferral: Scc = CLR's exact byte RMW, which covers `(A7)` m2 and passes), (An)+ (3), -(An) (4), d16(An) \
         (5), d8(An,Xn) (6), abs.w (7/0), abs.l (7/1) — per-mode true counts 1280 / 1253 / 1303 / 1302 / 1320 / \
         1295 / 159 / 153 = 8065; An-direct (1) is DBcc (handled by `dbcc_covered` above) / not data-alterable, \
         PC-rel / #imm are not data-alterable and absent. Byte-only → NO odd-EA address-error faults (no parity \
         filter). The Scc.json file is 100% PURE (one opcode, no contaminant). \
         Prior baseline — G3 (the prior FINAL commit of that push) adds EXT.w / EXT.l + SWAP (their own \
         EXT.w/EXT.l/SWAP files, 0x4880/0x48C0/0x4840, mask `opcode & 0xFFF8`): EXT.w 8065 + EXT.l 8065 + SWAP \
         8065 = +24195 over G2's 454120. EXT sign-extends the `Dn`-only source whose result WIDTH follows the \
         size — EXT.w sign-extends the low BYTE to 16 bits and writes the LOW WORD (the high word of Dn is \
         PRESERVED), N = bit15 / Z = (word == 0); EXT.l sign-extends the low WORD to 32 bits and writes the FULL \
         32, N = bit31 / Z = (long == 0) — both with V = 0, C = 0, X PRESERVED (re-injected `ccr_nz | (sr & \
         CCR_X)`, never computed) via the new unary `AluOp::Ext`. SWAP swaps the two 16-bit halves of Dn on the \
         FULL 32 bits (`res = (Dn >> 16) | (Dn << 16)`, size always Long), LOGIC flags on bit31 / zero via the \
         new unary `AluOp::Swap`. All three decode with mask `0xFFF8` (mode FIXED 000 = `Dn`; the low 3 bits the \
         register) — NOT `0xFFC0`, which would swallow the PEA/MOVEM neighbours in 0x48xx (mode ≥ 2, reserved \
         this push). `ext_recipe`/`swap_recipe` are `[Prefetch, Alu{{...}}]` (4 cyc, one Prefetch, no idle, no \
         memory — `Dn`-only, no fault possible). `covered()` = `ext_swap_in_scope` (the `0xFFF8` mask match, NO \
         deferral — every case is `Dn`-direct, so the whole opcode space of each file is in scope). The \
         EXT.w/EXT.l/SWAP files are 100% pure (no contaminants). Prior baseline — G2 adds NOT.b / NOT.w / NOT.l \
         (their own NOT.b/.w/.l files, 0x4600/4640/ \
         4680, SS bits 7-6 = b/w/l): NOT.b 7901 + NOT.w 7894 + NOT.l 7899 = +23694 over G1's 430426. NOT \
         bitwise-complements the data-alterable EA — `res = (~d) & mask` with LOGIC flags (the SAME MOVE flag \
         shape as AND/OR/EOR): N = msb(res), Z = (res == 0), **V = 0, C = 0, X PRESERVED** (re-injected \
         `ccr_nz | (sr & CCR_X)`, never computed) via the new unary `AluOp::Not` (`~a`, `b` ignored, passed \
         `Operand::Zero`). NOT REUSES the shared `neg_family_recipe` VERBATIM (the read-then-write RMW shape is \
         identical to NEG/NEGX/CLR's `ea_dst`/`ea_dst_long`; only the `AluOp` exec differs — `~a` instead of a \
         subtraction). The data-alterable EA set is in scope — Dn (0), (An) (2), (An)+ (3), -(An) (4), d16(An) \
         (5), d8(An,Xn) (6), abs.w (7/0), abs.l (7/1) — except the pre-existing `(A7)` (mode 2) plain-indirect \
         deferral (its `(A7)+`/`-(A7)` siblings ARE in scope); An-direct / PC-rel / #imm are not data-alterable \
         and are absent. Odd word/long EAs address-error on the READ (low5 = 0x15), the E3/E4 abort covers them \
         (no parity filter). The NOT.* files are 100% pure (no contaminants). Prior baseline — G1 adds NEGX.b / \
         NEGX.w / NEGX.l (their own NEGX.b/.w/.l files, 0x4000/ \
         4040/4080, SS bits 7-6 = b/w/l): NEGX.b 7917 + NEGX.w 7893 + NEGX.l 7883 = +23693 over G0's 406733. NEGX \
         negate-with-extends the data-alterable EA — `res = (0 − d − X_in) & mask` with SUBX-style flags: N = \
         msb(res), Z is STICKY (`Z_final = Z_in AND (res == 0)` — NEGX only ever CLEARS Z, never sets it; a plain \
         `res == 0` is WRONG on the `res == 0 && Z_in == 0` case), V = `(d & res & signbit) != 0`, C = X = NOT(d \
         == 0 AND X_in == 0) borrow, where `X_in = (sr >> 4) & 1` and `Z_in = (sr >> 2) & 1` feed BOTH the value \
         and the borrow — via the DEDICATED `AluOp::Negx` (no Sub/Cmp delegation: only NEGX has sticky Z + X-in). \
         NEGX REUSES the shared `neg_family_recipe` VERBATIM (the read-then-write RMW shape is identical to \
         NEG/CLR's `ea_dst`/`ea_dst_long`; only the `AluOp` exec differs). The data-alterable EA set is in scope — \
         Dn (0), (An) (2), (An)+ (3), -(An) (4), d16(An) (5), d8(An,Xn) (6), abs.w (7/0), abs.l (7/1) — except the \
         pre-existing `(A7)` (mode 2) plain-indirect deferral (its `(A7)+`/`-(A7)` siblings ARE in scope); \
         An-direct / PC-rel / #imm are not data-alterable and are absent. Odd word/long EAs address-error on the \
         READ (low5 = 0x15), the E3/E4 abort covers them (no parity filter). The NEGX.* files are 100% pure (no \
         contaminants). Prior baseline — G0 adds NEG.b / NEG.w / NEG.l (their own NEG.b/.w/.l files, 0x4400/4440/ \
         4480, SS bits 7-6 = b/w/l): NEG.b 7915 + NEG.w 7893 + NEG.l 7917 = +23725 over L4's 383008. NEG negates \
         the data-alterable EA — `res = (0 − d) & mask` with FULL SUBTRACT flags (NEG is literally `0 − d`): N = \
         msb(res), Z = (res == 0), V = (d == sign-min) (the 0-minus-itself overflow), C = X = (d != 0) borrow — \
         byte-identical to `Sub(0, d)` via the new unary `AluOp::Neg` (the exec arm delegates to the same \
         `sub_{{b,w,l}}` helpers with `lhs = 0, rhs = the operand`; `b` is ignored, passed `Operand::Zero`). NEG is a \
         READ-then-WRITE for a memory dest — the SAME `ea_dst`/`ea_dst_long` RMW path as CLR, but the read operand \
         is the UNARY SOURCE (CLR discards it and writes 0; NEG negates it), built by the shared \
         `neg_family_recipe` (`Dn`-direct mode 0 has no memory: one Prefetch + the size-masked Neg into Dn — NEG.l \
         Dn = 6 cyc with a trailing n2, NEG.b/.w Dn = 4). The data-alterable EA set is in scope — Dn (0), (An) (2), \
         (An)+ (3), -(An) (4), d16(An) (5), d8(An,Xn) (6), abs.w (7/0), abs.l (7/1) — except the pre-existing \
         `(A7)` (mode 2) plain-indirect deferral (its `(A7)+`/`-(A7)` siblings ARE in scope); An-direct / PC-rel / \
         #imm are not data-alterable and are absent. Odd word/long EAs address-error on the READ (low5 = 0x15), \
         the E3/E4 abort covers them (no parity filter). The NEG.* files are 100% pure (no contaminants). \
         Prior baseline — L4 adds EOR.b / EOR.w / EOR.l in the `Dn,<ea>` direction ONLY (their \
         own EOR.b/.w/.l files): EOR.b 7026 + EOR.w 6999 + EOR.l 7012 = +21037 over L3's 361971. EOR is bitwise \
         `a ^ b` — the SAME MOVE flag shape as AND/OR (N = msb(result at size) / Z = (result == 0), V/C cleared, \
         X PRESERVED, re-injected as `ccr_nz | (sr & CCR_X)`, never computed) via the new `AluOp::Eor`; only the \
         bit op (`^`) differs. EOR exists ONLY in `Dn,<ea>` (opmode 4/5/6 = 0xB100/40/80); opmode 0/1/2 of 0xB is \
         CMP (there is NO `EOR <ea>,Dn`). The dest is a data register (mode 000 = `Dn,Dn`, its own no-memory arm — \
         `EOR.l Dn,Dn` carries a trailing n4 = 8 cyc; `.b`/`.w` = 4 cyc) or alterable memory (modes 2..6/abs.w/ \
         abs.l) via `arith_dn_ea` VERBATIM (EOR Dn,<ea> = ADD Dn,<ea> byte-for-byte). **Mode field 001 = CMPM** (a \
         DIFFERENT instruction handled by the `cmp_class` arm FIRST in dispatch — the EOR arm never sees mode \
         001; the EOR.* files have NO mode-001 cases). **CRITICAL *I CONTAMINATION**: the EOR.* files MIX the \
         genuine register form (high nibble 0xB) with the dedicated EORI immediate opcode (`0x0Axx`, high nibble \
         0 — a DIFFERENT instruction this push does NOT implement, ~9xx cases/file). `covered()` classifies by \
         OPCODE (`eor_in_scope`: high nibble == 0xB), admitting ONLY the genuine register form so the EORI cases \
         are skipped cleanly (never decoded — admitting one would panic at `todo!()`). All dest modes in scope \
         except the pre-existing `(A7)` (mode 2) plain-indirect deferral; odd word/long EAs are address errors \
         the E3/E4 abort covers (no parity filter). \
         Prior baseline — L3 adds OR.b / OR.w / OR.l in BOTH directions (their own OR.b/.w/.l \
         files): OR.b 7453 + OR.w 7402 + OR.l 7399 = +22254 over L2's 339717. OR is bitwise `a | b` — IDENTICAL \
         to AND in every respect except the bit op (`|` vs `&`) and the base nibble (0x8 vs 0xC): the MOVE flag \
         shape (N = msb(result at size) / Z = (result == 0), V/C cleared, X PRESERVED, re-injected as `ccr_nz | \
         (sr & CCR_X)`, never computed) via the new `AluOp::Or`; the size-masked result is written back \
         (low8/low16/full32 for a Dn dest, or parked in Scratch for a memory dest the trailing Write stores). \
         `<ea>,Dn` (opmode 0/1/2 = 0x8000/40/80, Dn = Dn | <ea>) reuses `arith_ea_dn` VERBATIM — OR <ea>,Dn = AND \
         <ea>,Dn = ADD <ea>,Dn byte-for-byte, MINUS the illegal An-direct source (mode 1 absent). `Dn,<ea>` \
         (opmode 4/5/6 = 0x8100/40/80, <ea> = <ea> | Dn, alterable-memory dest 2..6/abs.w/abs.l) reuses \
         `arith_dn_ea` VERBATIM = AND Dn,<ea> byte-for-byte; mode 000/001 = SBCD/PACK is RESERVED (excluded by \
         `is_dst_mem_mode`). **CRITICAL *I CONTAMINATION**: the OR.* files MIX the genuine register form (high \
         nibble 0x8) with the dedicated ORI immediate opcode (`0x00xx`, high nibble 0 — a DIFFERENT instruction \
         this push does NOT implement, 5xx cases/file). `covered()` classifies by OPCODE (`and_or_in_scope`: high \
         nibble == 0x8), admitting ONLY the genuine register form so the ORI cases are skipped cleanly (never \
         decoded — admitting one would panic at `todo!()`). The single `and_or_in_scope` predicate covers both \
         AND (0xC) and OR (0x8). All source/dest modes in scope except the pre-existing `(A7)` (mode 2) \
         plain-indirect deferral; odd word/long EAs are address errors the E3/E4 abort covers (no parity filter). \
         Prior baseline — L2 adds AND.b / AND.w / AND.l in BOTH directions (their own AND.b/.w/.l \
         files): AND.b 7391 + AND.w 7419 + AND.l 7417 = +22227 over L1's 317490. AND is bitwise `a & b` with the \
         MOVE flag shape — N = msb(result at size) / Z = (result == 0), V/C cleared, X PRESERVED (re-injected as \
         `ccr_nz | (sr & CCR_X)`, never computed) — via the new `AluOp::And`; the size-masked result is written \
         back (low8/low16/full32 for a Dn dest, or parked in Scratch for a memory dest the trailing Write stores). \
         `<ea>,Dn` (opmode 0/1/2 = 0xC000/40/80, Dn = Dn & <ea>) reuses `arith_ea_dn` VERBATIM — AND <ea>,Dn = ADD \
         <ea>,Dn byte-for-byte, MINUS the illegal An-direct source (mode 1 absent). `Dn,<ea>` (opmode 4/5/6 = \
         0xC100/40/80, <ea> = <ea> & Dn, alterable-memory dest 2..6/abs.w/abs.l) reuses `arith_dn_ea` VERBATIM = \
         ADD Dn,<ea> byte-for-byte; mode 000/001 = ABCD/EXG is RESERVED (excluded by `is_dst_mem_mode`). **CRITICAL \
         *I CONTAMINATION**: the AND.* files MIX the genuine register form (high nibble 0xC) with the dedicated \
         ANDI immediate opcode (`0x02xx`, high nibble 0 — a DIFFERENT instruction this push does NOT implement, \
         5xx cases/file). `covered()` classifies by OPCODE (`and_or_in_scope`: high nibble == 0xC), admitting ONLY \
         the genuine register form so the ANDI cases are skipped cleanly (never decoded — admitting one would \
         panic at `todo!()`). This is exactly parallel to the CMP/CMPM/CMPI 3-way mix. All source/dest modes in \
         scope except the pre-existing `(A7)` (mode 2) plain-indirect deferral (its `(A7)+`/`-(A7)` siblings ARE in \
         scope); odd word/long EAs are address errors the E3/E4 abort covers (no parity filter). \
         Prior baseline — L1 adds SUBA.w / SUBA.l (their own SUBA.w/.l files, `1001 aaa s11 mmm \
         rrr` = 0x90C0 (.w) / 0x91C0 (.l)): SUBA.w 7934 + SUBA.l 7971 = +15905 over L0's 301585. SUBA is the \
         no-flag address arithmetic `An = An − src` (SR untouched), a near-exact mirror of ADDA: `.w` \
         sign-extends the source word→long before the long-boundary SUBTRACT (mirroring MOVEA.w / CMPA.w — \
         `AluOp::Suba` does this internally), `.l` subtracts the full 32; An is written full-width \
         (`Dest::AddrReg`). All 12 source modes in scope (An-direct LEGAL — it is address arithmetic; odd \
         word/long source EAs are address errors the E3/E4 abort covers, no parity filter) except the \
         pre-existing `(A7)` (mode 2) plain-indirect deferral. The recipe REUSES the AluOp-parameterized \
         `adda_suba_recipe` built in L0 (`.w` appends a uniform trailing n4 idle = MOVEA.w's source stream + \
         n4; `.l` appends nothing — `ea_src_long`'s built-in n4/n2 idle already equals ADD.l <ea>,Dn). New \
         vocabulary: `AluOp::Suba` (the no-flag An-write early-return op, mirroring `AluOp::Adda`). The \
         SUBA.w/.l files are 100% pure (no contaminants). \
         Prior baseline — L0 adds ADDA.w / ADDA.l (their own ADDA.w/.l files, `1101 aaa s11 mmm \
         rrr` = 0xD0C0 (.w) / 0xD1C0 (.l)): ADDA.w 7935 + ADDA.l 7935 = +15870 over N6's 285715. ADDA is the \
         no-flag address arithmetic `An = An + src` (SR untouched): `.w` sign-extends the source word→long \
         before the long-boundary add (mirroring MOVEA.w / CMPA.w — `AluOp::Adda` does this internally), `.l` \
         adds the full 32; An is written full-width (`Dest::AddrReg`). All 12 source modes in scope (An-direct \
         LEGAL — it is address arithmetic; odd word/long source EAs are address errors the E3/E4 abort covers, \
         no parity filter) except the pre-existing `(A7)` (mode 2) plain-indirect deferral. The recipe reuses \
         `ea_src`: `.w` appends a uniform trailing n4 idle (ADDA.w = MOVEA.w's source stream + n4 for every \
         source mode), `.l` appends nothing (`ea_src_long`'s built-in n4/n2 idle already equals ADD.l <ea>,Dn). \
         New vocabulary: `AluOp::Adda` (the no-flag An-write early-return op, mirroring `AluOp::MoveA`). The \
         ADDA.w/.l files are 100% pure (no contaminants). \
         Prior baseline — N6 adds MOVEQ (its own MOVE.q file, `0111 ddd 0 dddddddd` = 0x7000 | \
         dn<<9 | imm8, bit 8 = 0): MOVE.q 8065 = +8065 over N5's 277650 (the WHOLE opcode space is in scope — no \
         EA modes, no memory access, no odd-address sub-cases; every vendored case has bit 8 clear, the only \
         legal form). MOVEQ loads a sign-extended 8-bit immediate into the FULL 32 bits of Dn (N = msb, Z = \
         value == 0, V/C cleared, X PRESERVED = the `MOVE` flag op at the long boundary). The immediate is the \
         opcode's OWN low byte (`Operand::BranchDisp8` = `sign_extend8(prefetch[0] & 0xFF)`), so there is NO \
         operand fetch: the recipe is a single flag-ALU into Dn (full 32) + the trailing FC-6 queue refill \
         (`[Alu Move/Long BranchDisp8 -> DataReg(Dn), Prefetch]`, length 4). No new vocabulary (`AluOp::Move` + \
         `Operand::BranchDisp8` + `Dest::DataReg` all exist). \
         Prior baseline — N5 adds CLR `<ea>` (its own CLR.b/.w/.l files, 0x4200/4240/4280, SS \
         bits 7-6 = b/w/l): CLR.b 8065 + CLR.w 8065 + CLR.l 8065 = +24195 over N4's 253455. CLR clears the \
         data-alterable EA to 0 (Z = 1, N = 0, V = 0, C = 0, X PRESERVED = `move_flags(0)`). CLR is a \
         READ-then-WRITE: it READS the EA (value DISCARDED), refills, then WRITES 0 — reusing the existing \
         `ea_dst`/`ea_dst_long` RMW path with `make_alu` building `AluOp::Move` (`a = Operand::Zero`, dst \
         `Scratch(1)`): the Move sets the flags + parks the 0 the trailing Write stores; the `.l` write order \
         is the reversed long store, lo @ EA+2 then hi @ EA. `Dn`-direct has no memory access (one Prefetch + \
         the size-masked \
         Move-of-zero into Dn); CLR.l Dn = 6 cyc (one trailing `Internal(2)` idle), CLR.b/.w Dn = 4. The \
         data-alterable EA set is FULLY in scope — Dn (0), (An) (2), (An)+ (3), -(An) (4), d16(An) (5), \
         d8(An,Xn) (6), abs.w (7/0), abs.l (7/1) (all 8065 per file; NO `(A7)` mode-2 deferral — the plain \
         `(A7)` indirect RMW is clean); An-direct / PC-relative / #imm are not data-alterable and are absent. \
         Odd word/long EAs address-error on the READ (low5 = 0x15), the E3/E4 abort covers them (no parity \
         filter). \
         Prior baseline — N4 adds TST `<ea>` (its own TST.b/.w/.l files, 0x4A00/4A40/4A80, SS \
         bits 7-6 = b/w/l): TST.b 8065 + TST.w 8065 + TST.l 8065 = +24195 over N3's 229260. TST is the \
         flag-only test `<ea> − 0` (`AluOp::Cmp` with `b = Operand::Zero` + `Dest::None`): N = msb(operand), \
         Z = (operand == 0), V = 0, C = 0, X PRESERVED, NO write-back. The data-alterable EA set is FULLY in \
         scope — Dn (0), (An) (2), (An)+ (3), -(An) (4), d16(An) (5), d8(An,Xn) (6), abs.w (7/0), abs.l (7/1) \
         (all 8065 per file; NO `(A7)` mode-2 deferral — TST never writes, so the plain `(A7)` indirect read is \
         clean); An-direct / PC-relative / #imm are not data-alterable and are absent. UNLIKE CMP/ADD there is \
         NO trailing idle for any size (TST.l Dn = 4 not 6; TST.l (An) = 12 not 14 — the long source reuses the \
         idle-free MOVEA.l reader, byte/word reuse `ea_src` directly). Odd word/long EAs are address errors the \
         E3/E4 abort covers (no parity filter). \
         Prior baseline — N3 adds CMPA `<ea>,An` (its own CMPA.w/.l files, opmode 3/7 = .w/.l): \
         CMPA.w 7935 + CMPA.l 7921 = +15856 over N2's 213404. CMPA is the flag-only address compare `An − \
         <ea>` (An the minuend, full 32 bits); the `.w` source word is sign-extended to 32 (`AluOp::Cmpa`, \
         mirroring `AluOp::MoveA`) before the long-boundary subtraction setting N/Z/V/C and PRESERVING X, with \
         NO write-back (`Dest::None`). Its source bus stream mirrors MOVEA of the same size (every source mode) \
         plus a uniform trailing `Internal(2)` idle (CMPA = MOVEA + 2 cyc — pinned to the data). All 12 source \
         modes in scope except the pre-existing `(A7)` mode-2 plain-indirect deferral (its `(A7)+`/`-(A7)` \
         siblings ARE in scope); odd word/long source EAs are address errors the E3/E4 abort covers. \
         Prior baseline — N2 adds the CMPI `#imm,<ea>` (Cmpi) class of the 3-way CMP.* mix, so \
         the CMP.* files are now FULLY covered (CMP.b 7939 + CMP.w 7949 + CMP.l 7943 = +2012 over N1's 211392: \
         CMPI in-scope contributes 778 + 603 + 631). CMPI captures the immediate (one ext word for b/w, TWO \
         for `.l`) BEFORE the EA's extension words, then reads the data-alterable EA and DISCARDS it (NO write \
         — CMPI is not an RMW), feeding the flag-only `<ea> − #imm` (`AluOp::Cmp` + `Dest::None`, X preserved). \
         The data-alterable modes in scope: Dn (0, no memory access), (An) (2, minus the `(A7)` mode-2 \
         deferral), (An)+ (3), -(An) (4), d16(An) (5), d8(An,Xn) (6), abs.w (7/0), abs.l (7/1); An-direct / \
         PC-rel / #imm are illegal/absent. Odd word/long EAs are address errors the E3/E4 abort covers. N1 had \
         added the CMPM `(Ay)+,(Ax)+` (Cmpm) class (CMP.b 7161 + CMP.w 7346 + CMP.l 7312 over N0's 208715: \
         CMPM contributes 911 + 889 + 877). CMPM is two post-increment reads (src @ (Ay)+ first, then dst @ \
         (Ax)+) feeding the flag-only `(Ax) − (Ay)`; the `(An)+` bump is committed before the read (odd-EA \
         read-faults still bump, via the E3/E4 abort), so no parity filter. N0 added the CMP `<ea>,Dn` (Cmp) \
         class (CMP.b 6250 + CMP.w 6457 + CMP.l 6435 = +19142 over E6's 189573; only the Cmp class in scope, \
         classified by OPCODE never the misleading `name`, minus the `(A7)` mode-2 plain-indirect source \
         deferral). CMP sets N/Z/V/C \
         exactly as SUB but PRESERVES X and writes nothing; odd word/long source EAs are address errors the \
         E3/E4 abort covers. Prior baseline — E6 adds the four privileged-op files (ANDItoSR/ORItoSR/EORItoSR/RESET, \
         8065 each, fully in scope = +32260 over E5's 157313). E4 had flipped the odd-address xfails IN: the \
         execution-time address-error abort (E3) installs the group-0 14-byte vector-3 frame, so every odd \
         word/long EA, odd branch / jump / return target, and odd popped PC/return-address PASSES through both \
         drivers unchanged (regs/SR/RAM/prefetch/cycles + the per-cycle transaction stream). The `(An)+`/`-(An)` \
         auto-(in/de)crement register bump is committed BEFORE the faulting access (matching the data: a \
         read-fault leaves `(An)+` bumped and `-(An)` decremented, while the MOVE.l predecrement-store \
         write-fault leaves `-(An)` decremented by only 2 — the 68000's two-step long store). The only \
         remaining deferrals are mode-scope (NOT odd-address): the `(A7)` (mode 2) plain-indirect form (a \
         pre-existing convention — its `(A7)+`/`-(A7)` siblings are in scope), the illegal `ADD.b An,Dn` byte \
         source, and not-yet-implemented EA modes. Per-file true counts (each file holds 8065 cases): ADD.w \
         4841 + SUB.w 4845 + ADD.b 5003 + SUB.b 4971 + ADD.l 4876 + SUB.l 4898 (ADD/SUB 29434) + MOVE.w 7746 \
         + MOVE.b 7796 + MOVE.l 7768 (MOVE 23310) + MOVEA.w 7923 + MOVEA.l 7931 (MOVEA 15854) + Bcc 8065 + \
         BSR 8065 (incl. the 35 `0x61FF` byte-form −1 odd-target cases) + JMP 8065 + JSR 8065 + RTS 8065 + \
         DBcc 8065 + RTR 8065 + RTE 8065 (all 8 fully in scope — every odd target/pop now covered) + TRAP \
         8065 + TRAPV 8065 + CHK 8065 (CHK = all 11 source modes, no-trap + both trap predicates; an odd \
         source EA is the E3/E4 address-error frame, and there is no `(A7)` mode-2 deferral) + ANDItoSR 8065 \
         + ORItoSR 8065 + EORItoSR 8065 (all supervisor — the mid-instruction FC switch S→user IS exercised; \
         the user-mode privilege-violation entry is correctness-only) + RESET 8065 (n4 + n124 + one queue \
         refill) (the always-supervisor S/T/A7 transform is structurally exercised but a no-op on the data — \
         correctness-only). ran {ran}"
    );
    eprintln!("SingleStepTests ADD+SUB+MOVE+MOVEA+Bcc+BSR+JMP+JSR+RTS+DBcc+RTR+TRAP+RTE+TRAPV+CHK+ANDItoSR+ORItoSR+EORItoSR+RESET+CMP+CMPA+TST+CLR+MOVEQ+ADDA+SUBA+AND+OR+EOR+NEG+NEGX+NOT+EXT+SWAP+Scc+TAS+BTST+BCHG+BCLR+BSET+ASL+ASR+LSL+LSR+ROL+ROR+ROXL+ROXR (.w + .b + .l): {ran} covered cases passed (both framework drivers, regs/SR/RAM/prefetch/cycles/transactions)");
}

/// E3 — the execution-time **address-error abort** + the group-0 **14-byte frame**, proven on a handful of
/// NAMED odd anchors WITHOUT flipping `covered()` (the mass flip of every family's odd-address xfails is E4).
/// Each anchor is a real vendored case (scattered inside the already-vendored ADD/MOVE/Bcc files) whose
/// word/long bus access — or program fetch — targets an ODD address, so the faulting micro-op rewrites its
/// `MicroState` into the vector-3 14-byte frame in place. Both framework drivers must reproduce
/// regs/SR/RAM/prefetch/cycles AND the per-cycle bus-transaction stream (so the abort is identical on the
/// run-to-completion and quiesce paths). These anchors exercise every shape of the new mechanism:
/// data-read (An), a multi-word computed EA (SSW high = the ORIGINAL opcode; the access address stacked as a
/// full 32-bit long), the ADD/SUB RMW (which faults on the READ, low5 = 0x15, never the write), the MOVE
/// data-WRITE (low5 = 0x05, SR stacked with MOVE's CCR already updated), a taken-branch program fetch to an
/// odd target (low5 = 0x1E, stacked PC = target − 4), and an `abs.l` odd base (the full 32-bit access
/// address `(extHi << 16) | extLo`, which the old 24-bit EaCalc mask would have destroyed).
#[test]
fn address_error_anchors_match_singlesteptests() {
    // (file, opcode-hex name prefix, expected length) — the (prefix, length) pair uniquely picks the
    // address-error case out of the many same-opcode cases (the clean even cases are far shorter).
    let anchors: &[(&str, &str, u32)] = &[
        ("ADD.w.json", "d850", 50), // ADD.w (A0),D4 — data-read fault (An), low5=0x15
        ("ADD.w.json", "d06c", 54), // ADD.w (d16,A4),D0 — multi-word EA, SSW-high = original opcode
        ("ADD.w.json", "dd56", 50), // ADD.w D6,(A6) — RMW faults on the READ, low5=0x15
        ("MOVE.w.json", "3c82", 50), // MOVE.w D2,(A6) — data-WRITE fault, low5=0x05, SR pre-updated
        ("Bcc.json", "6d25", 52),   // Bcc taken odd target — program-read low5=0x1E, stPC=target-4
        ("ADD.l.json", "d8b9", 58), // ADD.l (abs.l),D4 — odd base, full-32 access-addr
        ("CMP.w.json", "bc51", 50), // CMP.w (A1),D6 — odd source read (An), low5=0x15, flag-only CMP
    ];
    let mut found = 0usize;
    for (fname, prefix, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap().starts_with(prefix)
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| {
                panic!("E3 address-error anchor {prefix} (len {length}) not found in {fname}")
            });
        run_case(case);
        found += 1;
    }
    assert_eq!(
        found,
        anchors.len(),
        "all E3 address-error anchors exercised"
    );
    eprintln!(
        "E3 address-error anchors: {found} odd cases (group-0 14-byte vector-3 frame) passed both drivers"
    );
}

/// N0 — the named `CMP <ea>,Dn` source-mode anchors, pinning each shape of the flag-only compare against the
/// vendored CMP.* stream WITHOUT relying on the bulk `covered()` sweep to have reached them: a **Dn** source
/// (no read, the n2 register long idle), an **An** source (the legal `.w/.l` register source), and a **memory**
/// source per size (the `[Read, Prefetch]` / long `[Read.hi, Read.lo, Prefetch]` interleave). Each runs both
/// drivers + the per-cycle transaction stream via `run_case`. The load-bearing invariant — **CMP sets N/Z/V/C
/// like SUB but PRESERVES X** — is exercised by every case (and pinned tightly by the `b685` unit test in
/// `decode.rs`, whose initial SR carries X set and whose positive-diff result keeps X).
#[test]
fn cmp_source_mode_anchors_match_singlesteptests() {
    // (file, name-prefix, length) — uniquely picks the clean (even-EA) anchor out of same-opcode cases.
    let anchors: &[(&str, &str, u32)] = &[
        ("CMP.l.json", "b685", 6), // CMP.l D5,D3 — Dn source (n2 register long idle, 6 cyc)
        ("CMP.l.json", "bc88", 6), // CMP.l A0,D6 — An source (legal .l register source, 6 cyc)
        ("CMP.w.json", "b650", 8), // CMP.w (A0),D3 — memory source (.w, [Read, PF], 8 cyc)
        ("CMP.l.json", "b492", 14), // CMP.l (A2),D2 — memory source (.l, [Read.hi, Read.lo, PF], 14 cyc)
    ];
    let mut found = 0usize;
    for (fname, prefix, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap().starts_with(prefix)
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| {
                panic!("N0 CMP source-mode anchor {prefix} (len {length}) not found in {fname}")
            });
        // Every anchor must classify as the Cmp class (by OPCODE, not name).
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            cmp_class(opcode),
            CmpClass::Cmp,
            "anchor {prefix} must be the Cmp class"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(
        found,
        anchors.len(),
        "all N0 CMP source-mode anchors exercised"
    );
    eprintln!(
        "N0 CMP source-mode anchors: {found} cases (Dn / An / memory sources) passed both drivers"
    );
}

/// N2 — the named `CMPI #imm,<ea>` anchors, pinning each size's immediate-then-EA prefetch interleave + the
/// data-alterable EA read (discarded, NO write) against the vendored CMP.* (CMPI) stream WITHOUT relying on the
/// bulk `covered()` sweep: a **Dn-dest** form per size (no memory access — `#imm` then a register compare) and a
/// **memory-dest** form per size (the `.b`/`.w` single read, the `.l` long read pair). Each runs both drivers +
/// the per-cycle transaction stream via `run_case`. The load-bearing pins: the immediate's extension word(s)
/// precede the EA's extension word(s); the EA is READ-and-DISCARDED (no write-back — CMPI is not an RMW); X is
/// preserved. Every anchor must classify as the Cmpi class **by OPCODE** (the CMP.* `name` fields lie).
#[test]
fn cmpi_anchors_match_singlesteptests() {
    // (file, name-prefix, length) — the (prefix, length) pair picks the clean (even-EA) CMPI case.
    let anchors: &[(&str, &str, u32)] = &[
        ("CMP.b.json", "0c06", 8), // CMPI.b #imm,D6 — Dn-dest (no memory access, 1 imm word, 2 refills)
        ("CMP.b.json", "0c10", 12), // CMPI.b #imm,(A0) — memory-dest single read, [PF, READ, PF]
        ("CMP.b.json", "0c39", 20), // CMPI.b #imm,abs.l — abs.l address assembly after the immediate
        ("CMP.w.json", "0c47", 8),  // CMPI.w #imm,D7 — Dn-dest (no memory access)
        ("CMP.w.json", "0c55", 12), // CMPI.w #imm,(A5) — memory-dest single read
        ("CMP.l.json", "0c82", 14), // CMPI.l #imm,D2 — Dn-dest (2 imm words, 3 refills, n2 idle)
        ("CMP.l.json", "0c93", 20), // CMPI.l #imm,(A3) — memory-dest long read pair, no trailing idle
        ("CMP.l.json", "0caa", 24), // CMPI.l #imm,d16(A2) — d16 EA after the 2-word immediate
        ("CMP.l.json", "0cb9", 28), // CMPI.l #imm,abs.l — the heaviest interleave (imm.l then abs.l)
    ];
    let mut found = 0usize;
    for (fname, prefix, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap().starts_with(prefix)
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| {
                panic!("N2 CMPI anchor {prefix} (len {length}) not found in {fname}")
            });
        // Every anchor must classify as the Cmpi class (by OPCODE, not name).
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            cmp_class(opcode),
            CmpClass::Cmpi,
            "anchor {prefix} must be the Cmpi class"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all N2 CMPI anchors exercised");
    eprintln!(
        "N2 CMPI anchors: {found} cases (Dn-dest + memory-dest, each size) passed both drivers"
    );
}

/// N3 — the named `CMPA <ea>,An` anchors, pinning each source-mode shape of the flag-only address compare `An
/// − <ea>` against the vendored CMPA.w/.l stream WITHOUT relying on the bulk `covered()` sweep: a **Dn**
/// source whose source word has the high bit set (the `AluOp::Cmpa` sign-extend word→long test — `b2c3`'s D3
/// low word is `0xd87d`), an **An** source (the legal register source), a **memory** source per size (`.w`
/// `[Read, PF, n2]`, `.l` `[Read.hi, Read.lo, PF, n2]` — including the plan's `bdd7 CMPA.l (A7),A6`), and an
/// **#imm** source per size. Each runs both drivers + the per-cycle transaction stream via `run_case`. The
/// load-bearing pins: the `.w` source is sign-extended to 32 before the long-boundary subtraction; N/Z/V/C are
/// set; X is PRESERVED; nothing is written back; and CMPA = MOVEA + a uniform trailing `n2` idle. Every anchor
/// must classify as the Cmpa class **by OPCODE**.
#[test]
fn cmpa_anchors_match_singlesteptests() {
    // (file, name-prefix, length) — the (prefix, length) pair picks the clean (even-EA) CMPA case.
    let anchors: &[(&str, &str, u32)] = &[
        ("CMPA.w.json", "b2c3", 6), // CMPA.w D3,A1 — Dn source, D3 low word 0xd87d (sign-extend test)
        ("CMPA.w.json", "bccb", 6), // CMPA.w A3,A6 — An source (register source, n2 idle)
        ("CMPA.w.json", "b8d3", 10), // CMPA.w (A3),A4 — memory source (.w, [Read, PF, n2])
        ("CMPA.w.json", "bcfc", 10), // CMPA.w #imm,A6 — immediate source (.w)
        ("CMPA.l.json", "b1d1", 14), // CMPA.l (A1),A0 — memory source (.l, [Read.hi, Read.lo, PF, n2])
        ("CMPA.l.json", "bdd7", 14), // CMPA.l (A7),A6 — memory source via A7 (the plan's named anchor)
        ("CMPA.l.json", "b9fc", 14), // CMPA.l #imm,A4 — immediate source (.l, 3 refills)
    ];
    let mut found = 0usize;
    for (fname, prefix, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap().starts_with(prefix)
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| {
                panic!("N3 CMPA anchor {prefix} (len {length}) not found in {fname}")
            });
        // Every anchor must classify as the Cmpa class (by OPCODE, not name).
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            cmp_class(opcode),
            CmpClass::Cmpa,
            "anchor {prefix} must be the Cmpa class"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all N3 CMPA anchors exercised");
    eprintln!(
        "N3 CMPA anchors: {found} cases (Dn / An / memory / #imm sources, each size) passed both drivers"
    );
}

/// N4 — the named `TST <ea>` anchors, pinning each source-mode shape of the flag-only test `<ea> − 0` against
/// the vendored TST.b/.w/.l stream WITHOUT relying on the bulk `covered()` sweep: a **Dn** source whose low
/// byte sets N (the N-set Dn anchor `4a03`), a **memory** source per size (the `.w`/`.b` single `[Read, PF]`
/// read, the `.l` `[Read.hi, Read.lo, PF]` long read pair — including the plan's `4a56`/`4a97`), a **-(An)**
/// predecrement source, a long **d8(An,Xn)** indexed source, and a long **abs.l** source. Each runs both
/// drivers + the per-cycle transaction stream via `run_case`. The load-bearing pins: N = msb / Z =
/// (operand == 0), V/C cleared, X PRESERVED, nothing written back, and — UNLIKE CMP/ADD — NO trailing idle
/// for any size. Every anchor must decode as a TST opcode (0x4A00/4A40/4A80, SS != 3 = not TAS).
#[test]
fn tst_anchors_match_singlesteptests() {
    // (file, name-prefix, length) — the (prefix, length) pair picks the clean (even-EA) TST case.
    let anchors: &[(&str, &str, u32)] = &[
        ("TST.b.json", "4a03", 4), // TST.b D3 — Dn source, low byte 0xE9 sets N (the N-set anchor)
        ("TST.b.json", "4a20", 10), // TST.b -(A0) — predecrement source ([n2, Read, PF])
        ("TST.w.json", "4a56", 8), // TST.w (A6) — memory source (.w, [Read, PF], NO trailing idle)
        ("TST.l.json", "4a97", 12), // TST.l (A7) — memory source via A7 (.l, [Read.hi, Read.lo, PF])
        ("TST.l.json", "4ab0", 18), // TST.l (d8,A0,Xn) — indexed long source ([n2, PF, Read.hi, Read.lo, PF])
        ("TST.l.json", "4ab9", 20), // TST.l abs.l — long abs.l source (the heaviest, 5 reads, no idle)
    ];
    let mut found = 0usize;
    for (fname, prefix, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap().starts_with(prefix)
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| {
                panic!("N4 TST anchor {prefix} (len {length}) not found in {fname}")
            });
        // Every anchor must be a TST opcode (0x4A00/4A40/4A80, SS != 3 = not TAS) — never a CMP-class opcode.
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode & 0xFF00,
            0x4A00,
            "anchor {prefix} must be a TST opcode"
        );
        assert_ne!(
            opcode & 0xC0,
            0xC0,
            "anchor {prefix} must not be TAS (SS == 3)"
        );
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "TST is not a CMP-class opcode"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all N4 TST anchors exercised");
    eprintln!(
        "N4 TST anchors: {found} cases (Dn / memory / -(An) / indexed / abs.l sources, each size) passed both drivers"
    );
}

/// N5 — the named `CLR <ea>` anchors, pinning each destination-mode shape of the EA-clear against the vendored
/// CLR.b/.w/.l stream WITHOUT relying on the bulk `covered()` sweep: the **CLR.l Dn** 6-cyc register clear (one
/// trailing idle — `4282`), the **CLR.b (An)** READ-then-WRITE memory clear (the canonical `[r @ EA, PF, w 0 @
/// EA]` order — `4216`), the **CLR.w -(A1)** predecrement clear (`[n2, r, PF, w]` at the decremented EA —
/// `4261`), and the **CLR.l (A3)+** postincrement clear (the long RMW with the reversed long store `[r.hi,
/// r.lo, PF, w.lo, w.hi]` — `429b`). Each runs both drivers + the per-cycle transaction stream via `run_case`.
/// The load-bearing pins: the READ precedes the WRITE (CLR is not write-only), 0 is written, the flags are
/// `move_flags(0)` (Z=1/N=0/V=0/C=0, X PRESERVED), and the `.l` register clear carries its one trailing idle.
/// Every anchor must decode as a CLR opcode (0x4200/4240/4280, SS != 3 = not the illegal 0x42C0).
#[test]
fn clr_anchors_match_singlesteptests() {
    // (file, name-prefix, length) — the (prefix, length) pair picks the clean (even-EA) CLR case.
    let anchors: &[(&str, &str, u32)] = &[
        ("CLR.l.json", "4282", 6), // CLR.l D2 — Dn-direct (6 cyc, one trailing idle)
        ("CLR.b.json", "4216", 12), // CLR.b (A6) — memory READ-then-WRITE ([r, PF, w])
        ("CLR.w.json", "4261", 14), // CLR.w -(A1) — predecrement ([n2, r, PF, w])
        ("CLR.l.json", "429b", 20), // CLR.l (A3)+ — postinc long RMW (reversed long store)
    ];
    let mut found = 0usize;
    for (fname, prefix, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap().starts_with(prefix)
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| {
                panic!("N5 CLR anchor {prefix} (len {length}) not found in {fname}")
            });
        // Every anchor must be a CLR opcode (0x4200/4240/4280, SS != 3 = not the illegal 0x42C0) — never a
        // CMP-class opcode.
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode & 0xFF00,
            0x4200,
            "anchor {prefix} must be a CLR opcode"
        );
        assert_ne!(opcode & 0xC0, 0xC0, "anchor {prefix} must not be SS == 3");
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "CLR is not a CMP-class opcode"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all N5 CLR anchors exercised");
    eprintln!(
        "N5 CLR anchors: {found} cases (Dn / (An) RMW / -(An) / (An)+ long RMW) passed both drivers"
    );
}

/// N6 — the named `MOVEQ #imm8,Dn` anchor, pinning the sign-extended quick-load against the vendored MOVE.q
/// stream WITHOUT relying on the bulk `covered()` sweep: the plan's `7cb5` case (MOVEQ #0xB5,D6) whose immediate
/// 0xB5 sign-extends to 0xFFFFFFB5, exercising the load-bearing sign-extension into the FULL 32 bits of Dn (the
/// upper 24 bits are NOT preserved — MOVEQ writes all 32). It runs both drivers + the per-cycle transaction
/// stream via `run_case`. The load-bearing pins: the value is `sign_extend8(opcode low byte)`, written full-width
/// to Dn; N = msb / Z = (value == 0), V/C cleared, X PRESERVED; one FC-6 queue refill (length 4), no operand
/// fetch. The anchor must be a MOVEQ opcode (0x7000 | dn<<9 | imm8, bit 8 = 0) — never a CMP-class opcode.
#[test]
fn moveq_anchor_matches_singlesteptests() {
    // (file, name-prefix, length) — the (prefix, length) pair picks the named MOVEQ case.
    let anchors: &[(&str, &str, u32)] = &[
        ("MOVE.q.json", "7cb5", 4), // MOVEQ #0xB5,D6 — sign-extend 0xB5 → 0xFFFFFFB5 (full 32 bits)
    ];
    let mut found = 0usize;
    for (fname, prefix, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap().starts_with(prefix)
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| {
                panic!("N6 MOVEQ anchor {prefix} (len {length}) not found in {fname}")
            });
        // The anchor must be a MOVEQ opcode (0x7000 | dn<<9 | imm8, bit 8 = 0) — never a CMP-class opcode.
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode & 0xF100,
            0x7000,
            "anchor {prefix} must be a MOVEQ opcode (bit 8 = 0)"
        );
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "MOVEQ is not a CMP-class opcode"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all N6 MOVEQ anchors exercised");
    eprintln!("N6 MOVEQ anchors: {found} case (sign-extended quick-load) passed both drivers");
}

/// L2 — the named `AND` anchors, pinning each shape of bitwise AND in BOTH directions against the vendored
/// AND.b/.w/.l stream WITHOUT relying on the bulk `covered()` sweep: `<ea>,Dn` with a **Dn** source, a
/// **(An)** memory source, and a **#imm** source (each size b/w/l); `Dn,(An)` **memory dest** (each size); and
/// two **odd-EA** cases (the word/long memory access faults into the group-0 14-byte address-error frame —
/// byte never faults). Each runs both drivers + the per-cycle transaction stream via `run_case`. The
/// load-bearing pins: AND sets N = msb / Z = (result == 0), clears V/C, **PRESERVES X** (every case); the
/// `<ea>,Dn` direction reuses `arith_ea_dn` (= ADD <ea>,Dn timing) and `Dn,<ea>` reuses `arith_dn_ea` (= ADD
/// Dn,<ea> RMW). Every anchor must classify **by OPCODE** as the genuine register form (high nibble 0xC, via
/// `and_or_in_scope`) and NOT as a CMP-class opcode — the *I (ANDI) contaminant is never an anchor.
#[test]
fn and_anchors_match_singlesteptests() {
    // (file, name-prefix, length) — the (prefix, length) pair picks the clean (even-EA) or odd-EA AND case.
    let anchors: &[(&str, &str, u32)] = &[
        // <ea>,Dn — Dn / (An) / #imm sources, each size.
        ("AND.b.json", "c801", 4), // AND.b D1,D4 — Dn source (.b, 4 cyc, no idle)
        ("AND.b.json", "c614", 8), // AND.b (A4),D3 — (An) memory source ([Read, PF])
        ("AND.b.json", "c03c", 8), // AND.b #imm,D0 — immediate source
        ("AND.w.json", "c440", 4), // AND.w D0,D2 — Dn source (.w)
        ("AND.w.json", "c250", 8), // AND.w (A0),D1 — (An) memory source
        ("AND.w.json", "c07c", 8), // AND.w #imm,D0 — immediate source
        ("AND.l.json", "c880", 8), // AND.l D0,D4 — Dn source (.l, 8 cyc reg idle)
        ("AND.l.json", "c294", 14), // AND.l (A4),D1 — (An) memory source (.l, [r.hi, r.lo, PF])
        ("AND.l.json", "c6bc", 16), // AND.l #imm,D3 — immediate.l source (2 imm words)
        // Dn,<ea> — memory dest, each size (the arith_dn_ea RMW).
        ("AND.b.json", "cf12", 12), // AND.b D7,(A2) — byte memory dest ([r, PF, w])
        ("AND.w.json", "cf54", 12), // AND.w D7,(A4) — word memory dest
        ("AND.l.json", "cb92", 20), // AND.l D5,(A2) — long memory dest (reversed long store)
        // Odd-EA address-error frames (word/long only — byte never faults).
        ("AND.w.json", "cf50", 50), // AND.w D7,(A0) odd dest → group-0 14-byte frame (write fault, low5=0x05)
        ("AND.l.json", "c494", 50), // AND.l (A4),D2 odd source → 14-byte frame (read fault, low5=0x15)
    ];
    let mut found = 0usize;
    for (fname, prefix, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap().starts_with(prefix)
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| {
                panic!("L2 AND anchor {prefix} (len {length}) not found in {fname}")
            });
        // Every anchor must be the genuine register-form AND (high nibble 0xC, in scope) classified by OPCODE —
        // never the ANDI immediate contaminant (high nibble 0) and never a CMP-class opcode.
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode >> 12,
            0xC,
            "anchor {prefix} must be a genuine AND opcode"
        );
        assert!(
            and_or_in_scope(opcode),
            "anchor {prefix} must be an in-scope genuine register-form AND"
        );
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "AND is not a CMP-class opcode"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all L2 AND anchors exercised");
    eprintln!(
        "L2 AND anchors: {found} cases (<ea>,Dn Dn/(An)/#imm + Dn,(An) dest, each size + odd-EA) passed both drivers"
    );
}

/// L3 — the named `OR` anchors, pinning each shape of bitwise OR in BOTH directions against the vendored
/// OR.b/.w/.l stream WITHOUT relying on the bulk `covered()` sweep: `<ea>,Dn` with a **Dn** source, a
/// **(An)** memory source, and a **#imm** source (each size b/w/l); `Dn,(An)` **memory dest** (each size); and
/// two **odd-EA** cases (the word/long memory access faults into the group-0 14-byte address-error frame —
/// byte never faults). Each runs both drivers + the per-cycle transaction stream via `run_case`. The
/// load-bearing pins: OR sets N = msb / Z = (result == 0), clears V/C, **PRESERVES X** (every case); the
/// `<ea>,Dn` direction reuses `arith_ea_dn` (= ADD <ea>,Dn timing = AND <ea>,Dn) and `Dn,<ea>` reuses
/// `arith_dn_ea` (= ADD Dn,<ea> RMW = AND Dn,<ea>). Every anchor must classify **by OPCODE** as the genuine
/// register form (high nibble 0x8, via `and_or_in_scope`) and NOT as a CMP-class opcode — the *I (ORI)
/// contaminant (high nibble 0) is never an anchor. The opcodes mirror the L2 AND anchors with 0xC -> 0x8.
#[test]
fn or_anchors_match_singlesteptests() {
    // (file, name-prefix, length) — the (prefix, length) pair picks the clean (even-EA) or odd-EA OR case.
    let anchors: &[(&str, &str, u32)] = &[
        // <ea>,Dn — Dn / (An) / #imm sources, each size.
        ("OR.b.json", "8801", 4), // OR.b D1,D4 — Dn source (.b, 4 cyc, no idle)
        ("OR.b.json", "8614", 8), // OR.b (A4),D3 — (An) memory source ([Read, PF])
        ("OR.b.json", "803c", 8), // OR.b #imm,D0 — immediate source
        ("OR.w.json", "8440", 4), // OR.w D0,D2 — Dn source (.w)
        ("OR.w.json", "8250", 8), // OR.w (A0),D1 — (An) memory source
        ("OR.w.json", "807c", 8), // OR.w #imm,D0 — immediate source
        ("OR.l.json", "8880", 8), // OR.l D0,D4 — Dn source (.l, 8 cyc reg idle)
        ("OR.l.json", "8294", 14), // OR.l (A4),D1 — (An) memory source (.l, [r.hi, r.lo, PF])
        ("OR.l.json", "86bc", 16), // OR.l #imm,D3 — immediate.l source (2 imm words)
        // Dn,<ea> — memory dest, each size (the arith_dn_ea RMW).
        ("OR.b.json", "8f12", 12), // OR.b D7,(A2) — byte memory dest ([r, PF, w])
        ("OR.w.json", "8f54", 12), // OR.w D7,(A4) — word memory dest
        ("OR.l.json", "8b92", 20), // OR.l D5,(A2) — long memory dest (reversed long store)
        // Odd-EA address-error frames (word/long only — byte never faults).
        ("OR.w.json", "8f50", 50), // OR.w D7,(A0) odd dest → group-0 14-byte frame (write fault, low5=0x05)
        ("OR.l.json", "8494", 50), // OR.l (A4),D2 odd source → 14-byte frame (read fault, low5=0x15)
    ];
    let mut found = 0usize;
    for (fname, prefix, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap().starts_with(prefix)
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("L3 OR anchor {prefix} (len {length}) not found in {fname}"));
        // Every anchor must be the genuine register-form OR (high nibble 0x8, in scope) classified by OPCODE —
        // never the ORI immediate contaminant (high nibble 0) and never a CMP-class opcode.
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode >> 12,
            0x8,
            "anchor {prefix} must be a genuine OR opcode"
        );
        assert!(
            and_or_in_scope(opcode),
            "anchor {prefix} must be an in-scope genuine register-form OR"
        );
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "OR is not a CMP-class opcode"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all L3 OR anchors exercised");
    eprintln!(
        "L3 OR anchors: {found} cases (<ea>,Dn Dn/(An)/#imm + Dn,(An) dest, each size + odd-EA) passed both drivers"
    );
}

/// L4 — the named `EOR` anchors, pinning each shape of bitwise EOR (the `Dn,<ea>` direction ONLY) against the
/// vendored EOR.b/.w/.l stream WITHOUT relying on the bulk `covered()` sweep: `Dn,Dn` **register dest** (each
/// size b/w/l — including the load-bearing `EOR.l Dn,Dn` 8-cyc / trailing-n4 case); `Dn,(An)` **memory dest**
/// (each size); and two **odd-EA** cases (the word/long memory access faults into the group-0 14-byte
/// address-error frame on the RMW READ, low5 = 0x15 — byte never faults). Each runs both drivers + the
/// per-cycle transaction stream via `run_case`. The load-bearing pins: EOR sets N = msb / Z = (result == 0),
/// clears V/C, **PRESERVES X** (every case); the register-dest `Dn,Dn` uses its own no-memory arm (`.l` carries
/// a trailing n4) and the memory-dest `Dn,<ea>` reuses `arith_dn_ea` (= ADD Dn,<ea> RMW). Every anchor must
/// classify **by OPCODE** as the genuine register form (high nibble 0xB, via `eor_in_scope`) and NOT as a
/// CMP-class opcode — the *I (EORI) contaminant (high nibble 0) is never an anchor, and EOR has NO `<ea>,Dn`.
#[test]
fn eor_anchors_match_singlesteptests() {
    // (file, name-prefix, length) — the (prefix, length) pair picks the clean (even-EA) or odd-EA EOR case.
    let anchors: &[(&str, &str, u32)] = &[
        // Dn,Dn — register dest, each size (the eor_recipe mode-0 no-memory arm).
        ("EOR.b.json", "b504", 4), // EOR.b D2,D4 — register dest (.b, 4 cyc, no idle)
        ("EOR.w.json", "b744", 4), // EOR.w D3,D4 — register dest (.w, 4 cyc)
        ("EOR.l.json", "b782", 8), // EOR.l D3,D2 — register dest (.l, 8 cyc, trailing n4)
        // Dn,(An) — memory dest, each size (the arith_dn_ea RMW).
        ("EOR.b.json", "b312", 12), // EOR.b D1,(A2) — byte memory dest ([r, PF, w])
        ("EOR.w.json", "b153", 12), // EOR.w D0,(A3) — word memory dest
        ("EOR.l.json", "bb91", 20), // EOR.l D5,(A1) — long memory dest (reversed long store)
        // Odd-EA address-error frames (word/long only — byte never faults; fault on the RMW READ, low5 = 0x15).
        ("EOR.w.json", "b154", 50), // EOR.w D0,(A4) odd dest → group-0 14-byte frame (read fault)
        ("EOR.l.json", "b596", 50), // EOR.l D2,(A6) odd dest → 14-byte frame (read fault)
    ];
    let mut found = 0usize;
    for (fname, prefix, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap().starts_with(prefix)
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| {
                panic!("L4 EOR anchor {prefix} (len {length}) not found in {fname}")
            });
        // Every anchor must be the genuine register-form EOR (high nibble 0xB, in scope) classified by OPCODE —
        // never the EORI immediate contaminant (high nibble 0) and never a CMP-class opcode.
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode >> 12,
            0xB,
            "anchor {prefix} must be a genuine EOR opcode"
        );
        assert!(
            eor_in_scope(opcode),
            "anchor {prefix} must be an in-scope genuine register-form EOR"
        );
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "EOR is not a CMP-class opcode"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all L4 EOR anchors exercised");
    eprintln!(
        "L4 EOR anchors: {found} cases (Dn,Dn register dest + Dn,(An) memory dest, each size + odd-EA) passed both drivers"
    );
}

/// G0 — the named `NEG <ea>` anchors, pinning each shape of the data-alterable negate against the vendored
/// NEG.b/.w/.l stream WITHOUT relying on the bulk `covered()` sweep. NEG is `res = (0 − d) & mask` with FULL
/// subtract flags (it is literally `0 − d`): N = msb(res), Z = (res == 0), **V = (d == sign-min)** (the
/// 0-minus-itself overflow), **C = X = (d != 0)** borrow — byte-identical to `Sub(0, d)` via `AluOp::Neg`. The
/// load-bearing pins: the **d == sign-min** case (`4405 [NEG.b D5] 359`, d.b = 0x80 → 0x80 with **V = 1**, N =
/// C = X = 1) and the **d == 0** case (`4401 [NEG.b D1] 1689`, d = 0 → 0 with **Z = 1, C = X = 0**); the `Dn`
/// register forms across sizes (NEG.l Dn = 6 cyc with the trailing n2, NEG.b/.w Dn = 4); the `(An)` memory
/// READ-then-WRITE RMW (`.w` = 12 cyc `[r, PF, w]`, `.l` = 20 cyc reversed long store) — the SAME `ea_dst`/
/// `ea_dst_long` path as CLR, EXCEPT the read operand is the unary source (not discarded); and an **odd-EA**
/// case (`4459 [NEG.w (A1)+]`, len 50) faulting on the RMW READ into the group-0 14-byte address-error frame.
/// Each runs both drivers + the per-cycle transaction stream via `run_case`. Every anchor must decode as a NEG
/// opcode (mask 0xFFC0 ∈ {0x4400, 0x4440, 0x4480}, SS != 3 = not the 0x44C0 MOVE-to-CCR) — never a CMP-class.
#[test]
fn neg_anchors_match_singlesteptests() {
    // (file, name-prefix, length) — the (prefix, length) pair picks the case. The V=1 / Z=1 pins use the FULL
    // (unique) name to select the exact sign-min / zero operand; the rest use the opcode-hex prefix.
    let anchors: &[(&str, &str, u32)] = &[
        ("NEG.b.json", "4401 [NEG.b D1] 1689", 4), // NEG.b D1, d = 0 → Z = 1, C = X = 0
        ("NEG.b.json", "4405 [NEG.b D5] 359", 4),  // NEG.b D5, d = 0x80 (sign-min) → V = 1 overflow
        ("NEG.w.json", "4445", 4),                 // NEG.w D5 — register word (4 cyc, no idle)
        ("NEG.l.json", "4482", 6),                 // NEG.l D2 — register long (6 cyc, trailing n2)
        ("NEG.w.json", "4456", 12),                // NEG.w (A6) — memory RMW ([r, PF, w])
        ("NEG.l.json", "4495", 20), // NEG.l (A5) — long memory RMW (reversed long store)
        ("NEG.w.json", "4459 [NEG.w (A1)+]", 50), // NEG.w (A1)+ odd EA → group-0 14-byte frame (read fault)
    ];
    let mut found = 0usize;
    for (fname, prefix, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap().starts_with(prefix)
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| {
                panic!("G0 NEG anchor {prefix} (len {length}) not found in {fname}")
            });
        // Every anchor must be a NEG opcode (mask 0xFFC0 ∈ {0x4400, 0x4440, 0x4480}, SS != 3) — never a
        // CMP-class opcode.
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert!(
            matches!(opcode & 0xFFC0, 0x4400 | 0x4440 | 0x4480),
            "anchor {prefix} must be a NEG opcode"
        );
        assert_ne!(opcode & 0xC0, 0xC0, "anchor {prefix} must not be SS == 3");
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "NEG is not a CMP-class opcode"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all G0 NEG anchors exercised");
    eprintln!(
        "G0 NEG anchors: {found} cases (Dn each size incl. V=1 sign-min + Z=1 zero, (An) RMW .w/.l, odd-EA frame) passed both drivers"
    );
}

/// G1 — the named `NEGX <ea>` anchors, pinning the **load-bearing subtlety** of the family (sticky Z + X-in)
/// against the vendored NEGX.b/.w/.l stream WITHOUT relying on the bulk `covered()` sweep. NEGX is
/// `res = (0 − d − X_in) & mask` with SUBX-style flags: N = msb(res), **Z is STICKY — `Z_final = Z_in AND
/// (res == 0)`** (NEGX never SETS Z, only clears it — the multi-precision idiom), V = `(d & res & signbit) != 0`,
/// **C = X = NOT(d == 0 AND X_in == 0)** (the borrow of `0 − d − X_in`), where `X_in = (sr >> 4) & 1` and
/// `Z_in = (sr >> 2) & 1` participate in the value AND the borrow (`AluOp::Negx`). The pins, each chosen to break
/// a *plain* `Z = (res == 0)` implementation and to exercise X-in in both the value and the borrow:
/// - `4001 [NEGX.b D1] 13` — `X_in = 1, Z_in = 1, d = 0xF3 → res = 0x0C != 0`: **Z goes to 0** (sticky cleared),
///   N = 0, C = X = 1 (borrow). X-in feeds the value (`0 − 0xF3 − 1 = 0x0C`).
/// - `4004 [NEGX.b D4] 1630` — `Z_in = 1, d = 0 (X_in = 0) → res = 0`: **Z STAYS 1** (kept from `Z_in`), C = X = 0
///   (the lone no-borrow case: `d == 0 && X_in == 0`).
/// - `4000 [NEGX.b D0] 197` — `Z_in = 0, d = 0 (X_in = 0) → res = 0`: **Z STAYS 0** — the case a plain
///   `Z = (res == 0)` gets WRONG (it would set Z = 1); the sticky `Z_in AND (res == 0)` keeps it 0.
/// - `4046 [NEGX.w D6] 27` — `X_in = 1, d = 0x3AEE → res = 0xC511 != 0`: word register (4 cyc), C = X = 1 borrow,
///   N = 1.
/// - `4092 [NEGX.l (A2)] 17` (len 20) — `(An)` memory `.l` READ-then-WRITE RMW (reversed long store) — the SAME
///   `ea_dst`/`ea_dst_long` path as NEG/CLR (the read operand is the unary source).
/// - `405a [NEGX.w (A2)+] 3` (len 50) — odd-EA → faults on the RMW READ into the group-0 14-byte address-error
///   frame (in scope, no parity filter).
///
/// Each runs both drivers + the per-cycle transaction stream via `run_case`. Every anchor must decode as a NEGX
/// opcode (mask 0xFFC0 ∈ {0x4000, 0x4040, 0x4080}, SS != 3 = not the 0x40C0 MOVE-from-SR) — never a CMP-class.
#[test]
fn negx_anchors_match_singlesteptests() {
    // (file, full-name, length). The sticky-Z / borrow pins use the FULL (unique) name to select the EXACT
    // X_in / Z_in / operand combination the doc-comment describes; a plain `Z = (res == 0)` mishandles the
    // `4000 [NEGX.b D0] 197` (Z_in = 0, res == 0 → Z stays 0) anchor.
    let anchors: &[(&str, &str, u32)] = &[
        ("NEGX.b.json", "4001 [NEGX.b D1] 13", 4), // X_in=1 Z_in=1 res!=0 → Z→0, C=X=1
        ("NEGX.b.json", "4004 [NEGX.b D4] 1630", 4), // Z_in=1 res==0 → Z stays 1, C=X=0
        ("NEGX.b.json", "4000 [NEGX.b D0] 197", 4), // Z_in=0 res==0 → Z stays 0 (breaks plain res==0)
        ("NEGX.w.json", "4046 [NEGX.w D6] 27", 4),  // X_in=1 d!=0 → word reg, C=X=1 borrow
        ("NEGX.l.json", "4092 [NEGX.l (A2)] 17", 20), // (An) memory .l RMW (reversed long store)
        ("NEGX.w.json", "405a [NEGX.w (A2)+] 3", 50), // odd-EA → group-0 14-byte frame (read fault)
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("G1 NEGX anchor {name} (len {length}) not found in {fname}"));
        // Every anchor must be a NEGX opcode (mask 0xFFC0 ∈ {0x4000, 0x4040, 0x4080}, SS != 3) — never a
        // CMP-class opcode.
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert!(
            matches!(opcode & 0xFFC0, 0x4000 | 0x4040 | 0x4080),
            "anchor {name} must be a NEGX opcode"
        );
        assert_ne!(opcode & 0xC0, 0xC0, "anchor {name} must not be SS == 3");
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "NEGX is not a CMP-class opcode"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all G1 NEGX anchors exercised");
    eprintln!(
        "G1 NEGX anchors: {found} cases (sticky-Z: Z→0 / Z-stays-1 / Z-stays-0, X-in borrow C=X, (An) .l RMW, odd-EA frame) passed both drivers"
    );
}

/// G2 — the named `NOT <ea>` anchors, pinning each shape of the data-alterable bitwise complement against the
/// vendored NOT.b/.w/.l stream WITHOUT relying on the bulk `covered()` sweep. NOT is `res = (~d) & mask` with
/// LOGIC flags (the SAME MOVE flag shape as AND/OR/EOR): N = msb(res), Z = (res == 0), **V = 0, C = 0, X
/// PRESERVED** (logic never touches X — the live X is re-injected as `ccr_nz | (sr & CCR_X)`, never computed)
/// via the new `AluOp::Not`. NOT REUSES the shared `neg_family_recipe` VERBATIM (the read-then-write RMW shape
/// is identical to NEG/NEGX/CLR's `ea_dst`/`ea_dst_long`; only the `AluOp` exec differs — `~a` instead of a
/// subtraction). The load-bearing pins (each entering with **X = 1** to confirm X is KEPT while V/C are
/// CLEARED and N/Z recomputed):
/// - `4600 [NOT.b D0] 25` — `Dn.b` register (4 cyc): enters X1 V1, exits X1 V0 C0 (X kept, V cleared).
/// - `4640 [NOT.w D0] 14` — `Dn.w` register (4 cyc): enters X1 V1 C1, exits X1 V0 C0 (X kept, V/C cleared).
/// - `4681 [NOT.l D1] 2` (len 6) — `Dn.l` register (6 cyc, trailing n2): enters X1 N1 Z1 V1 C1, exits
///   X1 N0 Z0 V0 C0 (X kept, N/Z recomputed from the result, V/C cleared).
/// - `4653 [NOT.w (A3)] 15` (len 12) — `(An)` memory `.w` READ-then-WRITE RMW (`[r, PF, w]`) — the SAME
///   `ea_dst` path as NEG/NEGX/CLR (the read operand is the unary source).
/// - `4652 [NOT.w (A2)] 27` (len 50) — odd-EA → faults on the RMW READ into the group-0 14-byte address-error
///   frame (in scope, no parity filter).
///
/// Each runs both drivers + the per-cycle transaction stream via `run_case`. Every anchor must decode as a NOT
/// opcode (mask 0xFFC0 ∈ {0x4600, 0x4640, 0x4680}, SS != 3 = not the 0x46C0 form) — never a CMP-class.
#[test]
fn not_anchors_match_singlesteptests() {
    // (file, full-name, length). The X-preservation pins use the FULL (unique) name to select the EXACT
    // X_in / V_in / C_in combination the doc-comment describes (each enters with X = 1).
    let anchors: &[(&str, &str, u32)] = &[
        ("NOT.b.json", "4600 [NOT.b D0] 25", 4), // X1 V1 in → X1 V0 C0 out (X kept, V cleared)
        ("NOT.w.json", "4640 [NOT.w D0] 14", 4), // X1 V1 C1 in → X1 V0 C0 out (X kept, V/C cleared)
        ("NOT.l.json", "4681 [NOT.l D1] 2", 6), // X1 N1 Z1 V1 C1 in → X1 N0 Z0 V0 C0 (6 cyc trailing n2)
        ("NOT.w.json", "4653 [NOT.w (A3)] 15", 12), // (An) memory .w RMW ([r, PF, w])
        ("NOT.w.json", "4652 [NOT.w (A2)] 27", 50), // odd-EA → group-0 14-byte frame (read fault)
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("G2 NOT anchor {name} (len {length}) not found in {fname}"));
        // Every anchor must be a NOT opcode (mask 0xFFC0 ∈ {0x4600, 0x4640, 0x4680}, SS != 3) — never a
        // CMP-class opcode.
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert!(
            matches!(opcode & 0xFFC0, 0x4600 | 0x4640 | 0x4680),
            "anchor {name} must be a NOT opcode"
        );
        assert_ne!(opcode & 0xC0, 0xC0, "anchor {name} must not be SS == 3");
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "NOT is not a CMP-class opcode"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all G2 NOT anchors exercised");
    eprintln!(
        "G2 NOT anchors: {found} cases (Dn each size with X-preservation pin X kept + V/C cleared, (An) .w RMW, odd-EA frame) passed both drivers"
    );
}

/// G3 — the named `EXT.w` / `EXT.l` / `SWAP` anchors (the FINAL commit of this push), pinning each `Dn`-only
/// transform against the vendored stream WITHOUT relying on the bulk `covered()` sweep. All three are
/// **logic-shaped** (N = result-msb, Z = (result == 0), V = 0, C = 0, X PRESERVED — re-injected `ccr_nz | (sr &
/// CCR_X)`, never computed) via the new unary `AluOp::Ext` / `AluOp::Swap`. The load-bearing pins (each entering
/// with **X = 1** and at least one of **V/C set** to confirm X is KEPT while V/C are CLEARED and N/Z are
/// recomputed from the result):
/// - `4884 [EXT.w D4] 7` — EXT.w D4 with the byte HIGH BIT set: `d = 0xd84abfae`, low byte `0xae` sign-extends
///   to `0xffae` → result `0xd84affae` (the high word `0xd84a` is PRESERVED). Enters X1 N1 Z1 V1 C0, exits
///   X1 N1 Z0 V0 C0 (X kept, N from bit15 of the word, V cleared). The width follows the size (LOW WORD write).
/// - `48c6 [EXT.l D6] 3` — EXT.l D6 with the word HIGH BIT set: `d = 0x2f8d86a1`, low word `0x86a1`
///   sign-extends to `0xffff86a1` → result `0xffff86a1` (FULL 32). Enters X1 N1 Z1 V0 C1, exits X1 N1 Z0 V0 C0
///   (X kept, N from bit31, C cleared).
/// - `4844 [SWAP D4] 5` — SWAP D4 with DISTINCT halves: `d = 0xf93bedf2` → `0xedf2f93b` (the swapped-in low
///   half `0xedf2` puts bit31 = 1). Enters X1 N0 Z0 V1 C1, exits X1 N1 Z0 V0 C0 (X kept, N from the swapped-in
///   bit31, V/C cleared).
///
/// Each runs both drivers + the per-cycle transaction stream via `run_case`. Every anchor must decode as an
/// EXT/SWAP opcode (mask 0xFFF8 ∈ {0x4880, 0x48C0, 0x4840}) — never a CMP-class — and is length 4 (one
/// Prefetch, no idle, no memory; `Dn`-only, no fault possible).
#[test]
fn ext_swap_anchors_match_singlesteptests() {
    // (file, full-name, length). The X-preservation pins use the FULL (unique) name to select the EXACT
    // d / X_in / V_in / C_in combination the doc-comment describes (each enters with X = 1 and V or C set).
    let anchors: &[(&str, &str, u32)] = &[
        ("EXT.w.json", "4884 [EXT.w D4] 7", 4), // byte high-bit → 0xFFxx, high word kept, N1, X1 V1 in → V0
        ("EXT.l.json", "48c6 [EXT.l D6] 3", 4), // word high-bit → N from bit31, X1 C1 in → C0
        ("SWAP.json", "4844 [SWAP D4] 5", 4), // distinct halves → N from swapped-in bit31, X1 V1 C1 in → V0 C0
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| {
                panic!("G3 EXT/SWAP anchor {name} (len {length}) not found in {fname}")
            });
        // Every anchor must be an EXT/SWAP opcode (mask 0xFFF8 ∈ {0x4880, 0x48C0, 0x4840}) — never a CMP-class.
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert!(
            matches!(opcode & 0xFFF8, 0x4880 | 0x48C0 | 0x4840),
            "anchor {name} must be an EXT/SWAP opcode"
        );
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "EXT/SWAP is not a CMP-class opcode"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all G3 EXT/SWAP anchors exercised");
    eprintln!(
        "G3 EXT/SWAP anchors: {found} cases (EXT.w byte→word high word preserved, EXT.l word→long, SWAP distinct halves; each X-preservation pin X kept + V/C cleared) passed both drivers"
    );
}

/// C0 — the named `Scc <ea>` anchors, pinning each shape of the conditional byte set against the vendored Scc
/// stream WITHOUT relying on the bulk `covered()` sweep. The condition-determined value is pinned with the two
/// flag-INDEPENDENT encodings — `ST` (cc 0 = T, ALWAYS 0xFF) and `SF` (cc 1 = F, ALWAYS 0x00) — so the written
/// byte is fixed by the OPCODE alone (not by the case's flags):
/// - `50c4 [Scc D4] 858` — **ST D4**, condition TRUE: `d4 = 0x2CC260E3` → `0x2CC260FF` (the upper 24 bits are
///   PRESERVED), length **6** (the taken `Internal(2)` idle). The load-bearing true/false timing difference.
/// - `51c5 [Scc D5] 680` — **SF D5**, condition FALSE: `d5 = 0x30E9E7D6` → `0x30E9E700` (0x00, upper 24 kept),
///   length **4** (no trailing idle).
/// - `50d3 [Scc (A3)] 63` — **ST (A3)** memory: read the EA byte (discarded), refill, write **0xFF** —
///   `[r .b @EA, r .w refill, w .b 0xFF @EA]`, length 12 (CLR's exact byte RMW timing).
/// - `51d3 [Scc (A3)] 1265` — **SF (A3)** memory: same RMW writing **0x00**.
/// - `50d7 [Scc (A7)] 312` — **ST (A7)** the plain `(A7)` mode-2 indirect (`mode == 2 && reg == 7`), COVERED
///   (NOT deferred): the byte RMW at the active A7, writing 0xFF.
///
/// Each runs both drivers + the per-cycle transaction stream via `run_case`. The load-bearing pins: Scc sets
/// **NO flags** (`final.sr == initial.sr`, verified by `run_case` against the data on every case), the Dn TRUE
/// form is 6 cyc / FALSE is 4 cyc, the memory form READS before it WRITES (it is not write-only) and the byte
/// write is condition-independent. Every anchor must decode as a Scc opcode (0xF0C0 == 0x50C0, NOT the 0x50C8
/// DBcc form) and never a CMP-class opcode.
#[test]
fn scc_anchors_match_singlesteptests() {
    // (file, full-name, length). The ST (cc 0) / SF (cc 1) encodings fix the written byte by opcode alone; the
    // Dn anchors' lengths (6 vs 4) pin the only true/false timing difference. Full names select the EXACT case
    // (the Dn TRUE case has the upper-24-nonzero `d4` the comment describes).
    let anchors: &[(&str, &str, u32)] = &[
        ("Scc.json", "50c4 [Scc D4] 858", 6), // ST D4 TRUE → 0xFF, upper 24 preserved, len 6 (taken idle)
        ("Scc.json", "51c5 [Scc D5] 680", 4), // SF D5 FALSE → 0x00, len 4 (no idle)
        ("Scc.json", "50d3 [Scc (A3)] 63", 12), // ST (A3) memory RMW → 0xFF
        ("Scc.json", "51d3 [Scc (A3)] 1265", 12), // SF (A3) memory RMW → 0x00
        ("Scc.json", "50d7 [Scc (A7)] 312", 12), // ST (A7) mode-2 indirect (COVERED, not deferred) → 0xFF
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("C0 Scc anchor {name} (len {length}) not found in {fname}"));
        // Every anchor must be a Scc opcode (0xF0C0 == 0x50C0) and NOT the 0x50C8 DBcc mode-001 form, never a
        // CMP-class opcode. Also pin that the SR is unchanged (Scc sets NO flags).
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode & 0xF0C0,
            0x50C0,
            "anchor {name} must be a Scc opcode"
        );
        assert_ne!(
            opcode & 0xF0F8,
            0x50C8,
            "anchor {name} must NOT be the DBcc mode-001 form"
        );
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "Scc is not a CMP-class opcode"
        );
        assert_eq!(
            case["initial"]["sr"], case["final"]["sr"],
            "Scc sets NO flags — SR unchanged [{name}]"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all C0 Scc anchors exercised");
    eprintln!(
        "C0 Scc anchors: {found} cases (Dn TRUE 0xFF len6 / Dn FALSE 0x00 len4, (An) RMW 0xFF & 0x00, (A7) mode-2 indirect; SR unchanged on every case) passed both drivers"
    );
}

/// C0 — the snapshot/restore anchor for the Scc memory-dest byte RMW (the new `SetByte` parked in `Scratch`,
/// then written back). Drives a real vendored `Scc (A3)` case (`[Read, Prefetch, SetByte, Write]`, 4 micro-ops)
/// through the quiesce driver, snapshotting + restoring the WHOLE `Cpu68000` (incl. the in-flight cursor) at
/// every micro-op boundary — including the interesting mid-bus-access boundary between the operand Read and the
/// result Write — and proves the resumed run reproduces the run-to-completion final state + transaction stream
/// bit-for-bit. This pins that `MicroOp::SetByte` keeps `MicroState` fixed-size bincode (it is `Copy`).
#[test]
fn scc_an_quiescable_and_serializable_at_every_micro_op_boundary() {
    let path = format!("{VENDOR_DIR}/Scc.json");
    if !Path::new(&path).exists() {
        eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
        return;
    }
    let file = std::fs::File::open(&path).unwrap();
    let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
    let case = data
        .iter()
        .find(|t| t["name"].as_str().unwrap() == "50d3 [Scc (A3)] 63")
        .expect("Scc (A3) snapshot anchor present");
    let ini = &case["initial"];

    // Run-to-completion reference.
    let mut rref = Cpu68000::new(build_regs(ini));
    let mut bref = build_bus(ini);
    rref.run_instruction(&mut bref);

    let cfg = bincode::config::standard();
    // 4 micro-ops (Read, Prefetch, SetByte, Write) → in-flight boundaries after 0..=3 of them.
    for pause_after in 0..=3 {
        let mut cpu = Cpu68000::new(build_regs(ini));
        let mut bus = build_bus(ini);
        cpu.start_instruction();
        for _ in 0..pause_after {
            assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        }
        // Snapshot + restore the whole CPU (incl. the in-flight cursor) mid-instruction.
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        loop {
            if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                break;
            }
        }
        assert_eq!(
            cpu2.regs, rref.regs,
            "resume from boundary {pause_after} diverged"
        );
        assert_eq!(
            bus.log, bref.log,
            "transaction stream from boundary {pause_after} diverged"
        );
    }
    eprintln!(
        "C0 Scc snapshot/restore: Scc (A3) byte RMW resumed identically at every micro-op boundary"
    );
}

/// C1 — the named `TAS Dn` anchors, pinning the register form of the indivisible test-and-set against the
/// vendored TAS stream WITHOUT relying on the bulk `covered()` sweep. The load-bearing subtlety: the flags are
/// computed on the byte READ (the INPUT), but the WRITTEN value is `input | 0x80` (bit 7 ALWAYS set) — DISTINCT
/// (unlike NOT, whose flags are on the result `~a`). X is PRESERVED, V/C cleared. Every TAS Dn is **4 cyc** (a
/// single Prefetch refill; the Alu is a 0-cycle internal compute, no bus access).
/// - `4ac5 [TAS D5] 54` — bit7 ALREADY set + X PRESERVATION: `d5 = 0xeaab87c0`, low byte `0xc0` → `0xc0 | 0x80
///   == 0xc0` (the low byte is UNCHANGED, the upper 24 `0xeaab87` PRESERVED). Enters X1 N0 Z1 V1 C1, exits X1
///   N1 Z0 V0 C0 (X kept, N = bit7(input) = 1, V/C cleared).
/// - `4ac2 [TAS D2] 3359` — low byte == 0: `d2 = 0xb3cb1000`, `0x00 | 0x80 == 0x80` written (the flag input
///   0x00 DIFFERS from the written 0x80), Z = 1, N = 0, upper 24 `0xb3cb10` PRESERVED.
/// - `4ac1 [TAS D1] 36` — X PRESERVATION pin entering with EVERY CCR bit set: `d1 = 0x16b7ad4a`, low byte
///   `0x4a` → `0x4a | 0x80 == 0xca`. Enters X1 N1 Z1 V1 C1, exits X1 (only) — X kept while N/Z are recomputed
///   from the input byte (N = 0, Z = 0) and V/C cleared.
///
/// Each runs both drivers + the per-cycle transaction stream via `run_case`. Every anchor must decode as a TAS
/// opcode (0xFFC0 == 0x4AC0, mode 000) — never a CMP-class — and is length 4.
#[test]
fn tas_dn_anchors_match_singlesteptests() {
    // (file, full-name, length). The full names select the EXACT case (the specific d / X_in / V_in / C_in the
    // doc-comment describes). All TAS Dn are length 4 (one Prefetch, the Alu a 0-cycle compute, no memory).
    let anchors: &[(&str, &str, u32)] = &[
        ("TAS.json", "4ac5 [TAS D5] 54", 4), // bit7 set → N1, write keeps bit7 (df==di), X1 kept, V1C1 → V0C0
        ("TAS.json", "4ac2 [TAS D2] 3359", 4), // low byte 0 → Z1, write 0x80, upper 24 preserved
        ("TAS.json", "4ac1 [TAS D1] 36", 4), // X-preservation: X1 N1 Z1 V1 C1 in → X1 only (V/C cleared)
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("C1 TAS anchor {name} (len {length}) not found in {fname}"));
        // Every anchor must be a TAS opcode (0xFFC0 == 0x4AC0), mode 000 (Dn), and never a CMP-class opcode.
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode & 0xFFC0,
            0x4AC0,
            "anchor {name} must be a TAS opcode"
        );
        assert_eq!(
            (opcode >> 3) & 7,
            0,
            "anchor {name} must be the Dn (mode 000) form"
        );
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "TAS is not a CMP-class opcode"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all C1 TAS anchors exercised");
    eprintln!(
        "C1 TAS anchors: {found} cases (bit7-set → N1 / write keeps bit7, byte==0 → Z1 / write 0x80, X-preservation X kept + V/C cleared; flags on the READ byte, write input|0x80) passed both drivers"
    );
}

/// C1 — the snapshot/restore anchor for the `TAS Dn` register form (the new `AluOp::Tas`). Drives a real
/// vendored `TAS D5` case (`[Prefetch, Alu]`, 2 micro-ops) through the quiesce driver, snapshotting + restoring
/// the WHOLE `Cpu68000` (incl. the in-flight cursor) at every micro-op boundary and proving the resumed run
/// reproduces the run-to-completion final state + transaction stream bit-for-bit. This pins that `AluOp::Tas`
/// keeps `MicroState` fixed-size bincode.
#[test]
fn tas_dn_quiescable_and_serializable_at_every_micro_op_boundary() {
    let path = format!("{VENDOR_DIR}/TAS.json");
    if !Path::new(&path).exists() {
        eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
        return;
    }
    let file = std::fs::File::open(&path).unwrap();
    let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
    let case = data
        .iter()
        .find(|t| t["name"].as_str().unwrap() == "4ac5 [TAS D5] 54")
        .expect("TAS D5 snapshot anchor present");
    let ini = &case["initial"];

    // Run-to-completion reference.
    let mut rref = Cpu68000::new(build_regs(ini));
    let mut bref = build_bus(ini);
    rref.run_instruction(&mut bref);

    let cfg = bincode::config::standard();
    // 2 micro-ops (Prefetch, Alu) → in-flight boundaries after 0..=1 of them.
    for pause_after in 0..=1 {
        let mut cpu = Cpu68000::new(build_regs(ini));
        let mut bus = build_bus(ini);
        cpu.start_instruction();
        for _ in 0..pause_after {
            assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        }
        // Snapshot + restore the whole CPU (incl. the in-flight cursor) mid-instruction.
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        loop {
            if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                break;
            }
        }
        assert_eq!(
            cpu2.regs, rref.regs,
            "resume from boundary {pause_after} diverged"
        );
        assert_eq!(
            bus.log, bref.log,
            "transaction stream from boundary {pause_after} diverged"
        );
    }
    eprintln!("C1 TAS snapshot/restore: TAS D5 resumed identically at every micro-op boundary");
}

/// C2 — the named `TAS <ea>` MEMORY anchors, pinning the atomic indivisible read-modify-write against the
/// vendored TAS stream WITHOUT relying on the bulk `covered()` sweep. The load-bearing facts: TAS memory is
/// ONE atomic `'t'` transaction (10 cyc, value = the WRITTEN byte `orig | 0x80`), NOT a separate `'r'`+`'w'`
/// pair; the flags come from the READ byte (`orig`) — N = bit7(orig) / Z = (orig == 0), V/C cleared, X
/// PRESERVED. Each anchor pins the EXACT per-cycle bus stream + cycle count (= CLR + 2 everywhere):
/// - `4ad2 [TAS (A2)] 1` — `(An)`: bus `t, r`, len 14. orig 0x35 → written 0xB5 (`'t'` value 181), N0 Z0.
/// - `4ad7 [TAS (A7)] 23` — `(A7)` mode-2 indirect (COVERED, NOT deferred): bus `t@A7, r`, len 14 — the
///   atomic `[t@A7, prefetch]`, no increment.
/// - `4ae2 [TAS -(A2)] 8` — `-(An)`: bus `n, t, r`, len 16 (the predecrement idle, then the atomic RMW).
/// - `4aea [TAS (d16, A2)] 2` — `d16(An)`: bus `r, t, r`, len 18.
/// - `4af9 [TAS (xxx).l] 51` — `abs.l`: bus `r, r, t, r`, len 22 (the two-word address assembly, then the
///   atomic RMW, then the final refill).
///
/// Each runs both drivers + the per-cycle transaction stream via `run_case` (the `'t'` token maps to
/// `TxKind::Tas`). Every anchor must decode as a TAS opcode (0xFFC0 == 0x4AC0), a MEMORY mode (mode != 0),
/// never a CMP-class.
#[test]
fn tas_memory_anchors_match_singlesteptests() {
    // (file, full-name, length, expected mode). All TAS memory bus streams contain EXACTLY one `'t'`.
    let anchors: &[(&str, &str, u32, u16)] = &[
        ("TAS.json", "4ad2 [TAS (A2)] 1", 14, 2),      // (An): t, r
        ("TAS.json", "4ad7 [TAS (A7)] 23", 14, 2),     // (A7) m2 indirect (covered): t@A7, r
        ("TAS.json", "4ae2 [TAS -(A2)] 8", 16, 4),     // -(An): n, t, r
        ("TAS.json", "4aea [TAS (d16, A2)] 2", 18, 5), // d16(An): r, t, r
        ("TAS.json", "4af9 [TAS (xxx).l] 51", 22, 7),  // abs.l: r, r, t, r
    ];
    let mut found = 0usize;
    for (fname, name, length, want_mode) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| {
                panic!("C2 TAS memory anchor {name} (len {length}) not found in {fname}")
            });
        // Every anchor must be a TAS opcode (0xFFC0 == 0x4AC0), the expected MEMORY mode, never CMP-class.
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode & 0xFFC0,
            0x4AC0,
            "anchor {name} must be a TAS opcode"
        );
        assert_eq!((opcode >> 3) & 7, *want_mode, "anchor {name} EA mode");
        assert_ne!(
            (opcode >> 3) & 7,
            0,
            "anchor {name} must be a MEMORY form (mode != 0)"
        );
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "TAS is not a CMP-class opcode"
        );
        // The vendored stream must contain EXACTLY ONE `'t'` (the atomic RMW), never an `'r'`+`'w'` pair.
        let t_count = case["transactions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|tr| tr.as_array().unwrap()[0].as_str().unwrap() == "t")
            .count();
        assert_eq!(
            t_count, 1,
            "anchor {name} has exactly one atomic `'t'` transaction"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all C2 TAS memory anchors exercised");
    eprintln!(
        "C2 TAS memory anchors: {found} cases ((An) t,r 14 / (A7) m2 t@A7 14 / -(An) n,t,r 16 / d16(An) r,t,r 18 / abs.l r,r,t,r 22; ONE atomic Tas txn value=orig|0x80, flags from the read byte) passed both drivers"
    );
}

/// C2 — the snapshot/restore anchor at the atomic-RMW boundary for TAS MEMORY (the new `TxKind::Tas` /
/// `Bus68k::tas` / `MicroOp::TasRmw` / `ea_tas`). Drives the real vendored `4af9 [TAS (xxx).l]` case (`abs.l`,
/// 6 micro-ops `[EaCalc(HI), Prefetch, EaCalc(ADDR), Prefetch, TasRmw, Prefetch]`) through the quiesce
/// driver, snapshotting + restoring the WHOLE `Cpu68000` (incl. the in-flight cursor) at EVERY micro-op
/// boundary — INCLUDING the atomic-RMW boundary (the single locked `Tas` bus access is one quiesce point) —
/// and proving the resumed run reproduces the run-to-completion final state + transaction stream bit-for-bit.
/// Pins that `MicroOp::TasRmw` keeps `MicroState` fixed-size bincode.
#[test]
fn tas_memory_quiescable_and_serializable_at_every_micro_op_boundary() {
    let path = format!("{VENDOR_DIR}/TAS.json");
    if !Path::new(&path).exists() {
        eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
        return;
    }
    let file = std::fs::File::open(&path).unwrap();
    let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
    let case = data
        .iter()
        .find(|t| t["name"].as_str().unwrap() == "4af9 [TAS (xxx).l] 51")
        .expect("TAS abs.l snapshot anchor present");
    let ini = &case["initial"];

    // Run-to-completion reference.
    let mut rref = Cpu68000::new(build_regs(ini));
    let mut bref = build_bus(ini);
    rref.run_instruction(&mut bref);

    let cfg = bincode::config::standard();
    // 6 micro-ops (EaCalc(HI), Prefetch, EaCalc(ADDR), Prefetch, TasRmw, Prefetch) → boundaries after 0..=5.
    for pause_after in 0..=5 {
        let mut cpu = Cpu68000::new(build_regs(ini));
        let mut bus = build_bus(ini);
        cpu.start_instruction();
        for _ in 0..pause_after {
            assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        }
        // Snapshot + restore the whole CPU (incl. the in-flight cursor) mid-instruction.
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        loop {
            if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                break;
            }
        }
        assert_eq!(
            cpu2.regs, rref.regs,
            "resume from boundary {pause_after} diverged"
        );
        assert_eq!(
            bus.log, bref.log,
            "transaction stream from boundary {pause_after} diverged"
        );
    }
    eprintln!(
        "C2 TAS snapshot/restore: TAS abs.l resumed identically at every micro-op boundary (incl the atomic-RMW boundary)"
    );
}

/// B0 — the named `BTST` anchors, pinning the bit test (Z = NOT(bit), READ-ONLY, X/N/V/C + the SR system byte
/// PRESERVED) against the vendored BTST stream WITHOUT relying on the bulk `covered()` sweep. The load-bearing
/// facts: ONLY Z changes; a `Dn` operand is 32-bit (mod 32), a memory/`#imm`/PC-relative operand is 8-bit
/// (mod 8); BTST register timing is FIXED (NO `pos>=16` +2 — that variance is only for the RMW trio); static =
/// dynamic + 4 (the extra bitnum ext word). The four `Dn`-dynamic anchors span the 2×2 of {pos<16, pos>=16} ×
/// {bit=1→Z=0, bit=0→Z=1}, ALL 6 cyc (confirming no register-timing variance):
/// - `0901 [BTST D4, D1] 44` — Dn dynamic, pos 2 (<16), bit 1 → Z 1→0, len 6.
/// - `0107 [BTST D0, D7] 125` — Dn dynamic, pos 25 (>=16), bit 0 → Z 0→1, len 6 (HIGH bit, still 6 — no +2).
/// - `0d00 [BTST D6, D0] 62` — Dn dynamic, pos 9 (<16), bit 0 → Z 0→1, len 6.
/// - `0d02 [BTST D6, D2] 2` — Dn dynamic, pos 24 (>=16), bit 1 → Z 1→0, len 6 (HIGH bit, still 6 — no +2).
/// - `0807 [BTST #, D7] 87` — Dn STATIC, len 10 (dynamic 6 + 4 for the bitnum ext word).
/// - `0115 [BTST D0, (A5)] 4` — `(An)` memory source: bus `r.b, r.w`, len 8 (byte read, mod 8).
/// - `0d3a [BTST D6, (d16, PC)] 16` — `d16(PC)` source: bus `r.w, r.b, r.w`, len 12 (PC-relative byte source).
/// - `073c [BTST D3, #] 91` — `#imm` source: bus `r.w, r.w, n2`, len 10 (the immediate operand, mod 8).
/// - `0f17 [BTST D7, (A7)] 40` — `(A7)` mode-2 indirect (COVERED, NOT deferred): bus `r.b@A7, r.w`, len 8.
///
/// Each runs both drivers + the per-cycle transaction stream via `run_case`. Every anchor must decode as a BTST
/// opcode (dynamic `0xF1C0 == 0x0100` or static `0xFF00 == 0x0800` with tt bits 7-6 == 00) — never a CMP-class.
#[test]
fn btst_anchors_match_singlesteptests() {
    // (file, full-name, length). The full names select the EXACT case the doc-comment describes.
    let anchors: &[(&str, &str, u32)] = &[
        ("BTST.json", "0901 [BTST D4, D1] 44", 6), // Dn dyn pos<16 bit1 → Z 1→0, FIXED 6
        ("BTST.json", "0107 [BTST D0, D7] 125", 6), // Dn dyn pos>=16 bit0 → Z 0→1, FIXED 6 (no +2)
        ("BTST.json", "0d00 [BTST D6, D0] 62", 6), // Dn dyn pos<16 bit0 → Z 0→1, FIXED 6
        ("BTST.json", "0d02 [BTST D6, D2] 2", 6),  // Dn dyn pos>=16 bit1 → Z 1→0, FIXED 6 (no +2)
        ("BTST.json", "0807 [BTST #, D7] 87", 10), // Dn STATIC, 10 = dynamic 6 + 4
        ("BTST.json", "0115 [BTST D0, (A5)] 4", 8), // (An) byte source: r.b, r.w
        ("BTST.json", "0d3a [BTST D6, (d16, PC)] 16", 12), // d16(PC) byte source: r.w, r.b, r.w
        ("BTST.json", "073c [BTST D3, #] 91", 10), // #imm byte operand: r.w, r.w, n2
        ("BTST.json", "0f17 [BTST D7, (A7)] 40", 8), // (A7) m2 indirect (covered): r.b@A7, r.w
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("B0 BTST anchor {name} (len {length}) not found in {fname}"));
        // Every anchor must be a BTST opcode (dynamic 0xF1C0 == 0x0100 OR static 0xFF00 == 0x0800, tt == 00),
        // never a CMP-class opcode.
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        let is_dyn = opcode & 0xF1C0 == 0x0100;
        let is_static = opcode & 0xFF00 == 0x0800 && (opcode >> 6) & 3 == 0;
        assert!(
            is_dyn || is_static,
            "anchor {name} must be a BTST opcode (dynamic 0x0100 / static 0x0800, tt == 00)"
        );
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "BTST is not a CMP-class opcode"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all B0 BTST anchors exercised");
    eprintln!(
        "B0 BTST anchors: {found} cases (Dn dyn 2×2 bit×pos all 6 cyc / Dn static 10 / (An) / d16(PC) / #imm / (A7) m2; only Z changes, X/N/V/C preserved) passed both drivers"
    );
}

/// B0 — the snapshot/restore anchor for the BTST STATIC memory form (the new `AluOp::Btst` + the cmpi-style
/// bitnum-capture interleave). Drives a real vendored `BTST #, (A0)` case (`[EaCalc(capture), Prefetch, Read,
/// Prefetch, Alu]`, 5 micro-ops) through the quiesce driver, snapshotting + restoring the WHOLE `Cpu68000`
/// (incl. the in-flight cursor) at every micro-op boundary — including the mid-bus-access boundary around the
/// operand Read — and proving the resumed run reproduces the run-to-completion final state + transaction stream
/// bit-for-bit. Pins that `AluOp::Btst` + the captured bit number keep `MicroState` fixed-size bincode.
#[test]
fn btst_static_mem_quiescable_and_serializable_at_every_micro_op_boundary() {
    let path = format!("{VENDOR_DIR}/BTST.json");
    if !Path::new(&path).exists() {
        eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
        return;
    }
    let file = std::fs::File::open(&path).unwrap();
    let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
    let case = data
        .iter()
        .find(|t| t["name"].as_str().unwrap() == "0810 [BTST #, (A0)] 105")
        .expect("BTST static (A0) snapshot anchor present");
    let ini = &case["initial"];

    // Run-to-completion reference.
    let mut rref = Cpu68000::new(build_regs(ini));
    let mut bref = build_bus(ini);
    rref.run_instruction(&mut bref);

    let cfg = bincode::config::standard();
    // 5 micro-ops (EaCalc(capture), Prefetch, Read, Prefetch, Alu) → in-flight boundaries after 0..=4 of them.
    for pause_after in 0..=4 {
        let mut cpu = Cpu68000::new(build_regs(ini));
        let mut bus = build_bus(ini);
        cpu.start_instruction();
        for _ in 0..pause_after {
            assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        }
        // Snapshot + restore the whole CPU (incl. the in-flight cursor) mid-instruction.
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        loop {
            if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                break;
            }
        }
        assert_eq!(
            cpu2.regs, rref.regs,
            "resume from boundary {pause_after} diverged"
        );
        assert_eq!(
            bus.log, bref.log,
            "transaction stream from boundary {pause_after} diverged"
        );
    }
    eprintln!(
        "B0 BTST snapshot/restore: BTST static (A0) resumed identically at every micro-op boundary (incl the bitnum-capture interleave)"
    );
}

/// B1 — the named `BCHG` anchors, pinning the bit test-and-TOGGLE (Z = NOT(the PRE-modify bit), then write
/// `operand ^ (1<<pos)`; X/N/V/C + the SR system byte PRESERVED) against the vendored BCHG stream WITHOUT
/// relying on the bulk `covered()` sweep. The load-bearing facts: ONLY Z changes; Z is from the bit BEFORE the
/// toggle (the read value); a `Dn` dest is 32-bit (mod 32, FULL-32 write with one bit flipped), a memory dest
/// is 8-bit (mod 8, byte RMW); the register `+2` is a DECODE-TIME `pos >= 16` decision (the dynamic bit number
/// is a LIVE `Dn` / the static one is the captured `prefetch[1]`); memory has NO `+2`; static = dynamic + 4
/// (the extra bitnum ext word). The four `Dn`-dynamic anchors span the 2×2 of {pos<16 → 6 cyc, pos>=16 → 8 cyc
/// (the decode-time +2)} × {bit=1→Z 1→0, bit=0→Z 0→1}, each showing the full-32 register write with one bit
/// flipped:
/// - `0540 [BCHG D2, D0] 50` — Dn dyn, pos 0 (<16), bit 1 → Z 1→0, len 6; D0 `…25`→`…24` (bit0 toggled 1→0).
/// - `0d42 [BCHG D6, D2] 135` — Dn dyn, pos 4 (<16), bit 0 → Z 0→1, len 6; D2 `…23`→`…33` (bit4 toggled 0→1).
/// - `0344 [BCHG D1, D4] 47` — Dn dyn, pos 25 (>=16), bit 1 → Z 1→0, len 8 (the decode-time +2); bit25 toggled.
/// - `0541 [BCHG D2, D1] 31` — Dn dyn, pos 28 (>=16), bit 0 → Z 0→1, len 8 (the decode-time +2); bit28 toggled.
/// - `0845 [BCHG #, D5] 195` — Dn STATIC, pos 9 (<16), len 10 (dynamic 6 + 4 for the bitnum ext word).
/// - `0843 [BCHG #, D3] 23` — Dn STATIC, pos 22 (>=16), len 12 (static +4 AND the decode-time +2 — both apply).
/// - `0552 [BCHG D2, (A2)] 9` — `(An)` byte RMW: bus `r.b, r.w, w.b`, len 12, Z from the pre-modify byte bit.
/// - `0979 [BCHG D4, (xxx).l] 34` — `abs.l` byte RMW: bus `r, r, r, r, w`, len 20 (no register +2 for memory).
/// - `0557 [BCHG D2, (A7)] 48` — `(A7)` mode-2 indirect (COVERED, NOT deferred): bus `r.b@A7, r.w, w.b`, len 12.
///
/// Each runs both drivers + the per-cycle transaction stream via `run_case`. Every anchor must decode as a BCHG
/// opcode (dynamic `0xF1C0 == 0x0140` or static `0xFF00 == 0x0800` with tt bits 7-6 == 01) — never a CMP-class.
#[test]
fn bchg_anchors_match_singlesteptests() {
    // (file, full-name, length). The full names select the EXACT case the doc-comment describes.
    let anchors: &[(&str, &str, u32)] = &[
        ("BCHG.json", "0540 [BCHG D2, D0] 50", 6), // Dn dyn pos<16 bit1 → Z 1→0, 6 cyc; bit0 toggled
        ("BCHG.json", "0d42 [BCHG D6, D2] 135", 6), // Dn dyn pos<16 bit0 → Z 0→1, 6 cyc; bit4 toggled
        ("BCHG.json", "0344 [BCHG D1, D4] 47", 8), // Dn dyn pos>=16 bit1 → Z 1→0, 8 cyc (decode-time +2)
        ("BCHG.json", "0541 [BCHG D2, D1] 31", 8), // Dn dyn pos>=16 bit0 → Z 0→1, 8 cyc (decode-time +2)
        ("BCHG.json", "0845 [BCHG #, D5] 195", 10), // Dn STATIC pos<16, 10 = dynamic 6 + 4
        ("BCHG.json", "0843 [BCHG #, D3] 23", 12), // Dn STATIC pos>=16, 12 (static +4 AND decode-time +2)
        ("BCHG.json", "0552 [BCHG D2, (A2)] 9", 12), // (An) byte RMW: r.b, r.w, w.b — Z from pre-modify bit
        ("BCHG.json", "0979 [BCHG D4, (xxx).l] 34", 20), // abs.l byte RMW: r, r, r, r, w
        ("BCHG.json", "0557 [BCHG D2, (A7)] 48", 12), // (A7) m2 indirect (covered): r.b@A7, r.w, w.b
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("B1 BCHG anchor {name} (len {length}) not found in {fname}"));
        // Every anchor must be a BCHG opcode (dynamic 0xF1C0 == 0x0140 OR static 0xFF00 == 0x0800, tt == 01),
        // never a CMP-class opcode.
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        let is_dyn = opcode & 0xF1C0 == 0x0140;
        let is_static = opcode & 0xFF00 == 0x0800 && (opcode >> 6) & 3 == 1;
        assert!(
            is_dyn || is_static,
            "anchor {name} must be a BCHG opcode (dynamic 0x0140 / static 0x0840, tt == 01)"
        );
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "BCHG is not a CMP-class opcode"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all B1 BCHG anchors exercised");
    eprintln!(
        "B1 BCHG anchors: {found} cases (Dn dyn 2×2 bit×pos — pos<16 6 cyc / pos>=16 8 cyc the decode-time +2 / Dn static 10 & 12 / (An) / abs.l / (A7) m2 byte RMW; only Z changes from the pre-toggle bit, X/N/V/C preserved) passed both drivers"
    );
}

/// B1 — the snapshot/restore anchor for the BCHG STATIC memory form (the new `AluOp::Bchg` + the cmpi-style
/// bitnum-capture interleave + the byte read→toggle→write RMW). Drives a real vendored `BCHG #, (A0)` case
/// (`[EaCalc(capture), Prefetch, Read, Prefetch, Alu, Write]`, 6 micro-ops) through the quiesce driver,
/// snapshotting + restoring the WHOLE `Cpu68000` (incl. the in-flight cursor) at every micro-op boundary —
/// including the mid-bus-access boundaries around the operand Read and the write-back — and proving the resumed
/// run reproduces the run-to-completion final state + transaction stream bit-for-bit. Pins that `AluOp::Bchg` +
/// the captured bit number keep `MicroState` fixed-size bincode.
#[test]
fn bchg_static_mem_quiescable_and_serializable_at_every_micro_op_boundary() {
    let path = format!("{VENDOR_DIR}/BCHG.json");
    if !Path::new(&path).exists() {
        eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
        return;
    }
    let file = std::fs::File::open(&path).unwrap();
    let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
    let case = data
        .iter()
        .find(|t| t["name"].as_str().unwrap() == "0850 [BCHG #, (A0)] 44")
        .expect("BCHG static (A0) snapshot anchor present");
    let ini = &case["initial"];

    // Run-to-completion reference.
    let mut rref = Cpu68000::new(build_regs(ini));
    let mut bref = build_bus(ini);
    rref.run_instruction(&mut bref);

    let cfg = bincode::config::standard();
    // 6 micro-ops (EaCalc(capture), Prefetch, Read, Prefetch, Alu, Write) → boundaries after 0..=5 of them.
    for pause_after in 0..=5 {
        let mut cpu = Cpu68000::new(build_regs(ini));
        let mut bus = build_bus(ini);
        cpu.start_instruction();
        for _ in 0..pause_after {
            assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        }
        // Snapshot + restore the whole CPU (incl. the in-flight cursor) mid-instruction.
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        loop {
            if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                break;
            }
        }
        assert_eq!(
            cpu2.regs, rref.regs,
            "resume from boundary {pause_after} diverged"
        );
        assert_eq!(
            bus.log, bref.log,
            "transaction stream from boundary {pause_after} diverged"
        );
    }
    eprintln!(
        "B1 BCHG snapshot/restore: BCHG static (A0) resumed identically at every micro-op boundary (incl the bitnum-capture interleave + the byte read→toggle→write RMW)"
    );
}

/// B2 — the named `BCLR` anchors, pinning the bit test-and-CLEAR (Z = NOT(the PRE-clear bit), then write
/// `operand & !(1<<pos)`; X/N/V/C + the SR system byte PRESERVED) against the vendored BCLR stream WITHOUT
/// relying on the bulk `covered()` sweep. The load-bearing facts: ONLY Z changes; Z is from the bit BEFORE the
/// clear (the read value); a `Dn` dest is 32-bit (mod 32, FULL-32 write with one bit cleared), a memory dest is
/// 8-bit (mod 8, byte RMW); the register `+2` is a DECODE-TIME `pos >= 16` decision (the dynamic bit number is
/// a LIVE `Dn` / the static one is the captured `prefetch[1]`); memory has NO `+2` (IDENTICAL to BCHG, fixed
/// byte); static = dynamic + 4 (the extra bitnum ext word). BCLR's REGISTER base idle is `n4` (NOT `n2`) — its
/// `Dn` forms are 2 cycles SLOWER than BCHG/BSET: dynamic 8 (pos<16) / 10 (pos>=16), static 12 / 14. The four
/// `Dn`-dynamic anchors span the 2×2 of {pos<16 → 8 cyc, pos>=16 → 10 cyc (the decode-time +2)} × {bit=1→Z 1→0,
/// bit=0→Z 0→1}, each showing the full-32 register write with one bit cleared:
/// - `0587 [BCLR D2, D7] 51` — Dn dyn, pos 7 (<16), bit 1 → Z 1→0, len 8; D7 `…85`→`…05` (bit7 cleared 1→0).
/// - `0186 [BCLR D0, D6] 27` — Dn dyn, pos 15 (<16), bit 0 → Z 0→1, len 8; D6 unchanged (bit15 already 0).
/// - `0d83 [BCLR D6, D3] 15` — Dn dyn, pos 23 (>=16), bit 1 → Z 1→0, len 10 (the decode-time +2); bit23 cleared.
/// - `0383 [BCLR D1, D3] 69` — Dn dyn, pos 31 (>=16), bit 0 → Z 0→1, len 10 (the decode-time +2); unchanged.
/// - `0882 [BCLR #, D2] 25` — Dn STATIC, pos 11 (<16), len 12 (dynamic 8 + 4 for the bitnum ext word).
/// - `0882 [BCLR #, D2] 307` — Dn STATIC, pos 16 (>=16), len 14 (static +4 AND the decode-time +2 — both apply).
/// - `0d92 [BCLR D6, (A2)] 5` — `(An)` byte RMW: bus `r.b, r.w, w.b`, len 12, Z from the PRE-clear byte bit (Z
///   stays 0 though the bit ends cleared — proving Z reflects the bit BEFORE the clear, not after).
/// - `05b9 [BCLR D2, (xxx).l] 47` — `abs.l` byte RMW: bus `r, r, r, r, w`, len 20 (no register +2 for memory).
/// - `0597 [BCLR D2, (A7)] 58` — `(A7)` mode-2 indirect (COVERED, NOT deferred): bus `r.b@A7, r.w, w.b`, len 12.
///
/// Each runs both drivers + the per-cycle transaction stream via `run_case`. Every anchor must decode as a BCLR
/// opcode (dynamic `0xF1C0 == 0x0180` or static `0xFF00 == 0x0800` with tt bits 7-6 == 10) — never a CMP-class.
#[test]
fn bclr_anchors_match_singlesteptests() {
    // (file, full-name, length). The full names select the EXACT case the doc-comment describes.
    let anchors: &[(&str, &str, u32)] = &[
        ("BCLR.json", "0587 [BCLR D2, D7] 51", 8), // Dn dyn pos<16 bit1 → Z 1→0, 8 cyc; bit7 cleared
        ("BCLR.json", "0186 [BCLR D0, D6] 27", 8), // Dn dyn pos<16 bit0 → Z 0→1, 8 cyc; unchanged
        ("BCLR.json", "0d83 [BCLR D6, D3] 15", 10), // Dn dyn pos>=16 bit1 → Z 1→0, 10 cyc (decode-time +2)
        ("BCLR.json", "0383 [BCLR D1, D3] 69", 10), // Dn dyn pos>=16 bit0 → Z 0→1, 10 cyc (decode-time +2)
        ("BCLR.json", "0882 [BCLR #, D2] 25", 12),  // Dn STATIC pos<16, 12 = dynamic 8 + 4
        ("BCLR.json", "0882 [BCLR #, D2] 307", 14), // Dn STATIC pos>=16, 14 (static +4 AND decode-time +2)
        ("BCLR.json", "0d92 [BCLR D6, (A2)] 5", 12), // (An) byte RMW: r.b, r.w, w.b — Z from PRE-clear bit
        ("BCLR.json", "05b9 [BCLR D2, (xxx).l] 47", 20), // abs.l byte RMW: r, r, r, r, w
        ("BCLR.json", "0597 [BCLR D2, (A7)] 58", 12), // (A7) m2 indirect (covered): r.b@A7, r.w, w.b
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("B2 BCLR anchor {name} (len {length}) not found in {fname}"));
        // Every anchor must be a BCLR opcode (dynamic 0xF1C0 == 0x0180 OR static 0xFF00 == 0x0800, tt == 10),
        // never a CMP-class opcode.
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        let is_dyn = opcode & 0xF1C0 == 0x0180;
        let is_static = opcode & 0xFF00 == 0x0800 && (opcode >> 6) & 3 == 2;
        assert!(
            is_dyn || is_static,
            "anchor {name} must be a BCLR opcode (dynamic 0x0180 / static 0x0880, tt == 10)"
        );
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "BCLR is not a CMP-class opcode"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all B2 BCLR anchors exercised");
    eprintln!(
        "B2 BCLR anchors: {found} cases (Dn dyn 2×2 bit×pos — pos<16 8 cyc / pos>=16 10 cyc the decode-time +2 / Dn static 12 & 14 / (An) / abs.l / (A7) m2 byte RMW; only Z changes from the pre-clear bit, X/N/V/C preserved; register base n4 — 2 slower than BCHG) passed both drivers"
    );
}

/// B2 — the snapshot/restore anchor for the BCLR STATIC memory form (the new `AluOp::Bclr` + the cmpi-style
/// bitnum-capture interleave + the byte read→clear→write RMW). Drives a real vendored `BCLR #, (A0)` case
/// (`[EaCalc(capture), Prefetch, Read, Prefetch, Alu, Write]`, 6 micro-ops) through the quiesce driver,
/// snapshotting + restoring the WHOLE `Cpu68000` (incl. the in-flight cursor) at every micro-op boundary —
/// including the mid-bus-access boundaries around the operand Read and the write-back — and proving the resumed
/// run reproduces the run-to-completion final state + transaction stream bit-for-bit. Pins that `AluOp::Bclr` +
/// the captured bit number keep `MicroState` fixed-size bincode.
#[test]
fn bclr_static_mem_quiescable_and_serializable_at_every_micro_op_boundary() {
    let path = format!("{VENDOR_DIR}/BCLR.json");
    if !Path::new(&path).exists() {
        eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
        return;
    }
    let file = std::fs::File::open(&path).unwrap();
    let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
    let case = data
        .iter()
        .find(|t| t["name"].as_str().unwrap() == "0890 [BCLR #, (A0)] 872")
        .expect("BCLR static (A0) snapshot anchor present");
    let ini = &case["initial"];

    // Run-to-completion reference.
    let mut rref = Cpu68000::new(build_regs(ini));
    let mut bref = build_bus(ini);
    rref.run_instruction(&mut bref);

    let cfg = bincode::config::standard();
    // 6 micro-ops (EaCalc(capture), Prefetch, Read, Prefetch, Alu, Write) → boundaries after 0..=5 of them.
    for pause_after in 0..=5 {
        let mut cpu = Cpu68000::new(build_regs(ini));
        let mut bus = build_bus(ini);
        cpu.start_instruction();
        for _ in 0..pause_after {
            assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        }
        // Snapshot + restore the whole CPU (incl. the in-flight cursor) mid-instruction.
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        loop {
            if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                break;
            }
        }
        assert_eq!(
            cpu2.regs, rref.regs,
            "resume from boundary {pause_after} diverged"
        );
        assert_eq!(
            bus.log, bref.log,
            "transaction stream from boundary {pause_after} diverged"
        );
    }
    eprintln!(
        "B2 BCLR snapshot/restore: BCLR static (A0) resumed identically at every micro-op boundary (incl the bitnum-capture interleave + the byte read→clear→write RMW)"
    );
}

/// B3 — the named `BSET` anchors, pinning the bit test-and-SET (Z = NOT(the PRE-set bit), then write
/// `operand | (1<<pos)`; X/N/V/C + the SR system byte PRESERVED) against the vendored BSET stream WITHOUT
/// relying on the bulk `covered()` sweep. The load-bearing facts: ONLY Z changes; Z is from the bit BEFORE the
/// set (the read value); a `Dn` dest is 32-bit (mod 32, FULL-32 write with one bit set), a memory dest is
/// 8-bit (mod 8, byte RMW); the register `+2` is a DECODE-TIME `pos >= 16` decision (the dynamic bit number is
/// a LIVE `Dn` / the static one is the captured `prefetch[1]`); memory has NO `+2` (IDENTICAL to BCHG, fixed
/// byte); static = dynamic + 4 (the extra bitnum ext word). BSET's REGISTER base idle is `n2` (SAME as BCHG,
/// NOT BCLR's `n4`): dynamic 6 (pos<16) / 8 (pos>=16), static 10 / 12. The four `Dn`-dynamic anchors span the
/// 2×2 of {pos<16 → 6 cyc, pos>=16 → 8 cyc (the decode-time +2)} × {bit=1→Z 1→0, bit=0→Z 0→1}, each showing the
/// full-32 register write with one bit set:
/// - `0dc4 [BSET D6, D4] 37` — Dn dyn, pos 11 (<16), bit 1 → Z 1→0, len 6; D4 bit11 already set (unchanged).
/// - `09c2 [BSET D4, D2] 3` — Dn dyn, pos 9 (<16), bit 0 → Z 0→1, len 6; D2 bit9 set 0→1.
/// - `0bc2 [BSET D5, D2] 15` — Dn dyn, pos 19 (>=16), bit 1 → Z 1→0, len 8 (the decode-time +2); bit19 set.
/// - `07c2 [BSET D3, D2] 50` — Dn dyn, pos 16 (>=16), bit 0 → Z 0→1, len 8 (the decode-time +2); bit16 set 0→1.
/// - `08c5 [BSET #, D5] 174` — Dn STATIC, pos 14 (<16), len 10 (dynamic 6 + 4 for the bitnum ext word).
/// - `08c7 [BSET #, D7] 134` — Dn STATIC, pos 24 (>=16), len 12 (static +4 AND the decode-time +2 — both apply).
/// - `0dd3 [BSET D6, (A3)] 8` — `(An)` byte RMW: bus `r.b, r.w, w.b`, len 12, Z from the PRE-set byte bit (bit
///   was 0 → Z 0→1 yet the byte ends with the bit SET — proving Z reflects the bit BEFORE the set, not after).
/// - `0df8 [BSET D6, (xxx).w] 49` — `abs.w` byte RMW: bus `r.w, r.b, r.w, w.b`, len 16 (no register +2 for memory).
/// - `05d7 [BSET D2, (A7)] 43` — `(A7)` mode-2 indirect (COVERED, NOT deferred): bus `r.b@A7, r.w, w.b`, len 12.
///
/// Each runs both drivers + the per-cycle transaction stream via `run_case`. Every anchor must decode as a BSET
/// opcode (dynamic `0xF1C0 == 0x01C0` or static `0xFF00 == 0x0800` with tt bits 7-6 == 11) — never a CMP-class.
#[test]
fn bset_anchors_match_singlesteptests() {
    // (file, full-name, length). The full names select the EXACT case the doc-comment describes.
    let anchors: &[(&str, &str, u32)] = &[
        ("BSET.json", "0dc4 [BSET D6, D4] 37", 6), // Dn dyn pos<16 bit1 → Z 1→0, 6 cyc; bit11 set
        ("BSET.json", "09c2 [BSET D4, D2] 3", 6), // Dn dyn pos<16 bit0 → Z 0→1, 6 cyc; bit9 set 0→1
        ("BSET.json", "0bc2 [BSET D5, D2] 15", 8), // Dn dyn pos>=16 bit1 → Z 1→0, 8 cyc (decode-time +2)
        ("BSET.json", "07c2 [BSET D3, D2] 50", 8), // Dn dyn pos>=16 bit0 → Z 0→1, 8 cyc (decode-time +2)
        ("BSET.json", "08c5 [BSET #, D5] 174", 10), // Dn STATIC pos<16, 10 = dynamic 6 + 4
        ("BSET.json", "08c7 [BSET #, D7] 134", 12), // Dn STATIC pos>=16, 12 (static +4 AND decode-time +2)
        ("BSET.json", "0dd3 [BSET D6, (A3)] 8", 12), // (An) byte RMW: r.b, r.w, w.b — Z from PRE-set bit
        ("BSET.json", "0df8 [BSET D6, (xxx).w] 49", 16), // abs.w byte RMW: r.w, r.b, r.w, w.b
        ("BSET.json", "05d7 [BSET D2, (A7)] 43", 12), // (A7) m2 indirect (covered): r.b@A7, r.w, w.b
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("B3 BSET anchor {name} (len {length}) not found in {fname}"));
        // Every anchor must be a BSET opcode (dynamic 0xF1C0 == 0x01C0 OR static 0xFF00 == 0x0800, tt == 11),
        // never a CMP-class opcode.
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        let is_dyn = opcode & 0xF1C0 == 0x01C0;
        let is_static = opcode & 0xFF00 == 0x0800 && (opcode >> 6) & 3 == 3;
        assert!(
            is_dyn || is_static,
            "anchor {name} must be a BSET opcode (dynamic 0x01C0 / static 0x08C0, tt == 11)"
        );
        assert_eq!(
            cmp_class(opcode),
            CmpClass::None,
            "BSET is not a CMP-class opcode"
        );
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all B3 BSET anchors exercised");
    eprintln!(
        "B3 BSET anchors: {found} cases (Dn dyn 2×2 bit×pos — pos<16 6 cyc / pos>=16 8 cyc the decode-time +2 / Dn static 10 & 12 / (An) / abs.w / (A7) m2 byte RMW; only Z changes from the pre-set bit, X/N/V/C preserved; register base n2 — same as BCHG) passed both drivers"
    );
}

/// B3 — the snapshot/restore anchor for the BSET STATIC memory form (the new `AluOp::Bset` + the cmpi-style
/// bitnum-capture interleave + the byte read→set→write RMW). Drives a real vendored `BSET #, (A0)` case
/// (`[EaCalc(capture), Prefetch, Read, Prefetch, Alu, Write]`, 6 micro-ops) through the quiesce driver,
/// snapshotting + restoring the WHOLE `Cpu68000` (incl. the in-flight cursor) at every micro-op boundary —
/// including the mid-bus-access boundaries around the operand Read and the write-back — and proving the resumed
/// run reproduces the run-to-completion final state + transaction stream bit-for-bit. Pins that `AluOp::Bset` +
/// the captured bit number keep `MicroState` fixed-size bincode.
#[test]
fn bset_static_mem_quiescable_and_serializable_at_every_micro_op_boundary() {
    let path = format!("{VENDOR_DIR}/BSET.json");
    if !Path::new(&path).exists() {
        eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
        return;
    }
    let file = std::fs::File::open(&path).unwrap();
    let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
    let case = data
        .iter()
        .find(|t| t["name"].as_str().unwrap() == "08d0 [BSET #, (A0)] 678")
        .expect("BSET static (A0) snapshot anchor present");
    let ini = &case["initial"];

    // Run-to-completion reference.
    let mut rref = Cpu68000::new(build_regs(ini));
    let mut bref = build_bus(ini);
    rref.run_instruction(&mut bref);

    let cfg = bincode::config::standard();
    // 6 micro-ops (EaCalc(capture), Prefetch, Read, Prefetch, Alu, Write) → boundaries after 0..=5 of them.
    for pause_after in 0..=5 {
        let mut cpu = Cpu68000::new(build_regs(ini));
        let mut bus = build_bus(ini);
        cpu.start_instruction();
        for _ in 0..pause_after {
            assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        }
        // Snapshot + restore the whole CPU (incl. the in-flight cursor) mid-instruction.
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        loop {
            if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                break;
            }
        }
        assert_eq!(
            cpu2.regs, rref.regs,
            "resume from boundary {pause_after} diverged"
        );
        assert_eq!(
            bus.log, bref.log,
            "transaction stream from boundary {pause_after} diverged"
        );
    }
    eprintln!(
        "B3 BSET snapshot/restore: BSET static (A0) resumed identically at every micro-op boundary (incl the bitnum-capture interleave + the byte read→set→write RMW)"
    );
}

/// S0 — the named ASL anchors, pinning the foundational shift/rotate op against the vendored ASL.b/.w/.l
/// stream WITHOUT relying on the bulk `covered()` sweep. ASL is arithmetic shift LEFT, the ONE shift that
/// owns the **V** flag (the sign bit changed at ANY point during the shift). Each anchor is a real vendored
/// case run through both drivers + the per-cycle transaction stream via `run_case`; the load-bearing pins:
///
/// - `e302 [ASL.b Q, D2] 1` (len 8) — REGISTER **immediate** `.b`, cnt 1 → timing `6 + 2*1`.
/// - `e103 [ASL.b Q, D3] 16` (len 22) — REGISTER immediate `.b`, `ccc == 0` → cnt **8** → `6 + 2*8`.
/// - `e741 [ASL.w Q, D1] 59` (len 12) — REGISTER immediate `.w`, cnt 3 → `6 + 2*3`.
/// - `e386 [ASL.l Q, D6] 3` (len 10) — REGISTER immediate `.l`, cnt 1 → the `.l` base `8 + 2*1`.
/// - `ed66 [ASL.w D6, D6] 603` (len 6) — REGISTER **`Dn`-count `cnt == 0`** (the ZERO-COUNT special case),
///   AND `ccc == rrr` (count reg == operand reg, legal): value UNCHANGED, **X PRESERVED** (X1 in → X1 out),
///   V = 0, C = 0, timing `6` (`6 + 2*0`).
/// - `e9a1 [ASL.l D4, D1] 173` (len 8) — `Dn`-count `cnt == 0` `.l`, the `8`-cyc zero-count register form.
/// - `e564 [ASL.w D2, D4] 347` (len 86) — **large `Dn`-count** cnt 40 → `6 + 2*40` (the DECODE-TIME live
///   `Dn & 63` driving the idle).
/// - `efa3 [ASL.l D7, D3] 252` (len 88) — large `Dn`-count cnt 40 `.l` → `8 + 2*40`.
/// - `eb46 [ASL.w Q, D6] 3` (len 16) — an ASL.w that **SETS V** (the sign bit changed during the shift).
/// - `e542 [ASL.w Q, D2] 4` (len 10) — an ASL.w that **CLEARS V** (the sign did not change).
/// - `e1d5 [ASL.w (A5)] 103` (len 12) — `.w` **memory** shift-by-1 `(An)`: the word `ea_dst` RMW.
/// - `e1f9 [ASL.w (xxx).l] 199` (len 20) — `.w` memory shift-by-1 **abs.l** (the heaviest RMW).
/// - `e1d7 [ASL.w (A7)] 464` (len 12) — the plain `(A7)` mode-2 indirect (`mode == 2 && reg == 7`), COVERED
///   (NOT deferred), a clean word RMW at the active A7.
/// - `e1d6 [ASL.w (A6)] 41` (len 50) — an **odd-EA** `.w` memory address error (the E3/E4 abort installs the
///   group-0 14-byte vector-3 frame), which must PASS unchanged.
///
/// Every anchor must decode as an ASL opcode (`0xExxx`, type AS / direction LEFT) — never any other family.
#[test]
fn asl_anchors_match_singlesteptests() {
    let anchors: &[(&str, &str, u32)] = &[
        ("ASL.b.json", "e302 [ASL.b Q, D2] 1", 8), // imm .b cnt1 → 6+2
        ("ASL.b.json", "e103 [ASL.b Q, D3] 16", 22), // imm .b ccc=0 → cnt8 → 6+16
        ("ASL.w.json", "e741 [ASL.w Q, D1] 59", 12), // imm .w cnt3 → 6+6
        ("ASL.l.json", "e386 [ASL.l Q, D6] 3", 10), // imm .l cnt1 → 8+2
        ("ASL.w.json", "ed66 [ASL.w D6, D6] 603", 6), // Dn cnt0 (zero-count) + ccc==rrr, X kept
        ("ASL.l.json", "e9a1 [ASL.l D4, D1] 173", 8), // Dn cnt0 .l → 8
        ("ASL.w.json", "e564 [ASL.w D2, D4] 347", 86), // Dn cnt40 → 6+80
        ("ASL.l.json", "efa3 [ASL.l D7, D3] 252", 88), // Dn cnt40 .l → 8+80
        ("ASL.w.json", "eb46 [ASL.w Q, D6] 3", 16), // V SET (sign changed)
        ("ASL.w.json", "e542 [ASL.w Q, D2] 4", 10), // V CLEAR
        ("ASL.w.json", "e1d5 [ASL.w (A5)] 103", 12), // memory (An) shift-by-1
        ("ASL.w.json", "e1f9 [ASL.w (xxx).l] 199", 20), // memory abs.l shift-by-1
        ("ASL.w.json", "e1d7 [ASL.w (A7)] 464", 12), // (A7) mode-2 indirect — COVERED
        ("ASL.w.json", "e1d6 [ASL.w (A6)] 41", 50), // odd-EA memory address error
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("S0 ASL anchor {name} (len {length}) not found in {fname}"));
        // Every anchor must be an ASL opcode: 0xExxx, type AS (register bits 4-3 == 0 / memory bits 10-9 ==
        // 0), direction LEFT (bit 8 == 1).
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode >> 12,
            0xE,
            "anchor {name} must be a 0xExxx shift opcode"
        );
        let is_asl = if (opcode >> 6) & 3 == 3 {
            (opcode >> 8) & 1 == 1 && (opcode >> 9) & 3 == 0
        } else {
            (opcode >> 8) & 1 == 1 && (opcode >> 3) & 3 == 0
        };
        assert!(
            is_asl,
            "anchor {name} must be ASL (type AS, direction LEFT)"
        );
        // Load-bearing flag/scope pins (run_case verifies the FULL final state against the data either way).
        let ini_sr = case["initial"]["sr"].as_u64().unwrap() as u16;
        let fin_sr = case["final"]["sr"].as_u64().unwrap() as u16;
        match *name {
            "eb46 [ASL.w Q, D6] 3" => {
                assert_ne!(
                    fin_sr & 0x02,
                    0,
                    "ASL V-set anchor must set V (sign changed)"
                )
            }
            "e542 [ASL.w Q, D2] 4" => {
                assert_eq!(fin_sr & 0x02, 0, "ASL V-clear anchor must clear V")
            }
            "ed66 [ASL.w D6, D6] 603" => {
                // Zero-count: X PRESERVED (not set to C), V and C cleared, value unchanged.
                assert_eq!(ini_sr & 0x10, fin_sr & 0x10, "zero-count must PRESERVE X");
                assert_ne!(
                    ini_sr & 0x10,
                    0,
                    "zero-count anchor must enter with X=1 (pins preservation)"
                );
                assert_eq!(fin_sr & 0x02, 0, "zero-count must clear V");
                assert_eq!(fin_sr & 0x01, 0, "zero-count must clear C");
                assert_eq!(
                    case["initial"]["d6"], case["final"]["d6"],
                    "zero-count must leave the operand unchanged"
                );
            }
            "e1d7 [ASL.w (A7)] 464" => {
                assert_eq!((opcode >> 3) & 7, 2, "(A7) anchor must be mode 2");
                assert_eq!(opcode & 7, 7, "(A7) anchor must be reg 7 (the A7 indirect)");
            }
            "e1d6 [ASL.w (A6)] 41" => {
                // Odd-EA address error: the group-0 frame pushes the SSP down (the standard 14-byte frame).
                assert!(
                    case["final"]["ssp"].as_u64().unwrap()
                        < case["initial"]["ssp"].as_u64().unwrap(),
                    "odd-EA anchor must install the address-error frame (SSP pushed down)"
                );
            }
            _ => {}
        }
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all S0 ASL anchors exercised");
    eprintln!(
        "S0 ASL anchors: {found} cases (imm .b/.w/.l 6+2cnt / 8+2cnt, Dn cnt0 zero-count X-preserved + ccc==rrr, large Dn cnt40, V-set & V-clear, (An)/abs.l/(A7) memory shift-by-1, odd-EA address-error) passed both drivers"
    );
}

/// S0 — the snapshot/restore anchor for the ASL.w memory shift-by-1 (the new `shift_recipe` word `ea_dst`
/// RMW: `[Read, Prefetch, Alu, Write]`). Drives a real vendored `ASL.w (A5)` case through the quiesce driver,
/// snapshotting + restoring the WHOLE `Cpu68000` (incl. the in-flight cursor) at every micro-op boundary —
/// including the mid-bus-access boundary between the operand Read and the result Write — and proves the
/// resumed run reproduces the run-to-completion final state + transaction stream bit-for-bit. This pins that
/// `Operand::ShiftCount` + `AluOp::Asl` keep `MicroState` fixed-size bincode (it stays `Copy`).
#[test]
fn asl_w_mem_quiescable_and_serializable_at_every_micro_op_boundary() {
    let path = format!("{VENDOR_DIR}/ASL.w.json");
    if !Path::new(&path).exists() {
        eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
        return;
    }
    let file = std::fs::File::open(&path).unwrap();
    let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
    let case = data
        .iter()
        .find(|t| t["name"].as_str().unwrap() == "e1d5 [ASL.w (A5)] 103")
        .expect("ASL.w (A5) snapshot anchor present");
    let ini = &case["initial"];

    // Run-to-completion reference.
    let mut rref = Cpu68000::new(build_regs(ini));
    let mut bref = build_bus(ini);
    rref.run_instruction(&mut bref);

    let cfg = bincode::config::standard();
    // 4 micro-ops (Read, Prefetch, Alu, Write) → in-flight boundaries after 0..=3 of them.
    for pause_after in 0..=3 {
        let mut cpu = Cpu68000::new(build_regs(ini));
        let mut bus = build_bus(ini);
        cpu.start_instruction();
        for _ in 0..pause_after {
            assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        }
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        loop {
            if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                break;
            }
        }
        assert_eq!(
            cpu2.regs, rref.regs,
            "resume from boundary {pause_after} diverged"
        );
        assert_eq!(
            bus.log, bref.log,
            "transaction stream from boundary {pause_after} diverged"
        );
    }
    eprintln!(
        "S0 ASL snapshot/restore: ASL.w (A5) word shift-by-1 RMW resumed identically at every micro-op boundary"
    );
}

/// S1 — the named ASR anchors, pinning arithmetic shift RIGHT against the vendored ASR.b/.w/.l stream WITHOUT
/// relying on the bulk `covered()` sweep. ASR is the sign-EXTENDING right shift; it reuses S0's `shift_recipe`/
/// `Operand::ShiftCount`/`dn_*` VERBATIM (only the AluOp + the AS/right decode arm differ). Each anchor is a
/// real vendored case run through both drivers + the per-cycle transaction stream via `run_case`; the
/// load-bearing pins:
///
/// - `e202 [ASR.b Q, D2] 9` (len 8) — REGISTER **immediate** `.b`, cnt 1 → timing `6 + 2*1`.
/// - `e005 [ASR.b Q, D5] 20` (len 22) — REGISTER immediate `.b`, `ccc == 0` → cnt **8** → `6 + 2*8`.
/// - `e605 [ASR.b Q, D5] 4` (len 12) — REGISTER immediate `.b`, cnt 3, **NEGATIVE operand `cnt <= n`**: the
///   value sign-extends (N set) AND **C = bit(cnt-1) = 1** (the in-range carry; `6 + 2*3`).
/// - `e645 [ASR.w Q, D5] 1` (len 12) — REGISTER immediate `.w`, cnt 3 → `6 + 2*3`.
/// - `e282 [ASR.l Q, D2] 1` (len 10) — REGISTER immediate `.l`, cnt 1 → the `.l` base `8 + 2*1`.
/// - `ea23 [ASR.b D5, D3] 8` (len 30) — **THE ASR CARRY QUIRK**: a `Dn`-count `cnt = 12 > n = 8` over-shift of
///   a NEGATIVE operand — the value sign-extends to all-ones (N set) but **C = 0** (NOT the sign bit; the naive
///   "last bit out = sign for over-shift" rule would wrongly set C = 1). V = 0. Timing `6 + 2*12` (the live
///   `Dn & 63` driving the idle).
/// - `e867 [ASR.w D4, D7] 17` (len 6) — REGISTER **`Dn`-count `cnt == 0`** (the ZERO-COUNT special case): value
///   UNCHANGED, **X PRESERVED** (X1 in → X1 out), V = 0, C = 0, timing `6` (`6 + 2*0`).
/// - `e6a1 [ASR.l D3, D1] 89` (len 8) — `Dn`-count `cnt == 0` `.l`, the `8`-cyc zero-count register form.
/// - `e0d5 [ASR.w (A5)] 26` (len 12) — `.w` **memory** shift-by-1 `(An)`: the word `ea_dst` RMW.
/// - `e0e3 [ASR.w -(A3)] 22` (len 14) — `.w` memory shift-by-1 `-(An)` (the `14`-cyc predecrement RMW).
/// - `e0d7 [ASR.w (A7)] 401` (len 12) — the plain `(A7)` mode-2 indirect (`mode == 2 && reg == 7`), COVERED
///   (NOT deferred), a clean word RMW at the active A7.
/// - `e0d6 [ASR.w (A6)] 76` (len 50) — an **odd-EA** `.w` memory address error (the E3/E4 abort installs the
///   group-0 14-byte vector-3 frame), which must PASS unchanged.
///
/// Every anchor must decode as an ASR opcode (`0xExxx`, type AS / direction RIGHT) — never any other family.
#[test]
fn asr_anchors_match_singlesteptests() {
    let anchors: &[(&str, &str, u32)] = &[
        ("ASR.b.json", "e202 [ASR.b Q, D2] 9", 8), // imm .b cnt1 → 6+2
        ("ASR.b.json", "e005 [ASR.b Q, D5] 20", 22), // imm .b ccc=0 → cnt8 → 6+16
        ("ASR.b.json", "e605 [ASR.b Q, D5] 4", 12), // imm .b cnt3 NEGATIVE, cnt<=n, C=bit(cnt-1)=1
        ("ASR.w.json", "e645 [ASR.w Q, D5] 1", 12), // imm .w cnt3 → 6+6
        ("ASR.l.json", "e282 [ASR.l Q, D2] 1", 10), // imm .l cnt1 → 8+2
        ("ASR.b.json", "ea23 [ASR.b D5, D3] 8", 30), // QUIRK: Dn cnt12 > n=8 negative → C=0 (not sign), 6+24
        ("ASR.w.json", "e867 [ASR.w D4, D7] 17", 6), // Dn cnt0 (zero-count), X kept
        ("ASR.l.json", "e6a1 [ASR.l D3, D1] 89", 8), // Dn cnt0 .l → 8
        ("ASR.w.json", "e0d5 [ASR.w (A5)] 26", 12),  // memory (An) shift-by-1
        ("ASR.w.json", "e0e3 [ASR.w -(A3)] 22", 14), // memory -(An) shift-by-1
        ("ASR.w.json", "e0d7 [ASR.w (A7)] 401", 12), // (A7) mode-2 indirect — COVERED
        ("ASR.w.json", "e0d6 [ASR.w (A6)] 76", 50),  // odd-EA memory address error
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("S1 ASR anchor {name} (len {length}) not found in {fname}"));
        // Every anchor must be an ASR opcode: 0xExxx, type AS (register bits 4-3 == 0 / memory bits 10-9 ==
        // 0), direction RIGHT (bit 8 == 0).
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode >> 12,
            0xE,
            "anchor {name} must be a 0xExxx shift opcode"
        );
        let is_asr = if (opcode >> 6) & 3 == 3 {
            (opcode >> 8) & 1 == 0 && (opcode >> 9) & 3 == 0
        } else {
            (opcode >> 8) & 1 == 0 && (opcode >> 3) & 3 == 0
        };
        assert!(
            is_asr,
            "anchor {name} must be ASR (type AS, direction RIGHT)"
        );
        // Load-bearing flag/scope pins (run_case verifies the FULL final state against the data either way).
        let ini_sr = case["initial"]["sr"].as_u64().unwrap() as u16;
        let fin_sr = case["final"]["sr"].as_u64().unwrap() as u16;
        match *name {
            "e605 [ASR.b Q, D5] 4" => {
                // cnt <= n, NEGATIVE operand: the value sign-extends (N set) AND C = bit(cnt-1) = 1 (the
                // in-range carry — distinct from the cnt>n quirk), V cleared.
                assert_ne!(
                    fin_sr & 0x08,
                    0,
                    "cnt<=n negative ASR must set N (sign-extended)"
                );
                assert_ne!(fin_sr & 0x01, 0, "cnt<=n ASR must set C = bit(cnt-1) = 1");
                assert_eq!(fin_sr & 0x02, 0, "ASR must always clear V");
            }
            "ea23 [ASR.b D5, D3] 8" => {
                // THE QUIRK: cnt=12 > n=8 over-shift of a negative operand. The value is all-sign-bits
                // (N set) yet C MUST be 0 (NOT the sign — the naive over-shift rule would set C=1). V = 0.
                assert_ne!(
                    fin_sr & 0x08,
                    0,
                    "over-shift ASR of a negative operand must set N (all-sign-bits)"
                );
                assert_eq!(
                    fin_sr & 0x01,
                    0,
                    "ASR carry quirk: cnt>n must clear C (NOT the sign bit)"
                );
                assert_eq!(fin_sr & 0x02, 0, "ASR must always clear V");
            }
            "e867 [ASR.w D4, D7] 17" => {
                // Zero-count: X PRESERVED (not set to C), V and C cleared, value unchanged.
                assert_eq!(ini_sr & 0x10, fin_sr & 0x10, "zero-count must PRESERVE X");
                assert_ne!(
                    ini_sr & 0x10,
                    0,
                    "zero-count anchor must enter with X=1 (pins preservation)"
                );
                assert_eq!(fin_sr & 0x02, 0, "zero-count must clear V");
                assert_eq!(fin_sr & 0x01, 0, "zero-count must clear C");
                assert_eq!(
                    case["initial"]["d7"], case["final"]["d7"],
                    "zero-count must leave the operand unchanged"
                );
            }
            "e0d7 [ASR.w (A7)] 401" => {
                assert_eq!((opcode >> 3) & 7, 2, "(A7) anchor must be mode 2");
                assert_eq!(opcode & 7, 7, "(A7) anchor must be reg 7 (the A7 indirect)");
            }
            "e0d6 [ASR.w (A6)] 76" => {
                // Odd-EA address error: the group-0 frame pushes the SSP down (the standard 14-byte frame).
                assert!(
                    case["final"]["ssp"].as_u64().unwrap()
                        < case["initial"]["ssp"].as_u64().unwrap(),
                    "odd-EA anchor must install the address-error frame (SSP pushed down)"
                );
            }
            _ => {}
        }
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all S1 ASR anchors exercised");
    eprintln!(
        "S1 ASR anchors: {found} cases (imm .b/.w/.l 6+2cnt / 8+2cnt, cnt<=n negative C=bit(cnt-1), the cnt>n CARRY QUIRK C=0 on a negative operand, Dn cnt0 zero-count X-preserved, (An)/-(An)/(A7) memory shift-by-1, odd-EA address-error) passed both drivers"
    );
}

/// S1 — the snapshot/restore anchor for the ASR.w memory shift-by-1 (the shared `shift_recipe` word `ea_dst`
/// RMW: `[Read, Prefetch, Alu, Write]`). Drives a real vendored `ASR.w (A5)` case through the quiesce driver,
/// snapshotting + restoring the WHOLE `Cpu68000` (incl. the in-flight cursor) at every micro-op boundary —
/// including the mid-bus-access boundary between the operand Read and the result Write — and proves the
/// resumed run reproduces the run-to-completion final state + transaction stream bit-for-bit. This pins that
/// `AluOp::Asr` keeps `MicroState` fixed-size bincode (it stays `Copy`).
#[test]
fn asr_w_mem_quiescable_and_serializable_at_every_micro_op_boundary() {
    let path = format!("{VENDOR_DIR}/ASR.w.json");
    if !Path::new(&path).exists() {
        eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
        return;
    }
    let file = std::fs::File::open(&path).unwrap();
    let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
    let case = data
        .iter()
        .find(|t| t["name"].as_str().unwrap() == "e0d5 [ASR.w (A5)] 26")
        .expect("ASR.w (A5) snapshot anchor present");
    let ini = &case["initial"];

    // Run-to-completion reference.
    let mut rref = Cpu68000::new(build_regs(ini));
    let mut bref = build_bus(ini);
    rref.run_instruction(&mut bref);

    let cfg = bincode::config::standard();
    // 4 micro-ops (Read, Prefetch, Alu, Write) → in-flight boundaries after 0..=3 of them.
    for pause_after in 0..=3 {
        let mut cpu = Cpu68000::new(build_regs(ini));
        let mut bus = build_bus(ini);
        cpu.start_instruction();
        for _ in 0..pause_after {
            assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        }
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        loop {
            if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                break;
            }
        }
        assert_eq!(
            cpu2.regs, rref.regs,
            "resume from boundary {pause_after} diverged"
        );
        assert_eq!(
            bus.log, bref.log,
            "transaction stream from boundary {pause_after} diverged"
        );
    }
    eprintln!(
        "S1 ASR snapshot/restore: ASR.w (A5) word shift-by-1 RMW resumed identically at every micro-op boundary"
    );
}

/// S2 — the named LSL anchors, pinning logical shift LEFT against the vendored LSL.b/.w/.l stream WITHOUT
/// relying on the bulk `covered()` sweep. LSL is IDENTICAL to ASL's value and carry, with V FORCED to 0 (a
/// logical shift never tracks the sign change — only ASL owns V); it reuses S0's `shift_recipe`/
/// `Operand::ShiftCount`/`dn_*` VERBATIM (only the AluOp + the LS/left decode arm differ). Each anchor is a
/// real vendored case run through both drivers + the per-cycle transaction stream via `run_case`; the
/// load-bearing pins:
///
/// - `e30a [LSL.b Q, D2] 4` (len 8) — REGISTER **immediate** `.b`, cnt 1 → timing `6 + 2*1`.
/// - `e10f [LSL.b Q, D7] 3` (len 22) — REGISTER immediate `.b`, `ccc == 0` → cnt **8** → `6 + 2*8`.
/// - `e74b [LSL.w Q, D3] 6` (len 12) — REGISTER immediate `.w`, cnt 3, **THE V-SUPPRESSION PIN**: the operand
///   is one an ASL would mark `V = 1` (the sign bit changes during the shift) — LSL MUST keep `V = 0` (its
///   sole difference from ASL). Timing `6 + 2*3`.
/// - `e38d [LSL.l Q, D5] 21` (len 10) — REGISTER immediate `.l`, cnt 1 → the `.l` base `8 + 2*1`.
/// - `e529 [LSL.b D2, D1] 28` (len 40) — **`cnt >= n` over-shift**: a `Dn`-count `cnt = 17 > n = 8` → the
///   register is CLEARED (`res = 0`, Z set, N = 0) and **C = 0** (`cnt > n` shifts nothing meaningful out),
///   X = C = 0, V = 0. Timing `6 + 2*17` (the live `Dn & 63` driving the idle).
/// - `e36b [LSL.w D1, D3] 271` (len 6) — REGISTER **`Dn`-count `cnt == 0`** (the ZERO-COUNT special case):
///   value UNCHANGED, **X PRESERVED** (X1 in → X1 out), V = 0, C = 0, timing `6` (`6 + 2*0`).
/// - `e3d5 [LSL.w (A5)] 91` (len 12) — `.w` **memory** shift-by-1 `(An)`: the word `ea_dst` RMW.
/// - `e3d7 [LSL.w (A7)] 159` (len 12) — the plain `(A7)` mode-2 indirect (`mode == 2 && reg == 7`), COVERED
///   (NOT deferred), a clean word RMW at the active A7.
/// - `e3de [LSL.w (A6)+] 1` (len 50) — an **odd-EA** `.w` memory address error (the E3/E4 abort installs the
///   group-0 14-byte vector-3 frame), which must PASS unchanged.
///
/// Every anchor must decode as an LSL opcode (`0xExxx`, type LS / direction LEFT) — never any other family.
#[test]
fn lsl_anchors_match_singlesteptests() {
    let anchors: &[(&str, &str, u32)] = &[
        ("LSL.b.json", "e30a [LSL.b Q, D2] 4", 8), // imm .b cnt1 → 6+2
        ("LSL.b.json", "e10f [LSL.b Q, D7] 3", 22), // imm .b ccc=0 → cnt8 → 6+16
        ("LSL.w.json", "e74b [LSL.w Q, D3] 6", 12), // imm .w cnt3, V-SUPPRESSION (ASL would set V → LSL V=0)
        ("LSL.l.json", "e38d [LSL.l Q, D5] 21", 10), // imm .l cnt1 → 8+2
        ("LSL.b.json", "e529 [LSL.b D2, D1] 28", 40), // Dn cnt17 > n=8 → res=0, C=0, V=0, 6+34
        ("LSL.w.json", "e36b [LSL.w D1, D3] 271", 6), // Dn cnt0 (zero-count), X kept
        ("LSL.w.json", "e3d5 [LSL.w (A5)] 91", 12), // memory (An) shift-by-1
        ("LSL.w.json", "e3d7 [LSL.w (A7)] 159", 12), // (A7) mode-2 indirect — COVERED
        ("LSL.w.json", "e3de [LSL.w (A6)+] 1", 50), // odd-EA memory address error
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("S2 LSL anchor {name} (len {length}) not found in {fname}"));
        // Every anchor must be an LSL opcode: 0xExxx, type LS (register bits 4-3 == 1 / memory bits 10-9 ==
        // 1), direction LEFT (bit 8 == 1).
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode >> 12,
            0xE,
            "anchor {name} must be a 0xExxx shift opcode"
        );
        let is_lsl = if (opcode >> 6) & 3 == 3 {
            (opcode >> 8) & 1 == 1 && (opcode >> 9) & 3 == 1
        } else {
            (opcode >> 8) & 1 == 1 && (opcode >> 3) & 3 == 1
        };
        assert!(
            is_lsl,
            "anchor {name} must be LSL (type LS, direction LEFT)"
        );
        // Load-bearing flag/scope pins (run_case verifies the FULL final state against the data either way).
        let ini_sr = case["initial"]["sr"].as_u64().unwrap() as u16;
        let fin_sr = case["final"]["sr"].as_u64().unwrap() as u16;
        match *name {
            "e74b [LSL.w Q, D3] 6" => {
                // THE V-SUPPRESSION PIN: this exact operand/count is one an ASL would mark V=1 (the sign bit
                // changes during the shift). LSL MUST keep V=0 — its ONLY difference from ASL. Recompute ASL's
                // closed-form V here to prove the case really would set V under ASL, then assert LSL's final
                // V is cleared.
                let cnt = ((opcode >> 9) & 7) as u32; // imm: ccc != 0 → 3 (this anchor)
                let x = (case["initial"]["d3"].as_u64().unwrap() as u32) & 0xFFFF;
                let (n, mask) = (16u32, 0xFFFFu32);
                let top_mask = mask & !((1u32 << (n - 1 - cnt)) - 1);
                let top = x & top_mask;
                let asl_would_set_v = top != 0 && top != top_mask;
                assert!(
                    asl_would_set_v,
                    "the V-suppression anchor must be a case ASL would mark V=1"
                );
                assert_eq!(
                    fin_sr & 0x02,
                    0,
                    "LSL must FORCE V=0 even where ASL would set it (the sole LSL/ASL difference)"
                );
            }
            "e529 [LSL.b D2, D1] 28" => {
                // cnt = 17 > n = 8 over-shift: the register is CLEARED (res = 0 → Z set, N = 0) and C = 0
                // (X = C = 0), V = 0.
                assert_ne!(fin_sr & 0x04, 0, "cnt>=n over-shift must set Z (res = 0)");
                assert_eq!(fin_sr & 0x08, 0, "cnt>=n over-shift must clear N (res = 0)");
                assert_eq!(fin_sr & 0x01, 0, "cnt>n over-shift must clear C");
                assert_eq!(
                    fin_sr & 0x10,
                    0,
                    "cnt>n over-shift must clear X (X = C = 0)"
                );
                assert_eq!(fin_sr & 0x02, 0, "LSL must always clear V");
            }
            "e36b [LSL.w D1, D3] 271" => {
                // Zero-count: X PRESERVED (not set to C), V and C cleared, value unchanged.
                assert_eq!(ini_sr & 0x10, fin_sr & 0x10, "zero-count must PRESERVE X");
                assert_ne!(
                    ini_sr & 0x10,
                    0,
                    "zero-count anchor must enter with X=1 (pins preservation)"
                );
                assert_eq!(fin_sr & 0x02, 0, "zero-count must clear V");
                assert_eq!(fin_sr & 0x01, 0, "zero-count must clear C");
                assert_eq!(
                    case["initial"]["d3"], case["final"]["d3"],
                    "zero-count must leave the operand unchanged"
                );
            }
            "e3d7 [LSL.w (A7)] 159" => {
                assert_eq!((opcode >> 3) & 7, 2, "(A7) anchor must be mode 2");
                assert_eq!(opcode & 7, 7, "(A7) anchor must be reg 7 (the A7 indirect)");
            }
            "e3de [LSL.w (A6)+] 1" => {
                // Odd-EA address error: the group-0 frame pushes the SSP down (the standard 14-byte frame).
                assert!(
                    case["final"]["ssp"].as_u64().unwrap()
                        < case["initial"]["ssp"].as_u64().unwrap(),
                    "odd-EA anchor must install the address-error frame (SSP pushed down)"
                );
            }
            _ => {}
        }
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all S2 LSL anchors exercised");
    eprintln!(
        "S2 LSL anchors: {found} cases (imm .b/.w/.l 6+2cnt / 8+2cnt, the V-SUPPRESSION pin (ASL would set V → LSL keeps V=0), cnt>n over-shift res=0/C=0, Dn cnt0 zero-count X-preserved, (An)/(A7) memory shift-by-1, odd-EA address-error) passed both drivers"
    );
}

/// S2 — the snapshot/restore anchor for the LSL.w memory shift-by-1 (the shared `shift_recipe` word `ea_dst`
/// RMW: `[Read, Prefetch, Alu, Write]`). Drives a real vendored `LSL.w (A5)` case through the quiesce driver,
/// snapshotting + restoring the WHOLE `Cpu68000` (incl. the in-flight cursor) at every micro-op boundary —
/// including the mid-bus-access boundary between the operand Read and the result Write — and proves the
/// resumed run reproduces the run-to-completion final state + transaction stream bit-for-bit. This pins that
/// `AluOp::Lsl` keeps `MicroState` fixed-size bincode (it stays `Copy`).
#[test]
fn lsl_w_mem_quiescable_and_serializable_at_every_micro_op_boundary() {
    let path = format!("{VENDOR_DIR}/LSL.w.json");
    if !Path::new(&path).exists() {
        eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
        return;
    }
    let file = std::fs::File::open(&path).unwrap();
    let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
    let case = data
        .iter()
        .find(|t| t["name"].as_str().unwrap() == "e3d5 [LSL.w (A5)] 91")
        .expect("LSL.w (A5) snapshot anchor present");
    let ini = &case["initial"];

    // Run-to-completion reference.
    let mut rref = Cpu68000::new(build_regs(ini));
    let mut bref = build_bus(ini);
    rref.run_instruction(&mut bref);

    let cfg = bincode::config::standard();
    // 4 micro-ops (Read, Prefetch, Alu, Write) → in-flight boundaries after 0..=3 of them.
    for pause_after in 0..=3 {
        let mut cpu = Cpu68000::new(build_regs(ini));
        let mut bus = build_bus(ini);
        cpu.start_instruction();
        for _ in 0..pause_after {
            assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        }
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        loop {
            if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                break;
            }
        }
        assert_eq!(
            cpu2.regs, rref.regs,
            "resume from boundary {pause_after} diverged"
        );
        assert_eq!(
            bus.log, bref.log,
            "transaction stream from boundary {pause_after} diverged"
        );
    }
    eprintln!(
        "S2 LSL snapshot/restore: LSL.w (A5) word shift-by-1 RMW resumed identically at every micro-op boundary"
    );
}

/// S3 — the named LSR anchors, pinning logical shift RIGHT against the vendored LSR.b/.w/.l stream WITHOUT
/// relying on the bulk `covered()` sweep. LSR is the ZERO-FILL right shift (contrast ASR, which sign-EXTENDS);
/// it reuses S0's `shift_recipe`/`Operand::ShiftCount`/`dn_*` VERBATIM (only the AluOp + the LS/right decode
/// arm differ). Each anchor is a real vendored case run through both drivers + the per-cycle transaction
/// stream via `run_case`; the load-bearing pins:
///
/// - `e20f [LSR.b Q, D7] 10` (len 8) — REGISTER **immediate** `.b`, cnt 1 → timing `6 + 2*1`. **THE ZERO-FILL
///   PIN**: the operand byte's msb is SET (0x..dc) yet LSR fills 0 → final **N = 0** (an ASR would sign-extend
///   and keep N=1 — this proves LSR zero-fills, the sole value difference from ASR).
/// - `e00c [LSR.b Q, D4] 2` (len 22) — REGISTER immediate `.b`, `ccc == 0` → cnt **8** → `6 + 2*8`.
/// - `e64c [LSR.w Q, D4] 47` (len 12) — REGISTER immediate `.w`, cnt 3 → `6 + 2*3`.
/// - `e28f [LSR.l Q, D7] 23` (len 10) — REGISTER immediate `.l`, cnt 1 → the `.l` base `8 + 2*1`.
/// - `ee29 [LSR.b D7, D1] 6` (len 40) — **`cnt > n` over-shift**: a `Dn`-count `cnt = 17 > n = 8` → the
///   register is CLEARED (`res = 0`, Z set, N = 0) and **C = 0** (zero-fill has nothing left to shift out),
///   X = C = 0, V = 0. Timing `6 + 2*17` (the live `Dn & 63` driving the idle).
/// - `e86a [LSR.w D4, D2] 90` (len 6) — REGISTER **`Dn`-count `cnt == 0`** (the ZERO-COUNT special case): the
///   operand's msb is SET → value UNCHANGED → final **N = 1 from the operand** (NOT forced to 0 — the LSR
///   "N = 0 for cnt >= 1" rule does NOT apply when the shift never ran); **X PRESERVED** (X1 in → X1 out),
///   V = 0, C = 0, timing `6` (`6 + 2*0`).
/// - `e2d5 [LSR.w (A5)] 91` (len 12) — `.w` **memory** shift-by-1 `(An)`: the word `ea_dst` RMW.
/// - `e2d7 [LSR.w (A7)] 156` (len 12) — the plain `(A7)` mode-2 indirect (`mode == 2 && reg == 7`), COVERED
///   (NOT deferred), a clean word RMW at the active A7.
/// - `e2d1 [LSR.w (A1)] 27` (len 50) — an **odd-EA** `.w` memory address error (the E3/E4 abort installs the
///   group-0 14-byte vector-3 frame), which must PASS unchanged.
///
/// Every anchor must decode as an LSR opcode (`0xExxx`, type LS / direction RIGHT) — never any other family.
#[test]
fn lsr_anchors_match_singlesteptests() {
    let anchors: &[(&str, &str, u32)] = &[
        ("LSR.b.json", "e20f [LSR.b Q, D7] 10", 8), // imm .b cnt1 → 6+2, ZERO-FILL N=0 (operand msb set)
        ("LSR.b.json", "e00c [LSR.b Q, D4] 2", 22), // imm .b ccc=0 → cnt8 → 6+16
        ("LSR.w.json", "e64c [LSR.w Q, D4] 47", 12), // imm .w cnt3 → 6+6
        ("LSR.l.json", "e28f [LSR.l Q, D7] 23", 10), // imm .l cnt1 → 8+2
        ("LSR.b.json", "ee29 [LSR.b D7, D1] 6", 40), // Dn cnt17 > n=8 → res=0, C=0, V=0, 6+34
        ("LSR.w.json", "e86a [LSR.w D4, D2] 90", 6), // Dn cnt0 (zero-count), N from operand, X kept
        ("LSR.w.json", "e2d5 [LSR.w (A5)] 91", 12), // memory (An) shift-by-1
        ("LSR.w.json", "e2d7 [LSR.w (A7)] 156", 12), // (A7) mode-2 indirect — COVERED
        ("LSR.w.json", "e2d1 [LSR.w (A1)] 27", 50), // odd-EA memory address error
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("S3 LSR anchor {name} (len {length}) not found in {fname}"));
        // Every anchor must be an LSR opcode: 0xExxx, type LS (register bits 4-3 == 1 / memory bits 10-9 ==
        // 1), direction RIGHT (bit 8 == 0).
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode >> 12,
            0xE,
            "anchor {name} must be a 0xExxx shift opcode"
        );
        let is_lsr = if (opcode >> 6) & 3 == 3 {
            (opcode >> 8) & 1 == 0 && (opcode >> 9) & 3 == 1
        } else {
            (opcode >> 8) & 1 == 0 && (opcode >> 3) & 3 == 1
        };
        assert!(
            is_lsr,
            "anchor {name} must be LSR (type LS, direction RIGHT)"
        );
        // Load-bearing flag/scope pins (run_case verifies the FULL final state against the data either way).
        let ini_sr = case["initial"]["sr"].as_u64().unwrap() as u16;
        let fin_sr = case["final"]["sr"].as_u64().unwrap() as u16;
        match *name {
            "e20f [LSR.b Q, D7] 10" => {
                // THE ZERO-FILL PIN: the operand byte's msb is SET, but LSR zero-fills the vacated top bit, so
                // the final N MUST be 0 (an ASR would sign-extend and keep N=1 — this is LSR's sole value
                // difference from ASR). cnt = 1 (< n) so the "N=0 for cnt>=1" rule applies.
                let rrr = (opcode & 7) as usize;
                let operand = case["initial"][format!("d{rrr}")].as_u64().unwrap() as u32;
                assert_ne!(
                    operand & 0x80,
                    0,
                    "the zero-fill anchor must have the operand byte's msb SET (so ASR would keep N=1)"
                );
                assert_eq!(
                    fin_sr & 0x08,
                    0,
                    "LSR must ZERO-FILL → N = 0 for cnt>=1 even when the operand's msb was set"
                );
                assert_eq!(fin_sr & 0x02, 0, "LSR must always clear V");
            }
            "ee29 [LSR.b D7, D1] 6" => {
                // cnt = 17 > n = 8 over-shift: the register is CLEARED (res = 0 → Z set, N = 0) and C = 0
                // (zero-fill has nothing left to shift out; X = C = 0), V = 0.
                assert_ne!(fin_sr & 0x04, 0, "cnt>n over-shift must set Z (res = 0)");
                assert_eq!(fin_sr & 0x08, 0, "cnt>n over-shift must clear N (res = 0)");
                assert_eq!(fin_sr & 0x01, 0, "cnt>n over-shift must clear C");
                assert_eq!(
                    fin_sr & 0x10,
                    0,
                    "cnt>n over-shift must clear X (X = C = 0)"
                );
                assert_eq!(fin_sr & 0x02, 0, "LSR must always clear V");
            }
            "e86a [LSR.w D4, D2] 90" => {
                // Zero-count: value unchanged, X PRESERVED (not set to C), V and C cleared. The operand's msb
                // is SET, so N comes FROM THE OPERAND (final N = 1) — NOT forced to 0 (the LSR zero-fill N=0
                // rule applies only when the shift actually ran, i.e. cnt >= 1).
                let rrr = (opcode & 7) as usize;
                let operand = case["initial"][format!("d{rrr}")].as_u64().unwrap() as u32;
                assert_ne!(
                    operand & 0x8000,
                    0,
                    "the zero-count anchor must have the operand word's msb SET (pins N-from-operand)"
                );
                assert_ne!(
                    fin_sr & 0x08,
                    0,
                    "zero-count must take N FROM THE OPERAND (here 1) — NOT force it to 0"
                );
                assert_eq!(ini_sr & 0x10, fin_sr & 0x10, "zero-count must PRESERVE X");
                assert_ne!(
                    ini_sr & 0x10,
                    0,
                    "zero-count anchor must enter with X=1 (pins preservation)"
                );
                assert_eq!(fin_sr & 0x02, 0, "zero-count must clear V");
                assert_eq!(fin_sr & 0x01, 0, "zero-count must clear C");
                assert_eq!(
                    case["initial"]["d2"], case["final"]["d2"],
                    "zero-count must leave the operand unchanged"
                );
            }
            "e2d7 [LSR.w (A7)] 156" => {
                assert_eq!((opcode >> 3) & 7, 2, "(A7) anchor must be mode 2");
                assert_eq!(opcode & 7, 7, "(A7) anchor must be reg 7 (the A7 indirect)");
            }
            "e2d1 [LSR.w (A1)] 27" => {
                // Odd-EA address error: the group-0 frame pushes the SSP down (the standard 14-byte frame).
                assert!(
                    case["final"]["ssp"].as_u64().unwrap()
                        < case["initial"]["ssp"].as_u64().unwrap(),
                    "odd-EA anchor must install the address-error frame (SSP pushed down)"
                );
            }
            _ => {}
        }
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all S3 LSR anchors exercised");
    eprintln!(
        "S3 LSR anchors: {found} cases (imm .b/.w/.l 6+2cnt / 8+2cnt, the ZERO-FILL pin (operand msb set → N=0, the ASR distinction), cnt>n over-shift res=0/C=0, Dn cnt0 zero-count N-from-operand/X-preserved, (An)/(A7) memory shift-by-1, odd-EA address-error) passed both drivers"
    );
}

/// S3 — the snapshot/restore anchor for the LSR.w memory shift-by-1 (the shared `shift_recipe` word `ea_dst`
/// RMW: `[Read, Prefetch, Alu, Write]`). Drives a real vendored `LSR.w (A5)` case through the quiesce driver,
/// snapshotting + restoring the WHOLE `Cpu68000` (incl. the in-flight cursor) at every micro-op boundary —
/// including the mid-bus-access boundary between the operand Read and the result Write — and proves the
/// resumed run reproduces the run-to-completion final state + transaction stream bit-for-bit. This pins that
/// `AluOp::Lsr` keeps `MicroState` fixed-size bincode (it stays `Copy`).
#[test]
fn lsr_w_mem_quiescable_and_serializable_at_every_micro_op_boundary() {
    let path = format!("{VENDOR_DIR}/LSR.w.json");
    if !Path::new(&path).exists() {
        eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
        return;
    }
    let file = std::fs::File::open(&path).unwrap();
    let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
    let case = data
        .iter()
        .find(|t| t["name"].as_str().unwrap() == "e2d5 [LSR.w (A5)] 91")
        .expect("LSR.w (A5) snapshot anchor present");
    let ini = &case["initial"];

    // Run-to-completion reference.
    let mut rref = Cpu68000::new(build_regs(ini));
    let mut bref = build_bus(ini);
    rref.run_instruction(&mut bref);

    let cfg = bincode::config::standard();
    // 4 micro-ops (Read, Prefetch, Alu, Write) → in-flight boundaries after 0..=3 of them.
    for pause_after in 0..=3 {
        let mut cpu = Cpu68000::new(build_regs(ini));
        let mut bus = build_bus(ini);
        cpu.start_instruction();
        for _ in 0..pause_after {
            assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        }
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        loop {
            if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                break;
            }
        }
        assert_eq!(
            cpu2.regs, rref.regs,
            "resume from boundary {pause_after} diverged"
        );
        assert_eq!(
            bus.log, bref.log,
            "transaction stream from boundary {pause_after} diverged"
        );
    }
    eprintln!(
        "S3 LSR snapshot/restore: LSR.w (A5) word shift-by-1 RMW resumed identically at every micro-op boundary"
    );
}

/// S4 — the named ROL anchors, pinning rotate LEFT against the vendored ROL.b/.w/.l stream WITHOUT relying on
/// the bulk `covered()` sweep. ROL is a plain bit-rotate that does NOT pass through X (contrast ROXL, which
/// threads X — S6); it reuses S0's `shift_recipe`/`Operand::ShiftCount`/`dn_*` VERBATIM (only the AluOp + the
/// RO/left decode arm differ). Each anchor is a real vendored case run through both drivers + the per-cycle
/// transaction stream via `run_case`; the load-bearing pins:
///
/// - `e31b [ROL.b Q, D3] 26` (len 8) — REGISTER **immediate** `.b`, cnt 1 → timing `6 + 2*1` → a genuine
///   `cnt % n != 0` rotate. **X PRESERVE PIN**: the case enters with X = 1; ROL does NOT touch X → final X = 1
///   (an ASL/ASR/LSL/LSR/ROXL would have set X = C here).
/// - `e118 [ROL.b Q, D0] 30` (len 22) — REGISTER immediate `.b`, `ccc == 0` → cnt **8** → `6 + 2*8`. **`cnt %
///   n == 0` with `cnt != 0`**: the WHOLE byte rotates back to itself → value UNCHANGED, yet **C comes from
///   the formula** `(x >> ((n - (cnt % n)) % n)) & 1 = x & 1` (NOT 0 — only `cnt == 0` clears C).
/// - `e75a [ROL.w Q, D2] 5` (len 12) — REGISTER immediate `.w`, cnt 3 → `6 + 2*3`.
/// - `e39d [ROL.l Q, D5] 16` (len 10) — REGISTER immediate `.l`, cnt 1 → the `.l` base `8 + 2*1`.
/// - `e91d [ROL.b Q, D5] 12` (len 14) — REGISTER immediate `.b`, cnt 4, **incoming X = 1** confirmed to STAY 1
///   through an actual rotate (a second X-untouched pin, this time with `cnt % n != 0`).
/// - `ed7a [ROL.w D6, D2] 558` (len 6) — REGISTER **`Dn`-count `cnt == 0`** (the ZERO-COUNT case): value
///   UNCHANGED, **C = 0** (a zero count clears C — the sole way ROL clears C), **X PRESERVED** (X1 in → X1
///   out), V = 0, timing `6` (`6 + 2*0`).
/// - `e7d1 [ROL.w (A1)] 97` (len 12) — `.w` **memory** rotate-by-1 `(An)`: the word `ea_dst` RMW.
/// - `e7d7 [ROL.w (A7)] 328` (len 12) — the plain `(A7)` mode-2 indirect (`mode == 2 && reg == 7`), COVERED
///   (NOT deferred), a clean word RMW at the active A7.
/// - `e7d1 [ROL.w (A1)] 146` (len 50) — an **odd-EA** `.w` memory address error (the E3/E4 abort installs the
///   group-0 14-byte vector-3 frame), which must PASS unchanged.
///
/// Every anchor must decode as a ROL opcode (`0xExxx`, type RO / direction LEFT) — never any other family.
#[test]
fn rol_anchors_match_singlesteptests() {
    let anchors: &[(&str, &str, u32)] = &[
        ("ROL.b.json", "e31b [ROL.b Q, D3] 26", 8), // imm .b cnt1 → 6+2, cnt%n!=0, X=1 preserved
        ("ROL.b.json", "e118 [ROL.b Q, D0] 30", 22), // imm .b ccc=0 → cnt8, cnt%n==0: value kept, C=x&1
        ("ROL.w.json", "e75a [ROL.w Q, D2] 5", 12),  // imm .w cnt3 → 6+6
        ("ROL.l.json", "e39d [ROL.l Q, D5] 16", 10), // imm .l cnt1 → 8+2
        ("ROL.b.json", "e91d [ROL.b Q, D5] 12", 14), // imm .b cnt4, X=1 in stays 1 (rotate)
        ("ROL.w.json", "ed7a [ROL.w D6, D2] 558", 6), // Dn cnt0 (zero-count): C=0, X kept, value unchanged
        ("ROL.w.json", "e7d1 [ROL.w (A1)] 97", 12),   // memory (An) rotate-by-1
        ("ROL.w.json", "e7d7 [ROL.w (A7)] 328", 12),  // (A7) mode-2 indirect — COVERED
        ("ROL.w.json", "e7d1 [ROL.w (A1)] 146", 50),  // odd-EA memory address error
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("S4 ROL anchor {name} (len {length}) not found in {fname}"));
        // Every anchor must be a ROL opcode: 0xExxx, type RO (register bits 4-3 == 3 / memory bits 10-9 ==
        // 3), direction LEFT (bit 8 == 1).
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode >> 12,
            0xE,
            "anchor {name} must be a 0xExxx shift opcode"
        );
        let is_rol = if (opcode >> 6) & 3 == 3 {
            (opcode >> 8) & 1 == 1 && (opcode >> 9) & 3 == 3
        } else {
            (opcode >> 8) & 1 == 1 && (opcode >> 3) & 3 == 3
        };
        assert!(
            is_rol,
            "anchor {name} must be ROL (type RO, direction LEFT)"
        );
        // Load-bearing flag/scope pins (run_case verifies the FULL final state against the data either way).
        let ini_sr = case["initial"]["sr"].as_u64().unwrap() as u16;
        let fin_sr = case["final"]["sr"].as_u64().unwrap() as u16;
        match *name {
            "e31b [ROL.b Q, D3] 26" => {
                // THE X-PRESERVE PIN: a genuine cnt%n != 0 rotate. ROL does NOT touch X, so an incoming X = 1
                // MUST stay X = 1 in the final SR (an arithmetic/logical shift or ROXL would set X = C). V = 0.
                assert_ne!(
                    ini_sr & 0x10,
                    0,
                    "the X-preserve anchor must enter with X = 1 (pins ROL leaving X untouched)"
                );
                assert_ne!(
                    fin_sr & 0x10,
                    0,
                    "ROL must PRESERVE X — incoming X = 1 stays 1 (X is NOT set to C)"
                );
                assert_eq!(fin_sr & 0x02, 0, "ROL must always clear V");
            }
            "e118 [ROL.b Q, D0] 30" => {
                // cnt = 8 == n: a WHOLE-byte rotation leaves the value unchanged (r == 0), but C still comes
                // from the formula `(x >> ((n - cnt%n) % n)) & 1` = `x & 1` (NOT 0 — only cnt == 0 clears C).
                let rrr = (opcode & 7) as usize;
                let operand = case["initial"][format!("d{rrr}")].as_u64().unwrap() as u32;
                assert_eq!(
                    operand & 0xFF,
                    case["final"][format!("d{rrr}")].as_u64().unwrap() as u32 & 0xFF,
                    "cnt%n==0 (cnt!=0) must leave the rotated byte unchanged (r == 0)"
                );
                assert_eq!(
                    fin_sr & 0x01,
                    (operand & 1) as u16,
                    "cnt%n==0 (cnt!=0) C must come from the formula (= x & 1), NOT be cleared"
                );
                assert_eq!(fin_sr & 0x02, 0, "ROL must always clear V");
            }
            "e91d [ROL.b Q, D5] 12" => {
                // A second X-untouched pin: incoming X = 1, an actual rotate (cnt = 4, cnt%n != 0), X stays 1.
                assert_ne!(ini_sr & 0x10, 0, "anchor must enter with X = 1");
                assert_ne!(fin_sr & 0x10, 0, "ROL must leave X untouched (1 → 1)");
                assert_eq!(fin_sr & 0x02, 0, "ROL must always clear V");
            }
            "ed7a [ROL.w D6, D2] 558" => {
                // Zero-count (Dn count = 0): value unchanged, X PRESERVED (not set to C), V = 0, and C = 0 (a
                // zero count is the ONLY way ROL clears C). The case enters with X = 1, pinning preservation.
                assert_eq!(ini_sr & 0x10, fin_sr & 0x10, "zero-count must PRESERVE X");
                assert_ne!(
                    ini_sr & 0x10,
                    0,
                    "zero-count anchor must enter with X = 1 (pins preservation)"
                );
                assert_eq!(fin_sr & 0x01, 0, "zero-count must clear C");
                assert_eq!(fin_sr & 0x02, 0, "zero-count must clear V");
                assert_eq!(
                    case["initial"]["d2"], case["final"]["d2"],
                    "zero-count must leave the operand unchanged"
                );
            }
            "e7d7 [ROL.w (A7)] 328" => {
                assert_eq!((opcode >> 3) & 7, 2, "(A7) anchor must be mode 2");
                assert_eq!(opcode & 7, 7, "(A7) anchor must be reg 7 (the A7 indirect)");
            }
            "e7d1 [ROL.w (A1)] 146" => {
                // Odd-EA address error: the group-0 frame pushes the SSP down (the standard 14-byte frame).
                assert!(
                    case["final"]["ssp"].as_u64().unwrap()
                        < case["initial"]["ssp"].as_u64().unwrap(),
                    "odd-EA anchor must install the address-error frame (SSP pushed down)"
                );
            }
            _ => {}
        }
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all S4 ROL anchors exercised");
    eprintln!(
        "S4 ROL anchors: {found} cases (imm .b/.w/.l 6+2cnt / 8+2cnt, X-preserve pins (incoming X=1 stays 1), cnt%n==0 value-kept/C-from-formula, Dn cnt0 zero-count C=0/X-kept, (An)/(A7) memory rotate-by-1, odd-EA address-error) passed both drivers"
    );
}

/// S4 — the snapshot/restore anchor for the ROL.w memory rotate-by-1 (the shared `shift_recipe` word `ea_dst`
/// RMW: `[Read, Prefetch, Alu, Write]`). Drives a real vendored `ROL.w (A1)` case through the quiesce driver,
/// snapshotting + restoring the WHOLE `Cpu68000` (incl. the in-flight cursor) at every micro-op boundary —
/// including the mid-bus-access boundary between the operand Read and the result Write — and proves the
/// resumed run reproduces the run-to-completion final state + transaction stream bit-for-bit. This pins that
/// `AluOp::Rol` keeps `MicroState` fixed-size bincode (it stays `Copy`).
#[test]
fn rol_w_mem_quiescable_and_serializable_at_every_micro_op_boundary() {
    let path = format!("{VENDOR_DIR}/ROL.w.json");
    if !Path::new(&path).exists() {
        eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
        return;
    }
    let file = std::fs::File::open(&path).unwrap();
    let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
    let case = data
        .iter()
        .find(|t| t["name"].as_str().unwrap() == "e7d1 [ROL.w (A1)] 97")
        .expect("ROL.w (A1) snapshot anchor present");
    let ini = &case["initial"];

    // Run-to-completion reference.
    let mut rref = Cpu68000::new(build_regs(ini));
    let mut bref = build_bus(ini);
    rref.run_instruction(&mut bref);

    let cfg = bincode::config::standard();
    // 4 micro-ops (Read, Prefetch, Alu, Write) → in-flight boundaries after 0..=3 of them.
    for pause_after in 0..=3 {
        let mut cpu = Cpu68000::new(build_regs(ini));
        let mut bus = build_bus(ini);
        cpu.start_instruction();
        for _ in 0..pause_after {
            assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        }
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        loop {
            if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                break;
            }
        }
        assert_eq!(
            cpu2.regs, rref.regs,
            "resume from boundary {pause_after} diverged"
        );
        assert_eq!(
            bus.log, bref.log,
            "transaction stream from boundary {pause_after} diverged"
        );
    }
    eprintln!(
        "S4 ROL snapshot/restore: ROL.w (A1) word rotate-by-1 RMW resumed identically at every micro-op boundary"
    );
}

/// S5 — the named ROR anchors, pinning rotate RIGHT against the vendored ROR.b/.w/.l stream WITHOUT relying on
/// the bulk `covered()` sweep. ROR is ROL's right-direction twin — a plain bit-rotate that does NOT pass
/// through X (contrast ROXR, which threads X — S7); it reuses S0's `shift_recipe`/`Operand::ShiftCount`/`dn_*`
/// VERBATIM (only the AluOp + the RO/right decode arm differ). Each anchor is a real vendored case run through
/// both drivers + the per-cycle transaction stream via `run_case`; the load-bearing pins:
///
/// - `e219 [ROR.b Q, D1] 5` (len 8) — REGISTER **immediate** `.b`, cnt 1 → timing `6 + 2*1` → a genuine
///   `cnt % n != 0` rotate. **X PRESERVE PIN**: the case enters with X = 1; ROR does NOT touch X → final X = 1
///   (an ASL/ASR/LSL/LSR/ROXR would have set X = C here).
/// - `e01b [ROR.b Q, D3] 9` (len 22) — REGISTER immediate `.b`, `ccc == 0` → cnt **8** → `6 + 2*8`. **`cnt %
///   n == 0` with `cnt != 0`**: the WHOLE byte rotates back to itself → value UNCHANGED, yet **C comes from
///   the formula** `(x >> ((cnt - 1) % n)) & 1 = (x >> 7) & 1` (the MSB — NOT 0, only `cnt == 0` clears C; and
///   NOT `x & 1`, which is ROL's left-direction low bit).
/// - `e65e [ROR.w Q, D6] 2` (len 12) — REGISTER immediate `.w`, cnt 3 → `6 + 2*3`.
/// - `e29f [ROR.l Q, D7] 3` (len 10) — REGISTER immediate `.l`, cnt 1 → the `.l` base `8 + 2*1`.
/// - `e81b [ROR.b Q, D3] 17` (len 14) — REGISTER immediate `.b`, cnt 4, **incoming X = 1** confirmed to STAY 1
///   through an actual rotate (a second X-untouched pin, this time with `cnt % n != 0`).
/// - `e27a [ROR.w D1, D2] 460` (len 6) — REGISTER **`Dn`-count `cnt == 0`** (the ZERO-COUNT case): value
///   UNCHANGED, **C = 0** (a zero count clears C — the sole way ROR clears C), **X PRESERVED** (X1 in → X1
///   out), V = 0, timing `6` (`6 + 2*0`).
/// - `e6d5 [ROR.w (A5)] 44` (len 12) — `.w` **memory** rotate-by-1 `(An)`: the word `ea_dst` RMW.
/// - `e6d7 [ROR.w (A7)] 19` (len 12) — the plain `(A7)` mode-2 indirect (`mode == 2 && reg == 7`), COVERED
///   (NOT deferred), a clean word RMW at the active A7.
/// - `e6d5 [ROR.w (A5)] 15` (len 50) — an **odd-EA** `.w` memory address error (the E3/E4 abort installs the
///   group-0 14-byte vector-3 frame), which must PASS unchanged.
///
/// Every anchor must decode as a ROR opcode (`0xExxx`, type RO / direction RIGHT) — never any other family.
#[test]
fn ror_anchors_match_singlesteptests() {
    let anchors: &[(&str, &str, u32)] = &[
        ("ROR.b.json", "e219 [ROR.b Q, D1] 5", 8), // imm .b cnt1 → 6+2, cnt%n!=0, X=1 preserved
        ("ROR.b.json", "e01b [ROR.b Q, D3] 9", 22), // imm .b ccc=0 → cnt8, cnt%n==0: value kept, C=(x>>7)&1
        ("ROR.w.json", "e65e [ROR.w Q, D6] 2", 12), // imm .w cnt3 → 6+6
        ("ROR.l.json", "e29f [ROR.l Q, D7] 3", 10), // imm .l cnt1 → 8+2
        ("ROR.b.json", "e81b [ROR.b Q, D3] 17", 14), // imm .b cnt4, X=1 in stays 1 (rotate)
        ("ROR.w.json", "e27a [ROR.w D1, D2] 460", 6), // Dn cnt0 (zero-count): C=0, X kept, value unchanged
        ("ROR.w.json", "e6d5 [ROR.w (A5)] 44", 12),   // memory (An) rotate-by-1
        ("ROR.w.json", "e6d7 [ROR.w (A7)] 19", 12),   // (A7) mode-2 indirect — COVERED
        ("ROR.w.json", "e6d5 [ROR.w (A5)] 15", 50),   // odd-EA memory address error
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("S5 ROR anchor {name} (len {length}) not found in {fname}"));
        // Every anchor must be a ROR opcode: 0xExxx, type RO (register bits 4-3 == 3 / memory bits 10-9 ==
        // 3), direction RIGHT (bit 8 == 0).
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode >> 12,
            0xE,
            "anchor {name} must be a 0xExxx shift opcode"
        );
        let is_ror = if (opcode >> 6) & 3 == 3 {
            (opcode >> 8) & 1 == 0 && (opcode >> 9) & 3 == 3
        } else {
            (opcode >> 8) & 1 == 0 && (opcode >> 3) & 3 == 3
        };
        assert!(
            is_ror,
            "anchor {name} must be ROR (type RO, direction RIGHT)"
        );
        // Load-bearing flag/scope pins (run_case verifies the FULL final state against the data either way).
        let ini_sr = case["initial"]["sr"].as_u64().unwrap() as u16;
        let fin_sr = case["final"]["sr"].as_u64().unwrap() as u16;
        match *name {
            "e219 [ROR.b Q, D1] 5" => {
                // THE X-PRESERVE PIN: a genuine cnt%n != 0 rotate. ROR does NOT touch X, so an incoming X = 1
                // MUST stay X = 1 in the final SR (an arithmetic/logical shift or ROXR would set X = C). V = 0.
                assert_ne!(
                    ini_sr & 0x10,
                    0,
                    "the X-preserve anchor must enter with X = 1 (pins ROR leaving X untouched)"
                );
                assert_ne!(
                    fin_sr & 0x10,
                    0,
                    "ROR must PRESERVE X — incoming X = 1 stays 1 (X is NOT set to C)"
                );
                assert_eq!(fin_sr & 0x02, 0, "ROR must always clear V");
            }
            "e01b [ROR.b Q, D3] 9" => {
                // cnt = 8 == n: a WHOLE-byte rotation leaves the value unchanged (r == 0), but C still comes
                // from the formula `(x >> ((cnt - 1) % n)) & 1` = `(x >> 7) & 1` (the MSB — NOT 0, only
                // cnt == 0 clears C; and NOT `x & 1`, which is ROL's left low bit).
                let rrr = (opcode & 7) as usize;
                let operand = case["initial"][format!("d{rrr}")].as_u64().unwrap() as u32;
                assert_eq!(
                    operand & 0xFF,
                    case["final"][format!("d{rrr}")].as_u64().unwrap() as u32 & 0xFF,
                    "cnt%n==0 (cnt!=0) must leave the rotated byte unchanged (r == 0)"
                );
                assert_eq!(
                    fin_sr & 0x01,
                    ((operand >> 7) & 1) as u16,
                    "cnt%n==0 (cnt!=0) C must come from the formula (= (x >> 7) & 1, the MSB), NOT be cleared \
                     and NOT x & 1"
                );
                assert_eq!(fin_sr & 0x02, 0, "ROR must always clear V");
            }
            "e81b [ROR.b Q, D3] 17" => {
                // A second X-untouched pin: incoming X = 1, an actual rotate (cnt = 4, cnt%n != 0), X stays 1.
                assert_ne!(ini_sr & 0x10, 0, "anchor must enter with X = 1");
                assert_ne!(fin_sr & 0x10, 0, "ROR must leave X untouched (1 → 1)");
                assert_eq!(fin_sr & 0x02, 0, "ROR must always clear V");
            }
            "e27a [ROR.w D1, D2] 460" => {
                // Zero-count (Dn count = 0): value unchanged, X PRESERVED (not set to C), V = 0, and C = 0 (a
                // zero count is the ONLY way ROR clears C). The case enters with X = 1, pinning preservation.
                assert_eq!(ini_sr & 0x10, fin_sr & 0x10, "zero-count must PRESERVE X");
                assert_ne!(
                    ini_sr & 0x10,
                    0,
                    "zero-count anchor must enter with X = 1 (pins preservation)"
                );
                assert_eq!(fin_sr & 0x01, 0, "zero-count must clear C");
                assert_eq!(fin_sr & 0x02, 0, "zero-count must clear V");
                assert_eq!(
                    case["initial"]["d2"], case["final"]["d2"],
                    "zero-count must leave the operand unchanged"
                );
            }
            "e6d7 [ROR.w (A7)] 19" => {
                assert_eq!((opcode >> 3) & 7, 2, "(A7) anchor must be mode 2");
                assert_eq!(opcode & 7, 7, "(A7) anchor must be reg 7 (the A7 indirect)");
            }
            "e6d5 [ROR.w (A5)] 15" => {
                // Odd-EA address error: the group-0 frame pushes the SSP down (the standard 14-byte frame).
                assert!(
                    case["final"]["ssp"].as_u64().unwrap()
                        < case["initial"]["ssp"].as_u64().unwrap(),
                    "odd-EA anchor must install the address-error frame (SSP pushed down)"
                );
            }
            _ => {}
        }
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all S5 ROR anchors exercised");
    eprintln!(
        "S5 ROR anchors: {found} cases (imm .b/.w/.l 6+2cnt / 8+2cnt, X-preserve pins (incoming X=1 stays 1), cnt%n==0 value-kept/C-from-formula (MSB), Dn cnt0 zero-count C=0/X-kept, (An)/(A7) memory rotate-by-1, odd-EA address-error) passed both drivers"
    );
}

/// S5 — the snapshot/restore anchor for the ROR.w memory rotate-by-1 (the shared `shift_recipe` word `ea_dst`
/// RMW: `[Read, Prefetch, Alu, Write]`). Drives a real vendored `ROR.w (A5)` case through the quiesce driver,
/// snapshotting + restoring the WHOLE `Cpu68000` (incl. the in-flight cursor) at every micro-op boundary —
/// including the mid-bus-access boundary between the operand Read and the result Write — and proves the
/// resumed run reproduces the run-to-completion final state + transaction stream bit-for-bit. This pins that
/// `AluOp::Ror` keeps `MicroState` fixed-size bincode (it stays `Copy`).
#[test]
fn ror_w_mem_quiescable_and_serializable_at_every_micro_op_boundary() {
    let path = format!("{VENDOR_DIR}/ROR.w.json");
    if !Path::new(&path).exists() {
        eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
        return;
    }
    let file = std::fs::File::open(&path).unwrap();
    let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
    let case = data
        .iter()
        .find(|t| t["name"].as_str().unwrap() == "e6d5 [ROR.w (A5)] 44")
        .expect("ROR.w (A5) snapshot anchor present");
    let ini = &case["initial"];

    // Run-to-completion reference.
    let mut rref = Cpu68000::new(build_regs(ini));
    let mut bref = build_bus(ini);
    rref.run_instruction(&mut bref);

    let cfg = bincode::config::standard();
    // 4 micro-ops (Read, Prefetch, Alu, Write) → in-flight boundaries after 0..=3 of them.
    for pause_after in 0..=3 {
        let mut cpu = Cpu68000::new(build_regs(ini));
        let mut bus = build_bus(ini);
        cpu.start_instruction();
        for _ in 0..pause_after {
            assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        }
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        loop {
            if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                break;
            }
        }
        assert_eq!(
            cpu2.regs, rref.regs,
            "resume from boundary {pause_after} diverged"
        );
        assert_eq!(
            bus.log, bref.log,
            "transaction stream from boundary {pause_after} diverged"
        );
    }
    eprintln!(
        "S5 ROR snapshot/restore: ROR.w (A5) word rotate-by-1 RMW resumed identically at every micro-op boundary"
    );
}

/// S6 — the named ROXL anchors, pinning rotate LEFT THROUGH X against the vendored ROXL.b/.w/.l stream WITHOUT
/// relying on the bulk `covered()` sweep. ROXL is the FIRST X-threading rotate — it treats `{X:operand}` as an
/// `(n+1)`-bit register (X above the msb) and rotates it left by `cnt % (n+1)`; the bit ejected into X is BOTH
/// the new X and C, so the result depends on the INCOMING X (unlike ROL/ROR, which leave X untouched, or
/// ASL/ASR/LSL/LSR, which set X = C from the value). It reuses S0's `shift_recipe`/`Operand::ShiftCount`/`dn_*`
/// VERBATIM (only the AluOp + the ROX/left decode arm differ). Each anchor is a real vendored case run through
/// both drivers + the per-cycle transaction stream via `run_case`; the load-bearing pins:
///
/// - `e314 [ROXL.b Q, D4] 59` (len 8) — REGISTER **immediate** `.b`, cnt 1, **incoming X = 0**: the X threads
///   into the new low bit → final byte `0x5a`. Timing `6 + 2*1`.
/// - `e314 [ROXL.b Q, D4] 3084` (len 8) — the SAME opcode/operand byte but **incoming X = 1**: the X-1 threads
///   into the new low bit → final byte `0x5b` (DIFFERENT from the X=0 case — the X-THREADING pin). C = X =
///   ejected bit.
/// - `e755 [ROXL.w Q, D5] 3` (len 12) — REGISTER immediate `.w`, cnt 3 → `6 + 2*3`.
/// - `e394 [ROXL.l Q, D4] 1` (len 10) — REGISTER immediate `.l`, cnt 1 → the `.l` base `8 + 2*1`.
/// - `e535 [ROXL.b D2, D5] 37` (len 6) — REGISTER **`Dn`-count `cnt == 0`** (the ZERO-COUNT case): value
///   UNCHANGED, **C = X (the INCOMING X — NOT 0), X UNCHANGED**, V = 0, timing `6`. The case enters with X = 1
///   so the final C must be 1 (= X) — the defining ROXL zero-count behaviour vs AS/LS/RO's C = 0.
/// - `e932 [ROXL.b D4, D2] 229` (len 24) — REGISTER `Dn`-count `cnt == 9` for `.b`: cnt WRAPS the `(n+1) = 9`
///   PERIOD (`eff = 9 % 9 = 0`) → the value returns to its start (byte UNCHANGED), timing `6 + 2*9`.
/// - `e5d5 [ROXL.w (A5)] 157` (len 12) — `.w` **memory** rotate-by-1 `(An)`: the word `ea_dst` RMW.
/// - `e5d7 [ROXL.w (A7)] 58` (len 12) — the plain `(A7)` mode-2 indirect (`mode == 2 && reg == 7`), COVERED
///   (NOT deferred), a clean word RMW at the active A7.
/// - `e5d5 [ROXL.w (A5)] 136` (len 50) — an **odd-EA** `.w` memory address error (the E3/E4 abort installs the
///   group-0 14-byte vector-3 frame), which must PASS unchanged.
///
/// Every anchor must decode as a ROXL opcode (`0xExxx`, type ROX / direction LEFT) — never any other family.
#[test]
fn roxl_anchors_match_singlesteptests() {
    let anchors: &[(&str, &str, u32)] = &[
        ("ROXL.b.json", "e314 [ROXL.b Q, D4] 59", 8), // imm .b cnt1, X=0 in → byte 0x5a, 6+2
        ("ROXL.b.json", "e314 [ROXL.b Q, D4] 3084", 8), // SAME op, X=1 in → byte 0x5b (X-threading)
        ("ROXL.w.json", "e755 [ROXL.w Q, D5] 3", 12), // imm .w cnt3 → 6+6
        ("ROXL.l.json", "e394 [ROXL.l Q, D4] 1", 10), // imm .l cnt1 → 8+2
        ("ROXL.b.json", "e535 [ROXL.b D2, D5] 37", 6), // Dn cnt0 zero-count: C=X(=1), X kept, value unchanged
        ("ROXL.b.json", "e932 [ROXL.b D4, D2] 229", 24), // Dn cnt9 wraps the (n+1)=9 period: value unchanged
        ("ROXL.w.json", "e5d5 [ROXL.w (A5)] 157", 12),   // memory (An) rotate-by-1
        ("ROXL.w.json", "e5d7 [ROXL.w (A7)] 58", 12),    // (A7) mode-2 indirect — COVERED
        ("ROXL.w.json", "e5d5 [ROXL.w (A5)] 136", 50),   // odd-EA memory address error
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("S6 ROXL anchor {name} (len {length}) not found in {fname}"));
        // Every anchor must be a ROXL opcode: 0xExxx, type ROX (register bits 4-3 == 2 / memory bits 10-9 ==
        // 2), direction LEFT (bit 8 == 1).
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode >> 12,
            0xE,
            "anchor {name} must be a 0xExxx shift opcode"
        );
        let is_roxl = if (opcode >> 6) & 3 == 3 {
            (opcode >> 8) & 1 == 1 && (opcode >> 9) & 3 == 2
        } else {
            (opcode >> 8) & 1 == 1 && (opcode >> 3) & 3 == 2
        };
        assert!(
            is_roxl,
            "anchor {name} must be ROXL (type ROX, direction LEFT)"
        );
        // Load-bearing flag/scope pins (run_case verifies the FULL final state against the data either way).
        let ini_sr = case["initial"]["sr"].as_u64().unwrap() as u16;
        let fin_sr = case["final"]["sr"].as_u64().unwrap() as u16;
        match *name {
            "e314 [ROXL.b Q, D4] 59" => {
                // X = 0 incoming: the threaded bit is 0 → final low bit is 0; final byte 0x5a. V = 0.
                assert_eq!(ini_sr & 0x10, 0, "the X=0 anchor must enter with X = 0");
                assert_eq!(
                    case["final"]["d4"].as_u64().unwrap() as u32 & 0xFF,
                    0x5a,
                    "X=0 ROXL.b #1 must thread a 0 into the low bit → 0x5a"
                );
                assert_eq!(fin_sr & 0x02, 0, "ROXL must always clear V");
            }
            "e314 [ROXL.b Q, D4] 3084" => {
                // SAME opcode/operand byte but X = 1 incoming → the threaded bit is 1 → final low bit is 1;
                // final byte 0x5b — DIFFERENT from the X=0 case. This is the X-THREADING pin (the result
                // depends on the incoming X). C = X = the ejected bit (here the operand's old msb).
                assert_ne!(ini_sr & 0x10, 0, "the X=1 anchor must enter with X = 1");
                assert_eq!(
                    case["final"]["d4"].as_u64().unwrap() as u32 & 0xFF,
                    0x5b,
                    "X=1 ROXL.b #1 must thread a 1 into the low bit → 0x5b (DIFFERENT from the X=0 0x5a)"
                );
                assert_eq!(fin_sr & 0x02, 0, "ROXL must always clear V");
                // C = X (the two are always equal for ROXL).
                assert_eq!(
                    (fin_sr >> 4) & 1,
                    fin_sr & 1,
                    "ROXL must set X = C (the bit ejected into X)"
                );
            }
            "e535 [ROXL.b D2, D5] 37" => {
                // Zero-count (Dn count = 0): value UNCHANGED, V = 0, X UNCHANGED, and C = X (the INCOMING X —
                // NOT 0; this is ROXL's defining difference from AS/LS/RO, which clear C on a zero count). The
                // case enters with X = 1, so the final C must be 1.
                assert_ne!(
                    ini_sr & 0x10,
                    0,
                    "zero-count anchor must enter with X = 1 (pins C = X, not 0)"
                );
                assert_eq!(
                    ini_sr & 0x10,
                    fin_sr & 0x10,
                    "zero-count must leave X UNCHANGED"
                );
                assert_ne!(
                    fin_sr & 1,
                    0,
                    "zero-count must set C = X (= 1 here) — NOT clear C like AS/LS/RO"
                );
                assert_eq!(fin_sr & 0x02, 0, "zero-count must clear V");
                assert_eq!(
                    case["initial"]["d5"], case["final"]["d5"],
                    "zero-count must leave the operand unchanged"
                );
            }
            "e932 [ROXL.b D4, D2] 229" => {
                // cnt = 9 for .b WRAPS the (n+1) = 9 period (eff = 9 % 9 = 0) → the value returns to its start
                // (the rotated byte is UNCHANGED). V = 0.
                assert_eq!(
                    case["initial"]["d2"].as_u64().unwrap() as u32 & 0xFF,
                    case["final"]["d2"].as_u64().unwrap() as u32 & 0xFF,
                    "cnt = 9 (= n+1 for .b) must wrap the period → the rotated byte is unchanged"
                );
                assert_eq!(fin_sr & 0x02, 0, "ROXL must always clear V");
            }
            "e5d7 [ROXL.w (A7)] 58" => {
                assert_eq!((opcode >> 3) & 7, 2, "(A7) anchor must be mode 2");
                assert_eq!(opcode & 7, 7, "(A7) anchor must be reg 7 (the A7 indirect)");
            }
            "e5d5 [ROXL.w (A5)] 136" => {
                // Odd-EA address error: the group-0 frame pushes the SSP down (the standard 14-byte frame).
                assert!(
                    case["final"]["ssp"].as_u64().unwrap()
                        < case["initial"]["ssp"].as_u64().unwrap(),
                    "odd-EA anchor must install the address-error frame (SSP pushed down)"
                );
            }
            _ => {}
        }
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all S6 ROXL anchors exercised");
    eprintln!(
        "S6 ROXL anchors: {found} cases (imm .b/.w/.l 6+2cnt / 8+2cnt, X-threading pins (X=0 → 0x5a vs X=1 → 0x5b, same opcode/operand), Dn cnt0 zero-count C=X/X-kept/value-kept, cnt9 (n+1)-period wrap value-kept, (An)/(A7) memory rotate-by-1, odd-EA address-error) passed both drivers"
    );
}

/// S6 — the snapshot/restore anchor for the ROXL.w memory rotate-by-1 (the shared `shift_recipe` word `ea_dst`
/// RMW: `[Read, Prefetch, Alu, Write]`). Drives a real vendored `ROXL.w (A5)` case through the quiesce driver,
/// snapshotting + restoring the WHOLE `Cpu68000` (incl. the in-flight cursor) at every micro-op boundary —
/// including the mid-bus-access boundary between the operand Read and the result Write — and proves the
/// resumed run reproduces the run-to-completion final state + transaction stream bit-for-bit. This pins that
/// `AluOp::Roxl` keeps `MicroState` fixed-size bincode (it stays `Copy`).
#[test]
fn roxl_w_mem_quiescable_and_serializable_at_every_micro_op_boundary() {
    let path = format!("{VENDOR_DIR}/ROXL.w.json");
    if !Path::new(&path).exists() {
        eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
        return;
    }
    let file = std::fs::File::open(&path).unwrap();
    let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
    let case = data
        .iter()
        .find(|t| t["name"].as_str().unwrap() == "e5d5 [ROXL.w (A5)] 157")
        .expect("ROXL.w (A5) snapshot anchor present");
    let ini = &case["initial"];

    // Run-to-completion reference.
    let mut rref = Cpu68000::new(build_regs(ini));
    let mut bref = build_bus(ini);
    rref.run_instruction(&mut bref);

    let cfg = bincode::config::standard();
    // 4 micro-ops (Read, Prefetch, Alu, Write) → in-flight boundaries after 0..=3 of them.
    for pause_after in 0..=3 {
        let mut cpu = Cpu68000::new(build_regs(ini));
        let mut bus = build_bus(ini);
        cpu.start_instruction();
        for _ in 0..pause_after {
            assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        }
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        loop {
            if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                break;
            }
        }
        assert_eq!(
            cpu2.regs, rref.regs,
            "resume from boundary {pause_after} diverged"
        );
        assert_eq!(
            bus.log, bref.log,
            "transaction stream from boundary {pause_after} diverged"
        );
    }
    eprintln!(
        "S6 ROXL snapshot/restore: ROXL.w (A5) word rotate-by-1 RMW resumed identically at every micro-op boundary"
    );
}

/// S7 (FINAL) — the named ROXR anchors, pinning rotate RIGHT THROUGH X against the vendored ROXR.b/.w/.l stream
/// WITHOUT relying on the bulk `covered()` sweep. ROXR is ROXL's right-direction twin — it treats `{X:operand}`
/// as an `(n+1)`-bit register (X above the msb) and rotates it RIGHT by `cnt % (n+1)`; the bit ejected into X is
/// BOTH the new X and C, so the result depends on the INCOMING X (unlike ROL/ROR, which leave X untouched, or
/// ASL/ASR/LSL/LSR, which set X = C from the value). It reuses S0's `shift_recipe`/`Operand::ShiftCount`/`dn_*`
/// VERBATIM (only the AluOp + the ROX/right decode arm differ). Each anchor is a real vendored case run through
/// both drivers + the per-cycle transaction stream via `run_case`; the load-bearing pins:
///
/// - `e216 [ROXR.b Q, D6] 137` (len 8) — REGISTER **immediate** `.b`, cnt 1, **incoming X = 0**: the operand
///   `0x40 >> 1 = 0x20`; the X = 0 threads into the new msb → final byte `0x20`. Timing `6 + 2*1`.
/// - `e216 [ROXR.b Q, D6] 7050` (len 8) — the SAME opcode/operand byte but **incoming X = 1**: the X = 1 threads
///   into the new msb → final byte `0xa0` = `0x20 | 0x80` (DIFFERENT from the X=0 case — the X-THREADING pin).
///   C = X = ejected bit.
/// - `e650 [ROXR.w Q, D0] 1` (len 12) — REGISTER immediate `.w`, cnt 3 → `6 + 2*3`.
/// - `e294 [ROXR.l Q, D4] 7` (len 10) — REGISTER immediate `.l`, cnt 1 → the `.l` base `8 + 2*1`.
/// - `e830 [ROXR.b D4, D0] 410` (len 6) — REGISTER **`Dn`-count `cnt == 0`** (the ZERO-COUNT case): value
///   UNCHANGED, **C = X (the INCOMING X — NOT 0), X UNCHANGED**, V = 0, timing `6`. The case enters with X = 1
///   so the final C must be 1 (= X) — the defining ROXR zero-count behaviour vs AS/LS/RO's C = 0.
/// - `ee33 [ROXR.b D7, D3] 150` (len 24) — REGISTER `Dn`-count `cnt == 9` for `.b`: cnt WRAPS the `(n+1) = 9`
///   PERIOD (`eff = 9 % 9 = 0`) → the value returns to its start (byte UNCHANGED), timing `6 + 2*9`.
/// - `e4d3 [ROXR.w (A3)] 120` (len 12) — `.w` **memory** rotate-by-1 `(An)`: the word `ea_dst` RMW.
/// - `e4d7 [ROXR.w (A7)] 343` (len 12) — the plain `(A7)` mode-2 indirect (`mode == 2 && reg == 7`), COVERED
///   (NOT deferred), a clean word RMW at the active A7.
/// - `e4d6 [ROXR.w (A6)] 2` (len 50) — an **odd-EA** `.w` memory address error (the E3/E4 abort installs the
///   group-0 14-byte vector-3 frame), which must PASS unchanged.
///
/// Every anchor must decode as a ROXR opcode (`0xExxx`, type ROX / direction RIGHT) — never any other family.
#[test]
fn roxr_anchors_match_singlesteptests() {
    let anchors: &[(&str, &str, u32)] = &[
        ("ROXR.b.json", "e216 [ROXR.b Q, D6] 137", 8), // imm .b cnt1, X=0 in → byte 0x20, 6+2
        ("ROXR.b.json", "e216 [ROXR.b Q, D6] 7050", 8), // SAME op, X=1 in → byte 0xa0 (X-threading)
        ("ROXR.w.json", "e650 [ROXR.w Q, D0] 1", 12),  // imm .w cnt3 → 6+6
        ("ROXR.l.json", "e294 [ROXR.l Q, D4] 7", 10),  // imm .l cnt1 → 8+2
        ("ROXR.b.json", "e830 [ROXR.b D4, D0] 410", 6), // Dn cnt0 zero-count: C=X(=1), X kept, value unchanged
        ("ROXR.b.json", "ee33 [ROXR.b D7, D3] 150", 24), // Dn cnt9 wraps the (n+1)=9 period: value unchanged
        ("ROXR.w.json", "e4d3 [ROXR.w (A3)] 120", 12),   // memory (An) rotate-by-1
        ("ROXR.w.json", "e4d7 [ROXR.w (A7)] 343", 12),   // (A7) mode-2 indirect — COVERED
        ("ROXR.w.json", "e4d6 [ROXR.w (A6)] 2", 50),     // odd-EA memory address error
    ];
    let mut found = 0usize;
    for (fname, name, length) in anchors {
        let path = format!("{VENDOR_DIR}/{fname}");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
            return;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        let case = data
            .iter()
            .find(|t| {
                t["name"].as_str().unwrap() == *name
                    && t["length"].as_u64().unwrap() as u32 == *length
            })
            .unwrap_or_else(|| panic!("S7 ROXR anchor {name} (len {length}) not found in {fname}"));
        // Every anchor must be a ROXR opcode: 0xExxx, type ROX (register bits 4-3 == 2 / memory bits 10-9 ==
        // 2), direction RIGHT (bit 8 == 0).
        let opcode = case["initial"]["prefetch"][0].as_u64().unwrap() as u16;
        assert_eq!(
            opcode >> 12,
            0xE,
            "anchor {name} must be a 0xExxx shift opcode"
        );
        let is_roxr = if (opcode >> 6) & 3 == 3 {
            (opcode >> 8) & 1 == 0 && (opcode >> 9) & 3 == 2
        } else {
            (opcode >> 8) & 1 == 0 && (opcode >> 3) & 3 == 2
        };
        assert!(
            is_roxr,
            "anchor {name} must be ROXR (type ROX, direction RIGHT)"
        );
        // Load-bearing flag/scope pins (run_case verifies the FULL final state against the data either way).
        let ini_sr = case["initial"]["sr"].as_u64().unwrap() as u16;
        let fin_sr = case["final"]["sr"].as_u64().unwrap() as u16;
        match *name {
            "e216 [ROXR.b Q, D6] 137" => {
                // X = 0 incoming: the threaded bit is 0 → the new msb is 0; operand 0x40 >> 1 → final byte
                // 0x20. V = 0.
                assert_eq!(ini_sr & 0x10, 0, "the X=0 anchor must enter with X = 0");
                assert_eq!(
                    case["final"]["d6"].as_u64().unwrap() as u32 & 0xFF,
                    0x20,
                    "X=0 ROXR.b #1 of 0x40 must thread a 0 into the msb → 0x20"
                );
                assert_eq!(fin_sr & 0x02, 0, "ROXR must always clear V");
            }
            "e216 [ROXR.b Q, D6] 7050" => {
                // SAME opcode/operand byte but X = 1 incoming → the threaded bit is 1 → the new msb is 1;
                // final byte 0xa0 = 0x20 | 0x80 — DIFFERENT from the X=0 case. This is the X-THREADING pin (the
                // result depends on the incoming X). C = X = the ejected bit (here the operand's old lsb).
                assert_ne!(ini_sr & 0x10, 0, "the X=1 anchor must enter with X = 1");
                assert_eq!(
                    case["final"]["d6"].as_u64().unwrap() as u32 & 0xFF,
                    0xa0,
                    "X=1 ROXR.b #1 of 0x40 must thread a 1 into the msb → 0xa0 (DIFFERENT from the X=0 0x20)"
                );
                assert_eq!(fin_sr & 0x02, 0, "ROXR must always clear V");
                // C = X (the two are always equal for ROXR).
                assert_eq!(
                    (fin_sr >> 4) & 1,
                    fin_sr & 1,
                    "ROXR must set X = C (the bit ejected into X)"
                );
            }
            "e830 [ROXR.b D4, D0] 410" => {
                // Zero-count (Dn count = 0): value UNCHANGED, V = 0, X UNCHANGED, and C = X (the INCOMING X —
                // NOT 0; this is ROXR's defining difference from AS/LS/RO, which clear C on a zero count). The
                // case enters with X = 1, so the final C must be 1.
                assert_ne!(
                    ini_sr & 0x10,
                    0,
                    "zero-count anchor must enter with X = 1 (pins C = X, not 0)"
                );
                assert_eq!(
                    ini_sr & 0x10,
                    fin_sr & 0x10,
                    "zero-count must leave X UNCHANGED"
                );
                assert_ne!(
                    fin_sr & 1,
                    0,
                    "zero-count must set C = X (= 1 here) — NOT clear C like AS/LS/RO"
                );
                assert_eq!(fin_sr & 0x02, 0, "zero-count must clear V");
                assert_eq!(
                    case["initial"]["d0"], case["final"]["d0"],
                    "zero-count must leave the operand unchanged"
                );
            }
            "ee33 [ROXR.b D7, D3] 150" => {
                // cnt = 9 for .b WRAPS the (n+1) = 9 period (eff = 9 % 9 = 0) → the value returns to its start
                // (the rotated byte is UNCHANGED). V = 0.
                assert_eq!(
                    case["initial"]["d3"].as_u64().unwrap() as u32 & 0xFF,
                    case["final"]["d3"].as_u64().unwrap() as u32 & 0xFF,
                    "cnt = 9 (= n+1 for .b) must wrap the period → the rotated byte is unchanged"
                );
                assert_eq!(fin_sr & 0x02, 0, "ROXR must always clear V");
            }
            "e4d7 [ROXR.w (A7)] 343" => {
                assert_eq!((opcode >> 3) & 7, 2, "(A7) anchor must be mode 2");
                assert_eq!(opcode & 7, 7, "(A7) anchor must be reg 7 (the A7 indirect)");
            }
            "e4d6 [ROXR.w (A6)] 2" => {
                // Odd-EA address error: the group-0 frame pushes the SSP down (the standard 14-byte frame).
                assert!(
                    case["final"]["ssp"].as_u64().unwrap()
                        < case["initial"]["ssp"].as_u64().unwrap(),
                    "odd-EA anchor must install the address-error frame (SSP pushed down)"
                );
            }
            _ => {}
        }
        run_case(case);
        found += 1;
    }
    assert_eq!(found, anchors.len(), "all S7 ROXR anchors exercised");
    eprintln!(
        "S7 ROXR anchors: {found} cases (imm .b/.w/.l 6+2cnt / 8+2cnt, X-threading pins (X=0 → 0x20 vs X=1 → 0xa0, same opcode/operand), Dn cnt0 zero-count C=X/X-kept/value-kept, cnt9 (n+1)-period wrap value-kept, (An)/(A7) memory rotate-by-1, odd-EA address-error) passed both drivers"
    );
}

/// S7 (FINAL) — the snapshot/restore anchor for the ROXR.w memory rotate-by-1 (the shared `shift_recipe` word
/// `ea_dst` RMW: `[Read, Prefetch, Alu, Write]`). Drives a real vendored `ROXR.w (A3)` case through the quiesce
/// driver, snapshotting + restoring the WHOLE `Cpu68000` (incl. the in-flight cursor) at every micro-op boundary
/// — including the mid-bus-access boundary between the operand Read and the result Write — and proves the resumed
/// run reproduces the run-to-completion final state + transaction stream bit-for-bit. This pins that
/// `AluOp::Roxr` keeps `MicroState` fixed-size bincode (it stays `Copy`).
#[test]
fn roxr_w_mem_quiescable_and_serializable_at_every_micro_op_boundary() {
    let path = format!("{VENDOR_DIR}/ROXR.w.json");
    if !Path::new(&path).exists() {
        eprintln!("SKIP: {path} missing — run tools/fetch-tests.sh");
        return;
    }
    let file = std::fs::File::open(&path).unwrap();
    let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
    let case = data
        .iter()
        .find(|t| t["name"].as_str().unwrap() == "e4d3 [ROXR.w (A3)] 120")
        .expect("ROXR.w (A3) snapshot anchor present");
    let ini = &case["initial"];

    // Run-to-completion reference.
    let mut rref = Cpu68000::new(build_regs(ini));
    let mut bref = build_bus(ini);
    rref.run_instruction(&mut bref);

    let cfg = bincode::config::standard();
    // 4 micro-ops (Read, Prefetch, Alu, Write) → in-flight boundaries after 0..=3 of them.
    for pause_after in 0..=3 {
        let mut cpu = Cpu68000::new(build_regs(ini));
        let mut bus = build_bus(ini);
        cpu.start_instruction();
        for _ in 0..pause_after {
            assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        }
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut cpu2, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        loop {
            if let Step::Done(_) = cpu2.step_micro_op(&mut bus) {
                break;
            }
        }
        assert_eq!(
            cpu2.regs, rref.regs,
            "resume from boundary {pause_after} diverged"
        );
        assert_eq!(
            bus.log, bref.log,
            "transaction stream from boundary {pause_after} diverged"
        );
    }
    eprintln!(
        "S7 ROXR snapshot/restore: ROXR.w (A3) word rotate-by-1 RMW resumed identically at every micro-op boundary"
    );
}
