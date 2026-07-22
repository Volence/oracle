//! Scripted-input motion runner — the s4.bin milestone's **motion extension**
//! (`docs/plans/2026-07-21-s4-boot-milestone.md`). Where [`boot_rom`](boot_rom.rs) walks the static-scene
//! ladder, this drives *input* deterministically so we can A/B input-driven motion (player movement, camera
//! scroll, tile streaming, mid-frame h-scroll) against Oracle. It boots a ROM **file** (a build artifact,
//! never committed), applies a scripted pad state at every frame boundary through the sole input path
//! [`System::set_pad`], and at each requested dump frame writes the same region snapshots `boot_rom` does,
//! one set per frame, so two dumps can be byte-diffed to prove the scene actually moved.
//!
//! This is a **dev tool, not a gate artifact** — nothing in CI depends on the ROM existing. A missing or
//! unreadable ROM / script path is a plain error, not a panic backtrace.
//!
//! Usage: `cargo run --release --example motion_run -- <rom.bin> <script.txt> <dump_frames> [prefix] [total]`
//!   - `<script.txt>` — the pad script (format below).
//!   - `<dump_frames>` — comma-separated frame counts to snapshot, e.g. `300,600` (after N frames elapsed).
//!   - `[prefix]` — output basename (default `motion`).
//!   - `[total]` — total frames to run (default = max of the dump frames and the script's latest end).
//!
//! For each dump frame N it writes `<prefix>.f<N>.ppm`, `<prefix>.f<N>.vram.bin`, `.cram.bin`, `.vsram.bin`,
//! `.ram.bin`, `.z80.bin`, and `.regs.txt` — the same regions `boot_rom` dumps, keyed by frame.
//!
//! ## Pad-script format
//!
//! A trivial line-oriented text format. Blank lines and lines beginning with `#` are ignored. Every other
//! line is a hold row:
//!
//! ```text
//! START END BUTTONS [PORT]
//! ```
//!
//! - `START` / `END` — frame indices; the buttons are held during frame indices `[START, END)` (0-based:
//!   frame index `i` is the `i`-th frame run). A dump for frame N captures the state *after* N frames, i.e.
//!   right after frame index `N-1` runs.
//! - `BUTTONS` — a combination of the letters `U D L R A B C S` (up/down/left/right/A/B/C/Start,
//!   case-insensitive, order-free, no separators), or `-` / `none` for "all released".
//! - `PORT` — optional, `0` = Player 1 (default), `1` = Player 2.
//!
//! Rows may overlap; the pad for a frame is the **union** of every row covering it (so one row can hold
//! Right for a whole run while another taps A for a single frame). Example:
//!
//! ```text
//! # hold Right from frame 60 for five seconds (~300 frames), tap A once at frame 120
//! 60 360 R
//! 120 121 A
//! ```

use oracle_core::io::Pad;
use oracle_core::system::System;
use std::io::Write;

/// One parsed hold row: hold `pad` on `port` during frame indices `[start, end)`.
struct HoldRow {
    start: u64,
    end: u64,
    port: usize,
    pad: Pad,
}

/// Parse a `BUTTONS` token (`ULRDABCS` combo, or `-`/`none`) into a [`Pad`]. Case-insensitive, order-free.
fn parse_buttons(tok: &str) -> Result<Pad, char> {
    let mut pad = Pad::default();
    if tok == "-" || tok.eq_ignore_ascii_case("none") {
        return Ok(pad);
    }
    for ch in tok.chars() {
        match ch.to_ascii_uppercase() {
            'U' => pad.up = true,
            'D' => pad.down = true,
            'L' => pad.left = true,
            'R' => pad.right = true,
            'A' => pad.a = true,
            'B' => pad.b = true,
            'C' => pad.c = true,
            'S' => pad.start = true,
            other => return Err(other),
        }
    }
    Ok(pad)
}

/// Parse the whole pad script. Returns the hold rows or a human-readable error (with the 1-based line number).
fn parse_script(text: &str) -> Result<Vec<HoldRow>, String> {
    let mut rows = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let lineno = i + 1;
        // Strip trailing `#` comments and surrounding whitespace; skip blanks/full-line comments.
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let start = fields
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| format!("line {lineno}: expected START frame, got `{line}`"))?;
        let end = fields
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| format!("line {lineno}: expected END frame, got `{line}`"))?;
        let buttons = fields
            .next()
            .ok_or_else(|| format!("line {lineno}: expected BUTTONS, got `{line}`"))?;
        let pad = parse_buttons(buttons)
            .map_err(|c| format!("line {lineno}: unknown button `{c}` in `{buttons}`"))?;
        let port = match fields.next() {
            None => 0,
            Some(p) => p
                .parse::<usize>()
                .ok()
                .filter(|&p| p < 2)
                .ok_or_else(|| format!("line {lineno}: PORT must be 0 or 1, got `{p}`"))?,
        };
        if end <= start {
            return Err(format!(
                "line {lineno}: END ({end}) must be > START ({start})"
            ));
        }
        rows.push(HoldRow {
            start,
            end,
            port,
            pad,
        });
    }
    Ok(rows)
}

/// Union of every row covering `frame` on `port` — the pad state to inject before running that frame.
fn pad_for(rows: &[HoldRow], port: usize, frame: u64) -> Pad {
    let mut pad = Pad::default();
    for row in rows
        .iter()
        .filter(|r| r.port == port && r.start <= frame && frame < r.end)
    {
        pad.up |= row.pad.up;
        pad.down |= row.pad.down;
        pad.left |= row.pad.left;
        pad.right |= row.pad.right;
        pad.a |= row.pad.a;
        pad.b |= row.pad.b;
        pad.c |= row.pad.c;
        pad.start |= row.pad.start;
    }
    pad
}

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

/// Dump one frame's full snapshot under `prefix` (e.g. `motion.f300`): PPM + VRAM/CRAM/VSRAM/RAM/Z80 region
/// bins + human-readable registers — byte-for-byte the same set `boot_rom` writes, keyed by frame.
fn dump_frame(sys: &System, prefix: &str) {
    let height = 224usize;
    let width = sys.vdp().render_line(0).len();
    let mut fb = Vec::with_capacity(width * height);
    for line in 0..height as u16 {
        fb.extend_from_slice(&sys.vdp().render_line(line));
    }
    write_ppm(&format!("{prefix}.ppm"), width, height, &fb).expect("write ppm");

    for (name, bytes) in [
        ("vram", sys.vram()),
        ("cram", sys.vdp().cram()),
        ("vsram", sys.vdp().vsram()),
        ("ram", sys.ram()),
        ("z80", sys.z80_ram()),
    ] {
        std::fs::write(format!("{prefix}.{name}.bin"), bytes).expect("write dump");
    }

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
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(rom_path), Some(script_path), Some(dumps_arg)) =
        (args.next(), args.next(), args.next())
    else {
        eprintln!("usage: motion_run <rom.bin> <script.txt> <dump_frames> [prefix] [total]");
        eprintln!("  dump_frames: comma-separated frame counts, e.g. 300,600");
        std::process::exit(2);
    };
    let prefix = args.next().unwrap_or_else(|| "motion".to_string());
    let total_override: Option<u64> = args.next().and_then(|s| s.parse().ok());

    // Dump frames: comma-separated counts (state *after* N frames).
    let mut dumps: Vec<u64> = match dumps_arg
        .split(',')
        .map(|s| s.trim().parse::<u64>())
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("invalid dump_frames `{dumps_arg}`: expected comma-separated frame counts");
            std::process::exit(2);
        }
    };
    dumps.sort_unstable();
    dumps.dedup();

    let script_text = match std::fs::read_to_string(&script_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read script {script_path}: {e}");
            std::process::exit(1);
        }
    };
    let rows = match parse_script(&script_text) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("script error: {e}");
            std::process::exit(2);
        }
    };

    let rom = match std::fs::read(&rom_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("cannot read ROM {rom_path}: {e}");
            std::process::exit(1);
        }
    };
    println!("ROM {rom_path}: {} bytes", rom.len());

    // Run long enough to reach the last dump and honour the whole script, unless the caller pins `total`.
    let script_end = rows.iter().map(|r| r.end).max().unwrap_or(0);
    let last_dump = dumps.last().copied().unwrap_or(0);
    let total = total_override.unwrap_or_else(|| script_end.max(last_dump));
    println!(
        "{} hold row(s), {} dump frame(s), running {total} frames",
        rows.len(),
        dumps.len()
    );

    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset(); // reset re-powers-on; inject pad state AFTER it (as a frontend would).

    // Frame-boundary loop: set the scripted pad for frame index `i`, run exactly that frame, then dump if
    // `i + 1` (frames elapsed) is a requested dump frame. set_pad is the sole input path — deterministic and
    // host-decoupled — so this run is reproducible bit-for-bit.
    for i in 0..total {
        sys.set_pad(0, pad_for(&rows, 0, i));
        sys.set_pad(1, pad_for(&rows, 1, i));
        sys.run_frames(1);
        let elapsed = i + 1;
        if dumps.binary_search(&elapsed).is_ok() {
            let fprefix = format!("{prefix}.f{elapsed}");
            dump_frame(&sys, &fprefix);
            println!("dumped frame {elapsed} -> {fprefix}.ppm + region bins + regs");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_buttons_case_and_order_free() {
        assert_eq!(
            parse_buttons("R"),
            Ok(Pad {
                right: true,
                ..Default::default()
            })
        );
        assert_eq!(parse_buttons("-"), Ok(Pad::default()));
        assert_eq!(parse_buttons("none"), Ok(Pad::default()));
        // order-free + case-insensitive
        assert_eq!(parse_buttons("sa"), parse_buttons("AS"));
        assert_eq!(
            parse_buttons("uldr"),
            Ok(Pad {
                up: true,
                left: true,
                down: true,
                right: true,
                ..Default::default()
            })
        );
        assert_eq!(parse_buttons("X"), Err('X'));
    }

    #[test]
    fn parses_a_script_with_comments_ports_and_defaults() {
        let rows =
            parse_script("# lead comment\n\n60 360 R   # hold right\n120 121 A 1\n").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].start, rows[0].end, rows[0].port), (60, 360, 0));
        assert!(rows[0].pad.right);
        assert_eq!((rows[1].start, rows[1].end, rows[1].port), (120, 121, 1));
        assert!(rows[1].pad.a);
    }

    #[test]
    fn rejects_bad_rows() {
        assert!(parse_script("10 5 R").is_err()); // end <= start
        assert!(parse_script("10 20 Z").is_err()); // bad button
        assert!(parse_script("10 20 R 2").is_err()); // port out of range
        assert!(parse_script("abc 20 R").is_err()); // non-numeric frame
    }

    #[test]
    fn pad_for_unions_overlapping_rows_per_port() {
        let rows = parse_script("0 100 R\n50 60 A\n0 100 S 1\n").unwrap();
        // frame 55, port 0: Right (whole span) + A (the tap) union.
        let p0 = pad_for(&rows, 0, 55);
        assert!(p0.right && p0.a && !p0.start);
        // frame 55, port 1: only the Start row on that port.
        let p1 = pad_for(&rows, 1, 55);
        assert!(p1.start && !p1.right);
        // outside every span: released.
        assert_eq!(pad_for(&rows, 0, 200), Pad::default());
        // end is exclusive.
        assert!(!pad_for(&rows, 0, 100).right);
    }
}
