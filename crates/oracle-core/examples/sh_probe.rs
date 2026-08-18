//! Throwaway probe for the OJZ water-line S/H investigation: boots a ROM, runs to a target frame,
//! then single-steps one full frame logging every change of VDP reg $00 (IE1), $0A (HINT counter
//! arm), $0C (S/H) plus the hint-pending latch — each tagged with the scanline it landed on.
//!
//! Usage: cargo run --release -p oracle-core --example sh_probe -- <rom.bin> [settle_frames]

use oracle_core::system::System;
use oracle_core::vdp::{MCLK_PER_FRAME, MCLK_PER_LINE};

fn main() {
    let mut args = std::env::args().skip(1);
    let rom_path = args
        .next()
        .expect("usage: sh_probe <rom.bin> [settle_frames]");
    let settle: u64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(300);
    let down_frames: u64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(0);
    let rom = std::fs::read(&rom_path).expect("read ROM");

    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();
    for i in 0..settle {
        let mut pad = oracle_core::io::Pad::default();
        // Optional descent: hold Down for the last `down_frames` settle frames (debug fly).
        if down_frames > 0 && i >= settle - down_frames.min(settle) {
            pad.down = true;
        }
        sys.set_pad(0, pad);
        sys.set_pad(1, oracle_core::io::Pad::default());
        sys.run_frames(1);
    }
    sys.set_pad(0, oracle_core::io::Pad::default());

    let cam_y = u16::from_be_bytes([sys.ram()[0xA4E2], sys.ram()[0xA4E3]]);
    let cam_x = u16::from_be_bytes([sys.ram()[0xA4DE], sys.ram()[0xA4DF]]);
    println!(
        "after {settle} frames: Camera=({cam_x},{cam_y}) water screen line = {}",
        224i32 - cam_y as i32
    );

    let frame_start = sys.scheduler().now();
    let frame_end = frame_start + MCLK_PER_FRAME;
    let line_of = |mclk: u64| (mclk % MCLK_PER_FRAME) / MCLK_PER_LINE;

    let mut last = (
        sys.vdp().regs()[0x00],
        sys.vdp().regs()[0x0A],
        sys.vdp().regs()[0x0C],
        sys.vdp().hint_pending(),
    );
    println!(
        "frame start (line {}): reg0={:02X} reg10={:02X} reg12={:02X} hint_pending={}",
        line_of(frame_start),
        last.0,
        last.1,
        last.2,
        last.3
    );

    // March through the frame in quarter-line slices so register flips are tagged to the line
    // (coarser than per-instruction, but the caller owns time — run_until is the legal way).
    let step = MCLK_PER_LINE / 4;
    let mut t = frame_start;
    while t < frame_end {
        t += step;
        sys.run_until(t);
        let cur = (
            sys.vdp().regs()[0x00],
            sys.vdp().regs()[0x0A],
            sys.vdp().regs()[0x0C],
            sys.vdp().hint_pending(),
        );
        if cur != last {
            let pc = sys.cpu_regs().pc;
            println!(
                "line {:3} mclk+{:6}: reg0={:02X} reg10={:02X} reg12={:02X} hint_pending={} (pc={:06X})",
                line_of(t),
                t - frame_start,
                cur.0,
                cur.1,
                cur.2,
                cur.3,
                pc
            );
            last = cur;
        }
    }
    println!("frame end: reg12={:02X}", sys.vdp().regs()[0x0C]);

    // Live-frame capture: run one more frame with a ScanlineCapture sink and write the frame the
    // VDP actually drew (mid-frame raster effects included) as a PPM next to the post-hoc dumps.
    use oracle_core::scanline_capture::{Retain, ScanlineCapture};
    let mut cap = ScanlineCapture::new(Retain::LastFrame);
    // The register march above already ran through frame `settle+1` via run_until, which the frame-grid
    // anchor doesn't know about — ask for 2 so one full frame actually runs under the sink.
    sys.run_frames_with_sink(2, &mut cap);
    let px = cap.pixels();
    if !px.is_empty() {
        let w = px.len() / 224;
        let path = format!("sh_live_f{}.ppm", settle + 1);
        let mut out = format!("P6\n{w} 224\n255\n").into_bytes();
        for &(r, g, b) in px {
            out.extend_from_slice(&[r, g, b]);
        }
        std::fs::write(&path, out).expect("write live ppm");
        println!("live frame ({w}x224) -> {path}");
    }
}
