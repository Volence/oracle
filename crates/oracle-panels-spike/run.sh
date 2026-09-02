#!/usr/bin/env bash
# Runner for the throwaway toolkit spike. See src/main.rs for what each mode measures and why.
#
# THE INSTRUMENT RULE, ENCODED HERE: `eframe` mode must never draw on a real compositor. This script
# creates its OWN Xvfb at a geometry no real monitor has (1281x803), unsets WAYLAND_DISPLAY and
# XDG_SESSION_TYPE so nothing can fall back to the session, and passes that geometry to the binary as
# --expect-screen. The binary asks the toolkit for its screen size on the first frame and exits(2) without
# drawing if the two disagree. Both halves are required: the env alone would fail silently.
#
#   ./run.sh screens                 # print :0 and :77 geometry, to see they are different displays
#   ./run.sh cpu   <secs>            # display-independent CPU pass (no window, no GPU)
#   ./run.sh unpaced <secs>          # same pipeline, deadline removed -> headroom
#   ./run.sh eframe <secs>           # the real winit+wgpu stack, on Xvfb (llvmpipe: fps NOT the answer)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$HERE/target/release/oracle-panels-spike"
ROM="${ORACLE_SPIKE_ROM:-/home/volence/sonic_hacks/aeon/s4.debug.bin}"
DISP="${ORACLE_SPIKE_DISPLAY:-:77}"
GEOM_W=1281
GEOM_H=803

headless() { env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE DISPLAY="$DISP" "$@"; }

# Bring up the private Xvfb if it is not already there, and remember whether WE started it so that only
# our own server is torn down. `-nolisten tcp` keeps it off the network; the geometry is deliberately one
# no real monitor has, so the binary's --expect-screen check is a real discriminator and not a coincidence.
XVFB_PID=""
ensure_xvfb() {
  if headless xrandr >/dev/null 2>&1; then
    echo "run.sh: reusing an existing X server on $DISP" >&2
    return
  fi
  Xvfb "$DISP" -screen 0 "${GEOM_W}x${GEOM_H}x24" -nolisten tcp &
  XVFB_PID=$!
  echo "run.sh: started Xvfb on $DISP at ${GEOM_W}x${GEOM_H} (pid $XVFB_PID)" >&2
  for _ in $(seq 1 50); do
    headless xrandr >/dev/null 2>&1 && return
    sleep 0.1
  done
  echo "run.sh: Xvfb on $DISP did not come up" >&2
  exit 1
}
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
    "$BIN" --rom "$ROM" --mode cpu --secs "${2:-60}" --audio "${3:-on}"
    ;;
  unpaced)
    "$BIN" --rom "$ROM" --mode cpu-unpaced --secs "${2:-20}" --audio "${3:-off}"
    ;;
  eframe)
    ensure_xvfb
    headless "$BIN" --rom "$ROM" --mode eframe --secs "${2:-60}" --audio "${3:-on}" \
      --expect-screen "${GEOM_W}x${GEOM_H}"
    ;;
  *)
    echo "usage: $0 {screens|cpu|unpaced|eframe} [secs] [on|off]" >&2
    exit 64
    ;;
esac
