#!/usr/bin/env bash
# Installs the Oracle icon + .desktop entries for the current user, so the desktop (KDE/GNOME/...) shows the
# Oracle mark on both windows and in launchers. Per-user only; touches nothing outside ~/.local.
#
#   usage: install-desktop.sh [--dry-run] [PATH-TO-oracle-frontend-BINARY] [PATH-TO-oracle-player-BINARY]
#
# The binary paths default to the release builds in this checkout. Re-run after moving a binary. A missing
# binary is skipped with a note rather than being an error: the two windows ship independently, and
# refusing to install either entry because one is unbuilt would be the wrong trade.
#
# `--dry-run` decides everything the normal run decides and writes nothing, printing the exact list of
# files it would create. A normal run prints the same list of what it did create. The decision is taken
# once and both modes consume it, so what the dry run promises is what the real run does.
#
# WHY TWO ENTRIES. The window icon can be set from inside the process on X11 and NOT on Wayland: there
# is no Wayland protocol for a per-window icon that either toolkit here speaks. On Wayland the compositor
# takes the window's app id and looks for a `.desktop` whose `StartupWMClass` (or file name) matches, and
# the icon comes from there. The two windows report two different classes, `oracle-frontend` (minifb sets
# no app id at all; KWin falls back to the executable name) and `oracle-player` (eframe sends
# `ViewportBuilder::app_id`), so one entry cannot cover both, and the one that is missing is the window
# that shows a blank or generic icon.
#
# ONE CLASS, ONE ENTRY, AND WHY THIS SCRIPT SKIPS RATHER THAN COMPETES.
#
# A `StartupWMClass` is how the desktop decides which launcher a running window belongs to. Two entries
# declaring the same class are two answers to a question with one answer, and the desktop picks one of
# them: the window's icon, its name and where it lands in a task switcher all become a coin toss. This
# script used to install exactly that, twice over. It wrote `oracle-frontend.desktop` and then copied it
# to `oracle.desktop`, so its own two entries collided with each other before anyone else's file was
# considered, and it also overwrote nothing and warned about nothing when the user had made an entry of
# their own for the same window.
#
# So, before writing an entry, it looks for a `.desktop` that already claims that class and is not one of
# this script's own names. If it finds one it installs nothing for that class, names the file it found,
# and says why. Three alternatives were considered and are worse:
#
#   * ADOPTING the conflicting entry means rewriting a file the user wrote by hand. It is theirs. A tool
#     that edits your home directory because it disagreed with you is not a tool you leave installed.
#   * CHOOSING A DIFFERENT CLASS is not available. ⚑ **The class is what the BINARY reports** (`icon.rs`'s
#     `WM_CLASS` for the frontend, `ViewportBuilder::app_id` from `ui::APP_NAME` for the player), not a
#     value the entry may pick. An entry declaring a class no window ever reports matches nothing, which
#     is worse than a duplicate: it looks installed and does nothing. Do not "fix" a conflict here by
#     editing a `StartupWMClass=` line.
#   * INSTALLING ANYWAY and letting the desktop sort it out is the defect this paragraph exists to
#     describe.
#
# `oracle.desktop` UNDER `applications/` IS NO LONGER WRITTEN. It was the historical file name, kept so
# that an install made before this script grew a second entry would be replaced rather than left pointing
# at a stale path. That intent is served by the current entry existing; maintaining a second copy of the
# same entry under a second name is the self-collision above, and on a machine that never had the old
# install it was pure duplication of a file nothing was migrating. If one is found, it is reported, with
# the command to remove it. This script does not delete files in a home directory it does not own.
set -euo pipefail

dry=0
args=()
for a in "$@"; do
  case "$a" in
    --dry-run) dry=1 ;;
    -h|--help) command sed -n '2,52p' "$0"; exit 0 ;;
    -*) echo "install-desktop: unknown option '$a' (see --help)" >&2; exit 2 ;;
    *) args+=("$a") ;;
  esac
done
set -- ${args[@]+"${args[@]}"}

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
bin="${1:-$root/target/release/oracle-frontend}"
player="${2:-$root/target/release/oracle-player}"
if [ ! -x "$bin" ] && [ ! -x "$player" ]; then
  echo "no executable at $bin or $player: build one (cargo build --release -p oracle-frontend -p oracle-player) or pass its path" >&2
  exit 1
fi
data="${XDG_DATA_HOME:-$HOME/.local/share}"
apps="$data/applications"

# The entry names this script owns. A file it wrote itself is not a competitor for a class: it is the
# previous run's copy of the entry being written now, and treating it as one would make the script unable
# to refresh its own install.
managed=" oracle-frontend.desktop oracle-player.desktop oracle.desktop "

# The class a `.desktop` declares, read from the file rather than restated here. The templates document
# themselves as tracking the binary; a second copy of the string in this script would be the thing that
# goes stale.
class_of() { awk '/^StartupWMClass=/ { sub(/^StartupWMClass=/, ""); print; exit }' "$1"; }

# The first entry under $apps, other than one of ours, that already claims class $1. Empty when none does.
claimed_by() {
  local want="$1" f base
  [ -d "$apps" ] || return 0
  for f in "$apps"/*.desktop; do
    [ -e "$f" ] || continue
    base="${f##*/}"
    case "$managed" in *" $base "*) continue ;; esac
    if [ "$(class_of "$f")" = "$want" ]; then
      printf '%s\n' "$base"
      return 0
    fi
  done
  return 0
}

# ---------------------------------------------------------------------------------------------------
# Decide first, act second. Both `--dry-run` and the real run consume exactly these two lists, so the
# dry run is a report on the decision rather than a second implementation of it.
# ---------------------------------------------------------------------------------------------------
planned=()   # "target|template|exec"
skipped=()   # "target|class|conflicting file"

plan_entry() {
  local tmpl="$1" target="$2" exe="$3" cls other
  cls="$(class_of "$tmpl")"
  # An empty class would make the scan below match every entry that declares none, which is most of
  # them: an unguarded "" is a wildcard, not an absence. A template with no class is a template bug, and
  # the entry is still worth installing for its launcher and its icon.
  if [ -z "$cls" ]; then
    echo "note: $tmpl declares no StartupWMClass, so it can claim nothing and nothing can conflict with it" >&2
    planned+=("$target|$tmpl|$exe")
    return 0
  fi
  other="$(claimed_by "$cls")"
  if [ -n "$other" ]; then
    skipped+=("$target|$cls|$other")
  else
    planned+=("$target|$tmpl|$exe")
  fi
}

if [ -x "$bin" ]; then
  plan_entry "$here/oracle.desktop" oracle-frontend.desktop "$bin"
else
  echo "note: no oracle-frontend at $bin, skipping its entry" >&2
fi
if [ -x "$player" ]; then
  plan_entry "$here/oracle-player.desktop" oracle-player.desktop "$player"
else
  echo "note: no oracle-player at $player, skipping its entry" >&2
fi

icons=()
for s in 32 64 128 256; do
  icons+=("$data/icons/hicolor/${s}x${s}/apps/oracle.png")
done
icons+=("$data/icons/hicolor/scalable/apps/oracle.svg")

report_skips() {
  local row target cls other
  for row in ${skipped[@]+"${skipped[@]}"}; do
    target="${row%%|*}"; row="${row#*|}"; cls="${row%%|*}"; other="${row##*|}"
    echo "SKIPPED $target: $apps/$other already declares StartupWMClass=$cls."
    echo "        Two entries with one class leave the desktop guessing which launcher a window belongs"
    echo "        to, and the class is what the binary reports, so a second entry cannot pick another"
    echo "        one. Yours is left as the only claim on it. Remove or rename it and re-run to install"
    echo "        this entry instead."
  done
  local legacy="$apps/oracle.desktop"
  if [ -e "$legacy" ]; then
    echo "note: $legacy is a leftover from an older version of this script and declares the same class as"
    echo "      the frontend entry. It is no longer written or updated here. To clear the duplicate:"
    echo "      rm $legacy"
  fi
}

if [ "$dry" = 1 ]; then
  echo "install-desktop --dry-run: nothing was written. It would create:"
  for f in "${icons[@]}"; do echo "  $f"; done
  for row in ${planned[@]+"${planned[@]}"}; do
    echo "  $apps/${row%%|*}   (Exec=${row##*|})"
  done
  if [ ${#planned[@]} -eq 0 ]; then echo "  (no .desktop entry: see below)"; fi
  report_skips
  exit 0
fi

for s in 32 64 128 256; do
  d="$data/icons/hicolor/${s}x${s}/apps"
  mkdir -p "$d"
  cp "$here/oracle-$s.png" "$d/oracle.png"
done
# The tinted vector, for themes that prefer scalable.
mkdir -p "$data/icons/hicolor/scalable/apps"
cp "$here/oracle-icon.svg" "$data/icons/hicolor/scalable/apps/oracle.svg"

mkdir -p "$apps"
# The entry file name matches the app id it serves, because a compositor that finds no `StartupWMClass`
# match falls back to `<app-id>.desktop`: belt and braces for one `sed`.
for row in ${planned[@]+"${planned[@]}"}; do
  target="${row%%|*}"; rest="${row#*|}"; tmpl="${rest%%|*}"; exe="${rest##*|}"
  sed "s|^Exec=.*|Exec=$exe %f|" "$tmpl" > "$apps/$target"
done

command -v update-desktop-database >/dev/null && update-desktop-database "$apps" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$data/icons/hicolor" 2>/dev/null || true
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 --noincremental >/dev/null 2>&1 || true

echo "wrote:"
for f in "${icons[@]}"; do echo "  $f"; done
for row in ${planned[@]+"${planned[@]}"}; do
  echo "  $apps/${row%%|*}   (Exec=${row##*|})"
done
if [ ${#planned[@]} -eq 0 ]; then echo "  (no .desktop entry)"; fi
report_skips
echo "A window that is already open keeps its old icon; relaunch it."
