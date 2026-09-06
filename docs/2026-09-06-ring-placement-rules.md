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
