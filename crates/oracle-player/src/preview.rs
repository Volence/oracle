//! **What the thing you are about to place looks like** — one picture per archetype, measured off the
//! running game rather than decoded out of anybody's mapping format.
//!
//! The owner asked for this three times in one breath, and the third sentence is the one that shrinks the
//! other two:
//!
//! > *"tbh having a preview of what it looks like would be great (the object)"*
//! >
//! > *"oh also can placement mode preview the item too?"*
//! >
//! > *"I mean we don't have to actually draw it in the game, like where my mouse is when placing it have
//! > roughly the size and shape of sprite as a preview from the shell of oracle no?"*
//! >
//! > *"like I mean if we get a preview of it in our list of items we can just use that preview image no?"*
//!
//! So: **one image per archetype**, drawn by this window in two places, the picker's list and a ghost under
//! the cursor in spawn mode. The image is the same image; this module produces it and decides nothing about
//! where it goes.
//!
//! # ⚑ Where the picture comes from, and the one thing this crate refuses to do to get it
//!
//! There is no sprite-mapping decoder anywhere in this workspace and there must not be one. Reading aeon's
//! mapping format would be this crate asserting a fact about somebody else's game, which is the thing
//! `oracle-frontend`'s `spawn.rs` refuses in its own words one line above `ARCHETYPE_PREFIX`: *"A
//! hard-coded `ObjDef_Ring` would be this crate asserting a fact about somebody else's game."* A mapping
//! reader is that same assertion with more code in it.
//!
//! `Vdp::sprites_decoded` reads the **sprite attribute table**, which holds only the sprites the game has
//! already placed. So the measurement is: **put one of the object into a running machine, let it place its
//! own sprites, and take the difference.** What appeared is the object's, exactly, with no game knowledge
//! whatsoever, and both of the things a ghost needs fall out of it for free:
//!
//! * the **footprint** is just the rectangles the entries describe, and
//! * the **anchor** is the offset from the dot the object was asked for to those rectangles, because the
//!   window chose that dot and therefore knows it.
//!
//! # ⚑ The difference needs a CONTROL, and the naive version of this is confidently wrong
//!
//! "Snapshot the table, spawn, snapshot again, diff" does not work, and it fails in the direction that
//! looks like success. The spawn's own mailbox handshake advances one or two frames
//! (`OBJREQ_DEFAULT_MAX_FRAMES`), and in those frames **the whole game moves**: every other object's
//! sprites change position, and inserting one object shifts every entry after it into a different slot. A
//! slot-by-slot diff of before against after returns most of the screen.
//!
//! So the measurement runs the machine **twice from one checkpoint**, for the identical number of frames:
//!
//! 1. checkpoint the machine;
//! 2. spawn the archetype and run [`EXTRA_FRAMES`] more, so the object gets a display pass; note the
//!    server's own `framesAdvanced`, call it `f`;
//! 3. read the live sprite list, restore the checkpoint;
//! 4. run `f + EXTRA_FRAMES` frames with **no spawn** — the control;
//! 5. read the live sprite list, restore the checkpoint, drop it.
//!
//! The two lists then differ by exactly one object. [`appeared`] takes the difference as a **multiset of
//! sprite shapes with the slot and the link left out**, so an entry that merely moved slots cancels, and it
//! reports a perturbation rather than a picture when the control holds a sprite the probe does not: that
//! means the extra object pushed something off the table, and a picture taken then would be missing part
//! of itself while looking complete.
//!
//! The lists are the **link walk**, not all 80 slots ([`walk`]). Slots past the walk hold whatever the last
//! game that used them left behind; they are not drawn, and they do not cancel, because the probe's longer
//! list overwrites leftovers the control still has.
//!
//! # ⚑ Residency is TWO conditions and only one of them is detectable
//!
//! 1. **The tiles are unwritten.** The object's sprites name tile indices that are blank in video memory,
//!    which happens whenever the act running has not loaded that object's art. This is a **measurement**:
//!    a Mega Drive sprite pixel of colour 0 is transparent, so "every tile these sprites name is blank"
//!    and "the assembled picture has no visible pixel in it" are the same statement, and [`compose`]
//!    tests the second one directly rather than guessing from a tile index. The answer is
//!    [`Art::NotResident`], and what is shown instead is the **silhouette**, which is still measured: the
//!    rectangles are real and only the colours are missing.
//!
//! 2. **The tiles hold somebody else's art.** Video memory is not cleared between acts or between load
//!    cues, so an object whose art is absent can name tiles another object's art is sitting in, and the
//!    grab comes back a real, plausible, **confidently wrong** picture. This is **not detectable** from
//!    here without exactly the game knowledge this module refuses to acquire.
//!
//! So condition 1 is detected and falls back with its reason stated, and for everything else the picture is
//! shown **labelled as a capture from the running game** rather than asserted as what the object looks like
//! ([`Preview::note`]). That labelling is not ceremony. It is the difference between the panel claiming a
//! fact and the panel showing a measurement, and a person looking at a wrong picture usually knows it in a
//! way nothing here can.
//!
//! # ⚑ The cache key, and why it cannot serve a stale picture after an act change
//!
//! Keying on the archetype alone is the trap: the same object previews correctly in one act and falls back
//! in another, and a picture kept under the name alone comes back after an act change looking **exactly
//! like the feature working**. So a preview carries the tiles it was drawn from and a fingerprint of the
//! bytes behind them ([`fingerprint`]: the whole colour table, then every one of those tiles' 64 pixels),
//! and [`Preview::still_current`] retakes that fingerprint against the machine as it is now.
//!
//! An act change rewrites both halves: the palette is reloaded and the art region is decompressed over. So
//! the fingerprint moves, the picture is dropped, and the panel says so rather than drawing it.
//! **The residual, stated rather than papered over:** a change that leaves the whole colour table *and*
//! every one of those tiles byte-identical while moving the object's tile indices would not be caught. The
//! picture would still be a true picture of those tiles; it would be the wrong tiles. Nothing short of a
//! mapping reader can tell the difference, and this module does not have one.
//!
//! [`Key::subtype`] exists and is always `None`. Subtypes do not exist in this window yet
//! (`SPAWN-PICKER-SUBTYPE` is blocked on another lane), and the field is here so that dimension slots into
//! the key without a rewrite. Nothing constructs it as anything but `None`, and no concept of a subtype is
//! invented here.
//!
//! # ⚑ Nothing here holds an egui type
//!
//! `docs/2026-09-05-debug-window-audit.md` §3's first lesson, from the Pacing exemplar: this window cannot
//! be opened from an agent seat, so a panel whose correctness lives in its draw calls is a panel nothing
//! can check. Everything that decides what the reader sees is here, testable against values a test chooses;
//! `crate::ui` uploads a texture and lays it out.

use oracle_core::render::SpriteDecoded;
use oracle_core::state_hash::fnv1a_bytes;
use oracle_core::vdp::Vdp;

/// How many frames past the spawn's own handshake the probe runs before reading the sprite table.
///
/// One. The mailbox hands back a placed object, and the object writes its sprites on its next display
/// pass, so one frame is what stands between "the object exists" and "the object has told the video chip
/// about itself". The control runs the identical total, so this number's only cost is emulated time.
pub const EXTRA_FRAMES: u64 = 1;

/// A ceiling on a preview's own size, in screen dots per axis.
///
/// A Mega Drive sprite is at most 32 dots on a side and an object is a handful of them, so a footprint
/// wider than this did not come from one object: it came from a difference that swept up something else,
/// and drawing it would put a screen-sized ghost under the cursor. Refused with a sentence rather than
/// clamped, because a clamped footprint is a wrong anchor drawn confidently.
pub const MAX_DOTS: u32 = 256;

// --------------------------------------------------------------------------------------------------
// The pieces of a picture
// --------------------------------------------------------------------------------------------------

/// One sprite's rectangle, in dots from the footprint's top left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// The picture itself: `w * h` samples in row-major order, `None` where the sprite is transparent.
///
/// Transparency is carried rather than flattened to a background colour, because the ghost is drawn over
/// the game and a preview with an opaque box around it is a preview of a box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shot {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Option<(u8, u8, u8)>>,
}

impl Shot {
    /// Whether any pixel in this picture is visible. `false` is condition 1 above, measured.
    pub fn has_ink(&self) -> bool {
        self.px.iter().any(Option::is_some)
    }
}

/// What was recovered for this archetype: a real picture, or the shape without it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Art {
    /// The tiles these sprites name hold art, and this is it, read out of this machine's own video memory
    /// and colour table.
    Captured(Shot),
    /// Every tile these sprites name is blank, so there is no picture to take and the silhouette stands in
    /// for it. Measured, not guessed: see the module header's condition 1.
    NotResident,
}

/// What a preview is cached under.
///
/// `subtype` is always `None` today and is here so a subtype dimension slots in without a rewrite. See the
/// module header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub archetype: String,
    pub subtype: Option<u32>,
}

/// **One archetype's preview, and the evidence for it.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// What this is a preview of.
    pub key: Key,
    /// The footprint, in screen dots.
    pub w: u32,
    pub h: u32,
    /// Where the object's own position sits inside the picture, in dots from its top left. This is what a
    /// ghost hangs on: put this point under the pointer and the picture sits where the object would.
    pub anchor: (u32, u32),
    /// One rectangle per sprite the object placed, for the silhouette.
    pub cells: Vec<Cell>,
    /// The picture, or the stated absence of one.
    pub art: Art,
    /// The tiles the picture was drawn from, so [`still_current`](Preview::still_current) can retake the
    /// fingerprint against the same bytes.
    pub tiles: Vec<u16>,
    /// [`fingerprint`] over those tiles and the colour table, at the instant the picture was taken.
    pub art_print: u64,
}

impl Preview {
    /// Whether the art this picture was drawn from is still the art in the machine.
    ///
    /// Retakes [`fingerprint`] over the same tiles and compares. See the module header for what this
    /// catches (an act change moves both halves of it) and the one residual it does not.
    pub fn still_current(&self, vdp: &Vdp) -> bool {
        fingerprint(vdp, &self.tiles) == self.art_print
    }

    /// Whether this preview is of that archetype and that subtype.
    pub fn is_of(&self, archetype: &str, subtype: Option<u32>) -> bool {
        self.key.archetype == archetype && self.key.subtype == subtype
    }

    /// How many sprites the object placed.
    pub fn sprites(&self) -> usize {
        self.cells.len()
    }

    /// **The line that goes under the picture**, and it is the difference between claiming a fact and
    /// showing a measurement.
    ///
    /// Both arms say where the picture came from, because neither is an assertion about the object: one is
    /// a capture off a running machine, and the other is a shape with the colours honestly missing.
    pub fn note(&self) -> String {
        match &self.art {
            Art::Captured(_) => format!(
                "Captured from the running game: one {} was placed, the {} it drew were read back, \
                 and the machine was put where it was. It is what this act drew, not a claim about \
                 what this object looks like.",
                self.key.archetype,
                plural(self.sprites(), "sprite", "sprites"),
            ),
            Art::NotResident => format!(
                "The art this object asks for is not in video memory in the act running now, so there \
                 is no picture to take. The outline is the size and shape its {} asked for, which is \
                 measured; the colours are the part that is missing.",
                plural(self.sprites(), "sprite", "sprites"),
            ),
        }
    }

    /// The size, as one small line beside the picture. Dots, because that is the unit a person placing an
    /// object is looking at.
    pub fn size_line(&self) -> String {
        format!(
            "{} by {} dots, {}",
            self.w,
            self.h,
            plural(self.sprites(), "sprite", "sprites")
        )
    }
}

/// **What the window has for the archetype a click would place.**
///
/// The `Absent` arm is a sentence and never an empty frame: every way this measurement can fail is a
/// different thing to tell somebody, and "no preview" with no reason is the P6 defect exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A picture, or a silhouette, and the evidence for it.
    Ready(Box<Preview>),
    /// No picture, and why. Carries the server's own words when the reason was a refusal.
    Absent(String),
    /// A picture was taken and the art under it has since been replaced, so it is not shown.
    ///
    /// Separate from [`Outcome::Absent`] because it is the one case with a remedy a person can act on, and
    /// because a stale picture drawn anyway is the failure the whole cache key exists to prevent.
    Stale(Box<Preview>),
}

impl Outcome {
    /// The preview to draw, or `None`. A stale one is deliberately **not** drawable.
    pub fn drawable(&self) -> Option<&Preview> {
        match self {
            Self::Ready(p) => Some(p),
            Self::Absent(_) | Self::Stale(_) => None,
        }
    }

    /// **The line the panel draws**, in every arm. There is no arm that says nothing.
    pub fn sentence(&self) -> String {
        match self {
            Self::Ready(p) => p.note(),
            Self::Absent(why) => why.clone(),
            Self::Stale(p) => format!(
                "The art behind this picture of {} has been replaced since it was taken, most likely \
                 by an act loading, so it is not shown: it would be a real picture of the wrong tiles. \
                 Take it again to see this act's.",
                p.key.archetype
            ),
        }
    }
}

// --------------------------------------------------------------------------------------------------
// The measurement's arithmetic
// --------------------------------------------------------------------------------------------------

/// **The sprites the video chip would actually walk**, in link order, from a decoded table.
///
/// Slots past the walk are not drawn: they hold whatever was last written there, and they are the reason a
/// diff over all 80 entries reports a perturbation that never happened. The probe's list is longer than the
/// control's, so it overwrites leftovers the control still carries, and those leftovers then look like
/// sprites that vanished.
///
/// `max` is the chip's own parse cap for the current width ([`Vdp::parsed_sprite_max`]), so an H32 machine
/// walks 64 and an H40 machine walks 80 rather than a constant either would be wrong for. The walk ends on
/// a zero link, on an out-of-range link, on the cap, or on a slot it has already visited, which is the
/// guard against a link loop written by a game mid-frame.
///
/// Written here rather than borrowed from `oracle_core::render`, whose walk is private, per-line, and
/// carries the evaluation outcome instead of the attribute half a picture needs. The two are held together
/// by `the_walk_ends_the_way_the_chip_ends_it`, which pins the four terminations against the same rules the
/// core's walk states.
pub fn walk(sprites: &[SpriteDecoded], max: u8) -> Vec<SpriteDecoded> {
    let mut seen = [false; 80];
    let mut out = Vec::new();
    let mut idx = 0usize;
    for _ in 0..usize::from(max).min(sprites.len()) {
        let Some(s) = sprites.get(idx) else { break };
        if seen[idx] {
            break;
        }
        seen[idx] = true;
        out.push(*s);
        if s.link == 0 {
            break;
        }
        idx = usize::from(s.link);
    }
    out
}

/// The part of a sprite entry that is **what it looks like and where**, with the slot and the link left
/// out.
///
/// Both of those move when an object is inserted into the list, and neither changes what is on the glass.
/// A diff keyed on the whole entry would report the entire tail of the list as new.
fn shape(s: &SpriteDecoded) -> (i16, i16, u8, u8, u16, u8, bool, bool, bool) {
    (
        s.x,
        s.y,
        s.width_cells,
        s.height_cells,
        s.tile,
        s.palette,
        s.hflip,
        s.vflip,
        s.priority,
    )
}

/// **The sprites the probe has and the control does not** — the object, isolated.
///
/// A multiset difference, so an entry that merely moved slots cancels against itself.
///
/// `Err` is a **perturbed measurement**: the control holds a sprite shape the probe does not, which means
/// the extra object displaced something rather than simply joining it. The likely cause is the sprite table
/// filling up. A picture taken from a displaced measurement can be missing a limb while looking whole,
/// which is the class of wrong answer this whole module is written against, so it is refused and named
/// instead of drawn.
pub fn appeared(
    control: &[SpriteDecoded],
    probe: &[SpriteDecoded],
) -> Result<Vec<SpriteDecoded>, String> {
    use std::collections::BTreeMap;
    let mut left: BTreeMap<_, i32> = BTreeMap::new();
    for s in control {
        *left.entry(shape(s)).or_insert(0) += 1;
    }
    let mut new = Vec::new();
    for s in probe {
        let k = shape(s);
        match left.get_mut(&k) {
            Some(n) if *n > 0 => *n -= 1,
            _ => new.push(*s),
        }
    }
    if left.values().any(|n| *n > 0) {
        let lost: i32 = left.values().filter(|n| **n > 0).sum();
        return Err(format!(
            "placing this object took {} off the sprite table instead of simply adding to it, so \
             what is left is not a whole picture of it. This usually means the table was already \
             full. Nothing was drawn rather than drawing a piece of the object as though it were \
             all of it.",
            plural(lost as usize, "sprite", "sprites")
        ));
    }
    Ok(new)
}

/// **The fingerprint of the art a picture was drawn from**: the whole colour table, then every one of
/// those tiles' 64 pixels, folded with the same FNV-1a the state hash uses.
///
/// The colour table is in it because a palette reload changes the picture without touching a tile, and an
/// act change always does both. The tiles are in it because that is what the pixels came from. Both halves
/// are read live, so this is a measurement of the machine now and never a remembered number.
///
/// Order is fixed and the tiles are taken in the order the preview recorded them, so the same art always
/// gives the same number.
pub fn fingerprint(vdp: &Vdp, tiles: &[u16]) -> u64 {
    let mut bytes: Vec<u8> = Vec::with_capacity(vdp.cram().len() + tiles.len() * 64);
    bytes.extend_from_slice(vdp.cram());
    for &t in tiles {
        bytes.extend_from_slice(&vdp.tile_pixels(usize::from(t)));
    }
    fnv1a_bytes(&bytes)
}

/// The tile a sprite's own pixel `(lx, ly)` comes from, in the sprite's own coordinates.
///
/// The same addressing `oracle_core::render::sprite_tile_at` states, in local coordinates rather than
/// screen ones, because a preview's sprites can sit at a negative screen X and that function's `u16`
/// arguments cannot express one. Flips mirror the whole sprite rather than each cell, and a multi cell
/// sprite's patterns run down a column before moving right, so the offset from the base tile is
/// `(col * height_cells) + row`. The addition wraps, because a base tile near the top of video memory with
/// a large sprite genuinely does wrap there.
///
/// `the_local_tile_walk_agrees_with_the_cores` holds this to the core's version wherever both can be asked.
fn tile_at_local(s: &SpriteDecoded, lx: u32, ly: u32) -> (u16, usize) {
    let wpx = u32::from(s.width_cells) * 8;
    let hpx = u32::from(s.height_cells) * 8;
    let sx = if s.hflip { wpx - 1 - lx } else { lx };
    let sy = if s.vflip { hpx - 1 - ly } else { ly };
    let offset = (sx / 8) * u32::from(s.height_cells) + sy / 8;
    let within = (sy % 8) * 8 + (sx % 8);
    (s.tile.wrapping_add(offset as u16), within as usize)
}

/// **Assemble the preview** from the entries the difference isolated.
///
/// `dot` is the screen dot the object was asked for, which is what makes the anchor free: the entries are
/// in screen dots too, so the offset from `dot` to the footprint's top left is exactly where the object's
/// own position sits inside the picture.
///
/// Sprites are composited in **reverse link order**, so the sprite the chip would draw first ends up on
/// top: on this hardware the earlier sprite in the walk wins a contested dot, and painting the list
/// backwards with transparent pixels skipped is that rule.
///
/// The `Err` arms are the two ways this cannot produce a picture at all, and each is a sentence.
pub fn compose(
    entries: &[SpriteDecoded],
    dot: (u16, u16),
    vdp: &Vdp,
    key: Key,
) -> Result<Preview, String> {
    if entries.is_empty() {
        return Err(format!(
            "placing one {} added nothing to the sprite table, so there is nothing to show. The \
             object may have been removed in the frame it was placed in, or it may draw nothing at \
             all.",
            key.archetype
        ));
    }
    let min_x = entries.iter().map(|s| i32::from(s.x)).min().unwrap_or(0);
    let min_y = entries.iter().map(|s| i32::from(s.y)).min().unwrap_or(0);
    let max_x = entries
        .iter()
        .map(|s| i32::from(s.x) + i32::from(s.width_cells) * 8)
        .max()
        .unwrap_or(0);
    let max_y = entries
        .iter()
        .map(|s| i32::from(s.y) + i32::from(s.height_cells) * 8)
        .max()
        .unwrap_or(0);
    let w = (max_x - min_x).max(0) as u32;
    let h = (max_y - min_y).max(0) as u32;
    if w == 0 || h == 0 || w > MAX_DOTS || h > MAX_DOTS {
        return Err(format!(
            "the sprites that appeared when one {} was placed cover {w} by {h} dots, which is not one \
             object's worth. Something else on screen changed in the same frames, so nothing was \
             drawn rather than drawing a ghost the size of the picture.",
            key.archetype
        ));
    }

    let cells: Vec<Cell> = entries
        .iter()
        .map(|s| Cell {
            x: (i32::from(s.x) - min_x) as u32,
            y: (i32::from(s.y) - min_y) as u32,
            w: u32::from(s.width_cells) * 8,
            h: u32::from(s.height_cells) * 8,
        })
        .collect();

    // The tiles, in a fixed order and each named once, so the fingerprint over them is stable.
    let mut tiles: Vec<u16> = Vec::new();
    for s in entries {
        let n = u16::from(s.width_cells) * u16::from(s.height_cells);
        for i in 0..n {
            let t = s.tile.wrapping_add(i);
            if !tiles.contains(&t) {
                tiles.push(t);
            }
        }
    }

    let cram = vdp.cram_decoded();
    let mut px: Vec<Option<(u8, u8, u8)>> = vec![None; (w as usize) * (h as usize)];
    for (s, c) in entries.iter().zip(cells.iter()).rev() {
        let pal = usize::from(s.palette & 0x03) * 16;
        for ly in 0..c.h {
            for lx in 0..c.w {
                let (tile, within) = tile_at_local(s, lx, ly);
                let nibble = vdp.tile_pixels(usize::from(tile))[within];
                if nibble == 0 {
                    continue; // colour 0 is transparent on a sprite, and that is the whole rule
                }
                let dst = ((c.y + ly) as usize) * (w as usize) + (c.x + lx) as usize;
                px[dst] = Some(cram[pal + usize::from(nibble & 0x0F)]);
            }
        }
    }

    let shot = Shot {
        w: w as usize,
        h: h as usize,
        px,
    };
    // ⚑ **Condition 1, measured on the assembled picture rather than guessed from a tile index.** A sprite
    // pixel of colour 0 is transparent, so "every tile these sprites name is blank" and "nothing in this
    // picture is visible" are the same statement, and this is the second one asked directly.
    let art = if shot.has_ink() {
        Art::Captured(shot)
    } else {
        Art::NotResident
    };

    // The anchor: the window chose `dot`, and the entries are in the same screen dots, so this subtraction
    // is the whole derivation. Saturating rather than wrapping because a sprite drawn above or left of the
    // dot puts the origin outside its own footprint, and clamping to the edge is the honest ghost.
    let anchor = (
        (i32::from(dot.0) - min_x).clamp(0, w as i32) as u32,
        (i32::from(dot.1) - min_y).clamp(0, h as i32) as u32,
    );

    let art_print = fingerprint(vdp, &tiles);
    Ok(Preview {
        key,
        w,
        h,
        anchor,
        cells,
        art,
        tiles,
        art_print,
    })
}

/// `1 sprite` against `2 sprites`, in one place, because a count that says `1 sprites` reads as a bug in
/// the number rather than in the sentence.
fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_core::render::sprite_tile_at;

    fn sprite(index: u8, x: i16, y: i16, w: u8, h: u8, tile: u16, link: u8) -> SpriteDecoded {
        SpriteDecoded {
            index,
            x,
            y,
            width_cells: w,
            height_cells: h,
            link,
            tile,
            palette: 0,
            hflip: false,
            vflip: false,
            priority: false,
            cache_divergence: false,
        }
    }

    /// Colour 1 of palette line 0, as a value nothing else in a blank machine can produce.
    const INK: u16 = 0x0EEE;

    /// A machine whose tiles 1 to 4 are solid colour 1 and whose video memory is otherwise blank, so
    /// "this tile has art" and "this tile does not" are both reachable and neither is a coincidence.
    ///
    /// Built through the chip's own ports and pokes rather than by assembling a `Vdp` literal, which is
    /// the idiom `screen_pick.rs`'s own rig uses one file over.
    fn art_machine() -> oracle_core::system::System {
        let mut sys = oracle_core::system::System::new(0x5EED);
        let v = sys.vdp_mut();
        v.vram_mut().fill(0);
        for t in 1..=4usize {
            for b in 0..32 {
                v.vram_mut()[t * 32 + b] = 0x11;
            }
        }
        v.poke_cram(1, INK, 0);
        sys
    }

    // ------------------------------------------------------------------------------------------
    // The walk
    // ------------------------------------------------------------------------------------------

    /// **The walk ends the way the chip ends it**, in all four ways, and the fourth is the one a diff
    /// depends on: a link loop must terminate rather than spin.
    #[test]
    fn the_walk_ends_the_way_the_chip_ends_it() {
        // A zero link ends the list.
        let t = vec![
            sprite(0, 0, 0, 1, 1, 0, 1),
            sprite(1, 8, 0, 1, 1, 0, 0),
            sprite(2, 16, 0, 1, 1, 0, 0),
        ];
        assert_eq!(
            walk(&t, 80).iter().map(|s| s.index).collect::<Vec<_>>(),
            [0, 1],
            "the walk stops at the zero link and never reaches slot 2"
        );

        // The cap ends it: every link points at the next slot and none is zero.
        let chain: Vec<SpriteDecoded> = (0..8)
            .map(|i| sprite(i, 0, 0, 1, 1, 0, (i + 1) % 8))
            .collect();
        assert_eq!(walk(&chain, 4).len(), 4, "the parse cap bounds the walk");

        // An out-of-range link ends it.
        let oor = vec![sprite(0, 0, 0, 1, 1, 0, 60), sprite(1, 0, 0, 1, 1, 0, 0)];
        assert_eq!(
            walk(&oor, 80).len(),
            1,
            "a link past the table ends the list"
        );

        // A loop ends it, and this is the one that would hang without the visited set.
        let loopy = vec![sprite(0, 0, 0, 1, 1, 0, 1), sprite(1, 0, 0, 1, 1, 0, 1)];
        assert_eq!(walk(&loopy, 80).len(), 2, "a self link terminates the walk");
    }

    /// **The cap is the chip's, so an H32 machine walks fewer slots than an H40 one.**
    #[test]
    fn the_cap_is_the_machines_and_not_a_constant() {
        let chain: Vec<SpriteDecoded> = (0..80)
            .map(|i| sprite(i, 0, 0, 1, 1, 0, ((i as u16 + 1) % 80) as u8))
            .collect();
        assert_eq!(walk(&chain, 64).len(), 64, "H32 parses 64");
        assert_eq!(walk(&chain, 80).len(), 80, "H40 parses 80");
    }

    // ------------------------------------------------------------------------------------------
    // The difference
    // ------------------------------------------------------------------------------------------

    /// **An entry that only moved slots cancels**, which is the whole reason the diff is on shapes.
    ///
    /// This is the failure the naive version has: inserting one object shifts every entry after it into a
    /// different slot, and a slot-by-slot diff calls all of them new.
    #[test]
    fn a_sprite_that_only_changed_slots_is_not_a_new_sprite() {
        let control = vec![
            sprite(0, 10, 10, 1, 1, 0x100, 1),
            sprite(1, 20, 20, 2, 2, 0x200, 0),
        ];
        // The same two shapes, at different slots and with different links, plus one genuinely new one.
        let probe = vec![
            sprite(0, 40, 40, 1, 1, 0x300, 5),
            sprite(5, 20, 20, 2, 2, 0x200, 7),
            sprite(7, 10, 10, 1, 1, 0x100, 0),
        ];
        let new = appeared(&control, &probe).expect("nothing was displaced");
        assert_eq!(new.len(), 1, "exactly one sprite is new: {new:?}");
        assert_eq!(new[0].tile, 0x300);
        assert_eq!(new[0].x, 40);
    }

    /// **A displaced measurement is refused, not drawn.**
    ///
    /// The dangerous case: the extra object pushed one off the table, so the picture that is left could be
    /// missing part of itself while looking complete.
    #[test]
    fn a_control_sprite_the_probe_lost_is_a_refusal_with_its_reason() {
        let control = vec![
            sprite(0, 10, 10, 1, 1, 0x100, 1),
            sprite(1, 20, 20, 1, 1, 0x200, 0),
        ];
        let probe = vec![sprite(0, 10, 10, 1, 1, 0x100, 0)];
        let why = appeared(&control, &probe).expect_err("a lost sprite must refuse");
        assert!(
            why.contains("1 sprite ") && !why.contains("1 sprites"),
            "the count must read as English: {why:?}"
        );
        assert!(
            why.contains("sprite table"),
            "the reason must name what happened: {why:?}"
        );
    }

    /// **Two identical sprites are two, not one.** A set would collapse them and lose half the object.
    #[test]
    fn identical_sprites_are_counted_rather_than_deduplicated() {
        let s = sprite(0, 10, 10, 1, 1, 0x100, 0);
        let control = vec![s];
        let probe = vec![s, s, s];
        assert_eq!(
            appeared(&control, &probe).unwrap().len(),
            2,
            "three minus one is two, not zero"
        );
    }

    // ------------------------------------------------------------------------------------------
    // The picture
    // ------------------------------------------------------------------------------------------

    /// **The local tile walk agrees with the core's**, so there are not two derivations of the same
    /// addressing. Swept over both flips and every dot of a 2 by 2 sprite.
    #[test]
    fn the_local_tile_walk_agrees_with_the_cores() {
        for hflip in [false, true] {
            for vflip in [false, true] {
                let mut s = sprite(0, 64, 64, 2, 2, 0x40, 0);
                s.hflip = hflip;
                s.vflip = vflip;
                for ly in 0..16u32 {
                    for lx in 0..16u32 {
                        let (mine, _) = tile_at_local(&s, lx, ly);
                        let theirs =
                            sprite_tile_at(&s, (s.x as u32 + lx) as u16, (s.y as u32 + ly) as u16)
                                .expect("inside the sprite");
                        assert_eq!(
                            mine, theirs,
                            "hflip={hflip} vflip={vflip} at ({lx},{ly}): the two walks disagree"
                        );
                    }
                }
            }
        }
    }

    /// **The footprint and the anchor both fall out of the entries, and the anchor is the spawn dot.**
    #[test]
    fn the_footprint_is_the_rectangles_and_the_anchor_is_the_dot_it_was_asked_for() {
        let sys = art_machine();
        let v = sys.vdp();
        // Two sprites: one 8 by 8 at (100, 100), one 8 by 8 at (108, 100). Footprint 16 by 8.
        let entries = vec![
            sprite(0, 100, 100, 1, 1, 1, 1),
            sprite(1, 108, 100, 1, 1, 2, 0),
        ];
        let p = compose(
            &entries,
            (104, 104),
            v,
            Key {
                archetype: "ObjDef_Test".into(),
                subtype: None,
            },
        )
        .expect("two on screen sprites are a picture");
        assert_eq!(
            (p.w, p.h),
            (16, 8),
            "the box is the union of the rectangles"
        );
        assert_eq!(
            p.anchor,
            (4, 4),
            "the origin sits where the dot was, measured from the box's top left"
        );
        assert_eq!(p.cells.len(), 2);
        assert_eq!(
            p.cells[0],
            Cell {
                x: 0,
                y: 0,
                w: 8,
                h: 8
            }
        );
        assert_eq!(
            p.cells[1],
            Cell {
                x: 8,
                y: 0,
                w: 8,
                h: 8
            }
        );
    }

    /// **Art in video memory is a captured picture; art that is not there is the silhouette, and the
    /// difference is measured rather than assumed.**
    ///
    /// This is condition 1 in the module header, and it is the one of the two residency conditions that
    /// can be detected at all.
    #[test]
    fn blank_tiles_fall_back_to_the_silhouette_and_written_tiles_do_not() {
        let sys = art_machine();
        let v = sys.vdp();
        let key = Key {
            archetype: "ObjDef_Test".into(),
            subtype: None,
        };
        // Tile 1 is written.
        let drawn = compose(&[sprite(0, 50, 50, 1, 1, 1, 0)], (50, 50), v, key.clone()).unwrap();
        match &drawn.art {
            Art::Captured(s) => {
                assert!(s.has_ink(), "a written tile must produce visible pixels");
                assert_eq!(
                    s.px[0],
                    Some(v.cram_decoded()[1]),
                    "the colour must be the machine's own entry 1, not a value this test invented"
                );
            }
            Art::NotResident => panic!("tile 1 is written and must not read as absent art"),
        }
        // Tile 0x300 is blank, and the whole point is that this is not guessed from the index.
        let blank = compose(&[sprite(0, 50, 50, 1, 1, 0x300, 0)], (50, 50), v, key).unwrap();
        assert_eq!(
            blank.art,
            Art::NotResident,
            "an all zero tile is a transparent picture, which is no picture"
        );
        assert_eq!(
            blank.cells.len(),
            1,
            "the silhouette is still measured, so the rectangle survives"
        );
    }

    /// **An empty difference is a sentence and never an empty frame** (P6), and so is a footprint that is
    /// obviously not one object's.
    #[test]
    fn the_two_ways_a_picture_cannot_be_taken_are_both_sentences() {
        let sys = art_machine();
        let v = sys.vdp();
        let key = Key {
            archetype: "ObjDef_Test".into(),
            subtype: None,
        };
        let none = compose(&[], (0, 0), v, key.clone()).expect_err("nothing appeared");
        assert!(
            none.contains("ObjDef_Test") && none.len() > 40,
            "the empty case must name the archetype and say what it means: {none:?}"
        );
        let huge = vec![sprite(0, 0, 0, 1, 1, 1, 1), sprite(1, 300, 0, 1, 1, 1, 0)];
        let big = compose(&huge, (0, 0), v, key).expect_err("308 dots is not one object");
        assert!(
            big.contains("308"),
            "the refusal must carry the measurement that caused it: {big:?}"
        );
    }

    // ------------------------------------------------------------------------------------------
    // The cache key
    // ------------------------------------------------------------------------------------------

    /// **⚑ The picture goes stale when the art under it moves, and that is a measurement of the machine
    /// now.**
    ///
    /// Both halves are swept, because an act change moves both and either alone must be enough: a palette
    /// reload with the tiles untouched, and a tile rewrite with the palette untouched. Keyed on the
    /// archetype alone, both of these would serve the old picture, which is the failure that looks exactly
    /// like the feature working.
    #[test]
    fn the_fingerprint_moves_when_either_the_palette_or_the_art_moves() {
        let mut sys = art_machine();
        let key = Key {
            archetype: "ObjDef_Test".into(),
            subtype: None,
        };
        let p = compose(&[sprite(0, 50, 50, 1, 1, 1, 0)], (50, 50), sys.vdp(), key).unwrap();
        assert!(
            p.still_current(sys.vdp()),
            "the control: nothing has changed yet"
        );

        // Half one: the palette is reloaded and not one tile is touched.
        sys.vdp_mut().poke_cram(1, 0x0222, 0);
        assert!(
            !p.still_current(sys.vdp()),
            "a palette reload changes the picture without touching a tile, so it must invalidate it"
        );
        // ⚑ **The restoring control.** Putting the palette back must make it current again, or the
        // fingerprint is measuring "something happened" rather than "the art is different", and a test
        // that only ever sees it go false would pass on a function that always returns false.
        sys.vdp_mut().poke_cram(1, INK, 0);
        assert!(
            p.still_current(sys.vdp()),
            "the same art must fingerprint the same, or this is a change detector and not a measurement"
        );

        // Half two: the art under the same tile is decompressed over and the palette is untouched.
        sys.vdp_mut().vram_mut()[32] = 0x22;
        assert!(
            !p.still_current(sys.vdp()),
            "new art at the same tile must invalidate the picture drawn from the old art"
        );
    }

    /// **The subtype dimension is in the key and is always absent**, so one can be added without a rewrite
    /// and nothing here invents the concept.
    #[test]
    fn the_key_carries_a_subtype_slot_that_nothing_fills() {
        let sys = art_machine();
        let v = sys.vdp();
        let p = compose(
            &[sprite(0, 50, 50, 1, 1, 1, 0)],
            (50, 50),
            v,
            Key {
                archetype: "ObjDef_Ring".into(),
                subtype: None,
            },
        )
        .unwrap();
        assert!(p.is_of("ObjDef_Ring", None));
        assert!(
            !p.is_of("ObjDef_Ring", Some(3)),
            "a preview of no subtype is not a preview of subtype 3"
        );
        assert!(!p.is_of("ObjDef_Spring", None));
    }

    /// **A stale preview is not drawable**, which is the cache key's whole purpose expressed in the type
    /// rather than in a caller's discipline.
    #[test]
    fn a_stale_preview_cannot_be_drawn_and_still_says_what_happened() {
        let sys = art_machine();
        let v = sys.vdp();
        let p = compose(
            &[sprite(0, 50, 50, 1, 1, 1, 0)],
            (50, 50),
            v,
            Key {
                archetype: "ObjDef_Ring".into(),
                subtype: None,
            },
        )
        .unwrap();
        let stale = Outcome::Stale(Box::new(p.clone()));
        assert!(
            stale.drawable().is_none(),
            "a stale picture must be unreachable to a renderer, not merely discouraged"
        );
        assert!(stale.sentence().contains("ObjDef_Ring"));
        assert!(Outcome::Ready(Box::new(p)).drawable().is_some());
        assert!(Outcome::Absent("x".repeat(50)).drawable().is_none());
    }

    // ------------------------------------------------------------------------------------------
    // The style gates, over every string this module can put in front of a person
    // ------------------------------------------------------------------------------------------

    /// Every string this module can draw, assembled **from the projection** rather than from a list of
    /// literals a later edit would not appear in.
    fn every_string() -> Vec<String> {
        let sys = art_machine();
        let v = sys.vdp();
        let key = Key {
            archetype: "ObjDef_Test".into(),
            subtype: None,
        };
        let drawn = compose(&[sprite(0, 50, 50, 1, 1, 1, 0)], (50, 50), v, key.clone()).unwrap();
        let blank = compose(
            &[sprite(0, 50, 50, 1, 1, 0x300, 0)],
            (50, 50),
            v,
            key.clone(),
        )
        .unwrap();
        let two = compose(
            &[sprite(0, 50, 50, 1, 1, 1, 1), sprite(1, 58, 50, 1, 1, 2, 0)],
            (50, 50),
            v,
            key.clone(),
        )
        .unwrap();
        let mut all = vec![
            drawn.note(),
            drawn.size_line(),
            blank.note(),
            blank.size_line(),
            two.note(),
            two.size_line(),
            Outcome::Ready(Box::new(drawn.clone())).sentence(),
            Outcome::Stale(Box::new(drawn)).sentence(),
            compose(&[], (0, 0), v, key.clone()).unwrap_err(),
            compose(
                &[sprite(0, 0, 0, 1, 1, 1, 1), sprite(1, 300, 0, 1, 1, 1, 0)],
                (0, 0),
                v,
                key,
            )
            .unwrap_err(),
        ];
        all.push(appeared(&[sprite(0, 1, 1, 1, 1, 1, 0)], &[]).unwrap_err());
        all.push(Outcome::Absent(all[0].clone()).sentence());
        all
    }

    /// **P2, on the rendered value rather than on the source** — the widened check the Pacing tab proved
    /// out, because hand counted spaces inside a literal are invisible to a format specifier grep.
    #[test]
    fn no_string_this_module_draws_pads_itself_into_a_column() {
        for s in every_string() {
            assert!(
                !s.contains("  "),
                "a run of spaces is a column drawn inside a string: {s:?}"
            );
            assert!(!s.contains('\t'), "a tab is the same defect: {s:?}");
        }
    }

    /// **P10** (no em or en dashes) and **P9** (no specification citations), over every arm.
    #[test]
    fn nothing_this_module_draws_carries_a_dash_or_cites_the_specification() {
        for s in every_string() {
            for bad in ['\u{2014}', '\u{2013}'] {
                assert!(
                    !s.contains(bad),
                    "user facing text carries {bad:?}, which the owner's 2026-09-05 ruling bars: {s:?}"
                );
            }
            assert!(
                !s.contains('§') && !s.contains("protocol.md"),
                "the person at this window is not holding the specification: {s:?}"
            );
        }
    }

    /// **Every string says something.** A one word absence is the P6 defect wearing a sentence's clothes.
    #[test]
    fn every_line_this_module_draws_is_a_statement_rather_than_a_label() {
        for s in every_string() {
            assert!(
                s.len() > 20,
                "too short to be telling anybody anything: {s:?}"
            );
        }
    }
}
