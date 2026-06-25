//! Phase-0 placeholder "chip".
//!
//! It exists only to (a) prove the [`Bus`] / split-borrow / deferred-write architecture end-to-end
//! through [`System`](crate::system::System), and (b) give `run_frames` deterministic, state-evolving
//! work before the real M68000 lands (Task C). It reads a RAM byte, mixes it into an accumulator, and
//! writes the result to VRAM through the deferred seam. **Delete this module when the M68000 replaces it.**

use crate::bus::{Bus, Size, RAM_BASE, VRAM_BASE};

/// A tiny deterministic state machine that exercises the bus each step.
#[derive(Clone, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct StubCpu {
    pc: u32,
    acc: u32,
}

impl StubCpu {
    /// Power-on state (non-zero accumulator so the first writes are immediately distinguishable).
    pub fn new() -> Self {
        Self {
            pc: 0,
            acc: 0x9E37_79B9,
        }
    }

    /// One step: read a RAM byte, fold it into the accumulator, write the low byte to VRAM (deferred).
    pub fn step(&mut self, bus: &mut impl Bus) {
        let r = bus.read(RAM_BASE + (self.pc & 0xFFFF), Size::Byte);
        self.acc = self.acc.rotate_left(5) ^ r.wrapping_mul(0x0100_0193);
        bus.write(VRAM_BASE + (self.pc & 0xFFFF), Size::Byte, self.acc & 0xFF);
        self.pc = self.pc.wrapping_add(1);
    }
}

impl Default for StubCpu {
    fn default() -> Self {
        Self::new()
    }
}
