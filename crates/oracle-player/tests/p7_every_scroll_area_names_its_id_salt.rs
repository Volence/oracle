//! **P7, made a checked fact for the whole crate: every `ScrollArea` production code builds names its
//! `id_salt`.**
//!
//! The style page's P7 (`docs/2026-09-05-debug-window-style.md`): *one panel, one scroll position; scroll
//! areas carry a stable `id_salt`.* An unsalted scroll area draws identically and only loses its place
//! when something else in the same parent changes, which no headless frame reaches, so this is a source
//! gate.
//!
//! # Why this replaced the gate in `ui.rs`
//!
//! The debug-window audit's parcel 6-8 addendum found three unsalted scroll areas in the crate, salted
//! the one in `ui.rs` and gated `ui.rs` alone
//! (`every_scroll_area_production_ui_rs_builds_names_its_id_salt`). The other two were in the command
//! palette (`palette.rs`) and the cartridge-swap modal (`rom_open.rs`), and that gate could not see them:
//! it read one file and cut production off at the first test module, which is wrong for `bus.rs` and
//! `main.rs`, whose production code continues past theirs. This gate reads every file under `src/` and
//! holds test modules out with the shared lexer (`source_lex`), so a test module anywhere in a file is
//! exempt and production code after it is not.
//!
//! # What counts
//!
//! A `ScrollArea::` in code (not in a comment or string) outside a test-gated module, not on a `use`
//! line. Its builder runs from there to the first `.show` in code; `.id_salt(` must appear in code
//! before it.

mod source_lex;

use source_lex::{cfg_test_spans, classify, Kind};
use std::fs;
use std::path::{Path, PathBuf};

/// One scroll area production code builds.
struct Built {
    file: String,
    line: usize,
    salted: bool,
    builder: String,
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("COULD NOT MEASURE: {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Every scroll-area constructor in production code in one file's text: line, salted, builder text.
fn builders_in(text: &str) -> Vec<(usize, bool, String)> {
    let chars: Vec<char> = text.chars().collect();
    let kinds = classify(&chars);
    let spans = cfg_test_spans(&chars, &kinds);
    let in_test = |i: usize| spans.iter().any(|&(a, b)| i >= a && i < b);
    let code_at = |i: usize, needle: &str| {
        let n: Vec<char> = needle.chars().collect();
        i + n.len() <= chars.len()
            && chars[i..i + n.len()] == n[..]
            && kinds[i..i + n.len()].iter().all(|k| *k == Kind::Code)
    };
    let mut out = Vec::new();
    for i in 0..chars.len() {
        if !code_at(i, "ScrollArea::") || in_test(i) {
            continue;
        }
        let line_start = chars[..i]
            .iter()
            .rposition(|c| *c == '\n')
            .map_or(0, |p| p + 1);
        let lead: String = chars[line_start..i].iter().collect();
        if lead.trim_start().starts_with("use ") {
            continue;
        }
        let show = (i..chars.len())
            .find(|&j| code_at(j, ".show"))
            .expect("COULD NOT MEASURE: a ScrollArea builder with no `.show` after it");
        out.push((
            chars[..i].iter().filter(|c| **c == '\n').count() + 1,
            (i..show).any(|j| code_at(j, ".id_salt(")),
            chars[i..show].iter().collect(),
        ));
    }
    out
}

/// Every scroll-area constructor in production code under `src/`.
fn built() -> (Vec<Built>, usize) {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src_dir, &mut files);
    let mut out = Vec::new();
    for f in &files {
        let text = fs::read_to_string(f)
            .unwrap_or_else(|e| panic!("COULD NOT MEASURE: {}: {e}", f.display()));
        for (line, salted, builder) in builders_in(&text) {
            out.push(Built {
                file: f.strip_prefix(&src_dir).unwrap_or(f).display().to_string(),
                line,
                salted,
                builder,
            });
        }
    }
    (out, files.len())
}

/// **The gate.**
///
/// Anti-vacuity: the walk must read more than a handful of files, find constructors in more than one of
/// them, and find at least as many as the fifteen the parcel 6-8 addendum counted (a count taken by a
/// different method, a Python pass, so a lexer that silently stopped matching would fall under it).
#[test]
fn every_scroll_area_the_crate_builds_in_production_names_its_id_salt() {
    let (built, files) = built();
    assert!(
        files >= 20,
        "COULD NOT MEASURE: only {files} source files read under src/"
    );
    let mut in_files: Vec<&str> = built.iter().map(|b| b.file.as_str()).collect();
    in_files.dedup();
    assert!(
        built.len() >= 15 && in_files.len() >= 3,
        "COULD NOT MEASURE: only {} ScrollArea constructors, in {:?}",
        built.len(),
        in_files
    );
    let unsalted: Vec<String> = built
        .iter()
        .filter(|b| !b.salted)
        .map(|b| format!("  src/{}:{}\n{}", b.file, b.line, b.builder))
        .collect();
    assert!(
        unsalted.is_empty(),
        "P7: {} of {} scroll areas are built with no id_salt:\n{}",
        unsalted.len(),
        built.len(),
        unsalted.join("\n")
    );
}

/// **The detector tells a salted builder from an unsalted one, and ignores what is not a builder**, on a
/// planted file shaped like the cases above: a salt on a later line, a `ScrollArea::` in a comment and in
/// a string, and one inside a test module that sits above production code.
#[test]
fn the_detector_reads_builders_and_skips_comments_strings_and_test_modules() {
    let sample = "\
fn a(ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .max_height(3.0)
        .id_salt(\"a\")
        .show(ui, |_| {});
}
// egui::ScrollArea::vertical().show(ui, |_| {});
const S: &str = \"egui::ScrollArea::vertical().show\";
#[cfg(all(test, unix))]
mod tests {
    fn t(ui: &mut egui::Ui) { egui::ScrollArea::vertical().show(ui, |_| {}); }
}
fn b(ui: &mut egui::Ui) {
    egui::ScrollArea::both().show(ui, |_| {});
}
";
    let found: Vec<bool> = builders_in(sample).into_iter().map(|b| b.1).collect();
    assert_eq!(
        found,
        vec![true, false],
        "expected the salted builder in `a` and the unsalted one in `b`, and nothing else"
    );
}
