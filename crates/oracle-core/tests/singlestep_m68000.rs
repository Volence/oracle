//! SingleStepTests runner for the 68000 vertical slice.
//!
//! Drives the pinned, vendored SingleStepTests data (`tools/fetch-tests.sh`) for every clean
//! `ADD.w Dn,(An)` case and asserts post regs/SR/RAM/prefetch, the cycle count, **and** the per-cycle
//! bus-transaction stream — for *both* stepping models (instruction-stepped and the cycle-stepped FSM),
//! which must also agree with each other.
//!
//! Versioned xfail manifest (slice scope — implemented later): the `A7`/SP form (`An == 7`) and
//! odd-address cases (which raise an address-error exception) are skipped. If the vendor data is
//! missing, the test skips cleanly (run `tools/fetch-tests.sh`).

use oracle_core::m68000::prototype::{step_instruction, AddWFsm, FlatBus, Transaction, TxKind};
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

#[test]
fn add_w_dn_an_matches_singlesteptests() {
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
        if opcode & 0xF1F8 != 0xD150 {
            continue; // not ADD.w Dn,(An)
        }
        let an = (opcode & 7) as usize;
        if an == 7 {
            continue; // xfail: A7/SP form (slice scope)
        }
        if u32f(ini, &format!("a{an}")) & 1 != 0 {
            continue; // xfail: odd address -> address-error exception (slice scope)
        }

        let length = t["length"].as_u64().unwrap() as u32;
        let expected = expected_transactions(t);

        // Instruction-stepped path.
        let mut regs = build_regs(ini);
        let mut bus = build_bus(ini);
        let cycles = step_instruction(&mut regs, &mut bus);
        assert_eq!(cycles, length, "cycle count [{}]", t["name"]);
        assert_final(t, &regs, &bus);
        assert_eq!(bus.log, expected, "transactions [{}]", t["name"]);

        // Cycle-stepped FSM path — must agree with both the suite and the instruction-stepped path.
        let mut regs_fsm = build_regs(ini);
        let mut bus_fsm = build_bus(ini);
        let mut fsm = AddWFsm::new(&regs_fsm);
        let cycles_fsm = fsm.run_to_completion(&mut regs_fsm, &mut bus_fsm);
        assert_eq!(cycles_fsm, cycles, "fsm cycle count [{}]", t["name"]);
        assert_eq!(regs_fsm, regs, "fsm final regs [{}]", t["name"]);
        assert_eq!(bus_fsm.log, bus.log, "fsm transactions [{}]", t["name"]);

        ran += 1;
    }

    assert!(
        ran >= 150,
        "expected ~179 clean ADD.w Dn,(An) cases, ran {ran}"
    );
    eprintln!("SingleStepTests ADD.w Dn,(An): {ran} cases passed (instruction-stepped + FSM, regs/RAM/prefetch/cycles/transactions)");
}
