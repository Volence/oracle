# Toolkit spike — measuring the frame loop under egui + egui_dock + wgpu

**Date:** 2026-09-02 · **Scope:** measurement and pricing only. Nothing was rebuilt.
**Artefact:** `crates/oracle-panels-spike/` (throwaway; delete once this is banked).

The ruling is that the player is rebuilt on a real UI toolkit so debug panels can dock, tab and remember
layouts. This document is the measurement that was asked for *before* anything is rebuilt, plus the pricing
of the separate-process hedge.

---

## 0. Bottom line

| Question | Answer | Established? |
|---|---|---|
| Does the frame loop fit under this toolkit? | **Yes, with room to spare.** Median CPU cost **3.01 ms** of a 16.67 ms budget; sustained **60.03 fps**. | Measured, display-independent |
| What does the *toolkit* itself cost per frame? | **0.22 ms median, 0.66 ms at p99** — convert + upload + UI-build + tessellate combined. ~1.3 % of the frame. | Measured |
| What dominates? | **Emulation**, 2.76 ms median — 92 % of the CPU frame. The toolkit is noise beside it. | Measured |
| Headroom? | **374.6 fps unpaced** = **6.2× real time** on the CPU side. | Measured |
| Does audio hold pace? | **Yes** in the display-independent pass: **0 steady-state starvations** in 2813 device callbacks, 0 producer drops. | Measured, real device |
| Presented fps under vsync on the real GPU? | **NOT MEASURED. Deferred to a foreground pass.** | See §4 |

And one correction to the brief, in §6: **minifb does not own a blocking event loop.** It is a poll-per-frame
loop that returns control every iteration. That changes the in-process answer, though not the conclusion.

---

## 1. What was built, and what was kept out of it

`crates/oracle-panels-spike/` — a throwaway binary, `publish = false`.

**It is not a workspace member.** It is in the root manifest's `exclude` list *and* carries its own empty
`[workspace]` table (needed because this tree is also checked out as a git worktree under
`oracle/.claude/worktrees/`, where cargo walks past the worktree's root manifest and finds the parent
checkout's). Consequences, all verified:

* the workspace `Cargo.lock` is **unmodified** — 260 packages before and after (`git status` shows only
  `Cargo.toml`, never `Cargo.lock`);
* the spike's own lock file has **466** packages, of which **233 are new crates** the toolkit drags in;
* `cargo test --workspace`, `cargo clippy --all-targets`, `cargo fmt --all` neither build nor lint it;
* `oracle-core` and `oracle-frontend` dependency graphs are untouched — egui/eframe/wgpu appear in neither.

The audio path is **not re-implemented**. `src/main.rs` does

```rust
#[path = "../../oracle-frontend/src/audio.rs"]
mod audio;
```

so the ring, the `frames_to_run` feedback loop and `fill_output` are byte-for-byte the player's own file,
at the player's own `ringbuf 0.4` / `cpal 0.15`.

Versions under test: **egui 0.36.1, eframe 0.36.1, egui_dock 0.21.1** (latest at time of writing).

---

## 2. The instrument problem, and how it was respected

The owner is using this machine. Two hazards, two guards.

### 2.1 No window on his screen

The session is **KDE Plasma on Wayland**; `DISPLAY=:0` is his XWayland server, spanning `4764x1600`.

`run.sh` starts its own `Xvfb :77 -screen 0 1281x803x24 -nolisten tcp` (printing the PID, and killing only
a server it started itself, on an `EXIT` trap) and launches the binary under
`env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE DISPLAY=:77`. The geometry `1281x803` is deliberately a size no
real monitor is, so the check below is a real discriminator rather than a coincidence.

**The env is not the guard — it is only the setup.** The guard is that the binary *asks the toolkit for its
own screen size* on its first frame, before drawing anything:

```rust
let monitor = ctx.input(|i| i.viewport().monitor_size);
// ...mismatch against --expect-screen -> eprintln + std::process::exit(2), without drawing
```

Both displays were shown side by side first (`./run.sh screens`):

```
--- the session's real display (:0) ---
Screen 0: minimum 16 x 16, current 4764 x 1600, maximum 32767 x 32767
DP-1 connected 2844x1600+1920+0 ...
--- this run's private Xvfb (:77) ---
Screen 0: minimum 1 x 1, current 1281 x 803, maximum 1281 x 803
```

and every `eframe` run printed, from inside the toolkit:

```
display ownership CONFIRMED: toolkit reports 1281x803
```

### 2.2 No sound on his speakers

There **is** a working output device here (44100 Hz, 2 ch, f32), so "no device, audio deferred" would have
been a false report. Instead the spike opens the **real** default device through the real cpal path — real
callbacks, real ring, real pacing — and pushes every frame at **gain 0.0**:

```rust
a.dropped += audio::push_frame(&mut a.prod, &pcm, 0.0) as u64;
```

`push_frame` multiplies each sample by the gain on the producer side, so the ring dynamics, the feedback
loop and the starvation counts are the genuine ones and the amplitude is exactly zero. Nothing was audible.

### 2.3 Two counters the player does not have

`fill_output` reports nothing about what it could not serve, so the spike reads the ring occupancy
immediately before calling it and counts the shortfall. Starvations are split into **total** and
**steady-state** (excluding the first 60 callbacks): the pre-roll is two frames of silence and the device's
*first* callback can ask for a quantum several times that, so a warm-up starve is the reservoir filling, not
the loop failing. Reporting one number for both would have been the believable wrong answer — the first
draft did exactly that and showed "1 starvation" for what was purely start-up.

---

## 3. The numbers

ROM: `aeon/s4.debug.bin` (736,454 bytes), free-running from reset through title into attract-mode gameplay,
no input injected. Each run ends by reporting the last picture, as proof the costs are for a real frame and
not a black screen: **320×224, 27 distinct colours, 42–56 % non-black pixels** in every run.

Layout under test: an `egui_dock` `DockState` of **three docked panels** — Screen (the uploaded texture,
aspect-fit), Registers (20 monospace rows of formatted CPU state, rebuilt every frame), Timings (13 rows) —
plus a top bar.

All figures are **milliseconds per loop iteration**. `n` is the sample count; every sample is retained, so
the median and max are real observed values, not estimates.

### 3.1 Pass A — display-independent CPU cost (`--mode cpu`, 75 s, audio on)

No window, no GPU, no eframe: a bare `egui::Context` driven by hand to a 60 Hz deadline, running the
identical pipeline (emulate → convert → `TextureHandle::set` → `DockArea::show_inside` →
`Context::tessellate`). **This is the answer to the CPU-cost question.**

```
loop iterations      4500
emulated frames      4502  (60.03/s)
drawn frames         4502  (60.03/s)   <-- sustained frame rate
last picture         320x224, 39914 non-black pixels (55.7%), 27 distinct colours

part               mean   median      p95      p99      max        n
emulate           3.263    2.764    6.078    8.329   16.463     4500
audio             0.001    0.001    0.002    0.002    0.038     4500
convert           0.045    0.036    0.066    0.079    5.307     4500
tex-upload        0.009    0.007    0.016    0.023    0.469     4500
ui-build          0.164    0.141    0.267    0.469    2.923     4500
tessellate        0.039    0.033    0.063    0.091    1.104     4500
CPU TOTAL         3.556    3.014    6.638    8.891   16.969     4500
period           16.667   16.667   16.713   17.330   20.104     4499
```

**Reading it:**

* **Sustained frame rate: 60.03 fps** over 75 s, with a period distribution of median 16.667 ms, p99
  17.330 ms, worst 20.104 ms. The worst frame is 3.4 ms over budget, once in 4499.
* **The toolkit's own share** (convert + upload + UI-build + tessellate) is **0.217 ms** summing the four
  medians and **0.662 ms** summing the four p99s — about **1.3 %** of a 16.67 ms frame, rising to 4 % at
  p99. It is not the problem. *(A sum of per-part quantiles is not the quantile of the sum; since the parts
  rarely peak together, the p99 figure over-states. Both are quoted only to size the share.)*
* **Emulation is 92 % of the CPU frame** at the median. Whatever the player is drawn with, this is what it
  costs.
* Texture upload is the *cheapest* part measured — 7 µs to hand egui a 320×224 image. Conversion from the
  core's `(u8,u8,u8)` scanlines to `Color32` is 36 µs. Note the 7 µs is an **upper bound**: the spike
  `clone()`s the retained `ColorImage` because `TextureHandle::set` takes it by value, and 287 KB of memcpy
  at ~40 GB/s is essentially the whole figure. A real implementation that moves the image instead would pay
  close to nothing here.

### 3.2 Pass B — headroom (`--mode cpu-unpaced`, 25 s, audio off)

Same pipeline, deadline removed.

```
loop iterations      9365
emulated frames      9365  (374.58/s)

part               mean   median      p95      p99      max        n
emulate           2.532    2.503    2.723    3.056    9.322     9365
convert           0.038    0.035    0.053    0.064    0.289     9365
tex-upload        0.005    0.004    0.005    0.007    0.233     9365
ui-build          0.071    0.072    0.103    0.125    0.399     9365
tessellate        0.017    0.016    0.022    0.031    0.223     9365
CPU TOTAL         2.669    2.636    2.896    3.247    9.762     9365
```

**374.6 fps = 6.2× real time** on the CPU side.

**A caveat that matters, and it cuts against the unpaced numbers.** Every bucket is *cheaper* here than in
the paced pass — UI-build is 0.072 ms against 0.141 ms, less than half. The paced loop sleeps ~13.6 ms of
every 16.7 ms, so it wakes on a downclocked core with cold caches; the unpaced loop never sleeps and runs at
boost clock with everything hot. **Pass A is the realistic pass; Pass B understates cost** and should be read
only as a ceiling on throughput, not as a per-part cost.

The same effect explains why `emulate` has a p95 of 6.08 ms in Pass A against a 2.76 ms median: it is
duty-cycle and clock variance, not a double-frame iteration (only 2 of 4500 iterations ran two frames).

### 3.3 Pass C — the real stack under Xvfb (`--mode eframe`, 75 s, audio on)

winit + wgpu + eframe, on llvmpipe. `MESA-EGL: warning: DRI3 error: Could not get DRI3 device` — i.e.
software rasterisation, confirmed.

```
display ownership CONFIRMED: toolkit reports 1281x803
loop iterations      4141
emulated frames      4477  (59.69/s)
drawn frames         4477  (59.69/s)

part               mean   median      p95      p99      max        n
emulate           3.221    2.716    7.665    9.158   14.684     4141
convert           0.046    0.036    0.108    0.134    1.378     4141
tex-upload        0.008    0.007    0.014    0.019    0.128     4141
ui-build          0.161    0.148    0.259    0.317    2.504     4141
CPU TOTAL         3.437    2.910    8.052    9.576   15.109     4141
period           18.049   16.931   26.079   29.195   39.685     4140
```

**What this does establish:**

* The whole stack **assembles and runs**: eframe + egui_dock + a per-frame emulator texture + the player's
  cpal path, in one process, for 75 s.
* **The CPU parts are unchanged by the real backend** — 2.910 ms median here against 3.014 ms in Pass A.
  So Pass A's breakdown is not an artefact of the fake harness.
* **The emulator still hits 59.69 fps even though the render loop stalls**, because `audio::frames_to_run`
  compensated: 4477 emulated frames across only 4141 iterations, i.e. 336 iterations ran two frames. The
  player's existing audio-mastered pacing survives the toolkit swap intact. That is a real result.

**What it does NOT establish** — and this is the whole reason the mode exists separately: the `period`
column (median 16.93 ms, p95 26.08 ms, max 39.69 ms) is **llvmpipe's number, not the machine's**. It is
software rasterisation of a 1281×803 surface on the same cores as the emulator. **It must not be quoted as
the player's frame rate.**

### 3.4 Audio

| | Pass A (`cpu`) | Pass C (`eframe`, llvmpipe) |
|---|---|---|
| device | 44100 Hz, 2 ch, f32 | same |
| ring capacity | 11760 samples (8 frames) | same |
| device callbacks | 2813 | 2813 |
| starvations, total | 1 | 34 |
| **starvations, steady-state** | **0** | **23** |
| leanest ring | 2940 samples (33.3 ms) | 1176 samples (13.3 ms) |
| producer drops (ring full) | 0 | 0 |

**Pacing held in Pass A: zero steady-state starvations, zero drops.**

**Pass C's 23 starvations are a finding, not a failure.** They are caused by the render loop stalling
26–40 ms, which drains a ring whose low-water mark is one frame. `crates/oracle-frontend/src/audio.rs`
already documents the dial for exactly this case:

> A machine that stalls its render loop for tens of milliseconds at a time wants 2 or 3 here; nothing else
> has to change. — `LOW_WATER_FRAMES`

**Recommendation:** if the player moves to a toolkit whose present can stall (a compositor hiccup, a shader
recompile, a resize), raise `LOW_WATER_FRAMES` from 1 to 2. It costs ~17 ms of latency and buys ~32 ms of
margin, and the table above it in `audio.rs` says all three settings give zero underruns in the current
design. This is the one concrete change the spike says the rebuild will need.

---

## 4. What is NOT measured, and why

These are reach limits of a headless instrument, not omissions.

1. **Presented fps under vsync on the real GPU.** Xvfb has no vsync and llvmpipe is not the machine's GPU.
   Pass C's `period` column is a software rasteriser's number and is reported as such. **Deferred to a
   foreground pass** on a real display — TAGGED for foreground follow-up.
2. **GPU-side cost.** Only the CPU-side `TextureHandle::set` is measured (7 µs). The actual
   `queue.write_texture` upload, the egui render pass and the swapchain present are the backend's, on the
   far side of `update()`. On a real GPU a 320×224 RGBA upload is ~287 KB/frame = 17 MB/s, which is
   trivial, but that is reasoning, not measurement.
3. **Input latency, resize, DPI, multi-monitor.** Not exercised headless.
4. **Layout persistence** — the "remember layouts" requirement. Not exercised, but it is directly
   supported: `egui_dock::DockState` derives `Serialize`/`Deserialize` under the crate's `serde` feature
   (`dock_state/mod.rs:44`). One feature flag plus a `Serialize` bound on the tab enum.
5. **Anything requiring the emulator MCP.** Not touched, per the standing rule.

---

## 5. Deliverable 2 — pricing the separate-process hedge

**Question:** can the debug panels start as a separate process talking to the existing player over Aether,
leaving today's minifb player untouched?

**Answer: yes for every panel that is a table of numbers; no for anything that has to show live pixels.**

### 5.1 What already exists

The player hosts the bus in-process (`--aether` / `--socket PATH`,
`crates/oracle-frontend/src/bus.rs`), and the dispatch table is a single const array —
`crates/oracle-aether/src/engine.rs:234`, `pub const METHODS: &[MethodSpec]` — so the advertised list and
the implemented list cannot drift (`initialize` builds `methods` from the same array, engine.rs:1918).

**56 methods.** The ones a panel app would want, verbatim:

| Panel | Methods (all `emulator/…`) |
|---|---|
| Registers / status | `registers`, `status` |
| Memory / hex editor | `read`, `read_memory`, `write_memory`, `read_vram`, `write_vram`, `read_cram`, `write_cram`, `z80_read`, `z80_write`, `memory_hash` |
| Breakpoints | `breakpoint_add`, `breakpoint_set_enabled`, `breakpoint_list`, `breakpoint_clear` |
| Watchpoints | `watchpoint_add`, `watchpoint_clear`, `watchpoint_list`, `watchpoint_hits` |
| Profiler | `set_profiler`, `get_profiler`, `get_profiler_frames` |
| Execution | `step`, `step_over`, `step_out`, `run_frames`, `run_to`, `run_to_scanline`, `pause`, `resume`, `reset`, `reload_rom` |
| Input | `press`, `play_input`, `hold`, `release_all` |
| Video introspection | `pixel_attribution`, `sprites`, `get_layer_states`, `set_layer_enabled`, `screenshot`, `scanlines` |
| Game objects | `object_list`, `object_slot`, `object_at`, `player_state` |
| Symbols | `lookup_symbol`, `load_symbols` |
| Checkpoints (volatile, 8 slots) | `checkpoint`, `restore`, `checkpoint_list`, `checkpoint_drop` |
| Player overlay text | `screen_text` |

**Every panel in the owner's list is already served.** Registers, memory, watchpoints and the profiler need
no new bus surface at all.

**There is a push channel**, which is the thing that makes a panel app viable rather than a polling toy:
`EVENTS = ["emulator/stopped", "emulator/resumed", "emulator/romReloaded"]`
(`crates/oracle-aether/src/engine.rs:632-636`), opt-in via `initialize.params.clientCapabilities.events =
true`. A panel is *told* it stopped at a breakpoint; it does not poll. (`emulator/wait_for_break` is a
poll and is already marked deprecated by that event.) Delivery is non-blocking and drops oldest-first at
1024 per connection, with the drop count reported to the client as `droppedEvents`
(`crates/oracle-aether/src/outbound.rs:31,62-77`) — a panel that stalls stalls only itself.

Wire format: NDJSON, JSON-RPC 2.0, one message per line, 1 MiB line cap. Every result, error `data` and
event `params` is merged with a `{frame, mclk, running}` machine stamp that a handler cannot shadow
(`crates/oracle-aether/src/rpc.rs:244-249`).

### 5.2 What a panel app would need to build

The existing non-MCP client is **aurora**, and it is **not Rust** — Electron/TypeScript,
`/home/volence/sonic_hacks/aurora/src/main/aether/`. Its reusable core is **~586 lines** (`client.ts` 431 +
`protocol.ts` 46 + `unserved.ts` 109), dependency-free over an injected socket. A TypeScript panel app could
copy it wholesale. A **Rust** panel app cannot reuse the source, but the design is small — `UnixStream`,
`BufReader::lines()`, a writer half, an `id → oneshot` map, a notification fan-out: **~200 lines**, and the
repo already contains two working minimal Rust NDJSON clients to crib from (`crates/oracle-frontend/src/bus.rs`
test module; `crates/oracle-aether/tests/common/mod.rs`, 260 lines).

Three robustness details worth copying from aurora rather than rediscovering:
* `initialize` **without** `clientCapabilities.events:true` gives a healthy connection that silently never
  receives an event (aurora comments this at `client.ts:203-207`);
* it has **no request timeouts at all** — a hung server is a promise that never settles. Do not copy that
  part; add one;
* it pre-checks the advertised `methods` list, so an unserved method costs no round trip and never leaves
  the machine paused mid-sequence.

**Rough size for a Rust panel app:** ~200 lines of client + ~150 lines per panel × 4 panels + a small egui
shell ≈ **1,000–1,500 lines**, with zero risk to the player, since the player is not modified at all.

### 5.3 What it could NOT show

**Live pixels. That is the whole gap, and it is structural, not a bandwidth quibble.**

There are two pixel paths and neither works at 60 Hz:

* `emulator/screenshot` (engine.rs:4747) **writes a PNG to a file** and returns `{path, format, width,
  height, bytes}` — the image bytes are never on the wire. The encoder is a hand-rolled fixed-Huffman
  deflate with greedy LZ77 (`crates/oracle-aether/src/png.rs`) over 215,040 bytes, run **synchronously on
  the thread that runs the game**.
* `emulator/scanlines` (engine.rs:4231) is the only method that puts pixels on the wire, as
  `{"line":N,"width":320,"rgb":"0x<1920 hex chars>"}` per row, built with a `format!("{b:02X}")` **per
  byte** (`crates/oracle-aether/src/hex.rs:25-32`). A full 320×224 frame is **≈440 KB of JSON and 215,040
  `format!` allocations**. At 60 Hz: ≈26 MB/s and ≈12.9 M allocations/s.

The structural reason is the pump. The player drains the bus **on its own run-loop thread**
(`crates/oracle-frontend/src/main.rs:2216-2217`), and `DEFAULT_PUMP_BUDGET` is 4 ms checked *between*
commands, never inside one (`crates/oracle-aether/src/host.rs:83,91-101`). A command that has begun runs to
completion. So a per-frame `scanlines` executes **inside** the player's 16.67 ms frame, on the same thread
as the 68000 and the 224-line render, and the budget cannot stop it.

**Verdict: a separate process cannot render a live 60 Hz framebuffer over this socket.** Realistic ceiling
is a few frames per second, or `screenshot`-to-tmpfs with `state_hash{includeFramebuffer}` as a
change-detector. There is also no `frame-drawn` event to drive a pull — the push surface is three stop/resume
events and nothing else.

Also with no bus equivalent at all (player-process only): the command palette and lens panel *text*
(a registered gap, `F-SCREEN-TEXT-PALETTE-LENS`, `screen_text.rs:20-35` — `screen_text` serves only
`titleBar`/`statusLine`/`toast`); the overlay's geometry, fades and `PAUSED` banner; symbol watches from
`player.conf` (`symbol_watch.rs`, zero Aether surface); the on-disk F2/F4 save-state slots (the bus's
`checkpoint*` family is a *different*, volatile, 8-slot in-memory mechanism); ROM browser; `player.conf`
itself; **all audio state** (volume, mute, console filter, ring occupancy — `capabilities.vgm` is
hard-coded `false`, engine.rs:1954); window geometry/aspect/scale/hover; gamepad state; `.srm` persistence.

And one that will surprise: **overlays are drawn into the presentation buffer at window resolution, never
into the retained native framebuffer** (`overlay.rs:16-23`, `present.rs:16-17`), so even
`emulator/screenshot` returns the game picture *without any overlay ink*. A cross-process panel cannot
obtain the composited image by any method that exists.

---

## 6. Correction — minifb does not own a blocking event loop

The brief's expectation was that "minifb and winit/eframe each want to own an event loop and that this is
the fragile option." **Half right, and the wrong half is load-bearing.**

`crates/oracle-frontend/src/main.rs:1456` is a plain poll loop:

```rust
while window.is_open() && running {
    ...
    if let Err(e) = window.update_with_buffer(&screen, win_w, win_h) { ... }   // main.rs:2493
}
```

`update_with_buffer` presents, pumps the OS event queue, applies the FPS limiter and **returns**. There is
no `EventLoop::run`, no callback registration, nothing that takes ownership of the thread. Input is polled
(`get_mouse_down`, `get_mouse_pos`, `is_key_down`). There is exactly **one** `Window::new` in the whole
crate (main.rs:1259), on the main thread, and **zero** `thread::spawn` in `main.rs` — the cpal and Aether
threads are spawned by their own crates and never touch the window.

So the usual blocker is genuinely absent: nothing already owns `main`.

**But the conclusion still holds, for four other reasons:**

1. **`eframe::run_native` still takes the thread.** It wraps `winit::EventLoop::run`, which consumes `self`
   and must run on the main thread. Two things cannot both own `main`. The escape hatches
   (`EventLoopExtPumpEvents::pump_events`, `run_on_demand`) are platform-limited and are *not* what
   `eframe` exposes — you would drop to raw `winit` + `egui-winit` + `egui-wgpu` and hand-roll the
   integration.
2. **Two display connections in one process.** minifb opens its own X11 `Display` — which `icon.rs` already
   reaches into via `x11-dl` to call `XSetClassHint` (`crates/oracle-frontend/src/icon.rs:22,166`) — and
   winit would open a second. On X11 that mostly works but both queues must be pumped every iteration. On
   Wayland it is materially worse (two `wl_display`s, two lots of focus/IME/clipboard bookkeeping); the
   frontend already carries a `--x11` escape hatch (main.rs:1250-1257) because that path is the fragile one.
3. **The pacing model has one master and it is the audio ring.** `frames_to_run_for` decides 0/1/2 emulated
   frames per iteration from ring occupancy (main.rs:2081-2091). Any per-iteration cost the egui window adds
   is charged to the same iteration that must produce 735 samples. §3.4 measured exactly this failing under
   llvmpipe: 23 steady-state starvations when the render stalled 26–40 ms.
4. **The Aether pump is on that thread too**, with its 4 ms budget (main.rs:2217), inside the same 16.67 ms
   that already holds emulation, capture, model-building, overlay draw, blit and present.

**Revised recommendation.** If a toolkit panel must be in-process, the shape that fits the existing code is
a raw-`winit` second window pumped with `pump_events` from inside the existing `while window.is_open()`
body, X11-only, with the egui frame skipped whenever the audio ring says the loop is behind. That is
buildable — but `eframe::run_native` specifically is not, and the safer sequencing is the one the numbers
support: **a full rebuild on eframe, replacing minifb rather than sitting beside it.** §3 says that fits in
budget with 6× headroom.

---

## 7. One pricing input that is not a performance number

**egui 0.36 is a `Ui`-first redesign, not the `Context`-first API most examples show.** Written against the
0.32-era shape, the spike failed to compile with six errors:

* `eframe::App::update(&mut self, &egui::Context, &mut Frame)` is now
  **`App::ui(&mut self, &mut egui::Ui, &mut Frame)`**;
* `Context::run` is now **`Context::run_ui`**, and its closure receives `&mut Ui`, not `&Context`;
* `TopBottomPanel::top(id)` is now **`Panel::top(id)`**;
* panels take **`.show(ui, …)`**, not `.show(ctx, …)`;
* `NativeOptions` no longer has a `vsync` field.

egui_dock 0.21.1 tracks it correctly (`DockArea::show_inside(ui, viewer)`; note there is no
`show(ctx, …)` in this version). The port itself was ~20 lines. Budget for this churn on every egui bump —
it is a fast-moving dependency, and 233 new crates come with it.

---

## 8. Reproducing

```bash
ln -s /home/volence/sonic_hacks/oracle/vendor vendor        # 17 TestRoms entries
cargo build --release --manifest-path crates/oracle-panels-spike/Cargo.toml
./crates/oracle-panels-spike/run.sh screens        # prove :0 and :77 are different displays
./crates/oracle-panels-spike/run.sh cpu 75 on      # Pass A — the answer
./crates/oracle-panels-spike/run.sh unpaced 25 off # Pass B — headroom
./crates/oracle-panels-spike/run.sh eframe 75 on   # Pass C — real stack, Xvfb, fps NOT the answer
```

`run.sh` creates the Xvfb, strips the Wayland env, and passes `--expect-screen 1281x803`; the binary
verifies that against what the toolkit reports and exits(2) rather than drawing on a display it does not own.

## 9. Gate status

* `cargo fmt --all -- --check` — clean.
* `cargo clippy --all-targets -- -D warnings` (workspace) — clean.
* `cargo clippy --all-targets -- -D warnings` (spike, run against its own manifest) — clean.
* `cargo test --workspace` — **exit 0; 2026 passed, 0 failed, 6 ignored, across 64 test binaries.** No
  suite line reports a non-zero failure count; the 12 `save_state::tests::*` rows ran rather than
  panicking, confirming the `vendor` symlink (17 `TestRoms` entries) was in place.
* Workspace `Cargo.lock` — **unmodified** (260 packages, and `git status` never lists it).
* `cargo tree -p oracle-core` — 38 entries, no `egui`/`eframe`/`wgpu`/`winit`.
* `cargo tree -p oracle-frontend` — **0** lines matching `^(egui|eframe|wgpu|winit)`.
