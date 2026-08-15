# Probing the live wire against the contract schema (2026-08-15)

> **SUPERSEDED IN ITS FIGURES, KEPT FOR ITS METHOD (2026-08-15, later the same day).** Every number below
> was true when measured and none is true now: the schema covered 8 of 20 methods and now covers **21 of
> 21**; F4's "ten methods" was a floor that the ruling's condition-7 sweep raised to **sixteen**
> (`docs/2026-08-15-result-key-surplus.md`); the checkpoint-id failure (F1) and the `otherMatches`
> divergence (F5) are both fixed; §8 item 20 now closes every result against its fragment, so the class of
> defect F4 describes cannot ship any more. **This document is deliberately not rewritten.** Its value is
> the record of a count that was wrong three times in a row, each time in the same direction, and of F2 —
> which has not aged at all, because no schema can ever express it.

Written before the schema validator of contract §8 item 15 was built, to find out what it would catch —
and, more usefully, what it would **not**. Everything below was measured firsthand against a running
server, not read off the source.

**Method.** Built `target/release/oracle-aether`, served `vendor/TestRoms/vdp_port_access.bin` on a private
socket, and drove a Python NDJSON client through **33 messages** — the handshake, thirteen methods, six
deliberate error paths (`-32602` bad hex, `-32602` bad `label` type, `-32602` `frames: 0`, `-32005`
unknown checkpoint, `-32013` unknown symbol, `-32601` unknown method) and every event the server pushes.
Each received line was validated against
`empyrean/contract/schema/bus-protocol.schema.json`: the `anyMessage` envelope for every line, plus
`methods.<name>.result`, `events.<name>.params` or `handshake.initialize.result` where the schema has one.

A 23-message run and the 33-message run find **exactly the same three failures**.

---

## F1 — the only schema-level failure on the live wire is the checkpoint `id`

```
x1  emulator/checkpoint       $.id                  1 is not of type 'string'
x2  emulator/checkpoint_list  $.checkpoints[0].id   1 is not of type 'string'
```

Precisely what contract §8 item 16 records, and nothing else. Across 33 messages, 20 advertised methods
and six error paths, no other shape the schema states is violated. This is the positive control a
validator has to reproduce, and it is worth knowing it is a *narrow* control: three occurrences in two
methods, not a broad class.

## F2 — a schema validator structurally **cannot** catch §8 item 13

`emulator/stopped` carrying `reason: "step"` for a completed `run_frames` **passes the schema**, because
`step` is a legal member of the reason enum. The rule item 13 states — that `runFrames` is the value and
`step` is a knowing mislabel — lives in §3's prose, and **D14 puts behaviour under the prose, not the
schema**.

This is worth stating flatly because the inference is so natural: *"we validate every reply against the
normative wire schema"* sounds like it subsumes conformance, and it does not. Of the three conformance
items in this arc, the validator catches **one** (item 16) and is blind to the other (item 13) by
construction. D14's split is not a technicality here; it is the difference between a green suite and a
conformant server.

## F3 — the schema covers 8 of the 20 methods we advertise

| | methods |
|---|---|
| result schema present (8) | `registers`, `run_to`, `checkpoint`, `restore`, `checkpoint_list`, `checkpoint_drop`, `read_memory`, `lookup_symbol` |
| result schema **absent** (12) | `status`, `run_frames`, `pause`, `resume`, `read_vram`, `state_hash`, `screenshot`, `press`, `hold`, `release_all`, `load_symbols`, `reload_rom` |

Not a defect: the schema's own title says **SEED**, and §6 says the remaining per-method schemas are
*"completed mechanically during emulator conformance."* But it means a validator wired in today gives
**100% envelope coverage and 40% result coverage**, and a harness that does not say so reads as though it
checks everything.

So the harness must report the split itself, and fail if it shrinks. This is the same rule the overnight
per-scanline work already applied to a different instrument: *if a harness bounds its own coverage, it
logs what it dropped* — silent truncation reads as "covered everything" when it did not.

(`emulator/write_cram` is schematized but not advertised; we do not implement it. That direction is
harmless — D4 makes the advertised list authoritative.)

## F4 ★ — ten methods put result keys on the wire that appear in no contract text

Measured by diffing each result's key set (stamp fields removed) against §6's catalog row for that method.
Every one of these was confirmed absent from `protocol.md` by grep, not by memory.

| method | §6's row says | we also emit |
|---|---|---|
| `initialize` | §2.1's listed keys | `limits`, `methodSummaries` |
| `emulator/status` | `running,pc,sp,sr,symbolAtPc?,frameToken,symbolCount,romLoading?` | `romBytes`, `romPath`, `symbolsPath` |
| `emulator/read_memory` | `addr,len,bytes,symbol?` | `caveat`, `region` |
| `emulator/read_vram` | `addr,len,bytes` | `caveat` |
| `emulator/state_hash` | `vram,cram,vsram,regs,combined,framebuffer?` | `caveat` |
| `emulator/press` | `buttons,frames,frameToken` | `port` (also an undocumented *param*) |
| `emulator/hold` | `buttons,down` | `port`, `held` |
| `emulator/pause` / `emulator/resume` | *(no result)* | `wasRunning` |
| `emulator/release_all` | *(no result)* | `released` |
| `emulator/checkpoint_list` | `checkpoints[],cursor?,truncated` | `total`, `returned`, `limit` |
| `emulator/run_to` | `target,reached,pc,maxFrames,symbol?,symbolDisp?,caveat?` | `stoppedAtFrame`, `stoppedAtMclk` |

**This is CR-8's offence at scale.** CR-8 was raised retroactively, in this repo's own words, because
`droppedEvents` *"reached the wire with no trace in any document — not in the contract, and not in this
file either."* These are the same thing, ten times over, and the count is a floor: only 33 messages were
sampled and `screenshot` / `reload_rom` / `load_symbols` were not among them.

Two things follow, and they pull in opposite directions, so both are stated:

- **The fix is almost certainly to register them, not to remove them.** CR-4 already settled that optional
  additive fields on a catalogued method are not a new op, and several of these are load-bearing:
  `caveat` is D12's own device applied to reads, `total`/`returned`/`limit` are the house bounded-array
  envelope, `wasRunning` is what makes `pause` idempotent-safe for a client. Deleting them to reach
  conformance would be conforming by amputation.
- **But the direction of travel is the hazard.** Writing the 12 missing schema fragments *from what this
  server emits* would encode the implementation as the contract — the exact inversion of *"the contract
  leads; the implementation follows it, never the reverse."* The source for a schema fragment is §6's
  catalog row. Every key beyond that row is a change request first and a schema edit second, in that
  order, in one pass (D14: *"the two artifacts are amended together, always"*).

## F5 ★ — `lookup_symbol.otherMatches` is the wrong JSON **type**, and the first probe missed it

Added after the fact, and the way it was missed is the point. The 33-message run called
`emulator/lookup_symbol` with a name that does not resolve, so it only ever exercised the **error** path and
reported zero failures for that method. Loading a three-symbol listing first and asking for a prefix that
matches two of them produces:

```
x1  emulator/lookup_symbol  $.otherMatches  {'cursor': 0, 'items': [{'addr': '0x00FF8CFA', …}], …}
                                            is not of type 'array'
```

The schema types `otherMatches` as `{"type":"array","items":{"type":"string"}}`, and §4's prose agrees
(`result:{"addr":…,"name":…,"otherMatches":[…]?}`). We emit the **house bounded-array object** —
`{items, total, returned, cursor, limit, truncated, nextCursor}` — with object items, not strings. So the
divergence is two levels deep: wrong container, and wrong element type inside it.

**And our shape is probably the better one**, which is what makes this a spec bug rather than a server bug.
A bare array is exactly the thing the recon's non-negotiable #2 exists to forbid — *every array is bounded,
cursored, and flags truncation* — and §6.1 states the reason in the checkpoint context: *"a client must
never be handed a partial list it can mistake for a complete one."* `crates/oracle-aether/tests/methods.rs`
pins our envelope under the name `arrays_are_bounded_cursored_and_flag_truncation`.

D14 anticipates precisely this: *"Where the two disagree, that is a spec bug, to be fixed by amendment in
the pass that finds it — and until it is amended, the schema governs what goes on the wire."* So neither
side changes unilaterally. Raised as **CR-14**.

Two things follow for the plan:

- **The validator will catch this on day one**, because the existing suite *does* exercise the success path
  (`methods.rs`) even though my probe did not. That is a real, un-manufactured catch and it is the best
  argument yet that §8 item 15 was worth doing.
- **F1's "only one failure" was an artifact of my sampling**, not a property of the server. A probe that
  drives a method's error path and calls the method covered is measuring its own reach. The in-tree
  validator does not have that weakness: it rides every message every existing test already produces.

Related, found the same way and folded into CR-13 rather than CR-14: `rpc::bounded_array` emits `cursor`
and `nextCursor` as **JSON numbers**, while §8 item 16 says *"a checkpoint `id` and **every list `cursor`**
are JSON strings."* And `lookup_symbol` accepts no `cursor` param at all, so it ships a continuation token
a client has no way to hand back.

---

## What this changes about the plan

1. The validator is worth building and lands as scoped — but its value is **coverage reporting** at least
   as much as type checking, because F1 says the type checking finds one already-known bug.
2. F4 is the larger finding and it is a **contract** deliverable, not a server one. It needs a change
   request and an owner's ruling before any of the 12 missing fragments are written, or the writing itself
   decides the ruling.
3. F2 means item 13 needs its own pinned test, in prose terms, and must not be assumed covered by item 15.

## Reproducing

`scratchpad/probe.py` and `scratchpad/probe2.py` in this session's scratchpad drive the live server and
print the failure list, the coverage split and the per-method key sets. They are throwaway instruments,
deliberately outside the repo: the durable version of this check is the in-tree validator of §8 item 15.
