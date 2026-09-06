//! **`assets/install-desktop.sh` never installs a second entry for a window class somebody else claims.**
//!
//! A `StartupWMClass` is how a desktop decides which launcher a running window belongs to, so two entries
//! declaring one class leave the icon, the name and the task-switcher grouping to a coin toss. The script
//! shipped exactly that: it wrote `oracle-frontend.desktop` and copied it to `oracle.desktop`, two of its
//! own entries under one class, and it also wrote over the top of a user's hand-made entry for the same
//! window without noticing.
//!
//! ⚑ **What this test does if the skip is deleted.** The fixture puts a foreign entry claiming
//! `oracle-frontend` in the applications directory before the script runs. With the skip gone the script
//! writes `oracle-frontend.desktop` and [`a_foreign_entrys_class_is_not_taken_over`]'s first assertion is
//! red. The two assertions after it exist because a skip is also achieved by the script failing outright,
//! which would be a green nobody wanted: the player entry must still be installed, and the run must exit
//! 0. And [`with_nothing_claiming_the_class_both_entries_install`] is the control that says the fixture
//! permits an install at all, so the red above is about the conflict and not about the rig.
//!
//! **It never touches a real home directory.** `HOME` and the three XDG roots are redirected into a
//! temporary tree that this file creates and removes, and `PATH` is prefixed with stubs for the three
//! desktop-cache refreshers so a suite run cannot rebuild the session's caches as a side effect.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The script under test, addressed from this crate rather than from a guessed repository root.
fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("install-desktop.sh")
}

/// A temporary tree, unique per test, removed by [`Rig::drop`].
struct Rig {
    root: PathBuf,
}

impl Rig {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "oracle-install-desktop-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("a clock after 1970")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("share/applications")).expect("temp tree");

        // Stubs for the three cache refreshers, first on PATH. Without these a suite run would shell out
        // to the real `kbuildsycoca6` and rebuild the session's own caches, which is a side effect no
        // test is entitled to.
        let stubs = root.join("bin");
        fs::create_dir_all(&stubs).expect("stub dir");
        for tool in [
            "update-desktop-database",
            "gtk-update-icon-cache",
            "kbuildsycoca6",
        ] {
            let p = stubs.join(tool);
            fs::write(&p, "#!/bin/sh\nexit 0\n").expect("stub");
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).expect("stub mode");
        }
        Self { root }
    }

    fn apps(&self) -> PathBuf {
        self.root.join("share/applications")
    }

    /// Run the script with both binary paths pointed at something that really is executable, so the
    /// `[ -x ]` guards pass and both entries are candidates.
    fn run(&self) -> (bool, String) {
        let path = format!(
            "{}:{}",
            self.root.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let out = Command::new("bash")
            .arg(script())
            .arg("/bin/sh")
            .arg("/bin/sh")
            .env("HOME", &self.root)
            .env("XDG_DATA_HOME", self.root.join("share"))
            .env("XDG_CACHE_HOME", self.root.join("cache"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("PATH", path)
            .output()
            .expect("the script is runnable");
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        (out.status.success(), text)
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// The defect, as a gate: a hand-made entry already owns `oracle-frontend`, so the script adds no rival.
#[test]
fn a_foreign_entrys_class_is_not_taken_over() {
    let rig = Rig::new("conflict");
    fs::write(
        rig.apps().join("someones-own.desktop"),
        "[Desktop Entry]\nType=Application\nName=Oracle (s4 debug)\nExec=/somewhere/oracle-debug\n\
         Icon=oracle-debug\nStartupWMClass=oracle-frontend\n",
    )
    .expect("the fixture entry");

    let (ok, text) = rig.run();

    assert!(
        !rig.apps().join("oracle-frontend.desktop").exists(),
        "a second entry was installed for a class someone else already declares, which is the defect \
         this gate exists for. Output:\n{text}"
    );
    // A skip achieved by the script falling over is not the skip that was asked for.
    assert!(ok, "the script did not exit 0. Output:\n{text}");
    assert!(
        rig.apps().join("oracle-player.desktop").exists(),
        "the uncontested entry was not installed either, so the run refused everything rather than the \
         one thing that conflicted. Output:\n{text}"
    );
    assert!(
        text.contains("someones-own.desktop"),
        "the conflicting file was not named, so a reader cannot act on the skip. Output:\n{text}"
    );
    // The historical name is the script's own self-collision and is never created.
    assert!(
        !rig.apps().join("oracle.desktop").exists(),
        "the legacy copy is back, and it declares the same class as the entry above. Output:\n{text}"
    );
}

/// The control. With nothing claiming either class the script installs both entries, so the assertion
/// above is about the conflict rather than about a rig in which nothing installs.
#[test]
fn with_nothing_claiming_the_class_both_entries_install() {
    let rig = Rig::new("clean");
    let (ok, text) = rig.run();

    assert!(ok, "the script did not exit 0. Output:\n{text}");
    for entry in ["oracle-frontend.desktop", "oracle-player.desktop"] {
        assert!(
            rig.apps().join(entry).exists(),
            "{entry} was not installed on a clean machine, so this rig cannot witness an install and \
             the conflict gate beside it proves nothing. Output:\n{text}"
        );
    }
    assert!(
        !rig.apps().join("oracle.desktop").exists(),
        "the legacy name was created fresh on a machine that had no old install to migrate, which is \
         the script colliding with itself. Output:\n{text}"
    );
    // Each installed entry points at the binary it was given, rather than at the template's placeholder.
    let front = fs::read_to_string(rig.apps().join("oracle-frontend.desktop")).expect("read back");
    assert!(
        front.contains("Exec=/bin/sh %f"),
        "the Exec line was not rewritten to the binary passed in:\n{front}"
    );
}

/// `--dry-run` reports the same decision it would act on, and writes nothing at all.
#[test]
fn a_dry_run_names_every_file_it_would_create_and_creates_none() {
    let rig = Rig::new("dry");
    let path = format!(
        "{}:{}",
        rig.root.join("bin").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new("bash")
        .arg(script())
        .arg("--dry-run")
        .arg("/bin/sh")
        .arg("/bin/sh")
        .env("HOME", &rig.root)
        .env("XDG_DATA_HOME", rig.root.join("share"))
        .env("XDG_CACHE_HOME", rig.root.join("cache"))
        .env("XDG_CONFIG_HOME", rig.root.join("config"))
        .env("PATH", path)
        .output()
        .expect("the script is runnable");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();

    assert!(out.status.success(), "dry run did not exit 0:\n{text}");
    for named in [
        "oracle-frontend.desktop",
        "oracle-player.desktop",
        "256x256/apps/oracle.png",
        "scalable/apps/oracle.svg",
    ] {
        assert!(
            text.contains(named),
            "the dry run did not name {named}, so its list is not the list of what a real run creates:\n{text}"
        );
    }
    assert!(
        !rig.apps().join("oracle-player.desktop").exists()
            && !rig.root.join("share/icons").exists(),
        "a dry run wrote something"
    );
}
