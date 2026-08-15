# Aether change requests + recorded ambiguities (2026-08-14)

**Status: ALL SIX ADOPTED, 2026-08-14** (Fable ruling C; `empyrean` commit `3b49e1a`). This document is
now the *record* of what was raised and why — the normative text lives in `empyrean/contract/protocol.md`
(amendment log: its §11). Two changes were made to the CRs as drafted below, and the drafts are left
un-rewritten so the difference stays visible:

- **CR-1's enum value is `runFrames`, not `frames`** — matching the existing `runTo` / `runToScanline`
  spelling convention.
- **CR-5 was adopted with its trade-off accepted, not hedged.** Making the stamp normative renders the
  retiring C++ Oracle non-conformant; no `stamped` capability flag was added, because one would preserve
  the defect through capability negotiation indefinitely (contract §10 decision 5).

CR-2 additionally gained three rules the draft did not pin: checkpoints are volatile (never persisted —
the snapshot format is version-fragile by design), `restore` covers the whole machine *including the
ROM*, and the count is capped and **refused loudly** rather than silently evicted (contract D13, §6.1).

**Update 2026-08-15 — CR-7 ADOPTED, CR-8 registered and adopted, and one ruling lands on us as work.**
The contract's second amendment (`empyrean` commit `627e5e4`, `protocol.md` §11.2) closes the last open
item in this document and adds one that was never in it:

- **CR-7 (`timingBasis`) is adopted** as contract **D16** + §2.1, at exactly the shape we shipped, with
  the numbers normative and the field REQUIRED. Nothing to change here; see CR-7 below.
- **CR-8 (`droppedEvents`) is adopted** as contract **D17** + a new §2.3 — a field this server has been
  putting on every reply while it appeared in **no contract text and no change request**. It is now
  written down. Nothing to change here either; see CR-8 below. The lesson is the one §11.1 closed on:
  raising CR-7 was right, and not raising this one was the failure that praise exists to prevent.
- **We are now non-conformant on one point, deliberately.** Contract **D9 gained a fourth type category**
  — opaque handles (checkpoint ids, cursors) are **strings** — and **D14** pins schema-vs-prose
  precedence (the schema is normative for wire shapes). Our checkpoint `id` is a `u64` on the wire, with
  `parse_checkpoint_id`'s doc comment reading *"a **JSON number** per D9"* — a fair reading of D9 as it
  then stood, and one the schema has disagreed with in all four id positions since the methods were
  specified. **`crates/oracle-aether` must move the `id` to a string** (`checkpoint`'s result,
  `restore`/`checkpoint_drop`'s params, `checkpoint_list`'s items and cursor) with its checkpoint tests.
  See contract §8 item 16. It is a breaking change to a surface with no clients yet.

*Original framing, kept for the record:* `empyrean/contract/protocol.md` §8 is explicit — *"What the
Oracle side must not do: invent new ops not in this spec, design its own envelope, or start a second
parser. Deviations are raised as **change requests against this file**, not implemented unilaterally —
the contract leads."* This document is that raising. It was deliberately not sent at the time: the
overnight plan (`docs/plans/2026-08-14-tooling-track2-overnight.md`, question b) recorded that filing a
change request against another team's contract is outward-facing and not ours to initiate unprompted.
**Ruling E retired that framing** — one person owns every repo, so a CR against this contract is the
owner editing the owner's own spec. The sequencing discipline (contract first, implementation second)
stands; the foreign-counterparty fiction does not.

Everything below was hit while implementing `crates/oracle-aether` against the contract verbatim. Each
entry says what the contract says, what we did instead, and why. **Nothing here was deviated from
silently.**

---

## CR-1 — `emulator/stopped` has no `reason` for a completed bounded frame advance

**Contract.** §6 (run-control) catalogs `emulator/run_frames | frames? (≥1, def 1) | frames,
frameToken`. §3 fixes the `emulator/stopped` `reason` enum at
`breakpoint | watchpoint | step | runTo | runToScanline | pause | entry`.

**The gap.** A `run_frames(n)` that completes is none of those seven. It is not a `step` (a step is one
instruction), not a `runTo` (there was no target), and not a `pause` (nobody asked). A client watching
the event stream sees `resumed` and then either an unmapped reason or, worse, nothing at all.

**What we did.** Emit `reason: "step"` — the nearest listed value — and carry the precise outcome in two
**additive** `params` fields, `frames` (how many) and `deadlineReached` (always `true` here, `false`
when a `run_to` fired). Additive params on a catalogued event are not a new op.

**Proposed change.** Add `frames` to the `reason` enum in §3.

> **Adopted as `runFrames`** (contract §3) — the proposed `frames` was inconsistent with the enum's
> existing `runTo` / `runToScanline` spelling. `frames` and `deadlineReached` are normative params.

> **Migrated 2026-08-15.** The adoption sat unimplemented for a day: `Engine::run_frames` kept emitting
> `"step"`, with the pre-adoption CR-1 comment still above it explaining why. It now emits `"runFrames"`
> (`crates/oracle-aether/src/engine.rs`), pinned at the wire in
> `crates/oracle-aether/tests/events.rs::events_reach_a_subscriber_and_carry_the_stamp`, which asserts
> the reason **and** the two additive params §3 makes normative alongside it. `Engine::press` emitted the
> same wrong `"step"` and is covered by **CR-9** below, which is the part that is not merely a migration.

---

## CR-2 — there is no `emulator/checkpoint`, and §9 defers the ops that would cover it

**Contract.** §9 explicitly defers *"Scenario / save-state ops (`scenario_load/advance/seek`) and
`frame_hash` — the replay & regression primitives (later)."* §8 forbids inventing ops.

**The gap.** A "checkpoint" — take a citable, restorable coordinate and come back to it — is one of the
six methods this slice was scoped around, and the core already has the whole capability:
`System::snapshot()` / `System::restore()` are O(struct) bincode round-trips and are already covered by
a determinism property test (`snapshot/restore == identical hash`). But the catalog has no op for it,
so implementing one would be exactly the unilateral invention §8 forbids.

**What we did.** Implemented `emulator/state_hash` instead — which *is* catalogued (§6, status/misc) and
serves the "is this the same machine state?" half of the need — and did **not** implement checkpointing.
This is the one place the intended scope was consciously not delivered.

**Proposed change.** Promote save-state out of §9's deferred list with a concrete shape, e.g.:

| Method | params | result |
|---|---|---|
| `emulator/checkpoint` | `label`? | `id`, `frame`, `mclk`, `bytes` |
| `emulator/restore` | `id` | `frame`, `mclk` |
| `emulator/checkpoint_list` | `cursor`?, `limit`? | bounded array of `{id, label?, frame, mclk, bytes}` |
| `emulator/checkpoint_drop` | `id` \| `all` | `removed` |

`id` is server-assigned, so a client cannot collide with another client's checkpoints on a shared bus.

> **Adopted as drafted, plus three rules this draft did not pin** (contract D13 + §6.1): checkpoints are
> **volatile** (in-memory, per-server-session, never written to disk — the snapshot is a serialization of
> the live emulator struct and is version-fragile across builds *by design*, so persisting it would
> promise a durability the format does not have); **`restore` restores the whole machine, ROM included**
> (restoring across a `reload_rom` brings the old cartridge back — defined behaviour, not a refusal);
> and the count is **capped and refused loudly** at the limit with `-32005`, never silently evicted.
> §9's deferral was lifted, with the reasoning recorded in the contract's §9.1 so the history is legible.

> **Delivered.** The four methods are live in `crates/oracle-aether/src/engine.rs` (`METHODS` rows +
> `Engine::{checkpoint, restore, checkpoint_list, checkpoint_drop}`), advertised as
> `capabilities.checkpoints = {supported, cap}` with `cap = 8`, and pinned at the wire by
> `crates/oracle-aether/tests/checkpoints.rs` — one test per D13 rule, including "the previous cartridge
> comes back across a `reload_rom`" and "the oldest id still means what it meant once the cap is hit".
> The sentence above — *"the one place the intended scope was consciously not delivered"* — no longer
> holds; it is left in place because these drafts are the record of what was raised, not a status page.

> **Review corrections (same day).** An adversarial review of the delivery found three things, all fixed
> on top rather than by rewriting the commit:
>
> 1. **`checkpoint_list`'s cursor is an `id`, not a `Vec` position.** `checkpoint_drop` compacts the
>    slot vector, so a drop *before* an outstanding cursor shifted every later slot left and the next
>    page stepped over a live checkpoint — while still reporting `truncated: false`, which is exactly the
>    "partial list a client can mistake for a complete one" §6.1 forbids, on a bus §6.1 explicitly
>    expects two clients to share. The cursor now means *"resume at the first id strictly greater than
>    this"*, which is stable under concurrent drops because ids are monotonic and never reused.
>    `rpc::bounded_array` is **not** changed — it is shared with `lookup_symbol`, whose cursor really is
>    a position into an immutable result set — so it is fed the positional skip count for its
>    `total`/`returned`/`truncated` maths and only the emitted continuation token is checkpoint-specific.
>    A knock-on: a cursor whose slots have since been dropped is now an honest empty page instead of a
>    hard `-32602`; an id the server never issued is still refused.
> 2. **The symbol table rides in the slot too.** `symbols`/`symbols_path` are engine-side shadows of the
>    loaded cartridge in exactly the way `rom_path` is, and leaving them behind meant a restore came back
>    with ROM A while `lookup_symbol` still answered from ROM B's listing — D7's named hazard, and a
>    `read_memory {symbol}` that reads a wrong address and reports success. `reload_rom` already drops a
>    table that stops binding; `restore` was strictly weaker for the same cartridge transition. No wire
>    field was invented (§6.1: "no extra fields are needed and none should be invented"). The pair is not
>    re-validated on restore — both halves come from one slot and were checked against each other when
>    the listing was loaded — and that reasoning is a `debug_assert!` rather than a comment.
> 3. **The volatility test was a name grep, and it was vacuous.** Adding a `std::fs::write` inside
>    `Engine::checkpoint` left the whole suite green. D13 rule 1 is a claim about *code paths*, so it is
>    now checked by reading them: the four handler bodies and their helpers are scanned for filesystem
>    tokens, with an anti-vacuity control asserting the same scan **does** fire on `reload_rom` and
>    `load_symbols`, which legitimately read files. Not airtight — a violation hidden behind a helper
>    defined elsewhere in the file would slip past — but it catches what actually happens, and it fails
>    on the exact mutation that used to pass.

> **The `id` became a string, 2026-08-15 — and this is not a CR, it is us conforming.** The delivery
> above shipped the `id` as a **JSON number**, with an in-code comment reading *"a checkpoint `id`: a
> JSON number per D9"*. That reading of D9's *"counts, lengths, **slot indices**, line numbers"* was fair
> against the text as it then stood, and the contract says so — but the **schema** has typed all four
> `id` positions as `{"type":"string"}` since the checkpoint methods were specified, and D14 makes the
> schema the authority on the wire. The 2026-08-15 amendment settled it by adding **D9 category 4**
> (opaque handles are strings) and named this server as non-conformant by file and comment (§8 item 16).
> The `id` is now a string in all five wire positions — `checkpoint`'s result, `restore`'s and
> `checkpoint_drop`'s params, `checkpoint_list`'s entries, and the `-32005` `error.data.id`. The
> internal counter stays a `u64`, which §6.1 blesses explicitly, so the id-ordered cursor is untouched.
>
> One judgement call inside it, recorded because it is a deliberate asymmetry a reader will trip over:
> `parse_checkpoint_id` is **strict** (a JSON string only) while `parse_cursor` still accepts a bare
> number. A cursor is only ever *round-tripped*, so a number-typed field in a client's own storage is a
> plausible accident and refusing a token we ourselves issued would punish a client for our bug. An `id`
> is the handle a human hand-types into the next call, and typing `{"id": 3}` **is** the
> arithmetic-on-a-handle D9 category 4 exists to forbid — accepting it would reward the forbidden usage
> and keep it invisible until ids stop looking like small integers. §8 item 16 names why the strictness
> is affordable: this surface has no clients yet. Relatedly, a well-formed string this server could never
> have issued (`"0x1"`) is answered `-32005 unknownCheckpoint`, not `-32602`: to a client the handle is
> opaque, so "that is not one of mine" is the only distinction the wire may draw, and a parse error there
> would publish the internal spelling of an id.

---

## CR-3 — §5 has no error code for "wrong machine state for this operation"

**Contract.** §5's table covers parse/envelope/method/params/internal, plus `-32000` (op not wired),
`-32004` (address out of range), `-32010` (timed out), `-32012`/`-32013` (symbols), `-32015` (version).

**The gap.** `emulator/run_frames` while the machine is free-running is not a bad envelope, not a bad
param, and not an internal error — it is a well-formed request that is wrong *right now*. Doing it
implicitly (pause, run, stay paused) would change the machine's mode as a side effect of a call the
client did not ask to change mode, which is the class of silent state change this bus exists to prevent.

**What we did at the time.** `-32600` with `data.reason = "machineRunning"` and a message naming the fix
(`emulator/pause` first). `-32600` was the least-wrong code but it reads as "bad envelope", which this
is not.

**Proposed change.** Add `-32005 | invalid state for this operation` to §5, with `data.reason` carrying
a machine-readable discriminant.

> **Adopted** (contract §5). `-32005` exists in code as `rpc::code::INVALID_STATE`, with
> `RpcError::invalid_state(reason, message, extra)` merging the discriminant into `data` so it cannot be
> forgotten. The checkpoint methods use it (`checkpointCapReached`, `unknownCheckpoint`).
>
> **Migrated — delivered.** `Engine::require_paused` (`crates/oracle-aether/src/engine.rs`) now returns
> `-32005` via `RpcError::invalid_state`, so `emulator/run_frames` while free-running — §5's own first
> worked example of `-32005` — is conformant. The `data.reason` discriminant was already the contract's
> `"machineRunning"` and is unchanged, as is the message naming the fix; the migration was the code
> integer alone, with no implicit pausing added. The pinning assertion in `crates/oracle-aether/tests/methods.rs`
> (`a_run_request_while_free_running_is_refused_rather_than_silently_changing_mode`) was flipped to
> `-32005` first and observed failing against the old implementation before the change. This closes the
> branch's last known deviation from §5 / §6's run-control state rule / §8 item 12.
>
> The gate covers all four `require_paused` callers — `run_frames`, `run_to`, `press` and `reload_rom`
> (each of which *advances or replaces* the machine). `run_to_scanline` and `step*`, also named by §6's
> rule, are not implemented in this slice and so are not advertised by `initialize`. The remaining
> `-32600` sites were audited and are all genuine envelope errors: batch / non-object message /
> malformed `id` / `jsonrpc` / `method` (`rpc.rs`), the over-long-line refusal (`server.rs`), and the
> handshake-sequencing checks (`session.rs`) — the last of which are *connection*-state, not machine-mode,
> and so stay outside `-32005`'s §5 definition ("not in the machine's current mode").

---

## CR-4 — `emulator/run_to` is specified without a bound, and without a fired-vs-timed-out result

**Contract.** §6 catalogs `emulator/run_to | addr|symbol | target`.

**The gap.** Two problems, and they compound.

1. **No bound.** A target PC that is never reached is an unbounded run. That is precisely the failure
   `aeon/docs/BUGS.md:494-551` records — a frozen repro frame *"lost to an emulator control-socket hang
   before the sprite table could be dumped"*, irreplaceable evidence destroyed by a hang in the debug
   transport.
2. **No outcome field.** The result is `target` — an echo of the input. It cannot distinguish "I got
   there" from "I gave up", so a client reads whatever state the machine happens to be in and draws a
   conclusion from it. Our core deliberately refuses to merge those two:
   `StopReason::{SinkRequested, DeadlineReached}` exists because *"a caller that cannot tell 'my
   condition happened' from 'I gave up waiting' will confidently draw the wrong conclusion from the
   state it reads"* (`crates/oracle-core/src/system.rs`).

**What we did.** Added an **optional additive** `maxFrames` param (default 600) and always return
`reached: bool` alongside `target`, plus a `caveat` when it did not fire saying in words that nothing
about the machine state follows from where it stopped. Optional params on a catalogued method are
additive, not a new op.

**Proposed change.** Make `maxFrames` and `reached` normative in §6, for `run_to` and for any future
`run_to_scanline` / `wait_for_break`-shaped op.

---

## CR-5 — no reply carries a machine timestamp

**Contract.** §2's response shape is `{"jsonrpc","id","result"}` with per-method result fields; only
`emulator/status` returns anything time-like (`frameToken`).

**The gap.** This is the recon's headline finding about the sibling's surface
(`docs/2026-08-14-tooling-frontier-recon.md` §4): *"The worst single defect: no reply carries a
timestamp… An agent stitching four reads into one conclusion may be reading four different machine
states with no way to detect it. That is a silent-wrong-answer generator."* And §5 C2 adds that the
stamp must be **emulated**, never wall-clock, or two runs of the same ROM do not agree — the sibling's
`frame_token` is a UI counter, which forced hand-rolled realignment three separate ways.

**What we did.** Every reply — success `result`, error `data`, **and** every pushed event's `params` —
carries `{frame, mclk, running}`, both clocks emulated. It is applied structurally after the handler
returns and overwrites any key of the same name, so a handler cannot omit or shadow it. Additive to
every method, so no client breaks.

**Proposed change.** Make the stamp a normative **envelope-level** field in §2, not a per-method one.
Retrofitting it later costs every method and every client; the whole reason it is here from line one is
that it is far cheaper now.

---

## CR-6 — `emulator/romReloaded` vs `emulator/rom_reloaded` (carried, not resolved)

Already registered as recon §7 open question (b): Aurora's **approved**
`aurora/docs/specs/2026-07-03-aether-client-playtest-design.md` subscribes to a differently-spelled
event name from the one `protocol.md` §3 fixes. We implement `protocol.md`'s spelling
(`emulator/romReloaded`), because it is the stated source of truth. **One of the two documents is wrong
and only the owner should decide which.** Until then, an approved client is subscribing to an event
this server will never emit.

> **RESOLVED — the contract's spelling stands; Aurora's spec was corrected** (`aurora` commit `26378c9`,
> line 35, one identifier, nothing else touched). camelCase event names are now normative in the contract
> (§3, §10 decision 4), and servers are explicitly forbidden from emitting both spellings to bridge the
> gap. The live bug is closed: it was the only item in this set where an approved client was subscribed
> to an event that would never arrive. Our implementation needed no change — it already emitted the
> contract's spelling.

---

## CR-7 — `initialize` now advertises `timingBasis`, which the contract does not define (2026-08-14)

Added while executing **Fable ruling A** (`docs/2026-08-14-fable-rulings.md` §A): the `initialize`
result carries a new top-level key

```json
"timingBasis": {"standard": "ntsc", "mclkPerFrame": 896040, "linesPerFrame": 262}
```

**Why it is not a silent deviation, and why it is recorded here anyway.** Every `frame` in every stamp
(CR-5's envelope field) is currently NTSC and says so nowhere; a client that caches frame coordinates
across sessions has no way to record what a frame *was*. Prose cannot be branched on, so the basis is a
field. It is additive and ignorable — a client that does not read it is unaffected — which is exactly why
it went in ahead of a contract edit rather than waiting. But `protocol.md` §8 is about *unilateral
invention*, and a new result key is one, so: **request that `timingBasis` be added to the contract's
`initialize` result**, with the numbers normative (not just the label — "ntsc" alone is under-specified;
262 lines × 3420 mclk is the thing that matters) and the field REQUIRED, so it stays meaningful the day a
server is not NTSC-only. Source: `oracle_core::system::TimingBasis`, derived from the core's own
`MCLK_PER_FRAME`, so a server cannot advertise a basis its stamps were not computed with.

> **ADOPTED 2026-08-15** as contract **D16** + §2.1 (`empyrean` `627e5e4`), at the shape that shipped and
> with both asks granted: the field is REQUIRED, and the *numbers* are normative rather than the label
> alone. It is a top-level key of the `initialize` result, not a capability flag, on the grounds that it
> is not something a server may or may not support — it is what that server's stamps mean. The schema
> requires all three of `standard` / `mclkPerFrame` / `linesPerFrame`, and `standard` is deliberately a
> string rather than an enum so a client branches on the numbers. **No change here.** One case the
> contract left open rather than settle by inference: what happens if the basis changes mid-connection
> (a `reload_rom` of another region's ROM). Registered in the contract's §10 remaining list; unreachable
> while the core is NTSC-only.

---

## CR-8 — every reply carries `droppedEvents`, which the contract does not define (raised retroactively, 2026-08-15)

**Contract as it stood.** §2.2 defined the envelope-level fields as exactly `{frame, mclk, running}`.
Nothing anywhere mentioned a dropped-event count.

**The gap — and it is a self-report, not a finding about someone else.** `server.rs`'s `with_dropped`
inserts `droppedEvents` into the stamp map of **every** success `result` and **every** `error.data`
(`crates/oracle-aether/src/server.rs`), carrying the per-connection monotonic total that
`Outbound::take_dropped` accumulates when the bounded outbound queue discards for a client that is not
draining (`crates/oracle-aether/src/outbound.rs`). It is pinned at the wire by the incident test
`a_client_that_subscribes_and_stops_reading_cannot_wedge_the_emulator`
(`crates/oracle-aether/tests/events.rs`), whose third assertion is precisely *"the loss is visible, never
silent."*

It is a good field — a non-blocking event channel is required by contract §8 item 4, and one that drops
*silently* turns the push stream into the same silent-wrong-answer generator CR-5's stamp exists to
prevent. But it reached the wire with no trace in any document: **not in the contract, and not in this
file either.** That makes it the same offence as CR-7 one degree worse — the deviation was not silent in
the code, only in the record — and it is written up here retroactively so the record is complete rather
than flattering.

> **ADOPTED 2026-08-15** as contract **D17** + new **§2.3** (`empyrean` `627e5e4`), at the shape that
> shipped: on every `result` and every `error.data`, per-connection, cumulative, monotonically
> non-decreasing, **always present even at zero** (absence and zero must not both mean "nothing was
> lost"), and **absent from events** (an event that did not arrive cannot report anything, and one that
> did would carry a number that can change between queueing and writing). The contract states explicitly
> that it is **not part of the machine stamp** — the stamp is a machine coordinate every connection sees
> identically, while this is a per-connection fact two clients will legitimately disagree about — which
> is why §2.2 and §2.3 are separate sections even though both fields ride the same envelope and are
> applied the same way. **No change here.**

---

## CR-9 — `emulator/stopped` has no `reason` for a bounded frame advance driven by `press` (2026-08-15)

**Contract.** §3 fixes the `emulator/stopped` `reason` enum at
`breakpoint | watchpoint | step | runTo | runToScanline | runFrames | pause | entry` — a closed set — and
pins two of its members against each other: **`step`** is *"a `step` / `step_over` / `step_out`
completed. One instruction, or one instruction-shaped unit. It is **not** the value for a frame
advance."* **`runFrames`** is *"an `emulator/run_frames(n)` ran to completion."* §6 catalogs
`emulator/press | buttons, port?, frames? | buttons, frames, port, frameToken`.

**The gap.** `emulator/press` advances whole **frames** — it holds the buttons down, runs `frames` of
them, then restores the held set — and then stops. That completion is none of the eight. It is not a
`runTo` (no target), not a `pause` (nobody asked), not `entry`, and §3's own pinning rules out `step`
affirmatively: a frame advance is exactly what `step` is defined not to be. `runFrames` is the only
member left standing, and it is *imprecise* rather than wrong — this was not an `emulator/run_frames`
call. CR-1 closed the same hole for `run_frames` and did not reach the second method that has it.

**What we did.** Emit **`reason: "runFrames"`** for `press`, keeping the `frames` / `deadlineReached`
params it already carried (`Engine::press`, `crates/oracle-aether/src/engine.rs`; pinned by
`crates/oracle-aether/tests/events.rs::a_press_reports_runframes_because_step_is_the_one_value_section_3_rules_out`).
The reasoning is written into the code comment beside it, not just here. Between a value the contract
rules out and the nearest admissible one, we take the nearest admissible one: the enum is closed, so
minting a ninth value unilaterally is the invention §8 forbids, and a client watching the stream is then
told "a bounded frame advance completed" — true, if under-specified — instead of "an instruction
completed", which is false.

**Why it still matters that the value is imprecise.** `press`'s **reply** already distinguishes the
case: it carries `buttons` and `port`, which no `run_frames` reply has. So a caller can always tell what
happened. The ambiguity is confined to the **event stream** — and that is precisely the consumer that
cannot undo it. A subscriber that was not the caller sees only `resumed`, `stopped {reason: runFrames,
frames: 2}`, and has no way to learn that an input was injected into those two frames. For a bus whose
whole purpose is reproducible experiments, "someone pressed Start here" is not a detail the stream
should have to lose.

**Proposed change.** Either of two, and the cheaper one is fine:

1. Add an explicit value to §3's enum — e.g. `press` — for "a bounded frame advance driven by
   `emulator/press` completed", with the same `frames` / `deadlineReached` params; or
2. Add one sentence to §3 confirming that **`runFrames` covers any bounded frame advance regardless of
   which method drove it**, which makes today's emission conformant by construction and tells the next
   server author the same thing without their having to derive it.

Option 2 costs a sentence and closes the question. Option 1 costs an enum value and additionally lets
the stream carry the fact that an input was injected — which is the half option 2 gives up. **This is the
raising; the ruling is the owner's.** The contract repo was not edited.

---

## Recorded ambiguities (no change requested, but the reading should be confirmed)

**A1 — what error code answers a method sent before `initialize`?** §2.1 says `initialize` is the first
request on every connection but names no code for violating that. We use `-32600` with
`data.expected = "initialize"`.

**A2 — may methods be called between `initialize` and `initialized`?** §2.1 forbids the server pushing
*events* before `initialized` and says nothing about method calls. We allow them (LSP would not). The
permissive reading is the one that cannot break a client that already works.

**A3 — `capabilities.events`: what we emit, or what the protocol defines?** §2.1 calls it *"the
authoritative event set"* and its example lists all three events. We read "authoritative" as *what this
server actually pushes*, so a server that emits two would advertise two. Advertising an event we never
send would be a lie a client cannot detect. (We do emit all three, so the reading is currently moot —
but it will not stay moot.)

**A4 — mode 0600 has an unavoidable creation race in safe Rust.** §7.1 says *"Created mode `0600`"*;
D8 says *"SHOULD be created mode `0600`"*. `UnixListener::bind` creates the node with `0777 & ~umask`
and we narrow it immediately after, so there is a brief window at whatever the umask allowed. Closing
it properly needs a pre-bind `umask(2)`, and `oracle-aether` is `#![forbid(unsafe_code)]`. We instead
**verify** the mode after setting it and refuse to serve if it is not `0600` — so the window exists but
serving on a wrong-mode socket cannot.

**A5 — `mclk` as a JSON number.** D9 puts counts in JSON numbers. Master-clock ticks accumulate at
~53.8 M/s of emulated time, so `mclk` crosses 2^53 after roughly six emulated years of continuous
running and a float-backed JSON parser would start losing ticks there. Flagged, not worked around: the
hex-string alternative would make the most-read field in the protocol the least readable one.

**A6 — the handshake's own arrays are deliberately unbounded.** The recon's non-negotiable #2 ("every
array is bounded, cursored, and flags truncation") is applied to every *query result*. It is **not**
applied to `initialize`'s `methods` / `capabilities.events`, which are complete, generated,
small self-descriptions — cursoring them would mean a client could hold a partial method list and
believe it complete, which is worse than the dump it prevents.

---

## What this slice did NOT implement (and why that is not a deviation)

Of the 53-method catalog (§6), 16 are implemented. The rest are absent, not broken: D4 makes the
advertised `methods` list authoritative and generated, so a client discovers exactly what exists, and
D5 makes adding the rest a capability-flag change rather than a breaking one. The capability flags
`z80`, `vgm`, `objectDecoders`, `profiler`, `breakpoints`, `watchpoints`, `batch` and `sixButtonPad` are
all advertised **`false`** so no client has to guess.

Deferred deliberately, with the contract's own justification where it has one:

- **breakpoints / watchpoints / step / step_over / step_out / call_stack** — the archaeology's negative
  evidence on interactive debugging is strong and specific (recon §2a: three independent statements of
  *harm*, including a 1,691,410-hit stale breakpoint contaminating later captures). `run_to` gives the
  non-blocking stop-on-condition the record actually wants.
- **Z80 ops, VGM, profiler, object/player decoders** — catalogued but out of a thin slice; the decoders
  additionally need the descriptor-driven design recon §6 argues for, not the sibling's compiled-in
  offsets (which *"already rotted once"*).
- **`wait_for_break`** — §3 deprecates it in favour of `emulator/stopped`, and §9's transition window is
  for the *sibling's* existing clients. A brand-new server has no client to transition, so shipping a
  deprecated poll loop would create the debt the deprecation exists to retire.
- **`ping`, `list_ops`** — §6 removes `list_ops` outright; `ping` is superseded by `initialize`.
- **`write_memory` / `write_vram`** — read-only for now. Note for whoever adds `write_vram`: recon §4
  calls the sibling's version *"a genuine landmine"* because it writes straight into the VRAM buffer,
  bypassing the VDP port path, autoincrement, FIFO and DMA, and *"nothing in its docstring says so"*.
  If ported, it must be named `poke_vram` and flag `bypassesVdpPort: true`.
