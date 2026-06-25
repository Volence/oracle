//! Perf measurement for the cycle-granularity decision: instruction-stepped vs cycle-stepped FSM,
//! executing `ADD.w Dn,(An)` in a tight loop. Run with `cargo run --release --example cycle_granularity_perf`.
//!
//! Uses a non-logging flat bus so the measurement isolates CPU-stepping overhead (not allocation/logging).

use oracle_core::m68000::prototype::{decode_add_w_dn_an, step_instruction, AddWFsm, Bus68k};
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
        prefetch: [0xDB50, 0x6A3C],
    };
    r.d[5] = 0x020D_2596;
    r.a[0] = 0x0004_4F46; // even, in range
    r
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
    let _ = decode_add_w_dn_an(base.prefetch[0]); // sanity

    // Instruction-stepped.
    let t0 = Instant::now();
    let mut acc = 0u64;
    for _ in 0..n {
        let mut r = black_box(base.clone());
        step_instruction(&mut r, black_box(&mut bus));
        acc = acc.wrapping_add(black_box(r.sr) as u64);
    }
    let dt_instr = t0.elapsed();

    // Cycle-stepped FSM.
    let t1 = Instant::now();
    for _ in 0..n {
        let mut r = black_box(base.clone());
        let mut fsm = AddWFsm::new(&r);
        fsm.run_to_completion(&mut r, black_box(&mut bus));
        acc = acc.wrapping_add(black_box(r.sr) as u64);
    }
    let dt_fsm = t1.elapsed();

    let instr_ns = dt_instr.as_secs_f64() * 1e9 / n as f64;
    let fsm_ns = dt_fsm.as_secs_f64() * 1e9 / n as f64;
    println!("iterations:        {n}");
    println!(
        "instruction-step:  {:.2} ns/op  ({:.1} M ops/s)",
        instr_ns,
        1000.0 / instr_ns
    );
    println!(
        "cycle-step FSM:    {:.2} ns/op  ({:.1} M ops/s)",
        fsm_ns,
        1000.0 / fsm_ns
    );
    println!("FSM / instr ratio: {:.2}x", fsm_ns / instr_ns);
    println!("(checksum {acc})");
}
