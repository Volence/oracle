//! **The standalone binary's startup banner must not contradict its own handshake.**
//!
//! `src/main.rs` prints `aether: N methods advertised` before it parks. Until 2026-09-09 that `N` was
//! `engine::METHODS.len()` — the raw dispatch table — while the socket's `initialize` answered
//! `advertised_methods(presents_frames)`, which since §11.42 (CR-S) filters `PRESENTING_ONLY` off a
//! headless deployment. `presents_frames` defaults to `false` and `main.rs` sets nothing, so the two
//! numbers were 62 and 61: one process, one commit, two answers to *how many methods do you serve*, and
//! the louder of them printed on every start.
//!
//! Why a row rather than a comment: empyrean **§11.46** rules that `methods` names what *this process*
//! serves, not what the build implements, and that **no count derived from it may be used as a
//! build-identity or freshness signal**. A consumer in another repo was about to use this banner as a
//! stale-binary detector; it would have accused a current, correct server of being stale, off by exactly
//! the one filtered row. The contract's remedy is `initialize.serverBuild`, and the banner's obligation
//! is the narrower one asserted here — *agree with the socket beside you*.
//!
//! # Why this spawns the real binary
//!
//! The defect lived in `main.rs`, which is a `[[bin]]`: no integration test can call into it, and a unit
//! test of a helper would have asserted the helper, not the string the process prints. So this row runs
//! `CARGO_BIN_EXE_oracle-aether`, reads the line off its stdout, then **connects to the socket that same
//! process just bound** and counts the `methods` array it serves. Nothing is transcribed — not 61, not
//! 62 — and the two sides of the comparison are produced by the same running process. Spawning the
//! server binary is the ordinary arrangement here (`tests/socket_lifecycle.rs` drives the same server
//! type); no emulator MCP surface is touched.
//!
//! # Red-first evidence, 2026-09-09
//!
//! | mutation applied on disk | result |
//! |---|---|
//! | baseline (the shipped `METHODS.len()` banner, before the fix) | **RED** — banner 62, socket serves 61 |
//! | `advertised_methods(true)` in the banner (wrong deployment's answer) | **RED** — banner 62, socket serves 61 |
//! | `PRESENTING_ONLY` emptied | **RED** on `the_banner_would_be_wrong_if_it_counted_the_raw_table`'s vacuity guard |
//!
//! Each was applied to the working tree, shown with `git diff`, run, and restored from the committed
//! baseline. The middle one matters most: it is the *same* wrong number reached by a different route, so
//! a fix that merely stopped saying `METHODS.len()` without asking this deployment's question would still
//! be caught.

mod common;

use common::Client;
use oracle_aether::engine::{advertised_methods, METHODS, PRESENTING_ONLY};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// How long the banner may take to appear. Generous: this is a hang budget, not a performance bar.
const BANNER_TIMEOUT: Duration = Duration::from_secs(30);

/// A temp path unique per process and per tag, so a parallel run cannot collide.
fn temp_path(tag: &str, ext: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "oracle-banner-{tag}-{}-{}.{ext}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

/// Kills the child and removes the ROM on the way out, whichever assertion fired.
struct Reaper {
    child: std::process::Child,
    rom: std::path::PathBuf,
    socket: std::path::PathBuf,
}

impl Drop for Reaper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.rom);
        let _ = std::fs::remove_file(&self.socket);
    }
}

/// **The number in the banner is the number the socket serves.**
///
/// Both sides come from one running process: the left off its stdout, the right off its `initialize`.
#[test]
fn the_startup_banner_counts_what_this_process_serves() {
    let rom = temp_path("rom", "bin");
    let socket = temp_path("sock", "sock");
    std::fs::write(&rom, oracle_core::testrom::build()).expect("write the temp ROM");

    let mut child = Command::new(env!("CARGO_BIN_EXE_oracle-aether"))
        .arg(&rom)
        .arg("--socket")
        .arg(&socket)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the oracle-aether binary");

    let stdout = child.stdout.take().expect("piped stdout");
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                return;
            }
        }
    });

    let mut reaper = Reaper {
        child,
        rom: rom.clone(),
        socket: socket.clone(),
    };

    // Read until the banner line or the budget. Other startup lines (`rom: …`, `aether: listening …`,
    // any symbol note) are skipped rather than positionally indexed, so a new line above this one is not
    // a false red.
    let deadline = std::time::Instant::now() + BANNER_TIMEOUT;
    let mut seen: Vec<String> = Vec::new();
    let banner = loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(
            !left.is_zero(),
            "no `aether: N methods advertised` line within {BANNER_TIMEOUT:?}; stdout so far: {seen:#?}"
        );
        match rx.recv_timeout(left) {
            Ok(line) => {
                if line.contains("methods advertised") {
                    break line;
                }
                seen.push(line);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                panic!("timed out reading stdout; lines so far: {seen:#?}")
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => panic!(
                "the binary exited before printing its banner; stdout was: {seen:#?}. A non-zero exit \
                 here is a startup failure, not a banner defect — read stderr by hand."
            ),
        }
    };

    // Parse the number out of the line the process actually printed. Derived from the string, never
    // retyped: the assertion below must fail if the count is wrong, not if the wording changed.
    let printed: usize = banner
        .split_whitespace()
        .find_map(|w| w.parse::<usize>().ok())
        .unwrap_or_else(|| panic!("no count in the banner line: {banner:?}"));

    // The other half of the comparison, from the socket this same process bound.
    let mut client = Client::connect_path(&socket);
    let init = client.handshake(false);
    let served = init["methods"]
        .as_array()
        .unwrap_or_else(|| panic!("initialize.methods is an array; got {}", init["methods"]))
        .len();

    // ⚑ Positive control, asserted BEFORE the equality. An extraction that found nothing would report
    // `served == 0`, and if the banner ever also printed 0 the equality would certify a pair of failures
    // as agreement. A positive control validates the SEARCH, not the question.
    let names: Vec<&str> = init["methods"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        names.contains(&"emulator/status"),
        "control: the served list must contain a row this repo is certain of; got {names:?}"
    );

    assert_eq!(
        printed, served,
        "the banner says {printed} methods are advertised and the socket beside it advertises {served}. \
         The word in that line is `advertised`; it must not count `METHODS.len()` (which is {}), because \
         since §11.42 the dispatch table is not the same set in every process. See §11.46: a consumer \
         reading this number as a freshness signal is reading the wrong number, and it must at least not \
         be a WRONG wrong number.",
        METHODS.len()
    );

    drop(client);
    let _ = &mut reaper; // explicit: the reaper's Drop is the cleanup, and it runs on panic too.
}

/// **The two answers really are different numbers today**, so the row above is not a tautology.
///
/// This is the anti-vacuity half. If `PRESENTING_ONLY` no longer names any `METHODS` row then the banner
/// and the raw table agree by construction, the row above would pass under the very bug it exists to
/// catch, and this asserts loudly rather than going quiet.
#[test]
fn the_banner_would_be_wrong_if_it_counted_the_raw_table() {
    let filtered = METHODS
        .iter()
        .filter(|m| PRESENTING_ONLY.contains(&m.name))
        .count();
    assert!(
        filtered > 0,
        "VACUOUS: `PRESENTING_ONLY` ({PRESENTING_ONLY:?}) names no row in `METHODS`, so a headless \
         deployment's advertised count equals `METHODS.len()` and `the_startup_banner_counts_what_this_\
         process_serves` can no longer tell a correct banner from `METHODS.len()`. Re-aim both rows or \
         delete them — do not silence this."
    );
    assert_eq!(
        advertised_methods(false).count(),
        METHODS.len() - filtered,
        "a headless deployment advertises the table minus exactly its presenting-only rows"
    );
    assert!(
        advertised_methods(false).count() < METHODS.len(),
        "the defect this parcel closed was printing the larger of two different numbers"
    );
}
