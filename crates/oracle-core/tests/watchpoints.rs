//! Integration proof for the bus-level recording watchpoints: register a watch on a work-RAM address, run a
//! real frame on a real [`System`] with the [`Watchpoints`] attached as the bus-event sink, and confirm each
//! hit is attributed to the exact instruction (PC) and master (function code) that drove the access.
//!
//! The fixture is `testrom::build`: after boot it stirs the first `$4000` work-RAM words (`$FF0000..$FF7FFE`)
//! with `move.w D0,(A0)+` at PC `$212`, each preceded by a `move.w (A0),D0` read at PC `$20E`. One stir pass
//! is far longer than one frame (~3.4M mclk vs ~896k), so within the first frame `$FF0000` is read exactly
//! once (at `$20E`) and written exactly once (at `$212`) — a clean single-hit assertion. Work RAM is
//! randomized at power-on, so the read returns the power-on word `V` and the store writes `V + 1`.

use oracle_core::bus::{BusEventSink, BusOp, Fanout, Size};
use oracle_core::system::System;
use oracle_core::watchpoints::{AddrParity, WatchOp, WatchSpace, WatchVia, Watchpoints};

/// A booted machine running the stir-RAM fixture ROM.
fn booted() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    sys
}

/// The first stirred work-RAM word — written once, early, in frame 0.
const WATCH_ADDR: u32 = 0x00FF_0000;
/// The `move.w D0,(A0)+` that stores the stirred word.
const STORE_PC: u32 = 0x0000_0212;
/// The `move.w (A0),D0` that reads it back each pass.
const LOAD_PC: u32 = 0x0000_020E;

/// The big-endian word currently at `$FF0000` (work RAM is randomized at power-on, so this is a
/// deterministic-but-nonzero value; the fixture reads it and stores it + 1).
fn ff0000_word(sys: &System) -> u32 {
    ((sys.ram()[0] as u32) << 8) | sys.ram()[1] as u32
}

#[test]
fn write_watch_attributes_the_hit_to_the_storing_instruction() {
    let mut sys = booted();
    let initial = ff0000_word(&sys); // the fixture stores initial + 1 on the one pass that touches $FF0000
    let mut wp = Watchpoints::new(64);
    wp.add_watch(WATCH_ADDR..=WATCH_ADDR + 1, WatchOp::Write, "ff0000");
    sys.run_frames_with_sink(1, &mut wp);

    assert_eq!(wp.dropped(), 0, "no hits dropped");
    assert_eq!(
        wp.hits().len(),
        1,
        "exactly one write to $FF0000 in frame 0"
    );
    let hit = wp.hits()[0];
    assert_eq!(hit.addr, WATCH_ADDR, "watched address");
    assert_eq!(
        hit.value,
        (initial + 1) & 0xFFFF,
        "stored the read word + 1"
    );
    assert_eq!(hit.size, Size::Word, "move.w");
    assert_eq!(hit.op, BusOp::Write);
    assert_eq!(hit.fc, 5, "supervisor data space (CPU master)");
    assert_eq!(hit.pc, STORE_PC, "attributed to the storing instruction");
    assert_eq!(hit.frame, 0, "in the first frame");
    assert_eq!(hit.seq, 0, "first recorded hit");
}

#[test]
fn read_watch_attributes_the_hit_to_the_loading_instruction() {
    let mut sys = booted();
    let initial = ff0000_word(&sys);
    let mut wp = Watchpoints::new(64);
    wp.add_watch(WATCH_ADDR..=WATCH_ADDR + 1, WatchOp::Read, "ff0000");
    sys.run_frames_with_sink(1, &mut wp);

    assert_eq!(wp.hits().len(), 1, "exactly one read of $FF0000 in frame 0");
    let hit = wp.hits()[0];
    assert_eq!(hit.op, BusOp::Read);
    assert_eq!(hit.value, initial, "read the pre-increment word");
    assert_eq!(hit.fc, 5, "supervisor data space (CPU master)");
    assert_eq!(hit.pc, LOAD_PC, "attributed to the loading instruction");
}

#[test]
fn a_watch_outside_any_touched_address_never_hits() {
    // $FFC000 is above the stirred range ($FF0000..$FF7FFE), below the handler sentinels/stack — untouched.
    let mut sys = booted();
    let mut wp = Watchpoints::new(64);
    wp.add_watch(0x00FF_C000..=0x00FF_C0FF, WatchOp::Any, "untouched");
    sys.run_frames_with_sink(2, &mut wp);
    assert_eq!(
        wp.hits().len(),
        0,
        "no access ever lands in the watched range"
    );
    assert_eq!(wp.dropped(), 0);
}

#[test]
fn function_code_distinguishes_program_from_data_space() {
    // The CPU fetches instructions from ROM in program space (fc 6) and reads/writes RAM in data space
    // (fc 5). A watch over the inner-loop opcodes ($20E..$217) sees the prefetch reads carry fc 6, proving
    // the function code carries the real 68000 space (the mechanism DMA would use to report fc 0).
    let mut sys = booted();
    let mut wp = Watchpoints::new(256);
    wp.add_watch(
        0x0000_020E..=0x0000_0217,
        WatchOp::Read,
        "inner-loop opcodes",
    );
    sys.run_frames_with_sink(1, &mut wp);
    assert!(!wp.hits().is_empty(), "the inner loop is fetched");
    assert!(
        wp.hits().iter().all(|h| h.fc == 6),
        "instruction prefetch reads are program space (fc 6)"
    );
}

// --- VDP-internal watches (v2): who wrote this tile / palette entry? -------------------------------------

use oracle_core::testrom;

/// A direct CPU data-port write to VRAM is captured byte-granular and attributed to the poking instruction.
#[test]
fn vram_watch_attributes_a_direct_poke_to_the_writing_instruction() {
    let mut sys = System::new(0x5EED);
    sys.load_rom(testrom::build_vram_poke());
    sys.reset();
    let old_hi = sys.vram()[testrom::VRAM_POKE_ADDR as usize];
    let old_lo = sys.vram()[testrom::VRAM_POKE_ADDR as usize + 1];

    let mut wp = Watchpoints::new(64);
    wp.add_vdp_watch(
        WatchSpace::Vram,
        testrom::VRAM_POKE_ADDR..=testrom::VRAM_POKE_ADDR + 1,
        WatchOp::Write,
        "poke",
    );
    sys.run_frames_with_sink(1, &mut wp);

    assert_eq!(wp.dropped(), 0, "no hits dropped");
    assert_eq!(wp.hits().len(), 2, "one direct VRAM word = two byte writes");
    let hi = wp.hits()[0];
    assert_eq!(hi.space, WatchSpace::Vram);
    assert_eq!(hi.addr, testrom::VRAM_POKE_ADDR, "resolved VRAM address");
    assert_eq!(hi.old, old_hi as u32, "pre-write byte");
    assert_eq!(hi.value, (testrom::VRAM_POKE_WORD >> 8) as u32, "$BE");
    assert_eq!(hi.size, Size::Byte);
    assert_eq!(hi.op, BusOp::Write);
    assert_eq!(hi.via, WatchVia::Direct, "a direct CPU data-port write");
    assert_eq!(hi.pc, testrom::VRAM_POKE_PC, "attributed to the poke");
    assert_eq!(hi.frame, 0);
    let lo = wp.hits()[1];
    assert_eq!(lo.addr, testrom::VRAM_POKE_ADDR + 1);
    assert_eq!(lo.old, old_lo as u32);
    assert_eq!(lo.value, (testrom::VRAM_POKE_WORD & 0xFF) as u32, "$EF");
    assert_eq!(lo.via, WatchVia::Direct);
    assert_eq!(lo.pc, testrom::VRAM_POKE_PC);
}

/// A VRAM watch catches a *DMA* write (the pad-poll fixture zeroes VRAM with a fill DMA) with `via = Dma`,
/// attributed to the instruction that triggered the transfer.
#[test]
fn vram_watch_catches_a_dma_fill_write_with_via_dma() {
    let rom = testrom::build_pad_poll();
    let mut sys = System::new(0x1234);
    sys.load_rom(rom.clone());
    sys.reset();
    let old = sys.vram()[0x0100];

    let mut wp = Watchpoints::new(64);
    wp.add_vdp_watch(WatchSpace::Vram, 0x0100..=0x0100, WatchOp::Write, "tile");
    sys.run_frames_with_sink(1, &mut wp);

    assert_eq!(wp.hits().len(), 1, "the fill writes $0100 exactly once");
    let hit = wp.hits()[0];
    assert_eq!(hit.space, WatchSpace::Vram);
    assert_eq!(hit.addr, 0x0100);
    assert_eq!(hit.old, old as u32, "pre-fill byte");
    assert_eq!(hit.value, 0x00, "the fill byte is $00");
    assert_eq!(hit.via, WatchVia::Dma, "driven by DMA");
    // The PC attributes to the data-port write that triggered the fill (opcode `move.w #imm,(a1)` = $32BC).
    let op = ((rom[hit.pc as usize] as u16) << 8) | rom[hit.pc as usize + 1] as u16;
    assert_eq!(op, 0x32BC, "triggering instruction is the data-port write");
}

/// A CRAM watch captures a direct palette write as a word with `via = Direct`.
#[test]
fn cram_watch_captures_a_direct_palette_write() {
    let rom = testrom::build_pad_poll();
    let mut sys = System::new(0x1234);
    sys.load_rom(rom.clone());
    sys.reset();

    // CRAM entry 1 (byte address $02) is written once = $0EEE (the "white" backdrop colour).
    let mut wp = Watchpoints::new(64);
    wp.add_vdp_watch(WatchSpace::Cram, 0x02..=0x03, WatchOp::Any, "palette-1");
    sys.run_frames_with_sink(1, &mut wp);

    assert_eq!(wp.hits().len(), 1, "CRAM entry 1 written exactly once");
    let hit = wp.hits()[0];
    assert_eq!(hit.space, WatchSpace::Cram);
    assert_eq!(hit.addr, 0x02, "resolved CRAM byte address");
    assert_eq!(hit.old, 0x0000, "CRAM starts zeroed");
    assert_eq!(hit.value, 0x0EEE, "9-bit white");
    assert_eq!(hit.size, Size::Word, "a CRAM entry is a word");
    assert_eq!(hit.via, WatchVia::Direct);
}

/// A bus watch attached to the same run never sees VDP-internal writes, and vice versa — capture only arms
/// when a VDP watch is registered, and spaces do not cross.
#[test]
fn a_bus_only_watch_does_not_arm_vdp_capture() {
    let mut sys = System::new(0x5EED);
    sys.load_rom(testrom::build_vram_poke());
    sys.reset();
    // A bus watch numerically overlapping VRAM addresses ($0100) must not pick up the VDP-internal write.
    let mut wp = Watchpoints::new(64);
    wp.add_watch(0x0000_0100..=0x0000_0101, WatchOp::Write, "bus-100");
    assert!(
        !wp.wants_vdp_writes(),
        "a bus-only watch never arms VDP capture"
    );
    sys.run_frames_with_sink(1, &mut wp);
    assert_eq!(
        wp.hits().len(),
        0,
        "the VDP-internal poke is not a bus write at $0100"
    );
}

// --- The trace recorder (2026-08-14): mclk, determinism, stop-after, census -------------------------------

use oracle_core::system::StopReason;
use oracle_core::watchpoints::{CensusKey, Watch, WatchMode};

/// **T5 — two runs of the same ROM, input and power-on seed produce byte-identical hit sequences.**
///
/// This is the property the whole instrument's credibility rests on: every comparison in the corpus that
/// mattered was a diff between two traces, and a diff is only evidence if a re-run reproduces itself. It
/// falls out of the emulator's determinism (recon C2), but it is asserted rather than assumed — and it is
/// asserted over the *whole* `WatchHit`, which is why the type stays `Copy + Eq`.
#[test]
fn two_identical_runs_produce_byte_identical_hit_sequences() {
    let trace = || {
        let mut sys = booted();
        let mut wp = Watchpoints::new(4096);
        wp.add_watch(WATCH_ADDR..=WATCH_ADDR + 0xFF, WatchOp::Any, "stir");
        sys.run_frames_with_sink(2, &mut wp);
        wp.take_hits()
    };
    let a = trace();
    let b = trace();
    assert!(!a.is_empty(), "the fixture drives accesses in the window");
    assert_eq!(a, b, "the same run must trace identically, hit for hit");
}

/// The master clock reaches a hit from the bus's own `on_event_at`: it is non-zero on a real run, never
/// decreases, and agrees with the frame the hit was stamped with.
#[test]
fn hits_carry_a_monotonic_master_clock_consistent_with_the_frame() {
    let mut sys = booted();
    let mut wp = Watchpoints::new(4096);
    wp.add_watch(WATCH_ADDR..=WATCH_ADDR + 0xFF, WatchOp::Any, "stir");
    sys.run_frames_with_sink(2, &mut wp);
    // The frame length comes from the report's own timing basis (F-TRACE-PAL), not from a copy of 896_040
    // pasted into the test — a reader who has to look the number up is the failure mode the basis prevents.
    let mclk_per_frame = wp.timing_basis().mclk_per_frame;

    let hits = wp.hits();
    assert!(!hits.is_empty());
    assert!(hits[0].mclk > 0, "a real bus access carries its own clock");
    assert!(
        hits.windows(2).all(|w| w[0].mclk <= w[1].mclk),
        "the clock never runs backwards across the hit log"
    );
    assert!(
        hits.iter().all(|h| h.mclk / mclk_per_frame == h.frame),
        "mclk and the step-boundary frame stamp name the same instant"
    );
    assert!(wp.seen() > wp.matched(), "the filter rejected most traffic");
    assert!(
        wp.caveats().is_empty(),
        "a plain bus watch has nothing to caveat: {:?}",
        wp.caveats()
    );
}

/// **`stop_after` composes with the S1 stop signal**: a watch that has fired N times ends the run at the next
/// instruction boundary, with `StopReason::SinkRequested` — "run until X happens" instead of a hand-tuned
/// frame budget. The bound is still honoured if it never fires.
#[test]
fn stop_after_ends_the_run_at_the_watch() {
    let mut sys = booted();
    let mut wp = Watchpoints::new(64);
    let id =
        wp.add(Watch::bus(WATCH_ADDR..=WATCH_ADDR + 0xFF, WatchOp::Write, "stir").stop_after(3));
    let record = sys.run_frames_with_sink(60, &mut wp);

    assert_eq!(
        record.reason,
        StopReason::SinkRequested,
        "the watch ended the run, long before the 60-frame bound"
    );
    assert!(record.frame < 60, "stopped early (frame {})", record.frame);
    // The flag is raised mid-instruction and honoured at the next boundary, so the triggering access has
    // committed: exactly the threshold, never fewer.
    assert_eq!(wp.watch(id).unwrap().matched, 3);
    assert_eq!(wp.hits().len(), 3);

    // A threshold the run never reaches degrades to the plain bounded run.
    let mut sys = booted();
    let mut never = Watchpoints::new(64);
    never.add(Watch::bus(0xFF_C000..=0xFF_C0FF, WatchOp::Any, "untouched").stop_after(1));
    let record = sys.run_frames_with_sink(1, &mut never);
    assert_eq!(record.reason, StopReason::DeadlineReached);
}

/// A `Census` over a real run answers the shape of question that retracted two recorded root causes: *which*
/// destinations does this ROM touch, and how many distinct ones are there — with the log itself switched off.
#[test]
fn a_census_over_a_real_run_reports_distinct_destinations() {
    let mut sys = booted();
    let mut wp = Watchpoints::new(0); // pure aggregate: nothing stored at all
    let id = wp.add(
        Watch::bus(WATCH_ADDR..=WATCH_ADDR + 0x1F, WatchOp::Write, "stir")
            .mode(WatchMode::Census(CensusKey::Addr)),
    );
    sys.run_frames_with_sink(2, &mut wp);

    let r = wp.watch(id).unwrap();
    assert_eq!(wp.hits().len(), 0, "a census stores no hits");
    assert!(r.matched > 0, "but it counted the accesses");
    assert!(!r.keys_capped, "well inside the default 256-key cap");
    assert_eq!(
        r.distinct_keys,
        r.census.as_ref().unwrap().len() as u64,
        "the cardinality is the census's own key count"
    );
    // The fixture stirs consecutive words: every key is even and inside the watched window.
    for (k, n) in r.census.unwrap() {
        assert_eq!(k % 2, 0, "word writes land on even addresses");
        assert!((WATCH_ADDR as u64..=WATCH_ADDR as u64 + 0x1F).contains(&k));
        assert!(n > 0);
    }
    assert_eq!(r.first.map(|s| s.seq), Some(0), "first matched access");
    assert_eq!(r.last.map(|s| s.seq), Some(r.matched - 1));
}

/// **C1, with a real instrument.** `System::boot_with_sink` arms a `Watchpoints` *for the reset itself*, so
/// the vector fetches — the first accesses of the machine's life, invisible to every sink in this tree until
/// 2026-08-14 — are recordable by the same facility used for everything else, not just by a `Vec<BusEvent>`.
#[test]
fn a_watchpoint_can_be_armed_at_power_on() {
    let mut wp = Watchpoints::new(64);
    let id = wp.add_watch(0x0..=0x7, WatchOp::Read, "reset vectors");
    let sys = System::boot_with_sink(0x5EED, oracle_core::testrom::build(), &mut wp);

    let hits = wp.hits();
    assert_eq!(
        hits.len(),
        4,
        "the four reset-vector reads reach a watch armed at power-on: {hits:?}"
    );
    assert_eq!(
        hits.iter().map(|h| h.addr).collect::<Vec<_>>(),
        vec![0x0, 0x2, 0x4, 0x6]
    );
    assert!(
        hits.iter().all(|h| h.fc == 6 && h.op == BusOp::Read),
        "the reset vector is fetched from supervisor PROGRAM space"
    );
    // No instruction drives a reset, so the PC attribution is 0 — the honest answer, not a lost stamp.
    assert!(hits.iter().all(|h| h.pc == 0));
    assert_eq!(wp.watch(id).unwrap().matched, 4);
    // The capture describes the boot that actually happened.
    assert!(wp.seen() > 4, "and it saw the prefetches too");
    assert!(!sys.is_pristine_power_on(), "the machine did come up");
}

/// `F-TRACE-PAL`: a trace's `frame` stamps are only interpretable with the basis that produced them, so the
/// report carries it — and it must be the *machine's* basis, not a second opinion. This is the assertion that
/// fails the day a PAL machine is stamped with an NTSC report.
#[test]
fn the_trace_report_carries_the_machines_own_timing_basis() {
    let mut sys = booted();
    let mut wp = Watchpoints::new(16);
    wp.add_watch(WATCH_ADDR..=WATCH_ADDR, WatchOp::Any, "stir");
    sys.run_frames_with_sink(1, &mut wp);

    let basis = wp.timing_basis();
    assert_eq!(
        basis,
        sys.timing_basis(),
        "the report's basis is the machine's basis"
    );
    assert_eq!(basis.standard.as_str(), "ntsc");
    assert!(!wp.hits().is_empty(), "there was something to check");
    // Every hit's `frame` is its `mclk` divided by the reported frame length — the basis is the arithmetic
    // the stamps were made with, not a decoration next to them.
    for h in wp.hits() {
        assert_eq!(h.frame, h.mclk / basis.mclk_per_frame);
    }
}

// --- F-TRACE-SIZEFILTER: the size + address-parity filters, on a real machine ----------------------------

/// `K4Probe`'s read-arm classifier, copied from `examples/k4_openbus_probe.rs` (outer `Read | Tas` gate
/// included) so the comparison below is against the real hand-rolled counters, not a paraphrase.
#[derive(Default)]
struct K4Counters {
    io_even_byte_reads: u64,
    io_word_reads: u64,
    status_upper_reads: u64,
    status_odd_byte_reads: u64,
    io_reads_total: u64,
}

impl K4Counters {
    fn classify(&mut self, e: &oracle_core::bus::BusEvent) {
        if !matches!(e.op, BusOp::Read | BusOp::Tas) {
            return;
        }
        match e.addr {
            0xA1_0000..=0xA1_001F => {
                self.io_reads_total += 1;
                match e.size {
                    Size::Byte if e.addr & 1 == 0 => self.io_even_byte_reads += 1,
                    Size::Word => self.io_word_reads += 1,
                    _ => {}
                }
            }
            0xC0_0004..=0xC0_0007 => {
                if e.size == Size::Word || e.addr & 1 == 0 {
                    self.status_upper_reads += 1;
                } else {
                    self.status_odd_byte_reads += 1;
                }
            }
            _ => {}
        }
    }
}

/// The counters are a sink in their own right, so `Fanout` (not a hand-rolled composite) feeds them and the
/// `Watchpoints` from **one** event stream — the comparison below cannot be confounded by two runs.
impl BusEventSink for K4Counters {
    fn on_event(&mut self, event: oracle_core::bus::BusEvent) {
        self.classify(&event);
    }
}

/// A booted machine running the pad-poll fixture, whose loop reads `$A10003` — a **byte** access at an
/// **odd** address in the I/O range, i.e. real traffic the two new filters must discriminate.
fn booted_pad_poll() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build_pad_poll());
    sys.reset();
    sys
}

/// Three of `K4Probe`'s four motivating counters, expressed as watch configuration, agree with the
/// hand-rolled originals over a real run of a real machine — and the agreement is not vacuous: a filterless
/// watch over the same range counts hundreds of accesses, every one of which the *odd*-parity byte watch
/// claims and the *even*-parity byte watch correctly refuses.
#[test]
fn k4_io_counters_as_watch_config_agree_with_the_hand_rolled_counters() {
    let mut sys = booted_pad_poll();
    let mut hand = K4Counters::default();
    let mut wp = Watchpoints::new(0);
    const IO: std::ops::RangeInclusive<u32> = 0xA1_0000..=0xA1_001F;
    const STATUS: std::ops::RangeInclusive<u32> = 0xC0_0004..=0xC0_0007;
    let io_even_byte = wp.add(
        Watch::bus(IO, WatchOp::Read, "io_even_byte_reads")
            .size(Size::Byte)
            .addr_parity(AddrParity::Even)
            .mode(WatchMode::Count),
    );
    let io_word = wp.add(
        Watch::bus(IO, WatchOp::Read, "io_word_reads")
            .size(Size::Word)
            .mode(WatchMode::Count),
    );
    let status_odd_byte = wp.add(
        Watch::bus(STATUS, WatchOp::Read, "status_odd_byte_reads")
            .size(Size::Byte)
            .addr_parity(AddrParity::Odd)
            .mode(WatchMode::Count),
    );
    // Controls: every I/O read, and every I/O read that is an odd-address byte.
    let io_any = wp.add(Watch::bus(IO, WatchOp::Read, "io.any").mode(WatchMode::Count));
    let io_odd_byte = wp.add(
        Watch::bus(IO, WatchOp::Read, "io.odd.byte")
            .size(Size::Byte)
            .addr_parity(AddrParity::Odd)
            .mode(WatchMode::Count),
    );

    let mut both = Fanout::new(&mut hand, &mut wp);
    sys.run_frames_with_sink(2, &mut both);

    let of = |id| wp.watch(id).unwrap().matched;
    assert!(wp.seen() > 10_000, "a live instrument: {}", wp.seen());
    // The negative control comes first: the range really is busy.
    let io_total = of(io_any);
    assert!(io_total > 0, "the pad-poll loop reads the I/O range");
    assert_eq!(io_total, hand.io_reads_total, "same stream, same total");
    assert_eq!(
        of(io_odd_byte),
        io_total,
        "every I/O read this ROM makes is an odd-address byte ($A10003)"
    );
    // ...so the even-parity filter must refuse all of them, and it agrees with the probe that it should.
    assert_eq!(of(io_even_byte), hand.io_even_byte_reads);
    assert_eq!(
        of(io_even_byte),
        0,
        "no even-address byte read, per the ROM"
    );
    assert_eq!(of(io_word), hand.io_word_reads);
    assert_eq!(of(io_word), 0, "and no word read of the I/O range");
    assert_eq!(of(status_odd_byte), hand.status_odd_byte_reads);
}

/// The precondition behind "`status_odd_byte_reads` = Byte and odd": the 68000 bus adapter emits **only**
/// `Byte` and `Word` accesses (a `.l` operand is two word bus cycles), so the probe's "odd and non-Word"
/// and a `Byte`+`Odd` watch classify identically on any real stream. A census over `Size` proves it on a
/// real run rather than asserting it from the source comment.
#[test]
fn the_bus_emits_only_byte_and_word_accesses() {
    let mut sys = booted_pad_poll();
    let mut wp = Watchpoints::new(0);
    let id = wp.add(
        Watch::bus(0..=0xFFFF_FFFF, WatchOp::Any, "widths")
            .mode(WatchMode::Census(CensusKey::Size)),
    );
    sys.run_frames_with_sink(2, &mut wp);
    let census = wp.watch(id).unwrap().census.unwrap();
    assert!(!census.is_empty(), "the machine ran");
    assert!(
        census.iter().all(|(k, _)| *k == 1 || *k == 2),
        "only 1-byte and 2-byte bus accesses, never 4: {census:?}"
    );
    assert!(!wp.watch(id).unwrap().keys_capped);
}
