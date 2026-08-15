//! **Re-stamping**: repairing a fixture whose expected hashes have gone stale, in **one** pass.
//!
//! # The problem
//!
//! The replay stream embeds an expected 32-bit state hash at every 64-tick checkpoint. When a *legitimate*
//! engine change alters behaviour, those hashes go stale and the net trips. Re-**recording** is not the
//! repair: it forfeits the fixture's hand-won coverage, and `aeon/tools/test_replay_fixture.py:53-72` exists
//! specifically to catch a re-record masquerading as a re-stamp. The repair is to replace each stale
//! expected hash with the actual one and change **nothing else**.
//!
//! Aeon's ledger (`docs/DEFERRED_WORK.md:113-125`) names this as candidate fix (b), *"a committed re-stamp
//! tool that makes the manual loop cheap enough to run routinely"*. Today it costs ~7 sequential
//! playthroughs — each replays from tick 0, trips at the *first* stale checkpoint, and a human reads
//! `d0`/`d1`/`d2` and patches four bytes before running again.
//!
//! # The mechanism: substitute the match path for the trap
//!
//! `oracle-core` exposes `rom()`, `ram()` and `cpu_regs()` **read-only** — there is no `rom_mut` and no
//! `cpu_regs_mut` — and this crate does not change `oracle-core`. So the live ROM cannot be patched
//! mid-run, and the PC cannot be rewritten to resume past a trap. Neither is needed.
//!
//! The guest's checkpoint compare is (verified byte-for-byte in `s4.debug.bin`):
//!
//! ```text
//! $2626 .fetch:      20 78 80 3C     movea.l (Replay_Ptr).w, a0
//! $262A .fetch_a0:   10 18           move.b  (a0)+, d0
//!   …
//! $263C              24 18           move.l  (a0)+, d2          ; expected hash, out of ROM
//! $263E              21 C8 80 3C     move.l  a0, (Replay_Ptr).w ; cursor advanced PAST the payload
//! $2642              61 00 00 DC     bsr.w   Replay_Hash        ; d0 = the hash actually produced
//! $2646              B4 80           cmp.l   d0, d2
//! $2648              66 58           bne.s   .desync
//! $264A              20 78 80 3C     movea.l (Replay_Ptr).w, a0 ; ── THE MATCH PATH ──
//! $264E              60 DA           bra.s   .fetch_a0          ; ──                ──
//! $26A2 .desync:     22 38 80 04     move.l  Logic_Tick, d1
//! $26A6              4E B9 000A214A  jsr     ErrorHandlerBlob   ; never returns
//! ```
//!
//! Before booting, the ten bytes at `.desync` are overwritten with a **recovery stub** that is the match
//! path, reached absolutely rather than by fall-through:
//!
//! ```text
//! 20 78 80 3C        movea.l (Replay_Ptr).w, a0   ; copied verbatim from rom[.fetch .. .fetch_a0]
//! 4E F9 0000262A     jmp     .fetch_a0            ; absolute — no displacement arithmetic
//! ```
//!
//! Exactly ten bytes — exactly the space the `move.l`/`jsr` pair occupied — and [`build_recovery_stub`]
//! refuses unless those ten bytes really *are* that pair (both opcodes, the `Logic_Tick` operand and the
//! `ErrorHandlerBlob` target all checked against symbols resolved by name).
//!
//! Then `PC == .desync` becomes a stop predicate. Every stale checkpoint now stops the machine **at the
//! stub**, where `d0` is the actual hash, `d2` the expected one, and `Replay_Ptr` already points one
//! longword past the payload — so the payload's ROM address is `Replay_Ptr - 4`, read out of work RAM
//! rather than guessed. The stop is recorded, and the machine runs on: the stub rejoins the fetch loop
//! exactly where the match path would have.
//!
//! ## Why one pass equals seven
//!
//! The instrumented pass is behaviourally the run a **fully re-stamped** image produces. On a re-stamped
//! image the compare succeeds and falls through to `movea.l (Replay_Ptr).w,a0 / bra.s .fetch_a0`; under the
//! stub the branch is taken to the same two effects. `d2` differs (stale hash vs fresh) and is dead — its
//! next writer is the `move.l (a0)+,d2` at the next checkpoint, and `Input_Tick` declares
//! `clobbers(d0-d3/a0-a1)`. `d1` the stub does not touch at all (the code it replaces clobbered it). The
//! cost is a handful of CPU cycles per stale checkpoint, absorbed by the frame's `VSync_Wait` spin.
//!
//! That argument is *discharged*, not merely asserted: the tool re-runs the re-stamped image clean, end to
//! end, and requires a PASS before it will emit anything (see `main.rs`). A timing leak into hashed RAM —
//! the one attack the equivalence argument cannot rule out statically — would surface there.
//!
//! # Trust flows from the static walk, never from the running guest
//!
//! [`StreamMap::walk`] parses the whole stream out of the ROM *before* the machine boots, and is the
//! **authoritative** slot map. `--restamp` may only ever patch an offset that walk identified as checkpoint
//! payload, and every runtime stop must land on one ([`StreamMap::slot_for_payload`]). This inverts trust
//! correctly: the guest's runtime cursor becomes evidence checked against our model rather than the model
//! itself, which is what stops a mis-packed stream from being "repaired" into a green-looking corrupt
//! fixture.
//!
//! # The size invariant
//!
//! A re-stamp changes hash **payloads** only: same offsets, same total length. The fixture sits before the
//! fault-handler island, which must be the last byte-emitting section
//! (`aeon/engine/debug/error_handler.emp:45-72`), so a size change moves `EndOfRom` and requires a sigil
//! repin. [`RestampPlan::apply_to_rom`] and [`RestampPlan::apply_to_fixture`] both assert it.

use crate::header::{ReplayHeader, REPLAY_ESCAPE, REPLAY_OP_CHECK, REPLAY_OP_END};
use std::fmt;

/// Bytes of one expected-hash payload.
pub const PAYLOAD_LEN: u32 = 4;

/// Ticks between checkpoints. The recorder fires one at every `(ring & 63) == 0`
/// (`aeon/tools/test_replay_fixture.py:44-49`), so checkpoint `i` sits at ring `i * 64`.
pub const RING_STRIDE: u32 = 64;

/// The ten bytes the recovery stub occupies at `.desync` — `move.l (xxx).w,d1` (4) + `jsr xxx.l` (6).
pub const STUB_LEN: usize = 10;

// ---------------------------------------------------------------------------------------------------
// The static stream walk — the authoritative slot map
// ---------------------------------------------------------------------------------------------------

/// One checkpoint, located by walking the stream in the ROM image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    /// Position in the stream: 0 for the opening checkpoint.
    pub index: usize,
    /// Ticks consumed before this checkpoint — the *ring index* the recorder stamped it with.
    pub ring: u32,
    /// Absolute address of the 4-byte expected-hash payload in the ROM image.
    pub payload: u32,
    /// The hash currently recorded there.
    pub expected: u32,
}

/// Every checkpoint in a stream, plus what the walk reconciled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamMap {
    pub slots: Vec<Slot>,
    /// Ticks the RLE runs account for. Must equal the header's `tick_count`.
    pub total_ticks: u32,
    /// Address of the `FF 00` terminator.
    pub end: u32,
    /// The fixture symbol's address, so payloads can be expressed as offsets into the committed `.bin`.
    pub base: u32,
}

/// Why a stream is not walkable, or does not reconcile.
///
/// Every one of these is a **refusal** for `--restamp`. A stream we cannot model is one whose payload
/// offsets we cannot vouch for, and re-stamping the wrong four bytes writes garbage into a fixture that
/// then looks green.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamError {
    /// The walk ran past the end of the ROM image without finding a terminator.
    NoTerminator { at: u32 },
    /// An escape byte followed by something that is neither `REPLAY_OP_END` nor `REPLAY_OP_CHECK`.
    UnknownOpcode { at: u32, op: u8 },
    /// A checkpoint record whose 4-byte payload runs off the end of the image.
    TruncatedCheckpoint { at: u32 },
    /// The RLE runs do not add up to the tick count the header declares — the signature of a truncated or
    /// mis-packed stream.
    TickMismatch { walked: u32, declared: u32 },
    /// A checkpoint that is not on the 64-tick ring grid. Means record and playback disagree about what a
    /// ring index is (`test_replay_fixture.py:44-49` pins the same property from the other side).
    RingOffGrid { index: usize, ring: u32 },
    /// A stream with nothing to re-stamp.
    NoCheckpoints,
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTerminator { at } => write!(
                f,
                "the stream runs off the end of the ROM image at ${at:06X} without a REPLAY_OP_END \
                 (FF 00) terminator"
            ),
            Self::UnknownOpcode { at, op } => write!(
                f,
                "unknown escape opcode ${op:02X} at ${at:06X} (expected $00 = END or $01 = CHECK)"
            ),
            Self::TruncatedCheckpoint { at } => write!(
                f,
                "the checkpoint record at ${at:06X} has no room for its 4-byte expected hash"
            ),
            Self::TickMismatch { walked, declared } => write!(
                f,
                "the RLE runs account for {walked} ticks, but the header declares {declared} — this is \
                 a truncated or mis-packed stream, and re-stamping one would write fresh hashes into a \
                 fixture that verifies almost nothing while looking green"
            ),
            Self::RingOffGrid { index, ring } => write!(
                f,
                "checkpoint {index} sits at ring {ring}, not {} — record and playback disagree about \
                 what a ring index means, so the payload offsets cannot be vouched for",
                *index as u32 * RING_STRIDE
            ),
            Self::NoCheckpoints => {
                f.write_str("the stream carries no checkpoints — there is nothing to re-stamp")
            }
        }
    }
}

fn be_u32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

impl StreamMap {
    /// Walk the whole stream from `header.body`, locating every checkpoint payload and reconciling the
    /// tick total against the header.
    ///
    /// This runs **before the machine boots** and is the only thing allowed to say where a payload lives.
    pub fn walk(rom: &[u8], header: &ReplayHeader) -> Result<Self, StreamError> {
        let mut slots: Vec<Slot> = Vec::new();
        let mut p = header.body as usize;
        let mut ring: u32 = 0;
        loop {
            if p + 1 >= rom.len() {
                return Err(StreamError::NoTerminator { at: p as u32 });
            }
            if rom[p] != REPLAY_ESCAPE {
                // An ordinary `buttons, hold_minus_1` pair: the run lasts `hold_minus_1 + 1` ticks.
                ring += u32::from(rom[p + 1]) + 1;
                p += 2;
                continue;
            }
            match rom[p + 1] {
                REPLAY_OP_END => break,
                REPLAY_OP_CHECK => {
                    let payload = p + 2;
                    if payload + PAYLOAD_LEN as usize > rom.len() {
                        return Err(StreamError::TruncatedCheckpoint { at: p as u32 });
                    }
                    let index = slots.len();
                    if !ring.is_multiple_of(RING_STRIDE) || ring / RING_STRIDE != index as u32 {
                        return Err(StreamError::RingOffGrid { index, ring });
                    }
                    slots.push(Slot {
                        index,
                        ring,
                        payload: payload as u32,
                        expected: be_u32(&rom[payload..payload + PAYLOAD_LEN as usize]),
                    });
                    p = payload + PAYLOAD_LEN as usize;
                }
                op => return Err(StreamError::UnknownOpcode { at: p as u32, op }),
            }
        }
        if slots.is_empty() {
            return Err(StreamError::NoCheckpoints);
        }
        if ring != header.tick_count {
            return Err(StreamError::TickMismatch {
                walked: ring,
                declared: header.tick_count,
            });
        }
        Ok(Self {
            slots,
            total_ticks: ring,
            end: p as u32,
            base: header.base,
        })
    }

    /// The checkpoint whose payload starts exactly at `addr`, or `None`. **Exact match only** — a cursor
    /// that lands anywhere else is not a checkpoint we are willing to patch.
    pub fn slot_for_payload(&self, addr: u32) -> Option<&Slot> {
        self.slots.iter().find(|s| s.payload == addr)
    }
}

// ---------------------------------------------------------------------------------------------------
// The recovery stub
// ---------------------------------------------------------------------------------------------------

/// `movea.l (xxx).w, a0` — the opcode word the `.fetch` instruction must carry for us to copy it.
const MOVEA_L_ABS_W_A0: [u8; 2] = [0x20, 0x78];
/// `move.l (xxx).w, d1` — the first half of what we overwrite at `.desync`.
const MOVE_L_ABS_W_D1: [u8; 2] = [0x22, 0x38];
/// `jsr xxx.l` — the second half.
const JSR_ABS_L: [u8; 2] = [0x4E, 0xB9];
/// `jmp xxx.l` — what we write in its place.
const JMP_ABS_L: [u8; 2] = [0x4E, 0xF9];

/// The addresses the stub is built from and verified against. All resolved **by name**; not one is a
/// literal, because the ROM is a regenerated build output and every address in Aeon's own runbook is stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StubAnchors {
    /// `Input_Tick.fetch` — `movea.l (Replay_Ptr).w, a0`, the instruction we copy.
    pub fetch: u32,
    /// `Input_Tick.fetch_a0` — the loop head we jump to, and the end of the copied instruction.
    pub fetch_a0: u32,
    /// `Input_Tick.desync` — where the stub goes, and the stop predicate.
    pub desync: u32,
    /// `Replay_Ptr`, to check the copied instruction really addresses it.
    pub replay_ptr: u32,
    /// `Logic_Tick`, to check the first instruction we overwrite really loads it.
    pub logic_tick: u32,
    /// `ErrorHandlerBlob`, to check the `jsr` we overwrite really targets it.
    pub error_handler: u32,
}

/// A built, verified recovery stub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryStub {
    /// Where it is installed — also the `--restamp` stop predicate.
    pub at: u32,
    pub bytes: [u8; STUB_LEN],
    /// The bytes it displaced, kept so the report can show what was verified.
    pub replaced: [u8; STUB_LEN],
}

/// Sign-extend a 16-bit absolute-short operand the way the 68000 does, then mask to the bus.
fn abs_w_target(word: u16) -> u32 {
    crate::bus_addr(word as i16 as i32 as u32)
}

fn read2(rom: &[u8], at: u32) -> Option<[u8; 2]> {
    let i = at as usize;
    rom.get(i..i + 2).map(|s| [s[0], s[1]])
}

/// Build the recovery stub, refusing unless every byte it copies and every byte it displaces is exactly
/// what the design says it is.
///
/// The shape assertions are the whole safety story for an in-image code patch: they pin the stub to a build
/// whose `.desync` really is `move.l Logic_Tick,d1 / jsr ErrorHandlerBlob` and whose `.fetch` really is
/// `movea.l Replay_Ptr,a0`. Any engine change that moves those instructions makes this refuse loudly rather
/// than write ten bytes over something else.
pub fn build_recovery_stub(rom: &[u8], a: &StubAnchors) -> Result<RecoveryStub, String> {
    // --- what we copy ------------------------------------------------------------------------------
    let copied_len = a.fetch_a0.checked_sub(a.fetch).ok_or_else(|| {
        format!(
            "`Input_Tick.fetch_a0` (${:06X}) is not after `Input_Tick.fetch` (${:06X}) — the listing \
             does not describe the loop this stub rejoins",
            a.fetch_a0, a.fetch
        )
    })?;
    if copied_len != 4 {
        return Err(format!(
            "`Input_Tick.fetch` is {copied_len} bytes long, not 4 — it is supposed to be the single \
             `movea.l (Replay_Ptr).w, a0` the match path repeats. The engine has changed shape; refusing \
             to patch code into it"
        ));
    }
    let fetch_op = read2(rom, a.fetch)
        .ok_or_else(|| format!("`Input_Tick.fetch` (${:06X}) is outside the image", a.fetch))?;
    if fetch_op != MOVEA_L_ABS_W_A0 {
        return Err(format!(
            "`Input_Tick.fetch` at ${:06X} opens {fetch_op:02X?}, not {MOVEA_L_ABS_W_A0:02X?} \
             (`movea.l (xxx).w, a0`) — refusing to copy an instruction that is not the one this stub \
             needs",
            a.fetch
        ));
    }
    let fetch_operand =
        u16::from_be_bytes(read2(rom, a.fetch + 2).expect("in range: checked above"));
    if abs_w_target(fetch_operand) != a.replay_ptr {
        return Err(format!(
            "`Input_Tick.fetch` loads from ${:06X}, but `Replay_Ptr` resolves to ${:06X} — the \
             instruction this stub copies does not reload the stream cursor",
            abs_w_target(fetch_operand),
            a.replay_ptr
        ));
    }

    // --- what we displace --------------------------------------------------------------------------
    let i = a.desync as usize;
    let old = rom
        .get(i..i + STUB_LEN)
        .ok_or_else(|| {
            format!(
                "`Input_Tick.desync` (${:06X}) leaves fewer than {STUB_LEN} bytes in the image",
                a.desync
            )
        })?
        .to_vec();
    if old[0..2] != MOVE_L_ABS_W_D1 {
        return Err(format!(
            "`Input_Tick.desync` at ${:06X} opens {:02X?}, not {MOVE_L_ABS_W_D1:02X?} \
             (`move.l (xxx).w, d1`). The ten bytes this stub overwrites are not the raise site it was \
             written against; refusing",
            a.desync,
            &old[0..2]
        ));
    }
    let d1_operand = u16::from_be_bytes([old[2], old[3]]);
    if abs_w_target(d1_operand) != a.logic_tick {
        return Err(format!(
            "`Input_Tick.desync` loads d1 from ${:06X}, but `Logic_Tick` resolves to ${:06X} — this is \
             not the raise site's `move.l Logic_Tick, d1`",
            abs_w_target(d1_operand),
            a.logic_tick
        ));
    }
    if old[4..6] != JSR_ABS_L {
        return Err(format!(
            "`Input_Tick.desync` + 4 is {:02X?}, not {JSR_ABS_L:02X?} (`jsr xxx.l`) — the raise site is \
             not the two-instruction shape this stub replaces",
            &old[4..6]
        ));
    }
    let jsr_target = crate::bus_addr(be_u32(&old[6..10]));
    if jsr_target != a.error_handler {
        return Err(format!(
            "`Input_Tick.desync`'s jsr targets ${jsr_target:06X}, but `ErrorHandlerBlob` resolves to \
             ${:06X} — refusing to overwrite a call to something else",
            a.error_handler
        ));
    }

    // --- assemble ----------------------------------------------------------------------------------
    if a.fetch_a0 & 0xFF00_0000 != 0 {
        return Err(format!(
            "`Input_Tick.fetch_a0` (${:08X}) does not fit a 24-bit absolute-long target",
            a.fetch_a0
        ));
    }
    let mut bytes = [0u8; STUB_LEN];
    bytes[0..4].copy_from_slice(&rom[a.fetch as usize..a.fetch as usize + 4]);
    bytes[4..6].copy_from_slice(&JMP_ABS_L);
    bytes[6..10].copy_from_slice(&a.fetch_a0.to_be_bytes());
    let mut replaced = [0u8; STUB_LEN];
    replaced.copy_from_slice(&old);
    Ok(RecoveryStub {
        at: a.desync,
        bytes,
        replaced,
    })
}

impl RecoveryStub {
    /// Install into a ROM image. Length is invariant by construction — a fixed-size overwrite.
    pub fn install(&self, rom: &mut [u8]) -> Result<(), String> {
        let i = self.at as usize;
        rom.get_mut(i..i + STUB_LEN)
            .ok_or_else(|| format!("the stub does not fit at ${:06X}", self.at))?
            .copy_from_slice(&self.bytes);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------------------------------

/// One checkpoint the pass found stale, and the four bytes that repair it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaleCheckpoint {
    pub index: usize,
    pub ring: u32,
    /// `Logic_Tick` at the stop — the number the human procedure used to read off a debugger screenshot.
    pub logic_tick: u32,
    /// Absolute address of the payload in the ROM image.
    pub payload: u32,
    /// The same payload as an offset into the committed fixture `.bin`, which is the artifact of record.
    pub fixture_offset: u32,
    pub expected: u32,
    pub actual: u32,
}

/// Everything one `--restamp` pass found, and the repair it implies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestampPlan {
    /// Stale checkpoints in the order the pass met them (which is stream order).
    pub stale: Vec<StaleCheckpoint>,
    /// How many checkpoints the stream has in total, for "3 of 27".
    pub total_checkpoints: usize,
    pub fixture_base: u32,
    /// The image length every artifact must preserve.
    pub rom_len: usize,
}

impl RestampPlan {
    /// Nothing to repair — the fixture is already current.
    pub fn is_clean(&self) -> bool {
        self.stale.is_empty()
    }

    /// Apply to a full ROM image.
    ///
    /// **Hard-refuses any length change.** A re-stamp changes hash payloads only: same offsets, same total
    /// length. The fixture sits before the fault-handler island, which must be the last byte-emitting
    /// section, so a size change moves `EndOfRom` and requires a sigil repin.
    pub fn apply_to_rom(&self, rom: &mut [u8]) -> Result<(), String> {
        if rom.len() != self.rom_len {
            return Err(format!(
                "this plan was computed against a {}-byte image but was handed a {}-byte one — \
                 refusing, because the offsets would not mean the same thing",
                self.rom_len,
                rom.len()
            ));
        }
        for s in &self.stale {
            patch4(rom, s.payload, s.expected, s.actual, "ROM")?;
        }
        Ok(())
    }

    /// Apply to the committed fixture `.bin` — the durable artifact. `blob` must be byte-identical to the
    /// slice of the ROM the fixture symbol names; the caller proves that with
    /// [`verify_fixture_embedding`].
    pub fn apply_to_fixture(&self, blob: &mut [u8]) -> Result<(), String> {
        for s in &self.stale {
            patch4(blob, s.fixture_offset, s.expected, s.actual, "fixture")?;
        }
        Ok(())
    }
}

/// Replace exactly four bytes, refusing unless what is there is the value we recorded as stale. The
/// old-value check is what makes a plan safe to apply to a *different copy* of the same image, and what
/// stops it being applied to a different build.
fn patch4(buf: &mut [u8], at: u32, expect_old: u32, new: u32, what: &str) -> Result<(), String> {
    let i = at as usize;
    let len = buf.len();
    let slot = buf
        .get_mut(i..i + PAYLOAD_LEN as usize)
        .ok_or_else(|| format!("payload at ${at:06X} is outside the {what} image ({len} bytes)"))?;
    let found = be_u32(slot);
    if found != expect_old {
        return Err(format!(
            "the {what} bytes at ${at:06X} hold ${found:08X}, but this plan expects the stale \
             ${expect_old:08X} there — the image is not the one the plan was computed from. Refusing"
        ));
    }
    slot.copy_from_slice(&new.to_be_bytes());
    Ok(())
}

/// Prove the committed fixture `.bin` is byte-identical to the copy `embed()`ed in the ROM, so its file
/// offsets and the ROM's agree.
pub fn verify_fixture_embedding(rom: &[u8], base: u32, blob: &[u8]) -> Result<(), String> {
    let i = base as usize;
    let slice = rom.get(i..i + blob.len()).ok_or_else(|| {
        format!(
            "the fixture at ${base:06X} + {} runs off the ROM",
            blob.len()
        )
    })?;
    if slice != blob {
        let at = slice
            .iter()
            .zip(blob)
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        return Err(format!(
            "the fixture file does not match the copy embedded in the ROM — first difference at file \
             offset {at} (ROM ${:02X} vs file ${:02X}). This is a different build than the one that \
             produced the ROM, and its offsets cannot be trusted",
            slice[at], blob[at]
        ));
    }
    Ok(())
}

/// Provenance for the patch artifact's header block.
#[derive(Debug, Clone, Copy)]
pub struct PatchMeta<'a> {
    pub rom_path: &'a str,
    pub lst_path: &'a str,
    pub fixture_symbol: &'a str,
    pub fixture_name: &'a str,
    pub total_ticks: u32,
}

/// Render the patch artifact: file offsets and replacement bytes, for deliberate review and application.
pub fn render_patch(plan: &RestampPlan, meta: &PatchMeta<'_>) -> String {
    use std::fmt::Write as _;
    let mut o = String::new();
    let _ = writeln!(o, "# replay-restamp patch");
    let _ = writeln!(o, "#");
    let _ = writeln!(
        o,
        "# rom          {} ({} bytes)",
        meta.rom_path, plan.rom_len
    );
    let _ = writeln!(o, "# lst          {}", meta.lst_path);
    let _ = writeln!(
        o,
        "# fixture      {} ({}) embedded at ${:06X}",
        meta.fixture_symbol, meta.fixture_name, plan.fixture_base
    );
    let _ = writeln!(
        o,
        "# stream       {} ticks, {} checkpoints, walked and reconciled against the header",
        meta.total_ticks, plan.total_checkpoints
    );
    let _ = writeln!(
        o,
        "# stale        {} of {}",
        plan.stale.len(),
        plan.total_checkpoints
    );
    let _ = writeln!(o, "#");
    let _ = writeln!(
        o,
        "# Every row replaces exactly 4 bytes; nothing else changes and the total length is invariant."
    );
    let _ = writeln!(
        o,
        "# `fix_off` is the offset in the committed fixture .bin, which is the artifact of record --"
    );
    let _ = writeln!(
        o,
        "# the ROM is a regenerated build output. `rom_off` is the same payload in this image."
    );
    let _ = writeln!(o, "#");
    let _ = writeln!(
        o,
        "#  idx   ring   tick   rom_off   fix_off   old       new"
    );
    for s in &plan.stale {
        let _ = writeln!(
            o,
            "  {:4}  {:5}  {:5}   {:06X}    {:06X}    {:08X}  {:08X}",
            s.index, s.ring, s.logic_tick, s.payload, s.fixture_offset, s.expected, s.actual
        );
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic ROM carrying a well-formed stream at `base`.
    fn rom_with_stream(base: usize, body: &[u8], ticks: u32) -> (Vec<u8>, ReplayHeader) {
        let mut rom = vec![0u8; base + 0x200];
        rom[base..base + 4].copy_from_slice(b"ARP0");
        rom[base + 6..base + 10].copy_from_slice(&ticks.to_be_bytes());
        rom[base + 20..base + 20 + body.len()].copy_from_slice(body);
        let h = ReplayHeader::parse(&rom, base as u32).expect("a usable header");
        (rom, h)
    }

    /// Ring 0 checkpoint, a 64-tick run, ring 64 checkpoint, a 64-tick run, terminator, packer padding.
    const TWO: [u8; 20] = [
        0xFF, 0x01, 0x11, 0x11, 0x11, 0x11, // checkpoint 0 @ ring 0
        0x00, 0x3F, // 64 ticks
        0xFF, 0x01, 0x22, 0x22, 0x22, 0x22, // checkpoint 1 @ ring 64
        0x04, 0x3F, // 64 ticks
        0xFF, 0x00, // end
        0x00, 0x00, // packer padding
    ];

    #[test]
    fn the_walk_locates_every_payload_and_reconciles_the_tick_total() {
        let (rom, h) = rom_with_stream(0x100, &TWO, 128);
        let m = StreamMap::walk(&rom, &h).expect("must walk");
        assert_eq!(m.total_ticks, 128);
        assert_eq!(m.slots.len(), 2);
        assert_eq!(
            m.slots[0],
            Slot {
                index: 0,
                ring: 0,
                payload: 0x116,
                expected: 0x1111_1111
            }
        );
        assert_eq!(
            m.slots[1],
            Slot {
                index: 1,
                ring: 64,
                payload: 0x11E,
                expected: 0x2222_2222
            }
        );
        // Exact-match only: one byte either side is not a checkpoint.
        assert!(m.slot_for_payload(0x116).is_some());
        assert!(m.slot_for_payload(0x117).is_none());
    }

    /// **The refusal that matters most.** A truncated stream — the exact shape the runner's SHORT
    /// classification exists for — must never be re-stamped: fresh hashes written into it would produce a
    /// fixture that is green and verifies almost nothing.
    #[test]
    fn a_stream_whose_ticks_do_not_reconcile_is_refused() {
        let (rom, h) = rom_with_stream(
            0x100,
            &[0xFF, 0x01, 0x11, 0x11, 0x11, 0x11, 0xFF, 0x00],
            1721,
        );
        let e = StreamMap::walk(&rom, &h).expect_err("must refuse");
        assert_eq!(
            e,
            StreamError::TickMismatch {
                walked: 0,
                declared: 1721
            }
        );
        assert!(e.to_string().contains("truncated or mis-packed"), "{e}");
    }

    #[test]
    fn an_unknown_opcode_is_refused_rather_than_skipped() {
        let (rom, h) = rom_with_stream(
            0x100,
            &[0xFF, 0x01, 0x11, 0x11, 0x11, 0x11, 0xFF, 0x7E, 0xFF, 0x00],
            0,
        );
        assert_eq!(
            StreamMap::walk(&rom, &h),
            Err(StreamError::UnknownOpcode {
                at: 0x11A,
                op: 0x7E
            })
        );
    }

    /// The recorder fires a checkpoint at every `(ring & 63) == 0`, so checkpoint `i` is at ring `i * 64`.
    /// A stream that drifts off that grid is one whose payload offsets we cannot vouch for.
    #[test]
    fn a_checkpoint_off_the_ring_grid_is_refused() {
        let body = [
            0xFF, 0x01, 0x11, 0x11, 0x11, 0x11, // ring 0
            0x00, 0x0F, // 16 ticks
            0xFF, 0x01, 0x22, 0x22, 0x22, 0x22, // ring 16 — not on the grid
            0xFF, 0x00,
        ];
        let (rom, h) = rom_with_stream(0x100, &body, 16);
        assert_eq!(
            StreamMap::walk(&rom, &h),
            Err(StreamError::RingOffGrid { index: 1, ring: 16 })
        );
    }

    #[test]
    fn a_stream_with_no_terminator_is_refused() {
        let mut rom = vec![0u8; 0x140];
        rom[0x100..0x104].copy_from_slice(b"ARP0");
        rom[0x114..0x116].copy_from_slice(&[0xFF, 0x01]);
        let h = ReplayHeader::parse(&rom, 0x100).unwrap();
        assert!(matches!(
            StreamMap::walk(&rom, &h),
            Err(StreamError::NoTerminator { .. })
        ));
    }

    // --- the stub ----------------------------------------------------------------------------------

    /// A ROM whose `.fetch` and `.desync` carry exactly the real shapes.
    fn rom_with_shapes() -> (Vec<u8>, StubAnchors) {
        let mut rom = vec![0u8; 0x1000];
        // $200 .fetch:  movea.l ($803C).w, a0
        rom[0x200..0x204].copy_from_slice(&[0x20, 0x78, 0x80, 0x3C]);
        // $300 .desync: move.l ($8004).w, d1 ; jsr $000A214A.l
        rom[0x300..0x30A]
            .copy_from_slice(&[0x22, 0x38, 0x80, 0x04, 0x4E, 0xB9, 0x00, 0x0A, 0x21, 0x4A]);
        let a = StubAnchors {
            fetch: 0x200,
            fetch_a0: 0x204,
            desync: 0x300,
            replay_ptr: 0x00FF_803C,
            logic_tick: 0x00FF_8004,
            error_handler: 0x0A_214A,
        };
        (rom, a)
    }

    #[test]
    fn the_stub_is_the_match_path_reached_absolutely() {
        let (rom, a) = rom_with_shapes();
        let s = build_recovery_stub(&rom, &a).expect("must build");
        assert_eq!(s.at, 0x300);
        assert_eq!(
            s.bytes,
            [0x20, 0x78, 0x80, 0x3C, 0x4E, 0xF9, 0x00, 0x00, 0x02, 0x04]
        );
        // …and it is exactly as long as what it displaces, so no byte outside `.desync` moves.
        let mut patched = rom.clone();
        s.install(&mut patched).unwrap();
        assert_eq!(patched.len(), rom.len());
        assert_eq!(&patched[0x30A..0x320], &rom[0x30A..0x320]);
        assert_eq!(&patched[0x300..0x30A], &s.bytes);
    }

    /// The shape assertions are the whole safety story for writing code into a ROM. Each one, alone.
    #[test]
    fn every_shape_assertion_refuses_on_its_own() {
        // The displaced `move.l` is not a `move.l`.
        let (mut rom, a) = rom_with_shapes();
        rom[0x300] = 0x4E;
        assert!(build_recovery_stub(&rom, &a)
            .unwrap_err()
            .contains("move.l (xxx).w, d1"));

        // …it loads the wrong cell.
        let (mut rom, a) = rom_with_shapes();
        rom[0x302..0x304].copy_from_slice(&[0x80, 0x40]);
        assert!(build_recovery_stub(&rom, &a)
            .unwrap_err()
            .contains("Logic_Tick"));

        // …the displaced call is not a `jsr`.
        let (mut rom, a) = rom_with_shapes();
        rom[0x304] = 0x60;
        assert!(build_recovery_stub(&rom, &a).unwrap_err().contains("jsr"));

        // …the `jsr` goes somewhere other than the blob.
        let (mut rom, a) = rom_with_shapes();
        rom[0x306..0x30A].copy_from_slice(&[0x00, 0x0A, 0x99, 0x99]);
        assert!(build_recovery_stub(&rom, &a)
            .unwrap_err()
            .contains("ErrorHandlerBlob"));

        // …the copied instruction is not a `movea.l`.
        let (mut rom, a) = rom_with_shapes();
        rom[0x200] = 0x30;
        assert!(build_recovery_stub(&rom, &a)
            .unwrap_err()
            .contains("movea.l"));

        // …it reloads something other than the cursor.
        let (mut rom, a) = rom_with_shapes();
        rom[0x202..0x204].copy_from_slice(&[0x80, 0x36]);
        assert!(build_recovery_stub(&rom, &a)
            .unwrap_err()
            .contains("Replay_Ptr"));

        // …and `.fetch` is not one 4-byte instruction.
        let (rom, mut a) = rom_with_shapes();
        a.fetch_a0 = 0x206;
        assert!(build_recovery_stub(&rom, &a).unwrap_err().contains("not 4"));
    }

    // --- the plan ----------------------------------------------------------------------------------

    fn plan() -> RestampPlan {
        RestampPlan {
            stale: vec![
                StaleCheckpoint {
                    index: 0,
                    ring: 0,
                    logic_tick: 2,
                    payload: 0x116,
                    fixture_offset: 0x16,
                    expected: 0x1111_1111,
                    actual: 0xAAAA_AAAA,
                },
                StaleCheckpoint {
                    index: 1,
                    ring: 64,
                    logic_tick: 66,
                    payload: 0x11E,
                    fixture_offset: 0x1E,
                    expected: 0x2222_2222,
                    actual: 0xBBBB_BBBB,
                },
            ],
            total_checkpoints: 2,
            fixture_base: 0x100,
            rom_len: 0x300,
        }
    }

    #[test]
    fn applying_a_plan_changes_only_the_payloads_and_never_the_length() {
        let (mut rom, _) = rom_with_stream(0x100, &TWO, 128);
        rom.truncate(0x300);
        let before = rom.clone();
        plan().apply_to_rom(&mut rom).expect("must apply");
        assert_eq!(rom.len(), before.len());
        assert_eq!(&rom[0x116..0x11A], &0xAAAA_AAAAu32.to_be_bytes());
        assert_eq!(&rom[0x11E..0x122], &0xBBBB_BBBBu32.to_be_bytes());
        // Every other byte is untouched.
        for (i, (a, b)) in rom.iter().zip(&before).enumerate() {
            let inside = (0x116..0x11A).contains(&i) || (0x11E..0x122).contains(&i);
            assert!(inside || a == b, "byte {i:#X} moved and should not have");
        }
    }

    /// **The length refusal.** A plan may only ever be applied to the image it was computed from.
    #[test]
    fn a_plan_refuses_an_image_of_a_different_length() {
        let mut rom = vec![0u8; 0x301];
        let e = plan().apply_to_rom(&mut rom).expect_err("must refuse");
        assert!(e.contains("769"), "{e}");
        assert!(e.contains("refusing"), "{e}");
    }

    /// A plan carries the stale value it saw, so applying it to a *different* build's image — where those
    /// four bytes hold something else — refuses instead of silently overwriting.
    #[test]
    fn a_plan_refuses_an_image_whose_payload_is_not_the_stale_one_it_recorded() {
        let (mut rom, _) = rom_with_stream(0x100, &TWO, 128);
        rom.truncate(0x300);
        rom[0x116..0x11A].copy_from_slice(&0xDEAD_BEEFu32.to_be_bytes());
        let e = plan().apply_to_rom(&mut rom).expect_err("must refuse");
        assert!(e.contains("DEADBEEF"), "{e}");
    }

    #[test]
    fn the_fixture_blob_is_patched_at_its_own_offsets() {
        let (rom, _) = rom_with_stream(0x100, &TWO, 128);
        let mut blob = rom[0x100..0x100 + 0x30].to_vec();
        verify_fixture_embedding(&rom, 0x100, &blob).expect("embedded verbatim");
        plan().apply_to_fixture(&mut blob).expect("must apply");
        assert_eq!(blob.len(), 0x30);
        assert_eq!(&blob[0x16..0x1A], &0xAAAA_AAAAu32.to_be_bytes());
        assert_eq!(&blob[0x1E..0x22], &0xBBBB_BBBBu32.to_be_bytes());
    }

    /// A fixture file from a different build must not be patched at offsets computed from this one.
    #[test]
    fn a_fixture_that_is_not_the_embedded_one_is_refused() {
        let (rom, _) = rom_with_stream(0x100, &TWO, 128);
        let mut blob = rom[0x100..0x130].to_vec();
        blob[0x18] ^= 0xFF;
        let e = verify_fixture_embedding(&rom, 0x100, &blob).expect_err("must refuse");
        assert!(e.contains("offset 24"), "{e}");
    }

    #[test]
    fn the_patch_artifact_carries_both_offsets_and_both_values() {
        let text = render_patch(
            &plan(),
            &PatchMeta {
                rom_path: "s4.debug.bin",
                lst_path: "s4.debug.lst",
                fixture_symbol: "Replay_OJZ_Fixture",
                fixture_name: "ojz_fixture",
                total_ticks: 128,
            },
        );
        assert!(text.contains("stale        2 of 2"), "{text}");
        assert!(
            text.contains("000116    000016    11111111  AAAAAAAA"),
            "{text}"
        );
        assert!(
            text.contains("00011E    00001E    22222222  BBBBBBBB"),
            "{text}"
        );
    }
}
