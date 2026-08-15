# `emulator/pixel_attribution` — closing the §8 item 19 violation, and the sweep that found four more

**Status: design + change request, drafted here for the owner to rule on. No code, no contract edit.**
Base: `0a31d09` on `m68000-microop-framework`.

`empyrean/contract/protocol.md` §8 item 19 (D15's prescriptive half) says:

> every capability a GUI panel renders SHOULD exist as a bus method — and a schema entry — before the
> panel that renders it. **No panel-only capabilities.**

The handoff (`docs/2026-08-15-handoff-capability-layer.md` §2, ranked item 8) names one violation:
`pixel_attribution`. **It is real, and it is not alone.** The sweep in §1 found five, and the one the
handoff named is the third-largest.

---

## 0. A note on method, before any of it counts

Every claim below is anchored to a line I read in this worktree. Where I am *inferring* rather than
reading, the sentence says so. Two of this project's recorded failures are directly relevant: conclusions
measured against an invalid yardstick (the 2026-08-03 fixture mis-identification, thesis 0-for-2), and the
handoff's own finding that **ranking capabilities by how often documents propose them is confidently
wrong** — 201 of 225 mentions in the densest files are proposed, never executed. So §1 ranks by *what a
consumer actually calls*, and §4 refuses several things this repo's own design documents asked for.

**One reading is a judgement call and I am flagging it rather than burying it.** Item 19 says *"a GUI panel
renders"*. Most of the player's inspection output is `println!` to the terminal, not drawn pixels. I read
that as in scope, because `notify()` (`crates/oracle-frontend/src/main.rs:310-313`) deliberately mirrors
every run-loop message to an on-screen toast — *"Every message the run loop used to `println!` goes through
here, so the two can never drift apart"* (`main.rs:307-309`) — so terminal and glass are one surface by
construction. If the owner reads item 19 narrowly as "drawn pixels only", finding **E** in §1 drops out and
nothing else changes.

---

## 1. The sweep — every panel-only capability in the player

Method: read `crates/oracle-frontend/src/` in full, list every `oracle_core` call whose result the player
*shows a person*, and check each against §6's catalog and `schema/bus-protocol.schema.json`. Item 19 has
two clauses — a bus method **and** a schema entry — so a catalogued-but-unschematized method fails it too.

### A. Screen-dot attribution — **the named violation, confirmed**

**What the panel consumes.** `crates/oracle-frontend/src/pick.rs:111`:

```rust
let attr = vdp.pixel_attribution(x, y);
```

reached from the click handler at `main.rs:918` (`let p = pick::resolve(sys.vdp(), x, y);`), and again
from the `shots` diagnostic at `main.rs:2082` (`sys.vdp().pixel_attribution(x, y).winner`).

**What it renders** — three fields of `PixelAttribution`, and only three:

| field | read at | rendered at |
|---|---|---|
| `attr.winner` | `pick.rs:112` | `pick.rs:159-169` (sprite), `:179-186` (backdrop), `:215-220` (plane/window) |
| `attr.cell` (`.tile`, `.palette`, `.priority`) | `pick.rs:204`, `:213`, `:216-219` | `pick.rs:215-221` — `"plane A tile $055 (pal 1 hi-pri) @ VRAM $0AA0-$0ABF"` |
| `attr.cram_index` | `pick.rs:177` | `pick.rs:179-187` — `"backdrop at (x,y) — CRAM entry 37 (palette 2, colour 5) @ CRAM $4A-$4B"` |

**Not rendered anywhere:** `attr.rgb`, `attr.state`, `attr.candidates`. That matters in §4.

**What the catalog and schema lack.** `protocol.md` §6's *VRAM / CRAM / layers* table has eight rows
(`read_vram`, `write_vram`, `read_cram`, `write_cram`, `set_layer_enabled`, `get_layer_states`,
`read_vdp_registers`, `read_vsram`). **None is pixel- or coordinate-shaped.** `bus-protocol.schema.json`
`methods` schematizes nine methods; attribution is not among them. `crates/oracle-aether/src/engine.rs:97-198`
(`METHODS`) has 20 rows; attribution is not among them. Violation on all three counts.

**Aggravating provenance, which strengthens the CR rather than weakening it.** This capability was designed
*bus-first* thirteen months ago. `docs/2026-07-01-vdp-design.md:162` opens §4 with *"Wire form: new
`emulator/<op>` methods on the existing bus protocol (Aether JSON-RPC)"*, and `:173-177` specifies
`pixel_attribution(x, y)` with the winning layer, the decoded entry, the CRAM→RGB chain and *"the ordered
list of losing candidates"*. The core delivered it (`render.rs:1291`, doc comment *"design §4
`pixel_attribution`"*). The bus half was simply never built. **This CR is not an invention; it is the
undelivered half of an approved design.**

**And the whole §4 family is in the same state.** All seven introspection ops that design specified as bus
methods exist in core today and *none* is on the bus: `plane_decoded` (`render.rs:546`), `sprites_decoded`
(`:578`), `render_line_report` (`:1172`), `frame_report` (`:1179`), `pixel_attribution` (`:1291`),
`tile_pixels` (`vdp.rs:2214`), `cram_decoded` (`vdp.rs:2230`). Only the two the panel consumes are item-19
violations; the rest are catalog gaps. §4 says which of them I would still not build.

### B. SAT / sprite decode — **violation, and it has no catalog row at all**

`pick.rs:116` — `let sprites = vdp.sprites_decoded();` — indexed at `:117`, with nine `SpriteDecoded`
fields rendered at `pick.rs:160-169`: `s.x`, `s.y`, `s.width_cells`, `s.height_cells`, `s.tile`,
`s.palette`, `s.hflip`, `s.vflip`, `s.priority`. Plus `pick.rs:124` — `vdp.sat_base()` — producing the
`"SAT entry @ VRAM $B000-$B007"` in the description (`:161`) and the toast (`:170`).

§6 has **no sprite or SAT row anywhere**. The handoff reached this independently: ranked item 1 notes SAT
decode *"has no catalogued method at all"*. `sat_base` is a decoded register field that
`emulator/read_vdp_registers`'s `decoded{…}` row could in principle cover — but that method has no schema
entry and no implementation, so it fails item 19's second clause regardless.

### C. `sprite_tile_at` — **the purest violation in the tree**

`pick.rs:91-103`. Which VRAM pattern a multi-cell sprite drew *this specific dot* from — column-major,
after flips, with the core's wrapping:

```rust
let offset = (src_sx / 8) * usize::from(s.height_cells) + src_sy / 8;
Some(s.tile.wrapping_add(offset as u16))
```

Rendered at `pick.rs:135`, `:143`, `:146` (`"tile $0A3 @ VRAM $1460-$147F"`).

This is not in the catalog, not in the schema, not on the bus — **and not in `oracle-core` either.** It
exists only inside the player's own crate. No other client could obtain it at any price, and it does not
travel: D15's own words are that *"a method travels to a successor server by conformance to this file,
while a panel's internals travel only by being rewritten."* This is that sentence's exact case.

The module header even records that it re-derives core's addressing on purpose (`pick.rs:27-30`), with
tests pinning it against the core's renderer rather than against the arithmetic
(`pick.rs:326-365`). Good engineering, wrong side of the process boundary.

### D. The watchpoint hit log — **the largest violation, and the handoff found it too**

§6's *breakpoints & watchpoints* section has exactly one watch row:

| `emulator/watchpoint_add` | `addr`\|`symbol`, `read`?, `write`? | `addr` |

The player renders three capabilities that row cannot express:

1. **Reading the hits.** `watchpoints.hits()` at `main.rs:451`, rendered at `main.rs:452` (`"--- watch
   hits: N recorded ---"`) and per-hit at `main.rs:464-467`, carrying `seq`, `frame`, `pc` (symbolised via
   `SymbolTable::resolve_within` at `main.rs:461`), `addr`, `old→value`, and `via`. **There is no
   catalogued method that returns a watchpoint hit.** `watchpoint_add` returns `addr` and nothing else.
2. **The drop count.** `watchpoints.dropped()` at `main.rs:469`, rendered as `"dropped: N"`. Structurally
   the same honesty D17 made mandatory for `droppedEvents` — and here it is panel-only.
3. **VDP-internal-space watches.** `main.rs:925-930` calls `add_vdp_watch(space, lo..=hi, …)` with
   `WatchSpace::Vram` / `Cram` (`main.rs:921-924`). `watchpoint_add`'s params are `addr|symbol` with no
   space, so a bus client cannot ask for the *"who wrote this tile?"* watch at all
   (`crates/oracle-core/src/watchpoints.rs:623-628`).

The handoff's ranked item 4 reached this from the other direction — *"`watchpoints.rs` already has
record/count/census modes — **it is simply not on the bus**"* — without connecting it to item 19. It is the
same finding. **On evidence this outranks attribution**, and I say so even though it is not what I was
sent to design.

### E. Symbol-listing integrity diagnostics — **partial, and mostly the contract's lag, not ours**

The player renders the listing's ROM-binding verdict and integrity notes at boot
(`crates/oracle-frontend/src/symbol_file.rs:71-139`): `validate_against_rom` → `RomBinding`, `is_intact()`,
`source()`, `matches_declared_count()`, `declared_count()`, `skipped_lines()`.

§6's row is `emulator/load_symbols | path | path, symbolCount`. **But our bus handler already returns more
than the catalog does** — `binding`, `moduleCount` and a `caveat`, and it *refuses* a mismatched listing
(`crates/oracle-aether/src/engine.rs:1217-1266`), which is §4's *"forward hook"* delivered early. So the
bulk of this is a **CR-7/CR-8-shaped catalog gap (the server shipped ahead of the contract)**, not a
panel-only capability. Only `is_intact` / `declaredCount` / `skippedLines` are genuinely GUI-only, and they
are diagnostics about a *file*, not about the machine.

### F–H. The remainder, reported for completeness and not proposed

| | capability | anchor | verdict |
|---|---|---|---|
| **F** | SRAM presence/dirty/size (`sram_used`, `sram_dirty`, `sram().len()`) | `main.rs:558`, `:561`, `:1391-1402` | Real gap — §6 has no SRAM row. Low value; see §4. |
| **G** | Native active geometry, the `320X224` status field | `main.rs:835` (`render_line(0).len()`), `:1477` → `overlay.rs:297-300` | `read_vdp_registers.decoded{h40Mode}` is catalogued but unimplemented and unschematized. |
| **H** | Console output-stage model (`synth::ConsoleModel`) in the status line | `main.rs:649`, `:688`, `:1475` | **Not machine state** — a player output preference, same class as volume and aspect. Not a bus capability. |

**One thing that looks like a violation and must not be "fixed".** The player's disk-backed save-state
slots (`save_state.rs`, F-keys, `main.rs:999-1072`) have no bus equivalent **because D13 rule 1 forbids
one**: *"A server MUST NOT offer a 'save to file' variant of these methods."* The bus's answer is §6.1's
volatile checkpoints, which we implement. Named here so the next sweep does not file it as a gap.

### Sweep summary

| # | capability | bus method? | schema? | item 19 |
|---|---|---|---|---|
| A | pixel attribution | no | no | **violation** |
| B | SAT / sprite decode | no | no | **violation** |
| C | `sprite_tile_at` | no (not even in core) | no | **violation** |
| D | watch hits / drop count / VDP-space watches | no | no | **violation, largest** |
| E | listing integrity notes | partial (ahead of catalog) | no | catalog gap |
| F | SRAM state | no | no | gap, low value |
| G | native geometry | catalogued, unbuilt | no | gap |
| H | console model | — | — | not a capability |

---

## 2. The change request

Drafted in the CR-1…CR-8 house style of `docs/2026-08-14-aether-change-requests.md`. **Not sent, not
applied to the contract repo** — §8: *"Deviations are raised as change requests against this file, not
implemented unilaterally."*

### CR-9 — there is no coordinate-shaped read, so screen-position → game-state attribution is panel-only

**Contract.** §6's *VRAM / CRAM / layers* table has eight rows, none keyed by a screen coordinate. §8
item 19 requires a bus method and a schema entry *before* the panel. D15: *"A capability that exists only
inside a panel is the `list_ops` drift of §0 re-created in pixels."*

**The gap.** `oracle_core::vdp::Vdp::pixel_attribution` (`crates/oracle-core/src/render.rs:1291`) is
consumed by our own player (`pick.rs:111`, `main.rs:918`, `main.rs:2082`) and by nothing else. The panel
shipped **one day after** item 19 became binding. `docs/2026-07-01-vdp-design.md:162,173-177` had already
specified this as an `emulator/<op>` bus method; only the core half was built.

**What we did.** Consumed it in-process from the player and shipped no bus surface. Recorded here rather
than worked around.

**Proposed change.** Add one row to §6's *VRAM / CRAM / layers* table:

| Method | params | result |
|---|---|---|
| `emulator/pixel_attribution` | `x` (0–511), `y` (0–511) | `x`, `y`, `width`, `height`, `winner{layer,spriteIndex?}`, `cramIndex`, `cramAddr`, `rgb{r,g,b}`, `state`, `cell?`, `sprite?`, `candidates[]` |

Semantics to add to §6 prose (D14: the schema governs shapes, prose governs behaviour):

- **It answers about the VDP's state *now*, not about the presented frame.** The core re-derives the
  scanline on every call (`render.rs:1296`, `let resolved = self.resolve_line(y);`) — there is no
  framebuffer lookup anywhere in the path. See §3.4; this is the single most misreadable thing about the
  method and belongs in the contract, not in a doc comment.
- **It is a pure read.** It does not require a paused machine (§3.3).
- **A dot outside the active display is refused** with `-32004` (§3.5).

### The schema fragment

Ready to paste into `schema/bus-protocol.schema.json` under `methods`. Field-by-field D9 reasoning is in
§3.1.

```json
    "emulator/pixel_attribution": {
      "$comment": "protocol.md §6 (VRAM/CRAM/layers). Answers 'why is the dot at (x,y) the colour it is' against the VDP's CURRENT state — the server re-derives the scanline per call and reads no framebuffer, so on a free-running machine the answer is a live sample and the envelope's `running: true` is what says so (D11). A pure read: NOT subject to §6's run-control state rule.",
      "params": {
        "type": "object",
        "required": ["x", "y"],
        "properties": {
          "x": { "type": "integer", "minimum": 0, "maximum": 511, "description": "Screen column, 0-based (D9 category 2 — a position a client counts with, not an address). Bounded here at the widest addressable value; the ACTIVE bound is the `width` this method reports, and a dot outside it is refused with -32004." },
          "y": { "type": "integer", "minimum": 0, "maximum": 511, "description": "Screen line, 0-based. Same category and the same treatment as `x` — D9 names `line` among the JSON numbers, and this is that field." }
        }
      },
      "result": {
        "allOf": [{ "$ref": "#/$defs/replyFields" }],
        "required": ["x", "y", "width", "height", "winner", "cramIndex", "cramAddr", "rgb", "state", "candidates"],
        "properties": {
          "x": { "type": "integer", "minimum": 0 },
          "y": { "type": "integer", "minimum": 0 },
          "width": { "type": "integer", "minimum": 1, "description": "Active display width the coordinates were resolved against (256 H32 / 320 H40). Reported so a client can bound its own sweep without first provoking a -32004." },
          "height": { "type": "integer", "minimum": 1, "description": "Active display height (224, or 240 in V30)." },
          "winner": {
            "type": "object",
            "required": ["layer"],
            "properties": {
              "layer": { "enum": ["backdrop", "planeB", "planeA", "window", "sprite"], "description": "The displayed layer. camelCase values, matching the `runTo`/`runToScanline` stopped-reason spelling (protocol.md §3)." },
              "spriteIndex": { "type": "integer", "minimum": 0, "maximum": 79, "description": "SAT slot of the winning sprite. Present if and only if `layer` is 'sprite'. A slot index — D9 category 2." }
            },
            "additionalProperties": false
          },
          "cramIndex": { "type": "integer", "minimum": 0, "maximum": 63, "description": "The DISPLAYED colour's CRAM entry (palette*16 + nibble; the backdrop register's value for a backdrop dot). An index — D9 category 2." },
          "cramAddr": { "$ref": "#/$defs/hex", "$comment": "The CRAM byte address of that entry (index*2). An address — D9 category 1, and `cramAddr` is the spelling emulator/write_cram already uses." },
          "rgb": {
            "type": "object",
            "required": ["r", "g", "b"],
            "properties": {
              "r": { "type": "integer", "minimum": 0, "maximum": 255 },
              "g": { "type": "integer", "minimum": 0, "maximum": 255 },
              "b": { "type": "integer", "minimum": 0, "maximum": 255 }
            },
            "additionalProperties": false,
            "description": "The colour actually put on the glass: the 3-bit channels run through the intensity ramp at the resolved shadow/highlight `state`. NOT the stored CRAM components — emulator/write_cram's r/g/b (0-7) are the stored colour, this is the displayed one, and the two differ whenever `state` is not 'normal'. Components, so JSON numbers (D9 category 2); 0-255 because that is the range the ramp emits."
          },
          "state": { "enum": ["shadow", "normal", "highlight"], "description": "The resolved shadow/highlight state applied to this dot's intensity (never its identity)." },
          "cell": {
            "type": "object",
            "required": ["tile", "tileAddr", "palette", "hflip", "vflip", "priority"],
            "properties": {
              "tile": { "type": "integer", "minimum": 0, "maximum": 2047, "description": "Pattern index. A JSON number (D9 category 2) and NOT a hex string: arithmetic on it is meaningful and load-bearing — tile*32 is the VRAM address, and a multi-cell sprite's dot is base+offset. D9's category-4 test is 'a type a client must never compute on'; this is the opposite of that." },
              "tileAddr": { "$ref": "#/$defs/hex", "$comment": "VRAM byte address of that pattern (tile*32, wrapped into VRAM). The address form, so a client can feed it straight to emulator/read_vram. A pattern is 32 bytes and cannot straddle the 64 KB wrap, so this plus 32 is the whole range." },
              "palette": { "type": "integer", "minimum": 0, "maximum": 3 },
              "hflip": { "type": "boolean" },
              "vflip": { "type": "boolean" },
              "priority": { "type": "boolean" }
            },
            "additionalProperties": false,
            "description": "The winning nametable entry, decoded. Present if and only if `winner.layer` is planeA, planeB or window — a sprite or backdrop dot has no nametable cell."
          },
          "sprite": {
            "type": "object",
            "required": ["index", "x", "y", "widthCells", "heightCells", "baseTile", "palette", "hflip", "vflip", "priority", "satAddr"],
            "properties": {
              "index": { "type": "integer", "minimum": 0, "maximum": 79 },
              "x": { "type": "integer", "description": "Screen X of the sprite's top-left, (Xfield & 0x1FF) - 128. Signed: a sprite scrolling in from the left is legitimately negative." },
              "y": { "type": "integer", "description": "Screen Y, (Yfield & 0x3FF) - 128. Signed, same reason." },
              "widthCells": { "type": "integer", "minimum": 1, "maximum": 4 },
              "heightCells": { "type": "integer", "minimum": 1, "maximum": 4 },
              "baseTile": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "The sprite's base pattern index from its attribute word." },
              "tile": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "The pattern this DOT is drawn from — base + (col*heightCells) + row after flips, column-major, wrapping. ABSENT when the winning sprite's box no longer contains the dot, which means the SAT was rewritten between the render and this query; the server reports the absence rather than inventing a tile." },
              "tileAddr": { "$ref": "#/$defs/hex", "$comment": "VRAM byte address of `tile`. Present exactly when `tile` is." },
              "palette": { "type": "integer", "minimum": 0, "maximum": 3 },
              "hflip": { "type": "boolean" },
              "vflip": { "type": "boolean" },
              "priority": { "type": "boolean" },
              "satAddr": { "$ref": "#/$defs/hex", "$comment": "VRAM byte address of this sprite's 8-byte attribute-table entry (sat_base + index*8). This is the range a game writes when it moves, re-points or re-links the sprite — the answer to 'who MOVED this?' as opposed to 'who DREW this?'." },
              "cacheDivergence": { "type": "boolean", "description": "The cached Y/size/link disagree with VRAM at the current reg-5 base — the stale-SAT-cache state made visible (the Castlevania Bloodlines mixing). Present only when true." }
            },
            "additionalProperties": false,
            "description": "The winning sprite, decoded. Present if and only if `winner.layer` is 'sprite'."
          },
          "candidates": {
            "type": "array",
            "minItems": 1,
            "maxItems": 4,
            "description": "Every layer that could have shown at this dot, in VDP priority order, with why each won or lost. Deliberately NOT cursored: the list is bounded at 4 BY CONSTRUCTION (at most one flattened sprite pixel, the plane-A slot, plane B, and the backdrop), not by a server policy a client would have to page around. A live dot yields 3 or 4 entries; a BLANKED dot (display off, or the leftmost-column blank at x<8) yields exactly one — the backdrop, verdict 'won' — which is why minItems is 1 and not 3.",
            "items": {
              "type": "object",
              "required": ["layer", "opaque", "priority", "cramIndex", "verdict"],
              "properties": {
                "layer": { "enum": ["backdrop", "planeB", "planeA", "window", "sprite"] },
                "spriteIndex": { "type": "integer", "minimum": 0, "maximum": 79 },
                "opaque": { "type": "boolean" },
                "priority": { "type": "boolean" },
                "cramIndex": { "type": "integer", "minimum": 0, "maximum": 63, "description": "The colour this layer WOULD have shown." },
                "verdict": { "enum": ["won", "lostToPriority", "transparent", "operator"], "description": "won = displayed. lostToPriority = opaque, but outranked. transparent = colour nibble 0, contributes nothing. operator = an opaque sprite operator (palette 3, nibble 14/15) that outranks the winner but is not drawn — it shifted the winner's shadow/highlight state instead." }
              },
              "additionalProperties": false
            }
          }
        }
      }
    }
```

**The fragment was executed, not just written.** It parses, splices into
`schema/bus-protocol.schema.json` under `methods` without disturbing it (9 methods → 10), the method name
satisfies the D3 request-method pattern, and the `result` sub-schema is a legal draft-2020-12 schema that
accepts a sprite reply, a plane reply and a blanked-dot reply while rejecting: candidates over the
structural bound, `tileAddr` as a bare number, a snake_case `layer` value, a reply missing `droppedEvents`,
and an unknown key inside `cell`. **That run earned its keep immediately — it caught a `minItems: 3` that
contradicted the blanked-dot behaviour documented in the same object** (see §3.2). This is §11.2's own
lesson about the method-name pattern, repeated at small scale: an artifact nothing executes is an artifact
nobody has checked.

---

## 3. The judgement calls, stated as judgement calls

### 3.1 D9 typing — where I argued rather than followed the brief

The brief told me *"CRAM/VRAM addresses and tile ids are hex strings"* and invited me to argue rather than
guess. **I agree on addresses and disagree on tile ids.**

- **Addresses → hex strings** (`cramAddr`, `tileAddr`, `satAddr`). D9 category 1, and `emulator/write_cram`
  already spells one `cramAddr` as `$defs/hex` (schema line 271). Uncontroversial.
- **Tile ids → JSON numbers.** D9 category 2 covers *"slot indices"*, and the ruling that produced category
  4 turned on a specific test: *"A type a client must never compute on should not be a number, because a
  number invites the computation."* A tile index is the exact inverse — computing on it is the point.
  `pick.rs:102` does `s.tile.wrapping_add(offset)` and `pick.rs:80` does `tile * 32`. Making it a hex
  string would force every client to parse it back. **I report both: `tile` as the number and `tileAddr` as
  the hex address**, so no client has to do the multiply and none is tempted to parse the string.
- **Coordinates, indices, cell counts → JSON numbers.** D9 category 2 names `line` explicitly.
- **RGB → JSON numbers, 0-255, as an object not an array.** Neither an address nor a payload, so not
  category 1; components are counted with, so category 2. Object rather than 3-array because
  `write_cram`'s `r`/`g`/`b` are already named keys and a client reading both should not have to remember a
  channel order. **The value is the *displayed* colour after the intensity ramp, not the stored CRAM
  components** — `render.rs:715-723` runs each 3-bit channel through `intensity(…, state)`. Those are
  genuinely different quantities and the schema description says so.
- **Sprite `x`/`y` are signed.** `SpriteDecoded.x/y` are `i16` (`render.rs:64-66`) because the SAT fields
  are biased by 128; a sprite entering from the left has a negative screen X. No `minimum: 0`.

*What would change my mind on tile ids:* a client that only ever wants an address. There is none — the
panel formats the index itself (`pick.rs:135`, `"TILE $0A3"`).

### 3.2 One method or two? — **one.** Recommended, and the reason is measurable

The tempting split is a cheap *"who is at this dot"* against a full candidate/verdict dump. I recommend
against it, on the code rather than on taste:

1. **The split saves the server nothing.** `pixel_attribution` builds the candidate list unconditionally on
   the only path that answers the cheap question (`render.rs:1315-1335`). A "cheap" variant would compute
   the identical work and then discard it. The cost being split is ~4 JSON objects of wire, not compute.
2. **The list is bounded at 4 by construction, not by policy.** `dot_candidates`
   (`render.rs:1237-1262`) allocates `Vec::with_capacity(4)` and pushes at most one sprite pixel, the
   plane-A slot, plane B, and the backdrop — 3 or 4 entries for a live dot, and exactly **1** for a blanked
   one (`render.rs:1315-1323` returns the lone backdrop candidate). That is why `candidates` needs **no
   cursor** — a cursor exists so a client cannot mistake a partial list for a complete one, and a list that
   cannot be partial does not have that failure mode. `maxItems: 4` in the schema is the honest expression
   of it. *(`minItems` is 1, not 3: I first wrote 3 and the schema check in §5.3 caught it against the
   blanked case I had documented two lines away — a small live demonstration of §8 item 15's argument that
   a normative artifact nothing executes is one nobody has checked.)* (Contrast `checkpoint_list`, where the bound is `max_checkpoints`, a *server policy*,
   which is exactly why that one is cursored — `engine.rs:1449-1521`.)
3. **The method's own name is the candidate list.** *Attribution* is "why this and not the others". A
   variant that returns only the winner is `read_pixel`, a different and much smaller question.

*What would change my mind:* a consumer that sweeps thousands of dots per call and is wire-bound. The one
in-tree sweeper (`main.rs:2080-2094`) reads `.winner` only — but it runs in-process and would not be a bus
client (§5), so it does not argue for a split. If a *remote* sweeper ever appears, the right answer is a
line- or rect-shaped method (§3.6), not a thinner dot.

### 3.3 Does it require a paused machine? — **no.** Recommended

§6's run-control state rule names the ops that require a pause: `run_to`, `run_to_scanline`, `run_frames`,
`step*`, `press`, `reload_rom` — and gives the reason: *"they mutate the timeline just as surely."* A pure
read mutates nothing. `read_memory` and `read_vram` are not gated, and gating this one would be a new rule.

The real hazard — that a dot sampled from a free-running machine is a torn instant — is **already solved by
the envelope**. D11's whole argument is that *"a client that stitches four reads into one conclusion may be
reading four different machine states, and with no stamp it has no way to detect that."* The reply carries
`frame`, `mclk` and `running: true`. That is the designed answer, and adding a `-32005` on top of it would
be refusing a request the contract already made safe.

*What would change my mind:* nothing short of the attribution path being made stateful. It is not — see
next.

### 3.4 What if the last frame was never rendered? — **the question does not arise, and the brief's framing here is wrong**

I was asked what happens *"when the last frame was never rendered."* There is no last frame in this path.
`pixel_attribution` calls `self.resolve_line(y)` on entry (`render.rs:1296`) and re-derives the scanline
from live VDP state — VRAM, CRAM, the registers, the SAT cache. It never touches a framebuffer, a capture
buffer, or a cached line. A machine that has rendered nothing since power-on answers exactly as well as one
mid-level; the answer is *"what the VDP would put at this dot right now."*

**This is the single most misreadable property of the method and it belongs in the contract prose.** The
player already knows the hazard and reasons about it in the click path
(`main.rs:910-913`): the view rectangle comes from the last blit, *"not a fresh `render_line` query, which
would answer for the mode the VDP is in **now** (a post-hoc read)"*. A remote client has no such instinct
and will assume it is querying the picture it just screenshotted. It is not. Hence the `$comment` in the
schema and the bullet in the CR.

*Consequence worth stating plainly:* against a free-running machine, `emulator/screenshot` and
`emulator/pixel_attribution` can disagree, legitimately. Pause first, or read the stamp.

### 3.5 A dot outside the active display — **refuse with `-32004`.** Recommended, against the core's behaviour

The core is deliberately total. `xi >= width` falls back to `backdrop_px(xi, backdrop)` and a backdrop-only
candidate list (`render.rs:1315-1323`), and **`y` is not bounded at all** — `resolve_line(1000)` resolves a
line quite happily.

That totality is right for an in-process caller that already knows the frame size. It is wrong on a wire,
for the reason D12 gives about `reached`: *"a result that only echoes its own input cannot distinguish 'my
condition happened' from 'I gave up waiting'."* Here, a client asking about a dot that does not exist would
get a perfectly well-formed backdrop answer and **could not tell it apart from a genuine backdrop dot**.
That is a silent wrong answer, which is the class of failure this bus exists to prevent.

So: `-32004` (§5, *"address out of range"*), with `width` and `height` in `error.data` so the client learns
the bound from the refusal. And `width`/`height` on the **success** result too, so it never has to provoke
one.

Two sub-cases that are **not** errors and must keep answering:
- **`x < 8` with the leftmost-column blank set**, and **display off** — the dot exists and the backdrop
  genuinely is what is shown. `render.rs:1313-1323` already models both. Refusing here would be wrong.
- **A dot inside the active area with no sprite and transparent planes** — backdrop is the true answer.

*What would change my mind:* a consumer that deliberately probes the border/blanking region. None exists;
`main.rs:2080-2081` sweeps `0..HEIGHT` × `0..width` and never leaves the active area.

### 3.6 A `line`-shaped variant? — **not now.** Recommended, with the cost recorded

There is a real efficiency argument. `pixel_attribution(x, y)` resolves the **entire** scanline — all 320
dots plus the full sprite walk — to answer about one dot. The in-tree sweeper at `main.rs:2080-2094`
therefore pays ~80 full line-resolves per line it samples.

I still would not build it yet, because §4's rule applies to me too: the only consumer that would benefit
is a diagnostic subcommand that runs in-process and would not be a bus client. Building a `line` variant
now would be ranking a capability by how good the argument sounds, which is precisely the failure the
handoff quantified.

*What would change my mind — a stated settling experiment:* time `emulator/pixel_attribution` over a
realistic sweep from a real client. If a full-frame attribution sweep is dominated by re-resolution rather
than by wire, the answer is `emulator/render_line_report(line)` — which the core **already has**
(`render.rs:1172`) and `docs/2026-07-01-vdp-design.md:168-172` already specified — not a bespoke variant.

---

## 4. What I would **not** add

D15 makes surface a cost, and the handoff's central finding is that documents propose far more than anyone
executes. Each exclusion below names what it would take to reverse it.

**From `PixelAttribution` itself — nothing is excluded, and here is why that is not a cop-out.** The
honest audit is that the panel renders three of its seven fields (§1A): `rgb`, `state` and `candidates`
have **no consumer today**. But the exclusion test is *value per unit of cost*, and all three are already
computed on the only path that answers the question at all — none adds a line of core, a branch, or a
traversal. `candidates` additionally *is* the method's name (§3.2), and `state`+`rgb` are the difference
between "CRAM entry 37" and "the colour a person is actually looking at", which for a *visual* debugging
method is the point. **If the owner disagrees, `rgb` and `state` are the two to cut** — they are pure
derivation from `cramIndex` plus the S/H state and a client could recompute them from `read_cram`. I would
not cut `candidates`.

**Not proposed, deliberately:**

| | why not |
|---|---|
| **A `targets[]` / armable-ranges field** (what the panel actually arms — `pick.rs:126-145`) | It is a *watchpoint* concern, and this bus has no working watchpoint surface (§1D). Putting watch targets inside a read method designs the watch surface sideways, from inside the wrong method. `cramAddr`, `tileAddr` and `satAddr` are already exactly the inputs a future `watchpoint_add` needs. Fix §1D properly instead. |
| **The nametable-entry address** | `docs/2026-07-01-vdp-design.md:174` promised *"nametable-entry address + decoded entry"*, and the core delivers only the decoded `Cell` (`render.rs:29-40` — no address field). It is a real un-kept promise. It also has **zero consumers**: the panel watches the *pattern*, never the nametable word (`pick.rs:213-227`). Adding it means new core surface for a documented wish. Exactly the ranking error the handoff measured. |
| **`plane_decoded`, `frame_report`, `tile_pixels`, `cram_decoded`** | The rest of design §4's family. All exist in core, none is consumed by any panel, so none is an item-19 violation. They are candidates for the catalog on their own merits, later, on evidence. |
| **A `pixel_attribution` write/poke counterpart** | The handoff's *Do NOT build* list is explicit: a register-write op was *"wanted twice; both times its absence forced a better answer."* |
| **Cursoring `candidates`** | The bound is structural, not policy (§3.2). A cursor over a 4-element list is ceremony that teaches clients to page things that cannot page. |
| **Console model (§1H), volume, aspect** | Player output preferences, not machine state. The bus should not grow a settings surface. |
| **SRAM state (§1F)** | A genuine gap, but the interesting half (does this cart save; is it dirty) has no recorded request from any client, and `state_hash` already answers "did anything change". Register it; do not build it in this pass. |
| **A `line`-shaped variant** | §3.6 — no consumer that would be a bus client. |

**And the one I would put *ahead* of this work:** §1D, the watchpoint hit log. It is a larger item-19
violation than the one I was sent to fix, the core capability is complete
(`crates/oracle-core/src/watchpoints.rs`), the handoff independently ranked it 4th on executed-usage
evidence, and unlike attribution it needs **two** change requests (a hits-reading method, and a `space`
parameter on `watchpoint_add`) because the catalogued row cannot express what the panel already does. I am
flagging it, not designing it — that is the owner's call on sequencing.

---

## 5. Implementation sketch

### 5.1 Should the panel be rewritten to call the bus? — **no.** This is the part to get right

D15 is explicit, and it argues *against* the round-trip in the same breath as it mandates the parity:

> *An in-process GUI is a consumer of the same registry, not a second server.* A debugger or inspector view
> living in the player's own window reads the method registry directly, in-process; it does not open a
> socket to itself. … it buys a process boundary and a wire round-trip per repaint, and couples the view to
> the wire format at the one moment when not being coupled is free.

Our hosting arrangement makes this concrete and slightly worse than the general case. `Host::pump(&mut
sys)` is the **only** point in the loop where anything but the player touches the `System`
(`crates/oracle-frontend/src/bus.rs:150-160`). A click handler that went through the bus would have to
enqueue a command and wait for the next pump — a queue round-trip inside its own process, one frame later,
to answer a question it can answer synchronously from the `&Vdp` already in hand at `main.rs:918`.

**So: the panel keeps calling core.** What item 19 requires is that the *capability* exist on the bus, and
that both consumers derive from **one implementation**, so they cannot drift.

### 5.2 Where the shared code goes — and the constraint that decides it

`oracle-aether` is an **optional** frontend dependency (`crates/oracle-frontend/Cargo.toml:21,31`), while
`pick.rs` is on the unconditional click path. So the shared derivation **cannot live in `oracle-aether`**
without breaking `--no-default-features`. It goes in `oracle-core`.

| file | change | ~lines |
|---|---|---|
| `crates/oracle-core/src/render.rs` | `pub fn sprite_tile_at(&SpriteDecoded, x, y) -> Option<u16>` — move `pick.rs:91-103` in, beside the `draw_sprite` addressing it mirrors. This is §1C's fix and it is the whole of the new core surface. | ~15 |
| `crates/oracle-aether/src/engine.rs` | one `METHODS` row + `Engine::pixel_attribution` handler: parse two counts, bounds-check against the active width/height, call core, serialize. Shaped on `read_vram` (`engine.rs:921-945`). | ~120 |
| `crates/oracle-frontend/src/pick.rs` | delete the local `sprite_tile_at`, call the core one. Behaviour unchanged; its tests keep pinning against the core renderer (`pick.rs:326-365`). | ~-15 |
| `crates/oracle-aether/tests/` | new `pixel_attribution.rs`. | ~180 |
| contract repo | **owner's, on ruling** — the §6 row, the §2 schema fragment, an §11.3 amendment entry. | — |

No change to `crates/oracle-core/tests/` — this adds a method and moves a helper; **no pinned literal
moves**, and that must stay true through review.

Note the handler needs the active `width`/`height`. Width is `render_h40()`-derived (`render.rs:1292-1293`)
and currently private; the smallest honest seam is to compute it in core beside the method rather than
export the predicate.

### 5.3 What the tests would pin

1. **The parity invariant, which is the whole point.** For a fixture VDP, the bus reply's `winner`, `cell`
   and `cramIndex` equal what `pick::resolve` renders for the same dot. If they ever diverge, the panel and
   the bus have drifted — the exact failure item 19 exists to prevent.
2. **`rgb` equals `render_line(y)[x]`.** The core already pins this internally
   (`render.rs:2770`, *"for every pixel, the attribution's RGB equals…"*); the test asserts it survives
   serialization, which is where a channel-order bug would live.
3. **Bounds.** A dot at `width`/`height` refuses with `-32004` and `error.data` carries `width`/`height`;
   `width-1`/`height-1` succeeds. This is the §3.5 decision, pinned.
4. **Blanked-but-valid still answers.** Display off, and leftmost-column-blank at `x < 8`: success with a
   single backdrop candidate, **not** a refusal.
5. **`candidates` is 3 or 4 for a live dot and exactly 1 for a blanked one**, over a randomised sweep —
   the structural bound §3.2 rests on, including the case that corrected the fragment's own `minItems`.
6. **The sprite path end-to-end**, on the `pick.rs` fixture (`vdp_with_sprite`, `pick.rs:288-320`, where
   each pattern is a unique solid colour so the rendered CRAM index *names* the pattern): `sprite.tile` is
   the pattern the renderer actually drew from, for every dot of a 3×2 and a 2×3 sprite under all four flip
   combinations. That fixture is what caught row-major/column-major confusion once already.
7. **`sprite.tile` is absent, not invented**, when the SAT is rewritten between render and query.
8. **No pause required**: the call succeeds against a free-running machine and the reply carries
   `running: true` (§3.3).
9. **Schema conformance**, once the validator from the handoff's work-queue item 1.3 lands — every reply
   validated against `bus-protocol.schema.json`. Two divergences already survived review without one.

### 5.4 Sequencing

This work is blocked on the owner ruling CR-9 in or out. Per §8 and §11.1's *"the discipline worth keeping
is the sequencing — write the contract first, implement second"*, the contract row and schema fragment land
in `empyrean` **before** the handler. If the ruling is "yes, but fix the watchpoint log first" (§4's
closing note, which I think is the stronger evidence-based order), nothing here spoils.
