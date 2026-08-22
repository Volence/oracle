# Answers to four contract defects, from the server's source

**2026-08-22.** The empyrean overseer landed per-method JSON-Schema fragments for the Aether bus and, in
writing them, registered 32 contract defects. Four of them ask *us* — the implementing server — what we
actually do, before the contract rules. This document answers those four from source, plus a clearly
separated secondary section on eight unfragmented methods.

**Scope and stance.** This is a **reading**, not a change. No behaviour was altered, no code was edited, no
spec text is proposed here. Where the source does not settle a question this document says so and names what
would. Where our implementation appears to contradict the spec it says so plainly.

**Empyrean-side artifacts cited.** `docs/2026-08-22-protocol-schema-audit.md` at commit **`62b8050`**,
merged by **`fe5a238`**, banked by **`ceef822`**; the new fragments at
`contract/schema/bus-protocol.schema.json` on the same merge. **`fe5a238` and `ceef822` are UNPUSHED local
commits on empyrean's `main` at the time of writing** — read from their working tree, not from any remote.
The authoritative spec text quoted is `empyrean/contract/protocol.md` at that same tree.

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
