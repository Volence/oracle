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

## 6. Who else changes behaviour (population, measured; draft numbers, instrument at `c60d3b0`)

Observer-only spike, release profile, every suite green with it armed (so it is inert). One row per
DMA fill or copy: the window today's model opens (`dma_cost`) and the one the proposed model would
take (pending FIFO entries drain first, then one external slot per step on the published slot
positions and the flat blanked rate). Counted inside the proposed window: every VDP port access, every
DMA started, and every active line rendered.

* **aeon replay fixtures** (`replay_real_artifacts`, 16 tests): 10 fills, all 64 KiB VRAM clears with
  the display off. Proposed window 1,093,302 mclk against today's 1,093,315 (13 mclk shorter). **No port
  access, no DMA and no displayed line inside any of them.** No replay verdict is at risk.
* **Vendored ROMs other than VDPFIFOTesting**: every fill is a 64 KiB display-off clear at boot, plus
  io_sample's 313 fills (308 of 462 bytes). Windows agree to 2-15 mclk. m68k_bcd (5,578 polls) and
  io_sample (19,289 polls) poll status inside the window, and **no poll reads a different busy bit**
  under the two models. No data write, data read, control write or DMA inside any window.
* **VDPFIFOTesting**: 73 fills, 34 copies. Exactly **6 fills see mid-fill data-port writes (9 writes)**:
  tests 31/32/33, one write in group 2 and two in group 3. No data read, control write or DMA inside a
  fill window. 50 fills have polls whose busy bit differs between the models (the per-slot rate against
  today's flat rate taken at the start instant); 34 fills overlap displayed lines. The longest proposed
  window is 5,038,907 mclk (test 34 group 4's 64 KiB fill, display on) against today's 12,451,840.
* `golden_frames`, `determinism_gate` and `export_state_v1` run no fill through the bus at all.
