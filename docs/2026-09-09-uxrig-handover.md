# uxrig — seat handover

**Tree:** `/home/volence/sonic_hacks/oracle-uxrig` (worktree, branch `rig/ux-seat-rig`)
**Pin:** `e06a3717019d2d406653c2937096f8adf8f67e92`
**Binaries:** `<tree>/target/release/{oracle-frontend,oracle-player}`, built in that tree.
**Isolation evidence:** `docs/2026-09-09-uxrig-isolation-proof.md`. Read §0 of it once before you
start; you do not need the rest.

Everything below is copy-pasteable from the tree root. `cd /home/volence/sonic_hacks/oracle-uxrig`
first.

---

## ⚑ The three rules

1. **The owner is asleep and his live debug window is running: pid 1570308 on `:0`.** Never kill,
   signal, or interact with it, or with any process you did not start. Never `pkill`/`killall` by
   name — that has bitten two lanes on this machine. Kill only PIDs you recorded at spawn.
2. **Never launch a window any way other than through `launch.sh`.** Forcing the display is the
   helper's job precisely so it is not yours to remember. Running a binary directly with
   `DISPLAY=:90` set is *not* equivalent and has put a window on the owner's real screen before.
3. **Never press F2 in either window.** Save states are written *next to the ROM*, in the aeon tree
   which is read-only for us: `/home/volence/sonic_hacks/aeon/s4.debug.state0` already exists and
   F2 would overwrite it. F4 (load) and F6/F7 (pick slot) are read-only and safe. Same for the
   game saving to `s4.debug.srm`.

---

## Start the display (once per session)

```sh
tools/uxrig/launch.sh display-start
```

Prints the display it picked (`:NN` — probed free, not hardcoded) and the screen geometry read back
from inside it. Everything after this reads the display from `.uxrig/display`:

```sh
DISP=$(cat .uxrig/display)
```

`tools/uxrig/launch.sh display-show` prints the current display, the Xvfb pid, and all the scratch
paths. When you are done: `tools/uxrig/launch.sh display-stop` (kills only the pid it recorded).

---

## Launch the two windows

Both run in the **foreground**, so background them and record the pid — the pid you get is the
binary's own pid, because the helper `exec`s it.

**The game window** (`oracle-frontend`, minifb):

```sh
nohup tools/uxrig/launch.sh frontend /home/volence/sonic_hacks/aeon/s4.debug.bin \
  > .uxrig/frontend.log 2>&1 & echo $! > .uxrig/frontend.pid
```

**The debug tabs** (`oracle-player`, egui + egui_dock):

```sh
nohup tools/uxrig/launch.sh player \
  --rom /home/volence/sonic_hacks/aeon/s4.debug.bin \
  --symbols /home/volence/sonic_hacks/aeon/s4.debug.lst \
  > .uxrig/player.log 2>&1 & echo $! > .uxrig/player.pid
```

`--x11` and `--socket <private>` are supplied by the helper. **Do not pass `--aether`** — a bare one
is refused, by design. Extra flags pass straight through, so `--dock every-tab`, `--scale`,
`--aspect` etc. all work.

Confirm it came up where you think, before you drive anything:

```sh
python3 tools/uxrig/drive.py windows --display $DISP
```

Optionally re-run the full structural check on your own launch:

```sh
python3 tools/uxrig/drive.py procproof --display $DISP \
  --pid $(cat .uxrig/player.pid) --xvfb-pid $(cat .uxrig/xvfb.pid)
```

---

## Two gotchas that will cost you an hour if you skip them

**There is no window manager on this display.** Nothing arranges windows, nothing raises them, and
nothing assigns keyboard focus. Two consequences:

### 1. Overlapping windows capture as BLACK

Both windows open near the top-left and overlap. X11 without a compositor does not preserve an
occluded window's contents, so `shot --window <id>` on a covered window returns black. Measured:
958 bytes / 6 distinct colours covered, versus 21825 bytes / 26 colours after raising. **The
screenshot's colour check passes the black capture** (6 > 1), so it will not save you. Raise or
move first:

```sh
python3 tools/uxrig/drive.py place --display $DISP --window 0xID --raise
python3 tools/uxrig/drive.py place --display $DISP --window 0xID --x 0 --y 300
```

Capturing the **root** window is always safe and shows whatever is actually visible.

### 2. Typing does nothing unless you focus first

The display sits at `PointerRoot`. The X server *does* deliver key events to the window under the
pointer, so XTEST appears to work — but winit/egui only act on keys when they believe the window is
focused, and with no `FocusIn` they never do. Measured: clicking a text field and typing left the
placeholder showing; the identical sequence with focus set filled it in. **Clicks are fine without
focus** (they go by pointer position); this is keyboard-only.

```sh
python3 tools/uxrig/drive.py focus --display $DISP --window 0xID
# or let key/type do it for you:
python3 tools/uxrig/drive.py type --display $DISP --window 0xID --text "Sonic_Main"
```

---

## Drive input

Input is **XTEST only**, over the connection to your own display, so it cannot leave it. Coordinates
are display coordinates (both windows sit at known offsets — read them from `windows`).

```sh
python3 tools/uxrig/drive.py move   --display $DISP --x 400 --y 300
python3 tools/uxrig/drive.py click  --display $DISP --x 362 --y 13            # left button
python3 tools/uxrig/drive.py click  --display $DISP --x 362 --y 13 --button 3 # right button
python3 tools/uxrig/drive.py key    --display $DISP --window 0xID --key Return
python3 tools/uxrig/drive.py key    --display $DISP --window 0xID --key F3
python3 tools/uxrig/drive.py type   --display $DISP --window 0xID --text "hello_world"
```

`--key` takes a keysym **name**: `Return`, `Escape`, `Tab`, `space`, `F1`..`F12`, `Left`, `a`,
`grave` (the game window's command palette). Unknown names are an error, never a silent no-op, and
`type` fails loudly if any character is missing from the keyboard mapping rather than typing a
partial string.

---

## Take a screenshot

```sh
python3 tools/uxrig/drive.py shot --display $DISP --out shots/whole-screen.png
python3 tools/uxrig/drive.py shot --display $DISP --window 0xID --out shots/just-that-window.png
```

It decodes the PNG afterwards and prints size and **distinct colour count**, because `import` exits
0 on plenty of degenerate captures. A single flat colour is reported `SUSPECT` and exits non-zero.
Remember gotcha 1: a >1 colour count does not mean the window was not occluded.

---

## Enumerate windows

```sh
python3 tools/uxrig/drive.py windows --display $DISP
python3 tools/uxrig/drive.py windows --display $DISP --pid $(cat .uxrig/player.pid)
python3 tools/uxrig/drive.py windows --display $DISP --wm-class oracle-frontend
```

**The two windows need opposite filters, and this is a real trap:**

| window | toolkit | `_NET_WM_PID` | use |
|---|---|---|---|
| `oracle-player` | winit | **set** | `--pid` (its WM_CLASS collides with the owner's window) |
| `oracle-frontend` | minifb | **NOT set** | `--wm-class oracle-frontend` (`--pid` matches nothing, ever) |

A `--pid` filter on the game window returns zero matches *even on the display where it is*. If you
build any absence check, build it on the filter that discriminates, and prove the filter hits
something before you trust it missing.

---

## What the rig CANNOT do

Findings in these areas are not available to you. Say so rather than reporting a guess.

* **Audio — nothing at all.** Both binaries fail `snd_pcm_open` with `Host is down`, because the
  private `XDG_RUNTIME_DIR` puts PipeWire's socket out of reach. That is deliberate (the rig must
  not touch the owner's audio). No volume, mute, or sound findings.
* **Pacing, vsync, frame timing, "does it feel smooth".** A virtual display has no vsync and
  rendering is llvmpipe software (`DRI3 error: Could not get DRI3 device`). The player itself says
  `Pacing is UNMEASURED`. The Pacing tab's *layout and wording* are fair game; its **numbers** are
  not evidence about the real machine. Explicitly out of scope for these seats.
* **Gamepad / controller.** `gamepad: no controllers detected, keyboard only`. Presenting a virtual
  pad would need `/dev/uinput`, which is **forbidden** here: its events enter the kernel input stack
  and land wherever focus is — i.e. the owner's live desktop. If you need it, that is a BLOCKED
  report, not a workaround.
* **Anything on the owner's real display**, including how a window looks under his compositor, his
  window decorations, HiDPI scaling, or his theme.
* **Window-manager behaviour** — decorations, title bars, snapping, maximise/restore, multi-monitor,
  drag-to-dock across windows. There is no WM. Anything you observe about window borders or
  placement is an artifact of the rig, not of Oracle.
* **The owner's persisted state.** Your player writes its layout to
  `.uxrig/xdg/data/oracle-player/app.ron` and starts from the default arrangement, not his. You
  cannot see, and must not look at, the layout he actually uses.
* **The emulator MCP tools (`mcp__oracle__*`) — never call them.** They deadlock from background
  agents. Everything you need is the two windows plus this driver. If something genuinely needs
  runtime confirmation you cannot get, TAG it for the controller.

---

## Scratch layout (yours alone)

```
.uxrig/display        the display number this rig owns
.uxrig/xvfb.pid       its Xvfb pid — the only pid the rig will ever kill
.uxrig/sock/          private Aether sockets (frontend.sock, player.sock)
.uxrig/xdg/{config,data,cache}/   private XDG homes; the player's app.ron lands under data/
.uxrig/run/           private XDG_RUNTIME_DIR
.uxrig/*.log          launch logs
```

`.uxrig/` is git-ignored (via the worktree's `info/exclude`) and is not shared with anything.
