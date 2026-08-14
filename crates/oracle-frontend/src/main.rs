//! Live windowed frontend for the oracle-core Genesis / Mega Drive core — the milestone's "watchable and
//! interactive" substrate (`docs/plans/2026-07-21-s4-boot-milestone.md`). Where `boot_rom`/`motion_run`
//! wrap [`System`]'s run loop and dump frames to files, this wraps the identical loop in a window: poll the
//! host keyboard *and* any connected gamepads → merge them into a [`Pad`] per player → `set_pad(port, pad)` →
//! `run_frames(1)` → blit the 224-line frame → throttle to ~60 fps.
//!
//! This crate deliberately owns *all* the non-determinism (real-time keyboard/gamepad, wall-clock throttle) so
//! the core stays the "deterministic, no-I/O" artifact its Cargo description promises. The core is driven only
//! through its existing public API — nothing here adds a core method. (Audio was once out of scope; it
//! arrived with Phase SY-5 and lives behind the default-on `audio` feature — see the `audio` module and
//! `build_audio`.)
//!
//! Upgrade path (not this slice): swap minifb for `pixels` + `winit` when GPU-composited debug overlays
//! (watchpoint highlights, bus-legality heatmaps — `docs/2026-07-20-diagnostic-tooling-ideas.md`) are wanted.
//!
//! Usage: `cargo run --release -p oracle-frontend -- <rom.bin> [--scale N]`
//!
//! ## Controls
//!
//! The keyboard drives **Player 1** only. Gamepads (feature `gamepad`, on by default) drive Player 1 and
//! Player 2 — the first connected controller takes port 0, the second port 1, hotplugged either way. Keyboard
//! and gamepad are merged per button (logical OR), so the keyboard never goes dead while a pad is plugged in,
//! and no controller at all leaves behaviour exactly as it was: keyboard-only Player 1.
//!
//! | Host key          | Emulated input / action |
//! |-------------------|-------------------------|
//! | Arrow keys        | D-pad (P1)     |
//! | A / S / D         | A / B / C (P1) |
//! | Enter             | Start (P1)     |
//! | Space             | pause / resume |
//! | `.` (period)      | single-frame step (while paused) |
//! | F1                | soft-reset the console (SRAM contents preserved, as on real hardware) |
//! | F5                | re-read the ROM file from disk and reset — the edit-assemble-test loop |
//! | F2                | save state to the current slot |
//! | F4                | load state from the current slot |
//! | F6 / F7           | previous / next save-state slot |
//! | 0 – 9             | select save-state slot directly |
//! | `-` / `=`         | output volume down / up (audio builds; repeats while held) |
//! | M                 | mute toggle (audio builds; remembers the volume level) |
//! | Left mouse click  | watch the VRAM tile under the clicked pixel ("who wrote this tile?") |
//! | W                 | dump recorded watch hits (seq/frame/pc/addr/old→new/via) + drop count to stdout |
//! | C                 | clear the watch (return to the fast null-sink run path) |
//! | Esc / window-close| quit           |
//!
//! The gamepad layout (face buttons → A/B/C, Start, d-pad and left stick → directions, analog deadzone) lives
//! in one place — the mapping tables at the top of the `gamepad` module — so remapping means editing those
//! tables.
//!
//! ## Reset and ROM reload
//!
//! `F1` drives the core's [`System::reset`] — the same `/RESET` sequence the frontend already runs at boot,
//! which keeps the cartridge and its battery-backed SRAM and re-runs the vector fetch. `F5` additionally
//! re-reads the ROM **file**, so rebuilding a ROM and testing it costs a key press instead of a relaunch.
//!
//! Both are more than a single core call, and every extra step is there to stop something being lost:
//!
//! * **Battery data first.** `reset` clears `sram_dirty` and `load_rom` re-provisions a *zeroed* buffer, so
//!   anything the guest saved inside the autosave debounce window would silently never reach disk. Both paths
//!   flush the pending `.srm` first ([`flush_pending_srm`]). If that write fails, reset re-arms the debounce
//!   to retry (the bytes survive a reset, so a retry writes the right thing) and reload **aborts** (the bytes
//!   do not survive `load_rom`, so there would be nothing left to retry with).
//! * **The `.srm` is re-applied after a reload,** because `load_rom` zeroes the buffer it just sized from the
//!   new header. The path is derived from the ROM *path*, which a reload does not change.
//! * **The ROM fingerprint is re-derived,** so save states written against the previous build are refused
//!   (`StateError::Rom`) rather than restoring a machine that still carries the old cartridge bytes — a state
//!   file is a snapshot of a whole machine, ROM included.
//! * **The audio sink is rebuilt** ([`audio::resync_sink`]) because both paths rewind the master clock, and a
//!   sink carrying a stale high frame index renders nothing at all until the machine catches back up.
//!
//! ## Save states
//!
//! Ten numbered slots written next to the ROM (`…/foo.bin` slot 3 → `…/foo.state3`), the same naming rule the
//! `.srm` battery save uses. The container — magic, version, a derived machine-layout fingerprint, the ROM's
//! fingerprint, and a payload checksum — lives in [`save_state`]; a stale or corrupt file is refused with a
//! message and the running machine keeps going untouched. Loading also resynchronises audio (the queued
//! samples, and the sink's own frame clock, belong to a timeline that no longer exists) and, because the
//! snapshot carries the cartridge SRAM backwards with it, first persists any pending `.srm` and then cancels
//! the autosave debounce — so a state load never rewinds the on-disk battery. See the load handler below.
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

// Host gamepad input (gilrs) — frontend-only, feature-gated exactly like `audio`, and degrading to
// keyboard-only on every failure path. See the module docs for the mapping tables.
#[cfg(feature = "gamepad")]
mod gamepad;

// User-facing save states — the versioned/checksummed container around the core's snapshot/restore pair.
mod save_state;

// Slice S2 — `.srm` battery-save persistence (frontend-only file I/O around the core's SRAM buffer).
mod sram_file;

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

/// Number keys 0-9, in slot order: pressing one selects that save-state slot directly. Indexed by slot, so
/// the array length must stay equal to [`save_state::SLOT_COUNT`] (asserted in the tests below).
const SLOT_KEYS: [Key; save_state::SLOT_COUNT] = [
    Key::Key0,
    Key::Key1,
    Key::Key2,
    Key::Key3,
    Key::Key4,
    Key::Key5,
    Key::Key6,
    Key::Key7,
    Key::Key8,
    Key::Key9,
];

/// Step the save-state slot by `delta`, wrapping over `0..SLOT_COUNT` in both directions (F6 = -1, F7 = +1).
fn next_slot(slot: usize, delta: isize) -> usize {
    let n = save_state::SLOT_COUNT as isize;
    (((slot as isize + delta) % n + n) % n) as usize
}

/// Build the Player-1 [`Pad`] from the host keyboard: arrows = D-pad, A/S/D = A/B/C, Enter = Start. The
/// keyboard is Player 1 only; Player 2 comes from a second gamepad (see the `gamepad` module).
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

/// Live host-audio state (Phase SY-5b). Held for the whole run: the persistent synth `AudioSink` (advanced
/// exactly one frame per loop iteration by the composite sink, then drained), the SPSC ring producer the
/// drained PCM is pushed into, the ring-flush flag shared with the audio callback, and the kept-alive cpal
/// [`Stream`](cpal::Stream) — dropping the stream stops playback, so it must outlive the loop.
#[cfg(feature = "audio")]
struct AudioState {
    sink: oracle_core::synth::AudioSink,
    prod: audio::AudioProd,
    /// Raised by the emulation thread whenever the machine's timeline jumps (state load, soft reset, ROM
    /// reload — see [`resync_audio`]); the next audio callback drops the whole ring backlog (see
    /// [`audio::fill_output`]). The main thread owns only the producer half, so it cannot drain the ring
    /// itself — this flag is the hand-off.
    flush: std::sync::Arc<std::sync::atomic::AtomicBool>,
    _stream: cpal::Stream,
}

/// Put audio back in step with a machine whose timeline has just jumped — a save-state load, a soft reset, or
/// a ROM reload. No-op when audio is disabled (no device).
///
/// Two things have to happen, for two different reasons:
///
/// 1. **Drop the ring backlog.** Up to [`audio::RING_FRAMES`] frames of already-rendered PCM belong to the
///    timeline the machine has left; playing them out would be an audible burp of the past. The emulation
///    thread owns only the producer half of the ring, so it raises the shared flag the callback checks
///    ([`audio::fill_output`]) rather than draining it itself.
/// 2. **Rebuild the sink** ([`audio::resync_sink`]). All three jumps can move the master clock *backwards*,
///    and the sink renders only on an advancing frame index — a stale one would go silent indefinitely. See
///    that function's docs for the full mechanism.
#[cfg(feature = "audio")]
fn resync_audio(audio: Option<&mut AudioState>) {
    if let Some(a) = audio {
        audio::resync_sink(&mut a.sink);
        a.flush.store(true, std::sync::atomic::Ordering::Release);
    }
}

/// Persist any battery data the guest has written that the autosave debounce has not yet flushed, ahead of an
/// operation that would otherwise lose it (quit, state load, reset, ROM reload). `why` names that operation
/// and appears verbatim in the log line — "on quit", "before the reset", ….
///
/// Returns `true` when the on-disk `.srm` is up to date, **including** the common case of nothing being
/// pending (a cart that has never saved never touches the disk — the `sram_used` gate). `false` means the
/// write failed and memory still holds the only copy, which the callers treat differently according to
/// whether their operation destroys that copy: the reset path re-arms its debounce to retry, the ROM-reload
/// path aborts, and quit can only report it.
fn flush_pending_srm(
    sys: &System,
    path: &std::path::Path,
    countdown: Option<u32>,
    why: &str,
) -> bool {
    if !(sys.sram_used() && (sys.sram_dirty() || countdown.is_some())) {
        return true; // nothing pending — disk already matches memory
    }
    match sram_file::save_srm(path, sys.sram()) {
        Ok(()) => {
            println!(
                "SRAM: saved {} bytes to {} {why}",
                sys.sram().len(),
                path.display()
            );
            true
        }
        Err(e) => {
            eprintln!("SRAM: save {why} failed ({}): {e}", path.display());
            false
        }
    }
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
/// The stream's callback is [`audio::fill_output`]: it pops the ring's consumer into the device buffer,
/// zero-filling any underrun tail (design §2.5), handles the device channel count (stereo copy / mono
/// average / wide-device first-two-lanes, design §3.3), and honours the shared save-state flush flag.
#[cfg(feature = "audio")]
fn build_audio(device: Option<cpal::Device>) -> Option<AudioState> {
    use cpal::traits::{DeviceTrait, StreamTrait};

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
    // Shared with the callback so a save-state load can drop the (now bogus) queued backlog.
    let flush = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cb_flush = std::sync::Arc::clone(&flush);

    let data_cb = move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
        audio::fill_output(&mut cons, out, channels, &cb_flush);
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
        flush,
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

    // Identity of this cartridge, computed before the ROM moves into the core. Every save state records it,
    // so a state written while running a *different* game is refused instead of silently swapping the ROM
    // (the snapshot carries the cartridge bytes with it). Re-derived by the F5 reload, which deliberately
    // invalidates every state written against the previous build.
    let mut rom_fp = save_state::rom_fingerprint(&rom);

    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);

    // Slice S2/S4 — battery-save persistence. Since S4 every cart has a provisioned SRAM buffer (a valid "RA"
    // header, else the standard fallback page), so we always load a `.srm` when one exists on disk — the save
    // data must be present before the game first reads it. But we only ever *write* a `.srm` for carts that
    // actually saved (`sram_used()`), so a pure-ROM cart (e.g. s4.soundtest.bin) still creates no file. Load
    // before reset — a soft reset preserves SRAM contents (S1), so ordering is free.
    let srm_path = sram_file::srm_path_for(std::path::Path::new(&args.rom_path));
    if let Some(bytes) = sram_file::load_srm(&srm_path) {
        sys.load_sram(&bytes);
        println!(
            "SRAM: loaded {} bytes from {}",
            bytes.len(),
            srm_path.display()
        );
    } else {
        println!(
            "SRAM: no save yet at {} (a `.srm` is written only once the game saves)",
            srm_path.display()
        );
    }

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
        "window {win_w}x{win_h} (up to {MAX_WIDTH}x{HEIGHT} @ {}x) — keyboard (P1): arrows=D-pad, A/S/D=A/B/C, Enter=Start; Space=pause, .=step, click=watch-tile, W=dump, C=clear, Esc=quit",
        args.scale
    );
    println!(
        "save states: F2=save, F4=load, F6/F7=prev/next slot, 0-9=pick slot ({} slots, written next to the ROM as `{}`)",
        save_state::SLOT_COUNT,
        save_state::state_path_for(std::path::Path::new(&args.rom_path), 0).display()
    );
    println!(
        "machine: F1=soft reset, F5=reload the ROM from disk (re-read {} and reset)",
        args.rom_path
    );
    #[cfg(feature = "audio")]
    println!(
        "audio: -/= volume down/up ({} steps, starts at full), M=mute",
        audio::VOLUME_STEPS
    );

    // Host gamepads: `None` = gilrs unavailable → keyboard-only, never a panic (same contract as `start_audio`
    // below). `Some` with no controller attached is normal — one plugged in later is picked up by `poll`.
    // Detected controllers are announced by `Gamepads::new` itself, one line per pad.
    #[cfg(feature = "gamepad")]
    let mut gamepads = gamepad::Gamepads::new();

    // Start the host audio stream (Phase SY-5b). `None` = no device / build failure → video-only, never a
    // panic (the default in a headless, /dev/snd-less environment). When present, its persistent AudioSink is
    // advanced one frame per iteration below and drained→pushed into the ring the cpal callback consumes.
    #[cfg(feature = "audio")]
    let mut audio = start_audio();

    let mut buf: Vec<u32> = Vec::with_capacity(MAX_WIDTH * HEIGHT);
    let mut paused = false;
    let mut frame: u64 = 0;

    // Slice S2 autosave throttle: when the guest has dirtied SRAM, wait this many frames of quiescence before
    // writing the `.srm`, so a burst of saves coalesces into one file write (~2 s at 60 fps).
    const SRAM_AUTOSAVE_DEBOUNCE_FRAMES: u32 = 120;
    let mut sram_save_countdown: Option<u32> = None;

    // The pixel-attribution watch — a *caller-owned* sink (the core never stores it, keeping this slice
    // zero-diff on oracle-core). `watch_armed` mirrors "a VDP watch is registered" so the run loop can stay on
    // the fast null-sink path when nothing is being watched. `watched_pixel` drives the on-screen crosshair.
    let mut watchpoints = Watchpoints::new(WATCH_CAP);
    let mut watch_armed = false;
    let mut watched_pixel: Option<(u16, u16)> = None;
    let mut prev_mouse_down = false;

    // The save-state slot F2/F4 act on; F6/F7 step it, 0-9 pick it directly.
    let mut state_slot: usize = 0;

    // Output volume (audio builds only — with no audio there is nothing to attenuate, and the state would be
    // dead code). `volume` is a step in `0..=audio::VOLUME_STEPS`, defaulting to full so behaviour is
    // unchanged until the user touches it; `muted` is an independent toggle so unmuting restores the level.
    #[cfg(feature = "audio")]
    let mut volume: u8 = audio::VOLUME_STEPS;
    #[cfg(feature = "audio")]
    let mut muted = false;

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

        // --- Save states (edge-triggered like every control above; usable while paused too). ---
        // Slot selection: F6/F7 step, 0-9 pick directly.
        if window.is_key_pressed(Key::F6, KeyRepeat::No) {
            state_slot = next_slot(state_slot, -1);
            println!("state: slot {state_slot} selected");
        }
        if window.is_key_pressed(Key::F7, KeyRepeat::No) {
            state_slot = next_slot(state_slot, 1);
            println!("state: slot {state_slot} selected");
        }
        for (n, key) in SLOT_KEYS.iter().enumerate() {
            if window.is_key_pressed(*key, KeyRepeat::No) {
                state_slot = n;
                println!("state: slot {state_slot} selected");
            }
        }

        // F2 = save, F4 = load, both on the currently selected slot. The path is built inside each arm so the
        // idle loop allocates nothing.
        if window.is_key_pressed(Key::F2, KeyRepeat::No) {
            let state_path =
                save_state::state_path_for(std::path::Path::new(&args.rom_path), state_slot);
            match save_state::save(&state_path, &sys, rom_fp) {
                Ok(n) => println!(
                    "state: saved {n} bytes to slot {state_slot} ({})",
                    state_path.display()
                ),
                Err(e) => eprintln!("state: save to {} failed: {e}", state_path.display()),
            }
        }
        if window.is_key_pressed(Key::F4, KeyRepeat::No) {
            let state_path =
                save_state::state_path_for(std::path::Path::new(&args.rom_path), state_slot);
            match save_state::load(&state_path, rom_fp) {
                Ok(loaded) => {
                    // SRAM rides the snapshot, so the restore is about to roll the battery buffer backwards.
                    // Flush any battery data the guest has written but the debounce has not yet persisted
                    // *first*, exactly like the quit path — otherwise a state load would silently destroy a
                    // real in-game save that happened to be a second old. (A failed flush is reported by the
                    // helper; the load still proceeds, since the restored buffer is a *valid* older save and
                    // the user explicitly asked to go back to it.)
                    flush_pending_srm(
                        &sys,
                        &srm_path,
                        sram_save_countdown,
                        "before the state load",
                    );

                    // Whole-value swap: `save_state::load` builds a complete machine or returns `Err`, so
                    // there is no window in which a half-restored `System` is running.
                    sys = loaded;

                    // A restore rewinds the master clock, so audio has to be resynchronised: the queued PCM
                    // belongs to the timeline we just left, and the sink's frame clock would otherwise sit
                    // above the restored one and render nothing at all. Rebuilding also drops the synth's
                    // register shadow — a brief gap until the driver rewrites its patches, versus the
                    // indefinite silence of a stale sink.
                    #[cfg(feature = "audio")]
                    resync_audio(audio.as_mut());

                    // …and now the other half of the SRAM interaction (bytes, `sram_dirty` and `sram_used`
                    // are all `System` fields — proven empirically in `save_state`'s tests): (1) cancel the
                    // pending autosave, which would otherwise fire moments later and overwrite the `.srm`
                    // we just flushed with the *older*, restored contents; (2) clear the restored dirty
                    // flag, so the rolled-back SRAM reaches disk only once the game actually saves again.
                    // Net effect: the on-disk battery is never rewound by a state load, only by a real
                    // in-game save.
                    sram_save_countdown = None;
                    sys.clear_sram_dirty();

                    // The watch (if armed) stays armed on the same VRAM range; hits recorded before the load
                    // remain in the log — `C` clears them.
                    println!(
                        "state: loaded slot {state_slot} from {} (frame counter continues at {frame})",
                        state_path.display()
                    );
                }
                Err(e) => eprintln!("state: load of slot {state_slot} failed: {e}"),
            }
        }

        // --- Machine control: F1 soft-resets, F5 re-reads the ROM from disk and resets (module doc). ---
        if window.is_key_pressed(Key::F1, KeyRepeat::No) {
            // `System::reset` keeps the SRAM *contents* but clears `sram_dirty`, so an in-flight save would
            // lose the only signal that it still needs writing. Persist it first; if that fails the bytes
            // survive the reset unchanged, so re-arming the debounce retries with exactly the right image.
            if flush_pending_srm(&sys, &srm_path, sram_save_countdown, "before the reset") {
                sram_save_countdown = None;
            } else {
                sram_save_countdown = Some(SRAM_AUTOSAVE_DEBOUNCE_FRAMES);
            }
            sys.reset();
            frame = 0; // the machine's own clock restarts, so the displayed counter follows it
            #[cfg(feature = "audio")]
            resync_audio(audio.as_mut());
            println!("reset: soft reset — SRAM contents preserved, as on real hardware");
        }
        if window.is_key_pressed(Key::F5, KeyRepeat::No) {
            // Read the file first: a rebuild that failed (or is still being written) must leave the running
            // machine — and its battery data — completely untouched.
            match std::fs::read(&args.rom_path) {
                Err(e) => eprintln!(
                    "reload: cannot read ROM {} ({e}) — still running the previous image",
                    args.rom_path
                ),
                Ok(bytes) => {
                    // Unlike a reset, `load_rom` re-provisions a *zeroed* SRAM buffer from the new header and
                    // clears `sram_used`/`sram_dirty` — unflushed battery data would be destroyed outright,
                    // with nothing left to retry from. So a failed flush aborts the reload.
                    if !flush_pending_srm(
                        &sys,
                        &srm_path,
                        sram_save_countdown,
                        "before the ROM reload",
                    ) {
                        eprintln!(
                            "reload: ABORTED — unsaved battery data could not be written to {}, and reloading \
                             would zero it. Fix the write error and press F5 again.",
                            srm_path.display()
                        );
                    } else {
                        println!(
                            "reload: re-read {} bytes from {}",
                            bytes.len(),
                            args.rom_path
                        );
                        // The cartridge identity changes with its bytes. Re-deriving it makes every state
                        // written against the previous build fail with `StateError::Rom` — which is the point:
                        // a state carries the whole machine, so restoring one would put the old ROM back.
                        rom_fp = save_state::rom_fingerprint(&bytes);
                        sys.load_rom(bytes);
                        // `load_rom` zeroed the buffer it just sized from the new header, so re-apply the
                        // on-disk battery image. The `.srm` path comes from the ROM *path*, unchanged here.
                        if let Some(saved) = sram_file::load_srm(&srm_path) {
                            sys.load_sram(&saved);
                            println!(
                                "SRAM: re-loaded {} bytes from {}",
                                saved.len(),
                                srm_path.display()
                            );
                        }
                        sram_save_countdown = None; // the fresh buffer is clean and matches disk
                        sys.reset(); // `load_rom` only swaps the cartridge; this runs the /RESET sequence
                        frame = 0;
                        #[cfg(feature = "audio")]
                        resync_audio(audio.as_mut());
                    }
                }
            }
        }

        // --- Output volume (audio builds only). Repeat-on-hold for the level so `-`/`=` ramp smoothly; the
        // mute toggle is edge-only so holding M does not flap it. ---
        #[cfg(feature = "audio")]
        {
            let mut changed = false;
            if window.is_key_pressed(Key::Minus, KeyRepeat::Yes) {
                volume = volume.saturating_sub(1);
                changed = true;
            }
            if window.is_key_pressed(Key::Equal, KeyRepeat::Yes) {
                volume = (volume + 1).min(audio::VOLUME_STEPS);
                changed = true;
            }
            if window.is_key_pressed(Key::M, KeyRepeat::No) {
                muted = !muted;
                changed = true;
            }
            if changed {
                println!(
                    "volume: {volume}/{}{}",
                    audio::VOLUME_STEPS,
                    if muted { "  [MUTED]" } else { "" }
                );
            }
        }

        // Inputs are sampled live every frame; set_pad is the sole, deterministic input path into the core.
        // Player 1 = keyboard OR gamepad 1 (merged per button, so neither source can suppress the other);
        // Player 2 = gamepad 2 only, and an all-released Pad when there is none — which is exactly the state
        // port 1 already had before that slice, so a one-player session is unaffected.
        // `mut` is used only by the `gamepad` arm below; a no-gamepad build never writes it.
        #[allow(unused_mut)]
        let mut player = [poll_pad(&window), Pad::default()];
        #[cfg(feature = "gamepad")]
        if let Some(g) = gamepads.as_mut() {
            let pads = g.poll();
            for (p, from_pad) in player.iter_mut().zip(pads) {
                *p = gamepad::merge_pads(*p, from_pad);
            }
        }
        sys.set_pad(0, player[0]);
        sys.set_pad(1, player[1]);

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
                    // The volume/mute setting is applied here, on the producer side, so the real-time
                    // callback stays a pure copy (see `audio::push_frame`).
                    audio::push_frame(&mut a.prod, &pcm, audio::gain_for(volume, muted));
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

        // Slice S2/S4 autosave: when the guest has dirtied SRAM, arm a debounce countdown and flush the `.srm`
        // once it elapses (coalescing a burst of saves into one write). Guarded on `sram_used()` (S4) so only
        // carts that actually saved touch the disk — the header-less fallback buffer never fabricates a file.
        // A save failure is logged, not fatal.
        if sys.sram_used() {
            if sys.sram_dirty() && sram_save_countdown.is_none() {
                sram_save_countdown = Some(SRAM_AUTOSAVE_DEBOUNCE_FRAMES);
            }
            if let Some(n) = sram_save_countdown {
                if n == 0 {
                    match sram_file::save_srm(&srm_path, sys.sram()) {
                        Ok(()) => {
                            sys.clear_sram_dirty();
                            println!(
                                "SRAM: saved {} bytes to {}",
                                sys.sram().len(),
                                srm_path.display()
                            );
                        }
                        Err(e) => eprintln!("SRAM: save failed ({}): {e}", srm_path.display()),
                    }
                    sram_save_countdown = None;
                } else {
                    sram_save_countdown = Some(n - 1);
                }
            }
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

    // Slice S2/S4 — final save on quit: persist any SRAM the guest dirtied since the last autosave (or that a
    // pending debounce never reached), so a save made just before closing the window is never lost. The helper
    // is gated on `sram_used()` (S4) so only a cart that actually saved writes a file.
    flush_pending_srm(&sys, &srm_path, sram_save_countdown, "on quit");
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

    /// **The premises the F1/F5 SRAM handling is built on**, pinned against the core rather than assumed
    /// from its docs — the two paths deliberately behave differently, and only because these two facts differ:
    ///
    /// * `System::reset` keeps the SRAM *bytes* and the `sram_used` latch but **clears `sram_dirty`**. So a
    ///   reset silently discards the *signal* that a save is pending while keeping the data — hence F1 flushes
    ///   first, and can safely re-arm the debounce to retry if that flush fails.
    /// * `System::load_rom` re-provisions a **zeroed** buffer and clears both flags. So a reload discards the
    ///   *data* — hence F5 aborts outright when its flush fails, because a retry would have nothing to write.
    ///
    /// The cart is a hand-assembled ROM that enables `$A130F1` SRAM access and stores one byte, mirroring
    /// `save_state`'s `snapshot_carries_sram_bytes_and_its_dirty_bookkeeping`.
    #[test]
    fn reset_keeps_sram_bytes_but_load_rom_zeroes_them() {
        //   move.b #$01,$00A130F1   ; SRAM access on
        //   move.b #$5A,$00200001   ; store into the fallback SRAM window (odd byte lane)
        //   bra.s  *                ; spin
        let mut rom = vec![0u8; 0x400];
        rom[0x00..0x04].copy_from_slice(&0x00FF_0000u32.to_be_bytes()); // initial SSP
        rom[0x04..0x08].copy_from_slice(&0x0000_0200u32.to_be_bytes()); // initial PC
        let code: [u8; 18] = [
            0x13, 0xFC, 0x00, 0x01, 0x00, 0xA1, 0x30, 0xF1, // move.b #$01,$00A130F1
            0x13, 0xFC, 0x00, 0x5A, 0x00, 0x20, 0x00, 0x01, // move.b #$5A,$00200001
            0x60, 0xFE, // bra.s *
        ];
        rom[0x200..0x200 + code.len()].copy_from_slice(&code);

        let mut sys = System::new(0x5EED);
        sys.load_rom(rom.clone());
        sys.reset();
        sys.run_frames(1);
        assert_eq!(sys.sram()[0], 0x5A, "the guest stored a byte");
        assert!(
            sys.sram_dirty() && sys.sram_used(),
            "…and both flags latched"
        );

        // F1's premise: the bytes survive, the "needs writing" flag does not.
        sys.reset();
        assert_eq!(
            sys.sram()[0],
            0x5A,
            "a soft reset preserves SRAM contents, as on real hardware"
        );
        assert!(sys.sram_used(), "the `this cart saves` latch also survives");
        assert!(
            !sys.sram_dirty(),
            "…but `sram_dirty` is cleared — so an unflushed save would never be written again"
        );

        // F5's premise: reloading the cartridge wipes the buffer outright, flags and all.
        sys.load_rom(rom);
        assert!(
            sys.sram().iter().all(|&b| b == 0),
            "load_rom re-provisions a zeroed buffer — a reload destroys unflushed battery data"
        );
        assert!(
            !sys.sram_used() && !sys.sram_dirty(),
            "and clears both flags"
        );
    }

    /// Slot stepping wraps in both directions and never leaves `0..SLOT_COUNT`, and every slot has a
    /// distinct number key.
    #[test]
    fn slot_stepping_wraps_and_covers_every_key() {
        let last = save_state::SLOT_COUNT - 1;
        assert_eq!(next_slot(0, 1), 1);
        assert_eq!(
            next_slot(0, -1),
            last,
            "stepping back from 0 wraps to the last"
        );
        assert_eq!(next_slot(last, 1), 0, "stepping past the last wraps to 0");
        for s in 0..save_state::SLOT_COUNT {
            assert!(next_slot(s, 1) < save_state::SLOT_COUNT);
            assert!(next_slot(s, -1) < save_state::SLOT_COUNT);
        }
        // A full lap of +1 returns to the start, so every slot is reachable by stepping alone.
        let mut s = 0;
        for _ in 0..save_state::SLOT_COUNT {
            s = next_slot(s, 1);
        }
        assert_eq!(s, 0);

        let keys: std::collections::BTreeSet<_> = SLOT_KEYS.iter().map(|k| *k as u32).collect();
        assert_eq!(
            keys.len(),
            save_state::SLOT_COUNT,
            "each slot needs its own distinct number key"
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
