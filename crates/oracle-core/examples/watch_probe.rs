//! Watchpoint-probe dev tool — the "who wrote this?" primitive against a real ROM. Loads a ROM **file**,
//! registers a recording [`Watchpoints`] on a work-RAM address (or any bus address), runs it for N frames on a
//! real [`System`] with the watchpoints attached as the bus-event sink, and prints the hit log: every access
//! that touched the watched range, attributed to the instruction (PC) and master (CPU vs DMA, via the function
//! code) that drove it, with the value and frame.
//!
//! This is a **dev tool, not a gate artifact** — nothing in CI depends on the ROM existing. A missing or
//! unreadable ROM path is a plain error, not a panic backtrace (the same convention as `boot_rom`).
//!
//! Usage: `cargo run --release --example watch_probe -- <rom.bin> [addr_hex] [frames] [op]`
//! - `addr_hex`  the word address to watch (default `FF0000`); the watch covers that word (`addr..=addr+1`)
//! - `frames`    how many frames to run (default `60`)
//! - `op`        `write` (default), `read`, or `any`
//!
//! Example: `cargo run --release --example watch_probe -- s4.bin FFF700 120 write`

use oracle_core::bus::BusOp;
use oracle_core::system::System;
use oracle_core::watchpoints::{WatchOp, Watchpoints};

/// A readable master tag from the 68000 function code (5/6 = supervisor data/program → CPU; 0 = DMA / another
/// master; 1/2 = user data/program → CPU).
fn master(fc: u8) -> &'static str {
    if fc == 0 {
        "DMA/other"
    } else {
        "CPU"
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(rom_path) = args.next() else {
        eprintln!("usage: watch_probe <rom.bin> [addr_hex] [frames] [op:write|read|any]");
        std::process::exit(2);
    };
    let addr = args
        .next()
        .and_then(|s| {
            u32::from_str_radix(s.trim_start_matches("0x").trim_start_matches("$"), 16).ok()
        })
        .unwrap_or(0x00FF_0000);
    let frames: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(60);
    let op = match args.next().as_deref() {
        Some("read") => WatchOp::Read,
        Some("any") => WatchOp::Any,
        _ => WatchOp::Write,
    };

    let rom = match std::fs::read(&rom_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("cannot read ROM {rom_path}: {e}");
            std::process::exit(1);
        }
    };
    println!("ROM {rom_path}: {} bytes", rom.len());
    println!(
        "watching ${addr:06X}..=${:06X} for {op:?} over {frames} frames",
        addr + 1
    );

    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();

    let mut wp = Watchpoints::new(4096);
    wp.add_watch(addr..=addr + 1, op, "probe");
    sys.run_frames_with_sink(frames, &mut wp);

    let hits = wp.hits();
    println!("{} hit(s), {} dropped", hits.len(), wp.dropped());

    // The first accesses, in order.
    for h in hits.iter().take(24) {
        let m = if h.op == BusOp::Write { '←' } else { '→' };
        println!(
            "  #{:<4} f{:<3} ${:06X} {m} {:0width$X} by PC ${:06X} ({}, fc={}) [{:?}]",
            h.seq,
            h.frame,
            h.addr,
            h.value,
            h.pc,
            master(h.fc),
            h.fc,
            h.op,
            width = h.size.bytes() as usize * 2,
        );
    }
    if hits.len() > 24 {
        println!("  … {} more", hits.len() - 24);
    }

    // Which instructions touched this address (the headline "who wrote this?" answer).
    let mut writers: Vec<u32> = hits.iter().map(|h| h.pc).collect();
    writers.sort_unstable();
    writers.dedup();
    println!(
        "distinct driving PCs: {}",
        writers
            .iter()
            .map(|p| format!("${p:06X}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
}
