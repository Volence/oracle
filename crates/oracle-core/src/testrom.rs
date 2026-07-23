//! A hand-authored vendored 68000 test ROM — machine-code bytes built in-test, **no toolchain
//! dependency**. Used by the determinism gate, the `System` unit tests, and the integration tests to give
//! the real CPU real code to run.
//!
//! `#[doc(hidden)]` — this is a test fixture, not part of the public API. Every opcode below is decoded by
//! the (SST-proven) decoder, so the ROM's *behavior* under the real CPU is the ground truth; the byte
//! comments name each instruction.
//!
//! ## Structure (big-endian; ROM base `$000000`)
//!
//! ```text
//! $000000  dc.l $00FFFFFE   ; initial SSP (top of work RAM, even)
//! $000004  dc.l $00000200   ; initial PC  (code start)
//! $000010  dc.l $00000280   ; vector 4  (illegal instruction) -> ILLEGAL_H
//! $000078  dc.l $000002A0   ; vector 30 (autovector level 6, VInt) -> INT_H
//!
//! $000200  main:   move.w #$2000, SR      ; supervisor, T=0, INT mask 0 (interrupts enabled)
//! $000204  reload: lea    $00FF0000, A0   ; A0 = work-RAM base
//! $00020A          move.w #$3FFF, D1      ; 0x4000 words to stir ($FF0000..$FF7FFE)
//! $00020E  inner:  move.w (A0), D0
//! $000210          addq.w #1, D0
//! $000212          move.w D0, (A0)+       ; += 1, advance
//! $000214          dbra   D1, inner
//! $000218          bra.w  reload          ; forever — each pass +1 to every stirred RAM word
//!
//! $000280  ILLEGAL_H: move.w #$DEAD, $FF8004   ; sentinel (outside the stirred range)
//! $000286             stop   #$2700            ; park (exercises STOP)
//!
//! $0002A0  INT_H:     move.w #$1234, $FF8000   ; sentinel (outside the stirred range)
//! $0002A6             rte
//! ```
//!
//! The main loop stirs the first `$4000` work-RAM words (`$FF0000..$FF7FFE`) by +1 per pass, reloading A0
//! each pass so it stays in RAM. `$FF8000..$FFFFFE` is left untouched — the supervisor stack and the two
//! handler sentinels live there, safe from the loop.

/// Total ROM image size (covers the reset vectors, the two used exception vectors, and the code).
const ROM_LEN: usize = 0x300;

/// Code / handler addresses (also the values written into the vector table).
const MAIN: u32 = 0x0000_0200;
const RELOAD: u32 = 0x0000_0204;
const INNER: u32 = 0x0000_020E;
const ILLEGAL_H: u32 = 0x0000_0280;
const INT_H: u32 = 0x0000_02A0;

/// Initial supervisor stack pointer (top of the 64 KiB work RAM, kept even).
const INITIAL_SSP: u32 = 0x00FF_FFFE;

/// A byte address inside work RAM the main loop never stirs — where the interrupt handler drops its
/// sentinel (`$FF8000`, the low mirror is `ram[0x8000]`).
pub const INT_SENTINEL_ADDR: u32 = 0x00FF_8000;
/// The value `INT_H` writes at [`INT_SENTINEL_ADDR`] — proof the interrupt was taken.
pub const INT_SENTINEL: u16 = 0x1234;

/// Write a big-endian word at byte offset `at`.
fn put_word(rom: &mut [u8], at: u32, w: u16) {
    rom[at as usize] = (w >> 8) as u8;
    rom[at as usize + 1] = (w & 0xFF) as u8;
}

/// Write a big-endian long at byte offset `at`.
fn put_long(rom: &mut [u8], at: u32, l: u32) {
    put_word(rom, at, (l >> 16) as u16);
    put_word(rom, at + 2, (l & 0xFFFF) as u16);
}

/// The 16-bit displacement of a `Bcc.w`/`BRA.w`/`DBcc` that reaches `target` from an extension word at
/// `ext_addr`. The 68000 adds the displacement to the PC pointing at the *extension word* (instruction
/// address + 2), so `disp = target - ext_addr`.
fn disp16(target: u32, ext_addr: u32) -> u16 {
    (target as i32 - ext_addr as i32) as i16 as u16
}

/// Build the test ROM image.
#[doc(hidden)]
pub fn build() -> Vec<u8> {
    let mut rom = vec![0u8; ROM_LEN];

    // --- Reset vectors (read by the power-on reset recipe: SSP@$0, PC@$4) ---
    put_long(&mut rom, 0x0, INITIAL_SSP);
    put_long(&mut rom, 0x4, MAIN);
    // --- Exception vectors ---
    put_long(&mut rom, 0x10, ILLEGAL_H); // vector 4  (illegal instruction)
    put_long(&mut rom, 0x78, INT_H); // vector 30 (autovector, interrupt level 6 / VInt)

    // --- main: enable interrupts (supervisor, T=0, INT mask 0) ---
    put_word(&mut rom, 0x200, 0x46FC); // move.w #imm, SR
    put_word(&mut rom, 0x202, 0x2000); //   #$2000

    // --- reload: A0 = work-RAM base ---
    put_word(&mut rom, 0x204, 0x41F9); // lea (xxx).l, A0
    put_long(&mut rom, 0x206, 0x00FF_0000); //   $00FF0000

    // --- D1 = iteration count (0x4000 words: DBRA runs count+1 times) ---
    put_word(&mut rom, 0x20A, 0x323C); // move.w #imm, D1
    put_word(&mut rom, 0x20C, 0x3FFF); //   #$3FFF

    // --- inner: stir one word ---
    put_word(&mut rom, 0x20E, 0x3010); // move.w (A0), D0
    put_word(&mut rom, 0x210, 0x5240); // addq.w #1, D0
    put_word(&mut rom, 0x212, 0x30C0); // move.w D0, (A0)+

    // --- dbra D1, inner ---
    put_word(&mut rom, 0x214, 0x51C9); // dbra D1, <disp>
    put_word(&mut rom, 0x216, disp16(INNER, 0x216));

    // --- bra.w reload ---
    put_word(&mut rom, 0x218, 0x6000); // bra.w <disp>
    put_word(&mut rom, 0x21A, disp16(RELOAD, 0x21A));

    // --- ILLEGAL_H: sentinel then STOP ---
    put_word(&mut rom, 0x280, 0x33FC); // move.w #imm, (xxx).l
    put_word(&mut rom, 0x282, 0xDEAD); //   #$DEAD
    put_long(&mut rom, 0x284, 0x00FF_8004); //   $00FF8004
    put_word(&mut rom, 0x288, 0x4E72); // stop #imm
    put_word(&mut rom, 0x28A, 0x2700); //   #$2700

    // --- INT_H: sentinel then RTE ---
    put_word(&mut rom, 0x2A0, 0x33FC); // move.w #imm, (xxx).l
    put_word(&mut rom, 0x2A2, INT_SENTINEL); //   #$1234
    put_long(&mut rom, 0x2A4, INT_SENTINEL_ADDR); //   $00FF8000
    put_word(&mut rom, 0x2A8, 0x4E73); // rte

    rom
}

/// A variant of [`build`] whose level-6 (VInt) handler **increments** the word counter at
/// [`INT_SENTINEL_ADDR`] instead of writing a constant sentinel, and then RTEs — so a test can count how
/// many times the interrupt was taken. Used by the "VInt taken once, not re-fired after RTE" docket test.
/// [`build`] itself is left byte-identical (the golden fixture depends on it).
#[doc(hidden)]
pub fn build_vint_counter() -> Vec<u8> {
    let mut rom = build();
    // INT_H: addq.w #1, ($00FF8000).L ; rte  (replaces the `move.w #$1234, $FF8000` body).
    put_word(&mut rom, 0x2A0, 0x5279); // addq.w #1, (xxx).L
    put_long(&mut rom, 0x2A2, INT_SENTINEL_ADDR); // $00FF8000
    put_word(&mut rom, 0x2A6, 0x4E73); // rte
    rom
}

/// The VRAM byte address the [`build_vram_poke`] fixture writes to (high byte at this address, low byte at
/// `+1`, autoinc 2).
pub const VRAM_POKE_ADDR: u32 = 0x0100;
/// The word [`build_vram_poke`] stores at [`VRAM_POKE_ADDR`] (so VRAM `$0100 = $BE`, `$0101 = $EF`).
pub const VRAM_POKE_WORD: u16 = 0xBEEF;
/// The PC of the `move.w #VRAM_POKE_WORD,(a1)` data-port write in [`build_vram_poke`] — the instruction a VRAM
/// watch attributes the poke to.
pub const VRAM_POKE_PC: u32 = 0x0000_0216;

/// Build the **VRAM-poke fixture ROM** — the minimal end-to-end proof for a *direct* VDP-internal watch
/// (watchpoints v2). It boots, points a0/a1 at the VDP control/data ports, sets autoinc 2, issues a VRAM-write
/// command at [`VRAM_POKE_ADDR`], writes [`VRAM_POKE_WORD`] through the data port (at PC [`VRAM_POKE_PC`]), then
/// spins forever. So one frame produces exactly one direct VRAM word write — two byte captures (`$BE` at
/// `$0100`, `$EF` at `$0101`), both `via = Direct`, both attributed to [`VRAM_POKE_PC`].
#[doc(hidden)]
pub fn build_vram_poke() -> Vec<u8> {
    let mut rom = vec![0u8; 0x220];
    put_long(&mut rom, 0x0, INITIAL_SSP); // reset SSP
    put_long(&mut rom, 0x4, MAIN); // reset PC = $200

    // a0 = VDP control ($C00004), a1 = VDP data ($C00000).
    put_word(&mut rom, 0x200, 0x41F9); // lea (xxx).l, a0
    put_long(&mut rom, 0x202, 0x00C0_0004);
    put_word(&mut rom, 0x206, 0x43F9); // lea (xxx).l, a1
    put_long(&mut rom, 0x208, 0x00C0_0000);
    put_word(&mut rom, 0x20C, 0x30BC); // move.w #imm,(a0)
    put_word(&mut rom, 0x20E, 0x8F02); //   reg 15 = autoinc 2
    put_word(&mut rom, 0x210, 0x20BC); // move.l #imm,(a0)
                                       // VRAM write command @ $0100: word1 = 0x4100 (CD1CD0=01, A13-A0=$0100), word2 = 0x0000.
    put_long(&mut rom, 0x212, 0x4100_0000);
    put_word(&mut rom, VRAM_POKE_PC, 0x32BC); // move.w #imm,(a1)  ← the poke
    put_word(&mut rom, VRAM_POKE_PC + 2, VRAM_POKE_WORD);
    put_word(&mut rom, 0x21A, 0x60FE); // bra.s * (spin at $21A forever)
    rom
}

/// Build the **pad-poll fixture ROM** — the end-to-end proof for the controller/I/O push. It boots on a real
/// [`crate::system::System`], zeroes VRAM (so the whole screen is transparent → pure backdrop), configures
/// Player-1's port for a 3-button read (TH output), and then loops forever polling the pad through the real
/// TH protocol (recon IO4): it reads the TH=0 nibble, extracts **Start** (bit 5, active-low), and sets the
/// backdrop colour register (VDP reg 7) to CRAM index **1** (Start released) or **2** (Start held). So a
/// glance at the rendered frame — every pixel is the backdrop — shows whether the injected pad reached the
/// screen. There are no conditional branches on the input: `backdrop = $8702 - start_released_bit`, so the
/// only branch is the outer poll loop.
///
/// Used by the `io_controllers` integration test (inject → run → assert the pixel flipped) and the
/// `pad_probe` example (before/after PPM pair). This is a **new scene**, independent of the golden fixtures.
#[doc(hidden)]
pub fn build_pad_poll() -> Vec<u8> {
    let mut rom = vec![0u8; 0x8];
    rom[0..4].copy_from_slice(&INITIAL_SSP.to_be_bytes()); // reset SSP
    rom[4..8].copy_from_slice(&MAIN.to_be_bytes()); // reset PC = $200
    rom.resize(0x200, 0);

    // Append helpers (big-endian), mirroring examples/frame_dump.rs.
    fn w(rom: &mut Vec<u8>, word: u16) {
        rom.push((word >> 8) as u8);
        rom.push((word & 0xFF) as u8);
    }
    fn l(rom: &mut Vec<u8>, long: u32) {
        w(rom, (long >> 16) as u16);
        w(rom, (long & 0xFFFF) as u16);
    }
    // VDP write-command longword (control-port) for a two-word command.
    fn vdp_cmd(code: u8, addr: u16) -> u32 {
        let word1 = (((code & 0x03) as u32) << 14) | (addr as u32 & 0x3FFF);
        let word2 = ((((code >> 2) & 0x0F) as u32) << 4) | (addr as u32 >> 14);
        (word1 << 16) | word2
    }
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

    // a0 = VDP control ($C00004), a1 = VDP data ($C00000), a2 = I/O base ($A10000).
    w(&mut rom, 0x41F9);
    l(&mut rom, 0x00C0_0004);
    w(&mut rom, 0x43F9);
    l(&mut rom, 0x00C0_0000);
    w(&mut rom, 0x45F9);
    l(&mut rom, 0x00A1_0000);

    // VDP registers: display + DMA enable, plane/sprite bases, H32, autoinc 2 (for the CRAM writes below).
    for word in [
        0x8150u16, // reg 1  display + DMA enable
        0x8230,    // reg 2  plane A $C000
        0x8407,    // reg 4  plane B $E000
        0x8558,    // reg 5  SAT $B000
        0x8701,    // reg 7  backdrop = CRAM 1 (initial; the loop overwrites it)
        0x8B00,    // reg 11 full scroll
        0x8C00,    // reg 12 H32, no shadow/highlight
        0x8D20,    // reg 13 h-scroll table $8000
        0x8F02,    // reg 15 autoinc 2 (CRAM entries are 2 bytes)
        0x9000,    // reg 16 32×32 planes
    ] {
        ctrl(&mut rom, word);
    }

    // CRAM palette: 0 black, 1 white, 2 red — the two backdrop colours the loop selects between. Autoinc 2
    // steps entry-by-entry (a byte-1 autoinc would overlap the two-byte entries and corrupt the palette).
    cmd(&mut rom, vdp_cmd(0x03, 0x0000));
    for c in [0x0000u16, 0x0EEE, 0x000E] {
        data(&mut rom, c);
    }

    // Zero VRAM with a fill DMA ($0000, $FFFF bytes, fill byte $00): every tile/nametable/SAT byte → 0, so
    // all plane + sprite pixels are transparent and the whole screen shows the backdrop. Autoinc 1 covers
    // every byte.
    ctrl(&mut rom, 0x8F01); // reg 15 autoinc 1
    ctrl(&mut rom, 0x93FF); // reg 19 fill length low
    ctrl(&mut rom, 0x94FF); // reg 20 fill length high → $FFFF bytes
    ctrl(&mut rom, 0x9780); // reg 23 DMA fill mode
    cmd(&mut rom, vdp_cmd(0x21, 0x0000)); // VRAM write @ $0000 + CD5
    data(&mut rom, 0x0000); // data-port write triggers the fill (fill byte = top byte = $00)

    // Controller init: P1 control = $40 (TH output), P1 data = $00 (drive TH low for the Start/A nibble).
    w(&mut rom, 0x157C);
    w(&mut rom, 0x0040);
    w(&mut rom, 0x0009); // move.b #$40,(9,a2)
    w(&mut rom, 0x157C);
    w(&mut rom, 0x0000);
    w(&mut rom, 0x0003); // move.b #$00,(3,a2)

    // Poll loop (forever): read the TH=0 nibble, isolate Start (bit 5, active-low), set reg 7 = $8702 − s,
    // where s = 1 when Start is *released* → backdrop 1, s = 0 when *held* → backdrop 2. No input branch.
    let loop_top = rom.len() as u32;
    w(&mut rom, 0x102A);
    w(&mut rom, 0x0003); // move.b (3,a2),d0
    w(&mut rom, 0xEA48); // lsr.w #5,d0        d0 bit0 = old Start bit
    w(&mut rom, 0x0240);
    w(&mut rom, 0x0001); // andi.w #1,d0       d0 = 1 released / 0 held
    w(&mut rom, 0x323C);
    w(&mut rom, 0x8702); // move.w #$8702,d1
    w(&mut rom, 0x9240); // sub.w d0,d1        d1 = $8701 released / $8702 held
    w(&mut rom, 0x3081); // move.w d1,(a0)     reg 7 ← backdrop select
    let bra_at = rom.len() as u32;
    let disp = (loop_top as i32 - (bra_at as i32 + 2)) as i8 as u8;
    w(&mut rom, 0x6000 | disp as u16); // bra.s loop_top

    rom
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::MegaDriveBus;
    use crate::m68000::microop::Cpu68000;
    use crate::m68000::registers::Registers;

    fn rd_word(rom: &[u8], at: usize) -> u16 {
        ((rom[at] as u16) << 8) | rom[at + 1] as u16
    }
    fn rd_long(rom: &[u8], at: usize) -> u32 {
        ((rd_word(rom, at) as u32) << 16) | rd_word(rom, at + 2) as u32
    }

    #[test]
    fn vector_table_points_at_the_ssp_pc_and_handlers() {
        let rom = build();
        assert_eq!(rd_long(&rom, 0x0), INITIAL_SSP, "reset SSP");
        assert_eq!(rd_long(&rom, 0x4), MAIN, "reset PC");
        assert_eq!(rd_long(&rom, 0x10), ILLEGAL_H, "vector 4 = illegal handler");
        assert_eq!(rd_long(&rom, 0x78), INT_H, "vector 30 = interrupt handler");
    }

    #[test]
    fn opcode_words_are_the_expected_instructions() {
        let rom = build();
        assert_eq!(rd_word(&rom, 0x200), 0x46FC, "move.w #imm,SR");
        assert_eq!(rd_word(&rom, 0x204), 0x41F9, "lea (xxx).l,A0");
        assert_eq!(rd_word(&rom, 0x20E), 0x3010, "move.w (A0),D0");
        assert_eq!(rd_word(&rom, 0x210), 0x5240, "addq.w #1,D0");
        assert_eq!(rd_word(&rom, 0x212), 0x30C0, "move.w D0,(A0)+");
        assert_eq!(rd_word(&rom, 0x214), 0x51C9, "dbra D1");
        assert_eq!(rd_word(&rom, 0x218), 0x6000, "bra.w");
        assert_eq!(rd_word(&rom, 0x288), 0x4E72, "stop");
        assert_eq!(rd_word(&rom, 0x2A8), 0x4E73, "rte");
    }

    /// Zeroed registers — the power-on state before the reset recipe populates SSP/PC/prefetch.
    fn zeroed_regs() -> Registers {
        Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 0,
            pc: 0,
            sr: 0,
            prefetch: [0; 2],
        }
    }

    /// Drive the real CPU over a `MegaDriveBus` (the same adapter `System` uses) and confirm the loop
    /// mechanics: reset primes PC, then `lea`/`move`/`addq`/`move (A0)+`/`dbra` stir RAM and branch back.
    #[test]
    fn runs_on_the_real_cpu_and_stirs_ram() {
        let rom = build();
        let mut ram = vec![0u8; crate::system::RAM_SIZE];
        let mut z80 = vec![0u8; crate::bus::Z80_RAM_SIZE];
        let mut last = 0u16;
        let mut cpu = Cpu68000::new(zeroed_regs());
        cpu.assert_reset();

        let step = |cpu: &mut Cpu68000, ram: &mut Vec<u8>, z80: &mut Vec<u8>, last: &mut u16| {
            let mut sink = ();
            // This ROM never touches the VDP ports; a fresh VDP + mclk 0 suffices for the CPU/RAM checks.
            let mut vdp = crate::vdp::Vdp::power_on(&mut crate::rng::SplitMix64::new(1));
            let mut io = crate::io::Io::default();
            let mut z80_busreq = false;
            let mut z80_running = false;
            let mut sram_enabled = false;
            let mut sram_write_protect = false;
            let mut sram: Vec<u8> = Vec::new();
            let mut sram_dirty = false;
            let mut fm = crate::ym2612::Ym2612::new();
            let mut bus = MegaDriveBus::new(
                &rom,
                ram,
                z80,
                &mut vdp,
                &mut io,
                0,
                last,
                &mut z80_busreq,
                &mut z80_running,
                &mut sram_enabled,
                &mut sram_write_protect,
                &mut sram,
                &mut sram_dirty,
                None,
                &mut fm,
                &mut sink,
            );
            cpu.step(&mut bus);
        };

        step(&mut cpu, &mut ram, &mut z80, &mut last); // reset recipe -> PC primed at $200
        assert_eq!(cpu.regs.pc, MAIN, "reset primed PC at main");

        step(&mut cpu, &mut ram, &mut z80, &mut last); // move.w #$2000,SR
        assert_eq!(cpu.regs.sr & 0x0700, 0, "INT mask lowered to 0");
        assert!(cpu.regs.sr & 0x2000 != 0, "still supervisor");

        step(&mut cpu, &mut ram, &mut z80, &mut last); // lea $FF0000,A0
        assert_eq!(cpu.regs.a[0], 0x00FF_0000);

        step(&mut cpu, &mut ram, &mut z80, &mut last); // move.w #$3FFF,D1
        assert_eq!(cpu.regs.d[1] & 0xFFFF, 0x3FFF);

        // First inner pass: move.w (A0),D0 ; addq.w #1,D0 ; move.w D0,(A0)+
        step(&mut cpu, &mut ram, &mut z80, &mut last);
        step(&mut cpu, &mut ram, &mut z80, &mut last);
        step(&mut cpu, &mut ram, &mut z80, &mut last);
        assert_eq!(&ram[0..2], &[0x00, 0x01], "ram[0] incremented 0 -> 1");
        assert_eq!(cpu.regs.a[0], 0x00FF_0002, "A0 advanced one word");

        // dbra branches back to inner; a second inner pass stirs the next word.
        step(&mut cpu, &mut ram, &mut z80, &mut last); // dbra
        assert_eq!(cpu.regs.d[1] & 0xFFFF, 0x3FFE, "D1 decremented");
        step(&mut cpu, &mut ram, &mut z80, &mut last); // move.w (A0),D0
        step(&mut cpu, &mut ram, &mut z80, &mut last); // addq
        step(&mut cpu, &mut ram, &mut z80, &mut last); // move.w D0,(A0)+
        assert_eq!(&ram[2..4], &[0x00, 0x01], "ram[1] word incremented");
        assert_eq!(cpu.regs.a[0], 0x00FF_0004, "dbra looped: A0 advanced again");
    }
}
