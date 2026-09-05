//! **The schema harness's own tests** — contract §8 item 15 (D14).
//!
//! Four jobs, and only the first is the obvious one:
//!
//! 1. **Freshness.** The vendored schema is byte-identical to the contract's copy.
//! 2. **Coverage, reported and pinned.** The schema began as a SEED covering a minority of what this
//!    server advertised; it now has a `result` fragment for **every** advertised method, which is what
//!    `UNCOVERED_METHODS` being pinned *empty* says. The document's fragments therefore split exactly two
//!    ways, and both halves are named rather than counted: the ones this server advertises
//!    (`engine::METHODS`, derived) and the ones it does not (`SCHEMATIZED_NOT_ADVERTISED`, enumerated
//!    below). No number appears in this file that is not the length of a list printed beside it — the
//!    counts in the prose here went stale twice while every assertion stayed green, which is the whole
//!    argument for enumerating instead. The pins guard the two silent directions: a newly advertised
//!    method joining the unchecked pile, and a fragment arriving for a method nobody serves.
//! 3. **The divergence registry, reported and kept live.** Shapes where the server and the schema
//!    disagree are registered rather than silenced, each with its CR number, and the report prints
//!    beside the coverage split so nobody reads a green suite as "fully conformant". An entry that stops
//!    firing fails `every_registered_divergence_is_still_live`, so the list cannot rot after a ruling —
//!    which is exactly how the first two entries it held came to be deleted. It holds CR-16 today, found
//!    by turning item 20's closure on for the first time.
//! 4. **Anti-vacuity.** Proof that the validator *rejects*. This repo has twice shipped an assertion that
//!    passed while testing nothing — a volatility test that was a name grep, an assertion that passed with
//!    zero enqueues. A validator that accepts everything is exactly that failure wearing a library's
//!    clothes, and it would be invisible: the suite would be green and the wire unchecked. Since §8 item
//!    20 landed, this carries the closure's own control too: the working keyword is proven to accept a
//!    conformant reply and catch a surplus key, and the obvious-but-wrong one is proven to do neither.

mod common;

use common::schema::{
    check_incoming, check_incoming_strict, compile_fragment, divergence_report, schema_root,
    schemas, vectors_root, KNOWN_CONTRACT_DIVERGENCES,
};
use oracle_aether::engine::METHODS;
use serde_json::{json, Value};
use std::path::PathBuf;

/// The provenance sidecar, compiled in. It is the pin: the gate reads `pin.blob` out of *this* text and
/// hashes the vendored bytes against it, so the record a human maintains and the value a machine checks
/// are one artifact rather than two that can drift.
const PROVENANCE: &str = include_str!("contract/PROVENANCE.md");

/// The two environment variables this file will consult, named here so every refusal can print them.
const ENV_SCHEMA_FILE: &str = "AETHER_CONTRACT_SCHEMA";
const ENV_CONTRACT_REPO: &str = "AETHER_CONTRACT_REPO";

/// The path of the schema inside the contract repo — used only as an argument to `git`, never joined
/// onto a checkout and read.
const CONTRACT_REL: &str = "contract/schema/bus-protocol.schema.json";

/// One `pin.<key> = <value>` marker out of [`PROVENANCE`].
///
/// **Missing is loud.** A parser that returned `None` and let the caller shrug would turn "the sidecar
/// lost its pin" into a silent pass, which is the failure this whole section exists to end.
fn pin(key: &str) -> String {
    let needle = format!("pin.{key}");
    for line in PROVENANCE.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(&needle) {
            if let Some(v) = rest.trim_start().strip_prefix('=') {
                let v = v.trim();
                assert!(
                    !v.is_empty(),
                    "PROVENANCE.md: `{needle}` has an empty value"
                );
                return v.to_string();
            }
        }
    }
    panic!(
        "PROVENANCE.md carries no `{needle} = …` marker. The vendored schema's provenance sidecar IS \
         the pin (empyrean contract/SUITE_PATHS.md, \"What a resolver owes its reader\"), so a missing \
         marker is a gate that cannot run, not a gate that passes."
    );
}

// ---------------------------------------------------------------------------------------------------
// A git blob hash, computed here. ~60 lines of SHA-1 rather than a dependency: `oracle-aether`'s runtime
// deps are pinned at two crates by its own Cargo.toml note, and a hash whose implementation is in this
// file can be — and is — proven against known constants below rather than trusted.
// ---------------------------------------------------------------------------------------------------

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let bitlen = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());

    for chunk in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for (i, word) in chunk.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// `git hash-object -t blob`, in one line of definition: SHA-1 over `"blob <len>\0"` then the content.
fn git_blob_hash(content: &[u8]) -> String {
    let mut framed = format!("blob {}\0", content.len()).into_bytes();
    framed.extend_from_slice(content);
    sha1(&framed)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
}

/// **The control on the hasher**, and it comes first because everything below is worthless without it.
///
/// A hand-rolled hash that is subtly wrong would make the pin check pass for the wrong reason forever —
/// and it would pass *today*, because the pin would simply record whatever the broken function returns.
/// So the implementation is closed against constants that come from outside this repo: git's empty blob
/// and the SHA-1 of "abc", both of which are published values, plus git's own hash of a short literal.
#[test]
fn the_blob_hasher_is_git_s() {
    assert_eq!(
        sha1(b"abc")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
        "a9993e364706816aba3e25717850c26c9cd0d89d",
        "SHA-1(\"abc\"), the FIPS 180-1 test vector"
    );
    assert_eq!(
        git_blob_hash(b""),
        "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391",
        "git's empty blob, the most-quoted hash in the tool"
    );
    assert_eq!(
        git_blob_hash(b"hello\n"),
        "ce013625030ba8dba906f756967f9e9ca394464a",
        "`printf 'hello\\n' | git hash-object --stdin`"
    );
}

// ---------------------------------------------------------------------------------------------------
// 1. Freshness — content-addressed against the pin, then optionally confirmed against the contract repo
// ---------------------------------------------------------------------------------------------------

/// **Step 0, and it always runs.** The vendored bytes hash to the blob `PROVENANCE.md` pins.
///
/// This replaces a gate that walked up from `CARGO_MANIFEST_DIR` looking for
/// `empyrean/contract/schema/bus-protocol.schema.json` and byte-compared **the peer's live working
/// tree** — registered as `F-SCHEMA-READS-LIVE-EMPYREAN` and ruled on suite-wide in
/// `empyrean/contract/SUITE_PATHS.md` at `38f6df4`: *"A gate that proves a vendored copy of a peer's
/// CONTENT is fresh reads the peer through git objects at a named revision, never through the peer's
/// working tree."* The old shape went red when the hub saved mid-edit and, the half that matters, would
/// have gone **green against a change no other lane could see**.
///
/// Content-addressed rather than "equal to a path resolved at a revision", per the same ruling and
/// aurora's `effects-preset-schema-drift` precedent: a hash cannot be satisfied by a coincidentally
/// similar file, and it needs no peer at all, so this half of the gate is never skipped on any machine.
#[test]
fn the_vendored_schema_is_the_blob_provenance_pins() {
    let vendored = common::schema::VENDORED_SCHEMA.as_bytes();
    let want_blob = pin("blob");
    let want_bytes: usize = pin("bytes").parse().expect("pin.bytes is a number");
    let got_blob = git_blob_hash(vendored);
    eprintln!(
        "RESULT ok step=0-pin blob={got_blob} bytes={} revision={}",
        vendored.len(),
        pin("revision")
    );
    assert_eq!(
        vendored.len(),
        want_bytes,
        "the vendored schema is {} bytes; PROVENANCE.md pins {want_bytes}",
        vendored.len()
    );
    assert_eq!(
        got_blob,
        want_blob,
        "the vendored schema hashes to {got_blob}; PROVENANCE.md pins blob {want_blob} at revision {}.\n\
         Either the copy was edited in place — which is never allowed, a hand-corrected vendored copy is \
         a copy whose provenance is worthless — or a re-vendor landed without updating the sidecar. \
         PROVENANCE.md carries the recipe.",
        pin("revision")
    );
}

/// **The same step 0, for the second vendored artifact** — the contract's own wire vectors (§11.36).
///
/// Vendored for the first time in the CR-M parcel, and pinned by the same content address for the same
/// reason: a vector file read out of a peer's working tree proves nothing attributable, and one that is
/// hand-edited here proves less than nothing.
#[test]
fn the_vendored_vectors_are_the_blob_provenance_pins() {
    let vendored = common::schema::VENDORED_VECTORS.as_bytes();
    let want_blob = pin("vectors.blob");
    let want_bytes: usize = pin("vectors.bytes")
        .parse()
        .expect("pin.vectors.bytes is a number");
    let got_blob = git_blob_hash(vendored);
    eprintln!(
        "RESULT ok step=0-pin artifact=vectors blob={got_blob} bytes={} revision={}",
        vendored.len(),
        pin("vectors.revision")
    );
    assert_eq!(vendored.len(), want_bytes);
    assert_eq!(
        got_blob, want_blob,
        "the vendored vectors hash to {got_blob}; PROVENANCE.md pins blob {want_blob}"
    );
    // The two artifacts are pinned at ONE revision, deliberately. They are adopted together — §11.36 is
    // one commit carrying both — and a schema pinned at one revision beside vectors pinned at another is
    // a gate asserting that a fragment accepts documents written for a different fragment.
    assert_eq!(
        pin("vectors.revision"),
        pin("revision"),
        "the schema and the vectors must be pinned at the SAME contract revision"
    );
}

/// **The contract's own vectors, run** — upstream's G3 and G4, replicated against the vendored copies.
///
/// This is what makes vendoring the vectors worth the bytes. Every `expect: "pass"` case must validate
/// against the fragment its `method`/`kind` names, and — the half that matters — **every
/// `expect: "fail"` case must be REFUSED**. A fail-vector the schema accepts is upstream's own word for
/// what it means: *"this fragment is vacuous here"*. Our previous position was that the schema's shapes
/// were checked only by the replies this server happens to emit, which cannot witness a refusal the
/// server never attempts.
///
/// Server-emitted payloads (a `result`, or an event's `params`) are merged with the vectors file's own
/// envelope before validation, exactly as upstream's runner does, and then re-validated under §8 item
/// 20's closure — the keyword that lives in the harness and never in the published artifact.
///
/// **What this cannot witness**, so nobody reads its green as more: it is a document check. It says
/// nothing about whether this server ever emits any of these shapes. `emulator/lookup_equate`'s live
/// obligations are `tests/symbols_path.rs`-shaped and live in `tests/methods.rs`.
#[test]
fn the_contracts_own_vectors_pass_and_fail_exactly_as_declared() {
    let schema = schema_root();
    let vectors = vectors_root();
    let cases = vectors["cases"]
        .as_array()
        .expect("vectors.cases is an array");

    let mut passed = 0usize;
    let mut refused = 0usize;
    let mut closed_ok = 0usize;
    let mut per_method: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();

    for case in cases {
        let method = case["method"].as_str().expect("case.method");
        let kind = case["kind"].as_str().expect("case.kind");
        let expect = case["expect"].as_str().expect("case.expect");
        let group = case
            .get("group")
            .and_then(Value::as_str)
            .unwrap_or("methods");
        let why = case.get("why").and_then(Value::as_str).unwrap_or("");
        let label = format!("{method} {kind} [{group}] ({why})");
        *per_method.entry(method).or_default() += 1;

        // A server-EMITTED payload travels inside the reply (or event) envelope; a `params` document is
        // the bare request object.
        let emitted = kind == "result" || group == "events";
        let mut doc = case["doc"].clone();
        if emitted {
            let envelope = if group == "events" {
                &vectors["eventEnvelope"]
            } else {
                &vectors["envelope"]
            };
            let mut merged = envelope
                .as_object()
                .cloned()
                .expect("envelope is an object");
            for k in case
                .get("envelopeDrop")
                .and_then(Value::as_array)
                .unwrap_or(&Vec::new())
            {
                merged.remove(k.as_str().expect("envelopeDrop entry is a string"));
            }
            for (k, v) in doc.as_object().expect("case.doc is an object") {
                merged.insert(k.clone(), v.clone());
            }
            doc = Value::Object(merged);
        }

        let Some(fragment) = schema
            .get(group)
            .and_then(|g| g.get(method))
            .and_then(|f| f.get(kind))
        else {
            failures.push(format!("{label}: no fragment in the vendored schema"));
            continue;
        };
        let open = compile_fragment(fragment, &label, false);
        let accepted = open.is_valid(&doc);
        match expect {
            "pass" => {
                if accepted {
                    passed += 1;
                    if emitted {
                        if compile_fragment(fragment, &label, true).is_valid(&doc) {
                            closed_ok += 1;
                        } else {
                            failures.push(format!(
                                "{label}: §8 item 20's closure rejects a PASS vector — it carries a key \
                                 its fragment never declared"
                            ));
                        }
                    }
                } else {
                    let first = open
                        .iter_errors(&doc)
                        .next()
                        .map_or_else(|| "?".to_string(), |e| e.to_string());
                    failures.push(format!("{label}: expected to validate, got {first}"));
                }
            }
            "fail" => {
                if accepted {
                    failures.push(format!(
                        "{label}: expected REJECTION, the schema ACCEPTED it — this fragment is vacuous \
                         here"
                    ));
                } else {
                    refused += 1;
                }
            }
            other => failures.push(format!("{label}: unknown expect {other:?}")),
        }
    }

    eprintln!(
        "RESULT vectors cases={} pass={passed} fail={refused} closed={closed_ok} methods={}",
        cases.len(),
        per_method.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {} contract vectors did not behave as declared:\n  {}",
        failures.len(),
        cases.len(),
        failures.join("\n  ")
    );
    // Anti-vacuity for the runner ITSELF: a loop that validated nothing would report zero failures and
    // read as a clean sweep. Both halves must be non-empty, and the ten §11.36 rows must be among them.
    assert!(
        passed > 0 && refused > 0,
        "the vector runner checked nothing"
    );
    assert_eq!(
        per_method.get("emulator/lookup_equate").copied(),
        Some(10),
        "§11.36 adopted ten lookup_equate vectors; the vendored file must carry all ten"
    );
}

/// **Steps 1 and 2, and their loud refusal.** Confirm the pin against something outside this repo, when
/// something outside this repo has been named.
///
/// There is deliberately **no walk**. `SUITE_PATHS.md`: *"An env-var override pointing at a file is
/// legitimate; its absence is a loud skip naming the variable, not a walk."* A skip here costs nothing
/// that matters, because the artifact's identity is already closed by
/// [`the_vendored_schema_is_the_blob_provenance_pins`]; what a skip gives up is only the confirmation
/// that the pin names something real and merged upstream.
#[test]
fn the_pin_is_confirmed_against_the_contract_repo_or_says_it_could_not() {
    let vendored = common::schema::VENDORED_SCHEMA.as_bytes();
    let rev = pin("revision");
    let blob = pin("blob");

    // Step 1 — an explicit file.
    if let Ok(p) = std::env::var(ENV_SCHEMA_FILE) {
        let path = PathBuf::from(&p);
        eprintln!(
            "RESULT ok step=1-env-file var={ENV_SCHEMA_FILE} path={}",
            path.display()
        );
        let up = std::fs::read(&path).unwrap_or_else(|e| {
            panic!(
                "${ENV_SCHEMA_FILE} points at {}, which cannot be read: {e}",
                path.display()
            )
        });
        assert_eq!(
            git_blob_hash(&up),
            blob,
            "${ENV_SCHEMA_FILE} = {} hashes to {}, not the pinned {blob}",
            path.display(),
            git_blob_hash(&up)
        );
        assert_eq!(
            up, vendored,
            "same hash, different bytes is impossible; something is very wrong"
        );
        return;
    }

    // Step 2 — a contract CHECKOUT, read only through its object store.
    if let Ok(repo) = std::env::var(ENV_CONTRACT_REPO) {
        eprintln!("RESULT ok step=2-env-repo var={ENV_CONTRACT_REPO} repo={repo} rev={rev}");
        let git = |args: &[&str]| -> Vec<u8> {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .output()
                .unwrap_or_else(|e| panic!("running `git {args:?}` in {repo}: {e}"));
            assert!(
                out.status.success(),
                "`git {args:?}` in {repo} failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            out.stdout
        };
        // The blob exists in that repo and IS the vendored bytes. `cat-file` reads the object store;
        // the working tree is never touched, which is the whole point of the reshape.
        //
        // **This first check alone would be nearly vacuous, and that was observed rather than reasoned:**
        // pointed at *this* repository it PASSES, because vendoring the schema here put the identical
        // blob in this repo's object store too. Content-addressing says "some repo has these bytes",
        // which every repo that vendored them does. The two checks after it are what make the step mean
        // something — the named revision carries that blob at that path, and the revision is merged —
        // and pointing this at `oracle` fails on the second one, naming the path it could not resolve.
        let object = git(&["cat-file", "blob", &blob]);
        assert_eq!(
            object,
            vendored,
            "blob {blob} in {repo} is {} bytes and the vendored copy is {}",
            object.len(),
            vendored.len()
        );
        // …and the revision the sidecar names really carries that blob, and is merged.
        let at_rev = String::from_utf8(git(&["rev-parse", &format!("{rev}:{CONTRACT_REL}")]))
            .expect("utf8")
            .trim()
            .to_string();
        assert_eq!(
            at_rev, blob,
            "{rev}:{CONTRACT_REL} is blob {at_rev}, not the pinned {blob}"
        );
        let default = ["origin/main", "main"]
            .into_iter()
            .find(|r| {
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(&repo)
                    .args(["rev-parse", "--verify", "--quiet", r])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false)
            })
            .unwrap_or_else(|| {
                panic!(
                    "{repo} has neither `origin/main` nor `main` — cannot check the pin is merged"
                )
            });
        let merged = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["merge-base", "--is-ancestor", &rev, default])
            .status()
            .expect("git merge-base")
            .success();
        assert!(
            merged,
            "{rev} is NOT an ancestor of {default} in {repo}: the vendored copy tracks a revision that \
             is not on the contract's default branch. That is a legitimate state while an adjudicated \
             amendment finishes merging — and it is not a state to be silent about."
        );
        return;
    }

    // Step 3 — nothing named. A loud line in the run's own output, because "a green log and an absent
    // run are the same artifact" (SUITE_PATHS.md, protocol bar 25).
    eprintln!(
        "\n=========================================================================\n\
         SKIPPED: the vendored schema's pin was NOT confirmed against the contract repo.\n\
         Consulted, in order, and neither was set:\n  \
           ${ENV_SCHEMA_FILE}  — a path to a bus-protocol.schema.json FILE\n  \
           ${ENV_CONTRACT_REPO} — a path to a git CHECKOUT of the contract repo\n\
         There is no filesystem walk on purpose: a gate that goes looking for a peer's working tree\n\
         reports a verdict about whatever that tree contained when it ran\n\
         (F-SCHEMA-READS-LIVE-EMPYREAN; empyrean contract/SUITE_PATHS.md at 38f6df4).\n\
         What still ran: the copy's identity is closed content-addressed against PROVENANCE.md's\n\
         pin.blob = {blob} (revision {rev}) by\n\
         `the_vendored_schema_is_the_blob_provenance_pins`, which never skips.\n\
         What did NOT run: confirmation that {rev} exists upstream and is merged.\n\
         =========================================================================\n"
    );
}

// ---------------------------------------------------------------------------------------------------
// 2. Coverage — reported, and pinned so it cannot shrink
// ---------------------------------------------------------------------------------------------------

/// The methods we advertise that the SEED schema gives **no** `result` schema for.
///
/// Pinned deliberately. Two ways this list can be wrong, and the test catches both:
///
/// * a **newly advertised** method would join it silently, so a new op would arrive on the wire with
///   nothing checking its result shape and nothing saying so;
/// * a **newly schematized** method would leave it, and that has to be a deliberate edit here rather
///   than a number quietly improving.
///
/// The list did **not** move when `emulator/pixel_attribution` was implemented, and that is the
/// mechanism working rather than a gap: CR-10 put the fragment in the contract *first*, so the method
/// arrived already schematized and the covered count went 8 → 9 while this list stayed at 12.
///
/// **It is now empty, and it emptied the right way round.** Writing the 12 missing fragments was
/// explicitly not this harness's job — writing schemas from what this server emits would encode the
/// implementation as the contract, the exact inversion of "the contract leads" (§8). So they were raised
/// as CR-13, the contract ruled every key on its merits (`empyrean` `f309cc8`, `protocol.md` §11.5:
/// registered, restructured, or **struck**), and the 12 fragments arrived upstream. Coverage went 9 → 21
/// because the contract moved first, which is the only direction that counts — and 21 → 25 on 2026-08-15
/// for the same reason, when CR-11/CR-12's four fragments were ruled into the contract before a line of
/// handler was written.
///
/// **Those four numbers are a record of two moves in 2026-08, not a claim about today.** The advertised
/// list has grown well past 25 since, and every fragment that arrived with it arrived the same way round;
/// what the file asserts about the present is the *empty list* below and the enumerated one further down,
/// never a total. A count in a comment is exactly what went stale here twice — the second time while this
/// very paragraph sat beside a green assertion saying otherwise.
///
/// The pin stays, and it now guards the one remaining direction: a **newly advertised** method would join
/// this list silently, arriving on the wire with nothing checking its result shape and nothing saying so.
/// An empty expectation makes that failure loud on the first run.
const UNCOVERED_METHODS: &[&str] = &[];

#[test]
fn the_schema_covers_every_method_we_advertise_and_the_uncovered_list_is_pinned_empty() {
    let advertised: Vec<&str> = METHODS.iter().map(|m| m.name).collect();
    let schematized = schemas().methods_with_result();

    let mut covered: Vec<&str> = advertised
        .iter()
        .copied()
        .filter(|m| schematized.contains(m))
        .collect();
    let mut uncovered: Vec<&str> = advertised
        .iter()
        .copied()
        .filter(|m| !schematized.contains(m))
        .collect();
    covered.sort_unstable();
    uncovered.sort_unstable();

    // Schematized but not advertised: harmless in that direction — D4 makes the advertised list
    // authoritative — but worth printing, because it is the shape of a method we might be missing.
    let schema_only: Vec<&str> = schematized
        .iter()
        .copied()
        .filter(|m| !advertised.contains(m))
        .collect();

    println!("--- Aether wire-schema coverage (contract §8 item 15) ---");
    println!(
        "advertised methods: {}   result schema present: {}   absent: {}",
        advertised.len(),
        covered.len(),
        uncovered.len()
    );
    println!("  COVERED   ({}): {}", covered.len(), covered.join(", "));
    println!(
        "  UNCOVERED ({}): {}",
        uncovered.len(),
        uncovered.join(", ")
    );
    println!(
        "  schematized but not advertised ({}): {}",
        schema_only.len(),
        schema_only.join(", ")
    );
    println!(
        "  events with a params schema ({}): {}",
        schemas().events_with_params().len(),
        schemas().events_with_params().join(", ")
    );
    println!(
        "  => envelope coverage 100% of lines; result coverage {}/{} methods. \
         Every line is checked against `anyMessage`; a reply to an UNCOVERED method would get the \
         envelope and nothing more — there are {} of those.",
        covered.len(),
        advertised.len(),
        uncovered.len()
    );

    // The registry prints HERE, beside the coverage split, and not in a test of its own — because the
    // failure mode being guarded against is someone reading a green suite as "fully conformant". A
    // number that says what is checked has to sit next to the list of what is checked *differently*.
    println!(
        "  KNOWN CONTRACT DIVERGENCES ({}) — registered, ruling-pending, NOT conformant:",
        KNOWN_CONTRACT_DIVERGENCES.len()
    );
    for line in divergence_report() {
        println!("    {line}");
    }
    // The closing claim adapts to the registry, because a fixed sentence is how a report starts lying.
    // With divergences registered, green means only "nothing UNregistered". With none, the claim is
    // genuinely stronger — but it is still a claim about *shapes*, and D14 puts behaviour under the
    // prose, so it must not be read as conformance to §8 as a whole.
    if KNOWN_CONTRACT_DIVERGENCES.is_empty() {
        println!(
            "  => no registered divergences: every advertised method's replies match the contract's \
             own wire shapes, closed against their fragments (§8 item 20), so an unknown key is a red \
             test rather than a shape nobody sampled. This is NOT the same as conformance to §8: D14 \
             puts BEHAVIOUR under the prose, and the sharpest live example is that `reason: \"step\"` \
             for a completed run_frames would pass everything here (see the item-13 test below). Two \
             shape-level holes also remain, both pinned as open: `anyMessage` is a oneOf over both wire \
             directions, and the closure is top-level only."
        );
    } else {
        println!(
            "  => this server is NOT fully schema-conformant. A green suite means \"no UNREGISTERED \
             divergences\", which is a weaker and more useful claim — and since §8 item 20 landed it is \
             a much sharper one: every result is closed against its fragment, so an unknown key is a \
             red test rather than a shape nobody sampled."
        );
    }

    let mut expected_uncovered = UNCOVERED_METHODS.to_vec();
    expected_uncovered.sort_unstable();
    assert_eq!(
        uncovered, expected_uncovered,
        "the set of methods with no `result` schema changed.\n\
         If a method was ADDED to `engine::METHODS`, it joined the unchecked pile — add it to \
         UNCOVERED_METHODS deliberately, or (better) get a fragment into the contract schema first.\n\
         If a schema fragment was COMPLETED, remove that method from UNCOVERED_METHODS in the same \
         commit that re-vendors the schema."
    );
    assert_eq!(
        covered.len() + uncovered.len(),
        advertised.len(),
        "every advertised method is in exactly one bucket"
    );

    // **The other direction — and the decision the 58-fragment re-vendor owed it.**
    //
    // For most of this file's life `schema_only` was non-empty by design — fragments landed ahead of
    // their handlers, which is the order §8 item 20 wants — so it was printed and not asserted. Serving
    // the CRAM pair emptied it, and `assert!(schema_only.is_empty())` was written to stop an empty list
    // going back to non-empty silently. Its own comment said the rule was that a fragment ahead of its
    // handler "has to be a decision, taken in the commit that re-vendors". empyrean's §9
    // mechanical-completion pass added 21 such fragments, so this is that commit, and this is that
    // decision. It is written down here rather than in a doc because this is the assertion that will ask
    // the question again.
    //
    // **The decision: schematized-but-unadvertised is a legitimate steady state here, and the ones listed
    // below are not deferred work.** Three grounds, in order of weight.
    //
    //  1. **The contract says the fragment comes first.** §8 item 20 makes a fragment the *precondition*
    //     for a handler and not its record; the schema's own description says the CRAM pair was
    //     "schematized first on purpose" for exactly that reason. A gate that turns the contract's
    //     required order into a failure is punishing the artifact for being correct.
    //  2. **They are not this server's rows — yet.** §6 is the suite catalog, not our backlog. Every name
    //     still listed is served today by `oracle-old` and by no route in this process — no `METHODS`
    //     entry, no handler symbol, no cargo feature (this crate has no `[features]` at all), no runtime
    //     toggle. The 2026-08-22 dry run enumerated all seven routes that could produce a reply and found
    //     the blocker on each. Eight are governed by a capability flag this server publishes as `false`,
    //     which is the contract's own way of saying "not here".
    //
    //     *"Yet" is now load-bearing.* These 21 became this repo's acceptance contract — what the successor
    //     must serve before it can replace the legacy server — and the `step*` trio has since been served
    //     and removed from the list below. So the ground is "not served here today", never "never ours",
    //     and the set is expected to shrink one shipped handler at a time.
    //  3. **The failure it was written to catch is a different failure.** It was aimed at *our* rows
    //     sitting unserved after we accepted them (§11.13's, §11.14's). Nothing about these 21 was
    //     accepted here.
    //
    // **What it does NOT become: a printed count.** A report says "21" and goes green forever; a 22nd
    // fragment for a method we never serve — the exact original failure — would then print `22` and pass.
    // Worse, a bare `is_empty()` and a bare count share the vacuous shape this repo has been bitten by:
    // both are satisfied by a schema that failed to load, parsed to nothing, or was silently empty. `0`
    // and `not checked` are the same observation.
    //
    // So it becomes a **pinned set**, the shape `UNCOVERED_METHODS` above already uses, and it is
    // strictly louder than what it replaces in all three directions:
    //
    //   * a fragment arriving for one more unserved method → red (the original purpose, kept);
    //   * one of the listed names becoming served → red, forcing its removal in the commit that ships it,
    //     which is the half `is_empty()` could never have caught — and which is exactly what happened to
    //     the `step*` trio on 2026-08-22;
    //   * an empty or unparsed schema → red, because the expectation is a literal set of names and not
    //     zero. This is the property the brief demands: the check cannot be satisfied by not running.
    //
    // The list is a *decision record*, so it is literal on purpose — the same reasoning as
    // `UNCOVERED_METHODS`, where the point is that the number cannot quietly improve. What must not be
    // literal is any claim about the names, so the two claims the pin implies are re-derived below from
    // the schema and from `METHODS` rather than trusted: each pinned name has a fragment, and each is
    // genuinely unadvertised. A typo or a stale entry cannot survive that.
    const SCHEMATIZED_NOT_ADVERTISED: &[&str] = &[
        "emulator/audio_spectrum",
        // The five breakpoint rows LEFT this set on 2026-08-27, together, by being SERVED. They were
        // pinned here on 2026-08-26 with the reason "the breakpoint family is governed by
        // `capabilities.breakpoints`, which this server publishes `false`, so all five rows are absent
        // from `METHODS` together" — that capability now publishes `true` and all five are advertised, so
        // the reason retired with the entries. This assertion is what forced the pin to be edited in the
        // commit that shipped the handlers, which is the direction its second bullet exists for.
        "emulator/get_channel_states",
        "emulator/log_clear",
        "emulator/ping",
        "emulator/set_channel_enabled",
        // `emulator/step`, `emulator/step_out` and `emulator/step_over` were here until 2026-08-22, and
        // `emulator/run_to_scanline` left the same day — the first four of the 21 to leave the set by being
        // SERVED, which is the direction the pin's second bullet was written for and the one `is_empty()`
        // could never have caught. `emulator/get_layer_states` and `emulator/set_layer_enabled` left the
        // same way on 2026-08-26, making six. `emulator/write_vram` left on 2026-08-27, making seven —
        // its fragment is FIRST-FRAGMENT-transcribed (three registered D-16 absences, served as written),
        // and this assertion is what forced the pin to be edited in the commit that shipped the handler.
        // Each removal was forced by this assertion going red on the commit that shipped the handler, not
        // remembered afterwards. The set also GREW by one on 2026-08-26 —
        // `emulator/breakpoint_set_enabled` — which is the direction the pin's first bullet was written
        // for, and it left again on 2026-08-27 with the other four when the family was served. The names
        // in this literal are the only count worth having, and the three grounds above still hold for
        // every one.
        "emulator/vgm_start",
        "emulator/vgm_status",
        "emulator/vgm_stop",
    ];

    let mut expected_schema_only = SCHEMATIZED_NOT_ADVERTISED.to_vec();
    expected_schema_only.sort_unstable();
    let mut schema_only_sorted = schema_only.clone();
    schema_only_sorted.sort_unstable();
    assert_eq!(
        schema_only_sorted, expected_schema_only,
        "the set of schematized-but-unadvertised methods changed.\n\
         A fragment ARRIVED for a method we do not serve: decide it in the re-vendor commit — serve it, \
         or add it here with the reason, exactly as the 21 above were decided.\n\
         A method LEFT the set because we now serve it: remove it here in the same commit that ships the \
         handler.\n\
         Neither is a conformance failure (D4 makes the advertised list authoritative); both are \
         decisions that must be taken deliberately rather than noticed later."
    );

    // The two claims the pin implies, re-derived rather than trusted — so a typo or a stale name in the
    // list above cannot sit there looking like a decision.
    for m in SCHEMATIZED_NOT_ADVERTISED {
        assert!(
            schematized.contains(m),
            "{m} is pinned as schematized-but-unadvertised, but the vendored schema has no fragment for \
             it — the pin names something that does not exist"
        );
        assert!(
            !advertised.contains(m),
            "{m} is pinned as UNadvertised but appears in engine::METHODS — this server serves it, so \
             the pin is stale and must be removed in the commit that shipped the handler"
        );
    }

    // **Every fragment in the document reaches the coverage split — the one blind spot the pins leave.**
    //
    // The first thing written here was `covered.len() + schema_only.len() == schematized.len()`, and it
    // was deleted for being **tautological**: those two sets partition `schematized` by construction, so
    // the identity holds however few fragments `schematized` contains. An assertion that cannot fail is
    // worse than no assertion, because it reads like coverage.
    //
    // The real gap is one level further back. `schematized` is built from fragments that declare a
    // `result`, and every bucket in this test derives from it — so a fragment **without** a `result` is
    // invisible to all of them. Trace it: it is not in `schematized`, therefore not in `covered`, not in
    // `uncovered` and not in `schema_only`, so the UNCOVERED pin, the schematized-but-unadvertised pin
    // and the per-name checks above are all satisfied while a fragment nobody has looked at sits in the
    // document. That is exactly the arrival this test exists to make loud.
    //
    // The other routes into `schematized` shrinking are already covered earlier and this check is
    // deliberately not a second guard on them: an *advertised* method losing its `result` trips the
    // UNCOVERED pin, and a *pinned* one losing it trips the pin — both were confirmed by making them
    // fail. What is left, and what this catches, is a fragment that is neither advertised nor pinned and
    // carries no `result`. Proven red by adding exactly that fragment to a scratch copy of the schema and
    // watching every assertion above stay green.
    //
    // The population is re-derived straight off the raw document — every non-`$` key under `methods` —
    // on every run, never a literal.
    let fragment_names: Vec<&str> = common::schema::schema_root()["methods"]
        .as_object()
        .expect("the vendored schema has a methods object")
        .keys()
        .filter(|k| !k.starts_with('$'))
        .map(String::as_str)
        .collect();
    let unsplit: Vec<&str> = fragment_names
        .iter()
        .copied()
        .filter(|n| !schematized.contains(n))
        .collect();
    assert!(
        unsplit.is_empty(),
        "{} fragments in the vendored schema declare no `result` and so reach neither bucket of this \
         coverage split: {unsplit:?}.\n\
         Every assertion in this test — the UNCOVERED pin and the schematized-but-unadvertised pin \
         alike — is blind to them. Either the fragment is incomplete upstream, or `methods_with_result` \
         is not reading the whole document.",
        unsplit.len()
    );
}

// ---------------------------------------------------------------------------------------------------
// 3. The divergence registry — kept live, so it cannot rot after a ruling
// ---------------------------------------------------------------------------------------------------

#[test]
fn every_registered_divergence_is_still_live() {
    // **The anti-rot property, and the reason the registry is a list of canonical MESSAGES rather than a
    // list of method names.** Each entry claims that the schema, as vendored, rejects a specific shape
    // this server puts on the wire. This test checks the claim both ways round:
    //
    //   * the schema must STILL reject it with no allowances — so when a CR is ruled on and the schema
    //     re-vendored, this goes red and the entry must be deleted in the same commit;
    //   * the harness must ACCEPT it with allowances — so the entry is actually doing the job it exists
    //     for, and is not a dead line of documentation next to an allowance that fires somewhere else.
    //
    // Liveness is keyed to the schema rather than to observed traffic on purpose. Counting live firings
    // would only see the messages the binary that owns the counter happened to produce — precisely the
    // sampling weakness that let CR-14 through a 33-message probe of a live server.
    //
    // The non-emptiness assertion that used to stand here is gone, deliberately: its own comment asked
    // for exactly that ("if the last divergence was ruled on and removed, delete this assertion"), CR-14's
    // ruling is the event it was written for, and a registry that is empty for a day should not have to
    // fake an entry to keep a test honest. The anti-vacuity job it was doing has moved somewhere it holds
    // whether the list is empty or not: `the_strict_closure_rejects_a_surplus_key_and_needs_the_…`
    // proves the validator still bites. (The list is not empty today — CR-16 landed the same afternoon.)
    for d in KNOWN_CONTRACT_DIVERGENCES {
        let (line, method) = (d.canonical)();

        let strict = check_incoming_strict(&line, method);
        assert!(
            strict.is_err(),
            "{} ({} {}) NO LONGER DIVERGES: the vendored schema now accepts the shape this entry was \
             registered for. That is good news — a ruling landed — but the entry is now a lie. Delete \
             it from KNOWN_CONTRACT_DIVERGENCES, delete its allowance in common::schema, and close the \
             CR in docs/2026-08-14-aether-change-requests.md.\n  canonical: {line}",
            d.cr,
            d.method,
            d.path
        );

        // A weak locator, and labelled as one rather than dressed up. For a divergence the schema
        // reports at a key (`/otherMatches`) this really does check the entry describes the right bug.
        // For one that falls out at the root `oneOf` (CR-15) the failure text embeds the whole instance,
        // so the key name is present either way and this proves little. The load-bearing assertions are
        // the two either side of it; this one only catches an entry whose path is outright unrelated.
        // An entry may name several keys (`"$.region, $.symbolDisp, $.caveat"`), and EVERY one of them
        // must show up — a divergence that listed three keys and tripped over one would otherwise pass
        // while two-thirds of it went unproven.
        let failures = strict.unwrap_err();
        for key in d.path.split(',').map(|k| k.trim().trim_start_matches("$.")) {
            assert!(
                failures.iter().any(|f| f.contains(key)),
                "{} ({} {}) diverges, but `{key}` appears nowhere in the failure — the entry may be \
                 describing one bug while its canonical message trips over another.\n  failures: {failures:#?}",
                d.cr,
                d.method,
                d.path
            );
        }

        check_incoming(&line, method).unwrap_or_else(|e| {
            panic!(
                "{} ({} {}) is registered but its allowance does not actually admit its own canonical \
                 message — the entry and the allowance have drifted apart.\n  failures: {e:#?}",
                d.cr, d.method, d.path
            )
        });
    }
}

#[test]
fn every_registered_divergence_names_a_real_change_request() {
    // A CR number that does not exist in the ledger makes the registry unauditable: the whole promise is
    // "registered, not silenced", and an entry pointing at nothing is silenced with extra steps.
    let ledger = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/2026-08-14-aether-change-requests.md"),
    )
    .expect("the change-request ledger must be readable from the crate");
    for d in KNOWN_CONTRACT_DIVERGENCES {
        assert!(
            ledger.contains(&format!("## {} —", d.cr)),
            "{} is registered in KNOWN_CONTRACT_DIVERGENCES but has no `## {} —` section in \
             docs/2026-08-14-aether-change-requests.md",
            d.cr,
            d.cr
        );
    }
}

// ---------------------------------------------------------------------------------------------------
// 4. Anti-vacuity controls — proof the validator rejects
// ---------------------------------------------------------------------------------------------------

/// Assert the harness rejects `line`, and that some failure message mentions `needle`.
///
/// Naming the field is the point: an error that says only "invalid" would not distinguish a validator
/// that found the planted defect from one that rejects everything, and the positive control below is
/// what rules the latter out.
fn rejects(line: &Value, method: Option<&str>, needle: &str) -> Vec<String> {
    match check_incoming(line, method) {
        Ok(()) => panic!(
            "THE VALIDATOR ACCEPTED A MESSAGE IT MUST REJECT (expected a failure mentioning \
             `{needle}`). A validator that accepts everything is a vacuous assertion: the suite would \
             be green and the wire unchecked.\n  line: {line}"
        ),
        Err(failures) => {
            assert!(
                failures.iter().any(|f| f.contains(needle)),
                "rejected, but no failure mentions `{needle}` — the validator may be failing for an \
                 unrelated reason, which would make this control prove nothing.\n  failures: {failures:#?}"
            );
            failures
        }
    }
}

/// A well-formed `emulator/read_memory` reply, used as the base for the planted defects below.
fn good_read_memory_reply() -> Value {
    // `region` is REQUIRED as of the CR-16 re-vendor (`empyrean` `d45dc87`, §11.6) — §6's row lists it
    // un-parenthesised, and the server has always emitted it. This fixture omitted it and so stopped
    // being conformant the moment the fragment declared it, which is the positive control doing its job
    // on itself: a "conformant reply" fixture that drifts from the contract silently weakens every
    // rejection control underneath it.
    json!({"jsonrpc":"2.0","id":7,"result":{
        "addr":"0x00FFA144","len":4,"bytes":"0x02600000","symbol":"Camera_X","region":"work RAM",
        "frame":601,"mclk":538008040,"running":false,"droppedEvents":0}})
}

#[test]
fn positive_control_a_conformant_message_is_accepted() {
    // Without this, every control below is satisfied by a validator that rejects unconditionally.
    check_incoming(&good_read_memory_reply(), Some("emulator/read_memory"))
        .expect("a conformant reply must pass");
    check_incoming(
        &json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
            "reason":"runFrames","pc":"0x00012A4C","stopPrecision":"exact","frames":1,
            "deadlineReached":true,"frame":1,"mclk":896040,"running":false}}),
        None,
    )
    .expect("a conformant event must pass");
}

#[test]
fn control_a_reply_without_droppedevents_is_rejected() {
    // §2.3 / D17 / §8 item 18: present even at zero — "0 is the answer, not the absence of one".
    let mut line = good_read_memory_reply();
    line["result"]
        .as_object_mut()
        .unwrap()
        .remove("droppedEvents");
    rejects(&line, Some("emulator/read_memory"), "droppedEvents");
}

#[test]
fn control_a_numeric_checkpoint_id_is_rejected() {
    // A REGRESSION GUARD, not a synthetic one: this exact shape was live in this tree until 2026-08-15,
    // and it is the one thing the probe found the live wire actually violating (F1). §8 item 16 / D9
    // category 4: an opaque handle is a JSON string.
    //
    // Note this control also proves the *keying* works. The envelope alone accepts it — `replyFields`
    // says nothing about a key called `id` inside `result` — so it can only be caught by having reached
    // for `methods["emulator/checkpoint"].result`, which means `recv` correctly attributed the reply to
    // the request that asked for it.
    let line = json!({"jsonrpc":"2.0","id":3,"result":{
        "id":1,"bytes":262144,"frame":10,"mclk":8960400,"running":false,"droppedEvents":0}});
    check_incoming(&line, None).expect("the envelope alone accepts it — that is the point");
    rejects(&line, Some("emulator/checkpoint"), "id");
}

/// A well-formed `emulator/watchpoint_hits` reply carrying one **bus** hit, used as the base for the
/// planted defects below.
fn good_hits_reply(hit: Value) -> Value {
    json!({"jsonrpc":"2.0","id":11,"result":{
        "hits":[hit],"total":1,"returned":1,"limit":100,"truncated":false,
        "dropped":0,"seen":42,"matched":1,
        "frame":3,"mclk":2688120,"running":false,"droppedEvents":0}})
}

fn good_bus_hit() -> Value {
    json!({"watch":"w0","space":"bus","addr":"0x00FF8000","value":"0x00001234","size":2,
           "op":"write","fc":5,"via":"bus","pc":"0x000002A0","frame":1,"mclk":896040,"seq":0})
}

#[test]
fn control_a_bus_hit_carrying_old_and_a_vdp_hit_missing_it_are_both_rejected() {
    // **The structural presence rule, both directions.** `Watchpoints::on_event` builds every bus hit with
    // `old: 0` unconditionally — the 68000 bus event stream carries no prior value — so a bus hit that
    // reports `old` is asserting something false, and it would defeat the one exact per-write change test
    // this instrument offers. A VDP-internal write has the prior value and has no bus function code.
    check_incoming(
        &good_hits_reply(good_bus_hit()),
        Some("emulator/watchpoint_hits"),
    )
    .expect("the positive control first: a conformant bus hit must pass");

    let mut hit = good_bus_hit();
    hit["old"] = json!("0x00000000");
    rejects(
        &good_hits_reply(hit),
        Some("emulator/watchpoint_hits"),
        "old",
    );

    // And the mirror: a VDP hit must carry `old` and must not carry `fc`.
    let vdp = json!({"watch":"w0","space":"vram","addr":"0x00000100","value":"0x000000BE",
                     "old":"0x00000000","size":1,"op":"write","via":"direct","pc":"0x00000216",
                     "frame":0,"mclk":0,"seq":0});
    check_incoming(
        &good_hits_reply(vdp.clone()),
        Some("emulator/watchpoint_hits"),
    )
    .expect("a conformant VDP hit must pass");
    let mut missing_old = vdp.clone();
    missing_old.as_object_mut().unwrap().remove("old");
    rejects(
        &good_hits_reply(missing_old),
        Some("emulator/watchpoint_hits"),
        "old",
    );
    let mut with_fc = vdp;
    with_fc["fc"] = json!(0);
    rejects(
        &good_hits_reply(with_fc),
        Some("emulator/watchpoint_hits"),
        "fc",
    );
}

#[test]
fn control_a_numeric_watch_handle_is_rejected_wherever_it_appears() {
    // §8 item 16's mistake, generalised: this server shipped a numeric checkpoint handle once, and the
    // watch handle appears in five places. Two of them are checked here — the rest are covered
    // behaviourally by `tests/watchpoints.rs`.
    let mut hit = good_bus_hit();
    hit["watch"] = json!(0);
    rejects(
        &good_hits_reply(hit),
        Some("emulator/watchpoint_hits"),
        "watch",
    );

    let line = json!({"jsonrpc":"2.0","id":12,"result":{
        "watch":0,"space":"bus","addr":"0x00FF8000","len":2,"op":"write","mode":"record",
        "frame":0,"mclk":0,"running":false,"droppedEvents":0}});
    rejects(&line, Some("emulator/watchpoint_add"), "watch");
}

#[test]
fn control_a_hits_reply_missing_an_honesty_counter_is_rejected() {
    // §8 item 21: `seen`, `dropped` and `matched` are REQUIRED beside §2.4's `total`/`returned`/
    // `truncated`. `seen` is the one that matters most — without it a client cannot tell "this address is
    // never written" from "the recorder was not in the run" — so a reply that omits it is rejected rather
    // than read as a zero.
    for key in [
        "seen",
        "dropped",
        "matched",
        "total",
        "returned",
        "truncated",
    ] {
        let mut line = good_hits_reply(good_bus_hit());
        line["result"].as_object_mut().unwrap().remove(key);
        rejects(&line, Some("emulator/watchpoint_hits"), key);
    }
}

#[test]
fn control_a_watchpoint_stop_that_does_not_name_its_watch_is_rejected() {
    // Unlike CR-9's `buttons`/`port`, this rule HAS a discriminator in the event, so the schema enforces
    // both halves of it: a `watchpoint` stop must name its watch, and no other stop may.
    let missing = json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
        "reason":"watchpoint","pc":"0x000002A0","stopPrecision":"afterCommit","deadlineReached":false,
        "frame":1,"mclk":896040,"running":false}});
    rejects(&missing, None, "watch");

    let spurious = json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
        "reason":"runFrames","pc":"0x000002A0","stopPrecision":"exact","frames":1,"deadlineReached":true,"watch":"w0",
        "frame":1,"mclk":896040,"running":false}});
    rejects(&spurious, None, "watch");
}

#[test]
fn control_buttons_without_port_is_rejected_and_that_is_all_the_schema_can_do() {
    // **CR-9's cost, made executable.** `dependentRequired` catches the half-attribution — a subscriber
    // told which buttons went down and not which pad would blame the wrong controller in a two-pad
    // session — and that is the ONLY half a schema can reach. The other half (present iff `press` drove
    // the advance) has no discriminator in the event, deliberately: `reason` names the stop condition and
    // never the driving method, so a press-driven advance and a `run_frames` one both read `runFrames`.
    // The second assertion below is that gap, asserted rather than assumed, so nobody later reads the
    // schema as enforcing more than it does. Its behavioural half is
    // `watchpoints::press_stops_carry_buttons_and_port_and_run_frames_does_not`.
    let half = json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
        "reason":"runFrames","pc":"0x000002A0","stopPrecision":"exact","frames":2,"deadlineReached":true,
        "buttons":["start"],"frame":2,"mclk":1792080,"running":false}});
    rejects(&half, None, "port");

    let fabricated = json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
        "reason":"runFrames","pc":"0x000002A0","stopPrecision":"exact","frames":2,"deadlineReached":true,
        "buttons":["start"],"port":0,"frame":2,"mclk":1792080,"running":false}});
    check_incoming(&fabricated, None).expect(
        "a run_frames stop wearing buttons/port is SCHEMA-VALID — the event carries no method \
         discriminator, so this rule is behavioural and is pinned in tests/watchpoints.rs",
    );
}

#[test]
fn control_a_stopped_event_with_an_unknown_reason_is_rejected() {
    // §3's reason enum is closed. A server may not widen it unilaterally (§8).
    let line = json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
        "reason":"frameAdvance","pc":"0x00012A4C","stopPrecision":"exact","frame":1,"mclk":896040,"running":false}});
    rejects(&line, None, "reason");
}

#[test]
fn control_an_envelope_with_both_result_and_error_is_rejected() {
    // §2: success is signalled by the presence of `result`; an error response carries `error`. Both at
    // once tells a client two different things about one call.
    let line = json!({"jsonrpc":"2.0","id":9,
        "result":{"frame":1,"mclk":2,"running":false,"droppedEvents":0},
        "error":{"code":-32603,"message":"internal","data":{"frame":1,"mclk":2,"running":false,"droppedEvents":0}}});
    // This one names the keyword rather than a field, and necessarily so: `successResponse` and
    // `errorResponse` both close `additionalProperties`, so the message matches NEITHER arm and the
    // failure is reported at the root `oneOf` of `anyMessage`. Asserting on "oneOf" is asserting that
    // the envelope discriminant did its job.
    let failures = rejects(&line, None, "oneOf");
    assert!(
        failures.iter().any(|f| f.starts_with("anyMessage:")),
        "the failure must come from the envelope check: {failures:#?}"
    );
}

#[test]
fn control_an_error_without_data_is_rejected() {
    // §2 / §2.2 / §2.3: `error.data` is ALWAYS present, because it carries the stamp and the counter.
    // This is the arm of the funnel that reaches `$defs/errorObject` — it needs no separate dispatch,
    // `anyMessage` -> `errorResponse` -> `errorObject` gets there on its own.
    let line =
        json!({"jsonrpc":"2.0","id":11,"error":{"code":-32004,"message":"address out of range"}});
    rejects(&line, None, "oneOf");

    // And the stamp inside `data` is not optional either.
    let line = json!({"jsonrpc":"2.0","id":11,"error":{
        "code":-32004,"message":"address out of range","data":{"addr":"0x0","droppedEvents":0}}});
    rejects(&line, None, "oneOf");
}

#[test]
fn control_a_hex_field_that_is_not_hex_is_rejected() {
    // `$defs/hex` (D9 category 1) is a pattern, not just "a string": an address that lost its `0x` is a
    // client-side parse bug waiting to happen.
    let mut line = good_read_memory_reply();
    line["result"]["addr"] = json!("00FFA144");
    rejects(&line, Some("emulator/read_memory"), "addr");
}

// ---------------------------------------------------------------------------------------------------
// The blind spot, recorded as an executable fact
// ---------------------------------------------------------------------------------------------------

#[test]
fn the_schema_cannot_express_section_8_item_13_and_this_test_proves_it() {
    // Probe finding F2, promoted from a comment to an assertion so it cannot quietly stop being true.
    //
    // A completed `run_frames` mislabelled `reason:"step"` PASSES the schema, because `step` is a legal
    // member of the enum. §3 pins `step` as "one instruction, or one instruction-shaped unit … not the
    // value for a frame advance" — that is BEHAVIOUR, and D14 puts behaviour under the prose, not the
    // schema. So of the two mechanical conformance items in this arc the validator catches one (item 16,
    // the checkpoint id, controlled above) and is blind to the other by construction.
    //
    // The rule itself is asserted behaviourally in `tests/events.rs`
    // (`a_completed_run_frames_reports_runframes_not_step`, `a_press_reports_runframes_…`). If this test
    // ever starts FAILING, the schema grew a way to express item 13 and those tests have a mechanical
    // backstop they did not have before — which is good news, and means this test should be deleted and
    // the doc updated.
    let mislabelled = json!({"jsonrpc":"2.0","method":"emulator/stopped","params":{
        "reason":"step","pc":"0x00012A4C","stopPrecision":"exact","frames":8,"deadlineReached":true,
        "frame":8,"mclk":7168320,"running":false}});
    assert!(
        check_incoming(&mislabelled, None).is_ok(),
        "the schema now rejects a mislabelled run_frames stop — see this test's comment"
    );
}

#[test]
fn the_null_id_allowance_is_exactly_as_wide_as_json_rpc_2_0_requires_and_no_wider() {
    // FOUND BY THIS VALIDATOR, not by the probe — the probe's 33-message run drove six error paths, all
    // of which had a parseable request and therefore a real id, so it never saw a null one.
    //
    // JSON-RPC 2.0 §5 MANDATES `"id": null` when the id could not be detected, and our server obeys:
    // three handshake tests (invalid JSON, a batch, an over-long line) all produce it. `$defs/id` was
    // `["integer","string"]` and rejected it — a spec bug under D14, raised as CR-15, and originally
    // carried here as a narrow harness allowance.
    //
    // **CR-15 was ruled and the contract amended the same day** (`empyrean` `protocol.md` §11.4), at
    // exactly this width: `errorResponse.id` is nullable, narrowed by an `if`/`then` to the two codes
    // decided before a request object exists. So the four fences below are no longer OUR fences — they
    // are the schema's, and this test now checks that the schema's width and
    // `is_json_rpc_undetectable_id_error`'s still agree. If the contract ever widens (a nullable id on a
    // code that answers a parsed request), (a) goes red here rather than being discovered by a client
    // that cannot correlate its own failures.
    let parse_error = json!({"jsonrpc":"2.0","id":null,"error":{
        "code":-32700,"message":"invalid JSON",
        "data":{"frame":0,"mclk":0,"running":false,"droppedEvents":0}}});
    assert!(common::schema::is_json_rpc_undetectable_id_error(
        &parse_error
    ));
    check_incoming(&parse_error, None)
        .expect("the schema must accept the shape JSON-RPC 2.0 mandates");
    // ...and the *strict* verdict too: with CR-15 retired there is no allowance left to lean on, so the
    // mandated shape must pass with no allowances in play at all. This is what proves the entry was
    // retired because the contract moved, not because the harness got more permissive.
    check_incoming_strict(&parse_error, None)
        .expect("no allowance should be needed for this shape any more");

    // Now the fence, four sides of it. Each of these must still be rejected.

    // (a) A null id on a code that answers a request we DID parse. There is a real id to echo, so a
    //     null there is a correlation bug, not the protocol's mandate. The schema's `if`/`then` is what
    //     rejects this now.
    let mut wrong_code = parse_error.clone();
    wrong_code["error"]["code"] = json!(-32602);
    assert!(!common::schema::is_json_rpc_undetectable_id_error(
        &wrong_code
    ));
    rejects(&wrong_code, None, "oneOf");

    // (b) A null id on a SUCCESS. Nothing in JSON-RPC 2.0 permits it: a result always answers a request
    //     whose id was read. `$defs/id` was deliberately left unwidened, which is what keeps this shut.
    let null_id_success = json!({"jsonrpc":"2.0","id":null,"result":{
        "frame":0,"mclk":0,"running":false,"droppedEvents":0}});
    assert!(!common::schema::is_json_rpc_undetectable_id_error(
        &null_id_success
    ));
    rejects(&null_id_success, None, "oneOf");

    // (c) Legalising the null id does not legalise anything else about the message. A parse error that
    //     lost its `data` (and so its stamp and `droppedEvents`, §2.2/§2.3) is still caught.
    let mut no_data = parse_error.clone();
    no_data["error"].as_object_mut().unwrap().remove("data");
    assert!(common::schema::is_json_rpc_undetectable_id_error(&no_data));
    rejects(&no_data, None, "oneOf");

    // (d) ...and so is one whose `data` lost `droppedEvents`.
    let mut no_dropped = parse_error.clone();
    no_dropped["error"]["data"]
        .as_object_mut()
        .unwrap()
        .remove("droppedEvents");
    rejects(&no_dropped, None, "oneOf");
}

#[test]
fn the_strict_closure_rejects_a_surplus_key_and_needs_the_unevaluated_keyword_to_do_it() {
    // **Contract §8 item 20, and its own anti-vacuity control.** Two claims, and the second is the one
    // that was got wrong upstream before it was got right — so it is reproduced here rather than trusted.
    //
    // Claim 1: a conformant reply passes, and the same reply with one invented key does not. This is the
    // whole of item 20: "an unknown key on the wire is a change request, never a shipment".
    let good = good_read_memory_reply();
    check_incoming(&good, Some("emulator/read_memory")).expect("a conformant reply must pass");

    let mut surplus = good.clone();
    surplus["result"]["stoppedAtVibes"] = json!(7);
    rejects(&surplus, Some("emulator/read_memory"), "stoppedAtVibes");

    // Claim 2, the mechanics: `additionalProperties: false` would have rejected the CONFORMANT reply, not
    // the surplus one, because in draft 2020-12 it sees only its own `properties` and never those an
    // adjacent `allOf` contributes — and every fragment pulls the stamp (§2.2) and `droppedEvents` (§2.3)
    // in through `allOf: [{"$ref": "#/$defs/replyFields"}]`. Asserting this here means the harness cannot
    // be "simplified" to the obvious keyword without a red test explaining why not.
    let mut fragment = common::schema::schema_root()["methods"]["emulator/read_memory"]["result"]
        .as_object()
        .expect("the fragment is an object")
        .clone();
    fragment.insert(
        "$schema".into(),
        json!("https://json-schema.org/draft/2020-12/schema"),
    );
    fragment.insert(
        "$defs".into(),
        common::schema::schema_root()["$defs"].clone(),
    );

    let mut wrong = fragment.clone();
    wrong.insert("additionalProperties".into(), json!(false));
    let wrong = jsonschema::validator_for(&Value::Object(wrong)).expect("compiles");
    let envelope_fields: Vec<String> = wrong
        .iter_errors(&good["result"])
        .map(|e| e.to_string())
        .collect();
    assert!(
        !envelope_fields.is_empty(),
        "additionalProperties:false was expected to reject the conformant reply — if it no longer does, \
         the fragments stopped composing their envelope through `allOf` and item 20's note is stale"
    );
    for f in ["frame", "mclk", "running", "droppedEvents"] {
        assert!(
            envelope_fields.iter().any(|e| e.contains(f)),
            "additionalProperties:false must reject `{f}` — that is the defect item 20 documents. \
             errors: {envelope_fields:#?}"
        );
    }

    let mut right = fragment;
    right.insert("unevaluatedProperties".into(), json!(false));
    let right = jsonschema::validator_for(&Value::Object(right)).expect("compiles");
    assert!(
        right.is_valid(&good["result"]),
        "unevaluatedProperties:false must ACCEPT the conformant reply — it is the keyword that sees \
         across applicators, which is the whole reason item 20 names it"
    );
    assert!(
        !right.is_valid(&surplus["result"]),
        "...and must still catch the surplus key"
    );
}

#[test]
fn othermatches_is_the_bounded_container_the_contract_pins_and_carries_no_cursor() {
    // **What is left of the CR-14 fence, re-aimed at the ruled shape.** The test this replaces asserted
    // the *un-ruled* shape under an allowance, which was the honest thing to do while a ruling was
    // pending and is the wrong thing to do now that one has landed (`empyrean` `f309cc8`, §4 + §2.4).
    //
    // The history is worth keeping because of how the defect was found: `otherMatches` is emitted only on
    // the partial-match paths, and the 33-message probe that opened this arc called `lookup_symbol` with a
    // name resolving to nothing, so it only ever drove the error path. Probe finding F1 — "the only
    // schema-level failure on the live wire is the checkpoint id" — was a floor, not a result. That is the
    // sampling weakness §8 item 20 replaces with a gate.
    let ok = json!({"jsonrpc":"2.0","id":3,"result":{
        "query":"Play","exact":false,
        "otherMatches":{"items":[{"name":"Player_2","addr":"0x00FF8D4A"}],
                        "total":2,"returned":1,"limit":5,"truncated":true},
        "frame":0,"mclk":0,"running":false,"droppedEvents":0}});
    check_incoming(&ok, Some("emulator/lookup_symbol"))
        .expect("the ruled container shape must pass");

    // A bare array of strings — what the schema asked for BEFORE the ruling — is now rejected. CR-14 was
    // the first divergence where the server's shape was the better one, and this is the ruling landing.
    let mut bare = ok.clone();
    bare["result"]["otherMatches"] = json!(["Player_2"]);
    rejects(&bare, Some("emulator/lookup_symbol"), "otherMatches");

    // §2.4 clause (a): losing `truncated` is losing the one field the whole rule exists for.
    let mut lost = ok.clone();
    lost["result"]["otherMatches"]
        .as_object_mut()
        .unwrap()
        .remove("truncated");
    rejects(&lost, Some("emulator/lookup_symbol"), "truncated");

    // §2.4 clause (b) / §8 item 16: `lookup_symbol` accepts no continuation param, so a token it emits is
    // one the client can never hand back. The schema spells this `"cursor": false` / `"nextCursor": false`
    // rather than leaving it to prose, and both spellings are fenced.
    for token in ["cursor", "nextCursor"] {
        let mut with_token = ok.clone();
        with_token["result"]["otherMatches"][token] = json!(1);
        rejects(&with_token, Some("emulator/lookup_symbol"), token);
    }

    // One item shape, closed. A branch-specific extra key is exactly the "which branch am I on?" problem
    // §4 struck, and `items[]` closes with `additionalProperties` legally — that subschema has no `allOf`
    // for the keyword to be blind past.
    let mut odd_item = ok.clone();
    odd_item["result"]["otherMatches"]["items"][0]["rawName"] = json!("Player_2");
    rejects(&odd_item, Some("emulator/lookup_symbol"), "rawName");

    // The rest of the result is still the schema's business. `addr` is `$defs/hex`.
    let mut bad_addr = ok.clone();
    bad_addr["result"]["addr"] = json!(4290743546_u64);
    rejects(&bad_addr, Some("emulator/lookup_symbol"), "addr");
}

#[test]
fn anymessage_is_bidirectional_so_a_stray_id_on_an_event_slips_through() {
    // A SECOND blind spot, found by building the controls rather than by reading the schema — it was
    // written as a control that must reject, and it passed.
    //
    // `anyMessage` is a `oneOf` over BOTH directions of the wire (`request`, `successResponse`,
    // `errorResponse`, `notification`). `$defs/notification` does carry `"not": {"required": ["id"]}`,
    // but a line with `jsonrpc` + `id` + `method` + `params` is a perfectly legal `$defs/request` — so
    // an event that grew an `id` is accepted, because from the schema's side of the fence it is
    // indistinguishable from a client request that happened to travel the wrong way.
    //
    // Consequence for THIS harness, which is the part that matters: `check_incoming` keys event-params
    // validation off "has no `id`", so such a line would also skip `events.<name>.params` entirely and
    // be checked by the envelope alone. The server does not do this today (`tests/events.rs` asserts a
    // notification carries no id, directly), and closing it in the schema would need `anyMessage` split
    // into a client-to-server and a server-to-client half — a contract change, not a server one, and so
    // out of this slice by §8's ban on unilateral invention. Recorded, not worked around.
    let stray = json!({"jsonrpc":"2.0","id":4,"method":"emulator/stopped","params":{
        "reason":"pause","pc":"0x0","frame":0,"mclk":0,"running":false}});
    assert!(
        check_incoming(&stray, None).is_ok(),
        "`anyMessage` gained a direction — see this test's comment, and tell the contract"
    );

    // The narrower half IS enforced, and this is what keeps the observation honest: the bad `reason`
    // is caught the moment the line is attributable as an event, i.e. once the `id` is gone.
    let mut as_event = stray.clone();
    as_event.as_object_mut().unwrap().remove("id");
    as_event["params"]["reason"] = json!("frameAdvance");
    assert!(check_incoming(&as_event, None).is_err());
}
