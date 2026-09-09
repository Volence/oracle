#!/usr/bin/env python3
"""uxrig input/capture driver — XTEST over a named X display, and nothing else.

WHY XTEST AND ONLY XTEST
------------------------
There is no xdotool, no ydotool and no wtype on this machine. `/dev/uinput` IS openable by this
user (a POSIX ACL grants `user:volence:rw-`; the mode string `crw-rw---- root:root` says otherwise
and is not the authority -- a mode string is not a permission). Do not use it anyway: uinput
creates a KERNEL-level virtual input device, so its events enter the system input stack and land
wherever focus currently is, which on this machine is the owner's live Wayland desktop. XTEST
events are delivered to the X server this process is connected to and cannot leave it, so XTEST is
the only input path that is *structurally* confined to a private display.

Every subcommand takes an explicit --display. There is no default, deliberately: an omitted
display that quietly means ":0" is the exact failure this rig exists to make impossible.

SUBCOMMANDS
  windows     --display :NN [--pid P]        enumerate windows (name, pid, geometry, map state)
  move        --display :NN --x X --y Y
  click       --display :NN --x X --y Y [--button 1]
  key         --display :NN --key Return     press+release one key by keysym name
  type        --display :NN --text "hello"
  shot        --display :NN --out FILE.png [--window ID]   screenshot, content-verified
  procproof   --display :NN --pid P          structural isolation check on a running process
"""

import argparse
import os
import subprocess
import sys
import time

from Xlib import X, XK, display as xdisplay
from Xlib.ext import xtest

# --------------------------------------------------------------------------------------------
# display
# --------------------------------------------------------------------------------------------


def open_display(name):
    """Open a display BY NAME. Never falls back to $DISPLAY."""
    if not name or not name.startswith(":"):
        sys.exit(f"uxrig: REFUSED: --display must be given explicitly as :NN, got {name!r}")
    return xdisplay.Display(name)


# --------------------------------------------------------------------------------------------
# window enumeration
# --------------------------------------------------------------------------------------------


def _atom_prop(d, win, name):
    try:
        a = d.get_atom(name, only_if_exists=True)
        if a == 0:
            return None
        p = win.get_full_property(a, X.AnyPropertyType)
        return p.value if p else None
    except Exception:
        return None


def window_info(d, win):
    try:
        g = win.get_geometry()
    except Exception:
        return None
    pid = _atom_prop(d, win, "_NET_WM_PID")
    pid = int(pid[0]) if pid else None
    name = _atom_prop(d, win, "_NET_WM_NAME")
    if name:
        name = bytes(name).decode("utf-8", "replace")
    else:
        try:
            name = win.get_wm_name()
        except Exception:
            name = None
    try:
        state = {X.IsUnmapped: "unmapped", X.IsUnviewable: "unviewable", X.IsViewable: "viewable"}[
            win.get_attributes().map_state
        ]
    except Exception:
        state = "?"
    cls = None
    try:
        c = win.get_wm_class()
        cls = "/".join(c) if c else None
    except Exception:
        pass
    return {
        "id": hex(win.id),
        "pid": pid,
        "name": name,
        "class": cls,
        "geom": f"{g.width}x{g.height}+{g.x}+{g.y}",
        "state": state,
    }


def walk(d, win, out, depth=0):
    info = window_info(d, win)
    if info:
        info["depth"] = depth
        out.append(info)
    try:
        for c in win.query_tree().children:
            walk(d, c, out, depth + 1)
    except Exception:
        pass


def cmd_windows(args):
    d = open_display(args.display)
    out = []
    walk(d, d.screen().root, out)
    # A window with neither a name nor a pid is almost always an internal/reparenting shim; keep
    # the noise down but NEVER filter by pid when the caller asked about a pid, because "absent"
    # is the answer an absence proof depends on.
    interesting = [w for w in out if w["pid"] is not None or w["name"]]
    if args.pid is not None:
        interesting = [w for w in interesting if w["pid"] == args.pid]
    print(f"display {args.display}: {len(out)} windows in tree, {len(interesting)} named/with-pid"
          + (f", {len(interesting)} matching pid {args.pid}" if args.pid is not None else ""))
    for w in interesting:
        print(f"  {w['id']:>12}  pid={str(w['pid']):>8}  {w['geom']:>18}  {w['state']:>9}  "
              f"class={w['class']}  name={w['name']!r}")
    if args.pid is not None and not interesting:
        print(f"  (no window on {args.display} carries pid {args.pid})")
    return 0


# --------------------------------------------------------------------------------------------
# input (XTEST)
# --------------------------------------------------------------------------------------------


def sync(d):
    d.sync()
    time.sleep(0.03)


def do_move(d, x, y):
    xtest.fake_input(d, X.MotionNotify, x=x, y=y)
    sync(d)


def do_click(d, x, y, button):
    do_move(d, x, y)
    xtest.fake_input(d, X.ButtonPress, button)
    sync(d)
    xtest.fake_input(d, X.ButtonRelease, button)
    sync(d)


def keymap(d):
    """keysym -> (keycode, needs_shift), read back from the server's live mapping."""
    lo = d.display.info.min_keycode
    n = d.display.info.max_keycode - lo + 1
    mapping = d.get_keyboard_mapping(lo, n)
    table = {}
    for i, syms in enumerate(mapping):
        kc = lo + i
        for level, sym in enumerate(syms[:2]):
            if sym and sym not in table:
                table[sym] = (kc, level == 1)
    return table


def press_keysym(d, table, sym):
    if sym not in table:
        return False
    kc, shift = table[sym]
    shift_kc = d.keysym_to_keycode(XK.string_to_keysym("Shift_L"))
    if shift:
        xtest.fake_input(d, X.KeyPress, shift_kc)
    xtest.fake_input(d, X.KeyPress, kc)
    sync(d)
    xtest.fake_input(d, X.KeyRelease, kc)
    if shift:
        xtest.fake_input(d, X.KeyRelease, shift_kc)
    sync(d)
    return True


def cmd_key(args):
    d = open_display(args.display)
    sym = XK.string_to_keysym(args.key)
    if sym == 0:
        sys.exit(f"uxrig: unknown keysym name {args.key!r} (try Return, Escape, Tab, F1, space, a)")
    if not press_keysym(d, keymap(d), sym):
        sys.exit(f"uxrig: keysym {args.key!r} is not in this server's keyboard mapping")
    print(f"key {args.key} pressed+released on {args.display}")
    return 0


def cmd_type(args):
    d = open_display(args.display)
    table = keymap(d)
    missed = []
    for ch in args.text:
        sym = ord(ch) if ord(ch) < 0x100 else 0x1000000 + ord(ch)
        if not press_keysym(d, table, sym):
            missed.append(ch)
    if missed:
        # Loud on partial, never silent. A half-typed string that reports success is a defect.
        sys.exit(f"uxrig: FAILED to type {missed!r} — not in the keyboard mapping; string was typed partially")
    print(f"typed {args.text!r} on {args.display}")
    return 0


def cmd_move(args):
    do_move(open_display(args.display), args.x, args.y)
    print(f"pointer -> {args.x},{args.y} on {args.display}")
    return 0


def cmd_click(args):
    do_click(open_display(args.display), args.x, args.y, args.button)
    print(f"button {args.button} click at {args.x},{args.y} on {args.display}")
    return 0


# --------------------------------------------------------------------------------------------
# capture
# --------------------------------------------------------------------------------------------


def cmd_shot(args):
    """Screenshot via ImageMagick, then VERIFY THE CONTENT.

    `import` exits 0 on plenty of degenerate outcomes. A capture is only useful if it has pixels
    in it, so this decodes the PNG and reports the distinct-colour count; a single-colour image is
    reported as SUSPECT rather than passed off as a screenshot.
    """
    target = ["-window", args.window] if args.window else ["-window", "root"]
    cmd = ["/usr/bin/import", "-display", args.display] + target + ["-silent", args.out]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"uxrig: import failed ({r.returncode}): {r.stderr.strip()}")
    if not os.path.exists(args.out):
        sys.exit(f"uxrig: import exited 0 but wrote no file at {args.out}")
    size = os.path.getsize(args.out)
    from PIL import Image
    import numpy as np

    im = Image.open(args.out).convert("RGB")
    a = np.asarray(im).reshape(-1, 3)
    distinct = len(np.unique(a, axis=0))
    verdict = "OK" if distinct > 1 else "SUSPECT (single flat colour — nothing was drawn)"
    print(f"{args.out}: {im.width}x{im.height}, {size} bytes, {distinct} distinct colours — {verdict}")
    return 0 if distinct > 1 else 4


# --------------------------------------------------------------------------------------------
# structural isolation proof
# --------------------------------------------------------------------------------------------


def cmd_procproof(args):
    """Prove a RUNNING process cannot reach the compositor.

    This is the load-bearing half of the display proof and it is stronger than any window
    enumeration, because it does not depend on catching a window at the right moment.

    On 2026-08-29 this repo set DISPLAY=:91, launched, and the window opened on the owner's real
    Wayland desktop: the log said `Wayland window` while python-xlib found ZERO windows on the
    Xvfb. An X-only absence check on :0 would have returned a clean empty answer during exactly
    that incident. So the question is not "is it absent from :0" but "can this process reach a
    Wayland compositor at all".

    Environment-as-launched and environment-as-running are different claims; /proc gives the
    second one.
    """
    pid = args.pid
    ok = True

    # ---- half 1: the environment AS RUNNING -------------------------------------------------
    env_raw = open(f"/proc/{pid}/environ", "rb").read()
    env = dict(kv.split(b"=", 1) for kv in env_raw.split(b"\0") if b"=" in kv)
    print(f"--- /proc/{pid}/environ (environment AS RUNNING, not as launched) ---")
    for k in (b"WAYLAND_DISPLAY", b"XDG_SESSION_TYPE"):
        present = k in env
        print(f"  {k.decode():18} "
              f"{'PRESENT ' + env[k].decode() + '  <- FORBIDDEN' if present else 'ABSENT  <- required'}")
        if present:
            ok = False
    for k in (b"DISPLAY", b"WINIT_UNIX_BACKEND", b"XDG_RUNTIME_DIR", b"XDG_DATA_HOME",
              b"ORACLE_SOCKET", b"EXODUS_SOCKET"):
        print(f"  {k.decode():18} {env.get(k, b'(unset)').decode()}")

    # ---- half 2: the sockets it actually holds -----------------------------------------------
    #
    # An X *client* socket carries no path of its own -- the path is on the listening (server)
    # side, so `ss` shows our fd as `* <inode> * <peer-inode>`. Grepping our own pid's lines for
    # "/tmp/.X11-unix" therefore finds nothing even when the connection is real (it did, on the
    # first version of this check). The connection must be resolved through its PEER inode.
    #
    # That turns out to be the stronger proof anyway: the peer is held by a process, and that
    # process can be compared against the Xvfb pid THIS RIG RECORDED AT SPAWN. It binds the
    # window process to our own display by kernel state, not by a string in a log.
    fddir = f"/proc/{pid}/fd"
    our_inodes = set()
    for fd in os.listdir(fddir):
        try:
            t = os.readlink(os.path.join(fddir, fd))
        except OSError:
            continue
        if t.startswith("socket:["):
            our_inodes.add(t[8:-1])

    r = subprocess.run(["ss", "-x", "-a", "-p"], capture_output=True, text=True)
    by_inode = {}   # local inode -> (path, peer_inode, users)
    for line in r.stdout.splitlines():
        f = line.split()
        if len(f) < 7 or not f[0].startswith("u_str"):
            continue
        # ... <path> <inode> <peer-path> <peer-inode> [users:(...)]
        try:
            path, inode, _peer_path, peer_inode = f[4], f[5], f[6], f[7]
        except IndexError:
            continue
        users = line.split("users:", 1)[1] if "users:" in line else ""
        by_inode[inode] = (path, peer_inode, users)

    print(f"--- /proc/{pid}/fd unix sockets, resolved through their PEER ---")
    x_peers, way_peers = [], []
    for ino in sorted(our_inodes):
        rec = by_inode.get(ino)
        if not rec:
            print(f"  inode {ino}: not a unix socket (netlink/other)")
            continue
        path, peer_ino, users = rec
        prec = by_inode.get(peer_ino)
        ppath, pusers = (prec[0], prec[2]) if prec else ("(none)", "")
        kind = "LISTEN(ours)" if path != "*" else "client"
        print(f"  inode {ino:>12} {kind:12} local={path}")
        print(f"       peer inode {peer_ino:>12} path={ppath} held by{pusers.strip()}")
        # ⚑ Classify on the socket PATH, never on the process name in `users:`.
        # A first version matched the substring "wayland" anywhere in the line and so flagged
        # `@/tmp/.X11-unix/X0 held by (("Xwayland",...))` as a compositor connection. Xwayland is
        # an X SERVER; a client talking to it is talking X11, not Wayland. Matching the name would
        # make this check cry wolf on the one machine it has to be trusted on.
        #
        # A compositor socket is `$XDG_RUNTIME_DIR/wayland-<N>` (and its abstract twin), so the
        # test is on the peer path's basename.
        base = os.path.basename(ppath.lstrip("@"))
        if base.startswith("wayland-") and ".X11-unix" not in ppath:
            way_peers.append(f"{ppath} {pusers}")
        if ".X11-unix" in ppath:
            x_peers.append((ppath, pusers))

    # ---- verdict ------------------------------------------------------------------------------
    disp = env.get(b"DISPLAY", b"").decode()
    want = f".X11-unix/X{disp.lstrip(':')}"
    on_our_display = [p for p in x_peers if want in p[0]]
    print("--- verdict ---")
    print(f"  connected to an X server on its own DISPLAY ({disp}, socket *{want}): "
          f"{'YES' if on_our_display else 'NO  <- required'}")
    if not on_our_display:
        ok = False
    if args.xvfb_pid is not None:
        matched = [p for p in on_our_display if f"pid={args.xvfb_pid}," in p[1]]
        print(f"  and that X server is THE Xvfb THIS RIG SPAWNED (pid {args.xvfb_pid}): "
              f"{'YES' if matched else 'NO  <- required'}")
        if not matched:
            ok = False
    print(f"  holds ANY socket to a Wayland compositor: "
          f"{'YES  <- FORBIDDEN' if way_peers else 'NO'}")
    if way_peers:
        ok = False
    wl = "/run/user/1000/wayland-0"
    print(f"  (the compositor socket that must not appear above: {wl}; "
          f"it exists on this box: {os.path.exists(wl)}, so its absence here is a real absence "
          f"and not a missing target)")
    print(f"  STRUCTURAL ISOLATION: {'PROVEN' if ok else 'NOT PROVEN'}")
    return 0 if ok else 5


# --------------------------------------------------------------------------------------------


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    def add(name, fn, *extra):
        s = sub.add_parser(name)
        # No default. An omitted display must be an error, never ":0".
        s.add_argument("--display", required=True, help="X display, e.g. :91 (REQUIRED, no default)")
        for a, kw in extra:
            s.add_argument(a, **kw)
        s.set_defaults(fn=fn)
        return s

    add("windows", cmd_windows, ("--pid", dict(type=int, default=None)))
    add("move", cmd_move, ("--x", dict(type=int, required=True)), ("--y", dict(type=int, required=True)))
    add("click", cmd_click, ("--x", dict(type=int, required=True)), ("--y", dict(type=int, required=True)),
        ("--button", dict(type=int, default=1)))
    add("key", cmd_key, ("--key", dict(required=True)))
    add("type", cmd_type, ("--text", dict(required=True)))
    add("shot", cmd_shot, ("--out", dict(required=True)), ("--window", dict(default=None)))
    add("procproof", cmd_procproof, ("--pid", dict(type=int, required=True)),
        ("--xvfb-pid", dict(type=int, default=None,
                            help="the Xvfb pid this rig recorded at spawn; when given, the check "
                                 "also requires that the process is connected to THAT server")))

    args = p.parse_args()
    sys.exit(args.fn(args))


if __name__ == "__main__":
    main()
