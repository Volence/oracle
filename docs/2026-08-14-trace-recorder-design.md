# The trace recorder — design pass (2026-08-14)

**Status:** design only. Nothing implemented, nothing committed by this pass. This is the S4 design gate
that `docs/plans/2026-08-14-tooling-track2-overnight.md:81-86` says must happen before dispatch:

> **S4 — candidate: the trace recorder** … **Higher risk than it looks**: attribution wants to live *in*
> the event, which collides with constraint 2 above. Needs a design pass on the forwarder-vs-field
> question before implementation, so it is not being dispatched blind overnight.

**Design of record above this one:** `docs/2026-08-14-tooling-frontier-recon.md` (the archaeology that
ranks this #1 by recurrence).

**Assumes and depends on two in-flight slices** (see §10): S1 (sink stop signal + `run_until` +
`Fanout`) and S2 (`.lst` symbol parser). Neither is implemented here.

---

## 0. Verdict, up front

**Build it — but at roughly a quarter of the size the recon doc implies, as four additive changes to
`crates/oracle-core/src/watchpoints.rs`, not as a new subsystem.** And unbundle three cheap items the
recon doc folded into "the trace recorder" that are actually separate problems: **arm-at-power-on**
(§8/C1, ~5 lines, and today structurally impossible), **scanline capture** (§9, ~60 lines, stronger
duplication evidence than half the recorder), and **`VdpWrite` timestamps** (§12).

The reason is not caution, it is measurement. Three things came back different from the recon doc:

1. **`Watchpoints` already *is* a filtered, attributed, bounded, record-time bus-event recorder**
   (`crates/oracle-core/src/watchpoints.rs`, 552 lines). It has record-time address+op filtering, PC and
   frame attribution, a monotonic `seq`, a bounded drop-oldest ring with a drop count, and both the bus
   and VDP-internal spaces. It is missing exactly four things: **mclk**, **label propagation**, **watch
   ids**, and **aggregation**. That is a much smaller delta than "the capability does not exist".

2. **The "6 ad-hoc sinks" figure does not survive inspection as evidence for a *bus trace*.** Two of the
   six (`FrameCapture`, `LineCollector`) have `fn on_event(&mut self, _event: BusEvent) {}` — a
   *byte-identical empty stub* (`crates/oracle-core/tests/conformance_roms.rs:262`,
   `crates/oracle-core/tests/scanline_capture.rs:27`). They are scanline collectors on the
   `wants_scanlines`/`on_scanline` seam and consume no bus events at all. A trace recorder would not
   have made either unnecessary. A third (`AudioAndWatch`) is a fan-out combinator that S1 is already
   replacing. A fourth (`VgmLogger`) is a chip-protocol decoder that no generic trace can subsume.
   **Committed recurrence for a generic filtered bus trace is 2, not 6** — `diag_soundqueue.rs` and
   `k4_openbus_probe.rs`. The 11 uncommitted hunts remain the real evidence, and they are enough; but
   the committed-code case is weaker than advertised and the scope should reflect that.

3. **The blast radius of the thing everyone is afraid of is 2 assertions in 1 file** (§2). The prior
   estimate on record — `docs/2026-07-23-phase-sy4-subframe-timing-design.md:97-100`, "~40 sites" and
   "tests compare recorded events for equality" — is ~3.3× too high on sites and overstated on
   semantics. The SY-4 *decision* was still right; its *justification* was folklore. That matters,
   because the folklore is now a standing constraint
   (`docs/plans/2026-08-14-tooling-track2-overnight.md:21-23`) and constraints should be true.

One thing came back **stronger** than the recon doc, and it moved this design: the aggregation half is
not the speculative part. It disproved two locatable root causes, and the corpus states outright that a
raw per-access trace *"would swamp the signal"* at this event rate (§7). **Aggregation is the primary
read mode; the event log is the fallback.** That inverts how the capability is usually built.

**The failure mode this design is written against** is named in the recon doc itself: a shipped,
charter-promised, elaborately-designed render-decode API (`CHARTER.md:43-45`) with **zero uses in 19
root-cause docs and 14 probes** (`docs/2026-08-14-tooling-frontier-recon.md:90-94`). The counter-precedent
is watchpoints: promoted as a real API, then reused unmodified three times (§2b). The difference between
the two was not design quality. It was that watchpoints was **the** thing you reach for, and the render
API was a second thing next to what people already used. **So: extend the API that already gets used.
Do not build a second one beside it.**

---

## 1. What is actually being asked for

From the archaeology, a "filtered, attributed, timestamped bus-event trace" decomposes into five
independent asks, which have been treated as one item and should not be:

| Ask | Status today | This design |
|---|---|---|
| Record-time filtering | `Watchpoints` specs (addr range + op, per space) | keep; widen slightly (§6) |
| Attribution (pc, frame) | `Watchpoints` latches both | keep |
| Attribution (mclk) | **missing** everywhere except `VgmLogger`/`AudioSink`, each hand-rolled | add (§5) |
| Attribution (master) | `fc` + address window; one port where both are wrong | **do not add a field**; the recon's justifying episode does not hold (§4.4) |
| Aggregation | **missing** — every counter hand-declared | add: count, census, cardinality, first/last (§7) |
| Bounded/cursored output | `Watchpoints` ring + `seq` + `dropped` | keep; add `seen` (§8) |

---

## 2. The blast radius, measured

`BusEvent` (`crates/oracle-core/src/bus.rs:42-49`) derives `Clone, Copy, Debug, PartialEq, Eq`. There is
**no `Default` impl and no builder** anywhere in the tree, so no literal can use `..Default::default()`
— every literal is a hard compile break if a field is added.

**Hard breaks (compile errors): 12 struct literals across 6 files.**

| File | Lines | Kind |
|---|---|---|
| `crates/oracle-core/src/bus.rs` | 190, 201, 563, 852, 886, 1398 | 3 production emit sites + 3 test literals |
| `crates/oracle-core/src/watchpoints.rs` | 317, 344 | test literal + `fn ev` helper |
| `crates/oracle-core/src/z80/bus.rs` | 226 | production emit site (FM/PSG tap) |
| `crates/oracle-core/src/vgm.rs` | 363 | test `write_event` helper |
| `crates/oracle-core/src/synth/audio_sink.rs` | 254 | test `write_event` helper |
| `crates/oracle-frontend/src/audio.rs` | 226 | test `write_event` helper |

Four of the twelve are single helper functions feeding ~25 test call sites, so the *edit* count is 12,
not 25 — which is where the earlier "~40 sites" estimate came from (it counted helper call sites).

**Exhaustive destructuring / struct patterns (`let BusEvent { .. } = e`): 0.** Nothing pattern-matches
the struct.

**Semantic breaks (compiles, assertion changes meaning): 2, both in `crates/oracle-core/src/bus.rs`.**

- `bus.rs:884-892` — `assert_eq!(sink, vec![BusEvent { … }])`: a recorded `Vec<BusEvent>` against an
  independently constructed expected event. A field with a differing runtime value fails here.
- `bus.rs:1405-1408` — `assert!(sink.contains(&tap))` where `tap` is the literal at `bus.rs:1398`.
  `contains` uses whole-struct `PartialEq`; a timestamp-like field makes this never match.

Two further whole-struct equalities exist (`bus.rs:866-869`, `bus.rs:870`, the SY-4a forwarder-equivalence
test) but both compare a value against *itself* round-tripped, so they would still pass.

Everything else is a field projection and is benign: `bus.rs:973` (`all(|e| e.fc == 0)`), `bus.rs:1417`,
`bus.rs:2569`, `z80/bus.rs:419-437` (a zip loop comparing `op`/`fc`/`size`/`addr`/`value` individually),
`system.rs:1885-1898`. The 22 test-local `Vec<BusEvent>` declarations are storage only.

**Conclusion: "adding a field breaks N assertions across M files" = 2 assertions across 1 file, plus 12
mechanical literal edits across 6 files.** That is cheap. **The field is still the wrong answer — for
reasons that have nothing to do with tests.** See §4.

---

## 3. The central tension — the options

Six shapes, each evaluated against: what breaks, what it costs, and whether the no-instrumentation path
stays bit-identical (recon §2a — three real investigations damaged by instrumentation that perturbed
scheduling, so this is not theoretical).

### Option A — add attribution fields to `BusEvent`

`BusEvent { op, fc, addr, size, value, pc, frame, mclk, master }`.

- **Breaks:** 12 literals, 2 assertions (§2). Cheap.
- **Bit-identity:** *safe*. The struct is unhashed, unserialized, in neither currency; with the null sink
  `()` the whole construction is dead code the optimizer removes. Constructing a wider value has no
  emulated side effect and cannot reorder a bus access or move the clock. **The bit-identity objection to
  a field is weak and should not be the argument used.**
- **Why it is nevertheless wrong:** see §4. Summary: **three of the four emission sites do not know the
  values**, and two of the fields are per-*step* constants being copied into per-*event* storage.

### Option B — a parallel attribution struct passed alongside the event

`fn on_event_ctx(&mut self, event: BusEvent, ctx: EventCtx)` with a defaulted forwarder, generalizing
`on_event_at` (`bus.rs:59-61`, commit `ebebe8e`).

- **Breaks:** nothing. Every existing sink compiles untouched.
- **Bit-identity:** safe by construction — a defaulted method the null sink never overrides.
- **Cost:** *the emission sites still cannot fill it.* `MegaDriveBus::emit` (`bus.rs:561-572`) holds
  `self.now_mclk` and nothing else; `SystemBus` (`bus.rs:190,201`) holds no clock at all. So `ctx` would
  be `{ mclk }` — which `on_event_at` already delivers. **This option buys nothing over what exists**
  unless the bus adapters are threaded a PC, which is exactly the structural hot-path change §2a warns
  against. **Rejected as redundant, not as harmful.**

### Option C — a richer event type layered *above* `BusEvent`

`TracedEvent { event: BusEvent, pc, frame, mclk, seq }`, produced by the recorder from the latched
context, never by the seam.

- **Breaks:** nothing.
- **Bit-identity:** safe — it exists only inside an attached consumer.
- **Cost:** essentially zero; it is a data type, not a seam change.
- **Verdict: adopt.** This already exists in embryo: `WatchHit`
  (`crates/oracle-core/src/watchpoints.rs:111-135`) is precisely this type, minus `mclk`. `diag_soundqueue`'s
  `Rec` (`crates/oracle-core/examples/diag_soundqueue.rs:19-27`) is an independent re-invention of it
  — `BusEvent` re-declared field-for-field plus a hand-copied `frame`.

### Option D — make the recording tests compare a projection

Replace `assert_eq!(sink, vec![BusEvent{…}])` with a comparison over `(op, addr, size, value)`.

- **Breaks:** nothing, but changes 2 assertions to be *weaker* — they would stop pinning `fc`, or need a
  hand-maintained projection that drifts from the struct.
- **Cost:** small but pure downside: it loosens the only two exhaustive-sequence assertions in the tree
  in order to enable a field that §4 says should not exist. **Rejected — it pays a real cost for a
  capability we do not want.** (Worth noting the reverse: those two assertions are *load-bearing*. They
  are the only place the full emitted event shape is pinned. Do not weaken them casually.)

### Option E — a decorator on the sink side

`Attributed<S: AttributedSink>` implementing `BusEventSink`, latching pc/frame/mclk and forwarding
`TracedEvent` to a second, richer trait.

- **Breaks:** nothing.
- **Bit-identity:** safe.
- **Cost:** **a second sink trait.** Every consumer must now decide which trait it implements; anything
  wanting `on_scanline` or `wants_vdp_writes` needs pass-through plumbing through the decorator; `Fanout`
  composition gets a type-level split. This is the render-decode-API failure mode in miniature: an
  elegant second surface beside the one people already use. **Rejected.**
- The *useful residue* of this option — a small reusable latch — survives as §4.3, without a trait.

### Option F — a generic event parameter on the trait

`trait BusEventSink { type Event; fn on_event(&mut self, e: Self::Event); }`.

- **Breaks:** every emission site, every sink, every `MegaDriveBus<'_, S>` type annotation (18 of them in
  `bus.rs` alone), the `Vec<BusEvent>` blanket impl, `()`.
- **Cost:** the largest possible churn for the least benefit; monomorphization across two event types
  doubles the bus code; the emission site still has to *choose* which event to build, so it still needs
  to know the attribution it does not have. **Rejected outright.**

### Decision

**C, with the latch in §4.3.** No seam change, no new trait, no field on `BusEvent`. The richer type is
`WatchHit` (extended); the relatching happens in **one** place that consumers get by using the recorder
rather than by implementing anything.

---

## 4. Why attribution genuinely does not belong in `BusEvent`

Stated as reasons, ranked by weight, because the standing constraint currently rests on the weakest one.

### 4.1 (Decisive) Three of the four emission sites do not know the values

There are exactly four production `BusEvent` construction sites:

| Site | Has `mclk`? | Has `pc`/`frame`? |
|---|---|---|
| `bus.rs:190` — `SystemBus::read` (phase-0 synthetic bus) | **no** — the adapter holds no clock | no |
| `bus.rs:201` — `SystemBus::write` | **no** | no |
| `bus.rs:563` — `MegaDriveBus::emit` | yes (`self.now_mclk`) | **no** |
| `z80/bus.rs:225-234` — the Z80 FM/PSG tap | yes (`self.now_mclk`) | **no** |

`pc` and `frame` enter the stream at exactly one place — `system.rs:723`,
`sink.on_step_boundary(self.cpu.regs.pc, self.scheduler.now() / MCLK_PER_FRAME)` — which is in the *run
loop*, above the bus. The bus adapters are constructed by split-borrow inside `step_cpu`
(`system.rs:805-820`) and are handed memory regions, not the CPU.

So a `pc` field would be a sentinel at all four sites, and an `mclk` field a sentinel at two. **The
sentinel would be a lie exactly where the struct is most heavily used**: the 22 `Vec<BusEvent>` recording
sites in `bus.rs`, `z80/bus.rs` and `system.rs` are unit tests that construct a bus *with no `System` at
all* (`bus.rs:1048`, `fn bus<'a>(&'a mut self, sink) -> MegaDriveBus<'a, Vec<BusEvent>>`). A field that
reads `pc: 0` in every one of those tests is worse than no field: it invites exactly the silent-wrong-answer
the recon doc's §4 warns about.

### 4.2 `pc` and `frame` are per-*step* constants, not per-*event* data

One instruction drives 2–8 bus accesses (a `MOVEM` drives far more). `pc`/`frame` are invariant across
all of them; the step-boundary stamp is the correct normalization, and `Watchpoints` already exploits it
(`watchpoints.rs:16-21`: *"An instruction that drives several accesses (a `MOVEM`, a read-modify-write)
attributes them all to its own PC, which is exactly right."*). Copying them into every event multiplies
the redundancy by the access count and grows a `Copy` struct from 16 bytes to ~40 on a path that
`AudioSink` stores wholesale (`synth/audio_sink.rs:59`, `pending: BTreeMap<u64, Vec<(u32, BusEvent)>>`).

### 4.3 The duplication is real, and the fix is a shared latch

The census found the same three lines written four times:

- `crates/oracle-core/examples/diag_soundqueue.rs:53-55` — `fn on_step_boundary(&mut self, _pc, frame) { self.frame = frame }`
- `crates/oracle-core/src/vgm.rs:277-279` — identical
- `crates/oracle-core/src/watchpoints.rs:260-263` — the pc+frame variant
- `crates/oracle-core/src/synth/audio_sink.rs:227-245` — an edge-detecting variant

plus `VgmLogger`'s `pending_mclk: Option<u64>` (`vgm.rs:86-90`, set at `:350-353`, taken at `:284-287`),
a field whose entire purpose is to smuggle an argument from `on_event_at` into `on_event` — with a
fallback that *synthesizes* an mclk from the latched frame when the emitter did not supply one.

**Ship a ~25-line `Attribution` helper** in `bus.rs` next to the trait:

```
pub struct Attribution { pc: u32, frame: u64, mclk: u64 }   // + a `pending mclk` staging slot
impl Attribution {
    pub fn step_boundary(&mut self, pc: u32, frame: u64);
    pub fn timestamp(&mut self, mclk: u64);      // called from on_event_at before delegating
    pub fn stamp(&self) -> (u32, u64, u64);      // pc, frame, mclk
}
```

Three one-line forwards make any hand-written sink attributed. It deletes `pending_mclk` and the four
copies of the frame latch.

**But be honest about its power: a helper that is not *the* API does not get adopted.** The recon doc's
own sharpest finding is `conformance_roms.rs:231`, where `fnv1a_rgb` was factored out *and the hazard
named in a comment* — *"so they cannot drift into different layouts"* — and then shared with nobody,
while three sites kept hand-rolling the identical loop
(`docs/2026-08-14-tooling-frontier-recon.md:165-170`). `Attribution` is for the irreducible hand-rolled
tail only. **The primary vehicle must be §5**, a thing you *use* rather than a thing you *embed*.

### 4.4 `master` — the recon's justifying episode does not hold up

Recon §5 says master attribution *"belongs in the event"*, on the grounds that *"[function-code master
attribution] is what let the sound-silence hunt eliminate the 68k."*
**Checked against the primary docs, that sentence is materially wrong, in two separate ways.**

**(i) The hunt did not classify on `fc`. It classified on `addr`, deliberately and explicitly.**

> The logger normalizes by **classifying on `addr` alone**, folding both windows into the same decoder
> state (RT1/RT2). `fc` is carried as *attribution only* … **never as a routing key**
> — `docs/2026-07-22-phase-rt-design.md:145-148`

> **One chip, two windows (RT3).** Classify on `addr` alone, **`fc`-agnostic**
> — `docs/2026-07-22-phase-rt-design.md:444`

The same `fc`-agnostic rule is restated in `docs/2026-07-23-phase-sy-synthesis-design.md:23-24` and is
implemented at `crates/oracle-core/src/vgm.rs:288`.

**(ii) The 68k was never the suspect, so nothing "eliminated" it.**

> its value is near-zero without the Z80 driving the chips (**no music/SFX flows through the 68k side**)
> — `docs/2026-07-22-sound-stack-recon.md:139-142`

The actual discriminator was the **raw address window** — `$4000…` = Z80, `$A04000…` = 68k — which is a
perfect master proxy *for this chip*. And `fc` could not have been the discriminator anyway, because on
the Z80 path it is a **fabricated literal**, not a real function code:

> the Z80 tap (RT-1) emits `BusEvent { op: Write, fc: 0, addr: <raw Z80 port>, .. }`
> — `docs/2026-07-22-phase-rt-design.md:37`, matching `crates/oracle-core/src/z80/bus.rs:226-234`

**So there is no episode demanding a `master` field.** There is, however, a real latent defect in the
address proxy, and it is the same hole. `fc = 0` is overloaded three ways — DMA/non-CPU masters, the
Z80's own writes (`z80/bus.rs:228`), and **68000 writes through the Z80 window** (`bus.rs:522`,
`self.emit(BusOp::Write, 0, 0x7F11, …)`) — and the third case *also* rewrites the address into the
Z80-side shape. This is deliberate and documented at `bus.rs:509-513`:

> `$7F11` -> the PSG port through the mirror: tap the Z80-side-shaped BusEvent into the sink (addr
> `$7F11`, fc 0 — the same event the Z80's own write emits, so the VGM logger/synth unify the two paths
> at the register-file level)

**Consequence: for `$7F11` specifically, *both* master signals are wrong simultaneously.** A 68000 PSG
write through the window reports `fc = 0` (reads as Z80) *and* address `$7F11` (reads as Z80). Every
other port keeps the address proxy intact. That is a genuine finding, sitting inside the exact
instrument the recon's recommendation cites — but it is a one-port hole, not a case for widening every
bus event.

**The fix belongs at the source, not in the struct.** Either emit the true `fc` at `bus.rs:522` (changes
the event stream, could move VGM/synth output → currency-gated), or leave the conflation and make the
recorder refuse to draw a master conclusion there.

→ **`F-TRACE-MASTER`** (open, owner ruling needed): (a) emit true `fc` at `bus.rs:522` behind a currency
check; (b) add a first-class `master: Master` field to `BusEvent` — now priced at 12 literal edits + 2
assertions, and note it would have to be justified *on its own merits*, not as "`fc` already does this";
or (c) document the conflation and have the recorder attach a caveat to any master-flavoured query over
`$7F11`/`$C00011`. **This design provisionally picks (c)** — free, reversible, correct, and the primary
evidence demands no more. **Do not pick (b) on the strength of recon §5's sentence: it has now been
checked and does not hold.**

---

## 5. The design — four additive changes to `watchpoints.rs`, plus one test

No new module. No new trait. No rename (`Watchpoints` is the name people already reach for; renaming it
to `Trace` would strand the muscle memory that is the whole asset here).

### T1 — `mclk` on `WatchHit`

Override `on_event_at` to latch the timestamp, then stamp it into every hit. This is the same move
`VgmLogger` makes, done once.

- `WatchHit` gains `pub mclk: u64` (`watchpoints.rs:111-135`).
- `Watchpoints` gains `cur_mclk: u64`, set by a new `on_event_at` override that stages then delegates.
- VDP-internal writes (`on_vdp_write`, `watchpoints.rs:270-295`) are delivered *after* the driving CPU
  step (`system.rs:728-732`), so their mclk is the step's, not the write's. **Stamp them with the
  step-boundary mclk and mark them approximate** — see §8's caveat rule. Do not fabricate precision.

**Blast radius of extending `WatchHit`:** it derives `PartialEq, Eq` and is compared whole in exactly
**2 places** (`watchpoints.rs:326`, `watchpoints.rs:471`), both in the same file's test module, plus 2
construction sites (`:245`, `:279`). `WatchHit` is in neither frozen currency (it is not in
`export_state`/`state_hash`). Four edits, one file. **Cheap by measurement, not by assumption.**

### T2 — watch ids, labels in hits, enumeration, removal

Discharges recon §5's two named pre-exposure gaps verbatim
(`docs/2026-08-14-tooling-frontier-recon.md:268-270`): the `label` on `WatchSpec` is
`#[allow(dead_code)]` and never reaches a hit (`watchpoints.rs:101-103`), and there is no id, removal, or
enumeration — *"an agent running three concurrent watches cannot tell which one fired."*

- `add_watch`/`add_vdp_watch` return a `WatchId(u32)`.
- `WatchHit` gains `pub watch: WatchId`. Store the id, not the `String` — a hit stays `Copy` and the
  reader resolves the label via `watches()`. (Putting the `String` in the hit would make `WatchHit`
  non-`Copy` and allocate per hit on the instrumented hot path.)
- Add `remove(id)`, `watches() -> &[WatchInfo]`. Keep `clear()`.
- **A hit matching several watches records once, attributed to the lowest matching id**, and that must be
  documented — the current `any(…)` (`watchpoints.rs:237-241`) already collapses overlaps silently.

### T3 — `WatchMode`: record / count / census

The aggregation primitives, per watch. Justified in §7.

```
enum WatchMode { Record, Count, Census(CensusKey) }
enum CensusKey { Addr, AddrPage(u8), Fc, Op, Size, Value, ValueHiEqLo }
```

- `Record` is today's behaviour (the default; existing call sites keep working with an
  `add_watch_with_mode` variant or a builder — **do not change the existing two-arg signature**, three
  callers depend on it: `examples/watch_probe.rs:121`, `tests/watchpoints.rs:39`, `frontend/main.rs`).
- `Count` stores nothing and increments one `u64`. This is what turns "trace everything in
  `$400000-$7FFFFF`" from a context bomb into a number.
- `Census(k)` keeps a `BTreeMap<u64, u64>` with the k4 rule — no new keys past the cap, keep counting
  known ones (`k4_openbus_probe.rs:152-156`) — plus `census_overflow: u64` and `keys_capped: bool` so
  the cap is never silent. **Default cap 256, not 16** — see §7's cap trap; the episodes that mattered
  ranged over 390–516 distinct keys and a 16-cap would have produced a confidently wrong cardinality.

**Deliberately an enum of key extractors, not a closure.** A `Box<dyn FnMut>` would make `Watchpoints`
non-`Debug`, non-`Clone`, and un-serializable, which forecloses the JSON-RPC exposure S3 is heading for.
The enum covers every key observed in the corpus. The cost is stated honestly in §9: it cannot express
`K4Probe`'s stateful classification.

### T4 — `seen`, and a `VecDeque` ring

- **`seen: u64`** — every event offered to the sink, matched or not, counted unconditionally. This is
  §8's structural negative control and is the single highest-value line in the whole design.
- Replace `self.hits.remove(0)` (`watchpoints.rs:207`) with a `VecDeque` or head index. At the watch
  granularity it exists for today, O(n)-per-drop is fine; a `Count`/`Census` watch over a wide range
  makes the ring the hot path and the current implementation quadratic. This is a prerequisite, not
  polish.

### T5 — traces must be diffable (a requirement, not a feature)

Two of the corpus's sharpest results are **comparisons between two traces**, not queries within one:

> **first 5,153 writes IDENTICAL** … First divergence, melody index 5,153
> — `docs/2026-07-23-timing-adjudication-oracle.md:160-162`
> Writes only-in-ours (multiset) | **0** — we emit no spurious or wrong value
> — `docs/2026-07-23-rt3-oracle-ab-findings.md:29`

Building a diff tool is **out of scope** (that is recon item 3, frame comparison). But the recorder must
not *foreclose* one. Two cheap properties:

1. `WatchHit` keeps `PartialEq + Eq` and `Copy`, so `Vec<WatchHit>` diffs with stock tooling.
2. **Two runs of the same ROM + input + seed must produce byte-identical hit sequences.** This falls out
   of C2 but should be a *test*, not an assumption — it is the property the whole instrument's
   credibility rests on, and it is one `assert_eq!` between two runs.

---

## 6. Filtering — record time, and the "trace everything" trap

Filtering is already record-time and stays there. `Watchpoints::on_event` matches specs and returns
before allocating (`watchpoints.rs:236-244`); nothing is stored for a non-match. The recon doc's warning
about the sibling's `audio_spectrum` — *"a context bomb (up to 16,384 floats at `indent=2`)"*
(`docs/2026-08-14-tooling-frontier-recon.md:205-206`) — is answered by construction here.

**Two additions, each with its episode:**

- **`fc` filter** on a spec (`Option<u8>`). `diag_soundqueue` censuses by fc but cannot *filter* by it;
  `system.rs:1885-1898` hand-writes `e.fc == 0` inline. Cheap, one field.
- **No value predicate.** Tempting — `K4Probe::unmapped_would_change` (`k4_openbus_probe.rs:171-180`)
  is exactly a value predicate, as is `z80win_open_word_dup_changes` — but both are *derived*
  classifications ("would the K4-1 rule change this?"), not simple comparisons, and encoding them
  needs an expression language. `CensusKey::ValueHiEqLo` covers the one recurring shape.
  **Open question, not a silent pick:** does anything need `value == X` / `value & mask` filtering?
  No episode found. → **`F-TRACE-VALUEPRED`**, deferred until one appears.

**Explicitly discouraged in the docs:** `add_watch(0..=0xFFFF_FFFF, Any, …)` in `Record` mode. It is
legal, bounded by the ring, and almost always the wrong instrument. Say so in the doc comment, and make
`Count` the obvious first move.

---

## 7. Aggregation — the minimum set, each justified from an episode

The recon doc ranks this #9 with the note that it *"disproved two recorded root causes"*
(`docs/2026-08-14-tooling-frontier-recon.md:161`). **That claim checks out, and the two retractions are
locatable** — both are `docs/2026-07-25-testrom-conformance.md`, both killed the recorded reason
*"border-only rendering"*, and both were killed by a **CRAM-write census**:

> The ROM writes CRAM **16 times per active line, on every active line** … **all CPU writes, zero DMA**,
> cycling `$000E` / `$00E0` / `$0E00` round-robin into exactly **two entries: index 4 and index 36**.
> That last fact is the adjudication. The earlier reason on this row — "border-only rendering" — does not
> survive it: a border-colour demo would hammer **index 0** … and this ROM never touches index 0.
> — `docs/2026-07-25-testrom-conformance.md:783-789`, retracted at `:796`

> The ROM's CRAM traffic is **99.997% DMA** (4,923,072 of 4,923,206 writes over 120 frames) and lands
> **entirely on CRAM index 0** — the backdrop — with the address never advancing.
> — `docs/2026-07-25-testrom-conformance.md:806-811`

**Note what the winning query actually was in both cases: the *set of distinct destination indices*,
not a count.** `{4, 36}` and `{0}`. A counter would have said "lots of CRAM writes" and settled nothing.
That has a direct design consequence — see the cap discussion below.

There is also a decisive volume argument the corpus states outright. Raw traces were never viable at
this machine's event rate:

> per-access `Vec` logging is System-side instrumentation, not CPU cost, and **would swamp the signal**
> — `docs/plans/2026-07-16-m68000-macro-rtc.md:48`

> ≈8,700 steps/frame steadily; 10.45M steps at 1200f … 185k reads over 120f → 2.0M over 3000f
> — `docs/2026-07-22-tf4-nextlayer-triage.md:35,41`

Every hunt's repro block in the corpus collapses its trace to counters. **Aggregation is not a
convenience layer on top of the trace; at this event rate it is the only usable read mode.**

### The set

| Primitive | Justifying episode | Verdict |
|---|---|---|
| **Count** (per named bucket) | `K4Probe` declares **16** `u64` counters by hand (`k4_openbus_probe.rs:53-68`). TF4's read-tally by region *"directly **eliminates the Z80-mailbox branch**"* on `z80_reads=0` (`docs/2026-07-22-tf4-triage.md:20-24`) | **Ship.** The modal aggregation, and the one that makes a wide watch survivable. |
| **Census / group-by, bounded** | The two CRAM retractions above; `diag_soundqueue`'s three `BTreeMap<u8,u64>` fc tallies (`diag_soundqueue.rs:40-42`); `K4Probe.ww_detail` (`k4_openbus_probe.rs:72`); *"**298,333** reads (**all** `$7F05`…)"* (`docs/2026-07-25-testrom-conformance.md:119`) | **Ship.** Four independent authors, and it is what disproved both root causes. |
| **Distinct-key cardinality** | *"**distinct PCs** 390→403, first-new-frame=301 … distinct PCs 390→516"* (`docs/2026-07-22-tf4-nextlayer-triage.md:138-139`); *"**4 distinct colours** in the whole frame … ~1400 distinct colours"* (`docs/2026-07-25-testrom-conformance.md:761,765`) | **Ship — as a distinct number, not as a side effect of the census.** See the cap trap below. |
| **First / last occurrence, with stamp** | *"first-seen frames"* in the TF4 PC instrument (`docs/2026-07-22-tf4-nextlayer-triage.md:19-21`); the *"9 hand-tuned magic frame budgets"* exist because nobody could ask when something first happened | **Ship.** One `Option<Stamp>` pair per watch; `Count`/`Census` modes otherwise discard it. |
| **Composite-key histogram** | T16: `(probe1, probe2, gap) → count`, *"**1815 out of 1815** active-display retries produce the identical miss"* (`docs/2026-08-03-t16-slot-scheduling-recon.md:252-262`) — the "is this deterministic or stochastic?" query | **Defer** (`F-TRACE-TUPLEKEY`). Real and sharp, but that episode's keys are VDP-internal probe results, not bus-event fields. A 2-key `Census((k1,k2))` is a cheap later extension. |
| **Min / max of a numeric projection** | none found | **Do not ship** (`F-TRACE-MINMAX`). |
| **Percentiles / time-series** | none found | **Do not ship.** The "what a tracing library usually has" category. |

**Shipped set: count, bounded census, distinct-cardinality, first/last.**

### The cap trap — a correction to T3

A 16-key cap (the `k4_openbus_probe.rs:152-156` rule) is right for *that* probe and **wrong as a
default**. The census episodes that mattered ranged over 390–516 distinct PCs and ~1400 distinct
colours. A census silently capped at 16 would have reported "16 distinct PCs" and been confidently
wrong — the exact failure class this design exists to prevent.

→ **Default key cap 256, caller-configurable, and the report MUST carry `keys_capped: bool`** so
`distinct = 256` is never readable as an exact answer when it means "≥ 256". When capped, the
cardinality is reported as a lower bound and a caveat is attached. **Refuse loud, never clamp silently**
(recon §4).

### Where counting is the *wrong* instrument — design the refusal in

The corpus contains one place where it says so explicitly, and the recorder should be able to say it too:

> `st` is huge everywhere (vblank-poll idioms; TF4 677k) — *counting* cannot prove flag-consumption
> either way (the STOP (b) third clause is structurally unobservable from the bus stream…)
> — `docs/2026-08-02-k4-0-hit-table.md:82-84`

A large count over a polling idiom proves nothing about consumption. This is not automatable, but it is
exactly what the `caveats` field (§11) is for, and the doc comment on `Count` should say it.

### And a warning the arming evidence makes concrete

A census with a wrong arm point produces a confidently wrong verdict, not a null one — see §8/C1, where
per-second `$27=$15` tick counting returned "no drop" purely because the capture was armed after boot
(`docs/2026-07-23-timing-adjudication-oracle.md:3-11`). **Aggregation multiplies the cost of a C1
failure**, because a number looks like an answer in a way a truncated event log does not. `seen` (§8/C3)
is the counterweight.

**Correction to the recon doc:** it states `examples/k4_openbus_probe.rs` has *"26 hand-declared
counters"* (`docs/2026-08-14-tooling-frontier-recon.md:153`, and the brief for this pass repeats it). The
file has **16** `u64` counters (`k4_openbus_probe.rs:53-68`) plus 2 shadow booleans and 1 `BTreeMap`. The
26 appears to come from counting the 17-column markdown table in the file header (`:23-35`). The
recurrence conclusion is unchanged; the number should be fixed.

---

## 8. C1–C4, applied

### C1 — atomic arm-at-power-on: **currently structurally impossible, and this is a real defect**

`System::reset()` ends with:

```
// crates/oracle-core/src/system.rs:359-361
self.cpu.assert_reset();
self.step_cpu(&mut ()); // services reset_pending: runs the power-on reset recipe over the bus
```

**The null sink is hardcoded.** The reset recipe's bus traffic — the initial SSP and PC vector fetches,
the first accesses in the machine's life — is invisible to every possible caller. The census confirms the
consequence: **all eight bespoke sinks in the tree attach after `reset()` has already run**
(`diag_soundqueue.rs:103-108`, `k4_openbus_probe.rs:226-255`, `watch_probe.rs`, `conformance_roms.rs`,
`scanline_capture.rs:43-48`, the frontend loop). Not one has ever observed a reset vector fetch.

This is precisely the C1 failure the recon doc reconstructs. The primary source is
`docs/2026-07-23-timing-adjudication-oracle.md`, which carries a retraction banner over its own body at
`:3-11`:

> **⚠️ CORRECTION (overseer, 2026-07-23): the "Verdict: B — artifact / no real drop" in this doc's BODY
> is WRONG and superseded.** It rested on a bad control: Oracle's VGM was armed AFTER boot … so it
> sampled only the 60 Hz steady state and skipped the startup window where the drop is seeded.

and, at `:141-150`, both the mechanism and the verification check C1 says the API must *expose*:

> a first attempt armed this way **diverged at melody index 0, a garbage comparison**. The correct arm
> point is the **pristine power-on state** reached by `reset` → immediate `pause`, confirmed by
> `PC=0xFFFFFFFF, SP=0xFFFFFFFF, SR=0xFFFF` (reset vector not yet fetched, before any sound write).
>
> **This is almost certainly why Agent 2 saw "no drop": arming after boot skips the very ~1 s startup
> window where the drop is seeded, so per-second counting only ever samples the 60 Hz steady state.**

Note the interaction with §7: the instrument that produced the wrong verdict was a **count**. A
mis-armed aggregate does not return "nothing"; it returns a plausible number.

**Fix — unbundle it and ship it separately from the recorder; it is ~5 lines:**

```
pub fn reset_with_sink<S: BusEventSink>(&mut self, sink: &mut S);
pub fn reset(&mut self) { self.reset_with_sink(&mut ()); }   // unchanged behaviour
```

and one indivisible constructor so "reset, then arm" is not expressible:

```
pub fn boot_with_sink<S: BusEventSink>(seed: u64, rom: Vec<u8>, sink: &mut S) -> System;
```

Note `load_rom` must precede `reset` (`system.rs:337`, `:371`), which is exactly why the three-step dance
is error-prone and why the atomic constructor is the right shape.

**Open question — the verifiable-power-on-state half of C1.** Recon §5 says the correct arm point was
confirmed by observing pristine values `PC=0xFFFFFFFF, SP=0xFFFFFFFF, SR=0xFFFF`, and that *"the API must
**expose** that check, not assume it"* (`docs/2026-08-14-tooling-frontier-recon.md:239-241`). Those values
are from the sibling Oracle. **What our `Cpu68000::assert_reset` (`m68000/microop.rs:3146-3148`) leaves
observable has not been checked in this pass** and must not be assumed. → **`F-TRACE-POWERON-CHECK`**:
determine our pre-reset register anchor and expose a `System::is_pristine_power_on()` predicate, or
record that we have no equivalent.

### C2 — deterministic emulated frame identity: satisfied, once T1 lands

Every stamp is emulated. `frame` comes from `self.scheduler.now() / MCLK_PER_FRAME` (`system.rs:723`);
`mclk` from `Scheduler::now()` (`scheduler.rs:42`). No wall clock is reachable from a sink. S1's
`StopRecord` already commits to the same discipline in its own doc comment. **Nothing to build; T1 just
has to carry `mclk` so the stamp is complete rather than frame-granular.**

### C3 — cheap negative control: made structural by `seen`

The recon doc's precedent is that a null result was only trustworthy because a control proved the
detector fired at all — *"Absent a control, a silent zero is indistinguishable from a pass"*
(`docs/2026-08-14-tooling-frontier-recon.md:248-252`).

The usual answer is a `--control` flag. **This design prefers a counter.** `seen` (T4) is incremented for
every event offered to the sink regardless of match, so a report of `seen: 4_182_339, matched: 0` is
*self-evidently* a live instrument that found nothing, while `seen: 0, matched: 0` is *self-evidently* a
dead one. One `u64`, no flag to remember, no separate run, and it cannot be forgotten because it is not
optional.

A flag-based control is still available and free: invert or widen the spec. The design does not need a
mode for it.

### C4 — no residual instrument state: satisfied, with one thing for S1 to check

`Watchpoints` is caller-owned; `System` never stores a sink (`system.rs:683-686`). This is the property
the recon doc contrasts against the 1,691,410-hit stale breakpoint.

**One hazard to verify in S1's review, not here:** `run_until_with_sink` arms `vdp.set_write_capture(true)`
at entry and disarms at exit (`system.rs:712-713`, `:743-746`). S1 adds an early `break` on
`stop_requested`. Confirm the disarm still executes on the early-stop path — a leaked `write_capture` is
exactly a residual instrument state, in the machine, across runs. → **`F-TRACE-S1-DISARM`** (a review
item for S1, raised here because this design depends on it).

---

## 9. The standard to beat — what each ad-hoc sink becomes

The design succeeds only if it would have made the ad-hoc sinks unnecessary. **Scoreboard: of the six, it
fully replaces one, partially replaces one, and correctly declines three (one of which S1 handles).**
That is a weaker result than the recon doc implies, and it is the honest one.

### `examples/diag_soundqueue.rs` — **fully replaced**

Today: 81 lines of sink (`:19-81`) — a `Rec` struct that re-declares `BusEvent` field-for-field plus a
hand-copied `frame`, a frame latch, two `is_fm`/`is_psg` free functions, three unbounded `Vec<Rec>`, and
three `BTreeMap<u8, u64>` fc tallies. Bounding happens only at print time
(`.min(32)` at `:129`, `:147`; `.take(64)` at `:188`).

Under this design: **six `add_watch` calls and no sink at all.**

```
let fm  = wp.add_watch_multi([0x4000..=0x4003, 0xA0_4000..=0xA0_4003], Write, "fm",  Record);
let fmc = wp.add_watch_multi([0x4000..=0x4003, 0xA0_4000..=0xA0_4003], Write, "fm.fc",  Census(Fc));
… same for psg ($7F11 / $C00011) and the Z80-RAM window …
```

`Rec` → `WatchHit` (which already carries `frame`, and after T1 `mclk` too, which `Diag` never had).
Bounding moves to record time via the ring. The fc census becomes `Census(Fc)`. **And the report would
carry the §4.4 caveat about `$7F11` fc conflation, which the hand-rolled version silently gets wrong.**

*(One gap: the design needs an "or these ranges" spec, since `$4000-$4003` and `$A04000-$A04003` are one
logical chip. Either `add_watch_multi`, or two watches sharing a label. Prefer two watches — simpler, and
T2's ids already distinguish them.)*

### `examples/k4_openbus_probe.rs` — **partially replaced; the remainder is a genuine finding**

**Replaced:** roughly 11 of the 16 counters become `Count` watches over an address range + op + size —
`unmapped_reads`, `a11200_reads`, `a11100_reads`, `io_even_byte_reads`, `io_word_reads`,
`status_upper_reads`, `status_odd_byte_reads`, `z80win_bank_reads`, `z80win_bank_writes`,
`z80win_vdp_mirror_writes`, `z80win_open_word_writes`. And `ww_detail` becomes exactly one
`Census(Addr)` + `Census(ValueHiEqLo)` pair, cap 16, with the overflow now counted instead of silently
dropped.

**Not replaced — and this is the finding.** Five counters are *stateful* or *derived*:

- `z80win_closed_reads`, `z80win_closed_writes`, `a11100_while_reset` gate on the probe's own shadow of
  the `$A11100`/`$A11200` arbiter latches, reconstructed from the write stream
  (`k4_openbus_probe.rs:49-52`, `:131-140`) — *"Reconstruct the arbiter latches exactly as the bus latches
  them."*
- `unmapped_would_change` (`:171-180`) and `z80win_open_word_dup_changes` (`:194-195`) are derived value
  predicates encoding the K4-1 open-bus rule.

A `CensusKey` enum cannot express either. A closure could — and is rejected in T3 for concrete reasons.
**So `K4Probe` would still be hand-rolled**, though as a much smaller `Fanout<Watchpoints, K4Shadow>`
where the shadow is only the 5 stateful counters and the other 11 are configuration.

The same pathology appears twice more: `VgmLogger.fm_addr_latch`/`psg_latch` (`vgm.rs:291`, `:302`,
`:322-324`) and `AudioSink.fm_addr_latch` (`synth/audio_sink.rs:43-45`, whose own doc comment says it is
decoded *"exactly as the `VgmLogger` does"*) reconstruct the *same* FM address latch from the write
stream. **Three separate sinks shadow hardware state the machine already has.** The right fix is not a
richer trace — it is exposing the latches (`z80_busreq`, `z80_running`, the FM address latch) as
read-only accessors on `System`/`Ym2612`. → **`F-TRACE-EXPOSE-LATCHES`**, separate item, cheap, and it
would delete more hand-rolled code than the census primitives will.

### `FrameCapture` (`tests/conformance_roms.rs:254-276`) vs `LineCollector` (`tests/scanline_capture.rs:12-39`) — **NOT replaced. Wrong seam.**

The near-duplicate claim is **confirmed and then some**: same seam, same day (`cb2162e` 2026-08-03 00:59
introduced `LineCollector`; `fe61692` 2026-08-03 10:12 introduced `FrameCapture`, 9h13m later, same
author). Three of four method bodies are byte-identical between them:

- `fn on_event(&mut self, _event: BusEvent) {}` — identical, in both
- `fn wants_scanlines(&self) -> bool { true }` — identical, unconditional, in both
- `fn on_scanline(&mut self, line, rgb)` copying out of the borrowed slice — same signature, same reason

They differ only in retention policy: `LineCollector` keeps first-wins
(`if self.first_line_rgb.is_none()`); `FrameCapture` keeps the last complete frame and to do so
hand-detects frame boundaries with two magic line comparisons — `if line == 0 { clear }` and
`if line == ACTIVE_LINES - 1 { last = take(building) }` (`conformance_roms.rs:267-274`). Both hard-code
224.

**Neither consumes a single bus event.** This design would change nothing about either. **Two of the six
"ad-hoc sinks" cited as evidence for the trace recorder are evidence for a different, smaller item:**

→ **`F-SCANLINE-CAPTURE`**: promote a `ScanlineCapture { retain: First | LastFrame | All }` into
`oracle-core`, and add the missing `on_frame_boundary` hook to `BusEventSink` (the census found **no
`on_frame_boundary` anywhere** — every sink that needs frame structure infers it, which is *"the single
largest source of duplicated bookkeeping"*). ~60 lines, collapses both types to configuration, and is
independently useful. **Do this before or instead of half of the recorder work if forced to choose** —
it is cheaper and its duplication evidence is stronger (byte-identical bodies, 9 hours apart).

### `AudioAndWatch` (`crates/oracle-frontend/src/audio.rs:158-217`) — **replaced by S1, not by this**

Seven hand-written forwards with an inconsistency the census caught: `wants_vdp_writes` drops
`self.audio`'s answer (`:193-196`) while `wants_scanlines` ORs both (`:205-209`), with a comment
explaining why the OR is correct. S1's `Fanout` + `Option<S>` fixes this. Not this design's work; noted
so it is not double-counted as evidence.

### `VgmLogger` (`crates/oracle-core/src/vgm.rs`) — **correctly not replaced**

It is a chip-protocol decoder (FM address-latch/data pairs, PSG latch → `VgmRecord`), classifying
fc-agnostically on address alone by deliberate design (`vgm.rs:288`). No generic trace subsumes that, and
it should not try. It *should* adopt §4.3's `Attribution` to delete `pending_mclk`.

---

## 10. Dependencies on the two in-flight slices

Both are read from their worktrees as of this pass; APIs below are what they currently implement, not
speculation.

**S1 (stop signal + `run_until` + `Fanout`)** — this design depends on it in three places:

1. **`Fanout<A, B>` and `impl BusEventSink for Option<S>` / `&mut S`** are what let a trace ride alongside
   the audio sink or a stop condition. Without them, adding a trace to the frontend means a third
   hand-written composite.
2. **`stop_requested(&self) -> bool`** is the payoff composition: a `Watchpoints` that returns `true`
   once a watch has fired *is* "record until X happens", which is the shape the 9 magic frame budgets
   were approximating. **This design should add an opt-in `stop_after(id, n)` per watch** — small, and it
   makes the recorder a first-class predicate source rather than only a passive consumer.
3. **`StopRecord { reason, pc, frame, mclk }`** is the same stamp shape as an extended `WatchHit`. Keep
   the field names and semantics identical so a stop and a hit name the same coordinate.
4. The `F-TRACE-S1-DISARM` review item in §8/C4.

**S2 (`.lst` symbol parser)** — a **read-time** dependency only, and the layering must be kept strict:

- `WatchHit` stores a raw `pc: u32`. It must **not** hold a symbol name. The core has no I/O (the caller
  reads the `.lst`), symbols bind per-game and per-shape, and a mismatched table is *"a silent wrong
  answer that must be refused"* (`docs/2026-08-14-tooling-frontier-recon.md:63-64`).
- Annotation happens in the *reporter*: `pc` → `EntryPoint.wait_dma+6`. That is where the shape-refusal
  check lives too.
- A trace report with no symbol table loaded must still be fully useful (raw hex). **Do not make symbols
  a soft requirement.**

---

## 11. Output shape

Consumers are a human at a terminal and a future agent over JSON-RPC with a hard context budget. One
report type serves both; only the rendering differs. The core produces the struct; **no JSON in
`oracle-core`** (charter: no I/O) — S3's transport serializes it.

```
TraceReport {
    // recon §4's first non-negotiable: every reply carries these
    frame: u64, mclk: u64, running: bool,
    // C3, structural (§8)
    seen: u64,            // every event offered to the sink, matched or not
    // per watch
    watches: [ WatchReport { id, label, space, range, op, mode,
                             matched: u64, first: Option<Stamp>, last: Option<Stamp>,
                             census: Option<[(key, count)]>,
                             distinct_keys: u64, keys_capped: bool, census_overflow: u64 } ],
    // recon §4's second non-negotiable: bounded, cursored, truncation flagged
    hits: [WatchHit],  next_cursor: Option<u64>,  truncated: bool,  dropped: u64,
    // recon §4's third non-negotiable
    caveats: [String],
}
```

- **Cursor = `seq`.** `Watchpoints` already assigns a monotonic `seq` to every matched access, stable
  across ring drops, so *"a gap in `seq` marks dropped hits"* (`watchpoints.rs:108-110`). It is already
  the right cursor; expose it as one.
- **`truncated` is distinct from `dropped`.** `dropped` = lost at record time (ring overflow);
  `truncated` = this *page* stopped short. Conflating them is exactly the sibling's ambiguous-success
  defect that S1's `StopReason` refuses to repeat.
- **`caveats` is a payload field, not documentation.** Recon §4 calls carrying caveats inside the payload
  *"the sibling's single best idea — agents over-trust precise-looking numbers."* Populate it
  unconditionally for: VDP-write hits stamped with the step's mclk rather than the write's (§5/T1); any
  `Census(Fc)` over `$7F11`/`$C00011` (§4.4); any dot/pixel coordinate (§12); a census that hit its key
  cap.
- **Human rendering is a `Display` impl** producing the k4-probe-style table. Formatted output is
  usually *shorter* than raw hex and vastly more usable (recon §4).

---

## 12. mclk → (frame, line, dot): where it belongs, and what is exact

**This is a real gap, done by hand at least twice.** An analyst reduced raw mclks to a frame and line
manually in the middle of an adjudication:

> Measured with a throwaway `on_event_at` instrument: memtest's three row-11 reads land at mclk
> 9,949,478 / 9,949,646 / 9,949,814 = **frame 11, line 27** — mid active scan, nowhere near the vblank
> window — `docs/2026-07-25-testrom-conformance.md:262-265`

and the arithmetic itself was written out longhand in a plan:

> so `line(mclk) = (mclk % 896_040) / 3420` and the in-line position is `mclk % 3420`
> — `docs/plans/2026-07-16-vdp-timing-skeleton.md:71-72`

**`frame` and `line` are exact within the model, and are pure functions of mclk.** Both already exist as
open-coded divisions in several places:

- `frame = mclk / MCLK_PER_FRAME` — `system.rs:723` (the run loop's own definition),
  `synth/audio_sink.rs:100-105` (independently re-derived), `vgm.rs:284-287` (the inverse).
- `line = (mclk % MCLK_PER_FRAME) / MCLK_PER_LINE` — `system.rs:760` (`deliver_event`'s own definition),
  `vdp.rs:348` (inside `v_counter`).

→ **Extract one free function**, `pub fn frame_line(mclk: u64) -> (u64, u16)`, next to the constants in
`system.rs`. This is not a new model; it is the deduplication of two divisions written four times plus
one written into a doc. Exact by definition — it *is* the definition the scheduler uses.

**`dot` is different and must not be stored in a hit.** Two reasons, both from the code:

1. **It is not a pure function of mclk.** It needs `h40()` (`vdp.rs:316-318`), a live VDP register read,
   because the line sweeps 342 positions in H32 and 422 in H40 (`vdp.rs:320-328`). A sink has no VDP.
2. **It is approximate.** `h_counter` documents *"`mclk % 3420` maps linearly across the positions (the
   sub-position phase within the line is pure timing)"* (`vdp.rs:321-323`), and `v_counter` documents a
   known error: *"On hardware the V counter increments mid-line … we increment at the line boundary — a
   sub-line phase difference that is pure timing (documented open item, recon R2)"* (`vdp.rs:345-347`).

→ **Decision:** a `WatchHit` stores `mclk` only. `(frame, line)` are derived exactly, on demand, by
`frame_line`. `dot` is resolved **at read time** by the reporter, which has the `System`, via the existing
`vdp.h_counter(mclk)` — and **any reply carrying a dot MUST carry a caveat** naming the linear-mapping
approximation and the sub-line V phase. That is recon §5 C3 and the §4 `caveat` rule applied literally.

**The h-position gap is recorded as a live blocker, and it is not a mclk-conversion problem.** The
conformance doc names it flatly — *"**CRAM writes carry no h-position.**"*
(`docs/2026-07-25-testrom-conformance.md:815`) — and the analyst worked around it by deriving a rate by
hand: *"**44,352 words per frame** = 198 × 224, i.e. ~198 colours per line"* (`:807-808`).

That is worth being precise about, because it is tempting to read it as an argument for putting
`(frame, line, dot)` in the event. It is not. A CRAM write arrives via `on_vdp_write`, which
`run_until_with_sink` drains **after** the driving CPU step (`system.rs:728-732`), so the whole batch of
a step's VDP writes shares one timestamp no matter what field carries it. **The missing datum is a
per-write mclk on `VdpWrite`, not a wider `BusEvent`.**

→ **`F-TRACE-VDPWRITE-MCLK`**: give `VdpWrite` its own mclk at the point the VDP performs the write, so
sub-scanline CRAM/VRAM effects become locatable. This is the single change that would have answered the
`cram_flicker` "sub-scanline" question directly instead of by arithmetic. Bounded, but it touches
`vdp.rs`'s capture path, so it is its own slice — **not** folded into the recorder. Until it lands, T1
stamps VDP hits with the step's mclk **and says so in `caveats`**.

**Open question — PAL.** `MCLK_PER_FRAME = 896_040` is NTSC V28 only (`system.rs:24`;
`vdp.rs:17-21` derives it as `3420 × 262`), and the project's known-gaps list already records
region/PAL as hardcoded. **Every frame and line stamp this design produces is silently NTSC.** →
**`F-TRACE-PAL`**: either carry the timing basis in the report, or refuse to stamp lines when a PAL model
lands. Recording it now is free; retrofitting it after agents have cached "frame 601" is not.

---

## 13. What this design deliberately does NOT build

Each with the reason, so a later reader can reopen it on evidence rather than taste.

- **No new sink trait** (Option E). A second surface beside the one people use is the render-decode-API
  failure mode.
- **No richer event type at the seam** (Option B). The emission sites cannot fill it (§4.1).
- **No fields on `BusEvent`** (Option A) — despite the blast radius being cheap. §4.1/4.2.
- **No closures in watch specs.** Kills `Debug`/`Clone`/serializability and forecloses S3.
- **No histograms, percentiles, or time-series.** No episode.
- **No min/max.** No episode (`F-TRACE-MINMAX`).
- **No value-predicate filtering.** No clean episode (`F-TRACE-VALUEPRED`).
- **No query DSL.** The corpus asks six questions, all expressible as `(space, range, op, fc, mode)`.
- **No JSON, no transport, no MCP wiring.** That is S3, and it must not be pre-empted here.
- **No rename of `Watchpoints`.** The name is the asset.
- **No trace-on-by-default anywhere.** The null path stays the null path.

---

## 14. Named follow-ups

| Tag | What | Where | Owner call needed? |
|---|---|---|---|
| `F-TRACE-MASTER` | `fc = 0` conflates DMA / Z80 / 68k-through-Z80-window; an fc census on `$7F11` mis-attributes | `bus.rs:522`, `z80/bus.rs:228` | **Yes** — option (a) is currency-gated |
| `F-TRACE-POWERON-CHECK` | What our `assert_reset` leaves observable; expose `is_pristine_power_on()` | `m68000/microop.rs:3146`, `system.rs:359` | no — investigate first |
| `F-TRACE-S1-DISARM` | Verify `vdp.set_write_capture(false)` still runs on S1's early-stop path | `system.rs:712-746` | no — S1 review item |
| `F-SCANLINE-CAPTURE` | Promote `ScanlineCapture` + add `on_frame_boundary`; collapses `FrameCapture`/`LineCollector` | `tests/conformance_roms.rs:254`, `tests/scanline_capture.rs:12` | no — cheap, strong evidence |
| `F-TRACE-EXPOSE-LATCHES` | Expose `z80_busreq`/`z80_running`/FM address latch read-only; deletes 3 shadow reimplementations | `k4_openbus_probe.rs:49`, `vgm.rs:291`, `synth/audio_sink.rs:43` | no |
| `F-TRACE-PAL` | Every frame/line stamp is silently NTSC | `system.rs:24`, `vdp.rs:17-21` | **Yes** — carry basis now, or accept the retrofit cost |
| `F-TRACE-VDPWRITE-MCLK` | `VdpWrite` has no per-write mclk, so sub-scanline CRAM effects are unlocatable — the recorded blocker *"CRAM writes carry no h-position"* | `vdp.rs` capture path, `system.rs:728-732`, `docs/2026-07-25-testrom-conformance.md:815` | no — own slice |
| `F-TRACE-TUPLEKEY` | 2-key composite census (`(k1,k2) → count`), the T16 "deterministic or stochastic?" shape | `docs/2026-08-03-t16-slot-scheduling-recon.md:252-262` | no |
| `F-TRACE-MINMAX` | Add min/max aggregation when an episode demands it | — | no |
| `F-TRACE-VALUEPRED` | Add value-predicate filtering when an episode demands it | — | no |
| `F-RECON-K4-COUNT` | Recon doc says "26 hand-declared counters"; the file has 16 | `docs/2026-08-14-tooling-frontier-recon.md:153` | no — doc fix |
| `F-RECON-MASTER-CLAIM` | Recon §5's *"function-code master attribution … let the sound-silence hunt eliminate the 68k"* is refuted by the primary docs (§4.4) | `docs/2026-08-14-tooling-frontier-recon.md:266-268` | no — doc fix |
| `F-SY4-ESTIMATE` | SY-4 doc's "~40 sites" / "tests compare recorded events for equality" is 12 sites / 2 assertions | `docs/2026-07-23-phase-sy4-subframe-timing-design.md:97-100` | no — doc fix |

---

## 15. Open questions for the owner

1. **`F-TRACE-MASTER`.** Provisionally picked option (c) — document the `fc = 0` conflation and caveat any
   fc census on the PSG. Options (a) emit true `fc` (currency-gated) and (b) add a `master` field (now
   priced at 12 literal edits + 2 assertions) are both live. **This is the one place recon §5 makes a
   recommendation this design declines, so it wants a ruling rather than a silent override.**

2. **`F-TRACE-PAL`.** Carry the timing basis in every report now (free), or accept that stamps are NTSC
   and retrofit later (not free once agents cache coordinates)?

3. **Sequencing against `F-SCANLINE-CAPTURE`.** The scanline-capture item has *stronger* duplication
   evidence than half of the recorder work (two byte-identical method bodies written 9 hours apart) and
   is cheaper. Should it go first?

4. **Does the aggregation half (T3) earn its cost now?** *This pass started sceptical and changed its
   mind on evidence.* The initial read was that T3 was the speculative half. The primary docs say
   otherwise: aggregation disproved two recorded root causes (§7), and the corpus states outright that
   raw per-access logging *"would swamp the signal"* at ~8,700 steps/frame — every hunt's repro block
   collapses to counters because the raw trace was never viable. **Recommendation: ship T3.** The
   remaining honest caveat is §9's — it only partially retires `K4Probe`, because five of that probe's
   counters are stateful. That argues for `F-TRACE-EXPOSE-LATCHES`, not against T3.

5. **Should `F-TRACE-VDPWRITE-MCLK` be pulled forward?** It is the one change that would have answered
   the `cram_flicker` sub-scanline question directly rather than by hand arithmetic, and until it lands
   every VDP-space hit in this design carries a "timestamp is the step's, not the write's" caveat.
   It is a separate slice touching `vdp.rs`, so it is the owner's sequencing call.

---

## 16. Method note

Four claims in the inputs to this pass were checked and came back different:

1. the k4 counter count (26 → **16**);
2. the `BusEvent` field blast radius (~40 sites → **12 literals + 2 assertions**);
3. the composition of the "6 ad-hoc sinks" (6 bus traces → **2**, plus 2 scanline collectors, 1
   combinator, 1 chip decoder);
4. recon §5's master-attribution justification, which the primary docs **refute** — the hunt classified
   on `addr` and is explicitly `fc`-agnostic, and the 68k was never the suspect (§4.4).

One claim was checked and came back **stronger** than stated: aggregation really did disprove two
recorded root causes, both are locatable, and the winning query in both was a *distinct-value set*
rather than a count — which changed a design parameter (the census cap, 16 → 256) that would otherwise
have shipped wrong.

None of these changes the *direction* of the recon doc's ranking. Together they change the *size* and
the *shape* of what should be built, and one of them (4) reverses a specific recommendation. The recon
doc's own method note asks for exactly this
(`docs/2026-08-14-tooling-frontier-recon.md:347-353`: *require file:line or a primary quote for every
claim, and report negative evidence*). Worth adding: the standing constraint that gated this design
(constraint 2 in the overnight plan — "`BusEvent` gains no fields") had never been re-measured since
2026-07-23. It turned out to be directionally right and quantitatively wrong by 3×, which is exactly the
condition under which a constraint stops being reasoning and starts being folklore.
