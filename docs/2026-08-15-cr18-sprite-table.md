# CR-18 — the sprite attribute table has no row anywhere

**Status: proposed, unruled.** Ranked item 2 of `docs/2026-08-15-handoff-conformance-and-item19.md` §7 —
the last open §8 item-19 violation of the four the 2026-08-15 sweep found (A closed by CR-10, C by moving
`sprite_tile_at` into `oracle-core`, D by CR-11/CR-12).

## The violation

**MEASURED.** `grep` for `emulator/sprite`, `sprite_list` and `emulator/sat` across `contract/protocol.md`
and `contract/schema/bus-protocol.schema.json` returns **zero hits**. There is no method, no §6 row, and no
schema fragment for the sprite attribute table, in either document.

**MEASURED.** The reference player's click-to-inspect panel renders it: `pick.rs:99` calls
`vdp.sprites_decoded()` and `pick.rs:107` calls `vdp.sat_base()`, and the resulting toast names the
sprite's index, position, cell dimensions, base tile, palette and flips. That is a capability a GUI panel
renders with no bus method behind it — §8 item 19, at the SHOULD force D15 gives it.

## ★ Scope: the table, NOT the walk. These are two instruments and only one is in violation.

`oracle-core` has **two** distinct sprite instruments, and the difference decides what this CR may contain:

| | **the table** (`Vdp::sprites_decoded`) | **the walk** (the renderer's evaluation record) |
|---|---|---|
| what it is | all 80 SAT slots decoded as static state | the per-scanline link-walk, in link order |
| what it carries | position, size, tile, palette, flips, priority, link, cache divergence | each walked sprite's outcome (drew / dropped / `Masked`), and why the walk ended (`LinkCut` vs the parse cap) |
| scope | whole-machine, no scanline | one scanline |
| **rendered by a panel?** | **yes** — `pick.rs` | **no** — core-internal, consumed by the renderer |

**MEASURED:** a grep for `sprite_walk`, `SpriteWalk`, `evaluation` and `Masked` across
`crates/oracle-frontend/src/` returns nothing. Only the table escapes into a panel.

So **this CR registers the table and says nothing about the walk.** Item 19's force is "every capability a
GUI panel *renders*"; the walk renders nowhere and is not in violation. Shipping it here would repeat the
error this project has already measured once — the ranked-item-4 conflation of a per-frame *sampler* with
the watch *recorder*, and the ruling note that shipping the recorder and calling it the value trace is
"precisely the ranking error this project measured." The walk is a genuinely better instrument for "why is
my sprite not drawing"; it should be registered **when something renders it**, on its own evidence, not
smuggled in under a row written for the table.

## Proposed: one row, `emulator/sprites`

§6 *VRAM / CRAM / layers*, beside `pixel_attribution`:

| Method | params | result |
|---|---|---|
| `emulator/sprites` | `limit`? | `satBase`, `parsedMax`, `total`, `returned`, `truncated`, `sprites[]` |

Each `sprites[]` entry: `index`, `x`, `y`, `widthCells`, `heightCells`, `link`, `tile`, `palette`,
`hflip`, `vflip`, `priority`, `cacheDivergence`.

### Name

`emulator/sprites`, not `sprite_list`. In this catalog the `_list` suffix belongs to collections of
**bus-owned handles** — `checkpoint_list`, `watchpoint_list` — where the list is of things the server
issued. Machine state reads carry no suffix: `read_vram`, `pixel_attribution`. The SAT is machine state.

### Shape: §2.4's bounded list, in the `watchpoint_list` spelling

`total` / `returned` / `truncated` / `limit`, and **no cursor** — the collection is fixed at 80 entries and
the method accepts no continuation, which is exactly the shape CR-14 ruled for `otherMatches`
(`$defs/boundedList` with one pinned item shape and no continuation at all). Adding a cursor to an
80-element fixed table would be inventing a continuation nothing can page through.

### Types, and the one field deliberately absent

- `index`, `tile`, `palette`, `widthCells`, `heightCells`, `link` — **numbers** (D9 category 2: slot
  indices and counts, where "arithmetic on them is meaningful and permitted").
- `x`, `y` — **signed numbers**. Screen coordinates are `field − 128` and are legitimately negative for a
  sprite entering from the left or top; that is not an error state and MUST NOT be clamped.
- `satBase` — a **hex string** (D9 category 1, an address).
- `hflip`, `vflip`, `priority`, `cacheDivergence` — booleans.

**No per-entry `satAddr`.** It is `satBase + index * 8`, and two rules agree it should not be on the wire:
CR-13 removed result keys that were byte-identical to values already present, and D9 category 2 explicitly
*permits* the client arithmetic that derives it. One address on the envelope, not eighty.

### `parsedMax` — the field that stops the table from lying

**MEASURED (code read).** `Vdp::sprites_decoded` decodes `(0..80)` unconditionally
(`render.rs:621`). The hardware parses at most **80 sprites in H40 and 64 in H32** — the core knows this and
gets it right in the renderer, whose own test asserts *"H32 parses at most 64 sprites"* (`render.rs:2126`).

So in H32 the table hands back 16 slots the hardware would never look at. Both obvious fixes are wrong:

- **clipping to 64** hides bytes that are physically in VRAM, and is the shape of error the watchpoint
  ruling already named — a clipped instrument "reports a negative finding about addresses it never looked
  at";
- **returning 80 silently** presents 16 non-sprites as sprites.

So: return all 80, and emit `parsedMax` (64 or 80) so the caller can tell the two regions apart. It is
**not** derivable client-side — nothing in our 25 methods reports H40 (§6 lists
`read_vdp_registers.decoded.h40Mode`, which this server does not implement), and requiring a second call to
interpret the first is the coupling D11 exists to make explicit.

A per-entry `parsed` boolean is deliberately **not** proposed: it is `index < parsedMax`, i.e. exactly the
derivable key CR-13 struck out.

### `cacheDivergence` is normative, not decoration

The 68000 writes the SAT to VRAM, but the VDP renders Y/size/link from an **internal cache** refreshed on
its own schedule; X and the attribute word come from VRAM live. `sprites_decoded` already compares the two
(`render.rs:635`) and reports disagreement. That is the difference between "the game wrote the sprite" and
"the VDP will draw the sprite", and it is the single most useful field on the row for the question a user
actually asks. It MUST be emitted on every entry, and its `false` is a real answer (the two agree), not an
absent-field.

### Behaviours to pin in prose, following CR-10's precedent

1. **Current state, not the presented frame** — same sentence CR-10 needed. The table is read from live
   VDP state; it is not what the last completed frame was drawn from.
2. **A pure read, exempt from the run-control state rule** — no `-32005`, callable while running (with the
   caveat that a running machine's answer is a sample).
3. **No `-32004`**: unlike `pixel_attribution` there is no coordinate to be out of range, and every one of
   the 80 slots is always readable.

## What this closes, and what it does not

Closes the panel's whole sprite story. With the table on the wire, `sprite_tile_at`'s answer — which
pattern a given dot came from — becomes computable client-side from a sprite's `tile`, `widthCells`,
`heightCells`, flips and position, because a multi-cell sprite is a column-major run from the base tile.
That is the item-19 concern for violation C discharged **by the table**, rather than by a second row.

Does not close: the walk (above, deliberately). Does not touch `pixel_attribution.sprite?`, whose
absent-branch is already recorded as unreachable via the bus.

## Cost

Schema: 25 methods → 26. Server: one handler over an existing, tested core API — `sprites_decoded()` and
`sat_base()` are both public and covered by `render.rs` tests, including the cache-divergence and H32 cases.

**One small core change is required, and an earlier draft of this CR said otherwise.** `parsedMax` needs the
parse cap, and every route to it is private: `Vdp::h40` (`vdp.rs:316`), `Vdp::render_h40`
(`render.rs:521`) and `sprite_limits` (`render.rs:485`, which returns the cap as its third element). So
core gains one accessor — `parsed_sprite_max() -> u8`, returning `sprite_limits(render_h40()).2`.

Export it for the reason core **already** exports `active_geometry`, in a comment written for CR-10's
situation: *"Exported so a caller that has to bound a coordinate — a bus method refusing a dot outside the
display — gets the same answer the renderer resolves against, instead of re-deriving `render_h40` on its
own. The two cannot drift."* A bus method re-deriving the sprite cap would be that same drift, one field
over. The handler MUST NOT compute `64 or 80` itself.

*(Checked, so the CR does not invent a hazard: `Vdp::h40` and `Vdp::render_h40` are byte-identical
expressions — `regs[0x0C] & 0x81 == 0x81` — deliberately duplicated across a module boundary so the
renderer never reaches into private VDP state. They cannot disagree, so "which H40 does `parsedMax` mean"
is not a real question and no prose is spent on it.)*

## ☐ Unruled question for the adjudicator

Should `emulator/sprites` accept an `index` param for a single slot, or is `limit` enough? Against: 80
entries is one small reply and a client filtering an array is not a burden — and a second param shape is
how a read surface starts to sprawl, which is what capability 1 exists to undo. For: the panel's actual
access pattern is **one** sprite by index, and it is the only in-tree consumer. No recommendation offered;
this is the kind of call the register exists to make.
