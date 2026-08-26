#!/usr/bin/env bash
# Installs the Oracle icon + .desktop entry for the current user, so the desktop (KDE/GNOME/…) shows the
# Oracle mark on the player's window and in launchers. Per-user only; touches nothing outside ~/.local.
#
#   usage: install-desktop.sh [PATH-TO-oracle-frontend-BINARY]
#
# The binary path defaults to the release build in this checkout. Re-run after moving the binary.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
bin="${1:-$(cd "$here/../../.." && pwd)/target/release/oracle-frontend}"
if [ ! -x "$bin" ]; then
  echo "no executable at $bin — build it (cargo build --release -p oracle-frontend) or pass its path" >&2
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
sed "s|^Exec=.*|Exec=$bin %f|" "$here/oracle.desktop" > "$data/applications/oracle.desktop"

command -v update-desktop-database >/dev/null && update-desktop-database "$data/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$data/icons/hicolor" 2>/dev/null || true
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 --noincremental >/dev/null 2>&1 || true

echo "installed: $data/applications/oracle.desktop (Exec=$bin), icons under $data/icons/hicolor/*/apps/oracle.*"
echo "A window that is already open keeps its old icon; relaunch the player."
