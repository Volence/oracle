//! 68000 programmer-visible state — the serializable register file.
//!
//! A7 is the active stack pointer, selected by the SR supervisor bit between `ssp` and `usp` (so it is
//! not stored in `a[]`, which holds A0–A6). `prefetch` models the two-word prefetch queue: `prefetch[0]`
//! is the word at `pc` (the opcode about to execute), `prefetch[1]` the word at `pc + 2`.

/// SR / CCR bit masks (CCR is the low byte of SR).
pub const SR_SUPERVISOR: u16 = 0x2000;
pub const CCR_X: u16 = 0x10;
pub const CCR_N: u16 = 0x08;
pub const CCR_Z: u16 = 0x04;
pub const CCR_V: u16 = 0x02;
pub const CCR_C: u16 = 0x01;

/// The 68000 register file.
#[derive(Clone, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct Registers {
    pub d: [u32; 8],
    /// Address registers A0–A6 (A7 lives in `ssp`/`usp`).
    pub a: [u32; 7],
    pub usp: u32,
    pub ssp: u32,
    pub pc: u32,
    pub sr: u16,
    /// Two-word prefetch queue: `[word @ pc, word @ pc + 2]`.
    pub prefetch: [u16; 2],
}

impl Registers {
    /// True when the supervisor bit is set.
    pub fn supervisor(&self) -> bool {
        self.sr & SR_SUPERVISOR != 0
    }

    /// The active A7 (stack pointer), selected by the supervisor bit.
    pub fn a7(&self) -> u32 {
        if self.supervisor() {
            self.ssp
        } else {
            self.usp
        }
    }

    /// Read address register `i` (0–7); 7 is the active A7.
    pub fn addr_reg(&self, i: usize) -> u32 {
        if i == 7 {
            self.a7()
        } else {
            self.a[i]
        }
    }

    /// Write address register `i` (0–7) — the mirror of [`addr_reg`](Self::addr_reg). Register 7 routes
    /// to the active A7 (`ssp` in supervisor mode, `usp` in user mode), exactly as the read side selects
    /// it; A0–A6 write `a[i]`. Used by [`MicroOp::AdjustAddr`](super::microop::MicroOp::AdjustAddr) for
    /// `(An)+` / `-(An)` register side effects, where A7 must hit the right stack pointer.
    pub fn addr_reg_set(&mut self, i: usize, v: u32) {
        if i == 7 {
            if self.supervisor() {
                self.ssp = v;
            } else {
                self.usp = v;
            }
        } else {
            self.a[i] = v;
        }
    }

    /// The 68000 function code (FC0–FC2) for an access: supervisor/user × data/program.
    /// Supervisor data = 5, supervisor program = 6, user data = 1, user program = 2.
    pub fn fc(&self, program: bool) -> u8 {
        let s = if self.supervisor() { 4 } else { 0 };
        s | if program { 2 } else { 1 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regs() -> Registers {
        Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0x00AA_AAAA,
            ssp: 0x00BB_BBBB,
            pc: 0,
            sr: 0,
            prefetch: [0; 2],
        }
    }

    #[test]
    fn a7_selects_usp_in_user_mode() {
        let mut r = regs();
        r.sr = 0x0000; // S clear
        assert_eq!(r.a7(), 0x00AA_AAAA);
    }

    #[test]
    fn a7_selects_ssp_in_supervisor_mode() {
        let mut r = regs();
        r.sr = SR_SUPERVISOR;
        assert_eq!(r.a7(), 0x00BB_BBBB);
        assert_eq!(r.addr_reg(7), 0x00BB_BBBB);
    }

    #[test]
    fn addr_reg_set_writes_a0_a6_directly() {
        let mut r = regs();
        r.addr_reg_set(3, 0x0012_3456);
        assert_eq!(r.a[3], 0x0012_3456);
        assert_eq!(r.addr_reg(3), 0x0012_3456);
    }

    #[test]
    fn addr_reg_set_routes_a7_to_ssp_in_supervisor_mode() {
        let mut r = regs();
        r.sr = SR_SUPERVISOR;
        r.addr_reg_set(7, 0x00CC_CCCC);
        assert_eq!(r.ssp, 0x00CC_CCCC, "A7 write hit ssp in supervisor mode");
        assert_eq!(r.usp, 0x00AA_AAAA, "usp untouched");
        assert_eq!(r.addr_reg(7), 0x00CC_CCCC);
    }

    #[test]
    fn addr_reg_set_routes_a7_to_usp_in_user_mode() {
        let mut r = regs();
        r.sr = 0x0000; // S clear
        r.addr_reg_set(7, 0x00DD_DDDD);
        assert_eq!(r.usp, 0x00DD_DDDD, "A7 write hit usp in user mode");
        assert_eq!(r.ssp, 0x00BB_BBBB, "ssp untouched");
        assert_eq!(r.addr_reg(7), 0x00DD_DDDD);
    }

    #[test]
    fn function_codes_match_68000_encoding() {
        let mut r = regs();
        r.sr = SR_SUPERVISOR;
        assert_eq!(r.fc(false), 5); // supervisor data
        assert_eq!(r.fc(true), 6); // supervisor program
        r.sr = 0;
        assert_eq!(r.fc(false), 1); // user data
        assert_eq!(r.fc(true), 2); // user program
    }
}
