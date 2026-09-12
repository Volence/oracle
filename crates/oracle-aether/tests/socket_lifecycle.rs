//! The AF_UNIX socket file's lifecycle — bind, restart-over-a-corpse, and unlink.
//!
//! Raised from a real session: a client's first connection hit a **stale** `oracle.sock` left by a
//! server that had died the day before, and got `ECONNREFUSED` from a dead file rather than `ENOENT`
//! from an absent one. That reads as "the server is broken" instead of "the server is not running".
//!
//! Two mechanisms stand between that and an unrecoverable path, and both were untested until now — which
//! is the more interesting half of the report, because the recovery mechanism working is what makes the
//! stale file a papercut rather than an outage:
//!
//! 1. **`Server::bind` probes before it binds.** It connects to the path; a live server answering means
//!    `AddrInUse` (two emulators must never fight over one bus), and nothing answering means the file is
//!    a corpse and is unlinked. So a stale socket never blocks a restart.
//! 2. **`ServerHandle::drop` unlinks.** Any handle that goes out of scope cleans up after itself (its
//!    own file only, since lens M13: see `src/server.rs`'s `a_dead_emulator_thread_releases_the_socket_and_reports_why`,
//!    which also pins what happens when the emulator thread dies).
//!
//! What is **not** covered, deliberately and with the reasoning at the call site
//! (`src/main.rs`'s park on `ServerHandle::wait`): the standalone binary parks until its emulator thread
//! ends, so `SIGINT`/`SIGTERM` kills it without unwinding and the file survives. Catching those needs `signal-hook` or `unsafe` `libc`, and
//! this crate's runtime dependency set is documented as not growing while the library is
//! `forbid(unsafe_code)`. The limitation is recorded rather than papered over.

mod common;

use common::{spawn_system, Client};
use oracle_aether::host::{Host, HostConfig};
use oracle_aether::server::{Server, ServerConfig};
use oracle_core::system::System;
use serde_json::json;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn machine() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    sys
}

fn socket_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "oracle-lifecycle-{tag}-{}.sock",
        std::process::id()
    ))
}

/// A handle that goes out of scope takes its socket file with it. This is the mechanism the standalone
/// binary *has* and never reaches; everything else that embeds the server does reach it.
#[test]
fn dropping_the_handle_unlinks_the_socket() {
    let path = socket_path("drop");
    let _ = std::fs::remove_file(&path);
    {
        let config = ServerConfig {
            socket_path: path.clone(),
            ..ServerConfig::default()
        };
        let h = Server::bind(config)
            .expect("bind")
            .spawn(oracle_aether::server::Machine::new(machine()));
        assert!(path.exists(), "the socket exists while the server is live");
        assert!(UnixStream::connect(&path).is_ok(), "…and answers");
        drop(h);
    }
    assert!(
        !path.exists(),
        "the socket file must not outlive the handle that owns it"
    );
}

/// **The reported case, and its recovery.** A stale file with no listener behind it must not block a
/// restart — the new server unlinks the corpse and binds over it.
///
/// The two assertions before the restart are the anti-vacuity control: without them a test that simply
/// bound to a fresh path would pass, proving nothing about staleness. So the corpse is first shown to be
/// present *and* to refuse connections in exactly the way the client reported.
#[test]
fn a_stale_socket_file_does_not_block_a_restart() {
    let path = socket_path("stale");
    let _ = std::fs::remove_file(&path);

    // Fabricate the corpse: a real socket file with no process behind it. Binding and dropping the
    // listener without unlinking is precisely what a killed server leaves.
    {
        let l = std::os::unix::net::UnixListener::bind(&path).expect("bind a bare listener");
        drop(l);
    }
    // On Linux, dropping a UnixListener does not unlink; if that ever changes this test would silently
    // become the vacuous one it exists not to be, so the state is asserted rather than assumed.
    assert!(
        path.exists(),
        "the corpse must exist for this test to mean anything"
    );
    assert!(
        UnixStream::connect(&path).is_err(),
        "the corpse must refuse connections — that is the client-visible symptom"
    );

    let config = ServerConfig {
        socket_path: path.clone(),
        ..ServerConfig::default()
    };
    let h = Server::bind(config)
        .expect("a stale socket must not block a restart")
        .spawn(oracle_aether::server::Machine::new(machine()));

    let mut c = Client::connect(&h);
    c.handshake(false);
    let s = c.ok("emulator/status", json!({}));
    assert!(s["running"].is_boolean(), "the new server serves normally");
    drop(h);
    assert!(!path.exists());
}

/// The other direction, and the reason the probe cannot simply unlink unconditionally: a **live** server
/// on the path must refuse the second bind, or two emulators would share one bus.
#[test]
fn a_live_server_refuses_a_second_bind_on_the_same_path() {
    let path = socket_path("live");
    let _ = std::fs::remove_file(&path);

    let first = Server::bind(ServerConfig {
        socket_path: path.clone(),
        ..ServerConfig::default()
    })
    .expect("first bind")
    .spawn(oracle_aether::server::Machine::new(machine()));

    let second = Server::bind(ServerConfig {
        socket_path: path.clone(),
        ..ServerConfig::default()
    });
    match second {
        Ok(_) => panic!("a second server bound over a live one — two emulators, one bus"),
        Err(e) => {
            assert_eq!(e.kind(), std::io::ErrorKind::AddrInUse);
            assert!(
                e.to_string().contains("already live"),
                "the refusal must say why: {e}"
            );
        }
    }

    // The refused bind must not have disturbed the live server — including its socket file.
    assert!(path.exists());
    let mut c = Client::connect(&first);
    c.handshake(false);
    c.ok("emulator/status", json!({}));
    drop(first);
}

/// **The unlink is bounded to actual sockets.** `--socket` takes a path from a human or a config file,
/// and the corpse-removal above is a `remove_file` on whatever is there. A typo naming a real file — a
/// ROM, a listing, a dotfile — must be refused, not deleted: refusing is recoverable and deleting is not,
/// and a server that had not even started serving would have done it.
#[test]
fn bind_refuses_to_delete_a_path_that_is_not_a_socket() {
    let path = socket_path("notasocket");
    let _ = std::fs::remove_file(&path);
    std::fs::write(&path, b"this is a precious file, not a socket").unwrap();

    let e = match Server::bind(ServerConfig {
        socket_path: path.clone(),
        ..ServerConfig::default()
    }) {
        Ok(_) => panic!("binding over a regular file must fail"),
        Err(e) => e,
    };
    assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
    assert!(
        e.to_string().contains("not a socket"),
        "the refusal must say why: {e}"
    );

    // The point of the test: the file is still there.
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"this is a precious file, not a socket",
        "bind deleted a file it did not create"
    );
    let _ = std::fs::remove_file(&path);
}

/// The ordinary spawn helper cleans up too, so the suite does not litter `$TMPDIR` with sockets — and a
/// leak here would eventually make `a_stale_socket_file_does_not_block_a_restart` pass for the wrong
/// reason on a shared path.
#[test]
fn the_test_harness_cleans_up_after_itself() {
    let p = {
        let h = spawn_system("lifecycle-tidy", machine(), 8);
        let p = h.socket_path().to_path_buf();
        assert!(p.exists());
        p
    };
    assert!(!p.exists(), "the harness left a socket behind");
}

// ---------------------------------------------------------------- the hosted arrangement

/// **A `Host` removes only the socket file it bound** (HOST-SHUTDOWN-UNLINK, the hosted half of lens M13).
///
/// The player window and `oracle-frontend` both run a [`Host`], and on the owner's machine several
/// processes share socket paths. So the replacement below is what a restart or a second window does once
/// the path is free: the host's file is removed and another socket is bound at the same path. The host's
/// listener is still open at that moment and holds its inode, so the replacement's file has a different
/// `(device, inode)` on every filesystem, tmpfs or ext4, and the row needs no inode reuse to bite.
///
/// The host's shutdown must then leave the replacement's socket where it is, still answering. A shutdown
/// that unlinks its configured path whatever is there deletes it.
#[test]
fn a_host_never_unlinks_a_socket_it_did_not_bind() {
    let path = socket_path("host-foreign");
    let _ = std::fs::remove_file(&path);
    let mut host = Host::new(HostConfig::default());
    let served = host.serve(Some(path.clone())).expect("the host binds");
    assert_eq!(served, path, "the host bound the path it was given");
    assert!(
        UnixStream::connect(&path).is_ok(),
        "the host answers on its own socket before anything replaces it"
    );

    std::fs::remove_file(&path)
        .expect("take the host's socket file away, as a restart would find it");
    let replacement =
        UnixListener::bind(&path).expect("bind a replacement socket at the host's path");
    host.shutdown();

    let file_survived = path.exists();
    let still_answers = UnixStream::connect(&path).is_ok();
    drop(replacement);
    let _ = std::fs::remove_file(&path);
    assert!(
        file_survived && still_answers,
        "Host::shutdown removed {}, a socket another process bound there after the host's own file was \
         gone (file present after shutdown: {file_survived}, connect after shutdown: {still_answers}). \
         A hosted bus may remove only the file it bound itself.",
        path.display()
    );
}

/// **And it does remove its own**, on both ways a `Host` stops serving: an explicit [`Host::shutdown`]
/// and a drop. The positive half of the row above, so the fix cannot pass it by never unlinking at all.
#[test]
fn a_host_removes_its_own_socket_on_shutdown_and_on_drop() {
    let path = socket_path("host-own-shutdown");
    let _ = std::fs::remove_file(&path);
    let mut host = Host::new(HostConfig::default());
    host.serve(Some(path.clone())).expect("the host binds");
    assert!(
        path.exists() && UnixStream::connect(&path).is_ok(),
        "the socket exists and answers while the host serves"
    );
    host.shutdown();
    assert!(
        !path.exists(),
        "Host::shutdown left its own socket file behind at {}",
        path.display()
    );
    assert!(!host.is_serving() && host.socket_path().is_none());

    let path = socket_path("host-own-drop");
    let _ = std::fs::remove_file(&path);
    {
        let mut host = Host::new(HostConfig::default());
        host.serve(Some(path.clone())).expect("the host binds");
        assert!(path.exists(), "the socket exists while the host serves");
    }
    assert!(
        !path.exists(),
        "a dropped Host left its own socket file behind at {}",
        path.display()
    );
}

/// **A `Host` that was shut down serves again when asked.** [`Host::serve`] refuses only while a host is
/// already serving, so a serve after [`Host::shutdown`] is a request the API accepts, and it must produce a
/// bus a client can use, not an `Ok` naming a socket that answers nothing.
///
/// Measured with a real handshake and a real call, pumped by this thread the way a player's loop pumps,
/// because a bare `connect` could win the race against an accept loop that has already given up.
#[test]
fn a_host_that_was_shut_down_serves_again() {
    let path = socket_path("host-reserve");
    let _ = std::fs::remove_file(&path);
    let mut sys = machine();
    let mut host = Host::new(HostConfig::default());
    host.serve(Some(path.clone()))
        .expect("the first serve binds");
    host.shutdown();
    assert!(!path.exists(), "the first serve's socket is gone");

    let again = host
        .serve(Some(path.clone()))
        .expect("a host that was shut down binds again");
    assert_eq!(again, path);

    let client_path = path.clone();
    let client = std::thread::spawn(move || {
        let mut c = Client::connect_path(&client_path);
        c.handshake(false);
        c.ok("emulator/status", json!({}))
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    while !client.is_finished() && Instant::now() < deadline {
        host.pump(&mut sys);
        std::thread::sleep(Duration::from_millis(1));
    }
    let finished = client.is_finished();
    let status = if finished { client.join().ok() } else { None };
    host.shutdown();
    assert!(
        status.is_some_and(|s| s["running"].is_boolean()),
        "a host serving again after a shutdown did not answer a client on {} (client finished: {finished}). \
         Host::serve returned Ok for a bus nobody can reach.",
        path.display()
    );
    assert!(
        !path.exists(),
        "and the second serve's socket is removed too"
    );
}
