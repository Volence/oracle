//! End-to-end check of [`oracle_core::symbols`] against a **real stock-AS listing** — `s1disasm`'s
//! `sonic.lst` and the ROM it builds.
//!
//! `symbols_real_lst.rs` is the same evidence for the *sigil* dialect. This file is the other producer:
//! the AS macro assembler itself, whose output the classic Sonic disassemblies emit and which Aurora's
//! classic loop loads. The two dialects differ in three ways that each silently drop symbols (see the
//! module docs on `symbols`), and the synthetic `AS_FIXTURE` in the unit tests encodes our understanding
//! of them — this file is the only place that checks that understanding against 12,410 genuine rows.
//!
//! # Why every test here skips instead of failing
//!
//! `sonic.lst` and the built ROM live in a **different git repository** (`s1disasm/`) and are build
//! outputs, not checked in — absent on CI and in any fresh clone. A test that failed when they were
//! missing would make the suite red for everyone who has not run `lua build.lua` there. So each test
//! resolves its inputs first and returns early with a printed note. The path is overridable with
//! `ORACLE_S1DISASM_DIR`.
//!
//! The ROM is deliberately **not vendored**: it is a build of a commercial game's disassembly, it is
//! 551,288 bytes, and reading it from the sibling checkout costs nothing this suite needs.

use oracle_core::symbols::{
    rom_declared_end, AddrSpace, Indeterminate, RomBinding, SymbolKind, SymbolTable, TableSource,
    BUS_ADDR_MASK,
};
use std::path::PathBuf;

/// Where the S1 disassembly's build outputs live.
fn s1_dir() -> PathBuf {
    std::env::var("ORACLE_S1DISASM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/home/volence/sonic_hacks/s1disasm"))
}

/// Read one `s1disasm` build artifact, or `None` (with a loud printed note) when it is not present.
fn artifact(name: &str) -> Option<Vec<u8>> {
    let p = s1_dir().join(name);
    match std::fs::read(&p) {
        Ok(b) => Some(b),
        Err(_) => {
            println!(
                "SKIP: {} not present. Build it with `cd {} && lua build.lua` \
                 (non-destructive, ~600ms, artifacts are gitignored), or point ORACLE_S1DISASM_DIR \
                 at a checkout that has it.",
                p.display(),
                s1_dir().display()
            );
            None
        }
    }
}

fn listing() -> Option<String> {
    artifact("sonic.lst").map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// The real `sonic.lst` parses completely: every row accounted for, and **both** of the file's own footers
/// reconcile. This is the test the three dialect changes exist to make pass — before them 4,059 of 12,410
/// rows parsed, and every one of the 932 sign-extended RAM addresses was among the losses.
#[test]
fn real_sonic_lst_parses_completely() {
    let Some(text) = listing() else { return };
    let t = SymbolTable::parse(&text).expect("sonic.lst must parse");

    assert_eq!(t.source(), TableSource::SymbolTable);
    assert_eq!(
        t.skipped_lines(),
        0,
        "unrecognised rows in a healthy AS symbol table"
    );
    // The `N symbols` footer counts AS's string/float pseudo-symbols, so the reconciliation is
    // ingested + address-less. Both terms are asserted so a compensating error cannot hide.
    assert_eq!(t.declared_count(), Some(12_410));
    assert_eq!(
        t.non_address_rows(),
        4 + 1,
        "ARCHITECTURE/CONSTPI/DATE/MOMCPUNAME, and TIME whose value contains a space"
    );
    assert_eq!(t.len(), 12_405);
    assert_eq!(t.matches_declared_count(), Some(true));
    // …and so does `N unused symbols`: all five address-less rows are `*`-marked.
    assert_eq!(t.declared_unused(), Some(1_573));
    assert_eq!(t.non_address_unused(), 5);
    assert_eq!(
        t.declared_unused(),
        Some(t.symbols().iter().filter(|s| s.unused).count() + t.non_address_unused())
    );
    assert!(
        t.is_intact(),
        "a freshly built sonic.lst must read as a whole file — the frontend refuses one that does not"
    );
    println!(
        "sonic.lst: {} symbols ingested, {} address-less rows, {} skipped",
        t.len(),
        t.non_address_rows(),
        t.skipped_lines()
    );
}

/// **The measured type split**, which the demand side asked for by name.
///
/// The fear was that S1's RAM labels are equ-defined and would be lost to a skip-them-wholesale rule.
/// Half true, and the half that is true is why the ruling is forward-only rather than either extreme:
/// `v_player` *is* a `-` row, `f_debugmode` is a `C` row, and the two sit in the same RAM.
#[test]
fn real_sonic_lst_type_split_is_what_the_ruling_assumes() {
    let Some(text) = listing() else { return };
    let t = SymbolTable::parse(&text).expect("sonic.lst must parse");

    let code = t
        .symbols()
        .iter()
        .filter(|s| s.kind == SymbolKind::Code)
        .count();
    let equ = t.len() - code;
    // 2,232 rows carry `-` in the file; five of those are the address-less metadata rows, which are
    // consumed rather than ingested, so 2,227 reach the table. 10,178 + 2,227 == the 12,405 ingested.
    assert_eq!(code, 10_178);
    assert_eq!(equ, 2_227);
    assert_eq!(code + equ, t.len());

    // The two named symbols, and the mixed spelling that makes the type column useless as a RAM test.
    let v_player = t.by_name("v_player").expect("v_player is in the listing");
    assert_eq!(v_player.kind, SymbolKind::Equate, "v_player is equ-defined");
    let f_debugmode = t
        .by_name("f_debugmode")
        .expect("f_debugmode is in the listing");
    assert_eq!(f_debugmode.kind, SymbolKind::Code, "f_debugmode is a label");
    assert_eq!(v_player.space(), AddrSpace::Ram);
    assert_eq!(f_debugmode.space(), AddrSpace::Ram);

    // Why the rule is forward-*only*: the overwhelming majority of `-` rows are not addresses at all, so
    // admitting them to reverse lookup would flood the low addresses with constants.
    let equ_below_ram = t
        .symbols()
        .iter()
        .filter(|s| s.kind == SymbolKind::Equate && s.space() != AddrSpace::Ram)
        .count();
    assert!(
        equ_below_ram > 2_000,
        "only {equ_below_ram} non-RAM equ rows — the pollution argument would not hold"
    );
    println!(
        "sonic.lst type split: {code} C, {equ} `-` ({equ_below_ram} of the `-` rows are not RAM); \
         v_player=`-`, f_debugmode=`C`"
    );
}

/// The 48-bit sign-extended spelling, on the real file. Every one of these overflows `u32` and was
/// dropped before; each is a RAM address, and RAM is all the classic loop touches.
#[test]
fn real_sonic_lst_sign_extended_ram_addresses_reach_the_bus() {
    let Some(text) = listing() else { return };
    let t = SymbolTable::parse(&text).expect("sonic.lst must parse");

    // The three the demand side named, at both widths.
    assert_eq!(t.address_of("v_player"), Some(0x00FF_D000));
    assert_eq!(t.address_of("f_debugmode"), Some(0x00FF_FFFA));
    assert_eq!(t.address_of("v_palette_line_2"), Some(0x00FF_FB20));

    let ram: Vec<_> = t
        .symbols()
        .iter()
        .filter(|s| s.space() == AddrSpace::Ram)
        .collect();
    assert!(
        ram.len() > 900,
        "only {} RAM symbols — the sign-extended rows are being dropped again",
        ram.len()
    );
    for s in &ram {
        assert_eq!(
            s.addr,
            s.raw_addr & BUS_ADDR_MASK,
            "{} broke the raw/masked invariant",
            s.name
        );
    }
    // The specific spelling this test is named for: sixteen hex digits, `u32`-overflowing, truncated to
    // the conventional 32-bit form. (Not every RAM row uses it — S1 also writes plain `FF0000` — so this
    // is asserted on the sign-extended ones rather than swept over all of RAM.)
    assert_eq!(
        t.by_name("v_player").map(|s| s.raw_addr),
        Some(0xFFFF_D000),
        "FFFFFFFFFFFFD000 must truncate to the 32-bit spelling"
    );
    println!(
        "sonic.lst: {} RAM symbols recovered from the 48-bit spelling",
        ram.len()
    );
}

/// **The forward-only ruling, swept over every equ row in the real file.** Not a sampled check: all 2,231
/// must resolve name→value and none may answer value→name.
#[test]
fn real_sonic_lst_equ_rows_never_answer_in_reverse() {
    let Some(text) = listing() else { return };
    let t = SymbolTable::parse(&text).expect("sonic.lst must parse");

    let equ: Vec<_> = t
        .symbols()
        .iter()
        .filter(|s| s.kind == SymbolKind::Equate)
        .cloned()
        .collect();
    assert!(!equ.is_empty(), "no equ rows — the sweep would be vacuous");
    for s in &equ {
        // Forward: resolves, to the masked value.
        assert_eq!(
            t.address_of(&s.name),
            Some(s.addr),
            "{} lost its forward resolution",
            s.name
        );
        assert!(!s.resolves_in_reverse());
        // Reverse: never, in either query.
        assert!(
            t.symbols_at(s.addr).iter().all(|r| r.name != s.name),
            "{} answered an exact addr->name query",
            s.name
        );
        if let Some(r) = t.resolve(s.addr) {
            assert_ne!(
                r.symbol.name, s.name,
                "{} answered nearest-preceding",
                s.name
            );
        }
    }
    // The concrete failure the rule prevents: a tiny constant naming a low address.
    let tiny = equ
        .iter()
        .find(|s| s.addr == 8)
        .expect("a `-` row valued $8");
    println!(
        "sonic.lst: {} equ rows are forward-only (e.g. {} = $8)",
        equ.len(),
        tiny.name
    );
    if let Some(r) = t.resolve(8) {
        assert_ne!(r.symbol.kind, SymbolKind::Equate);
    }
}

/// **The binding measurement.** Which path does a real stock-AS listing actually land on against the ROM
/// it describes? The aurora session predicted binding-for-free; the recon predicted the ruling was needed.
///
/// Answer: **`Indeterminate(EndOfRomIsImageEnd)`** — the ruling is load-bearing. `RomEndLoc: dc.l
/// EndOfRom-1` puts `EndOfRom` at exactly the image length, so before the ruling this was a positive
/// `Mismatch(EndOfRomOutOfRange)` and the frontend refused the listing outright. It is now
/// accepted-unverified, which is the honest verdict for a ROM that carries no appendix to check.
#[test]
fn real_sonic_lst_binds_to_the_rom_it_describes_as_appendix_less() {
    let (Some(text), Some(rom)) = (listing(), artifact("s1built.bin")) else {
        return;
    };
    let t = SymbolTable::parse(&text).expect("sonic.lst must parse");

    // The premise, measured rather than assumed: `EndOfRom` is the image length to the byte, and the ROM
    // header's own `EndOfRom-1` longword agrees — so the equality is by construction, not coincidence.
    let end = t
        .address_of("EndOfRom")
        .expect("EndOfRom is in the listing");
    assert_eq!(end as usize, rom.len(), "EndOfRom must be the image length");
    assert_eq!(rom_declared_end(&rom), Some(end - 1));

    assert_eq!(
        t.validate_against_rom(&rom),
        RomBinding::Indeterminate(Indeterminate::EndOfRomIsImageEnd { rom_len: rom.len() }),
        "a stock AS listing has no appendix to probe"
    );
    // Indeterminate alone is not enough for the caller to accept — it also needs the file to be whole.
    assert!(t.is_intact());

    // The guard that must NOT have moved: truncate the image and the same listing is positively wrong
    // again, because `EndOfRom` now lands inside a shorter ROM with no magic at it.
    assert!(
        matches!(
            t.validate_against_rom(&rom[..rom.len() / 2]),
            RomBinding::Mismatch(_)
        ),
        "a wrong-sized image must still be refused"
    );
    println!(
        "sonic.lst vs s1built.bin: Indeterminate(EndOfRomIsImageEnd {{ rom_len: {} }}) — \
         accepted unverified, is_intact={}",
        rom.len(),
        t.is_intact()
    );
}
