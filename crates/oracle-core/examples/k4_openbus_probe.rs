//! K4-0 — the open-bus blast-radius instrument (design: `docs/2026-08-02-k4-openbus-design.md` §5).
//!
//! Boots ROMs headlessly with a [`BusEventSink`] attached and classifies every 68k bus access whose
//! **returned value would change under the K4 open-bus rules** (or whose count gates a K4 slice's risk).
//! Zero `src/` changes: this is a pure event-stream consumer riding the existing
//! `System::run_frames_with_sink` seam. The per-ROM hit table it prints is the gate for slices K4-1..K4-5
//! (and for the STOP conditions: a differential-corpus *game* reading `$A11100` under Z80 reset, touching
//! the Z80 window while closed, or consuming `$C00004` through flags).
//!
//! ```text
//! cargo run --release -p oracle-core --example k4_openbus_probe            # default sweep (see below)
//! cargo run --release -p oracle-core --example k4_openbus_probe -- <rom.bin> [frames]
//! ```
//!
//! The default sweep covers: the currency-gate fixture (`testrom::build()`, 120 frames — the
//! `determinism_gate`/`export_state_v1` workload), the vendored conformance corpus
//! (`vendor/TestRoms/*.bin`, 700 frames each, with the page-2 `Start` press for `vdp_port_access`), and
//! the differential game corpus from `docs/2026-07-22-differential-rom-findings.md` (user-disk paths,
//! **skip-if-absent**, 900 frames each — boot through title/attract).
//!
//! ## Columns (a read is counted at the access size the CPU issued; longs are two word events)
//!
//! | column | reads counted | K4 slice it gates |
//! |---|---|---|
//! | `unm` / `unm!` | `$400000-$7FFFFF` total / value-would-change (word low byte ≠ 0, odd byte ≠ 0) | K4-1 |
//! | `1200` | `$A11200-1` reads (constant readback → arbiter open bus) | K4-1 |
//! | `1100` / `1100R!` | `$A11100-1` reads / of those, while Z80 reset is still asserted (STOP (b)) | K4-2 |
//! | `zcR!` / `zcW!` | Z80-window (`$A00000-$A0FFFF` minus FM) reads / writes while the window would be CLOSED (`!busreq \|\| !running`, reconstructed from the write stream) (STOP (b)) | K4-3 |
//! | `zw!` | open-window WORD reads whose two bytes differ (word = even byte duplicated under K4-3) | K4-3 |
//! | `bank` | open-window reads in `$A06000-$A07EFF` (→ `$FF` under K4-3) | K4-3 |
//! | `ioE` / `ioW` | `$A10000-$A1001F` even-byte reads / word reads (register mirrors onto both lanes) | K4-4 |
//! | `st` / `stO` | `$C00004-7` word + even-byte reads (upper 6 bits become residue) / odd-byte reads (unaffected, for scale) | K4-5 |
//! | `wwW!` | open-window WORD writes to `$A0xxxx` (Q4: hardware lands only ONE byte; we store both) | Q4 |
//! | `bkW` | open-window writes landing at Z80 offset `$6000-$60FF` (the 68k-side bank-latch path) | K4-3 follow-up |
//! | `vpW` | open-window writes landing at Z80 offset `$7F00-$7F1F` (the VDP/PSG mirror seen from the 68k) | K4-3 follow-up |
//!
//! The three follow-up columns classify by the window's TRUE 15-bit offset (`addr & 0x7FFF`,
//! `MDBusArbiter.cpp:487` — hardware masks the window to 15 bits), so `$A0E000` counts as bank-register
//! territory even though the pre-remodel bus still treats it as a RAM mirror.

use oracle_core::bus::{BusEvent, BusEventSink, BusOp, Size};
use oracle_core::io::Pad;
use oracle_core::system::System;

#[path = "common/rom_source.rs"]
mod rom_source;

/// The classifier. Shadows the two arbiter latches from the write stream (the sink sees every write,
/// so the reconstruction is exact — same even-byte-only latch rule as the bus).
///
/// **The shadow stays, deliberately** (`F-TRACE-EXPOSE-LATCHES`, measured 2026-08-14). `System` now
/// exposes `z80_busreq()`/`z80_running()`, but a sink is *handed events* — it never holds the machine —
/// so the only place this probe could read them is the frame boundary in [`probe`], and the five
/// stateful counters gate on the latch state **at each event**. Those are not the same instant: the
/// guest's take-bus → upload → release handshake completes many times *inside* one frame. Substituting
/// a frame-boundary sample was tried and moved **22 of the 29 rows** in the sweep — `zcW!` on
/// `aeon-s4` went 0 → 7563 (a normal driver upload misread as writes through a closed window),
/// `m68k_memory_test`'s `zcR!` collapsed 3 → 0, and `1100R!` fabricated a hit on nearly every ROM.
/// That table is the evidence behind the shipped K4 open-bus finding, so the shadow is the correct
/// implementation here, not a duplication to be deleted.
#[derive(Default)]
struct K4Probe {
    /// Shadow of the `$A11100` BUSREQ latch (bit0 of the even byte).
    z80_busreq: bool,
    /// Shadow of the `$A11200` reset-release latch (bit0 of the even byte).
    z80_running: bool,
    unmapped_reads: u64,
    unmapped_would_change: u64,
    a11200_reads: u64,
    a11100_reads: u64,
    a11100_while_reset: u64,
    z80win_closed_reads: u64,
    z80win_closed_writes: u64,
    z80win_open_word_dup_changes: u64,
    z80win_bank_reads: u64,
    io_even_byte_reads: u64,
    io_word_reads: u64,
    status_upper_reads: u64,
    status_odd_byte_reads: u64,
    z80win_open_word_writes: u64,
    z80win_bank_writes: u64,
    z80win_vdp_mirror_writes: u64,
    /// Q4 adjudication detail: per (addr, "high byte == low byte") counts of the open-window word
    /// writes, so the docket can say WHAT the writes are (an upload loop? a mailbox?) and whether the
    /// one-byte-lands hardware rule could even change the stored value. Kept small — capped at 16 keys.
    ww_detail: std::collections::BTreeMap<(u32, bool), u64>,
}

impl K4Probe {
    fn any_hits(&self) -> bool {
        self.unmapped_reads
            + self.a11200_reads
            + self.a11100_reads
            + self.z80win_closed_reads
            + self.z80win_closed_writes
            + self.z80win_open_word_dup_changes
            + self.z80win_bank_reads
            + self.io_even_byte_reads
            + self.io_word_reads
            + self.status_upper_reads
            > 0
    }

    fn row(&self, name: &str) -> String {
        format!(
            "| {name} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            self.unmapped_reads,
            self.unmapped_would_change,
            self.a11200_reads,
            self.a11100_reads,
            self.a11100_while_reset,
            self.z80win_closed_reads,
            self.z80win_closed_writes,
            self.z80win_open_word_dup_changes,
            self.z80win_bank_reads,
            self.io_even_byte_reads,
            self.io_word_reads,
            self.status_upper_reads,
            self.status_odd_byte_reads,
            self.z80win_open_word_writes,
            self.z80win_bank_writes,
            self.z80win_vdp_mirror_writes,
        )
    }

    /// The Q4 word-write detail lines (empty string when there were none).
    fn ww_detail_lines(&self, name: &str) -> String {
        self.ww_detail
            .iter()
            .map(|((addr, same), n)| {
                format!(
                    "    wwW! {name}: {n} word write(s) @ ${addr:06X} (hi==lo: {same}) — one-byte-lands would {}\n",
                    if *same { "store the SAME bytes" } else { "CHANGE the stored bytes" }
                )
            })
            .collect()
    }
}

impl BusEventSink for K4Probe {
    fn on_event(&mut self, e: BusEvent) {
        // Z80-side bus events are 16-bit addresses (< $10000) — no overlap with any category below.
        match e.op {
            BusOp::Write => {
                // Reconstruct the arbiter latches exactly as the bus latches them: bit0 of the EVEN byte
                // (a word write carries the even byte in bits 15-8; odd-byte writes drop).
                let bit = |e: &BusEvent| match e.size {
                    Size::Byte => e.value & 1 != 0,
                    Size::Word => (e.value >> 8) & 1 != 0,
                    Size::Long => unreachable!("the 68k bus emits byte/word events only"),
                };
                match (e.addr, e.size) {
                    (0xA1_1100, _) => self.z80_busreq = bit(&e),
                    (0xA1_1200, _) => self.z80_running = bit(&e),
                    (0xA0_0000..=0xA0_FFFF, _) => {
                        // FM ports answer regardless of Z80 bus ownership (design §4, row 4).
                        let fm = (0xA0_4000..=0xA0_4003).contains(&e.addr);
                        if !fm && !(self.z80_busreq && self.z80_running) {
                            self.z80win_closed_writes += 1;
                        } else if !fm {
                            // Open-window write classification by the TRUE 15-bit window offset
                            // (hardware masks to 15 bits, MDBusArbiter.cpp:487).
                            if e.size == Size::Word {
                                self.z80win_open_word_writes += 1; // Q4
                                let same = (e.value >> 8) & 0xFF == e.value & 0xFF;
                                if self.ww_detail.len() < 16
                                    || self.ww_detail.contains_key(&(e.addr, same))
                                {
                                    *self.ww_detail.entry((e.addr, same)).or_insert(0) += 1;
                                }
                            }
                            match e.addr & 0x7FFF {
                                0x6000..=0x60FF => self.z80win_bank_writes += 1,
                                0x7F00..=0x7F1F => self.z80win_vdp_mirror_writes += 1,
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            BusOp::Read | BusOp::Tas => match e.addr {
                0x40_0000..=0x7F_FFFF => {
                    self.unmapped_reads += 1;
                    let changes = match e.size {
                        // K4-1 word value = latch & $FF00 — differs iff the low byte is nonzero.
                        Size::Word => e.value & 0xFF != 0,
                        // Even byte (UDS half) is unchanged; odd byte becomes $00.
                        Size::Byte => e.addr & 1 == 1 && e.value != 0,
                        Size::Long => false,
                    };
                    if changes {
                        self.unmapped_would_change += 1;
                    }
                }
                0xA1_1200 | 0xA1_1201 => self.a11200_reads += 1,
                0xA1_1100 | 0xA1_1101 => {
                    self.a11100_reads += 1;
                    if !self.z80_running {
                        self.a11100_while_reset += 1;
                    }
                }
                0xA0_4000..=0xA0_4003 => {} // FM status — K4 leaves it alone (row 4 already passes)
                0xA0_0000..=0xA0_FFFF => {
                    if !(self.z80_busreq && self.z80_running) {
                        self.z80win_closed_reads += 1;
                    } else {
                        if e.size == Size::Word && (e.value >> 8) & 0xFF != e.value & 0xFF {
                            self.z80win_open_word_dup_changes += 1;
                        }
                        if (0xA0_6000..=0xA0_7EFF).contains(&e.addr) {
                            self.z80win_bank_reads += 1;
                        }
                    }
                }
                0xA1_0000..=0xA1_001F => match e.size {
                    Size::Byte if e.addr & 1 == 0 => self.io_even_byte_reads += 1,
                    Size::Word => self.io_word_reads += 1,
                    _ => {}
                },
                0xC0_0004..=0xC0_0007 => {
                    if e.size == Size::Word || e.addr & 1 == 0 {
                        self.status_upper_reads += 1;
                    } else {
                        self.status_odd_byte_reads += 1;
                    }
                }
                _ => {}
            },
        }
    }
}

/// Run one ROM image for `frames` with the probe attached; `press_start_at` optionally holds Start for
/// 5 frames (the `vdp_port_access` page-2 advance, mirroring the conformance scraper).
///
/// Frame-by-frame with a panic guard: games whose Z80 driver hits a deferred undocumented opcode
/// (`z80/mod.rs` DD/FD half-register ops — a pre-existing, out-of-K4-scope gap) stop there instead of
/// killing the sweep; the returned frame count says how much boot path the row actually covers.
fn probe(rom: Vec<u8>, frames: u64, press_start_at: Option<u64>) -> (K4Probe, u64) {
    let mut sys = System::new(0x1234_5678);
    sys.load_rom(rom);
    sys.reset();
    let mut sink = K4Probe::default();
    let mut done = 0;
    for f in 0..frames {
        if press_start_at.is_some_and(|at| f == at) {
            sys.set_pad(
                0,
                Pad {
                    start: true,
                    ..Default::default()
                },
            );
        }
        if press_start_at.is_some_and(|at| f == at + 5) {
            sys.set_pad(0, Pad::default());
        }
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sys.run_frames_with_sink(1, &mut sink)
        }))
        .is_ok();
        if !ok {
            break; // the panic message (deferred opcode) already went to stderr
        }
        done = f + 1;
    }
    (sink, done)
}

const HEADER: &str =
    "| ROM | unm | unm! | 1200 | 1100 | 1100R! | zcR! | zcW! | zw! | bank | ioE | ioW | st | stO | wwW! | bkW | vpW |";
const RULE: &str = "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    println!("{HEADER}");
    println!("{RULE}");

    if !args.is_empty() {
        let frames: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(900);
        let rom = std::fs::read(&args[0]).expect("ROM path");
        let (p, ran) = probe(rom, frames, None);
        println!("{}", p.row(&format!("{} ({ran}/{frames}f)", args[0])));
        eprint!("{}", p.ww_detail_lines(&args[0]));
        return;
    }

    // 1. The currency-gate fixture: the exact `determinism_gate` / `export_state_v1` workload.
    let (p, _) = probe(oracle_core::testrom::build(), 120, None);
    println!("{}", p.row("gate-fixture (testrom::build, 120f)"));
    assert!(
        !p.any_hits(),
        "the gate fixture touched a K4-affected address — currencies are NOT frozen-by-construction"
    );

    // 2. The vendored conformance corpus (skip-if-absent, like the harness).
    let vendor = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/TestRoms");
    let mut names: Vec<_> = match std::fs::read_dir(vendor) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "bin"))
            .collect(),
        Err(_) => {
            eprintln!("SKIP: {vendor} not found — run tools/fetch-testroms.sh");
            Vec::new()
        }
    };
    names.sort();
    for path in names {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let press = (name == "vdp_port_access").then_some(60);
        let (p, ran) = probe(std::fs::read(&path).unwrap(), 700, press);
        println!("{}", p.row(&format!("{name} ({ran}/700f)")));
        eprint!("{}", p.ww_detail_lines(&name));
    }

    // 3. The differential game corpus (docs/2026-07-22-differential-rom-findings.md — user-disk paths,
    //    never downloaded, never committed, skip-if-absent).
    //
    //    The `false` flag marks a row whose bytes are NOT reproducible from this repository. For the
    //    commercial dumps that is inherent and harmless: a dumped cartridge does not change. For an
    //    Aeon build product it is not harmless — that tree is rebuilt without warning, so a row could
    //    silently describe a different game than the one it names. `aeon-s4` therefore reads our own
    //    frozen copy; `s4.soundtest.bin` has no frozen counterpart at all (it is absent from sigil's
    //    chain-attested goldens) and `demo.bin` we chose not to add to the freeze for one survey row,
    //    so both stay live and say so on the row rather than passing as pinned.
    // Built through `rom_source` rather than spelled out, so `LIVE_AEON_DIR` is the one place the live
    // tree is named and the row marker below cannot drift from the paths it is judging.
    let frozen_s4 = rom_source::frozen("s4.bin");
    let live_soundtest = rom_source::live_aeon("s4.soundtest.bin");
    let live_demo = rom_source::live_aeon("demo.bin");
    let games: [(&str, &str, bool); 11] = [
        ("s2rev01", "/home/volence/sonic_hacks/AP Backups/Awesome Project/s2rev01.bin", false),
        ("sonic3k", "/home/volence/sonic_hacks/skdisasm/sonic3k.bin", false),
        ("aeon-s4", &frozen_s4, true),
        ("aeon-s4-soundtest", &live_soundtest, false),
        ("aeon-demo", &live_demo, false),
        ("thunderforce4", "/home/volence/sonic_hacks/The Adventures of Batman and Robin/Thunder Force IV (U).bin", false),
        ("gunstar", "/home/volence/sonic_hacks/The Adventures of Batman and Robin/Gunstar Heroes (USA).md", false),
        ("batman-robin", "/home/volence/sonic_hacks/The Adventures of Batman and Robin/Adventures of Batman & Robin, The (USA).md", false),
        ("alien-soldier", "/home/volence/sonic_hacks/The Adventures of Batman and Robin/Alien Soldier (Europe).md", false),
        ("vectorman", "/home/volence/sonic_hacks/The Adventures of Batman and Robin/Vectorman (USA, Europe).md", false),
        ("ristar", "/home/volence/sonic_hacks/The Adventures of Batman and Robin/Ristar (USA, Europe) (August 1994).md", false),
    ];
    for (name, path, pinned) in games {
        match std::fs::read(path) {
            Ok(rom) => {
                // An Aeon build product read from outside the freeze is the one case where the row
                // itself can go stale under us, so mark it in the row rather than only in a comment.
                let mark = if pinned || !path.starts_with(rom_source::LIVE_AEON_DIR) {
                    String::new()
                } else {
                    " [LIVE aeon tree, NOT frozen]".to_string()
                };
                let (p, ran) = probe(rom, 900, None);
                println!("{}", p.row(&format!("{name} ({ran}/900f){mark}")));
                eprint!("{}", p.ww_detail_lines(name));
            }
            Err(_) => eprintln!("SKIP: {path} not on this disk"),
        }
    }
}
