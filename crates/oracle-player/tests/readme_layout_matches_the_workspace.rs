//! **The README's Layout section names every crate this workspace builds, and no crate it does not.**
//!
//! UX packet finding 7, made a checked fact. The front door said *"Four crates in one workspace"* while
//! `Cargo.toml` listed five members and `crates/` held six directories, and the crate it left out was
//! **`oracle-player`** — the entire Registers/Memory/Objects/Screen/Breakpoints surface, the window that
//! does four of the five things a newcomer opens this repo to do. `grep -c oracle-player README.md`
//! returned **0**. A person following the README learned about the window that *cannot* arm a breakpoint
//! and never learned the one that can.
//!
//! # Why a test and not a careful edit
//!
//! The wrong count was not a typo. It was correct when it was written and went stale the moment a crate
//! was added, silently, with every other assertion in the file still true — the exact failure mode this
//! README's own methods-count section describes at length and then refused to repeat for methods. That
//! section's remedy is *"derive it when you need it"*, which works for a number a reader can compute.
//! It does not work for a **list**, because a reader cannot notice a name that is not there.
//!
//! So the list is checked instead of trusted, and the count is checked wherever the prose states one.
//!
//! # What is asserted, and why each one is not implied by the others
//!
//! 1. **Every workspace member has a Layout bullet.** This is the finding: `oracle-player` had none.
//! 2. **Every Layout bullet is a workspace member.** The other direction, which catches a crate that has
//!    been deleted or renamed and left its paragraph behind — a README describing a directory that is not
//!    there is as wrong as one omitting a directory that is.
//! 3. **Every directory under `crates/` is either a member or in `exclude`.** Neither of the first two
//!    can see a directory that is in *no* list at all, which is how a new crate arrives.
//! 4. **An excluded directory is still NAMED in the README.** `crates/oracle-panels-spike` is
//!    deliberately not a member, and a reader who counts the directories will get a number the workspace
//!    does not have. Silence about it is what makes that confusing; the exclusion is a fact about the
//!    repo, not a reason to hide the directory.
//! 5. **Any `"<n> crates"` phrase equals the member count.** The defect was a number in prose. Banning
//!    numbers would be easier and worse: a count is useful, so it is made self-checking instead.
//!
//! # Anti-vacuity
//!
//! Every derivation has a floor, because a parser that finds nothing agrees with everything: fewer than
//! three members, fewer than three bullets, or a `crates/` directory that reads as empty is a **red**
//! test naming the parser, not a green one.
//!
//! # Why this file lives in `oracle-player/tests/`
//!
//! It is a workspace-wide rule with no crate of its own, and this is where the workspace-wide text rules
//! already live: `p10_no_dashes_in_shipped_text.rs`, in this same directory, lexes `crates/*/src/**`
//! across every crate in the repo. Same precedent, same reason.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The workspace root, from this crate's manifest directory.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("the workspace root is two levels above this crate's manifest")
}

/// The `"crates/NAME"` entries of one top-level array in the workspace manifest.
///
/// A deliberately small scan rather than a TOML dependency: it reads from `key = [` to the first `]`,
/// strips `#` comments line by line (the `members` array in this repo carries a seven-line comment) and
/// collects the double-quoted strings. The caller asserts a floor on what comes back, so a scan that
/// stops matching is loud rather than empty.
fn manifest_array(manifest: &str, key: &str) -> BTreeSet<String> {
    let start = manifest
        .find(&format!("{key} = ["))
        .unwrap_or_else(|| panic!("the workspace manifest has no `{key} = [`"));
    let body = &manifest[start..];
    let end = body
        .find(']')
        .unwrap_or_else(|| panic!("`{key} = [` is never closed"));
    let mut out = BTreeSet::new();
    for line in body[..end].lines() {
        let code = line.split('#').next().unwrap_or("");
        let mut rest = code;
        while let Some(a) = rest.find('"') {
            let after = &rest[a + 1..];
            let Some(b) = after.find('"') else { break };
            out.insert(after[..b].to_string());
            rest = &after[b + 1..];
        }
    }
    out
}

/// `"crates/oracle-core"` -> `"oracle-core"`. Directory name, which is this repo's package name too.
fn crate_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// The `## Layout` section's text, from its heading to the next top-level heading.
///
/// Scoped rather than whole-file, and the reason is a caught mistake rather than a precaution: the first
/// version of this file scanned every line and picked up
/// `- **`crates/oracle-aether/tests/contract/`**` from the *documentation* section at the bottom, then
/// reported it as a crate that had been renamed away. A scan whose first failure is about the wrong
/// paragraph teaches the reader to distrust the next one.
fn layout_section(readme: &str) -> &str {
    let start = readme
        .find("\n## Layout\n")
        .expect("README.md has no `## Layout` section");
    let body = &readme[start + 1..];
    match body[1..].find("\n## ") {
        Some(end) => &body[..end + 2],
        None => body,
    }
}

/// The crates the README's Layout section names, from its `- **`crates/NAME`**` bullets.
///
/// A name containing `/` is a path to something inside a crate, not a crate, and is skipped: those are
/// legitimate bullets about directories, and treating one as a package name is how the whole-file scan
/// above went wrong.
fn readme_layout_bullets(readme: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in layout_section(readme).lines() {
        let Some(rest) = line.trim().strip_prefix("- **`crates/") else {
            continue;
        };
        let Some(name) = rest.split('`').next() else {
            continue;
        };
        if name.contains('/') || name.is_empty() {
            continue;
        }
        out.insert(name.to_string());
    }
    out
}

#[test]
fn the_readme_layout_section_names_exactly_the_workspace_members() {
    let root = root();
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("workspace Cargo.toml");
    let readme = std::fs::read_to_string(root.join("README.md")).expect("README.md");

    let members: BTreeSet<String> = manifest_array(&manifest, "members")
        .iter()
        .map(|p| crate_name(p))
        .collect();
    let excluded: BTreeSet<String> = manifest_array(&manifest, "exclude")
        .iter()
        .map(|p| crate_name(p))
        .collect();
    let bullets = readme_layout_bullets(&readme);

    assert!(
        members.len() >= 3,
        "COULD NOT MEASURE: only {} workspace members parsed out of Cargo.toml, so the manifest scan \
         is broken and not the README: {members:?}",
        members.len()
    );
    assert!(
        bullets.len() >= 3,
        "COULD NOT MEASURE: only {} Layout bullets parsed out of README.md, so the bullet scan is \
         broken and not the README: {bullets:?}",
        bullets.len()
    );

    // 1 and 2, reported as one difference each way so the message says which direction is wrong.
    let missing: Vec<&String> = members.difference(&bullets).collect();
    assert!(
        missing.is_empty(),
        "the README's Layout section does not mention {missing:?}, which the workspace BUILDS. This is \
         the front door: a crate with no paragraph here is a crate a newcomer never learns exists"
    );
    let phantom: Vec<&String> = bullets.difference(&members).collect();
    assert!(
        phantom.is_empty(),
        "the README's Layout section describes {phantom:?}, which is not a workspace member. Either the \
         crate was renamed or removed and its paragraph outlived it, or it belongs in `members`"
    );

    // 3: a directory in neither list.
    let dirs: BTreeSet<String> = std::fs::read_dir(root.join("crates"))
        .expect("crates/")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        dirs.len() >= members.len(),
        "COULD NOT MEASURE: `crates/` holds {} directories but the manifest names {} members, so the \
         directory walk is broken",
        dirs.len(),
        members.len()
    );
    let unaccounted: Vec<&String> = dirs
        .iter()
        .filter(|d| !members.contains(*d) && !excluded.contains(*d))
        .collect();
    assert!(
        unaccounted.is_empty(),
        "`crates/{unaccounted:?}` is in neither `members` nor `exclude`. Cargo ignores it, every \
         workspace-wide `fmt`, `clippy` and `test` skips it, and this test's other assertions cannot \
         see it: decide which list it belongs in"
    );

    // 4: an excluded directory is a fact about this repo, so the README says it exists.
    for d in dirs.iter().filter(|d| excluded.contains(*d)) {
        assert!(
            readme.contains(d),
            "`crates/{d}` is excluded from the workspace and the README never mentions it, so anyone \
             counting the directories under `crates/` gets a number this workspace does not have and \
             nothing explains the difference"
        );
    }

    // 5: a stated count is the member count. Words as well as digits, because prose says "Four".
    const WORDS: [&str; 11] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    ];
    let words: Vec<&str> = readme.split_whitespace().collect();
    let mut counts_checked = 0usize;
    for pair in words.windows(2) {
        if pair[1].trim_matches(|c: char| !c.is_alphanumeric()) != "crates" {
            continue;
        }
        let w = pair[0].trim_matches(|c: char| !c.is_alphanumeric());
        let stated = w
            .parse::<usize>()
            .ok()
            .or_else(|| WORDS.iter().position(|x| x.eq_ignore_ascii_case(w)));
        let Some(stated) = stated else { continue };
        counts_checked += 1;
        assert_eq!(
            stated,
            members.len(),
            "the README says {w:?} crates and the workspace has {}. This is the exact shape of the \
             defect being fixed: a number that was true when written, went stale on the next crate, and \
             stayed green because nothing checked it",
            members.len()
        );
    }
    // Not an assertion that a count EXISTS — the README is allowed to have none, and today it has none.
    // Recorded so a reader of a passing run knows which of the two it was.
    eprintln!(
        "{counts_checked} stated crate-count(s) checked against {} members; {} directories under \
         crates/, {} excluded",
        members.len(),
        dirs.len(),
        excluded.len()
    );
}
