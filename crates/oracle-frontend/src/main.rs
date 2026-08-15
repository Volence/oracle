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
//! | W                 | dump recorded watch hits (seq/frame/pc/addr/old→new/via, PC symbolised) + drop count |
//! | C                 | clear the watch (stop recording write hits) |
//! | Esc / window-close| quit           |
//!
//! The gamepad layout (face buttons → A/B/C, Start, d-pad and left stick → directions, analog deadzone) lives
//! in one place — the mapping tables at the top of the `gamepad` module — so remapping means editing those
//! tables.
//!
//! ## Pixels — why the window is painted from the per-scanline seam
//!
//! The frame shown here is assembled **during** the run, line by line, by an
//! [`oracle_core::scanline_capture::ScanlineCapture`] attached to every `run_frames_with_sink` call — not by
//! reading the VDP once the frame is over. The distinction is not academic: `run_frames(1)` returns in
//! V-Blank, by which time a game has already rewritten CRAM for the *next* frame, so a post-hoc
//! `Vdp::render_line` sweep cannot show any mid-frame palette effect. Sonic 3 & Knuckles' underwater palette
//! split is the loud case — the water came out bright red (the above-water palette) instead of slate blue.
//! The core already rendered every line during the run and threw the pixels away; this frontend simply opts
//! into them (`wants_scanlines`), so nothing in `oracle-core` changed and no currency moved. Measured on the
//! S3K state at 320x224, it is also ~36% *cheaper* per frame than the old path, which rendered every scanline
//! twice.
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
//! * **The symbol table is re-read and re-validated** against the new bytes, because a rebuild moves symbols
//!   and a cached table would keep naming the previous build's addresses while looking perfectly healthy.
//!
//! ## Symbols
//!
//! If a `<rom>.lst` listing sits next to the ROM (`…/s4.bin` → `…/s4.lst`, exactly where
//! `sigil build --emit-lst` puts it) it is loaded at boot and used to name addresses — today, the PC of every
//! watch hit. Loading is opt-in by presence and never load-bearing: absent, unparseable, or belonging to a
//! *different build* all leave the emulator running exactly as before, with raw hex. That last case is a
//! deliberate refusal rather than a best effort — see [`symbol_file`] and [`oracle_core::symbols`].
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
//! [`Watchpoints`] (the core never stores it — this stays a zero-diff, frontend-only slice). The run loop
//! always drives the sink-generic [`System::run_frames_with_sink`] (see "Pixels" above); arming a watch just
//! composes it into that sink. `W` prints the recorded hits; `C` disarms. Sprite /
//! backdrop pixels (`cell == None`) report and arm nothing this slice (a documented follow-up). An on-screen
//! text overlay is out of scope (minifb has no text). Break-on-hit is no longer *blocked*: as of 2026-08-14
//! the core's run loop honours a sink's [`oracle_core::bus::BusEventSink::stop_requested`], so a run can end
//! at the instruction boundary a watch fires on. Wiring that into this loop (pause, then report the
//! [`oracle_core::system::StopRecord`]) is unbuilt, not impossible — the "the core is frame-batched" reason
//! recorded here previously no longer holds.
//!
//! ## Pacing — why the audio device, not the window, decides how many frames run
//!
//! The loop runs **0, 1 or 2** emulated frames per iteration, chosen from the audio ring's occupancy by
//! `audio::frames_to_run`, whose docs carry the measurements (the module is feature-gated, so that is
//! deliberately not an intra-doc link). This is not a refinement; without it the frontend crackles
//! continuously.
//! `minifb::set_target_fps(60)` cannot hold 60: its limiter sleeps `target - elapsed` and only *then*
//! restarts its clock, so every period is `16.667 ms + sleep overshoot` (measured 59.54–59.63 fps here). One
//! emulated frame per iteration produces `735 x 59.63 = 43,826` samples a second into a device that eats
//! 44,100 — a **permanent 0.62 % deficit**, which drains any reservoir no matter how large, pins the ring at
//! empty and makes the output callback silence-fill 8–16 % of its buffers forever.
//!
//! So the *device* is the master clock (the standard emulator arrangement) and minifb keeps pacing
//! presentation. The cost is bounded and small: ~0.43 **video** frames a second dropped from the display
//! (0.7 %), and none duplicated — the drift only ever runs one way.
//! A 0-frame iteration re-presents the retained framebuffer, exactly as a paused one does; a 2-frame
//! iteration presents the second frame whole. Measured: 0 underruns over 30 s at every callback block size.

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
// Opt-in `<rom>.lst` symbol loading — the file half of `oracle_core::symbols`, kept out of the no-I/O core.
mod symbol_file;

use minifb::{Key, KeyRepeat, MouseButton, MouseMode, ScaleMode, Window, WindowOptions};
use oracle_core::bus::Fanout;
use oracle_core::io::Pad;
use oracle_core::scanline_capture::{Retain, ScanlineCapture};
use oracle_core::symbols::SymbolTable;
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

/// Hard ceiling on the [`ScanlineCapture`]'s per-delivery bookkeeping before it is released unconditionally.
/// The steady-state path clears the capture after every clean frame (see the run loop), so this only ever
/// fires on the pathological case where a run keeps ending mid-frame — e.g. a single 68k→VRAM DMA billed as
/// tens of thousands of CPU wait cycles, which can carry the clock past a whole frame's worth of events.
/// Eight frames of `(line, width)` pairs is ~28 KB; without the valve the log grows ~215 KB per emulated
/// second forever (`ScanlineCapture`'s memory note).
const MAX_CAPTURE_LINES: usize = 8 * HEIGHT;

/// Pack the capture's most recently **completed** frame into `buf` at native resolution, returning its width,
/// or `None` when the capture is not holding a whole frame right now (nothing completed yet, or a run that
/// ended mid-frame). `buf` is left untouched on `None`, so the caller re-presents the last good framebuffer
/// instead of flashing an empty one — which is exactly what a paused frontend does every iteration.
///
/// **Why the capture and not `Vdp::render_line`.** `render_line` is a pure, *post-hoc* read of whatever CRAM
/// happens to hold when it is called — and `run_frames(1)` returns in V-Blank, after a game has rewritten the
/// palette for the next frame. Every mid-frame palette effect (S3K's underwater split being the loud one) is
/// therefore invisible to it: the water renders in the above-water palette. The pixels here come from
/// [`Vdp::render_scanline`](oracle_core::vdp::Vdp), delivered line by line *during* the run against the CRAM
/// live at that line, through the sink seam the core already had.
///
/// **Width, and ragged frames.** Width is taken from the capture's own per-line log, never from re-querying
/// the VDP — a post-hoc query answers for whatever mode the chip is in *now*, which after an H32↔H40 switch is
/// the next frame's. A frame is *not* guaranteed rectangular: a game can switch mode part-way down, and S3K
/// does exactly that on the first frame after a soft reset (two 256-px lines, then 222 at 320). So the display
/// width is the width the frame **ended** on — what the VDP is actually scanning out by V-Blank — and shorter
/// lines are padded with black to reach it. Rejecting those frames instead would blank the window for as long
/// as a game kept switching.
fn blit_capture(cap: &ScanlineCapture, buf: &mut Vec<u32>) -> Option<usize> {
    let px = cap.pixels();
    let log = cap.lines();
    if px.is_empty() || log.len() < HEIGHT {
        return None;
    }
    // The completed frame is the last HEIGHT deliveries; the sum check is what proves that (a run that ended
    // mid-frame leaves a *previous* frame in `pixels()` whose lines are no longer the tail of the log).
    let widths = &log[log.len() - HEIGHT..];
    if widths.iter().map(|&(_, w)| w).sum::<usize>() != px.len() {
        return None;
    }
    let width = widths[HEIGHT - 1].1;
    if width == 0 {
        return None;
    }
    buf.clear();
    buf.reserve(width * HEIGHT);
    let mut at = 0;
    for &(_, line_width) in widths {
        let line = &px[at..at + line_width];
        at += line_width;
        for x in 0..width {
            let (r, g, b) = line.get(x).copied().unwrap_or((0, 0, 0));
            buf.push((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b));
        }
    }
    Some(width)
}

/// How far past a symbol a PC may sit before the name stops being useful. Aeon's listings are dense (2,129
/// symbols over ~660 KB, and code far denser than that average, which is dragged out by art blobs), so a PC
/// more than 4 KiB past the nearest label is almost certainly in data or off the end of the image — cases
/// where naming the previous routine would be actively misleading. Beyond this the hit prints raw hex.
const MAX_SYMBOL_DISPLACEMENT: u32 = 0x1000;

/// Print every recorded watch hit (oldest first) and the drop count to stdout. `pc` is annotated with the
/// nearest preceding symbol when a `<rom>.lst` was loaded (`EntryPoint.wait_dma+$1A` — the `$`-scope tree
/// means this names the *local* label inside a long routine, not just its entry). `via` is the CPU-vs-DMA
/// attribution (`Direct` = CPU data-port write, `Dma` = DMA step, `Bus` = a v1 68000 bus access — unused by
/// this slice's VRAM watch but printed faithfully).
fn dump_hits(watchpoints: &Watchpoints, symbols: Option<&SymbolTable>) {
    let hits = watchpoints.hits();
    println!("--- watch hits: {} recorded ---", hits.len());
    for h in hits {
        let via = match h.via {
            WatchVia::Bus => "Bus",
            WatchVia::Direct => "Direct(CPU)",
            WatchVia::Dma => "Dma",
        };
        // Raw hex stays, always: the symbol is an annotation on the address, never a replacement for it.
        let sym = symbols
            .and_then(|t| t.resolve_within(h.pc, MAX_SYMBOL_DISPLACEMENT))
            .map(|r| format!(" {r}"))
            .unwrap_or_default();
        println!(
            "seq {:>6}  frame {:>6}  pc ${:06X}{sym}  addr ${:04X}  ${:X}->${:X}  via {via}",
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
    /// `f32` one emulated frame contributes to the ring at this device's rate — the unit
    /// [`audio::frames_to_run`] measures the ring's occupancy in. Cached from the device rate at build time.
    frame_samples: usize,
    /// Consecutive loop iterations the feedback has already answered "run 0 frames" for. Bounded by
    /// [`audio::MAX_CONSECUTIVE_SKIPS`] so a device that stops consuming cannot back-pressure the emulator
    /// into a permanent standstill.
    skips: usize,
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
    let (mut prod, mut cons) = audio::make_ring(sample_rate);
    // Queue a reservoir of silence *before* the stream is allowed to pop anything, so the first callbacks
    // have something to play while the first emulated frame is still being computed, and the feedback loop
    // starts inside its steady band instead of climbing out of the empty wall (see `audio::preroll_silence`).
    let frame_samples = audio::frame_samples(sample_rate);
    let prerolled = audio::preroll_silence(&mut prod, frame_samples);
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

    println!(
        "audio: {sample_rate} Hz, {channels} ch (f32) — streaming ({} ms pre-roll, {} ms ring)",
        prerolled * 500 / sample_rate.max(1) as usize,
        audio::ring_capacity(&prod) * 500 / sample_rate.max(1) as usize,
    );
    Some(AudioState {
        sink,
        prod,
        frame_samples,
        skips: 0,
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

    // Opt-in symbols. Loaded here, while `rom` is still ours to borrow: the binding check probes the image's
    // `deb2` appendix at the offset the listing's own `EndOfRom` names, so it needs the bytes, not the core.
    // A missing/unusable/mismatched listing is never fatal — the machine runs identically without one.
    let mut symbols = symbol_file::load_symbols(std::path::Path::new(&args.rom_path), &rom);

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

    // The presented framebuffer and its native width. Both are *retained*: a frame that produced no capture
    // (paused, or a run that ended mid-frame) re-presents the last good one rather than a blank. Seeded with
    // a black frame at the VDP's post-reset width so the very first iteration has something valid to show and
    // a click before the first frame resolves against a sane geometry.
    let mut width = sys.vdp().render_line(0).len();
    let mut buf: Vec<u32> = vec![0; width * HEIGHT];
    // Scratch copy used only when the crosshair overlay is active, so `buf` stays a clean frame and the
    // XOR-based crosshair cannot accumulate across the repeated presents of a paused frontend.
    let mut present: Vec<u32> = Vec::new();
    let mut paused = false;
    let mut frame: u64 = 0;

    // The per-scanline pixel path (`F-SCANLINE-CAPTURE`). Attached to **every** run below so the window shows
    // what the VDP drew line by line, against the CRAM live at each line — the only way a mid-frame palette
    // change (S3K's underwater split) can reach the screen. `Retain::LastFrame` holds exactly the frame the
    // run completed. The core is untouched by this: the sink is caller-owned and opts in via
    // `wants_scanlines`, so nothing frozen moves.
    let mut cap = ScanlineCapture::new(Retain::LastFrame);

    // Slice S2 autosave throttle: when the guest has dirtied SRAM, wait this many frames of quiescence before
    // writing the `.srm`, so a burst of saves coalesces into one file write (~2 s at 60 fps).
    const SRAM_AUTOSAVE_DEBOUNCE_FRAMES: u32 = 120;
    let mut sram_save_countdown: Option<u32> = None;

    // The pixel-attribution watch — a *caller-owned* sink (the core never stores it, keeping this slice
    // zero-diff on oracle-core). `watch_armed` mirrors "a VDP watch is registered" so the run loop can stay on
    // the recording sink out of the run when nothing is being watched. `watched_pixel` drives the crosshair.
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
            // The width of the frame *currently on screen* — the one the user clicked into. That is the width
            // the last successful blit reported, not a fresh `render_line` query, which would answer for the
            // mode the VDP is in *now* (a post-hoc read; see `blit_capture`).
            let display_width = width;
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

        // W dumps the recorded hits; C disarms the watch (dropping it back out of the run's sink).
        if window.is_key_pressed(Key::W, KeyRepeat::No) {
            dump_hits(&watchpoints, symbols.as_ref());
        }
        if window.is_key_pressed(Key::C, KeyRepeat::No) {
            watchpoints.clear();
            watch_armed = false;
            watched_pixel = None;
            println!("watch cleared — no longer recording VRAM writes");
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

                    // The scanline capture is pointed at a different machine now, so anything it has buffered
                    // belongs to the timeline we just left. `ScanlineCapture` self-heals when the line stream
                    // restarts, but dropping it here means the very next completed frame is unambiguously the
                    // restored one. `buf` deliberately keeps the old image until that frame arrives.
                    cap.clear();

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
            cap.clear(); // the line stream restarts from the reset vector — drop the pre-reset frame
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
                        // Re-read the listing too. This is the whole reason symbol resolution is a
                        // primitive rather than a lookup the user does once: a rebuild moves symbols, and a
                        // table cached across it names the *previous* build's addresses while looking
                        // perfectly healthy (the suite contract's D7 incident — every symbol shifted +$24
                        // mid-session and a "verified" literal rotted). Re-validated against the new bytes,
                        // so a listing that stopped matching is dropped rather than carried forward.
                        symbols =
                            symbol_file::load_symbols(std::path::Path::new(&args.rom_path), &bytes);
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
                        cap.clear(); // a different cartridge draws a different frame — drop the old one
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

        // How many emulated frames this iteration runs. Normally one; while paused, none unless `.` asked for
        // a single step. With audio live it is whatever the ring's occupancy asks for — 0, 1 or 2 — which is
        // what makes the **audio device** the master clock while minifb keeps pacing presentation.
        //
        // Why it cannot simply be one: minifb's `set_target_fps(60)` limiter sleeps `target - elapsed` and
        // only then restarts its clock, so its period is `16.667 ms + sleep overshoot` and its rate is always
        // *under* 60 (measured 59.54–59.63 fps here). One frame per iteration therefore produces
        // `735 x 59.63 = 43,826` samples/s against a device eating 44,100 — a permanent 0.62 % deficit that
        // pins the ring at empty and makes the callback silence-fill ~8–16 % of its buffers. See
        // `audio::frames_to_run` for the measurements and the policy.
        #[cfg(feature = "audio")]
        let frames_this_iter = if paused {
            usize::from(step) // a `.` step is deliberate: it runs its frame whatever the ring says
        } else if let Some(a) = audio.as_mut() {
            let n = audio::frames_to_run_for(&a.prod, a.frame_samples, a.skips);
            a.skips = if n == 0 { a.skips + 1 } else { 0 };
            n
        } else {
            1 // no device — nothing to pace against, so run at the window's rate as before
        };
        // No-audio build: nothing to steer by, so one frame per iteration exactly as before.
        #[cfg(not(feature = "audio"))]
        let frames_this_iter = usize::from(!paused || step);

        // Every branch below carries the scanline capture, because it *is* the pixel path — there is no
        // longer a "fast null-sink" video run. That is cheaper, not dearer, than what it replaces: the run
        // already rendered every scanline and threw the RGB away (`system.rs`), and the presenter then
        // rendered all 224 lines a *second* time. Measured on the S3K state at 320x224, 600 frames x3:
        // 4.0 ms/frame before, 2.5 ms/frame after.
        //
        // One iteration of this loop is exactly one emulated frame, capture lifecycle and all, so the two
        // unusual counts stay correct by construction: running 2 frames presents the *second* one (the first
        // is dropped from the display, never half-shown), and running 0 leaves `buf`/`width` untouched so the
        // retained framebuffer is re-presented below — the identical path a paused frontend takes.
        for _ in 0..frames_this_iter {
            // With audio live (SY-5b): drive every frame through the AudioAndWatch composite (audio + the
            // optional armed watch), then drain that frame's PCM and push it into the ring for the cpal
            // callback. The composite borrows `sink` for the run and is dropped before the drain re-borrow.
            #[cfg(feature = "audio")]
            {
                if let Some(a) = audio.as_mut() {
                    {
                        let audio_and_watch = audio::AudioAndWatch::new(
                            &mut a.sink,
                            watch_armed.then_some(&mut watchpoints),
                        );
                        let mut sink = Fanout::new(&mut cap, audio_and_watch);
                        sys.run_frames_with_sink(1, &mut sink);
                    }
                    let pcm = a.sink.drain();
                    // The volume/mute setting is applied here, on the producer side, so the real-time
                    // callback stays a pure copy (see `audio::push_frame`).
                    audio::push_frame(&mut a.prod, &pcm, audio::gain_for(volume, muted));
                } else if watch_armed {
                    // Audio disabled at runtime (no device): same video-only path as a no-audio build. Only
                    // pay for the recording watch sink when a watch is armed.
                    let mut sink = Fanout::new(&mut cap, &mut watchpoints);
                    sys.run_frames_with_sink(1, &mut sink);
                } else {
                    sys.run_frames_with_sink(1, &mut cap);
                }
            }
            // No-audio build: same shape without the audio half.
            #[cfg(not(feature = "audio"))]
            {
                if watch_armed {
                    let mut sink = Fanout::new(&mut cap, &mut watchpoints);
                    sys.run_frames_with_sink(1, &mut sink);
                } else {
                    sys.run_frames_with_sink(1, &mut cap);
                }
            }
            frame += 1;

            // Take the frame the run just completed, width and all — an H32↔H40 switch rides along in the
            // capture's own per-line log, so nothing here re-queries the VDP. A run that completed no frame
            // (one that ended mid-frame) leaves `buf`/`width` alone, so the last good image stays on screen.
            // Done per emulated frame rather than once per iteration, so the capture lifecycle below sees
            // exactly one frame at a time however many this iteration runs.
            if let Some(w) = blit_capture(&cap, &mut buf) {
                width = w;
            }
            // Release the capture's per-delivery log (~215 KB/emulated second, unbounded by design) now that
            // the frame it held has been copied out. The normal case is a run that ended cleanly on the frame
            // boundary — exactly `HEIGHT` deliveries and one completed frame — where the sink's in-progress
            // buffer is empty and `clear` drops nothing but bookkeeping. A run that ended *mid*-frame still
            // has real pixels buffered for a frame that has not completed yet, so it is left to finish, with
            // `MAX_CAPTURE_LINES` as the backstop that bounds even a pathological run of those.
            let ended_on_a_frame_boundary =
                cap.frames_completed() >= 1 && cap.lines().len() == HEIGHT;
            if ended_on_a_frame_boundary || cap.lines().len() >= MAX_CAPTURE_LINES {
                cap.clear();
            }
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

        // Optional debug marker: a contrasting crosshair at the watched pixel so the live driver can confirm
        // the click landed where intended (bounds-guarded against an H40→H32 mode switch since the click).
        // Drawn into a scratch copy, never into `buf`: the crosshair is an XOR, and a paused frontend
        // re-presents the same buffer every iteration — applied in place it would flicker and smear.
        let shown: &[u32] = match watched_pixel {
            Some((wx, wy)) => {
                present.clear();
                present.extend_from_slice(&buf);
                draw_crosshair(&mut present, width, wx, wy);
                &present
            }
            None => &buf,
        };
        let title = if paused {
            format!("oracle-next — frame {frame} [PAUSED]")
        } else {
            format!("oracle-next — frame {frame}")
        };
        window.set_title(&title);

        // update_with_buffer both presents and pumps the OS event queue; it honours set_target_fps.
        if let Err(e) = window.update_with_buffer(shown, width, HEIGHT) {
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

    /// Build a fixture ROM whose **only** interesting behaviour is changing CRAM *continuously, mid-frame*.
    ///
    /// It boots, configures a plain H32 display, zeroes VRAM with a fill DMA (so every plane and sprite pixel
    /// is transparent and the whole screen is the backdrop), points the backdrop register at CRAM entry 1, and
    /// then spins in a tight loop rewriting **entry 1** with a colour derived from a counter. Every scanline is
    /// therefore drawn in a different colour, while at any single instant CRAM holds exactly one backdrop
    /// colour — which is precisely the property that separates the two pixel paths.
    ///
    /// Structurally this is `oracle_core::testrom::build_pad_poll`'s scene (full-screen backdrop) with the
    /// pad-poll loop swapped for a palette-churn loop; it is written here rather than added to `testrom`
    /// because this slice does not touch `oracle-core`.
    fn build_midframe_cram_rom() -> Vec<u8> {
        fn w(rom: &mut Vec<u8>, word: u16) {
            rom.push((word >> 8) as u8);
            rom.push((word & 0xFF) as u8);
        }
        fn l(rom: &mut Vec<u8>, long: u32) {
            w(rom, (long >> 16) as u16);
            w(rom, (long & 0xFFFF) as u16);
        }
        /// Two-word VDP control-port command longword.
        fn vdp_cmd(code: u8, addr: u16) -> u32 {
            let word1 = (((code & 0x03) as u32) << 14) | (addr as u32 & 0x3FFF);
            let word2 = ((((code >> 2) & 0x0F) as u32) << 4) | (addr as u32 >> 14);
            (word1 << 16) | word2
        }

        let mut rom = Vec::new();
        l(&mut rom, 0x00FF_0000); // reset SSP
        l(&mut rom, 0x0000_0200); // reset PC = $200
        rom.resize(0x200, 0);

        // a0 = VDP control ($C00004), a1 = VDP data ($C00000).
        w(&mut rom, 0x41F9);
        l(&mut rom, 0x00C0_0004);
        w(&mut rom, 0x43F9);
        l(&mut rom, 0x00C0_0000);

        for reg in [
            0x8154u16, // reg 1  display + DMA enable + M5 (regs 11+ need mode 5)
            0x8230,    // reg 2  plane A $C000
            0x8407,    // reg 4  plane B $E000
            0x8558,    // reg 5  SAT $B000
            0x8701,    // reg 7  backdrop = CRAM entry 1 — the entry the loop rewrites
            0x8B00,    // reg 11 full scroll
            0x8C00,    // reg 12 H32, no shadow/highlight
            0x8D20,    // reg 13 h-scroll table $8000
            0x8F02,    // reg 15 autoinc 2 (one CRAM entry per data write)
            0x9000,    // reg 16 32x32 planes
        ] {
            w(&mut rom, 0x30BC);
            w(&mut rom, reg);
        }

        // Zero VRAM with a fill DMA: every tile / nametable / SAT byte -> 0, so the whole screen is backdrop.
        for reg in [0x8F01u16, 0x93FF, 0x94FF, 0x9780] {
            w(&mut rom, 0x30BC);
            w(&mut rom, reg);
        }
        w(&mut rom, 0x20BC);
        l(&mut rom, vdp_cmd(0x21, 0x0000)); // VRAM write @ $0000 + CD5
        w(&mut rom, 0x32BC);
        w(&mut rom, 0x0000); // data write triggers the fill (fill byte $00)
        w(&mut rom, 0x30BC);
        w(&mut rom, 0x8F02); // reg 15 back to autoinc 2

        // The churn loop: d0++ ; d1 = d0 & $0EEE ; CRAM[1] = d1. Roughly a couple of thousand iterations per
        // frame, so the backdrop colour differs from one scanline to the next, and no branch depends on
        // anything external.
        let loop_top = rom.len() as u32;
        w(&mut rom, 0x5240); // addq.w #1,d0
        w(&mut rom, 0x3200); // move.w d0,d1
        w(&mut rom, 0x0241);
        w(&mut rom, 0x0EEE); // andi.w #$0EEE,d1   (keep to the 9 valid CRAM colour bits)
        w(&mut rom, 0x20BC);
        l(&mut rom, vdp_cmd(0x03, 0x0002)); // move.l #<CRAM write @ entry 1>,(a0)
        w(&mut rom, 0x3281); // move.w d1,(a1)
        let bra_at = rom.len() as u32;
        let disp = (loop_top as i32 - (bra_at as i32 + 2)) as i8 as u8;
        w(&mut rom, 0x6000 | disp as u16); // bra.s loop_top
        rom
    }

    /// **The regression test for the post-hoc frame bug.** The window used to be painted by looping
    /// `Vdp::render_line` *after* `run_frames(1)` returned — i.e. against whatever CRAM the frame ended with.
    /// Every mid-frame palette effect was therefore invisible; S3K's underwater split rendered the water in the
    /// above-water palette (bright red instead of slate blue).
    ///
    /// The fixture repaints the backdrop colour continuously during the frame, so the frame the VDP actually
    /// drew has many colours in it while CRAM at any instant — and so the whole post-hoc render — has exactly
    /// one. Asserting `live > post` fails on the old path by construction: there, both counts are 1.
    #[test]
    fn the_presented_frame_carries_mid_frame_palette_changes() {
        use std::collections::HashSet;

        let mut sys = System::new(0x5EED);
        sys.load_rom(build_midframe_cram_rom());
        sys.reset();
        sys.run_frames(4); // let the setup (including the fill DMA) finish; the loop is running by now

        // The frontend's path: one frame run with the scanline capture attached, then blit what it holds.
        let mut cap = ScanlineCapture::new(Retain::LastFrame);
        sys.run_frames_with_sink(1, &mut cap);
        let mut buf: Vec<u32> = Vec::new();
        let width = blit_capture(&cap, &mut buf).expect("a completed frame to present");
        assert_eq!(width, 256, "the fixture programs H32");
        assert_eq!(buf.len(), width * HEIGHT);
        let live: HashSet<u32> = buf.iter().copied().collect();

        // The old path: `render_line` over the same machine, right where `render_into` used to run it.
        let post: HashSet<u32> = (0..HEIGHT)
            .flat_map(|line| {
                sys.vdp()
                    .render_line(line as u16)
                    .into_iter()
                    .map(|(r, g, b)| (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))
            })
            .collect();

        assert_eq!(
            post.len(),
            1,
            "the whole screen is backdrop, so a single post-hoc CRAM read can only produce ONE colour \
             — this is the ceiling the old presenter was stuck at"
        );
        assert!(
            live.len() > post.len(),
            "the presented frame must show the palette the VDP drew each line with, not the one CRAM \
             ended the frame holding: got {} distinct colours, post-hoc ceiling {}",
            live.len(),
            post.len()
        );
        // Not merely "more than one": the churn is per-scanline, so a correct capture shows many bands.
        assert!(
            live.len() >= 16,
            "expected the frame to carry many per-line colours, got {}",
            live.len()
        );
    }

    /// **The audio-feedback / retained-framebuffer interaction.** The run loop no longer runs exactly one
    /// emulated frame per iteration — `audio::frames_to_run` can ask for 0 (ring nearly full) or 2 (ring
    /// nearly empty). This replays that loop body verbatim over the sequence 1, 2, 0, 1 and pins the two
    /// properties the change could plausibly break:
    ///
    /// * a **0-frame** iteration must re-present the retained framebuffer unchanged — never an empty capture,
    ///   never a blanked screen (the same contract the paused path has);
    /// * a **2-frame** iteration must present the *second* frame whole, never a half-collected one, and must
    ///   leave the capture's per-line log bounded exactly as a 1-frame iteration does.
    #[test]
    fn the_frame_budget_presents_correctly_at_zero_one_and_two_frames() {
        let mut sys = System::new(0x5EED);
        sys.load_rom(build_midframe_cram_rom());
        sys.reset();
        sys.run_frames(4);

        // A second machine from the same seed and ROM, advanced strictly one frame at a time. It is the
        // oracle for "which frame is on screen": two identical machines given the same number of frames are
        // in the same state, so `reference[k]` is exactly what the k-th emulated frame looked like.
        let mut refsys = System::new(0x5EED);
        refsys.load_rom(build_midframe_cram_rom());
        refsys.reset();
        refsys.run_frames(4);
        let mut refcap = ScanlineCapture::new(Retain::LastFrame);
        let mut next_reference_frame = |cap: &mut ScanlineCapture| -> Vec<u32> {
            refsys.run_frames_with_sink(1, cap);
            let mut b = Vec::new();
            blit_capture(cap, &mut b).expect("the reference completes a frame every run");
            cap.clear();
            b
        };

        let mut cap = ScanlineCapture::new(Retain::LastFrame);
        let mut buf: Vec<u32> = Vec::new();
        let mut width = 0usize;

        for frames_this_iter in [1usize, 2, 0, 1] {
            let before = buf.clone();
            // What each of this iteration's frames should look like, in order.
            let expected: Vec<Vec<u32>> = (0..frames_this_iter)
                .map(|_| next_reference_frame(&mut refcap))
                .collect();
            // --- verbatim copy of the run loop's per-frame body ---
            for _ in 0..frames_this_iter {
                sys.run_frames_with_sink(1, &mut cap);
                if let Some(w) = blit_capture(&cap, &mut buf) {
                    width = w;
                }
                let ended_on_a_frame_boundary =
                    cap.frames_completed() >= 1 && cap.lines().len() == HEIGHT;
                if ended_on_a_frame_boundary || cap.lines().len() >= MAX_CAPTURE_LINES {
                    cap.clear();
                }
            }
            // --- what the presenter then hands to the window ---
            assert_eq!(width, 256, "the fixture programs H32");
            assert_eq!(
                buf.len(),
                width * HEIGHT,
                "the presented buffer is always a whole frame, never a partial one"
            );
            match expected.last() {
                // 0 frames: the retained framebuffer is re-presented byte for byte — the paused contract.
                None => assert_eq!(
                    buf, before,
                    "a 0-frame iteration must re-present the retained framebuffer byte for byte"
                ),
                // 1 or 2 frames: what is on screen is the LAST frame the machine ran. For the 2-frame case
                // that is the pin that matters — the display must show the second frame, not the first and
                // not a blend of the two.
                Some(last) => {
                    assert_eq!(
                        &buf, last,
                        "the presented frame must be the last one run at budget {frames_this_iter}"
                    );
                    if frames_this_iter == 2 {
                        assert_ne!(
                            &buf, &expected[0],
                            "the 2-frame iteration must present the SECOND frame — the first is dropped \
                             from the display, not shown"
                        );
                    }
                }
            }
            // The capture log never accumulates across iterations, whatever the frame budget was.
            assert!(
                cap.lines().len() < MAX_CAPTURE_LINES,
                "the per-line log stayed bounded at budget {frames_this_iter}"
            );
        }
    }

    /// A run that completed no frame must not blank the window: [`blit_capture`] reports `None` and leaves the
    /// caller's framebuffer untouched, which is what keeps the last good image on screen while paused (no run
    /// happens, so no scanlines are delivered) and across the rare run that ends mid-frame.
    #[test]
    fn blit_capture_leaves_the_last_frame_alone_when_nothing_completed() {
        use oracle_core::bus::BusEventSink;

        let mut buf: Vec<u32> = vec![0xAA_BBCC; 256 * HEIGHT];
        let before = buf.clone();

        // Nothing captured at all — the paused case.
        let empty = ScanlineCapture::new(Retain::LastFrame);
        assert_eq!(blit_capture(&empty, &mut buf), None);
        assert_eq!(buf, before, "framebuffer untouched");

        // A partial frame with no boundary — the mid-frame-exit case. `LastFrame` hands out nothing until a
        // boundary, so this is also `None`.
        let mut partial = ScanlineCapture::new(Retain::LastFrame);
        for line in 0..100u16 {
            partial.on_scanline(line, &vec![(1, 2, 3); 256]);
        }
        assert_eq!(blit_capture(&partial, &mut buf), None);
        assert_eq!(buf, before, "framebuffer untouched");

        // A capture whose completed frame is not the tail of its line log (a mid-frame exit *after* an earlier
        // frame completed) is refused by the sum check rather than blitted from mismatched metadata.
        let mut stale = ScanlineCapture::new(Retain::LastFrame);
        for line in 0..HEIGHT as u16 {
            stale.on_scanline(line, &vec![(7, 7, 7); 256]);
        }
        stale.on_frame_boundary(0);
        for line in 0..20u16 {
            stale.on_scanline(line, &vec![(8, 8, 8); 320]); // a torn next frame, wider
        }
        assert_eq!(blit_capture(&stale, &mut buf), None);
        assert_eq!(buf, before, "framebuffer untouched");
    }

    /// A frame the VDP switched display mode part-way down is **not** rectangular. It must still reach the
    /// window — S3K produces exactly one of these on the first frame after a soft reset (two H32 lines, then
    /// H40), and refusing them would blank the window for as long as a game kept switching. The presented
    /// width is the one the frame *ended* on (what the chip is scanning out by V-Blank) and short lines are
    /// padded with black.
    #[test]
    fn a_mid_frame_mode_switch_is_padded_to_the_final_width() {
        use oracle_core::bus::BusEventSink;

        let mut ragged = ScanlineCapture::new(Retain::LastFrame);
        for line in 0..HEIGHT as u16 {
            // Two narrow (H32) lines, then the rest H40 — the shape S3K's post-reset frame has.
            let w = if line < 2 { 256 } else { 320 };
            ragged.on_scanline(line, &vec![(0x11, 0x22, 0x33); w]);
        }
        ragged.on_frame_boundary(0);
        assert!(
            !ragged.pixels().len().is_multiple_of(HEIGHT),
            "the payload really is ragged — no single width divides it"
        );

        let mut buf: Vec<u32> = Vec::new();
        assert_eq!(blit_capture(&ragged, &mut buf), Some(320));
        assert_eq!(buf.len(), 320 * HEIGHT);
        assert_eq!(buf[0], 0x0011_2233, "the narrow line's own pixels survive");
        assert_eq!(buf[255], 0x0011_2233, "…up to its real width");
        assert_eq!(buf[256], 0, "…and the rest of that line is padded black");
        assert_eq!(buf[319], 0);
        assert_eq!(buf[2 * 320], 0x0011_2233, "full-width lines are unpadded");
        assert_eq!(buf[2 * 320 + 319], 0x0011_2233);
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
