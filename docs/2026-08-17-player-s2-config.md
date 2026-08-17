# Player S2 shipped — settings persist (2026-08-17, overnight)

Branch `player-s2-config`, 7 commits `0e0856b..b1d217c`, merged to `m68000-microop-framework`
on top of S1 (`docs/2026-08-17-player-s1-palette.md`).
Spec: `docs/superpowers/specs/2026-08-16-player-buildout-design.md` §7.
Plan: `docs/superpowers/plans/2026-08-17-player-s2-config.md`.

## What shipped

- **`config.rs`** — `$XDG_CONFIG_HOME/oracle/player.conf` (fallback `~/.config/...`), flat
  hand-parsed `key = value`, six keys: `volume, muted, aspect, scale, status_line, deadzone`.
  Per-key warnings for unknown keys/bad values; a structurally corrupt file is backed up to
  `.bak` (**first evidence wins** — a second corruption never overwrites the first backup) and
  defaults load; saves are atomic (tmp+rename).
- **Precedence**: CLI flag > config > built-in default, resolved exactly once at startup;
  a `--scale`/`--aspect` override is never written back.
- **Runtime persistence**: volume/mute (`-`/`=`/`M`) and the F3 status-line latch autosave on a
  120-frame debounce against a last-successfully-saved baseline (no redundant writes; a failed
  autosave retries at quit). Gamepad deadzone is now per-instance, fed from the config
  (`STICK_DEADZONE` is only the default) — hand-edit `deadzone` to tune it until S5's picker.
- **Fresh install is byte-identical to pre-S2**: no config file = the old defaults everywhere.

Gates at merge: fmt clean, clippy `-D warnings` 0 ×2 variants, frontend 137/109 tests, **full
workspace suite EXIT=0 (36 legs, 0 failures)**, zero `crates/oracle-core/` diff. A compile-time
`const` assert now pins `audio::VOLUME_STEPS == config::VOLUME_MAX`.

## ☐ OWNER-OWED smoke checklist — EXTENDED (still never run; headless here)

Everything from the S1 checklist (`docs/2026-08-17-player-s1-palette.md`), plus:
- change volume + F3, relaunch → both restored (and a toast-free clean boot);
- corrupt `~/.config/oracle/player.conf` by hand (e.g. add a line `garbage`) → red toast names
  the `.bak`, defaults load;
- hand-edit `deadzone = 0.25` → felt on a real stick (the long-standing owed item);
- `--scale 5` for one run → not written into the file.

## Registered follow-ups

- ~~**F-CONFIG-UNKNOWN-KEYS**~~ **CLOSED in S3's lens-spine commit.** The reversal fired exactly as
  adjudicated: the seventh key (`lenses`) landed together with `Config::unknown`, a `serialize` that
  writes unrecognised keys back verbatim, a truthful file header and main.rs banner, and
  `an_unknown_key_survives_a_save` (plus a strengthened `round_trip_is_identity`, whose fixture now
  carries a preserved key). Anchor: `crates/oracle-frontend/src/config.rs`.
- Non-gamepad builds carry a bare `0.5` deadzone default literal in `main.rs` with no
  compile-time tie to `gamepad::STICK_DEADZONE` (feature-variant drift risk; commented).
- Spec-§7 residue: no palette commands for aspect/scale exist yet (lands with the settings-UI
  work); in-session hand edits to the file are overwritten by any autosave (self-resolves when
  in-app editing covers those keys).

## Review-loop record (this slice)

Caught and fixed before merge: a `.bak`-overwrite hole that would have destroyed first evidence
on a second corruption (now mutation-pinned); a clippy gate-breaker in a plan-authored test; the
quit-write comparing against the load snapshot instead of the last save (redundant writes +
wrong template for S3 — now a `cfg_saved` baseline); corrupt-load warnings missing stderr; a
`debug_assert` that no test would ever run (now a compile-time assert, falsification-verified);
and the config-file header promising unknown-key safety the code doesn't provide. The
unknown-key preservation question got a full adjudication (DEFER with mechanical reversal, four
grounds, my own lean overruled).
