#!/usr/bin/env bash
# Resolve the aeon suite's native assembler toolchain (`asl`, `p2bin`) for the differential harness
# builders. Sourced, not executed.
#
# WHAT THIS REPLACED, AND WHY IT WAS WRONG TWICE
# ----------------------------------------------
# All three builders carried `TOOLS="${TOOLS:-$HERE/../../../aeon/tools}"`.
#
#   1. It is a silent fallback into a **peer's live working tree**. `empyrean`
#      `contract/SUITE_PATHS.md` at `38f6df4` rules that shape out across the suite: precedence is the
#      explicit variable, then `EMPYREAN_SUITE_ROOT` joined with the repo's directory name, then
#      derivation, then *"refuse, naming what was looked for and where. Never a home literal, and never
#      a silent fallback to the live tree."*
#
#   2. The relative walk was **already broken from a linked worktree**, which is where agents run. From
#      the main checkout `$HERE/../../..` is the suite root; from `<repo>/.claude/worktrees/<name>` the
#      same three steps land on `<repo>/.claude/worktrees`, so it looked for
#      `<repo>/.claude/worktrees/aeon/tools` and failed with `No such file` naming a path nobody wrote.
#      That is the relative-derivation hazard `SUITE_PATHS.md` records against `--git-common-dir`,
#      arriving here through plain `..` instead: the wrong answer is invisible from a main checkout and
#      only shows up where the suite does not normally run.
#
# There is no derivation step. What this resolves is a directory of **binaries** that no revision
# attributes — the same reason `examples/common/rom_source.rs` stops at a refusal. A walk would hand a
# build an unattributable toolchain while looking like resolution.
#
# `TOOLS` keeps its own name deliberately: it points at a directory of ARTIFACTS, not a checkout, and
# `SUITE_PATHS.md` says such a variable "keeps its own name; it is not an alias of `AEON_DIR`".

# Echoes the resolved tools directory on stdout and the step that answered on stderr; returns non-zero
# with the full refusal on stderr when nothing named one.
resolve_aeon_tools() {
    local tried=()

    if [ -n "${TOOLS:-}" ]; then
        echo >&2 "RESULT ok step=1-env-tools \$TOOLS=$TOOLS"
        echo "$TOOLS"
        return 0
    fi
    tried+=("\$TOOLS (a path to the assembler tools DIRECTORY) — not set")

    if [ -n "${AEON_DIR:-}" ]; then
        echo >&2 "RESULT ok step=2-env-checkout \$AEON_DIR=$AEON_DIR"
        echo "$AEON_DIR/tools"
        return 0
    fi
    tried+=("\$AEON_DIR/tools — AEON_DIR not set")

    if [ -n "${EMPYREAN_SUITE_ROOT:-}" ]; then
        echo >&2 "RESULT ok step=3-suite-root \$EMPYREAN_SUITE_ROOT=$EMPYREAN_SUITE_ROOT"
        echo "$EMPYREAN_SUITE_ROOT/aeon/tools"
        return 0
    fi
    tried+=("\$EMPYREAN_SUITE_ROOT/aeon/tools — EMPYREAN_SUITE_ROOT not set")

    {
        echo "REFUSED: the aeon assembler toolchain was not named. Consulted, in order:"
        printf '  %s\n' "${tried[@]}"
        echo
        echo "There is deliberately no default and no filesystem walk: the previous fallback was a"
        echo "relative walk into a peer's live working tree that resolved WRONG from a linked worktree"
        echo "(empyrean contract/SUITE_PATHS.md at 38f6df4). Name one:"
        echo
        echo "    AEON_DIR=/path/to/aeon $0"
    } >&2
    return 1
}
