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

use oracle_core::render::{sprite_tile_at, Layer, LayerMask};
use oracle_core::vdp::Vdp;

/// The clause every answer carries when a display layer is hidden, or `None` when none is.
///
/// **The panel must describe the picture on screen, and say so when that is not the whole machine.** A dot
/// where plane A is hidden resolves to plane B — correctly, because plane B is what the window is painting
/// there — and an answer that said `plane B` without saying `planeA is hidden` would be a true sentence a
/// reader cannot help but take as false. This is `loud-on-unmeasurable` arriving on the panel: the honest
/// answer names the lens it was taken through.
///
/// It reads [`LayerMask::hidden`], the same derivation the bus's screenshot caveat and the window's standing
/// badge read, so the three cannot name different layers.
fn mask_clause(mask: LayerMask) -> Option<String> {
    let hidden = mask.hidden();
    if hidden.is_empty() {
        return None;
    }
    Some(format!(
        "{} hidden, so this is the masked picture, not the machine's",
        hidden.join(" + ")
    ))
}

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
    /// **A sentence a person reads**, and the first thing printed. Says what was clicked, in words, before
    /// any address or index appears.
    ///
    /// The rule comes from a measured failure in the editor lane, adopted here: they shipped a lens that
    /// highlighted 1,244 cells, entirely correctly, and the reaction was *"what are the purple boxes"* — not
    /// *that's wrong* but ***what is that***. A feature that works perfectly and communicates nothing.
    /// `[planeB:won, backdrop:lostToPriority]` is the right data and the wrong answer. So the structured
    /// detail keeps its place; it just stops being the top line.
    pub headline: String,
    /// The full human-readable line, for the terminal log — the headline, then everything the click
    /// resolved. Kept as one string because it is what goes to stdout, and the two halves are never wanted
    /// apart there.
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

/// **The space a tile index is in, stated in the answer.** Every tile this module names is an index into
/// VRAM patterns — `tileAddr == tile * 32` — and nothing else.
///
/// It is spelled out because *an index whose space is unstated is a transpose bug waiting to happen* (the
/// editor lane's formulation, out of their own injector). Their model rebases this index into a blob-local
/// slot with a base constant **they** own; this panel deliberately does not do that arithmetic and does not
/// name a slot in their space. The failure it avoids is not a throw: a rebase can land outside the artwork
/// while still passing a naive capacity check, and what comes out is a confident wrong slot, which is
/// indistinguishable from a correct answer. *In-capacity is not in-blob.* Naming the space we do own, and
/// only that, is what keeps the join theirs to make.
const TILE_SPACE: &str = "VRAM-absolute";

/// Assemble the answer: the sentence, then the mask clause if there is one, then the detail.
///
/// One function so no caller can drop the mask clause on a path that happens not to think about masks —
/// which is precisely how the invariant this parcel closes went unasserted in the first place.
fn describe(headline: String, mask: LayerMask, detail: String) -> (String, String) {
    let headline = match mask_clause(mask) {
        Some(c) => format!("{headline} ({c})"),
        None => headline,
    };
    let description = format!("{headline} — {detail}");
    (headline, description)
}

/// Resolve the dot at `(x, y)` into a description and the ranges worth watching, **under the display mask
/// the picture was drawn with**.
///
/// Plane and window winners keep the behaviour they always had (the winning cell's tile). Sprite winners
/// name the sprite's own tile for *this* dot, plus its SAT entry. A backdrop winner arms the CRAM entry the
/// backdrop register points at.
///
/// # The mask is a parameter, and there is no unmasked twin to fall into
///
/// The panel's whole job is to describe **the picture on screen**. Once the window could hide a layer, an
/// unmasked `pixel_attribution` stopped doing that: hide plane A, click where it used to be, and an unmasked
/// panel names plane A while the window paints plane B. That is not a near-miss, it is the wrong object
/// armed for a watch, reported confidently.
///
/// So the mask comes in from the caller — the same `LayerMask` the engine holds and the renderer just used,
/// never a second one — exactly as `Engine::framebuffer` takes its mask explicitly and for the same stated
/// reason: each call site says which picture it means. [`LayerMask::ALL`] is the old behaviour precisely,
/// because the mask reaches only `resolve_dot`'s candidate tests.
///
/// The bus-parity guard below now runs over masked states as well as the default, so "this panel and
/// `emulator/pixel_attribution` never disagree" is an assertion rather than a precondition nobody checks.
pub fn resolve(vdp: &Vdp, x: u16, y: u16, mask: LayerMask) -> Pick {
    let attr = vdp.pixel_attribution_masked(x, y, mask);
    match attr.winner {
        Layer::Sprite(index) => {
            // `sprites_decoded` reads the same cached Y/size/link and VRAM X/attribute the renderer's walk
            // does, so the geometry here is the geometry that drew the pixel.
            let sprites = vdp.sprites_decoded();
            let Some(s) = sprites.get(usize::from(index)) else {
                let (headline, description) = describe(
                    format!(
                        "That dot is a sprite, but sprite {index} is out of range — nothing to watch"
                    ),
                    mask,
                    format!("pixel ({x},{y}): sprite {index} is out of range"),
                );
                return Pick {
                    headline,
                    description,
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
            let mut named_tile = None;
            let tile_note = match sprite_tile_at(s, x, y) {
                Some(tile) => {
                    short_tile = format!("TILE ${tile:03X}");
                    named_tile = Some(tile);
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
            // The sentence. It names the subject — *a sprite*, which one, and what it is drawn from — before
            // any of the addressing below. `Sprite 12` is the most specific thing we can honestly say: the
            // SAT index is the hardware's own name for it, and this panel does **not** claim to know which
            // game object put it there (see the module docs' closing note on that).
            let drawn_from = match named_tile {
                Some(tile) => format!("drawn from {TILE_SPACE} tile ${tile:03X}"),
                None => {
                    "whose tile could not be resolved (the SAT moved since the frame was drawn)"
                        .to_string()
                }
            };
            let (headline, description) = describe(
                format!("That dot is sprite {index}, {drawn_from}."),
                mask,
                format!(
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
            );
            Pick {
                headline,
                description,
                toast: format!("WATCH SPRITE {index} {short_tile} + SAT ${sat_lo:04X}"),
                targets,
            }
        }
        Layer::Backdrop => {
            // Nothing is *drawn* at a backdrop dot — the only writable thing behind it is the palette entry
            // reg $07 selects, so that is what a "who changes this?" question means here.
            let idx = u32::from(attr.cram_index);
            let (headline, description) = describe(
                format!(
                    "Nothing is drawn at ({x},{y}) — you clicked the backdrop, so the colour comes \
                     straight from palette entry {}.",
                    attr.cram_index
                ),
                mask,
                format!(
                    "backdrop at ({x},{y}) — CRAM entry {} (palette {}, colour {}) @ CRAM ${:02X}-${:02X}",
                    attr.cram_index,
                    attr.cram_index / 16,
                    attr.cram_index % 16,
                    idx * 2,
                    idx * 2 + 1
                ),
            );
            Pick {
                headline,
                description,
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
                let (headline, description) = describe(
                    format!("That dot is {plane}, but the VDP reported no cell for it."),
                    mask,
                    format!("pixel ({x},{y}) is {plane}, but the VDP reported no cell"),
                );
                return Pick {
                    headline,
                    description,
                    toast: "NO CELL REPORTED".to_string(),
                    targets: Vec::new(),
                };
            };
            let (lo, hi) = tile_range(cell.tile);
            let (headline, description) = describe(
                format!(
                    "That dot is {plane}, drawn from {TILE_SPACE} tile ${:03X}.",
                    cell.tile
                ),
                mask,
                format!(
                    "{plane} tile ${:03X} (pal {}{}) @ VRAM ${lo:04X}-${hi:04X} — click ({x},{y})",
                    cell.tile,
                    cell.palette,
                    if cell.priority { " hi-pri" } else { "" },
                ),
            );
            Pick {
                headline,
                description,
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
        let pick = resolve(&v, 70, 70, LayerMask::ALL);
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
        let pick = resolve(&v, 8, 8, LayerMask::ALL); // well away from the sprite at (64,64)
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

        let pick = resolve(&v, 2, 2, LayerMask::ALL);
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

    // ---------------------------------------------------------------------------------------------
    // The parity invariant — contract §8 item 19's whole point
    // ---------------------------------------------------------------------------------------------

    /// **This panel and the `emulator/pixel_attribution` bus method must never disagree**, and the
    /// guard lives here rather than in `oracle-aether/tests/` for a structural reason: `oracle-frontend`
    /// depends on `oracle-aether`, so only this crate can see both sides at once.
    ///
    /// §8 item 19 mandates the *capability* on the bus; D15 argues explicitly against the panel reaching
    /// it through a socket round-trip per repaint ("an in-process GUI is a consumer of the same registry,
    /// not a second server"), and our `Host::pump` arrangement makes that worse — a click would have to
    /// enqueue a command and wait a frame to answer a question it can answer synchronously. So the panel
    /// keeps calling core, and *this test* is what makes "one implementation under both consumers"
    /// checkable rather than merely intended. Moving `sprite_tile_at` into `oracle-core` is what makes it
    /// true; if the two ever drift, this is the assertion that says so.
    #[cfg(feature = "aether")]
    mod bus_parity {
        use super::*;
        use oracle_aether::engine::{Engine, EngineConfig};
        use oracle_aether::outbound::Subscribers;
        use oracle_core::system::System;
        use serde_json::{json, Value};

        /// An engine whose machine shows `v`.
        fn engine_showing(v: &Vdp) -> Engine {
            let mut sys = System::new(0x5EED);
            sys.load_rom(oracle_core::testrom::build());
            sys.reset();
            *sys.vdp_mut() = v.clone();
            Engine::new(sys, EngineConfig::default(), Subscribers::new())
        }

        /// `"0x0000B000"` → `0xB000`. The bus spells addresses as hex strings (D9 category 1); the panel
        /// carries them as numbers, so the comparison has to cross that boundary explicitly.
        fn addr_of(v: &Value) -> u32 {
            let s = v
                .as_str()
                .unwrap_or_else(|| panic!("an address string, got {v}"));
            u32::from_str_radix(s.trim_start_matches("0x"), 16).expect("hex")
        }

        fn attribution(e: &mut Engine, x: u16, y: u16) -> Value {
            e.dispatch("emulator/pixel_attribution", &json!({"x": x, "y": y}))
                .expect("a dot inside the active display must answer")
        }

        /// Sprite dots: the tile the panel arms a watch on is the tile the bus reports, for every dot of
        /// a 3x2 and a 2x3 sprite under all four flips — and the SAT entry likewise. A column-major /
        /// row-major split between the two would surface here as a mismatched `lo`.
        #[test]
        fn the_panel_and_the_bus_name_the_same_sprite_tile_and_sat_entry() {
            for (w, h) in [(3u8, 2u8), (2, 3), (1, 1), (4, 1)] {
                for (hflip, vflip) in [(false, false), (true, false), (false, true), (true, true)] {
                    let v = vdp_with_sprite(w, h, hflip, vflip);
                    let mut e = engine_showing(&v);
                    for dy in 0..usize::from(h) * 8 {
                        for dx in 0..usize::from(w) * 8 {
                            let (x, y) = (SPRITE_AT + dx as u16, SPRITE_AT + dy as u16);
                            let r = attribution(&mut e, x, y);
                            let p = resolve(&v, x, y, LayerMask::ALL);

                            assert_eq!(r["winner"]["layer"], json!("sprite"), "({x},{y})");
                            assert_eq!(r["winner"]["spriteIndex"], json!(0), "({x},{y})");
                            assert_eq!(
                                p.targets[0].lo,
                                addr_of(&r["sprite"]["tileAddr"]),
                                "{w}x{h} hflip={hflip} vflip={vflip} at ({x},{y}): the panel arms \
                                 ${:04X} but the bus names {} — the two have DRIFTED",
                                p.targets[0].lo,
                                r["sprite"]["tileAddr"]
                            );
                            assert_eq!(p.targets[0].space, Space::Vram);
                            assert_eq!(
                                p.targets[1].lo,
                                addr_of(&r["sprite"]["satAddr"]),
                                "({x},{y}): SAT entry"
                            );
                            // And the tile index itself, as the panel prints it into its description.
                            let tile = r["sprite"]["tile"].as_u64().expect("tile") as u16;
                            assert!(
                                p.description.contains(&format!("tile ${tile:03X}")),
                                "({x},{y}): the panel says {:?}, the bus says tile ${tile:03X}",
                                p.description
                            );
                        }
                    }
                }
            }
        }

        /// Plane dots: the panel's armed pattern range is the bus's `cell.tileAddr`, and the two agree on
        /// the winning layer. Backdrop dots: the panel's armed CRAM word is the bus's `cramAddr`, and on
        /// `cramIndex` — which is the number the panel prints to a person.
        #[test]
        fn the_panel_and_the_bus_agree_on_plane_cells_and_on_the_backdrop() {
            let mut v = fresh();
            set_reg(&mut v, 0x01, 0x74); // display on, mode 5 — before $0C
            set_reg(&mut v, 0x0C, 0x81); // H40
            set_reg(&mut v, 0x02, 0x30); // plane A nametable @ $C000
            set_reg(&mut v, 0x04, 0x07); // plane B nametable @ $E000
            set_reg(&mut v, 0x05, 0x58); // SAT @ $B000, empty
            set_reg(&mut v, 0x07, 0x25); // backdrop = CRAM entry $25
            set_reg(&mut v, 0x0F, 0x02);
            set_reg(&mut v, 0x10, 0x00);
            write_vram(&mut v, 0xC000, &[(1 << 13) | 0x055]);
            write_vram(&mut v, 0x055 * 32, &[0x3333; 16]);
            let mut e = engine_showing(&v);

            // The one opaque plane-A cell.
            let r = attribution(&mut e, 2, 2);
            let p = resolve(&v, 2, 2, LayerMask::ALL);
            assert_eq!(r["winner"]["layer"], json!("planeA"));
            assert_eq!(p.targets[0].lo, addr_of(&r["cell"]["tileAddr"]));
            assert_eq!(p.targets[0].hi, addr_of(&r["cell"]["tileAddr"]) + 31);
            assert_eq!(r["cell"]["tile"], json!(0x055));
            assert!(p.description.contains("$055"), "{}", p.description);

            // Everywhere else is backdrop.
            let r = attribution(&mut e, 200, 100);
            let p = resolve(&v, 200, 100, LayerMask::ALL);
            assert_eq!(r["winner"]["layer"], json!("backdrop"));
            assert_eq!(r["cramIndex"], json!(0x25));
            assert_eq!(p.targets[0].space, Space::Cram);
            assert_eq!(p.targets[0].lo, addr_of(&r["cramAddr"]));
            assert!(
                p.description.contains("CRAM entry 37"),
                "0x25 = 37, the number the panel shows a person: {}",
                p.description
            );
        }

        /// The panel is total over the whole active display; the bus refuses outside it (§3.5). That
        /// difference is deliberate — the core's totality is right in-process and a silent wrong answer
        /// on a wire — so it is pinned rather than left to look like an accident.
        #[test]
        fn the_bus_refuses_a_dot_the_panel_would_still_answer() {
            let v = vdp_with_sprite(1, 1, false, false);
            let mut e = engine_showing(&v);
            // 400 is past the H40 active width; the core answers backdrop, the bus refuses.
            assert_eq!(v.pixel_attribution(400, 10).winner, Layer::Backdrop);
            let err = e
                .dispatch("emulator/pixel_attribution", &json!({"x": 400, "y": 10}))
                .expect_err("outside the active display");
            assert_eq!(err.code, -32004);
            let data = err
                .data
                .expect("-32004 must carry the bound it refused against");
            assert_eq!(data["width"], json!(320));
            assert_eq!(data["height"], json!(224));
        }

        // -----------------------------------------------------------------------------------------
        // The same invariant, with its precondition removed
        // -----------------------------------------------------------------------------------------

        /// One dot with **four** different right answers, depending on what is hidden.
        ///
        /// `vdp_with_sprite` alone cannot catch a masked/unmasked split: its planes are transparent, so
        /// hiding plane A changes nothing and an unmasked panel keeps agreeing with a masked bus by
        /// coincidence. That is the "green poison with the guard sound" shape — the row would pass with
        /// the rule broken — so the fixture is built for the opposite: at `(70,70)` a sprite covers an
        /// opaque plane-A cell which covers an opaque plane-B cell over a non-zero backdrop, and every one
        /// of the four layers is the winner under some mask.
        ///
        /// Returns the VDP and the two plane tiles, so the assertions read the expected answers off the
        /// fixture rather than restating them.
        fn vdp_with_four_answers() -> (Vdp, u16, u16) {
            const A_TILE: u16 = 0x055;
            const B_TILE: u16 = 0x066;
            let mut v = vdp_with_sprite(2, 2, false, false);
            set_reg(&mut v, 0x02, 0x30); // plane A nametable @ $C000
            set_reg(&mut v, 0x04, 0x07); // plane B nametable @ $E000
            set_reg(&mut v, 0x07, 0x25); // backdrop = CRAM $25, so "backdrop" is distinguishable
                                         // Two opaque patterns, well clear of the sprite's $010-$01F block and of both
                                         // nametables.
            write_vram(&mut v, A_TILE * 32, &[0x3333; 16]);
            write_vram(&mut v, B_TILE * 32, &[0x5555; 16]);
            // The cell containing dot (70,70) in a 32x32 plane with no scroll: column 8, row 8.
            let cell = |col: u16, row: u16| (row * 32 + col) * 2;
            write_vram(&mut v, 0xC000 + cell(8, 8), &[(1 << 13) | A_TILE]);
            write_vram(&mut v, 0xE000 + cell(8, 8), &[(2 << 13) | B_TILE]);
            (v, A_TILE, B_TILE)
        }

        /// Hide layers **through the served method**, so the path under test is the one a client uses.
        fn hide(e: &mut Engine, layers: &[&str]) {
            for l in layers {
                e.dispatch(
                    "emulator/set_layer_enabled",
                    &json!({"layer": l, "enabled": false}),
                )
                .unwrap_or_else(|err| panic!("set_layer_enabled({l}) refused: {err:?}"));
            }
        }

        /// **The invariant, asserted rather than noted.** The panel and `emulator/pixel_attribution` agree
        /// at one dot under every one of the four masks that change its winner — not only under the
        /// default, which is the single state the guard used to run in.
        ///
        /// A rule with an unasserted precondition is this workspace's recurring defect, and this row is
        /// what removes the precondition: it drives the real `emulator/set_layer_enabled`, so the engine's
        /// mask and the panel's argument are provably the same value, and it checks the **winner** — the
        /// thing a mask actually moves — not only an address that a coincidence could keep aligned.
        ///
        /// Planting the drift: change `resolve` back to `vdp.pixel_attribution(x, y)` (the unmasked call
        /// this parcel replaced) and this fails on the `sprites` step with
        /// *"hiding [\"sprites\"]: the bus says planeA and the panel armed a range that is not the cell's
        /// — the two have DRIFTED"*, because the panel is still resolving the sprite the window has
        /// stopped drawing. Verified before this row was believed.
        #[test]
        fn the_panel_and_the_bus_agree_under_every_mask_that_changes_the_answer() {
            let (v, a_tile, b_tile) = vdp_with_four_answers();
            // (layers hidden, the wire's winner, the range the panel must arm)
            let steps: [(&[&str], &str, (u32, u32)); 4] = [
                (&[], "sprite", (0, 0)), // the sprite's tile is computed below, not restated
                (&["sprites"], "planeA", tile_range(a_tile)),
                (&["sprites", "planeA"], "planeB", tile_range(b_tile)),
                (
                    &["sprites", "planeA", "planeB"],
                    "backdrop",
                    (0x25 * 2, 0x25 * 2 + 1),
                ),
            ];
            for (hidden, want_layer, want_range) in steps {
                let mut e = engine_showing(&v);
                hide(&mut e, hidden);

                // The panel's mask is the engine's, not a second one assembled here: that is the whole
                // claim, so the test reads it back off the engine rather than building its own.
                let mask = e.layers();
                // Compared as sets: `hidden()` answers in `Layer::ALL` order, which is a property of the
                // core's own enumeration and deliberately not the order a caller happened to switch
                // things off in. What is being pinned here is *which* layers are hidden.
                let (mut got, mut want) = (mask.hidden(), hidden.to_vec());
                got.sort_unstable();
                want.sort_unstable();
                assert_eq!(
                    got, want,
                    "the engine's mask is not what set_layer_enabled was told to make it"
                );

                let r = attribution(&mut e, 70, 70);
                let p = resolve(&v, 70, 70, mask);

                assert_eq!(
                    r["winner"]["layer"],
                    json!(want_layer),
                    "hiding {hidden:?}: the BUS's own winner is not the one this fixture was built \
                     for — the fixture, not the panel, is wrong"
                );
                let want_range = if want_layer == "sprite" {
                    tile_range(
                        sprite_tile_at(&v.sprites_decoded()[0], 70, 70).expect("inside the sprite"),
                    )
                } else {
                    want_range
                };
                assert_eq!(
                    (p.targets[0].lo, p.targets[0].hi),
                    want_range,
                    "hiding {hidden:?}: the bus says {want_layer} and the panel armed \
                     ${:04X}-${:04X} — the two have DRIFTED",
                    p.targets[0].lo,
                    p.targets[0].hi
                );
                // And the words, because the range alone cannot tell plane A's tile from a sprite that
                // happened to draw from it. The panel names the layer in prose; the bus names it in an
                // enum; a disagreement here is the "purple boxes" failure with a correct address under it.
                let spoken = match want_layer {
                    "sprite" => "sprite 0",
                    "planeA" => "plane A",
                    "planeB" => "plane B",
                    _ => "backdrop",
                };
                assert!(
                    p.headline.contains(spoken),
                    "hiding {hidden:?}: the bus says {want_layer}, the panel's sentence says {:?}",
                    p.headline
                );
            }
        }

        /// The mask **is named in the answer**, and only when there is one — the loud-on-unmeasurable half.
        ///
        /// Describing a masked picture without saying so is a true sentence a reader cannot help but take
        /// as false, and it is the reason this parcel exists. The negative half matters as much: an
        /// unmasked answer must be byte-identical to the one this panel has always given, or every reader
        /// learns to skip the clause.
        #[test]
        fn the_answer_says_a_layer_is_hidden_and_says_it_only_then() {
            let (v, _, _) = vdp_with_four_answers();

            let plain = resolve(&v, 70, 70, LayerMask::ALL);
            assert!(
                !plain.headline.contains("hidden"),
                "an unmasked answer must not mention a mask: {:?}",
                plain.headline
            );
            assert!(
                !plain.description.contains("hidden"),
                "{:?}",
                plain.description
            );

            let mut e = engine_showing(&v);
            hide(&mut e, &["sprites", "planeA"]);
            let masked = resolve(&v, 70, 70, e.layers());
            // Every hidden layer, by the wire's own name — not "a mask is set", which sends the reader
            // hunting for which one.
            for name in ["sprites", "planeA"] {
                assert!(
                    masked.headline.contains(name),
                    "the sentence must name {name}: {:?}",
                    masked.headline
                );
            }
            assert!(
                masked.headline.contains("masked picture"),
                "and must say what that means for what is on screen: {:?}",
                masked.headline
            );
            // The clause rides on the human-facing line, not only on a wire caveat: the consumer's point 3
            // is explicit that the human-facing line carries it.
            assert!(
                masked.description.starts_with(&masked.headline),
                "the description leads with the sentence: {:?}",
                masked.description
            );
        }

        /// **The panel never names a slot in another tool's space, and says which space its own index is
        /// in.** The editor rebases our index into a blob-local slot with a base constant *they* own; an
        /// index whose space is unstated is a transpose bug waiting to happen, and a rebase this panel
        /// performed could land outside their artwork while passing a naive capacity check — a confident
        /// wrong slot, indistinguishable from a right one. *In-capacity is not in-blob.*
        ///
        /// So: every answer that names a tile says `VRAM-absolute`, and no answer uses the vocabulary of
        /// somebody else's model.
        #[test]
        fn a_named_tile_states_its_space_and_never_another_models_slot() {
            let (v, _, _) = vdp_with_four_answers();
            let mut e = engine_showing(&v);
            let mut seen_a_tile = false;
            for hidden in [&[][..], &["sprites"][..], &["sprites", "planeA"][..]] {
                let mut e2 = engine_showing(&v);
                hide(&mut e2, hidden);
                let p = resolve(&v, 70, 70, e2.layers());
                assert!(
                    p.headline.contains("tile $"),
                    "this fixture's first three steps all name a tile: {:?}",
                    p.headline
                );
                seen_a_tile = true;
                assert!(
                    p.headline.contains(TILE_SPACE),
                    "a named tile must state its space: {:?}",
                    p.headline
                );
                for forbidden in ["slot", "blob", "blob-local"] {
                    assert!(
                        !p.headline.to_ascii_lowercase().contains(forbidden),
                        "the panel must not speak in another model's terms ({forbidden}): {:?}",
                        p.headline
                    );
                }
            }
            assert!(
                seen_a_tile,
                "COULD NOT MEASURE: no step of this fixture named a tile, so the rule was never \
                 exercised — the fixture is broken, not the rule"
            );
            // The backdrop names no tile, and must therefore claim no space either.
            hide(&mut e, &["sprites", "planeA", "planeB"]);
            let p = resolve(&v, 70, 70, e.layers());
            assert!(!p.headline.contains("tile $"), "{:?}", p.headline);
            assert!(!p.headline.contains(TILE_SPACE), "{:?}", p.headline);
        }
    }
}
