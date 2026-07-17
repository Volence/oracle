//! Frame-dump dev tool — the project's first *visible* output. Boots a hand-authored 68000 fixture ROM on
//! a real [`System`], runs it for a few frames (the CPU programs the VDP over the control/data ports), then
//! renders the active display through the pure `Vdp::render_line` pipeline and writes it to a binary PPM
//! (P6) image.
//!
//! This is a **dev tool, not a gate artifact** — it exercises the full CPU → `MegaDriveBus` → VDP → renderer
//! path end-to-end. The fixture puts vertical white/red stripes (plane A, two solid tiles) on screen with
//! green/blue sprite boxes composited over them (via the SAT written through the data port, so the sprite
//! cache write-through is exercised). Push 5's **output stage** is visible too: **shadow/highlight** is enabled
//! (reg $0C bit 3), so the low-priority stripes render dimmed while a band of **high-priority** plane-A cells
//! renders at normal intensity (visible priority × S/H), and a palette-3 **highlight-operator** sprite lifts a
//! rectangle of the stripes back to normal. A glance at the PPM confirms priority ordering + shadow/highlight.
//!
//! Usage: `cargo run --release --example frame_dump -- [frames] [out.ppm]`
//! (defaults: 2 frames, `frame.ppm` in the current directory).

use oracle_core::system::System;
use std::io::Write;

/// Push a big-endian word.
fn w(rom: &mut Vec<u8>, word: u16) {
    rom.push((word >> 8) as u8);
    rom.push((word & 0xFF) as u8);
}

/// Push a big-endian long (a 32-bit immediate, or a VDP command = two control words).
fn l(rom: &mut Vec<u8>, long: u32) {
    w(rom, (long >> 16) as u16);
    w(rom, (long & 0xFFFF) as u16);
}

/// Assemble a VDP write-command longword for `move.l #cmd,(a0)` (control port): CD-low + A13–0 in the high
/// word, CD-high + A15–14 in the low word. `code` = the 6-bit command code (1 = VRAM write, 3 = CRAM write).
fn vdp_cmd(code: u8, addr: u16) -> u32 {
    let word1 = (((code & 0x03) as u32) << 14) | (addr as u32 & 0x3FFF);
    let word2 = ((((code >> 2) & 0x0F) as u32) << 4) | (addr as u32 >> 14);
    (word1 << 16) | word2
}

/// The 16-bit displacement of a `DBcc`/`BRA.w` whose extension word is at `ext_addr`, reaching `target`
/// (the 68000 adds the displacement to the address of the extension word).
fn disp16(target: u32, ext_addr: u32) -> u16 {
    (target as i32 - ext_addr as i32) as i16 as u16
}

/// Build the hand-authored fixture ROM: reset vectors + a setup routine that programs the VDP (registers,
/// CRAM palette, plane + sprite tiles, a striped plane-A nametable, and a small sprite attribute table) then
/// parks in `STOP`.
fn fixture_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x8]; // reset vectors filled below
                                  // Reset vectors: initial SSP = top of work RAM (even), initial PC = $200.
    rom[0..4].copy_from_slice(&0x00FF_FFFEu32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x0000_0200u32.to_be_bytes());
    rom.resize(0x200, 0); // pad to the code start

    // move.w #imm,(a0) / (a1) — the two VDP ports; move.l #imm,(a0) sets a two-word command.
    let ctrl = |rom: &mut Vec<u8>, word: u16| {
        w(rom, 0x30BC);
        w(rom, word);
    }; // move.w #word,(a0)
    let data = |rom: &mut Vec<u8>, word: u16| {
        w(rom, 0x32BC);
        w(rom, word);
    }; // move.w #word,(a1)
    let cmd = |rom: &mut Vec<u8>, c: u32| {
        w(rom, 0x20BC);
        l(rom, c);
    }; // move.l #cmd,(a0)

    // lea $00C00004,a0 (control) ; lea $00C00000,a1 (data)
    w(&mut rom, 0x41F9);
    l(&mut rom, 0x00C0_0004);
    w(&mut rom, 0x43F9);
    l(&mut rom, 0x00C0_0000);

    // VDP registers (only what the planes renderer reads): display on, plane A $C000, plane B $E000,
    // backdrop = CRAM index 1, full scroll, H32, h-scroll table $8000, autoinc 2, 32×32 planes.
    for word in [
        0x8140u16, // reg 1  = $40  display enable
        0x8230,    // reg 2  = $30  plane A base $C000
        0x8407,    // reg 4  = $07  plane B base $E000
        0x8558,    // reg 5  = $58  sprite attribute table base $B000
        0x8701,    // reg 7  = $01  backdrop = CRAM 1
        0x8B00,    // reg 11 = $00  full h + full v scroll
        0x8C08,    // reg 12 = $08  H32 + shadow/highlight enabled (bit 3)
        0x8D20,    // reg 13 = $20  h-scroll table base $8000
        0x8F02,    // reg 15 = $02  auto-increment 2
        0x9000,    // reg 16 = $00  32×32 plane
    ] {
        ctrl(&mut rom, word);
    }

    // Zero the h-scroll table (Scroll A/B = 0 at $8000; VRAM powers on random).
    cmd(&mut rom, vdp_cmd(0x01, 0x8000)); // VRAM write @ $8000
    data(&mut rom, 0x0000); // Scroll A
    data(&mut rom, 0x0000); // Scroll B

    // CRAM palette: 0 black, 1 white, 2 red, 3 green, 4 blue.
    cmd(&mut rom, vdp_cmd(0x03, 0x0000)); // CRAM write @ 0
    for c in [0x0000u16, 0x0EEE, 0x000E, 0x00E0, 0x0E00] {
        data(&mut rom, c);
    }

    // Tile 1 (byte 32) = solid colour 1; tile 2 (byte 64) = solid colour 2.
    cmd(&mut rom, vdp_cmd(0x01, 32));
    for _ in 0..16 {
        data(&mut rom, 0x1111);
    }
    cmd(&mut rom, vdp_cmd(0x01, 64));
    for _ in 0..16 {
        data(&mut rom, 0x2222);
    }

    // Sprite tiles (a 2×2 sprite uses 4 consecutive tiles, column-major): tiles 3–6 = solid green, tiles
    // 7–10 = solid blue. Tiles 3–10 are contiguous from byte 96, so one autoincrementing write fills them.
    cmd(&mut rom, vdp_cmd(0x01, 96));
    for _ in 0..(4 * 16) {
        data(&mut rom, 0x3333); // tiles 3–6 green
    }
    for _ in 0..(4 * 16) {
        data(&mut rom, 0x4444); // tiles 7–10 blue
    }

    // Operator tiles 11–14 = solid colour 14 (a shadow/highlight highlight-operator when drawn at palette 3).
    cmd(&mut rom, vdp_cmd(0x01, 11 * 32));
    for _ in 0..(4 * 16) {
        data(&mut rom, 0xEEEE);
    }

    // Clear plane B ($E000): power-on VRAM is random, and with real RR9 priority ordering a high-priority
    // garbage cell would beat the low-priority stripes. Zero all 1024 cells (32×32).
    cmd(&mut rom, vdp_cmd(0x01, 0xE000));
    w(&mut rom, 0x343C);
    w(&mut rom, 0x0000); // move.w #0,d2
    w(&mut rom, 0x303C);
    w(&mut rom, 1023); // move.w #1023,d0
    let clr_top = rom.len() as u32;
    w(&mut rom, 0x3282); // move.w d2,(a1)  write a zero cell
    w(&mut rom, 0x51C8); // dbra d0,<disp>
    let clr_ext = rom.len() as u32;
    w(&mut rom, disp16(clr_top, clr_ext));

    // Fill plane A ($C000) with vertical stripes: 896 cells, tile toggling 1↔2 each cell.
    cmd(&mut rom, vdp_cmd(0x01, 0xC000));
    w(&mut rom, 0x303C);
    w(&mut rom, 895); // move.w #895,d0  (DBRA runs count+1)
    w(&mut rom, 0x323C);
    w(&mut rom, 0x0001); // move.w #1,d1  (current tile)
    let loop_top = rom.len() as u32;
    w(&mut rom, 0x3281); // move.w d1,(a1)   write the cell
    w(&mut rom, 0x0A41);
    w(&mut rom, 0x0003); // eori.w #3,d1     toggle 1<->2
    w(&mut rom, 0x51C8); // dbra d0,<disp>
    let ext = rom.len() as u32;
    w(&mut rom, disp16(loop_top, ext));

    // A band of HIGH-priority white plane-A cells (rows 6–9 → screen y 48–79): under shadow/highlight these
    // render at normal intensity while the surrounding low-priority stripes are shadowed — visible priority×S/H.
    cmd(&mut rom, vdp_cmd(0x01, 0xC000 + 6 * 64));
    for _ in 0..(4 * 32) {
        data(&mut rom, 0x8001); // high-priority (bit 15) tile 1 (white)
    }

    // Sprite attribute table at $B000 (reg 5), written through the data port so the SAT-cache write-through
    // populates the cache the sprite evaluation reads. Three boxes over the stripes (Y/X fields are
    // screen + 128; size byte = high byte of the size/link word; the last sprite links to 0 to end the list).
    cmd(&mut rom, vdp_cmd(0x01, 0xB000));
    // Sprite 0: 2×2 green (tile 3) at screen (40, 40), link 1.
    data(&mut rom, 40 + 128); // Y
    data(&mut rom, 0x0501); // size 2×2, link 1
    data(&mut rom, 0x0003); // tile 3 (green)
    data(&mut rom, 40 + 128); // X
                              // Sprite 1: 2×2 blue (base tile 7) at screen (120, 80), link 2.
    data(&mut rom, 80 + 128); // Y
    data(&mut rom, 0x0502); // size 2×2, link 2
    data(&mut rom, 0x0007); // base tile 7 (blue block)
    data(&mut rom, 120 + 128); // X
                               // Sprite 2: 1×1 green (tile 3) at screen (200, 150), link 3.
    data(&mut rom, 150 + 128); // Y
    data(&mut rom, 0x0003); // size 1×1, link 3
    data(&mut rom, 0x0003); // tile 3 (green)
    data(&mut rom, 200 + 128); // X
                               // Sprite 3: a 2×2 highlight OPERATOR (palette 3, tiles 11–14) over the stripes at screen (100, 120),
                               // link 0 (end). It is not displayed — it lifts the shadowed stripes beneath it back to normal intensity.
    data(&mut rom, 120 + 128); // Y
    data(&mut rom, 0x0500); // size 2×2, link 0
    data(&mut rom, 0x600B); // palette 3, base tile 11 → highlight operator
    data(&mut rom, 100 + 128); // X

    // Park.
    w(&mut rom, 0x4E72);
    w(&mut rom, 0x2700); // stop #$2700
    rom
}

/// Write an RGB framebuffer (`width × height`, row-major) as a binary PPM (P6).
fn write_ppm(path: &str, width: usize, height: usize, rgb: &[(u8, u8, u8)]) -> std::io::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    write!(f, "P6\n{width} {height}\n255\n")?;
    let mut bytes = Vec::with_capacity(rgb.len() * 3);
    for &(r, g, b) in rgb {
        bytes.extend_from_slice(&[r, g, b]);
    }
    f.write_all(&bytes)?;
    f.flush()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let frames: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(2);
    let out = args.next().unwrap_or_else(|| "frame.ppm".to_string());

    let mut sys = System::new(0x5EED);
    sys.load_rom(fixture_rom());
    sys.reset();
    sys.run_frames(frames);

    // Render the active display (224 lines) through the pure renderer.
    let height = 224usize;
    let width = sys.vdp().render_line(0).len();
    let mut fb = Vec::with_capacity(width * height);
    for line in 0..height as u16 {
        fb.extend_from_slice(&sys.vdp().render_line(line));
    }

    match write_ppm(&out, width, height, &fb) {
        Ok(()) => println!("wrote {width}x{height} frame to {out} (after {frames} frames)"),
        Err(e) => eprintln!("failed to write {out}: {e}"),
    }
}
