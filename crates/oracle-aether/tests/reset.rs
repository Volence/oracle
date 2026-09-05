//! `emulator/reset` — `protocol.md` §6 (run-control), result defined by CR-22 / §11.13
//! (`docs/2026-08-18-cr21-23-tier1-rows.md`, `docs/2026-08-18-ruling-cr21-23.md`).

mod common;

use common::{spawn_system, Client};
use oracle_core::system::System;
use serde_json::json;

/// A work-RAM address to arm a watch on — the same one `tests/watchpoints.rs` uses, for the same
/// reason: it is inside the machine's own RAM, so the watch is a plausible one rather than a probe
/// aimed at nowhere.
const SENTINEL: &str = "0x00FF8000";

fn machine() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    sys
}

fn client(h: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(h);
    c.handshake(false);
    c
}

/// The "which machine is this?" probe used throughout this file: 64 bytes off the top of work RAM,
/// which the fixture ROM stirs every frame. See [`reset_restores_the_power_on_anchor`] for why this
/// and not `state_hash`.
fn ram(c: &mut Client) -> serde_json::Value {
    c.ok(
        "emulator/read_memory",
        json!({"addr": "0xFF0000", "len": 64}),
    )["bytes"]
        .clone()
}

/// The reset restores the deterministic power-on anchor: the machine after run-frames + reset is
/// byte-for-byte the machine captured at first boot — two independently derived values.
///
/// Work RAM is the instrument, not `state_hash`: the five VDP fingerprints are the only thing
/// `state_hash` covers, and this fixture ROM stirs work RAM every frame without touching the VDP —
/// so a `state_hash` sanity check passes while the machine sits still and would assert nothing.
/// `tests/checkpoints.rs` reads `$FF0000` for exactly this reason; `state_hash` rides along as the
/// VDP half of the same claim.
#[test]
fn reset_restores_the_power_on_anchor() {
    let h = spawn_system("rs-anchor", machine(), 64);
    let mut c = client(&h);
    let boot_ram = ram(&mut c);
    let boot_hash = c.ok("emulator/state_hash", json!({}))["combined"].clone();
    c.ok("emulator/run_frames", json!({"frames": 5}));
    assert_ne!(
        boot_ram,
        ram(&mut c),
        "sanity: five frames actually changed the machine"
    );
    let r = c.ok("emulator/reset", json!({}));
    assert_eq!(
        r["deferred"],
        json!(false),
        "this server applies before the reply"
    );
    assert_eq!(boot_ram, ram(&mut c), "back at the anchor");
    assert_eq!(
        boot_hash,
        c.ok("emulator/state_hash", json!({}))["combined"],
        "and the VDP came back with it"
    );
}

/// The master clock restarts: stamps after the reset report frame 0 again.
#[test]
fn the_clock_restarts_from_zero() {
    let h = spawn_system("rs-clock", machine(), 64);
    let mut c = client(&h);
    c.ok("emulator/run_frames", json!({"frames": 3}));
    c.ok("emulator/reset", json!({}));
    let s = c.ok("emulator/status", json!({}));
    assert_eq!(
        s["frame"],
        json!(0),
        "stamps jump backwards by design — that is the reset"
    );
    assert_eq!(s["mclk"], json!(0), "the master clock restarts from 0");
}

/// NOT run-state gated, and run state is PRESERVED both ways (contract: reset MUST NOT change it).
#[test]
fn run_state_is_preserved_both_ways() {
    // Paused machine: reset leaves it paused.
    let h = spawn_system("rs-paused", machine(), 64);
    let mut c = client(&h);
    let r = c.ok("emulator/reset", json!({}));
    assert_eq!(r["deferred"], json!(false));
    assert_eq!(
        r["running"],
        json!(false),
        "paused stays paused at the reset vector"
    );

    // Free-running machine: reset answers (MUST NOT refuse -32005) and it keeps running.
    let h2 = spawn_system("rs-freerun", machine(), 64);
    let mut c2 = client(&h2);
    c2.ok("emulator/resume", json!({}));
    let r2 = c2.ok("emulator/reset", json!({}));
    assert_eq!(r2["deferred"], json!(false));
    assert_eq!(
        r2["running"],
        json!(true),
        "free-running keeps running from the vector"
    );
}

/// Watchpoints and checkpoints survive a reset (contract: they survive).
///
/// The checkpoint is taken **four frames in, not at the anchor**, and that is the whole design of this
/// test. A checkpoint captured at the anchor describes the same machine the reset produces, so the
/// closing `restore` would move nothing and a restore that silently did nothing would pass — the
/// surviving-handle assertions would carry the test on their own. Capturing a machine the reset
/// destroys makes the restore observable: the RAM probe has to come *back* to a value the reset had
/// already thrown away.
#[test]
fn watch_and_checkpoint_surfaces_survive() {
    let h = spawn_system("rs-survive", machine(), 64);
    let mut c = client(&h);
    let w = c.ok(
        "emulator/watchpoint_add",
        json!({"addr": SENTINEL, "write": true}),
    );
    let handle = w["watch"].clone();

    // Move off the anchor first, so the captured machine and the post-reset machine differ.
    let anchor_ram = ram(&mut c);
    c.ok("emulator/run_frames", json!({"frames": 4}));
    let captured_ram = ram(&mut c);
    assert_ne!(
        anchor_ram, captured_ram,
        "sanity: the checkpoint is being taken somewhere the reset cannot land"
    );
    let cp = c.ok("emulator/checkpoint", json!({}));
    let cp_id = cp["id"]
        .as_str()
        .expect("a checkpoint id is a string")
        .to_string();

    c.ok("emulator/reset", json!({}));
    assert_eq!(anchor_ram, ram(&mut c), "the reset really did land");

    let list = c.ok("emulator/watchpoint_list", json!({}));
    assert!(
        list["watches"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["watch"] == handle),
        "the armed watch survives"
    );
    let cps = c.ok("emulator/checkpoint_list", json!({}));
    assert!(
        cps["checkpoints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["id"] == json!(cp_id)),
        "the checkpoint survives the reset"
    );

    // …and it is still a *usable* checkpoint, not merely a surviving row: restoring it brings back the
    // machine the reset discarded.
    let r = c.ok("emulator/restore", json!({ "id": &cp_id }));
    assert_eq!(r["frame"], json!(4), "restored to the capture coordinate");
    assert_eq!(
        captured_ram,
        ram(&mut c),
        "the pre-reset machine came back byte-for-byte"
    );
}

/// §8 item 20 closure, asserted locally: the key set is exact.
#[test]
fn the_key_set_is_exact() {
    use std::collections::BTreeSet;
    let h = spawn_system("rs-keys", machine(), 64);
    let mut c = client(&h);
    let r = c.ok("emulator/reset", json!({}));
    let got: BTreeSet<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
    // `hitsDropped` joined the set on 2026-09-05 (§11.38, CR-O): required, and present even when 0.
    let want: BTreeSet<&str> = [
        "deferred",
        "hitsDropped",
        "frame",
        "mclk",
        "running",
        "droppedEvents",
    ]
    .into_iter()
    .collect();
    assert_eq!(got, want);
}
