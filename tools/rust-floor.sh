#!/bin/sh
# Print THE RUST FLOOR — the workspace's declared MSRV — and nothing else.
#
# One declaration, in `Cargo.toml` under `[workspace.package]`; this script is how every other place
# that needs the number READS it instead of restating it. Both CI workflows call it into a step output
# and hand that to `dtolnay/rust-toolchain`, so the version a runner installs is the version the
# manifest declares, by construction rather than by anyone remembering to update four files.
#
# LOUD ON UNMEASURABLE, deliberately: if the key is missing or unreadable this exits 1 with a message
# on stderr and prints nothing on stdout. A CI step that captures this as `v=$(tools/rust-floor.sh)`
# under `bash -e` dies there — which is the point. The failure mode being avoided is an empty value
# flowing into the toolchain action and silently installing whatever `stable` happens to be that day,
# i.e. a fix that certifies nothing.
#
# Under test by `crates/oracle-core/tests/toolchain_floor.rs`, which RUNS this script and compares its
# output against its own independent parse of the manifest — so the exact code CI executes is exercised
# locally rather than being an untested shell snippet living only in a YAML file.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
manifest="$root/Cargo.toml"

if [ ! -r "$manifest" ]; then
    echo "rust-floor: cannot read $manifest" >&2
    exit 1
fi

# Anchored at column 0, so this matches the table-level key under `[workspace.package]` and NOT the
# members' `rust-version.workspace = true` (different text) nor any indented key.
floor=$(sed -n 's/^rust-version *= *"\([0-9][0-9.]*\)".*/\1/p' "$manifest" | head -n 1)

if [ -z "$floor" ]; then
    echo "rust-floor: no [workspace.package] rust-version in $manifest" >&2
    echo "rust-floor: declare the floor there; every CI toolchain step reads it from this script." >&2
    exit 1
fi

printf '%s\n' "$floor"
