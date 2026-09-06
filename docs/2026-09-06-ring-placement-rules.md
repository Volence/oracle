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

---

## ⚑ Addendum, 2026-09-06: the four things this document does not say, found by building it

*Appended by the `SPAWN-PICKER-RINGS` lane. Everything above is kept as written; this is what
measurement found while implementing it. The durable half of the page above, **the mechanism and the
equate door**, held exactly. What needed adding is what the mechanism implies and the prose did not
follow through to.*

### 1. A ring is not an object, so it could never have been a row in the picker

The row that commissioned this work reads as though a ring were one more thing to select in the spawn
picker's archetype list. It is not, and it cannot be. `emulator/lookup_symbol` over `ObjDef_` finds
**six** archetypes in `s4.debug.lst` and `s4.lst` and **one** in `demo.lst`:

| listing | archetypes |
|---|---|
| `s4.lst` / `s4.debug.lst` | `ObjDef_Spring`, `ObjDef_PathSwap`, `ObjDef_Static`, `ObjDef_Solid`, `ObjDef_Enemy`, `ObjDef_Parent` |
| `demo.lst` | `ObjDef_DemoBox` |

**There is no `ObjDef_Ring` in any of them.** A ring takes no SST pool slot, has no definition record,
and never reaches the `Obj_Req_*` mailbox `emulator/object_spawn` writes: it is a six-byte record in one
flat array that `DrawRings` walks straight into the sprite table and `RingCollision` walks backwards for
the pickup. So ring placement is a **third thing a click can be**, alongside picking and placing an
object, and it gets its own control. Inventing an `ObjDef_Ring` row would have been this window
asserting a name the game does not have, which is what the discovered-not-listed design exists to stop.

### 2. There is a THIRD "cannot place", not two

The page names two: the ring buffer is full, and the section already holds `MAX_LIST_ENTRIES` rings.
Both are real and both are implemented. But a section's ROM ring list is reachable **only** through a
scan-state slot (`EntityScanState_ess_rom_ring_ptr`), and the engine tracks at most
`MAX_TRACKED_SECTIONS` of them. **A click in a section the camera window is not tracking therefore has
no measurable ring count and no index that can be shown safe.** That is unavoidable rather than a design
choice, and it is a friendly refusal, because the way out is to move the camera. It says which sections
*are* tracked so the way out is visible.

### 3. A null `ess_rom_ring_ptr` is UNMEASURABLE, not a count of zero

`EntityWindow_ScanRingsRight` treats a null list pointer as *"no rings ever enter"*, so the obvious
reading is that a null pointer means a real count of zero. **It does not, reliably.**
`EntityWindow_Init` clears the whole `Entity_Scan_State` array to zeroes on a cold boot, which reads
from outside as *section id 0, at origin (0, 0), with a null ring list* — indistinguishable from a
genuine section 0 that publishes no rings. Reading it as zero would place a ring at index 0 of section 0
and, the moment the level really loaded section 0, that is precisely the aliasing rule 2 exists to
prevent, arriving through the check meant to prevent it.

So both are refused, and the refusal says why. Losing the genuinely-empty section costs one placement.
Guessing costs a real ring nobody will ever find again.

### 4. The upper bound has a second, quieter reason

The page gives one reason for `list_index < MAX_LIST_ENTRIES`: the collected mask. There is another, and
it is in aeon's own source as an `ensure`:

```
ensure(ENTITY_LOADED_OBJ_OFFSET*8 == MAX_LIST_ENTRIES, "Loaded-mask half-slot does not cover MAX_LIST_ENTRIES bits")
```

The per-section **loaded** mask's ring half is exactly `MAX_LIST_ENTRIES` bits wide, and the object half
is immediately after it. An index at or above the ceiling does not merely fall off the end; it lands in
the object half and corrupts an object's loaded bit. One bound, two ways to be wrong past it.

There is also a **field-width** ceiling that is not `MAX_LIST_ENTRIES` and must not be confused with it:
`RING_ENTRY_LIST_INDEX_OFFSET` names a **one-byte** field, so a build that raised `MAX_LIST_ENTRIES`
past 255 would have its index silently truncated into a different ring's bit. That is refused out loud
rather than masked.

### Symbols the page's inventory omits, all verified present

`RING_BUFFER_ENTRY_SIZE` ($06) — the page says *"a 6-byte record"* in prose but does not name the
equate, and typing 6 would be exactly the sin the page forbids. Also used and read rather than typed:
`SECTION_SIZE` ($800), `SEC_VOID` ($FF), `EntityScanState_ess_origin_x`/`_origin_y`/`_entry_idx`. The
RAM labels are `Ring_Buffer`, `Ring_Count`, `Ring_HighWater` and `Entity_Scan_State`, and those are
**labels, not equates**, so they go through `lookup_symbol` and get their own refusal: telling a person
their listing has no ring equates when what it is actually missing is `Ring_Buffer` sends them to the
wrong place.

`Ring_HighWater` is written because `RingBuffer_Add` maintains it and this stands in for
`RingBuffer_Add`. **Our own Objects panel reads it**, so skipping it would have made one of our readouts
quietly wrong about a machine we had just written to.

### ▶ BOOKED FOLLOW-UP: `emulator/ring_place`, a capability on one surface only

Confirmed by construction: **ring placement needs no new bus method.** `lookup_equate` for the bounds,
`lookup_symbol` for the four labels, `read_memory` for the scan state and the ROM list,
`write_memory` for the two writes. All served today, and `write_memory`'s work-RAM window
(`$E00000-$FFFFFF`) covers `Ring_Buffer` and `Ring_Count` in every build shape.

**And that is exactly why the gap is worth booking rather than leaving unsaid.** A socket or MCP client
can reach every one of those methods and still cannot place a ring without reimplementing the list walk,
the tracked-section join, the free-index search and both rules above — which means it would reimplement
the two rules that are destructive to get wrong, from a page it may not have read. The capability exists
on the GUI surface only.

The follow-up is a possible `emulator/ring_place { x, y }` CR: same refusal vocabulary
(`ringBufferFull`, `ringSectionFull`, `ringSectionUntracked`, `ringListUnmeasurable`,
`ringBoundsUnknown`, `ringRecordLayoutUnknown`), the safe index chosen server-side, and rule 1 carried
as a `caveat` on the reply so a client cannot receive a placement without receiving the sentence that
says it is temporary. **Named here, not built:** it is a decision to take, not an omission to discover.

### Where the implementation lives

`crates/oracle-frontend/src/rings.rs` is the model and the choreography, shared by both windows and
holding every sentence. `crate::spawn::world_at` and `crate::spawn::in_act` were extracted out of
`spawn::place` so ring placement and object placement share **one** world join and **one** act gate,
rather than a second copy that can answer one click two ways. `crates/oracle-player/src/spawn_picker.rs`
projects the panel; `screen_pick.rs` holds the click and the pause.
