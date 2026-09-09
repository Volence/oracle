# UXa — task walk, running log

Seat: UXa (task walk), Roster C. Tree: `/home/volence/sonic_hacks/oracle-uxa`, branch `seat/uxa`,
built from `cb21f43`. Display: `:90` (Xvfb, `launch.sh display-start`, geometry read back from
inside: 1280x960 depth=24). Client: only `tools/uxrig/drive.py` over its own connection to `:90`,
plus the binaries in **this** worktree's `target/release/`. No MCP, no `/dev/uinput`.

Every time below is UTC wall-clock from `date -u`.

## Timeline

| time (UTC) | what |
|---|---|
| 07:39:20 | walk starts; charter + handover read |
| 07:39:39 | worktree created. **`s4.debug.bin` already absent** — first check of the run |
| 07:39:54 | `cargo build --release -p oracle-player -p oracle-frontend` started |
| 07:40:26 | display `:90` up |
| 07:44:43 | build finished, 4m49s, rc=0 |
| 07:45:0x | ROM still absent; three `./build.sh` processes live in the aeon tree |
| 07:45:1x | no-ROM probes run against both binaries while waiting |

## Job 1 — "get a game running and see it", from the README alone

### The README does not carry a newcomer to a loaded ROM. Two independent reasons.

**(a) The README never tells you where a ROM comes from.** Both run recipes are
`cargo run ... -- <rom.bin>` with `<rom.bin>` a placeholder. `grep -niE rom README.md` returns 13
lines; the only concrete artifact named is "Aeon's debug ROM" (line 95), with no path, no repo, and
no instruction to build it. A newcomer with a clean checkout cannot execute step one.

**(b) The README does not know the debug window exists.** `grep -n "oracle-player" README.md` →
**no match, rc=1**. The Layout section says *"Four crates in one workspace"* and names
`oracle-core`, `oracle-aether`, `oracle-frontend`, `oracle-replay`. `ls -1 crates/` shows **six**:
those four plus `oracle-panels-spike` and **`oracle-player`**. `oracle-player` is the entire debug
surface — Registers, Memory, Objects, Screen, Pacing — i.e. the window that does jobs 2 through 5.
A newcomer reading only the front door does not learn that it exists, and would attempt every
debugging job in the wrong window.

Window: this is a repo-front-door finding that lands **against `oracle-player`** (the window that is
missing), discovered before either window was launched.

## Job 1 sidebar — asking either window for help

Evidence: `help-frontend.txt`, `help-player.txt` (verbatim, with exit codes).

| invocation | `oracle-frontend` (game window) | `oracle-player` (debug tabs) |
|---|---|---|
| `--help` | ``error: unknown flag `--help` `` then usage, **EXIT=2** | full usage + 5 paragraphs of prose, EXIT=64 |
| `-h` | `cannot read ROM -h: No such file or directory (os error 2)`, **EXIT=1** | same full usage, EXIT=64 |
| no args | `error: missing <rom.bin>` + usage, EXIT=2 | `--rom is required` + usage, EXIT=64 |

The two windows disagree about the same fact — *how do I ask this program what it takes?* — and the
game window's answer to `-h` is actively misleading: it reports a **missing ROM file called `-h`**,
never mentions that `-h` is not a flag, and prints no usage at all. `--help` is called an *error*
by the one window and is the documented way in by the other.

---

## Rig and ROM (facts the findings rest on)

* Display `:90`, Xvfb pid 3863285, geometry read back from inside: 1280x960x24.
* **Isolation proof form used: TWO-SIDED**, `isolation-proof.txt`. Present half — both windows
  enumerated on `:90` (`oracle-player` by `--pid 1039145`, `oracle-frontend` by
  `--wm-class oracle-frontend`, which is the filter that discriminates; the pid filter finds it
  nowhere). Absent half — `WAYLAND_DISPLAY`/`XDG_SESSION_TYPE` read ABSENT from
  `/proc/<pid>/environ` of the RUNNING processes, **and** their open unix sockets enumerated:
  X11 only, to the Xvfb this rig spawned, no socket to `/run/user/1000/wayland-0`, which **does
  exist on this box** (positive control on the absence target). Private sockets bound:
  `.uxrig/sock/{frontend,player}.sock`, never the shared chain.
* **ROM outage: 07:39:39 → 07:48:13 UTC = 8m34s** with up to three of another lane's `./build.sh`
  in flight. Not a product finding; time burned is real. Snapshot validated COMPLETE:
  header end `0xcec52` → 846931 bytes == file size, `sha256
  bb492d964eb30440c27feb0ae7a67a5be875e2a6547325822c03bbee63d13b34`, domestic name
  `SONIC THE HEDGEHOG 4` (`rom-integrity.txt`).

## Job 1 — a game rendering. DONE, 1 step once a ROM existed.

`launch.sh frontend $ROM`. `shots/job1-game-window.png` — Sonic 4 level art, rings, a sprite.
The **launch banner on stdout is excellent** and lists every key binding; a newcomer who launches
from a menu or a desktop file never sees it. The in-window route it names is the `` ` `` palette.

## Job 2 — what drew this pixel. DONE, 1 click, in BOTH windows — and they answer differently.

| | `oracle-frontend` (game window) | `oracle-player` (debug tabs) |
|---|---|---|
| gesture | one click on the sprite | one click on the sprite in the Screen panel |
| on-screen answer | `WATCH SPRITE 0 TILE $3F8 + SAT $B800` | `That dot is sprite 0, drawn from VRAM-absolute tile $3F8.` / `sprite 0 at (152,104) 2x2 cells, base $3F8, pal 1 hi-pri: tile $3F8 @ VRAM $7F00-$7F1F, SAT entry @ VRAM $B800-$B807` |
| layer | yes (SPRITE) | yes |
| tile | yes ($3F8) | yes |
| **palette** | **NO** | yes (`pal 1 hi-pri`) |
| evidence | `shots/job2-a-gamewindow-click.png` | `shots/job2-b-player-screen-click.png` |

Job 2 asks for layer, tile **and palette**. The game window's on-screen line drops the palette; the
identical click prints the full line, palette included, to the game window's **stdout** — so the
window is a strict subset of its own terminal output.

## Job 3 — stop at a chosen moment. DONE in `oracle-player`, 6 steps, ~3m50s, most of it lost.

07:50:52 typed `VBlank` (a newcomer's guess) → 07:51:13 armed → REFUSED.
07:51:13→07:53:47 (**2m34s**) hunting a valid symbol name.
07:55:05 armed `VBlank_Handler` → `HALTED BY BREAKPOINT b0 at 0x00002334 (VBlank_Handler)`.

## Job 4 — read a register and a memory address while stopped. DONE in `oracle-player`.

PC `00002334`, SR `2604`, A7=SP `FFFFFEF6`, D3 `0000FFFF` (`shots/job4-b-registers-scrolled.png`).
Memory `0x00FF8000` = `00 00 23 58 00 00 22 8F ...`, labelled `region work RAM`
(`shots/job4-d-memory-read.png`). The A7/SP/USP/SSP explanatory note under the register file is
genuinely good.

## Job 5 — what is alive. DONE in `oracle-player`, 1 click.

Objects tab: engine `aeon-sst`, table `0x00FF8FFE`, 66 slots, `$50` bytes each.
Identified: **slot 36, `Spring_Main`, addr `0x00FF9B3E`, code `0x2A3A`, at (520,536)**; also
slot 0 `Player_Main` `0x00FF8FFE` code `0x0208` at (256,256).
Evidence `shots/job5-a-objects-tab.png`, `shots/job5-b-objects-live.png`.
