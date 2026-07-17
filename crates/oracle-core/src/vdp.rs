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
/// Master-clock ticks per NTSC frame (`MCLK_PER_LINE * LINES_PER_FRAME` = 896_040).
pub const MCLK_PER_FRAME: u64 = MCLK_PER_LINE * LINES_PER_FRAME;

/// The V-counter value at which the vertical-blank status flag sets — line 224 (the `0xDF`→`0xE0`
/// transition, recon R2). Also the first non-active line.
const VBLANK_START_LINE: u64 = 0xE0;

/// SAT-cache size: 80 sprite entries × the **cached 4 bytes** of each 8-byte entry — Y (word) + size/link
/// (word); recon R5 / RR8. X + tile/attr (the other 4 bytes) are never cached.
const SAT_CACHE_LEN: usize = 320;

/// The VDP's owned state. The four hashed regions are always allocated at their fixed hardware sizes
/// ([`crate::state_hash`]); the `state_hash`/`export_state` currencies read straight through them, so their
/// byte layout is frozen.
/// One write-FIFO slot (recon R3): the data word plus a copy of the command code/address registers as they
/// were when the write was enqueued. The physical slot **retains** its data after the entry drains (the pending
/// count drops but the bytes stay) — that stale data is exactly what the CRAM/VSRAM snoop quirk and the
/// VRAM-fill data source read ("the data written 4 writes ago").
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, bincode::Encode, bincode::Decode)]
struct FifoEntry {
    data: u16,
    code: u8,
    addr: u16,
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
    /// Sprite-overflow status latch (status bit 6). Set by the render pipeline (push 4); serialized here so
    /// the status word can report it. Read-only this push.
    sprite_overflow: bool,
    /// Sprite-collision status latch (status bit 5). Set by the render pipeline (push 4); read-only here.
    sprite_collision: bool,
    /// Odd-frame flag (status bit 4). Toggled each frame when the VInt latch is set (recon R12 delivery).
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
    /// Power-on = false. Committed by [`Vdp::render_scanline`]; the pure `render_line` seeds from it read-only.
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
    /// The mclk up to which the FIFO has been drained (recon R3) — the coarse drain clock. `fifo_drain`
    /// advances it one entry at a time at the pinned per-line slot rate (2 slots per VRAM word, 1 per
    /// CRAM/VSRAM word; 16/18 active / 167/205 blanked slots per line). Real timing state, serialized; in
    /// neither frozen currency. Power-on = 0.
    fifo_slot_clock: u64,
    /// mclk before which the status DMA-busy bit (bit 1) reads set (recon R4 / Eke: DMA-busy sets on the
    /// control-port setup write; a fill/copy runs the 68k in parallel, so a poll sees busy for the coarse
    /// transfer window). Power-on = 0 (never busy). In neither frozen currency.
    dma_busy_until: u64,
}

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
        }
    }

    /// Read-only access to VRAM (for the `state_hash`/`export_state` currencies and introspection).
    pub fn vram(&self) -> &[u8] {
        &self.vram
    }

    /// Read-only access to CRAM (Oracle byte layout).
    pub fn cram(&self) -> &[u8] {
        &self.cram
    }

    /// Read-only access to VSRAM.
    pub fn vsram(&self) -> &[u8] {
        &self.vsram
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
    /// [`Vdp::render_scanline`] advances it.
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

    /// Mutable access to VRAM (used by tests to perturb state; the data-port write path lands in a later
    /// slice). Kept crate-internal-friendly but public for the `System::vram_mut` pass-through.
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

    /// VBlank status flag: set across the whole vertical-blank region — V counter ≥ `0xE0` (line ≥ 224, the
    /// `0xDF`→`0xE0` transition; recon R2). Pure function of mclk, no stored flag.
    pub fn vblank(&self, mclk: u64) -> bool {
        (mclk % MCLK_PER_FRAME) / MCLK_PER_LINE >= VBLANK_START_LINE
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
    /// sprite-overflow, b7 VINT(F), b8 FIFO-full, b9 FIFO-empty. This slice fills the FIFO-empty placeholder
    /// (the FIFO drains immediately this push) + the vblank/hblank timing bits; the interrupt / sprite /
    /// odd-frame bits land with their state in later slices. Not yet wired to the control port — that is the
    /// ports slice (which also clears the pending toggle on a status read).
    pub fn status_word(&self, mclk: u64) -> u16 {
        let mut s = 1u16 << 9; // FIFO empty (placeholder: immediate drain this push)
        if mclk < self.dma_busy_until {
            s |= 1 << 1; // DMA busy (recon R4 / Eke): set across a fill/copy's coarse transfer window
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
        if self.vblank(mclk) {
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
    fn target(&self) -> Target {
        Self::target_of(self.code)
    }

    /// The data-port target region for an arbitrary command `code` (recon R1) — the FIFO stores the code at
    /// enqueue time, so the drain/snoop path decodes a captured code rather than the live one.
    fn target_of(code: u8) -> Target {
        match code & 0x0F {
            0x3 | 0x8 => Target::Cram,  // CRAM write (0x3) / CRAM read (0x8)
            0x4 | 0x5 => Target::Vsram, // VSRAM read (0x4) / write (0x5)
            _ => Target::Vram,          // VRAM read (0x0) / write (0x1); unknown → VRAM
        }
    }

    /// Enqueue a data-port write into the FIFO ring (recon R3): capture the data word plus the live
    /// code/address registers. The physical slot is overwritten in place (retaining nothing of the old entry
    /// beyond the ring position), and `fifo_len` counts pending entries (saturating at 4 until the drain clock
    /// advances it in the wait-channel slice).
    fn fifo_enqueue(&mut self, data: u16) {
        self.fifo[self.fifo_write as usize] = FifoEntry {
            data,
            code: self.code,
            addr: self.addr,
        };
        self.fifo_write = (self.fifo_write + 1) & 3;
        self.fifo_len = (self.fifo_len + 1).min(4);
    }

    /// The number of pending (not-yet-drained) FIFO entries, 0..=4 (recon R3; introspection / debuggers).
    pub fn fifo_len(&self) -> u8 {
        self.fifo_len
    }

    /// External (CPU/DMA) access slots per line at `mclk` (recon R3): active display H32 = 16 / H40 = 18;
    /// a blanked line (vblank or display-off) H32 = 167 / H40 = 205. One slot = one VRAM byte access.
    fn slots_per_line(&self, mclk: u64) -> u64 {
        let blanked = self.vblank(mclk) || (self.regs[1] & 0x40) == 0;
        match (self.h40(), blanked) {
            (false, false) => 16,
            (true, false) => 18,
            (false, true) => 167,
            (true, true) => 205,
        }
    }

    /// The mclk cost of draining one FIFO entry with command `code` at slot-clock instant `at` (recon R3): a
    /// VRAM word exits in 2 slots, a CRAM/VSRAM word in 1, at the per-line slot rate. Integer throughout.
    fn entry_drain_cost(&self, code: u8, at: u64) -> u64 {
        let slots = match Self::target_of(code) {
            Target::Vram => 2,
            _ => 1,
        };
        slots * MCLK_PER_LINE / self.slots_per_line(at)
    }

    /// Advance the FIFO drain clock up to `now` (recon R3): pop each pending entry whose slot cost has elapsed.
    /// When the FIFO empties, the clock coasts forward to `now` (an idle FIFO does not bank drain credit).
    fn fifo_drain(&mut self, now: u64) {
        while self.fifo_len > 0 {
            let oldest = self.fifo[(self.fifo_write.wrapping_sub(self.fifo_len) & 3) as usize];
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
    /// ago). Recon R3: this is what the CRAM/VSRAM read snoop quirk and the CRAM/VSRAM VRAM-fill data source
    /// read. Exposed for introspection; consumed internally by the snoop/fill slices.
    pub fn fifo_snoop_word(&self) -> u16 {
        self.fifo[self.fifo_write as usize].data
    }

    /// Whether the DMA-busy status bit reads set at `mclk` (recon R4): true while a fill/copy's coarse transfer
    /// window is still open. Introspection companion to the status word's bit 1.
    pub fn dma_busy(&self, mclk: u64) -> bool {
        mclk < self.dma_busy_until
    }

    /// Read the current-address word from the data-port target (recon R1/R3). VRAM/CRAM/VSRAM are stored
    /// big-endian (high byte first) — the `state_hash` currency's byte layout.
    fn read_target(&self) -> u16 {
        match self.target() {
            Target::Vram => {
                let b = (self.addr & 0xFFFE) as usize;
                ((self.vram[b] as u16) << 8) | self.vram[b | 1] as u16
            }
            Target::Cram => {
                let b = (self.addr as usize) & 0x7E;
                ((self.cram[b] as u16) << 8) | self.cram[b | 1] as u16
            }
            Target::Vsram => {
                let b = ((self.addr & 0xFFFE) as usize) % VSRAM_SIZE;
                ((self.vsram[b] as u16) << 8) | self.vsram[b | 1] as u16
            }
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
        self.vram[a] = byte;
        let base = self.sat_base();
        let entries = if self.h40() { 80 } else { 64 };
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
            Target::Vram => {
                // VRAM odd-address byte-swap (recon R3): high byte → `addr`, low byte → `addr ^ 1`, so an
                // odd address swaps the two bytes of the word. (Implementation-time pin, unit-tested below.)
                // Both bytes route through `write_vram_byte` so the SAT-cache write-through sees every byte
                // (recon R5 / RR8; byte-granular — RR8 open-remainder 2, odd-address SAT writes).
                let a = self.addr as usize;
                self.write_vram_byte(a & (VRAM_SIZE - 1), (w >> 8) as u8);
                self.write_vram_byte((a ^ 1) & (VRAM_SIZE - 1), (w & 0xFF) as u8);
            }
            Target::Cram => {
                let masked = w & 0x0EEE; // 9-bit colour (---- BBB- GGG- RRR-)
                let b = (self.addr as usize) & 0x7E;
                self.cram[b] = (masked >> 8) as u8;
                self.cram[b | 1] = (masked & 0xFF) as u8;
            }
            Target::Vsram => {
                let masked = w & 0x07FF; // 11-bit vertical scroll
                let b = ((self.addr & 0xFFFE) as usize) % VSRAM_SIZE;
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
        if !self.pending {
            if (w >> 14) & 0x03 == 0b10 {
                // Register write: reg = bits 12..8, value = bits 7..0. Does not arm the toggle.
                self.write_register(((w >> 8) & 0x1F) as usize, (w & 0xFF) as u8, mclk);
                return;
            }
            // First command word: apply the low half (CD1-CD0 + A13-A0) to the live registers immediately.
            self.addr = (self.addr & 0xC000) | (w & 0x3FFF);
            self.code = (self.code & 0x3C) | ((w >> 14) & 0x03) as u8;
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
        }
    }

    /// A control-port (status) read ($C00004/6; recon R2 timing bits). **Clears the pending toggle** — the
    /// instrument-sourced experiment pin (permitted docs are silent; see the pending-toggle experiment in
    /// `docs/2026-07-16-vdp-recon.md`, same standing as the STOP×trace pin). Also **clears the sprite-overflow
    /// and collision status latches** (Sega Genesis Software Manual — those two flags are cleared by reading
    /// the status). Golden-safe: the frozen currencies do not include these fields, and the test ROM drives no
    /// sprite rendering, so both are `false` here regardless.
    pub fn control_read_status(&mut self, mclk: u64) -> u16 {
        self.pending = false;
        let s = self.status_word(mclk);
        self.sprite_overflow = false;
        self.sprite_collision = false;
        s
    }

    /// A data-port write ($C00000/2; recon R1). Clears the toggle, routes the word to VRAM/CRAM/VSRAM, and
    /// auto-increments. A CD5 (DMA) command latches state and does nothing here — DMA is push 6 (documented
    /// placeholder); the toggle still clears and the address does not advance.
    pub fn data_write(&mut self, w: u16) {
        self.apply_data_write(w);
    }

    /// The write body shared by the untimed [`Vdp::data_write`] and the bus-timed [`Vdp::data_write_at`]:
    /// clear the toggle, and on a non-DMA command enqueue into the FIFO (recon R3, contents + timing) then
    /// apply the write (enqueue-immediate model — see the `fifo` field docs; the enqueue captures the
    /// pre-autoincrement address, per the pin).
    fn apply_data_write(&mut self, w: u16) {
        self.pending = false;
        if self.code & 0x20 != 0 {
            return; // CD5/DMA: placeholder no-op until the DMA-mode slices
        }
        self.fifo_enqueue(w);
        self.write_target(w);
        self.autoinc();
    }

    /// A bus-timed data-port write (recon R1/R3): drain the FIFO up to `now`, stall the 68k if the FIFO is
    /// full (returning the wait in **CPU cycles** for the `Bus68k` channel — `MegaDriveBus` folds it into the
    /// instruction cost; `FlatBus` never reaches this path so the SST corpus is untouched), then apply the
    /// write. A 5th write while all 4 slots are pending waits for the oldest to drain (official /DTACK stall).
    pub fn data_write_at(&mut self, w: u16, now: u64) -> u32 {
        // A DMA command's data write is not FIFO-timed here (the DMA slices own it); no stall, apply as before.
        if self.code & 0x20 != 0 {
            self.apply_data_write(w);
            return 0;
        }
        self.fifo_drain(now);
        let mut wait_mclk = 0u64;
        if self.fifo_len == 4 {
            let oldest = self.fifo[(self.fifo_write.wrapping_sub(self.fifo_len) & 3) as usize];
            let cost = self.entry_drain_cost(oldest.code, self.fifo_slot_clock);
            let drain_at = self.fifo_slot_clock + cost;
            wait_mclk = drain_at.saturating_sub(now);
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
        // Snoop quirk (recon R3): a CRAM/VSRAM data-port read fills its UNDEFINED bits (above the 9-bit CRAM /
        // 11-bit VSRAM masks) from the next-available FIFO entry (the word written 4 writes ago). VRAM reads
        // are fully defined — no snoop. Behavioral, currency-safe (rendering + the hashed currencies read the
        // stored bytes directly, never through `data_read`).
        match self.target() {
            Target::Cram => out = (out & 0x0EEE) | (self.fifo_snoop_word() & !0x0EEE),
            Target::Vsram => out = (out & 0x07FF) | (self.fifo_snoop_word() & !0x07FF),
            Target::Vram => {}
        }
        self.autoinc();
        self.read_buffer = self.read_target();
        out
    }

    /// A bus-timed data-port read (recon R1/R3): a read waits for the write FIFO to drain first (pending
    /// writes take priority over reads), then returns the pre-cached word (with the CRAM/VSRAM snoop merge).
    /// Returns the value plus the CPU wait cycles for the `Bus68k` channel (`FlatBus` never reaches here → the
    /// SST corpus is untouched).
    pub fn data_read_at(&mut self, open_bus: u16, now: u64) -> (u16, u32) {
        self.fifo_drain(now);
        let mut wait_mclk = 0u64;
        // Reads wait for the write FIFO to empty (recon R3): drain every pending entry, banking the elapsed
        // time as the read's stall.
        while self.fifo_len > 0 {
            let oldest = self.fifo[(self.fifo_write.wrapping_sub(self.fifo_len) & 3) as usize];
            let cost = self.entry_drain_cost(oldest.code, self.fifo_slot_clock);
            self.fifo_slot_clock += cost;
            self.fifo_len -= 1;
            wait_mclk = self.fifo_slot_clock.saturating_sub(now);
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

    /// Set the VINT pending latch and toggle the odd-frame flag (recon R12; the VInt scheduler event at line
    /// 224 drives this). The latch is cleared only by [`Vdp::acknowledge`].
    pub fn raise_vint(&mut self) {
        self.vint_pending = true;
        self.odd_frame = !self.odd_frame;
    }

    /// Set the HINT pending latch (recon R12; an HInt scheduler event drives this on HINT-counter underflow).
    pub fn raise_hint(&mut self) {
        self.hint_pending = true;
    }

    /// Commit one scanline's sprite latches (recon R10), driven by [`Vdp::render_scanline`]. Sets the R10
    /// masking carry to **this line's** dot overflow (so the next line's first-on-line x=0 sprite masks), and
    /// **ORs** the sprite-overflow (status bit 6) / collision (status bit 5) status latches — sticky until a
    /// status read clears them. Kept in `vdp.rs` (the owner of these serialized fields); the renderer computes
    /// the deltas and hands them here.
    pub fn commit_scanline_sprites(&mut self, dot_overflow: bool, overflow: bool, collision: bool) {
        self.sprite_dot_overflow_carry = dot_overflow;
        self.sprite_overflow |= overflow;
        self.sprite_collision |= collision;
    }

    /// Per-line HINT-counter bookkeeping (recon R7), driven at each line start by the Scanline event.
    /// Returns `true` on HINT-counter underflow (the caller schedules an HInt for this line):
    /// - vblank lines 225..=261 reload the counter from reg 10 (no decrement, no HINT);
    /// - active lines 0..=224 decrement; on underflow the counter reloads from reg 10 and HINT fires
    ///   (so reg10 = N → HINT on lines N, 2N+1, 3N+2, …; reg10 = 0 → every line 0..=224, incl. line 224).
    pub fn on_line_start(&mut self, line: u16) -> bool {
        if (225..=261).contains(&line) {
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
    // The wire-protocol wrapping (the Oracle-parity ops) is out of scope this push; these are the pure,
    // state-derived primitives the API owes now so it does not accrete later. render_line_report /
    // pixel_attribution land with their pipeline stages (pushes 3–5).

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
}

/// The fixed introspection colour ramp: a 3-bit channel level (`0..=7`) → 8-bit, linear (`level × 255 / 7`).
fn ramp3(level: u8) -> u8 {
    (level as u16 * 255 / 7) as u8
}

/// The data-port target region, decoded from the command code's low nibble (recon R1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Target {
    Vram,
    Cram,
    Vsram,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_on_allocates_fixed_region_sizes() {
        let mut rng = SplitMix64::new(1);
        let vdp = Vdp::power_on(&mut rng);
        assert_eq!(vdp.vram().len(), VRAM_SIZE);
        assert_eq!(vdp.cram().len(), CRAM_SIZE);
        assert_eq!(vdp.vsram().len(), VSRAM_SIZE);
        assert_eq!(vdp.regs().len(), REG_COUNT);
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

    fn fresh() -> Vdp {
        Vdp::power_on(&mut SplitMix64::new(1))
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
        let v = fresh(); // H32
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
        v.control_read_status(0);
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
        v.control_read_status(0); // clears the control-port toggle, NOT the interrupt latches
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
    /// (mirrors the Scanline chain: reload during vblank 225..=261, then step lines 0..=224).
    fn hint_lines(reg10: u8) -> Vec<u16> {
        let mut v = fresh();
        v.regs[0x0A] = reg10;
        // Reload during a vblank line so we enter line 0 with the counter = reg10 (as the chain does).
        v.on_line_start(261);
        (0..=224).filter(|&line| v.on_line_start(line)).collect()
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
        v.on_line_start(261); // vblank reload → counter = 7 entering line 0
        assert_eq!(v.hint_counter, 7);
        // Step three active lines to draw the counter down…
        for line in 0..3 {
            v.on_line_start(line);
        }
        assert_eq!(v.hint_counter, 4, "7 → decremented three times");
        // …then a vblank line reloads it from reg 10.
        v.on_line_start(230);
        assert_eq!(v.hint_counter, 7, "reloaded from reg 10 during vblank");
    }

    #[test]
    fn raise_vint_toggles_the_odd_frame_flag() {
        let mut v = fresh();
        assert!(!v.odd_frame);
        v.raise_vint();
        assert!(
            v.vint_pending && v.odd_frame,
            "VINT set + odd-frame toggled"
        );
        v.raise_vint();
        assert!(!v.odd_frame, "toggled back on the next frame");
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

    #[test]
    fn fifo_and_dma_fields_survive_a_bincode_round_trip() {
        let mut v = fresh();
        vram_write_cmd(&mut v, 0x0200);
        v.data_write(0x1234);
        v.data_write(0x5678);
        v.dma_busy_until = 42_000;
        let bytes = bincode::encode_to_vec(&v, bincode::config::standard()).unwrap();
        let (back, _): (Vdp, usize) =
            bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
        assert_eq!(v, back, "the FIFO ring + DMA-busy deadline round-trip");
        assert_eq!(back.fifo_len, 2);
        assert_eq!(back.dma_busy_until, 42_000);
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
        let s = v.control_read_status(0);
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
            v.control_read_status(0) & 0x60,
            0,
            "a subsequent read sees them cleared"
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
}
