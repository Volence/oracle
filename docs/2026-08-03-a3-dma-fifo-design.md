# A3 — DMA routed through the write FIFO (VDPFIFOTesting tests 3 and 4)

**Status:** design / recon. No code written. Written for the Arc-A slice A3 of
`docs/plans/2026-08-03-fifo-scanline-arcs.md`.

**Verdict up front.** Both failures are explained, and both fixes are small. Test 3 needs
**one** thing: the words a 68k→VDP DMA moves must land in the physical 4-slot FIFO ring, because
the ROM reads the ring back through the CRAM/VSRAM undefined-bit snoop. Test 4 needs **two**
things: the fill's triggering data-port write must be applied as a *normal full-word write*
(today it is swallowed), and the fill engine's byte writes must go to `address ^ 1`.

**"Decision 1" (write-at-enqueue) SURVIVES intact.** Neither test observes write *ordering*;
both observe FIFO *contents* and final memory. No deferred-write pipeline is needed. See §3.1.

**One hard blast-radius finding, needs an owner ruling before A3b lands:** the test-4 fill fix
changes the vendored fixture ROM's VRAM by one byte and therefore **moves
`export_state_v1::GOLDEN_HASH`**, which the arc's ground rules list as frozen. Details + options
in §4.1. Recommendation: split A3 into A3a (test 3, provably currency-neutral) and A3b (test 4,
one documented, evidenced golden regen).

---

## 1. Method and evidence

### 1.1 How the ROM was read

`vendor/TestRoms/vdp_port_access.bin` (Nemesis, *VDPFIFOTesting*, 524288 bytes, header
`SEGA GENESIS (C)T-xx 2011.11 VDP Port Access Test ROM`). Two tools, both in the scratchpad, both
throwaway:

* raw byte/word dumps via `python3` (`struct.unpack_from('>H', …)` — the 68k is big-endian);
* a 68000 disassembler built on `capstone` (`CS_ARCH_M68K` / `CS_MODE_M68K_000`), driven from
  python over the flat ROM image (ROM address == file offset for a Mega Drive cartridge).

No emulator source was read. `/home/volence/sonic_hacks/oracle/` was never opened.

### 1.2 The record format, and why the table offsets are authoritative

Each test builds a result record in a RAM buffer (`a0`), then the ROM's own comparison/render pass
diffs the *expected* half against the *actual* half. The two table addresses are not guessed — the
ROM loads them with literal `lea`s, which is the ground truth:

```
005E3C: 43f900005de8   lea.l  $5de8.l,a1     ; test 3: 36-byte name string
005E50: 43f900005e0c   lea.l  $5e0c.l,a1     ; test 3: 32 bytes = 16 expected words
00DC84: 43f90000dc30   lea.l  $dc30.l,a1     ; test 4: 36-byte name string
00DC98: 43f90000dc54   lea.l  $dc54.l,a1     ; test 4: 32 bytes = 16 expected words
```

So the `$5DE8` / `$DC30` offsets quoted in the arc plan are the **name** strings
(`"DMA Transfer using FIFO"`, `"DMA Fill FIFO Usage"`, both space-padded to 36); the expected-value
tables start 36 bytes later.

```
ROM $5E0C (test 3, 16 words):  c800 c800 c000 c000 d800 d800 d111 d111
                               e800 e800 e000 e000 f800 f800 f111 f111
ROM $DC54 (test 4, 16 words):  0000 0000 0000 0000 0000 0000 1000 1000
                               1234 1212 1212 1212 1212 0012 0000 0000
```

These are hardware answers captured by the ROM's author on real silicon; they are the spec.

### 1.3 Test 3 — the exact port sequence

Disassembled `$5E60`…`$60AC` (`rts`). Ancillary data: the DMA source payload sits immediately
before the name string at `$5DDC`: `AAAA BBBB CCCC DDDD EEEE FFFF`.

| PC | port traffic | meaning |
|---|---|---|
| `$5E60` | ctrl `$C000`,`$0000` | CD = `000011` = **CRAM write**, addr `$0000` |
| `$5E6A` | 64 × data `$0000` | zero all 128 bytes of CRAM |
| `$5E7A` | ctrl `$4000`,`$0002` | CD = `000001` = **VRAM write**, addr `$8000` |
| `$5E84` | data `$1000`,`$2000`,`$3000`,`$4000`,`$5000`,`$6000` | **six marker words** into the FIFO |
| `$5EBE` | reg 1 = `$54` | display on + **M1 (DMA enable)** |
| `$5EC6` | reg 15 = `$02` | autoinc 2 |
| `$5ECE`…`$5F0C` | regs 19,20 = `$0006`; regs 21,22,23 = `$5DDC >> 1` | length 6 words, source `$5DDC`, mode Mem (reg 23 bit 7 = 0) |
| `$5F18` | ctrl `$4000`,`$0082` (one `move.l`) | CD = `100001` = VRAM write **+ CD5** → fires the 68k→VDP DMA to addr `$8000` |
| `$5F1E` | reg 1 = `$44` | M1 off |
| then ×4 | ctrl `$0000`,`$0010` → 2 × data read; ctrl `$0000`,`$0020` → 2 × data read; ctrl `$C020`,`$0000` → data `$FFFF` | CD `000100` = **VSRAM read** @0; CD `001000` = **CRAM read** @0; CRAM write `$FFFF` @ `$0020` |

16 reads, in four groups of `{VSRAM, VSRAM, CRAM, CRAM}`, with a single CRAM data-port write of
`$FFFF` separating each group from the next. Nothing is ever read back from VRAM — the DMA's
destination contents are irrelevant to this test.

### 1.4 Decoding test 3's expected words

CRAM defines 9 bits (`$0EEE`), VSRAM defines 11 (`$07FF`); everything else in the 16-bit result is
undefined and is filled from the FIFO's **next-available entry** (the slot about to be overwritten
= the word written four writes ago). Both memories were zeroed at addr 0, so *every* expected word
is purely a snoop readout:

| group | expected | mask applied | implied snoop word |
|---|---|---|---|
| 1 VSRAM ×2 | `c800` | `& $F800` | `$CCCC` |
| 1 CRAM ×2 | `c000` | `& $F111` | `$CCCC` |
| 2 | `d800` / `d111` | | `$DDDD` |
| 3 | `e800` / `e000` | | `$EEEE` |
| 4 | `f800` / `f111` | | `$FFFF` |

Read that column bottom-up and the whole test states one sentence: **after the DMA, the four
physical FIFO slots hold the DMA's last four payload words — `CCCC DDDD EEEE FFFF` — with the
write cursor parked on `CCCC`.** Each intervening `$FFFF` CRAM write advances the cursor by one
slot, walking the snoop through `CCCC → DDDD → EEEE → FFFF`. The six marker words have been
completely displaced.

Note also that the FIFO stores the **raw, unmasked** word: group 4's `f111`/`f800` comes from the
`$FFFF` that CRAM stored as `$0EEE`. Our `fifo_enqueue` already records the raw word.

### 1.5 What we produce today, and why (predicted from source, matches the A0 probe)

`Vdp::dma_write_word` (`vdp.rs:730-735`) calls `write_target` + `autoinc` and never touches the
ring. So the ring still holds the last four *marker* writes, `$3000 $4000 $5000 $6000`, and the
same walk yields:

```
3000 3000 3000 3000 4000 4000 4000 4000 5000 5000 5000 5000 6000 6000 6000 6000
```

which is exactly the failure the A0 probe reported. That agreement — a value predicted from the
ROM + our source, matching an independently observed probe — is the main cross-check that the
decode chain above (CD decode, snoop model, masks, ring cursor) is right.

### 1.6 Test 4 — the exact port sequence

Disassembled `$DCA8`…`$DEAE` (`rts`).

| PC | port traffic | meaning |
|---|---|---|
| `$DCA8` | ctrl `$4000`,`$0002` | VRAM write, addr `$8000` |
| `$DCB2` | 8 × data `$0000` | zero VRAM `$8000`…`$800F` (autoinc 2, inherited) |
| `$DCF6` | reg 15 = `$01` | **autoinc 1** |
| `$DCFE` | reg 1 = `$54` | M1 on |
| `$DD06` | regs 19,20 = `$000A` | fill length **10** |
| `$DD20` | reg 23 = `$80` | **fill mode** |
| `$DD56` | ctrl `$4000`,`$0082` | CD = `100001`, addr `$8000` — arms the fill |
| `$DD5C` | **data `$1234`** | the fill trigger |
| `$DD64` | poll status until bit 1 (DMA busy) clears | |
| `$DD7A` | reg 15 = `$02` | autoinc 2 |
| then ×4 | ctrl `$0000`,`$0010` → 2 × data read; ctrl `$C020`,`$0000` → data `$FFFF` | VSRAM-read snoop probes + a ring-advancing CRAM write |
| `$DE60` | ctrl `$0000`,`$0002` → 8 × data read | **VRAM read** @ `$8000`, 8 words |

So test 4's 16 expected words split into two halves: **words 0–7 are snoop probes** (does the fill
put anything in the ring beyond its trigger word?) and **words 8–15 are the resulting VRAM image**
at `$8000`…`$800F`.

### 1.7 Decoding test 4's expected words

**Snoop half (`0000 0000 0000 0000 0000 0000 1000 1000`).** VSRAM at 0 is zero, so the defined bits
contribute nothing; the undefined bits are `word & $F800`. Groups 1–3 read `0000`; group 4 reads
`1000`, and `$1234 & $F800 == $1000`. Writes in flight before the probes: eight `$0000`s, then the
trigger `$1234`. Cursor lands on the third-from-last zero, then walks `0 → 0 → 0 → $1234`. Meaning:
**the fill's trigger word occupies exactly one FIFO slot, and the fill's own replicated bytes
occupy none.** We already satisfy this half (`apply_data_write` enqueues the trigger, `run_fill`
does not enqueue) — which is why only two of test 4's sixteen words are wrong today.

**VRAM half (`1234 1212 1212 1212 1212 0012 0000 0000`).** As bytes, `$8000`…`$800F`:

```
12 34 12 12 12 12 12 12 12 12 00 12 00 00 00 00
```

Reconstructing the only sequence that produces this, given addr `$8000`, autoinc 1, length 10,
trigger `$1234`, from an all-zero start:

1. The trigger write is a **normal word write**: MSB `$12` → `$8000`, LSB `$34` → `$8000 ^ 1` =
   `$8001`. That is the only way `$8001` can hold `$34`. Then the address auto-increments to
   `$8001`.
2. The fill engine then performs **10** byte writes of the MSB `$12`, each to **`address ^ 1`**,
   incrementing the address by the autoincrement after each. Addresses visited `$8001`…`$800A`;
   bytes actually written `$8000, $8003, $8002, $8005, $8004, $8007, $8006, $8009, $8008, $800B`.

That union is `{$8000} ∪ {$8002…$8009} ∪ {$800B}` — leaving `$800A` at zero and touching `$800B`,
which is precisely the `0012` at word 13 and the `1212`s in between. The autoincrement in step 1 is
also *forced* by the data: without it, the fill's first step would land on `$8001` and destroy the
`$34`.

I re-ran both models numerically against the ROM's tables; new-model output is byte-identical to
both expected tables, and the old-model output is byte-identical to the reported failure
(`1212` at word 8, `0000` at word 13).

### 1.8 Independent corroboration from public documentation

The two behaviors above are documented by the same author who wrote this ROM, plus one
corroborating maintainer note. Quoted verbatim, clean-room (forum prose only — no emulator source
was consulted):

* **Nemesis**, *VDP Internals* (SpritesMind), on the snoop the tests read through:
  > "If you're reading from a target like VSRAM or CRAM, which has some bits in the 16-bit wide
  > result that are undefined in the target memory, those undefined bits are actually initialized
  > to the content on the next available FIFO entry (the one containing the data written to control
  > port four writes ago)."
  and, on why this ROM matters:
  > "A lot of the tests in this ROM rely on this being done correctly, as a lot of the tests verify
  > the resulting FIFO state."

* **Nemesis**, same thread, on 68k→VDP DMA (the test-3 pin):
  > "A DMA transfer is similar, if CD5 is set and DMD1 is clear, and there's an available slot in
  > the FIFO, it will read a value from external memory using the DMA source address register and
  > **add it to the FIFO** using the current command code and incremented command address
  > registers, then it runs the standard set of DMA advance operations."

* **Nemesis**, same thread, on the fill trigger (the first half of the test-4 pin):
  > "When a DMA Fill operation is pending, and you perform a data port write, that data port write
  > is completed as normal… That pending write is then pulled out of the FIFO, and processed as a
  > normal FIFO write."
  and on the fill engine:
  > "if CD5 is set, and DMD1 is true, and DMD0 is false, the DMA unit will pull the write target and
  > the upper byte of the write data from the FIFO entry, and write that single byte to the write
  > target, using the current incremented command address register, which will then be incremented
  > afterwards."

* **Mask of Destiny**, *Is DMA Fill buggy?* (SpritesMind), the `^ 1` statement (second half of the
  test-4 pin):
  > "MSB of the word in the FIFO is written to the saved address… LSB of the word in the FIFO is
  > written to the saved address ^ 1… Actual fill starts, MSB of the word in the FIFO is written DMA
  > length times to address ^ 1"

* **Eke** (same thread), root cause and its reach:
  > "VRAM byte writes (used by VRAM fill and copy DMA) actually occur to VRAM address ^ 1 so you can
  > get unexpected results depending on start address, DMA length and increment alignments."

* **Kabuto's hardware notes** (Plutiedev mirror), corroborating that DMA shares the FIFO:
  > "When writing a value to the VDP's data port (or the VDP does that internally through DMA) both
  > value and current address are appended to its internal FIFO."

Sources:
[Nemesis, VDP Internals p.3](https://gendev.spritesmind.net/forum/viewtopic.php?t=1291&start=43) ·
[Is DMA Fill buggy?](https://gendev.spritesmind.net/forum/viewtopic.php?t=2663) ·
[Kabuto's hardware notes (Plutiedev mirror)](https://plutiedev.com/mirror/kabuto-hardware-notes)

---

## 2. The behavior being pinned (implementer-facing prose)

**P1 — DMA payload words occupy FIFO slots.** Every word a 68k→VDP (Mem) DMA moves is appended to
the same physical 4-slot write FIFO a CPU data-port write uses, carrying the command code and
address in force at that moment. After an *N*-word DMA with *N* ≥ 4, the ring holds the last four
payload words and the write cursor points at the oldest of them. This is observable — and is
observed by test 3 — through the CRAM/VSRAM undefined-bit snoop, which reads the slot the cursor
points at.

**P2 — A DMA-fill's trigger is an ordinary write.** The data-port write that fires a pending VRAM
fill is not swallowed: it is enqueued into the FIFO (one slot, as today) **and applied to the
current target exactly as a non-DMA write would be** — for VRAM, MSB to `address`, LSB to
`address ^ 1` — and then the address auto-increments.

**P3 — The fill engine writes to `address ^ 1`.** Starting from the (already incremented) address,
the fill performs `length` single-byte writes of the fill data's **upper byte**, each to
`address ^ 1`, incrementing the address by the autoincrement value after each write. With an odd
autoincrement this produces the characteristic interleave — a skipped byte at the tail and one byte
written past the naive end — that test 4 checks.

**P4 — The fill engine adds nothing to the FIFO.** Its replicated bytes are pulled *from* a FIFO
entry, not pushed into one. The ring's contents and cursor are untouched for the whole fill.

Unchanged and explicitly out of scope: the CRAM/VSRAM fill data source (still the next-available
"four writes ago" entry, recon R4(b)); VRAM copy (still FIFO-bypassing); all drain/wait-state
timing; the coarse `dma_busy_until` window.

---

## 3. Design — the minimal change set

### 3.1 Does "Decision 1" (apply-at-enqueue) survive? Yes.

The arc plan hypothesized that test 3 needs the DMA payload to *interleave* with pending writes in
memory. It does not. Test 3 never reads its DMA destination — its sixteen observations are sixteen
reads of CRAM/VSRAM at address 0, all of whose *defined* bits are zero. Every expected bit comes
from FIFO **contents**, not from write ordering. Test 4 likewise reads a settled VRAM image after
polling DMA-busy to completion.

So no deferred-write pipeline is required, no drain-time application, and no change at all to the
normal data-port write path's *ordering*. The FIFO keeps carrying "timing + contents"; A3 only
widens *which* writes contribute contents, and fixes two byte-placement/omission bugs in the fill.

### 3.2 Change 1 — split ring-store from pending-count (`vdp.rs:443-451`)

Today `fifo_enqueue` does two things at once. Split them:

```rust
/// Store `data` (plus the live code/address) into the next physical ring slot and advance the
/// write cursor. The *pending count* is untouched: this is the physical slot write, which a DMA
/// payload word performs without ever occupying a pending-drain slot in our synchronous model.
fn fifo_store(&mut self, data: u16) {
    self.fifo[self.fifo_write as usize] = FifoEntry { data, code: self.code, addr: self.addr };
    self.fifo_write = (self.fifo_write + 1) & 3;
}

/// A CPU data-port write: store into the ring AND count it as pending (drain/stall timing).
fn fifo_enqueue(&mut self, data: u16) {
    self.fifo_store(data);
    self.fifo_len = (self.fifo_len + 1).min(4);
}
```

No caller of `fifo_enqueue` changes.

### 3.3 Change 2 — DMA payload words enter the ring (`vdp.rs:730-735`)

```rust
pub fn dma_write_word(&mut self, w: u16) {
    self.in_dma = true;
    self.fifo_store(w);   // P1: the payload occupies a physical FIFO slot (snoop/fill source)
    self.write_target(w);
    self.in_dma = false;
    self.autoinc();
}
```

Signature unchanged; `bus.rs:653-680` (`run_mem_dma`) needs no edit. That is the entire test-3 fix.

**Why `fifo_store` and not `fifo_enqueue`** (i.e. why the pending count is deliberately not bumped):
we execute a Mem DMA synchronously inside one bus access and bill its elapsed time through
`dma_cost` + the returned halt wait. Bumping `fifo_len` would leave four phantom pending entries
after every DMA that no clock has been advanced past, so the *next* data-port write in every
DMA-using game would take a spurious `/DTACK` stall. That is a timing change with real blast radius
(every conformance ROM) bought for no test-3 benefit. Keeping the count at zero also matches the
physical story already documented on the `FifoEntry` type: "the physical slot **retains** its data
after the entry drains". Recorded as open question **Q1** (§6) — it interacts with slice A1/T16.

### 3.4 Change 3 — the fill trigger becomes a real write (`vdp.rs:789-805`)

`apply_data_write`'s CD5 arm currently enqueues and returns, dropping the write:

```rust
    if self.code & 0x20 != 0 {
        if self.regs[0x17] & 0xC0 == 0x80 {
            self.fifo_enqueue(w);
            let len = ((self.regs[0x14] as u16) << 8) | self.regs[0x13] as u16;
            // P2: the trigger is completed as a NORMAL write before the fill starts —
            // MSB → addr, LSB → addr ^ 1 for VRAM — and the address then auto-increments,
            // so the fill's first replicated byte lands back on the start address.
            self.write_target(w);
            self.autoinc();
            self.dma_pending = Some(DmaRequest::Fill { len, fill: w });
        }
        return;
    }
```

Exactly one enqueue, so the snoop half of test 4 (which passes today) is preserved. Applying the
priming write for CRAM/VSRAM fill targets too is the consistent reading of the citation, but the
ROM does not cover it — see **Q3**.

### 3.5 Change 4 — the fill engine writes to `addr ^ 1` (`vdp.rs:817-824`)

```rust
            Target::Vram => {
                let byte = (fill >> 8) as u8;
                for _ in 0..count {
                    // P3: a VRAM *byte* write from the fill/copy engine lands at address ^ 1.
                    self.write_vram_byte((self.addr ^ 1) as usize & (VRAM_SIZE - 1), byte);
                    self.autoinc();
                }
            }
```

The CRAM/VSRAM arm of `run_fill` is untouched (word-addressed; no byte lane, no `^ 1`). The SAT
write-through still fires because the write still routes through `write_vram_byte` — it now fires
on the correct byte.

### 3.6 Data flow summary

```
CPU data-port write  →  data_write_at → apply_data_write → fifo_enqueue(ring + count) → write_target → autoinc
CD5 fill trigger     →  data_write_at → apply_data_write → fifo_enqueue(ring + count) → write_target → autoinc → arm Fill
                                                                                                  ↓  (bus: run_pending_dma)
                                                                                        run_fill: N × write_vram_byte(addr^1) + autoinc
                                                                                        (ring untouched — P4)
CD5 Mem DMA trigger  →  control_write → arm_dma → (bus) run_mem_dma → N × dma_write_word
                                                                       → fifo_store(ring only) → write_target → autoinc
```

Four edits, all in `vdp.rs`; `bus.rs` unchanged. No new fields, no new public API, no signature
changes, no floats.

---

## 4. Risk and blast radius

### 4.1 The one real problem: `export_state_v1::GOLDEN_HASH` moves (change 3 + 4)

> **CORRECTION, 2026-08-03 (slice A3b, measured — this section is WRONG).** `GOLDEN_HASH` does **not**
> move; A3b landed currency-neutral. The lines cited below (`testrom.rs:255-263`) are inside
> **`testrom::build_pad_poll`**, a fixture used only by `io_controllers.rs`, `watchpoints.rs` and the
> `pad_probe` example. The `export_state_v1` golden fixture is **`testrom::build`** — the RAM-stirring ROM
> at `$200`, which never writes a VDP port. Probed directly on the frozen fixture (seed
> `0xD8_5EED_0F1C_ED01`, 60 frames): every VDP register reads `$00` and VRAM is untouched power-on noise
> (254 zero bytes out of 65536, `vram[$FFFF] == $3B` **both before and after** the fix, because no fill
> ever runs). `GOLDEN_HASH` stays `0xBF5D_1E1A_A727_143B`, and every currency suite passed unmodified.
> The `$3B → $00` reasoning below is sound *for the pad-poll fixture* — that byte does change there — but
> `$FFFF` is outside every plane/SAT window, so its render hash and watchpoint counts are unchanged too.
> Everything else in this document (§1-§3, §5) was verified correct in implementation. Consequently
> §4.2's `A3b` column, and the "recommended slicing" note's premise, are the only wrong rows.

The arc's ground rules inherit the push-6 assumption that "the golden fixture drives no VDP DMA".
**That assumption is now false.** `crates/oracle-core/src/testrom.rs:255-263` zeroes VRAM with a
DMA fill:

```
reg 15 = 1 (autoinc 1);  regs 19/20 = $FFFF;  reg 23 = $80 (fill);  cmd VRAM write @ $0000 + CD5;  data $0000
```

* **Today:** `run_fill` writes bytes `$0000`…`$FFFE`; **`vram[$FFFF]` is never written** and keeps
  its power-on pseudo-random value. For the fixture seed `0xD8_5EED_0F1C_ED01` that byte is
  **`$3B`** (verified by replaying `SplitMix64` in the documented draw order: work RAM `0x10000`
  bytes first, then VRAM).
* **After the fix:** the trigger word zeroes `$0000`/`$0001`, and the 65535 fill steps at addresses
  `$0001`…`$FFFF` write `address ^ 1` = `{$0000} ∪ {$0002…$FFFF}`. The whole 64 KiB ends up zero,
  including `$FFFF`.

`export_state` serializes full VRAM, so `GOLDEN_HASH = 0xBF5D_1E1A_A727_143B` **will change**. This
is a value change, not a layout change, so `EXPORT_STATE_VERSION` does *not* bump (the offsets and
sizes are untouched).

**Options.**

1. **Accept and regenerate `GOLDEN_HASH` in the same commit, with the evidence in the commit
   message** (the ROM table at `$DC54`, the two citations, the `$3B → $00` byte). This is the
   procedure `export_state_v1.rs`'s own header prescribes for a deliberate change, and it is the
   honest option: the current golden encodes a *bug* (a stray random byte at the top of VRAM that
   real hardware would have cleared). **Recommended.**
2. Do not fix the fill; leave test 4 red. Costs the slice its entire purpose.
3. Change `testrom.rs`'s fill parameters so old and new coincide. Also moves the golden (different
   fixture), and hides the fix. Rejected.

**Recommended slicing:** make this an explicit two-commit slice.

* **A3a — test 3 only** (changes 1 + 2). Provably currency-neutral: touches `fifo`/`fifo_write`
  only, and neither is in `state_hash` (vram/cram/vsram/regs) nor in the `export_state` VDP region.
  Every currency gate must stay byte-identical with its existing constant.
* **A3b — test 4 only** (changes 3 + 4). Carries the single documented `GOLDEN_HASH` regen. Every
  *other* currency stays byte-identical.

That way the one currency movement in the arc is isolated to one commit with one cause.

### 4.2 Currency-by-currency

| gate | A3a | A3b |
|---|---|---|
| `export_state_v1::GOLDEN_HASH` | unchanged (no hashed region touched) | **moves** — see §4.1 |
| `oracle_differential` (`ORACLE_REGS/CRAM/VSRAM_HASH`) | unchanged — the test hashes *captured static byte arrays*, it never runs the emulator | unchanged, same reason |
| `golden_frames` (6 scenes) | unchanged — the scenes build a `Vdp` through the ports directly; grep confirms **zero** DMA/fill usage in `tests/golden_frames.rs` | unchanged, same reason |
| `state_hash.rs` unit goldens | unchanged — synthetic all-zero / patterned buffers | unchanged |
| `determinism_gate`, `proptests` | self-consistency (run *n* vs *n*×1, snapshot round-trip); no absolute pins. Must stay green | same |
| `singlestep_m68000` (≥ 1,000,058) | untouched — `m68000/*` zero-diff, `FlatBus` has no VDP | same |

Nothing is added to either currency. No new serialized field is introduced at all, so the
"round-trips bincode" obligation is trivially met by the existing fields.

### 4.3 Existing unit tests that will move (all in A3b; A3a should move none)

* `bus.rs:1933 vram_fill_fills_the_target_with_the_top_byte` — fills 8 bytes of `$EE` from word
  `$EEAA` at `$0100`, autoinc 1, and asserts `vram[$0100..$0108]` are all `$EE`. Under P2/P3 the
  image becomes `EE AA EE EE EE EE EE EE 00 EE` — `$0101` holds the trigger's LSB, `$0108` is
  skipped, `$0109` is written. **Rewrite the assertion to the exact new image** (do not weaken it).
* `bus.rs:1968 fill_updates_the_sat_cache_on_window_hits` — fills 4 bytes of `$77` from `$77AA` at
  `$0000`; `sat_cache[0..4]` becomes `77 AA 77 77`. Rewrite; the *point* of the test (fill bytes hit
  the write-through window) still holds.
* `vdp.rs:2323 armed_captures_a_dma_fill_with_via_dma` — asserts each capture's `addr == 0x0200 + i`.
  Becomes `0x0201, 0x0200, 0x0203, 0x0202`. Rewrite the address expectation; `via == Dma`, `size`,
  `old`/`new` all still hold. (It calls `run_fill` directly, so it is unaffected by change 3.)

Explicitly **not** affected: `vdp.rs:2060 cram_fill_uses_the_four_writes_ago_entry` (CRAM arm, calls
`run_fill` directly, no `^ 1`); `bus.rs:1948 fill_sets_dma_busy_…`; `bus.rs:2009/2041` copy tests;
`bus.rs:1842-1908` Mem-DMA tests (a ring store changes no VRAM byte, no register, no wait);
`vdp.rs:2089 a_completed_dma_clears_cd5_…` (the DR-2 CD5 fix is untouched — `take_dma_request` is
not modified).

### 4.4 Conformance scorecard and the differential ROMs

* `conformance_roms.rs` BASELINE: the `vdp_port_access` row is the *only* row that may move.
  Expected new value: **`page1 pass/fail/total=8/1/9; pages1+2 cumulative=11/5/16`** (page 1's three
  failures are tests 3, 4 and 6; A3 fixes two of them, leaving 6 for slice A4). Any other row
  moving = stop and triage. Amend `docs/2026-07-25-testrom-conformance.md` in the same commit.
* The visual-baseline rows are the main thing to watch in A3b: any ROM that DMA-fills with an odd
  autoincrement gets a genuinely different VRAM tail. Common game usage (even start address, even
  length, autoinc 1, fill byte 0) maps `{a … a+N-1}` onto itself under `^ 1` for the interior, so the
  only differences are the two boundary bytes and the trigger word's LSB — usually invisible. But
  it *is* a real risk; run the whole scorecard.
* **Manual re-check (not automated):** the DR-1/DR-2/DR-3 differential ROMs (Gunstar Heroes,
  Thunder Force IV, Batman) are documented in `docs/2026-07-22-differential-rom-findings.md` but have
  **no automated conformance row**. TF4's fix was the CD5 clear (untouched here) and Batman's was
  CD5 routing (untouched), so neither should regress — but re-run them by hand before pushing A3b,
  because both are heavy DMA users.
* `crates/oracle-core/examples/frame_dump.rs:120-127` fills plane B (`$E000`, `$0800` bytes,
  autoinc 1, fill `$0000`). After the fix, `$E800` stays untouched and `$E801` is zeroed — both
  outside plane B's 2048 bytes, so the rendered PPM should be unchanged. Worth an eyeball since the
  example is the push-6 end-to-end proof.

---

## 5. Test plan (TDD — write these first, watch them fail)

### 5.1 The two headline tests: replay the ROM's sequences

The strongest possible assertions available are the ROM's own tables. Both can be driven through
`MegaDriveBus` in `bus.rs`'s test module using the existing `MdMem` harness, in a couple of dozen
port writes each. **Write these first; they are the slice's acceptance criteria.**

* `vdpfifo_t3_dma_payload_walks_the_fifo_ring` — stage `AAAA BBBB CCCC DDDD EEEE FFFF` in 68k
  memory; zero CRAM (64 words @ CD 3); six data writes `$1000`…`$6000` @ CD 1 addr `$8000`; program
  regs 19–23 for a 6-word Mem DMA; fire CD `100001` @ `$8000`; then four groups of
  {2 × VSRAM read @0, 2 × CRAM read @0} separated by a CRAM `$FFFF` write @ `$0020`. Assert the 16
  collected words equal
  `[c800,c800,c000,c000,d800,d800,d111,d111,e800,e800,e000,e000,f800,f800,f111,f111]`
  — cite ROM `$5E0C` in the comment.
* `vdpfifo_t4_fill_trigger_and_byte_placement` — zero VRAM `$8000`…`$800F`; autoinc 1; regs 19/20 =
  10; reg 23 = `$80`; fire CD `100001` @ `$8000`; data write `$1234`; then the four VSRAM-snoop
  groups, then 8 VRAM reads @ `$8000` with autoinc 2. Assert the 16 words equal
  `[0,0,0,0,0,0,0x1000,0x1000,0x1234,0x1212,0x1212,0x1212,0x1212,0x0012,0,0]` — cite ROM `$DC54`.

### 5.2 Focused unit tests (each pins one clause)

In `vdp.rs`'s test module (direct `Vdp` driving, no bus):

1. `mem_dma_words_occupy_the_physical_fifo_ring` — after four marker writes and a 6-word DMA fed via
   `dma_write_word`, `fifo_snoop_word() == 0xCCCC` (the *third*-from-last payload word, i.e. the
   cursor parked on the oldest of the surviving four).
2. `mem_dma_ring_store_does_not_add_pending_entries` — `fifo_len()` is the same before and after the
   DMA (guards §3.3's deliberate choice, and guards against a spurious stall regression).
3. `fill_trigger_is_applied_as_a_normal_word_write` — arm a VRAM fill @ `$8000`, autoinc 1, length
   0; data-write `$1234`; assert `vram[$8000] == 0x12 && vram[$8001] == 0x34` and that the address
   register has advanced by 1.
4. `vram_fill_writes_the_msb_to_address_xor_one` — the §1.7 image, asserted byte-for-byte:
   `[12,34,12,12,12,12,12,12,12,12,00,12]` at `$8000`, with `$800C..$8010` still zero.
5. `fill_adds_only_its_trigger_word_to_the_ring` — eight `$0000` writes, the `$1234` trigger + fill,
   then three ring-advancing writes; assert the snoop sequence `0, 0, 0, 0x1234`.
6. `cram_fill_still_uses_the_four_writes_ago_entry` — the existing `vdp.rs:2060` test must stay green
   verbatim (guards that change 3 did not leak into the CRAM/VSRAM fill source).

### 5.3 Gates

`cargo test -p oracle-core` (+ `--release` for the conformance/golden suites); `cargo fmt --check`;
`cargo clippy --all-targets -- -D warnings`. For A3a: all currency suites green **with existing
constants** — that is the currency-neutrality proof, and it must be shown in the commit. For A3b:
every currency suite green with existing constants **except** `export_state_v1`, whose single
regenerated constant ships in the same commit alongside the ledger amendment.

---

## 6. Open questions / where I am not confident

* **Q1 — should DMA payload words count toward FIFO fullness?** On real hardware they occupy real
  slots and a DMA can end with the FIFO still draining. §3.3 deliberately keeps `fifo_len` at zero
  because our DMA is synchronous and the alternative injects phantom stalls into every DMA-using
  ROM. Neither test 3 nor test 4 can tell the difference. This *may* matter to slice A1/T16 ("FIFO
  Wait States") and to any future non-synchronous DMA; if A1's implementer finds T16 needs post-DMA
  occupancy, revisit here rather than papering over it in the status word.
* **Q2 — does VRAM *copy* also write to `address ^ 1`?** Eke's quote says yes ("used by VRAM fill
  **and copy** DMA"). Neither test 3 nor test 4 exercises copy, so I am **not** proposing to change
  `run_copy` (`vdp.rs:854-877`) in this slice — an unevidenced change there could move visual
  baselines with no test to justify it. Recommend a named follow-up in the ledger, pinned from a ROM
  or an instrument, not from this citation alone. (Also unresolved there: whether the copy's *source*
  byte read is likewise `^ 1`.)
* **Q3 — the priming write for CRAM/VSRAM fill targets.** Nemesis's "completed as normal" is
  generic, so §3.4 applies it to all targets. The ROM only covers VRAM. Low risk (a CRAM/VSRAM fill
  is already a documented hardware-bug path nothing sane uses) but genuinely unverified.
* **Q4 — the priming write's MSB is unobservable in test 4.** With autoinc 1 the fill's first step
  rewrites `$8000` with the same `$12`, so the table cannot distinguish "full word write" from "LSB
  only". I chose the full word because it is what the citation says and what a normal FIFO write
  does; a probe with autoinc ≥ 2 would settle it if anyone wants certainty.
* **Q5 — what the ring entry records for a DMA word.** Nemesis says the FIFO entry uses "the current
  command code and *incremented* command address registers", which may mean the stored address is
  post-increment, unlike our data-port `fifo_enqueue` (pre-increment, an earlier pin). Only `data` is
  observable in tests 3/4, so this is cosmetic today — but it will matter if `addr` is ever read out
  of a drained entry.
* **Q6 — `fifo_slot_clock` during a DMA.** Not advanced by §3.3. Because `fifo_len` stays 0,
  `fifo_drain` coasts the clock forward to `now` on the next access, so nothing is banked
  incorrectly; but the DMA's elapsed slot budget is accounted only through `dma_cost` /
  `dma_busy_until`, not through the drain clock. Fine at Phase-2 granularity, worth a look if
  Phase 3 makes the two clocks interact.
* **Q7 — the `GOLDEN_HASH` ruling is not mine to make.** §4.1 recommends option 1, but the arc's
  ground rules say byte-identical, and the owner set that rule. A3b should not land until this is
  ruled on.
* **Verification I did not do.** I did not run the emulator against the ROM (the brief scoped me to
  read-only recon, and adding a probe example would have meant touching the tree). The
  current-behaviour predictions in §1.5 and §1.7 are derived from the source and happen to match the
  A0 probe's observed outputs exactly, which is a strong but indirect check. The implementer will get
  the direct check from the §5.1 replay tests on their first red run.
