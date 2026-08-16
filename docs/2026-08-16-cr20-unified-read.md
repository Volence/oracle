# CR-20 — the unified read, and the "six methods" that are three reads and three decodes

**Status: RULED 2026-08-16 — adopt with seven changes, all applied below.** Ruling recorded in
`docs/2026-08-16-ruling-cr20.md`.

> **What the adjudication caught.** One **factual error in the proposed bounds** — I narrowed `z80_read`'s
> catalogued `addr` (0–`$3FFF`) and `len` (≤`$2000`) to `$1FFF`/4096 against both the contract and the
> working legacy implementation. Two of the four open questions turned out to be **one** question that
> answers itself. And the strongest argument *for* the collapse was one I underplayed: **a `cram`/`vsram`
> watch hit reports `space`+`addr`, and no method today accepts that pair back.**

Ranked capability 1, and the item the owner's **MCP-first** ruling was explicitly made to inform:
*"letting what it learns decide whether the collapse earns itself."* The MCP validation is done
(`docs/2026-08-15-mcp-as-aether-client-validation.md`), so the evidence is in.

## ☠ The scope correction, first — because the headline count is wrong

The capability is recorded everywhere as *"a unified `read{space, addr|symbol, len}` collapsing **six** read
methods into one."* **MEASURED**, by reading every read-shaped row in §6:

| row | params | result | byte read? |
|---|---|---|---|
| `emulator/read_memory` | `addr`\|`symbol`, `len` | `addr`,`len`,`bytes`,`region` | **yes** |
| `emulator/read_vram` | `addr`?, `len`? | `addr`,`len`,`bytes` | **yes** |
| `emulator/z80_read` | `addr`, `len` | `addr`,`len`,`bytes` | **yes** |
| `emulator/read_cram` | `line`? (0–3) | `palette[]` (+ raw words) | **no — not address-shaped** |
| `emulator/read_vsram` | — | `wordCount`,`raw[]`,`planeA[]`,`planeB[]` | **no — not address-shaped** |
| `emulator/read_vdp_registers` | — | `raw[]`,`decoded{…}`,`status{…}` | **no — not address-shaped** |

**Three of the six are not *address-shaped*.** They take no address and no length, and their results are
keyed by line, plane or register name rather than by position. That — not "they return interpretations" —
is the accurate ground: **all three carry the raw content alongside the decode** (`read_vsram.raw[]`,
`read_vdp_registers.raw[]`, and `read_cram`'s raw words). What forcing them into `{space, addr, len} →
bytes` would discard is the **decode half**, which is real work with a live consumer — the MCP renders
`read_cram`'s decoded palette.

**So the collapse is 3 → 1, not 6 → 1** — and under the ruling on question 1 below it is more honestly
**2 built methods absorbed plus 2 never-built rows**, since nothing is removed. Stating it as six is what
made it look bigger than the methods that would actually merge. This is the third consecutive ranked item whose headline claim did not
survive being checked (CR-19's "six re-implementations" was one; its ARP0 justification was the wrong
half), which is itself worth recording.

### And the three excluded rows are excluded on a principle this register keeps rediscovering

A byte range and an interpretation of that byte range are **two instruments**. CR-18 kept the SAT *table*
apart from the sprite *walk*; CR-19 kept playback apart from recording. Same shape here: `read{space:
"cram"}` hands back 128 bytes; `read_cram` hands back a palette. Both are useful, neither replaces the
other, and merging them would lose the half that took work to build — the half a live client renders.

## What the collapse actually buys

Not "fewer methods". **Two address spaces we cannot read at all today.**

**MEASURED:** our server implements **2 of the 6** rows — `read_memory` and `read_vram`. `z80_read`,
`read_cram`, `read_vsram` and `read_vdp_registers` are catalog rows we have never built, and the MCP
validation found `read_cram`, `z80_read` and `memory_hash` sitting in the "backed by shipped core
capability, exposable" pile — three more read shapes queued behind a surface that already has two.

So the choice is not *collapse six or keep six*. It is:

- **build three more byte-read methods** (`z80_read`, and byte access to CRAM and VSRAM, which have no row
  at all today), each with its own row, fragment, params, bounds and tests; **or**
- **build one `read` with a `space` param** and get all of them, including the two spaces that currently
  have no byte access of any kind.

The collapse is *cheaper than not collapsing*, which is the opposite of how it has been ranked.

**★ And the strongest argument is one an earlier draft underplayed: this is the read half of a surface we
already shipped.** A `cram` or `vsram` watch hit reports `space` **and** `addr` — and **no method on this
bus accepts that pair back.** A client is handed a coordinate it cannot use. The unified read is not
primarily a deduplication; it is the missing counterpart to the watch surface's own output.

## ★ The `space` vocabulary already exists on this bus, and is proven

`emulator/watchpoint_add` has taken `space: ["bus", "vram", "cram", "vsram"]` since CR-11/CR-12 shipped,
with the rule that *"spaces never cross-trigger: a numeric collision between a bus address and a VRAM byte
address matches only the watch in its own space."*

A unified read **reuses that enum verbatim** rather than inventing a parallel one — the house rule that
gave CR-18 `baseTile` instead of `tile`, and `cramAddr` its spelling. One vocabulary for "which address
space", defined once, shared by the watch surface and the read surface.

**And it needs no new value at all.** An earlier draft proposed adding `z80`; question 2 below rules it
out, on the grounds that the reuse argument decides against itself the moment the read enum holds a value
the watch enum refuses. The enum is `watchpoint_add`'s four, unchanged.

## Proposed

| Method | params | result |
|---|---|---|
| `emulator/read` | `space`? (`bus`\|`vram`\|`cram`\|`vsram`, def `bus`), `addr`\|`symbol`, `len`? (def 1, ≤4096) | `space`, `addr`, `len`, `bytes`, `region`?, `symbol`?, `symbolDisp`?, `caveat`? |

- **`symbol` is valid only with `space: "bus"`** — a symbol names a 68000 address and a VDP-internal byte
  address has no symbol. This is `watchpoint_add`'s existing rule, verbatim, for the same reason, and it
  is already implemented there.
- **`region` stays optional and bus-only**: it answers "which region of the 68000 map did this land in",
  which is meaningless for a VDP-internal space.
- **Per-space bounds**: `len`? defaults to **1** and is ≤ 4096; the base **and** `addr + len − 1` are
  bounded by each space's own size — bus 24-bit, VRAM `$FFFF`, CRAM `$7F`, VSRAM `$4F` — and a range whose
  end runs past is **refused (`-32004`), never clipped**, because a clipped read reports bytes it never
  looked at. An unknown `space` is `-32602`.
- **A pure read**: no `-32005` on a free-running machine, as `pixel_attribution` and `sprites` are.
- **`region`, `symbol` and `symbolDisp` are present iff `space: "bus"`**, and that conditionality is
  **schema-enforced in both directions**, per the `old`/`fc` precedent the contract states as *"enforced in
  the schema rather than left to prose"*.
- **`caveat`**: declared in the fragment but emitted **conditionally**. `read_memory` emits a constant
  debug-read caveat today, which §2.4's advisory names as the anti-pattern — a caveat every reply carries
  is one nobody reads. The debug-read property belongs in the row's prose, read once by an implementer.

## The four questions, ruled

1. **`read_memory` and `read_vram` survive, deprecated-and-kept.** The distinction from §11.11 holds and
   was tested: `press` survived `play_input` because union-vs-replace is a *semantic* difference, whereas
   `read{space:"vram"}` and `read_vram` are **identical** — a genuine two-spellings case. But removal is
   barred by D5 and by a live client that drives both by name, and the catalog already has the mechanism:
   the inline marker, as `emulator/wait_for_break` *(deprecated by `stopped`)* carries. Both rows gain
   *(deprecated by `read`)* and are defined as **exact aliases**, each keeping its current defaults.
   Nothing is removed. *(A pleasing closure: the legacy socket op was literally named `read` — §6 still
   records `emulator/read_memory ← read` — and `read` with `space` defaulting to `bus` is its superset.)*
2. **`z80` is NOT a `space` value; `z80_read` stays its own method.** Three grounds. The reuse argument
   decides it against itself: "reuses the enum **verbatim**" stops being true the moment the read enum
   holds a value the watch enum refuses, which manufactures the two-enums-one-name drift the reuse was
   meant to prevent — and extending the *watch* surface to the Z80 is unfunded work no client asked for.
   The bounds mismatch found above is the design saying the same thing. And `z80_registers` / `z80_write`
   stay Z80-named regardless, so a `z80_read` sibling is the coherent surface. `z80_read` is already a
   catalogued row with pinned bounds (`addr` 0–`$3FFF`, `len` ≤ `$2000`): implementing it is conformance,
   not design.
3. **`memory_hash` is struck from this CR** — digest-shaped, with its own obligations (the algorithm must
   be pinned as `state_hash`'s is, since cross-server comparability is its whole point). One sentence
   reserves this `space` vocabulary for it if it is ever adopted with multi-space reach.
4. **The MCP-churn question dissolves into question 1.** Deprecate-and-keep means the MCP keeps calling
   `read_memory` and `read_vram` by name with **zero** client changes. The measured churn is not "one
   special case" — it is nothing. An earlier draft presented these as two independent questions; they are
   one.

## Cost, and the adoption condition

Schema **28 → 29** fragments; advertised **27 → 28**. Core needs nothing new: `Vdp::cram`, `Vdp::vsram`
are public and the bus read path already exists.

Under question 1 the count is honest about itself: **2 built methods absorbed plus 2 never-built rows**,
not "3 → 1" — the CR's own arithmetic habit, applied once more to its own arithmetic.

**★ ADOPTION IS CONDITIONAL ON THE FRAGMENT BEING EXECUTED**, per §11.6 / §11.8 / §11.10 / §11.11: a
conformant reply passes it **closed** for **each of the four spaces**, plus one refused out-of-bounds case
per space.
