# VDP-PORT-ACCESS-FULL-ROM: all 122 VDPFIFOTesting tests, read, sorted and pinned

> **Dated 2026-09-12.** Parcel VDP-PORT-ACCESS-FULL-ROM, branch `parcel/vdp-port-access-full-rom`, brief
> `docs/2026-09-12-vdp-port-access-full-rom-brief.md`. **Test code and docs only. Nothing under
> `crates/*/src/` changes, so no frozen golden can move.** Every number below was measured on this branch.
> The scratch-copy experiment in the "Method" section was run in a copy of `crates/` outside the repo, and
> none of it is committed.

## What this found, in one paragraph

The vendored hardware test ROM `vdp_port_access` (VDPFIFOTesting, "VDP Port Access Test ROM (11-12-19)")
runs **122 tests over 22 pages** and prints **76 passed / 46 failed / 122** on our VDP. Until now the
scorecard read only the first two pages (16 tests, all passing), and M22's copy test read 28 more. The 46
failures come from **six causes**. **Five are behaviour bugs (bucket A)**: the ROM's tables pin the right
answer, and each is a small fix row. **One is a mechanism we do not model (bucket M)**: a DMA fill that
runs over time. There are **no timing (T) and no unexplained (U) failures**. A scratch copy that applies
the five A rules together takes the ROM to **119/3/122**, leaving only the three M tests. Each rule, applied
alone and left out alone, flips exactly the tests this document assigns to it. No frozen currency moved in
`oracle-core` under the five rules.

## Where the brief was wrong

1. **The copy matrix's first eight words are not a post-copy read-path behaviour.** They are VSRAM reads
   (command `$00000010`: code `$04`, VSRAM address 0) and CRAM reads (command `$00000020`: code `$08`, CRAM
   address 0), alternating, with a CRAM write of `$FFFF` to CRAM `$0020` (command `$C0200000`) between the
   pairs (ROM `$EA2C..$EB6E`). The CRAM halves already match. The VSRAM halves match in their snooped
   undefined bits and differ only in bits 10-0, which are the stored VSRAM word 0: hardware `$0123`, ours
   `$064E`. The cause is the test's own setup. At `$E87C` it loads 64 words into VSRAM from address 0 (table
   `$EC6C`), and our VSRAM wraps at 80 bytes, so table word 40 (`$064E`) overwrites word 0. It has nothing
   to do with the copy, which is why the eight words are identical in all 27 cases. This is cause A1 below.
   The brief's hypothesis came from `vdp_port_access_copy_dma_matches_the_roms_own_tables`' doc comment and
   the F-COPYXOR residual in `docs/2026-07-25-testrom-conformance.md`. This parcel corrects the test's doc
   comment. The ledger gets a dated line pointing here.
2. **Test 29 "DMA Copy Source Reg Update" is not an unmodelled mechanism.** The source registers are
   modelled. We compute a wrong value: a copy (and a fill, test 28) must advance registers 21/22 by its
   length, and ours leaves them where they were. This is cause A3, a behaviour bug.
3. **The record layout in the M22 doc comment was slightly off.** Word 0 is the record's height in screen
   rows (the display routine adds it to its row counter at `$0E16`), not a test number. `$FFFF` takes one
   word, not two (`$0E50`). Neither mistake changed M22's result, because the only `$FFFF` is the list's
   last word.
4. **"Records with no expected table" do not occur in this ROM.** All 122 carry one. The rule for them is
   still read and implemented (see "How the ROM judges a test").
5. **F-FILLTGT's "the ROM does not cover the case" was wrong too**: test 34 covers it (cause A5).
6. Premises that held: 122 tests, 22 pages, 76/46/122, and page 1 and page 2 at 9/0/9 and 16/0/16.

## How the ROM judges a test

The ROM keeps one result record per test in 68000 RAM from `$FF0000`. A record is: word `rows`, word
`title_len`, word `data_len`, the title, word `has_expected`, then `data_len` bytes of expected values
copied from the ROM's own table (when `has_expected` is set), then `data_len` bytes of what the VDP
answered. After every test the main list (`$030C..$0D66`) calls the display routine at `$0D8A`, which
redraws the current page from its first record:

* `$0000` ends the list. `$8000` (`$0E6E`) and `$FFFF` (`$0E50`) each take one word and end the page. A
  page holds at most 21 rows (`$0E16..$0E1C`), so a record that would overflow it starts the next page.
* At a page end the routine prints `Results: ( P/ F/ T)`, which is cumulative over all pages so far
  (`$0E74..$0F1A`), and waits for a button in `$0F46..$0F62`. **Start** continues. **A** sets a
  "run every page" flag (`$FFFF02` bit 0), which the final `$FFFF` clears. **B** stops only at pages with
  new failures. The pad read is `$1122`.
* The verdict is in `$0F9E..$103E` and the compare loop `$1092..$10E6`. `data_len` bytes of the actual data
  are compared byte by byte against a second pointer, and any difference paints the cell red (`$4100`) and
  counts a fail. The second pointer is the expected table when `has_expected` is set. Otherwise it is the
  actual data itself, so **a record with no table is shown, not judged, and counts as a pass**, and so does
  a `data_len` of zero or less. Transcribed as `PortAccessRecord::rom_pass`.
* After the last page the list ends `$FFFF`, `bra $02DA`: the ROM starts again from test 1.

## Method

* **One run, read three ways.** `port_access_run` (a `OnceLock` in `conformance_roms.rs`) boots the ROM
  once per test binary. The scorecard row, M22's copy tables and the new pin all read that one run. It
  pages with the ROM's own signal: whenever the 68000 is parked in the wait-for-a-button code, the run
  reads the printed tally and presses Start. That code is entered only after a page's `Results` line is
  printed. The run takes 617 frames.
* **The decoder is checked against the ROM's own arithmetic.** It replicates the display routine's page
  split, and its verdicts must reproduce **all 22 cumulative tallies the ROM printed**. A misread layout,
  a wrong page boundary or a wrong verdict rule changes at least one of them. Mutations M5 and M6 below show
  both failures happening.
* **Disassembly** of `vendor/TestRoms/vdp_port_access.bin` with the Capstone disassembler (Python
  bindings), a disassembler rather than emulator source. ROM addresses below are its output.
* **Hardware prose** (clean-room policy, no emulator source): SpritesMind threads by Nemesis, Eke and
  Mask of Destiny, the MegaDrive Wiki "VDP" page and Plutiedev's DMA page. Quotes are in each cause's
  section.
* **Scratch-copy experiment (never in the repo).** A copy of `crates/`, `Cargo.toml` and `Cargo.lock`
  lives in the parcel's scratch directory, with each candidate rule gated by a bit of an `EXP` environment
  variable. It was run through the same `vdp_port_access_full_rom_verdicts` test, release profile:

| EXP | Rule(s) on | ROM tally | Failing tests |
|---:|---|---|---|
| 0 | none (control) | 76/46/122 | the pinned 46 |
| 1 | A1 writes: 7-bit address, `$50-$7F` discarded | 103/19/122 | 13 20 27 28 29 31 32 33 34 36 38 the 8 VSRAM fills |
| 2 | A1 reads: 7-bit address, `$50-$7F` read word 0 | 75/47/122 | the 46, plus 13 |
| 3 | A1, both halves | 112/10/122 | 20 27 28 29 31 32 33 34 36 38 |
| 4 | A2 | 77/45/122 | the 46 minus 27 |
| 8 | A3 | 78/44/122 | the 46 minus 28, 29 |
| 16 | A4 | 78/44/122 | the 46 minus 36, 38 |
| 32 | A5 | 77/45/122 | the 46 minus 34 |
| 5 | A1 writes + A2 | 105/17/122 | 13 28 29 31 32 33 34 36 38 the 8 VSRAM fills |
| 7 | A1 + A2 | 114/8/122 | 28 29 31 32 33 34 36 38 |
| 63 | all five | **119/3/122** | **31 32 33** |
| 62 | all but A1 writes | 81/41/122 | 13 20 23 31 32 33 74-95 96-122 |
| 61 | all but A1 reads | 110/12/122 | 13 31 32 33 74-95 |
| 60 | all but A1 | 82/40/122 | 20 23 31 32 33 74-95 96-122 |
| 59 | all but A2 | 117/5/122 | 20 27 31 32 33 |
| 55 | all but A3 | 117/5/122 | 28 29 31 32 33 |
| 47 | all but A4 | 117/5/122 | 31 32 33 36 38 |
| 31 | all but A5 | 118/4/122 | 31 32 33 34 |

  Under EXP=63, `cargo test --release -p oracle-core --no-fail-fast` (with `CI=1`): **determinism_gate
  2/0, export_state_v1 3/0, golden_frames 7/0, scanline_goldens 5/0**. The scorecard's only moved row is
  `vdp_port_access` (to 119/3/122); the other 16 rows are byte-identical. One unit test failed,
  `vdp::tests::a_register_write_retains_cd5_cd2`, because it indexes VSRAM storage with the old decode
  (`vdp.rs:2072`, `% VSRAM_SIZE`); the A1 row must update it. `toolchain_floor` failed 6 tests only
  because the scratch copy has no `.github/`. The other crates were not run under the rules.

## The failures by cause

Six causes for 46 tests. Test 20 has two causes, one per half. Every A cause names our mechanism at
`file:line` (as of this branch) and the hardware rule its table pins.

### A1: VSRAM address decode (36 tests, and half of test 20)

**The rule.** The VSRAM address is 7 bits, so it wraps at `$80`. Writes to `$50-$7F` are discarded, and
reads from `$50-$7F` return the VSRAM read latch. Nemesis, SpritesMind "Scaling hardware?" p.5, 2012-06-19:
"Writes to CRAM and VSRAM wrap at an 0x80 byte boundary. Writes to the upper portion of VSRAM in this
region (0x50-0x80) are discarded." Nemesis, SpritesMind *VDP Internals* p.4: "When you read beyond the end
of VSRAM, you don't get the first VSRAM entry, what actually happens is that the read doesn't latch any
data at all, and what gets returned is actually the current state of the internal register that latches
the VSRAM read data."

**Ours.** Both port paths wrap at 80 bytes: `write_target`, `crates/oracle-core/src/vdp.rs:1014`, and
`read_target`, `vdp.rs:950`, each compute `((addr & 0xFFFE) as usize) % VSRAM_SIZE` (`VSRAM_SIZE = 0x50`,
`state_hash.rs:16`). A write to `$50-$7F` lands on words 0-23 instead of being dropped. An address of `$80`
or above maps to `addr % 80` instead of `addr & $7F`.

**Evidence that it is one cause.**
* **Test 23 "DMA Transfer to VSRAM Wrapping"** (ROM `$6BFC`) DMAs `$40` words, then `$80` words, from table
  `$6F16` to VSRAM 0 with autoincrement 2, and reads words 0-3 back. After `$40` words, hardware has
  `0777 0666 0555 0444`, source words 0-3, because words 40-63 were discarded. Ours has `0222 0111 0777
  0666`, source words 40-43, wrapped onto word 0. After `$80` words, hardware has `0123 0123 03ea 04e8`,
  source words 64-67: the address wrapped at `$80` and words 104-127 hit `$50-$7E`. Ours has source words
  120-123.
* **Tests 74, 77, 80, 83, 86, 89, 92, 95, the eight VSRAM fills** (ROM `$13BEC`, the fill run with d4 = 0-7)
  load 64 words (table `$13E58`) from VSRAM 0, fill, and read 64 words back. Hardware: words 0-39 are the
  table as the fill left it, and every read of `$50-$7E` returns `$0123`. Ours: words 0-23 hold table
  words 40-63, and reads of `$50-$7E` alias back onto words 0-23. The fill's own writes agree everywhere
  (with autoincrement 1, for example, words 16-20 are `$0604` on both).
* **Tests 96-122, the copy matrix.** Their first eight words are VSRAM word 0 and CRAM word 0, as brief
  item 1 explains. Only the VSRAM halves differ, in all 27 cases alike. The last eight words are the copy
  image, which M22 fixed and which match.
* **Test 20's VSRAM half** DMAs to VSRAM `$8020` (command `$40200092`) and reads at `$0020` (command
  `$00200010`). On hardware, `$8020 & $7F = $20` is word 16. Ours puts `$8020 % 80 = 0` at word 0, so the
  read finds the zeros the test left there.
* **Test 13 constrains the fix, not the diagnosis.** It passes today. It writes and reads VSRAM at `$8000`
  (commands `$40000012` / `$00000012`), which round-trips under any decode that reads and writes share.
  EXP=1 and EXP=2 alone each turn it red, and EXP=3 keeps it green. **The two halves must change together.**
* What varies across the 36 tests is the operation (DMA, fill, copy) and the parameters. What stays the
  same is that every wrong word is a VSRAM word, and each one is explained by `% 80` against `& $7F` with
  the `$50-$7F` discard. EXP=3 flips exactly these 36 and nothing else. EXP=61, the write half without the
  read half, flips 23 and 96-122, which read only words 0-3. The fills, which read `$50-$7E`, stay red, and
  so does test 13.

**The one open point inside A1: the value a read of `$50-$7F` returns.** Every such read in these tables
returns `$0123`, and in each of those tests VSRAM words 0 and 1 are both `$0123`. The ROM therefore cannot
tell "word 0", "word 1" and "the read latch" apart, if the latch holds the renderer's last full-screen
vertical-scroll fetch. The latch is not the port's own pre-cache: reading word 39 just before reading
`$50` would have left `$0560` in the pre-cache, and the table says `$0123`. The scratch rule "return
word 0" reproduces every table, but it is not the documented mechanism. See "What would settle the open
points".

> **Fixed 2026-09-12 (VSRAM-DECODE, branch `parcel/vsram-decode`).** Both halves go through one decode,
> `Vdp::vsram_byte` (`addr & $7E`, `None` at `$50` and above), used by `read_target` and `write_target`,
> and so by the port write, the 68k-to-VSRAM DMA word, the fill body and the port read-ahead. A read of
> `$50-$7F` returns a new `Vdp` field, `vsram_read_latch`. It rides the snapshot and is in neither
> `state_hash` nor `export_state`. **The open point, settled for this core, not for the chip.** The latch is
> fed by the committed render only (`commit_scanline_vscroll`, called from `render_scanline` and
> `advance_scanline`). A display-enabled line leaves the last VSRAM word its background fetch reads:
> word 1 in full-screen mode, and the last column's plane-B word in 2-cell mode. A scratch probe measured
> where the ROM's reads land. All 16 reads past `$4E` fall on active lines 93 and 105, with the display
> on, full-screen vertical scroll and H40, and each `$50` read-ahead falls on the same line as the `$4E`
> read before it. So a latch fed by port reads would answer `$0560` (word 39) where the table says `$0123`.
> This core renders a line at its start, so the hardware's interleaving of render fetches between port
> slots cannot be expressed here, and the render-only feed is its line-granular form. The ROM still cannot
> tell word 0 from word 1 (both `$0123`). The unit test `a_vsram_read_of_50_7f_returns_the_read_latch`
> separates all three candidates. The "What would settle" item below still stands for the chip. Result:
> **76/46/122 → 112/10/122**, failing exactly 20 27 28 29 31 32 33 34 36 38, the EXP=3 prediction. Test 20
> goes from 8/12 to 6/12 words off: the two A1 words of its VSRAM half now match, and the remaining six
> are A2's source wrap, two per half.
>
> Mutation record (release, each applied to the committed file, run, and restored from `HEAD` with `cmp`):
> the write half reverted gives 75/47/122, which adds test 13, and three unit tests go red. The read half
> reverted gives 103/19/122, which adds 13 and the eight fills, and two unit tests go red. Port reads feeding
> the latch give 104/18/122 (the eight fills fail), and `a_vsram_read_of_50_7f_returns_the_read_latch` goes
> red. `advance_scanline` not feeding it gives 104/18/122, because the ROM runs on that path, and the
> both-paths unit test goes red. Three mutations leave the ROM at 112/10/122 and each turns a unit test red:
> "return word 0", "a display-off line feeds it" and "full-screen's last fetch is word 0". For those three
> the unit tests are the only guard.

### A2: the 68k-to-VDP DMA source wraps inside its 128 KB page (test 27, and half of test 20)

**The rule.** Only source registers 21 and 22 count. Register 23 never takes a carry, so the source wraps
inside its 128 KB page. MegaDrive Wiki, "VDP", *DMA Limitations*: "Every DMA cycle, only the low and middle
bytes of the DMA source registers are incremented." Plutiedev, *DMA transfer*: "the source address can't
cross a 128KB boundary".

**Ours.** `run_mem_dma` steps the source with `src = src.wrapping_add(2)` (`crates/oracle-core/src/bus.rs:1502`)
and hands `src >> 1` to `Vdp::dma_complete` (`bus.rs:1513`), which writes the carry into register 23
(`vdp.rs:1298`).

**Evidence.** In **test 20** (ROM `$60EA`) the VRAM and CRAM halves DMA 4 words from `$5FFFC`. The ROM holds
`89ab cdef` at `$5FFFC`, `dead c0de` at `$60000` and `ffff eeee` at `$40000`. Hardware reads `89ab cdef ffff
eeee`; ours reads `89ab cdef dead c0de`. In **test 27** (ROM `$A5D0`), group 4 DMAs from `$5FFFC` across the
boundary. Group 5 then rewrites registers 21 and 22 but not 23 (`$A96A..$A96C`). On hardware register 23 is
still `$02`, so the DMA reads `$401FC` and gets `1111 ffff eeee dddd`. Ours took the carry to `$03` and reads
`$601FC`, which gives `dead c0de dead c0de`. EXP=4 flips 27; 20 also needs A1 (EXP=5 and EXP=7).

### A3: fill and copy advance the DMA source registers (tests 28, 29)

**The rule.** Nemesis, *VDP Internals* p.4: "Every DMA operation also performs the exact same set of steps
after it is advanced one step, which is to firstly add 1 to the lower 2 DMA source address registers, then
to subtract 1 from the DMA length counter register". That includes fill and copy, even though a fill
never reads its source.

**Ours.** `run_fill` (`vdp.rs:1392`) and `run_copy` (`vdp.rs:1462`) zero the length registers 19/20 but
never touch 21/22.

**Evidence.** **Test 28** (ROM `$AA16`) fills 4 bytes with source registers set to `$00FA`, then starts a
68k DMA that writes registers 22 and 23 but skips 21 (`$AC54..$AC58`). Hardware: 21 = `$FE` (that is,
`$FA + 4`), so the source is `$200FE`, byte `$401FC`, which gives `1111 ffff eeee dddd`. Ours: 21 still
`$FA`, so the DMA reads `$401F4` and gets `5555 4444 3333 2222`. The next group repeats the shift from
`$00FE`. **Test 29** (ROM `$AEC0`) is the same with a copy (`$97C0`, command `$000000C2`), and its tables
match 28's word for word. EXP=8 flips exactly 28 and 29.

**Fixed 2026-09-12 (DMA-SRC-ADVANCE, branch `parcel/dma-src-advance`).** `run_fill` and `run_copy` now end
with `Vdp::advance_dma_source_low16`, which leaves registers 22:21 at `(start + steps) & $FFFF` and never
touches register 23. The arithmetic comes from the tables. A 4-byte operation takes 4 steps, so `$00FA`
becomes `$00FE` (group 2, the table word `1111` at ROM `$401FC`). Register 21 carries into 22, so `$00FE`
becomes `$0102` (group 3, whose table `ffff eeee dddd cccc` sits at ROM `$403FC`: word `$0201FE`, so register
22 is `$01`; this group's follow-up DMA writes 21 and 23 but not 22, at `$AE04..$AE08`). Two cases come from
the rule rather than the ROM: no carry into register 23 (the ROM's follow-up DMA always rewrites 23), and a
length of 0, which is 65,536 steps and one whole turn of the 16-bit counter. The copy advances from the source
`arm_dma` read out of 21/22. The fill advances from the live 21/22, since it reads nothing. The only
production path that completes a fill or a copy is `MegaDriveBus::run_pending_dma`. It runs after every
68k data-port write (the fill trigger) and control-port write (the copy trigger), word and byte alike. The
Z80's `$7F00` window drops VDP writes, and the untimed `Vdp::data_write` only arms a fill. Measured: the ROM
prints **78/44/122** and fails the 46 minus 28 and 29, no count moved, and 25, 26 and 27 unchanged.
`vdp::tests::a_fill_advances_source_registers_21_22_by_its_length_and_never_carries_into_23` and its copy
twin pin the four cases. The other 16 scorecard rows, `determinism_gate`, `export_state_v1`, `golden_frames`
and `scanline_goldens` are byte-identical.

### A4: DMA busy is set by the fill command itself (tests 36, 38)

**The rule.** Eke, *VDP Internals* p.4: "on DMA Fill, busy flag is actually immediately (?) set after the
CTRL port write, not the DATA port write that starts the Fill operation". The ROM confirms it.

**Ours.** Status bit 1 is `mclk < dma_busy_until` (`vdp.rs:660`). Only a completed transfer opens that
window (`run_fill` `vdp.rs:1440`, `run_copy` `vdp.rs:1487`, `dma_complete`). The fill's control word only
arms (`vdp.rs:1092`, `arm_dma` `vdp.rs:1099`).

**Evidence.** **Test 36** (ROM `$B6F8`) reads status after the fill command `$40020082` and before the `$1234`
trigger. Hardware gives `$0202` (FIFO empty plus busy); ours gives `$0200`. **Test 38** (ROM `$C34C`) group 2
is the same, and on hardware busy stays set across a `$8144` register write and a `$4002` half-command
(`$C870..$C878`). In group 1, where DMA is disabled when the command is written, neither machine reports
busy. This is behaviour, not timing: the two samples fall between two port writes and depend on nothing
the beam does. EXP=16 flips exactly 36 and 38.

**Fixed 2026-09-12 (FILL-BUSY-ARM, branch `parcel/fill-busy-and-target`).** `Vdp::dma_busy` is now
`self.fill_armed() || mclk < self.dma_busy_until`, and `status_word` reads bit 1 through it so there is one
predicate. `fill_armed` is **exactly the condition that makes the next data-port write a fill trigger** —
CD5 latched in the code register and register 23's mode bits naming Fill — so it is derived from existing
state: no new field, no snapshot or `export_state` layout change, and the flag cannot disagree with the
trigger. The DMA-disabled negative comes out for free: CD5 only latches while register 1 bit 4 is set, so
test 38 group 1's command (issued at ROM `$C528` with DMA off) arms nothing and reads clear at all four
probes, and its `$1234` lands as an ordinary VRAM write.

*Two corrections to this section, from the run and the disassembly.* (i) The line numbers above are stale
(they predate A1 and A3): status bit 1 is `vdp.rs:690`, `run_fill` ends at `:1513`, `run_copy` at `:1564`,
`control_write`'s arm path is `:1160` and `arm_dma` `:1168`. (ii) "In group 1 … the arm is conditional on
DMA being enabled" is right about the outcome but understates the mechanism: in group 1 CD5 is never
latched at all, so no fill is armed and the later `$1234` is a plain data-port write — which is why the
group's VRAM reads back `0000 1234 0000 0000`. The ROM also samples status **twice** per probe (the second
read's undefined high bits carry the next prefetch word), so each probe is the pair `0202 4e02` when busy
and `0200 4e00` when not; the "two samples" of the old text are one probe.

**The open question, answered by construction.** What ends the busy flag of a fill that is armed but never
triggered: **nothing but the arming condition going away** — the fill running (`take_dma_request` clears
CD5 on consumption and `run_fill` then opens the timed window), a later command word clearing CD5 while
DMA-enable is set, or register 23 leaving Fill mode. Not time, and not a frame boundary. Test 38 group 2
rules out the three cheap alternatives itself: a register write (`$8144`, which also clears DMA-enable) and
a new first command word (`$4002`) both leave it set. The alternative model is a latched `fill_armed` bool
cleared only by `run_fill`; it agrees with every table in this ROM and differs on exactly two sequences —
arm a fill, then write a full non-DMA command word (or set register 23 out of Fill mode), and poll status.
This model says clear, the latch says set. A third, any timed window, is separated from both by polling an
untriggered fill across frames. **These are the ROMs that would settle it**, and they are what the "What
would settle the open points" entry below should now read. Pinned as shipped by
`vdp::tests::a_fill_reads_dma_busy_from_its_control_write_not_from_its_trigger`,
`a_fill_command_written_with_dma_disabled_arms_nothing_and_is_not_busy` and
`an_armed_fill_stops_reading_busy_once_its_arming_condition_is_gone`.

Measured: the ROM prints **116/6/122** (from 114/8/122), failing 20 27 31 32 33 34. Every one of the 122
per-test records is byte-identical to the pre-fix run except 36 and 38, which go FAIL 2/16 and FAIL 4/32 →
PASS 0/16 and PASS 0/32. Pages 1 and 2 are unchanged (9/0/9, 16/0/16), the other 16 scorecard rows are
byte-identical, and so are `determinism_gate`, `export_state_v1`, `golden_frames` and `scanline_goldens`.

### A5: a fill whose code names no write target writes nothing (test 34, closes F-FILLTGT)

**The rule, from the ROM's table.** **Test 34** (ROM `$45B2`) group 3 (`$4898..$4948`) arms a 4-byte fill
with `$40020082`. It then writes register `$8F02`, which leaves code `$22`: A2's rule, pinned by test 13,
is that a register write replaces CD1-CD0 with `10`. Then comes the `$68AC` trigger. On hardware VRAM
`$8000-$800F` reads back **unchanged**. Ours shows `5568 7768 9968 bb68`. Our trigger write is suppressed
by `code_names_a_write_target`, but the fill body resolves its target through `target_of`'s `_ => Vram`
fallback (`vdp.rs:714`, used at `vdp.rs:1395`) and writes VRAM. The hardware fill still **ran**: group 4
writes no length, and it fills well past 16 bytes, so group 3 had counted its length down to 0 (65,536).
The body therefore consumes its length and writes nowhere. That is exactly the question F-FILLTGT left
open ("one of the two decodes is wrong; the ROM does not cover the case"). The fill body must share the
write decode. EXP=32 flips exactly 34.

### M1: a DMA fill that runs over time (tests 31, 32, 33)

**The mechanism.** On hardware a fill takes time, and a data-port write made while it runs goes through
the FIFO; the fill then continues with a byte of the new word. Mask of Destiny, SpritesMind *Is DMA Fill
buggy?*: "If you write another word before the fill is complete, that write will take place as normal and
then the fill will continue with a byte from the new word." Nemesis, *VDP Internals* p.4: "The DMA fill
operation will effectively be suspended until the FIFO is empty again, and at that point, it will now pick
up its fill data from the last data that was moved through the FIFO."

**Ours.** The fill completes inside its trigger write: `MegaDriveBus::run_pending_dma` (`bus.rs:1463`) calls
`Vdp::run_fill` (`bus.rs:1471`), which writes every byte at once and only opens a busy window.

**Evidence.** **Test 31** (ROM `$2BD8`) group 2 waits for the start of active display, fills `$FFB` bytes
with `$12`, waits `$20` loop turns, and writes `$5678` mid-fill (`$2EF4`). Hardware's tail at `$8FF8` reads
`5656 5656 0056`. Ours reads `1212 1212 5678`: the fill finished with `$12` and the write landed after
it. Group 3 adds a second write, `$9ABC`. Hardware reads `9a9a 9a9a 9a9a`; ours reads `1212 1212 bc9a`.
**Tests 32 and 33** are the CRAM and VSRAM forms, with the same shape (`0666`/`0888` on hardware against
`0444`/`0ccc`/`0eee` on ours). No A rule moves them (EXP=63).

**Why M and not T.** The ROM reads the fill's tail, so exactly where the switch happens does not change the
tables. What is missing is the mechanism: a fill that exists across time and shares the FIFO. Timing only
matters once that mechanism exists, and then the per-line slot rate decides how far the fill has got (the
deferred "Phase 3 per-line DMA cost", and follow-up F-DMAHALT).

## Proposed queue rows

One row per A cause. Each is small, and each lists the tests it must flip and the ones it must not. The
scratch copy reproduced every flip with a rule of about five lines, and moved no frozen currency in
`oracle-core`.

| Row | Cause | Size | Touches | Must flip | Must not move |
|---|---|---|---|---|---|
| **VSRAM-DECODE** | A1 | M | `vdp.rs` `read_target` (:950) and `write_target` (:1014), plus a VSRAM read latch; the unit test at `vdp.rs:2072` | 23, 74 77 80 83 86 89 92 95, 96-122 (and 20 once A2 lands) | 13, any frozen golden |
| **DMA-SRC-128K** | A2 | S | `bus.rs:1502` (source step), `vdp.rs:1298` (`dma_complete` writing register 23) | 27 (and 20 with A1) | 24-26, 42-71 |
| **DMA-SRC-ADVANCE** | A3 | S | `vdp.rs` `run_fill` (:1392), `run_copy` (:1462): registers `0x15`/`0x16` | 28, 29 | 25, 26 |
| **FILL-BUSY-ARM** | A4 | S | `vdp.rs` `control_write`'s arm path (:1092), the busy window behind the status bit (:660) | 36, 38 | 35, 37, 39-41 |
| **FILL-TGT** | A5 | S | `vdp.rs` `run_fill` (:1395) to share `code_names_a_write_target`; `target_of` (:714); retire F-FILLTGT | 34 | 4, 72-95 |

VSRAM-DECODE is M rather than S because of the read half. Modelling a read latch adds machine state
(snapshot and bincode layout), and what the latch holds during active display is an open point. The write
half alone is S, but it must not land alone, because test 13 goes red. FILL-BUSY-ARM carries one question the
ROM does not answer: what ends the busy flag of a fill that is armed but never triggered. Test 38 shows only
that busy survives a register write and a half-command.

**M1, as a design item rather than a fix row: FILL-OVER-TIME** (size L). A fill (and a copy) that runs
across time instead of inside its trigger write, sharing the FIFO with port writes. It touches
`bus.rs` `run_pending_dma` and `vdp.rs` `run_fill`/`run_copy`, plus a DMA clock. It is the same design space
as the deferred per-line DMA cost and F-DMAHALT. Tests 31, 32 and 33 are its acceptance tables.

## What would settle the open points

There are no U failures. Two points inside A rows are open:

* **A1, the value of a VSRAM read at `$50-$7F`.** A ROM or instrument that makes VSRAM words 0 and 1 differ,
  then reads `$50` both in active display and in vblank, would separate "word 0", "word 1" and "the
  renderer's last fetch". **TAG: needs a live look** (a hardware capture or an owner-run instrument). No
  vendored ROM separates them.
* **A4, the end of an untriggered fill's busy flag.** A ROM that arms a fill, never triggers it, and polls
  status across a new command word and across frames.

## The pin

`conformance_roms::vdp_port_access_full_rom_verdicts` asserts that the failing set derived from the ROM's
own verdicts equals `PORT_ACCESS_FAILING`. Each entry is (test number, title, words off the ROM's table,
words in the table), and the list is exactly the 46 above. The test also asserts that the set is as long
as the failure count the ROM printed. `run_port_access` asserts that the run reaches 122, that there is one
record per test, and that the decoder reproduces all 22 printed tallies. Its doc comment says the list
records today and accepts nothing, under the owner's rule. A flip in either direction fails naming the test,
and so does a failing test whose wrong-word count moves. Mutation record and runs: see "Runs" below.

## The two-page scorecard row: widened, not retired

The row was `page1 pass/fail/total=9/0/9; pages1+2 cumulative=16/0/16` and is now
`page1 pass/fail/total=9/0/9; pages1+2 cumulative=16/0/16; all 22 pages cumulative=76/46/122`, read from
the shared run. It is kept because the scorecard is the one-line photograph of every ROM, and the page-1
and page-2 numbers are the ones this ROM's whole dated history in the ledger cites. They are unchanged
under the new paging, which is evidence the new paging did not disturb those pages. It is widened because
"16/0/16, this ROM is COMPLETE" was true of the two pages read, never of the ROM. The per-test detail
belongs in the pin, not in the row.

## Runs

**Wall time per test** (libtest's "finished in"; each test run alone with `--exact`, so it pays for the
shared run itself; load average 4.4-5.4 at the time):

| Test | Debug | Release |
|---|---:|---:|
| `vdp_port_access_full_rom_verdicts` | 2.68 s | 0.31 s |
| `vdp_port_access_copy_dma_matches_the_roms_own_tables` | 2.62 s | 0.30 s |
| `testrom_conformance_scorecard` (all 17 ROMs) | 16.25 s | 1.80 s |
| the whole `conformance_roms` binary (5 tests, one shared run) | 13.93 s | 1.47 s |

The ROM run itself is 617 frames. The paging bound is 5000 frames.

**Mutation record.** Each mutation was applied on disk, run (release; every run recompiled the test crate),
and restored from a copy of the committed file (`git show HEAD:…`), with `cmp` confirming the restore.
The script and its full output are in the parcel's scratch directory.

| # | Mutation (the line on disk) | Result |
|---|---|---|
| M1 | the `(34, "DMA Fill Control Port Writes", 4, 80)` entry deleted | red: `test 34 'DMA Fill Control Port Writes' NOW FAILS (4/80 words off the ROM's table)` |
| M2 | `(35, "DMA Busy Flag DMA Transfer", 0, 16),` added | red: `test 35 'DMA Busy Flag DMA Transfer' NOW PASSES` |
| M3 | test 20's entry changed to `7, 12` | red: `test 20 'DMA Transfer Source Wrapping' still fails, but 8/12 words are off the ROM's table (pinned: …, 7/12)` |
| M4 | `start: false, // MUTATION M4` (Start never pressed) | red in both tests that read the run: `vdp_port_access never reported 122 tests: 552 page stops recorded, the last tally Some((9, 0, 9)), after 5008 frames` |
| M5 | `rom_pass` compares only `expected[..2] == actual[..2]` | red: `the decoder's page split and verdicts must reproduce the tally the ROM printed at the end of every page` |
| M6 | `if rows_on_page + rows > 0x14` (a page of 20 rows) | red: the same page-tally assertion |

**The other ways the pin could go green.** A run that reads fewer tests fails M4's check. A verdict rule or
page split that is wrong fails the 22-tally check (M5, M6). A missing ROM prints `SKIP`, which
`vendor_data_present_when_running_in_ci` turns into a failure under CI. Locally, the
`vdp_port_access: 122 records, 22 pages, 617 frames; the ROM prints 76/46/122` line shows the run happened.

## The inventory

One row per test, generated from the `REC` lines `vdp_port_access_full_rom_verdicts` prints with
`--nocapture`. "Bytes" is the record's `data_len`. "Words off table" counts the words where the ROM's
expected table and the VDP's answer differ.

| # | Page | Title | Bytes | ROM verdict | Words off table | Cause |
|---:|---:|---|---:|---|---:|---|
| 1 | 1 | FIFO Buffer Size | 32 | pass | 0/16 |  |
| 2 | 1 | Separate FIFO Read/Write Buffer | 32 | pass | 0/16 |  |
| 3 | 1 | DMA Transfer using FIFO | 32 | pass | 0/16 |  |
| 4 | 1 | DMA Fill FIFO Usage | 32 | pass | 0/16 |  |
| 5 | 1 | FIFO Write to invalid target | 32 | pass | 0/16 |  |
| 6 | 1 | 8-bit VRAM Read target 01100 | 32 | pass | 0/16 |  |
| 7 | 1 | VRAM Byteswapping | 32 | pass | 0/16 |  |
| 8 | 1 | CRAM Byteswapping | 32 | pass | 0/16 |  |
| 9 | 1 | VSRAM Byteswapping | 32 | pass | 0/16 |  |
| 10 | 2 | Partial CP Writes | 28 | pass | 0/14 |  |
| 11 | 2 | Register Write Bit13 Masked | 32 | pass | 0/16 |  |
| 12 | 2 | Register Write Mode4 Mask | 32 | pass | 0/16 |  |
| 13 | 2 | Register Writes and Code Reg | 24 | pass | 0/12 |  |
| 14 | 2 | CP Write Pending Reset | 32 | pass | 0/16 |  |
| 15 | 2 | Read target switching | 32 | pass | 0/16 |  |
| 16 | 2 | FIFO Wait States | 80 | pass | 0/40 |  |
| 17 | 3 | HV Counter Latch | 80 | pass | 0/40 |  |
| 18 | 3 | HBlank/VBlank flags | 24 | pass | 0/12 |  |
| 19 | 3 | DMA Transfer Bus Locking | 128 | pass | 0/64 |  |
| 20 | 4 | DMA Transfer Source Wrapping | 24 | **FAIL** | 8/12 | A2 + A1 |
| 21 | 4 | DMA Transfer to VRAM Wrapping | 32 | pass | 0/16 |  |
| 22 | 4 | DMA Transfer to CRAM Wrapping | 32 | pass | 0/16 |  |
| 23 | 4 | DMA Transfer to VSRAM Wrapping | 32 | **FAIL** | 12/16 | A1 |
| 24 | 4 | DMA Transfer Length Reg Update | 32 | pass | 0/16 |  |
| 25 | 4 | DMA Fill Length Reg Update | 32 | pass | 0/16 |  |
| 26 | 4 | DMA Copy Length Reg Update | 32 | pass | 0/16 |  |
| 27 | 4 | DMA Transfer Source Reg Update | 32 | **FAIL** | 4/16 | A2 |
| 28 | 4 | DMA Fill Source Reg Update | 24 | **FAIL** | 8/12 | A3 |
| 29 | 4 | DMA Copy Source Reg Update | 24 | **FAIL** | 8/12 | A3 |
| 30 | 5 | FIFO Full Before DMA Transfer | 32 | pass | 0/16 |  |
| 31 | 5 | DP Writes During DMA Fill VRAM | 96 | **FAIL** | 6/48 | M1 |
| 32 | 5 | DP Writes During DMA Fill CRAM | 96 | **FAIL** | 6/48 | M1 |
| 33 | 5 | DP Writes During DMA Fill VSRAM | 96 | **FAIL** | 6/48 | M1 |
| 34 | 5 | DMA Fill Control Port Writes | 160 | **FAIL** | 4/80 | A5 |
| 35 | 6 | DMA Busy Flag DMA Transfer | 32 | pass | 0/16 |  |
| 36 | 6 | DMA Busy Flag DMA Fill | 32 | **FAIL** | 2/16 | A4 |
| 37 | 6 | DMA Busy Flag DMA Copy | 32 | pass | 0/16 |  |
| 38 | 6 | DMA Busy Flag DMA Toggle Fill | 64 | **FAIL** | 4/32 | A4 |
| 39 | 6 | DMA Busy Flag DMA Toggle Copy | 32 | pass | 0/16 |  |
| 40 | 6 | DMA Busy Flag DMA Disabled Fill | 32 | pass | 0/16 |  |
| 41 | 6 | DMA Busy Flag DMA Disabled Copy | 32 | pass | 0/16 |  |
| 42 | 7 | DMA Transfer to VRAM inc=0 | 32 | pass | 0/16 |  |
| 43 | 7 | DMA Transfer to CRAM inc=0 | 32 | pass | 0/16 |  |
| 44 | 7 | DMA Transfer to VSRAM inc=0 | 32 | pass | 0/16 |  |
| 45 | 7 | DMA Transfer to VRAM inc=1 | 32 | pass | 0/16 |  |
| 46 | 7 | DMA Transfer to CRAM inc=1 | 32 | pass | 0/16 |  |
| 47 | 7 | DMA Transfer to VSRAM inc=1 | 32 | pass | 0/16 |  |
| 48 | 7 | DMA Transfer to VRAM inc=2 | 32 | pass | 0/16 |  |
| 49 | 7 | DMA Transfer to CRAM inc=2 | 32 | pass | 0/16 |  |
| 50 | 7 | DMA Transfer to VSRAM inc=2 | 32 | pass | 0/16 |  |
| 51 | 8 | DMA Transfer to VRAM inc=3 | 32 | pass | 0/16 |  |
| 52 | 8 | DMA Transfer to CRAM inc=3 | 32 | pass | 0/16 |  |
| 53 | 8 | DMA Transfer to VSRAM inc=3 | 32 | pass | 0/16 |  |
| 54 | 8 | DMA Transfer to VRAM inc=4 | 32 | pass | 0/16 |  |
| 55 | 8 | DMA Transfer to CRAM inc=4 | 32 | pass | 0/16 |  |
| 56 | 8 | DMA Transfer to VSRAM inc=4 | 32 | pass | 0/16 |  |
| 57 | 9 | DMA Transfer to VRAM CD4=1 inc=0 | 32 | pass | 0/16 |  |
| 58 | 9 | DMA Transfer to CRAM CD4=1 inc=0 | 32 | pass | 0/16 |  |
| 59 | 9 | DMA Transfer to VSRAM CD4=1 inc=0 | 32 | pass | 0/16 |  |
| 60 | 9 | DMA Transfer to VRAM CD4=1 inc=1 | 32 | pass | 0/16 |  |
| 61 | 9 | DMA Transfer to CRAM CD4=1 inc=1 | 32 | pass | 0/16 |  |
| 62 | 9 | DMA Transfer to VSRAM CD4=1 inc=1 | 32 | pass | 0/16 |  |
| 63 | 9 | DMA Transfer to VRAM CD4=1 inc=2 | 32 | pass | 0/16 |  |
| 64 | 9 | DMA Transfer to CRAM CD4=1 inc=2 | 32 | pass | 0/16 |  |
| 65 | 9 | DMA Transfer to VSRAM CD4=1 inc=2 | 32 | pass | 0/16 |  |
| 66 | 10 | DMA Transfer to VRAM CD4=1 inc=3 | 32 | pass | 0/16 |  |
| 67 | 10 | DMA Transfer to CRAM CD4=1 inc=3 | 32 | pass | 0/16 |  |
| 68 | 10 | DMA Transfer to VSRAM CD4=1 inc=3 | 32 | pass | 0/16 |  |
| 69 | 10 | DMA Transfer to VRAM CD4=1 inc=4 | 32 | pass | 0/16 |  |
| 70 | 10 | DMA Transfer to CRAM CD4=1 inc=4 | 32 | pass | 0/16 |  |
| 71 | 10 | DMA Transfer to VSRAM CD4=1 inc=4 | 32 | pass | 0/16 |  |
| 72 | 11 | DMA Fill to VRAM inc=0 | 64 | pass | 0/32 |  |
| 73 | 11 | DMA Fill to CRAM inc=0 | 256 | pass | 0/128 |  |
| 74 | 11 | DMA Fill to VSRAM inc=0 | 256 | **FAIL** | 94/128 | A1 |
| 75 | 12 | DMA Fill to VRAM inc=1 | 64 | pass | 0/32 |  |
| 76 | 12 | DMA Fill to CRAM inc=1 | 256 | pass | 0/128 |  |
| 77 | 12 | DMA Fill to VSRAM inc=1 | 256 | **FAIL** | 85/128 | A1 |
| 78 | 13 | DMA Fill to VRAM inc=2 | 64 | pass | 0/32 |  |
| 79 | 13 | DMA Fill to CRAM inc=2 | 256 | pass | 0/128 |  |
| 80 | 13 | DMA Fill to VSRAM inc=2 | 256 | **FAIL** | 80/128 | A1 |
| 81 | 14 | DMA Fill to VRAM inc=4 | 64 | pass | 0/32 |  |
| 82 | 14 | DMA Fill to CRAM inc=4 | 256 | pass | 0/128 |  |
| 83 | 14 | DMA Fill to VSRAM inc=4 | 256 | **FAIL** | 88/128 | A1 |
| 84 | 15 | DMA Fill to VRAM CD4=1 inc=0 | 64 | pass | 0/32 |  |
| 85 | 15 | DMA Fill to CRAM CD4=1 inc=0 | 256 | pass | 0/128 |  |
| 86 | 15 | DMA Fill to VSRAM CD4=1 inc=0 | 256 | **FAIL** | 94/128 | A1 |
| 87 | 16 | DMA Fill to VRAM CD4=1 inc=1 | 64 | pass | 0/32 |  |
| 88 | 16 | DMA Fill to CRAM CD4=1 inc=1 | 256 | pass | 0/128 |  |
| 89 | 16 | DMA Fill to VSRAM CD4=1 inc=1 | 256 | **FAIL** | 85/128 | A1 |
| 90 | 17 | DMA Fill to VRAM CD4=1 inc=2 | 64 | pass | 0/32 |  |
| 91 | 17 | DMA Fill to CRAM CD4=1 inc=2 | 256 | pass | 0/128 |  |
| 92 | 17 | DMA Fill to VSRAM CD4=1 inc=2 | 256 | **FAIL** | 80/128 | A1 |
| 93 | 18 | DMA Fill to VRAM CD4=1 inc=4 | 64 | pass | 0/32 |  |
| 94 | 18 | DMA Fill to CRAM CD4=1 inc=4 | 256 | pass | 0/128 |  |
| 95 | 18 | DMA Fill to VSRAM CD4=1 inc=4 | 256 | **FAIL** | 88/128 | A1 |
| 96 | 19 | DMA Copy 9000 to 8000 inc=0 | 32 | **FAIL** | 4/16 | A1 |
| 97 | 19 | DMA Copy 9000 to 8000 inc=1 | 32 | **FAIL** | 4/16 | A1 |
| 98 | 19 | DMA Copy 9000 to 8000 inc=2 | 32 | **FAIL** | 4/16 | A1 |
| 99 | 19 | DMA Copy 9000 to 8000 inc=4 | 32 | **FAIL** | 4/16 | A1 |
| 100 | 19 | DMA Copy 8000 to 8002 for 0A | 32 | **FAIL** | 4/16 | A1 |
| 101 | 19 | DMA Copy 8000 to 8001 for 0A | 32 | **FAIL** | 4/16 | A1 |
| 102 | 19 | DMA Copy 8001 to 8003 for 0A | 32 | **FAIL** | 4/16 | A1 |
| 103 | 20 | DMA Copy 9000 to 8000 for 09 | 32 | **FAIL** | 4/16 | A1 |
| 104 | 20 | DMA Copy 9000 to 8001 for 09 | 32 | **FAIL** | 4/16 | A1 |
| 105 | 20 | DMA Copy 9001 to 8000 for 09 | 32 | **FAIL** | 4/16 | A1 |
| 106 | 20 | DMA Copy 9001 to 8001 for 09 | 32 | **FAIL** | 4/16 | A1 |
| 107 | 20 | DMA Copy 9000 to 8000 for 0A | 32 | **FAIL** | 4/16 | A1 |
| 108 | 20 | DMA Copy 9000 to 8001 for 0A | 32 | **FAIL** | 4/16 | A1 |
| 109 | 20 | DMA Copy 9001 to 8000 for 0A | 32 | **FAIL** | 4/16 | A1 |
| 110 | 20 | DMA Copy 9001 to 8001 for 0A | 32 | **FAIL** | 4/16 | A1 |
| 111 | 21 | DMA Copy 9000 to 8000 CD0-3=0000 | 32 | **FAIL** | 4/16 | A1 |
| 112 | 21 | DMA Copy 9000 to 8000 CD0-3=0001 | 32 | **FAIL** | 4/16 | A1 |
| 113 | 21 | DMA Copy 9000 to 8000 CD0-3=0011 | 32 | **FAIL** | 4/16 | A1 |
| 114 | 21 | DMA Copy 9000 to 8000 CD0-3=0100 | 32 | **FAIL** | 4/16 | A1 |
| 115 | 21 | DMA Copy 9000 to 8000 CD0-3=0101 | 32 | **FAIL** | 4/16 | A1 |
| 116 | 21 | DMA Copy 9000 to 8000 CD0-3=0111 | 32 | **FAIL** | 4/16 | A1 |
| 117 | 22 | DMA Copy 9000 to 8000 CD0-3=1000 | 32 | **FAIL** | 4/16 | A1 |
| 118 | 22 | DMA Copy 9000 to 8000 CD0-3=1001 | 32 | **FAIL** | 4/16 | A1 |
| 119 | 22 | DMA Copy 9000 to 8000 CD0-3=1011 | 32 | **FAIL** | 4/16 | A1 |
| 120 | 22 | DMA Copy 9000 to 8000 CD0-3=1100 | 32 | **FAIL** | 4/16 | A1 |
| 121 | 22 | DMA Copy 9000 to 8000 CD0-3=1101 | 32 | **FAIL** | 4/16 | A1 |
| 122 | 22 | DMA Copy 9000 to 8000 CD0-3=1111 | 32 | **FAIL** | 4/16 | A1 |
