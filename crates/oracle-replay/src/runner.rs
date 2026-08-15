//! Booting the machine, arming the stream, and running it to a verdict.
//!
//! Everything here is the impure half; the decisions it makes live in [`crate::header`],
//! [`crate::policy`], [`crate::fault`] and [`crate::outcome`], which are pure and unit-tested without a ROM.

use crate::cli::Fixture;
use crate::fault::{self, FaultDecodeError, FaultReport, RegSnapshot};
use crate::header::{HeaderError, ReplayHeader, REPLAY_HEADER_LEN};
use crate::outcome::{dispose, Disposition, Probe, Watchdog};
use crate::policy::{self, LstVerdict};
use crate::{ram_u32, ram_u8, stack_in_work_ram};
use oracle_core::bus::StopWhen;
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

/// The verdict of one run.
#[derive(Debug, Clone)]
pub enum Verdict {
    Pass,
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

    // Boot with the arm predicate already attached. `boot_with_sink` makes "load, then reset, then arm"
    // inexpressible, so the sibling's "arm the breakpoint BEFORE reload_rom" ordering hazard cannot occur.
    let mut arm = StopWhen::new(|pc, _| pc == a.init);
    let mut sys = System::boot_with_sink(POWER_ON_SEED, prepared.rom.clone(), &mut arm);

    // The pad stays at all-zero for the entire run, and is never touched again: `replay.emp:157-159` reads
    // live `Ctrl_1_Press` *before* the playback overwrite and sets `Replay_Exit_Request` on Start.
    if sys.pad(0) != Pad::default() || sys.pad(1) != Pad::default() {
        return Err(
            "the machine powered on with a non-empty pad — pressing Start would set \
                    Replay_Exit_Request and end the replay early"
                .into(),
        );
    }

    let boot = sys.run_frames_with_sink(cfg.max_frames, &mut arm);
    if !boot.fired() {
        return Ok(RunReport {
            verdict: Verdict::Timeout(Box::new(TimeoutReport {
                phase: Phase::Boot,
                reason: TimeoutReason::Deadline,
                frames: boot.frame,
                probe: None,
                pc: boot.pc,
                pc_symbol: prepared.table.resolve(boot.pc).map(|r| r.to_string()),
            })),
            frames_to_arm: boot.frame,
            frames_after_arm: 0,
            probe: probe(&sys, &a),
            header: prepared.header,
            anchors: a,
            fixture: prepared.fixture,
        });
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

        match dispose(rec.fired(), &p, stalled, deadline) {
            Disposition::Continue => continue,
            Disposition::Passed => return done(Verdict::Pass),
            Disposition::Trapped => {
                return done(Verdict::Trap(Box::new(decode_trap(
                    &sys, prepared, frames, p,
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
fn decode_trap(sys: &System, prepared: &Prepared, frames: u64, p: Probe) -> TrapReport {
    let regs = RegSnapshot::from_regs(sys.cpu_regs());
    let a7 = regs.a[7];
    let Some(sp) = stack_in_work_ram(a7) else {
        return TrapReport {
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
