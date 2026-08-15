//! **The headless replay runner** — boots Aeon's DEBUG ROM, arms its embedded input-replay stream, runs it
//! to completion, and reports PASS / DESYNC / FAULT / TIMEOUT with an exit code.
//!
//! It turns a regression net that is fully built and completely dead into something a CI gate can run.
//! Aeon's own ledger states the gap (`aeon/docs/DEFERRED_WORK.md:113-125`): *"The replay net has NO
//! automated runner — it is invisible to every gate we own … it cannot detect a desync — that needs the
//! emulator."*
//!
//! Design: `docs/2026-08-14-replay-runner-design.md`.
//!
//! # Shape
//!
//! Four pure modules carry every decision, so they are unit-tested without a ROM:
//!
//! - [`header`] — the `ARP0` stream header, parsed out of the ROM image.
//! - [`policy`] — the two refusals (mismatched listing, release ROM).
//! - [`fault`] — decoding a trap from the stack: message text, raise site, pre-clobber registers.
//! - [`outcome`] — the classification precedence and the progress watchdog.
//! - [`restamp`] — the one-pass fixture repair: the static slot map, the recovery stub, and the plan.
//!
//! [`runner`] is the thin impure half that drives `oracle-core`, and [`cli`] is a hand-rolled argument
//! parser. **`oracle-core` is unchanged by this crate**: `boot_with_sink`, `run_until_stop`, `mega_bus`,
//! `ram`, `rom`, `cpu_regs` and `symbols` were all already public, so currency-neutrality is structural.
//!
//! # The three things most likely to produce a wrong answer, and what stops them
//!
//! 1. **A release ROM reports a false green.** `replay.emp:174-186` gates the checkpoint compare on
//!    `DEBUG == 1`; the release shape steps over the payload without comparing, runs the stream to the end,
//!    and sets `Replay_Done = $FF` having verified nothing. [`policy`] refuses it two independent ways.
//! 2. **Every documented address is stale.** The plans' table is off by −4 in the RAM cells, and the
//!    documented `Replay_Done` address *is now `Replay_Ptr`* — a runner that hardcoded it would poll the top
//!    byte of a stream cursor for `$FF` and report a hang on a green run. Everything is resolved by name.
//! 3. **A desync presents exactly as a hang.** [`outcome::dispose`] checks for the trap before it will ever
//!    report a timeout.

pub mod artifacts;
pub mod cli;
pub mod fault;
pub mod header;
pub mod outcome;
pub mod policy;
pub mod restamp;
pub mod runner;

/// Work RAM is a 64 KiB chip mirrored across `$E00000-$FFFFFF`; the mirror index is `addr & (RAM_SIZE - 1)`.
pub use oracle_core::system::RAM_SIZE;

/// Low end of the canonical work-RAM window the listing's `FFFFxxxx` symbols mask into.
pub const WORK_RAM_LO: u32 = 0x00FF_0000;
/// High end of that window.
pub const WORK_RAM_HI: u32 = 0x00FF_FFFF;

/// The bus address a 32-bit 68000 address register actually drives: the chip has 24 address lines, so the
/// top byte is not connected.
///
/// This matters at every stop: A7 in a running Aeon build reads `$FFFFFEF8`, which is `$FFFEF8` on the bus
/// — inside work RAM. A range check applied to the raw register value rejects a perfectly good stack frame,
/// which is exactly how the first `--negative-control` run failed to decode a trap that had fired correctly.
/// It is the same masking `oracle_core::symbols` applies to the listing's `FFFFxxxx` rows.
pub fn bus_addr(addr: u32) -> u32 {
    addr & oracle_core::symbols::BUS_ADDR_MASK
}

/// The bus address of a stack pointer, when it points into work RAM. `None` means there is no frame to
/// read there.
pub fn stack_in_work_ram(a7: u32) -> Option<u32> {
    let a = bus_addr(a7);
    (WORK_RAM_LO..=WORK_RAM_HI).contains(&a).then_some(a)
}

/// Read one work-RAM byte through the 64 KiB mirror.
pub fn ram_u8(ram: &[u8], addr: u32) -> u8 {
    ram[(addr as usize) & (RAM_SIZE - 1)]
}

/// Read one big-endian work-RAM longword through the 64 KiB mirror. Each byte is masked independently, so
/// a longword that straddles the top of the chip wraps exactly as the guest's own read would.
pub fn ram_u32(ram: &[u8], addr: u32) -> u32 {
    let b = |i: u32| ram_u8(ram, addr.wrapping_add(i)) as u32;
    (b(0) << 24) | (b(1) << 16) | (b(2) << 8) | b(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_ram_reads_go_through_the_mirror() {
        let mut ram = vec![0u8; RAM_SIZE];
        ram[0x8004..0x8008].copy_from_slice(&[0x00, 0x00, 0x06, 0xB9]);
        // The listing spells `Logic_Tick` `FFFF8004`; the bus carries `$FF8004`; both index 0x8004.
        assert_eq!(ram_u32(&ram, 0x00FF_8004), 1721);
        assert_eq!(ram_u32(&ram, 0xFFFF_8004), 1721);
        assert_eq!(ram_u8(&ram, 0x00FF_8006), 0x06);
    }

    /// The regression the negative control caught on its very first run: a live A7 is `$FFFFFEF8`, which is
    /// `$FFFEF8` on the 24-line bus. Checking the raw register against the work-RAM window rejects a frame
    /// that is perfectly readable, and turns a correctly-fired trap into "not decodable".
    #[test]
    fn a_stack_pointer_is_masked_to_the_bus_before_it_is_judged() {
        assert_eq!(stack_in_work_ram(0xFFFF_FEF8), Some(0x00FF_FEF8));
        assert_eq!(stack_in_work_ram(0x00FF_FEF8), Some(0x00FF_FEF8));
        // …and something genuinely outside work RAM is still rejected.
        assert_eq!(stack_in_work_ram(0x0000_2000), None);
        assert_eq!(stack_in_work_ram(0xFF00_2000), None);
        assert_eq!(bus_addr(0xFFFF_8036), 0x00FF_8036);
    }

    #[test]
    fn a_longword_at_the_top_of_the_chip_wraps_like_the_guest() {
        let mut ram = vec![0u8; RAM_SIZE];
        ram[RAM_SIZE - 1] = 0xAA;
        ram[0] = 0xBB;
        assert_eq!(ram_u32(&ram, 0x00FF_FFFF), 0xAABB_0000);
    }
}
