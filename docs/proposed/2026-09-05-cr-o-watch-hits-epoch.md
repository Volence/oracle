# CR-O — watchpoint hits do not survive a reload or a reset

**Raised by:** oracle lane, 2026-09-05, from a consumer report (aeon, relayed by the hub).
**Target:** `contract/protocol.md` §11.38 (next free) — §6 rows for `emulator/reload_rom` and
`emulator/reset`, plus one §8 clause.
**Reviewer:** to be named by the adjudicator in the ruling itself, per the 2026-08-27 substituted-reviewer rule.

## The defect, as the consumer met it

The watchpoint hit ring survives `emulator/reload_rom` **and** `emulator/reset`. A first read after
reloading handed aeon hits stamped **frames 397 and 655** — recorded against a *previous build* — and they
read as the new run's toggles. Nothing on the wire distinguished them.

A hit is epoch-relative in three of its fields at once: `frame` and the cycle stamp restart at the
boundary, and `pc` is resolved against whatever symbol table is loaded *now*. After a reload that table
may describe a different build entirely, so a surviving hit can name a function that was never at that
address in the image the hit came from. It is not a stale datum; it is an uninterpretable one wearing the
same shape as a live one.

## Why this is a change and not a caveat, in this server's own words

`Engine::reload_rom` already drops the latched framebuffer, and the comment sitting in that function is the
whole argument:

> *"A different cartridge draws a different picture, and the line stream restarts from the reset vector — so
> the frame latched from the previous image is not 'slightly stale', it is another game's. Dropped rather
> than kept, which puts `framebuffer` back on its honest fallback until the new image has drawn a frame of
> its own."*

**A watchpoint hit from the previous image is another game's for exactly the same reason.** The principle
is already ours, already stated, already applied — to the artifact immediately beside this one. This CR
asks only that it be applied consistently.

## What this does NOT overturn

Hits are deliberately durable against *client* actions, and that stays untouched:

* `watchpoint_clear` does **not** delete recorded hits — *"a destructive clear would let one client erase
  another's evidence"* (`engine.rs:7585`);
* reads use `hits()` and never `take_hits()` — *"a draining read is one client stealing another's evidence"*
  (`engine.rs:7654`).

Those protect one client from another. **A reload is not a client action against another client; it is a
discontinuity in the machine both of them are watching.** Different question, different answer, and saying
so explicitly is the point of raising it rather than quietly editing.

## Proposed

1. **`emulator/reload_rom` clears the hit ring**, and its result reports how many hits were discarded.
   Silent clearing would be an absence with nothing left to re-examine; the count makes it loud. This is
   the shape `symbolsDropped` already has on the same method, so it is a precedent followed rather than a
   new idea.
2. **`emulator/reset` clears it too**, and reports the same. Weaker case than the reload — the image and
   symbols are unchanged — but the frame counter restarts, which is *exactly* the confusion the consumer
   hit, and a hit whose `frame` belongs to a previous epoch is indistinguishable from a current one.
3. **§8 clause:** a server advertising `watchpoint_hits` MUST show, in its own suite, that hits recorded
   before a `reload_rom` and before a `reset` are absent afterwards, and that the discarded count is
   reported. A schema cannot witness a clearing obligation, so this rides as conformance, not shape.

## The alternative I considered and am NOT proposing

Keep the hits and add an epoch/build counter so stale ones are *distinguishable* rather than removed. The
machinery already exists — `rom_generation` is bumped in `reload_rom` today — so this is cheap, and it is
the more informative option in the abstract.

Declined, and the reason is the honest one rather than the tidy one: **no consumer has asked to read hits
across a reload or reset.** The one consumer we have asked for the opposite — they were misled by hits
surviving. Building a distinguishing mechanism for a workflow nobody runs, while leaving the demonstrated
misreading available by default, spends the wrong side of the trade. If a lane later wants
reset-and-compare history, the counter is the right answer then and `rom_generation` is waiting.

**If the adjudicator prefers the counter, the case I would want tested is:** who reads hits after a
boundary, and would they rather have an empty list or a list they must filter correctly to be right?

## Consumer set

`watchpoint_hits` has one named consumer, aeon, and they are the reporter. Enumerated across the sibling
trees for callers: nothing else reads it. So the clearing breaks nobody, and the current behaviour has
already produced one wrong reading.

## What would have to be true for this to be wrong

* If a hit's usefulness outlived its image — e.g. someone diffing two builds' watch behaviour in one
  session. That is a real workflow and the clearing forecloses it; the mitigation is to read before
  reloading, which the reporter can do, but if the adjudicator judges the workflow likely then the counter
  wins on the merits.
* If `reset` is understood contractually as a *warm* boundary that preserves observers wholesale. It does
  preserve SRAM and symbols by design. I argue the frame-epoch break is what matters for this artifact
  specifically, but that is a reading of `reset`'s intent and the adjudicator owns it.
