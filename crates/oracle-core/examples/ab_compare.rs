//! Motion A/B pixel-compare — the committed, reusable diff at the center of the s4-boot milestone's motion
//! extension (`docs/plans/2026-07-21-s4-boot-milestone.md`). Where [`motion_run`](motion_run.rs) drives our
//! core under a scripted pad and dumps one PPM per frame, this tool compares such a frame against an **Oracle
//! reference** PPM — tolerant of the two known, benign differences between the two:
//!
//! 1. **The P8 DAC-ramp encoding difference** (`docs/2026-07-16-vdp-pixel-known-differences.md`). Oracle and
//!    our core expand a 3-bit VDP channel level (`0..=7`) to 8 bits through *different* fixed ramps — Oracle
//!    `level × 34` → `{0,34,68,102,136,170,204,238}`, ours `(level·2)·255/14` (truncated) →
//!    `{0,36,72,109,145,182,218,255}`. A raw byte compare would false-alarm on **every** non-zero pixel. So
//!    we compare in **3-bit VDP-level space**: each channel is mapped to its nearest level index on *its own*
//!    side's ramp, and the two level triples are compared. Identical palette levels ⇒ zero diff.
//! 2. **A small frame offset** from Oracle's deferred reset. The caller passes one Oracle reference plus a set
//!    of candidate "ours" PPMs (one per frame offset, e.g. `ours.f596.ppm … ours.f604.ppm`); the tool reports
//!    the ramp-normalized differing-pixel count for each and picks the minimum, absorbing the offset without
//!    hand-tuning.
//!
//! This is a **dev tool, not a gate artifact** — nothing in CI depends on the input files existing. Missing or
//! unreadable inputs are a plain error, not a panic backtrace (the same convention as `boot_rom`/`watch_probe`
//! /`motion_run`). It reads only PPMs; converting Oracle's PNG screenshot to PPM (ImageMagick) is the caller's
//! job, and driving Oracle over MCP is the overseer's.
//!
//! Usage: `cargo run --release --example ab_compare -- <oracle_ref.ppm> <ours.ppm...>`
//!   - `<oracle_ref.ppm>` — the Oracle reference frame (P6 binary PPM).
//!   - `<ours.ppm...>`    — one or more candidate frames from our core, one per frame offset.
//!
//! Output: a per-offset delta table (each candidate's ramp-normalized differing-pixel count), then the
//! best-offset verdict — `PASS` if that candidate has zero differing pixels after normalization, else the
//! first-divergence report (`(x,y)` + both sides' level triples there).

use std::process::ExitCode;

/// A parsed binary PPM (P6): `width × height` row-major RGB.
struct Ppm {
    width: usize,
    height: usize,
    pixels: Vec<(u8, u8, u8)>,
}

/// Oracle's 3-bit→8-bit channel ramp (P8): `level × 34`.
const ORACLE_RAMP: [u8; 8] = [0, 34, 68, 102, 136, 170, 204, 238];

/// Our core's 3-bit→8-bit channel ramp — the pinned `intensity(level, Normal)` formula from
/// `crates/oracle-core/src/render.rs`: `((level·2) · 255 / 14) as u8` (integer truncation, **not** rounding).
fn ours_ramp() -> [u8; 8] {
    let mut r = [0u8; 8];
    for (level, slot) in r.iter_mut().enumerate() {
        // Mirrors `intensity(level, PixelState::Normal)`: step = level·2 on the shared 0..=14 quantization.
        *slot = ((level as u16 * 2) * 255 / 14) as u8;
    }
    r
}

/// Map an 8-bit channel value to its nearest 3-bit level index (`0..=7`) on `ramp`.
fn nearest_level(value: u8, ramp: &[u8; 8]) -> u8 {
    let mut best = 0u8;
    let mut best_dist = u16::MAX;
    for (i, &anchor) in ramp.iter().enumerate() {
        let dist = (value as i16 - anchor as i16).unsigned_abs();
        if dist < best_dist {
            best_dist = dist;
            best = i as u8;
        }
    }
    best
}

/// The ramp-normalized level triple of a pixel on a given side's ramp.
fn levels(pixel: (u8, u8, u8), ramp: &[u8; 8]) -> [u8; 3] {
    [
        nearest_level(pixel.0, ramp),
        nearest_level(pixel.1, ramp),
        nearest_level(pixel.2, ramp),
    ]
}

/// The outcome of comparing one Oracle reference against one "ours" candidate, in 3-bit VDP-level space.
#[derive(Debug)]
struct DiffResult {
    /// Count of pixels whose ramp-normalized level triples differ.
    differing: usize,
    /// The first differing pixel in row-major order: `(x, y, oracle_levels, ours_levels)`.
    first_divergence: Option<(usize, usize, [u8; 3], [u8; 3])>,
}

/// Compare an Oracle reference against an "ours" candidate in ramp-normalized level space. Dimension mismatch
/// is a clean error, not a divergence.
fn compare(oracle: &Ppm, ours: &Ppm) -> Result<DiffResult, String> {
    if oracle.width != ours.width || oracle.height != ours.height {
        return Err(format!(
            "dimension mismatch: oracle {}×{} vs ours {}×{}",
            oracle.width, oracle.height, ours.width, ours.height
        ));
    }
    let ours_ramp = ours_ramp();
    let mut differing = 0usize;
    let mut first_divergence = None;
    for (i, (&o, &m)) in oracle.pixels.iter().zip(ours.pixels.iter()).enumerate() {
        let ol = levels(o, &ORACLE_RAMP);
        let ml = levels(m, &ours_ramp);
        if ol != ml {
            differing += 1;
            if first_divergence.is_none() {
                let (x, y) = (i % oracle.width, i / oracle.width);
                first_divergence = Some((x, y, ol, ml));
            }
        }
    }
    Ok(DiffResult {
        differing,
        first_divergence,
    })
}

/// Parse a binary PPM (P6). Handles the standard header grammar: the `P6` magic, then width, height, and
/// maxval as whitespace-separated ASCII decimals, with `#`-comments allowed between tokens; a single
/// whitespace byte follows maxval, then `width·height·3` raw bytes. Only 8-bit (`maxval == 255`) is supported.
fn parse_ppm(bytes: &[u8]) -> Result<Ppm, String> {
    let mut pos = 0usize;

    // Skip whitespace and #-comments (a comment runs to end-of-line).
    let skip_ws = |pos: &mut usize| {
        while *pos < bytes.len() {
            match bytes[*pos] {
                b' ' | b'\t' | b'\n' | b'\r' => *pos += 1,
                b'#' => {
                    while *pos < bytes.len() && bytes[*pos] != b'\n' {
                        *pos += 1;
                    }
                }
                _ => break,
            }
        }
    };
    // Read a whitespace/comment-delimited ASCII decimal token.
    let read_uint = |pos: &mut usize| -> Result<usize, String> {
        skip_ws(pos);
        let start = *pos;
        while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
            *pos += 1;
        }
        if *pos == start {
            return Err("expected an integer in the PPM header".to_string());
        }
        std::str::from_utf8(&bytes[start..*pos])
            .unwrap()
            .parse::<usize>()
            .map_err(|e| format!("bad integer in PPM header: {e}"))
    };

    if bytes.len() < 2 || &bytes[0..2] != b"P6" {
        return Err("not a binary PPM (missing P6 magic)".to_string());
    }
    pos += 2;
    let width = read_uint(&mut pos)?;
    let height = read_uint(&mut pos)?;
    let maxval = read_uint(&mut pos)?;
    if maxval != 255 {
        return Err(format!(
            "unsupported PPM maxval {maxval} (only 255 supported)"
        ));
    }
    // Exactly one whitespace byte separates the header from the raster.
    if pos >= bytes.len() || !matches!(bytes[pos], b' ' | b'\t' | b'\n' | b'\r') {
        return Err("missing whitespace after PPM maxval".to_string());
    }
    pos += 1;

    let want = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(3))
        .ok_or_else(|| "PPM dimensions overflow".to_string())?;
    let raster = &bytes[pos..];
    if raster.len() < want {
        return Err(format!(
            "truncated PPM raster: have {} bytes, need {want}",
            raster.len()
        ));
    }
    let pixels = raster[..want]
        .chunks_exact(3)
        .map(|c| (c[0], c[1], c[2]))
        .collect();
    Ok(Ppm {
        width,
        height,
        pixels,
    })
}

/// Read + parse a PPM file, mapping any I/O or format error to a plain message keyed by path.
fn load_ppm(path: &str) -> Result<Ppm, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    parse_ppm(&bytes).map_err(|e| format!("{path}: {e}"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: ab_compare <oracle_ref.ppm> <ours.ppm...>");
        return ExitCode::from(2);
    }
    let oracle_path = &args[0];
    let ours_paths = &args[1..];

    let oracle = match load_ppm(oracle_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    println!(
        "oracle ref {oracle_path}: {}×{} ({} px)",
        oracle.width,
        oracle.height,
        oracle.pixels.len()
    );

    // Per-offset delta table, then pick the minimum differing-pixel count.
    let mut best: Option<(&String, DiffResult)> = None;
    println!("per-offset ramp-normalized delta:");
    for path in ours_paths {
        let ours = match load_ppm(path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("  {e}");
                return ExitCode::from(1);
            }
        };
        match compare(&oracle, &ours) {
            Ok(res) => {
                println!("  {path:<40} {} differing px", res.differing);
                if best
                    .as_ref()
                    .is_none_or(|(_, b)| res.differing < b.differing)
                {
                    best = Some((path, res));
                }
            }
            Err(e) => {
                // A dimension mismatch is per-candidate; report it in the table and move on.
                println!("  {path:<40} ERROR: {e}");
            }
        }
    }

    let Some((best_path, best_res)) = best else {
        eprintln!("no comparable candidate (all mismatched dimensions)");
        return ExitCode::from(1);
    };

    println!();
    if best_res.differing == 0 {
        println!("PASS: {best_path} is pixel-identical to {oracle_path} (ramp-normalized)");
        ExitCode::SUCCESS
    } else {
        println!(
            "FAIL: best offset {best_path} has {} differing px (ramp-normalized)",
            best_res.differing
        );
        if let Some((x, y, ol, ml)) = best_res.first_divergence {
            println!("  first divergence at ({x},{y}): oracle levels {ol:?} vs ours {ml:?}");
        }
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a binary PPM byte blob for `width × height` RGB pixels, optionally with a header comment (to
    /// exercise the parser's comment handling — ImageMagick-style output may carry one).
    fn make_ppm(
        width: usize,
        height: usize,
        pixels: &[(u8, u8, u8)],
        comment: Option<&str>,
    ) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"P6\n");
        if let Some(c) = comment {
            v.extend_from_slice(format!("#{c}\n").as_bytes());
        }
        v.extend_from_slice(format!("{width} {height}\n255\n").as_bytes());
        for &(r, g, b) in pixels {
            v.extend_from_slice(&[r, g, b]);
        }
        v
    }

    /// Build an [`Ppm`] whose pixels are the given per-channel *level* triples, expanded through `ramp`.
    fn ppm_from_levels(width: usize, height: usize, levels: &[[u8; 3]], ramp: &[u8; 8]) -> Ppm {
        let pixels = levels
            .iter()
            .map(|l| {
                (
                    ramp[l[0] as usize],
                    ramp[l[1] as usize],
                    ramp[l[2] as usize],
                )
            })
            .collect();
        Ppm {
            width,
            height,
            pixels,
        }
    }

    #[test]
    fn ours_ramp_matches_pinned_truncated_formula() {
        // The core truncates, so 72/145/218 — NOT the rounded 73/146/219 an approximation would give.
        assert_eq!(ours_ramp(), [0, 36, 72, 109, 145, 182, 218, 255]);
    }

    #[test]
    fn nearest_level_maps_exact_and_near_anchors() {
        // Exact anchors on each side map to their own index.
        for level in 0u8..8 {
            assert_eq!(
                nearest_level(ORACLE_RAMP[level as usize], &ORACLE_RAMP),
                level
            );
            assert_eq!(
                nearest_level(ours_ramp()[level as usize], &ours_ramp()),
                level
            );
        }
        // A value between two anchors snaps to the nearer one.
        assert_eq!(nearest_level(1, &ORACLE_RAMP), 0); // 1 is nearest 0, not 34
        assert_eq!(nearest_level(255, &ORACLE_RAMP), 7); // saturates to the top anchor 238
    }

    #[test]
    fn identical_modulo_ramp_yields_zero_diff() {
        // Same level field on both sides, expanded through each side's *different* ramp — a raw byte compare
        // would flag every non-zero pixel, but ramp-normalized they are identical.
        let field = [[7, 0, 0], [0, 7, 3], [4, 4, 4], [1, 6, 2]];
        let oracle = ppm_from_levels(2, 2, &field, &ORACLE_RAMP);
        let ours = ppm_from_levels(2, 2, &field, &ours_ramp());
        // Sanity: the raw bytes really do differ (e.g. level 7 → 238 vs 255).
        assert_ne!(oracle.pixels, ours.pixels);
        let res = compare(&oracle, &ours).unwrap();
        assert_eq!(res.differing, 0);
        assert!(res.first_divergence.is_none());
    }

    #[test]
    fn single_pixel_diff_is_located() {
        let mut ours_field = [[7, 0, 0], [0, 7, 3], [4, 4, 4], [1, 6, 2]];
        let oracle = ppm_from_levels(2, 2, &ours_field, &ORACLE_RAMP);
        // Flip one level on the ours side at index 2 → (x=0, y=1) in a 2×2 image.
        ours_field[2] = [4, 5, 4];
        let ours = ppm_from_levels(2, 2, &ours_field, &ours_ramp());
        let res = compare(&oracle, &ours).unwrap();
        assert_eq!(res.differing, 1);
        assert_eq!(res.first_divergence, Some((0, 1, [4, 4, 4], [4, 5, 4])));
    }

    #[test]
    fn dimension_mismatch_is_a_clean_error() {
        let oracle = ppm_from_levels(2, 2, &[[0, 0, 0]; 4], &ORACLE_RAMP);
        let ours = ppm_from_levels(3, 2, &[[0, 0, 0]; 6], &ours_ramp());
        let err = compare(&oracle, &ours).unwrap_err();
        assert!(err.contains("dimension mismatch"), "got: {err}");
    }

    #[test]
    fn parse_ppm_round_trips_with_a_comment() {
        let pixels = vec![(0, 34, 238), (255, 0, 109), (12, 34, 56), (255, 255, 255)];
        let blob = make_ppm(2, 2, &pixels, Some(" ImageMagick"));
        let ppm = parse_ppm(&blob).unwrap();
        assert_eq!((ppm.width, ppm.height), (2, 2));
        assert_eq!(ppm.pixels, pixels);
    }

    #[test]
    fn parse_ppm_rejects_bad_magic_and_truncation() {
        assert!(parse_ppm(b"P3\n1 1\n255\n\x00\x00\x00").is_err()); // ASCII PPM, not P6
                                                                    // Header promises 2×2 but only one pixel of raster follows.
        assert!(parse_ppm(b"P6\n2 2\n255\n\x00\x00\x00").is_err());
    }
}
