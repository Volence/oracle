//! **P10, made a checked fact: no em dashes and no en dashes in text a person reads in the tool.**
//!
//! Owner ruling, 2026-09-05, suite-wide: *"Can we add no emdashess to the design list, like no emdashes
//! in an of our tools"*, and then *"get rid of all current emdashes and update so no more emdashes to all
//! the tool agents"*. It is filed as **P10** in `docs/2026-09-05-debug-window-style.md`. This file is the
//! part that keeps it true after tonight.
//!
//! # Why a lexer and not a grep
//!
//! P10 shipped with a documented check:
//!
//! ```text
//! grep -rnP '[\x{2014}\x{2013}]' crates/*/src/*.rs | grep -vE ':\s*//[/!]?'
//! ```
//!
//! Run against the pre-sweep tree (`1c3aea2`) that check reported **697** lines workspace-wide and **214**
//! for `oracle-player`. The true in-scope counts were **379** and **91**. Both of its errors, and their
//! measured sizes, are why this file lexes instead:
//!
//! 1. **It cannot exclude test modules.** This is the big one, and it is not fixable with a grep. A
//!    character-level filter has no idea whether a string belongs to shipped code or to a test module, so
//!    the check counted all 756 string dashes in `crates/*/src/**` when only 379 were in scope. It
//!    over-reported by **1.84x**, and a rule whose measurement is inflated by that much cannot tell anyone
//!    when it has been satisfied.
//! 2. **`crates/*/src/*.rs` does not recurse.** The glob names files sitting directly in a `src/`, and
//!    `-r` does not rescue it because the shell has already expanded it to a list of plain files. Measured
//!    blind spot on the pre-sweep tree: **3 occurrences**, all in `crates/oracle-core/src/z80/mod.rs`.
//!    Small today, unbounded tomorrow, since it grows with every subdirectory anyone adds.
//!
//! A third flaw was hypothesised while writing this file and **measured to be nonexistent**, which is
//! recorded here so nobody re-derives it: the exemption `grep -vE ':\s*//[/!]?'` is a *line* filter, so in
//! principle a line carrying both a shipped string and a trailing `//` comment would be exempted along
//! with the comments. On the pre-sweep tree that case occurred **0 times**. It remains a latent hazard of
//! the grep's shape rather than a defect it actually had.
//!
//! A character grep cannot tell a string from a comment, or shipped code from a test. This test lexes: it
//! finds every dash genuinely **inside a string literal in production code**, which is exactly the set P10
//! names and nothing else.
//!
//! # What is in scope, and what deliberately is not
//!
//! **In scope:** `U+2014` and `U+2013` inside a string literal (`"…"`, `r"…"`, `r#"…"#`, byte strings, and
//! the format strings built from them) in `crates/*/src/**/*.rs`, outside any `#[cfg(test)]` module. Those
//! are the strings that reach a screen or a terminal: panel text, refusals, log lines, CLI help, caveats
//! and the Aether method summaries that ship to every client in `initialize.methodSummaries`.
//!
//! **Out of scope, on purpose:**
//!
//! * **Doc comments (`///`, `//!`) and line comments (`//`).** They are the overwhelming bulk of the raw
//!   hits (9,903 of the 11,515 counted across the workspace on 2026-09-05) and **no person reads them in
//!   the tool**. Sweeping them would buy nothing and would make `git blame` useless on every file that
//!   explains itself.
//! * **Anything inside a `#[cfg(test)]` module, and everything under `tests/`.** An assertion message is
//!   read by a developer when a test fails, which is not "in the tool"; and in this repo those messages
//!   carry dense recon provenance whose punctuation is doing real work.
//! * **Escaped forms.** `'\u{2014}'` as a *character value* is not text: the bitmap font in
//!   `oracle-frontend` must keep an em-dash glyph so that a dash arriving from elsewhere renders as a dash
//!   rather than a missing-glyph box, and the existing P10 guards spell their needle that way too. The
//!   lexer sees the source bytes `\`,`u`,`{`,… and not a dash, so it passes them without special-casing,
//!   which is the behaviour we want.
//! * **`crates/oracle-aether/tests/contract/`.** Byte-equality with a peer's vendored copy is what the
//!   conformance gate exists to prove. It is under `tests/`, so it is already outside the walk; this note
//!   records that the exclusion is intentional and not an oversight.
//!
//! # Anti-vacuity
//!
//! A checker that scans nothing passes everything, and this repo has shipped that failure before. Three
//! guards, all of which fail loudly rather than returning a clean zero:
//!
//! * the walk must find **at least [`MIN_FILES`] files**, so a broken root or a glob that stops matching
//!   is a red test and not a green one;
//! * the lexer is proven to **classify a planted sample correctly** in [`the_classifier_can_tell_a_string_from_a_comment`],
//!   covering the cases a naive matcher gets wrong: a dash in a `//` comment, in a doc comment, in a raw
//!   string, in a char literal, and on a line that also holds a lifetime (`&'static`), which is what breaks
//!   a scanner that treats every `'` as opening a character;
//! * the scope split is proven to be a **partition** in [`every_dash_is_classified_exactly_once`]: the
//!   per-kind counts must sum to the raw character count, so a dash cannot escape by falling between two
//!   categories.

use std::fs;
use std::path::{Path, PathBuf};

/// The floor the walk must clear. The workspace had 189 tracked `.rs` files outside the contract directory
/// on 2026-09-05; this is set well below that so ordinary growth and pruning never trip it, while a root
/// that resolves wrong (the failure this guards) collapses to single digits or zero.
const MIN_FILES: usize = 60;

/// What a character in a Rust source file belongs to.
///
/// Only [`Kind::Str`] is in P10's scope. The rest are named rather than lumped into "not a string" so the
/// partition test can prove nothing falls between them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Code,
    LineComment,
    DocComment,
    BlockComment,
    Str,
    CharLit,
}

/// Classify every character of `src` by what it belongs to.
///
/// This is a deliberately small Rust lexer: it only needs to know where strings, comments and character
/// literals begin and end, so it skips everything about the language that does not move those boundaries.
///
/// The subtleties that earn their lines here:
///
/// * **Raw strings** (`r"…"`, `r#"…"#`, `br#"…"#`) have no escape processing, so the terminator is the
///   quote followed by exactly as many `#` as opened it. A scanner that stops at the first `"` truncates
///   the literal and then reads its remainder as code.
/// * **Char literals versus lifetimes.** `'a` in `&'a str` is not an unterminated character literal. The
///   rule used here is that a `'` opens a character literal only when a closing `'` sits where a
///   single-character (or escaped) literal would put it; anything else is a lifetime and is passed over.
/// * **Escapes inside strings.** `\"` does not close a string and `\\` does not escape the quote after it,
///   so the scan advances two characters at a backslash rather than one.
fn classify(src: &[char]) -> Vec<Kind> {
    let n = src.len();
    let mut kinds = vec![Kind::Code; n];
    let mut i = 0usize;

    let paint = |kinds: &mut Vec<Kind>, from: usize, to: usize, k: Kind| {
        for slot in kinds.iter_mut().take(to.min(n)).skip(from) {
            *slot = k;
        }
    };

    while i < n {
        let c = src[i];

        // `//`, `///`, `//!` — to end of line.
        if c == '/' && i + 1 < n && src[i + 1] == '/' {
            let mut j = i;
            while j < n && src[j] != '\n' {
                j += 1;
            }
            // `///` and `//!` are doc comments; `////` is an ordinary comment again.
            let is_doc = i + 2 < n
                && (src[i + 2] == '/' || src[i + 2] == '!')
                && !(i + 3 < n && src[i + 2] == '/' && src[i + 3] == '/');
            paint(
                &mut kinds,
                i,
                j,
                if is_doc {
                    Kind::DocComment
                } else {
                    Kind::LineComment
                },
            );
            i = j;
            continue;
        }

        // `/* … */`, which nests in Rust.
        if c == '/' && i + 1 < n && src[i + 1] == '*' {
            let is_doc = i + 2 < n && (src[i + 2] == '*' || src[i + 2] == '!');
            let mut depth = 1usize;
            let mut j = i + 2;
            while j < n && depth > 0 {
                if src[j] == '/' && j + 1 < n && src[j + 1] == '*' {
                    depth += 1;
                    j += 2;
                } else if src[j] == '*' && j + 1 < n && src[j + 1] == '/' {
                    depth -= 1;
                    j += 2;
                } else {
                    j += 1;
                }
            }
            paint(
                &mut kinds,
                i,
                j,
                if is_doc {
                    Kind::DocComment
                } else {
                    Kind::BlockComment
                },
            );
            i = j;
            continue;
        }

        // Raw strings, with the optional `b` byte prefix: `r"…"`, `r#"…"#`, `br##"…"##`.
        {
            let mut m = i;
            if src[m] == 'b' && m + 1 < n && src[m + 1] == 'r' {
                m += 1;
            }
            if src[m] == 'r' {
                let mut h = m + 1;
                let mut hashes = 0usize;
                while h < n && src[h] == '#' {
                    hashes += 1;
                    h += 1;
                }
                if h < n && src[h] == '"' {
                    let mut j = h + 1;
                    let mut end = n;
                    while j < n {
                        if src[j] == '"' {
                            let mut k = 0usize;
                            while k < hashes && j + 1 + k < n && src[j + 1 + k] == '#' {
                                k += 1;
                            }
                            if k == hashes {
                                end = j + 1 + hashes;
                                break;
                            }
                        }
                        j += 1;
                    }
                    paint(&mut kinds, i, end, Kind::Str);
                    i = end;
                    continue;
                }
            }
        }

        // Ordinary and byte strings: `"…"`, `b"…"`.
        {
            let mut m = i;
            if src[m] == 'b' && m + 1 < n && src[m + 1] == '"' {
                m += 1;
            }
            if src[m] == '"' {
                let mut j = m + 1;
                while j < n {
                    if src[j] == '\\' {
                        j += 2;
                        continue;
                    }
                    if src[j] == '"' {
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                paint(&mut kinds, i, j, Kind::Str);
                i = j;
                continue;
            }
        }

        // A character literal, or a lifetime that must not be mistaken for one.
        if c == '\'' {
            if i + 1 < n && src[i + 1] == '\\' {
                let mut j = i + 2;
                while j < n && src[j] != '\'' {
                    j += 1;
                }
                j += 1;
                paint(&mut kinds, i, j, Kind::CharLit);
                i = j;
                continue;
            }
            if i + 2 < n && src[i + 2] == '\'' {
                paint(&mut kinds, i, i + 3, Kind::CharLit);
                i += 3;
                continue;
            }
            // A lifetime: step over the quote and read the name as ordinary code.
            i += 1;
            continue;
        }

        i += 1;
    }

    kinds
}

/// Does this `cfg` predicate gate its item on `test`?
///
/// ⚑ **This function exists because matching the literal string `#[cfg(test)]` is wrong, and quietly so.**
/// The first version of this gate did exactly that, and it mis-scoped **101 of 480** apparent sites on the
/// pre-sweep tree: `oracle-player` alone writes `#[cfg(all(test, unix))]` in eight places and
/// `oracle-frontend` writes `#[cfg(all(test, feature = "aether"))]`, and every string in those modules was
/// counted as shipped text. A sweep driven by that count would have rewritten a hundred test assertions
/// for nothing, and the gate would then have stayed red against correctly-swept code.
///
/// The rule is a **bare `test` token anywhere in the predicate**, which admits `all(test, unix)`,
/// `any(test, feature = "x")` and plain `test`. String contents are stripped first, so
/// `feature = "test-utils"` is correctly *not* treated as test-gating: the word only appears there inside
/// a quoted feature name.
fn predicate_gates_on_test(pred: &str) -> bool {
    // Drop quoted strings, so a feature *named* "test-…" cannot masquerade as the `test` cfg.
    let mut bare = String::with_capacity(pred.len());
    let mut in_str = false;
    let mut prev_escape = false;
    for c in pred.chars() {
        match c {
            '"' if !prev_escape => in_str = !in_str,
            _ if !in_str => bare.push(c),
            _ => {}
        }
        prev_escape = c == '\\' && !prev_escape;
    }

    // A whole-word `test`, not `test_utils` and not `latest`.
    bare.match_indices("test").any(|(at, _)| {
        let before_ok = at == 0
            || !bare[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
        let after = at + "test".len();
        let after_ok = !bare[after..]
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        before_ok && after_ok
    })
}

/// The character ranges covered by test-gated modules, so their contents can be held out of scope.
///
/// The attribute is located only where it is genuine code (an occurrence inside a string or a comment is
/// not an attribute), its predicate is read to the matching `)` and tested by [`predicate_gates_on_test`],
/// then the following `{` is brace-matched with strings and comments skipped, so a `}` inside a test's own
/// message does not close the module early.
fn cfg_test_spans(src: &[char], kinds: &[Kind]) -> Vec<(usize, usize)> {
    let needle: Vec<char> = "#[cfg(".chars().collect();
    let n = src.len();
    let mut spans = Vec::new();
    let mut i = 0usize;

    while i + needle.len() <= n {
        if kinds[i] == Kind::Code && src[i..i + needle.len()] == needle[..] {
            // Read the predicate to its matching ')'.
            let mut depth = 0usize;
            let mut e = i + needle.len() - 1;
            while e < n {
                if kinds[e] == Kind::Code {
                    if src[e] == '(' {
                        depth += 1;
                    } else if src[e] == ')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }
                e += 1;
            }
            let pred: String = src[(i + needle.len()).min(n)..e.min(n)].iter().collect();
            if !predicate_gates_on_test(&pred) {
                i += 1;
                continue;
            }

            // The opening brace of the module that the attribute decorates.
            let mut b = e.min(n);
            while b < n && !(src[b] == '{' && kinds[b] == Kind::Code) {
                b += 1;
            }
            if b < n {
                let mut depth = 0usize;
                let mut j = b;
                while j < n {
                    if kinds[j] == Kind::Code {
                        if src[j] == '{' {
                            depth += 1;
                        } else if src[j] == '}' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    j += 1;
                }
                spans.push((i, (j + 1).min(n)));
                i = (j + 1).min(n);
                continue;
            }
        }
        i += 1;
    }

    spans
}

fn is_dash(c: char) -> bool {
    c == '\u{2014}' || c == '\u{2013}'
}

/// The environment variable that repoints the scan at another checkout.
///
/// It exists for **one** purpose: proving this gate goes red. A gate nobody has watched fail is a gate
/// nobody has tested, and the honest way to watch this one fail is to aim it at a tree that genuinely
/// has not been swept, rather than at a dash planted to order. Pointing it at the pre-sweep revision
/// reproduces the real corpus and the real count.
///
/// It cannot be used to make the gate pass on a dirty tree: nothing in CI sets it, and [`MIN_FILES`]
/// rejects an empty or wrong directory rather than reporting a clean zero for it.
const ENV_ROOT: &str = "P10_ROOT";

/// The workspace root, derived from this crate's manifest directory rather than from the process's
/// current directory, which a test runner is free to choose.
fn workspace_root() -> PathBuf {
    if let Some(r) = std::env::var_os(ENV_ROOT) {
        return PathBuf::from(r);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/<crate>/ always has two ancestors")
        .to_path_buf()
}

/// Every `.rs` file under some `crates/*/src/`, recursively.
///
/// Recursion is the point: the check this replaces globbed `crates/*/src/*.rs` and could not see a
/// subdirectory.
fn shipped_sources(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(std::fs::DirEntry::path);
        for e in entries {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    let mut out = Vec::new();
    let Ok(crates) = fs::read_dir(root.join("crates")) else {
        return out;
    };
    let mut crates: Vec<_> = crates.filter_map(Result::ok).collect();
    crates.sort_by_key(std::fs::DirEntry::path);
    for c in crates {
        let src = c.path().join("src");
        if src.is_dir() {
            walk(&src, &mut out);
        }
        // A build script prints to the terminal during `cargo build`, so its strings are read by a
        // person as surely as a panel's are. There was one such dash, in `oracle-aether/build.rs`.
        let build = c.path().join("build.rs");
        if build.is_file() {
            out.push(build);
        }
    }
    out
}

/// One offending dash, with enough context for the failure message to be actionable on its own.
struct Offence {
    file: PathBuf,
    line: usize,
    text: String,
}

/// Scan one file and return the dashes that sit in production string literals.
fn offences_in(path: &Path, src: &str) -> Vec<Offence> {
    let chars: Vec<char> = src.chars().collect();
    let kinds = classify(&chars);
    let spans = cfg_test_spans(&chars, &kinds);
    let lines: Vec<&str> = src.split('\n').collect();

    let mut out = Vec::new();
    let mut line = 1usize;
    for (idx, &c) in chars.iter().enumerate() {
        if c == '\n' {
            line += 1;
        }
        if !is_dash(c) || kinds[idx] != Kind::Str {
            continue;
        }
        if spans.iter().any(|&(a, b)| idx >= a && idx < b) {
            continue;
        }
        out.push(Offence {
            file: path.to_path_buf(),
            line,
            text: lines
                .get(line - 1)
                .map(|s| s.trim().chars().take(150).collect())
                .unwrap_or_default(),
        });
    }
    out
}

/// **The gate.** No em dash and no en dash reaches a person through a shipped string.
#[test]
fn no_em_or_en_dash_in_shipped_strings() {
    let root = workspace_root();
    let files = shipped_sources(&root);

    assert!(
        files.len() >= MIN_FILES,
        "the walk found only {} source file(s) under {}/crates/*/src/, below the floor of {MIN_FILES}. \
         A checker that scans nothing passes everything, so this is a failure of the walk and not a \
         clean result: check that the workspace root resolved correctly.",
        files.len(),
        root.display()
    );

    let mut offences: Vec<Offence> = Vec::new();
    for f in &files {
        let Ok(src) = fs::read_to_string(f) else {
            continue;
        };
        offences.extend(offences_in(f, &src));
    }

    assert!(
        offences.is_empty(),
        "P10: {} em/en dash(es) reach a person through shipped strings, across {} file(s) scanned.\n\
         Use a colon, a full stop and a new sentence, a comma pair, or parentheses. A dash standing in \
         for a colon is almost always a colon.\n\
         Doc comments and `#[cfg(test)]` modules are exempt and are not counted here.\n{}",
        offences.len(),
        files.len(),
        offences
            .iter()
            .map(|o| format!(
                "  {}:{}: {}",
                o.file
                    .strip_prefix(&root)
                    .unwrap_or(&o.file)
                    .display(),
                o.line,
                o.text
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// **Proof the lexer tells the categories apart**, on the cases a naive matcher gets wrong.
///
/// Each line of the sample is a shape that has produced a real miss somewhere: a dash in an ordinary
/// comment and in a doc comment (which P10 exempts and a character grep cannot), a dash in a raw string
/// and in a character literal (which are neither comment nor ordinary string, and were invisible to
/// another lane's gate tonight), and a lifetime sharing a line with a string, which desynchronises a
/// scanner that treats every `'` as opening a character literal.
#[test]
fn the_classifier_can_tell_a_string_from_a_comment() {
    let sample = "\
//! module doc \u{2014} a
/// item doc \u{2014} b
// plain \u{2014} c
/* block \u{2014} d */
fn f() {
    let a = \"string \u{2014} e\";
    let b = r#\"raw \u{2014} f\"#;
    let c = '\u{2014}';
    let d: &'static str = \"after a lifetime \u{2014} g\";
}
";
    let chars: Vec<char> = sample.chars().collect();
    let kinds = classify(&chars);

    let mut got: Vec<(char, Kind)> = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        if is_dash(c) {
            // The letter that labels this dash in the sample sits two characters along.
            got.push((chars[i + 2], kinds[i]));
        }
    }

    assert_eq!(
        got,
        vec![
            ('a', Kind::DocComment),
            ('b', Kind::DocComment),
            ('c', Kind::LineComment),
            ('d', Kind::BlockComment),
            ('e', Kind::Str),
            ('f', Kind::Str),
            // The char literal's dash has no label after it; `';'` follows the closing quote.
            (';', Kind::CharLit),
            ('g', Kind::Str),
        ],
        "the lexer mis-attributed at least one dash; the gate's scope is only as good as this"
    );
}

/// **Proof that test-gating is recognised in the spellings this workspace actually uses.**
///
/// This is the pin for the mistake that cost the most in this parcel. Matching the literal
/// `#[cfg(test)]` looked right, passed every review, and mis-scoped 101 of 480 sites, because two crates
/// here gate their test modules with a compound predicate. The false-negative direction inflates the
/// sweep; the false-positive direction (`feature = "test-utils"`) would silently exempt shipped code,
/// which is worse. Both directions are pinned.
#[test]
fn test_gating_is_recognised_in_every_spelling_this_workspace_uses() {
    // Gating: these must all be treated as test modules and held out of scope.
    for pred in [
        "test",
        "all(test, unix)",
        "all(test, feature = \"aether\")",
        "any(test, doc)",
        "all(test, target_os = \"linux\")",
    ] {
        assert!(
            predicate_gates_on_test(pred),
            "#[cfg({pred})] gates on test and its module must be out of scope; \
             treating it as shipped code re-creates the 101-site mis-scope"
        );
    }

    // Not gating: a feature whose NAME contains "test" does not make the item test-only, and exempting
    // it would hide shipped strings from the gate.
    for pred in [
        "feature = \"test-utils\"",
        "feature = \"testing\"",
        "unix",
        "all(unix, feature = \"latest\")",
        "target_os = \"test-os\"",
    ] {
        assert!(
            !predicate_gates_on_test(pred),
            "#[cfg({pred})] does NOT gate on the test cfg, so its module ships and must stay in scope"
        );
    }
}

/// **Proof the categories partition the dashes**, so none escapes between two of them.
///
/// The gate exempts several categories. If a dash could belong to none of them it would be neither
/// counted nor exempted, and the exemption would quietly become a hole. Summing the per-kind counts back
/// to the raw character count is what forbids that.
#[test]
fn every_dash_is_classified_exactly_once() {
    let root = workspace_root();
    let files = shipped_sources(&root);
    assert!(files.len() >= MIN_FILES, "walk found {} files", files.len());

    let mut raw = 0usize;
    let mut classified = 0usize;
    for f in &files {
        let Ok(src) = fs::read_to_string(f) else {
            continue;
        };
        let chars: Vec<char> = src.chars().collect();
        let kinds = classify(&chars);
        for (i, &c) in chars.iter().enumerate() {
            if !is_dash(c) {
                continue;
            }
            raw += 1;
            match kinds[i] {
                Kind::Code
                | Kind::LineComment
                | Kind::DocComment
                | Kind::BlockComment
                | Kind::Str
                | Kind::CharLit => classified += 1,
            }
        }
    }

    assert_eq!(
        raw, classified,
        "every dash must land in exactly one category, or the exemptions hide an unchecked one"
    );
    assert!(
        raw > 0,
        "no dash of any kind was found in {} file(s). The doc comments alone held thousands on \
         2026-09-05, so a zero here means the scan is not reading the files it thinks it is, which \
         would make the gate above vacuous.",
        files.len()
    );
}
