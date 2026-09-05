#!/usr/bin/env bash
# Installs the Oracle icon + .desktop entries for the current user, so the desktop (KDE/GNOME/…) shows the
# Oracle mark on both windows and in launchers. Per-user only; touches nothing outside ~/.local.
#
#   usage: install-desktop.sh [PATH-TO-oracle-frontend-BINARY] [PATH-TO-oracle-player-BINARY]
#
# The binary paths default to the release builds in this checkout. Re-run after moving a binary. A missing
# binary is skipped with a note rather than being an error: the two windows ship independently, and
# refusing to install either entry because one is unbuilt would be the wrong trade.
#
# ⚑ WHY TWO ENTRIES. The window icon can be set from inside the process on X11 and NOT on Wayland — there
# is no Wayland protocol for a per-window icon that either toolkit here speaks. On Wayland the compositor
# takes the window's app id and looks for a `.desktop` whose `StartupWMClass` (or file name) matches, and
# the icon comes from there. The two windows report two different classes — `oracle-frontend` (minifb sets
# no app id at all; KWin falls back to the executable name) and `oracle-player` (eframe sends
# `ViewportBuilder::app_id`) — so one entry cannot cover both, and the one that is missing is the window
# that shows a blank or generic icon.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
bin="${1:-$root/target/release/oracle-frontend}"
player="${2:-$root/target/release/oracle-player}"
if [ ! -x "$bin" ] && [ ! -x "$player" ]; then
  echo "no executable at $bin or $player — build one (cargo build --release -p oracle-frontend -p oracle-player) or pass its path" >&2
  exit 1
fi
data="${XDG_DATA_HOME:-$HOME/.local/share}"

for s in 32 64 128 256; do
  d="$data/icons/hicolor/${s}x${s}/apps"
  mkdir -p "$d"
  cp "$here/oracle-$s.png" "$d/oracle.png"
done
# The tinted vector, for themes that prefer scalable.
mkdir -p "$data/icons/hicolor/scalable/apps"
cp "$here/oracle-icon.svg" "$data/icons/hicolor/scalable/apps/oracle.svg"

mkdir -p "$data/applications"
installed=""
# The entry file name matches the app id it serves, because a compositor that finds no `StartupWMClass`
# match falls back to `<app-id>.desktop` — belt and braces for one `sed`.
if [ -x "$bin" ]; then
  sed "s|^Exec=.*|Exec=$bin %f|" "$here/oracle.desktop" > "$data/applications/oracle-frontend.desktop"
  # The historical file name, kept so an install made before this script grew a second entry is replaced
  # rather than left behind pointing at a stale path.
  cp "$data/applications/oracle-frontend.desktop" "$data/applications/oracle.desktop"
  installed="$installed oracle-frontend.desktop(Exec=$bin)"
else
  echo "note: no oracle-frontend at $bin — skipping its entry" >&2
fi
if [ -x "$player" ]; then
  sed "s|^Exec=.*|Exec=$player %f|" "$here/oracle-player.desktop" > "$data/applications/oracle-player.desktop"
  installed="$installed oracle-player.desktop(Exec=$player)"
else
  echo "note: no oracle-player at $player — skipping its entry" >&2
fi

command -v update-desktop-database >/dev/null && update-desktop-database "$data/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$data/icons/hicolor" 2>/dev/null || true
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 --noincremental >/dev/null 2>&1 || true

echo "installed under $data/applications:$installed"
echo "icons under $data/icons/hicolor/*/apps/oracle.*"
echo "A window that is already open keeps its old icon; relaunch it."
