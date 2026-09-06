#!/usr/bin/env bash
# Verify every vendored corpus against its pinned sha256 manifest.
#
# WHY THIS EXISTS, AND WHY IT IS NOT REDUNDANT WITH THE FETCH SCRIPTS
#
# Each `tools/fetch-*.sh` already ends with its own `sha256sum -c`, so on the path where CI actually
# downloads, this script re-proves something already proven. The path it exists for is the OTHER one:
# `.github/workflows/ci.yml` restores `vendor/` from an `actions/cache` entry and SKIPS the fetch on a
# hit. Those bytes arrive without ever passing a manifest. A cache that is stale, truncated or
# corrupt would satisfy the `vendor_data_present_when_running_in_ci` guards — they check that named
# files EXIST, not what is in them — and the suite would go green over the wrong corpus. That is the
# same shape as the vacuity the guards were written to prevent, one layer further out.
#
# So: the cache is restored, this runs unconditionally, and only then does anything read the corpus.
#
# It also closes the .gz -> .json derivation gap. `tools/singlesteptests.sha256` pins the DOWNLOADED
# `.json.gz` files, but `singlestep_m68000.rs` reads the gunzipped `.json` beside them, which no
# manifest covers. Verifying the archives and then regenerating the plaintext from them (as the fetch
# script itself does) is what makes the bytes the tests read provably the pinned bytes.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$HERE/.."

M68K="$ROOT/vendor/ProcessorTests/68000/v1"
Z80="$ROOT/vendor/ProcessorTests/z80/v1"
ROMS="$ROOT/vendor/TestRoms/.zips"

fail() { echo "verify-vendor: $1" >&2; exit 1; }

[[ -d "$M68K" ]] || fail "missing $M68K — run tools/fetch-tests.sh"
[[ -d "$Z80"  ]] || fail "missing $Z80 — run tools/fetch-z80-tests.sh"
[[ -d "$ROMS" ]] || fail "missing $ROMS — run tools/fetch-testroms.sh"

# `--quiet` prints only failures; 949 "OK" lines would bury the three headings below. A missing file
# is still a hard failure (`sha256sum -c` reports "FAILED open or read" and exits non-zero).
echo "== 68000 SingleStepTests: $(wc -l < "$HERE/singlesteptests.sha256") pinned archives =="
( cd "$M68K" && sha256sum -c --quiet "$HERE/singlesteptests.sha256" )
# Re-derive the plaintext the test runner actually reads, from archives just proven to be the pinned
# bytes. Idempotent, and the same `gunzip -kf` tools/fetch-tests.sh ends with.
gunzip -kf "$M68K"/*.json.gz
echo "   ...and the .json plaintext regenerated from them"

echo "== z80 SingleStepTests: $(wc -l < "$HERE/singlesteptests-z80.sha256") pinned files =="
( cd "$Z80" && sha256sum -c --quiet "$HERE/singlesteptests-z80.sha256" )

echo "== Mega Drive test ROMs: $(wc -l < "$HERE/testroms.sha256") pinned archives =="
( cd "$ROMS" && sha256sum -c --quiet "$HERE/testroms.sha256" )

echo
echo "all three vendored corpora match their pinned manifests"
