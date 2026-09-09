//! **H24 — a damaged-but-BINDING symbol listing must be disclosed on this surface too.**
//!
//! `SymbolTable::is_intact()` is a fact about the FILE, not about which ROM it describes. The two are
//! combined in a refusal — an `Indeterminate` binding on a damaged file is refused, because "no
//! fingerprint" may just be the fingerprint symbol having fallen off a truncated end — but a listing
//! that positively **binds** and is *also* damaged is accepted, and correctly so: its symbols are real,
//! there are simply fewer of them.
//!
//! What follows from "fewer of them" is the hazard this file exists for. An address then resolves to the
//! nearest **surviving** label rather than to the one that owns it, so `lookup_symbol`, a watch hit's PC
//! and a profiler routine name all come back plausible, healthy-looking and coarser than the truth — the
//! frontend's own words: *"a PC lands on a coarser name rather than a wrong one… Say so out loud (the
//! coarser name looks perfectly healthy)."*
//!
//! Three surfaces said it (`oracle-frontend`, `oracle-player`, `oracle-replay`). **Both `oracle-aether`
//! surfaces said nothing**, so the same truncated file warned in the window and accepted clean over the
//! socket. `grep -rn is_intact crates/oracle-aether/src` returned two hits, both the *Indeterminate*
//! guard, neither the match-but-damaged disclosure.
//!
//! ## The reachable case, and why it takes a real appendix
//!
//! `Match` is the only accepted-and-damaged binding: every `Indeterminate` shape is refused once the
//! file is not intact, and `Mismatch` is refused outright. So the fixture ROM here carries a genuine
//! `de b2` appendix and the listing declares `EndOfRom` at its offset — the same construction
//! `oracle-core`'s `rom_with_appendix` uses, built here from `testrom::build()`'s own length so it
//! cannot rot into an `Indeterminate` silently.
//!
//! This is not contrived. `oracle-core::symbols`' module doc records the live incident: a `Phase Table`
//! shape the parser did not yet recognise cost **684 phantom `skipped_lines`** on the real
//! `aeon/s4.debug.lst` and took `is_intact` false — on a current, correct listing that binds.
//!
//! ## Anti-vacuity
//!
//! Every assertion below has its negative twin on the SAME server with the SAME ROM: an intact listing
//! that binds must produce no damage sentence, on either surface. Without it a caveat that fired
//! unconditionally would satisfy every positive row here — and §11.27 makes an unconditional caveat a
//! MUST NOT, not merely a weak test.

mod common;

use common::{spawn_with, Client};
use oracle_core::symbols::{DEB2_MAGIC, DEB2_MIN_LEN};
use serde_json::json;

/// `testrom::build()` with a real `de b2` appendix bolted on, and the offset that appendix starts at.
///
/// The offset is the stock image's length, so the listing's `EndOfRom` is derived from the fixture
/// rather than typed — a hardcoded offset would degrade to `Mismatch` (refused) or `Indeterminate`
/// (refused, when damaged) the moment the fixture ROM changed size, and every row below would then pass
/// or fail for a reason that has nothing to do with damage.
fn rom_with_appendix() -> (Vec<u8>, usize) {
    let mut rom = oracle_core::testrom::build();
    let end = rom.len();
    rom.extend(std::iter::repeat_n(0u8, DEB2_MIN_LEN));
    rom[end] = DEB2_MAGIC[0];
    rom[end + 1] = DEB2_MAGIC[1];
    (rom, end)
}

/// A listing that BINDS (`EndOfRom` sits on the appendix) and is whole. The control.
///
/// `addr` is `Level_Width`'s address, and it is a parameter so the damaged twin can be **the same file
/// with that one field unreadable** — one token apart, which is what makes the pair a differential
/// rather than two unrelated fixtures.
fn lst(end: usize, level_width: &str) -> String {
    format!(
        "  Symbol Table (* = unused):\n  --------------------------\n\n \
         EntryPoint : 200 C |\n Level_Width : {level_width} C |\n EndOfRom : {end:X} C |\n \
         Player_1 : FFFF8CFA C |\n\n    4 symbols\n    0 unused symbols\n"
    )
}

/// The whole file: `Level_Width` at `$280`, footer agreeing, `EndOfRom` on the appendix.
fn intact_lst(end: usize) -> String {
    lst(end, "280")
}

/// The same file with one row the parser cannot read — the shape the real `s4.debug.lst` incident had.
/// Still binds (`EndOfRom` is untouched); no longer intact.
fn damaged_lst(end: usize) -> String {
    lst(end, "ZZZZ")
}

/// An address inside what `Level_Width` owns in the whole listing, and past `EntryPoint`. The one probe
/// whose answer differs between the two files.
const INSIDE_LEVEL_WIDTH: &str = "0x00000290";

fn write_lst(tag: &str, text: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("ae-h24-{tag}-{}.lst", std::process::id()));
    std::fs::write(&p, text).unwrap();
    p
}

/// `emulator/load_symbols` must ACCEPT a damaged-but-binding listing and say what is wrong with it, and
/// `emulator/status` must keep saying so for every client that did not perform the load.
#[test]
fn a_damaged_but_binding_listing_is_accepted_and_disclosed_on_both_aether_surfaces() {
    let (rom, end) = rom_with_appendix();
    let h = spawn_with("h24", rom, 1024);
    let mut c = Client::connect(&h);
    c.handshake(false);

    // ---- the control, FIRST: an intact listing that binds says nothing about damage ----
    let lst = write_lst("intact", &intact_lst(end));
    let r = c.ok(
        "emulator/load_symbols",
        json!({"path": lst.display().to_string()}),
    );
    assert_eq!(
        r["binding"],
        json!("match"),
        "the fixture must reach the ONLY accepted-and-damaged binding, or this test proves nothing: {r}"
    );
    let clean = r["caveat"].as_str().unwrap_or("").to_string();
    assert!(
        !clean.contains("NOT INTACT"),
        "a whole listing must not be called damaged: {clean}"
    );
    let clean_status = c
        .ok("emulator/status", json!({}))
        .get("caveat")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert!(
        !clean_status.contains("NOT INTACT"),
        "and status must stay quiet about it too: {clean_status}"
    );
    // The truth this address has, while the file is whole.
    let truth = c.ok(
        "emulator/lookup_symbol",
        json!({"addr": INSIDE_LEVEL_WIDTH}),
    );
    assert_eq!(
        truth["name"], "Level_Width",
        "the control must establish the RIGHT answer, or the wrong one below proves nothing: {truth}"
    );

    // ---- the finding: the same ROM, a listing that binds and is not whole ----
    let lst = write_lst("damaged", &damaged_lst(end));
    let r = c.ok(
        "emulator/load_symbols",
        json!({"path": lst.display().to_string()}),
    );
    assert_eq!(
        r["binding"],
        json!("match"),
        "accepted, not refused — the symbols are real: {r}"
    );
    let cav = r["caveat"]
        .as_str()
        .expect("a damaged listing must carry a caveat")
        .to_string();
    assert!(
        cav.contains("NOT INTACT"),
        "load_symbols accepted a damaged listing in silence: {cav}"
    );
    assert!(
        cav.contains("unrecognised rows"),
        "…and must name the damage rather than gesture at it: {cav}"
    );
    assert!(
        cav.contains("lookup_symbol"),
        "…and name what inherits it, which is the whole hazard: {cav}"
    );

    // The standing disclosure, for the three routes that never see the load reply: a `--symbols` path
    // taken at startup, a client that joined an already-running server, and a hosted player whose window
    // did the loading.
    let status = c.ok("emulator/status", json!({}));
    let scav = status["caveat"]
        .as_str()
        .expect("status must carry the damage caveat")
        .to_string();
    assert!(
        scav.contains("NOT INTACT") && scav.contains("symbolAtPc"),
        "status serves symbolAtPc from this table and must say it is coarse: {scav}"
    );

    // ⚑ The differential, and the reason this is a caveat rather than a refusal: `lookup_symbol` still
    // ANSWERS, and its answer still looks healthy. The row that owned `$290` is gone, so the address
    // resolves backwards onto `EntryPoint` — a real symbol, a well-formed reply, and not the routine the
    // address is in. Same server, same ROM, same query, one token of difference in the file.
    let r = c.ok(
        "emulator/lookup_symbol",
        json!({"addr": INSIDE_LEVEL_WIDTH}),
    );
    assert_eq!(
        r["name"], "EntryPoint",
        "the coarse answer is what a client gets, and it looks entirely healthy: {r}"
    );
    assert_ne!(
        r["name"], truth["name"],
        "the two files must disagree about this address, or there is nothing to disclose"
    );
}
