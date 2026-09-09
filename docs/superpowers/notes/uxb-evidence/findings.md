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
