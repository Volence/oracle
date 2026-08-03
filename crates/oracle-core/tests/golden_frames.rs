//! Golden-frame regression harness (design brief §5 validation-ladder rung 1/3 — *self-consistency*, not the
//! s4.bin/Exodus rung 2 which needs a real ROM). Each fixture scene is a `Vdp` built through the real
//! control/data ports, rendered to a full active-height framebuffer, and FNV-1a-hashed; the hashes are pinned
//! constants. Their purpose is to **lock every accumulated interim model** so a refinement can never silently
//! change a frame: the priority/shadow-highlight output stage (RR9/R11), the R8 partial-column extent, the R9
//! sub-tile alignment, the mode-01/10 h-scroll offsets, the R5 cache-window remainders, and the push-4
//! no-mid-sprite-cut budget approximation. Which model each scene pins is enumerated in
//! `docs/2026-07-16-vdp-pixel-known-differences.md` (the frame-level analogue of `known_differences.py`).
//!
//! A pinned hash is a *self-consistency* pin: it captures what the current model produces. Changing one is a
//! deliberate, evidenced amendment (a model refinement or a cross-emulator confirmation) — never a silent
//! regen. The `golden_frame_hash_discriminates` test proves the harness actually depends on the pixels.

use oracle_core::rng::SplitMix64;
use oracle_core::vdp::Vdp;

const ACTIVE_LINES: u16 = 224;

/// A powered-on VDP with cleared VRAM, **in Mode 5**. Every scene below programs Mode-5-only state
/// (autoincrement, plane/window bases, H40, two-cell scroll), and in Mode 4 — reg 1 bit 2 clear, which is
/// what a bare `power_on` leaves — registers above 10 are not writable. Declaring M5 here is what these
/// scenes always meant; it does not change any pinned hash below.
fn fresh() -> Vdp {
    let mut v = Vdp::power_on(&mut SplitMix64::new(1));
    v.vram_mut().fill(0);
    v.control_write(0x8104, 0); // reg 1 = $04 → M5 set (mode 5)
    v
}

/// Write VDP register `r` via a real `$8xxx` control-port register write.
fn set_reg(v: &mut Vdp, r: u8, val: u8) {
    v.control_write(0x8000 | ((r as u16) << 8) | val as u16, 0);
}

/// Arm a data-port write of `code` at VRAM/CRAM/VSRAM byte `addr` (two control words).
fn setup_write(v: &mut Vdp, code: u8, addr: u16) {
    let w1 = (((code & 0x03) as u16) << 14) | (addr & 0x3FFF);
    let w2 = ((((code >> 2) & 0x0F) as u16) << 4) | (addr >> 14);
    v.control_write(w1, 0);
    v.control_write(w2, 0);
}

fn write_cram(v: &mut Vdp, index: usize, word: u16) {
    setup_write(v, 0x03, (index * 2) as u16);
    v.data_write(word);
}

fn write_vsram(v: &mut Vdp, word_index: usize, word: u16) {
    setup_write(v, 0x05, (word_index * 2) as u16);
    v.data_write(word);
}

fn fill_tile(v: &mut Vdp, tile: usize, nibble: u8) {
    let byte = (nibble << 4) | nibble;
    for i in 0..32 {
        v.vram_mut()[tile * 32 + i] = byte;
    }
}

fn put_cell(v: &mut Vdp, addr: usize, word: u16) {
    v.vram_mut()[addr] = (word >> 8) as u8;
    v.vram_mut()[addr + 1] = (word & 0xFF) as u8;
}

/// Write one SAT entry's four words through the data port (so the SAT-cache write-through runs), base + reg-15
/// autoinc already set.
fn write_sprite(v: &mut Vdp, index: usize, y: u16, sizelink: u16, attr: u16, x: u16) {
    write_sprite_at(v, v.sat_base(), index, y, sizelink, attr, x);
}

/// [`write_sprite`] against an **explicit literal base**. Scene 5 uses this ON PURPOSE (reviewer fix): a
/// fixture that derives its write addresses from `sat_base()` moves coherently with any change to the
/// base-mask model and can never catch one — the literal base is what makes the P2 (H40 bit-0 mask) lock
/// real: if the mask model changed, the evaluation/window base would move away from these literal writes
/// and the frame would change.
fn write_sprite_at(
    v: &mut Vdp,
    base: usize,
    index: usize,
    y: u16,
    sizelink: u16,
    attr: u16,
    x: u16,
) {
    setup_write(v, 0x01, (base + index * 8) as u16);
    v.data_write(y);
    v.data_write(sizelink);
    v.data_write(attr);
    v.data_write(x);
}

/// A small shared palette: 1 white, 2 red, 3 green, 4 blue, plus a couple of gradients.
fn base_palette(v: &mut Vdp) {
    for (i, c) in [
        (0usize, 0x0000u16),
        (1, 0x0EEE),
        (2, 0x000E),
        (3, 0x00E0),
        (4, 0x0E00),
    ] {
        write_cram(v, i, c);
    }
}

/// FNV-1a-64 over the full active framebuffer (RGB bytes, row-major). Width follows the mode (H32 256 / H40
/// 320) via `render_line`'s own length.
fn frame_hash(v: &Vdp) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for line in 0..ACTIVE_LINES {
        for (r, g, b) in v.render_line(line) {
            for byte in [r, g, b] {
                h ^= byte as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    h
}

// --- Scene 1: priority ordering (RR9) + shadow/highlight (R11) ----------------------------------------------

/// Alternating high-priority red / low-priority blue plane-A stripes over a solid low-priority green plane B,
/// with a low-priority red sprite (loses to high-A, wins over low-A) and a highlight operator, S/H enabled.
fn scene_priority_sh() -> Vdp {
    let mut v = fresh();
    set_reg(&mut v, 0x01, 0x44); // display on
    set_reg(&mut v, 0x0C, 0x08); // S/H on, H32
    set_reg(&mut v, 0x02, 0x30); // plane A $C000
    set_reg(&mut v, 0x04, 0x07); // plane B $E000
    set_reg(&mut v, 0x05, 0x58); // SAT base $B000
    set_reg(&mut v, 0x10, 0x00); // 32×32
    set_reg(&mut v, 0x0D, 0x20); // h-scroll table $8000
    set_reg(&mut v, 0x0B, 0x00); // full scroll
    set_reg(&mut v, 0x0F, 0x02); // autoinc 2
    base_palette(&mut v);
    fill_tile(&mut v, 1, 1); // white
    fill_tile(&mut v, 2, 2); // red
    fill_tile(&mut v, 3, 3); // green
    fill_tile(&mut v, 8, 14); // highlight operator
    for row in 0..28 {
        for col in 0..32 {
            let a = if col % 2 == 0 { 0x8001 } else { 0x0002 }; // high white / low red
            put_cell(&mut v, 0xC000 + (row * 32 + col) * 2, a);
            put_cell(&mut v, 0xE000 + (row * 32 + col) * 2, 0x0003); // low green everywhere
        }
    }
    // 2×2 low-priority red sprite at (40,40), link 1; a highlight operator sprite at (80,72), link 0.
    write_sprite(&mut v, 0, 40 + 128, 0x0501, 0x0002, 40 + 128);
    write_sprite(&mut v, 1, 72 + 128, 0x0000, 0x6008, 80 + 128);
    v
}

// --- Scene 2: R8 partial-column v-scroll (leftmost 16 px in 2-cell mode) -------------------------------------

/// H32 2-cell v-scroll with plane-B `hscroll & 15 != 0` — the leftmost partial column takes the R8 fixed-0
/// path (H32) while the other columns scroll by their own VSRAM words. Plane B is a per-row tile gradient so
/// the v-scroll choice is visible in the frame.
fn scene_r8_partial_column() -> Vdp {
    let mut v = fresh();
    set_reg(&mut v, 0x01, 0x44);
    set_reg(&mut v, 0x04, 0x07); // plane B $E000
    set_reg(&mut v, 0x10, 0x00);
    set_reg(&mut v, 0x0D, 0x20);
    set_reg(&mut v, 0x0B, 0x04); // full h, 2-cell v
    base_palette(&mut v);
    for t in 1..=8 {
        fill_tile(&mut v, t, t as u8); // distinct tiles per row band
        write_cram(&mut v, t, (t as u16 & 7) << 1); // distinct reds
    }
    for row in 0..32 {
        let tile = (row % 8 + 1) as u16;
        for col in 0..32 {
            put_cell(&mut v, 0xE000 + (row * 32 + col) * 2, tile);
        }
    }
    put_cell(&mut v, 0x8002, 4); // plane B hscroll = 4 → & 15 != 0 (engages R8)
    for c in 0..20 {
        write_vsram(&mut v, c * 2 + 1, ((c * 8) & 0xFF) as u16); // per-column B vscroll
    }
    write_vsram(&mut v, 39, 24); // VSRAM $4E (the H40 AND partner; H32 forces 0 regardless)
    v
}

// --- Scene 3: R9 window bug (left window + plane-A fine scroll) ----------------------------------------------

fn scene_r9_window_bug() -> Vdp {
    let mut v = fresh();
    set_reg(&mut v, 0x01, 0x44);
    set_reg(&mut v, 0x02, 0x30); // plane A $C000
    set_reg(&mut v, 0x03, 0x28); // window $A000
    set_reg(&mut v, 0x10, 0x00);
    set_reg(&mut v, 0x0D, 0x20);
    set_reg(&mut v, 0x0B, 0x00);
    set_reg(&mut v, 0x11, 0x03); // left window WHP=3 → [0,48)
    base_palette(&mut v);
    fill_tile(&mut v, 1, 1);
    fill_tile(&mut v, 2, 2);
    fill_tile(&mut v, 3, 3);
    // Window last columns distinct from plane A so the R9 reuse is visible.
    for col in 0..6 {
        put_cell(&mut v, 0xA000 + col * 2, (1 + (col as u16 % 3)) & 0x7FF);
    }
    for col in 0..32 {
        put_cell(&mut v, 0xC000 + col * 2, 0x0002); // plane A solid red
    }
    put_cell(&mut v, 0x8000, 5); // plane A hscroll = 5 → & 15 != 0 → R9 active
    v
}

// --- Scene 4: mode-01 / mode-10 h-scroll offsets ------------------------------------------------------------

fn scene_hscroll_modes() -> Vdp {
    let mut v = fresh();
    set_reg(&mut v, 0x01, 0x44);
    set_reg(&mut v, 0x04, 0x07); // plane B $E000
    set_reg(&mut v, 0x10, 0x00);
    set_reg(&mut v, 0x0D, 0x20);
    set_reg(&mut v, 0x0B, 0x02); // per-cell-row h-scroll (mode 10: (line & !7) * 4)
    base_palette(&mut v);
    fill_tile(&mut v, 1, 1);
    // Dashes on EVERY nametable row (reviewer fix): with content only on row 0, the sole visible band was
    // band 0 whose mode-10 offset is 0 under any plausible indexing model — the hash could not discriminate
    // the P6 model it exists to lock. With all 28 bands populated, each band's distinct scroll is visible
    // and a changed mode-10/01 indexing changes the frame.
    for row in 0..28 {
        for c in 0..32 {
            put_cell(
                &mut v,
                0xE000 + (row * 32 + c) * 2,
                if c % 2 == 0 { 0x0001 } else { 0x0000 },
            );
        }
    }
    // A per-cell-row h-scroll table: each 8-line band scrolls by a distinct amount.
    for band in 0..28 {
        let off = (band as usize * 8) * 4 + 2; // Scroll B entry for line band*8
        put_cell(&mut v, 0x8000 + off, ((band * 3) & 0x3FF) as u16);
    }
    v
}

// --- Scene 5: R5 cache window (H40 base bit-0 mask + byte-granular pokes) ------------------------------------

fn scene_r5_cache_window() -> Vdp {
    let mut v = fresh();
    set_reg(&mut v, 0x01, 0x44);
    set_reg(&mut v, 0x0C, 0x81); // H40
    set_reg(&mut v, 0x05, 0x59); // SAT base: H40 masks bit 0 → $B000 (0x58<<9), not $B200
    set_reg(&mut v, 0x0F, 0x02);
    base_palette(&mut v);
    fill_tile(&mut v, 3, 3); // green sprite
                             // Write the sprites at the LITERAL masked base $B000 — deliberately NOT via `sat_base()` (reviewer fix:
                             // a `sat_base()`-derived fixture moves coherently with a mask-model change and can never catch one; the
                             // literal address is what locks P2 — under an unmasked model the evaluation base would be $B200 and
                             // these writes would populate the wrong entries → a different frame).
    write_sprite_at(&mut v, 0xB000, 0, 30 + 128, 0x0501, 0x0003, 60 + 128); // 2×2 green at (60,30), link 1
    write_sprite_at(&mut v, 0xB000, 1, 50 + 128, 0x0000, 0x0003, 160 + 128); // 1×1 green at (160,50), link 0
                                                                             // A byte-granular poke into the cached half of entry 0's Y (odd LITERAL address) through the port.
    setup_write(&mut v, 0x01, 0xB001);
    v.data_write(0x00_2A); // low byte of entry-0 Y → moves the sprite; exercises byte-granular write-through
    v
}

// --- Scene 6: no-mid-sprite-cut pixel-budget approximation --------------------------------------------------

/// H32 (256 px / 16 sprites / 256 px budget): nine 4-cell-wide sprites on one line = 288 px > 256. The sprite
/// that overshoots the budget draws FULLY (no mid-sprite cut); the next is dropped. Pins the push-4 model.
fn scene_no_mid_sprite_cut() -> Vdp {
    let mut v = fresh();
    set_reg(&mut v, 0x01, 0x44);
    set_reg(&mut v, 0x05, 0x58); // SAT base $B000
    set_reg(&mut v, 0x0F, 0x02);
    base_palette(&mut v);
    fill_tile(&mut v, 4, 4); // blue
    for i in 0..9u16 {
        let link = if i + 1 < 9 { i + 1 } else { 0 };
        // 4×1 blue sprites (32 px wide), stepped across the line, all on line 20.
        write_sprite(
            &mut v,
            i as usize,
            20 + 128,
            (0x0C << 8) | link,
            0x0004,
            128 + i * 28,
        );
    }
    v
}

#[test]
fn golden_frame_scene_1_priority_shadow_highlight() {
    assert_eq!(frame_hash(&scene_priority_sh()), 0x2c8f_6ffb_131a_8fa5);
}

#[test]
fn golden_frame_scene_2_r8_partial_column() {
    assert_eq!(
        frame_hash(&scene_r8_partial_column()),
        0xf453_9d9d_293d_8725
    );
}

#[test]
fn golden_frame_scene_3_r9_window_bug() {
    assert_eq!(frame_hash(&scene_r9_window_bug()), 0x2c70_3e59_7d69_a625);
}

#[test]
fn golden_frame_scene_4_hscroll_modes() {
    // Hash regenerated 2026-07-16 (reviewer amendment, documented in the commit): the scene gained content
    // on all 28 bands so the mode-10 per-band scroll is actually visible — the prior single-band scene
    // hashed identically under any indexing model and locked nothing. Prior: 0xd985_1c89_7724_cf25.
    assert_eq!(frame_hash(&scene_hscroll_modes()), 0xab18_e048_ff40_0b25);
}

#[test]
fn golden_frame_scene_5_r5_cache_window() {
    assert_eq!(frame_hash(&scene_r5_cache_window()), 0x9b24_7c8a_c955_c165);
}

#[test]
fn golden_frame_scene_6_no_mid_sprite_cut() {
    assert_eq!(
        frame_hash(&scene_no_mid_sprite_cut()),
        0xd749_dfdf_587d_0d25
    );
}

/// The harness actually depends on the pixels: a one-cell change to a scene changes its frame hash. Guards
/// against a degenerate hash (e.g. a constant) that would make the goldens vacuous.
#[test]
fn golden_frame_hash_discriminates() {
    let base = scene_priority_sh();
    let mut altered = scene_priority_sh();
    put_cell(&mut altered, 0xC000, 0x0004); // change one plane-A cell
    assert_ne!(
        frame_hash(&base),
        frame_hash(&altered),
        "the frame hash must depend on the framebuffer contents"
    );
}
