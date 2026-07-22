//! Live windowed frontend for the oracle-core Genesis / Mega Drive core — the milestone's "watchable and
//! interactive" substrate (`docs/plans/2026-07-21-s4-boot-milestone.md`). Where `boot_rom`/`motion_run`
//! wrap [`System`]'s run loop and dump frames to files, this wraps the identical loop in a window: poll the
//! host keyboard → build a [`Pad`] → `set_pad(0, pad)` → `run_frames(1)` → blit the 224-line frame → throttle
//! to ~60 fps.
//!
//! This crate deliberately owns *all* the non-determinism (real-time keyboard, wall-clock throttle) so the
//! core stays the "deterministic, no-I/O" artifact its Cargo description promises. The core is driven only
//! through its existing public API — this slice adds no core methods. Audio is out of scope (milestone D3).
//!
//! Upgrade path (not this slice): swap minifb for `pixels` + `winit` when GPU-composited debug overlays
//! (watchpoint highlights, bus-legality heatmaps — `docs/2026-07-20-diagnostic-tooling-ideas.md`) are wanted.
//!
//! Usage: `cargo run --release -p oracle-frontend -- <rom.bin> [--scale N]`
//!
//! ## Controls (Player 1 only)
//!
//! | Host key          | Emulated input |
//! |-------------------|----------------|
//! | Arrow keys        | D-pad          |
//! | Z / X / C         | A / B / C      |
//! | Enter             | Start          |
//! | Space             | pause / resume |
//! | `.` (period)      | single-frame step (while paused) |
//! | Esc / window-close| quit           |

use minifb::{Key, KeyRepeat, ScaleMode, Window, WindowOptions};
use oracle_core::io::Pad;
use oracle_core::system::System;

/// Active display height in scanlines (Genesis NTSC active area). Width is queried from the VDP *every frame*
/// (H32=256 / H40=320) — the game reprograms it after boot, so it is not fixed at reset.
const HEIGHT: usize = 224;

/// Widest display mode (H40). The window is sized for this so an H40 scene fills it exactly at the requested
/// integer scale; H32 content is pillarboxed by [`ScaleMode::AspectRatioStretch`].
const MAX_WIDTH: usize = 320;

/// Parsed command line: the ROM path and the integer window scale.
struct Args {
    rom_path: String,
    scale: usize,
}

/// Parse `<rom.bin> [--scale N]`. Returns a human-readable error string on misuse (the caller prints it and
/// exits non-zero) — a missing/garbled ROM is a plain error, not a panic, matching the `boot_rom` convention.
fn parse_args() -> Result<Args, String> {
    let mut rom_path: Option<String> = None;
    let mut scale: usize = 3;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--scale" => {
                let v = it.next().ok_or("--scale needs a value")?;
                scale = v
                    .parse::<usize>()
                    .ok()
                    .filter(|&s| (1..=8).contains(&s))
                    .ok_or_else(|| format!("--scale must be an integer 1..=8, got `{v}`"))?;
            }
            other if other.starts_with("--") => return Err(format!("unknown flag `{other}`")),
            other => {
                if rom_path.replace(other.to_string()).is_some() {
                    return Err("multiple ROM paths given".to_string());
                }
            }
        }
    }
    let rom_path = rom_path.ok_or("missing <rom.bin>")?;
    Ok(Args { rom_path, scale })
}

/// Build the Player-1 [`Pad`] from the host keyboard: arrows = D-pad, Z/X/C = A/B/C, Enter = Start.
fn poll_pad(window: &Window) -> Pad {
    Pad {
        up: window.is_key_down(Key::Up),
        down: window.is_key_down(Key::Down),
        left: window.is_key_down(Key::Left),
        right: window.is_key_down(Key::Right),
        a: window.is_key_down(Key::Z),
        b: window.is_key_down(Key::X),
        c: window.is_key_down(Key::C),
        start: window.is_key_down(Key::Enter),
    }
}

/// Render the current frame into `buf` at *native* resolution and return the frame width. `buf` is (re)sized
/// to `width * HEIGHT`; the window does the integer upscale. Native pixels come from the pure `render_line`
/// renderer as `(r,g,b)`, packed here into minifb's `0x00RR_GGBB` u32 layout. Width is re-queried each call
/// because the game switches H32↔H40 after boot.
fn render_into(sys: &System, buf: &mut Vec<u32>) -> usize {
    let width = sys.vdp().render_line(0).len();
    buf.clear();
    buf.reserve(width * HEIGHT);
    for line in 0..HEIGHT {
        for &(r, g, b) in sys.vdp().render_line(line as u16).iter() {
            buf.push((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b));
        }
    }
    width
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("usage: oracle-frontend <rom.bin> [--scale N]   (N = 1..=8, default 3)");
            std::process::exit(2);
        }
    };

    let rom = match std::fs::read(&args.rom_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("cannot read ROM {}: {e}", args.rom_path);
            std::process::exit(1);
        }
    };
    println!("ROM {}: {} bytes", args.rom_path, rom.len());

    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();

    // Fixed window sized for the widest mode (H40) at the requested integer scale; minifb scales the
    // native-resolution frame buffer up to fill it, pillarboxing H32 to keep the aspect ratio correct.
    let (win_w, win_h) = (MAX_WIDTH * args.scale, HEIGHT * args.scale);
    let mut window = Window::new(
        "oracle-next",
        win_w,
        win_h,
        WindowOptions {
            scale_mode: ScaleMode::AspectRatioStretch,
            ..WindowOptions::default()
        },
    )
    .unwrap_or_else(|e| {
        eprintln!("cannot open window: {e}");
        std::process::exit(1);
    });
    // ~60 fps wall-clock throttle (the run loop itself is untimed; this paces presentation).
    window.set_target_fps(60);

    println!(
        "window {win_w}x{win_h} (up to {MAX_WIDTH}x{HEIGHT} @ {}x) — arrows=D-pad, Z/X/C=A/B/C, Enter=Start, Space=pause, .=step, Esc=quit",
        args.scale
    );

    let mut buf: Vec<u32> = Vec::with_capacity(MAX_WIDTH * HEIGHT);
    let mut paused = false;
    let mut frame: u64 = 0;
    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Edge-triggered controls (fire once per physical press, not every frame held).
        if window.is_key_pressed(Key::Space, KeyRepeat::No) {
            paused = !paused;
        }
        let step = window.is_key_pressed(Key::Period, KeyRepeat::No);

        // The pad is sampled live every frame; set_pad is the sole, deterministic input path into the core.
        sys.set_pad(0, poll_pad(&window));

        // Advance when running, or on an explicit step request while paused.
        if !paused || step {
            sys.run_frames(1);
            frame += 1;
        }

        // Native-resolution frame (width re-queried in case the game switched H32↔H40); window upscales it.
        let width = render_into(&sys, &mut buf);
        let title = if paused {
            format!("oracle-next — frame {frame} [PAUSED]")
        } else {
            format!("oracle-next — frame {frame}")
        };
        window.set_title(&title);

        // update_with_buffer both presents and pumps the OS event queue; it honours set_target_fps.
        if let Err(e) = window.update_with_buffer(&buf, width, HEIGHT) {
            eprintln!("present failed: {e}");
            break;
        }
    }
}
