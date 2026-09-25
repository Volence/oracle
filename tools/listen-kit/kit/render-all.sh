#!/usr/bin/env bash
# render-all.sh before|after : render the same four stretches with that side's listen-kit.
# Rebuild recipe and SHAs: docs/2026-09-25-d51-listen-kit.md on branch side/d51-int-level-listen.
set -u
here="$(cd "$(dirname "$0")" && pwd)"
side=$1
kit="$here/$side/listen-kit"
out="$here/renders"
fx="${FIXTURES:?set FIXTURES to the fixtures/aeon directory of an oracle checkout}"
mkdir -p "$out"
st=0
# 1. What the owner plays: aeon's live debug ROM (snapshot in before/ and after/), power-on, no input, 30 s.
"$kit" render "$here/$side/s4.debug.bin" 1800 "$out/boot30s-$side.wav" "$out/boot30s-$side.census.tsv" || st=1
# 2. The design's leg A: the frozen fixture ROM, power-on, no input, 1200 frames.
"$kit" render "$fx/s4.debug.bin" 1200 "$out/legA-$side.wav" "$out/legA-$side.census.tsv" || st=1
# 3/4. The two recorded input replays embedded in the frozen fixture ROM (gameplay with SFX).
"$kit" replay "$fx/s4.debug.bin" "$fx/s4.debug.lst" ojz 1900 "$out/ojz-$side.wav" "$out/ojz-$side.census.tsv" || st=1
"$kit" replay "$fx/s4.debug.bin" "$fx/s4.debug.lst" slide 2700 "$out/slide-$side.wav" "$out/slide-$side.census.tsv" || st=1
# 5. aeon's sound-test ROM (built 2026-07-22, copied into renders/): the only stretch with continuous music.
"$kit" render "$out/s4.soundtest.bin" 1800 "$out/st-$side.wav" "$out/st-$side.census.tsv" || st=1
# 6. The ojz replay on the LIVE ROM: its stream is stale for this build (never completes) and the render is
#    silent, but the inputs are identical both ways, so its census is a valid A/B on what the owner plays.
"$kit" replay "$here/$side/s4.debug.bin" "$here/$side/s4.debug.lst" ojz 1900 "$out/liveojz-$side.wav" "$out/liveojz-$side.census.tsv" || st=1
echo "render-all $side exit=$st"
exit $st
