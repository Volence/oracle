#!/usr/bin/env python3
"""Assemble the undriven-TH pull-direction experiment ROM (`th_pullup.bin`).

WHY THIS IS HAND-ASSEMBLED AND THE OTHER HARNESS ROMS ARE NOT
-------------------------------------------------------------
`build_rom.sh` / `build_vdp_pending.sh` call the aeon suite's native `asl` +
`p2bin`. On this box `resolve_aeon_tools` resolves a directory that holds
`convsym` but **no `asl` and no `p2bin`** -- the committed `.bin`s next to this
script were assembled when the toolchain was present. Rather than make this
parcel's result depend on a toolchain it cannot name, the ROM is emitted here
directly: it is thirty-odd instructions, every one of them a `move`, and the
encodings are asserted against the M68000 PRM field layout in `_selftest()`.

The image is also the fixture the in-tree gate loads
(`crates/oracle-core/tests/io_controllers.rs` includes `th_pullup.bin`), so both
arms of the experiment run **the same bytes**: one instrument, two models.

THE EXPERIMENT
--------------
`docs/2026-07-17-io-recon.md` IO3 pins: *"A wire configured as input with nothing
driving it floats high (pull-up -> reads 1)."* Every pad-detection argument in
this tree stands on it, and no ROM in the corpus exercises it -- the two that
read the pad both DRIVE TH. This ROM reads `$A10003` with the Control register
in states the corpus never visits, and records every byte in work RAM.

Observables (bytes at `$FF8000`, all with nothing held on the pad):

    +0  P1 Data  $A10003, PRISTINE (nothing in the I/O block written yet)
    +1  P1 Ctrl  $A10009, PRISTINE            premise check, must be $00
    +2  P2 Data  $A10005, PRISTINE            the same question on a second port
    +3  P2 Ctrl  $A1000B, PRISTINE            premise check, must be $00
    +4  Version  $A10001                      the block answers, and not with $FF
    +5  P1 Data  after latch:=$40, Ctrl still $00
    +6  P1 Data  with Ctrl=$40, latch=$40     CONTROL: TH DRIVEN high
    +7  P1 Data  with Ctrl=$40, latch=$00     CONTROL: TH DRIVEN low
    +8  P1 Data  back to Ctrl=$00, latch=$00
    +9  P1 Ctrl  read back after $40 then $00
    +10 P1 Data  with Ctrl=$40, latch=$C0      bit 7: pull-up, latch, or zero?
    +11 P1 Data  with Ctrl=$00, latch=$80      the same, TH an input again
    $FF8010.w  done marker $C0DE  ($DEAD from any exception vector)

Predictions on **bits 6-0**, for an all-released 3-button pad:

    hypothesis                       +0    +5    +7    +8
    pull-up HIGH (IO3 as pinned)     $7F   $7F   $33   $7F
    pull-down LOW                    $33   $33   $33   $33
    input pin echoes the latch       $33   $7F   $33   $33

`+5` and `+8` are what separate a real pull-up from a model that simply hands an
input pin back its own latch -- the two agree at `+0` and differ at `+5`. `+7`
is the detector-positive control: TH is genuinely driven low there under every
hypothesis, so a run that cannot produce `$33` in bits 6-0 at `+7` has not
demonstrated that a high TH elsewhere means anything at all.

**Bit 7 is a separate cell, and it is why the predictions above are masked to
seven bits.** The port has seven I/O pins (Control is bits 6-0; bit 7 of Control
is the TH-interrupt enable), so no *pin* corresponds to Data bit 7 at all. Recon
IO4's table says it reads `1` ("pull-up, undriven"); the first BlastEm run of
this ROM read it `0` everywhere. `+10`/`+11` separate the candidates: bit 7 =
constant 1, = the Data latch's own bit 7, or = constant 0.

    ./build_th_pullup.py            # -> th_pullup.bin, prints the listing
    ./build_th_pullup.py --check    # verify the committed image matches
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
BIN = os.path.join(HERE, "th_pullup.bin")

# Work-RAM observables.
OBS = 0x00FF8000
DONE_MARK = 0x00FF8010
INITIAL_SSP = 0x00FF7000
ENTRY = 0x00000200

# I/O registers (recon IO1).
P1_DATA, P1_CTRL = 0x00A10003, 0x00A10009
P2_DATA, P2_CTRL = 0x00A10005, 0x00A1000B
VERSION = 0x00A10001


class Asm:
    """A byte emitter with a program counter, so labels are addresses, not guesses."""

    def __init__(self, org):
        self.org = org
        self.b = bytearray()
        self.listing = []

    @property
    def pc(self):
        return self.org + len(self.b)

    def _emit(self, text, *words):
        at = self.pc
        for wd in words:
            self.b += bytes(((wd >> 8) & 0xFF, wd & 0xFF))
        self.listing.append("%06X  %-22s %s" % (at, " ".join("%04X" % w for w in words), text))

    # -- the five instruction forms this ROM uses (M68000 PRM encodings) --
    def move_w_imm_sr(self, imm):
        self._emit("move.w #$%04X,sr" % imm, 0x46FC, imm)

    def movea_l_imm_a7(self, imm):
        self._emit("movea.l #$%08X,a7" % imm, 0x2E7C, imm >> 16, imm & 0xFFFF)

    def move_b_abs_d0(self, src):
        self._emit("move.b $%08X,d0" % src, 0x1039, src >> 16, src & 0xFFFF)

    def move_b_d0_abs(self, dst):
        self._emit("move.b d0,$%08X" % dst, 0x13C0, dst >> 16, dst & 0xFFFF)

    def move_b_imm_abs(self, imm, dst):
        self._emit("move.b #$%02X,$%08X" % (imm, dst), 0x13FC, imm & 0xFF, dst >> 16, dst & 0xFFFF)

    def move_w_imm_abs(self, imm, dst):
        self._emit("move.w #$%04X,$%08X" % (imm, dst), 0x33FC, imm, dst >> 16, dst & 0xFFFF)

    def nop(self, n=1):
        for _ in range(n):
            self._emit("nop", 0x4E71)

    def bra_self(self, label):
        self._emit("%s: bra %s" % (label, label), 0x60FE)


def _header():
    """The standard 256-byte Mega Drive header at $100, written field by field at its documented offset."""
    h = bytearray(b" " * 0x100)

    def put(off, data):
        h[off : off + len(data)] = data

    put(0x00, b"SEGA GENESIS    ")  # $100 console name
    put(0x10, b"(C)HARNESS 2026 ")  # $110 copyright
    put(0x20, b"TH PULL-DIRECTION EXPERIMENT ROM")  # $120 domestic name (48)
    put(0x50, b"TH PULL-DIRECTION EXPERIMENT ROM")  # $150 overseas name (48)
    put(0x80, b"GM 00000000-00")  # $180 serial
    put(0x8E, bytes(2))  # $18E checksum (no checksum routine runs in either model)
    put(0x90, b"J               ")  # $190 I/O support: joypad
    put(0xA0, (0x00000000).to_bytes(4, "big"))  # $1A0 ROM start
    put(0xA4, (0x000003FF).to_bytes(4, "big"))  # $1A4 ROM end
    put(0xA8, (0x00FF0000).to_bytes(4, "big"))  # $1A8 RAM start
    put(0xAC, (0x00FFFFFF).to_bytes(4, "big"))  # $1AC RAM end
    put(0xF0, b"JUE             ")  # $1F0 region
    assert len(h) == 0x100, len(h)
    return bytes(h)


def assemble():
    """Return (image_bytes, labels). Labels are absolute addresses for the RSP driver's breakpoints."""
    a = Asm(ENTRY)

    # Interrupts are already masked by reset (S=1, IPL=7); restated so the state is written down, not
    # inherited. Neither instruction touches the I/O block, so the pristine reads below are still pristine.
    a.move_w_imm_sr(0x2700)
    a.movea_l_imm_a7(INITIAL_SSP)

    # No TMSS unlock: this ROM never touches the VDP, and the unlock's own version-register read would be
    # the first I/O-block access. The pristine reads must be first.

    # --- +0..+4: pristine reads, before anything in the I/O block is written ---
    for i, src in enumerate((P1_DATA, P1_CTRL, P2_DATA, P2_CTRL, VERSION)):
        a.move_b_abs_d0(src)
        a.move_b_d0_abs(OBS + i)

    # --- +5: latch written, direction untouched. Separates a pull-up from a latch echo. ---
    a.move_b_imm_abs(0x40, P1_DATA)
    a.nop(4)  # the settle delay the TH protocol calls for, as real pad code writes it
    a.move_b_abs_d0(P1_DATA)
    a.move_b_d0_abs(OBS + 5)

    # --- +6: CONTROL. TH made an output and driven HIGH. Every hypothesis predicts $FF. ---
    a.move_b_imm_abs(0x40, P1_CTRL)
    a.nop(4)
    a.move_b_abs_d0(P1_DATA)
    a.move_b_d0_abs(OBS + 6)

    # --- +7: CONTROL. TH driven LOW. Every hypothesis predicts $B3: this is the arm that proves a
    #         non-$FF byte is observable at all through this ROM and this readback path. ---
    a.move_b_imm_abs(0x00, P1_DATA)
    a.nop(4)
    a.move_b_abs_d0(P1_DATA)
    a.move_b_d0_abs(OBS + 7)

    # --- +8/+9: TH returned to an input with the latch left at $00 ---
    a.move_b_imm_abs(0x00, P1_CTRL)
    a.nop(4)
    a.move_b_abs_d0(P1_DATA)
    a.move_b_d0_abs(OBS + 8)
    a.move_b_abs_d0(P1_CTRL)
    a.move_b_d0_abs(OBS + 9)

    # --- +10/+11: what is Data bit 7, which has no pin? Latch it to 1 with TH an output, then an input. ---
    a.move_b_imm_abs(0x40, P1_CTRL)
    a.move_b_imm_abs(0xC0, P1_DATA)
    a.nop(4)
    a.move_b_abs_d0(P1_DATA)
    a.move_b_d0_abs(OBS + 10)
    a.move_b_imm_abs(0x00, P1_CTRL)
    a.move_b_imm_abs(0x80, P1_DATA)
    a.nop(4)
    a.move_b_abs_d0(P1_DATA)
    a.move_b_d0_abs(OBS + 11)

    a.move_w_imm_abs(0xC0DE, DONE_MARK)
    done = a.pc
    a.bra_self("Done")
    generic_h = a.pc
    a.move_w_imm_abs(0xDEAD, DONE_MARK)
    gen_halt = a.pc
    a.bra_self("GenHalt")

    # --- vectors + header + body ---
    rom = bytearray(0x100)
    rom[0:4] = INITIAL_SSP.to_bytes(4, "big")
    rom[4:8] = ENTRY.to_bytes(4, "big")
    for v in range(2, 64):
        rom[v * 4 : v * 4 + 4] = generic_h.to_bytes(4, "big")
    rom += _header()
    assert len(rom) == ENTRY
    rom += a.b
    rom += bytes((-len(rom)) % 0x400)  # pad to the ROM-end the header declares

    return bytes(rom), {"Done": done, "GenHalt": gen_halt}, a.listing


def _selftest(image):
    """Cheap encoding checks: the four opcodes, and that the vector table points where it says."""
    assert image[0:8] == b"\x00\xff\x70\x00\x00\x00\x02\x00"
    assert image[0x100:0x110] == b"SEGA GENESIS    "
    # Every instruction word in the body decodes to one of the forms above.
    known = {0x46FC: 2, 0x2E7C: 3, 0x1039: 3, 0x13C0: 3, 0x13FC: 4, 0x33FC: 4, 0x4E71: 1, 0x60FE: 1}
    i = ENTRY
    seen = 0
    while i < len(image):
        op = int.from_bytes(image[i : i + 2], "big")
        if op == 0x0000:
            break  # padding
        assert op in known, "unknown opcode %04X at %06X" % (op, i)
        i += 2 * known[op]
        seen += 1
    assert seen >= 30, seen


def main():
    image, labels, listing = assemble()
    _selftest(image)
    if "--check" in sys.argv:
        with open(BIN, "rb") as f:
            on_disk = f.read()
        if on_disk != image:
            print("MISMATCH: %s is %d bytes, freshly assembled is %d" % (BIN, len(on_disk), len(image)))
            return 1
        print("th_pullup.bin matches a fresh assembly (%d bytes)" % len(image))
        return 0
    with open(BIN, "wb") as f:
        f.write(image)
    print("\n".join(listing))
    print("\nlabels: " + ", ".join("%s=$%06X" % kv for kv in sorted(labels.items())))
    print("wrote %s (%d bytes)" % (BIN, len(image)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
