; ---------------------------------------------------------------------------
; VDP DMA fill + data-port-read experiment ROM (clean-room, authored here).
;
; Two cells for the DMA + FIFO push (VDP recon R4(b) / R1 open remainders),
; driven by BlastEm's GDB-remote stub exactly like vdp_pending.asm.
;
; Cell 0 (VRAM fill baseline, R4(b)): program a VRAM fill DMA of 8 bytes at
;   $0400 with fill value $EE (data word $EEEE -> top byte for VRAM), then read
;   VRAM $0400/$0402 back. Confirms the fill writes the top byte to consecutive
;   addresses (autoincrement 1) and that DMA-enable (reg1 bit4) gates it. This
;   is the concrete behavior slice F implements — pinned against hardware here.
;
; Cell 1 (data-port READ while a WRITE command is armed, R1 open cell): arm the
;   FIRST word of a VRAM-WRITE command @ $0200, then perform a data-port READ.
;   Recon R1 pins this configuration as a hardware LOCKUP ("setup a write then
;   read -> the 68K hangs"). BlastEm's response (hang = watchdog timeout, or a
;   returned value) is recorded by the driver — a low-exposure cell the recon
;   flags as bracketed by the lockup; we record honestly, do not force a pin.
;
; Control block (driver writes $FF9000.b before `c`):
;   0 -> cell 0 (VRAM fill baseline)
;   1 -> cell 1 (data-read while write-armed; may hang = lockup)
;
; Observable RAM:
;   $FF8000.w VRAM[$0400]  (fill readback; expect $EEEE for cell 0)
;   $FF8002.w VRAM[$0402]  (fill readback; expect $EEEE for cell 0)
;   $FF8004.w VRAM[$0406]  (fill tail; expect $EEEE if all 8 bytes filled)
;   $FF8008.w cell-1 data-read value (if it did not hang)
;   $FF8010.w done marker $C0DE  (absent => the machine hung, i.e. lockup)
; ---------------------------------------------------------------------------
        cpu     68000
        padding off
        org     $000000

; ---- Exception vector table (64 vectors) ----
        dc.l    $00FF0000               ; 0  initial SSP
        dc.l    Init                    ; 1  reset PC
        rept    62
        dc.l    GenericH                ; 2..63
        endm

; ---- Minimal Mega Drive header at $100 ----
        org     $000100
        dc.b    "SEGA GENESIS    "
        dc.b    "(C)HARNESS 2026.JUL "
        dc.b    "VDP DMA FILL EXPERIMENT ROM                       "
        dc.b    "VDP DMA FILL EXPERIMENT ROM                       "
        dc.b    "GM 00000000-00"
        dc.w    $0000
        dc.b    "J               "
        dc.l    $00000000
        dc.l    $0003FFFF
        dc.l    $00FF0000
        dc.l    $00FFFFFF
        dc.b    "                        "
        dc.b    "                        "
        dc.b    "JUE             "

; ---- Entry ----
        org     $000200
Init:
        movea.l #$00FF7000,a7           ; SSP in work RAM
        move.w  #$2700,sr               ; supervisor, all interrupts masked

        ; TMSS unlock (harmless on pre-TMSS models)
        move.b  $00A10001,d0
        andi.b  #$0F,d0
        beq.s   NoTmss
        move.l  #$53454741,$00A14000    ; 'SEGA'
NoTmss:
        movea.l #$00C00004,a5           ; VDP control port
        movea.l #$00C00000,a4           ; VDP data port

        ; VDP init: mode 5, display off, DMA ENABLE (reg1 bit4), autoincrement 1
        move.w  #$8004,(a5)             ; reg 0: no HINT, no HV latch
        move.w  #$8114,(a5)             ; reg 1: mode5 (bit2) + DMA enable (bit4), display off
        move.w  #$8F01,(a5)             ; reg 15: autoincrement 1 (fill consecutive bytes)

        move.b  $00FF9000,d1
        cmpi.b  #1,d1
        beq     Cell1

; ================= Cell 0: VRAM fill baseline =================
        ; DMA length = 8 (reg19 low = 8, reg20 high = 0)
        move.w  #$9308,(a5)             ; reg 19 = $08
        move.w  #$9400,(a5)             ; reg 20 = $00
        ; DMA mode = VRAM fill (reg23 bits 7-6 = 10)
        move.w  #$9780,(a5)             ; reg 23 = $80

        ; Destination command: VRAM write @ $0400 with CD5 set (code $21).
        move.w  #$4400,(a5)             ; word1: CD1-0=01, A13-0=$0400
        move.w  #$0080,(a5)             ; word2: CD5-2=1000, A15-14=0
        ; Trigger the fill: data-port write supplies the fill value ($EE = top byte).
        move.w  #$EEEE,(a4)

        ; Wait for DMA-busy (status bit1) to clear before reading VRAM.
FillWait:
        move.w  (a5),d0
        andi.w  #$0002,d0
        bne.s   FillWait

        ; Read back VRAM $0400 / $0402 / $0406 into work RAM.
        movea.l #$00FF8000,a0
        move.w  #$0400,(a5)
        move.w  #$0000,(a5)
        move.w  (a4),(a0)+
        move.w  #$0402,(a5)
        move.w  #$0000,(a5)
        move.w  (a4),(a0)+
        move.w  #$0406,(a5)
        move.w  #$0000,(a5)
        move.w  (a4),(a0)+

        bra     Finish

; ================= Cell 1: data-read while write-armed =================
Cell1:
        ; Arm the FIRST word of a VRAM-WRITE command @ $0200 (CD0=1 = write).
        move.w  #$4200,(a5)
        ; Now perform a DATA-PORT READ — recon R1 pins this as a hardware lockup.
        move.w  (a4),d0                 ; may hang the machine here (BlastEm models the lockup)
        move.w  d0,$00FF8008            ; only reached if it did NOT hang

Finish:
        move.w  #$C0DE,$00FF8010        ; done marker (absent => the machine hung)
Done:
        bra     Done

; ---- Generic handler: mark + halt ----
GenericH:
        move.w  #$DEAD,$00FF8010
GenHalt:
        bra     GenHalt

        end
