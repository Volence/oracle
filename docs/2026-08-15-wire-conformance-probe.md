# Probing the live wire against the contract schema (2026-08-15)

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
