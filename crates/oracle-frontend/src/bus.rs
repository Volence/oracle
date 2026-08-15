//! The player's side of the **Aether capability layer**, hosted in this process.
//!
//! The decision this implements: *the player process owns the machine, hosts the capability layer, and serves
//! Aether from it.* It is forced by the contract rather than chosen for convenience — a checkpoint is "a
//! serialization of the live emulator struct" (D13/§6.1), every reply carries the machine's `frame`/`mclk` at
//! reply time (D11), and the trust model is the emulator process serving a loopback-only socket it created
//! (D8). None of those can be honoured by a layer that does not have hands on this `System`.
//!
//! Everything hard lives in [`oracle_aether::host`]. What is left here is the four places the player's own
//! loop has to meet it, and this module exists mostly so those four places read as one thing:
//!
//! 1. **Pause is one flag.** The player's pause state *is* the bus's free-run state, and a client's
//!    `emulator/pause` is the player's. Written before the drain, read back after it.
//! 2. **Pads merge.** A client's held set ORs with live keyboard/gamepad input, exactly as the keyboard and
//!    the gamepad already OR with each other.
//! 3. **The picture is published.** The frame the window drew is the frame `emulator/screenshot` serves.
//! 4. **The drain is bounded.** One call per iteration, `try_recv`-only, with a wall-clock budget — see
//!    [`oracle_aether::host::HostConfig::pump_budget`].
//!
//! **Serving is opt-in** (`--aether`, or `--socket PATH`). Without it nothing binds, no filesystem entry is
//! created, no thread starts, and the pump is an identity operation on the machine.
//!
//! The mirror image of this file, [`crate::bus`] in a build without the `aether` feature, is a set of
//! no-ops with the same surface — which is why the run loop calls into it unconditionally and has no
//! `#[cfg]` of its own.

use oracle_aether::host::{Host, HostConfig};
use oracle_core::bus::Observe;
use oracle_core::io::Pad;
use oracle_core::scanline_capture::ScanlineCapture;
use oracle_core::symbols::SymbolTable;
use oracle_core::system::System;
use oracle_core::watchpoints::Watchpoints;
use std::path::PathBuf;

/// What the bus should know about the loaded cartridge — the ROM's path, and the listing bound to it (D7).
/// Declared here rather than re-exported so the stub build has the identical type.
#[derive(Default)]
pub struct MachineInfo {
    pub rom_path: Option<String>,
    pub symbols: Option<SymbolTable>,
    pub symbols_path: Option<String>,
}

impl From<MachineInfo> for oracle_aether::host::MachineInfo {
    fn from(m: MachineInfo) -> Self {
        Self {
            rom_path: m.rom_path,
            symbols: m.symbols,
            symbols_path: m.symbols_path,
        }
    }
}

/// What one [`Bus::pump`] did, in the terms the run loop reacts to. Deliberately not `oracle-aether`'s own
/// report type: the stub build has to produce this too, and the loop must not be able to tell them apart.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pumped {
    /// The machine's clock moved under the loop (a client-driven run, or a checkpoint restore that rewound
    /// it). Audio, the frame counter and the scanline capture all have to resynchronise, exactly as they do
    /// after a save-state load.
    pub timeline_moved: bool,
    /// A client-driven run replaced the picture — present [`Bus::present_frame`] instead of the retained
    /// framebuffer, which is now a frame from before the run.
    pub screen_changed: bool,
    /// The cartridge was replaced (`emulator/reload_rom`, or a restore that may have). Anything derived from
    /// the ROM bytes is stale.
    pub rom_changed: bool,
    /// Whole emulated frames the drain advanced.
    pub frames_advanced: u64,
}

/// The hosted capability layer, or an inert placeholder when `--aether` was not asked for.
pub struct Bus {
    host: Host,
}

impl Bus {
    /// Build the layer. When `socket` is `None` **nothing is bound** — the returned bus is inert and every
    /// method below is a no-op, which is what makes the default launch identical to a build without any of
    /// this.
    ///
    /// A bind failure is reported and degraded to inert, never fatal: someone who launched a game to play it
    /// should not be stopped by a stale socket file, and the message says exactly what did not happen.
    pub fn start(socket: Option<Option<PathBuf>>, info: MachineInfo) -> Self {
        // The hit ring is sized by the *player*, not by the headless default: this instrument is shared with
        // the pixel-attribution panel (see [`Bus::watchpoints_mut`]), and a panel that silently held fewer
        // hits when the bus happened to be compiled in would be the drift item 19 exists to prevent, wearing
        // a config value's clothes.
        let mut config = HostConfig::default();
        config.engine.watch_ring_cap = crate::WATCH_CAP;
        let mut host = Host::new(config);
        host.set_machine_info(info.into());
        if let Some(path) = socket {
            match host.serve(path) {
                Ok(p) => println!(
                    "aether: serving on {} (mode 0600, {} methods, protocol version {})",
                    p.display(),
                    oracle_aether::engine::METHODS.len(),
                    oracle_aether::rpc::PROTOCOL_VERSION
                ),
                Err(e) => eprintln!("aether: NOT serving — cannot bind the socket ({e})"),
            }
        }
        Self { host }
    }

    /// Conflict 2 — merge the client's held set into the pads the loop is about to write. See
    /// [`Host::held`](oracle_aether::host::Host::held).
    pub fn merge_held(&self, pads: [Pad; 2]) -> [Pad; 2] {
        if !self.host.is_serving() {
            return pads;
        }
        [
            oracle_aether::engine::merge_pads(pads[0], self.host.held(0)),
            oracle_aether::engine::merge_pads(pads[1], self.host.held(1)),
        ]
    }

    /// The other half of conflict 2: tell the bus what the human is holding, so `emulator/press` and
    /// `emulator/hold` compose with it instead of erasing it.
    pub fn set_live_pads(&mut self, pads: [Pad; 2]) {
        self.host.set_live_pads(pads);
    }

    /// **Conflict 4: the watch instrument the player's own run loop must feed.**
    ///
    /// The player owns the loop, so a `Watchpoints` that only the bus's own runs fed would see nothing while
    /// the window is running the game — and would report `seen == 0`, which correctly means "the recorder was
    /// never attached" and is useless. So the loop arms and reads *this* one instead of a private instance,
    /// and the pixel-attribution panel and `emulator/watchpoint_hits` become two readers of one instrument
    /// rather than two instruments that have to be kept in step (contract §8 item 19).
    ///
    /// Returned unconditionally, not only while serving: a `Host` exists either way, and switching
    /// instruments on `--aether` would mean the panel behaved differently depending on whether a socket was
    /// bound — a difference nobody asked for and the harder one to debug.
    pub fn watchpoints_mut(&mut self) -> &mut Watchpoints {
        self.host.watchpoints_mut()
    }

    /// The same instrument, wrapped for **attaching to this loop's run** — see
    /// [`Observe`](oracle_core::bus::Observe). A watch armed over the socket with `stopAfter` raises
    /// `stop_requested` on a level, not an edge, so attaching the bare instrument would end every one of
    /// this loop's 1-frame runs before it began: a client's stop condition turned into a frozen window on a
    /// machine nobody asked to pause. The observations still land; only the halt, which belongs to the runs
    /// a client bounded, is dropped.
    pub fn watch_sink(&mut self) -> Observe<&mut Watchpoints> {
        Observe(self.watchpoints_mut())
    }

    /// Conflict 1, outbound: the player's pause state becomes the bus's free-run state.
    pub fn set_paused(&mut self, paused: bool) {
        self.host.set_paused(paused);
    }

    /// Conflict 1, inbound: whether a client has paused or resumed the machine.
    pub fn is_paused(&self) -> bool {
        self.host.is_paused()
    }

    /// Conflict 3, the common half: hand the bus the frame the window just drew. Skipped internally while
    /// nobody is connected, so an unwatched player pays nothing for it.
    pub fn publish(&mut self, cap: &ScanlineCapture) {
        self.host.publish_capture(cap);
    }

    /// Conflict 3, the other half: after a client-driven run the window's retained framebuffer is a frame
    /// from before the run, and the bus is holding the one the raster actually drew. Pack it into `buf` and
    /// return its width.
    pub fn present_frame(&self, buf: &mut Vec<u32>) -> Option<usize> {
        let (width, px) = self.host.framebuffer()?;
        if width == 0 || px.is_empty() {
            return None;
        }
        buf.clear();
        buf.reserve(px.len());
        for &(r, g, b) in px {
            buf.push((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b));
        }
        Some(width)
    }

    /// Drain every queued command against the machine. Bounded, non-blocking, and the only point in the loop
    /// where anything other than the player touches the `System`.
    pub fn pump(&mut self, sys: &mut System) -> Pumped {
        let r = self.host.pump(sys);
        Pumped {
            timeline_moved: r.timeline_moved(),
            screen_changed: r.screen_changed,
            rom_changed: r.rom_changed,
            frames_advanced: r.frames_advanced(),
        }
    }

    /// Re-point the bus at a cartridge the *player* just swapped (F5). Without this `emulator/status` keeps
    /// naming the previous image and `read_memory {symbol}` keeps resolving against its listing — the exact
    /// stale-symbol hazard D7 exists to prevent.
    pub fn set_machine_info(&mut self, info: MachineInfo) {
        self.host.set_machine_info(info.into());
    }
}
