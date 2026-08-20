# Aeon's fourth demand-side statement: the streaming-choke instrument asks (2026-08-19)

**Source.** `aeon` master **`3469c920`** ("Merge diag/streaming-choke — the choke decomposed, explained,
and its fix pair measured"), packet `docs/benchmarks/streaming/CHOKE-DIAGNOSIS.md`, 643 lines, delivered
by `d933162e` on branch `diag/streaming-choke`. Every anchor below is `CHOKE-DIAGNOSIS.md:<line>` at that
revision. The packet's own framing of §9 is `:543` — *"This section goes verbatim to the oracle-next
session."*

**Why this is a demand and not a wish list.** Same test the profiler demand met
(`docs/2026-08-19-aeon-profiler-demand.md:15`): a consumer measured something, could not measure it, and
named the instrument gap that stopped them — with the workaround they were forced into and what it cost.
Here the cost is concrete and stated per ask: two bespoke camera states built purely to route around an
attribution defect (C1, C3), and every rate in a root-cause packet sourced from an engine-side counter
that happened to exist by luck (C2).

**Their sort, and it is the sort we asked for.** §9 arrives pre-sorted against our in-flight v1 — `:545-547`:

> Sorted against oracle-next's in-flight profiler v1 (per-routine `cyclesSelf` + `stallCycles`
> with a completeness identity, and opt-in `perFrame[]`), per the coordinator's 2026-08-19
> note. Only category (c) generates work.

That is the demand-doc shape we asked them for, and it held: (a) four confirmations, (b) three items they
booked as *method* rather than as gaps, (c) three asks. Only (c) generates work, and the design for it is
`docs/2026-08-19-streaming-asks-recon.md`.

---

## 1. The 20.6% finding — and why it belongs in *our* acceptance artifact

The headline of §7 is not an ask. It is **corroboration of the identity we designed**, produced
independently, by a consumer who was not looking for it, on the instrument we are replacing. It belongs
in the CR-26 acceptance record for that reason, and this section exists to put it there.

**How they found it** (`:409-411`):

> This was not looked for. It surfaced because `GameState_OJZScroll_Update`'s row (55,145
> cyc/frame at `maxdiag`) is **smaller than the sum of its own children** — `Tile_Cache_Fill`
> 51,357 + `Parallax_Update` 12,188 = 63,545 — which is impossible for an inclusive row.

**The measurement** (`:415-420`) — top-level rows summed against `total_cycles`, four camera states:

| state | frames/tick | state handler + `VSync_Wait` + `VBlank_Handler` + HInt | `total_cycles` | **gap** |
|---|---|---|---|---|
| `idle` | 1.033 | 125,230 | 127,789 | −2,559 (−2.0%) |
| `right` | 1.000 | 129,054 | 127,765 | +1,289 (+1.0%) |
| `down` | 1.000 | 129,053 | 127,769 | +1,284 (+1.0%) |
| `maxdiag` | 2.067 | 100,571 | 126,734 | **−26,163 (−20.6%)** |

**Their diagnosis of it** (`:425-429`):

> **The accounting closes at 1.000 frames/tick and loses a fifth of the frame at 2.067.** The
> common factor is preemption: when a logic tick spans a VBlank, the profiled routine that was
> executing across the boundary loses cycles. The same signature shows in throwaway B, where
> the probe's own inclusive-row check fires outright — `Tile_Cache_Fill` row 42,291 against
> 44,681 of children, a **negative** own cost.

**Why this is identity-design corroboration, stated plainly.** CR-26's reconciliation identity —
Σ `routines[].cyclesSelf` + Σ `interrupts[].cyclesSelf` + `unattributedCycles` == `sampleCycles`,
undivided and exact (`empyrean` `6d5cb4b`, `contract/protocol.md:1327`) — was argued on first principles:
that a sum with no closing term lets a loss hide. §7 is the same claim measured. A 26,163 cyc/frame hole
sat inside a shipped instrument, at the one camera state a root-cause parcel was *about*, and the only
thing that surfaced it was an arithmetic impossibility a human happened to notice in a table. Their own
verdict on that, at `:559-561`: *"An exact identity would have surfaced it on the first run, and it
directly revises how much of this packet's max-diagonal table can be quoted as exact."*

The consequences they draw for their own packet (`:431-441`) are worth carrying too, because they bound
what of it we may cite: the `maxdiag` decomposition in §2 is **indicative, not exact** (`:433`);
everything the packet *concludes* from is preemption-free (the exact `Block_Stage_Gen` decompress counts,
the `right`/`down` decompositions at 1.000 frames/tick, and `work_per_tick`); and — `:441` — *"It is why
the two single-axis states were built at all."*

---

## 2. §9(a) — the four confirmations, transcribed

`:549` — *"Already satisfied by v1 on arrival — no ask, just confirmation that these land."* These
generate no work. They are recorded because each one names a *specific* place the absence cost them,
which is what makes them usable as acceptance evidence rather than as praise.

**(a)1 — `cyclesSelf` per routine** (`:551-554`):

> Every exclusive figure in §2 was hand-derived by subtracting
> child rows from parent rows, which first required proving sole-parenthood for each callee
> by grepping every `jbsr` in the tree. `cyclesSelf` deletes both the subtraction and the
> proof burden.

**(a)2 — the completeness identity** (`:555-561`), the one quoted in §1 above:

> **The completeness identity (Σ self + unattributed == sample, exact).** This is the one
> that matters most here. Old oracle silently lost **26,163 cyc/frame — 20.6% of the frame**
> at max-diagonal while closing to within 1–2% at the three non-lagging states (§7). I found
> it only because a
> parent row came out smaller than its children. An exact identity would have surfaced it on
> the first run, and it directly revises how much of this packet's max-diagonal table can be
> quoted as exact.

**(a)3 — `stallCycles` per routine** (`:562-565`):

> Everything in this packet is ideal cycles. The two
> components it indicts — the S4LZ decode and the 280-word/tick patch loop — are both
> memory-traffic-bound, so their true share is understated by an unknown amount and the
> ranking in §8 could in principle change under real stall accounting.

This one has teeth beyond confirmation: it says the packet's **ranked fix list may reorder** under real
stall accounting. Our slice 6 therefore does not merely add a column to a report — it is capable of
changing which fix their arc does first. Worth saying to them once slice 6 lands.

**(a)4 — `perFrame[]` `{frame, cycles, stallCycles, hintCycles, vintCycles}`** (`:566-571`):

> The fill's cost
> is BURSTY: block-crossing ticks pay 4–5 synchronous decompresses, interior ticks pay
> none. 31-frame averages hide that entirely, and §6's most instructive result — throwaway B
> LOWERING the mean fill cost at `right` while RAISING the lag — is a fact about the spike
> distribution that only a per-frame series explains. Burst analysis is exactly the
> anticipated customer; this arc is one.

Note the field list matches the adopted fragment exactly (`bus-protocol.schema.json`,
`get_profiler_frames` → `perFrame.items`, vendored at `6d5cb4b`). No shape gap; nothing to reconcile.

---

## 3. §9(b) — theirs to compose, not ours to build

`:573` — *"Composable TODAY on oracle-aether via mclk-stamped watch hits — booked as method, not as
gaps."* `:575-576`: *"Noted for the record and for whoever runs the next parcel; these would have
shortened this one materially."*

**Recorded here as theirs to compose.** These are client-side timelines assembled from hits we already
record. Nothing on this list is an instrument gap, no CR follows from it, and we build nothing for it.
The reason it is transcribed at all is that a future round must not re-raise it as a gap.

**(b)5 — staging-claim timeline** (`:578-581`):

> watch writes to `Block_Stage_Next` / `Block_Stage_Gen`. Gives
> exact decompress instants with mclk, i.e. the rate AND its burst structure. I got the rate
> only because `Block_Stage_Gen` happens to be a monotone counter bumped once per claim; on
> a component without such an accident I would have had nothing.

**(b)6 — residency-lifetime timeline** (`:582-588`):

> watch `Block_Stage_Keys` slot writes (stage instant) and
> the `FindStagedBlock` hit path (use instant). The difference IS the
> "staged-but-evicted-before-use" waste. **This is the single instrument that would have
> turned §3's central claim from an inference — residency 3.53 ticks < lead 8 ticks, derived
> from slots ÷ rate — into a direct measurement of dead speculations.** Worth building
> before the fix parcel, so F2's success criterion is a measured lifetime rather than a
> proxy.

**(b)7 — budget timeline** (`:589-590`):

> watch `Cache_Fill_Budget` writes for the per-pass consumption
> profile, which is what would show whether a pass is budget-bound or geometry-bound.

### 3.1 What actually enables these — and one correction to how it has been described

The enabler is `emulator/watchpoint_hits` carrying a per-hit `{frame, mclk, seq, pc, value, old, via}`
tuple at all — CR-12, catalogued at `contract/protocol.md:937`. A timeline is `mclk` + `seq` over a
filtered hit stream, and all three of (b)5–(b)7 are exactly that.

> ⚠ **Precision, because it has been relayed loosely.** §11.15 did sharpen a watch hit's `mclk` — but
> **only for VDP-internal hits**, which it moved from step-granular (borrowing the draining CPU step's
> clock) to instruction-granular, stamped from the write itself (`protocol.md:1024-1030`, and the
> amendment paragraph at `:2858-2871`). All four of aeon's (b) targets — `Block_Stage_Next`,
> `Block_Stage_Gen`, `Block_Stage_Keys`, `Cache_Fill_Budget` — are **work-RAM** addresses, i.e.
> `space: bus` hits off the 68000 bus event stream, which §11.15 did not touch. So the honest statement
> is: **CR-12's `mclk` + `seq` is what makes (b) composable; §11.15 sharpened a different subset.** The
> conclusion (theirs to compose, today, no gap) is unchanged — only the citation is.

---

## 4. §9(c) — the three asks, transcribed verbatim

`:592` — *"GENUINELY NEW — these are the asks."* Design and disposition:
`docs/2026-08-19-streaming-asks-recon.md`.

### C1 — attribution correctness across an interrupt / frame boundary (`:594-605`)

> **C1. Attribution correctness across an interrupt / frame boundary.**
> The measured defect is §7: when a profiled routine is preempted by VBlank, its cycles go
> missing — 20.6% of the frame at 2.067 frames/tick, ~1% at 1.000. **Ask:** define and
> document what the profiler does when an interrupt preempts a profiled routine, and make
> `cycles` / `cyclesSelf` exact under preemption — credit the preempted routine's pre- and
> post-interrupt segments to it, and the handler to the handler. If an exact split is not free,
> a `preemptedCycles` (or `resumedSegments`) diagnostic per routine would at least make the
> loss visible and bounded. **Replaces:** my workaround of building two extra single-axis camera
> states purely to obtain a non-lagging measurement, which still leaves the actual subject of
> the parcel — sustained max-diagonal — decomposable only indicatively. **Note:** the v1
> completeness identity will REVEAL this defect, but does not by itself FIX the attribution;
> these are two different asks and this is the second one.

Their closing note is the right distinction and we adopt it: **reveal and fix are two asks.** The
identity is the first. C1 is the second.

### C2 — exact total call counts (`:607-613`)

> **C2. Exact total call counts, not integer-rounded per-frame averages.**
> `calls` is a per-frame average truncated to an integer. For a routine invoked 4.53 times per
> logic tick it reports `2`; `TileCache_DecompressBlock` and `S4LZ_DecompressDict` are called
> 1:1 and report `2` and `1`. Every rate in this packet therefore had to come from an engine-side
> monotone counter that happened to exist (`Block_Stage_Gen`) or from cache geometry. **Ask:**
> a `callsTotal` integer alongside the per-frame average, over the sampled window. **Replaces:**
> hunting the engine's RAM for an accidental counter — which worked here only by luck.

### C3 — per-routine caller breakdown (`:615-623`)

> **C3. Per-routine CALLER breakdown, even just top-N.**
> `TileCache_FindStagedBlock` has three live call sites inside a single routine (column fill,
> row fill, prefetch scan) plus a fourth elsewhere. No amount of parent/child subtraction can
> split a row across call sites that share a parent, so I built two additional camera states
> (`right`, `down`) whose sole purpose was to make one call site active at a time. **Ask:**
> `callers: [{addr, calls, cycles}]` per routine row, top-N is plenty. **Replaces:** constructing
> one bespoke camera state per call site — which does not generalise, since not every call site
> has a camera axis that isolates it. Of the three asks this is the one that most changes the
> shape of a decomposition parcel.

---

## 5. Their sequencing, and what it means for our corpus A/B

**Their side.** The fix parcels proceed on their own rulings and are not waiting on us. Packet `:452`:
*"Nothing here is started. F1 and F2 need rulings before anyone writes code."* F5 is
*"cheap and worth doing first purely to de-noise every subsequent measurement in this arc"* (`:520-521`), F4
is ready, and F1/F2/F6 are parked — `docs/DEFERRED_WORK.md` §0 at `3469c920`: *"F1, F2 and F6 are PARKED
for owner/controller rulings and must not be started before one."* Nothing in that ladder blocks on any
of C1–C3, and none of C1–C3 blocks on it.

**Our side, and one coordination hazard worth naming now.** Our corpus A/B is slice 7
(`docs/2026-08-19-profiler-recon.md:973`, protocol at `:981`), and it A/Bs against **Phase 0's**
measurement corpus, not against this diagnosis packet — a different artifact, whose merge SHA is still
**PENDING** (`profiler-recon.md:986`). So the A/B is independent of the streaming fix ladder and stays
unblocked throughout.

> ⚠ **But the A/B compares two instruments on one ROM, and their ladder moves the ROM.** F5 alone is
> *"3,548 cyc/frame amortised at `maxdiag`"* (`:519`), and F1+F2 together move max-diagonal from 2.067 to
> 1.107 frames/tick (`:45-46`). An A/B row measured on the pre-fix ROM is not comparable to one measured
> after. This packet hands us the pin for free: the delivered branch rebuilds to **`crc=06af0010`
> (debug, 713863 B)** and **`crc=e111dff7` (release, 698411 B)** — *"the pre-parcel identities"* (`:5-6`).
> **Recommendation: the slice 7 evidence document records the ROM CRC of every A/B row.** Without it the
> first landed fix silently invalidates the artifact, and the failure mode is a table that still looks
> fine.

---

## 6. Verification note

**Docs only. No `cargo` was run — another agent holds the serialized build lock — and no emulator MCP
tooling was used at any point.** Every `CHOKE-DIAGNOSIS.md` line cited above was read from
`git show 3469c920:docs/benchmarks/streaming/CHOKE-DIAGNOSIS.md` and every quoted sentence was verified at
its stated line number in that blob, not from a working tree that can move. The old-oracle anchors in §1
were re-verified firsthand against `/home/volence/sonic_hacks/oracle` rather than carried from our own
recon; the trace is in `docs/2026-08-19-streaming-asks-recon.md` §2.

Two claims in the dispatch brief were checked and one did not survive: the §11.15 attribution for (b)
is corrected in §3.1 above. The other — that the packet carries an explicit sequencing note — is
**not literally true**: §5's sequencing is assembled from `:451`, `:522` and `DEFERRED_WORK.md` §0, and
is labelled as our synthesis rather than as their sentence. The ROM-CRC hazard in §5 is ours, not theirs.

Branch `streaming-asks-recon`, cut from `m68000-microop-framework` at `a535384`.
