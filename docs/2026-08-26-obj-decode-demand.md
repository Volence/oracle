# OBJ-DECODE demand: `object_list` and `player_state` on the successor (filed by aeon, 2026-08-26)

**Filed by:** aeon-f2, over the peer channel, ~03:00Z 2026-08-26, under the "file it, don't fall back"
arrangement. Measured against `oracle-frontend` at `1000472` (41 methods): both return `no such method`.
**Consumer:** aeon's ring-sparkle gates (an effect-pool object spawned per collected ring) need "what is on
screen" as a witness instead of raw `read_memory` on `Player_1` (`$FF8DB0`) and `Camera_X`, which is what
aeon worked around with tonight. **Not blocking; wanted within the week if cheap.**

## Shape, derived from what the gates will assert (aeon's words, verbatim in substance)

Per slot:
- `slot` (index)
- `pool`: one of `player|dynamic|system|effect`, from the `core.emp` bounds
- `code`: the `Sst.code_addr` word as-is. Aeon's objects are identified by routine offset from
  `ObjCodeBase`, not by a class id. A symbolic name via the `.lst` is a nice-to-have, never required.
- `x`, `y` in WORLD PIXELS: the integer half of the 16.16 at `$02`/`$06`. The gates compare against
  ring/entity coordinates, which are pixel words.
- the raw `sst` bytes, or at least `anim` and `mapping_frame` (offsets to come from `sst.emp` at dispatch).

Empty slots (`code_addr == 0`) omitted, but a `count` so "zero effects" is a stated fact, not an empty list.

## Authorities (all readable from the `.lst` symbols; no aeon change needed)
- SST layout: `aeon/engine/objects/sst.emp`
- Pool bounds: `aeon/engine/objects/core.emp:207-280`
- Symbols: `Player_1`, `Effect_Slots` in `s4.debug.lst`

## Reference policy
No captured legacy sample exists on aeon's side. **The Rust output is the first and only reference; do
NOT A/B against the legacy bridge's shape.** This is the consumer's explicit instruction, not a shortcut.

## Contract note for the dispatcher (corrected the same hour — first draft was wrong)
Neither method has a schema fragment at empyrean `origin/main` (`fc7d7a5`) nor in our vendored copy. Both
are catalogued in protocol.md §6 as ⚙ engine-dependent rows (`:1496-1497`): `object_list` →
`objects[]{slot,…,x,y,class}`, `player_state` → "engine-dependent decoded player struct(s)". So they sit
among the 8 unschematized §6 rows, not the schematized-but-unserved set. Serving them is
**contract-first**: a CR proposing the fragments, shaped from the consumer's list above (note `code` where
§6's sketch says `class`, `pool`, `count`, world-pixel `x`/`y`), adjudicated, landed in empyrean, then
served here. The dispatch brief's first deliverable is that CR.

## Sst layout, as relayed by aeon (read from `engine/objects/sst.emp` at aeon master `f4896139` on origin)

Anchor to that SHA, not to line numbers. `pub struct Sst` size `$50`. The dispatched agent MUST re-read
`sst.emp` at that SHA (or newer, noting drift) and derive offsets from it — this table is the demand's
statement, not the implementation's source of truth.

| field | offset | type/notes |
|---|---|---|
| code_addr | $00 | u16, routine offset from `ObjCodeBase`; 0 = empty slot |
| x_pos | $02 | 16.16 |
| y_pos | $06 | 16.16 |
| x_vel | $0A | |
| y_vel | $0C | |
| render_flags | $0E | u8 |
| collision_resp | $0F | u8 |
| mappings | $10 | u32 |
| art_tile | $14 | u16 |
| width_pixels | $16 | u8 |
| height_pixels | $17 | u8 |
| anim | $18 | u8 |
| subtype | $19 | u8 |
| anim_table | $1A | u32 |
| status | $1E | u8 |
| angle | $1F | u8 |
| prev_anim | $20 | |
| anim_frame | $21 | |
| anim_timer | $22 | |
| mapping_frame | $23 | |
| prev_frame | $24 | |
| sprite_piece_count | $25 | |
| parent_ptr | $26 | u16 |
| sibling_ptr | $28 | u16 |
| slot_tag | $2A | |
| entity_section_id | $2B | u8 |
| entity_list_index | $2C | u8 |
| layer | $2D | u8 |
| frame_off | $2E | u16 |

Pool bounds: read `Player_1` and the pool-length constants from the `.lst`, never hard-code;
`core.emp:207-280` (at the same SHA) documents the contiguous order Player | Dynamic | System | Effect.
