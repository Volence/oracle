//! SingleStepTests runner for the 68000 micro-op framework.
//!
//! Drives the pinned, vendored SingleStepTests data (`tools/fetch-tests.sh`) for every covered `ADD`/`SUB`
//! case in **word, byte and long** sizes — `Dn,<ea>` (alterable-memory destination: (An) / (An)+ / -(An) /
//! d16(An) / d8(An,Xn) / abs.w / abs.l) and `<ea>,Dn` (register destination) for the source modes Dn / An
//! (word/long) / (An) / (An)+ / -(An) / d16(An) / d8(An,Xn) / abs.w / abs.l / d16(PC) / d8(PC,Xn) / #imm —
//! and asserts post regs/SR/RAM/prefetch, the cycle count, **and** the per-cycle bus-transaction stream
//! (byte-granular for `.b`; two word accesses per `.l` operand, hi then lo for a read, lo then hi for a
//! write), through *both* framework drivers (run-to-completion fast path and the step-one-micro-op quiesce
//! path), which must also agree with each other.
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
        ran >= 21700,
        "expected ~21790 covered ADD/SUB cases — word (~5871) + byte (~9974) + long (~5945: Dn,<ea> + \
         <ea>,Dn for Dn/An/(An)/(An)+/-(An)/d16(An)/d8(An,Xn)/abs.w/abs.l/d16(PC)/d8(PC,Xn)/#imm, even EAs \
         since a long access to an odd EA is an address error), ran {ran}"
    );
    eprintln!("SingleStepTests ADD+SUB (.w + .b + .l): {ran} covered cases passed (both framework drivers, regs/SR/RAM/prefetch/cycles/transactions)");
}
