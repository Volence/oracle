# The watchpoint bus surface — CR-11 and CR-12, and the two rulings settled in this pass

> **★ ADOPTED 2026-08-15 — both, as a package, with eight conditions.** Ruled in
> `docs/2026-08-15-fable-ruling-cr9-cr11-cr12.md`; landed in `empyrean` `af434a2` (`protocol.md` §11.8).
> Both rulings this document settled are **upheld** — value-changes-not-write-counts, and the
> interactive-debugging line — as are poll-only, the handle-not-address argument, the seam, and §5's
> refusal list. The `via` census is adopted, the one core change.
>
> **The drafts below are left un-rewritten and the conditions are marked inline as block quotes**, per this
> project's convention, so the difference stays visible. Five conditions touch this document:
>
> 1. **★ Rebase onto §2.4** — which landed **2 h 11 m after this document was committed**. Three
>    non-conformances: `total`/`returned` missing from both list results (§4.2, §8); `caveats[]` replaced
>    by §2.4's optional singular `caveat` (§1.2, §8); the `limit` echo added.
> 2. **★ The `watch` param on `emulator/stopped` was prose-only** — in zero of the four JSON fragments
>    (§4.3, §7). CR-16's exact defect. Now in the schema.
> 3. **The watch cap is specified**: `-32005 {reason:"watchCapReached", cap, count}` (§7).
> 4. **`censusKey` is refused, not ignored**, without `mode:"census"` (§7).
> 5. **`watchpoint_list.limit` takes the house 4096 cap** (§4.2, §8); the package hedge in §8.1 is struck;
>    every `protocol.md` anchor below is **recomputed in the ruling document, not here** — they were exact
>    at this document's base (`empyrean` `18a551e`) and are now stale by between +10 and +507 lines
>    depending on section, with five having moved section or been rewritten outright.
>
> Conditions 6 and 7 are "as drafted". Condition 8 sequences the handlers **after** the rows and fragments,
> which have now landed — so the implementation pass is unblocked.

**Status: DRAFT, raised not applied.** Base: `f69d361` on `m68000-microop-framework`. No contract file was
edited; §8 is explicit that *"deviations are raised as change requests against this file, not implemented
unilaterally."* Directed by `docs/2026-08-15-fable-ruling-attribution.md` ("Directed next"): *"The watchpoint
surface is the next CR pair to draft — a hits-reading method, plus a `space` parameter on `watchpoint_add` —
with the 'measure value changes, not write counts' shape ruling settled **in that pass**, not inherited by
default."*

CR numbers **11** and **12** are reserved for this document. CR-9 is the `press` reason
(`docs/2026-08-14-aether-change-requests.md:351`); CR-10 is the coordinate-shaped read
(`docs/2026-08-15-pixel-attribution-bus-method.md:193`).

## 0. Method, and what I did not do

Every claim below is anchored to a line I opened in this worktree. Where I am inferring rather than reading,
the sentence says so. I read `crates/oracle-core/src/watchpoints.rs` lines 1–1029 (the whole non-test body
plus the first test block), the contract in full, the schema in full, and the frontend paths named below.

The previous design in this arc had one wrong anchor and **two false provenance claims struck before they
reached the contract's permanent record** (`docs/2026-08-15-fable-ruling-attribution.md:42-54`). So: I make
no chronology claim in this document at all. Nothing here rests on when anything landed — only on what the
files say now. Where I quote a figure from another repo I opened that repo and read it (§2.1).

---

## 1. The evidence

### 1.1 What §6 has

`empyrean/contract/protocol.md:568-574`, the whole *breakpoints & watchpoints* section:

| Method | params | result |
|---|---|---|
| `emulator/breakpoint_add` | `addr`\|`symbol` | `addr` |
| `emulator/breakpoint_list` | — | `breakpoints[]{addr,enabled,hits}` |
| `emulator/breakpoint_clear` | `all`\|`addr`\|`symbol` | `removed` |
| `emulator/watchpoint_add` | `addr`\|`symbol`, `read`?, `write`? | `addr` |

One watch row. It has no space, no range, no mode, no lifecycle inverse, and its result is the address the
caller already supplied — which D12 names as its own defect class: *"a result that only echoes its own input
cannot distinguish 'my condition happened' from 'I gave up waiting'"* (`protocol.md:154`). There is **no
`watchpoint_list` and no `watchpoint_clear`**, though the three breakpoint rows immediately above it have
exactly that shape. Nothing in §6 returns a watchpoint hit.

The schema (`schema/bus-protocol.schema.json`) schematizes nine methods (`lookup_symbol`, `read_memory`,
`write_cram`, `registers`, `run_to`, `checkpoint`, `restore`, `checkpoint_list`, `checkpoint_drop`, at lines
212–393). No watch method is among them. Our own `METHODS` table has twenty rows
(`crates/oracle-aether/src/engine.rs:97-198`); no watch method is among them, and
`crates/oracle-aether/src/engine.rs:607` advertises `"watchpoints": false`.

**One thing §3 already has that §6 cannot produce.** The `emulator/stopped` `reason` enum includes
**`watchpoint`** (`protocol.md:411`, schema line 190). No catalogued method can cause that stop: the one
watch row says nothing about halting, and `breakpoint_add` is the only other member of its section. The
contract anticipated a watchpoint stop and never specified what arms one. That gap is CR-11's, and §4.3 is
where it gets settled rather than assumed.

### 1.2 What the core actually provides

`crates/oracle-core/src/watchpoints.rs`, 1,956 lines, of which 1–927 are the facility and 928–1956 are its
unit tests. It is a `BusEventSink` the **caller** owns — *"stored by the caller (never by `System`), and so
sits in neither frozen currency and can never move a state hash"* (`:11-13`). The handoff's one-line summary
(*"`watchpoints.rs` already has record/count/census modes — it is simply not on the bus"*,
`docs/2026-08-15-handoff-capability-layer.md:73`) is true but undersells it by about half. What is there:

| capability | anchor | notes |
|---|---|---|
| `Watch` builder | `:274-380` | `space`, `lo`, `hi`, `op`, `fc?`, `size?`, `addr_parity?`, `mode`, `key_cap`, `stop_after?`, `label` |
| Four spaces | `:149-155` | `Bus` (68000 address space) and `Vram` / `Cram` / `Vsram` (VDP-internal **byte** addresses) |
| Three read modes | `:177-190` | `Record` (ring), `Count` (store nothing), `Census(CensusKey)` (bounded group-by) |
| Seven census keys | `:196-215` | `Addr`, `AddrPage(u8)`, `Fc`, `Op`, `Size`, `Value`, `ValueHiEqLo` |
| Stop-on-condition | `:369-379` | `stop_after(n)` raises `BusEventSink::stop_requested`, so the run ends at the next instruction boundary with `StopReason::SinkRequested` |
| Handles | `:167-173` | `WatchId(u32)`; **ids are never reused** — `clear`/`remove` retire one permanently *"so a stale handle resolves to nothing rather than silently to a different watch"* |
| The hit | `:475-507` | `watch`, `space`, `addr`, `old`, `value`, `size`, `op`, `fc`, `via`, `pc`, `frame`, `mclk`, `seq` |
| Per-watch report | `:512-542` | config + `matched`, `first`/`last` `Stamp`, `census`, `distinct_keys`, `key_cap`, `keys_capped`, `census_overflow` |
| The ring | `:545-562`, `:640-654` | drop-**oldest**, amortized O(1), capacity fixed at construction |
| Loss + controls | `:723-752` | `hits()`, `take_hits()`, `dropped()`, `seen()`, `matched()` |
| Timing basis | `:766-768` | returns `TimingBasis::NTSC`, derived from `MCLK_PER_FRAME` |
| Caveats | `:772-810` | `Vec<String>`, emitted *with* the numbers: `seen == 0`, VDP-internal step-granular `mclk`, the PSG-port master ambiguity, and a capped census |

> **Condition 1 (the `caveats[]` → `caveat` collapse).** Core's `Vec<String>` is right for core and wrong
> for the wire: §2.4 defines `caveat` as **singular, string, optional, handler-emitted, never parsed**.
> Nothing was lost placing the four, because this document had already given every machine-actionable half
> a typed key for §2.4 rule 3's reason:
>
> | core's caveat | on the wire |
> |---|---|
> | `seen == 0` | the typed key **`seen`**, REQUIRED on every hits read |
> | census hit its key cap | **`keysCapped`** + **`censusOverflow`** (+ `keyCap` / `distinctKeys`) |
> | a VDP hit's `mclk` is step-granular | **§6 prose, as a permanent property of a VDP hit** |
> | the PSG-port `fc` census cannot attribute a master | unreachable from the wire — §4.6 does not expose `fc` as a `censusKey` — and left to the optional `caveat` if that ever changes |
>
> The third is the one that moved rather than mapped, and §2.4's own advisory is why: *a caveat that is
> always present is documentation wearing signal's clothes, and clients learn to ignore it — including on
> the one reply where it would have mattered.* It is now read once by an implementer instead of ignored
> forever by a client. `caveat` is still **declared** in every fragment, because §2.4 clause 4 requires it
> of any method that can emit one or §8 item 20's closure rejects the reply.
>
> *One anchor correction while here:* `old: 0` is at `:851`, not `:850`, and `old: w.old` at `:892`, not
> `:891`. The claim survives; the anchors were each off by one.

Four properties matter to the wire and are easy to get wrong:

1. **`old` is meaningful only off the bus.** *"The value that was there **before** the access. Meaningful for
   VDP-internal writes …; `0` for bus accesses (the bus event stream carries no prior value)"* (`:483-485`),
   and the sink confirms it: `on_event` builds the hit with `old: 0` unconditionally (`:850`), while
   `on_vdp_write` uses the captured `w.old` (`:891`).
2. **`fc` is meaningful only on the bus.** `0` for a VDP-internal write, *"there is no bus function code — the
   CPU-vs-DMA distinction is in `via`"* (`:494-496`, `:896`).
3. **`seq` is a stable monotonic id across ring drops**, *"so a gap in `seq` marks dropped hits"* (`:469-471`);
   it is assigned in `dispatch` to every **matched** access whether or not it is stored (`:660-662`).
4. **A hit is stored at most once** — attributed to the lowest-id matching `Record` watch — while *every*
   matching watch updates its own aggregates (`:656-680`). So the ring is one shared stream and the per-watch
   counts are not derivable from it.

The ring's bound and drop policy, exactly: `Watchpoints::new(cap)`; at capacity the oldest is discarded and
`dropped` incremented (`:640-654`); `cap == 0` records nothing and counts every hit as a drop, which the doc
calls *"a legitimate configuration for a pure `Count`/`Census` run"* (`:565-567`).

### 1.3 What the consumers do — and there are two, not one

**The player.** `crates/oracle-frontend/src/main.rs:869` constructs `Watchpoints::new(WATCH_CAP)` with
`WATCH_CAP = 8192` (`:236`). A left-click resolves the dot and arms one watch per target:

```rust
// main.rs:919-931
watchpoints.clear();
for t in &p.targets {
    let space = match t.space { pick::Space::Vram => WatchSpace::Vram, pick::Space::Cram => WatchSpace::Cram };
    watchpoints.add_vdp_watch(space, t.lo..=t.hi, WatchOp::Write, t.label.clone());
}
```

A sprite dot arms **two** — the drawing pattern and the 8-byte SAT entry (`pick.rs:126-145`), which is why
"the address" cannot name a watch (§4.1). `W` calls `dump_hits` (`main.rs:942-943`), which prints
`hits().len()`, then per hit `seq`, `frame`, `pc` (symbolised through `SymbolTable::resolve_within` at
`:461`), `addr`, `old->value` and `via` (`:450-468`), then `dropped()` (`:469`). `C` calls
`watchpoints.clear()` and disarms (`:951-955`). The sink is attached to the run **only while armed** —
`watch_armed.then_some(&mut watchpoints)` (`:1273`), and the two no-audio arms at `:1285` and `:1295`.

**The second consumer, which the sweep missed.** `crates/oracle-core/examples/watch_probe.rs` is a dev tool
over a real ROM: `add_watch` / `add_vdp_watch` (`:121-122`), then `hits()` and `dropped()` (`:126-127`). It
supports all four spaces and three ops from the command line (module header, `:6-20`). It uses no mode, no
`stop_after`, no `seen()`, no `caveats()`, no `matched()`.

**So the executed consumer surface of this capability is exactly: arm (any of four spaces, a range, an op, a
label) → read hits → read the drop count.** That is the surface CR-11/CR-12 must carry, and §5 is where the
rest is refused.

### 1.4 Where the previous sweep is wrong or imprecise

`docs/2026-08-15-pixel-attribution-bus-method.md:122-144` (violation D). The three capabilities it names are
**all confirmed** against the source, at the anchors it gives. Four corrections:

1. **`watchpoints.rs:623-628` is off by a line-and-a-bit.** That span is the doc comment plus the first line
   of the signature; `add_vdp_watch` is `:628-636`. Harmless, but the last adjudication checked anchors, so
   this one is corrected rather than repeated.
2. **"Structurally the same honesty D17 made mandatory for `droppedEvents`" is right on principle and wrong
   on scope**, and the distinction changes where the field goes. `droppedEvents` is a **connection** fact —
   D17's own words: *"two clients reading at the same instant will legitimately disagree about it"*
   (`protocol.md:277-278`). The ring's `dropped()` is an **instrument** fact: one number, identical for every
   client, about loss inside the machine's own recorder. That is a §2.2-shaped quantity, not a §2.3-shaped
   one, so it belongs in the **result body** and must never be lifted into the envelope. §3 depends on this.
3. **"On evidence this outranks attribution"** — the ruling already disputed this on the grounds that the
   handoff's evidence was *request* evidence
   (`docs/2026-08-15-fable-ruling-attribution.md:61-71`). I can now settle it on executed evidence: this
   capability has **two** in-tree executed consumers (`main.rs` and `examples/watch_probe.rs`) against
   attribution's two (`pick.rs` and the `shots` diagnostic at `main.rs:2082`). **They are peers, and the
   ruling's ordering was right.** The sweep's claim is not supported by the yardstick it invoked.
4. **The handoff's ranked item 4 conflates two instruments**, and the conflation is load-bearing enough to
   have its own refusal in §5. It calls this *"Per-frame value trace — the most-requested missing instrument,
   **which never existed**"* (`docs/2026-08-15-handoff-capability-layer.md:70-73`) and then, one sentence
   later, points at the core file that implements a watch recorder with two live consumers. Both halves
   cannot be true. What is true: **a per-frame value trace and a watchpoint hit log are different
   instruments** — a sampler reads one address once per frame whether or not anything wrote it; a watch
   records accesses and is silent about an address nobody touched. Neither the core nor these CRs delivers
   the sampler.

One thing the sweep got exactly right and I want to keep visible: `watchpoint_add`'s params *"carry no space
and no range"*, so *"a bus client cannot ask for the 'who wrote this tile?' watch at all."*

### 1.5 A contamination vector already live in our own player

Core's `clear()` removes the specs and **leaves the hits**: *"Recorded hits are left intact — drain them with
`take_hits`. Ids are not reused, so hits recorded before the clear keep naming watches that no longer exist"*
(`:688-693`). The player's `C` key calls `clear()` and nothing else (`main.rs:951-955`), and a click calls
`clear()` before arming the new targets (`main.rs:919`).

**Consequence, read off those five lines:** click a sprite, let it record, click a different sprite, press
`W` — and the dump interleaves hits from both watches, in one `seq` order, with no way to tell them apart
(`dump_hits` prints no watch id, `main.rs:464-467`) and with the old watch's label no longer resolvable
(`label_of` returns `None` after `clear`, `:715-720`). That is the stale-instrument shape at small scale, in
our tree, today. It is not a bug I was sent to fix and I have not fixed it; it is the reason §4.4 makes
per-hit watch attribution a **required** field rather than an optional nicety.

---

## 2. The two rulings

### 2.1 Ruling A — what the bus reports is not a write count

**The evidence, read at source rather than inherited.** The handoff cites *"a census found 97% of freq writes
were redundant re-writes of unchanged values and nearly funded two unnecessary features"*
(`docs/2026-08-15-handoff-capability-layer.md:70-73`). I opened it: `aeon/docs/DEFERRED_WORK.md`, section
**"Per-frame pitch / volume envelopes (Phase 3a #2/#3) — DEFERRED, build-on-demand"** (heading, not a line
number — `docs/2026-08-15-handoff-capability-layer.md:163` records that citations into that tree are
perishable). It says: a VGM census *"first looked like MT needed these (oracle wrote freq ~16×/note, TL
~33×/note). **Re-measurement proved that was an artifact:** the Zyrinx driver re-asserts every register every
frame (60Hz full-state refresh) — **97% of its freq writes and 99% of its TL writes are redundant re-writes
of UNCHANGED values.** Normalized to actual value *changes* per note, ours ≈ oracle (freq 0.92 vs 0.93/note;
TL 0.43 vs 0.50/note)."* It closes: *"LESSON: register write-COUNT is a misleading proxy; measure value
CHANGES."* The two features were a per-frame pitch-envelope processor and a per-frame volume-envelope/TL
processor.

**The ruling.** The bus surface must not present a write count as the answer, and it must not present itself
as the value trace. Three parts:

1. **`matched` (the write count) is reported, and never alone.** Every read that carries it also carries
   either the distinct-value census or the per-hit `old→value` pair, so the ratio that exposed the artifact is
   computable from one call. A reply that carried only a count would reproduce the exact instrument that
   nearly funded two features.
2. **The honest change-measurement is per-space, and the schema says which.** For a VDP-internal watch, `old`
   is the real pre-write value (`:483-485`, `:891`), so `old != value` is exact per hit and a client can count
   changes precisely. For a **bus** watch, `old` is structurally `0` (`:850`) — the bus event stream carries no
   prior value — so a change count is *not* derivable at all. Therefore `old` is present **if and only if**
   `space != "bus"` (§4.5). Emitting `"0x0"` for a bus hit would be a silent wrong answer of exactly the class
   D11 exists to prevent, and it is the single most likely way for this ruling to be defeated in
   implementation.
3. **The aggregate that survives the fallacy is a distinct-value census, and it already exists.**
   `WatchMode::Census(CensusKey::Value)` gives `key -> count` over the values written plus `distinct_keys`
   (`:186-190`, `:196-215`, `:531-541`), with the cap never silent (`keys_capped`, `census_overflow`,
   `:801-807`) — the cap default is **256**, chosen precisely because *"a census silently capped at 16 would
   have reported '16 distinct' and been confidently wrong"* (`:90-97`). This is the aggregate the aeon
   re-measurement produced by hand, and it needs no new core.
   *Its honest limit, stated:* a value census cannot separate `A,A,A,B` from `A,B,A,B` — both are two distinct
   values, one change versus three. Where that distinction matters the answer is the per-hit `old→value` pair
   on a VDP-space watch, not a cleverer aggregate.

**What would change my mind:** a consumer that needs exact change counts on the **68000 bus**. That needs a
last-value-per-address table in core, which is real new state on the instrumented path, and no consumer has
asked. Registered, not built.

### 2.2 Ruling B — which side of the interactive-debugging line each piece falls on

The record against interactive debugging is specific, not vague. `docs/2026-08-14-aether-change-requests.md:444-447`:
*"the archaeology's negative evidence on interactive debugging is strong and specific (recon §2a: three
independent statements of harm, including a **1,691,410-hit stale breakpoint contaminating later
captures**). `run_to` gives the non-blocking stop-on-condition the record actually wants."* The handoff's
*Do NOT build* list opens with interactive `step`/`step_over`/`step_out`
(`docs/2026-08-15-handoff-capability-layer.md:94-96`), and its resolved line is
*"breakpoint-as-deterministic-anchor is proven; breakpoint-as-interactive-session is proven harmful"*
(`:116-117`).

Every piece proposed here, sorted:

| proposed | side | why |
|---|---|---|
| `watchpoint_add` with `space`/range/`op`/`mode`/`label` | **anchor** | Arms an observer. Nothing halts, nothing blocks, nothing single-steps. |
| `watchpoint_clear` | **anchor** | It is the *anti*-staleness half. Its absence is what the 1,691,410-hit incident is made of. |
| `watchpoint_list` | **anchor** | Makes an armed instrument visible. A stale watch that appears in a list with a moving `matched` is not stale-and-silent. |
| `watchpoint_hits` | **anchor** | A post-hoc read of recorded evidence. No run, no halt, no session. |
| `stopAfter` → `stopped {reason:"watchpoint"}` | **anchor** | Non-blocking stop-on-condition, at an instruction boundary, ending an already-**bounded** run. This is precisely *"`run_to` gives the non-blocking stop-on-condition the record actually wants"*, generalised from "PC reaches X" to "the machine touched X". |

**Not proposed, on this ruling:** break-on-hit as an interactive session; any blocking wait for a hit; any
`watch_step`; resuming from a hit; and — importantly — **any pushed per-hit event** (§3).

**The catalogued row is ambiguous about halting, and CR-11 pins it.** `emulator/watchpoint_add | addr|symbol,
read?, write?` does not say whether a watchpoint *stops the machine*. The classical debugger reading is
break-on-access, which is the interactive shape the record calls harmful; our core's default is
`WatchMode::Record`, which observes and never halts (`:62`, `:82-83`). Two conformant servers could ship
opposite behaviours from that row today. CR-11 proposes to pin it: **a watch records by default and halts
only when `stopAfter` is given**, which is both what our core does and the side of the line the record
supports.

**And the design against silent contamination, concretely** (§4.4 carries the detail): every hit names the
watch that recorded it; every armed watch is listed with `matched`, `first`, `last` and its `stopAfter`;
every hits read carries `seen`, the structural negative control (*"`seen > 0, matched == 0` is a live
instrument that found nothing; `seen == 0` is an instrument that was never attached, and a zero from it means
nothing at all"*, `:741-746`); and a `stopped` event caused by a watch names that watch. A stale watch under
this design is loud in four places. It is not auto-cleared on disconnect — §5 says why.

---

## 3. Hits: poll, push, or both? — **poll only.** Recommended

D6 calls push *"the highest-leverage single upgrade in Phase 1"* (`protocol.md:74-79`), so the reflex is a
`emulator/watchHit` notification. **Against**, on three grounds, and the third is the one I would not have
found without reading D17 closely:

1. **Volume.** The core's own module docs: at ~8,700 CPU steps per frame a raw per-access log *"would swamp
   the signal"*, which is why *"aggregation is the primary read mode here; the event log is the fallback"*
   (`:56-60`). The measured scale in this repo's own record is not hypothetical: `direct_color_dma` writes
   **4,923,206 CRAM words over 120 frames** and `cram_flicker` **420,386**
   (`docs/2026-07-25-testrom-conformance.md:787`, `:809`). A push channel fed by that is not an event stream.
2. **There are already two lossy stages, and pushing would couple them.** Stage one is the core ring:
   drop-oldest, counted by `dropped()` (`:640-654`, `:735-739`). Stage two is the event queue: bounded,
   non-blocking, counted by `droppedEvents` (§8 item 4, D17). Pushing hits routes stage-one survivors into
   stage two — and then **`droppedEvents` moves**, which D17 defines as the signal that a client *"MUST treat
   any state it inferred from the event stream as stale and re-read it"* (`protocol.md:398-400`). Hit spam
   would therefore degrade the trust signal for `stopped` and `romReloaded`, which is the one thing that
   channel exists to carry reliably. The lossy stages are not independent; they share a counter's meaning.
3. **The one thing worth pushing already has an event.** A watch that reaches `stopAfter` ends the run, and
   §3 already has `reason: "watchpoint"` for exactly that. So the push half of this capability is a stop
   notification the contract already defines — no new event, no new subscription, no second drop counter.

**Therefore: no new event, and no second drop counter.** The ring's `dropped` is reported in the
`watchpoint_hits` **result body** (§1.4 item 2 — an instrument fact, not a connection fact), and
`droppedEvents` keeps meaning exactly what D17 says it means.

*What would change my mind:* a consumer that must react to a hit within the frame it happens. There is none —
both executed consumers read the log after the fact (`main.rs:942`, `watch_probe.rs:126`) — and if one
appears, `stopAfter` plus `stopped` serves it without a per-hit stream.

---

## 4. The judgement calls, stated as judgement calls

### 4.1 Is a watch an opaque handle or an address? — **an opaque handle (D9 category 4).** Recommended

`watchpoint_add`'s catalogued result is `addr`, i.e. the input echoed back. An address cannot identify a
watch, for three reasons I can point at:

- **One address can carry several watches.** A single sprite click arms two (`pick.rs:126-145`), and core
  explicitly supports overlap: *"When several watches match one access it is recorded **once** … (every
  matching watch still updates its own aggregates)"* (`:477-479`).
- **One numeric address exists in four spaces that never cross-trigger** (`:146-148`). `0x0AA0` is a VRAM byte
  and a bus address at once.
- **`WatchId` is already a handle by D9's own test.** It is server-assigned, never client-proposed, and
  **never reused** — *"`clear` and `remove` retire an id permanently, so a stale handle resolves to nothing
  rather than silently to a different watch"* (`:167-173`). That is verbatim the §6.1 checkpoint-id argument
  (*"it names a snapshot, not a position"*, `protocol.md:655-657`), and D9 category 4's test is *"a type a
  client must never compute on should not be a number, because a number invites the computation"*
  (`protocol.md:113-115`).

So: `watch`, a JSON **string** via `$defs/handle`, from the first line of the implementation. §8 item 16
records this server as non-conformant today for shipping a checkpoint id as a number
(`protocol.md:813-821`); repeating that on a brand-new surface with the ruling already on the books would be
inexcusable.

*What would change my mind:* if watches were guaranteed one-per-address-per-space. They are demonstrably not.

### 4.2 What bounds the hit list, and is it cursored? — **`limit` + `cursor` + `truncated`.** Recommended

This is the opposite call from CR-10's `candidates`, and the difference is the one §6.1 already draws.
`candidates` is bounded **structurally** at 4 (`docs/2026-08-15-pixel-attribution-bus-method.md:390-400`), so
it cannot be partial. A hit ring is bounded by **server policy** — `Watchpoints::new(cap)`, `8192` in the
player (`main.rs:236`) — which is precisely why `checkpoint_list` is cursored
(`protocol.md:696-698`). A client must never hold a partial hit log and believe it complete.

**The cursor is cheap here and satisfies §6.1's invariant by construction.** `seq` is monotonic, never
reused, assigned to every matched access, and *"stable across ring-buffer drops"* (`:469-471`). §6.1's
non-normative hint is literally realizable: *"the first id strictly greater than the one you were given
cannot skip or repeat"* (`protocol.md:713-716`).

Two honest sub-rules that follow from the ring being lossy at the *record* end:

- **Resuming from a cursor whose hits have since been dropped is not an invariant violation.** §6.1's rule is
  that a cursor *"never skips an item that was live at both requests"* (`protocol.md:703-705`). An item
  dropped by the ring between the two requests was not live at both. The client learns about it from
  `dropped` moving, which is why that field is REQUIRED and not optional.
- **The bus MUST use `hits()` and never `take_hits()`.** `take_hits` drains (`:727-733`); on a bus two clients
  share (D13's argument for engine-owned checkpoints, `protocol.md:645-647`), a draining read means one client
  silently steals another's evidence. Non-destructive read plus an explicit `watchpoint_clear` is the same
  split §6.1 uses for `checkpoint_list` versus `checkpoint_drop`.

`limit`: minimum 1, maximum 4096 (matching `read_memory`'s house cap, schema line 241), default 100. That
default is a policy number and is stated as one — which is exactly why the reply carries `truncated` and a
`cursor`.

> **Conditions 1 and 5 (what a policy-bounded list must carry).** This section reasons its way to the
> **right** conclusion — *"a hit ring is bounded by server policy … which is precisely why `checkpoint_list`
> is cursored"* — against the rule as it stood when this was written, and §2.4 clause (a) had already
> generalised it by the time it was read. `truncated` alone is not enough: **`total` and `returned` are
> REQUIRED beside it**, and `limit` is an optional echo of the ceiling the server actually applied. Both
> list results take them, in `checkpoint_list`'s **flat** spelling rather than `$defs/boundedList` — the
> list *is* the whole result here, and a nested container inside a result whose entire content is that
> container buys a level of indirection and nothing else.
>
> `total` is the number that will get misread, so §6 pins what it is **not**: it is how many hits the ring
> currently *holds* matching the query — not `matched` (accesses, including ones no ring stored) and not
> `dropped` (hits already discarded). Three numbers, three questions.
>
> And the cap in this paragraph applies to **`watchpoint_list.limit` too**, which the draft left at
> `minimum: 1` with no maximum while its sibling capped at 4096. A `limit` bounded on one list and
> unbounded on its twin is two policies wearing one name.

### 4.3 Does arming require a paused machine? — **no.** Recommended

§6's run-control state rule names a closed list — `run_to`, `run_to_scanline`, `run_frames`, `step*`, `press`,
`reload_rom` — and gives the reason: *"they mutate the timeline just as surely"* (`protocol.md:531-539`).
Arming a watch mutates an **observer**, not the timeline: `Watchpoints` *"observes only: it never touches CPU
or memory state"* (`:11-13`). Gating it would be a new rule, and §5's ban on implicit mode changes cuts the
same way — a bus that refuses a read-shaped call teaches clients to pause defensively.

**The one case that genuinely differs is `stopAfter` on a free-running machine**, where a watch will end
free-run at a moment nobody asked for. I still recommend against a gate, because a conditional gate ("paused
required only when `stopAfter` is present") is unprecedented on this bus and harder to discover than the
thing it prevents. Instead the stop is made **attributable**: §3's `stopped` gains an additive `watch` param
(CR-1's precedent: *"additive params on a catalogued event are not a new op"*,
`docs/2026-08-14-aether-change-requests.md:63-65`), so a free-run halt always names the watch that caused it.

*What would change my mind:* one recorded instance of a client being surprised by a free-run halt it could
not attribute. Then gate `stopAfter`, not arming.

> **★ Condition 2 (the additive `watch` param stopped at the prose).** The call is upheld — the halt is
> made attributable rather than gated — but the param **never reached a JSON fragment**. This document has
> four fenced JSON blocks (`:624`, `:693`, `:769`, `:892`); `emulator/stopped` appears in JSON at `:626`
> (a `$comment`) and `:650` (a `description`), and **in none of the four is there an `events` entry**. That
> is CR-16's exact defect — a registration that stopped at the prose — proposed on the day CR-16 was
> adopted for it, and under §8 item 20's closure the server's own conformant stop event would have been
> rejected by the artifact meant to describe it.
>
> It is in the schema now, in `events["emulator/stopped"].params`, alongside CR-9's `buttons`/`port`. And
> unlike those two it is **mechanically bindable**: `reason: "watchpoint"` is a discriminator, so an
> `if`/`then`/`else` requires `watch` when the reason is `watchpoint` and forbids it otherwise. `buttons`
> and `port` cannot be bound that way, because CR-9's ruling makes `reason` name the stop *condition* and
> never the driving method — recorded in §11.7 as a real cost rather than hidden.

### 4.4 How is a watch cleared, and what stops a stale one contaminating a later capture?

**`emulator/watchpoint_clear {watch | all} → removed`**, mirroring `breakpoint_clear` (`protocol.md:573`) and
`checkpoint_drop`. Clearing an unknown handle **succeeds with `removed: 0`**, for §6.1's reason pinned
verbatim: *"deletion is idempotent … an error a client must learn to swallow teaches clients to swallow
errors"* (`protocol.md:684-694`). Two clients racing to clear one watch is normal traffic on a shared bus.

**Clearing does not delete the hits already recorded** — that is core's documented behaviour (`:688-693`) and
I am not proposing to change it, because a destructive clear would let one client erase another's evidence
(the same argument as `take_hits` in §4.2). The staleness is instead made **impossible to miss**, in four
places, none of which is a new counter:

1. **Every hit names its watch.** `hits[].watch` is REQUIRED. This is the fix for the interleaving described
   in §1.5, which our own player has today because `dump_hits` prints no watch id.
2. **A retired handle is discoverable.** A handle that appears in `hits[].watch` but not in
   `watchpoint_list` is a cleared watch; ids are never reused (`:167-173`), so that test can never give a
   false negative. The schema description says this in words so a client does not have to derive it.
3. **`watchpoint_list` shows the armed instrument with its aggregates.** `matched`, `first`, `last`,
   `stopAfter`. A watch left armed across a later capture shows a `last` stamp inside that capture — the
   contamination is legible as a coordinate, not inferred.
4. **`seen` is REQUIRED on every hits read.** Core's structural negative control (`:741-746`): `seen > 0,
   matched == 0` is a live instrument that found nothing; `seen == 0` is an instrument that was never
   attached. Under the hosted arrangement this is not decorative — see §6.2 — because there are two run
   drivers and a watch armed over the bus is worthless unless the loop that is actually running carries the
   sink. `seen == 0` is what makes that failure self-announcing instead of reading as "nothing happened".

**Rejected: a `staleHits` count in the result.** It is derivable from (1) and (2), and §3 has already argued
against adding counters by reflex. Recorded as considered.

**Rejected: auto-clearing a connection's watches on disconnect.** Tempting, and wrong here. Watches are
engine-owned like checkpoints — *"the coordinates belong to the machine, so two clients on one bus see one
set"* (`engine.rs:308-311`) — and a watch with `stopAfter` changes how the machine runs, so silently
disarming one on a socket close is a machine-state change nobody asked for, which §5 forbids in the
neighbouring case (`protocol.md:488-491`). The visibility rules above are the answer instead.

### 4.5 Does `space` extend `watchpoint_add`, or justify a new method? — **extend it.** Recommended

Extend, with `space` defaulting to `"bus"` so the catalogued call is unchanged (D5 additive; no client
breaks). Grounds:

- **Core has one `add`.** `add_watch` and `add_vdp_watch` are two-line shorthands over the same
  `Watch::in_space` / `Watchpoints::add` (`:291-325`, `:586-636`). Two bus methods over one primitive is
  surface without a distinction.
- **One ring, one `seq` order.** Bus and VDP-internal hits interleave in a single log with a single monotonic
  ordering (`:660-680`). Two arming methods would imply two streams and invite two reading methods, which is
  the wrong shape for a single ordering that is the whole point of `seq`.
- **One hit type.** `WatchHit` carries `space` as a field (`:476-507`); the wire shape does not fork.

The range likewise extends rather than forks: **`addr` + `len` (default 1)**, matching `read_memory` /
`read_vram` house style and mapping to core's `addr..=addr+len-1`. `len: 1` reproduces the catalogued
single-address call exactly.

`symbol` stays valid **only** for `space: "bus"` — a symbol resolves to a 68000 address, and D7's whole
argument is that clients resolve rather than hardcode. `symbol` with a VDP space is `-32602`.

*What would change my mind:* if VDP-space watches needed a different **result** shape. They do not; the
difference is entirely in which of `old` and `fc` is meaningful, and §2.1 handles that with presence rules.

### 4.6 Which census keys go on the wire? — **`addr`, `value`, and `via`.** Recommended, with `via` flagged

Core has seven (`:196-215`). Exposing all seven would be surface for its own sake; exposing none would defeat
Ruling A. The three I propose each have an executed episode behind them in this repo's or the suite's record:

| key | evidence |
|---|---|
| `value` | Ruling A. The aeon re-measurement is a distinct-value census done by hand. |
| `addr` | `cram_flicker` was settled by *"exactly **two entries: index 4 and index 36**"*, and `direct_color_dma` by *"entirely on CRAM index 0"* (`docs/2026-07-25-testrom-conformance.md:787`, `:809`). Both are group-by-address over a CRAM watch, and both **retracted a wrong root cause** — the old `cram_flicker` reason *"does not survive it"* (`:793-799`). |
| `via` | **Does not exist in core.** The other half of those same two findings is *"all CPU writes, zero DMA"* and *"99.997% DMA (4,923,072 of 4,923,206…)"* — a group-by-`via`. `CensusKey::Fc` cannot answer it on a VDP watch: *"On a VDP-internal watch this is always 0"* (`:202-205`). Adding `CensusKey::Via` is ~8 lines beside the existing arms. |

**`via` is the one item here that needs a core change, and I am flagging it as a judgement call rather than
folding it in.** The case for it: the marquee use of this instrument is *"who wrote this tile — the CPU or a
DMA?"*, the per-hit `via` is bounded by the ring (8,192 in the player) while these ROMs write millions of
times, so the ring **structurally cannot** answer it and the census can. The case against: no consumer has
called for it yet. *What would change my mind:* if the ruling prefers zero core change in this pass, drop it
and register it — `addr` and `value` alone still satisfy Ruling A.

**Not exposed, and why:** `addrPage(n)` (needs a shift param and has no consumer), `fc` (carries a documented
attribution caveat at `:789-800` and is answered by `via` for the case that matters), `op` and `size` (a
watch already filters on both — a census over a filter you set yourself answers a question you already knew).
`valueHiEqLo` is an open-bus probe shape from `examples/k4_openbus_probe.rs` with no bus consumer.

### 4.7 Reads that are not gated, and one shape trap

`watchpoint_list` and `watchpoint_hits` are pure reads and are **not** subject to §6's run-control rule, for
§4.3's reason.

**The trap, stated because it is the easiest way to break D11.** A hit has its own `frame`, `mclk` and `pc`.
`frame` and `mclk` are envelope stamp names, and §2.2 requires the server to apply the stamp *"after the
handler produces its payload"* and to *"overwrite any key of the same name"* (`protocol.md:370-372`). So a
per-hit coordinate at the **top level** of a result would be silently clobbered by the machine's current
coordinate — a silent wrong answer of the exact shape D11 exists to prevent. The per-hit coordinate therefore
lives **inside** `hits[]`, which is the shape `checkpoint_list` already uses for its items' `frame`/`mclk`
(schema lines 361-367). Pinned as a test (§6.3).

---

## 5. What I would **not** add

Surface is a cost (D15), and the handoff's central finding is that documents propose far more than anyone
executes (`docs/2026-08-15-handoff-capability-layer.md:46-51`). Each exclusion names what would reverse it.

| | why not |
|---|---|
| **A per-frame value trace** | The handoff's ranked item 4 asks for one and this is not it (§1.4 item 4). A sampler reads an address once per frame whether or not anything wrote it; a watch is silent about an address nobody touched, and floods on one that is rewritten 60×/frame with the same value. Building this and *calling* it the value trace would be the ranking error the handoff itself measured. Register it separately, on its own evidence. |
| **A `watchHit` event** | §3. Two lossy stages would become coupled, and `droppedEvents` would stop meaning what D17 says it means. |
| **A second drop counter** | §3, §4.4. One ring, one `dropped`, in the body. |
| **`fc` / `size` / `addr_parity` filters** (`:329-353`) | Real core capability, zero bus consumers. All three exist for `examples/k4_openbus_probe.rs`, which runs in-process and would not be a bus client. Core itself records that the parity filter's motivating disjunction *"does not bind on this machine"* (`:414-421`). |
| **`WatchMode::Count` as an explicit mode** | Redundant on the wire: `matched` is counted *"in every mode, including the modes that store nothing"* (`:525-527`), so `record` with `limit: 0`, or simply not reading the hits, is a count. One fewer enum member to keep in sync. *Reverses if* a client hits memory pressure that only a store-nothing mode fixes — but the ring is the server's own `cap`, so the server fixes that. |
| **`take_hits` / any destructive read** | §4.2. Evidence theft between clients. |
| **`timingBasis` on the reply** | Core exposes `timing_basis()` (`:766-768`) and it is tempting to attach it to a trace. D16 puts it in the `initialize` result and §2.2 says why: *"it is a property of the machine, not of the answer"* (`protocol.md:381-382`). Repeating it per reply would be the eleventh undocumented key F4 warns about, for a value that cannot differ. |
| **Auto-clear on disconnect** | §4.4. A silent machine-state change. |
| **A `watchpoint_hits` push subscription / filter language** | Core's `matches` is *"a conjunction of optional filters, deliberately — not a predicate language"* (`:409-412`). Inventing one on the wire would out-run the core it wraps. |
| **`CensusKey::Pc`** | Worth naming because core's own justification for its 256 default cites *"390–516 distinct PCs"* (`:93-96`, citing `docs/2026-07-22-tf4-nextlayer-triage.md:138-139`) — **and no `CensusKey` variant groups by PC.** The cap's headline example is a census core cannot perform. Genuine finding, genuinely not my scope: registered here, not proposed, because it has no bus consumer and adding it would be ranking by how good the argument sounds. |
| **Breakpoints** (`breakpoint_add`/`list`/`clear`) | Catalogued, unimplemented, and adjacent enough to look like free scope. Ruling B says breakpoint-as-anchor is proven — but that is a separate design with a separate core seam (`System::run_until_stop`'s predicate is `(pc, frame)` only, `system.rs:924`), and the handoff ranks it as capability 3 in its own right. |

---

## 6. Implementation sketch

### 6.1 Where the shared instrument lives — the part that is specific to us

`oracle-aether` is an **optional** frontend dependency while the click path is unconditional
(`docs/2026-08-15-pixel-attribution-bus-method.md:551-554`), so nothing new may land in `oracle-aether` that
the player needs. Nothing does here: the capability is already in `oracle-core`.

The real constraint is that **there are two run drivers**, and this is the one thing a naive implementation
will get wrong. In the standalone server the engine drives the run itself
(`engine.rs:492`, `:506` — both already `Fanout` a sink). In the hosted arrangement the **player** owns the
loop and the engine only borrows the machine inside `Host::pump`, which answers queued commands and swaps
back (`host.rs:318-371`); the player's own per-frame run attaches its sinks at `main.rs:1271-1299`. A
`Watchpoints` owned by the engine would therefore see **nothing** while the player is running the machine —
`seen == 0`, correctly reported as "the instrument was never attached" (§4.4 item 4), which is honest but
useless.

So: the `Watchpoints` is **engine-owned** (so the standalone server works at all), and the host exposes it
for the player's `Fanout` to borrow, replacing the player's private instance when serving. That is also what
delivers D15's parity — the panel's `W` dump and the bus's `watchpoint_hits` then read *one* instrument, and
cannot drift.

### 6.2 Files

| file | change | ~lines |
|---|---|---|
| `crates/oracle-aether/src/engine.rs` | 4 `METHODS` rows; `watchpoints: Watchpoints` + a `WatchId ↔ handle` map on `Engine`; four handlers (parse, arm/clear, serialize reports and hits, cursor by `seq`); `capabilities.watchpoints` from `false` to the object; attach the sink to the engine's own runs alongside `screen` | ~340 |
| `crates/oracle-frontend/src/bus.rs` | `Host::watchpoints_mut() -> &mut Watchpoints` accessor | ~15 |
| `crates/oracle-frontend/src/main.rs` | arm/read through the host's instrument when serving, the private one otherwise; `dump_hits` prints the watch id (§1.5) | ~30 |
| `crates/oracle-core/src/watchpoints.rs` | **only if §4.6's `via` census is adopted**: one `CensusKey` variant, one `key_of` arm, one `describe` arm | ~8 / 0 |
| `crates/oracle-aether/tests/watchpoints.rs` | new | ~260 |
| `crates/oracle-core/tests/` | **no change** — this adds a bus surface over an existing core capability. No pinned literal moves, and that must stay true through review. | 0 |
| `empyrean/` | **owner's, on ruling.** Two §6 rows amended/added, two new rows, the schema fragments below, an §11.3 amendment entry. Not edited here. | — |

### 6.3 What the tests would pin

1. **The handle is a string, everywhere it appears** — `watchpoint_add`'s result, `watchpoint_list`'s items,
   `hits[].watch`, `watchpoint_clear`'s param. The one thing §8 item 16 records us as having got wrong once.
2. **`old` is present iff `space != "bus"`, and `fc` iff `space == "bus"`.** Ruling A's load-bearing pin: a
   bus hit must not report `old`, because core's `old` is unconditionally `0` there (`:850`).
3. **The write-count fallacy, as an executable test.** A fixture that writes the same value N times and a
   different value once: `matched == N+1` while a `census` over `value` reports `distinctKeys == 2`. This is
   the aeon lesson pinned in our own suite, so it cannot be re-learned.
4. **Cursor invariant under mutation.** Read page 1, arm another watch and run more frames, resume from the
   cursor: no live hit skipped, none delivered twice (§6.1's rule, `protocol.md:703-705`).
5. **Drop honesty.** Overflow a small ring: `dropped` moves, `seq` gaps appear, `matched > hits.len() +
   dropped` holds where non-`Record` watches absorbed accesses (`:749-751`).
6. **The negative control.** `seen == 0` before any run; `seen > 0, matched == 0` after a run with a watch on
   an address nothing touches.
7. **The stamp is not shadowed.** A hits reply's top-level `frame`/`mclk` are the machine's *now*, and the
   per-hit coordinates inside `hits[]` differ from them (§4.7).
8. **`stopped {reason:"watchpoint", watch}`** fires from a `stopAfter` watch inside a bounded `run_frames`,
   and the run ends at an instruction boundary with the triggering instruction committed (`:376-379`).
9. **Idempotent clear**: unknown handle → `removed: 0`, never `-32005` (§4.4).
10. **A retired handle is legible**: after a clear, its hits still carry the handle and it is absent from
    `watchpoint_list`.
11. **Parity, which is the whole point of item 19.** For one fixture run, the bus's `watchpoint_hits` and the
    player's `dump_hits` render from the same instrument and agree hit-for-hit.
12. **Schema conformance** of every reply, once §8 item 15's validator lands. Per the ruling's sequencing, the
    handler emits **exactly** the schematized keys — no eleventh instance of the wire probe's F4
    (`docs/2026-08-15-wire-conformance-probe.md:63-98`).

---

## 7. CR-11

Drafted in the CR-1…CR-10 house style of `docs/2026-08-14-aether-change-requests.md`. **Not sent, not applied
to the contract repo.**

### CR-11 — a catalogued watchpoint cannot name a space, a range, a mode or itself

**Contract.** §6's *breakpoints & watchpoints* section (`protocol.md:568-574`) carries one watch row:
`emulator/watchpoint_add | addr|symbol, read?, write? | addr`. §8 item 19 requires a bus method **and** a
schema entry before the panel that renders the capability. D15: *"A capability that exists only inside a
panel is the `list_ops` drift of §0 re-created in pixels."*

**The gap.** Three things that row cannot express, all of them already done by our own player:

1. **Space.** `crates/oracle-frontend/src/main.rs:919-931` arms `add_vdp_watch(space, lo..=hi, WatchOp::Write,
   label)` over `WatchSpace::Vram` / `Cram`. The catalogued params carry no space, so a bus client cannot ask
   for the *"who wrote this tile?"* watch at all. Core supports four spaces
   (`crates/oracle-core/src/watchpoints.rs:146-155`).
2. **Range.** The panel arms a 32-byte pattern and an 8-byte SAT entry (`pick.rs:126-145`); the row takes one
   address.
3. **Identity.** The result echoes the input address, so nothing names the watch. One address can carry
   several watches (`watchpoints.rs:477-479`) and a click arms two at once, so an address cannot be the
   handle. Core already issues a never-reused id for exactly this reason (`watchpoints.rs:167-173`), which is
   D9 category 4's shape verbatim.

And one thing the row leaves **ambiguous**, which two conformant servers could resolve oppositely: whether a
watchpoint *halts* the machine. The classical reading is break-on-access. Our core's default is
`WatchMode::Record` — observe, never halt (`watchpoints.rs:62`, `:82-83`) — with an explicit opt-in
`stop_after(n)` that ends the run at the next instruction boundary (`:369-379`). The project's own record
draws the same line: *"breakpoint-as-deterministic-anchor is proven; breakpoint-as-interactive-session is
proven harmful"* (`docs/2026-08-15-handoff-capability-layer.md:116-117`), against a recorded
**1,691,410-hit stale breakpoint contaminating later captures**
(`docs/2026-08-14-aether-change-requests.md:444-447`).

Note also that §3's `stopped` enum already contains `watchpoint` (`protocol.md:411`) and **no catalogued
method can produce it**.

**What we did.** Consumed the capability in-process from the player (`main.rs:869`, `:919-931`, `:942-955`)
and from a core example (`crates/oracle-core/examples/watch_probe.rs:121-127`), and shipped no bus surface.
`crates/oracle-aether/src/engine.rs:607` advertises `"watchpoints": false`. Recorded here rather than worked
around.

**Proposed change.**

1. Amend §6's watch row and add its inverse:

| Method | params | result |
|---|---|---|
| `emulator/watchpoint_add` | `space`? (`bus`\|`vram`\|`cram`\|`vsram`, def `bus`), `addr`\|`symbol`, `len`? (≥1, def 1), `read`?, `write`?, `mode`? (`record`\|`census`, def `record`), `censusKey`? , `stopAfter`? (≥1), `label`? | **`watch`** (str), `space`, `addr`, `len`, `op`, `mode`, `label`? |
| `emulator/watchpoint_clear` | `watch` (str)\|`all` | `removed` |

2. Add to §6 prose (D14: the schema governs shapes, prose governs behaviour):
   - **A watch records; it halts only when `stopAfter` is given.** With `stopAfter: n` the run ends at the
     next instruction boundary once the watch has matched `n` accesses, emitting `emulator/stopped` with
     `reason: "watchpoint"` — the enum member §3 already defines — and an additive `watch` param naming the
     cause. Without it a watch never affects when the machine stops.
   - **A watch is an opaque handle** (D9 category 4), server-assigned and never reused, so a stale handle
     resolves to nothing rather than to a different watch.
   - **`symbol` is valid only for `space: "bus"`**; a symbol with any other space is `-32602`.
   - **Neither `read` nor `write` given means `write`** — the recorded purpose of this instrument is *"who
     wrote this?"*, and both executed consumers default to write (`main.rs:928`, `watch_probe.rs` header).
     Both `true` means any access; the 68000 TAS matches a write watch and not a read one
     (`watchpoints.rs:104-107`).
   - **Arming and clearing are not subject to the run-control state rule** — they mutate an observer, not the
     timeline.
   - **`watchpoint_clear` of an unknown handle succeeds with `removed: 0`**, for §6.1's stated reason:
     deletion is idempotent, and only a `restore`-shaped operation refuses an unknown handle.
   - **Clearing does not delete hits already recorded.** They keep naming the handle that recorded them,
     which is how a client tells a cleared watch's evidence from a live one's (CR-12).

3. Advertise `capabilities.watchpoints` as an **object**, for D13's reason applied to the same shape — a
   client that has to hit a limit to learn it is a client that loses evidence finding out:
   `{supported, spaces[], maxWatches, ringCap}`.

> **Condition 3 (advertising a cap is half a rule).** `maxWatches` is advertised above with **no behaviour
> at the cap**, and core's spec list is an unbounded `Vec` — so a server could satisfy the advertisement by
> ignoring it. **D13 rule 3 verbatim:** refuse at the cap with `-32005` carrying
> `{"reason":"watchCapReached","cap":n,"count":n}`; never silently grow past the advertised number, never
> silently evict. The reason is sharper here than for checkpoints: a silently-dropped watch produces a
> `seen`-positive, `matched`-zero reading, which is indistinguishable from a genuine negative finding — the
> one failure §4.4 item 4 exists to make impossible.
>
> **Condition 4 (`censusKey` must not be silently ignored).** The fragment below reads *"Required when mode
> is 'census', **ignored otherwise**"*, which is against §5's refuse-and-name ethos: a param a bus quietly
> discards is a caller believing it asked for a grouping it did not get. It is **`-32602`**, enforced by an
> `if`/`then` — the same device this document already uses for `old`/`fc` — and in both directions, since
> `mode: "census"` with no key has nothing to group by either.
>
> Both landed in `empyrean` `af434a2`, and §8 item 21 lists them for the reason item 19 exists.

**Schema fragment**, ready to paste under `methods`. **The version that landed is the contract's**
(`empyrean` `af434a2`), which differs from the draft below by conditions 1–5; the draft is left as written
so the difference stays visible:

```json
    "emulator/watchpoint_add": {
      "$comment": "protocol.md §6 (breakpoints & watchpoints). Arms a RECORDING watch: it observes and does not halt unless `stopAfter` is given, in which case the run ends at the next instruction boundary and `emulator/stopped` fires with reason 'watchpoint'. Not subject to §6's run-control state rule — it mutates an observer, not the timeline.",
      "params": {
        "type": "object",
        "oneOf": [{ "required": ["addr"] }, { "required": ["symbol"] }],
        "properties": {
          "space": {
            "enum": ["bus", "vram", "cram", "vsram"],
            "default": "bus",
            "description": "Which address space. 'bus' is the 68000 address space (work RAM, ROM, Z80 RAM, I/O, VDP ports); the other three are VDP-INTERNAL byte-address spaces — the 'who wrote this tile / palette entry?' watch. Spaces never cross-trigger: a numeric collision between a bus address and a VRAM byte address matches only the watch in its own space."
          },
          "addr": { "$ref": "#/$defs/hex", "description": "First address of the watched range, inclusive. D9 category 1." },
          "symbol": { "type": "string", "description": "Resolved to an address (D7). Valid ONLY with space 'bus' — a symbol names a 68000 address, and a VDP-internal byte address has no symbol. Any other space with `symbol` is -32602." },
          "len": { "type": "integer", "minimum": 1, "maximum": 16777216, "default": 1, "description": "Length of the watched range in bytes; the range is addr..=addr+len-1. A count, so a JSON number (D9 category 2). Default 1 reproduces the single-address call this row carried before this amendment." },
          "read": { "type": "boolean", "description": "Match reads." },
          "write": { "type": "boolean", "description": "Match writes. Neither given means write-only: the recorded purpose of this instrument is 'who wrote this?'. Both true means any access. A write watch also matches the 68000 TAS (its read-modify-write store); a read watch does not." },
          "mode": {
            "enum": ["record", "census"],
            "default": "record",
            "description": "'record' stores each matched access in the server's bounded hit ring. 'census' stores NOTHING and instead groups matched accesses by one key, which is what turns a watch over a wide range from a context bomb into a number. Both modes count `matched` either way."
          },
          "censusKey": {
            "enum": ["addr", "value", "via"],
            "description": "Required when mode is 'census', ignored otherwise. 'value' is the distinct-value census: a WRITE COUNT IS A MISLEADING PROXY for how much a value moves, and this is the aggregate that survives that (protocol.md §6 prose). 'addr' answers which addresses in the range are actually touched. 'via' answers CPU-vs-DMA on a VDP-internal watch, which a function-code census cannot (there is no bus function code on a VDP-internal write)."
          },
          "stopAfter": { "type": "integer", "minimum": 1, "description": "Halt the run at the next instruction boundary once this watch has matched N accesses, emitting emulator/stopped with reason 'watchpoint'. ABSENT means the watch never affects when the machine stops. This is stop-on-condition, not an interactive break: the triggering instruction has fully committed when the run ends, and the run was already bounded by its own caller." },
          "label": { "type": "string", "description": "Optional human string, carried back verbatim and never interpreted." }
        }
      },
      "result": {
        "allOf": [{ "$ref": "#/$defs/replyFields" }],
        "required": ["watch", "space", "addr", "len", "op", "mode"],
        "properties": {
          "watch": {
            "allOf": [{ "$ref": "#/$defs/handle" }],
            "description": "The watch handle. Server-assigned, NEVER REUSED, and an opaque string per D9 category 4 — not an address and not an index. It cannot be an address: one address may carry several watches, and the same number names four different things across the four spaces. A client hands it back to watchpoint_clear and reads it off every hit."
          },
          "space": { "enum": ["bus", "vram", "cram", "vsram"] },
          "addr": { "$ref": "#/$defs/hex" },
          "len": { "type": "integer", "minimum": 1 },
          "op": { "enum": ["read", "write", "any"], "description": "The resolved op filter — what `read`/`write` actually became, so a caller that supplied neither is told what it got." },
          "mode": { "enum": ["record", "census"] },
          "censusKey": { "enum": ["addr", "value", "via"] },
          "stopAfter": { "type": "integer", "minimum": 1 },
          "label": { "type": "string" }
        }
      }
    },
    "emulator/watchpoint_clear": {
      "$comment": "protocol.md §6. The inverse of watchpoint_add, and the anti-staleness half of this surface: without it an armed watch outlives the session that wanted it. Idempotent — an unknown handle succeeds with removed: 0, per §6.1's rule for checkpoint_drop. Hits already recorded are NOT deleted; they keep naming their handle, and a handle absent from watchpoint_list is a retired watch.",
      "params": {
        "type": "object",
        "oneOf": [{ "required": ["watch"] }, { "required": ["all"] }],
        "properties": {
          "watch": { "$ref": "#/$defs/handle" },
          "all": { "type": "boolean" }
        }
      },
      "result": {
        "allOf": [{ "$ref": "#/$defs/replyFields" }],
        "required": ["removed"],
        "properties": { "removed": { "type": "integer", "minimum": 0 } }
      }
    }
```

And in `handshake.initialize.result.capabilities`:

```json
              "watchpoints": {
                "type": "object",
                "required": ["supported"],
                "properties": {
                  "supported": { "type": "boolean" },
                  "spaces": { "type": "array", "items": { "enum": ["bus", "vram", "cram", "vsram"] }, "description": "Which address spaces this server can watch. Advertised rather than assumed: a server with no VDP-internal write capture supports only 'bus', and a client must not have to arm a watch to find out." },
                  "maxWatches": { "type": "integer", "minimum": 1 },
                  "ringCap": { "type": "integer", "minimum": 0, "description": "Capacity of the hit ring, in hits. Discoverable BEFORE a client plans around it, for D13's reason: past this the oldest hits are dropped (and counted by watchpoint_hits.dropped), so a client sweeping a hot range needs the number in advance, not after losing evidence to it. 0 is legal and means record nothing — a pure census configuration." }
                }
              }
```

---

## 8. CR-12

### CR-12 — nothing on this bus can read what a watchpoint observed

**Contract.** §6 has `breakpoint_list` (`breakpoints[]{addr,enabled,hits}`) and no watch equivalent. The one
watch row returns `addr`. §8 item 19 requires the capability on the bus before the panel. D17 is the
precedent for the specific honesty at stake: *"Event loss is counted and reported, never silent"*
(`protocol.md:254`).

**The gap.** Two capabilities our player renders that no catalogued method can return:

1. **The hits.** `crates/oracle-frontend/src/main.rs:450-468` prints, per hit, `seq`, `frame`, `pc`
   (symbolised at `:461`), `addr`, `old->value` and `via` — the CPU-vs-DMA attribution. **No catalogued
   method returns a watchpoint hit at all**, in any shape.
2. **The drop count.** `main.rs:469` prints `watchpoints.dropped()`. The hit log is a bounded drop-oldest ring
   (`crates/oracle-core/src/watchpoints.rs:640-654`) and its loss is currently visible only to whoever is
   sitting at the player's terminal. That is D17's principle — a bounded queue that discards is correct, the
   *silence* is not — applied to a second lossy stage that has no wire counterpart.

Note the scope difference, because it decides where the field goes: `droppedEvents` is a **connection** fact
(§2.3), while this is an **instrument** fact — one number, identical for every client, about loss inside the
machine's recorder. It belongs in the result **body**, and this CR proposes no change to the envelope.

**What we did.** Rendered both to the player's terminal and its on-screen toast (`main.rs:942-949`), and put
neither on the wire.

**Proposed change.** Two rows in §6's *breakpoints & watchpoints* table:

| Method | params | result |
|---|---|---|
| `emulator/watchpoint_list` | `cursor`? (str), `limit`? | `watches[]{watch,label?,space,addr,len,op,mode,censusKey?,stopAfter?,matched,first?,last?,census?,distinctKeys?,keyCap?,keysCapped?,censusOverflow?}`, `cursor`? (str), `truncated` |
| `emulator/watchpoint_hits` | `watch`? (str), `cursor`? (str), `limit`? (1–4096, def 100) | `hits[]{watch,space,addr,value,old?,size,op,fc?,via,pc,symbol?,symbolDisp?,frame,mclk,seq}`, `cursor`? (str), `truncated`, `dropped`, `seen`, `matched`, `caveats[]` |

Semantics for §6 prose:

- **Reads, not waits.** Neither runs the machine and neither requires a paused one. Hits are **polled, never
  pushed**: a per-hit event would route one bounded lossy stage into another and make `droppedEvents` move
  for reasons that have nothing to do with the client's ability to keep up, degrading the very signal D17
  defines. The one push this capability needs already exists — `stopped {reason:"watchpoint"}` (CR-11).
- **`dropped` is loss at *record* time**, distinct from `truncated` (loss at read time) and from
  `droppedEvents` (loss on the event channel). All three may be non-zero at once and they mean three
  different things.
- **`seen` is the structural negative control**, and is REQUIRED. It counts every access offered to the
  recorder, matched or not: `seen > 0, matched == 0` is a live instrument that found nothing; `seen == 0` is
  an instrument that was never attached, and a zero from it means nothing at all. Without it a client cannot
  distinguish "this address is never written" from "the recorder was not in the run".
- **`old` is present if and only if `space` is not `bus`**, and **`fc` if and only if it is**. The bus event
  stream carries no prior value, so a bus hit has no `old` to report; a VDP-internal write has no bus
  function code, and its CPU-vs-DMA attribution is `via`. Reporting either as a zero would be a silent wrong
  answer.
- **A write count is not a measure of change**, and this is normative guidance rather than a shape: a driver
  that re-asserts registers every frame produces a large `matched` and almost no movement. Where the question
  is "how much does this value move", the answers are the per-hit `old`→`value` pair (VDP spaces) or a
  `census` over `value`; `matched` alone answers "how often was it written", never "did it change".
- **Cursors** obey §6.1's invariant. A hit dropped by the ring between two requests was not live at both, so
  the invariant is not violated by it; `dropped` moving is how the client learns.
- **A hit's `frame`/`mclk` are inside `hits[]`, never at the top level**, where the envelope stamp (§2.2)
  would overwrite them.

> **Condition 1, applied to the two rows above.** Both result shapes carry `truncated` alone; §2.4 clause
> (a) requires **`total` and `returned`** beside it and permits an optional `limit` echo, and the contract's
> rows carry all four. The `caveats[]` in `watchpoint_hits`'s row is gone — see §1.2 for where its four
> members went. And **the cursor invariant these bullets cite as "§6.1's" now lives in §2.4 clause (c)**,
> moved there whole by §11.5; the ruling document carries the full recomputed anchor table.
>
> One drafting point worth keeping visible: the step-granular-`mclk` note the `hits[].mclk` description
> below defers to `caveats` is a **permanent property of a VDP-space hit**, and permanent properties do not
> belong in a per-reply warning. It is §6 prose now, and the landed description says so in place of the
> deferral.

**Schema fragment**, ready to paste under `methods`. As above, **the contract's version is the one that
landed**; this draft is left un-rewritten:

```json
    "emulator/watchpoint_list": {
      "$comment": "protocol.md §6. The armed instrument, with what each watch has observed. Bounded and cursored like every list on this bus. A pure read: not subject to §6's run-control state rule. A handle that appears on a hit but not here is a watch that has been cleared — ids are never reused, so that test cannot give a false negative.",
      "params": {
        "type": "object",
        "properties": {
          "cursor": { "allOf": [{ "$ref": "#/$defs/handle" }], "description": "Continuation handle from a previous call. Opaque (D9 category 4); its behaviour is normative (protocol.md §6.1), its representation is the server's." },
          "limit": { "type": "integer", "minimum": 1 }
        }
      },
      "result": {
        "allOf": [{ "$ref": "#/$defs/replyFields" }],
        "required": ["watches", "truncated"],
        "properties": {
          "watches": {
            "type": "array",
            "items": {
              "type": "object",
              "required": ["watch", "space", "addr", "len", "op", "mode", "matched"],
              "properties": {
                "watch": { "$ref": "#/$defs/handle" },
                "label": { "type": "string" },
                "space": { "enum": ["bus", "vram", "cram", "vsram"] },
                "addr": { "$ref": "#/$defs/hex" },
                "len": { "type": "integer", "minimum": 1 },
                "op": { "enum": ["read", "write", "any"] },
                "mode": { "enum": ["record", "census"] },
                "censusKey": { "enum": ["addr", "value", "via"] },
                "stopAfter": { "type": "integer", "minimum": 1, "description": "Present only when this watch will halt the run. A watch left armed with this set is the one that can change a later capture's outcome, so it is listed rather than left to be discovered." },
                "matched": { "type": "integer", "minimum": 0, "description": "Accesses this watch matched, counted in EVERY mode including the ones that store nothing. A count of writes, NOT a measure of how much the value changed — see the census below and protocol.md §6." },
                "first": { "$ref": "#/$defs/watchStamp" },
                "last": { "$ref": "#/$defs/watchStamp" },
                "census": {
                  "type": "array",
                  "description": "key -> count, ascending by key. Present only when mode is 'census'. The bounded group-by that makes a wide watch survivable, and — over censusKey 'value' — the aggregate that a raw write count misleads about.",
                  "items": {
                    "type": "object",
                    "required": ["key", "count"],
                    "properties": {
                      "key": { "type": "integer", "minimum": 0, "description": "The grouped value: an address, a written value, or 0=bus/1=direct(CPU)/2=dma for censusKey 'via'." },
                      "count": { "type": "integer", "minimum": 0 }
                    },
                    "additionalProperties": false
                  }
                },
                "distinctKeys": { "type": "integer", "minimum": 0, "description": "How many distinct keys the census retained. A LOWER BOUND when keysCapped is true — never read distinctKeys == keyCap as an exact answer." },
                "keyCap": { "type": "integer", "minimum": 0, "description": "The configured cap, so a reader can tell distinctKeys AT the cap from distinctKeys under it." },
                "keysCapped": { "type": "boolean", "description": "The census refused at least one new key because it was at its cap. Known keys keep counting past the cap; only new ones are refused." },
                "censusOverflow": { "type": "integer", "minimum": 0, "description": "Accesses carrying a key the capped census could not retain. Counted, never silently dropped." }
              },
              "additionalProperties": false
            }
          },
          "cursor": { "allOf": [{ "$ref": "#/$defs/handle" }], "description": "Present when more remain." },
          "truncated": { "type": "boolean" }
        }
      }
    },
    "emulator/watchpoint_hits": {
      "$comment": "protocol.md §6. The recorded hit log — the 'who wrote this?' answer. POLLED, never pushed: a per-hit event would feed one bounded lossy stage (the ring) into another (the event queue), and moving droppedEvents for recorder volume would degrade the signal D17 defines for stopped/romReloaded. A pure read: not subject to §6's run-control state rule, and non-destructive, so two clients on one bus cannot steal each other's evidence.",
      "params": {
        "type": "object",
        "properties": {
          "watch": { "allOf": [{ "$ref": "#/$defs/handle" }], "description": "Return only hits recorded by this watch. Absent means every watch — the ring is one shared, seq-ordered stream." },
          "cursor": { "allOf": [{ "$ref": "#/$defs/handle" }], "description": "Continuation handle. Opaque (D9 category 4). A hit dropped from the ring between two calls was not live at both requests, so resuming past it does not violate §6.1's invariant; `dropped` moving is how a client learns it happened." },
          "limit": { "type": "integer", "minimum": 1, "maximum": 4096, "default": 100 }
        }
      },
      "result": {
        "allOf": [{ "$ref": "#/$defs/replyFields" }],
        "required": ["hits", "truncated", "dropped", "seen", "matched", "caveats"],
        "properties": {
          "hits": {
            "type": "array",
            "description": "Oldest first. Each hit is attributed to the instruction that drove it (pc, from the step boundary) and to the master that drove it (fc on the bus, via for a VDP-internal write).",
            "items": {
              "type": "object",
              "required": ["watch", "space", "addr", "value", "size", "op", "via", "pc", "frame", "mclk", "seq"],
              "properties": {
                "watch": { "allOf": [{ "$ref": "#/$defs/handle" }], "description": "The watch that recorded this hit. REQUIRED: the ring is shared, so without it a log spanning two watches is uninterpretable — and a handle absent from watchpoint_list marks a hit recorded by a watch that has since been cleared, which is how a stale instrument's evidence stays distinguishable from a live one's." },
                "space": { "enum": ["bus", "vram", "cram", "vsram"] },
                "addr": { "$ref": "#/$defs/hex" },
                "value": { "$ref": "#/$defs/hex", "description": "The value read or written (for a VDP-internal write, the NEW value)." },
                "old": { "$ref": "#/$defs/hex", "$comment": "The value that was there before the access. Present IF AND ONLY IF space is not 'bus': the 68000 bus event stream carries no prior value, so a bus hit has none to report and emitting 0x0 would assert something false. Where present, old != value is the exact per-write change test — the measurement a raw write count misleads about." },
                "size": { "type": "integer", "enum": [1, 2, 4], "description": "Access width in bytes. A count (D9 category 2)." },
                "op": { "enum": ["read", "write", "tas"], "description": "'tas' is the 68000 read-modify-write store; it matches a write watch and not a read one." },
                "fc": { "type": "integer", "minimum": 0, "maximum": 7, "description": "68000 function code of the access (5/6 = CPU supervisor data/program, 0 = a non-CPU master). Present IF AND ONLY IF space is 'bus' — a VDP-internal write has no bus function code, and its CPU-vs-DMA attribution is `via`." },
                "via": { "enum": ["bus", "direct", "dma"], "description": "How the access reached its target. 'bus' = a 68000 bus access. 'direct' = a CPU data-port write into VDP memory; 'dma' = a DMA step, attributed to the instruction that TRIGGERED the transfer. This is the CPU-vs-DMA answer for VDP-internal writes, which no function code can give." },
                "pc": { "$ref": "#/$defs/hex", "description": "PC of the instruction that drove the access, stamped at the step boundary. One instruction driving several accesses (a MOVEM, a read-modify-write) attributes them all to itself." },
                "symbol": { "type": "string", "description": "Nearest preceding label for `pc`, when symbols are loaded (D7). An annotation on the address, never a replacement for it." },
                "symbolDisp": { "type": "integer", "minimum": 0 },
                "frame": { "type": "integer", "minimum": 0, "description": "Emulated frame of THIS HIT — deliberately inside the hit and never at the top level of the result, where the envelope stamp (§2.2) would overwrite it with the machine's current coordinate." },
                "mclk": { "type": "integer", "minimum": 0, "description": "Emulated master clock of this hit. STEP-GRANULAR for a VDP-internal hit (the write is drained after the driving CPU step, so it carries that step's clock) — reported in `caveats` whenever any such hit is present, rather than dressed up as precision." },
                "seq": { "type": "integer", "minimum": 0, "description": "Monotonic id of the matched access, assigned in order and STABLE ACROSS RING DROPS — so a gap in seq marks hits the ring discarded. A count (D9 category 2); the cursor is the server's business and is not this number." }
              },
              "additionalProperties": false,
              "allOf": [
                {
                  "$comment": "The two presence rules, enforced mechanically rather than left to prose. A bus hit has no prior value to report (the 68000 bus event stream carries none) and a VDP-internal write has no bus function code; emitting either as a zero would assert something false, which is the silent-wrong-answer class this bus exists to prevent. D14 makes the schema the authority on which keys exist, so the rule belongs here and not only in §6.",
                  "if": { "properties": { "space": { "const": "bus" } } },
                  "then": { "required": ["fc"], "not": { "required": ["old"] } },
                  "else": { "required": ["old"], "not": { "required": ["fc"] } }
                }
              ]
            }
          },
          "cursor": { "allOf": [{ "$ref": "#/$defs/handle" }], "description": "Present when more remain." },
          "truncated": { "type": "boolean", "description": "Loss at READ time — more hits are held than this page returned. Distinct from `dropped`." },
          "dropped": { "type": "integer", "minimum": 0, "description": "Hits the recorder discarded at RECORD time because its ring was at capacity (oldest first). An INSTRUMENT fact, identical for every client — unlike droppedEvents (§2.3), which is a per-connection fact — which is why it rides in the body and not in the envelope. D17's principle, applied to the second lossy stage on this bus: a bounded ring that discards is correct, the silence is not." },
          "seen": { "type": "integer", "minimum": 0, "description": "THE STRUCTURAL NEGATIVE CONTROL. Every access offered to the recorder, matched or not. seen > 0 with matched == 0 is a live instrument that found nothing; seen == 0 is an instrument that was never attached to the run, and a zero from it means nothing at all. REQUIRED so a client cannot mistake the second for the first." },
          "matched": { "type": "integer", "minimum": 0, "description": "Accesses that matched at least one watch, across all modes. matched - hits returned - dropped is what the non-recording watches absorbed. A count of accesses, not a measure of change." },
          "caveats": {
            "type": "array",
            "items": { "type": "string" },
            "description": "Caveats that must travel WITH the numbers rather than in documentation beside them — precise-looking figures are over-trusted. Empty when none applies. REQUIRED so that absence is an assertion the server made, not a field it forgot."
          }
        }
      }
    }
```

Plus one shared `$def`, used by `first` and `last` above:

```json
    "watchStamp": {
      "type": "object",
      "required": ["pc", "frame", "mclk", "seq"],
      "properties": {
        "pc": { "$ref": "#/$defs/hex" },
        "frame": { "type": "integer", "minimum": 0 },
        "mclk": { "type": "integer", "minimum": 0 },
        "seq": { "type": "integer", "minimum": 0 }
      },
      "additionalProperties": false,
      "description": "A deterministic emulated coordinate for one matched access. The same shape a stop record uses, so a stop and a hit name the same kind of point. Never wall-clock. Nested rather than top-level: the envelope stamp (§2.2) overwrites same-named keys at the top level of a result."
    }
```

### 8.1 The seam, and why it is drawn there

The two CRs split on **what a watch is** versus **what a watch tells you**, and that line maps exactly onto
the three capabilities the sweep found:

| sweep's capability | CR |
|---|---|
| 3. VDP-internal-space watches | **CR-11** |
| 1. Reading the hits | **CR-12** |
| 2. The drop count | **CR-12** |

Three further reasons the seam is there rather than anywhere else:

1. **They are different kinds of contract change.** CR-11 **amends an existing catalogued row** (and pins an
   ambiguity in it about halting) and adds that row's inverse. CR-12 adds two **new query rows**. Mixing an
   amendment with new rows in one CR makes the ruling coarser than it needs to be — the owner may want to
   adopt the `space`/handle change and rule differently on how much observation surface lands.
2. **Each CR owns exactly one of the two rulings.** Ruling B (the interactive-debugging line, and the
   staleness lifecycle) is entirely inside CR-11 — `stopAfter`, `watchpoint_clear`, the record-vs-halt
   pinning. Ruling A (write counts versus value changes) is entirely inside CR-12's prose and presence rules,
   with only the `censusKey` param reaching back into CR-11. A reader can rule on one without re-deriving the
   other.
3. **D9 category 4 lands wholly in CR-11.** The handle's type, its non-reuse and why it cannot be an address
   are one argument, made once, in the CR that introduces it. CR-12 only consumes it.

**They should be adopted together or not at all**, and that is stated rather than left to be discovered:
CR-11 alone leaves the largest half of the violation open (a client could arm a watch it can never read,
which is a strictly worse surface than none), and CR-12 alone can only read watches nobody can arm in a VDP
space. ~~If exactly one is adopted, CR-12 is the one that closes the item-19 violation the sweep called
largest; CR-11 is the one that makes the capability match what our own panel already does.~~

> **Condition 5 — the struck sentence.** It contradicts the package rule stated two sentences above it, and
> the package rule is **structural** rather than a preference: CR-12's `hits[].watch` and its `watch`
> filter consume the D9-category-4 handle that only **CR-11** introduces, so "CR-12 alone" is not a
> reachable state. Offering a fallback the argument rules out invites exactly the partial adoption the
> paragraph exists to prevent. *(The ruling attributed this sentence to the CR register; it is here, in
> this document — recorded in the ruling doc rather than silently corrected.)*

### 8.2 The fragments were executed, not just written

Both CRs' fragments were spliced into the real `schema/bus-protocol.schema.json` (in a scratch copy — the
contract repo was not touched) and driven against `jsonschema` 4.26 draft-2020-12: **9 methods → 13**, the
whole spliced document still a legal draft-2020-12 schema, all four method names satisfying D3's request
pattern, and **39 expectations, all met** — 15 accept cases and 24 refusals. The refusals include the ones
that matter most:

- a **numeric** `watch` handle (the §8 item 16 mistake, refused at both places it appears);
- a reply missing `droppedEvents`, and a `watchpoint_hits` result missing `seen`, `dropped` or `caveats`;
- a hit that does not name its watch (§1.5's contamination vector);
- an unknown key inside a hit — the F4 guard, so the handler cannot quietly grow an eleventh undocumented
  key;
- a bare-number address, an impossible access width, an unknown `space`/`via`/`censusKey`, `len: 0`,
  `stopAfter: 0`, a partial `watchStamp`.

**The run earned its keep once**, the same way CR-10's did. Ruling A's two presence rules — `old` only off
the bus, `fc` only on it — were prose-only in the first draft, and the executed check showed the schema
happily accepting a bus hit carrying `old: "0x0"`: the exact silent-wrong-answer the ruling exists to
prevent, admitted by the artifact D14 makes authoritative over exactly this question. They are now an
`if/then/else` inside the hit item, mechanically enforced in both directions, with four new cases pinning it.
This is §11.2's method-name-pattern lesson at small scale for the second time in this arc.

Reproduced by `scratchpad/check_wp_schema.py` in this session's scratchpad, which extracts the fragments from
this document itself rather than from a copy — a throwaway instrument, deliberately outside the repo. The
durable version is §8 item 15's in-tree validator.

> **Re-executed on adoption, against the committed contract and in both validators.** The rebased fragments
> were spliced into the real `contract/schema/bus-protocol.schema.json` and driven through `jsonschema` 4.26
> (Python) **and** `jsonschema` 0.49 (Rust — the crate `crates/oracle-aether/tests/common/schema.rs` uses,
> compiled the same way that harness compiles fragments). **22 → 26 methods**, the whole document still a
> legal draft 2020-12 schema, all four names passing D3's request pattern, all eleven new subschemas
> compiling **open and closed**, and **72 cases met — 24 accept / 48 refuse**.
>
> **The run earned its keep a second time, and differently.** The `caveats[]` → `caveat` collapse looked
> complete until it was executed: the **published** fragment still *accepts* a stray `caveats` array, and
> refuses it only under §8 item 20's test-time `unevaluatedProperties: false`. That is D5 working as
> intended — the published artifact is deliberately open so a stale vendored schema cannot reject a
> conformant server — but it means the closure case has to be *in the case list* or the collapse is
> untested. Two closed-mode refusals and five closed-mode accepts were added for exactly that, and §11.6's
> rule is the one they enforce: **a registration is done when a conformant reply passes its closed
> fragment.**

---

## 9. Sequencing

Per the ruling (`docs/2026-08-15-fable-ruling-attribution.md:56-59`) the order is **validator → CR-10 → this
pair**. Nothing here jumps that queue: this document raises two change requests and edits no contract file.
When they are ruled on, the §6 rows and schema fragments land in `empyrean` **before** any handler, per §8
and §11.1's *"the discipline worth keeping is the sequencing — write the contract first, implement second."*
And per the ruling's fourth condition, the handler must emit **exactly** the schematized keys: the wire
probe's F4 found ten existing methods emitting result keys documented nowhere
(`docs/2026-08-15-wire-conformance-probe.md:63-98`), and every key proposed above is in this document.

> **Done, 2026-08-15.** Rows and fragments landed in `empyrean` `af434a2` (§11.8); CR-9's §3 redefinition
> landed separately in `8adf219` (§11.7), because it changes `stopped` for *every* stop-shaped method and
> earns its own line in the permanent record. **The handler is the next pass** — condition 8 — and it lands
> in `crates/oracle-aether` only, with `crates/oracle-core/tests/` untouched as §6.2's table promises. §8
> item 20's closure is already live in this repo's harness, so the "exactly the schematized keys" rule
> above is now a gate rather than an intention.
