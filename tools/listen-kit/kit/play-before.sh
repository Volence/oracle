#!/usr/bin/env bash
# BEFORE (today's main): the Z80 sound interrupt is held until the sound CPU takes it.
here="$(cd "$(dirname "$0")" && pwd)"
exec "$here/before/oracle-player" --rom "$here/before/s4.debug.bin" --symbols "$here/before/s4.debug.lst" "$@"
