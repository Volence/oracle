# A Z80-timing currency (M24-NEEDS-A-CURRENCY), shaped with F-Z80-ACCESSES-UNWATCHED

**Design call, 2026-09-13.** Brief: `docs/2026-09-13-m24-z80-timing-currency-brief.md`. Base `5ee7d6a`. Every
number below was measured by the spike commits named in §8. Those commits are kept in history and removed from
the tip, so this merge changes docs only. No emulator was launched, and no emulator source was read.

## Plain summary

**What was wrong.** Two things were missing, and one was worse than thought:

- Nothing we record could see the sound chip's timing.
- The sound chip's interrupt has two wrong behaviours, not one.
- The shortcut the last session hoped would fix both at once, a recorded stream of every sound-chip memory
  access, does work as a detector. It is still not the best check for this fix.

**Recommendation.** Add a small set of in-tree sound-chip timing tests. Each is a tiny Z80 program that stores
what it observed into Z80 RAM, and the test compares that against the documented answer: Zilog's manual for
the `EI` delay, the pinned one-line interrupt width for `/INT` (recon R6), and the clock constants for bus
grants. Measured, each test moves under exactly the behaviour it names and under nothing else. The tests can
tell right from wrong, which a recorded digest cannot. They cost milliseconds, touch no hot path, no save
format, no frozen hash and no wire. They land pinned at today's values, with the documented value beside each
one, so that the M24 fix has to move them by a predicted amount.

**The watchpoint gap.**

- First, and on its own, a caveat. It makes the watchpoint's header true and tells a reader that a zero on a
  sound-chip-reachable address means nothing. It moves no bytes.
- Later, the full fix: a separate delivery path for the Z80's accesses (not the existing bus-event stream),
  which leaves every current consumer untouched.
- Its watch hits would need a new field on the wire. That is a contract change, and yours to rule on.
- Once that path exists, the whole-replay digest that confirmed the last session's hypothesis becomes a cheap
  breadth tripwire.

**Cost.**

- One parcel of tests, size S.
- The two M24 fixes, S each.
- The caveat, XS.
- The access hook, M plus a contract change.

**Audible effect.** The interrupt-width fix changes what aeon's sound driver sees: it loses 2-3% of its
VBlank interrupts (34 of 1198, 46 of 1822, 57 of 2447), every one of them during a 68000 bus grant. **TAG:** listen on `aeon/s4.debug.bin`.

## 0. Where the brief was wrong

Each item is measured or read from the code, not reasoned.

1. **"`/INT` is held for the whole vblank" is not what the code does, and it hides a second defect.** `/INT` is
   not modelled as a level. `EventKind::VInt` sets a request (`Z80::set_int_line(true)`),
   `Z80::accept_interrupt` *consumes* it (`int_pending = false`), and line 0 drops it if nobody took it. So a
   request waits up to about 38 lines for a Z80 that has interrupts disabled, and a still-asserted line can
   never re-trigger. The hardware (R6, below) is a level for one line: a handler that re-enables inside the
   window takes it again. **M24's `/INT` half is therefore two behaviours, width and level.** Measured on
   aeon:
   - The width alone costs 34 of 1198 accepted interrupts over 1200 frames (leg A). It costs 46 of 1822 on the
     `ojz` replay and 57 of 2447 on the slide replay.
   - The level adds nothing on aeon (0 re-triggers anywhere, §2), because its handler re-enables too late.
2. **The Genesis side of question 6 is already answered in-tree.** `docs/2026-07-16-vdp-recon.md` §R6 pins the
   Z80 `/INT` pulse at "exactly one line ≈ 228 Z80 clocks", from SpritesMind t=740 and t=787. The VDP design
   doc marks it `[settled — R6]`. The brief asks for that prose as if it were open. §6 re-fetches and re-quotes
   it, and names what the sources leave open.
3. **Candidate (b) does not need a layout change, and it fails the neutrality test anyway.** `export_state`
   already carries the Z80's RAM (region 3) and its live register file (region 4, 30 of 64 bytes, filled at
   Z-live without a bump). What it lacks is a fixture where the Z80 runs: `export_state_v1` loads
   `testrom::build`, which holds the Z80 in reset. Sampled at every frame end over aeon, `export_state` moves
   under every timing mutation, and also under the timing-neutral `R` control (§2). It is a state currency,
   not a timing currency. `state_hash` covers the VDP only and is untouched by every option here.
4. **Candidate (c), "why did the FM/PSG tap not see M21's mutation?"**
   - The premise is off: no committed golden is built from the tap. `VgmLogger` has two consumers,
     `examples/vgm_capture.rs` and `system::tests::vgm_logger_captures_z80_fm_and_psg_writes_end_to_end`,
     which decodes a synthetic Z80 program's writes with no bus grant in it.
   - Measured on aeon, the tap itself does move under M21's mutation. It is **blind to the `EI` delay**: the
     subframe VGM, the frame VGM and the Z80-side tap digest are all byte-identical under it, because aeon's FM
     writes come from its Timer-A sequencer, not from the interrupt handler the delay shifts.
5. **"`fc = 0` already means DMA/other non-CPU master" is not how the run loop uses it.** DMA never reaches the
   bus-event stream: VDP-internal DMA writes arrive through `on_vdp_write` with `via = dma`, per the
   `watchpoints.rs` header. In the real run loop, `fc = 0` is emitted only by the Z80's FM/PSG tap and by the
   68000's `$A07F11` PSG write, which is re-emitted Z80-shaped (`addr $7F11`, F-TRACE-MASTER). The phase-0
   `SystemBus` emits it too, but only in tests. So today `fc = 0` already means "Z80-shaped".
6. **Two of the brief's consumer hypotheses are refuted by the code.**
   - The profiler cannot count Z80 accesses as 68000 work. `Profiler::on_event` latches only `fc = 7` reads
     (the interrupt acknowledge), and its cycles come from `on_step_retire`.
   - An address breakpoint cannot fire on Z80 reads. `BreakStop` raises its flag from `on_step_boundary`
     (a 68000 PC) and ignores `on_event`.
7. **The watchpoint attribution hypothesis holds, and the brief understates it.** `catch_up_z80` runs *after*
   the 68000 step's `on_step_boundary` and `on_step_retire`. So a Z80 access delivered as a `BusEvent` would be
   attributed to the 68000 instruction that has already retired. Its clock, the Z80 frontier, lags that
   instruction's, so the hit ring would stop being clock-ordered. That breaks an invariant
   `tests/watchpoints.rs::hits_carry_a_monotonic_master_clock_consistent_with_the_frame` asserts: "the clock
   never runs backwards across the hit log".
8. **A collision exists today that the brief does not name.** A bus-space watch matches on the address alone
   (`WatchSpec::matches`; the `fc` filter is optional). The tap emits the Z80's FM/PSG writes at their raw Z80
   addresses, `$4000-$4003` and `$7F11`, and those are also 68000 cartridge-ROM addresses. So a write watch
   over ROM `$004000` already records Z80 FM writes as hits, with a 68000 PC and `fc 0`.
9. **"25 tests take the gated-on arm (aeon replay fixtures, vendored-ROM scorecards)" hides the part that
   matters.** Re-measured over the whole release suite (§2.3), it reproduces as 32 test threads gated-on, 25 of
   them with a grant that cuts an instruction. **Only 9 ever execute `EI`**: the 8 aeon `replay_real_artifacts`
   tests and one unit test. No vendored test ROM enables Z80 interrupts. Every scorecard runs millions of Z80
   instructions with interrupts off, so none can see either M24 half.
10. **The brief frames the Z80 stream "as `BusEvent`s".** §3 enumerates what that would break. A separate hook,
    the `on_vdp_write` precedent, leaves every existing consumer's stream byte-identical, and it is the
    better shape.
11. **Hypothesis 4 holds on sensitivity but is not the best check.** A timestamped digest of every Z80 access
    over the aeon replays moves under all three timing mutations and holds still under both neutral controls.
    It cannot say which answer is right, though. It also moves under any 68000 timing change (grants are
    68000-timed), and it needs the F-Z80 hook first. §1 picks differently and says what that costs.

A finding the brief did not ask about, but that the `/INT` fix must respect: **the Z80 sees `/INT` late by up
to one 68000 instruction.** The run loop's order is events, then the 68000 step, then the Z80 catch-up. The
catch-up that crosses the VInt instant runs *before* that event is delivered, on the next iteration. A fix
that times the deassert should therefore compare the Z80's own frontier against the assert instant, as the
spike's `int1` mutation does. It should not rely on a scheduled event.

## 1. What a Z80-timing currency is, and which one to build

**The property.** A committed artifact that **moves** when *when the Z80 executes, is granted the bus, or
accepts an interrupt* changes, and that does **not** move under a timing-neutral change. It should also say
*which* of the three moved, and ideally whether the new answer is the right one.

The candidates:

| | Candidate | What it is | Sensitivity (§2) | Right-vs-wrong? | Touches |
|---|---|---|---|---|---|
| (a) | Z80 access-stream digest | FNV over every Z80 bus access `(op, addr, value, mclk)` across a corpus run | moves under M21, EI, `/INT`; still under both neutrals | no (it only detects change) | needs the F-Z80 hook (§3); the hot path (§4) |
| (b) | `export_state` at fixed instants | the export image at every frame end, folded | moves under all three, **and under the `R` neutral** | no | nothing new (regions 3-4 already live); needs a Z80-running fixture |
| (c) | the FM/PSG tap | `VgmLogger` output (subframe or frame), or the tap digest | moves under M21 and `/INT`; **still under EI** | no | nothing new |
| (d) | acceptance log | the Z80 frontier at each `/INT` acceptance | moves under all three on aeon; still under both neutrals; **empty in a corpus with no interrupts** (C3) | no | needs an acknowledge signal (the F-Z80 hook's `Ack` kind, §3) |
| (e) | **Z80 timing probes** | tiny in-tree Z80 programs on the `testrom::build` machine, each storing its observable in Z80 RAM, compared against a value derived from UM0080, R6 and the clock constants | each moves under **exactly** its own behaviour; still under both neutrals | **yes** | nothing: a test file |

**Better-approach pass. The pick is (e).** Reasons, each measured in §2:

- It is the only candidate whose expected value comes from documentation rather than from the implementation.
  C1 must read 1 because UM0080 says the instruction after `EI` runs first. C2 must read `$E0` because R6 says
  the pulse is one line. C3 must equal gated-on time / 510 because the loop is 34 T-states. A digest can only
  say "changed". These tests say "now correct".
- It is selective. C1 moves only under the `EI` delay, C3 only under the grant mutation, and C2 under `/INT`
  (V and HL) and, by one instruction, under the `EI` delay (HL only). That is correct physics, not bleed.
- It is free. There is no hook, no hot-path change, no wire change, no save-format or export change, and each
  test takes milliseconds.

**What the losers catch that (e) does not:**

- **(a) and (d) see the real driver.** A probe program tests what its author thought to test. The whole-replay
  digest would catch the interactions nobody wrote a probe for: the grant-spanned interrupts aeon actually
  hits (33 per 1200 frames), a reordering of the catch-up, `HALT` wake-up, a grant landing mid-handler. (d)
  catches the acceptance share of that for a fraction of the size.
- **(b) catches consequences.** It is the only one that would show a Z80 timing change leaking into 68000
  state. None did in 1200 aeon frames, where `export_rest` was unmoved under every mutation.
- **(c) is the only one that measures what reaches the sound chips**, which is what the owner hears. It is
  blind to the `EI` delay on this corpus.

So (a) is the right *second* artifact, a breadth tripwire over the committed replays. It arrives with the
F-Z80 hook, which is when it becomes cheap (§7, parcel 6).

## 2. Sensitivity, measured

**Instrument.** Spike commits `0b1dd3b` and `aa04b7b`, both removed from the tip. They add a thread-local
probe and env-gated mutations (`M24_MUT`) to one build, so no leg depends on cargo noticing an edit. Every
mutation counts its own firings (`mut_fired`), and that count is the runtime proof that it applied. The
mutations:

- `m21`: `+ 8 × MCLK_PER_LINE = 27,360` mclk, once at each grant edge, carried like the tail. This is the M21
  corpus measurement's mutation G.
- `ei`: a pending request is not accepted at the boundary immediately after an `EI` (UM0080 p.18).
- `int1`: `/INT` is dropped once the Z80's own frontier is `MCLK_PER_LINE` = 3420 mclk (228 Z80 clocks) past
  the assert (R6).
- `int1lvl`: `int1`, and acceptance no longer consumes the line.
- `r` (neutral, state): `R` advances by 2 per M1. Aeon's driver never reads `R`
  (`grep` of `engine/sound/*.emp` for `ld a,r`: none).
- `render` (neutral, output): bit 0 of every decoded red channel flipped.

**The legs:**

- A: aeon `s4.debug.bin` from power-on, no input, 1200 frames. `export_state` is folded at every frame end,
  and the real `VgmLogger` runs in both timings.
- A2: the same for 300 frames, with the renderer armed.
- B: both committed replay fixtures through `runner::run`.
- C1, C2, C3: the probes.

### 2.1 Candidates × mutations

✔ = moved, · = did not move. Every "·" under a mutation comes with that leg's `mut_fired > 0`, so the mutation
ran there; the exceptions are called out.

| Candidate (leg) | m21 | ei | int1 (= int1lvl) | r (neutral) | render (neutral) |
|---|---|---|---|---|---|
| (a) timed access digest (A, B×2, A2) | ✔ | ✔ | ✔ | · | · (A2, fired 21.5 M) |
| (a′) untimed access order (A, B×2) | ✔ | ✔ | ✔ | · | · |
| (b) `export_state` every frame end (A) | ✔ | ✔ | ✔ | **✔** (regs region only) | · |
| (b) of which `export_z80regs` | ✔ | **·** | ✔ | ✔ | · |
| (b) of which non-Z80 regions | · | · | · | · | · |
| (c) `VgmLogger` subframe, frame (A) | ✔ | **·** | ✔ | · | · |
| (c) Z80 tap digest (A, B×2) | ✔ | **·** | ✔ | · | · |
| (d) acceptance log (A, B×2) | ✔ | ✔ | ✔ | · | · |
| (e) C1 `A` at acceptance | · (no grants: `mut_fired` 0) | **0 → 1** | · | · | · |
| (e) C2 `V`, `HL` at acceptance | · (no grants) | HL **0 → 1** | V **$E2 → $E0**, HL **0 → 3291** | · | · |
| (e) C3 count | **4314 → 3778** | · (no `EI`: `mut_fired` 0) | · | · | · |
| Existing committed suite (release, 90 legs) | **3 fail**, all M21's own synthetic unit tests (§2.4) | **0 fail** (2905/0/3) | **0 fail** under `int1lvl` (2905/0/3) | not run (the SST-z80 gate checks R) | not run |

**Expected values, derived before being read:**

- **C1:** UM0080 gives `A = 1`: the `INC A` after `EI` runs before the handler. Today's value is 0, and 1 under
  `ei`.
- **C2:** the Z80 enables at line 226 (V = `$E2`), two lines after the assert. Under a one-line pulse it takes
  the *next* frame's interrupt, at V = `$E0`. HL then counts one `INC HL`/`JR` pass (18 T = 270 mclk) per 270
  mclk from the `EI` to that assert. That is about 260 lines × 3420 = 889,200 mclk, less the poll's exit slack
  (at most 480 mclk) and the `LD HL`/`EI` (210 mclk), divided by 270: about 3290-3293. Measured: 3291.
- **C3:** the harness drove 10 grants of 10,000 mclk around 11 gated-on spans totalling 2,200,226 mclk, so the
  count is 2,200,226 / 510 = 4314. Measured: 4314. Under `m21` the Z80 loses 10 × 27,360 / 510 = 536.5 passes.
  Measured: 4314 − 3778 = 536.

**What else would give the same result, and how each was ruled out:**

- A mutation that never ran looks like stillness. Each leg's `mut_fired` rules that out. Two places show it
  mattering: `render` fired 0 times in legs A, B and C, because nothing decodes CRAM on a null or VGM sink run
  (finding C5), so leg A2 was added. `m21` fired 0 times in C1 and C2, which drive no grants, so their
  stillness under it says nothing, and C3 carries that column.
- A digest might exclude the changed field. It doesn't here: the timed digest folds the clock of every
  access, and the order digest drops it on purpose. `export_rest` was unmoved under every timing mutation; that
  is aeon's 68000 not observing Z80 timing within 1200 frames, not a digest gap. The same digest moves under
  the 68000-side `render` control through its pixels (A2).
- A driver that never runs `EI` cannot move under the `EI` or `/INT` mutations. Aeon executes 866,459 `EI`s in
  leg A and accepts 1198 interrupts. 1053 of those acceptances come right after an `EI`, at the idle loop's
  one-instruction `ei; jp SndDrv_Idle` window. Those are exactly the ones `ei` defers (`mut_fired` = 1053).
- For `/INT`, the question is whether any acceptance is late enough to be lost. Leg A has 33 acceptances at
  least one line after the assert, and `int1` removes exactly those plus one (34 lost). **All 33 are
  grant-spanned** (`late_grant` 33, `late_masked` 0): aeon never masks through the window, but its 68000 holds
  the bus across it. B shows the same, 45/45 and 56/56.
- **The level half moves nothing on aeon.** `int1lvl` is field-for-field identical to `int1` on every leg except
  `mut_fired`, with 0 re-triggers. Aeon's handler (four pushes and a mailbox poll, then `ei; ret`) re-enables
  after the one-line window closes. A probe for the level half must supply its own short handler (parcel 2,
  C4).

### 2.2 The replay verdicts

All four timing mutations leave both replay fixtures green (`verdict = PASS`, same `Logic_Tick`). A Z80 timing
change this size is invisible to them. They see whether the driver answers (M21's mutation H, "Z80 never
runs", does fail them), not when.

### 2.3 Which committed tests reach the Z80

Whole release suite with the probe, `CI=1 --no-fail-fast`: 90/90 legs, 2904 passed / 0 failed / 3 ignored.
That is the 2898/0/3 baseline plus the spike's 6. The probe logs one line per test thread that ran the Z80
gated-on or granted, when that thread exits. The six spike threads are excluded below.

| Test | gated-on Z80 instrs | grants (cut) | `EI` | accepted | right after `EI` | late |
|---|---|---|---|---|---|---|
| `replay_real_artifacts` × 8 (aeon `s4.debug.bin`) | 0.23 M – 24.9 M | 103 – 11,138 | 25 k – 2.7 M | 35 – 3711 | 30 – 3271 | 0 – 90 (4 tests > 0) |
| `system::tests::z80_takes_the_vblank_interrupt_and_runs_its_im1_handler` | 14,929 | 0 | 1 | 1 | 0 (`EI; HALT`) | 0 |
| `conformance_roms::testrom_conformance_scorecard` | 3,579,764 | 638 (616) | 0 | 0 | – | – |
| `scanline_goldens::scanline_golden_scorecard` | 3,013,494 | 478 (460) | 0 | 0 | – | – |
| `watchpoints` × 3 (vendored K4 ROMs) | 207,253 each | 68 (62) | 0 | 0 | – | – |
| `oracle-frontend` `save_state` × 5, `scanline_capture`, `scanline_goldens::the_live_hash_depends_on_the_pixels`, `color_1536_gradient_guard`, `oracle-aether` `scanlines` | 3 – 6 | 1 – 3 | 0 | 0 | – | – |
| `system::tests` M21 × 4 and Z-live × 5 (synthetic) | 8 – 44,800 | 0 – 201 | 0 | 0 | – | – |

So 32 threads run the Z80 gated-on, 25 take a grant that cuts an instruction (M21's figures, reproduced), and
**9 execute `EI`**. The only committed fixture that can see either M24 half is aeon's replay pair. The
instrument logs at thread exit, so a thread alive at process exit would go unlogged. The counts match M21's
independent measurement, so none was lost here.

### 2.4 Which committed tests carry each signal

The whole release suite (`CI=1 cargo test --workspace --release --no-fail-fast`) was run under each timing
mutation, on the spike build at `aa04b7b`.

- **`m21`:** 90/90 legs, **2902 passed / 3 failed** / 3 ignored. The three are M21's own synthetic reproductions
  in `system::tests`, each of which pins the tail arithmetic by construction:
  - `a_bus_grant_does_not_refund_the_z80_the_tail_of_the_instruction_it_cut` ("executed 0" of 580
    instructions);
  - `a_bus_grant_keeps_the_tail_of_the_cut_instruction_owed_until_the_release`;
  - `a_reset_under_a_bus_grant_cancels_the_tail_the_grant_was_carrying` (27,478 against 118).

  No corpus test, no golden and no scorecard moved. This is M21's finding, re-confirmed with the M21 tests
  included: grant timing is guarded only by the tests written for it.
- **`ei`:** 90/90 legs, **2905 passed / 0 failed** / 3 ignored. **Nothing in the suite pins the `EI` delay.**
  The SST-z80 gate checks only the final IFF bits, the one interrupt unit test uses `EI; HALT` (acceptance
  during `HALT`, which the delay does not touch), and the aeon replays pass.
- **`int1lvl`:** 90/90 legs, **2905 passed / 0 failed** / 3 ignored. **Nothing in the suite pins the `/INT`
  width or level either.** The one interrupt unit test enables before the assert and halts. The aeon replays
  lose 46 and 57 interrupts and still pass.

So the committed suite guards neither M24 half. Parcel 2's probes would be the first tests to move when M24
lands, and today they would be the only ones.

## 3. The Z80 access stream: every consumer, and the shape that breaks none

`BusEventSink` implementors at `5ee7d6a` (by `impl … BusEventSink for`), and what Z80 accesses would do to each
**if delivered as `BusEvent`s** (option A). Under option B, a new defaulted hook, every row reads "unaffected"
except where marked.

| Consumer | Where | Today's `on_event` | Z80 accesses as `BusEvent`s (A) |
|---|---|---|---|
| `Watchpoints` | `oracle-core/src/watchpoints.rs` | builds a hit with the latched 68000 PC, frame and mclk; bus space matches on address (optional `fc`) | **breaks**. Attributed to the 68000 instruction that just retired (§0.7); the hit ring stops being clock-ordered; raw Z80 addresses `$0000-$7FFF` collide with 68000 ROM; `fc 0` conflates with the PSG re-emit; `seen` grows by about 15 k per frame on aeon (17.9 M accesses / 1200 frames); a `stop_after` watch can end a run on a Z80 access. **Wire-visible.** |
| `Profiler` | `profiler.rs` | latches only `fc = 7` reads | unaffected in function (hypothesis refuted); a per-event dispatch cost only |
| `VgmLogger` | `vgm.rs` | classifies writes on address alone (`$4000-3`/`$A04000-3`, `$7F11`/`$C00011`) | **breaks** two ways. A generic write at `$4001` alongside the existing tap is a second record of one write. If window accesses carry 68000 addresses, a Z80 write through its own window to `$A04001` becomes a VGM record, although `write_window` routes it into Z80 RAM. |
| `AudioSink` (feature `synth`) | `synth/audio_sink.rs` | same classification, writes only (`on_event_at`) | breaks exactly as `VgmLogger` |
| `ScanlineCapture` | `scanline_capture.rs` | no-op | unaffected |
| `BreakStop` / `LineStop` / `StepStop` | `oracle-aether` `breakpoints.rs`, `engine.rs` | no-op (execution, raster and retire conditions) | unaffected (breakpoint hypothesis refuted) |
| `StopWhen`, `()` | `bus.rs` | no-op | unaffected; `()` is the hot path (§4) |
| `Vec<BusEvent>` | `bus.rs` | records everything | **changes** every test that asserts an exact recorded stream over a Z80-releasing run. `z80::bus::tests::fm_and_psg_writes_tap_into_the_event_sink` asserts Z80-RAM and bank writes emit *nothing* (`sink.len() == 5`), so it breaks by design. |
| `&mut S`, `Option<S>`, `Observe<S>`, `Fanout<A,B>` | `bus.rs` | forward `on_event`/`on_event_at` explicitly | carry the change to whatever they wrap. **Under B, each must forward the new hook explicitly.** A combinator that forgets starves a wrapped `Watchpoints` silently in every production composition: `oracle-aether` `engine.rs` (`Fanout(… Observe(armed) …)`), `oracle-frontend` `main.rs`/`bus.rs`/`bus_stub.rs`/`audio.rs`, and `oracle-player` `machine.rs`. `bus::tests::fanout_forwards_every_hook_to_both_halves` and `observe_forwards_everything_except_the_stop_signal` must grow a row for it. |
| Test/example sinks | `RetireLog`, `BackdropVerdict`, `MagicLines`, `BoundaryLog`, `RowCounter`, `StopAfter`, `render` `Stop` | no-op | unaffected |
| | `VdpIdle` (`conformance_roms.rs`) | counts writes to VDP ports | unaffected with raw Z80 addresses; with 68000-space addresses, Z80 VDP-mirror writes would count as VDP activity |
| | `K4Counters` (`tests/watchpoints.rs`) | counts reads of `$A10000-1F` and `$C00004-7` | same: safe raw, not safe mapped |
| | `K4Probe` (`examples/k4_openbus_probe.rs`) | *"Z80-side bus events are 16-bit addresses (< $10000)"* | its stated assumption; safe raw, broken mapped |
| | `WriteWatch`, `Spy` (`bus.rs` tests) | count one address; record everything | `Spy` is the forwarding tests' witness |

Constructors of `BusEvent` outside tests: `MegaDriveBus::emit` (68000), the `$A07F11` PSG re-emit, `Z80Bus`'s
FM/PSG tap, and the phase-0 `SystemBus`. `oracle-frontend`'s `lens/watch.rs`, `lens/profile.rs` and
`audio.rs` construct them only in unit tests.

**The shape: option B.** A defaulted hook, `on_z80_access(&mut self, a: Z80Access)`, gated by
`wants_z80_accesses() -> bool` (default `false`). This is the `on_vdp_write`/`wants_vdp_writes` precedent.
`Z80Access` carries:

- the kind: `Fetch | Read | Write | Ack`. `Ack` is the interrupt-acknowledge cycle UM0080 describes ("M1, when
  operating together with IORQ, indicates an interrupt acknowledge cycle"), the Z80's counterpart of the
  68000's `fc = 7` acknowledge the profiler reads. It gives candidate (d) for free.
- the Z80-space address,
- the resolved 68000-space address,
- the value,
- the access's mclk,
- the Z80 PC of the instruction.

The existing FM/PSG `BusEvent` tap stays byte-for-byte as it is, so `VgmLogger`, `AudioSink` and the profiler
see an identical stream.

**Which address.** Both, and they answer different questions. A bus-space watch matches on the **resolved
68000 address**:

- The window maps through the bank: `(bank << 15) | (addr & $7FFF)`, via the cartridge mapper, as
  `Z80Bus::read_window` resolves it.
- Z80 space `$0000-$7FFF` maps to the 68000's own alias of it, `$A00000 | addr`.

That is what makes aeon's acceptance case work: a watch on the ROM bytes of an FM patch hits when the Z80
reads them through its window. A new Z80-space watch would match the 16-bit address, for "who touched my
driver's variable at `$1C00`". The Z80's accesses then never collide with 68000 ROM (§0.8), because they carry
the resolved address.

**The wire.** `Watchpoints` feeds `emulator/watchpoint_hits`. Each hit carries these keys:

- `fc` (0-7; the schema says "0 = a non-CPU master"),
- `via` (`bus`/`direct`/`dma`),
- `pc`, plus `symbol`/`symbolDisp` resolved against the **68000** listing,
- `seen`/`matched` totals.

A Z80 hit would need a master a client can read (a new `via` value such as `z80`, or a new key) and a PC that
is not symbolised against the 68000 listing. If Z80-space watches exist, the watch `space` enum
(`bus`/`vram`/`cram`/`vsram`) gains a value. Any of these is a schema change: **a contract change request,
which is your ruling.** Delivering Z80 hits as `fc 0`/`via bus` with a 68000 PC would need no schema edit, and
it would be a wrong answer on the wire. It is not offered.

## 4. The hot path

The null sink `()` is monomorphised and must stay free. Under option B, `Z80Bus::read`/`write` stay
**textually unchanged**. The design adds:

- a wrapper `Watched<'b, 'a, S>(&'b mut Z80Bus<'a, S>)` in `z80/bus.rs`, which implements `Z80Io` by
  forwarding and then calling `self.0.sink.on_z80_access(…)`;
- one branch in `catch_up_z80`'s gated-on loop: `if sink.wants_z80_accesses() { z80.step(&mut Watched(&mut
  bus)) } else { z80.step(&mut bus) }`.

For `S = ()`, `wants_z80_accesses` is the trait's constant `false`, so the branch folds and `Z80::step::<Z80Bus<()>>`
is the same monomorph as today. The instrumented path pays only when a sink opts in. This is as close as the
Z80 side can come to `watchpoints.rs`'s "unchanged textually" claim: the adapter's bodies are untouched, and
the only textual change on the null path is one branch on a constant.

**Measured.** Spike `21622cd` built exactly this shape: the trait methods and all four forwarders,
`Watched`, and the branch. `examples/m24_hotpath.rs` timed null-sink `run_frames` over
`fixtures/aeon/s4.debug.bin`. The run was five interleaved rounds of 5 × 600 frames each, at release, load
2.7-3.1. X is the baseline core at `5ee7d6a`; Y is the hook.

| Build | median of round medians (ns/frame) | round medians |
|---|---|---|
| X, null sink | 785,524 | 770,211 – 799,501 |
| Y, null sink | 761,218 | 751,148 – 782,491 |
| Y, armed (counting sink, 14,927 Z80 accesses/frame) | 811,553 | 798,113 – 814,417 |

- **Identity.** `export_state` is byte-identical across all three (`9a78410bcbdce273`).
- **Positive control.** Armed over 1200 frames counts 17,945,764 Z80 accesses, exactly the probe's independent
  count for the same boot (leg A). The hook sees every access.
- **Resolution.** Y is about 3% faster than X in every round. The change cannot plausibly speed anything up,
  and the two example binaries differ in layout (Y also carries the armed branch). That makes ±4% this A/B's
  resolution, and the claim is **no slowdown detectable on the null path**. An armed sink costs about 6.6% of
  a frame at aeon's Z80 load. Parcel 5 should re-run this A/B with both builds from one example text.

## 5. The cheap half of F-Z80-ACCESSES-UNWATCHED

**Land it first and alone.** It moves no emulation bytes, no frozen currency and no Aether wire:
`Watchpoints::caveats()` is not serialised by `oracle-aether`. It does move three readers:

- `oracle-player/src/stopping.rs`, which shows caveats in the watch panel;
- `examples/diag_soundqueue.rs`;
- one test. `tests/watchpoints.rs::hits_carry_a_monotonic_master_clock_consistent_with_the_frame` asserts that a
  plain `$FF0000` watch has no caveats, and work RAM is Z80-reachable through the window, so its assertion is
  re-derived with a `cause:` line naming this caveat.

**The module header**, the sentence to replace:

> ~~the real 68000/Z80 bus adapters deliver every access through `on_event_at`~~ → *the 68000 bus adapter
> delivers every access through `on_event_at`. The Z80 adapter delivers only its FM/PSG register writes
> (F-Z80-ACCESSES-UNWATCHED): its opcode fetches, its reads and writes through the `$8000` bank window, its own
> RAM traffic and its VDP-mirror accesses never reach a sink.*

**Caveat 1**, for any bus-space watch overlapping a Z80-reachable 68000 range: cartridge ROM `$000000-$3FFFFF`,
Z80 RAM `$A00000-$A0FFFF`, or work RAM `$E00000-$FFFFFF`.

> watch #N 'label': the Z80 can reach this range, and its accesses are not delivered to watchpoints
> (F-Z80-ACCESSES-UNWATCHED). Its fetches, its reads and writes through the $8000 bank window and its own RAM
> traffic never reach this sink; only its FM/PSG register writes do. Every count here is the 68000's alone, so
> zero Z80 hits is the instrument being absent, not a negative finding.

**Caveat 2**, for a bus-space watch that matches writes and overlaps `$004000-$004003` or `$007F11`. This is
§0.8's collision, which exists today.

> watch #N 'label': this range covers $004000-$004003 or $007F11, the raw Z80 addresses at which the Z80's
> FM/PSG register writes are emitted (fc 0). They match here as if they were accesses to cartridge ROM; filter
> on fc to exclude them.

**The acceptance case for the full half, red today.** A new test in `crates/oracle-core/tests/watchpoints.rs`,
`a_bus_read_watch_on_rom_sees_the_z80_read_it_through_its_bank_window`. It loads the Z80 program
`LD A,($8000); LD ($1000),A; HALT` with the bank on the ROM page (`z80_reads_rom_through_the_bank_window_in_the_run_loop`'s
program), releases the Z80, and puts a bus-space `Read` watch on the 68000 ROM byte it reads. It asserts at
least one hit whose master is the Z80, and that the byte landed in Z80 RAM. Today it reads 0 hits with
`seen > 0`, which is aeon's measurement in miniature.

## 6. What M24 needs, from documentation only

Sources are quoted verbatim. No emulator source was consulted, including `oracle-old`.

### 6.1 `EI` (Zilog *Z80 CPU User Manual*, UM008011-0816)

- p.18, *Interrupt Enable/Disable*: "Interrupts can be enabled at any time by an EI instruction from the
  programmer. When an EI instruction is executed, any pending interrupt request is not accepted until after
  the instruction following EI is executed. This single instruction delay is necessary when the next
  instruction is a return instruction. Interrupts are not allowed until a return is completed."
- *EI* instruction page (p.183-184): "Note: During the execution of this instruction and the following
  instruction, maskable interrupts are disabled." Example: "When the CPU executes an EI RETI instruction, the
  maskable interrupt is enabled then upon the execution of an the RETI instruction." Aeon's handler ends in
  exactly this idiom, `ei; ret`.
- *INT* pin: "The CPU honors a request at the end of the current instruction if the internal
  software-controlled interrupt enable flip-flop (IFF) is enabled."
- *BUSREQ* pin: "always recognized at the end of the current machine cycle"; *Bus Request/Acknowledge Cycle*:
  "The BUSREQ signal is sampled by the CPU with the rising edge of the most recent clock period of any machine
  cycle."
- *HALT Exit*: "The two interrupt lines are sampled with the rising clock edge during each T4 state."
- *Mode 1*: "the CPU responds to an interrupt by executing a restart at address 0038h … The number of cycles
  required to complete the restart instruction is two more than normal due to the two added wait states." (The
  core's 13 T-states for IM 1 agree.)

**Silent:**

- A run of `EI`s (`EI; EI; …`). The manual describes one following instruction.
- Whether an `EI` delay extends across a `HALT`. `EI; HALT` is the common idiom, and the manual says only
  that `HALT` waits for an interrupt "with the mask enabled".
- The INT pin text does not say whether the request is latched internally. It describes sampling at the end of
  each instruction and no latch. That a request is lost if it disappears before an instruction ends with IFF
  set is an **inference** from that silence, not a quotation.
- Likewise inferred, not quoted: no instruction ends while BUSREQ holds the bus, so a `/INT` pulse wholly
  inside a grant is not honoured. This is what makes aeon's 33 grant-spanned acceptances disappear under a
  one-line pulse.

### 6.2 How long the Genesis holds `/INT` (pinned in-tree as R6; re-fetched 2026-09-13)

- Charles MacDonald, SpritesMind t=740: "The INT output is asserted every frame for exactly one scanline, and
  it can't be disabled." "A very short Z80 interrupt routine would be triggered multiple times if it finishes
  within 228 Z80 clock cycles." (228 = `MCLK_PER_LINE` 3420 / `MCLK_PER_Z80_CYCLE` 15, which agrees.)
- Eke, SpritesMind t=787: "It is set at the exact same time as V-Interrupt (HCounter = $02). It is
  (apparently) cleared on next line, still at $Hcounter = $02, regardless of interrupts being masked on Z80 side
  or not. NB: the last statement needs to be confirmed." In Mode 4: "Z80 INT is also generated at the same
  point, dependless of the state of VINT."
- Nemesis, same thread: "Z80 int is triggered at 0x01, which is the same time as the F flag is set in the status
  register."

**Disagreements and silences. No number is picked here:**

- **The assert edge.** Eke says H = `$02`; Nemesis says H = `$01`. That is one H count apart, a pure timing
  detail. R6 already classifies it "defer, pure timing".
- **Deassert independent of Z80 masking.** Its own author says it "needs to be confirmed". R6 grades it medium
  confidence, and the "a masked Z80 misses the frame" corollary with it.
- **Plutiedev.** "Using the Z80" says only that the Z80 "also receives" the VBlank interrupt, with nothing on
  its width. Kabuto's hardware notes (Plutiedev mirror) say nothing on the Z80 interrupt.
- **Sega's own documents.** Both the *Genesis Software Manual* and the *Genesis Technical Overview* returned
  HTTP 403 from segaretro.org. **BLOCKED on that source**: Sega's wording is unread here, and is not
  paraphrased from memory.

## 7. Staging

| # | Parcel | Size | Moves | Needs your ruling |
|---|---|---|---|---|
| 1 | **F-Z80 cheap half.** The header sentence and caveats 1 and 2 (§5). | XS | caveat strings only: the player's watch panel, `diag_soundqueue`, and one test's expectation (with a `cause:` line). No emulation bytes, no currency, no wire. | no |
| 2 | **Z80 timing probes (the currency).** C1 (`EI` delay), C2 (`/INT` width), C3 (grant timing: count = gated-on time / 510, derived per run), and C4 (level re-trigger: a short handler that re-enables inside the window, counting its entries). Committed pinned at **today's** values, each with its documented value and source beside it. C3 is already right and guards M21. | S | a new test file; nothing else | no |
| 3 | **M24, `EI` delay.** A one-instruction shadow on the Z80. It fits in the export region-4 reserve if exported (a content fill at unchanged size, no version bump; the export golden's Z80 is held in reset, so it stays zero). | S | C1 0 → 1, and C2's HL 0 → 1, with `cause:` lines citing UM0080 p.18. Aeon replays: each acceptance after an `EI` moves one instruction later (1053 per 1200 frames); verdicts unchanged (§2.2). Existing tests: none move (§2.4), which is why parcel 2 must land first. | no (the documentation is unambiguous) |
| 4 | **M24, `/INT` as a one-line level.** The drop is timed against the Z80's own frontier (§0, the late-view note), and acceptance no longer consumes the line. | S | C2 V `$E2` → `$E0` and HL → about 3291; C4's entry count. Aeon loses the grant-spanned acceptances (34 per 1200 frames, 46 and 57 on the replays); verdicts unchanged. Existing tests: none move (§2.4). | **yes**: it rests on R6's medium-confidence masking corollary and changes what a real game's driver sees. **TAG: listen** on `aeon/s4.debug.bin`. |
| 5 | **F-Z80 full half.** `on_z80_access` + `Watched` + `Watchpoints` matching on the resolved 68000 address, and optionally a Z80 space. | M | `Watchpoints`' hits and `seen`; the forwarding tests; the red acceptance test (§5) goes green. The null path must measure unchanged (§4). | **yes: a contract change request** for the hit's master and PC (and `space`, if a Z80 space is added) |
| 6 | **Breadth tripwire.** Candidates (a) and (d) over both aeon replays, through a test sink on parcel 5's hook. Commit each digest *with* its counts (accesses, acceptances, right-after-`EI`, late; grant-spanned vs masked) so a mover's `cause:` line can name the measured change. | S | a new golden. It moves on any Z80 timing or behaviour change **and on any 68000 timing change** (grants are 68000-timed). | **yes**: whether to accept a golden that 68000 timing parcels will also move |

**How the currency verifies the M24 fix.** Parcel 2 lands green at today's values. Parcels 3 and 4 must each
move exactly the probe they name, by the predicted amount, and must leave C3 and the other probe's field
unmoved. A fix that moves nothing, or moves C3, is wrong by construction. The replay verdicts stay green and
are not evidence.

**Regeneration.** Goldens never regenerate silently. A probe's constant changes only in the commit that changes
the behaviour, next to a `cause:` comment. The comment names the mechanism and the measured delta, for example
`cause: M24 EI delay (UM0080 p.18): C1 A 0 -> 1`. Parcel 6's digest is regenerated the same way, and its
committed counts are what make the `cause:` line checkable rather than asserted.

## 8. Method, instruments and what they cannot see

- **Spike commits:**
  - `0b1dd3b` (probe, five mutations, legs A/B/C);
  - `aa04b7b` (the `int1lvl` mutation, the grant/masked split, re-trigger counting, leg A2);
  - `21622cd` (the probe removed from core; the proposed hook shape and the hot-path A/B harness);
  - the final `spike:` commit removes all of them from the tip.
- **Mutations are env-gated in one build** (`M24_MUT`), so no result depends on cargo's mtime fingerprint. The
  mutated lines are on disk in the spike commits. `mut_fired` counted each firing, and an unfired mutation is
  reported as unmeasured, never as stillness (`render` in A/B/C; `m21` in C1/C2).
- **Every timing figure ships with the machine's load.** Spike legs ran at load 2.8-6.3, and each leg is under
  3 s wall time in release. The whole suite with the probe took 7 min 18 s at load 3.5-8.5, uptime 7 h 46 m.
- **The probe's thread-exit log** cannot see a thread alive at process exit. The census matched M21's
  independent counts (32/25), so none was lost.
- **Not measured:**
  - how aeon sounds under either fix (TAG);
  - a Z80 access stream from the real hook (the probe computes digests in place; parcel 5 builds the hook);
  - the Sega manuals (BLOCKED, 403).
