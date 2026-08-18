//! Throwaway probe for HInt raster / shadow-highlight investigations: boots a ROM (or restores a
//! frontend `.stateN` save-state), optionally flies the debug player Down then Up, then single-frame
//! marches the machine logging every change of VDP reg $00 (IE1), $0A (HINT counter arm), $0C (S/H)
//! plus the hint-pending latch — each tagged with the scanline it landed on — and writes the LIVE
//! per-scanline frame (mid-frame raster effects included) as a PPM.
//!
//! Usage: cargo run --release -p oracle-core --example sh_probe -- <rom.bin> [settle] [down] [up]
//!        cargo run --release -p oracle-core --example sh_probe -- <file.stateN>

use oracle_core::system::System;
use oracle_core::vdp::{MCLK_PER_FRAME, MCLK_PER_LINE};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("usage: sh_probe <rom.bin|file.stateN> [settle] [down] [up]");
    let settle: u64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(300);
    let down_frames: u64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(0);
    let up_frames: u64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(0);
    let bytes = std::fs::read(&path).expect("read input file");

    // State-file mode: a frontend save-state — the 38-byte ONSS header
    // (magic/version/layout-fp/rom-fp/len/checksum) wraps a System::snapshot payload. The extra
    // args become a journey from the restored state: [right] [left] [down] [up] frames, then a
    // 10-frame rest before the probe.
    if bytes.len() > 38 && &bytes[0..4] == b"ONSS" {
        let mut sys = System::restore(&bytes[38..]).expect("restore save state");
        let legs = [
            settle,
            down_frames,
            up_frames,
            args.next().map(|s| s.parse().unwrap()).unwrap_or(0),
        ];
        for (leg, &frames) in legs.iter().enumerate() {
            for _ in 0..frames {
                let mut pad = oracle_core::io::Pad::default();
                match leg {
                    0 => pad.right = true,
                    1 => pad.left = true,
                    2 => pad.down = true,
                    _ => pad.up = true,
                }
                sys.set_pad(0, pad);
                sys.run_frames(1);
            }
        }
        sys.set_pad(0, oracle_core::io::Pad::default());
        sys.run_frames(10);
        let tag = std::path::Path::new(&path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace('.', "_");
        probe(
            &mut sys,
            &format!("{tag}_r{}l{}d{}u{}", legs[0], legs[1], legs[2], legs[3]),
        );
        return;
    }

    let mut sys = System::new(0x5EED);
    sys.load_rom(bytes);
    sys.reset();
    for i in 0..settle {
        let mut pad = oracle_core::io::Pad::default();
        // Optional journey: hold Down for `down_frames`, then Up for `up_frames`, ending REST
        // frames before the probe so the camera is at rest when we measure (separates schedule
        // lag-while-moving from a settled mismatch, and exposes any descend/ascend hysteresis).
        const REST: u64 = 10;
        let up_end = settle.saturating_sub(REST);
        let up_start = up_end.saturating_sub(up_frames);
        let down_end = up_start;
        let down_start = down_end.saturating_sub(down_frames);
        if down_frames > 0 && (down_start..down_end).contains(&i) {
            pad.down = true;
        }
        if up_frames > 0 && (up_start..up_end).contains(&i) {
            pad.up = true;
        }
        sys.set_pad(0, pad);
        sys.set_pad(1, oracle_core::io::Pad::default());
        sys.run_frames(1);
    }
    sys.set_pad(0, oracle_core::io::Pad::default());
    probe(&mut sys, &format!("f{}", settle + 1));
}

/// March one frame logging register transitions by line, then capture and write the live frame.
fn probe(sys: &mut System, tag: &str) {
    let cam_y = u16::from_be_bytes([sys.ram()[0xA4E2], sys.ram()[0xA4E3]]);
    let cam_x = u16::from_be_bytes([sys.ram()[0xA4DE], sys.ram()[0xA4DF]]);
    println!(
        "Camera=({cam_x},{cam_y}) water screen line = {}",
        224i32 - cam_y as i32
    );

    // Dump the aeon raster state (addresses from s4.debug.lst): the active-buffer pointer, the
    // ROM patch-table pointer, and both working program buffers as u16 words.
    let ram = sys.ram();
    let long = |o: usize| u32::from_be_bytes([ram[o], ram[o + 1], ram[o + 2], ram[o + 3]]);
    println!(
        "Raster_Active_Buf={:08X} Raster_Patch_Tab={:08X}",
        long(0x8AA2),
        long(0x8ACE)
    );
    for (name, base) in [("Buf_A", 0x89A2usize), ("Buf_B", 0x8A22)] {
        let words: Vec<String> = (0..24)
            .map(|i| {
                format!(
                    "{:04X}",
                    u16::from_be_bytes([ram[base + 2 * i], ram[base + 2 * i + 1]])
                )
            })
            .collect();
        println!("Raster_{name}: {}", words.join(" "));
    }

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
    // VDP actually drew (mid-frame raster effects included) as a PPM. The register march above
    // already ran through one frame via run_until, which the frame-grid anchor doesn't know
    // about — ask for 2 so one full frame actually runs under the sink.
    use oracle_core::scanline_capture::{Retain, ScanlineCapture};
    let mut cap = ScanlineCapture::new(Retain::LastFrame);
    sys.run_frames_with_sink(2, &mut cap);
    let px = cap.pixels();
    if !px.is_empty() {
        let w = px.len() / 224;
        let path = format!("sh_live_{tag}.ppm");
        let mut out = format!("P6\n{w} 224\n255\n").into_bytes();
        for &(r, g, b) in px {
            out.extend_from_slice(&[r, g, b]);
        }
        std::fs::write(&path, out).expect("write live ppm");
        println!("live frame ({w}x224) -> {path}");
    }
}
