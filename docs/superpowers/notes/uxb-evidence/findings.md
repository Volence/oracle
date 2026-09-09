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
