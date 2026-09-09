//! Where a `--restamp` repair is allowed to land, and what it takes to put it there.
//!
//! # The default is to write nothing
//!
//! `--restamp` with no `--out` is a pure dry run: the patch report goes to stdout, no file is created or
//! touched. That is not politeness. The inputs this tool reads are another repository's build outputs, and
//! the fixture it repairs sits next to `sonic3k.state0` and `sonic3k.srm` — the owner's own artifacts. A
//! tool that boots a ROM and writes into the tree it came from, by default, is a tool that will one day
//! surprise someone.
//!
//! # Two independent guards, because they catch different mistakes
//!
//! 1. **[`SourceGuard`]** — a write that resolves inside the git repository any INPUT came from — the
//!    ROM, the listing, or the fixture being repaired — requires `--allow-source-write`. The fixture is
//!    named explicitly because it is the file the repair rewrites and the one whose neighbours this
//!    module's opening paragraph is about; it can also sit in a different checkout from the ROM, and for
//!    a while it was not in the list at all (M47). "Resolves inside" means after symlinks, not after
//!    text. The *repository* is the boundary, not "the directory holding the
//!    ROM": a file does not define a tree root, and the parent-directory rule both under-protects (a ROM
//!    at the repo root protects only that one level) and over-protects surprisingly (copy the ROM into
//!    your working directory and the tool refuses to write its own report beside it).
//! 2. **Overwrite** — an existing file requires `--force`. Orthogonal to the first: the mistake it catches
//!    is clobbering a report you meant to keep, which has nothing to do with whose tree it is in.
//!
//! Both are decided by [`SourceGuard::check`], which is pure — it is handed the facts (does the path
//! resolve inside a protected root, does the file already exist) rather than looking them up — so every
//! refusal is unit-tested without a filesystem.

use std::path::{Path, PathBuf};

/// Whether a candidate write is permitted, and why not when it is not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteRefusal {
    /// The path resolves inside a protected repository.
    InsideSourceRepo { repo: PathBuf },
    /// The file is already there.
    WouldOverwrite,
}

/// The write policy for one `--restamp` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceGuard {
    /// Repository roots that count as "the owner's tree". Empty means nothing is protected — which
    /// happens only when neither input is inside a git repository.
    pub protected: Vec<PathBuf>,
    pub allow_source_write: bool,
    pub force: bool,
}

impl SourceGuard {
    /// Decide one write. `resolved` must already be absolute (see [`resolve_for_guard`]).
    pub fn check(&self, resolved: &Path, exists: bool) -> Result<(), WriteRefusal> {
        if !self.allow_source_write {
            if let Some(repo) = self.protected.iter().find(|r| resolved.starts_with(r)) {
                return Err(WriteRefusal::InsideSourceRepo { repo: repo.clone() });
            }
        }
        if exists && !self.force {
            return Err(WriteRefusal::WouldOverwrite);
        }
        Ok(())
    }

    /// Decide one write against the real filesystem, rendering the refusal as the message the user sees.
    pub fn check_path(&self, target: &Path) -> Result<(), String> {
        let resolved = resolve_for_guard(target);
        match self.check(&resolved, target.exists()) {
            Ok(()) => Ok(()),
            Err(WriteRefusal::InsideSourceRepo { repo }) => Err(format!(
                "REFUSING to write {}: it resolves inside {}, a repository this run's inputs came from \
                 (note that it resolves there after symlinks, which the path itself may not show). \
                 Those are the owner's artifacts, and a re-stamp is a change to review before it \
                 lands. Write the repair somewhere else and apply it deliberately, or pass \
                 --allow-source-write if writing in place is genuinely what you want.",
                target.display(),
                repo.display()
            )),
            Err(WriteRefusal::WouldOverwrite) => Err(format!(
                "REFUSING to overwrite {}: it already exists. Pass --force if replacing it is what you \
                 want.",
                target.display()
            )),
        }
    }
}

/// Resolve a candidate write path for the containment check: **lexically first, then through the
/// symlinks that actually exist.**
///
/// **M47.** This used to be the lexical half alone, which meant the guard could be walked straight past:
/// a `--out` reaching the protected repository through a symlink — `ln -s ~/aeon /tmp/tree`, then
/// `--out /tmp/tree/games/.../ojz.bin` — resolved to a path starting `/tmp`, matched no protected root,
/// and landed inside the very tree the guard exists to protect. A containment check that is decided on
/// the spelling of a path rather than on where the path leads is not a containment check.
///
/// Both halves are needed and neither is sufficient:
///
/// * **lexical `..`** because `canonicalize` refuses a file that is not there yet, which is every file
///   this tool is about to create, and because `aeon/../scratch/x` must not be judged inside `aeon`;
/// * **`canonicalize` on the longest existing ancestor**, with the not-yet-existing tail re-attached,
///   because that is the only thing that follows a link.
pub fn resolve_for_guard(p: &Path) -> PathBuf {
    follow_existing_symlinks(&resolve_lexically(p))
}

/// The lexical half of [`resolve_for_guard`]: absolute, `.` dropped, `..` popped, nothing touched on
/// disk. Public so a test can show the two halves disagree — a symlink test that could have passed on
/// the lexical answer alone would be witnessing nothing.
pub fn resolve_lexically(p: &Path) -> PathBuf {
    use std::path::Component;
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(p)
    };
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Canonicalize the longest prefix of `lexical` that exists on disk and re-attach the rest verbatim.
///
/// The tail is the part being created, so it has no links to follow; the prefix is where a link can
/// hide. Falls back to the lexical path unchanged when nothing resolves — which is a *weaker* answer,
/// never a wrong one, because the lexical path is what the check used to run on.
fn follow_existing_symlinks(lexical: &Path) -> PathBuf {
    let mut prefix = lexical.to_path_buf();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(real) = prefix.canonicalize() {
            let mut out = real;
            for name in tail.iter().rev() {
                out.push(name);
            }
            return out;
        }
        let Some(name) = prefix.file_name().map(|n| n.to_os_string()) else {
            return lexical.to_path_buf();
        };
        tail.push(name);
        if !prefix.pop() {
            return lexical.to_path_buf();
        }
    }
}

/// The git repository a path belongs to: the nearest ancestor holding a `.git` entry (a directory for an
/// ordinary clone, a file for a worktree). `None` when there is none.
pub fn enclosing_repo(p: &Path) -> Option<PathBuf> {
    let mut cur = resolve_for_guard(p);
    if cur.is_file() {
        cur.pop();
    }
    loop {
        if cur.join(".git").exists() {
            return Some(cur);
        }
        if !cur.pop() {
            return None;
        }
    }
}

/// The guard for one `--restamp` invocation, assembled from **every** file the run was handed.
///
/// **M47.** The call site used to pass the ROM and the listing and stop there, while this module's own
/// doc names *the fixture's neighbours* — `sonic3k.state0`, `sonic3k.srm` — as the thing being
/// protected. `--fixture-bin` is the file the repair rewrites, so its repository is the one most
/// certain to be the owner's; leaving it out meant a fixture from a different checkout than the ROM
/// left that checkout entirely unprotected. Assembling the list here rather than at the call site is
/// what lets a test see which inputs a run protects.
pub fn guard_for_run(
    rom: &Path,
    lst: &Path,
    fixture: Option<&Path>,
    allow_source_write: bool,
    force: bool,
) -> SourceGuard {
    let mut inputs: Vec<&Path> = vec![rom, lst];
    inputs.extend(fixture);
    guard_for_inputs(&inputs, allow_source_write, force)
}

/// Build the guard for a run: whatever repositories the inputs live in are protected.
pub fn guard_for_inputs(inputs: &[&Path], allow_source_write: bool, force: bool) -> SourceGuard {
    let mut protected: Vec<PathBuf> = Vec::new();
    for p in inputs {
        if let Some(r) = enclosing_repo(p) {
            if !protected.contains(&r) {
                protected.push(r);
            }
        }
    }
    SourceGuard {
        protected,
        allow_source_write,
        force,
    }
}

/// One artifact to write.
pub struct Artifact<'a> {
    pub path: PathBuf,
    pub what: &'a str,
    pub bytes: Vec<u8>,
}

/// Check every artifact against the guard **before writing any of them**, then write them all.
///
/// The two phases are deliberate: a half-written repair (patch report present, re-stamped fixture refused)
/// is a worse state than no repair, and the check is free.
pub fn write_all(guard: &SourceGuard, artifacts: &[Artifact<'_>]) -> Result<(), String> {
    for a in artifacts {
        guard.check_path(&a.path)?;
    }
    for a in artifacts {
        if let Some(dir) = a.path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        std::fs::write(&a.path, &a.bytes)
            .map_err(|e| format!("cannot write the {} to {}: {e}", a.what, a.path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard(protected: &[&str], allow: bool, force: bool) -> SourceGuard {
        SourceGuard {
            protected: protected.iter().map(PathBuf::from).collect(),
            allow_source_write: allow,
            force,
        }
    }

    /// The whole point: the sibling repository's tree is not written to by accident.
    #[test]
    fn a_write_inside_the_source_repo_is_refused_by_default() {
        let g = guard(&["/home/u/aeon"], false, false);
        assert_eq!(
            g.check(
                Path::new("/home/u/aeon/games/sonic4/data/replays/ojz.bin"),
                true
            ),
            Err(WriteRefusal::InsideSourceRepo {
                repo: PathBuf::from("/home/u/aeon")
            })
        );
        // …and the flag is what unlocks it, at which point the overwrite guard still applies.
        let g = guard(&["/home/u/aeon"], true, false);
        assert_eq!(
            g.check(Path::new("/home/u/aeon/x.bin"), true),
            Err(WriteRefusal::WouldOverwrite)
        );
        let g = guard(&["/home/u/aeon"], true, true);
        assert_eq!(g.check(Path::new("/home/u/aeon/x.bin"), true), Ok(()));
    }

    /// Somewhere else entirely is fine without any flag — the default must not be so cautious that the
    /// tool cannot write its own report.
    #[test]
    fn a_write_outside_every_protected_repo_needs_no_flag() {
        let g = guard(&["/home/u/aeon"], false, false);
        assert_eq!(g.check(Path::new("/tmp/restamp/patch.txt"), false), Ok(()));
    }

    /// The two guards are independent: an existing file outside the repo still needs `--force`.
    #[test]
    fn overwriting_needs_force_wherever_the_file_is() {
        let g = guard(&["/home/u/aeon"], false, false);
        assert_eq!(
            g.check(Path::new("/tmp/patch.txt"), true),
            Err(WriteRefusal::WouldOverwrite)
        );
        assert_eq!(
            guard(&[], false, true).check(Path::new("/tmp/patch.txt"), true),
            Ok(())
        );
    }

    /// A prefix match on unnormalized text would call `/home/u/aeon-scratch` "inside `/home/u/aeon`".
    /// `starts_with` on `Path` compares whole components, and this pins that.
    #[test]
    fn a_sibling_directory_with_a_shared_prefix_is_not_inside_the_repo() {
        let g = guard(&["/home/u/aeon"], false, false);
        assert_eq!(
            g.check(Path::new("/home/u/aeon-scratch/p.txt"), false),
            Ok(())
        );
    }

    /// `..` must be resolved before containment is judged, or `aeon/../scratch` reads as inside `aeon`.
    #[test]
    fn parent_components_are_resolved_before_the_containment_check() {
        assert_eq!(
            resolve_for_guard(Path::new("/home/u/aeon/../scratch/p.txt")),
            PathBuf::from("/home/u/scratch/p.txt")
        );
        let g = guard(&["/home/u/aeon"], false, false);
        assert_eq!(
            g.check(
                &resolve_for_guard(Path::new("/home/u/aeon/../scratch/p.txt")),
                false
            ),
            Ok(())
        );
        // …and the reverse: a path that only *looks* outside is still caught.
        assert_eq!(
            g.check(
                &resolve_for_guard(Path::new("/home/u/scratch/../aeon/p.txt")),
                false
            ),
            Err(WriteRefusal::InsideSourceRepo {
                repo: PathBuf::from("/home/u/aeon")
            })
        );
    }

    /// This crate's own tree is a git repository, so the discovery walk has something real to find.
    #[test]
    fn the_enclosing_repository_is_found_by_walking_up() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let repo = enclosing_repo(here).expect("this crate is inside a git repository");
        assert!(repo.join(".git").exists());
        assert!(resolve_for_guard(here).starts_with(&repo));
    }

    /// A scratch directory of our own, canonicalized so the assertions below are about the symlink
    /// under test and not about whatever `/tmp` happens to be on the host.
    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let d = std::env::temp_dir().join(format!(
            "replay-guard-{tag}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("create the scratch directory");
        d.canonicalize()
            .expect("canonicalize the scratch directory")
    }

    /// **M47, hole one.** The containment check must be decided by where a path LEADS, not by how it is
    /// spelled. `..` was resolved and symlinks were not, so `--out` through a link into the protected
    /// repository resolved to a path outside every protected root and landed inside the tree the guard
    /// exists to protect.
    ///
    /// The lexical assertion is the clause that keeps this honest: if the link's own path already
    /// started with the repository, the refusal below would prove nothing about following it.
    #[test]
    fn a_write_reaching_the_source_repo_through_a_symlink_is_refused() {
        let base = scratch("symlink");
        let repo = base.join("aeon");
        std::fs::create_dir_all(repo.join("games")).expect("create the fake repo");
        std::fs::write(repo.join(".git"), "gitdir: elsewhere\n")
            .expect("a worktree-style .git file");
        std::fs::write(repo.join("s4.bin"), b"rom").expect("write the input ROM");

        let link = base.join("innocent-looking");
        std::os::unix::fs::symlink(&repo, &link).expect("create the symlink");
        let via_link = link.join("games/ojz.bin");

        // The clause that stops this test agreeing with itself: on the spelling alone, this write is
        // nowhere near the repository.
        assert!(
            !resolve_lexically(&via_link).starts_with(&repo),
            "the lexical path must NOT look like it is inside {} — otherwise this test would pass \
             without following the link at all: {}",
            repo.display(),
            resolve_lexically(&via_link).display()
        );
        assert!(
            resolve_for_guard(&via_link).starts_with(&repo),
            "…but resolving it must land inside the repo: {}",
            resolve_for_guard(&via_link).display()
        );

        let g = guard_for_inputs(&[&repo.join("s4.bin")], false, false);
        assert_eq!(
            g.protected,
            vec![repo.clone()],
            "the input's repo is protected"
        );
        assert!(
            g.check_path(&via_link).is_err(),
            "a write reaching {} through a symlink must be refused",
            repo.display()
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// **M47, hole two.** `--fixture-bin` is the file the repair rewrites, and it can sit in a different
    /// checkout from the ROM. The call site passed the ROM and the listing only, so that checkout was
    /// not protected at all — while this module's doc names the fixture's neighbours as the thing being
    /// protected.
    ///
    /// Two separate repositories, so "protected" cannot be inherited from the ROM's.
    #[test]
    fn the_fixture_repository_is_protected_alongside_the_roms() {
        let base = scratch("fixture");
        let rom_repo = base.join("aeon");
        let fixture_repo = base.join("sonic3k-fixtures");
        for r in [&rom_repo, &fixture_repo] {
            std::fs::create_dir_all(r).expect("create a fake repo");
            std::fs::write(r.join(".git"), "gitdir: elsewhere\n").expect(".git file");
        }
        let rom = rom_repo.join("s4.bin");
        let lst = rom_repo.join("s4.lst");
        let fixture = fixture_repo.join("ojz.bin");
        for f in [&rom, &lst, &fixture] {
            std::fs::write(f, b"x").expect("write an input");
        }
        assert_ne!(
            rom_repo, fixture_repo,
            "the two inputs must be in different repositories, or this measures one repo twice"
        );

        let g = guard_for_run(&rom, &lst, Some(&fixture), false, false);
        assert!(
            g.protected.contains(&fixture_repo),
            "the fixture's repository must be protected; protected = {:?}",
            g.protected
        );
        assert!(
            g.check_path(&fixture_repo.join("sonic3k.srm")).is_err(),
            "…so a write next to the fixture is refused"
        );

        // The control: without a fixture the fixture's repo is NOT protected, so the assertion above
        // is about the new argument and not about the discovery walk finding everything anyway.
        let g = guard_for_run(&rom, &lst, None, false, false);
        assert!(
            !g.protected.contains(&fixture_repo),
            "with no fixture, its repo must not be protected; protected = {:?}",
            g.protected
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}
