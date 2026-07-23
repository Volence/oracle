//! VGM capture experiment (Phase RT-3, first half) — attach a [`VgmLogger`] to a real ROM run and see
//! whether the game's sound driver produces an actual YM2612 FM / SN76489 PSG register stream in our core.
//!
//! This is a **dev tool, not a gate artifact** — it boots a ROM *file* (a build artifact, never committed),
//! runs N frames with a caller-owned [`VgmLogger`] sink threaded through both CPUs via
//! [`System::run_frames_with_sink`], and reports the captured register-write counts, a sample of the decoded
//! records, and the rendered VGM. It touches no `src/` state and uses only the public API.
//!
//! Usage: `cargo run --release -p oracle-core --example vgm_capture -- [rom.bin] [frames]`
//!   - `[rom.bin]` — ROM path (default `/home/volence/sonic_hacks/aeon/s4.bin`).
//!   - `[frames]` — frames to run (default 600).

use oracle_core::system::System;
use oracle_core::vgm::VgmLogger;

const DEFAULT_ROM: &str = "/home/volence/sonic_hacks/aeon/s4.bin";
const DEFAULT_FRAMES: u64 = 600;
const OUT_VGM: &str =
    "/tmp/claude-1000/-home-volence-sonic-hacks-oracle-next/scratchpad/s4_capture.vgm";

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
    println!("ROM {rom_path}: {} bytes", rom.len());

    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();

    let mut logger = VgmLogger::new();
    sys.run_frames_with_sink(frames, &mut logger);

    let records = logger.records();
    println!("frames run:  {frames}");
    println!("fm_writes:   {}", logger.fm_writes());
    println!("psg_writes:  {}", logger.psg_writes());
    println!("records:     {}", records.len());
    println!("is_empty:    {}", logger.is_empty());

    let show_first = records.len().min(24);
    if show_first > 0 {
        println!("\nfirst {show_first} records (chip port reg value frame):");
        for r in &records[..show_first] {
            println!(
                "  {:?} port={} reg={:02X} value={:02X} frame={}",
                r.chip, r.port, r.reg, r.value, r.frame
            );
        }
    }
    if records.len() > show_first {
        let tail_start = records.len().saturating_sub(8).max(show_first);
        println!("\nlast {} records:", records.len() - tail_start);
        for r in &records[tail_start..] {
            println!(
                "  {:?} port={} reg={:02X} value={:02X} frame={}",
                r.chip, r.port, r.reg, r.value, r.frame
            );
        }
    }

    let vgm = logger.render_vgm();
    println!("\nrendered VGM: {} bytes", vgm.len());

    if let Some(parent) = std::path::Path::new(OUT_VGM).parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("cannot create output dir {}: {e}", parent.display());
            std::process::exit(1);
        }
    }
    match std::fs::write(OUT_VGM, &vgm) {
        Ok(()) => println!("wrote VGM to {OUT_VGM}"),
        Err(e) => {
            eprintln!("cannot write VGM {OUT_VGM}: {e}");
            std::process::exit(1);
        }
    }
}
