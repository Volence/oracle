# Timing adjudication — is Oracle's "~10-tick startup drop" cycle-principled ground truth?

> **⚠️ CORRECTION (overseer, 2026-07-23): the "Verdict: B — artifact / no real drop" in this doc's BODY is WRONG and
> superseded.** It rested on a bad control: Oracle's VGM was armed AFTER boot (post-reset auto-resume ran ~5 s ahead),
> so it sampled only the 60 Hz steady state and skipped the startup window where the drop is seeded. A controlled
> clean-boot re-capture (see the "Tiebreaker: fresh-both position diff" section appended below) REPRODUCES the RT-3
> divergence exactly (identical prefix ends at write 5,153), and the `docs/2026-07-23-timing-ground-truth-fable.md`
> investigation then established the drop is **REAL but is Oracle over-dropping** (an over-conservative
> `ClampHandshakeTimeDeterministic` bus-arbitration model, `oracle/Devices/MD1600IO/MDBusArbiter.cpp`), so **ours is the
> tick-accurate side.** Net answer is still "don't change our Z80," but the reasoning in the body below is superseded —
> read the tiebreaker section + the fable doc for the correct picture. Retained for the audit trail.

**Status: MEASUREMENT / ADJUDICATION, 2026-07-23. No core edits, no commits.** Follow-up to
`docs/2026-07-23-subframe-drift-triage.md`, which framed the RT-3 Oracle A/B divergence as a **bounded ~10-tick
sequencer-count offset acquired during the first ~1.5 s of song startup**, where "Oracle drops ~10 heavy-frame
ticks (runs the sequencer at ~55 Hz over ticks 15–100) that our Z80 timing does not." The open question this
doc answers: **is that Oracle startup drop cycle-principled hardware truth (→ ours is inaccurate, needs a
bounded Z80 op-cost fix), or is Oracle itself artifacted (→ inconclusive)?**

## Verdict: **B — the alleged startup drop is a measurement artifact; ours is already accurate.**

Sharper than "inconclusive": a **fresh, live, deterministic re-capture of Oracle on the current ROM does not
reproduce any startup drop at all.** Live Oracle runs a clean ~60.05 Hz with exactly one sequencer tick per
frame across the entire alleged 15–100 drop window — **identical to ours.** There is no real per-frame timing
divergence to chase, and **no cycle-principled basis for one exists.** Do **not** make a Z80 op-cost change.

---

## 1. What I measured (all numbers are real, measured this session)

### 1.1 Live Oracle VGM re-captures (the primary evidence)
Loaded `/home/volence/sonic_hacks/aeon/s4.soundtest.bin` into Oracle, hard-reset, started VGM logging **at
boot** (68k PC = `BuildStaticDMA`, before the song loads), ran, stopped. Counted `$27=$15` writes (Timer-A
rearm = one `Sequencer_Frame` tick) per absolute VGM second and per 1/60 s frame. Oracle uses sample-accurate
`0x61` waits, so per-VGM-second counts are a **true, timebase-independent** clock. Two independent runs:

| Capture | Dur | Ticks | Per-second tick counts | Startup window (frames 0–129) |
|---|---|---|---|---|
| Oracle run 1 | 6.81 s | 411 | **61, 60, 60, 61, 60, 60,** (49 partial) | every frame = 1 tick, except boot frame 1 = 2 / frame 2 = 0, beat frame 50 = 2 |
| Oracle run 2 | 5.71 s | 344 | **60, 60, 60, 61, 60,** (43 partial) | frames 0–89 **all exactly 1 tick** |

**Zero seconds fall below 60 ticks. Zero dropped-tick runs in the 15–100 window.** The occasional `61` and the
lone double-tick frames are the *correct* 60.053-vs-59.922 beat (the same effect §1.3 of the triage doc
attributes to ours). Oracle is running **60.053 Hz**, slightly *ahead* of 60, not the ~55 Hz the triage doc
reported. Result is deterministic across both runs.

### 1.2 Our side, identical methodology (cross-validates the parser)
`cargo run --release -p oracle-core --example vgm_capture -- .../s4.soundtest.bin 430`, same tick counter:

| Capture | Dur | Ticks | Per-second tick counts |
|---|---|---|---|
| Ours | 7.10 s | 426 | **58, 61, 60, 60, 60, 60, 60,** (7 partial) |

Ours = ~60.05 Hz with a couple of boot-transient zero-frames (frames 0, 8, 9) then locked. **Ours and live
Oracle agree**; both hold the driver's intended Timer-A rate. The handful of absolute-count difference over the
window is boot-alignment noise (which frame the song's first tick lands + where logging armed), **not** a
sustained 55 Hz regime in either.

### 1.3 The driver author already characterized (and compensated for) the *real* effect
`s4.lst` around the Timer-A tuning constant (source: `engine/sound/z80_sound_driver.asm`) documents a direct
measurement **against Oracle**:

> "the effective tick rate at N=136 measured **59.873 Hz (3597 ticks / 3600 emulated frames**, SND_STAT_TICK vs
> oracle frame count; **deficit = ~3 residual long-tick overruns per minute**). Pin ONE N-step tighter so the
> DELIVERED rate lands on 59.92 ± 0.02 … **N=137** ← target 60053 mHz."

So the genuine tick-vs-frame deficit — occasional "long-tick overruns" where one `Sequencer_Frame` runs past a
Timer-A period and swallows a tick — is **~3 ticks per MINUTE** (~0.05 Hz), and the shipped ROM already **tunes
NA = 137 to compensate**, landing Oracle's delivered rate on ~60 Hz (which my live captures confirm). The triage
doc's alleged "~10 drops in ~1.5 s" is **~200× denser** than the real, author-measured long-tick rate — it is
inconsistent with both the author's calibration and my live captures.

### 1.4 Z80 duty-cycle spot checks (why a dense drop is physically impossible)
Oracle breakpoints/profiler are 68k-only, so I sampled the Z80 PC by single-stepping the 68k in batches (the
Z80 catches up concurrently) and reading `z80_registers`. Idle-spin poll = Z80 PC in **$00CF–$00F4**
(`SndDrv_Idle`); frame work = `SndDrv_IdleTick` $00F5 → `Run_SeqFrame_OnSongBank` $0339 → `Sequencer_Frame`
$0565 (all Z80 addresses from `s4.lst`).

- **Steady state (song-frame ~340):** 5/5 samples in the idle-spin ($00E2, $00DF, $00E4, $00F1, $00E2), SP=$1FFE
  (no nested calls). Z80 is waiting on the overflow flag essentially the whole frame.
- **Startup window (song-frame ~12, the alleged heavy region):** 7 samples → **1** caught mid-work ($022E,
  SP=$1FEE = nested calls, genuine `Sequencer_Frame`/ISR work) + **6** in the idle-spin. The work window was
  bounded to **< a few hundred 68k instructions** (idle both 200 and 400 68k-steps away from the one work hit),
  against **~16,700 68k instructions per frame** measured here (200,000-step run advanced 12 frames).

**Budget math.** One Timer-A period = 894,096 mclk = **59,606 Z80 cycles** (MCLK_PER_Z80_CYCLE = 15). The
heaviest `Sequencer_Frame` (the 87-register-write startup frame) is on the order of ~8k–15k Z80 cycles (≲25 % of
**one** period), consistent with the ≲few-percent duty samples. A **dropped tick requires the per-tick blind
window to span two overflows — i.e. per-frame work > 2 periods ≈ 119,000 Z80 cycles.** Measured work is short of
that by ~8×. Two overflows in one blind window **cannot occur** here on either emulator; both are pinned to the
Timer-A rate.

---

## 2. Why the triage doc saw a "~10-tick offset"
The triage doc compared **our** 29.93 s / 1,799-tick capture (frame-bucketed `0x62` = exactly 735 samples/frame
→ a 60.000 Hz-by-construction timebase) against an on-disk `oracle_s4b.vgm` (56.75 s / 3,400-tick,
sample-accurate `0x61`). 3,400 / 56.75 = 59.91 Hz vs 1,799 / 29.93 = 60.11 Hz — a ~0.2 Hz wall-clock skew that,
over ~30 s plus a capture-start offset, force-aligns to look exactly like "one stream dropped ~10 ticks early,
then matched forever." That signature (bounded offset acquired early, frozen after) is precisely what a
**constant capture-start + timebase skew** produces — the very skew the triage doc itself flagged in its §1.4
caveat. The original `oracle_s4b.vgm` is no longer on disk; the live re-capture (§1.1), measured
timebase-independently (ticks per absolute VGM second), supersedes it and shows no drop.

## 3. Recommended next action
1. **Do NOT pursue a Z80 op-cost "fix."** There is no reproduced divergence, no cycle-principled drop, and the
   physical budget (§1.4) rules one out. A Z80 cost change would carry currency-review risk for zero accuracy
   gain and could only *manufacture* drops that live Oracle does not have.
2. **Amend the triage doc's premise.** Its §2 #1 attribution ("Oracle drops ~10 startup ticks our Z80 misses")
   is not reproduced; the offset is a capture-alignment artifact. Ours already runs the driver's intended,
   author-calibrated (NA=137) 60.053 Hz — the same rate live Oracle runs.
3. **If any future A/B is run, fix the methodology:** capture both emulators with the **same** VGM timebase
   (both sample-accurate `0x61`, or both frame-bucketed) and compare **ticks per absolute VGM second**
   (timebase-independent), not force-aligned wall-clock triple positions. That removes the artifact at the
   source.
4. **The only real residual** is the author's documented **~3 long-tick overruns per minute** (≈0.05 Hz),
   already compensated by NA=137. Ours may drop 0 of these vs Oracle's ~3/min — a sub-0.1 Hz curiosity, not a
   bug, not worth the currency risk to match.

## 4. One-line verdict
**B.** Live, deterministic Oracle on the current ROM does **not** drop startup ticks — it runs a clean 60.053 Hz
with one tick per frame across the whole alleged 15–100 window, identical to ours. The triage doc's "~10-tick
startup drop" is a VGM capture-alignment / timebase-skew artifact of a stale reference file, corroborated by the
driver author's own measurement (real deficit is ~3 ticks/**minute**, already NA-compensated) and by the Z80
cycle budget (a dense drop needs >2 periods of per-frame work; the heaviest `Sequencer_Frame` is ≲¼ of one).
Ours is accurate; there is nothing to fix.

---

## Tiebreaker: fresh-both position diff (2026-07-23, third agent, MEASUREMENT ONLY)

Agent 1 (`subframe-drift-triage`) and Agent 2 (§1–4 above) reached opposite conclusions. This tiebreaker ran
the single decisive experiment neither ran cleanly: a **fresh capture of BOTH emulators from the SAME clean
boot point over the same window**, then the position-for-position diff. **The evidence supports Agent 1: the
~5,153-write divergence is REAL and reproduces with a fresh live Oracle capture — it is NOT a stale-file
artifact.**

### Controls enforced (this is what made the difference)
- **Same ROM, byte-verified.** `/home/volence/sonic_hacks/aeon/s4.soundtest.bin`, sha256
  `4d2574f6…95cf4`; Oracle `emulator_reload_rom` confirmed size 429322 identical.
- **Same clean boot point — the load-bearing control.** Ours boots via `sys.reset()` (frame 0; first sound
  write at frame 1). For Oracle, arming at `BuildStaticDMA` is **NOT** clean boot: the deferred `emulator_reset`
  auto-resumes and, uncapped, runs ~5 s ahead before the arm — so the capture **misses the entire driver YM
  init** (`$22 $2B $24 $25 $27=05`, PSG mute, first `$28` batch) and starts mid-song (verified: a first attempt
  armed this way diverged at melody index **0**, a garbage comparison). The correct arm point is the **pristine
  power-on state** reached by `reset` → immediate `pause`, confirmed by `PC=0xFFFFFFFF, SP=0xFFFFFFFF,
  SR=0xFFFF` (reset vector not yet fetched, before any sound write). Armed there, Oracle's stream head is
  **byte-identical to ours** (`$22=08, $2B=80, $24=22, $25=01, $27=05, $27=15×2, PSG-mute×4, $28 2/1/0/6/5/4…`).
  **This is almost certainly why Agent 2 saw "no drop": arming after boot skips the very ~1 s startup window
  where the drop is seeded, so per-second counting only ever samples the 60 Hz steady state.**
- **Window:** ours = 1800 frames (30.0 s emulated); Oracle run to 32.2 s so the common prefix is bounded by ours.
- Both parsed with `tools/vgm_diff.py` (strips all waits → timebase-independent position comparison).

### Result — the fresh diff reproduces 5,153 exactly
`python3 tools/vgm_diff.py ours_fresh.vgm oracle_fresh2.vgm`:

| Measure | Value |
|---|---|
| Melody writes ours / Oracle | 17,665 / 19,434 |
| **Exact position-for-position prefix** | **first 5,153 writes IDENTICAL** |
| Writes only-in-ours (multiset) | **0** |
| First divergence, melody index 5,153 | ours `$A4=02` (note freq) vs Oracle `$27=15` (sequencer tick) |

The identical prefix ends at **5,153** — the same bounded early point as RT-3 — with the same signature (Oracle
emits an extra `$27=$15` sequencer tick where ours emits note data: a one-tick phase slip, not a wrong value).
**Agent 2's premise that this was an artifact of the deleted `oracle_s4b.vgm` is refuted: a fresh, clean-boot,
live Oracle capture produces the identical 5,153-then-diverge result.**

### Cross-check — startup tick cadence (`$27=$15` per frame/second), both fresh captures
| | Ours (`ours_fresh.vgm`) | Oracle (`oracle_fresh2.vgm`) |
|---|---|---|
| Overall rate | 1,799 ticks / 29.93 s = **60.100 Hz** | 1,924 ticks / 32.18 s = **59.792 Hz** |
| ticks in VGM second 0 | 58 | **50** |
| ticks/sec thereafter | 60–61 steady | 60–61 steady |
| startup inter-tick gaps (frames) | all 1.0 except one shared 3-frame gap at tick 6 (both ≈3.0) → double-tick | same shared 3-frame gap, **PLUS an extra 8.438-frame stall at tick 20** (~140 ms, no tick) that ours lacks |

**Tick-count lead of ours over Oracle, sampled at equal VGM-second:** `+8` at 1 s, `+9` at 2 s, `+9` at 5 s,
`+9` at 8 s, `+10` at 10 s, `+8` at 15 s, `+9` at 20 s, `+10` at 25 s. A **roughly constant ~+9-tick lead
acquired at startup and frozen** — not linear growth. This matches Agent 1's "+9 to +11 constant from 2 s to
25 s" almost exactly. The offset is localized: Oracle's **~8.4-frame stall in second 0** (≈8 dropped ticks)
accounts for essentially the whole constant lead. Ours holds a clean 60.1 Hz through the same window and does
not stall.

### Settled verdict — **Agent 1 direction**
A **real, reproducible behavioral divergence** exists. From a byte-identical clean boot, both streams are
position-for-position identical for **5,153** melody writes, then part on a one-sequencer-tick phase slip.
The seed is a **bounded ~8–10-tick startup offset**: Oracle drops ~8 ticks during the first second (a distinct
~8.4-frame Timer-A stall in the heavy song-load window) that our clean-60 Hz Z80 timing does not; both then run
steady 60 Hz and the offset freezes. This is Agent 1's model, confirmed with fresh clean-boot captures of both
emulators. Agent 2's "artifact of a stale file" verdict does not survive the controlled re-capture — its
Oracle capture, armed after boot, never sampled the startup window where the drop lives.

**Scope note (unchanged from Agent 1 §3.1):** this establishes the divergence is *real*, **not** *who is
hardware-correct*. Whether Oracle's ~8-tick startup stall is ground truth (→ ours slightly optimistic on the
heavy startup frames) or Oracle over-drops (→ ours correct) still needs a third reference (BlastEm / real-HW
VGM) and is out of this tiebreaker's scope. No core edits, no commits; measurement only.

