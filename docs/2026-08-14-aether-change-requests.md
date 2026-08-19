# Aether change requests + recorded ambiguities (2026-08-14)

> **★ ALL SEVENTEEN CRs ARE RULED AND APPLIED as of 2026-08-15.** CR-1..CR-6 on 2026-08-14; CR-7..CR-17
> across seven contract amendments (`protocol.md` §11.2–§11.9) the same day. Every entry below carries its
> own outcome marker — CR-4, CR-5, CR-13 and CR-14 got theirs late, on 2026-08-15, when an audit found the
> register was relying on this header for some entries and on per-entry blockquotes for others. A register
> a reader has to know the history of in order to read is not a register.
>
> **The drafts are deliberately left un-rewritten.** Where a CR proposed something the ruling changed or
> refused — CR-1's `frames` spelling, CR-9's enum value, CR-13's expected "register everything", CR-14's
> full-envelope option 1, CR-13's `additionalProperties` — the draft stands and the outcome is quoted
> beneath it, so the difference stays visible. Four CRs were **changed on the way in** and two shipped
> **refusals of a shape the server had already sent**; a register that hid that would make §8's
> raise-don't-deviate rule look cheaper than it is.

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

> **Adopted 2026-08-14** as contract **D12** + the §6 rows (`empyrean` `3b49e1a`), covered by this
> document's opening "ALL SIX ADOPTED" line. Marker added here 2026-08-15 for consistency: CR-7 onward each
> carry their own, and a register where some entries record their outcome and some rely on a header is one
> a reader has to know the history of in order to read.

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

> **Adopted 2026-08-14** as contract **D11** + §2.2 (`empyrean` `3b49e1a`), with its trade-off accepted
> rather than hedged — no `stamped` capability flag, because one would have preserved the defect through
> negotiation indefinitely. Marker added here 2026-08-15 for the same consistency reason as CR-4.

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
`emulator/press | buttons[], frames? (1–1000, def 2) | buttons, frames, frameToken`.

> **Corrected on review.** This paragraph first quoted the row as carrying `port` in both its params and
> its result. It does not — `port` appears nowhere in §6, and we emit and accept it anyway. That is not a
> slip in this CR so much as a symptom of the thing **CR-13** below documents: a field we have been putting
> on the wire long enough that it reads as catalogued. The argument below is unaffected; `buttons` alone
> still distinguishes a `press` reply from a `run_frames` one.

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

> **RULED 2026-08-15 — neither option as drafted** (`docs/2026-08-15-fable-ruling-cr9-cr11-cr12.md`;
> `empyrean` `8adf219`, `protocol.md` §11.7). **Option 2's sentence is adopted, option 1 is refused, and
> the half both options gave up is added as params.**
>
> §3's `runFrames` is redefined by its **stop condition**: *"a bounded frame advance ran to completion —
> `emulator/run_frames`, `emulator/press`, or any future method whose stop condition is an exhausted frame
> count. `reason` names the condition that ended the run, never the method that drove it."* And
> `emulator/stopped` gains two **additive** params, **`buttons`** and **`port`**, REQUIRED when the advance
> was `press`-driven and absent otherwise.
>
> **Why the enum value went.** The enum's organizing principle was already the stop condition and not the
> method: `step` covers *three* methods because they share one condition, while `runTo` and `runToScanline`
> are separate because their **conditions** differ. `press` is a method whose condition is an exhausted
> frame count. Adding a value for it would have been the first time this enum named a caller. And option 1
> is **incomplete on its own terms** — the CR's whole case is that a subscriber who was not the caller
> cannot learn an input was injected, and `reason: "press"` still does not say *what* or *on which pad*.
> Once `buttons` and `port` are params, the enum value carries zero extra bits.
>
> **The house rule it set**, because the watchpoint design reached it independently one document over:
> **`reason` is a small closed vocabulary of stop *conditions*; anything identifying which instance fired
> is a param.** A new `reason` value needs a genuinely new condition, never a new method or cause.
>
> **The cost, recorded not hidden.** `buttons`/`port` cannot be bound to their trigger mechanically — the
> event carries no discriminator, *precisely because* `reason` no longer names the method. What is enforced
> is that the two travel together (`dependentRequired`), since a subscriber told which buttons went down
> but not which pad would blame the wrong controller. Our emission of `"runFrames"` is now conformant by
> construction; **what we still owe is the two params.** That rides with the same server pass as CR-11/12.

> **CLOSED 2026-08-15 — the two params are on the wire.** `Engine::press` now supplies `buttons` and `port`
> to a shared `emit_run_stop`, and it is the **only** call site that does: they enter as one `Option` of a
> pair, so there is no path that emits one without the other and the `dependentRequired` half is structural
> here as well as schematic.
>
> **The unenforceable half is pinned twice, deliberately.**
> `tests/watchpoints.rs::press_stops_carry_buttons_and_port_and_run_frames_does_not` asserts the behaviour
> (a `press` stop carries them, a `run_frames` stop does not, and a *held* button is not a press-driven
> advance); and
> `tests/schema_conformance.rs::control_buttons_without_port_is_rejected_and_that_is_all_the_schema_can_do`
> asserts the **gap** — a `run_frames` stop wearing `buttons`/`port` is schema-*valid*, and that fact is
> written down as a passing assertion so nobody later reads the schema as covering more than it does.

---

## CR-10 — no method is keyed by a screen coordinate, so pixel attribution is panel-only (2026-08-15)

Drafted in full in **`docs/2026-08-15-pixel-attribution-bus-method.md`** §2, with a paste-ready schema
fragment, and **RULED "adopt with changes"** in `docs/2026-08-15-fable-ruling-attribution.md`. Summarised
here only so this register stays the single index of what has been raised.

> **ADOPTED 2026-08-15** (`empyrean` `28ef4bb`, `protocol.md` §11.3), with the ruling's four conditions
> applied first: renumbered from CR-9, the "pause first" reconciliation sentence corrected (it is wrong —
> attribution disagrees with the drawn raster *paused or not*), two false provenance claims struck before
> they could enter the amendment log, and the handler sequenced **after** the §8 item 15 validator so its
> replies are schema-checked from the first run. The §6 row, the three normative behaviours and the schema
> entry are live; the handler follows.

**The gap.** `oracle_core::vdp::Vdp::pixel_attribution` is consumed by our own player and by nothing else;
§6's *VRAM / CRAM / layers* table has eight rows and none is coordinate-shaped. That is a live §8 item 19
violation, and the sweep that found it found **three more** (the watchpoint surface — CR-11/CR-12 below —
SAT/sprite decode, and `sprite_tile_at`, which is not even in `oracle-core`).

**Proposed change.** One row, `emulator/pixel_attribution | x, y | …`, plus the schema fragment.

**Numbering note.** This was drafted as "CR-9" and renumbered: a *different* CR-9 (the `press` reason,
above) was committed **one minute later** by a concurrent agent. Recorded because a register whose numbers
silently collide is worse than one with a gap.

---

## CR-11 / CR-12 — the watchpoint surface (2026-08-15)

The largest of the four item-19 violations: watch **hit reading**, the **drop count**, and **VDP-internal
range watches**, none of which §6's single `watchpoint_add | addr|symbol, read?, write? | addr` row can
express. Directed as the next design pass by the CR-10 ruling. Drafted in full, with paste-ready schema
fragments, in **`docs/2026-08-15-watchpoint-bus-surface.md`**. **Adopt both or neither.**

> **★ BOTH ADOPTED 2026-08-15, as a package, with eight conditions**
> (`docs/2026-08-15-fable-ruling-cr9-cr11-cr12.md`; `empyrean` `af434a2`, `protocol.md` §11.8). §6's one
> watch row became four; `capabilities.watchpoints` is an object; `emulator/stopped` gained `watch`; and
> the schema went from 22 methods to 26. Both rulings the design settled **stand** — value-changes-not-
> write-counts, and the interactive-debugging line — as do poll-only, the handle-not-address argument, the
> seam, `hits()`-not-`take_hits`, and the refusal list.
>
> **The substance of the ruling is a rebase, and the reason is chronology.** The design was committed at
> **14:37**; §2.4 — the shared result conventions §11.5 introduced — landed at **16:49** the same
> afternoon, **2 h 11 m later**. Three non-conformances, all the same mistake in three costumes (an
> honesty mechanism invented locally where the bus had just specified one): both list results carried
> `truncated` without `total`/`returned`; a REQUIRED `caveats[]` array contradicted §2.4's optional
> singular `caveat`; the `limit` echo was missing. **The collapse cost nothing** — every machine-actionable
> half already had a typed key (`seen`, `keysCapped`, `censusOverflow`), and the one permanent property
> left over, a VDP hit's step-granular `mclk`, is §6 prose now per §2.4's own advisory that an
> always-present caveat is one clients learn to ignore.
>
> **Two conditions land on us as work, and one of them is CR-16 repeating itself.** The design gave
> `emulator/stopped` an additive `watch` param **in prose only** — in zero of its four JSON fragments,
> proposed on the day CR-16 was adopted for exactly that defect. It is in the schema now. The other:
> `watchpoint_list.limit` was uncapped while its sibling capped at 4096.
>
> **Also normative, and neither was in the draft:** the watch count is capped, advertised and **refused
> loudly** at `maxWatches` with `-32005 {reason:"watchCapReached"}` (D13 rule 3 verbatim, because a
> silently-dropped watch reads exactly like a negative finding), and a `censusKey` without
> `mode:"census"` is **`-32602`** rather than ignored. New §8 item 21 lists both.
>
> **`via` is adopted** — the one core change, ~8 lines — on evidence rather than symmetry: `CensusKey::Fc`
> provably cannot answer CPU-vs-DMA on a VDP watch, because `fc` is hardwired to `0` there
> (`crates/oracle-core/src/watchpoints.rs:203`, `:896`).
>
> **Registered, not built,** each with its reversal condition: a `sinceSeq` / read-from-now param on
> `watchpoint_hits`; a bus-side last-value table (what exact change counts on the 68000 bus would need); the
> per-frame sampler, as its own capability on its own evidence; and `CensusKey::Pc`.
>
> **What we owe:** the handlers, in `crates/oracle-aether` only, per condition 8 — contract first. Plus the
> `via` census arm in core, and `crates/oracle-core/tests/` untouched throughout.

> **CLOSED 2026-08-15 — the four handlers are live, and `crates/oracle-core/tests/` was not touched.**
> `capabilities.watchpoints` is an object; the cap refuses with `-32005 {reason:"watchCapReached",cap,count}`;
> `censusKey` without `mode:"census"` is `-32602` in both directions; hits are read with `hits()` and never
> `take_hits()`. Coverage went 21 → 25 advertised-and-schematized methods, and the four fragments were in
> the contract before a line of handler existed — the direction §8 requires.
>
> **`via` landed as ~8 lines of `oracle-core/src`, and two more went with it that the design did not
> anticipate.** `WatchReport::stop_after` (the wire needs `stopAfter` in `watchpoint_list`, and it is also
> what lets a stop *name* the watch that caused it — `stop_requested()` is one bool over all of them), and
> `Watchpoints::watch_count` (the cheap "is this instrument worth attaching?" probe a run loop needs when
> the instrument is shared and the panel's own flag no longer answers for it).
>
> **★ And one hazard the design did not foresee, found by a test written for something else.**
> `stopAfter` raises `stop_requested` on a **level** (`matched >= n`, permanently), not an edge. Once the
> instrument is shared with the player's 60 Hz loop — which is the whole point of the hosted arrangement —
> a client arming a `stopAfter` watch would have ended **every** subsequent frame-run before it began: a
> stop condition turned into a permanently frozen window, on a machine nobody asked to pause. §6 already
> rules the case (*"answered by attribution rather than by a gate"*); what was missing was a way to lend an
> instrument's *observations* without its *halt*. That is `oracle_core::bus::Observe`, and it is used at
> every borrowed-run seam: the player's loop, and `Engine::free_run_step`. The halt now applies only to
> runs a client bounded — `run_frames`, `press`, `run_to` — which is what the contract's own *"the run was
> already bounded by its own caller"* describes.
>
> **One place the contract turned out to be impractical, recorded rather than worked around.**
> `run_frames.frames` and `press.frames` are both `"Frames actually advanced"` with `minimum: 1`, which was
> exact while an exhausted frame count was the only way a bounded advance could end. A `stopAfter` watch can
> end one inside its first frame, where the truthful whole-frame count is **0** and the schema cannot say
> so. `Engine::frames_advanced` rounds that single case up to 1 and says why at the site; `frameToken` in
> the same reply is the unrounded coordinate, and the `stopped` event for such a run omits `frames`
> entirely (it is REQUIRED only for `reason: "runFrames"`), so nothing that *can* be exact was made vague.

**The seam: CR-11 is what a watch *is*; CR-12 is what a watch *tells you*.** They are different kinds of
contract change — amend an existing row and add its inverse, versus add two new query rows — and each owns
exactly one of the two rulings the design had to settle, so neither can be adopted while leaving a ruling
homeless.

**Two claims in the sweep that produced this CR did not survive the design pass**, and both are corrected
in that document:

- *"On evidence this outranks attribution."* The design found a **second executed consumer** the sweep
  missed — `crates/oracle-core/examples/watch_probe.rs`, a real "who wrote this?" dev tool against a real
  ROM — which makes the watchpoint surface a **peer** of pixel attribution on executed-consumer count
  (2 vs 2), not its superior. The CR-10 ruling's sequencing was right for a reason neither document had.
- *The D17 analogy is right in principle and wrong in scope.* `droppedEvents` is a **connection** fact, so
  it rides the envelope; the hit ring's `dropped()` is an **instrument** fact, identical for every client
  looking at it. It belongs in the body, and inventing a second envelope counter by analogy would have been
  the wrong move.

**And the handoff's ranked item 4 conflates two instruments.** It calls this *"the most-requested missing
instrument, which never existed"* while pointing at the file that implements it and which has two live
consumers. A per-frame **sampler** and a watch **recorder** are different things; these CRs deliver the
recorder and explicitly refuse to be sold as the sampler.

**One live bug fell out of the design and is already fixed** (`a542b54`): `Watchpoints::clear()` retires the
specs and *keeps* the recorded hits — deliberately, and documented — while the player's click path
clears-and-rearms on every click and its `dump_hits` printed no watch id. Two successive clicks therefore
produced a single interleaved log with no way to tell which pixel a hit belonged to, and the first click's
labels gone. `WatchHit.watch` already carried the attribution; only the printing was missing. This is
exactly the stale-watch contamination the handoff's negative record on interactive debugging warns about,
found in our own player rather than in the record.

---

## CR-13 — ten methods put result keys on the wire that appear in no contract text (2026-08-15)

> **The heading undercounts, and the drafts are left un-rewritten by house convention.** The table below
> covers twelve rows; the complete sweep the ruling then demanded (condition 7) found **sixteen**
> (`docs/2026-08-15-result-key-surplus.md`). Each pass called its own figure a floor and each was right to.

**This is CR-8's offence at scale, and like CR-8 it is a self-report.**

**Contract.** §6's catalog gives each method a params/result row, and §8 forbids the emulator side to
*"invent new ops not in this spec, design its own envelope, or start a second parser."* CR-4 established
that an **optional additive field** on a catalogued method is not a new op — but CR-8 established the other
half: a field that reaches the wire with **no trace in any document** is a deviation whether or not it is a
good field, and it must be recorded.

**The gap, measured rather than recalled.** A live server was driven through 33 messages and every result's
key set diffed against §6's row for that method; each surplus key was then confirmed absent from
`protocol.md` by grep. Full method and evidence in `docs/2026-08-15-wire-conformance-probe.md` (finding F4).

| method | §6's row | we also emit |
|---|---|---|
| `initialize` | §2.1's listed keys | `limits`, `methodSummaries` |
| `emulator/status` | `running,pc,sp,sr,symbolAtPc?,frameToken,symbolCount,romLoading?` | `romBytes`, `romPath`, `symbolsPath` |
| `emulator/read_memory` | `addr,len,bytes,symbol?` | `caveat`, `region` |
| `emulator/read_vram` | `addr,len,bytes` | `caveat` |
| `emulator/state_hash` | `vram,cram,vsram,regs,combined,framebuffer?` | `caveat` |
| `emulator/press` | `buttons,frames,frameToken` | `port` (and `port` as an undocumented **param**) |
| `emulator/hold` | `buttons,down` | `port`, `held` |
| `emulator/pause` / `emulator/resume` | *(no result)* | `wasRunning` |
| `emulator/release_all` | *(no result)* | `released` |
| `emulator/checkpoint_list` | `checkpoints[],cursor?,truncated` | `total`, `returned`, `limit` |
| `emulator/run_to` | `target,reached,pc,maxFrames,symbol?,symbolDisp?,caveat?` | `stoppedAtFrame`, `stoppedAtMclk` |

The count is a **floor**: only 33 messages were sampled, and `screenshot`, `reload_rom` and `load_symbols`
were not among them. (`load_symbols` is separately known to return `binding`, `moduleCount` and a `caveat`
beyond its row.)

**Why nothing caught it.** The schema has no `additionalProperties: false` anywhere, **12 of the 20 methods
we advertise have no `result` schema at all**, and until today nothing validated our replies against the
schema in the first place. The one place the drift is already visible in this very document is CR-9's
opening paragraph, which quoted `port` as though §6 catalogued it.

**What we did.** Nothing yet — deliberately. This is raised before any of the 12 missing schema fragments
are written, because writing them **from what this server emits** would encode the implementation as the
contract, which is the exact inversion of *"the contract leads; the implementation follows it, never the
reverse."* The source for a fragment is §6's row; every key beyond that row is a change request first.

**Proposed change.** Rule on the ten as a block, and expect the answer to be **register, not remove** —
several are load-bearing and deleting them would be conforming by amputation:

- `caveat` is **D12's own device** (*"a `caveat` string stating in words that nothing about the machine's
  state follows"*) applied to reads. If it is right for `run_to` it is right for a truncated read.
- `total` / `returned` / `limit` are the house bounded-array envelope, which §6.1 relies on for the rule
  that *"a client must never be handed a partial list it can mistake for a complete one."*
- `wasRunning` is what lets a client make `pause` idempotent without a second round-trip.
- `limits.maxRunFrames` is how a hosted server advertises that it **refuses rather than clamps** above 120
  frames — a client cannot discover that any other way.
- `port` is the two-controller surface, which the catalog simply never grew.

Three that deserve a harder look rather than a rubber stamp: `methodSummaries` (D4 makes `methods`
authoritative — is a second, richer list a second source of truth?), `stoppedAtFrame`/`stoppedAtMclk` on
`run_to` (D11 already stamps every reply with `frame`/`mclk`; are these the *same numbers* under different
names, and if so they should go), and `romBytes`/`romPath` (a path is host filesystem state on a bus whose
trust model is deliberately local-only — fine, but it should be said).

**And one structural ask.** Once the ten are ruled on, adding `"additionalProperties": false` to the
schematized result objects would make the next such drift **impossible to ship**, rather than discoverable
by a 33-message probe. That is a bigger change than this CR and is named, not proposed.

---

> **RULED AND APPLIED 2026-08-15** — the block was **split, not registered wholesale**
> (`docs/2026-08-15-fable-ruling-cr13-cr14.md`; contract `empyrean` `f309cc8`, `protocol.md` §11.5).
>
> **Registered:** `initialize.limits` (REQUIRED) and `.methodSummaries` (with a MUST-derive clause, without
> which it would be a second op inventory and D4 retired those by name); `status.romBytes/romPath/
> symbolsPath/symbolDisp`; `read_memory.region` + `symbolDisp`; `registers.usp/ssp`;
> `pause`/`resume.wasRunning`; `press.port` and `hold.port/held`; `checkpoint_list.total/returned/limit`;
> `load_symbols.binding/moduleCount`; `reload_rom.romBytes/symbolsDropped`;
> `screenshot.format/width/height/bytes/source`; `state_hash.framebufferSource`.
>
> **Removed — and these are the two the CR's own triage flagged least:** `run_to.stoppedAtFrame`/
> `stoppedAtMclk`, byte-identical to the envelope stamp on both branches (verified three ways), for which
> §6.1 had already ruled the identical case on `restore`; and `release_all.released`, a hardcoded `true`
> carrying zero bits. **Restructured:** `caveat` into a new **§2.4**, once, for the whole bus.
>
> **The CR's structural ask was adopted in its goal and corrected in its mechanism.**
> `additionalProperties: false` provably rejects *every* conformant reply — the stamp and `droppedEvents`
> arrive through `allOf: [$ref replyFields]`, which it cannot see in draft 2020-12. The working keyword is
> `unevaluatedProperties: false`, and it belongs in the **harness**, not the published schema: D5 makes
> fields additive, so closure there would break clients on the next conformant amendment. **Closure binds
> servers; additivity protects clients.** Landed as §8 item 20.
>
> The ruling also warned about this CR's own framing — it opened *expecting* "register", and an expectation
> like that becomes self-fulfilling, at which point §8's prohibition is a filing ritual. Two removals and
> two restructurings out of one block are the teeth.

## CR-14 — `lookup_symbol.otherMatches` is an object where the schema says an array of strings (2026-08-15)

**The first divergence where the contract and the server disagree about a *type*, not a spelling — and the
first where the server's shape looks like the better one.**

> **Confirmed independently by the in-tree validator, 2026-08-15.** Wiring the §8 item 15 schema check into
> the test client's `recv` turned `arrays_are_bounded_cursored_and_flag_truncation` red on exactly this,
> with no knowledge of this CR. It is now registered as `CR-14 emulator/lookup_symbol $.otherMatches` in
> `common::schema::KNOWN_CONTRACT_DIVERGENCES` — **not** exempted from validation: the key is lifted out
> and checked against the house bounded-array shape instead, so a `truncated` flag that went missing would
> still fail, and the rest of the result is still the schema's business. The registry prints on every run
> beside the coverage split, and `every_registered_divergence_is_still_live` fails the day this entry stops
> diverging, so the allowance cannot outlive the ruling. See `docs/2026-08-15-schema-validator.md`.

**Contract.** §4 gives `emulator/lookup_symbol`'s result as
`{"addr":…,"name":"Camera_X","otherMatches":[…]?}`, and the schema types `otherMatches` as
`{"type":"array","items":{"type":"string"}}`.

**What we emit.** The house bounded-array **object** — `{items, total, returned, cursor, limit, truncated,
nextCursor}` — whose `items` are objects `{name, demangled, addr}`, not strings. Two levels of divergence:
wrong container, wrong element type inside it. Pinned by
`crates/oracle-aether/tests/methods.rs::arrays_are_bounded_cursored_and_flag_truncation`, and confirmed on
a live socket.

**Why we did it, and why it is not obviously wrong.** The recon's **non-negotiable #2** is *every array is
bounded, cursored, and flags truncation*, and §6.1 states the reason in the checkpoint context: *"a client
must never be handed a partial list it can mistake for a complete one."* A bare `[…]` is exactly the shape
that rule exists to forbid — §4 even says the method *"returns up to 5 `otherMatches`"*, i.e. it truncates,
with nothing on the wire to say so. And bare strings would drop `addr`, which is the entire reason a client
asked.

**How it survived.** Nothing validated our replies against the schema. It also survived a 33-message probe
run *today*, because that probe only exercised `lookup_symbol`'s **error** path — a reminder that a sample
which never reaches a code path is measuring its own reach. The in-tree validator catches it immediately,
since the existing suite does exercise the success path.

**Proposed change — and deliberately not resolved here.** D14 is explicit that this is a **spec bug**,
*"to be fixed by amendment in the pass that finds it — and until it is amended, the schema governs what
goes on the wire."* So neither artifact moves unilaterally. Three options, in the order we would rank them:

1. **Amend §4 and the schema to the bounded-array envelope**, making the house rule normative for every
   list on this bus rather than one it was applied to informally. Costs a schema `$def`; gains one
   pagination convention instead of two.
2. Keep the bare array and **drop the envelope for this method**, accepting silent truncation on a method
   that already truncates at 5. We would argue against this.
3. Emit both. Rejected on the same grounds §3 rejects emitting two event spellings: it makes the drift
   permanent and invisible because every client keeps working and nobody ever fixes it.

**Folded in with option 1:** `rpc::bounded_array` emits `cursor` and `nextCursor` as **JSON numbers**,
while §8 item 16 says *"a checkpoint `id` and **every list `cursor`** are JSON strings."* `checkpoint_list`
already converts its own token to a string on the way out; the shared helper does not, so `lookup_symbol`
ships numeric ones. Worse, `lookup_symbol` accepts **no `cursor` param at all**, so the token it emits can
never be handed back — a continuation offered for a query with no continuation. Whatever is ruled for the
envelope should settle the token's type and whether it should be emitted here at all.

---

> **RULED AND APPLIED 2026-08-15** (`empyrean` `f309cc8`, `protocol.md` §4 rewritten + §2.4's new
> bounded-list rule) — **our way on the container, against us on the token.** `otherMatches` is now the
> bounded object with **one pinned item shape** (`{name, addr, demangled?}`) and **no `cursor`, no
> `nextCursor`**: the method accepts no continuation param, and a token that can never be handed back
> trains clients that handles are ignorable while publishing the server's position for nothing — which is
> what D9 category 4's opacity exists to prevent. The CR's own option 1, adopted as drafted, would have
> made that dead token normative.
>
> **The rewrite exposed something larger than the CR.** `name` meant *opposite things* on two branches —
> the mangled identifying spelling on one, the readable one on the other, where it additionally carried a
> `+$hex` displacement suffix duplicating the `disp` field beside it. Confirmed on a live server:
> `lookup_symbol {addr}` returned `name: "EntryPoint+$10"`, and passing that straight back was **refused
> `-32013`**. The one field D7 exists to make reliable did not resolve, while `rawName` — which §4 strikes
> as redundant — did. §4 now pins `name` as the identifying, round-trippable spelling and forbids a
> displacement inside it, expressed as `$defs/symbolName`'s pattern rather than as prose, and a round-trip
> test hands every returned `name` back.
>
> The registry entry was retired when the schema was re-vendored, forced by
> `every_registered_divergence_is_still_live` rather than remembered.

## CR-15 — the schema's `$defs/id` forbids the `null` JSON-RPC 2.0 *mandates* on a parse error (2026-08-15)

**Found by the §8 item 15 validator on its first run against the existing suite, not by reading either
document.** It turned three tests red the moment it was wired into the test client's `recv`:
`invalid_json_is_32700_with_a_null_id`, `batches_are_refused_with_32600` and
`an_over_long_line_is_refused_without_desyncing_the_connection` (`crates/oracle-aether/tests/handshake.rs`).

**Contract.** §2 is titled *"The envelope (JSON-RPC 2.0)"* and §8 item 2 says to **adopt** that envelope.
The schema types the correlation id as `$defs/id`: `{"type": ["integer", "string"]}`. §5 catalogs `-32700`
*parse error (invalid JSON)* and `-32600` *invalid request*, and says nothing about what `id` those replies
carry.

**The gap.** JSON-RPC 2.0 §5 is explicit: *"If there was an error in detecting the id in the Request
object (e.g. Parse error / Invalid Request), it **MUST** be Null."* Those two codes are decided **before a
request object exists** — reading the id is the step that failed. So the adopted standard *requires*
exactly the value the schema's type union excludes, and under D14 the schema governs the wire, which would
make the conformant JSON-RPC reply non-conformant to the contract.

**Why this one is a schema bug rather than a server bug**, which is not the usual direction and is worth
stating: the two alternatives are inventing an id (`0`, or the last one seen — which tells a client the
failure belongs to a call it never made, on the one code path where the client's own framing is already
suspect) or omitting `id` entirely (which breaks JSON-RPC 2.0 a second way). Neither is available. This is
the one divergence in the register where the server has no conformant option at all.

**Why the probe missed it.** All six of the probe's error paths sent a *parseable* request and therefore
had a real id to echo (`docs/2026-08-15-wire-conformance-probe.md`, F1). Same lesson as CR-14 from the
other direction: a sample that never reaches a code path is measuring its own reach.

**What we did.** Nothing to the server. The harness registers it in
`common::schema::KNOWN_CONTRACT_DIVERGENCES` as `CR-15 <envelope> $.id`, printed beside the coverage
report on every run, with a **narrow** allowance — exactly `error` + `id: null` + a code in
`{-32700, -32600}` — that substitutes a placeholder id rather than skipping the check, so the code, the
message and the always-present `data` with its stamp and `droppedEvents` are all still validated. Its
width is pinned by
`tests/schema_conformance.rs::the_null_id_allowance_is_exactly_as_wide_as_json_rpc_2_0_requires_and_no_wider`,
which asserts a null id on any *other* code, and on a *success*, are both still rejected.

**Proposed change.** One line in the schema: `$defs/id` becomes `{"type": ["integer","string","null"]}` —
or, if the extra precision is wanted, `null` is admitted only in `errorResponse`'s `id`, since a `result`
always answers a request whose id was read. A sentence in §5 naming which codes carry it would close the
question for the next server author, who will otherwise derive it from JSON-RPC 2.0 exactly as we did, one
test failure at a time. **This is the raising; the ruling is the owner's. The contract repo was not
edited.**

> **ADOPTED the same day** (`empyrean` `90178fc`, `protocol.md` §11.4) — and adopted at the *second* of the
> two shapes proposed above, not the first. `errorResponse.id` accepts null via `anyOf`; `$defs/id` is
> deliberately **not** widened, because a request must carry a real id (an id-less request is a
> notification) and a success response echoes an id that was read, so null is meaningless in both.
>
> **And it was narrowed one step further than this CR asked.** A bare "nullable error id" is *wider than
> the standard*: `-32700` and `-32600` are the only codes decided before a request object exists, and on
> every other code a real id was available to echo, so a null one is a **correlation bug** — the client
> cannot match the failure to the call that caused it, and will retry the wrong one. The schema therefore
> carries an `if`/`then` restricting null to those two codes, which is exactly the width this harness's own
> allowance had already been fenced to. The CR's closing suggestion — *"a sentence in §5 naming which codes
> carry it"* — was taken as a **schema rule** instead of a sentence, on D14's grounds that the tiebreaker
> should be the artifact a validator can enforce mechanically.
>
> **The divergence is retired** (`f45d318`): contract amended, vendored copy refreshed, registry entry
> removed, allowance deleted. That sequence was not voluntary —
> `every_registered_divergence_is_still_live` went red the moment the new schema stopped rejecting the
> canonical message, which is the anti-rot property firing on real traffic on the day it was written. The
> fence test survives, re-pointed: it now checks that the **schema's** width and the harness's predicate
> still agree, so a future widening of the contract goes red here rather than being discovered by a client
> that cannot correlate its own failures.

---

## CR-16 — two fragments were left behind by the CR-13 amendment, and §8 item 20 makes that fatal (2026-08-15)

**Found by implementing §8 item 20 on the day it was written, which is the item working exactly as
advertised: a sampling instrument would have called this clean.** Item 20 closes every result against its
schema fragment with `unevaluatedProperties: false` at test time. Wiring it in turned the whole suite red
on two fragments — and *only* two, out of 22.

**Contract.** §11.5's CR-13 table registers, by name and with conditions:

> **Registered** with conditions: `initialize.limits` and `.methodSummaries` (§2.1); … `read_memory.region`
> + `symbolDisp?`; …

and §2.4 adds a MUST that points straight at this:

> **A schema fragment, however, MUST declare `caveat` for any method that can emit one**, because §8 item 20
> closes results against their fragments at test time and an undeclared caveat would fail that check.

**The gap.** Both statements are in the prose; neither reached the schema.

| fragment | declares | the prose registers, and it is missing |
|---|---|---|
| `handshake.initialize.result` | `serverName`, `serverVersion`, `protocolVersion`, `capabilities`, `methods`, `timingBasis` | **`limits`**, **`methodSummaries`** |
| `methods["emulator/read_memory"].result` | `addr`, `len`, `bytes`, `symbol` | **`region`**, **`symbolDisp`**, **`caveat`** |

Twelve fragments *were* updated by `f309cc8` and are complete — `status` carries `romBytes`/`romPath`/
`symbolsPath`/`symbolDisp`, `run_to` carries `caveat` and `symbolDisp`, `load_symbols`/`read_vram`/
`screenshot`/`state_hash`/`reload_rom` all carry `caveat`. So this is an omission in a large amendment, not
a disagreement about design: every key above is one the contract already decided to keep, in the same
document, in the same commit.

**Why it is registered rather than fixed on our side.** Every one of the five keys is *required* of us by
the prose that omitted it from the schema. `read_memory.symbolDisp` is §4's own "a displacement is never
inside a name string" rule applied to this method — deleting it would put us back to concatenating. Nor is
this a case where the server has a conformant option: the two authorities disagree, D14 makes the schema
the tiebreaker on shapes, and the schema here says less than the document it is derived from. The
resolution is five `properties` entries upstream, and the contract repo was **not** edited.

> **ADOPTED THE SAME DAY** (`empyrean` `d45dc87`, `protocol.md` §11.6) — five `properties` entries across
> the two fragments, `limits` added to `initialize`'s `required` and `region` to `read_memory`'s, and **no
> prose changed, because the prose was already right**. `read_memory.symbol` was also retyped to
> `$defs/symbolName` while the fragment was open, since §4's round-trip rule binds the `symbol` param of any
> method that accepts one and a result field a client hands back carries the same obligation.
>
> **Retiring the two registry entries was forced, not remembered — and by a failure mode the registry was
> not designed around.** A checker here *lifts its key out of the payload* before validating it, so the
> moment the amended schema **required** `limits`, lifting it made it missing and **every checkpoint test
> went red on the handshake** — tests with nothing to do with checkpoints. The liveness test would have
> caught a stale entry; what actually happened was louder and stranger. Recorded because the general rule is
> now stronger than "the list cannot rot": *an allowance that outlives its divergence starts causing the
> failure it was written to suppress, somewhere unrelated.*
>
> One fixture moved with it: `schema_conformance.rs`'s `good_read_memory_reply()` omitted `region` and so
> stopped being conformant the moment the fragment declared it — the positive control catching its own
> drift, which is the only reason the rejection controls beneath it stayed meaningful.

**What we did.** Two entries in `common::schema::KNOWN_CONTRACT_DIVERGENCES`, printed beside the coverage
report on every run. Each lifts only its own keys out and hands them to a checker that asserts the shape
**§2.1 and §11.5's prose** gives them — an object of non-empty summary strings, a `limits` object of
non-negative integers, a non-empty `region` string, a non-negative `symbolDisp`, a `caveat` string. So the
allowance swaps one authority for another rather than opening a hole, and everything else in both results
— including item 20's closure over every other key — still runs. `methodSummaries`' key-set equality with
`methods` (§2.1 rule 2) is asserted separately and unconditionally in
`tests/handshake.rs::method_summaries_are_derived_from_the_same_registry_and_their_key_set_equals_methods`,
so the one clause with teeth is not inside the allowance.

**Proposed change.** Five `properties` entries, no new prose:

```json
"handshake.initialize.result.properties.limits":           {"type": "object"},
"handshake.initialize.result.properties.methodSummaries":  {"type": "object", "additionalProperties": {"type": "string"}},
"methods[emulator/read_memory].result.properties.region":     {"type": "string"},
"methods[emulator/read_memory].result.properties.symbolDisp": {"type": "integer", "minimum": 0},
"methods[emulator/read_memory].result.properties.caveat":     {"type": "string"}
```

**This is the raising; the ruling is the owner's.**

> **Worth noting for the ruling, because it is evidence for item 20 rather than against the amendment.**
> The whole surplus this arc chased was found by *sweeping* — three passes, each calling its own count a
> floor and each being wrong about how much of a floor. Item 20 replaced that with a gate on its first
> day, and the first thing the gate caught was a gap in the amendment that created it. Five keys, two
> fragments, zero sampling.

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

---

## CR-17 — a `frames` count could not report zero, because the amendment before it made zero reachable (2026-08-15)

**Raised by the implementer of CR-11/CR-12, mid-implementation, rather than absorbed.** This is the
behaviour §11.5 spent an entry arguing for, turning up unprompted one amendment later.

**Contract.** `emulator/run_frames` and `emulator/press` report `frames` as *"Frames actually advanced"*,
schematized `minimum: 1`. Exact for as long as the only way a bounded advance could end was by exhausting
its own count.

**The gap.** §11.8 gave a watch a `stopAfter`, so a bounded advance can now end **inside its own first
frame**. The truthful whole-frame count is then `0`, and the schema forbade it — leaving a conformant
server two illegal moves: emit `0` and fail the schema, or round to `1` and report a frame it did not
advance. Neither fragment permits a `caveat` either; both are closed to three keys.

**What we did.** Shipped the round-to-`1` with the reason written at the site, and raised this rather than
leaving a silent lie in the field.

**Proposed change.** `minimum: 0` on both, with the reachability stated in the field's description.

> **ADOPTED THE SAME DAY** (`empyrean` `34a1993`, `protocol.md` §11.9). `minimum: 0` on both result fields;
> **`stopped.frames` deliberately left at `minimum: 1`**, because `frames` is REQUIRED on that event only
> when `reason` is `runFrames`, and a run cut short by a watch ended on the *watch's* condition — it
> reports `reason: "watchpoint"` and carries `watch`. The zero case cannot arise there, and widening it
> would legalise a shape no server should emit. The reply answers *"how far did my call get"*; the event
> answers *"why did the machine stop"*. The rounding is now gone from `Engine::frames_advanced`
> (`crates/oracle-aether/src/engine.rs`), which records why it was worth refusing: a count that silently
> becomes `1` when it was `0` is wrong exactly when a caller most needs it right — it is establishing
> whether anything executed at all before its watch fired. Widening a minimum cannot break a conformant
> reply or a client, so the adoption costs nothing but the exactness promise it adds.

---

## CR-21 — `emulator/write_memory` needs the specification its row never had (proposed 2026-08-18)

Full text in `docs/2026-08-18-cr21-23-tier1-rows.md` (one document, three CRs — the §11.5 multi-entry
precedent). The row at `protocol.md:825` is legacy vintage: no prose, no schema fragment, untyped
`width`, a bound with no refusal code, and unnamed in the run-control state rule. Proposed: work-RAM
window `$E00000–$FFFFFF` refused-never-clipped (`-32004`), exactly one payload spelling (`bytes` XOR
`value`+`width` ∈ 1|2|4, big-endian), `-32005` paused-machine gate named in the rule, `len ≤
limits.maxWriteLen`, writes via the bus path.

**This CR supersedes this file's own `write_memory` deferral** (the *"`write_memory` / `write_vram` —
read-only for now"* bullet under "What this slice did NOT implement"), by the owner ruling of
2026-08-17 (`docs/2026-08-17-aeon-switchover-gap-list.md`, §"Assessment": ADOPT, scoped — the
keep-dead "register-write op" entry covers register writes only, and `write_memory` is built
contract-first). **The `write_vram`/`poke_vram` landmine note in that same bullet stands untouched** —
VRAM writing is not covered by this CR or this slice, and whoever adds it still owes the `poke_vram`
rename and the `bypassesVdpPort: true` flag.

## CR-22 — `emulator/reset`'s `deferred` is a result key two servers share and no text defines (proposed 2026-08-18)

Full text in `docs/2026-08-18-cr21-23-tier1-rows.md`. The row at `protocol.md:783` returns `deferred`,
a key defined nowhere — while Oracle's `ControlSocket.cpp` emits both truthful values today
(`deferred: false` at `:469`, `deferred: true` at `:492`) and our own books called `reset` *"the
conspicuous absence"* from a 25-method control surface. The player has had it on Tab/F1 the whole time
(`commands.rs:102-108`, `main.rs:1369`) — a live D15 parity gap. Proposed: prose defining `deferred`
as *when the reset landed* (both values conformant), not run-state gated (the checkpoint precedent),
with the survival set pinned (SRAM, symbols, checkpoints, watchpoints survive; held pads clear; stamps
restart at 0).

## CR-23 — `emulator/memory_hash`, the genuinely new row, raised before it is built (proposed 2026-08-18)

Full text in `docs/2026-08-18-cr21-23-tier1-rows.md`. No row exists anywhere in `empyrean` (grep
verified), so §8's ban on inventing ops applies and the row comes first. De-facto spec is the legacy
MCP row (`oracle_mcp.py:239-254`, socket op `1c54004` 2026-08-01): FNV-1a-64 + IEEE/zlib CRC-32, no
4096 cap (`len` ≤ 4194304, advertised as `limits.maxHashLen`), RAM/ROM auto-route, refuse-never-clip.
A pure read. Explicitly NOT a lift of §9's `frame_hash` deferral (that is a picture hash, already
served by `state_hash includeFramebuffer`); discharges CR-20 question 3's struck-with-obligations
ruling by pinning both algorithms.

