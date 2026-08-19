# F-SCANLINE-SUBLINE — recon + design: sub-line landing resolution (2026-08-19)

**Recon and design only. No code, no tests, no gates in this pass.** Branch `subline-recon`, cut from
`m68000-microop-framework` at `678ed96`. Docs-only: nothing under `crates/`.

Registration: `docs/2026-08-19-aeon-acceptance-results.md:181-190`. Demand: Aeon's first acceptance sweep,
`/home/volence/sonic_hacks/aeon/docs/benchmarks/scanline-p2/HBLANK-WINDOW-SWEEP-RESULTS.md` — their §6
procedure measured the HBlank window's **upper** edge (N = 21.5) and had to **derive** the lower one
(15.21) from blanking width, and `flipX` was `0` at all 201 sampled N *because a partial row cannot exist
on this surface* (their §"`flipX` is 0 at every single N — that is the finding", `:311-332`).

---

## 0. The finding that resizes the problem: CRAM is a **decode-stage** input

F-SCANLINE-SUBLINE was registered as *"a core renderer change (resolve a line in segments, or re-resolve on
CRAM writes within a line) … larger than either [F-SCANLINE-INDEX or F-SCANLINE-SH]"*
(`docs/2026-08-19-aeon-acceptance-results.md:188-190`). Read against the renderer, that framing is
pessimistic by a wide margin, and the whole design below follows from why.

`Vdp::resolve_line` (`crates/oracle-core/src/render.rs:988-1035`) **never reads CRAM.** Trace every input it
takes: `sprite_line` (`:1074`), `window_span` (`:789`), `plane_hscroll` (`:711`), `plane_pixel` (`:760`),
`a_slot_pixel` (`:865`), `resolve_dot` (`:956`) → `rr9_winner` (`:882`) / `sh_state` (`:923`), and
`backdrop_index` (`:669`, which is *register 7*, an index — not a colour). All of them work in the
**index domain**. The resolved product is `Vec<PixelResolution>`, and a `PixelResolution` carries
`cram_index: u8` + `state: PixelState` per pixel (`render.rs:403-412`, built by `px_from` `:404`,
`backdrop_px` `:417`, `sprite_px_res` `:430`).

CRAM enters in exactly one place: `pixels_rgb` (`render.rs:1045-1052`) mapping each `PixelResolution`
through `cram_rgb_state(index, state)` (`render.rs:767-777`) — the **decode** stage, shared by
`render_line` (`:1039`) and `report_rgb` (`:1426`) *"so the two cannot drift"*.

Therefore:

> **A CRAM write landing inside line N cannot change which index any pixel of line N resolved to. It can
> only change the colour those already-decided indices decode to.**

Sub-line CRAM landing is not "resolve the line in segments". It is **decode the already-resolved line in
segments**, each segment against the CRAM contents that were live while that span of pixels was emitted.
The expensive, currency-loaded half of the renderer — plane fetch, sprite pipeline, priority, S/H,
`commit_scanline_sprites` (`render.rs:1450`) — is not touched at all, and does not move in time.

That single fact is what makes every answer below cheap, and it is why the recommendation is not the
option the registration anticipated.

---

## A. Timing model choice

### The convention today, and where it is written

Row N = VDP state at `N * MCLK_PER_LINE`, resolved once, atomically:

| Step | Anchor |
|---|---|
| `Scanline` event for line N scheduled at exactly `N * MCLK_PER_LINE` | seeded `system.rs:405`, self-rescheduled `system.rs:1070-1071` |
| Events are drained **before** the CPU step at each instruction boundary | `system.rs:970-975` |
| The handler renders line N immediately, unconditionally for `line < 224` | `system.rs:1043-1044` |
| The RGB decode + `on_scanline` happen **only** if the sink opted in | `system.rs:1045-1048`, gate `bus.rs:91-96` |
| `on_frame_boundary` fires at the line-224 event, after line 223's row | `system.rs:1055-1061`, contract `bus.rs:100-147` |

So row N is the state **before any of line N's writes**, and hardware consumes state as the beam crosses
the line. The consequence is stated in three places already (`bus.rs:91-95` Limitation L1;
`testrom.rs:425-427`; `crates/oracle-aether/tests/scanlines.rs:298-300`) and, since `112d683`, in the
contract itself (`empyrean` `contract/protocol.md:1165-1189`).

### (i) Deferred render — resolve row N at line N's end, segmented — **REJECTED**

Moving `render_scanline` from line N's start to line N's end moves three things that are not opt-in:

1. **The sprite latch commit.** `render_scanline` is `&mut self` precisely because it commits the R10
   masking carry and ORs the sprite-overflow / collision status latches
   (`render.rs:1438-1453`, `commit_scanline_sprites` at `:1450`). Those bits are **polled by games** —
   `system.rs:1036-1041` says so in as many words, and `system.rs:1574` (`scanline_wiring_evolves_the_
   sprite_masking_carry_during_a_run`) pins that the carry has advanced *by line 223, during the run*.
   Deferring the render deferres the instant at which a status read can see this line's flags, for
   **every ROM, armed or not**. That is a behavioural change on the hot path, not a capture change.
2. **Every non-CRAM input to the row.** Resolving at line end reads VRAM, VSRAM, scroll and the mode
   registers as of line end. Every row of every ROM that writes any of those mid-line changes content.
   That is the maximal currency blast radius, in exchange for nothing the demand side asked for
   (their ask is CRAM — §C).
3. **The report the sink sees.** `LineReport`'s non-pixel fields (`plane_a`, `plane_b`, `window`,
   `sprites`, …, `render.rs:1402-1419`) would all be end-of-line samples.

Rejected. It buys the same answer as (ii) and pays for it in the one place we are not allowed to spend.

### (ii) Eager resolve at line start + segmented decode at line close — **RECOMMENDED**

Concretely, and this is the whole of the mechanism:

```
line N's Scanline event, at N*MCLK_PER_LINE
  ├─ (armed only) emit the pending row N-1: decode its retained PixelResolution vector in
  │    segments, walking a CRAM working copy from the line-(N-1)-start snapshot and applying
  │    each journalled write at its pixel boundary            → sink.on_scanline(N-1, rgb)
  ├─ schedule the HInt anchor                                 [unchanged, system.rs:1035-1037]
  ├─ report = vdp.render_scanline(N)                          [unchanged, system.rs:1044]
  └─ (armed only) stash report + snapshot CRAM (128 B), clear the journal
line 224's event
  ├─ emit the pending row 223 (as above)
  └─ sink.on_frame_boundary(...)                              [unchanged order, system.rs:1061]
```

The journal is a list of `(mclk, cram_byte_addr, new_word)` for CRAM writes, filled at the VDP's own CRAM
choke (`Vdp::write_target`'s `Target::Cram` arm, `vdp.rs:730-738` — the same choke watchpoints v2 already
captures at, `vdp.rs:735`) and drained per CPU step by the run loop (`system.rs:990-992`).

Why this is the right shape:

- **`render_scanline` does not move.** Same instant, same inputs, same `commit_scanline_sprites`, same
  `LineReport`. Objections 1–3 to option (i) all evaporate.
- **The unarmed hot path is byte-for-byte today's.** `system.rs:1044` still renders and discards; the
  only additions are guarded by the same arming query that already gates the RGB decode
  (`sink.wants_scanlines()`, `bus.rs:93-96`) and the same per-run arming the VDP write capture already
  uses (`wants_vdp_writes` → `Vdp::set_write_capture`, `system.rs:966-967`, field docs `vdp.rs:195-200`
  — *"a single cheap `if self.capture_armed` test with no behavioral effect"*).
- **The armed path costs the same decode as today**, plus a 128-byte memcpy per line and one
  `Vec<PixelResolution>` alive at a time (~320 × 12 B). Each pixel is still decoded exactly **once**:
  the segments partition `0..width`. There is no re-render and no re-resolve. (Re-resolving would in fact
  be *wrong*, and `render.rs:1420-1425` already says why — the committed dot-overflow carry would reseed
  the R10 masking and could change the sprites.)
- **Zero mid-line CRAM writes ⇒ one segment ⇒ bit-identical bytes to today's row.** This is the
  currency-neutrality argument, and it is structural rather than measured: with an empty journal the
  emit path is `pixels_rgb(&report.pixels)` against the line-start CRAM snapshot, which is precisely
  what `report_rgb` does today at `system.rs:1046`.

The one thing that does move is **when** row N's bytes are handed to the sink: at `(N+1) *
MCLK_PER_LINE` instead of `N * MCLK_PER_LINE`. The *row index* is unchanged — `on_scanline(N, …)` still
means line N. §D audits who can tell.

Note the variant that avoids the lag — emit at line start as today, then *patch* the row as writes land —
was considered and rejected: it needs a new sink method (`on_scanline_patch`), every consumer has to
implement re-entrant row edits, and `ScanlineCapture` would have to make an already-latched row mutable.
A one-line emission lag with an unchanged interface is strictly cheaper.

### (iii) Exodus-style per-pixel-clock stepping — **NOT VIABLE HERE**

Exodus advances *"a single pixel clock cycle at a time"*
(`oracle/Devices/315-5313/S315-5313_Rendering.cpp:285`, loop `:322-345`; quoted at
`docs/2026-08-19-aeon-acceptance-results.md:150-155`). Five reasons it is not our answer:

1. **It contradicts the VDP's stated design.** `vdp.rs:4-7`: *"the rendering output stays **derived, not
   state** (nothing render-related serializes). Timing … is a pure function of the master clock, computed
   at read time — never an incremental counter."* A beam FSM is an incremental counter carrying
   render-derived state, and it would land in `System`'s `bincode`/`PartialEq` surface
   (`system.rs:167-169`).
2. **It buys nothing over (ii) for CRAM.** Per §0, CRAM is decode-stage; a per-pixel beam would compute
   the identical picture (ii) computes, one pixel at a time.
3. **Cost is unconditional.** 224 × 320 = 71,680 FSM steps per frame, paid on the hot path where today we
   render-and-discard a vectorized per-line resolve. `resolve_line` hoists per-line work (`sprite_line`,
   `window_span`, `plane_hscroll`, `ASlotCtx`) out of the dot loop by construction (`render.rs:993-1013`);
   a streaming rewrite un-hoists all of it.
4. **It re-times the sprite pipeline.** The dot-overflow budget, masking carry and collision latch are
   per-line commits (`render.rs:1444-1451`); splitting them per pixel changes when polled status bits
   flip — the same objection as (i), one order of magnitude worse.
5. **Clean-room policy.** `vdp.rs:9-11`: *"no emulator source informs this code (clean-room, audit
   policy 3)."* Matching Exodus's *answers* where evidence supports them is fine; adopting its
   *architecture* is exactly what that policy forbids.

Registered as considered-and-declined; if a future item ever needs true per-pixel semantics (the CRAM dot,
mid-line register latches), it re-opens on its own merits and with its own design pass.

---

## B. mclk → pixel-x mapping

### The line's geometry, derived from constants in this tree

| Quantity | Value | Anchor |
|---|---|---|
| Master clocks per scanline (NTSC) | `MCLK_PER_LINE = 3420` | `vdp.rs:17` |
| Readable-H counter positions per line | 422 (H40) / 342 (H32) | `vdp.rs:327` (`h_counter`), `vdp.rs:1377-1380` (`dot_at_h`) |
| Active display width | 320 (H40) / 256 (H32) | `render.rs:536-546` (`active_display`), `render.rs:990` |
| Pixel clock | mclk/8 (H40) / mclk/10 (H32) | hardware; see the cross-checks below |
| **Active display span** | **2560 mclk in both modes** | 320 × 8 = 256 × 10 = 2560 |
| Blanking span | 860 mclk (`3420 − 2560`) = 122.9 CPU cycles | matches the demand side's own figure, `HBLANK-WINDOW-SWEEP-RESULTS.md:368` |

Active display begins **at the line boundary** in our model: `h_counter` maps `dot = mclk % 3420 = 0` to
position 0, and readable H `$00` is active-display start on hardware. So there is no pre-active region
inside line N — the pre-active blanking a client would call "just before line N" is the tail of line N−1
(§"boundary cases").

### The mapping

Given a CRAM write at absolute master clock `m`:

```
line N = (m % MCLK_PER_FRAME) / MCLK_PER_LINE          [same derivation as v_counter, vdp.rs:345]
d      = m % MCLK_PER_LINE                              [0 .. 3419]
ppc    = 8 if the row was resolved h40, else 10         [MCLK per pixel]
x      = clamp(d / ppc, 0, width)                       [integer division = floor]
```

`x` is the first pixel that shows the **new** colour; pixels `0 .. x` keep the pre-write CRAM. Equivalently
`x = min(d * width / 2560, width)`, which is the single mode-independent form.

Four properties worth naming:

- **`d = 0` ⇒ `x = 0` ⇒ the whole row is recoloured.** Correct: events drain before the step
  (`system.rs:970-975`), so a write at exactly `N * MCLK_PER_LINE` happens after line N was resolved but
  before any of its pixels are consumed.
- **`d ≥ 2560` ⇒ `x = width` ⇒ row N is untouched**, and the write takes full effect from row N+1 — which
  is exactly today's behaviour. **The new model changes nothing for any write outside `d ∈ [0, 2560)`.**
- **A write "in the blanking before active-start of line N"** is, in this clock, a write with `d ≥ 2560`
  on line **N−1**: no effect on row N−1, whole-row effect on row N. Also exactly today's behaviour.
- **Writes on non-active lines (224..261)** are never journalled against a pending row; CRAM simply
  carries into the next line-0 resolve. Unchanged.

`floor` rather than `ceil` is chosen for consistency with the existing derivation style (`h_counter` uses
`(dot * positions) / MCLK_PER_LINE`, `vdp.rs:327-328`), and because the ±1 px it decides is two orders of
magnitude below the instruction-granularity limit below.

### Why the pixel clock, and not the 422/342 H-counter grid

Our `h_counter` divides the line **uniformly** into 422 (H40) / 342 (H32) positions (`vdp.rs:325-329`).
For H32 that is exact: 3420 / 342 = 10 mclk per pixel. For **H40 it is an approximation** — 3420 / 422 =
8.104 mclk per position — because the real H40 line is not a uniform divide (the EDCLK mix; the tree
already names the same class of approximation as **F-SLOTGRID** at `vdp.rs:558-560`). Carried onto the
pixel axis it would put active-display end at `d = 2593` instead of 2560, i.e. up to ~4 px of error at
the right edge, with no evidence behind it.

Two independent cross-checks say the 8/10 model is the right one:

1. **The demand side's blanking width.** They compute `3420/7 − 320*8/7 = 122.9` cycles
   (`HBLANK-WINDOW-SWEEP-RESULTS.md:368`) — i.e. the 8-mclk pixel — and their whole window arithmetic
   ([15.21, 21.5]) rests on it.
2. **Their measured landing pixel.** They place Aeon's HInt-dispatched CRAM burst at **253.6 cycles into
   line 100 ⇒ pixel ≈ 222 of 320** (`HBLANK-WINDOW-SWEEP-RESULTS.md:410-413`). Run through the mapping
   above: `253.6 × 7 = 1775 mclk`, `1775 / 8 = 221.9` → **x = 221**. The 422-position grid gives 219.
   The recommended mapping reproduces an independently derived number to within its own rounding.

**Named decision B-1.** The pixel axis uses 8 mclk/px (H40) and 10 mclk/px (H32), i.e. a 2560-mclk active
window; `h_counter` / `dot_at_h` are **not** touched. This leaves a knowingly inconsistent pair of
in-line clocks (HV reads say active ends at H `$9F` ≈ 2593 mclk; the pixel axis says 2560). Registered as
**F-SUBLINE-HGRID**: re-derive `h_counter`'s H40 grid from the same anchor. Explicitly **out of this
arc** — `h_counter` feeds `$C00008` reads, `hblank()`, `hint_offset()` and `vint_offset()`
(`vdp.rs:352-380`, `:1365-1371`), so moving it is an observable behaviour change on every ROM, which is a
different currency conversation from an opt-in capture.

**Named decision B-2.** `ppc` is chosen from the **resolved row's own `h40` flag** (`LineReport.h40`,
`render.rs:1407`), not from the live register. A mid-line H40→H32 switch would otherwise place `x` on a
grid the row was not drawn on. This mirrors the existing rule that `mode` comes from the answering
frame's width, not from a register read (`docs/2026-08-19-scanline-readback.md:47-52`).

### Resolution limit — instruction-granular, and say so

`MegaDriveBus` takes `now_mclk` **by value**, constructed once per CPU step from `scheduler.now()`
(`bus.rs:587`, `:654`, `:674`; `system.rs:1099` `let now = self.scheduler.now();`). It is therefore
**frozen for the duration of one 68000 instruction** — `vdp.data_write_at(value, self.now_mclk)`
(`bus.rs:945`) stamps the instruction's *start*, and every word of a `movem` or a 68k→VDP DMA
(`run_mem_dma`, `bus.rs:983-1000`) shares one stamp. The conformance ledger already names this:
*"writes carry no h-position, and the mclk is instruction-granular"*
(`docs/2026-07-25-testrom-conformance.md:39`).

Consequence, stated as a limitation rather than smoothed over:

- A boundary lands at the pixel of the **start** of the writing instruction, so it can read up to one
  instruction early — 8–26 px at H40 for a typical `move.w #imm,(An)` (20 cycles = 140 mclk = 17.5 px).
- That is still a **~15–40× improvement** on the current quantum (one whole 320-px row), and it is
  finer than the effect Aeon is measuring: their three `move.w (a1)+, VDP_DATA` words are separate
  instructions 30 cycles = 210 mclk = **26 px** apart (`HBLANK-WINDOW-SWEEP-RESULTS.md:205-207`), so
  each word gets its own distinct boundary.
- Registered as **F-SUBLINE-ACCESSMCLK**: stamp the write with the access instant inside the instruction
  (the bus already knows the /DTACK wait it returned) rather than the instruction start. Out of scope;
  it is a strict refinement of the same seam.

### Interaction with the V-counter phase difference (recon R2)

`v_counter` increments **at** the line boundary; hardware increments mid-line at H `$84`→`$85` (H32) /
`$A4`→`$A5` (H40). `vdp.rs:339-343` names this in its own doc comment as a known sub-line phase
difference and a documented open item.

In this clock: external H `$A5` is `dot_at_h(0xA5) = 0xA5*2*3420/422 = 2674` mclk, i.e. **746 mclk ≈ 107
CPU cycles before our line boundary** (the acceptance doc's independently computed "~103 CPU cycles",
`docs/2026-08-19-aeon-acceptance-results.md:161-164`, from the 420-position figure).

**Does the fix change where HV-polled fixtures' boundaries land? Yes — and this is the load-bearing
currency consequence, not a side note.** For a fixture that spins on `$C00008` until V ≥ N and then
writes (which is exactly `testrom::build_cram_midframe`, `testrom.rs:442`, poll `:474-486`, write
`:534-536`):

| | today | under (ii) | Exodus |
|---|---|---|---|
| row N | entirely colour A | **A up to x, B after** | entirely colour B |
| row N+1 | entirely colour B | entirely colour B | entirely colour B |

So fixing (a) alone does **not** make us agree with the GUI oracle on HV-polled fixtures; it converts a
whole-row disagreement into a partial-row disagreement over the row's first `x` pixels, because
difference (b) — the V-counter phase — is untouched. On Exodus the poll exits ~107 cycles *before* the
line boundary, so the write lands with `d < 0` (previous line's tail) and recolours row N whole.

**Named decision B-3.** The V-counter phase (recon R2) is **out of this arc**. It is a `v_counter`
change, i.e. a change to what `$C00008` returns for every ROM, in both frozen-currency-adjacent read
paths (`bus.rs:933`) — a behaviour change, where this arc is an opt-in capture change. Registered/carried
as **F-VCOUNT-PHASE** (recon R2's existing open item). A design that shipped both would close the Exodus
gap for HV-polled fixtures; shipping (a) first is still strictly correct on its own terms, and Aeon's
fixtures are **HInt-dispatched**, where (a) is the *only* difference
(`docs/2026-08-19-aeon-acceptance-results.md:166-168`).

---

## C. Scope of sub-line state

Recommendation: **CRAM writes only.** Everything else stays a line-start sample. Each exclusion is a
named decision with a reason, not an omission.

| # | Decision | State |
|---|---|---|
| **C-1** | **CRAM word writes are sub-line.** Every path that reaches `Vdp::write_target`'s `Target::Cram` arm (`vdp.rs:730-738`): CPU data-port writes (`apply_data_write` `:1084-1085`), the CD5 fill trigger (`:1073-1074`), a CRAM fill body (`run_fill` `:1121-1126`), and 68k→VDP DMA words (`dma_write_word` `:937`). | **IN** |
| **C-2** | **Registers stay line-start.** A mid-line write to reg 7 (backdrop index), reg 0/1 (blank column, display enable), reg 11/12 (scroll mode, S/H, H32/H40), reg 2/4/5 (bases) affects the row it lands in only from the next line. | **OUT** |
| **C-3** | **VSRAM / HSCROLL-table / VRAM stay line-start.** These are resolve-stage inputs (§0); making them sub-line means segmenting the *resolve*, which is option (i)'s cost with option (i)'s blast radius. | **OUT** |
| **C-4** | **The CRAM dot artefact stays unmodelled.** On hardware a CRAM write during active display paints the written value *at the beam position* regardless of which index any pixel resolved to. That is a different mechanism from re-decoding resolved indices, it is what `cram_flicker` and `direct_color_dma` actually test, and it is already registered as **F-CRAMDOT** (`docs/2026-07-25-testrom-conformance.md:877-879`, rows `:38-39`). This arc neither implements nor blocks it. | **OUT** |
| **C-5** | **Shadow/highlight state stays line-start.** `PixelState` is resolved in the same pass as the index (`resolve_dot`, `render.rs:956-981`), so a mid-line reg-`$0C` bit-3 change is a resolve-stage input, same as C-2. Note this is *also* what F-SCANLINE-SH would surface per pixel — see §G. | **OUT** |
| **C-6** | **One DMA burst = one boundary.** A 68k→CRAM palette DMA writes many words under a single frozen `now_mclk` (§B), so all of its words land at one `x`. On hardware a 32-word palette DMA spreads across ~1 slot/word. Registered as **F-SUBLINE-DMASPREAD**; the honest reading is that a DMA'd palette change is located to the DMA's start pixel, not smeared. | **OUT** |
| **C-7** | **Z80 CRAM writes** reach the VDP through the mirror (`z80::bus::vdp_mirror_read`/write path, `bus.rs:745`) during `catch_up_z80` (`system.rs:996`); they are journalled by the same choke with whatever mclk that path carries. No special handling, and no ROM in the corpus is known to do it. | IN by construction, untested |

The narrow scope is the demand's own scope. Aeon: *"a landing inside a row, i.e. the row rendered from the
CRAM state as it evolves across the row"* (`docs/2026-08-19-aeon-acceptance-results.md:183-185`).

---

## D. Consumer / latency audit

Under (ii) the **content** of row N changes only when a CRAM write lands in `d ∈ [0, 2560)` of line N; the
**delivery instant** of every row moves from `N × 3420` to `(N+1) × 3420`. Row *indices* and row *order*
are unchanged. Full consumer set, grepped:

### Sinks that actually receive rows (`wants_scanlines() == true`)

| Sink | Anchor | Exposure |
|---|---|---|
| `ScanlineCapture` — the only production one | `crates/oracle-core/src/scanline_capture.rs:134`; `wants_scanlines` `:137`, `on_scanline` `:141`, `on_frame_boundary` `:166` | **Content only.** Delivery order is unchanged, so the `line <= prev` resync guard (`:149-153`) and the `LastFrame` latch (`:172`) behave identically. |
| `MagicLines` (test oracle) | `crates/oracle-core/tests/scanline_capture.rs:149-154` | Keys off `line == 0` / `line == 223`; row indices unchanged ⇒ unaffected. |
| `BoundaryLog` | `crates/oracle-core/tests/scanline_capture.rs:216-227` | Logs the interleaving; see the moved test below. |
| `Spy` | `crates/oracle-core/src/bus.rs:3180-3206` | Unit-test spy over all hooks. |

Composites all forward and OR their capability queries — `&mut S` `bus.rs:184-206`, `Option<S>`
`:217-249`, `Observe<S>` `:276-298`, `Fanout<A,B>` `:338-367`. Every run-driver that carries a capture is
a `Fanout` with the capture first: `crates/oracle-frontend/src/main.rs:1587/1597/1600/1607/1610`,
`crates/oracle-aether/src/engine.rs:645/678/720`, `crates/oracle-aether/src/host.rs:659`,
`crates/oracle-aether/tests/hosted.rs:70`. `crates/oracle-replay/src/runner.rs:549` carries **no** capture
— oracle-replay never touches this path.

Sinks with `wants_scanlines() == false` are structurally blind to all of this: `Watchpoints`
(`watchpoints.rs:881`), `AudioSink` (`synth/audio_sink.rs:260`), `VgmLogger` (`vgm.rs:276`),
`StopWhen<F>` (`bus.rs:402`), `()` (`:417`), `Vec<BusEvent>` (`:422`), and the test/example sinks
(`bus.rs:1268`, `system.rs:2478`, `tests/watchpoints.rs:452`, `tests/conformance_roms.rs:328/376`,
`examples/k4_openbus_probe.rs:137`). `AudioAndWatch` (`crates/oracle-frontend/src/audio.rs:304`) has no
scanline half at all.

### Readers of the capture

| Reader | Anchor | Verdict |
|---|---|---|
| `blit_capture` (frontend blit) | `crates/oracle-frontend/src/main.rs:503`, sum-check `:511-513`, width from the last line `:515`, called once per frame right after `run_frames_with_sink(1, …)` `:1620` | **Safe.** `run_frames(1)` ends on a frame-boundary mclk, so `pixels()` was just latched at line 224 — after row 223 was flushed. The sum check (`lines()` tail widths == `pixels().len()`) holds because the delivery *count* per frame is unchanged. |
| `store_from_capture` | `crates/oracle-aether/src/engine.rs:3538`, same reader byte-for-byte (`:3520-3536`) | Same verdict, same reason. |
| `Engine::framebuffer` → `scanlines` / `screenshot` / `state_hash` | `engine.rs:1066` (rectangularity re-check `:1068`), handlers `:1721` (`scanlines`, slice `:1750`), `:1781` (`screenshot`), `:1590` (`state_hash`, `includeFramebuffer` at `:1613`, `framebufferSource` at `:1622`) | **Safe.** All read `last_frame`, i.e. a *completed* frame; `latch_screen` (`:746`) runs only at run-step ends. |
| `ScanlineCapture::pixels()` mid-frame | `scanline_capture.rs:119`, doc `:112-118` | **Cannot observe a partial frame at all** — a partially-filled frame lives in `building` and is never exposed. The one-line lag is therefore invisible to every `pixels()` reader. |
| `sh_probe` example | `crates/oracle-core/examples/sh_probe.rs:172-184`, width inferred as `px.len() / 224` `:177` | **Safe but fragile** — it reads after `run_frames(2)`. Its width inference already ignores `lines()`; unchanged by this arc, noted because it is the one reader that would mis-render a ragged frame. |
| `pixel_attribution` | `render.rs:1343`; handler `engine.rs:1485` → `:1510`; `frontend/src/lens/mod.rs:235`, `pick.rs:94`, `main.rs:2538` | **Diverges further, by design.** It is a live *post-hoc* resolve of current VDP state, not a capture read, so it already disagrees with `scanlines` on any mid-frame effect (its own first normative bullet says so). After this arc it will also disagree *within* a row. That is F-SCANLINE-INDEX's territory (§G), not a regression. |
| `render_line` post-hoc callers | `engine.rs:1072/1075` (the stateRender fallback), `frontend/src/main.rs:971/2289`, examples, and the golden/conformance tests | **Untouched.** Nothing in this design changes `render_line`; the stateRender path stays exactly the blind post-hoc render the contract says it is. |
| `frame_report` | `render.rs:1231` | No production consumer (only `bus.rs:2744/2754`). |

### Ordering: what breaks, precisely

`crates/oracle-core/tests/scanline_capture.rs:233`
(`frame_boundary_fires_exactly_once_per_frame_after_the_last_active_line`) asserts the exact delivery log
equals `[Line(0)..Line(223), Boundary(f)] × 3` (`:246-264`). Under (ii) that log is **unchanged**: row N−1
is emitted at line N's event, and row 223 at line 224's event *before* `on_frame_boundary`. The emission
order is preserved by putting the flush first in the handler — that ordering is a **hard requirement of
the design**, not an implementation detail, and slice 3's test must pin it.

The one test that genuinely moves:

- `crates/oracle-core/tests/scanline_capture.rs:347`
  `a_run_ending_between_the_last_active_line_and_the_boundary_defers_it_to_the_next_run` — today that
  window yields **224 lines / 0 boundaries**; under (ii) it yields **223 lines / 0 boundaries**, because
  row 223 is flushed at the line-224 event that the run stopped short of. The test's *thesis* survives
  intact (the deferral is real and the next run completes it); its **number** moves by one. Expected,
  named, and the fix is a one-line edit with the reason in the assertion message.

Everything else in that file holds: `:54` (224 lines in order), `:79`, `:90` (`LastFrame` latch), `:119`
(`All` never latches), `:141` (magic-line equivalence), `:269` (`3 × (224+1)`), ~~`:309` (reset resync)~~,
`:382`, `:409` (`lines().len() == 448`). So do `bus.rs:3234/3269/3320/3365` (composite forwarding) and
`system.rs:1574` (the R10 carry by line 223 — which (ii) deliberately does not move).

> **CORRECTION (2026-08-19, at slice 3 implementation — this audit line was wrong).** `:309`
> `last_frame_resyncs_after_a_reset_that_interrupts_a_frame` **does not hold**: it ends its first run at
> `100 * MCLK_PER_LINE`, i.e. mid-frame, and then asserts `sink.lines().len() == PARTIAL_LINES` (100).
> Under (ii) that is **99** — the identical mechanism as `:347` above (a run that stops before line K's
> event has not yet delivered row K−1), which this paragraph simply failed to apply to the second
> mid-frame-stopping test in the same file. Measured, not predicted: `left: 99, right: 100`.
>
> So **two** pinned counts move in slice 3, not one. The second was edited the same way — `PARTIAL_LINES
> as usize - 1` with the reason in the assertion message (`scanline_capture.rs:316` after the edit) — and
> the deviation from the "one authorized pin edit" instruction was surfaced for ratification rather than
> applied silently. **Ratification granted** (controller, 2026-08-19): the edit is well-founded, this
> audit line is the error, and no pixel byte, hash literal or golden moved in either case.
>
> Method note for later slices: an audit of "which pinned assertions move" should be generated by
> enumerating the tests that *end a run mid-frame*, not by reading down the file.

### Retained state must not become machine state

`System` derives `PartialEq` **and** `bincode::Encode/Decode` (`system.rs:167-169`), and
`frame_boundary_is_state_neutral` (`tests/scanline_capture.rs:269`) asserts
`assert_eq!(plain, tapped)` — *the whole machine* — between an unarmed run and a run with a
scanline-wanting sink attached. A naively-added `pending_row` / `cram_journal` field would make an armed
run that ends mid-frame leave residue and violate that neutrality claim (whole-frame runs would stay
green, which is worse: the invariant would be false and untested).

**Named decision D-1.** The retained row + snapshot + journal are **render scaffolding, not machine
state**, exactly as `vdp.rs:4-5` already rules for render output (*"the rendering output stays derived,
not state (nothing render-related serializes)"*). They live behind a wrapper type whose `PartialEq` is
constant-true and whose `Encode`/`Decode` round-trip as empty — so the checkpoint byte format is
**unchanged** (`System::restore`, `system.rs:690`, keeps reading old snapshots) and every
whole-machine equality assertion keeps its meaning. `reset` (`system.rs:443`/`:466`) clears it, matching
`Engine::invalidate_screen`'s existing rule that `reset`/`reload_rom`/`restore` drop the retained frame
(`engine.rs:755`, `:2196/:2224/:2369`).

**Named decision D-2.** The scaffolding must persist **across runs** (it cannot be a `run_until_with_sink`
local): a run that ends mid-frame would otherwise drop the pending row entirely, leaving the frame one
row short and silently failing `blit_capture`'s sum check (which returns `None`, i.e. a stale frame stays
on the glass — `main.rs:506-513`). Persisting it in `System` behind D-1's wrapper is the only shape that
satisfies both.

---

## E. Currency inventory

Baseline established firsthand on this worktree at `678ed96`: `cargo test --workspace` **exit 0**, and
across **all 40 `test result` lines**: **1588 passed, 0 failed, 4 ignored, 0 measured, 0 filtered out.**
(Aggregate, not a tail excerpt.) That is the state any slice below must return to.

### E.1 The frozen currencies do **not** move — verified, not assumed

`state_hash` hashes **VDP memory + registers only**: VRAM → CRAM → VSRAM → REGS, in Oracle's byte order
(`crates/oracle-core/src/state_hash.rs:14-17`, order fixed `:41-43`; five outputs `:32-38`; driven from
`system.rs:697-704`). `export_state`'s region list is `version → m68k regs → work RAM → Z80 RAM → Z80 regs
→ VDP → FM → PSG → SRAM` (`system.rs:708-714`), and its "VDP" region is *the same four*
(`system.rs:762-768`).

> **No rendered pixel enters either hash.** Confirmed for this arc: `state_hash` (5 hashes),
> `export_state`, `export_state_hash`, `crates/oracle-core/tests/export_state_v1.rs:47`
> (`GOLDEN_HASH = 0xBF5D_1E1A_A727_143B`, over `testrom::build()` which drives no VDP port), and
> `crates/oracle-core/tests/determinism_gate.rs:19-20` are all **immune by construction**, because this
> design changes neither CRAM/VRAM/VSRAM/register contents nor any bus timing (no cycle count, no
> /DTACK wait, no FIFO drain instant is touched).

Note the one thing that *does* fingerprint pixels on the bus: `emulator/state_hash
{"includeFramebuffer": true}` (`crates/oracle-aether/src/engine.rs:1613`, `framebufferSource` `:1622`).
Its in-tree tests (`crates/oracle-aether/tests/hosted.rs:310/324/335/343`,
`crates/oracle-aether/tests/methods.rs:387`) pin only *provenance* (`== "raster"`) and *self-equality*
across two reads — **no literal framebuffer hash is pinned anywhere**, so nothing there moves.

`crates/oracle-core/tests/golden_frames.rs` (6 pinned scene hashes, `:292/:298/:305/:313/:318/:324`)
builds static `Vdp` scenes by hand and hashes post-hoc `render_line` (`:107-121`) — **no run loop, no
CRAM write "inside" a line**. Immune.

### E.2 `crates/oracle-core/tests/scanline_goldens.rs` — the live-raster scorecard, per ROM

17 ROMs (`:90-108`), all at `FRAMES = 120`, `SEED = 0x1234_5678` (`:78`, `:87`); one inline `BASELINE`
table (`:120-184`) compared in a single assert (`:308-314`). Six rows pin a live hash
(`LIVE-DIFFERS frame_hash=0x…`); eleven pin an *equality* (`IDENTICAL-TO-POST-HOC` — no literal, the
equality **is** the pin). The verdict below uses each row's own measured `cause:` note, which is the
strongest static evidence available: the notes were produced by splitting every VDP-port write into
"during active display" vs "during vblank" (file header `:116-119`).

| ROM | pin | line | verdict | why |
|---|---|---|---|---|
| `color_1536` | `LIVE-DIFFERS frame_hash=0x917371f07409cb25` | `:130` | **MOVES** | its own cause note: 645 active-display writes/frame from line 48, *"387 of them register writes (R1/R19) and **the rest CRAM**"* — ~258 CRAM writes per frame inside active display is exactly the class this arc re-times. The 1536-colour trick **is** mid-scanline CRAM. |
| `io_sample` | `LIVE-DIFFERS 0xe5e133a2b8f9fe93` | `:141` | does not move | cause = *"328 **VRAM** writes per frame during active display"* — resolve-stage, excluded by C-3. |
| `m68k_opcode_sizes` | `LIVE-DIFFERS 0xfb9783a5ab564eb4` | `:151` | does not move | cause = *"~34 **VRAM** writes per frame during active display"* — C-3. |
| `shadow_highlight` | `LIVE-DIFFERS 0xfd6f02e7574d67f5` | `:160` | does not move | cause = *"**zero** VDP writes during active display"*. |
| `vdp_sprite_masking` | `LIVE-DIFFERS 0xce1c5a0559088d5d` | `:172` | does not move | cause = *"**zero** VDP writes in either phase"* (stale sprite carry). |
| `window_distortion` | `LIVE-DIFFERS 0xdf5bae342cc03667` | `:181` | does not move | cause = *"exactly ONE VDP-port write during active display per frame — **R17**"* — a register, excluded by C-2. |
| `cram_flicker` | `IDENTICAL-TO-POST-HOC` | `:132` | does not move **(low-risk TAG)** | `conformance_roms.rs:86-91`: *"leaves the screen blank and hammers palette **indices 4 and 36** (never index 0 …) 16× per active line"*. It writes CRAM 16× per active line — but to indices **no pixel samples**, so every segment decodes the same unchanged index and the row is unchanged. Its whole artefact is the CRAM dot, which C-4 keeps out. |
| `direct_color_dma` | `IDENTICAL-TO-POST-HOC` | `:133` | **CANNOT TELL STATICALLY — TAG** | `conformance_roms.rs:96-98` pins it *"because a whole frame's 44,352 CRAM words (all DMA, **all into index 0**) land inside ONE inter-line window in our model"* — i.e. **this row's justification is literally the assumption this arc removes**, and unlike `cram_flicker` the target index is one the picture can use. Must be measured. |
| `fm_test`, `gfx_joystick`, `m68k_bcd`, `m68k_illegal`, `m68k_memory_test`, `vcounter`, `vdp_port_access`, `vdp_test_register`, `window_test` | `IDENTICAL-TO-POST-HOC` | `:134`, `:135`, `:143`, `:144`, `:145`, `:162`, `:163`, `:174`, `:183` | does not move **(low-risk TAG, 9 rows)** | None is characterised as writing CRAM during active display, and an `IDENTICAL` row can only flip if a CRAM write both lands in `d ∈ [0,2560)` **and** touches a sampled index **and** is reverted before the next line-start (otherwise it would already be `LIVE-DIFFERS` today). Cheap to confirm with the same active-vs-vblank instrumentation the file's cause notes came from. |

**Counts: MOVES 1 · does-not-move 14 (5 mechanism-proven, 9+1 low-risk) · cannot-tell-statically 1.**

Structural gates in the same file: `scanline_golden_scorecard` `:272` (the one big diff),
`baseline_covers_every_rom` `:320`, `the_live_hash_depends_on_the_pixels` `:349` (perturbs
`window_distortion`'s split row at `111*width + 200`, `:360` — unaffected), and
`the_baseline_actually_pins_live_coverage` `:376` — `assert!(pinned >= 6)` at `:382`. That floor is
satisfied today by exactly the 6 `LIVE-DIFFERS` rows; this arc can only *raise* the count, never lower
it, so the floor holds either way.

### E.3 `crates/oracle-core/tests/conformance_roms.rs` — only one row reads the live capture

Verified at the dispatch (`:672-689`): **`color_1536` alone** is scraped through `frame_hash_scanline`
(`:661-665`, `:282-289`, `ScanlineCapture(Retain::LastFrame)`). Every other row — including
`cram_flicker` (`:681-684`) and `direct_color_dma` (`:685`) — goes through `scrape_visual` →
`frame_hash` (`:263-269`), a **post-hoc** `render_line` sweep.

- **MOVES (1):** `color_1536`, `:83` `frame_hash=0x917371f07409cb25`. It is the **same measured value**
  as `scanline_goldens.rs:130`, and the cross-check is documented at `scanline_goldens.rs:125-128` —
  so the two literals must be re-pinned **together, to the same new value**, or the cross-check silently
  stops being one.
- **Does not move (11 `frame_hash` pins + 4 glyph hashes):** `cram_flicker` `:93`,
  `direct_color_dma` `:100`, `fm_test` `:102`, `gfx_joystick` `:105`, `m68k_opcode_sizes` `:133`,
  `shadow_highlight` `:137`, `vcounter` `:139`, `vdp_test_register` `:213`, `window_distortion` `:217`,
  `window_test` `:221`, plus `TICK_TICK`/`TICK_CROSS`/`PASS`/`FAIL` (`:628-631`, via `block_hash`
  `:400` over `render_line`). All post-hoc.
  *(This corrects a natural first reading: `cram_flicker` and `direct_color_dma` are the two ROMs most
  exposed in `scanline_goldens.rs`, and simultaneously the two whose `conformance_roms.rs` hashes are
  **not** exposed at all.)*
- **Prose, not a hash:** if `direct_color_dma`'s live picture does change, the caption
  `"NOT-RENDERABLE (sub-scanline CRAM)"` and the ledger row at
  `docs/2026-07-25-testrom-conformance.md:39` (and L1b) become partly wrong prose over a still-correct
  hash. That is a doc edit in the same arc, not a re-golden.

### E.4 The suite gate that breaks structurally, and what it becomes

`crates/oracle-aether/tests/scanlines.rs:302`, `a2_two_timings_differ_and_the_boundary_moves`. Today it
boots `build_cram_midframe(50)` and `(150)` (`:303-304`) and compares rows against
`flat_row(width, hex)` (`:73`) — a **uniform** 256-pixel string. Under (ii) the fixture's write lands
partway across its own row (§B), so a boundary row is no longer flat and **four assertion sites fail**:

| site | today | under (ii) |
|---|---|---|
| `:327-331` `rgb_of(&ra, 50) == black` | flat black | **split**: black up to `x`, white after → FAILS |
| `:337` `rgb_of(&rb, 150) == black` | flat black | **split** → FAILS |
| `:341-352` band `for line in 51..=150` (ROM-150 rows must be flat black) | holds | row **150** is now split → FAILS |
| `:355-361` outside-band equality over `[0, 25, 50, 151, 200, 223]` | holds | line **50** is split in ROM-A and flat black in ROM-B → FAILS |

Unaffected: `:309-316` (`source == "raster"`), `:317` (`mode == "h32"`), `:322` (rows 40 vs 160 differ),
`:332-336` (`row 51` flat white), `:338` (`row 151` flat white), and the ROM-A half of the band loop.

**What it becomes — and it becomes strictly stronger.** The rewritten gate should assert, for
`build_cram_midframe(L)`:

1. rows `< L` flat colour A and rows `> L` flat colour B (the timing claim, unchanged in substance);
2. **row `L` is split**: its first pixel is A, its last pixel is B, and it contains exactly one A→B
   transition — which is a *poison a line-atomic renderer cannot pass*, i.e. the gate now discriminates
   sub-line liveness the way the current one discriminates line liveness;
3. the transition column `x` **moves with `L`** only within the tolerance §B predicts — see the
   flakiness warning below;
4. the band/outside-band structure with line `L` removed from the flat-equality list and line `L-1`
   added.

**Flakiness warning, and it is a real one.** The exact `x` is *not* a round number and is CPU-timing
sensitive: the poll loop is `move.w (a3),d0` / `lsr.w #8,d0` / `cmpi.b #L,d0` / `bcs` ≈ 48 cycles
(`testrom.rs:476-485`), so the iteration that first observes `V ≥ L` starts anywhere in a ~336-mclk
window, and the CRAM data write follows ~58 cycles (~406 mclk) later. Analytically, **`x ≈ 40–75` of
256** at H32 (`d ≈ 406–742` mclk, ÷10). Worse, a frame is `896_040 / 7 = 128_005.71` CPU cycles — **not
an integer** — so the loop's phase relative to the line drifts slightly frame to frame. **A gate that
pins `x` to an exact column is asking to be flaky.** Slice 4 must pin a *band* plus the structural
facts, and must state which frame it reads. Measuring the real value and its frame-to-frame spread is a
**TAG** (controller-run, or a plain in-tree test at slice time — no emulator MCP needed).

`build_cram_midframe`'s **ROM bytes do not change**: this is a rustdoc correction only
(`testrom.rs:422-427`). *"rows above the boundary in A, rows at and below it in B … the boundary sits at
`line + 1`"* becomes: *rows above `line` are wholly A, **row `line` is split at the pixel the write
landed on**, and the first wholly-B row is `line + 1`.* The `+1` claim survives — restated as "first
**fully** B row" — which matters, because that same `+1` is asserted at `bus.rs:91-95`,
`testrom.rs:425-427` and `scanlines.rs:298-300`, and all three want the same restatement.

### E.5 Everything else that touches rows

- `crates/oracle-core/tests/scanline_capture.rs` — **no hash literals**; pins structure. One test moves
  (`:347`, §D). The `256*224` / `448` length assertions (`:58`, `:83`, `:338-339`, `:423`) hold
  **because the design emits exactly one `on_scanline` per line with a complete row** — segments are
  internal to the emitter and never reach the sink. That is a hard interface constraint, not an
  incidental property.
- `crates/oracle-core/tests/watchpoints.rs:469-482`, `:565-575` — pins **bus-access counts** over five
  vendored ROMs (corpus total `[1, 10, 14020, 9329]`). No pixels, no new bus accesses. Immune.
- `crates/oracle-aether/tests/pixel_attribution.rs:296`
  (`rgb_and_cram_index_equal_what_render_line_produces_at_the_same_dot`) — ties the attribution reply to
  post-hoc `render_line`; **both sides are post-hoc**, so it is immune. Its *meaning* narrows: after this
  arc, `pixel_attribution` and `emulator/scanlines` can disagree *within* a row, not merely between
  rows. Worth one sentence in that test's doc comment.
- `crates/oracle-core/tests/io_controllers.rs:21` — post-hoc `render_line(112)` on `build_pad_poll`.
  Immune.
- `crates/oracle-aether/src/png.rs:419-421` — a PNG-encoder golden over a synthetic gradient. Immune.
- No snapshot infrastructure exists anywhere (`0` `.snap`, `insta`, `expect_file`, or `include_str!`
  goldens under `crates/*/tests`): **every golden in this repo is an inline `const` in the test that
  asserts it**, which is what makes the per-ROM justification discipline enforceable at all.

### E.6 Contract follow-through — required in the same arc

The `emulator/scanlines` §6 blockquote in `empyrean` now states the line-start convention **as live
normative-adjacent prose**, merged at `112d683` (`contract/protocol.md:1165-1189`). Three of its
sentences become false the moment this behaviour ships:

| `protocol.md` | today's text | after |
|---|---|---|
| `:1167-1170` | *"A row is a sample of VDP state taken at that row's own line-start, and it is **atomic** … nothing re-samples CRAM, scroll or the mode registers part-way across a row."* | Scroll and the mode registers still don't; **CRAM does.** |
| `:1172-1177` | *"A write that lands during line N **cannot change row N** … the change first appears in row N+1."* | A CRAM write inside line N's active window changes row N **from its landing pixel onward**; the first *fully* changed row is N+1. Non-CRAM writes keep the old rule. |
| `:1178-1181` | *"A mid-row landing is **not expressible** … this surface resolves a landing time to one scanline, never to a pixel within one."* | Expressible for CRAM, to instruction granularity (§B). |

`:1183-1189` — *"This catalog does **not** pin the intra-line sampling point … Pinning the sampling point
normatively … is left to a future amendment"* — **stays true and is the reason this is cheap**: the
catalog deliberately left room for exactly this, so a conformant client was already told not to depend
on the atomic reading. The prose must still be corrected: a reference server whose own documented
convention is stale is worse than one that never documented it.

**Vehicle question for the controller (§G Q1).** `112d683`'s own merge message records that the
clarification landed *without* a numbered CR or §11 entry precisely because *"it adds no behaviour,
changes no wire shape, touches no schema fragment"*. This edit **does add behaviour** to the reference
server, so the same reasoning points the other way — likely a numbered CR + §11.15 entry. Neither the
wire shape nor the schema fragment changes (still `rows[].rgb`, still 33 fragments), so it is a *server
behaviour* amendment, not a protocol one.

**External fixture.** Aeon's sweep re-runs as the acceptance protocol, unchanged in form
(`aeon/tools/hblank_window_sweep.py`; A1 verbatim, A2 restated per
`docs/2026-08-19-aeon-acceptance-results.md:56-61`). Two of its numbers are the *acceptance criteria*
for this arc, and both are predictions this design can be held to:

- **`flipX` becomes a direct measurement** instead of the constant `0` it was at all 201 sampled N
  (`HBLANK-WINDOW-SWEEP-RESULTS.md:311-332`). Predicted value for their row-100 fixture: **≈ 222**
  (§B's cross-check).
- **Restated-A2 distinct pictures over N ∈ 0..57 rise from 4.** Each spin iteration is ~10 CPU cycles
  = 70 mclk = **8.75 px** at H40, so consecutive N should be distinguishable and the count should
  approach the sample count (58 at step 1, ~19 at step 3) until the landing walks out of active
  display. "≥ 30 over N ∈ 0..57 step 1" is a defensible acceptance number; anything near 4 means the
  arc did not land.

---

## F. Slice plan

Ordered so **every intermediate state is green**, and so the two currency-moving events (the `a2` gate
rewrite; the `color_1536` re-pin) happen in named, isolated slices rather than as fallout. Red-first
discipline throughout: each slice writes the failing test first, records the mutation that proves it is
not vacuous, and states the gates.

**Standing gates for every slice** (the shipping-slice set, `docs/2026-08-19-scanline-readback.md:142-151`):
`cargo fmt --check`; `cargo clippy --all-targets --workspace -- -D warnings`; the same with
`--no-default-features`; `cargo test --workspace` reporting **every** `test result` line, with the
aggregate compared against the 40 / 1588 / 0 / 4 baseline in E.

### Slice 1 — the mclk reaches the CRAM choke *(prerequisite; discharges the hard half of `F-TRACE-VDPWRITE-MCLK`)*

- **Goal.** The VDP knows "now" at the moment it performs a write. Nothing observable changes.
- **Files.** `crates/oracle-core/src/vdp.rs` (a `now_mclk` shadow set by the already-timed entry points
  `data_write_at` `:1178`, `control_write` `:780`, `run_fill` `:1095`, `run_copy` `:1149`, and by the
  bus around `run_mem_dma`'s `dma_write_word` loop, `bus.rs:983-1000`); `crates/oracle-core/src/bus.rs`
  (pass the DMA's `now` in).
- **Tests-first.** A unit test that a CRAM write through `data_write_at(w, m)` records `m`, and one that
  a `run_mem_dma` burst records the DMA's `now` for **every** word (pinning C-6 as a decision, not an
  accident). Mutation: drop the DMA arm → the burst test fails.
- **Expected currency movement.** **None.** No behaviour reads the field yet.
- **Note.** Stamping `VdpWrite` itself with the same field completes `F-TRACE-VDPWRITE-MCLK` and is a
  two-line follow-on; keep it out of this slice unless the controller wants both (§G Q4).

### Slice 2 — the pixel-x mapping, as a pure function

- **Goal.** `mclk → x` (§B) exists, is tested, and is used by nobody.
- **Files.** `crates/oracle-core/src/render.rs` or `vdp.rs` — one `pub(crate) fn subline_x(d_mclk: u64,
  h40: bool) -> usize`, plus a `MCLK_PER_ACTIVE = 2560` const beside `MCLK_PER_LINE` (`vdp.rs:17`) with
  the derivation (320×8 = 256×10) in its doc comment.
- **Tests-first.** Table tests: `d=0 → 0`; `d=2559 → 319` (H40) / `255` (H32); `d=2560 → width`;
  `d=3419 → width`; and the **evidence check** — `253.6 CPU cycles → x = 221`, citing
  `HBLANK-WINDOW-SWEEP-RESULTS.md:410-413`. Mutation: swap `floor` for a `*422/3420` grid → the 221
  check fails at 219.
- **Expected currency movement.** **None** (dead code path).
- **Also lands here:** the `F-SUBLINE-HGRID` follow-up registration, in the const's doc comment.

### Slice 3 — deferred emission with an *empty* journal *(the neutrality slice)*

- **Goal.** Rows are emitted one line later, from the retained report + a line-start CRAM snapshot, with
  the journal always empty. **Every byte on every sink is unchanged.** This is where the whole
  currency-neutrality claim is made and tested, in isolation from any behaviour change.
- **Files.** `crates/oracle-core/src/system.rs` (`deliver_event`'s `Scanline` arm `:1031-1072`: flush
  pending → render → stash; the `run_until_with_sink` arming `:966-967`); the D-1 wrapper type
  (constant-true `PartialEq`, zero-byte `Encode`/`Decode`); `reset` clears it (`system.rs:443/:466`).
- **Tests-first.**
  1. **The neutrality poison**: boot a vendored ROM twice, capture with today's code path vs the
     deferred one, assert the two live frame hashes are **byte-identical** for a ROM with no
     active-display CRAM writes. (In practice: the whole `scanline_goldens` scorecard must stay green
     unchanged — which is the real assertion.)
  2. **Ordering**: `scanline_capture.rs:233`'s exact-interleaving log must still be
     `[Line(0)..Line(223), Boundary(f)] × 3`. Mutation: flush *after* `on_frame_boundary` → it fails.
  3. **Checkpoint compatibility**: a `restore` of a snapshot taken before this slice still decodes, and
     `System: PartialEq` between an armed and an unarmed whole-frame run still holds
     (`scanline_capture.rs:269`).
- **Expected currency movement.** **None** — and that is the slice's entire claim. The one test that
  *does* move is `scanline_capture.rs:347` (224 → 223 lines in the between-last-line-and-boundary
  window, §D), edited here with the reason in the assertion message.

### Slice 4 — the journal, the segments, and the behaviour

- **Goal.** CRAM writes inside `d ∈ [0, 2560)` split the row.
- **Files.** the journal (fed from the per-step VDP write drain, `system.rs:990-992`, arming widened to
  `wants_vdp_writes() || wants_scanlines()`); the segmented emitter (walk a CRAM working copy from the
  snapshot, decode each span with `pixels_rgb`'s existing map so the two cannot drift —
  `render.rs:1045-1052`).
- **Must include, or it is wrong:**
  - **Journal coalescing by `x`.** `direct_color_dma` pushes **44,352 CRAM words in one instruction**
    (`conformance_roms.rs:96-98`), all sharing one mclk under C-6. Without coalescing that is a ~700 KB
    per-line `Vec` and 44 k zero-length segments. Entries with the same `x` collapse into one segment.
    Test it with a synthetic 40 k-write burst and assert the segment count is 1.
  - A `debug_assert!` that the working CRAM after the last segment equals live CRAM at emit time — the
    cheapest possible guard against a missed journal path (Z80 mirror, a new DMA arm).
- **Tests-first.** A `build_cram_midframe`-style unit fixture at core level asserting one row is split
  with the first pixel A and the last pixel B; the zero-write case still bit-identical (slice 3's
  poison, re-run).
- **Expected currency movement.** **This is the slice that moves things**, and exactly two named things:
  the four `scanlines.rs` assertion sites (E.4) and `color_1536`'s two hash literals (E.2/E.3). Neither
  may be regenerated silently — see §G Q2. The `a2` rewrite lands **in this commit**, because leaving it
  red for even one commit violates "every intermediate state is green".
- **Doc edits in the same commit:** `testrom.rs:422-427`, `bus.rs:91-95`, `scanlines.rs:298-300`.

### Slice 5 — measure the corpus, justify each move, re-pin

- **Goal.** Turn E.2's three TAGged categories into measurements, one row at a time.
- **Shape.** Run `scanline_goldens` and record which rows moved. For **each** mover: state the
  mechanism (which CRAM writes, at which lines, to which indices), confirm it is the mechanism this arc
  intends, and only then re-pin — with the reason in the `cause:` comment beside the row, in the style
  the file already uses. A row that moves for an unexplained reason is a **bug report, not a re-pin**.
- **Expected currency movement.** `color_1536` (both copies, same value); `direct_color_dma` and
  possibly `cram_flicker` flipping `IDENTICAL-TO-POST-HOC → LIVE-DIFFERS frame_hash=0x…`; the 9 low-risk
  rows expected to stay put and each confirmed. If any of the 9 moves, **stop and explain before
  re-pinning**.
- **Also here:** `docs/2026-07-25-testrom-conformance.md` rows `:38-39` and ledger L1a/L1b get their
  prose corrected, and `docs/2026-08-19-scanline-readback.md:186-189`'s *"`cram_flicker` is pinned
  IDENTICAL-TO-POST-HOC and is **not** a liveness discriminator — do not substitute it"* is re-checked
  against the new measurement.

### Slice 6 — contract + acceptance

- **Goal.** The prose stops being wrong, and the demand side re-runs.
- **Shape.** The `empyrean` `contract/protocol.md:1165-1189` correction in whatever vehicle §G Q1
  rules; then Aeon re-runs `hblank_window_sweep.py` against the new server, with `flipX` and the
  restated-A2 distinct-picture count as the two acceptance numbers (E.6).
- **Expected currency movement.** None in this tree; a schema re-vendor only if the ruling adds a
  fragment (it should not — no wire shape changes).

---

## G. Open questions needing a controller ruling before slice 1

Ranked by how much rework a late answer causes.

1. **Q1 — Is the behaviour change authorised at all, and in which contract vehicle?** This arc makes the
   reference server's documented §6 convention false (`contract/protocol.md:1167-1181`, merged 8 hours
   before this doc as `112d683`). The catalog explicitly declines to pin the intra-line sampling point
   (`:1183-1189`), so no *conformance* rule breaks — but a client written against the reference
   server's stated convention does. **Ruling wanted:** plain prose correction (the vehicle `112d683`
   used) vs a numbered CR + §11.15 entry (the vehicle *"it adds no behaviour"* excluded, which no longer
   applies).
2. **Q2 — Golden re-generation policy for `color_1536`.** The standing rule is that goldens never
   regenerate silently and every movement is per-ROM justified and owner-visible. `color_1536`'s hash is
   pinned **twice** (`scanline_goldens.rs:130`, `conformance_roms.rs:83`) with a documented cross-check
   (`scanline_goldens.rs:125-128`). **Ruling wanted:** confirm both literals may be re-pinned to one new
   measured value in slice 5, with the mechanism written into the `cause:` comment — and confirm the
   cross-check must be preserved rather than dropped.
3. **Q3 — May `IDENTICAL-TO-POST-HOC` rows flip to `LIVE-DIFFERS`?** For `direct_color_dma` (and
   possibly `cram_flicker`) the *shape* of the baseline row changes, not just a hash — the pin stops
   being an equality and becomes a literal. That is a strictly better pin (it captures a real effect the
   corpus exists to catch), but it is a structural baseline amendment. **Ruling wanted:** allowed with
   per-row justification, or is a flip a blocker pending separate review?
4. **Q4 — Fold `F-TRACE-VDPWRITE-MCLK` in, or keep it adjacent?** Slice 1 does the hard half (the mclk
   reaches the choke). Stamping `VdpWrite` and letting the trace recorder / watchpoints report it is
   ~2 lines plus tests, and it retires a follow-up registered three times over
   (`docs/2026-08-14-trace-recorder-design.md:1010-1014`, `:1064`, `:1099`;
   `watchpoints.rs:33`, `:821` — where the caveat text currently *names* the gap on the wire).
   **Ruling wanted:** in-arc (one extra slice) or separate.
5. **Q5 — What tolerance does the rewritten `a2` gate pin?** §E.4 shows the exact split column is
   CPU-phase-sensitive and drifts frame to frame (a frame is 128 005.71 CPU cycles). A band assertion is
   robust; an exact column is a stronger gate and a flake risk. **Ruling wanted:** band + structure
   (recommended), or exact column with a pinned frame index.
6. **Q6 — Does `pixel_attribution` need to follow?** It answers from a live post-hoc resolve
   (`engine.rs:1510`), so after this arc it disagrees with `emulator/scanlines` *within* a row, not only
   between rows. Nothing breaks; the question is whether the divergence gets documented (one sentence)
   or closed (which is `F-SCANLINE-INDEX`, a separate item). **Recommendation:** document only.

### Not blocked, but named

- **`F-SUBLINE-HGRID`** — `h_counter`'s uniform 422-position H40 grid disagrees with the 8-mclk pixel
  axis by ~33 mclk at active-end (§B, decision B-1). Out of arc: it moves `$C00008` reads.
- **`F-VCOUNT-PHASE`** (recon R2, `vdp.rs:339-343`) — until this lands, HV-polled fixtures still
  disagree with the GUI oracle, now over a row's first `x` pixels instead of a whole row (§B,
  decision B-3).
- **`F-SUBLINE-ACCESSMCLK`** — instruction-start stamping bounds the resolution at one instruction
  (§B). A refinement of the same seam.
- **`F-SUBLINE-DMASPREAD`** (decision C-6) — a DMA burst lands at one pixel, not smeared across the
  slots it really occupies.
- **`F-CRAMDOT`** (`docs/2026-07-25-testrom-conformance.md:877-879`) — **unblocked-adjacent, not
  advanced.** This arc gives the CRAM write an h-position, which is half of what F-CRAMDOT's own
  description asks for (*"Timestamp CRAM writes with an h-position and advance the clock through a DMA
  body"*); the other half (the dot painted at the beam position regardless of resolved index, and the
  DMA-body clock advance) it deliberately does not do (decisions C-4, C-6). So F-CRAMDOT gets cheaper
  and stays open. `cram_flicker` and `direct_color_dma` remain its rows.
- **`F-SCANLINE-INDEX` / `F-SCANLINE-SH`** — **this arc makes both markedly cheaper and neither is
  folded in.** They were held out because *"the renderer resolves indices and S/H internally and hands
  the sink only RGB"* (`docs/2026-08-19-scanline-readback.md:225-228`). Slice 3 already **retains the
  per-line `Vec<PixelResolution>`**, which carries `cram_index` and `state` per pixel
  (`render.rs:403-412`) — i.e. exactly the two fields those follow-ups need, already alive at emit time
  and already surviving to the sink boundary. What remains for them is a sink-interface extension and a
  §6 fragment, not a renderer change. Deliberately out of scope: adding fields to `on_scanline` is a
  contract-shaped change with its own four-surface accounting.

---

## Verification note

**Docs-only change on this branch** — one new file under `docs/`, nothing under `crates/` (checked with
`git show --stat`). No emulator MCP tooling was touched, and nothing in this pass required a live boot.

The one command run against the tree was `cargo test --workspace`, to establish the baseline in §E:
**exit 0, 40 `test result` lines, 1588 passed / 0 failed / 4 ignored** — reported as the aggregate over
all lines, not a tail excerpt.

Every code statement above is from source read in this tree at `678ed96`. Every measurement quoted from
the demand side is theirs, from
`/home/volence/sonic_hacks/aeon/docs/benchmarks/scanline-p2/HBLANK-WINDOW-SWEEP-RESULTS.md`.

**Three things this pass could not settle from the code and did not paper over**, carried as TAGs for a
controller-run foreground check (none needs the emulator MCP; a plain `cargo test` at slice time
answers all three):

1. Whether `direct_color_dma`'s live hash moves (E.2) — its pin's stated justification is the exact
   assumption this arc removes.
2. Whether any of the 9 low-risk `IDENTICAL-TO-POST-HOC` rows moves (E.2).
3. The measured split column `x` for `build_cram_midframe(50)` and its frame-to-frame spread (E.4) —
   analytically `x ≈ 40–75` of 256, which decides Q5.
