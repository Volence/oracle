# CR-P — the epoch rule, enumerated over every artifact it covers

**Raised by:** oracle lane, 2026-09-05.
**Target:** `contract/protocol.md` §11.39 (next free), amending §11.38.
**Reviewer:** to be named by the adjudicator in the ruling itself, per the 2026-08-27 substituted-reviewer rule.

## Why this exists

§11.38 (CR-O) established that a watchpoint hit does not survive a boundary, because its `frame`, cycle and
`pc` are all relative to an epoch the boundary ends. It applied that to `reload_rom` and `reset` — the two
cases in front of us.

**This CR is that principle enumerated over its neighbours instead of stopped at its first instance.** It
is filed under a pattern the hub has banked suite-wide, from three instances in two days where reasoning
that was already correct and already written down was applied to one artifact and not the one beside it.
Running the enumeration deliberately here found one thing the ask did not name and one limit that changes
what the ask buys.

## 1. `emulator/restore` — the same argument, and it was simply out of reach of §11.38's text

A checkpoint restore rewinds the frame counter exactly as a reset does. Hits recorded after the capture
point describe a timeline that no longer happened, and their `frame` is indistinguishable from the restored
run's. §11.38 did not raise it and the serving parcel correctly declined to extend past the adopted text.

**Proposed:** `emulator/restore` clears the hit ring and carries `hitsDropped`, identical in shape and
requiredness to §11.38's two rows.

## 2. `emulator/romReloaded` — the count reaches the asking client and not the listening one

§11.38 put `hitsDropped` on the *result*. A client that learns of reloads by subscribing rather than by
calling never sees it.

**Proposed:** the `emulator/romReloaded` event carries `hitsDropped` too, so the listener sees what the
caller sees.

⚑ **The limit that must ride with it, or this reads as more than it is.** This server emits exactly three
events — `emulator/stopped`, `emulator/resumed`, `emulator/romReloaded`. **There is no reset event and no
restore event.** So adding `hitsDropped` to `romReloaded` closes **one of three boundaries** for a
listening client; after this CR a listener still cannot learn that a reset or a restore happened at all,
by any route. That is not a defect this CR fixes and I am not proposing event coverage for the other two —
no consumer has asked to observe them — but a reader who sees item 2 adopted and concludes "listeners are
covered now" would be wrong, and the ruling should say so.

## 3. Breakpoint hit counts — found by enumerating, and DELIBERATELY NOT CHANGED

`crates/oracle-aether/src/breakpoints.rs` carries `pub hits: u64` per breakpoint, and nothing clears it at a
boundary (`hits: 0` appears only at construction). So a breakpoint's fired-count spans reloads, resets and
restores today.

**Proposed: leave it, and record that as a decision.** The CR-O serve already drew this line for the
watchpoint lifetime counters `seen`/`matched`/`dropped`, leaving them alone on the ground that **they
describe the recorder, not the epoch**. A breakpoint's `hits` is the same kind of quantity: an aggregate
over an observer's life, not a record with epoch-relative fields. Clearing it would make §11.38's rule
mean two different things in two places.

**It is in this CR because it was found, and something found must be decided rather than omitted.** A later
reader who notices breakpoint counts surviving should meet a ruling, not a silence they have to interpret.
If the adjudicator draws the line differently, that is fine — but it should be drawn once, for both
counters, and this CR is where.

## The distinction being ratified

Worth stating plainly, since it now governs four artifacts:

| kind | example | at a boundary |
|---|---|---|
| a **record** with epoch-relative fields | a watchpoint hit (`frame`, cycle, `pc`) | **dropped** — it is uninterpretable, not merely stale |
| an **aggregate** over an observer's life | breakpoint `hits`, watch `seen`/`matched`/`dropped` | **kept** — it describes the recorder |

## Suite obligations

Extending §8 item 28 rather than adding an item, since it is the same obligation over more surfaces:
hits recorded before an `emulator/restore` are absent afterwards with `hitsDropped` counting them;
`hitsDropped` present at `0` there too; the `romReloaded` event carries the same count as the reply that
caused it; and — the clause item 28 already earned — a client action still does **not** drop hits.

## Consumer set

Unchanged from CR-O: aeon is the only named consumer of `watchpoint_hits`, and they are the reporter. The
event change affects any subscriber; nothing outside oracle subscribes today.

## What would have to be true for this to be wrong

* If `restore` is contractually a *continuation* rather than a boundary — it does restore a captured state,
  so one could argue the hits belong to the timeline being resumed. I think not, because hits recorded
  *after* the capture point are the ones at issue and those describe a discarded future. But it is a
  reading of `restore`'s intent and the adjudicator owns it.
* If someone does read hits across a boundary, all of this is the wrong trade and the epoch counter §11.38
  declined is the right answer instead. Still no such consumer.
