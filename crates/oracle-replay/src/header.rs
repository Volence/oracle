//! The `ARP0` replay-stream header, parsed **out of the ROM image** rather than off a `.bin` on disk.
//!
//! The fixture is `embed()`ed into the ROM (`aeon/games/sonic4/test/replay_fixture.emp:16`), so *the ROM is
//! the fixture*: reading the stream from the resolved `Replay_OJZ_Fixture` symbol makes a fixture-vs-ROM
//! mismatch structurally impossible, and it costs nothing.
//!
//! # `core_hash` is not a guard
//!
//! The header carries a `core_hash` whose docstring advertises it as a build-identity check. It has **zero
//! consumers** (`aeon/docs/superpowers/notes/2026-08-05-objtest-gate-ab.md:59-66`) and the committed value
//! is byte-identical in both fixtures despite their having been recorded on different days against
//! different builds. It is stale metadata. This module exposes it for reporting and **never** validates
//! against it — and does not "helpfully" refresh it either (that note's ruling is *"do not hand-refresh a
//! field nothing reads"*).
//!
//! # Layout
//!
//! `REPLAY_HEADER_LEN = 20`, big-endian throughout. Confirmed firsthand at `$A1D80` in `s4.debug.bin`:
//!
//! ```text
//! 41 52 50 30 | 00 | 00 | 00 00 06 b9 | 70 54 d2 8b | 00 00 00 00 | 00 00 | ff 01 1d 37 50 66 | 00 3f
//! "ARP0"      flags pad   ticks=1721    core_hash     seed=0        resv    ring-0 checkpoint    RLE pair
//! ```
//!
//! Body opcodes (`aeon/engine/system/constants.emp:488-492`): `REPLAY_ESCAPE = $FF`, `REPLAY_OP_END = $00`,
//! `REPLAY_OP_CHECK = $01` (+ a big-endian u32 expected hash). An ordinary pair is
//! `buttons u8 (≠ $FF), hold_minus_1 u8`, so a run lasts `hold_minus_1 + 1` ticks. RLE runs are split at
//! every checkpoint and the packer zero-pads after the terminator, so a decoder must tolerate trailing
//! zeros.

use std::fmt;

/// The stream magic, first four bytes of the header.
pub const REPLAY_MAGIC: [u8; 4] = *b"ARP0";
/// Bytes from the fixture symbol to the first stream opcode.
pub const REPLAY_HEADER_LEN: u32 = 20;
/// Escape byte introducing an opcode (`aeon/engine/system/constants.emp:488`).
pub const REPLAY_ESCAPE: u8 = 0xFF;
/// End-of-stream opcode.
pub const REPLAY_OP_END: u8 = 0x00;
/// Checkpoint opcode, followed by a big-endian u32 expected hash.
pub const REPLAY_OP_CHECK: u8 = 0x01;

/// A parsed `ARP0` header, plus the absolute address of the first stream byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayHeader {
    /// Absolute address of the fixture symbol (the `A` of `ARP0`).
    pub base: u32,
    /// Feature flags (offset 4). No flag is defined today; carried for reporting.
    pub flags: u8,
    /// Recorded tick count (offset 6). **Not** a completion test — see the crate docs: `Logic_Tick`
    /// overshoots it, because after end-of-stream the game keeps running on live input.
    pub tick_count: u32,
    /// Offset 10. Stale metadata with zero consumers — reported, never validated against.
    pub core_hash: u32,
    /// Offset 14. `aeon/tools/replay_pack.py:18` documents it as *"reserved, 0 until an RNG exists"*, and
    /// nothing in `replay.emp` reads it. A **non-zero** value is refused rather than reported (see
    /// [`HeaderError::RngSeedNotZero`]).
    pub rng_seed: u32,
    /// Absolute address of the first stream opcode (`base + REPLAY_HEADER_LEN`). This is the value the
    /// runner pokes into `Replay_Ptr`.
    pub body: u32,
}

/// Why a candidate fixture is not a usable `ARP0` stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderError {
    /// The header (or its first two body bytes) runs past the end of the ROM image.
    OutOfRange { base: u32, rom_len: usize },
    /// The first four bytes are not `ARP0`.
    BadMagic([u8; 4]),
    /// `pad` (offset 5) must be zero (`aeon/tools/replay_pack.py:123`).
    PadNotZero(u8),
    /// `reserved` (offset 18) must be zero (`aeon/tools/replay_pack.py:124`).
    ReservedNotZero(u16),
    /// `rng_seed` (offset 14) is non-zero, and this runner cannot honour it.
    RngSeedNotZero(u32),
    /// Every recorded fixture opens on a ring-0 checkpoint, i.e. `FF 01`. Anything else means the symbol
    /// does not point where we think it does.
    BodyNotCheckpoint([u8; 2]),
}

impl fmt::Display for HeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfRange { base, rom_len } => write!(
                f,
                "the fixture at ${base:06X} runs past the end of the {rom_len}-byte ROM image"
            ),
            Self::BadMagic(got) => write!(
                f,
                "bad magic {got:02X?} — expected {:02X?} (\"ARP0\")",
                REPLAY_MAGIC
            ),
            Self::PadNotZero(v) => write!(f, "header pad byte is ${v:02X}, must be $00"),
            Self::ReservedNotZero(v) => {
                write!(f, "header reserved word is ${v:04X}, must be $0000")
            }
            Self::RngSeedNotZero(v) => write!(
                f,
                "header rng_seed is ${v:08X}, but the packer documents the field as \"reserved, 0 until \
                 an RNG exists\" (replay_pack.py:18) and nothing in replay.emp reads it. This runner \
                 pins its own power-on seed, so it cannot honour a recorded one — a non-zero seed would \
                 mean the run is not the run that was recorded, silently. Refusing"
            ),
            Self::BodyNotCheckpoint(got) => write!(
                f,
                "the stream opens with {got:02X?}, not FF 01 (a ring-0 checkpoint) — \
                 is this really the fixture?"
            ),
        }
    }
}

fn be_u16(b: &[u8]) -> u16 {
    u16::from_be_bytes([b[0], b[1]])
}

fn be_u32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

impl ReplayHeader {
    /// Parse the header sitting at `base` in `rom`, validating everything that is cheap to validate:
    /// magic, `pad == 0`, `reserved == 0`, and that the body opens on a checkpoint (`FF 01`).
    pub fn parse(rom: &[u8], base: u32) -> Result<Self, HeaderError> {
        let start = base as usize;
        let need = start + REPLAY_HEADER_LEN as usize + 2;
        if need > rom.len() {
            return Err(HeaderError::OutOfRange {
                base,
                rom_len: rom.len(),
            });
        }
        let h = &rom[start..start + REPLAY_HEADER_LEN as usize + 2];

        let magic: [u8; 4] = [h[0], h[1], h[2], h[3]];
        if magic != REPLAY_MAGIC {
            return Err(HeaderError::BadMagic(magic));
        }
        if h[5] != 0 {
            return Err(HeaderError::PadNotZero(h[5]));
        }
        let reserved = be_u16(&h[18..20]);
        if reserved != 0 {
            return Err(HeaderError::ReservedNotZero(reserved));
        }
        // The third reserved field. `core_hash` is dead metadata we deliberately do not validate, but
        // `rng_seed` is different in kind: it is dead *today*, and the day it stops being dead this runner
        // — which pins `POWER_ON_SEED` — would be running a different machine than the one recorded,
        // without saying so. Refusing a non-zero value keeps that from ever being silent.
        let rng_seed = be_u32(&h[14..18]);
        if rng_seed != 0 {
            return Err(HeaderError::RngSeedNotZero(rng_seed));
        }
        let opener: [u8; 2] = [h[20], h[21]];
        if opener != [REPLAY_ESCAPE, REPLAY_OP_CHECK] {
            return Err(HeaderError::BodyNotCheckpoint(opener));
        }

        Ok(Self {
            base,
            flags: h[4],
            tick_count: be_u32(&h[6..10]),
            core_hash: be_u32(&h[10..14]),
            rng_seed,
            body: base + REPLAY_HEADER_LEN,
        })
    }

    /// Absolute address of the 4-byte expected-hash payload of the **first** checkpoint in the stream, or
    /// `None` if the stream ends (or turns unintelligible) before one appears.
    ///
    /// This is what `--negative-control` corrupts. It walks the stream rather than assuming `body + 2`, so
    /// the walk is exercised by the same code path that the future `--restamp` slice will need.
    pub fn first_checkpoint_payload(&self, rom: &[u8]) -> Option<u32> {
        let mut p = self.body as usize;
        while p + 1 < rom.len() {
            if rom[p] != REPLAY_ESCAPE {
                p += 2; // ordinary `buttons, hold_minus_1` pair
                continue;
            }
            match rom[p + 1] {
                REPLAY_OP_CHECK if p + 6 <= rom.len() => return Some(p as u32 + 2),
                // End of stream, a truncated checkpoint, or an opcode we do not know: no checkpoint here.
                _ => return None,
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic ROM carrying one well-formed fixture at `base`, laid out exactly like the real bytes.
    fn rom_with_fixture(base: usize, ticks: u32, tail: &[u8]) -> Vec<u8> {
        let mut rom = vec![0u8; base + 0x200];
        rom[base..base + 4].copy_from_slice(&REPLAY_MAGIC);
        rom[base + 6..base + 10].copy_from_slice(&ticks.to_be_bytes());
        rom[base + 10..base + 14].copy_from_slice(&0x7054_D28Bu32.to_be_bytes());
        rom[base + 20..base + 20 + tail.len()].copy_from_slice(tail);
        rom
    }

    /// The canonical body opener: a ring-0 checkpoint expecting `$1D375066`, then a 64-tick idle run.
    const OPENER: [u8; 8] = [0xFF, 0x01, 0x1D, 0x37, 0x50, 0x66, 0x00, 0x3F];

    #[test]
    fn parses_the_real_layout() {
        let rom = rom_with_fixture(0x100, 1721, &OPENER);
        let h = ReplayHeader::parse(&rom, 0x100).expect("must parse");
        assert_eq!(h.base, 0x100);
        assert_eq!(h.flags, 0);
        assert_eq!(h.tick_count, 1721);
        assert_eq!(h.core_hash, 0x7054_D28B);
        assert_eq!(h.rng_seed, 0);
        assert_eq!(h.body, 0x100 + 20);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut rom = rom_with_fixture(0x100, 1721, &OPENER);
        rom[0x100] = b'B';
        assert!(matches!(
            ReplayHeader::parse(&rom, 0x100),
            Err(HeaderError::BadMagic(_))
        ));
    }

    #[test]
    fn rejects_a_nonzero_pad_and_reserved() {
        let mut rom = rom_with_fixture(0x100, 1721, &OPENER);
        rom[0x105] = 1;
        assert_eq!(
            ReplayHeader::parse(&rom, 0x100),
            Err(HeaderError::PadNotZero(1))
        );

        let mut rom = rom_with_fixture(0x100, 1721, &OPENER);
        rom[0x113] = 2;
        assert_eq!(
            ReplayHeader::parse(&rom, 0x100),
            Err(HeaderError::ReservedNotZero(2))
        );
    }

    /// The third reserved field, and the one with teeth. `rng_seed` is dead today (`replay_pack.py:18`:
    /// *"reserved, 0 until an RNG exists"*, no reader in `replay.emp`), but this runner pins its own
    /// power-on seed — so the day a stream carries one, running it anyway would silently be a different
    /// run than the one recorded. Refused rather than reported, unlike `core_hash`, which is dead metadata
    /// nothing could ever honour.
    #[test]
    fn a_non_zero_rng_seed_is_refused_because_this_runner_cannot_honour_it() {
        let mut rom = rom_with_fixture(0x100, 1721, &OPENER);
        rom[0x10E..0x112].copy_from_slice(&0x1234_5678u32.to_be_bytes());
        assert_eq!(
            ReplayHeader::parse(&rom, 0x100),
            Err(HeaderError::RngSeedNotZero(0x1234_5678))
        );
        assert!(HeaderError::RngSeedNotZero(1)
            .to_string()
            .contains("cannot honour"));
    }

    /// The load-bearing shape check: a symbol that points somewhere plausible but wrong (or a stream whose
    /// first op is not a checkpoint) must not be accepted, because the runner would then poke a pointer
    /// into the middle of nothing and replay garbage.
    #[test]
    fn rejects_a_body_that_does_not_open_on_a_checkpoint() {
        let rom = rom_with_fixture(0x100, 1721, &[0x00, 0x3F, 0x00, 0x3F]);
        assert_eq!(
            ReplayHeader::parse(&rom, 0x100),
            Err(HeaderError::BodyNotCheckpoint([0x00, 0x3F]))
        );
    }

    #[test]
    fn rejects_a_fixture_that_runs_off_the_end() {
        let rom = vec![0u8; 8];
        assert!(matches!(
            ReplayHeader::parse(&rom, 0),
            Err(HeaderError::OutOfRange { .. })
        ));
    }

    /// The first checkpoint of the real fixtures is at `body + 2`; the walker must agree.
    #[test]
    fn finds_the_opening_checkpoint_payload() {
        let rom = rom_with_fixture(0x100, 1721, &OPENER);
        let h = ReplayHeader::parse(&rom, 0x100).unwrap();
        let at = h.first_checkpoint_payload(&rom).expect("a checkpoint");
        assert_eq!(at, h.body + 2);
        assert_eq!(
            &rom[at as usize..at as usize + 4],
            &[0x1D, 0x37, 0x50, 0x66]
        );
    }

    /// …and the walk is a real walk: with ordinary pairs in front of it, the payload is still found.
    /// (`parse` demands a checkpoint opener, so this exercises the walker directly.)
    #[test]
    fn walks_past_ordinary_pairs_to_reach_a_checkpoint() {
        let rom = rom_with_fixture(
            0x100,
            10,
            &[0x00, 0x3F, 0x04, 0x07, 0xFF, 0x01, 0xDE, 0xAD, 0xBE, 0xEF],
        );
        let h = ReplayHeader {
            base: 0x100,
            flags: 0,
            tick_count: 10,
            core_hash: 0,
            rng_seed: 0,
            body: 0x114,
        };
        let at = h.first_checkpoint_payload(&rom).expect("a checkpoint");
        assert_eq!(at, h.body + 6);
        assert_eq!(
            &rom[at as usize..at as usize + 4],
            &[0xDE, 0xAD, 0xBE, 0xEF]
        );
    }

    /// A stream that terminates before any checkpoint yields `None` rather than a wild address.
    #[test]
    fn reports_no_checkpoint_when_the_stream_ends_first() {
        let rom = rom_with_fixture(0x100, 1, &[0x00, 0x3F, 0xFF, 0x00]);
        let h = ReplayHeader {
            base: 0x100,
            flags: 0,
            tick_count: 1,
            core_hash: 0,
            rng_seed: 0,
            body: 0x114,
        };
        assert_eq!(h.first_checkpoint_payload(&rom), None);
    }
}
