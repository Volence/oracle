# YM2612 Timer-A / Timer-B — design recon (the first real FM-chip state)

**Status: RECON + DESIGN, 2026-07-22. Docs only — no code this slice.** This designs the **YM2612 timers**
(Timer A and Timer B) — the first piece of live YM2612 state in the core. It is the direct enabler of the
confirmed-this-session silent-song bug: `aeon/s4.soundtest.bin` queues a song, the Z80 SMPS driver loads it,
but the driver **clocks its sequencer off the Timer-A overflow flag** in the YM2612 status byte, and our status
read is hardwired to `0x00` (`crates/oracle-core/src/z80/bus.rs:113`, `crates/oracle-core/src/bus.rs:297`), so
the flag never sets and the sequencer never ticks. Implement Timer A → the flag appears → the driver plays.

It mirrors the rigor/format of the two adjacent recons — `docs/2026-07-22-z80-core-design.md` (the Z80 core)
and `docs/2026-07-22-phase-rt-design.md` (the FM/PSG register-tap → VGM logger). This doc sits *between* them:
the Z80 core executes the driver, this doc makes the chip the driver polls answer truthfully, and Phase RT logs
the note writes the newly-ticking sequencer emits.

**Permitted sources only** (audit policy 3, identical to the VDP/IO/BUSREQ/FM/Z80/RT recons): the **YM2612 /
YM3438 datasheet + register documentation**, **Plutiedev** (YM2612 registers page), **Nemesis's YM2612
documentation / SpritesMind** hardware threads, and the **in-repo code + the aeon Z80 driver ASM**
(`/home/volence/sonic_hacks/aeon/engine/sound/*.asm`) as behavioral ground truth for what the driver needs. **No
third-party emulator chip source was opened** — not BlastEm, GPGX, Ares, jgenesis, nor the C++ Oracle core. The
`Vdp` (a chip owned by `System`) is used as an in-repo **design precedent**, not as chip source.

Grounding read: `crates/oracle-core/src/bus.rs` (the `$A04000-3` FM status stub + FM write-drop),
`crates/oracle-core/src/z80/bus.rs` (the `$4000-3` status stub + the RT-1 tap), `crates/oracle-core/src/system.rs`
(the `Vdp`-owned-by-`System` precedent, the split-borrow `mega_bus`/`catch_up_z80` adapters, `MCLK_PER_*`,
`state_hash`, `export_state`), `docs/export-state-v1.md` (region 6, the FM `0x200` reserve + no-bump-on-fill
rule), `CHARTER.md` / `docs/foundations.md`, and the aeon driver ASM (`z80_sound_driver.asm`,
`sound_constants.asm`).

Items are numbered **FM1–FM10**, grouped by the seven questions. Each pins the call, the evidence class, the
confidence, and the open remainder, and marks **pinned-from-source** vs **design judgment surfaced for the
overseer**.

---

## Executive summary

The YM2612 timers are a **tiny, purely-computable piece of state**. Timer A is a 10-bit reload value `NA` from
regs `$24`/`$25`; Timer B is 8-bit `NB` from `$26`; reg `$27` is the load/enable/reset control. Each timer is a
free-running periodic counter: **Timer-A tick = 1008 mclk** (exactly `18.773 µs`), overflow period
`= (1024 − NA) × 1008 mclk`; **Timer-B tick = 16128 mclk**, period `= (256 − NB) × 16128 mclk` — both **clean
integers in the machine clock** the scheduler already uses (no fractional accumulator). The status byte at
`$4000` / `$A04000` is bit0 = Timer-A overflow, bit1 = Timer-B overflow, bit7 = BUSY; **bit7 stays 0** (the
Gunstar DR-1b not-busy poll depends on it — `bus.rs` F2/F4), only bits 0/1 go live.

The load-bearing architecture call is **lazy evaluation**: store the timer's `(period, enabled, started_at_mclk,
flag_cleared_at_mclk)` and derive the overflow flag at *status-read time* as a **pure function of (registers,
current mclk)** — an overflow boundary lies in `(flag_cleared_at, now]`. No per-step advancement, no scheduler
event, snapshot-safe by construction, deterministic. A minimal `Ym2612` struct (bincode `Encode`/`Decode`) is
owned by `System` exactly like `Vdp`, and threaded into **both** the 68k `MegaDriveBus` and the Z80 `Z80Bus`
adapters as a new `&mut fm` split-borrow (like `vdp`); the reader passes *its own* current mclk (the 68k passes
`now`, the Z80 passes its frontier), which is correct because both are absolute times on the one shared
timeline.

**Currency-neutrality verdict (the most important output): SAFE — currency-neutral by construction, NO owner
gate needed.** The FM status read returns non-zero *only if* the timer was programmed by a `$27` LOAD+ENBL
write **and** a period has elapsed. Surveying all five frozen currencies: **no committed fixture programs the
YM2612 timer, and none releases the Z80 to run the driver** (verified below). So every fixture's status read
stays byte-identical `0x00`, bit7 stays 0 (Gunstar preserved), region 6 of `export_state` stays zero (export
golden frozen), and the FM is not in `state_hash` (VDP-only). All five currencies stay byte-identical, proven
construction-first then empirically.

---

## 1. Timer register + period model (FM1–FM2)

### FM1 — The register map and the `$27` control bits

**PINNED (from the YM2612 datasheet / Plutiedev + the aeon driver's own use).** The timers occupy four
registers in bank 0 (part I), reachable via the `$4000`/`$4001` (Z80) or `$A04000`/`$A04001` (68k)
address-then-data pair:

| Reg | Field | Width | Meaning |
|---|---|---|---|
| `$24` | Timer-A value high | 8 bits | `NA` bits 9..2 |
| `$25` | Timer-A value low | 2 bits (bits 1..0) | `NA` bits 1..0 |
| `$26` | Timer-B value | 8 bits | `NB` bits 7..0 |
| `$27` | Timer control | 8 bits | load/enable/reset — bit layout below |

Reg `$27` bit layout (Plutiedev "YM2612 registers"; the aeon `sound_constants.asm:124-132` uses exactly these
positions):

| Bit | Name | Effect |
|---|---|---|
| 0 | `LOAD:A` | 0 = Timer A frozen, 1 = running (counting) |
| 1 | `LOAD:B` | 0 = Timer B frozen, 1 = running |
| 2 | `ENBL:A` | 0 = overflow does nothing, 1 = overflow **sets the status flag** |
| 3 | `ENBL:B` | as ENBL:A for Timer B |
| 4 | `RST:A` | 1 = **clear** the Timer-A overflow flag (a strobe; timer keeps counting) |
| 5 | `RST:B` | 1 = clear the Timer-B overflow flag |

The 10-bit `NA = ($24 << 2) | ($25 & 3)`; `NB = $26`. (aeon `Snd_TimerA_ProgramFixed`,
`z80_sound_driver.asm:1002-1021`: writes `$24 = NA>>2`, `$25 = NA&3`, `$27 = $05 = LOAD:A | ENBL:A`.)

**Confidence**: high (datasheet + Plutiedev + the driver assembles against these exact bit constants).
**Classification**: behavioral (register model). **Open remainder**: none.

### FM2 — Period formulas, pinned in **mclk** (the clean-integer conversion)

**PINNED (from Plutiedev + the SpritesMind/datasheet `72`-cycle formula, reconciled against the repo clock).**
Both timers count **up** and overflow when the counter passes its top (`$400` = 1024 for A, `$100` = 256 for B),
then auto-reload the value and continue — a **free-running periodic** counter. Plutiedev gives the wall-clock
periods:

- **Timer A**: `period = (1024 − NA) × 18.77 µs`
- **Timer B**: `period = (256 − NB) × 300.34 µs`

The SpritesMind/datasheet form is `time = 72 × (1024 − NA) / clock`, "timers run at half the YM2612 input
clock." On the Genesis the YM2612 input clock is the master clock ÷ 7 (the same ÷7 the 68000 uses —
`MCLK_PER_CPU_CYCLE = 7`, `system.rs:27`). One Timer-A tick is therefore `72` cycles of `input/2` = `144`
cycles of the input clock = `144 × 7 = 1008` machine-clock cycles:

> **Timer-A tick = 1008 mclk = 18.7733 µs** (verified: `1008 / 53_693_175 Hz = 18.773 µs`, matching Plutiedev's
> `18.77 µs` exactly).
> **Timer-A overflow period = (1024 − NA) × 1008 mclk.**

Timer B's step is `16×` Timer A's (`300.34 µs / 18.773 µs = 16.000`; the 8-bit vs 10-bit granularity):

> **Timer-B tick = 16 × 1008 = 16128 mclk = 300.373 µs** (matches Plutiedev's `300.34 µs`).
> **Timer-B overflow period = (256 − NB) × 16128 mclk.**

**Both periods are exact integers in mclk** — no fractional-sample accumulator, no rounding drift. This is the
whole reason the timer ties cleanly into `now` (the scheduler's mclk): the flag boundary is an exact mclk value.

**Cross-check against the driver's own tuning.** The aeon driver programs `NA = SND_TIMERA_N = 137`
(`sound_constants.asm:194,205`, hard-pinned). `(1024 − 137) × 1008 = 894 096 mclk ≈ 16.652 ms → 60.053 Hz`,
against `MCLK_PER_FRAME = 896_040` (`system.rs:23`) `= 59.923 Hz`. The driver's target is
`SND_FRAME_MILLIHZ = 60053` (`sound_constants.asm:192`) — i.e. `60.053 Hz` — so **our derived mclk period
reproduces the driver's own intended frame rate to the last digit**, an independent confirmation that
`tick = 1008 mclk` is correct.

**Confidence**: high (three-way corroboration: Plutiedev µs, the datasheet 72-cycle formula, and the driver's
pinned `N=137` frame rate all agree). **Classification**: behavioral (timing values). **Open remainder**: the
exact YM2612 warm-up / first-tick alignment sub-cycle is a timing nicety with no acceptance consumer (the driver
only needs the *period*, polling once per frame) — deferred as a timing item, cannot change any flag *value* at
the frame granularity the driver reads.

---

## 2. The status byte (FM3)

### FM3 — `$4000` / `$A04000` read = bit0 Timer-A ovf, bit1 Timer-B ovf, bit7 BUSY (BUSY stays 0)

**PINNED (from the datasheet + the existing Gunstar carve-out).** The YM2612 status register (readable at
either window — one chip, two windows, RT3) is:

| Bit | Meaning |
|---|---|
| 7 | BUSY — the chip is processing the previous write |
| 6..2 | unused (read 0) |
| 1 | Timer-B overflow flag |
| 0 | Timer-A overflow flag |

**bit7 (BUSY) MUST stay 0.** The Gunstar Heroes boot (DR-1b) does a `btst #7,(a0) / bne` busy-poll and hangs if
BUSY never clears; `bus.rs:293-297` is explicit that the read returns "the status byte with bit7 (BUSY) clear =
not busy, so the … busy-poll exits (recon F2/F4, DR-1b Gunstar)." This design **preserves that**: the status
model computes bits 0/1 from the timers and leaves bits 7 and 6..2 zero. The only change from today's constant
`0x00` is that **bits 0/1 become live** (they were `0` before because "no FM timers modeled").

**Both windows read the same chip status.** The Z80 SMPS driver reads it at `$4000` (`SND_Z80_YM_A0`,
`z80_sound_driver.asm:286,363`); a 68k boot poke reads it at `$A04000`. Both route to the **same** `Ym2612`
status query (FM7) — the "one chip, two windows" discipline the RT recon (RT3) and both bus layers already
document.

**Confidence**: high (datasheet status layout + the in-repo Gunstar pin). **Classification**: behavioral +
currency-safety (the BUSY invariant). **Open remainder**: none.

---

## 3. Overflow-flag latch + reset semantics (FM4)

### FM4 — Free-running counter, flag latches on overflow (if ENBL), clears on `$27` RST strobe — pinned from what the driver actually writes

**PINNED (from the aeon driver's observed program/rearm sequence + Plutiedev).** The flag lifecycle is exactly
what the driver's init + per-tick loop dictates:

1. **Program once, at init** (`Snd_TimerA_ProgramFixed`, `z80_sound_driver.asm:1002-1021`): writes `$24`/`$25`
   (the reload value `NA`), then `$27 = $05 = LOAD:A | ENBL:A`. `LOAD:A=1` starts the counter running; `ENBL:A=1`
   lets an overflow raise the status flag. The song loader **never re-programs** Timer A — it is the fixed
   whole-driver frame clock (`z80_sound_driver.asm:242-249`; `sound_constants.asm:133-135` "Timer A is NEVER
   disabled at runtime").
2. **Poll each pass**: `ld a,(SND_Z80_YM_A0) / and SND_TIMERA_OVF_MASK` (`= 1`, bit0) / tick only on set
   (`z80_sound_driver.asm:284-289` idle, `360-365` streaming).
3. **Rearm on every tick** (`Snd_TimerA_Rearm`, `z80_sound_driver.asm:1031-1038`): writes
   `$27 = $15 = LOAD:A | ENBL:A | RST:A`. **`LOAD:A` stays 1** (no 0→1 edge — the counter is never stopped),
   `RST:A=1` is a one-shot strobe that **clears the overflow flag** while the counter keeps its phase.

So the pinned semantics: **the timer FREE-RUNS after the single init `$05` write** — it auto-reloads `NA` on each
overflow and keeps counting; it does **not** reload on the rearm write. The rearm's only effect is `RST:A`
clearing the flag. The flag therefore behaves as *"has an overflow boundary passed since the last RST clear?"*.
It **latches** on the first overflow after a clear (given `ENBL:A`) and **stays set until the next RST** —
regardless of how many further periods elapse (the driver clears once per frame, well within one period). This
"free-run + clear-flag-only" reading is corroborated by Plutiedev: `LOAD` "controls whether the timer runs",
`RST` "clears the flag."

**Nuance noted (Plutiedev vs. datasheet phrasing).** Plutiedev frames `RST` as "modifies the flag *when reading
from `$A04000`*." The behavioral effect our model needs is identical either way — a `$27` write with `RST:A=1`
results in the flag reading clear afterward — and the driver writes `RST:A` as a discrete strobe (not as a
side-effect of the status read). We model **`$27` write with RST:A ⇒ clear the flag** (see FM6); the read itself
has no side effect (keeping the read a pure function, FM6). If a future ROM is found that relies on the
read-clears-on-read variant, that is a pinnable refinement with no current consumer.

**Confidence**: high (the driver's exact `$05`/`$15` writes are the ground truth; Plutiedev corroborates).
**Classification**: behavioral (flag lifecycle). **Open remainder**: the read-side-effect variant of RST
(deferred; no consumer — the driver uses the `$27`-strobe form).

---

## 4. Time-advance model — the key architecture call (FM5–FM6)

### FM5 — Recommendation: **lazy evaluation** (compute the flag at read time), not eager per-catch-up advance

**PINNED (design call — argued against the eager alternative).** Two models:

- **(A) Eager.** Give the timer a scheduler event (or advance it in the `run_until` / `catch_up_z80` loop): on
  each overflow boundary, set a stored `flag` bool. Requires the timer to be *stepped* every catch-up, adds a
  scheduler entry, and stores a mutable `flag` that must be kept coherent across every partial advance and every
  snapshot boundary.
- **(B) Lazy.** Store only the timer's *configuration + anchors* and **derive** the overflow flag at
  status-read time as a pure function of `(registers, current mclk)`. Nothing is stepped; there is no per-tick
  work; the flag is never stored as live mutable state — it is *computed*.

**Recommend (B), lazy.** It fits the CHARTER and the scheduler strictly better:

1. **Serializable, no host-stack state (CHARTER non-negotiable).** The lazy timer is *pure data*: a handful of
   `u64`s. There is no in-flight "timer position" to hold anywhere; the flag is a function, not a field. A
   snapshot captures the anchors; a restore + a read recomputes the identical flag. Eager's stored `flag` bool
   is also serializable, but it must be *maintained* correctly across every advance — more invariants to keep,
   more ways to desync a snapshot.
2. **Deterministic by construction.** The flag at time `now` is `f(NA, NB, enables, anchors, now)` — the same
   inputs always yield the same bit, independent of *how* `now` was reached (one big advance or many small
   ones). This is the exact property the determinism gate wants, and it is free with lazy; eager must prove the
   stepped `flag` is advance-granularity-independent.
3. **Zero run-loop cost / no new scheduler entry.** The driver polls the flag ~60×/frame; a naive eager stepping
   would either add a periodic event or advance the timer each catch-up. Lazy touches the timer **only on the
   register writes and the status reads that actually happen** — no background work, no `run_until` change. This
   matches the `Vdp`'s existing "derive on demand" flavor and the lazy `z80_busreq`/`z80_running` scalars.
4. **Snapshot-safe mid-run.** Because a read is a pure function of the stored anchors and the passed `now`,
   there is no window in which the timer is "half-advanced." (Eager is fine too if stepped only at instruction
   boundaries, but lazy makes the property structural.)

**Confidence**: high. **Classification**: architecture (the central call). **Open remainder**: none — lazy is
strictly dominant here.

### FM6 — The lazy state + the exact flag function (edge cases pinned)

**PINNED (design, argued from FM4's free-running semantics).** Per timer, store four fields (Timer A shown;
Timer B symmetric):

```
enabled_a:          bool   // ENBL:A (bit2 of the last $27) — gates whether overflow sets the flag
running_a:          bool   // LOAD:A (bit0) — is the counter counting
period_a_mclk:      u64    // (1024 − NA) × 1008, recomputed on any $24/$25/$27-LOAD change
started_a_mclk:     u64    // mclk at which LOAD:A last went 0→1 (the counter's phase anchor)
cleared_a_mclk:     u64    // mclk of the last RST:A clear (or of start); the "flag cleared up to here" mark
```

The overflow flag, computed at read time from the reader's current mclk `now`:

```
fn timer_a_overflow(&self, now: u64) -> bool {
    self.running_a && self.enabled_a && self.period_a_mclk != 0
      && floor((now            - self.started_a_mclk) / self.period_a_mclk)
       > floor((self.cleared_a_mclk - self.started_a_mclk) / self.period_a_mclk)
}
```

i.e. **an overflow boundary `started_a + k·period` (k ≥ 1) lies in the half-open interval
`(cleared_a_mclk, now]`.** Pure function of the stored anchors + `now`.

Register-write side effects (the only mutations, all at write time):

- **`$24`/`$25` write** → recompute `NA`, set `period_a_mclk = (1024 − NA) × 1008`.
- **`$27` write**:
  - if `LOAD:A` rises 0→1 (or on the fixed program) → `started_a_mclk = now`, `cleared_a_mclk = now`,
    `running_a = true`.
  - `enabled_a = (byte >> 2) & 1`.
  - if `RST:A` (bit4) set → `cleared_a_mclk = now` (clears the flag; the next overflow is the next boundary
    after `now`).
  - if `LOAD:A` = 0 → `running_a = false` (frozen; flag reads 0).

**Status read has no side effect** — it only *computes* (FM4 nuance). `now` is the reader's current mclk (FM7).

**Edge cases, pinned:**
- **Flag stays set until reset.** As `now` advances past a boundary, `floor((now − started)/P)` only grows, so
  once `> cleared`'s bucket the flag stays true across every subsequent read — until `cleared_a_mclk` advances
  on the next `$27` RST. Exactly the driver's poll-then-rearm loop.
- **Multiple periods elapse between reads.** The `floor` arithmetic is bucket-based, not edge-counting, so any
  number of missed boundaries still reads as "flag set" (the hardware flag is a single sticky bit, not a
  counter). Correct.
- **RST clears without stopping.** `cleared_a_mclk = now` moves the clear-mark forward but leaves
  `started_a_mclk` (the phase anchor) untouched, so the counter's periodic boundaries stay aligned — the
  free-running behavior FM4 pins.
- **`period == 0` guard.** `NA = 1024` → `$24 = $FF, $25 = 3` gives `(1024−1024)=0`; guard against a
  divide-by-zero (the driver never programs this; `N=137`).

**Confidence**: high. **Classification**: architecture (data model + the flag function). **Open remainder**:
sub-period first-tick phase alignment (FM2 remainder) — deferred, unobserved at frame granularity.

---

## 5. State location + struct + threading (FM7)

### FM7 — A minimal `Ym2612` owned by `System` (the `Vdp` precedent), split-borrowed into both bus adapters

**PINNED (from the `Vdp` precedent in `system.rs`).** Add a chip struct owned by `System`, bincode
`Encode`/`Decode` + `Clone`/`PartialEq` (like `Vdp`), holding only the timer state (this slice — the register
file / synthesis are later, Phase RT/SY):

```
#[derive(Clone, PartialEq, bincode::Encode, bincode::Decode)]
struct Ym2612 {                 // (name it `Ym2612`; `FmTimers` if we want to scope the name to this slice)
    // Timer A
    a_running: bool, a_enabled: bool, a_period_mclk: u64, a_started_mclk: u64, a_cleared_mclk: u64,
    // Timer B (symmetric; shipped in FM-timer-2 if the DAC/driver ever needs it)
    b_running: bool, b_enabled: bool, b_period_mclk: u64, b_started_mclk: u64, b_cleared_mclk: u64,
    // the transient latched register number per bank ($4000/$4002 address writes) — needed to route a
    // $24/$25/$26/$27 *data* write to the right register. Mirrors the RT `FmDecode.addr_latch[2]`.
    addr_latch: [u8; 2],
}
```

`System` gains one field `fm: Ym2612` (power-on: all-zero / not-running — matches the current `0x00` status),
bincode-serialized like `vdp` (`system.rs:69,189`).

**Threading — a new `&mut fm` split-borrow in BOTH adapters, exactly like `vdp`:**

- **68k `MegaDriveBus`** (`system.rs:238-263 mega_bus`): add `fm` to the destructured `System { .. }` split
  and pass `&mut fm` into `MegaDriveBus::new`. The bus's `$A04000-3` arm (`bus.rs:297` read, `bus.rs:340` write)
  changes from the constant `0x00` / drop to: **read** → `fm.status(now)` (it already has `now` —
  `now_mclk`/`self.scheduler.now()`); **write** → decode the port (address vs. data, bank) and call
  `fm.write_reg(reg, value, now)`.
- **Z80 `Z80Bus`** (`system.rs:592 Z80Bus::new(z80_ram, rom, ram, z80_bank, sink)`): add `&mut fm` **and** the
  Z80's current mclk. The `$4000-3` arm (`z80/bus.rs:113` read, `:141` write) changes from constant `0x00` /
  tap-and-drop to: **read** → `fm.status(z80_now)`; **write** → still emit the RT-1 `BusEvent` (Phase RT is
  unchanged) **and** call `fm.write_reg(reg, value, z80_now)` so the timer sees the driver's `$24/$25/$27`
  writes. The RT tap and the timer update coexist (the tap is for the VGM logger; the timer update is for the
  status flag).

**Which `now` each reader passes (pinned, and it matters).** The 68k reads at `now = self.scheduler.now()`; the
Z80 reads at its own instruction-boundary time. In `catch_up_z80` (`system.rs:591-595`) the Z80 runs whole
instructions while `z80_frontier_mclk < now`, so the Z80's *current* time is `z80_frontier_mclk` (its next
boundary), **behind** the 68000's `now`. The FM status the Z80 sees must be computed at `z80_frontier_mclk`,
not the 68000's `now` — both are absolute mclk on the one shared timeline, so this is correct and
deterministic. **Implementation note:** thread `*z80_frontier_mclk` into `Z80Bus::new` (a new arg) and hand it
to the FM read. This is the one subtlety beyond "add `&mut fm`": the Z80 timer read is time-anchored to the Z80,
not the CPU. (The write-time `now` for the Z80's `$27` writes is likewise `z80_frontier_mclk`.)

No `Rc`/`RefCell`/`unsafe` — pure split-borrow, identical to how `vdp` is shared between the two adapters today.

**Confidence**: high (the `Vdp` split-borrow is the exact precedent). **Classification**: architecture.
**Open remainder**: whether to name it `Ym2612` (forward-looking, the register file/synthesis land in it later)
or `FmTimers` (scoped to this slice) — cosmetic; recommend `Ym2612` so Phase RT/SY grow the same struct.

---

## 6. Currency-neutrality — the load-bearing safety analysis (FM8)

### FM8 — SAFE: currency-neutral by construction. No fixture programs the timer or releases the Z80. No owner gate.

**PINNED (verified in-repo, the doc's most important output).** Making the FM status non-zero *during runs* is
the risk. Two-part guarantee:

**(a) The new state never enters the two export currencies.**
- **`export_state` region 6 (FM, `0x200`) stays all-zero.** The timer state rides the **bincode snapshot** (in
  `Ym2612`, a `System` field) for determinism/rewind, but is **NOT** emitted to `export_state`. Region 6 remains
  the zeroed reserve (`docs/export-state-v1.md`; `export_state_v1.rs:139` asserts `("FM", OFF_FM, SZ_FM)` is
  all-zero) — the export golden does not move. (Filling it later is a content-change-at-unchanged-size — no
  version bump — but that is a *later* Phase-RT/register-file slice, not this one.)
- **`state_hash` is VDP-only** (`system.rs:279-285`) — the FM is not in it. Untouched.

**(b) The status byte stays byte-identical `0x00` in every committed fixture** — because the flag is non-zero
*only if* the timer was programmed (a `$27` LOAD+ENBL write) **and** a period elapsed, and **no committed fixture
does either**. Survey of all five frozen currencies:

| # | Frozen currency | Programs FM timer? | Releases Z80 (runs driver)? | Reads `$A04000`? | Evidence |
|---|---|---|---|---|---|
| 1 | export golden (`export_state_v1.rs` + `testrom`) | **No** | **No** | **No** | The testrom is a RAM-stir + STOP loop; `grep` finds **zero** `$A04000`/`$A11200`/`$24`/`$27` accesses (`testrom.rs`). Asserts region 6 all-zero. |
| 2 | determinism (`determinism_gate.rs`, testrom) | **No** | **No** | **No** | Same testrom (`determinism_gate.rs:27`); RAM-stirring loop only. |
| 3 | golden_frames | **No** | **No** | **No** | Pure `Vdp` fixtures — no `System`, no CPU, no bus (`golden_frames.rs:15-20`). Cannot touch the FM. |
| 4 | oracle_differential | **No** | **No** | **No** | Static captured VDP bytes fed through FNV-1a — no `System`, no run (`oracle_differential.rs`). |
| 5 | SingleStepTests (m68000 + z80) | **No** | **No** | **No** | `FlatBus`, no `System`, no `Ym2612` in the harness at all. |

So `fm` is **never programmed** in any gate → `a_running`/`a_enabled` stay false → the status read returns
`0x00` (bits 0/1 zero, bit7 zero) — **byte-identical to today's constant stub**. BUSY (bit7) is *always* 0 by
construction (FM3), so the Gunstar DR-1b poll is preserved even in a hypothetical FM-reading fixture. The five
currencies stay byte-identical; **prove construction-first, then empirically by re-running all five** (the
FM/BUSREQ-slice discipline).

**Verdict: currency-neutral, no owner gate needed.** No fixture would move. (Contrast the hypothetical that
*would* need a gate: a committed fixture that reads `$A04000` and branches on bit0/1 *and* had the timer
programmed — that fixture would move and require the owner's call. The survey finds no such fixture.)

**Confidence**: high (every fixture inspected; the non-zero condition is precisely characterized). **Classification**:
currency safety. **Open remainder**: none.

---

## 7. What the fix unblocks (FM9)

### FM9 — Timer A live → the sequencer ticks → real note writes → Phase RT's Oracle A/B becomes possible

**PINNED (ties to the Phase-RT recon, RT7).** With Timer A answering truthfully:

1. The Z80 driver's `and SND_TIMERA_OVF_MASK` poll (idle `z80_sound_driver.asm:287`, streaming `:364`) sees the
   overflow bit set once per ~60 Hz period → takes the `SndDrv_IdleTick` / `SndDrv_TimerATick` path →
   `Sequencer_Frame` runs. **The sequencer ticks** — the exact step that is dead today (flag never sets → never
   ticks → silence).
2. `Sequencer_Frame` emits the song's real **FM/PSG register writes** (note-ons, frequencies, TL, PSG tones) to
   `$4000-$4003`/`$7F11`.
3. Those writes are **already tapped** by the landed RT-1 `Z80Bus` tap (`z80/bus.rs:141`) into the `BusEvent`
   stream. Phase RT's `VgmLogger` (RT-2) decodes them into a real song's register-write stream — **not just the
   init writes**, the whole sequenced song.
4. That makes **RT-3's Oracle A/B** feasible: `s4.soundtest.bin` (or another SMPS build) is exactly the
   "sound-driving fixture" RT7 said Phase RT needs (RT7's load-bearing finding was that *no existing fixture
   drives the driver*). Timer A is the missing piece that lets a released-Z80 fixture actually produce a song —
   turning RT-3 from "blocked on a sound source" into "capture our `VgmLogger` vs. Oracle's `vgm_start` and
   diff the triple sequences."

In short: **Timer A is the clock that makes the whole sound stack move.** The Z80 core (executes the driver),
this timer (answers the driver's poll), and Phase RT (logs what the driver emits) are the three layers; this
doc is the middle one that was still a `0x00` stub.

**Confidence**: high (the driver's poll→tick→emit chain is read directly from the ASM). **Classification**:
sequencing / enablement. **Open remainder**: none — the downstream (VGM logger, Oracle A/B) is Phase RT's scope.

---

## 8. Scope boundary (FM10)

### FM10 — In: the timers + the status flag. Out: FM synthesis, the register file, the DAC — all named

**PINNED (restating the CHARTER boundary).** This slice implements **only** the two timers and the status bits
they drive. Explicitly **out** (each with its home):
- **FM synthesis** (operators, envelopes, phase accumulators, LFO, DAC mixing) → **Phase SY**, off by default
  (CHARTER: "Audio: synthesis off by default"). Nothing here produces a sample.
- **The FM register file** (last-value-per-register, the `0x200` `export_state` region 6) → the Phase-RT /
  register-file go-live slice (RT4, S10/S13). This timer slice does **not** fill region 6.
- **The DAC** (`$2A`/`$2B` — the driver streams samples there) → tapped by RT-1 for VGM, synthesized in Phase
  SY; not a timer concern.
- **BUSY-cycle timing** (bit7 going busy for N cycles after a write) → not modeled; bit7 stays 0 (FM3, the
  Gunstar invariant). No consumer needs a non-zero BUSY.

**Confidence**: high. **Classification**: scope. **Open remainder**: none — this is the boundary.

---

## Slice ladder + owner-gated flags

Each slice is currency-neutral (FM8) and independently verifiable.

- **FM-timer-1 — Timer A + status bit0 + the driver plays (THIS slice).** Add `Ym2612` (Timer-A fields +
  `addr_latch`) to `System`; thread `&mut fm` + the reader's mclk into both `MegaDriveBus` and `Z80Bus`; the
  `$4000`/`$A04000` read returns `fm.status(now)` (bit0 live, bit7=0); the `$24/$25/$27` writes update the lazy
  Timer-A model (FM6). **Verified**: (i) a unit test on `Ym2612` alone — program `NA=137`, advance mclk, assert
  bit0 flips at `(1024−137)×1008` and clears on a `$27` RST; (ii) end-to-end — boot `s4.soundtest.bin`, release
  the Z80, run N frames, assert the driver leaves silence and the sequencer ticks (the very bug this fixes).
  Currency-neutral by construction (FM8).
- **FM-timer-2 — Timer B + status bit1 (only if a driver needs it).** Symmetric fields already stubbed in the
  struct (FM7). The aeon driver does **not** use Timer B, so this is **deferred** until a ROM exercises it —
  additive, no layout churn.

**Owner-gated decisions: NONE forced by frozen currency.** The currency analysis (FM8) finds the change
**fully currency-neutral** — no fixture programs the timer or releases the Z80, so every frozen currency stays
byte-identical and **no owner gate is required**. The only calls surfaced for the overseer are **design
judgment**, not currency gates: (i) FM5 — lazy vs. eager (recommended: **lazy**, strictly dominant); (ii) FM7 —
struct name `Ym2612` vs. `FmTimers` (recommended: `Ym2612`, grows into Phase RT/SY). Both are reversible and
neither moves a currency.

---

## Summary (the seven asks)

1. **Register/period model (FM1–FM2).** Timer A = 10-bit `NA` from `$24`(hi 8)/`$25`(lo 2); Timer B = 8-bit `NB`
   from `$26`; `$27` = LOAD:A/B(0,1) · ENBL:A/B(2,3) · RST:A/B(4,5). **Timer-A tick = 1008 mclk (18.773 µs),
   period `= (1024 − NA) × 1008 mclk`; Timer-B tick = 16128 mclk, period `= (256 − NB) × 16128 mclk`** — clean
   integers; derived from `72×(1024−NA)/clock` with the YM input = mclk/7, corroborated by Plutiedev's µs figures
   and the driver's own `N=137 → 60.05 Hz`.
2. **Status byte (FM3).** `$4000`/`$A04000` = bit0 Timer-A ovf, bit1 Timer-B ovf, bit7 BUSY. **bit7 stays 0**
   (Gunstar DR-1b), only bits 0/1 go live; both windows read the same chip.
3. **Latch/reset (FM4).** Free-running counter (auto-reloads on overflow); flag latches on overflow if `ENBL`,
   stays set until the `$27` `RST` strobe clears it. Pinned from the driver's `$27=$05` program-once +
   `$27=$15` rearm-per-tick (`LOAD` never drops; `RST` only clears).
4. **Time-advance (FM5–FM6).** **Lazy** — store `(running, enabled, period, started_at, cleared_at)`, derive the
   flag at read time as "an overflow boundary lies in `(cleared_at, now]`" — a pure function of (registers, mclk),
   snapshot-safe, deterministic, zero run-loop cost. Strictly dominates eager per-catch-up advance for the CHARTER
   + scheduler.
5. **State + struct (FM7).** A minimal bincode `Ym2612` owned by `System` (the `Vdp` precedent); a new `&mut fm`
   split-borrow into **both** `MegaDriveBus` and `Z80Bus`; each reader passes its own current mclk (68k → `now`,
   Z80 → `z80_frontier_mclk`), both absolute on the one timeline.
6. **Currency-neutrality (FM8) — SAFE, no gate.** The status is non-zero only if the timer is programmed *and*
   time elapses; **no committed fixture programs the YM2612 timer or releases the Z80** (all five surveyed), so
   every status read stays byte-identical `0x00`, bit7 stays 0, region 6 stays zero, FM is not in `state_hash`.
   Currency-neutral by construction — **no owner-gated decision**.
7. **Unblocks (FM9).** Timer A live → the driver's overflow poll fires → `Sequencer_Frame` ticks → real note
   writes → RT-1 taps them → Phase RT's `VgmLogger` captures a real song → RT-3's Oracle A/B becomes possible.
   Timer A is the clock the whole sound stack was missing.

## Sources

- **Plutiedev — [YM2612 register reference](https://plutiedev.com/ym2612-registers)**: `$24`/`$25` (Timer-A
  10-bit), `$26` (Timer-B 8-bit), `$27` bits LOAD:A/B (frozen/running), ENBL:A/B (overflow does nothing / sets
  flag), RST:A/B (clear flag); **Timer-A period `= ($400 − TMRA) × 18.77 µs`, Timer-B period
  `= ($100 − TMRB) × 300.34 µs`**.
- **YM2612 datasheet / SpritesMind [YM2612 Information Thread](https://gendev.spritesmind.net/forum/viewtopic.php?t=1883)**:
  Timer-A `time = 72 × (1024 − A) / clock`; counts up, overflows past `$400`, auto-reloads; **timers run at half
  the YM2612 input clock** (→ tick = 144 input cycles = mclk/7 domain = 1008 mclk). Status register bit7 = BUSY,
  bit1 = Timer-B ovf, bit0 = Timer-A ovf.
- **aeon Z80 driver ASM (behavioral ground truth):**
  `engine/sound/z80_sound_driver.asm` — `Snd_TimerA_ProgramFixed` (`:1002-1021`, writes `$24=N>>2`, `$25=N&3`,
  `$27=$05`), `Snd_TimerA_Rearm` (`:1031-1038`, `$27=$15 = LOAD|ENBL|RST`), the overflow polls (`:284-289`,
  `:360-365`); `engine/sound_constants.asm` — `SND_REG_TIMER_A_HI/LO/CTRL = $24/$25/$27` (`:121-123`), the
  `$27` bit constants + `$05`/`$15` (`:124-143`), `SND_TIMERA_OVF_MASK = 1` (`:136`), the
  `period = 18.773µs × (1024 − N)` comment + count-up-past-1024 (`:145-167`), `SND_TIMERA_N = 137` /
  `SND_FRAME_MILLIHZ = 60053` (`:192-207`).
- **oracle-next in-repo (precedent + the currency survey):** `crates/oracle-core/src/system.rs` (`Vdp`
  owned-by-`System` + split-borrow `mega_bus`/`catch_up_z80`, `MCLK_PER_CPU_CYCLE=7`, `MCLK_PER_FRAME=896040`,
  `state_hash` VDP-only, `export_state` region layout), `crates/oracle-core/src/bus.rs` (the `$A04000-3` status
  stub `:293-297` + write-drop `:337-340`, the BUSY-clear Gunstar pin), `crates/oracle-core/src/z80/bus.rs` (the
  `$4000-3` stub `:113` + the RT-1 tap `:141`), `crates/oracle-core/src/testrom.rs` (no FM/Z80-release access),
  `crates/oracle-core/tests/{export_state_v1,determinism_gate,golden_frames,oracle_differential,singlestep_m68000,singlestep_z80}.rs`
  (the five-currency survey), `docs/export-state-v1.md` (region 6 FM `0x200` reserve + no-bump-on-fill rule),
  `docs/2026-07-22-z80-core-design.md` (the Z80 core that executes the driver; the split-borrow + no-fixture-releases-Z80
  discipline), `docs/2026-07-22-phase-rt-design.md` (RT-1 tap, RT3 one-chip-two-windows, RT7 sound-driving-fixture
  finding), `CHARTER.md` / `docs/foundations.md` (serializable no-host-stack state, determinism, synthesis off by
  default).
