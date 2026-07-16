; ---------------------------------------------------------------------------
; VDP control-port pending-toggle experiment ROM (clean-room, authored here).
;
; Question (VDP recon R1, open cell): after the FIRST word of a two-word VDP
; command is written to $C00004, which accesses CLEAR the first/second-write
; toggle? Permitted docs pin "data-port write clears" (Kabuto) and are silent
; on control-port (status) reads and HV-counter reads; the folklore claim
; ("status read clears the flag") exists only in non-permitted docs.
;
; Method: per cell, arm the toggle with the first word of a VRAM-write command
; for $0200, apply the probe, then write the AMBIGUOUS word $4300:
;   - toggle still armed  -> $4300 is the SECOND word (CD5-2=0, A15-14=0):
;                            completes the VRAM write @ $0200
;   - toggle cleared      -> $4300 is a FIRST word (CD1-0=01, A13-0=$0300):
;                            arms a VRAM write @ $0300
; A data-port write of $BBBB then lands at $0200 (armed) or $0300 (cleared) —
; both interpretations are writes, so no read-with-write-target lockup hazard.
; Sentinels $AAAA/$1111/$2222 pre-fill $0100/$0200/$0300; results are read back
; through the data port into work RAM for the RSP driver.
;
; Control block (driver writes before `c`):
;   $FF9000.b  probe selector: 0 -> none (control: pending persists)
;                              1 -> status read  (move.w (a5),d0)
;                              2 -> HV counter read ($C00008)
;                              3 -> data-port write $CCCC (Kabuto predicts clear)
;
; Observable RAM:
;   $FF8000.w VRAM[$0100]  (sentinel check, expect $AAAA)
;   $FF8002.w VRAM[$0200]
;   $FF8004.w VRAM[$0300]
;   $FF8006.w VRAM[$0202]  (catches the sel-3 "data write did NOT clear" case)
;   $FF8008.w the probe's read value (status / HV), $0000 for sel 0/3
;   $FF8010.w done marker $C0DE
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
        dc.b    "VDP PENDING-TOGGLE EXPERIMENT ROM                 "
        dc.b    "VDP PENDING-TOGGLE EXPERIMENT ROM                 "
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

        ; Minimal VDP init: mode 5, display off, no interrupts, autoincrement 2
        move.w  #$8004,(a5)             ; reg 0: no HINT, no HV latch
        move.w  #$8104,(a5)             ; reg 1: mode 5, display off, no VINT, no DMA
        move.w  #$8F02,(a5)             ; reg 15: autoincrement 2

        ; Sentinels: VRAM[$0100]=$AAAA, [$0200]=$1111, [$0300]=$2222
        move.w  #$4100,(a5)
        move.w  #$0000,(a5)
        move.w  #$AAAA,(a4)
        move.w  #$4200,(a5)
        move.w  #$0000,(a5)
        move.w  #$1111,(a4)
        move.w  #$4300,(a5)
        move.w  #$0000,(a5)
        move.w  #$2222,(a4)

        ; ---- Arm the toggle: FIRST word of "VRAM write @ $0200" ----
        move.w  #$4200,(a5)

        ; ---- Probe (per selector) ----
        moveq   #0,d0
        move.b  $00FF9000,d1
        cmpi.b  #1,d1
        beq.s   ProbeStatus
        cmpi.b  #2,d1
        beq.s   ProbeHv
        cmpi.b  #3,d1
        beq.s   ProbeData
        bra.s   ProbeDone               ; sel 0: no probe
ProbeStatus:
        move.w  (a5),d0                 ; status read
        bra.s   ProbeDone
ProbeHv:
        move.w  $00C00008,d0            ; HV counter read
        bra.s   ProbeDone
ProbeData:
        move.w  #$CCCC,(a4)             ; data-port write (lands @ live addr $0200)
ProbeDone:
        move.w  d0,$00FF8008            ; record the probe's read value

        ; ---- Ambiguous word + discriminator write ----
        move.w  #$4300,(a5)             ; word2 (completes @$0200) or word1 (arms @$0300)
        move.w  #$BBBB,(a4)             ; lands where the toggle state says

        ; ---- Force the toggle to a known-clean state before readback ----
        ; If armed, the first $8F02 is consumed as a second word (CD5-2=0 -> no
        ; DMA hazard) and cleans the toggle; the second is then a real register
        ; write (autoinc 2). If already clean, both are register writes.
        move.w  #$8F02,(a5)
        move.w  #$8F02,(a5)

        ; ---- Read back VRAM $0100/$0200/$0300/$0202 into work RAM ----
        movea.l #$00FF8000,a0
        move.w  #$0100,(a5)             ; VRAM read @ $0100 (CD=0000)
        move.w  #$0000,(a5)
        move.w  (a4),(a0)+
        move.w  #$0200,(a5)
        move.w  #$0000,(a5)
        move.w  (a4),(a0)+
        move.w  #$0300,(a5)
        move.w  #$0000,(a5)
        move.w  (a4),(a0)+
        move.w  #$0202,(a5)
        move.w  #$0000,(a5)
        move.w  (a4),(a0)+

        move.w  #$C0DE,$00FF8010        ; done marker
Done:
        bra     Done

; ---- Generic handler: mark + halt ----
GenericH:
        move.w  #$DEAD,$00FF8010
GenHalt:
        bra     GenHalt

        end
