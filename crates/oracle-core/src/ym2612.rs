//! The YM2612 (OPN2) FM chip — **the timers**, the first piece of live FM-chip state.
//!
//! This slice models only Timer A and Timer B and the status byte their overflow flags drive; FM synthesis,
//! the full register file, and the DAC are out of scope (see `docs/2026-07-22-fm-timer-design.md`, FM10). It
//! exists because the Genesis SMPS sound driver clocks its sequencer off the **Timer-A overflow flag** in the
//! status byte (`$4000`/`$A04000` bit0): our old status stub hardwired that byte to `0x00`, so the flag never
//! set, the sequencer never ticked, and no song ever played. With the timers live, the driver's poll fires
//! once per period and the song's register stream appears (FM9).
//!
//! **Lazy evaluation (FM5/FM6).** No per-step advance and no scheduler event: the chip stores each timer's
//! configuration + phase anchors, and the overflow flag is a **pure function of (registers, current mclk)**
//! computed at status-read time. That makes it snapshot-safe and deterministic by construction — the flag at
//! time `now` depends only on the stored anchors and `now`, never on *how* `now` was reached.
//!
//! **Period model (FM1/FM2), pinned in master-clock ticks:**
//! - Timer A: 10-bit `NA` = (`$24` << 2) | (`$25` & 3); overflow period = `(1024 − NA) × 1008` mclk.
//! - Timer B: 8-bit `NB` = `$26`; overflow period = `(256 − NB) × 16128` mclk.
//!
//! **`$27` control byte:** bit0 LOAD_A, bit1 LOAD_B, bit2 ENBL_A, bit3 ENBL_B, bit4 RST_A, bit5 RST_B.
//!
//! **Status byte:** bit0 = Timer-A overflow, bit1 = Timer-B overflow, all other bits 0 — crucially **bit7
//! (BUSY) stays 0** (the Gunstar DR-1b busy-poll depends on never seeing BUSY set; FM3).

/// Timer-A tick = 1008 mclk (72 YM cycles at input/2 = 144 input cycles × mclk/7); overflow period is
/// `(1024 − NA)` ticks (FM2). Derived, corroborated by Plutiedev's 18.77 µs and the driver's N=137 → 60.05 Hz.
const TIMER_A_TICK_MCLK: u64 = 1008;
/// Timer-B tick = 16 × the Timer-A tick (the 8-bit vs 10-bit granularity); period is `(256 − NB)` ticks (FM2).
const TIMER_B_TICK_MCLK: u64 = 16 * TIMER_A_TICK_MCLK;

/// The lazy state of one free-running timer: whether it is counting (`LOAD`), whether an overflow raises the
/// status flag (`ENBL`), the mclk its counter last started (its phase anchor), and the mclk of the last flag
/// clear (RST or start). The overflow flag is derived from these + `now`, never stored.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, bincode::Encode, bincode::Decode)]
struct Timer {
    running: bool,
    enabled: bool,
    started_mclk: u64,
    cleared_mclk: u64,
}

impl Timer {
    /// Whether the overflow flag reads set at `now`, given the timer's `period` (in mclk): the timer must be
    /// running + enabled with a non-zero period, at least one period must have elapsed since it started, and
    /// the most recent overflow boundary must lie strictly after the last flag-clear mark (FM6). The
    /// `&&` short-circuit guarantees `now >= started_mclk` before the subtraction in `last_boundary`.
    fn overflow(&self, period: u64, now: u64) -> bool {
        self.running
            && self.enabled
            && period != 0
            && now >= self.started_mclk + period
            && last_boundary(self.started_mclk, period, now) > self.cleared_mclk
    }

    /// Apply a `$27` control write for this timer (FM6). `load`/`enbl`/`rst` are this timer's three control
    /// bits already extracted. A `LOAD` 0→1 edge starts the free-running counter (anchoring its phase and
    /// clearing the flag); `LOAD`=0 freezes it. `ENBL` is always latched. `RST` (applied last) clears the
    /// flag by advancing the clear-mark to `now`, leaving the phase anchor — and the free-run — intact.
    fn apply_control(&mut self, load: bool, enbl: bool, rst: bool, now: u64) {
        if load && !self.running {
            // 0→1 edge: start the counter running from `now`, flag cleared.
            self.running = true;
            self.started_mclk = now;
            self.cleared_mclk = now;
        } else if !load {
            self.running = false;
        }
        self.enabled = enbl;
        if rst {
            self.cleared_mclk = now;
        }
    }
}

/// The most recent overflow boundary at-or-before `now`: `started + k·period` for the largest `k` with that
/// value ≤ `now`. Callers guarantee `now >= started` and `period != 0`.
fn last_boundary(started: u64, period: u64, now: u64) -> u64 {
    started + ((now - started) / period) * period
}

/// The YM2612 timer + status model (this slice). Owned by `System` like the `Vdp`; plain owned data, so it
/// rides the bincode snapshot for determinism/rewind. It is **not** emitted by `export_state` (region 6 stays
/// the all-zero reserve) and is not in `state_hash`.
#[derive(Clone, PartialEq, Eq, Debug, Default, bincode::Encode, bincode::Decode)]
pub struct Ym2612 {
    /// The latched register number per bank: index 0 = bank 0 (`$4000`/`$4001`), index 1 = bank 1
    /// (`$4002`/`$4003`). An even-port (address) write latches here; the next odd-port (data) write targets it.
    addr_latch: [u8; 2],
    /// Timer-A reload value `NA` (10 bits): `$24` supplies bits 9..2, `$25` bits 1..0.
    na: u16,
    /// Timer-B reload value `NB` (8 bits): `$26`.
    nb: u8,
    a: Timer,
    b: Timer,
}

impl Ym2612 {
    /// A powered-on chip: all zero → neither timer running → status reads `0x00` (byte-identical to the old
    /// constant stub until a `$27` LOAD+ENBL write programs a timer).
    pub fn new() -> Self {
        Self::default()
    }

    /// Timer-A overflow period in mclk: `(1024 − NA) × 1008`. `NA` is 10-bit so this is always ≥ 1008 (never 0).
    fn a_period_mclk(&self) -> u64 {
        (1024u64.saturating_sub(self.na as u64)) * TIMER_A_TICK_MCLK
    }

    /// Timer-B overflow period in mclk: `(256 − NB) × 16128`. `NB` is 8-bit so this is always ≥ 16128 (never 0).
    fn b_period_mclk(&self) -> u64 {
        (256u64.saturating_sub(self.nb as u64)) * TIMER_B_TICK_MCLK
    }

    /// Handle a write to one of the chip's four ports, at either the `$4000-$4003` (Z80) or `$A04000-$A04003`
    /// (68k) window — the address-latch/data protocol (FM7, RT3 one-chip-two-windows). The window is
    /// normalized on the low two address bits: even = address latch, odd = data write; bit1 selects the bank.
    /// Only bank-0 timer registers `$24/$25/$26/$27` mutate timer state; every other data write (DAC, patch
    /// regs, bank-1 regs) is ignored here — those do not touch the timers.
    pub fn write_port(&mut self, port: u16, value: u8, now_mclk: u64) {
        let bank = ((port & 2) >> 1) as usize;
        if port & 1 == 0 {
            // Even port: latch the register number for this bank.
            self.addr_latch[bank] = value;
        } else {
            // Odd port: write to the latched register. Only bank-0 timer registers matter to the timers.
            if bank == 0 {
                self.write_reg(self.addr_latch[0], value, now_mclk);
            }
        }
    }

    /// Apply a data write to bank-0 register `reg` (FM1/FM6). Only the four timer registers are handled; any
    /// other register (DAC `$2A/$2B`, operator/patch regs, …) is a no-op for the timer model.
    fn write_reg(&mut self, reg: u8, value: u8, now_mclk: u64) {
        match reg {
            0x24 => self.na = (self.na & 0x0003) | ((value as u16) << 2),
            0x25 => self.na = (self.na & 0x03FC) | ((value as u16) & 0x0003),
            0x26 => self.nb = value,
            0x27 => self.write_control(value, now_mclk),
            _ => {}
        }
    }

    /// Apply a `$27` control-byte write to both timers (FM6). Load/enable/reset are decoded per-timer and
    /// handed to [`Timer::apply_control`] (RST applied after load/enable, per the pinned order).
    fn write_control(&mut self, byte: u8, now_mclk: u64) {
        self.a.apply_control(
            byte & 0x01 != 0, // LOAD_A
            byte & 0x04 != 0, // ENBL_A
            byte & 0x10 != 0, // RST_A
            now_mclk,
        );
        self.b.apply_control(
            byte & 0x02 != 0, // LOAD_B
            byte & 0x08 != 0, // ENBL_B
            byte & 0x20 != 0, // RST_B
            now_mclk,
        );
    }

    /// The register number currently latched for `bank` (0 = `$4000`/`$4001`, 1 = `$4002`/`$4003`) — the
    /// even-port half of the chip's latch-then-data protocol, i.e. the register the *next* odd-port data
    /// write will target. Read-only introspection (`F-TRACE-EXPOSE-LATCHES`); powers on `0`.
    ///
    /// Sampling caveat: this is the latch *now*. A sink decoding a write stream into (register, value)
    /// triples sees the latch as it was at each write and must keep its own running decode state — which
    /// is exactly why `VgmLogger` and `AudioSink` keep theirs.
    ///
    /// # Panics
    /// If `bank > 1`.
    pub fn addr_latch(&self, bank: usize) -> u8 {
        self.addr_latch[bank]
    }

    /// The status byte at `$4000`/`$A04000` (and mirrored across `$4001-$4003`/`$A04001-$A04003`): bit0 =
    /// Timer-A overflow, bit1 = Timer-B overflow, **bit7 = 0** (BUSY never set — the Gunstar DR-1b poll
    /// depends on it, FM3), all other bits 0. Pure function of the stored anchors + `now_mclk`, no side effect.
    pub fn read_status(&self, now_mclk: u64) -> u8 {
        let mut status = 0u8;
        if self.a.overflow(self.a_period_mclk(), now_mclk) {
            status |= 0x01;
        }
        if self.b.overflow(self.b_period_mclk(), now_mclk) {
            status |= 0x02;
        }
        status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The driver's fixed Timer-A tuning: NA = 137 → period (1024 − 137) × 1008 = 894096 mclk ≈ 60.05 Hz.
    const NA: u16 = 137;
    const A_PERIOD: u64 = (1024 - NA as u64) * TIMER_A_TICK_MCLK; // 894096

    /// Program Timer A with NA and start it (LOAD|ENBL) at mclk `t`, the driver's init sequence
    /// (`$24`,`$25`,`$27=$05`).
    fn program_timer_a(fm: &mut Ym2612, t: u64) {
        // $24 = NA high 8 bits, $25 = NA low 2 bits.
        fm.write_port(0x4000, 0x24, t);
        fm.write_port(0x4001, (NA >> 2) as u8, t);
        fm.write_port(0x4000, 0x25, t);
        fm.write_port(0x4001, (NA & 3) as u8, t);
        // $27 = $05 = LOAD_A | ENBL_A.
        fm.write_port(0x4000, 0x27, t);
        fm.write_port(0x4001, 0x05, t);
    }

    /// `F-TRACE-EXPOSE-LATCHES`: `addr_latch(bank)` reports the register number the even-port write
    /// latched, **per bank**. The two banks are deliberately given different values and read back in the
    /// opposite order, so an accessor that ignores `bank` (or returns a constant) fails.
    #[test]
    fn addr_latch_reports_the_latched_register_per_bank() {
        let mut fm = Ym2612::new();
        assert_eq!(fm.addr_latch(0), 0x00, "bank 0 powers on at zero");
        assert_eq!(fm.addr_latch(1), 0x00, "bank 1 powers on at zero");

        // $4000 = bank-0 address port, $4002 = bank-1 address port.
        fm.write_port(0x4000, 0x27, 0);
        fm.write_port(0x4002, 0xB4, 0);
        assert_eq!(fm.addr_latch(1), 0xB4, "bank 1 holds its own latch");
        assert_eq!(fm.addr_latch(0), 0x27, "bank 0 is untouched by bank 1");

        // Odd ports are data writes: they target the latch, they do not replace it.
        fm.write_port(0x4001, 0x05, 0);
        fm.write_port(0x4003, 0xC0, 0);
        assert_eq!(
            fm.addr_latch(0),
            0x27,
            "bank 0 latch survives its data write"
        );
        assert_eq!(
            fm.addr_latch(1),
            0xB4,
            "bank 1 latch survives its data write"
        );
    }

    #[test]
    fn timer_a_period_matches_the_driver_tuning() {
        let fm = {
            let mut f = Ym2612::new();
            program_timer_a(&mut f, 0);
            f
        };
        assert_eq!(fm.a_period_mclk(), A_PERIOD, "NA=137 → 894096 mclk");
    }

    #[test]
    fn timer_a_flag_sets_after_one_period_and_stays_set() {
        let t = 1_000_000u64;
        let mut fm = Ym2612::new();
        program_timer_a(&mut fm, t);
        // Just after starting: no period has elapsed, flag clear.
        assert_eq!(fm.read_status(t) & 1, 0, "flag clear just after LOAD");
        assert_eq!(
            fm.read_status(t + A_PERIOD - 1) & 1,
            0,
            "clear one mclk shy"
        );
        // Exactly one period out: the flag latches.
        assert_eq!(fm.read_status(t + A_PERIOD) & 1, 1, "flag set at t+period");
        // Further out (multiple periods) it stays set — a single sticky bit, not an edge counter.
        assert_eq!(fm.read_status(t + A_PERIOD + 1) & 1, 1, "stays set");
        assert_eq!(fm.read_status(t + 5 * A_PERIOD) & 1, 1, "stays set way out");
    }

    #[test]
    fn timer_a_rst_clears_the_flag_and_it_re_sets_free_running() {
        let t = 0u64;
        let mut fm = Ym2612::new();
        program_timer_a(&mut fm, t);
        // Let it overflow.
        let after_ovf = t + A_PERIOD;
        assert_eq!(fm.read_status(after_ovf) & 1, 1, "set after one period");
        // Rearm: $27 = $15 = LOAD | ENBL | RST — LOAD stays high (no restart), RST clears the flag.
        fm.write_port(0x4000, 0x27, after_ovf);
        fm.write_port(0x4001, 0x15, after_ovf);
        assert_eq!(fm.read_status(after_ovf) & 1, 0, "RST clears the flag");
        // The counter kept its phase (free-run): the flag re-sets at the NEXT boundary (started + 2·period).
        assert_eq!(
            fm.read_status(t + 2 * A_PERIOD - 1) & 1,
            0,
            "still clear one mclk before the next boundary"
        );
        assert_eq!(
            fm.read_status(t + 2 * A_PERIOD) & 1,
            1,
            "re-sets at the next free-running boundary"
        );
    }

    #[test]
    fn timer_a_not_enabled_never_sets_the_flag() {
        let t = 0u64;
        let mut fm = Ym2612::new();
        // Program NA then LOAD without ENBL ($27 = $01 = LOAD_A only).
        fm.write_port(0x4000, 0x24, t);
        fm.write_port(0x4001, (NA >> 2) as u8, t);
        fm.write_port(0x4000, 0x25, t);
        fm.write_port(0x4001, (NA & 3) as u8, t);
        fm.write_port(0x4000, 0x27, t);
        fm.write_port(0x4001, 0x01, t);
        assert_eq!(
            fm.read_status(t + A_PERIOD) & 1,
            0,
            "no ENBL → flag never sets"
        );
        assert_eq!(
            fm.read_status(t + 100 * A_PERIOD) & 1,
            0,
            "still never sets"
        );
    }

    #[test]
    fn unprogrammed_chip_reads_zero_and_bit7_always_clear() {
        let fm = Ym2612::new();
        assert_eq!(fm.read_status(0), 0x00, "power-on status is 0x00");
        assert_eq!(fm.read_status(u64::MAX), 0x00, "still 0x00 far out");
        // bit7 (BUSY) is never set — even a programmed, long-overflowed chip must keep it clear (Gunstar).
        let mut prog = Ym2612::new();
        program_timer_a(&mut prog, 0);
        assert_eq!(
            prog.read_status(100 * A_PERIOD) & 0x80,
            0,
            "bit7 (BUSY) stays clear by construction"
        );
    }

    #[test]
    fn both_windows_are_equivalent() {
        // Programming via the 68k window ($A04000/$A04001) must yield the identical timer state as the Z80
        // window ($4000/$4001) — the ports are normalized on the low bits (fc-agnostic, RT3). The 68k bus
        // passes `a as u16` (truncating $A04000 → $4000), so the chip sees the same low-bit ports either way.
        let t = 500u64;
        let mut z80_win = Ym2612::new();
        program_timer_a(&mut z80_win, t);

        let m68k_addr = 0xA0_4000u32 as u16; // what the 68k bus hands write_port (a as u16) = $4000
        let m68k_data = 0xA0_4001u32 as u16; // = $4001
        let mut m68k_win = Ym2612::new();
        m68k_win.write_port(m68k_addr, 0x24, t);
        m68k_win.write_port(m68k_data, (NA >> 2) as u8, t);
        m68k_win.write_port(m68k_addr, 0x25, t);
        m68k_win.write_port(m68k_data, (NA & 3) as u8, t);
        m68k_win.write_port(m68k_addr, 0x27, t);
        m68k_win.write_port(m68k_data, 0x05, t);

        assert_eq!(
            z80_win, m68k_win,
            "both windows program the same chip state"
        );
        // And both windows read the same status.
        assert_eq!(
            z80_win.read_status(t + A_PERIOD),
            m68k_win.read_status(t + A_PERIOD)
        );
    }

    #[test]
    fn timer_b_sets_after_its_period_and_clears_on_rst() {
        let t = 0u64;
        let nb: u8 = 200;
        let b_period = (256 - nb as u64) * TIMER_B_TICK_MCLK; // (56) × 16128 = 903168
        let mut fm = Ym2612::new();
        // $26 = NB, then $27 = LOAD_B | ENBL_B = 0x02 | 0x08 = 0x0A.
        fm.write_port(0x4000, 0x26, t);
        fm.write_port(0x4001, nb, t);
        fm.write_port(0x4000, 0x27, t);
        fm.write_port(0x4001, 0x0A, t);
        assert_eq!(
            fm.read_status(t + b_period - 1) & 2,
            0,
            "clear before period"
        );
        assert_eq!(
            fm.read_status(t + b_period) & 2,
            2,
            "Timer-B flag (bit1) sets"
        );
        // Timer A untouched.
        assert_eq!(
            fm.read_status(t + b_period) & 1,
            0,
            "Timer-A flag stays clear"
        );
        // Rearm B with RST: $27 = LOAD_B | ENBL_B | RST_B = 0x2A.
        fm.write_port(0x4000, 0x27, t + b_period);
        fm.write_port(0x4001, 0x2A, t + b_period);
        assert_eq!(fm.read_status(t + b_period) & 2, 0, "RST_B clears bit1");
        assert_eq!(
            fm.read_status(t + 2 * b_period) & 2,
            2,
            "re-sets free-running"
        );
    }
}
