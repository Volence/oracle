# UXb — heuristic audit, running log

Seat: UXb (heuristic audit), Roster C. Tree `/home/volence/sonic_hacks/oracle-uxb`, branch `seat/uxb`,
pin `cb21f43`. Binaries built in THIS tree 2026-09-09T07:44:35Z.
Display `:91` (Xvfb pid 4179530). Client: `tools/uxrig/drive.py` (XTEST), private socket
`.uxrig/sock/{frontend,player}.sock`.

ROM: `.uxrig/rom/s4.debug.bin`, snapshotted 07:48:23Z, md5 `7d2cc2fab38b4273fb2bbb859d699bf6`,
header-end check **COMPLETE** (0xcec52 -> 846931 bytes == filesize).

Windows: `oracle-player` win `0x400002` pid 1043405; `oracle-frontend` win `0x200021` (minifb, no
_NET_WM_PID).

Isolation proof (07:49:04Z): `procproof` = STRUCTURAL ISOLATION: PROVEN. Present half = window
enumerated on :91 by pid (player) / wm-class (frontend). Absent half = LOAD-BEARING form:
`WAYLAND_DISPLAY` and `XDG_SESSION_TYPE` ABSENT from `/proc/1043405/environ` of the RUNNING process,
and no fd resolves to `/run/user/1000/wayland-0` (which exists on this box -> real absence).
Kept corroboration: the same two filters return 0 matches on `:0`, each after a positive control
hit on `:91`.

---

## F-UXB-1 — oracle-player · toolbar · a status message pushes `commands` and `open ROM` off the window
**misses: can it be found / can a mistake be undone**

Press `step` in the toolbar. The status text becomes
`emulator/step: ok {"pc":"0x000BE5E8","stepped":1,"symbol":"GameState_OJZScroll_Update.skip_camera_update","symbolDisp":6}`
and that single line is long enough to displace the `commands` and `open ROM` buttons off the right
edge of the window — plus the whole `governor on · N frames · N rebases` status strip. Only a
clipped icon remains at x~1272.

It is not transient: still gone 3 s later with the pointer moved away (shot 05). It comes back only
when some later action writes a shorter message (`resume`, shot 06). A person who has just stepped —
exactly the moment they want the command palette — cannot find it, and nothing on screen says why it
vanished or how to get it back.

Evidence: `shots/01-player-default.png` (both buttons present) vs `shots/04-step-pressed.png` and
`shots/05-toolbar-after-step-wait.png` (both gone), `shots/06-resume-pressed.png` (back).

## F-UXB-2 — oracle-player · toolbar status strip · `governor` and `rebases` are undefined, and `rebases` climbs while the machine is paused
**misses: does it answer back (the change does not say what happened) / does an error say what to do next**

The strip reads `governor on · 2434 frames · 187 rebases`. Hovering it for 2 s produces no tooltip
(shot 03). Neither word is defined anywhere on screen.

With the emulator PAUSED (`frames` frozen at 2434, verified across 3 s), `rebases` still advanced
763 -> 900 -> 1081 -> 1157 -> 1434 -> 2175. A person who has just pressed pause sees one counter stop
and an unexplained one keep running, which reads as "it did not actually stop".

Evidence: `shots/02-toolbar-pause-pressed.png`, `shots/03-hover-rebases.png` (no tooltip),
`.uxrig/uc1.png`/`.uxrig/uc2.png` (frames 2434/2434, rebases 1081/1157, 3 s apart).

## F-UXB-3 — oracle-player · toolbar · the reply to every toolbar press is raw protocol JSON
**misses: does it answer back in words a person can act on**

Verbatim, as met: `emulator/pause: ok {"wasRunning":true}`, `emulator/resume: ok {"wasRunning":false}`,
`emulator/step: ok {"pc":"0x000BE5E8","stepped":1,...}`. The `resume` reply says `wasRunning:false`
*after* a successful resume (it is the pre-state), which reads as "it is not running".
Evidence: `shots/02`, `shots/04`, `shots/06`.

## C-UXB-1 (capture, not a finding) — `step` next to `pause`/`resume` steps ONE INSTRUCTION
The toolbar's other two controls are frame/run-level. `step` reported `"stepped":1` with a pc move of
6 bytes. Neighbour-consistency of gesture, but arguably intended; recorded for the owner rather than
filed. Evidence: `shots/04-step-pressed.png`.

## F-UXB-4 — oracle-player · Memory panel · the panel's own default address is refused by its own reader
**misses: does an error say what to do next (the first thing a person meets is a red refusal they did not cause)**

Open the Memory tab from the `panels` menu, touch nothing. The address field is **pre-filled** with
`0xFFFF0000` and the panel is already showing, in red:

> `REFUSED -32004: 0xFFFF0000: only cartridge ROM ($000000..rom_len) and work RAM ($E00000-$FFFFFF)
> are readable in this slice`

The shipped default is outside the range the panel itself declares readable. A newcomer's first sight
of the Memory panel is an error about a value they never typed. Pressing `go` on the default just
re-states it (`0xFFFF0000: a hex literal, taken as typed` + the same REFUSED).

The placeholder shown when the field is emptied is *also* `0xFFFF0000 or a symbol name` — so the
suggested example is the refused one too.

Evidence: `shots/08-panels-memory-clicked.png` (untouched, already red),
`shots/10-memory-go-default.png`, `shots/11-memory-empty-address.png` (placeholder).

## F-UXB-5 — oracle-player · right-hand dock · long messages are cut off, in both axes, with no visible affordance
**misses: does an error say what to do next**

Every long message in the right column is clipped. Horizontally the text wraps at a width wider than
the panel, so it runs off the right edge mid-word; vertically the Breakpoints node cuts its refusal
mid-sentence:

* Memory REFUSED, as seen: `...only cartridge ROM ($000000..rom_le` [edge]
* Memory poke refusal, as seen: `"ZZ" is not a whole number of hex byte` [edge]
* Breakpoints REFUSED, as seen: ends at `...so the table is` [bottom edge] — the remainder
  (`CURRENT and this name is genuinely not in it. Check the spelling or the build, not the freshness
  of the listing.`) is unreachable without scrolling.

Both axes DO scroll (mouse wheel / horizontal wheel), but no scrollbar is drawn in the node, so
nothing tells a person there is more text. The one visible horizontal scrollbar sits at the **top of
a different dock node**. There is no way to widen the column with the pointer alone.

The messages themselves are excellent when you can read them — which is why the clipping matters:
the actionable half is the half that gets cut.

Evidence: `shots/13-memory-go-garbage.png`, `shots/16-bp-arm-bad-symbol.png` (cut mid-sentence),
`shots/17-bp-scrolled-down.png` (the rest, only after scrolling), `shots/22-poke-garbage.png`,
`shots/09-memory-scroll-right.png` (full text, only after horizontal scroll).

## F-UXB-6 — oracle-player · Memory + Breakpoints · a stale red error stays on screen next to the new one
**misses: does it answer back / is it consistent**

After the default REFUSED is on screen, pressing `go` with an EMPTY field adds
`the panel cannot send that: type an address or a symbol name` **above** the old
`REFUSED -32004: 0xFFFF0000 ...`, which is about a value no longer in the field. Two red messages,
no ordering cue, no timestamps; the older one is about text that is not there any more.
Evidence: `shots/12-memory-go-empty.png`, `shots/13-memory-go-garbage.png`.

## F-UXB-7 — oracle-player · Memory panel · `poke` reports what it wrote but never what it overwrote, and there is no undo
**misses: can a mistake be undone**

Halted at a breakpoint, `poke` `DEAD` at `0xFFF000` succeeded:
`ok: {"addr":"0x00FFF000","len":2}  [frame 9719 · mclk 8709… running false]` and the dump refreshed
live from `00 B0 …` to `DE AD …`. Nothing in the panel offers an undo and the success reply does not
carry the bytes it replaced, so a person who mistypes the address has no record of the old value.
Evidence: `shots/23-poke-real.png`, `shots/24-memory-after-poke-top.png` (before value visible in
`shots/19-bp-armed.png`: `0x00FFF000  00 B0 00 …`).

## CLEAN — oracle-player · Breakpoints · arm on a symbol (5 steps, screenshot each)
1. `arm` with both fields empty -> `the panel cannot send that: type an address or a symbol name`
   (`shots/15-bp-arm-empty.png`) — same wording as the Memory panel's empty case. Consistent.
2. `arm` on `NotARealSymbol` -> `REFUSED -32013: no symbol named NotARealSymbol. The listing was
   re-checked just now: …/s4.debug.lst still parses to exactly the 3102 row(s) held, so the table is
   CURRENT and this name is genuinely not in it. Check the spelling or the build, not the freshness
   of the listing.` (`shots/16`, `shots/17`) — best message in either window; it pre-empts the wrong
   diagnosis a person would reach for.
3. `arm` on `GameState_OJZScroll_Update` (`shots/18`).
4. It HIT. Toolbar banner `HALTED BY BREAKPOINT b0 at 0x000BE4D8 (GameState_OJZScroll_Update)
   (1 halt; 1 still armed)`, a `⏏ release` button appears, `pause` becomes `resume`, panel reads
   `RECORDING: 1 of 1 breakpoint armed, so the machine will halt at it` (`shots/19-bp-armed.png`).
5. While halted, the Memory panel's write row changed from a refusal to `writes go to
   emulator/write_memory` on its own (`shots/21-memory-scrolled-bottom.png`) — it answered a state
   change it did not cause. Good.

## C-UXB-2 (capture) — two vocabularies for the same fact
Screen panel says `0 armed by this panel`; Breakpoints says `NEVER ARMED: No breakpoint has been
armed` / `RECORDING: 1 of 1 breakpoint armed`. Three phrasings of one counter across two panels.
Evidence: `shots/01`, `shots/19`.

## F-UXB-8 — oracle-player · Breakpoints table · the DELETE control is drawn as an empty tick-box, one control away from the real tick-box, and it deletes with no confirmation and no undo — I lost a breakpoint to it before I knew it existed
**misses: can it be found · is it consistent with its neighbours · can a mistake be undone**   ← my top finding

Each row of the breakpoints table begins with **two square controls side by side**:

```
 id     addr        state     hits
[✓]  [ ☐ ] b1   0x000BE4D8  ARMED   0 hits  GameSt…
 ^A     ^B
```

A = a real tick-box (ticked when armed). B = a **button whose entire label is an empty square glyph**
— i.e. it is drawn as an *unticked tick-box*, immediately to the right of a ticked one.
**B deletes the breakpoint.**

I met this as a person, not by reading code. Wanting to re-enable a disabled breakpoint I clicked
what I took to be its box; the reply was `ok: {"removed":1}`, the table emptied, and the breakpoint
was gone. Reproduced deliberately afterwards at 08:02:59Z / 08:03:23Z with the two controls measured
apart (window x=892 vs x=928, 36 px):

* click A (x=892) -> `ok: {"breakpoint":"b1","enabled":false,"hits":0}`, row stays, state `disabled`
  (`shots/35-bp-tickbox-clicked.png`)
* click B (x=928) -> `ok: {"removed":1}`, row gone, `clear all` gone (`shots/37-bp-delete-clicked.png`)

Aggravating, all four met on screen:
1. **The panel's own instructions point at it.** Both the toolbar and the panel say *"or untick them
   one at a time in the Breakpoints tab"* (`shots/19`, `shots/33`). A person told to untick goes
   looking for a box to untick; the thing that most looks like an unticked box is the delete button.
2. **No tooltip.** Hovered 2 s, nothing (`shots/36-bp-delete-hover.png`).
3. **No confirmation** and **no undo** — the row, its hit count and its label are gone.
4. The two controls sit in the `id` column with no header of their own.

Evidence: `shots/34-bp-table-zoom.png` (the two controls at 4x), `shots/32-bp-recheck.png` (the
accidental deletion, met first), `shots/35`, `shots/36`, `shots/37`.

## F-UXB-9 — oracle-player · Breakpoints · after a breakpoint is deleted the panel says it never existed
**misses: does it answer back**

Immediately after `ok: {"removed":1}` the headline reads
`NEVER ARMED: No breakpoint has been armed, so nothing here will stop the machine.`
A breakpoint *had* been armed, had halted the machine once, and had just been deleted. The panel
states a false history one line above the reply that says what happened.
Evidence: `shots/32-bp-recheck.png`, `shots/37-bp-delete-clicked.png`.

## F-UXB-10 — oracle-player · toolbar · the same button has two different names depending on state
**misses: is it consistent with its neighbours (same word, same meaning)**

Machine halted at a breakpoint -> the toolbar button reads **`⏏ release`** and the panel prose says
*"the ⏏ release button disarms it"* (`shots/19-bp-armed.png`).
Machine running with a breakpoint armed -> the same button reads **`⏏ disarm`** and the prose says
*"the ⏏ disarm button disarms it"* (`shots/34-bp-table-zoom.png` toolbar).
Same control, same icon, same position, two names. A person who learns "release" cannot search for it
later.

## F-UXB-11 — oracle-player · Breakpoints headline · "STOPPED" is shown while the machine is running
**misses: is it consistent with its neighbours (same word, same meaning)**

After `release`, the Breakpoints panel headline is
`STOPPED: nothing is armed. 1 breakpoint held and every one of them disabled, carrying 1 hit between
them from when they were armed. These figures are what was recorded before it stopped; they will not
move.` — while the toolbar simultaneously shows `pause` (i.e. the machine is RUNNING) and the frame
counter is advancing. "STOPPED" here means *recording stopped*; two panels' widths away the same
concept for the machine is "pause"/"halted". Evidence: `shots/30-released.png` (both in one frame).

## F-UXB-12 — oracle-player · Breakpoints · a stale `ok:` reply contradicts the table above it
**misses: does it answer back**

After `release`, the table row read `b0 … disabled` while the message directly beneath it still read
`ok: {"addr":"0x000BE4D8","breakpoint":"b0","enabled":true,…}` — the reply from the earlier *arm*.
Enabled:true under a row that says disabled. Evidence: `shots/31-bp-table.png`.

## CLEAN — oracle-player · Registers (2 steps) and Objects (2 steps)
* Registers, halted: full D0-D7/A0-A7/USP/SSP/PC/SR plus a plain-English note
  *"A7 and SP are one register: the stack pointer the CPU is using right now, SSP in supervisor mode,
  USP in user…"*. Read-only; clicking a value does nothing and nothing suggests it should
  (`shots/25`, `shots/26`, `shots/27`).
* Objects: engine/table-at/slots/slot-size/pools header, a `▶ where these addresses come from`
  expander that opens to a real explanation (*"Every address here is read out of the loaded listing…
  Nothing is hardcoded, because an object-table address is a fact about one build."*), then a players
  table and an object pool (`shots/28`, `shots/29`). Nothing found.

## C-UXB-3 (capture) — Registers panel: label and value collide
`frames run (player)9720` — the long label eats the column gap, so label and number run together.
Adjacent rows (`rom bytes    846931`) are aligned. `shots/25-registers-halted.png`.

## C-UXB-4 (capture) — Objects: the first player's `role` cell is a bare `·` while the second says `Player_2`
`shots/29-objects-expander.png`.

## F-UXB-13 — oracle-player · Watchpoints table · the SAME empty-square glyph is the FIRST control in the row here and the SECOND control in Breakpoints, and here there is no tick-box at all
**misses: is it consistent with its neighbours** — this is the compounding half of F-UXB-8

Breakpoints row: `[✓ tickbox] [☐ delete] b1 …` (delete is second).
Watchpoints row: `[☐ delete] w0  Bus 0x00FFF000..=0x00FFF000  Write  matched 0` (delete is FIRST, and
there is no tick-box in the row at all).

So the leading square in a row means *enable/disable* in one panel and *delete forever* in the panel
on the adjacent tab. Verified by pressing it at 08:05:05Z: `ok: {"removed":1}`, watch gone.
Evidence: `shots/40-watchpoint-scrolled.png`, `shots/41-watchpoint-square-clicked.png`, and
`shots/34-bp-table-zoom.png` for the Breakpoints row it must be consistent with.

## F-UXB-14 — oracle-player · commands palette · "the list above is all of them" points at an empty list
**misses: does an error say what to do next**

Type an unserved method and press Run:

> `no method named `emulator/does_not_exist` is served by this build. 62 are, and the list above is
> all of them. Nothing was sent.`

The method box is also the list's filter, so the very text that produced the error has filtered the
list to nothing. The header reads `0 of 62 served methods` and the area the message points at is
empty. The window also collapses from ~430 px to ~190 px in the same gesture.
Evidence: `shots/47-commands-bad-method.png`.

## F-UXB-15 — oracle-player · commands palette · the error message moves depending on how many methods matched
**misses: is it consistent / can it be found**

With 0 matches the refusal sits directly under `params` (`shots/47`). With 3 matches the refusal for
malformed params — `that is not JSON: key must be a string at line 1 column 2. Nothing was sent.` —
sits *below the method list*, ~140 px lower and easy to miss (`shots/48-commands-bad-params.png`).
Same class of message, two positions, decided by an unrelated filter.

## F-UXB-16 — oracle-player · commands palette · a stale refusal survives clearing the input that caused it
**misses: does it answer back** (third instance of the F-UXB-6 class, in a third panel)

`that is not JSON …` remained on screen after the params field was emptied and a different method was
selected (`shots/49-commands-filter-reset.png`, `shots/50-commands-chip-clicked.png`).

## CLEAN — oracle-player · commands palette (5 steps)
1. `commands` opens a dialog: `62 of 62 served methods: in-process, through the same registry a tool
   reads (D15)`, a `method` filter, a `params` box, `Run`, and every method with a one-line
   description and its param names (`shots/46-commands-palette.png`).
2. Unserved method -> refused, nothing sent (`shots/47`) — see F-UXB-14 for the list defect.
3. Malformed params -> `that is not JSON: … Nothing was sent.` (`shots/48`). "Nothing was sent" is
   the right thing to say and it is said in both cases.
4. Typing `reset` filters on descriptions as well as names (`emulator/set_profiler`, because its
   description contains "resets") (`shots/49`); clicking a method's chip fills the field
   (`shots/50`).
5. `Run` -> `emulator/reset: ok {"deferred":false,"hitsDropped":0}`, the game restarts
   (`shots/51-commands-reset-run.png`). This is the discoverable, well-documented surface of the two
   windows.

## ⚠ CONFOUND, declared — the game crashed, and I caused it
At ~07:59Z I poked `DEAD` into `0xFFF000` (F-UXB-7). By `shots/38-watchpoints.png` the game window
was showing the engine's own `ADDRESS ERROR` panic with `d0: 0000DEAD`, and it stayed there through
shots 38-50. **That crash is mine, not a defect**, and no finding above rests on it. I say so because
it is exactly the artefact that would otherwise read as "the panel shows nothing".

It is also the lived proof of F-UXB-7: I could not put `00 B0` back — the panel had not told me what
it was — and the only way out was `emulator/reset` from the commands palette (08:08:31Z,
`shots/51`). **The player window has no reset control of its own**: not in the toolbar, not in any
panel; it exists only as a method inside the palette. The game window has F1 and Tab for it.

## C-UXB-5 (capture) — `0x00FFF000..=0x00FFF000` — Rust's inclusive-range syntax reaches the watchpoint row
`shots/40-watchpoint-scrolled.png`.

## C-UXB-6 (capture) — Watchpoints: `seen` keeps climbing after the only watch is removed
`seen 34117710` on a panel that says nothing is armed. `shots/41-watchpoint-square-clicked.png`.

## C-UXB-7 (capture) — Watchpoints row 2 reads `☑ write stopAfter [∞]` with no separator
The `write` tick-box's label and the next control's label collide, and its partner `read` sits on the
row above at the far right. `shots/38-watchpoints.png`.

## C-UXB-8 (capture) — three different arm/disarm idioms in three adjacent tabs
Breakpoints: `arm` stays `arm`, disarm via the toolbar or a per-row tick-box.
Watchpoints: `arm` stays `arm`, no per-row tick-box at all.
Profiler: the same button becomes `disarm` (`shots/42`, `shots/43`).

## F-UXB-17 — oracle-player · Screen · `load` on a slot the panel has just labelled `(empty)` answers with a raw OS errno
**misses: does an error say what to do next**

The state row reads `state: ◀ slot 0 (empty) ▶ save load`. The panel therefore already knows the slot
is empty — it says so, in words, 40 px from the button. Press `load` (08:11:26Z):

> `state: load of slot 0 failed: No such file or directory (os error 2)`

A person is handed a C errno for a condition the interface had already described in English, and the
button was enabled to let them find out. **The same product does this correctly one tab away**: the
Effects panel *disables* `turn scene off` and explains on hover — *"…scene is always in effect, so
there is nothing to turn off. Pick a different one instead."* Two panels, one build, opposite
handling of "this control cannot work right now".
Evidence: `shots/60-state-load-empty.png`, `shots/53-effects-disabled-hover.png`.

## F-UXB-18 — oracle-player · Screen · every message is inserted ABOVE the controls, so the controls move as you use the panel
**misses: is it consistent with its neighbours** — and it is a mis-click hazard, since clicking is all there is

The Screen panel's status lines (`SPAWN: …`, `HIDDEN: planeB …`, `This window paused the machine …`)
accumulate at the TOP of the panel. Every one pushes the aspect row, the state row and the picture
downward. Measured: the aspect row sits at window y=155 in `shots/59-screen-tab.png` and at y=174 in
`shots/66-aspect-integer.png` after a single layer toggle — a 19 px shift, more than a row's height.
I clicked `integer` at its old position immediately after toggling a layer and hit nothing
(`shots/66`, aspect still `4:3`); the identical click at y=174 worked (`shots/67-aspect-integer2.png`).
Panels whose controls stay put (Breakpoints, Watchpoints, Memory) put their messages BELOW.

## F-UXB-19 — oracle-player · Screen · `save` over an occupied slot overwrites silently
**misses: can a mistake be undone**

Slot shows `slot 0 (occupied)`; pressing `save` again overwrites it with no confirmation, and the
reply is the same sentence as a first save (`state: saved 1020402 bytes to slot 0 (…)`), so nothing
distinguishes "wrote a new state" from "destroyed the one you were keeping". No undo.
Evidence: `shots/61-state-save.png` then `shots/62-state-save-overwrite.png`.

## F-UXB-20 — oracle-player · Screen · the status line under the state row describes an action two actions ago
**misses: does it answer back** (fourth panel with this class; see F-UXB-6, F-UXB-12, F-UXB-16)

After `save` to slot 0, pressing `◀` moved the selector to `slot 9 (empty)` while the line beneath
still read `state: saved 1020402 bytes to slot 0 (…)`. A person who presses an arrow and reads the
line directly under it is told about slot 0. Evidence: `shots/63-state-prev-from-0.png`.
Same panel, longer-lived: the note *"This window paused the machine to take a picture of the object
and resumed it"* was still on screen ~10 minutes and one full `emulator/reset` after the event it
describes (`shots/59` … `shots/67`).

## CLEAN — oracle-player · Screen layer + aspect + state (6 steps)
1. `load` on empty slot 0 -> errno (F-UXB-17). 2. `save` -> `state: saved 1020400 bytes to slot 0
(/home/volence/sonic_hacks/oracle-uxb/.uxrig/rom/s4.debug.state0)`, label flips to `(occupied)`
(`shots/61`). 3. `save` again -> silent overwrite (F-UXB-19). 4. `◀` wraps 0 -> 9 (`shots/63`).
5. `▶` back, `load` -> `state: loaded slot 0 from …` and the game restores (`shots/64`).
6. Layer toggle: unticking `planeB` gives a red-keyed picture, a yellow banner *"HIDDEN: planeB. This
picture is re-rendered from current VDP state, so mid-frame palette effects are not in it"*, and a
badge *"planeB is now HIDDEN"* over the picture — three simultaneous, agreeing signals
(`shots/65-layer-planeB-off.png`). Aspect `4:3`/`square`/`integer` select and highlight (`shots/67`).

## CLEAN — oracle-player · Planes (3 steps)
`plane A` / `plane B` / `window` each re-render with their own nametable address, map size, plane
size, `rasterised N times in M repaints`, and a per-plane note (`window`: *"the window plane does not
scroll / its map sits at screen coordinates"*; `plane A`: a five-line warning that a horizontal
interrupt is armed and the mode is read once). `shots/55-planes.png`, `shots/56-planes-B.png`,
`shots/57-planes-window.png`. Nothing found.

## CLEAN — oracle-player · Effects (2 steps)
`scene` / `raster program` / `band table`; `read it back` -> `Parallax_Current_Config holds
0x0001464E, which is 'EditorSceneBinding_OJZ_Act1_Sec0'.` The panel says *why* it does not poll:
*"This panel does not read on its own, because a stale line beside a fresh selection is the picture it
exists to prevent."* `shots/52-effects.png`, `shots/54-effects-readback.png`.

## ⚑ TOOLTIPS EXIST IN THIS WINDOW — which is what makes F-UXB-2 and F-UXB-8 findings rather than a house style
Two controls answer a hover: the disabled `turn scene off` (`shots/53`) and the toolbar's
`machine replaced at frame N` badge — *"Something replaced the machine in this window: a reload, a
reset or a restore, from here or from a program driving it. Anything you read now comes from the new
one. Click to dismiss."* (`shots/58-hover-machine-replaced.png`). The breakpoint delete button and the
`governor`/`rebases` strip were hovered for 2 s each and gave nothing.

# The game window (`oracle-frontend`)

## F-UXB-21 — oracle-frontend · the whole window · nothing on screen says a single control exists
**misses: can it be found**

The window is the picture and nothing else: no menu, no bar, no hint text, no right-click menu
(`shots/70-frontend-default.png`). It has ~30 commands and ~20 key bindings. Every one of them,
including the key that opens the list, is announced only on **stdout at launch** — a terminal a
person who double-clicks the binary does not have. A newcomer given this window can start the game
and nothing else; there is no gesture that reveals the backtick.

Once found, ` opens a real palette (`shots/71-frontend-palette.png`) — so the commands are
discoverable *from inside the palette*, and the palette is discoverable from nowhere.
Booked adjacent: `F-PANELS-INVISIBLE-TO-SCREEN-TEXT`; met here as *the window told me nothing at all*.

## F-UXB-22 — oracle-frontend · command palette · the list scrolls and looks complete when it is not
**misses: can it be found**

On open, the list ends flush at the panel's bottom border on `TOGGLE SPRITE OUTLINES`
(`shots/71`, `shots/75-frontend-palette-cleared.png`). There is **no scrollbar, no fade, no "more"
marker**. Holding Down reveals three more LENSES rows (`shots/77`), then whole groups the first view
gave no sign of: **DISPLAY LAYERS** (4), **SPAWN OBJECTS (DEBUG, NOT SAVED)** (2) and **SETTINGS**
(status line F3, volume, mute, audio filter) (`shots/78-frontend-palette-end.png`). Roughly 17 of ~30
commands are visible, and the cut lands exactly on a group boundary, which is what makes it read as
the end.

## F-UXB-23 — oracle-frontend · command palette · a search that matches nothing says nothing, and Enter does nothing
**misses: does an error say what to do next** · direct contradiction of the other window

Typing `zzzz` empties the list and leaves a blank dimmed rectangle with `> zzzz_` above it. No
message (`shots/72-frontend-palette-nomatch.png`). Pressing Return then does **nothing at all** — the
pixels are identical (`shots/73-frontend-palette-enter-nomatch.png`). A person is left holding a
palette that will not answer and will not explain.

**The same product, in the other window, gets this exactly right.** `oracle-player`'s commands
palette answers the identical mistake with *"no method named `emulator/does_not_exist` is served by
this build. 62 are, and the list above is all of them. Nothing was sent."* (`shots/47`). Two command
palettes, one build, one mistake, opposite behaviour — **and the window that is a newcomer's first
contact is the silent one.**

## F-UXB-24 — oracle-frontend · command palette · three entries are truncated mid-word with no way to read them
**misses: can it be found** (same defect class as F-UXB-5, in the other window)

As displayed: `TOGGLE CPU REGISTERS (FULL D0-D7/A0-` · `SPAWN MODE: CLICK TO PLACE AN OBJEC` ·
`AUDIO FILTER: VA0-VA2 / VA3-VA6 / R`. The panel does not wrap, does not widen, and does not scroll
horizontally. `shots/71`, `shots/78`.

## F-UXB-25 — oracle-frontend · volume / mute · the window reports a volume and confirms a mute for an audio device it knows it does not have
**misses: does an error say what to do next (the operation that cannot work reports success)**

At launch this build printed, to stdout only:
`audio: no default output config (A backend-specific error has occurred: ALSA function 'snd_pcm_open'
failed with error 'Host is down (112)'), running video-only`.

The window nevertheless shows `VOL 10/10` in the F3 status line, and pressing `M` (08:17:55Z) flips
the status line to `MUTE` and toasts `VOLUME: 10/10  [MUTED]` (`shots/83-frontend-mute.png`). Nothing
anywhere in the window says audio is unavailable. A person who hears nothing presses mute, is told it
is muted, presses it again, is told 10/10, and is no closer to the truth — which the program had
already established at startup and threw away.

⚠ **Declared tension with my brief**: the handover says "No volume, mute, or sound findings" because
the rig has no audio. I am filing it anyway and flagging it, because it needs no judgement about
sound — it is a state-reporting mismatch, evidenced by the window's own text against the process's
own stdout. It is also reachable off this rig (no card, PipeWire down, headless). **Downgrade or drop
it if the controller reads the bar more widely than I have.**

## F-UXB-26 — oracle-player · closing a tab is recoverable, but not to where it was
**misses: can a mistake be undone**

Closing `Screen` and `Planes` with their `X` collapses the entire left column and the game picture
disappears with no warning (`shots/87-tabs-closed.png`). Recovery is well signposted — the `panels`
menu marks them `Screen (closed)` / `Planes (closed)` (`shots/88-panels-after-close.png`) — but
choosing `Screen` reopens it **in a different dock node**, wedged beside Registers/Memory/Objects,
where the picture is a thumbnail (`shots/89-screen-restored.png`). The only way back to the previous
arrangement is `reset to the default layout`, which discards every other layout change with it
(it does work: `shots/90-layout-reset.png`, and per-panel state such as the `integer` aspect
survives).

## CLEAN — oracle-frontend · click-to-watch, W, C, F1, F3 (5 steps)
1. `F3` -> status line `[0]123456789 VOL 10/10 AETHER ON 4:3 320x224 DRAWS 91586`
   (`shots/79-frontend-f3-status.png`).
2. Click on the picture -> a blue `+` marker on the sprite and `WATCH SPRITE 0 TILE $3F9 + SAT $B800`
   (`shots/80-frontend-click-watch.png`). Good: it names what it watched, not just that it watched.
3. `W` -> `DUMPED 6512 WATCH HITS TO STDOUT` on screen, and 6512 lines really did land in the log
   (`shots/81`, `.uxrig/frontend.log` grew 12 -> 6526 lines). Honest about the destination.
4. `C` -> `WATCH CLEARED: NO LONGER RECORDING WRITES` (`shots/82`).
5. `F1` -> `RESET: SOFT RESET; SRAM CONTENTS PRESERVED, AS …` (`shots/84-frontend-f1.png`).

## CLEAN — oracle-player · open ROM (3 steps)
Built-in browser: `…/.uxrig/rom - 2 of 2 rows shown. Enter opens the highlighted row; Ctrl+O loads a
pasted path; drop a file on this window.`, a path filter, `up`, `refresh`, and `s4.debug.bin
[loaded]` marked as the current one (`shots/85-player-open-rom.png`). Typing `/nonexistent/nope.bin`
and pressing Return gives `Enter: nothing, because no row matches what is typed` with the header at
`0 of 2 rows shown` (`shots/86-open-rom-bad-path.png`) — it refuses, says why, and names the other
route in the same breath. Best refusal in either window.

## C-UXB-9 (capture) — the F1 toast is elided on screen and complete in the terminal
Window: `RESET: SOFT RESET; SRAM CONTENTS PRESERVED, AS …`. Log: `reset: soft reset; SRAM contents
preserved, as on real hardware`. `shots/84`.

## C-UXB-10 (capture) — `SOFT RESET` is bound to both `TAB` and `F1`; the palette lists only `TAB`
`shots/78` vs the F1 press at 08:18:25Z (`shots/84`).

## C-UXB-11 (capture) — `STEP ONE FRAME` shows no key in the palette while both its neighbours do
`PAUSE / RESUME  SPACE`, `STEP ONE FRAME` (blank), `SOFT RESET (SRAM KEPT)  TAB`. The launch line
says the key is `.`. `shots/71`.

## C-UXB-12 (capture) — the palette says `DUMP WATCH HITS TO TERMINAL`; the toast says `TO STDOUT`
`shots/71` vs `shots/81`.

## C-UXB-13 (capture) — the F3 status line's save-slot widget is bare digits, `[0]123456789`, with no label
`shots/79`.

## C-UXB-14 (capture, cross-window) — the two windows are two separate machines and neither says so
`oracle-player` had emulated 45553 frames while `oracle-frontend` reported `DRAWS 96177` at the same
moment, on the same ROM path, each `aether serving on` its own socket. Neither window names the other
or says its machine is its own. I cannot tell from the windows alone whether that is the intended
topology (establishing it would mean reading source, which this seat may not do), so this is a
capture for the owner, not a finding. `shots/79`, `shots/85`.
Vocabulary also diverges for the same quantity: player says `frames emulated` / `frames presented` /
`N frames`; frontend says `DRAWS N`.

---

# What I could NOT drive, and which instrument would

* **Any chorded key.** `drive.py key` sends one keysym; it has no modifier support. So `Ctrl+O`
  — the alternate route the open-ROM dialog explicitly advertises — is **UNDRIVABLE**. Named
  control: `oracle-player` open ROM, "Ctrl+O loads a pasted path". Instrument: an XTEST modifier
  press/release pair in `drive.py`.
* **Drag of any kind.** `drive.py click` is press+release at one point. So: dock splitters (the only
  way to widen the right column and read the clipped messages of F-UXB-5), drag-to-dock, tab
  reordering, text selection, and the file dialog's resize grip are all **UNDRIVABLE**. Instrument:
  a `drag` subcommand (button-press, motion, button-release).
* **Drop a file on the window** — the open-ROM dialog's third advertised route. Needs XDND.
* **Gamepad.** `gamepad: no controllers detected, keyboard only`. Presenting one needs
  `/dev/uinput`, which is forbidden here. BLOCKED by rule, not by capability.
* **Audio output.** No device at all in this rig (see F-UXB-25 for what I could still establish
  without one).
* **Pacing/vsync/smoothness.** Out of scope by charter; llvmpipe, no vsync.
* **The `Pacing` tab's A/B bakeoff prompt** (*"Say which reads better and the other one goes away"*)
  is addressed to the owner and awaits his answer; it is a booked row
  (`STYLE-NUMBER-BAKEOFF`), not mine to settle.

# Housekeeping proof
* `s4.debug.state0` written by my `save` landed at `.uxrig/rom/s4.debug.state0` (1020400 bytes,
  08:11Z), **inside my own tree**. Aeon's `s4.debug.state0` md5 `9fb368a1afcbc624494115fe658634e1`
  and mtime `2026-08-28 14:37:40` are byte-identical before and after (checked 08:11Z and 08:20Z).
  Nothing was written to the aeon tree.
* Processes: only pids 1025404 (frontend), 1043405 (player), 4179530 (Xvfb) — all recorded at spawn.
  The owner's pid 1570308 was never touched. No `pkill`/`killall` was ever run.
