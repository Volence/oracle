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
//! Usage: `cargo run --release -p oracle-frontend -- <rom.bin> [--scale N] [--aspect tv|square|integer]
//! [--aether] [--socket PATH]`
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
//! | `.` (period)      | single-frame step (pauses first if running) |
//! | `` ` `` (backtick) or Ctrl+P | open the command palette (backtick again, or Esc, closes it) |
//! | Tab (or F1)       | soft-reset the console (SRAM contents preserved, as on real hardware) |
//! | F5                | re-read the ROM file from disk and reset — the edit-assemble-test loop |
//! | F2                | save state to the current slot |
//! | F4                | load state from the current slot |
//! | F6 / F7           | previous / next save-state slot |
//! | 0 – 9             | select save-state slot directly |
//! | `-` / `=`         | output volume down / up (audio builds; repeats while held) |
//! | M                 | mute toggle (audio builds; remembers the volume level) |
//! | F3                | toggle the on-screen status line (slot strip, volume, filter, aspect, frame) |
//! | Left mouse click  | watch what is under the clicked pixel — plane tile, **sprite**, or backdrop |
//! | W                 | dump recorded watch hits (seq/frame/pc/addr/old→new/via, PC symbolised) + drop count |
//! | C                 | clear the watch (stop recording write hits) |
//! | Esc               | close the palette / picker (quit = window close button or the Quit command) |
//!
//! Every action lives in the command registry ([`commands`]); the palette lists them grouped, with hotkeys
//! shown — the list is the cheat-sheet.
//!
//! The window is **resizable** (so the window manager's own maximise / fullscreen works), and `--aspect`
//! chooses how the picture is fitted into it — see [`present`].
//!
//! ## On-screen output
//!
//! Every message also appears **in the window** ([`overlay`]), because a person who launched a window never
//! reads stdout — the owner's first real session was spent guessing whether a save had happened. `println!`
//! is unchanged, so terminal logs and anything parsing them are unaffected; the toast is additional. The
//! `PAUSED` banner is load-bearing rather than decorative: since the render path started retaining the last
//! good framebuffer, a paused frontend and a hung one are otherwise pixel-identical.
//!
//! The gamepad layout (face buttons → A/B/C, Start, d-pad and left stick → directions) lives in one place —
//! the mapping tables at the top of the `gamepad` module — so remapping means editing those tables. The
//! analog deadzone is no longer one of them: it is a per-`Gamepads` value fed from the config file, with
//! `gamepad::STICK_DEADZONE` as its built-in default.
//!
//! ## Settings
//!
//! Seven values persist between runs in a flat `key = value` file at
//! `$XDG_CONFIG_HOME/oracle/player.conf` (falling back to `$HOME/.config/oracle/player.conf`; a system
//! with neither variable set runs fine and simply does not persist): `volume`, `muted`,
//! `aspect`, `scale`, `status_line`, `deadzone` and `lenses` — see [`config`]. **A CLI flag beats the
//! file**, which beats the built-in default, so `--scale 4` is a one-run override and never rewrites what
//! is stored. Changing the volume, the mute toggle, the F3 status line or any lens saves automatically:
//! the write is debounced by two seconds (a held volume ramp is one write, not ten) and flushed again on
//! quit if anything is still outstanding. A session that changed nothing writes nothing.
//!
//! A file that is structurally corrupt is renamed to `.bak` and defaults load in its place — the evidence
//! is kept, nothing crashes, and the toast on screen says which. A value out of range costs only that key
//! at *load*: it warns once and the default stands. Anything unrecognised — a **key**, or a lens name
//! inside `lenses` — is carried through instead: kept verbatim and written back out by the next save
//! (`F-CONFIG-UNKNOWN-KEYS`, reversed now that the key set has widened past six), so launching an older
//! build once can no longer delete what a newer build wrote. That is one collapsed toast per category
//! rather than one per name, because these recur on every launch. Key bindings are not stored yet.
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
//! plane/window tile, its 32-byte VRAM range is armed as a VDP-internal write watch. The run loop always
//! drives the sink-generic [`System::run_frames_with_sink`] (see "Pixels" above); arming a watch just
//! composes it into that sink. `W` prints the recorded hits; `C` disarms.
//!
//! **The instrument is the bus's, not this file's** (`bus.watchpoints_mut()`, and `bus.watch_sink()` for the
//! run). It used to be a private `Watchpoints` owned by this loop, which was right while the panel was the
//! only reader. It stopped being right when `emulator/watchpoint_add` landed: the player owns the run loop,
//! so a bus-owned instrument attached only to the *bus's* runs would see nothing while the window runs the
//! game and would report `seen == 0` — "the recorder was never attached", which is honest and useless —
//! while a loop-owned one would be a second instrument for `emulator/watchpoint_hits` to disagree with.
//! There is one, and contract §8 item 19's parity is therefore structural rather than maintained. Two
//! consequences show up in the code below: a click retires only the ids **this panel** armed
//! (`panel_watches`) instead of calling `clear()`, which would take a socket client's watches with it; and
//! the sink is attached whenever the instrument holds any watch, not when this panel armed one.
//!
//! **Sprites too, as of this slice.** A sprite dot used to print "no tile watch this slice (follow-up)" and
//! do nothing, which in a Sonic game means almost everything interesting on screen was un-clickable. It now
//! arms the sprite's *drawing* tile for that dot **and** its 8-byte attribute-table entry, so "who drew
//! this?" and "who moved this?" both land in the same log; a backdrop dot arms the CRAM entry behind it. The
//! addressing lives in [`pick`], computed entirely from public core API. Break-on-hit is no longer *blocked*:
//! as of 2026-08-14
//! the core's run loop honours a sink's [`oracle_core::bus::BusEventSink::stop_requested`], so a run can end
//! at the instruction boundary a watch fires on. Wiring that into this loop (pause, then report the
//! [`oracle_core::system::StopRecord`]) is unbuilt, not impossible — the "the core is frame-batched" reason
//! recorded here previously no longer holds.
//!
//! ## The Aether control bus, hosted here
//!
//! With `--aether` (or `--socket PATH`, or `ORACLE_AETHER=1`) this process **also serves the Aether control
//! surface** — `empyrean/contract/protocol.md`, the same one `oracle-aether` serves headlessly. It is hosted
//! rather than run as a separate process because the contract leaves no alternative: a checkpoint is "a
//! serialization of the live emulator struct" (D13), every reply carries the machine's `frame`/`mclk` *at
//! reply time* (D11), and the trust model is the emulator process serving a socket it created (D8). All
//! three need hands on *this* `System`.
//!
//! Off by default, and off means off: no socket is created, no thread starts, and every call the loop makes
//! into [`bus`] is an identity operation on the machine.
//!
//! Three things about it are visible from inside this loop, and each is a decision rather than a detail:
//!
//! * **Pause is one flag.** An un-paused player *is* a free-running bus, so a client's `run_frames`/`run_to`
//!   is refused with `-32005 machineRunning` while the window is running — which is what §6's run-control
//!   state rule requires, not a workaround. `emulator/pause` therefore pauses *the window*, and Space is
//!   read back out of the bus so the two can never disagree.
//! * **Pads merge.** A client's `emulator/hold` set is OR'd into the pad this loop writes, exactly as the
//!   keyboard and gamepad already OR with each other, and the human's live input is published to the bus so
//!   `emulator/press` does not silently drop it.
//! * **The picture is shared.** The completed frame is handed to the bus (before the capture is released),
//!   so `emulator/screenshot` serves what is on the glass. A client-driven run happens with this loop's
//!   sinks detached, so the bus runs its own capture and the loop pulls that frame back afterwards.
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

// On-screen output: a self-contained bitmap font, and the notification / status / paused overlay it draws.
// Nothing in a window ever reads stdout, which is where every message used to go.
mod commands;
// Slice S2 — the player's persistent settings file (spec §7): pure parse/serialize plus
// load-with-recovery and atomic save.
mod config;
mod font;
// Lenses: read-only overlays over the picture, each its own toggle command (spec §5).
mod lens;
mod overlay;
mod palette;
// The Aether capability layer, hosted in this process (`--aether` / `--socket`). Two implementations with
// one surface: the real one when the `aether` feature is on, a set of no-ops when it is not — so the run
// loop below has a single shape and no `#[cfg]` of its own. See `bus.rs`'s module docs for the design and
// for the three semantic conflicts it resolves.
#[cfg(feature = "aether")]
mod bus;
#[cfg(not(feature = "aether"))]
#[path = "bus_stub.rs"]
mod bus;
// Display geometry — aspect handling, the window-sized presentation blit, and the exact click inverse.
mod present;
// Click-to-watch: resolving a clicked dot to armable VRAM/CRAM ranges, sprites included.
mod pick;

use minifb::{Key, KeyRepeat, MouseButton, MouseMode, ScaleMode, Window, WindowOptions};
use oracle_core::bus::Fanout;
use oracle_core::io::Pad;
use oracle_core::scanline_capture::{Retain, ScanlineCapture};
use oracle_core::symbols::SymbolTable;
use oracle_core::system::System;
use oracle_core::watchpoints::{WatchOp, WatchSpace, WatchVia, Watchpoints};
use overlay::{Overlay, Status, ACCENT, ERROR, INFO};
use present::Aspect;

/// Active display height in scanlines (Genesis NTSC active area). Width is queried from the VDP *every frame*
/// (H32=256 / H40=320) — the game reprograms it after boot, so it is not fixed at reset.
const HEIGHT: usize = 224;

/// Widest display mode (H40). The window is sized for this so an H40 scene fills it exactly at the requested
/// integer scale; H32 content is pillarboxed by [`ScaleMode::AspectRatioStretch`].
const MAX_WIDTH: usize = 320;

/// Ring capacity of the pixel-attribution watch log. One armed watch covers a single 32-byte tile, so the
/// per-frame write count is small; this is a generous bound (drops are still counted and reported by `W`).
const WATCH_CAP: usize = 8192;

/// Parsed command line: the ROM path, the initial window scale, the aspect mode, and whether to serve the
/// Aether bus.
struct Args {
    rom_path: String,
    /// `None` = the flag was not typed — the config file (falling back to the built-in default) fills it in
    /// via [`resolve_scale`]. `Some(v)` = the CLI wins regardless of what the config file says.
    scale: Option<usize>,
    /// `None` = the flag was not typed; resolved the same way as `scale`, via [`resolve_aspect`].
    aspect: Option<Aspect>,
    /// `None` = do not serve (the default — no socket is created at all). `Some(None)` = serve on the
    /// contract's default path; `Some(Some(p))` = serve on `p`.
    socket: Option<Option<std::path::PathBuf>>,
}

/// Parse `<rom.bin> [--scale N] [--aspect tv|square|integer]`. Returns a human-readable error string on
/// misuse (the caller prints it and exits non-zero) — a missing/garbled ROM is a plain error, not a panic,
/// matching the `boot_rom` convention.
fn parse_args() -> Result<Args, String> {
    parse_args_from(std::env::args().skip(1))
}

/// The testable half of [`parse_args`], over an arbitrary argument sequence.
fn parse_args_from(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut rom_path: Option<String> = None;
    let mut scale: Option<usize> = None;
    let mut aspect: Option<Aspect> = None;
    // Serving is **opt-in**, and the default is "no socket exists". `ORACLE_AETHER` is read here rather than
    // deep in the bus so that `--aether` and the environment are one decision with one spelling, and so the
    // usage text can be truthful about both.
    let mut socket: Option<Option<std::path::PathBuf>> = std::env::var_os("ORACLE_AETHER")
        .is_some_and(|v| v != "0")
        .then_some(None);
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            // `--socket PATH` implies `--aether`: asking for a specific path and then not serving on it
            // would be a flag that does nothing.
            "--aether" => socket = Some(socket.flatten()),
            "--socket" => {
                let v = it.next().ok_or("--socket needs a value")?;
                socket = Some(Some(std::path::PathBuf::from(v)));
            }
            "--scale" => {
                let v = it.next().ok_or("--scale needs a value")?;
                scale = Some(
                    v.parse::<usize>()
                        .ok()
                        .filter(|&s| (1..=8).contains(&s))
                        .ok_or_else(|| format!("--scale must be an integer 1..=8, got `{v}`"))?,
                );
            }
            "--aspect" => {
                let v = it.next().ok_or("--aspect needs a value")?;
                aspect =
                    Some(Aspect::from_name(&v).ok_or_else(|| {
                        format!("--aspect must be tv, square or integer, got `{v}`")
                    })?);
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
    Ok(Args {
        rom_path,
        scale,
        aspect,
        socket,
    })
}

/// The gamepad module's default deadzone, visible regardless of the `gamepad` feature so the
/// config file round-trips it identically in every build. This is only the **default**: the live
/// value each `Gamepads` polls with comes from the loaded config's `deadzone`, and this fn is what
/// fills that field in when the file says nothing.
#[cfg(feature = "gamepad")]
pub(crate) fn gamepad_default_deadzone() -> f32 {
    gamepad::STICK_DEADZONE
}
#[cfg(not(feature = "gamepad"))]
pub(crate) fn gamepad_default_deadzone() -> f32 {
    0.5 // gamepad module absent from this build; the file still round-trips the key
}

/// CLI beats config beats built-in default (spec §7). Two tiny fns rather than one struct so each call site
/// reads as what it is. `cfg` is the config file as loaded (itself defaults when there is no file), so an
/// unset flag falls through to what was stored and a set one overrides it for this run only — a `--scale`
/// override is never written back.
fn resolve_scale(cli: Option<usize>, cfg: &config::Config) -> usize {
    cli.unwrap_or(cfg.scale)
}
fn resolve_aspect(cli: Option<Aspect>, cfg: &config::Config) -> Aspect {
    cli.unwrap_or(cfg.aspect)
}

/// Say something to **both** audiences: the terminal log (which scripts, the tests and a developer read) and
/// the window (which is the only place a person who double-clicked the binary is looking). Every message the
/// run loop used to `println!` goes through here, so the two can never drift apart.
fn notify(ov: &mut Overlay, color: u32, msg: impl AsRef<str> + Into<String>) {
    println!("{}", msg.as_ref());
    ov.push(msg, color);
}

/// The same, for failures: `eprintln!` plus a red toast.
fn notify_err(ov: &mut Overlay, msg: impl AsRef<str> + Into<String>) {
    eprintln!("{}", msg.as_ref());
    ov.push(msg, ERROR);
}

/// Which save-state slots currently have a file on disk. Probed only when it can have changed (a save, a
/// load, a slot change), never per frame — this is the only thing the slot strip cannot know for itself.
fn probe_slots(rom_path: &str) -> [bool; save_state::SLOT_COUNT] {
    let mut out = [false; save_state::SLOT_COUNT];
    for (slot, occupied) in out.iter_mut().enumerate() {
        *occupied = save_state::state_path_for(std::path::Path::new(rom_path), slot).exists();
    }
    out
}

/// What the hosted bus should know about the cartridge: the ROM's path, and the `.lst` listing bound to it.
/// The listing is only named when one was actually loaded — the bus refuses to resolve symbols rather than
/// resolve them against a path that holds nothing (D7).
fn bus_machine_info(rom_path: &str, symbols: Option<SymbolTable>) -> bus::MachineInfo {
    let symbols_path = symbols
        .is_some()
        .then(|| symbol_file::lst_path_for(std::path::Path::new(rom_path)))
        .map(|p| p.display().to_string());
    bus::MachineInfo {
        rom_path: Some(rom_path.to_string()),
        symbols,
        symbols_path,
    }
}

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

/// Next value of the "these keys were typed at the palette, not at the game" latch.
///
/// The palette closes **mid-iteration** — Esc, or Enter running a command — and the keys that closed it are
/// still physically held when the pad is polled further down the same iteration. Without a latch, `Enter`
/// (the key that ran the command) reads straight through as Start, so every palette command handed the game
/// several frames of Start — an in-game pause in Sonic — and a held A/S/D or arrow leaked the same way.
///
/// So the keyboard half of Player 1 stays released until the user has let go of *every* game key: a press
/// that began as text can never finish as gameplay. Gamepads are unaffected; they were never typing.
fn release_latch(latch: bool, any_game_key_down: bool) -> bool {
    latch && any_game_key_down
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
        // `watch` is printed because `Watchpoints::clear()` discards the *specs* and keeps the recorded
        // hits by design ("ids are not reused, so hits recorded before the clear keep naming watches that
        // no longer exist"), and the click path clears-then-rearms on every click. Without the id, two
        // successive clicks produced one interleaved log with no way to tell which pixel a hit belonged
        // to, and the first click's labels gone — a misleading instrument, which is the failure this
        // whole subsystem exists to prevent. The id is the attribution the core already recorded; only
        // the printing was missing.
        println!(
            "watch {:>3}  seq {:>6}  frame {:>6}  pc ${:06X}{sym}  addr ${:04X}  ${:X}->${:X}  via {via}",
            h.watch.0, h.seq, h.frame, h.pc, h.addr, h.old, h.value
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
    /// Name of the console output-filter revision in use, for the on-screen status line. Which revision is
    /// "correct" is a choice the listener makes (`ORACLE_CONSOLE_FILTER`), so it is worth showing rather than
    /// leaving in a startup line that has long since scrolled away.
    filter: &'static str,
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

    // The console's analog output stage (SY-6b). Which RC corner is "correct" is revision-dependent, so
    // the core deliberately defaults to `Unfiltered` rather than baking a number in; this lets the
    // listener pick one without a rebuild. Accepts the same spellings as `ConsoleModel::from_name`
    // ("va0", "va3", "off"). An unrecognised value is named and ignored rather than silently falling
    // back, since a typo would otherwise present as "the filter does nothing".
    let console_model = match std::env::var("ORACLE_CONSOLE_FILTER") {
        Ok(name) => match oracle_core::synth::ConsoleModel::from_name(&name) {
            Some(m) => m,
            None => {
                eprintln!(
                    "audio: ORACLE_CONSOLE_FILTER={name:?} is not a known console revision \
                     (try va0, va3, or off) — using the default"
                );
                oracle_core::synth::ConsoleModel::default()
            }
        },
        // The PLAYER models a console, and every real board has an output stage — so unfiltered is the
        // one setting that matches no hardware at all, and it is the wrong default here. VA0-VA2 was
        // picked by ear against VA3-VA6 and the raw output (2026-08-15). This is deliberately a
        // frontend-only default: `AudioSink::new` stays `Unfiltered`, so library users, tests and
        // offline renders keep their bit-identical output and nothing shifts underneath them.
        Err(_) => oracle_core::synth::ConsoleModel::Model1Va0Va2,
    };
    let sink = oracle_core::synth::AudioSink::with_console_model(sample_rate, console_model);
    println!("audio: console output stage = {}", console_model.name());
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
        filter: console_model.name(),
        _stream: stream,
    })
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!(
                "usage: oracle-frontend <rom.bin> [--scale N] [--aspect tv|square|integer] \
                 [--aether] [--socket PATH]\n  \
                 --scale   N = 1..=8 (default 3) — multiples of the 224-line frame height\n  \
                 --aspect  tv = the console's own 4:3 (default), square = square pixels, \
                 integer = square pixels at a whole scale\n  \
                 --aether  serve the Aether control bus from this process (also: ORACLE_AETHER=1). \
                 Off by default — no socket is created.\n  \
                 --socket  serve on PATH instead of the contract's default; implies --aether\n  \
                 An unset --scale/--aspect falls back to ~/.config/oracle/player.conf (or \
                 $XDG_CONFIG_HOME/oracle/player.conf), then to the defaults above."
            );
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

    // A **resizable** window, presented 1:1 from a window-sized buffer this frontend fills itself
    // (`ScaleMode::Stretch` with a buffer of exactly the window's size is an identity present). minifb has no
    // runtime fullscreen call and no way to ask how big the screen is, so "make it fullscreen" is delegated
    // to the window manager, which `resize: true` is what enables. All the geometry — aspect, letterboxing,
    // and the exact inverse a click needs — is [`present`]'s, so it stays correct at any size the user drags
    // the window to. The initial size is `--scale` applied to the frame's height, widened per `--aspect`.
    // Persistent settings (spec §7): CLI beats config beats built-in default. Resolved ONCE — every site
    // below reads `scale`/`aspect`, never `args.scale`/`args.aspect` directly, so there is exactly one
    // precedence decision per run. A missing file is silently defaults; a corrupt one was backed up to
    // `.bak` (the load's warnings say so on the glass, once, as soon as the overlay exists).
    let cfg_path = config::config_path();
    let loaded = match &cfg_path {
        Some(p) => config::load(p),
        None => config::Loaded {
            config: config::Config::default(),
            warnings: vec![
                "config: no $XDG_CONFIG_HOME or $HOME — settings will not persist".into(),
            ],
            recovered: false,
        },
    };
    let mut cfg = loaded.config.clone();
    // What is believed to be ON DISK right now — not what loaded. Every successful save advances it, so the
    // quit write can tell "nothing changed" from "changed and already flushed" from "changed but the flush
    // FAILED" (that last one clears the countdown, so only this baseline can catch it and retry).
    let mut cfg_saved = loaded.config.clone();
    let scale = resolve_scale(args.scale, &cfg);
    let aspect = resolve_aspect(args.aspect, &cfg);

    let (win_w, win_h) = present::initial_window_size(scale, MAX_WIDTH, HEIGHT, aspect);
    let mut window = Window::new(
        "oracle-next",
        win_w,
        win_h,
        WindowOptions {
            scale_mode: ScaleMode::Stretch,
            resize: true,
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
        "window {win_w}x{win_h}, resizable, aspect {} — keyboard (P1): arrows=D-pad, A/S/D=A/B/C, Enter=Start; Space=pause, .=step, click=watch, W=dump, C=clear, F3=status line, Tab=reset, `=command palette (the full list)",
        aspect.name()
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
    // The restored volume step, read once so the banner below and the loop's `volume` local cannot
    // disagree. No clamp is needed: `config::parse` rejects anything above `VOLUME_MAX`, and this assert
    // pins `VOLUME_MAX` to the step count the audio module actually uses — change either and the build
    // stops here rather than a saved level silently rescaling.
    #[cfg(feature = "audio")]
    const _: () = assert!(audio::VOLUME_STEPS == config::VOLUME_MAX);
    #[cfg(feature = "audio")]
    let restored_volume: u8 = cfg.volume;
    #[cfg(feature = "audio")]
    println!(
        "audio: -/= volume down/up (starting at {}/{}{}, remembered between runs), M=mute",
        restored_volume,
        audio::VOLUME_STEPS,
        if cfg.muted { " [MUTED]" } else { "" }
    );

    // The Aether capability layer, hosted here. **Opt-in**: with no `--aether`/`--socket`/`ORACLE_AETHER`
    // this binds nothing, creates no filesystem entry and starts no thread, and every call into it below is
    // an identity operation — the default launch is the launch it always was.
    //
    // Built here, before the loop, because the bus has to be able to name the cartridge and resolve against
    // its listing from the first request (D7). The symbol table is cloned rather than moved: this frontend
    // keeps using its own for watch-hit annotation, and the bus's copy travels with checkpoints — and the
    // clone is skipped entirely when nothing is being served, so the default launch does not pay for a
    // second copy of a listing nobody will read.
    let serving = args.socket.is_some();
    let mut bus = bus::Bus::start(
        args.socket.clone(),
        if serving {
            bus_machine_info(&args.rom_path, symbols.clone())
        } else {
            bus::MachineInfo::default()
        },
    );

    // Host gamepads: `None` = gilrs unavailable → keyboard-only, never a panic (same contract as `start_audio`
    // below). `Some` with no controller attached is normal — one plugged in later is picked up by `poll`.
    // Detected controllers are announced by `Gamepads::new` itself, one line per pad.
    #[cfg(feature = "gamepad")]
    let mut gamepads = gamepad::Gamepads::new(cfg.deadzone);

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
    let mut marked: Vec<u32> = Vec::new();
    // The window-sized presentation buffer, rebuilt from `buf` every present, plus the blit's column map.
    // Both are scratch that outlives the loop only so the steady state allocates nothing. The overlay draws
    // into *this*, never into `buf` — which is the whole reason a re-presented retained frame can never
    // accumulate overlay ink.
    let mut screen: Vec<u32> = Vec::new();
    let mut xmap: Vec<usize> = Vec::new();
    let mut paused = false;
    let mut frame: u64 = 0;

    // On-screen notifications, status line and the paused banner. Everything the loop `println!`s is also
    // pushed here (see `notify`), because the window is where the user is looking.
    let mut ov = Overlay::new();
    ov.status_line = cfg.status_line;
    let mut slots_on_disk = probe_slots(&args.rom_path);

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

    // Config autosave: same debounce shape as the `.srm`, so a volume ramp held down coalesces into one
    // write instead of one per step.
    const CONFIG_AUTOSAVE_DEBOUNCE_FRAMES: u32 = 120;
    let mut config_save_countdown: Option<u32> = None;

    // The pixel-attribution watch. **The instrument now lives on the bus** (`bus.watchpoints_mut()`) rather
    // than here, and that is the whole of contract §8 item 19 for this capability: the player owns the run
    // loop, so an instrument the bus owned privately would see nothing while the window runs the game, and
    // one *this* loop owned privately would be a second instrument for `emulator/watchpoint_hits` to
    // disagree with. There is one, both sides feed and read it, and they cannot drift.
    //
    // `panel_watches` is what this panel itself armed. It exists because the instrument is shared: a click
    // must replace **the panel's** prior watch and not a watch some client armed over the socket, so the
    // panel removes its own ids by handle instead of reaching for `clear()`.
    let mut panel_watches: Vec<oracle_core::watchpoints::WatchId> = Vec::new();
    let mut watched_pixel: Option<(u16, u16)> = None;
    let mut prev_mouse_down = false;

    // The save-state slot F2/F4 act on; F6/F7 step it, 0-9 pick it directly.
    let mut state_slot: usize = 0;

    // Output volume (audio builds only — with no audio there is nothing to attenuate, and the state would be
    // dead code). `volume` is a step in `0..=audio::VOLUME_STEPS`, restored from the config file (which
    // defaults to full, so behaviour is unchanged until the user touches it); `muted` is an independent
    // toggle so unmuting restores the level. Both come from the single clamp done at the banner above.
    #[cfg(feature = "audio")]
    let mut volume: u8 = restored_volume;
    #[cfg(feature = "audio")]
    let mut muted = cfg.muted;

    // The command registry + palette (spec §4). The registry is the single source of truth for actions;
    // dispatch happens in ONE `match cmd` below so the actions keep borrowing the loop's state directly.
    let reg = commands::registry();
    let mut palette = palette::Palette::new();
    let mut running = true;
    // Set when the palette closes under a still-held key; see [`release_latch`].
    let mut swallow_keys_until_release = false;
    ov.push("PRESS ` FOR COMMANDS", INFO); // discoverability layer 1 (spec §4)

    // Whatever the settings load had to say, said once, on the glass. A file that was structurally corrupt
    // and got moved aside is a failure like any other, so it takes the failure path — red toast *and*
    // stderr. A merely ignored key, or a location that cannot persist, is worth noticing but is not damage.
    for w in &loaded.warnings {
        if loaded.recovered {
            notify_err(&mut ov, w.clone());
        } else {
            notify(&mut ov, ACCENT, w.clone());
        }
    }

    // Esc no longer quits (spec §3): it closes the palette. Quitting is the window's close button or the
    // Quit command, which clears `running`.
    while window.is_open() && running {
        // The window is resizable, so its geometry is re-derived every iteration rather than assumed. The
        // click inverse below and the present at the bottom both use this same rectangle.
        let (win_w, win_h) = window.get_size();
        let view = present::dest_rect(win_w, win_h, width, HEIGHT, aspect);

        // A left-click edge maps the clicked window pixel to a native dot and asks the VDP who is showing
        // there; a plane/window tile winner arms a watch on that tile's 32-byte VRAM range (replacing any
        // prior watch). Width is the *currently displayed* frame's width (pre-step), so the click resolves
        // against what the user is actually looking at.
        // The palette eats input while it is up (spec §3), and that includes the mouse: a click meant for the
        // panel must not arm a watch on whatever tile is behind it. The edge is still *tracked* either way, so
        // a press that started under the palette cannot re-fire as a fresh click once it closes.
        let mouse_down = window.get_mouse_down(MouseButton::Left);
        let clicked = mouse_down && !prev_mouse_down;
        prev_mouse_down = mouse_down;
        if clicked && !palette.is_open() {
            // `view` is the rectangle the frame *currently on screen* occupies, derived from the width the
            // last successful blit reported — not a fresh `render_line` query, which would answer for the
            // mode the VDP is in *now* (a post-hoc read; see `blit_capture`). `window_to_native` is the exact
            // inverse of the blit that painted it, so this is correct at any window size.
            if let Some((mx, my)) = window.get_mouse_pos(MouseMode::Discard) {
                if let Some((x, y)) = present::window_to_native(mx, my, view, width, HEIGHT) {
                    // Resolve the dot to whatever it is — plane tile, sprite (its drawing pattern *and* its
                    // attribute-table entry), or backdrop palette entry. A sprite dot used to arm nothing.
                    let p = pick::resolve(sys.vdp(), x, y);
                    let wp = bus.watchpoints_mut();
                    // Retire only what this panel armed. `clear()` would take a socket client's watches
                    // with it — the shared-instrument hazard, and the one thing that made the panel's
                    // "a click replaces the prior watch" rule need a list instead of a reset.
                    for id in panel_watches.drain(..) {
                        wp.remove(id);
                    }
                    for t in &p.targets {
                        let space = match t.space {
                            pick::Space::Vram => WatchSpace::Vram,
                            pick::Space::Cram => WatchSpace::Cram,
                        };
                        panel_watches.push(wp.add_vdp_watch(
                            space,
                            t.lo..=t.hi,
                            WatchOp::Write,
                            t.label.clone(),
                        ));
                    }
                    let armed_now = !p.targets.is_empty();
                    watched_pixel = armed_now.then_some((x, y));
                    // The terminal gets the full line; the toast gets the short form that fits on screen.
                    println!("{}", p.description);
                    ov.push(p.toast, if armed_now { INFO } else { ERROR });
                }
            }
        }

        // --- Input routing (spec §3), three cases. Palette open: it eats every key, and no binding fires.
        // The frame the palette *opens*: the opening chord is consumed too, so backtick/Ctrl+P can never
        // also trigger whatever else those keys are bound to. Otherwise: the registry's bindings are
        // scanned. Every case funnels into `pending`, dispatched once below. ---
        let mut pending: Vec<commands::Cmd> = Vec::new();
        let mut step = false;
        let was_open = palette.is_open();
        if palette.is_open() {
            for k in window.get_keys_pressed(KeyRepeat::Yes) {
                let pk = match k {
                    // Backtick toggles: the key that opened the palette closes it again (Quake muscle
                    // memory). `key_char` deliberately never types it, so this is its only meaning here.
                    Key::Backquote => Some(palette::PaletteKey::Esc),
                    Key::Backspace => Some(palette::PaletteKey::Backspace),
                    Key::Up => Some(palette::PaletteKey::Up),
                    Key::Down => Some(palette::PaletteKey::Down),
                    Key::Enter => Some(palette::PaletteKey::Enter),
                    Key::Escape => Some(palette::PaletteKey::Esc),
                    _ => commands::key_char(k).map(palette::PaletteKey::Char),
                };
                if let Some(pk) = pk {
                    if let palette::PaletteAction::Run(cmd) = palette.handle(pk, &reg) {
                        pending.push(cmd);
                    }
                }
            }
        } else {
            let ctrl = window.is_key_down(Key::LeftCtrl) || window.is_key_down(Key::RightCtrl);
            if window.is_key_pressed(Key::Backquote, KeyRepeat::No)
                || (ctrl && window.is_key_pressed(Key::P, KeyRepeat::No))
            {
                palette.open(&reg);
            } else {
                for c in &reg {
                    let Some(key) = c.hotkey else { continue };
                    let rep = if c.repeat {
                        KeyRepeat::Yes
                    } else {
                        KeyRepeat::No
                    };
                    if window.is_key_pressed(key, rep) {
                        pending.push(c.cmd);
                    }
                }
            }
        }

        // --- Dispatch: the one match, with every action's body moved verbatim from the hotkey chain it
        // replaced. It lives inside the loop, not in handler functions, because that is what lets each arm
        // keep borrowing the loop's own state (`sys`, `ov`, `bus`, the slot and volume locals) directly. ---
        for cmd in pending {
            match cmd {
                commands::Cmd::Pause => {
                    paused = !paused;
                    // No toast: the PAUSED banner is the feedback, and it stays up for as long as the state
                    // lasts.
                    println!("{}", if paused { "paused" } else { "resumed" });
                }
                commands::Cmd::Step => {
                    // DWIM (spec §4): stepping while unpaused pauses AND steps, rather than doing nothing.
                    paused = true;
                    step = true;
                }
                commands::Cmd::ToggleStatusLine => {
                    ov.status_line = !ov.status_line;
                    cfg.status_line = ov.status_line;
                    config_save_countdown = Some(CONFIG_AUTOSAVE_DEBOUNCE_FRAMES);
                }
                // `cfg.lenses` **is** the live lens set, not a copy of one: unlike `ov.status_line`
                // (where the Overlay owns the flag and the duplicate is forced), nothing here owns
                // a lens set, `LensSet` is `Copy`, and a second writer for the value the picture
                // will depend on would be an invariant nothing enforces. Persisted through the
                // same debounce as every other setting. The toast is what tells you a toggle took
                // for the lenses that do not draw yet — the watch ticker, the CPU chip and the
                // CRAM strip do so far.
                commands::Cmd::ToggleLens(id) => {
                    cfg.lenses.toggle(id);
                    config_save_countdown = Some(CONFIG_AUTOSAVE_DEBOUNCE_FRAMES);
                    ov.push(
                        format!(
                            "{} {}",
                            id.label(),
                            if cfg.lenses.is_on(id) { "ON" } else { "OFF" }
                        ),
                        INFO,
                    );
                }
                // W dumps the recorded hits; C disarms the watch (dropping it back out of the run's sink).
                commands::Cmd::DumpHits => {
                    // **The panel side of the item-19 parity.** This reads the same ring
                    // `emulator/watchpoint_hits` serves, through the same non-destructive `hits()` — never
                    // `take_hits()`, which would let this key press delete a socket client's evidence.
                    let n = {
                        let wp = bus.watchpoints_mut();
                        let n = wp.hits().len();
                        dump_hits(wp, symbols.as_ref());
                        n
                    };
                    // The hits themselves are far too wide for the glass; the toast just confirms the key
                    // landed and says how much went to the terminal — which is where the answer actually is.
                    ov.push(format!("DUMPED {n} WATCH HITS TO STDOUT"), INFO);
                }
                commands::Cmd::ClearWatch => {
                    let wp = bus.watchpoints_mut();
                    for id in panel_watches.drain(..) {
                        wp.remove(id);
                    }
                    watched_pixel = None;
                    notify(&mut ov, INFO, "watch cleared — no longer recording writes");
                }
                // --- Save states (usable while paused too). Slot selection: F6/F7 step, 0-9 pick directly,
                // and the palette's picker runs the same `SlotSelect`. ---
                commands::Cmd::SlotPrev
                | commands::Cmd::SlotNext
                | commands::Cmd::SlotSelect(_) => {
                    match cmd {
                        commands::Cmd::SlotPrev => state_slot = next_slot(state_slot, -1),
                        commands::Cmd::SlotNext => state_slot = next_slot(state_slot, 1),
                        commands::Cmd::SlotSelect(n) => state_slot = n,
                        // Contract: the outer or-pattern lists exactly the variants matched here —
                        // extend both together, or this becomes reachable.
                        _ => unreachable!(),
                    }
                    // Reaching here *is* the slot change, so what used to sit behind `if slot_changed` runs
                    // unconditionally. Re-probe on every slot move: another process (or an earlier session)
                    // can have written a state file since we last looked, and the strip is only useful if it
                    // is telling the truth.
                    slots_on_disk = probe_slots(&args.rom_path);
                    ov.flash(); // put the slot strip on screen without needing F3 first
                    notify(
                        &mut ov,
                        ACCENT,
                        format!(
                            "slot {state_slot} selected ({})",
                            if slots_on_disk[state_slot] {
                                "occupied"
                            } else {
                                "empty"
                            }
                        ),
                    );
                }
                commands::Cmd::SlotPicker => {
                    // Items carry occupancy, exactly what the slot toast says today.
                    let items: Vec<(String, commands::Cmd)> = (0..save_state::SLOT_COUNT)
                        .map(|n| {
                            let occ = if slots_on_disk[n] {
                                "occupied"
                            } else {
                                "empty"
                            };
                            (format!("slot {n} ({occ})"), commands::Cmd::SlotSelect(n))
                        })
                        .collect();
                    palette.open_picker("SELECT SLOT".into(), items, &reg);
                }
                // F2 = save, F4 = load, both on the currently selected slot. The path is built inside each
                // arm so the idle loop allocates nothing.
                commands::Cmd::SaveState => {
                    let state_path = save_state::state_path_for(
                        std::path::Path::new(&args.rom_path),
                        state_slot,
                    );
                    match save_state::save(&state_path, &sys, rom_fp) {
                        Ok(n) => {
                            slots_on_disk[state_slot] = true;
                            println!(
                                "state: saved {n} bytes to slot {state_slot} ({})",
                                state_path.display()
                            );
                            ov.push(format!("SAVED SLOT {state_slot}"), ACCENT);
                            ov.flash();
                        }
                        Err(e) => {
                            eprintln!("state: save to {} failed: {e}", state_path.display());
                            ov.push(format!("SAVE SLOT {state_slot} FAILED: {e}"), ERROR);
                        }
                    }
                }
                commands::Cmd::LoadState => {
                    let state_path = save_state::state_path_for(
                        std::path::Path::new(&args.rom_path),
                        state_slot,
                    );
                    // Refusal first, so the long restore below reads at one level instead of three. A stale or
                    // corrupt file leaves the running machine untouched, which is the whole contract of
                    // `save_state::load` returning `Err` rather than a half-built `System`.
                    let loaded = match save_state::load(&state_path, rom_fp) {
                        Ok(loaded) => loaded,
                        Err(e) => {
                            eprintln!("state: load of slot {state_slot} failed: {e}");
                            ov.push(format!("LOAD SLOT {state_slot} FAILED: {e}"), ERROR);
                            continue;
                        }
                    };

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
                    ov.push(format!("LOADED SLOT {state_slot}"), ACCENT);
                    ov.flash();
                }

                // --- Machine control: Reset (Tab / F1) soft-resets, ReloadRom (F5) re-reads the ROM from
                // disk and resets (module doc). ---
                commands::Cmd::Reset => {
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
                    notify(
                        &mut ov,
                        ACCENT,
                        "reset: soft reset — SRAM contents preserved, as on real hardware",
                    );
                }
                commands::Cmd::ReloadRom => {
                    // Read the file first: a rebuild that failed (or is still being written) must leave
                    // the running machine — and its battery data — completely untouched.
                    let bytes = match std::fs::read(&args.rom_path) {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            notify_err(
                                &mut ov,
                                format!(
                                    "reload: cannot read ROM {} ({e}) — still running the previous image",
                                    args.rom_path
                                ),
                            );
                            continue;
                        }
                    };
                    // Unlike a reset, `load_rom` re-provisions a *zeroed* SRAM buffer from the new header
                    // and clears `sram_used`/`sram_dirty` — unflushed battery data would be destroyed
                    // outright, with nothing left to retry from. So a failed flush aborts the reload.
                    if !flush_pending_srm(
                        &sys,
                        &srm_path,
                        sram_save_countdown,
                        "before the ROM reload",
                    ) {
                        notify_err(
                            &mut ov,
                            format!(
                                "reload: ABORTED — unsaved battery data could not be written to {}, and \
                                 reloading would zero it. Fix the write error and press F5 again.",
                                srm_path.display()
                            ),
                        );
                        continue;
                    }
                    notify(
                        &mut ov,
                        ACCENT,
                        format!(
                            "reload: re-read {} bytes from {}",
                            bytes.len(),
                            args.rom_path
                        ),
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
                    // …and re-point the bus at the same pair, for the same reason. A hosted client
                    // resolving `read_memory {symbol}` against the previous build's listing reads a
                    // wrong address and reports success — the D7 incident, exactly.
                    if serving {
                        bus.set_machine_info(bus_machine_info(&args.rom_path, symbols.clone()));
                    }
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
                commands::Cmd::Quit => running = false,
                // --- Output volume (audio builds only). The registry marks `-`/`=` as repeat-on-hold so the
                // level ramps smoothly; the mute toggle is edge-only so holding M does not flap it. A
                // no-audio build needs no arm at all here: `Cmd`'s volume variants are themselves
                // `#[cfg(feature = "audio")]`, so the match stays exhaustive without a dead one. ---
                #[cfg(feature = "audio")]
                commands::Cmd::VolumeUp | commands::Cmd::VolumeDown | commands::Cmd::MuteToggle => {
                    match cmd {
                        commands::Cmd::VolumeUp => volume = (volume + 1).min(audio::VOLUME_STEPS),
                        commands::Cmd::VolumeDown => volume = volume.saturating_sub(1),
                        commands::Cmd::MuteToggle => muted = !muted,
                        // Contract: the outer or-pattern lists exactly the variants matched here —
                        // extend both together, or this becomes reachable.
                        _ => unreachable!(),
                    }
                    let line = if muted {
                        format!("volume: {volume}/{}  [MUTED]", audio::VOLUME_STEPS)
                    } else {
                        format!("volume: {volume}/{}", audio::VOLUME_STEPS)
                    };
                    notify(&mut ov, INFO, line);
                    cfg.volume = volume;
                    cfg.muted = muted;
                    config_save_countdown = Some(CONFIG_AUTOSAVE_DEBOUNCE_FRAMES);
                }
            }
        }
        // Did that batch close the palette? Checked after dispatch, not inside routing, because a command
        // may reopen it in the same iteration (SlotPicker does exactly that) — and a palette that is still
        // up needs no latch, its keys are swallowed anyway.
        if was_open && !palette.is_open() {
            swallow_keys_until_release = true;
        }

        // Inputs are sampled live every frame; set_pad is the sole, deterministic input path into the core.
        // Player 1 = keyboard OR gamepad 1 (merged per button, so neither source can suppress the other);
        // Player 2 = gamepad 2 only, and an all-released Pad when there is none — which is exactly the state
        // port 1 already had before that slice, so a one-player session is unaffected.
        // Palette open = the keyboard is typing text, not playing (spec §3), so the keyboard half of P1 is
        // swallowed for as long as it is up — and, via the latch, until the keys that dismissed it are let
        // go (see `release_latch`). Gamepads are untouched and always reach the game.
        //
        // `poll_pad` is read unconditionally: an all-released `Pad` is exactly "no game key is down", so the
        // latch's release condition comes from the very same key list the pad is built from and cannot
        // drift from it.
        let keys = poll_pad(&window);
        swallow_keys_until_release =
            release_latch(swallow_keys_until_release, keys != Pad::default());
        let p1 = if palette.is_open() || swallow_keys_until_release {
            Pad::default()
        } else {
            keys
        };
        // `mut` is used only by the `gamepad` arm below; a no-gamepad build never writes it.
        #[allow(unused_mut)]
        let mut player = [p1, Pad::default()];
        #[cfg(feature = "gamepad")]
        if let Some(g) = gamepads.as_mut() {
            let pads = g.poll();
            for (p, from_pad) in player.iter_mut().zip(pads) {
                *p = gamepad::merge_pads(*p, from_pad);
            }
        }
        // The hosted bus is a third input source, and it composes with the other two the same way they
        // compose with each other: **per-button OR**. A client's `emulator/hold` set is added to what the
        // human is holding, and the human's input is published to the bus so that `emulator/press` and
        // `emulator/hold` write pads that still contain it. Neither side can suppress the other, and
        // `emulator/hold`'s reply keeps reporting exactly what the client asked for rather than whatever the
        // keyboard happened to be doing. Both calls are no-ops while nothing is being served.
        bus.set_live_pads(player);
        let pads = bus.merge_held(player);
        sys.set_pad(0, pads[0]);
        sys.set_pad(1, pads[1]);

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
            //
            // **The attach condition is the instrument's own count, and there is no longer a panel flag
            // beside it.** A `watch_armed` boolean would say only "the *click* armed something"; a watch a
            // socket client armed with
            // `emulator/watchpoint_add` is just as real, and a loop that skipped the sink for it would hand
            // that client a `seen == 0` reply — "the recorder was never attached to the run" — about frames
            // that really happened. Asking the shared instrument how many watches it holds is the one
            // question that covers both sources.
            let armed = bus.watchpoints_mut().watch_count() > 0;
            #[cfg(feature = "audio")]
            {
                if let Some(a) = audio.as_mut() {
                    {
                        let audio_and_watch =
                            audio::AudioAndWatch::new(&mut a.sink, armed.then(|| bus.watch_sink()));
                        let mut sink = Fanout::new(&mut cap, audio_and_watch);
                        sys.run_frames_with_sink(1, &mut sink);
                    }
                    let pcm = a.sink.drain();
                    // The volume/mute setting is applied here, on the producer side, so the real-time
                    // callback stays a pure copy (see `audio::push_frame`).
                    audio::push_frame(&mut a.prod, &pcm, audio::gain_for(volume, muted));
                } else if armed {
                    // Audio disabled at runtime (no device): same video-only path as a no-audio build. Only
                    // pay for the recording watch sink when a watch is armed.
                    let mut sink = Fanout::new(&mut cap, bus.watch_sink());
                    sys.run_frames_with_sink(1, &mut sink);
                } else {
                    sys.run_frames_with_sink(1, &mut cap);
                }
            }
            // No-audio build: same shape without the audio half.
            #[cfg(not(feature = "audio"))]
            {
                if armed {
                    let mut sink = Fanout::new(&mut cap, bus.watch_sink());
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
                // Hand the same completed frame to the bus, so `emulator/screenshot` serves the picture that
                // is on the glass rather than a post-hoc re-render of the VDP state — which, taken in
                // V-Blank after a game has rewritten CRAM for the next frame, cannot show a single mid-frame
                // palette effect. Published *before* the release below, because that release drops the
                // retained pixels along with the line log. A no-op while nothing is served, and skipped
                // internally while nobody is connected.
                bus.publish(&cap);
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

        // --- The hosted Aether bus: one bounded, non-blocking drain per iteration. ---
        //
        // This is the whole seam. It runs *after* the frame and its publish (so a client asking for the
        // screen this iteration gets the frame just drawn, not the one before it) and *before* the present
        // (so a client-driven run reaches the glass without a frame of lag).
        //
        // Nothing here can stall: the drain only ever `try_recv`s, every socket write happens on a connection
        // thread, and events go into per-connection queues that drop oldest-first rather than wait. A client
        // that stops reading stalls its own reader thread and nothing else. The two length bounds are the
        // bus's: `HOSTED_MAX_RUN_FRAMES` caps one command, `pump_budget` caps one drain.
        bus.set_paused(paused);
        let pumped = bus.pump(&mut sys);
        if pumped.timeline_moved {
            // A client advanced (or rewound) the machine behind the loop's back. That is the same class of
            // event as a save-state load, and it needs the same two repairs: audio belongs to a timeline
            // that has moved, and the capture is holding lines from before the jump.
            frame += pumped.frames_advanced;
            cap.clear();
            #[cfg(feature = "audio")]
            resync_audio(audio.as_mut());
        }
        if pumped.screen_changed {
            // The bus's advancing calls run their own scanline capture (this loop's is not attached to
            // them), so the frame they drew lives there. Pull it in; `None` means the run drew no complete
            // frame, in which case the retained image stays up exactly as it does for a 0-frame iteration.
            if let Some(w) = bus.present_frame(&mut buf) {
                width = w;
            }
        }
        if pumped.rom_changed {
            // `emulator/reload_rom` (or a restore that brought a different cartridge back) changed the bytes
            // under us. Re-derive the save-state fingerprint or every slot written for the previous image
            // would silently load into this one.
            rom_fp = save_state::rom_fingerprint(sys.rom());
            notify(
                &mut ov,
                ACCENT,
                "aether: the cartridge was replaced over the bus — save-state slots re-keyed",
            );
        }
        // Conflict 1's inbound half: `emulator/pause` / `emulator/resume` are the client's way of stopping
        // and starting *this* loop, and they only mean anything if the loop follows them.
        let bus_paused = bus.is_paused();
        if bus_paused != paused {
            paused = bus_paused;
            notify(
                &mut ov,
                ACCENT,
                if paused {
                    "aether: paused by a client"
                } else {
                    "aether: resumed by a client"
                },
            );
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

        // The settings twin of the above: armed by whichever dispatch arm changed a persisted value, and
        // written once the user has stopped changing it. A failed write is a toast, not a crash — the
        // session keeps running with the setting live in memory.
        if let Some(n) = config_save_countdown {
            config_save_countdown = if n == 0 {
                if let Some(p) = &cfg_path {
                    match config::save(p, &cfg) {
                        Ok(()) => cfg_saved = cfg.clone(),
                        Err(e) => notify_err(
                            &mut ov,
                            format!("config: save to {} failed: {e}", p.display()),
                        ),
                    }
                }
                None
            } else {
                Some(n - 1)
            };
        }

        // Optional debug marker: a contrasting crosshair at the watched pixel so the live driver can confirm
        // the click landed where intended (bounds-guarded against an H40→H32 mode switch since the click).
        // Drawn into a scratch copy, never into `buf`: the crosshair is an XOR, and a paused frontend
        // re-presents the same buffer every iteration — applied in place it would flicker and smear.
        let native: &[u32] = match watched_pixel {
            Some((wx, wy)) => {
                marked.clear();
                marked.extend_from_slice(&buf);
                draw_crosshair(&mut marked, width, wx, wy);
                &marked
            }
            None => &buf,
        };
        let title = if paused {
            format!("oracle-next — frame {frame} [PAUSED]")
        } else {
            format!("oracle-next — frame {frame}")
        };
        window.set_title(&title);

        // Scale the native frame into a window-sized buffer ourselves and draw the overlay on top of *that*.
        // `buf` — the retained framebuffer — is never written here, which is what stops a re-presented frame
        // accumulating overlay ink across the many iterations a paused (or 0-frame) loop spends on one image.
        //
        // Re-derived rather than reusing `view`: the frames run above can have switched H32↔H40, and the
        // picture must be fitted to the width actually being presented. `view` stays what it was — the
        // geometry of the frame the user was looking at when they clicked.
        let present_view = present::dest_rect(win_w, win_h, width, HEIGHT, aspect);
        present::scale_into(
            &mut screen,
            win_w,
            win_h,
            present::Frame {
                px: native,
                w: width,
                h: HEIGHT,
            },
            present_view,
            0x0000_0000,
            &mut xmap,
        );
        ov.tick();
        #[cfg(feature = "audio")]
        let (vol, filt) = (
            Some((volume, audio::VOLUME_STEPS, muted)),
            audio.as_ref().map(|a| a.filter),
        );
        #[cfg(not(feature = "audio"))]
        let (vol, filt) = (None, None);
        // Lenses: under the palette and the toasts, over the picture (spec §5). Models are built only for
        // what is on — with everything off and the machine running this is two bools and no reads at all —
        // and only into the *window* buffer. `buf`, the retained framebuffer, is re-presented every
        // iteration while paused, so ink there would accumulate (the lesson `draw_crosshair` records above).
        //
        // `|| paused` because the CPU chip auto-shows while stopped (spec §5.3): the guard is about skipping
        // work nobody asked for, and a pause is itself the ask.
        if cfg.lenses.any() || paused {
            let models = lens::models(
                cfg.lenses,
                &lens::FrameCtx {
                    sys: &sys,
                    wp: bus.watchpoints_mut(),
                    symbols: symbols.as_ref(),
                    frame,
                    paused,
                },
            );
            lens::draw(&mut screen, win_w, win_h, present_view, &models);
        }
        // Under the toasts, over the picture: drawn first so `ov.draw` still lands on top (a notification
        // must stay readable while the palette is open). Same buffer, same rect the present uses.
        palette.draw(&mut screen, win_w, win_h, present_view, &reg);
        ov.draw(
            &mut screen,
            win_w,
            win_h,
            present_view,
            &Status {
                paused,
                frame,
                slot: state_slot,
                occupied: slots_on_disk,
                volume: vol,
                filter: filt,
                aspect: aspect.name(),
                native: (width, HEIGHT),
            },
        );

        // update_with_buffer both presents and pumps the OS event queue; it honours set_target_fps. The
        // buffer is exactly the window's size, so `ScaleMode::Stretch` is a 1:1 present.
        if let Err(e) = window.update_with_buffer(&screen, win_w, win_h) {
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

    // Settings on quit (spec §7). The rule, against `cfg_saved` (what is believed to be on disk) rather
    // than what loaded: write when the live config differs from disk, OR when a debounced write was still
    // pending as the window closed. Three consequences, all wanted — a session that changed nothing writes
    // nothing; a change already flushed by the debounce is not written a second time; and a mid-session
    // save that FAILED is retried here, because the failure cleared the countdown but left the baseline
    // behind. A same-bytes rewrite remains possible (a setting toggled back with its countdown still
    // armed) and is harmless: the save is atomic.
    if let Some(p) = &cfg_path {
        // No `cfg_saved` update on success here: nothing runs after this, so the compiler rightly calls
        // that assignment dead. The baseline is advanced at the in-loop save, which is the only site whose
        // result a later iteration can observe.
        if cfg != cfg_saved || config_save_countdown.is_some() {
            if let Err(e) = config::save(p, &cfg) {
                eprintln!("config: save on quit to {} failed: {e}", p.display());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The command line parses as documented, and a bad value is refused rather than silently ignored.
    #[test]
    fn the_command_line_parses_scale_and_aspect() {
        let a = |v: &[&str]| parse_args_from(v.iter().map(|s| s.to_string()));
        let ok = a(&["rom.bin"]).expect("a bare ROM path is enough");
        assert_eq!(ok.rom_path, "rom.bin");
        assert_eq!(
            ok.scale, None,
            "unset — the built-in default now lives in resolve_scale/config::Config"
        );
        assert_eq!(
            ok.aspect, None,
            "unset — a player defaults to the console's own 4:3 only after resolve_aspect"
        );

        let both = a(&["--scale", "2", "--aspect", "integer", "rom.bin"]).unwrap();
        assert_eq!((both.scale, both.aspect), (Some(2), Some(Aspect::Integer)));

        assert!(a(&[]).is_err(), "no ROM path");
        assert!(a(&["a.bin", "b.bin"]).is_err(), "two ROM paths");
        assert!(
            a(&["--scale", "0", "rom.bin"]).is_err(),
            "scale out of range"
        );
        assert!(a(&["--scale", "9", "rom.bin"]).is_err());
        assert!(a(&["--scale"]).is_err(), "missing value");
        assert!(
            a(&["--aspect", "cinema", "rom.bin"]).is_err(),
            "unknown aspect"
        );
        assert!(a(&["--aspect"]).is_err());
        assert!(a(&["--nope", "rom.bin"]).is_err(), "unknown flag");
    }

    /// Config precedence (spec §7) needs to know whether a flag was actually typed, not just what value it
    /// would take — an unset `--scale` must read as `None` so `resolve_scale` can fall through to the config
    /// file, not as the numeric default `3` (which would make the config file's own `scale` unreachable).
    #[test]
    fn args_report_explicitness_for_config_precedence() {
        let a = parse_args_from(["rom.bin".to_string()]).unwrap();
        assert_eq!(
            a.scale, None,
            "unset scale must be None so config can fill it"
        );
        assert_eq!(a.aspect, None);
        let a =
            parse_args_from(["rom.bin", "--scale", "5", "--aspect", "integer"].map(String::from))
                .unwrap();
        assert_eq!(a.scale, Some(5));
        assert_eq!(a.aspect, Some(Aspect::from_name("integer").unwrap()));
    }

    /// The precedence rule itself (spec §7): CLI beats config beats built-in default. `resolve_scale`/
    /// `resolve_aspect` are the pure fns every call site goes through instead of reading `args.scale`/
    /// `args.aspect` directly.
    #[test]
    fn resolve_prefers_cli_then_config() {
        let cfg = config::Config {
            scale: 6,
            aspect: Aspect::from_name("square").unwrap(),
            ..config::Config::default()
        };
        // CLI silent -> config wins.
        assert_eq!(resolve_scale(None, &cfg), 6);
        assert_eq!(resolve_aspect(None, &cfg), cfg.aspect);
        // CLI explicit -> CLI wins.
        assert_eq!(resolve_scale(Some(2), &cfg), 2);
        assert_eq!(
            resolve_aspect(Some(Aspect::default()), &cfg),
            Aspect::default()
        );
    }

    /// Serving the Aether bus is **opt-in**, and the default really is "no socket exists" — the whole
    /// "a default launch is unaffected" guarantee starts here.
    #[test]
    fn the_aether_bus_is_off_unless_asked_for() {
        let a = |v: &[&str]| parse_args_from(v.iter().map(|s| s.to_string()));
        // The env var is read at parse time, so a stray one in the test environment would make this lie.
        assert!(
            std::env::var_os("ORACLE_AETHER").is_none(),
            "this test assumes ORACLE_AETHER is unset"
        );
        assert!(
            a(&["rom.bin"]).unwrap().socket.is_none(),
            "no flag, no socket — nothing is bound and nothing is created"
        );
        assert_eq!(
            a(&["--aether", "rom.bin"]).unwrap().socket,
            Some(None),
            "--aether serves on the contract's default path"
        );
        assert_eq!(
            a(&["--socket", "/tmp/x.sock", "rom.bin"]).unwrap().socket,
            Some(Some(std::path::PathBuf::from("/tmp/x.sock"))),
            "--socket implies --aether, and names the path"
        );
        // Order must not matter, and the explicit path must survive a later bare --aether.
        assert_eq!(
            a(&["--socket", "/tmp/x.sock", "--aether", "rom.bin"])
                .unwrap()
                .socket,
            Some(Some(std::path::PathBuf::from("/tmp/x.sock"))),
            "--aether after --socket must not throw the path away"
        );
        assert!(a(&["--socket"]).is_err(), "--socket needs a value");
    }

    /// Slot occupancy is read from the disk, and an absent file reads as empty rather than as an error — the
    /// slot strip's whole job is to say which slots have something in them.
    #[test]
    fn slot_occupancy_is_probed_from_disk() {
        let dir = std::env::temp_dir().join(format!("oracle-slots-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rom = dir.join("probe.bin");
        std::fs::write(&rom, b"not a rom").unwrap();
        let path = rom.to_str().unwrap();

        assert_eq!(
            probe_slots(path),
            [false; save_state::SLOT_COUNT],
            "nothing saved yet"
        );
        std::fs::write(save_state::state_path_for(&rom, 4), b"x").unwrap();
        let occ = probe_slots(path);
        assert!(occ[4], "slot 4 is occupied");
        assert_eq!(occ.iter().filter(|&&o| o).count(), 1, "and only slot 4");
        std::fs::remove_dir_all(&dir).ok();
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

    /// The keyboard-swallow latch (`release_latch`). Closing the palette arms it while the key that did
    /// the closing is still down; it clears only once every game key is released, so `Enter` — the key
    /// that ran the command — can never arrive at the game as Start.
    #[test]
    fn the_swallow_latch_clears_only_on_full_release() {
        // (latch now, any game key still held, latch next)
        let cases = [
            (false, false, false), // idle: nothing to suppress
            (false, true, false),  // ordinary gameplay never arms it
            (true, true, true),    // still holding the key that dismissed the palette
            (true, false, false),  // let go — the keyboard plays again from here
        ];
        for (latch, down, want) in cases {
            assert_eq!(
                release_latch(latch, down),
                want,
                "release_latch(latch={latch}, any_down={down})"
            );
        }
    }

    /// Slot stepping wraps in both directions and never leaves `0..SLOT_COUNT`.
    #[test]
    fn slot_stepping_wraps_and_reaches_every_slot() {
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
        // The number-key half of this used to live here; the registry owns those bindings now, and
        // `commands::tests::slot_selects_cover_all_slots` (plus `hotkeys_unique`) asserts it there.
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

    /// **Visual check for the presentation path** (`cargo test -p oracle-frontend -- --ignored
    /// --nocapture`). Renders real frames of a real game through *exactly* the code the run loop presents
    /// with — `blit_capture` → `present::scale_into` → `Overlay::draw` — and writes them out as PPM, because
    /// nobody can review a windowed frontend by reading its unit tests. Ignored by default: it needs a ROM,
    /// which the repository does not carry.
    ///
    /// `ORACLE_SHOT_ROM` names the ROM (a `<rom>.state0` beside it is loaded when present), `ORACLE_SHOT_DIR`
    /// where the images go. Nothing here is a pass/fail assertion beyond "it produced images"; the point is
    /// the images.
    #[test]
    #[ignore = "needs a ROM: set ORACLE_SHOT_ROM (and optionally ORACLE_SHOT_DIR)"]
    fn write_presentation_screenshots() {
        let Ok(rom_path) = std::env::var("ORACLE_SHOT_ROM") else {
            panic!("set ORACLE_SHOT_ROM to a Genesis ROM");
        };
        let dir = std::env::var("ORACLE_SHOT_DIR")
            .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into());
        std::fs::create_dir_all(&dir).unwrap();
        let rom = std::fs::read(&rom_path).expect("readable ROM");
        let rom_fp = save_state::rom_fingerprint(&rom);

        let mut sys = System::new(0x5EED);
        sys.load_rom(rom.clone());
        sys.reset();
        // Prefer a save state (a real in-game scene beats a title screen); fall back to booting a while.
        let state = save_state::state_path_for(std::path::Path::new(&rom_path), 0);
        match save_state::load(&state, rom_fp) {
            Ok(loaded) => {
                println!("shots: loaded {}", state.display());
                sys = loaded;
            }
            Err(e) => println!("shots: no usable state ({e}) — booting instead"),
        }

        // Advance far enough to be past any blank frame, exactly as the run loop does it.
        let mut cap = ScanlineCapture::new(Retain::LastFrame);
        let mut buf: Vec<u32> = Vec::new();
        let mut width = 0usize;
        for _ in 0..12 {
            sys.run_frames_with_sink(1, &mut cap);
            if let Some(w) = blit_capture(&cap, &mut buf) {
                width = w;
            }
            if cap.frames_completed() >= 1 && cap.lines().len() == HEIGHT {
                cap.clear();
            }
        }
        assert!(width > 0, "no frame was captured");
        println!("shots: native frame {width}x{HEIGHT}");

        // --- Task 1, against a real game: what does a click actually resolve to now? ---
        // Sweep the frame, tally what each dot is, and report the picker's answer for a real sprite dot and
        // a real plane dot. On the old code every sprite dot printed "no tile watch this slice (follow-up)".
        let (mut sprite_dots, mut plane_dots, mut backdrop_dots) = (0usize, 0usize, 0usize);
        // The sprite dot nearest the middle of the screen — i.e. the player character rather than the HUD,
        // which in S3K is also sprites and would otherwise always win by being first in the sweep.
        let (cx, cy) = (width as i32 / 2, HEIGHT as i32 / 2);
        let mut a_sprite_dot: Option<(u16, u16, i32)> = None;
        for y in (0..HEIGHT as u16).step_by(4) {
            for x in (0..width as u16).step_by(4) {
                match sys.vdp().pixel_attribution(x, y).winner {
                    oracle_core::render::Layer::Sprite(_) => {
                        sprite_dots += 1;
                        let d = (i32::from(x) - cx).pow(2) + (i32::from(y) - cy).pow(2);
                        if a_sprite_dot.is_none_or(|(_, _, best)| d < best) {
                            a_sprite_dot = Some((x, y, d));
                        }
                    }
                    oracle_core::render::Layer::Backdrop => backdrop_dots += 1,
                    _ => plane_dots += 1,
                }
            }
        }
        println!(
            "shots: sampled dots — {sprite_dots} sprite, {plane_dots} plane/window, {backdrop_dots} backdrop"
        );
        assert!(
            sprite_dots > 0,
            "the scene must contain sprites to test against"
        );
        let (sx, sy, _) = a_sprite_dot.unwrap();
        let sprite_pick = pick::resolve(sys.vdp(), sx, sy);
        println!("shots: click ({sx},{sy}) -> {}", sprite_pick.description);
        println!("shots:   toast: {}", sprite_pick.toast);
        for t in &sprite_pick.targets {
            println!(
                "shots:   arms {:?} ${:04X}-${:04X}  \"{}\"",
                t.space, t.lo, t.hi, t.label
            );
        }
        assert_eq!(
            sprite_pick.targets.len(),
            2,
            "a sprite click arms its tile and its SAT entry"
        );

        // "Before": what the old path put on screen — the native frame, nearest-scaled 3x by minifb, with no
        // overlay and no aspect correction.
        let mut before = Vec::new();
        let mut xmap = Vec::new();
        let (bw, bh) = (width * 3, HEIGHT * 3);
        present::scale_into(
            &mut before,
            bw,
            bh,
            present::Frame {
                px: &buf,
                w: width,
                h: HEIGHT,
            },
            present::Rect {
                x: 0,
                y: 0,
                w: bw,
                h: bh,
            },
            0,
            &mut xmap,
        );
        write_ppm(&format!("{dir}/before-3x-square.ppm"), &before, bw, bh);

        // "After": the new presentation buffer, per aspect mode, with the overlay the run loop draws.
        let mut ov = Overlay::new();
        ov.status_line = true;
        ov.push("STATE: SAVED SLOT 3", ACCENT);
        ov.push("WATCH SPRITE 12 TILE $2A0 + SAT $F080", INFO);
        ov.push("VOLUME 7/10", INFO);
        let mut occupied = [false; save_state::SLOT_COUNT];
        occupied[0] = true;
        occupied[3] = true;
        occupied[7] = true;

        for (name, aspect, (ww, wh)) in [
            ("after-tv", Aspect::Tv, (896usize, 672usize)),
            ("after-square", Aspect::Square, (960, 672)),
            ("after-integer", Aspect::Integer, (1000, 700)),
            ("after-tv-wide", Aspect::Tv, (1280, 600)),
        ] {
            let view = present::dest_rect(ww, wh, width, HEIGHT, aspect);
            let mut screen = Vec::new();
            present::scale_into(
                &mut screen,
                ww,
                wh,
                present::Frame {
                    px: &buf,
                    w: width,
                    h: HEIGHT,
                },
                view,
                0x0000_0000,
                &mut xmap,
            );
            ov.draw(
                &mut screen,
                ww,
                wh,
                view,
                &Status {
                    paused: name == "after-tv-wide",
                    frame: 4211,
                    slot: 3,
                    occupied,
                    volume: Some((7, 10, false)),
                    filter: Some("VA0-VA2"),
                    aspect: aspect.name(),
                    native: (width, HEIGHT),
                },
            );
            write_ppm(&format!("{dir}/{name}.ppm"), &screen, ww, wh);
            println!("shots: {name} {ww}x{wh} view {view:?}");
        }

        // How long the present costs, since the audio pacer only holds if the loop keeps up with the device.
        // The blit is the frontend's own work now (minifb used to do it in C), so it is worth a number.
        {
            let view = present::dest_rect(896, 672, width, HEIGHT, Aspect::Tv);
            let mut screen = Vec::new();
            let t = std::time::Instant::now();
            const N: u32 = 200;
            for _ in 0..N {
                present::scale_into(
                    &mut screen,
                    896,
                    672,
                    present::Frame {
                        px: &buf,
                        w: width,
                        h: HEIGHT,
                    },
                    view,
                    0,
                    &mut xmap,
                );
                ov.draw(
                    &mut screen,
                    896,
                    672,
                    view,
                    &Status {
                        paused: true,
                        aspect: "4:3",
                        native: (width, HEIGHT),
                        ..Status::default()
                    },
                );
            }
            println!(
                "shots: present cost {:.3} ms/frame at 896x672 (budget 16.7 ms)",
                t.elapsed().as_secs_f64() * 1000.0 / f64::from(N)
            );
        }

        // …and one clean frame with no overlay at all, to prove the overlay is additive and the picture
        // underneath is untouched.
        let view = present::dest_rect(896, 672, width, HEIGHT, Aspect::Tv);
        let mut clean = Vec::new();
        present::scale_into(
            &mut clean,
            896,
            672,
            present::Frame {
                px: &buf,
                w: width,
                h: HEIGHT,
            },
            view,
            0,
            &mut xmap,
        );
        write_ppm(&format!("{dir}/after-tv-nooverlay.ppm"), &clean, 896, 672);

        // The whole click path, end to end, on a real sprite: crosshair at the clicked dot + the toast the
        // click produces. The retained `buf` is deliberately not the buffer the crosshair goes into.
        let retained = buf.clone();
        let mut marked = buf.clone();
        draw_crosshair(&mut marked, width, sx, sy);
        assert_ne!(marked, retained, "the crosshair marked the scratch copy");
        let mut shot = Vec::new();
        present::scale_into(
            &mut shot,
            896,
            672,
            present::Frame {
                px: &marked,
                w: width,
                h: HEIGHT,
            },
            view,
            0,
            &mut xmap,
        );
        let mut click_ov = Overlay::new();
        click_ov.push(sprite_pick.toast.clone(), INFO);
        click_ov.draw(
            &mut shot,
            896,
            672,
            view,
            &Status {
                aspect: "4:3",
                native: (width, HEIGHT),
                ..Status::default()
            },
        );
        write_ppm(&format!("{dir}/after-sprite-click.ppm"), &shot, 896, 672);
        assert_eq!(
            buf, retained,
            "…and the retained framebuffer itself is never written — the invariant the whole \
             scratch-copy dance exists for"
        );
    }

    /// Dump a packed `0x00RR_GGBB` buffer as a binary PPM (P6) — the least machinery that produces an image
    /// any viewer can open.
    #[cfg(test)]
    fn write_ppm(path: &str, buf: &[u32], w: usize, h: usize) {
        use std::io::Write;
        let mut out = Vec::with_capacity(15 + w * h * 3);
        out.extend_from_slice(format!("P6\n{w} {h}\n255\n").as_bytes());
        for px in buf {
            out.push((px >> 16) as u8);
            out.push((px >> 8) as u8);
            out.push(*px as u8);
        }
        std::fs::File::create(path)
            .and_then(|mut f| f.write_all(&out))
            .unwrap_or_else(|e| panic!("cannot write {path}: {e}"));
        println!("shots: wrote {path}");
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
