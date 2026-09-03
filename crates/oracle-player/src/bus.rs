//! **The player owns a `Host`** — the Aether capability layer, in-process, and (opt-in) bound to a socket.
//!
//! Parcel 2a shipped `oracle-aether` as a *dev*-dependency: the parity test needed to see both sides and
//! nothing on the shipped path reached the bus. Parcel 2b promotes it, because the Memory panel's writes,
//! its symbol lookups and its `memory_hash` all go through [`Host::call`] — the in-process read of the
//! same method registry that contract D15 says an in-process GUI *is*. A panel that showed a refusal it
//! composed itself would be a panel guessing at the server it lives inside.
//!
//! # ⚑ The socket is for EXTERNAL clients only — this GUI is NOT a client of itself
//!
//! `PLAYER-SERVE` adds [`Host::serve`], so another lane's tool, a script or the MCP shim can attach to
//! *this* window. It changes **nothing** about how the window talks to itself, and that is a decision this
//! repo already made rather than an accident of the diff:
//!
//! * `empyrean:contract/protocol.md` **D15** — *"An in-process GUI is a consumer of the same registry, not
//!   a second server… it reads the method registry directly, in-process; it does not open a socket to
//!   itself."*
//! * So the panels keep reading the shared derivation directly ([`Bus::read_instruments`],
//!   [`Bus::read_breakpoints`], [`Bus::held_pads`]) and every gesture keeps going through [`Host::call`],
//!   which is synchronous and answers against the machine handed in.
//! * **Routing panels through the socket is the option that was considered and rejected.** It would put a
//!   `serde_json` round trip and a thread hop between a click and its answer, and it would have to land in
//!   [`Host::pump`] — the drain that runs *once per iteration* — so a click would cost a frame before it
//!   was even dispatched. A later reader tempted to "unify the two paths" is looking at the worse of the
//!   two, and this paragraph is why it was not taken.
//!
//! What serving *does* change is listed at [`Bus::publish`] and [`Bus::mirror_pause`]: `has_clients()` can
//! now be true, and a socket client can move the machine behind the loop's back.
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
//! ⚑ **§5.6.1's `bus-pump` measurement rested on a premise `PLAYER-SERVE` deletes.** It read 0.000 ms
//! median with the reasoning *"an unserved player queues nothing, so the drain is a `try_recv` on an empty
//! channel"*. A bound socket can queue. The measurement was retaken for that reason; §5.8.1 has it.
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
use oracle_aether::engine::FrameRef;
use oracle_aether::host::{Host, HostConfig, MachineInfo, PumpReport};
use oracle_aether::rpc::RpcError;
use oracle_core::bus::Observe;
use oracle_core::io::Pad;
use oracle_core::profiler::Profiler;
use oracle_core::scanline_capture::ScanlineCapture;
use oracle_core::symbols::SymbolTable;
use oracle_core::system::System;
use oracle_core::watchpoints::Watchpoints;
use serde_json::{Map, Value};
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

/// **What this launch has to say about the bus** — one of exactly three states, each with its own
/// sentence.
///
/// It is a stored value rather than a `println!` at the bind site, and that is the improvement over
/// `oracle-frontend`'s twin of this code. There, the three lines are printed inline and the file's own
/// comment records the consequence: *"no test over this file's output could have caught its removal — a
/// unit test cannot read `println!`"*, so the silent-arm defect had to be defended with a `match` whose
/// deletion is a compile error. That defence is kept here **and** the lines themselves are now readable by
/// a test, because they are [`ServeOutcome::sentence`]'s return value rather than a side effect.
///
/// The same sentence is what the status strip's `aether` row shows a human ([`crate::ui::StatusStrip`]),
/// so the launch line and the window cannot describe this window's bus differently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServeOutcome {
    /// Nobody asked. **This is still a statement and it is still printed** — see [`ServeOutcome::sentence`].
    NotAsked,
    /// Bound, accepting, and reachable at this path.
    Serving(PathBuf),
    /// Asked, and it did not happen. Carries the `io::Error`'s own text, never a paraphrase.
    Failed(String),
}

impl ServeOutcome {
    /// The one sentence this outcome is, without the `aether: ` prefix.
    ///
    /// ⚑ **The [`NotAsked`](ServeOutcome::NotAsked) arm is the load-bearing one, and it is why this is not
    /// an `Option<String>`.** Until 2026-08-29 `oracle-frontend`'s equivalent printed nothing at all when
    /// it was not serving, so a launch without the flag emitted no line about Aether anywhere — and **an
    /// absence is not a statement**. The measured cost was the owner launching twice in one evening and
    /// going to a window that could not be attached to. A silent unserved player is the defect, not the
    /// default, so this arm returns prose like the other two.
    ///
    /// It names all three switches because the reader of this line is, by construction, someone who wanted
    /// the bus and did not get it — a message that reports a state without naming the remedy sends them to
    /// the `--help` this line could have been.
    pub fn sentence(&self) -> String {
        match self {
            ServeOutcome::NotAsked => String::from(
                "not serving — no --aether given, so nothing can attach to this window \
                 (pass --aether, or --socket PATH, or set ORACLE_AETHER=1)",
            ),
            ServeOutcome::Serving(p) => format!(
                "serving on {} (mode 0600, {} methods, protocol version {})",
                p.display(),
                oracle_aether::engine::METHODS.len(),
                oracle_aether::rpc::PROTOCOL_VERSION
            ),
            ServeOutcome::Failed(e) => {
                format!("NOT serving — cannot bind the socket ({e})")
            }
        }
    }
}

/// **What the window says about its bus right now** — the startup outcome, plus whether anybody is
/// actually on the other end of it.
///
/// Two fields rather than one because they answer two different questions and a human staring at the
/// strip usually has the second: *a socket exists* is a fact about this launch, *somebody is attached* is
/// a fact about this second. The second is the one that explains a character walking left on its own,
/// beside the held-pads row that names the buttons.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AetherStatus {
    pub outcome: ServeOutcome,
    /// Live, now. Structurally `false` whenever `outcome` is not [`ServeOutcome::Serving`] — nothing can
    /// attach to a socket that does not exist — so [`AetherStatus::sentence`] mentions it only there,
    /// rather than printing "0 attached" under a bus that is off and inviting the reader to wonder
    /// whether it should be 1.
    pub attached: bool,
}

impl AetherStatus {
    /// The row's sentence: [`ServeOutcome::sentence`] plus, when there is a socket, who is on it.
    pub fn sentence(&self) -> String {
        let base = self.outcome.sentence();
        match (&self.outcome, self.attached) {
            (ServeOutcome::Serving(_), true) => format!("{base} — a client is attached"),
            (ServeOutcome::Serving(_), false) => format!("{base} — nothing attached yet"),
            _ => base,
        }
    }
}

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
    /// What [`Bus::new`] did about the socket. Stored so [`Bus::announcement`] and the status strip read
    /// one fact, and so a test can read it at all.
    outcome: ServeOutcome,
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
    ///
    /// # The socket (`PLAYER-SERVE`)
    ///
    /// `socket` carries `oracle-frontend`'s shape unchanged, because a second spelling of one decision is
    /// the drift R2 exists to prevent: **`None` binds nothing**, `Some(None)` binds the contract's own
    /// resolved default (`$ORACLE_SOCKET` → `$EXODUS_SOCKET` → `$XDG_RUNTIME_DIR/oracle.sock` →
    /// `/tmp/oracle.sock`, §7.1), and `Some(Some(p))` binds `p`.
    ///
    /// **The default is the well-known path deliberately, and a collision is loud rather than avoided.**
    /// Every lane's client resolver commits to `$XDG_RUNTIME_DIR/oracle.sock`, so a player that defaulted
    /// to a path of its own would be unreachable by exactly the tools this parcel exists to reach — the
    /// flag would bind a socket and still leave "nothing can attach to this window" true in practice.
    /// [`Server::bind`](oracle_aether::server::Server::bind) already separates the two collisions that
    /// matter: it **connects first**, refuses `AddrInUse` against a live server on the path (the owner's
    /// own `oracle-frontend` window, historically), and unlinks only a corpse that is actually a socket.
    /// So there is no silent fallback to a second path and no unlink that could remove a live server's
    /// entry; there is a refusal, and the refusal is printed.
    ///
    /// A bind failure degrades to inert and is **never fatal**: someone who launched a game to play it
    /// should not be stopped by a socket, and [`ServeOutcome::Failed`] says exactly what did not happen.
    ///
    /// The `match` on `socket` is deliberate, and `oracle-frontend`'s reason travels with it: written as
    /// `if let Some(path) = socket`, **deleting the silent case would be a silent regression** rather than
    /// a compile error. Here it is defended twice — by the `match`, and by [`ServeOutcome::sentence`]
    /// being a value a test can read.
    pub fn new(
        sys: &mut System,
        info: MachineInfo,
        paused: bool,
        socket: Option<Option<PathBuf>>,
    ) -> Self {
        let mut host = Host::new(HostConfig::default());
        host.set_machine_info(info);
        let outcome = match socket {
            Some(path) => match host.serve(path) {
                Ok(p) => ServeOutcome::Serving(p),
                Err(e) => ServeOutcome::Failed(e.to_string()),
            },
            None => ServeOutcome::NotAsked,
        };
        let mut bus = Bus { host, outcome };
        // The report is explicitly discarded, and this is the one call site where that is right: nothing
        // has been derived from this machine yet — no picture, no audio ring, no symbol cache older than
        // the `MachineInfo` handed in three lines up — so there is nothing here to put back in step.
        let _ = bus.mirror_pause(sys, paused);
        bus
    }

    /// The launch line, prefixed. Printed unconditionally by `main`, in **one** call site with no per-case
    /// arm to delete — which is the property the frontend's three inline prints do not have.
    pub fn announcement(&self) -> String {
        format!("aether: {}", self.outcome.sentence())
    }

    // # The three cross-check accessors below are `#[cfg(test)]`, and that is the design
    //
    // The **shipped** surface for "what about the bus?" is exactly two calls: [`Bus::announcement`] for
    // the launch line and [`Bus::aether_status`] for the window's row. These three exist so a test can
    // compare our stored [`ServeOutcome`] against the **`Host`'s own** view — the two are built three
    // lines apart and could disagree, and a test that read only our copy would be checking that a field
    // holds what we put in it.
    //
    // `#[cfg(test)]` states in the type system what a paragraph would only ask for: a future gate on any
    // of them does not compile until somebody deliberately removes the attribute. That matters here more
    // than usual — `oracle-frontend` gates two behaviours on its own `is_serving()`, and this crate
    // deliberately gates none, because [`ServeOutcome`] carries *why* (`NotAsked` and `Failed` are
    // different facts a `bool` collapses) and a second weaker spelling of one state is the drift R2
    // exists to prevent.

    /// What [`Bus::new`] did about the socket. Our stored copy.
    #[cfg(test)]
    pub fn serve_outcome(&self) -> &ServeOutcome {
        &self.outcome
    }

    /// The path the socket is actually bound to, or `None` when nothing is bound. The **`Host`'s**, so it
    /// is the *resolved* path and not the argument — `Some(None)` asks for a default whose value only
    /// §7.1's resolver knows.
    #[cfg(test)]
    pub fn socket_path(&self) -> Option<&Path> {
        self.host.socket_path()
    }

    /// Whether the bus is actually **bound and reachable by an external client**.
    ///
    /// It asks the `Host` — `accept.is_some()` — rather than re-deriving from the command line, so a
    /// launch that asked to serve and failed to bind answers `false`. That is the one case a flag-derived
    /// field gets wrong, and it is the case that matters.
    ///
    /// ⚑ **Read the next sentence before gating anything on this.** `is_serving()` is *not* "the bus is
    /// usable": [`Host::call`] is in-process and reachable with it false (D15), so every panel gesture,
    /// `emulator/hold`, the transport bar and the whole method registry work in an unserved player. This
    /// answers exactly one question — *can something outside this process attach* — and the only correct
    /// uses of it are ones where the answer to that question is the point.
    ///
    /// ⚑ **Nothing in this crate gates on it**, unlike `oracle-frontend`, which has two call sites for
    /// its own. See the block above [`serve_outcome`](Bus::serve_outcome) for why, and for why this is
    /// `#[cfg(test)]`.
    #[cfg(test)]
    pub fn is_serving(&self) -> bool {
        self.host.is_serving()
    }

    /// **What this window says about its bus right now**, for the status strip.
    ///
    /// One call rather than two accessors, because the row is one sentence and the two facts in it are
    /// read in the same instant.
    pub fn aether_status(&self) -> AetherStatus {
        AetherStatus {
            outcome: self.outcome.clone(),
            attached: self.has_clients(),
        }
    }

    /// Whether an external client is connected **right now**.
    ///
    /// ⚑ This is the predicate design §5.6.2 called one that *"an unserved player never satisfies"*. That
    /// sentence was true of every build before `PLAYER-SERVE` and is false of this one, which is why it is
    /// exposed here at all: [`Bus::publish`]'s cost and `emulator/screenshot`'s answer both turn on it,
    /// and a claim about a gate that can never open needs re-establishing the moment it can.
    ///
    /// Distinct from [`is_serving`](Bus::is_serving): serving is *a socket exists*, this is *somebody is
    /// on it*. A served player with nobody attached answers `false` here and `true` there.
    pub fn has_clients(&self) -> bool {
        self.host.has_clients()
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
    /// # The [`PumpReport`] is returned, not dropped — `PLAYER-SERVE`'s booked gap, closed
    ///
    /// `PLAYER-SERVE` dropped it and said why that had been sound: the report's three interesting flags all
    /// describe *a socket client* moving the machine behind the loop's back, and until that parcel this
    /// player bound no socket, so none of them could ever be true. Binding one made all three reachable and
    /// turned the drop into the defect. It is `PLAYER-PUMPREPORT` that acts on them, and the acting lives in
    /// [`drain`] rather than here — one function both the loop and its tests call, so a window that pumped
    /// and then ignored the answer is not a shape this crate can be written in.
    ///
    /// **`#[must_use]` states that in the type.** A caller that wants only the pause mirror — [`Bus::new`],
    /// which has no window, no picture and no audio to put back in step — says so with an explicit `let _`.
    #[must_use]
    pub fn mirror_pause(&mut self, sys: &mut System, paused: bool) -> PumpReport {
        self.host.set_paused(paused);
        self.host.pump(sys)
    }

    /// **The picture a client's own run drew**, line-major RGB and its width, or `None` when the bus is
    /// holding no whole frame.
    ///
    /// `PLAYER-SERVE` recorded that "this crate never reads [`Host::framebuffer`] at all" as the reason its
    /// window could not show a client-driven run. This is the read that makes it possible; [`drain`] is what
    /// decides when to take it, and [`Machine::adopt_frame`](crate::machine::Machine::adopt_frame) is what
    /// puts it on the glass.
    ///
    /// Unmasked, deliberately: this window applies no display-layer mask to its own picture either (there is
    /// no `blit_masked` in this crate), so masking here would make a client-driven frame the *only* one that
    /// honoured `emulator/set_layer_enabled` — one window, two rules for what it is showing.
    pub fn framebuffer(&self) -> Option<FrameRef<'_>> {
        self.host.framebuffer()
    }

    /// **The listing the engine resolves against now.**
    ///
    /// Read rather than remembered because `emulator/reload_rom` can *drop* the table — that is the D7
    /// binding check's whole point — and the copy this process handed to [`Bus::new`] would outlive the
    /// drop. See [`drain`].
    pub fn symbols(&self) -> Option<&SymbolTable> {
        self.host.symbols()
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
    /// # ⚑ `has_clients()` is no longer permanently false, and this is the call that notices
    ///
    /// [`Host::publish_capture`] is gated on `has_clients()` internally. Before `PLAYER-SERVE` that gate
    /// could never open — the player bound no socket, so `live` was 0 forever and this was one atomic load
    /// and a return. It was wired anyway "because the alternative is a seam that has never been exercised
    /// on the day something does connect"; **this is that day**.
    ///
    /// So the cost here is now *conditional on an attached client*, not zero: with one connected, every
    /// emulated frame that produced a picture copies it into the engine's latched frame, which is what
    /// makes `emulator/screenshot` and `emulator/state_hash {includeFramebuffer}` answer with what is on
    /// the glass rather than a post-hoc re-render of the VDP state — which, taken in V-Blank after the game
    /// has rewritten CRAM for the next frame, cannot show a single mid-frame palette effect. That is the
    /// point of publishing; it is priced in §5.8.1 rather than assumed free.
    ///
    /// **It changes nothing about the window's own picture.** The glass is
    /// [`Machine::image`](crate::machine::Machine::image); this crate never reads [`Host::framebuffer`].
    /// See §5.8.2 and [`Bus::mirror_pause`].
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
    /// **Not inert even when nothing is bound.** `Host::call` is in-process and reachable — the transport
    /// bar and every panel gesture already go through it (D15) — so `emulator/hold` can install a held set
    /// with `is_serving()` false. That is exactly why the hoisted merge has no `is_serving()` gate; see its
    /// doc.
    ///
    /// ⚑ `PLAYER-SERVE` does **not** weaken that argument, it strengthens it. The gate's absence was
    /// already load-bearing here (an in-process `hold` under a false `is_serving()`); now a *socket*
    /// client's `hold` reaches the same held set through the same engine, so the gate would drop two kinds
    /// of caller instead of one. Nothing about this method changed.
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

/// What [`drain`] put back in step, for a caller that wants to say so and for the tests that prove it.
///
/// Three booleans and not one, because they are three different repairs with three different triggers, and
/// a single "resynchronised" flag would let a test that meant to prove one of them pass on another.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Drained {
    /// Commands the drain answered. Reported, never acted on — see [`drain`]'s field-by-field note.
    pub calls: usize,
    /// The capture was dropped and the audio ring and its clock were rebuilt
    /// ([`Machine::resync_after_replacement`](crate::machine::Machine::resync_after_replacement)).
    pub timeline: bool,
    /// A frame the bus had drawn was taken onto the glass
    /// ([`Machine::adopt_frame`](crate::machine::Machine::adopt_frame)). `false` when `screen_changed` said
    /// the picture was *invalidated* rather than redrawn — there is nothing to present and the window keeps
    /// what it has.
    pub picture: bool,
    /// The symbol cache was re-derived from the engine's own listing.
    pub symbols: bool,
}

/// **The loop's one drain, and everything the drain's answer obliges the window to do.**
///
/// One function rather than a `pump` in the loop and a reaction beside it, because the reaction is the
/// parcel: `PLAYER-SERVE` left the window pumping and discarding, and the shape that made that possible was
/// a drain whose answer the caller could simply not mention. Here the caller cannot pump without this — the
/// only other `mirror_pause` in the crate is [`Bus::new`]'s, which has nothing derived to repair — and the
/// tests in this file drive *this* function, not a re-implementation of it beside it.
///
/// # What each [`PumpReport`] field makes this window do
///
/// * **`calls`** — nothing, and that is a decision. It counts commands answered; no state this window
///   derives from the machine is a function of how many commands went past. `oracle-frontend` ignores it
///   too. It is carried on [`Drained`] so a caller can say "the bus was busy", not so anything branches.
/// * **`deferred`** — nothing, for a stronger reason: it means the drain stopped on `pump_budget` with the
///   queue possibly non-empty, and the remainder is taken next iteration with nothing lost. There is no
///   repair to make. Reacting to it — a second drain, say — would trade the bound the budget exists to
///   enforce for a stall on the UI thread.
/// * **`mclk_before`/`mclk_after`**, via [`PumpReport::timeline_moved`] — the machine's clock moved under
///   the window, so the capture and the audio ring are holding a timeline that is gone:
///   [`Machine::resync_after_replacement`](crate::machine::Machine::resync_after_replacement).
///   **`frames_advanced()` is deliberately not added to any counter here**, which is where this window and
///   `oracle-frontend` part company; the reason is on that method.
/// * **`screen_changed`** — take the bus's frame ([`Bus::framebuffer`]). This is the field with no
///   equivalent at all before this parcel, and the one whose absence was visible: a client must pause this
///   player before it may run anything (§6's run-control state rule), and a paused player runs no frame of
///   its own, so *every* frame a client asks for was drawn where this window could not see it.
/// * **`rom_changed`** — re-derive the symbol cache, and resynchronise the timeline as above. See the note
///   below on why the second is not redundant even though it usually is.
///
/// # ⚑ `rom_changed` drives the timeline repair too, and `emulator/reset` is why
///
/// `PumpReport::rom_changed`'s own doc says to read it as *"resynchronise"*, and names `emulator/reset` as
/// the producer a caller is most likely to treat as harmless. Measured, on this build: a reset also moves
/// the clock (`System::reset` rebuilds the `System`, so `mclk` restarts near 0) and `timeline_moved()` is
/// therefore true for it as well, which makes `|| report.rom_changed` **redundant in every case this
/// crate can reach today**. It is written anyway, because the alternative is a window whose correctness
/// depends on a coincidence between two flags that are documented as separate facts — and the case where
/// they separate is not exotic: a reset issued at `mclk == 0`, or any future producer that replaces the
/// machine without moving its clock. The condition is what the doc asks for; the redundancy is measured
/// and recorded rather than assumed.
pub fn drain(
    machine: &mut crate::machine::Machine,
    bus: &mut Bus,
    symbols: &mut Option<SymbolTable>,
    paused: bool,
) -> Drained {
    let report = bus.mirror_pause(machine.system_mut(), paused);
    let mut out = Drained {
        calls: report.calls,
        ..Drained::default()
    };

    if report.timeline_moved() || report.rom_changed {
        machine.resync_after_replacement();
        out.timeline = true;
    }
    if report.screen_changed {
        // `None` is the documented invalidated-not-redrawn case (a restore, a ROM reload): nothing to
        // present, and the retained image stays up exactly as it does for an iteration that ran no frame.
        if let Some((width, rgb)) = bus.framebuffer() {
            out.picture = machine.adopt_frame(width, rgb);
        }
    }
    if report.rom_changed {
        // Unconditional re-derivation rather than a drop, because the engine's answer covers both outcomes:
        // `emulator/reload_rom` drops the listing when it no longer binds (D7) and keeps it when it does,
        // and `emulator/reset` keeps it always. Cloning a table is not free, but this runs only when a
        // cartridge was replaced, which is not a per-frame event.
        *symbols = bus.symbols().cloned();
        out.symbols = true;
    }
    out
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

    /// …and the mirror lands. Same machine, same gesture, one `Bus::new(.., paused: false, None)` between
    /// them, and the write is refused with the tool's own `-32005 machineRunning`.
    #[test]
    fn the_pause_mirror_makes_a_running_machine_refuse_a_paused_only_write() {
        let mut sys = booted();
        let mut bus = Bus::new(&mut sys, MachineInfo::default(), false, None);
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
        let mut bus = Bus::new(&mut sys, MachineInfo::default(), false, None);
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
        let mut bus = Bus::new(&mut sys, MachineInfo::default(), false, None);
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
        let mut bus = Bus::new(&mut sys, MachineInfo::default(), false, None);
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
        let bus = Bus::new(machine.system_mut(), MachineInfo::default(), false, None);
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

// ---------------------------------------------------------------------------------------------------
// ⚑ PLAYER-SERVE — the socket, and the three things that change once one exists
// ---------------------------------------------------------------------------------------------------

/// These bind **real** Unix sockets. Three constraints shape every fixture below and each one has
/// already cost somebody an hour:
///
/// * **Never the well-known path.** `Some(None)` resolves `$XDG_RUNTIME_DIR/oracle.sock`, which the
///   owner's live `oracle-frontend` window has historically held. A test that bound it would either
///   collide with his window or (worse, if his window were down) leave a socket on it. Every test here
///   names its own path.
/// * **Never the session scratchpad.** That directory's path is long enough to exceed `SUN_LEN`, and the
///   bind fails with *"path must be shorter than SUN_LEN"* — a refusal that looks exactly like the
///   bind-failure case one of these tests is trying to *produce* deliberately, so it would make that test
///   pass for the wrong reason. `/tmp/<short>` throughout.
/// * **Unique per test.** `cargo test` runs these on threads of one process, so two tests sharing a path
///   would race the live-server check.
///
/// `oracle-aether` is `#![cfg(unix)]`, so this module is too.
#[cfg(all(test, unix))]
mod serving {
    use super::*;
    use oracle_core::system::System;
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    fn booted() -> System {
        let mut sys = System::new(0x5EED);
        sys.load_rom(oracle_core::testrom::build());
        sys.reset();
        sys
    }

    /// A short, unique directory under `/tmp` — see the module doc for why not the scratchpad and why not
    /// `$XDG_RUNTIME_DIR`. Returned with the socket file name already joined.
    fn short_path(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("orp-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a short /tmp dir for the probe socket");
        dir.join("s")
    }

    fn cleanup(p: &Path) {
        if let Some(d) = p.parent() {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    /// **The control, and it comes first.** With `socket: None` the player is what it has always been:
    /// nothing bound, no path, and the outcome that says so in words.
    ///
    /// Without this assertion the served case below would be checking that something we never established
    /// was off has been turned on — and a `Bus::new` that bound unconditionally would pass it just as
    /// green.
    #[test]
    fn the_default_launch_binds_nothing_and_says_so() {
        let mut sys = booted();
        let bus = Bus::new(&mut sys, MachineInfo::default(), false, None);
        assert!(!bus.is_serving(), "no socket was requested");
        assert_eq!(bus.socket_path(), None, "and none was resolved");
        assert_eq!(bus.serve_outcome(), &ServeOutcome::NotAsked);
        assert!(
            !bus.has_clients(),
            "nothing is bound, so nothing can be attached"
        );
    }

    /// `is_serving()` answers for the **socket**, not for the flag — and the socket is real.
    ///
    /// ⚑ **The `connect` is the assertion that matters.** `is_serving()` is `accept.is_some()`, so a
    /// `serve` that bound, spawned an accept thread and then had it die instantly would still report
    /// `true`. Connecting is what distinguishes *a flag was honoured* from *a client can attach*, which is
    /// the entire subject of this parcel. `socket_path()` is checked against the requested path for the
    /// sibling failure: serving, but somewhere nobody is looking.
    #[test]
    fn a_requested_socket_is_bound_and_a_client_can_actually_connect() {
        let p = short_path("connect");
        let mut sys = booted();
        let bus = Bus::new(
            &mut sys,
            MachineInfo::default(),
            false,
            Some(Some(p.clone())),
        );
        assert!(
            bus.is_serving(),
            "a bound socket is serving, got {:?}",
            bus.serve_outcome()
        );
        assert_eq!(
            bus.socket_path(),
            Some(p.as_path()),
            "serving on a path nobody asked for is not serving"
        );
        assert_eq!(bus.serve_outcome(), &ServeOutcome::Serving(p.clone()));
        UnixStream::connect(&p).expect("an external client must be able to attach to this window");
        drop(bus);
        cleanup(&p);
    }

    /// **A bind failure is reported, and is never fatal.** The bus degrades to inert and every in-process
    /// caller keeps working, because someone who launched a game to play it should not be stopped by a
    /// socket.
    ///
    /// The failure is produced by giving the socket a **regular file as its parent directory**, so
    /// `create_dir_all` fails. That is a real `io::Error` from the real bind path rather than a stub.
    ///
    /// ⚑ The alternative green path ruled out: this test would pass identically against a `Bus::new` that
    /// ignored `socket` altogether and never even tried — so it asserts `Failed` (not `NotAsked`) and
    /// checks that the sentence carries the *error's own text*, which only a real attempt can produce.
    #[test]
    fn a_bind_failure_is_loud_specific_and_not_fatal() {
        let p = short_path("badparent");
        let blocker = p.parent().unwrap().join("f");
        std::fs::File::create(&blocker)
            .unwrap()
            .write_all(b"not a directory")
            .unwrap();
        let doomed = blocker.join("s");

        let mut sys = booted();
        let mut bus = Bus::new(&mut sys, MachineInfo::default(), false, Some(Some(doomed)));
        assert!(!bus.is_serving(), "the bind failed, so nothing is serving");
        assert_eq!(bus.socket_path(), None);
        let ServeOutcome::Failed(e) = bus.serve_outcome() else {
            panic!(
                "a failed bind must report Failed, not {:?} — an ignored `socket` argument would look \
                 exactly like NotAsked here",
                bus.serve_outcome()
            );
        };
        assert!(
            !e.is_empty(),
            "the io::Error's own text is carried, never a paraphrase"
        );
        let line = bus.announcement();
        assert!(
            line.contains("NOT serving") && line.contains(e),
            "the launch line must name the failure and quote the error: {line}"
        );
        // …and it is not fatal: the in-process registry still answers, which is the whole claim behind
        // "degraded to inert".
        assert!(matches!(
            bus.call(&mut sys, "emulator/status", &serde_json::json!({})),
            Answer::Ok(_)
        ));
        cleanup(&p);
    }

    /// **A live server on the path is REFUSED, and the live one is not disturbed.**
    ///
    /// This is the collision the default path makes reachable: every lane's client resolver commits to
    /// `$XDG_RUNTIME_DIR/oracle.sock` and the owner's own window has historically held it. The decision
    /// recorded at [`Bus::new`] is that the default stays the well-known path and the collision is loud,
    /// so this pins both halves of that: the second bind fails, **and the first is still serving and still
    /// connectable**. A "fix" that unlinked the incumbent's entry to make room would pass the first half
    /// and fail the second.
    #[test]
    fn a_live_server_on_the_path_is_refused_and_never_stolen() {
        let p = short_path("incumbent");
        let mut sys_a = booted();
        let a = Bus::new(
            &mut sys_a,
            MachineInfo::default(),
            false,
            Some(Some(p.clone())),
        );
        assert!(a.is_serving(), "the incumbent must be up first");

        let mut sys_b = booted();
        let b = Bus::new(
            &mut sys_b,
            MachineInfo::default(),
            false,
            Some(Some(p.clone())),
        );
        assert!(!b.is_serving(), "the second window must not bind");
        let ServeOutcome::Failed(e) = b.serve_outcome() else {
            panic!("a busy path must fail, got {:?}", b.serve_outcome());
        };
        assert!(
            e.contains("already live"),
            "the refusal must say a live server holds the path, not something generic: {e}"
        );

        assert!(a.is_serving(), "the incumbent is untouched");
        assert!(
            p.exists(),
            "the incumbent's filesystem entry survived the collision"
        );
        UnixStream::connect(&p).expect("the incumbent is still reachable after the refused bind");
        drop(b);
        drop(a);
        cleanup(&p);
    }

    /// **`has_clients()` can now go true, and §5.6.2's claim about it needs re-establishing.**
    ///
    /// The design says the picture-publish gate is one *"which an unserved player never satisfies"*. Three
    /// states are checked in one test because the interesting fact is the transition, and a test that only
    /// asserted the last one would pass against a `has_clients()` hardcoded to `true`.
    ///
    /// The accept loop polls at 5 ms ([`ACCEPT_POLL`](oracle_aether::server)), so the third state is
    /// waited for with a deadline rather than slept at — and the deadline **fails loudly** rather than
    /// falling through to a green `assert!(true)`.
    #[test]
    fn has_clients_is_false_unserved_false_unattached_and_true_once_someone_attaches() {
        let mut sys = booted();
        let unserved = Bus::new(&mut sys, MachineInfo::default(), false, None);
        assert!(
            !unserved.has_clients(),
            "an unserved player never satisfies the gate — this is §5.6.2's premise, checked"
        );
        drop(unserved);

        let p = short_path("clients");
        let mut sys = booted();
        let bus = Bus::new(
            &mut sys,
            MachineInfo::default(),
            false,
            Some(Some(p.clone())),
        );
        assert!(bus.is_serving());
        assert!(
            !bus.has_clients(),
            "serving with nobody attached is still no clients — the two predicates are different facts"
        );

        let client = UnixStream::connect(&p).expect("attach");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !bus.has_clients() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            bus.has_clients(),
            "a connected client must open the gate; it stayed shut for 5 s, so the publish path is \
             still dead and §5.6.2's bullet would still stand for the reason it gave"
        );
        drop(client);
        drop(bus);
        cleanup(&p);
    }

    /// **Every outcome says something out loud, and the quiet one names the remedy.**
    ///
    /// The `NotAsked` arm is the one this exists for: an absence is not a statement, and the frontend's
    /// version of this defect cost the owner two launches in one evening. `oracle-frontend`'s own comment
    /// records that no test there could cover it — *"a unit test cannot read `println!`"* — which is why
    /// the line is a returned `String` here.
    ///
    /// ⚑ The alternative green path: this would pass against a `sentence()` whose result nothing prints.
    /// That is covered structurally rather than by this assertion — `main` prints `announcement()`
    /// unconditionally, in one call site with no per-case arm to delete, and `Bus::new`'s `match` makes
    /// deleting the `None` arm a compile error. Both defences are named at those two sites.
    #[test]
    fn every_outcome_is_a_sentence_and_the_quiet_one_names_all_three_switches() {
        let quiet = ServeOutcome::NotAsked.sentence();
        for needle in ["not serving", "--aether", "--socket", "ORACLE_AETHER"] {
            assert!(
                quiet.contains(needle),
                "the not-serving line must name `{needle}` — the reader of it is by construction \
                 someone who wanted the bus and did not get it: {quiet}"
            );
        }
        let up = ServeOutcome::Serving(PathBuf::from("/tmp/x/s")).sentence();
        assert!(
            up.contains("/tmp/x/s") && up.contains("serving on"),
            "the serving line must name the path a client is supposed to dial: {up}"
        );
        let down = ServeOutcome::Failed("boom".into()).sentence();
        assert!(
            down.contains("NOT serving") && down.contains("boom"),
            "the failure line must be distinguishable from the quiet one at a glance: {down}"
        );
        // The three are pairwise different sentences. Collapsing any two would make the state this
        // parcel exists to reveal indistinguishable from a state it is not.
        assert_ne!(quiet, up);
        assert_ne!(quiet, down);
        assert_ne!(up, down);
    }
}
