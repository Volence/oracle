//! The frozen pin, checked against its own manifest — and **named in the suite's output**.
//!
//! # Why this file exists
//!
//! `fixtures/aeon/` is a set of committed bytes from another lane's build. Two failures are possible
//! and neither is visible from the test files that consume it:
//!
//! 1. **The bytes move without the record moving.** Someone drops a newer ROM in to make a red test go
//!    green — the one thing `PROVENANCE.md` forbids in terms — and nothing notices, because every
//!    consumer reads whatever is on disk. This file makes that a compile-free, hard failure: every
//!    artifact is hashed and compared to `PIN.tsv`, and the manifest must list exactly the files that
//!    are there.
//! 2. **A green is read as a statement about aeon's master.** It is not, and never was: it is a
//!    statement about one frozen chain. So this test *prints the chain per file*, which is the whole
//!    of its second job. A reader of the suite output can no longer fail to know which build passed.
//!
//! # What this is NOT
//!
//! It is not a currency check. It asks *"are the bytes here the bytes we recorded?"* — a **recovery**
//! question, correctly asked at the pinning revision, where the answer is a fact about this repository
//! alone. Whether aeon's master has moved past the pin is a **currency** question, must be asked at
//! sigil's **tip**, and deliberately lives outside the suite as a non-gating reporter:
//! `tools/aeon_pin_report.py`. A gate that reddens because someone else moved puts the whole gradient
//! behind bending our side until it goes green.
//!
//! # The path is the frozen directory, deliberately — not `ORACLE_AEON_DIR`
//!
//! The manifest describes *these committed bytes*. Under an `ORACLE_AEON_DIR` override the other tests
//! run against a live aeon build, and hashing that against this manifest would be a guaranteed red that
//! means nothing. So this test reads `fixtures/aeon/` directly and, when an override is set, says out
//! loud that the rest of the suite is NOT running against the pin it just named.
//!
//! # ⚠ A limit of the harness, and how it is worked around
//!
//! libtest **captures a passing test's stdout**, so under a plain `cargo test` the banner below is
//! printed and then swallowed — visible only on a failure, which is the one case where naming the pin
//! matters least. So the chain is named by running this file with `--nocapture` as its own step: the
//! `Name the frozen aeon pin` step in `.github/workflows/ci.yml`, and inside
//! `tools/replay_playthroughs.sh`. Both take well under a second. If you add a third place the suite
//! runs, name the pin there too, or that run's green says nothing about which build it passed against.

use std::path::PathBuf;

/// The committed fixture directory. Not `ORACLE_AEON_DIR` — see the module docs.
fn frozen_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/aeon"))
}

/// One row of `PIN.tsv`.
#[derive(Debug)]
struct PinRow {
    file: String,
    sha256: String,
    bytes: usize,
    chain: u32,
    sigil_freeze: String,
    aeon_rev: String,
    authority: String,
    upstream: String,
}

/// Parse `PIN.tsv`: `#` comments and blank lines dropped, first surviving line is the header.
///
/// Hand-rolled on purpose. `oracle-replay` carries exactly one dependency (`oracle-core`) as a stated
/// design property of the crate — a gate artifact another repo invokes by path should not drag a
/// dependency tree behind it — so a TOML crate for a six-row table is not a trade this crate makes.
fn read_manifest() -> Vec<PinRow> {
    let path = frozen_dir().join("PIN.tsv");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));
    let mut rows = Vec::new();
    let mut header_seen = false;
    for line in text.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(
            f.len(),
            8,
            "PIN.tsv rows are 8 tab-separated columns; got {} in `{line}`",
            f.len()
        );
        if !header_seen {
            assert_eq!(
                f,
                [
                    "file",
                    "sha256",
                    "bytes",
                    "chain",
                    "sigil_freeze",
                    "aeon_rev",
                    "authority",
                    "upstream"
                ],
                "PIN.tsv header changed — every reader of this file must be updated together"
            );
            header_seen = true;
            continue;
        }
        rows.push(PinRow {
            file: f[0].to_string(),
            sha256: f[1].to_string(),
            bytes: f[2].parse().expect("bytes column must be a number"),
            chain: f[3].parse().expect("chain column must be a number"),
            sigil_freeze: f[4].to_string(),
            aeon_rev: f[5].to_string(),
            authority: f[6].to_string(),
            upstream: f[7].to_string(),
        });
    }
    assert!(header_seen, "PIN.tsv has no header row");
    assert!(!rows.is_empty(), "PIN.tsv lists no artifacts");
    rows
}

// ---------------------------------------------------------------------------------------------------
// SHA-256. Implemented here rather than pulled in, for the crate's one-dependency rule; proved against
// the published FIPS 180-4 vectors below before it is trusted with anything.
// ---------------------------------------------------------------------------------------------------

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256_hex(data: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bitlen = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());

    for block in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, c) in block.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*c);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d) = (h[0], h[1], h[2], h[3]);
        let (mut e, mut f, mut g, mut hh) = (h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (dst, src) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *dst = dst.wrapping_add(src);
        }
    }
    h.iter().map(|w| format!("{w:08x}")).collect()
}

/// The hasher is proved before it is trusted, against the FIPS 180-4 published vectors.
///
/// The empty-input vector is the one worth naming: `e3b0c442…` is what a shell pipeline returns when
/// the command feeding it failed to stderr and hashed nothing at all. It has been mistaken for a real
/// artifact hash in this workspace more than once.
#[test]
fn the_sha256_used_by_this_gate_matches_the_published_vectors() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    // The 448-bit multi-block vector, so the padding path with a second block is exercised too.
    assert_eq!(
        sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    // 1,000,000 'a' — the vector that catches a length-counter that overflows or is byte-counted.
    assert_eq!(
        sha256_hex(&vec![b'a'; 1_000_000]),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

// ---------------------------------------------------------------------------------------------------
// The pin itself.
// ---------------------------------------------------------------------------------------------------

/// Every frozen artifact is the artifact `PIN.tsv` records, the manifest covers exactly the files that
/// are present, and **the chain each one sits at is printed**.
///
/// This is the test that makes "our pin moved and nobody said so" impossible, and it is the one place
/// the suite states what it is testing against.
#[test]
fn the_frozen_pin_matches_its_manifest_and_the_suite_names_the_chain() {
    let dir = frozen_dir();
    let rows = read_manifest();

    for r in &rows {
        let p = dir.join(&r.file);
        let bytes = std::fs::read(&p)
            .unwrap_or_else(|e| panic!("{} is listed in PIN.tsv but unreadable: {e}", p.display()));
        assert_eq!(
            bytes.len(),
            r.bytes,
            "{}: {} bytes on disk, PIN.tsv records {}",
            r.file,
            bytes.len(),
            r.bytes
        );
        let got = sha256_hex(&bytes);
        assert_eq!(
            got, r.sha256,
            "{} does NOT match the pin.\n  on disk  {got}\n  PIN.tsv  {}\nIf this artifact was moved \
             on purpose, PIN.tsv and PROVENANCE.md move with it, in the same commit. The pin never \
             moves to make a red test go green.",
            r.file, r.sha256
        );
    }

    // Completeness in both directions: a file dropped into this directory without a manifest row is a
    // pin nobody recorded, which is the same failure wearing the other hat.
    let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
        .expect("the frozen fixture directory must exist")
        .map(|e| {
            e.expect("readable entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| !n.ends_with(".md") && *n != "PIN.tsv")
        .collect();
    on_disk.sort();
    let mut listed: Vec<String> = rows.iter().map(|r| r.file.clone()).collect();
    listed.sort();
    assert_eq!(
        on_disk,
        listed,
        "PIN.tsv must list exactly the artifacts in {} (left: on disk, right: in the manifest)",
        dir.display()
    );

    // ---- the output half of this test's job ----
    let mut chains: Vec<u32> = rows.iter().map(|r| r.chain).collect();
    chains.sort_unstable();
    chains.dedup();
    let headline = if chains.len() == 1 {
        format!("aeon chain {}", chains[0])
    } else {
        let names: Vec<String> = chains.iter().map(|c| c.to_string()).collect();
        format!("MIXED — aeon chains {}", names.join(" and "))
    };
    println!("\n=== FROZEN AEON PIN: {headline} ===");
    println!(
        "  {:<15} {:>7}  {:<5} {:<9} {:<9} authority",
        "file", "bytes", "chain", "sigil", "aeon_rev"
    );
    for r in &rows {
        println!(
            "  {:<15} {:>7}  {:<5} {:<9} {:<9} {}",
            r.file,
            r.bytes,
            r.chain,
            r.sigil_freeze,
            &r.aeon_rev[..8.min(r.aeon_rev.len())],
            r.authority
        );
    }
    if chains.len() > 1 {
        println!(
            "  ^ MIXED PIN. Rows at the older chain are a dated gap, not a design property — see \
             fixtures/aeon/PROVENANCE.md."
        );
    }
    let upstreamless = rows.iter().filter(|r| r.upstream == "-").count();
    println!(
        "  {upstreamless} of {} rows have NO upstream counterpart (sigil freezes no listings), so \
         their currency is not measurable from sigil at all.",
        rows.len()
    );
    println!(
        "  A green here is a statement about these bytes ONLY. Whether aeon's master has moved past \
         them is a separate, non-gating question: tools/aeon_pin_report.py"
    );
    match std::env::var("ORACLE_AEON_DIR") {
        Ok(v) => println!(
            "  ⚠ ORACLE_AEON_DIR={v} is set — the REST of the suite is NOT running against the pin \
             named above.\n"
        ),
        Err(_) => println!("  The rest of the suite is running against these bytes.\n"),
    }
}
