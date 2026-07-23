# Overnight verification audit — sound stack (2026-07-23)

**Scope:** independent double-check of the sound-stack session arc (`bf5b250` FM Timer-A/B,
`e0c7d90` SY-1 PSG, `3839807` SY-2 FM, `0d7e4dd` SY-3a DAC) requested by the owner before bed.
Method: firsthand gate runs by the auditing overseer + two independent code-review agents
(one per commit group, run in clean detached worktrees) + one adversarial source-verification
agent for the single Important finding. No source files were edited by this audit.

## Everything green (verified firsthand, fresh runs)

- `cargo fmt --check` clean; `cargo clippy --workspace --all-targets` 0 warnings (default
  and `--features synth`).
- Full workspace test suite: **all suites 0 failures**, including 627 lib tests, both SST
  corpora (68000 112 tests / 868 s; Z80), `export_state_v1` frozen-golden, `determinism_gate`,
  `golden_frames`. Lib tests at HEAD: 627 default / 644 with `--features synth`.
- Money test reproduced exactly: `vgm_capture s4.soundtest.bin` → `fm_writes: 5951`,
  psg 8, notes spanning frame 1→599, timer rearm `$27=$15` per frame.
- SY render reproduced exactly: `synth_render` → 98.2% non-silent.
- SY-3a differential: WAV at `3839807` vs `0d7e4dd` is **byte-identical** (md5 `6585e637…`) —
  this *confirms* the SY-3a commit's "this ROM streams no PCM (all $2A = $80 keep-alive)"
  claim; the DAC path is exercised by its 5 unit tests only, as that commit states.
- Both reviewers verified the currency contract holds: synth feature-gating airtight
  (`lib.rs` cfg gate, no default features, caller-owned sink), FM timer state correctly in
  bincode snapshot but excluded from `state_hash` and `export_state` (region 6 still the
  all-zero placeholder; frozen golden green).
- Reviewer verdicts: `bf5b250` APPROVE (timer periods/register map/lazy-flag arithmetic all
  verified against OPN2 ground truth; no double-advance possible — reads are pure on one
  scheduler timeline). `e0c7d90` APPROVE with one Important finding (below). `3839807`
  ship-worthy (SLOT_MAP, $28 key-bit order, all 8 algorithm topologies, fnum/block math,
  $A4-latch ordering, panic-freedom of the render path all checked correct).

## Findings docket (ranked; none blocks what's shipped — the target ROM exposes none of them)

### F1 — CONFIRMED BUG: SN76489 noise LFSR clocked 2× too fast (SY-1)
`crates/oracle-core/src/synth/sn76489.rs` (`Noise::reload_ticks` + `next_sample`): the LFSR
shifts on **every** counter underflow (reloads 0x10/0x20/0x40 in the clock/16 domain →
clock/256, /512, /1024; mode 3 at 2× tone-2 frequency). Real hardware shifts only on the
counter output's 0→1 transition — **every other underflow**: clock/512, /1024, /2048; mode 3
at exactly tone-2's frequency. Adversarially verified against three independent sources that
all agree: Maxim's SN76489 doc (SMSPower lineage; "only ONCE for every two times the related
counter reaches zero"), MAME `sn76496.cpp` (`1 << (5+(n&3))`, mode 3 = `m_period[2] << 1`),
and TmEE's real-hardware measurements (NESdev). Consequence: periodic-noise bass (the classic
SMPS mode-3 trick) renders one octave high (e.g. tone-2 period 254 → 55 Hz instead of 27.5 Hz);
white noise an octave too bright. **Minimal fix:** shift the LFSR only every other underflow
(or double all reloads, including doubling the tone-2 period in mode 3). Add a rate unit test
pinned to clock/512-family numbers. NOTE: `seraph/src-tauri/src/sn76489/chip.rs:132-160`
carries the identical wrong convention — same fix applies there.

Related, verified CORRECT: tone period 0 → `max(1)` matches the Sega-VDP variant (TmEE:
"$000 acts same as $001"). Optional synthesis nicety: render period ≤ 1 as constant +volume
(the ~112 kHz square aliases at 44.1 kHz; DC is what PCM-via-attenuation tricks assume).

### F2 — FM key-on is level-triggered, should be edge-triggered (SY-2)
`ym2612_synth.rs` (`key_on` path from `$28`): every key-on write resets phase and restarts
attack; real OPN2 retriggers only on 0→1. A driver that re-asserts `$28` per tick during a
held note would buzz/click. S4's SMPS writes `$28` per note event, so inaudible today.
Fix: retrigger only when the EG is Off or Release.

### F3 — DAC-enable freezes ch6 FM envelopes (SY-2, still present at HEAD)
`ym2612_synth.rs:617-620`: the `continue` skips `ch.next_sample()`, freezing ch6 EG/phase
while `$2B` bit7 is set; hardware keeps the EG running muted. A note releasing when DAC turns
on will pop back at stale volume when DAC turns off. Fix: run ch6's sample generation and
discard the FM output while DAC is active.

### F4 — hardening/quality nits
- `Ym2612Synth::write` is `pub` and `bank >= 2` indexes out of bounds (`channel_index`);
  unreachable from committed call sites — mask `bank & 1` or `debug_assert!`.
- Mixing headroom: worst case 6ch × 4 carriers × FM_LEVEL 3500 = 84,000 pre-clamp vs i16
  max — dense passages will hard-clip (clamped, so quality not safety).
- FM-timer design doc: add to the FM10 deferral list the two unpinned lazy-model edges:
  (a) ENBL 0→1 after dormant overflow reads 1 immediately (real chip waits for next
  overflow); (b) rewriting `$24/$25` mid-run rescales past boundaries (real chip applies new
  NA at next reload). Not exposed by NA=137-fixed SMPS; would matter for mid-song retuning.
- Commit-message bookkeeping: `bf5b250` says "55 lib tests" (actual 627 at that point);
  `3839807` says 640 synth tests (644 at HEAD incl. SY-3a's 4).
- `tools/__pycache__/` is untracked noise — add `__pycache__/` to `.gitignore`.

## Suggested slice: SY-1b (PSG noise-rate fix)
F1 is a one-file, source-pinned fix with an obvious unit test — a clean bounded slice before
or alongside SY-3b. F2/F3 fold naturally into SY-3c (exact envelope generator), which rewrites
the EG state machine anyway.
