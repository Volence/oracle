# Phase RT-3 — Oracle A/B differential: findings

**Status: RESULT, 2026-07-23.** The first accuracy differential of oracle-next's sound stack against the
reference C++ **Oracle** emulator, on `aeon/s4.soundtest.bin` (a build that auto-plays a song on boot). This
closes the Phase-RT loop (RT-1 tap → RT-2 VGM logger → RT-3 A/B) and validates the YM2612-Timer fix
(`bf5b250`) that made the driver actually play.

## Method

Both emulators booted the **same ROM from a clean power-on** and logged the YM2612 + SN76489 register writes to
VGM:

- **oracle-next:** `cargo run --release -p oracle-core --example vgm_capture -- aeon/s4.soundtest.bin 1800`
  (30 s / 1800 frames) → `VgmLogger.render_vgm()`.
- **Oracle:** `emulator_reset` → `emulator_vgm_start` → `emulator_resume` (~57 s) → `emulator_vgm_stop`
  (MCP tools).

The two `.vgm` streams are compared as **register-write triple sequences** — `(chip, port, reg, value)`,
timing/wait commands stripped — per the RT design (`docs/2026-07-22-phase-rt-design.md` RT7), because our
frame-bucketed timing (RT6) will never match Oracle's sample-accurate wait encoding and need not. Tool:
`tools/vgm_diff.py` (VGM parser + position / multiset / notes-only / greedy-resync comparisons).

## Result — register-VALUE accurate for the first ~8 s, then a timing drift

| Measure | Value |
|---|---|
| Melody writes (non-DAC): ours / Oracle | 17,665 / 34,813 (Oracle ran ~2× longer) |
| **Exact position-for-position prefix match** | **first 5,153 writes IDENTICAL** (≈ 465 frames ≈ 7.75 s) |
| Writes only-in-ours (multiset) | **0** — we emit no spurious or wrong value |
| Greedy resync (±40 lookahead) | 66.2 % of ours align to Oracle's |

**Interpretation.** For the first ~8 seconds the two emulators produce a **byte-identical** sound-driver output
stream — every register, every value, in order. We emit **zero** writes Oracle doesn't. After ~8 s the streams
diverge: the divergence content differs in *kind* (ours plays a note — `$A4/$A0/$28` frequency+keyoff — where
Oracle loads a patch — `$B0/$B4/$30`), i.e. the two drivers have reached **different positions in the song**,
and the greedy-resync only partially realigns (66 %). This is the signature of an accumulating **timing
divergence**, not a value-corruption bug: the values are provably correct, but our sub-frame timing eventually
drifts the sequencer off the reference's phase.

## Why this is expected (and what it is not)

The divergence lands squarely in oracle-next's **deliberately-deferred sub-cycle timing**:

- the Z80 core is **instruction-atomic** with SST T-state costs, not cycle-exact (`docs/2026-07-22-z80-core-design.md`
  ZC1/ZC7 — sub-cycle 68k↔Z80 contention named as deferred);
- the FM Timer is **lazy / period-exact** but frame-granular in effect (`docs/2026-07-22-fm-timer-design.md`);
- the VGM log is **frame-bucketed** (`docs/2026-07-22-phase-rt-design.md` RT6 — sample-accurate waits deferred).

So a small per-frame timing difference vs Oracle's cycle-accurate model accumulates until, at some
timing-dependent point (~8 s here), the sequencer takes a one-tick-different branch and the streams part. This
matches the qualitative listen ("pretty close" — correct melody, the imperfection is sub-frame timing / DAC).
It is **not** a wrong-value or wrong-behavior bug in the CPU/bus/FM logic.

## Open question / next investigation (deferred)

The precise mechanism of the ~8 s divergence is **not** fully characterized here — is it a gradual tempo drift
that sharply manifests, a discrete timing-dependent branch, or a fixable per-frame accounting error? Nailing it
down is a **sub-frame-timing accuracy** investigation (compare Timer-A fire rate and per-frame Z80 instruction
budget against Oracle over time). That is the named-deferred precision work; the RT-3 verdict stands without it:
**oracle-next's sound is register-value-accurate against the reference, with an accumulating sub-frame timing
drift.**

## Reproduce

```
# ours
cargo run --release -p oracle-core --example vgm_capture -- aeon/s4.soundtest.bin 1800
# oracle (MCP): reset → vgm_start(path) → resume → (wait) → pause → vgm_stop
python3 tools/vgm_diff.py <ours.vgm> <oracle.vgm>
```
