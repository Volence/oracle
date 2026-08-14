# Phase SY-5 — Real-Time Audio in `oracle-frontend`: Design & Recon (2026-07-23)

**Status:** planning / recon pass only. **No source edits, no commits.** Docs-only, mirroring the
prior recon commits (`git show 500cc73` = SY-4 sub-frame-timing recon;
`docs/2026-07-23-phase-sy-synthesis-design.md`). Every factual claim below cites the `file:line` it was
read from. Build state at HEAD unchanged.

This doc makes the existing windowed player
(`crates/oracle-frontend/src/main.rs`) actually **play sound through the host speakers**, driven by the
already-complete `AudioSink` (`crates/oracle-core/src/synth/audio_sink.rs`), via a lock-free SPSC ring
buffer feeding a `cpal` audio-callback thread. It is **frontend-only** — zero edits to `oracle-core` (the
synth is complete through SY-4, `git show 500cc73`), zero currency surface.

---

## 0. TL;DR (for the impatient implementer)

- **The synth already exists and is complete.** `AudioSink::new(sample_rate)` (`audio_sink.rs:55`)
  renders `sample_rate / 60` interleaved-stereo `i16` per frame (`audio_sink.rs:58,163`),
  `drain() -> Vec<i16>` takes+clears (`audio_sink.rs:81-83`), `finish()` flushes the last frame
  (`audio_sink.rs:191-195`). SY-4 already landed the timed path (`on_event_at`, `bus.rs:59`). **SY-5 adds
  no core code — it only *consumes* what exists.**
- **Threading + buffer:** one `ringbuf::HeapRb<f32>` split into `(Producer, Consumer)`. Producer = the
  emulation main thread pushes each frame's `drain()`ed samples (converted `i16 → f32`). Consumer = the
  `cpal` output callback pops into the device buffer, writing `0.0` silence on underrun. Capacity
  **~4 video frames** of stereo (`≈ 4 · (rate/60) · 2` f32); the buffer absorbs the 60 fps-vs-device-clock
  drift so **no resampler is needed** for the first cut.
- **Sink integration:** when audio is on, **always** drive `run_frames_with_sink(1, sink)`
  (`system.rs:442`) — drop the null-sink fast path (`main.rs:312-317`). Use a tiny **composite sink** so
  the existing pixel-attribution watch (`main.rs:245-317`) keeps working alongside audio.
- **cpal config = take the device's own rate.** `AudioSink::new()` accepts *any* rate, so construct it at
  the **device default sample rate** and render natively at that rate — **no resampling, no pitch error,
  ever.** Request stereo `f32`; handle mono by averaging. **No output device → run video-only, never
  crash** (this env has no `/dev/snd`).
- **Feature:** add a frontend `audio` feature (pulls `oracle-core/synth` + `cpal` + `ringbuf`),
  **default-ON** (it is a *player*), but fully buildable with `--no-default-features`. Deps land **only**
  in the frontend; `oracle-core`'s default build is untouched.
- **Split into two commits: SY-5a** (ring buffer + composite sink + producer wiring, fully headless-
  testable) and **SY-5b** (the `cpal` stream/device glue, only audibly verifiable by the owner).

---

## 1. Ground truth — what already exists

### 1.1 The producer side is one method call away

The run loop today (`main.rs:312-319`) is:

```rust
// main.rs:312-319
if !paused || step {
    if watch_armed {
        sys.run_frames_with_sink(1, &mut watchpoints);   // :314
    } else {
        sys.run_frames(1);                               // :316  (null-sink fast path)
    }
    frame += 1;
}
```

`run_frames_with_sink<S: BusEventSink>(frames, sink)` (`system.rs:442`) threads a caller-owned sink
through the whole run; `run_frames` is exactly `run_frames_with_sink(frames, &mut ())` (`system.rs:435`).
So attaching audio is: keep a **persistent** `AudioSink` across loop iterations, run one frame into it,
then `drain()` that frame's PCM and push it to the ring producer.

### 1.2 `AudioSink` renders one frame per `run_frames_with_sink(1, …)` call

`AudioSink` renders **at frame boundaries** in `on_step_boundary` (`audio_sink.rs:217-235`): the first
boundary latches `last_frame` (`audio_sink.rs:219-221`), and each later boundary where `frame > prev`
renders every elapsed frame (`audio_sink.rs:223-232`). Because the run loop calls `on_step_boundary`
before every CPU step with `frame = scheduler.now() / MCLK_PER_FRAME` (`system.rs:478`), a single
`run_frames_with_sink(1, &mut sink)` advances the sink across exactly one boundary and renders exactly one
frame's worth of audio (`sample_rate / 60` stereo samples). Therefore:

```rust
sys.run_frames_with_sink(1, &mut audio_sink);   // advances one frame, renders it at the crossing
let pcm: Vec<i16> = audio_sink.drain();          // ~ (rate/60) stereo pairs = ~1470 i16 at 44.1 kHz
// push pcm into the ring producer …
```

**Critical:** the `AudioSink` must be created **once, outside** the loop and reused — its rendering is
driven by frame *advance*, so a fresh sink per iteration would only ever latch and never render. On quit,
call `finish()` (`audio_sink.rs:191`) once to flush the final in-progress frame (harmless if the ring is
already draining to a closed device).

### 1.3 `drain()` semantics (verified)

`drain()` is `std::mem::take(&mut self.out)` (`audio_sink.rs:82`) — returns the accumulated interleaved
`i16` and leaves the sink's buffer empty (test `drain_takes_and_clears`, `audio_sink.rs:400-408`). So
per-frame draining never re-emits old samples and never unbounded-grows the sink.

### 1.4 The synth is reachable from the frontend only under a feature

`synth` is a `oracle-core` feature, **default OFF** (`crates/oracle-core/Cargo.toml:8-12`), and
`pub mod synth` is `#[cfg(feature = "synth")]` (`lib.rs:19-22`) re-exporting
`synth::{AudioSink, DEFAULT_SAMPLE_RATE}` (`synth/mod.rs:31`). So the frontend must **turn on
`oracle-core/synth`** to see `AudioSink` — done through a frontend feature (§5).

---

## 2. Threading + the ring buffer (SY-5a)

### 2.1 Two threads, one lock-free SPSC queue

- **Producer (main thread):** the existing `while window.is_open()` loop (`main.rs:250`). Each iteration
  runs one emulated frame into the persistent `AudioSink`, `drain()`s it, converts `i16 → f32`, and
  `push_slice`s into the ring. Presentation stays throttled to 60 fps by
  `window.set_target_fps(60)` (`main.rs:231`) / `update_with_buffer` (`main.rs:336`).
- **Consumer (`cpal` callback thread):** `cpal` owns and drives this; it fires the data callback whenever
  the device needs `N` frames, and the callback `pop_slice`s from the ring, zero-filling any shortfall.

A **single-producer/single-consumer** (SPSC) lock-free ring is exactly the right primitive: one writer
(main), one reader (audio callback), no locks on the audio thread (a mutex in an audio callback is the
classic priority-inversion/glitch bug). The `ringbuf` crate is the standard Rust SPSC ring; its
`HeapRb::split()` hands out a `Producer` and a `Consumer` that are `Send` and move to their respective
threads.

### 2.2 Element type: `f32`, not `i16`

Store **`f32`** in the ring, converting on the producer side. Rationale:

- `cpal` output streams are overwhelmingly `f32` (see §3.3); an `f32` ring makes the audio callback a
  **pure memcpy** (`pop_slice` into the device buffer) with **no per-sample work on the real-time
  thread** — the conversion `s as f32 / 32768.0` happens on the *producer* (the non-real-time main
  thread), where a stall is only a dropped frame, never an audible glitch.
- Cost is 4 bytes/sample vs 2 — negligible at these sizes (a 4-frame stereo buffer is ~24 KB).

Conversion (producer): `let f = s as f32 / 32768.0;` for each interleaved `i16` from `drain()`.
`i16::MIN (-32768) / 32768.0 = -1.0` and `i16::MAX (32767)/32768.0 ≈ 0.99997`, i.e. cleanly in
`[-1.0, 1.0)` — the conventional `f32` PCM range cpal expects. (The synth already clamps to `i16` range
at mix, `audio_sink.rs:173-174`, so no `f32` value can exceed the range.)

### 2.3 Concrete types and split

```rust
use ringbuf::{HeapRb, traits::{Producer, Consumer, Split}};

// capacity in f32 samples (interleaved L,R,L,R…) — see §2.4
let ring = HeapRb::<f32>::new(capacity);
let (mut prod, mut cons) = ring.split();   // prod → main thread; cons → moved into the cpal callback
```

(`ringbuf` 0.4 exposes `push_slice` / `pop_slice` via the `Producer` / `Consumer` traits; `split()` comes
from the `Split` trait. Pin the version in Cargo.toml, §5, and adapt the exact import paths to the pinned
release.)

### 2.4 Capacity: ~4 video frames, with reasoning

Let `spf = rate / 60` **stereo pairs** per frame (`audio_sink.rs:58`); interleaved that is `2 · spf` f32
per frame (735 pairs / 1470 f32 at 44.1 kHz; 800 / 1600 at 48 kHz).

- **Too small (1 frame):** any scheduling jitter on either thread underruns → clicks.
- **Too large (10+ frames):** audio lags video by the buffer depth (a 10-frame buffer ≈ 167 ms of
  latency), noticeable as A/V desync.
- **Recommended: capacity = `4 · 2 · spf` f32 (≈ 4 frames).** ~4 frames of slack tolerates a couple of
  late producer ticks (the 60 fps throttle is wall-clock, not hard-real-time) while keeping end-to-end
  latency around **~50–67 ms** — fine for a debugging player. Round the capacity up to the next power of
  two if the pinned `ringbuf` prefers it (e.g. `8192` f32 ≈ 5.5 frames at 44.1 kHz) for a little extra
  headroom; the exact number is not load-bearing. Make it a named `const` (e.g. `RING_FRAMES: usize = 4`)
  so the owner can retune by one number.

### 2.5 Underrun (consumer) and overrun (producer)

- **Underrun — callback wants more than the ring holds:** `pop_slice` returns the count actually popped;
  **write `0.0` (silence) to the remaining tail of the device buffer.** A brief silence is an inaudible-
  to-mild click, and it is self-correcting (the producer refills next frame). Never block the callback.
- **Overrun — producer has more than the ring can hold:** `push_slice` returns the count actually pushed;
  **drop the remainder** (the device is draining slightly slower than we produce; the newest few samples
  are discarded). This is a **block-skip drop**, not a block — the producer must never spin/park waiting
  for the audio thread (that would stall video). Optionally count drops for a debug print (mirrors
  `Watchpoints::dropped()`, `main.rs:164`), but do not gate on them.

Both are **expected, rare, and bounded** under the drift model in §4 — they are the price of "no
resampler, no clock sync" and are the correct first-cut trade.

---

## 3. `cpal` configuration (SY-5b)

### 3.1 Device + config enumeration

```rust
use cpal::traits::{HostTrait, DeviceTrait, StreamTrait};

let host = cpal::default_host();
let Some(device) = host.default_output_device() else {
    eprintln!("no audio output device — running video-only");
    // continue with audio disabled; the loop still renders video (§3.4)
    return None;
};
let default_cfg = match device.default_output_config() {
    Ok(c) => c,
    Err(e) => { eprintln!("no default output config ({e}) — video-only"); return None; }
};
let sample_rate = default_cfg.sample_rate().0;   // Hz — feed THIS to AudioSink::new (§3.2)
let channels    = default_cfg.channels();        // usually 2
```

### 3.2 Sample rate — **take the device rate, don't fight it**

`AudioSink::new(sample_rate)` accepts **any** rate and computes `samples_per_frame = sample_rate / 60`
(`audio_sink.rs:55-58`). So the **simplest correct** design is:

> **Construct `AudioSink::new(device_default_rate)` and render natively at the device's rate.**

This means **no resampler and no pitch/speed error** regardless of whether the device runs at 44.1 kHz,
48 kHz, or anything else — the synth simply generates `rate/60` samples per emulated frame at that rate.
The only constraint is that `rate/60` be a sane integer sample count; 44100→735 and 48000→800 both work,
and the integer truncation of any non-multiple rate (e.g. 44101) loses at most a fraction of a sample per
frame, absorbed by the ring exactly like the drift in §4. **Do not** pass `DEFAULT_SAMPLE_RATE`
(`audio_sink.rs:23`) blindly — that constant is the *offline* WAV convention; for real-time playback the
device rate is the right rate.

*(Rejected alternative: request 44.1 kHz explicitly and resample if the device refuses. That adds a
resampler for zero benefit here, because the synth is itself the "resampler" — it renders at whatever rate
we ask. Named limitation of the chosen path: if a device *only* supports a rate where `rate/60` is a poor
integer, per-frame sample counts wobble by ≤1 sample; harmless and ring-absorbed.)*

### 3.3 Sample format + channels

- **Format:** request/expect `f32` (`default_output_config().sample_format()` is `F32` on effectively all
  modern hosts; the `f32` ring in §2.2 matches). If robustness across `i16`/`u16` device formats is
  wanted later it is a `match` on `sample_format()` with a per-format callback — **out of scope for the
  first cut; assume/require `f32`** and log-and-fall-back-to-silent if the device is non-`f32`.
- **Channels:** the synth emits **interleaved stereo** (`audio_sink.rs:173-176`). If the device is stereo
  (the common case) the callback is a straight copy. If `channels != 2`:
  - **Mono (`channels == 1`):** average each L,R pair → one sample (`(l + r) * 0.5`).
  - **>2 channels:** write L,R to the first two and `0.0` to the rest (or duplicate). Name this as a
    first-cut simplification. Simplest correct default: **request a stereo config when the device offers
    one** (`supported_output_configs()` filtered to `channels == 2` at the chosen rate), else fall back to
    the mono average. Do not crash on any channel count.

### 3.4 Building the stream and graceful no-device fallback

```rust
let config: cpal::StreamConfig = default_cfg.into();   // or a stereo-forced config, §3.3
let stream = device.build_output_stream(
    &config,
    move |out: &mut [f32], _| {
        let n = cons.pop_slice(out);      // consumer moved in here
        out[n..].fill(0.0);               // underrun → silence (§2.5)
    },
    move |err| eprintln!("audio stream error: {err}"),
    None,                                 // no timeout
).ok();                                    // Err → None → video-only
if let Some(ref s) = stream { let _ = s.play(); }
```

**No-device / build-failure path (required for this environment):** every fallible step above
(`default_output_device`, `default_output_config`, `build_output_stream`, `play`) returns
`Option`/`Result`; on **any** failure, print a one-line warning and **continue the frontend with audio
disabled** — the producer simply skips draining/pushing (or the ring is never created). The window and
input keep working exactly as today (`main.rs:250-340`). This env has **no `/dev/snd`**, so the
device-absent branch is the *default* here and MUST be clean, not a panic. Keep the `Stream` alive by
binding it to a variable that lives for the whole run (dropping a cpal `Stream` stops playback).

---

## 4. Sync / drift — why no resampler is needed (first cut)

Three independent clocks are in play:

1. **The emulation's audio clock:** *exactly* `rate/60` samples per emulated frame, by construction
   (`audio_sink.rs:58`; a run of N frames yields ~N·`spf` samples).
2. **The video presentation clock:** `window.set_target_fps(60)` (`main.rs:231`) paces the producer to
   ~60 wall-clock fps. **NTSC is actually ~59.92 fps**, so the emulator's "60" is ~0.13% fast vs true
   NTSC, and the 60 fps throttle is itself only approximate.
3. **The device DAC clock:** the sound card consumes at its own crystal rate, independent of the CPU.

These never agree to the last sample. **The ring buffer is the elastic band that absorbs the difference:**

- If the producer is momentarily *ahead* of the device, the ring **fills**; once full, `push_slice` drops
  the surplus (§2.5 overrun) — a few discarded samples, inaudible.
- If the producer is momentarily *behind*, the ring **drains**; if it empties, the callback emits silence
  (§2.5 underrun) — a rare, brief click.

At ~0.1% clock skew the ring crosses its slack (~4 frames) only every many seconds, so the audible
consequence is **an occasional faint click** — acceptable for a debugging player, and the standard
behavior of every "just play it" audio path that omits a resampler.

**Who drives the pace? Keep video driving (the 60 fps throttle), let the ring regulate.** Video-clocked
pacing keeps the existing loop, keyboard, and single-step behavior (`main.rs:250-340`) completely intact —
the audio thread is a passive consumer. **Named drift consequence:** rare underrun clicks under sustained
clock skew or a slow producer frame. **Named future fix (SY-6+, out of scope):** audio-clock-driven pacing
(block the producer on ring space so the *device* sets the frame rate) or a proper resampler with a
feedback loop on ring fill level. For the first cut, **do neither** — the ring alone is correct and
simplest.

---

## 5. Feature gating + dependencies (SY-5a)

### 5.1 The frontend `audio` feature

Add an `audio` feature to `crates/oracle-frontend/Cargo.toml` that pulls the core's synth plus the two new
frontend-only deps. Make it **default-ON** (see §5.3):

```toml
[dependencies]
oracle-core = { path = "../oracle-core" }
minifb = "0.28"
# Real-time audio (Phase SY-5) — frontend-only; pulls the core's opt-in synth + host audio + SPSC ring.
cpal    = { version = "0.15", optional = true }
ringbuf = { version = "0.4",  optional = true }

[features]
default = ["audio"]
# Turns on the core's native synth (AudioSink) and the host audio path.
audio = ["dep:cpal", "dep:ringbuf", "oracle-core/synth"]
```

- `oracle-core/synth` in the `audio` feature is what makes `oracle_core::synth::AudioSink` (`lib.rs:21-22`,
  `synth/mod.rs:31`) visible to the frontend.
- `dep:cpal` / `dep:ringbuf` + `optional = true` keep both **entirely absent** from a
  `--no-default-features` build.
- **Versions:** `cpal = "0.15"` and `ringbuf = "0.4"` are the current stable majors; pin them and adjust
  the §2.3/§3 API snippets to the exact release chosen (both crates have had API churn across majors —
  verify `HeapRb`/`split`/`push_slice`/`pop_slice` and `build_output_stream` signatures against the pinned
  docs during implementation).

### 5.2 All frontend code behind `#[cfg(feature = "audio")]`

The ring, the composite sink's audio arm, the cpal setup, and the per-frame drain/push live under
`#[cfg(feature = "audio")]`; a `#[cfg(not(feature = "audio"))]` path keeps today's exact loop
(`main.rs:312-317`, including the `run_frames` null-sink fast path). So `--no-default-features` reproduces
the current binary byte-for-behavior.

### 5.3 Default-ON, with reasoning

**Recommend `default = ["audio"]`.** This crate is a *player* — a player that is silent by default fails
its one job, and the module doc's "Audio is out of scope (milestone D3)" (`main.rs:9`) is precisely the
line SY-5 retires. Keeping it a *feature* (not unconditional) preserves: (a) a lean
`--no-default-features` build for CI/headless/currency contexts and for this no-`/dev/snd` environment,
and (b) a clean revert lever. The runtime no-device fallback (§3.4) means default-ON is safe even where no
sound card exists.

### 5.4 Confirming zero impact on `oracle-core`'s default build

- `cpal`/`ringbuf` are declared **only** in `oracle-frontend/Cargo.toml`; `oracle-core/Cargo.toml`
  (`:14-15`) gains **nothing**.
- The workspace uses `resolver = "2"` (`Cargo.toml:2`), so feature unification is **per-build-target**, not
  global. Building `oracle-core` on its own (`cargo build -p oracle-core`, the currency-gate build) does
  **not** see the frontend's `oracle-core/synth` activation — that only happens in a build that includes
  `oracle-frontend` with `audio` on. The currency gates compile `oracle-core` with default features
  (synth OFF), exactly as today.
- Even *with* synth on, currency is neutral by the three-layer argument the synth module already carries
  (caller-owned sink / feature gate / not in `System`, `synth/mod.rs:7-15`; SY-4 verdict, `git show
  500cc73`). SY-5 constructs `AudioSink` through the same public `run_frames_with_sink` seam
  (`system.rs:442`) and touches no `System` state.

---

## 6. Sink integration — audio + the existing watch (SY-5a)

### 6.1 The problem

`run_frames_with_sink` takes **one** sink (`system.rs:442`). Today the frontend passes either `Watchpoints`
(when a tile watch is armed, `main.rs:314`) or `()` (the fast path, `main.rs:316`). Audio needs
`AudioSink` attached **every** frame. Two options:

**(a) Audio-only sink, drop the null path.** When audio is on, always run
`run_frames_with_sink(1, &mut audio_sink)`. Simplest, but the pixel-attribution watch
(`main.rs:245-317`, `W`/`C`/click controls) would be **mutually exclusive** with audio unless disabled.

**(b) Composite sink (recommended).** A tiny two-field `BusEventSink` that forwards every trait method to
`AudioSink` **and** an optional `&mut Watchpoints`. Keeps *both* features live at once.

### 6.2 Recommendation: composite sink

`BusEventSink` has five methods (`bus.rs:52-83`): `on_event`, `on_event_at`, `on_step_boundary`,
`wants_vdp_writes`, `on_vdp_write`. A composite forwarding all five is ~30 lines and keeps the watch
tooling working. Shape (frontend-local, under `#[cfg(feature = "audio")]`):

> **Superseded 2026-08-14 (historical record kept as written).** The hand-written struct below shipped as
> described and behaved correctly; it has since been replaced, with identical behaviour, by the generic
> `oracle_core::bus::Fanout<&mut AudioSink, Option<&mut Watchpoints>>` — `AudioAndWatch` is now a type alias
> for it, and its members are the fields `a` / `b`. The trait has also grown `wants_scanlines`, `on_scanline`
> and `stop_requested` since this doc was written, which the generic combinator composes for free.

```rust
struct AudioAndWatch<'a> {
    audio: &'a mut AudioSink,
    watch: Option<&'a mut Watchpoints>,   // Some only while a tile watch is armed (main.rs:246)
}

impl BusEventSink for AudioAndWatch<'_> {
    fn on_event(&mut self, e: BusEvent) {
        self.audio.on_event(e);
        if let Some(w) = &mut self.watch { w.on_event(e); }
    }
    fn on_event_at(&mut self, e: BusEvent, mclk: u64) {
        self.audio.on_event_at(e, mclk);                  // AudioSink's SY-4 timed path (audio_sink.rs:202)
        if let Some(w) = &mut self.watch { w.on_event_at(e, mclk); } // Watchpoints rides the default fwd
    }
    fn on_step_boundary(&mut self, pc: u32, frame: u64) {
        self.audio.on_step_boundary(pc, frame);           // drives AudioSink's render (audio_sink.rs:217)
        if let Some(w) = &mut self.watch { w.on_step_boundary(pc, frame); }
    }
    fn wants_vdp_writes(&self) -> bool {
        self.watch.as_ref().map_or(false, |w| w.wants_vdp_writes())   // AudioSink wants none
    }
    fn on_vdp_write(&mut self, wr: oracle_core::vdp::VdpWrite) {
        if let Some(w) = &mut self.watch { w.on_vdp_write(wr); }       // AudioSink ignores VDP writes
    }
}
```

`BusEvent` is `Copy` (it is a small `#[derive(Clone, Copy, …)]` POD in `bus.rs`; verify at implementation)
so forwarding the same event to two sinks is a trivial copy. The run loop becomes:

```rust
// audio ON:
let watch_ref = watch_armed.then_some(&mut watchpoints);
let mut sink = AudioAndWatch { audio: &mut audio_sink, watch: watch_ref };
sys.run_frames_with_sink(1, &mut sink);
let pcm = audio_sink.drain();     // re-borrow after `sink` drops
// convert + push pcm to the ring producer
```

**Note the borrow ordering:** `AudioAndWatch` holds `&mut audio_sink` for the duration of the run; drain
after the composite drops. If the borrow checker fights the re-borrow, drain via the composite (add a
`fn drain(&mut self)` passthrough) or restructure so `audio_sink` is drained inside a scope. Trivial.

**Fallback if (b) is deemed over-scope for the first commit:** ship **(a)** and document the watch as
"disabled while audio is on (composite is a fast-follow)." But (b) is cheap and keeps the milestone-D3
watch tooling (`main.rs:30-38`) intact, so **prefer (b)**.

### 6.3 Perf note

Attaching a sink every frame drops the `run_frames` null-sink fast path (`main.rs:316`) when audio is on.
The seam is cheap (the sink methods are small; `on_event_at` for audio just buckets, `audio_sink.rs:202-208`)
and the frontend is already comfortably real-time at 60 fps, so this is a non-issue for a player. The
`--no-default-features` build keeps the fast path (§5.2).

---

## 7. Verification plan — headless-first (this env has NO audio device)

Audible output is **OWNER-RUN only** (there is no `/dev/snd` here). Everything else is verifiable
headlessly.

**Headless (runnable here / in CI):**

1. **Ring-buffer FIFO + underrun + overrun (unit).** Construct a small `HeapRb::<f32>`, split it, and:
   - push N frames of known ramps, pop them, assert **FIFO order + values** round-trip.
   - pop more than pushed → assert the shortfall is **`0.0` silence** (underrun contract, §2.5).
   - push more than capacity → assert `push_slice` returns `< offered` and the **surplus is dropped**
     (overrun contract, §2.5), with no panic.
2. **`i16 → f32` conversion (unit, pure fn).** Pin `-32768 → -1.0`, `0 → 0.0`, `32767 → ~0.99997`; assert
   all outputs land in `[-1.0, 1.0]`.
3. **Producer smoke path (integration, `--features audio`).** Boot a tiny ROM (or the existing test
   fixture pattern), create a persistent `AudioSink` at some rate, run a few frames via
   `run_frames_with_sink(1, …)`, `drain()`, convert, `push_slice` into the ring, then `pop_slice` and
   assert **samples flow** (non-empty, correct length ≈ `2·(rate/60)` f32 per frame). This exercises the
   whole producer chain **without** a device.
4. **Composite sink equivalence (unit, `--features audio`).** Feed a scripted event/boundary sequence
   through `AudioAndWatch { watch: Some(..) }` and assert both the `AudioSink` produced samples **and** the
   `Watchpoints` recorded the same hits it would standalone — i.e. the composite forwards faithfully
   (mirrors SY-4a's forwarder-equivalence test, `bus.rs:613`).
5. **Build/lint matrix.**
   - `cargo build -p oracle-frontend` (default = audio ON) — compiles with cpal+ringbuf.
   - `cargo build -p oracle-frontend --no-default-features` — compiles WITHOUT audio; reproduces today's
     loop.
   - `cargo clippy -p oracle-frontend` both ways, warning-clean.
   - `cargo build -p oracle-core` (default features) — **unchanged**; confirm cpal/ringbuf are **not** in
     its dependency tree (`cargo tree -p oracle-core` shows neither).
6. **Currency unchanged (default build).** `oracle-core` default-feature tests/goldens green — SY-5 cannot
   move them (frontend-only, §5.4), but assert it.
7. **No-device graceful path (headless, THIS env).** Run `oracle-frontend` (audio ON) where no output
   device exists → assert it prints the video-only warning and **does not crash** (the window path itself
   needs a display; drive this as the device-enumeration unit returning `None`, or run under the same
   harness the existing frontend tests use — the frontend already has headless unit tests, `main.rs:343-418`).

**OWNER-RUN (cannot verify here):**

8. **Audible playback.** `cargo run --release -p oracle-frontend -- s4.soundtest.bin` on the owner's
   machine → **the game's music plays through the speakers**, in sync with video, no sustained
   crackle/underrun. This is the milestone acceptance and can only be judged by ear on real hardware.

---

## 8. Slice / commit plan — split into two

| # | Slice | Scope | `file:line` touched | Success check (headless unless noted) |
|---|---|---|---|---|
| **SY-5a** | **Ring buffer + sink integration** (behind `audio` feature; **no device yet**) | Add the `audio` feature + `ringbuf` dep; `HeapRb<f32>` producer/consumer split; `i16→f32` convert; `AudioAndWatch` composite sink; persistent `AudioSink` + per-frame `drain()`→`push_slice` wiring; `#[cfg(not(audio))]` keeps today's loop. Consumer side pops into a **test harness** (no cpal). | `oracle-frontend/Cargo.toml`; `main.rs:250-340` (loop), new module for ring+composite | Tests 1–4,6 pass; both build/clippy variants clean (Test 5); `oracle-core` default tree has no new deps. |
| **SY-5b** | **cpal output stream** (the device glue) | Add `cpal` dep; enumerate default device/config; construct `AudioSink` at the **device rate**; `build_output_stream` with the pop-or-silence callback; stereo/mono handling; **no-device → video-only** fallback; keep `Stream` alive. | `main.rs` (cpal setup in `main`), Cargo.toml (cpal) | Test 5 build matrix incl. cpal; Test 7 no-device fallback clean **here**; **Test 8 audible — OWNER-RUN**. |

**Why split:** SY-5a is **fully verifiable in this environment** — it lands the real logic (ring
semantics, composite sink, producer wiring, feature gating) with unit/integration tests that need no sound
card, so its correctness is *banked and reviewed* before touching the one part nobody here can hear.
SY-5b is the thin cpal/device layer whose only true test is **audible on the owner's machine**; isolating
it means the un-verifiable-here glue is a small, reviewable diff and, if a session ends after SY-5a, the
tested substrate is committed and the player still builds and runs (silent) with zero regression. Mirrors
the SY-4 "biggest-risk / un-verifiable-part isolated last" house style (`git show 500cc73`, §8).

*(Alternative: one commit. Rejected — it would fold the headlessly-provable core into the un-testable-here
cpal layer, defeating the point of a reviewable, banked SY-5a.)*

---

## 9. Risks & open questions (owner ratification)

1. **[FLAG — new external deps] `cpal` + `ringbuf`.** These are the **first host-facing crates** in the
   tree beyond `minifb` (`oracle-frontend/Cargo.toml:10`). Both are mainstream, permissively licensed
   (MIT/Apache-2.0), and confined to the frontend under an optional feature (§5.4), so they never reach
   `oracle-core` or the currency gate. Owner ratifies adding them. **Recommendation: yes** — there is no
   host-audio path without a host-audio crate, and `cpal`/`ringbuf` are the de-facto standard pair.
2. **[CANNOT VERIFY HERE] Audible output.** No `/dev/snd` in this environment (memory
   `roadmap-sound-stack`). Everything except the final "does it sound right" is headless-tested (§7);
   the audible check is **OWNER-RUN** (Test 8). Flagged so "green tests" is not mistaken for "confirmed
   audible."
3. **[LOW — named limitation] Sample-rate handling.** Chosen path renders the synth **at the device rate**
   (§3.2) → **no resampler, no pitch error**. Residual: if a device offers only a rate where `rate/60` is
   a poor integer, per-frame sample counts wobble by ≤1 sample — ring-absorbed, inaudible. No action; a
   real resampler is a deliberate SY-6+ non-goal for the first cut.
4. **[LOW] Drift → occasional underrun clicks.** The three-clock skew (§4) crosses the ~4-frame ring slack
   only rarely; consequence is a faint periodic click under sustained skew. Accepted first-cut behavior;
   future fix = audio-clock pacing or a fill-level feedback resampler (§4). Owner confirms this is
   acceptable for a debugging player (it is the norm for "just play it" paths).
5. **[LOW — resolve in SY-5a] Watch-vs-audio sink exclusivity.** Recommendation is the **composite sink**
   (§6.2) so both stay live; the fallback is audio-only with the watch disabled while audio is on. Owner
   ratifies composite (preferred) vs. the simpler exclusive path. **Recommendation: composite** — cheap,
   keeps the milestone-D3 watch tooling (`main.rs:30-38`) working.
6. **[LOW] Latency (~50–67 ms).** The ~4-frame buffer trades A/V latency for underrun robustness (§2.4).
   Tunable by one `const`; owner can dial it after hearing it. No blocker.
7. **[NONE gating] Currency.** Frontend-only; `oracle-core` default build and its dep tree are unchanged
   (§5.4). No currency gate. Same class as every prior SY sink-seam slice.

---

## 10. Cross-references

- Producer seam: `main.rs:312-319` (loop), `system.rs:435,442,478` (`run_frames` / `run_frames_with_sink`
  / boundary stamp).
- `AudioSink` API consumed: `audio_sink.rs:55` (`new`), `:58,163` (`samples_per_frame`), `:81-83`
  (`drain`), `:191-195` (`finish`), `:202-235` (SY-4 timed path + boundary render), `:173-176` (stereo
  mix/clamp).
- Feature/visibility: `oracle-core/Cargo.toml:8-12` (`synth` gate), `lib.rs:19-22` +
  `synth/mod.rs:7-15,31` (module gate + re-export), `Cargo.toml:2` (resolver 2).
- Sink trait for the composite: `bus.rs:52-83` (five methods), `watchpoints.rs:234-268` (Watchpoints impl),
  `bus.rs:613` (forwarder-equivalence test precedent).
- Existing frontend watch tooling to preserve: `main.rs:30-38,245-317`.
- House-style precedent (recon → split → risks): SY-4, `git show 500cc73` /
  `docs/2026-07-23-phase-sy4-subframe-timing-design.md`.
