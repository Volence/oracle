//! **The small Rust lexer the source gates share**: where strings, comments and character literals begin
//! and end, and which spans are test-gated modules.
//!
//! Moved here unchanged from `p10_no_dashes_in_shipped_text.rs` (2026-09-17) when a second gate needed
//! it: `p7_every_scroll_area_names_its_id_salt.rs`, which has to tell production code from test modules
//! in files (`bus.rs`, `main.rs`) whose production code continues past their first test module. A second
//! copy would be a second lexer to keep in step; the proofs that it classifies correctly stay in the P10
//! file, which is where they were written.

/// What a character in a Rust source file belongs to.
///
/// Only [`Kind::Str`] is in P10's scope. The rest are named rather than lumped into "not a string" so the
/// partition test can prove nothing falls between them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
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
pub fn classify(src: &[char]) -> Vec<Kind> {
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
pub fn predicate_gates_on_test(pred: &str) -> bool {
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
pub fn cfg_test_spans(src: &[char], kinds: &[Kind]) -> Vec<(usize, usize)> {
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
