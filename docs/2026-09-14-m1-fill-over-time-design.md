# M1-FILL-OVER-TIME: a DMA fill that runs across time and shares the FIFO (design)

> **Dated 2026-09-14.** Parcel M1-FILL-OVER-TIME, branch `parcel/m1-fill-over-time-design`, brief
> `docs/2026-09-14-m1-fill-over-time-design-brief.md`. **Design document, no fix.** Spikes are committed as
> `spike:` and removed from the tip. DRAFT: sections marked PENDING are filled as measurements land.

## Summary (read this if nothing else)

PENDING.

## Where the brief was wrong (measured)

1. **"Where exactly the switch happens does not change the tables" is only half true.** The tail words
   do not depend on the switch point, but the **head** words do. Tests 31/32/33 read the fill's first four
   words back as well as its tail, and those must still hold the *old* fill data. That puts a lower bound on
   the switch: the mid-fill write must land after at least **7 fill steps** have run (derivation in §1.3). An
   upper bound comes from the tail: the write must land before the last step. So timing matters inside a
   window, and the model's slot rate must put the write inside it. In test 32/33 group 2 the margin is small
   (the ROM waits 25 `dbra` turns, about 262 CPU cycles; see §5).
2. **The fill's data and target are not the trigger word's, they are the FIFO's.** Nemesis, *VDP
   Internals* p.4, says the DMA unit "will pull the write target and the upper byte of the write data from
   the FIFO entry". `DmaRequest::Fill { fill }` carries the trigger word as a copy. For an instant fill the
   two agree. For a fill that runs over time they do not, and the ROM's tables side with the FIFO (§1.2).
3. **A mid-fill CPU write does not consume a length count.** The brief left "which byte" open and did not
   ask this. Test 31 group 2 pins it: if the write consumed a count the tail would read `5656 5656 0000
   0000`; hardware reads `5656 5656 0056 0000` (§1.3).

## 1. The hardware behaviour, from documentation and the ROM

### 1.2 Fill data after a mid-fill write

Test 31 (VRAM): hardware continues with the **high byte of the new word**: `$56` from `$5678` (group 2),
`$9A` from `$9ABC` (group 3). That is the most recently written FIFO entry, the same place the trigger's
`$12` came from. Tests 32/33 (CRAM/VSRAM): hardware continues with the **next-available ring entry** (the
word written four writes ago), re-read after the CPU write: `0666` after `$0CCC` is written into the slot
that held `0444`, and `0888` after `$0EEE` as well. That is the documented CRAM/VSRAM fill bug applied to
the ring as it stands after the write.

### 1.3 Derivation of test 31 group 2 (ROM `$2E5E..$2F06`)

PENDING write-up (derived by hand from the disassembly; see commit message).
