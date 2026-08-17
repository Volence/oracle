# Player S2 — Config File + Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Settings survive relaunch: a flat hand-parsed config file at `$XDG_CONFIG_HOME/oracle/player.conf` persisting volume, mute, aspect, scale, status-line latch, and gamepad deadzone — CLI flags override config, config overrides built-in defaults.

**Architecture:** `config.rs` is pure data + pure parse/serialize plus two small IO functions (load-with-recovery, atomic save). `main.rs` loads once at startup, resolves precedence through a pure `resolve` step, marks a debounce on runtime changes (volume/mute/status-line — the same countdown pattern the `.srm` autosave uses), and writes on quit if anything changed. `gamepad.rs`'s deadzone const becomes the *default* for a field. Spec: `docs/superpowers/specs/2026-08-16-player-buildout-design.md` §7 (S2 scope per §12: "persistence for what already exists" — key/pad **bindings are S5, not here**; lens/view keys arrive with S3/S4).

**Tech Stack:** Rust, zero new dependencies (no TOML/serde — flat `key = value`, hand-parsed per spec §7).

**Verified facts (do not re-derive):** `Args { rom_path: String, scale: usize, aspect: Aspect, socket: … }` with `parse_args_from` at main.rs:256-321 (scale default 3 range 1..=8; aspect via `Aspect::from_name`, default `tv`). Volume state at main.rs:903-909 (`volume: u8 = audio::VOLUME_STEPS`, `muted = false`, both `#[cfg(feature="audio")]`; `audio::VOLUME_STEPS` exists, gain via `audio::gain_for(step, muted)`). Status line: `ov.status_line` (bool, Overlay field). Quit path: after the loop, `flush_pending_srm(..., "on quit")` at main.rs:~1653. Deadzone: `pub const STICK_DEADZONE: f32 = 0.5` at gamepad.rs:78, used at gamepad.rs:114-116 inside the axis mapping; `Gamepads::poll` at :222. SRAM debounce pattern: `SRAM_AUTOSAVE_DEBOUNCE_FRAMES: u32 = 120` + `Option<u32>` countdown. `notify`/`notify_err` helpers exist (main.rs:326). The startup toast + registry/palette setup from S1 sit just before the loop.

**House rules binding every task:** `cargo test -p oracle-frontend` never piped through `tail`/`head`. Every evidence-bearing test mutation-checked at writing time, one recorded line in the commit body. No `#[allow(dead_code)]`. No Co-Authored-By trailers. `ls` is aliased to eza — use `command ls`.

**File structure:**

| File | Responsibility |
|---|---|
| `crates/oracle-frontend/src/config.rs` (create) | `Config` (typed fields + defaults), `parse` (with per-key warnings vs structural corruption), `serialize`, `config_path`, `load` (recovery: `.bak` + defaults on corruption), `save` (atomic tmp+rename) |
| `crates/oracle-frontend/src/main.rs` (modify) | `mod config;` · Args.scale/aspect become `Option` · pure `resolve` precedence · startup load + init · dirty-mark + debounce + quit write · usage/module-doc text |
| `crates/oracle-frontend/src/gamepad.rs` (modify) | deadzone becomes a field (const stays as its default) |

---

### Task 1: `config.rs` — Config, parse, serialize (pure core)

**Files:**
- Create: `crates/oracle-frontend/src/config.rs`
- Modify: `crates/oracle-frontend/src/main.rs` (add `mod config;` next to `mod commands;`)

- [ ] **Step 1: Write the module with types, a stub parse/serialize, and the failing tests**

```rust
//! The player's persistent settings (spec §7). One flat `key = value` file, hand-parsed —
//! deliberately not TOML/serde: six typed keys need ~60 lines, not a dependency tree.
//!
//! Failure model (spec §7): a file that is STRUCTURALLY corrupt (unreadable bytes, a line
//! that is not `key = value`, a comment, or blank) is renamed to `.bak` and defaults load —
//! never a crash, never a silent overwrite of evidence. A key we don't know, or a value that
//! doesn't parse, is a per-key warning and the default for that key: an older build reading a
//! newer build's file must not nuke it (forward compatibility).

use crate::present::Aspect;

/// Everything the file can hold, always fully typed with the built-in default in place.
/// Fields deliberately mirror the runtime locals they feed; bindings/lenses/views arrive with
/// their own slices (S3-S5), not here.
#[derive(Clone, PartialEq, Debug)]
pub struct Config {
    /// Output volume step, clamped 0..=10 (the audio module's step count; stored plainly so a
    /// no-audio build still round-trips the file untouched).
    pub volume: u8,
    pub muted: bool,
    pub aspect: Aspect,
    /// Window scale multiplier, clamped to the CLI's own 1..=8.
    pub scale: usize,
    /// The F3 status-line latch.
    pub status_line: bool,
    /// Analog stick deadzone, clamped 0.05..=0.95 (0.0 would make drift into input; 1.0 would
    /// make sticks dead).
    pub deadzone: f32,
}

pub const VOLUME_MAX: u8 = 10;

impl Default for Config {
    fn default() -> Self {
        Config {
            volume: VOLUME_MAX,
            muted: false,
            aspect: Aspect::default(),
            scale: 3,
            status_line: false,
            deadzone: crate::gamepad_default_deadzone(),
        }
    }
}

/// A parsed file: the config plus one human line per ignored key/value (shown as toasts once
/// at load — the file is not rewritten to drop them).
pub struct Parsed {
    pub config: Config,
    pub warnings: Vec<String>,
}

/// `Err(line_no)` = structural corruption (a non-blank, non-comment line with no `=`);
/// the caller backs the file up and uses defaults.
pub fn parse(text: &str) -> Result<Parsed, usize> {
    let _ = text;
    Err(0) // Task Step 3 fills this in
}

pub fn serialize(c: &Config) -> String {
    let _ = c;
    String::new() // Task Step 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_is_identity() {
        let mut c = Config::default();
        c.volume = 4;
        c.muted = true;
        c.aspect = Aspect::from_name("integer").unwrap();
        c.scale = 5;
        c.status_line = true;
        c.deadzone = 0.35;
        let p = parse(&serialize(&c)).expect("own output must parse");
        assert_eq!(p.config, c);
        assert!(p.warnings.is_empty(), "own output warned: {:?}", p.warnings);
    }

    #[test]
    fn empty_and_comments_are_defaults() {
        let p = parse("\n# a comment\n\n").expect("blank/comment lines are fine");
        assert_eq!(p.config, Config::default());
        assert!(p.warnings.is_empty());
    }

    #[test]
    fn unknown_key_warns_and_is_ignored() {
        let p = parse("future_key = 7\nvolume = 4\n").expect("unknown keys are not corruption");
        assert_eq!(p.config.volume, 4);
        assert_eq!(p.warnings.len(), 1);
        assert!(p.warnings[0].contains("future_key"), "warning names the key");
    }

    #[test]
    fn bad_value_warns_and_keeps_default() {
        let p = parse("volume = banana\nscale = 12\ndeadzone = 2.0\n").unwrap();
        // Unparseable OR out-of-range values fall back per-key; nothing is clamped silently —
        // the warning is the contract.
        assert_eq!(p.config.volume, Config::default().volume);
        assert_eq!(p.config.scale, Config::default().scale);
        assert_eq!(p.config.deadzone, Config::default().deadzone);
        assert_eq!(p.warnings.len(), 3);
    }

    #[test]
    fn structural_corruption_is_an_error_with_the_line() {
        assert_eq!(parse("volume = 4\nnot a key value line\n"), err_line(2));
        assert_eq!(parse("garbage"), err_line(1));
    }

    fn err_line(n: usize) -> Result<Parsed, usize> {
        Err(n)
    }

    #[test]
    fn serialize_covers_every_field() {
        // The serializer and parser must agree on the key set: serialize a non-default config
        // and check every key name appears. Guards against adding a Config field that never
        // reaches the file.
        let s = serialize(&Config::default());
        for key in ["volume", "muted", "aspect", "scale", "status_line", "deadzone"] {
            assert!(s.contains(key), "serialize dropped `{key}`");
        }
    }
}
```

Note `Parsed` cannot derive `PartialEq` (it doesn't need to); the corruption test compares the
`Err` side only — write a tiny helper as shown or match on the result.

Also add to `main.rs`: `mod config;` (next to `mod commands;`), and a crate-level helper so
`config.rs` doesn't reach into `gamepad` under a feature gate:

```rust
/// The gamepad module's default deadzone, visible regardless of the `gamepad` feature so the
/// config file round-trips it identically in every build.
pub(crate) fn gamepad_default_deadzone() -> f32 {
    0.5
}
```

(Task 5 makes `gamepad.rs` consume this same value so there is exactly one default.)

- [ ] **Step 2: Run to verify FAIL**

Run: `cargo test -p oracle-frontend config::`
Expected: FAIL — parse stub returns `Err(0)`, serialize returns `""` (all six tests).

- [ ] **Step 3: Implement parse and serialize**

```rust
pub fn parse(text: &str) -> Result<Parsed, usize> {
    let mut c = Config::default();
    let mut warnings = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(i + 1);
        };
        let (key, value) = (key.trim(), value.trim());
        let mut warn = |what: &str| warnings.push(format!("config: ignored {key} ({what})"));
        match key {
            "volume" => match value.parse::<u8>() {
                Ok(v) if v <= VOLUME_MAX => c.volume = v,
                _ => warn(&format!("want 0..={VOLUME_MAX}, got `{value}`")),
            },
            "muted" => match value {
                "on" => c.muted = true,
                "off" => c.muted = false,
                _ => warn(&format!("want on|off, got `{value}`")),
            },
            "aspect" => match Aspect::from_name(value) {
                Some(a) => c.aspect = a,
                None => warn(&format!("want tv|square|integer, got `{value}`")),
            },
            "scale" => match value.parse::<usize>() {
                Ok(v) if (1..=8).contains(&v) => c.scale = v,
                _ => warn(&format!("want 1..=8, got `{value}`")),
            },
            "status_line" => match value {
                "on" => c.status_line = true,
                "off" => c.status_line = false,
                _ => warn(&format!("want on|off, got `{value}`")),
            },
            "deadzone" => match value.parse::<f32>() {
                Ok(v) if (0.05..=0.95).contains(&v) => c.deadzone = v,
                _ => warn(&format!("want 0.05..=0.95, got `{value}`")),
            },
            _ => warn("unknown key"),
        }
    }
    Ok(Parsed { config: c, warnings })
}

pub fn serialize(c: &Config) -> String {
    let on_off = |b: bool| if b { "on" } else { "off" };
    format!(
        "# oracle player settings — edited in-app; hand edits are fine (unknown keys are kept\n\
         # out of harm's way with a warning, a malformed line backs the file up to .bak)\n\
         volume = {}\nmuted = {}\naspect = {}\nscale = {}\nstatus_line = {}\ndeadzone = {}\n",
        c.volume,
        on_off(c.muted),
        c.aspect.name(),
        c.scale,
        on_off(c.status_line),
        c.deadzone,
    )
}
```

The closure capture of `warnings` inside `warn` while also assigning `c.*` fields borrows
disjointly, but if the borrow checker objects to the closure form, use a plain
`warnings.push(format!(...))` at each site — content over form.

- [ ] **Step 4: Run to verify PASS (both variants)**

Run: `cargo test -p oracle-frontend config::` then `cargo test -p oracle-frontend --no-default-features config::`
Expected: 6 passed ×2. (`f32` display for 0.35 round-trips via `parse::<f32>` — the round-trip
test proves it; if a float formatting mismatch fails the test, format deadzone with `{:.2}`
and re-verify.)

- [ ] **Step 5: Mutation checks (record each)**

1. In `parse`, change the unknown-key arm to silently `{}`: expect `unknown_key_warns_and_is_ignored` FAIL. Revert.
2. In `parse`, make bad values clamp instead of warn (e.g. `scale` arm `Ok(v) => c.scale = v.clamp(1,8)`): expect `bad_value_warns_and_keeps_default` FAIL. Revert.
3. Drop `deadzone` from `serialize`: expect `serialize_covers_every_field` FAIL (and round-trip). Revert.

- [ ] **Step 6: Commit**

```bash
cargo fmt && git add crates/oracle-frontend/src/config.rs crates/oracle-frontend/src/main.rs
git commit -m "feat(frontend): config file core — parse/serialize with per-key warnings" \
  -m "mutation: silent unknown-key arm -> unknown_key_warns_and_is_ignored FAIL" \
  -m "mutation: clamp-instead-of-warn -> bad_value_warns_and_keeps_default FAIL" \
  -m "mutation: serializer drops deadzone -> serialize_covers_every_field FAIL"
```

---

### Task 2: `config.rs` — path resolution + load-with-recovery + atomic save

**Files:**
- Modify: `crates/oracle-frontend/src/config.rs`

- [ ] **Step 1: Failing tests** (append inside `mod tests`)

```rust
    #[test]
    fn path_prefers_xdg_then_home() {
        let lookup = |k: &str| match k {
            "XDG_CONFIG_HOME" => Some(std::ffi::OsString::from("/xdg")),
            "HOME" => Some(std::ffi::OsString::from("/home/u")),
            _ => None,
        };
        assert_eq!(
            config_path_with(lookup),
            Some(std::path::PathBuf::from("/xdg/oracle/player.conf"))
        );
        let no_xdg = |k: &str| (k == "HOME").then(|| std::ffi::OsString::from("/home/u"));
        assert_eq!(
            config_path_with(no_xdg),
            Some(std::path::PathBuf::from("/home/u/.config/oracle/player.conf"))
        );
        assert_eq!(config_path_with(|_| None), None);
    }

    #[test]
    fn load_missing_is_clean_defaults() {
        let dir = scratch_dir("load_missing");
        let out = load(&dir.join("player.conf"));
        assert_eq!(out.config, Config::default());
        assert!(out.warnings.is_empty());
        assert!(!out.recovered);
    }

    #[test]
    fn save_then_load_round_trips_on_disk() {
        let dir = scratch_dir("save_load");
        let path = dir.join("player.conf");
        let mut c = Config::default();
        c.volume = 2;
        c.deadzone = 0.25;
        save(&path, &c).expect("save must succeed");
        let out = load(&path);
        assert_eq!(out.config, c);
        assert!(out.warnings.is_empty() && !out.recovered);
        // Atomicity leaves no droppings on the happy path.
        assert!(!path.with_extension("conf.tmp").exists());
    }

    #[test]
    fn corrupt_file_is_backed_up_and_defaults_load() {
        let dir = scratch_dir("corrupt");
        let path = dir.join("player.conf");
        std::fs::write(&path, "volume = 4\nthis line is not a setting\n").unwrap();
        let out = load(&path);
        assert_eq!(out.config, Config::default(), "corruption must not half-apply");
        assert!(out.recovered);
        assert!(out.warnings.iter().any(|w| w.contains(".bak")), "toast names the backup");
        let bak = path.with_extension("conf.bak");
        assert!(bak.exists(), "evidence preserved");
        assert!(std::fs::read_to_string(bak).unwrap().contains("not a setting"));
    }

    /// House test scratch: a fresh dir under the target tmpdir, unique per test.
    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("oracle-config-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
```

Before writing these, check how `save_state.rs`/`sram_file.rs` tests create scratch files
(grep `temp_dir\|tempdir` in those files) and MATCH the house pattern if it differs from the
`scratch_dir` helper above — the pattern above is the fallback, not a mandate.

- [ ] **Step 2: Run to verify FAIL** — `cargo test -p oracle-frontend config::` → compile errors (`config_path_with`, `load`, `save`, `recovered` missing).

- [ ] **Step 3: Implement**

```rust
/// Where the file lives: `$XDG_CONFIG_HOME/oracle/player.conf`, else
/// `$HOME/.config/oracle/player.conf`, else nowhere (a system with neither var set gets an
/// in-memory-only session — settings simply don't persist, nothing errors).
pub fn config_path() -> Option<std::path::PathBuf> {
    config_path_with(|k| std::env::var_os(k))
}

/// The testable half: same logic over an arbitrary environment lookup.
fn config_path_with(
    lookup: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Option<std::path::PathBuf> {
    let base = match lookup("XDG_CONFIG_HOME") {
        Some(x) => std::path::PathBuf::from(x),
        None => std::path::PathBuf::from(lookup("HOME")?).join(".config"),
    };
    Some(base.join("oracle").join("player.conf"))
}

/// A load never fails: the worst outcomes are defaults. `recovered` = the file was corrupt and
/// has been renamed to `.bak` (spec §7 — evidence preserved, never silently overwritten).
pub struct Loaded {
    pub config: Config,
    pub warnings: Vec<String>,
    pub recovered: bool,
}

pub fn load(path: &std::path::Path) -> Loaded {
    let defaults = || Loaded { config: Config::default(), warnings: Vec::new(), recovered: false };
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return defaults(),
        // Unreadable-but-present (permissions, invalid UTF-8, ...) is corruption too.
        Err(_) => return back_up(path, format!("config: {} unreadable", path.display())),
    };
    match parse(&text) {
        Ok(p) => Loaded { config: p.config, warnings: p.warnings, recovered: false },
        Err(line) => back_up(path, format!("config: {} line {line} malformed", path.display())),
    }
}

fn back_up(path: &std::path::Path, why: String) -> Loaded {
    let bak = path.with_extension("conf.bak");
    let moved = std::fs::rename(path, &bak).is_ok();
    let mut warnings = vec![if moved {
        format!("{why} — backed up to {} and using defaults", bak.display())
    } else {
        format!("{why} — could not back it up; using defaults, file left in place")
    }];
    warnings.truncate(1);
    Loaded { config: Config::default(), warnings, recovered: true }
}

/// Atomic save: temp file in the same directory, then rename — a crash mid-write can never
/// leave a half-written config (the `.srm` writer's rule). Creates the directory on first save.
pub fn save(path: &std::path::Path, c: &Config) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("conf.tmp");
    std::fs::write(&tmp, serialize(c))?;
    std::fs::rename(&tmp, path)
}
```

- [ ] **Step 4: Run to verify PASS (both variants)** — `cargo test -p oracle-frontend config::` → 10 passed; `--no-default-features` → 10 passed.

- [ ] **Step 5: Mutation checks (record each)**

1. In `load`, treat parse `Err` as defaults WITHOUT renaming (return `defaults()`): expect `corrupt_file_is_backed_up_and_defaults_load` FAIL. Revert.
2. In `save`, write directly to `path` (no tmp+rename): expect `save_then_load_round_trips_on_disk` still passes → that test does not pin atomicity (the droppings assertion passes trivially). That is fine — atomicity is pinned by inspection here, but RECORD in the commit body that this mutation survives and why (the test pins behavior, the tmp+rename pins the crash property that a unit test cannot exercise). Do NOT add a fake test for it.
3. In `config_path_with`, swap the XDG/HOME priority: expect `path_prefers_xdg_then_home` FAIL. Revert.

- [ ] **Step 6: Commit**

```bash
cargo fmt && git add -u crates/oracle-frontend
git commit -m "feat(frontend): config load with .bak recovery + atomic save" \
  -m "mutation: skip-backup-on-corrupt -> corrupt_file_is_backed_up_and_defaults_load FAIL" \
  -m "mutation: swap XDG/HOME priority -> path_prefers_xdg_then_home FAIL" \
  -m "note: direct-write mutation survives save_then_load (atomicity is a crash property; pinned by code shape, not unit-testable)"
```

---

### Task 3: `gamepad.rs` — deadzone becomes a field

**Files:**
- Modify: `crates/oracle-frontend/src/gamepad.rs`, `crates/oracle-frontend/src/main.rs`

- [ ] **Step 1: Read the target first.** Read gamepad.rs fully (427 lines): the `Gamepads` struct, its constructor (find it — likely `Gamepads::new()`), `poll` (:222), the axis mapping at :114-116 using `STICK_DEADZONE`, and the existing test `analog_stick_past_deadzone_presses_directions` (:329) to see how the axis path is tested.

- [ ] **Step 2: Failing test** — add alongside the existing gamepad tests, following their exact fixture style (they test the pure mapping helpers; find the function under test used by :329 and its signature):

```rust
    /// The deadzone is per-instance now (config-fed); the const is only the default. A tighter
    /// deadzone turns the same axis magnitude into a press that the default would ignore.
    #[test]
    fn deadzone_is_configurable_per_instance() {
        // Use whatever pure helper the existing analog test drives, but with two different
        // deadzone values around a fixed axis magnitude of 0.4:
        //   deadzone 0.5 (default) -> 0.4 is NOT a press
        //   deadzone 0.3           -> 0.4 IS a press
        // Exact call shape depends on the helper's real signature — mirror the :329 test.
    }
```

(The comment block is the requirement; write the real body against the actual helper. If the
axis mapping is a method on `Gamepads` rather than a free function, refactor the smallest
testable unit out — e.g. `fn axis_presses(v: f32, deadzone: f32) -> AxisPress` — and drive both
the existing test and the new one through it.)

- [ ] **Step 3: Run to verify FAIL**, then implement: `Gamepads` gains `deadzone: f32`; its constructor gains a `deadzone: f32` parameter (callers pass `config.deadzone` — main.rs wiring happens in Task 5, so for THIS commit update the existing construction site with `gamepad_default_deadzone()` to stay behavior-identical); :114-116 read `self.deadzone` (or the extracted helper's parameter); `STICK_DEADZONE` stays as the documented default and `gamepad_default_deadzone()` in main.rs returns it when the `gamepad` feature is on — reconcile so there is ONE numeric literal:

```rust
// main.rs — replace the Task 1 stub:
#[cfg(feature = "gamepad")]
pub(crate) fn gamepad_default_deadzone() -> f32 {
    gamepad::STICK_DEADZONE
}
#[cfg(not(feature = "gamepad"))]
pub(crate) fn gamepad_default_deadzone() -> f32 {
    0.5 // gamepad module absent from this build; the file still round-trips the key
}
```

- [ ] **Step 4: Run to verify PASS** — `cargo test -p oracle-frontend` (full crate, both variants; also confirm the pre-existing analog test still passes).

- [ ] **Step 5: Mutation check** — make the axis path ignore the instance value and read `STICK_DEADZONE` directly again: expect `deadzone_is_configurable_per_instance` FAIL. Revert; record.

- [ ] **Step 6: Commit**

```bash
cargo fmt && git add -u crates/oracle-frontend
git commit -m "feat(frontend): gamepad deadzone is per-instance, const is the default" \
  -m "mutation: axis path reads const not instance -> deadzone_is_configurable_per_instance FAIL"
```

---

### Task 4: `main.rs` — Args gain Options; pure precedence resolve

**Files:**
- Modify: `crates/oracle-frontend/src/main.rs`

- [ ] **Step 1: Failing tests.** main.rs has existing `parse_args_from` tests (find them). Add:

```rust
    #[test]
    fn args_report_explicitness_for_config_precedence() {
        let a = parse_args_from(["rom.bin".to_string()]).unwrap();
        assert_eq!(a.scale, None, "unset scale must be None so config can fill it");
        assert_eq!(a.aspect, None);
        let a = parse_args_from(
            ["rom.bin", "--scale", "5", "--aspect", "integer"].map(String::from),
        )
        .unwrap();
        assert_eq!(a.scale, Some(5));
        assert_eq!(a.aspect, Some(Aspect::from_name("integer").unwrap()));
    }

    #[test]
    fn resolve_prefers_cli_then_config() {
        let mut cfg = config::Config::default();
        cfg.scale = 6;
        cfg.aspect = Aspect::from_name("square").unwrap();
        // CLI silent -> config wins.
        assert_eq!(resolve_scale(None, &cfg), 6);
        assert_eq!(resolve_aspect(None, &cfg), cfg.aspect);
        // CLI explicit -> CLI wins.
        assert_eq!(resolve_scale(Some(2), &cfg), 2);
        assert_eq!(resolve_aspect(Some(Aspect::default()), &cfg), Aspect::default());
    }
```

- [ ] **Step 2: Run to verify FAIL** (compile: fields are not Options, resolve fns missing).

- [ ] **Step 3: Implement.** `Args { scale: Option<usize>, aspect: Option<Aspect>, ... }`; in `parse_args_from` the locals become `Option`s (`let mut scale: Option<usize> = None;` set to `Some(v)` in the flag arms; delete the defaults there). Add:

```rust
/// CLI beats config beats built-in default (spec §7). Two tiny fns rather than one struct so
/// each call site reads as what it is.
fn resolve_scale(cli: Option<usize>, cfg: &config::Config) -> usize {
    cli.unwrap_or(cfg.scale)
}
fn resolve_aspect(cli: Option<Aspect>, cfg: &config::Config) -> Aspect {
    cli.unwrap_or(cfg.aspect)
}
```

Fix every existing `args.scale` / `args.aspect` use site to go through locals resolved ONCE at
startup (`let scale = resolve_scale(args.scale, &cfg.config);` etc. — Task 5 supplies `cfg`;
for THIS commit resolve against `config::Config::default()` so behavior is identical and the
commit stands alone). Grep for `args.aspect` and `args.scale` — update all sites (the loop's
`dest_rect` calls use `args.aspect` at ~:911 and post-run; `initial_window_size` uses both; the
status line's aspect name display may too). Update any other parse_args tests that assert the
old defaults. Update the usage text (main.rs:~716-727) to mention that unset flags fall back to
`~/.config/oracle/player.conf`.

- [ ] **Step 4: Run to verify PASS** — full crate, both variants. Also `cargo clippy -p oracle-frontend --all-targets -- -D warnings` (both variants) — resolve fns are used, so no dead code.

- [ ] **Step 5: Mutation check** — swap precedence in `resolve_scale` (`cfg.scale` ignoring cli... i.e. return `cfg.scale` always): expect `resolve_prefers_cli_then_config` FAIL. Revert; record.

- [ ] **Step 6: Commit**

```bash
cargo fmt && git add -u crates/oracle-frontend
git commit -m "feat(frontend): CLI flags become explicit-or-None; precedence resolve fns" \
  -m "mutation: resolve ignores CLI -> resolve_prefers_cli_then_config FAIL"
```

---

### Task 5: `main.rs` — startup load, runtime dirty-marking, quit write

**Files:**
- Modify: `crates/oracle-frontend/src/main.rs`

This is the integration task; it is mostly placement, so the code below is complete and the
work is fitting it into the real spots.

- [ ] **Step 1: Startup load** (before window creation, where `scale`/`aspect` resolve):

```rust
    // Persistent settings (spec §7). A missing file is silently defaults; a corrupt one was
    // backed up to .bak (the load's warnings say so on the glass, once).
    let cfg_path = config::config_path();
    let loaded = match &cfg_path {
        Some(p) => config::load(p),
        None => config::Loaded {
            config: config::Config::default(),
            warnings: vec!["config: no $XDG_CONFIG_HOME or $HOME — settings will not persist".into()],
            recovered: false,
        },
    };
    let mut cfg = loaded.config.clone();
    let scale = resolve_scale(args.scale, &cfg);
    let aspect = resolve_aspect(args.aspect, &cfg);
```

The warnings toast AFTER `ov` exists (find where the S1 startup hint `ov.push("PRESS ` FOR
COMMANDS", ...)` sits and put them adjacent): `for w in &loaded.warnings { notify(&mut ov, ERROR, w.clone()); }`
(use ACCENT not ERROR for the no-persist case if you can distinguish; a single color is also
acceptable — pick ERROR only when `loaded.recovered`).

Initialize runtime state from `cfg`:
- `ov.status_line = cfg.status_line;`
- volume block (audio builds): `let mut volume: u8 = cfg.volume.min(audio::VOLUME_STEPS); let mut muted = cfg.muted;` — replacing the current hardcoded init at main.rs:903-909. (If `audio::VOLUME_STEPS != config::VOLUME_MAX`, the `.min` is the reconciliation; add a `debug_assert_eq!(audio::VOLUME_STEPS, config::VOLUME_MAX);` beside it so a future step-count change is caught in tests.)
- gamepad construction site passes `cfg.deadzone` instead of the Task-3 placeholder default.

- [ ] **Step 2: Dirty-marking + debounce.** Beside the SRAM debounce state:

```rust
    // Config autosave: same debounce shape as the .srm (coalesce a volume ramp into one write).
    const CONFIG_SAVE_DEBOUNCE_FRAMES: u32 = 120;
    let mut config_save_countdown: Option<u32> = None;
```

In the dispatch arms that change persisted state, keep `cfg` in sync and arm the countdown:
- `Cmd::ToggleStatusLine` arm: after the toggle, `cfg.status_line = ov.status_line; config_save_countdown = Some(CONFIG_SAVE_DEBOUNCE_FRAMES);`
- volume fold arm (audio): after the inner match and notify, `cfg.volume = volume; cfg.muted = muted; config_save_countdown = Some(CONFIG_SAVE_DEBOUNCE_FRAMES);`

Find the SRAM debounce tick (search `sram_save_countdown` decrement in the loop tail) and add
the config twin beside it:

```rust
        if let Some(n) = config_save_countdown {
            config_save_countdown = if n == 0 {
                if let Some(p) = &cfg_path {
                    if let Err(e) = config::save(p, &cfg) {
                        notify_err(&mut ov, format!("config: save to {} failed: {e}", p.display()));
                    }
                }
                None
            } else {
                Some(n - 1)
            };
        }
```

- [ ] **Step 3: Quit write.** After the loop, beside `flush_pending_srm(..., "on quit")`:

```rust
    // Settings on quit: write only if something changed since load (or a debounced write is
    // still pending) — an untouched session leaves the file's mtime alone.
    if let Some(p) = &cfg_path {
        if cfg != loaded_config_at_start || config_save_countdown.is_some() {
            if let Err(e) = config::save(p, &cfg) {
                eprintln!("config: save on quit to {} failed: {e}", p.display());
            }
        }
    }
```

where `loaded_config_at_start` is a clone kept from Step 1 (`let loaded_config_at_start = loaded.config.clone();` — note Step 1 already clones into `cfg`; keep `loaded.config` itself for this comparison instead of a second clone if ownership allows).

- [ ] **Step 4: Module doc.** Add a short "## Settings" section to the main.rs module doc: the path, the six keys, CLI-beats-config, the `.bak` recovery sentence, and that F3/volume changes persist automatically.

- [ ] **Step 5: Verify.** Full gates: `cargo test -p oracle-frontend` ×2 variants, `cargo clippy --all-targets -- -D warnings` ×2, `cargo build --release -p oracle-frontend`. Then a behavioral spot-check that needs no window: run the release binary long enough to boot is NOT possible headless — instead verify the load path with a unit-style check already covered by config:: tests, and verify wiring by `grep`: every persisted field (`cfg.volume`, `cfg.muted`, `cfg.status_line`, `cfg.deadzone`, `scale`, `aspect`) has (a) an init-from-config site and (b) — for the three runtime-changeable ones — a dirty-marking site. Paste the grep evidence into the report.

- [ ] **Step 6: Commit**

```bash
cargo fmt && git add -u crates/oracle-frontend
git commit -m "feat(frontend): settings persist — startup load, debounced autosave, quit write" \
  -m "volume/mute/status-line/aspect/scale/deadzone survive relaunch; CLI beats config; corrupt file -> .bak + defaults"
```

---

### Task 6: Final gates

- [ ] **Step 1:** `cargo fmt --check`; clippy `-D warnings` both variants; `cargo test --workspace` (single tree only); `git diff <branch base>..HEAD -- crates/oracle-core/` must be EMPTY.
- [ ] **Step 2:** Commit any fixups; report gate outputs verbatim.

---

## Self-review (done at plan-writing time)

- **Spec §7 coverage:** file+format+location ✓ (T1/T2), what persists minus S3-S5 items ✓ (T5; bindings explicitly out per §12), write-on-change-debounced + on-quit ✓ (T5), corrupt→`.bak`+defaults+toast ✓ (T2/T5), unknown-key warn ✓ (T1), settings-UI-is-the-palette needs no new UI this slice (volume/mute/F3 commands already exist and now mark dirty) ✓. Deadzone *picker with live meter* is S5 (spec §7 rebinding cluster) — only the plumbing lands here.
- **Placeholders:** Task 3 Step 2's test body is deliberately a specified-behavior comment because the helper's real signature must be read first — the requirement (0.4 vs deadzones 0.5/0.3, both directions) is fully pinned. Everything else is complete code.
- **Type consistency:** `config::{Config, Parsed, Loaded, VOLUME_MAX, parse, serialize, config_path, load, save}` defined in T1/T2 match every later use; `resolve_scale`/`resolve_aspect` (T4) match T5's calls; `gamepad_default_deadzone()` (T1 stub → T3 real) is the single default source.
