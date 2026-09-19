# `m68k_opcode_sizes` — the diagnosis

**Parcel:** `TESTROM-OPCODE-SIZES-NEVER-SETTLES` · **date:** 2026-09-18 · **branch:** `parcel/opcode-sizes-diag`

## The question, and the answer

The question was: *does this ROM have a verdict our harness could read at all — and if it does not, is
that because the ROM has none to give, or because our machine never gets it there?*

**Neither.** The ROM has a verdict, our machine reaches it, and it says **PASS, 2/2**. It is the most
complete verdict in the whole corpus: a CRC-32, against a constant baked into the ROM image, over the
measured length of **every one of the 65536 opcode words**.

```
tiles $400-$407 at >= 685 frames:
  00000000 | 5c6da501 | 00000000 | 66666666 | 00000000 | 20ac2324 | 00000000 | 66666666
             ^ CRC of               ^ colour 6            ^ CRC of              ^ colour 6
               the 32768-byte         = the ROM's           the 8192-byte         = match
               size map               "match" word          class bitmap
```

Both computed CRCs equal the ROM's own baked expectations (`$5C6DA501` at ROM `$56C`, `$20AC2324` at
`$57E`) and both verdict bands are the ROM's green.

So this is **not** a core finding, and section B of the dispatch does not apply. It is a harness gap — of
an unusually complete kind, because the harness was pinning a 27.8%-finished picture of a ROM that
finishes and grades itself.

## What the ROM actually does

Read out of its own code (disassembled from the vendored image; every address below is a ROM offset).

**Boot.** Reset PC `$314`. Standard init, then three things that matter:

* ROM `$3C6-$3D8` fills plane B at VRAM `$E000` with an **identity tile map**: 64 ascending cells per
  row, then `subi.w #$18,d3`, so `cell = row * 40 + col`. Written once, never touched again.
* ROM `$3EC` points the data port at VRAM `$0000` with autoincrement 2. Everything the ROM draws from
  here on is **tile pattern data**, written linearly.
* ROM `$3DC` copies a 36-word routine to work RAM at `$FFFF80` and `jmp`s to it.

**The routine in RAM** is the instrument:

```
FFFF80: 4CF8 FFFF 0000028C   movem.l ($28C).w,d0-d7/a0-a7   ; all 16 registers <- $FFFFE000
FFFF86: <the opcode under test>
FFFF88: F000 F000 F000 ...   ; 32 words of line-F padding
```

All sixteen registers are preloaded with `$FFFFE000` from ROM `$28C` — a scratch RAM address, so every
effective address the tested opcode can form is harmless (and ROM `$4EC` re-clears `$FFDFF0-$FFE007`
before each run, so a memory-destination instruction cannot leave residue).

Vectors 4, 10 and 11 — illegal, line-A, line-F — **all** point at ROM `$42C`. So the routine always ends
in a trap, and the handler's first two instructions are the whole measurement:

```
42C: move.w $4(a7),d1      ; the group-1 frame's stacked PC, low word
430: subi.w #$FF86,d1      ; = the number of bytes consumed since $FFFF86
```

* an **illegal** opcode traps at `$FFFF86` itself, so `d1 = 0`;
* a **valid** opcode executes, and the `$F000` that follows it traps at `$FFFF86 + its size`, so `d1` is
  its length in bytes.

`lsr.w #1` makes that a word count, clamped to 15. Four consecutive counts are packed into one 16-bit word
(`$FFCC` accumulator, `$FFCA` counter) and written to the VDP data port — so the screen is **one pixel
per opcode word, its colour the instruction's length in words, colour 0 = illegal**, laid out linearly
from VRAM `$0000` and displayed through the identity nametable. The opcode itself lives at `$FFFF86` and
is simply incremented (`$464`), wrapping to `$0000` after `$FFFF` to end the pass (`$46E`).

**Opcodes it refuses to execute.** ROM `$472` walks mask/value pairs at ROM `$400` and, on a match,
substitutes the size the ROM knows the encoding to have — `moveq #2` / `moveq #4`, then back into the
packer at `$434` — instead of running it. The first pair, mask `$F100` value `$6000`, is the `Bcc` family,
and for it the ROM even tests the displacement byte (`tst.b d0` at `$47E`) to choose 2 or 4. `$FF00/$6100`
is `BSR`, `$F1F8/$51C8` is `DBcc`. So a branch, a jump or a `STOP` is *asserted*, not measured.

**Second pass.** ROM `$506` sets a phase flag at `$FFCE` and re-walks all 65536 opcodes through the same
classifier, recording **one bit each** — measured or asserted — into an 8192-byte bitmap at `$FF8000`.

**Then it grades itself.** ROM `$62C` builds a CRC-32 table at `$FFA010` (reflected polynomial
`$EDB88320`). ROM `$54C-$55A` reads the 32768-byte pixel map back **out of VRAM** into RAM at `$FF0000`
(8192 long reads through the data port). Then two checks, each `bsr $674`:

| check | buffer | length | expected, baked at |
|---|---|---|---|
| size map | RAM `$FF0000` (read back from VRAM `$0000`) | `$8000` | ROM `$56C` = `$5C6DA501` |
| class bitmap | RAM `$FF8000` | `$2000` | ROM `$57E` = `$20AC2324` |

`$674` writes four tiles per check to VRAM `$8000` onward (set at `$55E`): eight longs of `$00000000`,
eight longs of the **computed CRC**, eight more of zero, and then eight longs of
**`$66666666` if the CRC matched or `$99999999` if it did not** (`$682` / `$68C`). Palette entry 6 is
`$00E0`, full green; entry 9 is `$000E`, full red.

**The verdict is a colour.** That is why the dispatch's printable-ASCII sweep found no `PASS`, `FAIL`,
`result`, `error`, `size` or `done` anywhere in the image: there is no such word to find. The ROM's
verdict alphabet is `$66666666` / `$99999999`, and both words *are* in the image, at `$684` and `$68E`.

**Finally it waits.** ROM `$5E4` configures TH on port 1 (`$40` to `$A10009`, `$40` to `$A10003`) and
seeds a shadow byte at `$FFA000` with `$FF`; `$5FA-$62A` polls both halves of the pad and loops until
`new & ~old` is non-zero — a **release** edge on any of the eight buttons. Each release then draws one of
sixteen magnified pages of the same map and comes back to the wait.

## What we measured

All with `crates/oracle-core/examples/testrom_probe.rs`, which this parcel extended with four dumps:
`CPU=1` (PC/SR/registers), `RAM=<hex>,<count>`, `RAM_CRC=<hex>,<len>` and `VRAM_CRC=<hex>,<len>`.

**1. The verdict, read off the machine.** `TILES=400,8` at 800 frames gives the band quoted at the top of
this document, identical on all eight pixel rows of every tile (the ROM writes each long eight times, so a
complete band is 32 identical bytes per tile — which is also how a half-painted band is detected).

**2. The verdict, recomputed independently.** Transcribing the ROM's own digest and running it over *our*
machine's buffers:

```
VRAM_CRC $0000+$8000 = 0x5c6da501      ROM's baked expectation: 0x5c6da501
RAM_CRC $FF8000+$2000 = 0x20ac2324     ROM's baked expectation: 0x20ac2324
```

This is the measurement that matters. It means the result is not merely "the ROM printed green": the
65536-entry instruction-length table **our decoder produced** is byte-exact against a constant that was
computed on hardware in 2017 and shipped inside the image.

> **The transcription cost, recorded because it would have inverted the answer.** `moveq #$FF,d0` at ROM
> `$656` seeds the digest — and **`MOVEQ` sign-extends its byte**, so the seed is `$FFFFFFFF`, the standard
> one. Read as the literal `$000000FF` the digest yields `0x3e13d6a8` where the ROM expects `0x5c6da501`.
> The first run of this check therefore *disagreed*, and a less careful reading of that disagreement is a
> report that says "our CPU gets 65536 opcode lengths wrong".

**3. Progressing, then parked — not spinning, not trapped.** `CPU=1` at rising budgets:

| frames | PC | where |
|---|---|---|
| 60 | `$FFFFFF86` | executing the opcode under test |
| 120 | `$FFFFFF8A` | ditto (a 2-word instruction's padding) |
| 300 | `$00048E` | the classifier at `$472` |
| 400 | `$000438` | the trap handler at `$42C` |
| 660 | `$000664` | inside the CRC digest loop `$65A-$66C` |
| 685 | `$00060E` | **the pad wait `$600-$628`** |
| 800 | `$000620` | the pad wait |
| 3600 | `$000610` | the pad wait — 60 s of emulated time later, still there |

With no pad input the wait can never exit (`$FFA000` is seeded `$FF`, every poll reads `$FF`, so
`new & ~old` is always 0), so this is a terminal, deliberate state.

**4. The picture settles.** Every budget from 685 to 3600 frames gives `frame_hash=0xb1e54eed02744f27`:

| frames | 120 | 300 | 400-675 | 680 | 685-3600 |
|---|---|---|---|---|---|
| `frame_hash` | `0x5436cda5786ea450` | `0x102fe6ffdd51e11c` | `0x5330009202fa2287` | `0x4d0039ce282facc7` | `0xb1e54eed02744f27` |

Sampled at 400, 500, 550, 600, 620, 640, 650, 660, 665, 670, 675 (all one value) and 685, 690, 695, 700,
800, 900, 1000, 1100, 1150, 1200, 1800, 3600 (all another).

**5. The layout, cross-checked against the 68000 rather than against us.** Tile `$200` covers opcodes
`$8000-$803F`, `OR.b <ea>,D0`. At 800 frames its eight rows read

```
1×8 / 0×8 / 1×8 / 1×8 / 1×8 / 2×8 / 2×8 / 2,3,2,2,2,0,0,0
```

which is exactly `Dn` 1 word · **`An` illegal** (the K1 fix) · `(An)`, `(An)+`, `-(An)` 1 word ·
`d16(An)`, `d8(An,Xn)` 2 · `(xxx).w` 2 · `(xxx).l` 3 · `d16(PC)`, `d8(PC,Xn)`, `#imm` 2 · modes 7/5-7/7
illegal. The pixel-per-opcode model is therefore not an inference from our own decoder.

**6. The pad path, at the right moment.** `PRESS=<btn> PRESS_AT=700 PRESS_LEN=5` run to 800 frames moves
the picture from `0xb1e54eed02744f27` to `0x36cc9a7834e70a45`, and the PC returns to `$60E`. Identical for
`a`, `start`, `up` and `c` — consistent with "a release edge on any button".

## Why the harness never saw it — two independent causes, both ours

**The budget.** `scrape_visual` runs 120 frames, and has since the suite's first commit (`7b46ae2`;
`git log -S` finds no other value). At frame 120 the ROM's own opcode counter at `$FFFF86` reads
**`$46AF`** — 27.8% of the way through the sweep. Tiles `$200`, `$300` and `$380` (the `$8xxx`, `$Cxxx`
and `$Exxx` pages) are **entirely unwritten** at that point, measured: all-zero at 120 frames, populated
at 800.

**The channel.** The verdict is tile pattern data. The probe's text grid decodes *nametable cells* — and
this ROM's nametable is an identity map, constant by construction. The "ASCII character ramp" that read as
a font page is the identity map's own tile indices `$20`-`$7E` rendering as printable characters. That
channel could never have carried a verdict, at any budget. Neither cause is discoverable from the other:
a longer budget in the text channel still shows the ramp, and the right channel at 120 frames still shows
a blank band.

## Corrections

**To the dispatch brief.** Four of its six measurements hold; two do not.

1. *"The decoded plane text is identical at 120, 600 and 1800 frames — it holds an ASCII character
   ramp."* **True as a measurement, wrong as a reading.** It is not a ramp the ROM printed; it is the
   identity nametable's tile indices. Nothing about it could ever change, which is a stronger statement
   than "it did not change here".
2. *"No button changed it."* **True, at a moment nothing could.** Frame 60 is ~620 frames before the ROM
   initialises its pad shadow at `$5E4`. Pressed at 700, every button changes it.
3. *"The frame hash differs at EVERY budget — the picture is still in motion at thirty seconds of
   emulated time."* **FALSE.** The picture settles by frame 685. The four samples (120/300/600/1800)
   straddled the two transitions; at 1800 frames it has been static for ~18.7 s. **So the row's
   `F-TIMED-PIN-UNSETTLED` amendment is itself wrong on this ROM**: this pin was unsettled because the
   budget was too *short*, not because the picture never settles. There was a settled frame all along.
4. *"The ROM does touch the pad."* **Right**, and it is `$5E6`/`$5EC` writing and `$5FA-$62A` reading;
   the edge is a release edge and any of eight buttons serves.
5. *"It carries no verdict strings at all… a ROM whose verdict is a word would usually contain the
   word."* **Right about the strings**, and the inference is sound as far as it goes. The missing step is
   the converse: a ROM whose verdict is a *colour* contains the colour, and `$66666666`/`$99999999` are
   both in the image.
6. *"The cheap surface is the one that lies."* **Right, and the mechanism is worth naming.** The cheap
   surface did not lie by being coarse. It lied by being **constant by construction** — a decode of data
   the ROM writes once at boot. A surface that cannot vary cannot disagree, and "it agreed at three
   budgets" is the reading such a surface always produces.

**To this ROM's own ledger row.** Its `BASELINE` comment says the K1 fix moved "476 px across the
`$0/4/8/C/E` opcode pages". Three of those five pages (`$8xxx`, `$Cxxx`, `$Exxx`) are **not plotted at
all** in the frame the 120-frame pin hashes, and the budget has never been anything but 120. So the span
as stated cannot be right. What the 476 figure actually measured is not recoverable from here — the
pre-K1 core would have to be rebuilt, which is a `src/` change and out of this parcel's scope — so this is
recorded as an inconsistency, not as a corrected number. The K1 fix *is* visible in the settled picture,
at tile `$200` row 1 (`OR.b An,D0`, `$8008-$800F`, colour 0 = illegal); it simply cannot have been visible
in the pinned frame.

## What the row is now

`m68k_opcode_sizes` becomes a verdict row:

```
2/2 crc32 sizemap=0x5c6da501 classmap=0x20ac2324 — 65536 opcode lengths
+ the 8192-byte measured/asserted bitmap, against the ROM's own baked constants
```

**The scrape is proved from the machine, not asserted.** `OpcodeSizesRun::establish` is the only
constructor and the only route to `verdict()`, and it requires four things that do not imply one another:

1. the PC is inside the ROM's own pad wait `$5FA-$62A` — the completion signal, and the one a still
   picture cannot give, because finished-and-waiting and hung look identical on screen;
2. both bands read *as bands* — separators zero, every tile 32 identical bytes, the verdict tile one
   repeated nibble;
3. **our own CRC-32 of the live machine equals the CRC the band displays**, for both buffers. This is the
   condition that ties the four tiles being read to the 32768-byte map and 8192-byte bitmap they are
   supposed to describe;
4. the verdict nibble is one of the ROM's two, read out of the image at `$684`/`$68E`.

What it deliberately does *not* require is that the CRCs match the ROM's expectation — that is the
reading, not the proof, and a failing emulator has to stay representable or the instrument could never go
red. The sweep is **waited on, never timed**: `establish` steps 20 frames at a time until the ROM parks
itself, with a 2400-frame ceiling that is only the point at which we stop believing it will. It
established at 700 frames.

Every expectation comes from a source that does not move when our code does: the two CRC constants are
read **out of the vendored image** (with the `move.l #imm,d2` opcode bytes checked at `$56A`/`$57C`, so a
wrong offset is refused rather than used), the verdict colours out of the image at `$682`/`$68C`, and the
digest is transcribed from the ROM's own table build and loop.

### The mutations, and the guard each one fired

Every mutation was applied on disk (shown as a `git diff` line before each run) and restored from the
committed baseline `e72afc3`. Two that look alike fired two different guards, and one fired the *wrong*
guard and had to be re-cut — recorded, because a red on the wrong guard reads as proof of the right one.

| # | mutation | guard that fired |
|---|---|---|
| M1 | `OPSIZE_BAND_VRAM` `$8000` → `$8020` | the scorecard **count guard** (`:2412`) — "17 vendored ROMs are on disk but only 16 produced a scorecard row", after `establish` printed NOT ESTABLISHED |
| M2 | CRC seed `0xFFFF_FFFF` → `0x0000_00FF` (the sign-extension bug, deliberately) | the **independent-CRC agreement** assert (`:2282`) — "the band says 0x5c6da501, but the same digest over the live machine's own buffer is 0x3e13d6a8" |
| M3 | expectation read at ROM `$56E` instead of `$56A` | the **ROM-offset instruction self-check** (`:2070`) — "ROM $056E is $A501, not the sizemap-expectation instruction $243C" |
| M4 | `long_at(at + 2)` → `long_at(at + 2) ^ 1` | **WRONG GUARD** — the uniform-nibble assert (`:2091`), because `^1` also perturbed `$66666666`. Re-cut as M4′. (It does prove that guard is live.) |
| M4′ | only the sizemap expectation perturbed by one bit | the **scorecard BASELINE diff** (`:2429`); the row read `1/2 crc32 sizemap=0x5c6da501 MISMATCH (ROM expects 0x5c6da500, band colour 6)` |
| M5 | pass colour read off by one | the **verdict-nibble sanity** assert (`:2293`) — "the sizemap verdict tile is colour 6 — the ROM paints only 7 (pass) or 9 (fail)" |

M1's first run went red on the right guard with a **wrong explanation**: its note said "neither happened"
while the PC was in fact parked at `$610`. The note now reports the two conditions separately and says
what each combination means, and M1 was re-run against the new wording (the row in the table is the
re-run).

**One condition is not independently red-provable here, and is recorded rather than claimed.** Condition
1 (the parked PC) cannot be shown red separately from condition 2 on this machine: they become true within
the same 20-frame step, so widening the wait range leaves the run green and narrowing it fires M1's guard.
It exists for the machine that *hangs* mid-sweep, and producing one would need a `src/` change. Likewise
M4′ exercises the *expectation* path, not the band-colour path: a colour-9 band requires a real
instruction-length regression, which is also a `src/` change. Both are limits of a no-core-changes parcel,
not of the instrument.

## Left open

* **The 476-px K1 figure** (above). Recoverable only by rebuilding the pre-K1 core; not attempted.
* **The sixteen magnified pages** behind the pad wait are not read. They are the same data at 4× scale, so
  they carry no information the 1:1 map does not — a scrape of them would be a second reading of one
  measurement.
* **The classifier's own correctness is the ROM's, not ours.** For the encodings at ROM `$400` — `Bcc`,
  `BSR`, `DBcc` and the rest of the don't-execute list — the size in the map is the ROM author's
  assertion, and our CPU is never asked. The 8192-byte bitmap is exactly the census of which pixels those
  are, and it is the second thing the row checksums, so the split is *pinned* even though the asserted
  half is not *tested*. Roughly: whatever those encodings are, both halves agree about which is which.
* **No cross-emulator arm was run and none is warranted.** The ROM grades itself against a constant
  computed on hardware; another emulator's opinion could not add to that. (`tools/blastem-differential`
  stays unused here.)

## Scope

No file under `crates/oracle-core/src/` was touched. Changed: `crates/oracle-core/tests/conformance_roms.rs`
(this row and its section), `crates/oracle-core/examples/testrom_probe.rs` (four dumps),
`docs/2026-07-25-testrom-conformance.md` (the row and Q2), and this file. The other 16 scorecard rows are
byte-identical, and the four frozen currency suites were measured, not inferred — see the parcel's final
report.
