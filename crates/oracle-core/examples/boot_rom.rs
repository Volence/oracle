//! Real-ROM boot runner — the s4.bin first-scene milestone's rung-walking tool
//! (`docs/plans/2026-07-21-s4-boot-milestone.md`). Loads a ROM **file** (a build artifact, never committed),
//! runs it for N frames on a real [`System`], renders the settled frame to a binary PPM, and dumps every
//! memory region the checkpoint ladder byte-diffs against Oracle's ground truth: VRAM, CRAM, VSRAM, work
//! RAM, Z80 RAM, the VDP register file, and the 68000 architectural registers.
//!
//! This is a **dev tool, not a gate artifact** — nothing in CI depends on the ROM existing. A missing or
//! unreadable ROM path is a plain error, not a panic backtrace.
//!
//! Usage: `cargo run --release --example boot_rom -- <rom.bin> [frames] [out_prefix]`
//! (defaults: 600 frames — ten seconds, comfortably past the boot path — and prefix `boot`).
//! Writes `<prefix>.ppm`, `<prefix>.vram.bin`, `<prefix>.cram.bin`, `<prefix>.vsram.bin`, `<prefix>.ram.bin`,
//! `<prefix>.z80.bin`, `<prefix>.regs.txt`.

use oracle_core::system::System;
use std::io::Write;

/// Write an RGB framebuffer (`width × height`, row-major) as a binary PPM (P6).
fn write_ppm(path: &str, width: usize, height: usize, rgb: &[(u8, u8, u8)]) -> std::io::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    write!(f, "P6\n{width} {height}\n255\n")?;
    let mut bytes = Vec::with_capacity(rgb.len() * 3);
    for &(r, g, b) in rgb {
        bytes.extend_from_slice(&[r, g, b]);
    }
    f.write_all(&bytes)?;
    f.flush()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(rom_path) = args.next() else {
        eprintln!("usage: boot_rom <rom.bin> [frames] [out_prefix]");
        std::process::exit(2);
    };
    let frames: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(600);
    let prefix = args.next().unwrap_or_else(|| "boot".to_string());

    let rom = match std::fs::read(&rom_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("cannot read ROM {rom_path}: {e}");
            std::process::exit(1);
        }
    };
    println!("ROM {rom_path}: {} bytes", rom.len());

    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();
    sys.run_frames(frames);

    // Render the active display (224 lines) through the pure renderer.
    let height = 224usize;
    let width = sys.vdp().render_line(0).len();
    let mut fb = Vec::with_capacity(width * height);
    for line in 0..height as u16 {
        fb.extend_from_slice(&sys.vdp().render_line(line));
    }
    write_ppm(&format!("{prefix}.ppm"), width, height, &fb).expect("write ppm");

    // Memory-region dumps for the ladder's byte-diffs.
    for (name, bytes) in [
        ("vram", sys.vram()),
        ("cram", sys.vdp().cram()),
        ("vsram", sys.vdp().vsram()),
        ("ram", sys.ram()),
        ("z80", sys.z80_ram()),
    ] {
        std::fs::write(format!("{prefix}.{name}.bin"), bytes).expect("write dump");
    }

    // Register file + CPU state, human-readable (diffed by eye / grep, not byte-compared).
    let regs = sys.cpu_regs();
    let mut txt = String::new();
    txt.push_str(&format!(
        "pc {:08X}\nsr {:04X}\nusp {:08X}\nssp {:08X}\n",
        regs.pc, regs.sr, regs.usp, regs.ssp
    ));
    for (i, d) in regs.d.iter().enumerate() {
        txt.push_str(&format!("d{i} {d:08X}\n"));
    }
    for (i, a) in regs.a.iter().enumerate() {
        txt.push_str(&format!("a{i} {a:08X}\n"));
    }
    for (i, r) in sys.vdp().regs().iter().enumerate() {
        txt.push_str(&format!("vdp r{i:02} {r:02X}\n"));
    }
    std::fs::write(format!("{prefix}.regs.txt"), txt).expect("write regs");

    println!("wrote {width}x{height} {prefix}.ppm + vram/cram/vsram/ram/z80 dumps + {prefix}.regs.txt (after {frames} frames)");
}
