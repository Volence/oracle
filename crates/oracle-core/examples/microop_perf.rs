//! Interpreter throughput harness for the macro-RTC perf pass (audit policy 7). Measures the current
//! micro-op interpreter on a representative SST-derived workload so the fast-path design — and every
//! future perf slice — is measured, not guessed (the June ~5.4×-slower figure predates the integration
//! pivot; `MAX_OPS` has since grown 20→40 and the vocabulary with it). Reuses the vendored
//! SingleStepTests corpus as the workload (the exact distribution the fast path must reproduce); SKIPs
//! gracefully when the gitignored `vendor/` dir is absent, so it never breaks a fetch-less build.
//!
//! Three phases, each over the same prepared case set, repeated to ~1e8 executions:
//!   1. decode-only        — `decode(&regs)` (opcode dispatch + the `[MicroOp; MAX_OPS]` recipe build)
//!   2. exec-only (RTC)     — clone a pre-decoded template + `run_to_completion` (the walk the fast path targets)
//!   3. full (decode + RTC) — the real per-instruction interpreter cost the perf pass drives down
//!
//! Usage: `cargo run --release --example microop_perf -- [cap_per_file]`  (default cap 2000)

use oracle_core::m68000::bus68k::Bus68k;
use oracle_core::m68000::decode::decode;
use oracle_core::m68000::microop::MicroState;
use oracle_core::m68000::registers::Registers;
use serde_json::Value;
use std::hint::black_box;
use std::time::Instant;

const VENDOR_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../vendor/ProcessorTests/68000/v1"
);

/// A minimal, non-logging flat bus — the pure CPU+memory cost with none of `FlatBus`'s per-access
/// `Vec` logging (which would swamp the measurement and is System-side instrumentation, not CPU cost).
struct PerfBus {
    mem: Vec<u8>,
}
const MASK: u32 = 0x00FF_FFFF;
impl PerfBus {
    fn new() -> Self {
        Self {
            mem: vec![0u8; 0x0100_0000],
        }
    }
    fn poke(&mut self, addr: u32, val: u8) {
        self.mem[(addr & MASK) as usize] = val;
    }
}
impl Bus68k for PerfBus {
    fn read16(&mut self, addr: u32, _fc: u8) -> (u16, u32) {
        let a = (addr & MASK) as usize;
        let b = (addr.wrapping_add(1) & MASK) as usize;
        (((self.mem[a] as u16) << 8) | self.mem[b] as u16, 0)
    }
    fn write16(&mut self, addr: u32, _fc: u8, value: u16) -> u32 {
        let a = (addr & MASK) as usize;
        let b = (addr.wrapping_add(1) & MASK) as usize;
        self.mem[a] = (value >> 8) as u8;
        self.mem[b] = (value & 0xFF) as u8;
        0
    }
    fn read8(&mut self, addr: u32, _fc: u8) -> (u8, u32) {
        (self.mem[(addr & MASK) as usize], 0)
    }
    fn write8(&mut self, addr: u32, _fc: u8, value: u8) -> u32 {
        self.mem[(addr & MASK) as usize] = value;
        0
    }
    fn tas(&mut self, addr: u32, _fc: u8) -> (u8, u32) {
        let a = (addr & MASK) as usize;
        let orig = self.mem[a];
        self.mem[a] = orig | 0x80;
        (orig, 0)
    }
}

struct Case {
    regs: Registers,
    ram: Vec<(u32, u8)>,
}

fn parse_regs(ini: &Value) -> Registers {
    let mut d = [0u32; 8];
    for (i, slot) in d.iter_mut().enumerate() {
        *slot = ini[format!("d{i}")].as_u64().unwrap_or(0) as u32;
    }
    let mut a = [0u32; 7];
    for (i, slot) in a.iter_mut().enumerate() {
        *slot = ini[format!("a{i}")].as_u64().unwrap_or(0) as u32;
    }
    let pf = ini["prefetch"].as_array().unwrap();
    Registers {
        d,
        a,
        usp: ini["usp"].as_u64().unwrap_or(0) as u32,
        ssp: ini["ssp"].as_u64().unwrap_or(0) as u32,
        pc: ini["pc"].as_u64().unwrap_or(0) as u32,
        sr: ini["sr"].as_u64().unwrap_or(0) as u16,
        prefetch: [
            pf[0].as_u64().unwrap() as u16,
            pf[1].as_u64().unwrap() as u16,
        ],
    }
}

fn load(files: &[&str], cap: usize) -> Vec<Case> {
    let mut cases = Vec::new();
    for fname in files {
        let path = format!("{VENDOR_DIR}/{fname}");
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => {
                eprintln!("SKIP {fname} (missing)");
                continue;
            }
        };
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        for t in data.iter().take(cap) {
            let ini = &t["initial"];
            let ram = ini["ram"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    let p = p.as_array().unwrap();
                    (p[0].as_u64().unwrap() as u32, p[1].as_u64().unwrap() as u8)
                })
                .collect();
            cases.push(Case {
                regs: parse_regs(ini),
                ram,
            });
        }
    }
    cases
}

fn main() {
    let cap: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2000);

    // A common-code-weighted mix: MOVE / arithmetic / compare / logic / branch / address dominate real
    // 68k (Sonic) code. Heavy/rare ops (MUL, DIV, MOVEM) are included but at the same cap, so they are
    // over-represented vs a real ROM — the "common" sub-mix below isolates the realistic hot path.
    let common: &[&str] = &[
        "MOVE.w.json",
        "MOVE.l.json",
        "MOVE.b.json",
        "MOVE.q.json",
        "MOVEA.l.json",
        "MOVEA.w.json",
        "ADD.w.json",
        "ADD.l.json",
        "SUB.w.json",
        "SUB.l.json",
        "ADDA.l.json",
        "SUBA.l.json",
        "CMP.w.json",
        "CMP.l.json",
        "CMPA.l.json",
        "AND.w.json",
        "OR.w.json",
        "EOR.w.json",
        "TST.w.json",
        "TST.l.json",
        "CLR.w.json",
        "Bcc.json",
        "DBcc.json",
        "BSR.json",
        "JMP.json",
        "JSR.json",
        "RTS.json",
        "LEA.json",
        "ASL.w.json",
        "LSR.w.json",
        "BTST.json",
        "SWAP.json",
        "EXT.l.json",
        "NEG.w.json",
        "NOT.w.json",
    ];
    let heavy: &[&str] = &["MULU.json", "MULS.json", "DIVU.json", "MOVEM.l.json"];

    for (label, files) in [("COMMON", common), ("HEAVY", heavy)] {
        let cases = load(files, cap);
        if cases.is_empty() {
            eprintln!("no cases for {label}");
            continue;
        }
        // Union-load all RAM once (writes drift memory harmlessly; regs reset per case → timing faithful).
        let mut bus = PerfBus::new();
        for c in &cases {
            for &(a, v) in &c.ram {
                bus.poke(a, v);
            }
        }
        // Pre-decoded templates for the exec-only phase.
        let templates: Vec<MicroState> = cases.iter().map(|c| decode(&c.regs)).collect();

        // Pick a rep count for ~1e8 instruction executions.
        let reps = (100_000_000 / cases.len()).max(1);

        // Phase 1: decode-only.
        let t = Instant::now();
        for _ in 0..reps {
            for c in &cases {
                black_box(decode(black_box(&c.regs)));
            }
        }
        let decode_ns = t.elapsed().as_nanos() as f64 / (reps * cases.len()) as f64;

        // Phase 2: exec-only (clone template + RTC).
        let t = Instant::now();
        for _ in 0..reps {
            for (c, tmpl) in cases.iter().zip(&templates) {
                let mut regs = c.regs.clone();
                let mut st = tmpl.clone();
                black_box(st.run_to_completion(&mut regs, &mut bus));
                black_box(&regs);
            }
        }
        let exec_ns = t.elapsed().as_nanos() as f64 / (reps * cases.len()) as f64;

        // Phase 3: full (decode + RTC) — the real per-instruction interpreter cost.
        let t = Instant::now();
        for _ in 0..reps {
            for c in &cases {
                let mut regs = c.regs.clone();
                let mut st = decode(&regs);
                black_box(st.run_to_completion(&mut regs, &mut bus));
                black_box(&regs);
            }
        }
        let full_ns = t.elapsed().as_nanos() as f64 / (reps * cases.len()) as f64;

        println!(
            "{label}: {} cases, {reps} reps\n  \
             decode-only : {decode_ns:6.2} ns/instr\n  \
             exec-only   : {exec_ns:6.2} ns/instr  (RTC over pre-decoded template)\n  \
             full        : {full_ns:6.2} ns/instr  ({:.1} M instr/s)",
            cases.len(),
            1000.0 / full_ns,
        );
    }
}
