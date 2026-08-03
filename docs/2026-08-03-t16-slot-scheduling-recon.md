# T16 — discrete per-line access-slot scheduling (VDPFIFOTesting test 16 "FIFO Wait States")

**Status:** design / recon. No implementation code written, no source file touched. Written for the
owner's go/defer/stage decision on the last open item of
`docs/plans/2026-08-03-fifo-scanline-arcs.md` (Arc A) and the residual named in
`docs/plans/2026-08-03-PARKED-owner-ruling.md:152-155`.

---

## Verdict up front

**Three headline findings, in order of decision weight.**

1. **The currency answer is "no movement", and it is measured, not argued.** I built a working
   prototype of the whole fix in a throwaway copy of the tree and ran every gate. **T16 goes from 62/80
   verdict bytes to 80/80 — a full pass — and `export_state_v1::GOLDEN_HASH`, all three
   `oracle_differential` hashes, all six `golden_frames` scenes, `determinism_gate`,
   `singlestep_m68000` (113 tests) and **every** `VISUAL-BASELINE frame_hash=` row stay byte-identical
   with their existing constants.** The only movement anywhere is the `vdp_port_access` scorecard row
   itself (`12/4/16 → 13/3/16`) and exactly **one** unit test —
   `vdp::tests::mem_dma_ring_store_does_not_add_pending_entries`, which exists precisely to guard the
   A3a decision this work reverses. Evidence in §4.

2. **"A genuinely larger piece of work" overstates it, and the plan doc conflates two different
   problems.** T16's remaining 18 red bytes have **two independent causes**, and neither of them is the
   long-standing "Phase 3 per-line DMA cost" deferral:
   * groups 2/3/5/6/8 need **intra-line slot positions** (where in the line the external access slots
     sit) — the prototype is **45 lines** of table-driven integer code in `vdp.rs`;
   * groups 9/10 need **post-DMA FIFO occupancy** (design question Q1 from
     `docs/2026-08-03-a3-dma-fifo-design.md` §6) — the prototype is **two lines**.

   The actual "per-line DMA cost" deferral — `dma_cost` sampling one slot rate at the transfer's start
   instant instead of integrating across the lines the transfer spans (`vdp.rs:785-790`,
   `bus.rs:685-686`) — **is not needed for T16 at all** and should stay deferred. See §3.4.

3. **The choice is not "fudge vs. principle" — it is "which fix, at what price".** A crude uniform
   perturbation (`entry_drain_cost += 80` mclk) *also* turns groups 2/3/5/6/8 green — and it **moves two
   visual baselines** (`m68k_opcode_sizes`, `window_distortion`). The principled slot table produces the
   same T16 improvement and moves nothing. That contrast is the strongest argument for doing this
   properly rather than not at all: the correct model is measurably *cheaper* in currency than the
   arbitrary one.

**Recommendation: GO, staged into two slices (S1 intra-line slots, S2 post-DMA occupancy), with the
genuine per-line DMA-cost integration explicitly left deferred as S3.** Both S1 and S2 are
currency-neutral *as measured on today's fixtures*; §4.4 is honest about why that is an empirical
property and not a proof, and §5.3 says what the implementer must re-run to keep it true.

---

## 1. Method and evidence

### 1.1 How the evidence was gathered

Four independent channels, in increasing order of directness. All four agree.

* **The ROM's own embedded expected-value table.** `vendor/TestRoms/vdp_port_access.bin` (Nemesis,
  *VDPFIFOTesting*, 524288 bytes), disassembled with `capstone` (`CS_ARCH_M68K` /
  `CS_MODE_M68K_000`) over the flat image (ROM address == file offset for a Mega Drive cartridge),
  driven from python in the scratchpad. Same method as slices A0/A2/A3.
* **A direct read-out of what we produce today**, by booting the ROM through the real `System` in a
  throwaway crate outside the repo and scanning work RAM for the test's result record. This yields the
  40 expected words and the 40 actual words side by side.
* **A `BusEventSink` timing trace** (`on_event_at`, the SY-4a seam) recording every `$C00000-$C0000F`
  access with its mclk, which turns "the probe lands just past a drain boundary" from a hypothesis into
  a measured number.
* **A working prototype** of the proposed model in a `tar`-copy of the tree in the scratchpad, run
  against the full test suite. The repo itself was never modified (`git status --porcelain` empty
  throughout).

Public documentation was used only for the hardware slot pattern (§2). **No emulator source was read;
`/home/volence/sonic_hacks/oracle/` was never opened.**

### 1.2 Where test 16's table is, and why the address is authoritative

The record builder at ROM `$ED60` states its own field sizes, and loads both tables with literal
`lea`s — this is the ground truth, not a guess:

```
00ED60: 3e3c0024       move.w #$24, d7          ; name length = 36
00ED64: 30fc0004       move.w #$4, (a0)+        ; record type
00ED68: 30c7           move.w d7, (a0)+
00ED6A: 3c3c0050       move.w #$50, d6          ; expected-table length = 80 bytes = 40 words
00ED6E: 30c6           move.w d6, (a0)+
00ED70: 43f90000ecec   lea.l  $ecec.l, a1       ; the 36-byte name string
00ED7A: 10d9           move.b (a1)+, (a0)+
00ED80: 30fc0001       move.w #$1, (a0)+
00ED84: 43f90000ed10   lea.l  $ed10.l, a1       ; the 80-byte expected table
00ED8E: 10d9           move.b (a1)+, (a0)+
```

`$ECEC` = `"FIFO Wait States"` space-padded to 36; the table therefore starts 36 bytes later at `$ED10`,
and is **80 bytes — 40 words, not 32** (T16 is the only test in this ROM with an oversized table, which
is why the ledger counts it in *bytes*: "62/80"). Verbatim:

```
ROM $ED10 (test 16, 40 words = 10 groups × 4):
  group  1  0100 0100 0000 0200      group  6  0100 0100 0000 0200
  group  2  0100 0100 0000 0200      group  7  0100 ffff ffff 0200
  group  3  0100 0100 0000 0200      group  8  0100 0100 0000 0200
  group  4  0100 0100 0000 0200      group  9  0200 0100 0000 0200
  group  5  0100 0100 0000 0200      group 10  0000 0100 0000 0200
```

These are hardware answers captured by the ROM's author on real silicon. They are the spec.

### 1.3 What the four words in a group actually mean

Disassembled `$ED94`…`$FC76` (`rts`). Every group is the same 60-instruction machine, and decoding it
is the load-bearing step — without it the table is unreadable.

Per group, the ROM keeps `d0..d3` initialised to `$FFFF` ("never observed"), a found-flags mask in `d7`,
and a retry counter `d6 = $800` (**2048 retries**). Each retry:

1. runs the group's *stimulus* (see §1.4);
2. `move.w $C00004, d5` — **probe 1**, a status read taken into a register;
3. runs the group's *inserted operation* — the one thing that differs between groups;
4. `move.w $C00004, d4` — **probe 2**, also into a register (kept out of memory so the read is as
   tight as possible);
5. 15 × `move.w $C00004, (a1)+` — a **stream** of further status reads into the buffer;
6. `move.w d4, (a1)+` — probe 2 is appended **last**, so the buffer holds
   `[read3 … read17, read2]`.

Then it classifies. Every status is masked with `andi.w #$302` (bit 9 = FIFO EMPTY `$200`,
bit 8 = FIFO FULL `$100`, bit 1 = DMA busy `$2`):

| register | what it records |
|---|---|
| `d0` | **probe 1**, re-taken every retry until the group's latch condition fires (`btst #8` = "saw FULL" for groups 1-8 at `$EE60`; `btst #9` = "saw EMPTY" for group 9 at `$FA34`; `btst #9` inverted = "saw NOT-empty" for group 10 at `$FBCC`) |
| `d1` | the **first FULL** seen anywhere in the 16-word stream (`$EEBC`) |
| `d2` | the **first PARTIAL** (`$0000`/`$0002`) seen in the stream (`$EEAA`) |
| `d3` | the **first EMPTY** seen in the stream (`$EECE`) |

`cmpi.w #$f,d7 / beq` at `$EEEE` stops retrying once all four have been found; otherwise it retries up
to 2048 times. **`$FFFF` in the expected table therefore means "hardware never observed that FIFO state
in 2048 retries".**

Two consequences matter enormously:

* **Probe 1 is deliberately excluded from the FULL search.** The test is not asking "does the FIFO
  report FULL after six writes" (that is what probe 1 checks, and we already pass it in all 10 groups).
  It is asking **"is the FIFO *still* FULL one operation later"** — a pure timing question.
* **The 2048 retries sweep the frame.** On hardware each retry begins at a different h/v position, so
  the stimulus lands at a different phase of the access-slot pattern. The test is explicitly designed to
  give a *sometimes*-observable state 2048 chances to be seen.

### 1.4 The ten groups, decoded

Groups 1-8 share a stimulus: `move.l #$40000002,$C00004` (VRAM write @ `$8000`) then **six**
`move.w #$FFFF,$C00000` data-port writes into a 4-deep FIFO. They differ only in the operation inserted
between probe 1 and probe 2:

| # | ROM | inserted operation | expected `d1` (stream FULL) |
|---|---|---|---|
| 1 | `$EDAC` | *(nothing)* | `0100` |
| 2 | `$EF1A` | `move.w #$C000,$C00004` — one control word (CD1-0 ← CRAM write) | `0100` |
| 3 | `$F090` | `move.l #$40000012,$C00004` — full control pair, VSRAM write @ `$8000` | `0100` |
| 4 | `$F208` | the same control pair **+ two more data writes** | `0100` |
| 5 | `$F390` | `move.w #$0000,$C00004` — one control word | `0100` |
| 6 | `$F506` | `move.l #$00000002,$C00004` — full control pair, VRAM **read** @ `$8000` | `0100` |
| 7 | `$F67E` | that pair **+ `move.w $C00000,d4`, a data-port READ** | **`ffff`** |
| 8 | `$F7FC` | `move.l #$00008F01,$C00004` — control word `$0000` then **register 15 = 1** | `0100` |

Group 7 is the control that proves the semantics: a data-port read must wait for the write FIFO to
drain (Nemesis, VDP Internals: *"pending FIFO writes take priority"*), so after it the FIFO is empty and
neither FULL nor PARTIAL is ever seen — expected `0100 ffff ffff 0200`. We reproduce that exactly today.

Groups 9 and 10 are a different shape — they are about **DMA**:

* **Group 9** (`$F99E`, prologue at `$F966` sets reg 1 = `$54` display-on + DMA-enable, regs 20/22/23 =
  0): probe 1 on an **empty** FIFO (expected `0200`), then `reg 19 = 8` / `reg 21 = 0` and a
  `move.l #$40000082,(a0)` / `move.l (a0),$C00004` that fires an **8-word 68k→VRAM DMA**, then probe 2 +
  the stream. Expected `0200 0100 0000 0200`: after the DMA the CPU must see **FULL, then partial, then
  empty**.
* **Group 10** (`$FB04`): three data writes (probe 1 expects `0000` = partial), reg 1 = `$54`, then a
  **3-word DMA**. Expected `0000 0100 0000 0200` — same requirement.

Both say the same thing: **when a 68k→VDP DMA ends, the FIFO is still holding undrained words, and the
resuming CPU can see that.**

### 1.5 What we produce today — measured, word by word

Booted `vendor/TestRoms/vdp_port_access.bin` through `System` on the harness's own schedule (seed
`0x1234_5678`; 60 frames, `Start` for 5, 535 more — `conformance_roms.rs:425-456`) and read the record
out of work RAM at offset `$00644`:

```
grp | expected             | actual               | red
  1 | 0100 0100 0000 0200  | 0100 0100 0000 0200  |
  2 | 0100 0100 0000 0200  | 0100 ffff 0000 0200  |  X
  3 | 0100 0100 0000 0200  | 0100 ffff 0000 0200  |  X
  4 | 0100 0100 0000 0200  | 0100 0100 0000 0200  |
  5 | 0100 0100 0000 0200  | 0100 ffff 0000 0200  |  X
  6 | 0100 0100 0000 0200  | 0100 ffff 0000 0200  |  X
  7 | 0100 ffff ffff 0200  | 0100 ffff ffff 0200  |
  8 | 0100 0100 0000 0200  | 0100 ffff 0000 0200  |  X
  9 | 0200 0100 0000 0200  | 0200 ffff ffff 0200  |  XX
 10 | 0000 0100 0000 0200  | 0000 ffff ffff 0200  |  XX
red bytes = 18 / 80   (green = 62)
```

9 red words = 18 red bytes = **62/80 green**, reproducing the ledger's aggregate exactly
(`docs/2026-07-25-testrom-conformance.md:35`). Every single red word is a `$FFFF` — we never produce a
*wrong* FIFO state, we only fail to produce a *transient* one. That matters: this is a pure
observability/timing gap, not a semantic error.

The failure splits cleanly:

* **Groups 2/3/5/6/8 — one red word each (`d1`).** The FIFO is no longer FULL by probe 2. `d2`
  (partial) and `d3` (empty) are both found, so the drain *rate* is in the right ballpark; only the
  *first* transition is mistimed.
* **Groups 9/10 — two red words each (`d1`, `d2`).** Neither FULL nor PARTIAL is ever observed after a
  DMA. Our DMA leaves the FIFO completely empty.

### 1.6 The measured mechanism for groups 2/3/5/6/8

The `BusEventSink` trace of one group-2 retry, mclk relative to the first data write (status values are
the raw port reads; `$3180` = FULL, `$3080` = partial, `$3280` = empty):

```
+    0  Write C00000 = FFFF   data write #1
+  140  ...#2      +  280 ...#3      +  420 ...#4      +  560 ...#5
+  700  Write C00000 = FFFF   data write #6
+  903  Read  C00004 = 3180   PROBE 1  -> FULL
+ 1015  Write C00004 = C000   the inserted control word
+ 1155  Read  C00004 = 3080   PROBE 2  -> partial          <-- the failure
+ 1267  Read  C00004 = 3080   stream[0]   ... partial through stream[7] (+2247)
+ 2387  Read  C00004 = 3280   stream[8]   ... empty through stream[14]
```

Reconstructing the model from `vdp.rs:486-509` against those timestamps (H40 active display: a VRAM
word costs `2 × 3420/18` = **380 mclk**):

| event | model state |
|---|---|
| W1 @ 0 | `fifo_slot_clock = 0`, `fifo_len = 1` |
| W4 @ 420 | drain at 380 fires → `fifo_slot_clock = 380`, then enqueue → `len = 3` |
| W6 @ 700 | `len == 4` → **/DTACK stall**: next drain is at `380+380 = 760`, so `wait = 60` mclk → `div_ceil(7)` = **9 CPU cycles**; `fifo_slot_clock = 760`; enqueue → `len = 4` |
| probe 1 @ 903 | next drain 1140 > 903 → `len = 4` → **FULL** ✓ |
| probe 2 @ 1155 | 1140 ≤ 1155 → one entry pops → `len = 3` → **partial** ✗ |

The reconstruction is confirmed by the trace itself: the W6→probe-1 gap is 203 mclk = 29 CPU cycles =
20 (`MOVE.W #imm,(xxx).L`) + the 9 predicted stall cycles. The other measured groups:

| group | inserted operation | probe 2 at | drain boundary | **missed by** |
|---|---|---|---|---|
| 1 | none | +1015 | +1140 | *(catches it)* |
| 2, 5 | one control word (20 cycles) | +1155 | +1140 | **15 mclk** |
| 3, 6, 8 | control long / register write (28 cycles) | +1211 | +1140 | **71 mclk** |
| 4 | control long + 2 data writes | re-stalls, re-fills | — | *(catches it)* |
| 7 | data-port read | +2394 | drained by the read | *(correctly `ffff`)* |

**15 mclk out of a 380-mclk period — 2.1 CPU cycles.** And it is not a coin flip. Histogramming every
group-2 retry the ROM performs:

```
(probe1, probe2, gap)          count
("FULL",  "part",  252 mclk)   1815     <-- every single active-display retry
("EMPTY", "EMPTY", 252 mclk)    229     <-- retries that began on a blanked line; the ROM's
("part",  "part",  252 mclk)      3         latch rule discards these
```

**1815 out of 1815 active-display retries produce the identical miss.** That is the mechanism the
ledger names, confirmed: the write-6 /DTACK stall **phase-locks the CPU to the drain clock**. Whatever
h/v position a retry starts at, it resumes exactly at a drain instant, and because our slot spacing is
uniform the probe then lands at a fixed offset past the *next* drain instant, every time. The 2048
retries buy hardware 2048 different slot phases; they buy us 2048 copies of the same phase.

### 1.7 The measured mechanism for groups 9/10

From the same trace, a group-9 retry:

```
+   0  Write C00004 = 9308   reg 19 = 8  (DMA length)
+ ...  Write C00004 = 9500   reg 21 = 0
+ 420  Write C00006 = 0082   CD5 -> fires the 8-word 68k->VRAM DMA
+3241  Read  C00004 = 3280   first status read after the DMA  -> EMPTY
+3353  Read  C00004 = 3280   -> EMPTY
+3493  Read  C00004 = 3280   -> EMPTY
```

`bus.rs:669-694` runs the whole transfer synchronously inside the triggering access and bills
`8 words × 2 slots × 190 = 3040` mclk as one CPU halt. So the 68k does not regain the bus until the
transfer is entirely finished, at which point `fifo_len` is 0 (A3a deliberately used `fifo_store`, not
`fifo_enqueue` — `vdp.rs:763-783`). **There is no instant at which the CPU could observe an intermediate
FIFO state.** Slot scheduling cannot fix this; it is a different defect.

Hardware behaves differently because the DMA unit's job ends when the last word is *pushed into the
FIFO*, not when it reaches VRAM. Nemesis, *VDP Internals*:

> "it will read a value from external memory using the DMA source address register and **add it to the
> FIFO** using the current command code and incremented command address registers"

So the transfer completes with up to four words still queued, and the resuming CPU sees FULL → partial →
empty — exactly the `0100 0000 0200` the table demands. This is precisely design question **Q1** in
`docs/2026-08-03-a3-dma-fifo-design.md` §6, which predicted this interaction and asked to have it
revisited here rather than papered over. **The prediction was correct.**

---

## 2. What a "discrete per-line access-slot" model actually is

### 2.1 The hardware pattern

The VDP performs a fixed number of memory accesses per scanline and hands a fixed subset of them to
external masters (CPU FIFO writes, DMA). Every claim below is quoted and attributed; where sources
conflict, the conflict is stated rather than resolved.

**Accesses per line.** Kabuto's hardware notes:

> "The VDP does 210 accesses (171 in H32 mode) per line, one every 2 cycles. Each access is 32 bits
> wide."

and on the line length, in both H32 and H40 normal modes: *"3420 master clock ticks per line"*.
Corroborated by Eke: *"3420 MCLK = 840 EDCLK per line, i.e 420 pixels and 210 access slots."*

**Why a VRAM word costs two slots.** Nemesis: *"Within each 4 SC read cycle, 4 bytes are read … Within
each 4 SC write cycle, 1 byte is written"* — VRAM writes are byte-wide, so one FIFO word needs two
slots. This is already our model (`vdp.rs:486-492`).

**External slots during active display: 16 (H32) / 18 (H40).** The Sega *Genesis Technical Overview*
DMA capacity table gives Memory→VRAM 16 (H32) / 18 (H40) bytes per line during effective display;
`docs/2026-07-16-vdp-recon.md:109` already pins these, and `vdp.rs:474-482` already implements them.

**Where the slots are.** This is what we do **not** model, and Kabuto publishes it as a per-line
access-pattern string:

> `H40: Hssss AsaaBsbb ((A~aaBSbb)*3 AraaBSbb)*5 ~~ s*23 ~ s*11`
> `H32: Hssss AsaaBsbb ((A~aaBSbb)*3 AraaBSbb)*4 ~~ s*13 ~ s*13 ~`

with `H` = hscroll, `A`/`B` = tilemap, `a`/`b` = tile pixels, `S` = sprite refs, `s` = sprite pixels,
`~` = **external (CPU/DMA) slot**, `r` = **VRAM refresh**. Expanding them (arithmetic, not a quote):
H40 = 210 accesses containing exactly **18 `~` and 5 `r`**; H32 = 171 accesses containing exactly
**16 `~` and 4 `r`**. That reproduces the manual's counts from an independent source — a strong check
that the strings are faithful.

**The slots are irregularly spaced, and the manual is wrong about it.** This is the crux. TascoDLX
notes the manual's 4.77 µs maximum-wait figure was computed assuming *"a maximum gap of 16 slots"*, and
that *"according to the new data, the largest gap is actually 26 slots, and half of that is during
H-sync with the overall slower clock."* Nemesis, who took the measurements with a logic analyser on the
VRAM bus: *"I'd say the manual is simply incorrect. My measurements from the VRAM bus are pretty
definitive."*

Reading the H40 gap sequence straight off Kabuto's string (external-slot access indices 14, 22, 30, 46,
54, 62, 78, 86, 94, 110, 118, 126, 142, 150, 158, 173, 174, 198 within the 210-access line) gives gaps of

```
8, 8, 16 | 8, 8, 16 | 8, 8, 16 | 8, 8, 16 | 8, 8, 15 | 1 | 24 | 26   (sums to 210)
```

— a repeating render group of two close slots and one wide one, then a *pair of adjacent slots* one
access apart, then two very wide gaps straddling hblank. The wrap-around gap of **26** matches
TascoDLX's measured figure exactly, which is independent corroboration that Kabuto's string transcribes
Nemesis's measurements.

**Blanked lines: 167 (H32) / 205 (H40) — with a live disagreement.** The Sega manual's table gives
167/205 during V-blank, corroborated by TascoDLX (*"on V-blank lines it's all external access slots
except for the usual refresh slots (H32: 167+4=171 ; H40: 205+5=210)"*), which Nemesis confirmed. But
Mask of Destiny reports a *measured* extra refresh slot when the display is off:

> "In H40 mode, there are 6 refresh slots when the display is off. There is one refresh slot every 32
> slots starting at slot 37"

giving 204, not 205, and analogously 166 for H32 (whose display-off refresh positions he says he never
determined). **No source adjudicates this.** §4.3 measures that, for our fixtures, the choice is
currency-free either way.

**FIFO wait states.** Mask of Destiny: *"!DTACK is held high until there is room in the FIFO to store
the word being written"*, *"If the FIFO is not full, there is no delay"*, and — the piece we do not
model — *"There's also some latency involved in the FIFO, so there's a delay (2 or 3 slots IIRC)
between when a word gets written to the FIFO and when the first byte gets written to VRAM even when the
display is off."* Note the hedge; this figure is **not** authoritatively pinned anywhere I found.

Sources:
[Nemesis / TascoDLX / Eke, VRAM access timing (t=851)](https://gendev.spritesmind.net/forum/viewtopic.php?t=851) ·
[VDP Internals (t=1291) p.3](https://gendev.spritesmind.net/forum/viewtopic.php?t=1291&start=30) ·
[same, p.4](https://gendev.spritesmind.net/forum/viewtopic.php?t=1291&start=43) ·
[Mask of Destiny, DMA and FIFO (t=2436)](http://gendev.spritesmind.net/forum/viewtopic.php?t=2436) ·
[Kabuto's hardware notes (Plutiedev mirror)](https://plutiedev.com/mirror/kabuto-hardware-notes) ·
[Sega Genesis Technical Overview (1991)](https://segaretro.org/images/1/18/GenesisTechnicalOverview.pdf) ·
[MegaDrive Wiki — VDP](https://md.railgun.works/index.php?title=VDP)

### 2.2 How far today's model is from that

`vdp.rs:472-509` is an **aggregate-rate** model, not a schedule:

```rust
fn slots_per_line(&self, mclk: u64) -> u64 { /* 16 / 18 / 167 / 205 */ }

fn entry_drain_cost(&self, code: u8, at: u64) -> u64 {
    let slots = match Self::target_of(code) { Target::Vram => 2, _ => 1 };
    slots * MCLK_PER_LINE / self.slots_per_line(at)
}
```

It gets the *count* of slots per line exactly right and their *positions* uniformly wrong. In H40 active
display it says every slot is 190 mclk from the last (`3420/18`); hardware's gaps, converted at
`3420/210 ≈ 16.3` mclk per access, are roughly **130, 130, 260 … 260, 16, 391, 423 mclk**. A VRAM word
(two slots) therefore drains in anywhere from ~260 mclk to ~814 mclk depending on where in the line it
lands, against our invariant 380.

That variation is the entire content of groups 2/3/5/6/8: 15 mclk of slack is nothing next to a
±430 mclk spread.

Three things follow, and they are worth stating separately because the plan doc runs them together:

* **(A) Intra-line slot positions.** Missing. This is what T16 groups 2/3/5/6/8 need.
* **(B) Post-DMA FIFO occupancy.** Missing (deliberately, A3a §3.3). This is what T16 groups 9/10 need.
  It is **not** slot scheduling.
* **(C) Per-line cost integration.** `dma_cost` (`vdp.rs:785-790`) samples `slots_per_line` **once**, at
  the transfer's start instant, and bills the whole transfer at that rate. A DMA that starts in vblank
  and runs into active display is mis-billed by up to ~11×. This is the literal "Phase 3 per-line DMA
  cost" deferral — `docs/plans/2026-07-16-vdp-dma-fifo.md:205-207` specified the integration and the
  shipped code does not do it. **T16 does not need it.**

---

## 3. Design

### 3.1 S1 — intra-line slot positions (fixes groups 2/3/5/6/8)

Replace the uniform division inside `entry_drain_cost` with a lookup into the published slot table, on
active lines only. Integer arithmetic throughout; no new struct fields, no new public API, no
serialized state.

```rust
fn entry_drain_cost(&self, code: u8, at: u64) -> u64 {
    let slots = match Self::target_of(code) { Target::Vram => 2, _ => 1 };
    // Blanked lines keep the aggregate-rate model (see Q3): every access is a slot bar refresh,
    // so positions carry almost no information there.
    if self.vblank(at) || !self.display_enabled() {
        return slots * MCLK_PER_LINE / self.slots_per_line(at);
    }
    let mut t = at;
    for _ in 0..slots {
        t = self.next_active_slot(t);
    }
    t - at
}

/// mclk of the first external access slot strictly after `at`, on an active display line.
/// Indices are the `~` positions in Kabuto's published per-line access pattern:
///   H40: `Hssss AsaaBsbb ((A~aaBSbb)*3 AraaBSbb)*5 ~~ s*23 ~ s*11`   (210 accesses, 18 external)
///   H32: `Hssss AsaaBsbb ((A~aaBSbb)*3 AraaBSbb)*4 ~~ s*13 ~ s*13 ~` (171 accesses, 16 external)
fn next_active_slot(&self, at: u64) -> u64 {
    const H40_SLOTS: [u64; 18] =
        [14, 22, 30, 46, 54, 62, 78, 86, 94, 110, 118, 126, 142, 150, 158, 173, 174, 198];
    const H32_SLOTS: [u64; 16] =
        [14, 22, 30, 46, 54, 62, 78, 86, 94, 110, 118, 126, 141, 142, 156, 170];
    let (idx, total): (&[u64], u64) =
        if self.h40() { (&H40_SLOTS, 210) } else { (&H32_SLOTS, 171) };
    let line = at / MCLK_PER_LINE;
    let pos  = at % MCLK_PER_LINE;
    for &k in idx {
        let t = k * MCLK_PER_LINE / total;
        if t > pos { return line * MCLK_PER_LINE + t; }
    }
    (line + 1) * MCLK_PER_LINE + idx[0] * MCLK_PER_LINE / total
}
```

Callers are unchanged: `fifo_drain` (`vdp.rs:496-509`), `data_write_at` (`vdp.rs:942-960`) and
`data_read_at` (`vdp.rs:994-1010`) already treat `entry_drain_cost` as an opaque "when does the oldest
entry leave", so the /DTACK stall and the read-waits-for-drain rule automatically inherit the real
schedule. That is the whole reason this is cheap.

**Measured effect:** T16 62/80 → **72/80** (groups 2/3/5/6/8 all green). The scorecard row does *not*
move, because T16 is still FAIL on groups 9/10.

### 3.2 S2 — post-DMA FIFO occupancy (fixes groups 9/10; resolves Q1)

```rust
// vdp.rs, dma_write_word (~777): the payload word occupies a PENDING slot, not just a ring slot.
self.fifo_store(w);
self.fifo_len = (self.fifo_len + 1).min(4);

// vdp.rs, dma_complete (~795): the residual entries drain from the end of the transfer, so the
// resuming 68k sees FULL -> partial -> EMPTY exactly as the ROM's table demands.
if self.fifo_len > 0 { self.fifo_slot_clock = busy_until; }
```

Two lines. `bus.rs:669-694` (`run_mem_dma`) needs no edit: it already computes `now + cost` and passes
it to `dma_complete`, so `busy_until` is exactly the transfer's end instant.

**Measured effect (on top of S1):** T16 72/80 → **80/80**, a full pass. Scorecard `vdp_port_access`
`12/4/16 → 13/3/16` (page 1 stays `7/2/9`; page 2 gains T16).

This directly reverses A3a's deliberate choice, whose stated reason was: *"counting the payload as
pending would leave phantom entries no clock has advanced past — a spurious /DTACK stall on the next
data-port write in every DMA-using ROM"* (`vdp.rs:771-776`). The `fifo_slot_clock = busy_until` line is
what makes that reason no longer apply — the entries are *not* phantom, they are the four words that
genuinely have not reached VRAM yet, and the clock is anchored at the transfer's end so the stall they
produce is the real one. A3a was right to defer it and right to name it Q1; the answer is now evidenced.

### 3.3 What S1 + S2 do **not** change

Ordering (Decision 1 / apply-at-enqueue survives untouched); FIFO *contents* and the snoop; CD5
semantics and the DR-2 clear; `run_fill` / `run_copy`; the CRAM/VSRAM fill data source; VRAM byte
placement; every rendering path. No new serialized field, so the bincode round-trip obligation is
trivially met, and neither `state_hash` nor `export_state` gains a member.

### 3.4 S3 — the real "per-line DMA cost", explicitly NOT proposed here

`dma_cost` (`vdp.rs:785-790`) and `run_mem_dma` (`bus.rs:685-686`) should eventually integrate the slot
budget across the lines a transfer spans, and blanked lines should get slot positions too. **Neither is
required for T16**, both change DMA elapsed time for every DMA-using ROM, and that is where the visual
baselines actually live. Keeping S3 out of this arc is the single most important scoping decision in
this document.

---

## 4. Currency risk — measured, not argued

This is the question the brief flagged as most decision-relevant, so it was answered by experiment
rather than by reasoning. Method: `tar`-copy of the tree (excluding `target/`, `.git/`, with `vendor`
symlinked) into the scratchpad, patch the copy, run the gates. **The repo was never modified**
(`git status --porcelain` empty before, during and after).

### 4.1 Baseline

All five currency suites green in the unmodified copy, with their existing constants.

### 4.2 The full prototype (S1 + S2)

| gate | result |
|---|---|
| `export_state_v1` (`GOLDEN_HASH = 0xBF5D_1E1A_A727_143B`) | **3/3 pass, byte-identical** |
| `oracle_differential` (REGS/CRAM/VSRAM hashes) | **3/3 pass, byte-identical** |
| `golden_frames` (6 scenes + discriminator) | **7/7 pass, byte-identical** |
| `determinism_gate` | **2/2 pass** |
| `singlestep_m68000` | **113/113 pass** (641 s) |
| `singlestep_z80`, `proptests`, `watchpoints`, `io_controllers`, `scanline_capture` | **all pass** |
| `conformance_roms` BASELINE | **only `vdp_port_access` moves**: `12/4/16 → 13/3/16`. Every other row — including every `frame_hash=` row, `m68k_memory_test` 13/13, and the `vdp_sprite_masking` string — byte-identical |
| `--lib` unit tests | **693 pass, 1 fail**: `vdp::tests::mem_dma_ring_store_does_not_add_pending_entries`, the test written *specifically* to guard the A3a choice S2 reverses. It must be rewritten (not deleted) into its inverse, citing ROM `$ED10` groups 9/10 |

**No currency moves.** No `EXPORT_STATE_VERSION` bump is implied (no layout change; in fact no value
change either).

### 4.3 Why it is neutral, and the sensitivity ladder

Neutrality is not luck, but it is also not a proof. Three further experiments locate the boundary:

| perturbation | T16 | currency effect |
|---|---|---|
| `entry_drain_cost += 1` mclk | 62/80 (unchanged) | none |
| `entry_drain_cost += 80` mclk (an arbitrary uniform fudge) | 72/80 | **2 visual baselines move**: `m68k_opcode_sizes` `0x5436cda5786ea450 → 0xb902b515a3b9fb64`, `window_distortion` `0x5102219d295b4e2c → 0xbf132d2b24779667` |
| blanked rate `205/167 → 204/166` (Mask of Destiny's measured figures, §2.1's open conflict) | — | none |
| **S1 + S2 (the real slot table)** | **80/80** | **none** |

Two readings, both worth carrying:

* **The correct model is cheaper in currency than the arbitrary one.** A uniform +80 lengthens *every*
  drain and *every* /DTACK stall on *every* line, including blanking, so the two ROMs that stall on VDP
  writes (`m68k_opcode_sizes`, `window_distortion`) shift. The slot table changes only active-display
  drains, and only by redistributing them — the per-line total is unchanged by construction, because
  the table has exactly 18/16 entries.
* **The exposure is real but narrow.** Exactly two of the eleven frame-hash-pinned ROMs are sensitive to FIFO
  stall timing at all. The golden fixture (`testrom.rs`) and all six `golden_frames` scenes drive their
  VDP traffic through paths that never fill the FIFO on an active line (`golden_frames.rs` uses the
  untimed `Vdp::data_write` throughout — `grep` confirms zero `data_write_at`), which is why
  `GOLDEN_HASH` is structurally insulated here in a way it was **not** for A3b.

### 4.4 The honest caveat

**Currency-neutrality of S1+S2 is an empirical property of today's fixtures, not a theorem.** The
prototype's exact mclk mapping (`slot_index × 3420 / total_accesses`) is one defensible choice among
several — see Q1 — and a different mapping could plausibly move `m68k_opcode_sizes` or
`window_distortion`, the two known-sensitive rows. Any implementer must therefore treat "all gates
byte-identical with existing constants" as a **required acceptance criterion of the slice**, re-run
after every change to the table, and stop-and-triage rather than regenerate if a row moves. The
neutrality was demonstrated for the exact code in §3.1/§3.2; it does not transfer automatically to a
variant.

---

## 5. Staged proposal and recommendation

### 5.1 Where the line is

| stage | content | T16 | scorecard | currency |
|---|---|---|---|---|
| **S1** | intra-line slot positions (§3.1) | 62/80 → **72/80** | unchanged (`12/4/16`) | **none, measured** |
| **S2** | post-DMA FIFO occupancy (§3.2), resolves Q1 | 72/80 → **80/80** | **`12/4/16 → 13/3/16`** | **none, measured**; rewrites 1 unit test |
| **S3** | per-line `dma_cost` integration + blanked-line slot positions + refresh adjudication | n/a | n/a | **expected to move visual baselines — defer** |

The line the brief asked for: **it falls between S2 and S3, not inside the T16 work.** The whole of T16
is currency-neutral; the thing that is not currency-neutral is the deferral T16 was mistakenly filed
under.

Neither S1 nor S2 alone flips the scorecard row (S2 alone would reach 70/80). If the owner wants one
commit rather than two, S1+S2 as a single slice is defensible — but two commits keep the "one cause per
currency-relevant change" discipline the arc has used throughout, and S1's byte-count improvement
(62 → 72) is independently verifiable.

### 5.2 Recommendation: **GO**, as S1 then S2, S3 deferred

Reasoning:

* It is the cheapest remaining point on the board. ~50 lines of integer code closes the arc's largest
  named residual and the one the PARKED doc calls "a genuinely larger piece of work".
* It costs no currency, so it needs **no owner ruling** — unlike the two parked decisions. It can land
  under the arc's existing ground rules unchanged.
* It buys a real asset beyond the test: a slot *schedule* is the prerequisite for S3, for the
  mid-sprite pixel-budget cut (ledger row **P1**, which the sprite-masking row's two failures are
  attributed to), and for the mid-line display-disable budget (`docs/2026-07-16-vdp-recon.md:382`,
  Mickey Mania). It converts a "Phase 3" abstraction into a table with citations.
* It closes design question Q1 with evidence instead of leaving it open across two slices.

Counter-arguments, stated fairly: the scorecard gain is **one test row**, `CHARTER.md:53,102` lists this
ROM as a non-goal (superseded for this arc only, `docs/plans/2026-08-03-fifo-scanline-arcs.md:27-30`),
and the slot table introduces a hardware model whose finer details (Q1-Q3 below) are genuinely
unresolved. If the owner's priority is `backlog-gameplay-accuracy` (SRAM, broad ROM validation), this is
a reasonable *defer* — but it is a defer of a cheap, safe item, not of a risky one, and it should be
recorded as such rather than left described as "large".

### 5.3 Test plan (TDD)

1. **Acceptance test first, and it is the ROM's own table.** Extend the existing conformance instrument
   or add a focused test that drives T16's group-2 and group-9 port sequences through `MegaDriveBus`
   and asserts the expected words from ROM `$ED10`, citing the address in the comment. Watch it fail.
2. Unit tests in `vdp.rs` (direct `Vdp` driving), each pinning one clause:
   * `active_slot_gaps_follow_the_published_pattern` — assert the 18 H40 / 16 H32 slot instants for one
     line against the table, so a typo in the indices is caught at the source rather than three layers
     downstream. **This is the highest-value test in the slice**: the existing suite barely exercises
     the active-display drain path (693 of 694 unit tests passed under the prototype, because most
     fixtures leave the display disabled and take the blanked branch).
   * `a_full_fifo_write_stalls_to_the_next_real_slot_not_a_uniform_period`.
   * `mem_dma_leaves_the_fifo_full` — the inverse of today's
     `mem_dma_ring_store_does_not_add_pending_entries`, which is rewritten in the same commit with the
     ROM citation in its comment.
   * `post_dma_fifo_drains_from_the_transfer_end`.
3. **Gates**, and treat the first as the slice's proof, not a formality: `cargo test -p oracle-core`
   (+ `--release`), `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`. **All currency
   suites must pass with their existing constants**; the commit message should say so explicitly, as
   A3a's did.
4. Same-commit BASELINE row + `docs/2026-07-25-testrom-conformance.md` amendment for S2 (S1 amends the
   ledger's byte count only — no BASELINE change).
5. **Manual re-check before pushing S2:** the DR-1/DR-2/DR-3 differential ROMs (Gunstar Heroes, Thunder
   Force IV, Batman) have no automated conformance row and are all heavy DMA users. S2 changes what the
   FIFO looks like immediately after every DMA, so run them by hand
   (`docs/2026-07-22-differential-rom-findings.md`).

---

## 6. Open questions / where I am not confident

* **Q1 — the mclk mapping of a slot index is an approximation.** §3.1 places access *k* at
  `k × 3420 / total`, i.e. a uniform access grid. The research says that is not quite right for H40:
  210 accesses × 16 mclk = 3360 ≠ 3420, because EDCLK slows around hsync. Eke: *"EDCLK is not always
  MCLK/5 during HSYNC, it's actually variating between MCLK/5 and MCLK/4 around HSYNC"*; TmEE's nesdev
  timing table describes the H40 line as *"30 slow pixels, 4 medium pixels, and 386 fast pixels"*. The
  two descriptions both total 3420 but disagree on the microstructure, and I found no source
  reconciling them. The uniform grid is what the prototype measured green on; a more faithful mapping is
  a named follow-up, and it is exactly the kind of change §4.4 says must re-run the gates.
* **Q2 — the FIFO pipeline latency is not modeled.** Mask of Destiny describes *"a delay (2 or 3 slots
  IIRC)"* between a word entering the FIFO and its first byte reaching VRAM. Our drain starts at the
  first slot after the write. T16 cannot distinguish the two (it observes occupancy, not VRAM), and the
  source is explicitly hedged, so I am **not** proposing to add it. Pin it from an instrument or a ROM
  before touching it.
* **Q3 — blanked lines keep the aggregate-rate model.** §3.1 deliberately leaves the blanked branch
  alone: nearly every access is a slot there, so positions carry little information, and it is the path
  every real game's bulk VDP traffic takes — i.e. the highest-blast-radius place to change. This is an
  acknowledged inconsistency in the model (positions on active lines, rates on blanked ones), not an
  oversight. §4.3 shows the blanked *rate* itself is currency-free to change today, which suggests the
  eventual S3 work is less scary than feared; that is a hint, not a result.
* **Q4 — 205/167 vs 204/166.** The Sega manual and Mask of Destiny's measurement genuinely disagree
  about the display-off refresh count (§2.1), and no source adjudicates. Measured currency-free either
  way (§4.3). Recommend leaving 205/167 (the currently pinned, documented figures) and ledgering the
  conflict rather than switching on one unadjudicated measurement.
* **Q5 — whether S2's DMA halt should also shorten.** The prototype leaves the total halt at
  `count × slots × rate` and additionally leaves four entries pending, so the last four words are
  effectively accounted twice. Physically the DMA unit should release the bus roughly four slots
  earlier. The ROM cannot see the difference (it measures FIFO state, not transfer duration), so I did
  not change it — but an implementer should decide this consciously rather than inherit the prototype's
  silence. Shortening the halt *would* change DMA timing for every ROM and is a currency risk; not
  shortening it is a small, documented over-count.
* **Q6 — the H32 slot indices are unexercised.** T16 runs in H40 (measured: our drain cost is 380 mclk =
  `2 × 3420/18`). The H32 table in §3.1 is derived from Kabuto's H32 string by the same arithmetic and
  is asserted by nothing. The unit test in §5.3 step 2 is the only thing that would catch a transcription
  error; write it.
* **Verification I did not do.** I did not test the prototype against the DR-1/2/3 differential ROMs
  (not vendored, no automated row) — see §5.3 step 5. I did not attempt S3 even as a prototype, so its
  "expected to move visual baselines" classification in §5.1 is an inference from the +80 experiment,
  not a measurement.
