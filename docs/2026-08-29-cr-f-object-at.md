# CR-F — `emulator/object_at`: one click, one answer, and every failure named

**Date:** 2026-08-29 · **From:** oracle lane · **Against:** `empyrean` `contract/protocol.md` (§6 method
catalog, §2.1's event set) · **Status:** filed for adjudication; nothing served, nothing built.

**Project:** `LIVE-OBJECTS` (declared empyrean `origin/main` `6c5b540`).
**Companion:** `docs/2026-08-29-live-objects-card.md` — the measurement this CR is shaped by.

⚑ **The design call was ADOPTED IN PRINCIPLE by the hub under owner-armed delegation and relayed to
this lane; it is filed here so it lands as a committed artifact rather than a chat agreement.** Recorded
as a relay, not as the owner's own ruling.

## 1. Why a method at all — the case is measured, not argued

The join `pixel_attribution → read_memory(Sprite_Owner + 2i) → rebase → object_slot` **composes from
methods already served**, so this CR adds no capability. It exists because of **where the guard lives.**

Measured on a running game (card §1a): of the three sprites on screen, **two were rings**, whose owner
word is the bare sentinel `0x0001`. A client that rebases without guarding turns `0x0001` into a garbage
index and **confidently names the wrong object** — indistinguishable from a right answer. The guard is
therefore load-bearing on the *first* click anyone makes, not in some edge case.

Client-side composition means every client re-implements that guard. **Server-side means it exists
once.** That is the whole argument; the capability is not the point.

## 2. The method

```
emulator/object_at   params: x, y          (native dot coordinates — the same space `pixel_attribution`
                                            takes, so the two cannot drift apart)
```

Pure read. No `require_paused`, matching `read`/`sprites`/`pixel_attribution`/`scanlines`; the
envelope's `running` remains the contract's whole answer to a torn sample.

### 2.1 Result

```jsonc
{
  "dot":   { "x": 160, "y": 112 },          // echoed, so a reply stands alone in a log
  "world": { "x": 256, "y": 256 },          // ABSENT when unavailable — see `worldSource`
  "worldSource": "camera",                  // "camera" | "unavailable"
  "winner": { "layer": "sprite", "spriteIndex": 0 },   // exactly `pixel_attribution`'s winner
  "owner": {
    "kind": "object",                       // the discriminant — see the table below
    "slot": 0,                              // iff kind == "object"
    "raw":  "0x8ED6"                        // the owner word as read, always, so a caller can audit us
  }
}
```

### 2.2 `owner.kind` — the five outcomes, and why they are five and not two

**This is the substance of the CR.** Every one of these is a different fact about the world, and
collapsing any pair of them is a silent wrong answer rather than a missing feature.

| `kind` | means | `slot` |
|---|---|---|
| `"object"` | the sprite is drawn by a live object slot | present |
| `"ring"` | owner word `0x0001` — a ring, drawn by `DrawRings`, which stamps a bare sentinel and never an address | absent |
| `"mask"` | owner word `0x0002` — an X=0 mask sprite from `InsertSpriteMasks` | absent |
| `"none"` | the owner table **exists** and this entry is `0x0000`: nothing stamped this sprite this frame | absent |
| `"unavailable"` | the owner table **is not in this build at all** | absent |

⚑ **`"none"` and `"unavailable"` are the pair that must never merge, and the reason is asymmetric.**
`"none"` says *the table answered, and the answer is no owner*. `"unavailable"` says *this build has no
table to ask*. Merged, a caller cannot tell "this build cannot answer" from "the answer is no" — and
since a screen of rings legitimately produces `none` for every sprite, the merged shape lets a picker
silently report an empty world.

⚠ **AMENDED WITHIN THE HOUR, and the amendment corrects this CR's own reasoning, not the shape.**
As first filed this paragraph said `Sprite_Owner` is "`DEBUG`-only" and that a folding server "would
answer `none` for every sprite forever". **aeon corrected the premise and I verified it firsthand
against both listings: the symbol is not debug-only-and-reading-zeros, it is ABSENT from the release
build entirely** — 0 occurrences in `s4.lst`, `FFFFE1EE` in `s4.debug.lst`.

**That makes `"unavailable"` cheaper to produce, not less necessary.** A server resolving **by symbol**
gets an unambiguous lookup failure on a shipped ROM and can emit `"unavailable"` with no new engine
support — so the ask in the companion card ("give us a way to detect its absence") is **already
satisfied**, and is withdrawn. What survives is that the distinction must reach the *wire*: the
detection existing server-side is worth nothing if the result shape collapses it before the client sees
it. **The corrected hazard is narrower and still real** — it requires resolving a debug **address**
against a release ROM, which is a discipline failure rather than an unconditional outcome. §2.3 below
now makes that discipline normative, because the camera half fails the same way and does it silently.

`"not an object"` as a single value would collapse four of the five. The card's own measurement is the
argument: two different non-object reasons appeared on one screen within three sprites.

### 2.3 The coordinate space, named because its near-neighbour is a plausible liar

**`world` is act-world, computed as `Camera_X + dot.x`, `Camera_Y + dot.y`, and the symbols are the
UNBIASED `Camera_X` / `Camera_Y`.** Verified exactly: camera `(96, 144)` + dot `(160, 112)` = `(256,
256)`, and `object_slot(0)`'s own `x`/`y`, decoded from the engine's SST by a path sharing no code with
this arithmetic, are `256` / `256`.

⚠ **Naming the field is not pedantry — the adjacent symbol produces a wrong answer that looks right.**
Measured in the same halt: `Camera_X_Biased` = `65504`, `Camera_Y_Biased` = `16`. Read unsigned, 65504
is obviously garbage and would be caught. **Read signed it is −32** — an entirely plausible camera
value that would offset every world coordinate by 128 and be caught by nobody. A CR that said only
"world coordinates" would leave that choice to each implementer.

`worldSource: "unavailable"` (and `world` absent) when the camera symbols do not resolve — **independent
of the `owner` half**, so a build that can answer one and not the other answers the one it can, rather
than refusing both.

### 2.3a ⚑ NORMATIVE: resolve by symbol, per shape, and never carry an address between shapes

**Added by amendment, from aeon's finding, verified here against both listings.** Every address this
method depends on is resolved **by symbol on the loaded ROM's own listing**, per build shape. A server
or client that caches an address and reuses it against a different build is non-conformant.

The reason is measured, and it is the §2.3 hazard again with worse odds:

| symbol | release `s4.lst` | debug `s4.debug.lst` |
|---|---|---|
| `Camera_X` | `FFFFA576` | `FFFFA604` |
| `Camera_Y` | `FFFFA57A` | `FFFFA608` |
| `Sprite_Owner` | **absent** | `FFFFE1EE` |

**`Camera_X` and `Camera_Y` survive a release build — and they MOVE.** A consumer holding the debug
address and reading release RAM gets **no fault and a plausible number**, so click-to-world lands
silently in the wrong place. Unlike `Sprite_Owner`, whose absence announces itself as a lookup failure,
**the camera half has no loud failure at all** — which is why this is normative text rather than advice.

*One correction back to the finding, which strengthens its rule rather than weakening it:* aeon
described the shift as `$8E` "in the same tree". The two listings on this machine were written **two
days apart** (`s4.lst` 08-27, `s4.debug.lst` 08-29), so they are not demonstrably one tree and the
constant `$8E` is not established by them. **The rule does not need it.** If addresses can move between
build *shapes* and also between build *vintages*, carrying one is less safe than the single stated
instance implies, not more.

### 2.4 What is NOT changed

`-32012` (refuse rather than decode from a guessed base) and the `layout` field carry over from the
object decoders unchanged; this method is a member of that `⚙` group and inherits both. No new
refusal code.

## 3. Event vs method — the answer is both, and they are different questions

- **`emulator/object_at` (method)** — *a client asking about a dot it chose.* Above.
- **`emulator/clicked` (event)** — *the person at the window clicked, and a client wants to follow.*
  Payload: the §2.1 result verbatim. This is the half aurora actually needs, because the interesting
  click is the **user's**, and a client cannot poll for it.

The event set is currently exactly three (`stopped`, `resumed`, `romReloaded`), advertised verbatim as
`capabilities.events` — *"the authoritative event set"*. **Adding a fourth is the contract-visible half
of this CR** and is why it is filed rather than served.

**Deliberately NOT proposed here: spawn mode.** A client putting the window into *"a click means place,
not watch"* is a **mode on our window** with a `place` verb that only means something once aeon's
mailbox exists. Filing it now would be specifying a surface against an engine capability that has not
been designed. It gets its own CR when aeon's half has a shape. **Select-and-inspect stands alone and
is useful alone** — that is the declaration's own ordering.

⚑ **SCOPE REDUCED BY THE OWNER, 2026-08-29 — RELAYED, NOT WITNESSED BY THIS LANE. Read this before
drafting that CR; it is smaller than the declaration this document was written against.** His words as
transcribed: *"tbh the click to place is just for debug/throwaway, wasn't planned for permanent"*. The
hub has already amended `contract/projects.json` at empyrean `origin/main` to match and dropped aurora
from the project's lanes — **verified firsthand here**, so this note is a pointer to a corrected
declaration and not a competing account of it.

**What the spawn CR must therefore NOT contain**, since each of these was implied by the original goal
text and is now wrong:
* **no persistence.** A spawned object lives in the running machine and is gone on reset. Nothing is
  written into the level's placements. The original goal said the opposite in as many words.
* **no dependency on aurora.** The object *type* comes from a picker on **our own window**; their
  `ObjectDef` names are a cheap read if we want them and are **not a dependency**. This is aeon's
  *"palette supplies the type, click supplies the position"* — and it is the primary form, not a
  fallback, because it is the only one that works when there is nothing on screen to click.
* **debug-only is permitted.** aeon's mailbox is DEBUG-only, so the picker may be too. That removes the
  whole question of what a shipped build does with a spawn verb.

**Nothing in §11.26 as applied is affected** — this document's own §2 and §3 are the select-and-inspect
half, which the reduction does not touch.

## 4. Open for the adjudicator

0. **Confirm the withdrawn ask.** The card asked aeon for a way to detect `Sprite_Owner`'s absence;
   symbol-lookup failure already is one, so the ask is withdrawn and no engine change is requested by
   this CR. Nothing in `LIVE-OBJECTS` now waits on aeon for the select-and-inspect half.
1. **Method name.** `emulator/object_at` reads as "what object is at this dot", which is the question.
   `pick_at` / `resolve_click` were considered; the first is jargon, the second names the input.
2. **Should `owner.raw` be served?** It is the auditable form and it costs 6 bytes. Against: it invites
   a client to do its own rebase, which is the thing this CR exists to prevent. **Recommendation: keep
   it, and say in the field's own description that it is for auditing and not for rebasing** — the
   guard's value is that ours is right, not that theirs is impossible.
3. **Whether `emulator/clicked` should carry the full result or just the dot.** Full result costs a
   decode per user click on a window nobody may be listening to; the dot alone makes every listener
   round-trip. **Recommendation: full result**, since the decode is cheap and the round trip is the
   thing that makes a live picker feel broken.

## 5. Sequencing

Unchanged and not proposed for change. `BP-WINDOW-CONFIRM` stays in front of the picker: the hosted
breakpoint halt is proven by in-process fixture and **not yet by eye**, and building a picker on that
same fixture evidence is how a well-disclosed caveat quietly becomes an undisclosed foundation. The band
panel having landed (aurora `a44d91c`, relayed) removes the other lane's gate, leaving only ours.
