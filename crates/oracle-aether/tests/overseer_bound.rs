//! **The boot read is bounded — gated here, against the ruled bound, with ONE constant.**
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
//! failure as a vacuous check wearing a green result.
//!
//! THE RULING, read at a committed revision and never through the sibling working-tree path (that
//! path is a peer's live tree and is not a citable revision):
//!
//! ```text
//! git -C ../empyrean fetch -q origin
//! git -C ../empyrean show origin/main:docs/OVERSEER-PROTOCOL.md   # "The boot read is bounded"
//! ```
//!
//! **CARD 7 IS ANSWERED, AND THE RATCHET IS GONE (2026-09-04).** This file used to carry a second
//! constant, `RATCHET_BYTES`, pinned at the measured 128,776 B: the file was over the ruled bound,
//! the residual was live rulings interleaved with narrative, and the protocol names that residual
//! as *the owner's parcel* — so asserting the ruled bound would have failed the build permanently
//! on a question only he could answer. He answered it at 2026-09-04T15:38:47Z, one call for all six
//! lanes (hub card 7, his words *"7. Sounds fine"*): **the bound stays at 100,000 B, is never
//! raised, and the boot file is SPLIT BY WHEN A RULE IS READ** — a rule that matters only at a
//! specific later moment (how to dispatch, how to review, how to land) moves to a reference file
//! the lane opens at that moment. Oracle's cut moved "The bars" and "Ops" to
//! `docs/OVERSEER-REFERENCE.md`, taking the boot read from 128,776 B to 89,424 B.
//!
//! So the ratchet's own instruction — *"THE DAY CARD 7 IS ANSWERED: delete `RATCHET_BYTES` and
//! point the gate at `BOOT_READ_BOUND_BYTES`. One constant."* — is executed here, and the test that
//! forced the day to arrive did its job before being retired: on the split commit it failed with
//! *"is now 89,424 bytes, within the ruled 100,000 — but RATCHET_BYTES is 128,776, which is LOOSER
//! than the bound"*. That is the whole reason it existed; see the note on
//! `ratchet_is_never_looser_than_the_ruled_bound` below for why it is not replaced in kind.
//!
//! **BYTES ONLY.** The protocol's own warning, verbatim: *"Judge by bytes. Unwrapping a
//! multi-kilobyte one-line bullet into prose RAISES the line count while cutting bytes ... so the
//! line half of the bound can move the wrong way under a correct fix."* So the line count is
//! reported as a **residual** and is never asserted. Anything gating on lines punishes the fix.
//!
//! **The bound is named ONCE.** It is stated in the protocol as prose, so it cannot be computed
//! from an artifact. Every expectation in this file — the verdict text, the status line, the
//! fixtures, both directions of the two-directional test — is derived from that one name. Nothing
//! here re-types the number, and there is no second number for it to disagree with.
//!
//! **This gate can fail, and it is failable in the direction that matters.** It is not report-only.
//! A check that cannot fail is green by construction, so its presence and its absence read the
//! same — which is exactly the property that let the *missing* gate go unnoticed here. Today the
//! file is 10,576 B under; the day someone adds 10,577 B of prose to the boot read, this goes red
//! with the split procedure printed in the failure.
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

use std::io::Write as _;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------------------------
// THE RULED BOUND — the one and only place a number is written. There is no second one.
//
// empyrean `docs/OVERSEER-PROTOCOL.md`, section "The boot read is bounded", read at `origin/main`
// on 2026-09-04: *"`docs/OVERSEER.md` is the boot read, and it stays under about 900 lines /
// 100 KB."* 100 KB is read as 100,000 bytes — the decimal reading, and the stricter of the two
// (the KiB reading would be 102,400). The hub's own 09-02T17:45:03Z measurements are stated
// against 100,000 B, which settles the reading. Reaffirmed by the owner 2026-09-04T15:38:47Z:
// the bound stays here and is never raised; an over-long file is SPLIT, by when a rule is read.
//
// If the suite ever restates the bound, change it HERE and nowhere else.
const BOOT_READ_BOUND_BYTES: u64 = 100_000;

/// The protocol's "about 900 lines". **Reported beside the bytes, never asserted** — see the
/// module note on why gating this would punish a correct fix.
const BOOT_READ_LINES_GUIDE: usize = 900;

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

/// The one-line status emitted on EVERY run, pass or fail.
///
/// Built as a `String` rather than printed inline **so a test can read it**. The status line is the
/// artifact a human actually sees from this gate on a green run; leaving it untested would make the
/// only human-visible output the one thing nothing checks.
///
/// The distance to the bound is said in the direction it points — "10,576 UNDER" or "1,000 OVER" —
/// never as a signed number. A figure whose sign carries the meaning is a figure that gets misread.
fn status_line(size_bytes: u64, lines: usize) -> String {
    let against_bound = if over_bound(size_bytes) {
        format!(
            "{} OVER — SPLIT IT",
            commas(size_bytes - BOOT_READ_BOUND_BYTES)
        )
    } else {
        format!("{} UNDER", commas(BOOT_READ_BOUND_BYTES - size_bytes))
    };
    format!(
        "[boot-read gate] docs/OVERSEER.md = {} B | ruled bound {} ({}) | \
         residual {} lines vs guide ~{} (NOT gated)\n",
        commas(size_bytes),
        commas(BOOT_READ_BOUND_BYTES),
        against_bound,
        commas(lines as u64),
        BOOT_READ_LINES_GUIDE,
    )
}

/// Written straight to fd 2 rather than through `eprintln!` **on purpose**: libtest captures the
/// print macros and replays them only for failing tests, which would make the pass case silent —
/// and a silent pass is indistinguishable from no gate at all, the very defect this file exists to
/// end. A direct `stderr()` write bypasses that capture.
fn announce(size_bytes: u64, lines: usize) {
    let _ = std::io::stderr().write_all(status_line(size_bytes, lines).as_bytes());
}

/// The actionable verdict for a file that is over the ruled bound. Every number in it is derived
/// from [`BOOT_READ_BOUND_BYTES`]; the line figure is labelled as a residual so no later hand
/// mistakes it for something the gate checks.
///
/// The procedure it prints is the CURRENT one — the owner's 2026-09-04 ruling, split by *when a
/// rule is read*. The older "measure before pointer-ising a bar / move the dated tail" ordering it
/// used to print is still in the protocol and still correct for the log split; it is not what a
/// reader of THIS failure needs first, now that the reference file exists to receive the content.
fn verdict(path: &Path, size_bytes: u64, lines: usize) -> String {
    let over = size_bytes - BOOT_READ_BOUND_BYTES;
    format!(
        "{} is {} bytes against the suite's boot-read bound of {} bytes — {} OVER.\n\
         \x20 residual (NOT gated): {} lines against the protocol's guide of about {}.\n\
         \x20 The bound is empyrean docs/OVERSEER-PROTOCOL.md, section 'The boot read is bounded'.\n\
         \x20 Read it at a committed revision, never through the sibling path:\n\
         \x20   git -C ../empyrean fetch -q origin\n\
         \x20   git -C ../empyrean show origin/main:docs/OVERSEER-PROTOCOL.md\n\
         \x20 THE BOUND IS NEVER RAISED (owner, 2026-09-04T15:38:47Z, one call for all six lanes).\n\
         \x20 SPLIT BY WHEN A RULE IS READ, never by size:\n\
         \x20   - what a fresh session needs to ACT AT BOOT stays here — scope, queue, resume\n\
         \x20     brief, the standing rulings that change what it does first;\n\
         \x20   - a rule read only at a specific later moment (how to dispatch, how to review\n\
         \x20     returned work, how to land) goes to docs/OVERSEER-REFERENCE.md, which the boot\n\
         \x20     file names by path;\n\
         \x20   - closed dated history goes to docs/OVERSEER-LOG.md and is not read at boot.\n\
         \x20 Live repo-specific rulings interleaved with narrative are the OWNER'S parcel — report\n\
         \x20 the residual rather than trimming a ruling to hit this number.\n\
         \x20 PROVE THE SPLIT LOSSLESS AT A UNIT BELOW THE ONE YOU CUT AT: reassemble and diff,\n\
         \x20 then count TOKENS, then check at every seam that the retained side still ends a\n\
         \x20 sentence and the moved side still begins one — run from BOTH files. A 2026-09-02\n\
         \x20 split reported itself lossless with lines accounted for and diff exit 0, and had cut\n\
         \x20 34 sentences in half: every cut was INSIDE the unit its proof counted.\n\
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

/// **THE GATE.** The boot read stays within the ruled bound. One constant, asserted directly.
///
/// This is failable, today, in the live direction: content gets added to the boot read by people
/// who do not know what it costs every session that boots after them.
#[test]
fn boot_read_is_within_the_ruled_bound() {
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
        !over_bound(size_bytes),
        "\n{}\n\nDo not raise the bound and do not trim a live ruling to fit. Move the content: a \
         rule read at ONE LATER MOMENT belongs in docs/OVERSEER-REFERENCE.md, closed dated history \
         belongs in docs/OVERSEER-LOG.md, and what is left is what a fresh session needs to act at \
         boot.",
        verdict(&path, size_bytes, lines),
    );
}

// RETIRED 2026-09-04: `ratchet_is_never_looser_than_the_ruled_bound`.
//
// It compared `RATCHET_BYTES` against `BOOT_READ_BOUND_BYTES` and fired the moment the file came
// inside the bound, so that a local pin left sitting ABOVE the bound could not silently permit
// regrowth back into breach. It fired, exactly once, on the split commit — *"is now 89,424 bytes,
// within the ruled 100,000 — but RATCHET_BYTES is 128,776, which is LOOSER than the bound"* — and
// that failure is what deleted the ratchet.
//
// It is NOT re-pointed at something else, because its question no longer exists: it asked whether
// two constants disagreed, and there is now one constant. A test kept alive past its question
// becomes a test that cannot fail, which is the defect this whole file was written to end. The
// regrowth risk it hedged is now carried directly by `boot_read_is_within_the_ruled_bound`, which
// goes red at 100,001 B with no second number able to soften it.
//
// What its removal DID expose is that nothing covered the human-visible status line, in either
// direction. `status_line_says_which_side_of_the_bound_we_are_on` below covers that, and is a new
// test for a real gap rather than the old one wearing a new name.

/// **The status line is the only thing a human sees on a green run** — so it is checked, in both
/// states, against the one bound.
///
/// The over-case cannot be observed on the real file (it is compliant, and must stay so), which is
/// exactly why it is exercised on a derived size instead of left to chance.
#[test]
fn status_line_says_which_side_of_the_bound_we_are_on() {
    // A synthetic distance, not today's measurement: a fixture pinned to the real file's current
    // headroom would go red on any edit to a document this test does not govern.
    const GAP: u64 = 2_500;
    let under = status_line(BOOT_READ_BOUND_BYTES - GAP, 1_056);
    assert!(
        under.contains("2,500 UNDER"),
        "under the bound, the distance must be reported as UNDER: {under}"
    );
    // Not a bare `contains("OVER")` — the path `docs/OVERSEER.md` is in the line, and the naive
    // form of this assertion failed on the filename the first time it ran. The two markers below
    // are the ones that only the over-case can produce.
    assert!(
        !under.contains(" OVER"),
        "a compliant file must not report a distance as OVER: {under}"
    );
    assert!(
        !under.contains("SPLIT IT"),
        "a compliant file must not be told to split: {under}"
    );

    let over = status_line(BOOT_READ_BOUND_BYTES + 1, 1_056);
    assert!(
        over.contains("1 OVER"),
        "over the bound, the distance must be reported as OVER: {over}"
    );
    assert!(
        over.contains("SPLIT IT"),
        "the over-case must name the remedy, not just the number: {over}"
    );

    // Both states quote the ruled bound, and there is no second bound to quote.
    for line in [&under, &over] {
        assert!(
            line.contains(&commas(BOOT_READ_BOUND_BYTES)),
            "the status line must quote the ruled bound: {line}"
        );
        assert!(
            line.contains("NOT gated"),
            "the line residual must be labelled as ungated wherever it is printed: {line}"
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
        text.contains("OVERSEER-REFERENCE.md"),
        "the verdict must name where a rule read at a later moment goes"
    );
    assert!(
        text.contains("OVERSEER-LOG.md"),
        "the verdict must name where closed history goes"
    );
    assert!(
        text.contains("OWNER'S parcel"),
        "the verdict must say that live rulings are not the agent's to trim"
    );
    assert!(
        text.contains("NEVER RAISED"),
        "the verdict must say the bound is not the thing to change — that is the ruling"
    );
    assert!(
        text.contains("TOKENS"),
        "the verdict must tell the next splitter to prove it below the unit it cut at; a \
         line-and-md5 proof is what let 34 half-sentences through"
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

    // The real file's own residual is still over the guide after the split (1,056 vs ~900) while
    // being comfortably under the byte bound — which is the protocol's own point, made by this
    // repo's own file. If this ever stops being true the assertion below is the reminder that the
    // residual is a REPORT, not a target.
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
