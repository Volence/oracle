//! Integration proof for the bus-level recording watchpoints: register a watch on a work-RAM address, run a
//! real frame on a real [`System`] with the [`Watchpoints`] attached as the bus-event sink, and confirm each
//! hit is attributed to the exact instruction (PC) and master (function code) that drove the access.
//!
//! The fixture is `testrom::build`: after boot it stirs the first `$4000` work-RAM words (`$FF0000..$FF7FFE`)
//! with `move.w D0,(A0)+` at PC `$212`, each preceded by a `move.w (A0),D0` read at PC `$20E`. One stir pass
//! is far longer than one frame (~3.4M mclk vs ~896k), so within the first frame `$FF0000` is read exactly
//! once (at `$20E`) and written exactly once (at `$212`) — a clean single-hit assertion. Work RAM is
//! randomized at power-on, so the read returns the power-on word `V` and the store writes `V + 1`.

use oracle_core::bus::{BusEventSink, BusOp, Size};
use oracle_core::system::System;
use oracle_core::watchpoints::{WatchOp, WatchSpace, WatchVia, Watchpoints};

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
