//! **Every success-reply path field is absolute, and every refusal quotes the caller** — contract
//! `protocol.md` §6's paths note as ridden by §11.30 (CR-I, adopted from this lane's own filing,
//! `docs/proposed/2026-08-30-cr-i-symbolspath.md`).
//!
//! §11.30 enumerates the rule **by role, not by name**: every success-reply field whose *value* is a
//! filesystem path is resolved by one rule at the load boundary — `status.romPath` (already pinned in
//! `rom_path.rs`), `status.symbolsPath`, `load_symbols`'s reply `path`, `reload_rom`'s, `screenshot`'s,
//! and the `romReloaded` event's. Error payloads are the deliberate exception.
//!
//! **This file exists because the schema cannot witness any of it.** The contract change that carried
//! §11.30 was three JSON Schema `description` edits and nothing else — no shape, no `required`, no new
//! vectors — and `description` is an annotation with no validation force. So re-vendoring the schema
//! turns `schema_conformance.rs` green while a server that reports `fixtures/aeon/s4.lst` stays
//! non-conformant and every conformance vector still passes. A green suite after a description-only
//! re-vendor is not evidence of conformance; these assertions are.
//!
//! Every expectation below is derived from *this process's* resolved cwd or from `canonicalize` on a
//! file the test just wrote. A hardcoded `/home/...` literal would pass on one machine and be a lie
//! about what is being checked.

mod common;

use common::{spawn, temp_socket, Client};
use oracle_aether::engine::EngineConfig;
use oracle_aether::server::{Machine, Server, ServerConfig, ServerHandle};
use oracle_core::system::System;
use serde_json::json;
use std::path::{Path, PathBuf};

/// A minimal AS-dialect listing that binds to `testrom::build()` — the same spelling `methods.rs`
/// already loads.
const LST: &str = "\
  Symbol Table (* = unused):
  --------------------------

 EntryPoint : 200 C |
 Player_1 : FFFF8CFA C |

    2 symbols
    0 unused symbols
";

/// A relative spelling of `target`, built by walking up out of the current directory.
///
/// Deliberately **not** `std::env::set_current_dir`: the cwd is process-global and every test in this
/// binary shares it, so a test that moves it is a test that can break a sibling running beside it. This
/// produces a genuinely relative path — the thing under test — without touching any shared state.
/// (Lifted from `rom_path.rs`, which needed exactly this for exactly this reason.)
fn relative_to_cwd(target: &Path) -> String {
    let cwd = std::env::current_dir().expect("cwd");
    let ups = cwd.components().count() - 1; // every component but the root
    let mut rel = PathBuf::new();
    for _ in 0..ups {
        rel.push("..");
    }
    rel.push(target.strip_prefix("/").expect("an absolute target"));
    rel.to_string_lossy().into_owned()
}

/// A unique temp path for this test binary. Unique **per call**, not per tag: `cargo test` runs these in
/// parallel in one process, and two threads writing one file is a flake that reports the wrong thing.
fn temp_path(tag: &str, ext: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("ae-sympath-{}-{tag}-{n}.{ext}", std::process::id()))
}

/// Assert `rel` really is relative *and* really names `canonical` — the premise of every test below.
/// Without this a helper that quietly produced an absolute string would let the assertions pass while
/// proving nothing.
fn assert_is_a_relative_spelling_of(rel: &str, canonical: &Path) {
    assert!(
        Path::new(rel).is_relative(),
        "the input under test must be relative; got {rel}"
    );
    assert_eq!(
        std::fs::canonicalize(rel).expect("the relative spelling resolves"),
        canonical,
        "the relative spelling must name the same file, or the server is being asked about something else"
    );
}

/// A server launched with `symbols_path` set, the way `main.rs` does from `--symbols`.
fn serve_with_symbols(tag: &str, symbols_path: Option<String>, table: bool) -> ServerHandle {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    let mut machine = Machine::new(sys);
    machine.symbols_path = symbols_path;
    if table {
        machine.symbols = Some(oracle_core::symbols::SymbolTable::parse(LST).expect("parse LST"));
    }
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

// ------------------------------------------------------------------ the launch boundary

#[test]
fn status_reports_an_absolute_symbols_path_even_when_launched_with_a_relative_one() {
    // The exact defect CR-I was filed on, reduced: the filing seat drove `oracle-aether s4.bin
    // --symbols fixtures/aeon/s4.lst` and got an absolute `romPath` and a relative `symbolsPath` out of
    // ONE command line.
    let lst = temp_path("launch", "lst");
    std::fs::write(&lst, LST).expect("write the listing");
    let canonical = std::fs::canonicalize(&lst).expect("canonicalize the listing we just wrote");
    let rel = relative_to_cwd(&canonical);
    assert_is_a_relative_spelling_of(&rel, &canonical);

    let h = serve_with_symbols("symlaunch", Some(rel.clone()), true);
    let mut c = Client::connect(&h);
    c.handshake(true);
    let reported = c.ok("emulator/status", json!({}))["symbolsPath"]
        .as_str()
        .expect("status carries symbolsPath")
        .to_string();

    assert!(
        Path::new(&reported).is_absolute(),
        "§6 as ridden by §11.30: `symbolsPath` is the ABSOLUTE path of the loaded listing. The server \
         was launched with {rel} and reported {reported} — a string that answers \"which listing is this \
         server using?\" only for a reader who already shares this process's working directory, which is \
         exactly the reader who did not need to ask."
    );
    assert_eq!(
        Path::new(&reported),
        canonical,
        "the absolute path must name the listing that was actually loaded"
    );

    let _ = std::fs::remove_file(&lst);
}

#[test]
fn a_symbols_label_that_names_no_file_is_passed_through_untouched() {
    // The other half of the one rule, and the reason §11.30 gave `symbolsPath` the SAME helper as
    // `romPath` rather than a stricter one. `absolutise` is `canonicalize`-or-nothing on purpose: the
    // string is not always a filesystem path, and joining a working directory onto a label would
    // manufacture a path that resolves to nothing and looks authoritative. §6 is a SHOULD so that "I
    // cannot honestly say" stays available — for both keys, because a second rule is a second thing to
    // get wrong.
    let label = "not-a-listing-just-a-label";
    assert!(
        std::fs::canonicalize(label).is_err(),
        "the premise: this string must not accidentally name a real file in the test's cwd"
    );
    let h = serve_with_symbols("symlabel", Some(label.into()), false);
    let mut c = Client::connect(&h);
    c.handshake(true);
    assert_eq!(
        c.ok("emulator/status", json!({}))["symbolsPath"],
        json!(label),
        "a label that names no file is not a path this process can speak for, so it is reported as given"
    );
}

// ------------------------------------------------------------------ M1

#[test]
fn load_symbols_reply_path_and_a_following_status_agree_exactly() {
    // §11.30 M1, and its real content is the *exactly*: "one method never reports one file under two
    // spellings in one exchange." A reply that echoed the caller while `status` reported the resolved
    // path would leave a client holding two strings for one file and no way to tell that they are one.
    let lst = temp_path("m1", "lst");
    std::fs::write(&lst, LST).expect("write the listing");
    let canonical = std::fs::canonicalize(&lst).expect("canonicalize");
    let rel = relative_to_cwd(&canonical);
    assert_is_a_relative_spelling_of(&rel, &canonical);

    let h = spawn("symm1");
    let mut c = Client::connect(&h);
    c.handshake(false);

    let reply = c.ok("emulator/load_symbols", json!({ "path": rel }));
    let reply_path = reply["path"].as_str().expect("the reply carries path");
    assert!(
        Path::new(reply_path).is_absolute(),
        "§11.30 M1: load_symbols' reply `path` moves with symbolsPath. Caller sent {rel}, got \
         {reply_path}"
    );
    assert_eq!(Path::new(reply_path), canonical);

    let status_path = c.ok("emulator/status", json!({}))["symbolsPath"]
        .as_str()
        .expect("status carries symbolsPath")
        .to_string();
    assert_eq!(
        reply_path, status_path,
        "the same method's reply and the next status must be byte-identical strings, not merely two \
         paths that happen to resolve to one file — a client comparing them has only the strings"
    );

    let _ = std::fs::remove_file(&lst);
}

// ------------------------------------------------------------------ M2

#[test]
fn screenshot_reports_an_absolute_path_when_the_caller_supplied_a_relative_one() {
    // §11.30 M2. This key LOOKED compliant before the ruling and was not: it is absolute in the default
    // case only by the accident that the default is built from `temp_dir()`. A caller who passed a
    // relative `path` got it straight back, and had no way to find the file it named.
    //
    // The relative spelling points into the temp dir so the test writes nothing into the source tree.
    let target = temp_path("m2", "png");
    // The file does not exist yet, so `relative_to_cwd` is built from the directory that does.
    let dir =
        std::fs::canonicalize(target.parent().expect("temp dir has a parent")).expect("canon");
    let name = target.file_name().expect("a file name").to_owned();
    let rel = format!("{}/{}", relative_to_cwd(&dir), name.to_string_lossy());
    assert!(
        Path::new(&rel).is_relative(),
        "the input under test must be relative; got {rel}"
    );

    let h = spawn("symm2");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let r = c.ok("emulator/screenshot", json!({ "path": rel }));
    let reported = r["path"].as_str().expect("screenshot reports path");

    assert!(
        Path::new(reported).is_absolute(),
        "§11.30 M2: `screenshot.path` is in the enumeration too. Caller sent {rel}, got {reported}"
    );
    assert_eq!(
        Path::new(reported),
        std::fs::canonicalize(&rel).expect("the image the server says it wrote must exist"),
        "the reported path must name the file that was actually written"
    );

    let _ = std::fs::remove_file(&target);
}

// ------------------------------------------------------------------ M3

#[test]
fn a_refused_load_symbols_quotes_the_callers_own_spelling() {
    // §11.30's deliberate exception, and the reason it must be TESTED rather than merely written down:
    // the next person to "tidy" the error paths will absolutise them, and nothing else in the suite
    // would notice. A refusal describes the REQUEST; a success describes the STATE.
    //
    // **Sharp, not vacuous:** the file EXISTS at the relative spelling, so `absolutise` would resolve
    // it. It is the *parse* that fails. A nonexistent path would pass through unchanged and prove
    // nothing about the placement of the call.
    let lst = temp_path("m3lst", "lst");
    std::fs::write(&lst, "this is not a symbol listing at all\n").expect("write the garbage");
    let canonical = std::fs::canonicalize(&lst).expect("canonicalize");
    let rel = relative_to_cwd(&canonical);
    assert_is_a_relative_spelling_of(&rel, &canonical);
    assert_ne!(
        rel,
        canonical.display().to_string(),
        "the premise: absolutising this input would visibly change it, so a raw echo is a real finding"
    );

    let h = spawn("symm3a");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let e = c.err("emulator/load_symbols", json!({ "path": rel.clone() }));
    assert_eq!(
        e["data"]["path"],
        json!(rel),
        "a refusal quotes the caller's own spelling back — a client debugging a bad path wants to see \
         what it sent, not what this process made of it"
    );
    assert!(
        e["message"].as_str().expect("a message").contains(&rel),
        "the message names the caller's spelling too: {}",
        e["message"]
    );

    let _ = std::fs::remove_file(&lst);
}

#[test]
fn a_refused_reload_rom_quotes_the_callers_own_spelling() {
    // The call site §11.30 quotes by name. Same sharpness requirement: the path must RESOLVE, or
    // `absolutise` would be a no-op and the test would witness nothing. A directory resolves and cannot
    // be read as a ROM.
    let dir = temp_path("m3dir", "d");
    std::fs::create_dir_all(&dir).expect("make the directory");
    let canonical = std::fs::canonicalize(&dir).expect("canonicalize");
    let rel = relative_to_cwd(&canonical);
    assert!(Path::new(&rel).is_relative(), "relative premise: {rel}");
    assert_ne!(rel, canonical.display().to_string());
    assert!(
        std::fs::read(&rel).is_err(),
        "the premise: this path must resolve but fail to READ, so absolutise would have changed it"
    );

    let h = spawn("symm3b");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let e = c.err("emulator/reload_rom", json!({ "path": rel.clone() }));
    assert_eq!(e["data"]["path"], json!(rel));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_refused_screenshot_quotes_the_callers_own_spelling() {
    // The third refusal site, for the key M2 just moved. `screenshot` absolutises AFTER the write; if
    // someone were to hoist that above the write, this is what would catch it.
    //
    // **Sharpness again, and this one was caught being blunt.** The first draft used
    // `no-such-directory-here/shot.png`, which does not resolve — so `absolutise` passes it through and
    // the assertion held even with the error site deliberately mutated to absolutise. The probe that
    // fired on the other two refusals was silent here, which is exactly the vacuous-guard shape this
    // file's header warns about. A directory resolves (so absolutising would visibly change it) and
    // cannot be written as a file.
    let dir = temp_path("m3shot", "d");
    std::fs::create_dir_all(&dir).expect("make the directory");
    let canonical = std::fs::canonicalize(&dir).expect("canonicalize");
    let rel = relative_to_cwd(&canonical);
    assert!(Path::new(&rel).is_relative(), "relative premise: {rel}");
    assert_ne!(rel, canonical.display().to_string());
    assert!(
        std::fs::write(&rel, b"x").is_err(),
        "the premise: this path must resolve but fail to WRITE, so absolutise would have changed it"
    );

    let h = spawn("symm3c");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let e = c.err("emulator/screenshot", json!({ "path": rel.clone() }));
    assert_eq!(e["data"]["path"], json!(rel));

    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------------------------------ the boundary, not the reader

#[test]
fn a_checkpoint_restore_does_not_de_absolutise_the_symbols_path() {
    // §11.30's "at the load boundary so every route agrees" has one route that is not a load:
    // `emulator/restore` puts a path back from a slot rather than resolving one. It is correct only
    // because the value went INTO the slot already resolved. That is an invariant about two functions
    // far apart in the file, so it is asserted rather than remembered.
    let lst = temp_path("cp", "lst");
    std::fs::write(&lst, LST).expect("write the listing");
    let canonical = std::fs::canonicalize(&lst).expect("canonicalize");
    let rel = relative_to_cwd(&canonical);
    assert_is_a_relative_spelling_of(&rel, &canonical);

    let h = spawn("symcp");
    let mut c = Client::connect(&h);
    c.handshake(false);
    c.ok("emulator/load_symbols", json!({ "path": rel }));
    let id = c.ok("emulator/checkpoint", json!({}))["id"].clone();
    c.ok("emulator/run_frames", json!({"frames": 2}));
    c.ok("emulator/restore", json!({ "id": id }));

    let after = c.ok("emulator/status", json!({}))["symbolsPath"]
        .as_str()
        .expect("status carries symbolsPath after a restore")
        .to_string();
    assert_eq!(
        Path::new(&after),
        canonical,
        "a restore must not hand back a spelling the load boundary had already resolved"
    );

    let _ = std::fs::remove_file(&lst);
}
