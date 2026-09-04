//! **The boot read is bounded — gated here, because the ruling had no gate anywhere in this repo.**
//!
//! WHY THIS FILE EXISTS. The suite ruled on 2026-09-02T17:45:03Z that `docs/OVERSEER.md` — the file
//! every overseer session reads whole at boot — stays under the boot-read bound, and that **each
//! lane copies the check into its own gate**. That ruling measured this lane at 298,021 B and OVER.
//! Oracle split its file and never wired the check. The rule was therefore in force and unenforced,
//! which is precisely the state in which nothing can tell a reader whether the file is compliant:
//! an absent gate and a green gate produce the same suite output. The owner's later
//! 2026-09-02T18:20:19Z "cut the ceremony" ruling keeps it explicitly — *"The boot-read gate stays
//! (it is cheap and already built) but nobody trims by hand for it"* — so this is compliance with a
//! standing rule, not new instrument work.
//!
//! ⚠ AND THE BOOT DOC ALREADY SAID SO. `docs/OVERSEER.md:28` has carried the heading
//! **"## The boot read is bounded (100,000 B, gated)"** while nothing in this repo gated it — the
//! claim and the fact had come apart, and no artifact could tell them apart, which is the same
//! failure as a vacuous check wearing a green result. That heading becomes true at this commit,
//! which is why this parcel does not edit it.
//!
//! THE RULING, read at a committed revision and never through the sibling working-tree path (that
//! path is a peer's live tree and is not a citable revision):
//!
//! ```text
//! git -C ../empyrean fetch -q origin
//! git -C ../empyrean show origin/main:docs/OVERSEER-PROTOCOL.md   # "The boot read is bounded"
//! git -C ../empyrean show origin/main:docs/OVERSEER.md            # line 412, the 09-02T17:45:03Z ruling
//! ```
//!
//! **BYTES ONLY.** The protocol's own warning, verbatim: *"Judge by bytes. Unwrapping a
//! multi-kilobyte one-line bullet into prose RAISES the line count while cutting bytes ... so the
//! line half of the bound can move the wrong way under a correct fix."* So the line count is
//! reported as a **residual** and is never asserted. Anything gating on lines punishes the fix.
//!
//! **The bound is named ONCE.** It is stated in the protocol as prose, so it cannot be computed
//! from an artifact. Every expectation in this file — the verdict text, the fixtures, both
//! directions of the two-directional test — is derived from that one name. Nothing here re-types
//! the number.
//!
//! WHERE THIS LIVES AND WHY. It is a `tests/` file in `oracle-aether`, so `cargo test --workspace`
//! — the run this repo actually performs at every landing — picks it up with **zero registration**.
//! A gate placed where nobody runs it is the exact defect being fixed, and a new workspace member
//! would have needed a `members` entry, i.e. one more place it can be silently unwired (this
//! workspace already carries a deliberately `exclude`d crate). `oracle-aether` specifically because
//! it is already the home of this repo's suite-contract gates over documents rather than code
//! (`schema_conformance.rs` vendors and pins empyrean's schema; `mcp_tool_sweep.rs` walks docs).
//! `oracle-core` was rejected: its charter is "deterministic, no-I/O" emulation and this is repo
//! hygiene. The file needs no dependency beyond `std`, so no manifest changed.
//!
//! ⚠ If `docs/OVERSEER.md` is ever edited, [`RATCHET_BYTES`] moves with it — down freely, up only
//! with a loud statement of why. That is the ratchet, and it is the whole point.

use std::io::Write as _;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------------------------
// THE RULED BOUND — the one and only place the number is written.
//
// empyrean `docs/OVERSEER-PROTOCOL.md`, section "The boot read is bounded", read at `origin/main`
// on 2026-09-04: *"`docs/OVERSEER.md` is the boot read, and it stays under about 900 lines /
// 100 KB."* 100 KB is read as 100,000 bytes — the decimal reading, and the stricter of the two
// (the KiB reading would be 102,400). The hub's own 09-02T17:45:03Z measurements are stated
// against 100,000 B, which settles the reading.
//
// If the suite ever restates the bound, change it HERE and nowhere else.
const BOOT_READ_BOUND_BYTES: u64 = 100_000;

/// The protocol's "about 900 lines". **Reported beside the bytes, never asserted** — see the
/// module note on why gating this would punish a correct fix.
const BOOT_READ_LINES_GUIDE: usize = 900;

/// THE RATCHET, in force until the owner answers the suite-wide **card 7** (split the standing
/// rules into a second boot file, or raise the bound — one call for all six lanes).
///
/// ⚠ **Card 7 is NOT in this repo, and it is not this repo's queue item 7** (ours is closed and in
/// the log — a reader who goes looking locally finds the wrong thing). It is a card on the hub's
/// status. Read it at a committed revision:
///
/// ```text
/// git -C ../empyrean show origin/main:docs/OVERSEER-LOG.md   # 2026-09-04T12:43:53Z, 12:44:39Z, 13:26:52Z
/// ```
///
/// The 13:26:52Z entry is the ruling this file executes, and it carries oracle's own figure:
/// *"oracle installs the RATCHET form now, pinned at its measured size, failing on growth, printing
/// the distance to 100,000 B, aeon 882f79aa the reference; it decides nothing the owner is asked
/// and stops the regrowth."* Same entry: 128,776 B, "was 95,398 on 09-02, 118,762 at 10:5xZ ...
/// regrowth ~33 KB in two days" — which is the growth this ratchet exists to stop.
///
/// **Why this is not the ruled bound yet.** `docs/OVERSEER.md` is over 100,000 B and the residual
/// is live rulings interleaved with narrative, which the protocol names as *the owner's parcel*:
/// "report the residual bytes to him rather than trimming a ruling to hit a number; the bound
/// exists to make the boot read cheap, not to make rulings disappear." Asserting the ruled bound
/// today would fail this repo's build permanently on a question only he can answer.
///
/// **Why it is not report-only either.** A check that cannot fail is green by construction, so its
/// presence and its absence read the same — which is exactly the property that let the *missing*
/// gate go unnoticed here. Replacing an absent gate with an unfailable one changes what a reader
/// sees and not what is true. So the ratchet pins the MEASURED size, is failable today by growth,
/// and the distance to the ruled bound prints beside the verdict on every run, pass or fail.
///
/// **Measured on this worktree at 2026-09-04, byte-identical to `main:docs/OVERSEER.md`:**
/// 128,776 bytes, 1,550 lines.
///
/// THE DAY CARD 7 IS ANSWERED: delete `RATCHET_BYTES` and point the gate at
/// [`BOOT_READ_BOUND_BYTES`]. One constant. `ratchet_is_never_looser_than_the_ruled_bound` below
/// is what forces that day to arrive rather than pass unnoticed.
const RATCHET_BYTES: u64 = 128_776;

/// The boot read itself. `CARGO_MANIFEST_DIR` is this crate inside *this* worktree, so the gate
/// measures the tree it is running in — never a sibling checkout, never a peer's live working tree.
const BOOT_READ_REL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/OVERSEER.md");

fn boot_read() -> PathBuf {
    PathBuf::from(BOOT_READ_REL)
}

/// Bytes and lines of a file.
///
/// **Loud on unmeasurable**: returns an error rather than a guess. A gate that cannot see its
/// subject has not passed — it has not run.
fn measure(path: &Path) -> std::io::Result<(u64, usize)> {
    let data = std::fs::read(path)?;
    let lines = data.iter().filter(|&&b| b == b'\n').count();
    Ok((data.len() as u64, lines))
}

/// The whole predicate, in one place, so both directions test the same thing.
///
/// "Stays under" is read inclusively at the boundary: exactly the bound passes.
fn over_bound(size_bytes: u64) -> bool {
    size_bytes > BOOT_READ_BOUND_BYTES
}

/// Thousands separators without a dependency.
fn commas(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Signed variant, for the two figures that can legitimately go negative: headroom under the
/// ratchet, and the residual over the ruled bound once the file is compliant.
fn commas_i64(n: i64) -> String {
    if n < 0 {
        format!("-{}", commas(n.unsigned_abs()))
    } else {
        commas(n as u64)
    }
}

/// The one-line status emitted on EVERY run, pass or fail.
///
/// Written straight to fd 2 rather than through `eprintln!` **on purpose**: libtest captures the
/// print macros and replays them only for failing tests, which would make the pass case silent —
/// and a silent pass is indistinguishable from no gate at all, the very defect this file exists to
/// end. A direct `stderr()` write bypasses that capture.
fn announce(size_bytes: u64, lines: usize) {
    let headroom = RATCHET_BYTES as i64 - size_bytes as i64;
    let residual = size_bytes as i64 - BOOT_READ_BOUND_BYTES as i64;
    // Said in the direction it actually points. The day this file comes inside the bound, the
    // status line is the artifact someone reads to know card 7 can be closed — "-1,000 OVER" would
    // bury that, and a number whose sign carries the meaning is a number that gets misread.
    let against_bound = if over_bound(size_bytes) {
        // "hub card 7", not "card 7": this repo's own queue item 7 is closed and in the log, so a
        // bare number sends the reader to the wrong document.
        format!("{} OVER, awaiting hub card 7", commas_i64(residual))
    } else {
        format!(
            "{} UNDER — the ruled bound is MET, delete the ratchet",
            commas_i64(-residual)
        )
    };
    let line = format!(
        "[boot-read gate] docs/OVERSEER.md = {} B | ratchet {} ({} headroom) | \
         ruled bound {} ({}) | residual {} lines vs guide ~{} (NOT gated)\n",
        commas(size_bytes),
        commas(RATCHET_BYTES),
        commas_i64(headroom),
        commas(BOOT_READ_BOUND_BYTES),
        against_bound,
        commas(lines as u64),
        BOOT_READ_LINES_GUIDE,
    );
    let _ = std::io::stderr().write_all(line.as_bytes());
}

/// The actionable verdict for a file that is over the ruled bound. Every number in it is derived
/// from [`BOOT_READ_BOUND_BYTES`]; the line figure is labelled as a residual so no later hand
/// mistakes it for something the gate checks.
fn verdict(path: &Path, size_bytes: u64, lines: usize) -> String {
    let over = size_bytes - BOOT_READ_BOUND_BYTES;
    format!(
        "{} is {} bytes against the suite's boot-read bound of {} bytes — {} OVER.\n\
         \x20 residual (NOT gated): {} lines against the protocol's guide of about {}.\n\
         \x20 The bound is empyrean docs/OVERSEER-PROTOCOL.md, section 'The boot read is bounded'.\n\
         \x20 Read it at a committed revision, never through the sibling path:\n\
         \x20   git -C ../empyrean fetch -q origin\n\
         \x20   git -C ../empyrean show origin/main:docs/OVERSEER-PROTOCOL.md\n\
         \x20 The fix is that section's split procedure, IN ITS ORDER:\n\
         \x20   1. move the dated tail whole to docs/OVERSEER-LOG.md;\n\
         \x20   2. MEASURE before pointer-ising any bar — only lines a grep finds verbatim in\n\
         \x20      origin/main:docs/OVERSEER-PROTOCOL.md qualify (a bar that CITED the protocol and\n\
         \x20      wrote local precedent under it looks identical in a listing and is not a duplicate);\n\
         \x20   3. move closed history around a live rule to the log VERBATIM, keeping the rule.\n\
         \x20 Live repo-specific rulings interleaved with narrative are the OWNER'S parcel — report\n\
         \x20 the residual rather than trimming a ruling to hit this number.\n\
         \x20 Prove any split lossless by set-difference over every non-blank original line before\n\
         \x20 committing.\n\
         \x20 JUDGE BY BYTES: unwrapping a one-line bullet raises the line count while cutting bytes,\n\
         \x20 so the line figure above can move the wrong way under a correct fix. It is reported,\n\
         \x20 never asserted.",
        path.display(),
        commas(size_bytes),
        commas(BOOT_READ_BOUND_BYTES),
        commas(over),
        commas(lines as u64),
        BOOT_READ_LINES_GUIDE,
    )
}

// ---------------------------------------------------------------------------------------------
// A scratch directory, hand-rolled so this file keeps its zero-dependency property.
// ---------------------------------------------------------------------------------------------

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before the epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "oracle-boot-read-gate-{}-{tag}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("cannot create {dir:?}: {e}"));
        Scratch(dir)
    }

    /// A fixture of exactly `size_bytes` bytes. The SIZE is the argument, always derived from the
    /// bound by the caller — never a literal, so moving the constant moves the fixtures with it.
    fn file_of(&self, name: &str, size_bytes: u64) -> PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, vec![b'x'; size_bytes as usize])
            .unwrap_or_else(|e| panic!("cannot write {p:?}: {e}"));
        p
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------------------------------------
// The gate.
// ---------------------------------------------------------------------------------------------

/// **Loud on unmeasurable.** A missing or unreadable boot read FAILS; it never skips and never
/// passes. The failure this guards is the one the suite keeps re-finding: an absent instrument
/// reported as a green result.
#[test]
fn boot_read_exists_and_is_measurable() {
    let path = boot_read();
    assert!(
        path.is_file(),
        "{} does not exist, or is not a regular file. The boot read is the file every overseer \
         session reads first; its absence is a FAILURE, not a skip. If it moved, this gate moves \
         with it — do not delete the gate.",
        path.display()
    );
    let (size_bytes, _) = measure(&path)
        .unwrap_or_else(|e| panic!("cannot read the boot read at {}: {e}", path.display()));
    assert!(
        size_bytes > 0,
        "{} is empty — that is a broken boot read, not a small one.",
        path.display()
    );
}

/// **THE GATE, in its ratchet form.** The boot read may not GROW past the measured size, and the
/// distance to the ruled bound is reported either way.
///
/// This is failable today, by growth, which is the live risk while card 7 is open: content gets
/// added by people who do not know the file is over.
#[test]
fn boot_read_does_not_grow_past_the_ratchet() {
    let path = boot_read();
    let (size_bytes, lines) = measure(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read the boot read at {} — an unmeasurable gate has NOT passed, it has not \
             run: {e}",
            path.display()
        )
    });
    announce(size_bytes, lines);

    assert!(
        size_bytes <= RATCHET_BYTES,
        "\n{}\n\nGREW by {} bytes past the ratchet of {}.\nThe boot read is already over the \
         suite-ruled bound and is waiting on the owner: the suite-wide card 7 on the HUB's status, \
         NOT this repo's queue item 7 (ours is closed and in the log). Read it with\n  git -C \
         ../empyrean show origin/main:docs/OVERSEER-LOG.md   # 2026-09-04T12:43:53Z, 13:26:52Z\n\
         Do not add to the boot read: put the content in docs/OVERSEER-LOG.md instead. If you \
         SHRANK the file and want the new floor held, lower RATCHET_BYTES in this file to the new \
         measurement and say so.",
        verdict(&path, size_bytes, lines),
        commas(size_bytes - RATCHET_BYTES),
        commas(RATCHET_BYTES),
    );
}

/// **The ratchet may only ever move down.** If the file ever reaches the ruled bound, a ratchet
/// sitting ABOVE that bound would silently permit regrowth straight back into breach — so this
/// fails the moment the ratchet becomes the weaker of the two, which is the day someone should be
/// deleting it.
// The comparison inside is between two constants, which clippy flags — but its REACHABILITY is not
// constant: it is reached only when the measured file has come within the ruled bound. That is the
// design of a ratchet, and folding the comparison away at compile time would delete the trigger.
#[allow(clippy::assertions_on_constants)]
#[test]
fn ratchet_is_never_looser_than_the_ruled_bound() {
    let path = boot_read();
    let (size_bytes, _) = measure(&path)
        .unwrap_or_else(|e| panic!("cannot read the boot read at {}: {e}", path.display()));
    if !over_bound(size_bytes) {
        assert!(
            RATCHET_BYTES <= BOOT_READ_BOUND_BYTES,
            "{} is now {} bytes, within the ruled {} — but RATCHET_BYTES is {}, which is LOOSER \
             than the bound and would permit regrowth into breach. The residual is settled: delete \
             RATCHET_BYTES and assert BOOT_READ_BOUND_BYTES directly.",
            path.display(),
            commas(size_bytes),
            commas(BOOT_READ_BOUND_BYTES),
            commas(RATCHET_BYTES),
        );
    }
}

/// **Both directions, on fixtures.** A bound test that can only ever observe the one real file is
/// one-directional, and the direction it cannot see is the one that leaves it GREEN.
///
/// The fixtures are DERIVED from [`BOOT_READ_BOUND_BYTES`] rather than authored: the over-long case
/// is `BOUND + 1`, not a literal. Move the constant either way and this test tracks it; re-author a
/// fixture and it tracks neither.
#[test]
fn bound_check_is_two_directional() {
    let scratch = Scratch::new("directions");

    // AT the bound: passes. "Stays under" is inclusive at the boundary.
    let at = scratch.file_of("at_bound.md", BOOT_READ_BOUND_BYTES);
    assert_eq!(measure(&at).unwrap(), (BOOT_READ_BOUND_BYTES, 0));
    assert!(
        !over_bound(BOOT_READ_BOUND_BYTES),
        "the bound itself must PASS — 'under' is read inclusively here"
    );

    // UNDER: passes.
    let under = scratch.file_of("under.md", BOOT_READ_BOUND_BYTES - 1);
    assert!(!over_bound(measure(&under).unwrap().0));

    // OVER by one byte: fails.
    let over = scratch.file_of("over.md", BOOT_READ_BOUND_BYTES + 1);
    assert!(
        over_bound(measure(&over).unwrap().0),
        "one byte over the bound must FAIL — a bound test that only ever sees compliant input \
         cannot tell you the gate works"
    );

    // And the verdict must be actionable, or a reader cannot act on it.
    let text = verdict(&over, BOOT_READ_BOUND_BYTES + 1, 0);
    assert!(
        text.contains(&over.display().to_string()),
        "the verdict must name the file"
    );
    assert!(
        text.contains(&commas(BOOT_READ_BOUND_BYTES + 1)),
        "the verdict must report the ACTUAL size"
    );
    assert!(
        text.contains(&commas(BOOT_READ_BOUND_BYTES)),
        "the verdict must report the BOUND"
    );
    assert!(
        text.contains("residual (NOT gated)"),
        "the line count must be reported as a residual, never as something gated"
    );
    assert!(
        text.contains("OVERSEER-LOG.md"),
        "the verdict must name where the content goes"
    );
    assert!(
        text.contains("OWNER'S parcel"),
        "the verdict must say that live rulings are not the agent's to trim"
    );
}

/// **The gate is bytes only.** The protocol warns that the line half can move the wrong way under
/// a correct fix, so the guide is reported and never gated. This test is what stops a later hand
/// from "tightening" the gate by adding a line assertion: a file at three times the line guide but
/// comfortably under the byte bound must PASS.
#[test]
fn gate_never_asserts_the_line_count() {
    let many_short_lines = BOOT_READ_LINES_GUIDE * 3;
    let body_len = (many_short_lines * 2) as u64; // "a\n" per line
    assert!(body_len < BOOT_READ_BOUND_BYTES);
    assert!(
        !over_bound(body_len),
        "a file with three times the line guide but comfortably under the byte bound must PASS — \
         the gate is bytes only"
    );

    // And the real file's own residual is over the guide today, which is exactly the state the
    // protocol says is by design, not by neglect. If this ever stops being true the assertion
    // below is the reminder that the residual is a REPORT, not a target.
    let (_, lines) = measure(&boot_read()).expect("boot read unmeasurable");
    assert!(
        lines > 0,
        "a boot read with no lines is a broken measurement, not a compliant file"
    );
}

/// `measure()` must never manufacture a measurement for a file that is not there.
#[test]
fn measuring_a_missing_file_errors_rather_than_returning_a_number() {
    let scratch = Scratch::new("missing");
    let absent = scratch.0.join("no-such-file.md");
    assert!(
        measure(&absent).is_err(),
        "measuring a missing file must be an ERROR — a gate that returns a number for a file it \
         could not read is the unmeasurable-passes-quietly defect in miniature"
    );
}
