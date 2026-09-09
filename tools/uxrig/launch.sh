#!/usr/bin/env bash
# uxrig launcher — puts an Oracle window on a PRIVATE Xvfb display and nowhere else.
#
# WHY THIS EXISTS, AND WHY IT IS A HELPER AND NOT A CONVENTION
# -----------------------------------------------------------
# Every session on this machine inherits `WAYLAND_DISPLAY=wayland-0`. minifb (oracle-frontend) and
# winit (oracle-player) both PREFER Wayland whenever that variable is set, and both do so SILENTLY.
# On 2026-08-29 this repo set `DISPLAY=:91`, launched, and the window opened on the owner's real
# desktop anyway (docs/2026-08-29-window-runtime-checks.md). The failure has no error message; the
# only symptom is a window on somebody else's screen.
#
# So forcing the display is THIS SCRIPT'S job. A seat is never trusted to pass the right flags,
# because the cost of forgetting one is paid by the owner and not by the seat.
#
# The same reasoning applies to the Aether socket. The contract's default chain is
# `$ORACLE_SOCKET` -> `$EXODUS_SOCKET` -> `$XDG_RUNTIME_DIR/oracle.sock` -> `/tmp/oracle.sock`
# (crates/oracle-aether/src/server.rs:69-78). A bare `--aether` with no `--socket` walks that chain
# into a SHARED path: it either collides with the owner's live window or, if his is down, quietly
# BECOMES the server the next lane's client attaches to. This script therefore (a) refuses bare
# `--aether`, and (b) sets ORACLE_SOCKET/EXODUS_SOCKET/XDG_RUNTIME_DIR to private values anyway, so
# that no link in that chain can reach a shared last resort even if a flag were somehow missed.
# Every resolver terminates at a value set explicitly here.
#
# USAGE
#   launch.sh display-start [WxH]   start + own a private Xvfb, verify geometry FROM INSIDE it
#   launch.sh display-stop          kill only the Xvfb PID we recorded at spawn
#   launch.sh display-show          print the display, the Xvfb pid, and the scratch paths
#   launch.sh snapshot-rom ROM      copy a ROM (+ .lst) into scratch, immune to aeon rebuilds
#   launch.sh env -- CMD [ARGS...]  run any command in the scrubbed private environment
#   launch.sh frontend [ARGS...]    oracle-frontend --x11 --socket <private> <rom>
#   launch.sh player   [ARGS...]    oracle-player --socket <private> --rom <rom>
#
# Both launchers run in the FOREGROUND. A seat that wants one in the background backgrounds the
# whole `launch.sh` call itself and records the pid it gets; see the handover note.

set -euo pipefail

# ---------------------------------------------------------------------------------------------
# The tree. NAMED, never defaulted, never inferred from $PATH or $PWD.
#
# A prior run in this suite named a *binary* and silently measured a different checkout's build.
# TREE is resolved from this script's own location, and every binary is required to live under it.
# ---------------------------------------------------------------------------------------------
TREE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
RIG="$TREE/.uxrig"
BINDIR="$TREE/target/release"

DISPLAY_FILE="$RIG/display"
XVFB_PID_FILE="$RIG/xvfb.pid"
XDG_HOME="$RIG/xdg"          # XDG_CONFIG_HOME / XDG_DATA_HOME / XDG_CACHE_HOME live under here
RUNTIME_DIR="$RIG/run"       # our private XDG_RUNTIME_DIR
SOCK_DIR="$RIG/sock"         # our private Aether socket directory

die() { printf 'uxrig: REFUSED: %s\n' "$*" >&2; exit 3; }
note() { printf 'uxrig: %s\n' "$*" >&2; }

# ---------------------------------------------------------------------------------------------
# Geometry verification, FROM INSIDE the display.
#
# `xdpyinfo` is NOT installed on this machine (checked 2026-09-09), so the flags we passed to Xvfb
# are not evidence that Xvfb honoured them. python-xlib connects as a real client and reads the
# screen back, which is evidence.
# ---------------------------------------------------------------------------------------------
verify_geometry() {  # $1 = :NN
  python3 - "$1" <<'PY'
import sys
from Xlib import display as xd
d = xd.Display(sys.argv[1])
s = d.screen()
i = d.display.info
print("%dx%d depth=%d vendor=%r protocol=%d.%d screens=%d" % (
    s.width_in_pixels, s.height_in_pixels, s.root_depth,
    i.vendor, i.protocol_major, i.protocol_minor, len(i.roots)))
PY
}

display_in_use() {  # $1 = NN ; true if anything is already listening on that display
  [ -e "/tmp/.X11-unix/X$1" ] && return 0
  [ -e "/tmp/.X$1-lock" ] && return 0
  return 1
}

cmd_display_start() {
  local geom="${1:-1280x960}"
  mkdir -p "$RIG" "$XDG_HOME" "$RUNTIME_DIR" "$SOCK_DIR"
  chmod 700 "$RUNTIME_DIR" "$SOCK_DIR"

  if [ -f "$XVFB_PID_FILE" ] && kill -0 "$(cat "$XVFB_PID_FILE")" 2>/dev/null; then
    die "an Xvfb from this rig is already running (pid $(cat "$XVFB_PID_FILE"), display $(cat "$DISPLAY_FILE" 2>/dev/null)). Run display-stop first."
  fi

  # Probe for a free display number rather than hardcoding one: another lane on this machine may
  # already own :91. Skip :0 (the owner's Xwayland) unconditionally.
  local n found=""
  for n in $(seq 90 119); do
    if ! display_in_use "$n"; then found="$n"; break; fi
  done
  [ -n "$found" ] || die "no free X display number in :90..:119"

  Xvfb ":$found" -screen 0 "${geom}x24" -nolisten tcp >"$RIG/xvfb.log" 2>&1 &
  local pid=$!
  # Wait for the socket to appear rather than sleeping a guess.
  local i
  for i in $(seq 1 100); do
    [ -e "/tmp/.X11-unix/X$found" ] && break
    kill -0 "$pid" 2>/dev/null || die "Xvfb :$found died at startup; see $RIG/xvfb.log"
    sleep 0.1
  done
  [ -e "/tmp/.X11-unix/X$found" ] || die "Xvfb :$found never created its socket; see $RIG/xvfb.log"

  # The pid file is written FIRST and unconditionally, so that a display which then fails
  # verification is still one we can clean up by recorded pid rather than orphaning.
  printf '%s\n' "$pid" > "$XVFB_PID_FILE"

  # ⚑ Verify BEFORE publishing $DISPLAY_FILE. `require_display` gates every launch on that file, so
  # an unverified display must never be able to satisfy the gate. (This ordering is not cosmetic:
  # the first run of this script wrote both files and *then* failed verification, which left a
  # display the gate would have accepted and nobody had measured.)
  local seen
  if ! seen="$(verify_geometry ":$found" 2>&1)"; then
    kill "$pid" 2>/dev/null || true
    rm -f "$XVFB_PID_FILE"
    die "could not read the screen back from :$found — the display is UNMEASURABLE, which is a failure and not a pass. Xvfb killed. Detail: $seen"
  fi
  printf '%s\n' ":$found" > "$DISPLAY_FILE"
  note "display :$found pid $pid  screen(read back from inside): $seen"
  printf ':%s\n' "$found"
}

cmd_display_stop() {
  [ -f "$XVFB_PID_FILE" ] || die "no recorded Xvfb pid; nothing this rig started is known to be running"
  local pid; pid="$(cat "$XVFB_PID_FILE")"
  # Kill ONLY the pid we recorded at spawn. Never pkill/killall by name: the owner's session and
  # other lanes share this machine, and a name-match has bitten two lanes here already.
  if kill -0 "$pid" 2>/dev/null; then
    kill "$pid" && note "killed Xvfb pid $pid"
  else
    note "recorded Xvfb pid $pid is not running"
  fi
  rm -f "$XVFB_PID_FILE" "$DISPLAY_FILE"
}

# The one gate every launch passes through. Refuses rather than falling back.
require_display() {
  [ -f "$DISPLAY_FILE" ] || die "no private display: $DISPLAY_FILE does not exist. Run 'launch.sh display-start' first. This rig will NOT fall back to \$DISPLAY, which is the owner's real desktop."
  UXRIG_DISPLAY="$(cat "$DISPLAY_FILE")"
  [ -n "$UXRIG_DISPLAY" ] || die "$DISPLAY_FILE is empty"
  [ -f "$XVFB_PID_FILE" ] || die "display $UXRIG_DISPLAY is recorded but no Xvfb pid file exists — this rig will not drive a display it does not own"
  local pid; pid="$(cat "$XVFB_PID_FILE")"
  kill -0 "$pid" 2>/dev/null || die "the Xvfb we recorded for $UXRIG_DISPLAY (pid $pid) is not running — refusing to launch onto a display this rig does not own"
  [ -e "/tmp/.X11-unix/X${UXRIG_DISPLAY#:}" ] || die "$UXRIG_DISPLAY has no X socket"
}

# Resolve a binary strictly under the named tree. Never $PATH: /home/volence/sonic_hacks/oracle
# (the MAIN checkout) already holds older binaries with the same names, and reaching one of those
# would mean the rig measured a build nobody named.
resolve_bin() {  # $1 = binary name -> echoes absolute path
  local p="$BINDIR/$1"
  [ -x "$p" ] || die "$p is not an executable — build it in THIS tree ($TREE). This rig will not fall back to \$PATH or to another checkout's target/."
  local real; real="$(readlink -f "$p")"
  case "$real" in
    "$TREE"/*) ;;
    *) die "$p resolves to $real, which is outside the named tree $TREE" ;;
  esac
  printf '%s\n' "$real"
}

# ⚑ The ROM is a BUILD ARTIFACT OF A TREE SOMEBODY ELSE IS ACTIVELY REBUILDING.
#
# `/home/volence/sonic_hacks/aeon/s4.debug.bin` is produced by aeon's `./build.sh`, and that build
# DELETES the file before it rewrites it. Measured 2026-09-09T07:29Z: both windows launched
# cleanly at 07:18 and 07:22, and a rehearsal at 07:29 had both exit instantly with
#
#     cannot read ROM /home/volence/sonic_hacks/aeon/s4.debug.bin: No such file or directory
#
# because aeon's build.sh (pid 2770425) was mid-run in another lane. Nothing in this rig writes to
# the aeon tree; the file genuinely vanished underneath us.
#
# Without this preflight the failure reaches a seat as two processes that die a second after
# launch, an empty window list, and a stack trace from procproof about a pid that no longer
# exists -- none of which names the cause. Checked here so the message says what happened.
preflight_rom() {
  local a prev="" rom=""
  # The ROM is the one non-flag argument for the frontend, and the value of --rom for the player.
  for a in "$@"; do
    case "$prev" in --rom) rom="$a" ;; esac
    case "$a" in --*) ;; *) [ "${prev#--}" = "$prev" ] && rom="${rom:-$a}" ;; esac
    prev="$a"
  done
  [ -n "$rom" ] || return 0
  if [ ! -e "$rom" ]; then
    die "ROM $rom does not exist. It is a BUILD ARTIFACT of the aeon tree, which another lane may be rebuilding right now. Check with: ls -l $rom; pgrep -af 'build.sh'. Wait for the build to finish, or snapshot a ROM first: tools/uxrig/launch.sh snapshot-rom $rom"
  fi
  [ -r "$rom" ] || die "ROM $rom exists but is not readable"
  [ -s "$rom" ] || die "ROM $rom is empty (0 bytes) — almost certainly a build in progress"
}

# Copy a ROM (and its .lst, if present) into rig scratch so a concurrent aeon rebuild cannot pull
# it out from under a run in progress. Reads only; never writes to the aeon tree.
cmd_snapshot_rom() {
  local src="${1:-}"
  [ -n "$src" ] || die "usage: launch.sh snapshot-rom <rom path>"
  [ -s "$src" ] || die "$src is missing or empty — nothing to snapshot"
  mkdir -p "$RIG/rom"
  cp -- "$src" "$RIG/rom/"
  note "snapshotted $(basename "$src") ($(stat -c %s "$src") bytes) -> $RIG/rom/"
  local lst="${src%.bin}.lst"
  if [ -s "$lst" ]; then
    cp -- "$lst" "$RIG/rom/"
    note "snapshotted $(basename "$lst") -> $RIG/rom/"
  fi
  printf '%s\n' "$RIG/rom/$(basename "$src")"
}

# Reject the one flag combination that can reach a shared socket.
check_socket_flags() {
  local a saw_aether=0 saw_socket=0
  for a in "$@"; do
    case "$a" in
      --aether) saw_aether=1 ;;
      --socket) saw_socket=1 ;;
      --socket=*) saw_socket=1 ;;
    esac
  done
  if [ "$saw_aether" = 1 ] && [ "$saw_socket" = 0 ]; then
    die "bare --aether with no --socket. That resolves the CONTRACT DEFAULT chain (\$ORACLE_SOCKET -> \$EXODUS_SOCKET -> \$XDG_RUNTIME_DIR/oracle.sock -> /tmp/oracle.sock), which is the path the owner's live window holds. Pass --socket <path under $SOCK_DIR>, or pass neither and let this helper supply one."
  fi
  # A caller-supplied --socket must still be private to this rig.
  local prev=""
  for a in "$@"; do
    case "$prev" in --socket) case "$a" in
        "$SOCK_DIR"/*) ;;
        *) die "--socket $a is not under this rig's private socket dir $SOCK_DIR" ;;
      esac ;;
    esac
    case "$a" in --socket=*) case "${a#--socket=}" in
        "$SOCK_DIR"/*) ;;
        *) die "--socket ${a#--socket=} is not under this rig's private socket dir $SOCK_DIR" ;;
      esac ;;
    esac
    prev="$a"
  done
}

# The scrubbed environment. `env -u` REMOVES the two variables that make a toolkit choose Wayland;
# WINIT_UNIX_BACKEND=x11 is what forces winit (oracle-player has no --x11 flag of its own).
#
# ORACLE_SOCKET / EXODUS_SOCKET are set to a private path even though every launch also passes
# --socket, so that the default chain has no shared link left to fall through to.
run_in_rig_env() {  # $@ = command
  mkdir -p "$XDG_HOME/config" "$XDG_HOME/data" "$XDG_HOME/cache" "$RUNTIME_DIR" "$SOCK_DIR"
  chmod 700 "$RUNTIME_DIR" "$SOCK_DIR"
  exec env \
    -u WAYLAND_DISPLAY \
    -u XDG_SESSION_TYPE \
    -u ORACLE_AETHER \
    DISPLAY="$UXRIG_DISPLAY" \
    WINIT_UNIX_BACKEND=x11 \
    XDG_CONFIG_HOME="$XDG_HOME/config" \
    XDG_DATA_HOME="$XDG_HOME/data" \
    XDG_CACHE_HOME="$XDG_HOME/cache" \
    XDG_RUNTIME_DIR="$RUNTIME_DIR" \
    ORACLE_SOCKET="$SOCK_DIR/oracle.sock" \
    EXODUS_SOCKET="$SOCK_DIR/oracle.sock" \
    "$@"
}

cmd_env() {
  require_display
  [ "${1:-}" = "--" ] || die "usage: launch.sh env -- CMD [ARGS...]"
  shift
  [ $# -gt 0 ] || die "usage: launch.sh env -- CMD [ARGS...]"
  run_in_rig_env "$@"
}

cmd_frontend() {
  require_display
  check_socket_flags "$@"
  preflight_rom "$@"
  local bin; bin="$(resolve_bin oracle-frontend)"
  local extra=()
  # --x11 is added by the helper, always. Belt and braces with `env -u WAYLAND_DISPLAY`, because
  # the failure this guards is silent and lands on the owner's screen.
  case " $* " in *" --x11 "*) ;; *) extra+=(--x11) ;; esac
  case " $* " in *" --socket "*|*" --socket="*) ;; *) extra+=(--socket "$SOCK_DIR/frontend.sock") ;; esac
  note "exec $bin ${extra[*]} $* (DISPLAY=$UXRIG_DISPLAY)"
  run_in_rig_env "$bin" "${extra[@]}" "$@"
}

cmd_player() {
  require_display
  check_socket_flags "$@"
  preflight_rom "$@"
  local bin; bin="$(resolve_bin oracle-player)"
  local extra=()
  case " $* " in *" --socket "*|*" --socket="*) ;; *) extra+=(--socket "$SOCK_DIR/player.sock") ;; esac
  note "exec $bin ${extra[*]} $* (DISPLAY=$UXRIG_DISPLAY, WINIT_UNIX_BACKEND=x11)"
  run_in_rig_env "$bin" "${extra[@]}" "$@"
}

cmd_display_show() {
  printf 'tree           %s\n' "$TREE"
  printf 'bindir         %s\n' "$BINDIR"
  printf 'display        %s\n' "$(cat "$DISPLAY_FILE" 2>/dev/null || echo '(none)')"
  printf 'xvfb pid       %s\n' "$(cat "$XVFB_PID_FILE" 2>/dev/null || echo '(none)')"
  printf 'XDG_CONFIG_HOME %s\n' "$XDG_HOME/config"
  printf 'XDG_DATA_HOME  %s\n' "$XDG_HOME/data"
  printf 'XDG_RUNTIME_DIR %s\n' "$RUNTIME_DIR"
  printf 'socket dir     %s\n' "$SOCK_DIR"
}

case "${1:-}" in
  display-start) shift; cmd_display_start "$@" ;;
  display-stop)  shift; cmd_display_stop "$@" ;;
  display-show)  shift; cmd_display_show "$@" ;;
  snapshot-rom)  shift; cmd_snapshot_rom "$@" ;;
  env)           shift; cmd_env "$@" ;;
  frontend)      shift; cmd_frontend "$@" ;;
  player)        shift; cmd_player "$@" ;;
  *) sed -n '/^# USAGE/,/^# whole/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//' >&2; exit 2 ;;
esac
