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
//! Versioned xfail manifest (slice scope — implemented later): odd-address *word* accesses (which raise an
//! address-error exception — byte accesses have no such error, so odd byte EAs are in scope), a taken `Bcc`
//! to an *odd target* (the same address-error class), the `A7` form of the older `(An)` (mode 2) memory
//! access, `An`-direct as a byte source (`ADD.b An,Dn` is illegal), and the remaining EA modes / sizes are
//! skipped (see [`covered`]). The auto-(in/de)crement `(A7)+`/`-(A7)` forms are in scope for both sizes (word
//! steps 2; byte steps 2 for A7 to keep the SP even). If the vendor data is missing, the test skips cleanly
//! (run `tools/fetch-tests.sh`).

use oracle_core::m68000::bus68k::{FlatBus, Transaction, TxKind};
use oracle_core::m68000::ea::compute_ea;
use oracle_core::m68000::microop::{condition_true, Cpu68000, Size, Step};
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

/// Read the address register `reg` from `ini` exactly as the decoder does: A7 is `ssp` (supervisor) /
/// `usp` (user); A0–A6 are `a{reg}`.
fn move_areg(ini: &Value, reg: usize) -> u32 {
    if reg == 7 {
        if (u32f(ini, "sr") & 0x2000) != 0 {
            u32f(ini, "ssp")
        } else {
            u32f(ini, "usp")
        }
    } else {
        u32f(ini, &format!("a{reg}"))
    }
}

/// Read the byte at `addr` from the `initial.ram` array (0 if absent).
fn move_ram(ini: &Value, addr: u32) -> u8 {
    ini["ram"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p.as_array().unwrap()[0].as_u64().unwrap() as u32 == addr)
        .map(|p| p.as_array().unwrap()[1].as_u64().unwrap() as u8)
        .unwrap_or(0)
}

/// Read the big-endian word at `addr` from `initial.ram`.
fn move_ramw(ini: &Value, addr: u32) -> u32 {
    ((move_ram(ini, addr) as u32) << 8) | move_ram(ini, addr.wrapping_add(1)) as u32
}

/// Read extension word `k` (0-based, after the opcode) of a MOVE: word 0 is `prefetch[1]` (already in the
/// queue), word `k > 0` lives in RAM at `pc + 2 + 2k`. The DESTINATION's extension word, when the source
/// also has extension words, is **not** `prefetch[1]` — it lives later in the stream — so MOVE's dest
/// parity must read it from RAM (the runner's shared `compute_ea` cannot, hence this MOVE-local helper).
fn move_ext_word(ini: &Value, k: u32) -> u32 {
    if k == 0 {
        let pf = ini["prefetch"].as_array().unwrap();
        pf[1].as_u64().unwrap() as u32
    } else {
        move_ramw(ini, u32f(ini, "pc") + 2 + 2 * k)
    }
}

/// The number of extension words a MOVE source EA consumes (so the dest's extension words start after them).
/// `long` is true for `MOVE.l`, where `#imm.l` (7/4) is a **two-word** immediate (`#imm.b`/`#imm.w` are one).
fn move_src_ext(sm: u16, sr: u16, long: bool) -> u32 {
    match (sm, sr) {
        (5, _) | (6, _) => 1,
        (7, 0) | (7, 2) | (7, 3) => 1,
        (7, 4) => {
            if long {
                2
            } else {
                1
            }
        }
        (7, 1) => 2,
        _ => 0,
    }
}

/// The sign-extended 16-bit displacement.
fn move_sxt16(v: u32) -> u32 {
    v as u16 as i16 as i32 as u32
}

/// The brief-index value of a `d8(An,Xn)`/`d8(PC,Xn)` extension word `ext` — D/A reg file (A7-aware),
/// W/L size with sign-extension — identical to the framework's `Operand::BriefIndex` resolver.
fn move_brief_index(ini: &Value, ext: u32) -> u32 {
    let ireg = ((ext >> 12) & 7) as usize;
    let raw = if ext & 0x8000 != 0 {
        move_areg(ini, ireg)
    } else {
        u32f(ini, &format!("d{ireg}"))
    };
    if ext & 0x0800 != 0 {
        raw
    } else {
        move_sxt16(raw & 0xFFFF)
    }
}

/// The brief 8-bit displacement (sign-extended) of `ext`.
fn move_brief_disp8(ext: u32) -> u32 {
    (ext & 0xFF) as u8 as i8 as i32 as u32
}

/// The MOVE source effective address (its accessed address), for the parity filter. `e0` is the source's
/// first extension word (`prefetch[1]`). `predec` is the `-(An)` step (2 for word, 4 for long — parity is
/// preserved either way, but the step keeps the test honest). Returns `None` for non-memory sources
/// (register / immediate).
fn move_src_ea(ini: &Value, sm: u16, sr: u16, predec: u32) -> Option<u32> {
    let pc = u32f(ini, "pc");
    let e0 = move_ext_word(ini, 0);
    let ea = match (sm, sr) {
        (2, _) | (3, _) => move_areg(ini, sr as usize),
        (4, _) => move_areg(ini, sr as usize).wrapping_sub(predec),
        (5, _) => move_areg(ini, sr as usize).wrapping_add(move_sxt16(e0)),
        (6, _) => move_areg(ini, sr as usize)
            .wrapping_add(move_brief_index(ini, e0))
            .wrapping_add(move_brief_disp8(e0)),
        (7, 0) => move_sxt16(e0),
        (7, 1) => (e0 << 16) | move_ext_word(ini, 1),
        (7, 2) => pc.wrapping_add(2).wrapping_add(move_sxt16(e0)),
        (7, 3) => pc
            .wrapping_add(2)
            .wrapping_add(move_brief_index(ini, e0))
            .wrapping_add(move_brief_disp8(e0)),
        _ => return None, // Dn/An/#imm — no memory access
    };
    Some(ea & 0x00FF_FFFF)
}

/// The MOVE destination effective address. The dest's first extension word starts after the source's
/// extension words (`src_ext`), so it is read from RAM, not `prefetch[1]` (unless the source took none).
/// Returns `None` for `Dn` (no memory write).
fn move_dst_ea(ini: &Value, dm: u16, dr: u16, src_ext: u32, predec: u32) -> Option<u32> {
    let e0 = move_ext_word(ini, src_ext);
    let ea = match (dm, dr) {
        (2, _) | (3, _) => move_areg(ini, dr as usize),
        (4, _) => move_areg(ini, dr as usize).wrapping_sub(predec),
        (5, _) => move_areg(ini, dr as usize).wrapping_add(move_sxt16(e0)),
        (6, _) => move_areg(ini, dr as usize)
            .wrapping_add(move_brief_index(ini, e0))
            .wrapping_add(move_brief_disp8(e0)),
        (7, 0) => move_sxt16(e0),
        (7, 1) => (e0 << 16) | move_ext_word(ini, src_ext + 1),
        _ => return None, // Dn dest — no memory write
    };
    Some(ea & 0x00FF_FFFF)
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

/// The MOVE-specific parity/scope filter (called once `move_covered` matches), given the case's `ini`.
fn move_in_scope(opcode: u16, ini: &Value) -> bool {
    let size = move_size(opcode).expect("move_covered gates move_size");
    let byte = size == Size::Byte;
    let dst_reg = (opcode >> 9) & 7;
    let dst_mode = (opcode >> 6) & 7;
    let src_mode = (opcode >> 3) & 7;
    let src_reg = opcode & 7;
    // Supported source modes: Dn (0, always legal) + (for word only) An-direct (1, illegal `MOVE.b An,<ea>`)
    // + (An)/(An)+/-(An)/d16(An)/d8(An,Xn) (2..=6) + abs.w/abs.l/d16(PC)/d8(PC,Xn)/#imm (7/0..=4).
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
    // (A7) mode-2 form stays xfail (prior convention) — source and destination, both sizes.
    if src_mode == 2 && src_reg == 7 {
        return false;
    }
    if dst_mode == 2 && dst_reg == 7 {
        return false;
    }
    // Byte accesses have NO odd-address error (byte EAs may be odd), so there is no word-parity filter — the
    // byte case is in scope once the mode/reg gates pass.
    if byte {
        return true;
    }
    // Word and long share the parity filter (a word/long memory access to an odd EA is an address error →
    // xfail). The only size-dependent piece is the `-(An)` step (word 2, long 4 — parity is preserved either
    // way), and that `#imm.l` consumes two extension words (shifting the dest's ext word two words later).
    let long = size == Size::Long;
    let predec = if long { 4 } else { 2 };
    // Source parity: an odd memory access is an address error → xfail.
    if let Some(ea) = move_src_ea(ini, src_mode, src_reg, predec) {
        if ea & 1 != 0 {
            return false;
        }
    }
    // Destination parity: the dest ext word starts after the source's ext words.
    let src_ext = move_src_ext(src_mode, src_reg, long);
    if let Some(ea) = move_dst_ea(ini, dst_mode, dst_reg, src_ext, predec) {
        if ea & 1 != 0 {
            return false;
        }
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

/// Whether the framework covers this `MOVEA.w`/`MOVEA.l` case (called once `movea_size` matches), given the
/// case's `ini`. Source = all 12 EA modes (`An`-direct is a legal MOVEA source); destination is always `An`
/// (a register write — no memory access, so no destination parity). A word/long memory **source** access to
/// an odd EA is an address error → xfail; the `(A7)` (mode-2) source form stays xfail (the prior MOVE
/// convention). There is no flag effect and no destination memory write to filter.
fn movea_in_scope(opcode: u16, ini: &Value) -> bool {
    let size = movea_size(opcode).expect("movea_covered gates movea_size");
    let long = size == Size::Long;
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
    // (A7) mode-2 source stays xfail (prior convention), both sizes.
    if src_mode == 2 && src_reg == 7 {
        return false;
    }
    // Source parity: a word/long memory access to an odd EA is an address error → xfail. The `-(An)` step is
    // 4 for long, 2 for word (parity is preserved either way), and `#imm.l` consumes two extension words.
    let predec = if long { 4 } else { 2 };
    if let Some(ea) = move_src_ea(ini, src_mode, src_reg, predec) {
        if ea & 1 != 0 {
            return false;
        }
    }
    true
}

/// Whether this opcode is a `Bcc`/`BRA` the framework covers (`0110 cccc dddddddd`, 0x6xxx; cc != 1 — cc ==
/// 1 is `BSR`, a later commit, and is excluded). `Bcc.json` carries `BRA` (0x60xx) + cc 2..=15; the cc == 1
/// `BSR` cases live in `BSR.json` (not this file).
fn bcc_covered(opcode: u16) -> bool {
    opcode >> 12 == 0b0110 && (opcode >> 8) & 0xF != 1
}

/// The `Bcc`/`BRA` scope/parity filter (called once `bcc_covered` matches). Clean iff the branch is NOT taken
/// (a fall-through is always clean) OR the taken target is **even**. A taken odd target raises an address
/// error (the deferred odd-address class) → xfail. The condition is resolved exactly as the decoder does
/// (`condition_true` against the live CCR); the target is `pc + 2 + sign_extend(disp)` (the displacement is
/// relative to the extension-word address `pc + 2`), where `disp` is the opcode's low byte (byte form,
/// `disp8 != 0`) or the extension word `prefetch[1]` sign-extended (word form, `disp8 == 0`).
fn bcc_in_scope(opcode: u16, ini: &Value) -> bool {
    let cc = (opcode >> 8) as u8 & 0xF;
    let sr = u32f(ini, "sr") as u16;
    if !condition_true(cc, sr) {
        return true; // not taken — the sequential fall-through is always clean
    }
    let pc = u32f(ini, "pc");
    let disp8 = opcode & 0xFF;
    let disp = if disp8 == 0 {
        // word form: the 16-bit displacement is the extension word, sign-extended.
        let ext = ini["prefetch"].as_array().unwrap()[1].as_u64().unwrap() as u16;
        ext as i16 as i32 as u32
    } else {
        // byte form: the opcode's low byte, sign-extended.
        disp8 as u8 as i8 as i32 as u32
    };
    let target = pc.wrapping_add(2).wrapping_add(disp);
    target & 1 == 0 // even target is in scope; odd target is an address error → xfail
}

/// Whether this opcode is a `DBcc` the framework covers (`0101 cccc 11001 rrr`, opcode & 0xF0F8 == 0x50C8 —
/// the `An`-direct (mode 001) special case of the `Scc` opcode space; only this exact form is DBcc, every
/// other mode is `Scc`, which is NOT implemented). `DBcc.json` carries the full DBcc family.
fn dbcc_covered(opcode: u16) -> bool {
    opcode & 0xF0F8 == 0x50C8
}

/// The `DBcc` scope/parity filter (called once `dbcc_covered` matches). `cc` is a *termination* condition:
/// cond **true** → the loop terminates, fall through (NO branch) → always clean. cond **false** → decrement
/// `Dn.w` and, if the counter is still live (`Dn.w != 0`), take the branch — clean iff the (always word-form)
/// target is **even**; an odd taken target raises an address error (the deferred odd-address class — those
/// cases are length 52 in the data, vs. 10 for the clean taken branch) → xfail. If the counter is EXPIRED
/// (`Dn.w == 0`, so the decrement yields −1) the branch is NOT taken (fall-through) → clean; this expired
/// bucket is **absent from the vendored data** (a random 32-bit `Dn` has `Dn.w == 0` only ≈1/65536 of the
/// time, and `DBcc.json` has only lengths 10/12/52 — no expired cases), so the filter handles it for
/// completeness but it never selects a case here. The target is `pc + 2 + sign_extend16(prefetch[1])`.
fn dbcc_in_scope(opcode: u16, ini: &Value) -> bool {
    let cc = (opcode >> 8) as u8 & 0xF;
    let sr = u32f(ini, "sr") as u16;
    if condition_true(cc, sr) {
        return true; // cond true → loop terminates (fall-through), always clean
    }
    // cond false → decrement; an expired counter (Dn.w == 0) falls through (clean; absent from the data).
    let reg = (opcode & 7) as usize;
    if u32f(ini, &format!("d{reg}")) & 0xFFFF == 0 {
        return true;
    }
    // Counter live → branch taken: clean iff the target is even (odd = address error, 52 cyc → xfail).
    let pc = u32f(ini, "pc");
    let ext = ini["prefetch"].as_array().unwrap()[1].as_u64().unwrap() as u16;
    let disp = ext as i16 as i32 as u32;
    let target = pc.wrapping_add(2).wrapping_add(disp);
    target & 1 == 0
}

/// Whether this opcode is a `BSR` the framework covers (`0110 0001 dddddddd`, 0x61xx; cc == 1 — the BSR
/// encoding, decoded as its own arm). `BSR.json` carries `BSR.b`/`BSR.w` (`disp8 != 0xFF`) plus the 35 cases
/// of the 68020 long-displacement form `0x61FF`, deferred to `bsr_in_scope`.
fn bsr_covered(opcode: u16) -> bool {
    opcode & 0xFF00 == 0x6100
}

/// The `BSR` scope/parity filter (called once `bsr_covered` matches). Clean iff the form is byte/word
/// (`disp8 != 0xFF`) AND the (always-taken) target is **even**. `disp8 == 0xFF` is the 68020 long-displacement
/// form — an address-error trap on the 68000 (the documented deferred class, 35 cases) → xfail. An odd target
/// raises an address error (the deferred odd-address class) → xfail. The target is `pc + 2 + sign_extend(disp)`
/// (relative to the extension-word address `pc + 2`), where `disp` is the opcode's low byte (byte form,
/// `disp8 != 0`) or the extension word `prefetch[1]` sign-extended (word form, `disp8 == 0`).
fn bsr_in_scope(opcode: u16, ini: &Value) -> bool {
    let disp8 = opcode & 0xFF;
    if disp8 == 0xFF {
        return false; // BSR.l long-displacement (0x61FF) — address-error trap → xfail
    }
    let pc = u32f(ini, "pc");
    let disp = if disp8 == 0 {
        // word form: the 16-bit displacement is the extension word, sign-extended.
        let ext = ini["prefetch"].as_array().unwrap()[1].as_u64().unwrap() as u16;
        ext as i16 as i32 as u32
    } else {
        // byte form: the opcode's low byte, sign-extended.
        disp8 as u8 as i8 as i32 as u32
    };
    let target = pc.wrapping_add(2).wrapping_add(disp);
    target & 1 == 0 // even target is in scope; odd target is an address error → xfail
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

/// The `JMP` scope/parity filter (called once `jmp_covered` matches). Clean iff the computed target is
/// **even**; an odd target raises an address-error exception (the deferred odd-address class) → xfail. The
/// target is computed exactly as the decoder's recipe does: `(An)` is the address register itself; the
/// register-file EaCalc modes (`d16(An)`/`d8(An,Xn)`/`abs.w`/`d16(PC)`/`d8(PC,Xn)`) reuse the shared
/// `compute_ea` (the SAME helper the recipe's `TargetCalc` is pinned to — parity is preserved under its
/// 24-bit mask, since bit 0 is never affected); `abs.l` assembles its two extension words directly (HIGH =
/// `prefetch[1]`, LOW = the word at `pc+4` in RAM, which is not yet in the queue).
fn jmp_in_scope(opcode: u16, ini: &Value) -> bool {
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as usize;
    let supervisor = (u32f(ini, "sr") & 0x2000) != 0;
    let areg = |r: usize| -> u32 {
        if r == 7 {
            if supervisor {
                u32f(ini, "ssp")
            } else {
                u32f(ini, "usp")
            }
        } else {
            u32f(ini, &format!("a{r}"))
        }
    };
    let target = match (mode, reg) {
        // (An) — the target is the address register itself.
        (2, _) => areg(reg),
        // abs.l — two extension words: HIGH = prefetch[1], LOW = the word at pc+4 (in RAM, not the queue).
        (7, 1) => {
            let regs = build_regs(ini);
            let hi = regs.prefetch[1] as u32;
            let ram = |addr: u32| -> u8 {
                ini["ram"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p.as_array().unwrap()[0].as_u64().unwrap() as u32 == addr)
                    .map(|p| p.as_array().unwrap()[1].as_u64().unwrap() as u8)
                    .unwrap_or(0)
            };
            let pc = regs.pc;
            let lo = ((ram(pc + 4) as u32) << 8) | ram(pc + 5) as u32;
            (hi << 16) | lo
        }
        // The register-file EaCalc control modes reuse the shared compute_ea (parity-preserving mask).
        _ => compute_ea(opcode, &build_regs(ini), Size::Word),
    };
    target & 1 == 0
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

/// The `JSR` scope/parity filter (called once `jsr_covered` matches). Clean iff the computed target is
/// **even**; an odd target raises an address-error exception (the deferred odd-address class) → xfail. The
/// target is computed identically to `JMP` (the JSR recipe's per-mode target arithmetic mirrors the JMP
/// recipe — only the post-target push/reload-interleave differs), so this reuses `jmp_in_scope` directly:
/// `(An)` is the address register itself; the register-file EaCalc modes reuse the shared `compute_ea`;
/// `abs.l` assembles its two extension words (HIGH = `prefetch[1]`, LOW = the word at `pc+4` in RAM). The
/// even-target parity was cross-checked against the `JSR.json` clean/dirty split (the 50/52/54/56-length
/// odd-target cases are exactly the odd-parity ones).
fn jsr_in_scope(opcode: u16, ini: &Value) -> bool {
    // The JSR mode/reg layout is identical to JMP (same `mmm rrr` low six bits), and the target arithmetic is
    // the same — so map the JSR opcode to the equivalent JMP opcode (0x4EC0 | ea) and reuse jmp_in_scope.
    let jmp_equiv = 0x4EC0 | (opcode & 0x3F);
    jmp_in_scope(jmp_equiv, ini)
}

/// Whether this opcode is an `RTS` the framework covers (`0x4E75` — the sole RTS encoding; `RTS.json` carries
/// only `0x4E75`).
fn rts_covered(opcode: u16) -> bool {
    opcode == 0x4E75
}

/// The `RTS` scope/parity filter (called once `rts_covered` matches). Clean iff the **popped 32-bit return
/// address is even**; an odd popped target raises an address-error exception (the deferred odd-address class —
/// those cases are length 58 in the data, vs. 16 for the clean pops) → xfail. The target is the long popped
/// off the stack exactly as the recipe pops it: hi word @ `SP`, lo word @ `SP + 2`, read from `initial.ram`
/// (`SP` is `ssp` in supervisor mode, `usp` in user mode — the active A7, exactly as the decoder selects it).
fn rts_in_scope(ini: &Value) -> bool {
    let supervisor = (u32f(ini, "sr") & 0x2000) != 0;
    let sp = if supervisor {
        u32f(ini, "ssp")
    } else {
        u32f(ini, "usp")
    };
    // hi @ SP, lo @ SP+2 (big-endian) — the popped 32-bit return address.
    let hi = move_ramw(ini, sp);
    let lo = move_ramw(ini, sp.wrapping_add(2));
    let target = (hi << 16) | lo;
    target & 1 == 0 // even popped target is in scope; odd = address error → xfail
}

/// Whether this opcode is an `RTR` the framework covers (`0x4E77` — the sole RTR encoding; `RTR.json` carries
/// only `0x4E77`).
fn rtr_covered(opcode: u16) -> bool {
    opcode == 0x4E77
}

/// The `RTR` scope/parity filter (called once `rtr_covered` matches). `RTR` pops a saved CCR word (@ `SP`)
/// and then the 32-bit return address (hi @ `SP + 2`, lo @ `SP + 4`). Clean iff the **popped 32-bit return
/// address is even**; an odd popped target raises an address-error exception (the deferred odd-address class —
/// length 62 in the data, vs. 20 for the clean pops) → xfail. The CCR pop never traps; only the return
/// address is parity-checked. `SP` is `ssp` in supervisor mode, `usp` in user mode (the active A7).
fn rtr_in_scope(ini: &Value) -> bool {
    let supervisor = (u32f(ini, "sr") & 0x2000) != 0;
    let sp = if supervisor {
        u32f(ini, "ssp")
    } else {
        u32f(ini, "usp")
    };
    // The CCR word is @ SP; the return address is hi @ SP+2, lo @ SP+4 (big-endian).
    let hi = move_ramw(ini, sp.wrapping_add(2));
    let lo = move_ramw(ini, sp.wrapping_add(4));
    let target = (hi << 16) | lo;
    target & 1 == 0 // even popped target is in scope; odd = address error → xfail
}

/// Whether this opcode is an `RTE` the framework covers (`0x4E73` — the sole RTE encoding; `RTE.json` carries
/// only `0x4E73`).
fn rte_covered(opcode: u16) -> bool {
    opcode == 0x4E73
}

/// The `RTE` scope/parity filter (called once `rte_covered` matches). `RTE` pops the 6-byte exception frame —
/// the saved SR word (@ `SP`) and the 32-bit return PC (hi @ `SP + 2`, lo @ `SP + 4`, the same word layout as
/// `RTR`'s CCR+PC frame). Clean iff the **popped 32-bit return PC is even**; an odd popped PC raises an
/// address-error exception (the deferred odd-address class — length 62 in the data, vs. 20 for the clean even
/// pops) → xfail (it flips into scope at E4). The SR pop never traps; only the return PC is parity-checked.
/// Every vendored case starts in supervisor mode (S=1); the restored SR may switch to user mode, which changes
/// only the reload's function code — and THAT is validated by the data (the runner asserts the per-cycle
/// transaction stream, so the FC2-vs-FC6 reload split is gate-checked, unlike the user→supervisor ENTRY
/// transform which has no vendored user-mode case). `SP` is `ssp` in supervisor mode (the start mode of every
/// case), `usp` in user mode (the active A7).
fn rte_in_scope(ini: &Value) -> bool {
    let supervisor = (u32f(ini, "sr") & 0x2000) != 0;
    let sp = if supervisor {
        u32f(ini, "ssp")
    } else {
        u32f(ini, "usp")
    };
    // The SR word is @ SP; the return PC is hi @ SP+2, lo @ SP+4 (big-endian).
    let hi = move_ramw(ini, sp.wrapping_add(2));
    let lo = move_ramw(ini, sp.wrapping_add(4));
    let target = (hi << 16) | lo;
    target & 1 == 0 // even popped PC is in scope; odd = address error → xfail
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

/// Whether the framework currently covers this case (else it is an xfail for this push). `ADD`/`SUB` in
/// word and byte sizes, each in two forms — `Dn,<ea>` (memory dest; word ADD=0xD140/SUB=0x9140, byte
/// ADD=0xD100/SUB=0x9100) and `<ea>,Dn` (register dest; word ADD=0xD040/SUB=0x9040, byte
/// ADD=0xD000/SUB=0x9000). For **word** memory modes only even computed EAs are in scope (an odd word access
/// is an address error — deferred → xfail). For **byte** memory modes every EA is in scope (a byte access
/// has no odd-address error) — no parity filter; `An`-direct is excluded for byte (`ADD.b An,Dn` is
/// illegal). For `(An)+`/`-(An)` the A7/SP register is in scope for both sizes (the step keeps the SP even:
/// 2 for word, 2 for byte-on-A7; routed through `ssp`/`usp` by the S-bit) — only the older `(An)` (mode 2)
/// keeps its A7 exclusion. `An`-direct word source has no memory access (A7 source legal).
fn covered(opcode: u16, ini: &Value) -> bool {
    // MOVE.w (`00 11 RRR MMM mmm rrr`, dst_mode != 1) — its own EA→EA scope/parity filter.
    if move_covered(opcode) {
        return move_in_scope(opcode, ini);
    }
    // MOVEA.w / MOVEA.l (`00 SS RRR 001 mmm rrr`, dst_mode == 1) — its own source-EA scope/parity filter
    // (destination is always An, a register write — no destination parity). Byte MOVEA is illegal → not
    // covered.
    if movea_size(opcode).is_some() {
        return movea_in_scope(opcode, ini);
    }
    // Bcc / BRA (`0110 cccc dddddddd`, 0x6xxx; cc != 1) — its own taken/not-taken parity filter (a taken
    // odd target is an address error → xfail; a fall-through is always clean). cc == 1 is BSR (a later
    // commit), excluded by `bcc_covered`.
    if bcc_covered(opcode) {
        return bcc_in_scope(opcode, ini);
    }
    // DBcc (`0101 cccc 11001 rrr`, opcode & 0xF0F8 == 0x50C8) — its own decode-time condition + counter
    // filter: cond true → fall-through (always clean); cond false → decrement Dn.w, and (counter live) take
    // the branch (clean iff the target is even, odd = address error → xfail) or (counter expired) fall through
    // (clean, but absent from the data). Only the 0x50C8 An-direct form is DBcc (every other mode is Scc).
    if dbcc_covered(opcode) {
        return dbcc_in_scope(opcode, ini);
    }
    // BSR (`0110 0001 dddddddd`, 0x61xx; cc == 1) — its own byte/word + even-target parity filter (a taken
    // odd target is an address error → xfail; the 68020 long-disp form 0x61FF traps on the 68000 → xfail).
    if bsr_covered(opcode) {
        return bsr_in_scope(opcode, ini);
    }
    // JMP `<control ea>` (`0100 1110 11 mmm rrr`, 0x4EC0 | ea) — its own target-parity filter (an odd target
    // is an address error → xfail). The seven control addressing modes only.
    if jmp_covered(opcode) {
        return jmp_in_scope(opcode, ini);
    }
    // JSR `<control ea>` (`0100 1110 10 mmm rrr`, 0x4E80 | ea) — the SAME seven control modes as JMP, the
    // SAME even-target parity filter (an odd target is an address error → xfail). The JSR recipe pushes a
    // 32-bit return address (the reload splits around the push), but the target arithmetic is JMP's.
    if jsr_covered(opcode) {
        return jsr_in_scope(opcode, ini);
    }
    // RTS (`0x4E75`) — its own popped-target parity filter (an odd popped 32-bit return address is an address
    // error → xfail; the clean even pops are 16 cyc, the odd ones 58). No EA, no flags.
    if rts_covered(opcode) {
        return rts_in_scope(ini);
    }
    // RTR (`0x4E77`) — like RTS but pops a saved CCR word first; its own popped-target parity filter (an odd
    // popped 32-bit return address is an address error → xfail; clean even pops are 20 cyc, odd ones 62). The
    // CCR pop affects the SR but never traps.
    if rtr_covered(opcode) {
        return rtr_in_scope(ini);
    }
    // RTE (`0x4E73`) — return from exception: pop the 6-byte frame (SR + 32-bit PC), restore the full SR (may
    // switch S/T), pop SP by 6 while still supervisor, then reload at the popped PC (the reload FC follows the
    // RESTORED mode). Its own popped-PC parity filter (an odd popped PC is an address error → xfail; clean
    // even pops are 20 cyc, odd ones 62). The supervisor→user reload-FC split IS gate-validated by the data.
    if rte_covered(opcode) {
        return rte_in_scope(ini);
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
    // Read the value of address register `reg` exactly as the decoder's `addr_reg` does: A7 is `ssp` in
    // supervisor mode, `usp` in user mode (there is no `a7` field) — needed so `(A7)+`/`-(A7)` parity is
    // computed correctly.
    let supervisor = (u32f(ini, "sr") & 0x2000) != 0;
    let areg = |reg: usize| -> u32 {
        if reg == 7 {
            if supervisor {
                u32f(ini, "ssp")
            } else {
                u32f(ini, "usp")
            }
        } else {
            u32f(ini, &format!("a{reg}"))
        }
    };
    // A word memory access to an odd computed EA raises an address error (deferred → xfail). The EA is
    // computed exactly as the decoder does: `(An)`/`(An)+` access at `An`; `-(An)` at `An - step` (step 2
    // for word, so parity is preserved). Filter on the actual accessed address.
    let even = |reg: usize| areg(reg) & 1 == 0;
    let even_predec = |reg: usize| areg(reg).wrapping_sub(2) & 1 == 0;
    // For a long `-(An)` the access address is `An - 4` (a long predec steps by 4); parity is preserved.
    let even_predec_long = |reg: usize| areg(reg).wrapping_sub(4) & 1 == 0;
    // For the register-file EaCalc modes (`d16(An)` = 5, `d8(An,Xn)` = 6, `abs.w` = 111/000,
    // `d16(PC)` = 111/010, `d8(PC,Xn)` = 111/011) the accessed address is the shared `compute_ea` (the SAME
    // helper the decoder's recipe is pinned to by the hard-gate agreement test); a word access to an odd EA
    // is an address error → xfail.
    let even_computed = || compute_ea(opcode, &build_regs(ini), Size::Word) & 1 == 0;
    // abs.l (111/001) assembles its address from TWO extension words: HIGH = prefetch[1], LOW = the word at
    // pc+4 (which is NOT in the queue — it shifts in via the first refill). `compute_ea` can't reach the LOW
    // word (it lives in RAM), so the parity filter assembles the full EA directly here from `ini`, exactly
    // as the recipe's two-EaCalc interleave does. A word access to an odd EA is an address error → xfail.
    let abs_l_ea = || {
        let regs = build_regs(ini);
        let hi = regs.prefetch[1] as u32;
        let ram = |addr: u32| -> u8 {
            ini["ram"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p.as_array().unwrap()[0].as_u64().unwrap() as u32 == addr)
                .map(|p| p.as_array().unwrap()[1].as_u64().unwrap() as u8)
                .unwrap_or(0)
        };
        let pc = regs.pc;
        let lo = ((ram(pc + 4) as u32) << 8) | ram(pc + 5) as u32;
        ((hi << 16) | lo) & 0x00FF_FFFF
    };
    let even_abs_l = || abs_l_ea() & 1 == 0;
    // <op>.w Dn,<ea> — memory destination (modes (An)=2, (An)+=3, -(An)=4).
    if opcode & 0xF1C0 == 0xD140 || opcode & 0xF1C0 == 0x9140 {
        let mode = (opcode >> 3) & 7;
        let reg = (opcode & 7) as usize;
        return match mode {
            // (An) — even access address, not A7 (A7 form lands with its own slice).
            2 => reg != 7 && even(reg),
            // (An)+ — even access address; A7/SP in scope for word.
            3 => even(reg),
            // -(An) — even decremented access address; A7/SP in scope for word.
            4 => even_predec(reg),
            // d16(An) — computed EA must be even (odd → address error → xfail).
            5 => even_computed(),
            // d8(An,Xn) — computed EA (An + index + disp8) must be even.
            6 => even_computed(),
            // abs.w (111/000) — computed EA (sign-extended ext word) must be even.
            7 if reg == 0 => even_computed(),
            // abs.l (111/001) — two-word computed EA must be even.
            7 if reg == 1 => even_abs_l(),
            // Other alterable-memory dest modes: out of slice this push.
            _ => false,
        };
    }
    // <op>.w <ea>,Dn — register destination.
    if opcode & 0xF1C0 == 0xD040 || opcode & 0xF1C0 == 0x9040 {
        let mode = (opcode >> 3) & 7;
        let reg = (opcode & 7) as usize;
        return match mode {
            // Dn (register direct).
            0 => true,
            // An (register direct) — legal `ADD.w`/`SUB.w An,Dn`; A7 source is fine (no memory access).
            1 => true,
            // (An) — even source address, not A7 (A7 form lands with its own slice).
            2 => reg != 7 && even(reg),
            // (An)+ — even source address; A7/SP in scope for word.
            3 => even(reg),
            // -(An) — even decremented source address; A7/SP in scope for word.
            4 => even_predec(reg),
            // d16(An) — computed EA (An + sign-extended disp) must be even.
            5 => even_computed(),
            // d8(An,Xn) (110) — computed EA (An + index + disp8) must be even.
            6 => even_computed(),
            // abs.w (111/000) — computed EA must be even.
            7 if reg == 0 => even_computed(),
            // abs.l (111/001) — two-word computed EA must be even.
            7 if reg == 1 => even_abs_l(),
            // d16(PC) (111/010) — computed EA (pc+2 + sign-extended disp) must be even. Source-only.
            7 if reg == 2 => even_computed(),
            // d8(PC,Xn) (111/011) — computed EA (pc+2 + index + disp8) must be even. Source-only.
            7 if reg == 3 => even_computed(),
            // #imm (111/100).
            7 if reg == 4 => true,
            // Other EA modes: out of slice this push.
            _ => false,
        };
    }
    // <op>.b Dn,<ea> — byte memory destination (ADD=0xD100, SUB=0x9100). A byte access has NO odd-address
    // error (byte EAs may be odd), so there is no word-parity filter — every alterable-memory mode is in
    // scope. The `(A7)+`/`-(A7)` byte forms step by 2 (the in-scope A7 byte rule, keeping the SP even); the
    // step routes through `ssp`/`usp` by the S-bit. `An`-direct is not a destination here.
    if opcode & 0xF1C0 == 0xD100 || opcode & 0xF1C0 == 0x9100 {
        let mode = (opcode >> 3) & 7;
        let reg = (opcode & 7) as usize;
        return match mode {
            // (An) — byte form, any address (no parity filter). The A7 (`(A7)`) byte case stays xfail with
            // its older word sibling — its byte +2 step and exception cases are a separate slice.
            2 => reg != 7,
            // (An)+ / -(An) — byte step (1, or 2 for A7); any address. A7/SP in scope for byte.
            3 | 4 => true,
            // d16(An) / d8(An,Xn) / abs.w / abs.l — any (possibly odd) byte EA.
            5 | 6 => true,
            7 if reg == 0 || reg == 1 => true,
            _ => false,
        };
    }
    // <op>.b <ea>,Dn — byte register destination (ADD=0xD000, SUB=0x9000). Same broad byte coverage; `An`-
    // direct (mode 1) is EXCLUDED for byte (`ADD.b An,Dn` is illegal).
    if opcode & 0xF1C0 == 0xD000 || opcode & 0xF1C0 == 0x9000 {
        let mode = (opcode >> 3) & 7;
        let reg = (opcode & 7) as usize;
        return match mode {
            // Dn (register direct).
            0 => true,
            // An-direct is illegal for byte → out of scope (and not produced by the decoder).
            1 => false,
            // (An) — byte form, any address; the A7 form stays xfail with its word sibling.
            2 => reg != 7,
            // (An)+ / -(An) — byte step (1, or 2 for A7); any address.
            3 | 4 => true,
            // d16(An) / d8(An,Xn) / abs.w / abs.l / d16(PC) / d8(PC,Xn) — any (possibly odd) byte EA.
            5 | 6 => true,
            7 if matches!(reg, 0..=3) => true,
            // #imm (111/100) — byte immediate.
            7 if reg == 4 => true,
            _ => false,
        };
    }
    // <op>.l Dn,<ea> — long memory destination (ADD=0xD180, SUB=0x9180). A `.l` access is two word bus
    // accesses; a long access to an ODD computed EA is an address error → xfail (same word-style parity
    // filter, only the predec step is 4). `An`-direct is not a destination.
    if opcode & 0xF1C0 == 0xD180 || opcode & 0xF1C0 == 0x9180 {
        let mode = (opcode >> 3) & 7;
        let reg = (opcode & 7) as usize;
        return match mode {
            // (An) — even access, not A7 (the A7 form lands with its own slice, as in word).
            2 => reg != 7 && even(reg),
            // (An)+ — even access; A7/SP in scope (step 4 keeps the SP even).
            3 => even(reg),
            // -(An) — even decremented access (An - 4).
            4 => even_predec_long(reg),
            // d16(An) / d8(An,Xn) / abs.w — even computed EA.
            5 | 6 => even_computed(),
            7 if reg == 0 => even_computed(),
            // abs.l — even two-word computed EA.
            7 if reg == 1 => even_abs_l(),
            _ => false,
        };
    }
    // <op>.l <ea>,Dn — long register destination (ADD=0xD080, SUB=0x9080). Same long parity filter; `An`-
    // direct (mode 1) is LEGAL for long (`ADD.l An,Dn`) — A7 source is fine (no memory access).
    if opcode & 0xF1C0 == 0xD080 || opcode & 0xF1C0 == 0x9080 {
        let mode = (opcode >> 3) & 7;
        let reg = (opcode & 7) as usize;
        return match mode {
            // Dn / An (register direct) — no memory access.
            0 | 1 => true,
            // (An) — even source, not A7 (A7 form is a separate slice, as in word).
            2 => reg != 7 && even(reg),
            // (An)+ — even source; A7/SP in scope (step 4).
            3 => even(reg),
            // -(An) — even decremented source (An - 4).
            4 => even_predec_long(reg),
            // d16(An) / d8(An,Xn) / abs.w / d16(PC) / d8(PC,Xn) — even computed EA.
            5 | 6 => even_computed(),
            7 if matches!(reg, 0 | 2 | 3) => even_computed(),
            // abs.l — even two-word computed EA.
            7 if reg == 1 => even_abs_l(),
            // #imm.l (111/100) — long immediate.
            7 if reg == 4 => true,
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
        ran >= 98927,
        "expected 98927 covered cases — ADD/SUB (21790: word 5871 + byte 9974 + long 5945) plus \
         MOVE.w (3154: all 12 source modes × Dn + the alterable-memory dest modes \
         ((An)/(An)+/-(An)/d16(An)/d8(An,Xn)/abs.w/abs.l), even word EAs since an odd word access is an \
         address error, the (A7) mode-2 word form xfail) plus MOVE.b (7796: same modes, byte excludes \
         An-direct as a source (MOVE.b An,<ea> illegal), NO parity filter since a byte access has no \
         odd-address error, the (A7) mode-2 byte form xfail) plus MOVE.l (3082: same modes — An-direct is a \
         legal long source — the long word-style parity filter (a long access to an odd EA is an address \
         error → xfail; the -(An) step is 4 and #imm.l is two ext words), the (A7) mode-2 form xfail) plus \
         MOVEA.w (5180) + MOVEA.l (5245): all 12 source modes (An-direct legal both sizes) → An, no flags \
         (.w sign-extends, .l writes full 32), no destination access; even word/long source EAs since an odd \
         access is an address error, the (A7) mode-2 source form xfail; byte MOVEA is illegal (not covered) \
         plus Bcc/BRA (5865: cc != 1 (cc == 1 is BSR, a later commit); not-taken always clean (byte 8 cyc, \
         word 12 cyc), taken even-target in scope (10 cyc both forms), taken odd-target = address error → \
         xfail) plus BSR (4085: byte/word form with an even (always-taken) target (18 cyc both forms — push \
         hi @ SP−4 then lo @ SP−2, then the SetPc reload); odd-target = address error → xfail, and the 68020 \
         long-disp form 0x61FF (35 cases) traps on the 68000 → xfail) plus JMP (4259: the seven control \
         modes — (An) 8 cyc, (d16,An)/abs.w/(d16,PC) 10 cyc, abs.l 12 cyc, (d8,An,Xn)/(d8,PC,Xn) 14 cyc; \
         even-target in scope (target UNMASKED — abs.l keeps its full 32 bits), odd-target = address error → \
         xfail) plus JSR (4183: the SAME seven control modes, the F1 per-mode target combined with the F2 \
         return-address push via the reload-interleave (read target → push hi @ SP−4, lo @ SP−2 → read \
         target+2) — (An) 16 cyc, (d16,An)/abs.w/(d16,PC) 18 cyc, abs.l 20 cyc, (d8,An,Xn)/(d8,PC,Xn) 22 cyc; \
         return = pc+N (N: (An) 2, (d16,An)/abs.w/(d16,PC)/indexed 4, abs.l 6 — VERIFIED against the pushed \
         value in the data, the recon prose said pc+4 for abs.l but the DATA shows pc+6); even-target in scope \
         (target UNMASKED), odd-target = address error → xfail) plus RTS (4008: the sole 0x4E75 encoding — pop \
         the 32-bit return address (hi @ SP, lo @ SP+2, FC=Data), post-increment SP by 4, assemble the UNMASKED \
         target, SetPc + two-Prefetch queue reload; 16 cyc; even popped-target in scope, odd popped-target = \
         address error (58 cyc) → xfail) plus DBcc (6101: the An-direct 0x50C8 form — cond true → fall-through \
         (12 cyc, NO decrement, 4096 cases), cond false counter live → decrement Dn.w + branch taken (10 cyc, \
         even target, 2005 cases), cond false counter expired (Dn.w == 0) → decrement + fall-through (14 cyc, \
         correctness-only, ABSENT from the data — 0 cases); taken odd target = address error (52 cyc) → xfail) \
         plus RTR (4038: the sole 0x4E77 encoding — pop the saved CCR word (@ SP) then the 32-bit return \
         address (hi @ SP+2, ccr @ SP, lo @ SP+4 — the data's reordered read stream), restore the low 5 CCR \
         bits, post-increment SP by 6, SetPc + two-Prefetch reload; 20 cyc; even popped-target in scope, odd \
         popped-target = address error (62 cyc) → xfail) plus TRAP (8065: the standard 6-byte exception entry \
         — TRAP #n → vector 32+n; saved PC = pc+2 (no leading prefetch), frame written PCL @ B+4 / SR @ B+0 / \
         PCH @ B+2 (FC=5), vector fetched FC=5, handler reloaded FC=6 with n2 between; 34 cyc; all supervisor \
         (the S/T/A7 transform is structurally exercised but a no-op on the data — correctness-only)) plus \
         RTE (4011: the sole 0x4E73 encoding — pop the 6-byte frame (PC-hi @ SP+2, SR @ SP, PC-lo @ SP+4 — the \
         data's read order, FC=5), assemble the UNMASKED return PC, pop SP by 6 while still supervisor, restore \
         the full SR masked 0xA71F (LoadSr — may switch S supervisor→user and T), SetPc + two-Prefetch reload \
         under the RESTORED mode's FC (FC2 user / FC6 supervisor — the split IS gate-validated); 20 cyc; even \
         popped-PC in scope, odd popped-PC = address error (62 cyc) → xfail (flips in at E4)) plus \
         TRAPV (8065: the sole 0x4E76 encoding — a conditional trap resolved at decode time on the V flag; V=0 \
         → no trap (a single FC6 prefetch refill @ pc+4, 4 cyc, 3970 cases); V=1 → the standard 6-byte frame to \
         vector 7 (34 cyc, 4095 cases), distinguished from TRAP by a LEADING prefetch (its first bus event is \
         an FC6 refill @ pc+4, not the PCL write), saved PC = pc+2 captured BEFORE that prefetch; all \
         supervisor (the S/T/A7 transform is structurally exercised but a no-op on the data — correctness-only)), \
         ran {ran}"
    );
    eprintln!("SingleStepTests ADD+SUB+MOVE+MOVEA+Bcc+BSR+JMP+JSR+RTS+DBcc+RTR+TRAP+RTE+TRAPV (.w + .b + .l): {ran} covered cases passed (both framework drivers, regs/SR/RAM/prefetch/cycles/transactions)");
}
