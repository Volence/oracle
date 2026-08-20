//! The `--no-default-features` twin of [`crate::bus`]: the same surface, every method a no-op.
//!
//! It exists so the run loop has **one** shape. The alternative — a `#[cfg(feature = "aether")]` around every
//! place the loop meets the bus — is how a feature-gated path quietly stops compiling, and worse, how the two
//! builds start meaning different things. Here the gated build is a build in which every one of those calls
//! provably does nothing, which is exactly what the claim "the default launch is unaffected" needs in order
//! to be checkable rather than argued.
//!
//! `Bus::start` is the only method with an observable effect, and only on the path where the user explicitly
//! asked for a socket from a binary that cannot provide one: it says so, rather than silently ignoring the
//! flag.

use oracle_core::bus::Observe;
use oracle_core::io::Pad;
use oracle_core::scanline_capture::ScanlineCapture;
use oracle_core::symbols::SymbolTable;
use oracle_core::system::System;
use oracle_core::watchpoints::Watchpoints;
use std::path::PathBuf;

/// What the bus would know about the loaded cartridge. Carried and dropped — the fields are unread here on
/// purpose, because the whole point of this build is that there is nothing to tell.
#[derive(Default)]
#[allow(dead_code)]
pub struct MachineInfo {
    pub rom_path: Option<String>,
    pub symbols: Option<SymbolTable>,
    pub symbols_path: Option<String>,
}

/// What one `Bus::pump` did. Always nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pumped {
    pub timeline_moved: bool,
    pub screen_changed: bool,
    pub rom_changed: bool,
    pub frames_advanced: u64,
}

/// The inert bus. Holds exactly two things: the pause state it was told, so that reading it back is an
/// identity rather than a lie, and the watch instrument the run loop feeds. The loop assigns `paused` from
/// `is_paused` unconditionally and arms its watches through `watchpoints_mut` unconditionally, and both must
/// keep working in this build too.
pub struct Bus {
    paused: bool,
    /// **The one thing in this file that is not a no-op, and it is not one for a structural reason.** The
    /// pixel-attribution panel is unconditional — it predates the bus and does not depend on it — so the
    /// instrument it reads has to exist in both builds. Owning it here is what lets the run loop have a
    /// single shape: it always feeds `bus.watchpoints_mut()`, which in the served build is the engine's own
    /// instrument and here is simply the panel's. Nothing is exposed and nothing is served; the panel that
    /// arms it is the only thing that ever reads it.
    watchpoints: Watchpoints,
    /// **A profiler that is real, empty, and permanently disarmed.** Nothing in this build can arm it —
    /// the profiler is armed only over the bus (§6's `emulator/set_profiler`), and there is no bus here —
    /// so it never accumulates. It exists rather than being an `Option` so that
    /// [`read_instruments`](Bus::read_instruments) has the served build's exact signature and the profiler
    /// panel has the same shape in both builds: a header saying it is off, over a sample with no frames in
    /// it. That is the honest picture of this build, and it is one code path rather than two.
    profiler: oracle_core::profiler::Profiler,
}

impl Default for Bus {
    fn default() -> Self {
        Self {
            paused: false,
            watchpoints: Watchpoints::new(crate::WATCH_CAP),
            profiler: oracle_core::profiler::Profiler::new(),
        }
    }
}

impl Bus {
    pub fn start(socket: Option<Option<PathBuf>>, _info: MachineInfo) -> Self {
        if socket.is_some() {
            eprintln!(
                "aether: NOT serving — this binary was built without the `aether` feature. \
                 Rebuild with it to serve the bus."
            );
        }
        Self::default()
    }

    pub fn merge_held(&self, pads: [Pad; 2]) -> [Pad; 2] {
        pads
    }

    pub fn set_live_pads(&mut self, _pads: [Pad; 2]) {}

    /// The panel's instrument. Same signature as the served build's, so the run loop is one shape.
    pub fn watchpoints_mut(&mut self) -> &mut Watchpoints {
        &mut self.watchpoints
    }

    /// The instruments the run loop feeds, in the served build's shape. The watch half is the panel's own
    /// and is attached on the same condition; the **profiler half is always `None`**, and that is a fact
    /// about this build rather than a stub's shrug — the profiler is armed only over the bus (§6's
    /// `emulator/set_profiler`), and this build has no bus to arm it from, so there is never a sample for
    /// the loop to feed.
    ///
    /// Nothing here can arm `stopAfter` either, for the same reason, but the [`Observe`] wrapper is kept:
    /// the one place the two builds must not differ is the shape of what the loop attaches to its run.
    pub fn run_sinks(
        &mut self,
    ) -> (
        Option<Observe<&mut Watchpoints>>,
        Option<Observe<&mut oracle_core::profiler::Profiler>>,
    ) {
        let armed = self.watchpoints.watch_count() > 0;
        (armed.then_some(Observe(&mut self.watchpoints)), None)
    }

    /// What the lens layer reads, in the served build's shape. The profiler is the empty one above and the
    /// armed flag is unconditionally `false` — not a stub's shrug, but the truth about a build with no bus
    /// to arm it from.
    pub fn read_instruments(&self) -> (&Watchpoints, &oracle_core::profiler::Profiler, bool) {
        (&self.watchpoints, &self.profiler, false)
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Exactly what [`set_paused`](Bus::set_paused) was last told. Nothing outside the window can change it
    /// in this build, so reading it back is the identity the loop needs it to be.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn publish(&mut self, _cap: &ScanlineCapture) {}

    pub fn present_frame(&self, _buf: &mut Vec<u32>) -> Option<usize> {
        None
    }

    pub fn pump(&mut self, _sys: &mut System) -> Pumped {
        Pumped::default()
    }

    pub fn set_machine_info(&mut self, _info: MachineInfo) {}
}
