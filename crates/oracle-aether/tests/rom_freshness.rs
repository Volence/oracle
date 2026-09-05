//! **§11.37 / CR-N — the emulator is running an older image than the file on disk, and it says so.**
//!
//! The only stale-image warning this suite had was `romFreshness` in
//! `oracle-old/linux-port/mcp/oracle_mcp.py` — the legacy MCP shim, computed client-side, in the repo the
//! cutover exists to delete. Nothing in this server replaced it, and `grep` for `romFreshness`,
//! `differsBy` or `bytesCompared` across `crates/` found comments and nothing else.
//!
//! It is not a theoretical loss. On 2026-09-04 a seat answering a cross-lane diagnostic was holding a
//! **742,018-byte** image while the file at its own `romPath` was **844,730** bytes — a build from before
//! the other lane's landing. The shim's banner caught it on the first call. Without it that seat would
//! have run a per-scanline measurement against the wrong binary and published a confident finding about
//! another lane's work, to that lane. **Nothing errors. No method 404s. The numbers look plausible.**
//!
//! ## The rows §8 item 27 requires, and the one that is the point
//!
//! A server advertising `status` with `romPath` MUST show, in its own suite: the size-differs case; the
//! **same-size-different-bytes** case ("the required row, since two builds shared a byte count on
//! 2026-09-04"); a "could not check" caveat when the file cannot be read; and **NO** ROM caveat when the
//! bytes match, "so the quiet state is a real assertion".
//!
//! ⚑ Size is definitive only when it **differs**. A size mismatch is conclusive; matching sizes prove
//! nothing and must escalate to a real byte comparison. That is
//! [`a_rebuilt_image_of_the_same_size_is_named_on_status`], and it is the row this file exists for: a
//! size-only check passes every other row here.
//!
//! ## The independent channel
//!
//! A test that asserted only "the verdict says stale" would be our own state checked against our own
//! opinion of it. So the same-size row also asks `emulator/read_memory` for the byte that moved, and
//! asserts the server hands back the **old** one while the file on disk holds the new one — the
//! observable that does the misleading, and the reason a stale image is silent rather than loud.
//!
//! What is NOT here is the *cost* of serving the verdict on a per-frame method. That is measured in
//! `engine.rs`'s own test module against a private counter, because a cache that silently re-reads a
//! whole cartridge image every call produces byte-identical sentences and leaves every row below green.
//! Tonight's symbol parcel proved that the hard way: mutating its cache to invalidate on every call left
//! all twelve of its wire rows passing.

mod common;

use common::{spawn, spawn_with_rom_file, Client};
use serde_json::{json, Value};
use std::path::PathBuf;

/// The `caveat` on a reply, or `None` when the reply is quiet. Same key, same §2.4 string, same accessor
/// as `symbol_freshness.rs` — the two verdicts share the key by ruling, not by accident.
fn caveat(reply: &Value) -> Option<String> {
    reply
        .get("caveat")
        .map(|c| c.as_str().expect("a caveat is a string (§2.4)").to_string())
}

fn status_caveat(c: &mut Client) -> Option<String> {
    caveat(&c.ok("emulator/status", json!({})))
}

/// A unique temp path per call — this binary's tests run in parallel in one process.
fn temp_path(tag: &str, ext: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "ae-romfresh-{}-{tag}-{n}.{ext}",
        std::process::id()
    ))
}

/// A minimal AS-dialect listing that binds to `testrom::build()`, and its one-row-longer rebuild — the
/// pair `symbol_freshness.rs` uses, needed here only by the composition row.
const LST_OLD: &str = "\
  Symbol Table (* = unused):
  --------------------------

 EntryPoint : 200 C |
 Player_1 : FFFF8CFA C |

    2 symbols
    0 unused symbols
";

const LST_REBUILT: &str = "\
  Symbol Table (* = unused):
  --------------------------

 EntryPoint : 200 C |
 Level_Width : 400 C |
 Player_1 : FFFF8CFA C |

    3 symbols
    0 unused symbols
";

/// The offset the same-size rows rewrite. Inside `testrom::build()` (0x300 bytes), past every
/// instruction it lays down, and reachable through `emulator/read_memory` as cartridge address `$0002FF`
/// — so the same byte is both the thing that changed and the thing a client can ask the server for.
const POKE_OFFSET: usize = 0x2FF;
/// What that byte is in the image the server booted (`build()` zero-fills, then writes code low down).
const POKE_WAS: u8 = 0x00;
/// What the rebuilt file has there instead. Any non-zero value; `0xA5` is chosen only to be unmistakable
/// in a hex dump.
const POKE_NOW: u8 = 0xA5;

// ---------------------------------------------------------------------------------------------------
// §8 item 27, row 4 — the CONTROL. Listed first because everything below is worthless without it.
// ---------------------------------------------------------------------------------------------------

/// **Quiet is a real assertion, not an absence of one.**
///
/// An implementation that emitted the caveat unconditionally would satisfy every other row in this file
/// and carry no information at all — the class of check that passes because it refuses everything. §2.4
/// and §11.27 make an always-on caveat a MUST NOT for exactly this reason, and §11.37 raised the quiet
/// state to a contract obligation of its own.
///
/// The polling shape is deliberate: `status` is what a UI calls every frame, and a verdict that drifted
/// loud after N calls — because a cache confused itself — would put a permanent false alarm on the glass
/// of every session that changed nothing.
#[test]
fn an_untouched_image_leaves_status_quiet() {
    let (h, rom) = spawn_with_rom_file("romfresh-quiet");
    let mut c = Client::connect(&h);
    c.handshake(false);

    let status = c.ok("emulator/status", json!({}));
    assert_eq!(
        status["romPath"],
        json!(std::fs::canonicalize(&rom)
            .expect("canonicalize the ROM fixture")
            .display()
            .to_string()),
        "the premise: this server really does advertise a romPath, which is what §8 item 27's \
         obligation is conditioned on. Reply: {status}"
    );
    assert_eq!(
        status["romBytes"],
        json!(std::fs::metadata(&rom).expect("stat the ROM fixture").len()),
        "the premise: the image held and the file on disk are the same size to begin with"
    );

    for i in 0..30 {
        assert_eq!(
            status_caveat(&mut c),
            None,
            "call {i}: nothing has moved, so nothing is said"
        );
    }

    // ANTI-VACUITY: the verdict is still LIVE after 30 quiet polls. Without this the loop above is
    // satisfied by a build whose ROM check was deleted outright.
    let mut bytes = std::fs::read(&rom).expect("read the ROM fixture");
    bytes.push(0x00);
    std::fs::write(&rom, &bytes).expect("grow the ROM file");
    assert!(
        status_caveat(&mut c).is_some(),
        "ANTI-VACUITY: after 30 quiet polls the verdict must still fire. A cache that latched quiet \
         would be a way to be confidently stale about staleness."
    );

    let _ = std::fs::remove_file(&rom);
}

// ---------------------------------------------------------------------------------------------------
// §8 item 27, row 1 — stale by SIZE. The case that fired in the field.
// ---------------------------------------------------------------------------------------------------

/// **742,018 held against 844,730 on disk, in miniature.**
///
/// A size mismatch is conclusive and is reached from `metadata` alone — the whole image never enters
/// memory to answer it. The sentence must name what is held, what the path holds now, and the fix,
/// because all three were what made the field sighting actionable on the first call.
#[test]
fn a_rebuilt_image_of_a_different_size_is_named_on_status() {
    let (h, rom) = spawn_with_rom_file("romfresh-size");
    let rom_abs = std::fs::canonicalize(&rom).expect("canonicalize");
    let mut c = Client::connect(&h);
    c.handshake(false);

    let held = c.ok("emulator/status", json!({}))["romBytes"]
        .as_u64()
        .expect("romBytes is a number");
    assert_eq!(
        status_caveat(&mut c),
        None,
        "OVER-FIRING CONTROL: before anything moves, status is quiet"
    );

    // The peer lane rebuilds. Nothing at all happens on this connection — no reload, no event.
    let mut bigger = std::fs::read(&rom).expect("read the ROM fixture");
    bigger.extend_from_slice(&[0xFFu8; 64]);
    std::fs::write(&rom, &bigger).expect("rewrite the ROM larger");

    let status = c.ok("emulator/status", json!({}));
    assert_eq!(
        status["romBytes"],
        json!(held),
        "the misleading observable is untouched, deliberately: romBytes answers 'how big is what I \
         hold', which is honest and is not the question"
    );
    let text = caveat(&status).unwrap_or_else(|| {
        panic!("a status whose image has been rebuilt must SAY SO. Reply: {status}")
    });
    for needle in [
        rom_abs.display().to_string().as_str(),
        &format!("{held} byte(s)"),
        &format!("{} byte(s) now", bigger.len()),
        "emulator/reload_rom",
    ] {
        assert!(
            text.contains(needle),
            "the caveat must name {needle:?} — what is held, what the path holds now, and the fix. \
             Got: {text}"
        );
    }

    // ANTI-VACUITY: the fix the sentence NAMES must actually silence it, or it is advice that does not
    // work.
    c.ok("emulator/reload_rom", json!({}));
    assert_eq!(
        status_caveat(&mut c),
        None,
        "ANTI-VACUITY: after emulator/reload_rom the server holds the file's bytes and goes quiet"
    );

    let _ = std::fs::remove_file(&rom);
}

// ---------------------------------------------------------------------------------------------------
// ⚑ §8 item 27, row 2 — SAME SIZE, DIFFERENT BYTES. The point of this parcel.
// ---------------------------------------------------------------------------------------------------

/// **The case a size-only check misses, and the one aeon actually hit.**
///
/// Two different builds shared a byte count on 2026-09-04. Every other row in this file passes against an
/// implementation that stops at `metadata().len()`; this one is the only thing standing between that
/// implementation and a green suite.
///
/// The independent channel is the second half: `emulator/read_memory` is asked for the byte that moved
/// and must hand back the **old** value while the file holds the new one. That is the observable that
/// does the misleading — a measurement taken here would be about the previous build, and would look
/// exactly like an answer.
#[test]
fn a_rebuilt_image_of_the_same_size_is_named_on_status() {
    let (h, rom) = spawn_with_rom_file("romfresh-bytes");
    let rom_abs = std::fs::canonicalize(&rom).expect("canonicalize");
    let mut c = Client::connect(&h);
    c.handshake(false);

    let original = std::fs::read(&rom).expect("read the ROM fixture");
    assert_eq!(
        original[POKE_OFFSET], POKE_WAS,
        "the premise: the fixture really does hold {POKE_WAS:#04X} at {POKE_OFFSET:#06X}"
    );
    assert_eq!(
        status_caveat(&mut c),
        None,
        "OVER-FIRING CONTROL: before anything moves, status is quiet"
    );

    let mut rebuilt = original.clone();
    rebuilt[POKE_OFFSET] = POKE_NOW;
    std::fs::write(&rom, &rebuilt).expect("rewrite the ROM in place");

    // The premises of the whole row, derived rather than assumed.
    assert_eq!(
        original.len(),
        rebuilt.len(),
        "THE PREMISE: the two images are the same LENGTH, so a size check cannot separate them"
    );
    assert_ne!(
        original, rebuilt,
        "…and they must nonetheless differ, or there is nothing here to detect"
    );
    assert_eq!(
        std::fs::metadata(&rom).expect("stat").len(),
        original.len() as u64,
        "and the FILE's size on disk is unchanged — the check has nothing cheap left to go on"
    );

    let status = c.ok("emulator/status", json!({}));
    let text = caveat(&status).unwrap_or_else(|| {
        panic!(
            "SAME SIZE, DIFFERENT BYTES: a size check would call this current. It is not. Reply: \
             {status}"
        )
    });
    for needle in [
        rom_abs.display().to_string().as_str(),
        "SAME size",
        "1 byte(s) differ",
        &format!("${POKE_OFFSET:06X}"),
        "emulator/reload_rom",
    ] {
        assert!(
            text.contains(needle),
            "the caveat must name {needle:?} — including WHERE, since 'differs somewhere in the \
             image' is not something a reader can act on. Got: {text}"
        );
    }

    // ---- the independent channel: the observable that actually does the misleading ----
    let read = c.ok(
        "emulator/read_memory",
        json!({"addr": format!("0x{POKE_OFFSET:06X}"), "len": 1}),
    );
    assert_eq!(
        read["bytes"],
        json!(format!("0x{POKE_WAS:02X}")),
        "status is a verdict, not a re-read: the server is still SERVING the old image's byte while \
         the file on disk holds {POKE_NOW:#04X}. This is what a stale image looks like from the \
         client's side — a plausible number, no error, no clue."
    );

    // ANTI-VACUITY: after the correct action the byte changes AND the caveat goes.
    c.ok("emulator/reload_rom", json!({}));
    let read = c.ok(
        "emulator/read_memory",
        json!({"addr": format!("0x{POKE_OFFSET:06X}"), "len": 1}),
    );
    assert_eq!(
        read["bytes"],
        json!(format!("0x{POKE_NOW:02X}")),
        "ANTI-VACUITY: after the reload the server must serve the NEW byte. If it does not, the \
         assertion above passes on a build where read_memory never sees the cartridge at all."
    );
    assert_eq!(
        status_caveat(&mut c),
        None,
        "ANTI-VACUITY: and the fix the sentence names must silence it"
    );

    let _ = std::fs::remove_file(&rom);
}

// ---------------------------------------------------------------------------------------------------
// §8 item 27, row 3 — "could not check". Loud on unmeasurable, in both of its shapes.
// ---------------------------------------------------------------------------------------------------

/// **"I could not look" must never render as "I looked and it is fine".**
///
/// A failed `stat` is not "unchanged", and the filter in front of the check must not turn it into the
/// silence that means *I looked*. It must also stay loud: a cache that remembered an unmeasurable verdict
/// would be remembering an answer keyed on a fingerprint it never obtained.
#[test]
fn a_status_whose_image_vanished_says_it_could_not_check() {
    let (h, rom) = spawn_with_rom_file("romfresh-gone");
    let rom_abs = std::fs::canonicalize(&rom).expect("canonicalize");
    let mut c = Client::connect(&h);
    c.handshake(false);
    assert_eq!(
        status_caveat(&mut c),
        None,
        "the premise: quiet while the file is there, and the cache is warm when it goes"
    );

    std::fs::remove_file(&rom).expect("delete the ROM out from under the server");

    for i in 0..5 {
        let text = status_caveat(&mut c).unwrap_or_else(|| {
            panic!("call {i}: an unmeasurable ROM freshness must be LOUD, never quiet")
        });
        assert!(
            text.contains("could NOT be checked"),
            "call {i}: it must say it could not look, not imply it looked: {text}"
        );
        assert!(
            text.contains(&rom_abs.display().to_string()),
            "call {i}: and name the path it could not read: {text}"
        );
    }

    // ANTI-VACUITY: put the file back and the quiet returns. Without this every assertion above is
    // satisfied by a build that is loud unconditionally.
    std::fs::write(&rom, oracle_core::testrom::build()).expect("restore the ROM file");
    assert_eq!(
        status_caveat(&mut c),
        None,
        "ANTI-VACUITY: the same image back on disk is quiet again"
    );

    let _ = std::fs::remove_file(&rom);
}

/// **The other unmeasurable: an image that was never on disk at all.**
///
/// Not one of item 27's four rows — its obligation is conditioned on a server that *has* a `romPath` —
/// but it is the same rule, and it is the state `common::spawn` produces: `testrom::build()` loaded from
/// memory, `romPath: null`. §11.34's listing verdict says the same thing for a table held with no
/// recorded path, and the two must not disagree about what silence means.
///
/// The distinction being defended is *unmeasured* versus *measured and fine*. `romPath: null` beside a
/// silent `caveat` reads as the second; only the sentence makes it the first.
#[test]
fn an_image_that_was_never_on_disk_says_it_could_not_check() {
    let h = spawn("romfresh-nopath");
    let mut c = Client::connect(&h);
    c.handshake(false);

    let status = c.ok("emulator/status", json!({}));
    assert_eq!(
        status["romPath"],
        json!(null),
        "the premise: this server booted an image that came from no file. Reply: {status}"
    );
    let text = caveat(&status).unwrap_or_else(|| {
        panic!("an unmeasurable ROM must be LOUD even when the reason is that there is no path: {status}")
    });
    assert!(
        text.contains("could NOT be checked") && text.contains("no path"),
        "it must say both that it could not check and why: {text}"
    );
    assert!(
        !text.contains("emulator/reload_rom"),
        "and it must NOT prescribe a reload, which has no path to reload from: {text}"
    );
}

// ---------------------------------------------------------------------------------------------------
// Composition — both verdicts, one string, ROM FIRST.
// ---------------------------------------------------------------------------------------------------

/// **Two sentences, one `caveat`, the ROM one first** (§11.37: *"a stale image makes the listing question
/// moot"*).
///
/// The order is contract text rather than taste, and it is worth a row of its own because the natural way
/// to write this — appending each verdict as it is computed — puts them in whatever order the handler
/// happens to compute them in, and that order is invisible until someone reads a caveat under pressure.
#[test]
fn both_verdicts_fire_together_and_the_rom_sentence_comes_first() {
    let (h, rom) = spawn_with_rom_file("romfresh-compose");
    let rom_abs = std::fs::canonicalize(&rom).expect("canonicalize");
    let lst = temp_path("compose", "lst");
    std::fs::write(&lst, LST_OLD).expect("write the listing fixture");
    let lst_abs = std::fs::canonicalize(&lst).expect("canonicalize");

    let mut c = Client::connect(&h);
    c.handshake(false);
    c.ok(
        "emulator/load_symbols",
        json!({ "path": lst.display().to_string() }),
    );
    assert_eq!(
        status_caveat(&mut c),
        None,
        "OVER-FIRING CONTROL: both files match what is held, so both halves are quiet"
    );

    // Both peers land at once: the image is rebuilt AND the listing is rewritten.
    let mut bigger = std::fs::read(&rom).expect("read the ROM fixture");
    bigger.extend_from_slice(&[0xFFu8; 16]);
    std::fs::write(&rom, &bigger).expect("rewrite the ROM larger");
    std::fs::write(&lst, LST_REBUILT).expect("rewrite the listing");

    let text = status_caveat(&mut c).expect("both verdicts have something to say");
    let rom_at = text
        .find(&rom_abs.display().to_string())
        .expect("the ROM sentence must be present in the composed string");
    let lst_at = text
        .find(&lst_abs.display().to_string())
        .expect("the LISTING sentence must be present in the composed string");
    assert!(
        rom_at < lst_at,
        "ROM FIRST (§11.37): a stale image makes the listing question moot, so the reader who reads \
         one sentence must read that one. Got ROM at {rom_at}, listing at {lst_at}: {text}"
    );
    assert!(
        text.contains("emulator/reload_rom") && text.contains("emulator/load_symbols"),
        "each half must still name its own fix; a composed string that dropped one is worse than \
         either alone: {text}"
    );

    // ANTI-VACUITY, per half: fixing one leaves exactly the other speaking. Without this the ordering
    // assertion above is satisfied by a build that emits one fixed string containing both paths.
    c.ok("emulator/reload_rom", json!({}));
    let only_listing = status_caveat(&mut c).expect("the listing is still stale");
    assert!(
        !only_listing.contains(&rom_abs.display().to_string())
            && only_listing.contains(&lst_abs.display().to_string()),
        "after the reload the ROM half must go silent and the listing half must remain: {only_listing}"
    );
    c.ok(
        "emulator/load_symbols",
        json!({ "path": lst.display().to_string() }),
    );
    assert_eq!(
        status_caveat(&mut c),
        None,
        "and with both fixed, the key is absent again"
    );

    let _ = std::fs::remove_file(&rom);
    let _ = std::fs::remove_file(&lst);
}
