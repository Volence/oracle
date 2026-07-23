# Phase SY-3 — FM Accuracy (ymfm-grade) + DAC/PCM Drums: Decisions & Slice Plan (2026-07-23)

**Status:** planning/decision pass (overseer + Fable 5 consult). No core-logic edits in this doc.
Supersedes the SY-3 stub in `docs/2026-07-23-phase-sy-synthesis-design.md` §4 with a concrete,
sliced execution plan. Owner delegated the technical forks to a Fable 5 agent (owner asleep); the
three flagged items below are non-blocking for SY-3a/SY-3b and await owner ratification.

## Where SY-2 left off

`crates/oracle-core/src/synth/ym2612_synth.rs` is a **float (f32)** minimal OPN2: 6ch×4op sine-table
phase gen, an *approximate* ADSR envelope (`RATE_BASE`/`ATTACK_SPEED` calibration), all 8 algorithms +
op1 feedback, TL, L/R pan, `$2B` DAC-mute of ch6. It runs the phase generator at the **output** rate
(44.1 kHz), **not** the chip's native 53,267 Hz. Renders `s4.soundtest.bin` at 98.2% non-silent,
0.971 spectral corr / 100%-within-octave / ~69%-within-semitone vs `vgm2wav`. Deferred to SY-3:
DAC/PCM ch6 stream (`$2A`), LFO (`$22`), SSG-EG (`$90`), detune (`$30` DT), CSM/ch3-special, and the
**exact OPN2 envelope-rate + key-scale tables**.

Currency contract (unchanged, load-bearing): the synth is a caller-owned `BusEventSink` on the opt-in
`run_frames_with_sink` seam, behind the default-OFF `synth` feature — never in `System`/`state_hash`/
`export_state`. Every slice below MUST keep the default build byte-unchanged.

## Fork decisions (Fable 5, 2026-07-23)

**Fork 1 — ymfm port depth → DECISION: (B+) exact-integer rewrite in place, NOT a line-by-line port.**
Rewrite the operator/EG/PG internals in the OPN2-exact **integer** domain — 256-entry log-sin quarter
table, exp/pow table, 10-bit attenuation arithmetic, 64-entry EG increment table, exact key-scaling,
DT detune table, native ~53,267 Hz internal tick (EG clocked every 3 PG ticks) with a final resample
to 44.1 kHz. Use **ymfm (BSD-3) as the reference for table generation + semantics**, with unit tests
pinning our generated tables against known ymfm values. Keep our module structure / register decoder /
algorithm wiring / sink seam. Rationale: the audible gap is in the *data* (rates, detune, LFO, native
tick), not the architecture; a template transliteration of ymfm's `fm_engine` is high-risk and the
least-readable option, and buys only bit-exactness our spectral/pitch harness can't measure. Do **not**
stage a full port after B+. **Add a BSD-3 attribution header** to any file carrying ymfm-derived tables
("tables and semantics derived from ymfm, Copyright (c) Aaron Giles, BSD-3-Clause"). Nuked-OPN2 (LGPL)
stays forbidden — not even as an in-tree numeric fixture; use ymfm-generated fixtures only.

**Fork 2 — DAC drums vs. SY-4 sub-frame timing → DECISION: (A) render DAC at frame granularity NOW.**
Take each frame's ordered `$2A` writes and spread them evenly across the 735 output samples with
zero-order hold (matches the DAC ladder character), gated on `$2B`. The SMPS Z80 DAC loop streams at a
near-constant intra-frame rate, so even-distribution reconstructs very nearly the correct playback
rate — we smooth timing that was already nearly smooth. Do **not** gate drums on SY-4. Named failure
modes of (A), so we don't misdiagnose them as bugs: (1) onset quantization up to ~16.7 ms (under the
~30 ms flam threshold); (2) per-frame resample-rate wobble on *long* PCM samples (this is exactly what
SY-4 fixes); (3) occasional frame-boundary ZOH click (add a 1-sample ramp only if it appears). SY-4
(mclk timestamps on `BusEvent`) stays a separate later refinement — it touches the core seam and every
sink, a review burden that must not be coupled to the synth milestone.

**Fork 3 — slicing → DECISION: split into six independently-verifiable, independently-committable
sub-slices** (biggest audible jump first; the structural integer slice lands before features stack on
it). Gate tolerance: declare a spectral-corr regression only beyond **~0.005** (resample/dither jitters
the third decimal). From SY-3c on, add a second gate: **amplitude-envelope RMS-correlation** (~5 ms
windows) vs the `vgm2wav` render, since spectral corr is nearly blind to envelope *timing*.

| # | Slice | Scope | Success check |
|---|---|---|---|
| **SY-3a** | **DAC/PCM drums** | `$2A`/`$2B` path, frame-granular even-spread ZOH (Fork 2A); no FM-core changes | Drums audible in `s4.soundtest.bin`; non-silent% and spectral corr vs vgm2wav both **rise** (reference has drums → corr must strictly improve); default build byte-unchanged |
| **SY-3b** | **Integer core skeleton** | Replace f32 sine/amplitude path with OPN2 log-sin + exp tables, 10-bit attenuation, native ~53,267 Hz tick + resample to 44.1 k; keep *approximate* EG rates temporarily | Table unit tests pinned to ymfm fixtures (exact); spectral corr ≥ (−0.005 tol); within-octave pitch stays 100% |
| **SY-3c** | **Exact envelope generator** | 64-entry EG rate/increment table, EG tick every 3 samples, exact key-scaling (KS+rate→effective rate), exact attack curve, SL/RR exactness | Envelope-RMS-corr improves; within-semitone % improves (target: past ~69%); spectral corr ≥ |
| **SY-3d** | **Detune** | `$30` DT field via exact OPN detune table (per key-code); exact MUL edge cases | dt offsets pinned vs ymfm table; corr ≥; audible chorusing (ear note) |
| **SY-3e** | **LFO** | `$22` rate; per-channel AMS/FMS from `$B4`; exact LFO step table | LFO freqs vs documented OPN2 rates; corr ≥ |
| **SY-3f** | **SSG-EG + ch3 special** | `$90` SSG-EG shapes; ch3 per-op fnum (`$A8-$AE`); **CSM left a documented stub** | SSG shape inversion/loop tests; corr ≥; module deferred-list emptied (minus CSM) |

Every slice independently keeps: default build unchanged, `--features synth` builds + tests green,
spectral corr within the −0.005 gate. SY-3a is first on purpose — isolated own render path, lowest
risk, largest audible delta; if a session is cut short, drums are banked.

## Flags awaiting owner ratification (non-blocking for SY-3a/SY-3b)

1. **BSD-3 attribution header** on ymfm-derived-table files — overseer including it unconditionally
   (costs nothing, removes "informed by" vs "derived from" ambiguity). Owner may strip later.
2. **CSM permanently stubbed** vs eventually implemented — planning to stub (no Sonic-era SMPS content
   uses CSM).
3. If SY-3a's frame-granular DAC produces audible warble on any *long* PCM sample in real content, the
   fix is **pulling SY-4 forward**, not patching the synth — owner should ratify that sequencing if it
   triggers.
