# The two window checks, run — ROM-OPEN-RUNTIME and BP-WINDOW-CONFIRM

**Date:** 2026-08-29 · **By:** oracle overseer, foreground (this repo's rule: runtime checks are never
a subagent) · **Binaries:** rebuilt at `6bec033` before anything was believed

## 0. Why the overseer ran this rather than dispatching

Two reasons, both standing. **(a)** This repo tags every runtime pass ⟨RUNTIME⟩ *"foreground follow-up,
never a subagent"* — the emulator MCP deadlocks from background agents. **(b)** This session's
configuration forbids calling the Agent tool unless the owner asks. So the orchestrate-don't-implement
norm yields, exactly as it did for `docs/2026-08-28-rom-open.md` §0 and `docs/2026-08-27-obj-join-recon.md`
§0. Recorded because a later reader will otherwise see the seat driving a player and read it as drift.

**Authorisation:** relayed from the owner via aurora, verbatim — *"Oracle can start that even if it's out
of project as long as oracle has nothing left to do for our current goal of the parallax/effects system."*
⚑ **RELAYED, NOT WITNESSED BY THIS LANE**, flagged per this repo's standing rule; replace this flag with
his confirmation, do not delete it. The condition attached to it was **verified here rather than accepted**
— see §1.

## 1. The condition on the go, checked two ways

The relay's condition was *no open item serving EFFECTS-W1*. The hub asserted there were none **on our
board**, which is a claim about our tree and therefore the class this repo requires be checked rather than
taken (*a peer's warning about your own tree is the one that feels least like it needs checking*).

Two derivations with **different enumeration parameters** (bar 19), so this is corroboration and not echo:

| derivation | parameter enumerated | result |
|---|---|---|
| hub's declaration outward | EFFECTS-W1's own `lanes` list, at empyrean `origin/main` | `["aeon","aurora","sigil"]` — **oracle is not a lane on it at all** |
| our board inward | `project` tag over every row of `docs/lane-status.json` | **0 rows** tagged EFFECTS-W1 |

EFFECTS-W2 was checked too and also excludes oracle. The stronger of the two is the first: it is not that
we have no open items, it is that the project does not route here.

**Named against our own interest, since the relay invited it:** the nearest thing on our board to an
effects-serving item is **OVERLAY-STATE** (let a tool read the player's on-screen text), which would serve
aeon, who *is* on EFFECTS-W1. It is untagged, is not `next`, and blocks nothing of theirs today — but it is
the row a reasonable person might have meant, so it is named here rather than silently excluded.

**CR-F anchor checked by class**, since it was cited in the same message: empyrean `b5b8184` is an ancestor
of `origin/main` and `--stat`s to `contract/protocol.md` +17, `contract/schema/bus-protocol.schema.json`
+278, `contract/schema/tests/vectors.json` +307. It carries what it anchors.

## 2. The rig — and a correction to the recipe this repo had banked

The banked recipe (`OVERSEER.md` Ops, from the SCREEN-HONESTY parcel) is *"the player under `xvfb-run`
with `XDG_CONFIG_HOME` pointed at a scratch `player.conf`"*.

⚠ **THAT RECIPE IS INCOMPLETE ON THIS MACHINE, AND FOLLOWING IT PUTS A WINDOW ON THE OWNER'S DESKTOP.**
Measured firsthand, because it happened here on the first launch. `minifb` prefers Wayland when
`WAYLAND_DISPLAY` is set, and **`WAYLAND_DISPLAY=wayland-0` is inherited by every lane session on this
box**. So `DISPLAY=:91` was set, honoured by nothing, and the log said `Wayland window` while
`python-xlib` found **zero windows on the Xvfb**. The window was on his real screen. Killed within the
minute by recorded PID.

**The corrected recipe, which is what was used for everything below:**

```sh
Xvfb :91 -screen 0 1280x960x24 -nolisten tcp &   # own it; record the PID
env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE \
    DISPLAY=:91 XDG_CONFIG_HOME=<scratch>/xdg \
    target/release/oracle-frontend --x11 --socket /tmp/orc-w/o.sock <rom>
```

Both `-u WAYLAND_DISPLAY` **and** `--x11` — belt and braces, because the failure is silent and lands on
somebody else's screen. **Verify placement before driving anything**: enumerate windows on the display you
think you own; an empty list is the finding.

*Why this is worth a section: the banked recipe was correct as written for its author's purpose (they were
reading a screenshot, and any window would do). It is the **isolation** claim that was never true, and
nothing in the note said which of the two it was promising.* Same class as the vintage-of-the-process bar —
a note that is true of the file and false of the situation.

Ops respected: short socket path (17 chars, well under `SUN_LEN`); killed by **recorded PID**, never by
pattern; `/usr/bin/ls` not the `eza` alias; lane-owned ROMs from `vendor/TestRoms`; `mcp__oracle__*` **not
used at all** (a direct 30-line unix-socket client instead), since the shim reaches the owner's player.
Afterwards `/run/user/1000/oracle.sock` and all ten `/tmp/oracle-mcp-*` sockets were confirmed still
listening — nothing shared disturbed.

## 3. Free corroboration of today's SCREEN-HONESTY parcel, on the glass

Not the point of the exercise; recorded because it cost nothing and the parcel's own claims were made from
one author's run. At 896×672 the F3 line read:

```
0123456789 VOL 10/10 AETHER ON AUDIO VA0-VA2 4:3 320X224 F1106
```

`AETHER ON` present; `AUDIO VA0-VA2` **labelled**; `320X224` **not** cut mid-number; and **`F1106` — the
frame counter is visible**, which that parcel says had never once been visible at any window size. Four
claims, independently reproduced.

## 4. ROM-OPEN-RUNTIME — the checklist from `2026-08-28-rom-open.md` §5

| # | what was owed | verdict |
|---|---|---|
| — | picker opens, orders `../` → dirs → images, `[loaded]` marked | ✅ exactly as specified |
| — | typed filter narrows the list | ✅ `fm` → `FM_TEST.BIN` alone |
| — | **Enter runs the VISIBLE row, not row 0 of the unfiltered list** | ✅ **refuted on live data** |
| a | `.srm` / save slots follow the new cartridge | ✅ F2 wrote `fm_test.state0` |
| b | `[loaded]` survives the round trip through `../` | ✅ and it moved to the new cartridge |
| c | an unreadable folder **leaves the previous listing up** | ❌ **does not hold — see §4.2** |

### 4.1 The named failure mode is refuted, and by a second parameter

The parcel's stated risk was *"an implementation that indexed `items` directly would run row 0 of the
**unfiltered** list — a different game, chosen silently, with the right row highlighted."* With the filter
`fm` applied and one row visible, Enter produced:

```
romPath : /tmp/orc-w/roms/fm_test.bin
romBytes: 1276
frame   : 104          (reset; had been F2645)
```

**`romBytes` is the second parameter and it is the one that matters**: 1276 is `fm_test.bin`'s exact size
on disk, against 16384 for both other images in that folder. The path string alone would have been the
server agreeing with itself; the byte count is the cartridge.

### 4.2 (c) FAILS AS DOCUMENTED — the picker closes instead of persisting

Entering the unreadable directory produced a red toast and **dismissed the picker entirely**, returning to
the game. The player stayed alive, the bus kept answering, and `romPath` was unchanged — so the behaviour
is **safe and loud**, but it is not what §5 promised, and the promise is that parcel's own acceptance
criterion.

**Mechanism, from source rather than inferred** (`main.rs:494-501`): `open_rom_picker` early-`return`s on
`scan` failure, *before* `palette.open_picker`. Enter had already closed the palette (`palette.rs:195-204`,
*"Enter on an Item runs it and closes"*). So the previous listing is not "left up" — it is destroyed by the
close and then never rebuilt. **Structural, not a fluke.**

Not a defect worth a parcel on its own; it is a **documentation correction owed to `2026-08-28-rom-open.md`
§5**, plus a cheap real fix if anyone is in that file (re-open the picker on the *previous* directory before
notifying).

### 4.3 ⚑ NEW FINDING — the filter matches the `[loaded]` DECORATION, not the filename

Found by accident and then pinned deliberately. Typing `o` retained `FM_TEST.BIN`, which contains no `o`.
The discriminator: **`loaded` appears in no filename in that folder**, and typing it left `FM_TEST.BIN`
alone on screen.

Mechanism, exact: `rom_browser.rs:115` builds the row as `format!("{}   [loaded]", entry.label)` — the
marker is baked **into the label** — and `Picker::visible()` (`palette.rs:58-62`) runs `subseq_match` over
that same composed string.

**Consequence:** because `subseq_match` is a *subsequence* matcher, every letter of `l,o,a,d,e` is free for
the loaded row, so it survives filters that should exclude it. The row that spuriously persists is the ROM
you already have open — i.e. reliably the one you are *not* trying to pick.

**Severity: low.** Cosmetic only — §4.1 proves Enter still runs the correct visible row, so nothing loads
the wrong game.

⚑ **The interesting half is why no test caught it, and it is this repo's own bar from this morning.** This
is model-level, not render-level, so a headless test *could* have caught it. `rom_browser.rs:251` already
asserts over these labels (`dirs.iter().all(|l| !l.contains("[loaded]"))`). Nothing asserts the
**interaction** between the marker feature and the filter feature. *A test that asserts what you added is
structurally blind to what you displaced* — and here it is one step out: **a seam between two features,
each correctly tested alone.** Same family as `F-IDENTITY-JOIN-UNASSERTED`, found the same day by a reader
who had written neither side.

### 4.4 The error toast truncates away its own reason

The toast rendered `OPEN ROM: CANNOT READ /TMP/ORC-W/ROMS/LOCKED (PE` — cut at the right edge. The string,
derived from source (`main.rs:498`) rather than guessed: `open ROM: cannot read {dir} ({e})` with `e` =
`Permission denied (os error 13)`, ≈76 characters.

So **the failure names the path and loses the reason**, which is the half a person needs. Today's
SCREEN-HONESTY parcel fixed exactly this on the *status line* (by dropping a font step) and toasts were not
in scope — so this is the same defect on the neighbouring surface, found the same day, by the mechanism
that bar predicts.

## 5. BP-WINDOW-CONFIRM — ✅ PASSED, and it closes the parcel's last open leg

`docs/2026-08-27-bp-hosted-halt.md` proved the standalone path firsthand and said plainly what it did not
prove: *"a windowed `oracle-frontend` end-to-end remains unrun, because the only windowed players on this
machine are the owner's and aurora's."* There is now a lane-owned one.

Armed over the bus against a freely-running window (PC sampled at `0x0000025A` on 30/30 samples — a tight
loop — while `running: true`, frame ~11975):

```
breakpoint_add  -> {breakpoint: "b0", addr: "0x0000025A", enabled: true, running: true, frame: 12719}
wait_for_break  -> {pc: "0x0000025A", running: false, timeoutReached: false, waitedMs: 14}
frame  12719 -> 12719   across 1.5 s of wall clock
mclk   11396732858 -> 11396732858   (identical to the byte)
```

**The frozen frame counter across real wall-clock time is the load-bearing evidence** — a window still
presenting would advance frames. That is the exact failure the parcel named (the two-flag bug *"would have
kept it advancing"*). `mclk` also moved 11396732788 → 11396732858 between arm and stop, so the stop carries
a **real stamp**, not the D11 placeholder zero.

### 5.1 The control, because `running:false` is also what a dead window looks like

`running: false` with a frozen frame is equally consistent with a **crashed** window, and nothing in the
halt evidence separates the two. So: cleared the breakpoint, resumed, and re-measured.

```
after resume  running: True -> True
after resume  frame:   12721 -> 12813    (+92 in 1.5 s ≈ 61 fps)
```

**Clean halt, clean resume, not a wedge.** Recorded because the halt reading alone would have been a green
that two very different states both produce.

### 5.2 And the window SAYS it stopped, which was not required but is the right behaviour

The halted window rendered a large `PAUSED` with `PC $00025A` — the armed address exactly — plus
`SR $2704 S7` and `F 12720`. So the stop is not silent: a person watching sees *that* it stopped and
*where*. That is aurora's *a lens must state that it is on* arriving on the breakpoint surface, and aeon's
ruling 4 (*either the answer is exact or the server says it isn't*) shown on glass rather than on the wire.

⚠ **One unexplained off-by-one, recorded as a lead and NOT claimed as a defect.** The overlay and status
line both read `F 12720` while the bus reported `frame: 12719` for the same stop. A "frames completed" vs
"frame being presented" convention difference would explain it entirely and would not be a bug. It is
registered rather than diagnosed because **a consumer joining the window to the bus would meet it**, and
nobody has looked. Revival condition: any parcel that correlates an on-screen frame number with a bus
`frame`/`frameToken`.

## 6. What this does NOT cover, stated plainly

* **One ROM, one folder, one window size.** No 224px window (where §4.4's truncation is worst), no deep
  directory tree, no non-`.bin` extensions, no symlinks.
* **No `.srm`-writing game was used**, so (a) is proven for **save-state slots** (`fm_test.state0` appeared
  beside the new cartridge) and only *inferred* for `.srm`, which the ROMs used here never write. The
  parcel's own §5 asked for both; half of it is measured and half is not, and the halves are named rather
  than merged.
* **The breakpoint check used one address in one tight loop.** No symbol-armed breakpoint, no breakpoint in
  a rarely-hit path, no two subscribers on one PC (the handle discipline CR-A exists for).
* Nothing here touched `aeon/s4.debug.bin` or any owner-facing instance.

## 7. Registered follow-ups

| id | what | revival condition |
|---|---|---|
| **F-PICKER-FILTER-MARKER** | The ROM picker's filter matches the `[loaded]` decoration (`rom_browser.rs:115` + `palette.rs:58-62`), so the already-open ROM survives filters that should exclude it. Cosmetic; Enter is unaffected. Fix: match on `entry.label`, render the marker separately. | Next time anyone opens `rom_browser.rs` — it is a two-line fix plus the seam test that was missing. |
| **F-ROMOPEN-C-DOC** | `2026-08-28-rom-open.md` §5 promises an unreadable folder "leaves the previous listing up"; it dismisses the picker instead. Behaviour is safe and loud. | Correct the doc at the next pass over it; optionally re-open on the previous directory before notifying. |
| **F-TOAST-TRUNCATES** | `notify_err` toasts are cut from the right with no ellipsis, losing the reason (`(PE` for `Permission denied`). Status line was fixed today by a font step; toasts were not in scope. | Any parcel touching `overlay.rs` toast rendering; assert on the **whole** rendered string per this repo's 2026-08-29 bar. |
| **F-WINDOW-BUS-FRAME-OFFBYONE** | Overlay/status line read `F 12720` where the bus read `frame: 12719` for the same stop. Probably a completed-vs-presenting convention; unverified. | Any parcel correlating an on-screen frame number with bus `frame`/`frameToken`. |
| **F-XVFB-RECIPE** (closed by §2) | The banked headless recipe omitted `WAYLAND_DISPLAY`, so following it puts a window on the owner's desktop. Corrected in `OVERSEER.md` Ops. | — closed, but keep the reason: the note was true of the file and false of the situation. |

## 8. Consequence for the queue

**Both gates are discharged.** The sequencing blocker recorded on `LIVE-OBJECTS-CARD` — *"building waits
behind our own breakpoint-window confirmation"* — is **cleared**. Select-and-inspect (CR-F, applied at
empyrean `b5b8184`) is startable on this lane's own order, per the owner's relayed go in §0.

`OBJECT-AT-CONFORMANCE` still rides with that build and is unchanged: the two live checks a schema
structurally cannot do must land **with** the feature.
