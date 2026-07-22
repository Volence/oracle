# Sound-stack recon — sequencing + gate analysis for Z80 + YM2612 (FM) + SN76489 (PSG)

**Status: RECON + SEQUENCING, 2026-07-22. Docs only — no code this slice.** This is the *sequencing and
gate* recon that precedes the sound build-out. It does **not** specify the Z80 opcode set, the FM operator
model, or the PSG waveform math — each of those is a per-phase recon that follows this one (same gated rhythm
as the VDP: recon → overseer review → greenlit slice). Its job is to (1) recommend the phased build order and
the smallest first *landable* increment, (2) do the load-bearing **frozen-currency / gate analysis** against
the actual code, (3) recommend the validation strategy against the C++ Oracle, and (4) name what is
explicitly out of the early phases, with pins.

**Permitted sources only** (audit policy 3, same as the VDP/IO/BUSREQ/FM recons): official Sega
documentation, Plutiedev, SpritesMind hardware threads, the Zilog Z80 datasheet / official Z80 CPU User
Manual, the YM2612 and SN76489 datasheets, and the community VGM format spec. **No emulator source opened**
(not BlastEm, Ares, jgenesis, or the C++ Oracle). Grounding read: `crates/oracle-core/src/system.rs`,
`state_hash.rs`, `bus.rs`, `scheduler.rs`, `CHARTER.md`, `docs/foundations.md`, `docs/export-state-v1.md`,
`docs/plans/2026-07-16-vdp-timing-skeleton.md` (the currency-neutral-first template), and the two Z80/FM bus
recons (`docs/2026-07-22-z80-busreq-recon.md`, `docs/2026-07-22-fm-status-recon.md`).

Items are numbered **S1–S14**.

---

## Framing (settled, not relitigated)

The differential work already established that **no game needs the Z80 for bus arbitration** — that is fully
handled by two bus-level latches (`z80_busreq`, and the `z80_reset` latch this recon adds), with no Z80
instruction execution (`docs/2026-07-22-z80-busreq-recon.md` Part 3, `docs/2026-07-22-fm-status-recon.md`).
This recon is about the **other** role of the Z80: it is the **sound-driver host**. On the Genesis the sound
driver (SMPS and its kin) is a Z80 program the 68000 uploads into Z80 RAM at boot; the driver sequences music
and SFX and writes the YM2612 and SN76489. "Play the game with sound" therefore *is* "run the Z80 as the
driver host and let it drive the two sound chips." We build it as the driver host, not as another
bus-arbitration guess.

---

## Part 1 — Sequencing (S1–S5)

### S1 — The three pieces and their real dependency graph

**PINNED (architecture).** The Genesis sound path is three interlocking devices, and the dependency runs
**one way**:

- The **Z80 CPU** is the *host*. From the Z80's own address space it sees: `$0000–$1FFF` Z80 RAM,
  `$4000–$4003` the **YM2612** (FM) address/data ports, `$6000` the bank-address register, `$7F11` the
  **SN76489** (PSG) port, and `$8000–$FFFF` a windowed view onto the 68000 bus (bank-selected). The 68000
  additionally sees the FM directly at `$A04000–$A04003` (the same chip, decoded on the 68k side) and the
  whole Z80 RAM at `$A00000–$A0FFFF`.
- The **YM2612** and **SN76489** are **memory-mapped slave devices** — they never initiate anything. They
  receive register writes and (for the FM) return a status byte. On a running system essentially all of those
  writes come **from the Z80 driver**; only a handful of games poke the FM directly from the 68000 during
  boot init (Gunstar's `$66360` sequence is the canonical example, already handled by the FM-status stub).

So the graph is **Z80 → {FM, PSG}**, with a thin 68k→FM side-channel that already exists as a stub. FM and
PSG have **no producer** of register writes until the Z80 driver runs. This is the fact that fixes the order.

**Confidence**: high. **Classification**: architectural. **Open remainder**: the exact Z80 memory map beyond
the four sound-relevant regions (bank register `$6000`, bank window `$8000+`) is pinned in the Z80-core
recon, not here.

### S2 — Verdict on the "Z80 first, then FM, then PSG" hypothesis: **confirm Z80-first, collapse FM/PSG into one register-tap layer, and split synthesis off the end**

**PINNED (sequencing).** The obvious hypothesis is *mostly* right but two of its three joints move:

1. **Z80 core first — confirmed, and it is nearly mandatory, not merely preferred.** FM and PSG cannot be
   *exercised* (and therefore cannot be *validated*) until something writes their registers, and on a real
   system that something is the Z80 driver. Building FM/PSG synthesis before the Z80 would leave both chips
   with no producer and nothing to diff. The one thing you *can* validate without the Z80 — the Z80 CPU core
   itself — is exactly the piece that has a self-contained, Oracle-independent gate (ZEXDOC/ZEXALL, the Z80
   analog of SingleStepTests). So Z80-first is both the dependency-forced order *and* the lowest-risk,
   mechanically-verifiable-first order. It matches `foundations.md` ("Z80 gated on SingleStepTests/z80 +
   ZEXALL/ZEXDOC ... *then* the VDP/sound").

2. **"FM then PSG" collapses.** At the level that actually matters first — a **register-write tap** (catch
   each port write, log it as a deterministic event; no sound synthesis) — the FM and the PSG are the *same
   kind of thing*: a write-decoder into a small register file plus a VGM-style event stream. They are one
   slice, not two, and they land **together** as the layer directly above the Z80 core. Splitting them buys
   nothing; both are just port-write consumers on the same `BusEventSink` stream the engine already has.

3. **Synthesis splits off the tail.** Turning those register writes into actual audio samples (FM operator
   envelopes/DAC, PSG tone/noise waveforms) is a *separate* final layer, deferred and **off by default in
   headless runs** (CHARTER "Audio: synthesis off by default"; `foundations.md` "audio hashed as the VGM
   register stream, not PCM"). It is not on the critical path to "plays the game with correct driver
   behavior."

**Refined build order:**

```
Phase Z   Z80 CPU core            (host; gate: ZEXDOC then ZEXALL, Oracle-independent)
Phase RT  FM + PSG register-tap   (one slice: write-decode + VGM event stream; NO synthesis)
Phase SY  Synthesis backend       (deferred, off by default: ymfm-class FM, datasheet PSG)  ← tail
```

**Confidence**: high. **Classification**: sequencing. **Open remainder**: none — the per-phase recons refine
*within* each phase.

### S3 — The smallest first *landable* increment: a **currency-neutral Z80 execution skeleton** (held in reset)

**PINNED (the VDP-timing-skeleton analog).** The VDP started with a push that wired the whole `Vdp` struct,
the timing FSM, and the scheduler events **but left the `export_state` VDP region emitting zeros** until a
later, attributable slice flipped it live (`docs/plans/2026-07-16-vdp-timing-skeleton.md`, slices 1–4 build,
slice 5 goes live + regenerates the golden). The Z80's equivalent first landable slice:

- Add a `z80: Z80` field to `System` (its register file + execution state), a `Z80Bus` split-borrow adapter
  (the Z80's memory map, analogous to `MegaDriveBus`), and an **mclk/15** stepping slice inside `run_until`
  (the Z80 clock is master/15; the 68000 is master/7 — `foundations.md`).
- **Gate execution on the two arbitration latches**: the Z80 may step only when **reset is released AND the
  68000 is not holding the bus** (`z80_reset && !z80_busreq`). This requires promoting `z80_reset` from
  today's "reads 0, writes drop" stub (`bus.rs:307`) to a real write-catching latch, with **power-on =
  reset asserted** (real hardware holds the Z80 in reset until the 68000 releases it — Plutiedev "Using the
  Z80"; BUSREQ recon Z4).
- **Keep the `export_state` Z80-register region (region 4, the reserved `0x40`) emitting zeros** this slice —
  do not flip it live yet. Validate the Z80 core out-of-band (ZEXDOC/ZEXALL + a manual "release reset, run a
  real driver" harness), exactly as the 68000 is validated out-of-band by SingleStepTests.

Because no frozen-currency fixture ever releases the Z80 from reset (S7), the skeleton executes **zero Z80
instructions in every committed gate** → all five currencies stay byte-identical. It is the currency-neutral
first push: the Z80 is wired and runnable, but invisible to the frozen currencies until a deliberate later
slice makes it live.

**Confidence**: high. **Classification**: sequencing. **Open remainder**: the Z80's internal step model
(cycle-stepped floooh-style vs. instruction-stepped-with-serializable-state) and the 68k↔Z80 interleave
determinism (S14) are the Z80-core recon's to settle.

### S4 — After the skeleton: the two remaining Phase-Z slices, then Phase-RT

**PLANNED (indicative slicing, refined by the Z80-core recon).**

- **Z-skeleton** (S3): struct + `Z80Bus` + mclk/15 slice + `z80_reset` latch, held in reset. Currency-neutral.
- **Z-execute**: the Z80 opcode set behind the CHARTER's serializable-state constraint; gate ZEXDOC then
  ZEXALL. Still currency-neutral in the committed gates (no fixture releases reset).
- **Z-live**: flip the `export_state` Z80-register region live + regenerate the golden in its own attributable
  commit (the VDP slice-5 pattern). This is where the determinism currency starts covering Z80 register
  state. (S9 covers whether this moves the golden.)
- **RT-tap** (Phase RT): the FM + PSG register-tap + VGM event stream, and the FM/PSG `export_state` regions
  going live. One slice (S2). (S10 covers the gate.)

### S5 — Could FM/PSG register scaffolding land *before* the Z80? Yes, but it has no payoff — do Z80 first

**PINNED (rejected alternative, recorded).** One could extend today's 68k-side FM stub into a passive
register-file at `$A04000–$A04003` before the Z80 exists, catching the rare direct-68k FM writes (Gunstar
boot). It would be a small, currency-neutral change. **But its value is near-zero without the Z80 driving the
chips** (no music/SFX flows through the 68k side), and making the FM register file *live* in `export_state`
before there is a driver to fill it buys nothing. Recommendation: **do not** pull FM/PSG scaffolding forward;
land it as one slice on top of the Z80 core (S2), where it immediately becomes the differential surface for
real driver output. Recorded so it is a considered choice, not an oversight.

---

## Part 2 — Gate / currency analysis (S6–S11) — the load-bearing part

The five frozen currencies (each must stay byte-identical unless a change is explicitly justified):

1. **SST** — the 68000 single-step opcode currency (`ran >= 1_000_058`).
2. **export golden / `export_state_hash`** — oracle-next's own determinism currency (`export_state_v1.rs`).
3. **golden_frames** (7) — bus-less `Vdp` scenes.
4. **oracle_differential** (3) — captured static VDP bytes through FNV-1a.
5. **determinism** — same-seed `run_frames` reproducibility.

Plus the Oracle-compatible **`state_hash`** (VDP-only FNV), which is not one of the five determinism gates but
is the cross-Oracle currency and is analyzed in S11.

### S6 — The reserved export regions exist exactly as memory/recon claimed — **verified against the code**

**PINNED.** `crates/oracle-core/src/system.rs` defines, and `docs/export-state-v1.md` freezes, three
all-zero reserved regions in the `export_state` image, at these offsets/sizes:

| Region | `export_state` offset | Size | Constant (`system.rs`) | Current contents |
|---|---|---|---|---|
| Z80 registers | `0x12050` | `0x40` / 64 | `EXPORT_Z80_REGS_PLACEHOLDER = 0x40` | all-zero |
| FM (YM2612) | `0x22178` | `0x200` / 512 | `EXPORT_FM_PLACEHOLDER = 0x200` | all-zero |
| PSG (SN76489) | `0x22378` | `0x10` / 16 | `EXPORT_PSG_PLACEHOLDER = 0x10` | all-zero |

The Z80 RAM immediately before region 4 (`0x10050`, `0x2000`) is **already live** (the
`export_state_captures_live_z80_ram` test). So the export format **does** pre-reserve all three sound regions,
at the exact offsets and sizes the memory note claimed. **Confirmed, not corrected.**

The version-bump rule (`export-state-v1.md`): filling a reserved zeroed region with live bytes **at unchanged
size is a *content* change — no version bump** (the path the VDP region 5 already took). A version bump is
required **only** if a region is added/removed/reordered/**resized**. This is the pivot the whole gate
analysis turns on.

### S7 — Does a *running* Z80 touch SST? **No — structurally isolated, same as BUSREQ/FM**

**PINNED.** SST runs `Cpu68000::step` over a `FlatBus`, constructed directly in
`crates/oracle-core/tests/singlestep_m68000.rs` (`build_bus → FlatBus::new`, `Cpu68000::new`). It **never
instantiates `System`**, has no scheduler, and has no Z80. The Z80 core is added as a `System` field driven by
`System::run_until` — a code path SST does not exercise at all. So SST cannot see the Z80 whether it runs or
not; the mechanism that keeps SST invariant is **the absence of `System`/`Z80` from the SST harness**, the
identical structural isolation that made the BUSREQ and FM slices SST-neutral (FM recon Part 2). The Z80 also
does not "run during SST" in any sense — there is nothing for it to run inside. **SST is unaffected by
construction.**

### S8 — Does a running Z80 touch export golden / golden_frames / oracle_differential / determinism? **Not in the committed fixtures — because none release the Z80 from reset**

**PINNED (the key gate finding).** A running Z80 changes state only through two channels the currencies could
see: (a) its **register file** (if region 4 is live), and (b) **`z80_ram`**, which is *already* live in
`export_state`. Both are gated behind **the Z80 actually executing**, which requires reset released
(`z80_reset` true) AND the bus not held (`!z80_busreq`). Enumerate the fixtures:

| Gate | Fixture | Releases Z80 reset / runs a driver? | Neutral because |
|---|---|---|---|
| export golden (`export_state_v1`) | vendored `testrom::build()`, RAM-stir loop | **No** | The ROM never writes `$A11200`/`$A00000`; the test already asserts the whole Z80-RAM region is all-zero. Z80 stays in reset → 0 instructions → `z80_ram` and Z80 regs unchanged. |
| golden_frames (7) | bus-less `Vdp` scenes | **No** | Never instantiates the bus or `System`; no Z80 path exists. |
| oracle_differential (3) | captured static VDP bytes | **No** | No `System`, no bus, no live run — pre-captured bytes through FNV-1a. |
| determinism | vendored `testrom::build()` | **No** | Same RAM-stir ROM; Z80 held in reset, deterministic regardless. |
| SST (1,000,058) | `FlatBus` | **No** | No `System`/Z80 at all (S7). |

**None of the committed frozen fixtures release the Z80 from reset**, so with power-on `z80_reset = asserted`
the Z80 executes nothing in any of them. The skeleton (S3) and even a fully-executing Z80 core (S4 Z-execute)
are therefore **currency-neutral by construction** — the same evidence-class argument the FM slice used, now
resting on the reset gate. The neutrality must still be *proven* empirically (re-run all five gates
byte-identical), construction-argument first.

### S9 — When the Z80-register region goes live (Z-live slice): content change, no bump; golden move depends on the modeled reset-state bytes

**PINNED.** Flipping region 4 (`0x40`) from zeros to the live Z80 register file is a **content change at
unchanged size → NO version bump** (S6 rule; the VDP-region-5 precedent). Whether it **moves the export
golden** depends on what the Z80's register file holds *at the testrom capture point* — where the Z80 has
never run and sits in **reset state**:

- If the modeled Z80 reset-state register file serializes as **all-zero**, the golden is **unchanged** (region
  was already zero).
- If it serializes **non-zero** (e.g. a model that sets SP or the AF/undefined pairs to `$FFFF` at reset —
  the Z80 reset only strictly defines PC=0, I=0, R=0, interrupts disabled/IM 0; SP and the main registers are
  architecturally undefined and emulators vary), the golden **moves once** — a **legitimate one-time regen**
  in its own attributable commit, exactly as the VDP region-5 slice regenerated its golden. It is *not* a
  version bump.

**Recommendation:** keep region 4 zeroed through the skeleton + execute slices (currency-neutral, S8), and do
the go-live as a dedicated Z-live slice containing *only* the emission change + the regenerated golden
constant (attributability — the VDP slice-5 discipline). Pick the reset-state register model deliberately in
the Z80-core recon and pin it; do not let it drift.

### S10 — When FM + PSG register regions go live (RT-tap slice): content change, no bump, and **golden does not even move**

**PINNED.** The FM (`0x200`) and PSG (`0x10`) reserved regions are sized to the chips' **register-file
scale** (the addressable register set), not their full analog internals. A **register-tap** model (S13) —
which stores the last-written value per register and nothing else — fits inside these sizes. So going live is
again a **content change, no version bump**. Better still: at the **testrom capture point the FM and PSG have
received zero writes** (no driver runs — S8), so their register files are all-zero → **going live does not
move the export golden at all** (cleaner than the Z80-register case). The FM/PSG regions can flip live in the
RT-tap slice with the golden provably unchanged, and only later fixtures that actually run a driver would
exercise non-zero content.

**The v2-bump path is named and avoided:** if a *future* synthesis backend ever wanted the chips' **full
internal state** (FM envelope/phase accumulators, LFO; PSG LFSR + counters) inside `export_state`, that state
**exceeds** `0x200`/`0x10` → resizing the region **is** a layout change → **v2 bump** (`export-state-v1.md`).
The register-tap model deliberately stays within the reserved sizes, so the sound build-out reaches
"plays with correct driver output" **without any version bump**. Whether analog synthesis state ever belongs
in the determinism currency is itself deferred (S12) — it likely never does (the DAC is derived and compared
via the audio-spectrum op, not the byte currency), so the v2 path may never be walked.

### S11 — Does `state_hash` (the Oracle-compatible FNV) include Z80/FM/PSG? **No — it is VDP-only, so our hash keeps matching Oracle regardless of sound**

**PINNED.** `crates/oracle-core/src/state_hash.rs` hashes **only** the four VDP regions — VRAM (`0x10000`),
CRAM (`0x80`), VSRAM (`0x50`), and the 24 VDP registers — in Oracle's exact byte order, and its doc comment
states it is "byte-for-byte identical to Oracle's `OpStateHash`," which "hashes VDP memory + registers only."
`System::state_hash` reads only `self.vdp.*`. **Neither our `state_hash` nor the C++ Oracle's `OpStateHash`
includes Z80 registers, FM, or PSG state.** Consequences, both good:

- Adding Z80 execution and FM/PSG chips **cannot** move `state_hash` — it is not in that currency. Our hash
  continues to match Oracle's byte-for-byte once sound lands, with no coordination needed. This is
  **knowable now** from `state_hash.rs` and the frozen Oracle layout; nothing needs pinning against Oracle
  later for the *hash-matching* question.
- The flip side: `state_hash` gives **no cross-Oracle coverage of sound**. Validating Z80/FM/PSG against
  Oracle needs a *different* differential surface — the VGM register stream and the Z80-register/RAM
  introspection ops (Part 3). `state_hash` stays the VDP differential; sound gets its own.

What is **not** knowable purely from `docs/` and must be pinned against Oracle **when the differential is
built** (not now): the exact byte/field ordering of Oracle's `emulator_z80_registers` output and its VGM
register-log framing, so a lockstep comparison lines up field-for-field. That is a Part-3 pinning task at
differential-build time, not a blocker for the Z80 core.

---

## Part 3 — Validation strategy against the C++ Oracle (S12–S13)

The C++ Oracle (Exodus-based) has working audio and exposes the relevant MCP ops — confirmed in the tool
surface: `emulator_z80_registers`, `emulator_z80_read`/`z80_write`, `emulator_get_channel_states`,
`emulator_vgm_start`/`vgm_stop`/`vgm_status`, and `emulator_audio_spectrum`. It is our differential oracle for
sound.

### S12 — Three differential tiers, cheapest first; **do not require bit-exact DAC synthesis on day one**

**PINNED (validation ladder).**

1. **Tier 0 — Z80 CPU correctness, Oracle-independent (cheapest, first).** Gate the Z80 core on
   **ZEXDOC then ZEXALL** (the Z80 analog of SingleStepTests; `foundations.md`). This needs only Z80 RAM
   (already present) and no Oracle at all — it is the mechanically-verifiable, self-contained first gate, and
   it validates the *host* before anything downstream can be trusted. Optionally corroborate with a lockstep
   `emulator_z80_registers` / `emulator_z80_read` comparison on a real ROM's driver (run N Z80 instructions in
   both cores, diff register files + Z80 RAM) — but ZEXALL is the rigorous gate; the Oracle lockstep is a
   convenience cross-check.

2. **Tier 1 — driver + chip-register correctness via the VGM register-write stream (the cheapest *sound*
   differential).** The Z80 driver's writes to the FM ports (`$4000–$4003` Z80-side / `$A04000–$A04003`
   68k-side) and the PSG port (`$7F11`) are a **deterministic event stream**. Capture it (the FM/PSG port
   writes are just `BusEvent`s on the Z80 bus — the existing `BusEventSink` instrumentation is exactly the
   right consumer, no new mechanism) as a VGM-style register log, run the same ROM to the same point in
   oracle-next and in Oracle (`emulator_vgm_start`/`stop`, `emulator_get_channel_states`), and **A/B the
   register-write streams**. This tests **driver execution + chip-register decode** *independently of the
   analog synthesis* — the whole point of the VGM-as-currency approach (`foundations.md`: "audio hashed as the
   VGM register stream, not PCM"). It is the cheapest first differential that covers real sound behavior, and
   it fits the reserved export regions (S10) so it costs no version bump.

3. **Tier 2 — analog synthesis fidelity (deferred, off by default).** Only once Tier 1 is green does
   bit-/perceptually-exact DAC output matter, compared via `emulator_audio_spectrum` against the synthesis
   backend (ymfm-class FM per CHARTER; PSG from the datasheet; Nuked-OPN2 as the FM golden per
   `foundations.md`). This is the Phase-SY tail (S2), off by default in headless runs.

**The cheapest first differential is Tier 1's VGM register-write stream** (after the Oracle-free ZEXALL gate
on the CPU itself). It requires no synthesis, reuses the existing event-stream instrumentation, and diffs
against an op surface Oracle already exposes.

### S13 — What the register-tap actually stores (keeps Tier 1 within the reserved export sizes)

**PINNED (data model).** The FM/PSG "chips" for Phases Z + RT are **write-decoders into a small register
file**, not synthesizers:

- **FM**: bank-0/bank-1 address latches + the last value written to each addressable register (fits `0x200`);
  status reads stay the not-busy stub (FM recon F2/F4). The Z80-side `$4000–$4003` and the 68k-side
  `$A04000–$A04003` decode to the **same** register file (one chip, two windows).
- **PSG**: the tone/volume/noise register latches + the current latched-register selector (fits `0x10`).

Both also emit their writes to the `BusEventSink` as the VGM event stream (Tier 1). No envelope, no phase
accumulator, no LFSR, no DAC — those are Phase-SY and, if ever needed in the byte currency, the v2 path (S10).
This is what makes "FM/PSG go live" a content change with no golden move and no version bump.

---

## Part 4 — Scope boundaries / deferrable pieces (S14) — named, not silent

**PINNED deferrals** (each with its pin source for when it is picked up):

- **The 68k↔Z80 interleave determinism** — *not deferrable past the Z80 core; it is the Z80-core recon's
  central design question.* The scheduler owns the sole clock; the Z80 advances on the same mclk timeline at
  **master/15**. The interleave of 68000 and Z80 steps, and the sync points when one touches shared state
  (Z80 bank window → 68k bus; 68000 → Z80 RAM while granted), **must be deterministic** (CHARTER
  non-negotiable 1). Flagged here as the #1 Z80-core design risk; resolved in that recon (the ratified
  sync-on-demand model + a defined interleave order are the starting point).
- **Z80 serializable-state constraint** — the Z80 core must fit the CHARTER: fully serializable, cheap
  snapshot, **no live state on host stacks** (the exact reason Ares' libco was rejected). It must be
  cycle-steppable or at least bus-access-quiescable with all state in the struct, mirroring the 68000's
  single-definition micro-op approach. The step model (floooh-style per-clock FSM vs. instruction-stepped
  with serializable micro-state) is the Z80-core recon's call; the *constraint* is pinned now.
- **Z80 `/INT` line** — the VDP raises a Z80 interrupt once per frame (vblank); the Z80 driver's timing
  depends on it. Deferred to the Z80 core (it was explicitly out of the VDP timing skeleton: "the Z80 /INT
  line — no Z80 core"). Pin from the VDP recon + Z80 datasheet interrupt-mode semantics when the core lands.
- **Z80 undocumented opcodes + the X/Y (bits 5/3) undocumented flags** — gate **ZEXDOC first** (documented
  behavior), then **ZEXALL** (all behavior incl. undocumented flag propagation and the undocumented
  ED/DD/FD opcodes). Real SMPS drivers are unlikely to depend on undocumented behavior, so ZEXDOC unblocks
  driver validation; ZEXALL is the harder accuracy gate that follows. Pin: the Z80 CPU User Manual + the
  ZEXALL/ZEXDOC suites.
- **Exact FM operator envelope / DAC analog fidelity** — Phase-SY; off by default headless (CHARTER). Pin:
  YM2612 datasheet + ymfm/Nuked-OPN2 as the golden reference (a *differential* oracle, not source to copy).
- **PSG noise LFSR taps + waveform synthesis** — Phase-SY. The register-tap logs the writes; the actual
  15-bit LFSR tap sequence and tone/noise waveform generation are synthesis. Pin: SN76489 datasheet +
  SpritesMind PSG hardware-test threads.
- **FM timer-overflow status flags (F5)** — status bits 0/1 stay 0 (no timers modeled); a consumer that
  *waits for* a timer overflow would hang, but no boot/render/driver-bring-up path does. Already deferred in
  the FM recon; needs the FM core's timers. Pin: YM2612 datasheet.
- **Sub-cycle Z80↔68000 bus contention timing** (~3.3 Z80 / ~11 68000 cycles average) — a timing property
  (BlastEm-differential class), already deferred in BUSREQ recon Z3. No boot/render/driver-correctness
  consumer observes it; the instant-grant model stays observationally safe.
- **The broader Z80 memory map correctness** ($A06000 bank register, $A07F00–$A07F1F VDP mirror, $A08000+
  bank window) — today these are wrongly mirrored as Z80 RAM (FM recon "out of scope"). They come in **with**
  the Z80 core (the `Z80Bus` needs the bank register `$6000` and the `$8000+` bank window to reach the 68k
  bus/VDP), so they are *required for the Z80 core*, not deferrable past it. Pin: Plutiedev Z80 bank
  switching + the Kabuto hardware notes.

---

## Summary (the four asks)

1. **Sequence.** Z80 CPU core **first** (dependency-forced *and* lowest-risk: it is the driver host and has an
   Oracle-independent gate) → FM + PSG as **one** register-tap/VGM slice (the "FM then PSG" split collapses at
   the register-tap level) → analog synthesis as a **deferred, off-by-default tail**. Smallest first landable
   increment: a **currency-neutral Z80 execution skeleton held in reset** (struct + `Z80Bus` + mclk/15 slice +
   a real `z80_reset` latch, power-on = reset asserted), the direct analog of the VDP timing-skeleton push.

2. **Gate verdict.** The export format **really does** pre-reserve all three sound regions, verified against
   `system.rs` at the exact offsets/sizes (Z80 regs `0x40` @ `0x12050`, FM `0x200` @ `0x22178`, PSG `0x10` @
   `0x22378`), all currently zero. Filling them is a **content change, no version bump** (the VDP-region-5
   rule). **SST** is unaffected by construction (no `System`/Z80 in the harness). The other four gates are
   neutral because **no committed fixture releases the Z80 from reset**, so it executes nothing in any of them
   — construction-argument, prove empirically. What *moves*: only the export golden, and only when a region
   goes live — **FM/PSG going live does not move it at all** (zero writes at the testrom capture point); the
   **Z80-register region moving it depends on the modeled reset-state bytes** and, if it moves, is a
   **one-time legitimate regen** in an attributable slice, not a bump. **`state_hash` is VDP-only** in both
   oracle-next and Oracle, so sound never touches it and our hash keeps matching Oracle for free — no v-bump,
   the register-tap model stays within the reserved sizes; the only path to a v2 bump (full analog synthesis
   state in the byte currency) is named and deliberately avoided.

3. **Cheapest first differential.** After the **Oracle-free ZEXALL gate** on the Z80 CPU, the **VGM
   register-write stream** — capture the driver's FM/PSG port writes off the existing `BusEventSink` and A/B
   them against Oracle's `vgm`/`channel_states` ops. Tests driver + chip-register correctness with **no DAC
   synthesis**; analog fidelity (`audio_spectrum`) is the deferred Tier 2.

4. **First landable slice.** The Z80 execution skeleton of point 1: wired, runnable, gated on
   `z80_reset && !z80_busreq`, **invisible to all five frozen currencies** because every committed fixture
   leaves the Z80 in reset. Validate out-of-band (ZEXDOC/ZEXALL + a manual driver-run harness), exactly as the
   68000 is validated out-of-band by SingleStepTests. The `export_state` Z80-register region stays zeroed
   until a later, attributable go-live slice.

## Sources

- [Plutiedev — Using the Z80](https://plutiedev.com/using-the-z80), [YM2612 registers](https://plutiedev.com/ym2612-registers), [PSG](https://plutiedev.com/psg-chip), [Z80 memory map / banking](https://plutiedev.com/z80-vs-68000)
- [SpritesMind — Genesis 68K bus timing](https://gendev.spritesmind.net/forum/viewtopic.php?t=2943); Z80 BUSREQ/RESET and sound-chip hardware threads
- Zilog Z80 CPU User Manual (opcode/flag/interrupt semantics); YM2612 and SN76489 datasheets; the VGM format spec
- oracle-next in-repo: `system.rs` (reserved export regions), `state_hash.rs` (VDP-only hash), `bus.rs`
  (`z80_busreq` latch, FM stub, `z80_reset` stub), `docs/export-state-v1.md` (frozen layout + bump rule),
  `docs/plans/2026-07-16-vdp-timing-skeleton.md` (currency-neutral-first template), the two Z80/FM bus recons,
  `docs/foundations.md` / `CHARTER.md` (mclk/15, ZEXALL/ZEXDOC, VGM-register-stream audio, synthesis
  off-by-default, serializable-state non-negotiable)
