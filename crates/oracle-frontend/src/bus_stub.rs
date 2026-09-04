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
    pub symbols_changed: bool,
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
    /// **The display layer mask, and in this build it is the only one that exists.** Same story as
    /// `watchpoints` above: the window's layer toggles predate nothing and depend on no bus, so the state
    /// they move has to exist in both builds, and owning it here is what lets the run loop have a single
    /// shape. In the served build this same accessor pair reaches the *engine's* mask, which is what makes
    /// a socket client and the palette move one mask rather than two.
    ///
    /// It is the identical `oracle_core::render::LayerMask` type in both builds — deliberately, since a
    /// frontend-side notion of "hidden layers" that merely resembled the core's is the drift the whole
    /// arrangement exists to prevent.
    layers: oracle_core::render::LayerMask,
}

impl Default for Bus {
    fn default() -> Self {
        Self {
            paused: false,
            watchpoints: Watchpoints::new(crate::WATCH_CAP),
            profiler: oracle_core::profiler::Profiler::new(),
            layers: oracle_core::render::LayerMask::ALL,
        }
    }
}

/// This build's twin of [`crate::bus::NOT_SERVING`] — same opening words, and a reason that is a fact about
/// the binary rather than about the command line. Printed on the *default* path here too: "no bus" is a
/// statement this window owes its user either way, and a build that can never serve is the one case where
/// silence would be most easily mistaken for a bus that is merely idle.
const NOT_SERVING: &str =
    "aether: not serving — this binary was built without the `aether` feature, \
     so nothing can attach to this window";

impl Bus {
    pub fn start(socket: Option<Option<PathBuf>>, _info: MachineInfo) -> Self {
        // Exhaustive for the reason `bus.rs` gives at the same place: the arm that reports the quiet state
        // is the one whose deletion nothing else could notice.
        match socket {
            Some(_) => eprintln!(
                "aether: NOT serving — this binary was built without the `aether` feature. \
                 Rebuild with it to serve the bus."
            ),
            None => println!("{NOT_SERVING}"),
        }
        Self::default()
    }

    /// Always `false`, and that is this build's truth rather than a stub's shrug: nothing here binds a
    /// socket, so the status line's `AETHER` field is correct without a `#[cfg]` at its call site.
    pub fn is_serving(&self) -> bool {
        false
    }

    pub fn merge_held(&self, pads: [Pad; 2]) -> [Pad; 2] {
        pads
    }

    /// The panel's and the window's mask. Same signature as the served build's, so the run loop is one
    /// shape; there is simply no socket here for a second writer to arrive through.
    pub fn layers(&self) -> oracle_core::render::LayerMask {
        self.layers
    }

    pub fn set_layer(&mut self, layer: oracle_core::render::Layer, enabled: bool) -> bool {
        self.layers.set(layer, enabled)
    }

    pub fn set_live_pads(&mut self, _pads: [Pad; 2]) {}

    /// The screen-text seam, inert. Unreachable in practice as well as inert: the run loop only builds a
    /// snapshot when [`is_serving`](Self::is_serving) is true, and in this build it never is. Present so the
    /// loop keeps one shape — the whole reason this file exists.
    pub fn set_screen_text(&mut self, _surfaces: Vec<crate::screen_text::Surface>) {}

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
    ///
    /// **The breakpoint half is always `None`, and that too is a fact about this build rather than a
    /// shrug**: breakpoints are armed only over the bus (§6's `emulator/breakpoint_add`), and there is no
    /// bus here to arm one from, so there is never a halt for the loop to carry. `resume_pc` is taken and
    /// dropped for the same reason the `MachineInfo` fields are — the surface has to be the served build's
    /// or the run loop would need a `#[cfg]` of its own, which is how the two builds start meaning
    /// different things.
    pub fn run_sinks(
        &mut self,
        _resume_pc: u32,
    ) -> (
        Option<Observe<&mut Watchpoints>>,
        Option<Observe<&mut oracle_core::profiler::Profiler>>,
        Option<()>,
    ) {
        let armed = self.watchpoints.watch_count() > 0;
        (armed.then_some(Observe(&mut self.watchpoints)), None, None)
    }

    /// The served build's [`Bus::record_break`] twin. Unreachable here — [`run_sinks`](Bus::run_sinks)
    /// never hands out a sink that could fire — and kept so the run loop is one shape.
    pub fn record_break(&mut self, _addr: u32) {}

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

    /// Always `None`, and the run loop's `symbols_changed` branch is therefore unreachable in this build:
    /// `pump` reports nothing, so nothing ever asks. It exists for the same reason every other no-op here
    /// does — the loop has one shape, and a `#[cfg]` around the branch is how the two builds start
    /// meaning different things.
    pub fn symbols(&self) -> Option<&SymbolTable> {
        None
    }

    pub fn set_machine_info(&mut self, _info: MachineInfo) {}

    // ---------------------------------------------------------------- spawn mode (LIVE-OBJECTS)

    /// **Spawn mode cannot arm in this build, and it says why.**
    ///
    /// Not a shrug and not an empty list: an empty list would travel up to
    /// [`spawn::Mode::arm`](crate::spawn::Mode::arm) and be reported as *this build's listing names no
    /// archetype*, which is a true-sounding sentence about the wrong thing entirely. The reason a click
    /// places nothing here is that the whole capability layer — the mailbox rows, `object_at`, and
    /// `lookup_symbol` with it — is compiled out, and that is what the reader is told.
    pub fn archetypes(
        &mut self,
        _sys: &mut System,
    ) -> Result<crate::spawn::Archetypes, crate::spawn::Refusal> {
        Err(crate::spawn::Refusal::local(NO_BUS))
    }

    /// The served build's [`spawn_at`](crate::bus::Bus::spawn_at) twin. Unreachable — the mode cannot arm
    /// here, so no click can reach this — and kept so the run loop is one shape.
    pub fn spawn_at(
        &mut self,
        _sys: &mut System,
        _archetype: &str,
        _dot: (u16, u16),
    ) -> Result<crate::spawn::Placed, crate::spawn::Refusal> {
        Err(crate::spawn::Refusal::local(NO_BUS))
    }
}

/// Why nothing can be spawned from this binary. One constant, so the two refusals above cannot start
/// giving different reasons for the same fact.
const NO_BUS: &str =
    "this binary was built without the `aether` feature, so it has no object-mutation \
                      rows to spawn through — rebuild with it to place objects";

/// The served build's [`break_observed`](crate::bus::break_observed) twin, and in this build it is a
/// function that can only answer `None` — [`Bus::run_sinks`] never hands out a sink, so the argument is
/// always `None` too. It exists so the run loop's call site is one shape in both builds.
pub fn break_observed(_brk: Option<()>) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The twin of `bus.rs`'s pin. Both builds open with the same words, so a reader who sees "aether: not
    /// serving" has learned the same thing either way and only the reason differs — the two files cannot be
    /// compiled together, so this pairing is what stands in for a shared assertion.
    #[test]
    fn the_not_serving_line_matches_the_served_builds_opening() {
        assert!(
            NOT_SERVING.starts_with("aether: not serving"),
            "{NOT_SERVING:?}"
        );
        // The reason is this build's own, and it must not claim a flag would help — nothing here can serve.
        assert!(NOT_SERVING.contains("aether"), "{NOT_SERVING:?}");
        assert!(
            !NOT_SERVING.contains("--aether"),
            "this build cannot serve, so offering the flag as a remedy would be a lie: {NOT_SERVING:?}"
        );
    }

    /// **A click in this build refuses for the right reason.** The failure mode this guards is the one
    /// named on [`Bus::archetypes`]: an empty list would surface as *this listing has no archetypes*,
    /// which is a plausible sentence about the wrong cause and would send the reader to their symbol file.
    #[test]
    fn a_build_with_no_bus_blames_the_build_and_not_the_symbol_listing() {
        let mut b = Bus::default();
        let mut sys = System::new(0x5EED);
        let e = b
            .archetypes(&mut sys)
            .expect_err("this build cannot list archetypes");
        assert!(e.message.contains("aether"), "{:?}", e.message);
        assert!(
            !e.message.contains(crate::spawn::ARCHETYPE_PREFIX),
            "the reason is the build, not a missing prefix: {:?}",
            e.message
        );
        assert_eq!(
            b.spawn_at(&mut sys, "ObjDef_Ring", (10, 10)).unwrap_err(),
            e,
            "both halves must give one reason"
        );
    }

    /// This build never serves, and says so rather than leaving the status line to guess.
    #[test]
    fn is_serving_is_false_in_a_build_with_no_bus() {
        assert!(!Bus::start(None, MachineInfo::default()).is_serving());
        assert!(!Bus::start(Some(None), MachineInfo::default()).is_serving());
    }
}
