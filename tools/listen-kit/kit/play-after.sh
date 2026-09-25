#!/usr/bin/env bash
# AFTER (side/d51-int-level-listen): the Z80 sound interrupt is a one-scanline pulse, as on hardware.
here="$(cd "$(dirname "$0")" && pwd)"
exec "$here/after/oracle-player" --rom "$here/after/s4.debug.bin" --symbols "$here/after/s4.debug.lst" "$@"
