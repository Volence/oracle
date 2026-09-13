//! **SPIKE — H22 design parcel (2026-09-13). NOT PROPOSED FOR MERGE.**
//!
//! The measurement half of `docs/2026-09-13-h22-decode-design.md`. Needs `--features h22-spike` (see
//! `src/m68000/h22_spike.rs`). Five phases:
//!
//! 1. **Purity probe** over all 2 × 65 536 `(opcode, S)` keys: which register-file fields change the recipe.
//!    Checks the probe's impure set against the spike table's static classifier (a miss = a wrong table).
//! 2. **Table cost**: size, eager build time, distinct-recipe count.
//! 3. **Per-family decode cost** of single opcodes (early arm vs late arm) and the materialization floor.
//! 4. **Real ROM**: record the opcode mix, verify table == cascade on the live registers, then time N
//!    frames in cascade / table / double-decode modes from one cloned boot state, rotating mode order.
//! 5. **Tight loop** over the recorded real register files: ns per decode, cascade vs table.
//!
//! Usage: `cargo run --release --features h22-spike --example h22_decode_spike -- \
//!          [--rom PATH] [--warm N] [--frames N] [--reps N] [--skip-probe]`

use oracle_core::io::{Pad, PadPort};
use oracle_core::m68000::decode::decode;
use oracle_core::m68000::decode::h22_spike as spike;
use oracle_core::m68000::ea::RecipeBuf;
use oracle_core::m68000::microop::{MicroOp, MicroState};
use oracle_core::m68000::registers::Registers;
use oracle_core::system::System;
use std::collections::{BTreeMap, HashSet};
use std::hint::black_box;
use std::time::{Duration, Instant};

#[path = "common/rom_source.rs"]
mod rom_source;

/// A crude family name for an opcode, for reading histograms. Not a decoder: first match wins.
fn family(op: u16) -> &'static str {
    let hi = op >> 12;
    let mode = (op >> 3) & 7;
    match op {
        0x4E71 => return "NOP",
        0x4E75 => return "RTS",
        0x4E73 => return "RTE",
        0x4E77 => return "RTR",
        0x4E76 => return "TRAPV",
        _ => {}
    }
    if op & 0xFFC0 == 0x4E80 {
        return "JSR";
    }
    if op & 0xFFC0 == 0x4EC0 {
        return "JMP";
    }
    if op & 0xF1C0 == 0x41C0 {
        return "LEA";
    }
    if op & 0xFFC0 == 0x4840 && mode != 0 {
        return "PEA";
    }
    if op & 0xFFF8 == 0x4840 {
        return "SWAP";
    }
    if op & 0xFFB8 == 0x4880 {
        return "EXT";
    }
    if op & 0xFB80 == 0x4880 {
        return "MOVEM";
    }
    if op & 0xFF00 == 0x4A00 && (op >> 6) & 3 != 3 {
        return "TST";
    }
    if op & 0xFF00 == 0x4200 {
        return "CLR";
    }
    if op & 0xFF00 == 0x4400 {
        return "NEG";
    }
    if op & 0xFF00 == 0x4600 {
        return "NOT";
    }
    match hi {
        0x0 => {
            if op & 0xF138 == 0x0108 {
                "MOVEP"
            } else if op & 0x0100 != 0 || op & 0xFF00 == 0x0800 {
                "BIT"
            } else {
                "IMM"
            }
        }
        0x1 => "MOVE.b",
        0x2 | 0x3 => {
            if (op >> 6) & 7 == 1 {
                "MOVEA"
            } else if hi == 2 {
                "MOVE.l"
            } else {
                "MOVE.w"
            }
        }
        0x4 => "misc4",
        0x5 => {
            if op & 0xF0F8 == 0x50C8 {
                "DBcc"
            } else if op & 0xF0C0 == 0x50C0 {
                "Scc"
            } else {
                "ADDQ/SUBQ"
            }
        }
        0x6 => {
            if (op >> 8) & 0xF == 1 {
                "BSR"
            } else if (op >> 8) & 0xF == 0 {
                "BRA"
            } else {
                "Bcc"
            }
        }
        0x7 => "MOVEQ",
        0x8 => "OR/DIV",
        0x9 => "SUB",
        0xB => "CMP/EOR",
        0xC => "AND/MUL",
        0xD => "ADD",
        0xE => {
            if (op >> 6) & 3 == 3 {
                "SHIFT-mem"
            } else if op & 0x20 != 0 {
                "SHIFT-Dn-count"
            } else {
                "SHIFT-imm"
            }
        }
        _ => "LINE-A/F",
    }
}

fn pad_for(frame: u64) -> Pad {
    // Deterministic crude "play": Start taps through the first 1200 frames (title → game), then hold Right
    // and jump (A) for 10 frames of every 90.
    let mut p = Pad::default();
    if frame < 1200 {
        p.start = frame % 60 < 3;
    } else {
        p.right = true;
        p.a = frame % 90 < 10;
    }
    p
}

fn run(sys: &mut System, start: u64, frames: u64) {
    for f in start..start + frames {
        sys.set_pad(PadPort::P1, pad_for(f));
        sys.run_frames(1);
    }
}

fn stats(v: &[Duration]) -> (f64, f64, f64) {
    let mut ms: Vec<f64> = v.iter().map(|d| d.as_secs_f64() * 1e3).collect();
    ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (ms[0], ms[ms.len() / 2], ms[ms.len() - 1])
}

fn time_decode(regs: &[Registers], reps: usize) -> f64 {
    let t = Instant::now();
    for _ in 0..reps {
        for r in regs {
            black_box(decode(black_box(r)));
        }
    }
    t.elapsed().as_nanos() as f64 / (reps * regs.len()) as f64
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let get = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let rom_path = get("--rom").unwrap_or_else(|| rom_source::frozen("s4.debug.bin"));
    let warm: u64 = get("--warm").and_then(|s| s.parse().ok()).unwrap_or(1800);
    let frames: u64 = get("--frames").and_then(|s| s.parse().ok()).unwrap_or(600);
    let reps: usize = get("--reps").and_then(|s| s.parse().ok()).unwrap_or(5);
    let skip_probe = args.iter().any(|a| a == "--skip-probe");
    let probe_only = args.iter().any(|a| a == "--probe-only");

    println!("== sizes");
    println!(
        "  MicroOp {} B, MicroState {} B, Option<MicroState> {} B, Registers {} B",
        std::mem::size_of::<MicroOp>(),
        std::mem::size_of::<MicroState>(),
        std::mem::size_of::<Option<MicroState>>(),
        std::mem::size_of::<Registers>()
    );

    // ---- Phase 1: purity probe -------------------------------------------------------------------------
    let mut probe_mask = vec![0u32; spike::KEYS];
    if !skip_probe {
        println!("== phase 1: purity probe over {} keys", spike::KEYS);
        let t = Instant::now();
        let mut field_counts = [0u32; spike::NFIELDS];
        let mut joint = 0u32;
        let (mut probe_impure, mut static_impure_n, mut static_extra) = (0u32, 0u32, 0u32);
        let mut violations: Vec<(u16, bool, u32)> = Vec::new();
        let mut fam_dep: BTreeMap<&str, (u32, u32)> = BTreeMap::new(); // S=1 half: family → (impure, total)
        for key in 0..spike::KEYS {
            let op = key as u16;
            let s = key >> 16 == 1;
            let dep = spike::probe_key(op, s);
            probe_mask[key] = dep;
            for (f, c) in field_counts.iter_mut().enumerate() {
                if dep >> f & 1 == 1 {
                    *c += 1;
                }
            }
            if dep & spike::JOINT_BIT != 0 {
                joint += 1;
            }
            let st = spike::static_impure(op);
            static_impure_n += st as u32;
            if dep != 0 {
                probe_impure += 1;
                if !st {
                    violations.push((op, s, dep));
                }
            } else if st {
                static_extra += 1;
            }
            if s {
                let e = fam_dep.entry(family(op)).or_default();
                e.1 += 1;
                if dep != 0 {
                    e.0 += 1;
                }
            }
        }
        let mut s_sensitive = 0u32;
        let (mut privileged_n, mut s_vs_priv_mismatch) = (0u32, 0u32);
        for op in 0..=0xFFFFu16 {
            let differs = spike::decode_canonical(op, false) != spike::decode_canonical(op, true);
            s_sensitive += differs as u32;
            privileged_n += spike::is_privileged(op) as u32;
            if differs != spike::is_privileged(op) {
                s_vs_priv_mismatch += 1;
            }
        }
        println!(
            "  is_privileged_opcode count {privileged_n}; opcodes where 'S changes the recipe' != 'privileged': {s_vs_priv_mismatch}"
        );
        println!("  probe time {:.2} s", t.elapsed().as_secs_f64());
        println!("  keys whose recipe depends on a field (out of {} keys, both S halves):", spike::KEYS);
        for (f, c) in field_counts.iter().enumerate() {
            println!("    {:>10}: {}", spike::FIELD_NAMES[f], c);
        }
        println!("    {:>10}: {}", "joint-only", joint);
        println!("  probe-impure keys: {probe_impure}  static-classifier-impure keys: {static_impure_n}  static-only (conservative extras): {static_extra}");
        println!("  opcodes whose canonical recipe differs between S=0 and S=1 (the privilege gate): {s_sensitive}");
        println!("  VIOLATIONS (probe-impure but static says pure — a table built from the classifier would be WRONG here): {}", violations.len());
        for (op, s, dep) in violations.iter().take(20) {
            let names: Vec<&str> = (0..spike::NFIELDS)
                .filter(|f| dep >> f & 1 == 1)
                .map(|f| spike::FIELD_NAMES[f])
                .collect();
            println!("    {op:#06x} S={} {} deps={names:?} joint={}", *s as u8, family(*op), dep & spike::JOINT_BIT != 0);
        }
        println!("  S=1 half, by crude family: impure / total");
        for (fam, (imp, tot)) in &fam_dep {
            if *imp > 0 {
                println!("    {fam:>15}: {imp:>5} / {tot}");
            }
        }
    }

    if probe_only {
        return;
    }

    // ---- Phase 2: table cost ---------------------------------------------------------------------------
    println!("== phase 2: eager table");
    let t = Instant::now();
    let filled = spike::force_table();
    let build = t.elapsed();
    let bytes = spike::KEYS * std::mem::size_of::<Option<MicroState>>();
    println!(
        "  cold build {:.1} ms, {filled} of {} keys hold a recipe, {:.1} MiB ({} B/entry)",
        build.as_secs_f64() * 1e3,
        spike::KEYS,
        bytes as f64 / (1024.0 * 1024.0),
        std::mem::size_of::<Option<MicroState>>()
    );
    // Cascade-only cost of the same work (no allocation, no page faults): decode every pure key once.
    let t = Instant::now();
    for k in 0..spike::KEYS {
        let op = k as u16;
        if !spike::static_impure(op) {
            black_box(spike::decode_canonical(op, k >> 16 == 1));
        }
    }
    println!("  decode-only cost of the same {filled} keys: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    let t = Instant::now();
    let mut distinct: HashSet<String> = HashSet::new();
    for k in 0..spike::KEYS {
        let op = k as u16;
        if !spike::static_impure(op) {
            distinct.insert(format!("{:?}", spike::dispatch_canonical(op, k >> 16 == 1)));
        }
    }
    println!(
        "  distinct pre-latch recipes among them: {} ({:.1} s to count)",
        distinct.len(),
        t.elapsed().as_secs_f64()
    );

    // ---- Phase 3: per-opcode decode cost (arm position) ------------------------------------------------
    println!("== phase 3: single-opcode decode cost (cascade), materialization floor");
    spike::set_mode(spike::MODE_CASCADE);
    let singles: [(&str, u16); 12] = [
        ("MOVE.w D0,D1  (arm 1)", 0x3200),
        ("MOVE.l (A0)+,D1 (arm 3)", 0x2218),
        ("ADD.w D0,D1", 0xD240),
        ("LEA (A0),A1", 0x43D0),
        ("TST.w D0", 0x4A40),
        ("RTS", 0x4E75),
        ("BNE.s", 0x6604),
        ("DBF D0", 0x51C8),
        ("ADDQ.w #1,D0", 0x5240),
        ("MOVEQ #1,D0  (arm 101)", 0x7001),
        ("LSL.w #1,D0", 0xE348),
        ("ILLEGAL 0x4AFC", 0x4AFC),
    ];
    let reps1 = 2_000_000;
    for (name, op) in singles {
        let r = vec![spike::canonical_regs(op, true); 1];
        let ns = time_decode(&r, reps1);
        println!("  {name:>26} {op:#06x}: {ns:6.2} ns/decode");
    }
    let one = [MicroOp::Prefetch];
    let t = Instant::now();
    for _ in 0..reps1 * 4 {
        black_box(MicroState::from_ops(black_box(&one)));
    }
    println!(
        "  materialization floor MicroState::from_ops(&[Prefetch]): {:.2} ns",
        t.elapsed().as_nanos() as f64 / (reps1 * 4) as f64
    );
    let tmpl = spike::decode_canonical(0x3200, true);
    let t = Instant::now();
    for _ in 0..reps1 * 4 {
        black_box(black_box(&tmpl).clone());
    }
    println!(
        "  MicroState clone (the table's per-lookup copy): {:.2} ns",
        t.elapsed().as_nanos() as f64 / (reps1 * 4) as f64
    );
    // The real decode floor: RecipeBuf::new + one push + finish, with the repeat-expression filler and with
    // the static-copy filler.
    for ff in [false, true] {
        spike::set_fast_fill(ff);
        let t = Instant::now();
        for _ in 0..reps1 * 4 {
            let mut b = RecipeBuf::new();
            b.push(MicroOp::Prefetch);
            black_box(b.finish());
        }
        println!(
            "  RecipeBuf::new + push(Prefetch) + finish, fast_fill={ff}: {:.2} ns",
            t.elapsed().as_nanos() as f64 / (reps1 * 4) as f64
        );
    }
    spike::set_fast_fill(false);
    // The arm walk's own cost: decode() vs the same builder called directly.
    for ff in [false, true] {
        spike::set_fast_fill(ff);
        for (name, op) in [
            ("MOVE.w D0,D1 (arm 1)", 0x3200u16),
            ("TST.b abs.w", 0x4A38),
            ("BEQ.s -6 (not taken)", 0x67FA),
            ("DBF D1", 0x51C9),
            ("RTS", 0x4E75),
            ("MOVEQ #1,D0 (arm 101)", 0x7001),
        ] {
            let r = spike::canonical_regs(op, true);
            let via = time_decode(std::slice::from_ref(&r), reps1);
            let t = Instant::now();
            for _ in 0..reps1 {
                black_box(spike::builder_direct(black_box(&r)));
            }
            let direct = t.elapsed().as_nanos() as f64 / reps1 as f64;
            println!(
                "  fast_fill={ff} {name:>22}: decode {via:6.2} ns, builder direct {direct:6.2} ns, walk {:+6.2} ns",
                via - direct
            );
        }
    }
    spike::set_fast_fill(false);

    // ---- Phase 4: real ROM ------------------------------------------------------------------------------
    println!("== phase 4: real ROM {rom_path}, warm {warm} frames, then {frames} frames x {reps} reps");
    let rom = std::fs::read(&rom_path).expect("read ROM");
    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();
    spike::set_mode(spike::MODE_CASCADE);
    run(&mut sys, 0, warm);
    let s0 = sys.clone();
    println!("  boot state after warm-up: pc {:08X} sr {:04X}", s0.cpu_regs().pc, s0.cpu_regs().sr);

    spike::reset_record(8, 2_000_000);
    spike::set_mode(spike::MODE_RECORD);
    let mut s = s0.clone();
    run(&mut s, warm, frames);
    let hash_record = s.export_state_hash();
    let rec = spike::take_record();
    let touched = rec.hist.iter().filter(|&&c| c > 0).count();
    let user: u64 = rec.hist[..65536].iter().sum();
    let static_imp: u64 = (0..spike::KEYS)
        .filter(|&k| spike::static_impure(k as u16))
        .map(|k| rec.hist[k])
        .sum();
    let probe_imp: u64 = if skip_probe {
        0
    } else {
        (0..spike::KEYS).filter(|&k| probe_mask[k] != 0).map(|k| rec.hist[k]).sum()
    };
    let pct = |x: u64| 100.0 * x as f64 / rec.calls as f64;
    println!(
        "  decode calls {} ({:.0}/frame), distinct keys touched {touched}, user-mode calls {:.3}%",
        rec.calls,
        rec.calls as f64 / frames as f64,
        pct(user)
    );
    println!(
        "  calls on static-impure keys (cascade fallback in the table design): {:.2}%   on probe-impure keys: {}",
        pct(static_imp),
        if skip_probe { "n/a".to_string() } else { format!("{:.2}%", pct(probe_imp)) }
    );
    let mut by_fam: BTreeMap<&str, u64> = BTreeMap::new();
    for (k, &c) in rec.hist.iter().enumerate() {
        if c > 0 {
            *by_fam.entry(family(k as u16)).or_default() += c;
        }
    }
    let mut fams: Vec<_> = by_fam.into_iter().collect();
    fams.sort_by(|a, b| b.1.cmp(&a.1));
    println!("  executed mix by crude family:");
    for (fam, c) in fams.iter().take(24) {
        println!("    {fam:>15}: {:6.2}%", pct(*c));
    }
    let mut top: Vec<(usize, u64)> = rec.hist.iter().copied().enumerate().filter(|x| x.1 > 0).collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    println!("  top 20 keys:");
    for (k, c) in top.iter().take(20) {
        println!(
            "    S={} {:#06x} {:>15} {:6.2}%  static_impure={}",
            k >> 16,
            *k as u16,
            family(*k as u16),
            pct(*c),
            spike::static_impure(*k as u16)
        );
    }

    spike::set_mode(spike::MODE_TABLE_CHECKED);
    let _ = spike::take_checked();
    let mut s = s0.clone();
    run(&mut s, warm, frames);
    let checked = spike::take_checked();
    let hash_checked = s.export_state_hash();
    println!(
        "  TABLE_CHECKED: {checked} table lookups verified == cascade on the live registers; hash {:#018x} vs record-run {:#018x} ({})",
        hash_checked,
        hash_record,
        if hash_checked == hash_record { "equal" } else { "DIFFERENT" }
    );

    let modes = [
        ("cascade", spike::MODE_CASCADE, false),
        ("cascade+fastfill", spike::MODE_CASCADE, true),
        ("table", spike::MODE_TABLE, false),
        ("table+fastfill", spike::MODE_TABLE, true),
        ("lazy", spike::MODE_LAZY, false),
        ("lazy+fastfill", spike::MODE_LAZY, true),
        ("double", spike::MODE_DOUBLE, false),
    ];
    let mut times: Vec<Vec<Duration>> = vec![Vec::new(); modes.len()];
    let mut hashes: Vec<u64> = Vec::new();
    for rep in 0..reps {
        for i in 0..modes.len() {
            let idx = (i + rep) % modes.len();
            spike::set_mode(modes[idx].1);
            spike::set_fast_fill(modes[idx].2);
            let mut s = s0.clone();
            let t = Instant::now();
            run(&mut s, warm, frames);
            times[idx].push(t.elapsed());
            hashes.push(s.export_state_hash());
        }
    }
    let all_equal = hashes.iter().all(|&h| h == hash_record);
    let (_, base_med, _) = stats(&times[0]);
    spike::set_fast_fill(false);
    for (i, (name, _, _)) in modes.iter().enumerate() {
        let (lo, med, hi) = stats(&times[i]);
        println!(
            "  {name:>17}: median {med:8.1} ms ({:.3} ms/frame)  min {lo:.1}  max {hi:.1}  vs cascade {:+.2}%",
            med / frames as f64,
            100.0 * (med - base_med) / base_med
        );
    }
    println!("  every timed run ended at the record-run export_state_hash: {all_equal}");
    let lazy_n = spike::lazy_filled();
    println!(
        "  lazy memo: {lazy_n} keys filled; storage {:.2} MiB of OnceLock slots ({} B each) + {:.2} MiB of recipes",
        (spike::KEYS * std::mem::size_of::<std::sync::OnceLock<Option<Box<MicroState>>>>()) as f64
            / (1024.0 * 1024.0),
        std::mem::size_of::<std::sync::OnceLock<Option<Box<MicroState>>>>(),
        (lazy_n * std::mem::size_of::<MicroState>()) as f64 / (1024.0 * 1024.0)
    );

    // ---- Phase 5: tight loop over the recorded real register files -------------------------------------
    let samples = rec.samples;
    println!("== phase 5: tight loop over {} recorded real decode inputs", samples.len());
    let reps5 = (40_000_000 / samples.len().max(1)).max(1);
    for (name, m, ff) in [
        ("cascade", spike::MODE_CASCADE, false),
        ("cascade+fastfill", spike::MODE_CASCADE, true),
        ("table", spike::MODE_TABLE, false),
        ("table+fastfill", spike::MODE_TABLE, true),
        ("lazy", spike::MODE_LAZY, false),
        ("lazy+fastfill", spike::MODE_LAZY, true),
    ] {
        spike::set_mode(m);
        spike::set_fast_fill(ff);
        let _ = time_decode(&samples, 1); // warm
        let ns = time_decode(&samples, reps5);
        println!("  {name:>17}: {ns:6.2} ns/decode");
    }
    spike::set_mode(spike::MODE_CASCADE);
    spike::set_fast_fill(false);
}
