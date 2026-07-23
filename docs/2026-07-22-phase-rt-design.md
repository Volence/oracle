# Phase RT — FM/PSG register-tap → VGM logger design recon

**Status: RECON + DESIGN, 2026-07-22. Docs only — no code this slice.** This is the Phase-RT design recon the
sound-stack sequencing recon (`docs/2026-07-22-sound-stack-recon.md`, S2/S10/S13) and the Z80-core recon
(`docs/2026-07-22-z80-core-design.md`, ZC12) both named as the layer directly above the Z80 core. RT-1 (the
Z80-side FM/PSG `BusEvent` tap) has **already landed** in `crates/oracle-core/src/z80/bus.rs`. This doc designs
the rest of Phase RT: turning the Z80's (and the 68000's) FM/PSG register writes into a **logged
register-write stream** that renders to canonical VGM and can be A/B-compared against the C++ Oracle's
`vgm_start`/`vgm_stop`/`get_channel_states` surface. It does **not** design any synthesis — envelope/operator/
DAC/LFSR sample generation is Phase SY, off by default (S2/CHARTER).

**Permitted sources only** (audit policy 3, identical to the sound-stack / Z80-core / VDP / FM recons): the
public **VGM file-format specification** (vgmrips wiki), **Plutiedev** (YM2612 registers, PSG chip, Z80 usage),
**SpritesMind** hardware threads, and the **in-repo code** as design precedent. The Oracle **MCP tool schemas**
(`emulator_vgm_start`/`vgm_stop`/`vgm_status`/`get_channel_states`/`audio_spectrum`) are read as legitimate
**A/B comparison targets** — the same way `export_state` treats Oracle's state format — but the clean-room
*implementation* here derives from the VGM spec + chip write-protocol behavior, **not** from Oracle internals
or any third-party emulator (BlastEm/GPGX/Ares/jgenesis/the C++ Oracle core were not opened).

Grounding read: `crates/oracle-core/src/bus.rs` (the `BusEventSink` trait, `BusEvent`, `MegaDriveBus::emit`,
the `$A04000-3` FM carve-out, the `run_frames_with_sink`/`run_until_with_sink` sink path),
`crates/oracle-core/src/z80/bus.rs` (the landed RT-1 tap), `crates/oracle-core/src/z80/mod.rs`
(`Z80Regs`/`export_region`), `crates/oracle-core/src/system.rs` (`run_frames_with_sink`, `catch_up_z80`),
`crates/oracle-core/tests/oracle_differential.rs`, `docs/2026-07-22-differential-rom-findings.md`,
`CHARTER.md`, `docs/foundations.md`, `docs/export-state-v1.md`, and the two named recons.

Items are numbered **RT1–RT8**, grouped by the eight deliverables. Each pins the call, the evidence class, the
confidence, and the open remainder, and marks whether it is **pinned from a source** or **a design judgment
call surfaced for the overseer**.

---

## Executive summary

The whole of Phase RT is a **pure `BusEventSink` consumer** — no new mechanism in the run loop, no CPU/chip
core, no frozen-currency motion. The plumbing already exists on both sides: the Z80 tap (RT-1) emits
`BusEvent { op: Write, fc: 0, addr: <raw Z80 port>, .. }` for `$4000-$4003`/`$7F11`, and the 68000 side already
emits a `BusEvent` for **every** access via `MegaDriveBus::emit`, so the 68k FM writes at `$A04000-$A04003` and
the PSG at `$C00011` are **already in the stream** with the real function code — the logger needs **zero**
68k-side code, it just filters by `addr`. A `VgmLogger` struct implements `BusEventSink`, holds the two chips'
address-latch state, decodes each port write into a normalized `(chip, port, register, value, timestamp)`
record, and renders those records to canonical VGM (`0x52`/`0x53` YM2612, `0x50` PSG, `0x62` frame-wait) on
demand. It attaches through the existing `run_frames_with_sink` path; the null `()` path is untouched. It is
**currency-neutral by construction** — an opt-in caller-owned sink, and no committed fixture releases the Z80
or plays sound, so it captures **zero** writes in every gate. The Oracle A/B target (RT-3) is a **register-write
triple-sequence equality** against Oracle's `.vgm` output; the load-bearing feasibility finding is that **no
committed fixture drives the sound driver** (the in-repo `oracle_differential` is static captured VDP bytes;
the Gunstar/TF4/Batman differentials are docs-only Oracle-MCP runs, none of which exercise Z80 sound), so RT-3
**requires standing up a sound-driving fixture** — a real driver uploaded to a released Z80 — which is the
same out-of-band harness the Z80 core is validated with.

---

## 1. FM write-protocol decode (RT1)

### RT1 — Reconstructing `(bank, register, value)` triples from the per-port `BusEvent` stream via a per-bank address latch

**PINNED (from Plutiedev YM2612 + the VGM spec).** The YM2612 exposes two port pairs, and a register write is
a **two-step latch-then-data** sequence per bank (Plutiedev "YM2612 registers"):

| Port | Z80 addr | 68k addr | Role |
|---|---|---|---|
| Bank-0 address | `$4000` | `$A04000` | latch the register number for part I (regs `$21-$B7`) |
| Bank-0 data | `$4001` | `$A04001` | write the value to the latched bank-0 register |
| Bank-1 address | `$4002` | `$A04002` | latch the register number for part II (channels 4-6) |
| Bank-1 data | `$4003` | `$A04003` | write the value to the latched bank-1 register |

The `BusEvent` stream is **per-port**, so the decoder is a tiny state machine holding **two** latched register
numbers, one per bank:

```
struct FmDecode { addr_latch: [u8; 2] }   // [bank0, bank1]

on Write(addr) → let (bank, is_data) = classify(addr):   // $4000/$A04000 → (0,false), $4001/$A04001 → (0,true), $4002.. → (1,..)
    if !is_data:  self.addr_latch[bank] = value as u8            // address write: remember the register number
    else:         emit_triple(bank, self.addr_latch[bank], value)  // data write: (bank, latched reg, value) is complete
```

A data write with no preceding address write in this capture reuses the last latch (the chip is stateful across
writes; the driver latched it earlier) — the decoder preserves the latch across the whole capture, exactly like
the hardware register pointer. This maps **directly** onto the VGM command set (RT4): a completed bank-0 triple
is `0x52 aa dd`, a bank-1 triple is `0x53 aa dd` (VGM spec — port-0/port-1 YM2612 writes).

**Byte-size note (open remainder).** In practice every FM access is byte-sized (the Z80 tap always emits
`Size::Byte`; 68k drivers use `move.b`). A word-sized 68k write to `$A04000` is decomposed by the decoder into
its two byte halves at `$A04000`/`$A04001` (address then data) the same way the bus splits a word — a rare
edge, pinnable if a driver exercises it.

**Confidence**: high (the latch-then-data protocol is fully specified by Plutiedev; the VGM mapping by the
spec). **Classification**: behavioral (the protocol) + architecture (the decoder). **Open remainder**:
word-sized FM writes (decompose to two byte events) — not on the first-driver path.

---

## 2. PSG write decode (RT2)

### RT2 — Tracking the SN76489 latch via the self-describing bit-7 command byte

**PINNED (from Plutiedev PSG + the VGM spec).** The SN76489 is a single **write-only** port —
`$7F11` (Z80) / `$C00011` (68k) — with **self-describing** bytes; no separate address port exists (Plutiedev
"PSG chip"):

- **`bit7 = 1` → LATCH/DATA byte**: `%1 ccc t dddd` — `ccc` = channel/type selector (bits 6-5 channel, bit 4
  = 0 tone/1 volume), `dddd` = the low 4 data bits. Latches which register subsequent data bytes target **and**
  writes the low nibble.
- **`bit7 = 0` → DATA byte**: `%0 xdddddd` — the high 6 data bits of the **currently latched** register (tone
  period high bits).

So the decoder holds **one** latched-register selector and needs no address port at all:

```
struct PsgDecode { latch: u8 }   // the 4-bit channel/type selector, from the last bit7=1 byte

on Write($7F11 | $C00011, value):
    if value & 0x80 != 0:  self.latch = (value >> 4) & 0x07   // latch byte: remember channel/type
    // either way, the byte is one VGM PSG command
    emit_psg(value)
```

For the **VGM stream** the PSG is even simpler than the FM: **every** byte written to the port — latch byte or
data byte, indistinguishably — is emitted verbatim as `0x50 dd` (VGM spec — the SN76489 write command is a
single opaque byte). The decoder still tracks `latch` because the **register-file** view (the `0x10`-byte
`export_state` PSG region, S13) and any channel-level introspection need to know which register a bare data
byte updated; the raw VGM byte stream does not.

**Confidence**: high (Plutiedev PSG + VGM spec). **Classification**: behavioral + architecture. **Open
remainder**: the PSG stereo/`GG` extension byte does not exist on the mono Genesis SN76489 (Plutiedev) — no
handling needed.

---

## 3. "One chip, two windows" unification (RT3)

### RT3 — Normalizing both address windows to a single YM2612 + single SN76489, keyed off `addr`, `fc`-agnostic

**PINNED (from the two in-repo bus maps + Plutiedev).** There is exactly **one** YM2612 and **one** SN76489 on
the machine; each is reachable from two masters at two different address windows:

| Chip | Z80 window (fc = 0) | 68000 window (fc = 5/6) |
|---|---|---|
| YM2612 bank-0 addr/data | `$4000` / `$4001` | `$A04000` / `$A04001` |
| YM2612 bank-1 addr/data | `$4002` / `$4003` | `$A04002` / `$A04003` |
| SN76489 PSG | `$7F11` | `$C00011` |

The logger normalizes by **classifying on `addr` alone**, folding both windows into the same decoder state
(RT1/RT2). `fc` is carried as *attribution only* (which master issued the write — the Z80 driver vs. a rare
68k boot poke like Gunstar's `$66360` FM init), never as a routing key: a write to `$4000` and a write to
`$A04000` update the **same** `FmDecode.addr_latch[0]`. This is the exact discipline both bus layers already
document — `z80/bus.rs`: *"The RAW Z80-side address (`$4000`/`$7F11`) is emitted, NOT the 68k FM window
(`$A04000`): a consumer unifies the two at the register-file level"*, and the sound-stack recon S13:
*"The Z80-side `$4000-$4003` and the 68k-side `$A04000-$A04003` decode to the same register file (one chip, two
windows)."*

**Load-bearing free lunch (verified against `bus.rs`):** the 68k side needs **no new tap**. `MegaDriveBus`
emits a `BusEvent` on *every* access (`read16`/`write16`/`read8`/`write8` each call `self.emit(..)` after the
store), so a 68k write to `$A04000` already produces `BusEvent { op: Write, fc, addr: 0xA04000, size, value }`
even though `store_byte` drops the value into the FM carve-out. The PSG at `$C00011` is unmapped in
`mapped_byte` (open bus) and dropped in `store_byte`, but `emit` still fires
`BusEvent { op: Write, fc, addr: 0xC00011, .. }`. **Both 68k sound windows are therefore already in the
`BusEventSink` stream**; the logger is a pure `addr`-filter over them plus the RT-1 Z80 events. The one thing
to confirm at RT-2 build time is that `$C00011` byte writes emit with `addr == 0xC00011` (odd byte) and not the
even base — the current `write8` path passes `addr` through unmasked except `& ADDR_MASK`, so it does; a
word write `move.w` to `$C00010` would land the PSG byte in the low half and is the symmetric edge to RT1's
word-write note.

**Confidence**: high (both maps verified in-repo; Plutiedev for the window addresses). **Classification**:
architecture. **Open remainder**: word-sized 68k accesses to the odd PSG byte (`$C00011`) — decompose like
RT1's FM word case.

---

## 4. The VGM register-write representation (RT4)

### RT4 — Store a normalized internal record; render to canonical VGM on demand

**DESIGN JUDGMENT CALL (surfaced for the overseer), argued from the VGM spec + the A/B target.** Two candidate
internal representations:

- **(A) Canonical VGM command bytes** directly — append `0x52 aa dd` / `0x53 aa dd` / `0x50 dd` and interleave
  `0x61 nn nn` / `0x62` wait commands as they happen (VGM spec).
- **(B) An internal `(chip, port, reg, value, timestamp_mclk)` record vector**, rendered to VGM on demand.

**Recommendation: (B) — the normalized record is the source of truth; VGM is a render target.** Reasons:

1. **A/B-comparability wants the triples, not the bytes.** The RT-3 differential (RT7) is fundamentally
   *"did our driver write the same registers, in the same order, as Oracle's?"* — a comparison over the
   `(chip, port, reg, value)` sequence, **independent of how waits are encoded**. Extracting that from raw VGM
   bytes means re-parsing a variable-length command stream; from the record vector it is a trivial projection.
   The internal record decouples *register correctness* (the RT-3 gate) from *timing encoding* (RT6, where
   sub-sample precision is deliberately deferred).
2. **Byte-exact VGM interop is still free.** Oracle's differential surface **is** a `.vgm` file
   (`emulator_vgm_start` → *"captures YM2612 + SN76489 register writes to a `.vgm` file"*). Rendering (B) to
   canonical VGM — `0x52/0x53` for the two YM2612 ports, `0x50` for PSG, `0x61/0x62/0x7n` for waits, `0x66`
   end-of-data, with the SN76489 clock at header offset `0x0C` and the YM2612 clock at `0x2C`, all little-endian
   (VGM spec) — gives us a real `.vgm` we can byte-diff or feed to the whole VGM tool ecosystem. We keep both:
   the record for the register-sequence gate, the rendered VGM for file-level interop.
3. **It keeps timing decisions late.** VGM bakes a wait encoding into every stored byte; the record carries a
   raw `mclk` timestamp and lets the renderer choose the wait granularity (RT6), so the deferrable sub-sample
   question (RT6) does not contaminate the stored log.

**Relationship to the `export_state` register-file regions (regions 6/7).** These are two *different* things
fed by the *same* decoded writes: the **register file** (last-value-per-register, `0x200` FM / `0x10` PSG,
S10/S13) is a **snapshot** that belongs in the frozen `export_state` currency; the **VGM log** is a **transient
event stream** owned by the caller's sink, never in `export_state`. RT4 is about the event stream; the
register-file go-live is its sibling (both currency-neutral, S10 — zero writes at the testrom capture point, so
the golden does not even move).

**Confidence**: high on the VGM mapping (spec-pinned); the store-record-vs-store-VGM choice is a deliberate
judgment call. **Classification**: architecture (data model). **Open remainder**: whether the renderer also
emits the VGM `GD3` tag / PCM data blocks (`0x67`) — not needed for a register-only A/B; deferred.

---

## 5. Where the logger lives + how it is exposed (RT5)

### RT5 — A `VgmLogger` `BusEventSink` in `oracle-core`, attached via `run_frames_with_sink`; currency-neutral by construction

**PINNED (from `bus.rs`/`system.rs` precedent).** The logger is a struct implementing `BusEventSink`, living in
`oracle-core` (alongside `Watchpoints` — the existing precedent of a real sink consumer):

```
struct VgmLogger {
    fm: FmDecode,                 // per-bank address latch (RT1)
    psg: PsgDecode,               // channel/type latch (RT2)
    records: Vec<VgmRecord>,      // normalized (chip, port, reg, value, mclk) — RT4(B)
    frame: u64,                   // latched from on_step_boundary (RT6)
    // counters for status: fm_writes, psg_writes, frames
}
impl BusEventSink for VgmLogger {
    fn on_step_boundary(&mut self, _pc: u32, frame: u64) { self.frame = frame; }   // RT6 timestamp source
    fn on_event(&mut self, e: BusEvent) {
        if e.op != BusOp::Write { return; }
        match classify(e.addr) {                       // RT3: fc-agnostic, addr-keyed
            Fm{bank,is_data} => self.fm.step(bank, is_data, e.value as u8, self.frame, &mut self.records),
            Psg              => self.psg.step(e.value as u8, self.frame, &mut self.records),
            _ => {}
        }
    }
}
```

**Attachment uses the existing path, no new mechanism.** `System::run_frames_with_sink` /
`run_until_with_sink` already thread an arbitrary `&mut S: BusEventSink` through both `step_cpu` (the 68k
`MegaDriveBus`) **and** `catch_up_z80` (the `Z80Bus`) for a whole run — verified in `system.rs`: the same
`sink` is passed to both. So one `VgmLogger` sees **both** windows in one run with zero plumbing changes.

**Caller surface — mirror Oracle's `vgm_start`/`vgm_stop`/`vgm_status`.** Because `System` never *stores* the
sink (it is the caller's, passed per call — `run_frames_with_sink` doc: *"The sink is the caller's — `System`
never stores it, so it is in neither frozen currency and cannot move a state hash"*), the surface is:

- **`vgm_start`** = construct a `VgmLogger` (or clear an existing one's `records`) and begin passing it to
  `run_frames_with_sink` for subsequent frames.
- **`vgm_stop`** = stop passing it; call `logger.render_vgm() -> Vec<u8>` to flush canonical VGM bytes (to a
  buffer/file at the `oracle-bus` layer — the core stays zero-I/O per CHARTER).
- **`vgm_status`** = read the logger's counters (`fm_writes`, `psg_writes`, `frames`, active y/n).

The MCP-level `emulator_vgm_start/stop/status` names are the **oracle-bus** binding of this core primitive (a
later slice, when the JSON-RPC server lands); the core primitive is the sink itself.

**Why currency-neutral (explicit, S8-class argument).** (i) It is an **opt-in** sink — every frozen gate runs
`run_frames` = `run_frames_with_sink(_, &mut ())`, the untouched null path; the `()` impl of `on_event` is a
no-op. (ii) Even *with* a `VgmLogger` attached, **no committed fixture releases the Z80** (`z80_running ==
false` everywhere — S8/ZC13) and none writes the FM/PSG ports from the 68k, so the logger captures **zero**
records in any gate. (iii) The logger touches **no** `System` state — it only reads the `BusEvent` stream and
accumulates into its own `Vec` — so it cannot move `export_state`, `state_hash`, or determinism. The five
frozen currencies stay byte-identical, construction-first, prove empirically (the FM/BUSREQ-slice discipline).

**Confidence**: high. **Classification**: architecture + currency. **Open remainder**: none for the core sink;
the MCP binding is a separate `oracle-bus` slice.

---

## 6. Timing model for the VGM stream (RT6)

### RT6 — Frame-granular waits from the `on_step_boundary` frame stamp; sample-accurate waits deferred

**PINNED for the model, with a named judgment call.** VGM interleaves register writes with **wait** commands
measured in samples at **44100 Hz** (VGM spec: `0x61 nn nn` = wait `n` samples, `0x62` = wait 735 samples =
1/60 s, `0x63` = 882 = 1/50 s, `0x7n` = wait `n`+1). Our register events must be timestamped and turned into
waits. Two facts constrain the design:

- The `BusEvent` itself carries **no timestamp** (`{op, fc, addr, size, value}`), and `on_event` is not passed
  `mclk` — it is emitted deep in a chip access.
- The sink **is** handed a coarse clock: `run_until_with_sink` calls `on_step_boundary(pc, now /
  MCLK_PER_FRAME)` before **every** 68k step (verified in `system.rs`), i.e. the sink learns the **frame index**
  at instruction-boundary granularity for free.

**Recommended model (zero new mechanism): frame-bucketed waits.** The logger latches `frame` in
`on_step_boundary` and stamps each record with it; the renderer emits register writes in arrival order and one
**`0x62` (735-sample) frame-wait** at each frame boundary. This is not a compromise hack — it is the *native*
shape of a Genesis driver's VGM: SMPS-class drivers do essentially all their FM/PSG writes from the **vblank
handler**, one batch per frame, which is exactly what `0x62`-delimited frame buckets encode. NTSC arithmetic
lines up cleanly: `MCLK_PER_FRAME = 896_040`, one NTSC frame ≈ 735 samples at 44100 Hz = one `0x62`, so
frame-bucketing is **drift-free** by construction (no fractional-sample accumulator needed). This ships with
**no** change to `BusEvent` or the sink trait.

**Sample-accurate waits = the deferrable open item (judgment call surfaced).** If a future need arises to place
writes at sub-frame sample positions (a driver that writes off a Z80 timer rather than vblank, or a
byte-exact-VGM diff against Oracle's sample-accurate log), the record's timestamp must be finer than a frame.
The cheapest upgrade is to widen `on_step_boundary` to carry `now` (mclk) — a **backward-compatible,
default-carrying** trait extension (the same shape as the existing `wants_vdp_writes`/`on_vdp_write`
additions), letting the renderer convert `Δmclk → Δsamples` with a fractional accumulator
(`samples = mclk * 44100 / MASTER_CLOCK_HZ`, carry the remainder to avoid drift). **Recommendation:** ship
frame-bucketed waits at RT-2 (matches the driver idiom, no mechanism change); treat sample-accurate waits as an
RT-3+ enhancement gated on whether the Oracle A/B actually needs wait-byte equality (it does not for
register-sequence equality — RT7). This is the one timing precision item flagged deferrable.

**Confidence**: high for frame-bucketed (spec + driver idiom); the sub-frame upgrade is a named judgment call.
**Classification**: behavioral (wait encoding) + timing (deferred precision). **Open remainder**:
sample-accurate sub-frame waits (deferred; upgrade path named).

---

## 7. RT-3 A/B methodology + feasibility (RT7)

### RT7 — Register-sequence equality against Oracle's `.vgm`; and the finding that no committed fixture drives sound

**PINNED (scope + feasibility; not solved here).** The RT-3 differential compares our decoded stream against
the C++ Oracle. What each Oracle surface exposes (from the MCP schemas, read as A/B targets):

- **`vgm_start`/`vgm_stop`/`vgm_status`** → captures YM2612 + SN76489 register writes to a **`.vgm` file**.
  This is the **primary** and richest target: a full register-write stream in the canonical format we render to
  (RT4). The right differential is **register-write triple-sequence equality** — extract the `(chip, port, reg,
  value)` sequence from both our records and Oracle's `.vgm`, and compare order-for-order — *independent of the
  wait encoding* (which our frame-bucketing (RT6) will not match sample-for-sample and need not).
- **`get_channel_states`** → the enabled/disabled state of each channel (`fm1..fm6, dac, psg1..psg3,
  psg_noise`). This is a **coarse** derived view (which voices are active), **not** a register stream. It is a
  useful **cheap sanity check** (do the same voices light up?), but it is *weaker* than register equality and
  is a function of synthesis-adjacent decode, so register-sequence equality — not channel-state equality — is
  the right primary differential. Channel-state is the corroborating secondary.
- **`audio_spectrum`** → an FFT of the **synthesized** output bus. This is **Phase SY / Tier 2** (RT8), not a
  register-tap differential; out of RT scope.

**Feasibility finding — the load-bearing risk: no committed fixture drives the sound driver.** Surveying every
fixture the sound-stack recon named:

| Fixture | Drives Z80 sound? | Evidence |
|---|---|---|
| `oracle_differential.rs` (the 3 "differentials") | **No** | It is **static captured VDP bytes** (REGS/CRAM/VSRAM hashes) fed through FNV-1a — no `System`, no bus, no run, no Z80, no sound (read in full). |
| Gunstar / TF4 / Batman "differential ROMs" | **No (for sound)** | These are **docs-only** Oracle-MCP boot sweeps (`docs/2026-07-22-differential-rom-findings.md`), run via `boot_rom` for **render** comparison. All three were resolved at the **bus/VDP-DMA** level with **explicitly no Z80/FM core** (DR-1 BUSREQ+FM-status, DR-2/DR-3 CD5 DMA). None releases the Z80 to run its driver; TF4's triage confirms **0 Z80 reads**. They exercise nothing in the sound path. |
| export golden / determinism / golden_frames / SST | **No** | All hold the Z80 in reset or have no `System` at all (S7/S8, verified in `system.rs`: *"no fixture releases the Z80 from reset"*). |

**So RT-3 has no ready-made sound-driving fixture and must stand one up.** The differential needs a ROM whose
Z80 driver actually runs and writes the FM/PSG — i.e. a ROM booted far enough that the 68k has uploaded the
sound driver, released the Z80 (`$A11200` bit0 = 1), and triggered music/SFX. That is the **same out-of-band
harness the Z80 core is already validated with** (ZC13's *"release reset via `$A11200`, upload a real driver,
run N frames"* — already realized as the `z80_executes_in_the_run_loop_when_released` / `z80_takes_the_vblank_interrupt`
tests). **CORRECTION to this recon's original framing: the Z80 execute core is NOT a pending prerequisite — it is
already landed.** The documented Z80 ISA is complete (708k SST-z80 green), and Z-live wired it into the run loop:
a released Z80 executes real opcodes **today**, including FM/PSG writes (RT-1's own `z80_fm_and_psg_writes_surface_through_the_run_loop_sink`
system test proves exactly this end-to-end). So RT-3 is **unblocked on the CPU side**; its only real prerequisites
are (a) a sound-driving ROM fixture, and (b) pinning Oracle's `.vgm` framing against a real capture. Candidate
drivers already on the user's disk: the aeon `s4.bin` / the Sonic-2 / S&K disasm builds (SMPS drivers), run to a
point where music is playing, captured in both oracle-next (our `VgmLogger`) and Oracle (`vgm_start`), then
triple-sequence-compared. (Only the *undocumented*-opcode Z80 pass remains deferred — and SMPS drivers use
documented opcodes, so RT-3 does not need it.)

**Named risks/unknowns (surfaced, not resolved):**
1. **RT-3's prerequisite is a sound-driving FIXTURE, not the CPU core** (the core is done — see the correction
   above). RT-2 (the logger) lands and is unit-tested against synthetic `BusEvent` streams regardless; RT-3
   additionally needs a ROM whose driver runs to music-playing.
2. **Field/byte ordering of Oracle's `.vgm` vs. ours** must be pinned against a real capture at
   differential-build time (the sound-stack recon S11 named this: *"the exact ... VGM register-log framing ...
   is a Part-3 pinning task at differential-build time"*) — e.g. does Oracle emit the YM2612 clock/PSG clock in
   the header the same way, does it coalesce or split bank writes.
3. **Determinism of "the same point"** — the two cores must be run to a comparable driver state; a lockstep
   `emulator_z80_registers` cross-check (Tier 0) de-risks this before trusting the register-stream diff.
4. **DAC/PCM writes** — a driver streaming DAC samples writes `$2A` (bank-0 data) heavily; the register-stream
   diff will include those. Whether to compare them or filter them (they are synthesis-adjacent) is a
   differential-tuning call for RT-3.

**Confidence**: high on the methodology and the *no-fixture* finding (verified in-repo); the differential
itself is scoped, not built. **Classification**: validation + scope. **Open remainder**: everything named in
risks 1-4 — resolved when RT-3 is built, explicitly not now.

---

## 8. Synthesis boundary (RT8)

### RT8 — RT only taps and logs; envelope/operator/DAC/LFSR synthesis is Phase SY, off by default

**PINNED (restating CHARTER + S2, the boundary).** Nothing in Phase RT produces an audio sample. The FM/PSG
"chips" for RT are **write-decoders into a small register file + a VGM event stream** (S13) — there is **no**
operator phase accumulator, **no** envelope generator, **no** DAC mixing, **no** PSG tone/noise LFSR. All of
that is **Phase SY**, the deferred tail, **off by default in headless runs** (CHARTER: *"Audio: synthesis off
by default"*; `foundations.md`: *"audio hashed as the VGM register stream, not PCM"*). The `Z80Bus` and the 68k
bus only *tap* the writes; the `VgmLogger` only *decodes and records* them. The Oracle `audio_spectrum` surface
(FFT of real output) is the Phase-SY / Tier-2 differential and is explicitly out of RT scope (RT7). This
boundary is what keeps Phase RT inside the reserved `export_state` sizes with **no version bump** (S10): a
register-file fits `0x200`/`0x10`; full synthesis state (envelope/phase/LFO/LFSR/timers) would exceed them and
is the named-and-avoided v2 path.

**Confidence**: high. **Classification**: scope. **Open remainder**: none — this is the boundary.

---

## RT slice ladder

Each slice is currency-neutral and independently verifiable. **RT-1 is already landed.**

- **RT-1 — Z80 FM/PSG tap (LANDED).** `z80/bus.rs`: `$4000-$4003`/`$7F11` writes emit
  `BusEvent { op: Write, fc: 0, addr, size: Byte, value }`; RAM/bank writes emit nothing. Unit-tested
  (`fm_and_psg_writes_tap_into_the_event_sink`). Currency-neutral (Z80 held in reset).

- **RT-2 — the `VgmLogger` decode + exposure (NEXT).** Add the `VgmLogger` `BusEventSink` (RT1/RT2/RT3
  decoders + RT4 record model + RT6 frame-bucketed render) in `oracle-core`; wire `vgm_start`/`vgm_stop`/
  `vgm_status` as the caller surface over `run_frames_with_sink` (RT5). **No 68k-side change** — the FM/PSG
  writes are already in the stream (RT3). **Verified independently** by feeding a **synthetic `BusEvent`
  stream** (hand-built FM latch-then-data + PSG latch/data sequences) through the logger and asserting the
  decoded triples + the rendered VGM bytes — **no Z80 core, no ROM, no Oracle needed**, so it is trivially
  currency-neutral (opt-in sink, `()` path untouched; RT5). Optionally, in the same or an adjacent slice, fill
  the `export_state` FM/PSG regions (6/7) live from the register file — a content change, **no golden move**
  (zero writes at the testrom capture point, S10).

- **RT-3 — the Oracle A/B differential.** Stand up a **sound-driving fixture**
  (release the Z80, upload a real SMPS driver, run to music-playing — RT7), capture the `VgmLogger` stream in
  oracle-next and `vgm_start` in Oracle, and compare **register-write triple sequences** (with
  `get_channel_states` as a secondary sanity check). **The Z80 execute core is already landed** (documented ISA
  complete, wired live in Z-live), so RT-3 is unblocked on the CPU side — its prerequisite is the sound-driving
  fixture itself plus pinning Oracle's `.vgm` framing against a real capture at build time (RT7 risks). Not
  currency-affecting (a new out-of-band fixture + a comparison, like the SST/ZEX harnesses).

**Owner-gated decisions:** **none are forced by frozen currency** — Phase RT touches no frozen currency (it is
an opt-in caller-owned sink over an existing event stream; no fixture is sound-driving; the reserved
`export_state` regions absorb the register file with no version bump). The only calls surfaced for the overseer
are **design judgment**, not currency gates: (i) RT4 — store normalized records vs. raw VGM bytes
(recommended: records + on-demand VGM render); (ii) RT6 — ship frame-bucketed waits now and defer
sample-accurate waits (recommended), or thread `mclk` into `on_step_boundary` up front for sample precision.
Both are reversible and neither moves a currency.

---

## Summary (the eight asks)

1. **FM decode (RT1).** A per-bank address latch (`addr_latch[2]`): an address-port write (`$4000/$A04000`,
   `$4002/$A04002`) latches the register number; a data-port write (`$4001/$A04001`, `$4003/$A04003`) completes
   a `(bank, latched reg, value)` triple → VGM `0x52`/`0x53`. Latch persists across the capture like the
   hardware pointer. (Plutiedev YM2612 + VGM spec.)

2. **PSG decode (RT2).** Self-describing bytes: `bit7=1` = latch (channel/type + low nibble), `bit7=0` = data
   (high 6 bits of the latched register); one latched selector, no address port. Every byte → VGM `0x50 dd`
   verbatim; the latch is tracked only for the register-file/channel view. (Plutiedev PSG + VGM spec.)

3. **One chip, two windows (RT3).** Classify on `addr` alone, `fc`-agnostic; `$4000`/`$A04000` fold into the
   same FM state, `$7F11`/`$C00011` into the same PSG state. The 68k side needs **no new tap** — `MegaDriveBus`
   already emits a `BusEvent` per access, so both 68k sound windows are already in the stream (verified in
   `bus.rs`).

4. **Representation (RT4, judgment call).** Store a normalized `(chip, port, reg, value, mclk)` **record**;
   render to canonical VGM (`0x52`/`0x53`/`0x50` + `0x61`/`0x62` waits + `0x66`, LE, clocks at header `0x0C`/
   `0x2C`) on demand. Records give register-sequence A/B independent of wait encoding; VGM render gives
   byte-level interop with Oracle's `.vgm`. Distinct from the `export_state` register-file snapshot (regions
   6/7), which is fed by the same writes.

5. **Location + exposure (RT5).** A `VgmLogger: BusEventSink` in `oracle-core`, attached via the existing
   `run_frames_with_sink` (which already threads one sink through both `step_cpu` and `catch_up_z80`). Caller
   surface `vgm_start`/`vgm_stop`/`vgm_status` mirrors Oracle; the MCP binding is a later `oracle-bus` slice.
   **Currency-neutral**: opt-in sink, `()` path unchanged, no committed fixture releases the Z80 or plays sound
   → zero captured writes in every gate; the logger touches no `System` state.

6. **Timing (RT6).** Frame-bucketed waits from the free `on_step_boundary(pc, frame)` stamp — one `0x62`
   (735-sample) frame-wait per frame, drift-free (NTSC frame ≈ 735 samples), matching the SMPS
   vblank-batch idiom, **no mechanism change**. Sample-accurate sub-frame waits are the deferrable open item
   (upgrade = carry `mclk` in `on_step_boundary`, a default-carrying trait extension).

7. **RT-3 methodology + feasibility (RT7).** Primary differential = **register-write triple-sequence equality**
   against Oracle's `.vgm`; `get_channel_states` is a coarse secondary; `audio_spectrum` is Phase-SY.
   **Feasibility finding: NO committed fixture drives the sound driver** — `oracle_differential` is static
   bytes, the Gunstar/TF4/Batman differentials are render-only bus/VDP fixes with explicitly no Z80/FM core,
   and every currency gate holds the Z80 in reset. RT-3 must **stand up a sound-driving fixture** (released Z80
   + real SMPS driver). The Z80 execute core is **already landed** (708k SST-z80, wired live in Z-live), so this
   is unblocked on the CPU side — the prerequisite is the fixture + pinning Oracle's `.vgm` framing.

8. **Synthesis boundary (RT8).** RT only taps and logs register writes; envelope/operator/DAC/LFSR synthesis to
   PCM is Phase SY, off by default (CHARTER). Nothing in RT produces a sample. This keeps RT within the
   reserved `export_state` sizes with no version bump.

## Sources

- **VGM file-format specification** (vgmrips wiki) — command set: `0x50 dd` (SN76489), `0x52 aa dd` / `0x53 aa
  dd` (YM2612 port 0 / port 1 register writes), waits `0x61 nn nn` / `0x62` (735 = 1/60 s) / `0x63` (882 = 1/50
  s) / `0x7n` (n+1), `0x66` end-of-data; 44100-sample timebase; little-endian; header chip-clock fields at
  `0x0C` (SN76489) and `0x2C` (YM2612).
- **Plutiedev** — [YM2612 registers](https://plutiedev.com/ym2612-registers) (`$4000-$4003` two-bank
  latch-then-data), [PSG chip](https://plutiedev.com/psg-chip) (`$7F11`/`$C00011` self-describing bit-7
  latch/data byte, mono, write-only), [Using the Z80](https://plutiedev.com/using-the-z80) (the sound-window
  addresses).
- **SpritesMind** — Genesis sound-chip hardware threads (write-protocol corroboration; the deferred synthesis
  detail lives here for Phase SY).
- **Oracle MCP schemas (A/B targets only, not implementation source)** — `emulator_vgm_start`/`vgm_stop`/
  `vgm_status` (YM2612 + SN76489 → `.vgm`), `emulator_get_channel_states` (fm1..fm6/dac/psg1..psg3/psg_noise
  enable), `emulator_audio_spectrum` (Phase-SY FFT).
- **oracle-next in-repo (precedent):** `crates/oracle-core/src/z80/bus.rs` (the landed RT-1 tap — raw-Z80-addr
  `BusEvent`, `fc = 0`, one-chip-two-windows note), `crates/oracle-core/src/bus.rs` (`BusEventSink` /
  `BusEvent`, `MegaDriveBus::emit` emitting per-access events so the 68k `$A04000`/`$C00011` writes are already
  in the stream, the FM `$A04000-3` carve-out), `crates/oracle-core/src/system.rs`
  (`run_frames_with_sink`/`run_until_with_sink` threading one sink through `step_cpu` + `catch_up_z80`,
  `on_step_boundary(pc, frame)`), `crates/oracle-core/src/z80/mod.rs` (`Z80Regs`/`export_region` — the Z80
  introspection surface a Tier-0 cross-check would use), `crates/oracle-core/tests/oracle_differential.rs`
  (static captured bytes — the no-sound-fixture finding), `docs/2026-07-22-differential-rom-findings.md`
  (Gunstar/TF4/Batman resolved with no Z80/FM core), `docs/export-state-v1.md` (reserved FM `0x200` / PSG
  `0x10`, no-bump-on-content-fill rule), `CHARTER.md` / `docs/foundations.md` (synthesis off by default, audio
  hashed as the VGM register stream), `docs/2026-07-22-sound-stack-recon.md` (S2/S10/S13 — the RT-tap plan),
  `docs/2026-07-22-z80-core-design.md` (ZC12/ZC13 — the tap map + the out-of-band driver harness).
