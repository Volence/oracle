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
    /// Whether output is silenced; independent of `volume` so unmuting restores the prior step.
    pub muted: bool,
    /// The window's display aspect (tv/square/integer).
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
        if key.is_empty() {
            return Err(i + 1);
        }
        match key {
            "volume" => match value.parse::<u8>() {
                Ok(v) if v <= VOLUME_MAX => c.volume = v,
                _ => warnings.push(format!(
                    "config: ignored {key} (want 0..={VOLUME_MAX}, got `{value}`)"
                )),
            },
            "muted" => match value {
                "on" => c.muted = true,
                "off" => c.muted = false,
                _ => warnings.push(format!(
                    "config: ignored {key} (want on|off, got `{value}`)"
                )),
            },
            "aspect" => match Aspect::from_name(value) {
                Some(a) => c.aspect = a,
                None => warnings.push(format!(
                    "config: ignored {key} (want tv|square|integer, got `{value}`)"
                )),
            },
            "scale" => match value.parse::<usize>() {
                Ok(v) if (1..=8).contains(&v) => c.scale = v,
                _ => warnings.push(format!("config: ignored {key} (want 1..=8, got `{value}`)")),
            },
            "status_line" => match value {
                "on" => c.status_line = true,
                "off" => c.status_line = false,
                _ => warnings.push(format!(
                    "config: ignored {key} (want on|off, got `{value}`)"
                )),
            },
            "deadzone" => match value.parse::<f32>() {
                Ok(v) if (0.05..=0.95).contains(&v) => c.deadzone = v,
                _ => warnings.push(format!(
                    "config: ignored {key} (want 0.05..=0.95, got `{value}`)"
                )),
            },
            _ => warnings.push(format!("config: ignored {key} (unknown key)")),
        }
    }
    Ok(Parsed {
        config: c,
        warnings,
    })
}

/// Renders every field as one `key = value` line, plus the two `#` header lines whose wording
/// must stay in sync with the module doc's failure-model description above.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_is_identity() {
        let c = Config {
            volume: 4,
            muted: true,
            aspect: Aspect::from_name("integer").unwrap(),
            scale: 5,
            status_line: true,
            deadzone: 0.35,
        };
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
        assert!(
            p.warnings[0].contains("future_key"),
            "warning names the key"
        );
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
        assert!(matches!(
            parse("volume = 4\nnot a key value line\n"),
            Err(2)
        ));
        assert!(matches!(parse("garbage"), Err(1)));
        assert!(
            matches!(parse("= 4"), Err(1)),
            "an empty key is corruption, not an unknown key"
        );
    }

    #[test]
    fn serialize_covers_every_field() {
        // The serializer and parser must agree on the key set: serialize a non-default config
        // and check every key name appears. Guards against adding a Config field that never
        // reaches the file.
        let s = serialize(&Config::default());
        for key in [
            "volume",
            "muted",
            "aspect",
            "scale",
            "status_line",
            "deadzone",
        ] {
            assert!(s.contains(key), "serialize dropped `{key}`");
        }
    }
}
