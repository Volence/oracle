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

---

## The game window's half of jobs 3, 4, 5

Full inventory of `oracle-frontend`'s command palette (the only in-window discovery route it
advertises), walked end to end: `shots/job-fe-a-palette.png`, `job-fe-f-palette-lenses.png`,
`job-fe-g-palette-bottom.png`.

GAME (6) · SAVE STATES (5) · WATCH (2) · LENSES (7: watch ticker, CPU chip, CPU registers,
sprite outlines, CRAM strip, hover callout, profiler panel) · DISPLAY LAYERS (4) ·
SPAWN OBJECTS (2) · SETTINGS (5).

* **Job 3 (breakpoint): not available in the game window at all.** Pause and step-one-frame only.
* **Job 4 (register): available** — LENSES ▸ TOGGLE CPU REGISTERS. **Memory: not available.**
* **Job 5 (live objects): not available.** SPAWN OBJECTS places objects, it does not list them.

`shots/job-fe-h-cpu-registers-lens.png` — the lens shows D0-D7, A0-A7, `SR $2300 S`, and a
symbol line reading `ORLDLINES.NO_SWEEP+$4`: **the symbol name is clipped on the LEFT**, so the
one field that tells you where the CPU is cannot be read. There is **no numeric PC** in this
lens; `oracle-player` prints `PC 00002334` plainly.

## Two windows, two machines, and nothing says so

08:03:50 UTC, one display, one ROM, both windows up:

* `oracle-player` top bar: `HALTED BY BREAKPOINT b0 at 0x00002334 (VBlank_Handler) (1 halt; 1 still armed)`
  (`shots/twomachines-player-still-halted.png`)
* `oracle-frontend` title in the same seconds: `Oracle: draws 45579` → `Oracle: draws 45779`

They are separate emulator instances. Neither window's chrome says which machine it is, and both
show the same ROM path, the same game and the same title art. Job 3 — "stop the game" — succeeds
in one window and has no effect whatsoever on the game the person is watching in the other.

## Look / taste captures for the owner (NOT findings)

* `oracle-player` Registers: the register file needs a scroll in a 1280x800 window — D0-D7 and
  A0-A7/PC/SR are never on screen together in the default arrangement.
* `oracle-player` several panels clip **horizontally**: `emulator/player_stat…`,
  `not present (the sl…`, `only cartridge ROM ($000000..rom_le…`, and the `lookup_symbol` JSON
  reply wraps mid-token.
* `oracle-frontend` palette rows clip horizontally and the hotkey column collides with the label:
  `SPAWN MODE: CLICK TO PLACE AN OBJEC` + `P`, `AUDIO FILTER: VA0-VA2 / VA3-VA6 / R` + `F`.
* `oracle-player` Memory: the write hint says *"call `emulator/pause` first"* — an RPC method name
  offered to a GUI user who has a **pause** button in the same window's top bar.

## What I could not drive, and what would

* **Audio** — both binaries fail `snd_pcm_open` (`Host is down`) under the private
  `XDG_RUNTIME_DIR`. No volume/mute/sound finding is available. Would need the owner's session.
* **Gamepad** — `gamepad: no controllers detected`. Presenting a virtual pad needs `/dev/uinput`,
  which is forbidden here. BLOCKED by rule, not attempted.
* **Pacing / smoothness** — out of scope by charter, and the player itself says
  `Pacing is UNMEASURED`.
* **The `--dock every-tab` arrangement and the owner's own stored layout** — my instance writes to
  a private `app.ron` and starts from the default arrangement.
* **The emulator MCP** — never called.

---

## Time burned, ranked

| burned | on what | window |
|---|---|---|
| **8m34s** | `s4.debug.bin` absent (07:39:39→07:48:13, three of another lane's `build.sh` live). **Rig condition, not a product finding.** Net idle ≈ 0 only because I armed a background poller and spent the outage on the build, the isolation proof and the help-text probes. | — |
| **3m34s** | reaching ONE palette entry (LENSES ▸ TOGGLE CPU REGISTERS), 07:59:48→08:03:22, almost all of it re-issuing arrow keys the window dropped | `oracle-frontend` |
| **2m34s** | recovering a valid symbol name after the breakpoint refusal, 07:51:13→07:53:47 — required opening `commands` and hand-writing `emulator/lookup_symbol {"name":"V"}` | `oracle-player` |
| ~30s | discovering the refusal text was scrollable rather than complete | `oracle-player` |
| ~26s | one wasted arm attempt: the panel had scrolled, so `at` and `arm` were 46px from where they had been, and the click landed on the error text with no feedback | `oracle-player` |

## What the brief and the charter got wrong

1. **"Snapshot the ROM first" is not actionable advice, because the ROM can be gone before your
   first command.** It was: absent at 07:39:39, 15 seconds into the session. The handover frames
   the outage as something early snapshotting avoids. It is not. What worked was arming a poller
   in the background and spending the outage on the ROM-independent work. The handover's own
   snippet — `until [ -s ... ]; do sleep 10; done; snapshot` — is **blocking**, so a seat following
   it literally sits idle for 8+ minutes.
2. **That same snippet would have snapshotted a truncated ROM.** `-s` fires on the first non-empty
   byte of a file being written in place. I added a size-stability gate on my own initiative and
   the integrity check reached me only as an out-of-band coordinator message mid-run — from
   neither authority document. **The header-end equality check belongs in the handover**, next to
   the snapshot command, not in a message a seat may not get.
3. **The charter's "may read `README.md` and nothing else" does not say where help text sits.**
   `--help` output is a product surface, not source. I treated it as in bounds and it produced
   three findings in 20 seconds. A literal-minded seat either skips the cheapest surface in the
   whole product or logs reading it as a violation. **Name it explicitly.**
4. **"Failure to get a ROM in is FINDING NUMBER ONE" conflated two different things on this run.**
   The ROM did go in; what failed was the README's ability to tell anyone where a ROM comes from.
   The brief's framing invites a seat to write the aeon outage — a rig condition — up as the
   product finding. I kept them apart deliberately and recommend the brief do so in wording.
5. **The charter's window table reads as a panel inventory and is not one.** It lists "Registers,
   Memory, Objects, Screen, Pacing, nav"; `oracle-player` also has Planes, Spawn, Effects,
   Breakpoints, Watchpoints, Profiler and a `commands` JSON-RPC console. The console is where I
   recovered from the breakpoint refusal — a seat that took the table as complete would not have
   opened it. This is the charter's own "a list that reads as complete" shape.
6. **"A clean job ships its step count" undercounts what matters.** For job 3 the step count is
   uninformative without saying which steps were spent recovering from the tool. I have reported
   time-to-recovery alongside. Suggest: *steps, and of those, steps spent recovering from the tool.*

## The protocol delta owed to the hub (rides in the paragraph above, per the charter)

The fifth rule governs **resolvers falling through to a shared last resort**. FINDING 11 is the
same family with **nothing falling through at all**: both windows bound their own private socket,
both resolved exactly as instructed, both were correct — and a person still cannot tell which of
two live machines they are looking at, because **neither window displays its identity**. The
delta: *a resolver that never falls through can still leave the user attached to the wrong one of
two correct answers.* The charter audits the chain; this asks the artifact to **say which answer
it got**. Aurora's arrow was reaching, ours was becoming, sigil's was a third; this is a fourth
axis and it is not about direction at all — it is about **whether the result is displayed**. It
surfaced here as a UX defect and not as a safety one purely because both machines were mine.
