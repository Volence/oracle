//! End-to-end checks against the **real** artifacts Aeon's `sigil` build emits.
//!
//! The unit tests in the library encode the contract as we understand it; this file is the only place that
//! checks that understanding against the genuine `s4.debug.bin` / `s4.debug.lst` — which makes it the only
//! true end-to-end evidence available, and also the reason it cannot be a hard dependency.
//!
//! # Why every test here skips instead of failing
//!
//! These files live in a **different git repository** (`aeon/`), are build outputs (not checked in), and are
//! absent on CI and in any fresh clone. A test that failed when they were missing would make the suite red
//! for everyone who has not built Aeon locally. So each test resolves its inputs first and returns early
//! with a printed note when they are not there. `ORACLE_AEON_DIR` overrides the location. This mirrors
//! `crates/oracle-core/tests/symbols_real_lst.rs` exactly.
//!
//! # What runs by default, and what does not
//!
//! **Measured on this machine** (unoptimized test profile, which is what `cargo test` builds):
//!
//! | run | frames | wall-clock (debug) | wall-clock (release) |
//! |---|---|---|---|
//! | negative control | 34 to arm + 36 | **0.48 s** | 0.07 s |
//! | `ojz_fixture` green | 34 to arm + 1815 | **~34 s** | 3.7 s |
//! | `ojz_slide_fixture` green | 34 to arm + 2628 | **~49 s** | 5.5 s |
//!
//! So the two full playthroughs are `#[ignore]`d: ~83 s of unoptimized emulation does not belong in
//! `cargo test --workspace`. Run them deliberately with
//!
//! ```text
//! cargo test --release -p oracle-replay -- --ignored --nocapture
//! ```
//!
//! What stays in the default suite is not a thin substitute: the negative control drives the **entire**
//! pipeline — both refusals, symbol resolution, header parse from ROM, boot, arm, poke + read-back, the
//! frame loop, the exact-equality trap predicate, and the stack-frame fault decode — in half a second. It
//! simply reaches its verdict at `Logic_Tick 2` instead of 1723.

use oracle_replay::cli::Fixture;
use oracle_replay::fault::DESYNC_MESSAGE;
use oracle_replay::header::{ReplayHeader, REPLAY_ESCAPE, REPLAY_OP_END};
use oracle_replay::outcome::Shortfall;
use oracle_replay::policy;
use oracle_replay::runner::{
    self, Prepared, RunConfig, TimeoutReason, Verdict, NEGATIVE_CONTROL_PAYLOAD,
};
use std::path::PathBuf;

/// Where Aeon's build outputs live. Defaults to the sibling checkout this project is developed against.
fn aeon_dir() -> PathBuf {
    std::env::var("ORACLE_AEON_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/home/volence/sonic_hacks/aeon"))
}

fn artifact(name: &str) -> Option<Vec<u8>> {
    let p = aeon_dir().join(name);
    match std::fs::read(&p) {
        Ok(b) => Some(b),
        Err(_) => {
            println!(
                "SKIP: {} not present (aeon build outputs are not in this repo)",
                p.display()
            );
            None
        }
    }
}

fn listing(name: &str) -> Option<String> {
    artifact(name).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// The DEBUG pair, or `None` with a printed note.
fn debug_pair() -> Option<(Vec<u8>, String)> {
    let (Some(rom), Some(lst)) = (artifact("s4.debug.bin"), listing("s4.debug.lst")) else {
        return None;
    };
    Some((rom, lst))
}

fn prepared(fixture: Fixture) -> Option<Prepared> {
    let (rom, lst) = debug_pair()?;
    Some(Prepared::new(rom, &lst, fixture).expect("the DEBUG pair must prepare cleanly"))
}

/// The default run bounds, exercised as the shipped defaults rather than as test-only numbers.
fn default_config() -> RunConfig {
    RunConfig {
        max_frames: oracle_replay::cli::DEFAULT_MAX_FRAMES,
        stall_frames: oracle_replay::cli::DEFAULT_STALL_FRAMES,
    }
}

// ---------------------------------------------------------------------------------------------------
// Cheap checks — these run in the default suite.
// ---------------------------------------------------------------------------------------------------

/// Both refusals, against the genuine release/debug pair. This is the check that stands between the gate
/// and a false green, so it is asserted on real bytes rather than on synthetic ones.
#[test]
fn the_release_rom_is_refused_and_the_debug_rom_is_not() {
    let (Some(release), Some(debug)) = (artifact("s4.bin"), artifact("s4.debug.bin")) else {
        return;
    };
    assert!(
        !policy::has_desync_trap(&release),
        "s4.bin must not contain the DEBUG-only trap string, or the refusal proves nothing"
    );
    assert!(policy::require_debug_rom(&release).is_err());
    assert!(policy::has_desync_trap(&debug));
    assert!(policy::require_debug_rom(&debug).is_ok());
    println!("s4.bin refused as a release build; s4.debug.bin carries the compare path");
}

/// A listing from the other build shape must be refused before a single address is resolved: of the symbols
/// the two listings share, 92.6% name a different address, so every anchor would be confidently wrong.
#[test]
fn the_wrong_shape_listing_is_refused() {
    let (Some(dbg_rom), Some(rel_lst)) = (artifact("s4.debug.bin"), listing("s4.lst")) else {
        return;
    };
    let e = Prepared::new(dbg_rom, &rel_lst, Fixture::Ojz)
        .err()
        .expect("s4.lst must be refused against s4.debug.bin");
    assert!(e.contains("REFUSED"), "{e}");
    println!("s4.lst against s4.debug.bin: {e}");
}

/// Every anchor resolves by name, and the values are self-consistent: the RAM cells sit in work RAM, the
/// ROM ones inside the image. **No address is pinned** — every one of them moved in the last rebuild, which
/// is the entire reason this runner resolves by name.
#[test]
fn every_anchor_resolves_by_name() {
    let Some(p) = prepared(Fixture::Ojz) else {
        return;
    };
    let a = p.anchors;
    for (name, addr) in [
        ("Logic_Tick", a.logic_tick),
        ("Input_Source", a.input_source),
        ("Replay_Done", a.replay_done),
        ("Replay_Ptr", a.replay_ptr),
    ] {
        assert!(
            (0x00FF_0000..=0x00FF_FFFF).contains(&addr),
            "{name} resolved to ${addr:06X}, which is not work RAM"
        );
    }
    for (name, addr) in [
        ("GameState_OJZScroll_Init", a.init),
        ("Replay_OJZ_Fixture", a.fixture),
        ("ErrorHandlerBlob", a.error_handler),
    ] {
        assert!(
            (addr as usize) < p.rom.len(),
            "{name} resolved to ${addr:06X}, outside the {} byte image",
            p.rom.len()
        );
    }
    // The trap that motivated resolve-by-name: these four cells are distinct and tightly packed, and the
    // plans' table has `Replay_Done` sitting where `Replay_Ptr` now lives. If any two ever collide, the
    // runner would be polling the wrong byte.
    let mut cells = [a.logic_tick, a.input_source, a.replay_done, a.replay_ptr];
    cells.sort_unstable();
    for w in cells.windows(2) {
        assert_ne!(w[0], w[1], "two anchors resolved to the same address");
    }
    println!(
        "anchors: init=${:06X} fixture=${:06X} blob=${:06X} tick=${:06X} src=${:06X} done=${:06X} \
         ptr=${:06X}",
        a.init,
        a.fixture,
        a.error_handler,
        a.logic_tick,
        a.input_source,
        a.replay_done,
        a.replay_ptr
    );
}

/// Both fixtures parse out of the ROM image, and their tick counts are the ones
/// `aeon/tools/test_replay_fixture.py:28-30` pins (the `1496 / 24` in `replay_fixture.emp:27` is a stale
/// source comment). The counts are asserted because they are a property of the *recorded stream*, not of
/// the build layout, so unlike addresses they do not move on a rebuild.
#[test]
fn both_fixtures_parse_from_the_rom_image() {
    let Some((rom, lst)) = debug_pair() else {
        return;
    };
    for (fixture, ticks) in [(Fixture::Ojz, 1721), (Fixture::OjzSlide, 2350)] {
        let p = Prepared::new(rom.clone(), &lst, fixture).expect("must prepare");
        let h = p.header;
        assert_eq!(h.tick_count, ticks, "{fixture} tick count");
        assert_eq!(h.body, h.base + 20);
        assert_eq!(h.flags, 0);
        assert_eq!(h.rng_seed, 0);
        // Re-parsing straight from the bytes must agree — the header is read from the image, never from a
        // `.bin` on disk, so there is no second source to drift from.
        assert_eq!(ReplayHeader::parse(&p.rom, h.base), Ok(h));
        // The first checkpoint is where `--negative-control` plants its corruption.
        let at = h.first_checkpoint_payload(&p.rom).expect("a checkpoint");
        assert_eq!(
            at,
            h.body + 2,
            "{fixture}: the stream opens on a checkpoint"
        );
        println!(
            "{fixture}: base=${:06X} ticks={ticks} first checkpoint payload ${at:06X}",
            h.base
        );
    }
}

/// `core_hash` is stale metadata with zero consumers — the committed value is byte-identical in both
/// fixtures despite their having been recorded on different days against different builds. Asserted so that
/// nobody is ever tempted to build a build-identity guard on it, and so the day it *does* start moving is
/// visible rather than silent.
#[test]
fn core_hash_is_identical_in_both_fixtures_and_therefore_useless_as_a_guard() {
    let (Some(a), Some(b)) = (prepared(Fixture::Ojz), prepared(Fixture::OjzSlide)) else {
        return;
    };
    assert_ne!(
        a.header.tick_count, b.header.tick_count,
        "the two fixtures must differ, or this test proves nothing"
    );
    assert_eq!(
        a.header.core_hash, b.header.core_hash,
        "core_hash started differentiating the fixtures — re-read the design's 2.2 before relying on it"
    );
    println!(
        "core_hash ${:08X} is shared by a {}-tick and a {}-tick stream",
        a.header.core_hash, a.header.tick_count, b.header.tick_count
    );
}

/// **The negative control, end to end, in the default suite.** A gate you have never seen fail is not a
/// gate — and this one caught a real bug in its first run (A7 is a 32-bit register whose top byte the bus
/// does not carry, so a work-RAM range check on the raw value rejected a correctly-fired trap).
///
/// It drives the whole pipeline in ~0.5 s: refusals, resolution, header parse, boot, arm, poke + read-back,
/// the frame loop, the exact-equality trap predicate, and the `(A7)` fault decode.
#[test]
fn the_negative_control_trips_the_gate() {
    let Some(mut p) = prepared(Fixture::Ojz) else {
        return;
    };
    let (at, was) = p
        .corrupt_first_checkpoint()
        .expect("a checkpoint to corrupt");
    // `was` is read from the ROM, never pinned: it is the ring-0 hash of *this* build's curated state, and
    // a re-record moves it. What must hold is that the patch was a real change, and that the trap reports
    // this exact value back as `d0` (asserted below).
    assert_ne!(
        was, NEGATIVE_CONTROL_PAYLOAD,
        "the payload must differ from the sentinel, or the patch is a no-op"
    );

    let r = runner::run(&p, default_config()).expect("the run must complete");
    let Verdict::Trap(t) = &r.verdict else {
        panic!("the corrupted checkpoint must trap, got {:?}", r.verdict);
    };
    let f = t
        .fault()
        .unwrap_or_else(|| panic!("the trap frame must decode: {:?}", t.decoded));

    assert_eq!(f.message, DESYNC_MESSAGE);
    assert!(f.is_desync());
    let d = f.desync.expect("a desync carries its registers");
    assert_eq!(d.expected, NEGATIVE_CONTROL_PAYLOAD, "d2 = what we planted");
    assert_eq!(d.actual, was, "d0 = the hash the game really produced");
    // NOT `== 2`. `Logic_Tick = ring + 2` is fixture-specific and the design's own trap table
    // (`notes/…-restamp-ab.md:22`) singles it out as a thing never to hardcode: it is a property of where
    // the arm lands and how the packer split the runs, both of which a re-record moves. What is
    // load-bearing is that the trap happened during the replay at all.
    assert!(
        d.logic_tick > 0,
        "the desync must be reported at a real tick, got {}",
        d.logic_tick
    );

    // The raise site is decoded from the stack, not guessed: `(A7).l - 6` must land on a `jsr` opcode
    // ($4EB9 = jsr xxx.l) inside the image, and must name a symbol.
    assert_eq!(
        (
            p.rom[f.raise_site as usize],
            p.rom[f.raise_site as usize + 1]
        ),
        (0x4E, 0xB9),
        "the raise site must be the `jsr (MDDBG__ErrorHandler).l` itself"
    );
    let site = f
        .raise_site_symbol
        .as_deref()
        .expect("the raise site must resolve through the listing");
    assert!(
        site.contains("Input_Tick"),
        "the desync is raised inside Input_Tick, got `{site}`"
    );

    // And the judgement agrees.
    let why = runner::judge_negative_control(Some(f), NEGATIVE_CONTROL_PAYLOAD)
        .expect("the negative control must pass");
    println!("planted at ${at:06X}; {why}");
    println!("raised at ${:06X} ({site})", f.raise_site);
}

/// **The strongest test in this crate: a truncated stream must NOT report PASS.**
///
/// This is the failure the whole gate is built to prevent, and the one the negative control structurally
/// cannot catch. Splice `FF 00` (`REPLAY_OP_END`) in directly behind the ring-0 checkpoint of the real
/// fixture, in the real ROM image. The result passes every header check, arms cleanly, compares checkpoint
/// 0 — which *matches*, so no desync fires — reaches end-of-stream at tick 2 and sets `Replay_Done = $FF`
/// with `Input_Source` cleared and the cursor past the header, exactly as a green run does.
///
/// A PASS resting on `Replay_Done` alone reports exit 0 on that, having verified 1 checkpoint out of 27.
/// The negative control does not catch it: the negative control corrupts checkpoint 0, which this stream
/// still compares, so it would trip on this ROM just as happily.
///
/// It costs about what the negative control costs (it reaches its verdict at tick 2), so it runs by
/// default. What it asserts is a *relationship* — `Logic_Tick` fell short of the header's own declared
/// count — so nothing here moves when the fixture is re-recorded.
#[test]
fn a_truncated_stream_that_sets_replay_done_is_not_a_pass() {
    let Some(mut p) = prepared(Fixture::Ojz) else {
        return;
    };
    let h = p.header;
    let full_ticks = h.tick_count;

    // The real stream opens `FF 01 <4-byte hash> | 00 3F …`. Overwrite the first ordinary pair — the byte
    // right after the ring-0 checkpoint record — with the end-of-stream opcode.
    let end_at = (h.body + 6) as usize;
    let overwritten = [p.rom[end_at], p.rom[end_at + 1]];
    p.rom[end_at] = REPLAY_ESCAPE;
    p.rom[end_at + 1] = REPLAY_OP_END;
    assert_ne!(
        overwritten,
        [REPLAY_ESCAPE, REPLAY_OP_END],
        "the stream must not already end here, or this test proves nothing"
    );

    let r = runner::run(&p, default_config()).expect("the run must complete");

    let Verdict::Short(s) = &r.verdict else {
        panic!(
            "a stream truncated to one checkpoint must NOT be a PASS — got {:?}. The completion flag \
             alone cannot tell a full replay from a two-tick one.",
            r.verdict
        );
    };

    // The machine really did take the completion path: this is not a hang or a trap wearing a PASS's
    // clothes, which is exactly why a bare `Replay_Done` compare is fooled by it.
    assert_eq!(r.probe.replay_done, 0xFF, "Replay_Done IS set");
    assert!(r.corroborated(), "and Input_Source DID self-clear");
    assert!(
        !r.probe.stuck_in_header(r.anchors.fixture),
        "and the cursor DID leave the header — every other corroboration passes"
    );

    // The one thing that gives it away, and the shortfall that must be named.
    assert!(
        r.probe.logic_tick < full_ticks,
        "the truncated run must stop short of the {full_ticks} ticks the header declares, got {}",
        r.probe.logic_tick
    );
    assert_eq!(
        s.shortfalls,
        vec![Shortfall::TicksShort {
            logic_tick: r.probe.logic_tick,
            required: full_ticks
        }],
        "the failure must name what was short"
    );
    let said = s.shortfalls[0].to_string();
    assert!(said.contains("never replayed"), "{said}");

    println!(
        "truncated at ${end_at:06X} (was {overwritten:02X?}): Replay_Done=$FF at Logic_Tick {} of a \
         {full_ticks}-tick stream, {} frames after the arm — reported SHORT, not PASS",
        r.probe.logic_tick, s.frames
    );
    println!("  {said}");
}

/// **The stall watchdog's wiring**, which no test above the pure `Watchdog` unit level exercised: a stubbed
/// `observe()` that never returned `true` would have passed the entire suite.
///
/// A budget of 1 frame must fire almost immediately after the arm, because `GameState_OJZScroll_Init` is
/// the entry that loads the level and `Level_LoadArt`'s `VSync_Wait` spin runs inside that single dispatch
/// — the measured worst case on a *healthy* run is 34 consecutive frozen frames. Costs ~0.05 s.
#[test]
fn the_stall_watchdog_is_actually_wired_to_the_verdict() {
    let Some(p) = prepared(Fixture::Ojz) else {
        return;
    };
    let r = runner::run(
        &p,
        RunConfig {
            max_frames: oracle_replay::cli::DEFAULT_MAX_FRAMES,
            stall_frames: 1,
        },
    )
    .expect("the run must complete");

    let Verdict::Timeout(t) = &r.verdict else {
        panic!(
            "a 1-frame stall budget must wedge the run, got {:?}",
            r.verdict
        );
    };
    let TimeoutReason::Stalled { frozen_frames } = t.reason else {
        panic!(
            "the watchdog must be the reason, not the {} frame cap: {:?}",
            oracle_replay::cli::DEFAULT_MAX_FRAMES,
            t.reason
        );
    };
    assert!(frozen_frames >= 1, "the report must carry the count");
    assert!(
        t.frames < oracle_replay::cli::MEASURED_STALL_WORST_CASE + 10,
        "the watchdog must fire during the level-load spin, not hundreds of frames later: {} frames",
        t.frames
    );
    assert_eq!(t.phase, oracle_replay::runner::Phase::Replay);
    println!(
        "stall_frames=1 wedged {} frames after the arm ({frozen_frames} frozen) at Logic_Tick {}",
        t.frames,
        t.probe
            .expect("an armed timeout reports the cells")
            .logic_tick
    );
}

/// A ludicrously small frame cap must produce a TIMEOUT — with the trap checked first — rather than a
/// silent green. Cheap (it gives up almost immediately) and it is the only place the timeout path is
/// exercised on real artifacts.
#[test]
fn an_impossible_budget_times_out_rather_than_passing() {
    let Some(p) = prepared(Fixture::Ojz) else {
        return;
    };
    let r = runner::run(
        &p,
        RunConfig {
            max_frames: 60,
            stall_frames: oracle_replay::cli::DEFAULT_STALL_FRAMES,
        },
    )
    .expect("the run must complete");
    let Verdict::Timeout(t) = &r.verdict else {
        panic!(
            "a 60-frame cap cannot finish a 1721-tick stream, got {:?}",
            r.verdict
        );
    };
    assert_eq!(t.reason, TimeoutReason::Deadline);
    // The arm happened, so the report must show a cursor that has left the header — the runner's own
    // bad-arm signature must NOT be showing on a run that armed correctly.
    let probe = t.probe.expect("an armed timeout reports the cells");
    assert!(
        probe.stream_offset(p.anchors.fixture) >= 20,
        "the cursor is still inside the header — the arm failed"
    );
    println!(
        "timeout after {} frames at Logic_Tick {} (cursor at fixture+{})",
        t.frames,
        probe.logic_tick,
        probe.stream_offset(p.anchors.fixture)
    );
}

// ---------------------------------------------------------------------------------------------------
// The full playthroughs. Correct, but ~34 s and ~49 s unoptimized — see the module docs.
//     cargo test --release -p oracle-replay -- --ignored --nocapture
// ---------------------------------------------------------------------------------------------------

fn assert_green(fixture: Fixture, expected_ticks: u32) {
    let Some(p) = prepared(fixture) else {
        return;
    };
    let r = runner::run(&p, default_config()).expect("the run must complete");
    match &r.verdict {
        Verdict::Pass => {}
        other => panic!("{fixture} must pass, got {other:?}"),
    }
    assert!(
        r.corroborated(),
        "Input_Source must self-clear on completion"
    );
    assert_eq!(r.probe.replay_done, 0xFF);
    // `Logic_Tick` overshoots the header's count, because after end-of-stream the game keeps running on
    // live input. So this is a relationship, not a pin.
    assert!(
        r.probe.logic_tick >= expected_ticks,
        "{fixture}: Logic_Tick {} is below the {expected_ticks} the stream declares",
        r.probe.logic_tick
    );
    println!(
        "{fixture}: PASS — armed at frame {}, {} frames after the arm, Logic_Tick {} (stream declares \
         {expected_ticks})",
        r.frames_to_arm, r.frames_after_arm, r.probe.logic_tick
    );
}

#[test]
#[ignore = "full playthrough: ~34 s unoptimized. cargo test --release -p oracle-replay -- --ignored"]
fn the_standing_fixture_runs_green() {
    assert_green(Fixture::Ojz, 1721);
}

#[test]
#[ignore = "full playthrough: ~49 s unoptimized. cargo test --release -p oracle-replay -- --ignored"]
fn the_slide_fixture_runs_green() {
    assert_green(Fixture::OjzSlide, 2350);
}
