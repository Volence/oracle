# Aeon's second demand-side statement: scanline readback (2026-08-18)

Relayed from the Aeon session the evening Tier 1 shipped. Recorded for the same reason the
gap list was: it is a demand-side statement of what this bus is missing, with the demand
evidenced rather than asserted. Three asks, plus a parity fixture they handed back.

## Ask 1 — deterministic scanline/pixel readback (their top priority, above stepping)

**The problem, in their words:** Aeon's raster layer writes CRAM mid-scanline from an HBlank
handler. Whether a write lands inside horizontal blanking or in active display is a **pixel**
question ("row 99 tinted from x~170 of 320"), and nothing else can see it: CRAM reads report
the final value never the landing time; screenshots are press-frame non-deterministic;
the replay net is pixel-blind by construction. Three separate capture protocols have failed
their own controls in the aeon repo. Every landing measurement is a hand ritual
(pause → poke → screenshot → count pixels in a PNG).

**The ask:** "give me row N's 320 pixels as of frame F" — raw indices or RGB, no PNG, no
viewer. **Determinism is the whole requirement**: same ROM + same inputs → identical bytes
every run.

**What it unblocks now:** a confirmed in-flight Aeon defect (CRAM burst landing mid-active-
display) whose fix needs the HBlank window located in cycle space — a delay-value sweep that
is ~20 manual screenshot analyses today and would become an automated, permanently-protective
gate with this capability.

**This repo's position (assessment, not yet ruled):**
- The core seam **already exists**: `oracle_core::scanline_capture::ScanlineCapture` captures
  the LIVE per-line raster (mid-frame raster effects included) — it is what `sh_probe` uses,
  and the S3-era lesson is on record: post-hoc frame dumps are structurally blind to mid-frame
  raster effects; ScanlineCapture is the honest instrument.
- Determinism is already this core's construction (seeded machine, determinism gate); the
  capability needs no new determinism work, only a **bus surface**.
- It is adjacent to two standing backlog items: Tier 2 item 5 (`run_to_scanline`) and the
  per-scanline capture prerequisites (`F-TRACE-VDPWRITE-MCLK` unblocking `F-CRAMDOT`). The
  ask sharpens Tier 2 item 5's justification and may partially supersede its shape: what they
  need first is the *readback*, not the *stop*.
- **Contract-first applies in full**: no row exists for any pixel readback; this is a new
  capability → CR (candidate CR-24 or CR-25, alongside F-WM-ECHO) → §6 row + fragment →
  handler. Not built in the Tier 1 slice; queued for the owner's next-slice pick with Aeon's
  explicit ranking attached: **"asks 1 and 2 are worth more to Aeon than stepping is."**
- **SHIPPED 2026-08-19** as `emulator/scanlines` (CR-24 → ruling → §11.14 → handler), field 1
  exactly as scoped here — rendered RGB, S/H applied, row range, active-only, mode-aware width, no
  sub-frame addressing. Handoff, including the A1/A2 acceptance protocol handed back to Aeon:
  `docs/2026-08-19-scanline-readback.md`.

## Ask 1 follow-up — the SHAPE, ruled by the demand side (same evening)

Their answer to the RGB-vs-index question, with rationale worth preserving verbatim in the CR:

1. **Rendered RGB with S/H applied is REQUIRED** — "not a preference": the defect class is a
   mid-scanline CRAM *write*, so every pixel in the row references the same CRAM entry before
   and after; **pre-palette indices are identical either side of the landing point and are
   structurally blind to the bug** — exactly as our post-hoc frame dumps were blind to
   mid-frame raster effects. The boundary x in rendered RGB *is* the measurement.
2. **Per-pixel CRAM index ranked SECOND, not zero** — for attribution ("pixels at row 99,
   x>170 use index $4A and that entry changed mid-row" — a gate that detects *their* change,
   not *a* change), and to separate two failure modes they have actually shipped: Aeon
   toggles S/H (reg $0C bit 3) as a separate op from the palette write, and a recorded bug
   shipped the palette half without the register half ("tinted but visibly lighter", found in
   play). In RGB alone that reads as slightly-wrong colour, not a missing op.
3. **Per-pixel S/H state (shadow/normal/highlight) THIRD, if cheap.**
4. **Row RANGE, not single row** (assertions are always about a boundary; a single-row call
   would just be called N times). **Active-only 320 px is sufficient and cleaner** — a write
   landing correctly in blanking is invisible by definition, and shows up in active-only
   capture as the clean pass condition. Mode-aware width (H40 320 / H32 256) if free; Aeon is
   H40 throughout. "As of frame F" against the deterministic frame counter is fine — no
   sub-frame addressing needed.

**Feasibility facts, verified against `crates/oracle-core/src/scanline_capture.rs` (2026-08-18):**
the sink interface receives `(line, &[(r,g,b)])` — the LIVE rendered line, S/H applied. So
**field 1 (rendered RGB) is free today**: the capture already holds exactly the required
bytes, and only the bus surface is missing. **Fields 2–3 (per-pixel index / S/H state) require
extending the renderer→sink interface** — the renderer resolves indices and S/H internally
and hands the sink only RGB (the S/H-aware `cram_rgb_state` conversion is private, a fact the
S3 lens work already recorded). That extension is a core change and gets the standard
currency-neutrality scrutiny; the CR should scope field 1 as the first slice and fields 2–3
as a named follow-up with their attribution rationale attached, unless the extension proves
trivial at design time.

## Ask 1 second follow-up — the acceptance fixture EXISTS (same evening, verified)

`aeon/docs/benchmarks/scanline-p2/HBLANK-WINDOW-SWEEP-SPEC.md`, aeon commit `1fb982f7`
(branch `parcel/raster-substrate-byte-moving`) — **existence and structure verified firsthand**
(prediction, one-poke fixture, classification, acceptance criteria all present as described).
The future CR inherits a worked example and its acceptance tests ready-made:

- **Field 1 ALONE unblocks the sweep completely** — their explicit planning answer. RGB +
  row-range + active-only + mode-aware width is a sufficient first slice; fields 2–3 must NOT
  hold it up. The split proposed above is confirmed as the right call by the demand side.
- The sweep is one `write_memory` poke per value (`Raster_Buf_A + 20`), paused-poke discipline
  enforced by our own `-32005` gate ("enforced rather than remembered" — their words), a
  falsifiable prediction (clean N ∈ [15, 19], centre 17) with a §7 rule that a disagreeing
  measurement is the FINDING and the fixture must not be tuned until it agrees.
- **★ A2 is the capability's own non-vacuity check and should live in OUR suite permanently:
  N = 0 and N = 17 MUST produce different content on row 99.** By end of frame the CRAM value
  is identical either way; only the mid-frame landing time differs. A capture reporting
  post-frame state — however clean and deterministic — returns identical rows for both and is
  structurally blind to the defect class. This is the two-background-opacity-harness idea in
  raster form: an assertion that cannot go stale because it tests the instrument's
  discriminating power, not a pinned value.
- **A1 (determinism, ≥3 runs byte-identical)** is the criterion three prior Aeon capture
  protocols failed on their own controls.
- **Content trap for any synthetic fixture:** the tinted CRAM entry must be one the art at the
  measured rows actually references (their R1 got a null result from near-unused entries;
  the spec pins line 2 with Camera_Y frozen at 144). A synthetic test ROM inherits this
  constraint or its A1 passes while meaning nothing.

## Ask 1 closing addition — the disagreement DISCRIMINATOR (run unconditionally)

If the capture disagrees with the predicted clean range, two already-measured anchors assign
the fault — both observed CLEAN on oracle, both buildable by the same `raster_cost_probe.py`
encoder as the sweep fixture (two extra poke-and-capture runs on the same harness):

- the **row-119 fixture** (`reg_set` + `stream_cram`, CRAM op second) — measured 1 px spill → 0,
  boundary on the authored line;
- **R1 §7.3** (`pal_restore` alone, dispatch depth 4) — row 139 fully tinted, 140+ fully base,
  OFF edge exactly on the authored line.

The anchors **bracket** the disagreement — same handler, same burst, same window, differing
only in preamble cycles ahead of the write:

| Anchors | Sweep vs [15,19] | Verdict |
|---|---|---|
| both CLEAN | disagrees | our raster timing agrees with oracle's → **Aeon's §3 arithmetic**; they own it and re-derive |
| either DIRTY | — | the capture disagrees with a landing oracle measured clean on an untouched shape → **our raster timing or the capture's sampling point** |
| both DIRTY | — | almost certainly fixture/harness — check the **content trap** first (is the art actually sampling the tinted entry) |

**Run the two anchors unconditionally as the sweep's first two data points**, not only on
disagreement — they give the sweep two known-good calibration rows before it ventures into
the unmeasured shape. This belongs in the CR as its verification protocol, not as an appendix.

## Ask 2 — does oracle-next separate HInt from VInt (profiler conflation)? ANSWERED

Their finding about **oracle** (the C++ reference): `interrupts.hint` buckets by comparing
handler entry PC against `0x78`, Aeon's VBlank handler never matches, so HInt and VInt sum
into one bucket; they work around it with per-routine rows and never trust `interrupts.hint`.

**Answer given: oracle-next has NO profiler instrument at all** — none of the 31 advertised
methods is profiler-shaped, so the conflation is neither reproduced nor fixed; it is absent.
Their per-routine discipline applies to oracle only.

**Design pin registered here so it survives:** ★ when a profiler surface is built on this bus,
HInt and VInt MUST be separate buckets keyed by *cause* (which interrupt was taken), never by
handler-entry-PC pattern matching — the oracle conflation is the measured counterexample, and
"a per-frame HInt total" is a named instrument one Aeon budget phase has never had.

## Ask 3 — pause / write / resume semantics. ANSWERED, verified firsthand

**The guarantee, verified against `crates/oracle-aether/src/server.rs` (`engine_loop`):**

- One thread owns the `System` for its whole life; every command drains **in order** on it.
- While paused the loop **parks on the channel** (`rx.recv()`); the machine advances only in
  the free-run arm, which is unreachable while paused. So after `pause`'s reply, **zero
  emulated cycles execute until `resume`** — the window between pause-ack and a write landing
  is exactly zero, structurally.
- `pause` during free-run lands between frames (messages are polled between frames), so a
  paused machine is always at a **frame boundary** — a poked program is observed by the first
  resumed frame in its entirety.
- `write_memory` additionally **refuses `-32005 machineRunning`** on a running machine, so the
  race they lost a capture to is not merely avoidable but inexpressible: a poke that could
  race the engine's per-VBlank re-record cannot land at all.
- Hosted-mode caveat (the one honest asterisk): the player's own pause key shares
  `set_free_run` with the bus, so a *human* unpausing between a client's `pause` and its
  `write_memory` changes the mode — and the write then refuses rather than racing. The
  guarantee degrades to a refusal, never to a corrupted measurement.

## The parity fixture they handed back

Aeon's eight raster cost fixtures, re-measured on oracle **2026-08-18** against a changed wire
format (**do not use older figures from aeon docs**), all eight matching their cost model to
the cycle (3 boots, spread 0). Marginal cost per fire, (fixture − F0)/n, F0 = 572:

| Fixture | n | cost |
|---|---|---|
| F1 reg_set | 6 | 412 |
| F2 stream_cram 1w | 6 | 462 |
| F3 stream_cram 3w | 5 | 522 |
| F4 stream_pal_region 3w | 6 | 570 |
| F5 reg_set + cram 3w | 4 | 632 |
| F6 two cram 1w, 1 fire | 4 | 622 |
| F7 stream_vsram 1w | 6 | 462 |
| F8 pal_restore 3w | 6 | 708 |

Driver: `aeon/tools/raster_cost_probe.py` (pokes a program into `Raster_Buf_A`, reads the
per-routine profiler row for the HBlank trampoline). **Standing caution from our own record:**
absolute band-edge/cycle-count claims keep oracle as the reference instrument until this
core's instruction-granularity slop closes — these fixtures are an A/B instrument check
("an emulator reporting different numbers is measuring something else"), not yet a gate this
core is expected to pass absolutely.

## Also from the message

- **F-WM-ECHO deprioritized by its own beneficiary**: they read back to verify and
  `memory_hash` makes it cheap — "do not prioritise it on Aeon's behalf." Ledger updated here.
- **Stepping is not on Aeon's critical path** — their ranking puts asks 1–2 above it. The
  Tier 2 item 4 keep-dead collision still needs an owner ruling before any build; this makes
  the ruling less urgent, not resolved.
- **Their P2 measurement plan corrected off our answer**: Scanline P2 Phase 0 now runs on
  oracle (full stop); oracle-next is not a candidate for it until a profiler surface exists.
  They endorsed the A/B-not-absolute framing and will not assert oracle-next cycle parity
  while the instruction-granularity slop is open. They also asked that the HInt/VInt-by-cause
  design pin STAY pinned — their sharpened statement of why: entry-PC bucketing mis-buckets
  for ANY ROM whose vector points where the heuristic didn't anticipate, producing a silent
  wrong number rather than a missing one, "which is the worse kind."
- **Their read of write_memory's -32005 gate**: converting the silent-corrupted-measurement
  race into an unrepresentable state "is worth more than the zero-cycle window itself" —
  worth remembering as evidence next time strict-first refusal semantics are argued.
