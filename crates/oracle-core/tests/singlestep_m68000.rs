//! SingleStepTests runner for the 68000 micro-op framework.
//!
//! Drives the pinned, vendored SingleStepTests data (`tools/fetch-tests.sh`) for every covered `ADD.w`
//! case — `Dn,(An)` (memory destination) and `<ea>,Dn` (register destination) for source modes Dn / (An)
//! / #imm — and asserts post regs/SR/RAM/prefetch, the cycle count, **and** the per-cycle bus-transaction
//! stream, through *both* framework drivers (run-to-completion fast path and the step-one-micro-op quiesce
//! path), which must also agree with each other.
//!
//! Versioned xfail manifest (slice scope — implemented later): the `A7`/SP forms (`reg == 7`), odd-address
//! `(An)` cases (which raise an address-error exception), and the remaining EA modes / sizes are skipped
//! (see [`covered`]). If the vendor data is missing, the test skips cleanly (run `tools/fetch-tests.sh`).

use oracle_core::m68000::bus68k::{FlatBus, Transaction, TxKind};
use oracle_core::m68000::microop::{Cpu68000, Step};
use oracle_core::m68000::registers::Registers;
use serde_json::Value;
use std::path::Path;

const DATA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../vendor/ProcessorTests/68000/v1/ADD.w.json"
);

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
        out.push(Transaction {
            kind,
            fc: arr[2].as_u64().unwrap() as u8,
            addr: arr[3].as_u64().unwrap() as u32,
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

/// Whether the framework currently covers this case (else it is an xfail for this push):
/// `ADD.w Dn,(An)` (memory dest) and `ADD.w <ea>,Dn` (register dest) for source modes Dn / (An) / #imm,
/// minus the A7/SP forms and odd-address `(An)` cases (which raise an address error — deferred).
fn covered(opcode: u16, ini: &Value) -> bool {
    let even = |reg: usize| u32f(ini, &format!("a{reg}")) & 1 == 0;
    if opcode & 0xF1F8 == 0xD150 {
        // ADD.w Dn,(An)
        let an = (opcode & 7) as usize;
        return an != 7 && even(an);
    }
    if opcode & 0xF1C0 == 0xD040 {
        // ADD.w <ea>,Dn
        let mode = (opcode >> 3) & 7;
        let reg = (opcode & 7) as usize;
        return match mode {
            0 => true,                  // Dn (register direct)
            2 => reg != 7 && even(reg), // (An), even address, not A7
            7 if reg == 4 => true,      // #imm
            _ => false,                 // other EA modes: out of slice this push
        };
    }
    false // other ADD.w forms (e.g. Dn,(An)+ / byte / long): out of slice this push
}

#[test]
fn add_w_matches_singlesteptests() {
    if !Path::new(DATA).exists() {
        eprintln!("SKIP: {DATA} missing — run tools/fetch-tests.sh");
        return;
    }
    let file = std::fs::File::open(DATA).unwrap();
    let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();

    let mut ran = 0usize;
    for t in &data {
        let ini = &t["initial"];
        let opcode = ini["prefetch"][0].as_u64().unwrap() as u16;
        if !covered(opcode, ini) {
            continue;
        }

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

        ran += 1;
    }

    assert!(
        ran >= 800,
        "expected ~810 covered ADD.w cases (Dn,(An) + <ea>,Dn for Dn/(An)/#imm), ran {ran}"
    );
    eprintln!("SingleStepTests ADD.w: {ran} covered cases passed (both framework drivers, regs/RAM/prefetch/cycles/transactions)");
}
