#!/usr/bin/env bash
# Fetch the vendored Mega Drive TEST-ROM corpus driven by `tests/conformance_roms.rs`.
# Reproducible by construction: a pinned per-ROM source id + a sha256 manifest of the downloaded
# archives. Output lands in the gitignored vendor/ dir; the conformance runner reads it (and skips
# cleanly per-ROM if it is absent).
#
# NOTE: these are external, publicly-circulated homebrew/PD *test programs* (VDPFIFOTesting, the
# sprite-masking ROM, the 68000 BCD/illegal-opcode verifiers, ...), NOT emulator source and NOT
# commercial ROMs. They are the Genesis-level analog of tools/fetch-tests.sh (the 68000 SST corpus):
# an answer key we grade the whole console against. The harness that consumes them is deliberately
# NON-GATING — it pins today's per-ROM outcome and fails only on a REGRESSION from that pin, never
# on "everything must pass" (see CHARTER.md: the launch target is MVP-debuggable, not "passes
# VDPFIFOTesting"). Several of these ROMs fail today for documented reasons; see
# docs/2026-07-25-testrom-conformance.md.
#
# Sources are Google Drive file ids. Each id yields a ZIP; the ROM inside is extracted and
# NORMALIZED to $OUT/<local_name>.bin so the test runner never has to know the archive's internal
# (space-laden, mixed-case) filenames.
#
# Deliberately NOT fetched: the two TiTAN "Overdrive" mega-demos. They are the classic hardware-
# torture-test payloads, but their verdict is human judgment on a moving picture ("does the
# scroll/palette trickery look right"), not a scrapeable pass/fail, so they are not automatable in
# this harness. For the record their ids are 1MLB7IJgmCf0UobsEke_9bwRVkDz9LHT6 (Overdrive, ~34MB)
# and 1RAHRinL6gWFgD-RmmxAZJAc1UCcAqOHU (Overdrive 2). Drive serves files over ~25MB as an HTML
# "confirm" interstitial instead of the payload, so anything that large needs the alternate host
# form below (only Overdrive is affected):
#   https://drive.usercontent.google.com/download?id=<ID>&export=download&confirm=t
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$HERE/../vendor/TestRoms"
CHECKSUMS="$HERE/testroms.sha256"

# local_name | drive_id | rom filename inside the zip
ROMS=(
  "io_sample|1NWsPdBrlWuVbmcLjfXZsWHpo-v92GawD|Multitap - IO Sample Program (U) (Nov 28 1992).gen"
  "fm_test|1QcnghG6jWM2MrxBbIuSrJvyvV0U1Pqtc|FM Test by DevSter (PD).bin"
  "gfx_joystick|1WEbbh3S_oZ-T2TTULZd6EMCwI6k0TYWZ|Graphics & Joystick Sampler by Charles Doty (PD).bin"
  "m68k_bcd|1EgwX-T4g9bUcsGc6FywO_-4Irxjo1Vgn|bcd-verifier-u1.bin"
  "m68k_opcode_sizes|1VGTj6ZTLS7eVzqXuiCXmkdBBVnZ6KfJQ|m68k_opcode_sizes.bin"
  "m68k_illegal|1NKagoNVmUEZB9DojKcmXSsSlJY96xS8-|itest.BIN"
  "vcounter|1OULXRZwJd11D4Y5vVtEX11EugzuUELqf|vctest.bin"
  "cram_flicker|1-FZqLceTxBnzJv8AR4bBTAUZqS_pyjT-|cram flicker.bin"
  "vdp_port_access|19vQuo8diG5OQMD5ythejtJByFzOwIl4i|VDPFIFOTesting.bin"
  "vdp_sprite_masking|14qAO4_EKKN2bcumExkv-RgTcl9H_Auzq|SpriteMaskingTestRom.bin"
  "vdp_test_register|1W08doQKWZPEx7xJX4KlVgBsNMS--zzU3|DisableRegTestROM.bin"
  "m68k_memory_test|1btAFafip50yCpAV74R6hh0QOjN0XBv-L|memtest_68k.bin"
  "color_1536|1h5JZkMaq_43X4XTq5uYfBWIJg-cKtMrV|TEST1536.BIN"
  "shadow_highlight|1bmO4bE1NeDfIC1jxrC4GZc17xggthT3J|Shadow-Highlight Test Program #2 (PD).bin"
  "window_test|169ZUeEMrfYFLiKz1jpb53zJQJg1Wba00|Window Test by Fonzie (PD).bin"
  "direct_color_dma|1YdzJFkB4IrMxCwFfmunIb_hIHPvNZA_Y|Direct-Color-DMA.bin"
  "window_distortion|1nDITsbkmo3BYRve6NX35Aqt7jo7FWtqP|Window distortion bug.BIN"
)

ZIPS="$OUT/.zips"
mkdir -p "$ZIPS"

for entry in "${ROMS[@]}"; do
  name="${entry%%|*}"
  rest="${entry#*|}"
  id="${rest%%|*}"
  echo "fetching $name.zip"
  curl -fsSL -o "$ZIPS/$name.zip" "https://drive.google.com/uc?id=$id&export=download"
done

if [[ -f "$CHECKSUMS" ]]; then
  echo "verifying checksums"
  ( cd "$ZIPS" && sha256sum -c "$CHECKSUMS" )
else
  echo "no manifest yet — generating $CHECKSUMS from this fetch (first run; commit it to pin)"
  ( cd "$ZIPS" && sha256sum -- *.zip > "$CHECKSUMS" )
fi

for entry in "${ROMS[@]}"; do
  name="${entry%%|*}"
  member="${entry##*|}"
  echo "extracting $name.bin"
  # -j strips any directory prefix; the member name may contain spaces, hence the quoting.
  unzip -o -j -q "$ZIPS/$name.zip" "$member" -d "$ZIPS/$name"
  mv -f "$ZIPS/$name/$(basename "$member")" "$OUT/$name.bin"
  rmdir "$ZIPS/$name"
done

echo "vendored to $OUT"
