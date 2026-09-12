//! The `Vdp` — the Sega 315-5313 video display processor's owned state.
//!
//! Plain owned data (`Clone` + bincode + `PartialEq`), a field of [`crate::system::System`]. It owns the
//! four Oracle-hashed regions (VRAM/CRAM/VSRAM + the 24 registers) at their fixed hardware sizes; the
//! rendering output stays **derived, not state** (nothing render-related serializes). Timing (the h/v
//! counters, vblank/hblank) is a pure function of the master clock, computed at read time — never an
//! incremental counter — so it is not stored here either.
//!
//! Behavioral facts implemented here are pinned in `docs/2026-07-16-vdp-recon.md` (cited R1–R12) against
//! the ratified design brief `docs/2026-07-01-vdp-design.md`; no emulator source informs this code
//! (clean-room, audit policy 3).

use crate::rng::SplitMix64;
use crate::state_hash::{CRAM_SIZE, REG_COUNT, VRAM_SIZE, VSRAM_SIZE};

/// Master-clock ticks per scanline (NTSC): the line is a fixed 3420 mclk.
pub const MCLK_PER_LINE: u64 = 3420;
/// Scanlines per frame (NTSC V28): 224 active + 38 blanking.
pub const LINES_PER_FRAME: u64 = 262;

/// **The active display's height in lines: 224**, in V28, the only vertical mode this core models. The one
/// owner of that number in `oracle-core` (lens M53/M67).
///
/// It is also the **index of the first blanking line**, and that is the same fact rather than a second
/// number that happens to agree: lines count from the first active one, so the line after the last active
/// line is line `ACTIVE_LINES`. That is where the VBlank status flag sets ([`Vdp::vblank`], the V
/// counter's `0xDF`→`0xE0` step, recon R2), where the HINT counter stops decrementing
/// ([`Vdp::hint_anchor_tick`]), where `System` announces the frame boundary, and so where the renderer's
/// *last completed frame* ends ([`crate::render::cram_divergence_caveat`]). V30 (the 240-line mode, PAL
/// only) would move every one of them together, which is why they share one name; it would also have to
/// move [`LINES_PER_FRAME`], which this core fixes at NTSC's 262.
///
/// `u16`, the width of a line number wherever one is used, widened losslessly where it meets the `u64`
/// master clock. Not every `224` in the tree is this: aeon's `SCREEN_HEIGHT` (the player-bound inset two
/// test fixtures copy) happens to equal it and must not be folded in.
pub const ACTIVE_LINES: u16 = 224;
/// Master-clock ticks per NTSC frame (`MCLK_PER_LINE * LINES_PER_FRAME` = 896_040).
pub const MCLK_PER_FRAME: u64 = MCLK_PER_LINE * LINES_PER_FRAME;
/// Master-clock ticks the **active display** occupies inside a line — the same 2560 in both modes, because
/// H40 draws 320 pixels at 8 mclk each and H32 draws 256 at 10 mclk each: `320 × 8 = 256 × 10 = 2560`. The
/// remaining `MCLK_PER_LINE − MCLK_PER_ACTIVE` = 860 mclk (122.9 CPU cycles) is horizontal blanking, which is
/// the figure the demand side derives its whole HBlank window from (`3420/7 − 320*8/7`,
/// `aeon/docs/benchmarks/scanline-p2/HBLANK-WINDOW-SWEEP-RESULTS.md:368`).
///
/// **Decision B-1** (`docs/2026-08-19-subline-recon.md` §B). This pixel axis deliberately does **not** reuse
/// [`Vdp::h_counter`]'s grid, which divides the line uniformly into 422 (H40) / 342 (H32) positions. That
/// divide is exact for H32 (3420 / 342 = 10) but an approximation for H40 (3420 / 422 = 8.104, the EDCLK
/// mix — the same class of approximation the tree already names as F-SLOTGRID); carried onto the pixel axis
/// it would put active-display end at 2593 mclk instead of 2560, i.e. up to ~4 px of error at the right edge,
/// with no evidence behind it.
///
/// That leaves a knowingly inconsistent pair of in-line clocks: an HV read says active display ends at H
/// `$9F` ≈ 2593 mclk, while this axis says 2560. Registered as follow-up **F-SUBLINE-HGRID** — re-derive
/// `h_counter`'s H40 grid from this same anchor. Explicitly **out** of the F-SCANLINE-SUBLINE arc:
/// `h_counter` feeds `$C00008` reads, [`Vdp::hblank`], `hint_offset` and `vint_offset`, so moving it is an
/// observable behaviour change on every ROM, which is a different currency conversation from an opt-in
/// capture change.
pub const MCLK_PER_ACTIVE: u64 = 2560;

/// The first active-display pixel that shows the **new** value of a VDP write performed `d_mclk` into its own
/// scanline (`d_mclk = mclk % MCLK_PER_LINE`, so 0..3419) — the mclk → pixel-x mapping of
/// `docs/2026-08-19-subline-recon.md` §B. Pixels `0..x` still show the pre-write value.
///
/// `h40` is the **resolved row's own** mode, not a live register read (decision **B-2**): a mid-line H40→H32
/// switch would otherwise place `x` on a grid the row was never drawn on. The result is clamped to that
/// mode's active width, which is what makes the two out-of-active cases fall out as the identity this arc has
/// to preserve:
///
/// - `d_mclk = 0` → `x = 0`, the whole row takes the new value. Correct: scheduler events drain before the
///   CPU step, so a write at exactly `N × MCLK_PER_LINE` happens after line N resolved but before any of its
///   pixels are consumed.
/// - `d_mclk >= MCLK_PER_ACTIVE` → `x = width`, the row is untouched and the write first shows in row N+1 —
///   exactly today's line-atomic behaviour. **Nothing outside `[0, MCLK_PER_ACTIVE)` moves.**
///
/// `floor` rather than `ceil`, for consistency with [`Vdp::h_counter`]'s own derivation style and because the
/// ±1 px it decides is two orders of magnitude below the resolution limit the stamp itself carries — writes
/// are located to the start of the driving instruction (see the [`now_mclk`](Vdp#structfield.now_mclk) field;
/// follow-up F-SUBLINE-ACCESSMCLK).
pub(crate) fn subline_x(d_mclk: u64, h40: bool) -> usize {
    // The renderer's own width rule, `render::active_width`, which `Vdp::active_display` reads too (wave-3
    // residue 4; this was a copy of it). The literals 320/256 the test below checks are the independent
    // anchor: the dot clock's 8 and 10 mclk per pixel over one active span.
    let width = u64::from(crate::render::active_width(h40));
    let mclk_per_pixel = MCLK_PER_ACTIVE / width; // 8 (H40) / 10 (H32)
    (d_mclk / mclk_per_pixel).min(width) as usize
}

/// **How many slots the sprite attribute table has: 80**, the H40 table, and the most any mode parses
/// (H32 parses, and the cache refreshes, only the first 64; see [`Vdp::parsed_sprite_max`]).
///
/// The one name for it in `oracle-core` (lens M62). It lives here because the SAT is the VDP's: the SAT
/// cache below is sized by it, and the cache write-through (`write_vram_byte`, [`Vdp::poke_vram`]) bounds
/// its H40 window with it. `render.rs` re-exports it as `render::SAT_SLOTS`, the path wave 1B published,
/// for `sprite_limits`' H40 parse cap, `sprites_decoded`'s decode range and the sprite walk's
/// out-of-range-link test. **Not** the `320` in `sprite_limits`: that is the H40 per-line *pixel* budget,
/// which happens to equal the H40 width and is neither this nor a width.
pub const SAT_SLOTS: usize = 80;

/// SAT-cache size: [`SAT_SLOTS`] entries × the **cached 4 bytes** of each 8-byte entry — Y (word) +
/// size/link (word); recon R5 / RR8. X + tile/attr (the other 4 bytes) are never cached. It was the literal
/// `320`, a third spelling of the slot count beside the two bare `80`s.
pub(crate) const SAT_CACHE_LEN: usize = SAT_SLOTS * 4;

/// The VDP's owned state. The four hashed regions are always at their fixed hardware sizes
/// ([`crate::state_hash`]): allocated so by [`Vdp::power_on`], and a restore refuses a snapshot that decodes
/// one at any other size ([`crate::system::System::restore`]). They are handed out as arrays of those sizes
/// ([`Vdp::vram`] says where the stored `Vec` becomes one); the `state_hash`/`export_state` currencies read
/// straight through them, so their byte layout is frozen.
/// One write-FIFO slot (recon R3): the data word plus a copy of the command code/address registers as they
/// were when the write was enqueued. The physical slot **retains** its data after the entry drains (the pending
/// count drops but the bytes stay) — that stale data is exactly what the read snoop quirk (CRAM read $08,
/// VSRAM read $04, and the 8-bit VRAM read $0C) and the CRAM/VSRAM fill data source read ("the data written
/// 4 writes ago").
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, bincode::Encode, bincode::Decode)]
struct FifoEntry {
    data: u16,
    code: u8,
    addr: u16,
}

/// **Which of the three VDP-internal memories** — the one name for it, with three consumers.
///
/// # Why this is one type and not two (H30)
///
/// It was two, `VdpTarget` here and a second one spelled `Target` ~1,650 lines down, in **this same
/// module**: same three
/// variants in the same order, the same six derives, both `pub`, both on wire types (`VdpWrite::target`
/// and [`DmaRecord::target`]), and neither doc mentioning the other. [`Vdp::write_data`] read the
/// data-port decode as one spelling and then re-spelled it variant-for-variant to `capture` the write —
/// **twice, inside one function body** — so the conversion was the only thing asserting they agreed, and
/// it asserted it by being written out by hand.
///
/// Merging them is byte-identical on the wire and provably so: `bincode`'s derive encodes an enum as its
/// **variant index**, the two variant lists were `Vram, Cram, Vsram` in that order, and every save-state
/// and `frame_report` field that carried either one carried the same three indices before and after.
///
/// Its three consumers are the data-port decode ([`Vdp::target_of`]), the captured-write record
/// ([`VdpWrite`], watchpoints v2), and the DMA record ([`DmaRecord`], the `frame_report` introspection
/// surface). A fourth would add a consumer, never a fourth spelling.
///
/// **Note the one thing this type is not.** Code `$0C` — the 8-bit VRAM read — decodes to
/// [`VdpTarget::Vram`] and is told apart by [`Vdp::is_vram_byte_read`], a predicate on the read path,
/// **not** by a fourth variant here; see that method for the A2 argument.
#[derive(Clone, Copy, PartialEq, Eq, Debug, bincode::Encode, bincode::Decode)]
pub enum VdpTarget {
    Vram,
    Cram,
    Vsram,
}

/// How a captured VDP write was driven (watchpoints v2): straight from a CPU data-port write (`Direct`) or as
/// a step of a DMA transfer (`Dma`). A `Dma` write attributes to the instruction that *triggered* the transfer
/// (the step-boundary PC), which is exactly the "instruction $X triggered a DMA that wrote VRAM $Y" story.
#[derive(Clone, Copy, PartialEq, Eq, Debug, bincode::Encode, bincode::Decode)]
pub enum VdpVia {
    Direct,
    Dma,
}

/// One VDP-internal memory mutation captured at its choke point (watchpoints v2 — the "who wrote this tile?"
/// primitive). `addr` is the resolved byte address **within the region** (VRAM 0..64Ki, CRAM 0..128, VSRAM
/// 0..80). `size` is 1 for a VRAM byte (VRAM is captured byte-granular, at its single `write_vram_byte`
/// choke) or 2 for a CRAM/VSRAM word. `old`/`new` are the pre/post values in that width. `via` distinguishes a
/// direct CPU write from a DMA step.
///
/// `mclk` is the instant **the VDP performed this write**, read from the clock its entry point carried (the
/// [`now_mclk`](Vdp#structfield.now_mclk) shadow). It retires `F-TRACE-VDPWRITE-MCLK`, whose whole content
/// was that a captured write had no clock of its own and had to borrow the draining CPU step's. Two limits
/// travel with it, both inherited from the bus rather than introduced here: it is **instruction-granular**
/// (`MegaDriveBus` freezes the clock for one 68000 instruction — follow-up F-SUBLINE-ACCESSMCLK), and every
/// word of one DMA burst shares the **transfer's** instant (decision C-6 — follow-up F-SUBLINE-DMASPREAD).
/// A write driven through an untimed entry point ([`Vdp::data_write`], hand-driven fixtures) carries the
/// last clock the VDP was given.
#[derive(Clone, Copy, PartialEq, Eq, Debug, bincode::Encode, bincode::Decode)]
pub struct VdpWrite {
    pub target: VdpTarget,
    pub addr: u32,
    pub old: u32,
    pub new: u32,
    pub size: u8,
    pub via: VdpVia,
    pub mclk: u64,
}

#[derive(Clone, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct Vdp {
    /// 64 KiB video RAM.
    vram: Vec<u8>,
    /// 128 bytes of color RAM, stored in Oracle's byte layout (the `state_hash` currency defines the form).
    cram: Vec<u8>,
    /// 80 bytes of vertical-scroll RAM.
    vsram: Vec<u8>,
    /// The 24 VDP registers.
    regs: [u8; REG_COUNT],
    /// The frozen HV-counter value returned while the M3 latch (reg 0 bit 1) is set — an interim model of
    /// the HV counter latch (recon R2; the real trigger is the lightgun HL pin, which nothing on the Mega
    /// Drive pad path asserts). Populated when M3 is turned on by a register write; the read side
    /// ([`Vdp::hv_counter_read`]) consults it here.
    hv_latch: u16,
    /// The live command code CD5..CD0 (recon R1). Directly fed by the control port — no input latch. Bit 0
    /// (CD0) = 0 read / 1 write; bit 5 (CD5) = DMA (push 6). Determines the data-port target + direction.
    code: u8,
    /// The live A15..A0 address register (recon R1) — the auto-incrementing address *is* this register.
    addr: u16,
    /// The control-port first/second-write toggle (recon R1). Set by a first command word; cleared by the
    /// second word, a data-port write, or a status read (the last is the instrument-sourced experiment pin —
    /// see the pending-toggle experiment in `docs/2026-07-16-vdp-recon.md`). An HV-counter read does NOT
    /// clear it. A `$8xxx` register write never arms it.
    pending: bool,
    /// The data-port read pre-cache buffer (recon R3): a data read returns this and refills it from the
    /// current address (the read path bypasses the write FIFO). Pre-filled when a read command completes.
    ///
    /// It always holds the **full 16-bit word** the target defines, even for the 8-bit VRAM read `$0C`,
    /// whose low half is `vram[address ^ 1]` while its high half keeps the ordinary VRAM byte (A4). Bits the
    /// live command code leaves undefined are masked out and replaced from the FIFO snoop at *consume* time
    /// in [`Vdp::data_read`], not here — the two instants can see different codes (follow-up F-SNOOPWHEN),
    /// so the buffer deliberately stores nothing fabricated.
    read_buffer: u16,
    /// Latched debug flag for the pinned lockup cell (recon R1): a data-port *read* while a write command is
    /// armed hangs real hardware. We model a deterministic outcome (open bus + this flag) instead of hanging
    /// the host — a documented divergence (see the divergence note on [`Vdp::data_read`]).
    latched_fault: bool,
    /// The vertical-interrupt pending latch (recon R12). Set by the VInt scheduler event at line 224; the
    /// **only** thing that clears it is the 68k interrupt-acknowledge ([`Vdp::acknowledge`]) — not a status
    /// read, not clearing the enable, not a frame boundary. Gated into the IPL by IE0 (reg 1 bit 5).
    vint_pending: bool,
    /// The horizontal-interrupt pending latch (recon R12). Set on HINT-counter underflow; cleared only by
    /// the 68k interrupt-acknowledge. Gated into the IPL by IE1 (reg 0 bit 4).
    hint_pending: bool,
    /// The HINT line counter (recon R7): reloaded from reg 10 on every vblank line and on underflow;
    /// decremented once per active line (0..=224). Underflow sets [`Vdp::hint_pending`].
    hint_counter: u8,
    /// Sprite-overflow status latch (status bit 6). OR-set by `commit_scanline_sprites` (the render
    /// pipeline's per-line commit) and cleared by a status read — both in this file. Serialized here so the
    /// status word can report it. (Said "Read-only this push" until the lens sweep.)
    sprite_overflow: bool,
    /// Sprite-collision status latch (status bit 5). Same lifecycle as `sprite_overflow`: OR-set by
    /// `commit_scanline_sprites`, cleared on a status read. (Said "read-only here".)
    sprite_collision: bool,
    /// Odd-frame flag (status bit 4). Advanced each frame when the VInt latch is set (recon R12 delivery)
    /// under the reference's toggle rule `interlace_enabled && !odd` — forced to 0 while interlace is off
    /// (see [`Vdp::raise_vint`]).
    odd_frame: bool,
    /// The SAT cache (recon R5 / RR8): 80 entries × the cached 4 bytes (Y word + size/link word), in the
    /// same big-endian byte order as VRAM. **Real hardware state, not derivable from VRAM** — the write-
    /// through window updates it on every VRAM write against the *current* reg-5 base, and changing reg 5
    /// never invalidates/reloads it (the Castlevania Bloodlines stale-cache mixing). Evaluation reads only
    /// this; render fetches X + tile/attr from VRAM at the current base. Power-on = zeros (the real cache is
    /// undefined until the first SAT write; games/fixtures write it before it matters — documented interim).
    /// Serialized (round-trips snapshots); it is in **neither** frozen currency (`state_hash`/`export_state`
    /// read only VRAM/CRAM/VSRAM/regs), a v2 export-currency candidate.
    sat_cache: Vec<u8>,
    /// The R10 sprite-masking carry: whether the previously-rendered line ended in a sprite-pixel (dot /
    /// pixel-budget) overflow. Seeds the next line's x=0 masking latch so a first-on-line x=0 sprite masks
    /// (Nemesis's previous-line-dot-overflow exception; Kabuto's "previous line/frame" reach). Persists
    /// across lines *and* frames. Real state, serialized (round-trips); not in either frozen currency.
    /// Power-on = false. Committed by [`Vdp::render_scanline`] and by its picture-free twin
    /// [`Vdp::advance_scanline`] (the run loop calls one or the other every active line, per finding C5);
    /// the pure `render_line` seeds from it read-only.
    sprite_dot_overflow_carry: bool,
    /// The 4-entry write FIFO (recon R3), a physical ring. Each data-port write enqueues a [`FifoEntry`] here.
    /// `fifo_write` is the next slot to fill; the oldest pending entry is `fifo[(fifo_write − fifo_len) & 3]`;
    /// the **next-available** entry (about to be overwritten = written 4 writes ago) is `fifo[fifo_write]` —
    /// the snoop-quirk / CRAM-VSRAM-fill data source. Real hardware state, serialized; in **neither** frozen
    /// currency (`state_hash`/`export_state` read only VRAM/CRAM/VSRAM/regs). Power-on = empty.
    ///
    /// **Coarse Phase-2 model** (design brief §2): the FIFO carries *timing* (`fifo_len` + the drain clock,
    /// added in the wait-channel slice) and *contents* (`fifo` for snoop/fill-source); the VRAM/CRAM/VSRAM
    /// mutation itself stays applied at enqueue, since a data-port read waits for the FIFO to drain and
    /// rendering latches at line start — no observer can distinguish enqueue-time from drain-time application
    /// within Phase-2 granularity (divergence-ledger entry).
    fifo: [FifoEntry; 4],
    /// Index (0..=3) of the next FIFO slot to enqueue into (the physical write cursor).
    fifo_write: u8,
    /// Pending (not-yet-drained) FIFO entries, 0..=4 — the coarse timing abstraction. Saturates at 4 until the
    /// drain clock advances it; a 5th write while full stalls the 68k via the wait channel.
    fifo_len: u8,
    /// The mclk up to which the FIFO has been drained (recon R3) — the drain clock. Two things move it.
    /// **`fifo_drain`** (and the stall paths in `data_write_at` / `data_read_at`) advances it one entry at a
    /// time by [`Vdp::entry_drain_cost`]: on an **active display line** that is the real intra-line access
    /// schedule (T16/S1, [`Vdp::next_active_slot`] — 2 slots per VRAM word, 1 per CRAM/VSRAM word, at the
    /// published H32/H40 slot *positions*); on a **blanked** line it is still the flat per-line rate
    /// (167/205 slots per line — follow-up F-BLANKSLOT). **[`Vdp::dma_complete`]** additionally re-anchors it
    /// forward to a finished transfer's end instant while entries are still pending (T16/S2), so a DMA's
    /// residual drains from where the transfer ended rather than from where it began. Real timing state,
    /// serialized; in neither frozen currency. Power-on = 0.
    ///
    /// **It is also the 68k's "already charged to" mark (C2).** Whenever this clock sits *ahead* of a bus
    /// access's `now`, it is because the CPU has already been billed a hold reaching that far — either a
    /// /DTACK stall an earlier access of the same (clock-frozen) instruction returned, or a Mem DMA's hold
    /// window, which `MegaDriveBus::run_mem_dma` bills to the arming instruction in full. So both stall
    /// paths measure their wait from `fifo_slot_clock.max(now)`; measuring from `now` alone re-charged
    /// every earlier stall of the same instruction inside each later one.
    fifo_slot_clock: u64,
    /// mclk before which the status DMA-busy bit (bit 1) reads set (recon R4 / Eke: DMA-busy sets on the
    /// control-port setup write; a fill/copy runs the 68k in parallel, so a poll sees busy for the coarse
    /// transfer window). Power-on = 0 (never busy). In neither frozen currency.
    dma_busy_until: u64,
    /// A DMA the 68k has just triggered (recon R4), armed by the trigger write and taken by the bus to execute
    /// (the bus owns the 68k source memory). Always `None` at an instruction boundary — armed and consumed
    /// within one bus access — so it round-trips a quiesced snapshot as `None`. In neither frozen currency.
    dma_pending: Option<DmaRequest>,
    /// The most recently completed DMA, for the `frame_report` introspection surface (design §4; recon R4).
    /// Serialized; in neither frozen currency. Power-on = `None`.
    last_dma: Option<DmaRecord>,
    /// Transient VDP-write capture buffer (watchpoints v2). When [`Vdp::capture_armed`] is set, every write
    /// choke point (`write_vram_byte` for VRAM; the CRAM/VSRAM arms of `write_target`) pushes a [`VdpWrite`]
    /// here. The system drains it after every `step_cpu` and clears it, so it is **empty at every instruction
    /// boundary** — the `dma_pending` precedent. Serialized (round-trips a quiesced snapshot as empty); in
    /// **neither** frozen currency (`state_hash`/`export_state` read only VRAM/CRAM/VSRAM/regs, never this).
    /// Power-on = empty.
    write_captures: Vec<VdpWrite>,
    /// Whether the write-capture buffer is armed (watchpoints v2, and since `F-SCANLINE-SUBLINE` the deferred
    /// scanline emitter too). Arming is **opt-in and zero-cost when off**: each choke-point guard is a single
    /// cheap `if self.capture_armed` test with no behavioral effect, so a run with no VDP watch and no
    /// scanline sink (or a null sink) is byte-for-byte identical to today. Set per-run by the sink-generic
    /// run loop iff the sink wants VDP writes **or** wants rendered rows; false at power-on and between runs.
    /// In neither frozen currency. See [`Vdp::capture_cram_only`] for the narrowed mode a rows-only consumer
    /// arms.
    capture_armed: bool,
    /// Narrows an armed capture to **CRAM writes only** (`F-SCANLINE-SUBLINE`). The deferred scanline
    /// emitter needs the CRAM subset and nothing else, so a run that carries a scanline sink but no VDP
    /// watch sets this: a 64 KiB VRAM fill DMA then pushes **zero** entries instead of 65,536 that would be
    /// built, drained and dropped every frame. A run that also wants writes on the wire clears it, so the
    /// watchpoints path records exactly what it always did.
    ///
    /// A transient run flag exactly like [`Vdp::capture_armed`] and `in_dma` — false at power-on and
    /// between runs, in neither frozen currency. It does ride the bincode snapshot (as they do), which is
    /// the one byte this costs.
    capture_cram_only: bool,
    /// Transient "this write is a DMA step" tag (watchpoints v2): raised around `run_fill`/`run_copy`/
    /// `dma_write_word` so the choke points stamp `via = Dma`; otherwise `via = Direct`. Always false at an
    /// instruction boundary (a DMA runs to completion within one bus access). In neither frozen currency.
    in_dma: bool,
    /// The master clock the VDP is currently working at — a **shadow** of the `now` its timed entry points
    /// already receive ([`Vdp::control_write`], [`Vdp::data_write_at`], [`Vdp::run_fill`], [`Vdp::run_copy`],
    /// [`Vdp::dma_write_word`]), carried down to the write choke points so a write can be located in time and
    /// not merely in order (F-SCANLINE-SUBLINE slice 1, design `docs/2026-08-19-subline-recon.md` §B).
    ///
    /// Two honest limits, both of them properties of the clock the bus hands us rather than of this field:
    ///
    /// - **Instruction-granular.** `MegaDriveBus` freezes `now_mclk` for the duration of one 68000
    ///   instruction, so every word of a `movem` shares one stamp (follow-up **F-SUBLINE-ACCESSMCLK**).
    /// - **One DMA burst = one stamp** (decision **C-6**): every word of a 68k→VDP transfer carries the
    ///   transfer's own `now`, not an advancing per-word clock (follow-up **F-SUBLINE-DMASPREAD**).
    ///
    /// Untimed entry points ([`Vdp::data_write`], the hand-driven unit fixtures) leave it at its previous
    /// value, exactly as `Watchpoints`' own untimed-event rule does. Serialized like the other transient VDP
    /// fields; in **neither** frozen currency (`state_hash`/`export_state` read only VRAM/CRAM/VSRAM/regs).
    /// Power-on = 0.
    now_mclk: u64,
    /// Master clock of the **most recent write to each CRAM entry**, one slot per palette index 0..=63.
    ///
    /// Exists for one caller: `protocol.md` §11.27's caveat on `emulator/pixel_attribution`, which must
    /// say *"emit iff the entry at `cramIndex` has been written since line `y` of the last completed
    /// frame was drawn"*. That is a **measurement**, not a heuristic about raster programs — a vblank
    /// write after the line drew diverges just as much as a mid-frame one, and this catches both —
    /// so the state it needs is the write instant per entry, and nothing coarser will express it.
    /// §11.27 permits a server that cannot stamp per entry to fall back to "any CRAM write since the
    /// line drew"; there are exactly **two** CRAM store sites in this file, so per-entry costs one
    /// store more than the fallback would and the coarse rule is not worth implementing.
    ///
    /// **`None` is "never written", and it is an `Option` rather than a zero sentinel because the
    /// sentinel was wrong and a test caught it.** The first draft stored a bare `u64` and argued that 0
    /// could serve for both, since a stamp of 0 only satisfies the rule's comparison when line `y` of
    /// the last completed frame also drew at mclk 0. That instant is real and reachable: **line 0 of
    /// frame 0**, queried any time after the first frame completes. An entry nobody had ever written
    /// then disclosed, which is the unconditional shape §11.27 forbids, arrived at by arithmetic rather
    /// than by intent. `a_never_written_entry_is_silent_for_every_line_of_every_frame` is the row.
    ///
    /// **Reset-safe without special handling.** [`crate::system::System::reset`] rebuilds the whole
    /// machine (`*self = Self::new(seed)`), so these stamps return to 0 in the same instant the master
    /// clock does; the two can never be compared across a reset boundary.
    ///
    /// Serialized like the other transient VDP fields (it round-trips a snapshot, so a restored machine
    /// discloses what the live one would). In **neither** frozen currency — `state_hash` and
    /// `export_state` read only VRAM/CRAM/VSRAM/regs — and written unconditionally on the CRAM store
    /// path, so it cannot make an instrumented machine differ from a plain one. Power-on = all `None`.
    cram_written_mclk: [Option<u64>; CRAM_ENTRIES],
    /// **The VSRAM read latch** (cause A1, `docs/2026-09-12-vdp-port-access-full-rom.md`): the internal
    /// register VSRAM read data passes through, and what a data-port read of VSRAM `$50-$7F` returns
    /// ([`Vdp::vsram_byte`] is the decode). Nemesis, SpritesMind *VDP Internals* p.4: "When you read beyond
    /// the end of VSRAM, you don't get the first VSRAM entry, what actually happens is that the read
    /// doesn't latch any data at all, and what gets returned is actually the current state of the
    /// internal register that latches the VSRAM read data."
    ///
    /// **Fed by the renderer's committed vertical-scroll fetch, and only by it**
    /// ([`Vdp::latch_vsram_fetch`], called from `render_scanline` and `advance_scanline`, the two
    /// committed per-line paths; the masked renders take `&self` and cannot reach it). On a line whose
    /// display is enabled it takes the last VSRAM word that line's background fetch reads (see
    /// `render.rs`'s `last_vscroll_fetch`); a display-disabled line fetches nothing and leaves it alone.
    ///
    /// **The port's own reads are not modelled as feeding it**, although on hardware they very likely
    /// pass through the same register. The reason is granularity, and the ROM shows it. On hardware the
    /// renderer reads VSRAM before each 2-cell column's tilemap fetch, so during active display a render
    /// fetch lands between two external access slots and overwrites whatever a port read left there. This
    /// core renders a line at its start, so that interleaving cannot be expressed. A port read feeding the
    /// latch here would leave its own word for the very next read-ahead, and VDPFIFOTesting's tables
    /// contradict that: its VSRAM fill tests read words 0-39 and then `$50`, and hardware answers `$0123`
    /// (word 1, the renderer's fetch in full-screen mode), not `$0560` (word 39, the port's preceding
    /// read). What would overturn this: a capture showing a `$50` read in vblank, or with the display
    /// disabled, returning the port's previous word.
    ///
    /// Real state, serialized (a restore is exact). In **neither** frozen currency: `state_hash` and
    /// `export_state` read only VRAM/CRAM/VSRAM/regs, the same rule that already keeps the read
    /// pre-cache, the FIFO and the SAT cache out of them. Its effect is still observable to both, because
    /// a latch divergence reaches 68000 RAM through the reads that return it. Power-on = 0 (hardware's
    /// power-on value is unknown).
    vsram_read_latch: u16,
}

/// Palette entries in CRAM: 64 nine-bit colours, two bytes each ([`CRAM_SIZE`] = 128).
pub const CRAM_ENTRIES: usize = CRAM_SIZE / 2;

impl std::fmt::Debug for Vdp {
    /// Summarize instead of dumping the 64 KiB VRAM buffer (keeps assertion failures readable).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vdp")
            .field("vram", &format_args!("[{} bytes]", self.vram.len()))
            .field("cram", &format_args!("[{} bytes]", self.cram.len()))
            .field("vsram", &format_args!("[{} bytes]", self.vsram.len()))
            .field("regs", &self.regs)
            .field("code", &format_args!("{:#04X}", self.code))
            .field("addr", &format_args!("{:#06X}", self.addr))
            .field("pending", &self.pending)
            .field("latched_fault", &self.latched_fault)
            .finish()
    }
}

/// The length-carrying fields of a [`Vdp`], borrowed mutably for snapshot-surgery tests
/// ([`Vdp::regions_mut`]).
#[cfg(test)]
pub(crate) struct VdpRegionsMut<'a> {
    pub vram: &'a mut Vec<u8>,
    pub cram: &'a mut Vec<u8>,
    pub vsram: &'a mut Vec<u8>,
    pub sat_cache: &'a mut Vec<u8>,
    pub write_captures: &'a mut Vec<VdpWrite>,
    pub fifo_len: &'a mut u8,
    pub fifo_write: &'a mut u8,
}

impl Vdp {
    /// Power on: allocate the four regions at their fixed sizes and seed VRAM with deterministic
    /// pseudo-random bytes drawn from the single seeded RNG (CRAM/VSRAM/registers start zeroed) — exactly
    /// what [`crate::system::System::new`] did before the extraction, so the power-on `state_hash` is
    /// byte-identical. The RNG is drawn from **after** the work-RAM fill, preserving the draw order.
    pub fn power_on(rng: &mut SplitMix64) -> Self {
        let mut vram = vec![0u8; VRAM_SIZE];
        crate::system::fill_random(rng, &mut vram);
        Self {
            vram,
            cram: vec![0u8; CRAM_SIZE],
            vsram: vec![0u8; VSRAM_SIZE],
            regs: [0u8; REG_COUNT],
            hv_latch: 0,
            code: 0,
            addr: 0,
            pending: false,
            read_buffer: 0,
            latched_fault: false,
            vint_pending: false,
            hint_pending: false,
            hint_counter: 0,
            sprite_overflow: false,
            sprite_collision: false,
            odd_frame: false,
            sat_cache: vec![0u8; SAT_CACHE_LEN],
            sprite_dot_overflow_carry: false,
            fifo: [FifoEntry::default(); 4],
            fifo_write: 0,
            fifo_len: 0,
            fifo_slot_clock: 0,
            dma_busy_until: 0,
            dma_pending: None,
            last_dma: None,
            write_captures: Vec::new(),
            capture_armed: false,
            capture_cram_only: false,
            in_dma: false,
            now_mclk: 0,
            cram_written_mclk: [None; CRAM_ENTRIES],
            vsram_read_latch: 0,
        }
    }

    /// Read-only access to VRAM (for the `state_hash`/`export_state` currencies and introspection), as the
    /// fixed-size region [`StateHash::compute`](crate::state_hash::StateHash::compute) takes.
    ///
    /// **The one place the stored `Vec` becomes an array** (lens M70), and likewise for [`cram`](Self::cram)
    /// and [`vsram`](Self::vsram). The fields stay `Vec`s because the bincode snapshot (save states,
    /// checkpoints) encodes a `Vec` with a length prefix and an array without one, so changing the field
    /// type would change every snapshot's bytes. [`power_on`](Self::power_on) allocates each region at its
    /// size and nothing resizes one, so the conversion cannot fail on a machine this crate built; and
    /// [`System::restore`](crate::system::System::restore) refuses a snapshot that decodes any of the three at
    /// another size ([`SnapshotRegion`](crate::system::SnapshotRegion)), so it cannot fail on a restored one
    /// either. Until that check existed, a snapshot decoded with a wrong-length region restored "fine" and
    /// panicked here, at its first read. The `expect` stays as the statement of the invariant: reaching it
    /// means a new way to build a `Vdp` has skipped both.
    pub fn vram(&self) -> &[u8; VRAM_SIZE] {
        self.vram
            .as_slice()
            .try_into()
            .expect("VRAM is allocated at VRAM_SIZE and never resized")
    }

    /// Read-only access to CRAM (Oracle byte layout), as a fixed-size region. See [`vram`](Self::vram).
    pub fn cram(&self) -> &[u8; CRAM_SIZE] {
        self.cram
            .as_slice()
            .try_into()
            .expect("CRAM is allocated at CRAM_SIZE and never resized")
    }

    /// Read-only access to VSRAM, as a fixed-size region. See [`vram`](Self::vram).
    pub fn vsram(&self) -> &[u8; VSRAM_SIZE] {
        self.vsram
            .as_slice()
            .try_into()
            .expect("VSRAM is allocated at VSRAM_SIZE and never resized")
    }

    /// The VDP's share of [`System::restore`](crate::system::System::restore)'s door check: each field of
    /// this struct whose size the code relies on, at that size
    /// ([`SnapshotRegion`](crate::system::SnapshotRegion) lists them and says why each is there). It lives
    /// here rather than in `system.rs` because the fields are private to this module.
    pub(crate) fn check_regions(&self) -> Result<(), crate::system::MalformedSnapshot> {
        use crate::system::{
            check_region,
            RegionBound::{AtMost, Exactly},
            SnapshotRegion as R,
        };
        check_region(R::Vram, Exactly(VRAM_SIZE), self.vram.len())?;
        check_region(R::Cram, Exactly(CRAM_SIZE), self.cram.len())?;
        check_region(R::Vsram, Exactly(VSRAM_SIZE), self.vsram.len())?;
        check_region(R::SatCache, Exactly(SAT_CACHE_LEN), self.sat_cache.len())?;
        // Empty is the rule because the run loop drains this buffer at every instruction boundary and the
        // Z80 cannot write the VDP until decision C-7 lands. C-7 must drain again after the Z80 catch-up:
        // see the ORDERING HAZARD comment in `System::run_until_with_sink`'s drain.
        check_region(R::VdpWriteCaptures, Exactly(0), self.write_captures.len())?;
        let ring = self.fifo.len();
        check_region(R::VdpFifoPending, AtMost(ring), usize::from(self.fifo_len))?;
        check_region(
            R::VdpFifoCursor,
            AtMost(ring - 1),
            usize::from(self.fifo_write),
        )
    }

    /// **Test-only surgery on the fields [`check_regions`](Self::check_regions) guards**, so a test can take
    /// a real snapshot, bend one region, re-encode it (encoding checks nothing) and prove that
    /// `System::restore` refuses it by name.
    #[cfg(test)]
    pub(crate) fn regions_mut(&mut self) -> VdpRegionsMut<'_> {
        VdpRegionsMut {
            vram: &mut self.vram,
            cram: &mut self.cram,
            vsram: &mut self.vsram,
            sat_cache: &mut self.sat_cache,
            write_captures: &mut self.write_captures,
            fifo_len: &mut self.fifo_len,
            fifo_write: &mut self.fifo_write,
        }
    }

    /// Read-only access to the 24 VDP registers.
    pub fn regs(&self) -> &[u8; REG_COUNT] {
        &self.regs
    }

    /// Read-only access to the SAT cache (recon R5 / RR8): 80 entries × the cached 4 bytes (Y word +
    /// size/link word), big-endian. The sprite evaluation walk reads Y/size/link from here.
    pub fn sat_cache(&self) -> &[u8] {
        &self.sat_cache
    }

    /// The R10 sprite-masking carry (recon R10): whether the previously-rendered line ended in a sprite-pixel
    /// (dot) overflow. The pure `render_line` seeds the next line's masking latch from this (read-only);
    /// [`Vdp::render_scanline`] and [`Vdp::advance_scanline`] advance it.
    pub fn sprite_dot_overflow_carry(&self) -> bool {
        self.sprite_dot_overflow_carry
    }

    /// The sprite attribute table base VRAM byte address (recon R5 / RR8): `(reg $05 & mask) << 9`, mask
    /// `0x7E` in H40 (bit 0 forced to the $400 boundary) / `0x7F` in H32 ($200 boundary). Used by both the
    /// write-through window (real state) and the render-time X/tile fetch.
    pub fn sat_base(&self) -> usize {
        let mask = if self.h40() { 0x7E } else { 0x7F };
        ((self.regs[0x05] & mask) as usize) << 9
    }

    /// Mutable access to VRAM — a test/introspection escape hatch **beside** a live data-port write path
    /// ([`Vdp::data_write`] / [`Vdp::data_write_at`]), not a stand-in for one. (It read "the data-port write
    /// path lands in a later slice" until the lens sweep.) Kept crate-internal-friendly but public for the
    /// `System::vram_mut` pass-through.
    pub fn vram_mut(&mut self) -> &mut [u8] {
        &mut self.vram
    }

    // --- Timing FSM: the readable h/v counters + status timing bits are PURE functions of the master
    // clock (granularity C — NTSC V28 geometry is hardcoded per audit policy 4). No incremental counter
    // is stepped; nothing here is stored state. All behavioral values are pinned in recon R2.

    /// H40 (40-cell / 320px) mode iff both horizontal-resolution select bits are set in reg $0C (RS0 = bit
    /// 0, RS1 = bit 7 — the official Sega manual "set both for 40-cell mode" rule); otherwise H32.
    fn h40(&self) -> bool {
        self.regs[0x0C] & 0x81 == 0x81
    }

    /// The readable H counter (recon R2): the top 8 bits of the 9-bit horizontal counter, which sweeps 342
    /// positions (H32) / 422 positions (H40) across the 3420-mclk line. The 8-bit value jumps H32
    /// `0x93`→`0xE9` / H40 `0xB6`→`0xE4` at horizontal retrace. `mclk % 3420` maps linearly across the
    /// positions (the sub-position phase within the line is pure timing).
    pub fn h_counter(&self, mclk: u64) -> u8 {
        let h40 = self.h40();
        let dot = mclk % MCLK_PER_LINE;
        let positions = if h40 { 422 } else { 342 };
        let pos9 = (dot * positions) / MCLK_PER_LINE; // the 9-bit counter position, 0..positions
        let index = (pos9 >> 1) as u16; // the readable value is the top 8 bits
        if h40 {
            if index <= 0xB6 {
                index as u8
            } else {
                (0xE4 + (index - 0xB7)) as u8
            }
        } else if index <= 0x93 {
            index as u8
        } else {
            (0xE9 + (index - 0x94)) as u8
        }
    }

    /// The readable V counter (recon R2, NTSC V28): the scanline number remapped so it jumps `0xEA`→`0xE5`
    /// at line 235 (235 + 27 = 262 lines). On hardware the V counter increments mid-line (R2 anchor H
    /// `0x84`→`0x85` H32 / `0xA4`→`0xA5` H40); we increment at the line boundary — a sub-line phase
    /// difference that is pure timing (documented open item, recon R2).
    pub fn v_counter(&self, mclk: u64) -> u8 {
        let line = (mclk % MCLK_PER_FRAME) / MCLK_PER_LINE; // 0..=261
        if line <= 0xEA {
            line as u8
        } else {
            (0xE5 + (line - 0xEB)) as u8
        }
    }

    /// The HV-counter port value ($C00008): `(V << 8) | H`. Frozen to the M3 latch while reg 0 bit 1 (M3)
    /// is set — the interim HV-latch model (recon R2; see [`Vdp::hv_latch`]).
    pub fn hv_counter_read(&self, mclk: u64) -> u16 {
        if self.regs[0] & 0x02 != 0 {
            self.hv_latch
        } else {
            ((self.v_counter(mclk) as u16) << 8) | self.h_counter(mclk) as u16
        }
    }

    /// VBlank status flag: set across the whole vertical-blank region — V counter ≥ `0xE0`, i.e. line ≥
    /// [`ACTIVE_LINES`], the first line past the active display (the `0xDF`→`0xE0` transition; recon R2).
    /// Pure function of mclk, no stored flag.
    pub fn vblank(&self, mclk: u64) -> bool {
        (mclk % MCLK_PER_FRAME) / MCLK_PER_LINE >= u64::from(ACTIVE_LINES)
    }

    /// HBlank status flag: set across horizontal retrace, bounded by the pinned H anchors (recon R2): H32
    /// sets at `0x93` / clears at `0x05`; H40 sets at `0xB3` / clears at `0x06`. Derived from the readable H
    /// counter so the H↔mclk phase anchors live in exactly one place.
    pub fn hblank(&self, mclk: u64) -> bool {
        let h = self.h_counter(mclk);
        if self.h40() {
            h <= 0x05 || h >= 0xB3
        } else {
            h <= 0x04 || h >= 0x93
        }
    }

    /// The VDP status word ($C00004 read), with the timing bits live (recon R2). Bit layout (official Sega
    /// manual): b0 PAL, b1 DMA-busy, b2 HBlank, b3 VBlank, b4 odd-frame, b5 sprite-collision, b6
    /// sprite-overflow, b7 VINT(F), b8 FIFO-full, b9 FIFO-empty. The FIFO bits are LIVE from `fifo_len`
    /// (A1, VDPFIFOTesting T16): EMPTY = no pending entries, FULL = all 4 slots pending. This function is
    /// a pure snapshot — the mutable status-read path ([`Vdp::control_read_status`]) drains the FIFO to
    /// `mclk` first, so a poll observes the time-based drain and nothing else.
    pub fn status_word(&self, mclk: u64) -> u16 {
        let mut s = 0u16;
        if self.fifo_len == 0 {
            s |= 1 << 9; // FIFO empty (live: no pending entries)
        }
        if self.fifo_len == 4 {
            s |= 1 << 8; // FIFO full (live: all 4 slots pending — the /DTACK-stall condition)
        }
        if self.dma_busy(mclk) {
            // DMA busy (recon R4 / Eke): a fill/copy's coarse transfer window, AND — since A4 — the whole
            // time a fill sits armed and untriggered. One predicate, [`Vdp::dma_busy`].
            s |= 1 << 1;
        }
        if self.vint_pending {
            s |= 1 << 7; // F: VINT-pending readback (conservative no-side-effect pin, recon R12)
        }
        if self.sprite_overflow {
            s |= 1 << 6;
        }
        if self.sprite_collision {
            s |= 1 << 5;
        }
        if self.odd_frame {
            s |= 1 << 4;
        }
        if self.vblank(mclk) || !self.display_enabled() {
            // Bit 3 is forced set while the display is disabled, regardless of the beam position —
            // reference: Oracle `vblankFlag |= !_displayEnabledCached`, "hardware tests have confirmed"
            // (`Devices/315-5313/S315-5313_General.cpp:2345-2351`); hardware ground truth: memtest_68k's
            // `C00004-C00007` row (`4E88`, bit 3 set) reads mid active scan with reg 1 = $04.
            s |= 1 << 3;
        }
        if self.hblank(mclk) {
            s |= 1 << 2;
        }
        s
    }

    // --- Control / data ports + memories (recon R1; the toggle rules also carry the R1 experiment pin).
    // Writes apply immediately through `&mut self` — with the 68k the only bus master there is nothing to
    // defer; the deferred-write seam stays reserved for the Z80/DMA era.

    /// Whether the latched lockup-cell fault has fired (introspection / debuggers; recon R1). Reset only by
    /// power-on.
    pub fn latched_fault(&self) -> bool {
        self.latched_fault
    }

    /// The auto-increment amount (VDP register 15) added to the address after every data-port access.
    fn autoinc(&mut self) {
        self.addr = self.addr.wrapping_add(self.regs[0x0F] as u16);
    }

    /// The current data-port target region, decoded from CD3..CD0 (recon R1). The low nibble names the
    /// region for both plain and DMA (CD5) codes.
    fn target(&self) -> VdpTarget {
        Self::target_of(self.code)
    }

    /// The data-port target region for an arbitrary command `code` (recon R1) — the FIFO stores the code at
    /// enqueue time, so the drain/snoop path decodes a captured code rather than the live one.
    fn target_of(code: u8) -> VdpTarget {
        match code & 0x0F {
            0x3 | 0x8 => VdpTarget::Cram,  // CRAM write (0x3) / CRAM read (0x8)
            0x4 | 0x5 => VdpTarget::Vsram, // VSRAM read (0x4) / write (0x5)
            _ => VdpTarget::Vram,          // VRAM read (0x0) / write (0x1); unknown → VRAM
        }
    }

    /// Whether `code`'s low nibble names a **write** target. Only CD3-CD0 = 0001 / 0011 / 0101 (VRAM /
    /// CRAM / VSRAM write) do; every other code is undefined and "the write … is ignored" (genvdp.txt 1.5f
    /// code table — the same sentence that forbids writing after a read command). The port still *accepts*
    /// the word — it takes its FIFO slot and the address still steps — but nothing reaches memory.
    /// VDPFIFOTesting test 10 (ROM `$FCAA` word 7) pins the case CD0 alone cannot catch: a first-half-only
    /// word over a CRAM *read* command leaves code `001001`, which looks like a write but has no target.
    ///
    /// Shared by **all three** write paths — the ordinary data-port write, the A3b fill-trigger write, and
    /// (since A5) the fill *body* in [`Vdp::run_fill`] — so the valid-code set can only ever be changed in
    /// one place. [`Vdp::target_of`] still does not agree with this predicate: it falls back `_ => Vram` for
    /// an unrecognised nibble, because it answers "which region does this code name", which is a different
    /// question from "may this code write". Nothing now writes on the strength of `target_of` alone. That
    /// asymmetry used to reach memory — a fill armed on a no-write-target code had its trigger suppressed
    /// here and its body wrote VRAM anyway — and was tracked as follow-up **F-FILLTGT**, **retired
    /// 2026-09-12**: VDPFIFOTesting test 34 group 3 does cover the case, and it says the body writes nothing.
    fn code_names_a_write_target(code: u8) -> bool {
        matches!(code & 0x0F, 0x1 | 0x3 | 0x5)
    }

    /// Whether `code` selects the **undocumented 8-bit VRAM read**, CD3-CD0 = `1100` (A4). It is a VRAM
    /// read like code `0000`, but only half of the 16-bit result comes from VRAM: the low byte is the
    /// single byte at `address ^ 1`, and the high byte is stale FIFO contents (see [`Vdp::data_read`]).
    ///
    /// Deliberately a *predicate on the read path*, not a new [`VdpTarget`] variant: code `$0C` still decodes
    /// to `VdpTarget::Vram` for the write path (where it names no valid write target and is ignored, the A2
    /// rule) and for the FIFO drain-cost model, both of which A4 leaves byte-identical.
    fn is_vram_byte_read(code: u8) -> bool {
        code & 0x0F == 0x0C
    }

    /// Store `data` (plus the live code/address registers) into the next physical FIFO ring slot and advance
    /// the write cursor. The physical slot is overwritten in place (retaining nothing of the old entry beyond
    /// the ring position). The ring-write half of [`Vdp::fifo_enqueue`], which is its only caller and which
    /// bumps the pending count.
    fn fifo_store(&mut self, data: u16) {
        self.fifo[self.fifo_write as usize] = FifoEntry {
            data,
            code: self.code,
            addr: self.addr,
        };
        self.fifo_write = (self.fifo_write + 1) & 3;
    }

    /// Enqueue a data-port write into the FIFO ring (recon R3): store into the ring AND count it as pending.
    /// `fifo_len` counts pending entries (saturating at 4 until the drain clock advances it in the
    /// wait-channel slice).
    fn fifo_enqueue(&mut self, data: u16) {
        self.fifo_store(data);
        self.fifo_len = (self.fifo_len + 1).min(4);
    }

    /// The number of pending (not-yet-drained) FIFO entries, 0..=4 (recon R3; introspection / debuggers).
    pub fn fifo_len(&self) -> u8 {
        self.fifo_len
    }

    /// External (CPU/DMA) access slots per line at `mclk` (recon R3): active display H32 = 16 / H40 = 18;
    /// a blanked line (vblank or display-off) H32 = 167 / H40 = 205. One slot = one VRAM byte access.
    ///
    /// A per-line **rate**, with no notion of where in the line the slots sit. Since T16/S1 the FIFO drain
    /// uses it only on **blanked** lines (F-BLANKSLOT); on an active line it reads real slot positions from
    /// [`Vdp::next_active_slot`], whose tables carry these same 18/16 counts by construction.
    /// [`Vdp::dma_cost`] still bills every transfer at this rate.
    fn slots_per_line(&self, mclk: u64) -> u64 {
        let blanked = self.vblank(mclk) || !self.display_enabled();
        match (self.h40(), blanked) {
            (false, false) => 16,
            (true, false) => 18,
            (false, true) => 167,
            (true, true) => 205,
        }
    }

    /// The **positions** of the external (CPU/DMA) access slots within an active-display line, as access
    /// indices into the line's 210 (H40) / 171 (H32) accesses. These are the `~` entries in Kabuto's
    /// published per-line access-pattern strings (hardware notes, Plutiedev mirror):
    ///
    /// ```text
    /// H40: Hssss AsaaBsbb ((A~aaBSbb)*3 AraaBSbb)*5 ~~ s*23 ~ s*11        (210 accesses)
    /// H32: Hssss AsaaBsbb ((A~aaBSbb)*3 AraaBSbb)*4 ~~ s*13 ~ s*13 ~      (171 accesses)
    /// ```
    ///
    /// `H` = hscroll, `A`/`B` = tilemap, `a`/`b` = tile pixels, `S` = sprite refs, `s` = sprite pixels,
    /// `r` = VRAM refresh, `~` = external slot. Expanding them yields exactly **18 `~` + 5 `r`** (H40) and
    /// **16 `~` + 4 `r`** (H32), reproducing the Sega *Genesis Technical Overview* DMA-capacity table's
    /// 18/16 figures — already pinned in `docs/2026-07-16-vdp-recon.md:109` — from an independent source.
    ///
    /// The gaps are **irregular**, and that is the entire content of VDPFIFOTesting test 16 groups
    /// 2/3/5/6/8. The H40 gap sequence is `(8,8,16)×4 | 8,8,15 | 1 | 24 | 26` accesses — the fifth render
    /// group's last gap is 15, not 16, because the pair at 173/174 starts one access early; it sums to the
    /// 210 accesses of the line, which is the arithmetic cross-check. The wrap-around gap of 26
    /// matches TascoDLX's measured figure (SpritesMind t=851: the manual's 16-slot maximum gap is wrong,
    /// "the largest gap is actually 26 slots"), which is independent corroboration that Kabuto's string
    /// transcribes Nemesis's logic-analyser measurements.
    const H40_ACTIVE_SLOTS: [u64; 18] = [
        14, 22, 30, 46, 54, 62, 78, 86, 94, 110, 118, 126, 142, 150, 158, 173, 174, 198,
    ];
    const H32_ACTIVE_SLOTS: [u64; 16] = [
        14, 22, 30, 46, 54, 62, 78, 86, 94, 110, 118, 126, 141, 142, 156, 170,
    ];
    const H40_ACCESSES_PER_LINE: u64 = 210;
    const H32_ACCESSES_PER_LINE: u64 = 171;

    /// The mclk instant of the first external access slot **strictly after** `at`, on an active-display
    /// line. Access `k` is placed at `k × MCLK_PER_LINE / accesses`, i.e. a uniform access grid — an
    /// approximation, since EDCLK is not constant across the line (follow-up **F-SLOTGRID**). Wraps to the
    /// next line's first slot, so the returned instant is always `> at` and a drain always makes progress.
    ///
    /// **The wrap uses the active table unconditionally**, without asking whether the *next* line is active:
    /// a drain that starts past the last slot of line 223 is charged as if line 224 were still displaying,
    /// when it is in fact the first vblank line and nearly every access on it is external. The caller
    /// ([`Vdp::entry_drain_cost`]) makes the same simplification in the other direction — it picks the
    /// active/blanked branch once, from the *start* instant. Recorded as a rider on follow-up
    /// **F-BLANKSLOT**; it is the same active-vs-blanked boundary, seen from the far side.
    fn next_active_slot(&self, at: u64) -> u64 {
        let (slot_indices, accesses): (&[u64], u64) = if self.h40() {
            (&Self::H40_ACTIVE_SLOTS, Self::H40_ACCESSES_PER_LINE)
        } else {
            (&Self::H32_ACTIVE_SLOTS, Self::H32_ACCESSES_PER_LINE)
        };
        let line = at / MCLK_PER_LINE;
        let pos = at % MCLK_PER_LINE;
        // F-SLOTTABLE: `k * MCLK_PER_LINE / accesses` is recomputed per probe, up to 18 divisions per call.
        for &k in slot_indices {
            let t = k * MCLK_PER_LINE / accesses;
            if t > pos {
                return line * MCLK_PER_LINE + t;
            }
        }
        (line + 1) * MCLK_PER_LINE + slot_indices[0] * MCLK_PER_LINE / accesses
    }

    /// The mclk cost of draining one FIFO entry with command `code` at slot-clock instant `at` (recon R3): a
    /// VRAM word exits in 2 slots, a CRAM/VSRAM word in 1. Integer throughout.
    ///
    /// **T16/S1** — on an active display line the two slots are taken from the real schedule
    /// ([`Vdp::next_active_slot`]) rather than the uniform per-line rate, so a VRAM word drains in anywhere
    /// from 260 mclk (a drain starting *on* a slot instant and taking the two close slots of a render group
    /// — `t30 - t14` = 488 − 228) to ~814 (one starting just after the line's last slot, so both its slots
    /// come from the next line), against the old invariant 380. The
    /// per-line *total* is unchanged by construction (the table has exactly 18/16 entries) — this
    /// redistributes drains within a line, it does not add or remove capacity.
    ///
    /// A **blanked** line (vblank or display-off) keeps the aggregate-rate model: nearly every access there
    /// is an external slot, so positions carry almost no information. That is a deliberate, recorded
    /// inconsistency in the model — follow-up **F-BLANKSLOT**.
    fn entry_drain_cost(&self, code: u8, at: u64) -> u64 {
        let slots = match Self::target_of(code) {
            VdpTarget::Vram => 2,
            _ => 1,
        };
        if self.vblank(at) || !self.display_enabled() {
            return slots * MCLK_PER_LINE / self.slots_per_line(at);
        }
        let mut t = at;
        for _ in 0..slots {
            t = self.next_active_slot(t);
        }
        t - at
    }

    /// The **oldest pending** FIFO entry, the next one a drain retires (recon R3): `fifo_len` writes behind
    /// `fifo_write`, modulo the four slots. Meaningful only while `fifo_len > 0`, which each of its three
    /// callers (the drain, a full FIFO's write stall, a read's wait) checks first. One expression, named
    /// (lens M43): it used to be written out verbatim at all three.
    fn fifo_oldest(&self) -> FifoEntry {
        self.fifo[(self.fifo_write.wrapping_sub(self.fifo_len) & 3) as usize]
    }

    /// Advance the FIFO drain clock up to `now` (recon R3): pop each pending entry whose slot cost has elapsed.
    /// When the FIFO empties, the clock coasts forward to `now` (an idle FIFO does not bank drain credit).
    fn fifo_drain(&mut self, now: u64) {
        while self.fifo_len > 0 {
            let oldest = self.fifo_oldest();
            let cost = self.entry_drain_cost(oldest.code, self.fifo_slot_clock);
            if self.fifo_slot_clock + cost > now {
                break;
            }
            self.fifo_slot_clock += cost;
            self.fifo_len -= 1;
        }
        if self.fifo_len == 0 {
            self.fifo_slot_clock = self.fifo_slot_clock.max(now);
        }
    }

    /// The data word in the **next-available** FIFO slot — the entry about to be overwritten (written 4 writes
    /// ago). Recon R3: this is what the read snoop quirk and the CRAM/VSRAM fill data source read. Three read
    /// codes snoop it — CRAM read `$08` and VSRAM read `$04` take their undefined bits from it, and (A4) the
    /// undocumented 8-bit VRAM read `$0C` takes its whole high byte from it. Exposed for introspection;
    /// consumed internally by the snoop/fill slices.
    pub fn fifo_snoop_word(&self) -> u16 {
        self.fifo[self.fifo_write as usize].data
    }

    /// Whether a **DMA fill is armed and waiting for its data-port trigger** (cause A4). The same condition
    /// that makes the next data-port write a fill trigger in [`Vdp::apply_data_write`]: CD5 latched in the
    /// code register and register 23's mode bits naming Fill. Deliberately **derived from existing state**
    /// rather than latched in a new field — the flag and the trigger then cannot disagree, and no snapshot
    /// or `export_state` layout moves.
    ///
    /// Note CD5 only latches while DMA-enable (register 1 bit 4) is set, so a fill command written with DMA
    /// disabled arms nothing and is not busy — VDPFIFOTesting test 38 group 1 (ROM `$C528`), where all four
    /// status samples read clear on hardware and the trigger word lands as an ordinary VRAM write.
    fn fill_armed(&self) -> bool {
        self.code & 0x20 != 0 && self.regs[0x17] & 0xC0 == 0x80
    }

    /// Whether the DMA-busy status bit reads set at `mclk` (recon R4 / A4). Two ways it reads set:
    ///
    /// 1. **A fill is armed** ([`Vdp::fill_armed`]) — busy from the fill's *control* write, before the data
    ///    port has been touched. Eke, *VDP Internals* p.4: "on DMA Fill, busy flag is actually immediately
    ///    (?) set after the CTRL port write, not the DATA port write that starts the Fill operation."
    ///    VDPFIFOTesting pins it: **test 36** (ROM `$B6F8`) samples status at `$B8A2`, between the command
    ///    `$40020082` (`$B89C`) and the `$1234` trigger (`$B8BC`), and hardware reads `$0202`. **Test 38**
    ///    group 2 (ROM `$C850`) samples it there too, and again after a register write `$8144` that clears
    ///    DMA-enable and a half-command `$4002` (`$C870..$C878`) — still set both times.
    /// 2. **A fill or copy's coarse transfer window is still open** (`mclk < dma_busy_until`), the part that
    ///    was already here.
    ///
    /// **What ends an armed-but-never-triggered fill's busy flag is not something the ROM settles.** This
    /// model answers it by construction: nothing ends it except the arming condition going away — the fill
    /// running (`take_dma_request` clears CD5 when the request is consumed, and `run_fill` then opens the
    /// window), a later command word clearing CD5 while DMA-enable is set, or register 23 leaving Fill mode.
    /// Time does not end it, and neither does a frame boundary. Test 38 group 2 rules out the three cheap
    /// alternatives — a register write, clearing DMA-enable, and a new first command word all leave it set.
    /// A ROM that arms a fill, never triggers it, then writes a full non-DMA command word (or register 23)
    /// and polls status would separate this model from a plain latch; one that polls across frames would
    /// separate both from any timed window. See "What would settle the open points" in
    /// `docs/2026-09-12-vdp-port-access-full-rom.md`.
    ///
    /// Introspection companion to the status word's bit 1, which reads it.
    pub fn dma_busy(&self, mclk: u64) -> bool {
        self.fill_armed() || mclk < self.dma_busy_until
    }

    /// **The VSRAM address decode** (cause A1, `docs/2026-09-12-vdp-port-access-full-rom.md`): the byte
    /// index a data-port VSRAM access at `addr` reaches, or `None` for the unbacked `$50-$7F`.
    ///
    /// The VSRAM address is **7 bits**, so it wraps at `$80` (`addr & $7F`, word-aligned), and the 80
    /// bytes of storage cover only `$00-$4F`. Nemesis, SpritesMind "Scaling hardware?" p.5: "Writes to
    /// CRAM and VSRAM wrap at an 0x80 byte boundary. Writes to the upper portion of VSRAM in this region
    /// (0x50-0x80) are discarded." A read there returns [`Vdp::vsram_read_latch`] (see that field).
    /// VDPFIFOTesting pins it: test 23 (a `$80`-word DMA wraps at `$80`), the eight VSRAM fills (74-95,
    /// reads of `$50-$7E`), the copy matrix's VSRAM words (96-122) and test 20's VSRAM half (`$8020`
    /// reaches word 16). It used to be `addr % 80`, which put a write to `$50` on word 0.
    ///
    /// Every data-port path that touches VSRAM storage decodes through here: the port write and the DMA
    /// word (`write_target`), the fill body (`run_fill`, through `write_target`) and the port read-ahead
    /// (`read_target`). The renderer and the debug surfaces index storage directly and never see a port
    /// address.
    fn vsram_byte(addr: u16) -> Option<usize> {
        let b = usize::from(addr & 0x7E);
        (b < VSRAM_SIZE).then_some(b)
    }

    /// The VSRAM read latch (see the [`vsram_read_latch`](Vdp#structfield.vsram_read_latch) field): what
    /// a data-port read of VSRAM `$50-$7F` returns in its eleven defined bits.
    pub fn vsram_read_latch(&self) -> u16 {
        self.vsram_read_latch
    }

    /// Latch one committed vertical-scroll fetch — the renderer's half of the VSRAM read latch. Called
    /// only from the committed per-line paths (`render_scanline`, `advance_scanline`), never from a
    /// masked render, which takes `&self`.
    pub(crate) fn latch_vsram_fetch(&mut self, word: u16) {
        self.vsram_read_latch = word;
    }

    /// Read the current-address word from the data-port target (recon R1/R3). VRAM/CRAM/VSRAM are stored
    /// big-endian (high byte first) — the `state_hash` currency's byte layout.
    fn read_target(&self) -> u16 {
        match self.target() {
            // A4: the 8-bit VRAM read (code $0C) pre-caches the byte at `address ^ 1` into the LOW half —
            // that lane swap is the same one the fill/copy engine's byte writes take (Eke, *Is DMA Fill
            // buggy?*, SpritesMind: "VRAM byte writes … actually occur to VRAM address ^ 1"), and is pinned
            // by VDPFIFOTesting test 6's expected table (ROM $DED4): autoinc 1 from $8000 over the image
            // `11 22 33 44` reads $22, $11, $44, $33.
            //
            // The HIGH half keeps the real VRAM byte even though [`Vdp::data_read`] masks it away and
            // substitutes the FIFO snoop. Storing a fabricated zero there instead would be a value no
            // evidence supports, and it would leak: `data_read` re-decides whether to merge from
            // `self.code` at *consume* time, and A2's own pinned rule makes the two disagree — arm $0C,
            // then any `$8xxx` register write clobbers CD1-CD0 to give code $0E, for which
            // `is_vram_byte_read` is false and no merge happens. Keeping the real byte makes that
            // code-mismatch path return the plain VRAM word, i.e. exactly the pre-A4 behaviour, instead of
            // inventing a new one. The ROM is silent on the seam (follow-up F-SNOOPWHEN); where it is
            // silent, preserving prior behaviour is the conservative choice. Behaviour for $0C itself —
            // the only case the ROM pins — is identical either way.
            VdpTarget::Vram if Self::is_vram_byte_read(self.code) => {
                let b = (self.addr & 0xFFFE) as usize;
                ((self.vram[b] as u16) << 8) | self.vram[(self.addr ^ 1) as usize] as u16
            }
            VdpTarget::Vram => {
                let b = (self.addr & 0xFFFE) as usize;
                ((self.vram[b] as u16) << 8) | self.vram[b | 1] as u16
            }
            VdpTarget::Cram => {
                let b = (self.addr as usize) & 0x7E;
                ((self.cram[b] as u16) << 8) | self.cram[b | 1] as u16
            }
            // A1: 7-bit address; `$50-$7F` latches nothing and returns the VSRAM read latch.
            VdpTarget::Vsram => match Self::vsram_byte(self.addr) {
                Some(b) => ((self.vsram[b] as u16) << 8) | self.vsram[b | 1] as u16,
                None => self.vsram_read_latch,
            },
        }
    }

    /// Store one byte into VRAM **and run the SAT-cache write-through** (recon R5 / RR8). Every VRAM byte
    /// write is checked against the SAT window computed from the *current* reg-5 base: a byte landing in the
    /// cached half (bytes 0..4 = Y + size/link) of an entry within the window mirrors into the cache. The
    /// check is byte-granular, so odd-address writes update exactly the byte they touch (RR8 open-remainder
    /// 2). The H32 window covers the first 64 entries only (`base+512`); H40 covers all 80 (`base+640`), so
    /// entries 64–79 never refresh in H32 — a faithful R5 detail. Changing reg 5 does **not** invalidate the
    /// cache (there is no other refresh path) — the Bloodlines stale-cache behavior.
    fn write_vram_byte(&mut self, addr: usize, byte: u8) {
        let a = addr & (VRAM_SIZE - 1);
        // Watchpoints v2: the single VRAM byte choke — CPU data-port writes and every DMA byte route here, so
        // capturing here (with `old` read before the store) catches all of them. No-op when disarmed.
        self.capture(
            VdpTarget::Vram,
            a as u32,
            self.vram[a] as u32,
            byte as u32,
            1,
        );
        self.vram[a] = byte;
        let base = self.sat_base();
        let entries = if self.h40() { SAT_SLOTS } else { 64 };
        let off = a.wrapping_sub(base);
        let entry = off / 8;
        let byte_in_entry = off % 8;
        if byte_in_entry < 4 && entry < entries {
            self.sat_cache[entry * 4 + byte_in_entry] = byte;
        }
    }

    /// Write `w` to the data-port target at the current address (recon R1/R3), masked/laid out to match the
    /// stored `state_hash` byte form: VRAM byte-granular with the odd-address byte-swap; CRAM to 9 bits
    /// (`0x0EEE`); VSRAM to 11 bits (`0x07FF`); CRAM/VSRAM big-endian.
    fn write_target(&mut self, w: u16) {
        match self.target() {
            VdpTarget::Vram => {
                // VRAM odd-address byte-swap (recon R3): high byte → `addr`, low byte → `addr ^ 1`, so an
                // odd address swaps the two bytes of the word. (Implementation-time pin, unit-tested below.)
                // Both bytes route through `write_vram_byte` so the SAT-cache write-through sees every byte
                // (recon R5 / RR8; byte-granular — RR8 open-remainder 2, odd-address SAT writes).
                let a = self.addr as usize;
                self.write_vram_byte(a & (VRAM_SIZE - 1), (w >> 8) as u8);
                self.write_vram_byte((a ^ 1) & (VRAM_SIZE - 1), (w & 0xFF) as u8);
            }
            VdpTarget::Cram => {
                let masked = w & 0x0EEE; // 9-bit colour (---- BBB- GGG- RRR-)
                let b = (self.addr as usize) & 0x7E;
                // Watchpoints v2: the CRAM choke — capture the word write (old read before the store).
                let old = ((self.cram[b] as u32) << 8) | self.cram[b | 1] as u32;
                self.capture(VdpTarget::Cram, b as u32, old, masked as u32, 2);
                self.cram[b] = (masked >> 8) as u8;
                self.cram[b | 1] = (masked & 0xFF) as u8;
                // §11.27's write stamp. Unconditional — unlike `capture` above, which is armed — because
                // an instrumented machine and a plain one must stay byte-identical, and because the
                // caveat has to be answerable on a machine nobody armed anything on.
                self.cram_written_mclk[b >> 1] = Some(self.now_mclk);
            }
            VdpTarget::Vsram => {
                // A1: 7-bit address; a write to `$50-$7F` is discarded, so there is nothing to store and
                // nothing for a watch to capture.
                let masked = w & 0x07FF; // 11-bit vertical scroll
                let Some(b) = Self::vsram_byte(self.addr) else {
                    return;
                };
                // Watchpoints v2: the VSRAM choke — capture the word write (old read before the store).
                let old = ((self.vsram[b] as u32) << 8) | self.vsram[b | 1] as u32;
                self.capture(VdpTarget::Vsram, b as u32, old, masked as u32, 2);
                self.vsram[b] = (masked >> 8) as u8;
                self.vsram[b | 1] = (masked & 0xFF) as u8;
            }
        }
    }

    /// Apply one register write (`$8xxx` first control word; recon R1). Registers ≥ 24 are ignored. Turning
    /// on M3 (reg 0 bit 1) freezes the live HV counter into the latch at `mclk` (interim model, recon R2).
    fn write_register(&mut self, reg: usize, val: u8, mclk: u64) {
        if reg >= REG_COUNT {
            return; // n >= 24 ignored
        }
        // Mode-4 register mask (slice T12; VDPFIFOTesting test 12 "Register Write Mode4 Mask", ROM $20C8,
        // expected table $20EC). In Mode 4 — reg 1 bit 2 = M5 CLEAR, the SMS mode — only the eleven SMS
        // registers 0-10 are writable and writes above 10 are discarded: the ROM sets reg 15 = 4 inside a
        // mode-4 window and observes the autoincrement still at its Mode-5 value when mode 5 returns.
        // Kabuto's hardware notes: "All registers except for the 10(?) SMS registers are disabled."
        //
        // EVIDENCE HONESTY: the ROM pins **register 15 only**; the `> 10` boundary is extrapolated from
        // that hedged sentence ("the 10(?) SMS registers"), so masking 11-14 and 16-23 — the DMA registers
        // 19-23 included — is a uniform-rule inference, not a ROM-pinned fact. Registered as follow-up
        // F-M4REGS in docs/2026-07-25-testrom-conformance.md.
        if self.regs[1] & 0x04 == 0 && reg > 10 {
            return;
        }
        let m3_before = reg == 0 && self.regs[0] & 0x02 != 0;
        self.regs[reg] = val;
        if reg == 0 && val & 0x02 != 0 && !m3_before {
            self.hv_latch = ((self.v_counter(mclk) as u16) << 8) | self.h_counter(mclk) as u16;
        }
    }

    /// A control-port write ($C00004/6; recon R1). One of three things depending on the toggle + top bits:
    /// a `$8xxx` register write (top bits `10`, never arms the toggle); a first command word (top bits set
    /// CD1-CD0 + A13-A0 immediately, arm the toggle); or a second command word (CD5-CD2 + A15-A14, disarm).
    pub fn control_write(&mut self, w: u16, mclk: u64) {
        self.now_mclk = mclk; // the write choke points read it from here (slice 1)
        if !self.pending {
            // A first control word ALWAYS latches CD1-CD0 from bits 15-14 — including the `$8xxx` register
            // form, whose bits 15-14 are `10`. CD3-CD0 = `xx10` names no target in the code table, so after
            // a register write the data port is dead until the next command word: genvdp.txt 1.5f, "Writing
            // to a VDP register will clear the code register. Games that rely on this are Golden Axe II …
            // and Sonic 3D." CD5-CD2 are *retained*, so this is not a full clear — VDPFIFOTesting test 13
            // (ROM $22FA words 8-11) writes a register, then a first-half-only word, and the writes still
            // land on the previously latched VSRAM target. A13-A0 is left alone on the register form: the
            // ROM never observes it, and MacDonald records the address side as unknown.
            self.code = (self.code & 0x3C) | ((w >> 14) & 0x03) as u8;
            if (w >> 14) & 0x03 == 0b10 {
                // Register write: reg = bits 12..8, value = bits 7..0. Does not arm the toggle.
                self.write_register(((w >> 8) & 0x1F) as usize, (w & 0xFF) as u8, mclk);
                return;
            }
            // First command word: apply the low half (CD1-CD0 + A13-A0) to the live registers immediately.
            self.addr = (self.addr & 0xC000) | (w & 0x3FFF);
            self.pending = true;
        } else {
            // Second command word: apply the high half (CD5-CD2 + A15-A14), disarm.
            self.addr = (self.addr & 0x3FFF) | ((w & 0x03) << 14);
            let cd_hi = ((w >> 4) & 0x0F) as u8; // CD5..CD2
            if self.regs[1] & 0x10 != 0 {
                self.code = (self.code & 0x03) | (cd_hi << 2);
            } else {
                // CD5 can only change while DMA-enable (reg 1 bit 4) is set (recon R1); else it is retained.
                self.code = (self.code & 0x23) | ((cd_hi << 2) & 0x1C);
            }
            self.pending = false;
            // A completed read command pre-fills the read buffer from the set address (recon R3 pre-cache).
            if self.code & 0x01 == 0 {
                self.read_buffer = self.read_target();
            }
            // A completed CD5 (DMA) command arms the transfer (recon R4 / RD2/RD4). The mode is reg 23's top
            // bits: Mem (bit 7 = 0) and Copy (11) trigger on this control write; Fill (10) triggers on the next
            // data-port write (its value is the fill data), so it is armed there.
            if self.code & 0x20 != 0 {
                self.arm_dma();
            }
        }
    }

    /// Decode + arm the pending DMA from the live registers (recon R4 / RD1–RD4). Called when a CD5 command
    /// completes (Mem/Copy) — Fill is armed on its data-port trigger instead.
    fn arm_dma(&mut self) {
        let len = ((self.regs[0x14] as u16) << 8) | self.regs[0x13] as u16;
        match self.regs[0x17] & 0xC0 {
            0x80 => {} // Fill: armed by the data-port trigger, not here
            0xC0 => {
                // Copy: source = low 16 bits of the source registers (a VRAM byte address), len bytes.
                let source = ((self.regs[0x16] as u16) << 8) | self.regs[0x15] as u16;
                self.dma_pending = Some(DmaRequest::Copy { source, len });
            }
            _ => {
                // Mem (bit 7 = 0): source = 68k WORD address << 1 (RD3), len words.
                let source = (((self.regs[0x17] as u32 & 0x7F) << 16)
                    | ((self.regs[0x16] as u32) << 8)
                    | self.regs[0x15] as u32)
                    << 1;
                self.dma_pending = Some(DmaRequest::Mem { source, len });
            }
        }
    }

    /// Take the pending DMA (recon R4) — the bus calls this after each control/data write to execute it.
    /// Consuming a request clears CD5 (code bit 5, "DMA work pending"): the hardware DMA engine clears it on
    /// completion (recon V2), and oracle-next runs the transfer synchronously, so consume == complete. Without
    /// this, CD5 goes stale and a later M1=0 command retains it (recon R1/V1) and re-fires a phantom DMA — the
    /// DR-2 spurious 65536-word transfer. Guarded on an actual take: a non-DMA VDP access must not touch CD5.
    pub fn take_dma_request(&mut self) -> Option<DmaRequest> {
        let req = self.dma_pending.take();
        if req.is_some() {
            self.code &= !0x20;
        }
        req
    }

    /// The live data-port target address (recon R1) — the DMA destination, exposed for the `DmaRecord`.
    pub fn dma_dest(&self) -> u16 {
        self.addr
    }

    /// The live data-port target region (recon R1) — exposed for the `DmaRecord` / introspection.
    pub fn dma_target(&self) -> VdpTarget {
        self.target()
    }

    /// The most recently completed DMA (recon R4; `frame_report`). `None` until the first transfer.
    pub fn last_dma(&self) -> Option<DmaRecord> {
        self.last_dma
    }

    /// The master clock the VDP last performed work at — see the [`now_mclk`](Vdp#structfield.now_mclk)
    /// field for what "last" means at each entry point and for the two granularity limits it inherits.
    pub fn now_mclk(&self) -> u64 {
        self.now_mclk
    }

    /// Arm or disarm the VDP-write capture buffer (watchpoints v2). The sink-generic run loop arms it for a
    /// run whose sink wants VDP writes and disarms it after; disarmed is byte-for-byte the old hot path.
    pub fn set_write_capture(&mut self, on: bool) {
        self.capture_armed = on;
        self.capture_cram_only = false;
    }

    /// Arm the write-capture buffer for **CRAM writes only** — what a run with a scanline sink but no VDP
    /// watch needs (`F-SCANLINE-SUBLINE`). See [`Vdp::capture_cram_only`] for why the narrowing matters.
    pub fn set_write_capture_cram_only(&mut self, on: bool) {
        self.capture_armed = on;
        self.capture_cram_only = on;
    }

    /// Drain the VDP writes captured since the last drain (watchpoints v2), leaving the buffer empty. The
    /// system calls this after every `step_cpu`, so the buffer is empty at every instruction boundary.
    pub fn take_write_captures(&mut self) -> Vec<VdpWrite> {
        std::mem::take(&mut self.write_captures)
    }

    /// Record one captured write **iff the buffer is armed** (watchpoints v2). The single `capture_armed`
    /// branch is the whole cost when disarmed — it inlines to one predictable test in the DMA-fill hot path.
    #[inline]
    fn capture(&mut self, target: VdpTarget, addr: u32, old: u32, new: u32, size: u8) {
        if self.capture_armed && (!self.capture_cram_only || target == VdpTarget::Cram) {
            let via = if self.in_dma {
                VdpVia::Dma
            } else {
                VdpVia::Direct
            };
            self.write_captures.push(VdpWrite {
                target,
                addr,
                old,
                new,
                size,
                via,
                // The write's own instant, not the draining step's — F-TRACE-VDPWRITE-MCLK (slice 1b).
                mclk: self.now_mclk,
            });
        }
    }

    /// Feed one DMA word to the current data-port target (68k→VDP transfer, recon R4(a)): store it into the
    /// physical FIFO ring, route it to VRAM/CRAM/VSRAM through `write_target` (so the R5 SAT write-through
    /// fires for VRAM — "any DMA that writes VRAM counts") and autoinc. The bus reads the source word from
    /// 68k memory and calls this.
    ///
    /// **A3a / P1** — the ring store: a DMA payload word occupies a real FIFO slot, exactly like a CPU
    /// data-port write. Nemesis, *VDP Internals* (SpritesMind): a DMA "will read a value from external memory
    /// using the DMA source address register and **add it to the FIFO** using the current command code and
    /// incremented command address registers". Corroborated by Kabuto's hardware notes: "When writing a value
    /// to the VDP's data port (or the VDP does that internally through DMA) both value and current address are
    /// appended to its internal FIFO." Observable through the CRAM/VSRAM undefined-bit snoop — VDPFIFOTesting
    /// test 3 (expected table ROM `$5E0C`) reads it back and is the acceptance test.
    ///
    /// **T16/S2 — `fifo_enqueue`, so the word is *pending*, not just resident in the ring.** A3a used the
    /// bare `fifo_store` deliberately, on the reasoning that our synchronous Mem DMA bills its whole elapsed
    /// time through `dma_cost` + the returned halt wait, so pending entries would be phantoms no clock had
    /// advanced past — a spurious /DTACK stall on the next data-port write in every DMA-using ROM. That was
    /// the right call **at the time** and was recorded as open question Q1 of
    /// `docs/2026-08-03-a3-dma-fifo-design.md`, to be revisited here. It is now answered against, by the ROM.
    ///
    /// The reasoning no longer applies because [`Vdp::dma_complete`] anchors the drain clock at the
    /// transfer's end instant: the surviving entries are **not** phantoms, they are the (up to four) words
    /// that genuinely have not reached VRAM yet, and the stall they produce is the real one. Physically the
    /// DMA unit's job ends when the last word is pushed *into the FIFO*, not when it reaches VRAM — the same
    /// Nemesis sentence quoted above, "add it to the FIFO", is the pin. VDPFIFOTesting test 16 groups 9 and
    /// 10 (expected table ROM `$ED10` words 33-40, `0200 0100 0000 0200` / `0000 0100 0000 0200`) fire a
    /// DMA between their two probes and require the resuming 68k to see **FULL → partial → EMPTY**; with a
    /// bare ring store it sees EMPTY throughout and both groups report `$ffff`.
    ///
    /// `now` is the **transfer's** instant, not this word's: the bus passes the same value for every word of
    /// the burst (decision C-6 — a 32-word palette DMA is located at the pixel the DMA started on, not
    /// smeared across the slots it really occupies; follow-up F-SUBLINE-DMASPREAD). It is a parameter rather
    /// than a shadow set once around the loop because this is a **public** entry point the bus drives
    /// directly, bypassing every timed one — a caller that has to supply the time cannot forget to.
    pub fn dma_write_word(&mut self, w: u16, now: u64) {
        self.now_mclk = now; // C-6: the transfer's instant, per word (slice 1)
        self.in_dma = true; // watchpoints v2: this word's captures attribute to the triggering DMA
        self.fifo_enqueue(w);
        self.write_target(w);
        self.in_dma = false;
        self.autoinc();
    }

    /// The coarse mclk cost of a DMA (recon R4(e) / R3): `slots × MCLK_PER_LINE / slots_per_line`, using the
    /// slot rate at the transfer's start instant. Integer.
    ///
    /// Deliberately still the flat rate, and the **one** part of the slot model T16 left untouched: a
    /// transfer that starts in vblank and runs into active display is billed entirely at the vblank rate,
    /// mis-costing it by up to ~11×. That is the long-standing "Phase 3 per-line DMA cost" deferral (recon
    /// §3.4 "S3"). T16 does not need it, and unlike T16/S1 — which redistributes drains *within* a line and
    /// leaves the per-line total identical — integrating this rate would change DMA elapsed time for every
    /// DMA-using ROM, which is where the visual-baseline risk actually lives.
    pub fn dma_cost(&self, slots: u64, start: u64) -> u64 {
        slots * MCLK_PER_LINE / self.slots_per_line(start)
    }

    /// Record a completed DMA + advance the length/source registers to their post-transfer state (recon R4:
    /// regs 19–23 mutate during a transfer — length → 0, source advanced; visible in both currencies) and
    /// open the DMA-busy window to `busy_until`.
    ///
    /// **T16/S2** — it also anchors the FIFO drain clock at `busy_until` while entries are still pending.
    /// Our Mem DMA runs synchronously inside the triggering bus access, so without this the entries
    /// [`Vdp::dma_write_word`] left pending would be measured against a slot clock still sitting at the
    /// transfer's *start*, and would appear to have drained the instant the 68k resumed. Anchoring at the
    /// end instant is what makes those entries real rather than phantom, and it is what VDPFIFOTesting test
    /// 16 groups 9/10 observe: the residual drains **from the end of the transfer**, so the resuming CPU
    /// sees FULL, then partial, then EMPTY (ROM `$ED10` words 33-40).
    ///
    /// The re-anchor is a **`max`**, so the clock never runs backwards. `fifo_slot_clock` is a *time*, not a
    /// ring cursor, and it is routinely **ahead of** the caller's `now`: a /DTACK stall in `data_write_at` /
    /// `data_read_at` advances it to the drain instant it waited for, which is by construction later than
    /// the access that stalled. The invariant that keeps `busy_until >= fifo_slot_clock` in practice is a
    /// property of the *bus*, not of this function — every stall those paths return is folded into
    /// `now_mclk` before the next bus access, and `now_mclk` is instruction-start-granular, so a later
    /// access always starts at or after the stall it was charged. A single instruction touching both
    /// `$C00000` and `$C00004` would break that, which is why this takes the max rather than trusting it.
    /// (Measured: 0 occurrences across all 16 conformance ROMs; reproducible in
    /// `bus::tests::vdpfifo_t3_dma_payload_walks_the_fifo_ring`, which drives ports directly at a fixed
    /// mclk and so does not fold stalls at all.)
    ///
    /// The total halt is deliberately left at `count × slots × rate` even though up to four of those words
    /// are now also accounted as pending — physically the DMA unit should release the bus about four slots
    /// earlier. The ROM cannot see the difference (it measures FIFO state, not transfer duration) and
    /// shortening the halt would change DMA timing for every ROM; registered as follow-up **F-DMAHALT**.
    pub fn dma_complete(&mut self, record: DmaRecord, end_source_words: u32, busy_until: u64) {
        if self.fifo_len > 0 {
            // The drain clock is monotonic. It holds for every bus-driven caller (see above); the `max` is
            // what makes it hold unconditionally, and the assert is what would make a violation loud in a
            // debug build rather than silently shortening a drain.
            debug_assert!(
                busy_until >= self.fifo_slot_clock,
                "DMA ended at {busy_until} but the FIFO drain clock is already at {}: a bus access started \
                 before a stall it was charged for",
                self.fifo_slot_clock
            );
            self.fifo_slot_clock = self.fifo_slot_clock.max(busy_until);
        }
        self.regs[0x13] = 0;
        self.regs[0x14] = 0;
        // Advance the source registers (Mem: the 68k word address; the low 23 bits, reg 23 keeps its mode bit).
        self.regs[0x15] = (end_source_words & 0xFF) as u8;
        self.regs[0x16] = ((end_source_words >> 8) & 0xFF) as u8;
        self.regs[0x17] = (self.regs[0x17] & 0x80) | ((end_source_words >> 16) & 0x7F) as u8;
        self.last_dma = Some(record);
        self.dma_busy_until = busy_until;
    }

    /// A control-port (status) read ($C00004/6; recon R2 timing bits). **Clears the pending toggle** — the
    /// instrument-sourced experiment pin (permitted docs are silent; see the pending-toggle experiment in
    /// `docs/2026-07-16-vdp-recon.md`, same standing as the STOP×trace pin). Also **clears the sprite-overflow
    /// and collision status latches** (Sega Genesis Software Manual — those two flags are cleared by reading
    /// the status). Golden-safe: the frozen currencies do not include these fields, and the test ROM drives no
    /// sprite rendering, so both are `false` here regardless.
    ///
    /// K4-5: the VDP drives only the low 10 status lines (`StatusRegisterMask = 0x03FF`,
    /// `S315-5313_Ports.cpp:1163-1170`); bits 10-15 float — the caller passes the open-bus residue
    /// (`open_bus`, same plumbing pattern as [`Vdp::data_read_at`]) and gets it merged into the upper 6
    /// bits (memtest row 11: residue `4E71` + status `$288` → `4E88`). Internal/Z80-mirror callers pass 0
    /// (behavior-identical to pre-K4-5 for them).
    pub fn control_read_status(&mut self, open_bus: u16, mclk: u64) -> u16 {
        self.pending = false;
        // Advance the time-based FIFO drain to `mclk` first so the live EMPTY/FULL bits (A1, T16) reflect
        // the FIFO's occupancy *now* — a status read never pops entries beyond this normal drain.
        self.fifo_drain(mclk);
        let s = self.status_word(mclk);
        self.sprite_overflow = false;
        self.sprite_collision = false;
        (s & 0x03FF) | (open_bus & 0xFC00)
    }

    /// A data-port write ($C00000/2; recon R1). Clears the toggle, routes the word to VRAM/CRAM/VSRAM, and
    /// auto-increments. A CD5 **Mem/Copy** command latches state and does nothing here (those transfers
    /// trigger on the control write); a CD5 **fill** completes its trigger word as an ordinary write and
    /// auto-increments before arming the transfer (A3b / P2 — see [`Vdp::apply_data_write`]). The toggle
    /// clears either way.
    pub fn data_write(&mut self, w: u16) {
        self.apply_data_write(w);
    }

    /// The write body shared by the untimed [`Vdp::data_write`] and the bus-timed [`Vdp::data_write_at`]:
    /// clear the toggle, enqueue into the FIFO (recon R3, contents + timing) then apply the write
    /// (enqueue-immediate model — see the `fifo` field docs; the enqueue captures the pre-autoincrement
    /// address, per the pin). A CD5 **fill** trigger takes the same enqueue + write + autoincrement path
    /// and then arms the transfer (A3b / P2); a CD5 Mem/Copy command falls straight through.
    fn apply_data_write(&mut self, w: u16) {
        self.pending = false;
        if self.code & 0x20 != 0 {
            // CD5 DMA data write. For a VRAM fill (reg 23 bits 7-6 = 10) this word is the fill trigger: enqueue
            // it (so the FIFO's last/next-available entry holds it, recon R4(b)) and arm the fill; the bus
            // executes it. Mem/Copy do not use a data-port write (they trigger on the control write).
            if self.regs[0x17] & 0xC0 == 0x80 {
                self.fifo_enqueue(w);
                let len = ((self.regs[0x14] as u16) << 8) | self.regs[0x13] as u16;
                // A3b / P2: the trigger is NOT swallowed — it is completed as an ordinary data-port write
                // before the fill engine runs, and the address then auto-increments, so the fill's first
                // replicated byte lands back on the start address. Nemesis, *VDP Internals* (SpritesMind):
                // "When a DMA Fill operation is pending, and you perform a data port write, that data port
                // write is completed as normal… That pending write is then pulled out of the FIFO, and
                // processed as a normal FIFO write." Observed by VDPFIFOTesting test 4 (expected table ROM
                // $DC54): only a full word write puts the trigger's LSB $34 at $8001, and only the
                // auto-increment keeps the fill's first step from destroying it again.
                //
                // Same invalid-target guard as the non-DMA path below (one shared predicate, so the
                // valid-code set can only be changed in one place): a code whose low nibble names no write
                // target accepts the word into the FIFO and steps the address, but the *trigger* reaches no
                // memory. Since A5 the fill *body* takes the same guard (`run_fill`), so a no-write-target
                // fill runs and writes nowhere — follow-up **F-FILLTGT** retired 2026-09-12 by
                // VDPFIFOTesting test 34 group 3.
                //
                // The guard admits all three write targets, so a CRAM/VSRAM fill (code `$23`/`$25`) is
                // primed too. That is EXTRAPOLATED: the ROM's pin is VRAM-only, and Nemesis's "completed as
                // normal" is generic. Ruled 2026-08-03 — apply the pinned rule uniformly rather than invent
                // an equally unevidenced VRAM-only exception; tracked as follow-up **F-FILLPRIME** (same
                // ledger), and pinned as-shipped by `fill_trigger_primes_a_cram_fill_target` below.
                if Self::code_names_a_write_target(self.code) {
                    self.write_target(w);
                }
                self.autoinc();
                self.dma_pending = Some(DmaRequest::Fill { len, fill: w });
            }
            return;
        }
        self.fifo_enqueue(w);
        // Undefined codes name no write target and are dropped — see `code_names_a_write_target` for the
        // rule, its citation (genvdp.txt 1.5f) and the VDPFIFOTesting test 10 case that pins it.
        if Self::code_names_a_write_target(self.code) {
            self.write_target(w);
        }
        self.autoinc();
    }

    /// Execute a VRAM fill (recon R4(b) / RD2). 68k keeps running (the bus returns no wait); the busy window
    /// models the elapsed transfer time. Fill data source: the top byte of the trigger word for VRAM; the
    /// **next-available FIFO entry** ("4 writes ago") for CRAM/VSRAM — the documented hardware bug. Every write
    /// routes through the SAT write-through (R5 rider: fill steps hit the window compare like any VRAM write).
    /// Length is in bytes (RD2); regs 19/20 → 0 after the transfer (recon R4), and source regs 21/22 advance by
    /// one per step even though a fill never reads its source ([`Vdp::advance_dma_source_low16`], A3).
    ///
    /// **A5 / FILL-TGT: the body shares the data-port write decode.** If the live code's low nibble names no
    /// write target ([`Vdp::code_names_a_write_target`]) the fill **still runs** — it walks its address, counts
    /// its length down to 0, advances source registers 21/22 and opens its busy window — but no byte reaches
    /// memory. This closes the asymmetry booked as follow-up **F-FILLTGT**, where the trigger write was
    /// guarded and the body was not (the body resolved through [`Vdp::target_of`]'s `_ => Vram` fallback and
    /// wrote VRAM anyway).
    ///
    /// VDPFIFOTesting **test 34** "DMA Fill Control Port Writes" (ROM `$45B2`) group 3 (`$4898..$4948`) is the
    /// table. It arms a 4-byte fill with `$40020082` (code `$21`, VRAM `$8002`), then writes register `$8F02`
    /// — and a register write replaces CD1-CD0 with `10` (genvdp.txt 1.5f; VDPFIFOTesting test 13), leaving
    /// code `$22`, which names no write target. Then comes the `$68AC` trigger at `$48EE`. On hardware
    /// `$8000-$800F` reads back **completely unchanged** (`1122 3344 5566 7788 99aa bbcc ddee ff00`); before
    /// this fix we wrote `5568 7768 9968 bb68` into it, the four `$68` bytes of a fill that ran to VRAM.
    ///
    /// **Not an early return.** The hardware fill ran: group 4 (`$4954`) sets no length and fills well past
    /// 16 bytes, which is only possible if group 3 left the length counter at 0 — i.e. it counted 4 steps down
    /// and then group 4's 0 meant 65,536. So the length, the source-register advance (A3, tests 28 and 29) and
    /// the busy window all still happen; only the write is dropped. Group 3 also *depends* on the busy window:
    /// its `btst #1` poll at `$48F6` spins until the fill reports done.
    pub fn run_fill(&mut self, len: u16, fill: u16, now: u64) {
        self.now_mclk = now; // C-6: every step of the fill carries the transfer's own instant (slice 1)
        let count = if len == 0 { 0x1_0000u32 } else { len as u32 };
        let target = self.target();
        // A5 / F-FILLTGT: one shared decode with the port write. False → the engine runs and writes nowhere.
        let writes = Self::code_names_a_write_target(self.code);
        // The address register the fill engine starts from. Since A3b this is the *post-trigger* value
        // (the trigger's autoincrement has already run), i.e. one step past the armed command address —
        // which is what the engine actually walks. Introspection only (`last_dma`); in neither currency.
        let dest = self.addr;
        self.in_dma = true; // watchpoints v2: fill writes attribute to the triggering DMA
        match target {
            VdpTarget::Vram => {
                let byte = (fill >> 8) as u8; // top byte (recon R4(b))
                for _ in 0..count {
                    // A3b / P3: a VRAM *byte* write from the fill engine lands at `address ^ 1`, not at
                    // `address`. Mask of Destiny, *Is DMA Fill buggy?* (SpritesMind): "MSB of the word in
                    // the FIFO is written DMA length times to address ^ 1"; Eke, same thread: "VRAM byte
                    // writes (used by VRAM fill and copy DMA) actually occur to VRAM address ^ 1 so you can
                    // get unexpected results depending on start address, DMA length and increment
                    // alignments." With an odd autoincrement this produces the characteristic interleave —
                    // a skipped byte at the tail and one byte written past the naive end — that
                    // VDPFIFOTesting test 4 checks (expected table ROM $DC54). `run_copy` takes the same lane
                    // swap on its read AND its write (F-COPYXOR, closed 2026-09-12 by the same ROM's tests 26
                    // and 96-122; see `Vdp::run_copy`).
                    if writes {
                        self.write_vram_byte((self.addr ^ 1) as usize & (VRAM_SIZE - 1), byte);
                    }
                    self.autoinc();
                }
            }
            _ => {
                // CRAM/VSRAM fill: the data comes from the next-available FIFO entry, NOT the trigger word
                // (recon R4(b), "4 writes ago" — a documented hardware bug).
                let src = self.fifo_snoop_word();
                for _ in 0..count {
                    if writes {
                        self.write_target(src);
                    }
                    self.autoinc();
                }
            }
        }
        self.in_dma = false;
        let cost = self.dma_cost(count as u64, now); // fill ≈ 1 slot/byte (recon R4(e))
        self.regs[0x13] = 0;
        self.regs[0x14] = 0;
        // A3: the source counter steps with the fill although nothing is read from it (VDPFIFOTesting test 28).
        let start = ((self.regs[0x16] as u16) << 8) | self.regs[0x15] as u16;
        self.advance_dma_source_low16(start, count);
        self.last_dma = Some(DmaRecord {
            mode: DmaMode::Fill,
            source: 0,
            dest,
            len,
            target,
        });
        self.dma_busy_until = now + cost;
    }

    /// Execute a VRAM copy (recon R4(c) / RD2): `len` byte read+write steps within VRAM from `source` to the
    /// live address, **bypassing the FIFO**, at half the fill byte rate (one byte read + one byte write per
    /// step = 2 slots/byte). 68k keeps running (the bus returns no wait); the busy window models the elapsed
    /// time. Each write routes through the SAT write-through (R5 rider). Length is in bytes (RD2); regs 19/20
    /// → 0 after the transfer (recon R4), and source regs 21/22 advance by one per step
    /// ([`Vdp::advance_dma_source_low16`], A3).
    ///
    /// **Both byte accesses take the opposite byte lane (F-COPYXOR, lens M22).** Step `i` reads
    /// `vram[(source + i) ^ 1]` and writes that byte to `address ^ 1`; the address then advances by reg 15.
    /// Pinned by VDPFIFOTesting's own hardware tables (`vendor/TestRoms/vdp_port_access.bin`): test 26 "DMA
    /// Copy Length Reg Update" (table ROM `$9B24`) and the destination images of the copy matrix, tests
    /// 96-122 (tables from ROM `$E4BE`) — `conformance_roms::vdp_port_access_copy_dma_matches_the_roms_own_tables`.
    /// Only this model matches them: the read half alone, the write half alone, and neither all fail. The
    /// hardware prose agrees: Eke, SpritesMind *VDP Internals* (t=1291, p=21334), "on VRAM copy, VRAM source
    /// and destination address are actually adjacent address ( address ^ 1) to internal address registers
    /// value"; Kabuto's hardware notes (Plutiedev mirror), "the internal byte order of the VDP is the
    /// opposite of what the 68K sees". Because each two-step pair swaps lanes on both sides, an even-aligned
    /// copy (even source, even destination, even length, autoincrement 1) leaves exactly the image a plain
    /// byte copy would; the models part only at an odd source, destination or length, or an autoincrement
    /// other than 1 (2 included). The fill engine's write takes the same lane swap (`run_fill`, VRAM arm).
    pub fn run_copy(&mut self, source: u16, len: u16, now: u64) {
        self.now_mclk = now; // C-6: every step of the copy carries the transfer's own instant (slice 1)
        let count = if len == 0 { 0x1_0000u32 } else { len as u32 };
        let dest = self.addr;
        let mut src = source as usize;
        self.in_dma = true; // watchpoints v2: copy writes attribute to the triggering DMA
        for _ in 0..count {
            // F-COPYXOR: the READ half — the source byte comes from the opposite lane of `src`.
            let byte = self.vram[(src ^ 1) & (VRAM_SIZE - 1)];
            // F-COPYXOR: the WRITE half — the byte lands in the opposite lane of the live address.
            self.write_vram_byte((self.addr ^ 1) as usize & (VRAM_SIZE - 1), byte);
            src = src.wrapping_add(1);
            self.autoinc();
        }
        self.in_dma = false;
        let cost = self.dma_cost(count as u64 * 2, now); // half the fill byte rate (recon R4(c))
        self.regs[0x13] = 0;
        self.regs[0x14] = 0;
        // A3: `source` is registers 21/22 as `arm_dma` read them, and the copy walked them one byte per step,
        // so they end at the byte after the last one read (VDPFIFOTesting test 29).
        self.advance_dma_source_low16(source, count);
        self.last_dma = Some(DmaRecord {
            mode: DmaMode::Copy,
            source: source as u32,
            dest,
            len,
            target: VdpTarget::Vram,
        });
        self.dma_busy_until = now + cost;
    }

    /// **A3 / DMA-SRC-ADVANCE.** Leave source registers 21 (low) and 22 (middle) where a fill or copy of `steps`
    /// steps that started from `start` leaves them. Nemesis, *VDP Internals* p.4: "Every DMA operation also
    /// performs the exact same set of steps after it is advanced one step, which is to firstly add 1 to the
    /// lower 2 DMA source address registers, then to subtract 1 from the DMA length counter register". Fill and
    /// copy included, although a fill never reads its source.
    ///
    /// * **One step per length unit**, so the advance is the length in bytes. VDPFIFOTesting tests 28 (fill)
    ///   and 29 (copy) pin it: a 4-byte operation from `$00FA` leaves register 21 at `$FE`.
    /// * **21 carries into 22.** The same tests' third group runs from `$00FE` and reads its table from ROM
    ///   `$403FC`, i.e. registers 22:21 = `$0102`.
    /// * **Register 23 never takes the carry**, so the counter wraps at 16 bits and 23 keeps its mode bits.
    ///   "The lower 2" in the quote, and the MegaDrive Wiki's "only the low and middle bytes of the DMA source
    ///   registers are incremented". The ROM cannot reach this case for a fill or copy (its follow-up DMA
    ///   rewrites 23); it is the rule, not a table.
    /// * **A length of 0 is 65,536 steps** (RD2), one whole turn of the counter: 21/22 end where they started.
    ///
    /// Machine state: registers are in both currencies, so a ROM that fills or copies moves them. What that did
    /// to the frozen goldens is recorded under A3 in `docs/2026-09-12-vdp-port-access-full-rom.md`.
    fn advance_dma_source_low16(&mut self, start: u16, steps: u32) {
        let end = ((start as u32 + steps) & 0xFFFF) as u16; // 16 bits: no carry into register 23
        self.regs[0x15] = (end & 0xFF) as u8;
        self.regs[0x16] = (end >> 8) as u8;
    }

    /// A bus-timed data-port write (recon R1/R3): drain the FIFO up to `now`, stall the 68k if the FIFO is
    /// full (returning the wait in **CPU cycles** for the `Bus68k` channel — `MegaDriveBus` folds it into the
    /// instruction cost; `FlatBus` never reaches this path so the SST corpus is untouched), then apply the
    /// write. A 5th write while all 4 slots are pending waits for the oldest to drain (official /DTACK stall).
    pub fn data_write_at(&mut self, w: u16, now: u64) -> u32 {
        // Slice 1: the write choke points read the instant from here.
        self.now_mclk = now;
        // A DMA command's data write is not FIFO-timed here (the DMA slices own it); no stall, apply as before.
        if self.code & 0x20 != 0 {
            self.apply_data_write(w);
            return 0;
        }
        self.fifo_drain(now);
        let mut wait_mclk = 0u64;
        if self.fifo_len == 4 {
            let oldest = self.fifo_oldest();
            let cost = self.entry_drain_cost(oldest.code, self.fifo_slot_clock);
            let drain_at = self.fifo_slot_clock + cost;
            // C2: bill from where the CPU has ALREADY been held to, not from the caller's `now`. `now` is
            // frozen for a whole 68000 instruction (`MegaDriveBus::now_mclk` is taken by value; only
            // `System::run_until` advances the clock, by the retiring instruction's total cost), while
            // `fifo_slot_clock` advances one drain per stalling write — so a burst of data-port writes
            // inside one instruction (`movem.l dN-dM,(a6)` at `$C00000`, the common shape) hands every
            // write the same origin, and measuring `drain_at - now` re-charges write *k* for every stall
            // writes 1..k-1 were already billed. The bus SUMS these waits into one instruction cost
            // (`MegaDriveBus::stall_cycles`), so the burst was billed roughly quadratically in its length.
            //
            // `fifo_slot_clock.max(now)` is exactly the already-charged mark, with no extra state to carry
            // and nothing new in the snapshot: the clock is only ever ahead of `now` because the CPU was
            // already held that far. `fifo_drain` above leaves it at or before `now` in every other case
            // (it breaks with `slot_clock + cost > now`, and coasts to `max(now)` when it empties), and the
            // one other thing that can push it past `now` — `dma_complete`'s T16/S2 re-anchor to a
            // transfer's end instant — is a window `MegaDriveBus::run_mem_dma` bills to this same
            // instruction in full. See `a_burst_of_stalling_writes_at_one_instant_bills_each_drain_once`.
            wait_mclk = drain_at.saturating_sub(self.fifo_slot_clock.max(now));
            self.fifo_slot_clock = drain_at;
            self.fifo_len -= 1;
        }
        self.apply_data_write(w);
        (wait_mclk.div_ceil(crate::system::MCLK_PER_CPU_CYCLE)) as u32
    }

    /// A data-port read ($C00000/2; recon R1/R3). Clears the toggle, returns the pre-cached buffer, refills
    /// it from the (post-increment) address, and auto-increments.
    ///
    /// **Lockup cell (recon R1):** a data read while a *write* command is armed (CD0 = 1) hangs real
    /// hardware ("setup a write and then try to read → the 68K will hang until reset"). We return `open_bus`
    /// and latch [`Vdp::latched_fault`] instead of hanging the host — a deliberate divergence recorded in the
    /// divergence ledger (hardware hangs; we must stay debuggable).
    pub fn data_read(&mut self, open_bus: u16) -> u16 {
        self.pending = false;
        if self.code & 0x01 != 0 {
            self.latched_fault = true;
            return open_bus;
        }
        let mut out = self.read_buffer;
        // Snoop quirk (recon R3): a data-port read whose target does not define all sixteen result bits fills
        // the UNDEFINED ones from the next-available FIFO entry (the word written 4 writes ago). THREE codes
        // snoop: CRAM read $08 (above the 9-bit mask), VSRAM read $04 (above the 11-bit mask), and — since A4
        // — the undocumented 8-bit VRAM read $0C (its whole high byte). The *plain* VRAM read $00 returns a
        // full 16-bit word and is the one read target that does NOT snoop. Behavioral, currency-safe
        // (rendering + the hashed currencies read the stored bytes directly, never through `data_read`).
        match self.target() {
            VdpTarget::Cram => out = (out & 0x0EEE) | (self.fifo_snoop_word() & !0x0EEE),
            VdpTarget::Vsram => out = (out & 0x07FF) | (self.fifo_snoop_word() & !0x07FF),
            // A4: the undocumented 8-bit VRAM read (code $0C) is the third snooping target. Only its LOW
            // byte is defined — the pre-cache put `vram[address ^ 1]` there — so the whole HIGH byte is
            // undefined and reads back the next-available FIFO entry's high byte, MASKING AWAY the real
            // VRAM byte the pre-cache also holds (see `read_target`: that byte is kept, not fabricated, so
            // this arm not firing degrades to the plain-VRAM-read result rather than to a made-up zero).
            // VDPFIFOTesting test 6 (expected table ROM $DED4) pins it: with the ring holding the eight
            // marker words' last four, the high byte walks $99 → $BB → $DD → $12 as one CRAM write per
            // group advances the cursor, while both reads *within* a group return the same high byte — a
            // read does not advance it.
            VdpTarget::Vram if Self::is_vram_byte_read(self.code) => {
                out = (out & 0x00FF) | (self.fifo_snoop_word() & 0xFF00)
            }
            VdpTarget::Vram => {}
        }
        self.autoinc();
        self.read_buffer = self.read_target();
        out
    }

    /// A bus-timed data-port read (recon R1/R3): a read waits for the write FIFO to drain first (pending
    /// writes take priority over reads), then returns the pre-cached word (with the snoop merge that CRAM
    /// read `$08`, VSRAM read `$04` and the 8-bit VRAM read `$0C` each apply — see [`Vdp::data_read`]).
    /// Returns the value plus the CPU wait cycles for the `Bus68k` channel (`FlatBus` never reaches here → the
    /// SST corpus is untouched).
    pub fn data_read_at(&mut self, open_bus: u16, now: u64) -> (u16, u32) {
        self.fifo_drain(now);
        let mut wait_mclk = 0u64;
        // C2, same rule as `data_write_at`: measure from the already-charged mark, not from the caller's
        // frozen `now`. Within one call this loop could never double-count (it takes ONE final difference
        // after draining everything) — which is exactly why the asymmetry with the write path went unseen —
        // but across two port accesses of the SAME instruction it could, so both paths now read the mark.
        // Latched before the loop, because the loop is what moves the clock.
        let charged_to = self.fifo_slot_clock.max(now);
        // Reads wait for the write FIFO to empty (recon R3): drain every pending entry, banking the elapsed
        // time as the read's stall.
        while self.fifo_len > 0 {
            let oldest = self.fifo_oldest();
            let cost = self.entry_drain_cost(oldest.code, self.fifo_slot_clock);
            self.fifo_slot_clock += cost;
            self.fifo_len -= 1;
            wait_mclk = self.fifo_slot_clock.saturating_sub(charged_to);
        }
        let out = self.data_read(open_bus);
        (
            out,
            wait_mclk.div_ceil(crate::system::MCLK_PER_CPU_CYCLE) as u32,
        )
    }

    // --- Interrupts + the IPL-deassert path (recon R7/R12) ------------------------------------------------

    /// The combinational interrupt level driven at /IPL0-2 (recon R12): level 6 when VINT is pending AND
    /// enabled (IE0 = reg 1 bit 5); else level 4 when HINT is pending AND enabled (IE1 = reg 0 bit 4); else 0.
    /// The System recomputes `cpu.set_ipl(vdp.ipl())` after every event and every CPU step.
    pub fn ipl(&self) -> u8 {
        if self.vint_pending && self.regs[1] & 0x20 != 0 {
            6
        } else if self.hint_pending && self.regs[0] & 0x10 != 0 {
            4
        } else {
            0
        }
    }

    /// The VINT pending latch (introspection / debuggers; recon R12).
    pub fn vint_pending(&self) -> bool {
        self.vint_pending
    }

    /// The HINT pending latch (introspection / debuggers; recon R12).
    pub fn hint_pending(&self) -> bool {
        self.hint_pending
    }

    /// The 68k interrupt-acknowledge (recon R12): clear exactly the acknowledged level's pending latch — the
    /// **only** thing that clears these latches. Driven by the fc=7 /INTAK bus cycle in `MegaDriveBus`.
    pub fn acknowledge(&mut self, level: u8) {
        match level {
            6 => self.vint_pending = false,
            4 => self.hint_pending = false,
            _ => {}
        }
    }

    /// Whether an interlace mode is active: reg $0C bit 1 (LSM0) — set in both interlace mode 1 (LSM = 01)
    /// and mode 2 (LSM = 11). Reference: Oracle `_interlaceEnabledCached = data.GetBit(1)`
    /// (`Devices/315-5313/S315-5313_Ports.cpp:1883`).
    fn interlace_enabled(&self) -> bool {
        self.regs[0x0C] & 0x02 != 0
    }

    /// Whether the display is enabled: reg 1 bit 6 (the DISP/M2 bit).
    fn display_enabled(&self) -> bool {
        self.regs[1] & 0x40 != 0
    }

    /// Set the VINT pending latch and advance the odd-frame flag (recon R12; the VInt scheduler event at
    /// line 224 drives this). The latch is cleared only by [`Vdp::acknowledge`].
    ///
    /// The ODD flag (status bit 4) reflects the odd/even frame **only while an interlace mode is active**;
    /// outside interlace it reads 0. The reference implements this at the toggle point, not the read:
    /// `oddFlagSet = interlaceIsEnabled & !oddFlagSet` (Oracle `Devices/315-5313/S315-5313_Timing.cpp:1103`
    /// in `AdvanceHVCounters`, repeated at :1181 in `AdvanceHVCountersOneStep`) — the stored flag is forced
    /// to 0 at each toggle while interlace is off. Hardware ground truth: memtest_68k's `C00004-C00007` row
    /// reads `4E88` (bit 4 clear) with reg $0C = $81 (interlace off).
    pub fn raise_vint(&mut self) {
        self.vint_pending = true;
        self.odd_frame = self.interlace_enabled() && !self.odd_frame;
    }

    /// Set the HINT pending latch (recon R12; an HInt scheduler event drives this on HINT-counter underflow).
    pub fn raise_hint(&mut self) {
        self.hint_pending = true;
    }

    /// Commit one scanline's sprite latches (recon R10), driven by [`Vdp::render_scanline`] and by
    /// [`Vdp::advance_scanline`], the picture-free twin an unarmed run takes instead (finding C5) — the two
    /// hand this function the same three values, which is the whole of what makes them equivalent. Sets the R10
    /// masking carry to **this line's** dot overflow (so the next line's first-on-line x=0 sprite masks), and
    /// **ORs** the sprite-overflow (status bit 6) / collision (status bit 5) status latches — sticky until a
    /// status read clears them. Kept in `vdp.rs` (the owner of these serialized fields); the renderer computes
    /// the deltas and hands them here.
    pub fn commit_scanline_sprites(&mut self, dot_overflow: bool, overflow: bool, collision: bool) {
        self.sprite_dot_overflow_carry = dot_overflow;
        self.sprite_overflow |= overflow;
        self.sprite_collision |= collision;
    }

    /// Per-line HINT-counter bookkeeping (recon R7), driven at the HInt H anchor (~79% through the
    /// line, H = $A6 in H40 / $86 in H32 — see [`Vdp::hint_offset`]) by the HInt event, NOT at line
    /// start. The phase is load-bearing: reg-10 writes earlier in the same line must be visible to
    /// this line's reload ("writing reg 10 does not load the live counter; the value takes effect at
    /// the next reload"), which is exactly the S3K/aeon HInt-handler arm-chain idiom — the handler
    /// re-arms reg 10 mid-line and the reload at this line's anchor must pick it up.
    /// Returns `true` on HINT-counter underflow (the caller raises the HINT pending latch):
    /// - blanking lines after the first one, `ACTIVE_LINES + 1 .. LINES_PER_FRAME` (225..=261), reload the
    ///   counter from reg 10 (no decrement, no HINT);
    /// - lines `0..=ACTIVE_LINES` (0..=224: the active display plus the first blanking line) decrement; on
    ///   underflow the counter reloads from reg 10 and HINT fires (so reg10 = N → HINT on lines N, 2N+1,
    ///   3N+2, …; reg10 = 0 → every line 0..=224, incl. line 224).
    ///
    /// The reload range is [`ACTIVE_LINES`] restated (lens M53/M67), not a range of its own: it used to be
    /// the literal `225..=261`, and it is the same set of lines for every `u16`.
    pub fn hint_anchor_tick(&mut self, line: u16) -> bool {
        if (u64::from(ACTIVE_LINES) + 1..LINES_PER_FRAME).contains(&u64::from(line)) {
            self.hint_counter = self.regs[0x0A];
            false
        } else if self.hint_counter == 0 {
            self.hint_counter = self.regs[0x0A];
            true
        } else {
            self.hint_counter -= 1;
            false
        }
    }

    /// The in-line mclk offset of the HINT-pending H anchor (recon R7): H = $A6 (H40) / $86 (H32).
    pub fn hint_offset(&self) -> u64 {
        self.dot_at_h(if self.h40() { 0xA6 } else { 0x86 })
    }

    /// The in-line mclk offset of the VINT-pending H anchor (recon R7/R6): H = $02.
    pub fn vint_offset(&self) -> u64 {
        self.dot_at_h(0x02)
    }

    /// The mclk offset within a line at which the readable H counter first reaches `h` (the inverse of the
    /// linear dot→position map; used only for the pinned interrupt H anchors, all pre-jump values). Pure
    /// timing — the exact offset is not currency-critical (events are delivered at instruction boundaries).
    fn dot_at_h(&self, h: u8) -> u64 {
        let positions: u64 = if self.h40() { 422 } else { 342 };
        (h as u64 * 2) * MCLK_PER_LINE / positions
    }

    // --- Introspection primitives (design §4, state-shaped only) -----------------------------------------
    // These are the pure, state-derived primitives the API owes. render_line_report and pixel_attribution
    // both landed (in render.rs, with their pipeline stages) and the wire wrapping is SERVED —
    // `emulator/pixel_attribution` is in oracle-aether's dispatch table. This banner said the wrapping was
    // "out of scope this push" and that those two "land with … pushes 3-5" until the lens sweep.

    /// Decode tile `index` from VRAM to its 64 4-bit colour indices, row-major (8×8). A Genesis tile is 32
    /// bytes (8 rows × 4 bytes; each byte packs two pixels, high nibble = left). Pure VRAM decode.
    pub fn tile_pixels(&self, index: usize) -> [u8; 64] {
        let base = index.wrapping_mul(32);
        let mut out = [0u8; 64];
        for row in 0..8 {
            for col in 0..4 {
                let byte = self.vram[base.wrapping_add(row * 4 + col) & (VRAM_SIZE - 1)];
                out[row * 8 + col * 2] = byte >> 4;
                out[row * 8 + col * 2 + 1] = byte & 0x0F;
            }
        }
        out
    }

    /// Decode the 64 CRAM colour entries to RGB at the fixed introspection ramp (`ramp3`). CRAM words are
    /// stored big-endian; the 9-bit colour is laid out `---- BBB- GGG- RRR-`. These are our reported values,
    /// NOT calibrated DAC output (the measured-level calibration is a deferred rendering item, recon R11).
    pub fn cram_decoded(&self) -> [(u8, u8, u8); 64] {
        let mut out = [(0u8, 0u8, 0u8); 64];
        for (i, slot) in out.iter_mut().enumerate() {
            let word = ((self.cram[i * 2] as u16) << 8) | self.cram[i * 2 + 1] as u16;
            let r = ((word >> 1) & 0x07) as u8;
            let g = ((word >> 5) & 0x07) as u8;
            let b = ((word >> 9) & 0x07) as u8;
            *slot = (ramp3(r), ramp3(g), ramp3(b));
        }
        out
    }

    /// **Debug poke of one CRAM entry** (`emulator/write_cram`, `protocol.md` §11.17 / CR-27).
    ///
    /// `entry` is `line × 16 + index`, 0–63; `word` is the colour to store. Returns the word **actually
    /// stored** — masked to the chip's nine bits — so the caller can report what the hardware holds
    /// without re-deriving the mask, which is how `emulator/write_cram`'s `value` stays truthful.
    ///
    /// # Two deliberate departures from the port path, both of which are the point
    ///
    /// This **bypasses the VDP port path** — no FIFO, no autoincrement, no DMA, no control-port state —
    /// and writes the array directly. `emulator/write_cram` requires a paused machine, which blunts the
    /// fidelity objection to nearly nothing: stopped, there is no FIFO in flight, no DMA in progress and
    /// no active raster, so the port path's side effects are exactly the ones a paused poke has no
    /// business producing.
    ///
    /// And it deliberately **does NOT [`capture`](Self::capture) to the watch surface**, which is the
    /// half that could not be gotten any other way. A hit's `pc` names the instruction that drove the
    /// access and a debugger poke has none to name (`emulator/write_memory`'s standing rule); worse,
    /// since §11.15 a captured CRAM write also carries the landing clock its instruction supplies, and an
    /// instruction-less write would either fabricate one or silently take whatever `mclk` is current.
    /// `tests/cram.rs::a_poke_is_never_offered_to_the_watch_surface` is the direct pin, and it exists to
    /// catch a later "simplification" of this function into [`write_target`](Self::write_target).
    ///
    /// # Why the arithmetic is duplicated rather than shared
    ///
    /// The `0x0EEE` mask and the big-endian byte layout are lifted verbatim from `write_target`'s
    /// `VdpTarget::Cram` arm. Factoring the two into a shared helper would be an edit to a function on the
    /// **currency path** — every frozen golden depends on guest-driven CRAM writes — so the duplication
    /// is the conservative choice, and `cram_poke_matches_the_port_path` is the test that stops the two
    /// from drifting.
    ///
    /// # Why `at_mclk` is a parameter and not `self.now_mclk`
    ///
    /// A poke needs a write instant for §11.27's CRAM stamp — a repaint of an entry the raster already
    /// drew is precisely the divergence the caveat exists to disclose, and the anti-fix pin
    /// (`tests/pixel_attribution.rs`) drives it through *this* function. But
    /// [`now_mclk`](Self::now_mclk) is the instant the VDP last did **guest-driven** work, and a debug
    /// poke is not guest-driven: on a machine paused after a quiet stretch it can be arbitrarily stale,
    /// which would date the poke *before* the line it must be reported as landing after. Taking it as a
    /// parameter makes the caller — which knows the machine's actual now — responsible for the one fact
    /// this function cannot honestly invent, and matches the reasoning two paragraphs up about why a
    /// capture must not fabricate a clock. `self.now_mclk` is deliberately **not** advanced: a poke is
    /// not VDP work, and moving it would relocate the *next* guest write's stamp.
    ///
    /// # Panics
    ///
    /// If `entry > 63`. Callers on the bus refuse an out-of-range `line`/`index` with `-32602` long
    /// before this, so reaching it is a server bug rather than a client one.
    pub fn poke_cram(&mut self, entry: u8, word: u16, at_mclk: u64) -> u16 {
        assert!(entry < 64, "CRAM entry {entry} is outside 0-63");
        let masked = word & 0x0EEE; // 9-bit colour (---- BBB- GGG- RRR-)
        let b = (entry as usize) * 2;
        self.cram[b] = (masked >> 8) as u8;
        self.cram[b | 1] = (masked & 0xFF) as u8;
        self.cram_written_mclk[entry as usize] = Some(at_mclk);
        masked
    }

    /// The master clock at which CRAM `entry` (0–63) was last written, or `None` if it has not been
    /// written since power-on — see the [`cram_written_mclk`](Vdp#structfield.cram_written_mclk) field
    /// for why those two are distinguished rather than sharing a zero.
    ///
    /// This is the raw datum, not the verdict: §11.27's rule compares it against when line `y` of the
    /// last completed frame was drawn, and that comparison is
    /// [`render::cram_divergence_caveat`](crate::render::cram_divergence_caveat) — a free function, so
    /// the VDP stays a machine and does not acquire an opinion about what a caller should be warned of.
    ///
    /// **That home moved on 2026-09-04 and this sentence moved with it.** The comparison shipped inside
    /// `oracle-aether`, "with the reply that discloses it", which was right while
    /// `emulator/pixel_attribution` was its only consumer. The player's click panel
    /// (`oracle-frontend`'s `pick::resolve`) is now the second, it may not reach the bus for the answer
    /// (D15), and it links `oracle-aether` only under an optional feature — so a verdict living there
    /// would vanish from the window in a `--no-default-features` build. It is in `oracle-core` because
    /// both consumers link `oracle-core` unconditionally; it is still not on `Vdp`, because the reason
    /// for that was never about which crate.
    ///
    /// # Panics
    ///
    /// If `entry > 63`, for the same reason [`poke_cram`](Self::poke_cram) does.
    pub fn cram_written_mclk(&self, entry: u8) -> Option<u64> {
        assert!(entry < 64, "CRAM entry {entry} is outside 0-63");
        self.cram_written_mclk[entry as usize]
    }

    /// **Debug poke of one VRAM byte** (`emulator/write_vram`, `protocol.md` §6, row at line 1257).
    ///
    /// `addr` is a VRAM byte address, `byte` the value to store. Byte-addressed and linear: byte *i* of a
    /// payload lands at `addr + i`, which is what makes `write_vram` → `read_vram` an identity. The
    /// data port's odd-address byte-swap is a property of a **word** written through the port and has no
    /// counterpart here.
    ///
    /// # Why this exists rather than [`vram_mut`](Self::vram_mut)
    ///
    /// `vram_mut` hands out the bare array and runs **nothing else**. A write through it into the sprite
    /// attribute table would leave the SAT cache holding the previous Y/size/link, so the emulator would
    /// go on drawing a picture the VRAM no longer describes — `sprites`' `cacheDivergence` would report
    /// `true` for a table nobody had actually left stale. Every VRAM byte the guest writes routes through
    /// [`write_vram_byte`](Self::write_vram_byte), which maintains that cache; a poke that skipped it
    /// would be a write path with a missing side effect, so this function maintains it too. It is
    /// therefore the *only* supported write entry point for the bus; `vram_mut` stays what its own doc
    /// comment says it is — a fixture hook for tests that perturb state.
    ///
    /// # Two deliberate departures from the port path, both of which are the point
    ///
    /// This **bypasses the VDP port path** — no FIFO, no autoincrement, no DMA, no control-port state —
    /// exactly as [`poke_cram`](Self::poke_cram) does, and for the same reason.
    ///
    /// And it deliberately **does NOT [`capture`](Self::capture) to the watch surface**. A hit's `pc`
    /// names the instruction that drove the access and a debugger poke has none to name
    /// (`emulator/write_memory`'s standing rule, restated for this row in the fragment's `$comment`:
    /// *"on `write_memory`'s and `write_cram`'s standing rule it is never offered to the watch surface,
    /// and `watchpoint_hits.seen` does not move for it"*). The pin that actually catches a `capture` call
    /// added here is [`a_vram_poke_is_never_offered_to_the_watch_surface`](tests) **below**, which arms the
    /// buffer directly and carries the port path as its control; `oracle-aether`'s `tests/write_vram.rs`
    /// namesake is the end-to-end contract pin and, measured, is *not* sensitive to that poison — the
    /// capture buffer is armed only for the duration of a `System::run`, and a poke is issued between
    /// runs. Both exist to catch a later "simplification" of this function into `write_vram_byte`.
    ///
    /// # Why the SAT arithmetic is duplicated rather than shared
    ///
    /// The window computation is lifted verbatim from `write_vram_byte`. Factoring the two into a shared
    /// helper would be an edit to a function on the **currency path** — every frozen golden depends on
    /// guest-driven VRAM writes, and every DMA byte routes through it — so the duplication is the
    /// conservative choice, exactly as `poke_cram`'s own duplication of `write_target`'s CRAM arm was.
    /// `vram_poke_matches_the_port_path` is the test that stops the two from drifting.
    ///
    /// # Panics
    ///
    /// If `addr >= VRAM_SIZE`. The bus refuses an out-of-range address with `-32004` long before this,
    /// and masking here would turn a server bug into a silent wrap — the thing the row's whole-request
    /// refusal exists to prevent.
    pub fn poke_vram(&mut self, addr: usize, byte: u8) {
        assert!(
            addr < VRAM_SIZE,
            "VRAM address {addr:#X} is outside the 64K space"
        );
        self.vram[addr] = byte;
        let base = self.sat_base();
        let entries = if self.h40() { SAT_SLOTS } else { 64 };
        let off = addr.wrapping_sub(base);
        let entry = off / 8;
        let byte_in_entry = off % 8;
        if byte_in_entry < 4 && entry < entries {
            self.sat_cache[entry * 4 + byte_in_entry] = byte;
        }
    }
}

/// The fixed introspection colour ramp: a 3-bit channel level (`0..=7`) → 8-bit, linear (`level × 255 / 7`).
fn ramp3(level: u8) -> u8 {
    (level as u16 * 255 / 7) as u8
}

// ⚑ **A second enum spelled `Target` used to be declared here** — `pub enum { Vram, Cram, Vsram }`, the same six
// derives as [`VdpTarget`] ~1,650 lines up, on a wire type, with neither doc mentioning the other (H30).
// There is now one type; see [`VdpTarget`] for why the merge is byte-identical on the wire.

/// The three DMA modes (recon R4 / RD2): 68k→VDP transfer, VRAM fill, VRAM copy.
#[derive(Clone, Copy, PartialEq, Eq, Debug, bincode::Encode, bincode::Decode)]
pub enum DmaMode {
    Mem,
    Fill,
    Copy,
}

/// A DMA the 68k has just triggered (recon R4), armed on the trigger write and taken by the bus (which owns
/// the 68k source memory) to execute. `len` is in the mode's transfer unit (words for `Mem`, bytes for
/// `Fill`/`Copy` — RD2).
#[derive(Clone, Copy, PartialEq, Eq, Debug, bincode::Encode, bincode::Decode)]
pub enum DmaRequest {
    /// 68k→VDP: read `len` words from 68k byte address `source`, feed each through the FIFO to the current
    /// data-port target + autoinc. Total 68k halt (recon R4(a)).
    Mem { source: u32, len: u16 },
    /// VRAM fill: `len` byte writes of `fill`'s data to the current target, 68k keeps running (recon R4(b)).
    Fill { len: u16, fill: u16 },
    /// VRAM copy: `len` byte read+write steps within VRAM from `source`, FIFO-bypass, 68k runs (recon R4(c));
    /// each read and each write takes the opposite byte lane (`^ 1`, F-COPYXOR — see [`Vdp::run_copy`]).
    Copy { source: u16, len: u16 },
}

/// A completed DMA, for the `frame_report` introspection surface (design §4; recon R4).
#[derive(Clone, Copy, PartialEq, Eq, Debug, bincode::Encode, bincode::Decode)]
pub struct DmaRecord {
    pub mode: DmaMode,
    pub source: u32,
    /// The address register the transfer started from.
    ///
    /// **`Mem` / `Copy`:** the armed command address, i.e. exactly what the CD5 command word set.
    ///
    /// **`Fill`: one autoincrement step PAST the armed command address.** Since A3b the fill's trigger
    /// data-port write is completed as an ordinary write and auto-increments (P2), so the fill engine
    /// begins from `armed + reg15` — which is genuinely where it starts walking, and with the `address ^ 1`
    /// byte placement (P3) its first *written* byte is back at `armed` for the common even-address /
    /// autoinc-1 case. A fill armed at `$8000` with autoinc 1 therefore reports `dest == $8001`. This is
    /// deliberate but it does make `Fill` inconsistent with `Copy`; the field is introspection-only
    /// (`Vdp::last_dma` → `FrameReport::dma`) and is in neither frozen currency.
    pub dest: u16,
    pub len: u16,
    pub target: VdpTarget,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Power-on allocates every hashed region at its hardware size. **The check is the accessor calls**,
    /// not an assertion on their lengths: since lens M70 each accessor returns an array, so a `.len()`
    /// comparison is true by type and could not fail, while the accessor's own `Vec`→array conversion
    /// panics, naming the region, on a wrong allocation. A lengths-only version of this row is what was
    /// here before M70, and it would now be a test that cannot go red.
    #[test]
    fn power_on_allocates_fixed_region_sizes() {
        let mut rng = SplitMix64::new(1);
        let vdp = Vdp::power_on(&mut rng);
        let _: &[u8; VRAM_SIZE] = vdp.vram();
        let _: &[u8; CRAM_SIZE] = vdp.cram();
        let _: &[u8; VSRAM_SIZE] = vdp.vsram();
        let _: &[u8; REG_COUNT] = vdp.regs();
    }

    #[test]
    fn power_on_seeds_vram_zeros_the_rest() {
        let mut rng = SplitMix64::new(0xABCD);
        let vdp = Vdp::power_on(&mut rng);
        assert!(
            vdp.vram().iter().any(|&b| b != 0),
            "VRAM is seeded non-zero"
        );
        assert!(vdp.cram().iter().all(|&b| b == 0), "CRAM starts zeroed");
        assert!(vdp.vsram().iter().all(|&b| b == 0), "VSRAM starts zeroed");
        assert!(vdp.regs().iter().all(|&b| b == 0), "registers start zeroed");
    }

    #[test]
    fn same_rng_stream_yields_identical_vram() {
        let mut a = SplitMix64::new(42);
        let mut b = SplitMix64::new(42);
        assert_eq!(Vdp::power_on(&mut a), Vdp::power_on(&mut b));
    }

    // --- Timing FSM (recon R2) ---------------------------------------------------------------------------

    /// A powered-on VDP **in Mode 5**. A bare `power_on` leaves reg 1 at `$00` = M5 clear = Mode 4, where
    /// registers above 10 are not writable (see `Vdp::write_register`); every fixture here means Mode 5.
    /// The mode-4 masking test below re-clears M5 itself, deliberately.
    fn fresh() -> Vdp {
        let mut v = Vdp::power_on(&mut SplitMix64::new(1));
        v.control_write(0x8104, 0); // reg 1 = $04 → M5 set (mode 5)
        v
    }

    // --- Control-port / code-register edges (slice A2; VDPFIFOTesting tests 10, 12, 13) -------------------

    /// Replay a control-port word stream (`ctrl`) / data-port word (`data`) at mclk 0, the way the ROM does.
    fn ctrl(v: &mut Vdp, words: &[u16]) {
        for &w in words {
            v.control_write(w, 0);
        }
    }

    /// VDPFIFOTesting test 13 "Register Writes and Code Reg" (ROM `$22D6`, expected table `$22FA`), the
    /// second observation group at ROM `$23B4`: a `$8xxx` register write between a VRAM-write command and
    /// the data writes makes those writes vanish — VRAM keeps the `$FFFF`s. This is Charles MacDonald's
    /// "Writing to a VDP register will clear the code register. Games that rely on this are Golden Axe II
    /// … and Sonic 3D" (genvdp.txt 1.5f).
    #[test]
    fn a_register_write_drops_the_following_data_port_writes() {
        let mut v = fresh();
        ctrl(&mut v, &[0x4000, 0x0002]); // VRAM write @ $8000
        v.regs[0x0F] = 2; // autoinc 2 (the ROM inherits it)
        for _ in 0..4 {
            v.data_write(0xFFFF);
        }
        ctrl(&mut v, &[0x4000, 0x0002]); // VRAM write @ $8000 again
        ctrl(&mut v, &[0x8F02]); // reg 15 = 2 — a REGISTER write
        v.data_write(0x0123);
        v.data_write(0x4567);
        ctrl(&mut v, &[0x0000, 0x0002]); // VRAM read @ $8000
        assert_eq!(
            [v.data_read(0), v.data_read(0)],
            [0xFFFF, 0xFFFF],
            "ROM $22FA words 2-3: the writes after a register write never reach VRAM"
        );
    }

    /// Same test 13, the fifth/sixth groups (ROM `$24EC`): CD5-CD2 SURVIVE the register write. A following
    /// first-half-only control word re-supplies CD1-CD0 only, and the writes land on the *retained* VSRAM
    /// write target — not VRAM. This is what forbids modelling "clear the code register" as a full clear.
    #[test]
    fn a_register_write_retains_cd5_cd2() {
        let mut v = fresh();
        v.regs[0x0F] = 2;
        ctrl(&mut v, &[0x4000, 0x0002]); // VRAM write @ $8000
        for _ in 0..4 {
            v.data_write(0xFFFF);
        }
        ctrl(&mut v, &[0x4000, 0x0012]); // VSRAM write @ $8000 (CD5-CD2 = 0001)
        ctrl(&mut v, &[0x8F02]); // register write
        ctrl(&mut v, &[0x4000]); // first half only: CD1-CD0 = 01
        v.data_write(0x0123);
        v.data_write(0x4567);
        ctrl(&mut v, &[0x0000, 0x0002]); // VRAM read @ $8000
        assert_eq!(
            [v.data_read(0), v.data_read(0)],
            [0xFFFF, 0xFFFF],
            "ROM $22FA words 8-9: VRAM is untouched — the retained target was VSRAM"
        );
        ctrl(&mut v, &[0x0000, 0x0012]); // VSRAM read @ $8000
        let b = usize::from(0x8000u16 & 0x7F); // the 7-bit VSRAM address (A1): $8000 is byte 0
        assert_eq!(
            [
                u16::from_be_bytes([v.vsram[b], v.vsram[b + 1]]),
                u16::from_be_bytes([v.vsram[b + 2], v.vsram[b + 3]])
            ],
            [0x0123, 0x0567],
            "ROM $22FA words 10-11 ($F923/$FD67 before the snoop merge): the writes went to VSRAM"
        );
    }

    /// VDPFIFOTesting test 10 "Partial CP Writes" (ROM `$FC86`, expected table `$FCAA`), the eighth
    /// observation (ROM `$FFEA`): a first-half-only control word over a CRAM-*read* command leaves
    /// CD3-CD0 = `1001`, which is not in genvdp.txt's code table — so the data-port write is ignored even
    /// though CD0 (write) is set.
    #[test]
    fn a_data_write_with_an_undefined_code_is_ignored() {
        let mut v = fresh();
        v.regs[0x0F] = 2;
        ctrl(&mut v, &[0x4000, 0x0002]); // VRAM write @ $8000
        v.data_write(0xFFFF);
        ctrl(&mut v, &[0x0000, 0x0022]); // CRAM read @ $8000 → code 0b001000
        ctrl(&mut v, &[0x4000]); // first half only → code 0b001001 (undefined)
        v.data_write(0x0246);
        ctrl(&mut v, &[0x0000, 0x0002]); // VRAM read @ $8000
        assert_eq!(
            v.data_read(0),
            0xFFFF,
            "ROM $FCAA word 7: the undefined-code write is discarded"
        );
    }

    /// VDPFIFOTesting test 12 "Register Write Mode4 Mask" (ROM `$20C8`, expected table `$20EC`, sequence at
    /// ROM `$2244`): with M5 clear (reg 1 bit 2 — Mode 4, the SMS mode) a write to register 15 is ignored,
    /// so the autoincrement stays at its Mode-5 value. Kabuto's hardware notes: "All registers except for
    /// the 10(?) SMS registers are disabled".
    ///
    /// Landed in slice T12. The "this moves `export_state_v1::GOLDEN_HASH` and the `golden_frames`
    /// scenes" reason this test was previously `#[ignore]`d for was re-measured and is FALSE — it
    /// mis-identified `testrom::build_pad_poll` (no frozen currency) as the golden fixture, which is
    /// `testrom::build` and drives no VDP port at all. See docs/2026-08-03-decision2-premise-recheck.md.
    /// The fixtures that went red all declared Mode 4 while programming Mode-5 registers; declaring M5
    /// in them restored every frozen hash byte-identically.
    #[test]
    fn mode4_ignores_register_writes_above_ten() {
        let mut v = fresh();
        ctrl(&mut v, &[0x8F02]); // reg 15 = 2 (fresh() is already in mode 5)
        ctrl(&mut v, &[0x8144]); // reg 1 = $44 → display on, M5 still set
        ctrl(&mut v, &[0x8F02]); // reg 15 = 2
        ctrl(&mut v, &[0x8140]); // reg 1 = $40 → M5 CLEAR = mode 4
        ctrl(&mut v, &[0x8F04]); // reg 15 = 4 — must be IGNORED
        assert_eq!(v.regs[0x0F], 2, "reg 15 > 10 is not writable in mode 4");
        ctrl(&mut v, &[0x8144]); // reg 1 = $44 → back to mode 5
        assert_eq!(v.regs[1], 0x44, "reg 1 <= 10 IS writable in mode 4");
        ctrl(&mut v, &[0x8F04]);
        assert_eq!(v.regs[0x0F], 4, "and reg 15 is writable again in mode 5");
    }

    /// The first mclk-in-line dot whose readable H counter equals `target` (each value occurs across a line).
    fn dot_with_h(v: &Vdp, target: u8) -> u64 {
        (0..MCLK_PER_LINE)
            .find(|&d| v.h_counter(d) == target)
            .unwrap_or_else(|| panic!("H = {target:#04X} never occurs in a line"))
    }

    /// Collapse a per-dot sample stream to its distinct-in-order values.
    fn distinct_h(v: &Vdp) -> Vec<u8> {
        let mut seq: Vec<u8> = Vec::new();
        for dot in 0..MCLK_PER_LINE {
            let h = v.h_counter(dot);
            if seq.last() != Some(&h) {
                seq.push(h);
            }
        }
        seq
    }

    fn jumps(seq: &[u8]) -> Vec<(u8, u8)> {
        seq.windows(2)
            .filter(|w| w[1] != w[0].wrapping_add(1))
            .map(|w| (w[0], w[1]))
            .collect()
    }

    #[test]
    fn h_counter_h32_progression_and_jump() {
        let v = fresh(); // regs all zero → H32
        let seq = distinct_h(&v);
        assert_eq!(seq.first(), Some(&0x00), "H32 starts at 0x00");
        assert_eq!(seq.last(), Some(&0xFF), "H32 ends at 0xFF");
        assert_eq!(seq.len(), 171, "H32 has 171 distinct readable values");
        assert_eq!(jumps(&seq), vec![(0x93, 0xE9)], "H32 jumps 0x93→0xE9");
    }

    #[test]
    fn h_counter_h40_progression_and_jump() {
        let mut v = fresh();
        v.regs[0x0C] = 0x81; // RS0 | RS1 → H40
        let seq = distinct_h(&v);
        assert_eq!(seq.first(), Some(&0x00), "H40 starts at 0x00");
        assert_eq!(seq.last(), Some(&0xFF), "H40 ends at 0xFF");
        assert_eq!(seq.len(), 211, "H40 has 211 distinct readable values");
        assert_eq!(jumps(&seq), vec![(0xB6, 0xE4)], "H40 jumps 0xB6→0xE4");
    }

    #[test]
    fn v_counter_progression_and_jump() {
        let v = fresh();
        let mut seq: Vec<u8> = Vec::new();
        for line in 0..LINES_PER_FRAME {
            let vc = v.v_counter(line * MCLK_PER_LINE);
            if seq.last() != Some(&vc) {
                seq.push(vc);
            }
        }
        assert_eq!(seq.len(), 262, "262 distinct V values (235 + 27)");
        assert_eq!(seq.first(), Some(&0x00));
        assert_eq!(seq.last(), Some(&0xFF));
        assert_eq!(jumps(&seq), vec![(0xEA, 0xE5)], "V jumps 0xEA→0xE5");
    }

    #[test]
    fn hblank_h32_anchor_transitions() {
        let v = fresh(); // H32
        assert!(!v.hblank(dot_with_h(&v, 0x92)), "not in hblank at H=0x92");
        assert!(v.hblank(dot_with_h(&v, 0x93)), "hblank SETS at H=0x93");
        assert!(v.hblank(dot_with_h(&v, 0x04)), "still in hblank at H=0x04");
        assert!(!v.hblank(dot_with_h(&v, 0x05)), "hblank CLEARS at H=0x05");
    }

    #[test]
    fn hblank_h40_anchor_transitions() {
        let mut v = fresh();
        v.regs[0x0C] = 0x81; // H40
        assert!(!v.hblank(dot_with_h(&v, 0xB2)), "not in hblank at H=0xB2");
        assert!(v.hblank(dot_with_h(&v, 0xB3)), "hblank SETS at H=0xB3");
        assert!(v.hblank(dot_with_h(&v, 0x05)), "still in hblank at H=0x05");
        assert!(!v.hblank(dot_with_h(&v, 0x06)), "hblank CLEARS at H=0x06");
    }

    #[test]
    fn vblank_sets_at_line_224() {
        let v = fresh();
        assert_eq!(v.v_counter(223 * MCLK_PER_LINE), 0xDF, "line 223 → V=0xDF");
        assert!(!v.vblank(223 * MCLK_PER_LINE), "line 223 is active");
        assert_eq!(v.v_counter(224 * MCLK_PER_LINE), 0xE0, "line 224 → V=0xE0");
        assert!(
            v.vblank(224 * MCLK_PER_LINE),
            "vblank SETS at the 0xDF→0xE0 line"
        );
        assert!(
            v.vblank(261 * MCLK_PER_LINE),
            "vblank holds through the last line"
        );
    }

    #[test]
    fn status_word_reflects_the_timing_bits() {
        let mut v = fresh(); // H32
        v.regs[1] = 0x40; // display enabled (bit 3 is forced set while the display is disabled)
                          // Active display, H well inside the visible span (not hblank): only FIFO-empty (bit 9).
        let active = 100 * MCLK_PER_LINE + dot_with_h(&v, 0x40);
        assert_eq!(
            v.status_word(active),
            0x0200,
            "FIFO-empty only during active display"
        );
        // A vblank line sets bit 3.
        let in_vblank = 240 * MCLK_PER_LINE + dot_with_h(&v, 0x40);
        assert_eq!(
            v.status_word(in_vblank) & (1 << 3),
            1 << 3,
            "vblank bit (b3)"
        );
        // Inside horizontal retrace sets bit 2.
        let in_hblank = 100 * MCLK_PER_LINE + dot_with_h(&v, 0x93);
        assert_eq!(
            v.status_word(in_hblank) & (1 << 2),
            1 << 2,
            "hblank bit (b2)"
        );
    }

    /// Status bit 3 (VBlank) is FORCED SET while the display is disabled (reg 1 bit 6 clear), regardless of
    /// the beam position. Reference: Oracle `vblankFlag |= !_displayEnabledCached` with the comment
    /// "although not mentioned in the official documentation, hardware tests have confirmed that the VBlank
    /// flag is always forced to set when the display is disabled" (`Devices/315-5313/S315-5313_General.cpp:
    /// 2345-2351`). Hardware ground truth: memtest_68k's `C00004-C00007` row reads `4E88` (bit 3 SET) while
    /// its status reads land mid active scan (our probe: frame 11, line 27) with reg 1 = $04 — the ROM only
    /// enables the display after the memory sweep.
    #[test]
    fn status_vblank_bit_forced_while_display_is_disabled() {
        let mut v = fresh(); // reg 1 = 0 → display disabled
        let active = 100 * MCLK_PER_LINE + dot_with_h(&v, 0x40);
        assert_eq!(
            v.status_word(active) & (1 << 3),
            1 << 3,
            "display off → bit 3 set even mid active scan"
        );
        v.regs[1] = 0x40; // display on
        assert_eq!(
            v.status_word(active) & (1 << 3),
            0,
            "display on → bit 3 tracks the real beam position"
        );
        let in_vblank = 240 * MCLK_PER_LINE + dot_with_h(&v, 0x40);
        assert_eq!(
            v.status_word(in_vblank) & (1 << 3),
            1 << 3,
            "display on during vblank → bit 3 still set"
        );
    }

    #[test]
    fn hv_counter_read_combines_v_and_h() {
        let v = fresh();
        let mclk = 50 * MCLK_PER_LINE + dot_with_h(&v, 0x40);
        let expected = ((v.v_counter(mclk) as u16) << 8) | 0x40;
        assert_eq!(v.hv_counter_read(mclk), expected, "(V << 8) | H");
    }

    #[test]
    fn m3_latch_freezes_the_hv_read() {
        let mut v = fresh();
        v.regs[0] = 0x02; // M3 set (reg 0 bit 1)
        v.hv_latch = 0xABCD;
        assert_eq!(v.hv_counter_read(0), 0xABCD, "frozen regardless of mclk");
        assert_eq!(v.hv_counter_read(12_345), 0xABCD);
        v.regs[0] = 0x00; // M3 clear → live again
        assert_ne!(
            v.hv_counter_read(12_345),
            0xABCD,
            "returns the live counter"
        );
    }

    // --- Control / data ports + memories (recon R1) ------------------------------------------------------

    /// A VDP with a first VRAM-write command word written (toggle armed at address 0x0100, autoinc = 2).
    fn armed() -> Vdp {
        let mut v = fresh();
        v.regs[0x0F] = 2;
        v.control_write(0x4100, 0); // CD1CD0 = 01 (VRAM write low bits), A13-A0 = 0x0100
        v
    }

    #[test]
    fn command_splits_low_half_then_high_half_across_two_words() {
        // VRAM write to 0xC123: word 1 = (01<<14)|(0xC123 & 0x3FFF) = 0x4123; word 2 A15-A14 = 3 → 0x0003.
        let mut v = fresh();
        v.control_write(0x4123, 0);
        assert!(v.pending, "first word arms the toggle");
        assert_eq!(v.code, 0x01, "CD1-CD0 applied immediately");
        assert_eq!(
            v.addr, 0x0123,
            "A13-A0 applied immediately, A15-A14 old (0)"
        );
        v.control_write(0x0003, 0);
        assert!(!v.pending, "second word disarms");
        assert_eq!(v.code, 0x01);
        assert_eq!(v.addr, 0xC123, "A15-A14 applied by the second word");
    }

    fn regs_with_reg15() -> [u8; REG_COUNT] {
        let mut r = [0u8; REG_COUNT];
        r[0x01] = 0x04; // M5 set — reg 15 is only writable in mode 5
        r[0x0F] = 0x02;
        r
    }

    #[test]
    fn register_write_never_arms_the_toggle() {
        let mut v = fresh();
        v.control_write(0x8F02, 0); // reg 15 = 2
        assert!(!v.pending, "a $8xxx register write does not arm the toggle");
        assert_eq!(v.regs[0x0F], 0x02, "register written");
        v.control_write(0x9811, 0); // reg (0x18 = 24) >= 24 → ignored
        assert_eq!(v.regs, regs_with_reg15(), "reg >= 24 ignored");
    }

    // The four pending-toggle experiment cells (recon R1, the recorded BlastEm experiment):
    #[test]
    fn toggle_cell0_no_probe_persists() {
        let v = armed();
        assert!(v.pending, "sel 0: no probe → the armed toggle persists");
    }

    #[test]
    fn toggle_cell1_status_read_clears() {
        let mut v = armed();
        v.control_read_status(0, 0);
        assert!(
            !v.pending,
            "sel 1: a status read clears the toggle (instrument pin)"
        );
    }

    #[test]
    fn toggle_cell2_hv_read_does_not_clear() {
        let v = armed();
        v.hv_counter_read(0);
        assert!(
            v.pending,
            "sel 2: an HV-counter read does NOT clear the toggle"
        );
    }

    #[test]
    fn toggle_cell3_data_write_clears() {
        let mut v = armed();
        v.data_write(0x1234);
        assert!(!v.pending, "sel 3: a data-port write clears the toggle");
    }

    #[test]
    fn autoincrement_advances_the_address_by_reg15() {
        let mut v = fresh();
        v.regs[0x0F] = 2;
        v.code = 0x01; // VRAM write
        v.addr = 0x0100;
        v.data_write(0x1111);
        assert_eq!(v.addr, 0x0102, "address advanced by reg 15 (2)");
        v.data_write(0x2222);
        assert_eq!(v.addr, 0x0104);
    }

    #[test]
    fn cram_write_masks_to_nine_bits_big_endian() {
        let mut v = fresh();
        v.code = 0x03; // CRAM write
        v.addr = 0;
        v.data_write(0xFFFF);
        assert_eq!(v.cram[0], 0x0E, "high byte of 0x0EEE (0xFFFF & 0x0EEE)");
        assert_eq!(v.cram[1], 0xEE, "low byte");
    }

    // --- A1: the VSRAM address decode and the VSRAM read latch (docs/2026-09-12-vdp-port-access-full-rom.md) --

    /// One data-port VSRAM write at `addr`, driven the way the ROM drives it: a full command (code `000101`),
    /// then the word.
    fn vsram_write_at(v: &mut Vdp, addr: u16, w: u16) {
        ctrl(v, &[0x4000 | (addr & 0x3FFF), 0x0010 | (addr >> 14)]);
        v.data_write(w);
    }

    /// The VSRAM word at storage word `i`, straight from the array (not through the port).
    fn vsram_word_at(v: &Vdp, i: usize) -> u16 {
        u16::from_be_bytes([v.vsram[i * 2], v.vsram[i * 2 + 1]])
    }

    /// **Writes to `$50-$7F` are discarded** (Nemesis, "Scaling hardware?" p.5). Every word address in the
    /// unbacked range is written, and storage must not move. Under the old `% 80` decode, `$50` landed on word
    /// 0, `$52` on word 1, and so on. The `$4E` control proves the port path is live, and that the last
    /// backed word is still reached.
    #[test]
    fn a_vsram_write_to_50_7f_is_discarded() {
        let mut v = fresh();
        vsram_write_at(&mut v, 0x004E, 0x0456);
        assert_eq!(
            vsram_word_at(&v, 39),
            0x0456,
            "control: $4E is word 39, the last backed word"
        );
        let before = v.vsram.clone();
        for a in (0x50u16..0x80).step_by(2) {
            vsram_write_at(&mut v, a, 0x0100 | a);
        }
        assert_eq!(
            v.vsram, before,
            "a write to $50-$7F must land nowhere (the old decode put $50 on word 0)"
        );
    }

    /// **The VSRAM address is 7 bits, so it wraps at `$80`: an address at or above `$80` maps by `& $7F`.**
    /// The three addresses are chosen so the old `% 80` decode puts each somewhere else: `$8002 % 80` is
    /// byte 50 (word 25) rather than word 1, `$00A4 % 80` is byte 4 rather than word 18, and `$FFD0 % 80` is
    /// byte 48 (word 24), where `& $7F` gives `$50`, which is discarded. The read-back goes through the port,
    /// at wrapped addresses too, so the read half shares the decode.
    #[test]
    fn the_vsram_address_is_seven_bits_so_it_wraps_at_80() {
        let mut v = fresh();
        vsram_write_at(&mut v, 0x8002, 0x0111); // $8002 & $7F = $02: word 1
        vsram_write_at(&mut v, 0x00A4, 0x0222); // $A4 & $7F = $24: word 18
        vsram_write_at(&mut v, 0xFFD0, 0x0333); // $D0 & $7F = $50: unbacked, discarded
        let stored: Vec<(usize, u16)> = (0..VSRAM_SIZE / 2)
            .map(|i| (i, vsram_word_at(&v, i)))
            .filter(|&(_, w)| w != 0)
            .collect();
        assert_eq!(
            stored,
            [(1, 0x0111), (18, 0x0222)],
            "exactly words 1 and 18 hold data: $8002 and $00A4 wrap by & $7F, and $FFD0 is discarded"
        );
        ctrl(&mut v, &[0x0002, 0x0012]); // VSRAM read @ $8002
        assert_eq!(v.data_read(0), 0x0111, "a read at $8002 reaches word 1");
        ctrl(&mut v, &[0x00A4, 0x0010]); // VSRAM read @ $00A4
        assert_eq!(v.data_read(0), 0x0222, "a read at $00A4 reaches word 18");
    }

    /// **A read of `$50-$7F` returns the VSRAM read latch, which the committed render feeds.** This is the
    /// ROM's own shape (VDPFIFOTesting's VSRAM fills, ROM `$13D16`): read word 39 with autoincrement 2, and
    /// the read-ahead steps onto `$50`. Each of the three wrong models gives a different answer, and all
    /// three differ from the right one. Word 0 is `$0111` (the scratch shortcut, and the old `% 80` decode).
    /// The port's previous read, word 39, is `$0333` (a latch fed by port reads). Zero means the committed
    /// render never fed the latch.
    #[test]
    fn a_vsram_read_of_50_7f_returns_the_read_latch() {
        let mut v = fresh();
        ctrl(&mut v, &[0x8144]); // display on; reg 11 = 0, so vertical scroll is full-screen
        vsram_write_at(&mut v, 0x0000, 0x0111); // word 0, plane A
        vsram_write_at(&mut v, 0x0002, 0x0222); // word 1, plane B: a full-screen line's last fetch
        vsram_write_at(&mut v, 0x004E, 0x0333); // word 39
        v.render_scanline(0);
        assert_eq!(
            v.vsram_read_latch(),
            0x0222,
            "the committed line latched its last vertical-scroll fetch, word 1"
        );
        ctrl(&mut v, &[0x8F02]); // autoincrement 2
        ctrl(&mut v, &[0x004E, 0x0010]); // VSRAM read @ $4E
        assert_eq!(v.data_read(0), 0x0333, "control: $4E is word 39");
        assert_eq!(
            v.data_read(0),
            0x0222,
            "$50 returns the read latch: not word 0 ($0111), not the port's previous word ($0333)"
        );
        assert_eq!(v.data_read(0), 0x0222, "$52 latches nothing either");
    }

    /// **Both committed per-line paths feed the latch the same way, and a display-disabled line feeds it
    /// nothing.** `render_scanline` (a run that wants rows) and `advance_scanline` (every other run,
    /// `oracle-replay` included) must leave the same latch, or two runs of one input diverge on a `$50`
    /// read. The words are non-zero and distinct per mode, so a path that skipped the feed, or read the wrong
    /// word, would differ. The mode steps are the ones `last_vscroll_fetch` states: full-screen word 1, and
    /// in 2-cell mode the last column's plane-B word (31 in H32, 39 in H40).
    #[test]
    fn both_committed_scanline_paths_feed_the_vsram_read_latch_alike() {
        let mut full = fresh(); // reg 1 = $04: display DISABLED
        for (i, w) in [(1usize, 0x0222u16), (31, 0x0444), (39, 0x0555)] {
            vsram_write_at(&mut full, (i * 2) as u16, w);
        }
        let mut cheap = full.clone();
        let mut line = 0u16;
        let mut step = |full: &mut Vdp, cheap: &mut Vdp, regs: &[u16], want: u16, why: &str| {
            ctrl(full, regs);
            ctrl(cheap, regs);
            full.render_scanline(line);
            cheap.advance_scanline(line);
            line += 1;
            assert_eq!(full.vsram_read_latch(), want, "render_scanline: {why}");
            assert_eq!(cheap.vsram_read_latch(), want, "advance_scanline: {why}");
            assert!(
                *full == *cheap,
                "{why}: the two committed paths left different machines"
            );
        };
        step(
            &mut full,
            &mut cheap,
            &[],
            0,
            "display disabled: no fetch, the power-on 0 stays",
        );
        step(
            &mut full,
            &mut cheap,
            &[0x8144],
            0x0222,
            "full-screen: word 1",
        );
        step(
            &mut full,
            &mut cheap,
            &[0x8B04],
            0x0444,
            "2-cell, H32: word 31",
        );
        step(
            &mut full,
            &mut cheap,
            &[0x8C81],
            0x0555,
            "2-cell, H40: word 39",
        );
        step(
            &mut full,
            &mut cheap,
            &[0x8104],
            0x0555,
            "display disabled again: the latch keeps word 39",
        );
    }

    #[test]
    fn vsram_write_masks_to_eleven_bits_big_endian() {
        let mut v = fresh();
        v.code = 0x05; // VSRAM write
        v.addr = 0;
        v.data_write(0xFFFF);
        assert_eq!(v.vsram[0], 0x07, "high byte of 0x07FF");
        assert_eq!(v.vsram[1], 0xFF, "low byte");
    }

    #[test]
    fn vram_even_address_writes_normally_odd_address_swaps_bytes() {
        let mut v = fresh();
        v.code = 0x01; // VRAM write
        v.addr = 0x0002; // even
        v.data_write(0x5678);
        assert_eq!((v.vram[2], v.vram[3]), (0x56, 0x78), "even: high then low");
        v.addr = 0x0001; // odd → byte-swap
        v.data_write(0x1234);
        assert_eq!(
            (v.vram[0], v.vram[1]),
            (0x34, 0x12),
            "odd address swaps the two bytes (recon R3)"
        );
    }

    #[test]
    fn vram_readback_round_trips_through_the_precache() {
        let mut v = fresh();
        v.regs[0x0F] = 2;
        // Write two words at VRAM 0x0100.
        v.control_write(0x4100, 0); // VRAM write, addr 0x0100
        v.control_write(0x0000, 0);
        v.data_write(0xBEEF);
        v.data_write(0xCAFE);
        // Set up a VRAM read at 0x0100 (code 0x00) — completing the command pre-fills the read buffer.
        v.control_write(0x0100, 0);
        v.control_write(0x0000, 0);
        assert_eq!(v.data_read(0), 0xBEEF, "first word via the pre-cache");
        assert_eq!(v.data_read(0), 0xCAFE, "second word");
    }

    #[test]
    fn data_read_with_a_write_code_is_the_lockup_cell() {
        let mut v = fresh();
        v.code = 0x01; // a WRITE command armed (CD0 = 1)
        let got = v.data_read(0xDEAD);
        assert_eq!(got, 0xDEAD, "the lockup cell returns open bus, never hangs");
        assert!(
            v.latched_fault(),
            "the lockup fault is latched for the debugger"
        );
    }

    #[test]
    fn m3_latch_is_populated_on_the_register_write_that_sets_it() {
        let mut v = fresh();
        let mclk = 50 * MCLK_PER_LINE + 800;
        let live = ((v.v_counter(mclk) as u16) << 8) | v.h_counter(mclk) as u16;
        v.control_write(0x8002, mclk); // reg 0 = 0x02 → M3 set
        assert_eq!(
            v.hv_latch, live,
            "the live HV counter is frozen when M3 turns on"
        );
        assert_eq!(
            v.hv_counter_read(999_999),
            live,
            "the frozen value is returned regardless of mclk"
        );
    }

    #[test]
    fn mid_command_pending_survives_a_bincode_round_trip() {
        let v = armed(); // pending = true, mid two-word command
        let bytes = bincode::encode_to_vec(&v, bincode::config::standard()).unwrap();
        let (back, _): (Vdp, usize) =
            bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
        assert_eq!(v, back, "the whole VDP round-trips");
        assert!(back.pending, "the armed toggle survives snapshot/restore");
        assert_eq!(back.code, v.code);
        assert_eq!(back.addr, v.addr);
    }

    // --- Interrupts + IPL deassert (recon R7/R12) --------------------------------------------------------

    fn enable_ie0(v: &mut Vdp) {
        v.regs[1] |= 0x20; // IE0 (VINT enable)
    }
    fn enable_ie1(v: &mut Vdp) {
        v.regs[0] |= 0x10; // IE1 (HINT enable)
    }

    #[test]
    fn ipl_is_gated_by_the_enable_bits() {
        let mut v = fresh();
        v.vint_pending = true;
        assert_eq!(v.ipl(), 0, "pending but IE0 off → no IPL");
        enable_ie0(&mut v);
        assert_eq!(v.ipl(), 6, "pending + IE0 → level 6");
    }

    #[test]
    fn clearing_the_enable_drops_ipl_but_keeps_the_latch() {
        // The Counting-Cafe re-assert shape (recon R12): clearing IE0 while pending drops the IPL but keeps
        // the latch; re-enabling re-raises it (nothing cleared the latch but an /INTAK).
        let mut v = fresh();
        v.vint_pending = true;
        enable_ie0(&mut v);
        assert_eq!(v.ipl(), 6);
        v.regs[1] &= !0x20; // clear IE0
        assert_eq!(v.ipl(), 0, "IPL drops");
        assert!(v.vint_pending, "but the latch is kept");
        enable_ie0(&mut v);
        assert_eq!(v.ipl(), 6, "re-enabling re-raises the interrupt");
    }

    #[test]
    fn acknowledge_clears_only_the_acknowledged_level() {
        let mut v = fresh();
        v.vint_pending = true;
        v.hint_pending = true;
        v.acknowledge(6);
        assert!(!v.vint_pending, "level-6 IACK clears VINT");
        assert!(v.hint_pending, "HINT untouched");
        v.acknowledge(4);
        assert!(!v.hint_pending, "level-4 IACK clears HINT");
    }

    #[test]
    fn both_pending_cascades_level_6_then_level_4() {
        let mut v = fresh();
        v.vint_pending = true;
        v.hint_pending = true;
        enable_ie0(&mut v);
        enable_ie1(&mut v);
        assert_eq!(v.ipl(), 6, "both pending → level 6 first");
        v.acknowledge(6);
        assert_eq!(v.ipl(), 4, "after the level-6 IACK, HINT re-drives level 4");
        v.acknowledge(4);
        assert_eq!(v.ipl(), 0);
    }

    #[test]
    fn a_status_read_does_not_clear_the_pending_latches() {
        let mut v = fresh();
        v.vint_pending = true;
        v.hint_pending = true;
        v.control_read_status(0, 0); // clears the control-port toggle, NOT the interrupt latches
        assert!(
            v.vint_pending,
            "VINT latch survives a status read (recon R12)"
        );
        assert!(v.hint_pending, "HINT latch survives a status read");
    }

    #[test]
    fn status_word_f_bit_reflects_vint_pending() {
        let mut v = fresh();
        assert_eq!(
            v.status_word(0) & 0x80,
            0,
            "F bit clear when no VINT pending"
        );
        v.vint_pending = true;
        assert_eq!(
            v.status_word(0) & 0x80,
            0x80,
            "F bit set (recon R12 readback)"
        );
    }

    /// Which active lines (0..=224) raise a HINT for a given reg-10 value, using the per-line bookkeeping
    /// (mirrors the HInt anchor chain: reload during vblank 225..=261, then step lines 0..=224).
    fn hint_lines(reg10: u8) -> Vec<u16> {
        let mut v = fresh();
        v.regs[0x0A] = reg10;
        // Reload during a vblank line so we enter line 0 with the counter = reg10 (as the chain does).
        v.hint_anchor_tick(261);
        (0..=224).filter(|&line| v.hint_anchor_tick(line)).collect()
    }

    #[test]
    fn hint_schedule_reg10_n_fires_on_n_2n_plus_1_3n_plus_2() {
        // reg10 = 5 → HINT on lines 5, 11, 17, 23, … (N, 2N+1, 3N+2, …) while ≤ 224 (recon R7).
        let lines = hint_lines(5);
        assert_eq!(&lines[..4], &[5, 11, 17, 23], "N, 2N+1, 3N+2, 4N+3");
        assert!(lines.iter().all(|&l| l <= 224));
    }

    #[test]
    fn hint_schedule_reg10_zero_fires_every_line_including_224() {
        // reg10 = 0 → the interrupt occurs on every line 0..=224, line 224 included (recon R7).
        let lines = hint_lines(0);
        assert_eq!(lines.len(), 225, "every line 0..=224");
        assert_eq!(*lines.last().unwrap(), 224, "a HINT can fire on line 224");
    }

    #[test]
    fn hint_counter_reloads_during_vblank() {
        let mut v = fresh();
        v.regs[0x0A] = 7;
        v.hint_anchor_tick(261); // vblank reload → counter = 7 entering line 0
        assert_eq!(v.hint_counter, 7);
        // Step three active lines to draw the counter down…
        for line in 0..3 {
            v.hint_anchor_tick(line);
        }
        assert_eq!(v.hint_counter, 4, "7 → decremented three times");
        // …then a vblank line reloads it from reg 10.
        v.hint_anchor_tick(230);
        assert_eq!(v.hint_counter, 7, "reloaded from reg 10 during vblank");
    }

    /// The underflow reload reads reg 10 live at call time (recon R7: "writing reg 10 does not load the
    /// live counter; the value takes effect at the next reload") — a reg-10 rewrite between ticks is
    /// picked up by the next reload, not deferred further. This pins the reload-reads-live-reg10 half of
    /// the arm-chain contract; the other half — that the tick itself runs at the H anchor (~79% through
    /// the line), so a mid-line write from an HInt handler lands before this line's reload — is a
    /// scheduler-phase property pinned at the System level
    /// (`system::tests::hint_reg10_rewrite_after_line_start_is_seen_by_that_lines_anchor_reload`).
    #[test]
    fn hint_reg10_write_before_anchor_is_seen_by_this_lines_reload() {
        let mut v = fresh();
        v.regs[0x0A] = 0;
        v.hint_anchor_tick(261); // vblank reload → counter = 0 entering line 0
        assert_eq!(v.hint_counter, 0);
        // Mid-line (before this line's anchor) the HInt handler re-arms reg 10 = K…
        const K: u8 = 5;
        v.regs[0x0A] = K;
        // …then the anchor tick fires (counter was 0 → underflow) AND reloads the fresh K.
        assert!(v.hint_anchor_tick(0), "underflow fires on this line");
        assert_eq!(v.hint_counter, K, "reload sees the freshly-written reg 10");
        // The following K ticks must not fire…
        for line in 1..=u16::from(K) {
            assert!(!v.hint_anchor_tick(line), "no fire while counting down");
        }
        // …and the (K+1)-th fires (reg10 = K → next fire K+1 lines later).
        assert!(
            v.hint_anchor_tick(u16::from(K) + 1),
            "fires again after K quiet lines"
        );
    }

    #[test]
    fn raise_vint_toggles_the_odd_frame_flag_in_interlace() {
        let mut v = fresh();
        v.regs[0x0C] = 0x02; // LSM0 — interlace mode on
        assert!(!v.odd_frame);
        v.raise_vint();
        assert!(
            v.vint_pending && v.odd_frame,
            "VINT set + odd-frame toggled"
        );
        v.raise_vint();
        assert!(!v.odd_frame, "toggled back on the next frame");
    }

    /// ODD (status bit 4) reads 0 outside interlace across BOTH frame parities: the reference forces the
    /// stored flag at each toggle point — `oddFlagSet = interlaceIsEnabled & !oddFlagSet`
    /// (Oracle `Devices/315-5313/S315-5313_Timing.cpp:1103` in `AdvanceHVCounters`, and again at :1181 in
    /// `AdvanceHVCountersOneStep`) — so with interlace off it can never become 1. Hardware ground truth:
    /// memtest_68k row `C00004-C00007` reads `4E88` (bit 4 clear) with reg $0C = $81 (interlace off).
    #[test]
    fn odd_flag_stays_zero_outside_interlace() {
        let mut v = fresh(); // reg $0C = 0 → interlace off
        for frame in 0..4 {
            v.raise_vint();
            assert_eq!(
                v.status_word(0) & (1 << 4),
                0,
                "ODD reads 0 outside interlace (frame parity {frame})"
            );
        }
    }

    /// Interlace-enable is reg $0C bit 1 (LSM0) alone — both interlace modes 1 (LSM = 01) and 2 (LSM = 11)
    /// expose the toggle (Oracle `_interlaceEnabledCached = data.GetBit(1)`, `S315-5313_Ports.cpp:1883`).
    #[test]
    fn odd_flag_toggles_in_both_interlace_modes() {
        for lsm in [0x02u8, 0x06u8] {
            let mut v = fresh();
            v.regs[0x0C] = 0x81 | lsm; // H40 + interlace
            v.raise_vint();
            assert_eq!(
                v.status_word(0) & (1 << 4),
                1 << 4,
                "ODD toggles on with LSM bits {lsm:#04x}"
            );
            v.raise_vint();
            assert_eq!(v.status_word(0) & (1 << 4), 0, "…and off the next frame");
        }
    }

    /// Turning interlace OFF mid-stream: the stored flag keeps its value until the next toggle point, where
    /// the reference's `interlaceIsEnabled & !oddFlagSet` forces it to 0 (no read-time gate — Oracle reads
    /// the stored `_status` flag as-is).
    #[test]
    fn odd_flag_clears_at_the_first_toggle_after_interlace_off() {
        let mut v = fresh();
        v.regs[0x0C] = 0x02;
        v.raise_vint();
        assert!(v.odd_frame, "odd frame reached under interlace");
        v.regs[0x0C] = 0x00; // interlace off mid-frame
        assert_eq!(
            v.status_word(0) & (1 << 4),
            1 << 4,
            "stored flag still reads until the next toggle point"
        );
        v.raise_vint();
        assert_eq!(
            v.status_word(0) & (1 << 4),
            0,
            "forced to 0 at the first toggle with interlace off"
        );
    }

    #[test]
    fn pending_latches_survive_a_bincode_round_trip() {
        let mut v = fresh();
        v.vint_pending = true;
        v.hint_pending = true;
        v.hint_counter = 42;
        v.odd_frame = true;
        let bytes = bincode::encode_to_vec(&v, bincode::config::standard()).unwrap();
        let (back, _): (Vdp, usize) =
            bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
        assert_eq!(v, back, "the interrupt state round-trips");
    }

    // --- Introspection primitives (design §4) ------------------------------------------------------------

    #[test]
    fn tile_pixels_decodes_nibbles_row_major() {
        let mut v = fresh();
        // Tile 0, row 0: bytes 0x12 0x34 0x56 0x78 → pixels 1,2,3,4,5,6,7,8 (high nibble = left).
        v.vram[0] = 0x12;
        v.vram[1] = 0x34;
        v.vram[2] = 0x56;
        v.vram[3] = 0x78;
        let px = v.tile_pixels(0);
        assert_eq!(&px[0..8], &[1, 2, 3, 4, 5, 6, 7, 8], "row 0 nibbles");
        // Tile 1 starts at VRAM byte 32.
        v.vram[32] = 0xAB;
        let t1 = v.tile_pixels(1);
        assert_eq!((t1[0], t1[1]), (0xA, 0xB), "tile 1 offset = index * 32");
    }

    #[test]
    fn cram_decoded_maps_the_nine_bit_colour_to_rgb() {
        let mut v = fresh();
        // Entry 0 = 0x0EEE (R=G=B=7) → white; entry 1 = 0x000E (R=7 only) → red; entry 2 stays 0 → black.
        v.cram[0] = 0x0E;
        v.cram[1] = 0xEE;
        v.cram[2] = 0x00;
        v.cram[3] = 0x0E;
        let dec = v.cram_decoded();
        assert_eq!(dec[0], (255, 255, 255), "max colour → white");
        assert_eq!(dec[1], (255, 0, 0), "R=7, G=B=0 → red");
        assert_eq!(dec[2], (0, 0, 0), "zero → black");
    }

    /// `poke_cram` must agree with the port path **byte for byte in CRAM**, because the two are
    /// duplicated arithmetic rather than a shared helper (deliberately — `write_target` is on the
    /// currency path). This is the test that stops them drifting: the same colour driven through the real
    /// control/data port sequence and through the poke must leave CRAM identical, over every bit position
    /// that matters plus the mask's own edges.
    ///
    /// **The comparison is `cram()` only, and that is the intended scope rather than a shortcut.** The
    /// port path also advances the address register by the autoincrement, consumes the control-port
    /// latch, and moves FIFO state; the poke does none of that, by design (see [`Vdp::poke_cram`]). A
    /// whole-VDP comparison would therefore fail for exactly the reasons the seam exists, so what is
    /// pinned here is the one thing the two must agree on — the bytes that land in the colour array.
    #[test]
    fn cram_poke_matches_the_port_path() {
        for word in [
            0x0000, 0x0EEE, 0x000E, 0x0E00, 0x0AAA, 0x0246, 0xFFFF, 0x1111,
        ] {
            for entry in [0u8, 1, 17, 63] {
                let mut port = fresh();
                // The real path: control port sets CRAM-write at the entry's byte address, data port writes.
                let addr = u32::from(entry) * 2;
                port.control_write((0xC000 | (addr & 0x3FFF)) as u16, 0);
                port.control_write((addr >> 14) as u16, 0);
                port.data_write(word);

                let mut poke = fresh();
                let stored = poke.poke_cram(entry, word, 0);

                assert_eq!(
                    poke.cram(),
                    port.cram(),
                    "entry {entry}, word {word:#06X}: the poke and the port path disagree"
                );
                assert_eq!(
                    stored,
                    word & 0x0EEE,
                    "the returned word is the STORED word"
                );
                let b = entry as usize * 2;
                assert_eq!(
                    ((poke.cram()[b] as u16) << 8) | poke.cram()[b | 1] as u16,
                    stored,
                    "big-endian layout"
                );
            }
        }
    }

    /// The mask is not cosmetic: bits outside `0x0EEE` must not survive the store. (The bus refuses such
    /// a `raw` with `-32602` rather than reaching here, but a core primitive that silently kept the bits
    /// would make the reply's `value` a lie the moment anything else called it.)
    #[test]
    fn cram_poke_masks_to_nine_bits() {
        let mut v = fresh();
        assert_eq!(
            v.poke_cram(5, 0xFFFF, 0),
            0x0EEE,
            "every out-of-mask bit dropped"
        );
        assert_eq!(
            v.poke_cram(5, 0x0111, 0),
            0x0000,
            "only out-of-mask bits set → black"
        );
        assert_eq!(v.cram()[10..12], [0x00, 0x00]);
    }

    /// **`poke_cram` must not capture — pinned with the recorder ARMED**, which is the only arrangement
    /// in which the assertion means anything.
    ///
    /// This test exists because the bus-level version of it was **vacuous and looked airtight**. On the
    /// wire, `emulator/write_cram` requires a paused machine, and `capture_armed` is set only for the
    /// duration of a run (`System::run`, *"leave the VDP as the run found it"*) — so a paused poke cannot
    /// reach the watch surface no matter what this function does, and `tests/cram.rs`'s watch test went on
    /// passing when `poke_cram` was mutated to call `capture`. The property belongs to the primitive, so
    /// it is pinned on the primitive, with the recorder explicitly armed.
    ///
    /// The control beneath it is the point: the same armed recorder must catch the *port* path's write to
    /// the same entry. Without that, a `set_write_capture` that silently did nothing would satisfy the
    /// first half.
    #[test]
    fn poke_cram_never_captures_even_with_the_recorder_armed() {
        let mut v = fresh();
        v.set_write_capture(true);
        v.poke_cram(3, 0x0EEE, 0);
        assert!(
            v.take_write_captures().is_empty(),
            "a debug poke reached the watch surface — it has no instruction to name, and since \
             §11.15 no landing clock to supply"
        );

        // The control: the armed recorder is live, and the port path to the same entry proves it.
        let addr = 3u32 * 2;
        v.control_write((0xC000 | (addr & 0x3FFF)) as u16, 0);
        v.control_write((addr >> 14) as u16, 0);
        v.data_write(0x0EEE);
        let caps = v.take_write_captures();
        assert_eq!(
            caps.len(),
            1,
            "the recorder was armed and did catch the guest write"
        );
        assert_eq!(caps[0].target, VdpTarget::Cram);
        assert_eq!(caps[0].addr, addr);

        // And the CRAM-only narrowing arms the same way, so neither spelling of "armed" lets a poke through.
        v.set_write_capture_cram_only(true);
        v.poke_cram(3, 0x0246, 0);
        assert!(v.take_write_captures().is_empty());
    }

    /// A poke writes ONE entry. A neighbour-clobbering off-by-one in the byte index would be invisible to
    /// a single-entry assertion, so the two adjacent entries are pinned unchanged.
    #[test]
    fn cram_poke_touches_exactly_one_entry() {
        let mut v = fresh();
        v.poke_cram(0, 0x0EEE, 0);
        v.poke_cram(1, 0x0EEE, 0);
        v.poke_cram(2, 0x0EEE, 0);
        v.poke_cram(1, 0x000E, 0);
        assert_eq!(v.cram()[0..2], [0x0E, 0xEE], "entry 0 untouched");
        assert_eq!(v.cram()[2..4], [0x00, 0x0E], "entry 1 rewritten");
        assert_eq!(v.cram()[4..6], [0x0E, 0xEE], "entry 2 untouched");
    }

    #[test]
    fn ramp3_is_linear_across_the_eight_levels() {
        assert_eq!(ramp3(0), 0);
        assert_eq!(ramp3(7), 255);
        assert_eq!(ramp3(4), (4u16 * 255 / 7) as u8);
    }

    // --- SAT cache write-through (recon R5 / RR8) --------------------------------------------------------

    #[test]
    fn sat_base_masks_reg5_bit0_in_h40() {
        let mut v = fresh();
        v.regs[0x05] = 0x7F;
        assert_eq!(
            v.sat_base(),
            0x7F << 9,
            "H32 keeps reg5 bit 0 ($200 boundary)"
        );
        v.regs[0x0C] = 0x81; // H40
        assert_eq!(
            v.sat_base(),
            0x7E << 9,
            "H40 masks reg5 bit 0 ($400 boundary)"
        );
    }

    #[test]
    fn sat_cache_write_through_mirrors_only_the_cached_half() {
        let mut v = fresh();
        v.regs[0x05] = 0x10; // SAT base = 0x10 << 9 = 0x2000 (H32)
        v.code = 0x01; // VRAM write
        v.addr = 0x2000; // entry 0, Y word
        v.data_write(0x0142);
        assert_eq!(
            &v.sat_cache[0..4],
            &[0x01, 0x42, 0x00, 0x00],
            "Y word mirrored"
        );
        v.addr = 0x2002; // entry 0, size/link word
        v.data_write(0x0503);
        assert_eq!(
            &v.sat_cache[0..4],
            &[0x01, 0x42, 0x05, 0x03],
            "size/link mirrored"
        );
        v.addr = 0x2004; // entry 0, tile/attr word — the render-fetched half, NOT cached
        v.data_write(0xBEEF);
        assert_eq!(
            &v.sat_cache[0..4],
            &[0x01, 0x42, 0x05, 0x03],
            "tile/attr not cached"
        );
        v.addr = 0x1000; // outside the SAT window
        v.data_write(0xAAAA);
        assert!(
            v.sat_cache[4..].iter().all(|&b| b == 0),
            "out-of-window write ignored"
        );
    }

    #[test]
    fn sat_cache_write_through_is_byte_granular_on_odd_addresses() {
        // RR8 open-remainder 2: an odd-address VRAM write (byte-swapped, recon R3) updates the swapped cache
        // bytes — the write-through is byte-granular.
        let mut v = fresh();
        v.regs[0x05] = 0x10; // base 0x2000
        v.code = 0x01;
        v.addr = 0x2001; // odd → hi byte to 0x2001, lo byte to 0x2000
        v.data_write(0x1234);
        assert_eq!(
            &v.sat_cache[0..2],
            &[0x34, 0x12],
            "odd-address write updates the swapped cache bytes"
        );
    }

    /// ★ **The write-through window is all 80 slots in H40 and the first 64 in H32** (recon R5: `base+640`
    /// / `base+512`), through BOTH of this file's copies of the window rule: the data port
    /// (`write_vram_byte`) and [`Vdp::poke_vram`].
    ///
    /// Written because nothing tested the slots that tell the modes apart. Every other write-through test
    /// here uses slot 0, which is inside both windows, so mutating [`SAT_SLOTS`] from 80 to 79 reddened
    /// only `render.rs`'s decode-count test and left both `vdp.rs` sites unobserved (lens M62, measured
    /// 2026-09-11). The slot numbers here are R5's own, typed as literals, never read through the
    /// constant.
    #[test]
    fn the_sat_window_is_eighty_slots_in_h40_and_sixty_four_in_h32() {
        // Slot 79's Y word: the LAST slot of the H40 window. Base $2000, 8 bytes per slot.
        const SLOT_79: usize = 0x2000 + 79 * 8;
        // Slot 64's Y word: the first slot past the H32 window.
        const SLOT_64: usize = 0x2000 + 64 * 8;

        let mut port = fresh();
        port.regs[0x05] = 0x10; // SAT base $2000 (bit 0 clear, so H40's mask changes nothing)
        port.regs[0x0C] = 0x81; // H40
        port.code = 0x01; // VRAM write
        port.addr = SLOT_79 as u16;
        port.data_write(0x0142);
        let mut poked = fresh();
        poked.regs[0x05] = 0x10;
        poked.regs[0x0C] = 0x81;
        poked.poke_vram(SLOT_79, 0x01);
        poked.poke_vram(SLOT_79 + 1, 0x42);
        for (v, via) in [(&port, "the data port"), (&poked, "poke_vram")] {
            assert_eq!(
                &v.sat_cache[79 * 4..79 * 4 + 2],
                &[0x01, 0x42],
                "H40 must refresh slot 79, the last of its 80, through {via}"
            );
        }

        let mut port = fresh();
        port.regs[0x05] = 0x10;
        port.code = 0x01; // H32: reg 12 left at 0
        port.addr = SLOT_64 as u16;
        port.data_write(0x0142);
        let mut poked = fresh();
        poked.regs[0x05] = 0x10;
        poked.poke_vram(SLOT_64, 0x01);
        poked.poke_vram(SLOT_64 + 1, 0x42);
        for (v, via) in [(&port, "the data port"), (&poked, "poke_vram")] {
            assert_eq!(
                &v.sat_cache[64 * 4..64 * 4 + 2],
                &[0x00, 0x00],
                "H32's window is 64 slots, so slot 64 must NOT refresh through {via}"
            );
        }
    }

    /// **The drift guard named in [`Vdp::poke_vram`]'s doc comment.** The poke duplicates the SAT-window
    /// arithmetic rather than sharing `write_vram_byte`'s, because that function is on the currency path;
    /// this is the test that stops the two copies from diverging. Two machines, the same four SAT bytes,
    /// one written through the data port and one poked: VRAM and the cache must agree byte for byte.
    ///
    /// The addresses are chosen to exercise the whole window rule — the cached half (bytes 0–3), the
    /// render-fetched half (bytes 4–7, which must NOT reach the cache), and a byte outside the window.
    #[test]
    fn vram_poke_matches_the_port_path() {
        const BASE: usize = 0x2000; // reg5 0x10, H32
        let bytes: [(usize, u8); 6] = [
            (BASE, 0x01),     // entry 0, Y hi — cached
            (BASE + 1, 0x42), // entry 0, Y lo — cached
            (BASE + 3, 0x05), // entry 0, link — cached
            (BASE + 4, 0xBE), // entry 0, tile/attr — NOT cached
            (BASE + 8, 0x77), // entry 1, Y hi — cached
            (0x1000, 0xAA),   // outside the window entirely
        ];

        let mut port = fresh();
        port.regs[0x05] = 0x10;
        for (a, b) in bytes {
            port.write_vram_byte(a, b);
        }

        let mut poked = fresh();
        poked.regs[0x05] = 0x10;
        for (a, b) in bytes {
            poked.poke_vram(a, b);
        }

        assert_eq!(port.vram, poked.vram, "VRAM must land identically");
        assert_eq!(
            port.sat_cache, poked.sat_cache,
            "the SAT-cache write-through must land identically — the two copies of the window \
             arithmetic have drifted"
        );
        // Not a vacuous comparison of two untouched arrays: the poke really did move the cache.
        assert_eq!(
            &poked.sat_cache[0..4],
            &[0x01, 0x42, 0x00, 0x05],
            "the cached half followed the poke"
        );
    }

    /// A poke is a debugger access: it must not reach the watch surface, because a hit's `pc` names the
    /// instruction that drove the access and a poke has none to name. The control is the port path, whose
    /// identical write DOES capture — so a capture list that is empty for both would fail this test.
    #[test]
    fn a_vram_poke_is_never_offered_to_the_watch_surface() {
        let mut v = fresh();
        v.set_write_capture(true);
        v.poke_vram(0x1234, 0x5A);
        assert!(
            v.take_write_captures().is_empty(),
            "a poke must not be captured"
        );

        let mut control = fresh();
        control.set_write_capture(true);
        control.write_vram_byte(0x1234, 0x5A);
        assert_eq!(
            control.take_write_captures().len(),
            1,
            "the control proves capture was armed and the byte choke does capture"
        );
    }

    /// The address is refused by the bus, never wrapped here — `poke_vram` asserts rather than masking,
    /// so a server bug surfaces as a panic instead of a silent write to the wrong end of VRAM.
    #[test]
    #[should_panic(expected = "outside the 64K space")]
    fn a_vram_poke_past_the_end_panics_rather_than_wrapping() {
        let mut v = fresh();
        v.poke_vram(VRAM_SIZE, 0xFF);
    }

    #[test]
    fn changing_reg5_does_not_invalidate_the_cache() {
        // Recon R5 / Castlevania Bloodlines: reg5 changes never reload/invalidate the cache.
        let mut v = fresh();
        v.regs[0x05] = 0x10; // base 0x2000
        v.code = 0x01;
        v.addr = 0x2000;
        v.data_write(0x0142); // cache entry 0 Y = 0x0142
        v.regs[0x05] = 0x20; // move the SAT base to 0x4000
        assert_eq!(
            &v.sat_cache[0..2],
            &[0x01, 0x42],
            "the stale cache is kept across a reg5 change"
        );
        v.addr = 0x2000; // a write at the OLD base no longer hits the (moved) window
        v.data_write(0x0999);
        assert_eq!(
            &v.sat_cache[0..2],
            &[0x01, 0x42],
            "old-base writes miss the window"
        );
        v.addr = 0x4000; // a write at the NEW base does update the cache
        v.data_write(0x0777);
        assert_eq!(
            &v.sat_cache[0..2],
            &[0x07, 0x77],
            "new-base writes update the cache"
        );
    }

    #[test]
    fn sat_cache_and_carry_survive_a_bincode_round_trip() {
        let mut v = fresh();
        v.sat_cache[7] = 0xAB;
        v.sat_cache[SAT_CACHE_LEN - 1] = 0xCD;
        v.sprite_dot_overflow_carry = true;
        let bytes = bincode::encode_to_vec(&v, bincode::config::standard()).unwrap();
        let (back, _): (Vdp, usize) =
            bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
        assert_eq!(v, back, "the SAT cache + carry round-trip");
        assert_eq!(back.sat_cache[7], 0xAB);
        assert_eq!(back.sat_cache[SAT_CACHE_LEN - 1], 0xCD);
        assert!(back.sprite_dot_overflow_carry);
    }

    /// Set up a completed VRAM write command @ `addr` with autoinc 2 (recon R1), leaving the toggle disarmed.
    fn vram_write_cmd(v: &mut Vdp, addr: u16) {
        v.control_write(0x8F02, 0); // reg 15 = autoinc 2
        v.control_write(0x4000 | (addr & 0x3FFF), 0); // first word: CD1-0 = 01 (VRAM write), A13-A0
        v.control_write((addr >> 14) & 0x03, 0); // second word: A15-A14, CD5-2 = 0
    }

    #[test]
    fn fifo_records_data_code_addr_at_enqueue() {
        let mut v = fresh();
        vram_write_cmd(&mut v, 0x0100);
        v.data_write(0xBEEF);
        let newest = (v.fifo_write.wrapping_sub(1) & 3) as usize;
        assert_eq!(v.fifo[newest].data, 0xBEEF, "data word captured");
        assert_eq!(v.fifo[newest].code & 0x0F, 0x01, "VRAM-write code captured");
        assert_eq!(
            v.fifo[newest].addr, 0x0100,
            "pre-autoincrement address captured (recon R3)"
        );
        assert_eq!(v.fifo_len, 1, "one pending entry");
    }

    /// ★ **The drain retires the OLDEST pending write** (recon R3), pinned at the one helper all three
    /// retirement sites read (lens M43).
    ///
    /// Written because the gate the dedupe was briefed against could not see it. Mutating
    /// [`Vdp::fifo_oldest`] to return `fifo[fifo_write]` (the next-available slot, i.e. the wrong entry)
    /// left `oracle-core --lib` 918/0 and the FIFO conformance ROMs 3/0 GREEN, with the patch compiled.
    /// Nothing in the suite queued entries whose drain costs differ in an order that exposes which one is
    /// retired first. The expectations here are the words this test wrote, never a value read back
    /// through the helper.
    ///
    /// Two shapes, because a ring hides an index error at exactly one fill level: with four pending, the
    /// oldest slot and the next-available slot coincide, so a full FIFO cannot tell them apart.
    #[test]
    fn the_fifo_retires_its_oldest_pending_write_first() {
        // Three pending of four: the oldest is the first word written.
        let mut v = fresh();
        vram_write_cmd(&mut v, 0x0100);
        for w in [0x1111, 0x2222, 0x3333] {
            v.data_write(w);
        }
        assert_eq!(v.fifo_len(), 3, "three writes are three pending entries");
        assert_eq!(
            v.fifo_oldest().data,
            0x1111,
            "the oldest pending entry is the first word written"
        );

        // Across the ring's wrap: six writes land in slots 0,1,2,3,0,1. As if the drain had retired four,
        // two are pending, and the older of those two is the FIFTH write.
        let mut v = fresh();
        vram_write_cmd(&mut v, 0x0100);
        for w in [0x1111, 0x2222, 0x3333, 0x4444, 0x5555, 0x6666] {
            v.data_write(w);
        }
        v.fifo_len = 2;
        assert_eq!(
            v.fifo_oldest().data,
            0x5555,
            "with two pending after a wrap, the oldest is the fifth write, not the slot about to be reused"
        );
    }

    #[test]
    fn fifo_and_dma_fields_survive_a_bincode_round_trip() {
        let mut v = fresh();
        vram_write_cmd(&mut v, 0x0200);
        v.data_write(0x1234);
        v.data_write(0x5678);
        v.dma_busy_until = 42_000;
        v.last_dma = Some(DmaRecord {
            mode: DmaMode::Mem,
            source: 0x1234,
            dest: 0xC000,
            len: 16,
            target: VdpTarget::Vram,
        });
        let bytes = bincode::encode_to_vec(&v, bincode::config::standard()).unwrap();
        let (back, _): (Vdp, usize) =
            bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
        assert_eq!(
            v, back,
            "the FIFO ring + DMA-busy deadline + last-DMA record round-trip"
        );
        assert_eq!(back.fifo_len, 2);
        assert_eq!(back.dma_busy_until, 42_000);
        assert_eq!(back.last_dma.unwrap().len, 16);
    }

    #[test]
    fn power_on_fifo_is_empty_and_busy_clear() {
        let v = fresh();
        assert_eq!(v.fifo_len(), 0, "FIFO empty at power-on");
        assert_eq!(v.dma_busy_until, 0);
        assert!(!v.dma_busy(0), "not busy at power-on");
        assert_eq!(v.status_word(0) & (1 << 1), 0, "status DMA-busy bit clear");
    }

    #[test]
    fn enqueue_still_writes_vram_immediately() {
        // The enqueue-immediate model (currency-neutral): the FIFO tracks contents/timing but VRAM is written
        // exactly as before, so VRAM/CRAM/VSRAM/regs stay byte-identical.
        let mut v = fresh();
        vram_write_cmd(&mut v, 0x0100);
        v.data_write(0xBEEF);
        assert_eq!(v.vram[0x0100], 0xBE, "high byte written immediately");
        assert_eq!(v.vram[0x0101], 0xEF, "low byte written immediately");
        assert_eq!(
            v.fifo_snoop_word(),
            0x0000,
            "snoop slot still the pre-write default"
        );
    }

    #[test]
    fn cram_read_snoops_undefined_bits_from_next_available_fifo_entry() {
        let mut v = fresh();
        // Load the FIFO so the next-available slot holds a known word (recon R3: "4 writes ago").
        vram_write_cmd(&mut v, 0x0000);
        for w in [0xF00Du16, 0x1111, 0x2222, 0x3333] {
            v.data_write(w);
        }
        assert_eq!(
            v.fifo_snoop_word(),
            0xF00D,
            "next-available entry = the first of 4 writes"
        );
        // Give CRAM[0] a known (masked) value, then arm a CRAM read @ 0 (code 0x08 → prefills read_buffer).
        v.cram[0] = 0x0A;
        v.cram[1] = 0xA0;
        v.control_write(0x0000, 0); // word1: CD1-0 = 00 (read low), addr 0
        v.control_write(0x0020, 0); // word2: CD5-2 = 0010 → CRAM read (code 0x08)
        assert_eq!(v.code, 0x08, "CRAM read command armed");
        let out = v.data_read(0xABCD);
        let expected = (0x0AA0 & 0x0EEE) | (0xF00D & !0x0EEE);
        assert_eq!(
            out, expected,
            "CRAM read = defined bits | snooped undefined bits"
        );
    }

    // --- The 8-bit VRAM read target, CD = %001100 (slice A4; VDPFIFOTesting test 6) ----------------------

    /// Arm the undocumented 8-bit VRAM read (code `$0C`) at VRAM `addr` with autoinc `inc`, the way the
    /// ROM does it (control words `$00xx` / `$0032`; ROM $DF72 and friends).
    fn vram_byte_read_cmd(v: &mut Vdp, addr: u16, inc: u8) {
        v.control_write(0x8F00 | inc as u16, 0);
        v.control_write(addr & 0x3FFF, 0); // first word: CD1-0 = 00, A13-A0
        v.control_write(0x0030 | ((addr >> 14) & 0x03), 0); // second word: CD5-2 = 0011, A15-A14
        assert_eq!(v.code, 0x0C, "8-bit VRAM read command armed");
    }

    #[test]
    fn eight_bit_vram_read_takes_the_low_byte_from_address_xor_one() {
        // VDPFIFOTesting test 6 (expected table ROM $DED4), group 5: autoinc 1 from $8000 over the image
        // `11 22 33 44` reads $22, $11, $44, $33 — i.e. `vram[address ^ 1]`, the same byte-lane swap A3b
        // pinned for the fill engine (Eke, *Is DMA Fill buggy?*: "VRAM byte writes … actually occur to
        // VRAM address ^ 1"). A plain `vram[address]` would give $11, $22, $33, $44.
        let mut v = fresh();
        v.vram[0x8000..0x8004].copy_from_slice(&[0x11, 0x22, 0x33, 0x44]);
        vram_byte_read_cmd(&mut v, 0x8000, 1);
        let out: Vec<u8> = (0..4).map(|_| v.data_read(0xABCD) as u8).collect();
        assert_eq!(
            out,
            vec![0x22, 0x11, 0x44, 0x33],
            "low byte = vram[addr ^ 1]"
        );
        assert_eq!(v.addr, 0x8004, "the address auto-increments normally");
    }

    #[test]
    fn eight_bit_vram_read_takes_the_high_byte_from_the_fifo_snoop() {
        // Test 6's groups 1-4: the high half is NOT VRAM at all — it is the high byte of the
        // next-available FIFO entry, the same stale word the CRAM/VSRAM undefined bits snoop (Nemesis,
        // *VDP Internals*: undefined bits "are actually initialized to the content on the next available
        // FIFO entry (the one containing the data written to control port four writes ago)").
        let mut v = fresh();
        vram_write_cmd(&mut v, 0x8000);
        for w in [
            0x1122u16, 0x3344, 0x5566, 0x7788, 0x99AA, 0xBBCC, 0xDDEE, 0x1234,
        ] {
            v.data_write(w);
        }
        assert_eq!(
            v.fifo_snoop_word(),
            0x99AA,
            "cursor parked four writes back"
        );
        vram_byte_read_cmd(&mut v, 0x8000, 2);
        assert_eq!(v.data_read(0xABCD), 0x9922, "snoop MSB | vram[$8001]");
        assert_eq!(
            v.data_read(0xABCD),
            0x9944,
            "a read does NOT advance the cursor"
        );
    }

    #[test]
    fn eight_bit_vram_read_high_byte_walks_with_the_ring_cursor() {
        // Test 6's ring-advancing $FFFF CRAM writes: each one moves the snoop on by one slot, so the
        // read's high byte walks $99 → $BB → $DD → $12 while the VRAM image is untouched.
        let mut v = fresh();
        vram_write_cmd(&mut v, 0x8000);
        for w in [
            0x1122u16, 0x3344, 0x5566, 0x7788, 0x99AA, 0xBBCC, 0xDDEE, 0x1234,
        ] {
            v.data_write(w);
        }
        let mut highs = Vec::new();
        for (i, addr) in [0x8000u16, 0x8004, 0x8008, 0x800C].into_iter().enumerate() {
            vram_byte_read_cmd(&mut v, addr, 2);
            highs.push((v.data_read(0xABCD) >> 8) as u8);
            // One CRAM write advances the ring cursor by one slot (ROM $DF9C). `$C020` + `$0002` sets
            // A15-A14 = 10, i.e. address $8020, which the CRAM write path masks to $20 — the ROM's fourth
            // group uses `$0000` for the second word and so addresses $0020 directly. Either way the point
            // is the same: one enqueued word, one slot of cursor travel.
            ctrl(&mut v, &[0xC020, if i == 3 { 0x0000 } else { 0x0002 }]);
            v.data_write(0xFFFF);
        }
        assert_eq!(
            highs,
            vec![0x99, 0xBB, 0xDD, 0x12],
            "ROM $DED4 words 0/2/4/6 ($9922 $BB66 $DDAA $12EE) — the high byte tracks the snoop cursor"
        );
    }

    #[test]
    fn eight_bit_vram_read_does_not_change_the_write_path() {
        // A4 changes the READ path only. Code $0C's low nibble names no *write* target, so a data write
        // under it is accepted into the FIFO and steps the address but reaches no memory (the A2
        // invalid-target rule; VDPFIFOTesting test 5 "FIFO Write to invalid target" passes today and must
        // stay passing). Guards that the byte-read carve-out did not leak into `write_target`, into the
        // FIFO accounting, or into the drain-cost model.
        let mut v = fresh();
        v.vram[0x0100] = 0x5A;
        v.vram[0x0101] = 0xA5;
        vram_byte_read_cmd(&mut v, 0x0100, 2);
        // The drain cost is decided by `target_of`, whose `_ => Vram` fallback still claims code $0C — so a
        // $0C entry costs a VRAM word's two slots, unchanged by A4 (H32 blanked: 167 slots/line).
        assert_eq!(
            v.entry_drain_cost(0x0C, 0),
            2 * MCLK_PER_LINE / 167,
            "code $0C still drains at the VRAM word rate (2 slots)"
        );
        v.data_write(0xBEEF);
        assert_eq!(
            (v.vram[0x0100], v.vram[0x0101]),
            (0x5A, 0xA5),
            "a write under code $0C reaches no memory (invalid write target)"
        );
        assert_eq!(v.addr, 0x0102, "but the address still steps");
        assert_eq!(v.fifo_len(), 1, "and it still occupies a pending FIFO slot");
        let newest = (v.fifo_write.wrapping_sub(1) & 3) as usize;
        assert_eq!(
            (v.fifo[newest].data, v.fifo[newest].code),
            (0xBEEF, 0x0C),
            "the ring slot holds the word and the code that wrote it"
        );
    }

    #[test]
    fn eight_bit_vram_read_buffer_degrades_to_the_plain_word_when_the_code_changes() {
        // S2-3 seam. The pre-cache is filled when the read command completes, but `data_read` re-decides
        // whether to merge the snoop from `self.code` at CONSUME time, and A2's pinned rule makes the two
        // disagree: a `$8xxx` register write always latches CD1-CD0 from its top bits `10`, so arming $0C
        // and then writing any register leaves code $0E — not a byte read. `read_target` therefore keeps
        // the REAL VRAM high byte in the buffer rather than a fabricated zero, so this path returns the
        // plain VRAM word $1122 (exactly the pre-A4 result) instead of a value invented by A4.
        //
        // The ROM is silent here — test 6 never writes between arming and consuming — so this is pinned as
        // "preserve prior behaviour", not as hardware truth; see follow-up F-SNOOPWHEN.
        let mut v = fresh();
        v.vram[0x8000] = 0x11;
        v.vram[0x8001] = 0x22;
        // Load the ring so a snoop merge, if it happened, would be loudly visible ($EEEE, not $0000).
        vram_write_cmd(&mut v, 0x0000);
        for w in [0xEEEEu16, 0x1111, 0x2222, 0x3333] {
            v.data_write(w);
        }
        assert_eq!(v.fifo_snoop_word(), 0xEEEE, "snoop word primed");

        vram_byte_read_cmd(&mut v, 0x8000, 2);
        v.control_write(0x8F02, 0); // any `$8xxx` register write → code $0C becomes $0E
        assert_eq!(v.code, 0x0E, "the register write clobbered CD1-CD0");
        assert!(
            !Vdp::is_vram_byte_read(v.code),
            "so the consume-time merge does not fire"
        );
        assert_eq!(
            v.data_read(0xABCD),
            0x1122,
            "degrades to the plain VRAM word — the pre-A4 result, not $EE22 and not $0022"
        );
    }

    #[test]
    fn status_fifo_flags_clear_with_one_pending_entry() {
        // A1 (VDPFIFOTesting T16): one pending entry during active display → neither EMPTY (bit 9) nor
        // FULL (bit 8). Active H32 = 16 slots/line, a VRAM word costs 2 slots ≈ 427 mclk — at +10 mclk
        // nothing has drained yet.
        let mut v = fresh();
        v.control_write(0x8140, 0); // display on → active-line (slow) drain rate
        vram_write_cmd(&mut v, 0x0100);
        let t0 = 500; // line 0, active display
        v.data_write_at(0xBEEF, t0);
        let s = v.control_read_status(0, t0 + 10);
        assert_eq!(s & (1 << 9), 0, "EMPTY clear with a pending entry");
        assert_eq!(s & (1 << 8), 0, "FULL clear with only one pending entry");
    }

    #[test]
    fn status_fifo_full_with_four_pending_entries() {
        let mut v = fresh();
        v.control_write(0x8140, 0);
        vram_write_cmd(&mut v, 0x0100);
        let t0 = 500;
        for w in [0x1111u16, 0x2222, 0x3333, 0x4444] {
            v.data_write_at(w, t0);
        }
        let s = v.control_read_status(0, t0 + 10);
        assert_ne!(s & (1 << 8), 0, "FULL set with all 4 slots pending");
        assert_eq!(s & (1 << 9), 0, "EMPTY clear while full");
    }

    #[test]
    fn status_read_drains_the_fifo_to_now_and_reports_empty() {
        // After all four entries' slot costs elapse (4 × 427 ≈ 1708 mclk), a status read drains to `now`
        // and reports EMPTY again.
        let mut v = fresh();
        v.control_write(0x8140, 0);
        vram_write_cmd(&mut v, 0x0100);
        let t0 = 500;
        for w in [0x1111u16, 0x2222, 0x3333, 0x4444] {
            v.data_write_at(w, t0);
        }
        let s = v.control_read_status(0, t0 + 2000);
        assert_ne!(s & (1 << 9), 0, "EMPTY set once every entry has drained");
        assert_eq!(s & (1 << 8), 0, "FULL clear again");
        assert_eq!(v.fifo_len(), 0, "the drain really popped all four entries");
    }

    #[test]
    fn repeated_status_reads_do_not_consume_fifo_entries() {
        // A status read must NOT pop entries beyond the normal time-based drain: five reads at the same
        // instant leave the FIFO exactly as one read would.
        let mut v = fresh();
        v.control_write(0x8140, 0);
        vram_write_cmd(&mut v, 0x0100);
        let t0 = 500;
        for w in [0x1111u16, 0x2222, 0x3333, 0x4444] {
            v.data_write_at(w, t0);
        }
        for _ in 0..5 {
            let s = v.control_read_status(0, t0 + 10);
            assert_ne!(s & (1 << 8), 0, "still FULL: nothing has drained at +10");
        }
        assert_eq!(v.fifo_len(), 4, "status reads never pop entries themselves");
        // After one entry's drain cost (427 mclk = 2 slots for a VRAM word; a slot itself is 3420/16 ≈ 213
        // mclk here) exactly one entry has drained, no matter how many reads probe it.
        for _ in 0..5 {
            v.control_read_status(0, t0 + 500);
        }
        assert_eq!(v.fifo_len(), 3, "only the time-based drain moved the FIFO");
    }

    // --- Intra-line access-slot positions (slice T16/S1) --------------------------------------------------

    /// Expand Kabuto's published per-line access-pattern string into its literal access sequence, exactly
    /// as written in the hardware notes (Plutiedev mirror). Deliberately spelled out rather than parsed:
    /// the point is that a reader can diff this function against the quoted string character by character.
    fn kabuto_pattern(h40: bool) -> String {
        let mut s = String::new();
        s.push_str("Hssss"); // `Hssss`
        s.push_str("AsaaBsbb"); // `AsaaBsbb`
                                // `((A~aaBSbb)*3 AraaBSbb)*5` (H40) / `*4` (H32)
        for _ in 0..if h40 { 5 } else { 4 } {
            for _ in 0..3 {
                s.push_str("A~aaBSbb");
            }
            s.push_str("AraaBSbb");
        }
        if h40 {
            // `~~ s*23 ~ s*11`
            s.push_str("~~");
            s.push_str(&"s".repeat(23));
            s.push('~');
            s.push_str(&"s".repeat(11));
        } else {
            // `~~ s*13 ~ s*13 ~`
            s.push_str("~~");
            s.push_str(&"s".repeat(13));
            s.push('~');
            s.push_str(&"s".repeat(13));
            s.push('~');
        }
        s
    }

    #[test]
    fn active_slot_gaps_follow_the_published_pattern() {
        // T16/S1, and the guard on the whole slice: the slot-index tables are transcribed from Kabuto's
        // access-pattern strings, so re-expand those strings here and check the constants against them.
        // A typo in an index would otherwise only surface three layers downstream as a moved frame hash.
        //
        //   H40: Hssss AsaaBsbb ((A~aaBSbb)*3 AraaBSbb)*5 ~~ s*23 ~ s*11
        //   H32: Hssss AsaaBsbb ((A~aaBSbb)*3 AraaBSbb)*4 ~~ s*13 ~ s*13 ~
        //
        // The counts are the independent cross-check: they must reproduce the Sega *Genesis Technical
        // Overview* DMA-capacity figures (18 external slots per H40 active line, 16 per H32 —
        // `docs/2026-07-16-vdp-recon.md:109`) and the refresh counts Kabuto's own text states.
        for (h40, accesses, externals, refreshes, table) in [
            (
                true,
                Vdp::H40_ACCESSES_PER_LINE,
                18,
                5,
                &Vdp::H40_ACTIVE_SLOTS[..],
            ),
            (
                false,
                Vdp::H32_ACCESSES_PER_LINE,
                16,
                4,
                &Vdp::H32_ACTIVE_SLOTS[..],
            ),
        ] {
            let p = kabuto_pattern(h40);
            let label = if h40 { "H40" } else { "H32" };
            assert_eq!(p.len() as u64, accesses, "{label}: accesses per line");
            assert_eq!(
                p.matches('~').count(),
                externals,
                "{label}: external slots per active line"
            );
            assert_eq!(
                p.matches('r').count(),
                refreshes,
                "{label}: VRAM refresh slots per active line"
            );
            let from_pattern: Vec<u64> = p
                .char_indices()
                .filter(|&(_, c)| c == '~')
                .map(|(i, _)| i as u64)
                .collect();
            assert_eq!(
                from_pattern, table,
                "{label}: the slot-index constant matches the published pattern"
            );
        }
    }

    #[test]
    fn active_slots_are_irregularly_spaced_not_a_uniform_period() {
        // The whole content of T16 groups 2/3/5/6/8: hardware's external slots are NOT evenly spread, so a
        // VRAM word's two-slot drain costs a different amount depending where in the line it starts. The
        // old uniform model returned an invariant 2 × 3420/18 = 380 mclk everywhere.
        let mut v = fresh();
        // Reg 1 = $44 is mode 5 + display on. M5 must stay SET or the reg-12 write below is discarded by
        // the Mode-4 register mask (slice T12) and the VDP silently stays in H32.
        v.control_write(0x8144, 0);
        v.control_write(0x8C81, 0);
        // Slot instants are `k × 3420 / 210`: the first two are 228 and 358, and the last is 3224.
        assert_eq!(
            v.next_active_slot(0),
            228,
            "first slot of an active H40 line"
        );
        assert_eq!(v.next_active_slot(228), 358, "the 8-access gap = 130 mclk");
        assert_eq!(
            v.next_active_slot(3224),
            3420 + 228,
            "wraps to the next line"
        );
        // A VRAM word costs two slots, so its drain spans the gap sequence from wherever it starts. The
        // cheapest case is a drain that begins ON a slot instant and takes a render group's two close
        // slots (t14 → t30 = 488 − 228 = 260); the dearest starts just past the line's last slot so both
        // its slots come from the next line (648). Against the old invariant 380 that is a −120/+268
        // spread, and it is the whole of T16 groups 2/3/5/6/8.
        let vram_write = 0x01;
        assert_eq!(
            v.entry_drain_cost(vram_write, 228),
            260,
            "cheapest: starting on a slot instant, two close slots"
        );
        assert_eq!(
            v.entry_drain_cost(vram_write, 3000),
            648,
            "dearest: past the line's last slot, so both slots come from the next line"
        );
        assert_eq!(
            v.entry_drain_cost(vram_write, 0),
            358,
            "mid-range: mclk 0 is not itself a slot instant"
        );
        // A CRAM word costs one slot, and it too follows the schedule.
        let cram_write = 0x03;
        assert_eq!(v.entry_drain_cost(cram_write, 0), 228);
    }

    #[test]
    fn a_full_fifo_write_stalls_to_the_next_real_slot_not_a_uniform_period() {
        // The /DTACK stall a 5th write takes is now read off the schedule, which is what unlocks T16: the
        // stall lands the resuming 68k on a real slot instant, and the *following* gap differs by phase
        // instead of being a constant 380 mclk. Two phases, two different stalls.
        let stall_at = |t0: u64| -> u32 {
            let mut v = fresh();
            v.control_write(0x8144, 0); // mode 5 + display on
            v.control_write(0x8C81, 0); // H40
            vram_write_cmd(&mut v, 0x8000);
            for w in [0x1111u16, 0x2222, 0x3333, 0x4444] {
                v.data_write_at(w, t0);
            }
            v.data_write_at(0x5555, t0) // the 5th write into a full FIFO
        };
        let (early, late) = (stall_at(100), stall_at(2900));
        assert!(early > 0 && late > 0, "a full FIFO always stalls the 68k");
        assert_ne!(
            early, late,
            "the stall depends on where in the line the FIFO filled — it is not a uniform period"
        );
    }

    /// **C2 — the FIFO double-charge.** A burst of data-port writes inside ONE 68000 instruction hands
    /// `data_write_at` the same frozen `now` every time (`MegaDriveBus::now_mclk` is taken by value for the
    /// whole instruction; only `System::run_until` advances the clock, by the retiring instruction's total
    /// cost). `fifo_slot_clock` meanwhile advances one drain per stalling write. Measuring every write's
    /// wait as `drain_at - now` therefore re-charges, inside write *k*, the whole stall writes 1..k-1 were
    /// already billed for — and the bus SUMS those waits (`MegaDriveBus::stall_cycles`) into one
    /// instruction cost. The CPU is held off the bus **once**, from `now` to the last drain instant.
    ///
    /// Derivation of the expected charge, entirely from constants in this file / `system.rs`:
    ///
    /// * H40 (reg 12 bit 0) with the display OFF (`fresh()` leaves reg 1 bit 6 clear) → `entry_drain_cost`
    ///   takes its **blanked** branch, the position-independent closed form
    ///   `slots * MCLK_PER_LINE / slots_per_line(at)` — the same branch
    ///   `blanked_lines_keep_the_aggregate_slot_rate` pins.
    /// * `Vdp::slots_per_line` → `(h40 = true, blanked = true)` = **205**.
    /// * `Vdp::entry_drain_cost` → `VdpTarget::Vram => 2` slots.
    /// * [`MCLK_PER_LINE`] = **3420**.
    ///
    /// so every entry drains in `COST = 2 * 3420 / 205 = 33` mclk, at every instant.
    ///
    /// Ten writes at one frozen `now`: the first four fill the empty FIFO for free, and each of the six
    /// after that forces exactly one drain. The 68k resumes at `now + 6*COST`, and each of those six bus
    /// accesses is extended by `COST` mclk = `div_ceil(COST, MCLK_PER_CPU_CYCLE)` whole CPU cycles (a
    /// /DTACK-extended 68000 bus cycle ends on a cycle boundary, which is why the rounding is per access
    /// and not once over the burst — the pre-existing convention on both port paths).
    ///
    /// The pre-fix code billed write *k* the full `k*COST` span from the frozen `now`, i.e.
    /// `COST*(1+2+...+6) = 21*COST = 693` mclk → 102 CPU cycles against a true 198 mclk → 30 cycles: the
    /// first stall charged six times over, a 3.4× over-bill on this shape.
    #[test]
    fn a_burst_of_stalling_writes_at_one_instant_bills_each_drain_once() {
        const SLOTS_PER_LINE_H40_BLANKED: u64 = 205; // Vdp::slots_per_line, (h40, blanked)
        const VRAM_SLOTS_PER_WORD: u64 = 2; // Vdp::entry_drain_cost, VdpTarget::Vram
        let cost = VRAM_SLOTS_PER_WORD * MCLK_PER_LINE / SLOTS_PER_LINE_H40_BLANKED;
        assert_eq!(cost, 33, "closed-form blanked drain cost");

        let now = 250 * MCLK_PER_LINE + 700; // any instant: the blanked branch is position-independent
        let mut v = fresh(); // reg 1 bit 6 clear → display off → blanked
        v.control_write(0x8C81, 0); // reg 12 = $81 → H40
        vram_write_cmd(&mut v, 0x8000);

        let mut billed = 0u32;
        for i in 0..10u16 {
            billed += v.data_write_at(0x1000 + i, now);
        }

        let stalling_writes = 10 - 4; // the first four fill the empty FIFO without stalling
        assert_eq!(
            v.fifo_slot_clock,
            now + stalling_writes * cost,
            "six forced drains, so the 68k resumes {stalling_writes} drains past the frozen instant"
        );
        // The model-level law first, stated without reference to any number above so it can fail on its own:
        // the billed time covers the span from `now` to the resume instant and overshoots it by at most the
        // per-access rounding — under one whole CPU cycle for each extended bus access. Pre-fix this read
        // 714 mclk billed against a 198 mclk hold, a 516 mclk overshoot against a 36 mclk budget.
        let billed_mclk = billed as u64 * crate::system::MCLK_PER_CPU_CYCLE;
        let span = v.fifo_slot_clock - now;
        let rounding_budget = stalling_writes * (crate::system::MCLK_PER_CPU_CYCLE - 1);
        assert!(
            billed_mclk >= span && billed_mclk - span <= rounding_budget,
            "billed {billed_mclk} mclk against a {span} mclk hold (rounding budget {rounding_budget}) — \
             that is not rounding, it is double-charging"
        );
        // Then the exact pin.
        assert_eq!(
            billed,
            (stalling_writes * cost.div_ceil(crate::system::MCLK_PER_CPU_CYCLE)) as u32,
            "each stalling write bills only the drain IT forced — the burst is not re-charged per write"
        );
    }

    /// Companion to the burst test with **both** parameters moved: a CRAM target (1 slot per word, not 2)
    /// and H32 (167 blanked slots per line, not 205), driven eight writes deep rather than ten. Same law.
    /// `COST = 1 * 3420 / 167 = 20` mclk, four stalling writes.
    #[test]
    fn the_burst_law_holds_for_a_cram_target_in_h32_too() {
        const SLOTS_PER_LINE_H32_BLANKED: u64 = 167; // Vdp::slots_per_line, (!h40, blanked)
        const CRAM_SLOTS_PER_WORD: u64 = 1; // Vdp::entry_drain_cost, `_ => 1`
        let cost = CRAM_SLOTS_PER_WORD * MCLK_PER_LINE / SLOTS_PER_LINE_H32_BLANKED;
        assert_eq!(cost, 20, "closed-form blanked drain cost, CRAM word, H32");

        let now = 3 * MCLK_PER_LINE + 55;
        let mut v = fresh(); // display off → blanked; reg 12 left at 0 → H32
        v.control_write(0x8F02, 0); // reg 15 = autoinc 2
        v.control_write(0xC000, 0); // CRAM write @ $0000, word 1
        v.control_write(0x0000, 0); // word 2 → code $03
        assert_eq!(v.code, 0x03, "CRAM write command armed");

        let mut billed = 0u32;
        for i in 0..8u16 {
            billed += v.data_write_at(0x2000 + i, now);
        }
        let stalling_writes = 8 - 4;
        assert_eq!(v.fifo_slot_clock, now + stalling_writes * cost);
        assert_eq!(
            billed,
            (stalling_writes * cost.div_ceil(crate::system::MCLK_PER_CPU_CYCLE)) as u32
        );
    }

    #[test]
    fn blanked_lines_keep_the_aggregate_slot_rate() {
        // Deliberate scope line (follow-up F-BLANKSLOT): S1 changes active-display drains only. On a
        // blanked line nearly every access is an external slot, so positions carry almost no information,
        // and it is the path every real game's bulk VDP traffic takes.
        let mut v = fresh();
        v.control_write(0x8C81, 0); // H40, display OFF → blanked
        let vram_write = 0x01;
        let blanked = 2 * MCLK_PER_LINE / 205;
        assert_eq!(v.entry_drain_cost(vram_write, 0), blanked);
        assert_eq!(
            v.entry_drain_cost(vram_write, 3000),
            blanked,
            "position-independent while blanked"
        );
        // Display on but inside vblank is blanked too.
        v.control_write(0x8144, 0);
        assert_eq!(v.entry_drain_cost(vram_write, 240 * MCLK_PER_LINE), blanked);
    }

    #[test]
    fn vram_read_is_fully_defined_no_snoop() {
        let mut v = fresh();
        // FIFO garbage in the snoop slot.
        vram_write_cmd(&mut v, 0x1000);
        for w in [0xDEADu16, 0xBEEF, 0xCAFE, 0xF00D] {
            v.data_write(w);
        }
        // Put a known word at VRAM $0000, then arm a VRAM read there.
        v.vram[0] = 0x12;
        v.vram[1] = 0x34;
        v.control_write(0x0000, 0); // VRAM read @ 0 (code 0x00)
        v.control_write(0x0000, 0);
        assert_eq!(v.code, 0x00, "VRAM read command");
        assert_eq!(
            v.data_read(0xABCD),
            0x1234,
            "VRAM read fully defined — no snoop"
        );
    }

    // --- F-COPYXOR / lens M22: VRAM copy reads AND writes the opposite byte lane --------------------------
    //
    // The rule under test (see `Vdp::run_copy`): copy step `i` reads `vram[(source + i) ^ 1]` and writes it to
    // `(dest + i * reg15) ^ 1`. Every expected image below is derived BY HAND from that rule, step by step in
    // the comment, never read back from the implementation. Each case also carries the three images the
    // wrong models produce (read half dropped, write half dropped, both dropped = the pre-fix code), also
    // derived by hand, so a red names the missing half instead of printing two anonymous byte strings. All
    // four images differ in every odd case; that is what makes a case able to tell the halves apart.

    /// Run one copy of `len` bytes from `source` to `dest` with autoincrement `inc`, on a VDP whose VRAM is
    /// zero except the bytes `src` placed at `preset_at`; return `window` of VRAM afterwards.
    fn copy_image_at(
        preset_at: u16,
        src: &[u8],
        source: u16,
        dest: u16,
        len: u16,
        inc: u8,
        window: std::ops::Range<usize>,
    ) -> Vec<u8> {
        let mut v = fresh();
        v.vram.iter_mut().for_each(|b| *b = 0); // power-on VRAM is seeded: zero it so images are exact
        v.vram[preset_at as usize..preset_at as usize + src.len()].copy_from_slice(src);
        v.regs[0x0F] = inc;
        v.addr = dest;
        v.run_copy(source, len, 0);
        v.vram[window].to_vec()
    }

    /// [`copy_image_at`] with the source bytes placed at the copy's own `source`.
    fn copy_image(
        source: u16,
        src: &[u8],
        dest: u16,
        len: u16,
        inc: u8,
        window: std::ops::Range<usize>,
    ) -> Vec<u8> {
        copy_image_at(source, src, source, dest, len, inc, window)
    }

    /// Assert the rule's image, naming the missing half when the result is one of the wrong models' images.
    fn assert_copy_image(
        case: &str,
        got: &[u8],
        rule: &[u8],
        no_read: &[u8],
        no_write: &[u8],
        neither: &[u8],
    ) {
        let why = if got == no_read {
            "the READ half is missing: the source byte came from `source + i`, not `(source + i) ^ 1`"
        } else if got == no_write {
            "the WRITE half is missing: the byte landed at `dest + i*inc`, not `(dest + i*inc) ^ 1`"
        } else if got == neither {
            "BOTH halves are missing (the pre-F-COPYXOR copy: plain source, plain destination)"
        } else {
            "the image matches none of the four hand-derived models"
        };
        assert_eq!(got, rule, "{case}: {why}");
    }

    #[test]
    fn vram_copy_from_an_odd_source_reads_and_writes_the_opposite_byte_lane() {
        // source $0101 (odd), dest $0200, len 4, autoinc 1; VRAM $0100.. = A0 A1 A2 A3 A4 A5 A6 A7.
        // Rule: i=0 reads $0101^1=$0100 (A0) → writes $0200^1=$0201; i=1 reads $0102^1=$0103 (A3) → $0200;
        //       i=2 reads $0103^1=$0102 (A2) → $0203; i=3 reads $0104^1=$0105 (A5) → $0202.
        //       $0200..$0204 = A3 A0 A5 A2.
        // No read ^1: reads A1 A2 A3 A4 → $0201 $0200 $0203 $0202 = A2 A1 A4 A3.
        // No write ^1: reads A0 A3 A2 A5 → $0200..$0203 = A0 A3 A2 A5.   Neither: A1 A2 A3 A4.
        let src = [0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7];
        let got = copy_image_at(0x0100, &src, 0x0101, 0x0200, 4, 1, 0x0200..0x0204);
        assert_copy_image(
            "odd source $0101",
            &got,
            &[0xA3, 0xA0, 0xA5, 0xA2],
            &[0xA2, 0xA1, 0xA4, 0xA3],
            &[0xA0, 0xA3, 0xA2, 0xA5],
            &[0xA1, 0xA2, 0xA3, 0xA4],
        );
    }

    #[test]
    fn vram_copy_of_an_odd_length_leaves_the_last_pairs_even_byte_untouched() {
        // source $0100, dest $0200, len 3 (odd), autoinc 1; VRAM $0100.. = B0 B1 B2 B3.
        // Rule: i=0 reads $0101 (B1) → $0201; i=1 reads $0100 (B0) → $0200; i=2 reads $0103 (B3) → $0203.
        //       $0202 is never written: $0200..$0204 = B0 B1 00 B3.
        // No read ^1: reads B0 B1 B2 → $0201 $0200 $0203 = B1 B0 00 B2.
        // No write ^1: reads B1 B0 B3 → $0200..$0202 = B1 B0 B3 00.   Neither: B0 B1 B2 00.
        let got = copy_image(
            0x0100,
            &[0xB0, 0xB1, 0xB2, 0xB3],
            0x0200,
            3,
            1,
            0x0200..0x0204,
        );
        assert_copy_image(
            "odd length 3",
            &got,
            &[0xB0, 0xB1, 0x00, 0xB3],
            &[0xB1, 0xB0, 0x00, 0xB2],
            &[0xB1, 0xB0, 0xB3, 0x00],
            &[0xB0, 0xB1, 0xB2, 0x00],
        );
    }

    #[test]
    fn vram_copy_with_an_odd_autoincrement_writes_the_opposite_lane_of_each_step() {
        // source $0100, dest $0200, len 4, autoinc 3 (odd); VRAM $0100.. = C0 C1 C2 C3.
        // Destinations dest + 3i = $0200 $0203 $0206 $0209, each ^1 → $0201 $0202 $0207 $0208.
        // Reads (source + i) ^ 1 = $0101 $0100 $0103 $0102 = C1 C0 C3 C2.
        // Rule: $0201=C1 $0202=C0 $0207=C3 $0208=C2; $0200..$020A = 00 C1 C0 00 00 00 00 C3 C2 00.
        // No read ^1: C0 C1 C2 C3 → $0201 $0202 $0207 $0208 = 00 C0 C1 00 00 00 00 C2 C3 00.
        // No write ^1: C1 C0 C3 C2 → $0200 $0203 $0206 $0209 = C1 00 00 C0 00 00 C3 00 00 C2.
        // Neither: C0 C1 C2 C3 → $0200 $0203 $0206 $0209 = C0 00 00 C1 00 00 C2 00 00 C3.
        let got = copy_image(
            0x0100,
            &[0xC0, 0xC1, 0xC2, 0xC3],
            0x0200,
            4,
            3,
            0x0200..0x020A,
        );
        assert_copy_image(
            "odd autoincrement 3",
            &got,
            &[0x00, 0xC1, 0xC0, 0x00, 0x00, 0x00, 0x00, 0xC3, 0xC2, 0x00],
            &[0x00, 0xC0, 0xC1, 0x00, 0x00, 0x00, 0x00, 0xC2, 0xC3, 0x00],
            &[0xC1, 0x00, 0x00, 0xC0, 0x00, 0x00, 0xC3, 0x00, 0x00, 0xC2],
            &[0xC0, 0x00, 0x00, 0xC1, 0x00, 0x00, 0xC2, 0x00, 0x00, 0xC3],
        );
    }

    #[test]
    fn vram_copy_with_autoincrement_two_also_separates_the_models() {
        // An EVEN autoincrement other than 1 is not a safe case either (the brief's premise said only odd
        // starts, lengths and increments differ). source $0100, dest $0200, len 3, autoinc 2;
        // VRAM $0100.. = D0 D1 D2 D3. Destinations $0200 $0202 $0204, each ^1 → $0201 $0203 $0205.
        // Rule: reads $0101 $0100 $0103 = D1 D0 D3 → $0200..$0206 = 00 D1 00 D0 00 D3.
        // No read ^1: D0 D1 D2 → $0201 $0203 $0205 = 00 D0 00 D1 00 D2.
        // No write ^1: D1 D0 D3 → $0200 $0202 $0204 = D1 00 D0 00 D3 00.   Neither: D0 00 D1 00 D2 00.
        // (VDPFIFOTesting test 98, "DMA Copy 9000 to 8000 inc=2", pins the same shape from hardware.)
        let got = copy_image(
            0x0100,
            &[0xD0, 0xD1, 0xD2, 0xD3],
            0x0200,
            3,
            2,
            0x0200..0x0206,
        );
        assert_copy_image(
            "autoincrement 2",
            &got,
            &[0x00, 0xD1, 0x00, 0xD0, 0x00, 0xD3],
            &[0x00, 0xD0, 0x00, 0xD1, 0x00, 0xD2],
            &[0xD1, 0x00, 0xD0, 0x00, 0xD3, 0x00],
            &[0xD0, 0x00, 0xD1, 0x00, 0xD2, 0x00],
        );
    }

    #[test]
    fn an_aligned_even_vram_copy_keeps_the_pre_fix_image() {
        // CONTROL. source $0100 (even), dest $0200 (even), len 4 (even), autoinc 1; VRAM $0100.. = E0 E1 E2 E3.
        // Rule: reads $0101 $0100 $0103 $0102 = E1 E0 E3 E2 → writes $0201 $0200 $0203 $0202, so
        // $0200..$0204 = E0 E1 E2 E3 — the same image as the pre-fix code, because each read/write pair
        // swaps lanes on BOTH sides. This case passes under all four models, so it proves nothing about
        // either half; what it pins is that the fix is invisible to every aligned copy (which is every copy
        // the committed corpus performs — docs/2026-07-25-testrom-conformance.md, F-COPYXOR).
        let got = copy_image(
            0x0100,
            &[0xE0, 0xE1, 0xE2, 0xE3],
            0x0200,
            4,
            1,
            0x0200..0x0204,
        );
        assert_eq!(
            got,
            [0xE0, 0xE1, 0xE2, 0xE3],
            "aligned-even copy image unchanged"
        );
    }

    #[test]
    fn copy_runs_at_half_the_fill_byte_rate() {
        // Recon R4(c): a copy step is one byte read + one byte write = 2 slots/byte, half the fill's 1 slot/byte.
        let mut f = fresh();
        f.regs[1] = 0x40; // display on → active line, exact slot arithmetic
        f.code = 0x01; // VRAM target
        f.run_fill(8, 0xEE00, 0);
        let fill_window = f.dma_busy_until;
        let mut c = fresh();
        c.regs[1] = 0x40;
        c.code = 0x01;
        c.run_copy(0, 8, 0);
        let copy_window = c.dma_busy_until;
        assert_eq!(
            copy_window,
            2 * fill_window,
            "copy window = 2× the fill window for the same byte count"
        );
    }

    #[test]
    fn mem_dma_words_occupy_the_physical_fifo_ring() {
        // P1 (slice A3a): every word a 68k→VDP DMA moves is stored into the same physical 4-slot write FIFO
        // a CPU data-port write uses. Nemesis, VDP Internals: a DMA "will read a value from external memory
        // using the DMA source address register and *add it to the FIFO* using the current command code and
        // incremented command address registers". Observable through the CRAM/VSRAM undefined-bit snoop —
        // this is the whole of VDPFIFOTesting test 3 (ROM $5E0C; replayed end-to-end in `bus.rs`).
        let mut v = fresh();
        command(&mut v, 0x01, 0x8000); // VRAM write @ $8000
        for w in [0x1000u16, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000] {
            v.data_write(w); // six marker words fill (and wrap) the ring
        }
        for w in [0xAAAAu16, 0xBBBB, 0xCCCC, 0xDDDD, 0xEEEE, 0xFFFF] {
            v.dma_write_word(w, 0);
        }
        assert_eq!(
            v.fifo_snoop_word(),
            0xCCCC,
            "the DMA displaced every marker: the ring holds the last four payload words with the write \
             cursor parked on the oldest of them"
        );
        // Each subsequent data-port write walks the cursor one slot through the surviving payload.
        command(&mut v, 0x03, 0x0020); // CRAM write
        for expect in [0xDDDDu16, 0xEEEE, 0xFFFF] {
            v.data_write(0xFFFF);
            assert_eq!(v.fifo_snoop_word(), expect, "cursor walked one slot");
        }
    }

    #[test]
    fn mem_dma_leaves_the_fifo_full() {
        // T16/S2 — the INVERSE of A3a's `mem_dma_ring_store_does_not_add_pending_entries`, which this
        // replaces. A3a deliberately used the bare ring store so a DMA payload word took a physical slot
        // without bumping the pending count, and recorded the choice as design question Q1 of
        // `docs/2026-08-03-a3-dma-fifo-design.md`. **VDPFIFOTesting test 16 answers Q1 against it.**
        //
        // The ROM's own expected table at `$ED10` (word 34 of group 9, word 38 of group 10) requires the
        // 68k resuming after a 68k→VRAM DMA to observe the FIFO **FULL**. It can only do that if the
        // transfer left words undrained — which is what hardware does, because the DMA unit's job ends
        // when the last word is pushed *into the FIFO*, not when it reaches VRAM (Nemesis, VDP Internals:
        // a DMA "will read a value from external memory using the DMA source address register and *add it
        // to the FIFO*"). Group 10's three-word DMA onto a partly-filled FIFO is the case that pins
        // saturation at 4 rather than "the transfer leaves exactly `count` entries".
        let mut v = fresh();
        command(&mut v, 0x01, 0x8000);
        assert_eq!(v.fifo_len(), 0, "an idle FIFO before the transfer");
        for w in [0xAAAAu16, 0xBBBB, 0xCCCC, 0xDDDD, 0xEEEE, 0xFFFF] {
            v.dma_write_word(w, 0);
        }
        assert_eq!(
            v.fifo_len(),
            4,
            "a six-word DMA leaves the four-deep FIFO full, not empty (ROM $ED10 groups 9/10)"
        );
    }

    #[test]
    fn post_dma_fifo_drains_from_the_transfer_end() {
        // The other half of S2, and what makes the entries above real rather than phantom: our Mem DMA runs
        // synchronously inside the triggering bus access, so `dma_complete` anchors the drain clock at the
        // transfer's END instant. Without that anchor the residual would be measured against a slot clock
        // still sitting at the transfer's start and would appear already drained the moment the 68k
        // resumed — the very "phantom entries" objection A3a raised, here answered rather than ignored.
        let mut v = fresh();
        v.control_write(0x8144, 0); // mode 5 + display on
        v.control_write(0x8C81, 0); // H40
        command(&mut v, 0x01, 0x8000);
        let start = 100 * MCLK_PER_LINE;
        for w in [0xAAAAu16, 0xBBBB, 0xCCCC, 0xDDDD] {
            v.dma_write_word(w, start);
        }
        let end = start + 4_000; // a plausible transfer window; the exact cost is `Vdp::dma_cost`'s job
        v.dma_complete(
            DmaRecord {
                mode: DmaMode::Mem,
                source: 0,
                dest: 0x8000,
                len: 4,
                target: VdpTarget::Vram,
            },
            0,
            end,
        );
        // Immediately after the transfer nothing has drained: the clock starts at `end`, not at `start`.
        let s = v.control_read_status(0, end);
        assert_ne!(s & (1 << 8), 0, "FULL the instant the 68k resumes");
        assert_eq!(v.fifo_len(), 4, "no entry drained during the halt itself");
        // Letting one entry's slot cost elapse pops exactly one — the residual really drains from `end`.
        v.control_read_status(0, end + 400);
        assert_eq!(
            v.fifo_len(),
            3,
            "the residual drains from the transfer's end"
        );
    }

    /// Arm a VRAM DMA fill of `len` bytes at `addr` with autoinc 1, and return the armed request's
    /// `(len, fill)` after the trigger data-port write of `fill`.
    fn arm_and_trigger_vram_fill(v: &mut Vdp, addr: u16, len: u16, fill: u16) -> (u16, u16) {
        v.regs[1] = 0x10; // M1 (DMA enable) — CD5 only latches while it is set
        v.regs[0x0F] = 1; // autoinc 1
        v.regs[0x13] = (len & 0xFF) as u8;
        v.regs[0x14] = (len >> 8) as u8;
        v.regs[0x17] = 0x80; // fill mode
        command(v, 0x21, addr); // VRAM write + CD5
        v.data_write(fill);
        match v.take_dma_request() {
            Some(DmaRequest::Fill { len, fill }) => (len, fill),
            other => panic!("the data-port write must arm a fill, got {other:?}"),
        }
    }

    #[test]
    fn fill_trigger_is_applied_as_a_normal_word_write() {
        // A3b / P2: the data-port write that fires a pending fill is NOT swallowed — it is completed as an
        // ordinary write to the current target (VRAM: MSB → addr, LSB → addr ^ 1) and the address then
        // auto-increments, so the fill's first replicated byte lands back on the start address. Nemesis,
        // *VDP Internals* (SpritesMind): "When a DMA Fill operation is pending, and you perform a data port
        // write, that data port write is completed as normal". Observed by VDPFIFOTesting test 4 (expected
        // table ROM $DC54): only a full word write can put the trigger's LSB $34 at $8001.
        let mut v = fresh();
        let (len, fill) = arm_and_trigger_vram_fill(&mut v, 0x8000, 10, 0x1234);
        assert_eq!((len, fill), (10, 0x1234), "the armed fill request");
        assert_eq!(v.vram[0x8000], 0x12, "trigger MSB → address");
        assert_eq!(v.vram[0x8001], 0x34, "trigger LSB → address ^ 1");
        assert_eq!(v.addr, 0x8001, "the trigger auto-incremented the address");
        // I-4: `DmaRecord.dest` is the address the fill engine starts from, which since A3b is one
        // autoincrement step past the armed command address. Introspection only, in neither currency, but
        // it is what a debugger shows as "where the fill went" — pinned so the offset cannot drift silently.
        v.run_fill(len, fill, 0);
        assert_eq!(
            v.last_dma().expect("the fill recorded a DmaRecord").dest,
            0x8001,
            "a fill armed at $8000 with autoinc 1 reports dest = $8001 (post-trigger address)"
        );
    }

    #[test]
    fn a_fill_whose_code_names_no_write_target_writes_nothing_but_still_runs() {
        // A5 / FILL-TGT, and the retirement of follow-up F-FILLTGT.
        //
        // VDPFIFOTesting **test 34** "DMA Fill Control Port Writes" (ROM $45B2) group 3, disassembled at
        // $4898..$4948, is replayed here step for step:
        //
        //   $489C  reg $8F01   autoinc 1
        //   $48A4  reg $8154   M5 + DMA-enable + display
        //   $48B2  reg $9304   length low  = 4      ($48C0: reg $9400, length high = 0)
        //   $48C6  reg $9780   register 23 = fill mode
        //   $48DC  $40020082   the fill command: code $21 (VRAM write + CD5), address $8002
        //   $48E6  reg $8F02   autoinc 2 — AND a register write replaces CD1-CD0 with `10`, so the code
        //                      becomes $22, which names NO write target (test 13 pins that rule)
        //   $48EE  $68AC       the trigger
        //   $48F6  btst #1     poll until DMA-busy clears
        //   $4914  $00000002   VRAM read at $8000, then eight words
        //
        // Hardware reads back `1122 3344 5566 7788 99aa bbcc ddee ff00` — the initial pattern, COMPLETELY
        // unchanged. Before this fix we wrote `1122 3344 5568 7768 9968 bb68 ddee ff00`: the trigger was
        // suppressed (word 1 is right) but the body's four $68 bytes landed at $8005/$8007/$8009/$800B.
        //
        // And the fill still RAN. Group 4 ($4954) programs no length and fills well past the 16 bytes the
        // group reads, which is only possible if group 3 left the length counter at 0 — so it counted its
        // 4 steps. That is why this is a suppressed write, not an early return.
        let mut v = fresh();
        let pattern: [u8; 16] = [
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE,
            0xFF, 0x00,
        ];
        v.vram[0x8000..0x8010].copy_from_slice(&pattern);
        v.regs[1] = 0x54; // ROM $48A4
        v.regs[0x0F] = 1; // ROM $489C
        v.regs[0x13] = 4; // ROM $48B2 / $48C0: a 4-byte fill
        v.regs[0x14] = 0;
        v.regs[0x17] = 0x80; // ROM $48C6
        set_source_low16(&mut v, 0x00FA); // A3's group-2 start, so 21/22 are observable here too
        command(&mut v, 0x21, 0x8002); // ROM $48DC
        v.control_write(0x8F02, 0); // ROM $48E6 — autoinc 2, and the code becomes $22
        assert_eq!(v.code, 0x22, "a register write replaces CD1-CD0 with `10`");
        assert!(
            !Vdp::code_names_a_write_target(v.code),
            "code $22 names no write target (the premise of test 34 group 3)"
        );
        v.data_write(0x68AC); // ROM $48EE
        let Some(DmaRequest::Fill { len, fill }) = v.take_dma_request() else {
            panic!("the trigger must still arm the fill — the fill runs, it just writes nowhere");
        };
        assert_eq!((len, fill), (4, 0x68AC), "a 4-byte fill of $68");
        v.run_fill(len, fill, 0);

        assert_eq!(
            v.vram[0x8000..0x8010],
            pattern,
            "test 34 group 3: $8000-$800F reads back unchanged on hardware"
        );
        // The three things the fill must still do — the half that a `return` would silently break.
        assert_eq!(
            (v.regs[0x14], v.regs[0x13]),
            (0, 0),
            "the fill consumed its length (group 4 depends on the counter reaching 0)"
        );
        assert_eq!(
            source_low16(&v),
            0x00FE,
            "A3: source registers 21/22 still advance by the length (tests 28/29)"
        );
        assert!(
            v.dma_busy(0),
            "the busy window still opens — group 3's `btst #1` poll at ROM $48F6 waits on it"
        );
        assert_eq!(
            v.addr, 0x800C,
            "the engine still walked its address: $8002 + 2 for the suppressed trigger + 2 × 4 steps"
        );
    }

    #[test]
    fn a_fill_that_does_name_a_write_target_is_untouched_by_the_write_decode() {
        // CONTROL for the test above: the same group with the register write REMOVED, so the code stays $21.
        // Everything else identical. The four $68 bytes must land exactly where they did before A5 —
        // $8005/$8007/$8009/$800B under autoinc 2 — which is what test 34's groups 5-8 read
        // (`1122 68ac 5568 7768 9968 bb68 ddee ff00`, where the trigger DID land as well).
        let mut v = fresh();
        let pattern: [u8; 16] = [
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE,
            0xFF, 0x00,
        ];
        v.vram[0x8000..0x8010].copy_from_slice(&pattern);
        v.regs[1] = 0x54;
        v.regs[0x0F] = 2; // autoinc 2, as $8F02 leaves it
        v.regs[0x13] = 4;
        v.regs[0x14] = 0;
        v.regs[0x17] = 0x80;
        command(&mut v, 0x21, 0x8002);
        v.data_write(0x68AC);
        let Some(DmaRequest::Fill { len, fill }) = v.take_dma_request() else {
            panic!("the trigger must arm the fill");
        };
        v.run_fill(len, fill, 0);
        assert_eq!(
            (v.vram[0x8002], v.vram[0x8003]),
            (0x68, 0xAC),
            "code $21 names a write target, so the trigger word lands"
        );
        assert_eq!(
            [
                v.vram[0x8005],
                v.vram[0x8007],
                v.vram[0x8009],
                v.vram[0x800B]
            ],
            [0x68, 0x68, 0x68, 0x68],
            "…and so do the fill's four $68 bytes (test 34 groups 5-8)"
        );
    }

    #[test]
    fn fill_trigger_primes_a_cram_fill_target() {
        // EXTRAPOLATED BEHAVIOR, pinned as-shipped — design question Q3, follow-up **F-FILLPRIME**.
        //
        // A3b's guard admits all three write targets, so a fill armed with code $23 (CRAM write + CD5) has
        // its trigger word written into CRAM before the fill body runs — where previously the write was
        // swallowed. No ROM in `vendor/TestRoms/` exercises a $23/$25 fill: VDPFIFOTesting test 4 pins the
        // trigger rule for VRAM only, and Nemesis's statement of it ("that data port write is completed as
        // normal") names no target. Owner ruling 2026-08-03: apply the pinned rule uniformly rather than
        // add an equally unevidenced VRAM-only exception. This test exists so the extrapolation is an
        // explicit, greppable decision rather than a side effect of the guard's shape.
        //
        // Distinct from `cram_fill_uses_the_four_writes_ago_entry`, which pokes `code` directly and calls
        // `run_fill` — it never reaches `apply_data_write`, so it cannot see the priming write at all.
        let mut v = fresh();
        v.cram.fill(0);
        v.regs[1] = 0x10; // M1 (DMA enable)
        v.regs[0x0F] = 2; // autoinc 2 (one CRAM entry)
        v.regs[0x13] = 2; // fill length
        v.regs[0x17] = 0x80; // fill mode
        command(&mut v, 0x23, 0x0010); // CRAM write + CD5 @ entry 8
        v.data_write(0x0EEE);
        let entry = ((v.cram[0x10] as u16) << 8) | v.cram[0x11] as u16;
        assert_eq!(
            entry, 0x0EEE,
            "the trigger word primed the armed CRAM entry"
        );
        assert_eq!(v.addr, 0x0012, "the trigger auto-incremented the address");
    }

    #[test]
    fn vram_fill_writes_the_msb_to_address_xor_one() {
        // A3b / P3: every VRAM byte write the fill engine makes lands at `address ^ 1`. Mask of Destiny,
        // *Is DMA Fill buggy?* (SpritesMind): "MSB of the word in the FIFO is written DMA length times to
        // address ^ 1"; Eke, same thread: "VRAM byte writes (used by VRAM fill and copy DMA) actually occur
        // to VRAM address ^ 1 so you can get unexpected results depending on start address, DMA length and
        // increment alignments". With addr $8000, autoinc 1 and length 10 the ten steps run over addresses
        // $8001..$800A and therefore write $8000, $8003, $8002, $8005, $8004, $8007, $8006, $8009, $8008,
        // $800B — skipping $800A and reaching one byte past the naive end. That exact image is the second
        // half of VDPFIFOTesting test 4's expected table (ROM $DC54).
        let mut v = fresh();
        v.vram[0x8000..0x8010].fill(0); // as the ROM does: eight zeroing data writes before the fill
        let (len, fill) = arm_and_trigger_vram_fill(&mut v, 0x8000, 10, 0x1234);
        v.run_fill(len, fill, 0);
        assert_eq!(
            &v.vram[0x8000..0x800C],
            &[0x12, 0x34, 0x12, 0x12, 0x12, 0x12, 0x12, 0x12, 0x12, 0x12, 0x00, 0x12],
            "trigger word + ten `address ^ 1` fill bytes"
        );
        assert!(
            v.vram[0x800C..0x8010].iter().all(|&b| b == 0),
            "the fill reached exactly one byte past the naive end and no further"
        );
    }

    #[test]
    fn fill_adds_only_its_trigger_word_to_the_ring() {
        // A3b / P4: the fill engine pulls its byte *out of* a FIFO entry, it does not push entries in — so
        // the 4-slot ring ends up holding the last three zeroing writes plus the single trigger word, and
        // nothing else. This is the snoop half of VDPFIFOTesting test 4 (ROM $DC54 words 0-7 =
        // `0000 0000 0000 0000 0000 0000 1000 1000`), read here directly off the ring instead of through
        // the VSRAM undefined-bit merge.
        //
        // PROVENANCE: unlike its two siblings this is a **characterization guard, not a red-first test** —
        // it passed against the pre-A3b code as well (the fill body never enqueued there either). It is
        // kept for the same reason `cram_fill_uses_the_four_writes_ago_entry` is: it locks a property the
        // A3b changes could plausibly have broken.
        let mut v = fresh();
        v.regs[0x0F] = 2;
        command(&mut v, 0x01, 0x8000); // VRAM write
        for _ in 0..8 {
            v.data_write(0x0000);
        }
        let (len, fill) = arm_and_trigger_vram_fill(&mut v, 0x8000, 10, 0x1234);
        v.run_fill(len, fill, 0);
        assert_eq!(
            v.fifo_snoop_word(),
            0x0000,
            "cursor parked on a zeroing write"
        );
        command(&mut v, 0x03, 0x0020); // CRAM write: each one walks the cursor a slot
        for expect in [0x0000u16, 0x0000, 0x1234] {
            v.data_write(0xFFFF);
            assert_eq!(v.fifo_snoop_word(), expect, "cursor walked one slot");
        }
    }

    #[test]
    fn cram_fill_uses_the_four_writes_ago_entry() {
        // Recon R4(b): a CRAM (or VSRAM) fill takes its data from the next-available FIFO entry ("4 writes
        // ago"), NOT the trigger word — the documented hardware bug.
        let mut v = fresh();
        v.regs[1] = 0x10; // DMA enable
        v.regs[0x17] = 0x80; // fill mode
        v.fifo[v.fifo_write as usize].data = 0x0ABC; // the next-available (4-writes-ago) entry
        v.code = 0x23; // CRAM write + CD5
        v.addr = 0x0000;
        v.run_fill(2, 0x0EEE, 0); // trigger word 0x0EEE — must be IGNORED for CRAM
        let cram_word = ((v.cram[0] as u16) << 8) | v.cram[1] as u16;
        assert_eq!(
            cram_word,
            0x0ABC & 0x0EEE,
            "CRAM fill used the snooped entry, not the trigger word"
        );
        assert_ne!(cram_word, 0x0EEE, "the trigger word did NOT fill CRAM");
    }

    /// Issue a two-word VDP command through the control port (first word CD1-CD0 + A13-A0, second word
    /// CD5-CD2 + A15-A14). `cd` is the full 6-bit command code; `addr` the 16-bit target address.
    fn command(v: &mut Vdp, cd: u8, addr: u16) {
        let w1 = (((cd & 0x03) as u16) << 14) | (addr & 0x3FFF);
        let w2 = ((((cd >> 2) & 0x0F) as u16) << 4) | (addr >> 14);
        v.control_write(w1, 0);
        v.control_write(w2, 0);
    }

    // --- A3 / DMA-SRC-ADVANCE: a fill and a copy advance source registers 21/22 --------------------------
    //
    // Nemesis, *VDP Internals* p.4: "Every DMA operation also performs the exact same set of steps after it is
    // advanced one step, which is to firstly add 1 to the lower 2 DMA source address registers, then to
    // subtract 1 from the DMA length counter register". Fill and copy included; a fill never reads its source.
    //
    // The cases below are derived from VDPFIFOTesting's tables, not from our output. Tests 28 (fill, ROM
    // `$AA16`) and 29 (copy, ROM `$AEC0`) run the operation for 4 bytes, then start a 68k DMA that rewrites two
    // of the three source registers and leaves the third where the operation put it. Both tables are `$A9FE` /
    // `$AEA8`, word for word the same:
    //
    // * Group 2 starts from `$00FA`, and the follow-up DMA writes 22 = `$00` and 23 = `$02` but not 21
    //   (`$AC54..$AC58`). The hardware reads `1111 ffff eeee dddd`, which sits at ROM byte `$401FC` = word
    //   `$0200FE`, so the operation left register 21 at `$FE`: 4 steps, `$FA + 4`.
    // * Group 3 starts from `$00FE`, and the follow-up DMA writes 21 = `$FE` and 23 = `$02` but not 22
    //   (`$AE04..$AE08`). The hardware reads `ffff eeee dddd cccc`, which sits at ROM byte `$403FC` = word
    //   `$0201FE`, so the operation left register 22 at `$01`: `$FE + 4 = $102`, **21 carries into 22**.
    //
    // Two cases the ROM cannot reach, derived from the rule instead. Register 23 never takes the carry: only
    // "the lower 2" registers count (the same quote, and A2's rule for the 68k DMA, MegaDrive Wiki "VDP", *DMA
    // Limitations*: "only the low and middle bytes of the DMA source registers are incremented"), so `$FFFE + 4`
    // leaves `$0002` and 23 keeps its mode bits. A length of 0 is 65,536 steps (RD2), one whole turn of a
    // 16-bit counter, so 21/22 end where they started.

    /// Registers 21 (low) and 22 (middle) as the one 16-bit counter they are.
    fn source_low16(v: &Vdp) -> u16 {
        ((v.regs[0x16] as u16) << 8) | v.regs[0x15] as u16
    }

    fn set_source_low16(v: &mut Vdp, source: u16) {
        v.regs[0x15] = (source & 0xFF) as u8;
        v.regs[0x16] = (source >> 8) as u8;
    }

    /// `(start, length, end, why)`: the four cases derived above, shared by the fill and the copy test.
    const SOURCE_ADVANCE_CASES: [(u16, u16, u16, &str); 4] = [
        (0x00FA, 4, 0x00FE, "tests 28/29 group 2: $FA + 4 steps"),
        (
            0x00FE,
            4,
            0x0102,
            "tests 28/29 group 3: register 21 carries into 22",
        ),
        (
            0xFFFE,
            4,
            0x0002,
            "the 16-bit counter wraps, and register 23 takes no carry",
        ),
        (
            0x1234,
            0,
            0x1234,
            "a length of 0 is 65,536 steps, one whole turn of the counter",
        ),
    ];

    #[test]
    fn a_fill_advances_source_registers_21_22_by_its_length_and_never_carries_into_23() {
        for (start, len, end, why) in SOURCE_ADVANCE_CASES {
            let mut v = fresh();
            set_source_low16(&mut v, start);
            // Armed and triggered through the ports, as the ROM does: the helper writes register 23 = $80.
            let (len, fill) = arm_and_trigger_vram_fill(&mut v, 0x8000, len, 0x1234);
            v.run_fill(len, fill, 0);
            assert_eq!(
                source_low16(&v),
                end,
                "fill from ${start:04X} for {len} bytes: registers 22:21 ({why})"
            );
            assert_eq!(
                v.regs[0x17], 0x80,
                "fill from ${start:04X}: register 23 keeps fill mode and takes no carry ({why})"
            );
            assert_eq!(
                (v.regs[0x14], v.regs[0x13]),
                (0, 0),
                "the length counter still ends at 0"
            );
        }
    }

    #[test]
    fn a_copy_advances_source_registers_21_22_by_its_length_and_never_carries_into_23() {
        for (start, len, end, why) in SOURCE_ADVANCE_CASES {
            let mut v = fresh();
            set_source_low16(&mut v, start);
            v.regs[1] |= 0x10; // M1 (DMA enable): CD5 only latches while it is set
            v.regs[0x0F] = 1;
            v.regs[0x13] = (len & 0xFF) as u8;
            v.regs[0x14] = (len >> 8) as u8;
            v.regs[0x17] = 0xC0; // copy mode
                                 // Armed through the control port, as test 29 does (`$000000C2`: code $30, VRAM $8000), so the
                                 // copy's source comes from registers 21/22 by the real arming path.
            command(&mut v, 0x30, 0x8000);
            let Some(DmaRequest::Copy { source, len }) = v.take_dma_request() else {
                panic!("a code-$30 command in copy mode must arm a copy");
            };
            assert_eq!(source, start, "the copy armed from registers 21/22");
            v.run_copy(source, len, 0);
            assert_eq!(
                source_low16(&v),
                end,
                "copy from ${start:04X} for {len} bytes: registers 22:21 ({why})"
            );
            assert_eq!(
                v.regs[0x17], 0xC0,
                "copy from ${start:04X}: register 23 keeps copy mode and takes no carry ({why})"
            );
            assert_eq!(
                (v.regs[0x14], v.regs[0x13]),
                (0, 0),
                "the length counter still ends at 0"
            );
        }
    }

    // --- A4 / FILL-BUSY-ARM: the fill's CONTROL write sets DMA-busy -------------------------------------
    //
    // Eke, *VDP Internals* p.4: "on DMA Fill, busy flag is actually immediately (?) set after the CTRL port
    // write, not the DATA port write that starts the Fill operation."
    //
    // Every expectation below is read off VDPFIFOTesting's own tables and its code, never off ours. Both
    // busy tests sample the status port twice per probe (once with the next prefetch word `$0245`, once with
    // `$4E71`) and mask with `$FF02`, so on hardware a set busy bit shows as the pair `0202 4e02` and a clear
    // one as `0200 4e00`.
    //
    // * **Test 36** (ROM `$B6F8`), table `$B6B4`: `0200 4e00 | 0202 4e02 | 0202 4e02 | 0200 4e00`. Probe 1 is
    //   at `$B81E`, before any command. Probe 2 is at `$B8A2` — after the fill command `$40020082` went out
    //   at `$B89C` and **before** the `$1234` trigger at `$B8BC`. Probe 3 is mid-fill, probe 4 after a
    //   `$7FFF` delay loop.
    // * **Test 38** (ROM `$C34C`), table `$C2E8`, two groups of the same four probes.
    //   - Group 1 (`$C4CC`) issues the command at `$C528` with **DMA-enable clear**, and hardware reads
    //     `0200 4e00` at every probe — CD5 never latched, so nothing was ever armed, and the later `$1234`
    //     lands as an ordinary VRAM write (`0000 1234 0000 0000` at `$8000`).
    //   - Group 2 (`$C7EC`) sets register 1 = `$54` first, so the command at `$C850` latches CD5. Hardware
    //     reads `0202 4e02` at probe 1, and **still** `0202 4e02` at probe 2 — which is taken after a
    //     register write `$8144` that clears DMA-enable and a half-command word `$4002` (`$C870..$C878`).
    //
    // The model: busy = a fill is armed (CD5 + register 23 = Fill mode) OR the transfer window is open.
    // See [`Vdp::dma_busy`] for what that decides about the case the ROM leaves open.

    /// Arm a VRAM fill through the ports exactly as tests 36 and 38 group 2 do — register 1 = DMA-enable,
    /// register 23 = Fill, then the two command words — and stop **before** the data-port trigger.
    fn arm_vram_fill_without_triggering(v: &mut Vdp, dma_enable: bool, len: u16) {
        // The ROM's own register-1 values: $54 (M5 + DMA-enable + display) and $44 (the same with DMA off).
        // M5 matters — `write_register` discards writes above register 10 while it is clear.
        v.regs[1] = if dma_enable { 0x54 } else { 0x44 };
        v.regs[0x0F] = 1; // autoinc 1 (ROM: register $8F01)
        v.regs[0x13] = (len & 0xFF) as u8;
        v.regs[0x14] = (len >> 8) as u8;
        v.regs[0x17] = 0x80; // fill mode (ROM: register $9780)
        command(v, 0x21, 0x8002); // the ROM's $40020082: VRAM write + CD5 at $8002
    }

    #[test]
    fn a_fill_reads_dma_busy_from_its_control_write_not_from_its_trigger() {
        // Test 36 probe 2 / test 38 group 2 probe 1: busy between the command and the trigger.
        let mut v = fresh();
        arm_vram_fill_without_triggering(&mut v, true, 0x0100);
        assert!(
            v.dma_busy(0),
            "test 36 (ROM $B8A2) reads $0202 after the fill command and before the $1234 trigger"
        );
        assert_eq!(
            v.status_word(0) & 0x0002,
            0x0002,
            "…and it is status bit 1 that carries it"
        );

        // Test 38 group 2 probe 2: a register write that CLEARS DMA-enable, then a half-command word, and
        // hardware still reads $0202. Neither is allowed to end the arm.
        v.control_write(0x8144, 0); // register 1 = $44 — DMA-enable off (ROM $C870)
        assert!(
            v.dma_busy(0),
            "test 38 group 2 (ROM $C880) still reads $0202 after register 1 = $44"
        );
        v.control_write(0x4002, 0); // a first-half-only command word (ROM $C878)
        assert!(
            v.dma_busy(0),
            "…and after the half-command word $4002 (ROM $C888)"
        );

        // Probes 3 and 4: the trigger runs the fill, which clears CD5 and opens the timed window; the window
        // then expires. This is the ROM's `0202 4e02 | 0200 4e00` tail, and it is also what keeps test 34
        // group 3's `btst #1` poll loop (ROM $48F6) from spinning for ever. Note the ROM triggers with
        // DMA-enable already cleared — the trigger does not re-check it.
        v.data_write(0x1234); // ROM $C89A
        let Some(DmaRequest::Fill { len, fill }) = v.take_dma_request() else {
            panic!("the data-port write must arm a fill");
        };
        v.run_fill(len, fill, 0);
        assert!(v.dma_busy(0), "mid-transfer the window is open (probe 3)");
        assert!(
            !v.dma_busy(v.dma_busy_until),
            "the window closes and nothing keeps busy set (probe 4: hardware reads $0200)"
        );
    }

    #[test]
    fn a_fill_command_written_with_dma_disabled_arms_nothing_and_is_not_busy() {
        // Test 38 group 1 (ROM $C528): the command goes out with DMA-enable clear, so CD5 never latches.
        // Hardware reads $0200 at all four probes. This is the negative half of the rule — a busy flag armed
        // by the control word alone, without the CD5 condition, would fail here.
        let mut v = fresh();
        arm_vram_fill_without_triggering(&mut v, false, 0x0100);
        assert_eq!(
            v.code & 0x20,
            0,
            "CD5 cannot latch while DMA-enable is clear"
        );
        assert!(
            !v.dma_busy(0),
            "test 38 group 1 (ROM $C52E) reads $0200 after the fill command"
        );

        // The ROM then enables DMA (register 1 = $54, $C548) and writes a half-command. Still not busy on
        // hardware ($C558): enabling DMA later does not retroactively arm the fill that was refused.
        v.control_write(0x8154, 0);
        v.control_write(0x4002, 0);
        assert!(
            !v.dma_busy(0),
            "test 38 group 1 (ROM $C55E) still reads $0200 once DMA-enable is turned on"
        );

        // And the $1234 at $C572 is an ordinary VRAM write, not a fill trigger: $8000..$8007 reads back
        // `0000 1234 0000 0000`.
        v.data_write(0x1234);
        assert!(
            v.take_dma_request().is_none(),
            "no fill was armed, so the data write triggers nothing"
        );
        assert_eq!(
            (v.vram[0x8002], v.vram[0x8003]),
            (0x12, 0x34),
            "the trigger word lands as a plain VRAM write at $8002 (test 38 group 1 readback)"
        );
        assert!(!v.dma_busy(0), "and busy is still clear (ROM $C5CE)");
    }

    #[test]
    fn an_armed_fill_stops_reading_busy_once_its_arming_condition_is_gone() {
        // The case the ROM does NOT settle, pinned as the model decides it (see `Vdp::dma_busy`): busy ends
        // when the arm ends, and only then. Two ways for the arm to end without the fill ever running.
        let mut v = fresh();
        arm_vram_fill_without_triggering(&mut v, true, 0x0004);
        assert!(v.dma_busy(0), "armed");
        // A full non-DMA command word pair while DMA-enable is set clears CD5, so the next data write is no
        // longer a fill trigger — and the flag follows the trigger, by construction.
        command(&mut v, 0x01, 0x8000);
        assert!(
            !v.dma_busy(0),
            "a command that clears CD5 also ends the busy flag"
        );

        let mut v = fresh();
        arm_vram_fill_without_triggering(&mut v, true, 0x0004);
        v.control_write(0x9700, 0); // register 23 = $00: Mem mode, no longer a fill
        assert!(
            !v.dma_busy(0),
            "register 23 leaving Fill mode also ends the busy flag"
        );

        // Time does not end it: the same armed fill is still busy a whole frame later.
        let mut v = fresh();
        arm_vram_fill_without_triggering(&mut v, true, 0x0004);
        assert!(
            v.dma_busy(MCLK_PER_FRAME * 4),
            "no timed window ends an armed fill's busy flag"
        );
    }

    #[test]
    fn a_completed_dma_clears_cd5_so_a_later_m1_off_command_does_not_respawn_it() {
        // CD5 = "DMA work pending" — the engine clears it on completion (recon V2). oracle-next consumes a DMA
        // via take_dma_request; if CD5 is not cleared there, it goes stale and a later M1=0 command (which
        // retains CD5, recon R1/V1) re-fires a phantom DMA — the DR-2 TF4 ~28-frame halt.
        let mut v = fresh();
        v.regs[1] = 0x10; // M1 (DMA enable) on
        v.regs[0x17] = 0x00; // Mem mode (68k->VRAM); source high byte 0
        v.regs[0x13] = 0x02; // length low  = 2 words
        v.regs[0x14] = 0x00; // length high
        v.regs[0x15] = 0x00; // source low
        v.regs[0x16] = 0x00; // source mid

        // A real Mem DMA command (CD = 0b100001: CD5 DMA + CD0 write, VRAM target) arms + is consumed.
        command(&mut v, 0x21, 0x0000);
        assert!(
            v.take_dma_request().is_some(),
            "a normal M1=1 Mem DMA still arms and is consumed"
        );

        // The game disables DMA, then issues a PLAIN VRAM-write command (CD5 = 0 in its pattern). With M1=0
        // CD5 is retained (R1) — but the completed DMA must have cleared it, so nothing re-arms.
        v.regs[1] = 0x00; // M1 off
        command(&mut v, 0x01, 0x0100); // plain VRAM write
        assert!(
            v.take_dma_request().is_none(),
            "no phantom DMA: the consumed DMA cleared CD5, so the M1=0 command does not re-arm"
        );
    }

    #[test]
    fn fill_cd5_survives_the_control_write_until_the_data_trigger() {
        // A VRAM fill is a TWO-step DMA: the control write sets CD5 but arms nothing (arm_dma no-ops for
        // Fill); the fill is armed by the following data-port write. So CD5 must SURVIVE the control write —
        // take_dma_request returns None there and must NOT clear CD5 prematurely (it clears only on an actual
        // consume, guarded by is_some). Recon V2 + the overseer's Fill-two-step check.
        let mut v = fresh();
        v.regs[1] = 0x10; // M1 on
        v.regs[0x17] = 0x80; // Fill mode
        v.regs[0x13] = 0x02; // length 2
        v.addr = 0x0000;

        // Fill control command (CD = 0b100001) — arms nothing yet.
        command(&mut v, 0x21, 0x0000);
        assert!(
            v.take_dma_request().is_none(),
            "the Fill control write arms nothing (armed by the data trigger)"
        );

        // The data-port write is the fill trigger — it must still see CD5 set and arm the Fill.
        v.data_write(0xEEEE);
        assert!(
            v.take_dma_request().is_some(),
            "CD5 survived the control write, so the data trigger arms the Fill"
        );
    }

    #[test]
    fn commit_scanline_sprites_sets_carry_and_ors_status() {
        let mut v = fresh();
        v.commit_scanline_sprites(true, true, false);
        assert!(
            v.sprite_dot_overflow_carry,
            "carry = this line's dot overflow"
        );
        assert!(v.sprite_overflow);
        assert!(!v.sprite_collision);
        // The carry is REPLACED each line (it tracks the previous line); overflow/collision are STICKY.
        v.commit_scanline_sprites(false, false, true);
        assert!(
            !v.sprite_dot_overflow_carry,
            "carry replaced by the new line's dot overflow"
        );
        assert!(v.sprite_overflow, "overflow is sticky (OR)");
        assert!(v.sprite_collision, "collision is sticky (OR)");
    }

    #[test]
    fn status_read_clears_sprite_overflow_and_collision() {
        let mut v = fresh();
        v.sprite_overflow = true;
        v.sprite_collision = true;
        let s = v.control_read_status(0, 0);
        assert_eq!(
            s & 0x60,
            0x60,
            "the read still reports both flags (bits 6+5)"
        );
        assert!(
            !v.sprite_overflow && !v.sprite_collision,
            "reading the status clears them (Sega manual)"
        );
        assert_eq!(
            v.control_read_status(0, 0) & 0x60,
            0,
            "a subsequent read sees them cleared"
        );
    }

    #[test]
    fn status_read_floats_the_upper_six_bits_with_the_open_bus_residue() {
        // K4-5: the VDP drives only the low 10 status lines (StatusRegisterMask = 0x03FF); bits 10-15
        // are whatever the caller's bus residue holds. memtest row 11: residue $4E71 -> $4C00 merged
        // over the live low bits. A zero residue is byte-identical to the pre-K4-5 behavior.
        let mut v = fresh();
        let zero = v.control_read_status(0, 0);
        assert_eq!(zero & 0xFC00, 0, "zero residue -> upper 6 bits clear");
        let merged = v.control_read_status(0x4E71, 0);
        assert_eq!(
            merged & 0xFC00,
            0x4C00,
            "residue $4E71 -> bits 10-15 = $4C00"
        );
        assert_eq!(
            merged & 0x03FF,
            zero & 0x03FF,
            "the VDP-driven low 10 bits are untouched by the residue"
        );
        // The residue NEVER leaks into bits 8-9 (FIFO full/empty are VDP-driven).
        let full_residue = v.control_read_status(0xFFFF, 0);
        assert_eq!(
            full_residue & 0x0300,
            zero & 0x0300,
            "bits 8-9 stay VDP-driven under an all-ones residue"
        );
    }

    #[test]
    fn sat_cache_and_carry_are_not_in_the_hashed_regions() {
        // The currency-neutrality headline: the new fields are outside the four Oracle-hashed regions, so
        // the `state_hash` is byte-identical no matter what they hold (the export golden is proven by the
        // export_state_v1 test staying green).
        let a = fresh();
        let mut b = fresh();
        b.sat_cache[0] = 0xAB;
        b.sprite_dot_overflow_carry = true;
        assert_eq!(a.vram(), b.vram());
        assert_eq!(a.cram(), b.cram());
        assert_eq!(a.vsram(), b.vsram());
        assert_eq!(a.regs(), b.regs());
        assert_eq!(
            crate::state_hash::StateHash::compute(a.vram(), a.cram(), a.vsram(), a.regs()),
            crate::state_hash::StateHash::compute(b.vram(), b.cram(), b.vsram(), b.regs()),
            "the SAT cache + carry are outside the Oracle state_hash currency"
        );
    }

    // --- VDP-internal write capture (watchpoints v2) ------------------------------------------------------

    /// Arm a VRAM write at addr 0x0100, autoinc 2.
    fn arm_vram_write(v: &mut Vdp, addr: u16) {
        v.regs[0x0F] = 2;
        v.control_write(0x4000 | (addr & 0x3FFF), 0); // VRAM write (code 0x01), low addr bits
        v.control_write(addr >> 14, 0); // high addr bits, disarm toggle
    }

    /// Disarmed (the default), a data-port write records no capture — the zero-cost-when-off guarantee.
    #[test]
    fn disarmed_captures_nothing() {
        let mut v = fresh();
        arm_vram_write(&mut v, 0x0100);
        v.data_write(0xBEEF);
        assert!(
            v.take_write_captures().is_empty(),
            "no capture when the buffer is disarmed"
        );
    }

    /// Armed, a direct data-port VRAM write is captured byte-granular: two VRAM byte writes (high→addr,
    /// low→addr^1), each with the pre-write `old`, the new byte, size 1, and `via = Direct`.
    #[test]
    fn armed_captures_a_direct_vram_write() {
        let mut v = fresh();
        arm_vram_write(&mut v, 0x0100);
        let old_hi = v.vram()[0x0100];
        let old_lo = v.vram()[0x0101];
        v.set_write_capture(true);
        v.data_write(0xBEEF);
        let caps = v.take_write_captures();
        assert_eq!(caps.len(), 2, "one capture per VRAM byte");
        assert_eq!(
            caps[0],
            VdpWrite {
                target: VdpTarget::Vram,
                addr: 0x0100,
                old: old_hi as u32,
                new: 0xBE,
                size: 1,
                via: VdpVia::Direct,
                mclk: 0, // this fixture drives every port at mclk 0
            }
        );
        assert_eq!(
            caps[1],
            VdpWrite {
                target: VdpTarget::Vram,
                addr: 0x0101,
                old: old_lo as u32,
                new: 0xEF,
                size: 1,
                via: VdpVia::Direct,
                mclk: 0,
            }
        );
        assert!(
            v.take_write_captures().is_empty(),
            "take drained the buffer"
        );
    }

    /// Armed, a direct CRAM data-port write is captured as one word write: the resolved CRAM byte address,
    /// old→new (9-bit masked), size 2, via = Direct.
    #[test]
    fn armed_captures_a_direct_cram_write() {
        let mut v = fresh();
        v.regs[0x0F] = 2;
        v.control_write(0xC000, 0); // CRAM write (code 0x03), addr 0
        v.control_write(0x0000, 0);
        v.set_write_capture(true);
        v.data_write(0x0EEE);
        let caps = v.take_write_captures();
        assert_eq!(caps.len(), 1, "one word capture for CRAM");
        assert_eq!(
            caps[0],
            VdpWrite {
                target: VdpTarget::Cram,
                addr: 0x0000,
                old: 0x0000,
                new: 0x0EEE,
                size: 2,
                via: VdpVia::Direct,
                mclk: 0, // this fixture drives every port at mclk 0
            }
        );
    }

    /// Armed, a VRAM fill DMA attributes each byte write to `via = Dma`. Fill byte $AB over 4 bytes at $0200.
    #[test]
    fn armed_captures_a_dma_fill_with_via_dma() {
        let mut v = fresh();
        v.regs[0x0F] = 1;
        v.control_write(0x4200, 0); // VRAM write (code 0x01), A13-A0 = 0x0200
        v.control_write(0x0080, 0); // CD5..CD2 high nibble = 0b1000 → code 0x21 (VRAM write + CD5); disarm
                                    // A3b: the captured addresses are the *written* bytes, `address ^ 1` at each step (P3), so with
                                    // autoinc 1 from $0200 they come out pair-swapped. Everything else the test pins — one capture per
                                    // filled byte, `via = Dma`, byte size, the pre-write value — is unchanged. This test calls
                                    // `run_fill` directly, so the trigger-write change (P2) does not reach it.
        let addrs = [0x0201u32, 0x0200, 0x0203, 0x0202];
        let old: Vec<u8> = addrs.iter().map(|&a| v.vram()[a as usize]).collect();
        v.set_write_capture(true);
        v.run_fill(4, 0xAB00, 0); // fill byte = top byte = $AB, len 4
        let caps = v.take_write_captures();
        assert_eq!(caps.len(), 4, "one capture per filled byte");
        for (i, cap) in caps.iter().enumerate() {
            assert_eq!(cap.target, VdpTarget::Vram);
            assert_eq!(cap.addr, addrs[i]);
            assert_eq!(cap.old, old[i] as u32);
            assert_eq!(cap.new, 0xAB);
            assert_eq!(cap.size, 1);
            assert_eq!(cap.via, VdpVia::Dma, "fill writes attribute to DMA");
        }
    }

    /// The capture buffer is in neither frozen currency: arming + capturing never moves the state_hash-hashed
    /// regions beyond the real writes, and a quiesced (drained) buffer round-trips a snapshot as empty.
    #[test]
    fn capture_buffer_is_neither_currency_and_empties_at_boundaries() {
        let mut v = fresh();
        arm_vram_write(&mut v, 0x0100);
        v.set_write_capture(true);
        v.data_write(0xBEEF);
        v.take_write_captures(); // drain: buffer empty at the boundary
        v.set_write_capture(false);
        // A fresh VDP driven the same way (disarmed) has byte-identical hashed regions.
        let mut ref_vdp = fresh();
        arm_vram_write(&mut ref_vdp, 0x0100);
        ref_vdp.data_write(0xBEEF);
        assert_eq!(v.vram(), ref_vdp.vram(), "capture never perturbs VRAM");
        assert_eq!(v, ref_vdp, "drained capture buffer leaves the VDP equal");
    }

    // --- F-SCANLINE-SUBLINE slice 1: the mclk reaches the write chokes ------------------------------------

    /// Arm a CRAM word write at CRAM byte address 0, autoincrement 2, at mclk `arm`.
    fn arm_cram_write(v: &mut Vdp, arm: u64) {
        v.control_write(0x8F02, arm); // reg 15 = autoinc 2
        v.control_write(0xC000, arm); // CRAM write (code 0x03), A13-A0 = 0
        v.control_write(0x0000, arm); // high half, disarm the toggle
    }

    /// **§11.27's per-entry write stamp, on the GUEST path.** `write_target`'s CRAM arm is the store site
    /// every guest route funnels through — data port timed and untimed, DMA 68k→CRAM, DMA fill, the fill
    /// trigger — and the caveat on `emulator/pixel_attribution` is a comparison against what it records.
    ///
    /// **This test exists because the server-side rows could not reach it.** The wire conformance rows in
    /// `oracle-aether` drive their writes through `emulator/write_cram`, i.e. through
    /// [`poke_cram`](Vdp::poke_cram) — the *other* store site. Deleting the stamp from this arm left all
    /// of them green. So the arm is pinned here, where a guest write can actually be posed.
    ///
    /// Three properties, and the third is the one a shared helper would break: the stamp is **per entry**
    /// (a write to entry 0 does not date entry 1), it is the **write's own** instant rather than the
    /// arming control's, and an entry nobody has written reads `None` rather than a clock.
    #[test]
    fn a_guest_cram_write_stamps_that_entry_and_only_that_entry() {
        let mut v = fresh();
        arm_cram_write(&mut v, 0);
        for e in 0..64u8 {
            assert_eq!(
                v.cram_written_mclk(e),
                None,
                "entry {e}: arming a write is not writing one — an untouched entry has no instant, \
                 and `None` is not `Some(0)` (the caveat's rule distinguishes them at line 0 of \
                 frame 0)"
            );
        }

        let m = 100 * MCLK_PER_LINE + 1775;
        v.data_write_at(0x0EEE, m); // lands in entry 0; autoinc leaves the address on entry 1
        assert_eq!(
            v.cram_written_mclk(0),
            Some(m),
            "the written entry carries the WRITE's instant, not the arming control's (mclk 0)"
        );
        for e in 1..64u8 {
            assert_eq!(
                v.cram_written_mclk(e),
                None,
                "entry {e} was not written; a stamp here would make the caveat fire on colours \
                 nobody touched, which is the coarse rule §11.27 lets a server fall back to and \
                 NOT the one this server implements"
            );
        }

        // The autoincrement moved the address on, so the next write dates a different entry.
        let m2 = m + 4 * MCLK_PER_LINE;
        v.data_write_at(0x0888, m2);
        assert_eq!(v.cram_written_mclk(1), Some(m2), "entry 1, its own instant");
        assert_eq!(
            v.cram_written_mclk(0),
            Some(m),
            "and entry 0 keeps ITS instant — the stamps are per entry, not a single last-write clock"
        );
    }

    /// **The poke path stamps too, and takes its instant from the caller.** `emulator/write_cram` is a
    /// debug poke with no instruction behind it, so [`poke_cram`](Vdp::poke_cram) cannot read a clock off
    /// the machine the way the guest path does — see its own note on why `now_mclk` would be the wrong
    /// one. What it must not do is skip the stamp: a repaint of an entry the raster already drew is
    /// exactly the divergence §11.27 exists to disclose.
    ///
    /// It also must not advance `now_mclk`. That value belongs to guest-driven work; moving it here would
    /// relocate the *next* guest write's stamp, and a debugger poke must not change what the machine
    /// reports about the machine.
    #[test]
    fn a_poke_stamps_the_entry_with_the_callers_instant_and_moves_nothing_else() {
        let mut v = fresh();
        assert_eq!(v.cram_written_mclk(7), None);
        let before_now = v.now_mclk();

        v.poke_cram(7, 0x0EEE, 12_345);
        assert_eq!(
            v.cram_written_mclk(7),
            Some(12_345),
            "the poke's stamp is the instant the CALLER passed — the only honest source, since a poke \
             has no instruction to take one from"
        );
        assert_eq!(
            v.cram_written_mclk(6),
            None,
            "and its neighbours are untouched"
        );
        assert_eq!(v.cram_written_mclk(8), None);
        assert_eq!(
            v.now_mclk(),
            before_now,
            "a poke is not VDP work: advancing `now_mclk` here would relocate the next GUEST write's \
             stamp, which is a debugger changing what the machine reports about itself"
        );
    }

    /// A bus-timed CRAM data-port write carries its own instant into the VDP: `data_write_at(w, m)` leaves
    /// the VDP's notion of "now" at `m`, which is the clock the CRAM choke runs under. The write is checked
    /// to have actually landed, so the stamp is the stamp of a write that happened.
    #[test]
    fn a_cram_data_write_stamps_the_vdps_now_with_the_writes_own_mclk() {
        let mut v = fresh();
        arm_cram_write(&mut v, 0);
        assert_eq!(v.now_mclk(), 0, "the arming control writes were at mclk 0");

        // A landing well inside line 100's active display — the shape §B of the subline recon maps to a pixel.
        let m = 100 * MCLK_PER_LINE + 1775;
        v.data_write_at(0x0EEE, m);
        assert_eq!(
            v.now_mclk(),
            m,
            "the data write's own instant reached the VDP"
        );
        assert_eq!(
            ((v.cram()[0] as u16) << 8) | v.cram()[1] as u16,
            0x0EEE,
            "and it is the instant of a write that really landed in CRAM"
        );

        // It tracks, rather than latching once: a second write at a later instant re-stamps.
        let m2 = m + 3 * MCLK_PER_LINE;
        v.data_write_at(0x0AAA, m2);
        assert_eq!(v.now_mclk(), m2, "each timed write re-stamps");
    }

    /// The other three timed entry points stamp too, so no write choke can be reached under a stale clock:
    /// a control write (which can itself write a register), a fill body and a copy body.
    #[test]
    fn every_timed_entry_point_stamps_the_vdps_now() {
        let mut v = fresh();
        v.control_write(0x8F02, 11_111);
        assert_eq!(v.now_mclk(), 11_111, "control_write stamps");

        arm_vram_write(&mut v, 0x0200);
        v.run_fill(4, 0xAB00, 22_222);
        assert_eq!(v.now_mclk(), 22_222, "run_fill stamps");

        v.run_copy(0x0200, 4, 33_333);
        assert_eq!(v.now_mclk(), 33_333, "run_copy stamps");
    }

    /// **Decision C-6, pinned as a decision rather than left an accident.** Every word of a 68k→VDP burst
    /// carries the *transfer's* instant — not the instant the transfer was armed, and not a per-word clock.
    /// The shadow is checked after **each** word (a single post-burst check could not tell "every word" from
    /// "the last word"), and the burst is armed at a deliberately different mclk so an unstamped word would
    /// be visibly stale rather than accidentally right.
    #[test]
    fn a_mem_dma_burst_stamps_every_word_with_the_transfers_own_now() {
        const ARMED_AT: u64 = 40 * MCLK_PER_LINE;
        const TRANSFER_AT: u64 = 100 * MCLK_PER_LINE + 1775;

        let mut v = fresh();
        arm_cram_write(&mut v, ARMED_AT);
        assert_eq!(v.now_mclk(), ARMED_AT, "armed under the earlier clock");
        v.set_write_capture(true);

        for (i, w) in [0x0EEEu16, 0x0AAA, 0x0666, 0x0222].into_iter().enumerate() {
            v.dma_write_word(w, TRANSFER_AT);
            assert_eq!(
                v.now_mclk(),
                TRANSFER_AT,
                "word {i} of the burst carries the transfer's own now, not the arming clock"
            );
        }
        let caps = v.take_write_captures();
        assert_eq!(caps.len(), 4, "all four words reached the CRAM choke");
        assert!(
            caps.iter().all(|c| c.target == VdpTarget::Cram),
            "at the CRAM choke, which is the one the sub-line arc needs timed: {caps:?}"
        );
    }

    // --- F-SCANLINE-SUBLINE slice 2: the mclk -> pixel-x mapping ------------------------------------------

    /// The pixel axis end to end in both modes: the first pixel of the line, the pixel-clock boundary, the
    /// last active pixel, and the blanking cases that must stay line-atomic.
    #[test]
    fn subline_x_maps_the_active_window_onto_the_pixel_axis() {
        // H40: 8 mclk per pixel, 320 pixels.
        for (d, x) in [(0u64, 0usize), (7, 0), (8, 1), (2552, 319), (2559, 319)] {
            assert_eq!(subline_x(d, true), x, "H40 d={d}");
        }
        // H32: 10 mclk per pixel, 256 pixels.
        for (d, x) in [(0u64, 0usize), (9, 0), (10, 1), (2550, 255), (2559, 255)] {
            assert_eq!(subline_x(d, false), x, "H32 d={d}");
        }
        // Blanking: the write misses this row entirely and lands whole on the next one — today's behaviour,
        // which the mapping must not disturb.
        for d in [MCLK_PER_ACTIVE, MCLK_PER_ACTIVE + 1, MCLK_PER_LINE - 1] {
            assert_eq!(subline_x(d, true), 320, "H40 blanking d={d}");
            assert_eq!(subline_x(d, false), 256, "H32 blanking d={d}");
        }
    }

    /// **The evidence check.** The mapping reproduces a landing pixel the *demand side* derived
    /// independently, from their own measurement rather than from anything in this tree.
    ///
    /// Aeon's HBlank sweep places the shipped `SPIN_CRAM = 4` burst's first CRAM write *"253.6 cycles into
    /// line 100, at pixel ≈ 222 of 320"*
    /// (`aeon/docs/benchmarks/scanline-p2/HBLANK-WINDOW-SWEEP-RESULTS.md:410-413`), a figure they compute
    /// from the measured N = 27.5 crossing plus H40 line geometry and describe as *"independent of any
    /// emulator-internal constant"* (`:403-404`).
    ///
    /// This case is also what **discriminates the two candidate grids** (decision B-1): the 422-position
    /// H-counter grid answers 219 for the same instant, and only the 8-mclk pixel reproduces the measurement.
    #[test]
    fn subline_x_reproduces_the_demand_sides_measured_landing_pixel() {
        // 253.6 CPU cycles, carried in tenths so the conversion stays integer. MCLK_PER_CPU_CYCLE is the
        // tree's one CPU-cycle -> mclk constant, so the derivation reads from source rather than a literal.
        let d = 2536 * crate::system::MCLK_PER_CPU_CYCLE / 10;
        assert_eq!(d, 1775, "253.6 CPU cycles into the line, in mclk");
        assert_eq!(
            subline_x(d, true),
            221,
            "the mapping lands where Aeon measured, to within its own rounding (they report ~222)"
        );
    }

    /// The width `subline_x` clamps to is the width the renderer actually draws — one anchor, no drift.
    #[test]
    fn subline_x_clamps_to_the_width_the_renderer_reports() {
        let mut v = fresh();
        v.control_write(0x8C81, 0); // reg 12 bits 7+0 -> H40
        assert!(v.h40(), "the fixture really is in H40");
        assert_eq!(
            subline_x(MCLK_PER_ACTIVE, true) as u16,
            v.active_display().0,
            "H40 clamp == the renderer's H40 width"
        );
        v.control_write(0x8C00, 0); // reg 12 = 0 -> H32
        assert!(!v.h40(), "and really is in H32");
        assert_eq!(
            subline_x(MCLK_PER_ACTIVE, false) as u16,
            v.active_display().0,
            "H32 clamp == the renderer's H32 width"
        );
    }

    // --- F-TRACE-VDPWRITE-MCLK (slice 1b): the capture carries the write's own instant --------------------

    /// A captured CRAM write reports the clock **it** was performed at, not the clock of whatever the VDP
    /// last did. The fixture arms at one instant and writes at another, so borrowing the wrong one fails.
    #[test]
    fn a_captured_cram_write_carries_the_writes_own_mclk() {
        const ARMED_AT: u64 = 40 * MCLK_PER_LINE;
        const WRITTEN_AT: u64 = 100 * MCLK_PER_LINE + 1775;

        let mut v = fresh();
        arm_cram_write(&mut v, ARMED_AT);
        v.set_write_capture(true);
        v.data_write_at(0x0EEE, WRITTEN_AT);

        let caps = v.take_write_captures();
        assert_eq!(caps.len(), 1, "one word capture for CRAM");
        assert_eq!(
            caps[0].mclk, WRITTEN_AT,
            "the capture carries the write's own instant"
        );
        assert_ne!(
            caps[0].mclk, ARMED_AT,
            "and not the arming command's, which is the clock it would have borrowed before"
        );
    }

    /// **Decision C-6, now visible to a consumer.** Every captured word of one 68k→VDP burst reports the
    /// transfer's instant — one boundary per burst, not one per word and not an advancing clock.
    #[test]
    fn every_captured_dma_word_reports_the_transfers_instant() {
        const ARMED_AT: u64 = 40 * MCLK_PER_LINE;
        const TRANSFER_AT: u64 = 100 * MCLK_PER_LINE + 1775;

        let mut v = fresh();
        arm_cram_write(&mut v, ARMED_AT);
        v.set_write_capture(true);
        for w in [0x0EEEu16, 0x0AAA, 0x0666, 0x0222] {
            v.dma_write_word(w, TRANSFER_AT);
        }

        let caps = v.take_write_captures();
        assert_eq!(caps.len(), 4);
        assert_eq!(
            caps.iter().map(|c| c.mclk).collect::<Vec<_>>(),
            vec![TRANSFER_AT; 4],
            "one burst, one instant (C-6; F-SUBLINE-DMASPREAD is the follow-up that would smear it)"
        );
    }
}
