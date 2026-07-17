//! Pad-probe dev tool — the visible proof that injected controller input reaches the screen through the real
//! hardware path. Boots the pad-poll fixture ROM (`testrom::build_pad_poll`) on a real [`System`] twice: once
//! with Player 1's pad released, once with **Start** held. The fixture polls the pad through the actual
//! 3-button TH protocol (recon IO4) and drives the backdrop colour register from what it reads, so the two
//! PPMs differ by their whole backdrop — a glance confirms the pad reached the VDP.
//!
//! This is a **dev tool, not a gate artifact** — the `io_controllers` integration test is the automated
//! proof; this exists so the owner can eyeball a before/after pair.
//!
//! Usage: `cargo run --release --example pad_probe -- [released.ppm] [start.ppm]`
//! (defaults: `pad_released.ppm`, `pad_start.ppm`).

use oracle_core::io::Pad;
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

/// Boot the pad-poll fixture with `pad` injected on Player 1, run 3 frames, render the active display, and
/// write it to `path`.
fn probe(path: &str, pad: Pad) -> std::io::Result<()> {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build_pad_poll());
    sys.reset(); // reset re-powers-on; inject AFTER it (as a frontend would).
    sys.set_pad(0, pad);
    sys.run_frames(3);

    let height = 224usize;
    let width = sys.vdp().render_line(0).len();
    let mut fb = Vec::with_capacity(width * height);
    for line in 0..height as u16 {
        fb.extend_from_slice(&sys.vdp().render_line(line));
    }
    write_ppm(path, width, height, &fb)?;
    let sample = sys.vdp().render_line(112)[width / 2];
    println!("wrote {width}x{height} {path} (backdrop = {sample:?})");
    Ok(())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let released = args
        .next()
        .unwrap_or_else(|| "pad_released.ppm".to_string());
    let start = args.next().unwrap_or_else(|| "pad_start.ppm".to_string());

    if let Err(e) = probe(&released, Pad::default()) {
        eprintln!("failed to write {released}: {e}");
    }
    if let Err(e) = probe(
        &start,
        Pad {
            start: true,
            ..Default::default()
        },
    ) {
        eprintln!("failed to write {start}: {e}");
    }
}
