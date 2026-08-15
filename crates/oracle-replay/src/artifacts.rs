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
//! 1. **[`SourceGuard`]** — a write that resolves inside the git repository the ROM or listing came from
//!    requires `--allow-source-write`. The *repository* is the boundary, not "the directory holding the
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
                "REFUSING to write {} — it resolves inside {}, the repository the ROM and listing came \
                 from. Those are the owner's artifacts, and a re-stamp is a change to review before it \
                 lands. Write the repair somewhere else and apply it deliberately, or pass \
                 --allow-source-write if writing in place is genuinely what you want.",
                target.display(),
                repo.display()
            )),
            Err(WriteRefusal::WouldOverwrite) => Err(format!(
                "REFUSING to overwrite {} — it already exists. Pass --force if replacing it is what you \
                 want.",
                target.display()
            )),
        }
    }
}

/// Make a path absolute and lexically normal **without** requiring it to exist (`canonicalize` refuses a
/// file that is not there yet, which is every file this tool is about to create).
///
/// `..` is resolved lexically, which is what a containment check needs: `aeon/../scratch/x` must not be
/// judged as "inside aeon".
pub fn resolve_for_guard(p: &Path) -> PathBuf {
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
}
