//! The d-51 listening kit (M24 parcel 4, the Z80 `/INT` as a one-line level): a headless renderer and a
//! WAV differ. Built twice, once against the tree before the change and once after, so the two WAVs of
//! one stretch differ only by the change. See `docs/2026-09-25-d51-listen-kit.md`.
//!
//! ```text
//! listen-kit render <rom> <frames> <out.wav> <out.census.tsv>
//!     Power on (seed 0x51, the replay runner's), no input, run <frames> frames with the audio sink.
//! listen-kit replay <rom> <lst> <ojz|slide> <frames> <out.wav> <out.census.tsv>
//!     As the replay runner does: boot to the arm point, arm the embedded input stream, then run until
//!     <frames> frames from power-on, all with the audio sink attached. The pad is never touched.
//! listen-kit diff <a.wav> <b.wav>
//!     Sample-level comparison: first divergent sample, RMS of the difference, and per-second windows
//!     where the two differ or where one is silent and the other is not.
//! ```
//!
//! The census (`oracle_core::z80_census`) classifies every Z80 `/INT` assert against the documented
//! one-line window (R6): taken in the window, taken late, or missed; late and missed split by whether a
//! 68000 bus grant overlapped the window.

use std::fmt::Write as _;
use std::process::ExitCode;

use oracle_core::bus::{Fanout, StopWhen};
use oracle_core::m68000::bus68k::Bus68k;
use oracle_core::synth::{AudioSink, DEFAULT_SAMPLE_RATE};
use oracle_core::system::System;
use oracle_core::vdp::MCLK_PER_FRAME;
use oracle_core::z80_census::{self, Outcome, Totals};
use oracle_replay::cli::Fixture;
use oracle_replay::runner::{Prepared, POWER_ON_SEED};
use oracle_replay::{ram_u32, ram_u8};

const FC_SUPERVISOR_DATA: u8 = 5;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("render") if args.len() == 5 => render(&args[1], &args[2], &args[3], &args[4]),
        Some("replay") if args.len() == 7 => {
            replay(&args[1], &args[2], &args[3], &args[4], &args[5], &args[6])
        }
        Some("diff") if args.len() == 3 => diff(&args[1], &args[2]),
        _ => Err("usage: see the header of tools/listen-kit/src/main.rs".into()),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("listen-kit: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parse_frames(s: &str) -> Result<u64, String> {
    s.parse().map_err(|_| format!("not a frame count: {s}"))
}

fn read(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))
}

fn frame_of(sys: &System) -> u64 {
    sys.scheduler().now() / MCLK_PER_FRAME
}

fn render(rom: &str, frames: &str, wav: &str, census: &str) -> Result<(), String> {
    let frames = parse_frames(frames)?;
    let rom = read(rom)?;
    let _ = z80_census::take();
    let mut audio = AudioSink::new(DEFAULT_SAMPLE_RATE);
    let mut sys = System::boot_with_sink(POWER_ON_SEED, rom, &mut audio);
    sys.run_frames_with_sink(frames, &mut audio);
    audio.finish();
    let totals = z80_census::take();
    println!("frames run:    {}", frame_of(&sys));
    finish(&audio, &totals, wav, census, "")
}

fn replay(
    rom: &str,
    lst: &str,
    which: &str,
    frames: &str,
    wav: &str,
    census: &str,
) -> Result<(), String> {
    let fixture = match which {
        "ojz" => Fixture::Ojz,
        "slide" => Fixture::OjzSlide,
        _ => return Err(format!("unknown fixture {which}: ojz or slide")),
    };
    let frames = parse_frames(frames)?;
    let lst = String::from_utf8_lossy(&read(lst)?).into_owned();
    let p = Prepared::new(read(rom)?, &lst, fixture)?;
    let a = p.anchors;
    let _ = z80_census::take();
    let mut audio = AudioSink::new(DEFAULT_SAMPLE_RATE);
    // The runner's boot, with the audio sink beside its arm predicate.
    let mut boot = Fanout::new(StopWhen::new(|pc, _| pc == a.init), &mut audio);
    let mut sys = System::boot_with_sink(POWER_ON_SEED, p.rom.clone(), &mut boot);
    sys.run_frames_with_sink(frames, &mut boot);
    if !boot.a.fired() {
        return Err(format!(
            "never reached the arm point within {frames} frames"
        ));
    }
    let armed_at = frame_of(&sys);
    {
        let mut sink = ();
        let mut bus = sys.mega_bus(&mut sink);
        bus.write16(
            a.replay_ptr,
            FC_SUPERVISOR_DATA,
            (p.header.body >> 16) as u16,
        );
        bus.write16(a.replay_ptr + 2, FC_SUPERVISOR_DATA, p.header.body as u16);
        bus.write8(a.input_source, FC_SUPERVISOR_DATA, 1);
    }
    let mut done_at = None;
    while frame_of(&sys) < frames {
        sys.run_frames_with_sink(1, &mut audio);
        if done_at.is_none() && ram_u8(sys.ram(), a.replay_done) != 0 {
            done_at = Some(frame_of(&sys));
        }
    }
    audio.finish();
    let totals = z80_census::take();
    let tick = ram_u32(sys.ram(), a.logic_tick);
    let stuck = sys.cpu_regs().pc == a.error_handler;
    let mut extra = String::new();
    let _ = writeln!(extra, "armed at frame: {armed_at}");
    let _ = writeln!(
        extra,
        "replay done:   {} (stream declares {} ticks; Logic_Tick at end {tick})",
        done_at.map_or("NEVER".to_string(), |f| format!("frame {f}")),
        p.header.tick_count
    );
    let _ = writeln!(extra, "at error blob: {stuck}");
    println!("frames run:    {}", frame_of(&sys));
    finish(&audio, &totals, wav, census, &extra)
}

fn finish(
    audio: &AudioSink,
    t: &Totals,
    wav: &str,
    census: &str,
    extra: &str,
) -> Result<(), String> {
    let pcm = audio.samples();
    std::fs::write(wav, wav_bytes(pcm, audio.sample_rate(), 2))
        .map_err(|e| format!("cannot write {wav}: {e}"))?;
    print!("{extra}");
    println!(
        "wav:           {wav} ({} stereo frames, {:.2} s)",
        pcm.len() / 2,
        pcm.len() as f64 / 2.0 / f64::from(audio.sample_rate())
    );
    println!(
        "census:        asserts {} | acceptances {} | in window {} | late {} (grant {}, masked {}) | \
         missed {} (grant {}, masked {}) | re-triggers {} | orphans {}",
        t.asserts,
        t.acceptances,
        t.in_window,
        t.late_grant + t.late_masked,
        t.late_grant,
        t.late_masked,
        t.missed_grant + t.missed_masked,
        t.missed_grant,
        t.missed_masked,
        t.retriggers,
        t.orphan_acceptances
    );
    let mut tsv = String::from("frame\tseconds\toutcome\tdelay_lines\tgrant_in_window\n");
    for e in &t.events {
        let _ = writeln!(
            tsv,
            "{}\t{:.3}\t{}\t{:.2}\t{}",
            e.frame,
            e.assert_mclk as f64 / 53_693_175.0,
            match e.outcome {
                Outcome::Late => "late",
                Outcome::Missed => "missed",
                Outcome::InWindow => "in_window",
            },
            e.delay_mclk as f64 / 3420.0,
            e.grant_in_window
        );
    }
    std::fs::write(census, tsv).map_err(|e| format!("cannot write {census}: {e}"))
}

/// A canonical 44-byte-header PCM WAV, as `oracle-core/examples/synth_render.rs` writes it.
fn wav_bytes(samples: &[i16], rate: u32, channels: u16) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Parse a WAV this tool wrote: (sample rate, channels, samples).
fn parse_wav(path: &str) -> Result<(u32, usize, Vec<i16>), String> {
    let b = read(path)?;
    if b.len() < 44 || &b[0..4] != b"RIFF" || &b[8..16] != b"WAVEfmt " || &b[36..40] != b"data" {
        return Err(format!("{path}: not a canonical 44-byte-header PCM WAV"));
    }
    let channels = usize::from(u16::from_le_bytes([b[22], b[23]]));
    let rate = u32::from_le_bytes([b[24], b[25], b[26], b[27]]);
    let samples = b[44..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| i16::from_le_bytes(*c))
        .collect();
    Ok((rate, channels, samples))
}

fn rms(xs: impl Iterator<Item = f64>) -> f64 {
    let (mut sum, mut n) = (0.0, 0u64);
    for x in xs {
        sum += x * x;
        n += 1;
    }
    if n == 0 {
        0.0
    } else {
        (sum / n as f64).sqrt()
    }
}

/// Anything with an RMS below this (of full scale 32768) counts as silence: about -60 dBFS.
const SILENCE_RMS: f64 = 32.768;

fn diff(a: &str, b: &str) -> Result<(), String> {
    let (ra, ca, xa) = parse_wav(a)?;
    let (rb, cb, xb) = parse_wav(b)?;
    if (ra, ca) != (rb, cb) {
        return Err(format!("format differs: {ra} Hz x{ca} vs {rb} Hz x{cb}"));
    }
    let frame = ca;
    let secs = |i: usize| (i / frame) as f64 / f64::from(ra);
    println!("A: {a}\n   {} samples ({:.2} s)", xa.len(), secs(xa.len()));
    println!("B: {b}\n   {} samples ({:.2} s)", xb.len(), secs(xb.len()));
    let n = xa.len().min(xb.len());
    if xa.len() != xb.len() {
        println!(
            "LENGTH DIFFERS by {} samples; compared the common {n}",
            xa.len().abs_diff(xb.len())
        );
    }
    let Some(first) = (0..n).find(|&i| xa[i] != xb[i]) else {
        println!("IDENTICAL over the common length: no sample differs");
        return Ok(());
    };
    let differing = (0..n).filter(|&i| xa[i] != xb[i]).count();
    let d = rms((0..n).map(|i| f64::from(xa[i]) - f64::from(xb[i])));
    let sa = rms(xa[..n].iter().map(|&x| f64::from(x)));
    println!(
        "first divergent sample: index {first} (stereo frame {}), t = {:.4} s",
        first / frame,
        secs(first)
    );
    println!(
        "samples differing: {differing} of {n} ({:.2}%)",
        100.0 * differing as f64 / n as f64
    );
    println!(
        "RMS of the difference: {d:.1} (of full scale 32768; {:.1} dB relative to A's RMS {sa:.1})",
        if d > 0.0 && sa > 0.0 {
            20.0 * (d / sa).log10()
        } else {
            f64::NEG_INFINITY
        }
    );
    let win = ra as usize * frame;
    println!("per-second windows (only those that differ):");
    println!("  second  rms(A)   rms(B)   rms(A-B)  silence");
    let mut silence_mismatch = 0;
    for w in 0..n.div_ceil(win) {
        let (lo, hi) = (w * win, ((w + 1) * win).min(n));
        if xa[lo..hi] == xb[lo..hi] {
            continue;
        }
        let ra_ = rms(xa[lo..hi].iter().map(|&x| f64::from(x)));
        let rb_ = rms(xb[lo..hi].iter().map(|&x| f64::from(x)));
        let rd = rms((lo..hi).map(|i| f64::from(xa[i]) - f64::from(xb[i])));
        let sil = match (ra_ < SILENCE_RMS, rb_ < SILENCE_RMS) {
            (true, false) => {
                silence_mismatch += 1;
                "A silent, B not"
            }
            (false, true) => {
                silence_mismatch += 1;
                "B silent, A not"
            }
            (true, true) => "both silent",
            (false, false) => "",
        };
        println!("  {w:>6}  {ra_:>7.1}  {rb_:>7.1}  {rd:>8.1}  {sil}");
    }
    println!(
        "windows silent in one and not the other: {silence_mismatch} (silence = RMS < {SILENCE_RMS} of 32768)"
    );
    Ok(())
}
