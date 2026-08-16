# CR-20 — the unified read, and the "six methods" that are three reads and three decodes

**Status: proposed, unruled.** Ranked capability 1, and the item the owner's **MCP-first** ruling was
explicitly made to inform: *"letting what it learns decide whether the collapse earns itself."* The MCP
validation is done (`docs/2026-08-15-mcp-as-aether-client-validation.md`), so the evidence is in.

## ☠ The scope correction, first — because the headline count is wrong

The capability is recorded everywhere as *"a unified `read{space, addr|symbol, len}` collapsing **six** read
methods into one."* **MEASURED**, by reading every read-shaped row in §6:

| row | params | result | byte read? |
|---|---|---|---|
| `emulator/read_memory` | `addr`\|`symbol`, `len` | `addr`,`len`,`bytes`,`region` | **yes** |
| `emulator/read_vram` | `addr`?, `len`? | `addr`,`len`,`bytes` | **yes** |
| `emulator/z80_read` | `addr`, `len` | `addr`,`len`,`bytes` | **yes** |
| `emulator/read_cram` | `line`? (0–3) | `palette[]` | **no — a decode** |
| `emulator/read_vsram` | — | `wordCount`,`raw[]`,`planeA[]`,`planeB[]` | **no — a decode** |
| `emulator/read_vdp_registers` | — | `raw[]`,`decoded{…}`,`status{…}` | **no — a decode** |

**Three of the six are not reads.** They take no address and no length; they return *interpretations* —
a palette, the two planes' scroll values, the register file with its bits named. Forcing them into
`{space, addr, len} → bytes` would either throw the decode away or make one method return four unrelated
result shapes, which is the `anyMessage` `oneOf` defect this contract already pins as an open hole.

**So the collapse is 3 → 1, not 6 → 1** — and stating it as six is what made it look bigger than the
methods that would actually merge. This is the third consecutive ranked item whose headline claim did not
survive being checked (CR-19's "six re-implementations" was one; its ARP0 justification was the wrong
half), which is itself worth recording.

### And the three excluded rows are excluded on a principle this register keeps rediscovering

A byte range and an interpretation of that byte range are **two instruments**. CR-18 kept the SAT *table*
apart from the sprite *walk*; CR-19 kept playback apart from recording. Same shape here: `read{space:
"cram"}` hands back 128 bytes; `read_cram` hands back a palette. Both are useful, neither replaces the
other, and merging them would lose the half that took work to build.

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

## ★ The `space` vocabulary already exists on this bus, and is proven

`emulator/watchpoint_add` has taken `space: ["bus", "vram", "cram", "vsram"]` since CR-11/CR-12 shipped,
with the rule that *"spaces never cross-trigger: a numeric collision between a bus address and a VRAM byte
address matches only the watch in its own space."*

A unified read **reuses that enum verbatim** rather than inventing a parallel one — the house rule that
gave CR-18 `baseTile` instead of `tile`, and `cramAddr` its spelling. One vocabulary for "which address
space", defined once, shared by the watch surface and the read surface.

It needs exactly **one new value, `z80`**, for the Z80's own 16-bit space — which is a real extension to
argue, not a free ride: a Z80 address is not a 68000 address, and `bus` already means the 68000's space.

## Proposed

| Method | params | result |
|---|---|---|
| `emulator/read` | `space`? (def `bus`), `addr`\|`symbol`, `len`? | `space`, `addr`, `len`, `bytes`, `region`?, `symbol`?, `symbolDisp`? |

- **`symbol` is valid only with `space: "bus"`** — a symbol names a 68000 address and a VDP-internal byte
  address has no symbol. This is `watchpoint_add`'s existing rule, verbatim, for the same reason, and it
  is already implemented there.
- **`region` stays optional and bus-only**: it answers "which region of the 68000 map did this land in",
  which is meaningless for a VDP-internal space.
- **Per-space bounds**: `len` ≤ 4096 as today; each space's `addr` bounded by its own size (VRAM `$FFFF`,
  CRAM `$7F`, VSRAM `$4F`, Z80 `$1FFF`, bus 24-bit), refused rather than clipped — a clipped read reports
  bytes it never looked at.
- **The two existing rows stay as spellings**, or are removed. See the open question.

## ☐ Unruled questions

1. **Do `read_memory` and `read_vram` survive?** CR-19 ruled that `press` survives `play_input` because
   their *semantics* differ. Here they do **not** differ — `read{space:"vram"}` is `read_vram` exactly.
   That makes this a genuine two-spellings-one-meaning case, which is the thing CR-19's ruling implied
   should not accumulate. But `read_memory` is among the most-executed calls in the corpus and the MCP
   drives it by name. Deprecate-and-keep, or keep both silently?
2. **Is `z80` a `space` value, or does the Z80 keep its own method?** The watch surface's four spaces are
   all *VDP-internal or 68000*; the Z80 is a different processor with a different bus. One vocabulary is
   tidier; "the Z80's space is not one of the 68000's spaces" is the counter-argument.
3. **Does this need `memory_hash` too?** The MCP has it, we do not, and it is a read whose result is a
   digest rather than bytes. Same table-vs-decode question as `read_cram` — probably a separate row, but
   it should be ruled with these, not after.
4. **Does the collapse churn the MCP, and does that matter now?** The owner's sequencing ruling accepted
   that it would. The measured cost is now concrete: `oracle_mcp.py` maps tool names to methods
   mechanically, so `read_vram` → `read{space:"vram"}` is a per-tool special case in a file that currently
   has exactly one (`screenshot`).
