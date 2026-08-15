//! `replay_runner` — the command-line face of [`oracle_replay`].
//!
//! Output is a structured, human-readable report on stdout, with refusals and failures also named on
//! stderr; the exit code is what a bash harness in another repo consumes. See [`oracle_replay::cli::USAGE`]
//! for the codes.

use oracle_replay::artifacts::{self, Artifact};
use oracle_replay::cli::{self, Args, Parsed};
use oracle_replay::fault::FaultReport;
use oracle_replay::header::REPLAY_HEADER_LEN;
use oracle_replay::restamp::{self, PatchMeta, RestampPlan};
use oracle_replay::runner::{
    self, Phase, Prepared, RestampSession, RunConfig, RunReport, ShortReport, TimeoutReason,
    TimeoutReport, TrapReport, Verdict, NEGATIVE_CONTROL_PAYLOAD,
};
use std::path::Path;

/// Exit codes. Kept as named constants because another repo's harness branches on them.
mod exit {
    pub const PASS: i32 = 0;
    pub const USAGE_OR_REFUSAL: i32 = 1;
    pub const DESYNC: i32 = 2;
    pub const FAULT: i32 = 3;
    pub const TIMEOUT: i32 = 4;
    pub const GATE_INVERTED: i32 = 5;
    /// The stream reported completion that its own cells do not corroborate — a truncated or mis-packed
    /// stream. Distinct from every other code because it is the one failure that used to be a PASS.
    pub const SHORT: i32 = 6;
    /// `--restamp` computed a repair, but the re-stamped image did not come back clean (or the negative
    /// control stopped tripping on it). **Nothing is written.** Distinct from every other code because the
    /// pass itself succeeded — what failed is the proof that its output is good.
    pub const RESTAMP_UNVERIFIED: i32 = 7;
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

    let cfg = RunConfig {
        max_frames: args.max_frames,
        stall_frames: args.stall_frames,
    };

    if args.restamp {
        return go_restamp(args, prepared, cfg);
    }

    let report = runner::run(&prepared, cfg)?;

    print_verdict(&report);

    if args.negative_control {
        let (at, _) = planted.expect("planted when --negative-control");
        // Branch on the verdict's *shape* first. A run that never reached a verdict cannot say anything
        // about whether the gate is inverted, and reporting a timeout as "THE GATE IS INVERTED" is a
        // confident wrong answer where an honest "inconclusive" was available.
        if let Verdict::Timeout(t) = &report.verdict {
            eprintln!(
                "\nNEGATIVE CONTROL INCONCLUSIVE — the run timed out ({}) before it could either trap \
                 or complete, so it says nothing about whether the gate works. Diagnose the timeout \
                 above first; this is NOT evidence of an inverted gate.",
                match t.reason {
                    TimeoutReason::Stalled { frozen_frames } =>
                        format!("Logic_Tick frozen for {frozen_frames} frames"),
                    TimeoutReason::Deadline => format!("{} frame cap", t.frames),
                }
            );
            return Ok(exit::TIMEOUT);
        }
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

    Ok(code_for(&report.verdict))
}

fn code_for(v: &Verdict) -> i32 {
    match v {
        Verdict::Pass => exit::PASS,
        Verdict::Short(_) => exit::SHORT,
        Verdict::Trap(t) if t.is_desync() => exit::DESYNC,
        Verdict::Trap(_) => exit::FAULT,
        Verdict::Timeout(_) => exit::TIMEOUT,
    }
}

// -------------------------------------------------------------------------------------------------
// --restamp
// -------------------------------------------------------------------------------------------------

/// One pass that finds **every** stale checkpoint, then proves its own output before emitting it.
///
/// The order is the safety story:
///
/// 1. Walk the stream statically. A stream that does not reconcile is refused here, before the machine
///    boots — re-stamping a truncated one would write fresh hashes into a fixture that verifies almost
///    nothing while looking green, and the runner's runtime SHORT check cannot prevent that.
/// 2. Verify the committed fixture `.bin` against the copy embedded in the ROM, if one was given.
/// 3. Check every output path against the write guard — *before* spending a full playthrough.
/// 4. Install the recovery stub and run the single pass.
/// 5. Require a PASS. Anything else means the pass did not see the whole stream, so it cannot have found
///    every stale checkpoint, and a half-repaired fixture is worse than none.
/// 6. Apply the plan to a pristine copy, then **re-run that image clean end to end** and re-run the
///    negative control on it. This is what discharges the claim that one instrumented pass equals seven
///    sequential playthroughs.
/// 7. Only then write anything.
fn go_restamp(args: &Args, mut prepared: Prepared, cfg: RunConfig) -> Result<i32, String> {
    use std::time::Instant;

    println!("\n--restamp");

    // 1. The authoritative slot map.
    let map = prepared.stream_map()?;
    println!(
        "  stream   walked {} checkpoints over {} ticks — reconciles with the header, so its payload \
         offsets can be vouched for",
        map.slots.len(),
        map.total_ticks
    );

    // 2. The committed fixture, proven to be the bytes the ROM carries.
    let mut fixture_blob = match &args.fixture_bin {
        Some(p) => {
            let blob = std::fs::read(p)
                .map_err(|e| format!("cannot read the fixture {}: {e}", p.display()))?;
            restamp::verify_fixture_embedding(&prepared.rom, prepared.anchors.fixture, &blob)?;
            println!(
                "  fixture  {} ({} bytes) is byte-identical to the copy embedded at ${:06X}",
                p.display(),
                blob.len(),
                prepared.anchors.fixture
            );
            Some(blob)
        }
        None => {
            println!(
                "  fixture  no --fixture-bin given: the patch report will carry fixture-file offsets, \
                 but no re-stamped .bin can be emitted"
            );
            None
        }
    };

    // 3. Where anything would go, checked before a single frame is spent.
    let guard = artifacts::guard_for_inputs(
        &[args.rom.as_path(), args.lst.as_path()],
        args.allow_source_write,
        args.force,
    );
    for r in &guard.protected {
        println!(
            "  guard    {} is protected (the inputs came from it)",
            r.display()
        );
    }
    let targets = planned_targets(args, fixture_blob.is_some());
    if targets.is_empty() {
        println!(
            "  output   DRY RUN — no --out, so nothing will be written anywhere; the repair is \
             reported on stdout"
        );
    }
    for (path, what) in &targets {
        guard.check_path(path)?;
        println!("  output   {what} -> {}", path.display());
    }

    // 4. The pass.
    let pristine = prepared.rom.clone();
    let stub = prepared.install_recovery_stub()?;
    println!(
        "  stub     Input_Tick.desync (${:06X}) {:02X?} -> {:02X?}",
        stub.at, stub.replaced, stub.bytes
    );
    println!(
        "           = `movea.l (Replay_Ptr).w, a0` + `jmp Input_Tick.fetch_a0`, i.e. the compare's own \
         match path reached absolutely. Every stale checkpoint now STOPS the machine where it used to \
         TRAP it, so the pass continues instead of ending."
    );

    let mut session = RestampSession::new(&stub, &map);
    let t0 = Instant::now();
    let report = runner::run_restamp(&prepared, cfg, &mut session)?;
    let pass_secs = t0.elapsed().as_secs_f64();
    let found = session.stale().len();
    print_verdict(&report);

    if !matches!(report.verdict, Verdict::Pass) {
        eprintln!(
            "\nRESTAMP ABORTED — the pass stopped before the end of the stream, so it cannot have seen \
             every checkpoint. {found} stale checkpoint(s) were found on the way, and they are NOT \
             reported as a repair: a plan built from a partial pass would half-repair the fixture, which \
             is worse than not repairing it. Diagnose the verdict above first."
        );
        return Ok(code_for(&report.verdict));
    }

    let plan = session.into_plan(pristine.len());
    println!(
        "\nRESTAMP PASS COMPLETE — one pass, {:.2} s, {} of {} checkpoints stale.",
        pass_secs,
        plan.stale.len(),
        plan.total_checkpoints
    );
    println!(
        "  (the manual loop costs one full playthrough per stale checkpoint; this found them all in one)"
    );
    print_stale_table(&plan);

    if plan.is_clean() {
        println!(
            "\nNOTHING TO RE-STAMP — every checkpoint matched. The fixture is already current, and no \
             artifact is emitted (there is no repair to review)."
        );
        return Ok(exit::PASS);
    }

    // 5. The repair, and the proof it is good.
    let mut restamped = pristine.clone();
    plan.apply_to_rom(&mut restamped)?;
    if restamped.len() != pristine.len() {
        return Err("the re-stamped image changed length — impossible for a re-stamp".into());
    }
    println!(
        "\n  length   {} bytes before and after — a re-stamp changes hash payloads only, so EndOfRom \
         does not move and no sigil repin is needed",
        restamped.len()
    );

    println!("\nVERIFYING the re-stamped image (this is what makes one pass equal seven):");
    let verify_target = prepared.with_rom(restamped.clone())?;
    let t1 = Instant::now();
    let verified = runner::run(&verify_target, cfg)?;
    let verify_secs = t1.elapsed().as_secs_f64();
    if !matches!(verified.verdict, Verdict::Pass) {
        print_verdict(&verified);
        eprintln!(
            "\nRESTAMP UNVERIFIED — the re-stamped image does not run clean. The repair is NOT written. \
             This is the check that catches an instrumented pass whose behaviour differed from a plain \
             run; treat the verdict above as the finding, not this tool's output."
        );
        return Ok(exit::RESTAMP_UNVERIFIED);
    }
    println!(
        "  clean run  PASS in {verify_secs:.2} s — Logic_Tick {} over the header's {} ticks, every one \
         of the {} checkpoints compared and matched",
        verified.probe.logic_tick, verified.header.tick_count, plan.total_checkpoints
    );

    let mut control = prepared.with_rom(restamped.clone())?;
    let (at, was) = control.corrupt_first_checkpoint()?;
    let control_report = runner::run(&control, cfg)?;
    let control_fault = match &control_report.verdict {
        Verdict::Trap(t) => t.fault(),
        _ => None,
    };
    match runner::judge_negative_control(control_fault, NEGATIVE_CONTROL_PAYLOAD) {
        Ok(why) => println!(
            "  control    still trips on the re-stamped image — {why}\n             (planted at \
             ${at:06X}, over ${was:08X})"
        ),
        Err(why) => {
            eprintln!(
                "\nRESTAMP UNVERIFIED — the re-stamped image runs clean, but the negative control no \
                 longer trips on it: {why}\nA fixture that cannot fail is not a fixture. The repair is \
                 NOT written."
            );
            return Ok(exit::RESTAMP_UNVERIFIED);
        }
    }

    // 6. Emit.
    let patch = restamp::render_patch(
        &plan,
        &PatchMeta {
            rom_path: &args.rom.display().to_string(),
            lst_path: &args.lst.display().to_string(),
            fixture_symbol: args.fixture.symbol(),
            fixture_name: args.fixture.cli_name(),
            total_ticks: map.total_ticks,
        },
    );
    if let Some(blob) = fixture_blob.as_mut() {
        plan.apply_to_fixture(blob)?;
    }

    if targets.is_empty() {
        println!("\n--- patch artifact (dry run: stdout only) ---");
        print!("{patch}");
        println!("--- end ---");
        println!(
            "\nDRY RUN — verified, and NOTHING was written. Re-run with --out <dir> to emit the patch \
             report{}.",
            if fixture_blob.is_some() {
                " and the re-stamped fixture .bin"
            } else {
                " (and --fixture-bin <path> for the re-stamped fixture .bin, the artifact of record)"
            }
        );
        return Ok(exit::PASS);
    }

    let mut out = Vec::new();
    for (path, what) in targets {
        let bytes = match what {
            PATCH => patch.clone().into_bytes(),
            FIXTURE => fixture_blob
                .clone()
                .expect("planned only when a fixture was read"),
            ROM => restamped.clone(),
            other => return Err(format!("unknown artifact kind `{other}`")),
        };
        out.push(Artifact { path, what, bytes });
    }
    artifacts::write_all(&guard, &out)?;
    println!("\nWROTE:");
    for a in &out {
        println!(
            "  {:>18}  {} ({} bytes)",
            a.what,
            a.path.display(),
            a.bytes.len()
        );
    }
    println!(
        "\nApply the fixture .bin over aeon/games/sonic4/data/replays/{}.bin and rebuild. \
         `tools/test_replay_fixture.py` should stay green: the length is unchanged, the input stream is \
         untouched, and only {} expected-hash payload(s) moved.",
        args.fixture.cli_name(),
        plan.stale.len()
    );
    Ok(exit::PASS)
}

const PATCH: &str = "patch report";
const FIXTURE: &str = "re-stamped fixture";
const ROM: &str = "re-stamped ROM";

/// What `--out` would receive, in the order it is written. Pure: no filesystem access, so the guard can be
/// applied to every one of them before a single frame of emulation is spent.
fn planned_targets(args: &Args, have_fixture: bool) -> Vec<(std::path::PathBuf, &'static str)> {
    let Some(dir) = &args.out else {
        return Vec::new();
    };
    let mut v = vec![(
        dir.join(format!("{}.restamp.patch", args.fixture.cli_name())),
        PATCH,
    )];
    if have_fixture {
        let name = args
            .fixture_bin
            .as_ref()
            .and_then(|p| p.file_name())
            .map(Path::new)
            .unwrap_or_else(|| Path::new("fixture.bin"));
        v.push((dir.join(name), FIXTURE));
    }
    if args.emit_rom {
        let name = args
            .rom
            .file_name()
            .map(Path::new)
            .unwrap_or_else(|| Path::new("restamped.bin"));
        v.push((dir.join(name), ROM));
    }
    v
}

/// The table the whole flag exists to produce: every stale checkpoint found in one pass.
fn print_stale_table(plan: &RestampPlan) {
    if plan.is_clean() {
        return;
    }
    println!("\n   idx   ring   tick     rom_off   fix_off   expected    actual");
    for s in &plan.stale {
        println!(
            "  {:4}  {:5}  {:5}    {:06X}    {:06X}    {:08X}    {:08X}",
            s.index, s.ring, s.logic_tick, s.payload, s.fixture_offset, s.expected, s.actual
        );
    }
}

fn print_verdict(r: &RunReport) {
    println!(
        "  run      armed at frame {}, {} frames after the arm",
        r.frames_to_arm, r.frames_after_arm
    );
    match &r.verdict {
        Verdict::Pass => {
            println!("\nPASS — the stream ran to its end, corroborated three ways.");
            println!("  Replay_Done  = $FF");
            println!(
                "  Logic_Tick   = {} >= the {} ticks the header declares (an overshoot is normal — the \
                 game keeps running on live input after end-of-stream)",
                r.probe.logic_tick, r.header.tick_count
            );
            println!(
                "  Input_Source = ${:02X} — self-cleared on the completion path",
                r.probe.input_source
            );
            println!(
                "  Replay_Ptr   = ${:08X} — fixture+{}, well past the {REPLAY_HEADER_LEN}-byte header",
                r.probe.replay_ptr,
                r.probe.stream_offset(r.anchors.fixture)
            );
        }
        Verdict::Short(s) => print_short(s, r),
        Verdict::Trap(t) => print_trap(t, r),
        Verdict::Timeout(t) => print_timeout(t, r),
    }
}

/// The failure that used to be a PASS: `Replay_Done` set, corroborations missing.
fn print_short(s: &ShortReport, r: &RunReport) {
    println!("\nSHORT COMPLETION — Replay_Done is $FF, but this run verified less than it claims.");
    println!(
        "  The playback path reached an end-of-stream opcode, so the flag is honestly set. What is \
         wrong is WHICH end it reached:"
    );
    for f in &s.shortfalls {
        println!("    - {f}");
    }
    println!(
        "  This is what a truncated or mis-packed stream looks like. A byte compare on Replay_Done \
         alone would have called it a PASS."
    );
    println!(
        "  cells    Logic_Tick={} Replay_Done=${:02X} Input_Source=${:02X} Replay_Ptr=${:08X} \
         (fixture+{})",
        s.probe.logic_tick,
        s.probe.replay_done,
        s.probe.input_source,
        s.probe.replay_ptr,
        s.probe.stream_offset(r.anchors.fixture)
    );
    println!("  header   declares {} ticks", r.header.tick_count);
}

fn print_trap(t: &TrapReport, r: &RunReport) {
    if t.phase == Phase::Boot {
        println!(
            "\n(this trap fired BEFORE the arm — during boot or level load. The stream was never armed, \
             so nothing below implicates the fixture; the replay cells are whatever boot left there.)"
        );
    }
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
        println!(
            "  message  \"{}\"{} at ${:06X}",
            f.message,
            if f.truncated {
                " (readable prefix; the rest is MD Debugger format-control bytes — every `assert` site \
                 carries them)"
            } else {
                ""
            },
            f.message_addr
        );
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
            if p.stuck_in_header(r.anchors.fixture) {
                "  <-- STILL INSIDE THE HEADER: this is the signature of a bad arm"
            } else {
                ""
            }
        );
    }
    // Only true after the arm. In `Phase::Boot` the run was under the composed arm+trap predicate too, but
    // the sentence below is about the *replay* loop's ordering, and printing it unconditionally asserted
    // something the boot path had not checked.
    if t.phase == Phase::Replay {
        println!(
            "  (the trap predicate was checked first — the machine is not sitting at ErrorHandlerBlob, \
             so this is a genuine stall rather than a desync wearing a hang's clothes)"
        );
    }
}
