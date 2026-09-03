#!/usr/bin/env bash
# The pacing measurement runner for `oracle-player`.
#
# THE INSTRUMENT RULE, ENCODED HERE. This machine's owner is using it, so no bench may draw on his
# compositor. This script creates its OWN Xvfb at a geometry no real monitor has (1281x803), unsets
# WAYLAND_DISPLAY and XDG_SESSION_TYPE so nothing can fall back to the session, and passes that geometry to
# the binary as --expect-screen. The binary asks the TOOLKIT for its screen size on the first frame and
# exits(2) without drawing if the two disagree. Both halves are required: the environment alone fails
# silently, and the binary refuses to run a bench mode without --expect-screen at all.
#
# Both bench modes force audio gain 0.0, which multiplies on the producer side — the ring dynamics, the
# feedback loop and every underrun count are the genuine ones; only the amplitude is zero.
#
#   ./run-bench.sh screens              # print :0 and :77 geometry, to see they are different displays
#   ./run-bench.sh cpu    <secs>        # THE ANSWER: governor-paced, display-independent
#   ./run-bench.sh window <secs>        # the real winit+wgpu stack on Xvfb (llvmpipe: fps is REFUSED)
#
# Run these on a QUIET machine. Nothing else may be building: a cargo job on the other cores is what turned
# the toolkit spike's frame rate into 92.87 and then 22.71.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
BIN="${ORACLE_PLAYER_BIN:-$ROOT/target/release/oracle-player}"
ROM="${ORACLE_PLAYER_ROM:-/home/volence/sonic_hacks/aeon/s4.debug.bin}"
DISP="${ORACLE_PLAYER_DISPLAY:-:77}"
GEOM_W=1281
GEOM_H=803

headless() { env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE DISPLAY="$DISP" "$@"; }

XVFB_PID=""
ensure_xvfb() {
  if headless xrandr >/dev/null 2>&1; then
    echo "run-bench.sh: reusing an existing X server on $DISP" >&2
    return
  fi
  Xvfb "$DISP" -screen 0 "${GEOM_W}x${GEOM_H}x24" -nolisten tcp &
  XVFB_PID=$!
  echo "run-bench.sh: started Xvfb on $DISP at ${GEOM_W}x${GEOM_H} (pid $XVFB_PID)" >&2
  for _ in $(seq 1 50); do
    headless xrandr >/dev/null 2>&1 && return
    sleep 0.1
  done
  echo "run-bench.sh: Xvfb on $DISP did not come up" >&2
  exit 1
}
# Only ever tear down a server WE started.
cleanup() { [ -n "$XVFB_PID" ] && kill "$XVFB_PID" 2>/dev/null || true; }
trap cleanup EXIT

case "${1:-}" in
  screens)
    ensure_xvfb
    echo "--- the session's real display (:0) ---"
    env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE DISPLAY=:0 xrandr | head -3 || true
    echo "--- this run's private Xvfb ($DISP) ---"
    headless xrandr | head -3
    ;;
  cpu)
    # No window at all, so no display is touched — but --expect-screen is still passed, because the binary
    # refuses any bench mode without it and that refusal is the point.
    "$BIN" --rom "$ROM" --mode bench-cpu --secs "${2:-75}" --audio "${3:-on}" \
      --expect-screen "${GEOM_W}x${GEOM_H}"
    ;;
  window)
    ensure_xvfb
    headless "$BIN" --rom "$ROM" --mode bench-window --secs "${2:-75}" --audio "${3:-on}" \
      --expect-screen "${GEOM_W}x${GEOM_H}"
    ;;
  *)
    echo "usage: $0 {screens|cpu|window} [secs] [on|off]" >&2
    exit 64
    ;;
esac
