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
