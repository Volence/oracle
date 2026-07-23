# Phase SY-4 — Sub-frame Timing: Design & Recon (2026-07-23)

**Status:** planning / recon pass only. **No core-logic edits, no commits.** Docs-only, mirroring the
prior recon commits (`git show 9437d25` FM-timer design, `git show 5a0949d` RT design). Every factual
claim below cites the `file:line` it was read from. Build state at HEAD unchanged.

This doc closes out the single deferred item named in the SY synthesis design — "The ONE real data
gap: sub-frame timing" (`docs/2026-07-23-phase-sy-synthesis-design.md:55-75`) and the Slice SY-4 stub
(`…:229-234`) — and the SY-3 Fork-2 sequencing note that "SY-4 (mclk timestamps on `BusEvent`) stays a
separate later refinement — it touches the core seam and every sink"
(`docs/2026-07-23-phase-sy3-fm-accuracy-plan.md:37-46`).

---

## 0. TL;DR (for the impatient implementer)

- **Seam: Option B.** Add `fn on_event_at(&mut self, event: BusEvent, mclk: u64)` to `BusEventSink`
  with a **default forwarder** `{ self.on_event(event) }`. Leave `BusEvent` **unchanged**. Route only
  the two *real* chip-write emission sites (`MegaDriveBus::emit`, `Z80Bus::write`) through it, passing
  `self.now_mclk`. Every existing sink keeps compiling and behaving identically for free. Zero churn to
  `BusEvent`'s `PartialEq`/`Eq` and the ~40 test constructors that would break under Option A.
- **mclk source:** already in scope as `self.now_mclk` at *both* real emission sites — `bus.rs:375`
  (68k: `scheduler.now()` at step start, `system.rs:553,573`) and `z80/bus.rs:159` (Z80:
  `z80_frontier_mclk` at step start, `system.rs:606-611`). No new plumbing to reach the clock.
- **Intra-frame sample index:** `sample = (mclk % MCLK_PER_FRAME) * samples_per_frame / MCLK_PER_FRAME`,
  frame `= mclk / MCLK_PER_FRAME`. Pure integer math from the write's own absolute mclk; no reliance on
  `frame_boundary_mclk` state.
- **DAC:** exact stamps *refine* the SY-3a even-spread (they replace the synthetic even index with the
  write's true sample index); the ZOH, `$2B` gating, and per-frame snapshot survive.
- **Currency: SAFE, no owner gate.** `BusEvent` is not in `state_hash`/`export_state` (grep-proven). A
  new trait method with a default forwarder cannot move default-build bytes.
- **Split into two commits: SY-4a seam plumbing, SY-4b synth consumption.**

---

## 1. Where the mclk comes from at every emission site

Grep of every `.on_event(` call for a *real machine access* (test/helper constructors excluded):

| Site | `file:line` | Master | mclk in scope? | Value |
|---|---|---|---|---|
| `MegaDriveBus::emit` (68k, all real 68k accesses incl. FM `$A04000`, PSG `$C00011`) | `bus.rs:374-382` | 68000 | **Yes** — `self.now_mclk` | `scheduler.now()` at CPU-step start |
| `Z80Bus::write` (Z80 FM `$4000-$4003` + PSG `$7F11` tap) | `z80/bus.rs:158-165` | Z80 | **Yes** — `self.now_mclk` | `z80_frontier_mclk` at Z80-step start |
| `SystemBus::read`/`write` (Phase-0 **synthetic** RAM/VRAM stub bus) | `bus.rs:166,177` | — | **No** clock field | not a sound path — leave as-is |

### 1.1 The 68k side — `self.now_mclk`

`MegaDriveBus` already owns `now_mclk: u64` (`bus.rs:235`), documented as "the master-clock reading …
so the timing FSM" (`bus.rs:227-235`). It is fed from `step_cpu`:

```
// system.rs:552-580
let now = self.scheduler.now();            // :553
let mut bus = MegaDriveBus::new(rom, ram, z80_ram, vdp, io, now, …, fm, sink);  // :567-579
cpu.step(&mut bus)                          // :580
```

So `self.now_mclk` at `bus.rs:375` is the scheduler mclk **at the start of the instruction** that drove
the write. It is already consumed for VDP/FM timing (`bus.rs:306,350,397,451`). **The 68k answer to the
task question is: it is `scheduler.now()` / the CPU's current mclk, captured once per instruction into
`MegaDriveBus.now_mclk`.** This is instruction-boundary granular (not sub-instruction), which is the
correct and only granularity the run loop exposes anyway (see §5.1 overshoot).

### 1.2 The Z80 side — `self.now_mclk` = the frontier

`Z80Bus` owns `now_mclk: u64` (`z80/bus.rs:49`), documented "the Z80's current mclk (its frontier — the
value at the start of this step)" (`z80/bus.rs:46-49`). It is fed from `catch_up_z80`:

```
// system.rs:606-613
while *z80_frontier_mclk < now {
    let mut bus = Z80Bus::new(z80_ram, rom, ram, z80_bank, fm, *z80_frontier_mclk, sink);  // :610-611
    let t = z80.step(&mut bus);
    *z80_frontier_mclk += t as u64 * MCLK_PER_Z80_CYCLE;   // :613
}
```

So `self.now_mclk` at `z80/bus.rs:159` is `z80_frontier_mclk` **at the start of that Z80 instruction**.
It is already used for FM timer reads/writes (`z80/bus.rs:129,167`). **The Z80 answer is: it is
`z80_frontier_mclk`, i.e. the Z80's own frontier — which lags the 68000's `now` but is absolute on the
one shared timeline** (`system.rs:606-609` comment; `docs/2026-07-22-fm-timer-design.md`). This is the
line that actually matters for SY-4, because the SMPS driver's FM/PSG register stream is emitted almost
entirely by the **Z80** — the sound driver runs on the Z80 (RT-1, `7fdcfda`).

### 1.3 No site needs new plumbing

Both real emission sites already hold the correct absolute mclk in a struct field (`self.now_mclk`).
The change at each site is purely `self.sink.on_event(ev)` → `self.sink.on_event_at(ev, self.now_mclk)`.
The synthetic `SystemBus` (`bus.rs:101-190`) has no clock and is not a sound path (Phase-0 VRAM stub,
`bus.rs:18-20`); it keeps calling the untimed `on_event` and inherits mclk `= 0`-equivalent framing,
which no sound sink ever sees.

---

## 2. The seam change — options and recommendation

### 2.1 The three options, weighed

**Option A — add `mclk: u64` field to `BusEvent`** (`bus.rs:42-49`).

- *Call sites that must change:* every `BusEvent { … }` literal in the tree. Real: `bus.rs:166,177,375`,
  `z80/bus.rs:159`. **Plus every test/helper constructor**, because the struct-literal becomes
  ill-formed without the field: `vgm.rs`'s `write_event` helper + its ~14 call sites
  (`vgm.rs:264-397`), `watchpoints.rs`'s ~20 constructors (`watchpoints.rs:317-509`), `audio_sink.rs`'s
  test `write_event` (`audio_sink.rs:148-156`), `system.rs` tests. ~40 sites.
- *Every existing sink must be edited?* Not the sinks' method bodies, but **every literal**, and worse:
  `BusEvent` derives `PartialEq, Eq` (`bus.rs:42`). `Vec<BusEvent>` is the recording sink
  (`bus.rs:84-88`) and tests compare recorded events for equality. Adding `mclk` silently makes those
  comparisons *timestamp-sensitive*, so recording-sink assertions that were value/addr equalities now
  also pin mclk — a broad, brittle semantic change to the currency-adjacent recording path.
- *Default-build byte-invariance:* the field is unhashed/unserialized (see §4), so bytes don't move —
  but the blast radius and the `Eq` semantics shift make this the **riskiest** option to review.
- *Readability:* every consumer's pattern-match / literal now carries a field it usually ignores.

**Option B — add `fn on_event_at(&mut self, event, mclk)` with a defaulted forwarder.** *(recommended)*

```rust
pub trait BusEventSink {
    fn on_event(&mut self, event: BusEvent);

    /// Timestamped delivery: the same event plus the absolute master-clock (mclk) of the access.
    /// Emission sites that hold the current mclk (the real 68k/Z80 buses) call THIS; the default
    /// forwards to `on_event`, so every existing sink is behaviorally unchanged and needs no edit.
    /// Only a timing-aware sink (the synth AudioSink) overrides it.
    fn on_event_at(&mut self, event: BusEvent, _mclk: u64) {
        self.on_event(event);
    }

    fn on_step_boundary(&mut self, _pc: u32, _frame: u64) {}   // unchanged (bus.rs:61)
    fn wants_vdp_writes(&self) -> bool { false }               // unchanged
    fn on_vdp_write(&mut self, _write: crate::vdp::VdpWrite) {} // unchanged
}
```

- *Call sites that change:* exactly the **two real emission sites** (`bus.rs:375`, `z80/bus.rs:159`)
  switch `on_event(ev)` → `on_event_at(ev, self.now_mclk)`. Nothing else.
- *Every existing sink must be edited?* **No.** `()` (`bus.rs:79-81`), `Vec<BusEvent>` (`bus.rs:84-88`),
  `Watchpoints`, `VgmLogger` (`vgm.rs:194-…`) all keep their `on_event` and ride the default forwarder
  unchanged. `BusEvent` and its `Eq` are untouched, so no recording-sink assertion shifts.
- *Default-build byte-invariance:* a trait gains a defaulted method — the `()` sink monomorphizes to the
  same no-op, `state_hash`/`export_state` never observe the trait. Byte-identical by construction.
- *Readability:* the timestamp lives in the one place that needs it; the 99% of consumers that don't
  care never see it. This mirrors the existing precedent — `on_step_boundary` (`bus.rs:55-61`) and
  `on_vdp_write`/`wants_vdp_writes` (`bus.rs:63-75`) are *exactly this pattern*: opt-in, defaulted trait
  methods that keep the null/recording hot paths untouched.

**Option C — stamp mclk out-of-band via an `on_step_boundary`-style call** (e.g. `on_mclk(mclk)` fired
before each event, sink latches it).

- Adds a second call per bus event (doubles sink-call traffic on the hot path if not gated), and
  *decouples* the timestamp from the event it belongs to — an interleaving hazard if the run loop ever
  reorders. The stamp is not atomically bound to its event. **Reject.**

### 2.2 Recommendation: **Option B**

Option B is strictly dominant: smallest diff (2 sites), zero edits to existing sinks, no `BusEvent`/`Eq`
churn, provably neutral, and it is the house pattern already used three times in the same trait. The
mclk is bound atomically to its event as a call argument.

**Exact signatures to add (SY-4a):**

```rust
// bus.rs, in `trait BusEventSink`
fn on_event_at(&mut self, event: BusEvent, _mclk: u64) { self.on_event(event); }

// bus.rs:374-382 — MegaDriveBus::emit
fn emit(&mut self, op: BusOp, fc: u8, addr: u32, size: Size, value: u32) {
    self.sink.on_event_at(BusEvent { op, fc, addr, size, value }, self.now_mclk);
}

// z80/bus.rs:158-165 — Z80Bus::write, the FM/PSG tap arm
self.sink.on_event_at(
    BusEvent { op: BusOp::Write, fc: 0, addr: addr as u32, size: Size::Byte, value: value as u32 },
    self.now_mclk,
);
```

(The `SystemBus` sites at `bus.rs:166,177` stay on `on_event` — no clock, no sound.)

---

## 3. Turning an absolute mclk into an intra-frame sample index

### 3.1 The constants

- `MCLK_PER_FRAME = 896_040` (`system.rs:24`) — mclk per NTSC frame.
- `samples_per_frame = sample_rate / 60` (`synth/audio_sink.rs:27,47`); at 44_100 Hz this is **735**
  (asserted `audio_sink.rs:163`), matching the VGM `0x62` frame-wait (`vgm.rs:56`).
- Frame index the sink already sees: `scheduler.now() / MCLK_PER_FRAME` (`system.rs:478`).

### 3.2 The formula

For a write delivered at absolute `mclk`:

```
frame       = mclk / MCLK_PER_FRAME           // integer division — matches on_step_boundary (system.rs:478)
frame_start = frame * MCLK_PER_FRAME
offset      = mclk - frame_start              // == mclk % MCLK_PER_FRAME,  in [0, MCLK_PER_FRAME)
sample_idx  = (offset * samples_per_frame as u64 / MCLK_PER_FRAME) as u32   // in [0, samples_per_frame)
```

This is **self-contained in the sink** from the write's own absolute mclk — it does *not* need
`System::frame_boundary_mclk` (`system.rs:95`), avoiding any coupling to run-loop state. It is
consistent with the frame index the sink already latches via `on_step_boundary` (`system.rs:478`), so a
write's derived `frame` and the boundary-stamped `frame` agree by construction.

*Sanity:* `896_040 / 735 ≈ 1219.1` mclk per output sample. `offset = 0` → sample 0; `offset` just under
`MCLK_PER_FRAME` → sample 734. Integer division truncates toward 0; the non-integer ratio is absorbed
(no sample is skipped or doubled within a frame beyond ≤1-sample rounding).

### 3.3 Edge cases

- **Write exactly at the boundary** (`offset == 0`): `sample_idx == 0` → the first sample of *that*
  frame. Correct — a write stamped `frame = f` belongs to frame `f`'s sample 0, never to `f-1`'s tail.
- **Clamp:** `sample_idx.min(samples_per_frame - 1)` as a belt-and-suspenders guard against a boundary
  rounding artifact (mirrors the existing DAC clamp `…min(n - 1)` at `ym2612_synth.rs:944`).
- **CPU overshoot / Z80-past-boundary** (the important one): the 68k steps whole instructions, so `now`
  can land a little past `f * MCLK_PER_FRAME`; the Z80 frontier is caught up *after* the 68k step
  (`system.rs:606-613`) and can cross into frame `f+1` before the next 68k `on_step_boundary` stamps
  `f+1`. Such a Z80 write carries `mclk / MCLK_PER_FRAME == f+1`. **The sink must bucket each write by
  its own derived `frame`, not assume every write belongs to the frame currently being rendered.** When
  `render_frame` renders frame `f`, it consumes only bucket-`f` writes; a bucket-`f+1` write stays
  queued for the next render. In the SY-3 frame-batched model this reordering was invisible (all writes
  applied to current chip state before render); in the sub-frame model the per-frame bucket is what
  makes overshoot writes land in the right frame. See §5.1 for the concrete data-structure.

---

## 4. DAC / PCM (`$2A`) — refine, don't replace, the SY-3a even-spread

Today (SY-3a, Fork-2A): each frame's ordered `$2A` bytes are captured into `dac_queue`
(`ym2612_synth.rs:791-794`), snapshotted at the frame boundary into `dac_frame` by `begin_frame`
(`ym2612_synth.rs:927-932`), and output sample `i` plays `dac_frame[i * n / spf]` — a synthetic
**even** spread with zero-order hold (`ym2612_synth.rs:938-946`), gated on `$2B` bit7
(`ym2612_synth.rs:796,702`), with FM ch6 muted while enabled.

**What SY-4 changes:** the synthetic index `i * n / spf` is replaced by each `$2A` byte's **true**
intra-frame `sample_idx` (§3.2). Concretely, instead of pushing bare bytes to `dac_queue`, the sink
records `(sample_idx, byte)` pairs; `begin_frame` builds a length-`spf` ZOH track by writing each byte
at its real sample and holding it forward to the next byte's sample (still ZOH, still `dac_last` for a
gap-free-frame, still `$2B`-gated). It **refines**, not replaces: the ZOH character, the `$2B` gate, the
ch6 mute, the per-frame snapshot, and `DAC_SCALE` (`ym2612_synth.rs:211`) all survive unchanged.

**Effect on the SY-3 named failure modes** (`docs/2026-07-23-phase-sy3-fm-accuracy-plan.md:42-45`):

1. **Onset quantization up to ~16.7 ms → fixed.** A drum hit's first `$2A` now lands at its true sample
   (≈22 µs resolution at 44.1 kHz) instead of frame-start. The flam-threshold argument (was "under
   ~30 ms") becomes moot.
2. **Per-frame resample-rate wobble on long PCM → fixed.** This was flagged as "exactly what SY-4
   fixes." Bytes are placed at their real sub-frame times, so a stream that spans many frames plays at
   its true instantaneous rate instead of being re-flattened to `n/spf` each frame.
3. **Occasional frame-boundary ZOH click → reduced/eliminated.** With true placement the sample held
   across a boundary is the genuine last byte at its genuine time; the artificial "last synthetic index
   vs first synthetic index of next frame" seam disappears. (If any residual click remains, the SY-3
   escape hatch — a 1-sample ramp — still applies.)

The DAC path stays at output rate, added after the FM resample (as in SY-3a,
`ym2612_synth.rs:27,58-64`); only the index source changes.

---

## 5. Synth-side consumption (SY-4b)

### 5.1 AudioSink override + per-frame bucketing

`AudioSink` currently classifies writes in `on_event` (`audio_sink.rs:123-140`) and renders on frame
advance in `on_step_boundary` (`audio_sink.rs:107-121`). SY-4b adds:

```rust
impl BusEventSink for AudioSink {
    // NEW: the timestamped path the real buses now call. Derive the write's frame + intra-frame
    // sample, and route into a per-frame bucket instead of applying immediately.
    fn on_event_at(&mut self, e: BusEvent, mclk: u64) {
        if e.op != BusOp::Write { return; }
        let frame  = mclk / MCLK_PER_FRAME;
        let sample = ((mclk % MCLK_PER_FRAME) * self.samples_per_frame as u64 / MCLK_PER_FRAME)
                        .min(self.samples_per_frame as u64 - 1) as u32;
        self.enqueue(frame, sample, e);      // bucket by the write's OWN frame (overshoot-safe, §3.3)
    }

    // KEEP (required method): untimed fallback for direct test calls — apply at sample 0 of the
    // current frame (behaviorally the SY-3 frame-batched semantics). Real runs never hit this.
    fn on_event(&mut self, e: BusEvent) { self.enqueue(self.cur_frame, 0, e); }

    fn on_step_boundary(&mut self, _pc: u32, frame: u64) { /* render buckets < frame (as today) */ }
}
```

`render_frame` (`audio_sink.rs:79-93`) changes from "apply-then-render" to "walk this frame's bucket in
`sample` order, applying each write's register effect as the per-sample loop reaches its `sample`, then
render 735 samples." The FM/PSG register-apply calls (`fm.write`, `psg.write`,
`audio_sink.rs:131-137`) are unchanged — only *when* within the frame they fire moves from "all at
frame start" to "at the write's sample." The DAC pairs (§4) feed `begin_frame`'s new
`(sample_idx, byte)` track.

**Data structure:** a small `Vec<(u32 sample, BusEvent)>` per open frame (or a `BTreeMap<u64 frame, …>`
keyed by frame to hold overshoot writes for a not-yet-rendered frame). Writes arrive in mostly-sorted
sample order within a frame (monotone `now`), so a stable sort or an insertion into a
mostly-sorted vec is cheap.

### 5.2 What must NOT change

- `VgmLogger` (`vgm.rs`) keeps consuming `on_event` via the default forwarder — its RT-3 A/B VGM output
  is byte-unchanged (it uses the frame stamp from `on_step_boundary`, `vgm.rs:195-197`, not mclk). SY-4
  must not touch `vgm.rs`.
- The default (non-synth) build never constructs an `AudioSink` (feature-gated `synth`,
  `docs/2026-07-23-phase-sy-synthesis-design.md:104-106`); `on_event_at`'s default forwarder is the only
  thing compiled into `()`.

---

## 6. Currency-neutrality proof

**Claim:** SY-4 cannot move a single default-build byte.

*Evidence 1 — `BusEvent` is in no currency.* `state_hash` hashes only VDP regions
(`system.rs:291-298`: `vram/cram/vsram/regs`). `export_state` serializes version → m68k regs → work RAM
→ Z80 RAM → Z80 regs → VDP → **fixed all-zero FM/PSG placeholders** (`system.rs:310-364`, esp.
`:360-362`). Grep confirms **zero** occurrences of `BusEvent` in either function's region list; `BusEvent`
appears only in `bus.rs`, `z80/bus.rs`, `vgm.rs`, `watchpoints.rs`, `audio_sink.rs` — all
instrumentation/consumer code, never in `System`'s serialized state.

*Evidence 2 — Option B adds no field to any serialized type.* Unlike Option A, `BusEvent`'s layout is
untouched; the only change is a new **defaulted trait method**. The null sink `()` (`bus.rs:79-81`)
monomorphizes `on_event_at` to the same inlined no-op it does today; the run loop's null path
(`run_frames` → `&mut ()`, `system.rs:435`) is byte-identical.

*Evidence 3 — mclk is additive and unhashed.* The mclk passed to `on_event_at` is a call argument, not
stored in any `System` field newly hashed/serialized. It is read from the *already-existing*
`self.now_mclk` fields (`bus.rs:235`, `z80/bus.rs:49`) that already drive VDP/FM timing — no new state.

**Verdict: SAFE, no owner gate** (same class as the SY-1/2/3 sink-seam neutrality, and consistent with
the FM-timer design's currency verdict, `git show 9437d25`).

---

## 7. Test plan

All under `--features synth` unless noted; default build must stay green + byte-identical throughout.

1. **mclk → sample conversion (unit, pure fn).** Table-drive `sample_for(mclk)`:
   `mclk = 0 → (frame 0, sample 0)`; `mclk = MCLK_PER_FRAME → (frame 1, sample 0)`;
   `mclk = MCLK_PER_FRAME - 1 → (frame 0, sample 734)`; a mid-frame value → the hand-computed
   `offset * 735 / 896_040`. Pin the clamp at the top boundary.
2. **Frame bucketing / overshoot.** Feed `on_event_at` with a Z80-style write whose `mclk` is a few
   mclk *past* `f * MCLK_PER_FRAME` while the sink's current render frame is `f-?`; assert it renders in
   frame `f`, not the frame being flushed (§3.3). This is the guard against the Z80-past-boundary hazard.
3. **Monotonic non-decreasing within a frame.** Feed a sequence of writes with non-decreasing `mclk`
   inside one frame; assert the derived `sample_idx` sequence is non-decreasing (no write is placed
   earlier than a preceding one in the same frame).
4. **Default-forwarder equivalence.** A sink that only implements `on_event` (e.g. a test `Vec` or a
   VgmLogger-shaped stub) receives identical events whether the site calls `on_event` or `on_event_at`
   — pins the forwarder.
5. **Currency gate (default build).** `cargo test` default features green; `export_state` /
   `state_hash` goldens unchanged (they cannot move — §6, but assert it).
6. **A/B render regression + DAC-onset tightening.** Using **`examples/synth_render.rs`** (the real WAV
   render example, confirmed present at `crates/oracle-core/examples/synth_render.rs`) on
   `s4.soundtest.bin`:
   `cargo run --release -p oracle-core --features synth --example synth_render -- s4.soundtest.bin 600`.
   Compare sub-frame-ON vs the SY-3 render and vs `vgm2wav` of the same captured VGM (from
   `examples/vgm_capture.rs`):
   - **Spectral corr must not regress beyond the −0.005 gate** (`…sy3-fm-accuracy-plan.md:52`).
   - **DAC onset timing measurably tighter:** cross-correlate the drum-onset envelope (SY-3a
     even-spread) vs the reference and vs SY-4; the SY-4 lag-to-reference should shrink (target: onset
     error < one frame, ≤ a few ms, vs up to 16.7 ms). Reuse the SY-3c envelope-RMS-correlation gate
     (~5 ms windows, `…sy3-fm-accuracy-plan.md:52`), which is sensitive to exactly this timing.
   - `examples/drumtest_probe.rs` (present in the tree) is the natural DAC-specific probe fixture.

---

## 8. Slice / commit plan — **split into two**

| # | Slice | Scope | `file:line` touched | Success check |
|---|---|---|---|---|
| **SY-4a** | **Seam plumbing** | Add `on_event_at` + default forwarder to `BusEventSink`; route the two real emission sites through it with `self.now_mclk`. **No sink overrides it yet.** | `bus.rs` (trait ~`:52-76`; `emit` `:374-382`); `z80/bus.rs:158-165` | Default build byte-identical; ALL existing tests green with zero edits to `()`/`Vec`/Watchpoints/VgmLogger; new forwarder-equivalence test (Test 4) passes. |
| **SY-4b** | **Synth consumption** | `AudioSink` overrides `on_event_at`; per-frame `(sample, event)` bucketing; `render_frame` applies writes at their sample; DAC `begin_frame` takes `(sample_idx, byte)` pairs. | `synth/audio_sink.rs:107-140,79-93`; `synth/ym2612_synth.rs:791-794,927-946` | Tests 1-3,6 pass; spectral corr within −0.005; DAC onset tighter; `--features synth` green. |

**Why split:** SY-4a is a *currency-adjacent core-seam change* (trait + real emission sites) whose whole
value is being reviewable in isolation as "provably byte-neutral, no behavior change" — exactly the
review burden SY-3 Fork-2 warned must "not be coupled to the synth milestone"
(`…sy3-fm-accuracy-plan.md:44-46`). SY-4b is a *synth-only* change behind the `synth` feature that can
land, be A/B-measured, and be reverted independently. If a session is cut short after SY-4a, the seam is
banked and neutral with zero consumers; nothing regresses. This mirrors the RT/SY house style of
biggest-risk-isolated-first.

---

## 9. Risks & open questions (owner ratification)

1. **[FLAG — sequencing already pre-ratified] SY-4 pull-forward trigger.** SY-3 Fork-2 flag 3
   (`…sy3-fm-accuracy-plan.md:73-75`) said if SY-3a's frame-granular DAC "produces audible warble on any
   *long* PCM sample," the fix is pulling SY-4 forward. This doc **is** that pull-forward, scoped and
   neutral. Owner ratifies that SY-4 lands now (vs after all SY-3 feature slices) — the recommendation
   is yes, because it is decoupled (SY-4a touches no synth, SY-4b touches no FM core) and unblocks
   failure-mode 2 (long-PCM wobble) directly.
2. **[LOW] Instruction-granular 68k stamps.** 68k writes carry the *instruction-start* mclk
   (`bus.rs:375` reads `self.now_mclk`, fixed at step construction, `system.rs:553,573`), so two 68k
   writes in one instruction share a sample. This is inherent to the run loop and harmless for sound
   (the SMPS register stream is Z80-driven; the Z80 side is instruction-granular at ~Z80-cycle
   resolution, far finer than 735 samples/frame). Flagged only so it's not mistaken for a bug. No action.
3. **[LOW] Bucket memory / ordering.** Per-frame bucket holds one frame's writes (small — a frame of
   SMPS driver register traffic is tens-to-low-hundreds of writes). A `BTreeMap<frame, …>` bounds
   overshoot to ≤1-2 open frames. No unbounded growth. Confirm the chosen structure keeps `render_frame`
   O(writes + samples).
4. **[LOW] Frame-index agreement.** The sink derives `frame = mclk / MCLK_PER_FRAME`; the run loop
   stamps the same expression at `system.rs:478`. If a future change alters one, they must stay in lock
   step — worth a comment tying them together. (No divergence today.)
5. **[NONE gating] `SystemBus` synthetic path** stays on untimed `on_event`. It is a Phase-0 VRAM stub
   (`bus.rs:18-20`), never a sound source; leaving it untimed is correct, not a gap.

---

## 10. Cross-references

- Scope origin: `docs/2026-07-23-phase-sy-synthesis-design.md:55-75` (the data gap), `:229-234` (SY-4
  stub).
- Fork-2 DAC decision + failure modes SY-4 fixes: `docs/2026-07-23-phase-sy3-fm-accuracy-plan.md:37-46`.
- Seam precedent (opt-in defaulted trait methods): `bus.rs:55-75` (`on_step_boundary`,
  `wants_vdp_writes`, `on_vdp_write`).
- mclk sources: `bus.rs:227-235,374-382` (68k), `z80/bus.rs:46-49,158-167` (Z80),
  `system.rs:552-580` (68k feed), `system.rs:593-618` (Z80 feed).
- FM-timer currency-verdict precedent: `git show 9437d25`.
