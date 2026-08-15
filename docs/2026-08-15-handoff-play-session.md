# Handoff — the play session, and what it cost us to learn (2026-08-15)

Written at a session boundary. The headline is not the code: it is that **twenty minutes of a human
playing a commercial game found two real bugs, both root-caused somewhere other than where the
instrumentation pointed**, after an arc of tooling work that had validated itself against itself.

| repo | tip | state |
|---|---|---|
| `oracle-next` | `b51dcb9` on `m68000-microop-framework` | pushed, clean, gated |
| `aeon` | replay gate merged and pushed to `origin/master` (via a peer session) | — |

Gates on the merged tree, run firsthand: `cargo test --workspace` **1247 passed / 0 failed / 28 legs**,
clippy zero warnings across default / `--no-default-features` / `--features synth`, `cargo fmt --all
--check` clean, and **`git diff 67ef06f HEAD -- crates/oracle-core/tests/` is a zero-line diff** — 17
commits touching a new crate, the VDP presentation path, the audio pipeline, the synth and a wire
field, without one pinned literal moving. Tests went 1158 → 1247, all additive.

---

## 1. What landed

**The replay runner (P3).** Aeon's input-replay regression net was fully built and completely dead —
its own ledger said *"it cannot detect a desync — that needs the emulator."* It is now an automated
gate: `crates/oracle-replay` (new crate, `replay_runner` bin) boots the DEBUG ROM, arms the embedded
`ARP0` stream by poking two work-RAM cells resolved **by name**, runs under a trap predicate, and
classifies PASS / DESYNC / FAULT / TIMEOUT / SHORT with exit codes. Wired into `aeon/test.sh` as
section 8; `./test.sh` runs 18/18.

It replaced a procedure whose final step was *reading `d0`/`d1`/`d2` off a screenshot of the MD
Debugger*. At `ErrorHandlerBlob+0` the `jsr` return address **is** the message pointer, so `(A7)`
yields the text and `(A7)-6` the raise site, with registers still pre-clobber. Break-on-fault fell out
for free, with no engine-side mailbox.

**The `cursor` wire-type fix.** Schema types it `string` in both places; we emitted a bare number.
Emit is strict, parse accepts either as a documented migration allowance.

**Per-line presentation** (`aba49a3`). See §2.

**Audio: three separate defects** (`3af2197`, `41bb1f1`, `af2c592`, `b51dcb9`). See §3.

---

## 2. The window was throwing away the correct pixels

S3K's underwater palette split was invisible: everything below the water line drew in the above-water
palette. The core was right the whole time — `system.rs:1040` already renders every scanline live and
**discards the RGB** unless a sink opts in. The window re-rendered all 224 lines *after*
`run_frames(1)` returned, which is in VBlank, after the game's DMA restores the above-water palette.
Measured at blit time: CRAM 64/64 identical to `Normal_palette`, 7/64 to `Water_palette`.

The H-INT machinery was never at fault — IACK at line 13 every frame, 57 CRAM writes across lines
14–32, matching `WaterTransition_AIZ2`'s 19 entries × 3 colours exactly. `F-CRAMDOT` and
`F-TRACE-VDPWRITE-MCLK` stayed out of scope.

The window now blits a `ScanlineCapture`. Side effect: **32% faster** (4.0 → 2.5 ms/frame), because
the old path rendered every line twice.

> **★ The open consequence.** `golden_frames.rs` and `conformance_roms.rs` **also sample post-hoc**, so
> the frozen currency is blind to the same class of effect — raster splits, per-line palette cycling,
> water lines. The already-recorded `color_1536` row (4 colours end-of-frame vs ~1400 per-scanline) has
> been measuring this all along without being recognised as a general blindness. Work is in flight to
> add per-scanline goldens **alongside** the existing ones (new coverage, zero currency movement),
> leaving "retire the post-hoc ones?" as a separate unhurried decision.

---

## 3. Audio: one real delivery bug, one real synthesis bug, and two bad yardsticks

The owner reported crackle/pops on the SEGA chant. Three findings, in the order they were believed:

**(a) A delivery rate deficit — real, fixed, and NOT the reported bug.** The frontend emitted 735
pairs per *emulated* frame while minifb's limiter can never exceed 60 fps (measured 59.63; it resets
`prev_time` *after* its sleep), so we produced 43,826/s against 44,100 consumed — a permanent 0.62%
deficit with the ring started empty and no feedback. The callback silence-filled 8–16% of the time.
Fixed by occupancy feedback making the audio device the master clock. **Then measured live: 1 short
callback in 45 s, at t=0.002 s, before anything renders.** The fix was sound; it was not what the owner
heard.

**(b) The actual cause: unfiltered zero-order-hold DAC reconstruction.** Proven by mechanism, not
inference — a ~10 dB spectral notch at 14.50–14.75 kHz, exactly the measured 14.6 kHz DAC write rate,
with symmetric images. The sample is 14,434 Hz PCM, so **everything above ~7.4 kHz is reconstruction
artifact by definition**. Real hardware low-passes its output; we shipped the raw staircase. Fixed by
routing the DAC through the native tick plus a revision-keyed console filter: artifact power above
7.4 kHz **2.776% → 0.072%**, A-weighted **−19.6 dB → −33.4 dB**. The owner listened and picked Model 1
VA0–VA2, now the player's default (`ORACLE_CONSOLE_FILTER=va0|va3|off` overrides).

**(c) A 9-bit channel-clip bug, found while chasing a level figure that was mostly fake.** See §4.

---

## 4. ★ Two confident numbers that were measured against invalid yardsticks

Both cost real time and both were mine. Recording them because the pattern will recur.

**"3.69× more high-frequency energy than the reference."** The `vgm2wav` reference **is itself an
unfiltered ZOH render** and shares our register stream. It was never a valid yardstick in the artifact
band — the comparison *understated* the true gap by ~2×. The correct frame was absolute, not relative:
above the source's Nyquist there should be nothing.

**"We are 10.2 dB quieter than the reference."** `vgm2wav` is libvgm, whose YM2612 core carries
`/* level unknown */` on both DAC scaling steps, never divides by 6 for the six-slot output
multiplexer, and is then scaled by a content-dependent `NormalizeOverallVolume` ×2. The 3.047× gap
decomposes exactly as 1.5 × 65/64 × 2, and its own render **hard-clips 90 samples at 0 dB headroom** on
this material. Honest figure: **+2 to +3.5 dB RMS**, not +10 dB.

Chasing the headline would have been actively harmful: per-source measurement showed DAC 3.28×, PSG
3.56×, one FM carrier 4.67× — but four carriers on one channel only 1.17×, **because we were missing
the OPN2's 9-bit per-channel accumulator**. A flat 3.24× would have matched the number and broken the
mix. What shipped is one shared reference level with the carrier sum saturated at the die-derived
±8192, pinned by tests so a future by-ear tweak cannot silently rebalance the three sources.

**Also mine, same family:** I ranked the DAC finding as "a garnish, ~1% of total power". Power ratio
cannot answer an audibility question. And I wrote a passband acceptance criterion — "0–5 kHz within
0.98–1.02×" — that **no correct filter could ever satisfy**, since a 3386 Hz low-pass is −3 dB *at*
3386 Hz by construction. The implementer replaced it with a real test: predict the filtered spectrum
from the filter's own |H(f)|² and compare to the render (1.0000 in every band 0–5 kHz).

---

## 5. What review caught that reproduction did not

The replay runner was reported working — both fixtures green, negative control tripping, every claim
reproduced firsthand by the overseer — **and it still had a false green in it.** `PASS` rested on the
single byte `Replay_Done == $FF` while three corroborations it already computed were discarded;
`header.tick_count` appeared only inside two `println!` calls. A truncated stream ending at tick 2
would have exited 0 having verified **1 of 27 checkpoints**, and the negative control provably could
not catch it (it corrupts checkpoint 0 — the one such a stream *does* compare). Confirmed on real
bytes: on a truncated ROM, `--negative-control` still reports PASSED.

Lesson worth keeping: **reproducing a green run proves the happy path and says nothing about what the
gate rejects.**

Also found by review: the `(A7)` decoder rejected every engine `assert` as "not ASCII" because sigil
embeds format-control bytes in the message — ~75 raise sites, the exact population the decoder exists
to serve — and under `--negative-control` reported them as "THE GATE IS INVERTED" when a trap had
fired correctly.

---

## 6. A pre-existing bug found in the sibling repo

**`aeon/test.sh` was already red on `master`.** `build.sh`'s first positional is the *game* since the
engine/game split, so `./build.sh -pe` resolved `GAME="-pe"` and died on `games/-pe/game_root.asm`
(reproduced independently). Section 7 therefore never built anything, and every ROM assertion under it
— size, header, vectors — was grading a **stale hand-built `s4.bin`**. One-line fix, folded in.

Also measured, against an assumption I had flagged: the DEBUG build is **1.4 s**, of which `ctags -R .`
is 0.37 s. I had guessed `ctags` over 20+ worktrees would dominate. It does not.

---

## 6b. Overnight (delegated, 2026-08-15)

**Per-scanline goldens** (`ada89dc`) — additive, as planned. `tests/scanline_goldens.rs` hashes the last
complete frame *as the VDP drew it* and compares against a post-hoc hash from the same run. Rows that
diverge pin a live hash; rows that don't **pin nothing** and assert the equality instead, so they stay
machine-checked without adding to anyone's amendment burden. **Zero deleted lines in the whole commit**;
all 25 `conformance_roms.rs` literals byte-identical; `golden_frames.rs` and `src/` untouched.

Three findings beyond the brief:

- **The blindness was broader than "raster effects".** 6 of 17 ROMs diverge, by *three* mechanisms where
  only one was anticipated. The broadest is **vblank updates that postdate the frame** —
  `shadow_highlight` makes **zero** active-display writes and 18 vblank writes per frame, so any ROM
  that merely does its VDP work in vblank was hashed wrong. No raster trick required.
  `window_distortion` is the tightest case: **one** R17 write at line 111 makes 112 lines wrong.
- **★ The conformance scorecard may be understating us.** `vdp_sprite_masking` test 6 reads **FAIL
  post-hoc and PASS live** — the 8 divergent lines land exactly on the rectangle the glyph classifier
  reads. And the ledger credits *both* of that ROM's failures to P1, which owns at most one.
- **Adjudicated with a Fable pass and deliberately left unfixed** as `F-POSTHOC-STALE-CARRY`. The
  harness is non-gating; the "one-line fix" hides an instrument redesign (the glyph constants are
  themselves pinned from post-hoc pixels). Both readings now sit side by side as an exhibit, with a
  warning on the row: **do not "fix" the emulator until the post-hoc render agrees** — that would mean
  breaking correct carry-seeding to satisfy a broken instrument.

`golden_frames.rs` got no live coverage and *cannot* — its scenes are static `Vdp` fixtures with no
machine to run, so per-scanline capture is not merely unimplemented there but meaningless.

**Player polish** (`683f67f`) — driven entirely by what real play exposed.

- **Sprite/backdrop picking**, the gap hit in the first minutes. A sprite click arms both the VRAM
  pattern that drew that dot *and* the sprite's SAT entry. 205 of 4480 sampled dots in one S3K frame are
  sprites. The test refuses to restate the addressing arithmetic: it checks, for every dot of 7 sprite
  sizes × 4 flips, that the tile it names is the tile the **core's own renderer** drew from.
- **On-screen feedback** — volume, mute, pause, slot, filter revision, save/load all previously went
  only to stdout, which a windowed user never sees. Self-contained 5×7 font, no new dependency;
  `println!` output byte-unchanged; toasts de-duplicate and expire on the *presented* frame clock.
  `PAUSED` is load-bearing now that paused frames re-present a retained buffer.
- **Resizable window, correct aspect** (4:3 default — a real console puts H32's 256 dots and H40's 320
  across the same TV width). **A latent bug fixed in passing:** the old `window_to_native` was correct
  only by coincidence, agreeing with minifb's stretch at exactly one window size, and would have
  mis-mapped every click the moment the window could be dragged.
- **Fullscreen deliberately skipped**, reason recorded: minifb 0.28 has no runtime fullscreen call, no
  `set_size`, no screen-dimension query; the only route is recreating the `Window` mid-loop.
  `resize: true` delegates to the window manager.

## 7. Next

1. **Per-scanline goldens** (in flight) — additive, zero currency movement.
2. **Player polish** (in flight) — starting with click-to-watch on sprites, which the owner hit
   immediately; plus fullscreen, scaling, aspect, on-screen feedback (volume/mute/pause/slot state all
   currently go to stdout, which a windowed user never sees).
3. **Owner-owed, longest outstanding: nobody has plugged in a gamepad.** Deadzone 0.5 is an unfelt
   guess. Now joined by: nobody has heard the new mix levels.
4. Deferred with reasons, not forgotten: the runner hardcodes sonic4's scene entry (`F8`); its
   real-artifact tests skip green when Aeon isn't built (`F6`); `--restamp` (collapses Aeon's candidate
   fix (b) into a flag on the runner); the shared pad-timeline type retiring six re-implementations;
   9-bit channel *quantization* and per-carrier saturation.

## 8. Cross-repo asks, unraised

- **Checkpoint `id` has the cursor's divergence one layer deeper** — schema says `string` (line 290),
  we emit numbers, and there the schema contradicts D9's own prose that slot indices are JSON numbers.
  Needs a contract ruling, not a unilateral server change.
- **Nothing mechanically checks our replies against `bus-protocol.schema.json`** — which is precisely
  why a two-place type mismatch survived in a well-tested method.
- **Aeon's runbook carries stale addresses** and a symbol-grep recipe (`^Name `) that matches zero
  lines against sigil's real ` Name : ADDR C |` format — plausibly how the stale table survived.
- **`raise_exception` is used at non-vector sites** in `replay.emp`, so the crash screen's
  SR/Offset/Caller fields are decoded from a malformed frame. Register dump and `(A7)` are unaffected.

## 9. Ops notes

- **Agent worktrees are cut from the session-start commit, not current `HEAD`.** This bit three times:
  once silently omitting a design doc, once handing an agent a tree with no `oracle-replay` crate, once
  more caught by an explicit base check. Every agent brief now opens with "verify your base".
- **Never dispatch a file-touching agent without worktree isolation.** One was, and it edited the
  shared tree mid-gate — including `oracle-core` — turning a gate red for reasons that had nothing to
  do with the tree under test.
- `cargo test | tail` hides failures *and* returns `tail`'s exit code. Made this mistake once at the
  session's first gate.
- `pkill -f "release/oracle-frontend"` matches its own shell's command line and kills itself (exit
  144). Kill by PID.
- Worktrees pruned 9.0 GB → 6.3 MB; seven merged branches deleted with `-d`, never `-D`. `vendor`
  verified intact afterwards (17 TestRoms entries) — losing it silently *skips* conformance rows.
