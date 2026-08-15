//! Synth render experiment (Phase SY-1) — attach an [`AudioSink`] to a real ROM run and write the
//! synthesized PCM to a hand-rolled `.wav`, mirroring `vgm_capture.rs`.
//!
//! This is a **dev tool, not a gate artifact** — it boots a ROM *file* (a build artifact, never
//! committed), runs N frames with a caller-owned [`AudioSink`] threaded through both CPUs via
//! [`System::run_frames_with_sink`], and dumps a 44-byte-header + `i16` PCM WAV (no `hound` dep — the
//! same "hand-roll the container" discipline as the VGM writer). It touches no `src/` state, uses only
//! the public API, and is compiled ONLY under `--features synth`.
//!
//! Usage: `cargo run --release -p oracle-core --features synth --example synth_render -- [rom.bin] [frames]`
//!   - `[rom.bin]` — ROM path (default `/home/volence/sonic_hacks/aeon/s4.soundtest.bin`).
//!   - `[frames]` — frames to run (default 600 ≈ 10 s at 60 Hz).

use oracle_core::synth::{AudioSink, ConsoleModel, DEFAULT_SAMPLE_RATE};
use oracle_core::system::System;

const DEFAULT_ROM: &str = "/home/volence/sonic_hacks/aeon/s4.soundtest.bin";
const DEFAULT_FRAMES: u64 = 600;
const OUT_WAV: &str =
    "/tmp/claude-1000/-home-volence-sonic-hacks-oracle-next/scratchpad/s4_synth.wav";

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

    let console = match args.next() {
        None => ConsoleModel::default(),
        Some(s) => match ConsoleModel::from_name(&s) {
            Some(m) => m,
            None => {
                eprintln!("unknown console model {s:?}; try va0, va3 or va7");
                std::process::exit(1);
            }
        },
    };
    let mut sink = AudioSink::with_console_model(DEFAULT_SAMPLE_RATE, console);
    sys.run_frames_with_sink(frames, &mut sink);
    sink.finish(); // flush the final in-progress frame

    let pcm = sink.samples();
    let non_silent = pcm.iter().filter(|&&s| s != 0).count();
    println!("frames run:     {frames}");
    println!("sample rate:    {} Hz", sink.sample_rate());
    println!("console model:  {}", sink.console_model().name());
    println!("stereo frames:  {}", sink.len_frames());
    println!("i16 samples:    {}", pcm.len());
    println!(
        "non-silent:     {non_silent} ({:.1}%)",
        100.0 * non_silent as f64 / pcm.len().max(1) as f64
    );

    let wav = write_wav(pcm, sink.sample_rate(), 2);
    println!("WAV bytes:      {}", wav.len());

    if let Some(parent) = std::path::Path::new(OUT_WAV).parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("cannot create output dir {}: {e}", parent.display());
            std::process::exit(1);
        }
    }
    match std::fs::write(OUT_WAV, &wav) {
        Ok(()) => println!("wrote WAV to {OUT_WAV}"),
        Err(e) => {
            eprintln!("cannot write WAV {OUT_WAV}: {e}");
            std::process::exit(1);
        }
    }
}

/// Hand-roll a canonical 44-byte-header PCM WAV (RIFF/WAVE, 16-bit signed, little-endian) around the
/// interleaved `samples`. No dependency — same discipline as the VGM writer.
fn write_wav(samples: &[i16], sample_rate: u32, channels: u16) -> Vec<u8> {
    let bits_per_sample: u16 = 16;
    let bytes_per_sample = bits_per_sample / 8;
    let block_align = channels * bytes_per_sample;
    let byte_rate = sample_rate * block_align as u32;
    let data_len = (samples.len() * bytes_per_sample as usize) as u32;

    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes()); // ChunkSize = 36 + Subchunk2Size
    out.extend_from_slice(b"WAVE");
    // fmt subchunk.
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size (PCM)
    out.extend_from_slice(&1u16.to_le_bytes()); // AudioFormat = 1 (PCM)
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());
    // data subchunk.
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}
