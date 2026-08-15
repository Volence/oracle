//! `replay_runner` — the command-line face of [`oracle_replay`].
//!
//! Output is a structured, human-readable report on stdout, with refusals and failures also named on
//! stderr; the exit code is what a bash harness in another repo consumes. See [`oracle_replay::cli::USAGE`]
//! for the codes.

use oracle_replay::cli::{self, Args, Parsed};
use oracle_replay::fault::FaultReport;
use oracle_replay::header::REPLAY_HEADER_LEN;
use oracle_replay::runner::{
    self, Phase, Prepared, RunConfig, RunReport, TimeoutReason, TimeoutReport, TrapReport, Verdict,
    NEGATIVE_CONTROL_PAYLOAD,
};

/// Exit codes. Kept as named constants because another repo's harness branches on them.
mod exit {
    pub const PASS: i32 = 0;
    pub const USAGE_OR_REFUSAL: i32 = 1;
    pub const DESYNC: i32 = 2;
    pub const FAULT: i32 = 3;
    pub const TIMEOUT: i32 = 4;
    pub const GATE_INVERTED: i32 = 5;
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match cli::parse(args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("replay_runner: {e}");
            std::process::exit(exit::USAGE_OR_REFUSAL);
        }
    };
    let args = match parsed {
        Parsed::Help => {
            print!("{}", cli::USAGE);
            return;
        }
        Parsed::Run(a) => *a,
    };

    std::process::exit(match go(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("replay_runner: {e}");
            exit::USAGE_OR_REFUSAL
        }
    });
}

fn go(args: &Args) -> Result<i32, String> {
    let rom = std::fs::read(&args.rom)
        .map_err(|e| format!("cannot read the ROM {}: {e}", args.rom.display()))?;
    let lst = std::fs::read_to_string(&args.lst)
        .map_err(|e| format!("cannot read the listing {}: {e}", args.lst.display()))?;

    println!("replay_runner");
    println!("  rom      {} ({} bytes)", args.rom.display(), rom.len());
    println!("  lst      {}", args.lst.display());
    println!("  fixture  {}", args.fixture);

    let mut prepared = Prepared::new(rom, &lst, args.fixture)?;
    for note in &prepared.notes {
        println!("  {note}");
    }

    let mut planted = None;
    if args.negative_control {
        let (at, was) = prepared.corrupt_first_checkpoint()?;
        planted = Some((at, was));
        println!(
            "  NEGATIVE CONTROL: checkpoint payload at ${at:06X} patched ${was:08X} -> \
             ${NEGATIVE_CONTROL_PAYLOAD:08X} — the trap MUST fire"
        );
    }

    let a = prepared.anchors;
    let h = prepared.header;
    println!(
        "  anchors  init=${:06X} fixture=${:06X} blob=${:06X}",
        a.init, a.fixture, a.error_handler
    );
    println!(
        "           Logic_Tick=${:06X} Input_Source=${:06X} Replay_Done=${:06X} Replay_Ptr=${:06X}",
        a.logic_tick, a.input_source, a.replay_done, a.replay_ptr
    );
    println!(
        "  stream   ARP0 flags=${:02X} ticks={} core_hash=${:08X} (stale, not a guard) seed=${:08X} \
         body=${:06X}",
        h.flags, h.tick_count, h.core_hash, h.rng_seed, h.body
    );

    let report = runner::run(
        &prepared,
        RunConfig {
            max_frames: args.max_frames,
            stall_frames: args.stall_frames,
        },
    )?;

    print_verdict(&report);

    if args.negative_control {
        let (at, _) = planted.expect("planted when --negative-control");
        let fault: Option<&FaultReport> = match &report.verdict {
            Verdict::Trap(t) => t.fault(),
            _ => None,
        };
        return Ok(
            match runner::judge_negative_control(fault, NEGATIVE_CONTROL_PAYLOAD) {
                Ok(why) => {
                    println!("\nNEGATIVE CONTROL PASSED — {why}");
                    println!("  (the corruption was planted at ${at:06X}; the gate demonstrably fails when it should)");
                    exit::PASS
                }
                Err(why) => {
                    eprintln!("\nNEGATIVE CONTROL FAILED — {why}");
                    exit::GATE_INVERTED
                }
            },
        );
    }

    Ok(match &report.verdict {
        Verdict::Pass => exit::PASS,
        Verdict::Trap(t) if t.is_desync() => exit::DESYNC,
        Verdict::Trap(_) => exit::FAULT,
        Verdict::Timeout(_) => exit::TIMEOUT,
    })
}

fn print_verdict(r: &RunReport) {
    println!(
        "  run      armed at frame {}, {} frames after the arm",
        r.frames_to_arm, r.frames_after_arm
    );
    match &r.verdict {
        Verdict::Pass => {
            println!("\nPASS — the stream ran to its end.");
            println!(
                "  Replay_Done = $FF, Logic_Tick = {} (header declares {} ticks; an overshoot is \
                 normal — the game keeps running on live input after end-of-stream)",
                r.probe.logic_tick, r.header.tick_count
            );
            println!(
                "  Input_Source = ${:02X} — {}",
                r.probe.input_source,
                if r.corroborated() {
                    "self-cleared on the completion path, corroborating the verdict"
                } else {
                    "WARNING: not cleared, which the completion path should have done"
                }
            );
        }
        Verdict::Trap(t) => print_trap(t, r),
        Verdict::Timeout(t) => print_timeout(t, r),
    }
}

fn print_trap(t: &TrapReport, r: &RunReport) {
    match &t.decoded {
        Ok(f) if f.is_desync() => {
            let d = f.desync.expect("a desync carries its detail");
            println!("\nDESYNC — a checkpoint did not match.");
            println!(
                "  Logic_Tick {}   expected ${:08X}   actual ${:08X}",
                d.logic_tick, d.expected, d.actual
            );
        }
        Ok(f) => {
            println!("\nFAULT — `{}` was raised during the replay.", f.message);
        }
        Err(e) => {
            println!("\nFAULT — the machine stopped at ErrorHandlerBlob, but the frame is not decodable: {e}");
        }
    }
    if let Ok(f) = &t.decoded {
        println!("  message  \"{}\" at ${:06X}", f.message, f.message_addr);
        println!(
            "  raised at ${:06X}{}",
            f.raise_site,
            f.raise_site_symbol
                .as_ref()
                .map(|s| format!("  ({s})"))
                .unwrap_or_default()
        );
    }
    if let Some(sp) = t.stack_top {
        println!("  (A7).l   ${sp:08X}");
    }
    println!("  registers (PRE-CLOBBER — stopped at blob+0, before the handler draws its screen):");
    println!("{}", t.regs);
    println!(
        "  work RAM Logic_Tick={} Replay_Done=${:02X} Input_Source=${:02X} stream offset {}",
        t.probe.logic_tick,
        t.probe.replay_done,
        t.probe.input_source,
        t.probe.stream_offset(r.anchors.fixture)
    );
}

fn print_timeout(t: &TimeoutReport, r: &RunReport) {
    let what = match t.reason {
        TimeoutReason::Stalled { frozen_frames } => {
            format!("Logic_Tick has not advanced for {frozen_frames} frames")
        }
        TimeoutReason::Deadline => format!("the {} frame cap was reached", t.frames),
    };
    let phase = match t.phase {
        Phase::Boot => "before the arm — the machine never reached GameState_OJZScroll_Init",
        Phase::Replay => "after the arm",
    };
    println!("\nTIMEOUT — {what} ({phase}).");
    println!(
        "  pc       ${:06X}{}",
        t.pc,
        t.pc_symbol
            .as_ref()
            .map(|s| format!("  ({s})"))
            .unwrap_or_default()
    );
    if let Some(p) = t.probe {
        let off = p.stream_offset(r.anchors.fixture);
        println!("  Logic_Tick   {}", p.logic_tick);
        println!("  Replay_Done  ${:02X}", p.replay_done);
        println!("  Input_Source ${:02X}", p.input_source);
        if let Some(hold) = p.replay_hold {
            println!("  Replay_Hold  ${hold:02X}");
        }
        println!(
            "  Replay_Ptr   ${:08X}  (fixture + {off}){}",
            p.replay_ptr,
            if off < i64::from(REPLAY_HEADER_LEN) {
                "  <-- STILL INSIDE THE HEADER: this is the signature of a bad arm"
            } else {
                ""
            }
        );
    }
    println!(
        "  (the trap predicate was checked first — the machine is not sitting at ErrorHandlerBlob, so \
         this is a genuine stall rather than a desync wearing a hang's clothes)"
    );
}
