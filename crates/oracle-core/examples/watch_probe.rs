//! Watchpoint-probe dev tool — the "who wrote this?" primitive against a real ROM. Loads a ROM **file**,
//! registers a recording [`Watchpoints`] on an address, runs it for N frames on a real [`System`] with the
//! watchpoints attached as the sink, and prints the hit log: every access that touched the watched range,
//! attributed to the instruction (PC) and master that drove it, with the value and frame.
//!
//! Two spaces (watchpoints v1 + v2):
//! - `--space bus` (default): the **68000 bus** address space (work RAM, ROM, Z80 RAM, I/O, VDP ports). A hit
//!   shows the bus value + the master (CPU vs DMA, via the function code).
//! - `--space vram|cram|vsram`: a **VDP-internal** byte address — the "who wrote this tile / palette entry?"
//!   case. A hit shows old→new + whether the write came Direct from the CPU or via DMA.
//!
//! This is a **dev tool, not a gate artifact** — nothing in CI depends on the ROM existing. A missing or
//! unreadable ROM path is a plain error, not a panic backtrace (the same convention as `boot_rom`).
//!
//! Usage: `cargo run --release --example watch_probe -- [--space S] <rom.bin> [addr_hex] [frames] [op]`
//! - `--space S` `bus` (default), `vram`, `cram`, or `vsram`
//! - `addr_hex`  the address to watch (default `FF0000` bus / `0000` VDP); the watch covers `addr..=addr+1`
//! - `frames`    how many frames to run (default `60`)
//! - `op`        `write` (default), `read`, or `any`
//!
//! Examples:
//! - `cargo run --release --example watch_probe -- s4.bin FFF700 120 write`
//! - `cargo run --release --example watch_probe -- --space vram s4.bin 0100 120 any`

use oracle_core::bus::BusOp;
use oracle_core::system::System;
use oracle_core::watchpoints::{WatchOp, WatchSpace, WatchVia, Watchpoints};

/// A readable master tag from the 68000 function code (5/6 = supervisor data/program → CPU; 0 = DMA / another
/// master; 1/2 = user data/program → CPU).
fn master(fc: u8) -> &'static str {
    if fc == 0 {
        "DMA/other"
    } else {
        "CPU"
    }
}

/// Parse a `--space` name into a [`WatchSpace`], defaulting to `Bus`.
fn parse_space(name: &str) -> Option<WatchSpace> {
    match name.to_ascii_lowercase().as_str() {
        "bus" => Some(WatchSpace::Bus),
        "vram" => Some(WatchSpace::Vram),
        "cram" => Some(WatchSpace::Cram),
        "vsram" => Some(WatchSpace::Vsram),
        _ => None,
    }
}

fn main() {
    // Split off any `--space S` flag, leaving the positional args (rom, addr, frames, op).
    let mut space = WatchSpace::Bus;
    let mut positional: Vec<String> = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        if a == "--space" {
            match it.next().as_deref().and_then(parse_space) {
                Some(s) => space = s,
                None => {
                    eprintln!("--space expects one of: bus|vram|cram|vsram");
                    std::process::exit(2);
                }
            }
        } else if let Some(s) = a.strip_prefix("--space=") {
            match parse_space(s) {
                Some(s) => space = s,
                None => {
                    eprintln!("--space expects one of: bus|vram|cram|vsram");
                    std::process::exit(2);
                }
            }
        } else {
            positional.push(a);
        }
    }
    let mut args = positional.into_iter();

    let Some(rom_path) = args.next() else {
        eprintln!(
            "usage: watch_probe [--space bus|vram|cram|vsram] <rom.bin> [addr_hex] [frames] [op:write|read|any]"
        );
        std::process::exit(2);
    };
    let default_addr = if space == WatchSpace::Bus {
        0x00FF_0000
    } else {
        0x0000
    };
    let addr = args
        .next()
        .and_then(|s| {
            u32::from_str_radix(s.trim_start_matches("0x").trim_start_matches("$"), 16).ok()
        })
        .unwrap_or(default_addr);
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
        "watching {space:?} ${addr:X}..=${:X} for {op:?} over {frames} frames",
        addr + 1
    );

    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();

    let mut wp = Watchpoints::new(4096);
    match space {
        WatchSpace::Bus => wp.add_watch(addr..=addr + 1, op, "probe"),
        s => wp.add_vdp_watch(s, addr..=addr + 1, op, "probe"),
    }
    sys.run_frames_with_sink(frames, &mut wp);

    let hits = wp.hits();
    println!("{} hit(s), {} dropped", hits.len(), wp.dropped());

    // The first accesses, in order.
    for h in hits.iter().take(24) {
        let width = h.size.bytes() as usize * 2;
        match h.via {
            // Bus access (v1): value + master via the function code.
            WatchVia::Bus => {
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
                );
            }
            // VDP-internal write (v2): old→new + Direct/DMA attribution.
            via => {
                let tag = if via == WatchVia::Dma { "DMA" } else { "CPU" };
                println!(
                    "  #{:<4} f{:<3} {:?} ${:04X}: {:0width$X} → {:0width$X} by PC ${:06X} ({tag})",
                    h.seq, h.frame, h.space, h.addr, h.old, h.value, h.pc,
                );
            }
        }
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
