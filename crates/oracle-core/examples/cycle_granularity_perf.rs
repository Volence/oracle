//! Perf measurement for the cycle-granularity decision, executing `ADD.w Dn,(An)` in a tight loop.
//! Run with `cargo run --release --example cycle_granularity_perf`.
//!
//! Compares four execution models over the same opcode:
//!   1. instruction-stepped (the prototype's atomic `step_instruction`) — the speed *baseline*;
//!   2. **framework run-to-completion** (`Cpu68000::run_instruction`) — the new default fast path;
//!   3. framework step-one-micro-op (`start_instruction` + `step_micro_op` to completion) — the quiesce
//!      path's cost when driven micro-op by micro-op;
//!   4. per-master-clock FSM (the prototype's `AddWFsm`) — granularity C, the finest/worst case.
//!
//! The decision-relevant number is (2) vs (1): how much the interpreted micro-op recipe costs the default
//! path relative to a hand-written atomic execute. Uses a non-logging flat bus so the measurement isolates
//! CPU-stepping overhead (not allocation/logging).

use oracle_core::m68000::bus68k::Bus68k;
use oracle_core::m68000::microop::{Cpu68000, Step};
use oracle_core::m68000::prototype::{step_instruction, AddWFsm};
use oracle_core::m68000::registers::Registers;
use std::hint::black_box;
use std::time::Instant;

const ADDR_MASK: u32 = 0x00FF_FFFF;

struct PerfBus {
    mem: Vec<u8>,
}
impl Bus68k for PerfBus {
    fn read16(&mut self, addr: u32, _fc: u8) -> u16 {
        let a = (addr & ADDR_MASK) as usize;
        ((self.mem[a] as u16) << 8) | self.mem[(a + 1) & ADDR_MASK as usize] as u16
    }
    fn write16(&mut self, addr: u32, _fc: u8, value: u16) {
        let a = (addr & ADDR_MASK) as usize;
        self.mem[a] = (value >> 8) as u8;
        self.mem[(a + 1) & ADDR_MASK as usize] = (value & 0xFF) as u8;
    }
}

fn start_regs() -> Registers {
    let mut r = Registers {
        d: [0; 8],
        a: [0; 7],
        usp: 0,
        ssp: 0,
        pc: 0x0C00,
        sr: 0x2717,
        prefetch: [0xDB50, 0x6A3C], // ADD.w D5,(A0)
    };
    r.d[5] = 0x020D_2596;
    r.a[0] = 0x0004_4F46; // even, in range
    r
}

/// Time `n` iterations of `body`, returning ns/op. `body` returns a value folded into an accumulator so
/// the optimizer cannot elide the work.
fn bench(n: u64, mut body: impl FnMut() -> u16) -> f64 {
    let t0 = Instant::now();
    let mut acc = 0u64;
    for _ in 0..n {
        acc = acc.wrapping_add(black_box(body()) as u64);
    }
    let dt = t0.elapsed();
    black_box(acc);
    dt.as_secs_f64() * 1e9 / n as f64
}

fn main() {
    let n: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20_000_000);

    let mut bus = PerfBus {
        mem: vec![0u8; 0x0100_0000],
    };
    let base = start_regs();

    // 1. Instruction-stepped baseline (atomic execute).
    let instr_ns = bench(n, || {
        let mut r = black_box(base.clone());
        step_instruction(&mut r, black_box(&mut bus));
        r.sr
    });

    // 2. Framework run-to-completion (the default fast path).
    let rtc_ns = bench(n, || {
        let mut cpu = Cpu68000::new(black_box(base.clone()));
        cpu.run_instruction(black_box(&mut bus));
        cpu.regs.sr
    });

    // 3. Framework step-one-micro-op driven to completion (the quiesce path).
    let step_ns = bench(n, || {
        let mut cpu = Cpu68000::new(black_box(base.clone()));
        cpu.start_instruction();
        loop {
            if let Step::Done(_) = cpu.step_micro_op(black_box(&mut bus)) {
                break;
            }
        }
        cpu.regs.sr
    });

    // 4. Per-master-clock FSM (granularity C — finest/worst case).
    let fsm_ns = bench(n, || {
        let mut r = black_box(base.clone());
        AddWFsm::new(&r).run_to_completion(&mut r, black_box(&mut bus));
        r.sr
    });

    let row = |name: &str, ns: f64| {
        println!(
            "  {name:<34} {ns:6.2} ns/op  ({:6.1} M ops/s)  {:.2}x baseline",
            1000.0 / ns,
            ns / instr_ns
        );
    };
    println!("iterations: {n}\n");
    row("1. instruction-stepped (baseline)", instr_ns);
    row("2. framework run-to-completion", rtc_ns);
    row("3. framework step-one-micro-op", step_ns);
    row("4. per-master-clock FSM", fsm_ns);
}
