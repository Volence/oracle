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

/// Two listings that declare no `EndOfRom`, so both are accepted *unverified* against the fixture ROM.
/// They name the same symbol at two different addresses — D7's stale-literal hazard, in fixture form.
const LST_A: &str = "\
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

const LST_B: &str = "\
  Symbol Table (* = unused):
  --------------------------

 EntryPoint : 300 C |
 Player_1 : FFFF8D1E C |

    2 symbols
    0 unused symbols
";

fn write_lst(tag: &str, text: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("ae-{tag}-{}.lst", std::process::id()));
    std::fs::write(&p, text).unwrap();
    p
}

/// The ids in a `checkpoint_list` page, in the order the server returned them.
fn page_ids(page: &Value) -> Vec<u64> {
    page["checkpoints"]
        .as_array()
        .expect("`checkpoints` must be an array")
        .iter()
        .map(|e| e["id"].as_u64().expect("every entry carries a numeric id"))
        .collect()
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

    // D13 rule 1, one half: no advertised method is a persist-to-disk variant. The whole method table is
    // the advertised set, so this is exhaustive over *names*. Names are the weak half — see
    // `the_checkpoint_handlers_contain_no_path_from_the_bus_to_the_filesystem` for the half that bites.
    for spec in METHODS {
        let n = spec.name;
        for banned in [
            "save_state",
            "load_state",
            "save_checkpoint",
            "checkpoint_save",
            "checkpoint_export",
            "checkpoint_write",
            "export_state",
            "write_state",
            "dump_state",
            "persist",
            "to_file",
            "to_disk",
            "snapshot_to",
        ] {
            assert!(
                !n.contains(banned),
                "D13 rule 1 forbids a persist-to-disk variant, found {n} (matched {banned:?})"
            );
        }
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

#[test]
fn a_drop_between_two_pages_never_skips_a_live_checkpoint() {
    // §6.1: "a client must never be handed a partial list it can mistake for a complete one" — and the
    // section is explicit that two clients share one bus, which is the stated reason ids are
    // server-assigned. A *positional* cursor breaks exactly there: a drop before an outstanding cursor
    // shifts every later slot left, so the next page steps over a live checkpoint and still reports
    // `truncated: false`. The cursor is therefore an **id** ("resume after this id"), which is stable
    // under concurrent drops because ids are monotonic and never reused.
    let h = spawn("cpskip");
    let mut c = Client::connect(&h);
    let init = c.handshake(false);
    let n = cap(&init) as usize;
    assert!(n >= 5, "this test needs a cap of at least 5, got {n}");

    let ids: Vec<u64> = (0..n)
        .map(|i| take(&mut c, Some(&format!("p{i}"))))
        .collect();

    // Drop the first slot up front so a positional cursor and an id cursor cannot coincide by accident.
    assert_eq!(
        c.ok("emulator/checkpoint_drop", json!({"id": ids[0]}))["removed"],
        json!(1)
    );

    let p1 = c.ok("emulator/checkpoint_list", json!({"limit": 3}));
    assert_eq!(page_ids(&p1), vec![ids[1], ids[2], ids[3]]);
    assert_eq!(p1["truncated"], json!(true));
    assert_eq!(
        p1["cursor"],
        json!(ids[3]),
        "`cursor` must be the id to resume after, not a Vec position"
    );

    // A second client drops a checkpoint that page 1 already returned — i.e. *before* the cursor.
    let mut b = Client::connect(&h);
    b.handshake(false);
    assert_eq!(
        b.ok("emulator/checkpoint_drop", json!({"id": ids[1]}))["removed"],
        json!(1)
    );

    // Walk the rest and prove the union of the pages covers every checkpoint still alive.
    let mut seen = page_ids(&p1);
    let mut cursor = p1["cursor"].clone();
    loop {
        let p = c.ok(
            "emulator/checkpoint_list",
            json!({"cursor": cursor, "limit": 3}),
        );
        seen.extend(page_ids(&p));
        match p.get("cursor") {
            Some(next) => {
                assert_eq!(p["truncated"], json!(true));
                cursor = next.clone();
            }
            None => {
                assert_eq!(
                    p["truncated"],
                    json!(false),
                    "no cursor means the walk is complete"
                );
                break;
            }
        }
    }
    for id in &ids[2..] {
        assert!(
            seen.contains(id),
            "checkpoint {id} is live but the paged walk never listed it: {seen:?}"
        );
    }
    assert_eq!(
        c.ok("emulator/checkpoint_list", json!({}))["total"],
        json!(n - 2)
    );
}

#[test]
fn a_cursor_whose_checkpoints_are_all_gone_is_an_empty_page_not_a_hard_error() {
    // Under a positional cursor a `drop all` turned every outstanding cursor into a `-32602`, because
    // the bound was the live count. An id cursor has no such coupling: "nothing after id N" is a
    // complete, honest answer, and `total` tells the client the set changed under it. What is still
    // refused loudly is a cursor the server could never have issued — that is a typo, not a stale page.
    let h = spawn("cpstale");
    let mut c = Client::connect(&h);
    c.handshake(false);
    let ids: Vec<u64> = (0..3)
        .map(|i| take(&mut c, Some(&format!("s{i}"))))
        .collect();

    let p1 = c.ok("emulator/checkpoint_list", json!({"limit": 2}));
    let cursor = p1["cursor"].clone();
    assert_eq!(cursor, json!(ids[1]));

    c.ok("emulator/checkpoint_drop", json!({"all": true}));
    let p2 = c.ok("emulator/checkpoint_list", json!({"cursor": cursor}));
    assert_eq!(p2["checkpoints"], json!([]));
    assert_eq!(p2["total"], json!(0));
    assert_eq!(p2["truncated"], json!(false));
    assert!(p2.get("cursor").is_none());

    // An id the server never assigned is still refused, never clamped (the house rule).
    assert_eq!(
        c.err("emulator/checkpoint_list", json!({"cursor": 9999}))["code"],
        json!(-32602)
    );
}

#[test]
fn each_listed_slot_reports_its_own_coordinate_and_its_own_size() {
    // §6.1 names `frame`, `mclk` and `bytes` as normative result fields of `checkpoint_list`. Taking
    // every checkpoint at frame 0 would let all three be hardcoded and still pass, so these are taken at
    // three different coordinates and cross-checked against what `emulator/checkpoint` itself reported.
    let h = spawn("cpcoord");
    let mut c = Client::connect(&h);
    c.handshake(false);

    let mut expected: Vec<(u64, u64, u64, u64)> = Vec::new(); // id, frame, mclk, bytes
    for (i, advance) in [0u64, 3, 4].iter().enumerate() {
        if *advance > 0 {
            c.ok("emulator/run_frames", json!({"frames": advance}));
        }
        let r = c.ok("emulator/checkpoint", json!({"label": format!("c{i}")}));
        expected.push((
            r["id"].as_u64().unwrap(),
            r["frame"].as_u64().unwrap(),
            r["mclk"].as_u64().unwrap(),
            r["bytes"].as_u64().unwrap(),
        ));
    }
    assert_eq!(
        expected.iter().map(|e| e.1).collect::<Vec<_>>(),
        vec![0, 3, 7],
        "the three slots must sit at three different frames"
    );
    let mclks: Vec<u64> = expected.iter().map(|e| e.2).collect();
    assert!(mclks[0] < mclks[1] && mclks[1] < mclks[2]);

    // Advance again, so a list that reported *now* instead of the capture coordinate would be caught.
    c.ok("emulator/run_frames", json!({"frames": 2}));

    let list = c.ok("emulator/checkpoint_list", json!({}));
    let items = list["checkpoints"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    for (e, (id, frame, mclk, bytes)) in items.iter().zip(expected) {
        assert_eq!(e["id"], json!(id));
        assert_eq!(e["frame"], json!(frame), "entry {id} has the wrong frame");
        assert_eq!(e["mclk"], json!(mclk), "entry {id} has the wrong mclk");
        assert_eq!(e["bytes"], json!(bytes), "entry {id} has the wrong size");
    }
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

/// Everything the four checkpoint handlers and their helpers are made of, as source text.
///
/// D13 rule 1 is a claim about *code paths* ("there is no code path from here to the filesystem"), and
/// the only honest way to check a claim about code paths from a test is to read the code. A wire test
/// cannot see a stray `std::fs::write`, and neither can a grep over method names — the reviewer proved
/// that by adding one and watching the whole suite stay green.
fn checkpoint_source() -> String {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/engine.rs"))
        .expect("engine.rs must be readable from the crate root");
    let mut out = String::new();
    for name in [
        "checkpoint",
        "restore",
        "checkpoint_list",
        "checkpoint_drop",
        "parse_checkpoint_id",
        "unknown_checkpoint",
    ] {
        out.push_str(&fn_body(&src, name));
        out.push('\n');
    }
    out
}

/// The body of `fn <name>(…)`, by brace counting. Panics if the function is missing or unbalanced — a
/// rename must break this test loudly rather than silently stop checking anything.
fn fn_body(src: &str, name: &str) -> String {
    let needle = format!("fn {name}(");
    let start = src
        .find(&needle)
        .unwrap_or_else(|| panic!("`{needle}` is not in engine.rs — did it get renamed?"));
    let open = start
        + src[start..]
            .find('{')
            .unwrap_or_else(|| panic!("`{needle}` has no body"));
    let mut depth = 0usize;
    for (i, ch) in src[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let body = &src[open..open + i + 1];
                    assert!(
                        body.len() > 40,
                        "`{needle}` body came out suspiciously short"
                    );
                    return body.to_string();
                }
            }
            _ => {}
        }
    }
    panic!("`{needle}` body has unbalanced braces");
}

/// Tokens that can only appear in code that touches the filesystem.
const FS_TOKENS: &[&str] = &[
    "std::fs",
    "fs::",
    "File",
    "OpenOptions",
    "PathBuf",
    "Path::",
    "BufWriter",
    "write_all",
    "tempfile",
    "temp_dir",
    "create_dir",
    "to_writer",
];

fn fs_tokens_in(text: &str) -> Vec<&'static str> {
    FS_TOKENS
        .iter()
        .copied()
        .filter(|t| text.contains(t))
        .collect()
}

#[test]
fn the_checkpoint_handlers_contain_no_path_from_the_bus_to_the_filesystem() {
    // D13 rule 1, the half that bites: "checkpoints live in server memory ... and are **never** written
    // to disk". This is a source-level assertion over the four handlers and their helpers, because that
    // is what the claim is about. It is not airtight — a violation hidden behind a helper defined
    // elsewhere in the file would slip past — but it does catch the thing that actually happens, which
    // is somebody reaching for `std::fs` right where the bytes are.
    let src = checkpoint_source();
    let hits = fs_tokens_in(&src);
    assert!(
        hits.is_empty(),
        "D13 rule 1: the checkpoint handlers must not touch the filesystem, found {hits:?}"
    );

    // Anti-vacuity: the same scan, run over two handlers that legitimately DO read files, must fire.
    // Without this the assertion above could pass because the scanner is broken rather than because the
    // code is clean — which is exactly how the name-grep this replaced managed to prove nothing.
    let whole =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/engine.rs")).unwrap();
    for control in ["reload_rom", "load_symbols"] {
        assert!(
            !fs_tokens_in(&fn_body(&whole, control)).is_empty(),
            "the scanner is broken: it found no filesystem token in `fn {control}`, which reads a file"
        );
    }
}

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

// ------------------------------------------------------------------ the engine-side shadows

#[test]
fn restore_brings_the_held_pads_back_with_the_machine() {
    // `held` is the engine's mirror of the pads inside `System`, so a restore that put the machine back
    // but left the mirror alone would leave the *next* frame pressing a button the restored machine
    // never had down — the half-applied restore §6.1 forbids, one field over.
    let h = spawn("cpheld");
    let mut c = Client::connect(&h);
    c.handshake(false);

    let r = c.ok("emulator/hold", json!({"port": 0, "buttons": ["up"]}));
    assert_eq!(r["held"], json!(["up"]));
    let id = take(&mut c, Some("holding up"));

    c.ok("emulator/release_all", json!({}));
    let r = c.ok("emulator/hold", json!({"port": 0, "buttons": ["a"]}));
    assert_eq!(r["held"], json!(["a"]));

    c.ok("emulator/restore", json!({"id": id}));
    // A no-op `hold` reports the live set without changing it.
    let r = c.ok("emulator/hold", json!({"port": 0, "buttons": []}));
    assert_eq!(
        r["held"],
        json!(["up"]),
        "the restored machine's held set must come back with it"
    );
}

#[test]
fn restore_brings_the_symbol_table_back_with_the_cartridge_it_was_bound_to() {
    // D7's named hazard is stale symbol resolution: "the 'verified' literal went stale within the
    // session". `symbols`/`symbols_path` are engine-side shadows of the loaded cartridge in exactly the
    // way `rom_path` is, so a restore that brought the ROM back but left the table behind would resolve
    // names against a cartridge that is no longer loaded — and `read_memory {symbol}` would then read a
    // wrong address and report success.
    let h = spawn("cpsym");
    let mut c = Client::connect(&h);
    c.handshake(false);

    // A coordinate taken before any listing was loaded at all.
    let clean = take(&mut c, Some("before any listing"));

    let a = write_lst("cpsym-a", LST_A);
    let load_a = c.ok(
        "emulator/load_symbols",
        json!({"path": a.display().to_string()}),
    );
    assert_eq!(load_a["symbolCount"], json!(5));
    let with_a = take(&mut c, Some("listing A loaded"));

    // A different listing: same names, different addresses — the exact shape D7 says goes stale.
    let b = write_lst("cpsym-b", LST_B);
    let load_b = c.ok(
        "emulator/load_symbols",
        json!({"path": b.display().to_string()}),
    );
    assert_eq!(load_b["symbolCount"], json!(2));
    assert_eq!(
        c.ok("emulator/lookup_symbol", json!({"name": "EntryPoint"}))["addr"],
        json!("0x00000300")
    );

    // Back to the coordinate where listing A was loaded: the table that was bound there comes back.
    c.ok("emulator/restore", json!({"id": with_a}));
    let s = c.ok("emulator/status", json!({}));
    assert_eq!(
        s["symbolCount"],
        json!(5),
        "the restored machine's symbol table must come back with it"
    );
    assert_eq!(s["symbolsPath"], json!(a.display().to_string()));
    assert_eq!(
        c.ok("emulator/lookup_symbol", json!({"name": "EntryPoint"}))["addr"],
        json!("0x00000200"),
        "resolving after a restore must use the restored machine's table (D7)"
    );

    // And back to before any listing existed: the table goes away with the machine, rather than
    // outliving it and answering for a cartridge state that is gone.
    c.ok("emulator/restore", json!({"id": clean}));
    let s = c.ok("emulator/status", json!({}));
    assert_eq!(s["symbolCount"], json!(0));
    assert_eq!(s["symbolsPath"], json!(null));
    assert_eq!(
        c.err("emulator/lookup_symbol", json!({"name": "EntryPoint"}))["code"],
        json!(-32012),
        "no symbols were loaded at that coordinate, so resolution must refuse, not answer"
    );

    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
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
