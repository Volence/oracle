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
        // `zenity` and `notify-send` are stubbed for the same reason and one more: the launcher these
        // entries now run puts a dialog on screen when it refuses, and a test suite is not entitled to
        // open a window on the machine running it. The stubs are silent and successful, so the
        // file-chooser branch returns an empty selection and the refusal below it is what gets measured.
        for tool in [
            "update-desktop-database",
            "gtk-update-icon-cache",
            "kbuildsycoca6",
            "zenity",
            "notify-send",
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

    /// The stubbed `PATH` these runs use, so nothing here reaches the session's real `zenity`,
    /// `notify-send` or desktop-cache tools.
    fn path(&self) -> String {
        format!(
            "{}:{}",
            self.root.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        )
    }

    /// Run the script with both binary paths pointed at something that really is executable, so the
    /// `[ -x ]` guards pass and both entries are candidates.
    fn run(&self) -> (bool, String) {
        self.run_with("/bin/sh", "/bin/sh")
    }

    /// The same run, with the two binary paths chosen by the caller. Used by the parser-derived tests
    /// below, which need entries naming a binary they can actually run.
    fn run_with(&self, frontend: &str, player: &str) -> (bool, String) {
        let path = self.path();
        let out = Command::new("bash")
            .arg(script())
            .arg(frontend)
            .arg(player)
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
    // Each installed entry names the binary it was given, and no placeholder survives the substitution.
    for entry in ["oracle-frontend.desktop", "oracle-player.desktop"] {
        let body = fs::read_to_string(rig.apps().join(entry)).expect("read back");
        let exec = exec_line(&body);
        assert!(
            exec.contains("/bin/sh"),
            "{entry}: the Exec line does not name the binary passed in:\n{exec}"
        );
        assert!(
            !exec.contains('@'),
            "{entry}: a template placeholder survived substitution, so the entry would launch nothing:\n\
             {exec}"
        );
        assert!(
            exec.contains("oracle-launch.sh"),
            "{entry}: the Exec line does not run the launcher, so a click with no file selected reaches \
             a binary that refuses to start without a ROM:\n{exec}"
        );
    }
}

/// The one `Exec=` line of a `.desktop` body.
fn exec_line(body: &str) -> String {
    body.lines()
        .find(|l| l.starts_with("Exec="))
        .unwrap_or_else(|| panic!("no Exec= line in:\n{body}"))
        .to_string()
}

/// ⚑ **The hand-made `oracle-debug` launcher is REPORTED and never touched.**
///
/// Measured on the owner's machine, 2026-09-09: `oracle-debug.desktop` declares
/// `StartupWMClass=oracle-frontend`, which is why the frontend entry is skipped there; it runs a shell
/// script that execs the minifb `oracle-frontend`; and that script's already-serving guard is
/// `pgrep -f 'oracle-frontend'`, blind to an `oracle-player` holding the same socket. All three are
/// reasons to retire it and none of them is a licence to delete a file in somebody's home directory.
///
/// This test pins both halves. A report that quietly removed the thing it reported would pass an
/// assertion about its own output while doing the one thing this script has always refused to do, so the
/// survival of the file is asserted beside the words.
#[test]
fn a_hand_made_oracle_debug_launcher_is_reported_and_left_alone() {
    let rig = Rig::new("oracle-debug");
    let entry = rig.apps().join("oracle-debug.desktop");
    fs::write(
        &entry,
        "[Desktop Entry]\nType=Application\nName=Oracle (s4 debug)\nExec=/home/x/.local/bin/oracle-debug\n\
         Icon=oracle-debug\nStartupWMClass=oracle-frontend\n",
    )
    .expect("the fixture entry");

    let (ok, text) = rig.run();

    assert!(ok, "the script did not exit 0. Output:\n{text}");
    assert!(
        entry.exists(),
        "the script REMOVED a launcher it does not own. Output:\n{text}"
    );
    assert!(
        text.contains("oracle-debug.desktop"),
        "the hand-made launcher was not named, so a reader cannot act on the note. Output:\n{text}"
    );
    assert!(
        text.contains("Retiring it is the suggestion"),
        "the note does not say what it recommends, which leaves the reader with a fact and no next \
         step. Output:\n{text}"
    );
    assert!(
        text.contains(&format!("rm {}", entry.display())),
        "the note does not give the exact command, so acting on it means guessing a path. Output:\n\
         {text}"
    );
    // The player entry must still install: a note is not a refusal.
    assert!(
        rig.apps().join("oracle-player.desktop").exists(),
        "the note stopped the install it was attached to. Output:\n{text}"
    );
}

/// The `Exec=` line as an argv, with the `%f` field code replaced by `rom` (which is what a desktop does
/// for "Open With") or dropped entirely when `rom` is `None` (which is what a plain icon click does).
fn exec_argv(exec: &str, rom: Option<&str>) -> Vec<String> {
    exec.trim_start_matches("Exec=")
        .split_whitespace()
        .filter_map(|tok| match (tok, rom) {
            ("%f", Some(r)) => Some(r.to_string()),
            ("%f", None) => None,
            _ => Some(tok.to_string()),
        })
        .collect()
}

/// ⚑ **The generated frontend `Exec=` is one this binary's OWN parser accepts, and the check asks the
/// parser rather than matching a string.**
///
/// The installed entry is read back, `%f` is expanded the way a desktop expands it, and the launcher is
/// asked (with `--print-argv`, which resolves everything and launches nothing) for the argv it would
/// hand the binary. That argv then goes to the real `oracle-frontend`, whose verdict is the assertion.
/// A string comparison here would only prove the script wrote what this file expected; it could not
/// notice the CLI changing underneath it, which is the failure mode that produced the defect.
///
/// **No window opens.** The ROM file is deleted between resolving the argv and running the binary, so
/// the process dies at `fs::read` with `cannot read ROM`, which is well before any window or socket. That
/// message is also the positive control: it is only reachable once parsing has SUCCEEDED, so its absence
/// would mean the run never got that far and the two negative assertions proved nothing.
#[test]
fn the_generated_frontend_exec_is_an_argv_the_frontends_own_parser_accepts() {
    let rig = Rig::new("frontend-exec");
    let bin = env!("CARGO_BIN_EXE_oracle-frontend");
    let (ok, text) = rig.run_with(bin, "/bin/sh");
    assert!(ok, "the script did not exit 0. Output:\n{text}");

    let body = fs::read_to_string(rig.apps().join("oracle-frontend.desktop")).expect("read back");
    let rom = rig.root.join("fixture.bin");
    fs::write(&rom, b"not a real ROM").expect("fixture ROM");

    let mut argv = exec_argv(&exec_line(&body), Some(&rom.display().to_string()));
    argv.push("--print-argv".to_string());
    let printed = Command::new(&argv[0])
        .args(&argv[1..])
        .env("PATH", rig.path())
        .output()
        .expect("the launcher is runnable");
    let printed_text = String::from_utf8_lossy(&printed.stdout).into_owned();
    assert!(
        printed.status.success(),
        "the launcher refused to resolve a launch at all:\n{printed_text}{}",
        String::from_utf8_lossy(&printed.stderr)
    );
    let launch: Vec<String> = printed_text.lines().map(str::to_string).collect();
    assert_eq!(
        launch.first().map(String::as_str),
        Some(bin),
        "the launcher would run something other than the binary the entry names:\n{printed_text}"
    );
    assert!(
        launch.iter().any(|a| a == "--aether"),
        "--aether was dropped, so Aurora could not attach to the window this entry opens:\n{printed_text}"
    );

    // Delete the ROM: the argv is already fixed, and the binary now stops at the read.
    fs::remove_file(&rom).expect("remove the fixture ROM");
    let out = Command::new(&launch[0])
        .args(&launch[1..])
        .output()
        .expect("the frontend binary is runnable");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !said.contains("unknown flag"),
        "the frontend's parser REJECTED a flag in the argv this entry produces:\n{said}"
    );
    assert!(
        !said.contains("missing <rom.bin>"),
        "the frontend reached its parser with no ROM, which is the icon-click defect:\n{said}"
    );
    assert!(
        said.contains("cannot read ROM"),
        "the run did not reach the ROM read, so parsing cannot be said to have succeeded and the two \
         assertions above witness nothing:\n{said}"
    );
}

/// A plain icon click passes no file. The launcher must still produce a ROM, because both binaries
/// refuse to start without one and a launcher that exits to an unread stderr is the original defect.
#[test]
fn a_click_with_no_file_selected_still_reaches_the_binary_with_a_rom() {
    let rig = Rig::new("bare-click");
    let bin = env!("CARGO_BIN_EXE_oracle-frontend");
    let (ok, text) = rig.run_with(bin, "/bin/sh");
    assert!(ok, "the script did not exit 0. Output:\n{text}");

    let body = fs::read_to_string(rig.apps().join("oracle-frontend.desktop")).expect("read back");
    let rom = rig.root.join("default.bin");
    fs::write(&rom, b"not a real ROM").expect("fixture ROM");

    // `%f` DROPPED, which is what a menu click expands it to.
    let mut argv = exec_argv(&exec_line(&body), None);
    argv.push("--print-argv".to_string());
    let out = Command::new(&argv[0])
        .args(&argv[1..])
        .env("PATH", rig.path())
        .env("ORACLE_ROM", &rom)
        .output()
        .expect("the launcher is runnable");
    let printed = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "a click with no file selected produced no launch:\n{printed}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        printed.lines().any(|l| l == rom.display().to_string()),
        "the launch carries no ROM, so the binary would refuse to start:\n{printed}"
    );
}

/// ⚑ **A launch that cannot happen SAYS SO.** The defect being fixed was silent: a click produced no
/// window, no dialog and nothing a person could read. Every refusal in the launcher must exit non-zero
/// with the reason in it.
#[test]
fn a_launch_with_no_usable_rom_refuses_out_loud() {
    let rig = Rig::new("no-rom");
    let bin = env!("CARGO_BIN_EXE_oracle-frontend");
    let (ok, text) = rig.run_with(bin, "/bin/sh");
    assert!(ok, "the script did not exit 0. Output:\n{text}");

    let body = fs::read_to_string(rig.apps().join("oracle-frontend.desktop")).expect("read back");
    let mut argv = exec_argv(&exec_line(&body), None);
    argv.push("--print-argv".to_string());
    let out = Command::new(&argv[0])
        .args(&argv[1..])
        .env("PATH", rig.path())
        .env("ORACLE_ROM", "/nonexistent/rom.bin")
        .output()
        .expect("the launcher is runnable");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "a launch with an unusable ROM reported success:\n{said}"
    );
    assert!(
        said.contains("/nonexistent/rom.bin"),
        "the refusal does not name the ROM it could not use, so a reader cannot act on it:\n{said}"
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
