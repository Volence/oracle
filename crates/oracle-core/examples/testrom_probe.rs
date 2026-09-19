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
//! | `CUTS=1` | one FNV-1a per line (diff two builds to name the lines a model change moved) + any sprite the per-line pixel budget cut in half on that line |
//! | `PRESS=<start\|a\|b\|c>` + `PRESS_AT=<f>` + `PRESS_LEN=<f>` | hold a button on port 1 for `PRESS_LEN` frames starting at frame `PRESS_AT` |
//! | `PRESSES=<at>:<btn>:<len>,...` | a SCRIPT of presses on port 1 (ascending `at`), for menu-driven ROMs; `btn` is any of `up down left right a b c start`. Cannot be combined with `PRESS` |
//! | `PRIO_ROWS=1` | which plane-A cells have the priority bit set, per row — `vcounter`'s menu cursor is a priority-bit highlight, invisible in the text grid |
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
    let script = press_script();
    if script.is_empty() {
        sys.run_frames(frames);
    } else {
        let mut now = 0u64;
        for (at, pad, len) in script {
            assert!(at >= now, "PRESSES must be in ascending frame order");
            sys.run_frames(at - now);
            sys.set_pad(oracle_core::io::PadPort::P1, pad);
            sys.run_frames(len);
            sys.set_pad(
                oracle_core::io::PadPort::P1,
                oracle_core::io::Pad::default(),
            );
            now = at + len;
        }
        assert!(
            frames >= now,
            "frames ({frames}) is before the last press ends ({now})"
        );
        sys.run_frames(frames - now);
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

    // `PRIO_ROWS=1` — the priority bit (cell bit 15) per plane-A cell, one line per cell row. `vcounter`
    // highlights its menu cursor by setting that bit on the selected row's cells and nothing else, so the
    // cursor is invisible in the text grid above and this is the channel that carries it.
    if std::env::var_os("PRIO_ROWS").is_some() {
        let base = ((sys.vdp().regs()[2] as usize) & 0x38) << 10;
        for row in 0..28usize {
            let mut s = String::new();
            for col in 0..cols {
                let off = base + (row * plane_w + col) * 2;
                let cell = u16::from_be_bytes([sys.vdp().vram()[off], sys.vdp().vram()[off + 1]]);
                s.push(if cell & 0x8000 != 0 { 'P' } else { '.' });
            }
            println!("PRIO {row:02}|{s}|");
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

    // `CUTS=1` — the attribution instrument for the mid-sprite pixel-budget cut (ledger row P1). Prints one
    // hash per line (so two builds can be diffed line by line, naming exactly which lines a model change
    // moved) and, on any line where the budget ran out inside a sprite, the sprite that got cut. Both come
    // from the POST-HOC `render_line*` path, which is the path `frame_hash`/`block_hash` read.
    if std::env::var_os("CUTS").is_some() {
        for line in 0..224u16 {
            let mut lh = 0xcbf2_9ce4_8422_2325u64;
            for (r, g, b) in sys.vdp().render_line(line) {
                for byte in [r, g, b] {
                    lh ^= byte as u64;
                    lh = lh.wrapping_mul(0x0000_0100_0000_01b3);
                }
            }
            let rep = sys.vdp().render_line_report(line);
            // `hflip` is the field the P1 ledger row turns on: screen order and fetch order differ ONLY for
            // an h-flipped straddler, so a ROM whose cut sprites are all unflipped cannot discriminate them.
            let decoded = sys.vdp().sprites_decoded();
            let cuts: Vec<String> = rep
                .sprites
                .iter()
                .filter_map(|s| match s.outcome {
                    oracle_core::render::SpriteOutcome::CutPixelBudget { drawn_px } => {
                        Some(format!(
                            "idx={} x={} w={}cells drawn={drawn_px} hflip={}",
                            s.index, s.x, s.width_cells, decoded[s.index as usize].hflip
                        ))
                    }
                    _ => None,
                })
                .collect();
            println!(
                "LINE {line:3} 0x{lh:016x}{}",
                if cuts.is_empty() {
                    String::new()
                } else {
                    format!("  CUT[{}]", cuts.join("; "))
                }
            );
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

/// One button by name, as `PRESS`/`PRESSES` spell it.
fn button(name: &str) -> oracle_core::io::Pad {
    let mut pad = oracle_core::io::Pad::default();
    match name {
        "start" => pad.start = true,
        "a" => pad.a = true,
        "b" => pad.b = true,
        "c" => pad.c = true,
        "up" => pad.up = true,
        "down" => pad.down = true,
        "left" => pad.left = true,
        "right" => pad.right = true,
        other => panic!("bad button {other:?}"),
    }
    pad
}

/// The press script, as `(at, pad, len)` triples in frame order. `PRESS`/`PRESS_AT`/`PRESS_LEN` is the
/// one-press special case of `PRESSES`; declaring both is refused rather than silently resolved.
fn press_script() -> Vec<(u64, oracle_core::io::Pad, u64)> {
    let one = std::env::var("PRESS").ok();
    let many = std::env::var("PRESSES").ok();
    assert!(
        !(one.is_some() && many.is_some()),
        "PRESS and PRESSES are alternatives; declare one"
    );
    if let Some(btn) = one {
        let at: u64 = std::env::var("PRESS_AT").unwrap().parse().unwrap();
        let len: u64 = std::env::var("PRESS_LEN").unwrap().parse().unwrap();
        return vec![(at, button(&btn), len)];
    }
    let Some(spec) = many else {
        return Vec::new();
    };
    spec.split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|item| {
            let f: Vec<&str> = item.trim().split(':').collect();
            assert_eq!(
                f.len(),
                3,
                "PRESSES item must be <at>:<btn>:<len>, got {item:?}"
            );
            (
                f[0].parse().expect("at"),
                button(f[1]),
                f[2].parse().expect("len"),
            )
        })
        .collect()
}
