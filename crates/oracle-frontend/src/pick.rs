//! Click-to-watch: resolving the pixel under the mouse to something you can arm a watch on.
//!
//! ## The gap this closes
//!
//! The first version of click-to-watch only understood plane and window pixels. It asked the VDP
//! [`pixel_attribution`](oracle_core::vdp::Vdp::pixel_attribution) who was showing at a dot, and if the answer
//! carried a nametable [`Cell`] it armed a VRAM watch on that cell's 32-byte tile. Every other answer — every
//! **sprite**, and the backdrop — printed "no tile watch this slice (follow-up)" and did nothing. In a Sonic
//! game the interesting things on screen are almost entirely sprites, so the first person to use the feature
//! in anger clicked a sprite and got nothing.
//!
//! A sprite pixel has exactly the same story to tell, in three parts rather than one:
//!
//! * **which sprite** — [`Layer::Sprite`] already carries the winning SAT index;
//! * **which VRAM tile it draws that dot from** — a multi-cell sprite is a *column-major* run of tiles from a
//!   base index, so the dot's tile is `base + (col * height_cells) + row` after flips
//!   ([`oracle_core::render::sprite_tile_at`]);
//! * **which attribute-table entry positions it** — the 8 bytes at `sat_base + index * 8`, which is what a
//!   game writes when it moves, re-points, or re-links the sprite.
//!
//! So a sprite click arms *two* watches — the tile and the SAT entry — and the answer to "who drew this?" and
//! "who moved this?" both land in the same hit log. A backdrop click arms the CRAM entry the backdrop register
//! selects, which is the only writable thing behind a backdrop dot.
//!
//! ## Where the sprite addressing lives
//!
//! Everything here is computed from public core API: `pixel_attribution`, `sprites_decoded`, `sat_base`,
//! and [`oracle_core::render::sprite_tile_at`]. That last one used to be a local copy that deliberately
//! *re-derived* `draw_sprite`'s addressing; it now lives in `oracle-core` beside the renderer it mirrors,
//! because the same derivation answers `emulator/pixel_attribution` on the bus (contract §8 item 19: one
//! implementation under both consumers, so the panel and the wire cannot drift). The tests below still pin
//! it *against the core's own renderer* rather than against the arithmetic being restated — so if the
//! core's sprite addressing ever changed, these tests fail rather than the picker silently naming the
//! wrong tile.

use oracle_core::render::{sprite_tile_at, Layer};
use oracle_core::vdp::Vdp;

/// One armable range the click resolved to: a watch to arm, and how to describe it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchTarget {
    /// Which VDP memory the range is in.
    pub space: Space,
    /// Inclusive byte range within that memory.
    pub lo: u32,
    pub hi: u32,
    /// Short label carried into the hit log.
    pub label: String,
}

/// The VDP memory a [`WatchTarget`] lives in. A local mirror of the core's `WatchSpace` so this module's
/// tests do not need a `Watchpoints` to talk about a range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    Vram,
    Cram,
}

/// What the clicked pixel turned out to be, ready to report and to arm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    /// The full human-readable line, for the terminal log — everything the click resolved.
    pub description: String,
    /// A short form for the on-screen toast. The overlay draws at window resolution but a 960-pixel window
    /// still only fits ~50 characters at a legible size, so the toast names *what was armed* and the terminal
    /// keeps the detail.
    pub toast: String,
    /// The ranges to arm. Empty only if the pixel resolved to nothing armable (which, since sprites and the
    /// backdrop are both handled, no longer happens).
    pub targets: Vec<WatchTarget>,
}

/// Bytes per VDP pattern (tile): 8 rows x 4 bytes.
const TILE_BYTES: u32 = 32;
/// VRAM size, the modulus every VDP-internal address is taken in (`oracle_core`'s `VRAM_SIZE`).
const VRAM_MASK: u32 = 0xFFFF;
/// Bytes per SAT entry.
const SAT_ENTRY_BYTES: u32 = 8;

/// The inclusive VRAM byte range of pattern `tile`, wrapped into VRAM exactly as the core's `tile_nibble`
/// addressing does. A 32-byte block cannot straddle the 64 KB wrap (65536 is a multiple of 32), so this is a
/// single contiguous range.
fn tile_range(tile: u16) -> (u32, u32) {
    let lo = (u32::from(tile) * TILE_BYTES) & VRAM_MASK;
    (lo, lo + TILE_BYTES - 1)
}

/// Resolve the dot at `(x, y)` into a description and the ranges worth watching.
///
/// Plane and window winners keep the behaviour they always had (the winning cell's tile). Sprite winners are
/// the fix: the sprite's own tile for *this* dot, plus its SAT entry. A backdrop winner arms the CRAM entry
/// the backdrop register points at.
pub fn resolve(vdp: &Vdp, x: u16, y: u16) -> Pick {
    let attr = vdp.pixel_attribution(x, y);
    match attr.winner {
        Layer::Sprite(index) => {
            // `sprites_decoded` reads the same cached Y/size/link and VRAM X/attribute the renderer's walk
            // does, so the geometry here is the geometry that drew the pixel.
            let sprites = vdp.sprites_decoded();
            let Some(s) = sprites.get(usize::from(index)) else {
                return Pick {
                    description: format!("pixel ({x},{y}): sprite {index} is out of range"),
                    toast: format!("SPRITE {index}: OUT OF RANGE"),
                    targets: Vec::new(),
                };
            };
            let sat_lo = (vdp.sat_base() as u32 + u32::from(index) * SAT_ENTRY_BYTES) & VRAM_MASK;
            let sat_hi = sat_lo + SAT_ENTRY_BYTES - 1;
            let mut targets = vec![WatchTarget {
                space: Space::Vram,
                lo: sat_lo,
                hi: sat_hi,
                label: format!("sprite {index} SAT entry"),
            }];
            let mut short_tile = "TILE ?".to_string();
            let tile_note = match sprite_tile_at(s, x, y) {
                Some(tile) => {
                    short_tile = format!("TILE ${tile:03X}");
                    let (lo, hi) = tile_range(tile);
                    targets.insert(
                        0,
                        WatchTarget {
                            space: Space::Vram,
                            lo,
                            hi,
                            label: format!("sprite {index} tile ${tile:03X}"),
                        },
                    );
                    format!("tile ${tile:03X} @ VRAM ${lo:04X}-${hi:04X}")
                }
                // The winner's box not containing the dot would mean the SAT changed between the render and
                // this query (a mid-frame SAT rewrite). Report it rather than inventing a tile.
                None => "tile unresolved (the SAT moved since the frame was drawn)".to_string(),
            };
            let flips = match (s.hflip, s.vflip) {
                (false, false) => "",
                (true, false) => " hflip",
                (false, true) => " vflip",
                (true, true) => " hflip+vflip",
            };
            Pick {
                description: format!(
                    "sprite {index} at ({},{}) {}x{} cells, base ${:03X}, pal {}{flips}{} — {tile_note}, \
                     SAT entry @ VRAM ${sat_lo:04X}-${sat_hi:04X}",
                    s.x,
                    s.y,
                    s.width_cells,
                    s.height_cells,
                    s.tile,
                    s.palette,
                    if s.priority { " hi-pri" } else { "" },
                ),
                toast: format!("WATCH SPRITE {index} {short_tile} + SAT ${sat_lo:04X}"),
                targets,
            }
        }
        Layer::Backdrop => {
            // Nothing is *drawn* at a backdrop dot — the only writable thing behind it is the palette entry
            // reg $07 selects, so that is what a "who changes this?" question means here.
            let idx = u32::from(attr.cram_index);
            Pick {
                description: format!(
                    "backdrop at ({x},{y}) — CRAM entry {} (palette {}, colour {}) @ CRAM ${:02X}-${:02X}",
                    attr.cram_index,
                    attr.cram_index / 16,
                    attr.cram_index % 16,
                    idx * 2,
                    idx * 2 + 1
                ),
                toast: format!("WATCH BACKDROP CRAM {}", attr.cram_index),
                targets: vec![WatchTarget {
                    space: Space::Cram,
                    lo: idx * 2,
                    hi: idx * 2 + 1,
                    label: format!("backdrop CRAM {}", attr.cram_index),
                }],
            }
        }
        Layer::PlaneA | Layer::PlaneB | Layer::Window => {
            let plane = match attr.winner {
                Layer::PlaneA => "plane A",
                Layer::PlaneB => "plane B",
                _ => "window",
            };
            // `cell` is `Some` for exactly these three winners (the core returns `None` only for
            // sprite/backdrop), but the fallback keeps the picker total rather than unwrapping.
            let Some(cell) = attr.cell else {
                return Pick {
                    description: format!(
                        "pixel ({x},{y}) is {plane}, but the VDP reported no cell"
                    ),
                    toast: "NO CELL REPORTED".to_string(),
                    targets: Vec::new(),
                };
            };
            let (lo, hi) = tile_range(cell.tile);
            Pick {
                description: format!(
                    "{plane} tile ${:03X} (pal {}{}) @ VRAM ${lo:04X}-${hi:04X} — click ({x},{y})",
                    cell.tile,
                    cell.palette,
                    if cell.priority { " hi-pri" } else { "" },
                ),
                toast: format!("WATCH {plane} TILE ${:03X}", cell.tile),
                targets: vec![WatchTarget {
                    space: Space::Vram,
                    lo,
                    hi,
                    label: format!("tile ${:03X}", cell.tile),
                }],
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_core::render::{Layer, SpriteDecoded};
    use oracle_core::rng::SplitMix64;
    use oracle_core::vdp::Vdp;

    /// The sprite's base pattern index, and where its 4-cell-square worth of patterns live.
    const BASE_TILE: u16 = 0x10;
    /// SAT base for the fixtures: reg 5 = $58 → `($58 & $7E) << 9` = $B000.
    const SAT_BASE: u16 = 0xB000;
    /// Screen position of the fixture sprite (both axes), i.e. the SAT fields are this + 128.
    const SPRITE_AT: u16 = 64;

    /// A blank VDP, driven only through its **public ports** — no private-field pokes are available from this
    /// crate, and using the real write path is better anyway: it is what keeps the SAT cache write-through
    /// (which the sprite walk reads Y/size/link from) in step with VRAM.
    fn fresh() -> Vdp {
        let mut rng = SplitMix64::new(0x5EED);
        let mut v = Vdp::power_on(&mut rng);
        v.vram_mut().fill(0); // power-on VRAM is pseudo-random; a blank sheet is what these tests want
        v
    }

    /// Write VDP register `reg` (control port, `10rrrrr_vvvvvvvv`).
    fn set_reg(v: &mut Vdp, reg: u8, val: u8) {
        v.control_write(0x8000 | (u16::from(reg) << 8) | u16::from(val), 0);
    }

    /// Point the data port at `addr` with access `code` (1 = VRAM write, 3 = CRAM write) — the two-word
    /// control-port command.
    fn set_addr(v: &mut Vdp, code: u8, addr: u16) {
        v.control_write(((u16::from(code) & 0x03) << 14) | (addr & 0x3FFF), 0);
        v.control_write(((u16::from(code) >> 2) << 4) | (addr >> 14), 0);
    }

    /// Write consecutive words into VRAM through the data port (autoinc 2), so the SAT cache mirrors.
    fn write_vram(v: &mut Vdp, addr: u16, words: &[u16]) {
        set_addr(v, 0x01, addr);
        for w in words {
            v.data_write(*w);
        }
    }

    /// A VDP showing one sprite of `w_cells x h_cells` at (64, 64), with `w_cells * h_cells` distinguishable
    /// patterns behind it.
    ///
    /// Pattern `BASE_TILE + n` is filled solid with colour **nibble `n + 1`** — unique for `n` in `0..15`.
    /// The rendered pixel's CRAM index is `palette * 16 + nibble`, so the colour the core resolves at a dot
    /// *names the pattern it drew that dot from*. That is what lets the test below check [`sprite_tile_at`]
    /// against the core's own renderer rather than restating the addressing arithmetic on both sides.
    ///
    /// Everything else is blank: both plane bases sit at $0000 over zeroed VRAM, so every plane cell is
    /// pattern 0 (also zeroed) and therefore transparent, leaving the sprite the winner at every dot of its
    /// box.
    fn vdp_with_sprite(w_cells: u8, h_cells: u8, hflip: bool, vflip: bool) -> Vdp {
        assert!(
            usize::from(w_cells) * usize::from(h_cells) <= 15,
            "the fixture gives each cell a unique colour nibble (1..=15)"
        );
        let mut v = fresh();
        // Reg $01 FIRST, and the order is load-bearing: the mode-4 register mask discards writes to
        // registers above 10 while M5 (reg $01 bit 2) is clear, so an $0C written ahead of it is
        // silently dropped and this fixture comes up H32 while the comment claims H40.
        set_reg(&mut v, 0x01, 0x74); // display on, mode 5, DMA enable
        set_reg(&mut v, 0x0C, 0x81); // H40 — set before the SAT writes, the cache window depends on it
        set_reg(&mut v, 0x05, 0x58); // SAT base $B000
        set_reg(&mut v, 0x07, 0x00); // backdrop = CRAM 0
        set_reg(&mut v, 0x0F, 0x02); // autoincrement 2 (one word per data write)
        set_reg(&mut v, 0x10, 0x00); // 32x32 planes

        for n in 0..16u16 {
            let nib = u16::from((n as u8 % 15) + 1);
            let word = (nib << 12) | (nib << 8) | (nib << 4) | nib;
            write_vram(&mut v, (BASE_TILE + n) * 32, &[word; 16]);
        }

        let attr: u16 = (u16::from(vflip) << 12) | (u16::from(hflip) << 11) | BASE_TILE;
        let size = u16::from(((w_cells - 1) << 2) | (h_cells - 1));
        write_vram(
            &mut v,
            SAT_BASE,
            &[
                SPRITE_AT + 128, // Y field (cached)
                size << 8, // size in the high byte, link 0 in the low — link 0 ends the walk here
                attr,      // tile / palette / flips / priority (read from VRAM at render time)
                SPRITE_AT + 128, // X field (ditto)
            ],
        );
        v
    }

    /// **The pin that matters**: for every dot of a sprite, the tile this module names is the tile the
    /// *core's own renderer* drew that dot from. Checked by making each pattern a solid, unique colour, so the
    /// rendered CRAM index identifies the pattern — no restatement of the addressing arithmetic.
    #[test]
    fn the_named_tile_is_the_one_the_core_actually_drew_from() {
        // Sizes chosen so `w * h <= 15`: the fixture gives every cell a unique colour nibble, and nibble 0 is
        // transparent, so 15 is the ceiling. (3,2) and (2,3) are the pair that catch a row-major mix-up.
        for (w, h) in [(1u8, 1u8), (2, 2), (4, 1), (1, 4), (3, 2), (2, 3), (4, 3)] {
            for (hflip, vflip) in [(false, false), (true, false), (false, true), (true, true)] {
                let v = vdp_with_sprite(w, h, hflip, vflip);
                let sprites = v.sprites_decoded();
                let s = &sprites[0];
                assert_eq!((s.width_cells, s.height_cells), (w, h));
                assert_eq!((s.x, s.y), (SPRITE_AT as i16, SPRITE_AT as i16));
                assert_eq!((s.hflip, s.vflip), (hflip, vflip));
                let mut checked = 0;
                for dy in 0..usize::from(h) * 8 {
                    for dx in 0..usize::from(w) * 8 {
                        let (x, y) = (SPRITE_AT + dx as u16, SPRITE_AT + dy as u16);
                        let attr = v.pixel_attribution(x, y);
                        // Every pattern is fully opaque, so the sprite must win at every dot of its box.
                        assert_eq!(
                            attr.winner,
                            Layer::Sprite(0),
                            "{w}x{h} hflip={hflip} vflip={vflip}: sprite must win at ({x},{y})"
                        );
                        let named = sprite_tile_at(s, x, y).expect("the dot is inside the sprite");
                        // The renderer's own answer for which pattern it used: `PixelAttribution` does not
                        // carry the sprite's tile, but each pattern is a unique solid colour, so the CRAM
                        // index does — pattern `BASE_TILE + n` is nibble `n + 1` in palette 0.
                        let nibble = attr.cram_index % 16;
                        let want_nibble = (named - BASE_TILE) as u8 + 1;
                        assert_eq!(
                            nibble, want_nibble,
                            "{w}x{h} hflip={hflip} vflip={vflip} at ({x},{y}): named tile ${named:03X} \
                             but the renderer drew colour nibble {nibble}"
                        );
                        checked += 1;
                    }
                }
                assert_eq!(checked, usize::from(w) * usize::from(h) * 64);
            }
        }
    }

    /// A dot outside the sprite's box has no tile — the picker must say so rather than index past the sprite.
    #[test]
    fn a_dot_outside_the_sprite_has_no_tile() {
        let v = vdp_with_sprite(2, 2, false, false);
        let s = &v.sprites_decoded()[0];
        let a = SPRITE_AT;
        assert_eq!(
            sprite_tile_at(s, a - 1, a),
            None,
            "one dot left of the sprite"
        );
        assert_eq!(sprite_tile_at(s, a, a - 1), None, "one row above");
        assert_eq!(
            sprite_tile_at(s, a + 16, a),
            None,
            "one dot right of a 2-cell sprite"
        );
        assert_eq!(sprite_tile_at(s, a, a + 16), None, "one row below");
        assert_eq!(
            sprite_tile_at(s, 0, 0),
            None,
            "far outside, no underflow panic"
        );
    }

    /// **The bug, from the user's side**: clicking a sprite pixel now yields armable ranges and a real
    /// description, where it used to yield the "no tile watch this slice" refusal.
    #[test]
    fn clicking_a_sprite_arms_its_tile_and_its_sat_entry() {
        let v = vdp_with_sprite(2, 2, false, false);
        let pick = resolve(&v, 70, 70);
        assert_eq!(
            pick.targets.len(),
            2,
            "the tile and the SAT entry: {pick:?}"
        );

        let tile_t = &pick.targets[0];
        assert_eq!(tile_t.space, Space::Vram);
        assert_eq!(tile_t.hi - tile_t.lo, 31, "a pattern is 32 bytes");
        let tile = sprite_tile_at(&v.sprites_decoded()[0], 70, 70).unwrap();
        assert_eq!(tile_t.lo, u32::from(tile) * 32);

        let sat_t = &pick.targets[1];
        assert_eq!(sat_t.space, Space::Vram);
        assert_eq!(
            sat_t.lo,
            u32::from(SAT_BASE),
            "sprite 0's entry is at the SAT base"
        );
        assert_eq!(sat_t.hi - sat_t.lo, 7, "a SAT entry is 8 bytes");

        assert!(
            pick.description.contains("sprite 0"),
            "{}",
            pick.description
        );
        assert!(
            pick.description.contains("2x2 cells"),
            "{}",
            pick.description
        );
        assert!(
            !pick.description.contains("follow-up"),
            "the old refusal must be gone: {}",
            pick.description
        );
    }

    /// A backdrop dot arms the CRAM entry the backdrop register selects — the only writable thing behind it.
    #[test]
    fn clicking_the_backdrop_arms_its_palette_entry() {
        let mut v = vdp_with_sprite(1, 1, false, false);
        set_reg(&mut v, 0x07, 0x25); // backdrop = CRAM entry $25
        let pick = resolve(&v, 8, 8); // well away from the sprite at (64,64)
        assert_eq!(pick.targets.len(), 1);
        let t = &pick.targets[0];
        assert_eq!(t.space, Space::Cram);
        assert_eq!((t.lo, t.hi), (0x25 * 2, 0x25 * 2 + 1), "one CRAM word");
        assert!(
            pick.description.contains("backdrop"),
            "{}",
            pick.description
        );
        assert!(
            pick.description.contains("CRAM entry 37"),
            "{}",
            pick.description
        );
    }

    /// The plane path is unchanged: a nametable winner still arms exactly its one 32-byte pattern.
    #[test]
    fn clicking_a_plane_tile_still_arms_that_tile() {
        let mut v = fresh();
        set_reg(&mut v, 0x01, 0x74); // display on, mode 5 — before $0C, see `vdp_with_sprite`
        set_reg(&mut v, 0x0C, 0x81); // H40
        set_reg(&mut v, 0x02, 0x30); // plane A nametable @ $C000
        set_reg(&mut v, 0x04, 0x07); // plane B nametable @ $E000
        set_reg(&mut v, 0x05, 0x58); // SAT @ $B000 (empty — sprite 0's Y is 0-128 = off-screen)
        set_reg(&mut v, 0x0F, 0x02); // autoincrement 2
        set_reg(&mut v, 0x10, 0x00); // 32x32 planes
                                     // Plane A cell (0,0) → pattern $055, palette 1; the pattern itself is solid nibble 3 (opaque).
        write_vram(&mut v, 0xC000, &[(1 << 13) | 0x055]);
        write_vram(&mut v, 0x055 * 32, &[0x3333; 16]);

        let pick = resolve(&v, 2, 2);
        assert_eq!(pick.targets.len(), 1);
        assert_eq!(
            (pick.targets[0].lo, pick.targets[0].hi),
            (0x055 * 32, 0x055 * 32 + 31)
        );
        assert!(pick.description.contains("plane A"), "{}", pick.description);
        assert!(pick.description.contains("$055"), "{}", pick.description);
    }

    /// A base tile near the top of VRAM wraps exactly as the core's addressing does, and the armed range is
    /// still a single 32-byte block inside VRAM (65536 is a multiple of 32, so a pattern never straddles).
    #[test]
    fn a_tile_index_that_wraps_stays_inside_vram() {
        let s = SpriteDecoded {
            index: 0,
            y: 0,
            x: 0,
            width_cells: 2,
            height_cells: 2,
            link: 0,
            tile: 0xFFFF,
            palette: 0,
            hflip: false,
            vflip: false,
            priority: false,
            cache_divergence: false,
        };
        // The bottom-right cell is base + (1 * 2) + 1 = base + 3 → wraps to $0002.
        assert_eq!(sprite_tile_at(&s, 8, 8), Some(0x0002));
        let (lo, hi) = tile_range(0xFFFF);
        assert!(
            hi <= VRAM_MASK,
            "the armed range stays inside VRAM: ${lo:04X}-${hi:04X}"
        );
        assert_eq!(hi - lo, 31);
    }
}
