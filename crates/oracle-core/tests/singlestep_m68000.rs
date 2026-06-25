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
//! Versioned xfail manifest (slice scope — implemented later): odd-address *word* accesses (which raise an
//! address-error exception — byte accesses have no such error, so odd byte EAs are in scope), the `A7` form
//! of the older `(An)` (mode 2) memory access, `An`-direct as a byte source (`ADD.b An,Dn` is illegal), and
//! the remaining EA modes / sizes are skipped (see [`covered`]). The auto-(in/de)crement `(A7)+`/`-(A7)`
//! forms are in scope for both sizes (word steps 2; byte steps 2 for A7 to keep the SP even). If the vendor
//! data is missing, the test skips cleanly (run `tools/fetch-tests.sh`).

use oracle_core::m68000::bus68k::{FlatBus, Transaction, TxKind};
use oracle_core::m68000::ea::compute_ea;
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
        ran >= 46200,
        "expected ~46247 covered cases — ADD/SUB (~21790: word ~5871 + byte ~9974 + long ~5945) plus \
         MOVE.w (3154: all 12 source modes × Dn + the alterable-memory dest modes \
         ((An)/(An)+/-(An)/d16(An)/d8(An,Xn)/abs.w/abs.l), even word EAs since an odd word access is an \
         address error, the (A7) mode-2 word form xfail) plus MOVE.b (7796: same modes, byte excludes \
         An-direct as a source (MOVE.b An,<ea> illegal), NO parity filter since a byte access has no \
         odd-address error, the (A7) mode-2 byte form xfail) plus MOVE.l (3082: same modes — An-direct is a \
         legal long source — the long word-style parity filter (a long access to an odd EA is an address \
         error → xfail; the -(An) step is 4 and #imm.l is two ext words), the (A7) mode-2 form xfail) plus \
         MOVEA.w (5180) + MOVEA.l (5245): all 12 source modes (An-direct legal both sizes) → An, no flags \
         (.w sign-extends, .l writes full 32), no destination access; even word/long source EAs since an odd \
         access is an address error, the (A7) mode-2 source form xfail; byte MOVEA is illegal (not covered), \
         ran {ran}"
    );
    eprintln!("SingleStepTests ADD+SUB+MOVE+MOVEA (.w + .b + .l): {ran} covered cases passed (both framework drivers, regs/SR/RAM/prefetch/cycles/transactions)");
}
