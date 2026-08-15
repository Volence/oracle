//! The thin method set, and the three recon §4 non-negotiables it is built around:
//! every reply stamped, every array bounded, everything approximate caveated.

mod common;

use common::{spawn, Client};
use oracle_aether::engine::METHODS;
use oracle_core::system::MCLK_PER_FRAME;
use serde_json::{json, Value};

/// A `.lst` with no `EndOfRom` row — internally intact, so it is accepted *unverified* with a caveat.
const LST_UNBOUND: &str = "\
  Symbol Table (* = unused):
  --------------------------

 EntryPoint : 200 C |
 $engine.boot$EntryPoint$wait_dma : 214 C |
 $engine.boot$EntryPoint$warm_boot : 218 C |
 Player_1 : FFFF8CFA C |
 Player_2 : FFFF8D4A C |

    5 symbols
    0 unused symbols
";

/// A `.lst` that declares an `EndOfRom` the image does not corroborate — must be REFUSED.
const LST_WRONG_SHAPE: &str = "\
  Symbol Table (* = unused):
  --------------------------

 EntryPoint : 200 C |
 EndOfRom : 40 C |

    2 symbols
    0 unused symbols
";

fn write_lst(tag: &str, text: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("ae-{tag}-{}.lst", std::process::id()));
    std::fs::write(&p, text).unwrap();
    p
}

// ------------------------------------------------------------------ non-negotiable 1

#[test]
fn every_reply_from_every_method_carries_frame_mclk_and_running() {
    let h = spawn("stamp");
    let mut c = Client::connect(&h);

    // Including the handshake reply itself.
    let init = c.handshake(true);
    for k in ["frame", "mclk", "running"] {
        assert!(init.get(k).is_some(), "initialize result is missing `{k}`");
    }

    for m in METHODS {
        let v = c.call(m.name, json!({}));
        // Success carries it in `result`; failure carries it in `error.data`. Both are replies.
        let holder = if let Some(r) = v.get("result") {
            r.clone()
        } else {
            v["error"]["data"].clone()
        };
        for k in ["frame", "mclk", "running"] {
            assert!(
                holder.get(k).is_some(),
                "{} reply is missing `{k}`: {v}",
                m.name
            );
        }
    }
}

#[test]
fn the_stamp_is_emulated_and_advances_only_with_emulated_time() {
    let h = spawn("emulated");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let a = c.ok("emulator/status", json!({}));
    // Wall-clock time passes; the stamp must not.
    std::thread::sleep(std::time::Duration::from_millis(120));
    let b = c.ok("emulator/status", json!({}));
    assert_eq!(
        a["mclk"], b["mclk"],
        "the stamp must be emulated, not wall-clock"
    );

    let n = c.ok("emulator/run_frames", json!({"frames": 5}));
    assert_eq!(
        n["frame"].as_u64().unwrap(),
        a["frame"].as_u64().unwrap() + 5
    );
}

#[test]
fn an_error_reply_is_stamped_too() {
    let h = spawn("errstamp");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let e = c.err("emulator/read_memory", json!({"addr": "0xA10000"}));
    assert_eq!(e["code"], json!(-32004));
    assert!(e["data"]["frame"].is_number());
    assert!(e["data"]["mclk"].is_number());
    assert!(e["data"]["running"].is_boolean());
}

// ------------------------------------------------------------------ non-negotiable 2

#[test]
fn arrays_are_bounded_cursored_and_flag_truncation() {
    let h = spawn("bounded");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let lst = write_lst("bounded", LST_UNBOUND);
    c.ok(
        "emulator/load_symbols",
        json!({"path": lst.display().to_string()}),
    );

    // "Player" prefix-matches two symbols; the shape is the bounded envelope either way.
    let r = c.ok("emulator/lookup_symbol", json!({"name": "Play"}));
    let m = &r["otherMatches"];
    assert_eq!(m["total"], json!(2));
    assert_eq!(m["returned"], json!(2));
    assert_eq!(m["truncated"], json!(false));
    assert_eq!(m["nextCursor"], json!(null));
    assert!(m["items"].is_array());
    assert!(m["limit"].is_number());
}

#[test]
fn an_out_of_range_length_is_refused_loudly_never_clamped() {
    let h = spawn("clamp");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let e = c.err(
        "emulator/read_memory",
        json!({"addr": "0x000000", "len": 99999}),
    );
    assert_eq!(e["code"], json!(-32602));
    assert!(
        e["message"].as_str().unwrap().contains("99999"),
        "the refusal must name the offending value: {}",
        e["message"]
    );
    // And the legal ceiling really does work, so the refusal is a bound and not a bug. (Work RAM is
    // 64 KiB, so 4096 fits; the fixture ROM is smaller than the cap.)
    let ok = c.ok(
        "emulator/read_memory",
        json!({"addr": "0xFF0000", "len": 4096}),
    );
    assert_eq!(ok["len"], json!(4096));
    assert_eq!(ok["bytes"].as_str().unwrap().len(), 2 + 4096 * 2);
}

// ------------------------------------------------------------------ non-negotiable 3

#[test]
fn approximate_answers_carry_a_caveat() {
    let h = spawn("caveat");
    let mut c = Client::connect(&h);
    c.handshake(false);

    // A debug read bypasses the bus.
    let r = c.ok(
        "emulator/read_memory",
        json!({"addr": "0x000000", "len": 4}),
    );
    assert!(r["caveat"].as_str().unwrap().contains("bypassing the bus"));

    // A whole-frame render is not scanline-accurate.
    let png = std::env::temp_dir().join(format!("ae-shot-{}.ppm", std::process::id()));
    let r = c.ok(
        "emulator/screenshot",
        json!({"path": png.display().to_string()}),
    );
    assert!(r["caveat"].as_str().unwrap().contains("scanline-accurate"));

    // state_hash covers VDP state only.
    let r = c.ok("emulator/state_hash", json!({}));
    assert!(r["caveat"].as_str().unwrap().contains("VDP state only"));

    // A run that ends on its bound says so, loudly.
    let r = c.ok(
        "emulator/run_to",
        json!({"addr": "0x00BEEF", "maxFrames": 1}),
    );
    assert_eq!(r["reached"], json!(false));
    assert!(r["caveat"].as_str().unwrap().contains("never reached"));

    // A nearest-preceding symbol match is a guess.
    let lst = write_lst("caveat", LST_UNBOUND);
    c.ok(
        "emulator/load_symbols",
        json!({"path": lst.display().to_string()}),
    );
    let r = c.ok("emulator/lookup_symbol", json!({"addr": "0x000210"}));
    assert_eq!(r["name"], json!("EntryPoint+$10"));
    assert_eq!(r["disp"], json!(16));
    assert!(r["caveat"].as_str().unwrap().contains("nearest"));
}

// ------------------------------------------------------------------ methods

#[test]
fn status_reports_the_machine_and_the_symbol_at_pc() {
    let h = spawn("status");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let s = c.ok("emulator/status", json!({}));
    assert!(s["pc"].as_str().unwrap().starts_with("0x"));
    assert!(s["sr"].as_str().unwrap().starts_with("0x"));
    assert_eq!(s["symbolCount"], json!(0));
    assert_eq!(s["running"], json!(false));
    // `frameToken` is the emulated frame index, not a UI counter (recon §5 C2).
    assert_eq!(s["frameToken"], s["frame"]);
}

#[test]
fn registers_are_hex_strings_per_d9() {
    let h = spawn("regs");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let r = c.ok("emulator/registers", json!({}));
    for k in ["d0", "d7", "a0", "a7", "pc", "sp", "usp", "ssp", "sr"] {
        assert!(
            r[k].as_str().is_some_and(|s| s.starts_with("0x")),
            "`{k}` must be a hex string (D9), got {}",
            r[k]
        );
    }
}

#[test]
fn run_to_reaches_a_real_target_and_reports_fired_vs_timed_out_distinctly() {
    let h = spawn("runto");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let pc = c.ok("emulator/status", json!({}))["pc"]
        .as_str()
        .unwrap()
        .to_string();
    // The testrom loops, so its entry PC recurs — a target that genuinely fires.
    let r = c.ok("emulator/run_to", json!({"addr": pc, "maxFrames": 5}));
    assert_eq!(r["reached"], json!(true));
    assert_eq!(r["pc"], json!(pc));
    assert!(r.get("caveat").is_none(), "a fired run needs no caveat");
    // CR-13 (ruled 2026-08-15): `stoppedAtFrame`/`stoppedAtMclk` are removed because they *were* the
    // envelope stamp (§2.2). Pinned as **lossless**, not merely deleted — the two numbers they carried
    // are asserted here on the envelope. The target is the entry PC, which matches before anything
    // executes, so the halt coordinate is the origin.
    assert_no_stopped_at(&r);
    assert_eq!(r["frame"], json!(0), "the run fired before it advanced");
    assert_eq!(r["mclk"], json!(0));

    let miss = c.ok(
        "emulator/run_to",
        json!({"addr": "0x00FEED", "maxFrames": 1}),
    );
    assert_eq!(miss["reached"], json!(false));
    // The deadlined branch carries the same proof: the stamp is the *halt* coordinate. The run ended on
    // its one-frame bound, so `frame` is 1 and `mclk` is one frame's worth of master clock plus however
    // far the last instruction ran past the boundary — precisely what `stoppedAtMclk` used to report.
    assert_no_stopped_at(&miss);
    let frame = miss["frame"].as_u64().expect("`frame` on the envelope");
    let mclk = miss["mclk"].as_u64().expect("`mclk` on the envelope");
    assert_eq!(frame, 1, "one frame of bound was consumed");
    assert_eq!(frame, mclk / MCLK_PER_FRAME, "the stamp is self-consistent");
    assert!(
        (MCLK_PER_FRAME..2 * MCLK_PER_FRAME).contains(&mclk),
        "the halt mclk must be at-or-just-past the one-frame bound, got {mclk}"
    );
    // And nothing advanced between the halt and the stamp: a call that cannot move the machine reads
    // back the identical coordinate. That is the whole argument for the removal, executed.
    let s = c.ok("emulator/status", json!({}));
    assert_eq!(s["frame"], miss["frame"]);
    assert_eq!(s["mclk"], miss["mclk"]);
}

/// The two keys CR-13 removed from `run_to`, asserted absent — so the removal is pinned rather than
/// merely done, and re-introducing either fails a test instead of shipping.
fn assert_no_stopped_at(r: &Value) {
    for k in ["stoppedAtFrame", "stoppedAtMclk"] {
        assert!(
            r.get(k).is_none(),
            "`{k}` duplicated the envelope stamp and is removed (CR-13); got {r}"
        );
    }
}

#[test]
fn read_memory_resolves_symbols_and_refuses_unmapped_addresses() {
    let h = spawn("read");
    let mut c = Client::connect(&h);
    c.handshake(false);

    // ROM byte 0 of the fixture ROM is the initial SSP's top byte — read it back byte-identically.
    let rom = oracle_core::testrom::build();
    let r = c.ok(
        "emulator/read_memory",
        json!({"addr": "0x000000", "len": 8}),
    );
    let expect = format!(
        "0x{}",
        rom[..8]
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<String>()
    );
    assert_eq!(r["bytes"], json!(expect));
    assert_eq!(r["region"], json!("cartridge ROM"));

    // Work RAM is decoded at $E00000-$FFFFFF.
    let r = c.ok(
        "emulator/read_memory",
        json!({"addr": "0xFF0000", "len": 2}),
    );
    assert_eq!(r["region"], json!("work RAM"));

    // Unmapped -> -32004, never open-bus garbage from a debug read.
    assert_eq!(
        c.err("emulator/read_memory", json!({"addr": "0xA10000"}))["code"],
        json!(-32004)
    );
    // Running off the end of a region is a refusal, not a short read.
    assert_eq!(
        c.err(
            "emulator/read_memory",
            json!({"addr": "0xFFFFFF", "len": 4})
        )["code"],
        json!(-32004)
    );
}

#[test]
fn a_bare_address_is_refused_rather_than_guessed() {
    let h = spawn("bare");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let e = c.err("emulator/read_memory", json!({"addr": "1000"}));
    assert_eq!(e["code"], json!(-32602));
    assert!(e["message"].as_str().unwrap().contains("ambiguous"));
    // A JSON number where a hex string belongs is equally refused (D9).
    assert_eq!(
        c.err("emulator/read_memory", json!({"addr": 4096}))["code"],
        json!(-32602)
    );
}

#[test]
fn read_vram_is_bounded_to_the_region() {
    let h = spawn("vram");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let r = c.ok("emulator/read_vram", json!({"addr": "0x0000", "len": 16}));
    assert_eq!(r["len"], json!(16));
    assert_eq!(r["bytes"].as_str().unwrap().len(), 2 + 32);
    assert_eq!(
        c.err("emulator/read_vram", json!({"addr": "0xFFF8", "len": 32}))["code"],
        json!(-32004)
    );
}

#[test]
fn state_hash_is_deterministic_and_optionally_covers_the_framebuffer() {
    let h = spawn("hash");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let a = c.ok("emulator/state_hash", json!({}));
    let b = c.ok("emulator/state_hash", json!({}));
    assert_eq!(a["combined"], b["combined"], "no time passed, no change");
    assert!(a.get("framebuffer").is_none());
    let f = c.ok("emulator/state_hash", json!({"includeFramebuffer": true}));
    assert!(f["framebuffer"].as_str().unwrap().starts_with("0x"));
    assert_eq!(
        c.err("emulator/state_hash", json!({"includeFramebuffer": "yes"}))["code"],
        json!(-32602),
        "a string where a boolean belongs is refused (D9)"
    );
}

#[test]
fn screenshot_writes_a_readable_ppm() {
    let h = spawn("shot");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let p = std::env::temp_dir().join(format!("ae-ppm-{}.ppm", std::process::id()));
    let r = c.ok(
        "emulator/screenshot",
        json!({"path": p.display().to_string()}),
    );
    assert_eq!(r["format"], json!("ppm"));
    let bytes = std::fs::read(&p).expect("the PPM exists");
    assert!(bytes.starts_with(b"P6\n"));
    let width = r["width"].as_u64().unwrap() as usize;
    let height = r["height"].as_u64().unwrap() as usize;
    let header = format!("P6\n{width} {height}\n255\n");
    assert_eq!(bytes.len(), header.len() + width * height * 3);
    let _ = std::fs::remove_file(&p);
}

#[test]
fn input_has_set_semantics_and_refuses_six_button_buttons() {
    let h = spawn("input");
    let mut c = Client::connect(&h);
    c.handshake(false);

    let r = c.ok("emulator/hold", json!({"buttons": ["a", "start"]}));
    assert_eq!(r["held"], json!(["a", "start"]));
    // `down:false` clears exactly the named buttons and leaves the rest — set semantics, never additive
    // (the sibling's "hold ADDS, it does not replace" defect).
    let r = c.ok("emulator/hold", json!({"buttons": ["a"], "down": false}));
    assert_eq!(r["held"], json!(["start"]));

    // A tap must not leak past its frames, and must not clear a separately-held button.
    c.ok("emulator/press", json!({"buttons": ["c"], "frames": 1}));
    let r = c.ok("emulator/hold", json!({"buttons": [], "down": true}));
    assert_eq!(
        r["held"],
        json!(["start"]),
        "the tap was restored to the held set"
    );

    // CR-13 (ruled 2026-08-15): `release_all`'s result is `—`, so the reply is `{}` plus the envelope,
    // exactly like `emulator/restore`. The old `"released": true` was a hardcoded constant no branch
    // could falsify; it carried zero bits. Pinned as *lossless* rather than merely deleted: the key is
    // gone, the envelope still stamps the reply, and the answer a client actually needs — that the held
    // set is now empty — is observable, as the `hold` below shows.
    let r = c.ok("emulator/release_all", json!({}));
    assert!(
        r.get("released").is_none(),
        "`released` was a constant `true` and is removed (CR-13); got {r}"
    );
    for k in ["frame", "mclk", "running", "droppedEvents"] {
        assert!(
            r.get(k).is_some(),
            "the envelope must still stamp the reply — `{k}` missing from {r}"
        );
    }
    assert_eq!(
        r.as_object().unwrap().len(),
        4,
        "the result is the stamp and nothing else: {r}"
    );
    let r = c.ok("emulator/hold", json!({"buttons": [], "down": true}));
    assert_eq!(r["held"], json!([]));

    // A 6-button button is refused BY NAME rather than silently ignored — a silently-ignored button is
    // a test that "passes" while pressing nothing.
    let e = c.err("emulator/press", json!({"buttons": ["x"]}));
    assert_eq!(e["code"], json!(-32602));
    assert!(e["message"].as_str().unwrap().contains("6-button"));
    assert_eq!(e["data"]["button"], json!("x"));
    assert_eq!(
        c.err("emulator/press", json!({"buttons": ["l"]}))["code"],
        json!(-32602)
    );
}

#[test]
fn press_actually_advances_the_machine() {
    let h = spawn("press");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let before = c.ok("emulator/status", json!({}))["frame"]
        .as_u64()
        .unwrap();
    c.ok("emulator/press", json!({"buttons": ["start"], "frames": 4}));
    let after = c.ok("emulator/status", json!({}))["frame"]
        .as_u64()
        .unwrap();
    assert_eq!(after, before + 4);
}

// ------------------------------------------------------------------ symbols (D7, §4)

#[test]
fn no_symbols_and_symbol_not_found_are_distinct_codes() {
    let h = spawn("sym");
    let mut c = Client::connect(&h);
    c.handshake(false);

    // -32012: you forgot to load symbols.
    assert_eq!(
        c.err("emulator/lookup_symbol", json!({"name": "Player_1"}))["code"],
        json!(-32012)
    );
    assert_eq!(
        c.err("emulator/read_memory", json!({"symbol": "Player_1"}))["code"],
        json!(-32012)
    );

    let lst = write_lst("sym", LST_UNBOUND);
    let r = c.ok(
        "emulator/load_symbols",
        json!({"path": lst.display().to_string()}),
    );
    assert_eq!(r["symbolCount"], json!(5));
    assert_eq!(r["binding"], json!("indeterminate"));
    assert!(r["caveat"].as_str().unwrap().contains("unverified"));

    // -32013: that name does not exist. A different code, so the client can tell them apart.
    assert_eq!(
        c.err("emulator/lookup_symbol", json!({"name": "Nope_Nope"}))["code"],
        json!(-32013)
    );
}

#[test]
fn symbol_first_addressing_masks_the_32_bit_ram_spelling_to_the_24_bit_bus() {
    let h = spawn("mask");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let lst = write_lst("mask", LST_UNBOUND);
    c.ok(
        "emulator/load_symbols",
        json!({"path": lst.display().to_string()}),
    );

    // The listing spells it FFFF8CFA; the 68000 drives 24 lines, so the machine address is $FF8CFA.
    // Matching the raw spelling would resolve nothing, every time (recon §9a).
    let r = c.ok("emulator/lookup_symbol", json!({"name": "Player_1"}));
    assert_eq!(r["addr"], json!("0x00FF8CFA"));
    assert_eq!(r["rawAddr"], json!("0xFFFF8CFA"));

    // And a read by symbol lands in work RAM rather than out of range.
    let r = c.ok(
        "emulator/read_memory",
        json!({"symbol": "Player_1", "len": 4}),
    );
    assert_eq!(r["addr"], json!("0x00FF8CFA"));
    assert_eq!(r["region"], json!("work RAM"));
    assert_eq!(r["symbol"], json!("Player_1"));
}

#[test]
fn a_listing_that_does_not_bind_to_the_loaded_rom_is_refused() {
    let h = spawn("shape");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let lst = write_lst("shape", LST_WRONG_SHAPE);
    let e = c.err(
        "emulator/load_symbols",
        json!({"path": lst.display().to_string()}),
    );
    assert_eq!(e["code"], json!(-32602));
    assert_eq!(e["data"]["binding"], json!("mismatch"));
    // Refused means refused: nothing was loaded.
    assert_eq!(c.ok("emulator/status", json!({}))["symbolCount"], json!(0));
}

// ------------------------------------------------------------------ run-state discipline

#[test]
fn a_run_request_while_free_running_is_refused_rather_than_silently_changing_mode() {
    let h = spawn("mode2");
    let mut c = Client::connect(&h);
    c.handshake(false);
    c.ok("emulator/resume", json!({}));
    let e = c.err("emulator/run_frames", json!({"frames": 1}));
    // §5/§6: wrong *machine state*, not a bad envelope. The envelope is fine, so `-32600` would be a
    // lie to any client branching on the code.
    assert_eq!(e["code"], json!(-32005));
    assert_eq!(e["data"]["reason"], json!("machineRunning"));
    let paused = c.ok("emulator/pause", json!({}));
    assert_eq!(paused["wasRunning"], json!(true));
    assert_eq!(paused["running"], json!(false));
    c.ok("emulator/run_frames", json!({"frames": 1}));
}

/// Every `require_paused` call site, pinned on the wire — one row per gated method, not a matrix.
///
/// The helper's *logic* is tested once (above); what this pins is the *wiring*: that each entry point
/// actually calls it, and calls it before it parses params. Structural inheritance ("it is the first
/// statement in all four handlers") is a claim about today's source text and says nothing about a
/// refactor that hoists param parsing above the gate, a new run-shaped method that forgets the call, or
/// a cleanup that inlines one site and drops it. The litmus test is mutation-shaped: deleting any one
/// `require_paused(...)?` must fail a test. It does now; before this, it did for exactly one of four.
///
/// The "after pause" leg is what keeps the refusal leg honest — without it, a test whose calls all
/// errored for some unrelated reason would pass just as green.
#[test]
fn every_pause_gated_method_refuses_while_free_running_and_succeeds_once_paused() {
    let h = spawn("gated");
    let mut c = Client::connect(&h);
    c.handshake(false);

    // `reload_rom`'s minimal valid params need a real ROM on disk; anything less would weaken the
    // succeeds-once-paused leg into "fails for a different reason".
    let rom_path = std::env::temp_dir().join(format!("ae-gated-{}.bin", std::process::id()));
    std::fs::write(&rom_path, oracle_core::testrom::build()).unwrap();

    let gated = [
        ("emulator/run_frames", json!({"frames": 1})),
        (
            "emulator/run_to",
            json!({"addr": "0x000200", "maxFrames": 1}),
        ),
        ("emulator/press", json!({"buttons": ["a"], "frames": 1})),
        (
            "emulator/reload_rom",
            json!({"path": rom_path.display().to_string()}),
        ),
    ];

    c.ok("emulator/resume", json!({}));
    for (method, params) in &gated {
        let e = c.err(method, params.clone());
        assert_eq!(e["code"], json!(-32005), "{method} refusal code");
        assert_eq!(
            e["data"]["reason"],
            json!("machineRunning"),
            "{method} refusal reason"
        );
    }

    c.ok("emulator/pause", json!({}));
    for (method, params) in &gated {
        c.ok(method, params.clone());
    }

    let _ = std::fs::remove_file(&rom_path);
}

#[test]
fn reload_rom_resets_the_machine_and_reports_the_path() {
    let h = spawn("reload");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let rom = oracle_core::testrom::build();
    let p = std::env::temp_dir().join(format!("ae-reload-{}.bin", std::process::id()));
    std::fs::write(&p, &rom).unwrap();

    c.ok("emulator/run_frames", json!({"frames": 3}));
    let r = c.ok(
        "emulator/reload_rom",
        json!({"path": p.display().to_string()}),
    );
    assert_eq!(r["reloaded"], json!(true));
    assert_eq!(r["romBytes"], json!(rom.len()));
    assert_eq!(r["symbolsDropped"], json!(false));
    let _ = std::fs::remove_file(&p);
}
