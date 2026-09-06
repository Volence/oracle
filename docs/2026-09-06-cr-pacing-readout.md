# CR-R: `emulator/pacing` — let a program read what the window already measures

**Filed by:** oracle, 2026-09-06, at the hub's instruction after the owner reported lag this lane
could not put a number on.

**Status: ADOPTED WITH CHANGES and SERVED**, 2026-09-06. Registered as **§11.42 (CR-S)** at empyrean
`3f83c6c` — *not* CR-R, which is §11.41's label; the hub renamed it on adoption. Served on
`oracle-player` only, with the fragment and the ten vectors re-vendored in the same commit.

> **What the ruling CHANGED from the shape proposed below, so this file is not read as the built
> thing.** The result's fifth field is **`targetFps`** — an integer, `0` meaning the governor is off
> — and not the `governor` boolean this document proposed; §11.42 M1 wanted the target itself, so a
> reader can tell a paced 60 from a free-running 60 rather than only that something was pacing.
> `fps` became an object `{value, windowMs}` where this file said only that the window must travel
> with the figure (M2), and the window is the SERVER's to report, so no client param selects it. The
> conditional halves of M3 are enforced by `if`/`then` **in the fragment**, which is why vector 4 of
> the five proposed here — *"a headless server advertising `emulator/pacing` in `initialize`"* — is
> **not** a vector: which binary is answering is not a property of a document, and the ruling made
> that negative a suite obligation instead (`crates/oracle-aether/tests/pacing.rs`).
>
> **And one claim in this document was wrong, found while building it.** *"The window measures all
> three"* is true of the underrun count and near enough of the presented tally; it is **false of the
> frame rate and the frame-time percentiles**, which nothing on the live path computed —
> `crate::stats::Series` computes percentiles only for the `--bench-arm` report. Worse, the tab's
> "pictures drawn" was `Machine::pictures()`, whose own doc records it running **above** the
> presented count. So the derivation was built rather than exposed, and the Pacing tab now reads it
> instead of the quantity it had. `F-VSYNC-NEVER-MEASURED` stays open, exactly as this file says it
> must: the instrument exists, and nobody has read a figure off the owner's live window yet.

## The problem, and it arrived as a complaint rather than as a booking

The owner said his window *"lags super hard"*. This lane could describe the likely cause from source
and measure his process from outside (97 % of one core, main thread), but **could not state his frame
rate, his frame time, or whether audio was underrunning** — because nothing on the bus reports them.

The window measures all three. It has to: the Pacing tab draws them. `crates/oracle-player/src/device.rs`
even distinguishes *"Pacing is UNMEASURED, not measured-and-fine"* from a real zero. **Every one of those
numbers dies inside the process that computes them.**

**Measured absence, with a control, because an absence claimed without one is worth nothing.** The
served set is 58 methods (derived from `engine.rs:290 METHODS`, not typed). Control: `emulator/status`,
`emulator/registers`, `emulator/screenshot`, `emulator/read_memory` all present, so the enumeration
sees what is there. Of those 58, **none** names pacing, fps, frame time, presentation, vsync, underruns
or draws. `emulator/status` carries `frameToken`, which is the **emulated** frame position — a
different quantity from frames put on the glass, and not a substitute.

*(The first run of this check produced "NONE" from an extraction that had found zero methods. The
control is what caught it. Recorded because this repo has now been bitten repeatedly by a decorated
tool whose failure is indistinguishable from its empty result.)*

## Why this is worth a contract change rather than a local readout

`F-VSYNC-NEVER-MEASURED` is booked here as the one empty cell under the migration's retirement gate:
*"60 fps and audio pacing measured on the real player under the toolkit, in the same form as the spike
doc"*. That cell is empty because measuring it currently requires **the owner's own display and a fresh
authorisation from him** — a second window is barred while his is live, and a headless framebuffer has
no vsync, so it answers a different question while looking like an answer.

**A read-only method removes that requirement entirely.** The owner keeps using his window; a client
samples it. The gate stops needing his attention to close.

## Proposed shape: ONE method, not an event

`emulator/pacing`, read-only, no params.

An event was considered and rejected: pacing is a continuous quantity, an event stream would push
per-frame data nobody asked for, and `Host::pump` drains once per frame, so a method costs **one drain
per call at the caller's own rate** rather than a permanent per-frame tax on a window the owner is using.

### Result

| field | meaning |
|---|---|
| `presented` | frames actually put on the glass since start — **the window's `DRAWS n`**, stated as that quantity by name |
| `fps` | presented frames per second, **each figure carrying the window it was measured over** |
| `frameTimeMs` | `{p50, p99, samples}` — the percentile pair the spike doc's form already uses, plus the sample count behind it |
| `audio` | `{underruns, unmeasured}` |
| `governor` | whether the frame governor is off (`--target-fps 0`), since that changes what `fps` means |

### Three properties that are requirements, not polish

1. **Every number names the quantity it is, and `presented` is not `frameToken`.** The window already
   ships three different frame tallies and they are not interchangeable. *An index whose space is
   unstated is a transpose bug waiting to happen* (aurora's rule, earned on tile indices; it applies
   unchanged to frame counters).
2. **`unmeasured` must be expressible, and MUST NOT be reported as zero.** No audio device and zero
   underruns are the same JSON otherwise, and they are opposite facts. `device.rs` already draws this
   distinction on screen; the wire must not flatten it. This is the loud-on-unmeasurable bar.
3. **`fps` without its measurement window is invalid, not merely unhelpful.** A one-second and a
   sixty-second figure differ most exactly when something is wrong, which is when the field will be read.

### Process-dependence, and the hazard it inherits

This describes a **deployment**, not the protocol: only a process that presents frames can answer it.
**A headless `oracle-aether` MUST NOT advertise it** — the same shape §11.40 M2 imposed on
`capabilities.events`.

⚑ **And it inherits that ruling's hazard, so say it here rather than discovering it again:** advertising
a member only on some binaries makes the served set **process-dependent**, so a consumer that pins the
method list breaks by *which binary it is talking to* rather than by version. That is
`F-BANNER-INVITES-A-PIN` on a new surface. Consumers test **membership**, never a total and never a
pinned list.

## Vectors, red-first, authored here before handover

Per the bar this lane set on CR-F and §11.27, we write these and run them against the schema:

1. all fields present, `audio.unmeasured = true`, no device — **valid**
2. `audio.underruns = 0` with no audio device and `unmeasured` absent — **red** (a real zero and an
   unmeasured zero must not share a shape)
3. `fps` present with no measurement window — **red**
4. a headless server advertising `emulator/pacing` in `initialize` — **red**
5. `presented` present, `frameTimeMs.samples = 0`, percentiles absent — **valid** (nothing sampled yet
   is a legitimate state and must not be faked)

## What this does NOT do, stated so it is not oversold

It does **not** measure presented-under-vsync on the real GPU. It makes that measurable **without a
second window and without spending another of the owner's foreground authorisations**. It is the
instrument, not the answer, and `F-VSYNC-NEVER-MEASURED` stays open until someone reads it.
