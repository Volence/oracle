//! Hand-rolled argument parsing.
//!
//! No `clap`, and deliberately: this crate's whole dependency list is `oracle-core`, and a gate artifact
//! another repo invokes by path should not drag a dependency tree behind it. The surface is a dozen flags,
//! and the parser's only cleverness is refusing combinations that would mislead — `--restamp` with
//! `--negative-control`, or an output flag on a run that will never write anything.

use std::fmt;
use std::path::PathBuf;

/// Which recorded stream to replay. Both are `embed()`ed in the DEBUG ROM and arm identically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fixture {
    /// The standing fixture: 1721 ticks, 27 checkpoints.
    #[default]
    Ojz,
    /// The longer slide fixture: 2350 ticks, 37 checkpoints.
    OjzSlide,
}

impl Fixture {
    /// The `.lst` symbol naming this fixture's base address.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Ojz => "Replay_OJZ_Fixture",
            Self::OjzSlide => "Replay_OJZ_Slide_Fixture",
        }
    }

    /// The `--fixture` spelling.
    pub fn cli_name(self) -> &'static str {
        match self {
            Self::Ojz => "ojz_fixture",
            Self::OjzSlide => "ojz_slide_fixture",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "ojz_fixture" => Some(Self::Ojz),
            "ojz_slide_fixture" => Some(Self::OjzSlide),
            _ => None,
        }
    }
}

impl fmt::Display for Fixture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.cli_name())
    }
}

/// Absolute cap on frames, per phase. **Measured**, not guessed, against `s4.debug.bin`: the arm lands at
/// frame 34, then the standing fixture completes in 1815 more frames and the slide fixture in 2628. 9000 is
/// ~3.4× the longer one. It is only a backstop — the progress watchdog below is the primary bound and fires
/// roughly 75× sooner on a real wedge.
pub const DEFAULT_MAX_FRAMES: u64 = 9000;

/// Frames `Logic_Tick` may stay frozen before the run is called wedged.
///
/// **Measured, and the measurement corrected an assumption.** The design expected the frames-without-ticks
/// phase to be *pre*-arm, where the watchdog never sees it. It is not: `GameState_OJZScroll_Init` is the
/// entry that loads the level, so `Level_LoadArt`'s `VSync_Wait` spin inside a single dispatch happens
/// immediately **after** the arm, at `Logic_Tick 1`. Sweeping the budget on both fixtures:
/// `--stall-frames 34` reports a false TIMEOUT on a green run and 35 passes, identically for both — so the
/// true worst case is 34 consecutive frozen frames, and any budget in the tens would have shipped a gate
/// that failed on healthy runs.
///
/// 240 frames (~4 s of emulated time) is ~7× that worst case, and still catches a genuine wedge in
/// milliseconds of wall-clock rather than at the end of the frame cap.
pub const DEFAULT_STALL_FRAMES: u64 = 240;

/// The measured worst case the default above must clear: 34 consecutive frames with `Logic_Tick` frozen,
/// on a **healthy** run, from `Level_LoadArt`'s `VSync_Wait` spin immediately after the arm. Swept on both
/// fixtures: `--stall-frames 34` reports a false TIMEOUT on a green run and 35 passes.
///
/// Named so the *relationship* can be enforced, rather than a constant being asserted against its own
/// literal (which is what the previous `assert_eq!(DEFAULT_STALL_FRAMES, 240)` did — it could never fail,
/// and it said nothing about why 240 is a defensible number).
pub const MEASURED_STALL_WORST_CASE: u64 = 34;

/// The invariant that makes [`DEFAULT_STALL_FRAMES`] defensible: it must clear the worst case a **healthy**
/// run really produces, or the gate reports false TIMEOUTs on green runs. Enforced at compile time, which
/// is strictly stronger than a test — the crate does not build if someone lowers the budget below the
/// measurement.
const _: () = assert!(DEFAULT_STALL_FRAMES > MEASURED_STALL_WORST_CASE);

/// Parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub rom: PathBuf,
    pub lst: PathBuf,
    pub fixture: Fixture,
    /// Corrupt one checkpoint payload and require the trap to fire — see [`crate::runner::judge_negative_control`].
    pub negative_control: bool,
    pub max_frames: u64,
    pub stall_frames: u64,
    /// Find **every** stale checkpoint in one pass and compute the repair — see [`crate::restamp`].
    pub restamp: bool,
    /// The committed fixture `.bin`, proven byte-identical to the copy embedded in the ROM. Without it the
    /// patch report still carries fixture-file offsets, but no re-stamped `.bin` can be emitted.
    pub fixture_bin: Option<PathBuf>,
    /// Where artifacts go. **`None` is the default and writes nothing at all** — the report goes to stdout
    /// and the run is a pure dry run.
    pub out: Option<PathBuf>,
    /// Also write the re-stamped ROM image. Off by default: the ROM is a regenerated build output, and
    /// handing one back invites running the stale image instead of rebuilding from the repaired fixture.
    pub emit_rom: bool,
    /// Permit `--out` to resolve inside the git repository the inputs came from. Those are owner artifacts.
    pub allow_source_write: bool,
    /// Permit overwriting files that already exist.
    pub force: bool,
}

/// What `parse` produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    Run(Box<Args>),
    /// `--help` was asked for; print [`USAGE`] and exit 0.
    Help,
}

pub const USAGE: &str = "\
replay_runner — headless runner for Aeon's embedded input-replay regression net

USAGE:
    replay_runner --rom <path> --lst <path> [options]

REQUIRED:
    --rom <path>            The DEBUG ROM image (e.g. s4.debug.bin). A release ROM is REFUSED: its
                            checkpoint compare is assembled out, so it would report a false green.
    --lst <path>            The sigil listing for that exact image. Every address is resolved by name
                            from it; a listing that does not bind to the image is REFUSED.

OPTIONS:
    --fixture <name>        ojz_fixture (default) | ojz_slide_fixture
    --negative-control      Corrupt one checkpoint payload to $DEADBEEF and REQUIRE the desync trap to
                            fire. Exit 0 when it does, non-zero when it does not (an inverted gate).
    --max-frames <n>        Absolute frame cap per phase [default: 9000]
    --stall-frames <n>      Frames Logic_Tick may stay frozen before the run is called wedged
                            [default: 240; measured worst case on a healthy run is 34]
    -h, --help              Print this help

RE-STAMPING:
    --restamp               Find EVERY stale checkpoint in ONE pass and compute the repair, instead of
                            stopping at the first. Replaces ~7 sequential playthroughs. Then re-runs the
                            re-stamped image clean end-to-end and re-runs the negative control on it, and
                            emits nothing unless both come back as they must.
                            **Dry run by default**: without --out, nothing is written anywhere.
    --fixture-bin <path>    The committed fixture .bin (e.g. games/sonic4/data/replays/ojz_fixture.bin).
                            Proven byte-identical to the copy embedded in the ROM before it is used.
                            Required to emit a re-stamped fixture; the artifact of record.
    --out <dir>             Write the patch report (and the re-stamped fixture, when --fixture-bin is
                            given) here. Omit for a pure dry run.
    --emit-rom              Also write the re-stamped ROM image into --out. Off by default — the ROM is a
                            build output, and the durable repair belongs in the fixture .bin.
    --allow-source-write    Permit --out to resolve INSIDE the git repository the ROM/listing came from.
                            Those are owner artifacts; writing into them is never the default.
    --force                 Permit overwriting files that already exist.

EXIT CODES:
    0  PASS (or, under --negative-control, the trap fired exactly as required; or, under --restamp,
       the repair was computed and verified)
    1  usage error, or a refusal (release ROM / mismatched listing / unresolved symbol / a stream
       --restamp will not vouch for / a write that would land somewhere it may not)
    2  DESYNC — a checkpoint mismatch
    3  FAULT — some other trap fired during the replay
    4  TIMEOUT — wedged, or the frame cap was reached
    5  the negative control did NOT trip: the gate is inverted and proves nothing
    6  SHORT COMPLETION — Replay_Done was set, but the run verified less than it claims (a
       truncated or mis-packed stream). This is the case a bare Replay_Done compare called a PASS.
    7  --restamp computed a repair, but the re-stamped image did not verify. Nothing was written.
";

/// Parse an argument list (excluding `argv[0]`).
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Parsed, String> {
    let mut rom: Option<PathBuf> = None;
    let mut lst: Option<PathBuf> = None;
    let mut fixture = Fixture::default();
    let mut negative_control = false;
    let mut max_frames = DEFAULT_MAX_FRAMES;
    let mut stall_frames = DEFAULT_STALL_FRAMES;
    let mut restamp = false;
    let mut fixture_bin: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut emit_rom = false;
    let mut allow_source_write = false;
    let mut force = false;

    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| -> Result<String, String> {
            it.next()
                .ok_or_else(|| format!("{flag} needs a value (see --help)"))
        };
        match arg.as_str() {
            "-h" | "--help" => return Ok(Parsed::Help),
            "--rom" => rom = Some(PathBuf::from(value("--rom")?)),
            "--lst" => lst = Some(PathBuf::from(value("--lst")?)),
            "--fixture" => {
                let v = value("--fixture")?;
                fixture = Fixture::parse(&v).ok_or_else(|| {
                    format!("unknown fixture `{v}` — expected ojz_fixture or ojz_slide_fixture")
                })?;
            }
            "--negative-control" => negative_control = true,
            "--max-frames" => max_frames = count("--max-frames", &value("--max-frames")?)?,
            "--stall-frames" => stall_frames = count("--stall-frames", &value("--stall-frames")?)?,
            "--restamp" => restamp = true,
            "--fixture-bin" => fixture_bin = Some(PathBuf::from(value("--fixture-bin")?)),
            "--out" => out = Some(PathBuf::from(value("--out")?)),
            "--emit-rom" => emit_rom = true,
            "--allow-source-write" => allow_source_write = true,
            "--force" => force = true,
            other => return Err(format!("unexpected argument `{other}` (see --help)")),
        }
    }

    // The negative control *plants* a stale checkpoint; a re-stamp *repairs* stale checkpoints. Running
    // both would have the tool dutifully re-stamping its own corruption back out and reporting a repair
    // that repairs nothing. Refused rather than given a precedence rule nobody would remember.
    if restamp && negative_control {
        return Err(
            "--restamp and --negative-control are mutually exclusive: the negative control plants a \
             stale checkpoint, which the re-stamp would then 'repair', so the pair proves nothing. \
             (--restamp runs the negative control against the re-stamped image on its own.)"
                .into(),
        );
    }
    for (flag, given) in [
        ("--fixture-bin", fixture_bin.is_some()),
        ("--out", out.is_some()),
        ("--emit-rom", emit_rom),
        ("--allow-source-write", allow_source_write),
        ("--force", force),
    ] {
        if given && !restamp {
            return Err(format!(
                "{flag} only means something with --restamp; without it this run writes nothing and \
                 computes no repair"
            ));
        }
    }
    if emit_rom && out.is_none() {
        return Err("--emit-rom needs --out to say where (see --help)".into());
    }

    Ok(Parsed::Run(Box::new(Args {
        rom: rom.ok_or("--rom is required (see --help)")?,
        lst: lst.ok_or("--lst is required (see --help)")?,
        fixture,
        negative_control,
        max_frames,
        stall_frames,
        restamp,
        fixture_bin,
        out,
        emit_rom,
        allow_source_write,
        force,
    })))
}

/// A positive frame count. Zero is rejected rather than silently meaning "give up immediately".
fn count(flag: &str, v: &str) -> Result<u64, String> {
    match v.parse::<u64>() {
        Ok(0) | Err(_) => Err(format!("{flag} must be a positive whole number, got `{v}`")),
        Ok(n) => Ok(n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    fn run(s: &[&str]) -> Result<Args, String> {
        match parse(argv(s))? {
            Parsed::Run(a) => Ok(*a),
            Parsed::Help => Err("help".into()),
        }
    }

    #[test]
    fn the_minimal_invocation_defaults_to_the_standing_fixture() {
        let a = run(&["--rom", "s4.debug.bin", "--lst", "s4.debug.lst"]).unwrap();
        assert_eq!(a.rom, PathBuf::from("s4.debug.bin"));
        assert_eq!(a.lst, PathBuf::from("s4.debug.lst"));
        assert_eq!(a.fixture, Fixture::Ojz);
        assert_eq!(a.fixture.symbol(), "Replay_OJZ_Fixture");
        assert!(!a.negative_control);
        assert_eq!(a.max_frames, DEFAULT_MAX_FRAMES);
        assert_eq!(a.stall_frames, DEFAULT_STALL_FRAMES);
        // The property that matters — `DEFAULT_STALL_FRAMES > MEASURED_STALL_WORST_CASE` — is a
        // relationship between two constants, so it is enforced at compile time next to them rather than
        // re-stated here. (The line this replaces asserted `DEFAULT_STALL_FRAMES == 240`, i.e. a constant
        // against its own literal, which could never fail.)
    }

    #[test]
    fn every_flag_parses() {
        let a = run(&[
            "--rom",
            "r",
            "--lst",
            "l",
            "--fixture",
            "ojz_slide_fixture",
            "--negative-control",
            "--max-frames",
            "77",
            "--stall-frames",
            "9",
        ])
        .unwrap();
        assert_eq!(a.fixture, Fixture::OjzSlide);
        assert_eq!(a.fixture.symbol(), "Replay_OJZ_Slide_Fixture");
        assert!(a.negative_control);
        assert_eq!(a.max_frames, 77);
        assert_eq!(a.stall_frames, 9);
    }

    #[test]
    fn both_paths_are_required() {
        assert!(run(&["--lst", "l"]).unwrap_err().contains("--rom"));
        assert!(run(&["--rom", "r"]).unwrap_err().contains("--lst"));
    }

    #[test]
    fn bad_input_is_rejected_rather_than_defaulted() {
        assert!(run(&["--rom"]).unwrap_err().contains("needs a value"));
        assert!(run(&["--rom", "r", "--lst", "l", "--fixture", "nope"])
            .unwrap_err()
            .contains("unknown fixture"));
        assert!(run(&["--rom", "r", "--lst", "l", "--max-frames", "0"])
            .unwrap_err()
            .contains("positive"));
        assert!(run(&["--rom", "r", "--lst", "l", "--stall-frames", "x"])
            .unwrap_err()
            .contains("positive"));
        assert!(run(&["--rom", "r", "--lst", "l", "extra"])
            .unwrap_err()
            .contains("unexpected"));
    }

    #[test]
    fn restamp_defaults_to_a_dry_run_that_writes_nothing() {
        let a = run(&["--rom", "r", "--lst", "l", "--restamp"]).unwrap();
        assert!(a.restamp);
        assert_eq!(a.out, None, "no --out means nothing is written anywhere");
        assert_eq!(a.fixture_bin, None);
        assert!(!a.emit_rom);
        assert!(!a.allow_source_write);
        assert!(!a.force);
    }

    #[test]
    fn every_restamp_flag_parses() {
        let a = run(&[
            "--rom",
            "r",
            "--lst",
            "l",
            "--restamp",
            "--fixture-bin",
            "f.bin",
            "--out",
            "/tmp/o",
            "--emit-rom",
            "--allow-source-write",
            "--force",
        ])
        .unwrap();
        assert_eq!(a.fixture_bin, Some(PathBuf::from("f.bin")));
        assert_eq!(a.out, Some(PathBuf::from("/tmp/o")));
        assert!(a.emit_rom && a.allow_source_write && a.force);
    }

    /// The negative control plants a stale checkpoint; the re-stamp repairs stale checkpoints. Together
    /// the tool would "repair" its own corruption and report a repair that repaired nothing.
    #[test]
    fn restamp_and_the_negative_control_are_mutually_exclusive() {
        let e = run(&[
            "--rom",
            "r",
            "--lst",
            "l",
            "--restamp",
            "--negative-control",
        ])
        .unwrap_err();
        assert!(e.contains("mutually exclusive"), "{e}");
    }

    /// A write flag without `--restamp` is a user who believes something will be written. Say so rather
    /// than accepting it silently.
    #[test]
    fn output_flags_without_restamp_are_a_usage_error() {
        for flag in [
            "--emit-rom",
            "--allow-source-write",
            "--force",
            "--fixture-bin",
        ] {
            let mut argv = vec!["--rom", "r", "--lst", "l", flag];
            if flag == "--fixture-bin" {
                argv.push("f.bin");
            }
            let e = run(&argv).unwrap_err();
            assert!(
                e.contains("only means something with --restamp"),
                "{flag}: {e}"
            );
        }
        let e = run(&["--rom", "r", "--lst", "l", "--out", "/tmp/o"]).unwrap_err();
        assert!(e.contains("only means something with --restamp"), "{e}");
    }

    #[test]
    fn emitting_a_rom_needs_somewhere_to_put_it() {
        let e = run(&["--rom", "r", "--lst", "l", "--restamp", "--emit-rom"]).unwrap_err();
        assert!(e.contains("--out"), "{e}");
    }

    #[test]
    fn help_short_circuits() {
        assert_eq!(parse(argv(&["--help"])).unwrap(), Parsed::Help);
        assert_eq!(parse(argv(&["--rom", "r", "-h"])).unwrap(), Parsed::Help);
    }
}
