; ---------------------------------------------------------------------------
; BlastEm-over-the-bus differential harness ROM (clean-room, authored here).
;
; Instrument only. BlastEm's `-D` GDB-remote stub halts the CPU at Init on
; entry; the RSP driver writes a RAM control block, sets breakpoints, and
; continues. The ROM dispatches per the control block and records observable
; side effects in work RAM. (PC/PC-register writes are unavailable over this
; stub, so all experiment selection flows through writable RAM.)
;
; Control block (driver writes before `c`):
;   $FF9000.b  loaded-T selector: 0 -> STOP #$2700 (loaded-T=0)
;                                  1 -> STOP #$A700 (loaded-T=1)
;   $FF9002.w  start SR value loaded via MOVE d0,SR immediately before the STOP
;              ($2700 start-T=0 / $A700 start-T=1; S=1, mask=7 in both)
;
; Observable RAM:
;   $FF8000.w  outcome  ($A909 trace / $FExx fell-through / $DEAD other / 0 none)
;   $FF8004.l  stacked PC (from the 6-byte exception frame at 2(sp))
;   $FF8008.w  stacked SR (from the frame at 0(sp))
; ---------------------------------------------------------------------------
        cpu     68000
        padding off
        org     $000000

; ---- Exception vector table (64 vectors) ----
        dc.l    $00FF0000               ; 0  initial SSP
        dc.l    Init                    ; 1  reset PC
        dc.l    GenericH                ; 2  bus error
        dc.l    GenericH                ; 3  address error
        dc.l    GenericH                ; 4  illegal
        dc.l    GenericH                ; 5  zero divide
        dc.l    GenericH                ; 6  CHK
        dc.l    GenericH                ; 7  TRAPV
        dc.l    GenericH                ; 8  privilege violation
        dc.l    TraceH                  ; 9  trace              ($24)
        rept    54
        dc.l    GenericH                ; 10..63
        endm

; ---- Minimal Mega Drive header at $100 ----
        org     $000100
        dc.b    "SEGA GENESIS    "
        dc.b    "(C)HARNESS 2026.JUL "
        dc.b    "M68K STOP-TRACE DIFFERENTIAL HARNESS  ROM        "
        dc.b    "M68K STOP-TRACE DIFFERENTIAL HARNESS  ROM        "
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

; ---- Reset/dispatch entry ----
        org     $000200
Init:
        movea.l #$00FF7000,a7           ; SSP in observable RAM
        move.w  $00FF9002,d0            ; start SR (start-T selector)
        move.b  $00FF9000,d1            ; instruction selector
        cmpi.b  #1,d1
        beq.s   RunT1
        cmpi.b  #2,d1
        beq.s   RunNop

; selector 0: STOP #$2700 (loaded-T = 0)
RunT0:
        move    d0,sr                   ; load start SR (may set T) just before STOP
        stop    #$2700                  ; instruction under test (loaded-T=0)
        move.w  #$FE00,$00FF8000        ; fell-through marker
FT0:
        bra     FT0

; selector 1: STOP #$A700 (loaded-T = 1)
RunT1:
        move    d0,sr                   ; load start SR (may set T) just before STOP
        stop    #$A700                  ; instruction under test (loaded-T=1)
        move.w  #$FE11,$00FF8000        ; fell-through marker
FT1:
        bra     FT1

; selector 2: NOP control (validates trace fires for a normal instruction)
RunNop:
        move    d0,sr                   ; load start SR (may set T) just before NOP
        nop                             ; normal instruction under test
        move.w  #$FEAA,$00FF8000        ; nop-fell-through marker
FTN:
        bra     FTN

; ---- Trace handler (vector 9): record the 6-byte frame, then halt ----
TraceH:
        movea.l #$00FF8000,a0
        move.w  (sp),8(a0)              ; stacked SR  -> $FF8008
        move.l  2(sp),4(a0)             ; stacked PC  -> $FF8004
        move.w  #$A909,(a0)             ; trace-taken -> $FF8000
TraceHalt:
        bra     TraceHalt

; ---- Generic handler (any other vector): mark + halt ----
GenericH:
        movea.l #$00FF8000,a0
        move.l  2(sp),4(a0)
        move.w  #$DEAD,(a0)
GenHalt:
        bra     GenHalt

        end
