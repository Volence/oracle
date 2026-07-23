//! Phase-1 root-cause diagnostic (sound-driver silence): instrument the 68k↔Z80-RAM channel and the
//! FM/PSG init write stream for `s4.soundtest.bin`.
//!
//! Pure instrumentation — boots a ROM *file*, runs N frames with a caller-owned [`BusEventSink`], and
//! reports (1) every 68k→Z80-RAM-window write (`$A00000-$A0FFFF`, where the SMPS sound queue lives), with
//! frame/addr/value/size/fc; (2) an fc-attribution tally of the FM (`$4000-$4003`/`$A04000-$A04003`) and PSG
//! (`$7F11`/`$C00011`) writes (fc 0 = Z80 master, 5/6 = 68000 master); (3) the first 64 bytes of Z80 RAM at
//! the end of the run. Touches no `src/` state; uses only the public API.
//!
//! Usage: `cargo run --release -p oracle-core --example diag_soundqueue -- [rom.bin] [frames]`

use oracle_core::bus::{BusEvent, BusEventSink, BusOp, Size};
use oracle_core::system::System;
use std::collections::BTreeMap;

const DEFAULT_ROM: &str = "/home/volence/sonic_hacks/aeon/s4.soundtest.bin";
const DEFAULT_FRAMES: u64 = 600;

/// A captured write of interest.
struct Rec {
    frame: u64,
    op: BusOp,
    addr: u32,
    value: u32,
    size: Size,
    fc: u8,
}

#[derive(Default)]
struct Diag {
    frame: u64,
    /// Writes to the Z80-RAM window `$A00000-$A0FFFF` (the SMPS sound-queue window), EXCLUDING the FM ports
    /// `$A04000-$A04003` which alias inside that window but are the YM2612, not Z80 RAM.
    z80_ram_writes: Vec<Rec>,
    /// FM register writes, both windows (`$4000-$4003` Z80, `$A04000-$A04003` 68k).
    fm_writes: Vec<Rec>,
    /// PSG writes, both windows (`$7F11` Z80, `$C00011` 68k).
    psg_writes: Vec<Rec>,
    /// fc tallies.
    fm_fc: BTreeMap<u8, u64>,
    psg_fc: BTreeMap<u8, u64>,
    z80_ram_fc: BTreeMap<u8, u64>,
}

fn is_fm(a: u32) -> bool {
    matches!(a, 0x4000..=0x4003 | 0xA0_4000..=0xA0_4003)
}
fn is_psg(a: u32) -> bool {
    matches!(a, 0x7F11 | 0xC0_0011)
}

impl BusEventSink for Diag {
    fn on_step_boundary(&mut self, _pc: u32, frame: u64) {
        self.frame = frame;
    }

    fn on_event(&mut self, e: BusEvent) {
        if e.op != BusOp::Write {
            return;
        }
        let rec = || Rec {
            frame: self.frame,
            op: e.op,
            addr: e.addr,
            value: e.value,
            size: e.size,
            fc: e.fc,
        };
        if is_fm(e.addr) {
            *self.fm_fc.entry(e.fc).or_default() += 1;
            self.fm_writes.push(rec());
        } else if is_psg(e.addr) {
            *self.psg_fc.entry(e.fc).or_default() += 1;
            self.psg_writes.push(rec());
        } else if (0xA0_0000..=0xA0_FFFF).contains(&e.addr) {
            // The Z80-RAM window proper (FM ports already peeled off above): the sound queue lives here.
            *self.z80_ram_fc.entry(e.fc).or_default() += 1;
            self.z80_ram_writes.push(rec());
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let rom_path = args.next().unwrap_or_else(|| DEFAULT_ROM.to_string());
    let frames: u64 = args
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_FRAMES);

    let rom = match std::fs::read(&rom_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("cannot read ROM {rom_path}: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "ROM {rom_path}: {} bytes, running {frames} frames",
        rom.len()
    );

    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();

    let mut diag = Diag::default();
    sys.run_frames_with_sink(frames, &mut diag);

    // ---- 68k -> Z80-RAM-window writes ------------------------------------------------------------------
    println!("\n=== 68k->Z80-RAM-window writes ($A00000-$A0FFFF, FM ports excluded) ===");
    println!("count: {}", diag.z80_ram_writes.len());
    println!("fc tally: {:?}", diag.z80_ram_fc);
    for r in &diag.z80_ram_writes {
        let nz = if r.value != 0 { "  <== NON-ZERO" } else { "" };
        println!(
            "  frame={:4} {:?} addr={:06X} size={:?} value={:04X} fc={}{}",
            r.frame, r.op, r.addr, r.size, r.value, r.fc, nz
        );
    }

    // ---- FM fc attribution -----------------------------------------------------------------------------
    println!("\n=== FM writes ($4000-$4003 / $A04000-$A04003) ===");
    println!(
        "count: {}   fc tally: {:?}",
        diag.fm_writes.len(),
        diag.fm_fc
    );
    let show = diag.fm_writes.len().min(32);
    for r in &diag.fm_writes[..show] {
        println!(
            "  frame={:4} addr={:06X} size={:?} value={:02X} fc={}",
            r.frame, r.addr, r.size, r.value, r.fc
        );
    }
    if diag.fm_writes.len() > show {
        println!("  ... ({} more)", diag.fm_writes.len() - show);
    }

    // ---- PSG fc attribution ----------------------------------------------------------------------------
    println!("\n=== PSG writes ($7F11 / $C00011) ===");
    println!(
        "count: {}   fc tally: {:?}",
        diag.psg_writes.len(),
        diag.psg_fc
    );
    let show = diag.psg_writes.len().min(32);
    for r in &diag.psg_writes[..show] {
        println!(
            "  frame={:4} addr={:06X} size={:?} value={:02X} fc={}",
            r.frame, r.addr, r.size, r.value, r.fc
        );
    }
    if diag.psg_writes.len() > show {
        println!("  ... ({} more)", diag.psg_writes.len() - show);
    }

    // ---- Z80 RAM dump ----------------------------------------------------------------------------------
    let z80 = sys.z80_ram();
    println!("\n=== Z80 RAM first 64 bytes at end of run ===");
    for (i, chunk) in z80[..64.min(z80.len())].chunks(16).enumerate() {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02X}")).collect();
        println!("  {:04X}: {}", i * 16, hex.join(" "));
    }
    // The SMPS mailbox lives at Z80 offset $1F00 (68k $A01F00): ping/$1F00, sample/$1F01, music/$1F02,
    // sfx/$1F03. A non-zero music byte at end = the driver NEVER consumed the queued song.
    println!("\n=== SMPS mailbox at end of run (Z80 $1F00-$1F07) ===");
    for (off, &b) in z80[0x1F00..=0x1F07].iter().enumerate() {
        let i = 0x1F00 + off;
        let label = match i {
            0x1F00 => " (SND_REQ_PING)",
            0x1F01 => " (SND_REQ_SAMPLE)",
            0x1F02 => " (SND_REQ_MUSIC)",
            0x1F03 => " (SND_REQ_SFX)",
            _ => "",
        };
        println!("  z80_ram[{i:04X}] = {b:02X}{label}");
    }

    // Scan the whole 8 KiB for any non-zero byte (the queue may sit anywhere).
    let nz: Vec<(usize, u8)> = z80
        .iter()
        .enumerate()
        .filter(|(_, &b)| b != 0)
        .map(|(i, &b)| (i, b))
        .collect();
    println!("\nZ80 RAM non-zero bytes: {} total", nz.len());
    for (i, b) in nz.iter().take(64) {
        println!("  z80_ram[{i:04X}] = {b:02X}");
    }
    if nz.len() > 64 {
        println!("  ... ({} more)", nz.len() - 64);
    }
}
