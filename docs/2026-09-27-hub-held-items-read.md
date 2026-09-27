# Three hub-held items, read by oracle (2026-09-27)

The hub (empyrean) held three items until oracle read them. Oracle is the only Aether server and the only
lane that vendors the contract schema. This file answers each one. **Nothing here changes code.** The oracle
tree read is this worktree at `a51dd3b`. That commit is `main` minus one lane-status line (`a140928`), and
that line touches no source.

Sources were read through git objects, never through sibling working trees:

| Source | Ref | Last-touching commit |
|---|---|---|
| empyrean `docs/2026-09-17-event-readiness-premise.md` | `origin/main` = `8684c9b8` | `d16e4248` |
| empyrean `docs/2026-09-18-first-present-refusal-premise.md` | same | `b9bc2070` |
| empyrean `contract/protocol.md` | same | `290f8c94` |
| empyrean `contract/schema/bus-protocol.schema.json` | same | `616c2026` (oracle's vendored pin, identical) |
| empyrean `docs/OVERSEER-LOG.md` (the hub's ruling) | same | ruling at `bde5b1b3` |
| empyrean `docs/OVERSEER-PROTOCOL-REFERENCE.md` (clause accepted) | same | acceptance at `2258736f` |
| sigil `docs/superpowers/notes/2026-09-18-deb2-phase-symbol.md` | `origin/master` (sigil has no `main`) | `1bfce22f` |
| sigil `docs/superpowers/notes/2026-09-12-deb2-minus-20-bytes.md` | same | `19e12c99` |

⚑ **The brief said `sigil origin/main`, but that ref does not exist.** Sigil's default branch is `master`,
so every sigil read here is at `origin/master`.

---

## 1. F-EVENTS-BEGIN-UNSTATED: **AGREE-WITH-CORRECTIONS**

### What the draft claims about oracle, checked against the tree

| Draft claim | In oracle today | Holds? |
|---|---|---|
| oracle flips `ready` when it **processes** `initialized`. A duplicate is idempotent, and `initialized` before `initialize` is a hard error (cited as `session.rs:79-87`). | `crates/oracle-aether/src/session.rs:79-91`: same logic, and the cite has drifted by 4 lines. The `Subscribe` action it returns is honoured at `server.rs:762-768`, where `subs.add` runs on the connection's reader thread. | TRUE |
| Dispatch is "read as sequential per connection" but was not measured. | There is one reader thread per connection (`server.rs:676-806`). It reads one line and handles it completely before it reads the next. `subs.add` runs synchronously inside the `Subscribe` arm (`:766`). A later request goes to the engine over an mpsc channel (`:784-796`), which the engine can only receive after the subscription is registered. `Subscribers::broadcast` takes the same mutex as `add` (`outbound.rs:192-208`). The hosted players use the same `connection_loop` (`host.rs:299` → `server.rs:425`). **This was also measured.** In `c6ed909` (2026-09-13), a 300 ms delay placed *after* `subs.add` stayed green 20/20. The same delay placed *before* `subs.add` was red 20/20. | TRUE, **and measured** |
| `droppedEvents` cannot see events from before subscription. | The counter is `Inner::dropped`, which only `Outbound::push_event` increments (`outbound.rs:79-92`). An unsubscribed connection is not in `Subscribers`, so `broadcast` never touches its queue. The loss was measured in `c6ed909`: *"Nothing was dropped, so there was no droppedEvents count either"*. The `events.rs` dead-subscriber row read `dropped == 0` whenever its flood ran before registration. | TRUE |
| *"No event is known to have been lost in production … Nobody has measured the window."* | **FALSE as of the draft's own date.** Oracle's test clients lost events to exactly this window: `F-MACHINEREPLACED-EVENT-RACE`, seen in CI on 2026-09-10 and again on 2026-09-12 (`d77182c`). The mechanism was reproduced and fixed on 2026-09-13 (`725fb04` brief, `c6ed909` fix, merge `7ccd42d`), four days before the draft. The fix is R1's client-side barrier. `tests/common/mod.rs:612-633` ends `handshake(true)` with an `emulator/status` round trip, *"an ordering guarantee, not a wait"*. Oracle registered this as a hub gap at the time (`docs/OVERSEER-REFERENCE.md:1583-1588`). **The only part not observed is loss in a live window**, and that part is reasoned. | CORRECTION |
| `protocol.md` §3 is at `:791`. | It is at `:821` in `290f8c94`. The text is unchanged. §2.1 is still at `:365`. | cite drift only |

### Does the resolution need anything from oracle?

- **R1 (subscribe before replying to any later request): oracle already does this.** No server change is
  needed. The "what would make R1 wrong" test the draft asks the implementing lane to run is the `c6ed909`
  control, and it passed.
- **R2 ("`droppedEvents` does not cover the pre-subscription window"): this is exactly what oracle does.** No
  server change is needed.
- **Schema or vendoring: none.** R1 and R2 are prose in `protocol.md`. Oracle vendors only
  `bus-protocol.schema.json` and `vectors.json` (`crates/oracle-aether/tests/contract/`), so there is
  nothing to re-vendor. Adding a conformance vector for R1 would change that, and none is proposed.

### A defect in oracle next to R2, measured (proposed row, not a fix)

**`F-EVENT-EVICTS-RESPONSE`: a flood of events can throw away a queued reply, and that reply is then
counted as a dropped *event*.** Responses and events share one `VecDeque` per connection. When the queue is
full, `push_event` pops the front entry, whatever kind it is (`outbound.rs:83-87`). The module doc
(`outbound.rs:26-28`) says responses take "the other path", but that is only true of how they are *added*.
Nothing stops them being *evicted*.

I ran a scratch crate against this tree's `oracle_aether::outbound::Outbound`, outside the worktree. With
capacity 2, it queued `push_response("RESPONSE id=7")` and then two `push_event`s:

```
probe: dropped(counted as droppedEvents)=1 remaining=["e0", "e1"]
response survived: false
```

The control was events only, and it showed the documented drop-oldest behaviour (`dropped=1`,
`front=Some("e1")`). The result is two errors:

1. A JSON-RPC request goes unanswered, so the client waits forever.
2. `droppedEvents` counts something that §2.3 defines as "events discarded" but that was not an event.

To be exposed, a connection must be subscribed, reading slowly, and meet more than 1024 queued pushes
between a reply being queued and that reply being written. That is rare, but it is exactly the slow client
§8 item 4 exists for. **This does not change R2's wording.** R2 is correct about what is *not* counted.
It is oracle that currently counts too much. The fix would evict only events, for example with separate
response and event lanes, or by skipping responses when evicting. No gate exists. I propose adding one
with the fix: an `outbound.rs` unit row saying *"a queued response survives any event flood"*.

### Verdict and recommendation

**AGREE-WITH-CORRECTIONS.** Land R1 and R2 as drafted. Before landing, fix the draft in two places:

- Replace the "no incident / nobody has measured" bullet with the `F-MACHINEREPLACED-EVENT-RACE` record: two
  CI sightings, a 20/20 reproduction, and the R1 barrier already in oracle's harness.
- Re-cite §3 at `:821`.

Keep `F-EVENT-READINESS-UNORDERED-TRANSPORT` suggested and unfiled, as the draft does. It is transport-shaped,
and oracle serves only the ordered Unix socket.

**This verdict would be wrong if** oracle ever handled a connection's lines out of order. That would take a
per-request worker pool or a priority lane in `connection_loop`. Neither exists today (`server.rs:715-806`).
If one is ever added, R1 stops being free.

---

## 2. F-FIRST-PRESENT-REFUSAL: **premise STALE; ruled and accepted; only the hub's application pass is open**

### What was re-derived from the tree

The brief's claim that a "server half landed 2026-09-19" is **TRUE**: merge `c590c7d` (agent tip `661174b`,
branch `parcel/first-present-order`). It landed **after** the draft (`b9bc2070`, 2026-09-18).

| Draft claim | In oracle today |
|---|---|
| `host.rs:403-408` says the first drain has no push before it, and a request answered by that drain gets `noDisplay` / `display:false` *from a window that exists*. | **No longer true.** `host.rs:403-409` now reads *"The first drain **used to** have no push before it, and that was a defect — repaired by `F-FIRST-PRESENT-REFUSAL`"*. |
| `oracle-player/src/main.rs:959` names the first publish, so a request answered by iteration 1's drain is refused `noPacing`. | Repaired. On iteration 1 the drain is deferred: `main.rs:895-900` sets `owe_deferred_drain`. It is repaid behind both publishes at `main.rs:1042-1046`. The comment at `main.rs:953-956` says so. Gate: `a_request_queued_before_the_first_frame_is_not_told_there_is_no_window`. |
| The frontend has "the same order by reading" (labelled a reading, not measured). | Repaired as an object: `oracle-frontend/src/drain.rs`, `drain::Order` (`:271-320`), used from `main.rs:1482-1484`. It has two gates. One is a differential over a real socket (`drain.rs:1340-1480`). The other checks that the loop defers its first drain and only its first (`:1497`). The loop itself still cannot be driven without a `minifb::Window`. The residue is `FRONTEND-LOOP-UNTESTABLE`, and `docs/2026-09-19-first-present-order.md` §5 states it. |
| `display` means *a window exists*, per the schema. | The schema still says that (`bus-protocol.schema.json:1382-1384` at `616c2026`, the same bytes oracle vendors). **The code has always computed *readable now*:** `engine.rs:3865` is `"display": self.screen_text.is_some()`, and `screen_text` becomes `Some` only through `set_screen_text` (`engine.rs:2257-2259`). The description is wrong about the code whether or not the race happens. |
| *"No incident."* | Retired. CI run `34752339602` on `453aa96` (2026-09-13) was this defect, misfiled as a flake: `{"frame":1,"mclk":896042,"reason":"noDisplay"}`. The hub banked this at empyrean `2258736f`. |

### What the hub has already done, and what is still open

- **Ruled** at empyrean `bde5b1b3` (2026-09-19). Option 2 was adopted as a correction to a false description.
  Option 5 (remove the state) was left to oracle, on condition that the contract states it as an obligation.
  Options 1 and 4 were refused.
- **Accepted** oracle's proposed clause verbatim at `2258736f`, to be applied "as its own draft-and-apply pass
  per D-30":
  > A windowed embedder MUST NOT pump the bus before it has published the window state its readbacks report,
  > and MUST NOT publish a snapshot of a frame it has not composed.
- **Not yet applied.** At empyrean `origin/main` `8684c9b8`, `protocol.md` contains no "windowed embedder"
  text, and §11.29's rider (`:2190-2191`) and the schema's `display` description are unchanged. Since
  oracle's pin, `git log 616c2026..origin/main -- contract/schema/ contract/protocol.md` shows only
  `290f8c94`, the §11.52 registration.

So the hold on this item is stale. The only work left is the hub's own application pass. Nothing is waiting
on oracle.

### Oracle's recommendation for the application pass, with cost here

Apply both halves of the ruling in one pass: the Dn clause, plus option 2's description fix. For the
description, something like: *"whether `emulator/screen_text` would answer now: true once a windowed
embedder has published its first snapshot; a headless server serves false."*

- **The Dn clause is prose only.** Oracle has no vendoring cost and no code cost. Both embedders already
  conform (with the one exception below), and each has its gate.
- **Option 2 is a schema description string.** Oracle's cost is one routine re-vendor parcel: the schema blob
  and a PROVENANCE row, the same shape as the `616c2026` re-vendor. There is **no code change**, because
  `engine.rs:3865` already computes the corrected meaning. Aurora has to re-vendor in the same turn, because
  its drift gate fails closed, as the hub's own ruling notes.
- A third reason string (option 1) stays refused. Oracle has no case for it.

### A gap left by the repair, found by reading (proposed row, TAG for live confirmation)

**`F-FIRST-PRESENT-RUNTIME-SERVE`.** The deferral is keyed on `self.iterations == 1` (`main.rs:895`), which
means "the first iteration of the process". It is not keyed on "the first drain since the window started
serving". But the player can **start serving mid-session**. The toolbar button calls `bus.serve_now()` from
inside `build_ui` (`ui.rs:6146`, then `bus.rs:1085-1096` → `host.serve`). By then, iteration N has already
read `let serving = self.bus.is_serving()` as `false` (`main.rs:973`). So the screen-text publish (`if serving`,
`main.rs:1018-1020`) and the pacing publish (`main.rs:957-959`, evaluated before `build_ui`) both skip iteration N. Iteration N+1 then drains at
`main.rs:898`, **before** its own publishes, while `screen_text` is still `None`.

The result: a client that connects in that one-frame gap and has a readback **pipelined** behind `initialize`
gets `noDisplay`, `display:false` or `noPacing` from a window that exists. That breaks the accepted clause's
own wording.

The exposure is narrow. The client must pipeline, because a client that waits for the `initialize` reply
will be answered at N+2, after N+1's publish. The frontend is not affected, because it binds only at
construction (`oracle-frontend/src/bus.rs:137`). The fix is small: owe the deferred drain whenever serving
has started and nothing has been published yet, instead of only on iteration 1. **This was reasoned from
source, not run.** It needs a foreground live check, or a player-gate row that calls `serve_now` and then
queues `initialize` and `screen_text` together.

**This verdict would be wrong if** empyrean had applied the clause somewhere other than `contract/`
(it has not in `protocol.md` or the schema at `8684c9b8`). It would also be wrong if `c590c7d` had since
been reverted. It has not: the deferral code cited above is present at `a51dd3b`.

---

## 3. DEB2-ROUTING-TO-ORACLE: **received; no live two-symbol collision today; a different, live oracle defect found**

### What the routing asks

Sigil measured that `asl` lists a label declared inside a `PHASE` block at its **phase (VMA)** address. For
example, `SoundTablesZ80_Head : 8000`. So sigil is compatible, and the 0x8000 collision is created
downstream, where a Z80-space address is filed as a 68000 ROM address. The routing sends this to "the lane
that consumes sigil's symbol output — the debugger". **Its two caveats are kept here as written:**

1. *"Sigil is compatible" was not freshly measured.* What sigil prints for the probe files was never
   reached.
2. *Nobody has measured whether a live collision exists today.* The per-tree check is
   `grep -E ' : 8000 [A-Z] \|' <shape>.lst`.

The routing asks whoever prices the consuming side to measure both before costing anything.

### What oracle does with the symbols (from code)

- **Oracle does not read `deb2` records.** It checks only the magic at `EndOfRom`, as a filter that binds the
  listing to the ROM (`oracle-core/src/symbols.rs:514-551`, `engine.rs:10359`). `convsym`'s rule of keeping
  one record per address therefore never reaches oracle. Oracle resolves from the `.lst` text through
  `SymbolTable::parse` (`symbols.rs:741`).
- **Oracle drops nothing on a collision.** `symbols_at` returns every name at an address (`symbols.rs:1301`).
  `resolve` picks the last entry in `(addr, name)` order (`:898`, `:1322-1325`), so on a tie the name that
  sorts later wins. `SoundTablesZ80_Head` sorts after any `$…`-mangled local, so it would **win** over a
  local 68000 label at the same address. This is reasoned from the sort key, not run.
- **The `Phase Table` rows are kept out of the 68000 indexes** (`symbols.rs:72-128`, test
  `phase_rows_are_a_third_population_reaching_neither_syms_nor_equates`). **But the same labels also appear
  as ordinary `Symbol Table` rows**, for example `SoundTablesZ80_Head : 8000 C`. Those rows are ingested as
  68000 bus addresses. Nothing uses the phase table to demote them. The unit fixture cannot see this, because
  its phased names appear only in the phase table and not in its symbol table. The real listings have them
  in both.

### Measured

I built a scratch crate against this worktree's `oracle-core`, outside the worktree, and ran it on four
listings:

- oracle's git-tracked `fixtures/aeon/{s4,s4.debug}.lst` at `a51dd3b` (pin: sigil freeze `39c34fd2`,
  `aeon_rev 3f143178`, no Phase Table);
- the on-disk live build `/home/volence/sonic_hacks/aeon/{s4,s4.debug}.lst`. These are untracked build
  outputs with mtime 2026-09-27 04:30 / 03:38. aeon's working-tree HEAD was `aa9cb67b` at read time, but I
  have not verified these files are a clean build of it. They carry a Phase Table with six rows, so sigil
  emitted them.

**Positive control:** `address_of("EntryPoint") = 000200` in all four.

| Query | live `s4.debug.lst` | live `s4.lst` | frozen `s4.lst` |
|---|---|---|---|
| `address_of("SoundTablesZ80_Head")` | `008000` (phase says VMA `008000`, LMA `0B8000`) | `008000` | `008000` |
| `resolve($008010)` | `SoundTablesZ80_Head+$10` (real code there is `engine.parallax`) | `PageIn_Flush+$C` | `SoundTablesZ80_Head+$10` |
| `resolve($008002)` | `SoundTablesZ80_Head+$2` | `SoundTablesZ80_Head+$2` | `SoundTablesZ80_Head+$2` |
| `resolve($0083DA)` | `SndDefaultPitchTable+$1` | `SndDefaultPitchTable+$1` | `SndDefaultPitchTable+$83` |
| `resolve($0B8000)` (where the tables actually sit in ROM) | `__align$games.sonic4.dac_banks$0+$8000` | same | `EndOfRom+$12370` |

**The two-symbol collision the note worries about does not exist today.** The note's grep matches exactly
one row, `SoundTablesZ80_Head : 8000 C`, in each s4 listing, frozen and live. That hit is itself the grep's
positive control. `symbols_at($8000)` returns only `["SoundTablesZ80_Head"]` in all four. The demo
listings have no symbol at `8000`.

**On caveat 1, one data point from the consumer side:** sigil's real listing, both frozen and live, puts
`SoundTablesZ80_Head` at its VMA `8000`, which matches asl's probe. **Sigil's output for the probe files is
still unmeasured.** That measurement is sigil's to make, and this does not replace it.

### The oracle defect this exposes (proposed row, not a fix)

**`F-PHASED-LABEL-IN-68K-SYMBOLS`: Z80-phased labels answer 68000 symbol queries today.** Two effects,
both seen in the table above:

- **Forward.** `emulator/lookup_symbol {name:"SoundTablesZ80_Head"}`, and any `symbol:`-addressed read (via
  `resolve_target`, `engine.rs:3634`), answer `$008000` in 68000 space. That is unrelated 68000 code, not the
  tables, which the 68000 sees at LMA `$0B8000`.
- **Reverse.** Real 68000 PCs name Z80 labels. `symbolAtPc`, `lookup_symbol {addr}` and every decoder's
  `name` go through `symbol_at` → `resolve` (`engine.rs:10152`). In the live debug build, all of
  `$8000-$801B` reads as `SoundTablesZ80_Head+…`. Around `$83D9`, `$84E1`, `$85F3` and `$8633`, it reads as
  pitch, sfx, opcode and DAC table names.

This is the silent, plausible-but-wrong answer that `symbols.rs`'s module doc exists to prevent.

**Cost, and the open design question.** The `Phase Table` gives both VMA and LMA but not **which CPU** a
block belongs to. `symbols.rs:661-666` refuses to guess on purpose. So a correct fix needs either:

- (a) a CPU or address-space token on each `PHASE` row. That is a **sigil ask**, and it is additive.
  Oracle's parser already keys rows on `VMA` and `LMA` and tolerates extra tokens. Or:
- (b) an interim rule in oracle: any relocated phased name (VMA ≠ LMA) is dropped from `rev` for reverse
  lookup, and its forward address is taken from the LMA. That rule is right for Z80 blocks. It would be
  wrong for a 68000 block phased into RAM, and none exists in the six rows today.

Either way, the oracle side is roughly one `SymbolTable::build` change plus a fixture that matches the real
listing's shape (phased names in *both* sections).

Listings without a Phase Table, such as the frozen fixtures, older sigil output and stock AS, have no signal
to act on. They keep today's behaviour. That limit should be stated.

**Should a row be booked? Yes, for oracle:** `F-PHASED-LABEL-IN-68K-SYMBOLS`, live today, reverse and forward.
The `convsym` / `deb2` collision itself is **not** oracle's: oracle does not read those records. It belongs
to whoever owns aeon's `convsym` step, and it is latent with no current instance.

**This verdict would be wrong if** Aether's symbol addresses were meant to include Z80 VMAs, that is, if
`lookup_symbol` were intentionally CPU-agnostic. §4 and oracle's own `AddrSpace` handling treat them as
68000 bus addresses, but a contract reading on this should be confirmed before building (b).

---

## What was not run

- **No emulator and no MCP tool was used.** Two items need a foreground live check: item 2's
  runtime-serve gap, and item 3's reverse-naming as seen in a live `emulator/status` (`symbolAtPc` at
  PC ∈ `$8000-$801B` on the live debug ROM).
- **No crate test suite was run** (`cargo test` was not invoked in this worktree, and `vendor/` was not
  linked). The two measurements above are scratch crates in the session scratchpad that depend on this
  worktree's crates by path. Each has its control shown next to its result.
