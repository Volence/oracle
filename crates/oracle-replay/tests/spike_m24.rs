//! **SPIKE (M24-NEEDS-A-CURRENCY), NOT PROPOSED FOR MERGE.** The measurement legs behind
//! `docs/2026-09-13-z80-timing-currency-design.md`. Removed from the tip in the parcel's final `spike:`
//! commit.
//!
//! Every leg prints one `M24SPIKE` line per result to the real stderr (libtest cannot capture it) and, if
//! `M24_SPIKE_OUT` names a file, appends it there. The mutation under test is `M24_MUT` (see
//! `oracle_core::spike_m24::Mutation`); each line carries the probe's `mut_fired`, the runtime proof that
//! the mutation applied.
//!
//! - Leg A: `fixtures/aeon/s4.debug.bin` from power-on, no input, `FRAMES` whole frames. Candidates (a)
//!   access-stream digests, (b) `export_state` sampled at every frame end, (c) the real `VgmLogger` (both
//!   timings), (d) the acceptance log.
//! - Leg B: both committed aeon replay fixtures through `runner::run`, exactly as the release suite runs
//!   them, with the probe's digests.
//! - Leg C: three synthetic Z80 programs, one per behaviour (EI delay, `/INT` width, grant timing), whose
//!   observable lands in Z80 RAM.

use oracle_core::m68000::bus68k::Bus68k;
use oracle_core::spike_m24::{self, fnv, FNV_BASIS};
use oracle_core::system::System;
use oracle_core::vgm::VgmLogger;
use oracle_replay::cli::Fixture;
use oracle_replay::runner::{self, Prepared, RunConfig, Verdict};
use std::io::Write;
use std::path::PathBuf;

fn out(line: String) {
    let line = format!("M24SPIKE mut={:?} {line}\n", spike_m24::mutation());
    let _ = std::io::stderr().write_all(line.as_bytes());
    if let Ok(path) = std::env::var("M24_SPIKE_OUT") {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

fn aeon(name: &str) -> Vec<u8> {
    let p = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/aeon")).join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("UNMEASURABLE: {} unreadable: {e}", p.display()))
}

// export_state v2 offsets (docs/export-state-v1.md): region 3 Z80 RAM, region 4 Z80 registers.
const Z80_RAM: std::ops::Range<usize> = 0x10050..0x12050;
const Z80_REGS: std::ops::Range<usize> = 0x12050..0x12090;
const FRAMES: u64 = 1200;

#[test]
fn leg_a_aeon_fixed_frames() {
    spike_m24::reset();
    let mut sys = System::boot_with_sink(runner::POWER_ON_SEED, aeon("s4.debug.bin"), &mut ());
    let mut sub = VgmLogger::with_subframe_waits();
    let mut framed = VgmLogger::new();
    let (mut all, mut z80ram, mut z80regs, mut rest) = (FNV_BASIS, FNV_BASIS, FNV_BASIS, FNV_BASIS);
    let mut checkpoints = Vec::new();
    for f in 1..=FRAMES {
        {
            let mut sink = oracle_core::bus::Fanout::new(&mut sub, &mut framed);
            sys.run_frames_with_sink(1, &mut sink);
        }
        let e = sys.export_state();
        assert!(e.len() >= Z80_REGS.end, "UNMEASURABLE: export_state shorter than its layout");
        fnv(&mut all, &e);
        fnv(&mut z80ram, &e[Z80_RAM]);
        fnv(&mut z80regs, &e[Z80_REGS]);
        fnv(&mut rest, &e[..Z80_RAM.start]);
        fnv(&mut rest, &e[Z80_REGS.end..]);
        if [60, 300, 600, 1200].contains(&f) {
            checkpoints.push(format!("all@{f}={all:016x}"));
        }
    }
    let (mut vsub, mut vframe) = (FNV_BASIS, FNV_BASIS);
    fnv(&mut vsub, &sub.render_vgm());
    fnv(&mut vframe, &framed.render_vgm());
    let p = spike_m24::snapshot();
    out(format!(
        "leg=A frames={FRAMES} export_all={all:016x} export_z80ram={z80ram:016x} \
         export_z80regs={z80regs:016x} export_rest={rest:016x} vgm_records={} vgm_fm={} vgm_psg={} \
         vgm_subframe={vsub:016x} vgm_frame={vframe:016x} {} {}",
        sub.records().len(),
        sub.fm_writes(),
        sub.psg_writes(),
        checkpoints.join(" "),
        p.line()
    ));
}

/// Folds every emitted scanline into a digest, which arms the real renderer (`wants_scanlines`), so the
/// output-only `render` control has a CRAM decode to act on. Leg A's null/VGM sinks never decode CRAM
/// (finding C5: the unarmed arm runs `advance_scanline`), which is why `render` never fired there.
struct PixelDigest(u64);

impl oracle_core::bus::BusEventSink for PixelDigest {
    fn on_event(&mut self, _event: oracle_core::bus::BusEvent) {}
    fn wants_scanlines(&self) -> bool {
        true
    }
    fn on_scanline(&mut self, line: u16, rgb: &[(u8, u8, u8)]) {
        fnv(&mut self.0, &line.to_le_bytes());
        for &(r, g, b) in rgb {
            fnv(&mut self.0, &[r, g, b]);
        }
    }
}

/// Leg A2: 300 frames of aeon with the renderer armed. The pixel digest is the render control's positive
/// control; the probe's timing digests must not care whether anything was rendered.
#[test]
fn leg_a2_aeon_rendered() {
    spike_m24::reset();
    let mut sys = System::boot_with_sink(runner::POWER_ON_SEED, aeon("s4.debug.bin"), &mut ());
    let mut px = PixelDigest(FNV_BASIS);
    sys.run_frames_with_sink(300, &mut px);
    let p = spike_m24::snapshot();
    out(format!("leg=A2 frames=300 pixels={:016x} {}", px.0, p.line()));
}

fn leg_b(fixture: Fixture) {
    let lst = String::from_utf8_lossy(&aeon("s4.debug.lst")).into_owned();
    let prepared = Prepared::new(aeon("s4.debug.bin"), &lst, fixture).expect("prepare");
    let cfg = RunConfig {
        max_frames: oracle_replay::cli::DEFAULT_MAX_FRAMES,
        stall_frames: oracle_replay::cli::DEFAULT_STALL_FRAMES,
    };
    spike_m24::reset();
    let r = runner::run(&prepared, cfg).expect("the run must complete");
    let verdict = match &r.verdict {
        Verdict::Pass => "PASS".to_string(),
        other => {
            let d = format!("{other:?}");
            d.chars().take(60).collect::<String>().replace(' ', "_")
        }
    };
    let p = spike_m24::snapshot();
    out(format!(
        "leg=B fixture={fixture} verdict={verdict} frames_to_arm={} frames_after_arm={} logic_tick={} {}",
        r.frames_to_arm,
        r.frames_after_arm,
        r.probe.logic_tick,
        p.line()
    ));
}

#[test]
fn leg_b_ojz() {
    leg_b(Fixture::Ojz);
}

#[test]
fn leg_b_ojz_slide() {
    leg_b(Fixture::OjzSlide);
}

/// A booted `testrom::build` machine (its 68000 stirs work RAM forever and never touches the Z80) with
/// `program` loaded into Z80 RAM and the Z80 released from reset through `$A11200`, as a guest does.
fn synthetic(program: &[(usize, &[u8])]) -> System {
    let mut s = System::new(0x5EED);
    s.load_rom(oracle_core::testrom::build());
    s.reset();
    for (at, bytes) in program {
        s.z80_ram_mut()[*at..*at + bytes.len()].copy_from_slice(bytes);
    }
    s.mega_bus(&mut ()).write8(0xA1_1200, 5, 1);
    assert!(s.z80_running(), "UNMEASURABLE: the reset release did not latch");
    s
}

/// C1, EI delay. DI; IM 1; wait for V = $E0 (line 224), ~870 mclk more (past the VInt H=$02 anchor);
/// XOR A; EI; INC A; INC A; INC A; spin. The IM 1 handler stores A, then V and H, then $AA, and halts.
/// UM0080 p.18: the pending request "is not accepted until after the instruction following EI", so
/// hardware stores A = 1. Accepting at the boundary right after EI stores A = 0.
#[test]
fn leg_c1_ei_delay() {
    spike_m24::reset();
    let main: &[u8] = &[
        0xF3, // 0000 DI
        0xED, 0x56, // 0001 IM 1
        0x31, 0x00, 0x20, // 0003 LD SP,$2000
        0x3A, 0x08, 0x7F, // 0006 LD A,($7F08)   V counter
        0xFE, 0xE0, // 0009 CP $E0
        0x38, 0xF9, // 000B JR C,$0006
        0x06, 0x04, // 000D LD B,4
        0x10, 0xFE, // 000F DJNZ $000F
        0xAF, // 0011 XOR A
        0xFB, // 0012 EI
        0x3C, // 0013 INC A
        0x3C, // 0014 INC A
        0x3C, // 0015 INC A
        0x18, 0xFE, // 0016 JR $0016
    ];
    let isr: &[u8] = &[
        0x32, 0x00, 0x10, // 0038 LD ($1000),A
        0x3A, 0x08, 0x7F, // LD A,($7F08)
        0x32, 0x02, 0x10, // LD ($1002),A
        0x3A, 0x09, 0x7F, // LD A,($7F09)
        0x32, 0x03, 0x10, // LD ($1003),A
        0x3E, 0xAA, // LD A,$AA
        0x32, 0x01, 0x10, // LD ($1001),A
        0x76, // HALT
    ];
    let mut s = synthetic(&[(0, main), (0x38, isr)]);
    s.run_frames(2);
    let r = &s.z80_ram()[0x1000..0x1004];
    let p = spike_m24::snapshot();
    out(format!(
        "leg=C1 a_at_accept={} isr_ran={:02x} v={:02x} h={:02x} {}",
        r[0],
        r[1],
        r[2],
        r[3],
        p.line()
    ));
}

/// C2, `/INT` width. DI; IM 1; wait for V = $E2 (line 226, two lines past the assert); HL = 0; EI; INC HL
/// forever. The handler stores HL and V, then $AA, and halts. R6 (one-line pulse, SpritesMind t=740/787):
/// the request is gone by line 226, so hardware takes the NEXT frame's interrupt (V = $E0, HL ~ a frame
/// of INC HL). A request held to line 0 is taken at once (V = $E2, HL = 0).
#[test]
fn leg_c2_int_width() {
    spike_m24::reset();
    let main: &[u8] = &[
        0xF3, // 0000 DI
        0xED, 0x56, // 0001 IM 1
        0x31, 0x00, 0x20, // 0003 LD SP,$2000
        0x3A, 0x08, 0x7F, // 0006 LD A,($7F08)
        0xFE, 0xE2, // 0009 CP $E2
        0x38, 0xF9, // 000B JR C,$0006
        0x21, 0x00, 0x00, // 000D LD HL,0
        0xFB, // 0010 EI
        0x23, // 0011 INC HL
        0x18, 0xFD, // 0012 JR $0011
    ];
    let isr: &[u8] = &[
        0x22, 0x00, 0x10, // 0038 LD ($1000),HL
        0x3A, 0x08, 0x7F, // LD A,($7F08)
        0x32, 0x02, 0x10, // LD ($1002),A
        0x3E, 0xAA, // LD A,$AA
        0x32, 0x04, 0x10, // LD ($1004),A
        0x76, // HALT
    ];
    let mut s = synthetic(&[(0, main), (0x38, isr)]);
    s.run_frames(3);
    let r = &s.z80_ram()[0x1000..0x1005];
    let p = spike_m24::snapshot();
    out(format!(
        "leg=C2 hl_at_accept={} v_at_accept={:02x} isr_ran={:02x} {}",
        u16::from_le_bytes([r[0], r[1]]),
        r[2],
        r[4],
        p.line()
    ));
}

/// C3, grant timing. DI; HL = 0; loop { INC HL; LD ($1000),HL; JR } = 6 + 16 + 12 = 34 T = 510 mclk per
/// pass. The harness alternates GATED_ON mclk released with a GRANT mclk bus grant, `GRANTS` times. The
/// count is then the Z80's gated-on time / 510 (to within the loop's partial first and last pass).
#[test]
fn leg_c3_grant_timing() {
    const GRANTS: u32 = 10;
    const GATED_ON: u64 = 200_000;
    const GRANT: u64 = 10_000;
    spike_m24::reset();
    let main: &[u8] = &[
        0xF3, // 0000 DI
        0x21, 0x00, 0x00, // 0001 LD HL,0
        0x23, // 0004 INC HL
        0x22, 0x00, 0x10, // 0005 LD ($1000),HL
        0x18, 0xFA, // 0008 JR $0004
    ];
    let mut s = synthetic(&[(0, main)]);
    let mut gated_on = 0u64;
    for _ in 0..GRANTS {
        let t = s.scheduler().now();
        s.run_until(t + GATED_ON);
        gated_on += s.scheduler().now() - t;
        s.mega_bus(&mut ()).write8(0xA1_1100, 5, 1);
        let t = s.scheduler().now();
        s.run_until(t + GRANT);
        s.mega_bus(&mut ()).write8(0xA1_1100, 5, 0);
    }
    let t = s.scheduler().now();
    s.run_until(t + GATED_ON);
    gated_on += s.scheduler().now() - t;
    let r = &s.z80_ram()[0x1000..0x1002];
    let p = spike_m24::snapshot();
    out(format!(
        "leg=C3 count={} gated_on_mclk={gated_on} derived_count={} grants_driven={GRANTS} {}",
        u16::from_le_bytes([r[0], r[1]]),
        gated_on / 510,
        p.line()
    ));
}
