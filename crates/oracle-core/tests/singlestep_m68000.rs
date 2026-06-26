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
            _ => continue, // 'n' idle cycles etc. — not memory transactions
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

/// Whether the framework covers this case (else it is an xfail for this push). `ADD`/`SUB` in word, byte and
/// long sizes, each in two forms — `Dn,<ea>` (memory dest; word ADD=0xD140/SUB=0x9140, byte ADD=0xD100/
/// SUB=0x9100, long ADD=0xD180/SUB=0x9180) and `<ea>,Dn` (register dest; word ADD=0xD040/SUB=0x9040, byte
/// ADD=0xD000/SUB=0x9000, long ADD=0xD080/SUB=0x9080). **No parity filter** — E4 made odd word/long EAs
/// coverable (the execution-time address-error abort installs the group-0 14-byte vector-3 frame, so an odd
/// access PASSES unchanged; the auto-(in/de)crement register bump is committed before the faulting read,
/// pinned to the data). The only remaining deferrals are mode-scope: `An`-direct as a byte source/dest
/// (`ADD.b An,Dn` is illegal), the older `(An)` (mode 2) `A7` form (a pre-existing non-address-error
/// mode-scope convention — its `(A7)+`/`-(A7)` siblings ARE in scope), and not-yet-implemented EA modes.
fn covered(opcode: u16, _ini: &Value) -> bool {
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
            if !covered(opcode, ini) {
                continue;
            }
            run_case(t);
            file_ran += 1;
        }
        eprintln!("  {fname}: {file_ran} covered cases passed");
        ran += file_ran;
    }

    assert!(
        ran >= 277_650,
        "expected 277650 covered cases — N5 adds CLR `<ea>` (its own CLR.b/.w/.l files, 0x4200/4240/4280, SS \
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
    eprintln!("SingleStepTests ADD+SUB+MOVE+MOVEA+Bcc+BSR+JMP+JSR+RTS+DBcc+RTR+TRAP+RTE+TRAPV+CHK+ANDItoSR+ORItoSR+EORItoSR+RESET+CMP+CMPA+TST+CLR (.w + .b + .l): {ran} covered cases passed (both framework drivers, regs/SR/RAM/prefetch/cycles/transactions)");
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
