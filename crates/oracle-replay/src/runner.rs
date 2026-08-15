//! Booting the machine, arming the stream, and running it to a verdict.
//!
//! Everything here is the impure half; the decisions it makes live in [`crate::header`],
//! [`crate::policy`], [`crate::fault`] and [`crate::outcome`], which are pure and unit-tested without a ROM.

use crate::cli::Fixture;
use crate::fault::{self, FaultDecodeError, FaultReport, RegSnapshot};
use crate::header::{HeaderError, ReplayHeader, REPLAY_HEADER_LEN};
use crate::outcome::{dispose, Disposition, Expected, Probe, Shortfall, Watchdog};
use crate::policy::{self, LstVerdict};
use crate::{ram_u32, ram_u8, stack_in_work_ram};
use oracle_core::bus::{Fanout, StopWhen};
use oracle_core::io::Pad;
use oracle_core::m68000::bus68k::Bus68k;
use oracle_core::symbols::SymbolTable;
use oracle_core::system::System;

/// Function code for a supervisor **data** access — what the guest's own `move` to a RAM cell carries, and
/// what the poke should therefore look like on the bus.
const FC_SUPERVISOR_DATA: u8 = 5;

/// The power-on RNG seed. Fixed, because the whole value of this runner over a human at a debugger is that
/// two invocations are the same run: `boot_with_sink`'s seed is the only non-ROM input to the machine, so
/// pinning it here is what makes the verdict reproducible.
pub const POWER_ON_SEED: u64 = 0x51;

/// The value `--negative-control` writes over a checkpoint payload.
pub const NEGATIVE_CONTROL_PAYLOAD: u32 = 0xDEAD_BEEF;

/// Every address the runner needs, resolved **by name**. Not one of them is hardcoded: measured against the
/// current build, every address in the plans is stale, and one of them — the documented `Replay_Done` —
/// is now `Replay_Ptr`. A runner that hardcoded the documented values would poll the top byte of a stream
/// cursor for `$FF`, never see it, and report a hang on a green run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchors {
    /// The game's `Game.entry`, and the arm point.
    pub init: u32,
    /// The selected fixture's base address in ROM.
    pub fixture: u32,
    pub replay_ptr: u32,
    pub input_source: u32,
    pub replay_done: u32,
    pub logic_tick: u32,
    /// The single stop predicate, matched with **exact** equality.
    pub error_handler: u32,
    /// Optional: reported on a timeout when the listing declares it.
    pub replay_hold: Option<u32>,
}

/// Names that must resolve. Anything unresolved (or ambiguous — `address_of` returns `None` for a spelling
/// that names more than one address) is fatal, and the message names the symbol.
const REQUIRED: [&str; 6] = [
    "GameState_OJZScroll_Init",
    "Replay_Ptr",
    "Input_Source",
    "Replay_Done",
    "Logic_Tick",
    "ErrorHandlerBlob",
];

impl Anchors {
    /// Resolve every anchor from `table`, failing loudly on the first name that does not resolve.
    pub fn resolve(table: &SymbolTable, fixture: Fixture) -> Result<Self, String> {
        let need = |name: &str| -> Result<u32, String> {
            table.address_of(name).ok_or_else(|| {
                format!(
                    "the listing does not resolve `{name}` to a single address (missing, or the \
                     spelling is ambiguous). Every address this runner uses is resolved by name — the \
                     documented literals are all stale — so this is fatal."
                )
            })
        };
        for name in REQUIRED {
            need(name)?;
        }
        Ok(Self {
            init: need("GameState_OJZScroll_Init")?,
            fixture: need(fixture.symbol())?,
            replay_ptr: need("Replay_Ptr")?,
            input_source: need("Input_Source")?,
            replay_done: need("Replay_Done")?,
            logic_tick: need("Logic_Tick")?,
            error_handler: need("ErrorHandlerBlob")?,
            replay_hold: table.address_of("Replay_Hold"),
        })
    }
}

/// A ROM + listing that have passed both refusals, with every anchor resolved and the header parsed.
pub struct Prepared {
    pub rom: Vec<u8>,
    pub table: SymbolTable,
    pub anchors: Anchors,
    pub header: ReplayHeader,
    pub fixture: Fixture,
    /// One-line notes worth printing (the listing's binding verdict, any integrity warning).
    pub notes: Vec<String>,
}

impl Prepared {
    /// Apply both refusals, resolve the anchors, and parse the header out of the ROM image.
    ///
    /// The stream is read from the resolved fixture symbol *in the image*, never from a `.bin` on disk, so
    /// a fixture-vs-ROM mismatch is structurally impossible.
    pub fn new(rom: Vec<u8>, lst_text: &str, fixture: Fixture) -> Result<Self, String> {
        let mut notes = Vec::new();

        // Refusal 2 first: it is decisive where the shape binding is only suggestive, and it needs no
        // listing at all, so a release ROM is named as a release ROM rather than as a symbol problem.
        policy::require_debug_rom(&rom)?;
        notes
            .push("rom: contains `REPLAY DESYNC` — the DEBUG checkpoint compare is present".into());

        let table = SymbolTable::parse(lst_text)
            .map_err(|e| format!("the listing is not usable ({e}) — cannot resolve any address"))?;

        // Refusal 1: the shape binding.
        match policy::judge_listing(&table, &rom) {
            LstVerdict::Refuse { reason } => return Err(format!("listing REFUSED — {reason}")),
            LstVerdict::Accept { note } => notes.push(format!("lst: {note}")),
            LstVerdict::AcceptUnverified { note } => notes.push(format!("lst: WARNING {note}")),
        }
        if !table.is_intact() {
            notes.push(format!(
                "lst: WARNING the listing does not look intact ({}); addresses will resolve to \
                 coarser names",
                policy::integrity_note(&table)
            ));
        }

        let anchors = Anchors::resolve(&table, fixture)?;
        let header = ReplayHeader::parse(&rom, anchors.fixture).map_err(|e: HeaderError| {
            format!("`{}` is not a usable stream: {e}", fixture.symbol())
        })?;

        Ok(Self {
            rom,
            table,
            anchors,
            header,
            fixture,
            notes,
        })
    }

    /// Overwrite the first checkpoint's expected hash with [`NEGATIVE_CONTROL_PAYLOAD`], returning
    /// `(address, original value)`. This is Slice 1n: *a gate you have never seen fail is not a gate.*
    pub fn corrupt_first_checkpoint(&mut self) -> Result<(u32, u32), String> {
        let at = self.header.first_checkpoint_payload(&self.rom).ok_or(
            "the stream carries no checkpoint to corrupt — the negative control cannot run",
        )?;
        let i = at as usize;
        let original = u32::from_be_bytes([
            self.rom[i],
            self.rom[i + 1],
            self.rom[i + 2],
            self.rom[i + 3],
        ]);
        // A payload that already *is* the sentinel means the patch changes nothing: the trap that follows
        // would be the genuine recorded hash mismatching, not our corruption, and the whole control would
        // be a tautology dressed as evidence.
        if original == NEGATIVE_CONTROL_PAYLOAD {
            return Err(format!(
                "the first checkpoint payload at ${at:06X} is ALREADY ${NEGATIVE_CONTROL_PAYLOAD:08X}, \
                 so patching it would be a no-op and the negative control would prove nothing about \
                 whether the corruption caused the trap. Refusing"
            ));
        }
        self.rom[i..i + 4].copy_from_slice(&NEGATIVE_CONTROL_PAYLOAD.to_be_bytes());
        Ok((at, original))
    }
}

/// Configuration for one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunConfig {
    pub max_frames: u64,
    pub stall_frames: u64,
}

/// Which phase a timeout happened in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Before the arm: the machine never reached `GameState_OJZScroll_Init`.
    Boot,
    /// After the arm.
    Replay,
}

/// Why a run ran out of budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutReason {
    /// `Logic_Tick` frozen for the whole stall budget.
    Stalled { frozen_frames: u64 },
    /// The absolute frame cap.
    Deadline,
}

/// Everything worth saying about a run that ran out of budget.
#[derive(Debug, Clone)]
pub struct TimeoutReport {
    pub phase: Phase,
    pub reason: TimeoutReason,
    pub frames: u64,
    /// `None` in [`Phase::Boot`] — nothing was armed, so the replay cells mean nothing.
    pub probe: Option<Probe>,
    pub pc: u32,
    pub pc_symbol: Option<String>,
}

/// A stop at `ErrorHandlerBlob + 0`, decoded if the frame is intelligible.
#[derive(Debug, Clone)]
pub struct TrapReport {
    /// Which phase trapped. [`Phase::Boot`] means the fault happened before the arm — the replay cells in
    /// [`probe`](Self::probe) are meaningless there, and nothing about the stream is implicated.
    pub phase: Phase,
    pub decoded: Result<FaultReport, FaultDecodeError>,
    /// `(A7).l` as read — kept even when decoding failed, because it is the evidence.
    pub stack_top: Option<u32>,
    pub regs: RegSnapshot,
    pub frames: u64,
    pub probe: Probe,
}

impl TrapReport {
    /// Whether this is a checkpoint mismatch rather than an unrelated fault.
    pub fn is_desync(&self) -> bool {
        self.decoded.as_ref().is_ok_and(FaultReport::is_desync)
    }

    /// The decoded fault, when there is one.
    pub fn fault(&self) -> Option<&FaultReport> {
        self.decoded.as_ref().ok()
    }
}

/// A `Replay_Done == $FF` that does not stand up — see [`crate::outcome`]'s "A PASS is never one byte".
#[derive(Debug, Clone)]
pub struct ShortReport {
    /// Everything that was short. Never empty.
    pub shortfalls: Vec<Shortfall>,
    pub frames: u64,
    pub probe: Probe,
}

/// The verdict of one run.
#[derive(Debug, Clone)]
pub enum Verdict {
    Pass,
    /// The stream reported completion, but the completion was not corroborated. **A failure.**
    Short(Box<ShortReport>),
    Trap(Box<TrapReport>),
    Timeout(Box<TimeoutReport>),
}

/// A completed run.
#[derive(Debug, Clone)]
pub struct RunReport {
    pub verdict: Verdict,
    /// Emulated frame index at which `PC == GameState_OJZScroll_Init` was first seen.
    pub frames_to_arm: u64,
    /// Frames run after the arm.
    pub frames_after_arm: u64,
    /// The last poll of the work-RAM cells.
    pub probe: Probe,
    pub header: ReplayHeader,
    pub anchors: Anchors,
    pub fixture: Fixture,
}

impl RunReport {
    /// `Input_Source` self-cleared on the same path that set `Replay_Done` — the independent
    /// corroboration of a PASS.
    pub fn corroborated(&self) -> bool {
        self.probe.input_source_cleared()
    }
}

/// Boot, arm, run, classify.
pub fn run(prepared: &Prepared, cfg: RunConfig) -> Result<RunReport, String> {
    let a = prepared.anchors;

    // Boot with **both** predicates already attached. `boot_with_sink` makes "load, then reset, then arm"
    // inexpressible, so the sibling's "arm the breakpoint BEFORE reload_rom" ordering hazard cannot occur.
    //
    // The trap half matters as much as the arm half: every CPU exception vector routes through
    // `raise_exception` → `jsr (blob).l`, and the handler tail loops forever. A boot or level-load fault
    // under an arm-only predicate therefore burns the entire frame cap and reports a TIMEOUT with no
    // decode — the most expensive way to say "something crashed". Composed, it is a decoded FAULT within a
    // frame of the crash.
    let mut boot_sink = Fanout::new(
        StopWhen::new(|pc, _| pc == a.init),
        StopWhen::new(|pc, _| pc == a.error_handler),
    );
    let mut sys = System::boot_with_sink(POWER_ON_SEED, prepared.rom.clone(), &mut boot_sink);

    // The pad stays at all-zero for the entire run, and is never touched again: `replay.emp:157-159` reads
    // live `Ctrl_1_Press` *before* the playback overwrite and sets `Replay_Exit_Request` on Start.
    if sys.pad(0) != Pad::default() || sys.pad(1) != Pad::default() {
        return Err(
            "the machine powered on with a non-empty pad — pressing Start would set \
                    Replay_Exit_Request and end the replay early"
                .into(),
        );
    }

    let boot = sys.run_frames_with_sink(cfg.max_frames, &mut boot_sink);
    let boot_trapped = boot_sink.b.fired() || boot.pc == a.error_handler;
    let boot_armed = boot_sink.a.fired();
    if !boot_armed {
        let unarmed = |verdict| {
            Ok(RunReport {
                verdict,
                frames_to_arm: boot.frame,
                frames_after_arm: 0,
                probe: probe(&sys, &a),
                header: prepared.header,
                anchors: a,
                fixture: prepared.fixture,
            })
        };
        if boot_trapped {
            return unarmed(Verdict::Trap(Box::new(decode_trap(
                &sys,
                prepared,
                Phase::Boot,
                boot.frame,
                probe(&sys, &a),
            ))));
        }
        return unarmed(Verdict::Timeout(Box::new(TimeoutReport {
            phase: Phase::Boot,
            reason: TimeoutReason::Deadline,
            frames: boot.frame,
            probe: None,
            pc: boot.pc,
            pc_symbol: prepared.table.resolve(boot.pc).map(|r| r.to_string()),
        })));
    }
    let frames_to_arm = boot.frame;

    // The arm. Computed in typed code from a resolved symbol, which makes the recorded hex-vs-decimal
    // class of bug — three instances, each a silently-accepted wrong pointer replaying garbage from
    // inside the header — inexpressible.
    let ptr = prepared.header.body;
    {
        let mut sink = ();
        let mut bus = sys.mega_bus(&mut sink);
        bus.write16(a.replay_ptr, FC_SUPERVISOR_DATA, (ptr >> 16) as u16);
        bus.write16(a.replay_ptr + 2, FC_SUPERVISOR_DATA, ptr as u16);
        bus.write8(a.input_source, FC_SUPERVISOR_DATA, 1);
    }
    // Belt and braces: read both back off the bus's other side.
    let armed = probe(&sys, &a);
    if armed.replay_ptr != ptr {
        return Err(format!(
            "the arm did not take: Replay_Ptr reads ${:08X}, expected ${ptr:08X} (fixture ${:06X} + \
             {REPLAY_HEADER_LEN})",
            armed.replay_ptr, a.fixture
        ));
    }
    if armed.input_source != 1 {
        return Err(format!(
            "the arm did not take: Input_Source reads ${:02X}, expected $01",
            armed.input_source
        ));
    }
    // The third cell of the same read-back, and the one that is free. Work RAM is **not** zero at power-on
    // (`system.rs:393-398` seeds it with deterministic pseudo-random bytes), so `Replay_Done` reading clear
    // here is a property of *this* build's boot clearing work RAM before `Game.entry` — not of the runner.
    // Build-independence is this tool's stated value, so it is asserted rather than assumed: a flag that is
    // already set means the very first poll would report completion and the run would "pass" before it
    // started.
    if armed.replay_done != 0 {
        return Err(format!(
            "the arm is not trustworthy: Replay_Done already reads ${:02X} at the arm point, before a \
             single tick of the stream has been replayed. A pre-set flag makes the first poll report \
             completion, so this run would report a verdict it never earned. (Work RAM is seeded \
             pseudo-randomly at power-on; this build's boot is expected to clear it before Game.entry.)",
            armed.replay_done
        ));
    }

    let expected = Expected {
        tick_count: prepared.header.tick_count,
        fixture_base: a.fixture,
    };
    let mut watchdog = Watchdog::new(cfg.stall_frames);
    let mut frames = 0u64;
    loop {
        let rec = sys.run_until_stop(1, |pc, _| pc == a.error_handler);
        frames += 1;
        let p = probe(&sys, &a);
        let stalled = watchdog.observe(p.logic_tick);
        let deadline = frames >= cfg.max_frames;

        let done = |verdict| {
            Ok(RunReport {
                verdict,
                frames_to_arm,
                frames_after_arm: frames,
                probe: p,
                header: prepared.header,
                anchors: a,
                fixture: prepared.fixture,
            })
        };

        // `rec.pc` is the backstop the reviewer asked for: a `DeadlineReached` can land at an instruction
        // boundary whose PC already *is* the blob, and reporting that as a TIMEOUT would print "the machine
        // is not sitting at ErrorHandlerBlob" directly under the PC that disproves it.
        let trapped = rec.fired() || rec.pc == a.error_handler;

        match dispose(trapped, &p, &expected, stalled, deadline) {
            Disposition::Continue => continue,
            Disposition::Passed => return done(Verdict::Pass),
            Disposition::ShortCompletion => {
                return done(Verdict::Short(Box::new(ShortReport {
                    shortfalls: p.shortfalls(&expected),
                    frames,
                    probe: p,
                })))
            }
            Disposition::Trapped => {
                return done(Verdict::Trap(Box::new(decode_trap(
                    &sys,
                    prepared,
                    Phase::Replay,
                    frames,
                    p,
                ))))
            }
            Disposition::Stalled | Disposition::Deadline => {
                let reason = if stalled {
                    TimeoutReason::Stalled {
                        frozen_frames: watchdog.frozen_frames(),
                    }
                } else {
                    TimeoutReason::Deadline
                };
                return done(Verdict::Timeout(Box::new(TimeoutReport {
                    phase: Phase::Replay,
                    reason,
                    frames,
                    probe: Some(p),
                    pc: rec.pc,
                    pc_symbol: prepared.table.resolve(rec.pc).map(|r| r.to_string()),
                })));
            }
        }
    }
}

/// Read the four work-RAM cells the classification depends on.
fn probe(sys: &System, a: &Anchors) -> Probe {
    let ram = sys.ram();
    Probe {
        logic_tick: ram_u32(ram, a.logic_tick),
        replay_done: ram_u8(ram, a.replay_done),
        input_source: ram_u8(ram, a.input_source),
        replay_ptr: ram_u32(ram, a.replay_ptr),
        replay_hold: a.replay_hold.map(|addr| ram_u8(ram, addr)),
    }
}

/// Decode a stop at `ErrorHandlerBlob + 0`: `(A7).l` is the message pointer the `jsr` pushed, and the
/// registers have not been touched yet.
fn decode_trap(
    sys: &System,
    prepared: &Prepared,
    phase: Phase,
    frames: u64,
    p: Probe,
) -> TrapReport {
    let regs = RegSnapshot::from_regs(sys.cpu_regs());
    let a7 = regs.a[7];
    let Some(sp) = stack_in_work_ram(a7) else {
        return TrapReport {
            phase,
            decoded: Err(FaultDecodeError::StackNotInWorkRam { a7 }),
            stack_top: None,
            regs,
            frames,
            probe: p,
        };
    };
    // The pushed longword is a ROM pointer, and the 68000 drives only 24 address lines, so mask it the same
    // way before it is used as an index — a raw `$00A21D96`-style value is fine, but nothing guarantees the
    // top byte is clear.
    let stack_top = crate::bus_addr(ram_u32(sys.ram(), sp));
    let decoded = fault::decode(&prepared.rom, stack_top, &regs, |site| {
        prepared.table.resolve(site).map(|r| r.to_string())
    });
    TrapReport {
        phase,
        decoded,
        stack_top: Some(stack_top),
        regs,
        frames,
        probe: p,
    }
}

/// Judge a `--negative-control` run: the corruption **must** produce a `REPLAY DESYNC` whose expected hash
/// (`d2`) is the payload we wrote. Anything else means the gate is inverted and proves nothing.
///
/// Pure, so the inverted-gate verdict is unit-tested rather than only ever observed.
pub fn judge_negative_control(
    fault: Option<&FaultReport>,
    expected_payload: u32,
) -> Result<String, String> {
    let Some(f) = fault else {
        return Err(
            "the corrupted checkpoint did NOT trap. Either the arm silently failed, or this ROM is \
             not comparing checkpoints at all — in both cases a green from this runner would mean \
             nothing. THE GATE IS INVERTED."
                .into(),
        );
    };
    let Some(d) = f.desync else {
        return Err(format!(
            "a trap fired, but it was `{}`, not `{}` — the corruption did not reach the checkpoint \
             compare, so this run does not demonstrate the gate works",
            f.message,
            fault::DESYNC_MESSAGE
        ));
    };
    if d.expected != expected_payload {
        return Err(format!(
            "a desync fired, but it expected ${:08X} rather than the ${expected_payload:08X} we \
             wrote — a DIFFERENT checkpoint mismatched, so the corruption is not what tripped it",
            d.expected
        ));
    }
    Ok(format!(
        "the corrupted checkpoint tripped the gate: `{}` at Logic_Tick {}, expected ${:08X} \
         (the payload we wrote), actual ${:08X}",
        f.message, d.logic_tick, d.expected, d.actual
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fault::{DesyncDetail, DESYNC_MESSAGE};

    fn fault_with(message: &str, desync: Option<DesyncDetail>) -> FaultReport {
        FaultReport {
            message: message.to_string(),
            truncated: false,
            message_addr: 0x26AC,
            raise_site: 0x26A6,
            raise_site_symbol: None,
            regs: RegSnapshot {
                d: [0; 8],
                a: [0; 8],
                pc: 0,
                sr: 0,
            },
            desync,
        }
    }

    #[test]
    fn the_negative_control_passes_only_on_the_desync_it_planted() {
        let good = fault_with(
            DESYNC_MESSAGE,
            Some(DesyncDetail {
                actual: 0x1D37_5066,
                logic_tick: 2,
                expected: NEGATIVE_CONTROL_PAYLOAD,
            }),
        );
        assert!(judge_negative_control(Some(&good), NEGATIVE_CONTROL_PAYLOAD).is_ok());
    }

    /// The failure this whole flag exists to catch: nothing tripped.
    #[test]
    fn a_negative_control_that_does_not_trip_is_an_inverted_gate() {
        let e = judge_negative_control(None, NEGATIVE_CONTROL_PAYLOAD).unwrap_err();
        assert!(e.contains("INVERTED"), "{e}");
    }

    /// A trap that is not a desync does not demonstrate the checkpoint compare works.
    #[test]
    fn an_unrelated_trap_does_not_satisfy_the_negative_control() {
        let other = fault_with("REPLAY BAD OPCODE", None);
        assert!(judge_negative_control(Some(&other), NEGATIVE_CONTROL_PAYLOAD).is_err());
    }

    /// …and neither does a desync at some *other* checkpoint, which would mean the run diverged for an
    /// unrelated reason and the corruption was never reached.
    #[test]
    fn a_desync_at_a_different_checkpoint_does_not_satisfy_it() {
        let elsewhere = fault_with(
            DESYNC_MESSAGE,
            Some(DesyncDetail {
                actual: 1,
                logic_tick: 900,
                expected: 0x1234_5678,
            }),
        );
        let e = judge_negative_control(Some(&elsewhere), NEGATIVE_CONTROL_PAYLOAD).unwrap_err();
        assert!(e.contains("DIFFERENT checkpoint"), "{e}");
    }

    /// A release ROM must be refused before anything else happens, and named as such.
    #[test]
    fn prepare_refuses_a_rom_without_the_compare_path() {
        let e = Prepared::new(vec![0u8; 0x1000], "", Fixture::Ojz)
            .err()
            .expect("must refuse");
        assert!(e.contains("REPLAY DESYNC"), "{e}");
        assert!(e.contains("false green"), "{e}");
    }

    /// A negative control whose "corruption" changes nothing proves nothing: any trap that followed would
    /// be the recorded hash mismatching on its own merits, and we would credit our patch for it.
    #[test]
    fn corrupting_a_payload_that_is_already_the_sentinel_is_refused() {
        let mut rom = vec![0u8; 0x400];
        rom[0x100..0x100 + 4].copy_from_slice(b"ARP0");
        rom[0x100 + 6..0x100 + 10].copy_from_slice(&7u32.to_be_bytes());
        rom[0x114..0x11A].copy_from_slice(&[0xFF, 0x01, 0xDE, 0xAD, 0xBE, 0xEF]);
        let header = crate::header::ReplayHeader::parse(&rom, 0x100).expect("a usable header");
        let mut p = Prepared {
            rom,
            table: oracle_core::symbols::SymbolTable::parse(
                "  Symbol Table (* = unused):\n\n Main : 300 C |\n\n   1 symbols\n",
            )
            .unwrap(),
            anchors: Anchors {
                init: 0,
                fixture: 0x100,
                replay_ptr: 0,
                input_source: 0,
                replay_done: 0,
                logic_tick: 0,
                error_handler: 0,
                replay_hold: None,
            },
            header,
            fixture: Fixture::Ojz,
            notes: Vec::new(),
        };
        let e = p.corrupt_first_checkpoint().expect_err("must refuse");
        assert!(e.contains("ALREADY"), "{e}");
        assert!(e.contains("no-op"), "{e}");
    }

    /// **A fault before the arm must be a decoded FAULT, not a TIMEOUT.**
    ///
    /// Every CPU exception vector routes through `raise_exception` → `jsr (blob).l`, and the handler tail
    /// loops forever. Under an arm-only boot predicate, a boot or level-load fault therefore burns the
    /// whole frame cap and reports TIMEOUT with no decode — and `print_timeout` then asserts "the machine
    /// is not sitting at ErrorHandlerBlob", which in `Phase::Boot` was simply false.
    ///
    /// This uses a hand-assembled ROM rather than the real artifacts, so it runs everywhere: reset lands on
    /// a `jsr (blob).l` with an inline message behind it, and `GameState_OJZScroll_Init` is at an address
    /// the machine never reaches.
    #[test]
    fn a_fault_before_the_arm_is_a_decoded_fault_not_a_timeout() {
        const END_OF_ROM: usize = 0x8000;
        let mut rom = vec![0u8; END_OF_ROM + 0x4000];
        // Vectors: SSP into work RAM (so the frame is readable), reset PC at $200.
        rom[0..4].copy_from_slice(&0x00FF_FE00u32.to_be_bytes());
        rom[4..8].copy_from_slice(&0x0000_0200u32.to_be_bytes());
        // The refusal's positive assertion.
        rom[0x100..0x100 + policy::DESYNC_TRAP_STRING.len()]
            .copy_from_slice(policy::DESYNC_TRAP_STRING);
        // $200: jsr $00000400.l — the blob — with the message inline behind it, exactly as
        // `raise_exception` lowers.
        rom[0x200..0x206].copy_from_slice(&[0x4E, 0xB9, 0x00, 0x00, 0x04, 0x00]);
        rom[0x206..0x206 + 11].copy_from_slice(b"BOOT FAULT\0");
        // $400: the blob, whose tail loops forever (`bra.s *`).
        rom[0x400..0x402].copy_from_slice(&[0x60, 0xFE]);
        // A well-formed fixture, so nothing else refuses first.
        rom[0x1000..0x1004].copy_from_slice(b"ARP0");
        rom[0x1006..0x100A].copy_from_slice(&99u32.to_be_bytes());
        rom[0x1014..0x101A].copy_from_slice(&[0xFF, 0x01, 0x11, 0x22, 0x33, 0x44]);
        rom[END_OF_ROM] = 0xDE;
        rom[END_OF_ROM + 1] = 0xB2;

        let lst = "  Symbol Table (* = unused):\n\n \
             ErrorHandlerBlob : 400 C |\n \
             GameState_OJZScroll_Init : 600 C |\n \
             Input_Source : FFFF8036 C |\n \
             Logic_Tick : FFFF8004 C |\n \
             Replay_Done : FFFF8038 C |\n \
             Replay_OJZ_Fixture : 1000 C |\n \
             Replay_Ptr : FFFF803C C |\n \
             EndOfRom : 8000 C |\n\n   8 symbols\n";
        let p = Prepared::new(rom, lst, Fixture::Ojz).expect("this synthetic pair must prepare");

        let r = run(
            &p,
            RunConfig {
                max_frames: 4,
                stall_frames: 2,
            },
        )
        .expect("the run must reach a verdict");

        let Verdict::Trap(t) = &r.verdict else {
            panic!(
                "a boot fault must be a decoded FAULT, not {:?} — an arm-only boot predicate would burn \
                 the whole frame cap and report a TIMEOUT with nothing decoded",
                r.verdict
            );
        };
        assert_eq!(t.phase, Phase::Boot, "and it must say it happened pre-arm");
        let f = t
            .fault()
            .unwrap_or_else(|| panic!("the frame must decode: {:?}", t.decoded));
        assert_eq!(f.message, "BOOT FAULT");
        assert!(
            !f.is_desync(),
            "a boot fault implicates nothing about the stream"
        );
        assert_eq!(f.raise_site, 0x200, "(A7).l - 6 is the jsr");
        assert_eq!(r.frames_after_arm, 0, "nothing was ever armed");
        // …and it happens immediately, not at the end of the budget.
        assert!(t.frames <= 1, "the trap fired at frame {}", t.frames);
    }

    /// …and an unresolved symbol is fatal by name, never a silent fallback to a literal.
    #[test]
    fn prepare_names_the_symbol_it_could_not_resolve() {
        let mut rom = vec![0u8; 0x8000 + 0x4000];
        rom[0x100..0x100 + policy::DESYNC_TRAP_STRING.len()]
            .copy_from_slice(policy::DESYNC_TRAP_STRING);
        rom[0x8000] = 0xDE;
        rom[0x8001] = 0xB2;
        let lst = "  Symbol Table (* = unused):\n\n EndOfRom : 8000 C |\n\n   1 symbols\n";
        let e = Prepared::new(rom, lst, Fixture::Ojz)
            .err()
            .expect("must refuse");
        assert!(e.contains("GameState_OJZScroll_Init"), "{e}");
    }
}
