# LIVE-OBJECTS — oracle's priced card

**Date:** 2026-08-29 · **By:** oracle overseer · **Status:** scoping only, nothing built.
**Project:** `LIVE-OBJECTS`, verified firsthand as declared at empyrean `origin/main` (`6c5b540`,
`contract/projects.json`), lanes oracle/aeon/aurora/empyrean.

⚑ **RELAYED, NOT WITNESSED BY THIS LANE.** The assignment reached us through the Aurora-side session
quoting the owner. The *project declaration* is checkable and was checked; the owner's words are quoted
inside it with a timestamp. Recorded as a relay per this repo's standing rule. Nothing here is built.

## 0. The headline, because it changes the price of the whole project

**aeon's crux — "does oracle's click resolve to an object SLOT or only to a pixel/sprite?" — is
ANSWERED, and the answer is SLOT. Measured on a running game, not priced as a guess.** The whole
select-and-inspect chain composes from methods **already served**: no new server method, no new engine
work, and the arithmetic already exists in our tree.

**So aeon's mailbox is the small case.** The engine does not need to do the hit-test.

## 1. The measurement (read-only, lane-owned headless server, nothing of the owner's touched)

`target/release/oracle-aether` at oracle `9f20496`, on a copy of `aeon/s4.debug.bin`
(sha256 `1044da8c94976fdf…`) with its matching `s4.debug.lst`, both lifted out of aeon's rebuild path
first. `load_symbols` reported `binding: "match"`. Ran 600 frames, which leaves the machine halted at a
frame boundary — the coherent moment the OBJ-JOIN recon's §3 item 4 requires, since `Sprite_Owner` is
cleared and re-stamped every `Render_Sprites`.

**Derived, not typed:** `Sprite_Owner` `0xFFE1EE`, `Object_RAM` `0xFF8ED6`, stride **80** from
`Player_2 - Player_1`.

### 1a. Click → sprite → owning slot, three sprites, two outcomes

| dot | winner | `Sprite_Owner[i]` | verdict |
|---|---|---|---|
| (160,112) | sprite #0 | `0x8ED6` | → SST `0xFF8ED6` → **slot 0**, `object_slot(0)` = `active`, `name: "Player_Main"`, `code 0x013C`, `x 256`, `y 256` |
| (152,104) | sprite #0 | `0x8ED6` | same slot from a different dot on the same sprite |
| (212,12) | sprite #1 | `0x0001` | **ring sentinel → "not an object"** |
| (228,12) | sprite #2 | `0x0001` | **ring sentinel → "not an object"** |

**⚑ The ring sentinel fired for real.** The recon predicted `$0001 = ring` by reading aeon's source;
this observes it live, which upgrades a source-read to a measured fact. **Two of the three sprites on
screen were rings** — so the sentinel path is not an edge case, it is the common case, and a naive
rebase of `0x0001` would have produced a garbage index and *confidently named the wrong object*. The
guard `value <= 2 → not an object` is doing real work from the first frame anyone clicks.

### 1b. Click → world, and it is exact

`Camera_X` = **96**, `Camera_Y` = **144**. Dot (160,112) → `camera + dot` = **(256, 256)**.
`object_slot(0)`'s own `x`/`y`, decoded from the engine's SST and not from my arithmetic, are
**x 256, y 256**. **Independent agreement to the pixel** — the check is worth more than the formula
because the two sides share no code path.

### 1c. One hazard found by hitting it

`lookup_symbol` returns **both** `addr` (24-bit, `0x00FFE1EE`) and `rawAddr` (32-bit, `0xFFFFE1EE`).
Feeding `rawAddr` to `read_memory` is refused — *"the 68000 bus is 24 bits wide"* (`-32004`). Correct
refusal, and exactly the recon's own space-mismatch class arriving on a third surface. **A client
joining these methods must take `addr`.** Worth a line in the fragment's own description rather than
leaving each consumer to discover it; the refusal is loud, so this costs a round trip, not a wrong
answer.

## 2. Price, in this lane's terms

| piece | size | needs aeon? | notes |
|---|---|---|---|
| **(1) click → world** | **XS** | **question only** | `camera + dot`, measured exact. Needs `Camera_X`/`Camera_Y` by symbol. |
| **(2) sprite → slot** | **XS–S** | **one ask, small** | Composes today. Server-side vs client-side join is the only design call. |
| **(3a) publish the click over Aether** | **S** | no | New surface — see §3, it is a contract question not a code one. |
| **(3b) spawn mode** | **S–M** | yes (their mailbox) | Our half is window state + a click that means *place*; the placing is aeon's. |
| **(4) GUI half in the player window** | **S** | no | The picker itself, on top of the existing click path and `pick.rs`. |

**Total for oracle: S–M**, matching the declaration's rough size — but **weighted differently than the
declaration assumed**: the parts everyone expected to be hard (1 and 2) are the cheap ones, and the
cost is in (3) and (4), which are surface and UI rather than discovery.

## 3. What we need from aeon — two asks, both small, one is a question

1. **`Sprite_Owner` is `DEBUG` only** (`engine/ram.emp:1145`, their `origin/master`). So the picker
   works on debug ROMs and silently has no owner table on a shipped one — *silently* being the problem,
   since a cleared table reads as "no owner" for every sprite, which is indistinguishable from a scene
   of rings. **Ask: either promote it, or give us a way to detect its absence** so the picker can say
   *"this build has no owner table"* instead of *"nothing here is an object"*. The second is cheaper
   and is probably the right answer.
2. **Question, not an ask: are `Camera_X`/`Camera_Y` present in a non-debug build?** They are in the
   debug listing. If they are debug-only too, (1) inherits the same caveat as (2).

**Nothing else.** In particular we do **not** need a slot→sprite table, which the relay floated: the
join runs the other way (sprite → owner → slot) and already works.

## 4. The design call that is ours, stated so it can be ruled on

**Where does the join run — server-side or client-side?** Both are available.
- **Server-side** (a `clicked` method/field that returns world + slot): the base, stride and slot_count
  are already in `decoders.rs`; the client gets one answer and cannot get the rebase wrong. Costs a new
  surface.
- **Client-side** (aurora composes `pixel_attribution` + `read_memory` + `object_slot`): zero new
  surface, but **every client re-implements the sentinel guard**, and §1a shows that guard is load-bearing
  on the very first click. A client that skips it gets a confident wrong slot.

**Recommendation: server-side.** The sentinel guard is exactly the kind of rule that must exist once. This
is the lane's own call under the standing delegation, but it creates a contract fragment, so it wants an
adjudication before it is served rather than after.

## 5. Sequencing — unchanged and not proposed for change

Building waits behind aurora's band panel and does not pre-empt SIGIL-DECOUPLE or EFFECTS-W1. Our own
`BP-WINDOW-CONFIRM` still sits in front of the picker, per the ordering agreed with aeon: the hosted
breakpoint halt is proven by fixture and not yet by eye, and building a picker on that same fixture
evidence is how a well-disclosed caveat becomes an undisclosed foundation.
