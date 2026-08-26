//! **`status.romPath` is absolute** — contract `protocol.md` §6, and §12.2 item 9 of the CR-C ruling.
//!
//! §6 asks `emulator/status` for *the absolute path of the loaded image*. This server echoed the launch
//! argument verbatim, so `oracle-aether ./s4.bin` put `./s4.bin` on the wire and every consumer resolved
//! it against a working directory that was not this process's. The fix is at the boundary
//! (`Engine::set_rom_path`), so the binary, a hosted embedder, `emulator/reload_rom` and a checkpoint
//! restore all agree; this file pins both halves of it — the path that resolves, and the label that does
//! not.

mod common;

use common::{temp_socket, Client};
use oracle_aether::engine::EngineConfig;
use oracle_aether::server::{Machine, Server, ServerConfig, ServerHandle};
use oracle_core::system::System;
use serde_json::json;
use std::path::{Path, PathBuf};

fn serve_with_rom_path(tag: &str, rom_path: Option<String>) -> ServerHandle {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    let mut machine = Machine::new(sys);
    machine.rom_path = rom_path;
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

/// A relative spelling of `target`, built by walking up out of the current directory.
///
/// Deliberately **not** `std::env::set_current_dir`: the cwd is process-global and every test in this
/// binary shares it, so a test that moves it is a test that can break a sibling running beside it. This
/// produces a genuinely relative path — the thing under test — without touching any shared state.
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

#[test]
fn status_reports_an_absolute_rom_path_even_when_launched_with_a_relative_one() {
    let rom = std::env::temp_dir().join(format!("ae-rompath-{}.bin", std::process::id()));
    std::fs::write(&rom, oracle_core::testrom::build()).expect("write the rom");
    let canonical = std::fs::canonicalize(&rom).expect("canonicalize the rom we just wrote");

    let rel = relative_to_cwd(&canonical);
    // The premise of the test, asserted rather than assumed — if this ever produced an absolute string
    // the test below would pass while proving nothing.
    assert!(
        Path::new(&rel).is_relative(),
        "the launch argument under test must be relative; got {rel}"
    );
    assert_eq!(
        std::fs::canonicalize(&rel).expect("the relative spelling resolves"),
        canonical,
        "the relative spelling must name the same file, or the server is being asked about a different rom"
    );

    let h = serve_with_rom_path("rompath", Some(rel.clone()));
    let mut c = Client::connect(&h);
    c.handshake(true);
    let s = c.call("emulator/status", json!({}));
    let reported = s["result"]["romPath"]
        .as_str()
        .expect("status carries romPath");

    assert!(
        Path::new(reported).is_absolute(),
        "§6: `romPath` is the ABSOLUTE path of the loaded image. The server was launched with {rel} and \
         reported {reported} — a string whose meaning depends on a working directory the client does not \
         have, and which `emulator/reload_rom` would then resolve against ours rather than theirs."
    );
    assert_eq!(
        Path::new(reported),
        canonical,
        "the absolute path must name the image that was actually loaded"
    );

    let _ = std::fs::remove_file(&rom);
}

#[test]
fn a_rom_label_that_names_no_file_is_passed_through_untouched() {
    // The other half of the rule, and the reason the fix is `canonicalize`-or-nothing rather than
    // "join the cwd onto anything relative". `host::MachineInfo::rom_path` is whatever the embedder uses
    // to name its image — it is not required to be a filesystem path at all. Prefixing this process's
    // working directory onto such a label would manufacture a path that resolves to nothing and looks
    // authoritative, which is a worse answer than the label. §6 is a SHOULD so that "I cannot honestly
    // say" stays available.
    let label = "not-a-file-just-a-label";
    assert!(
        std::fs::canonicalize(label).is_err(),
        "the premise: this string must not accidentally name a real file in the test's cwd"
    );
    let h = serve_with_rom_path("romlabel", Some(label.into()));
    let mut c = Client::connect(&h);
    c.handshake(true);
    let s = c.call("emulator/status", json!({}));
    assert_eq!(
        s["result"]["romPath"],
        json!(label),
        "a label that names no file is not a path this process can speak for, so it is reported as given"
    );
}
