//! Decoding a fault from the stack, rather than from a screenshot of the crash screen.
//!
//! # Why the stack
//!
//! `raise_exception` lowers to `jsr (MDDBG__ErrorHandler).l` immediately followed by the inline
//! NUL-terminated message (`sigil/crates/sigil-frontend-emp/src/eval/diag.rs:727-736`). Confirmed
//! byte-for-byte at `$26A2` in `s4.debug.bin`:
//!
//! ```text
//! 22 38 80 04                move.l ($8004).w,d1      ; Logic_Tick -> d1
//! 4e b9 00 0a 21 3a          jsr    $000A213A.l       ; ErrorHandlerBlob + 0
//! 52 45 50 4c 41 59 20 44 45 53 59 4e 43 00           ; "REPLAY DESYNC\0"
//! ```
//!
//! So at blob+0 the `jsr` has pushed the address of the byte after itself, which is the message text:
//! `(A7).l` is a ROM pointer to the string, and `(A7).l - 6` is the `jsr` — the **raise site**, resolvable
//! through the `.lst` to the exact `$module$Proc$label`. Registers are **pre-clobber** at that instant.
//!
//! That is the whole point of stopping at blob+0. The current human procedure reads `d0`/`d1`/`d2` off a
//! *screenshot* of the MD Debugger, because by the time a human can query, the handler is ~3630 bytes in
//! and has clobbered `d0`-`d2` drawing its own screen
//! (`aeon/docs/superpowers/notes/2026-08-13-replay-net-restamp-ab.md:152-157`). This delivers a
//! break-on-fault with a pre-clobber register snapshot and needs **no engine-side mailbox** — the return
//! address *is* the mailbox.
//!
//! # Which faults land here
//!
//! All of them. There are two raise sites on the replay path (`REPLAY DESYNC`, `REPLAY BAD OPCODE`), but
//! 89 others exist across `engine/` + `games/` plus all 12 CPU exception vectors, and several are reachable
//! mid-replay. Every one lands at the same blob entry, so the decoded message — not the stop itself — is
//! what separates a desync from anything else.

use oracle_core::m68000::Registers;
use std::fmt;

/// The desync trap's message text. A fault carrying this is a checkpoint mismatch; anything else is an
/// unrelated fault that happened to occur during the replay.
pub const DESYNC_MESSAGE: &str = "REPLAY DESYNC";

/// Bytes from the `jsr` opcode to the inline message: `4e b9` + a 4-byte absolute address.
pub const JSR_ABS_LONG_LEN: u32 = 6;

/// The pre-clobber 68000 register file at the trap, flattened for reporting (A7 resolved to the active
/// stack pointer, so a reader never has to reason about which of `usp`/`ssp` was live).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegSnapshot {
    pub d: [u32; 8],
    /// A0–A7, where A7 is the *active* stack pointer.
    pub a: [u32; 8],
    pub pc: u32,
    pub sr: u16,
}

impl RegSnapshot {
    /// Flatten a live register file (`System::cpu_regs`).
    pub fn from_regs(r: &Registers) -> Self {
        let mut a = [0u32; 8];
        for (i, slot) in a.iter_mut().enumerate() {
            *slot = r.addr_reg(i);
        }
        Self {
            d: r.d,
            a,
            pc: r.pc,
            sr: r.sr,
        }
    }
}

impl fmt::Display for RegSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in 0..8 {
            writeln!(
                f,
                "    d{i} = ${:08X}    a{i} = ${:08X}",
                self.d[i], self.a[i]
            )?;
        }
        write!(f, "    pc = ${:08X}    sr = ${:04X}", self.pc, self.sr)
    }
}

/// The three registers the desync raise site loads before it traps (`replay.emp:210-218`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesyncDetail {
    /// `d0` — the hash the running game actually produced.
    pub actual: u32,
    /// `d1` — `Logic_Tick` at the mismatching checkpoint.
    pub logic_tick: u32,
    /// `d2` — the hash the fixture expected.
    pub expected: u32,
}

/// A fault decoded at `ErrorHandlerBlob + 0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaultReport {
    /// The inline message text, e.g. `REPLAY DESYNC`.
    pub message: String,
    /// Where that text lives in the ROM — i.e. the value the `jsr` pushed.
    pub message_addr: u32,
    /// The `jsr` itself: `message_addr - 6`.
    pub raise_site: u32,
    /// `raise_site` resolved through the listing, e.g. `$engine.replay$Input_Tick$desync+$0`.
    pub raise_site_symbol: Option<String>,
    /// Pre-clobber registers.
    pub regs: RegSnapshot,
    /// `Some` when [`message`](Self::message) is exactly [`DESYNC_MESSAGE`].
    pub desync: Option<DesyncDetail>,
}

impl FaultReport {
    /// Whether this fault is a checkpoint mismatch rather than some unrelated trap.
    pub fn is_desync(&self) -> bool {
        self.desync.is_some()
    }
}

/// Why a stop at the blob could not be decoded into a fault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaultDecodeError {
    /// `(A7).l` does not point into the ROM image.
    MessageOutOfRom { addr: u32, rom_len: usize },
    /// No NUL within `limit` bytes — `(A7)` is not a message pointer.
    Unterminated { addr: u32, limit: usize },
    /// The bytes are not printable ASCII, so this is not an inline diagnostic string.
    NotAscii { addr: u32, byte: u8 },
    /// A7 does not point into work RAM, so there is no stack frame to read.
    StackNotInWorkRam { a7: u32 },
}

impl fmt::Display for FaultDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MessageOutOfRom { addr, rom_len } => write!(
                f,
                "(A7) = ${addr:08X} is not inside the {rom_len}-byte ROM image, so it is not a \
                 raise-site return address"
            ),
            Self::Unterminated { addr, limit } => write!(
                f,
                "no NUL within {limit} bytes of ${addr:06X} — (A7) does not point at a message"
            ),
            Self::NotAscii { addr, byte } => write!(
                f,
                "byte ${byte:02X} at ${addr:06X} is not printable ASCII — (A7) does not point at a message"
            ),
            Self::StackNotInWorkRam { a7 } => write!(
                f,
                "A7 = ${a7:08X} is not in work RAM ($FF0000-$FFFFFF), so there is no frame to decode"
            ),
        }
    }
}

/// How far to look for the terminating NUL. The longest real message is a couple of dozen bytes; this is a
/// bound, not a budget.
pub const MAX_MESSAGE_LEN: usize = 128;

/// Read a NUL-terminated printable-ASCII string out of the ROM image.
pub fn read_message(rom: &[u8], addr: u32) -> Result<String, FaultDecodeError> {
    let start = addr as usize;
    if start >= rom.len() {
        return Err(FaultDecodeError::MessageOutOfRom {
            addr,
            rom_len: rom.len(),
        });
    }
    let limit = MAX_MESSAGE_LEN.min(rom.len() - start);
    for (i, &b) in rom[start..start + limit].iter().enumerate() {
        if b == 0 {
            return Ok(String::from_utf8_lossy(&rom[start..start + i]).into_owned());
        }
        if !(0x20..0x7F).contains(&b) {
            return Err(FaultDecodeError::NotAscii {
                addr: addr + i as u32,
                byte: b,
            });
        }
    }
    Err(FaultDecodeError::Unterminated { addr, limit })
}

/// The `jsr` that raised the fault, given the message pointer it pushed.
pub fn raise_site(message_addr: u32) -> u32 {
    message_addr.wrapping_sub(JSR_ABS_LONG_LEN)
}

/// Decode a stop at `ErrorHandlerBlob + 0` into a [`FaultReport`].
///
/// `stack_top` is `(A7).l` — the longword the `jsr` pushed, read from work RAM by the caller (this module
/// stays pure: it is handed the bytes, never the machine). `resolve` names the raise site, and is `None`
/// when the listing has nothing at or before it.
pub fn decode(
    rom: &[u8],
    stack_top: u32,
    regs: &RegSnapshot,
    resolve: impl FnOnce(u32) -> Option<String>,
) -> Result<FaultReport, FaultDecodeError> {
    let message = read_message(rom, stack_top)?;
    let site = raise_site(stack_top);
    let desync = (message == DESYNC_MESSAGE).then(|| DesyncDetail {
        actual: regs.d[0],
        logic_tick: regs.d[1],
        expected: regs.d[2],
    });
    Ok(FaultReport {
        message,
        message_addr: stack_top,
        raise_site: site,
        raise_site_symbol: resolve(site),
        regs: *regs,
        desync,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regs(d: [u32; 8]) -> RegSnapshot {
        RegSnapshot {
            d,
            a: [0; 8],
            pc: 0x000A_213A,
            sr: 0x2700,
        }
    }

    /// The real bytes at `$26A2`, transcribed: the `move.l`, the `jsr`, then the inline message.
    fn rom_with_real_raise_site() -> Vec<u8> {
        let mut rom = vec![0u8; 0x4000];
        let bytes: &[u8] = &[
            0x22, 0x38, 0x80, 0x04, // move.l ($8004).w,d1
            0x4E, 0xB9, 0x00, 0x0A, 0x21, 0x3A, // jsr $000A213A.l
            b'R', b'E', b'P', b'L', b'A', b'Y', b' ', b'D', b'E', b'S', b'Y', b'N', b'C', 0x00,
        ];
        rom[0x26A2..0x26A2 + bytes.len()].copy_from_slice(bytes);
        rom
    }

    #[test]
    fn decodes_the_real_desync_frame() {
        let rom = rom_with_real_raise_site();
        // The `jsr` at $26A6 pushes the address of the byte after it: the message at $26AC.
        let r = regs([0x1D37_5066, 2, 0xDEAD_BEEF, 0, 0, 0, 0, 0]);
        let f = decode(&rom, 0x26AC, &r, |a| Some(format!("site@${a:06X}"))).expect("must decode");

        assert_eq!(f.message, DESYNC_MESSAGE);
        assert_eq!(f.message_addr, 0x26AC);
        assert_eq!(f.raise_site, 0x26A6);
        assert_eq!(
            rom[0x26A6], 0x4E,
            "the raise site must land on the jsr opcode"
        );
        assert_eq!(rom[0x26A7], 0xB9);
        assert_eq!(f.raise_site_symbol.as_deref(), Some("site@$0026A6"));
        assert!(f.is_desync());
        assert_eq!(
            f.desync,
            Some(DesyncDetail {
                actual: 0x1D37_5066,
                logic_tick: 2,
                expected: 0xDEAD_BEEF,
            })
        );
    }

    /// A fault that is not a desync must NOT be reported as one — `d0`/`d1`/`d2` mean nothing there.
    #[test]
    fn a_different_message_is_not_a_desync() {
        let mut rom = vec![0u8; 0x100];
        rom[0x40..0x40 + 18].copy_from_slice(b"REPLAY BAD OPCODE\0");
        let f = decode(&rom, 0x40, &regs([1, 2, 3, 0, 0, 0, 0, 0]), |_| None).unwrap();
        assert_eq!(f.message, "REPLAY BAD OPCODE");
        assert!(!f.is_desync());
        assert_eq!(f.desync, None);
    }

    #[test]
    fn refuses_a_stack_top_that_is_not_a_message_pointer() {
        let rom = vec![0u8; 0x100];
        // Points past the image.
        assert!(matches!(
            decode(&rom, 0xFF_0000, &regs([0; 8]), |_| None),
            Err(FaultDecodeError::MessageOutOfRom { .. })
        ));
        // Points at all-zero bytes: that is an immediate NUL, i.e. an empty message — legal but useless…
        assert_eq!(read_message(&rom, 0), Ok(String::new()));
        // …whereas binary garbage is refused rather than lossily rendered.
        let mut rom = vec![0u8; 0x100];
        rom[0x10] = 0x4E;
        rom[0x11] = 0xB9;
        assert!(matches!(
            read_message(&rom, 0x10),
            Err(FaultDecodeError::NotAscii { .. })
        ));
    }

    #[test]
    fn refuses_an_unterminated_message() {
        let rom = vec![b'A'; 0x40];
        assert!(matches!(
            read_message(&rom, 0),
            Err(FaultDecodeError::Unterminated { .. })
        ));
    }

    /// A7 must flatten to the *active* stack pointer, not to a phantom `a[7]`.
    #[test]
    fn a7_flattens_to_the_active_stack_pointer() {
        let mut r = Registers {
            d: [0; 8],
            a: [0x11; 7],
            usp: 0x00FF_E000,
            ssp: 0x00FF_F000,
            pc: 0,
            sr: 0x2700, // supervisor
            prefetch: [0; 2],
        };
        assert_eq!(RegSnapshot::from_regs(&r).a[7], 0x00FF_F000);
        r.sr = 0x0000; // user mode
        assert_eq!(RegSnapshot::from_regs(&r).a[7], 0x00FF_E000);
    }
}
