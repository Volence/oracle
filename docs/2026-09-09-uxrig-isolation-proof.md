# uxrig isolation proof

**Tree:** `/home/volence/sonic_hacks/oracle-uxrig` (worktree, branch `rig/ux-seat-rig`)
**Pin:** `e06a3717019d2d406653c2937096f8adf8f67e92` — oracle `main` tip at 2026-09-09T07:08Z
**Binaries under proof:** `<tree>/target/release/oracle-frontend`, `<tree>/target/release/oracle-player`,
built in **that tree** (308 crates, `Finished release profile in 59.09s`, exit 0, 2026-09-09T07:14:30Z).
The player window's own title bar reads `build e06a3717019d`, which is the pin.
**All timestamps UTC.** Every command and its verbatim output is reproduced below.

The owner's live debug window (**pid 1570308**, on `:0`) was running throughout. It was never
killed, signalled, or interacted with. It appears in this document twice — once as a legitimate
`:0` inhabitant, once as a read-only positive control (`/proc` + `ss` only).

---

## 0. What each claim rests on, and which form it takes

The charter asks each proof to declare its form. This table is the summary; the sections below are
the evidence.

| claim | form | rests on |
|---|---|---|
| the window is on our display | **present half** | window enumeration on `:90` (§3) |
| nothing of ours is on `:0` | **absent half — WEAK, insufficient alone** | enumeration on `:0` (§4) |
| the process cannot reach the compositor at all | **absent half — LOAD-BEARING** | `/proc/<pid>/environ` + peer-resolved socket enumeration (§5) |
| the private socket is bound | one-sided (legitimate) | the listening socket in kernel state (§6) |
| the owner's dock layout is untouched | **structural, not before/after** | `/proc` env + where the RON actually landed (§7) |
| the unforced case refuses | refusal shown | §2 |

### Why the `:0` enumeration cannot stand as the absent half on this machine

This is the correction that matters most, and it is inherited from a real incident rather than
reasoned from first principles.

`docs/2026-08-29-window-runtime-checks.md:52-55` records that this lane set `DISPLAY=:91`,
launched, and the window appeared **on the owner's real screen**. The log said `Wayland window`
while python-xlib found **zero** windows on the Xvfb. The window escaped to his **compositor** —
not to `:0`. An X-only absence check on `:0` would have returned a clean, empty, *passing* answer
during exactly the incident it exists to prevent. A failing question and an empty world produce
identical output.

So §4 is kept — it is cheap and worth having — but the absent half is carried by §5, which asks a
different question: not *is the window absent from that other X server*, but *can this process
reach a Wayland compositor at all*. That is a property of the running process, so it does not
depend on catching a window at the right moment.

### ⚠ The transferable version of this is narrower than "strip the variable and you are safe"

§5's environment half is load-bearing **on this dependency graph** and would not be on another.
Re-derived here firsthand, not taken on report:

```
$ cargo tree -p oracle-frontend -i wayland-client
wayland-client v0.29.5
├── minifb v0.28.0
│   └── oracle-frontend v0.0.0 (/home/volence/sonic_hacks/oracle-uxrig/crates/oracle-frontend)

$ cargo tree -p oracle-player -i wayland-client
wayland-client v0.31.15
├── calloop-wayland-source v0.3.0
│   └── smithay-client-toolkit v0.19.2
│       ├── sctk-adwaita v0.10.1
│       │   └── winit v0.30.13
│       │       ├── accesskit_winit v0.32.2
│       │       │   └── egui-winit v0.36.1
│       │       │       └── eframe v0.36.1
│       │       │           └── oracle-player v0.0.0 (...)
```

| binary | toolkit | wayland-client | `WAYLAND_DISPLAY` unset |
|---|---|---|---|
| `oracle-frontend` | minifb 0.28.0 | **0.29.5** | `display.rs:146-150` → `NoCompositorListening`; also `XdgRuntimeDirNotSet` |
| `oracle-player` | eframe → winit 0.30.13 | **0.31.15** | `conn.rs:69-71` → `.ok_or(ConnectError::NoCompositor)?` |

Verbatim from `~/.cargo/registry/src/*/wayland-client-0.31.15/src/conn.rs`:

```rust
let socket_name = env::var_os("WAYLAND_DISPLAY")
    .map(Into::<PathBuf>::into)
    .ok_or(ConnectError::NoCompositor)?;
```

and from `wayland-client-0.29.5/src/display.rs`:

```rust
let mut socket_path = env::var_os("XDG_RUNTIME_DIR")
    .map(Into::<PathBuf>::into)
    .ok_or(ConnectError::XdgRuntimeDirNotSet)?;
socket_path
    .push(env::var_os("WAYLAND_DISPLAY").ok_or(ConnectError::NoCompositorListening)?);
```

Neither invents a default. **The C binding does**: `wl_display_connect(NULL)` falls back to the
literal name `wayland-0` under `$XDG_RUNTIME_DIR`, which is why aurora measured a window-less
Electron under `xvfb-run` reporting the owner's two real monitors with `WAYLAND_DISPLAY` deleted.
On that surface an environ read is fully consistent with being on his screen.

> **The deciding variable is the language binding, not the desktop.** Sigil and aurora inherit this
> table; the wrong lesson to carry away is "strip the variable and you are safe". It is true for
> Oracle's two binaries and false for a C or Electron surface. Whichever surface you are on, run
> the socket enumeration too — no-fallback in `connect_to_env` is a proof about one function, not
> about the whole graph.

This is also why the rig sets **`XDG_RUNTIME_DIR` to private scratch** rather than leaving the real
one: it makes 0.29.5's second failure mode (`XdgRuntimeDirNotSet`) reachable, and it moves
`$XDG_RUNTIME_DIR/oracle.sock` — a live link in the Aether default chain — out of reach. Nothing in
the rig needs the real one; the only casualty is audio (§8).

---

## 1. The build, in the named tree

A prior run in this suite named a *binary* and silently measured the main checkout's build, so the
tree is named and the identity is checked rather than assumed. `/home/volence/sonic_hacks/oracle/target/release/`
holds older binaries with the same names.

```
$ stat -c '%i %n' target/release/oracle-player target/release/oracle-frontend
44974010 target/release/oracle-player
44974038 target/release/oracle-frontend
$ stat -c '%i %n' /home/volence/sonic_hacks/oracle/target/release/oracle-player \
                  /home/volence/sonic_hacks/oracle/target/release/oracle-frontend
53031218 /home/volence/sonic_hacks/oracle/target/release/oracle-player
53038072 /home/volence/sonic_hacks/oracle/target/release/oracle-frontend
```

Distinct inodes on the same filesystem (`%d` = 66307 for both) — separate builds, not hardlinks
into the main checkout.

---

## 2. The unforced case REFUSES (it does not silently fall back)

⚑ No production code was edited to manufacture any of these. The rig only ever changed files under
`tools/uxrig/` and `docs/`. Refusals R1–R4 are the helper's own logic; R5 required a mutation, which
is shown applied on disk and then restored.

Note the ambient environment during R1: `DISPLAY=:0` and `WAYLAND_DISPLAY=wayland-0` were both set
and inherited. A fall-through would have landed on the owner's desktop.

**R1 — no private display** (2026-09-09T07:17:22Z):

```
$ tools/uxrig/launch.sh frontend /home/volence/sonic_hacks/aeon/s4.debug.bin
uxrig: REFUSED: no private display: /home/volence/sonic_hacks/oracle-uxrig/.uxrig/display does not
exist. Run 'launch.sh display-start' first. This rig will NOT fall back to $DISPLAY, which is the
owner's real desktop.
exit=3
```

**R2 — bare `--aether` with no `--socket`** (07:18:13Z):

```
$ tools/uxrig/launch.sh frontend --aether $ROM
uxrig: REFUSED: bare --aether with no --socket. That resolves the CONTRACT DEFAULT chain
($ORACLE_SOCKET -> $EXODUS_SOCKET -> $XDG_RUNTIME_DIR/oracle.sock -> /tmp/oracle.sock), which is
the path the owner's live window holds. ...
exit=3
```

**R3 / R4 — a `--socket` that points at a shared path** (07:18:13Z). Both links of the chain that
actually exist on this box are refused:

```
$ tools/uxrig/launch.sh player --rom $ROM --socket /run/user/1000/oracle.sock
uxrig: REFUSED: --socket /run/user/1000/oracle.sock is not under this rig's private socket dir
/home/volence/sonic_hacks/oracle-uxrig/.uxrig/sock
exit=3
$ tools/uxrig/launch.sh frontend --socket /tmp/oracle.sock $ROM
uxrig: REFUSED: --socket /tmp/oracle.sock is not under this rig's private socket dir ...
exit=3
```

**R5 — a binary outside the named tree.** MUTATION APPLIED ON DISK (07:18:24Z), quoted back:

```
$ mv target/release/oracle-frontend .uxrig/oracle-frontend.real
$ ln -s /home/volence/sonic_hacks/oracle/target/release/oracle-frontend target/release/oracle-frontend
$ readlink -f target/release/oracle-frontend
/home/volence/sonic_hacks/oracle/target/release/oracle-frontend      <-- the MAIN checkout

$ tools/uxrig/launch.sh frontend $ROM
uxrig: REFUSED: /home/volence/sonic_hacks/oracle-uxrig/target/release/oracle-frontend resolves to
/home/volence/sonic_hacks/oracle/target/release/oracle-frontend, which is outside the named tree
/home/volence/sonic_hacks/oracle-uxrig
exit=3
```

Restored, and the original inode confirmed back in place:

```
$ rm target/release/oracle-frontend && mv .uxrig/oracle-frontend.real target/release/oracle-frontend
$ stat -c '%i %n' target/release/oracle-frontend
44974038 target/release/oracle-frontend        <-- matches §1
```

### R6 — the display gate is loud on unmeasurable, and this was proven the hard way

The first run of `display-start` failed because `verify_geometry` called python-xlib APIs that do
not exist. The gate behaved correctly — it refused rather than rendering "couldn't measure" as
green:

```
uxrig: REFUSED: could not read the screen back from :90 — the display is UNMEASURABLE, which is a
failure and not a pass. Xvfb killed. Detail: AttributeError: protocol_major_version
exit=3
```

But it exposed a real ordering defect: `display-start` wrote **both** state files and *then*
verified, leaving a display that `require_display` would have accepted and nobody had measured.
Verification now happens **before** `$DISPLAY_FILE` is published, and a failed verification kills
the Xvfb it started. Fixed in `254516d`.

---

## 3. PRESENT half — the windows are on our own display

The display is owned by this rig and its geometry is **read back from inside it** (there is no
`xdpyinfo` on this machine, contrary to the brief's suggestion — see §9):

```
$ tools/uxrig/launch.sh display-start                              # 07:18:07Z
uxrig: display :90 pid 1651593  screen(read back from inside): 1280x960 depth=24
       vendor='The X.Org Foundation' protocol=11.0 screens=1
:90
```

**The game window** (`oracle-frontend`, spawn pid 1714665), 07:18:42Z:

```
$ python3 tools/uxrig/drive.py windows --display :90
display :90: 2 windows in tree, 1 named/with-pid
      0x200021  pid=    None     896x672+192+144   viewable  class=oracle-frontend/oracle-frontend  name='Oracle: draws 799'
```

**The debug window** (`oracle-player`, spawn pid 2236980), 07:22:51Z:

```
$ python3 tools/uxrig/drive.py windows --display :90 --pid 2236980
display :90: 4 windows in tree, 1 shown (filter: pid=2236980)
      0x400002  pid= 2236980        1280x800+0+0   viewable  class=/oracle-player  name='oracle-player'
```

Both windows render real content on this display; see `.uxrig/shots/root.png` (1280x960, 4264
distinct colours) and `.uxrig/shots/frontend2.png` (896x672, 26 colours).

### ⚑ `--pid` is a VACUOUS absence filter for the game window

The brief asked for the absent half to be proven "by PID you recorded at spawn". **That is not
available for `oracle-frontend`**: minifb sets no `_NET_WM_PID`, so the pid filter returns zero
matches *on the display where the window demonstrably is*:

```
$ python3 tools/uxrig/drive.py windows --display :90 --pid 1714665     # 07:22:02Z
display :90: 2 windows in tree, 0 shown (filter: pid=1714665)
  (NO window on :90 matches pid=1714665)

$ python3 tools/uxrig/drive.py windows --display :90 --wm-class oracle-frontend
display :90: 2 windows in tree, 1 shown (filter: class~'oracle-frontend')
      0x200021  pid=None  896x672+192+144  viewable  class=oracle-frontend/oracle-frontend  name='Oracle: draws 12739'
```

A pid-based absence check on `:0` for that window would have returned "absent" no matter what. The
filter that discriminates is `--wm-class`. For `oracle-player` (winit) `_NET_WM_PID` **is** set and
the pid filter is the right one — its WM_CLASS collides with the owner's window, so class would be
the confounded filter there. **The two windows need opposite filters.** Both are implemented.

---

## 4. ABSENT half, part one — enumeration of `:0` (WEAK; kept, not relied on)

07:21:38Z and 07:22:08Z. Neither spawned pid carries a window on `:0`, and no window on `:0` has
the game window's class:

```
$ python3 tools/uxrig/drive.py windows --display :0 --pid 1714665      # frontend
display :0: 122 windows in tree, 0 shown (filter: pid=1714665)
  (NO window on :0 matches pid=1714665)
$ python3 tools/uxrig/drive.py windows --display :0 --pid 1651593      # our Xvfb
  (NO window on :0 matches pid=1651593)
$ python3 tools/uxrig/drive.py windows --display :0 --pid 2236980      # player
  (NO window on :0 matches pid=2236980)
$ python3 tools/uxrig/drive.py windows --display :0 --wm-class oracle-frontend
display :0: 122 windows in tree, 0 shown (filter: class~'oracle-frontend')
  (NO window on :0 matches class~'oracle-frontend')
```

**Control for this check** — the same command on the same display with a filter that *should* hit
does hit, so the empty answers above are real absences and not a broken query. The owner's window
is legitimately on `:0`; it is not ours and was not disturbed:

```
$ python3 tools/uxrig/drive.py windows --display :0 --wm-class oracle-player
display :0: 122 windows in tree, 1 shown (filter: class~'oracle-player')
     0x1600002  pid= 1570308      1920x1012+0+24   viewable  class=/oracle-player  name='oracle-player'
```

**This section is not the absent half.** Per §0 it would have passed during the 2026-08-29
incident. §5 is the absent half.

---

## 5. ABSENT half, LOAD-BEARING — the process cannot reach a compositor

Two independent questions, asked of the **running process** rather than of the launch line.
Environment-as-launched and environment-as-running are different claims.

### The game window (pid 1714665), 07:19:59Z

```
$ python3 tools/uxrig/drive.py procproof --display :90 --pid 1714665 --xvfb-pid 1651593
--- /proc/1714665/environ (environment AS RUNNING, not as launched) ---
  WAYLAND_DISPLAY    ABSENT  <- required
  XDG_SESSION_TYPE   ABSENT  <- required
  DISPLAY            :90
  WINIT_UNIX_BACKEND x11
  XDG_RUNTIME_DIR    /home/volence/sonic_hacks/oracle-uxrig/.uxrig/run
  XDG_DATA_HOME      /home/volence/sonic_hacks/oracle-uxrig/.uxrig/xdg/data
  ORACLE_SOCKET      /home/volence/sonic_hacks/oracle-uxrig/.uxrig/sock/oracle.sock
  EXODUS_SOCKET      /home/volence/sonic_hacks/oracle-uxrig/.uxrig/sock/oracle.sock
--- /proc/1714665/fd unix sockets, resolved through their PEER ---
  inode    198884336 client       local=*
       peer inode    198892310 path=@/tmp/.X11-unix/X90 held by(("Xvfb",pid=1651593,fd=8))
  inode 198887189: not a unix socket (netlink/other)
  inode    198894796 LISTEN(ours) local=/home/volence/sonic_hacks/oracle-uxrig/.uxrig/sock/frontend.sock
       peer inode            0 path=(none) held by
--- verdict ---
  connected to an X server on its own DISPLAY (:90, socket *.X11-unix/X90): YES
  and that X server is THE Xvfb THIS RIG SPAWNED (pid 1651593): YES
  holds ANY socket to a Wayland compositor: NO
  (the compositor socket that must not appear above: /run/user/1000/wayland-0; it exists on this
   box: True, so its absence here is a real absence and not a missing target)
  STRUCTURAL ISOLATION: PROVEN
exit=0
```

### The debug window (pid 2236980), 07:22:51Z — same verdict

```
  inode    199460502 client  local=*
       peer inode    199451943 path=@/tmp/.X11-unix/X90 held by(("Xvfb",pid=1651593,fd=13))
  inode    199461935 LISTEN(ours) local=/home/volence/sonic_hacks/oracle-uxrig/.uxrig/sock/player.sock
  inode    199461936 client  local=*
       peer inode    199463481 path=@/tmp/.X11-unix/X90 held by(("Xvfb",pid=1651593,fd=11))
--- verdict ---
  connected to an X server on its own DISPLAY (:90, socket *.X11-unix/X90): YES
  and that X server is THE Xvfb THIS RIG SPAWNED (pid 1651593): YES
  holds ANY socket to a Wayland compositor: NO
  STRUCTURAL ISOLATION: PROVEN
```

Note what the peer resolution buys. An X *client* socket carries no path of its own — the path
lives on the listening side, so `ss` shows our fd as `* <inode> * <peer-inode>`. Resolving through
the peer inode does not merely recover the path; it recovers **the process holding it**, which is
then compared against **the Xvfb pid this rig recorded at spawn**. The window process is bound to
our own display by kernel state, not by a string in a log. (The first version of this check grepped
our own pid's `ss` lines for `/tmp/.X11-unix` and found nothing even though the connection was
real — a false negative, fixed in `254516d`.)

### Controls — this check is not vacuous

Both read-only (`/proc` + `ss`; no signal, no interaction).

**Control A — a genuine Wayland client** (`dolphin`, pid 1789), 07:21:11Z. Every discriminating
branch goes red:

```
  WAYLAND_DISPLAY    PRESENT wayland-0  <- FORBIDDEN
  XDG_SESSION_TYPE   PRESENT wayland  <- FORBIDDEN
       peer inode        29267 path=/run/user/1000/wayland-0 held by
  connected to an X server on its own DISPLAY (:0, socket *.X11-unix/X0): NO  <- required
  and that X server is THE Xvfb THIS RIG SPAWNED (pid 1651593): NO  <- required
  holds ANY socket to a Wayland compositor: YES  <- FORBIDDEN
  STRUCTURAL ISOLATION: NOT PROVEN
exit=5
```

**Control B — the owner's own window** (pid 1570308). This control **found a defect in the check
itself**, which is why it is reported rather than merely passed. The first version matched the
substring `wayland` anywhere in the `ss` line, so `@/tmp/.X11-unix/X0 held by (("Xwayland",...))`
was flagged as a compositor connection. **Xwayland is an X server**; a client talking to it is
talking X11. Classification is now on the peer socket's basename (`wayland-<N>`), never on the
process name. After the fix:

```
  and that X server is THE Xvfb THIS RIG SPAWNED (pid 1651593): NO  <- required
  holds ANY socket to a Wayland compositor: NO
  STRUCTURAL ISOLATION: NOT PROVEN
```

Correctly not flagged as a compositor connection, and still `NOT PROVEN` on the pid branch. Fixed
in `254516d`.

> Incidental but worth recording: the owner's `oracle-player` reports `WAYLAND_DISPLAY` **absent**
> and `DISPLAY=:0`, and holds three sockets to `Xwayland` pid 996. His window is running under
> X11/Xwayland, not native Wayland.

---

## 6. The socket — one-sided, and legitimately so

Read back from kernel state rather than from the log line, which is the stronger of the two
available readings. From the `procproof` output above:

```
  LISTEN(ours) local=/home/volence/sonic_hacks/oracle-uxrig/.uxrig/sock/frontend.sock
  LISTEN(ours) local=/home/volence/sonic_hacks/oracle-uxrig/.uxrig/sock/player.sock
```

Both processes' own logs agree:

```
aether: serving on /home/volence/sonic_hacks/oracle-uxrig/.uxrig/sock/frontend.sock (mode 0600, 62 methods, protocol version 1)
aether: serving on /home/volence/sonic_hacks/oracle-uxrig/.uxrig/sock/player.sock   (mode 0600, 62 methods, protocol version 1)
```

The owner's window continues to hold `/run/user/1000/oracle.sock` (visible in Control B's
enumeration as `LISTEN(ours) local=/run/user/1000/oracle.sock`, from *his* process's point of
view). Two servers, two paths, no collision.

**Why one-sided is enough here, unlike the display.** A Unix socket has exactly one bound path per
listener and the kernel enforces it; a process cannot bind two paths with one listener, so
"the private path is bound" leaves no room for a simultaneous shared binding. The display case had
that room — a toolkit really can open a window somewhere other than where `DISPLAY` points — which
is precisely why that one needs two sides.

Belt and braces beyond the flag: `ORACLE_SOCKET`, `EXODUS_SOCKET` and `XDG_RUNTIME_DIR` are all set
to private values in the launch environment (visible in §5), so **every link of the contract's
default chain terminates at a value this rig set explicitly.** There is no shared last resort left
for a missed flag to reach.

---

## 7. The owner's dock layout — structural, because before/after is confounded

`oracle-player` persists its layout via eframe storage to `$XDG_DATA_HOME/oracle-player/app.ron`
(`crates/oracle-player/src/layout.rs:1014-1022`; the app id is `ui::APP_NAME = "oracle-player"`).
A seat must not read or write the owner's real one.

⚠ **A before/after hash of his file cannot prove this**, and it is worth saying why rather than
quietly not doing it. His own live window rewrites that file continuously — its mtime moved from
07:22:08Z to 07:25:38Z during this run, with no involvement from the rig. Any after-differs-from-
before result would be his autosave, not evidence about us. The measurement is available and
useless. The structural claim is the one that holds:

```
$ tr '\0' '\n' < /proc/2236980/environ | grep -E '^XDG_(DATA|CONFIG|RUNTIME)'
XDG_RUNTIME_DIR=/home/volence/sonic_hacks/oracle-uxrig/.uxrig/run
XDG_CONFIG_HOME=/home/volence/sonic_hacks/oracle-uxrig/.uxrig/xdg/config
XDG_DATA_HOME=/home/volence/sonic_hacks/oracle-uxrig/.uxrig/xdg/data

$ ls -l /proc/2236980/fd | grep -c 'local/share'
0

$ find .uxrig/xdg -type f -name app.ron -exec stat -c '%s bytes  %n' {} \;
5380 bytes  .uxrig/xdg/data/oracle-player/app.ron
```

And the behavioural witness, from the player's own startup log — it did **not** restore the
owner's 25453-byte layout, because it never saw it:

```
layout: none stored yet (the default arrangement)
```

Our layout landed in scratch (5380 bytes). The owner's file remains 25453 bytes.

---

## 8. What is NOT proven, and the instrument that would

* **Audio is not merely out of scope; it is structurally unavailable.** Both binaries log
  `ALSA function 'snd_pcm_open' failed with error 'Host is down (112)'` — a consequence of the
  private `XDG_RUNTIME_DIR`, since PipeWire's socket lives at `$XDG_RUNTIME_DIR/pipewire-0`. This
  is a feature (the rig cannot touch the owner's audio) but it means **no seat can make any audio
  finding**, and the player states `Pacing is UNMEASURED` for the same reason. Instrument that
  would change it: a private PipeWire/ALSA null sink — deliberately not built, since it would
  reintroduce a path out of the sandbox for no UX benefit.
* **Pacing / vsync / frame timing.** A virtual display has no vsync and rendering is llvmpipe
  (`MESA-EGL: warning: DRI3 error: Could not get DRI3 device`). Numbers on the Pacing tab are real
  numbers about an unrepresentative machine. Out of scope for these seats by construction.
* **Gamepad.** `gamepad: no controllers detected, keyboard only`. Nothing in the rig can present
  one without `/dev/uinput`, which is forbidden (§9).
* **That no *future* code path writes the owner's `app.ron`.** §7 proves this run did not and
  cannot via XDG. A stronger instrument exists — running the process under a bind-mount namespace,
  or `strace -f -e trace=openat` filtered to `~/.local/share` — and was not used because it was not
  needed for the claim made. Named here so a later seat can reach for it rather than re-deriving.
* **Anything requiring the owner's real display.** Refused by design, not unproven.

---

## 9. Things the brief got wrong, and one thing this lane had banked wrong

Reported plainly because the reason travels further than the result.

1. **`xdpyinfo` is not installed on this machine.** The brief offered it as the primary way to
   verify screen geometry from inside the display. `which xdpyinfo` → `xdpyinfo not found`. The
   fallback the brief also named (python-xlib `screen()`) is what the helper uses. No impact beyond
   the tool choice.
2. **"By PID you recorded at spawn" does not work for the game window.** minifb sets no
   `_NET_WM_PID`. Detailed in §3; this changed the shape of the check rather than just its
   implementation, because a pid-based absence check for that window is *vacuous* rather than
   merely unavailable.
3. **`/dev/uinput` is NOT root-only on this box**, contrary to what this lane had banked. The mode
   string `crw-rw---- root:root` is not the authority; a POSIX ACL grants this user access:
   ```
   $ getfacl /dev/uinput
   user::rw-
   user:volence:rw-
   mask::rw-
   ```
   The only marker in a long listing is a `+` — rendered as `@` under this machine's `eza` alias,
   which is very easy to read straight past. **A mode string is not a permission.** It remains
   forbidden here regardless: uinput events enter the *kernel* input stack and land wherever focus
   is, which is the owner's live desktop. XTEST is the only input path structurally confined to a
   display, and it is what the rig uses.
4. **The `:0` enumeration is insufficient as the absent half** (§0). Corrected mid-task by the
   controller; recorded here because sigil and aurora inherit the table.
5. **The Wayland-fallback lesson is binding-specific, not universal** (§0). Also corrected
   mid-task, and re-derived firsthand here.

Defects found in the rig's own checks by running them, all fixed and all recorded in commit
bodies: the unmeasurable-display ordering bug (§2 R6), the X-client-socket false negative (§5), and
the `Xwayland` false positive (§5 Control B). Each was found by a control or by a real run, none by
review.

---

## 10. Reproducing this

```sh
cd /home/volence/sonic_hacks/oracle-uxrig
tools/uxrig/launch.sh display-start                  # prints :NN, verified from inside
DISP=$(cat .uxrig/display)
nohup tools/uxrig/launch.sh frontend /home/volence/sonic_hacks/aeon/s4.debug.bin \
      > .uxrig/frontend.log 2>&1 & echo $! > .uxrig/frontend.pid
python3 tools/uxrig/drive.py procproof --display $DISP \
      --pid $(cat .uxrig/frontend.pid) --xvfb-pid $(cat .uxrig/xvfb.pid)
tools/uxrig/launch.sh display-stop                   # kills ONLY the recorded pid
```

Commits: `14869af` (the two tools), `254516d` (isolation proved from kernel state), `79686f0`
(placement and focus).
