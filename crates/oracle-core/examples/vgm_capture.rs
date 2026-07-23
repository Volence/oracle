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
//!
//! Sub-frame mode (SY-4b validation, opt-in via env — default behavior is untouched):
//!   - `VGM_SUBFRAME=1` — use [`VgmLogger::with_subframe_waits`] so the rendered VGM carries mclk-derived
//!     `0x61` sample-delta waits, and write it to `VGM_OUT` (default `scratchpad/s4_subframe.vgm`).
//!   - `VGM_TIMELINE=<path>` — additionally dump a TSV of the ground-truth write timeline
//!     (`mclk  abs_sample  intra_frame_sample  chip  port  reg  value  frame`) for the direct timing proof.

use oracle_core::system::{System, MCLK_PER_FRAME};
use oracle_core::vgm::{SoundChip, VgmLogger};

const DEFAULT_ROM: &str = "/home/volence/sonic_hacks/aeon/s4.bin";
const DEFAULT_FRAMES: u64 = 600;
const OUT_VGM: &str =
    "/tmp/claude-1000/-home-volence-sonic-hacks-oracle-next/scratchpad/s4_capture.vgm";
const OUT_VGM_SUBFRAME: &str =
    "/tmp/claude-1000/-home-volence-sonic-hacks-oracle-next/scratchpad/s4_subframe.vgm";
/// Samples per NTSC frame at the 44100 Hz VGM timebase.
const SAMPLES_PER_FRAME: u64 = 735;

fn main() {
    let mut args = std::env::args().skip(1);
    let rom_path = args.next().unwrap_or_else(|| DEFAULT_ROM.to_string());
    let frames: u64 = args
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_FRAMES);

    // Opt-in sub-frame mode (env). Default (unset) reproduces the original frame-bucketed capture exactly.
    let subframe = std::env::var("VGM_SUBFRAME").is_ok_and(|v| v != "0" && !v.is_empty());
    let out_vgm = if subframe {
        std::env::var("VGM_OUT").unwrap_or_else(|_| OUT_VGM_SUBFRAME.to_string())
    } else {
        OUT_VGM.to_string()
    };

    let rom = match std::fs::read(&rom_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("cannot read ROM {rom_path}: {e}");
            std::process::exit(1);
        }
    };
    println!("ROM {rom_path}: {} bytes", rom.len());
    if subframe {
        println!("mode:        SUB-FRAME (mclk-derived 0x61 waits)");
    }

    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();

    let mut logger = if subframe {
        VgmLogger::with_subframe_waits()
    } else {
        VgmLogger::new()
    };
    sys.run_frames_with_sink(frames, &mut logger);

    // Optional ground-truth write-timeline dump (SY-4b direct timing proof). Each record's absolute mclk (from
    // the SY-4a on_event_at seam) → absolute 44100 Hz sample and intra-frame sample (`abs_sample % 735`), which
    // is exactly where SY-4b places the write; SY-4a places every write at intra-frame sample 0.
    if let Ok(tl_path) = std::env::var("VGM_TIMELINE") {
        let mut tsv =
            String::from("mclk\tabs_sample\tintra_frame_sample\tchip\tport\treg\tvalue\tframe\n");
        for (r, &mclk) in logger.records().iter().zip(logger.mclks().iter()) {
            let abs_sample = mclk * SAMPLES_PER_FRAME / MCLK_PER_FRAME;
            let intra = abs_sample % SAMPLES_PER_FRAME;
            let chip = match r.chip {
                SoundChip::Ym2612 => "FM",
                SoundChip::Psg => "PSG",
            };
            tsv.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{:02X}\t{:02X}\t{}\n",
                mclk, abs_sample, intra, chip, r.port, r.reg, r.value, r.frame
            ));
        }
        match std::fs::write(&tl_path, &tsv) {
            Ok(()) => println!(
                "wrote timeline ({} rows) to {tl_path}",
                logger.records().len()
            ),
            Err(e) => eprintln!("cannot write timeline {tl_path}: {e}"),
        }
    }

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

    if let Some(parent) = std::path::Path::new(&out_vgm).parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("cannot create output dir {}: {e}", parent.display());
            std::process::exit(1);
        }
    }
    match std::fs::write(&out_vgm, &vgm) {
        Ok(()) => println!("wrote VGM to {out_vgm}"),
        Err(e) => {
            eprintln!("cannot write VGM {out_vgm}: {e}");
            std::process::exit(1);
        }
    }
}
