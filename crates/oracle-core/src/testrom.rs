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
            let mut bus = MegaDriveBus::new(&rom, ram, z80, &mut vdp, 0, last, &mut sink);
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
