//! The frozen listings' **shape**, checked against its own manifest — and the blind spots named out loud.
//!
//! # The failure this exists for
//!
//! `fixtures/aeon/*.lst` are committed copies of another lane's build output. They drift from the build
//! they were taken from, and *the drift is exactly where new faults live*. `DIMENSIONS.tsv`'s header
//! carries every sighting; three of them are about namespaces and sections, each re-measured here rather
//! than taken on report (the live-effects panel's is the next section's subject):
//!
//! * the frozen listings publish **zero** equates under `ObjSub_`; aeon's build publishes eight
//!   (`ObjSub_Spring__{Up,Right,Down,Left}_{Red,Yellow}`). The spring-subtype picker
//!   (`oracle-frontend/src/spawn.rs`) works *entirely* by prefix-searching that namespace, so its whole
//!   coverage is synthetic — and nothing in the tree said so;
//! * frozen `s4.debug.lst` declares **five** `ObjDef_` archetypes where aeon's declares **six**, the
//!   missing one being `ObjDef_Spring` — the archetype that picker is about;
//! * no frozen listing carries a `Phase Table` at all, so the phase parse's row handling is untouched by
//!   the default path.
//!
//! # A dimension can move without ever becoming absent — so presence is only half a probe
//!
//! The live-effects panel (`oracle-player/src/effects.rs`) resolves twelve names and **refuses to write
//! when the listing's address disagrees with the one its note recorded** (`Channel::drift`). Against the
//! frozen listings that refusal fires on every channel, and for two different reasons at once:
//!
//! * **nine are present at a different address.** Frozen minus note: the parallax block is a uniform
//!   `-4`, the raster block `-0x10E`, `BgAnim_LastStep` `-0x128`. A probe that could only answer
//!   *present / absent* would report full coverage of a panel that, on these bytes, refuses every gesture
//!   it offers. That is why `symbol_addr` exists alongside `symbol_present`.
//! * **three are absent outright** — `BgAnim_Table_Ptr` (the bands channel's whole selector, missing from
//!   the frozen *debug* listing too), `BgAnim_Table_Empty` (its off target) and `Debug_Lab_Index` (the
//!   cursor `forbidden` refuses writes to). Those are recorded as `absent`, which is an **expected
//!   value**: a name becoming present reddens this gate exactly as a name vanishing does.
//!
//! ⚑ `$FFFF8BD6` — where the note puts `Raster_Program` — *is* a symbol in these listings: it is
//! `Raster_Active_Buf`. So the raster drift is a restructure of `Raster_State`, not a slide, and this is
//! the reason `symbol_addr` resolves **by name and reports the address** rather than the reverse: an
//! address-first probe would have found a plausible wrong answer here.
//!
//! The fixtures are the stale side — the listings were built before the note's commit — and this gate's
//! job is to record what the frozen bytes say, not to adjudicate that. **Whether to refresh them is the
//! currency question and belongs to `tools/aeon_pin_report.py` and to the owner, so these rows carry the
//! frozen values and the gate stays green.** Pinning the *note's* addresses here would convert a recovery
//! gate into a permanently-red currency gate, which the section below forbids in terms.
//!
//! [`crate::aeon_pin`]-style byte pinning cannot see any of this: the bytes are exactly the bytes we
//! recorded, and the *shape* they carry is what moved. This file records the shape.
//!
//! # Which question this asks — the same one `aeon_pin.rs` asks, one level up
//!
//! **Recovery**: *are our shapes the shapes we recorded?* A fact about this repository alone, correctly
//! asked at the pinning revision, hermetic, and therefore a gate. It goes red when the pin moves without
//! `DIMENSIONS.tsv` moving with it — the dimension-resolution form of the "someone dropped newer bytes in
//! to make a red test green" failure `PROVENANCE.md` forbids in terms.
//!
//! It is **not** a currency check. Whether aeon has published a dimension we lack must be asked at a ref
//! in aeon's object store, is a statement about somebody else's lane, and lives outside the suite in the
//! non-gating `tools/aeon_pin_report.py`. A gate that reddens because someone else moved puts the whole
//! gradient behind bending our side until it passes.
//!
//! # Why a recorded `0` is worth anything at all
//!
//! A row reading `0` is the interesting kind of row — it is a *recorded blind spot*, and `relied_on_by`
//! names who is running blind on it. But a `0` produced by a **broken probe** and a `0` produced by an
//! **absent dimension** are the same artifact, and the first would quietly certify the second forever.
//! So [`every_probe_kind_can_report_presence`] is a positive control: each probe kind is run against a
//! synthetic listing that *does* carry the dimension and must return the non-zero answer. Every `0` in
//! the manifest is only as good as that test, which is why it is not optional.
//!
//! # What was mutated to prove the above, so the next reader need not re-derive it
//!
//! Each mutation was made on disk and restored from a committed baseline; each had to go red **on the row
//! under test**, not merely go red.
//!
//! | mutation | went red as |
//! |---|---|
//! | a present `symbol_addr` row given the note's address instead of the frozen one | 1 of 84 rows, naming that row, both values in `$HEX` |
//! | `BgAnim_Table_Ptr` **added** to `s4.lst` (trailer bumped with it) | 2 of 84 — the `absent` row it was pinned in *and* `symbol_prefix_count(BgAnim_Table)` 1→2. This is the proof that an absence here is an expected value: a symbol *becoming present* reddens the gate. |
//! | `measure` returning `Symbol::addr` (24-bit door form) instead of `raw_addr` | 28 rows **and** [`every_probe_kind_can_report_presence`] — so the control witnesses `symbol_addr` rather than passing beside it |
//! | one `symbol_addr` arm deleted from the control | the uncovered-probe panic, naming `symbol_addr`/`Debug_Lab_Index` — checked, not assumed |
//! | an address row written in decimal | the `$HEX` spelling assert, in all three tests |
//!
//! # The path is the frozen directory, deliberately — not `ORACLE_AEON_DIR`
//!
//! The manifest describes *these committed bytes*. Under an override the rest of the suite reads a live
//! build whose shape is a different question, so measuring it against this manifest would be a guaranteed
//! red that means nothing. This file reads `fixtures/aeon/` directly and says loudly when an override is
//! set that the rest of the suite is not running against what it just measured.

use oracle_core::symbols::SymbolTable;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// The committed fixture directory. Not `ORACLE_AEON_DIR` — see the module docs.
fn frozen_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/aeon"))
}

/// Write where libtest's per-thread stdout capture cannot swallow it — the house pattern, on fd 2.
fn loud(msg: String) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{msg}");
}

/// The manifest's column contract. A change here must change `DIMENSIONS.tsv`'s header in the same edit.
const HEADER: [&str; 6] = ["file", "probe", "arg", "frozen", "upstream", "relied_on_by"];

/// One row of `DIMENSIONS.tsv`.
#[derive(Debug, Clone)]
struct DimRow {
    file: String,
    probe: String,
    arg: String,
    /// `Some(n)` for an integer answer; `None` for `absent`.
    frozen: Option<usize>,
    #[allow(dead_code)]
    upstream: String,
    relied_on_by: String,
}

impl DimRow {
    /// How the row reads back in a message — the manifest's own spelling, so a failure can be grepped
    /// straight into the file.
    fn ident(&self) -> String {
        if self.arg == "-" {
            format!("{}:{}", self.file, self.probe)
        } else {
            format!("{}:{}({})", self.file, self.probe, self.arg)
        }
    }

    /// Whether this row's value is an ADDRESS rather than a count. Decided by the probe, not by how the
    /// literal happens to be written, so a measured value and a manifest value are always printed in the
    /// same spelling — otherwise a failure would read `manifest says $FFFF88E8, the frozen bytes give
    /// 4294936300` and nobody could diff it.
    fn is_addr(&self) -> bool {
        self.probe == "symbol_addr"
    }

    /// Format a value in this row's own spelling: `$HEX` for an address, decimal for a count, `absent`
    /// for nothing.
    fn fmt(&self, v: Option<usize>) -> String {
        match v {
            Some(n) if self.is_addr() => format!("${n:X}"),
            Some(n) => n.to_string(),
            None => "absent".to_string(),
        }
    }

    fn frozen_str(&self) -> String {
        self.fmt(self.frozen)
    }
}

fn read_manifest() -> Vec<DimRow> {
    let path = frozen_dir().join("DIMENSIONS.tsv");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("DIMENSIONS.tsv unreadable at {}: {e}", path.display()));

    let mut rows = Vec::new();
    let mut header_seen = false;
    for line in text.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(
            f.len(),
            HEADER.len(),
            "DIMENSIONS.tsv: expected {} tab-separated columns, got {} in {line:?}",
            HEADER.len(),
            f.len()
        );
        if !header_seen {
            assert_eq!(f.as_slice(), HEADER, "DIMENSIONS.tsv header changed");
            header_seen = true;
            continue;
        }
        let frozen = match f[3] {
            "absent" => None,
            // `$FFFF88E8` — the listing's own spelling for an address. Decimal here would be unreadable
            // and unmatchable against the file it was copied from.
            n if n.starts_with('$') => Some(
                usize::from_str_radix(&n[1..], 16)
                    .unwrap_or_else(|_| panic!("`frozen` is not hexadecimal after the `$`: {n:?}")),
            ),
            n => Some(n.parse::<usize>().unwrap_or_else(|_| {
                panic!("`frozen` must be an integer, a `$hex` address or `absent`: {n:?}")
            })),
        };
        let row = DimRow {
            file: f[0].to_string(),
            probe: f[1].to_string(),
            arg: f[2].to_string(),
            frozen,
            upstream: f[4].to_string(),
            relied_on_by: f[5].to_string(),
        };
        // One spelling per kind, enforced rather than conventional: a decimal address row would still
        // measure correctly and would still print back in hex, so nothing would ever notice the drift
        // between the file and the messages it produces.
        assert!(
            !(row.is_addr() && frozen.is_some() && !f[3].starts_with('$')),
            "an address row must write its value as `$HEX`, not `{}`: {}",
            f[3],
            row.ident()
        );
        rows.push(row);
    }
    assert!(header_seen, "DIMENSIONS.tsv has no header row");
    assert!(!rows.is_empty(), "DIMENSIONS.tsv lists no dimensions");
    rows
}

/// Measure one dimension **through the API its consumers call**.
///
/// This is the whole reason the gate is worth more than a `grep` over the fixture: a dimension that is
/// present in the bytes but no longer reachable through `SymbolTable` is exactly as invisible to a
/// consumer as one that was never there, and only a measurement taken through the parser sees that.
fn measure(t: &SymbolTable, probe: &str, arg: &str) -> Option<usize> {
    match probe {
        // `with_prefix` returns one entry per symbol; consumers care about distinct names
        // (`spawn::archetypes` de-duplicates), so the manifest records distinct names.
        "symbol_prefix_count" => Some(
            t.with_prefix(arg)
                .iter()
                .map(|s| s.name.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
        ),
        // 1/0 rather than a bool, so a single integer column covers every probe. This is the probe for
        // a dimension that is one specific name rather than a namespace.
        "symbol_present" => Some(usize::from(t.by_name(arg).is_some())),
        // Presence AND location in one row. `raw_addr`, not `addr`: the 32-bit spelling the listing
        // itself writes and the one every consumer that carries a transcribed address compares against
        // (`oracle-player/src/effects.rs`'s `Channel::drift` is explicit that the 24-bit door form would
        // make every channel read as drifted). `None` here means the name is absent, which is exactly
        // what `symbol_present` would have called `0`, so this probe strictly subsumes that one.
        "symbol_addr" => t.by_name(arg).map(|s| s.raw_addr as usize),
        "equate_prefix_count" => Some(t.equates_with_prefix(arg).len()),
        "equate_rows" => t.equate_rows(),
        "phase_count" => {
            if t.has_phase_table() {
                // A section with no stated count is an older emitter, not an absence; record its rows.
                Some(t.phase_count().unwrap_or_else(|| t.phase_entries().len()))
            } else {
                None
            }
        }
        other => panic!("DIMENSIONS.tsv names an unknown probe `{other}`"),
    }
}

/// Every probe kind the manifest uses, in manifest order of first appearance.
fn probe_kinds(rows: &[DimRow]) -> Vec<String> {
    let mut seen = Vec::new();
    for r in rows {
        if !seen.contains(&r.probe) {
            seen.push(r.probe.clone());
        }
    }
    seen
}

/// A synthetic listing that carries **every** dimension the manifest probes for, so each probe has a
/// case in which it must answer non-trivially.
///
/// Deliberately hand-built rather than borrowed from the fixture: the fixture is the thing under
/// measurement, and a control taken from it could not distinguish a dead probe from an absent dimension.
/// The counts in the trailers are the real counts, so `is_intact` holds and the parser accepts it.
///
/// ⚑ The effects-panel addresses below are the ones **`oracle-player/src/effects.rs`'s note records**
/// (aeon `c4c5c3d8`), which are deliberately NOT the addresses the frozen listings give. So a
/// `symbol_addr` probe that had somehow started reading the fixture instead of the string it was handed
/// would return the frozen address here and fail, rather than agreeing for the wrong reason.
/// (`BgAnim_Table_Empty`'s is the one exception and is openly synthetic — the note predates that symbol
/// and records no address for it — which costs nothing, because the control's job is only to prove the
/// probe can find a name that is there.)
const CONTROL_LST: &str = "\
  Symbol Table (* = unused):
  --------------------------

 EntryPoint : 200 C |
 ObjDef_Ring : 1000 C |
 ObjDef_Spring : 1400 C |
 SoundTablesZ80_Head : 8000 C |
 Level_Height : FFFFEA72 C |
 Level_Width : FFFFEA70 C |
 Parallax_Current_Config : FFFF88EC C |
 Parallax_Target_Config : FFFF88F0 C |
 Parallax_Transition_Frames : FFFF88F4 C |
 Parallax_Snap_Pending : FFFF88F5 C |
 ParallaxConfig_Haze : 12C6C C |
 ParallaxConfig_OJZ_Default : 1267A C |
 Raster_Program : FFFF8BD6 C |
 Raster_Pending : FFFF8BDE C |
 Raster_Program_None : 881E C |
 EditorRaster_OJZ_Act1_ramp_probe : 14652 C |
 BgAnim_Table_Ptr : FFFFE91A C |
 BgAnim_LastStep : FFFF8F06 C |
 BgAnim_Table : 28BD4 C |
 BgAnim_Table_Empty : 28BE0 C |
 Debug_Lab_Index : FFFFEE0D C |

    21 symbols
    0 unused symbols

  Equate Table (name = value; values, not addresses):
  ---------------------------------------------------

EQU ObjSub_Spring__Up_Red = $00000000
EQU ObjSub_Spring__Up_Yellow = $00000002
EQU frame_count = $00000010

   3 equates

  Phase Table (every address above is a VMA):
  -------------------------------------------

PHASE-COUNT 1
PHASE SoundTablesZ80_Head VMA $00008000 LMA $000B8000
";

/// **The manifest is truthful about the frozen bytes.**
///
/// Red when the pin moves and the manifest does not — which is the dimension-resolution form of the
/// failure `PROVENANCE.md` forbids, and the one a byte pin structurally cannot see.
#[test]
fn manifest_matches_the_frozen_listings() {
    if let Ok(dir) = std::env::var("ORACLE_AEON_DIR") {
        loud(format!(
            "NOTE: ORACLE_AEON_DIR={dir} — the REST of the suite is reading that build. This test \
             measured fixtures/aeon/ regardless, because DIMENSIONS.tsv describes the frozen bytes."
        ));
    }

    let rows = read_manifest();
    let files: BTreeSet<&str> = rows.iter().map(|r| r.file.as_str()).collect();

    // Completeness, in the direction this file can be silent in. `aeon_pin.rs` already refuses an
    // artifact in the pinned directory with no `PIN.tsv` row, so a listing cannot arrive unpinned. But
    // a listing that IS pinned and has no row *here* would be measured for nothing at all, and this
    // gate would go green having said nothing about it — the exact "invisible by construction" shape
    // the manifest exists to end. So every `.lst` present must have at least one dimension row.
    let mut unprobed: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(frozen_dir()).expect("the frozen fixture directory must exist") {
        let name = entry.expect("readable entry").file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".lst") && !files.contains(name.as_ref()) {
            unprobed.push(name.into_owned());
        }
    }
    unprobed.sort();
    assert!(
        unprobed.is_empty(),
        "these frozen listings have no row in DIMENSIONS.tsv and are measured for nothing: {}\n\
         Add at least one dimension row each, or this gate is silent about them while looking green.",
        unprobed.join(", ")
    );

    // Parse each listing once; a per-row parse would be 24 parses of four files.
    let mut wrong: Vec<String> = Vec::new();
    for file in &files {
        let path = frozen_dir().join(file);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "DIMENSIONS.tsv names {} but it is unreadable: {e}",
                path.display()
            )
        });
        let t = SymbolTable::parse(&text)
            .unwrap_or_else(|e| panic!("{} must parse: {e:?}", path.display()));

        for r in rows.iter().filter(|r| r.file == *file) {
            let got = measure(&t, &r.probe, &r.arg);
            if got != r.frozen {
                wrong.push(format!(
                    "  {} — manifest says {}, the frozen bytes give {}",
                    r.ident(),
                    r.frozen_str(),
                    r.fmt(got)
                ));
            }
        }
    }

    assert!(
        wrong.is_empty(),
        "fixtures/aeon/DIMENSIONS.tsv disagrees with the frozen listings on {} of {} rows:\n{}\n\n\
         If the pin moved, move the manifest WITH it and re-read the blind-spot rows: a dimension \
         that appeared may now deserve real coverage, and one that vanished silently removed some.",
        wrong.len(),
        rows.len(),
        wrong.join("\n")
    );

    loud(format!(
        "fixtures/aeon/DIMENSIONS.tsv: {} rows over {} listings, all matching the frozen bytes.",
        rows.len(),
        files.len()
    ));
}

/// **The positive control that makes every recorded `0` mean something.**
///
/// Each probe kind is run against [`CONTROL_LST`], which carries the dimension, and must report it
/// present. Without this, a probe that had stopped working would report `0` on the fixture, agree with a
/// manifest row recording `0`, and certify a blind spot as a measurement forever.
///
/// The control is exercised at the manifest's own `arg` values, not at invented ones, so a manifest that
/// renamed a prefix cannot leave the control testing the old spelling.
#[test]
fn every_probe_kind_can_report_presence() {
    let rows = read_manifest();
    let t = SymbolTable::parse(CONTROL_LST).expect("the control listing must parse");

    // Anti-vacuity on the control itself: if it did not parse into a whole table, every assertion below
    // could pass for the wrong reason.
    assert!(
        t.is_intact(),
        "the control listing must be a whole, undamaged listing"
    );

    for kind in probe_kinds(&rows) {
        // The args the manifest actually uses for this probe kind.
        let args: BTreeSet<&str> = rows
            .iter()
            .filter(|r| r.probe == kind)
            .map(|r| r.arg.as_str())
            .collect();
        for arg in args {
            let got = measure(&t, &kind, arg);
            let expected: Option<usize> = match (kind.as_str(), arg) {
                // Derived from CONTROL_LST above, not copied from a nearby pin: it declares
                // `ObjDef_Ring` and `ObjDef_Spring`.
                ("symbol_prefix_count", "ObjDef_") => Some(2),
                // The effects panel's three option namespaces. Two scenes and one authored program:
                // asymmetric on purpose, because the number that matters is that each is the count of
                // ITS OWN prefix — a probe that ignored its argument would return 21 for all three.
                ("symbol_prefix_count", "ParallaxConfig_") => Some(2),
                ("symbol_prefix_count", "EditorRaster_") => Some(1),
                // `BgAnim_Table`, `BgAnim_Table_Empty` and `BgAnim_Table_Ptr` — the prefix genuinely
                // catches the channel's own live cell, which is why the panel draws that row without
                // offering it. The control has to carry that overlap or it would not be the real shape.
                ("symbol_prefix_count", "BgAnim_Table") => Some(3),
                // Declared above, so a probe that had stopped resolving names would report 0 here
                // and agree with the manifest's 0 on the fixture for the wrong reason.
                ("symbol_present", "Level_Width" | "Level_Height") => Some(1),
                // Every cell the effects panel resolves, at the address its note records — so a
                // `symbol_addr` row reading `absent` on the fixture is witnessed as a real absence, and
                // one reading an address is witnessed as a real read rather than a constant.
                ("symbol_addr", "Parallax_Current_Config") => Some(0xFFFF_88EC),
                ("symbol_addr", "Parallax_Target_Config") => Some(0xFFFF_88F0),
                ("symbol_addr", "Parallax_Transition_Frames") => Some(0xFFFF_88F4),
                ("symbol_addr", "Parallax_Snap_Pending") => Some(0xFFFF_88F5),
                ("symbol_addr", "Raster_Program") => Some(0xFFFF_8BD6),
                ("symbol_addr", "Raster_Pending") => Some(0xFFFF_8BDE),
                ("symbol_addr", "Raster_Program_None") => Some(0x0000_881E),
                ("symbol_addr", "BgAnim_Table_Ptr") => Some(0xFFFF_E91A),
                ("symbol_addr", "BgAnim_LastStep") => Some(0xFFFF_8F06),
                ("symbol_addr", "BgAnim_Table") => Some(0x0002_8BD4),
                ("symbol_addr", "BgAnim_Table_Empty") => Some(0x0002_8BE0),
                ("symbol_addr", "Debug_Lab_Index") => Some(0xFFFF_EE0D),
                // `ObjSub_Spring__Up_Red` and `ObjSub_Spring__Up_Yellow` — the third equate,
                // `frame_count`, is deliberately outside the prefix so a probe that ignored its
                // argument and returned "all equates" would fail here rather than pass.
                ("equate_prefix_count", "ObjSub_") => Some(2),
                // The `3 equates` trailer.
                ("equate_rows", "-") => Some(3),
                // `PHASE-COUNT 1`.
                ("phase_count", "-") => Some(1),
                (k, a) => panic!(
                    "DIMENSIONS.tsv uses probe `{k}` with arg `{a}`, which the positive control does \
                     not cover. Add a case to CONTROL_LST and here, or this probe's `0` rows in the \
                     manifest are unwitnessed."
                ),
            };
            assert_eq!(
                got, expected,
                "probe `{kind}({arg})` cannot see a dimension that IS present in the control \
                 listing. Every manifest row recording `0`/`absent` for this probe is therefore \
                 unwitnessed and must not be trusted."
            );
        }
    }
}

/// **The blind spots, named out loud, with who is blinded by each.**
///
/// This is the row's actual ask: a `0` buried in a manifest is not visibly different from coverage. The
/// inventory goes to stderr every run so a reader of the suite output cannot fail to know which
/// dimensions the default path does not exercise at all.
///
/// The guard is the half that cannot rot: an absent dimension may never be recorded without naming a
/// consumer, so nobody can quietly park a blind spot with an empty stake column.
#[test]
fn absent_dimensions_are_recorded_with_who_relies_on_them() {
    let rows = read_manifest();
    let absent: Vec<&DimRow> = rows
        .iter()
        .filter(|r| matches!(r.frozen, None | Some(0)))
        .collect();

    let unattributed: Vec<String> = absent
        .iter()
        .filter(|r| r.relied_on_by.trim().is_empty() || r.relied_on_by.trim() == "-")
        .map(|r| r.ident())
        .collect();
    assert!(
        unattributed.is_empty(),
        "these rows record a blind spot without naming who runs blind on it: {}\n\
         An absent dimension whose stake is unrecorded reads as 'nothing depends on this', which is \
         the state that made the last three faults invisible.",
        unattributed.join(", ")
    );

    if absent.is_empty() {
        loud(
            "fixtures/aeon/: no recorded blind spots — every probed dimension is present in the \
             frozen listings."
                .to_string(),
        );
        return;
    }

    loud(format!(
        "fixtures/aeon/: {} of {} probed dimensions are ABSENT from the frozen listings. Coverage \
         that names them is synthetic-only:",
        absent.len(),
        rows.len()
    ));
    for r in &absent {
        loud(format!(
            "  {:<44} {:<7} blind: {}",
            r.ident(),
            r.frozen_str(),
            r.relied_on_by
        ));
    }
    loud(
        "  (currency — has aeon published any of these since? — is `tools/aeon_pin_report.py`, \
         never this gate.)"
            .to_string(),
    );
}
