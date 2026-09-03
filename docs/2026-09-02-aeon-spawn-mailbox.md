# aeon's DEBUG spawn mailbox — the field list, banked from a relay

**✅ ANCHORED 2026-09-02 — aeon `36285940` (chain 206), and the flag is REPLACED rather than deleted, per
the rule that wrote it.** Verified firsthand here, not taken on trust: the object is an **ancestor of their
`origin/master`**, and `--stat` shows it carries **code** — `games/sonic4/config/ram.emp` +58,
`games/sonic4/test/ojz_scroll_test.emp` +262, `tools/test_object_mailbox_contract.py` +379 — so its SHA
class matches what it anchors. `Obj_Req_Def` at `ram.emp:299`, `Obj_Req_Flag` at `:320`, the flag-last
rule at `:279`, all present at that revision. The transcription below was accurate.

*Original flag, kept visible: "RELAY, NOT YET ANCHORED — their branch `parcel/live-objects-spawn` is not
merged, so there is no citable SHA and nothing here has been verified against their tree." That was the
correct posture at the time and it cost nothing; this is what replacing it looks like.*

**Still re-derive the field offsets from their source at this SHA before the CR is filed** — this file is
a transcription of their words, and the CR must be built against the symbol names resolved live per
build, never against the table below.

## ⚠ CORRECTED 2026-09-02 by the CR that consumed this file

`docs/2026-09-02-cr-spawn-mode.md` §2.4 re-derived every claim below from aeon `36285940`. The field
table, widths, order, offsets, op codes, status codes, flag-last rule, one-per-frame bound, dynamic-pool
reach, free-slot policy and the DELETE asymmetry **all re-derive correctly**. Three readings do not:

**(a) MATERIAL — "paused" below means aeon's *game* pause, not the emulator's.** The line *"Requests are
consumed on a paused frame; the object first ticks on resume"* transcribes aeon's
`ENGINE_ARCHITECTURE.md:3189`, which is about `Game_Paused` — declared `aeon:engine/ram.emp:699` as the
*"game pause / freeze flag"* and tested as the first instruction of `RunObjects`
(`aeon:engine/objects/core.emp:471`). It is the Start-button pause **inside the game**. Under an
**emulator** pause no frames execute at all, `objreq_consume` never runs, and the flag is never cleared —
so a client that writes the mailbox and waits for the ack **without advancing the machine hangs forever
against a correctly-working engine**. As written, this file points a reader straight at that design.

**(b) MISLEADING — no field of our `object_list` *is* the handle.** The table below (inheriting aeon's
own `:3169`) says the handle is *"exactly what our `object_list` already reports"*. `object_list` emits
`slot` (a pool **index**) and `addr` (the **full 32-bit** SST address as `"0xFFFF8DC2"`) —
`crates/oracle-aether/src/decoders.rs:636-637`, `hex.rs:15-17`. The handle is the **low 16 bits of
`addr`**. The join is one arithmetic step, and `slot` and `handle` are both small integers that both
plausibly mean "the slot". CR-J §9 makes the server perform the step.

**(c) IMPRECISE — "the level state" is one game state, in a test file.** The consumer is instantiated
exactly once, in `GameState_OJZScroll_Update`
(`aeon:games/sonic4/test/ojz_scroll_test.emp:902`). It is not an engine-wide facility and not present in
every level state. A timeout is still the right client posture; a general "in a level" precondition is
not what the ROM implements.

**One thing this file could not have known, added here:** aeon's own `require_paused`-shaped safety is
weaker than their prose implies. A pause at a mid-frame instruction boundary lets the rest of that
frame's `RunObjects` delete and recycle a slot between the client's listing and `objreq_consume` — so
"list and request from the same paused frame" does **not** close the stale-handle hazard on its own.
CR-J §7.2 and its adjudicator question Q3.

This exists because of the standing rule that a cross-lane commitment living only in mail does not
survive a session boundary.

## The interface rule, which is theirs and is the important part

> Resolve BY SYMBOL from the deb2 table, per shape, per build; the addresses below are a fact about one
> tree, the names are the interface. Absent from `s4.lst` entirely, so a release ROM fails to resolve
> rather than returning a plausible wrong number.

**Build the CR against the symbol names, resolved live via `lookup_symbol` per build, never the offsets.**
The release ROM refusing to resolve is a feature: it is the loud-failure property, not a gap.

## Fields

| offset | symbol | width | dir | meaning |
|---|---|---|---|---|
| +0 | `Obj_Req_Def` | u32 | C→E | SPAWN only. Absolute ROM address of an ObjDef archetype. Ignored by MOVE/DELETE. |
| +4 | `Obj_Req_X` | u16 | C→E | SPAWN/MOVE. World X, flat world PIXELS, same convention as `Warp_Req_X`. |
| +6 | `Obj_Req_Y` | u16 | C→E | SPAWN/MOVE. World Y, same convention. |
| +8 | `Obj_Req_Slot` | u16 | BOTH | MOVE/DELETE (in): slot handle = low 16 bits of the SST address — the **low word of** `object_list`'s `addr`, not any field it reports directly (correction (b)). SPAWN (out): the engine PUBLISHES the new handle here before clearing the flag. |
| +10 | `Obj_Req_Place` | u16 | C→E | SPAWN only. `Load_Object` placement word; engine masks to `$60FF`. |
| +12 | `Obj_Req_Op` | u8 | C→E | 1 SPAWN, 2 MOVE, 3 DELETE. Anything else → status 1. NOT cleared by the ack. |
| +13 | `Obj_Req_Status` | u8 | E→C | Result, valid when the flag reads 0. |
| +14 | `Obj_Req_Flag` | u8 | BOTH | 0 = idle/consumed (the ack); nonzero = pending. |
| +15 | (pad) | | | |

**STATUS:** 0 OK · 1 BAD OP · 2 BAD DEF · 3 POOL FULL (nothing evicted) · 4 BAD SLOT (not a live DYNAMIC
slot) · 5 OWNED (DELETE on an entity-window slot).

## Protocol — aeon's four "an integrator will get these wrong if nobody says them"

* **Write the payload, then the FLAG LAST. That ordering IS the concurrency control.**
* **The cleared flag is an ACK, not success** — it says the engine LOOKED. Status is written on every path
  before the flag clears, so status is valid exactly when the flag reads 0. **The five refusals are
  otherwise silent.**
* **ONE request per frame.** Ten objects = ten round trips.
* **A HANDLE IS AN ADDRESS**, so a slot deleted and recycled between listing and requesting resolves to the
  NEW occupant. **List and request from the SAME PAUSED FRAME.** Requests are consumed on a paused frame;
  the object first ticks on resume. Outside the level state the flag is never acked, so **poll with a
  timeout**. *(Corrected above: "paused" is aeon's `Game_Paused`, not the emulator's pause — see (a);
  and "the same paused frame" does not close the hazard from a mid-frame pause.)*
* **Reach is the DYNAMIC POOL ONLY.** Player, System and Effect handles get status 4; moving the player is
  the warp mailbox's job.

## Free-slot policy, and why DELETE refuses some slots

Refuse, status 3, **nothing evicted** — spawn goes through the engine's own allocator, which pops a free
stack that by construction never holds a live slot. DELETE refuses entity-window-owned slots because the
window clears its loaded bit before deleting, and a bare delete would leave the window believing the
entity is still spawned.

## Release cost

Theirs, unverified here: `s4.bin`, `demo.bin` and `demo.debug.bin` all byte-identical to master by `cmp`,
and `grep -c Obj_Req s4.lst` is 0. Only `s4.debug.bin` moves, +228 assembled + 109 appendix.

## What this lane owes

~~The spawn-mode + picker CR, built against the symbol names.~~ **DELIVERED 2026-09-02:**
`docs/2026-09-02-cr-spawn-mode.md` (CR-J), on `parcel/spawn-mode-cr`, anchored at aeon `36285940` and
`empyrean` `82982b7`. Seven questions stand open for an adjudicator; see its §15.
