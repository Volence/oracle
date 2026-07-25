//! Recon probe for the test-ROM conformance harness. Boots a vendored test ROM headlessly and dumps what
//! the harness scrapes, so every pinned baseline in `tests/conformance_roms.rs` can be re-derived and
//! audited by hand instead of taken on faith. Read-only: it touches nothing in `src/`.
//!
//! ```text
//! cargo run -p oracle-core --example testrom_probe -- vendor/TestRoms/<rom>.bin <frames> [font_base_hex]
//! ```
//!
//! Always prints: the rendered line width (256 = H32 / 320 = H40), R2, the backdrop CRAM word, the plane-A
//! and plane-B text grids decoded through `font_base`, and the whole-frame FNV-1a hash. Optional dumps,
//! selected by environment variable:
//!
//! | env | effect |
//! |---|---|
//! | `RAW_ROW=<n>` | raw 16-bit nametable cells of plane-A row `n`, across the full plane pitch |
//! | `TILES=<hex>,<count>` | decode `count` tiles from `hex` as 4-bit colour-index art (8 rows) |
//! | `SCREEN=<step>` (+ `SCRX0=<x>`) | framebuffer as luminance ASCII, every `step`-th pixel/line from `x` |
//! | `BLOCKS=1` | FNV-1a of the 32x8 rect at x 216..248 for cell rows 6..14 — the `vdp_sprite_masking` verdict glyphs |
//! | `PRESS=<start\|a\|b\|c>` + `PRESS_AT=<f>` + `PRESS_LEN=<f>` | hold a button on port 1 for `PRESS_LEN` frames starting at frame `PRESS_AT` |
//!
//! Documented in `docs/2026-07-25-testrom-conformance.md` ("How to amend a row").

use oracle_core::system::System;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = &args[1];
    let frames: u64 = args[2].parse().unwrap();
    let font_base: u16 = args
        .get(3)
        .map(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).unwrap())
        .unwrap_or(0);

    let rom = std::fs::read(path).unwrap();
    let mut sys = System::new(0x1234_5678);
    sys.load_rom(rom);
    sys.reset();
    if let Ok(btn) = std::env::var("PRESS") {
        let at: u64 = std::env::var("PRESS_AT").unwrap().parse().unwrap();
        let len: u64 = std::env::var("PRESS_LEN").unwrap().parse().unwrap();
        let mut pad = oracle_core::io::Pad::default();
        match btn.as_str() {
            "start" => pad.start = true,
            "a" => pad.a = true,
            "b" => pad.b = true,
            "c" => pad.c = true,
            _ => panic!("bad button"),
        }
        sys.run_frames(at);
        sys.set_pad(0, pad);
        sys.run_frames(len);
        sys.set_pad(0, oracle_core::io::Pad::default());
        sys.run_frames(frames - at - len);
    } else {
        sys.run_frames(frames);
    }

    let width = sys.vdp().render_line(0).len();
    println!("width={width} regs2={:02X}", sys.vdp().regs()[2]);
    println!(
        "backdrop={:04X} reg7={:02X}",
        u16::from_be_bytes([sys.vdp().cram()[0], sys.vdp().cram()[1]]),
        sys.vdp().regs()[7]
    );

    let plane_w = match sys.vdp().regs()[16] & 0x03 {
        0 => 32,
        1 => 64,
        _ => 128,
    };
    let cols = width / 8;
    for (label, base) in [
        ("A", ((sys.vdp().regs()[2] as usize) & 0x38) << 10),
        ("B", ((sys.vdp().regs()[4] as usize) & 0x07) << 13),
    ] {
        println!("--- plane {label} base ${base:04X} plane_w={plane_w} cols={cols}");
        for row in 0..28usize {
            let mut s = String::new();
            for col in 0..cols {
                let off = base + (row * plane_w + col) * 2;
                let cell = u16::from_be_bytes([sys.vdp().vram()[off], sys.vdp().vram()[off + 1]]);
                let c = (cell & 0x7FF).wrapping_sub(font_base);
                s.push(if (0x20..0x7F).contains(&c) {
                    c as u8 as char
                } else {
                    '.'
                });
            }
            println!("{row:02}|{s}|");
        }
    }

    if std::env::var_os("RAW_ROW").is_some() {
        let base = ((sys.vdp().regs()[2] as usize) & 0x38) << 10;
        let row: usize = std::env::var("RAW_ROW").unwrap().parse().unwrap();
        let mut s = String::new();
        for col in 0..plane_w {
            let off = base + (row * plane_w + col) * 2;
            let cell = u16::from_be_bytes([sys.vdp().vram()[off], sys.vdp().vram()[off + 1]]);
            s.push_str(&format!("{cell:04X} "));
        }
        println!("RAW row {row}: {s}");
    }

    if let Ok(spec) = std::env::var("TILES") {
        let mut it = spec.split(',');
        let first = usize::from_str_radix(it.next().unwrap(), 16).unwrap();
        let count: usize = it.next().unwrap().parse().unwrap();
        for py in 0..8 {
            let mut s = String::new();
            for t in first..first + count {
                let px = sys.vdp().tile_pixels(t);
                for x in 0..8 {
                    s.push(char::from_digit(px[py * 8 + x] as u32, 16).unwrap());
                }
                s.push('|');
            }
            println!("TILE {s}");
        }
    }

    if std::env::var_os("SCREEN").is_some() {
        let step: usize = std::env::var("SCREEN").unwrap().parse().unwrap_or(2);
        let x0: usize = std::env::var("SCRX0")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        for line in (0..224u16).step_by(step) {
            let px = sys.vdp().render_line(line);
            let mut s = String::new();
            for (i, (r, g, b)) in px.iter().enumerate().skip(x0) {
                if i % step == 0 {
                    let lum = (*r as u32 + *g as u32 + *b as u32) / 3;
                    s.push(match lum {
                        0..=32 => ' ',
                        33..=96 => '.',
                        97..=160 => '+',
                        161..=224 => '*',
                        _ => '#',
                    });
                }
            }
            println!("SCR {line:3}|{s}|");
        }
    }

    if std::env::var_os("BLOCKS").is_some() {
        for row in 6..15u16 {
            let mut bh = 0xcbf2_9ce4_8422_2325u64;
            for y in row * 8..row * 8 + 8 {
                let px = sys.vdp().render_line(y);
                for (r, g, b) in px.iter().take(248).skip(216) {
                    for byte in [*r, *g, *b] {
                        bh ^= byte as u64;
                        bh = bh.wrapping_mul(0x0000_0100_0000_01b3);
                    }
                }
            }
            println!("BLOCK row {row} = 0x{bh:016x}");
        }
    }

    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for line in 0..224u16 {
        for (r, g, b) in sys.vdp().render_line(line) {
            for byte in [r, g, b] {
                h ^= byte as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    println!("frame_hash=0x{h:016x}");
}
