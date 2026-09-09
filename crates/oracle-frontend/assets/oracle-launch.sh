#!/usr/bin/env bash
# The launcher every Oracle `.desktop` entry runs, instead of running a binary directly.
#
#   usage: oracle-launch.sh (--player | --frontend) [--bin PATH] [--rom PATH] [ROM]
#                           [--no-build] [--print-argv] [--dry-run] [-- EXTRA ARGS...]
#
# `--print-argv` resolves a ROM and prints the command line it would run. `--dry-run` goes one step
# further and takes the freshness decision as well, reporting which arm it took. Neither builds and
# neither execs, which is what makes both checkable from a test with no window and no compiler.
#
# WHY THIS EXISTS AT ALL. A `.desktop` `Exec=` is one line with no room to think, and the two windows it
# has to launch disagree about almost everything a launch needs to decide:
#
#   * THE ROM IS NOT OPTIONAL AND A CLICK SUPPLIES NONE. Both binaries refuse to start without one —
#     `oracle-player` prints `--rom is required`, `oracle-frontend` prints `missing <rom.bin>` — and a
#     launcher icon clicked from a menu passes no file at all, so `%f` expands to nothing. The measured
#     result was an icon that had never once opened a window: it exited to a stderr nobody reads.
#   * THE TWO CLIS DISAGREE ABOUT HOW A ROM IS SPELT. `oracle-frontend` takes it as a positional;
#     `oracle-player` requires `--rom PATH` and its argument loop's fallthrough arm REJECTS positionals
#     (`unknown flag /path/to/rom.bin`). One `Exec=` template for both is therefore wrong for one of
#     them whatever it says, which is exactly how it was wrong.
#   * A BINARY GOES STALE IN SILENCE. A `.desktop` that names `target/release/oracle-player` runs
#     whatever was built last, forever. Nothing about clicking it says the UI change you are looking for
#     is not in it, so a stale window reads as a broken change.
#
# So the decisions live here, in a file that is committed, reviewed and tested, and the `.desktop` entries
# carry only the flags that distinguish them. See `install-desktop.sh`, which substitutes `@LAUNCHER@` and
# `@BIN@` into each template's own `Exec=` line and knows nothing else about either CLI.
#
# ⚑ **NOTHING HERE IS EVER SILENT.** Every path that does not end in a window ends in a message the person
# who clicked can actually see — `zenity` when it is there, `notify-send` besides, and stderr always. An
# exit with no window and no statement is the defect this file was written to remove; do not add a branch
# that has one.
set -euo pipefail

prog="${0##*/}"
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"

have() { command -v "$1" >/dev/null 2>&1; }
say() { printf '%s: %s\n' "$prog" "$*" >&2; }

# A desktop notification, best effort. Never fatal: a launch must not fail because a notification daemon
# is not running.
notify() {
  local urgency="${2:-normal}"
  have notify-send && notify-send -a Oracle -u "$urgency" Oracle "$1" >/dev/null 2>&1 || true
}

# The loud exit. A person who clicked an icon is owed a window or a sentence, and stderr is not a sentence
# when there is no terminal attached — which, from a launcher, there never is.
die() {
  say "$1"
  notify "$1" critical
  have zenity && zenity --error --title=Oracle --width=520 --text="$1" >/dev/null 2>&1 || true
  exit 1
}

window=""
bin=""
rom=""
build=1
print_argv=0
dry_run=0
extra=()

while [ $# -gt 0 ]; do
  case "$1" in
    --player) window=player ;;
    --frontend) window=frontend ;;
    --bin) shift; bin="${1-}" ;;
    --rom) shift; rom="${1-}" ;;
    --no-build) build=0 ;;
    # Resolve everything, print the argv that would be `exec`'d, and stop. Builds nothing and launches
    # nothing, which is what lets a test feed this argv to the real parser without a window appearing.
    --print-argv) print_argv=1 ;;
    # Go one step further: take the freshness decision too, report it, and still neither build nor exec.
    # This is how the rebuild rule is checkable without a compiler in the loop.
    --dry-run) dry_run=1 ;;
    -h|--help) command sed -n '2,40p' "$0"; exit 0 ;;
    --) shift; extra=("$@"); break ;;
    -*) die "$prog: unknown option '$1' (see --help)" ;;
    *)
      if [ -n "$rom" ]; then die "$prog: more than one ROM path given ('$rom' and '$1')"; fi
      rom="$1"
      ;;
  esac
  shift
done

case "$window" in
  player) pkg=oracle-player; name=oracle-player ;;
  frontend) pkg=oracle-frontend; name=oracle-frontend ;;
  "")
    # P10: no em dashes in text a person reads. That rule is about the tool's own strings, and these are
    # the tool's own strings; the surrounding comments are out of its scope and keep theirs.
    die "$prog: no window chosen. Pass --player or --frontend. A .desktop entry whose Exec= lost its \
window flag lands here, which is why this refuses rather than guessing."
    ;;
esac

# An `Exec=` that still holds a placeholder was never substituted. Guessing past it would launch something
# nobody chose, so it is an error with the fix in it.
case "$bin" in
  *@*) die "$prog: --bin is still the template placeholder '$bin'. Re-run install-desktop.sh to write a \
real path into this entry." ;;
esac
: "${bin:=$root/target/release/$name}"

# ---------------------------------------------------------------------------------------------------
# THE ROM.
#
# Order: the argument, then $ORACLE_ROM, then the checkout's usual debug ROM if it is there, then ask.
#
# ⚑ **The argument comes FIRST, ahead of `$ORACLE_ROM`, and that is deliberate.** `%f` is a file the
# person picked in this gesture — "Open With → Oracle Player" on one specific ROM — while `$ORACLE_ROM`
# is a default sitting in a shell profile. An environment default that outranked an explicit pick would
# make "open this file" silently open a different file, which is worse than having no default at all.
# `$ORACLE_ROM` is spelt to match the MCP shim's variable of the same name (`ORACLE_ROM`, default
# `aeon/s4.debug.bin`), so one export moves both.
# ---------------------------------------------------------------------------------------------------
default_rom="$(cd "$root/.." 2>/dev/null && pwd)/aeon/s4.debug.bin"
if [ -z "$rom" ]; then rom="${ORACLE_ROM:-}"; fi
if [ -z "$rom" ] && [ -f "$default_rom" ]; then rom="$default_rom"; fi
if [ -z "$rom" ] && [ "$print_argv" = 0 ] && [ "$dry_run" = 0 ] && have zenity; then
  # Cancelling is an answer, not an error to retry: `|| true` keeps `set -e` from turning it into a
  # crash, and the empty result falls into the refusal below with its own message.
  rom="$(zenity --file-selection --title='Oracle: choose a ROM' \
                --file-filter='Genesis ROMs | *.bin *.md *.gen *.smd' \
                --file-filter='All files | *' 2>/dev/null || true)"
fi
if [ -z "$rom" ]; then
  die "$prog: no ROM to run. Pass one as an argument, set ORACLE_ROM=/path/to/rom.bin, or build \
$default_rom. (Both Oracle windows refuse to start without a ROM, so this stops here rather than \
exiting with nothing on screen.)"
fi
if [ ! -f "$rom" ]; then
  die "$prog: no ROM at '$rom'."
fi

# ---------------------------------------------------------------------------------------------------
# THE ARGV. This is the only place either CLI's shape is written down on the launching side, and the two
# arms are different on purpose — see the header.
#
# `--aether` is on both, because the point of these entries is a window Aurora can attach to. A second
# window cannot bind a socket the first one holds; the player reports that itself, naming the resolved
# path, so this does not try to pre-empt it beyond the warning below.
# ---------------------------------------------------------------------------------------------------
case "$window" in
  player) argv=("$bin" --rom "$rom" --aether) ;;
  frontend) argv=("$bin" "$rom" --aether) ;;
esac
argv+=(${extra[@]+"${extra[@]}"})

if [ "$print_argv" = 1 ]; then
  printf '%s\n' "${argv[@]}"
  exit 0
fi

# ---------------------------------------------------------------------------------------------------
# FRESHNESS.
#
# The gate is a cheap mtime comparison against the newest TRACKED source, and its only job is to decide
# whether to call `cargo` at all. Cargo is the real authority on what needs rebuilding; this exists so
# that the common case — nothing changed since the last launch — costs a `stat` sweep instead of a cargo
# invocation and its lock on `target/`, which parallel work in this tree does contend for.
#
# Tracked, not all, because `target/` and stray scratch files would make every launch look dirty. Mtime,
# not `git status`, because a tracked file edited and not yet committed is exactly the case the owner
# cares about.
# ---------------------------------------------------------------------------------------------------
# ⚑ **THE NUL-SEPARATED LIST NEVER PASSES THROUGH A VARIABLE, AND THAT IS THE WHOLE POINT.** The first
# version of this function did `list="$(git ... ls-files -z ...)"`. Command substitution STRIPS NUL bytes,
# so the separators vanished, `xargs -0` was handed one enormous concatenated filename, `stat` failed on
# it, and the function returned nothing. The gate then took its "cannot measure" arm and asked cargo on
# EVERY launch. Measured, not reasoned: running `--dry-run` printed bash's own
# `warning: command substitution: ignored null byte in input` above the verdict, which is how it was
# caught. Keep the NULs inside one pipeline.
#
# `pipefail` is off for the pipeline because a `stat` that cannot see one file should not turn the whole
# measurement into a hard failure; emptiness is the signal the caller reads, and the caller treats empty
# as "unmeasurable, so ask cargo" rather than as "fresh".
newest_tracked_mtime() {
  (
    set +o pipefail
    git -C "$root" ls-files -z -- crates Cargo.toml Cargo.lock 2>/dev/null \
      | (cd "$root" && xargs -0 -r stat -c %Y -- 2>/dev/null) \
      | sort -n | tail -n 1
  )
}

run_build() {
  local log status=0 build_pid zen_pid=""
  log="$(mktemp -t oracle-launch-build-XXXXXX.log)"
  say "rebuilding $name; log: $log"
  notify "Rebuilding $name. The window opens when the build finishes." low

  (cd "$root" && cargo build --release -p "$pkg" --bin "$name") >"$log" 2>&1 &
  build_pid=$!

  # A pulsating window for the whole build. A notification can be missed or switched off, and twenty
  # silent seconds after a click is indistinguishable from a launcher that did nothing.
  if have zenity; then
    (while kill -0 "$build_pid" 2>/dev/null; do printf '#Rebuilding %s...\n' "$name"; sleep 1; done) \
      | zenity --progress --pulsate --auto-close --no-cancel --width=420 \
               --title=Oracle --text="Rebuilding $name..." >/dev/null 2>&1 &
    zen_pid=$!
  fi

  wait "$build_pid" || status=$?
  [ -n "$zen_pid" ] && { wait "$zen_pid" >/dev/null 2>&1 || true; }

  if [ "$status" != 0 ]; then
    # ⚑ A FAILED BUILD DOES NOT FALL BACK TO THE OLD BINARY. Launching the stale one here would put the
    # previous UI on screen with no hint that the change being looked for is missing — which is the
    # original complaint, restored by the very code meant to fix it. Refuse, and show the compiler.
    local tail_text
    tail_text="$(tail -n 20 "$log" 2>/dev/null || true)"
    die "$prog: $name failed to build, so nothing was launched (the binary already on disk is older than \
your sources and would show you the previous UI).

Log: $log

$tail_text

To run the old binary anyway, add --no-build to the entry's Exec= line."
  fi
  say "build ok"
}

# ⚑ **EVERY ARM OF THIS DECISION SAYS WHICH ARM IT TOOK, INCLUDING THE ONE THAT DOES NOTHING.** The
# "already current" case used to be the silent one, and a freshness gate that is silent when it decides
# not to act is indistinguishable from a freshness gate that was never reached. That is also what makes
# the decision testable without a compiler: `--dry-run` runs exactly this block, prints the same verdict,
# and stops before `run_build` and before the exec.
freshness=""
if [ "$build" = 0 ]; then
  freshness="not rebuilding: --no-build was given"
elif [ "$bin" != "$root/target/release/$name" ]; then
  freshness="not rebuilding: --bin '$bin' is not this checkout's own target/release/$name"
elif ! src_mtime="$(newest_tracked_mtime)" || [ -z "${src_mtime:-}" ]; then
  # ⚑ Unmeasurable is not fresh. If the tracked-source timestamps cannot be read (not a checkout, no
  # git) the honest move is to hand the question to cargo, which answers it properly, rather than to
  # assume the binary is current because the cheap gate could not say otherwise.
  freshness="would rebuild: cannot read tracked-source timestamps under $root, so asking cargo instead \
of assuming fresh"
elif [ ! -x "$bin" ]; then
  freshness="would rebuild: no binary at $bin yet"
elif [ "$(stat -c %Y "$bin")" -lt "$src_mtime" ]; then
  freshness="would rebuild: $name is older than the newest tracked source"
else
  freshness="not rebuilding: $name is newer than every tracked source"
fi
say "$freshness"

if [ "$dry_run" = 1 ]; then
  say "dry run: stopping here. Would exec: ${argv[*]}"
  exit 0
fi

case "$freshness" in
  "would rebuild:"*) run_build ;;
esac

# ---------------------------------------------------------------------------------------------------
# THE ALREADY-SERVING WARNING.
#
# ⚑ `pgrep -x`, NOT `pgrep -f`, and both window names. The script this replaces asked
# `pgrep -f 'oracle-frontend'`, which was wrong twice over: it is blind to `oracle-player` holding the
# same socket — the exact case that leaves Aurora talking to a window nobody is looking at — and `-f`
# matches whole command lines, so a shell whose own arguments contain the string matches itself.
# `-x` matches the process NAME, which a shell never carries.
#
# `oracle-frontend` is fifteen characters, and a Linux `comm` is fifteen characters plus a NUL. It fits
# with nothing to spare: a longer binary name added here would be truncated in `comm` and would silently
# never match. Check any new name against that limit rather than trusting this loop.
# ---------------------------------------------------------------------------------------------------
sock="${ORACLE_SOCKET:-${EXODUS_SOCKET:-${XDG_RUNTIME_DIR:-/tmp}/oracle.sock}}"
if [ -S "$sock" ]; then
  holders=""
  for n in oracle-player oracle-frontend oracle-aether; do
    if pgrep -x "$n" >/dev/null 2>&1; then holders="$holders $n"; fi
  done
  if [ -n "$holders" ]; then
    say "an Oracle process is already running ($(echo "$holders" | command sed 's/^ //')) and $sock \
exists; this window's --aether will refuse to bind it and will say so. Close the old window first, or \
Aurora keeps talking to it."
    notify "Another Oracle window is already serving Aether on $sock. The new window will not bind it."
  fi
fi

say "exec ${argv[*]}"
exec "${argv[@]}"
