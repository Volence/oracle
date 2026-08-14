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

---

## CR-3 — §5 has no error code for "wrong machine state for this operation"

**Contract.** §5's table covers parse/envelope/method/params/internal, plus `-32000` (op not wired),
`-32004` (address out of range), `-32010` (timed out), `-32012`/`-32013` (symbols), `-32015` (version).

**The gap.** `emulator/run_frames` while the machine is free-running is not a bad envelope, not a bad
param, and not an internal error — it is a well-formed request that is wrong *right now*. Doing it
implicitly (pause, run, stay paused) would change the machine's mode as a side effect of a call the
client did not ask to change mode, which is the class of silent state change this bus exists to prevent.

**What we did.** `-32600` with `data.reason = "machineRunning"` and a message naming the fix
(`emulator/pause` first). `-32600` is the least-wrong code but it reads as "bad envelope", which this
is not.

**Proposed change.** Add `-32005 | invalid state for this operation` to §5, with `data.reason` carrying
a machine-readable discriminant.

> **Adopted** (contract §5). `-32005` now exists in code as `rpc::code::INVALID_STATE`, with
> `RpcError::invalid_state(reason, message, extra)` merging the discriminant into `data` so it cannot be
> forgotten. The checkpoint methods use it (`checkpointCapReached`, `unknownCheckpoint`).
> **Not yet migrated:** `Engine::require_paused` still returns `-32600`, so `emulator/run_frames` while
> free-running — §5's own first worked example of `-32005` — is knowingly non-conformant. Its
> `data.reason` is already the contract's `"machineRunning"`, so the fix is the code integer at
> `engine.rs` `require_paused` plus the assertion in `tests/methods.rs`
> (`a_run_request_while_free_running_is_refused_rather_than_silently_changing_mode`). Left for a separate
> slice rather than folded into the checkpoint work, so a behaviour change to shipped methods gets its
> own review.

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
