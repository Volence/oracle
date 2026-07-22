# DR-2 Thunder Force IV — next-layer triage (post-CD5-fix): the "$0FF35x loop" is an interactive menu, not a bug

**Status: TRIAGE (recon), 2026-07-22. Docs only — no code (all instrumentation reverted, tree clean).**

After the CD5 clear-on-consume fix (`b05786e`) killed the ~28-frame phantom-DMA halt, TF4 no longer freezes — it
runs at full step rate and renders. But boot did not reach gameplay: PC cycled a `$0FF35x` region across every
frame budget. The prior triage (`docs/2026-07-22-tf4-triage.md`) is a **STALE map** — that doc's blocker (the
`$0FF350` len=0 DMA halt) is gone. This doc re-measures the *current* behavior and answers the overseer's
question: **where is boot now, and what CLASS is the remaining blocker (1 render/VDP, 2 wait-on-flag, 3 needs
Z80)?**

**Verdict up front: NONE of the three — there is no remaining blocker.** TF4 boots correctly, reaches its
**interactive intro/config menu**, and that menu polls the controller *inline* and idles until the player
presses a button. The headless `boot_rom` harness injects **no input**, so the menu loops forever — which is
**correct emulation**, not a defect. Injecting the documented input sequence advances TF4 out of the intro into
the next stage. The prior "permanently blank" label is also overturned: the screen renders 13 colors with full
VRAM/CRAM — a real menu, not a blank frame.

**Method.** Throwaway instrumented run of `examples/boot_rom`'s machine via a `BusEventSink` over
`run_frames_with_sink`: `on_step_boundary` tallied PC histograms / first-seen frames / milestone hit-counts /
a raw contiguous PC trace; `on_event` tallied reads by region. Controller input was injected with the core's
own `System::set_pad` (the only input path in the core; `crates/oracle-core/src/io.rs`). Static disassembly
(capstone M68K) of the `$0FEE00–$0FF7xx` boot region cross-checked every dynamic finding. **All instrumentation
reverted; nothing committed except this doc.**

Items **N1–N8**.

---

## N1 — Still NOT the Z80 core (prior finding holds), and NOT a halt

**PINNED.** Across 120/300/600/1200/3000-frame runs: `z80_reads = 0`, `fm_reads = 0`, `hv_reads = 0`,
`vdp_ctrl(data-port)_reads = 0`. TF4 reads no Z80-RAM mailbox, no FM port, no HV counter. **Class 3 (needs Z80
execution) is ruled out**, exactly as before. And it is no longer halted: step count climbs linearly with the
frame budget (≈8,700 steps/frame steadily; 10.45M steps at 1200f) — the CPU runs at full speed, the loops
iterate. The old "531 steps frozen for any budget" symptom is gone.

## N2 — The one thing that CHANGED vs the stale doc: VDP **status** reads are now enormous

**PINNED — overturns the prior `vdp_status_reads = 0`.** The settled loop polls the VDP status port
(`$C00004`) ≈1,500×/frame (185k reads over 120f → 2.0M over 3000f). This is *expected*: pre-fix the CPU was
frozen inside the 28-frame DMA and never reached this code; post-fix it reaches a **poll-based vsync**. The poll
is healthy — see N3.

## N3 — The heavy poll is a vblank vsync on status bit 3, and it works

**PINNED.** The most-hit PCs are a 3-instruction spin at `$0FEF76`:

```
0FEF76: move.w  $4(a6),d0     ; a6=$C00000 → read VDP status ($C00004)
0FEF7A: and.w   #$8,d0        ; bit 3 = VBlank flag
0FEF7E: bne.b   $fef76        ; spin while IN vblank  (wait for active display)
0FEF80: move.w  $4(a6),d0
0FEF84: and.w   #$8,d0
0FEF88: beq.b   $fef80        ; spin while NOT in vblank (wait for vblank to begin)
0FEF8A: rts
```

This is a textbook "wait for next vblank". Both edges are observed each frame (the `$fef76` loop exits at once,
the `$fef80` loop spins ~600× across active display, then vblank arrives and it returns) — so oracle-next's
VBlank status bit toggles correctly and vsync completes every frame. **Not a stuck status poll.** SR = `$2700`
throughout (interrupts masked) — this is a **polled boot/menu phase**, before the game switches to its
vblank-IRQ main loop.

## N4 — The master loop and its single exit gate

**PINNED.** Boot entry `$0FEE00` runs a one-time init (`bsr $ff3b2` etc., each milestone hit exactly once),
sets VDP reg1 = `$44` (display on, DMA off), then enters the master loop at **`$0FEF2E`**:

```
0FEF2E: bsr.w $fef76      ; vsync (N3)
0FEF32: bsr.w $ff1da      ; READ CONTROLLER → $FF800E   (N6)
0FEF36: bsr.w $ff228      ; button dispatch → may set $FF800A (N5)
0FEF3A: bsr.w $ff4ac      ; scrolling-text update
0FEF3E..4A: bsr $ff71a / $ff0e0 / $ff2b2 / $ff31c  ; palette fade + per-frame VRAM upload
0FEF4E: tst.w $800a.w     ; <<< EXIT GATE: work-RAM var $FF800A
0FEF52: beq.b $fef2e      ; loop while $FF800A == 0
0FEF54: bsr.w $ff000      ; (exit) fade out
0FEF58: move.w #$8104,$4(a6) ; reg1=$04 → DISPLAY OFF
0FEF60..6C: clear VRAM
0FEF72: jmp   $200.w      ; → NEXT STAGE
```

The whole "`$0FF35x` region" the boot appears stuck in is the bodies of these per-frame subroutines
(`$ff30a` VRAM upload, `$ff2ea` palette fade). They are **bounded** — e.g. the `$ff336` upload loop runs its
`dbra` 24×/call and returns cleanly (loop-top 13,585 vs loop-exit 566 ≈ 24:1 over 600f; the upload routine
returns 566 times). Nothing here is an infinite `dbra` or a runaway DMA. The loop simply repeats because its
**only exit condition is `$FF800A != 0`**, and nothing sets it without input.

## N5 — `$FF800A` (the exit flag) is set only by a button, gated on a menu option

**PINNED.** `$FF800A` is cleared to 0 in init (`$0FF46E`) and set to 1 in exactly one place — the button
dispatcher `$0FF228`, which reads `$FF800E` (the assembled pad word, N6) and branches on its bits:

```
0FF228: move.w $800e.w,d0
        bit 0 (Up)   → $800c = $e7        ; menu option select
        bit 1 (Down) → $800c = $f7        ; menu option select
        bit 5 (C)    → $ff25a:
0FF25A:   tst.w $8004.w / bne (edge-detect guard)
0FF260:   move.w #$1,$8004.w
0FF266:   cmpi.w #$e7,$800c.w
0FF26C:   beq $ff276                       ; option==$e7 → toggle an "ON"/"NO" display, DON'T exit
0FF26E:   move.w #$1,$800a.w               ; option==$f7 → SET EXIT FLAG
```

`$ff276` even writes ASCII `$4E4F`="NO" / `$4F4E`="ON" to a text buffer (`$0FF28A`/`$0FF29A`). This is an
**options/config menu**: Up/Down pick an option (`$800c` = `$e7`↔`$f7`), **C** confirms — and only exits when
the non-default option is selected. Pure interactive UI; no external event, no timer, no chip handshake.

## N6 — The controller read is inline and correct (3-button TH protocol)

**PINNED.** `$0FF1DA` reads the pad *in the main loop* (not via a masked IRQ) with the standard 3-button
sequence and assembles `$FF800E`:

```
0FF1DC: move.b #$40,$a10009   ; P1 ctrl = TH output
0FF1E4: move.b #$40,$a10003   ; TH=1
0FF1EC: read $a10003; not; and #$0F → Up/Down/Left/Right  (bits 0–3)
0FF1FA: read $a10003; not; and #$30 → B,C                 (bits 4,5)
0FF208: move.b #$0,$a10003    ; TH=0
0FF210: read $a10003; not; and #$30; <<2 → A,Start        (bits 6,7)
0FF222: move.w d1,$800e.w
```

Assembled `$FF800E` = `[Start A C B | Right Left Down Up]` — so **bit 5 = C** (not Start; the stale
assumption). This exactly matches oracle-next's pad model (`io.rs pad_device_byte`: TH-high nibble
Up/Down/Left/Right + B/C; TH-low = A/Start). The `io_reads` tally (~3/frame at `$A10000–1F`) is precisely this
per-frame pad read — nothing is waiting on I/O; it is reading input that the harness never populates.

## N7 — Injecting the documented input advances TF4 out of the intro

**PINNED — the decisive test.** Using `System::set_pad`:

| Injected input | Result |
|---|---|
| none (default harness) | loops forever in `$0FEE00–$0FF764`; `$FF800A` never set; `intro_exit=0`, `stage2=0` |
| **C** held (frame 300) | new code runs at the exact press frame (distinct PCs 390→403, first-new-frame=301) → the `$ff276` **ON/NO toggle** branch. Input is delivered correctly; no exit because option was still `$e7` (N5). |
| **Down, release, then C** | **`intro_exit(FEF54)=1`, `stage2($200)=31,782` steps**, PC leaves the boot region and settles at `$006E06`, distinct PCs 390→516. **TF4 advances past the intro to the next stage.** |

This proves end-to-end that vsync, the inline controller read, the button dispatcher, and the exit `jmp $200`
all work in oracle-next. The only reason unattended boot never advances is the absence of injected input.

## N8 — The intro screen is NOT blank (overturns the sweep label)

**PINNED.** On the settled menu frame: **13 distinct rendered colors**, **VRAM = 31,944 non-zero words**,
**CRAM = 62/64 non-zero words**. `nonblack_px = 71,680` is the *whole* 320×224 filled by the menu backdrop
(no black gaps), not a blank frame. Font/tiles are uploaded and the palette is loaded. The earlier
"permanently blank" was an eyeball misjudgment of a mostly-uniform colored menu background. **No render defect
is implicated.**

---

## Verdict — class of the remaining blocker

**There is no remaining blocker of class 1/2/3.** TF4 boot is *correct*:

- **Class 3 (Z80 core): ruled out** — 0 Z80/FM reads (N1); confirms the prior triage.
- **Class 2 (wait on a 68k-visible flag we don't satisfy): ruled out** — the only "flag" is `$FF800A`, an
  internal work-RAM byte the game itself sets from **controller input** (N5/N6). The VDP vblank status poll it
  *does* depend on works (N3). It is not waiting on any HV/DMA-busy/IRQ cadence.
- **Class 1 (render/VDP correctness): ruled out for this layer** — control flow, VDP status, DMA (post-CD5),
  and rendering (13 colors, full VRAM/CRAM) are all correct (N3/N8). Injected input advances the machine (N7).

**The "$0FF35x loop" is TF4's interactive config/menu screen idling for input.** In the headless `boot_rom`
harness (no input path) it *should* loop forever. **DR-2 is fully closed: the CD5 fix resolved the only real
defect; the residual layer is expected behavior, not a bug.**

## Recommended NEXT SLICE — no TF4 fix; turn N7 into a differential fixture (optional, low priority)

There is **no code fix to make** for TF4 itself. The concrete, bounded follow-up (if the overseer wants TF4 to
count as a *gameplay* differential rather than a boot-only one) is an **input-scripted differential fixture**,
not an engine change:

- **Lead:** drive `boot_rom`/the differential harness with a small scripted pad timeline (e.g.
  `Down → release → C`, per N7) so TF4 reaches stage `$200`/`$6E06`, then A/B the resulting frame against
  Oracle at a fixed input+frame checkpoint.
- **Entry point:** the injection API already exists — `System::set_pad` (`crates/oracle-core/src/system.rs:191`)
  over `run_frames_with_sink`; the harness is `crates/oracle-core/examples/boot_rom.rs`. A future change would
  add an optional scripted-input timeline argument to that example (dev-tool only, never a CI gate — same
  disposition as the ROM files).
- **What a recon must re-derive first:** nothing about hardware behavior is open here (pad protocol and VDP
  status are already pinned by recon IO1–IO6 and R-series). The only thing to pin is the *reference* input
  script + checkpoint on the C++ Oracle side, so the two emulators are driven identically.

**Redirect the actual engine effort elsewhere.** With DR-1 (Gunstar) and DR-3 (Batman) resolved and DR-2 (TF4)
now shown to be correct, the differential punch-list from `docs/2026-07-22-differential-rom-findings.md` has no
open render/bus defect. The next *engine* rock should come from **widening the ROM set** (Alien Soldier,
Vectorman, Ristar, Sonic 3 Complete, S.C.E. — disasms already in-workspace) to surface the next real gap,
rather than from TF4.

## Gate-safety note (for whoever picks up the fixture)

An **input-scripted fixture is currency-neutral by construction** — it only *drives* existing public APIs
(`set_pad`, `run_frames_with_sink`) in a dev example; it changes no core behavior. Nothing in it touches SST,
the export golden, `golden_frames`, `oracle_differential`, or determinism. **If** a future engineer instead
tried to make TF4 auto-advance (it should not — that would be wrong), any change to the pad model (`io.rs`) or
the VDP status/DMA path (`vdp.rs`, `bus.rs`) would touch: **SST** (68k opcode currency — unaffected by I/O/VDP
but the bar to clear), **export golden / state-hash** (I/O pad state is an export-v2 candidate per `io.rs`
module docs; do not move the frozen `export_state` layout), **`oracle_differential`** and **`golden_frames`**
(any VDP-status or pad-read change alters rendered/streamed frames), and **determinism** (input injection must
be deterministic — `set_pad` is, being pure injected state). No such change is warranted by this triage.

## Reproduction

```
# Dynamic (throwaway, reverted): BusEventSink over run_frames_with_sink.
#  no input   → last_pc≈$0FF358, confined to $0FEE00–$0FF764 for 3000f, z80=fm=hv=0,
#               vdp_status≈1500/frame, intro_exit=0, stage2=0, 13 render colors.
#  Down→C via System::set_pad → intro_exit=1, stage2($200)=31782 steps, PC→$006E06.
# Static (capstone M68K, TF4 U ROM): master loop $0FEF2E; exit gate tst.w $800a @ $0FEF4E;
#   $800a set only @ $0FF26E (C+option); pad read $0FF1DA→$800e; vsync $0FEF76 on status bit 3.
```

## Sources

- Primary: instrumented oracle-next run (`run_frames_with_sink` + `set_pad`) + static disassembly of Thunder
  Force IV (U) (user's disk, D5; ROM never copied/committed).
- `crates/oracle-core/src/system.rs` (`run_frames_with_sink`, `set_pad`), `src/io.rs` (3-button pad model,
  `pad_device_byte`), `src/vdp.rs` (`take_dma_request` CD5 clear, status), `src/bus.rs` (DMA / VDP ports).
- `docs/2026-07-22-tf4-triage.md` (STALE pre-fix map), `docs/2026-07-22-vdp-dma-cd5-recon.md` (`b05786e`),
  `docs/2026-07-22-differential-rom-findings.md` (DR-1/2/3), `docs/2026-07-16-vdp-recon.md`,
  `docs/2026-07-17-io-recon.md`.
