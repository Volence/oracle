//! **The player owns a `Host`** — the Aether capability layer, in-process, unbound.
//!
//! Parcel 2a shipped `oracle-aether` as a *dev*-dependency: the parity test needed to see both sides and
//! nothing on the shipped path reached the bus. Parcel 2b promotes it, because the Memory panel's writes,
//! its symbol lookups and its `memory_hash` all go through [`Host::call`] — the in-process read of the
//! same method registry that contract D15 says an in-process GUI *is*. A panel that showed a refusal it
//! composed itself would be a panel guessing at the server it lives inside.
//!
//! # What this does NOT do, and why the list is the design
//!
//! * **No [`Host::serve`].** No socket is bound, no filesystem entry is created and no thread is started.
//!   `Host::new`'s own doc states the guarantee this leans on: *"a player that never asks for the bus
//!   behaves exactly as it did before this existed."* Owning one costs a struct.
//!
//! Two entries that stood here have been **struck by parcel 3 (`PANELS-3-STOPPING`)**, which is the parcel
//! they named as their owner. They are kept, struck, because the reason they were deferred is the reason
//! the parcel that took them owed a re-measurement:
//!
//! * ~~**No [`Host::pump`] in the frame loop.**~~ There is one now, once per iteration, in
//!   [`Bus::mirror_pause`] — see that method for why the drain became unconditional and why it sits
//!   *after* the frame rather than before it.
//! * ~~**No `Observe` wrappers, no `run_sinks`, no `record_break`, no `publish_capture`.**~~ All four are
//!   here, and [`crate::machine::Machine::step`] carries them on every emulated frame.
//!
//! Parcel 1's pacing numbers were taken against a loop with neither. **They were retaken** — before and
//! after, on one rig in one session — and the result is in `docs/2026-09-03-debug-panels-design.md` §5.6.
//!
//! # ⚑ The pause mirror, which is the one subtle piece
//!
//! Fifteen served methods refuse a running machine with `-32005 machineRunning` — `write_memory`,
//! `write_cram` and `z80_write` among them. "Running", hosted, means *the player's loop is advancing the
//! machine*, and `Host`'s own doc puts the equivalence outright: **an un-paused player is a free-running
//! bus.** If the bus is not told, its `free_run` stays at [`Engine::new`]'s default of `false` and every
//! one of those fifteen would happily succeed against a machine running at 60 Hz — a poke landing in a
//! frame nobody chose, reported as success, with the panel telling a human the write was accepted because
//! the tool said so. That is the wrong answer this mirror exists to prevent, and it is a *silent* one.
//!
//! **[`Host::set_paused`] alone does not do it.** It queues into `pending_free_run` and the queue is
//! applied at the top of the next [`Host::pump`] — deliberately, so the `emulator/stopped` /
//! `emulator/resumed` it emits carries a truthful D11 stamp instead of the placeholder's `frame 0,
//! mclk 0`. `Host::call` explicitly declines to apply it (a second apply site would break the ordering
//! argument between `pending_free_run` and `pending_break`). So a `set_paused` with no drain behind it is
//! inert, and the mirror needs a drain.
//!
//! **Where the drain goes.** Parcel 2b put it at setup only — [`Bus::new`] pumped once and
//! [`Bus::mirror_pause`] pumped again only on a change that could not happen, because there was no pause
//! control. Parcel 3 adds the transport bar *and* the breakpoint sink, and both need a drain that runs
//! whether or not the pause moved: [`Host::record_break`] only **latches**, and the latch is applied at
//! the top of the next [`Host::pump`]. A change-gated pump would leave a machine that halts on a
//! breakpoint and never tells the loop — the exact silent failure `record_break`'s own doc calls "the
//! *worse* of the two failures". So the drain is now unconditional, once per iteration, and its cost is
//! measured rather than argued (`report`'s `bus-pump` bucket).
//!
//! # ⚑ The seam this crate rides, in the order it runs
//!
//! 1. [`Bus::run_sinks`] hands the run the two instruments (wrapped in `Observe`, which drops only their
//!    stop signal) and the breakpoint sink (**bare** — the halt is the whole point).
//! 2. [`crate::machine::Machine::step`] puts all three in the sink it already builds for the scanline
//!    capture, and runs the frame.
//! 3. [`break_observed`] consumes the breakpoint sink — which is what releases its borrow of the `Bus` —
//!    and [`Bus::record_break`] latches whatever it saw.
//! 4. [`Bus::publish`] hands the completed frame over, and [`Bus::mirror_pause`] drains: the latch lands,
//!    the run flags clear, and [`Bus::is_paused`] goes true.
//! 5. `Loop::iterate` reads that and stops running frames. **That last step is what makes a breakpoint
//!    mean anything against this window**: without it the bus reports a halt the loop never took.

use oracle_aether::breakpoints::BreakStop;
use oracle_aether::host::{Host, HostConfig, MachineInfo};
use oracle_aether::rpc::RpcError;
use oracle_core::bus::Observe;
use oracle_core::io::Pad;
use oracle_core::profiler::Profiler;
use oracle_core::scanline_capture::ScanlineCapture;
use oracle_core::system::System;
use oracle_core::watchpoints::Watchpoints;
use serde_json::{Map, Value};

/// The hosted capability layer.
///
/// Parcel 2b carried a `mirrored: Option<bool>` beside it, to gate the drain on an actual change of the
/// player's pause. **Parcel 3 removed it, and the removal is the point**: the drain is now unconditional
/// (see [`Bus::mirror_pause`]), so a cached copy of the pause could only ever answer a question nobody
/// asks — and a second place where this process believes it knows the engine's run state is precisely the
/// duplication R2 exists to prevent. [`Host::set_paused`] already compares against the engine's own flag,
/// so calling it every iteration with an unchanged value queues nothing.
pub struct Bus {
    host: Host,
}

/// One command's answer, as the tool would have received it: the handler's own reply or its own refusal.
///
/// The panel renders this and composes nothing of its own. That is the whole point of routing a gesture
/// through `Host::call` rather than reaching around it — a refusal a panel writes for itself is a
/// sentence about a server, not the server's.
pub enum Answer {
    Ok(Value),
    Err(RpcError),
}

impl Answer {
    /// The refusal's machine-readable discriminant (`error.data.reason`), which is the field clients
    /// branch on everywhere else on this bus — `machineRunning`, `noDisplay`, `unknownCheckpoint`. Never
    /// the message text: §5 is explicit that the text is for humans and the reason is for code, and a
    /// panel that matched on prose would break on a wording fix.
    pub fn reason(&self) -> Option<&str> {
        match self {
            Answer::Err(e) => e.data.as_ref()?.get("reason")?.as_str(),
            Answer::Ok(_) => None,
        }
    }

    /// Whether this is a refusal. The panel colours on **this**, never on the shape of the rendered
    /// string: a refusal that reads like a success is the one rendering mistake a debug surface cannot
    /// afford, and matching on a `"REFUSED"` prefix would be a second encoding of the same fact.
    pub fn is_err(&self) -> bool {
        matches!(self, Answer::Err(_))
    }
}

impl Bus {
    /// Build the hosted bus around `sys`, tell it about the cartridge, and land the initial pause mirror.
    ///
    /// `paused` is the player's own pause at launch — `false` for the player, which plays. See the module
    /// doc for why this pumps and why the pump is here rather than in the loop.
    pub fn new(sys: &mut System, info: MachineInfo, paused: bool) -> Self {
        let mut host = Host::new(HostConfig::default());
        host.set_machine_info(info);
        let mut bus = Bus { host };
        bus.mirror_pause(sys, paused);
        bus
    }

    /// Tell the bus what the player's loop is doing, and **make it land** — the loop's one drain.
    ///
    /// The drain is [`Host::pump`] rather than a new apply-the-pending entry point on purpose: `pump` is
    /// the *single* site where `pending_free_run` and `pending_break` are applied in that order, and
    /// [`Host::call`]'s doc spends a paragraph on why a second site reintroduces "a machine that stops on
    /// a breakpoint and silently resumes". Reusing the one site adds no interleaving the ordering argument
    /// does not already cover.
    ///
    /// **Unconditional, unlike parcel 2b's change-gated version.** A halt latched by
    /// [`record_break`](Bus::record_break) is applied at the top of the *next* pump and nowhere else, so a
    /// pump that only ran when the pause moved would apply it only if the pause happened to move — which
    /// on the frame a breakpoint fires it has not. The player would keep running past a breakpoint the bus
    /// believed it had stopped on.
    ///
    /// The [`PumpReport`](oracle_aether::host::PumpReport) is dropped, deliberately. Its three interesting
    /// flags — `timeline_moved`, `screen_changed`, `rom_changed` — all describe *a socket client* moving
    /// the machine behind the loop's back, and this player never binds one ([`Host::serve`] is not called
    /// anywhere in this crate). The one caller that can move the machine here is the transport bar, and it
    /// goes through [`Host::call`], which is not a drain and so cannot appear in this report at all.
    pub fn mirror_pause(&mut self, sys: &mut System, paused: bool) {
        self.host.set_paused(paused);
        self.host.pump(sys);
    }

    /// **Both instruments plus the breakpoint sink, for the frame the player is about to run.**
    ///
    /// The whole argument lives in [`Engine::run_sinks`](oracle_aether::engine::Engine::run_sinks): why
    /// they are lent rather than owned by the run driver, why the arming conditions are asked of the
    /// shared instruments rather than of this loop's state, and why the pair comes from one call (one run
    /// needs both, and two `&mut self` accessors cannot both be live in the sink expression).
    ///
    /// **R2 in its load-bearing form.** These are the engine's own `Watchpoints` and `Profiler` — the same
    /// ones `emulator/watchpoint_hits` and `emulator/get_profiler` read. A player that armed instruments of
    /// its own would give a panel and a `Host::call` two different answers about one frame, which is the
    /// drift R2 exists to prevent; there is nothing here to drift *from*.
    ///
    /// The third half is the breakpoint sink, **bare**. The two instruments are wrapped in
    /// [`Observe`], which forwards every observation and drops only `stop_requested`; an `Observe` around
    /// the breakpoint sink would count hits on a window that never stopped — the same believable wrong
    /// answer wearing the other hat. `resume_pc` is the machine's PC *before* the run, which the loop has
    /// and the engine (holding its placeholder `System` outside a drain) does not.
    pub fn run_sinks(
        &mut self,
        resume_pc: u32,
    ) -> (
        Option<Observe<&mut Watchpoints>>,
        Option<Observe<&mut Profiler>>,
        Option<BreakStop<'_>>,
    ) {
        self.host.run_sinks(resume_pc)
    }

    /// Hand back the halt the sink from [`run_sinks`](Bus::run_sinks) observed.
    ///
    /// This only **latches**; [`mirror_pause`](Bus::mirror_pause) is what applies it. Dropping the
    /// observation on the floor is the worse of the two failures — a machine that halts with nothing
    /// saying so — which is why the loop consumes the sink through [`break_observed`] rather than letting
    /// it fall out of scope.
    pub fn record_break(&mut self, addr: u32) {
        self.host.record_break(addr);
    }

    /// **What a panel reads** — the watch instrument, the profiler, and whether the profiler is armed.
    ///
    /// One call rather than three accessors, because a draw pass needs all three live at once and
    /// `run_sinks` is `&mut self`: the borrows could not coexist. Shared borrows throughout, which states
    /// the guarantee in the type — a panel cannot move a number a `Host::call` is gating on.
    ///
    /// The armed flag is not derivable from the accumulator: disarming RETAINS the sample, so rows exist
    /// whether or not anything is still recording, and a panel showing only the rows could not tell the
    /// two apart. Parcel 3 provides this; the three tabs that read it are the next parcel's.
    pub fn read_instruments(&self) -> (&Watchpoints, &Profiler, bool) {
        self.host.read_instruments()
    }

    /// **What the Breakpoints tab reads** — the armed set, shared, from the one list the `Host` owns.
    ///
    /// ⚑ **This is not part of `read_instruments`, and the parcel-3 brief that said it was is the thing
    /// source corrected.** `read_instruments` is `(&Watchpoints, &Profiler, bool)` and has never carried
    /// breakpoints: a breakpoint *halts* and is lent to a run bare, where the two instruments *record* and
    /// are lent wrapped in `Observe`. So the Breakpoints panel draws from here instead — and it is still a
    /// direct read of a shared derivation (design §4.4), still one list rather than two (R2), and still a
    /// borrow the panel cannot arm through. Both are `&self`, so a draw pass holds all four at once.
    ///
    /// The alternative considered and rejected was calling `emulator/breakpoint_list` every repaint. That
    /// is route (b) on a 60 Hz path: a `serde_json` page built sixty times a second to render a table that
    /// is already in memory, when §4.4's whole line is that a gesture pays for the tool's exact answer and
    /// a repaint does not.
    ///
    /// **`hits` outlives `enabled`.** Disabling retains the count (§6 — a fresh count means clear and
    /// re-add), so a row can read `disabled` beside a five-figure hit count, and that pairing is a fact
    /// about the past rather than a live one. The panel says which, in words, for
    /// [`read_instruments`](Bus::read_instruments)'s reason one instrument over.
    pub fn read_breakpoints(&self) -> &oracle_aether::breakpoints::Breakpoints {
        self.host.read_breakpoints()
    }

    /// Hand the bus the frame the player's own run just drew, so `emulator/screenshot` and
    /// `emulator/state_hash {includeFramebuffer}` answer with what is on the glass rather than a post-hoc
    /// re-render of the VDP state — which, taken in V-Blank after the game has rewritten CRAM for the next
    /// frame, cannot show a single mid-frame palette effect.
    ///
    /// Free while nobody is connected: [`Host::publish_capture`] is gated on `has_clients()` internally,
    /// and this player binds no socket, so today this is one atomic load and a return. It is wired anyway
    /// because the alternative is a seam that has never been exercised on the day something does connect.
    pub fn publish(&mut self, cap: &ScanlineCapture) {
        self.host.publish_capture(cap);
    }

    /// **Conflict 2, the half parcel 3 booked as not done** — OR the client's held set into the pads the
    /// loop is about to write.
    ///
    /// A delegation and not an implementation: the merge itself is
    /// [`Host::merge_held`](oracle_aether::host::Host::merge_held), the same function
    /// `oracle-frontend`'s `Bus::merge_held` now calls. Design §7's R1 named porting the frontend's model
    /// halves into shared code as the opportunity here, and a second `merge_held` in this crate would have
    /// been the drift the tabs parcel published `watch_wire_id` to avoid.
    ///
    /// **Not inert here, despite the socket this player never binds.** `Host::call` is in-process and
    /// reachable — the transport bar and every panel gesture already go through it (D15) — so
    /// `emulator/hold` can install a held set with `is_serving()` false. That is exactly why the hoisted
    /// merge has no `is_serving()` gate; see its doc.
    pub fn merge_held(&self, pads: [Pad; 2]) -> [Pad; 2] {
        self.host.merge_held(pads)
    }

    /// The other half: tell the bus what the human at *this* keyboard is holding, so `emulator/press` and
    /// `emulator/hold` compose with it instead of erasing it.
    pub fn set_live_pads(&mut self, pads: [Pad; 2]) {
        self.host.set_live_pads(pads);
    }

    /// **What the status strip shows a human** (design §9.4) — the client's held set on both ports, as the
    /// engine holds it.
    ///
    /// Deliberately the raw pads and not a sentence: the wording is [`crate::ui::StatusStrip`]'s and the
    /// button *names* are [`oracle_aether::engine::held_names`]'s, which is the same function
    /// `emulator/hold`'s reply `held` array is built from. Three spellings of "which buttons are down" is
    /// what this shape exists to prevent.
    pub fn held_pads(&self) -> [Pad; 2] {
        [self.host.held(0), self.host.held(1)]
    }

    /// Whether the **bus** believes the machine is paused. Read this, never a `call` to
    /// `emulator/status`: `Host::is_paused` consults the pending change and a `call` does not, so between
    /// a `set_paused` and its drain the two disagree and only this one is truthful.
    pub fn is_paused(&self) -> bool {
        self.host.is_paused()
    }

    /// One command, answered synchronously against the machine the loop owns.
    ///
    /// The stamp is dropped rather than returned: every panel in this parcel is looking at the same
    /// machine it just handed in, in the same instant, so `{frame, mclk, running}` would be restating
    /// what the status strip already derives. A panel that ever caches an answer across a frame needs it
    /// back, and `Host::call` still returns it.
    pub fn call(&mut self, sys: &mut System, method: &str, params: &Value) -> Answer {
        let (result, _stamp) = self.host.call(sys, method, params);
        match result {
            Ok(v) => Answer::Ok(v),
            Err(e) => Answer::Err(e),
        }
    }

    /// The full D11 stamp beside the answer, for the one caller that needs to prove the call reached the
    /// real machine rather than the engine's inert placeholder.
    pub fn call_stamped(
        &mut self,
        sys: &mut System,
        method: &str,
        params: &Value,
    ) -> (Answer, Map<String, Value>) {
        let (result, stamp) = self.host.call(sys, method, params);
        (
            match result {
                Ok(v) => Answer::Ok(v),
                Err(e) => Answer::Err(e),
            },
            stamp,
        )
    }
}

/// The address a breakpoint sink halted the run on, or `None` if nothing fired.
///
/// A free function rather than a method because the sink borrows the [`Bus`] for the length of the run,
/// and the loop needs that borrow released before it can call [`Bus::record_break`] — **consuming the sink
/// here is what ends it**. Written as `brk.take()` inside `Machine::step` it would not compile, which is
/// the borrow checker enforcing the ordering this seam depends on.
pub fn break_observed(brk: Option<BreakStop<'_>>) -> Option<u32> {
    brk.and_then(|b| b.fired).map(|(_, addr)| addr)
}

// ---------------------------------------------------------------------------------------------------
// ⚑ The pause mirror is load-bearing, and this is what proves it
// ---------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_aether::rpc::code;
    use serde_json::json;

    fn booted() -> System {
        let mut sys = System::new(0x5EED);
        sys.load_rom(oracle_core::testrom::build());
        sys.reset();
        sys
    }

    /// A well-formed work-RAM poke — the gesture the Memory panel's `bus` write cell makes.
    fn poke() -> Value {
        json!({"addr": "0x00FF0000", "bytes": "0xAA"})
    }

    fn is_machine_running(a: &Answer) -> bool {
        matches!(a, Answer::Err(e) if e.code == code::INVALID_STATE)
            && a.reason() == Some("machineRunning")
    }

    /// **The control, and it must come first.** A bare `Host` — one that was never told what the
    /// player's loop is doing — accepts a work-RAM poke against a machine the loop is advancing at
    /// 60 Hz, and reports success.
    ///
    /// This is the wrong answer the mirror exists to prevent, and it is *silent*: the write really
    /// happens, the reply really says `ok`, and the panel would have shown a human that their poke was
    /// accepted because the tool said so. Without this assertion the test below would be checking that
    /// something we never established was broken has been fixed — and a mirror that turned out to be
    /// unnecessary would pass it just as green.
    #[test]
    fn an_unmirrored_host_wrongly_accepts_a_paused_only_write() {
        let mut sys = booted();
        let mut host = Host::new(HostConfig::default());
        let (result, _) = host.call(&mut sys, "emulator/write_memory", &poke());
        assert!(
            result.is_ok(),
            "the control has stopped holding: a Host that was never told the player is running now \
             refuses this write on its own, so `Engine::new`'s free_run default has moved and the \
             mirror below may no longer be what makes the refusal happen. Result: {result:?}"
        );
    }

    /// …and the mirror lands. Same machine, same gesture, one `Bus::new(.., paused: false)` between
    /// them, and the write is refused with the tool's own `-32005 machineRunning`.
    #[test]
    fn the_pause_mirror_makes_a_running_machine_refuse_a_paused_only_write() {
        let mut sys = booted();
        let mut bus = Bus::new(&mut sys, MachineInfo::default(), false);
        assert!(
            !bus.is_paused(),
            "an un-paused player IS a free-running bus (host.rs), so the bus must agree it is running"
        );
        let a = bus.call(&mut sys, "emulator/write_memory", &poke());
        assert!(
            is_machine_running(&a),
            "a work-RAM poke must be refused while the player's loop owns the clock, got {}",
            match &a {
                Answer::Ok(v) => format!("ok {v}"),
                Answer::Err(e) => format!("{} {}", e.code, e.message),
            }
        );
    }

    /// The mirror moves in both directions, and **`set_paused` alone is not what moves it**.
    ///
    /// The second half is the part worth having: `Host::set_paused` only queues into
    /// `pending_free_run`, and the queue is applied at the top of the next `Host::pump`. A mirror that
    /// called the setter and never drained would leave the engine believing whatever it believed
    /// before — and would look exactly like this test passing, because nothing else would have moved.
    #[test]
    fn pausing_the_player_opens_the_gate_and_un_pausing_closes_it_again() {
        let mut sys = booted();
        let mut bus = Bus::new(&mut sys, MachineInfo::default(), false);
        assert!(is_machine_running(&bus.call(
            &mut sys,
            "emulator/write_memory",
            &poke()
        )));

        bus.mirror_pause(&mut sys, true);
        assert!(bus.is_paused());
        let a = bus.call(&mut sys, "emulator/write_memory", &poke());
        assert!(
            matches!(a, Answer::Ok(_)),
            "a paused player must let the poke through — the mirror is not a one-way latch"
        );

        bus.mirror_pause(&mut sys, false);
        assert!(is_machine_running(&bus.call(
            &mut sys,
            "emulator/write_memory",
            &poke()
        )));
    }

    /// A repeated mirror of the value already landed is a no-op, which is what lets the transport bar
    /// call it every iteration in parcel 2c without a drain per frame. Checked by state rather than by
    /// counting calls: repeating `mirror_pause(true)` must not disturb a gate that is already open.
    ///
    /// **It starts from a RUNNING player and pauses into the loop, deliberately.** Written the obvious
    /// way — construct with `paused: true` and repeat — this test is nearly vacuous, because `paused` is
    /// where [`Engine::new`] already starts: the gate would be open with no mirror at all, and the
    /// assertions would pass against a `mirror_pause` that did nothing whatsoever. Coming from the
    /// running state means the open gate below is one the mirror had to *create*, so a repetition that
    /// undid it would be visible here.
    #[test]
    fn mirroring_an_unchanged_pause_state_changes_nothing() {
        let mut sys = booted();
        let mut bus = Bus::new(&mut sys, MachineInfo::default(), false);
        assert!(
            is_machine_running(&bus.call(&mut sys, "emulator/write_memory", &poke())),
            "the fixture must begin from a state where the gate is CLOSED, or nothing below is a fact \
             about the mirror"
        );
        for _ in 0..3 {
            bus.mirror_pause(&mut sys, true);
            assert!(bus.is_paused());
            assert!(matches!(
                bus.call(&mut sys, "emulator/write_memory", &poke()),
                Answer::Ok(_)
            ));
        }
    }

    /// `Host::call` swaps the caller's machine into the engine for the dispatch. If it ever stopped, the
    /// engine would answer off its inert placeholder — `frame 0, mclk 0` — and every panel would be
    /// reading a machine that has never run a single instruction while looking perfectly healthy.
    #[test]
    fn a_call_answers_for_the_machine_it_was_handed_and_not_the_placeholder() {
        let mut sys = booted();
        sys.run_frames(9);
        let mut bus = Bus::new(&mut sys, MachineInfo::default(), false);
        let (_, stamp) = bus.call_stamped(&mut sys, "emulator/status", &json!({}));
        assert_eq!(
            stamp["mclk"],
            json!(sys.scheduler().now()),
            "the call answered for the placeholder machine, not this one"
        );
        assert!(
            sys.scheduler().now() > 0,
            "the fixture ran, so a zero here would be two placeholders agreeing"
        );
    }
}

// ---------------------------------------------------------------------------------------------------
// ⚑ PARCEL 3 — the run-loop seam, driven through the player's own `Machine::step`
// ---------------------------------------------------------------------------------------------------

/// These tests run frames through [`crate::machine::Machine::step`] — the *real* per-frame path, sink
/// wiring and all — rather than rebuilding the `Fanout` here. A test that composed its own sink would pass
/// against a `Machine::step` that carried no bus at all, which is precisely the defect this parcel exists
/// to make impossible.
///
/// `oracle-aether` is `#![cfg(unix)]`, so this module is too.
#[cfg(all(test, unix))]
mod seam {
    use super::*;
    use crate::machine::Machine;
    use oracle_core::io::Pad;
    use serde_json::json;

    /// `move.w (A0),D0` in the fixture ROM's inner loop — the address `oracle-aether/tests/hosted.rs`
    /// uses for the same purpose, taken from there rather than re-derived.
    const HOT_PC: u32 = 0x0000_020E;

    /// Every test below is vacuous if this address stopped being hot, so it is **checked** rather than
    /// asserted in prose. Same check as `hosted.rs::assert_hot_pc_is_the_stirring_loop`.
    fn assert_hot_pc_is_the_stirring_loop() {
        let rom = oracle_core::testrom::build();
        let a = HOT_PC as usize;
        assert!(a + 1 < rom.len(), "HOT_PC is outside the fixture ROM");
        assert_eq!(
            u16::from_be_bytes([rom[a], rom[a + 1]]),
            0x3010,
            "0x{HOT_PC:08X} is no longer `move.w (A0),D0` — the fixture ROM moved and every breakpoint \
             test below is armed at a dead address"
        );
    }

    fn rig() -> (Machine, Bus) {
        let mut machine = Machine::new(oracle_core::testrom::build(), None);
        let bus = Bus::new(machine.system_mut(), MachineInfo::default(), false);
        (machine, bus)
    }

    fn ok(bus: &mut Bus, machine: &mut Machine, method: &str, params: Value) -> Value {
        match bus.call(machine.system_mut(), method, &params) {
            Answer::Ok(v) => v,
            Answer::Err(e) => panic!("{method} was refused: {} {}", e.code, e.message),
        }
    }

    /// **One iteration of `Loop::iterate`'s machine half**, in the order the loop runs it: run the frame
    /// (which latches any halt), then drain (which applies it), then read the bus's pause back.
    ///
    /// Written here rather than reaching into `main.rs` because `Loop` owns a `Governor`, a dock and an
    /// `egui::Context`; this is the part of it that is *this parcel's*, and keeping the order in one
    /// helper is what makes the order testable at all.
    fn iterate(machine: &mut Machine, bus: &mut Bus, paused: bool) -> bool {
        if !paused {
            machine.step([Pad::default(); 2], bus);
        }
        bus.mirror_pause(machine.system_mut(), paused);
        bus.is_paused()
    }

    /// ★ **HELD-PADS-PLAYER, half 1** — a client's `emulator/hold` reaches the pads this player writes,
    /// and the human's own pad reaches the bus.
    ///
    /// Design §5.6.2 booked both halves as *not done*: "a client's `hold` against the toolkit player does
    /// nothing whatsoever". This is the assertion that they are done, and it runs the client's side
    /// through [`Bus::call`] — the same in-process registry a socket client's request would land in
    /// (D15) — rather than reaching into the engine, so what is measured is the served method and not a
    /// field.
    ///
    /// **The alternative green paths, each ruled out by a named assertion:**
    ///
    /// 1. *`Machine::step` writes whatever it is handed, and always did.* Ruled out by the **control**
    ///    below: the same call with nothing held must leave the human's pad exactly as it was, and that
    ///    pad is non-empty, so the equality cannot be two `Pad::default()`s agreeing.
    /// 2. *The merge is a replace.* Caught by asserting the human's `right` **and** the client's `left`
    ///    are both down afterwards — a precedence rule in either direction drops one of them.
    /// 3. *Port 1 was left hardcoded to `Pad::default()`* (which it was before this parcel). Caught by
    ///    holding a button on port 1 and reading port 1 back.
    /// 4. *`set_live_pads` was never called, so the bus's own pad writes erase the human.* Caught
    ///    **without a second step**: `emulator/hold` ends in `Engine::apply_pads`, which writes
    ///    `merge_pads(live, held)` into the very `System` `Host::call` swapped in — so if the human's
    ///    `right` were not published, the pad immediately after the hold would carry `left` alone.
    /// 5. *The held set leaks and can never be cleared*, which would make the remedy the status-strip row
    ///    advertises a lie. Caught by the `release_all` clause at the end.
    #[test]
    fn a_clients_hold_reaches_the_players_pads_and_the_humans_pad_reaches_the_bus() {
        let (mut machine, mut bus) = rig();
        let human = [
            Pad {
                right: true,
                ..Pad::default()
            },
            Pad::default(),
        ];
        assert_ne!(
            human[0],
            Pad::default(),
            "the human's fixture pad must not be empty, or every equality below is two defaults agreeing"
        );

        // --- the control: nothing held, so the merge is the identity and the human drives alone ---
        machine.step(human, &mut bus);
        assert_eq!(
            machine.system().pad(0),
            human[0],
            "with nothing held the machine must see exactly the human's pad"
        );
        assert_eq!(machine.system().pad(1), human[1], "and port 1 likewise");

        // --- the client holds, through the served surface ---
        let reply = ok(
            &mut bus,
            &mut machine,
            "emulator/hold",
            json!({"port": 0, "buttons": ["left"]}),
        );
        assert_eq!(reply["held"], json!(["left"]), "the client's set, verbatim");

        // (4) — before any further step. `apply_pads` inside the handler already merged the human's pad,
        // which it can only have because `Machine::step` published it.
        let after_hold = machine.system().pad(0);
        assert!(
            after_hold.right,
            "the human's own button vanished the moment a client held one — `set_live_pads` is not being \
             called, and `emulator/hold` erased the person at the keyboard"
        );
        assert!(after_hold.left, "and the client's button landed");

        ok(
            &mut bus,
            &mut machine,
            "emulator/hold",
            json!({"port": 1, "buttons": ["a"]}),
        );

        // --- the loop's own write, which is the half that was missing ---
        machine.step(human, &mut bus);
        let p0 = machine.system().pad(0);
        assert!(
            p0.left,
            "a client's held button did not reach the pad the player writes — half 1 is not applied"
        );
        assert!(p0.right, "and it must not have replaced the human's own");
        assert!(
            machine.system().pad(1).a,
            "port 1's held set was dropped — `Machine::step` is still hardcoding `Pad::default()` there"
        );

        // --- (5) the remedy the status strip's row names must actually work ---
        ok(&mut bus, &mut machine, "emulator/release_all", json!({}));
        machine.step(human, &mut bus);
        assert_eq!(
            machine.system().pad(0),
            human[0],
            "`emulator/release_all` did not clear the held set, so the row that tells a human to call it \
             is advertising a remedy that does not work"
        );
        assert_eq!(machine.system().pad(1), human[1], "on both ports");
    }

    /// ★ **THE PARCEL** — a breakpoint armed over the bus halts the player's own loop, at the breakpoint.
    ///
    /// This is the whole seam end to end: `run_sinks` ▸ the frame ▸ `break_observed` ▸ `record_break` ▸
    /// `mirror_pause`'s drain ▸ `is_paused` ▸ the loop stops running frames. Before this parcel the player
    /// carried none of it, and a client that armed a breakpoint got `hits: 0` — a reply indistinguishable
    /// from "the ROM never reached that address", i.e. a statement about the program under test rather
    /// than about the emulator.
    ///
    /// **The alternative green paths, each ruled out by a named assertion:**
    ///
    /// 1. *The loop pauses for some reason of its own.* Ruled out by the **control**: the identical loop,
    ///    same frame count, with nothing armed, must NOT pause. Without it "it paused" is not evidence
    ///    that a breakpoint did it.
    /// 2. *The breakpoint sink was `Observe`-wrapped* (hits counted, run never ended). Caught by `pc`: an
    ///    `Observe` drops only `stop_requested`, so the halt still lands — after the frame has run to
    ///    completion — and the PC is then wherever the frame ended, not `HOT_PC`.
    /// 3. *`record_break` was never called* (the halt observed and dropped). Caught by the pause never
    ///    arriving at all, and distinguished from (1) by `hits`, which the sink counts either way.
    /// 4. *The pause is the bus's opinion and the loop ignored it.* Caught by the emulated clock standing
    ///    still across further iterations while the loop keeps turning.
    #[test]
    fn a_breakpoint_halts_the_players_own_loop_at_the_breakpoint() {
        assert_hot_pc_is_the_stirring_loop();
        const FRAMES: usize = 20;

        // --- (1) THE CONTROL, and it must come first. Nothing armed: the loop must not stop. ---
        {
            let (mut machine, mut bus) = rig();
            for _ in 0..FRAMES {
                assert!(
                    !iterate(&mut machine, &mut bus, false),
                    "the player paused itself with nothing armed, so a pause below would witness \
                     nothing about breakpoints"
                );
            }
            assert!(
                machine.system().scheduler().now() > 0,
                "the control ran no frames, so it established nothing"
            );
        }

        // --- The arrangement: a free-running player, stated as a fact. ---
        let (mut machine, mut bus) = rig();
        let refusal = bus.call(
            machine.system_mut(),
            "emulator/run_frames",
            &json!({"frames": 1}),
        );
        assert_eq!(
            refusal.reason(),
            Some("machineRunning"),
            "the player must really be free-running or this test proves nothing"
        );

        let bp = ok(
            &mut bus,
            &mut machine,
            "emulator/breakpoint_add",
            json!({"addr": format!("0x{HOT_PC:08X}")}),
        )["breakpoint"]
            .as_str()
            .expect("a breakpoint handle")
            .to_string();

        let mut paused = false;
        let mut ran = 0;
        while !paused && ran < FRAMES {
            paused = iterate(&mut machine, &mut bus, paused);
            ran += 1;
        }
        assert!(
            paused,
            "the player ran {FRAMES} frames past an armed breakpoint without stopping. Two defects \
             produce this identical result: the breakpoint sink never rode the run (`run_sinks` not \
             attached), or the halt was observed and dropped (`record_break` never called)."
        );

        // (2) AT the breakpoint, which is what separates a bare sink from an `Observe`-wrapped one.
        assert_eq!(
            machine.system().cpu_regs().pc,
            HOT_PC,
            "the machine stopped somewhere other than the breakpoint. An `Observe` around the \
             breakpoint sink produces exactly this: the hit is counted and the halt still lands, but \
             only after the frame has run to completion."
        );

        // (3) …and the hit was counted, on the handle that stopped it.
        let rows = ok(
            &mut bus,
            &mut machine,
            "emulator/breakpoint_list",
            json!({}),
        );
        let row = rows["breakpoints"]
            .as_array()
            .expect("breakpoints[]")
            .iter()
            .find(|b| b["breakpoint"] == json!(bp))
            .unwrap_or_else(|| panic!("no row for {bp} in {rows}"));
        assert_eq!(row["hits"], json!(1), "one halt is one hit: {row}");

        // (4) The loop really stopped: the clock stands still while the loop keeps turning.
        let halted_at = machine.system().scheduler().now();
        assert!(halted_at > 0, "the fixture never ran");
        for _ in 0..5 {
            paused = iterate(&mut machine, &mut bus, paused);
            assert!(paused, "the halt un-stuck itself");
        }
        assert_eq!(
            machine.system().scheduler().now(),
            halted_at,
            "the player kept emulating after a halt the bus told it about — a pause the loop does not \
             follow leaves the clock moving while the bus claims otherwise"
        );
    }

    /// ★ **The `Observe` wrappers: the watch sees the player's own frames, and does not stop them.**
    ///
    /// The asymmetry is the design. Both instruments are wrapped, so they observe everything and end
    /// nothing; the breakpoint sink is bare, so it ends the run. This checks both halves of the watch's
    /// side at once.
    ///
    /// **The alternative green path, ruled out by the control:** `seen()` counting something regardless of
    /// attachment. The same machine runs the same number of frames *around* `Machine::step` first — via
    /// `System::run_frames`, which carries no sink — and `seen()` must still be 0. Only then is a non-zero
    /// count after `step` evidence that the wrapper is attached to the player's run.
    #[test]
    fn an_armed_watch_sees_the_players_frames_through_observe_and_never_halts_them() {
        const FRAMES: usize = 4;
        let (mut machine, mut bus) = rig();

        // A write watch over the whole of work RAM: any ROM that runs at all writes here.
        ok(
            &mut bus,
            &mut machine,
            "emulator/watchpoint_add",
            json!({"addr": "0x00FF0000", "len": 65536, "write": true}),
        );

        // --- THE CONTROL: frames the seam did not carry must be invisible to the instrument. ---
        machine.system_mut().run_frames(FRAMES as u64);
        assert_eq!(
            bus.read_instruments().0.seen(),
            0,
            "the watch counted deliveries from a run it was never attached to, so a non-zero count \
             below would say nothing about `run_sinks`"
        );

        // --- The player's own frames, through the real path. ---
        for _ in 0..FRAMES {
            assert!(
                !iterate(&mut machine, &mut bus, false),
                "an `Observe`-wrapped watch must never end the run — that is the one thing the wrapper \
                 drops, and a halt here means the watch was attached BARE"
            );
        }
        let (watch, _, _) = bus.read_instruments();
        assert!(
            watch.seen() > 0,
            "the armed watch saw nothing across {FRAMES} of the player's own frames — the instrument \
             was not in the sink, and a client would be told `seen: 0` about frames that really happened"
        );
    }

    /// **Nothing armed lends no sinks**, which is what justifies `Machine::step` having no "is anything
    /// armed" branch: the unarmed case costs three `None`s whose sink impl wants nothing and does nothing.
    ///
    /// **The alternative green path, ruled out:** a `run_sinks` that always answers `None` would pass the
    /// first half and would silently disable every instrument. So the second half arms a watch and
    /// requires the first slot to become `Some`.
    #[test]
    fn run_sinks_lends_nothing_until_something_is_armed_and_lends_it_once_it_is() {
        let (mut machine, mut bus) = rig();
        let pc = machine.system().cpu_regs().pc;
        {
            let (w, p, b) = bus.run_sinks(pc);
            assert!(w.is_none(), "an unarmed watch must not be lent");
            assert!(p.is_none(), "an unarmed profiler must not be lent");
            assert!(
                b.is_none(),
                "with no breakpoints there is nothing to stop for"
            );
        }
        ok(
            &mut bus,
            &mut machine,
            "emulator/watchpoint_add",
            json!({"addr": "0x00FF0000", "len": 16, "write": true}),
        );
        let (w, _, _) = bus.run_sinks(pc);
        assert!(
            w.is_some(),
            "an armed watch was not lent to the run, so the `None`s above are unconditional and the \
             first half of this test is vacuous"
        );
    }

    /// **The drain is unconditional, and that is what lands the halt.**
    ///
    /// Parcel 2b's `mirror_pause` returned early when the pause had not changed. This pins the change
    /// directly at the seam it broke: a halt is latched while `paused` is `false`, and a pump called with
    /// that same unchanged `false` must still apply it.
    ///
    /// **The alternative green path, ruled out:** the halt landing because something *else* pumped. There
    /// is exactly one `Host::pump` call in this crate (`Bus::mirror_pause`), and this test makes only that
    /// one call — with an argument identical to the value already mirrored, which is precisely the case
    /// the old change-gate skipped.
    ///
    /// **A breakpoint must be armed at the latched address, and finding that out is worth recording**:
    /// `Engine::halt_on_breakpoint` answers `false` and changes nothing when no enabled breakpoint sits
    /// there ("which a client that cleared it between the observation and the apply can produce"). A
    /// version of this test that latched a bare address passed through the whole drain and left the
    /// machine running — red for the right reason, but not the reason it was written for.
    #[test]
    fn a_pump_with_an_unchanged_pause_still_applies_a_latched_halt() {
        let (mut machine, mut bus) = rig();
        assert!(!bus.is_paused(), "the fixture starts running");
        ok(
            &mut bus,
            &mut machine,
            "emulator/breakpoint_add",
            json!({"addr": format!("0x{HOT_PC:08X}")}),
        );

        bus.record_break(HOT_PC);
        assert!(
            !bus.is_paused(),
            "a latch alone must not move the run state — if it did, the drain below would be untested"
        );

        // The same value the bus was last told. Parcel 2b's gate returned here without pumping.
        bus.mirror_pause(machine.system_mut(), false);
        assert!(
            bus.is_paused(),
            "the latched halt was never applied. A change-gated drain produces exactly this: the bus \
             holds a halt it will apply only if the pause happens to move, which on the frame a \
             breakpoint fires it has not."
        );
    }
}
