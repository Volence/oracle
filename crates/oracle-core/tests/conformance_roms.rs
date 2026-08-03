//! Test-ROM conformance harness — a **non-gating instrument with a pinned baseline**.
//!
//! Boots the vendored corpus of well-known Mega Drive test ROMs (`tools/fetch-testroms.sh`) headlessly
//! against the real `System`, scrapes each ROM's own verdict, and compares the WHOLE scorecard against a
//! pinned baseline in a single `assert_eq!` so one diff shows every changed ROM at once.
//!
//! **This harness does NOT gate on "everything passes."** Per `CHARTER.md` the launch target is
//! *MVP-debuggable*, NOT "passes VDPFIFOTesting"; accuracy is an asymptote. Several of these ROMs fail
//! today for documented reasons (the absent open-bus model, the Z80 `$7F00` VDP-mirror stub, ...; the
//! K1 illegal-decode hole was fixed 2026-08-02 — `m68k_illegal` is green since). Those failures are
//! **recorded, not fixed** — the baseline below is
//! a *photograph of today*, and this test fires only on a **regression from it** (or an unannounced
//! improvement, which is equally worth seeing). Every entry, its reference, and its expected-fail reason
//! is written up in `docs/2026-07-25-testrom-conformance.md`.
//!
//! Currency note: this file is additive-only test code. It touches no `oracle-core/src/` semantics, so the
//! frozen golden state hashes (`determinism_gate`, `export_state_v1`) are unmoved by construction.
//!
//! ## How a verdict is scraped
//!
//! Most of these ROMs print their result as text through the VDP. There is no OCR: the text ROMs use an
//! ASCII-ordered font, so a plane-A nametable cell's low 11 bits are `font_base + ASCII` and the screen can
//! be read straight out of VRAM. The font base is **hardcoded per ROM** (see `Rom::font_base`) rather than
//! auto-detected, so a decode change shows up as garbled text (a visible regression) instead of being
//! silently re-derived. Two ROMs need other channels: `m68k_illegal` signals via the backdrop colour, and
//! `vdp_sprite_masking` draws its per-test verdict as a 32x8 glyph (PASS / FAIL / tick / cross) that is
//! classified by hashing the rendered pixels — its nametable cells are identical for the tick and cross
//! cases, so only the framebuffer can tell them apart.
//!
//! If the vendored ROM is missing, that ROM skips cleanly (run `tools/fetch-testroms.sh`).

use oracle_core::io::Pad;
use oracle_core::system::System;
use std::path::Path;

const VENDOR_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/TestRoms");

/// Power-on RNG seed. Fixed so the whole scorecard is reproducible (seeded power-on RAM is a
/// non-negotiable of the core; a different seed is a different — still deterministic — machine).
const SEED: u64 = 0x1234_5678;

const ACTIVE_LINES: u16 = 224;

/// Every ROM `tools/fetch-testroms.sh` vendors. Keep in sync with that script: the count guard below
/// asserts the pinned baseline covers exactly this list, so a ROM can never be quietly dropped.
const ROMS: &[&str] = &[
    "color_1536",
    "cram_flicker",
    "direct_color_dma",
    "fm_test",
    "gfx_joystick",
    "io_sample",
    "m68k_bcd",
    "m68k_illegal",
    "m68k_memory_test",
    "m68k_opcode_sizes",
    "shadow_highlight",
    "vcounter",
    "vdp_port_access",
    "vdp_sprite_masking",
    "vdp_test_register",
    "window_distortion",
    "window_test",
];

// ---------------------------------------------------------------------------------------------------
// The pinned baseline
// ---------------------------------------------------------------------------------------------------

/// Today's recorded outcome for every ROM. **A diff here is the whole point of this test.** Changing an
/// entry is a deliberate, evidenced amendment (a fixed bug, a model refinement) — never a silent regen;
/// update `docs/2026-07-25-testrom-conformance.md` in the same change.
const BASELINE: &[(&str, &str)] = &[
    (
        "color_1536",
        "VISUAL-BASELINE frame_hash=0x96b9c93c4f3dd325 (end-of-frame capture only)",
    ),
    (
        "cram_flicker",
        "NOT-RENDERABLE (border-only rendering) frame_hash=0x815bb645bc46a325",
    ),
    (
        "direct_color_dma",
        "NOT-RENDERABLE (sub-scanline CRAM) frame_hash=0xed40dc4a6c4fc325",
    ),
    ("fm_test", "VISUAL-BASELINE frame_hash=0xed40dc4a6c4fc325"),
    (
        "gfx_joystick",
        "VISUAL-BASELINE frame_hash=0xba8ce06075e4e163",
    ),
    ("io_sample", "PASS port1=JOYPAD port2=JOYPAD"),
    (
        "m68k_bcd",
        "PASS abcd/sbcd/nbcd 0 value errors, 0 flag errors",
    ),
    ("m68k_illegal", "PASS backdrop=$00E0 (green)"),
    (
        // K4-1 (2026-08-02): arbiter open-bus flavor — `400000-7FFFFF` + `A11200` rows green (was 4/13).
        // K4-2 (2026-08-02): $A11100 residue + reset-folded grant bit — both `A11100` rows green.
        // K4-3 (2026-08-02): Z80-window gating + word duplication + $A06000-$A07EFF=$FF (and the
        // bank-canary alias fix) — the three `A0xxxx` window rows green.
        // See docs/2026-08-02-k4-openbus-design.md and the K4 ledger section.
        "m68k_memory_test",
        "11/13 rows match the ROM's hardware reference; mismatch: \
         A10000-A1001F, C00004-C00007",
    ),
    (
        "m68k_opcode_sizes",
        // Re-pinned 2026-08-02 with the K1 illegal-decode fix: the ROM plots the live decode map, and
        // the newly-trapping encodings move a handful of its cells (476 px in the 0x0/4/8/C/E pages).
        "VISUAL-BASELINE frame_hash=0x5436cda5786ea450",
    ),
    (
        "shadow_highlight",
        "VISUAL-BASELINE frame_hash=0x428e03aa61cc0285",
    ),
    ("vcounter", "VISUAL-BASELINE frame_hash=0x294957c8001b9f93"),
    (
        "vdp_port_access",
        "page1 pass/fail/total=6/3/9; pages1+2 cumulative=9/7/16",
    ),
    (
        "vdp_sprite_masking",
        "H32: 1=TICK/TICK 2=TICK/TICK 3=TICK/CROSS 4=PASS 5=PASS 6=FAIL 7=PASS 8=PASS 9=TICK/TICK",
    ),
    (
        "vdp_test_register",
        "VISUAL-BASELINE frame_hash=0x4a49cfea306e928b",
    ),
    (
        "window_distortion",
        "VISUAL-BASELINE frame_hash=0x5102219d295b4e2c",
    ),
    (
        "window_test",
        "VISUAL-BASELINE frame_hash=0x4efcfda475af0d12",
    ),
];

// ---------------------------------------------------------------------------------------------------
// Harness plumbing
// ---------------------------------------------------------------------------------------------------

fn rom_path(name: &str) -> String {
    format!("{VENDOR_DIR}/{name}.bin")
}

/// Boot a vendored ROM headlessly, or `None` (with a `SKIP:` note) if it has not been fetched.
fn boot(name: &str) -> Option<System> {
    let path = rom_path(name);
    let Ok(rom) = std::fs::read(&path) else {
        eprintln!("SKIP: {path} not found — run tools/fetch-testroms.sh");
        return None;
    };
    let mut sys = System::new(SEED);
    sys.load_rom(rom);
    sys.reset();
    Some(sys)
}

/// FNV-1a over the whole active framebuffer (the `golden_frames.rs` idiom). A *self-consistency* pin: it
/// captures what the current model draws, nothing more.
fn frame_hash(sys: &System) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for line in 0..ACTIVE_LINES {
        for (r, g, b) in sys.vdp().render_line(line) {
            for byte in [r, g, b] {
                h ^= byte as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    h
}

/// FNV-1a over one rendered rectangle (used to classify `vdp_sprite_masking`'s verdict glyphs).
fn block_hash(sys: &System, x0: usize, x1: usize, y0: u16, y1: u16) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for line in y0..y1 {
        let px = sys.vdp().render_line(line);
        for (r, g, b) in px.iter().take(x1).skip(x0) {
            for byte in [*r, *g, *b] {
                h ^= byte as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    h
}

/// Read the on-screen text out of the plane-A nametable, one `String` per cell row.
///
/// Plane A base = `(R2 & $38) << 10`; plane pitch from R16; visible columns from the rendered line width
/// (256 = H32, 320 = H40). A cell's low 11 bits are `font_base + ASCII` for every text ROM here; anything
/// outside printable ASCII becomes a space, so rows compare by their text content after [`squeeze`].
fn text_rows(sys: &System, font_base: u16) -> Vec<String> {
    let vdp = sys.vdp();
    let base = ((vdp.regs()[2] as usize) & 0x38) << 10;
    let pitch = match vdp.regs()[16] & 0x03 {
        0 => 32,
        1 => 64,
        _ => 128,
    };
    let cols = vdp.render_line(0).len() / 8;
    let rows = (ACTIVE_LINES / 8) as usize;
    (0..rows)
        .map(|row| {
            (0..cols)
                .map(|col| {
                    let off = base + (row * pitch + col) * 2;
                    let cell = u16::from_be_bytes([vdp.vram()[off], vdp.vram()[off + 1]]);
                    let c = (cell & 0x07FF).wrapping_sub(font_base);
                    if (0x21..0x7F).contains(&c) {
                        c as u8 as char
                    } else {
                        ' '
                    }
                })
                .collect()
        })
        .collect()
}

/// Collapse runs of whitespace so a scraped row compares by content, not by column padding.
fn squeeze(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------------------------------
// Per-ROM verdict scrapers
// ---------------------------------------------------------------------------------------------------

/// `bcd-verifier-u1` — exhaustive ABCD/SBCD/NBCD value+flag verification. Reports one row per mnemonic
/// with the value-error and flag-error counts; `$00000 $00000` is a clean sweep. Settles well before
/// frame 700 (the sweep is long, hence the frame budget).
fn scrape_m68k_bcd(sys: &mut System) -> String {
    sys.run_frames(700);
    let rows = text_rows(sys, 0x000);
    let mut bad = Vec::new();
    for op in ["abcd", "sbcd", "nbcd"] {
        let row = rows
            .iter()
            .map(|r| squeeze(r))
            .find(|r| r.starts_with(op))
            .unwrap_or_else(|| format!("{op} <row not found>"));
        if row != format!("{op} $00000 $00000") {
            bad.push(row);
        }
    }
    if bad.is_empty() {
        "PASS abcd/sbcd/nbcd 0 value errors, 0 flag errors".to_string()
    } else {
        format!("FAIL {}", bad.join("; "))
    }
}

/// `memtest_68k` — reads every non-lockup address range twice and prints the two words it read, followed
/// by the ROM's OWN built-in real-hardware reference in parentheses (`?` = wildcard nibble). Row shape:
/// `ADDR : A B (EA EB)`. Most mismatches here are the absent open-bus model.
fn scrape_m68k_memory_test(sys: &mut System) -> String {
    sys.run_frames(30);
    let rows = text_rows(sys, 0x100);
    let mut total = 0usize;
    let mut matched = 0usize;
    let mut mismatched = Vec::new();
    for row in rows.iter().map(|r| squeeze(r)) {
        let Some((label, rest)) = row.split_once(" : ") else {
            continue;
        };
        let f: Vec<&str> = rest
            .split_whitespace()
            .map(|t| t.trim_matches(['(', ')']))
            .collect();
        if f.len() != 4 {
            continue;
        }
        total += 1;
        let hit = |got: &str, want: &str| {
            got.len() == want.len()
                && got
                    .chars()
                    .zip(want.chars())
                    .all(|(g, w)| w == '?' || g == w)
        };
        if hit(f[0], f[2]) && hit(f[1], f[3]) {
            matched += 1;
        } else {
            mismatched.push(label.trim().to_string());
        }
    }
    format!(
        "{matched}/{total} rows match the ROM's hardware reference; mismatch: {}",
        mismatched.join(", ")
    )
}

/// `Multitap - IO Sample Program` — probes both controller ports and names the detected device. A
/// three-button pad must read back as `JOYPAD` on port 1 and port 2.
fn scrape_io_sample(sys: &mut System) -> String {
    sys.run_frames(160);
    let rows = text_rows(sys, 0x000);
    let found: Vec<String> = rows
        .iter()
        .map(|r| squeeze(r))
        .filter(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_uppercase() || c == ' '))
        .filter(|r| r.contains("JOYPAD"))
        .collect();
    if found.len() == 2 {
        "PASS port1=JOYPAD port2=JOYPAD".to_string()
    } else {
        format!("FAIL {} port(s) report JOYPAD (want 2)", found.len())
    }
}

/// `itest` — sweeps illegal / privileged / unimplemented encodings and checks each traps correctly. No
/// text: the verdict is the backdrop colour — blue `$0E00` while the sweep runs, then green `$00E0` =
/// pass, red `$000E` = fail. A completed sweep (all 11529 table entries trapping) settles by ~frame 9;
/// the old 2-frame budget dated from when the ROM failed fast (K1) and never reached the verdict write.
fn scrape_m68k_illegal(sys: &mut System) -> String {
    sys.run_frames(20);
    let cram = sys.vdp().cram();
    let backdrop = u16::from_be_bytes([cram[0], cram[1]]);
    match backdrop {
        0x00E0 => "PASS backdrop=$00E0 (green)".to_string(),
        0x000E => "FAIL backdrop=$000E (red); pass would be $00E0 (green)".to_string(),
        other => format!("INDETERMINATE backdrop=${other:04X} (want $00E0 green / $000E red)"),
    }
}

/// `VDPFIFOTesting` — 16 VDP port-access tests over two pages. Page 1 auto-runs and settles by ~frame 42;
/// `Start` advances to page 2, whose tests take ~480 more frames. Both pages print a cumulative
/// `Results: ( P/ F/ T)` line.
fn scrape_vdp_port_access(sys: &mut System) -> String {
    let results = |sys: &System| -> String {
        text_rows(sys, 0x000)
            .iter()
            .map(|r| squeeze(r))
            .find(|r| r.starts_with("Results:"))
            .map(|r| {
                r.trim_start_matches("Results:")
                    .trim()
                    .trim_matches(['(', ')'])
                    .split('/')
                    .map(|t| t.trim().to_string())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .unwrap_or_else(|| "<no Results line>".to_string())
    };

    sys.run_frames(60);
    let page1 = results(sys);

    let pad = Pad {
        start: true,
        ..Default::default()
    };
    sys.set_pad(0, pad);
    sys.run_frames(5);
    sys.set_pad(0, Pad::default());
    sys.run_frames(535);
    let page2 = results(sys);

    format!("page1 pass/fail/total={page1}; pages1+2 cumulative={page2}")
}

/// `SpriteMaskingTestRom` — nine sprite-masking / dot-overflow tests, verdict drawn at the right edge as a
/// 32x8 glyph. Tests 4-8 print PASS/FAIL; tests 1-3 and 9 print a pair of tick/cross marks (one per
/// sub-case). The tick and cross share identical nametable cells, so the glyphs are classified by hashing
/// the rendered pixels — the framebuffer is the only channel that distinguishes them.
///
/// Runs in H32. The ROM's on-screen text says `Start` toggles H40/H32; in this core it is `C` that
/// toggles — an OPEN QUESTION recorded in `docs/2026-07-25-testrom-conformance.md`, deliberately NOT
/// "fixed" here.
fn scrape_vdp_sprite_masking(sys: &mut System) -> String {
    sys.run_frames(300);
    // Verdict-glyph classification, pinned from the rendered pixels (see doc).
    const TICK_TICK: u64 = 0xb498_5631_5ac3_a445;
    const TICK_CROSS: u64 = 0xa126_fa46_503f_8e4d;
    const PASS: u64 = 0x6609_4bba_88cb_93ed;
    const FAIL: u64 = 0x1f88_0fb1_901c_cfe5;

    let mut out = vec!["H32:".to_string()];
    for (n, row) in (6u16..15).enumerate() {
        let h = block_hash(sys, 216, 248, row * 8, row * 8 + 8);
        let label = match h {
            TICK_TICK => "TICK/TICK".to_string(),
            TICK_CROSS => "TICK/CROSS".to_string(),
            PASS => "PASS".to_string(),
            FAIL => "FAIL".to_string(),
            other => format!("UNKNOWN-GLYPH(0x{other:016x})"),
        };
        out.push(format!("{}={label}", n + 1));
    }
    out.join(" ")
}

/// The purely-visual ROMs: no machine-readable verdict, so the frame hash is a **regression baseline
/// only** — it says "the picture did not change", never "the picture is right". `note` carries a
/// NOT-RENDERABLE caveat where our capture model structurally cannot show what the ROM demonstrates.
fn scrape_visual(sys: &mut System, note: &str) -> String {
    sys.run_frames(120);
    format!("{note}frame_hash=0x{:016x}", frame_hash(sys))
}

// ---------------------------------------------------------------------------------------------------
// The scorecard
// ---------------------------------------------------------------------------------------------------

fn scrape(name: &str) -> Option<String> {
    let mut sys = boot(name)?;
    Some(match name {
        "m68k_bcd" => scrape_m68k_bcd(&mut sys),
        "m68k_memory_test" => scrape_m68k_memory_test(&mut sys),
        "io_sample" => scrape_io_sample(&mut sys),
        "m68k_illegal" => scrape_m68k_illegal(&mut sys),
        "vdp_port_access" => scrape_vdp_port_access(&mut sys),
        "vdp_sprite_masking" => scrape_vdp_sprite_masking(&mut sys),
        "cram_flicker" => scrape_visual(&mut sys, "NOT-RENDERABLE (border-only rendering) "),
        "direct_color_dma" => scrape_visual(&mut sys, "NOT-RENDERABLE (sub-scanline CRAM) "),
        "color_1536" => scrape_visual(
            &mut sys,
            "VISUAL-BASELINE ", // end-of-frame capture caveat appended below
        ),
        _ => scrape_visual(&mut sys, "VISUAL-BASELINE "),
    })
}

/// The instrument. Builds the whole scorecard and compares it to [`BASELINE`] in ONE assert, so a diff
/// lists every ROM whose outcome moved.
#[test]
fn testrom_conformance_scorecard() {
    let mut scorecard: Vec<(&str, String)> = Vec::new();
    for name in ROMS {
        if let Some(outcome) = scrape(name) {
            scorecard.push((name, outcome));
        }
    }

    // `color_1536` carries an extra capture caveat in the pin (mid-frame effects are lost at
    // end-of-frame capture); apply it here so the scraper stays uniform.
    for entry in scorecard.iter_mut() {
        if entry.0 == "color_1536" {
            entry.1.push_str(" (end-of-frame capture only)");
        }
    }

    let ran = scorecard.len();
    let present = ROMS
        .iter()
        .filter(|n| Path::new(&rom_path(n)).exists())
        .count();
    eprintln!("conformance: {ran}/{} ROMs ran", ROMS.len());
    for (name, outcome) in &scorecard {
        eprintln!("  {name:<20} {outcome}");
    }

    // Count guard: every vendored ROM present on disk MUST have produced a row. A ROM that boots into a
    // panic-free nothing, or a scraper that silently bails, cannot hide behind the soft-skip path.
    assert_eq!(
        ran, present,
        "{present} vendored ROMs are on disk but only {ran} produced a scorecard row"
    );

    if ran == 0 {
        eprintln!("SKIP: no vendored test ROMs at all — run tools/fetch-testroms.sh");
        return;
    }

    let expected: Vec<(&str, String)> = BASELINE
        .iter()
        .filter(|(n, _)| scorecard.iter().any(|(s, _)| s == n))
        .map(|(n, o)| (*n, squeeze(o)))
        .collect();
    let actual: Vec<(&str, String)> = scorecard.iter().map(|(n, o)| (*n, squeeze(o))).collect();

    assert_eq!(
        actual, expected,
        "test-ROM conformance moved. This harness is NON-GATING: a diff is information, not \
         automatically a failure to 'fix'. Confirm the change is intended, then update BASELINE and \
         docs/2026-07-25-testrom-conformance.md together."
    );
}

/// Structural guard: the pinned baseline must cover exactly the ROM list (which mirrors
/// `tools/fetch-testroms.sh`). Without this, dropping a ROM from `ROMS` would shrink the scorecard AND
/// the filtered baseline in lockstep and still pass.
#[test]
fn baseline_covers_every_rom() {
    assert_eq!(
        BASELINE.len(),
        ROMS.len(),
        "BASELINE has {} entries for {} ROMs",
        BASELINE.len(),
        ROMS.len()
    );
    for name in ROMS {
        assert!(
            BASELINE.iter().any(|(n, _)| n == name),
            "ROM {name} has no pinned baseline entry"
        );
    }
    let mut sorted = ROMS.to_vec();
    sorted.sort_unstable();
    assert_eq!(
        sorted, ROMS,
        "keep ROMS sorted so the scorecard diff is stable"
    );
}

/// CI guard: the vendored test ROMs MUST be present under CI, so a fetch failure fails LOUDLY instead of
/// every ROM skipping cleanly and the scorecard passing VACUOUSLY. Locally (no `CI` env var) this is a
/// no-op — fetching is optional for dev and the per-ROM `SKIP` guards keep the suite friendly.
#[test]
fn vendor_data_present_when_running_in_ci() {
    if std::env::var_os("CI").is_none() {
        return; // local dev: the per-ROM SKIP guards handle an absent vendor dir
    }
    assert!(
        Path::new(VENDOR_DIR).exists(),
        "CI: vendored test-ROM dir {VENDOR_DIR} is missing — tools/fetch-testroms.sh must run before \
         the test job (a missing dir makes the whole scorecard skip and pass vacuously)"
    );
    for name in ROMS {
        let p = rom_path(name);
        assert!(
            Path::new(&p).exists(),
            "CI: vendored test ROM {p} is missing — tools/fetch-testroms.sh did not fetch the full corpus"
        );
    }
}
