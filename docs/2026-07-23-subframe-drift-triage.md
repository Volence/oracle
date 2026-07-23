# Sub-frame drift triage — where the RT-3 Oracle A/B divergence really comes from

**Status: DIAGNOSIS, 2026-07-23. No core edits, no commits.** Follow-up to
`docs/2026-07-23-rt3-oracle-ab-findings.md`, which found the sound stream byte-identical to Oracle for the
first ~5,153 writes (~8 s) then diverging on an accumulating "sub-frame timing drift." This doc characterizes
that divergence quantitatively, attributes it, and scopes the #1 fix. The headline **corrects the prior
framing**: the divergence is **not** an accumulating per-frame precision drift — it is a **bounded ~10-tick
sequencer-count offset seeded during the first ~1.5 s of song startup**, after which both emulators run the
sequencer at a **byte-exact 60.000 Hz steady state**.

Reproduced with `cargo run --release -p oracle-core --example vgm_capture -- .../aeon/s4.soundtest.bin 1800`
(our capture) vs the on-disk Oracle reference `oracle_s4b.vgm` (the RT-3 MCP capture, 56.75 s). Analysis
scripts in the session scratchpad (`analyze.py`, `compare_ticks.py`, `drift.py`, `rate.py`).

---

## 1. Drift characterization (numbers)

### 1.1 The measured divergence
- **Byte-identical prefix:** first **5,153** melody writes position-for-position identical (reproduced exactly;
  0 writes only-in-ours). Confirms the RT-3 result.
- **First divergence:** melody index **5,153**, our frame **582**, our wall-clock **~9.70 s**. At that index
  Oracle emits `$27=$15` (a sequencer frame-tick / Timer-A rearm) where **ours** emits note data `$A4=02`
  — i.e. a **one-sequencer-tick phase slip**, not a wrong value. Frame 582 is a heavy sequencer event
  (87 register writes in that frame vs the usual 2), which is why the accumulated offset first *manifests*
  there.
- **Greedy resync** (±40) recovers only 66 %, consistent with a whole-tick phase offset, not value corruption.

### 1.2 Sequencer tick cadence — the load-bearing measurement
The driver emits exactly one `$27=$15` (Timer-A rearm) per `Sequencer_Frame` tick, so counting `$27=$15`
writes = counting sequencer ticks. Both captures confirm **one tick per frame heartbeat**.

| | Ours | Oracle |
|---|---|---|
| Total ticks | 1,799 in 29.93 s | 3,400 in 56.75 s |
| **Steady-state tick rate (ticks 100–500)** | **60.000 Hz** | **60.000 Hz** (identical, 16.667 ms/tick) |
| Ticks 0–15 | ~60 Hz | ~60 Hz (aligned to <2 ms) |
| Ticks ~15–100 (~0.3–1.8 s) | ~60 Hz | **~55 Hz (slower)** |

The tick-count *lead* of ours over Oracle, sampled at equal wall-clock, is **roughly constant at +9 to +11
ticks from 2 s all the way to 25 s** — it does **not** grow linearly. The entire lead is acquired in the
**ticks ~15–100 window (first ~1.5 s)**, where Oracle's sequencer runs ~55 Hz (drops/delays ~10 ticks worth of
work) while ours holds a clean 60 Hz. After tick 100 both run 60.000 Hz and the offset is frozen in.

### 1.3 The steady-state beat (a *correct* effect, not drift)
Our stream shows lone double-tick frames at **98, 558, 1018, 1478 — spaced exactly 460 frames apart**, and
zero-tick frames interleaved. This is the real physical beat between the Timer-A music clock
(NA=137 → 894,096 mclk → **60.053 Hz**) and the video frame (896,040 mclk → **59.922 Hz**):
beat period = 1/(60.053−59.922) ≈ 7.63 s ≈ **457 frames** ≈ the observed 460. This is the driver's
*intended* hardware behavior (the whole reason the FM-timer slice exists) — the ±1-tick wobble is correct and
is **not** the divergence source.

### 1.4 Timebase caveat
Our VGM uses frame-bucketed 735-sample waits (`0x62`); Oracle uses sample-accurate `0x61` waits. Cross-emulator
*wall-clock* comparisons therefore carry a ~1–2 % timebase skew. All load-bearing claims above use
**timebase-independent** measures (tick-index cadence, position-for-position triples), which are immune to it.

---

## 2. Root-cause attribution (ranked, with evidence)

The prior doc named three suspects. The data **re-ranks them and re-scopes the problem** — the divergence is a
startup-cadence transient, not steady-state precision loss.

### #1 (DOMINANT) — Z80 execution-timing fidelity on the heavy *startup* code paths
**Evidence:** the entire ~10-tick lead is acquired in ticks ~15–100 where Oracle runs the sequencer at ~55 Hz
and ours at 60 Hz; steady state (tick 100+) is byte-exact 60.000 Hz in both. Mechanism: during song startup the
driver runs long `Sequencer_Frame` passes (the 87-write frames — loading patches, filling the DAC ring). On a
finer timing model the driver can stay busy **longer than one Timer-A period (894,096 mclk ≈ one frame)** and
**miss** an overflow — the status flag is a single sticky bit, so a missed boundary drops that tick, pulling the
effective rate below 60 Hz. Oracle drops ~10 such ticks over the first ~1.5 s; **ours drops none**, because our
instruction-atomic Z80 (SST T-state totals, `MCLK_PER_Z80_CYCLE=15`, catch-up in bursts bounded by 68k
instruction granularity) evidently under-spends time in those heavy frames and never stays busy past a period
boundary. The lever is the **summed Z80 cycle cost of the heavy startup paths + the 68k↔Z80 interleave** that
sets how much Z80 time is available per burst. This is the only suspect whose signature (a startup-only cadence
difference that then freezes) matches the data.

### #2 (MINOR) — FM Timer-A observation granularity
**Evidence:** the Timer-A flag is **mclk-exact in value** (`ym2612.rs` computes it as a pure function of
`(registers, now)`), and it reproduces the driver's intended 60.053 Hz to the last digit — the steady-state
60.000 Hz match and the correct 460-frame beat both confirm the timer model is right. Its only residual effect
is the ±1-tick beat wobble (§1.3), which is *correct*. It is **not** "frame-granular" as the prior doc's caveat
implied; it is period-exact. Contribution to the divergence: essentially none beyond seeding *which* frame a
tick lands in (a sub-frame phase the driver's poll already tolerates). It matters only in that it is *observed*
at Z80 instruction boundaries — folding it into #1.

### #3 (NONE, for this metric) — VGM frame-bucketing
**Evidence:** `tools/vgm_diff.py` strips all wait commands; `vgm.rs` pushes records in arrival order regardless
of frame stamp. Frame-bucketing therefore **cannot change the triple order** the position metric compares — it
is provably irrelevant to the measured 5,153-then-diverge result. (It does blur the *audible* phase in the
rendered WAV, but that is a separate rendering nicety, not the A/B divergence.)

**Ranking: #1 dominates (it is the whole mechanism); #2 is correct-as-built and only folds into #1's observation
step; #3 has zero effect on the diff.**

---

## 3. Scoped fix plan for #1

### 3.1 The strategic call first
Three facts reframe "fix": (a) steady-state tick rate is **byte-exact 60.000 Hz in both**; (b) the divergence is
a **bounded, one-time ~10-tick startup offset**, not runaway drift; (c) **ours runs at the driver's
hardware-intended Timer-A rate (60.053 Hz)** — the aeon driver is explicitly Timer-A-clocked. It is *not
established that ours is wrong*: Oracle may be over-dropping startup ticks (modeling the Z80 slower), or
hardware may genuinely drop them. **Do not chase Oracle by degrading our proven-correct timer** (e.g. pinning
Timer-A to the frame period) — that would reduce accuracy, need an owner gate, and still not phase-match.

### 3.2 Recommended path (cheap, zero-risk, do this first)
1. **Adjudicate against a third reference.** Capture the same ROM on BlastEm (or a real-hardware VGM of this
   SMPS song) and measure the startup tick cadence (ticks 15–100). This decides whether the ground truth drops
   ~10 startup ticks (Oracle-correct) or holds 60 Hz (ours-correct). Pure measurement, **no core change, no
   currency risk.**
2. **Instrument the heavy startup frames.** Add a throwaway sink/counter (dev-tool only, like `vgm_capture`)
   that logs, per `Sequencer_Frame`, the Z80 mclk spent between successive Timer-A polls and whether it exceeds
   `894,096` mclk (one period). Confirm directly whether ours ever stays busy past a boundary. This localizes
   the fix to specific op-cost paths (DAC ring-fill, patch loads) without touching core logic to *diagnose*.

### 3.3 If the owner wants to close the offset (only after 3.2 shows ours is the one that's off)
- **Likely bounded fix, NOT a Z80 rewrite:** correct the summed T-state cost of the few heavy startup paths
  (whatever ops the instrumentation shows undercounted) so our driver stays busy past a Timer-A boundary in the
  same frames Oracle does, dropping the same ~10 startup ticks. This is a data-accuracy fix to specific
  Z80 instruction costs, contained to the SST cost table — the steady-state 60.000 Hz match proves the *bulk*
  model is already right.
- **Explicitly NOT required:** a full sub-instruction / cycle-exact Z80 core, or sub-instruction 68k↔Z80 bus
  contention. The byte-exact steady-state rate demonstrates the expensive rewrite would buy nothing here; it is
  not justified by this data.

### 3.4 Risk to proven properties
- **Currency-neutrality:** the recommended path (3.2) changes no `src/` logic → all five frozen currencies
  untouched. Even a 3.3 T-state correction touches only the Z80 op-cost table, which is **not** exercised by any
  frozen currency (no committed fixture releases the Z80), so it stays currency-neutral by the same argument as
  the FM-timer slice (FM8). Verify by re-running the five gates.
- **Byte-identical first-8 s:** the recommended path cannot endanger it (no logic change). A 3.3 fix would
  *increase* prefix agreement (aligning the startup), not reduce it — but must be validated by re-diffing to
  confirm the 5,153-prefix does not regress.

### 3.5 Success metric
Re-run `vgm_diff.py`: target is (a) the position-for-position prefix extends well past 5,153, and (b) the
tick-count lead (§1.2) falls to ~0 through the first 2 s. The greedy-resync % is a secondary gauge (should rise
from 66 %). Note that perfect equality is unlikely while the timebases differ (§1.4) and while the correct
60.053-vs-59.922 beat exists — the realistic target is "startup offset eliminated," not "byte-identical whole
run."

---

## 4. One-line verdict
The RT-3 "sub-frame timing drift" is really a **bounded ~10-tick sequencer-count offset acquired during the
first ~1.5 s of song startup**, where Oracle drops ~10 heavy-frame ticks that our Z80 timing does not; both then
run a byte-exact 60.000 Hz. The FM-timer model is correct (period-exact, right beat) and VGM bucketing is
irrelevant to the metric. The right next step is **measurement against a hardware/BlastEm reference to decide
who is correct at startup**, not a sub-instruction Z80 rewrite (which the data shows is unnecessary).
