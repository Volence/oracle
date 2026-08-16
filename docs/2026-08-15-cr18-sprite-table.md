# CR-18 — the sprite attribute table has no row anywhere

**Status: RULED 2026-08-16 — adopt with changes.** All sixteen required changes are applied below; the
ruling is recorded in `docs/2026-08-16-ruling-cr18.md`. Ranked item 2 of
`docs/2026-08-15-handoff-conformance-and-item19.md` §7 — the last open §8 item-19 violation of the four the
2026-08-15 sweep found (A closed by CR-10, C by moving `sprite_tile_at` into `oracle-core`, D by
CR-11/CR-12).

> **What the adjudication caught in the proposed draft, kept here rather than quietly fixed.** One
> **fabricated quotation** (a watchpoint-ruling sentence I reconstructed from memory and then put inside
> quote marks — it appears in no document), a **misnamed core symbol** (`active_geometry`, which does not
> exist), a **wrong schema count**, a **wrong enum variant**, two **overstated "no other route" claims**,
> and a **naming rationale that was false as a generalisation**. None changed the design; all of them were
> headed for the contract's permanent log, which is the register that has already had to strike two false
> provenance claims. §11.3's lesson holds: the mechanism earns its cost on the claims, not the conclusions.

## The violation

**MEASURED.** `grep` for `emulator/sprite`, `sprite_list` and `emulator/sat` across `contract/protocol.md`
and `contract/schema/bus-protocol.schema.json` returns **zero hits**. There is no method, no §6 row, and no
schema fragment for the sprite attribute table as a table, in either document.

**MEASURED.** The reference player's click-to-inspect panel renders it: `pick.rs:99` calls
`vdp.sprites_decoded()` and `pick.rs:107` calls `vdp.sat_base()`. The panel's *description* string
(`pick.rs:141–152`) names the sprite's index, position, cell dimensions, base tile, palette and flips; its
*toast* (`pick.rs:153`) carries index, tile and SAT address. That is a capability a GUI panel renders with
no bus method behind it — §8 item 19, at the SHOULD force D15 gives it.

## ★ Scope: the table, NOT the walk. These are two instruments and only one is in violation.

`oracle-core` has **two** distinct sprite instruments, and the difference decides what this CR may contain:

| | **the table** (`Vdp::sprites_decoded`) | **the walk** (the renderer's evaluation record) |
|---|---|---|
| what it is | all 80 SAT slots decoded as static state | the per-scanline link-walk, in link order |
| what it carries | position, size, tile, palette, flips, priority, link, cache divergence | each walked sprite's outcome (drew / dropped / `Masked`), and why the walk ended (`SpriteWalkEnd::LinkZero` vs `MaxCount`) |
| scope | whole-machine, no scanline | one scanline |
| **rendered by a panel?** | **yes** — `pick.rs` | **no** — core-internal, consumed by the renderer |

**MEASURED:** a grep for `sprite_walk`, `SpriteWalk`, `evaluation` and `Masked` across
`crates/oracle-frontend/src/` returns nothing. Only the table escapes into a panel.

So **this CR registers the table and says nothing about the walk.** Item 19's force is "every capability a
GUI panel *renders*"; the walk renders nowhere and is not in violation. **D15's parity rule does not reach
it either** — a renderer consuming its own evaluation record internally is not "a capability its GUI
consumes." Shipping it here would repeat the error this project has already measured once: the
ranked-item-4 conflation of a per-frame *sampler* with the watch *recorder*, which the ruling note calls
"precisely the ranking error this project measured."

This is a **deferral with a trigger**, not a permanent exclusion: the walk is the better instrument for
"why is my sprite not drawing", and it gets its own row **when something renders it**, on its own evidence.

## The row: `emulator/sprites`

§6 *VRAM / CRAM / layers*, beside `pixel_attribution`:

| Method | params | result |
|---|---|---|
| `emulator/sprites` | `limit`? (1–80) | `satBase`, `parsedMax`, `total`, `returned`, `truncated`, `sprites[]` |

Each `sprites[]` entry: `index`, `x`, `y`, `widthCells`, `heightCells`, `link`, `baseTile`, `palette`,
`hflip`, `vflip`, `priority`, `cacheDivergence`.

### Name

`emulator/sprites`, not `sprite_list`, on this ground: **machine-state reads in this catalog carry no
suffix** — `read_vram`, `read_cram`, `registers`, `pixel_attribution`. The SAT is machine state.

*(The proposed draft argued instead that `_list` is reserved for collections of bus-owned handles. That is
false: `emulator/breakpoint_list` (protocol.md:847) and `emulator/object_list` (protocol.md:1005) both
exist, and the latter is a machine-state decode with no server-issued handle in it. `object_list` is a
legacy ⚙ row, not a precedent to follow — but the rationale had to be rebuilt on the true ground.)*

### Shape: `watchpoint_hits`'s spelling, not `watchpoint_list`'s

`total` / `returned` / `truncated` / `limit` flat on the result, with `satBase` and `parsedMax` as
siblings, and **no cursor**.

The precedent is **`watchpoint_hits`** — the adopted row that carries non-list scalars (`seen`, `dropped`,
`matched`) beside a flat list, which is exactly the shape here. This matters because a strict reading of
§2.4's two-spellings clause ("a list that is a *field* of a larger result is a container object") would
otherwise argue for nesting under `$defs/boundedList`.

No cursor is correct per §2.4(b)/(d) and CR-14, which ruled `otherMatches` a bounded list with "no
`cursor`, no `nextCursor`" because the method accepts no continuation. A continuation over a fixed 80-slot
table would be a token nothing can page through.

**Pinned, because a second implementation could reasonably differ:**

- **Order is slot order, index-ascending. Never link order.** With no `limit`, entry *i* is slot *i*. The
  other sprite instrument is link-ordered; this is the one place the two could be confused, and the
  proposed draft left it unspecified.
- **`total` is 80, always** — the table's size. Never `parsedMax`, never `returned`. Without this an H32
  server reporting `total: 64` is a defensible misreading.
- **`limit`** is an integer, `minimum: 1`, `maximum: 80` — bounded, per §11.8's finding that "a `limit`
  bounded on one list and unbounded on its sibling is two policies wearing one name."

### Types, and the one field deliberately absent

- `index`, `baseTile`, `palette`, `widthCells`, `heightCells`, `link` — **numbers** (D9 category 2: slot
  indices and counts, where "arithmetic on them is meaningful and permitted").
- `x`, `y` — **signed numbers**. Screen coordinates are `field − 128` and are legitimately negative for a
  sprite entering from the left or top; that is not an error state and MUST NOT be clamped. This matches
  both the core (`i16`) and the existing `pixel_attribution.sprite` wire treatment.
- `satBase` — a **hex string** (D9 category 1, an address).
- `hflip`, `vflip`, `priority`, `cacheDivergence` — booleans.

**`baseTile`, not `tile`** — the spelling `pixel_attribution.sprite` already puts on the wire for the
identical value (schema line 843, *"The sprite's base pattern index from its attribute word"*). House
precedent is to reuse an existing spelling across methods, as `cramAddr` did.

**No per-entry `satAddr`.** It is `satBase + index * 8`; CR-13 removed result keys derivable from values
already present, and D9 category 2 explicitly *permits* that client arithmetic. One address on the
envelope, not eighty. **This does not conflict with `pixel_attribution.sprite.satAddr`** (schema line 850):
that reply carries no `satBase`, so the address is not derivable there and must be emitted. Same rule, two
answers, because the surrounding results differ.

### `parsedMax` — the field that stops the table from lying

**MEASURED (code read).** `Vdp::sprites_decoded` decodes `(0..80)` unconditionally (`render.rs:621`). The
hardware parses at most **80 sprites in H40 and 64 in H32** — the core knows this and gets it right in the
renderer, whose own test asserts *"H32 parses at most 64 sprites"* (`render.rs:2126`).

So in H32 the table hands back 16 slots the hardware would never look at. Both obvious fixes are wrong:

- **clipping to 64** hides bytes that are physically in VRAM, and is the shape of error the watchpoint
  ruling already named — a silently-dropped watch "produces a `seen`-positive, `matched`-zero reading that
  reads exactly like a negative finding" (protocol.md:889);
- **returning 80 silently** presents 16 non-sprites as sprites.

So: return all 80, and emit `parsedMax` (64 or 80) so the caller can tell the two regions apart.

`parsedMax` belongs on **this** reply because **no method reports the parse cap, and none reports H40 on
this reply**. (It is not strictly underivable: `pixel_attribution.width` is documented "256 H32 / 320 H40",
so a client that called *that* method could infer the mode. Requiring a second call to interpret the first
is the coupling D11 exists to make explicit — which is the argument that carries `parsedMax`, and the
proposed draft's blunter "nothing reports H40" was overstated.)

A per-entry `parsed` boolean is deliberately **not** proposed: it is `index < parsedMax`, i.e. exactly the
derivable key CR-13 struck out.

### `cacheDivergence` is normative, and always present

The 68000 writes the SAT to VRAM, but the VDP renders Y/size/link from an **internal cache** refreshed on
its own schedule; X and the attribute word come from VRAM live. `sprites_decoded` already compares the two
(`render.rs:635`). That is the difference between "the game wrote the sprite" and "the VDP will draw the
sprite", and it is the most useful field on the row for the question a user actually asks.

It MUST be emitted on **every** entry, `false` included — §11.5's `exact` re-ruling holds that "a field
that appears only in the unusual case is a field nobody reads", and `false` here is a real answer (the two
agree), not an absent field.

**Recorded divergence, with its follow-up.** `pixel_attribution.sprite.cacheDivergence` is specified
"Present only when true" (schema line 851). This row sets the opposite convention deliberately. Registered
follow-up: **align `pixel_attribution` to always-present** — cheap, because the property is already
declared, so an always-emitted `false` passes the existing fragment both open and closed.

### Behaviours pinned in prose, following CR-10's precedent

1. **Current state, not the presented frame.** The table is read from live VDP state; it is not what the
   last completed frame was drawn from.
2. **A pure read, exempt from the run-control state rule** — no `-32005`, callable while running.
3. **No `-32004`**: unlike `pixel_attribution` there is no coordinate to be out of range, and every one of
   the 80 slots is always readable.
4. **No `caveat`, and the fragment MUST NOT declare one** (§2.4 rule 4). A running machine's answer is a
   sample, and the envelope's `running: true` already says so — exactly as `pixel_attribution`'s fragment
   states.
5. **Bound to `pixel_attribution.sprite`.** The §6 prose MUST bind the two rows' shared fields to the same
   semantics; the single-sprite decode is already on the wire inside `pixel_attribution`, which uses
   `sprites_decoded`/`sat_base` today (`engine.rs:1330–1339`). Two independently-evolving sprite decodes
   are an invitation to drift.

## The open question, ruled: `limit` alone. No `index` param.

The decisive fact is the **asymmetry of reversal**: an optional `index` added later is additive and
non-breaking; one adopted now can never be removed. The case *for* it is also weaker than it looks — the
panel's one-sprite-by-index access is already served for the flow that produces the index, since
`pixel_attribution.sprite` returns the full winning-sprite decode in the same reply. A client holding an
index from anywhere else pays one small 80-row reply and a filter.

**Reversal condition** (the `sinceSeq`-on-`watchpoint_hits` idiom): adopt `index` when a client is
**measured** paying for whole-table reads it does not want.

## Cost, and the adoption condition

Schema: **26 fragments → 27**; **advertised methods 25 → 26**. *(The proposed draft said "25 → 26" for the
schema, conflating the two counts. Verified: 28 distinct `emulator/*` strings in the schema minus the 2
event fragments = 26, while the server advertises 25 — `write_cram` is schematized and not served.)*

Server: one handler over an existing, tested core API — `sprites_decoded()` and `sat_base()` are both
public and covered by `render.rs` tests, including the cache-divergence and H32 cases.

**One small core change is required, and the proposed draft claimed otherwise.** `parsedMax` needs the
parse cap, and every route to **the cap value** is private: `Vdp::h40` (`vdp.rs:316`), `Vdp::render_h40`
(`render.rs:521`) and `sprite_limits` (`render.rs:485`, which returns the cap as its third element). So
core gains one accessor — `parsed_sprite_max() -> u8`, returning `sprite_limits(render_h40()).2`.

Export it for the reason core **already** exports `active_display` (`render.rs:536`), whose comment reads:
*"Exported so a caller that has to bound a coordinate — a bus method refusing a dot outside the display —
gets the same answer the renderer resolves against, instead of re-deriving `render_h40` on its own. Width
is the length `Vdp::render_line` returns; the two cannot drift."* A bus method re-deriving the sprite cap
would be that same drift, one field over. **The handler MUST NOT compute `64 or 80` itself.**

*(`active_display` is public and encodes H40 in its width, so "every route is private" is true of the cap,
not of the mode. Checked so the CR does not invent a hazard: `Vdp::h40` and `Vdp::render_h40` are
semantically identical — `self.regs[0x0C] & 0x81 == 0x81` vs `self.regs()[0x0C] & 0x81 == 0x81` —
deliberately duplicated across a module boundary so the renderer never reaches into private VDP state. They
cannot disagree, so "which H40 does `parsedMax` mean" is not a real question.)*

**★ ADOPTION IS CONDITIONAL ON THE FRAGMENT BEING EXECUTED, not merely written.** §11.6 and §11.8 both
ruled — and both proved it on themselves — that a registration is done when a conformant reply passes its
**closed** fragment. Required cases: an H32 reply carrying `parsedMax: 64`; a `limit`-truncated reply
carrying all of `total`/`returned`/`truncated`; and an undeclared eleventh key rejected under §8 item 20's
closure.

## What this closes, and what it does not

Closes the panel's whole sprite story. With the table on the wire, `sprite_tile_at`'s answer — which
pattern a given dot came from — becomes computable client-side from a sprite's `baseTile`, `widthCells`,
`heightCells`, flips and position, because a multi-cell sprite is a column-major run from the base tile.
That discharges the item-19 concern behind violation C **by the table**, rather than by a second row.

Does not close: the walk (above, deliberately). Does not touch `pixel_attribution.sprite`'s absent-branch,
already recorded as unreachable via the bus.
