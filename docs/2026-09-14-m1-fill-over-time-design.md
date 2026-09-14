# M1-FILL-OVER-TIME: a DMA fill that runs across time and shares the FIFO (design)

> **Dated 2026-09-14.** Parcel M1-FILL-OVER-TIME, branch `parcel/m1-fill-over-time-design`, brief
> `docs/2026-09-14-m1-fill-over-time-design-brief.md` (oracle `335f907`, base `14cb815`). **Design document,
> no fix.** Every number below was measured on this branch. The measuring code was committed as `spike:`
> commits and removed from the tip, so the merge is docs-only. The spikes: an observer-only population
> instrument (`03b00a8`, fixed in `c60d3b0`), a save-compatibility probe (`5f266f6`), and a working prototype
> of the recommended model (`9da8784`). The hardware section uses documentation, forum prose and the test
> ROM's own code only (disassembled with Capstone 5.0.7). No emulator source was read.

## Summary

**Recommendation.** Make a fill a process that the VDP advances lazily, on the clock it already has. The
FIFO drain clock `fifo_slot_clock` becomes the one external-slot clock that FIFO entries and fill steps
share. Pending entries always go first, and a fill step takes one slot when the FIFO is empty. The fill
catches up to "now" at every VDP port access, before every active line renders, and at the end of every run.
Each step takes its data and write target from the FIFO ring, as Nemesis describes, never from a stored
copy of the trigger word. Every piece of progress lives in state the machine already has: the length
registers count down, the address and source registers walk, and CD5 clears when the length reaches 0.
**The only new state is one bit, "this fill has taken its trigger and is running"**, stored as a trailing
`DmaRequest::FillRunning` variant in the existing `Vdp::dma_pending` option. The busy flag needs no new
code: A4's derived `fill_armed()` already reads set for a running fill, because CD5 stays set until the last
step.

**What it costs.** One parcel of about 150 lines in `vdp.rs`, `bus.rs` and `system.rs`. A working prototype
of it was built and measured on this branch: VDPFIFOTesting goes **119/3/122 → 122/0/122**, tests 31/32/33
pass on exactly the words derived by hand, and no other VDPFIFOTesting test moves. The 16 other scorecard
rows, every visual baseline, the scanline goldens, `golden_frames`, `determinism_gate`, `export_state_v1` and
all aeon replay tests stay green. What moves: 2 conformance pins, 12 unit tests that encode an instant fill,
and 4 integration tests. Three of the four are on synthetic fixtures that start a 64 KiB fill and never wait
for it; the fourth is the watch-hit attribution below. The fill-free path shows no cost above noise
(median 0.591 s against 0.596 s for 1,200 frames, inside run-to-run spread).

**What it does to existing saves.** **No save that loads today refuses, and none loads into a different
machine.** The save-state layout fingerprint does not move: it is `dff350afa2eb3e1d` in both builds, a
140,072-byte probe. All three old snapshots tried load in the new build, two of them taken in the middle of
a fill, and they reach exactly the old build's hashes 60 and 120 frames later. The one asymmetry runs the
other way: a snapshot the **new** build takes while a fill is running is refused by an **old** build, as a
clean bincode decode error, not a silent misload.

**What needs a ruling before anything is built.** (R1) Watch hits for fill writes would be attributed to the
instruction that caught the fill up, not to the trigger, and would carry each step's own slot instant. That
is a wire-visible change of meaning. (R2) The same per-step stamp revisits decision C-6 ("one DMA burst =
one stamp") for fills. (R3) Is the forward-compatibility asymmetry acceptable? (R4) Three synthetic fixtures
need a busy-wait after their fill, because under this model their mid-fill command word stops the fill, as
documented. See §7.

## Where the brief was wrong (measured)

1. **"Where exactly the switch happens does not change the tables" holds for the tail only.** Tests 31/32/33
   also read the fill's first four words back, and those must still hold the *old* fill data. That puts a
   lower bound on the switch: the mid-fill write must land after at least **7 fill steps** (§1.3). The model
   meets it with little to spare: **11** steps for VRAM and **9** for CRAM/VSRAM, margins of 4 and 2 slots
   (§5). Timing matters inside a window, and the slot rate is what puts the write inside it.
2. **A mid-fill write does not consume a length count, and the ROM pins this.** If it did, test 31 group 2's
   tail would read `5656 5656 0000 0000`. Hardware reads `5656 5656 0056 0000` (§1.3).
3. **The fill's data and target come from the FIFO, not from the trigger word.** Nemesis: the DMA unit "will
   pull the write target and the upper byte of the write data from the FIFO entry". Today
   `DmaRequest::Fill { fill }` carries a copy of the trigger word. For an instant fill the two cannot
   differ; for a fill that runs over time they do, and the tables side with the FIFO (§1.2).
4. **The fill trigger is not FIFO-timed today.** `Vdp::data_write_at` returns before `fifo_drain` for every
   CD5 data write, so the trigger never stalls and never advances the drain clock. The brief did not name
   this. My first instrument tripped over it (its window could begin before its trigger). The design makes
   a fill-mode data write an ordinary FIFO-timed write.
5. **The corpus the brief named misses the population that actually moves.** The aeon replay fixtures and
   vendored ROMs never touch the VDP inside a fill window, except VDPFIFOTesting's own tests 31/32/33 (§6).
   The movers outside VDPFIFOTesting are three **synthetic** fixtures (`testrom::build_pad_poll`,
   `testrom::build_cram_midframe` and the frontend's `build_midframe_cram_rom`). My population instrument
   did not run those; the prototype found them.
6. **The official per-line fill rate is not the external slot count.** The MegaDrive Wiki's VRAM-fill
   bandwidth table gives 3,808 bytes per 60 Hz H40 active display, 17 per line, and 7,752 per blanking
   period, 204 per line. The 68k→VRAM table gives 4,032 (18) and 7,524. Eke says a fill loses a byte on
   the first line only. The documents disagree, so this design does not pick; the ROM's tables do not
   separate the readings (§1.1).
7. **The save-layout risk the brief expected does not arise.** The one new bit fits in an existing field, the
   probe's fingerprint is measured unchanged, and old saves are measured to load. The new risk is the
   reverse direction (R3).

Premises that held: the three failures are one mechanism; `fill_armed` is derived and can stay derived; the
listed unit tests (`vdpfifo_t4_fill_trigger_and_byte_placement`, the busy-window tests) do encode instant
completion; the baseline is 90/90 legs, 2912/0/3.

## 1. The hardware behaviour

### 1.1 How a fill advances

Nemesis, SpritesMind *VDP Internals* p.4 (t=1291, start=45):

> "When deciding whether to run a DMA fill, it checks if CD5 is currently set. Note that this is based on
> the live command register state, not anything written in the FIFO. If CD5 is set, and DMD1 is true, and
> DMD0 is false, the DMA unit will pull the write target and the upper byte of the write data from the FIFO
> entry, and write that single byte to the write target, using the current incremented command address
> register, which will then be incremented afterwards."

The same post gives the advance every DMA step shares: it adds 1 to the lower two source registers, then
subtracts 1 from the length counter, "and then if the resulting DMA length counter is 0, clear CD5 in the
command code register". Registers 19-22 are therefore the fill's own live counters, and a length of 0 is
65,536 steps because the counter wraps (RD2 needs no special case).

**Rate.** One step is one external access slot: a VRAM byte, or a CRAM/VSRAM word. The slot counts come
from the Sega *Genesis Technical Overview*: 16/18 per active line (H32/H40) and 167/205 per blanked line
(already pinned in `docs/2026-07-16-vdp-recon.md` R3, with positions from Kabuto's access pattern, and in
`Vdp::H40_ACTIVE_SLOTS`). Eke, SpritesMind t=851 p.2: "16/18 access per line when display is active",
"167/205 during VBLANK", and "VRAM fill requires one additional write to trigger the DMA operation so the
rate is one byte less (but only for the first line, which official doc does not clearly say)". **The
documentation disagrees.** The MegaDrive Wiki's VRAM-fill table
(md.railgun.works, "VDP") lists 60 Hz 320×224 as 3,808 active and 7,752 disabled, which is 17 and 204 per
line, one fewer on *every* line. The design keeps the existing slot model and does not pick between the two
readings. The ROM's tables do not separate them either: they bound the switch point, and one slot per line
moves the switch by less than the margin (§5).

### 1.2 A data-port write during a fill

Mask of Destiny, SpritesMind *Is DMA Fill buggy?* (t=2663): "If you write another word before the fill is
complete, that write will take place as normal and then the fill will continue with a byte from the new
word." Nemesis, *VDP Internals* p.4: "When a DMA Fill operation is pending, and you perform a data port
write, that data port write is completed as normal, because the DMA unit is a bolt-on addition to the VDP
core", and "The DMA fill operation will effectively be suspended until the FIFO is empty again, and at that
point, it will now pick up its fill data from the last data that was moved through the FIFO."

**Which byte.** For VRAM, the **high byte of the newest FIFO entry**, written to `address ^ 1`: `$56` from
`$5678`, then `$9A` from `$9ABC`. For CRAM/VSRAM, the **next-available ring entry** (the word written four
writes ago), re-read after the write. Nemesis: "when CRAM or VSRAM is the write target, DMA fill seems to
fail to latch the fill data correctly. The apparent effect you see is that instead of using the data in the
last written FIFO slot, it uses the data in the next available FIFO slot". The CPU write itself lands at the
fill's current address and steps it once. It consumes **no** length count, which the prose does not say and
the ROM does (§1.3).

### 1.3 The ROM's derivation (VDPFIFOTesting, disassembly)

Setup, from the ROM's init (`$0230..$0278`): register 12 = `$81` (H40), register 1 = `$44` (display on).
Test 31 is at `$2BD8`, 32 at `$340A`, 33 at `$3CBA` (entry list `$030C..`).

**Test 31 group 1** (`$2C0C`), the control. Clear `$8000-$8FFF`, then: autoincrement 1, register 1 `$54`,
length `$0FFB`, register 23 `$80`, command `$40000082` (VRAM `$8000` + CD5), trigger `$1234`, poll busy, read.
The trigger is completed as a normal write (`$8000 = $12`, `$8001 = $34`; A3b) and steps the address to
`$8001`. Step *k* (0..4090) runs at address `$8001 + k` and writes `$12` to that address `^ 1`. That covers
`$8000` and `$8002..$8FFB`; the last step is at `$8FFB`. Tail at `$8FF8`: `1212 1212 0000 0000`, as the table
says.

**Test 31 group 2** (`$2E5E`). The same fill, started just after vblank ends (`$2E86..$2EA0`). Then
`move.w #$20,d7 / dbra` (33 turns), then `$5678` at `$2EF4`. The write lands at the fill's current address
X, `$56` at X and `$78` at X ^ 1, and steps the address to X + 1. The remaining steps write `$56` to `addr ^ 1`
from X + 1 up to `$8FFC`: 4,091 steps in total, because the write consumed none. Tail: steps at `$8FF8`,
`$8FF9`, `$8FFA` and `$8FFB` fill `$8FF8..$8FFB`. No step runs at `$8FFD`, so `$8FFC` stays 0. The step at
`$8FFC` writes `$8FFD`. That gives **`5656 5656 0056 0000`**. The alternatives the table rules out:
- the write consumes a count: `5656 5656 0000 0000`;
- the fill keeps its latched byte: `1212 1212 0012 0000`;
- the fill uses the low byte: `7878 7878 0078 0000`.

**Head bound.** Bytes `$8000..$8007` read back `1234 1212 1212 1212`, so neither X nor X ^ 1 may fall below
`$8008`. That needs X ≥ `$8008`, which means at least 7 fill steps before the write.

**Test 31 group 3** (the same wait for vblank's end, trigger at `$3162`). Writes `$5678` at `$3172`, then
`$9ABC` at `$3182`, after 33 more turns. Two extra address steps put
the last fill step at `$8FFD`, and the data is the **second** write's high byte: **`9a9a 9a9a 9a9a 0000`**.
The eight snoop words after each group (`$2D08..`) check the ring:
- group 2: `0000 0000 0000 0000 1000 1010 5000 5010`;
- group 3: `0000 0000 1000 1010 5000 5010 9800 9010`.

These already pass today, which confirms that the mid-fill write enters the ring as an ordinary entry and
that fill steps add nothing to it.

**Test 32 (CRAM)**: group 1 at `$343E`, group 2 at `$36BA` (write `$0CCC` at `$377A`), group 3 after it
(disassembled only as far as its setup; its table is the one cited). Four VRAM writes load the ring with
`0222 0444 0666 0888`. The fill is 59 steps to CRAM 0 with autoincrement 1, and the trigger is `$0AAA`,
which leaves the ring at `0444 0666 0888 0AAA` with next-available `0444`.
- **Group 1:** the steps at addresses 1..59 write CRAM word `addr >> 1` = `0444`, words 0..29. The read at
  `$38` gives `0444 0444 0000 0000`.
- **Group 2:** after 25 `dbra` turns, `$0CCC` is written. The ring becomes `0666 0888 0AAA 0CCC`, so
  next-available is `0666`. One extra address step ends the fill at address 60 (word 30), and the read at
  `$38` gives **`0666 0666 0666 0000`**.
- **Group 3:** a second write, `$0EEE`, makes next-available `0888`, giving **`0888 0888 0888 0000`**.

**Test 33 (VSRAM)** is the same, except the reads merge the snoop into bits 15-11. In group 3 the ring's
next-available entry `0888` sets bit 11, so the unwritten word 31 reads **`0800`**: **`0888 0888 0888 0800`**.
Group 2 gives **`0666 0666 0666 0000`**.

### 1.4 The 68000 during a fill

A fill does not halt the 68000. MegaDrive Wiki: "These two modes don't freeze the M68k, but it is
important that only the VDP status register and H/V counter are read, and the PSG registers are written.
Doing otherwise may corrupt VRAM and VDP registers." Kabuto's hardware notes (Plutiedev mirror): "DMA
fill/copy: these run internally in the VDP and the CPU is not blocked from accessing the control port." A
mid-fill data-port write takes a FIFO slot and stalls the 68000 only when the FIFO is full, which is the
existing R3 rule and needs no change.

### 1.5 Status bits during a fill

**DMA busy (bit 1)** reads set from the fill's control write (Eke, *VDP Internals* p.4: "on DMA Fill, busy
flag is actually immediately (?) set after the CTRL port write"; A4) until the last step clears CD5
(Nemesis's advance rule). With CD5 staying set for the whole fill, A4's `fill_armed()` already expresses
this. **FIFO empty/full (bits 9/8): the documentation is silent** on their value during a fill. Nemesis's
description has the fill *read* the last entry rather than occupy the FIFO. In the model EMPTY therefore
reads set during a fill, unless the CPU has writes pending. No VDPFIFOTesting table samples the FIFO bits
mid-fill (tests 31-33 test bit 1 only, and tests 36/38 sample before the trigger and after completion).
**TAG: needs a live look.**

### 1.6 A control-port write or a data-port read during a fill

**Officially prohibited** (the MegaDrive Wiki sentence in §1.4). Nemesis's "live command register state"
settles the mechanism one step further:
- A command word written with DMA enabled rewrites CD5. A non-DMA command clears it, and **the fill stops**.
- With DMA disabled CD5 cannot change (recon R1), so the fill continues at the new address.
- A register write takes effect at the next step: register 15 changes the stride, register 1 the slot rate,
  and register 23 leaving Fill mode pauses the steps.

**Beyond that the documentation is silent**, and no ROM table covers it. **A data-port read during a fill:
silent.** With the fill's write code still live, a read is recon R1's lockup cell. The corpus population
for both is zero (§6), outside the synthetic fixtures. **TAG: needs a live look** (a capture of "fill, then a
non-DMA command word mid-fill").

### 1.7 VRAM copy

A copy runs over time the same way: two slots per byte, one read and one write. Eke, t=851: "VRAM Copy
requires 2 access (one read followed by a write) so the rate is half". The FIFO is bypassed (recon R4(c)),
and its counters are the same registers 19-22. **It belongs in a later parcel.**
- **No acceptance table.** Nothing in the corpus observes a copy mid-flight. All 34 VDPFIFOTesting copies
  are polled to completion before their results are read, so a copy-over-time change has no table to fix.
- **It would move timing.** It would still move poll timing in 31 of those 34 copies, whose busy bit
  differs between the two models (§6).
- **Its own open question.** Two 513-byte copies take register writes to 1/15/19-23 inside their proposed
  window, and whether a DMA-enable toggle or a command word stops a running copy is unanswered.
- **The M1 tables do not need it.** Copy shares none of the FIFO interplay that the M1 tables test.

The same scheme carries over directly: a trailing `CopyRunning` variant, with the source counter already
live in registers 21/22.

## 2. The state model

| What a running fill needs | Where it lives | New? |
|---|---|---|
| Remaining length | registers 19/20, counted down per step; CD5 clears at 0 | no: the hardware's own counter |
| Current address | the address register `addr` | no |
| Source counter | registers 21/22, +1 per step (A3's end state, reached incrementally) | no |
| Fill data | newest FIFO entry, high byte (VRAM); next-available entry (CRAM/VSRAM) | no: the ring already holds both |
| Write target and permission | the newest FIFO entry's code, through `code_names_a_write_target` (A5) | no |
| Progress clock | `fifo_slot_clock`, widened into the one external-slot clock | no |
| Armed vs running | trailing `DmaRequest::FillRunning` in `dma_pending` | **the one new bit, no new field** |
| DMA busy | `fill_armed()` = live CD5 ∧ register 23 = Fill (A4) | no: derived, unchanged |

**How it joins what exists.** The FIFO keeps priority because both share one clock. Catch-up first retires
entries whose slots have passed, and only while the FIFO is empty does it spend a slot on a fill step. A
mid-fill write is therefore serviced before the fill resumes, which is exactly Nemesis's "suspended until
the FIFO is empty again".
- **A3** becomes incremental. It ends in the same state, measured by tests 28/29 still passing.
- **A4** does not change. Busy and progress are one piece of state: CD5 clears in the same step that
  finishes the fill, so they cannot disagree.
- **A5** moves into the step, which decodes the newest entry's code. Test 34 group 3 still writes nothing.

`dma_busy_until` stays for copies and 68k DMA, and fills stop setting it.

**Why a bit is unavoidable, and why it goes where it does.** "Armed" and "running" must differ: an armed
fill with an empty FIFO must not step (test 36's table shows the fill running only after its trigger).
Every existing field was checked as a way to derive the bit, and each breaks on a sequence the machine can
reach. For example, "the newest FIFO entry carries CD5" is still true after a completed fill has been
re-armed. So one bit is needed. Stored as a new field, it would move the save layout. Stored as a **trailing**
variant of the existing `Option<DmaRequest>`, it does not:
- **The fingerprint stays.** The power-on probe holds `None`, so the fingerprint is unchanged.
- **Old snapshots decode unchanged.** They hold `None` at every instruction boundary (the documented
  `dma_pending` invariant).
- **An old build refuses a new mid-fill snapshot cleanly.** It reads discriminant 3 as `UnexpectedVariant`.

Re-shaping the existing `Fill` variant instead would be decode-unsafe. An old build would read the new
payload as `Fill { len, fill }` and misalign the rest of the stream.

**P1 invariant for restore's door check:** `dma_pending` is `None` or `FillRunning` at an instruction
boundary, and `FillRunning` implies CD5. `SnapshotRegion` gains a named refusal for either violation.

### Better-approach pass

| Approach | What it does | Why not chosen | What it would do better |
|---|---|---|---|
| **(a) Lazy catch-up on the shared slot clock (chosen)** | the fill advances to "now" at each port access, line render and run end | chosen | no scheduler traffic and no idle cost. Progress is a pure function of stored state and time, so a snapshot mid-lag restores exactly |
| (b) Eager per-slot scheduler events | an event per fill slot in `system.rs`'s queue | 65,536 events for one 64 KiB fill against a queue that today carries four kinds per line; a per-slot event also interleaves badly with instruction-granular CPU time | writes would land at their real instant even with no observer, so a watch hit's pc would be the instruction executing at that slot |
| (b′) Eager per-line events | one event per line advances the fill by that line's slots | still needs catch-up at every port access (test 31's write lands mid-line), so it is (a) plus a second trigger path | a paused machine is never more than a line behind, even without the run-end catch-up |
| (c) Closed-form segments | at each observation, compute how many steps fit, and apply them as a block | not a different model: it is (a) with O(1) arithmetic per blanked line instead of a loop per slot | speed on long display-off fills; kept as optional parcel P2 |
| (d) A separate DMA clock field | a second clock beside `fifo_slot_clock` | two clocks can disagree about who owns a slot, so FIFO priority would need arbitrating between them; and a new field moves the save layout | readability: the drain clock keeps its old meaning |
| (e) Instant fill plus retroactive patch | keep the instant fill, and rewrite its tail when a write lands inside the busy window | cannot undo lines already rendered from the finished fill; must store the fill's parameters (new state); a second source of truth | zero cost for fills nobody touches, which (a) already reaches |

## 3. The renderer

A line renders at its start (the existing model). Catch-up to the line's start instant runs just before
`render_scanline` / `advance_scanline` for lines 0..223. A fill that crosses active display is therefore
seen line by line, with each line showing the fill as it stood when that line began. This is the same
line granularity the FIFO's enqueue-immediate writes and A1's VSRAM latch already accept. The SAT cache
write-through fires per step at the step's own time, through `write_vram_byte`, so it stays ordered with
the renders. The VSRAM read latch and the sprite carry are not touched.

**Hot-path cost.** With no fill running, catch-up is one comparison of the `dma_pending` discriminant per port
access and per line. **Measured** (release; the old and new save-probe binaries alternating five times;
`vcounter.bin`, which runs no fill, 1,200 frames; load 1.60-1.80, up 1 day 44 min): old 0.578-0.606 s
(median 0.591), new 0.584-0.645 s (median 0.596), with identical final hashes. The difference is inside the
old build's own spread, so it is not measured as a cost. With a fill (aeon boot, one 64 KiB fill, 600 frames,
load 2.12, up 1 day 42 min): old 0.464-0.467 s, new 0.471-0.477 s. That is about 1%. Most likely it is the
prototype's per-slot arithmetic over 65,536 steps (an inference: this run does not separate it from the
per-access check); P1 should precompute the slot instants (follow-up F-SLOTTABLE).

**Event backlog.** A long 68k DMA stall can make `pop_due` deliver several line events at a `now` past their
deadlines, after a port access has already caught the fill further. Such a line then sees the fill a
little later than its start. It needs a 68k DMA and a fill running at once, and the population has no such
case.

## 4. Saves and frozen currencies (for a ruling)

| Currency | Moves? | Mechanism and proof |
|---|---|---|
| `export_state` layout (v2, `docs/export-state-v1.md`) | **does not move** | no region is added or resized, and the running bit is not architectural currency (like the FIFO and CD5). *Content* differs only at an instant inside a fill: partial VRAM, and registers 19-22 mid-count. `export_state_v1` 3/0 under the prototype (its fixture runs no fill) |
| `state_hash` (`state_hash.rs`) | **layout does not move**; content moves only mid-fill | it hashes VRAM/CRAM/VSRAM/registers and never covers the running bit, which is the same rule that keeps CD5 and the FIFO out. It sees the fill's effects, never its state. The Oracle-compat byte layout is untouched |
| Save-state layout fingerprint (`oracle_frontend::save_state::layout_fingerprint`) | **does not move** | `dff350afa2eb3e1d` from both binaries (`spike_m1_saves fp`, 140,072-byte probe) |
| Snapshot / bincode | **old → new: loads, same evolution. new → old: mid-fill snapshots refused** | details below |
| Aether wire | **no field changes shape; three values change meaning → contract ruling** | below |
| Visual baselines (scorecard) | **do not move** | 16 of 17 rows byte-identical under the prototype. The 17th, `vdp_port_access`, is a tally row, not a picture |
| Scanline goldens | **do not move** | `scanline_goldens` 5/0 under the prototype |
| aeon replay fixtures | **do not move** | `replay_real_artifacts` 16/0. The population finds no VDP access inside any aeon fill window (§6) |
| Vendored-ROM scorecard | **one row moves** | `vdp_port_access` `…all 22 pages cumulative=119/3/122` → `122/0/122` |
| The 22 printed VDPFIFOTesting tallies | **move from page 5 on** | pages 1-4 unchanged; page 5 (tests 31-33) and every later cumulative tally gain +3 passes, −3 fails. The decoder reproduced all 22 under the prototype |
| `PORT_ACCESS_FAILING` | **moves: empties** | 31, 32, 33 "NOW PASSES", no other test named |

**Snapshots in detail.** Measured with `spike_m1_saves` (baseline binary built at `5f266f6`, prototype at
`9da8784`):
- **Old snapshots in the new build.** Three old snapshots load, all without refusal: `gfx_joystick` taken
  mid-fill at mclk 500,024, `gfx_joystick` taken quiet at 3,000,004, and aeon `s4.debug.bin` taken mid-fill
  at 500,066. After 60 or 120 frames each reaches the old build's own hash: `7be37c9fe0de5249`,
  `7be37c9fe0de5249` and `51b8a46e5d8ffe0b`.
- **New snapshots in the old build.** The old build refuses both new mid-fill snapshots with
  `UnexpectedVariant { type_name: "DmaRequest", allowed: Range { min: 0, max: 2 }, found: 3 }`, and loads the
  new quiet snapshot normally.

**A save taken mid-fill** under the new model contains the fill as far as it has got:
- VRAM partly filled;
- registers 19/20 holding the steps left, and 21/22 advanced by the steps done;
- `addr` at the next step's address;
- CD5 set, `dma_pending = FillRunning`, and `fifo_slot_clock` at the last slot consumed.

Restoring it continues exactly: the same-build round trip reaches the uninterrupted run's hash. An **old
save taken mid-fill** holds a fill that is already complete in memory (the old model), with CD5 clear and
`dma_busy_until` still in the future. The new build reads busy from that window until it closes, then runs
on unchanged.

**The plain answer: no save that loads today refuses, and none loads differently.** A restored old save is
the same bytes, a different model runs it forward, and for every save the corpus can produce that
forward run is the same, as measured. Saves diverge only when their future holds a fill that something
observes before it finishes, and that is the fix.

**The wire, value by value** (no field added, removed or retyped):
- `emulator/read_vdp_registers` `status.raw` bit 1 clears when the last step runs, not at the end of the
  flat-rate window. Bits 9/8 read EMPTY during a fill. The peek is exact on a paused machine because every
  run ends with a catch-up.
- `emulator/read_vram` during a fill returns partial VRAM.
- **Watch hits for fill writes (R1).** `via: "dma"` hits used to carry the trigger's pc and the trigger's
  instant. They would carry each step's own slot instant, and the pc of the instruction whose port access
  (or line) caught the fill up. Measured: `watchpoints::vram_watch_catches_a_dma_fill_write_with_via_dma`
  expects opcode `$32BC` (the trigger) and gets `$3081`, `move.w d1,(a0)`, the poll loop's register write.

That last item is a change of meaning on the wire, and so a contract ruling rather than a design choice.
Two alternatives are available:
- keep attributing to the trigger, by storing its pc, which is new state and moves the layout;
- mark fill hits as engine-attributed, with no pc, which changes the contract shape.

## 5. Which tests move

**Predicted before the prototype ran** (commit `9da8784`'s message), from §1.3 only:

| Test | Group | Tail at `$38`/`$8FF8` on hardware | Ours today | Prototype |
|---|---|---|---|---|
| 31 VRAM | 2 | `5656 5656 0056 0000` | `1212 1212 5678 0000` | matches (0/48 off) |
| 31 VRAM | 3 | `9a9a 9a9a 9a9a 0000` | `1212 1212 bc9a 0000` | matches |
| 32 CRAM | 2 | `0666 0666 0666 0000` | `0444 0444 0ccc 0000` | matches |
| 32 CRAM | 3 | `0888 0888 0888 0000` | `0444 0444 0eee 0000` | matches |
| 33 VSRAM | 2 | `0666 0666 0666 0000` | `0444 0444 04cc 0000` | matches |
| 33 VSRAM | 3 | `0888 0888 0888 0800` | `0c44 0c44 0eee 0800` | matches |

**Switch-point margin** (population instrument, H40 slot table; §1.3's bound is 7 steps). From trigger to
write, 13 slots pass for VRAM and 10 for CRAM/VSRAM. Less the trigger's own FIFO drain (2 slots for a VRAM
word, 1 for CRAM/VSRAM), that is **11** and **9** fill steps: margins of 4 and 2 slots. The write's instant is
the instruction's start (F-SUBLINE-ACCESSMCLK); on hardware it comes about 16 CPU cycles later, which only
widens the margin.

**Must not move, and did not:** the other 119 VDPFIFOTesting tests. The pin reports any count change and
named none. That includes the fill tests 4, 16, 28, 29, 34, 36 and 38 and the eight VSRAM/CRAM/VRAM fill
ranges 72-95. **Moved, each with a named mechanism** (workspace, release, `--no-fail-fast`: 2894 passed / 18
failed / 3 ignored, against the baseline 2912/0/3; the same 2,915 tests):

* **Conformance pins (2):** `vdp_port_access_full_rom_verdicts` (`PORT_ACCESS_FAILING` empties) and
  `testrom_conformance_scorecard` (the tally row). Cause: M1.
* **Unit tests that encode an instant fill (12).** Each needs a `cause:` line and a rewrite that runs the
  clock:
  * `bus::tests`: `vdpfifo_t4_fill_trigger_and_byte_placement`,
    `fill_sets_dma_busy_for_the_coarse_window_but_returns_no_wait`, `fill_updates_the_sat_cache_on_window_hits`
    and `vram_fill_fills_the_target_with_the_top_byte`. They read VRAM or busy in the same instant as the
    trigger.
  * `vdp::tests`: `fill_trigger_is_applied_as_a_normal_word_write`, `vram_fill_writes_the_msb_to_address_xor_one`,
    `fill_adds_only_its_trigger_word_to_the_ring` and
    `a_fill_advances_source_registers_21_22_by_its_length_and_never_carries_into_23`. All four stop in the
    shared helper `arm_and_trigger_vram_fill`, which expects the trigger to hand the bus a
    `DmaRequest::Fill`.
  * Also in `vdp::tests`: `a_fill_whose_code_names_no_write_target_writes_nothing_but_still_runs`,
    `a_fill_that_does_name_a_write_target_is_untouched_by_the_write_decode`,
    `a_fill_reads_dma_busy_from_its_control_write_not_from_its_trigger` and
    `fill_cd5_survives_the_control_write_until_the_data_trigger`. They drive the untimed `data_write`
    path and read the result at once.
* **Integration tests on synthetic fixtures (4):**
  * `oracle-core` `watchpoints::vram_watch_catches_a_dma_fill_write_with_via_dma`: the attribution change
    of R1. `testrom::build_pad_poll` never waits for its fill, and its poll loop's register writes catch
    the fill up.
  * `oracle-core` `scanline_capture::the_row_a_mid_line_cram_write_lands_on_is_the_row_that_splits`
    (17 transitions on line 50 instead of 1), `oracle-aether`
    `scanlines::a2_two_timings_differ_and_the_boundary_moves` (line 50 closes on `000000`, not `FFFFFF`),
    and `oracle-frontend` `tests::the_presented_frame_carries_mid_frame_palette_changes` (the post-hoc
    render has 2 colours, not 1). `testrom::build_cram_midframe` and the frontend's
    `build_midframe_cram_rom` write a non-DMA command word with DMA enabled while their 64 KiB fill is
    still running. That clears live CD5, the fill stops (§1.6), and VRAM keeps its power-on bytes, so the
    "whole screen is backdrop" premise fails.

    These fixtures were never valid on hardware: the MegaDrive Wiki's "only the VDP status register and H/V
    counter are read" rule forbids exactly this. The fix is a busy-poll after each fill (R4). A fixture
    change, not a model change.

## 6. Who else changes behaviour (population, measured)

The spike is an observer-only instrument (`c60d3b0`), release profile, and every suite passes with it armed,
so it is inert. It writes one row per fill or copy. Each row records the window today's model opens
(`dma_cost`) and the one the proposed model would take: pending FIFO entries drain first, then one external
slot per step, on the published slot positions and the flat blanked rate. Counted inside the proposed
window: every VDP port access, every DMA started, and every active line rendered.

| Corpus | Fills | Accesses inside a proposed fill window | Verdict risk |
|---|---|---|---|
| aeon replay fixtures (`replay_real_artifacts`, 16 tests) | 10, all 64 KiB display-off clears | none: no port access, no DMA, no displayed line. Windows 1,093,302 mclk against today's 1,093,315 | none |
| gfx_joystick, m68k_opcode_sizes, shadow_highlight, vdp_test_register, window_test | 1 each, 64 KiB display-off at boot | none | none |
| m68k_bcd | 2, 64 KiB display-off | 5,578 status polls; **no poll reads a different busy bit** | none |
| io_sample | 313 (308 of 462 bytes) | 19,289 status polls; **no poll reads a different busy bit** | none |
| VDPFIFOTesting | 73 fills (+34 copies) | **6 fills see mid-fill data writes (9 writes: tests 31/32/33)**; no data read, control write or DMA inside any fill window; 50 fills have polls whose busy bit differs; 34 overlap displayed lines | only 31/32/33 (confirmed by the prototype) |
| golden_frames, determinism_gate, export_state_v1 | 0 (no fill through the bus) | — | none |
| **not in the instrument's run:** `testrom::build_pad_poll`, `build_cram_midframe`, frontend `build_midframe_cram_rom` | a 64 KiB fill each | port accesses inside the window (found by the prototype, §5) | 4 tests (§5) |

The other vendored ROMs run no fill at all (color_1536, cram_flicker, direct_color_dma, fm_test,
m68k_illegal, m68k_memory_test, vcounter, vdp_sprite_masking, window_distortion).

**What else could produce "unmoved", and how each was ruled out.** A corpus with no mid-fill access, or
fills too short to cross a line: the instrument counted accesses and lines directly, it saw exactly the six
fills the tables predict in VDPFIFOTesting, and it sees 34 fills crossing displayed lines. A golden that
excludes the field: the prototype ran every golden and every replay test, not just the population. An
instrument that cannot move: its first run did move, on two artifacts that were then fixed (the stale
trigger clock, and records outliving their machine). Both are recorded in `c60d3b0`'s message. **No replay
verdict is at risk.** **TAG: none of the corpus needs a live look for this mechanism.** The live-look items
are the documentation gaps in §1.5/§1.6.

## 7. Staging

| Parcel | What it moves | Needs a ruling |
|---|---|---|
| **P1 FILL-RUN** (the model, as prototyped, plus the door check and F-SLOTTABLE-style precomputed slot instants). Rewrites the 12 unit tests with `cause:` lines, adds a busy-poll to the 3 synthetic fixtures, empties `PORT_ACCESS_FAILING`, and updates the scorecard row and `docs/2026-09-12-vdp-port-access-full-rom.md`/`docs/2026-07-25-testrom-conformance.md` | bytes: VDPFIFOTesting's results and any mid-fill instant. **Save layout: none** (fingerprint measured unchanged). export_state/state_hash layout: none. Frozen goldens: none except the pins. Wire: three values (§4) | **yes:** R1 (watch-hit attribution and mclk, wire-visible), R2 (C-6 for fills: one stamp per step), R3 (old builds refuse new mid-fill saves), R4 (the three fixtures change) |
| P2 FILL-FASTPATH (optional): O(1) catch-up per blanked line | nothing: must be byte-identical to P1 across every currency | no |
| P3 COPY-OVER-TIME (own design): the same scheme with `CopyRunning` | poll timing in 31 VDPFIFOTesting copies; no known table | yes: its own mid-copy toggle question (§1.7) |
| P4 per-line 68k-DMA cost / F-DMAHALT (untouched by P1) | DMA elapsed time for every DMA-using ROM | yes (as already booked) |

**P1 moves no save layout**, and that is measured, not argued: the fingerprint is the same bytes, and old
saves load and evolve identically. P1 changes fill elapsed time and nothing else about timing. Display-off
fills end within 2-15 mclk of today's window, and fills that cross active display end at the per-slot
rate instead of the start instant's flat rate (test 34 group 4: 5.04 M mclk against 12.45 M).

## Open items and TAGs

* **TAG (live look), §1.5:** the FIFO EMPTY/FULL bits during a running fill.
* **TAG (live look), §1.6:** a non-DMA command word mid-fill: does the fill stop (the model, from Nemesis's
  live-CD5 sentence) or continue? Also a data-port read mid-fill.
* **Documentation disagreement, §1.1:** the per-line fill deficit (17/204 in the wiki's table against Eke's
  "first line only"). Not picked.
* **R1-R4** above go to the hub; R1 and R3 are wire- or save-facing and may go on to the owner.
* The run-end catch-up must cover every path that hands a paused machine to a debugger, not only
  `run_until_with_sink`. P1 should enumerate these by symbol.

## Measurement record

| Spike | Commit | What it ran | Result |
|---|---|---|---|
| population instrument | `03b00a8`, fixed `c60d3b0` | `FILLSPIKE_OUT=… cargo test --release` over conformance_roms, scanline_goldens, golden_frames, determinism_gate, export_state_v1, oracle-replay replay_real_artifacts | all green armed; §6 |
| save probe | `5f266f6` | `spike_m1_saves fp / save / load`, old and new binaries | §4 |
| prototype | `9da8784` | `cargo test --release --workspace --no-fail-fast`; conformance with `--nocapture` | §5 (load 2.87 at start, up 1 day 34 min) |
| null-path timing | old `5f266f6`, new `9da8784` binaries | §3 | §3 |

All spike code was removed from the tip in the final `spike:` commit; `git diff --stat 14cb815...<tip>`
names docs only.
