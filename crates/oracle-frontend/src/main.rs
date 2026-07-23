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
//! | Host key          | Emulated input / action |
//! |-------------------|-------------------------|
//! | Arrow keys        | D-pad          |
//! | A / S / D         | A / B / C      |
//! | Enter             | Start          |
//! | Space             | pause / resume |
//! | `.` (period)      | single-frame step (while paused) |
//! | Left mouse click  | watch the VRAM tile under the clicked pixel ("who wrote this tile?") |
//! | W                 | dump recorded watch hits (seq/frame/pc/addr/old→new/via) + drop count to stdout |
//! | C                 | clear the watch (return to the fast null-sink run path) |
//! | Esc / window-close| quit           |
//!
//! ## Pixel-attribution watch (record + display only)
//!
//! A left click asks the VDP `pixel_attribution(x,y)` who is showing at that dot; if the winner is a
//! plane/window tile, its 32-byte VRAM range is armed as a VDP-internal write watch on a *caller-owned*
//! [`Watchpoints`] (the core never stores it — this stays a zero-diff, frontend-only slice). While a watch is
//! armed the run loop drives the sink-generic [`System::run_frames_with_sink`]; with no watch it stays on the
//! untouched null-sink [`System::run_frames`] fast path. `W` prints the recorded hits; `C` disarms. Sprite /
//! backdrop pixels (`cell == None`) report and arm nothing this slice (a documented follow-up). Break-on-hit
//! and an on-screen text overlay are out of scope — the core is frame-batched and minifb has no text.

// Phase SY-5a real-time-audio substrate (SPSC ring + `i16→f32` + composite `BusEventSink`). Feature-gated
// and self-contained. Phase SY-5b (below) consumes it: the live loop drives the composite sink each frame
// and drains→pushes into the ring, and `start_audio` spawns the cpal output stream that pops the ring's
// consumer. See `docs/2026-07-23-phase-sy5-realtime-audio-design.md`.
#[cfg(feature = "audio")]
mod audio;

use minifb::{Key, KeyRepeat, MouseButton, MouseMode, ScaleMode, Window, WindowOptions};
use oracle_core::io::Pad;
use oracle_core::system::System;
use oracle_core::watchpoints::{WatchOp, WatchSpace, WatchVia, Watchpoints};

/// Active display height in scanlines (Genesis NTSC active area). Width is queried from the VDP *every frame*
/// (H32=256 / H40=320) — the game reprograms it after boot, so it is not fixed at reset.
const HEIGHT: usize = 224;

/// Widest display mode (H40). The window is sized for this so an H40 scene fills it exactly at the requested
/// integer scale; H32 content is pillarboxed by [`ScaleMode::AspectRatioStretch`].
const MAX_WIDTH: usize = 320;

/// Ring capacity of the pixel-attribution watch log. One armed watch covers a single 32-byte tile, so the
/// per-frame write count is small; this is a generous bound (drops are still counted and reported by `W`).
const WATCH_CAP: usize = 8192;

/// Map a physical window-pixel click `(mx, my)` to a native VDP pixel `(x, y)`, or `None` if the click lands
/// in the H32 pillarbox or outside the active frame.
///
/// The window is fixed at `MAX_WIDTH * scale` wide; the game's native frame is `width` wide (256 H32 / 320
/// H40) and — under [`ScaleMode::AspectRatioStretch`] with a native-height (224) buffer — displays at exactly
/// the integer `scale`, horizontally centered. So each axis divides by `scale`, and the horizontal pillarbox
/// (`(MAX_WIDTH - width) / 2` native pixels each side, zero for H40) is subtracted. `get_mouse_pos` returns
/// physical window coordinates here (the window uses `Scale::X1`), which is exactly this function's input.
fn window_to_native(mx: f32, my: f32, scale: usize, width: usize) -> Option<(u16, u16)> {
    if mx < 0.0 || my < 0.0 || scale == 0 {
        return None;
    }
    let scale_f = scale as f32;
    let pillarbox = MAX_WIDTH.saturating_sub(width) / 2; // native pixels of left/right box (0 for H40)
    let nx = mx / scale_f - pillarbox as f32;
    let ny = my / scale_f;
    if nx < 0.0 || ny < 0.0 {
        return None;
    }
    let (x, y) = (nx as usize, ny as usize);
    if x >= width || y >= HEIGHT {
        return None; // in the pillarbox (past the right edge of native content) or below the frame
    }
    Some((x as u16, y as u16))
}

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

/// Build the Player-1 [`Pad`] from the host keyboard: arrows = D-pad, A/S/D = A/B/C, Enter = Start.
fn poll_pad(window: &Window) -> Pad {
    Pad {
        up: window.is_key_down(Key::Up),
        down: window.is_key_down(Key::Down),
        left: window.is_key_down(Key::Left),
        right: window.is_key_down(Key::Right),
        a: window.is_key_down(Key::A),
        b: window.is_key_down(Key::S),
        c: window.is_key_down(Key::D),
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

/// Print every recorded watch hit (oldest first) and the drop count to stdout. `pc` is raw hex (oracle-next
/// has no symbol table), `via` is the CPU-vs-DMA attribution (`Direct` = CPU data-port write, `Dma` = DMA
/// step, `Bus` = a v1 68000 bus access — unused by this slice's VRAM watch but printed faithfully).
fn dump_hits(watchpoints: &Watchpoints) {
    let hits = watchpoints.hits();
    println!("--- watch hits: {} recorded ---", hits.len());
    for h in hits {
        let via = match h.via {
            WatchVia::Bus => "Bus",
            WatchVia::Direct => "Direct(CPU)",
            WatchVia::Dma => "Dma",
        };
        println!(
            "seq {:>6}  frame {:>6}  pc ${:06X}  addr ${:04X}  ${:X}->${:X}  via {via}",
            h.seq, h.frame, h.pc, h.addr, h.old, h.value
        );
    }
    println!("dropped: {}", watchpoints.dropped());
}

/// Draw a small contrasting crosshair (colour-inverted plus sign) at native pixel `(wx, wy)` in the packed
/// `0x00RR_GGBB` frame buffer. Bounds-guarded: silently does nothing if the pixel is outside the current
/// `width * HEIGHT` frame (e.g. after an H40→H32 mode switch since the click).
fn draw_crosshair(buf: &mut [u32], width: usize, wx: u16, wy: u16) {
    let (cx, cy) = (wx as usize, wy as usize);
    if cx >= width || cy >= HEIGHT {
        return;
    }
    // Horizontal arm over the full span, vertical arm only for d != 0 so the shared center is inverted once
    // (XORing it twice would cancel back to the original colour).
    for d in -2i32..=2 {
        let mut arms = vec![(cx as i32 + d, cy as i32)];
        if d != 0 {
            arms.push((cx as i32, cy as i32 + d));
        }
        for (px, py) in arms {
            if px >= 0 && (px as usize) < width && py >= 0 && (py as usize) < HEIGHT {
                let idx = py as usize * width + px as usize;
                buf[idx] ^= 0x00FF_FFFF; // invert RGB for visibility against any background
            }
        }
    }
}

/// Live host-audio state (Phase SY-5b). Held for the whole run: the persistent synth [`AudioSink`] (advanced
/// exactly one frame per loop iteration by the composite sink, then drained), the SPSC ring producer the
/// drained PCM is pushed into, and the kept-alive cpal [`Stream`](cpal::Stream) — dropping the stream stops
/// playback, so it must outlive the loop.
#[cfg(feature = "audio")]
struct AudioState {
    sink: oracle_core::synth::AudioSink,
    prod: audio::AudioProd,
    _stream: cpal::Stream,
}

/// Enumerate the default output device and start a cpal f32 output stream feeding the SPSC ring, returning the
/// live [`AudioState`]. Thin wrapper over [`build_audio`] with the host's real device; factored so the
/// no-device branch is deterministically unit-testable (design §3.1, §7 Test 7).
#[cfg(feature = "audio")]
fn start_audio() -> Option<AudioState> {
    use cpal::traits::HostTrait;
    build_audio(cpal::default_host().default_output_device())
}

/// Build the cpal output stream for `device` (design §3). Returns `None` — **run video-only** — on ANY
/// failure (no device, no default config, non-f32 format, stream build/play error), printing a one-line
/// warning; it **never panics**. This is the graceful path for a headless, `/dev/snd`-less environment: pass
/// `None` and it cleanly reports "no device" and disables audio.
///
/// The stream's callback pops the ring's consumer into the device buffer, zero-filling any underrun tail
/// (design §2.5). It handles the device channel count: stereo = a straight interleaved copy; mono = average
/// each L,R pair; other counts = write L,R into the first two lanes of each output frame and silence the rest
/// (a documented first-cut simplification, design §3.3).
#[cfg(feature = "audio")]
fn build_audio(device: Option<cpal::Device>) -> Option<AudioState> {
    use cpal::traits::{DeviceTrait, StreamTrait};
    use ringbuf::traits::Consumer;

    let Some(device) = device else {
        eprintln!("audio: no default output device — running video-only");
        return None;
    };
    let default_cfg = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("audio: no default output config ({e}) — running video-only");
            return None;
        }
    };
    // First cut requires an f32 device format (matches the f32 ring; an i16/u16 match is a later robustness
    // pass, design §3.3). Anything else → video-only rather than a wrong-format stream.
    if default_cfg.sample_format() != cpal::SampleFormat::F32 {
        eprintln!(
            "audio: device sample format {:?} is not f32 (first cut requires f32) — running video-only",
            default_cfg.sample_format()
        );
        return None;
    }

    // Take the DEVICE's native rate and channel count — AudioSink renders at exactly this rate, so there is
    // no resampler and no pitch error (design §3.2).
    let sample_rate = default_cfg.sample_rate().0;
    let channels = default_cfg.channels() as usize;
    let config: cpal::StreamConfig = default_cfg.config();

    let sink = oracle_core::synth::AudioSink::new(sample_rate);
    let (prod, mut cons) = audio::make_ring(sample_rate);

    let data_cb = move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
        match channels {
            2 => {
                // Stereo: the ring is already interleaved L,R,L,R… — a pure copy, silence any shortfall.
                let n = cons.pop_slice(out);
                out[n..].fill(0.0);
            }
            1 => {
                // Mono: average each ring L,R pair into one sample; underrun → silence.
                for slot in out.iter_mut() {
                    let mut lr = [0.0f32; 2];
                    let got = cons.pop_slice(&mut lr);
                    *slot = if got == 2 { (lr[0] + lr[1]) * 0.5 } else { 0.0 };
                }
            }
            ch => {
                // >2 (or a degenerate 0): L,R into the first two lanes of each output frame, rest silent.
                for out_frame in out.chunks_mut(ch.max(1)) {
                    let mut lr = [0.0f32; 2];
                    let got = cons.pop_slice(&mut lr);
                    for (i, s) in out_frame.iter_mut().enumerate() {
                        *s = if i < got { lr[i] } else { 0.0 };
                    }
                }
            }
        }
    };
    let err_cb = |err| eprintln!("audio stream error: {err}");

    let stream = match device.build_output_stream::<f32, _, _>(&config, data_cb, err_cb, None) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("audio: failed to build output stream ({e}) — running video-only");
            return None;
        }
    };
    if let Err(e) = stream.play() {
        eprintln!("audio: failed to start output stream ({e}) — running video-only");
        return None;
    }

    println!("audio: {sample_rate} Hz, {channels} ch (f32) — streaming");
    Some(AudioState {
        sink,
        prod,
        _stream: stream,
    })
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
        "window {win_w}x{win_h} (up to {MAX_WIDTH}x{HEIGHT} @ {}x) — arrows=D-pad, A/S/D=A/B/C, Enter=Start, Space=pause, .=step, click=watch-tile, W=dump, C=clear, Esc=quit",
        args.scale
    );

    // Start the host audio stream (Phase SY-5b). `None` = no device / build failure → video-only, never a
    // panic (the default in a headless, /dev/snd-less environment). When present, its persistent AudioSink is
    // advanced one frame per iteration below and drained→pushed into the ring the cpal callback consumes.
    #[cfg(feature = "audio")]
    let mut audio = start_audio();

    let mut buf: Vec<u32> = Vec::with_capacity(MAX_WIDTH * HEIGHT);
    let mut paused = false;
    let mut frame: u64 = 0;

    // The pixel-attribution watch — a *caller-owned* sink (the core never stores it, keeping this slice
    // zero-diff on oracle-core). `watch_armed` mirrors "a VDP watch is registered" so the run loop can stay on
    // the fast null-sink path when nothing is being watched. `watched_pixel` drives the on-screen crosshair.
    let mut watchpoints = Watchpoints::new(WATCH_CAP);
    let mut watch_armed = false;
    let mut watched_pixel: Option<(u16, u16)> = None;
    let mut prev_mouse_down = false;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Edge-triggered controls (fire once per physical press, not every frame held).
        if window.is_key_pressed(Key::Space, KeyRepeat::No) {
            paused = !paused;
        }
        let step = window.is_key_pressed(Key::Period, KeyRepeat::No);

        // A left-click edge maps the clicked window pixel to a native dot and asks the VDP who is showing
        // there; a plane/window tile winner arms a watch on that tile's 32-byte VRAM range (replacing any
        // prior watch). Width is the *currently displayed* frame's width (pre-step), so the click resolves
        // against what the user is actually looking at.
        let mouse_down = window.get_mouse_down(MouseButton::Left);
        let clicked = mouse_down && !prev_mouse_down;
        prev_mouse_down = mouse_down;
        if clicked {
            let display_width = sys.vdp().render_line(0).len();
            if let Some((mx, my)) = window.get_mouse_pos(MouseMode::Discard) {
                if let Some((x, y)) = window_to_native(mx, my, args.scale, display_width) {
                    match sys.vdp().pixel_attribution(x, y).cell {
                        Some(cell) => {
                            let lo = u32::from(cell.tile) * 32;
                            let hi = lo + 31;
                            watchpoints.clear();
                            watchpoints.add_vdp_watch(
                                WatchSpace::Vram,
                                lo..=hi,
                                WatchOp::Write,
                                format!("tile ${:03X}", cell.tile),
                            );
                            watch_armed = true;
                            watched_pixel = Some((x, y));
                            println!(
                                "watching tile ${:03X} (palette {}) @ VRAM ${lo:04X}-${hi:04X} — click ({x},{y})",
                                cell.tile, cell.palette
                            );
                        }
                        None => {
                            println!(
                                "pixel ({x},{y}) is a sprite/backdrop dot — no tile watch this slice (follow-up)"
                            );
                        }
                    }
                }
            }
        }

        // W dumps the recorded hits; C disarms the watch (back to the fast null-sink path).
        if window.is_key_pressed(Key::W, KeyRepeat::No) {
            dump_hits(&watchpoints);
        }
        if window.is_key_pressed(Key::C, KeyRepeat::No) {
            watchpoints.clear();
            watch_armed = false;
            watched_pixel = None;
            println!("watch cleared — back to the fast (null-sink) run path");
        }

        // The pad is sampled live every frame; set_pad is the sole, deterministic input path into the core.
        sys.set_pad(0, poll_pad(&window));

        // Advance when running, or on an explicit step request while paused.
        if !paused || step {
            // With audio live (SY-5b): drive every frame through the AudioAndWatch composite (audio + the
            // optional armed watch), then drain that frame's PCM and push it into the ring for the cpal
            // callback. The composite borrows `sink` for the run and is dropped before the drain re-borrow.
            #[cfg(feature = "audio")]
            {
                if let Some(a) = audio.as_mut() {
                    {
                        let mut sink = audio::AudioAndWatch {
                            audio: &mut a.sink,
                            watch: watch_armed.then_some(&mut watchpoints),
                        };
                        sys.run_frames_with_sink(1, &mut sink);
                    }
                    let pcm = a.sink.drain();
                    audio::push_frame(&mut a.prod, &pcm);
                } else if watch_armed {
                    // Audio disabled at runtime (no device): same video-only path as a no-audio build. Only
                    // pay for the recording sink when a watch is armed; otherwise the fast null-sink path.
                    sys.run_frames_with_sink(1, &mut watchpoints);
                } else {
                    sys.run_frames(1);
                }
            }
            // No-audio build: today's exact loop — recording sink only when a watch is armed, else the fast
            // null-sink path so idle stays at 60 fps.
            #[cfg(not(feature = "audio"))]
            {
                if watch_armed {
                    sys.run_frames_with_sink(1, &mut watchpoints);
                } else {
                    sys.run_frames(1);
                }
            }
            frame += 1;
        }

        // Native-resolution frame (width re-queried in case the game switched H32↔H40); window upscales it.
        let width = render_into(&sys, &mut buf);
        // Optional debug marker: a contrasting crosshair at the watched pixel so the live driver can confirm
        // the click landed where intended (bounds-guarded against an H40→H32 mode switch since the click).
        if let Some((wx, wy)) = watched_pixel {
            draw_crosshair(&mut buf, width, wx, wy);
        }
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

    // On quit, flush the final in-progress frame once (harmless if the ring is already draining to a closing
    // device). The cpal stream stops when `audio` — and with it the bound Stream — drops at scope end.
    #[cfg(feature = "audio")]
    if let Some(a) = audio.as_mut() {
        a.sink.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// H40 (native width 320) fills the window exactly: no pillarbox, every axis is a plain divide-by-scale.
    #[test]
    fn h40_maps_full_window_without_pillarbox() {
        let (scale, width) = (3, 320);
        assert_eq!(window_to_native(0.0, 0.0, scale, width), Some((0, 0)));
        // Bottom-right physical pixel of the 960x672 window maps to the last native dot (319, 223).
        assert_eq!(
            window_to_native(959.0, 671.0, scale, width),
            Some((319, 223))
        );
        // One row/column past the frame is rejected.
        assert_eq!(window_to_native(960.0, 0.0, scale, width), None);
        assert_eq!(window_to_native(0.0, 672.0, scale, width), None);
    }

    /// H32 (native width 256) is centered in the 320-wide window: a 32-native-pixel (= 96-window-pixel at 3x)
    /// pillarbox on each side. Clicks inside the box map to no native pixel; the first/last content columns
    /// map to native x 0 / 255.
    #[test]
    fn h32_pillarbox_is_rejected_and_content_maps() {
        let (scale, width) = (3, 256);
        let box_px = 32 * scale; // 96 window px of left pillarbox
                                 // Anywhere in the left box → None.
        assert_eq!(window_to_native(0.0, 100.0, scale, width), None);
        assert_eq!(
            window_to_native((box_px - 1) as f32, 100.0, scale, width),
            None
        );
        // First content column (window x = box_px) → native x 0.
        assert_eq!(
            window_to_native(box_px as f32, 0.0, scale, width),
            Some((0, 0))
        );
        // Last content column: window x = box_px + (255 * scale) → native x 255.
        let last = box_px + 255 * scale;
        assert_eq!(
            window_to_native(last as f32, 0.0, scale, width),
            Some((255, 0))
        );
        // One native column past content (into the right box) → None.
        let past = box_px + 256 * scale;
        assert_eq!(window_to_native(past as f32, 0.0, scale, width), None);
    }

    /// Negative coordinates (a click reported outside the top-left) and a zero scale are rejected, not panic.
    #[test]
    fn out_of_range_inputs_are_none() {
        assert_eq!(window_to_native(-1.0, 10.0, 3, 320), None);
        assert_eq!(window_to_native(10.0, -1.0, 3, 320), None);
        assert_eq!(window_to_native(10.0, 10.0, 0, 320), None);
    }

    /// The crosshair writer never indexes out of the frame buffer, including a watched pixel that is now
    /// outside a narrower (post-mode-switch) frame, and it does mutate in-bounds pixels.
    #[test]
    fn crosshair_is_bounds_safe() {
        let width = 320;
        let mut buf = vec![0u32; width * HEIGHT];
        // Center pixel: the plus mutates it and its neighbours.
        draw_crosshair(&mut buf, width, 160, 112);
        assert_ne!(buf[112 * width + 160], 0, "center inverted");
        // Watched pixel beyond the current (narrower) frame: no-op, no panic.
        let mut narrow = vec![0u32; 256 * HEIGHT];
        draw_crosshair(&mut narrow, 256, 300, 10); // x 300 >= width 256
        assert!(
            narrow.iter().all(|&p| p == 0),
            "out-of-frame click drew nothing"
        );
        // A pixel at the very corner clips its off-buffer arms without panicking.
        draw_crosshair(&mut buf, width, 0, 0);
    }

    /// Design §7 Test 7 — the no-device fallback. `build_audio(None)` is the graceful path taken in a
    /// headless, /dev/snd-less environment: it must return `None` (audio disabled → video-only) and **never
    /// panic**. Injecting `None` makes this deterministic regardless of whether the host running the tests
    /// has a sound card.
    #[cfg(feature = "audio")]
    #[test]
    fn build_audio_without_device_is_video_only_not_a_panic() {
        assert!(
            build_audio(None).is_none(),
            "no output device must disable audio (video-only), never panic"
        );
    }

    /// `start_audio()` — the real host-enumeration entry point — must also be panic-free. In THIS build
    /// environment there is no `/dev/snd`, so it returns `None`; on a machine with a sound card it may return
    /// `Some`. Either way the contract under test is that the call never panics.
    #[cfg(feature = "audio")]
    #[test]
    fn start_audio_never_panics() {
        let _ = start_audio();
    }
}
