//! **The breakpoint instrument** (`protocol.md` §6 *breakpoints & watchpoints*, §11.21 — CR-BP).
//!
//! An execution breakpoint is a PC the machine halts *before*, and this module is the whole of what the
//! five §6 rows keep: a set of handle-named entries, each with its own `enabled` and `hits`, plus the
//! [`BusEventSink`] that turns them into an actual halt.
//!
//! # Why a handle and not an address
//!
//! §11.21, verbatim: *"a stale handle resolves to nothing rather than to someone else's breakpoint. **One
//! address may carry several breakpoints**, each its own handle with its own `enabled` and `hits`."* The
//! amendment was raised on a measured harm — an agent cleared seven breakpoints it judged "not mine", one
//! of them at 1,691,410 hits — so [`BreakpointId`] is monotonic and **never reused**, exactly as
//! `WatchId` is, and the wire spelling (`b0`, `b1`, …) is not a number a client can compute on.
//!
//! # The re-trigger hazard, and why this sink latches a resume PC
//!
//! [`BusEventSink::on_step_boundary`]'s own documentation records the sharp edge:
//!
//! > *on the stopping iteration `on_step_boundary` is called for an instruction that does not run, and it
//! > is called again for that same PC when the caller resumes.*
//!
//! A breakpoint sink that fired on that repeat would halt at the same instruction forever: every resume
//! would break before executing anything, and the machine would never advance. That is not a hypothetical —
//! it is the **legacy server's defect 1**, and three tools in the `aeon` tree carry a hand-written
//! workaround for it (`emulator/step` before every resume, with a comment reading *"the sweep arm ran 24
//! iterations against ONE frozen tick"*). This sink suppresses a fire at the PC the run **started** on,
//! until at least one instruction has retired — GDB's rule, and the one that makes a
//! resume/wait/resume loop make progress without the caller having to know any of this.
//!
//! The cost is named rather than hidden: a breakpoint armed at the exact address the machine is *already*
//! sitting at does not fire on the next resume; it fires the next time execution *arrives* there. The
//! alternative is a machine that cannot be resumed, which is strictly worse and is the shape the workaround
//! above was written against.

use oracle_core::bus::{BusEventSink, StepRetire};

/// A breakpoint id. Monotonic within a server's life and **never reused** (D9 category 4), so a stale
/// handle resolves to nothing rather than to a breakpoint somebody else armed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BreakpointId(pub u32);

/// One armed breakpoint, in exactly the fields §6's `breakpoint_list` row reports.
#[derive(Clone, Debug)]
pub struct Breakpoint {
    pub id: BreakpointId,
    /// The address, **resolved at add time and fixed there**: §6 — *"a breakpoint does not move when
    /// symbols are reloaded."*
    pub addr: u32,
    /// Carried back verbatim and never interpreted. Empty means none was given.
    pub label: String,
    /// §11.21 design choice 2: written by `emulator/breakpoint_set_enabled` and by nothing else.
    pub enabled: bool,
    /// Firings **while enabled**, never reset by this surface — §6: *"a client wanting a fresh count
    /// clears and re-adds."*
    pub hits: u64,
}

/// The set of breakpoints a server holds.
///
/// Insertion-ordered, which is load-bearing twice: §6 pins the `stopped` event's `breakpoint` to *"the
/// earliest-added enabled breakpoint at that address"*, and `breakpoint_list`'s cursor is "resume at the
/// first id strictly greater than this", which needs ids to be non-decreasing down the vector.
#[derive(Debug, Default)]
pub struct Breakpoints {
    items: Vec<Breakpoint>,
    /// The next id to issue. Only ever increments — clearing every breakpoint does **not** rewind it,
    /// which is what "never reused" means.
    next_id: u32,
}

impl Breakpoints {
    pub fn new() -> Self {
        Self::default()
    }

    /// Arm one. Returns its handle. **Never an error and never idempotent**: §6 rules that a second add at
    /// an occupied address *"is not a duplicate error and not an idempotent echo — it is a second
    /// breakpoint"*. The cap is the caller's business, checked before this is reached.
    pub fn add(&mut self, addr: u32, enabled: bool, label: String) -> BreakpointId {
        let id = BreakpointId(self.next_id);
        self.next_id += 1;
        self.items.push(Breakpoint {
            id,
            addr,
            label,
            enabled,
            hits: 0,
        });
        id
    }

    /// How many are held, cap-checked against `limits.maxBreakpoints`.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Every breakpoint, in id order.
    pub fn iter(&self) -> impl Iterator<Item = &Breakpoint> {
        self.items.iter()
    }

    pub fn get(&self, id: BreakpointId) -> Option<&Breakpoint> {
        self.items.iter().find(|b| b.id == id)
    }

    pub fn get_mut(&mut self, id: BreakpointId) -> Option<&mut Breakpoint> {
        self.items.iter_mut().find(|b| b.id == id)
    }

    /// Remove one. `false` when the handle names nothing — which `breakpoint_clear` reports as
    /// `removed: 0` rather than as an error (§6.1's `checkpoint_drop` rule).
    pub fn remove(&mut self, id: BreakpointId) -> bool {
        let before = self.items.len();
        self.items.retain(|b| b.id != id);
        self.items.len() != before
    }

    /// `breakpoint_clear {all:true}` — **every breakpoint on the server, other clients' included**. Returns
    /// how many went. `next_id` is deliberately untouched.
    pub fn clear(&mut self) -> usize {
        let n = self.items.len();
        self.items.clear();
        n
    }

    /// Whether any breakpoint is armed. The arming condition for the sink: an unarmed instrument attached
    /// anyway would cost every run a per-step scan for nothing.
    pub fn any_enabled(&self) -> bool {
        self.items.iter().any(|b| b.enabled)
    }

    /// Whether this id was ever issued, live or retired. Used to tell a **retired** handle (a legitimate
    /// thing to hold) from one this server could never have spelled.
    pub fn was_issued(&self, id: BreakpointId) -> bool {
        id.0 < self.next_id
    }

    /// The **earliest-added enabled** breakpoint at `addr`, or `None` if nothing armed is there. §6 pins
    /// that ordering: the `stopped` event names *"one handle — the earliest-added enabled breakpoint at that
    /// address"*. Read-only, deliberately — see [`record_halt`](Breakpoints::record_halt).
    pub fn first_enabled_at(&self, addr: u32) -> Option<BreakpointId> {
        self.items
            .iter()
            .find(|b| b.enabled && b.addr == addr)
            .map(|b| b.id)
    }

    /// **Count the firing.** Every enabled breakpoint at `addr` increments its `hits` — §6: *"every enabled
    /// breakpoint at that address increments its `hits`"* — and the earliest is returned.
    ///
    /// **Split from [`first_enabled_at`](Breakpoints::first_enabled_at) on purpose, and the split is the
    /// definition of `hits`.** The sink can only observe that a boundary landed on an armed address; it
    /// cannot know whether the breakpoint is what the run *reports* as its cause, because a `step` that
    /// retires onto a breakpoint address, or a `run_to` whose target carries one, halts for the caller's
    /// own reason. Counting at observation time would make `hits` mean "stops that happened to land here",
    /// which inflates under exactly the `step`-then-resume idiom every consumer of this surface uses. So the
    /// engine calls this only once precedence has settled and the breakpoint really is the reported cause,
    /// and `hits` means *"times this breakpoint halted the machine"* — reproducible, and the number a client
    /// counting fires is actually asking for.
    pub fn record_halt(&mut self, addr: u32) -> Option<BreakpointId> {
        let mut first = None;
        for b in self
            .items
            .iter_mut()
            .filter(|b| b.enabled && b.addr == addr)
        {
            b.hits += 1;
            if first.is_none() {
                first = Some(b.id);
            }
        }
        first
    }
}

/// The [`BusEventSink`] that turns armed breakpoints into a halt, for the length of **one** run.
///
/// It raises its flag from [`on_step_boundary`](BusEventSink::on_step_boundary), which is what gives it
/// classic breakpoint semantics: the machine stops *before* the instruction at the breakpoint address
/// executes, with `pc` pointing at it. (A sink that raised the flag from `on_event` would stop after the
/// triggering instruction had committed — that is what a `stopAfter` watch does, and it is a different
/// instrument.)
/// It borrows the set **shared**, not mutably: counting the hit is the engine's, after precedence between
/// the run's three possible stop causes has settled. See [`Breakpoints::record_halt`].
pub struct BreakStop<'a> {
    set: &'a Breakpoints,
    /// The PC the run started on. See the module note: a fire here, before anything has retired, is the
    /// re-trigger that would make the machine unresumable.
    resume_pc: u32,
    retired_any: bool,
    /// The handle that halted this run and the address it sits at, latched once — so the repeat boundary
    /// the run loop delivers for the stopping instruction cannot be seen twice.
    pub fired: Option<(BreakpointId, u32)>,
}

impl<'a> BreakStop<'a> {
    pub fn new(set: &'a Breakpoints, resume_pc: u32) -> Self {
        Self {
            set,
            resume_pc,
            retired_any: false,
            fired: None,
        }
    }
}

impl BusEventSink for BreakStop<'_> {
    /// **A breakpoint is an execution condition, not an access one**, so the bus event stream is nothing to
    /// this sink. Required by the trait rather than defaulted; ignoring it here is the whole statement.
    fn on_event(&mut self, _event: oracle_core::bus::BusEvent) {}

    fn on_step_boundary(&mut self, pc: u32, _frame: u64) {
        // Latched, not accumulated. `on_step_boundary` is called again for the same PC on the stopping
        // iteration and once more when the caller resumes; `BusEventSink`'s own doc requires a counting
        // sink to account for that, and this is how.
        if self.fired.is_some() {
            return;
        }
        // The re-trigger suppression, and it is deliberately narrow: only the run's *starting* PC, and only
        // until something retires. One instruction of progress lifts it.
        if !self.retired_any && pc == self.resume_pc {
            return;
        }
        self.fired = self.set.first_enabled_at(pc).map(|id| (id, pc));
    }

    fn on_step_retire(&mut self, _retire: StepRetire) {
        self.retired_any = true;
    }

    fn stop_requested(&self) -> bool {
        self.fired.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A retirement whose only load-bearing property here is that it happened: this sink reads no field of
    /// it. Spelled out rather than defaulted because `StepRetire` deliberately derives no `Default` — every
    /// field of it is a machine fact and a zero would be a claim.
    fn a_retirement() -> StepRetire {
        StepRetire {
            pc: 0,
            opcode: 0x4E71,
            executed: true,
            sp: 0,
            ssp: 0,
            supervisor: true,
            cycles: 4,
            stall_cycles: 0,
            idle: false,
        }
    }

    #[test]
    fn one_address_carries_several_breakpoints_each_with_its_own_state() {
        // §6: "a second breakpoint_add at an address that already has one is not a duplicate error and not
        // an idempotent echo — it is a second breakpoint".
        let mut set = Breakpoints::new();
        let a = set.add(0x1234, true, "first".into());
        let b = set.add(0x1234, true, "second".into());
        assert_ne!(a, b, "the two adds must not collapse onto one handle");
        assert_eq!(set.len(), 2);
        set.get_mut(b).expect("live").enabled = false;
        assert!(set.get(a).expect("live").enabled, "`enabled` is per-handle");
    }

    #[test]
    fn a_fire_bumps_every_enabled_breakpoint_at_the_address_and_names_the_earliest() {
        // §6: "every enabled breakpoint at that address increments its hits", while the event names one —
        // "the earliest-added enabled breakpoint at that address".
        let mut set = Breakpoints::new();
        let first = set.add(0x1000, true, String::new());
        let second = set.add(0x1000, true, String::new());
        let disabled = set.add(0x1000, false, String::new());
        let elsewhere = set.add(0x2000, true, String::new());

        assert_eq!(set.record_halt(0x1000), Some(first));
        assert_eq!(set.get(first).expect("live").hits, 1);
        assert_eq!(set.get(second).expect("live").hits, 1);
        assert_eq!(
            set.get(disabled).expect("live").hits,
            0,
            "a disabled breakpoint does not halt and does not count"
        );
        assert_eq!(set.get(elsewhere).expect("live").hits, 0);

        // Retiring the earliest moves the name to the next one, not to the disabled one.
        set.remove(first);
        assert_eq!(set.record_halt(0x1000), Some(second));
    }

    #[test]
    fn handles_are_never_reused_even_after_clear_all() {
        let mut set = Breakpoints::new();
        let a = set.add(0x10, true, String::new());
        assert_eq!(set.clear(), 1);
        let b = set.add(0x10, true, String::new());
        assert_ne!(a, b, "clear must not rewind the id counter");
        assert!(
            set.was_issued(a) && set.get(a).is_none(),
            "a retired handle stays ISSUED and resolves to nothing — the property that makes \
             `removed: 0` a complete answer"
        );
    }

    #[test]
    fn the_sink_does_not_re_break_on_the_pc_it_resumed_from() {
        // The legacy server's defect 1, refused here. Without the suppression a resume/wait loop returns
        // instantly at the same instruction and the machine never advances.
        let mut set = Breakpoints::new();
        set.add(0x400, true, String::new());
        let mut sink = BreakStop::new(&set, 0x400);
        sink.on_step_boundary(0x400, 0);
        assert!(
            !sink.stop_requested(),
            "the run's own starting PC must not halt it before anything has executed"
        );
        // …but the moment something retires, that address is live again.
        sink.on_step_retire(a_retirement());
        sink.on_step_boundary(0x400, 0);
        assert!(
            sink.stop_requested(),
            "one instruction of progress lifts the suppression"
        );
        assert_eq!(sink.fired, Some((BreakpointId(0), 0x400)));
    }

    #[test]
    fn the_sink_latches_so_the_repeated_boundary_cannot_double_count() {
        // `BusEventSink::on_step_boundary`: "on the stopping iteration on_step_boundary is called for an
        // instruction that does not run, and it is called again for that same PC when the caller resumes."
        let mut set = Breakpoints::new();
        let id = set.add(0x800, true, String::new());
        let mut sink = BreakStop::new(&set, 0x100);
        sink.on_step_boundary(0x800, 0);
        sink.on_step_boundary(0x800, 0);
        sink.on_step_boundary(0x800, 0);
        assert_eq!(sink.fired, Some((id, 0x800)));
        assert_eq!(
            set.get(id).expect("live").hits,
            0,
            "the sink OBSERVES; only the engine counts, once precedence has settled"
        );
    }

    #[test]
    fn a_disabled_breakpoint_does_not_halt() {
        let mut set = Breakpoints::new();
        set.add(0x900, false, String::new());
        assert!(!set.any_enabled());
        let mut sink = BreakStop::new(&set, 0x100);
        sink.on_step_boundary(0x900, 0);
        assert!(!sink.stop_requested());
    }
}
