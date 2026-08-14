//! Bus-level recording watchpoints — the "who wrote this?" root-causing primitive
//! (`docs/2026-07-20-diagnostic-tooling-ideas.md`), and the project's **trace recorder**
//! (`docs/2026-08-14-trace-recorder-design.md`).
//!
//! A [`Watchpoints`] is a pure **consumer** of the bus event stream ([`crate::bus::BusEventSink`]): register
//! one or more address ranges to watch, pass it as the sink to a sink-generic run
//! (`System::run_frames_with_sink` / `run_until_with_sink`), and read back a log of every access that hit a
//! watched range — each hit attributed to the instruction that drove it (its PC) and to the master that drove
//! it (CPU vs DMA/other, via the event's function code) plus the value, size, op, frame, and master clock.
//!
//! It observes only: it never touches CPU or memory state, is stored by the *caller* (never by `System`), and
//! so sits in neither frozen currency and can never move a state hash. The null-sink hot path is untouched —
//! attaching a `Watchpoints` is what makes the machine observable, detaching it makes it a black box again.
//! No other file in `src/` changed to gain any of the recorder features below — the whole facility is one
//! sink implementation — so the no-instrumentation path is unchanged **textually**, not by optimiser faith.
//!
//! ## Attribution (how a hit learns its PC and its clock)
//!
//! A [`crate::bus::BusEvent`] is emitted deep inside a CPU access and carries no PC. The sink-generic run loop
//! instead calls [`BusEventSink::on_step_boundary`] once immediately before each CPU step, stamping the PC of
//! the instruction about to execute (and the current frame); `Watchpoints` latches that context, so every
//! event that follows — until the next boundary — is attributed to that one instruction. An instruction that
//! drives several accesses (a `MOVEM`, a read-modify-write) attributes them all to its own PC, which is
//! exactly right.
//!
//! The **master clock** enters through the other seam: the real 68000/Z80 bus adapters deliver every access
//! through [`BusEventSink::on_event_at`], which carries the absolute mclk of the access. `Watchpoints`
//! overrides it to latch the timestamp and then delegates, so a hit's [`WatchHit::mclk`] is the access's own
//! clock. Two honest caveats, both reported by [`Watchpoints::caveats`] rather than hidden:
//!
//! - A **VDP-internal** hit ([`WatchSpace::Vram`]/`Cram`/`Vsram`) is drained *after* the driving CPU step, so
//!   it is stamped with the latest timestamped bus access of that step, not with the write's own time. Treat
//!   it as step-granular (`F-TRACE-VDPWRITE-MCLK`).
//! - Events fed to [`BusEventSink::on_event`] directly (the phase-0 synthetic `SystemBus`, hand-written unit
//!   tests) carry no clock at all, so `mclk` holds the last latched value — `0` if none was ever supplied.
//!
//! ## Spaces: bus (v1) + VDP-internal (v2)
//!
//! A watch lives in a [`WatchSpace`]. v1 [`add_watch`](Watchpoints::add_watch) watches the **68000 bus**
//! address space (work RAM, ROM, Z80 RAM, I/O, VDP *ports* — anywhere a `BusEvent` is emitted). v2
//! [`add_vdp_watch`](Watchpoints::add_vdp_watch) watches a **VDP-internal** byte-address space —
//! `Vram`/`Cram`/`Vsram` — the "who wrote this tile / palette entry?" case. A VDP-internal write happens
//! inside `vdp.rs` (after a data-port write decodes + autoincrements, and during DMA fills/copies), off the
//! bus stream, so it is delivered through a separate sink method: registering any VDP watch makes
//! [`wants_vdp_writes`](BusEventSink::wants_vdp_writes) true, which arms the VDP's write-capture buffer for the
//! run; each captured write arrives via [`on_vdp_write`](BusEventSink::on_vdp_write) and is attributed to the
//! same step-boundary PC as bus hits. A hit reports the resolved region address, old→new value, region,
//! driving PC, and [`WatchVia`] (Direct CPU write vs DMA step). Spaces never cross: a numeric address
//! collision between the bus space and a VDP space does not cross-trigger.
//!
//! This also **resolves v1's DMA-attribution gap**: a DMA writes VDP memory with `fc = 0` and never reaches
//! the bus event stream, so v1 could not attribute it; v2 captures it at the VDP write itself with
//! `via = Dma`, attributed to the instruction that triggered the transfer.
//!
//! ## Read modes: record, count, census
//!
//! At ~8,700 CPU steps per frame a raw per-access log *"would swamp the signal"*
//! (`docs/plans/2026-07-16-m68000-macro-rtc.md:48`), which is why every hunt in the corpus collapses its trace
//! to counters. **Aggregation is the primary read mode here; the event log is the fallback.** Each watch picks
//! a [`WatchMode`]:
//!
//! - [`WatchMode::Record`] — the default and v1's behaviour: store the hit in the bounded ring.
//! - [`WatchMode::Count`] — store nothing, count. This is what turns "watch all of `$400000-$7FFFFF`" from a
//!   context bomb into a number.
//! - [`WatchMode::Census`] — a bounded group-by over one key ([`CensusKey`]). The two root causes this
//!   primitive retracted were both settled by the *set of distinct keys*, not by a total
//!   (`docs/2026-07-25-testrom-conformance.md:783-789`, `:806-811`), so the distinct count is reported as its
//!   own number and the key cap is never silent — see [`WatchReport::keys_capped`].
//!
//! Every mode records `matched`, `first`, and `last` stamps. `add_watch(range, op, label)` in `Record` mode
//! over the whole address space is legal, bounded by the ring, and almost always the wrong instrument: reach
//! for `Count` first, then narrow.
//!
//! ## The negative control is structural
//!
//! [`Watchpoints::seen`] counts **every** delivery offered to the sink, matched or not. A report of
//! `seen = 4_182_339, matched = 0` is self-evidently a live instrument that found nothing; `seen = 0` is
//! self-evidently a dead one that was never attached. There is no flag to remember and none to forget.
//!
//! Still **deferred**: break-on-hit / execution halt at *instruction* granularity (the core runs
//! frame-batched; [`Watch::stop_after`] gives the coarse form — end the run at the next instruction boundary
//! once a watch has fired N times), and any frontend / MCP wiring. Recording is bounded (drop-oldest ring with
//! a drop count), never a halt.

use crate::bus::{BusEvent, BusEventSink, BusOp, Size};
use crate::vdp::{VdpTarget, VdpVia, VdpWrite};
use std::collections::BTreeMap;
use std::ops::RangeInclusive;

/// Default per-watch census key cap (see [`Watch::key_cap`]).
///
/// **Not 16.** The `k4_openbus_probe` rule that inspired the cap uses 16, which is right for that probe and
/// wrong as a default: the census episodes that mattered ranged over **390–516 distinct PCs**
/// (`docs/2026-07-22-tf4-nextlayer-triage.md:138-139`) and ~1400 distinct colours
/// (`docs/2026-07-25-testrom-conformance.md:765`). A census silently capped at 16 would have reported "16
/// distinct" and been confidently wrong — the exact failure class this instrument exists to prevent.
pub const DEFAULT_CENSUS_KEY_CAP: usize = 256;

/// Which bus operations a watch matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchOp {
    /// Match reads only.
    Read,
    /// Match writes only. The 68000 TAS is an indivisible read-modify-write whose *write* is the point of a
    /// "who modified this?" watch, so a `Write` watch also matches [`BusOp::Tas`] (the RMW write cycle);
    /// a `Read` watch does not (TAS is fundamentally a store).
    Write,
    /// Match any access (read, write, or TAS).
    Any,
}

impl WatchOp {
    /// Whether this filter matches a bus operation.
    fn matches(self, op: BusOp) -> bool {
        match self {
            WatchOp::Any => true,
            WatchOp::Read => op == BusOp::Read,
            WatchOp::Write => op == BusOp::Write || op == BusOp::Tas,
        }
    }
}

/// An address-**parity** filter: which half of the 68000 data bus an access sits on. `Even` is the UDS
/// (high-byte) half, `Odd` the LDS (low-byte) half — the distinction four `K4Probe` counters draw by hand
/// (`examples/k4_openbus_probe.rs`, the `$A10000-$A1001F` and `$C00004-$C00007` read arms), because open-bus
/// and I/O behaviour differs between them. Optional and defaulting to "don't care", exactly like the `fc`
/// filter (`F-TRACE-SIZEFILTER`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddrParity {
    /// Even addresses (`addr & 1 == 0`) — the UDS half.
    Even,
    /// Odd addresses (`addr & 1 == 1`) — the LDS half.
    Odd,
}

impl AddrParity {
    /// Whether this filter matches an access address.
    fn matches(self, addr: u32) -> bool {
        match self {
            AddrParity::Even => addr & 1 == 0,
            AddrParity::Odd => addr & 1 == 1,
        }
    }
}

/// Which address space a watch (and a hit) lives in. `Bus` is the v1 68000 bus address space; `Vram`/`Cram`/
/// `Vsram` are the v2 VDP-internal byte-address spaces (the "who wrote this tile" case). A watch only ever
/// matches accesses in its own space — a numeric address collision across spaces never cross-triggers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchSpace {
    Bus,
    Vram,
    Cram,
    Vsram,
}

/// How a recorded write reached its target. `Bus` is a v1 68000 bus access (the master is in [`WatchHit::fc`]:
/// 5/6 = CPU, 0 = a non-CPU master). `Direct`/`Dma` are v2 VDP-internal writes: `Direct` is a CPU data-port
/// write, `Dma` is a DMA step — attributed to the instruction that *triggered* the transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchVia {
    Bus,
    Direct,
    Dma,
}

/// Handle for a registered watch, returned by [`Watchpoints::add`] and carried on every hit it produced
/// ([`WatchHit::watch`]) so an agent running several concurrent watches can tell which one fired.
///
/// Ids are never reused: [`Watchpoints::clear`] and [`Watchpoints::remove`] retire an id permanently, so a
/// stale handle resolves to nothing rather than silently to a different watch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WatchId(pub u32);

/// What a watch does with an access it matches. See the module docs for why aggregation is the primary read
/// mode rather than a convenience over the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchMode {
    /// Store the hit in the bounded ring (v1 behaviour, and the default).
    Record,
    /// Count only — store nothing.
    ///
    /// **Where counting is the wrong instrument:** a large count over a polling idiom proves nothing about
    /// consumption (*"`st` is huge everywhere (vblank-poll idioms; TF4 677k) — counting cannot prove
    /// flag-consumption either way"*, `docs/2026-08-02-k4-0-hit-table.md:82-84`). A count answers "how often",
    /// never "why".
    Count,
    /// Bounded group-by over one key: `key -> count`, plus the distinct-key cardinality. Stores no hits.
    Census(CensusKey),
}

/// The key a [`WatchMode::Census`] groups by. Deliberately an enum of extractors rather than a closure: a
/// `Box<dyn FnMut>` would make `Watchpoints` non-`Debug` and un-serializable, foreclosing a future JSON-RPC
/// exposure. The cost, stated honestly: a census cannot express a *stateful* classification (the k4 probe's
/// arbiter-latch shadows), and those stay hand-rolled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CensusKey {
    /// The full access address — the "which destination indices does this ROM actually touch?" query.
    Addr,
    /// The access address shifted right by `n` bits — one bucket per page/region (`AddrPage(8)` = 256-byte
    /// pages). Shifts ≥ 32 saturate to 31.
    AddrPage(u8),
    /// The 68000 function code. **On a VDP-internal watch this is always 0** (there is no bus function code);
    /// and on the PSG port it cannot attribute a master at all — see [`Watchpoints::caveats`].
    Fc,
    /// The bus operation (`0` = Read, `1` = Write, `2` = TAS).
    Op,
    /// The access width in bytes (1 / 2 / 4).
    Size,
    /// The full value read or written.
    Value,
    /// The byte-duplication test on the low 16 bits of the value: `1` when the high byte equals the low byte,
    /// `0` otherwise. This is the recurring open-bus word shape (a word access whose halves are the same byte).
    ValueHiEqLo,
}

impl CensusKey {
    /// Extract this key from a hit.
    fn key_of(self, hit: &WatchHit) -> u64 {
        match self {
            CensusKey::Addr => hit.addr as u64,
            CensusKey::AddrPage(shift) => (hit.addr >> u32::from(shift).min(31)) as u64,
            CensusKey::Fc => hit.fc as u64,
            CensusKey::Op => match hit.op {
                BusOp::Read => 0,
                BusOp::Write => 1,
                BusOp::Tas => 2,
            },
            CensusKey::Size => hit.size.bytes() as u64,
            CensusKey::Value => hit.value as u64,
            CensusKey::ValueHiEqLo => u64::from((hit.value >> 8) & 0xFF == hit.value & 0xFF),
        }
    }

    /// Render one census key for a human report. The raw `u64` is always available in
    /// [`WatchReport::census`]; this only names it.
    pub fn describe(self, key: u64) -> String {
        match self {
            CensusKey::Addr => format!("${key:06X}"),
            CensusKey::AddrPage(shift) => format!("${key:X}<<{shift}"),
            CensusKey::Fc => format!("fc={key}"),
            CensusKey::Op => match key {
                0 => "Read".to_string(),
                1 => "Write".to_string(),
                _ => "Tas".to_string(),
            },
            CensusKey::Size => format!("{key}B"),
            CensusKey::Value => format!("${key:X}"),
            CensusKey::ValueHiEqLo => if key == 0 { "hi!=lo" } else { "hi==lo" }.to_string(),
        }
    }
}

/// A deterministic emulated coordinate — the same shape and semantics as
/// [`crate::system::StopRecord`](crate::system::StopRecord), so a stop and a hit name the same point in the
/// run. Never wall-clock: two runs of the same ROM, input and power-on seed produce identical stamps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    /// The PC of the instruction that drove the access.
    pub pc: u32,
    /// Emulated frame index.
    pub frame: u64,
    /// Absolute emulated master clock (step-granular for VDP-internal hits — see the module docs).
    pub mclk: u64,
    /// The monotonic id of the matched access ([`WatchHit::seq`]).
    pub seq: u64,
}

/// A watch to register — the builder behind [`Watchpoints::add`].
///
/// [`Watchpoints::add_watch`] / [`add_vdp_watch`](Watchpoints::add_vdp_watch) are the two-line shorthands for
/// the common `Record`-mode case and keep their original signatures; this is how the optional knobs (fc
/// filter, mode, key cap, stop-after) are reached.
#[derive(Clone, Debug)]
pub struct Watch {
    space: WatchSpace,
    lo: u32,
    hi: u32,
    op: WatchOp,
    fc: Option<u8>,
    size: Option<Size>,
    addr_parity: Option<AddrParity>,
    mode: WatchMode,
    key_cap: usize,
    stop_after: Option<u64>,
    label: String,
}

impl Watch {
    /// A watch on the **68000 bus** address space.
    pub fn bus(range: RangeInclusive<u32>, op: WatchOp, label: impl Into<String>) -> Self {
        Self::in_space(WatchSpace::Bus, range, op, label)
    }

    /// A watch on a **VDP-internal** byte-address space (`Vram`/`Cram`/`Vsram`). Passing [`WatchSpace::Bus`]
    /// here is a misuse (use [`Watch::bus`]); it is treated as a never-matching VDP watch.
    pub fn vdp(
        space: WatchSpace,
        range: RangeInclusive<u32>,
        op: WatchOp,
        label: impl Into<String>,
    ) -> Self {
        Self::in_space(space, range, op, label)
    }

    fn in_space(
        space: WatchSpace,
        range: RangeInclusive<u32>,
        op: WatchOp,
        label: impl Into<String>,
    ) -> Self {
        Self {
            space,
            lo: *range.start(),
            hi: *range.end(),
            op,
            fc: None,
            size: None,
            addr_parity: None,
            mode: WatchMode::Record,
            key_cap: DEFAULT_CENSUS_KEY_CAP,
            stop_after: None,
            label: label.into(),
        }
    }

    /// Also require this 68000 function code (5/6 = CPU data/program, 0 = a non-CPU master). Record-time, like
    /// every other filter — a non-matching access is never stored.
    pub fn fc(mut self, fc: u8) -> Self {
        self.fc = Some(fc);
        self
    }

    /// Also require this access width (`F-TRACE-SIZEFILTER`). Record-time, like every other filter.
    ///
    /// On a VDP-internal watch the hit's size is the region's ([`Size::Byte`] for a VRAM byte,
    /// [`Size::Word`] for a CRAM/VSRAM entry), not a bus width.
    pub fn size(mut self, size: Size) -> Self {
        self.size = Some(size);
        self
    }

    /// Also require this address parity — the UDS (even) / LDS (odd) half (`F-TRACE-SIZEFILTER`).
    /// Record-time, like every other filter.
    pub fn addr_parity(mut self, parity: AddrParity) -> Self {
        self.addr_parity = Some(parity);
        self
    }

    /// Set the read mode (default [`WatchMode::Record`]).
    pub fn mode(mut self, mode: WatchMode) -> Self {
        self.mode = mode;
        self
    }

    /// Cap the number of distinct census keys retained (default [`DEFAULT_CENSUS_KEY_CAP`]). Past the cap,
    /// known keys keep counting and new ones are counted in [`WatchReport::census_overflow`] — never dropped
    /// silently. Ignored outside [`WatchMode::Census`].
    pub fn key_cap(mut self, cap: usize) -> Self {
        self.key_cap = cap;
        self
    }

    /// Ask the run to stop once this watch has matched `n` accesses.
    ///
    /// This is the payoff composition with the sink stop signal: a watch with `stop_after` set makes
    /// [`BusEventSink::stop_requested`] true, so `System::run_frames_with_sink` ends at the next instruction
    /// boundary with `StopReason::SinkRequested` — "run until X happens", instead of a hand-tuned frame budget.
    /// The triggering instruction has fully committed when the run stops (the flag is raised mid-step and
    /// honoured at the next boundary).
    pub fn stop_after(mut self, n: u64) -> Self {
        self.stop_after = Some(n);
        self
    }
}

/// One registered watch: the builder's configuration plus this watch's own aggregation state.
struct WatchSpec {
    id: WatchId,
    space: WatchSpace,
    lo: u32,
    hi: u32,
    op: WatchOp,
    fc: Option<u8>,
    size: Option<Size>,
    addr_parity: Option<AddrParity>,
    mode: WatchMode,
    key_cap: usize,
    stop_after: Option<u64>,
    label: String,
    // --- aggregation (every mode) ---
    matched: u64,
    first: Option<Stamp>,
    last: Option<Stamp>,
    // --- aggregation (Census only) ---
    census: BTreeMap<u64, u64>,
    census_overflow: u64,
    keys_capped: bool,
}

impl WatchSpec {
    /// Whether this watch matches an access. Every clause is a record-time filter: a non-match stores nothing.
    ///
    /// This is a **conjunction of optional filters**, deliberately — not a predicate language. A counter that
    /// needs a *disjunction* (`K4Probe`'s `status_upper_reads` = Word **or** even address) is not expressible
    /// as one watch; sum disjoint watches instead. See `docs/2026-08-14-trace-recorder-design.md`.
    fn matches(&self, hit: &WatchHit) -> bool {
        self.space == hit.space
            && self.op.matches(hit.op)
            && (self.lo..=self.hi).contains(&hit.addr)
            && self.fc.is_none_or(|f| f == hit.fc)
            && self.size.is_none_or(|s| s == hit.size)
            && self.addr_parity.is_none_or(|p| p.matches(hit.addr))
    }

    /// Fold one matched access into this watch's aggregates.
    fn observe(&mut self, hit: &WatchHit) {
        self.matched += 1;
        let stamp = Stamp {
            pc: hit.pc,
            frame: hit.frame,
            mclk: hit.mclk,
            seq: hit.seq,
        };
        if self.first.is_none() {
            self.first = Some(stamp);
        }
        self.last = Some(stamp);
        if let WatchMode::Census(k) = self.mode {
            let key = k.key_of(hit);
            let at_cap = self.census.len() >= self.key_cap;
            match self.census.get_mut(&key) {
                // A known key keeps counting past the cap (the k4 rule) — only *new* keys are refused.
                Some(n) => *n += 1,
                None if at_cap => {
                    self.census_overflow += 1;
                    self.keys_capped = true;
                }
                None => {
                    self.census.insert(key, 1);
                }
            }
        }
    }

    /// Whether this watch has reached its stop-after threshold.
    fn wants_stop(&self) -> bool {
        self.stop_after.is_some_and(|n| self.matched >= n)
    }
}

/// A recorded access that hit a watch. `pc` is the instruction that drove it (from the step-boundary stamp);
/// `fc` is the 68000 function code of the access (5 = supervisor data, 6 = supervisor program; a non-CPU
/// master such as DMA reports 0), so a hit attributes to both *which instruction* and *which master* touched
/// the address. `seq` is a monotonic id assigned to every matched access in order — stable across ring-buffer
/// drops, so a gap in `seq` marks dropped hits.
///
/// The type stays `Copy` and `Eq` on purpose: two traces of the same ROM must be diffable with stock tooling
/// (`Vec<WatchHit>` vs `Vec<WatchHit>`), which is how the sharpest comparisons in the corpus were made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WatchHit {
    /// Which registered watch recorded this hit. When several watches match one access it is recorded **once**,
    /// attributed to the lowest-id matching watch in [`WatchMode::Record`] mode (every matching watch still
    /// updates its own aggregates).
    pub watch: WatchId,
    /// The address space this hit lives in (v1 bus hits are [`WatchSpace::Bus`]).
    pub space: WatchSpace,
    pub addr: u32,
    /// The value that was there **before** the access. Meaningful for VDP-internal writes (the pre-write
    /// value, so a hit reads old→new); `0` for bus accesses (the bus event stream carries no prior value).
    pub old: u32,
    /// The value read or written (for a VDP write, the new value).
    pub value: u32,
    /// The access width: [`Size::Byte`]/[`Size::Word`]/[`Size::Long`] for a bus access; a VDP write reports
    /// [`Size::Byte`] (a VRAM byte) or [`Size::Word`] (a CRAM/VSRAM entry).
    pub size: Size,
    /// The bus op for a bus access; always [`BusOp::Write`] for a VDP-internal write (a store).
    pub op: BusOp,
    /// The 68000 function code of a bus access (5/6 = CPU, 0 = non-CPU master); `0` for a VDP-internal write
    /// (there is no bus function code — the CPU-vs-DMA distinction is in [`WatchHit::via`]).
    pub fc: u8,
    /// How the write reached the target — the CPU-vs-DMA attribution for VDP writes ([`WatchVia::Bus`] for a
    /// v1 bus access).
    pub via: WatchVia,
    pub pc: u32,
    pub frame: u64,
    /// Absolute emulated master clock of the access, from [`BusEventSink::on_event_at`]. **Step-granular for
    /// VDP-internal hits**, and `0` when the emitter supplied no clock at all — see the module docs and
    /// [`Watchpoints::caveats`].
    pub mclk: u64,
    pub seq: u64,
}

/// A read-time snapshot of one watch: its configuration and everything it aggregated. Owned (rather than
/// borrowed) because this is the shape a report / transport wants, and it is built once per read, never per
/// access.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchReport {
    pub id: WatchId,
    pub label: String,
    pub space: WatchSpace,
    pub range: RangeInclusive<u32>,
    pub op: WatchOp,
    /// The function-code filter, if any.
    pub fc: Option<u8>,
    /// The access-width filter, if any (`F-TRACE-SIZEFILTER`).
    pub size: Option<Size>,
    /// The address-parity filter, if any (`F-TRACE-SIZEFILTER`).
    pub addr_parity: Option<AddrParity>,
    pub mode: WatchMode,
    /// How many accesses this watch matched — counted in every mode, including the modes that store nothing.
    pub matched: u64,
    /// The first and last matched access, stamped. `None` when the watch never fired.
    pub first: Option<Stamp>,
    pub last: Option<Stamp>,
    /// `key -> count`, ascending by key. `None` outside [`WatchMode::Census`].
    pub census: Option<Vec<(u64, u64)>>,
    /// How many distinct keys the census retained. **A lower bound when [`keys_capped`](Self::keys_capped) is
    /// true** — never read `distinct_keys == key_cap` as an exact answer.
    pub distinct_keys: u64,
    /// The configured key cap, so a reader can tell `distinct_keys` at the cap from `distinct_keys` under it.
    pub key_cap: usize,
    /// Whether the census refused at least one new key because it was at its cap.
    pub keys_capped: bool,
    /// How many accesses carried a key the capped census could not retain. Counted, never silently dropped.
    pub census_overflow: u64,
}

/// A recording watchpoint facility — a [`BusEventSink`] the caller owns and passes to a sink-generic run.
pub struct Watchpoints {
    specs: Vec<WatchSpec>,
    next_id: u32,
    /// The hit ring. `head` is the index of the oldest live hit; drops advance it and the buffer is compacted
    /// once `head` reaches `cap`, so `hits()` stays one contiguous slice and drop-oldest is amortized O(1)
    /// (the previous `remove(0)` was O(n) per drop, which a wide watch makes quadratic).
    buf: Vec<WatchHit>,
    head: usize,
    cap: usize,
    dropped: u64,
    seq: u64,
    seen: u64,
    /// How many VDP-internal accesses have matched — drives the step-granular-mclk caveat.
    vdp_matched: u64,
    cur_pc: u32,
    cur_frame: u64,
    cur_mclk: u64,
}

impl Watchpoints {
    /// A facility whose hit log holds at most `cap` entries (drop-oldest past that, counted by
    /// [`dropped`](Self::dropped)). `cap` should be ≥ 1; `cap = 0` records nothing (every hit is counted as a
    /// drop) — which is a legitimate configuration for a pure [`WatchMode::Count`]/[`WatchMode::Census`] run.
    pub fn new(cap: usize) -> Self {
        Self {
            specs: Vec::new(),
            next_id: 0,
            buf: Vec::new(),
            head: 0,
            cap,
            dropped: 0,
            seq: 0,
            seen: 0,
            vdp_matched: 0,
            cur_pc: 0,
            cur_frame: 0,
            cur_mclk: 0,
        }
    }

    /// Register a configured [`Watch`], returning its [`WatchId`].
    pub fn add(&mut self, w: Watch) -> WatchId {
        let id = WatchId(self.next_id);
        self.next_id += 1;
        self.specs.push(WatchSpec {
            id,
            space: w.space,
            lo: w.lo,
            hi: w.hi,
            op: w.op,
            fc: w.fc,
            size: w.size,
            addr_parity: w.addr_parity,
            mode: w.mode,
            key_cap: w.key_cap,
            stop_after: w.stop_after,
            label: w.label,
            matched: 0,
            first: None,
            last: None,
            census: BTreeMap::new(),
            census_overflow: 0,
            keys_capped: false,
        });
        id
    }

    /// Register an inclusive **68000 bus** address range to watch for `op` accesses, tagged with `label` (v1).
    /// Shorthand for `add(Watch::bus(range, op, label))` — the `Record`-mode default.
    pub fn add_watch(
        &mut self,
        range: RangeInclusive<u32>,
        op: WatchOp,
        label: impl Into<String>,
    ) -> WatchId {
        self.add(Watch::bus(range, op, label))
    }

    /// Register an inclusive **VDP-internal** byte-address range in `space` (`Vram`/`Cram`/`Vsram`) to watch
    /// for `op` writes, tagged with `label` (v2 — the "who wrote this tile?" watch). Registering any VDP watch
    /// makes [`wants_vdp_writes`](Self::wants_vdp_writes) true, which arms the VDP write capture for the run.
    /// Passing [`WatchSpace::Bus`] here is a misuse (use [`add_watch`](Self::add_watch)); it is treated as a
    /// never-matching VDP watch.
    pub fn add_vdp_watch(
        &mut self,
        space: WatchSpace,
        range: RangeInclusive<u32>,
        op: WatchOp,
        label: impl Into<String>,
    ) -> WatchId {
        self.add(Watch::vdp(space, range, op, label))
    }

    /// Record `hit` into the bounded drop-oldest ring (shared by the bus and VDP paths). `seq` is already
    /// assigned; every matched access is counted in `seq` whether or not it is stored.
    fn record(&mut self, hit: WatchHit) {
        if self.buf.len() - self.head >= self.cap {
            // At capacity: drop the oldest to bound memory (or, for cap == 0, drop this hit outright).
            self.dropped += 1;
            if self.head == self.buf.len() {
                return;
            }
            self.head += 1;
            if self.head >= self.cap {
                self.buf.drain(..self.head);
                self.head = 0;
            }
        }
        self.buf.push(hit);
    }

    /// Offer one matched access to every registered watch: assign its `seq`, fold it into the aggregates of
    /// *each* matching watch, and store it at most once — attributed to the lowest-id matching watch in
    /// [`WatchMode::Record`] mode. (Watches that overlap therefore each get an accurate count, while the log
    /// never doubles an access.)
    fn dispatch(&mut self, mut hit: WatchHit) {
        hit.seq = self.seq;
        self.seq += 1;
        if hit.space != WatchSpace::Bus {
            self.vdp_matched += 1;
        }
        let mut store_as: Option<WatchId> = None;
        for spec in &mut self.specs {
            if !spec.matches(&hit) {
                continue;
            }
            spec.observe(&hit);
            if spec.mode == WatchMode::Record && store_as.is_none() {
                store_as = Some(spec.id);
            }
        }
        if let Some(id) = store_as {
            hit.watch = id;
            self.record(hit);
        }
    }

    /// Whether any registered watch matches this access (the cheap record-time reject: nothing is built and
    /// nothing is stored for a non-match).
    fn any_match(&self, hit: &WatchHit) -> bool {
        self.specs.iter().any(|s| s.matches(hit))
    }

    /// Remove all registered watches (the inverse of [`add_watch`](Self::add_watch)), discarding their
    /// aggregates. Recorded hits are left intact — drain them with [`take_hits`](Self::take_hits). Ids are not
    /// reused, so hits recorded before the clear keep naming watches that no longer exist.
    pub fn clear(&mut self) {
        self.specs.clear();
    }

    /// Remove one watch by id, discarding its aggregates. Returns whether it was registered.
    pub fn remove(&mut self, id: WatchId) -> bool {
        let before = self.specs.len();
        self.specs.retain(|s| s.id != id);
        self.specs.len() != before
    }

    /// A read-time snapshot of every registered watch, in registration order.
    pub fn watches(&self) -> Vec<WatchReport> {
        self.specs.iter().map(report_of).collect()
    }

    /// A read-time snapshot of one watch, or `None` if the id is not registered.
    pub fn watch(&self, id: WatchId) -> Option<WatchReport> {
        self.specs.iter().find(|s| s.id == id).map(report_of)
    }

    /// The label a watch was registered with, or `None` if the id is not registered. A [`WatchHit`] stores the
    /// id rather than the label (a hit stays `Copy`, and nothing allocates on the instrumented path); this is
    /// how a reader resolves it.
    pub fn label_of(&self, id: WatchId) -> Option<&str> {
        self.specs
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.label.as_str())
    }

    /// The recorded hits, oldest first.
    pub fn hits(&self) -> &[WatchHit] {
        &self.buf[self.head..]
    }

    /// Drain and return the recorded hits, oldest first, leaving the log empty.
    pub fn take_hits(&mut self) -> Vec<WatchHit> {
        let mut buf = std::mem::take(&mut self.buf);
        let out = buf.split_off(self.head);
        self.head = 0;
        out
    }

    /// How many hits were dropped (oldest-first) because the log was at capacity. **Distinct from a truncated
    /// read**: this is loss at *record* time.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// **The structural negative control.** Every delivery offered to this sink — bus event or VDP-internal
    /// write, matched or not. `seen > 0, matched == 0` is a live instrument that found nothing; `seen == 0` is
    /// an instrument that was never attached, and a zero from it means nothing at all.
    pub fn seen(&self) -> u64 {
        self.seen
    }

    /// How many accesses matched at least one watch (across all modes). Equals the next `seq` to be assigned,
    /// so `matched - hits().len() as u64 - dropped()` is what the non-`Record` watches absorbed.
    pub fn matched(&self) -> u64 {
        self.seq
    }

    /// **The timing basis every `frame` stamp in this trace is expressed in** (`F-TRACE-PAL`).
    ///
    /// A `frame` index is meaningless without the frame length that produced it, and a trace outlives the
    /// session that took it: once a report says "frame 601" and nothing says what a frame *was*, the
    /// ambiguity is permanent and lands in someone else's cached data. So the basis is a field of the
    /// report, machine-readable (label **and** numbers), not a sentence in a doc a script cannot branch on.
    ///
    /// Constant today because [`crate::system::System`] is NTSC-only, and derived from the scheduler's own
    /// [`MCLK_PER_FRAME`](crate::system::MCLK_PER_FRAME) so it cannot disagree with the stamps. When region
    /// becomes machine state the recorder will be handed the machine's
    /// [`System::timing_basis`](crate::system::System::timing_basis) — **this accessor's signature does not
    /// change, so no consumer breaks**; that is the whole reason for stamping it while it is free.
    pub fn timing_basis(&self) -> crate::system::TimingBasis {
        crate::system::TimingBasis::NTSC
    }

    /// Caveats that must travel **with** the numbers, not in documentation next to them — agents (and humans)
    /// over-trust precise-looking figures. Empty when nothing applies.
    pub fn caveats(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.seen == 0 {
            out.push(
                "seen = 0: no access was ever offered to this sink, so every count below is the \
                 instrument being absent, not a negative finding."
                    .to_string(),
            );
        }
        if self.vdp_matched > 0 {
            out.push(format!(
                "{} VDP-internal hit(s): `mclk` is the driving CPU step's clock, not the write's own \
                 (VDP writes are drained after the step). Treat it as step-granular \
                 (F-TRACE-VDPWRITE-MCLK).",
                self.vdp_matched
            ));
        }
        for s in &self.specs {
            if s.mode == WatchMode::Census(CensusKey::Fc)
                && s.space == WatchSpace::Bus
                && (overlaps(s, 0x7F11) || overlaps(s, 0xC0_0011))
            {
                out.push(format!(
                    "watch #{} '{}': an fc census over the PSG port cannot attribute a master — a 68000 \
                     write through the Z80 window is re-emitted Z80-shaped (addr $7F11, fc 0), so both \
                     master signals read as Z80 there (F-TRACE-MASTER).",
                    s.id.0, s.label
                ));
            }
            if s.keys_capped {
                out.push(format!(
                    "watch #{} '{}': census hit its {}-key cap — distinct_keys is a LOWER BOUND and {} \
                     further access(es) carried keys it could not retain.",
                    s.id.0, s.label, s.key_cap, s.census_overflow
                ));
            }
        }
        out
    }
}

/// Whether a watch's range contains `addr`.
fn overlaps(s: &WatchSpec, addr: u32) -> bool {
    (s.lo..=s.hi).contains(&addr)
}

/// Snapshot one watch for a report.
fn report_of(s: &WatchSpec) -> WatchReport {
    WatchReport {
        id: s.id,
        label: s.label.clone(),
        space: s.space,
        range: s.lo..=s.hi,
        op: s.op,
        fc: s.fc,
        size: s.size,
        addr_parity: s.addr_parity,
        mode: s.mode,
        matched: s.matched,
        first: s.first,
        last: s.last,
        census: match s.mode {
            WatchMode::Census(_) => Some(s.census.iter().map(|(k, v)| (*k, *v)).collect()),
            _ => None,
        },
        distinct_keys: s.census.len() as u64,
        key_cap: s.key_cap,
        keys_capped: s.keys_capped,
        census_overflow: s.census_overflow,
    }
}

impl BusEventSink for Watchpoints {
    fn on_event(&mut self, event: BusEvent) {
        self.seen += 1;
        let hit = WatchHit {
            watch: WatchId(0), // assigned by `dispatch`
            space: WatchSpace::Bus,
            addr: event.addr,
            old: 0,
            value: event.value,
            size: event.size,
            op: event.op,
            fc: event.fc,
            via: WatchVia::Bus,
            pc: self.cur_pc,
            frame: self.cur_frame,
            mclk: self.cur_mclk,
            seq: 0, // assigned by `dispatch`
        };
        if !self.any_match(&hit) {
            return;
        }
        self.dispatch(hit);
    }

    fn on_event_at(&mut self, event: BusEvent, mclk: u64) {
        // Latch the access's own clock, then take the ordinary path — the one place the mclk enters, done
        // once here instead of hand-rolled per consumer.
        self.cur_mclk = mclk;
        self.on_event(event);
    }

    fn on_step_boundary(&mut self, pc: u32, frame: u64) {
        self.cur_pc = pc;
        self.cur_frame = frame;
    }

    fn wants_vdp_writes(&self) -> bool {
        // Arm the (currency-sensitive) VDP capture only when at least one VDP-space watch is registered.
        self.specs.iter().any(|s| s.space != WatchSpace::Bus)
    }

    fn on_vdp_write(&mut self, w: VdpWrite) {
        self.seen += 1;
        // A VDP-internal write is a store: match `Write`/`Any` watches in the write's own space.
        let hit = WatchHit {
            watch: WatchId(0), // assigned by `dispatch`
            space: watch_space_of(w.target),
            addr: w.addr,
            old: w.old,
            value: w.new,
            size: if w.size >= 2 { Size::Word } else { Size::Byte },
            op: BusOp::Write,
            fc: 0, // a VDP-internal write has no bus function code; CPU-vs-DMA is in `via`
            via: match w.via {
                VdpVia::Direct => WatchVia::Direct,
                VdpVia::Dma => WatchVia::Dma,
            },
            pc: self.cur_pc,
            frame: self.cur_frame,
            // Step-granular: the write is drained after the driving CPU step, so this is that step's clock.
            // Reported as a caveat rather than dressed up as precision.
            mclk: self.cur_mclk,
            seq: 0, // assigned by `dispatch`
        };
        if !self.any_match(&hit) {
            return;
        }
        self.dispatch(hit);
    }

    fn stop_requested(&self) -> bool {
        self.specs.iter().any(WatchSpec::wants_stop)
    }
}

/// The [`WatchSpace`] a VDP-internal write lands in.
fn watch_space_of(target: VdpTarget) -> WatchSpace {
    match target {
        VdpTarget::Vram => WatchSpace::Vram,
        VdpTarget::Cram => WatchSpace::Cram,
        VdpTarget::Vsram => WatchSpace::Vsram,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed one write event to a Write watch on its address; it is recorded with the stamped pc/frame.
    #[test]
    fn records_a_write_hit_with_stamped_pc_and_frame() {
        let mut wp = Watchpoints::new(16);
        let id = wp.add_watch(0xFF_0000..=0xFF_0001, WatchOp::Write, "target");
        wp.on_step_boundary(0x0212, 3);
        wp.on_event(BusEvent {
            op: BusOp::Write,
            fc: 5,
            addr: 0xFF_0000,
            size: Size::Word,
            value: 0x0001,
        });
        assert_eq!(
            wp.hits(),
            &[WatchHit {
                watch: id,
                space: WatchSpace::Bus,
                addr: 0xFF_0000,
                old: 0,
                value: 0x0001,
                size: Size::Word,
                op: BusOp::Write,
                fc: 5,
                via: WatchVia::Bus,
                pc: 0x0212,
                frame: 3,
                mclk: 0,
                seq: 0,
            }]
        );
        assert_eq!(wp.dropped(), 0);
    }

    fn ev(op: BusOp, addr: u32, value: u32) -> BusEvent {
        BusEvent {
            op,
            fc: 5,
            addr,
            size: Size::Byte,
            value,
        }
    }

    /// A Write watch ignores reads of the watched address; a Read watch ignores writes.
    #[test]
    fn op_filter_selects_reads_or_writes() {
        let mut write_only = Watchpoints::new(16);
        write_only.add_watch(0xFF_0000..=0xFF_0000, WatchOp::Write, "w");
        write_only.on_event(ev(BusOp::Read, 0xFF_0000, 0));
        assert_eq!(
            write_only.hits().len(),
            0,
            "a read does not hit a Write watch"
        );
        write_only.on_event(ev(BusOp::Write, 0xFF_0000, 1));
        assert_eq!(write_only.hits().len(), 1, "the write hits");

        let mut read_only = Watchpoints::new(16);
        read_only.add_watch(0xFF_0000..=0xFF_0000, WatchOp::Read, "r");
        read_only.on_event(ev(BusOp::Write, 0xFF_0000, 1));
        assert_eq!(
            read_only.hits().len(),
            0,
            "a write does not hit a Read watch"
        );
        read_only.on_event(ev(BusOp::Read, 0xFF_0000, 0));
        assert_eq!(read_only.hits().len(), 1, "the read hits");
    }

    /// A Write watch also catches the 68000 TAS (its RMW write cycle); a Read watch does not.
    #[test]
    fn write_watch_catches_tas_read_watch_does_not() {
        let mut write_only = Watchpoints::new(16);
        write_only.add_watch(0xFF_0000..=0xFF_0000, WatchOp::Write, "w");
        write_only.on_event(ev(BusOp::Tas, 0xFF_0000, 0x80));
        assert_eq!(write_only.hits().len(), 1, "TAS is a write to the address");

        let mut read_only = Watchpoints::new(16);
        read_only.add_watch(0xFF_0000..=0xFF_0000, WatchOp::Read, "r");
        read_only.on_event(ev(BusOp::Tas, 0xFF_0000, 0x80));
        assert_eq!(
            read_only.hits().len(),
            0,
            "a Read watch ignores the TAS store"
        );
    }

    /// An access outside every watched range records nothing.
    #[test]
    fn access_outside_the_range_is_not_recorded() {
        let mut wp = Watchpoints::new(16);
        wp.add_watch(0xFF_0000..=0xFF_00FF, WatchOp::Any, "range");
        wp.on_event(ev(BusOp::Write, 0xFF_0100, 1)); // one past the top
        wp.on_event(ev(BusOp::Write, 0xFE_FFFF, 1)); // one below the bottom
        assert_eq!(wp.hits().len(), 0);
        assert_eq!(wp.dropped(), 0);
    }

    /// The hit log is a bounded drop-oldest ring: past `cap`, the oldest hit is dropped, the drop is counted,
    /// and the retained hits keep their original monotonic `seq` (so a `seq` gap marks the drop).
    #[test]
    fn ring_buffer_drops_oldest_and_counts_and_keeps_seq() {
        let mut wp = Watchpoints::new(2);
        wp.add_watch(0xFF_0000..=0xFF_00FF, WatchOp::Any, "range");
        for i in 0..5u32 {
            wp.on_event(ev(BusOp::Write, 0xFF_0000, i));
        }
        let seqs: Vec<u64> = wp.hits().iter().map(|h| h.seq).collect();
        let vals: Vec<u32> = wp.hits().iter().map(|h| h.value).collect();
        assert_eq!(wp.hits().len(), 2, "log bounded at cap");
        assert_eq!(vals, vec![3, 4], "the two most recent hits are retained");
        assert_eq!(seqs, vec![3, 4], "retained hits keep their original seq");
        assert_eq!(wp.dropped(), 3, "the first three hits were dropped");
    }

    /// The ring stays bounded and correct over many more drops than its capacity — the compaction path (the
    /// head index reaching `cap`) runs repeatedly and never loses order, count, or `seq`.
    #[test]
    fn ring_buffer_compaction_stays_bounded_over_many_drops() {
        let mut wp = Watchpoints::new(4);
        wp.add_watch(0xFF_0000..=0xFF_00FF, WatchOp::Any, "range");
        for i in 0..1000u32 {
            wp.on_event(ev(BusOp::Write, 0xFF_0000, i));
        }
        let vals: Vec<u32> = wp.hits().iter().map(|h| h.value).collect();
        let seqs: Vec<u64> = wp.hits().iter().map(|h| h.seq).collect();
        assert_eq!(
            vals,
            vec![996, 997, 998, 999],
            "the last cap hits, in order"
        );
        assert_eq!(seqs, vec![996, 997, 998, 999], "original seqs retained");
        assert_eq!(wp.dropped(), 996);
        assert_eq!(wp.matched(), 1000, "every matched access counted");
        assert!(
            wp.buf.capacity() <= 64,
            "the backing buffer stays bounded (was {})",
            wp.buf.capacity()
        );
    }

    /// `Any` matches every op; `take_hits` drains the log; `clear` removes the registered watches.
    #[test]
    fn any_matches_all_ops_take_hits_drains_clear_removes_watches() {
        let mut wp = Watchpoints::new(16);
        wp.add_watch(0xFF_0000..=0xFF_0000, WatchOp::Any, "any");
        wp.on_event(ev(BusOp::Read, 0xFF_0000, 0));
        wp.on_event(ev(BusOp::Write, 0xFF_0000, 1));
        wp.on_event(ev(BusOp::Tas, 0xFF_0000, 0x80));
        let drained = wp.take_hits();
        assert_eq!(drained.len(), 3, "Any matched read, write, and TAS");
        assert_eq!(wp.hits().len(), 0, "take_hits left the log empty");

        wp.clear();
        wp.on_event(ev(BusOp::Write, 0xFF_0000, 2));
        assert_eq!(
            wp.hits().len(),
            0,
            "clear removed the watch — nothing matches now"
        );
    }

    /// `take_hits` after a ring drop returns exactly the live window (not the dropped prefix), and leaves the
    /// log empty for the next batch.
    #[test]
    fn take_hits_after_a_drop_returns_only_the_live_window() {
        let mut wp = Watchpoints::new(2);
        wp.add_watch(0xFF_0000..=0xFF_0000, WatchOp::Any, "any");
        for i in 0..5u32 {
            wp.on_event(ev(BusOp::Write, 0xFF_0000, i));
        }
        let drained = wp.take_hits();
        assert_eq!(
            drained.iter().map(|h| h.value).collect::<Vec<_>>(),
            vec![3, 4]
        );
        assert_eq!(wp.hits().len(), 0);
        wp.on_event(ev(BusOp::Write, 0xFF_0000, 9));
        assert_eq!(wp.hits().len(), 1, "recording continues after the drain");
        assert_eq!(wp.hits()[0].seq, 5, "seq stays monotonic across the drain");
    }

    // --- T1: mclk ------------------------------------------------------------------------------------------

    /// `on_event_at` latches the access's own master clock into the hit; `on_event` (no clock supplied) keeps
    /// the last latched value.
    #[test]
    fn on_event_at_stamps_the_hit_with_its_mclk() {
        let mut wp = Watchpoints::new(16);
        wp.add_watch(0xFF_0000..=0xFF_0000, WatchOp::Any, "any");
        wp.on_event_at(ev(BusOp::Write, 0xFF_0000, 1), 12_345);
        wp.on_event_at(ev(BusOp::Read, 0xFF_0000, 1), 12_567);
        wp.on_event(ev(BusOp::Write, 0xFF_0000, 2)); // untimed: keeps the last latch
        let mclks: Vec<u64> = wp.hits().iter().map(|h| h.mclk).collect();
        assert_eq!(mclks, vec![12_345, 12_567, 12_567]);
    }

    /// A VDP-internal hit is stamped with the driving step's clock (the latest timestamped bus access), and
    /// that approximation is reported as a caveat rather than presented as the write's own time.
    #[test]
    fn vdp_hits_carry_the_step_clock_and_say_so() {
        let mut wp = Watchpoints::new(16);
        wp.add_vdp_watch(WatchSpace::Vram, 0..=0xFF, WatchOp::Any, "vram");
        wp.on_event_at(ev(BusOp::Write, 0xC0_0000, 0), 900_000);
        wp.on_vdp_write(vw(VdpTarget::Vram, 0x10, 0, 1, 1, VdpVia::Direct));
        assert_eq!(wp.hits()[0].mclk, 900_000, "the driving step's clock");
        assert!(
            wp.caveats()
                .iter()
                .any(|c| c.contains("step-granular") && c.contains("VDP")),
            "the approximation travels with the numbers: {:?}",
            wp.caveats()
        );
    }

    // --- T2: ids, labels, enumeration, removal --------------------------------------------------------------

    /// Each watch gets a distinct id; a hit names the watch that recorded it; the label is resolvable from the
    /// id; `remove` retires one watch and leaves the others alone.
    #[test]
    fn watch_ids_attribute_hits_and_resolve_labels() {
        let mut wp = Watchpoints::new(16);
        let low = wp.add_watch(0xFF_0000..=0xFF_000F, WatchOp::Any, "low");
        let high = wp.add_watch(0xFF_0010..=0xFF_001F, WatchOp::Any, "high");
        assert_ne!(low, high, "ids are distinct");
        wp.on_event(ev(BusOp::Write, 0xFF_0000, 1));
        wp.on_event(ev(BusOp::Write, 0xFF_0010, 2));
        assert_eq!(wp.hits()[0].watch, low);
        assert_eq!(wp.hits()[1].watch, high);
        assert_eq!(wp.label_of(low), Some("low"));
        assert_eq!(wp.label_of(high), Some("high"));

        assert!(wp.remove(low), "removing a registered watch reports true");
        assert!(!wp.remove(low), "removing it twice reports false");
        assert_eq!(wp.label_of(low), None, "a retired id resolves to nothing");
        assert_eq!(wp.watches().len(), 1, "the other watch survives");
        wp.on_event(ev(BusOp::Write, 0xFF_0000, 3));
        assert_eq!(wp.hits().len(), 2, "the removed watch matches nothing now");
    }

    /// Ids are never reused: a watch added after a `clear` gets a fresh id, so a stale handle can never
    /// silently resolve to a different watch.
    #[test]
    fn watch_ids_are_never_reused() {
        let mut wp = Watchpoints::new(16);
        let first = wp.add_watch(0..=0xFF, WatchOp::Any, "first");
        wp.clear();
        let second = wp.add_watch(0..=0xFF, WatchOp::Any, "second");
        assert_ne!(first, second);
        assert_eq!(wp.label_of(first), None);
        assert_eq!(wp.label_of(second), Some("second"));
    }

    /// An access matching several watches is recorded **once**, attributed to the lowest-id matching `Record`
    /// watch — while every matching watch still counts it.
    #[test]
    fn overlapping_watches_record_once_and_count_separately() {
        let mut wp = Watchpoints::new(16);
        let wide = wp.add_watch(0xFF_0000..=0xFF_00FF, WatchOp::Any, "wide");
        let narrow = wp.add_watch(0xFF_0000..=0xFF_0000, WatchOp::Any, "narrow");
        wp.on_event(ev(BusOp::Write, 0xFF_0000, 1));
        assert_eq!(wp.hits().len(), 1, "recorded once, not twice");
        assert_eq!(wp.hits()[0].watch, wide, "attributed to the lowest id");
        assert_eq!(wp.watch(wide).unwrap().matched, 1);
        assert_eq!(wp.watch(narrow).unwrap().matched, 1, "both counted it");
        assert_eq!(wp.matched(), 1, "one matched access overall");
    }

    /// When the lowest-id matching watch stores nothing (`Count`), the hit is attributed to the lowest-id
    /// matching watch that *does* record — never to a watch whose log it is not in.
    #[test]
    fn a_count_watch_never_claims_a_recorded_hit() {
        let mut wp = Watchpoints::new(16);
        let counter =
            wp.add(Watch::bus(0xFF_0000..=0xFF_00FF, WatchOp::Any, "count").mode(WatchMode::Count));
        let recorder = wp.add_watch(0xFF_0000..=0xFF_00FF, WatchOp::Any, "record");
        wp.on_event(ev(BusOp::Write, 0xFF_0000, 1));
        assert_eq!(wp.hits().len(), 1);
        assert_eq!(wp.hits()[0].watch, recorder);
        assert_eq!(
            wp.watch(counter).unwrap().matched,
            1,
            "the count still counts"
        );
    }

    // --- T3: modes ------------------------------------------------------------------------------------------

    /// `Count` stores nothing and counts everything — the mode that makes a wide watch survivable.
    #[test]
    fn count_mode_stores_nothing_and_counts_everything() {
        let mut wp = Watchpoints::new(16);
        let id = wp.add(Watch::bus(0..=0xFFFF_FFFF, WatchOp::Any, "all").mode(WatchMode::Count));
        for i in 0..1000u32 {
            wp.on_event(ev(BusOp::Read, i, i));
        }
        assert_eq!(wp.hits().len(), 0, "nothing stored");
        assert_eq!(
            wp.dropped(),
            0,
            "and nothing dropped — it was never offered"
        );
        let r = wp.watch(id).unwrap();
        assert_eq!(r.matched, 1000);
        assert_eq!(r.census, None, "no census outside Census mode");
    }

    /// `Census` groups by key with counts and a distinct-key cardinality — the query that settled the two
    /// retracted CRAM root causes (the *set* of destination indices, not the total).
    #[test]
    fn census_mode_groups_by_key_with_a_distinct_count() {
        let mut wp = Watchpoints::new(0); // cap 0: a pure-aggregate run stores nothing at all
        let id = wp.add(
            Watch::bus(0..=0xFFFF, WatchOp::Any, "cram-ish")
                .mode(WatchMode::Census(CensusKey::Addr)),
        );
        for _ in 0..10 {
            wp.on_event(ev(BusOp::Write, 0x0004, 1));
        }
        for _ in 0..3 {
            wp.on_event(ev(BusOp::Write, 0x0024, 1));
        }
        let r = wp.watch(id).unwrap();
        assert_eq!(r.census, Some(vec![(0x04, 10), (0x24, 3)]));
        assert_eq!(r.distinct_keys, 2, "exactly two destinations, ever");
        assert!(!r.keys_capped);
        assert_eq!(r.census_overflow, 0);
        assert_eq!(wp.hits().len(), 0);
    }

    /// The census cap refuses new keys **loudly**: known keys keep counting, the overflow is counted, and
    /// `keys_capped` marks the cardinality as a lower bound. The default cap is 256, not 16.
    #[test]
    fn census_cap_refuses_loudly_and_keeps_counting_known_keys() {
        let mut wp = Watchpoints::new(0);
        let id = wp.add(
            Watch::bus(0..=0xFFFF, WatchOp::Any, "capped")
                .mode(WatchMode::Census(CensusKey::Addr))
                .key_cap(3),
        );
        for a in 0..10u32 {
            wp.on_event(ev(BusOp::Read, a, 0));
        }
        wp.on_event(ev(BusOp::Read, 0, 0)); // a known key, past the cap: still counted
        let r = wp.watch(id).unwrap();
        assert_eq!(r.census, Some(vec![(0, 2), (1, 1), (2, 1)]));
        assert_eq!(r.distinct_keys, 3, "a LOWER bound — see keys_capped");
        assert!(r.keys_capped);
        assert_eq!(r.census_overflow, 7, "the seven refused keys are counted");
        assert!(
            wp.caveats().iter().any(|c| c.contains("LOWER BOUND")),
            "the cap is never silent: {:?}",
            wp.caveats()
        );
        assert_eq!(
            DEFAULT_CENSUS_KEY_CAP, 256,
            "the default cap is 256: the episodes that mattered ranged over 390-516 distinct keys"
        );
    }

    /// Every census key extractor, on one stream.
    #[test]
    fn census_keys_extract_what_they_name() {
        let mk = |key: CensusKey| {
            let mut wp = Watchpoints::new(0);
            let id = wp.add(Watch::bus(0..=0xFFFF, WatchOp::Any, "k").mode(WatchMode::Census(key)));
            wp.on_event(BusEvent {
                op: BusOp::Read,
                fc: 6,
                addr: 0x0123,
                size: Size::Word,
                value: 0x3C3C,
            });
            wp.on_event(BusEvent {
                op: BusOp::Write,
                fc: 5,
                addr: 0x0145,
                size: Size::Long,
                value: 0x3C40,
            });
            wp.watch(id).unwrap().census.unwrap()
        };
        assert_eq!(mk(CensusKey::Addr), vec![(0x0123, 1), (0x0145, 1)]);
        assert_eq!(mk(CensusKey::AddrPage(8)), vec![(0x01, 2)], "same page");
        assert_eq!(mk(CensusKey::Fc), vec![(5, 1), (6, 1)]);
        assert_eq!(mk(CensusKey::Op), vec![(0, 1), (1, 1)]);
        assert_eq!(mk(CensusKey::Size), vec![(2, 1), (4, 1)]);
        assert_eq!(mk(CensusKey::Value), vec![(0x3C3C, 1), (0x3C40, 1)]);
        assert_eq!(
            mk(CensusKey::ValueHiEqLo),
            vec![(0, 1), (1, 1)],
            "$3C3C duplicates its byte, $3C40 does not"
        );
        assert_eq!(CensusKey::Op.describe(2), "Tas");
        assert_eq!(CensusKey::ValueHiEqLo.describe(1), "hi==lo");
    }

    /// Every mode records the first and last matched access, stamped — the "when did this first happen?"
    /// question the hand-tuned frame budgets were approximating.
    #[test]
    fn first_and_last_stamps_are_recorded_in_every_mode() {
        let mut wp = Watchpoints::new(1);
        let rec = wp.add_watch(0xFF_0000..=0xFF_0000, WatchOp::Any, "rec");
        let cnt =
            wp.add(Watch::bus(0xFF_0000..=0xFF_0000, WatchOp::Any, "cnt").mode(WatchMode::Count));
        assert_eq!(wp.watch(rec).unwrap().first, None, "never fired yet");

        wp.on_step_boundary(0x0100, 7);
        wp.on_event_at(ev(BusOp::Write, 0xFF_0000, 1), 1_000);
        wp.on_step_boundary(0x0200, 9);
        wp.on_event_at(ev(BusOp::Write, 0xFF_0000, 2), 2_000);

        let first = Stamp {
            pc: 0x0100,
            frame: 7,
            mclk: 1_000,
            seq: 0,
        };
        let last = Stamp {
            pc: 0x0200,
            frame: 9,
            mclk: 2_000,
            seq: 1,
        };
        for id in [rec, cnt] {
            let r = wp.watch(id).unwrap();
            assert_eq!(r.first, Some(first), "first occurrence");
            assert_eq!(r.last, Some(last), "last occurrence");
            assert_eq!(r.matched, 2);
        }
    }

    // --- §6: the fc filter ----------------------------------------------------------------------------------

    /// An `fc` filter is a record-time filter like any other: a non-matching function code stores nothing.
    #[test]
    fn fc_filter_selects_the_master() {
        let mut wp = Watchpoints::new(16);
        wp.add(Watch::bus(0..=0xFFFF, WatchOp::Any, "dma-only").fc(0));
        wp.on_event(ev(BusOp::Write, 0x10, 1)); // fc 5 — the CPU
        assert_eq!(wp.hits().len(), 0, "a CPU access is filtered out");
        wp.on_event(BusEvent {
            op: BusOp::Write,
            fc: 0,
            addr: 0x10,
            size: Size::Byte,
            value: 1,
        });
        assert_eq!(wp.hits().len(), 1, "the fc-0 master hits");
        assert_eq!(wp.seen(), 2, "both were offered");
    }

    // --- F-TRACE-SIZEFILTER: the size and address-parity filters --------------------------------------------

    /// A bus event at `addr` of `size` (fc 5, the CPU), for the size/parity filter tests.
    fn ev_sized(op: BusOp, addr: u32, size: Size, value: u32) -> BusEvent {
        BusEvent {
            op,
            fc: 5,
            addr,
            size,
            value,
        }
    }

    /// A `size` filter is a record-time filter shaped exactly like `fc`: optional, defaulting to "don't
    /// care", and a non-matching width stores nothing.
    #[test]
    fn size_filter_selects_the_access_width() {
        let mut wp = Watchpoints::new(16);
        let word = wp.add(Watch::bus(0..=0xFFFF, WatchOp::Any, "words").size(Size::Word));
        let any = wp.add_watch(0..=0xFFFF, WatchOp::Any, "any-width");
        for size in [Size::Byte, Size::Word, Size::Long, Size::Word] {
            wp.on_event(ev_sized(BusOp::Read, 0x10, size, 0));
        }
        assert_eq!(wp.watch(word).unwrap().matched, 2, "only the two words");
        assert_eq!(
            wp.watch(any).unwrap().matched,
            4,
            "an unfiltered watch is unchanged"
        );
        assert_eq!(
            wp.watch(word).unwrap().size,
            Some(Size::Word),
            "the report carries the filter"
        );
        assert_eq!(
            wp.watch(any).unwrap().size,
            None,
            "and says so when there is none"
        );
    }

    /// An address-parity filter splits the UDS (even) half from the LDS (odd) half — the distinction four
    /// `K4Probe` counters make by hand (`examples/k4_openbus_probe.rs`).
    #[test]
    fn addr_parity_filter_selects_even_or_odd() {
        let mut wp = Watchpoints::new(16);
        let even = wp.add(
            Watch::bus(0xA1_0000..=0xA1_001F, WatchOp::Read, "even").addr_parity(AddrParity::Even),
        );
        let odd = wp.add(
            Watch::bus(0xA1_0000..=0xA1_001F, WatchOp::Read, "odd").addr_parity(AddrParity::Odd),
        );
        for addr in 0xA1_0000..=0xA1_0004u32 {
            wp.on_event(ev_sized(BusOp::Read, addr, Size::Byte, 0));
        }
        assert_eq!(wp.watch(even).unwrap().matched, 3, "$00, $02, $04");
        assert_eq!(wp.watch(odd).unwrap().matched, 2, "$01, $03");
        assert_eq!(
            wp.watch(even).unwrap().addr_parity,
            Some(AddrParity::Even),
            "the report carries the filter"
        );
        assert_eq!(wp.watch(odd).unwrap().addr_parity, Some(AddrParity::Odd));
        assert_eq!(wp.seen(), 5, "all five were offered");
    }

    /// The size and parity filters compose with each other and with `fc`, all as one conjunction.
    #[test]
    fn size_and_parity_and_fc_compose_as_a_conjunction() {
        let mut wp = Watchpoints::new(16);
        let id = wp.add(
            Watch::bus(0..=0xFFFF, WatchOp::Read, "byte.odd.cpu")
                .size(Size::Byte)
                .addr_parity(AddrParity::Odd)
                .fc(5),
        );
        wp.on_event(ev_sized(BusOp::Read, 0x11, Size::Byte, 0)); // matches
        wp.on_event(ev_sized(BusOp::Read, 0x10, Size::Byte, 0)); // wrong parity
        wp.on_event(ev_sized(BusOp::Read, 0x11, Size::Word, 0)); // wrong size
        wp.on_event(ev_sized(BusOp::Write, 0x11, Size::Byte, 0)); // wrong op
        wp.on_event(BusEvent {
            op: BusOp::Read,
            fc: 0,
            addr: 0x11,
            size: Size::Byte,
            value: 0,
        }); // wrong fc
        assert_eq!(wp.watch(id).unwrap().matched, 1, "exactly one full match");
    }

    /// `K4Probe`'s read-arm classifier, copied from `examples/k4_openbus_probe.rs` (including its outer
    /// `BusOp::Read | BusOp::Tas` gate) so these tests compare against the real thing rather than a paraphrase.
    #[derive(Default)]
    struct K4Counters {
        io_even_byte_reads: u64,
        io_word_reads: u64,
        status_upper_reads: u64,
        status_odd_byte_reads: u64,
    }

    impl K4Counters {
        fn classify(&mut self, e: &BusEvent) {
            if !matches!(e.op, BusOp::Read | BusOp::Tas) {
                return;
            }
            match e.addr {
                0xA1_0000..=0xA1_001F => match e.size {
                    Size::Byte if e.addr & 1 == 0 => self.io_even_byte_reads += 1,
                    Size::Word => self.io_word_reads += 1,
                    _ => {}
                },
                0xC0_0004..=0xC0_0007 => {
                    if e.size == Size::Word || e.addr & 1 == 0 {
                        self.status_upper_reads += 1;
                    } else {
                        self.status_odd_byte_reads += 1;
                    }
                }
                _ => {}
            }
        }
    }

    /// The four `K4Probe` counters that motivated this filter (`examples/k4_openbus_probe.rs`, the
    /// `$A10000-$A1001F` and `$C00004-$C00007` read arms), classified by the probe's own code, versus the
    /// same classification expressed as watch configuration.
    ///
    /// **Three of the four are one watch each.** `status_upper_reads` is a *disjunction* (Word **or** even
    /// address) and no single conjunction of optional filters expresses it — see
    /// [`status_upper_reads_is_a_disjunction_no_single_watch_expresses`].
    ///
    /// The stream is Byte and Word only, which is what the 68000 bus adapter actually emits (a `.l` access is
    /// two word bus cycles — `m68000::microop::Size`; the integration test `bus_emits_only_byte_and_word`
    /// pins it on a real run). `Size::Long` is where `status_odd_byte_reads` stops being "Byte and odd" —
    /// see [`status_odd_byte_reads_is_odd_and_non_word_not_odd_and_byte`].
    #[test]
    fn three_of_the_four_k4_counters_are_expressible_as_watch_config() {
        // Every (range x size x parity) cell the classifier distinguishes, at the widths the bus emits.
        let mut stream = Vec::new();
        for base in [0xA1_0000u32, 0xC0_0004] {
            for off in 0..4u32 {
                for size in [Size::Byte, Size::Word] {
                    stream.push(ev_sized(BusOp::Read, base + off, size, 0));
                }
            }
        }
        // Plus traffic no counter and no watch may claim: outside both ranges, and a write.
        stream.push(ev_sized(BusOp::Read, 0xFF_0000, Size::Word, 0));
        stream.push(ev_sized(BusOp::Write, 0xA1_0000, Size::Byte, 0));

        let mut hand = K4Counters::default();
        let mut wp = Watchpoints::new(0);
        let io_even_byte = wp.add(
            Watch::bus(0xA1_0000..=0xA1_001F, WatchOp::Read, "io_even_byte_reads")
                .size(Size::Byte)
                .addr_parity(AddrParity::Even)
                .mode(WatchMode::Count),
        );
        let io_word = wp.add(
            Watch::bus(0xA1_0000..=0xA1_001F, WatchOp::Read, "io_word_reads")
                .size(Size::Word)
                .mode(WatchMode::Count),
        );
        let status_odd_byte = wp.add(
            Watch::bus(
                0xC0_0004..=0xC0_0007,
                WatchOp::Read,
                "status_odd_byte_reads",
            )
            .size(Size::Byte)
            .addr_parity(AddrParity::Odd)
            .mode(WatchMode::Count),
        );
        for e in &stream {
            hand.classify(e);
            wp.on_event(*e);
        }

        let of = |id| wp.watch(id).unwrap().matched;
        assert_eq!(hand.io_even_byte_reads, 2, "$A10000 and $A10002, as bytes");
        assert_eq!(of(io_even_byte), hand.io_even_byte_reads);
        assert_eq!(hand.io_word_reads, 4, "one word read at each of the four");
        assert_eq!(of(io_word), hand.io_word_reads);
        assert_eq!(
            hand.status_odd_byte_reads, 2,
            "$C00005 and $C00007, as bytes"
        );
        assert_eq!(of(status_odd_byte), hand.status_odd_byte_reads);
        assert_eq!(hand.status_upper_reads, 6, "the fourth is exercised too");
        assert_eq!(wp.seen(), stream.len() as u64, "every event was offered");
    }

    /// A second honest residual, found by writing this test rather than by reading the design doc: the design
    /// calls `status_odd_byte_reads` "Byte **and** odd address", but the probe computes it as the `else` of
    /// `Word || even` — i.e. **odd and non-Word**, which also claims an odd `Size::Long` access.
    ///
    /// The two agree on every stream the real bus produces (Byte/Word only), and diverge the moment a `Long`
    /// bus event exists. Pinned here so the equivalence is a stated precondition, not an accident.
    #[test]
    fn status_odd_byte_reads_is_odd_and_non_word_not_odd_and_byte() {
        let stream = [
            ev_sized(BusOp::Read, 0xC0_0005, Size::Byte, 0),
            ev_sized(BusOp::Read, 0xC0_0005, Size::Long, 0),
        ];
        let mut hand = K4Counters::default();
        let mut wp = Watchpoints::new(0);
        let id = wp.add(
            Watch::bus(0xC0_0004..=0xC0_0007, WatchOp::Read, "byte.odd")
                .size(Size::Byte)
                .addr_parity(AddrParity::Odd)
                .mode(WatchMode::Count),
        );
        for e in &stream {
            hand.classify(e);
            wp.on_event(*e);
        }
        assert_eq!(
            hand.status_odd_byte_reads, 2,
            "the probe claims the Long too"
        );
        assert_eq!(
            wp.watch(id).unwrap().matched,
            1,
            "a Byte filter does not — the equivalence holds only while the bus emits no Long"
        );
    }

    /// The honest residual. `status_upper_reads` is `size == Word` **OR** `addr & 1 == 0`; a watch spec is a
    /// conjunction of optional filters, so no single watch expresses it. This pass deliberately did **not**
    /// grow a predicate language to catch it (Fable's sanction was `Option<Size>` + parity, nothing more).
    ///
    /// It is recoverable only by *summing disjoint watches* that enumerate the size domain — which the caller
    /// must do by hand, and which is only correct because `Size` has exactly three variants.
    #[test]
    fn status_upper_reads_is_a_disjunction_no_single_watch_expresses() {
        let mut stream = Vec::new();
        for off in 0..4u32 {
            for size in [Size::Byte, Size::Word, Size::Long] {
                stream.push(ev_sized(BusOp::Read, 0xC0_0004 + off, size, 0));
            }
        }
        let mut hand = 0u64;
        for e in &stream {
            if e.size == Size::Word || e.addr & 1 == 0 {
                hand += 1;
            }
        }

        // The naive single watch: "Word", "even", or the conjunction of both — none of the three is it.
        for (label, w) in [
            (
                "word-only",
                Watch::bus(0xC0_0004..=0xC0_0007, WatchOp::Read, "w").size(Size::Word),
            ),
            (
                "even-only",
                Watch::bus(0xC0_0004..=0xC0_0007, WatchOp::Read, "e").addr_parity(AddrParity::Even),
            ),
            (
                "word-and-even",
                Watch::bus(0xC0_0004..=0xC0_0007, WatchOp::Read, "we")
                    .size(Size::Word)
                    .addr_parity(AddrParity::Even),
            ),
        ] {
            let mut wp = Watchpoints::new(0);
            let id = wp.add(w.mode(WatchMode::Count));
            for e in &stream {
                wp.on_event(*e);
            }
            assert_ne!(
                wp.watch(id).unwrap().matched,
                hand,
                "{label} cannot be the disjunction"
            );
        }

        // The recovery: Word (any parity) + Byte-and-even + Long-and-even. Disjoint by construction, and
        // exhaustive only because `Size` has exactly three variants.
        let mut wp = Watchpoints::new(0);
        let parts: Vec<_> = [
            Watch::bus(0xC0_0004..=0xC0_0007, WatchOp::Read, "word").size(Size::Word),
            Watch::bus(0xC0_0004..=0xC0_0007, WatchOp::Read, "byte.even")
                .size(Size::Byte)
                .addr_parity(AddrParity::Even),
            Watch::bus(0xC0_0004..=0xC0_0007, WatchOp::Read, "long.even")
                .size(Size::Long)
                .addr_parity(AddrParity::Even),
        ]
        .into_iter()
        .map(|w| wp.add(w.mode(WatchMode::Count)))
        .collect();
        for e in &stream {
            wp.on_event(*e);
        }
        let sum: u64 = parts.iter().map(|id| wp.watch(*id).unwrap().matched).sum();
        assert_eq!(sum, hand, "three disjoint watches sum to the disjunction");
    }

    // --- T4: seen -------------------------------------------------------------------------------------------

    /// `seen` counts every delivery, matched or not — so a zero result is legible as "live instrument, nothing
    /// found" rather than "instrument never attached".
    #[test]
    fn seen_is_the_negative_control() {
        let mut wp = Watchpoints::new(16);
        assert_eq!(wp.seen(), 0);
        assert!(
            wp.caveats().iter().any(|c| c.contains("seen = 0")),
            "an unattached instrument says so: {:?}",
            wp.caveats()
        );
        wp.add_watch(0xFF_0000..=0xFF_0000, WatchOp::Any, "never-hit");
        for a in 0..100u32 {
            wp.on_event(ev(BusOp::Read, a, 0));
        }
        wp.on_vdp_write(vw(VdpTarget::Vram, 0, 0, 1, 1, VdpVia::Direct));
        assert_eq!(wp.seen(), 101, "bus events and VDP writes both count");
        assert_eq!(wp.matched(), 0, "a live instrument that found nothing");
        assert!(
            !wp.caveats().iter().any(|c| c.contains("seen = 0")),
            "and it no longer claims to be unattached"
        );
    }

    // --- §10: stop_after ------------------------------------------------------------------------------------

    /// `stop_after` turns a watch into a stop condition for the sink-generic run loop.
    #[test]
    fn stop_after_raises_the_run_loop_stop_signal() {
        let mut wp = Watchpoints::new(16);
        wp.add(Watch::bus(0xFF_0000..=0xFF_0000, WatchOp::Any, "third").stop_after(3));
        for i in 0..2u32 {
            wp.on_event(ev(BusOp::Write, 0xFF_0000, i));
            assert!(!wp.stop_requested(), "not yet: {i} hits");
        }
        wp.on_event(ev(BusOp::Write, 0xFF_0000, 2));
        assert!(wp.stop_requested(), "the third hit asks the run to stop");
    }

    /// With no `stop_after` configured a `Watchpoints` never ends a run — attaching a recorder must not
    /// change how long the machine runs.
    #[test]
    fn a_plain_watch_never_stops_a_run() {
        let mut wp = Watchpoints::new(16);
        wp.add_watch(0..=0xFFFF_FFFF, WatchOp::Any, "everything");
        for i in 0..100u32 {
            wp.on_event(ev(BusOp::Write, i, i));
        }
        assert!(!wp.stop_requested());
    }

    // --- §4.4: the PSG master caveat --------------------------------------------------------------------------

    /// An fc census over the PSG port carries the master-conflation caveat: a 68000 write through the Z80
    /// window is re-emitted Z80-shaped, so *both* master signals read as Z80 there.
    #[test]
    fn an_fc_census_over_the_psg_port_carries_the_master_caveat() {
        let mut wp = Watchpoints::new(0);
        wp.add(
            Watch::bus(0x7F11..=0x7F11, WatchOp::Write, "psg.fc")
                .mode(WatchMode::Census(CensusKey::Fc)),
        );
        assert!(
            wp.caveats().iter().any(|c| c.contains("F-TRACE-MASTER")),
            "{:?}",
            wp.caveats()
        );

        // The same census somewhere else draws no such caveat.
        let mut elsewhere = Watchpoints::new(0);
        elsewhere.add(
            Watch::bus(0xFF_0000..=0xFF_FFFF, WatchOp::Write, "ram.fc")
                .mode(WatchMode::Census(CensusKey::Fc)),
        );
        assert!(elsewhere
            .caveats()
            .iter()
            .all(|c| !c.contains("F-TRACE-MASTER")));
    }

    // --- VDP-internal watches (v2) -----------------------------------------------------------------------

    use crate::vdp::{VdpTarget, VdpVia, VdpWrite};

    fn vw(target: VdpTarget, addr: u32, old: u32, new: u32, size: u8, via: VdpVia) -> VdpWrite {
        VdpWrite {
            target,
            addr,
            old,
            new,
            size,
            via,
        }
    }

    /// A VRAM watch records a `VdpWrite` as a hit: resolved region address, old→new, byte size, `via`, and the
    /// step-boundary PC/frame — space is `Vram`, and a VDP write reports as a `Write` with `fc = 0`.
    #[test]
    fn vram_watch_records_a_vdp_write_hit() {
        let mut wp = Watchpoints::new(16);
        let id = wp.add_vdp_watch(WatchSpace::Vram, 0x0100..=0x01FF, WatchOp::Write, "tile");
        wp.on_step_boundary(0x0400, 2);
        wp.on_vdp_write(vw(VdpTarget::Vram, 0x0100, 0xAB, 0xBE, 1, VdpVia::Direct));
        assert_eq!(
            wp.hits(),
            &[WatchHit {
                watch: id,
                space: WatchSpace::Vram,
                addr: 0x0100,
                old: 0xAB,
                value: 0xBE,
                size: Size::Byte,
                op: BusOp::Write,
                fc: 0,
                via: WatchVia::Direct,
                pc: 0x0400,
                frame: 2,
                mclk: 0,
                seq: 0,
            }]
        );
    }

    /// A DMA-driven write attributes `via = Dma` and a CRAM word maps to `Size::Word`.
    #[test]
    fn cram_watch_records_a_word_and_dma_via() {
        let mut wp = Watchpoints::new(16);
        wp.add_vdp_watch(WatchSpace::Cram, 0x0000..=0x007F, WatchOp::Any, "palette");
        wp.on_vdp_write(vw(VdpTarget::Cram, 0x0002, 0x0000, 0x0EEE, 2, VdpVia::Dma));
        let h = wp.hits()[0];
        assert_eq!(h.space, WatchSpace::Cram);
        assert_eq!(h.size, Size::Word, "a CRAM word is two bytes");
        assert_eq!(h.via, WatchVia::Dma, "DMA-driven");
        assert_eq!(h.old, 0x0000);
        assert_eq!(h.value, 0x0EEE);
    }

    /// Spaces are isolated: a VRAM watch ignores a CRAM/VSRAM write, a bus watch ignores every VDP write, and a
    /// VDP watch ignores every bus event.
    #[test]
    fn spaces_do_not_cross() {
        let mut wp = Watchpoints::new(16);
        wp.add_vdp_watch(WatchSpace::Vram, 0..=0xFFFF, WatchOp::Any, "vram");
        wp.on_vdp_write(vw(VdpTarget::Cram, 0x0000, 0, 1, 2, VdpVia::Direct));
        wp.on_vdp_write(vw(VdpTarget::Vsram, 0x0000, 0, 1, 2, VdpVia::Direct));
        wp.on_event(ev(BusOp::Write, 0x0000, 1)); // a bus write at the same numeric address
        assert_eq!(wp.hits().len(), 0, "the VRAM watch matches none of these");

        let mut bus = Watchpoints::new(16);
        bus.add_watch(0..=0xFFFF, WatchOp::Any, "bus");
        bus.on_vdp_write(vw(VdpTarget::Vram, 0x0000, 0, 1, 1, VdpVia::Direct));
        assert_eq!(
            bus.hits().len(),
            0,
            "a bus watch ignores VDP-internal writes"
        );
    }

    /// A VDP write is a store: it hits `Write`/`Any` watches, never a `Read` watch.
    #[test]
    fn vdp_write_is_a_store_for_op_filtering() {
        let mut read = Watchpoints::new(16);
        read.add_vdp_watch(WatchSpace::Vram, 0..=0xFFFF, WatchOp::Read, "r");
        read.on_vdp_write(vw(VdpTarget::Vram, 0x10, 0, 1, 1, VdpVia::Direct));
        assert_eq!(read.hits().len(), 0, "a Read watch ignores a VDP store");

        let mut write = Watchpoints::new(16);
        write.add_vdp_watch(WatchSpace::Vram, 0..=0xFFFF, WatchOp::Write, "w");
        write.on_vdp_write(vw(VdpTarget::Vram, 0x10, 0, 1, 1, VdpVia::Direct));
        assert_eq!(write.hits().len(), 1, "a Write watch catches it");
    }

    /// `wants_vdp_writes` gates the (currency-sensitive) VDP capture: false with only bus watches, true once a
    /// VDP watch is registered, false again after `clear`.
    #[test]
    fn wants_vdp_writes_tracks_vdp_watch_registration() {
        let mut wp = Watchpoints::new(16);
        assert!(!wp.wants_vdp_writes(), "no watches → capture stays off");
        wp.add_watch(0..=0xFF, WatchOp::Any, "bus");
        assert!(
            !wp.wants_vdp_writes(),
            "a bus watch does not arm VDP capture"
        );
        wp.add_vdp_watch(WatchSpace::Vram, 0..=0xFF, WatchOp::Any, "vram");
        assert!(wp.wants_vdp_writes(), "a VDP watch arms capture");
        wp.clear();
        assert!(!wp.wants_vdp_writes(), "clear disarms");
    }
}
