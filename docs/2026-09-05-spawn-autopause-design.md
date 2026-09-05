# Spawn auto pause, and the picker panel: the design, ruled before it is built

**Status: ruled and queued, not built.** Sits with `SPAWN-PICKER-PANEL`, after the first style pass, as one
parcel. Ruled by the hub under the owner's standing delegation on 2026-09-05, from this lane's measured
answer; **overturnable by the owner**, and recorded as the hub's ruling rather than his.

## What the owner asked for

Verbatim: *"For spawn needing pause, can the click of the object pause for 1 ms or whatever is needed and
spawn it? like programaticallyy instead of me needing to manually pause. that's fine waiting for after the
look rules exist."*

## The answer, with the numbers rather than a reassurance

**At most two frames of emulated time, commonly one.** `OBJREQ_DEFAULT_MAX_FRAMES = 2`
(`crates/oracle-aether/src/engine.rs:9114`), and the 2 is itself measured, with its reasoning banked in
`docs/2026-09-02-cr-spawn-mode.md` 17.2: the game sits at a frame top, so a pause anywhere inside that
frame's remaining work needs the *next* frame top to reach it, and two rather than one because from a
mid frame pause the first advanced frame may not reach a top. The loop breaks the moment the game clears
the mailbox flag, so one is the common case and two is the ceiling.

In game time that is 16.7 to 33.3 ms. **In wall time it is roughly 3 to 6 ms**, since emulation runs at
about 2.764 ms per frame (the pacing measurements). His instinct was right; the real figure is a couple of
frames rather than a millisecond.

## Why it is safe, and why it would not have been this morning

Advancing frames outside the player's own paced loop moves the machine clock relative to the audio ring.
That divergence used to be silent. Since the S3 landing the window's own gestures record what they moved
and `drain` fires every repair on the union, audio resync included, and a spawn trips `timeline_moved()`
because it advances frames. **So this is safe for a named reason and not by luck.**

## The hazard, and it is the resume rather than the hitch

A blind resume after the spawn starts a machine somebody deliberately stopped. Two real cases: the owner
pauses to line up a placement and clicks, and the window resumes under him; or an attached client paused
the machine to read it, and the window resumes under the client mid read. **The second is worse because
nothing announces it and it breaks another actor's invariant rather than a person's expectation.**

**So: capture the prior run state, restore it. Resume only if the machine was running when the click
arrived.** Built that way, a client paused machine needs no pause and no resume at all; the spawn happens
and nothing else moves, which also answers what a mid read client sees, namely nothing.

## What the contract allows, checked rather than assumed

The run control state rule reads: the named methods, *"called while it is free-running they MUST fail with
`-32005` ... never pause implicitly"*. **It binds what a method does when called.** A window that pauses
explicitly, calls the method on a machine that genuinely is paused, then restores what it found, satisfies
it literally: no method paused implicitly, and the machine really was paused for the spawn.

## Required by the ruling: the window says it did this

**The standing indicator shows when the window paused for him and restored**, so a resume he did not
perform is never a mystery. This is the same principle the mask statement already carries, and the same one
the lens episode taught: a thing that changes what you are looking at, without saying so, is read as the
real state.

## Not a race, though it reads like one

During those one or two frames his held pad inputs continue to be merged and applied, since the hold and
pad merge is unchanged. That is his input applying to frames that were going to run anyway, not the window
racing him. Worth stating because *"the window ran frames while I was holding right"* sounds alarming and
is not.

## Why it waits for the style pass

It is a new panel. Building it before the look rules are applied means building it twice, and the refusal
surfacing and the standing indicator are exactly the parts that would be rebuilt. The owner accepted this
sequencing in his own words: *"that's fine waiting for after the look rules exist."*
