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
