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
//!
//! # The directory must be NAMED — there is no default (2026-09-02)
//!
//! Until this change `s1_dir()` fell back to the home literal `/home/volence/sonic_hacks/s1disasm`.
//! On the one machine that has that path, these five rows therefore ran against **another repository's
//! live, gitignored build output** — and `real_sonic_lst_parses_completely` asserts
//! `t.len() == 12_405` exactly. A `lua build.lua` over there, on any modified source, moves that number
//! and reddens *this* suite for a reason that has nothing to do with our code. That is the whole
//! complaint this campaign exists to answer, arriving in a repo nobody thinks of as a suite peer.
//!
//! It cannot be closed the way `mcp_tool_sweep.rs` closes it. There is no revision to pin to:
//! `sonic.lst` and `sonic.bin` are **build outputs and gitignored**, so they exist in no object store,
//! and at 10 MB and 551 KB they are not vendorable either. What is left is the contract's other arm —
//! *"An env-var override pointing at a file is legitimate; its absence is a loud skip naming the
//! variable, not a walk"* (`empyrean` `contract/SUITE_PATHS.md` at `38f6df4`) — so the dependency
//! becomes an explicit opt-in instead of an accident of one machine's disk layout.
//!
//! **The cost, stated plainly: on a machine that has `s1disasm` but sets no variable, these five rows
//! now SKIP where they used to run.** That is a real loss of coverage and it is deliberate: coverage
//! that depends on an unattributable directory is coverage whose green means "whatever was there".
//! Restore it in one line:
//!
//! ```text
//! ORACLE_S1DISASM_DIR=/path/to/s1disasm cargo test -p oracle-core --test symbols_as_dialect
//! ```
//!
//! No derivation step, on purpose. `SUITE_PATHS.md` allows a walk for answering *which checkout* and
//! refuses it for reference-dependent measurement, "because it derives the owner's live tree whose
//! revision moves under a run" — and reading `sonic.lst` to assert a row count is exactly that.

use oracle_core::symbols::{
    rom_declared_end, AddrSpace, Indeterminate, RomBinding, SymbolKind, SymbolTable, TableSource,
    BUS_ADDR_MASK,
};
use std::path::PathBuf;

/// Write a line where libtest's output capture cannot swallow it.
///
/// **Measured, and it invalidates the house pattern's premise.** `println!`/`eprintln!` route through
/// `std::io::_print`/`_eprint`, which libtest redirects per test thread, so a skip printed with them is
/// invisible in a plain `cargo test` run and appears only under `--nocapture`. Run against these five
/// rows with nothing named, they reported
///
/// ```text
/// test real_sonic_lst_parses_completely ... ok
/// test result: ok. 5 passed; 0 failed; 0 ignored
/// ```
///
/// — five rows that read nothing, indistinguishable from five rows that checked everything. *"A green
/// log and an absent run are the same artifact"* is the bar (`SUITE_PATHS.md`, protocol bar 25), and a
/// skip that is loud only under a flag nobody passes is on the wrong side of it.
///
/// `std::io::stderr()` is the real handle on fd 2 and the capture does not touch it, so this reaches
/// the terminal in a default run.
fn loud(msg: String) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{msg}");
}

/// A directory of s1disasm build ARTIFACTS. The name this repo already uses.
const ENV_S1_DIR: &str = "ORACLE_S1DISASM_DIR";
/// The suite root every checkout hangs off.
const ENV_SUITE_ROOT: &str = "EMPYREAN_SUITE_ROOT";

/// Where the S1 disassembly's build outputs live, or `None` when nothing named them.
///
/// A variable that is **set** is the answer, right or wrong: per `SUITE_PATHS.md` a set-but-wrong value
/// is evidence of a wrong environment, so it is reported as itself rather than falling through to a
/// step that would hide it.
fn s1_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var(ENV_S1_DIR) {
        return Some(PathBuf::from(d));
    }
    if let Ok(root) = std::env::var(ENV_SUITE_ROOT) {
        return Some(PathBuf::from(root).join("s1disasm"));
    }
    None
}

/// Read one `s1disasm` build artifact, or `None` (with a loud printed note) when it is not reachable.
fn artifact(name: &str) -> Option<Vec<u8>> {
    let Some(dir) = s1_dir() else {
        loud(format!(
            "SKIPPED: no s1disasm directory was named, so `{name}` was not read and this row did not \
             run. Consulted, in order:\n  \
               ${ENV_S1_DIR} (a directory of s1disasm build artifacts) — not set\n  \
               ${ENV_SUITE_ROOT}/s1disasm — {ENV_SUITE_ROOT} not set\n\
             There is deliberately no default and no walk: the home literal that used to sit here made \
             these rows assert exact counts against another repo's live, gitignored build output \
             (empyrean contract/SUITE_PATHS.md at 38f6df4). Re-enable with\n  \
               {ENV_S1_DIR}=/path/to/s1disasm cargo test -p oracle-core --test symbols_as_dialect"
        ));
        return None;
    };
    let p = dir.join(name);
    match std::fs::read(&p) {
        Ok(b) => {
            println!(
                "read {} ({} bytes) — a LIVE build output, not frozen and not pinned",
                p.display(),
                b.len()
            );
            Some(b)
        }
        Err(_) => {
            loud(format!(
                "SKIP: {} not present. Build it with `cd {} && lua build.lua` \
                 (non-destructive, ~600ms, artifacts are gitignored), or point {ENV_S1_DIR} \
                 at a checkout that has it.",
                p.display(),
                dir.display()
            ));
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
