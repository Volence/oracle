# OBJ-JOIN recon — the sprite→object join is composable today, and the last link is a transpose hazard

**Date:** 2026-08-27 · **By:** oracle overseer (no subagent — see §0) · **Status:** recon only, nothing built.

**Started under a HUB RULING UNDER OWNER-ARMED DELEGATION** (empyrean `87c015f`,
`docs/OVERSEER.md` "ORACLE ONTO OBJ-JOIN", 2026-08-27T11:00:19Z), which cites the owner's transcribed
words of 05:41:27Z: *"if anything stops, gets stuck, needs a decision, that's where you push it along
unless it's waiting for its work to finish, a sub agent, or another agent."* This lane was stopped and
waiting on none of those three. ⚑ **RELAYED, NOT WITNESSED BY THIS LANE** — recorded as the hub's
ruling, not the owner's; **he may reverse it on return** and nothing here has landed on a shared path.

## 0. Why this recon was run by the overseer and not dispatched

**The session's own configuration forbids calling the Agent tool unless the owner asks for it.** A hub
ruling cannot lift an owner-set constraint on this session — that is the permission-laundering line, and
it holds no matter how well-grounded the ruling is. So the ruling was honoured in the form actually
open: recon performed directly, read-only, **no subagent and no implementation**. The
orchestrate-don't-implement norm yields to the owner's explicit instruction; recon is judgement work
anyway, which is the overseer's own half.

## 1. Verified firsthand — the join exists and the index spaces MATCH

**aeon's side** (all at their `origin/master`):
* `Sprite_Owner: [u16; MAX_VDP_SPRITES]` — `engine/ram.emp:1145`, **DEBUG only**.
* `MAX_VDP_SPRITES = 80` — `engine/system/constants.emp:377`.
* Stamped by `owner_term`: **`move.w a1, (a6,d0.w)`** where `d0` = *"this entry's SAT index"* and `a1` =
  **the owning SST address word** (`engine/objects/sprites.emp`).
* **Cleared to zero** long-wise at the top of every `Render_Sprites` (`MAX_VDP_SPRITES/2` iterations).
* **Three writers**: `owner_term` via `Emit_ObjectPieces`, `InsertSpriteMasks`, `DrawRings`
  (`engine/objects/rings.emp:254`, under `if DEBUG == 1`).

**Our side:**
* `pixel_attribution`'s `winner` is `{"layer": "sprite", "spriteIndex": i}`, present **iff** the winner
  is a sprite (`crates/oracle-aether/src/engine.rs:6155`).
* `SpriteDecoded.index` is documented as **"SAT index 0..=79 (the slot, not the link-walk position)"**
  (`crates/oracle-core/src/render.rs:61`); `SpriteEval.index` likewise *"SAT index of this sprite (the
  slot)"*.

**⚑ THE LOAD-BEARING JOINT, CHECKED BECAUSE NOBODY CITED IT: both sides index by SAT SLOT, and the
ranges agree (ours 0..=79, theirs 80 entries).** Had ours been link-walk order — the obvious
alternative, and what a renderer naturally produces — the join would have silently named the wrong
object for every scene with more than one sprite. Our source distinguishes the two *in the field's own
doc comment*, which is what made this checkable rather than assumable.

**Consequence: the data path needs NO new server method.** `pixel_attribution` + `read_memory` at
`Sprite_Owner + 2*i` + `lookup_symbol` composes it with methods already served.

## 2. ⚠ THE LAST LINK IS A SPACE MISMATCH — an ADDRESS meeting an INDEX

`Sprite_Owner[i]` holds an **SST address word**. `emulator/object_slot` takes **`slot` as an INDEX**,
bounded `0..=slot_count-1` and *"refused rather than clamped"* (`engine.rs:4066`). **These are different
spaces**, so the join cannot complete without a rebase:

```
slot_index = (sst_address - object_pool_base) / sst_stride
```

**This is aurora's transpose hazard arriving on a second surface**, and their formulation transfers
intact: *in-capacity is not in-blob.* Here the danger set is larger than out-of-range:
* **`0`** — the cleared value. Every SAT slot no object drew this frame reads zero. Rebasing it yields a
  negative or wildly wrong index.
* **The mask sentinel** aeon names (`$0002`) — written by `InsertSpriteMasks`, and **never once
  exercised** by their three-sprite witness.
* **Ring entries** — `DrawRings` is a distinct writer; whether it stamps an SST in the same space is
  **NOT verified here** and must be before any picker trusts it.
* **A misaligned or out-of-pool address that still lands IN RANGE** — the genuinely dangerous case,
  because `object_slot` would answer confidently about a real slot that did not draw the sprite. The
  refuse-don't-clamp bound protects against *past the end*, not against *wrong but plausible*.

**So the picker must validate BEFORE converting** — in-pool, stride-aligned, non-sentinel — and must
answer ***"that sprite has no object owner"*** for the rest. Never index, and above all never guess:
per aurora's rule, an unchecked rebase either throws or confidently names something the author does not
own, and the second is indistinguishable from a correct answer.

## 3b. ⚑ ALL THREE CLOSED, 2026-08-27 11:5xZ — from source, no window, no agent

**§3's list is answered. Kept above as written, corrected here, because a doc that quietly edits its own
open questions loses the record of what was assumed at design time.**

**1. The sentinel set is COMPLETE and every member is unusable as an address — which makes the guard
trivial and total.** Read from aeon's `origin/master`:

| value | meaning | writer |
|---|---|---|
| `$0000` | **no owner** — the cleared value every slot no writer touched this frame carries | `Render_Sprites`'s long-wise clear |
| `$0001` | **ring** | `DrawRings` (`rings.emp:257`, *"`$0001` = ring sentinel"*) |
| `$0002` | **X=0 mask sprite** | `InsertSpriteMasks` (`sprites.emp:862`) |
| otherwise | the owning **SST address word** | `owner_term` via `Emit_ObjectPieces` |

**2. §3's guess about `DrawRings` was WRONG, and wrong in the safe direction.** It asked whether rings
stamp *"an SST address in the same space"*. **They do not** — `move.w #1, (a6,d0.w)`, a bare sentinel.
So there are not three address writers and one clearer; there is **one address writer and three
non-address values**. That is a materially easier problem than the one §2 described.

**⚑ And it collapses §2's worst case.** §2 named *"a misaligned or out-of-pool address that still lands
IN RANGE"* as the genuinely dangerous case — a confident wrong answer indistinguishable from a right
one. With the sentinels enumerated, **the entire non-address set is `{0, 1, 2}`**, which no plausible
`Object_RAM` base can collide with. **`value <= 2` → not an object, say so; otherwise rebase.** The
residual risk is not gone (a corrupt or torn read is still possible, and the read must be coherent with
the displayed frame) but it is no longer *"cannot be distinguished"*.

**3. The rebase arithmetic already exists in our tree, derived and cross-checked.**
`crates/oracle-aether/src/decoders.rs` derives **base** (`base_symbols: ["Object_RAM", "Player_1"]`),
**stride** (`stride_pair: ("Player_1", "Player_2")`) and **`slot_count`** (*"Measured, from
`(Object_RAM_End - base) / slot_bytes`"*) **from symbols, not from constants** — and it carries its own
cross-check, deriving the real stride from `Player_2 - Player_1` and reporting *that*. So
`slot_index = (sst_addr - base) / stride` needs no new derivation, only a decision about **where the
join runs**: server-side (all three already in hand) or client-side (they would have to be exposed).
**That is a design call for the parcel, not a missing capability.**

## 3. Open, and honestly unmeasured

1. The `$0002` mask sentinel's exact value and encoding — **read from aeon's `InsertSpriteMasks`, not
   assumed**; aeon states it has never been exercised.
2. Whether `DrawRings` stamps an SST address in the same space as `owner_term`.
3. `object_pool_base` and `sst_stride` — derivable from symbols via the same `decoders::derive` layout
   that already yields `slot_count`, but **not yet confirmed as exposed**.
4. **Timing/coherence.** `Sprite_Owner` is cleared and re-stamped every `Render_Sprites`, so a read is
   only meaningful at a halt where it is populated *and* coherent with the frame on screen. **This is
   exactly why aeon's witness is three sprites wide** — a busier moment was unreachable before the
   hosted breakpoint halt landed. Widening it is now possible and is theirs to run, at our ask.

## 4. Sequencing — unchanged, and deliberately

Recon first; **the picker itself stays behind the windowed breakpoint confirmation**, per the ordering
agreed with aeon: the halt is proven by in-process fixture and not yet by eye, and building a second
parcel on that same fixture evidence is how a well-disclosed caveat quietly becomes an undisclosed
foundation.
