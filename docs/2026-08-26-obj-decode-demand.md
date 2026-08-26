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

## Contract note for the dispatcher
Both methods already carry schema fragments in the vendored contract (they are among the 17
schematized-but-unserved). The consumer shape above must be reconciled with those fragments before code:
where they agree, serve the fragment; where the consumer needs a field the fragment lacks (`pool`, `count`,
world-pixel `x`/`y` vs 16.16), that is a CR against empyrean, adjudicated before serving, per the
contract-first bar. The dispatch brief must include that reconciliation as its first step.
