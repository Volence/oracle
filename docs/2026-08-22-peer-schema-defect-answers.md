# Answers to four contract defects, from the server's source

**2026-08-22.** The empyrean overseer landed per-method JSON-Schema fragments for the Aether bus and, in
writing them, registered 32 contract defects. Four of them ask *us* — the implementing server — what we
actually do, before the contract rules. This document answers those four from source, plus a clearly
separated secondary section on eight unfragmented methods.

**Scope and stance.** This is a **reading**, not a change. No behaviour was altered, no code was edited, no
spec text is proposed here. Where the source does not settle a question this document says so and names what
would. Where our implementation appears to contradict the spec it says so plainly.

### Provenance of the empyrean-side artifacts — revision for lineage, hash for identity

`docs/2026-08-22-protocol-schema-audit.md` at commit **`62b8050`**, merged by **`fe5a238`**, banked by
**`ceef822`**; the new fragments at `contract/schema/bus-protocol.schema.json` on the same merge.

An earlier draft of this document recorded `fe5a238`/`ceef822` as **unpushed local commits**. That was true
when the work started and is **now stale** — corrected here rather than left standing. Verified firsthand at
the time of writing:

| fact | value |
|---|---|
| `empyrean origin/main` | **`baf15c28540866dcc41c51e4d4f065c63a06dbaf`** |
| `fe5a238` ancestor of `origin/main` | YES (`git merge-base --is-ancestor`) |
| `ceef822` ancestor of `origin/main` | YES |
| `62b8050` ancestor of `origin/main` | YES |
| `contract/protocol.md` blob at `origin/main` | **`1e832b1d671992a8a41e2101b886b1e0c9ad1967`** |
| `contract/schema/bus-protocol.schema.json` blob at `origin/main` | **`bb252a4d1381e1cd9f20f93d6a5f2a160f9796dc`** |
| `docs/2026-08-22-protocol-schema-audit.md` blob at `origin/main` | **`864276db42f4bda0f8a13254e099f625c3822ad7`** |

**Why the blob hashes are here and not just the revisions.** Their `main` moves several times a day — it had
already advanced past the tip named in the request that prompted this check by the time it was run — so a
future reader resolving `main:<path>` gets whatever is current, not what was read. The blob hash names the
exact bytes this document was written against.

**Working-tree-versus-committed check, run because a sibling repo's directory is that peer's live tree.**
Every quotation below was originally taken by path from `/home/volence/sonic_hacks/empyrean`, which names
"whatever is on disk right now" rather than a revision. All three files were re-checked against
`origin/main`: `git diff --stat origin/main --` on all three produced **no output**, i.e. the working tree is
byte-identical to the committed version for each. **Every quotation therefore matches the committed text and
no difference had to be reported.** Line numbers cited against `contract/protocol.md` are valid at blob
`1e832b1d`.

---

## 0. The finding that reframes three of the four questions

Read this before D-10, D-13 and D-17, because it changes what those answers *are*.

**Our server serves 37 methods.** They are the 37 `MethodSpec` entries in
`crates/oracle-aether/src/engine.rs:200-423`, and `Engine::dispatch`
(`crates/oracle-aether/src/engine.rs:984-1003`) is the **only** dispatch path — a method absent from
`METHODS` is answered `-32601`, structurally, with no handler to reach.

**Cross-referencing that set against the fragments:**

| | count |
|---|---|
| methods our server serves (`engine.rs:200-423`) | 37 |
| method fragments in the **vendored** schema (`crates/oracle-aether/tests/contract/bus-protocol.schema.json`) | 37 |
| method fragments in empyrean's **new** schema (`ceef822`) | 58 |
| served methods with **no** fragment | **0** |
| fragments for methods our server **does not serve** | **21** |

The 21 are exactly the pass's new work: `audio_spectrum`, `breakpoint_add`, `breakpoint_clear`,
`breakpoint_list`, `get_channel_states`, `get_layer_states`, `log_clear`, `ping`, `run_to_scanline`,
`set_channel_enabled`, `set_layer_enabled`, `step`, `step_out`, `step_over`, `vgm_start`, `vgm_status`,
`vgm_stop`, `wait_for_break`, `write_vram`, `z80_read`, `z80_write`.

**So: every fragment added in this pass describes a method the reference server does not implement**, and
D-10, D-13 and D-17 all sit inside that 21. This is not an oversight on either side — our server *advertises*
the absence, in `initialize`'s capability block:

- `"z80": false` — `crates/oracle-aether/src/engine.rs:1046`
- `"vgm": false` — `crates/oracle-aether/src/engine.rs:1047`
- `"objectDecoders": false` — `crates/oracle-aether/src/engine.rs:1048`
- `"breakpoints": false` — `crates/oracle-aether/src/engine.rs:1050`

and the vendored schema's own preamble says so in as many words: the fragment set *"covers every method the
reference server advertises"*, and *"the rest of §6 (breakpoints, Z80, VGM, object decoders, and the other
deferred families) is completed as each is implemented"*
(`crates/oracle-aether/tests/contract/bus-protocol.schema.json:5`).

**The word `breakpoint` appears exactly once in the entire `crates/*/src` tree outside of prose**: as the
capability flag `false` at `engine.rs:1050`. (The only other occurrence anywhere in `src/` is a doc-comment
analogy at `crates/oracle-core/src/bus.rs:312`.) There is no breakpoint table, no `breakpoint_add`, no
`enabled` bit, nothing to key on. The same is true of `z80_read`/`z80_write`/`z80_registers`,
`set_layer_enabled`/`set_channel_enabled`, and all eight of the secondary section's methods.

**Where the §6 rows the audit is auditing actually come from.** They are transcriptions of the **legacy C++
Oracle**, `/home/volence/sonic_hacks/oracle-old`, which is the binary currently registered as the `oracle`
MCP server (`~/.claude.json` `mcpServers.oracle.command` →
`/home/volence/sonic_hacks/oracle-old/linux-port/mcp/oracle-mcp`). `protocol.md` itself calls that
implementation out by name — *"a number on the wire in the legacy server"* (`contract/protocol.md:2295`) —
so this is acknowledged provenance, not a discovery.

**What this means for the steward, stated once:** for D-10, D-13, D-17 and the eight secondary methods, the
question *"what has our server been built against?"* has the answer **"nothing — it has not been built."**
The only running implementation is the legacy C++ one. Sections below therefore answer in two clearly
labelled halves: **(A) the reference Rust server**, which is the thing conformance is measured against, and
**(B) the legacy C++ server**, which is the thing the §6 rows were transcribed from and the thing a client
talking to `mcp__oracle__*` reaches today. Half (B) is offered as evidence about where the rows came from. It
is **not** a claim about what the reference server will do when these families land.

D-30 is the one question of the four that is squarely about the reference server, and it is answered in full.

---

## D-30 — `caveat`: which sentence has our server been built against?

### The contradiction, restated from the source

`contract/protocol.md:473-474`, §2.4 rule 1: a caveat *"is **optional and handler-emitted**, and therefore
unlike the stamp in every way that matters: a server does not apply it structurally, does not overwrite it,
and **may emit it on any result**."*

`contract/protocol.md:481-486`, §2.4 clause 4: *"**A schema fragment, however, MUST declare `caveat` for any
method that can emit one**, because §8 item 20 closes results against their fragments at test time and an
undeclared caveat would fail that check."*

### Answer: clause 4. Unambiguously, and by construction

Our server has been built against the **clause-4** reading, and the fragments say so in their own prose. Two
fragment descriptions state the clause-4 rationale verbatim as the *reason the key is declared at all*:

- `emulator/read_memory`: *"Declared here because §8 item 20 closes results against their fragments, so a
  method that CAN emit a caveat must declare it or its own conformant reply is rejected."*
- `emulator/watchpoint_add`: *"Declared because §8 item 20 closes results against this fragment."*

(Both in `crates/oracle-aether/tests/contract/bus-protocol.schema.json`, `methods.<name>.result.properties.caveat.description`.)

Rule 1's "may emit it on any result" has **never** been exercised as a licence. Every emission site is a
site whose method's fragment declares the key.

### Every result-emitting site that can attach a `caveat`

**Method of enumeration, and why it is trustworthy.** The reply record on this bus is not a typed struct — it
is `serde_json::Value`, and `caveat` is a JSON *key*, so "the field's type" is the string `"caveat"` in a
`json!` literal or a `Map::insert`. The enumeration is therefore the closure of:

1. every occurrence of the literal `"caveat"` in any `crates/*/src/**.rs` (assignment, `json!` key, or
   `Map::insert` key);
2. every **dynamic** map key that could evaluate to `"caveat"` — checked and found to be exactly one site,
   `engine.rs:1115` (`params.insert(k, v)` in `Engine::emit`), which copies the **stamp** into an *event*'s
   params, not a result, and whose source `Engine::stamp` (`engine.rs:1006-1009`) produces only
   `frame`/`mclk`/`running` via `rpc::stamp_object` (`crates/oracle-aether/src/rpc.rs:244`);
3. every **copier** of a reply record or of a sub-record that lands inside one:
   - `rpc::stamp_result` (`crates/oracle-aether/src/rpc.rs:269-280`) — merges the stamp into a finished
     result; adds `frame`/`mclk`/`running` only;
   - `rpc::bounded_array` (`crates/oracle-aether/src/rpc.rs:302`) — wraps items with
     `items`/`total`/`returned`/`limit`/`truncated`; adds no `caveat`;
   - `watch_report_json` (`engine.rs:4181-4226`) — the one sub-record builder shared across the watch
     surface. It emits **no** `caveat`; the census-key caveat is added one level up by its single caller
     (`engine.rs:3663` → `engine.rs:3689`);
   - `watch_stamp_json` (`engine.rs:4230`), `match_item` (`engine.rs:4244`), `layer_json`
     (`engine.rs:4257`), `profiler_row` (`engine.rs:2456`), `profiler_caller_edge` (`engine.rs:2517`) — no
     `caveat` in any.

**Result: 15 write sites, in exactly one file** (`crates/oracle-aether/src/engine.rs`), across **11 distinct
methods**. Zero sites in `oracle-core`, `oracle-frontend`, `oracle-replay`, or in `oracle-aether`'s
`host.rs`/`rpc.rs`/`server.rs`/`session.rs`/`outbound.rs`.

| # | line | method | trigger | reachable in normal operation? |
|---|---|---|---|---|
| 1 | `engine.rs:1521` | `emulator/run_to` | `run.stopped_by.is_some()` — a `stopAfter` watch ended the run before the target PC was reached | **Yes.** Requires an armed `stopAfter` watch; ordinary once the watch surface is in use |
| 2 | `engine.rs:1527` | `emulator/run_to` | `!run.predicate_fired` — the run hit `maxFrames` without reaching the target | **Yes**, routinely |
| 3 | `engine.rs:1560` | `emulator/read_memory` | none — inside the `json!` literal | **Yes, on EVERY successful reply.** Unconditional |
| 4 | `engine.rs:1907` | `emulator/read_vram` | none — inside the `json!` literal | **Yes, on EVERY successful reply.** Unconditional |
| 5 | `engine.rs:2059` | `emulator/state_hash` | none — inside the `json!` literal | **Yes, on EVERY successful reply.** Unconditional |
| 6 | `engine.rs:2426` | `emulator/get_profiler_frames` | `profiler_caveat(abandoned_frames, depth_exceeded).is_some()` (`engine.rs:4655`) — non-zero on either | Yes, but only on a degraded sample |
| 7 | `engine.rs:2695` | `emulator/scanlines` | `!from_raster` — no completed frame retained, rows came from a post-hoc state render | Yes: before the first drawn frame, and after `reset`/`reload_rom`/`restore` |
| 8 | `engine.rs:2737` | `emulator/screenshot` | `!from_raster`, same condition | Same |
| 9 | `engine.rs:2933` | `emulator/lookup_symbol` (addr branch) | `r.displacement > 0` — nearest *preceding* symbol | **Yes**, extremely common |
| 10 | `engine.rs:2940` | `emulator/lookup_symbol` (addr branch) | `r.symbol.demangled_ambiguous` | Yes, on mangled listings |
| 11 | `engine.rs:3001` | `emulator/lookup_symbol` (exact-demangled branch) | `total > 1` | Yes |
| 12 | `engine.rs:3030` | `emulator/lookup_symbol` (prefix-fallback branch) | none — inside the `json!` literal | **Yes, on every reply that reaches this branch.** Unconditional within the branch |
| 13 | `engine.rs:3141` | `emulator/load_symbols` | `caveat.is_some()` | **Yes, on EVERY successful reply — see note below.** Effectively unconditional |
| 14 | `engine.rs:3222` | `emulator/reload_rom` | `symbols_dropped` — the loaded listing no longer binds to the new image | Yes |
| 15 | `engine.rs:3690` | `emulator/watchpoint_list` | a listed watch groups by a `CensusKey` with no wire spelling (`census_key_name` → `None`, `engine.rs:4163-4170`) | **No — not by any bus client.** The three census keys `parse_watch_mode` accepts (`engine.rs:4058-4065`) all *have* wire spellings, so this fires only for a watch armed **locally by the player's own panel**, which holds a `&mut Watchpoints` on the same shared instrument (`engine.rs:4156-4162`). Unreachable from the socket alone |

**Note on #13, `load_symbols`.** This looks conditional and is not. `caveat` is computed at
`engine.rs:3053-3100` by matching `RomBinding`; the three arms that **accept** a listing —
`Match`, `Indeterminate(EndOfRomIsImageEnd)`, `Indeterminate(NoEndOfRomSymbol)` — each yield `Some(..)`
(lines 3054, 3082, 3092), and the two rejecting arms return `Err` (3062, 3069). The later
`match (caveat, addressless)` at `engine.rs:3117-3131` only ever appends. So the `if let Some(c)` at 3140 is
always taken on success: **every successful `load_symbols` carries a caveat.**

**Totals.**
- **15** emission sites.
- **11** methods that can emit: `run_to`, `read_memory`, `read_vram`, `state_hash`,
  `get_profiler_frames`, `scanlines`, `screenshot`, `lookup_symbol`, `load_symbols`, `reload_rom`,
  `watchpoint_list`.
- Of those, **5 are unconditional-in-practice** — `read_memory`, `read_vram`, `state_hash`, `load_symbols`,
  and `lookup_symbol`'s prefix-fallback branch. §2.4's own advisory
  (`contract/protocol.md:488-495`) names `emulator/read_memory` as exactly this anti-pattern, so four of the
  five are the spec's own known-bad shape and one (`load_symbols`) is not yet named there. Reported, not
  defended.
- **1 is unreachable from the bus alone** (`watchpoint_list`).

### Would a strict "fragment must declare it" reading refuse our server today?

**No.** All 11 emitting methods declare `caveat` in both the vendored fragment set and empyrean's new one.
The strict reading costs us nothing.

More than that — our server **over-declares** rather than under-declares. Four methods declare `caveat` in
the fragments and have **no** emission site in the handler:

| method | declares `caveat` | emits it |
|---|---|---|
| `emulator/read` | yes | **no** — handler `engine.rs:1663-1725`, reply assembled at `1709-1724`, no `caveat` |
| `emulator/watchpoint_add` | yes | **no** — reply assembled at `engine.rs:3584-3601` |
| `emulator/watchpoint_clear` | yes | **no** — replies at `engine.rs:3619`, `engine.rs:3635` |
| `emulator/watchpoint_hits` | yes | **no** |

Declared 15, emitting 11. That asymmetry is deliberate: `read`'s fragment says the field is *"Emitted
CONDITIONALLY, never on every reply"* and `watchpoint_add`'s says *"Declared because §8 item 20 closes
results against this fragment"* — a defensive declaration against a future emission, not a record of one.

**Bearing on the ruling.** The audit's recommendation (narrow rule 1 to *"may emit one on any result whose
fragment declares it"*) is exactly what this server already implements, and adopting it would change nothing
about our behaviour or our fragments. We have no stake in rule 1's broad phrasing and would not lose a
capability if it were narrowed. The `sprites` / `write_memory` / `read_cram` explicit-absent rulings are also
already true of us: none of those three has an emission site.

### One thing found here that was not asked about

**`emulator/lookup_symbol`'s address branch can silently discard a caveat.** Sites #9 (`engine.rs:2933`) and
#10 (`engine.rs:2940`) both write `out["caveat"]`, and #10 runs after #9 with no `else`. A lookup that is
*both* displaced (`r.displacement > 0`) *and* ambiguous (`demangled_ambiguous`) reports only the ambiguity;
the "this address is $N past the nearest preceding symbol and may belong to no symbol at all" warning is
overwritten and lost.

This is conformant — §2.4 makes `caveat` singular and rule 3 forbids parsing it, so no client can be broken
by it — but it is a real loss of the more consequential of the two warnings, on the branch where D-08's
`symbolDisp` companion is the machine-readable half. Flagged because it is one line from the shape §2.4's
rule 3 argues for (both facts already have or want typed keys: `disp` and `ambiguous` are both on the reply
at `engine.rs:2925-2926`), and because the audit is the right forum for it. **No fix is proposed or applied
here.**

---

## D-13 — the breakpoint surface: no handle, no enable/disable, no cap

### (A) The reference Rust server: there is no breakpoint surface at all

Every part of D-13 (a), (b), (c) and (d) is **unanswerable against this server, because the thing being
described does not exist here**:

- No `breakpoint_add`, `breakpoint_clear` or `breakpoint_list` in `METHODS`
  (`crates/oracle-aether/src/engine.rs:200-423`), and `Engine::dispatch` (`engine.rs:984-1003`) has no
  other route in — an uncatalogued method is `-32601` before any handler is reached.
- No breakpoint store anywhere in `crates/*/src`. The word appears twice in the whole `src` tree and
  neither is code: the capability flag `"breakpoints": false` (`engine.rs:1050`) and a doc-comment analogy
  about sink semantics (`crates/oracle-core/src/bus.rs:312`).
- Consequently there is **no `enabled` field**, nothing that writes one, and nothing to cap.

So on (a): identity is neither address-based nor handle-based, because there is no identity. On (b): the
field does not exist. On (c): there is nothing to cap. On (d): see below.

**This is the honest answer and it should not be smoothed.** The reference server's stop-on-condition surface
is `run_to` + the watchpoint family, and the design record says that was a deliberate substitution rather than
a deferral — see the incident section below.

### (d) The contrast with the watchpoint surface, concretely

The watchpoint family is the shape D-13 recommends the breakpoint family be brought up to, and it is already
built. Point for point:

| discipline | watchpoints, as implemented | anchor |
|---|---|---|
| **Opaque handle, not an address** | `watchpoint_add` returns `watch: "w<N>"`, a server-assigned opaque string. `resolve_watch_handle` accepts **only** that spelling — no bare-number fallback, so `{"watch": 3}` is refused rather than blessed | `engine.rs:3579-3585`, `watch_wire_id` `engine.rs:3965-3967`, `resolve_watch_handle` `engine.rs:3974-3976` |
| **…and the reason is exactly D-13's** | The source states it: *"a watch id is precisely the value §6 says cannot be an address or an index — **one address may carry several watches**, and the same number names four different things across the four spaces"* | `engine.rs:3960-3964` |
| **Ids monotonic and never reused** | `watches_issued` only ever rises (`self.watches_issued.max(id.0 + 1)`), so a cleared handle's number is never re-issued and a stale cursor cannot step over a live watch | `engine.rs:3580`, `engine.rs:3641-3644` |
| **A never-issued handle is refused, loudly** | `resolve_issued_handle` filters on `id.0 < self.watches_issued` and returns `-32602` naming the handle. A typo cannot be answered with a plausible-looking empty page | `engine.rs:3824-3833` |
| **A retired handle still answers** | `watchpoint_hits` keeps working for a cleared watch — clearing does not delete evidence, so *"one client [cannot] erase another's evidence on a shared bus"* | `engine.rs:3631-3634`, `engine.rs:3700-3703` |
| **An advertised cap** | `capabilities.watchpoints.maxWatches` in the handshake, default 32 | `engine.rs:1071`, `engine.rs:124`, `engine.rs:167` |
| **…enforced with a loud, named refusal** | `-32005 {reason:"watchCapReached", cap, count}`. *"Never grow past the number, never evict a handle a client is still holding."* Checked **last**, so a malformed request is told about the malformation instead | `engine.rs:3553-3568`, `engine.rs:119` |
| **Idempotent clear** | `watchpoint_clear` of a retired, never-issued or unspellable handle is `removed: 0`, not an error — §6.1's *"an error a client must learn to swallow teaches clients to swallow errors"* | `engine.rs:3622-3635` |

The one discipline the watchpoint surface **also** lacks is enable/disable: there is no
`watchpoint_set_enabled`, and `watch_report_json` (`engine.rs:4181-4226`) emits no `enabled` key at all. So
on that specific sub-point the two surfaces are *not* asymmetric — the watch surface simply declined the
field rather than emitting an unwritable one. Worth stating, because D-13(a)'s remedy
(`breakpoint_set_enabled`) would be the first enable/disable on this bus, not a copy of an existing one.

### Has this actually bitten us? Yes — and the primary record is in this repo

Not a hypothetical. `docs/2026-07-23-timing-ground-truth-fable.md:162-165`, written by the agent that hit
it, under a heading called "Session hygiene":

> Oracle left **paused**, VGM logging stopped, **all breakpoints cleared**. Note: 7 breakpoints
> pre-existed this session (not mine) and were removed to keep the free-run captures clean:
> `0x5CAC8` (×2), `0x5CAB0`, `0x5E5C2`, `0x5E5AA`, `0x9C44`, `0x3C46` (1,691,410 hits). Restore if
> another workflow needs them.

That paragraph is D-13 (b) happening, in the field, in one sentence: **one client silently disarmed another
client's breakpoints by address, could not tell whose they were, and had no way to put them back** — hence
"Restore if another workflow needs them", an instruction it could not carry out itself. Ownership was not
recoverable because identity was the address and nothing else.

Three further things that record settles, beyond what was asked:

1. **`0x5CAC8` (×2) is a duplicate at one address.** The legacy server permitted two breakpoint entries on
   the same address, and they were legible as two. That is empirical evidence on **D-12** (which asks what a
   duplicate `breakpoint_add` does), and it argues *against* the idempotent reading D-12 recommends — at
   least as the legacy server behaves. D-12 should be ruled knowing this.
2. **The clear was total, not selective.** With no handle and no ownership there was no selective option; the
   only available move was "clear everything and hope".
3. **The measured harm is corroborated three ways in our tree**, all first-party:
   `docs/2026-08-14-tooling-frontier-recon.md:156-163` (the three independent statements of harm, of which
   the stale breakpoint is #2), `docs/2026-08-14-aether-change-requests.md:973-975`, and
   `docs/2026-08-15-watchpoint-bus-surface.md:271`, `:681`.

### The one correction we owe the record

`docs/2026-08-15-handoff-capability-layer.md:114-125` revisits the "breakpoints were never used" claim and
**narrows it**: that claim came from `oracle-next`'s hunt record only, and breakpoints *"paid off in ten
executed `aeon` episodes, three where nothing else would have worked."* The defensible finding it lands on is

> **breakpoint-as-deterministic-anchor is proven; breakpoint-as-interactive-session is proven harmful.**

That is why this server has `run_to` and a recording watch surface and no breakpoints: the anchor half was
built under different names, the interactive half was declined. It bears on D-13's disposition — the
recommendation to bring the breakpoint surface up to the watchpoint surface's shape is *not* in tension with
our design record, but "add breakpoints" as an interactive session surface would be.

### (B) The legacy C++ server — where the §6 rows came from

Read as evidence about the rows, not as a statement of what the reference server will do. All anchors are in
`/home/volence/sonic_hacks/oracle-old`. Two structural facts colour every reply below:

- **`ok` never reaches the wire.** Handlers build `{"ok":true,...}` and it is stripped twice —
  `linux-port/gui/ControlSocket.cpp:205` (`finish()` does `obj.erase("ok")`) and again in the dispatcher at
  `linux-port/gui/ControlSocket.cpp:2822-2823`. Key sets below are post-strip.
- **Error codes are inferred from message substrings.** `ErrorReply` throws
  (`linux-port/gui/ControlSocket.cpp:226-229`) and `CodeForMessage`
  (`linux-port/gui/ControlSocket.cpp:211-222`) picks the numeric code by matching substrings of the
  free-text message — `-32012` on "no symbols", `-32013` on "symbol not found", `-32000` on "not
  wired"/"not available"/"no 68000"/"no Z80", `-32010` on "loading"/"timed out", `-32004` on "out
  of"/"range"/"only"/"rejected"/"supported", default `-32602`. **This is worth the contract's attention on
  its own:** a code is a function of prose, so rewording a message silently changes the wire code, and a
  message that matches no substring lands on `-32602` regardless of what went wrong (see `"no 68k RAM"`
  below, which is a missing-device condition reported as invalid-params).

**(a) Identity is the ADDRESS, and it is worse than "keyed by address".**
`breakpoint_add` (`linux-port/gui/ControlSocket.cpp:792-811`) inserts into no server-side map at all; it
calls `CreateBreakpoint()` (`ExodusSDK/Processor/Processor.cpp:379-386`, an unbounded
`_breakpoints.push_back`) and stores the address as a *location condition*:
`SetLocationCondition(Condition::Equal)` (`:802`), `SetLocationConditionData1(addr)` (`:803`). Its reply is
`{addr}` plus a conditional `note` — **no id, no handle** (`:808-810`).

`breakpoint_clear` (`linux-port/gui/ControlSocket.cpp:834-859`) walks the whole list and matches on
`bp->GetLocationConditionData1() == addr` (`linux-port/gui/ControlSocket.cpp:854`). **The condition *kind*
is never checked**, so a range breakpoint (`Greater`, `Less`) created in the GUI still carries a `Data1` and
will be deleted by an address-equality clear that was never aimed at it. That is a second, sharper form of
the cross-client hazard than the one D-13 names.

Params: `breakpoint_add` takes `addr` **or** `symbol` (`:796`), neither ⇒ `-32602 "need addr or symbol"`.
`breakpoint_list` takes **none** — the params object is unnamed and unread (`:813`); no filter, no limit, no
cursor. `breakpoint_clear` takes `all` (checked first, `:838`), else `addr` or `symbol` (`:847`).

**(b) `breakpoint_list` emits `enabled`. Emit site, verbatim:**

```
linux-port/gui/ControlSocket.cpp:824-826
        std::snprintf(entry, sizeof(entry),
                      "{\"addr\":\"0x%08X\",\"enabled\":%s,\"hits\":%u}",
                      a, bp->GetEnabled() ? "true" : "false", bp->GetHitCounter());
```

**The writer enumeration, done as an enumeration.** "Nothing can write this" is a claim about *every*
writer, so the question was not settled by checking the catalogued methods and reporting their silence. The
closure taken was: the **field** (`_enabled`), the **mutator** (`SetEnabled`) repo-wide, every
**constructor** of the owning type, every **deserializer**, and every **copier**. C++ has no
struct-update or `Default::default()` analogue, so the constructor set *is* the implicit-initialisation set.
Each step and its result:

| step | search | result |
|---|---|---|
| the field itself | `_enabled` in `Breakpoint.{h,cpp,inl}` | **6 hits, all accounted for**: getter `Breakpoint.cpp:28`, setter `:34`, `LoadState` `:251`, `SaveState` `:271`, declaration `Breakpoint.h:74`, ctor init `Breakpoint.inl:11` |
| direct field access from outside the class | `_enabled` repo-wide, excluding `Breakpoint.*`/`Watchpoint.*` | **zero.** No friend access, no `memcpy`, no reinterpret |
| the mutator | `SetEnabled` repo-wide | **21 matches across 11 files**; after removing the `Watchpoint` family and the pure declarations/definitions, **9 breakpoint call sites in 6 files** |
| constructors | `new Breakpoint` / `CreateBreakpoint` repo-wide | **2 construction sites**, both routed through the single ctor at `Breakpoint.h:10`: `Processor.cpp:385` (`CreateBreakpoint`) and `Processor.cpp:~4466` (inside `LoadState`) |
| copy / assignment | `operator=`, copy-ctor in `Breakpoint.h` | **none declared.** Objects are heap-allocated and held by raw pointer; nothing copies one |
| deserialization | `LoadState` / `ExtractAttribute` | **one**, `Breakpoint.cpp:251`, already in the field list |
| test-only paths | — | the tree has no unit tests touching this type |

**11 writers total, in 6 files**, and here is the verdict **per path**:

| # | writer | value written | reachable over the bus? |
|---|---|---|---|
| 1 | ctor initialiser, `ExodusSDK/Processor/Breakpoint.inl:11` | **`true`**, always | indirectly — every `breakpoint_add` runs it |
| 2 | `LoadState`, `ExodusSDK/Processor/Breakpoint.cpp:251` (`ExtractAttribute(L"Enabled", _enabled)`; reached from `Processor.cpp:4460-4479`) | either, from XML | **no** |
| 3 | `breakpoint_add`, `linux-port/gui/ControlSocket.cpp:805` | **hard-coded `true`** | **yes — and this is the only bus path** |
| 4 | Linux GUI create, `linux-port/gui/main_gui.cpp:5874` | `true` | no |
| 5 | Linux GUI create, `linux-port/gui/main_gui.cpp:5991` | `true` | no |
| 6 | **Linux GUI per-row checkbox**, `linux-port/gui/main_gui.cpp:6047` | either | no |
| 7 | generic-access data source, `ExodusSDK/Processor/Processor.cpp:4956` | either | no |
| 8 | `BreakpointEnableAll`, `ExodusSDK/Processor/Processor.cpp:5115` | `true` | no |
| 9 | `BreakpointDisableAll`, `ExodusSDK/Processor/Processor.cpp:5127` | `false` | no |
| 10 | Windows disassembly view toggle, `Extensions/ProcessorMenus/DisassemblyView.cpp:914` | either | no (not in the Linux build) |
| 11 | Windows disassembly view, `Extensions/ProcessorMenus/DisassemblyView.cpp:933` | `true` | no |

**Verdict.** The audit's wording — *"no catalogued method can write it"* — survives the enumeration and is
**precisely the right wording**, because the enumeration shows uncatalogued paths that *do* write it. The
stronger claim "nothing can write it" would have been **false**: paths 2, 6, 7 and 9 all set it `false`.
What is true is narrower and worth stating exactly: **exactly one of eleven writers is reachable over the
JSON-RPC surface (path 3), and it writes the constant `true`.** So `enabled` reads back `false` only when a
human ticked the GUI checkbox, a savestate carried a disabled breakpoint in, or an Exodus command ran — none
of which a bus client can cause, observe the cause of, or undo. That is the field's real defect: not "dead",
but **read-only-and-externally-mutable**, which is worse than either, because a client sees it change and has
no way to act on it.

**One contradiction found inside the enumeration.** `Processor::CreateBreakpoint`'s own comment
(`ExodusSDK/Processor/Processor.cpp:381-382`) reads *"Note that the breakpoint is disabled by default, so it
will not trigger until it is modified"* — while the constructor it calls sets `_enabled = true`
(`ExodusSDK/Processor/Breakpoint.inl:11`). **Construction is enabled.** The comment is wrong, and it is
exactly the kind of single-path reasoning that makes "nothing enables this" feel settled.

Contrast D-14, which asks whether `breakpoint_list` needs §2.4's bounded-list companions: it has no `limit`,
no `cursor`, and emits every entry in one reply, so it is "complete by construction" **only in the sense
that nothing bounds it**, which is not the same as safe.

**(c) No cap, no quota, no refusal — enumerated by where a cap could live, not by one grep's silence.**
A cap on this surface could live in any of six places. Each was checked and each is **absent**:

| # | where a cap could live | checked | finding |
|---|---|---|---|
| 1 | the handler | `OpBreakpointAdd`, `linux-port/gui/ControlSocket.cpp:792-812` — read in full | **no count check of any kind.** It goes straight from `ResolveAddr` to `CreateBreakpoint` |
| 2 | the core create path | `Processor::CreateBreakpoint`, `ExodusSDK/Processor/Processor.cpp:379-388` — read in full | unconditional `new Breakpoint` + `_breakpoints.push_back`. **No `if (size() >= N)`**, no early return |
| 3 | the collection's own capacity | `std::vector<Breakpoint*> _breakpoints`, `ExodusSDK/Processor/Processor.h:389` | a plain `std::vector`. No fixed capacity, no `reserve`, no bounded container type |
| 4 | a named constant or config | `max_breakpoint` / `breakpoint.{cap,limit,max}` / the reverse, case-insensitive, repo-wide excluding docs | **zero hits.** No constant, no config key, no capability advertisement |
| 5 | the reply/message layer | `breakpoint_list`, `linux-port/gui/ControlSocket.cpp:813-832` — read in full; plus a search for a socket line/reply size ceiling | **no `limit` param, no `cursor`, no truncation, and no message-size cap in `ControlSocket.cpp`.** The only fixed buffer is `char entry[160]` (`:822`), which is **per entry** and cannot truncate a ~52-char row — so it bounds nothing |
| 6 | the MCP bridge | `linux-port/mcp/oracle_mcp.py` breakpoint tool definitions | the tools pass `all`/`addr`/`symbol` through; **no client-side cap** |

**Six places a cap would live; six checked; six absent.** Adding breakpoints grows `_breakpoints` without
bound and grows `breakpoint_list`'s reply array without bound. The cost lands on the hot path —
`CheckExecution` walks the whole vector per instruction (`ExodusSDK/Processor/Processor.cpp:532-535`) —
which is the mechanism by which a forgotten breakpoint becomes a 1,691,410-hit contaminant rather than
merely a stale entry.

The error-classification detail matters here too: even if a refusal were added, `CodeForMessage`
(`linux-port/gui/ControlSocket.cpp:211-222`) would pick its code by substring, so a message like
`"breakpoint cap reached"` would land on the default `-32602` rather than `-32005`. Any cap added to this
server needs a message engineered to hit the right substring, or the classifier replaced.

**(d) Duplicates and unknown addresses.** No dedup check whatsoever: two `breakpoint_add` calls at one
address create two distinct `Breakpoint` objects, both enabled, both rendered as identical
`{"addr":...,"enabled":true,"hits":0}` rows with nothing to tell them apart, and both `add` calls return the
same `{addr}` reply. **This is the empirical answer to D-12**, and it matches the field record above
(`0x5CAC8` ×2). The clear is a **match-all, not match-first** (`:849-857`), so clearing that address removes
both and reports `removed: 2`. An unknown address is not an error: the loop matches nothing and the handler
returns `{"removed": 0}` successfully (`linux-port/gui/ControlSocket.cpp:858`) — which is already the
idempotent reading D-15 recommends pinning.

**(e) The legacy watchpoint surface is *worse*, and this inverts the audit's framing.** D-13 reads the
breakpoint surface *"against the watchpoint surface directly below it"*. On the legacy server there is no
such surface to read against: `Handlers()` registers exactly one watch method,
`{"watchpoint_add", OpWatchpointAdd}` (`linux-port/gui/ControlSocket.cpp:2651`), and **no `watchpoint_list`,
no `watchpoint_clear`, no `watchpoint_hits`**. `OpWatchpointAdd`
(`linux-port/gui/ControlSocket.cpp:861-886`) returns `{addr}` only (`:881`) — no handle — creates via an
unbounded `CreateWatchpoint` (`ExodusSDK/Processor/Processor.cpp:638-646`), and hard-codes
`SetEnabled(true)` (`:879`). **Once added, a legacy watchpoint cannot be removed or inspected over the bus
at all** — a one-way door until process exit, savestate load, or GUI intervention.

The handle discipline D-13 wants the breakpoints to inherit therefore does **not** exist on the
implementation the rows describe. It exists only on the reference Rust server, where it was designed from
first principles as CR-11/CR-12 (`docs/2026-08-15-watchpoint-bus-surface.md`) explicitly *because* of the
stale-breakpoint incident. **The correct framing of D-13 is not "breakpoints never caught up with
watchpoints" but "the watch surface was rebuilt on a new server and the breakpoint surface was never carried
across."** That matters for the disposition: there is no legacy shape to preserve compatibility with.

One more thing the agent found that bears on the audit: the legacy repo's own MCP bridge
(`linux-port/mcp/oracle_mcp.py:414-449`) *defines* handle-based `watchpoint_list`/`watchpoint_hits`/
`watchpoint_clear` tools — with `"Watchpoint handle, e.g. 'w0'"` and cursor pagination — and filters them
out against the handshake's advertised method set (`linux-port/mcp/oracle_mcp.py:961-985`, whose comment
reads *"34 of 50 against oracle-next"*). Those are the **reference server's** surface, shipped in the legacy
repo's bridge. The two implementations are already being served through one client.

---

## D-10 — `z80_write`'s `value` has no width, so its own `len` is undefined

### (A) The reference Rust server: neither method exists

`emulator/z80_read` and `emulator/z80_write` are absent from `METHODS`
(`crates/oracle-aether/src/engine.rs:200-423`), and `capabilities.z80` is advertised `false`
(`engine.rs:1046`). Both are `-32601`. **We cannot report an accepted width, a byte order, a `len`
computation, or an out-of-range behaviour, because there is no handler.** The source does not settle
D-10 on this server, and only implementing the row would.

The absence is deliberate and recorded at the one place a reader would look — `emulator/read`'s doc comment,
which explains why the unified read surface has four spaces and not five:

> *"The Z80's space is deliberately absent — `emulator/z80_read` keeps its own row and its own catalogued
> bounds."* — `crates/oracle-aether/src/engine.rs:1661-1662`

### What the core would give a future handler, offered as constraint rather than as behaviour

This is not an answer to D-10. It is the material a `z80_read`/`z80_write` handler on this server would have
to be built from, and two facts in it bear directly on the row as written:

1. **The Z80 RAM is 8 KiB, not 16.** `Z80_RAM_SIZE = 0x2000` (`crates/oracle-core/src/bus.rs:704`), and the
   only accessor the core exposes is `System::z80_ram() -> &[u8]` of that length
   (`crates/oracle-core/src/system.rs:837-840`), read-only.
2. **`$2000-$3FFF` MIRRORS `$0000-$1FFF`.** The 68000-side Z80 window masks: `self.z80_ram[z as usize &
   (Z80_RAM_SIZE - 1)]` on read (`crates/oracle-core/src/bus.rs:929`) and the same mask on write
   (`crates/oracle-core/src/bus.rs:1020`). This is correct hardware behaviour.

   **This bears on the row and the audit did not name it.** §6 line 996 bounds `z80_read.addr` at `0–$3FFF`
   with `len ≤ $2000` — which is the *address space*, but only the bottom half is distinct storage. A read at
   `$3000` for `$2000` bytes is inside both stated bounds and would wrap the array twice. A server can answer
   it (mirrored bytes are the truthful answer) but the row does not say so, and a client cannot tell a
   mirrored read from a distinct one. Whatever D-10 rules about `value`, the `addr`/`len` pair on **both**
   Z80 rows needs the mirror stated or the bound tightened to `0–$1FFF`. Registered here as an observation,
   not a proposed amendment.

### `write_memory`'s machinery, for the symmetry comparison D-10 asks about

The audit's reading (b) is "take `write_memory`'s width machinery with little-endian order". Here is exactly
what that machinery is on this server, so the comparison is against code rather than against the row:

| | `emulator/write_memory`, as implemented | anchor |
|---|---|---|
| payload spellings | exactly one of `bytes` (hex string) **or** `value`+`width`; both, neither, or `value` without `width` is `-32602` | `engine.rs:1586-1637` |
| `width` domain | `1 \| 2 \| 4` only; anything else `-32602` *"`width` must be 1, 2 or 4"* | `engine.rs:1616-1624` |
| `value` fit | `value >= 1u64 << (width * 8)` is `-32602` *"`value` {value} does not fit width {width}"* — **refused, never truncated** | `engine.rs:1625-1629` |
| byte order | `value.to_be_bytes()[8 - width..]` — **big-endian**, with the comment *"Big-endian, as the 68000 stores."* | `engine.rs:1630-1631` |
| out-of-range | `-32004` for a base outside `$E00000-$FFFFFF` **or** an `end` past it — refused, never clipped | `engine.rs:1639-1645` |
| reply `len` | `data.len()` — the count of bytes actually placed, i.e. `width` on the `value` spelling and the hex payload length on the `bytes` spelling | `engine.rs:1651` |

So a `z80_write` built symmetrically here would take `len = data.len()`, and the **only** thing reading (b)
changes is line 1630's `to_be_bytes` → `to_le_bytes`. The audit's point that symmetry would be *actively
wrong* on byte order is correct and is a one-line difference in this implementation; nothing else in the
machinery has an endianness opinion.

**We take no position on (a) vs (b)** — that is the contract's call, and we have no shipped behaviour that
would be broken by either. If it helps the ruling: we would implement whichever is written, and (b) costs us
nothing beyond that one line.

### `z80_read`'s symmetry, treated in one paragraph as asked

Same answer: not implemented, `-32601`, `capabilities.z80: false`. The sibling it would be modelled on is
`emulator/read` (`engine.rs:1663-1725`), whose shape is `space`, `addr`, `len`, `bytes` with `len` defaulting
to 1 and ceilinged at `limits.maxReadLen`, `region`/`symbol`/`symbolDisp` present **iff** the space is `bus`
(`engine.rs:1709-1723`), and an out-of-range read refused with `-32004` rather than clipped
(`engine.rs:1696-1704`). A `z80_read` on the `read` pattern would therefore answer `{addr, len, bytes}` with
no symbol companions — which is exactly the §6 row at line 996 — and its `len` default would be 1, which is
the gap D-09 names. D-09 and D-10 are the same missing paragraph seen from two sides, and pinning `len`'s
default on the read row settles the reply's `len` on the write row too if `value` is given a width.

### (B) The legacy C++ server — and it has already chosen reading (a), in writing

This is the most directly decision-relevant finding in the whole document, because **the implementation the
row was transcribed from does not merely fail to specify the width — it deliberately refused to have one,
and left a comment saying why.** Verbatim, `oracle-old/linux-port/gui/ControlSocket.cpp:732-737`:

```
    else if (req.has("value"))
    {
        // Single byte — the Z80 bus is 8-bit. For multi-byte sequences use
        // `bytes` (written low-address-first, no endianness guesswork).
        bytes.push_back((uint8_t)(req.getInt("value") & 0xFF));
    }
```

**`value` is always exactly one byte.** There is no loop, no shift, and **`width` is never read by this
handler** — passing `width: 2` is silently ignored and you get a one-byte write of the low byte. So D-10's
byte-order question has no answer on this implementation *because the case it asks about was declined on
purpose*, with "no endianness guesswork" as the stated reason. That is reading (a), authored, shipped, and
justified.

For the contrast the audit draws: the 68000 `emulator/write` on the **same** legacy server *does* take
`width` and *is* big-endian —
`oracle-old/linux-port/gui/ControlSocket.cpp:612-618`, whose descending loop
(`for (int i = width - 1; i >= 0; --i)`) pushes the most significant byte first. So the legacy server already
implements the asymmetry the audit is asking the contract to bless: width-bearing big-endian on the 68000
row, width-free single-byte on the Z80 row.

**Everything else `z80_write` does** (`oracle-old/linux-port/gui/ControlSocket.cpp:718-749`):

| question | answer | anchor |
|---|---|---|
| params | `addr`, and one of `bytes` (hex pairs) or `value`; **`bytes` wins if both given**; neither ⇒ `-32602 "need bytes or value"` | `:723`, `:727-731`, `:732-737`, `:740` |
| **missing `addr`** | **silently writes to `$0000`** — `getU32("addr")` defaults to 0 with no required-check | `:723` |
| **malformed hex `addr`** | **silently becomes `$0000`** — `JsonObj::getInt`'s `catch (...) { return d; }` swallows it | `:145` |
| reply `len` | `bytes.size()` — decoded bytes actually written: `strlen/2` on the `bytes` path, always `1` on the `value` path | `:748` |
| reply keys | `{addr: hex string, 4 digits; len: number}`, both always present | `:748` |
| out-of-range **value** | **silently truncated**, `& 0xFF`. `value: 300` writes `0x2C`; `value: -1` writes `0xFF`. No warning | `:736` |
| out-of-range **addr** | refused, `-32004` (via the `"only"` substring), message names `$0000-$1FFF` *and its `$2000-$3FFF` mirror* | `:724`, `:690-692` |
| **the bound is start-only** | `addr=0x3FFF` with a 16-byte payload writes one byte then **wraps to `$0000` and clobbers the bottom of sound RAM**, reporting `len:16` and success. `WriteRamByte` folds modulo the device size | `:743-747`, `:298-317`, `:306` |
| odd-length / non-hex `bytes` | refused, `-32602` | `:730`, `:385-398` |
| partial write on failure | bytes `0..i-1` are already committed when it bails with `-32602 "write failed at offset N"` | `:745-746` |

**`z80_read`** (`oracle-old/linux-port/gui/ControlSocket.cpp:694-716`): `addr` (same silent-`$0000`
default), `len` **default 1** and **silently clamped** to `0x2000` with no error and no flag —
`std::min<uint32_t>((uint32_t)req.getInt("len", 1), 0x2000)` (`:701`), which also means **`len: -1` returns
8192 bytes** rather than erroring. Same start-only `$3FFF` bound (`:700`, `-32004`) and the same modulo-fold
on the tail (`:710`). Reply keys `{addr: hex string 4 digits, len: number, bytes: uppercase hex, no prefix,
2*len chars}`, all three always present (`:714`).

**What this adds to the ruling.** Our core independently confirms the mirror the legacy message names: the
Z80 RAM is `0x2000` bytes and the 68000-side window masks `& (Z80_RAM_SIZE - 1)`
(`crates/oracle-core/src/bus.rs:704`, `:929`, `:1020`). So two independent implementations agree that
`0–$3FFF` is an address space with 8 KiB of distinct storage behind it, and **neither bounds the tail of a
multi-byte access**. If D-10 is being ruled anyway, the tail bound and the mirror belong in the same
paragraph — a wrap that silently clobbers `$0000` while reporting success is the silent-wrong-answer class
this bus exists to prevent, and it is live in the only running implementation today.

---

## D-17 — the two setter enums

### The two methods, confirmed from §6 rather than assumed

The audit's candidates are correct, verified in `empyrean/contract/protocol.md`:

- `emulator/set_layer_enabled` — `contract/protocol.md:1135`, row `| layer, enabled | layer, enabled |`
- `emulator/set_channel_enabled` — `contract/protocol.md:1383`, row `| channel, enabled | channel, enabled |`

These are the only two setter rows in §6 with an enum-valued parameter and no vocabulary in the document.
Both are among the 21 fragments added this pass, and both are among the methods this server does not serve.

### Checking the "no vocabulary written anywhere in the spec" claim before inheriting it

A universally-quantified rule binds every actor it describes, so "the row is silent" is a much weaker
finding if a general clause elsewhere in `protocol.md` already answers the case. That phrasing is the
audit's, and confirming it is part of the job. `contract/protocol.md` (blob `1e832b1d`) was searched for a
general setter/enum/vocabulary clause: `every (setter|enum|method|request)`, `enum` near
`vocabular|value set|closed`, and `vocabular` alone.

**Result: the audit's claim stands. There is no general clause that supplies a vocabulary**, and no
"every setter…" or "every enum…" rule anywhere in the document. What the search returned is method-specific:
`lookup_symbol` as *"the shared-vocabulary op"* (`:696`), `stopped.reason` as *"a small closed vocabulary"*
(`:2662`), and the `read.space` ruling at `:925`/`:933`. So the finding is confirmed, not merely inherited.

**But the search turned up something that strengthens D-17's recommendation, which is why it was worth
running.** The rule the audit proposes — that a setter's enum *is* its sibling's key set, so the two cannot
drift — **is not a new invention. It is already a ruled precedent on this bus, for a different pair.**
`contract/protocol.md:925`: `emulator/read` takes *"`watchpoint_add`'s `space` vocabulary **unchanged** —
`bus`, `vram`, `cram`, `vsram`"*, and `:933` gives the reason in terms that transfer word for word:

> a read enum holding a value the watch enum refuses would be **two vocabularies wearing one name**

That is D-17's own argument, already adopted, already in the document, and already implemented on our
server — `emulator/read` and `emulator/watchpoint_add` share one parser, `parse_watch_space`
(`crates/oracle-aether/src/engine.rs:3998-4006`), called from `read` at `engine.rs:1664` and from
`watchpoint_add` at `engine.rs:3511` — one parser, two methods, so the two vocabularies **cannot** drift.
That is the mechanical form of the same rule, and it is what D-17 would be asking the layer/channel pair to
adopt. **So D-17's recommendation is the second application of an
existing house rule rather than a new one**, and the ruling can cite `:933` rather than argue from scratch.

### (A) The reference Rust server: neither method exists, and neither getter does

`set_layer_enabled`, `set_channel_enabled`, `get_layer_states` and `get_channel_states` are all absent from
`METHODS` (`crates/oracle-aether/src/engine.rs:200-423`). All four are `-32601`. **There is no parsing site,
so there is no accepted value set to report and no unrecognised-value behaviour to report.** The source does
not settle D-17 on this server; only implementing the rows would.

**One piece of adjacent ground truth that is real and that the ruling should have.** The layer vocabulary
already exists on this bus — not on a setter, but on `emulator/pixel_attribution`, which reports which layer
won a dot. Its wire spellings come from `layer_json` (`crates/oracle-aether/src/engine.rs:4257-4267`), and
they are the same four the audit reads off `get_layer_states`. If D-17 adopts the getter's key set as the
setter's enum, it should confirm it also matches `pixel_attribution`'s — three surfaces naming the same four
layers must not drift, and §11.18's rule that an emitted enum cannot be widened later applies to the one we
have already shipped.

### (B) The legacy C++ server — the exact accepted sets, and which members are contract

**`set_layer_enabled`** (`oracle-old/linux-port/gui/ControlSocket.cpp:1516-1526`). The matching site is
`LayerMuteFlag`, verbatim:

```
oracle-old/linux-port/gui/ControlSocket.cpp:1507-1514
static bool* LayerMuteFlag(const Context& ctx, const std::string& layer)
{
    if (layer == "plane_a" || layer == "planea" || layer == "a") return ctx.mutePlaneA;
    if (layer == "plane_b" || layer == "planeb" || layer == "b") return ctx.mutePlaneB;
    if (layer == "window")                                        return ctx.muteWindow;
    if (layer == "sprites" || layer == "sprite")                  return ctx.muteSprites;
    return nullptr;
}
```

**Accepted: 9 strings** — `plane_a`, `planea`, `a`, `plane_b`, `planeb`, `b`, `window`, `sprites`, `sprite`.
**Case-sensitive**: raw `std::string ==` with no `tolower`, so `"Plane_A"`, `"PLANE_A"`, `"Sprites"` are all
rejected. (This is a deliberate local decision, not an oversight — the button-name path on the same server
*is* case-folded, `oracle-old/linux-port/gui/ControlSocket.cpp:1687-1688`.)

**`set_channel_enabled`** (`oracle-old/linux-port/gui/ControlSocket.cpp:1560-1570`). The matcher is
character arithmetic rather than a literal chain (`AudioMuteFlag`,
`oracle-old/linux-port/gui/ControlSocket.cpp:1542-1558`): `dac`; any 3-char `f`,`m`,digit `1`–`6`; any
4-char `psg`+digit `1`–`3`; and `psg_noise` or `noise`.

**Accepted: 12 strings** — `dac`, `fm1`–`fm6`, `psg1`–`psg3`, `psg_noise`, `noise`. Case-sensitive for the
same reason (`ch[0]=='f' && ch[1]=='m'` is a literal lowercase test). Note `psg4` is **rejected** by
`ch[3] <= '3'` even though `mutePsg[3]` exists and *is* the noise channel — so the noise channel is
reachable only by name.

**Unrecognised value: a loud error, not a silent ignore.** Both setters refuse:

```
oracle-old/linux-port/gui/ControlSocket.cpp:1522
    if (!flag) return ErrorReply("unknown layer: " + layer + " (valid: plane_a, plane_b, window, sprites)");
oracle-old/linux-port/gui/ControlSocket.cpp:1566
    if (!flag) return ErrorReply("unknown channel: " + ch + " (valid: fm1..fm6, dac, psg1..psg3, psg_noise)");
```

Neither message matches any `CodeForMessage` substring, so both land on the **default `-32602`**. The intent
is stated in the source: *"Unknown layer names return an error rather than silently ignoring so the agent
notices typos"* (`oracle-old/linux-port/gui/ControlSocket.cpp:1503-1506`).

**Which accepted values were intended as contract — D-17 asks this explicitly, and the source answers it.**
The refusal messages are the server telling you its own intended vocabulary, and they name **only the
canonical spellings**: `plane_a, plane_b, window, sprites` and `fm1..fm6, dac, psg1..psg3, psg_noise`. The
other 5 layer spellings (`planea`, `a`, `planeb`, `b`, `sprite`) and the 1 channel spelling (`noise`) are
**typing conveniences that the server does not advertise, does not echo back normalised, and does not list
when it refuses**. They are accepted; they were not intended as contract.

**Recommendation for the ruling, stated as a preference and not a demand:** adopt the canonical sets only —
4 layers, 11 channels — matching the audit's reading. Under §11.18 an emitted enum cannot be widened later,
but a **request** enum can always be *widened* additively; it is narrowing that breaks clients. So writing
the 4 and the 11 into the rows is the safe direction, and the aliases can be added later by amendment if
anyone turns out to depend on them. Writing 9 and 12 into the contract would permanently bless spellings the
implementation itself declines to name.

**Getter key sets, for the "setter enum *is* the getter key set" tie the audit proposes:**

- `get_layer_states` (`oracle-old/linux-port/gui/ControlSocket.cpp:1528-1537`) emits **exactly 4 keys, all
  bool, all always present**: `plane_a` (`:1532`), `plane_b` (`:1533`), `window` (`:1534`), `sprites`
  (`:1535`). Top-level siblings, no wrapper.
- `get_channel_states` (`oracle-old/linux-port/gui/ControlSocket.cpp:1797-1821`) emits **exactly 11 keys,
  all bool, all always present**: `fm1`–`fm6` (built by `"fm%d"`, `:1811-1812`), `dac` (`:1814`),
  `psg1`–`psg3` (`"psg%d"`, `:1818-1819`), `psg_noise` (`:1821`).

**Verdict: they match, canonical-for-canonical, with one asymmetry each.** Layers: the 4 getter keys are
exactly the 4 canonical setter spellings; the 5 aliases have no getter key. Channels: the 11 getter keys are
a strict subset of the 12 accepted setter values, the only orphan being the alias `noise`. **So the audit's
proposed tie is sound**, and adopting it would be a *narrowing* to the canonical set on both rows.

**Three hazards the tie does not close, reported because D-17 would otherwise ship on top of them:**

1. **The getters are not the setters' inverse for channels.** `set_channel_enabled` writes a pure *mute*
   flag; `get_channel_states` reports **audibility**, folding solo state in
   (`oracle-old/linux-port/gui/ControlSocket.cpp:1801-1808`). So `set_channel_enabled(fm1, true)` followed
   by `get_channel_states()` can legitimately answer `fm1: false` when some other channel is soloed from the
   GUI. There is no solo setter on the bus, so a client can observe this state and never cause or clear it.
   If the two rows are formally tied, this asymmetry should be stated, or a client will read the tie as a
   round-trip guarantee it is not.
2. **An unwired layer reports "unknown".** `LayerMuteFlag` returns the `Context` pointer directly, and those
   pointers default to `nullptr` (`oracle-old/linux-port/gui/ControlSocket.h:117`). A correctly-spelled
   `plane_a` on a server whose GUI never wired the flag produces `"unknown layer: plane_a"` — a
   **not-implemented condition reported as a spelling error**, and classified `-32602` rather than `-32000`.
   The same conflation applies to channels via `ctx.muteFm` (`:1546`, `:1566`). A client cannot distinguish
   "you typed it wrong" from "this server cannot do that", which is precisely the distinction §5's
   refuse-and-name pattern exists to preserve.
3. **`enabled` is not validated.** Both setters call `req.getBool("enabled")` with default `false`
   (`:1523`, `:1567`), and `JsonObj::getBool` (`:153-165`) accepts a string as true only for `"true"`,
   `"1"`, `"yes"`. So `enabled: "on"`, `enabled: "True"`, or an array **silently disables** the layer or
   channel. The reply echoes what was applied so it is self-consistent, but there is no refusal. If the rows
   are being written anyway, `enabled` should be pinned to a JSON boolean.

**Reply shapes of the two setters** (`:1525`, `:1569`): `{layer|channel: string, enabled: bool}` — and the
string echoed is **the caller's spelling, not normalised**, so `set_layer_enabled{layer:"a"}` answers
`{"layer":"a"}`. If the rows adopt the canonical-only enum, that echo becomes a non-issue; if they adopt the
aliases, the echo needs a normalisation rule or the reply teaches clients that `a` is a layer name.


---

# SECONDARY — observed implementation shape, offered as transcription material

## NOT a proposed fragment and NOT a spec claim

**Read this paragraph before the tables.** What follows is a **transcription of what a handler emits**,
nothing more. It is offered as raw material for the contract steward, who decides what becomes spec. It is
**not** a fragment, **not** a proposal, and **not** an assertion that any of these shapes is correct,
intended, or worth preserving. Several of the shapes below are ones we would argue against carrying forward,
and where that is true this document says so — but the decision is the steward's, not ours.

**And the eight are not our server's.** None of `z80_registers`, `read_vdp_registers`, `read_vsram`,
`object_slot`, `object_list`, `player_state`, `call_stack` or `log_tail` is in the reference server's
`METHODS` (`crates/oracle-aether/src/engine.rs:200-423`); all eight answer `-32601`. The shapes below are
read from the **legacy C++ server** in `/home/volence/sonic_hacks/oracle-old`, which is the implementation
the §6 rows were transcribed from. All anchors in this section are relative to that tree. `ok` is stripped
from every reply before it reaches the wire (`linux-port/gui/ControlSocket.cpp:205` and `:2823`), so the key
sets below are post-strip; success is signalled by the presence of JSON-RPC `result`.

### Two of the eight do not exist at all

**`emulator/read_vdp_registers`** and **`emulator/read_vsram`** are **absent from the legacy server too**.
A repo-wide search for either literal returns zero hits in any file type. Neither is in `Handlers()`
(`linux-port/gui/ControlSocket.cpp:2632-2685`), so `AdvertisedMethods()` never lists them and `RunMethod`
answers `-32601` (`linux-port/gui/ControlSocket.cpp:2800`).

**So §6 lines 1137 and 1138 describe methods that no implementation on this bus has ever served.** They are
not "unfragmented because the server is deferred" — they are unfragmented because **there is nothing anywhere
to transcribe**. The steward should know that before treating them as a transcription backlog item: they are
a design task, not a documentation task.

VSRAM *is* reachable on the legacy server, but never as bytes and never as its own method: `state_hash`
folds it into an FNV-1a digest (`linux-port/gui/ControlSocket.cpp:2393-2406`, emitting a `vsram` **hash**
key at `:2406`) and the GUI memory editor exposes it as a panel region
(`linux-port/gui/main_gui.cpp:4755`). The VDP-memory methods that do exist are
`read_vram`/`write_vram`/`read_cram`/`write_cram` (`linux-port/gui/ControlSocket.cpp:2655-2658`). There is
no register-file read of any kind on either server.

**On the reference server the same information is already reachable** — `emulator/read` with
`space: "vsram"` (`crates/oracle-aether/src/engine.rs:1663`, spaces parsed at `engine.rs:3998-4006`), a
live, fragmented method. So `read_vsram` may be a row to **retire** rather than write; that is the steward's
call, and it is raised here because writing a fragment for it would otherwise be the default.

### 1. `emulator/z80_registers` — `linux-port/gui/ControlSocket.cpp:662-686`

Takes no params (the signature discards the `JsonObj`, `:662`). Flat object, **17 keys, all unconditional**
once the handler proceeds.

| keys | type | format | anchor |
|---|---|---|---|
| `pc`, `sp`, `af`, `bc`, `de`, `hl`, `ix`, `iy`, `af2`, `bc2`, `de2`, `hl2` | string | `0x%04X` | `:668-679` |
| `i`, `r` | string | `0x%02X` | `:680-681` |
| `im` | number | unsigned | `:682` |
| `iff1`, `iff2` | boolean | | `:683-684` |

Every register is a **hex string**; `im` is the only numeric register field. No flags decomposition, no
nested structure. Only failure path: `"no Z80"` (`:665`) → `-32000`. **Note it has no `rom loading` guard**,
unlike `z80_read`/`z80_write` — it will answer mid-load.

### 2–3. `read_vdp_registers`, `read_vsram` — do not exist. See above.

### 4. `emulator/object_slot` — `linux-port/gui/ControlSocket.cpp:966-1083`

**Params:** `slot` (int, default 0, `:973`). Range is engine-dependent — `0-65` for `s4_engine`, `0-107`
otherwise (`:972`); out of range → `-32004` (`:974-980`).

**The shape depends on an auto-detected engine (`DetectSST`, called at `:972`) and THERE IS NO `engine` KEY
IN THE REPLY.** A client must discriminate on the presence of `pool`/`code_addr` (s4) versus `id`
(sonic_hack). This is the worst property in the secondary set and we would argue against transcribing it
as-is: it makes the reply's own shape undiscoverable from the reply. It is also **inconsistent with
`player_state` below**, which emits `engine` on one branch and not the other — so the two decoder methods do
not even agree with each other on how to signal the engine.

**Branch A — s4, inactive** (early return `:993`), **4 keys**: `slot` (number, `:990`), `addr` (string
`0x%08X`, `:990`), `pool` (string, `:991`), `active` (bool `false`, `:992`).

**Branch B — s4, active.** The 4 above with `active: true`, plus:

| key | type | presence | anchor |
|---|---|---|---|
| `code_addr` | string `0x%04X` | always in-branch | `:995` |
| `class` | string | **conditional** — only if `S4ClassName(ctx, codeAddr)` is non-empty | `:997` |
| `mapping_symbol` | string | **conditional** — symbols loaded **and** `mapPtr != 0` **and** `mapPtr < 0x400000` **and** a nearest symbol within `0x10000` found | `:1019-1023` |
| `mapping_ptr` | string `0x%08X` | always | `:1024` |
| `art_tile` | string `0x%04X` | always | `:1025` |
| `priority` | number | always | `:1026` |
| `render_flags` | **string** `0x%02X` | always | `:1027` |
| `collision_response` | string `0x%02X` | always | `:1028` |
| `width`, `height` | number | always | `:1029` |
| `x`, `y` | number, signed | always | `:1030` |
| `xvel`, `yvel` | number, signed | always | `:1031` |
| `anim`, `anim_frame` | number | always | `:1032` |
| `mapping_frame` | number | always | `:1033` |
| `subtype` | number | always | `:1034` |
| `status` | **string** `0x%02X` | always | `:1035` |

**Branch C — sonic_hack, inactive** (early return `:1058`), **4 keys**: `slot` (number), `addr` (string
`0x%08X`), `id` (number), `active` (bool `false`) — `:1056-1057`. **No `pool` key**; that absence is the
discriminator.

**Branch D — sonic_hack, active.** The 4 above with `active: true`, plus `class` and `mapping_symbol`
(**both conditional on the same gate and emitted in one statement, so both-or-neither**, `:1066-1069`; note
`class` here is a **trimmed** symbol with a leading `Map_`/`Obj_` stripped, unlike branch B's raw
`S4ClassName`), then `mapping_ptr` (`:1072`), `art_tile` (`:1073`), `render_flags` (`:1074`),
`collision_response` (`:1075`), `width`/`height` (`:1076`), `x`/`y` (`:1077`), `xvel`/`yvel` (`:1078`),
`anim`/`anim_frame` (`:1079`), `subtype` (`:1080`).

**Branch-only keys.** s4 only: `pool`, `code_addr`, `priority`, `mapping_frame`, `status`. sonic_hack only:
`id`. Everything else is common — but **`class` means two different things across the branches** (raw vs
trimmed) under one key name.

Errors: `"rom loading"` (`:968`) → `-32010`; `"slot out of range"` (`:974-980`) → `-32004`; **`"no 68k RAM"`
(`:970`) → `-32602`**, because that string matches none of `CodeForMessage`'s substrings — in particular it
does **not** match `"no 68000"`. A missing-device condition is reported as invalid-params. That is a bug in
the legacy server, reported rather than smoothed.

### 5. `emulator/object_list` — `linux-port/gui/ControlSocket.cpp:1085-1142`

**Params: none** (the signature discards the `JsonObj`, `:1085`). No `limit`, no `cursor`, no filter.

**Top level: exactly ONE key** (`:1141`): `objects` — array, always present, possibly empty. **No `count`,
no `engine`, no `maxSlots`, and none of §2.4 clause (a)'s `total`/`returned`/`truncated`.** The array is
unbounded by construction (≤ 66 or ≤ 108 entries; only active slots appear, `continue` on `codeAddr == 0` at
`:1104` or `id == 0` at `:1127`, so slot numbers are sparse and **presence *is* activity** — there is no
`active` key here).

**Per-item: 5 keys, engine-dependent, again with no `engine` key to say which:**

- s4 (`:1110-1112`): `slot` (number), `pool` (string), `x`, `y` (numbers, signed), `class` (string).
- sonic_hack (`:1134-1136`): `slot` (number), `id` (number), `x`, `y` (numbers, signed), `class` (string).

**`class` is always present but may be `""`** — unlike `object_slot`, where the same fact is spelled as an
*omitted key*. One datum, two absence conventions, on two methods in the same family. If these rows are ever
written that inconsistency should be resolved, not transcribed.

Errors: `"rom loading"` (`:1087`) → `-32010`; `"no 68k RAM"` (`:1089`) → `-32602`, same misclassification.

### 6. `emulator/player_state` — `linux-port/gui/ControlSocket.cpp:1147-1286`

**Params: none** (`:1147`). **The top-level key set differs by engine, and this is the biggest shape hazard
in the eight:**

- **s4 branch** (`:1219-1220`), 3 keys: `engine` (string, literal `"s4_engine"`), `player_1` (object),
  `player_2` (object).
- **sonic_hack branch** (`:1285`), 2 keys: `main` (object), `sidekick` (object). **There is NO `engine` key
  on this branch.**

So `engine` is present on one branch and absent on the other, and a client that branches on `engine` will
mis-handle every sonic_hack reply. The only reliable discriminator is `player_1` vs `main`. We would flag
this as a defect rather than a shape to preserve.

**Nested player object — s4, inactive** (`renderS4`, early return `:1171`), 2 keys: `active` (bool `false`),
`addr` (string `0x%08X`).

**Nested player object — s4, active** (`:1194-1206`), **12 keys, all unconditional in-branch**: `active`
(bool `true`), `addr` (string), `class` (string — **always present, possibly `""`**), `x`, `y`, `xvel`,
`yvel` (numbers, signed), `anim` (number), `mapping_frame` (number), `subtype` (number), `render_flags`
(**number, decimal `%u` — not a hex string, unlike `object_slot.render_flags` at `:1027` on the same
engine**), `status` (object).

`status` sub-object: exactly 2 keys, `raw` (number) and `bits` (array of string) (`:1204`). `bits` lists
only set bits (`bitsList`, `:1154-1164`), so it is frequently empty. Bit names (`stBits`, `:1188-1191`,
index 0–7): `b0`, `xflip`, `yflip`, `in_air`, `rolling`, `on_object`, `pushing`, `underwater`.

**Nested player object — sonic_hack, inactive** (`renderPlayer`, early return `:1227-1232`), 2 keys:
`active` (bool `false`), `addr` (string `0x%08X`).

**Nested player object — sonic_hack, active** (`:1257-1272`), **22 keys, all unconditional in-branch**:
`active` (bool `true`), `addr` (string), `id` (number), `x`, `y`, `xvel`, `yvel` (numbers, signed),
`inertia` (number, signed), `angle` (number), `flip_angle` (number), `status`, `status2`, `status3`
(objects), `air_left`, `move_lock`, `invulnerable_time`, `invincibility_time`, `speedshoes_time`,
`spindash`, `shield`, `layer` (numbers).

Each status object has the same 2-key `{raw, bits}` shape. Bit-name tables (`:1250-1252`):

- `status`: `left`, `air`, `ball`, `onobject`, `rolljump`, `pushing`, `water`, `bit7`
- `status2`: `s2b0`, `s2b1`, `s2b2`, `s2b3`, `s2b4`, `doublejump`, `speedshoes`, `nofriction`
- `status3`: `lock_motion`, `lock_jumping`, `flip_turned`, `stick_convex`, `spindash`, `jumping`, `b6`, `b7`

**The s4 and sonic_hack bit names do not match spelling-for-spelling** (`in_air`/`air`,
`on_object`/`onobject`), so cross-engine bit-name comparison is unsafe. Under §11.18 these are emitted
enums and cannot be widened later — a reason to think hard before any of them becomes contract, and a reason
`status2`'s placeholder names (`s2b0`…`s2b4`) should not be frozen at all.

Errors: `"rom loading"` (`:1149`) → `-32010`; `"no 68k RAM"` (`:1151`) → `-32602`, same misclassification.

### 7. `emulator/call_stack` — `linux-port/gui/ControlSocket.cpp:1291-1347`

**Params:** `max_bytes` (int, **default 256**, `:1297`), `max_frames` (int, **default 24**, `:1298`).
Neither is validated or clamped. `max_bytes` is read unsigned, so a **negative value becomes ~4 billion** and
the scan runs until `max_frames` is satisfied, reading far past the stack.

**Note the parameter names.** §6 line 1373 spells them `maxBytes`/`maxFrames`; the implementation reads
`max_bytes`/`max_frames`. **The row and the only implementation disagree on the parameter names** — a
divergence the audit did not catch, and one that §2.5's params closure would turn from a silently-ignored
param into a hard `-32602`.

**Top level: exactly 3 keys, all always present** (`:1344-1346`): `pc` — **string** `0x%08X` (`:1342`,
emitted `:1344`); `sp` — **string** `0x%08X` (`:1343`, emitted `:1345`); `frames` — array, may be empty
(`:1346`).

**Per-frame: exactly 4 keys, all always present** (`:1338-1340`):

| key | type | note |
|---|---|---|
| `sp_offset` | number | byte offset from SP, always even (the loop steps `off += 2`, `:1330`) |
| `return` | string, `0x%06X` | **six** hex digits — **inconsistent with `pc`/`sp`'s eight in the same reply** |
| `symbol` | string | **always present, possibly `""`** when no symbols are loaded or nothing lies within `0x1000` (`:1334-1336`) |
| `disp` | number | displacement from `symbol`; `0` when `symbol` is `""` |

**The frames are heuristic, not an unwind.** `looksLikeReturn` (`:1300-1326`) keeps a stack word only if it
is even, non-zero, below `romBytes`, and preceded 2/4/6 bytes earlier by something decoding as `BSR`
(`0x61xx`) or `JSR` (`0x4Exx`). Frames may be spurious or missing, `sp_offset` is not a frame-chain link,
and **there is no confidence field and no `caveat`**. This is §2.4's exact use case — an answer weaker than
its shape suggests — and the method emits nothing to say so. If this row is ever fragmented we would argue
it needs `caveat` declared *and* emitted.

Errors: `"rom loading"` (`:1293`) → `-32010`; `"missing 68k/cart/ram"` (`:1295`) → `-32602`, the same
classifier miss as `object_slot`'s.

### 8. `emulator/log_tail` — `linux-port/gui/ControlSocket.cpp:1838-1887`

**Params:** `since` (uint, default 0, `:1842`), `limit` (uint, **default 100**, `:1843`). **No upper clamp
on `limit`** — `limit: 1000000` is accepted and returns the whole ring.

**Top level: exactly 2 keys, both always present:** `token` — number (`:1841`, emitted `:1852`, from
`GetEventLogLastModifiedToken()`, intended to be handed back as the next call's `since`); `entries` — array,
**newest-first** (comment `:1854-1856`), possibly empty (`:1885`).

**Per-entry: exactly 4 keys, all strings, all always present** (`:1878-1883`):

| key | type | note |
|---|---|---|
| `level` | string | **exact value set `debug`, `info`, `warning`, `error`, `critical`** (switch `:1869-1877`). `"info"` is both the initialiser (`:1868`) and the `default:` fallthrough, so **an unmapped level silently reports as `info`** |
| `source` | string | wide→ASCII via `wToStr` (`:1858-1862`) |
| `text` | string | same |
| `time` | string | same; a formatted string, **not** a numeric timestamp |

`wToStr` **replaces every character outside `[0x20, 0x7F)` with `?`**. Non-ASCII log text is lossily
mangled, not escaped. A row pinning these as strings without saying so would be pinning a lossy channel.

**This bears directly on §10's open `token`/`since` question** (`contract/protocol.md:2188`, `:2295`,
`:2592`). Two findings:

1. **`since` is a count heuristic, not a watermark.** The arithmetic at `:1844-1849` narrows `want` to
   `currentToken - since` when `0 < since <= currentToken`, but the loop then takes the **first `want`
   entries of the newest-first list** (`:1866`) — it never filters per-entry by token. If entries were
   evicted from the ring between polls, the result can **skip or repeat** entries, and the caller cannot
   tell.
2. **There is no `dropped`, `truncated` or `total` key**, so that gap is silent. The only signal is `token`
   jumping by more than the number of entries returned. That is §2.4 clause (a)'s failure mode exactly — a
   partial list a client can mistake for a complete one.

**The source does not settle** exactly how `GetEventLogLastModifiedToken` relates to ring eviction; that
would need the Exodus `System` class's event-log ring implementation, which was not opened. Naming it
matters because the answer decides whether `since` can be repaired into a real watermark or whether the row
needs a different continuation design.

Error: `"system not wired"` (`:1840`) → `-32000` (matches `"not wired"`).

### Cross-cutting properties of the legacy replies, for whoever transcribes them

1. **Four of these methods return hand-built raw JSON strings** rather than `JsonWriter` output:
   `breakpoint_list` (`:831`), `object_list` (`:1141`), `player_state` (`:1219`, `:1285`), `call_stack`
   (`:1344`). They are re-parsed by `json::parse` in the dispatcher (`:2822`), so a malformed fragment
   surfaces as a **`-32603`** from the transport rather than as a handler error. Items are formatted into
   fixed buffers — `char entry[256]` (`:1108`, `:1337`), `char out[512]` (`:1193`), `char out[1024]`
   (`:1256`) — so a pathologically long `class` symbol could truncate mid-JSON. Not reachable with ordinary
   symbol lengths, and **the source does not bound `class`'s length**, so it cannot be ruled out from source
   alone.
2. **Hex-versus-number typing is inconsistent for the same concept across handlers**: `render_flags` is a
   hex **string** in `object_slot` (`:1027`, `:1074`) and a decimal **number** in `player_state` (`:1204`);
   `call_stack.return` is `0x%06X` while `pc`/`sp` in the same reply are `0x%08X`. Any fragment written from
   these replies would freeze the inconsistency.
3. **`"no 68k RAM"` and `"missing 68k/cart/ram"` both fall through `CodeForMessage` to `-32602`**
   (`:211-222`), although they are wiring/availability failures that belong with the `-32000` family. If a
   client switches on error codes, this is a live misclassification on three of the six methods above.
4. **`z80_read`'s reply echoes `bytes`; `z80_write`'s does not** — it returns only `addr` and `len`
   (`:748`). Noted because D-10 is about that pair and the asymmetry is not in the row.

---

## Where the source does not settle things

Recorded honestly rather than filled in:

1. **`breakpoint_list`'s ordering across calls.** `GetBreakpointList`
   (`oracle-old/ExodusSDK/Processor/Processor.cpp:371-376`) copies the `std::vector` into a `std::list` in
   vector order, and `DeleteBreakpoint` erases from the middle
   (`oracle-old/ExodusSDK/Processor/Processor.cpp:503`), so order is *incidentally* insertion order — but
   nothing promises it, and `LockBreakpoint`'s `continue`
   (`oracle-old/linux-port/gui/ControlSocket.cpp:821`) can silently skip an entry under concurrent GUI
   access with no indication in the reply. Since replies carry no id, index-as-identity is unusable anyway.
   **The source does not settle this**; a stated ordering guarantee in the row would.
2. **Which engine `DetectSST` picks for a given ROM.** The reply-shape consequences are catalogued above,
   but the detection predicate itself was not read. **The source does not settle this** from what was
   examined; reading `DetectSST`, `S4ClassName` and `S4PoolName` in
   `oracle-old/linux-port/gui/ControlSocket.cpp` would.
3. **Whether `breakpoint_clear` can race a concurrent GUI delete.** At
   `oracle-old/linux-port/gui/ControlSocket.cpp:851-856` the code locks, reads, unlocks, and only then calls
   `DeleteBreakpoint(bp)` outside the lock. **The source does not settle** whether `Processor`'s debug mutex
   makes that safe; the full `DeleteBreakpoint` body plus `Processor.cpp`'s locking discipline would.
4. **How `log_tail`'s token relates to ring eviction** — see §8 above.
5. **What the reference server will do when these families land.** Nothing here predicts it. Every "(A)"
   answer in this document is an absence, and an absence constrains nothing.

Nothing in this document was checked against a running emulator, and no `mcp__oracle__*` tool was called. It
is a source reading end to end, on the same discipline the audit itself adopted — *"nothing in this pass was
checked against a running Oracle, deliberately"*
(`empyrean/docs/2026-08-22-protocol-schema-audit.md` §5, blob `864276db`).
