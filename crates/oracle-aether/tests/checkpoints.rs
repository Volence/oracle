//! The four checkpoint methods (`protocol.md` §6.1, decision D13).
//!
//! Every test here is a wire-level round trip, because the thing being pinned is the *contract*: the
//! result shapes, the `-32005` refusals, the cursored list envelope, and the three normative D13 rules
//! (volatile, whole-machine including the ROM, capped-and-refused-loudly).

mod common;

use common::{spawn, Client};
use oracle_aether::engine::METHODS;
use serde_json::{json, Value};

/// The advertised cap, read from the handshake rather than hardcoded — the contract's whole point in
/// making it discoverable is that a client never has to guess it.
fn cap(init: &Value) -> u64 {
    init["capabilities"]["checkpoints"]["cap"]
        .as_u64()
        .expect("capabilities.checkpoints.cap must be advertised (D13 rule 3)")
}

fn take(c: &mut Client, label: Option<&str>) -> u64 {
    let params = match label {
        Some(l) => json!({ "label": l }),
        None => json!({}),
    };
    c.ok("emulator/checkpoint", params)["id"]
        .as_u64()
        .expect("`id` must be a server-assigned number")
}

// ------------------------------------------------------------------ the catalog surface

#[test]
fn the_four_checkpoint_methods_are_advertised_and_no_save_to_file_variant_exists() {
    let h = spawn("cpcat");
    let mut c = Client::connect(&h);
    let init = c.handshake(false);

    let methods: Vec<&str> = init["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap())
        .collect();
    for m in [
        "emulator/checkpoint",
        "emulator/restore",
        "emulator/checkpoint_list",
        "emulator/checkpoint_drop",
    ] {
        assert!(methods.contains(&m), "{m} must be advertised: {methods:?}");
    }

    // D13 rule 1, structurally: there is no method that writes a checkpoint to disk, and no `path`-ish
    // sibling. The whole method table is the advertised set, so this is an exhaustive check.
    for spec in METHODS {
        let n = spec.name;
        assert!(
            !(n.contains("save_state")
                || n.contains("save_checkpoint")
                || n.contains("load_state")),
            "D13 rule 1 forbids a persist-to-disk variant, found {n}"
        );
    }

    assert_eq!(
        init["capabilities"]["checkpoints"]["supported"],
        json!(true)
    );
    assert!(cap(&init) >= 1, "the cap must be a positive integer");
}

// ------------------------------------------------------------------ capture -> restore

#[test]
fn a_checkpoint_restores_the_machine_to_exactly_the_same_coordinate() {
    let h = spawn("cprt");
    let mut c = Client::connect(&h);
    c.handshake(false);

    c.ok("emulator/run_frames", json!({"frames": 3}));
    let cp = c.ok("emulator/checkpoint", json!({"label": "three frames in"}));
    assert!(cp["id"].is_number());
    assert!(
        cp["bytes"].as_u64().unwrap() > 0,
        "`bytes` is the snapshot size"
    );
    assert_eq!(cp["frame"], json!(3), "the stamp IS the capture coordinate");
    let id = cp["id"].as_u64().unwrap();
    let (frame_at, mclk_at) = (cp["frame"].clone(), cp["mclk"].clone());
    let hash_at = c.ok("emulator/state_hash", json!({}))["combined"].clone();
    let ram_at = c.ok(
        "emulator/read_memory",
        json!({"addr": "0xFF0000", "len": 64}),
    )["bytes"]
        .clone();

    // Diverge — the fixture ROM stirs work RAM every frame, so this really is a different machine.
    let after = c.ok("emulator/run_frames", json!({"frames": 5}));
    assert_eq!(after["frame"], json!(8));
    let hash_after = c.ok("emulator/state_hash", json!({}))["combined"].clone();
    let ram_after = c.ok(
        "emulator/read_memory",
        json!({"addr": "0xFF0000", "len": 64}),
    )["bytes"]
        .clone();
    assert_ne!(ram_at, ram_after, "the machine must actually have moved on");

    // §6.1: the whole of `restore`'s result is the machine stamp, reporting the *restored* coordinate.
    let r = c.ok("emulator/restore", json!({"id": id}));
    assert_eq!(r["frame"], frame_at);
    assert_eq!(r["mclk"], mclk_at);
    assert_eq!(
        c.ok("emulator/state_hash", json!({}))["combined"],
        hash_at,
        "VDP state must come back"
    );
    assert_eq!(
        c.ok(
            "emulator/read_memory",
            json!({"addr": "0xFF0000", "len": 64})
        )["bytes"],
        ram_at,
        "work RAM must come back"
    );

    // And the restore is deterministic: replaying the same five frames reproduces the same machine.
    let again = c.ok("emulator/run_frames", json!({"frames": 5}));
    assert_eq!(again["frame"], json!(8));
    assert_eq!(
        c.ok("emulator/state_hash", json!({}))["combined"],
        hash_after
    );
    assert_eq!(
        c.ok(
            "emulator/read_memory",
            json!({"addr": "0xFF0000", "len": 64})
        )["bytes"],
        ram_after
    );
}

#[test]
fn a_checkpoint_can_be_restored_more_than_once_and_survives_a_restore() {
    let h = spawn("cptwice");
    let mut c = Client::connect(&h);
    c.handshake(false);
    c.ok("emulator/run_frames", json!({"frames": 2}));
    let id = take(&mut c, None);

    for _ in 0..3 {
        c.ok("emulator/run_frames", json!({"frames": 4}));
        let r = c.ok("emulator/restore", json!({"id": id}));
        assert_eq!(r["frame"], json!(2));
    }
    // Restoring must not consume the slot.
    let list = c.ok("emulator/checkpoint_list", json!({}));
    assert_eq!(list["checkpoints"].as_array().unwrap().len(), 1);
}

// ------------------------------------------------------------------ D13 rule 2: the ROM comes back

#[test]
fn restore_brings_the_previous_cartridge_back_it_never_partially_restores() {
    let h = spawn("cprom");
    let mut c = Client::connect(&h);
    c.handshake(false);

    // A second, byte-different image (a spare zero byte near the end of the fixture ROM).
    let mut other = oracle_core::testrom::build();
    other[0x2F0] = 0xA5;
    let p = std::env::temp_dir().join(format!("ae-cprom-{}.bin", std::process::id()));
    std::fs::write(&p, &other).unwrap();

    let orig = c.ok(
        "emulator/read_memory",
        json!({"addr": "0x0002F0", "len": 1}),
    );
    assert_eq!(orig["bytes"], json!("0x00"));
    let id = take(&mut c, Some("before the reload"));

    c.ok(
        "emulator/reload_rom",
        json!({"path": p.display().to_string()}),
    );
    assert_eq!(
        c.ok(
            "emulator/read_memory",
            json!({"addr": "0x0002F0", "len": 1})
        )["bytes"],
        json!("0xA5"),
        "the new cartridge is in"
    );

    // D13 rule 2: this is defined behaviour, not a refusal.
    c.ok("emulator/restore", json!({"id": id}));
    assert_eq!(
        c.ok(
            "emulator/read_memory",
            json!({"addr": "0x0002F0", "len": 1})
        )["bytes"],
        json!("0x00"),
        "restoring a pre-reload checkpoint must bring the OLD ROM back (D13 rule 2)"
    );
    // The whole machine, not half of it: `status` must not still be naming the reloaded image.
    let s = c.ok("emulator/status", json!({}));
    assert_ne!(
        s["romPath"],
        json!(p.display().to_string()),
        "a partial restore that leaves the new ROM's path behind is forbidden"
    );
    let _ = std::fs::remove_file(&p);
}

// ------------------------------------------------------------------ D13 rule 3: the cap

#[test]
fn the_cap_is_refused_loudly_and_never_silently_evicts() {
    let h = spawn("cpcap");
    let mut c = Client::connect(&h);
    let init = c.handshake(false);
    let n = cap(&init) as usize;

    let first = take(&mut c, Some("the one a client is still holding"));
    for i in 1..n {
        take(&mut c, Some(&format!("cp{i}")));
    }

    let e = c.err("emulator/checkpoint", json!({}));
    assert_eq!(e["code"], json!(-32005));
    assert_eq!(e["data"]["reason"], json!("checkpointCapReached"));
    assert_eq!(e["data"]["cap"], json!(n));
    assert_eq!(e["data"]["count"], json!(n));

    // Silent eviction is forbidden: the oldest id must still mean exactly what it meant.
    let r = c.ok("emulator/restore", json!({"id": first}));
    assert_eq!(r["frame"], json!(0));
    assert_eq!(
        c.ok("emulator/checkpoint_list", json!({}))["total"],
        json!(n)
    );

    // Dropping is how a client makes room.
    assert_eq!(
        c.ok("emulator/checkpoint_drop", json!({"id": first}))["removed"],
        json!(1)
    );
    c.ok("emulator/checkpoint", json!({}));
}

// ------------------------------------------------------------------ D13 rule 4: unknown ids

#[test]
fn restoring_an_unknown_or_dropped_id_is_refused_never_a_silent_no_op() {
    let h = spawn("cpunk");
    let mut c = Client::connect(&h);
    c.handshake(false);

    let e = c.err("emulator/restore", json!({"id": 9999}));
    assert_eq!(e["code"], json!(-32005));
    assert_eq!(e["data"]["reason"], json!("unknownCheckpoint"));
    assert_eq!(e["data"]["id"], json!(9999));

    let id = take(&mut c, None);
    c.ok("emulator/checkpoint_drop", json!({"id": id}));
    let e = c.err("emulator/restore", json!({"id": id}));
    assert_eq!(e["code"], json!(-32005));
    assert_eq!(e["data"]["reason"], json!("unknownCheckpoint"));

    // `id` is required and is a JSON number (D9), not a hex string.
    assert_eq!(c.err("emulator/restore", json!({}))["code"], json!(-32602));
    assert_eq!(
        c.err("emulator/restore", json!({"id": "0x1"}))["code"],
        json!(-32602)
    );
}

// ------------------------------------------------------------------ D13 rule 5: server-assigned ids

#[test]
fn ids_are_server_assigned_never_client_proposed_and_labels_are_carried_verbatim() {
    let h = spawn("cpid");
    let mut c = Client::connect(&h);
    c.handshake(false);

    // A client-proposed id is ignored: the server assigns, so two clients cannot collide.
    let a = c.ok("emulator/checkpoint", json!({"id": 4242}))["id"]
        .as_u64()
        .unwrap();
    let b = take(&mut c, None);
    assert_ne!(a, 4242, "the server assigns ids, the client never proposes");
    assert_ne!(a, b, "ids are unique");

    // A dropped id is never handed out again — an id must not quietly start meaning something else.
    c.ok("emulator/checkpoint_drop", json!({"id": b}));
    let d = take(&mut c, None);
    assert_ne!(d, b, "a retired id must never be reused");

    // The label is a human string, carried back verbatim and never interpreted.
    let weird = "  Zone 2 \"boss\" — take 3  ";
    let l = take(&mut c, Some(weird));
    let list = c.ok("emulator/checkpoint_list", json!({}));
    let entry = list["checkpoints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == json!(l))
        .unwrap()
        .clone();
    assert_eq!(entry["label"], json!(weird));
    // A checkpoint taken without one carries no `label` key at all, rather than an empty string.
    let unlabelled = list["checkpoints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == json!(a))
        .unwrap();
    assert!(unlabelled.get("label").is_none());
    assert!(unlabelled["frame"].is_number());
    assert!(unlabelled["mclk"].is_number());
    assert!(unlabelled["bytes"].as_u64().unwrap() > 0);

    assert_eq!(
        c.err("emulator/checkpoint", json!({"label": 7}))["code"],
        json!(-32602),
        "a non-string label is refused, never coerced"
    );
}

// ------------------------------------------------------------------ D13 rule 6: the cursored list

#[test]
fn checkpoint_list_is_bounded_cursored_and_flags_truncation() {
    let h = spawn("cplist");
    let mut c = Client::connect(&h);
    c.handshake(false);

    let empty = c.ok("emulator/checkpoint_list", json!({}));
    assert_eq!(empty["checkpoints"], json!([]));
    assert_eq!(empty["total"], json!(0));
    assert_eq!(empty["truncated"], json!(false));
    assert!(empty.get("cursor").is_none(), "no cursor when none remain");

    let ids: Vec<u64> = (0..3)
        .map(|i| take(&mut c, Some(&format!("s{i}"))))
        .collect();

    let page = c.ok("emulator/checkpoint_list", json!({"limit": 2}));
    let items = page["checkpoints"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["id"], json!(ids[0]));
    assert_eq!(items[1]["id"], json!(ids[1]));
    assert_eq!(page["total"], json!(3));
    assert_eq!(page["truncated"], json!(true));
    assert_eq!(
        page["cursor"],
        json!(2),
        "`cursor` is returned when more remain"
    );

    let rest = c.ok("emulator/checkpoint_list", json!({"cursor": 2, "limit": 2}));
    let items = rest["checkpoints"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], json!(ids[2]));
    assert_eq!(rest["truncated"], json!(false));
    assert!(rest.get("cursor").is_none());

    // Out-of-range paging params are refused loudly, never clamped (the house rule).
    assert_eq!(
        c.err("emulator/checkpoint_list", json!({"limit": 0}))["code"],
        json!(-32602)
    );
    assert_eq!(
        c.err("emulator/checkpoint_list", json!({"cursor": 99}))["code"],
        json!(-32602)
    );
}

// ------------------------------------------------------------------ drop

#[test]
fn drop_all_clears_every_slot_and_reports_how_many_went() {
    let h = spawn("cpdrop");
    let mut c = Client::connect(&h);
    c.handshake(false);

    for i in 0..3 {
        take(&mut c, Some(&format!("d{i}")));
    }
    let r = c.ok("emulator/checkpoint_drop", json!({"all": true}));
    assert_eq!(r["removed"], json!(3));
    let list = c.ok("emulator/checkpoint_list", json!({}));
    assert_eq!(list["checkpoints"], json!([]));
    assert_eq!(list["total"], json!(0));

    // Dropping nothing is an explicit `removed: 0`, not silence and not an invented error.
    assert_eq!(
        c.ok("emulator/checkpoint_drop", json!({"all": true}))["removed"],
        json!(0)
    );
    assert_eq!(
        c.ok("emulator/checkpoint_drop", json!({"id": 12345}))["removed"],
        json!(0)
    );

    // One of `id` or `all` is required, and they are mutually exclusive.
    assert_eq!(
        c.err("emulator/checkpoint_drop", json!({}))["code"],
        json!(-32602)
    );
    assert_eq!(
        c.err("emulator/checkpoint_drop", json!({"id": 1, "all": true}))["code"],
        json!(-32602)
    );
}

// ------------------------------------------------------------------ volatility, per session

#[test]
fn checkpoints_are_per_server_session_and_do_not_outlive_it() {
    let h = spawn("cpvol");
    let mut c = Client::connect(&h);
    c.handshake(false);
    take(&mut c, Some("session one"));
    assert_eq!(
        c.ok("emulator/checkpoint_list", json!({}))["total"],
        json!(1)
    );
    drop(c);
    drop(h);

    // A brand-new server on a brand-new socket starts with none — nothing was written anywhere.
    let h2 = spawn("cpvol2");
    let mut c2 = Client::connect(&h2);
    c2.handshake(false);
    assert_eq!(
        c2.ok("emulator/checkpoint_list", json!({}))["total"],
        json!(0)
    );
}

#[test]
fn checkpoints_are_shared_across_connections_to_one_server() {
    // The slots live on the engine (the machine), not on a connection — two clients on one bus see one
    // set of coordinates, which is exactly why ids are server-assigned.
    let h = spawn("cpshare");
    let mut a = Client::connect(&h);
    a.handshake(false);
    let id = take(&mut a, Some("taken by A"));

    let mut b = Client::connect(&h);
    b.handshake(false);
    let list = b.ok("emulator/checkpoint_list", json!({}));
    assert_eq!(list["checkpoints"][0]["id"], json!(id));
    b.ok("emulator/restore", json!({"id": id}));
}
