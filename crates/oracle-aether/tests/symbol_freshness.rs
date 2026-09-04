//! **`F-RELOAD-KEEPS-STALE-SYMBOLS` — a reload that keeps the symbol table must not be read as a
//! reload that refreshed it.**
//!
//! `emulator/reload_rom` drops the loaded listing only when it binds to a *different build shape*
//! (`RomBinding::Mismatch`). A newer build of the SAME shape passes that check, so the old table is kept
//! whole and `symbolsDropped: false` goes out — truthfully saying "I did not drop them" and being read
//! as "they are fine". The two came apart in the field: a peer lane published `Level_Width` /
//! `Level_Height`, a session reloaded, trusted `symbolsDropped: false`, and got
//! `-32013 no symbol named or prefixed Level_Width` for a symbol that was sitting in the listing on
//! disk. The near-miss was a false defect report filed against the peer's landing rather than against
//! this instrument.
//!
//! ## The independent channel, and why it is the whole point
//!
//! A test that asserted only "the freshness verdict says stale" would be two copies of one side: our own
//! state checked against our own opinion of it. So every test below **constructs the real situation** —
//! a listing on disk carrying a symbol, a table loaded from an older listing that lacks it — and asserts
//! on `emulator/lookup_symbol`'s actual answer, which is the observable that did the misleading.
//!
//! Three guards keep that from passing vacuously:
//!
//! * a **control name** (`Player_1`) that must resolve at the same instant the missing one does not, so
//!   a broken probe cannot be read as a real absence — this is the step that established the field
//!   sighting as a genuine absence rather than a broken call;
//! * an **anti-vacuity clause**: after the correct action (`emulator/load_symbols`) the symbol MUST
//!   resolve, or every assertion here would pass on a build where nothing ever resolves;
//! * an **over-firing control** ([`an_unchanged_listing_leaves_the_reload_reply_quiet`]): an
//!   unconditional caveat would satisfy the stale tests and is caught only there.

mod common;

use common::{spawn, temp_socket, Client};
use oracle_aether::engine::EngineConfig;
use oracle_aether::server::{Machine, Server, ServerConfig, ServerHandle};
use oracle_core::system::System;
use serde_json::{json, Value};
use std::path::PathBuf;

/// A minimal AS-dialect listing that binds to `testrom::build()` — the spelling `symbols_path.rs` and
/// `methods.rs` already use. Two rows, and the footer counts match, which is what keeps
/// `SymbolTable::is_intact` true and the listing acceptable.
const LST_OLD: &str = "\
  Symbol Table (* = unused):
  --------------------------

 EntryPoint : 200 C |
 Player_1 : FFFF8CFA C |

    2 symbols
    0 unused symbols
";

/// [`LST_OLD`] as a peer lane's next build publishes it: one new symbol, everything else identical.
/// This is `Level_Width` arriving.
const LST_REBUILT: &str = "\
  Symbol Table (* = unused):
  --------------------------

 EntryPoint : 200 C |
 Level_Width : 400 C |
 Player_1 : FFFF8CFA C |

    3 symbols
    0 unused symbols
";

/// [`LST_OLD`] with the SAME number of rows and the same names, one of them at a different address.
///
/// This is not a contrived case: of the symbols `s4.lst` and `s4.debug.lst` share, 92.6% name a
/// different address (`Engine::load_symbols` records the measurement). A freshness check that compared
/// row *counts* would call this file current and answer every lookup with a confidently wrong address.
const LST_MOVED: &str = "\
  Symbol Table (* = unused):
  --------------------------

 EntryPoint : 200 C |
 Player_1 : FFFF9000 C |

    2 symbols
    0 unused symbols
";

/// The address `LST_OLD` and `LST_REBUILT` both give `Player_1`, spelled the way the wire spells it.
const PLAYER_1_OLD: &str = "0x00FF8CFA";
/// The address `LST_MOVED` gives it instead.
const PLAYER_1_MOVED: &str = "0x00FF9000";

/// A unique temp path per call — this binary's tests run in parallel in one process, so a path shared
/// between two of them is a flake that reports the wrong thing. (Same reasoning, same shape, as
/// `symbols_path.rs`'s helper.)
fn temp_path(tag: &str, ext: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "ae-symfresh-{}-{tag}-{n}.{ext}",
        std::process::id()
    ))
}

/// The ROM file `reload_rom` will be pointed at: byte-identical to the image the server booted, because
/// the defect is specifically about a reload that KEEPS the table. A different image would risk the
/// binding check dropping it and the test would witness the other branch.
fn write_rom(tag: &str) -> PathBuf {
    let p = temp_path(tag, "bin");
    std::fs::write(&p, oracle_core::testrom::build()).expect("write the ROM fixture");
    p
}

fn write_lst(tag: &str, body: &str) -> PathBuf {
    let p = temp_path(tag, "lst");
    std::fs::write(&p, body).expect("write the listing fixture");
    p
}

/// A server holding a symbol table with **no recorded path** — the state an embedder reaches through
/// `host::MachineInfo`, and one `emulator/load_symbols` can never produce. Lifted from
/// `symbols_path.rs`'s `serve_with_symbols`, which needed the same seam for the same reason: the
/// launch boundary is the only way to install a table the wire could not have installed.
fn serve_with_a_pathless_table(tag: &str) -> ServerHandle {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    let mut machine = Machine::new(sys);
    machine.symbols =
        Some(oracle_core::symbols::SymbolTable::parse(LST_OLD).expect("parse the fixture listing"));
    machine.symbols_path = None;
    Server::bind(ServerConfig {
        socket_path: temp_socket(tag),
        engine: EngineConfig {
            free_run_pace: None,
            ..EngineConfig::default()
        },
        event_queue_cap: 1024,
    })
    .expect("bind aether socket")
    .spawn(machine)
}

/// `lookup_symbol`, returning the result on a hit and `None` on `-32013`.
///
/// Asserting the *code* rather than only "it failed" matters: `-32011` (no table loaded at all) is a
/// different finding and would make the absence uninteresting.
fn lookup(c: &mut Client, name: &str) -> Option<Value> {
    let v = c.call("emulator/lookup_symbol", json!({ "name": name }));
    match v.get("error") {
        None => Some(v["result"].clone()),
        Some(e) => {
            assert_eq!(
                e["code"],
                json!(-32013),
                "the absence under test is SYMBOL_NOT_FOUND; -32011 (no table) would mean the fixture \
                 never loaded and the assertion below would be witnessing nothing: {e}"
            );
            None
        }
    }
}

/// The caveat on a `reload_rom` reply, or `None` when the reply is quiet.
///
/// Used for `emulator/status` too since §11.34 — it is the same key, the same §2.4 string, and the same
/// verdict from the same function, which is the whole of "one implementation, two consumers".
fn caveat(reload: &Value) -> Option<String> {
    reload
        .get("caveat")
        .map(|c| c.as_str().expect("a caveat is a string (§2.4)").to_string())
}

// ---------------------------------------------------------------------------------------------------
// ⚑ §11.34 (CR-K) — the STANDING case, on `emulator/status`
//
// Everything above is the RELOAD edge, and closing it closed nothing that bites: a session that
// connects once and runs for hours while the engine lane rebuilds **never calls `reload_rom` at all**.
// Its table goes stale with no event and no observable. `emulator/status` is the method such a session
// does call, and until CR-K its fragment declared no key that could carry the verdict — `symbolCount` is
// a count, and a count is not an identity.
//
// The rows below keep the shape the reload rows established, for the reason stated at the top of this
// file: the assertion is on `lookup_symbol`'s actual answer, with a control name that must resolve at
// the same instant and an anti-vacuity clause after the correct action. What is NOT here is the *cost*
// of serving the verdict on a per-frame method — that is measured in `engine.rs`'s own test module,
// against a private counter, because a cache that silently does the work it claims to skip leaves every
// row in this file green.
// ---------------------------------------------------------------------------------------------------

#[test]
fn a_standing_session_that_never_reloads_learns_from_status_that_its_listing_moved() {
    let lst = write_lst("standing", LST_OLD);
    let lst_abs = std::fs::canonicalize(&lst).expect("canonicalize the listing");

    let h = spawn("symfresh-standing");
    let mut c = Client::connect(&h);
    c.handshake(false);

    let loaded = c.ok(
        "emulator/load_symbols",
        json!({ "path": lst.display().to_string() }),
    );
    assert_eq!(
        loaded["symbolCount"],
        json!(2),
        "the premise: the OLD listing"
    );
    assert_eq!(
        caveat(&c.ok("emulator/status", json!({}))),
        None,
        "OVER-FIRING CONTROL: before anything moves, status is quiet. A caveat emitted unconditionally \
         satisfies every assertion below and carries no information."
    );
    assert!(
        lookup(&mut c, "Level_Width").is_none(),
        "the premise: the symbol the peer is about to publish is not in the old table yet"
    );

    // The peer lane rebuilds. **No reload, no load_symbols, no event — nothing at all happens on this
    // connection.** This is the whole point: the session is doing what a long-lived session does.
    std::fs::write(&lst, LST_REBUILT).expect("rewrite the listing");

    let status = c.ok("emulator/status", json!({}));
    // The misleading observables are untouched, and deliberately so: `symbolCount` still answers "how
    // many rows do I hold", which is honest and is not the question.
    assert_eq!(
        status["symbolCount"],
        json!(2),
        "the count is unchanged — it was never the thing that could tell you"
    );
    assert_eq!(status["symbolsPath"], json!(lst_abs.display().to_string()));
    let text = caveat(&status).unwrap_or_else(|| {
        panic!("a status whose listing has moved past the table must SAY SO. Reply: {status}")
    });
    for needle in [
        lst_abs.display().to_string().as_str(),
        "2 row(s) held",
        "3 row(s) in the file now",
        "emulator/load_symbols",
    ] {
        assert!(
            text.contains(needle),
            "the caveat must name {needle:?} — what we hold, what the path holds now, and the fix. \
             Got: {text}"
        );
    }

    // ---- the independent channel: the observable that actually did the misleading ----
    assert!(
        lookup(&mut c, "Level_Width").is_none(),
        "status is a verdict, not a re-read: the server is still serving the old table"
    );
    let control = lookup(&mut c, "Player_1").expect(
        "CONTROL: a name in the held table must resolve at the same instant, or the probe above proves \
         nothing",
    );
    assert_eq!(control["addr"], json!(PLAYER_1_OLD));

    // ---- anti-vacuity: after the correct action, the symbol resolves AND status goes quiet ----
    let reloaded = c.ok(
        "emulator/load_symbols",
        json!({ "path": lst.display().to_string() }),
    );
    assert_eq!(reloaded["symbolCount"], json!(3));
    let found = lookup(&mut c, "Level_Width").expect(
        "ANTI-VACUITY: after re-reading the listing the new symbol must resolve. If it does not, every \
         `is_none()` above passes on a build where nothing resolves at all.",
    );
    assert_eq!(found["addr"], json!("0x00000400"));
    assert_eq!(
        caveat(&c.ok("emulator/status", json!({}))),
        None,
        "ANTI-VACUITY, the other direction: the fix the caveat NAMES must actually silence it, or the \
         sentence is advice that does not work"
    );

    let _ = std::fs::remove_file(&lst);
}

#[test]
fn polling_status_against_an_unchanged_listing_stays_quiet() {
    // The over-firing control for the polling path specifically. `status` is the method a UI calls every
    // frame; a verdict that drifted loud after N calls — because a cache confused itself — would put a
    // permanent false alarm on the glass of every session that changed nothing.
    let lst = write_lst("poll", LST_OLD);

    let h = spawn("symfresh-poll");
    let mut c = Client::connect(&h);
    c.handshake(false);
    c.ok(
        "emulator/load_symbols",
        json!({ "path": lst.display().to_string() }),
    );

    for i in 0..30 {
        assert_eq!(
            caveat(&c.ok("emulator/status", json!({}))),
            None,
            "call {i}: nothing has moved, so nothing is said"
        );
    }

    // …and it is still capable of firing, which is what stops "quiet" from being a stuck answer.
    std::fs::write(&lst, LST_MOVED).expect("rewrite the listing");
    assert!(
        caveat(&c.ok("emulator/status", json!({}))).is_some(),
        "ANTI-VACUITY: after 30 quiet polls the verdict must still be live. A cache that latched quiet \
         would be a way to be confidently stale about staleness."
    );
    // The independent channel: the server is still answering out of the OLD table.
    let held = lookup(&mut c, "Player_1").expect("the held table still answers");
    assert_eq!(held["addr"], json!(PLAYER_1_OLD));

    let _ = std::fs::remove_file(&lst);
}

#[test]
fn a_status_whose_listing_vanished_is_loud_and_stays_loud() {
    // Loud on unmeasurable, on the polling path. A failed `stat` is not "unchanged", and the filter in
    // front of the check must never turn "I could not look" into the silence that means "I looked".
    let lst = write_lst("statusgone", LST_OLD);
    let lst_abs = std::fs::canonicalize(&lst).expect("canonicalize");

    let h = spawn("symfresh-statusgone");
    let mut c = Client::connect(&h);
    c.handshake(false);
    c.ok(
        "emulator/load_symbols",
        json!({ "path": lst.display().to_string() }),
    );
    assert_eq!(
        caveat(&c.ok("emulator/status", json!({}))),
        None,
        "the premise: quiet while the file is there, and the cache is warm when it goes"
    );

    std::fs::remove_file(&lst).expect("delete the listing out from under the server");

    for i in 0..5 {
        let text = caveat(&c.ok("emulator/status", json!({}))).unwrap_or_else(|| {
            panic!("call {i}: an unmeasurable freshness must be LOUD, not quiet")
        });
        assert!(
            text.contains("could NOT be checked") && text.contains(&lst_abs.display().to_string()),
            "call {i}: it must say it could not check, and name the path it could not read: {text}"
        );
    }
    assert!(
        lookup(&mut c, "Player_1").is_some(),
        "CONTROL: the held table is unaffected; only our ability to check it is"
    );
}

#[test]
fn a_rebuilt_listing_leaves_the_reload_serving_stale_symbols_and_the_reply_says_so() {
    let rom = write_rom("stale");
    let lst = write_lst("stale", LST_OLD);
    let lst_abs = std::fs::canonicalize(&lst).expect("canonicalize the listing");

    let h = spawn("symfresh-stale");
    let mut c = Client::connect(&h);
    c.handshake(false);

    let loaded = c.ok(
        "emulator/load_symbols",
        json!({ "path": lst.display().to_string() }),
    );
    assert_eq!(
        loaded["symbolCount"],
        json!(2),
        "the premise: the OLD listing is what this server holds"
    );
    assert!(
        lookup(&mut c, "Level_Width").is_none(),
        "the premise: the symbol the peer is about to publish is not in the old table yet"
    );

    // The peer lane rebuilds. Same build shape, one new symbol.
    std::fs::write(&lst, LST_REBUILT).expect("rewrite the listing");

    let reload = c.ok(
        "emulator/reload_rom",
        json!({ "path": rom.display().to_string() }),
    );

    // **The misleading observable is unchanged, and that is deliberate.** `symbolsDropped` answers
    // "did I drop them", it is REQUIRED to be present even when false (§2.3), and this fix does not
    // touch it — changing what it means would break the one thing about it that is honest.
    assert_eq!(
        reload["symbolsDropped"],
        json!(false),
        "the table is KEPT — a same-shape rebuild passes the binding check, which is the whole defect"
    );

    let text = caveat(&reload).unwrap_or_else(|| {
        panic!(
            "a reload that kept a table the listing on disk has moved past must SAY SO. Reply: {reload}"
        )
    });
    // Derived from the fixtures, not typed as prose: 2 rows held, 3 in the file now.
    for needle in [
        lst_abs.display().to_string().as_str(),
        "2 row(s) held",
        "3 row(s) in the file now",
        "emulator/load_symbols",
    ] {
        assert!(
            text.contains(needle),
            "the caveat must name {needle:?} — what we hold, what the path holds now, and the fix. \
             Got: {text}"
        );
    }

    // ---- the independent channel: the observable that actually did the misleading ----
    //
    // The verdict is a verdict, not a re-read: the server is STILL serving the old table, exactly as
    // before. What changed is that it no longer lets that pass in silence.
    assert!(
        lookup(&mut c, "Level_Width").is_none(),
        "the reload did not refresh the table and must not pretend to have"
    );
    // The control, at the same instant, on the same connection: a name that IS in the held table
    // resolves. Without this the line above is satisfied by a lookup channel that answers nothing.
    let control = lookup(&mut c, "Player_1").expect(
        "CONTROL: a name in the held table must resolve, or the probe above proves nothing",
    );
    assert_eq!(control["addr"], json!(PLAYER_1_OLD));

    // ---- anti-vacuity: after the correct action, the symbol MUST resolve ----
    let reloaded = c.ok(
        "emulator/load_symbols",
        json!({ "path": lst.display().to_string() }),
    );
    assert_eq!(reloaded["symbolCount"], json!(3));
    let found = lookup(&mut c, "Level_Width").expect(
        "ANTI-VACUITY: after re-reading the listing the new symbol must resolve. If it does not, every \
         `is_none()` above passes on a build where nothing resolves at all.",
    );
    assert_eq!(found["addr"], json!("0x00000400"));

    let _ = std::fs::remove_file(&lst);
    let _ = std::fs::remove_file(&rom);
}

#[test]
fn a_listing_whose_addresses_moved_fires_even_though_the_row_count_did_not() {
    // The shortcut this exists to forbid: comparing `symbolCount`. Both listings have two rows, and the
    // one on disk resolves `Player_1` somewhere else. A count comparison calls this file current and the
    // server then answers with an address that is confidently wrong — which `load_symbols` already
    // records as strictly worse than degraded information.
    let rom = write_rom("moved");
    let lst = write_lst("moved", LST_OLD);

    let h = spawn("symfresh-moved");
    let mut c = Client::connect(&h);
    c.handshake(false);
    c.ok(
        "emulator/load_symbols",
        json!({ "path": lst.display().to_string() }),
    );

    std::fs::write(&lst, LST_MOVED).expect("rewrite the listing");
    assert_eq!(
        symbol_rows(LST_OLD),
        symbol_rows(LST_MOVED),
        "the premise: these two listings declare the same number of rows, so a count comparison cannot \
         tell them apart"
    );
    assert_ne!(
        LST_OLD, LST_MOVED,
        "…and they must nonetheless differ, or there is nothing here to detect"
    );

    let reload = c.ok(
        "emulator/reload_rom",
        json!({ "path": rom.display().to_string() }),
    );
    assert_eq!(reload["symbolsDropped"], json!(false), "the table is kept");
    let text = caveat(&reload)
        .unwrap_or_else(|| panic!("a moved address is a rewritten listing too. Reply: {reload}"));
    assert!(
        text.contains("2 row(s) held") && text.contains("2 row(s) in the file now"),
        "the counts are equal and the verdict still fires; the sentence must not read as nonsense: \
         {text}"
    );

    // The independent channel again: the server keeps answering with the OLD address.
    let held = lookup(&mut c, "Player_1").expect("the held table still answers");
    assert_eq!(
        held["addr"],
        json!(PLAYER_1_OLD),
        "the reload kept the old table, so the old address is what is served"
    );
    // Anti-vacuity: re-reading moves it.
    c.ok(
        "emulator/load_symbols",
        json!({ "path": lst.display().to_string() }),
    );
    let fresh = lookup(&mut c, "Player_1").expect("still resolves after the re-read");
    assert_eq!(
        fresh["addr"],
        json!(PLAYER_1_MOVED),
        "ANTI-VACUITY: the two listings must actually differ on the wire, or the assertion above is \
         satisfied by a fixture that changed nothing"
    );

    let _ = std::fs::remove_file(&lst);
    let _ = std::fs::remove_file(&rom);
}

/// The number of symbol rows a fixture listing carries, counted from the fixture text rather than
/// typed as a literal — the premise assertion above is worth nothing if its own number is a guess.
fn symbol_rows(lst: &str) -> usize {
    lst.lines()
        .filter(|l| l.contains(" : ") && l.trim_end().ends_with('|'))
        .count()
}

#[test]
fn an_unchanged_listing_leaves_the_reload_reply_quiet() {
    // **The over-firing control**, and the only test here that a caveat emitted unconditionally would
    // fail. Without it, `out["caveat"] = json!("...")` with no condition passes every stale test above
    // and the verdict carries no information.
    let rom = write_rom("quiet");
    let lst = write_lst("quiet", LST_OLD);

    let h = spawn("symfresh-quiet");
    let mut c = Client::connect(&h);
    c.handshake(false);
    c.ok(
        "emulator/load_symbols",
        json!({ "path": lst.display().to_string() }),
    );

    let reload = c.ok(
        "emulator/reload_rom",
        json!({ "path": rom.display().to_string() }),
    );
    assert_eq!(reload["symbolsDropped"], json!(false));
    assert_eq!(
        caveat(&reload),
        None,
        "the file at `symbolsPath` parses to exactly the rows held — the one state that is quiet. A \
         caveat here would be noise on the path a client takes every time it rebuilds nothing."
    );

    let _ = std::fs::remove_file(&lst);
    let _ = std::fs::remove_file(&rom);
}

#[test]
fn a_listing_that_vanished_is_reported_as_unchecked_never_as_fine() {
    // House rule: loud on unmeasurable beats a plausible answer. "I could not look" and "I looked and
    // it is fine" are the same silence unless one of them is made to speak, and this is the shape the
    // ROM-freshness banner got right first — its `unavailable` state raises a banner exactly like
    // `stale` does.
    let rom = write_rom("gone");
    let lst = write_lst("gone", LST_OLD);
    let lst_abs = std::fs::canonicalize(&lst).expect("canonicalize");

    let h = spawn("symfresh-gone");
    let mut c = Client::connect(&h);
    c.handshake(false);
    c.ok(
        "emulator/load_symbols",
        json!({ "path": lst.display().to_string() }),
    );

    std::fs::remove_file(&lst).expect("delete the listing out from under the server");

    let reload = c.ok(
        "emulator/reload_rom",
        json!({ "path": rom.display().to_string() }),
    );
    assert_eq!(
        reload["symbolsDropped"],
        json!(false),
        "the table is in memory and is unaffected by the file going away"
    );
    let text = caveat(&reload).unwrap_or_else(|| {
        panic!("an unmeasurable freshness must be LOUD, not quiet. Reply: {reload}")
    });
    assert!(
        text.contains("could NOT be checked") && text.contains(&lst_abs.display().to_string()),
        "the caveat must say it could not check, and name the path it could not read: {text}"
    );
    // The table itself still answers — the finding is about knowledge, not about capability.
    assert!(
        lookup(&mut c, "Player_1").is_some(),
        "CONTROL: the held table is unaffected; only our ability to check it is"
    );

    let _ = std::fs::remove_file(&rom);
}

#[test]
fn a_table_with_no_recorded_path_is_reported_as_unchecked_never_as_fine() {
    // The third unmeasurable, and the only one the wire cannot construct: `emulator/load_symbols`
    // always records a path, so a table with none arrives through the launch boundary — the binary, or
    // a hosted embedder's `MachineInfo`. Without this row the branch is dead code that reads as covered.
    let rom = write_rom("pathless");

    let h = serve_with_a_pathless_table("symfresh-pathless");
    let mut c = Client::connect(&h);
    c.handshake(false);

    let status = c.ok("emulator/status", json!({}));
    assert_eq!(
        status["symbolsPath"],
        json!(null),
        "the premise: this server holds a table and no path for it"
    );
    assert_eq!(status["symbolCount"], json!(2), "…and the table is real");
    // §11.34: `status` carries the same verdict, and this is the state the wire cannot construct. There
    // is no file to stat, so nothing is remembered and nothing can be — every call re-derives it.
    let from_status = caveat(&status).unwrap_or_else(|| {
        panic!("a freshness that cannot be measured AT ALL must be loudest of all. Reply: {status}")
    });
    assert!(
        from_status.contains("could NOT be checked at all"),
        "{from_status}"
    );

    let reload = c.ok(
        "emulator/reload_rom",
        json!({ "path": rom.display().to_string() }),
    );
    assert_eq!(reload["symbolsDropped"], json!(false));
    let text = caveat(&reload).unwrap_or_else(|| {
        panic!("a freshness that cannot be measured AT ALL must be loudest of all. Reply: {reload}")
    });
    assert!(
        text.contains("could NOT be checked at all"),
        "the caveat must say it could not check, and say why: {text}"
    );
    assert!(
        lookup(&mut c, "Player_1").is_some(),
        "CONTROL: the held table still answers; only our ability to check it is in question"
    );

    let _ = std::fs::remove_file(&rom);
}
