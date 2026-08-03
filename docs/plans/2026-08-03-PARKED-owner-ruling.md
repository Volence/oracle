# ~~PARKED — needs the owner's ruling~~ — **CLOSED 2026-08-03, both decisions moot**

> **CLOSED. Neither decision ever needed a ruling, because neither fix moved any
> frozen currency.** Decision 1 (A3b) landed currency-neutral. Decision 2 (T12)
> was re-measured before implementation
> (`docs/2026-08-03-decision2-premise-recheck.md`) and then landed as slice T12,
> also **currency-neutral**: `export_state_v1::GOLDEN_HASH`, the six
> `golden_frames` scene hashes, `oracle_differential`, `determinism_gate` and
> `singlestep_m68000` all came back **byte-identical**. `vdp_port_access` went
> `14/2/16 → 15/1/16`.
>
> **Both premises failed the same way:** each cited `testrom::build_pad_poll`
> (used only by `io_controllers`, `watchpoints` and the `pad_probe` example — no
> frozen currency) while believing it was the `export_state_v1` golden fixture,
> which is `testrom::build` and drives no VDP port at all. **The meta-point below
> is therefore withdrawn: the golden fixture does not encode our bugs.** The
> lesson that survives is the opposite one — *"a test goes red"* and *"the
> currency moves"* are different events, and only measurement distinguishes them.
> Everything below is kept as-written for the record; read it as history.

**TWO** decisions came up during the FIFO/scanline arcs that are above an
agent's pay grade, because they move frozen currency. Nothing was changed;
both fixes are written, evidenced, and staged behind this ruling.

## The meta-point (read this first)

Both cases are the same shape, and that is itself the finding:

> **`crates/oracle-core/src/testrom.rs`'s golden fixture ROM does things real
> hardware would not honour, so the frozen hashes encode our bugs. Accuracy
> work now collides with the golden in at least two independent places.**

Neither collision is a regression — in both, the *emulator gets more correct*
and the hash moves because it was pinned to incorrect behavior. The question is
not "is the fix right" (it is, with citations); it is **"do we regenerate the
goldens once, deliberately, with the derivations recorded?"**

My recommendation: **yes, once, covering both** — regenerate in a single
dedicated commit that changes no behavior of its own, with this document's
derivations in the message, so the new values have a paper trail. The
alternative is an emulator that stays knowingly wrong in order to protect a
number that was never the goal.

Note both are *value* changes, not layout changes — no `export_state` version
bump is implied.

---

## Decision 1: may slice A3b regenerate `export_state_v1::GOLDEN_HASH`?

> **RULED + RESOLVED 2026-08-03. Owner GRANTED permission to move the hash;
> slice A3b then landed and the hash did NOT need to move.** The premise below
> is wrong in one load-bearing detail: `testrom.rs:255-263` is
> **`testrom::build_pad_poll`**, not the golden fixture. The `export_state_v1`
> golden fixture is **`testrom::build`** — the RAM-stirring ROM at `$200` that
> never writes a VDP port (verified directly: after its 60 frames every VDP
> register is `$00` and VRAM is untouched power-on noise). `GOLDEN_HASH` stays
> `0xBF5D_1E1A_A727_143B`. A3b landed with **zero currency movement**; the
> scorecard moved to `page1 8/1/9; cumulative 13/3/16` as forecast.
> The `$3B → $00` byte is real, but it is in the pad-poll fixture's VRAM at
> `$FFFF`, outside every plane/SAT window — its render and watchpoint counts are
> unchanged. Decision 2 (T12 Mode-4 masking) is unaffected and still open.

**Short version:** fixing VDPFIFOTesting test 4 also fixes a real emulation bug
that the frozen golden hash currently encodes. Fixing the bug therefore moves
`GOLDEN_HASH`. That is the one currency the project has held byte-frozen, so
the arc stopped here rather than move it.

### What the bug is

Test 4 "DMA Fill FIFO Usage" pins two behaviors (evidence and full derivation in
`docs/2026-08-03-a3-dma-fifo-design.md`, recovered by disassembling the ROM and
decoding its embedded expected-value tables — the authoritative hardware answers):

1. A DMA fill's **triggering data-port write** must be applied as a normal
   full-word write (and autoincrement) before the fill engine runs. Today it is
   swallowed.
2. The fill engine writes its byte to **`address ^ 1`**, not `address`.

Corroborated by Nemesis (the test ROM's author), Mask of Destiny, Eke and
Kabuto's public notes — cited in the design doc. No emulator source was read
(clean-room rule respected).

### Why it moves the golden

The arc's ground rules inherited an assumption from the push-6 plan: that the
golden fixture drives no VDP DMA. **That assumption is now false.**
`crates/oracle-core/src/testrom.rs:255-263` zeroes VRAM with a `$FFFF`-byte DMA
fill. Under today's (buggy) fill, `vram[$FFFF]` is never written, so it keeps its
power-on random byte — replaying the fixture seed's SplitMix64 shows that byte is
`$3B`. After the fix the whole 64 KiB is genuinely zero.

So the frozen `GOLDEN_HASH` currently bakes in "one byte of VRAM that the ROM
asked to be cleared, wasn't." The hash is not neutral evidence here — it is a
record of the bug.

### Options

- **A (recommended): land A3b and regenerate `GOLDEN_HASH`,** with the commit
  message carrying this derivation. It is a *value* change, not a layout change,
  so no `export_state` version bump is needed. The old and new values both go in
  the ledger. This is the only option that leaves the emulator correct.
- **B: land A3a only** (test 3 — provably currency-neutral, since `fifo`/
  `fifo_write` are in neither `state_hash` nor `export_state`) and leave test 4
  failing, ledgered with its cause. The golden stays untouched.
- **C: land A3b but keep the fill bug behind a flag** so the golden is
  unaffected. Not recommended — it preserves a known-wrong behavior in the
  default path purely to protect a hash.

### What was done overnight

**Option B, pending the ruling.** A3a is the currency-neutral half; A3b is fully
designed and ready but unlanded. *(Update: the ruling arrived, A3b landed, and
it turned out to be currency-neutral too — see the banner above.)*

### Also worth knowing before ruling

- `oracle_differential` (static captured bytes) and `golden_frames` (verified to
  drive no DMA) are both **immune** — this is `export_state_v1` only.
- Three existing tests assert the current (buggy) fill behavior and would be
  rewritten as part of A3b: `bus.rs:1933`, `bus.rs:1968`, `vdp.rs:2323`.
- Expected scorecard move if A3b lands: `page1 8/1/9; cumulative 11/5/16`. *(Measured: `page1 8/1/9; cumulative 13/3/16` — the cumulative figure here was stale, it predated A2 landing.)*
- The differential ROMs (Gunstar / TF4 / Batman, fixed by the DR-1/2/3 slices)
  have **no automated conformance row**, so they should be re-checked by hand
  before pushing A3b.
- One open question the design doc could not settle from ROM evidence: whether
  DMA words should count toward FIFO *fullness* (it interacts with A1/T16), and
  whether VRAM **copy** also uses `^ 1` (Eke says yes; no ROM evidence in this
  slice). Both are flagged as named follow-ups rather than unevidenced changes.

---

## Decision 2: may T12's Mode-4 register masking land? (found during slice A2)

> **RESOLVED 2026-08-03 — no ruling was needed; slice T12 LANDED currency-neutral.**
> The "why it moves currency" section below is **FALSE**, verified by measurement
> twice (`docs/2026-08-03-decision2-premise-recheck.md`, then again in the landing
> slice). No frozen constant moved. The 52 (actually 57) red tests were real, but
> every one was a fixture declaring Mode 4 while programming Mode-5 registers — an
> impossible machine state; declaring M5 in them restored every pinned hash
> byte-identically. The `#[ignore]` is removed and the test now runs.
> One caveat did survive and is now follow-up **F-M4REGS**: the ROM pins register
> 15 only, so the `> 10` boundary is extrapolated from Kabuto's own hedged
> "10(?)". See the T12 addendum in `docs/2026-07-25-testrom-conformance.md`.


**The rule.** In Mode 4 (reg 1 bit 2 = M5 clear) only the eleven SMS registers
0-10 are writable; writes to registers above 10 are discarded. Source: Kabuto's
hardware notes — "All registers except for the 10(?) SMS registers are
disabled" (https://plutiedev.com/mirror/kabuto-hardware-notes), and the
behavior is pinned independently by VDPFIFOTesting test 12's own expected table
(ROM `$20EC`).

**The fix is one line** in `Vdp::write_register`:

```rust
if self.regs[1] & 0x04 == 0 && reg > 10 { return; }
```

It was implemented and **confirmed to make T12 pass**, then reverted per the
ground rules. The spec survives as an `#[ignore]`d test
(`vdp::tests::mode4_ignores_register_writes_above_ten`) next to a NOT MODELLED
note, so nothing is lost.

**Why it moves currency.** `testrom.rs`'s golden ROM writes `reg 1 = $50`,
which leaves M5 **clear**, and then programs registers 11/12/13/15/16 — writes
real hardware would ignore. So the fixture is running in Mode 4 while using
Mode 5 registers.

> **Caution added 2026-08-03 by the A3b implementer (UNVERIFIED for this
> decision):** the `reg 1 = $50` ROM cited here is `testrom::build_pad_poll`,
> the same fixture Decision 1 mis-identified as the golden. The `export_state_v1`
> golden fixture, `testrom::build`, writes **no** VDP registers at all — so the
> claim that Decision 2 moves `GOLDEN_HASH` should be re-measured before it is
> ruled on. The `golden_frames` scenes and the 52 red unit tests are separate
> fixtures and were not re-checked here.

Blast radius is **wider than Decision 1**:
- `export_state_v1::GOLDEN_HASH` moves
- `golden_frames` scene hashes move
- **52 unit tests** whose fixtures never set M5 go red and would need their
  setup corrected (they are also, strictly, testing an impossible machine state)

**Options.** Same three as Decision 1. The extra consideration here: the 52 red
tests are not collateral damage so much as a signal — a lot of our fixtures
quietly configure a machine that could not exist. Fixing them is real work but
buys real confidence.

If you want a middle path: land Decision 1 (narrow, 3 tests to rewrite) now and
schedule Decision 2 as its own slice with the fixture cleanup budgeted.

---

## Scorecard arithmetic, so the ruling has a number attached

| State | `vdp_port_access` cumulative |
|---|---|
| Start of tonight | 9 pass / 7 fail / 16 |
| After A1 + A2 (landed) | **11 pass / 5 fail / 16** |
| + A3a (landing tonight, currency-neutral) | 12 / 4 / 16 expected |
| + A3b (Decision 1) | ~13 / 3 / 16 |
| + T12 (Decision 2) | ~14 / 2 / 16 |

*(Actual, all landed 2026-08-03: A3a 12/4/16, A3b 13/3/16, A4 14/2/16, T12 **15/1/16**. The table above predates A4, so its T12 row is one slice behind. Only T16 remains.)*

The last two rows are exactly what these rulings buy. The remaining tail after
that is T6 (slice A4, 8-bit VRAM read target) and T16 (needs discrete per-line
access-slot scheduling — the long-standing "Phase 3 per-line DMA cost"
deferral, a genuinely larger piece of work).

> **CORRECTION (2026-08-03, slice T16/S1).** The parenthetical above is wrong and
> was corrected in `docs/2026-07-25-testrom-conformance.md` (S1 addendum). T16
> needed *two* independent things, and neither is the "Phase 3 per-line DMA
> cost" deferral: **intra-line slot positions** (groups 2/3/5/6/8, ~50 lines of
> table-driven integer code) and **post-DMA FIFO occupancy** (groups 9/10, two
> lines — design question Q1 of `docs/2026-08-03-a3-dma-fifo-design.md`). Both
> are currency-neutral. The genuinely larger piece — integrating `dma_cost`
> across the lines a transfer spans — is **not needed for T16** and stays
> deferred.
