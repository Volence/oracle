# Timing ground truth: who is right about the ~8-tick startup stall? (2026-07-23)

**Question.** Booting `s4.soundtest.bin`, Oracle's capture loses ~8 SMPS sequencer ticks in a
one-time startup stall (then runs clean 60 Hz forever); ours holds ~60 Hz from the first tick and
drops nothing. Which behavior matches real Sega hardware?

**Verdict: (B) — OURS is hardware-correct on tick count; Oracle over-blocks.** Confidence ~85–90%.
The stall is an artifact of Oracle's per-access Z80→68k-bus BR/BG handshake resolving against its
deterministic timeslice engine, which charges a single pending Z80 bank-window access **whole
frames** of delay that the real bus arbiter cannot produce. Matching Oracle here would make us
*less* accurate. (One nuance: real hardware *does* impose µs-to-few-ms Z80 bank-window delays that
we model as zero — but the Timer-A overflow flag latches, so those delays cannot change the tick
count. See §5.)

Everything below is labeled **[measured]**, **[reasoned]**, or **[assumed]**.

---

## 1. The precise shape of the stall [measured]

Tick = the driver's per-frame `$27=$15` Timer-A rearm write. Frame-bucketed tick counts from the
decisive clean-boot pair (`vgm_ticks.py`, scratchpad):

| frames | ours_fresh | oracle_fresh2 |
|---|---|---|
| 1–7 | 1/frame | 1/frame |
| 8–9 | **0,0** | **0,0** |
| 10 | **2** | **2** |
| 11–22 | 1/frame | 1/frame |
| 23–29 | 1/frame | **0×7** |
| 30+ | 1/frame | 1/frame (one more 0 at frame 38) |
| first second | 58 ticks | 50 ticks |

Two things matter:

- **The frames 8–10 gap is shared byte-identically by both emulators.** When the *driver itself*
  genuinely overruns (song-load window), both cores agree exactly. The divergence class is
  exclusively the frames 23–29 window (plus the single frame-38 drop), which only Oracle shows.
- **Write-level shape of Oracle's stall** (YM writes, frames 17–34 of `oracle_fresh2.vgm`):
  tick 22's rearm lands at f=22.13 — and then **zero YM writes and zero PSG writes for ~8 frames**
  — then the *entire* first-note sequencer pass (note freq `$A4/$A0`, key-offs, a full ~25-write
  patch upload, key-ons `$28=$F1/$F2`) bursts out between f=30.17 and f=30.57, followed by exactly
  **one** catch-up tick at f=30.57 and steady 1/frame from f=31.13.

So in Oracle the tick-22 `Sequencer_Frame` pass is *in flight for ~8 emulated frames*, frozen
before its first YM write, and the ~8 Timer-A overflows that elapse meanwhile collapse into one
catch-up tick (the overflow flag is a single latched bit). That is the entire divergence.

## 2. The stall is deterministic in Oracle [measured]

Suspicion of scheduler nondeterminism was tested directly: two additional fresh clean-boot,
free-running Oracle captures were taken this session (armed at pristine power-on, PC/SP =
`0xFFFFFFFF`; `fable_boot1.vgm`, `fable_boot2.vgm`, 25 s each). **Both reproduce the tick timeline
of `oracle_fresh2.vgm` frame-for-frame identically** — zeros at 8–9, double at 10, zeros at 23–29,
zero at 38. Three independent boots, one timeline. This is modeled behavior, not noise. (Exodus
lineage is deterministic-by-design, so determinism does not by itself mean hardware-faithful.)

## 3. What Oracle's Z80 and 68k are actually doing in the window [measured]

Live MCP session, stepped through the startup window frame-by-frame while reading the driver's own
tick counter `SND_STAT_TICK` (Z80 `$1F13`, incremented at the top of every `Sequencer_Frame`):

- The **Z80 executes freely** throughout the no-tick window — PC sampled at `$00D3/$00DF/$00E2/
  $00E4/$00F1` (the idle-loop Timer-A poll), SP=`$1FFE`, R advancing. **No BUSREQ hold, no Z80
  freeze.** The tick counter stayed frozen (18) for 5+ sampled frames while the Z80 polled.
- The **68k is doing ordinary CPU-bound load work**, sampled at five distinct sites across the
  window: an LZ inner copy loop at `$2658` (`Sine_Table+362`), `S4LZ_DecompressDict`,
  `BuildStaticDMA`, `Sound_FadeIn` (SR=`$2700`), `Collision_ProbeLeft`. A 68k that is visibly
  retiring instructions at five different PCs is **not** bus-locked by a 133 ms DMA.

Caveats, honestly stated: (a) under MCP instruction-stepping the stall signature differed from the
free-run captures (the overflow flag simply stopped arriving while the Z80 idled at top level,
around tick 18 instead of 22) — stepping distorts Oracle's device scheduling, so the live run is
treated as *secondary* evidence for "what the CPUs are doing," not for stall timing; (b) the user
touched the emulator mid-session (a resume) — the sampled window's frame tokens advanced exactly in
line with the issued step counts, so the samples above appear uncontaminated, but the possibility is
noted.

## 4. The mechanism in Oracle's source [measured — code read]

`oracle/Devices/MD1600IO/MDBusArbiter.cpp`, `ReadZ80ToM68000` / `WriteZ80ToM68000` (lines
670–877): **every** Z80 access to the 68k bus — each `$8000` window read of the song stream, each
`$6000` bank-latch write — performs a full M68000 bus-request handshake:

1. *"If the VDP is currently requesting the bus, wait until it is finished, IE, until BR is not
   asserted. (possible infinite delay)"* — `AdvanceToLineState(BR, 0, …)`
2. Wait for the 68k to finish re-acquiring the bus (BG negated) — *(possible infinite delay)*
3. Assert BR; 4. wait for BG asserted — *(possible infinite delay)*; 5. perform the access;
   6. negate BR — with `ClampHandshakeTimeDeterministic` clamping each step to the **current
   timeslice end** (the file's own comments describe measured multi-hundred-µs handshake wedges
   against slice boundaries).

Under the soundtest load window (decompress → `BuildStaticDMA` DMA bursts, repeatedly), the tick-22
pass's first 68k-bus access enters this handshake while VDP BR traffic is hot, and the multi-step
wait — each step resolvable only against device-timeslice progress — accumulates whole frames
before the access completes. That reproduces exactly the observed shape: one pass frozen before its
first write for ~8 frames, then completing normally, deterministic every boot.

Corroborating history [measured — source comment]: `aeon/engine/sound_constants.asm:186–190` — the
driver author *already measured* Oracle dropping ticks under load ("3597 ticks / 3600 emulated
frames … deficit = ~3 residual long-tick overruns per minute, SND_STAT_TICK vs oracle frame
count") and retuned NA 136→137 to compensate. Occasional multi-period charging of Z80 bus accesses
is a standing Oracle trait, not something this ROM provoked for the first time.

## 5. Can real hardware do this? The cycle budget [reasoned]

- One Timer-A period at NA=137: `(1024−137) × 1008 mclk = 894,096 mclk` = **59,606 Z80 cycles =
  16.652 ms** (~60.05 Hz).
- The heaviest startup pass (tick 22: first note on all channels + one full patch upload) costs, from
  the shipped code: 44 YM data writes × ~76 cyc (`Fm_YmWrite` has **no busy-poll** — one `nop` of
  spacing) ≈ 3.4k cyc; SetBank ×2 ≈ 0.4k; stream parse / ModUpdate / tempo+fade ramps, generously
  ~15–25k. **Total ≲ 30k cycles ≈ half of one Timer-A period.** The driver's own header budgets the
  whole tick the same way.
- Because the overflow flag **latches**, losing even one tick requires the Z80 to be unable to
  complete a poll+rearm for **> 2 consecutive periods (> 33 ms)**; losing 7–8 requires **~133 ms of
  continuous starvation**.
- The only hardware mechanisms that stop this Z80 are (a) BUSREQ hold and (b) a bank-window access
  blocked by an in-progress VDP DMA (the documented Sega warning; the driver's own comments model
  it — its DAC producer even has a DMA-drain path for exactly this). Worst realistic single block:
  one display-off DMA transfer, a few ms; a 133 ms *continuous* block would need one uninterrupted
  ~100 KB DMA (larger than VRAM) — and is contradicted by the measured fact that the 68k was
  *executing instructions* (decompressing) across the window, which requires the bus, which means
  every inter-DMA gap frees the arbiter and completes any pending Z80 access in µs.

**Conclusion of the budget:** on silicon this pass cannot overrun even one period, and no bus
activity present in this ROM can starve the Z80 for 33 ms — let alone 133 ms. Hardware drops 0
ticks here. Ours drops 0. Oracle drops 8.

The honest flip side: our core's bank-window read (`crates/oracle-core/src/z80/bus.rs`) is an
immediate array read — zero contention, by design. Real hardware would charge µs-to-ms occasionally.
That gap is real but **cannot change tick counts** (flag latch, above); it lives in the already-
deferred sub-cycle timing bucket, and closing it should target *real* arbiter costs (µs-scale), not
Oracle's frame-scale charges.

## 6. Third reference [assessed]

- No real-hardware VGM of this exact ROM/driver can exist to find — it is this project's own custom
  Layer-6 driver on the user's own hack; a rip of the original song would not adjudicate *this*
  driver's startup. Not pursued further.
- Headless BlastEm remains the nearest independent software reference; known-troublesome, not
  fought this session (per brief).
- The decisive experiment stays cheap and physical: **flash-cart run of `s4.soundtest.bin` on real
  hardware, phone-record the first two seconds of audio.** Oracle's behavior predicts an audible
  ~133 ms dead stall between the sequencer starting and the first notes (song start "hiccup");
  ours predicts a clean start. A first-second tick-rate count (50 vs 58) is measurable from a
  waveform. Alternatively: the same ROM under BlastEm with FM logging.

## 7. Verdict

**(B) Ours is hardware-correct on tick count — confidence ~85–90%.** Do **not** chase Oracle's
startup stall; document it (this file) and treat first-second tick-count divergence vs Oracle as
expected. If the residual ~8 s melody-index offset in RT-3 style diffs needs to disappear for
comparison hygiene, diff with a tick-index alignment rather than write-position alignment.

Remaining 10–15% doubt: silicon-level arbiter pathologies under sustained DMA are not fully
characterized by documentation alone; the flash-cart recording (§6) closes it.

## Session hygiene

- No source edits, no commits. Scratch artifacts in the session scratchpad
  (`fable_boot1.vgm`, `fable_boot2.vgm`).
- Oracle left **paused**, VGM logging stopped, **all breakpoints cleared**. Note: 7 breakpoints
  pre-existed this session (not mine) and were removed to keep the free-run captures clean:
  `0x5CAC8` (×2), `0x5CAB0`, `0x5E5C2`, `0x5E5AA`, `0x9C44`, `0x3C46` (1,691,410 hits). Restore if
  another workflow needs them.
