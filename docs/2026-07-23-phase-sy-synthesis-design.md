# Phase SY — Native Sound Synthesis: Design & Recon (2026-07-23)

**Status:** design pass only. No core-logic edits, no commits. Build verified green (`cargo build`
→ `Finished dev` in 0.92s; oracle-core + oracle-frontend both compile).

## Goal

Turn the *live sound-chip register stream* (YM2612 FM + SN76489 PSG) that our core already captures
into **PCM audio samples produced INSIDE the core**, so the emulator can make sound itself without the
external `vgm2wav` step. Hard requirements:

- **OFF BY DEFAULT / opt-in.** The proven capture/VGM path and currency-neutrality (byte-identical
  `state_hash` / `export_state` / A-B) must be untouchable by this feature.
- Do **not** design a windowed audio-device frontend — only the **core-side sample-production API**
  plus a way to prove it (render N seconds to `.wav` via an example, mirroring `vgm_capture`).

---

## 1. Register-data inventory — what we already have vs. what synth needs

### What is tapped today

Every sound-chip **write** is already emitted onto the bus-event stream and decoded, from *both*
windows (Z80 and 68k), classified on `addr` alone (fc-agnostic):

| Chip | Z80 window | 68k window | Where tapped |
|---|---|---|---|
| YM2612 FM | `$4000-$4003` | `$A04000-$A04003` | `z80/bus.rs` write arm; `bus.rs::store_byte` / `emit` |
| SN76489 PSG | `$7F11` | (`$C00011`, decoded but 68k PSG unmapped — PSG is Z80-only in practice) | `z80/bus.rs` write arm |

The decode (`vgm.rs::VgmLogger`) produces a normalized `VgmRecord { chip, port(bank), reg, value,
frame }` for **every FM data write and every PSG byte** — not just timer registers. Confirmed against
the RT-3 A/B run: the capture spans the full FM register space actually used by the driver — key-on
`$28`, per-operator params `$30-$9F`, fnum/block `$A0-$A6`, algorithm/feedback `$B0-$B6`,
stereo/AMS/FMS `$B4-$B6`, LFO `$22`, DAC `$2A/$2B`, timer/ch3-mode `$27`. Nothing in the register
*value* stream is dropped.

### What the `Ym2612` struct retains vs. what the tap carries

`ym2612.rs` is a deliberately narrow model: it latches ONLY `$24/$25/$26/$27` (Timer-A/B period +
control) and the two address latches, to drive the Timer-A overflow flag the SMPS sequencer polls.
**It intentionally ignores all synthesis registers.** That is fine — synthesis does **not** read from
`Ym2612`. Its data source is the **BusEvent stream / VgmRecord decode**, which carries the full
register file. So there is no need to widen `Ym2612`; the timer model stays exactly as-is.

### What synth needs — coverage check

- **YM2612 synth** needs: LFO (`$22`), timer/ch3-special (`$27`), key-on/off (`$28`), DAC
  (`$2A/$2B`), per-operator DT/MUL/TL/RS/AR/D1R/D2R/SL/RR/SSG-EG (`$30-$9F`), fnum/block
  (`$A0-$A6` + ch3 extra `$A8-$AE`), FB/algorithm (`$B0-$B6`), L/R/AMS/FMS (`$B4-$B6`). **All present
  in the tap.** ✅
- **SN76489 synth** needs: every self-describing byte (tone period lo/hi latches, 4-bit attenuation,
  noise control). **All present in the tap.** ✅

### The ONE real data gap: sub-frame timing

`BusEvent` carries `op / fc / addr / size / value` — **no timestamp**. The only time signal a sink
gets is the **frame index** via `on_step_boundary` (735-sample / 1-60s granularity). So today we know
*which frame* each write landed in and *the order within the frame*, but **not the exact mclk** of a
write within a frame.

- **Impact:** a first synth must batch each frame's writes at the frame boundary (exactly what our
  VGM render already does via `0x62` 735-sample waits). This produces recognizable, correct-pitch,
  correct-timbre audio, but quantizes write timing to 60 Hz. Fast intra-frame sequences (DAC PCM
  streams for drums, rapid arpeggios) lose sub-frame placement.
- **This does NOT block a recognizable first result** — VGM playback through `vgm2wav` uses the same
  frame-granular waits and the user already judged it "pretty close." It *is* the same deferred
  sub-cycle timing that drives the ~8s A/B drift (see `docs/2026-07-23-rt3-oracle-ab-findings.md`).
- **Fix path (later slice):** add an mclk timestamp to `BusEvent` (the bus already has `now_mclk` at
  every emit site). Additive field, not hashed → currency-neutral. Lets the synth advance to the exact
  time of each write. Shared with the A/B sub-frame-timing work.

**Verdict: no register-value gap. The full FM+PSG register stream needed for synthesis is already
captured. The only limitation is frame-granular write timing, which bounds accuracy but not
audibility.**

---

## 2. Architecture

### 2.1 The consuming seam — reuse the proven caller-owned sink

The capture path is currency-neutral **by construction** because it is a caller-owned `BusEventSink`
threaded through the opt-in `run_frames_with_sink` seam; the default `run_frames` passes `&mut ()`
(the null path) and is byte-untouched. **Synthesis rides the exact same seam.** A new sink type
consumes the same `on_event` (writes) + `on_step_boundary` (frame ticks) callbacks the VgmLogger uses.
No `System` state, no core logic, changes. This is the single most important design decision: it
inherits the VgmLogger's currency-neutrality guarantee for free.

### 2.2 Module layout (all new, additive)

```
crates/oracle-core/src/synth/
  mod.rs             // re-exports; AudioSink; sample-format constants (44_100, i16 stereo)
  sn76489.rs         // hand-rolled PSG: 3 tone counters + LFSR noise + attenuation table
  ym2612_synth.rs    // FM: 6 ch × 4 op, sine table, EG, algorithms, fnum→phase, DAC ch6
  audio_sink.rs      // AudioSink: BusEventSink adapter owning both chip synths + output buffer
crates/oracle-core/examples/
  synth_render.rs    // mirrors vgm_capture: boot ROM, run N frames w/ AudioSink, write .wav
```

`synth` is **feature-gated** in `Cargo.toml`: `[features] synth = []` (default features do NOT include
it). Rationale: keeps the synth code — and any vendored FM core / added dep — out of the default build
that the currency gates compile, so the neutrality argument is "the code is not even present unless you
ask." Note the currency-neutrality guarantee comes from the **sink seam**, not the feature; the feature
is belt-and-suspenders isolation for a possibly-large vendored core.

### 2.3 The `AudioSink` — how it turns writes into a continuous sample stream

A pure `BusEventSink` is called only on writes and frame boundaries, but envelopes/LFO/phase must
evolve *continuously between writes*. The frame-batched model handles this cleanly:

```
impl BusEventSink for AudioSink {
    on_event(e):        // decode like VgmLogger; apply the reg write to the right chip synth's state
    on_step_boundary(pc, frame):
        // a new frame began → render the PREVIOUS frame's 735 samples at 44.1kHz
        // (all of that frame's writes are already applied), append to the output buffer.
}
```

- Reuse the VgmLogger's decode logic verbatim (latch-then-data for FM, self-describing for PSG) so the
  synth sees the identical `(chip, bank, reg, value)` triples the VGM path sees — same source of truth.
- Render granularity: **735 samples per NTSC frame @ 44_100 Hz** (matches VGM's `0x62`). The synth
  clocks its operators internally at the chip rate and decimates/accumulates to 44_100, OR runs
  natively at 44_100 with per-sample phase increments derived from fnum/block (simpler first cut).
- **Native sample rate:** target **44_100 Hz stereo i16** directly (avoids a resampler dep). The YM2612
  native rate is ~53_267 Hz (7_670_453 / 144); a later accuracy slice can run native + resample, but
  44_100-native phase-accumulation is the fastest audible route and needs no `rubato`/`dasp`.

### 2.4 Sample-production API (core stays headless)

`AudioSink` exposes a minimal pull API — no audio device, no threads in the core:

```
impl AudioSink {
    fn new(sample_rate: u32) -> Self;
    fn samples(&self) -> &[i16];        // interleaved L,R,L,R...
    fn drain(&mut self) -> Vec<i16>;    // take + clear (frontend pulls per callback later)
    fn len_frames(&self) -> usize;
}
```

For a future real-time frontend, swap the `Vec<i16>` for a lock-free ring buffer (`ringbuf` crate) so
an audio callback thread can pull while the emulator thread pushes — **out of scope here**; the Vec is
sufficient for the WAV example and keeps zero new deps in the first slice.

### 2.5 Proving it — the WAV example

`examples/synth_render.rs` mirrors `vgm_capture.rs` exactly: boot a ROM file (build artifact, never
committed), `run_frames_with_sink(N, &mut AudioSink::new(44100))`, then write a **hand-rolled 44-byte
WAV header + i16 PCM** (no `hound` dep — same "hand-roll the container, no dep" discipline as the
VGM writer). Run: `cargo run --release -p oracle-core --features synth --example synth_render -- s4.soundtest.bin 600`.
The listener test: does the WAV sound like the game? Cross-check against `vgm2wav` of the *same* run's
captured VGM (we already produce it) — the two should converge as accuracy improves.

### 2.6 Off-by-default mechanism — summary

Three independent layers, any one of which already makes it neutral:
1. **Caller-owned sink**: only present when a caller opts into `run_frames_with_sink` with an
   `AudioSink`. `run_frames` / `&mut ()` unchanged.
2. **Feature gate `synth` (default off)**: the module + any vendored core + deps are absent from
   default builds.
3. **Not in `state_hash` / `export_state`**: `AudioSink` is caller-owned, never part of `System`, so
   it cannot enter any currency. (If a later slice adds an mclk field to `BusEvent`, that field is
   additive and unhashed.)

---

## 3. Hand-roll vs. vendor/port the FM core — recommendation

### PSG (SN76489): **hand-roll.** Unambiguous.

~150 lines: three 10-bit tone counters toggling a ±output, a 15/16-bit LFSR for the noise channel, a
16-entry logarithmic attenuation table, sum the four channels. Fully documented, license-clean,
trivially testable (a 440 Hz tone reg-write → count zero crossings). No dependency.

### YM2612 (OPN2): the hard part. Options:

| Option | Accuracy | Effort | License | Notes |
|---|---|---|---|---|
| **A. Hand-roll minimal OPN2** | Recognizable, long-tail inaccuracy | High (~800-1500 LOC) | clean | sine table + EG + 8 algorithms + fnum→phase. Envelope rate tables & TL/SL/key-scaling are where subtle wrongness lives. |
| **B. Port ymfm (Aaron Giles)** | High (production emulator) | High (C++→Rust port) | **BSD-3 — compatible with our MIT** | The right *accuracy* target; used by MAME. |
| **C. Vendor Nuked-OPN2** | Cycle-accurate (gold standard) | Medium (C, well-scoped) | **LGPL-2.1 — friction in an MIT repo** | Best fidelity but license/linking friction; avoid unless isolated as a separate optional crate. |
| **D. Existing Rust crate** | varies | Low if one fits | check per-crate | e.g. `ymfm`-derived or `nuked-opn2` Rust ports exist on crates.io; vet license + fnum/DAC coverage before adopting. |

**Recommendation — hybrid, staged:**

1. **Now, for soonest audible output:** hand-roll (Option A) a *minimal* OPN2 — sine + 4-op algorithms
   + a serviceable ADSR EG + fnum/block phase. Accept missing LFO/SSG-EG/exact-rate-table nuances.
   This gets recognizable melody in-emulator with zero deps and no license question, and it's directly
   testable.
2. **For the accuracy path:** adopt **ymfm's model (BSD-3)** — port or vendor its OPN2 operator/EG
   into `ym2612_synth.rs` behind the `synth` feature. BSD-3 is license-clean in our MIT tree; ymfm is
   the reference-grade target that `vgm2wav`-class players use. **Avoid Nuked-OPN2 (LGPL) in-tree.**
3. **The de-risker we already own:** the RT-3 A/B result proved our register stream is
   *position-identical* to the reference for the first ~8s. So we can validate synth output against
   `vgm2wav` of the *same captured VGM* — a ground-truth waveform diff, not just an ear test. This
   makes the hand-roll→ymfm accuracy climb measurable at each step.

Prefer the approach that gets recognizable audio soonest (hand-roll) while leaving the accuracy path
open (ymfm port behind the same feature) — exactly the brief.

---

## 4. Incremental slice plan

### ★ Slice SY-1 (SMALLEST AUDIBLE) — PSG-only synth + AudioSink pipeline + WAV example

Hand-roll SN76489; build `AudioSink` (decode + frame-batched 735-sample render + Vec output); add
`synth_render.rs` WAV example; feature-gate `synth`. Produces **audible, correct square-wave/noise
output** for the PSG channels (SFX, PSG bass/melody parts). Proves the entire sink→synth→buffer→WAV
pipeline end-to-end on a chip simple enough to be provably correct.
**Difficulty: low. Risk: low.** (No FM complexity; PSG is fully specified.)

### Slice SY-2 — Minimal hand-rolled FM

Add `ym2612_synth.rs`: sine table, per-operator EG (ADSR from OPN rate tables), the 8 algorithms,
fnum/block→phase, key-on/off, TL/SL scaling, stereo pan. Skip LFO, SSG-EG, CSM, exact detune nuance.
Produces **recognizable in-emulator music** (the actual song, imperfect timbre).
**Difficulty: high. Risk: medium** (envelope-rate & level-scaling correctness is the long tail).

### Slice SY-3 — FM accuracy + DAC

Port ymfm-grade operator/EG; add LFO (`$22` AMS/FMS), DAC/PCM channel-6 mode (`$2A` stream → drums),
correct rate/key-scaling tables. Validate against `vgm2wav` of the same captured VGM (waveform diff).
**Difficulty: high. Risk: medium** (fidelity work; validation harness makes it measurable).

### Slice SY-4 — Sub-frame timing

Add an mclk timestamp field to `BusEvent` (additive, unhashed → currency-neutral); apply writes at
exact intra-frame times instead of batching at the frame boundary. Removes the 60 Hz quantization;
shares the investigation with the ~8s A/B drift.
**Difficulty: medium. Risk: medium** (touches the shared `BusEvent` struct, but additively).

### Slice SY-5 — Real-time frontend (out of scope for this design)

Swap `Vec<i16>` for a `ringbuf` lock-free buffer; add `cpal` audio output in `oracle-frontend`. Hear
it live in the window. Core API is unchanged from SY-1.

---

## 5. Feasibility / build sanity

- `cargo build` (default features): **PASS** — `Finished dev [unoptimized + debuginfo] in 0.92s`,
  both `oracle-core` and `oracle-frontend` compile clean at HEAD.
- **Dependencies the design adds:** *none* for slices SY-1/2/3 (hand-roll the WAV container like the
  VGM writer; no `hound`, no resampler — synth runs natively at 44_100). SY-5 would add `ringbuf` +
  `cpal` in the *frontend* crate only. A ymfm port (SY-3) vendors source rather than adding a crate.
- The `synth` feature keeps all of the above out of the default currency-gate build.

---

## 6. TL;DR

- **Data:** the full FM+PSG register *value* stream synthesis needs is **already captured** via the
  BusEvent tap / VgmLogger decode. `Ym2612` (timer-only) is not the data source and needs no change.
  The single gap is **sub-frame write timing** (`BusEvent` has no timestamp; only a frame index) —
  bounds accuracy, does not block audibility, fixable additively later.
- **Architecture:** new feature-gated `synth/` module; an `AudioSink: BusEventSink` reuses the proven
  caller-owned `run_frames_with_sink` seam (inheriting VgmLogger's currency-neutrality), decodes the
  same triples, frame-batch-renders 735 samples/frame at 44_100 Hz into an i16 buffer the core exposes
  by pull; a `synth_render.rs` example dumps a hand-rolled WAV like `vgm_capture` dumps VGM.
- **Chips:** hand-roll PSG (trivial, clean); hand-roll a minimal OPN2 for soonest audio, with a
  **ymfm (BSD-3)** port as the accuracy path — avoid Nuked-OPN2 (LGPL) in-tree. Validate against
  `vgm2wav` of the same captured VGM.
- **Smallest audible first slice:** **SY-1 = PSG-only synth + the AudioSink/WAV pipeline** — low
  difficulty, low risk, proves the whole path on a provably-correct chip.
