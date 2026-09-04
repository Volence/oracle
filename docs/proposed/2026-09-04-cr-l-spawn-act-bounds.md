# CR-L — `emulator/object_spawn` should refuse a placement outside the loaded act

**Raised by:** oracle lane, 2026-09-04.
**Target:** `contract/protocol.md` §11.35 (next free), amending the §11.32 spawn row.
**Reviewer:** to be named by the adjudicator in the ruling itself, per the 2026-08-27 substituted-reviewer rule.

## The defect, measured

`emulator/object_spawn` accepts `x`/`y` that are inside the engine's 16-bit position cell but **outside
the loaded act**, writes the mailbox, and answers success. The engine then culls the object on
`RunObjects`' camera-distance test and it never appears. The caller is told a thing happened that did
not happen, and there is nothing on the wire or the glass that says otherwise.

This is not a hypothetical: it is the same defect oracle closed window-side on 2026-09-04
(`F-SPAWN-OUTSIDE-ACT`, merge `c783d96`), where a click outside the act was acked as placed and
silently binned.

## Why this is not a §11.32 oversight

§11.32 was adopted 2026-09-03. It already bounds the coordinates and already states the policy:

> `x` — *"World PIXELS … Bounded by the engine's own 16-bit position cell; out of range is `-32602`,
> refused and never clamped."*

That is a **type** bound — the width of the cell the value is written into. The bound this CR adds is a
**world** bound — the extent of the act actually loaded. The row could not have expressed it, because
the symbols that define it did not exist when the row was written: aeon published `Level_Width` and
`Level_Height` as derived RAM words at `4f7ff49b` on **2026-09-04**, the day after adoption.

So this is the row's own stated policy — *refused, never clamped* — applied to a second bound that
became expressible one day later. It is not a reversal of anything.

## Why the server, and not the caller

Oracle's window-side fix closed one surface. There are three, and they all reach one handler:

* the game window's click gesture (`oracle-frontend`) — **fixed**, window-side;
* the debug window's command line (`oracle-player`'s palette), which resolves any `METHODS` row by name
  and passes typed params straight through — **open**;
* every Aether client and the MCP wrapper — **open**.

Traced firsthand: the palette's `Bus::call` → `Host::call` → the same `Engine::object_spawn`
(`engine.rs:601-602`) that the wire dispatches to. **One refusal in that handler closes all three.** A
per-surface precondition hook closes surface two while surface three stays open, and is a second
implementation of a check that already exists — the parity hazard this suite has a bar about.

The handler already has the shape. §11.32 Q1 established a pre-flight rail there: a `def` outside the
cart window is `-32602` **before any write is attempted**. This CR adds one rail beside it, for the same
reason and at the same point.

## Why this does not overrule aeon

aeon deliberately does **not** clamp the object path, and that decision stands untouched. Their reason
is sound: an out-of-act object is culled harmlessly, so clamping in the engine would be wasted work,
where an out-of-act *player* would reach `SEC_VOID` and so the warp path does clamp.

The two are different actors answering different questions. **aeon's is an engine choice** — do not
spend cycles clamping something that costs nothing. **Ours is a debugger choice** — do not report
success for a request that cannot have an observable effect. The server refusing writes nothing and
clamps nothing; it declines to write a mailbox whose result is already known to be invisible.

This was put to aeon before it was written. Their answer is what produced the symbols above.

## What "outside the act" means, and the trap in it

Valid box is `[0, Level_Width) × [0, Level_Height)`, both resolved **by name, per call**, independently
of each other. Measured on a booted act: both are `$1800` (6144).

⚠ **`Player_Bound_Right` / `Player_Bound_Bottom` are NOT the act bounds.** They are the player's clamp
edges and are **inset**; objects are deliberately unclamped, so a placement between `Player_Bound_Right`
and the true `Level_Width` is **legal and renders**. Refusing there would refuse legitimate placements
**and look correct**, because the refusals would cluster at the edge where a person half expects them.
It is also the symbol a grep for "the bounds" finds first, since the warp path clamps against it. There
is no `Player_Bound_Left`/`_Top` at all — the low edge is a literal `0`.

This matters to the adjudicator because it is the one way a conformant-looking implementation of this CR
can be wrong while passing every test that only clicks far outside the act.

## Three states, three sentences

Oracle's window-side implementation already distinguishes these, and the server should answer the same
three rather than collapsing them:

| state | why it is its own sentence |
|---|---|
| `outsideAct` | the coordinates are outside a known extent |
| `actExtentUnknown` | the listing cannot answer, so the server says **it cannot check** rather than guessing the act is infinite |
| `noActLoaded` | both words are **boot-cleared**, so `0×0` is the *absence of an act*, not an act of no size — telling someone on a title screen their placement is "outside the level" sends them hunting for an edge that does not exist |

`actExtentUnknown` is the loud-on-unmeasurable case and is the one most likely to be dropped as pedantry.
It is the precedent already set by `arm`-refuses-with-no-archetypes: a measurement that cannot be made is
not a measurement of zero.

## What a schema can and cannot witness

**A document schema structurally cannot see this.** `params` describes what a conformant client sends; a
server's duty to REFUSE what falls outside it is behaviour, not shape. This is the same blindness that
let `emulator/step`'s `count` bounds go unenforced for ten days with every artifact green.

So this CR asks for **two** things and they are not the same kind:

1. a **fragment** amendment — `x` and `y` descriptions state the act bound alongside the cell bound, and
   name the three refusal reasons, so a client author reads the obligation where the parameter is
   defined;
2. a **live conformance obligation** — the refusal itself, which rides with the conformance rows the way
   §11.27's *"required when applicable"* half does, never as a schema property.

Please rule on both explicitly. Adopting only (1) produces a documented obligation nothing checks, which
is the shape this suite has now been bitten by twice.

## Consumer set

Enumerated across every sibling tree (`aeon`, `aurora`, `seraph`, `sigil`, `empyrean`), excluding
`node_modules`, vendored trees and worktree copies: **zero** files reference `object_spawn`. Control: 60
aeon files reference some `emulator/` method, so the search can see what is there.

**Nothing outside oracle calls this method today.** Adding a refusal breaks no consumer, and the
pre-release window for a REQUIRED addition shuts at first ship — so this is the cheapest moment this
change will ever have.

## Options

**A — server refuses (recommended).** One rail in `object_spawn_inner`, beside the existing cart-window
pre-flight, before any write. Closes all three surfaces at once. Cost: one more reason a call can fail,
and any future client must handle it. Given the consumer set is empty, that cost is currently zero.

**B — each surface refuses for itself.** Keeps the server permissive. Cost: the check exists twice today
and three times after the next surface; the palette and every Aether client stay open until someone
remembers. This is the option that looks conservative and leaves the defect reachable.

**C — leave it, document it.** Cost: `object_spawn` keeps answering success for placements it knows are
invisible. A capability that returns something plausible is worse here than one that refuses — that is
the cutover's own governing rule, and this is a live instance of it.

**Recommendation: A**, with both halves of the previous section ruled explicitly. The reason to prefer it
over B is not tidiness: B leaves a served method whose success reply is, in a knowable case, false — and
the whole argument for making oracle the suite's default emulator is that a gap refuses by name instead
of degrading to a plausible answer.

## What would have to be true for this recommendation to be wrong

* If some consumer legitimately wants to place an object outside the act — staged off-screen to enter
  later, say. Checked: the camera cannot leave the act, and the legal near-edge strip is *inside*
  `[0, Level_Width)`, so refusing outside the box removes no placement anyone can observe. If the
  adjudicator knows of such a use, A is wrong and B is right.
* If reading two symbols per call is too costly on this path. It is the same by-name-per-call resolution
  the row already mandates for the mailbox cells, so it adds no new class of work.
* If a future mega-act relaxes the build-time `ensure` that caps the grid at `$8000`: `level_width` and
  `level_height` word-wrap above `$FFFF` px, and **aeon's word stores break before our arithmetic does**.
  Recorded so a later reader does not discover the dependency by having it break.
