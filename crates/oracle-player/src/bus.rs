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
//! * **No [`Host::pump`] in the frame loop.** Parcel 1's pacing numbers were taken against a loop with no
//!   pump and no `Observe` wrappers. Breakpoints, watchpoints and the profiler need both, they share that
//!   one run-loop change, and it re-opens that measurement — so they are parcel 2c's, together, and this
//!   parcel leaves `Loop::iterate` alone.
//! * **No `Observe` wrappers, no `run_sinks`, no `record_break`, no `publish_capture`.** Same reason.
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
//! **Where the drain goes: at setup, not in the loop** — [`Bus::new`] pumps exactly once, before the
//! first iteration, and [`Bus::mirror_pause`] pumps again only on an actual change of the player's pause
//! state. In this parcel there is no pause control at all (the transport bar is 2c), so the mirrored
//! value cannot change and no pump ever runs inside a frame. That is why parcel 1's pacing is untouched
//! by this parcel rather than "probably fine": the loop body is byte-identical, and the one drain happens
//! before the governor starts.
//!
//! When 2c adds the transport bar, [`mirror_pause`](Bus::mirror_pause) is the call that moves to the top
//! of `Loop::iterate` — and it lands in the same parcel as the `Observe` wrappers, which is the parcel
//! that owes the re-measurement anyway. Splitting it out now would pay that cost early and bank it twice.

use oracle_aether::host::{Host, HostConfig, MachineInfo};
use oracle_aether::rpc::RpcError;
use oracle_core::system::System;
use serde_json::{Map, Value};

/// The hosted capability layer plus the one piece of state the host owes it: what the bus was last told
/// about the player's pause.
pub struct Bus {
    host: Host,
    /// The pause state the **engine** has actually been told, as opposed to the one queued. `None` before
    /// the first mirror. Compared against rather than `Host::is_paused`, because `is_paused` already
    /// consults `pending_free_run` — it would report the value we *asked* for and let a queued change we
    /// never drained look landed.
    mirrored: Option<bool>,
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
        let mut bus = Bus {
            host,
            mirrored: None,
        };
        bus.mirror_pause(sys, paused);
        bus
    }

    /// Tell the bus what the player's loop is doing, and **make it land**.
    ///
    /// A no-op — one `bool` compare, no dispatch, no allocation — when nothing changed, which in this
    /// parcel is every call after the first. The drain is [`Host::pump`] rather than a new
    /// apply-the-pending entry point on purpose: `pump` is the *single* site where `pending_free_run` and
    /// `pending_break` are applied in that order, and `Host::call`'s doc spends a paragraph on why a
    /// second site reintroduces "a machine that stops on a breakpoint and silently resumes". Reusing the
    /// one site adds no interleaving the ordering argument does not already cover.
    pub fn mirror_pause(&mut self, sys: &mut System, paused: bool) {
        if self.mirrored == Some(paused) {
            return;
        }
        self.host.set_paused(paused);
        self.host.pump(sys);
        self.mirrored = Some(paused);
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
