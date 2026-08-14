//! End-to-end check of [`oracle_core::symbols`] against the **real** artifacts Aeon's `sigil` build emits.
//!
//! The synthetic fixtures in the module's own unit tests encode the format as we understand it; this file
//! is the only place that checks our understanding against the genuine 2,129-symbol `s4.lst` and the
//! 696,836-byte `s4.bin` it describes. That makes it the only true end-to-end evidence available — and also
//! the reason it cannot be a hard dependency.
//!
//! # Why every test here skips instead of failing
//!
//! These files live in a **different git repository** (`aeon/`), are build outputs (not checked in), and
//! are absent on CI and in any fresh clone of this repo. A test that failed when they were missing would
//! make the suite red for everyone who has not built Aeon locally. So each test resolves its inputs first
//! and returns early with a printed note when they are not there.
//!
//! The path is overridable with `ORACLE_AEON_DIR` for a checkout in a different location.

use oracle_core::symbols::{
    rom_declared_end, AddrSpace, BindingFault, RomBinding, SymbolTable, TableSource,
};
use std::path::PathBuf;

/// Where Aeon's build outputs live. Defaults to the sibling checkout this project is developed against.
fn aeon_dir() -> PathBuf {
    std::env::var("ORACLE_AEON_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/home/volence/sonic_hacks/aeon"))
}

/// Read one Aeon build artifact, or `None` (with a printed note) when it is not present.
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

/// Read one Aeon `.lst` as text, or `None` when absent.
fn listing(name: &str) -> Option<String> {
    artifact(name).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// The real `s4.lst` parses completely: every line accounted for, and the count the file's own footer
/// declares is the count we produced. A silent shortfall here would mean the format drifted.
#[test]
fn real_s4_lst_parses_completely() {
    let Some(text) = listing("s4.lst") else {
        return;
    };
    let t = SymbolTable::parse(&text).expect("s4.lst must parse");

    assert_eq!(t.source(), TableSource::SymbolTable);
    assert_eq!(
        t.matches_declared_count(),
        Some(true),
        "parsed {} symbols but the footer declares {:?}",
        t.len(),
        t.declared_count()
    );
    assert_eq!(
        t.skipped_lines(),
        0,
        "unrecognised rows in the symbol table section"
    );
    assert_eq!(
        t.declared_unused(),
        Some(0),
        "s4.lst reports no unused symbols"
    );
    // Sanity floor rather than an exact pin: the count moves every time the game gains a label.
    assert!(
        t.len() > 1000,
        "only {} symbols — is the file truncated?",
        t.len()
    );
    println!("s4.lst: {} symbols, {} modules", t.len(), t.modules().len());
}

/// Both halves of the file describe the same symbol list, so parsing the body lines alone must agree with
/// parsing the `Symbol Table` section — on every one of the ~2,129 entries.
#[test]
fn real_s4_lst_body_and_table_halves_agree() {
    let Some(text) = listing("s4.lst") else {
        return;
    };
    let full = SymbolTable::parse(&text).expect("s4.lst must parse");
    let body_only = text.split("  Symbol Table").next().expect("body half");
    let body = SymbolTable::parse(body_only).expect("body half must parse");

    assert_eq!(body.source(), TableSource::BodyLines);
    assert_eq!(
        body.len(),
        full.len(),
        "the two halves list different symbol counts"
    );
    for s in body.symbols() {
        let f = full
            .by_name(&s.name)
            .unwrap_or_else(|| panic!("{} missing from table half", s.name));
        assert_eq!(
            f.raw_addr, s.raw_addr,
            "address disagreement for {}",
            s.name
        );
    }
}

/// Trap 1 + trap 2 on real data: RAM symbols are plain 8-hex `FFFFxxxx`, there are *many* of them (Aeon's
/// own `s4budget.py` reports `RAM: 0 bytes` because its regex still expects the old 48-bit sign-extended
/// form), and each masks to the 24-bit work-RAM address the bus actually carries.
#[test]
fn real_s4_lst_has_ram_symbols_that_mask_to_the_bus_address() {
    let Some(text) = listing("s4.lst") else {
        return;
    };
    let t = SymbolTable::parse(&text).expect("s4.lst must parse");

    let ram: Vec<_> = t
        .symbols()
        .iter()
        .filter(|s| s.space() == AddrSpace::Ram)
        .collect();
    assert!(
        !ram.is_empty(),
        "no RAM symbols found — the `FFFFxxxx` form was not recognised"
    );
    println!("s4.lst: {} RAM symbols (s4budget.py reports 0)", ram.len());

    for s in &ram {
        assert_eq!(
            s.raw_addr >> 16,
            0xFFFF,
            "{} is not the 8-hex FFFFxxxx form",
            s.name
        );
        assert_eq!(s.addr, s.raw_addr & 0x00FF_FFFF);
        assert!((0x00E0_0000..=0x00FF_FFFF).contains(&s.addr));
    }

    // The canonical example from the recon doc, both spellings.
    let p = t.by_name("Player_1").expect("Player_1 must exist");
    assert_eq!(p.raw_addr, 0xFFFF_8CFA);
    assert_eq!(p.addr, 0x00FF_8CFA);
    assert_eq!(t.resolve(0xFFFF_8CFA).unwrap().symbol.name, "Player_1");
    assert_eq!(t.resolve(0x00FF_8CFA).unwrap().symbol.name, "Player_1");
}

/// Trap 3 on real data: the type column is `C` for **every** row, RAM variables included, so it cannot be
/// used to tell code from data. Any logic keyed on it would be reading a constant.
#[test]
fn real_s4_lst_type_column_is_uniformly_code() {
    let Some(text) = listing("s4.lst") else {
        return;
    };
    let t = SymbolTable::parse(&text).expect("s4.lst must parse");
    let code = t
        .symbols()
        .iter()
        .filter(|s| s.kind == oracle_core::symbols::SymbolKind::Code)
        .count();
    assert_eq!(
        code,
        t.len(),
        "an equate appeared — the recon said there are none"
    );
    // …and yet plenty of those `C` rows are RAM.
    assert!(t.symbols().iter().any(|s| s.space() == AddrSpace::Ram));
}

/// The scope tree is real: the boot module's proc-locals demangle to `EntryPoint.<local>`, and a PC in the
/// middle of the entry point resolves to the *local* label rather than to `EntryPoint` plus a blind offset.
#[test]
fn real_s4_lst_scope_tree_resolves_a_pc_to_a_proc_local() {
    let Some(text) = listing("s4.lst") else {
        return;
    };
    let t = SymbolTable::parse(&text).expect("s4.lst must parse");

    let wait = t
        .by_name("$engine.boot$EntryPoint$wait_dma")
        .expect("the mangled proc-local must survive parsing");
    assert_eq!(wait.demangled, "EntryPoint.wait_dma");
    assert_eq!(wait.scope().module, Some("engine.boot"));
    assert_eq!(wait.scope().parent, Some("EntryPoint"));
    assert_eq!(wait.scope().local, "wait_dma");
    assert_eq!(wait.addr, 0x214);

    // A PC two bytes into that label lands on a local, not on `EntryPoint` + $16.
    let r = t.resolve(wait.addr + 2).expect("must resolve");
    assert_eq!(r.symbol.addr, 0x214);
    assert_eq!(r.displacement, 2);
    assert!(
        r.symbol.demangled.starts_with("EntryPoint."),
        "expected a proc-local, got {}",
        r.symbol.demangled
    );
    assert!(t.modules().contains(&"engine.boot"));
    assert!(!t.symbols_in_module("engine.boot").is_empty());
    println!("s4.lst: PC ${:06X} resolves to {}", wait.addr + 2, r);
}

/// Trap 4, the whole point of the binding check: `s4.lst` accepts `s4.bin`, and the two shape crosses are
/// **refused in both directions**. Cross-shape resolution would be confidently wrong for 92.6% of the
/// symbols the two listings share.
#[test]
fn real_shape_binding_accepts_the_matching_rom_and_refuses_the_crosses() {
    let (Some(rel_lst), Some(dbg_lst)) = (listing("s4.lst"), listing("s4.debug.lst")) else {
        return;
    };
    let (Some(rel_rom), Some(dbg_rom)) = (artifact("s4.bin"), artifact("s4.debug.bin")) else {
        return;
    };
    let rel = SymbolTable::parse(&rel_lst).expect("s4.lst");
    let dbg = SymbolTable::parse(&dbg_lst).expect("s4.debug.lst");

    match rel.validate_against_rom(&rel_rom) {
        RomBinding::Match {
            appendix_offset,
            appendix_len,
        } => {
            println!(
                "s4.lst/s4.bin: deb2 appendix at ${appendix_offset:06X}, {appendix_len} bytes"
            );
            assert_eq!(appendix_offset, 0x000A_11F0);
        }
        other => panic!("s4.lst must bind to s4.bin, got {other:?}"),
    }
    match dbg.validate_against_rom(&dbg_rom) {
        RomBinding::Match {
            appendix_offset, ..
        } => assert_eq!(appendix_offset, 0x000A_30B0),
        other => panic!("s4.debug.lst must bind to s4.debug.bin, got {other:?}"),
    }

    // The crosses. Both must be a positive Mismatch — not merely Indeterminate.
    assert!(
        matches!(
            dbg.validate_against_rom(&rel_rom),
            RomBinding::Mismatch(BindingFault::NoAppendixMagic { .. })
        ),
        "s4.debug.lst against s4.bin must be refused"
    );
    assert!(
        matches!(
            rel.validate_against_rom(&dbg_rom),
            RomBinding::Mismatch(BindingFault::NoAppendixMagic { .. })
        ),
        "s4.lst against s4.debug.bin must be refused"
    );

    // The corroborating header datum: sigil re-fixes `$1A4` after appending, so it spans the appendix.
    assert_eq!(rom_declared_end(&rel_rom), Some(rel_rom.len() as u32 - 1));
    assert_eq!(rom_declared_end(&dbg_rom), Some(dbg_rom.len() as u32 - 1));
}

/// The documented residual limit, asserted so it cannot silently stop being true: `demo.lst` and
/// `demo.debug.lst` declare the **same** `EndOfRom`, so the appendix probe cannot separate them even
/// though 1,197 of their shared symbols sit at different addresses. Recorded here as a known gap, not as
/// a defect of the check.
#[test]
fn real_demo_pair_documents_the_binding_checks_residual_limit() {
    let (Some(a), Some(b)) = (listing("demo.lst"), listing("demo.debug.lst")) else {
        return;
    };
    let (ta, tb) = (
        SymbolTable::parse(&a).unwrap(),
        SymbolTable::parse(&b).unwrap(),
    );
    let (ea, eb) = (ta.address_of("EndOfRom"), tb.address_of("EndOfRom"));
    assert_eq!(
        ea, eb,
        "if this ever diverges the demo shapes became separable — update the docs"
    );

    let shared_but_moved = ta
        .symbols()
        .iter()
        .filter(|s| tb.by_name(&s.name).is_some_and(|o| o.addr != s.addr))
        .count();
    assert!(
        shared_but_moved > 0,
        "the two demo shapes must actually differ, else there is no limit to document"
    );
    println!(
        "demo shapes share EndOfRom ${:06X} yet {shared_but_moved} shared symbols moved — the probe cannot separate them",
        ea.unwrap()
    );
}
