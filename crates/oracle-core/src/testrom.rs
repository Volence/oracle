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
///
/// Guarded for the same reason as [`short_disp`] (`F-TESTROM-DISP-GUARD`), with a far wider window: an
/// out-of-range delta truncated by `as i16` assembles into a *different valid branch* rather than failing.
fn disp16(target: u32, ext_addr: u32) -> u16 {
    let delta = target as i64 - ext_addr as i64;
    debug_assert!(
        (-32768..=32767).contains(&delta),
        "testrom word branch to {target:#X} from extension word at {ext_addr:#X}: displacement {delta} \
         is outside the signed-word window (-32768..=32767). `as i16` would truncate it into a different \
         VALID branch and the fixture would boot and measure the wrong thing."
    );
    delta as i16 as u16
}

/// The signed 8-bit displacement of a short branch (`BRA.s`/`Bcc.s`) whose **opcode word** sits at byte
/// offset `at` and which must reach `to`. The 68000 adds the displacement to `at + 2`.
///
/// **Why this is a function and not four open-coded casts** (`F-TESTROM-DISP-GUARD`). Truncating an
/// out-of-range delta with `as i8` does not fail to assemble — it silently produces a **different valid
/// branch**, so a builder whose loop body grows past the window yields a fixture that boots, runs, and
/// measures the wrong thing. That failure is invisible at every layer above it: the ROM is well-formed, the
/// emulator is correct, and only the *expectation* is wrong. Every computed short branch in this file
/// routes through here, so the failure mode is a loud debug panic naming both endpoints instead.
///
/// `0` is rejected for a neighbouring reason: `0x6000 | 0` is the **word**-displacement encoding, which
/// consumes the following word as its displacement. Every caller here emits a one-word branch, so a zero
/// displacement would silently execute the next instruction as branch data.
fn short_disp(to: u32, at: u32) -> u8 {
    let delta = to as i64 - (at as i64 + 2);
    debug_assert!(
        (-128..=127).contains(&delta) && delta != 0,
        "testrom short branch at {at:#X} -> {to:#X}: displacement {delta} is outside the signed-byte \
         window (-128..=127, and 0 is the word-displacement encoding). `as i8` would truncate it into a \
         different VALID branch and the fixture would boot and measure the wrong thing — shorten the loop \
         body, or emit a word branch via `disp16`."
    );
    delta as i8 as u8
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

/// Where [`build_pad_log`] leaves what it last read from the controller ports: four bytes, P1 then P2,
/// each port's TH=1 phase followed by its TH=0 phase. Active-low, exactly as the pins read.
///
/// `+0` P1 TH=1 (`C B R L D U`) · `+1` P1 TH=0 (`Start A 0 0 D U`) · `+2` P2 TH=1 · `+3` P2 TH=0.
pub const PAD_LOG_ADDR: u32 = 0x00FF_9000;

/// Build the **pad-log fixture ROM** — a ROM that makes the pad *observable*, for both ports and both TH
/// phases, by writing what it reads to [`PAD_LOG_ADDR`] on every poll.
///
/// [`build_pad_poll`] exposes only **Start**, and only as a backdrop colour. That is enough to prove input
/// reaches the machine and not enough to prove *which* input did: a test asking whether a timeline leaked a
/// held `right` into its frames, or whether an un-driven port was released, cannot see either. This ROM is
/// the instrument for those questions — read the four bytes back with a memory read and compare against
/// the buttons the timeline named.
#[doc(hidden)]
pub fn build_pad_log() -> Vec<u8> {
    let mut rom = vec![0u8; 0x8];
    rom[0..4].copy_from_slice(&INITIAL_SSP.to_be_bytes());
    rom[4..8].copy_from_slice(&MAIN.to_be_bytes());
    rom.resize(0x200, 0);

    fn w(rom: &mut Vec<u8>, word: u16) {
        rom.push((word >> 8) as u8);
        rom.push((word & 0xFF) as u8);
    }
    fn l(rom: &mut Vec<u8>, long: u32) {
        w(rom, (long >> 16) as u16);
        w(rom, (long & 0xFFFF) as u16);
    }
    // a2 = I/O base $A10000. Data regs at +3 (P1) / +5 (P2); control regs at +9 / +$B.
    w(&mut rom, 0x45F9);
    l(&mut rom, 0x00A1_0000);
    for ctrl_off in [0x0009u16, 0x000B] {
        w(&mut rom, 0x157C);
        w(&mut rom, 0x0040); // TH as output
        w(&mut rom, ctrl_off);
    }

    let loop_top = rom.len() as u32;
    for (data_off, log) in [(0x0003u16, PAD_LOG_ADDR), (0x0005, PAD_LOG_ADDR + 2)] {
        for (th, slot) in [(0x0040u16, 0u32), (0x0000, 1)] {
            w(&mut rom, 0x157C);
            w(&mut rom, th);
            w(&mut rom, data_off); // move.b #TH,(off,a2)
            w(&mut rom, 0x102A);
            w(&mut rom, data_off); // move.b (off,a2),d0
            w(&mut rom, 0x13C0);
            l(&mut rom, log + slot); // move.b d0,(log).l
        }
    }
    let bra_at = rom.len() as u32;
    let disp = short_disp(loop_top, bra_at);
    w(&mut rom, 0x6000 | disp as u16); // bra.s loop_top
    rom
}

/// The address of the illegal-instruction handler in every ROM this module builds — the fixture stand-in
/// for an engine's fault handler (Aeon vectors all sixteen TRAPs plus the reserved vectors at a single
/// `ErrorTrap`, and routes `raise_exception` to its MD Debugger blob).
pub const TRAP_HANDLER_ADDR: u32 = ILLEGAL_H;

/// Build a ROM that runs normally and then **faults on a chosen frame** — the positive control for a
/// fault-watching runner.
///
/// Its VInt handler counts frames at [`INT_SENTINEL_ADDR`] and executes an `ILLEGAL` once the count
/// reaches `frame`, so the CPU vectors to [`TRAP_HANDLER_ADDR`] exactly as an engine's `raise_exception`
/// reaches its handler. Without a ROM that really faults, a runner that watches for faults can only ever
/// be tested against ROMs that do not — which proves the watch is *silent*, not that it *works*.
///
/// [`build`] is left byte-identical (the golden fixture depends on it), as [`build_vint_counter`] is.
#[doc(hidden)]
pub fn build_trap_on_frame(frame: u16) -> Vec<u8> {
    let mut rom = build();
    // Zero the counter before interrupts are enabled. Work RAM comes up **seeded, not zeroed**
    // (`System::new` takes a fill seed), so a handler that increments-and-compares without this starts
    // from garbage and may never reach `frame` — which is precisely how the first draft of this fixture
    // produced a runner that reported CLEAN on a ROM built to fault.
    put_long(&mut rom, 0x4, 0x0000_02C0); // reset PC -> the new init stub
    put_word(&mut rom, 0x2C0, 0x4279); // clr.w (xxx).l
    put_long(&mut rom, 0x2C2, INT_SENTINEL_ADDR);
    // And enable the VDP's VInt (reg $01 IE0), which [`build`] never does — its VInt test enables IE0
    // from outside. A fixture that relies on a handler it never arms cannot fault, and this is the second
    // reason the first draft of it reported CLEAN on a ROM built to fault.
    put_word(&mut rom, 0x2C6, 0x33FC); // move.w #imm, (xxx).l
    put_word(&mut rom, 0x2C8, 0x8120); //   reg $01 = $20 (IE0)
    put_long(&mut rom, 0x2CA, 0x00C0_0004); //   VDP control port
    put_word(&mut rom, 0x2CE, 0x4EF9); // jmp (xxx).l
    put_long(&mut rom, 0x2D0, MAIN);
    // INT_H, replacing the constant-sentinel body:
    //   move.w ($00FF8000).l, d0 ; addq.w #1, d0 ; move.w d0, ($00FF8000).l
    //   cmpi.w #frame, d0 ; bne.w rte ; illegal ; rte
    put_word(&mut rom, 0x2A0, 0x3039); // move.w (xxx).l, d0
    put_long(&mut rom, 0x2A2, INT_SENTINEL_ADDR);
    put_word(&mut rom, 0x2A6, 0x5240); // addq.w #1, d0
    put_word(&mut rom, 0x2A8, 0x33C0); // move.w d0, (xxx).l
    put_long(&mut rom, 0x2AA, INT_SENTINEL_ADDR);
    put_word(&mut rom, 0x2AE, 0x0C40); // cmpi.w #imm, d0
    put_word(&mut rom, 0x2B0, frame);
    put_word(&mut rom, 0x2B2, 0x6600); // bne.w <rte>
    put_word(&mut rom, 0x2B4, disp16(0x2B8, 0x2B4));
    put_word(&mut rom, 0x2B6, 0x4AFC); // illegal
    put_word(&mut rom, 0x2B8, 0x4E73); // rte
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
    // The next two words write reg 15 = autoinc 2. NOTE: this ROM never sets reg 1, so it runs in Mode 4
    // (M5 clear) and hardware discards that write — registers above 10 are not writable in Mode 4 (see
    // `Vdp::write_register`). Left as-is deliberately: the fixture does a single word poke whose two bytes
    // land regardless of the autoincrement, and the ROM's byte layout is address-packed, so declaring M5
    // would mean relocating `VRAM_POKE_PC` and every literal offset below for no behavioural gain.
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
        0x8154u16, // reg 1  display + DMA enable + M5 (bit 2): regs 11+ need mode 5
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
    let disp = short_disp(loop_top, bra_at);
    w(&mut rom, 0x6000 | disp as u16); // bra.s loop_top

    rom
}

/// The two backdrop colours [`build_cram_midframe`] alternates between — black and white, the widest
/// contrast the 9-bit CRAM word offers, so a boundary row is unmistakable in a hex dump.
#[doc(hidden)]
pub const CRAM_MIDFRAME_A: u16 = 0x0000;
/// See [`CRAM_MIDFRAME_A`].
#[doc(hidden)]
pub const CRAM_MIDFRAME_B: u16 = 0x0EEE;

/// Build the **mid-frame CRAM fixture ROM** — a ROM that changes the backdrop colour *while the beam is
/// drawing*, at a chosen scanline.
///
/// The scene is [`build_pad_poll`]'s: VRAM zeroed by a fill DMA, so every plane and sprite pixel is
/// transparent and the whole screen is the backdrop (reg 7 = CRAM entry 1). What this one adds is timing.
/// Every frame it
///
/// 1. waits for vblank (V counter ≥ `$E0`) and sets CRAM entry 1 = [`CRAM_MIDFRAME_A`],
/// 2. waits for active display to resume (V < `$E0`, i.e. line 0),
/// 3. polls the HV counter at `$C00008` until the beam reaches `line` (V is the high byte), and
/// 4. sets CRAM entry 1 = [`CRAM_MIDFRAME_B`], then loops.
///
/// So **every completed frame after the first** carries the split — rows above `line` wholly in A, rows
/// below it wholly in B — rather than only the one frame a write-once fixture would mark, which would make
/// the assertion depend on the exact frame count the reader happened to stop at. Frame 0 draws entirely in
/// colour A: the poll begins after the frame-0 vblank arm, so the first B write lands in frame 1. Read at
/// any frame ≥ 1.
///
/// **Row `line` itself is split, and the first *fully* recoloured row is `line + 1`.** The row is resolved
/// at its own line start, before the write; the write then lands part-way across it and recolours it from
/// that pixel on (`F-SCANLINE-SUBLINE`). So the picture is: `..line-1` uniform A, `line` A-then-B with one
/// transition, `line+1..` uniform B. The `+1` claim this fixture has always carried survives, restated as
/// "the first **fully** B row" — and the split row is now the sharper poison, because a line-atomic
/// renderer draws it uniform in one colour or the other and cannot produce a transition at all. The landing
/// column is a function of the poll loop's own instruction timing; `crates/oracle-aether/tests/scanlines.rs`
/// derives the band it must fall in rather than pinning a number.
///
/// Nothing draws over the backdrop, so the content trap the golden fixtures set — a tinted palette entry
/// no pixel samples — cannot bite: the colour *is* the picture. **Two different `line` arguments must
/// yield different rendered rows**; that is the whole point of the fixture, and the acceptance gate for
/// `emulator/scanlines` (CR-24's adoption condition, suite gate (i)) is built on it. A reader that
/// re-renders the frame from end-of-frame VDP state instead of capturing the raster sees only whichever
/// colour was last written, identically for both arguments — which is exactly the blindness the gate exists
/// to catch.
///
/// `line` is best kept above the handful of lines the VDP/VRAM init costs and below 223; step 3 exits at the
/// first line ≥ `line`, so a `line` the machine has already passed fires late rather than hanging.
///
/// [`build`] is left byte-identical (the golden fixture depends on it), as every builder here does.
#[doc(hidden)]
pub fn build_cram_midframe(line: u8) -> Vec<u8> {
    let mut rom = vec![0u8; 0x8];
    rom[0..4].copy_from_slice(&INITIAL_SSP.to_be_bytes()); // reset SSP
    rom[4..8].copy_from_slice(&MAIN.to_be_bytes()); // reset PC = $200
    rom.resize(0x200, 0);

    fn w(rom: &mut Vec<u8>, word: u16) {
        rom.push((word >> 8) as u8);
        rom.push((word & 0xFF) as u8);
    }
    fn l(rom: &mut Vec<u8>, long: u32) {
        w(rom, (long >> 16) as u16);
        w(rom, (long & 0xFFFF) as u16);
    }
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
    /// One "spin until the V counter satisfies a condition" block, 10 bytes: read the HV counter word,
    /// shift V (the high byte) down, compare it against `value`, and branch back on `branch` — `$6500`
    /// (BCS/`blo`, "keep waiting while V is below") or `$6400` (BCC/`bhs`, "while V is at or above").
    fn wait_v(rom: &mut Vec<u8>, value: u8, branch: u16) {
        let top = rom.len() as u32;
        w(rom, 0x3013); // move.w (a3),d0     ; HV counter: V<<8 | H
        w(rom, 0xE048); // lsr.w #8,d0        ; d0.b = V
        w(rom, 0x0C00); // cmpi.b #imm,d0
        w(rom, u16::from(value));
        let at = rom.len() as u32;
        let disp = short_disp(top, at);
        w(rom, branch | u16::from(disp)); // bcs/bcc .top
    }
    // The CRAM entry the backdrop points at, rewritten twice a frame.
    let backdrop_write = |rom: &mut Vec<u8>, colour: u16| {
        cmd(rom, vdp_cmd(0x03, 0x0002)); // CRAM write @ entry 1
        data(rom, colour);
    };

    // a0 = VDP control ($C00004), a1 = VDP data ($C00000), a3 = HV counter ($C00008).
    w(&mut rom, 0x41F9);
    l(&mut rom, 0x00C0_0004);
    w(&mut rom, 0x43F9);
    l(&mut rom, 0x00C0_0000);
    w(&mut rom, 0x47F9);
    l(&mut rom, 0x00C0_0008);

    // VDP registers: display + DMA enable + M5, plane/sprite bases, H32, autoinc 2, backdrop = CRAM 1.
    for word in [
        0x8154u16, // reg 1  display + DMA enable + M5
        0x8230,    // reg 2  plane A $C000
        0x8407,    // reg 4  plane B $E000
        0x8558,    // reg 5  SAT $B000
        0x8701,    // reg 7  backdrop = CRAM entry 1 — the entry the loop rewrites
        0x8B00,    // reg 11 full scroll
        0x8C00,    // reg 12 H32, no shadow/highlight
        0x8D20,    // reg 13 h-scroll table $8000
        0x8F02,    // reg 15 autoinc 2 (CRAM entries are 2 bytes)
        0x9000,    // reg 16 32x32 planes
    ] {
        ctrl(&mut rom, word);
    }

    // CRAM entries 0 and 1: transparent-black, then colour A. Frame 0 draws entirely in A (the loop's first
    // B write lands in frame 1), which is fine — every later frame carries the split.
    cmd(&mut rom, vdp_cmd(0x03, 0x0000));
    data(&mut rom, 0x0000);
    data(&mut rom, CRAM_MIDFRAME_A);

    // Zero VRAM with a fill DMA, exactly as [`build_pad_poll`] does: all plane/sprite pixels transparent, so
    // every visible dot is the backdrop.
    ctrl(&mut rom, 0x8F01); // reg 15 autoinc 1
    ctrl(&mut rom, 0x93FF); // reg 19 fill length low
    ctrl(&mut rom, 0x94FF); // reg 20 fill length high -> $FFFF bytes
    ctrl(&mut rom, 0x9780); // reg 23 DMA fill mode
    cmd(&mut rom, vdp_cmd(0x21, 0x0000)); // VRAM write @ $0000 + CD5
    data(&mut rom, 0x0000); // the data-port write triggers the fill (fill byte $00)

    // The raster loop. Re-arming in vblank is what makes every frame carry the split.
    let outer = rom.len() as u32;
    wait_v(&mut rom, 0xE0, 0x6500); // spin while V < $E0  -> exits in vblank
    backdrop_write(&mut rom, CRAM_MIDFRAME_A);
    wait_v(&mut rom, 0xE0, 0x6400); // spin while V >= $E0 -> exits on line 0
    wait_v(&mut rom, line, 0x6500); // spin while V < line -> exits on the target line
    backdrop_write(&mut rom, CRAM_MIDFRAME_B);
    let bra_at = rom.len() as u32;
    let disp = short_disp(outer, bra_at);
    w(&mut rom, 0x6000 | disp as u16); // bra.s outer

    rom
}

// --- Profiler fixtures (`crates/oracle-core/src/profiler.rs`) --------------------------------------

/// Entry addresses in every [`build_profiler`] image. The profiler keys its rows by routine entry
/// address, so these ARE the expectations its tests assert against — which is why they are constants of
/// the builder rather than numbers copied into a test file.
pub const PROF_MAIN: u32 = 0x0000_0200;
/// The illegal-instruction handler: a self-spin, so a fixture that faults parks visibly instead of
/// running away into whatever follows.
pub const PROF_ILLEGAL: u32 = 0x0000_0280;
/// The level-4 (HBlank) autovector handler.
pub const PROF_HINT_H: u32 = 0x0000_02C0;
/// The level-6 (VBlank) autovector handler.
pub const PROF_VINT_H: u32 = 0x0000_02E0;
/// A leaf routine: two `nop`s and an `rts`. Called, never calls.
pub const PROF_LEAF: u32 = 0x0000_0300;
/// A middle routine that calls [`PROF_LEAF`] exactly [`PROF_MID_CALLS_LEAF`] times, then returns.
pub const PROF_MID: u32 = 0x0000_0340;
/// The `move.l #target,-(sp)` / `rts` **dispatch idiom** — a "return" to a pushed address, which is a
/// jump wearing a return's clothes. Nothing here pushed a frame for the target.
pub const PROF_DISPATCH: u32 = 0x0000_0380;
/// Where [`PROF_DISPATCH`] dispatches to. Its own `rts` unwinds the DISPATCH invocation.
pub const PROF_TARGET: u32 = 0x0000_03C0;
/// A self-recursive routine driven by a counter in `d6`.
pub const PROF_REC: u32 = 0x0000_0400;
/// A routine that provokes a bus stall — see [`ProfilerShape::Stall`]. Its whole purpose is to be the row
/// the stall lands on.
pub const PROF_STALL: u32 = 0x0000_0440;
/// How many times [`PROF_MID`] calls [`PROF_LEAF`] per invocation.
pub const PROF_MID_CALLS_LEAF: u64 = 2;
/// The user-mode stack [`ProfilerShape::ModeSwitch`] installs before dropping privilege — well below the
/// supervisor stack so the two never collide and a frame match cannot succeed by accident.
pub const PROF_USER_SP: u32 = 0x00FF_7FFE;

/// Which fixture [`build_profiler`] emits. Every shape shares one skeleton — vectors, VDP setup, and an
/// outer loop gated on the V counter so the body runs **exactly once per frame** — and differs only in
/// its body. The frame gate is what makes a per-frame expectation a constant rather than "however many
/// passes happened to fit", which is the whole reason these fixtures can be `==`-asserted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfilerShape {
    /// Call [`PROF_LEAF`] `k` times per frame.
    CallsLeaf { k: u16 },
    /// Call [`PROF_MID`] once per frame, which calls [`PROF_LEAF`] twice — a two-level tree with a known
    /// shape, so `self + children == inclusive` has something to be true of.
    TwoLevel,
    /// Call [`PROF_DISPATCH`] once per frame (the W3 regression).
    Dispatch,
    /// Call [`PROF_REC`] once per frame with `d6 = depth`, yielding `depth + 1` nested invocations of one
    /// routine.
    Recursive { depth: u16 },
    /// No body at all — just the frame gate. The interrupts enabled here are the whole content, which is
    /// what makes the bucket assertions unambiguous.
    Interrupts { hint: bool, vint: bool },
    /// Drop to **user mode** and call [`PROF_LEAF`] there with VBlank interrupts live, so a supervisor
    /// exception frame is pushed and popped in the middle of a user-mode routine's lifetime.
    ModeSwitch,
    /// Call [`PROF_LEAF`] once per frame, where the leaf `stop`s until a VBlank interrupt wakes it — so a
    /// long run of `Stopped` idle slices retires while a routine frame is open.
    IdleInRoutine,
    /// Call [`PROF_STALL`] once per frame, where it provokes one of the VDP's bus-hold conditions. The
    /// caller does nothing else, so any stall in the sample can only have come from that routine.
    Stall { kind: StallKind },
}

/// Which bus-hold condition [`ProfilerShape::Stall`] provokes.
///
/// The first is the one that matters most to a consumer (it is the only mechanism that halts the 68000 for
/// a long, measurable window); the other two are the controls that pin the boundary of what counts. A VRAM
/// fill and a VRAM copy leave the 68000 running, so they must contribute **nothing** — and they do so
/// because `run_pending_dma` returns `0` for them, not because anything filters them out afterwards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StallKind {
    /// A 68k->VDP DMA: the bus is held for the whole transfer.
    Dma,
    /// A VRAM fill — the 68000 keeps running, so this must cost no stall at all.
    Fill,
    /// A VRAM copy — likewise.
    Copy,
}

/// The V-counter value at which vblank starts (line 224 in the 224-line mode these fixtures run in).
const PROF_VBLANK_LINE: u8 = 0xE0;

/// Image size for [`build_profiler`]: enough for the vectors, the handler block and the routines.
const PROF_ROM_LEN: usize = 0x500;

/// Append a big-endian word.
fn pw(rom: &mut Vec<u8>, word: u16) {
    rom.push((word >> 8) as u8);
    rom.push((word & 0xFF) as u8);
}

/// Append a big-endian long.
fn pl(rom: &mut Vec<u8>, long: u32) {
    pw(rom, (long >> 16) as u16);
    pw(rom, (long & 0xFFFF) as u16);
}

/// Emit "spin until the V counter satisfies a condition", the [`build_cram_midframe`] idiom, reading the
/// HV counter through `a3`. `branch` is `$6500` (BCS — keep waiting while V is below `value`) or `$6400`
/// (BCC — while V is at or above it).
///
/// Deliberately a second copy of `build_cram_midframe`'s nested helper rather than a hoist of it: that
/// builder backs the scanline goldens, and moving code out of it to share with a new fixture would put a
/// frozen currency at risk to save a few bytes of duplication.
fn prof_wait_v(rom: &mut Vec<u8>, value: u8, branch: u16) {
    let top = rom.len() as u32;
    pw(rom, 0x3013); // move.w (a3),d0   ; HV counter: V<<8 | H
    pw(rom, 0xE048); // lsr.w #8,d0      ; d0.b = V
    pw(rom, 0x0C00); // cmpi.b #imm,d0
    pw(rom, u16::from(value));
    let at = rom.len() as u32;
    pw(rom, branch | u16::from(short_disp(top, at)));
}

/// `jsr (addr).w` — a two-word absolute-short call. Short-form absolute is a **control** addressing mode,
/// so this is a `JSR` the decoder's own control-flow classifier admits.
fn prof_jsr(rom: &mut Vec<u8>, addr: u32) {
    debug_assert!(
        addr < 0x8000,
        "jsr (addr).w sign-extends its operand: {addr:#X} would not address itself"
    );
    pw(rom, 0x4EB8);
    pw(rom, addr as u16);
}

/// Build one profiler fixture. See [`ProfilerShape`] for what each one is for.
#[doc(hidden)]
pub fn build_profiler(shape: ProfilerShape) -> Vec<u8> {
    let mut rom = vec![0u8; PROF_ROM_LEN];

    // --- Vectors ---
    put_long(&mut rom, 0x0, INITIAL_SSP);
    put_long(&mut rom, 0x4, PROF_MAIN);
    put_long(&mut rom, 0x10, PROF_ILLEGAL); // vector 4  (illegal instruction)
    put_long(&mut rom, 0x78, PROF_VINT_H); // vector 30 (autovector, level 6 / VInt)
                                           // **When BOTH interrupts are enabled, both vectors point at ONE handler.** That is the sharpest form
                                           // of the conflation regression: an accountant that keys an interrupt by where its handler lives sees a
                                           // single address and *cannot* split HBlank from VBlank, however carefully it compares. Only keying by
                                           // the acknowledged cause can pass. With one of them enabled the handlers stay distinct, so the
                                           // single-cause fixtures still say something about addresses too.
    let shared_handler = matches!(
        shape,
        ProfilerShape::Interrupts {
            hint: true,
            vint: true
        }
    );
    put_long(
        &mut rom,
        0x70, // vector 28 (autovector, level 4 / HInt)
        if shared_handler {
            PROF_VINT_H
        } else {
            PROF_HINT_H
        },
    );

    // --- The fixed-address routines, placed by absolute offset so the tests can name them ---
    // LEAF: two nops then rts. `IdleInRoutine` replaces the nops with a `stop`.
    if shape == ProfilerShape::IdleInRoutine {
        put_word(&mut rom, PROF_LEAF, 0x4E72); // stop #imm
        put_word(&mut rom, PROF_LEAF + 2, 0x2000); //   #$2000 — supervisor, mask 0, so a VInt wakes it
    } else {
        put_word(&mut rom, PROF_LEAF, 0x4E71); // nop
        put_word(&mut rom, PROF_LEAF + 2, 0x4E71); // nop
    }
    put_word(&mut rom, PROF_LEAF + 4, 0x4E75); // rts

    // MID: call LEAF twice, then return. The two-level tree.
    put_word(&mut rom, PROF_MID, 0x4EB8);
    put_word(&mut rom, PROF_MID + 2, PROF_LEAF as u16);
    put_word(&mut rom, PROF_MID + 4, 0x4EB8);
    put_word(&mut rom, PROF_MID + 6, PROF_LEAF as u16);
    put_word(&mut rom, PROF_MID + 8, 0x4E75); // rts

    // DISPATCH: push a target address and "return" to it. The stack pointer this leaves is the whole
    // point of the W3 regression — it does NOT match the frame DISPATCH was entered on.
    put_word(&mut rom, PROF_DISPATCH, 0x2F3C); // move.l #imm,-(sp)
    put_long(&mut rom, PROF_DISPATCH + 2, PROF_TARGET);
    put_word(&mut rom, PROF_DISPATCH + 6, 0x4E75); // rts  -> jumps to PROF_TARGET

    // TARGET: calls LEAF, then its rts unwinds the ORIGINAL return address and closes the DISPATCH
    // invocation. The call to LEAF is what makes the W3 regression *observable*: the leaf's cost can only
    // land in DISPATCH's child time if DISPATCH is still open when the target runs, which is exactly what
    // a return matched loosely rather than exactly would have destroyed.
    put_word(&mut rom, PROF_TARGET, 0x4EB8); // jsr (LEAF).w
    put_word(&mut rom, PROF_TARGET + 2, PROF_LEAF as u16);
    put_word(&mut rom, PROF_TARGET + 4, 0x4E75); // rts

    // REC: `if d6 == 0 return; d6 -= 1; REC(); return` — one routine, `depth + 1` invocations.
    put_word(&mut rom, PROF_REC, 0x4A46); // tst.w d6
    put_word(&mut rom, PROF_REC + 2, 0x6700); // beq.w .done
    put_word(&mut rom, PROF_REC + 4, disp16(PROF_REC + 12, PROF_REC + 4));
    put_word(&mut rom, PROF_REC + 6, 0x5346); // subq.w #1,d6
    put_word(&mut rom, PROF_REC + 8, 0x4EB8); // jsr (REC).w
    put_word(&mut rom, PROF_REC + 10, PROF_REC as u16);
    put_word(&mut rom, PROF_REC + 12, 0x4E75); // .done: rts

    // STALL: program and trigger one VDP transfer through the control port in a1, then return. The word
    // sequences are the ones `bus.rs`'s own DMA/fill/copy tests drive, so the trigger is the same trigger.
    if let ProfilerShape::Stall { kind } = shape {
        // Source for the Mem DMA: 68k word address of $000400 (inside this ROM). Length in WORDS.
        const DMA_SRC_WORD: u32 = 0x0000_0400 >> 1;
        const DMA_LEN_WORDS: u16 = 64;
        let setup: &[u16] = match kind {
            StallKind::Dma => &[
                0x8114, // reg 1: DMA enable (bit4) + mode5, display off
                0x8F02, // reg 15: autoinc 2
                0x9300 | (DMA_LEN_WORDS & 0xFF),
                0x9400 | (DMA_LEN_WORDS >> 8),
                0x9500 | (DMA_SRC_WORD as u16 & 0xFF),
                0x9600 | ((DMA_SRC_WORD >> 8) as u16 & 0xFF),
                0x9700 | ((DMA_SRC_WORD >> 16) as u16 & 0x7F), // bit7 = 0 -> Mem mode
                0x4000, // VRAM-write command word 1, dest $0000
                0x0080, // word 2: CD5 set -> code $21, the trigger
            ],
            StallKind::Fill => &[
                0x8114,
                0x8F01,      // autoinc 1
                0x9300 | 64, // reg 19: length low
                0x9400,      // reg 20: length high
                0x9780,      // reg 23: bits 7-6 = 10 -> VRAM fill
                0x4000,
                0x0080, // VRAM write @ $0000 with CD5 -> arms the fill
            ],
            StallKind::Copy => &[
                0x8114,
                0x8F01,
                0x9300 | 64,
                0x9400,
                0x9500,
                0x9600, // source $0000
                0x97C0, // reg 23: bits 7-6 = 11 -> VRAM copy
                0x4000,
                0x00C0, // CD5+CD4 -> the copy trigger
            ],
        };
        let mut at = PROF_STALL;
        for &word in setup {
            put_word(&mut rom, at, 0x33FC); // move.w #imm,(xxx).l
            put_word(&mut rom, at + 2, word);
            put_long(&mut rom, at + 4, 0x00C0_0004); //   the VDP control port
            at += 8;
        }
        // A fill is armed by a control write but TRIGGERED by a data-port write.
        if kind == StallKind::Fill {
            put_word(&mut rom, at, 0x33FC);
            put_word(&mut rom, at + 2, 0x7700); // the fill byte, in the high half
            put_long(&mut rom, at + 4, 0x00C0_0000); //   the VDP data port
            at += 8;
        }
        assert!(
            at + 2 <= PROF_ROM_LEN as u32,
            "the stall routine overran the image"
        );
        put_word(&mut rom, at, 0x4E75); // rts
    }

    // ILLEGAL: park.
    put_word(&mut rom, PROF_ILLEGAL, 0x60FE); // bra.s *

    // Both handlers are a bare `rte`: the fc=7 acknowledge already cleared the pending latch, so nothing
    // else is needed to keep the machine running — and a handler that does nothing else keeps the bucket
    // totals unambiguous.
    put_word(&mut rom, PROF_HINT_H, 0x4E73); // rte
    put_word(&mut rom, PROF_VINT_H, 0x4E73); // rte

    // --- main ---
    let (hint_on, vint_on) = match shape {
        ProfilerShape::Interrupts { hint, vint } => (hint, vint),
        ProfilerShape::ModeSwitch | ProfilerShape::IdleInRoutine => (false, true),
        _ => (false, false),
    };
    let mut code: Vec<u8> = Vec::new();
    // Supervisor. Interrupt mask 0 when anything is enabled (so both levels can be taken), else 7.
    pw(&mut code, 0x46FC); // move.w #imm,SR
    pw(&mut code, if hint_on || vint_on { 0x2000 } else { 0x2700 });
    // a0 = VDP control ($C00004), a3 = HV counter ($C00008).
    pw(&mut code, 0x41F9);
    pl(&mut code, 0x00C0_0004);
    pw(&mut code, 0x47F9);
    pl(&mut code, 0x00C0_0008);
    // VDP registers, written through a0. Reg 1 bit 6 = display on, bit 5 = IE0 (VInt enable);
    // reg 0 bit 4 = IE1 (HInt enable); reg 10 = the HInt line-counter reload (0 = every line).
    for (reg, val) in [
        (0u8, if hint_on { 0x14u8 } else { 0x04 }),
        (1, if vint_on { 0x64 } else { 0x44 }),
        (10, 0x00),
    ] {
        pw(&mut code, 0x30BC); // move.w #imm,(a0)
        pw(&mut code, 0x8000 | (u16::from(reg) << 8) | u16::from(val));
    }
    // ModeSwitch installs a user stack and drops privilege before entering the loop, so every call the
    // body makes is a USER-mode call whose frames live on the user stack while the interrupt frames that
    // interleave with them live on the supervisor stack.
    if shape == ProfilerShape::ModeSwitch {
        pw(&mut code, 0x207C); // move.l #imm,a0   (a0 is free again; the VDP writes are done)
        pl(&mut code, PROF_USER_SP);
        pw(&mut code, 0x4E60); // move.l a0,usp
        pw(&mut code, 0x46FC); // move.w #imm,SR
        pw(&mut code, 0x0000); //   user mode, mask 0 — a VInt is still taken, now switching modes to do it
    }

    // --- outer: one pass per frame ---
    let outer = PROF_MAIN + code.len() as u32;
    // Into vblank, then out of it: the pass therefore begins at the top of a fresh frame and ends well
    // before that frame's line-224 boundary, so the body lands wholly inside one counted frame.
    prof_wait_v(&mut code, PROF_VBLANK_LINE, 0x6500); // spin while V < $E0  -> exits in vblank
    prof_wait_v(&mut code, PROF_VBLANK_LINE, 0x6400); // spin while V >= $E0 -> exits on line 0
    match shape {
        ProfilerShape::CallsLeaf { k } => {
            debug_assert!(k >= 1, "a zero-call fixture proves nothing");
            pw(&mut code, 0x3E3C); // move.w #imm,d7
            pw(&mut code, k - 1); //   dbra runs count+1 times
            let loop_top = PROF_MAIN + code.len() as u32;
            prof_jsr(&mut code, PROF_LEAF);
            pw(&mut code, 0x51CF); // dbra d7,.loop
            let ext = PROF_MAIN + code.len() as u32;
            pw(&mut code, disp16(loop_top, ext));
        }
        ProfilerShape::TwoLevel => prof_jsr(&mut code, PROF_MID),
        ProfilerShape::Dispatch => prof_jsr(&mut code, PROF_DISPATCH),
        ProfilerShape::Recursive { depth } => {
            pw(&mut code, 0x3C3C); // move.w #imm,d6
            pw(&mut code, depth);
            prof_jsr(&mut code, PROF_REC);
        }
        ProfilerShape::ModeSwitch | ProfilerShape::IdleInRoutine => prof_jsr(&mut code, PROF_LEAF),
        ProfilerShape::Stall { .. } => prof_jsr(&mut code, PROF_STALL),
        ProfilerShape::Interrupts { .. } => {}
    }
    // bra.w outer — the word form, because a body can be longer than a short branch reaches.
    pw(&mut code, 0x6000);
    let ext = PROF_MAIN + code.len() as u32;
    pw(&mut code, disp16(outer, ext));

    assert!(
        PROF_MAIN as usize + code.len() <= PROF_ILLEGAL as usize,
        "profiler fixture main ({} bytes) ran into the handler block at {PROF_ILLEGAL:#X}",
        code.len()
    );
    rom[PROF_MAIN as usize..PROF_MAIN as usize + code.len()].copy_from_slice(&code);
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
            let mut z80_bank = 0u16;
            let mut sram_enabled = false;
            let mut sram_write_protect = false;
            let mut sram: Vec<u8> = Vec::new();
            let mut sram_dirty = false;
            let mut sram_used = false;
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
                &mut z80_bank,
                &mut sram_enabled,
                &mut sram_write_protect,
                &mut sram,
                &mut sram_dirty,
                &mut sram_used,
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

    /// The positive control for a fault-watching runner: a ROM built to fault really does reach the
    /// handler, and the stock ROM does not.
    ///
    /// Both halves are load-bearing. The negative half alone is what the first draft of this fixture
    /// passed — twice — while the ROM it claimed would fault ran cleanly to the bound, because work RAM
    /// comes up seeded (so the counter started from garbage) and because [`build`] never enables the
    /// VDP's VInt (its own VInt test arms IE0 from outside). A watch that has never been seen to fire
    /// has not been shown to work.
    #[test]
    fn a_rom_built_to_fault_reaches_the_handler_and_the_stock_rom_does_not() {
        use crate::system::System;

        let mut s = System::new(0x5EED);
        s.load_rom(build_trap_on_frame(3));
        s.reset();
        let stop = s.run_until_stop(60, |pc, _| pc == TRAP_HANDLER_ADDR);
        assert!(stop.fired(), "the trap fixture must reach the handler");
        assert_eq!(stop.pc, TRAP_HANDLER_ADDR, "stopped ON the handler");
        assert!(
            stop.frame < 10,
            "it must fault early (3rd VInt), not merely somewhere in 60 frames; got frame {}",
            stop.frame
        );
        assert_eq!(
            s.cpu_regs().d[0],
            3,
            "d0 carries the frame counter that tripped it — the register a real handler would report"
        );

        let mut s = System::new(0x5EED);
        s.load_rom(build());
        s.reset();
        let stop = s.run_until_stop(60, |pc, _| pc == TRAP_HANDLER_ADDR);
        assert!(
            !stop.fired(),
            "the stock ROM must NOT reach the handler — otherwise the positive half proves nothing"
        );
    }

    // --- F-TESTROM-DISP-GUARD -----------------------------------------------------------------------

    /// The positive control: an in-range backward branch still assembles to the byte the 68000 wants, so
    /// the guard has not changed what any existing fixture emits. `bra.s` at `at` reaching `to` encodes
    /// `to - (at + 2)`; the two ends of the legal window are checked alongside a representative middle.
    #[test]
    fn short_disp_encodes_the_window_it_admits() {
        // A backward branch of -2 (the `bra.s *` self-spin) and the extreme reachable ends.
        assert_eq!(short_disp(0x100, 0x100), 0xFE, "-2 -> 0xFE");
        assert_eq!(
            short_disp(0x082, 0x100),
            0x80,
            "-128, the far end backwards"
        );
        assert_eq!(
            short_disp(0x17F, 0x100),
            0x7D,
            "a forward branch inside the window"
        );
        assert_eq!(short_disp(0x181, 0x100), 0x7F, "+127, the far end forwards");
    }

    /// **The guard fires.** A loop body grown past the signed-byte window is the failure this exists for,
    /// and without the `debug_assert` it is silent: `as i8` turns -129 into +127, which is a perfectly
    /// valid branch to a perfectly wrong place. Proven red-first — the assertion below fails if the guard
    /// is removed, because the truncation would simply return a byte.
    #[test]
    #[should_panic(expected = "outside the signed-byte window")]
    #[cfg(debug_assertions)]
    fn short_disp_rejects_a_body_grown_past_the_window() {
        // -131 (`0x100 - (0x181 + 2)`): past the window. `as i8` would yield 0x7D — a +125 FORWARD
        // branch, which is a perfectly valid instruction to a completely wrong place.
        short_disp(0x100, 0x181);
    }

    /// Zero is rejected too: `0x6000 | 0` is the word-displacement encoding, so a caller emitting a
    /// one-word branch would have the following instruction eaten as branch data.
    #[test]
    #[should_panic(expected = "word-displacement encoding")]
    #[cfg(debug_assertions)]
    fn short_disp_rejects_the_zero_that_means_word_displacement() {
        short_disp(0x102, 0x100);
    }

    /// The word-branch helper carries the same guard, with the wider window. `disp16` is measured from the
    /// EXTENSION word, not the opcode, which is why the in-range case here has no `+2`.
    #[test]
    fn disp16_encodes_and_guards_its_own_window() {
        assert_eq!(
            disp16(0x0204, 0x021A),
            0xFFEA,
            "the stock ROM's own bra.w reload"
        );
        assert_eq!(disp16(0x7FFF, 0x0000), 0x7FFF, "+32767, the far end");
        assert_eq!(disp16(0x0000, 0x8000), 0x8000, "-32768, the other far end");
    }

    #[test]
    #[should_panic(expected = "outside the signed-word window")]
    #[cfg(debug_assertions)]
    fn disp16_rejects_a_target_past_its_window() {
        disp16(0x8000, 0x0000); // +32768: `as i16` would yield -32768, a branch BACKWARDS
    }
}
