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
| **Measured** | §4. Two runs of the display-independent pass, reported separately, never averaged. |
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

See §4.1–§4.3 below.

---

## 5. Reproducing

```bash
ln -s /home/volence/sonic_hacks/oracle/vendor vendor        # 17 TestRoms entries
cargo build --release -p oracle-player
./crates/oracle-player/run-bench.sh screens          # prove :0 and :77 are different displays
./crates/oracle-player/run-bench.sh cpu    75 on     # THE ANSWER — display-independent
./crates/oracle-player/run-bench.sh window 75 on     # real stack on Xvfb; presented fps is REFUSED
```

**The instrument rule, and where it is enforced.** The owner is using this machine.

* `run-bench.sh` starts its own `Xvfb :77 -screen 0 1281x803x24 -nolisten tcp`, kills only a server it
  started itself, and strips `WAYLAND_DISPLAY` / `XDG_SESSION_TYPE`. The geometry is deliberately one no
  real monitor is, so the check below is a discriminator rather than a coincidence.
* **The environment is setup, not the guard.** The guard is that the binary asks the *toolkit* for its own
  screen size on the first frame and `exit(2)`s without drawing on a mismatch — and that it **refuses to
  start any bench mode at all without `--expect-screen`**.
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
| Presented frame rate on the real GPU | Reach limit of a headless instrument, not an omission. See §4.3. **TAGGED for the foreground pass.** |
