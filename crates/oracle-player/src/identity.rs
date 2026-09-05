//! **Which build of this window a person is looking at** — the top bar's identity chip.
//!
//! # Why this module exists
//!
//! Booked as `F-STALE-BINARY-SILENT`. On 2026-09-05 the owner reported four faults in this window and
//! **two of them were not faults**: his binary was built 2026-09-03 19:23, the Objects-tab JSON fix landed
//! 09-04 00:54 and click-picking landed 09-05 07:37. He spent his attention reporting a fix he already had
//! but was not running, and a feature that did not exist in his copy — and a diagnosing agent was spent
//! before a one-command timestamp check settled it.
//!
//! **Nothing anywhere told him.** The asymmetry is the argument: this process already warns about a stale
//! ROM and a stale symbol listing, and `initialize` has carried `serverBuild` (contract `protocol.md` §2.1,
//! registered by §11.23) the whole time — but `serverBuild` appeared **nowhere** in `oracle-player`. We
//! built the freshness answer and never put it where the reader is.
//!
//! # What is displayed, and the one thing that is deliberately not
//!
//! The values come from [`oracle_aether::build_info`], which is compile-time by construction: `build.rs`
//! writes them into `$OUT_DIR` and `src/build_info.rs` `include!`s them. §2.1's ⚑ clause names exactly that
//! arrangement as conformant, and its point is the one this module depends on — *"a process which reads its
//! identity has an opinion about it, and an opinion can be stale, mismatched, or copied from a
//! neighbour."* Nothing here re-derives the identity; it only renders it.
//!
//! **A count is not displayed, and that is a decision.** `F-BANNER-INVITES-A-PIN` (aurora's O26,
//! 2026-08-29): our startup banner prints `aether: N methods advertised`, a consumer pinned
//! `methods === '35'` and *threw* `stale oracle-aether binary` on anything else — so the guard written to
//! detect staleness became the stale thing. The lesson is about the **observable**, not about printing: a
//! total changes for reasons unrelated to freshness and is identical across binaries that differ. That row
//! names `serverBuild` as the right answer, so rendering it here is what the row asks for rather than a
//! second instance of the hazard. The chip carries no total and no derived verdict word a consumer could
//! key on — only the raw facts, and §2.1 documents `id` as opaque and not to be parsed.
//!
//! # What this window CANNOT know, and says so
//!
//! Two limits, and both are stated in the hover rather than papered over. A confident staleness verdict
//! that is sometimes wrong is worse than an honest "here is the revision I was built from".
//!
//! 1. **Whether the checkout has moved since.** The process knows the revision it was *built from*. The
//!    revision on disk *now* is not answerable from inside the process — an installed binary may have no
//!    checkout beside it at all, and reading git at run time is the very shape §2.1 bars for these values.
//!    So the chip states the revision and leaves the comparison to the reader, who can make it.
//! 2. **⚑ `dirty` does not cover this crate.** `crates/oracle-aether/build.rs`'s `build_input_paths` scopes
//!    the flag to `oracle-aether/src`, `oracle-core/src`, the two manifests and `Cargo.lock` —
//!    `crates/oracle-player/src` is **not** in it, deliberately (the scope is the set it can declare
//!    `rerun-if-changed` over *completely*, which ruling M2 values above width). So uncommitted edits to
//!    this window's own sources do not raise `dirty`, and a bare "clean" here would be a false reassurance
//!    about the very thing the reader is looking at. The chip therefore marks dirty **positively** and says
//!    nothing at all when it is false, and the hover names the gap.
//!
//!    That caveat is **derived, not written down**: [`dirty_scope_covers_this_window`] asks the scope the
//!    build script actually emitted. Add this crate to `build_input_paths` and the caveat disappears on its
//!    own rather than going stale as prose.
//!
//! The revision the binary was built from *is* repo-wide and accurate — `build.rs` declares
//! `rerun-if-changed` on `HEAD`, on the ref it resolves to, on `packed-refs` and on the index, so a commit
//! anywhere in the tree (this crate included) moves it. That is precisely the fact the owner's case needed:
//! his window was built from a commit that predated the two fixes.
//!
//! # The binary's own timestamp
//!
//! The chip also carries how long ago the executable file was written, because that is the observable that
//! actually settled the incident and it is the one a human reads at a glance — a revision has to be
//! resolved before it means anything, "2d" does not. It is a plain `mtime` on
//! [`std::env::current_exe`], labelled in the hover as a fact about the *file* rather than about the
//! build: copying or `touch`ing a binary moves it without a rebuild. It is **not** part of the identity and
//! is never presented as such; when it cannot be read, it is simply absent.
//!
//! # Style
//!
//! P9 of `docs/2026-09-05-debug-window-style.md`: a panel never quotes the specification at the person
//! reading it. Every `§` in this module is in a doc comment, where the next editor needs it; no runtime
//! string carries one. `no_runtime_string_cites_the_specification` holds that.

use oracle_aether::build_info::{
    SERVER_BUILD_DIRTY, SERVER_BUILD_DIRTY_SCOPE, SERVER_BUILD_ID, SERVER_BUILD_SOURCE,
};
use std::sync::OnceLock;
use std::time::SystemTime;

/// How much of the 40-character revision the bar shows.
///
/// The bar is shared with the panel nav, the transport, the palette button and the status line, so the
/// full hash does not belong on it — but the hover carries it whole, and
/// `the_hover_carries_the_whole_revision` is what stops the abbreviation becoming the only form on offer.
/// Twelve rather than the customary seven: this is a display convenience, and an abbreviation a reader may
/// act on should not be one collisions are plausible in.
const SHORT_REV: usize = 12;

/// The revision half of the build id — everything before the first `+`.
///
/// The id is `<rev>+profile=…+target=…+features=…` under version control, and
/// `no-vcs+pkg=…+profile=…` without it. Split rather than parsed: the contract documents `id` as opaque,
/// and the only structure relied on here is the one its own SHOULD guarantees — the revision comes first
/// and whole.
fn revision() -> &'static str {
    SERVER_BUILD_ID.split('+').next().unwrap_or(SERVER_BUILD_ID)
}

/// The build-time selection that rides with the revision — everything after the first `+`.
fn configuration() -> &'static str {
    SERVER_BUILD_ID.split_once('+').map_or("", |(_, rest)| rest)
}

/// Whether the dirty flag's scope includes this crate's sources.
///
/// **Asked of the scope the build script emitted, never assumed.** `SERVER_BUILD_DIRTY_SCOPE` exists so a
/// consumer can re-derive what the flag means instead of pinning a duplicate list beside it; this is that
/// use. While the answer is `false`, the hover carries the caveat; the day someone adds this crate to
/// `build_input_paths`, the caveat stops being printed without anyone remembering to delete it.
fn dirty_scope_covers_this_window() -> bool {
    SERVER_BUILD_DIRTY_SCOPE
        .iter()
        .any(|p| p.contains("oracle-player"))
}

/// When the running executable file was written, read once.
///
/// `None` is a perfectly ordinary answer — a platform with no `current_exe`, a binary whose file has been
/// unlinked, a filesystem with no mtime — and it means the age is simply not shown. It never becomes a
/// guess.
fn exe_written() -> Option<SystemTime> {
    static AT: OnceLock<Option<SystemTime>> = OnceLock::new();
    *AT.get_or_init(|| {
        std::fs::metadata(std::env::current_exe().ok()?)
            .ok()?
            .modified()
            .ok()
    })
}

/// How long ago, coarsely, in the units a person reads at a glance.
///
/// A pure function of two instants so it is testable without a filesystem. `None` when `then` is in the
/// future (a clock that has gone backwards, a copied binary carrying a future mtime): an age it cannot
/// compute is not shown, rather than shown as zero.
fn age_text(now: SystemTime, then: SystemTime) -> Option<String> {
    let hours = now.duration_since(then).ok()?.as_secs() / 3600;
    Some(match hours {
        0 => "<1h".to_string(),
        1..=47 => format!("{hours}h"),
        _ => format!("{}d", hours / 24),
    })
}

/// **The bar's chip** — what a person sees without opening anything.
///
/// `build <rev> +local · <age>`, with each component present only when it is honestly available: the
/// `+local` marker only when the tree the build script measured had uncommitted changes, the age only when
/// the executable's mtime could be read.
pub fn chip() -> String {
    let mut s = format!("build {}", short_revision());
    if SERVER_BUILD_DIRTY == Some(true) {
        s.push_str(" +local");
    }
    if let Some(age) = exe_written().and_then(|w| age_text(SystemTime::now(), w)) {
        s.push_str(" · ");
        s.push_str(&age);
    }
    s
}

/// The revision as the chip shows it: abbreviated under version control, whole otherwise (the `no-vcs`
/// fallback is already short and truncating it would destroy the one thing it says).
///
/// `pub(crate)` for one reason: `a_client_reads_this_windows_top_bar_and_it_follows_the_run_state`
/// asserts this string reaches `emulator/screen_text` on the wire, and it must assert the value this
/// module computes rather than retype an abbreviation beside it. The full [`chip`] is the wrong thing for
/// that test to compare against — it carries the executable's age, which can tick over between the frame
/// the bar was painted and the line that reads it back.
pub(crate) fn short_revision() -> &'static str {
    let rev = revision();
    if SERVER_BUILD_SOURCE == "vcs" && rev.len() > SHORT_REV {
        &rev[..SHORT_REV]
    } else {
        rev
    }
}

/// **The hover** — the whole identity, and the two things it cannot tell you.
pub fn detail() -> String {
    let mut s = String::new();
    if SERVER_BUILD_SOURCE == "vcs" {
        s.push_str(&format!("Built from revision {}\n", revision()));
    } else {
        s.push_str(&format!(
            "Built without version control, so there is no revision to name — this build calls itself \
             {}\n",
            revision()
        ));
    }
    s.push_str(&format!("Configuration {}\n", configuration()));
    match SERVER_BUILD_DIRTY {
        Some(true) => s.push_str("The tree had uncommitted changes when this was built.\n"),
        Some(false) => s.push_str("No uncommitted changes were seen when this was built.\n"),
        None => {}
    }
    if let Some(age) = exe_written().and_then(|w| age_text(SystemTime::now(), w)) {
        s.push_str(&format!(
            "This program file was last written {age} ago (copying or touching a file moves that without \
             rebuilding it).\n"
        ));
    }
    s.push('\n');
    s.push_str(
        "It cannot tell whether your checkout has moved since. It knows the revision it was built from, \
         not the one on disk now — compare them yourself if a fix you expect seems to be missing.\n",
    );
    if !dirty_scope_covers_this_window() {
        s.push_str(
            "\nAnd the uncommitted-changes line above covers the emulator core and the bus, not this \
             window's own code, so it can say nothing while this window's sources have been edited.\n",
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// The chip's job in one line: name the build. A chip that named nothing would still be a chip.
    #[test]
    fn the_chip_names_the_revision_this_binary_was_built_from() {
        let c = chip();
        assert!(
            c.starts_with("build "),
            "the chip must say what it is — a bare hex string in a crowded bar is unreadable; got {c:?}"
        );
        assert!(
            c.contains(short_revision()),
            "the chip must carry the revision itself, which is the entire point of it; got {c:?}"
        );
        assert!(
            !short_revision().is_empty(),
            "the build id yielded an empty revision, so the chip would say `build ` and nothing else — \
             every assertion in this file would still pass"
        );
    }

    /// **The abbreviation must not be the only form on offer.** A twelve-character prefix is a display
    /// convenience; a reader resolving it back to a commit needs the whole thing, and §2.1's SHOULD says
    /// the same about the wire.
    #[test]
    fn the_hover_carries_the_whole_revision() {
        let d = detail();
        assert!(
            d.contains(revision()),
            "the hover must carry the FULL revision, not the chip's abbreviation — the chip is where a \
             reader glances and the hover is where they go to act; got:\n{d}"
        );
        if SERVER_BUILD_SOURCE == "vcs" {
            assert_eq!(
                revision().len(),
                40,
                "a `vcs` build id must start with the full 40-character hash; got {:?}",
                revision()
            );
        }
    }

    /// ⚑ **The load-bearing row.** The dirty marker is a claim about the tree, and a chip that carried it
    /// unconditionally — or never — would look exactly like this one on the glass.
    #[test]
    fn the_local_changes_marker_tracks_the_flag_and_is_not_decoration() {
        assert_eq!(
            chip().contains("+local"),
            SERVER_BUILD_DIRTY == Some(true),
            "the chip's `+local` marker must mean the build script measured uncommitted changes, and \
             must be absent otherwise. A marker that is always there says nothing; one that is never \
             there is a false reassurance."
        );
    }

    /// ⚑ **The honesty this module exists to preserve.** The flag's scope excludes this crate, so a reader
    /// who takes "no uncommitted changes" at face value is being misled about the window in front of them.
    /// The caveat is derived from the scope, so this row also fails if the derivation stops working.
    #[test]
    fn the_hover_admits_the_dirty_flag_does_not_cover_this_window() {
        assert!(
            !SERVER_BUILD_DIRTY_SCOPE.is_empty(),
            "the build script emitted no dirty scope, so the derivation below reads nothing and the \
             caveat's presence would prove nothing"
        );
        let covered = dirty_scope_covers_this_window();
        let d = detail();
        assert_eq!(
            d.contains("not this window's own code"),
            !covered,
            "the caveat must appear exactly when the scope excludes this crate. Scope was {:?}",
            SERVER_BUILD_DIRTY_SCOPE
        );
        // The state of the tree today, asserted so that closing the gap is a deliberate act that has to
        // come back through this test rather than a silent change of meaning.
        assert!(
            !covered,
            "`crates/oracle-player` has been added to `build_input_paths` — good, but the module doc and \
             this row's premise both describe the old scope and need rewriting with it"
        );
    }

    /// The other limit, stated rather than implied. A window that showed a revision and said nothing else
    /// invites the reader to assume it is comparing against something.
    #[test]
    fn the_hover_says_it_cannot_see_the_checkout() {
        let d = detail();
        assert!(
            d.contains("cannot tell whether your checkout has moved"),
            "the hover must decline the comparison out loud — a revision presented with no caveat reads \
             as a freshness verdict; got:\n{d}"
        );
    }

    /// P9: the reader at the window is not holding the specification.
    #[test]
    fn no_runtime_string_cites_the_specification() {
        for (what, s) in [("chip", chip()), ("hover", detail())] {
            for bad in ["§", "protocol.md", "serverBuild"] {
                assert!(
                    !s.contains(bad),
                    "the {what} contains {bad:?}. A section reference is addressed to somebody holding \
                     the contract, and the person at this window is not; got:\n{s}"
                );
            }
        }
    }

    #[test]
    fn an_age_is_coarse_and_never_guessed() {
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        let at = |secs| age_text(t0 + Duration::from_secs(secs), t0);
        assert_eq!(at(0).as_deref(), Some("<1h"));
        assert_eq!(at(3599).as_deref(), Some("<1h"));
        assert_eq!(at(3600).as_deref(), Some("1h"));
        assert_eq!(at(47 * 3600).as_deref(), Some("47h"));
        assert_eq!(at(48 * 3600).as_deref(), Some("2d"));
        assert_eq!(at(10 * 24 * 3600).as_deref(), Some("10d"));
        assert_eq!(
            age_text(t0, t0 + Duration::from_secs(60)),
            None,
            "a file written in the future is a clock that cannot be trusted; the age must then be absent \
             rather than reported as zero"
        );
    }

    /// The two halves of the id are split, not parsed — but the split must actually separate them, or the
    /// hover's "Configuration" line quietly becomes empty and nobody notices.
    #[test]
    fn the_configuration_rides_with_the_revision() {
        if SERVER_BUILD_SOURCE != "vcs" {
            return;
        }
        let c = configuration();
        for key in ["profile=", "target=", "features="] {
            assert!(
                c.contains(key),
                "`{key}` is missing from the configuration half {c:?}, so the hover would show a reader \
                 an identity that does not distinguish two builds that behave differently"
            );
        }
        assert!(
            !revision().contains('+'),
            "the revision half still carries configuration, so the chip would show it"
        );
    }
}
