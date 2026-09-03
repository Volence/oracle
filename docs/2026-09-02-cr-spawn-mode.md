# CR-J — spawn mode: three object-mutation rows, and the engine's five refusals kept as refusals

**Filed by:** the oracle lane (the ground-up Rust core + Aether server, `oracle/`), 2026-09-02.
**Against:** `empyrean` `contract/protocol.md` §5, §6 (*run-control state rule*, the *object / player
decoders ⚙* group and its four normative rules) and `contract/schema/bus-protocol.schema.json`, read at
`empyrean` **`82982b7`** — the revision this repo's schema copy is pinned to
(`crates/oracle-aether/tests/contract/PROVENANCE.md`, `pin.revision`).
**Engine grounding:** `aeon` **`36285940`** ("merge: chain 206 — the live-object spawn mailbox"),
verified an ancestor of their `origin/master` and read **through git objects at that revision**, never
through their working tree (`empyrean/contract/SUITE_PATHS.md` at `38f6df4`, *"What a resolver owes its
reader"*, and this repo's own `F-SCHEMA-READS-LIVE-EMPYREAN`).
**Closes:** no audit defect. This is new bus surface.

> **Filing-path note.** This lane's proposed CRs live in `docs/proposed/` (`cr-h`, `cr-i`). This one is
> filed at the path the dispatching seat named. If it is adopted as a contract amendment it should be
> moved or copied under `docs/proposed/` for consistency; nothing else about it changes.

---

## 0. How to read this document

**Nothing was compiled, run, or measured for this document.** No `cargo` command was executed, no
emulator was contacted, no ROM was built. Another agent holds the cargo lane. Every behavioural claim
about *this* server is a claim about **source text**, cited by file and line at this worktree's `main`
(`0a2446f`); every claim about aeon is cited by file and line at `36285940`. Anything I could not
establish that way is marked **UNMEASURED** and appears again in §14.

- §1 is the ask. §2 is the evidence base, including **three places where this repo's own transcription
  of aeon's interface is wrong or misleading** (§2.4) — one of them materially.
- §3 states the problem for a reader who has never seen either repo.
- §4–§8 are the five decisions the dispatch asked to be *argued*, one section each.
- §9 is the proposed surface in full; §10 the contract deltas.
- §11 is the three-surface question (MCP / plain Aether / player GUI).
- §12 is the better-approach pass; §13 the alternatives rejected; §14 where this CR is weakest.
- §15 is the questions handed to an adjudicator **unanswered**, because I could not defensibly settle
  them without running something.

---

## 1. Summary and the ask

aeon has landed an in-RAM mailbox by which an external tool can **spawn, move and delete live objects**
in a running game (aeon `36285940`, `games/sonic4/config/ram.emp:268-322`). It is DEBUG-shape only. The
engine half is done. Nothing on this bus reaches it.

**The proposal, in one line:** three narrow, symbol-resolved, paused-only bus rows —
`emulator/object_spawn`, `emulator/object_move`, `emulator/object_delete` — that own the whole mailbox
handshake server-side, and translate each of the engine's five refusals into a **typed error**, never
into a result a client can mistake for success.

| # | Change |
|---|---|
| **J1** | Three new ⚙ rows in §6's *object / player decoders* group: `object_spawn`, `object_move`, `object_delete`. One method per op, not one method with an `op` param (§4). |
| **J2** | The server performs the mailbox handshake — payload writes, **flag last**, frame advance, ack detection, status read — as one indivisible engine-thread operation. The client never sees a flag (§5). |
| **J3** | The engine's five status codes map to typed JSON-RPC errors, with `-32005` `data.reason` discriminants for the four state-shaped ones. A refusal is **never** a `result` (§6). |
| **J4** | All three rows join §6's **run-control state rule** (paused required, `-32005` `machineRunning`). A new optional `expectFrameToken` param closes the client half of aeon's stale-handle hazard; the residual window is stated rather than hidden (§7). |
| **J5** | The mailbox is resolved **by symbol, per build, every call**. A build whose symbol table lacks any one of the eight `Obj_Req_*` names is refused by name with `-32013` (or `-32012` if no table is loaded). A release ROM therefore **cannot** be written to at a guessed address (§8). |

**What is deliberately *not* proposed:** no new read row (the client already has `object_list`), no
archetype catalogue row (`lookup_symbol`'s prefix search already answers it), no queue, no persistence,
no lifetime tracking — because the engine has none of those and inventing them here would put state on
this side of the seam that the engine cannot honour (§13.4).

---

## 2. Evidence base

### 2.1 Sources, and the revision each was read at

| # | source | revision | what it establishes |
|---|---|---|---|
| S1 | `aeon:games/sonic4/config/ram.emp` | `36285940` | the eight cells, their widths, their **order**, the flag-last rule (`:277-281`), one-per-frame (`:283-287`), status-is-not-the-ack (`:288-293`) |
| S2 | `aeon:games/sonic4/test/ojz_scroll_test.emp` | `36285940` | the consumer `objreq_consume` (`:378-513`), the op/status constants (`:292-301`), the `$60FF` place mask (`:325`), the cart-window rail (`:319`), the single instantiation site (`:902`) |
| S3 | `aeon:tools/test_object_mailbox_contract.py` | `36285940` | that (S1, S2, S4) are held against each other by a build-fatal gate, in **both** directions, with the order asserted and eight proven-red mutations recorded |
| S4 | `aeon:docs/ENGINE_ARCHITECTURE.md` §4.12c | `36285940` | the published field/status tables and the protocol prose (`:3158-3215`) |
| S5 | `aeon:engine/system/constants.emp:956-957` | `36285940` | `OEF_YFLIP = 14`, `OEF_XFLIP = 13` |
| S6 | `aeon:engine/objects/load_object.emp:25,29-35,63-66` | `36285940` | `Load_Object`'s signature and the `rol.w #4` flip fold |
| S7 | `aeon:engine/coords.emp:24-29` | `36285940` | `pixels_to_coord` = `swap` + `clr.w` |
| S8 | `aeon:engine/objects/core.emp:471`, `aeon:engine/ram.emp:699` | `36285940` | `Game_Paused` is the **game's** pause flag, tested at the top of `RunObjects` |
| S9 | `empyrean:contract/protocol.md` §5, §6 | `82982b7` | the error table, the run-control state rule, the ⚙ group's four normative rules |
| S10 | `oracle:crates/oracle-aether/src/{engine,rpc,decoders,hex}.rs` | `0a2446f` | this server's `require_paused`, error constructors, object layout derivation, hex conventions |
| S11 | `oracle:docs/2026-09-02-aeon-spawn-mailbox.md` | `0a2446f` | this repo's transcription of S1/S4 — corrected in §2.4 |

### 2.2 The interface, re-derived from S1 rather than from S11

Read out of `ram.emp` at `36285940`, in declaration order, inside the
`if DEBUG == 1 @shape_divergent` group:

| order | symbol | width | dir | derived offset from `Obj_Req_Def` |
|---|---|---|---|---|
| 1 | `Obj_Req_Def` | `u32` | C→E | +0 |
| 2 | `Obj_Req_X` | `u16` | C→E | +4 |
| 3 | `Obj_Req_Y` | `u16` | C→E | +6 |
| 4 | `Obj_Req_Slot` | `u16` | **both** | +8 |
| 5 | `Obj_Req_Place` | `u16` | C→E | +10 |
| 6 | `Obj_Req_Op` | `u8` | C→E | +12 |
| 7 | `Obj_Req_Status` | `u8` | E→C | +13 |
| 8 | `Obj_Req_Flag` | `u8` | **both** | +14 |
| — | `pad(1)` | | | +15 |

Ops: `1` SPAWN, `2` MOVE, `3` DELETE (S2 `:292-294`). Statuses: `0` OK, `1` bad op, `2` bad def,
`3` pool full, `4` bad slot, `5` owned (S2 `:296-301`).

**The offset column above is a derivation, and this CR forbids using it.** It is here so a reader can
check S11's table, and for no other purpose. §8 is the rule.

### 2.3 What the consumer actually does, read from S2

Cold-path, at most one request, in this order (`ojz_scroll_test.emp:378-513`):

1. `tst.b Obj_Req_Flag`; zero ⇒ one read and a branch, nothing else happens.
2. Dispatch on `Obj_Req_Op`; anything but 1/2/3 ⇒ status `1`, ack.
3. **SPAWN** — four rails on `Obj_Req_Def`: nonzero, **even**, `< $400000`, and a nonzero head word
   (`ObjDef.code_addr`). Then `Obj_Req_Place` is masked with `$60FF` and `Load_Object` is called. On
   allocation failure ⇒ status `3`, **nothing evicted**. On success the new SST pointer's low word is
   published into `Obj_Req_Slot` and status is `0`.
4. **MOVE/DELETE** — the handle is rejected if zero, then linearly searched in `Dynamic_Live` bounded
   by `Dynamic_Live_Count`; a miss ⇒ status `4`. A hit whose `Sst.code_addr` is zero (a deleted but
   not-yet-compacted entry) ⇒ status `4` as well.
5. **MOVE** writes `Obj_Req_X`/`Y` through `pixels_to_coord` into `Sst.x_pos`/`y_pos` and touches
   nothing else — no clamp, no velocity reset, no animation reset.
6. **DELETE** refuses any slot whose `Sst.slot_tag != TagRef.none` with status `5`, because the entity
   window clears its own loaded bit before deleting and a bare `DeleteObject` here would skip it.
7. `clr.b Obj_Req_Flag` — **the last write of the consumption**, on every path.

It is spliced inline **once**, at `GameState_OJZScroll_Update`'s frame top, **after**
`Debug_Warp_Consume` and **before** `RunObjects` (S2 `:877-903`).

### 2.4 Three corrections to this repo's transcription (S11), one of them material

**(a) MATERIAL — "paused" in aeon's prose means the *game's* pause, not the emulator's.** S11 says
*"Requests are consumed on a paused frame; the object first ticks on resume."* S4 `:3189` says *"It
works while the game is PAUSED … the frame top runs before `RunObjects`, whose `Game_Paused` test is
what routes to the render-only pass."* `Game_Paused` is declared at `aeon:engine/ram.emp:699` as the
*"game pause / freeze flag"* and tested at `aeon:engine/objects/core.emp:471`, the first instruction of
`RunObjects` (S8). It is the Start-button pause **inside the game**.

The emulator's pause is a different thing entirely: when *this* server is paused **no frames execute at
all**, so `objreq_consume` never runs, the flag is never cleared, and the request is never consumed.
A design that wrote the mailbox and then waited for an ack **without advancing the machine would hang
forever, on a correctly-working engine.** Anyone reading S11 and building against it would write that
design. This is the single most important thing this CR had to get right, and S11 as written points the
other way. §5 and §7 are built on the corrected reading.

**(b) MISLEADING — no field of `emulator/object_list` *is* the handle.** S11 (and S4 `:3169`, whose
wording S11 inherited) say the handle is *"exactly what oracle's `object_list` reports."* It is not,
quite. `DecodedRecord::to_json` (`crates/oracle-aether/src/decoders.rs:636-637`) emits `slot` — a **pool
index** — and `addr` — the **full 32-bit** SST address as `hex::addr`, i.e. `"0xFFFF8DC2"`
(`crates/oracle-aether/src/hex.rs:15-17`). The handle aeon wants is the **low 16 bits of `addr`**. The
join is real but it is an arithmetic step, and `slot` and `handle` are both small integers that both
plausibly mean "the slot", which is exactly the confusion that produces a wrong request. §9 makes the
server perform that step so no client has to (and lets a client address by `slot` instead).

**(c) IMPRECISE — "the level state" is one game state, in a test file.** S11 says *"Outside the level
state the flag is never acked."* Concretely, the consumer is instantiated exactly once, in
`GameState_OJZScroll_Update` (S2 `:902`), which lives in `games/sonic4/test/ojz_scroll_test.emp`. It is
not an engine-wide facility and it is not present in every level state — there is one. A client is
otherwise correct to poll with a timeout, but this CR must not imply a general "in a level" precondition
that the ROM does not implement. **UNMEASURED:** whether other game states gain the splice later.

**What S11 got right:** the field list, the widths, the order, the offsets, the op and status codes, the
flag-last rule, the one-per-frame bound, the dynamic-pool-only reach, the free-slot policy and the
DELETE asymmetry all re-derive correctly from S1/S2/S4. The transcription is good; the two readings
above are where it would have cost us.

### 2.5 One derived fact aeon does not state, which a client needs

Which flip bit is which. S1 `:310-312` and S4 `:3169` say only *"OEF flips in bits 13/14"*.
`aeon:engine/system/constants.emp:956-957` (S5) settles it: `OEF_XFLIP = 13`, `OEF_YFLIP = 14`, and
`load_object.emp:25` carries a build-time `ensure` that ties the `rol.w #4` fold to it. §9's structured
`flipH`/`flipV` params rest on S5, not on S4.

---

## 3. The problem, stated cold

A debugging client that can already *see* a running game's objects (`emulator/object_list`) cannot
*change* them. It can move a camera, read a slot, name the code that owns it — and then has no way to
put a spring where the level does not have one, or take away the badnik it is trying to reproduce a
bug against. Every such experiment today is a ROM edit and a rebuild.

The engine now offers the missing half, but it offers it as **a raw memory protocol**: eight cells, a
write order that is load-bearing, a flag whose clearing means *the engine looked* and not *the engine
did it*, a per-frame budget of exactly one request, and five distinct refusals that are **silent** —
they write a status byte and clear the flag, exactly as a success does.

That last property is the whole danger. A client that writes a request, waits for the flag to clear and
reports "spawned" is **wrong four times out of five in the failure cases and looks right every time**.
It is the same failure class this contract names repeatedly and this suite has been bitten by: *a
plausible wrong answer instead of an error.*

And there is a second, quieter one. `Obj_Req_Status` is a **latch** — nothing clears it between
requests (S2 `:378-513`: every path writes it, no path resets it). A client that polls status without
having first observed *its own* flag go 1→0 can read a stale `0` from somebody else's earlier request
and call it success.

---

## 4. Decision 1 — three methods, not one `object_request { op }`

**Decided: three methods.** The argument, and it is not the obvious one.

**For one method.** It mirrors the engine 1:1 — one mailbox, one op byte, one row. A future fourth op
(aeon has room: op values 4..255 are refused today with status `1`, S2 `:298`) needs no contract change
if the `op` enum is open, and needs one schema edit if it is closed. The three ops share ~90% of an
implementation regardless, so three rows are three names over one body.

**For three methods, which wins.**

1. **Servedness is per-method on this bus.** D4 makes `initialize`'s `methods` list authoritative, and
   §6's ⚙ note makes *"per-row servedness remaining `methods` membership (item 23)"* explicit for this
   very group. With one row, *"this build can spawn"* and *"this build can delete"* are **the same
   bit** — a server that wanted to serve placement without destruction could not say so, and a client
   could not ask. That is a capability the shape would delete, not merely fail to provide.
2. **§2.5 closes request params.** A single row's params are op-dependent in three directions: `def`,
   `subtype`, `flipH`, `flipV` are SPAWN-only; `handle`/`slot` is MOVE/DELETE-only; `x`/`y` are
   SPAWN/MOVE-only. Expressing that against a closed param object means an `if`/`then`/`else` chain
   keyed on `op`, and every one of those branches is a param set that a reader has to reassemble in
   their head. Three closed schemas say the same thing by being three schemas. §6's `emulator/read`
   note is the precedent for the preference — its `symbol`-only-with-`space:"bus"` conditionality is
   called out as *"enforced in the schema, in both directions, rather than left to prose"* precisely
   because conditionality is the expensive part.
3. **The results genuinely differ.** SPAWN **returns a handle the client did not have**; MOVE and
   DELETE consume one. A single row's result would carry `handle` on one op and not the others, which
   §2.4's rules and §11.5's *"`released`'s defect with a useful name"* both push against.
4. **The family is already shaped this way.** `object_slot`, `object_list`, `object_at` are three
   narrow rows over one decoder, not one `object_query { op }`. A fourth shape in the same group would
   be the odd one.

**What three costs.** Three schema fragments, three `methods` entries, three MCP tools, three rows in
every conformance table — for one engine mechanism. And if aeon adds an op, this bus needs a new row
rather than a new enum value. Both are accepted; the second is genuinely a cost and is named again in
§14.

---

## 5. Decision 2 — the server hides the mailbox protocol

**Decided: hide it.** The client says "spawn this here"; the server does the dance and answers with a
handle or an error. No flag, no status byte, no op code and no cell address appears on the wire.

### 5.1 The argument

**(a) The protocol's guarantee does not survive the round trip.** Flag-last is a *memory-ordering*
discipline between a writer and a consumer that can run between any two of the writer's stores. Over
JSON-RPC it becomes: `write_memory` the payload, `write_memory` the flag — **two requests**, with the
whole bus, the scheduler and any other client in between. It happens to be safe on *this* server today
because `require_paused` means no frames tick between them, so the consumer cannot run — but that
safety is an accident of our pause model that **no contract text states**, and it evaporates the moment
a client resumes between the two calls, or a second client is talking to the same server. Handing a
client a rule whose correctness depends on an unstated property of the server it is talking to is worse
than not handing it the rule.

**(b) The ack is the part that gets built wrong.** The correct wait is: *observe my own flag go 1→0,
then read status.* The natural client implementation is: *poll status.* Because status is a latch (§3),
the natural implementation reports the previous request's outcome, and does so **only sometimes** — the
intermittent bug. This is exactly the shape of failure the dispatch predicted and it is why hiding
carries the argument rather than merely being convenient.

**(c) Hiding removes nothing.** `lookup_symbol` + `write_memory` + `run_frames` + `read` are all still
served. Any client that wants the raw mailbox — to batch a payload, to fire-and-forget, to reach an op
this CR does not know about — can still have it, byte for byte. This CR proposes a **convenience over a
surface that stays reachable**, which is the cheapest kind of abstraction to be wrong about: if the
three rows are shaped badly, nothing is trapped behind them.

### 5.2 What is lost, honestly

1. **Fire-and-forget is gone from the typed surface.** A client that wants to arm a request and let the
   game run to consume it cannot do that through these rows; they block until the ack or refuse. That
   is a real use case (arm a spawn, resume, watch it appear) and this CR does not serve it. The raw
   path serves it, and a later `object_request_arm` row could, but proposing one now would double the
   surface for a use case nobody has asked for.
2. **Forward compatibility is on us.** A new engine op does not reach clients until this bus grows a
   row. With a passthrough it would reach them the day aeon merged it.
3. **The server now advances the machine inside a write-shaped method.** This is new behaviour for this
   server and it is the single largest thing this CR asks for. It is made visible rather than hidden:
   see §7.3 and `framesAdvanced` in §9.
4. **The abstraction can lie about which object it touched.** Not because it hides the protocol — the
   raw path has the same hole — but it is the abstraction's reply that will be believed. §7.2.

---

## 6. Decision 3 — how a refusal reaches the client

**Decided: typed errors, one discriminant per engine status. A refusal is never a `result`.**

The rule this follows: **`-32602` when the client can fix it by changing the request; `-32005` when only
the machine's state can change the answer.** That is §5's own split (*"the params are fine"* /
*"wrong right now"*), and §5's worked examples of `-32005` include `unknownCheckpoint` and
`unknownBreakpoint` — a client-supplied identifier that named something real and no longer does. A stale
object handle is that, exactly.

| engine status | proposed error | `data.reason` | why |
|---|---|---|---|
| `0` OK | — (a `result`) | — | |
| `1` bad op | **`-32603`** | — | **Unreachable by construction:** the server owns the op byte and writes only 1/2/3. If it is ever observed, the server wrote an op the engine does not know — our bug, not the caller's, and an internal error is the only honest code. The mapping exists so the case is *named* rather than silently impossible. |
| `2` bad def | **`-32602`** | — | The `def` the caller chose failed the archetype rails in *this* build. It is a bad param and a different param fixes it. `error.data` carries `def` and **which rail failed is not knowable** — the engine returns one code for four rails — so `data` names all four rather than guessing. See §15 Q1: this is the weakest of the five. |
| `3` pool full | **`-32005`** | `objectPoolFull` | Textbook wrong-right-now: the same request succeeds a frame later. `data` carries the dynamic pool's size from `layout` when it resolves. **Nothing was evicted** — the message must say so, because a client's next instinct is to retry harder. |
| `4` bad slot | **`-32005`** | `unknownSlot` | The `unknownCheckpoint`/`unknownBreakpoint` precedent verbatim. Covers three distinct realities the engine cannot distinguish: the slot died, the handle was never a dynamic slot (a player, system or effect handle — see below), or the handle is malformed. `data` carries `handle`. |
| `5` owned | **`-32005`** | `slotOwnedByEntityWindow` | Refused *for coherence*, not because the request is malformed. `message` must name the fix: this slot is the entity window's; it despawns on its own, and MOVE is allowed on it. |

Plus two refusals that are the server's, not the engine's:

| condition | error | `data.reason` |
|---|---|---|
| the request was never acked within `maxFrames` | **`-32005`** | `mailboxNotConsumed` |
| `expectFrameToken` does not match the current frame | **`-32005`** | `frameMoved` |

**`unknownSlot` deserves a message, not just a code.** §6's ⚙ note observes that a picker listing *every*
object will hand back a player or a system handle, and S4 `:3200` calls status `4` *"the right answer
rather than a gap"* for that case. But `-32005 unknownSlot` on a player is a **confusing** right answer.
The server can do better cheaply: it already derives pool partitions
(`decoders::derive` → `layout.pools`, `crates/oracle-aether/src/decoders.rs:178-181,497-535`), so before
writing anything it can check whether the addressed slot lies in the `dynamic` pool and, if not, refuse
**pre-flight** with `-32602` and a message that says *this row reaches the dynamic pool only; moving the
player is `Debug_Warp_*`'s job.* That is a better error for a caller than the engine's, arrives without
burning a frame, and cannot disagree with the engine because it only fires where the engine would have
said `4` anyway. **UNMEASURED:** that `layout.pools` resolves on the DEBUG build in question; when it
does not resolve, the pre-flight check is skipped and the engine's `4` stands (`pools` is optional —
`decoders.rs:468-471`).

**Why not return the status in a `result`.** Because `{ "status": 3 }` is a 200 with a sad face. Every
client that forgets to branch on it reports success, which is precisely the degradation §3 names, and
this bus has an error channel with a machine-readable discriminant built for the purpose. The engine's
own reason for making the refusals silent — *"a refused request must never fall back to guessing"*
(S4 `:3185`) — is an argument for making them **loud** at this layer, not for propagating the silence.

---

## 7. Decision 4 — the paused-frame requirement, and what `require_paused` does not cover

### 7.1 Paused is required, and the existing gate is the right one for the reasons it already gives

All three rows are writes. §6's run-control state rule already names `write_memory`, `write_cram` and
`z80_write`; these belong on that list for the same reason, and `Engine::require_paused`
(`crates/oracle-aether/src/engine.rs:2495-2506`) is the mechanism unchanged. **`-32005`,
`data.reason = "machineRunning"`, never an implicit pause** (§5's explicit prohibition).

### 7.2 It does **not** cover the stale-handle hazard, and neither does aeon's own recipe

S4 `:3210` states the hazard and the rule: *"A handle is an address … list and request from the same
paused frame, which a tool driving a paused emulator satisfies trivially."*

**It does not satisfy it trivially, and this is the finding this section exists for.** `require_paused`
establishes that the machine is not free-running. It establishes **nothing about where in the frame it
stopped.** This server pauses at an *instruction boundary* — a breakpoint inside object code, a `step`,
a watchpoint hit — and `Engine::frame()` is `scheduler().now() / MCLK_PER_FRAME`
(`engine.rs:2506-2508`), a division, not a boundary assertion.

So consider a client paused mid-frame at a breakpoint inside `RunObjects`:

1. It calls `object_list`. Slot *k* is live at `$FFFF8E62`.
2. It calls `object_move { handle: 0x8E62 }`.
3. The server writes the mailbox and advances the machine.
4. **The rest of this frame's `RunObjects` runs first.** Object code deletes slot *k*; `DeleteObject`
   pushes it onto the dynamic free stack; a later object in the same pass spawns something and
   `AllocDynamic` pops that very slot.
5. *Then* the next frame's top runs `objreq_consume`, which finds `0x8E62` in `Dynamic_Live`, alive and
   well — **and moves the new occupant.**

Status `0`. A clean success. The wrong object moved. Aeon's consumer *cannot* detect this — S4 `:3210`
says as much — and neither can a server that only knows the machine is "paused".

**What closes it, and what does not:**

- **`expectFrameToken` (proposed, J4).** The client passes the `frameToken` from the machine stamp on
  its `object_list` reply (§2.2, D11 — every reply already carries one). The server refuses with
  `-32005 frameMoved` unless the machine is still at that frame. This closes the **client→server** half
  completely: the listing and the request are provably from the same paused instant. It is cheap, it
  uses a field that already exists on every reply, and it costs a client that does not care exactly one
  optional param it can omit.
- **Nothing closes the server→consume half from outside the machine.** Between the server's write and
  `objreq_consume` there is, by construction, *the remainder of the current frame's object code*. If
  the machine was paused at a frame top, that remainder is empty and the window is genuinely closed. If
  it was paused mid-frame, it is not. This CR's position: **say so**, on the row and in the schema
  description, and let a client that cares run to a frame boundary first (`run_frames` then pause) —
  which is what a placement UI does anyway.
- **A `-32005 pausedMidFrame` refusal** would close it hard, and I am not proposing it, because I
  cannot establish without running the emulator that this server can tell "at the game's frame top"
  from "at a scheduler frame boundary" — those are different instants, and the one that matters is the
  game's. **UNMEASURED**, and §15 Q3.

### 7.3 The frame advance, which is the part that needs the adjudicator's eye

Because the emulator's pause stops all execution (§2.4(a)), a server that owns the handshake **must
advance the machine** to collect the ack. Concretely: write payload → write flag → advance until
`Obj_Req_Flag == 0` or `maxFrames` is exhausted → read `Obj_Req_Status` → leave the machine paused.

This is a timeline mutation inside a method whose name says "write". Three things make it defensible:

1. **It is not an implicit mode change.** §5 forbids *"pausing a running machine in order to service a
   `run_frames`, and leaving it paused"* — a change to the machine's **mode** the caller did not ask
   for. This changes no mode: the machine is paused before and paused after. It changes the machine's
   **position**, which is what `step`, `run_to` and `run_frames` all do, all under the same paused
   precondition.
2. **It is reported.** `framesAdvanced` on every reply, success **and** failure (`error.data`), plus
   the stamp's own `frameToken` which moves visibly. A caller can always reconstruct where it is.
3. **It is bounded and the bound is the caller's.** `maxFrames`, default **2**, mirroring `run_to`'s
   `maxFrames` (def 600) in shape if not in value. Two, not one, because from a mid-frame pause the
   first advanced frame may not reach a frame top.

**On a timeout, the server clears the flag.** This is the one write in this design that the client did
not ask for, so the argument has to be explicit. Leaving the flag set means the request stays armed and
may fire **minutes later**, when the game happens to enter the state whose update carries the consumer
— long after the client was told it failed. That is a spontaneous world-change traced to an error
reply, which is strictly worse than a cancelled request. The clear is race-free because the machine is
paused when it happens (no consumer can be mid-consumption). It is reported as
`error.data.cancelled: true`, and `Obj_Req_Op` is deliberately left alone, preserving aeon's stated
property that a watchpoint can still see what the last request was (S1 `:313-316`). §15 Q2 hands the
call up anyway.

### 7.4 Concurrency: one mailbox, several clients

There is one mailbox and no queue. Two overlapping requests silently lose one. This CR requires that
each row's entire write→advance→read cycle be **one indivisible operation on the engine thread**, so
that two calls to this server serialise. **UNMEASURED:** that the engine loop is in fact single-request
(it appears to be — one `Engine` owning `sys`), but the requirement is stated as a requirement, not as
an observation. What this server cannot defend against is another client poking the same cells with
`write_memory`; that is inherent to a raw-memory mailbox and belongs in the row's description, not in a
lock.

---

## 8. Decision 5 — symbol absence must refuse by name, and the refusal is the safety property

**Decided:** resolve all eight `Obj_Req_*` names, **individually, by name, on every call**. Refuse if
any one is missing. Never compute a cell address from a base plus an offset.

### 8.1 The rule and its consequence

The addresses are a fact about one tree at one moment; the **names** are the interface. The eight cells
sit in an `if DEBUG == 1 @shape_divergent` group at the RAM tail (S1 `:268-322`, and S3's
`test_the_cells_are_debug_shape_only` gate holds them there), and aeon's release shape carries none of
them — S4 `:3162` and S3's own header say the release ROM's symbol table has no `Obj_Req_*` entry at
all.

So: **against a release ROM, symbol resolution fails, and the method refuses.** It does not compute
`$FFFFE610` from a table someone transcribed and write fifteen bytes into whatever the release build
put there. That refusal is not a gap in the feature — **it is the feature's safety property**, and it
exists only because the resolution is by name. An offset-based implementation would work beautifully
against every DEBUG build and corrupt game RAM against every release build, silently, with a `result`
that says success.

### 8.2 The refusals

| condition | error | `data` |
|---|---|---|
| no symbol table loaded at all | **`-32012`** (`NO_SYMBOLS_LOADED`) | — |
| a table is loaded; one or more `Obj_Req_*` names are absent | **`-32013`** (`SYMBOL_NOT_FOUND`) | `missing: [...]` — **every** missing name, not the first |

`-32012` and `-32013` are distinct on this bus on purpose (§4, and `rpc.rs:42-46`: *"a client must be
able to tell 'you forgot to load symbols' from 'that name does not exist'"*), and the distinction is
exactly the one a user hits here: *I forgot `load_symbols`* versus *this is a release ROM.* The
`message` on `-32013` must say the second thing out loud — the client's next question is always "why
not", and "this build has no live-object mailbox; it is a DEBUG-shape interface" is the answer.

Listing **all** missing names rather than the first is the same both-directions instinct S3's contract
test is built on: a partial answer to "what is missing" invites a fix-and-retry loop.

### 8.3 The rows are advertised unconditionally

`methods` membership is decided at build time; symbol resolution is decided at call time and can change
after the handshake (`load_symbols` exists and may be called at any point). §6's ⚙ note settles this
shape already for the decoder rows — `capabilities.objectDecoders` reports *"whether this **build** has
the handlers … and never whether a layout was detected; the detect result is on the reply, because
`load_symbols` may be called after the handshake."* Identical reasoning, identical answer: **always
advertised, refused at call time.**

### 8.4 One piece of optional hardening, offered and flagged

Having resolved all eight addresses, the server can assert for free that they are **contiguous, in the
published order, with `Obj_Req_Flag` highest**. If a build ever declared a cell *after* the flag, every
name would still resolve, every width would still be right, and the flag-last property — *the entire
concurrency control* — would be silently gone. That is precisely the mutation S3's
`test_flag_is_the_last_cell` was written to catch **in aeon's tree**; this check catches it in a ROM
handed to us, which is the only place we can catch it. Refusal: `-32005`,
`reason: "mailboxLayoutUnexpected"`, `data` carrying the resolved order.

I am proposing it, and flagging that it is the piece most likely to be judged over-built (§15 Q6). The
argument for it is this repo's own recent lesson: *a control must measure the string it names.*

---

## 9. The proposed surface

Types follow D9: address-shaped values are hex strings, counts and coordinates are numbers.

### 9.1 `emulator/object_spawn` ⚙

| | |
|---|---|
| **params** | `def` (hex string) **\|** `defSymbol` (string) — exactly one, the `ObjDef` archetype record; `x`, `y` (integers, world **pixels**, 0–65535); `subtype`? (0–255, def 0); `flipH`? / `flipV`? (bool, def false); `maxFrames`? (≥1, def 2); `expectFrameToken`? (integer) |
| **result** | `handle` (hex string, e.g. `"0x8E62"`), `addr` (hex string, the full SST address), `slot`? (pool index — **present iff `layout` resolves it**, never fabricated), `x`, `y`, `framesAdvanced`, `layout` |
| **errors** | §6's table, plus `-32602` for a `def` outside the cart window before the write is even attempted |

- `defSymbol` is the ergonomic path: a client finds archetypes with
  `lookup_symbol { name: "ObjDef_", ... }`'s bounded prefix search (`engine.rs:5059`) and hands the
  name straight back. **No archetype-catalogue row is proposed**, because that search already is one.
- `subtype` / `flipH` / `flipV` compose the placement word server-side: `subtype` into the low byte,
  `flipH` → bit 13, `flipV` → bit 14 (S5). The raw word is **not** a param, and the reason is that the
  engine masks it to `$60FF` **silently** (S2 `:325`) — a client's stray bit vanishes with no error,
  which is a value the caller set and the machine never saw. Structured params make every accepted bit
  meaningful and let an out-of-range `subtype` be a `-32602` instead of a truncation. §15 Q4 is the
  forward-compat cost.
- `slot` in the result is the join back to `object_list` (§2.4(b)) — the server inverts
  `layout.slot_addr` so the client never does address arithmetic. Omitted, per the ⚙ group's rule (3),
  when the layout cannot supply it.

### 9.2 `emulator/object_move` ⚙

| | |
|---|---|
| **params** | `handle` (hex string) **\|** `slot` (integer pool index) — exactly one; `x`, `y` (integers, world pixels); `maxFrames`? (≥1, def 2); `expectFrameToken`? |
| **result** | `handle`, `addr`, `slot`?, `x`, `y`, `framesAdvanced`, `layout` |

Accepting **either** `handle` or `slot` is the `addr`\|`symbol` pattern this contract uses on
`emulator/read`, `write_memory` and `memory_hash`, applied to the same problem: the client has one of
two spellings of the same thing and should not have to convert. `slot` is what `object_list` reports;
`handle` is what a previous `object_spawn` returned. Requiring exactly one, refusing both and neither
with `-32602`, is the existing convention.

The row's description must carry aeon's two behavioural facts, because a client will assume the
opposite of both: **position only** — velocity, status, angle and animation are untouched, so a moved
badnik keeps doing what it was doing — and **no clamp**, an out-of-act object is simply culled
(S4 `:3206`).

### 9.3 `emulator/object_delete` ⚙

| | |
|---|---|
| **params** | `handle` **\|** `slot` — exactly one; `maxFrames`? (≥1, def 2); `expectFrameToken`? |
| **result** | `handle`, `addr`, `slot`?, `framesAdvanced`, `layout` |

No `deleted: true`. A field that is `true` on every success is §11.5's *"`released`'s defect with a
useful name"*. The row's description carries the cascade (`DeleteObject` takes the child chain) and the
`slotOwnedByEntityWindow` asymmetry.

### 9.4 Shared

- **`layout` on every reply**, per the ⚙ group's normative rule (1) — these rows decode the same object
  table the decoder rows do and inherit the obligation. A server with no symbol table refuses with
  `-32012` rather than guessing a base; that is already rule (1)'s own sentence and it lines up exactly
  with §8.2.
- **`framesAdvanced` on every reply, success and failure**, so the caller can always reconstruct where
  the machine ended up (§7.3).
- **No `caveat` unless declared.** §11.20 permits `caveat` only where the schema declares it. These
  rows should declare it, and use it for exactly one thing: **the mid-frame window of §7.2**, when the
  server can tell it applied. If it cannot tell, no `caveat` — an unconditional caveat on every reply
  is a field nobody reads.

---

## 10. Contract deltas

1. **§6, *object / player decoders ⚙*** — three rows appended:

   ```
   | `emulator/object_spawn` ⚙  | `def`\|`defSymbol`, `x`, `y`, `subtype`?, `flipH`?, `flipV`?, `maxFrames`?, `expectFrameToken`? | `handle`, `addr`, `slot`?, `x`, `y`, `framesAdvanced`, `layout`, `caveat`? |
   | `emulator/object_move` ⚙   | `handle`\|`slot`, `x`, `y`, `maxFrames`?, `expectFrameToken`?                                     | `handle`, `addr`, `slot`?, `x`, `y`, `framesAdvanced`, `layout`, `caveat`? |
   | `emulator/object_delete` ⚙ | `handle`\|`slot`, `maxFrames`?, `expectFrameToken`?                                              | `handle`, `addr`, `slot`?, `framesAdvanced`, `layout`, `caveat`? |
   ```

   A **fifth normative rule** on the ⚙ group, because the four existing ones are all about *reading*:

   > **(5)** The three mutation rows write through a **game-defined mailbox resolved by symbol on every
   > call**. A server MUST resolve each cell by its own name and MUST NOT address any cell by an offset
   > from another; a build missing any required name is refused (`-32013`, `data.missing` naming every
   > absent one), never written to at a computed address. The rows advance the machine to collect the
   > engine's acknowledgement and MUST report `framesAdvanced`; a request the engine did not
   > acknowledge within `maxFrames` is `-32005 mailboxNotConsumed` and MUST NOT be reported as success.
   > A refusal by the game MUST reach the client as an error, never as a result field.

2. **§6, run-control state rule** — `object_spawn`, `object_move`, `object_delete` added to the list
   that requires a paused machine.

3. **§5** — four new `-32005` discriminants registered beside `machineRunning` and friends:
   `objectPoolFull`, `unknownSlot`, `slotOwnedByEntityWindow`, `mailboxNotConsumed`, `frameMoved`
   (five), plus `mailboxLayoutUnexpected` if §8.4 is adopted.

4. **`schema/bus-protocol.schema.json`** — three fragments, each with `params` closed by
   `unevaluatedProperties: false` per §2.5, each expressing its exactly-one-of pair in both directions,
   plus the new `reason` enum members. **The re-vendor of this repo's pinned copy lands with the serve,
   never before it** — the standing rule from the CR-STOPPREC arc, and the thing
   `crates/oracle-aether/tests/contract/PROVENANCE.md` is built to make checkable.

5. **§8 conformance** — one item: a server advertising any of these three rows must show a refusal for
   a build lacking the symbols, and must show that a non-zero engine status reaches the client as an
   error. The second half is the one that matters; it is the whole point of the CR.

---

## 11. Three surfaces, and the gap being a decision rather than an omission

| surface | what it gets | note |
|---|---|---|
| **plain Aether** | all three rows, as §9 | the normative surface |
| **MCP** | three tools, `emulator_object_spawn` / `_move` / `_delete`, 1:1 with the rows | the existing MCP layer mirrors rows; three tools rather than one `op` tool for §4's reason, and because a tool description is where `defSymbol`'s discoverability actually lands for an agent |
| **player GUI** | a **spawn mode**: pick an archetype, click on the viewport to place, drag a listed object to move, delete-key to remove | this is the surface the feature is *for*, and the one that makes the paused-frame discipline natural — a placement UI is already paused |

The GUI half has a dependency the other two do not: it needs a **screen-dot → world-pixel** mapping to
turn a click into `x`/`y`. `emulator/object_at` already takes a native dot and returns `world{x,y}` with
a `worldSource` (§6, §11.26), so the join exists. **UNMEASURED:** whether `object_at`'s world mapping is
in the same flat world-pixel space `Obj_Req_X`/`Y` want (S1 `:303-304` says *"the same convention as
`Warp_Req_X/Y`"*). If it is not, the GUI needs a conversion and this CR has not specified it. That is a
named gap, not a silent one.

---

## 12. Better-approach pass — where this proposal beats mirroring the engine

The standing instruction is that a peer's shape is the compatibility floor, not the ceiling. Five places
this CR deliberately does better than a faithful mirror, and each has a cost:

1. **Typed refusals instead of a status byte** (§6). Costs: a client cannot get a numeric status back;
   it gets a discriminant. Nobody needs the number.
2. **`expectFrameToken`** (§7.2). aeon's recipe is prose (*"list and request from the same paused
   frame"*); this makes it **checkable**, and — more to the point — reveals that the prose is not
   sufficient. Costs: one optional param.
3. **Structured `subtype`/`flipH`/`flipV` instead of a raw place word** (§9.1). Turns a silent mask into
   a `-32602`. Costs: forward compatibility if the mask widens (§15 Q4).
4. **`defSymbol` and `slot` accepted where the engine takes only addresses** (§9). Removes the two
   arithmetic steps (name→address, address→low word) that a client would otherwise re-implement, one of
   which §2.4(b) shows is already being described loosely across two repos.
5. **All-missing-names in the `-32013`, and the layout-order assertion** (§8.2, §8.4). Both are
   both-directions checks in the spirit of S3, applied on our side of the seam where S3 cannot reach.

And one place a mirror would have been better, recorded rather than argued away: a raw passthrough would
have been **forward-compatible for free** (§5.2(2)).

---

## 13. Alternatives rejected

**13.1 One `emulator/object_request { op, ... }`.** §4. The decisive loss is per-op servedness.

**13.2 Expose the mailbox: an `emulator/mailbox_write` primitive.** §5. It hands the client a rule whose
correctness depends on an unstated property of this server's pause model, and the natural client
implementation of the ack is the wrong one. Rejected — while noting that the *composition* it would
formalise (`lookup_symbol` + `write_memory` + `run_frames` + `read`) remains available today and is not
being removed by anything here.

**13.3 Return the engine status in a successful `result`.** §6. This is the failure the CR exists to
prevent, restated as a design.

**13.4 A server-side object registry — track what we spawned, offer "delete everything I made".**
Tempting and wrong. The engine has no lifetime tracking, no reserved range and no persistence, and says
so deliberately (S4 `:3204`): a spawned object is an ordinary dynamic slot that `InitObjectRAM` takes
with everything else at a level re-init or reset. A registry on this side would hold handles that go
stale invisibly, and "delete everything I made" would issue deletes against recycled slots — §7.2's bug,
industrialised. If a client wants that bookkeeping it can keep it, with its own knowledge of when it
reset the machine.

**13.5 Batch: `object_spawn` taking an array.** The engine is one request per frame (S1 `:283-287`), so
a batch of ten is ten frames of machine advance inside one call, with a partial-failure result shape and
no way to say which frame each landed on. Ten calls say the same thing with ten honest answers. If
throughput ever matters, the right place is a client loop, not a shape that hides nine frames.

**13.6 Refuse when paused mid-frame.** §7.2. Would close the residual window hard; not proposed because
I cannot establish unmeasured that this server can identify the game's frame top. §15 Q3.

---

## 14. What this CR does not bind, and where it is weakest

- **It binds no other server.** The legacy C++ server (`oracle-old/`) implements none of this and is not
  asked to. Per D4 a server that does not advertise these rows simply does not have them, and adding
  rows is additive (§6's own note).
- **It binds no engine behaviour.** Everything in §2 is aeon's, at `36285940`, and this CR asks them for
  nothing. If they change the mailbox, S3's gate turns red in *their* tree and this bus needs a
  re-derivation — which is an argument for §8.4's layout assertion.
- **Weakest point 1: `-32602` for bad def.** §6. `unknownCheckpoint` is the precedent pulling the other
  way, and a `def` that fails the cart-window rail really is *out-of-bounds params*, which §5 assigns to
  `-32602`. I picked the split rule and applied it; a reasonable adjudicator could apply §5's
  worked-example precedent instead and get `-32005 badArchetype`. §15 Q1.
- **Weakest point 2: everything about frame counts is unmeasured.** `maxFrames` default 2 is reasoning,
  not measurement. Whether one advanced frame reaches the consumer from a frame-top pause, whether two
  reach it from a mid-frame pause, and what the game's frame top is in scheduler terms are all
  **UNMEASURED**. The first thing the implementing parcel should do is measure them and correct this
  document.
- **Weakest point 3: the residual mid-frame window is disclosed, not closed** (§7.2). This CR converts
  aeon's *"satisfied trivially"* into *"satisfied when paused at a frame top, and here is what happens
  otherwise"*, which is an improvement in honesty and not in safety.
- **Weakest point 4: one game state.** The consumer exists in exactly one game state, in a test file
  (§2.4(c)). Outside it every request times out. This CR's answer is a typed
  `mailboxNotConsumed` — which is correct, and is also going to be the *most common* error a user
  meets, and a user who does not know the shape of the ROM will read it as a broken feature. The row's
  `message` has to carry that: *the game is not in a state that services this mailbox.*

---

## 15. Questions for the adjudicator

**Q1 — the code for a rejected archetype pointer.** `-32602` (my proposal: the client picked a bad
param and a different one fixes it) or `-32005 badArchetype` (the `unknownCheckpoint` precedent: an
identifier that does not name a live thing in *this* machine)? The five rails collapse to one status
byte, so no error can say *which* rail failed either way.

**Q2 — a timed-out request: cancel or leave armed?** I propose the server clears `Obj_Req_Flag` and
reports `cancelled: true`, because a request that fires minutes after its error reply is worse than a
cancelled one (§7.3). It is nevertheless a write the client never asked for, on a cell the client cannot
see, and this suite's instinct is against those.

**Q3 — the mid-frame window (§7.2).** Is `expectFrameToken` + a disclosed residual window sufficient, or
should these rows refuse a pause that is not at the game's frame top? The second needs a measurement I
could not take and possibly a new engine-side landmark; the first ships a row that can, in a nameable
circumstance, move the wrong object and call it success.

**Q4 — structured placement params vs a raw `place` word.** I propose `subtype`/`flipH`/`flipV` only,
so that no client-set bit can vanish into the engine's `$60FF` mask unremarked. If aeon widens the mask,
this bus lags by a contract amendment. Is that the right side to err on?

**Q5 — naming.** `def` / `defSymbol`, or reuse the established `addr` / `symbol` spellings for a field
that is not the operation's address? I lean `def`/`defSymbol` because `addr` on a spawn row would read
as *where to put it*, which is `x`/`y`.

**Q6 — the layout assertion (§8.4).** Cheap hardening against a real, silent, protocol-breaking change,
or a check that duplicates a gate aeon already runs in the only tree that can change it?

**Q7 — order of operations.** Should these rows be adjudicated into `protocol.md` **before** being
served here, on the CR-I/CR-E pattern? I assume yes, and note that whichever way it goes, the schema
re-vendor lands **with** the serve and never before it.

---

## 16. Corrections owed elsewhere

`docs/2026-09-02-aeon-spawn-mailbox.md` should be amended with §2.4(a) — the "paused frame" reading —
because as written it points a reader at a design that hangs. §2.4(b) and (c) are worth a line each.
The field table itself re-derives correctly and needs no change.

`aeon:docs/ENGINE_ARCHITECTURE.md` §4.12c `:3169` says the slot handle is *"exactly what oracle's
`emulator/object_list` reports"*; it is the low 16 bits of the `addr` that row reports. Worth a note
back to that lane — it is their doc's claim about our surface, and it is the sentence our transcription
inherited the imprecision from.

---

## 17. Adjudicated, and what this lane owes next (added 2026-09-02 after the ruling)

**ADOPTED WHOLE** as `protocol.md` **§11.32**, empyrean **`5ae18dc`** — verified here as reachable on their
`origin/main`, and `--stat` shows `contract/protocol.md +88`, so the SHA class matches what it anchors. All
seven §15 questions came back decided: Q1 `-32602` (one fault, one code), Q2 cancel with `cancelled: true`
and `Obj_Req_Op` left alone, Q3 token-plus-disclosure as v1 with the hard `pausedMidFrame` refusal **not
ruled out** pending a measurement that is the implementing parcel's first job, Q4 structured params, Q5
`def`/`defSymbol`, Q6 the §8.4 layout assertion adopted (not a duplicate of aeon's gate — ours runs against
the ROM we were handed), Q7 yes.

**§16 is discharged** — the corrections to `docs/2026-09-02-aeon-spawn-mailbox.md` landed inside `305b972`,
with the original text kept visible above them. Aeon landed their three at **`4f5ad5a1`** (reachable on
their `origin/master`, `ENGINE_ARCHITECTURE.md` only). **Nothing owed; nothing holds the serve.**

### ⚑ STANDING COMMITMENT — booked here because it was made in mail, and mail is not part of the tree

**The hub is authoring the schema fragments from §11.32; this lane CHECKS THEM AGAINST THE RUNNING
SERVER'S REAL REPLIES at serve time** — not against the schema, and not against this CR. **Anything a
fragment would refuse that we actually emit goes back to the hub BEFORE any re-vendor.**

**Why the split is that way round, and it is the load-bearing reason rather than a division of labour:** on
CR-F this lane authored the vectors, verified them programmatically, handed them over as ready, and **nine
of eleven could not have passed** — every result case carried `"layout": {}` against a `$defs.decoderLayout`
requiring five fields. Author-and-check by one lane share one frame, and that frame is what failed. Hub
authoring from the spec and this lane checking from the implementation are **different enumeration
parameters** (protocol bar 19), which is the only arrangement that catches what neither pass catches alone.

**Re-vendor lands WITH the serve, never before** (§10). A re-vendor ahead of the serve makes the gate green
against a server that does not yet emit the shape — this repo has already measured a re-vendor whose green
witnessed nothing.

### §17.1 — the x/y question, ruled by this lane as implementer (2026-09-02)

The hub's fragments (empyrean **`21c78d2`**, merge of `249690f`) left one thing neither §11.32 nor §9
settles: does the reply **echo the accepted request**, or **re-read the record after the frame advance**?

**RULED: it re-reads, and the field description must name the moment.** *(Adopted as the §11.32 addendum, empyrean `e04a94f` — verified reachable on their `origin/main`, `--stat` shows `contract/protocol.md +13` and the schema's spawn result descriptions carrying the same sentence, so the contract of record is there and this section is the reasoning behind it. The ATTR-RGB-LATCH parallel is recorded in the addendum too, so the colour reply can reuse the wording rather than re-deriving it.)* Echoing carries **zero
information** — the client already holds those numbers. The re-read is the actual machine state, which is
what every other reply on this bus reports. But an unqualified `x`/`y` *after* an advance is a plausible
wrong answer: an object with velocity has moved, and a client that reads the reply as "where I put it" is
wrong. So the description reads *"as read from the object's record after `framesAdvanced` frames, not an
echo of the accepted request"*, and `framesAdvanced` (Q2) is what lets a client reason about the gap.

⚑ **Same defect as `ATTR-RGB-LATCH`, one surface over:** that row exists because a colour reply does not
say **which moment its colour is for**. Identical remedy — the reply names the moment. If either is ever
reworded, reword both.

**The residual, NOT closed and deliberately so:** a re-read conflates *the engine adjusted your requested
position* with *the object moved under its own velocity*. `framesAdvanced` says time passed; it does not
separate the causes. Judged not worth a field in v1 — one honest number beats two the client must
reconcile. **If a consumer ever needs the split it is an ADDED field, never a redefinition of these**, and
nobody may later read the re-read as a spawn-position confirmation.

*Checked, not assumed, on the hub's other two flags:* `defSymbol` typed `$defs/symbolName` with an
embedded `+$hex` refused **matches our existing convention** — displacement is a separate field here
(`{symbol, symbolDisp}` out, `disp` in; `engine.rs:2085`, `:2202-2203`, `:2262`), never part of the name.
`handle` as `$defs/hex` rather than the opaque handle type is right, since §11.32 defines it as the low
word of `addr`. And the published closure covers `layout` only, so a green run on these rows is **not**
evidence that a stranger top-level key is caught.

---

## §17.2 — THE Q3 MEASUREMENT, taken 2026-09-03 by the implementing parcel

**This section corrects §7.2, §14's weakest point 2 and §15 Q3 in place.** All three were written as
UNMEASURED and the adjudicator made the measurement this parcel's first task. Everything below was run
firsthand against a real aeon build carrying the mailbox, through the server's own socket, using only
methods that existed **before** these rows did (`write_memory`, `run_frames`, `read_memory`,
`breakpoint_add`, `lookup_symbol`) — so nothing here is calibrated by the thing it calibrates.

### What it was measured on, and the provenance limit

`/home/volence/sonic_hacks/.aeon-live-objects/s4.debug.bin` + `.lst`, built by the aeon lane at
**`268d93a8`** — the parcel branch tip that aeon `36285940` merges as its second parent. `git diff
268d93a8 36285940 -- games/sonic4/config/ram.emp games/sonic4/test/ojz_scroll_test.emp` touches the
consumer file only through chain 205's unrelated edits: **zero lines matching `Obj_Req` or `objreq`**, and
`config/ram.emp` is not in the diff at all. So the mailbox cells and `objreq_consume` in the measured ROM
are byte-identical to the anchored revision.

⚑ **NOT A FIXTURE, and it must not become one by drift.** That ROM is an unattested build in a sibling
working tree. `fixtures/aeon/` is pinned at sigil chain 189 (`aeon_rev 3f143178`) and **carries no
`Obj_Req_*` symbol at all** — `grep -c Obj_Req fixtures/aeon/s4.debug.lst` is `0`, and sigil's tip chain
is 200, still short of the mailbox's 206. Nothing in the committed suite reads the measured ROM;
`crates/oracle-aether/tests/object_mutation.rs` uses a 68000 test double instead. This is a measurement
with its provenance stated, not a pin.

### Q3(a) — a scheduler boundary is NOT the game's frame top. Measured, decisively.

`GameState_OJZScroll_Update`'s entry — the game's frame top, where `Debug_Warp_Consume` and
`objreq_consume` are spliced — sampled over twelve consecutive frames with a breakpoint:

| what | in-frame offset (mclk of 896040) | PC |
|---|---|---|
| the game's frame top, 12 samples | **832096, 832123, 832153, 832154, 832155, 832166, 832220, 832251, 850488, 850490, 850545, 860067** — twelve distinct values | `0x000A63EA` every time |
| where `run_frames` leaves the machine, 6 samples | **10, 12, 23, 36, 49, 53** | `Render_Sprites$owner_clear`, and once `EntityWindow_DespawnRings$loop+20` |

Two facts, and the second is the one that matters. The game's frame top is **nowhere near** a scheduler
boundary, and it does not sit at a *fixed* scheduler offset either — it drifts by up to 28,000 mclk
frame to frame with the game's own workload. So **no scheduler quantity can stand in for it**, and
`emulator/run_frames` does not leave a client at one.

### Q3(b) — the server cannot recognise the game's frame top from a paused machine, so `pausedMidFrame` is NOT available. v1 stands as ruled.

Three findings, each measured rather than argued:

1. **The consumer carries no symbol to resolve.** `objreq_consume` is a `comptime fn ... -> Code`
   template, and aeon's own comment says why — *"a template declares no symbol at all, so the DEBUG-only
   guarantee is structural"*. Confirmed against the listing: the only `Obj_Req` entries in
   `s4.debug.lst` are the eight RAM cells. There is nothing to resolve as *"where the consumer is"*.
2. **`emulator/call_stack` is catalogued in §6 and NOT SERVED by this build** — `-32601`, measured. So
   the one row that could have answered *"is `RunObjects` on the stack"* is not on the wire, and the
   only landmark a paused machine offers is `pc` plus its nearest preceding symbol.
3. **And the predicate a hard refusal needs is not "am I at the frame top" anyway.** What must be true
   is *no object-mutating code runs between the server's write and the consumer*. That interval does not
   end at `RunObjects`: one of the six `run_frames` samples above stopped inside
   `EntityWindow_DespawnRings`, which calls `DeleteObject`. So the unsafe region runs from the frame top
   to the end of the frame, and its only safe point is the handful of instructions between the game
   state proc's entry and the consumer — an interval with no name, roughly one part in 800,000 of a
   frame. A refusal built on `pc == GameState_OJZScroll_Update` would be exact and useless: it would
   refuse every pause a human ever has, including safe ones.

**VERDICT: the server cannot distinguish it. `expectFrameToken` plus disclosure is v1, exactly as §11.32
ruled, and no `pausedMidFrame` addendum is filed.** The `caveat` key is declared on all three rows and
this server **emits it never**, because §11.32 declares it for a server that *can tell* the window
applied and this one cannot — an unconditional caveat on every reply is a field nobody reads.

**The hazard itself is real and was demonstrated at the allocator**, which is the half that can be shown
without staging a race: across four spawn/delete cycles the engine handed back **the same handle
`0x9B56` every time**. A handle names a slot, and the slot is recycled immediately — so a listing and a
request separated by any object code can name two different objects, which is exactly §7.2's story.

### Q3(c) — the frame counts, and `maxFrames` default 2 is CONFIRMED

Armed with `write_memory` (payload, then the flag last) and stepped one `run_frames` at a time:

| where the machine was paused | in-frame offset | frames to ack |
|---|---|---|
| the game's frame top (breakpoint on `GameState_OJZScroll_Update`) | 832256 | **1** |
| a scheduler boundary (`run_frames`) | 39 | **1** |
| inside `RunObjects`, +400 steps — **past** that frame's top | 878760 | **2** |
| inside `RunObjects`, +4000 steps — before the next top | 217604 | **1** |
| delete, from a frame top | — | **1** |

**Two is the right default and one would have been wrong.** A pause whose in-frame offset is past the
game's frame top misses that frame's consumer and needs the next one. Two was sufficient in every
position measured. §11.32's *"default 2 stands provisionally"* is hereby measured rather than reasoned,
and the value does not move.

### Q3(d) — the engine's five refusals, and the state precondition, reproduced against the real ROM

Every one of these is the raw mailbox answering, before any of the three rows existed:

* **status 3** after exactly **39** successful spawns into a 40-slot dynamic pool (one slot already
  held) — pool full, nothing evicted.
* **status 4** for a handle that named no live dynamic slot.
* **status 2** for `ObjDef_Solid | 1`, the odd-pointer rail.
* **Outside the consumer's game state the flag is never cleared** — armed after a reset and six frames,
  the ack never came. This is `mailboxNotConsumed`, and §11.32 is right that it will be the commonest
  error a user meets.

### What §14 said, and what now replaces it

> *"Weakest point 2: everything about frame counts is unmeasured. `maxFrames` default 2 is reasoning,
> not measurement."*

Measured; default 2 confirmed; the mid-frame case that requires the second frame is named and
reproducible.

> *"Weakest point 3: the residual mid-frame window is disclosed, not closed."*

**Still true, and now known to be unclosable from this side with today's surface**, which is a stronger
statement than the CR could make. What would change it is a landmark the server can read: an
`emulator/call_stack` that is actually served, or an engine-side symbol at the consumer's splice. Both
are additive and neither is proposed here.
