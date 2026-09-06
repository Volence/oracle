# Placing a ring: two rules the symbol channel cannot carry

**Status: the handoff artifact for `SPAWN-PICKER-RINGS`.** Read this before building it.

## Why this file exists at all

The engine lane publishes the ring layout as symbols, and we read them rather than typing them, on the
standing rule that this crate does not assert facts about somebody else's game. **But the listing carries
`EQU <name> = $<value>` and nothing else.** There are no comments in it. So the two facts below, which are
the difference between a working feature and a silently destructive one, **have no channel from their repo
to ours except prose.**

The engine lane said it plainly rather than letting us find out: *"I would rather hand you that as three
plain paragraphs than have you discover the channel cannot carry it."* This file is that handoff made
durable, because **a fact that lives only in a message is a fact with an expiry date nobody can see.**

## Rule 1: a placed ring is swept once off camera, by design

`EntityWindow_DespawnRings` removes any buffer record that leaves the camera window, and
`EntityWindow_TrySpawnRing` only ever re-adds rings **from the section's ROM list**. A record with no ROM
entry behind it therefore **does not come back**.

**The panel must say this in the owner's words.** A ring that vanishes the first time he scrolls, with no
explanation, reads as a broken tool rather than as the design. This is a condition of the feature, ruled
with it, not a polish item.

## Rule 2: the safe index range, and what happens below it

A placed ring's `list_index` must be **at or above that section's real ring count** and **below
`MAX_LIST_ENTRIES`**.

**Below the count it aliases a real ring's collected bit.** Collecting the placed ring then marks a
*different, real* ring as collected, and that ring stops appearing. ⚑ **The damage lands on a different
object at a different time, and nothing about the symptom points back at the debug window.** That is why
this rule is not a nicety.

**The count is not stored anywhere.** Walk the section's list from `EntityScanState_ess_rom_ring_ptr` in
strides of `RING_LIST_ENTRY_SIZE` until a long equal to `RING_LIST_TERMINATOR`.

**A section already holding `MAX_LIST_ENTRIES` rings has no free index at all.** That is the second
"cannot place" state, distinct from the ring buffer being full, and the panel must say **which** of the two
applies rather than greying out for an unstated reason.

## The symbols, all verified present in the listing

Offsets within a 6-byte record: `RING_ENTRY_X_OFFSET` 0, `RING_ENTRY_Y_OFFSET` 2,
`RING_ENTRY_SECTION_ID_OFFSET` 4, `RING_ENTRY_LIST_INDEX_OFFSET` 5.
Walking a section's ROM list: `RING_LIST_ENTRY_SIZE` 4, `RING_LIST_TERMINATOR` 0,
`EntityScanState_ess_rom_ring_ptr` $04, `EntityScanState_len` $1A.
Bounds: `MAX_RING_BUFFER`, `MAX_LIST_ENTRIES` $80, `COLLECTED_BITMASK_OFFSET` $02,
`MAX_TRACKED_SECTIONS` $04.

⚑ **`MAX_RING_BUFFER` is $80 on sonic4 and $10 on demo. Read it. Never type it.** A picker that is correct
on one game and silently wrong on the other is the kind of bug nobody finds.

**Read every one of these through `emulator/lookup_equate`, never `lookup_symbol`.** The two read disjoint
sections of the listing, and a symbol query against an equate refuses in a way that **reads exactly like the
name not existing**. That distinction cost the engine lane an hour and cost this lane a wrong design
decision on the same evening.

## Springs, while the channel is open

`ObjSub_Spring__Up_Red = $00` and `ObjSub_Spring__Up_Yellow = $02`. **Double underscore between the object
name and the subtype name**, single underscores inside the subtype name are free, because the picker builds
its search prefix by substitution from the `ObjDef_` name and never splits on an underscore.

**Two is the true count today, not a partial list.** Left and right are not subtypes at all in this engine;
they are the object's x-flip status bit. The other directions need an engine change, booked there as SP-5.
**Do not present two as a truncation, and do not invent the missing directions.**

### ⚑ Addendum, 2026-09-06: the paragraph above went stale within the day, and SP-5 landed

*Appended by the `SPAWN-PICKER-SUBTYPE` lane. The engine lane's words above are kept as written; this is
what measurement found afterwards, not a correction of what was true when they were written.*

**There are eight spring subtypes, not two, and left and right now ARE subtypes.** Read off
`aeon/s4.debug.lst` and `aeon/s4.lst`, both of which agree:

| name | value |
|---|---|
| `ObjSub_Spring__Up_Red` | `$00` |
| `ObjSub_Spring__Up_Yellow` | `$02` |
| `ObjSub_Spring__Right_Red` | `$10` |
| `ObjSub_Spring__Right_Yellow` | `$12` |
| `ObjSub_Spring__Down_Red` | `$20` |
| `ObjSub_Spring__Down_Yellow` | `$22` |
| `ObjSub_Spring__Left_Red` | `$50` |
| `ObjSub_Spring__Left_Yellow` | `$52` |

SP-5 is in aeon's history as merge `5a97876b`, *"springs in every direction, and the engine reads a vector
rather than a subtype"*, over `abc7fdfa` *"four spring directions from the subtype, and left/right get their
own values rather than S3K's flip bit"*. So the x-flip sentence above is now the **old** design, described
accurately at the time and superseded since. `demo.lst` publishes none of these, which is the second reason
the count is never typed.

**Nothing in the window changed because of this, and that is the point.** The picker reads the namespace,
so it offered eight the moment the listing published eight. What the staleness would have damaged is
anything that *asserted* the count: a fixture frozen at two, or a sentence promising there were no other
directions. Neither exists.

**Two lessons worth keeping, because both are about a document rather than about springs.**

1. ⚑ **A handoff paragraph ages faster than the mechanism it describes.** This one was written hours before
   it stopped being true. The durable half of it, the naming convention and the equate door, is still exact;
   the perishable half was the inventory. Prefer *how to read it* over *what it currently says* whenever
   both will fit.
2. ⚑ **With only the two upward springs, "ordered by value" would have been untestable.** `$00` and `$02`
   are in the same order alphabetically and numerically, so a picker that never sorted would have looked
   correct. The eight do not agree: by name they run Down, Left, Right, Up, and by value they run Up,
   Right, Down, Left. The frozen fixtures in `crates/oracle-frontend/src/bus.rs` and
   `crates/oracle-player/src/spawn_picker.rs` are therefore built to disagree in that dimension **and** to
   carry a value past a byte, which no real build has, so neither guard is vacuous against the corpus it
   will actually meet.
