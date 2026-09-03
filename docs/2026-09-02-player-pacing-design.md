# The toolkit player, parcel 1 — the crate, the pacing, the input, the dock

**Date:** 2026-09-02 · **Crate:** `crates/oracle-player/` · **Branch:** `player-toolkit-parcel1`
**Predecessor:** `docs/2026-09-02-toolkit-spike.md` (the throwaway measurement that priced this)

Parcel 1 of the ruled rebuild of the player onto a real UI toolkit. It is deliberately **not** the debug
panels. It is the shell they will dock into, the pacing they will run inside, and the input that lets the
owner answer his own question about how it feels.

---

## 0. Bottom line

| | |
|---|---|
| **New crate** | `crates/oracle-player` — a workspace **member**. `egui`/`eframe`/`wgpu` reach `oracle-core` and `oracle-frontend` in neither's dependency graph; §5 gates that. |
| **Master clock** | **The audio device** — adopted from the minifb player deliberately, argued in §2.2. |
| **What the spike was missing** | A **coarse rate limit**. Its two wild runs (92.87 fps, 22.71 fps) were the same fault in opposite directions, and §1 names it. |
| **The fix** | Three layers: a monotonic 60 Hz **governor**, the audio ring as the **clock**, the display as a **slave**. §2. |
| **Measured** | §4.1. **60.037 emulated fps in both** display-independent runs, median frame period **16.666 ms in both**, **0 steady-state audio starvations and 0 producer drops in both**. Reported separately, never averaged. |
| **Measured against its own absence** | §4.2. The same binary with the governor removed runs the machine at **324.3 fps (5.4× real time) and discards 23.3 M audio samples**. |
| **Under the real stack** | §4.3. Two runs on winit + wgpu, where the spike recorded 23 starvations and a ring drained to 13.3 ms: **0 steady-state starvations, 0 drops, ring never below 50.0 ms** — through render stalls of 283 and 256 ms. |
| **The machine was NOT quiet** | §4.0. It was asked for and could not be had; the condition is measured and reported instead of claimed. |
| **Input** | Keyboard → the same `System::set_pad` surface the minifb player feeds, with the incumbent's binding and its latch. §3.1. |
| **Dock** | `egui_dock`, screen + two panels, draggable and tabbable. §3.2. |
| **The minifb player** | **Untouched.** Not one byte of `crates/oracle-frontend` is modified. |

---

## 1. The correction this parcel is built on

The spike's headline is right and stands: the toolkit's own per-frame cost is **0.217 ms of 16.667 ms**,
about 1 %, reproduced twice. Drawing was never the risk.

Its frame-rate figures were withdrawn, and the reason is the whole design brief for this parcel. Two runs
of the same binary gave **92.87 fps** and **22.71 fps**, and those are not noise — they are two ends of one
missing mechanism:

* **Run 2 — 5.8 M producer drops, 93 fps.** The loop free-ran. `ctx.request_repaint()` unconditionally,
  plus a backend that never blocked (Xvfb has no vsync), and nothing at all limiting the iteration rate.
* **Run 3 — 4 122 starvations, 23 fps.** The render path stalled 26–40 ms per present under llvmpipe,
  draining a ring whose low-water mark is one frame (~17 ms of margin).

**The spike had one layer of pacing where the minifb player has two.** `audio::frames_to_run` answers 0, 1
or 2 emulated frames from ring occupancy — a *trim*, sized to correct the ~0.62 %/s drift between a nominal
60 Hz host loop and a real 44 100 Hz device. It cannot correct "the loop is iterating at 93 Hz". Its
high-water skip is bounded by `MAX_CONSECUTIVE_SKIPS` (4) on purpose, so a wedged audio device cannot
freeze the game — which means back-pressure alone can never hold back an unbounded producer.

What the minifb player has and the spike did not is `minifb::set_target_fps(60)`: a coarse governor that
the ring then finely trims. **The toolkit did not remove the fix; it removed the thing that was doing half
of it.** Parcel 1 puts that half back, explicitly, in code the player owns rather than in a windowing
library's limiter.

**This diagnosis is not left as an argument.** §4.2 runs the same binary with that half removed again, and
it reproduces the run-2 failure to order: 324 emulated fps and 23.3 M discarded audio samples.

---

## 2. The pacing design

> *Audio is the clock, the deadline is the governor, the display is a slave.*

Source of truth: `crates/oracle-player/src/pacing.rs`, whose module documentation carries this argument
next to the code that implements it.

### 2.1 Three layers

| Layer | What it is | What it fixes |
|---|---|---|
| **1. Governor** (coarse) | A monotonic 60.00 Hz deadline. The loop asks egui to repaint at the next deadline via `Context::request_repaint_after`, and **refuses to emulate when woken early**. | Bounds the iteration rate *from above* whatever the display does. Without it: the spike's run-2 overflow. |
| **2. Clock** (fine) | The audio ring. Occupancy decides 0, 1 or 2 emulated frames per iteration — the minifb player's own `frames_to_run` policy, unchanged. | The host's "60 Hz" is never the device's 44 100/735. Only the consumer knows the truth. |
| **3. Display** (slave) | Whatever the compositor does. | Nothing. It is not allowed to set the rate. |

The interaction of layers 1 and 3 is worth spelling out, because it is where a design like this usually
goes wrong:

* **Vsync on a 60 Hz panel** — present blocks for the remainder of the frame, so the governor's wait is
  already satisfied and costs nothing. The two agree.
* **No vsync, or a 144 Hz panel** — present returns immediately, the governor is the only thing holding the
  loop at 60. *This is the case the spike did not have.*
* **A 50 Hz panel, or a compositor hiccup** — present blocks longer than a period. The loop falls behind;
  the governor **rebases instead of sprinting** (below); and layer 2 runs the extra emulated frames, so the
  machine still runs at 60 while the display shows 50 of them. That is the correct behaviour, and it
  arrives for free out of having the audio ring as master.

### 2.2 Why the audio device stays the master clock

The incumbent's answer (`crates/oracle-frontend/src/audio.rs`; `bus.rs`'s note that the audio device is the
master clock) is adopted, not inherited. It is the right answer for three reasons that survive the toolkit
change:

1. **It is the only clock that cannot be made to wait.** A dropped video frame at 60 Hz is invisible; a
   starved audio callback is an audible click. Whichever clock is not the master absorbs the error, and
   video is the one that can absorb it silently.
2. **It is the only clock whose true rate is knowable at runtime.** `sample_rate` is nominal; the crystal
   is not 44 100.000 Hz and no API reports what it actually is. Ring occupancy measures it directly, in the
   only units that matter.
3. **It is the clock the core already produces against.** The synth emits exactly `sample_rate / 60` pairs
   per *emulated* frame. Pacing on anything else creates a one-directional deficit, and `audio.rs` measured
   that deficit at 0.62 %/s — enough to pin the ring at empty and silence-fill 8–16 % of callbacks. **A
   deficit is not a latency problem: no ring size fixes it.**

**The alternative that was rejected:** vsync as master, one emulated frame per present. It is correct only
on an exactly-60 Hz panel, it makes the emulator's speed a property of the user's monitor, and it is
precisely the arrangement that produced the 92.87 fps run.

### 2.3 "Rebase, never sprint"

`Governor::tick` never lets the deadline trail `now`. When an iteration starts more than a period late, the
naive fix is to advance the deadline by one period and let the backlog work itself off — which converts one
stall into a *burst* of zero-wait iterations, and a burst is exactly what overflowed the ring in run 2.

So a late frame costs **one** immediate iteration and then the period resumes, and the **audio ring** — not
the governor — decides whether the lost emulated frames are made up. That is the right division of labour,
because only the ring knows whether they need to be. `pacing::tests::a_stall_rebases_and_does_not_sprint`
pins it.

### 2.4 The early-wake rule, and why `request_repaint_after` beats sleeping

`request_repaint_after` is strictly better than a sleep inside the frame: the event loop waits with a
timeout and *still services input*, so a key press wakes the loop immediately and is seen on the very next
frame rather than up to 16 ms later. That responsiveness is only safe because the governor refuses to
emulate on an early wake — an input-driven repaint re-presents the retained picture and advances nothing.
Without that rule, holding a key down would run the emulator fast.

`EARLY_TOLERANCE` is 1 ms: above every timer's granularity here, far below a frame, and it stops the
governor busy-spinning on a repaint that missed its deadline by a microsecond.

### 2.5 The one departure from the incumbent: `LOW_WATER_FRAMES` 1 → 2

`audio.rs` documents this dial and names this exact case:

> A machine that stalls its render loop for tens of milliseconds at a time wants 2 or 3 here; nothing else
> has to change.

A toolkit present *can* stall for tens of milliseconds — llvmpipe does it at 26–40 ms, and a resize or a
shader recompile does it on real hardware. Two frames costs ~17 ms of added audio latency and buys ~32 ms
of margin, and `audio.rs`'s own table says 1, 2 and 3 all give zero underruns in steady state, so latency
is the only thing being spent.

**It is implemented as a parameter, not as an edit to `audio.rs`,** because changing that constant would
change the minifb player's behaviour and this parcel does not touch the minifb player.
`pacing::frames_to_run` takes `low_water` as an argument, and
`pacing::tests::low_water_of_one_is_the_players_policy` sweeps a grid of every branch proving that at
`low_water == audio::LOW_WATER_FRAMES` it *is* `audio::frames_to_run`. The two policies cannot drift apart
without a red test.

### 2.6 What this design does not do

**Emulation, UI layout and present run on one thread.** A panel expensive enough to stall the UI thread
stalls the emulator with it, and no ring depth fixes an emulator that has stopped.

That is a choice, made on the numbers: emulation is 2.76 ms of a 16.67 ms budget and the whole toolkit is
0.22 ms, so there is ~6× headroom, and the stall risk lives in *present*, not in compute — and present
stalls are exactly what a deeper ring absorbs. A thread split would also have to answer for input
determinism (`set_pad` is documented as the sole deterministic input path), for where the Aether pump
lives, and for a `System` that would need a lock held across a whole emulated frame.

The boundary is drawn so a later parcel can move it without redesigning: `Machine::step` takes a pad and
returns a picture, and **no toolkit type appears anywhere in `machine.rs`'s public surface**. If a debug
panel is ever *measured* to stall the UI thread, the fix is to put `Machine` behind a frame channel on its
own thread — not to raise the low-water mark again.

---

## 3. The rest of the parcel

### 3.1 Input

`src/input.rs`. Keyboard → `oracle_core::io::Pad` → `System::set_pad(0, pad)` — the same surface the minifb
player feeds, which `io.rs` documents as the sole, deterministic input path into the core. Nothing reaches
around it.

**The binding is the incumbent's, exactly:** arrows = D-pad, `A`/`S`/`D` = Genesis A/B/C, `Enter` = Start
(`crates/oracle-frontend/src/main.rs::poll_pad`). It is reproduced rather than improved on, because it is
the binding the owner's hands already know. `input::tests::the_binding_is_the_minifb_players_binding`
checks all eight one key at a time, and a separate test pins `S`→B / `D`→C specifically — the one
transcription error in the mapping that would be invisible until somebody tried to jump.

**One thing had to be redesigned.** The minifb player's `swallow_keys_until_release` latch exists because
its command palette closes *mid-iteration*, leaving the key that closed it physically held when the pad is
polled further down the same iteration — so `Enter` read straight through as Start and paused the game.
This player has no palette yet, but it has something minifb never had: **egui widgets that want the
keyboard**. So the same latch is kept, driven by `Context::wants_keyboard_input()` instead of a palette
flag. `input::release_latch` is the incumbent's function unchanged; only what feeds it is new.

Gamepads are **not** wired in parcel 1 (see §6).

### 3.2 The dock

`src/ui.rs`. An `egui_dock::DockState` with the game screen on the left and two panels on the right,
draggable, tabbable and splittable:

* **Screen** — the emulator picture, aspect-fit, nearest-sampled, uploaded to an `egui::TextureHandle` each
  frame. It is a tab like any other, which is the point of the rebuild.
* **Pacing** — *not* a placeholder. It shows this parcel's own subject live: governor rebases, early wakes,
  worst lateness, ring occupancy, steady-state starvations, producer drops. A wobble like the spike's is
  visible **while it is happening**, not only in a report afterwards.
* **Registers (placeholder)** — 20 monospace rows rebuilt every frame. It is there to *cost* what a real
  panel costs, so the measurement is not taken against an empty layout. The real panel is a later parcel.

**Layout persistence is deliberately not done.** `egui_dock::DockState` derives `Serialize`/`Deserialize`
under the crate's `serde` feature, so it is one feature flag and a bound on `Tab` — but persisting a layout
of placeholders would only have to be migrated when the real panels arrive. Parcel-2 line item, not an
unknown.

---

## 4. The measurement

ROM: `aeon/s4.debug.bin` (736 454 bytes), free-running from reset into attract-mode gameplay, no input
injected. Every run ends by reporting its last picture — **320×224, 27 distinct colours, 46–57 % non-black
pixels** throughout — so the costs are for a real frame and not a black screen.

**Every run below is a separate process invocation, reported separately. Nothing is averaged across runs.**

### 4.0 The condition of the machine — asked for, and not obtained

The brief asked for a quiet machine. **It was not available and could not be made available**, and saying
otherwise would be the more comfortable of two reports. Recorded either side of every run:

* **No `cargo`, and no other build.** That part *was* in my gift and was enforced (`pgrep -c -f cargo` = 0
  before each run).
* **Everything else was the owner's session, live**, and it is recorded per run in the tables below. Across
  the six runs the box carried a `sigil` process at 97–98 % of a core (sometimes two), or a pair of
  `python3` at 33–39 %, plus Vivaldi at ~50 % across three processes, Discord, `kwin_wayland` and Steam
  helpers throughout. Load average ran **2.3–5.7 on 16 cores**.

So the honest framing is that the design was measured **under a realistic contended load, not in a
laboratory**, and that this makes the passing results stronger and would have made a failing result
ambiguous. Nothing failed.

### 4.1 The answer — display-independent, `--mode bench-cpu`, 75 s, real audio device at gain 0.0

No window, no GPU: a bare `egui::Context` driven through the identical per-frame pipeline by the player's
own `Governor`. **The frame rate here is a real answer rather than a harness artefact, because in this
design the governor and not vsync sets the rate.**

| | **run 1** | **run 2** |
|---|---|---|
| **emulated frames/s** | **60.037** | **60.037** |
| presented frames/s | 59.984 | 59.997 |
| **frame period, median** | **16.666 ms** | **16.666 ms** |
| frame period, p95 / p99 | 16.890 / 17.686 ms | 16.927 / 17.153 ms |
| **frame period, WORST** | **29.036 ms** | **21.778 ms** |
| governor rebases | 1 | 0 |
| governor early wakes | 1 | 1 |
| worst lateness | 17.182 ms | 6.361 ms |
| iterations running 2 frames | 4 (0.089 %) | 3 (0.067 %) |
| iterations running 0 frames | 0 | 0 |
| **audio starvations, steady-state** | **0** | **0** |
| **audio producer drops** | **0** | **0** |
| leanest ring | 5174 samples (58.7 ms) | 7172 samples (81.3 ms) |
| device callbacks | 1758 | 1758 |
| load on the box during the run | two `python3` at 39 % / 33 % | one `sigil` at 98 % |

Per-iteration CPU cost, milliseconds (n = 4499 / 4500):

```
run 1                                                  run 2
part            mean  median   p95    p99     max      mean  median   p95    p99     max
emulate        2.752   2.695  3.028  4.315  16.663    2.705   2.676  2.940  3.433   5.985
audio          0.001   0.001  0.001  0.002   0.010    0.001   0.001  0.001  0.002   0.045
convert        0.037   0.035  0.047  0.068   0.304    0.037   0.035  0.046  0.066   0.324
tex-upload     0.006   0.005  0.009  0.014   0.384    0.005   0.005  0.007  0.012   0.055
ui-build       0.135   0.123  0.212  0.265   3.195    0.125   0.120  0.164  0.226   3.353
tessellate     0.026   0.023  0.041  0.055   0.212    0.025   0.023  0.033  0.048   0.224
CPU TOTAL      2.930   2.863  3.274  4.642  17.008    2.874   2.840  3.113  3.690   6.233
period        16.671  16.666 16.890 17.686  29.036   16.667  16.666 16.927 17.153  21.778
```

**Reading it.**

* **The two runs agree.** 60.037 emulated fps in both; median period 16.666 ms in both; zero steady-state
  starvations and zero producer drops in both. That agreement is the thing the spike could not produce, and
  it is why the spike's frame-rate figures had to be withdrawn.
* **The one number that materially differs is the worst frame: 29.0 ms against 21.8 ms.** Reported, not
  averaged. It is one frame in 4498 in run 1 — the same iteration as that run's single rebase and its
  16.663 ms `emulate` maximum, i.e. the emulator itself was preempted for ~16 ms on a contended box. The
  design absorbed it: the governor rebased instead of sprinting, and the ring never noticed (0 starvations,
  leanest still 58.7 ms).
* **The toolkit's own share is 0.186 ms / 0.183 ms** at the median (convert + upload + ui-build +
  tessellate), against the spike's 0.217 ms. The spike's headline reproduces.
* **Emulation is 94 % of the CPU frame.** Whatever the player is drawn with, this is what it costs.
* **The fine trim barely has to work: 0.07–0.09 % of iterations ran a second frame, and none ran zero.**
  That is a *better* result than the minifb player's, and for a structural reason. `minifb`'s limiter
  sleeps `target − elapsed` and only *then* restarts its clock, so its period is `16.667 ms + overshoot`
  and its rate is permanently under 60 (measured 59.54–59.63 in `audio.rs`), leaving the ring feedback a
  0.62 %/s deficit to close. `Governor` schedules against an **absolute** deadline that sleep overshoot
  cannot accumulate into, so the only residual the ring has to correct is the device crystal's true offset
  from 60 × 735 — about 4 frames in 75 seconds.

### 4.2 The control — the same binary with **layer 1 removed** (`--target-fps 0`)

A design that is only argued for is not measured. `--target-fps 0` switches the governor off, reproducing
the spike's arrangement — every repaint owns a frame, nothing waits, the audio ring's trim is the only
pacing left — and the report stamps the run `GOVERNOR OFF` so a number from it can never be mistaken for
the player's.

**`bench-cpu`, 60 s, governor off:**

| | governor ON (§4.1) | **governor OFF** |
|---|---|---|
| loop iterations/s | 60.0 | **1519.0** |
| **emulated frames/s** | 60.037 | **324.304 — 5.4× real time** |
| frame period, median | 16.666 ms | **0.065 ms** |
| iterations running 0 frames | 0 (0.000 %) | **71 681 (78.65 %)** |
| **audio producer drops** | **0** | **23 295 384 samples** |
| audio starvations, steady | 0 | 0 (the ring is pinned *full*) |

That is the spike's run-2 failure, reproduced on demand and four times larger. The mechanism is visible in
the middle row: the ring answers 0 for 78.6 % of iterations, but `MAX_CONSECUTIVE_SKIPS` caps the skip run
at 4, so roughly one iteration in 4.7 runs a frame regardless and `push_frame` discards it. **Back-pressure
with a bounded skip run cannot hold back an unbounded producer** — and the bound is not a bug, it is the
safety valve that stops a wedged audio device freezing the game.

**And a finding that cuts against the simple story.** The *same* control through the real winit + wgpu
stack came out fine — three times out of three:

| `bench-window`, governor OFF, 60 s | run 1 | run 2 | run 3 |
|---|---|---|---|
| emulated frames/s | 59.833 | 59.877 | 59.889 |
| producer drops | 0 | 0 | 0 |
| starvations, steady | 0 | 0 | 0 |
| iterations running 0 frames | 177 (4.73 %) | 174 (4.66 %) | 227 (5.98 %) |

llvmpipe's present on this Xvfb happens to cost ~16 ms, so it accidentally acted as a rate limiter at
~62 Hz. **The ungoverned loop is not reproducibly bad; it is reproducibly *uncontrolled*.** Its rate is
whatever the display happens to do that day — ~62 here, 93 in the spike's run 2, 23 in its run 3, and 1519
when nothing throttles it at all. That is a sharper argument for the governor than a uniformly bad control
would have been, and it is the clearest single explanation of why one run of the spike was never going to
settle anything. (Note also that 4.7–6.0 % of iterations were being held back by ring back-pressure against
**0.00 %** with the governor on: even on the runs that looked healthy, the ungoverned loop was riding
against the wall.)

### 4.3 The real stack — `--mode bench-window`, 75 s, on Xvfb

winit + wgpu + eframe on llvmpipe (`MESA-EGL: warning: DRI3 error: Could not get DRI3 device` — software
rasterisation, confirmed), with `display ownership CONFIRMED: toolkit reports 1281x803` printed from inside
the toolkit before anything was drawn.

| | **run 1** | **run 2** |
|---|---|---|
| **emulated frames/s** | **59.836** | **59.860** |
| frame period, median | 15.938 ms | 15.757 ms |
| frame period, p95 / p99 | 27.883 / 28.941 ms | 28.105 / 28.910 ms |
| frame period, WORST | 42.613 ms | 30.790 ms |
| **governor rebases** | **78** | **8** |
| **worst lateness** | **282.898 ms** | **256.262 ms** |
| governor early wakes | 236 | 305 |
| iterations running 2 frames | 96 (2.19 %) | 12 (0.27 %) |
| **audio starvations, steady-state** | **0** | **0** |
| audio starvations, total | 9 (all inside the 60-callback warm-up) | 8 (likewise) |
| **audio producer drops** | **0** | **0** |
| **leanest ring** | **4410 samples (50.0 ms)** | **5940 samples (67.3 ms)** |
| CPU TOTAL, median | 2.899 ms | 2.834 ms |

**This is the headline result of the parcel, and it is the direct comparison with the spike.** Pass C of the
spike — the same stack, the same rasteriser, the same machine — recorded **23 steady-state starvations and
a leanest ring of 1176 samples (13.3 ms)**, i.e. the ring fell *below* its one-frame low-water mark and
stayed there. Here, under a render path that stalled *worse* (worst lateness 283 and 256 ms, worst frame
42.6 ms), the ring never fell below **4410 samples (50.0 ms, three frames)** — still comfortably above the
two-frame mark — and **not one steady-state callback starved, in either run**.

Both mechanisms are visible in the table. The governor **rebased** 78 and 8 times rather than sprinting off
the debt; and the deeper low-water mark kept the reservoir under the stall while the fine trim spent 96 and
12 double-frame iterations refilling it. `audio.rs` predicted exactly this dial, and the prediction held.

**What this pass does NOT establish:** the `period` column is **llvmpipe's**, not the machine's, and the
report refuses to quote a presented frame rate from it at all. See §4.4.

### 4.4 What is not measured, and why

1. **Presented frame rate, and frame pacing, on the real GPU.** `Xvfb` has no vsync and llvmpipe is not
   this machine's GPU. Nothing here is a statement about how the picture looks on the owner's 2844×1600
   display at its 59.96 Hz. **Deferred to a foreground pass on a real display — TAGGED for foreground
   follow-up.** The display-independent pass (§4.1) is what stands in for it, and it stands in for it
   honestly precisely *because* this design's rate does not come from the display.
2. **GPU-side cost.** Only the CPU-side `TextureHandle::set` is measured (5–6 µs). `queue.write_texture`,
   the egui render pass and the swapchain present are the backend's, past `App::ui`.
3. **Input latency, resize, DPI, multi-monitor, gamepads.** Not exercised headless. The `bench` modes feed
   an empty `RawInput`, so the pad is all-released in every measurement above; the input path's
   *correctness* is pinned by unit tests, its *latency* is not measured.
4. **How it feels.** Which is the owner's question, and the reason input is in this parcel at all.
5. **Anything requiring the emulator MCP.** Not touched, per the standing rule.

### 4.5 Two reporting nits, stated rather than hidden

* `starved samples` is **not** split into warm-up and steady-state the way the callback *count* is. In
  every run above the steady-state count is 0, so the whole figure (e.g. 282 ms in win-1) is definitionally
  warm-up — but the line would be misleading in a run that did starve. Worth splitting when something
  starves.
* `--expect-screen` is **required** by `bench-cpu` even though that mode opens no display connection at
  all. That is deliberate — one uniform refusal for every bench mode is harder to forget than a
  per-mode one — but the check itself can only fire in `bench-window`.

---

## 5. Reproducing

```bash
ln -s /home/volence/sonic_hacks/oracle/vendor vendor        # 18 TestRoms entries
cargo build --release -p oracle-player
./crates/oracle-player/run-bench.sh screens            # prove :0 and :77 are different displays
./crates/oracle-player/run-bench.sh cpu    75 on       # THE ANSWER — display-independent
./crates/oracle-player/run-bench.sh window 75 on       # real stack on Xvfb; presented fps is REFUSED
./crates/oracle-player/run-bench.sh cpu    60 on 0     # THE CONTROL — governor off
./crates/oracle-player/run-bench.sh window 60 on 0     # the control, real stack
```

Dependency-graph gates, which are what "keep the toolkit out of the core" means in practice:

```bash
cargo tree -p oracle-core     | grep -icE 'egui|eframe|wgpu|winit'   # -> 0
cargo tree -p oracle-frontend | grep -icE 'egui|eframe|wgpu|winit'   # -> 0
```

**The instrument rule, and where it is enforced.** The owner is using this machine.

* `run-bench.sh` starts its own `Xvfb :77 -screen 0 1281x803x24 -nolisten tcp`, kills only a server it
  started itself, and strips `WAYLAND_DISPLAY` / `XDG_SESSION_TYPE`. The geometry is deliberately one no
  real monitor is, so the check below is a discriminator rather than a coincidence.
* **The environment is setup, not the guard.** The guard is that the binary asks the *toolkit* for its own
  screen size on the first frame and `exit(2)`s without drawing on a mismatch — and that it **refuses to
  start any bench mode at all without `--expect-screen`**.
* **The guard was proven to fire, not assumed to.** Run against the private `:77` (1281×803) while
  demanding `1920x1080`, the binary printed
  `ABORT: the toolkit reports a 1281x803 screen but this run demanded 1920x1080 ... Refusing to draw.`
  and exited **2**. The same invocation with `1281x803` printed `display ownership CONFIRMED` and ran. A
  guard that has never been seen to fire witnesses nothing.
* Both bench modes force **gain 0.0**, which multiplies on the producer side, so the ring dynamics, the
  feedback loop and every underrun count are genuine and the amplitude is exactly zero.

---

## 6. What is deliberately not in parcel 1

Everything here is a decision, not an omission.

| Not done | Why |
|---|---|
| The real debug panels | Later parcels, by the brief. All of them are already served by the Aether method table (spike §5.1). |
| Layout persistence | §3.2 — a layout of placeholders would have to be migrated. |
| Gamepad input | The brief asks for "keyboard at minimum". `gilrs` is a frontend-only optional dependency; wiring it is mechanical and belongs with the input-configuration parcel. |
| Aether bus hosting | The minifb player's `--aether` surface. Parcel 1 has no need of it and hosting it would put the 4 ms pump budget inside the frame that this parcel is trying to prove. |
| Save states, ROM browser, overlay, command palette, symbol watches, `.srm` | All minifb-player features. None are lost — that player is untouched — and each needs its own parcel. |
| Deleting `crates/oracle-panels-spike` | The spike is the evidence this parcel is built on. It should go once these numbers are banked, and that is a decision for the owner, not for the parcel that supersedes it. |
| Presented frame rate on the real GPU | Reach limit of a headless instrument, not an omission. See §4.4. **TAGGED for the foreground pass.** |
| Splitting `starved samples` into warm-up and steady | §4.5. Harmless while nothing starves; worth doing the first time something does. |
| Moving `Machine` to its own thread | §2.6. A decision on the numbers, with the seam left in place so a later parcel can take it without a redesign. |
| Fixing the same `TexturesDelta` drop in the spike | §7.1a. That crate is due for deletion; recorded rather than patched. |
| Fixing `oracle-aether`'s load-sensitive flake | §7.3. Real, pre-existing, and nothing to do with this parcel. **TAGGED.** |

---

## 7. Gate status

* `cargo fmt --all` — clean.
* `cargo clippy --all-targets -- -D warnings` (workspace, **including** `oracle-player`) — exit 0.
* `cargo build --release -p oracle-player` — exit 0, **0 warnings**.
* `cargo test -p oracle-player` — **34 passed, 0 failed**. 16 of those are the audio substrate's own tests,
  compiled and run here through the `#[path]` include — which is the proof that the file this crate
  compiles is the file the minifb player compiles.
* `cargo test --workspace` — see §7.2.
* `cargo tree -p oracle-core` / `-p oracle-frontend` — **0** lines matching `egui|eframe|wgpu|winit` in
  either.
* `Cargo.lock` — 243 → 476 packages. **0 packages removed and 0 pre-existing package's version changed**;
  every entry in the diff is a *second* version added alongside the one that was already there, so no other
  member's resolution moved.

### 7.1 Red-first, with the mutation shown on disk

Every load-bearing test here was proven to fail against a deliberately broken implementation before it was
trusted. Each mutation was applied to the working tree from the **committed** baseline, shown with
`git diff`, run, and reverted with `git checkout --`.

| # | Mutation (on disk, quoted from `git diff`) | Test | Result |
|---|---|---|---|
| 1 | `pacing.rs`: `- self.next = now + self.period;` — the rebase removed, i.e. the naive "work off the debt" scheduler | `a_stall_rebases_and_does_not_sprint` | **RED**, exit 101 |
| 2 | `pacing.rs`: `- if occupied < low_water * frame_samples` → `+ if occupied <= …` — a one-character drift from the player's policy | `low_water_of_one_is_the_players_policy` | **RED**, `diverged at occupied=1470 capacity=4410 skips=0` |
| 3 | `input.rs`: `b: down(egui::Key::D), c: down(egui::Key::S)` — B and C swapped | `s_is_b_and_d_is_c_not_the_other_way_round` **and** `the_binding_is_the_minifb_players_binding` | **RED** ×2, `S must be B` |
| 4 | `pacing.rs`: `- if self.next > now + EARLY_TOLERANCE` → `+ if false` — the early-wake guard removed | `an_early_wake_does_not_run_a_frame` | **RED**, `a repaint 12 ms before the deadline must not emulate` |
| 5 | `pacing.rs`: the *control's* `wait: Duration::ZERO` → `wait: FRAME_PERIOD` — i.e. the "governor off" run secretly pacing after all | `the_unpaced_control_never_waits_and_never_turns_a_repaint_away` | **RED**, `the control must never wait` |
| 6 | `input.rs`: `- i.key_down(k)` → `+ i.key_pressed(k)` — poll this frame's key *events* instead of the held-key set | `a_real_egui_context_holds_a_key_down_across_frames` | **RED**, `a held key must stay down on a frame that carries no events` — **and the five other `input` tests stayed green**, which is the entire reason that test exists |

Mutation 5 exists because the control in §4.2 is only evidence if it really is uncontrolled. A control that
quietly paces would have produced a comfortable number and witnessed nothing.

Mutation 6 is the sharpest of the six. Five `input` tests pin the *mapping* through a closure, and **none of
them notices** a `poll_pad` that reads key events instead of held keys — a player built that way responds to
a tap and drops every held direction on the next frame, which is unplayable in the least obvious possible
way. Only the test that drives a real `egui::Context` across frames catches it.

### 7.1a One real defect found by writing that test

Adding `a_real_egui_context_holds_a_key_down_across_frames` immediately failed — not on the assertion, but
inside `epaint`:

```
Dropped TexturesDelta with 1 unapplied deltas. Deltas need to be handled.
  epaint-0.36.1/src/textures.rs:337
```

`egui::FullOutput::textures_delta` **panics on drop** while it still holds unapplied deltas, because in a
real backend an unapplied delta is a leaked GPU texture. `bench-cpu` has no backend and was doing
`drop(out.textures_delta)`, which is the wrong discard: `clear()` is the API's own escape hatch. **The panic
is debug-only**, so a release-mode bench never sees it and the two 75-second measurement runs made before
this test existed were silently on the wrong side of it.

Fixed here. **The throwaway spike still has the same line** (`crates/oracle-panels-spike/src/main.rs:628`)
and nothing has ever run it in debug, because that crate has no test target and is excluded from the
workspace. Not fixed here — that crate is due for deletion — but recorded, because it is a small example of
the thing the exclusion costs.

### 7.2 Suite accounting

Baseline on `main` before this branch, measured here rather than quoted:
**64 legs / 2027 passed / 0 failed / 6 ignored**, exit 0.

After: **65 legs / 2061 passed / 0 failed / 6 ignored**, exit 0. Every added row is accounted for:

* **+1 leg** — `oracle-player`'s bin test target, which did not exist before.
* **+34 passed** — 10 `pacing`, 6 `input`, 3 `stats`, and **16 from `audio`**, the substrate's own
  `#[cfg(test)] mod tests` compiled a second time through the `#[path]` include. Those 16 also still run in
  `oracle-frontend`'s own leg; they are the same assertions over the same source, executed in two crates.

### 7.3 A pre-existing flake found while establishing the baseline

`oracle-aether::hosted::a_halted_window_resumes_past_its_own_breakpoint` **failed once on unmodified
`main`**, in a full `cargo test --workspace` run, before any file in this parcel existed:

```
assertion `left == right` failed: second halt:
  {"droppedEvents":0,"frame":2,"mclk":1792182,"running":true,"timeoutReached":true,"waitedMs":0}
  left: Bool(true)   right: Bool(false)
```

`timeoutReached: true, waitedMs: 0` — a wait that expired before it waited. Characterised: **6/6 green**
run alone, **8/8 green** running the whole `hosted` binary alone, and it has only ever been seen under a
full-workspace run, where 64 test binaries share 16 cores. So it is **load-sensitive, not this parcel's**,
and it is the one row that can turn a green workspace run red on a busy machine. **TAGGED for the
foreground pass** — it is not fixed here, because fixing it means touching `oracle-aether` for a reason
that has nothing to do with parcel 1.
