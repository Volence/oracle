# Woven S2 act music: is the oracle core at fault? (2026-09-28)

**Verdict: NOT OURS.** On the frozen clip ROM, the oracle core plays steady EHZ music after the same
~2 s load silence as GPGX. Every Z80 banked read in 20 s of play returns the correct ROM byte, and the
driver never reads the CPZ song body.

## Question

The owner heard "sporadic CPZ sounds, not EHZ, and mostly no music" on first load of aeon's
`s4.s2clip.debug.bin` (the d6ce40fb build, which no longer exists). aeon cannot reproduce it on GPGX. The
clip keeps its sound banks at and above 1 MiB, reached through the Z80 `$A06000` latch at values `$20`
and up, which canonical games never use. The suspect was the oracle core's latch or ROM mapping.

## Inputs (copied and hashed before use; worked only from the copies)

From aeon's frozen folder `/home/volence/sonic_hacks/woven-audio-0b8c65d7/`:

| File | sha256 |
|---|---|
| `s4.s2clip.debug.bin` (crc32 0b8c65d7, 1,200,274 B; S2CLIP=s2_woven DEBUG=1 build of aeon 5c1874be) | `700d9cd79b4a4e38b76d5e916941c4905fb4ecf01847191b729d92ceb2ab2dd2` |
| `s4.s2clip.debug.lst` | `521d92da811862bb26a4c8bf11bff9d15a25152a8a9eb27862ada5b82320df3e` |
| `woven_boot_reference.wav` (GPGX libretro, mame ym2612, filter off, 20 s from frame 0, no input) | `15bc0281350bab3645d9bb766db51976d1def66032fb02e005436772e119aa18` |
| `bootaudio.py` (the tool that made the reference) | `b9c6eb60af2393c0fc0aa9901909d466564d77db0b67f378b1c3820d6c8980d2` |

The frozen ROM is byte-identical to the `aeon/s4.s2clip.debug.bin` candidate built at 15:32. I also
rendered two other candidates: `s4.s2clip.bin` (`ca7d311f…ed87`) and `s4.woven.debug.bin` (`76a63d06…d56`).

Listing symbols: `Dac_SharedBank_Start` = `$100000` (latch `$20`), `Song_S2_EHZ` = `$10BC09`,
`S2_EHZ_Patches` = `$10C5E9`, `Song_S2_CPZ` = `$10C709`, `S2_CPZ_Patches` = `$10D527`,
`SongTable` = `$10F674`, `SongBank2_Head` = `$110000` (latch `$22`), `Music_Want`/`Music_Current` =
`$FFB5A4`/`$FFB5A5`, `SONG_S2_EHZ` = 2, `SONG_S2_CPZ` = 3.

## 1. Headless audio capture

This runs the existing dev tool `crates/oracle-core/examples/synth_render.rs`
(`--features synth`, console model `unfiltered`, 44.1 kHz stereo). It boots the ROM with no input and
runs 1200 frames. The result is 85.8% non-silent samples.

Per-second RMS, in dBFS of the mono mix. The formula is the same as `bootaudio.py`: `10*log10(mean(x^2)/32768^2)`, with -120 meaning digital silence.

| s | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 18 | 19 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| GPGX ref | -55.0 | -120 | -27.5 | -22.5 | -21.2 | -20.1 | -21.1 | -22.6 | -21.7 | -23.0 | -22.8 | -21.3 | -22.0 | -22.3 | -22.4 | -21.7 | -23.0 | -22.8 | -21.1 | -21.9 |
| oracle | -120 | -120 | -30.1 | -26.2 | -24.5 | -23.6 | -25.1 | -26.4 | -25.2 | -26.4 | -27.0 | -24.7 | -26.5 | -26.4 | -25.6 | -25.7 | -26.4 | -26.0 | -25.3 | -25.6 |

The load silence and the steady music that follows both match GPGX. The oracle mix runs a constant
~3.5-4 dB quieter, which is a gain difference between the two synth cores. It is not a loss of music.

## 2. Comparison with the GPGX reference

Script: `scratchpad/cmp.py` (numpy only, not committed). Methods:

- **Envelope correlation.** Pearson correlation of 20 ms RMS envelopes from t >= 3 s, with a ±2 s lag
  search. Result: **0.885 at lag -60 ms**. Control: the same measure forced to a 2.5-3.5 s misalignment
  peaks at **0.613**. That is the baseline for a repetitive track aligned to itself at the wrong place.
- **Waveform cross-correlation.** FFT xcorr over 4-16 s. Result: **0.598 at -2165 samples (-49 ms)**.
  The two arms use different YM2612 cores (oracle's own synth and mame's), so waveforms are not
  expected to match sample for sample.
- **Per-second log-spectrum correlation.** 0.68-0.87 for every second from 2 s on.

Other candidates on the same measure:

- `s4.woven.debug.bin`: steady music from 2 s, envelope 0.926.
- `s4.s2clip.bin` (non-debug): music from frame 0 with no load silence, envelope 0.952 at the ±2 s
  search edge. The non-debug build loads faster, so this lag is outside the search window.

The known "headless `s4.debug.bin` renders come out mostly silent" did **not** recur on any of these
three images.

## 3. Banked reads at and above 1 MiB (unit gate, committed)

The new gate is `z80::bus::tests::bank_window_reads_rom_above_one_mebibyte_via_serial_latch` in
`crates/oracle-core/src/z80/bus.rs`, commit `09b90a62`. It builds a 1,200,274-byte image (the clip's
size) in which each byte is `page*37 ^ offset`, so a read from the wrong page returns a different byte.
It serial-loads pages `$20`, `$21`, `$22` and `$24` (`$24` holds the image end, `$125091`) with nine
`$6000` writes each, LSB first, with junk set in bits 1-7. For each page it checks that window reads at
`$8000`, `$8001`, `$C123`, `$F674` and `$FFFF` return `rom[(page<<15)|(z&$7FFF)]`. It also checks that
the last image byte is mapped and that the byte one past the end reads open bus `$FF`.

**Green on the unmodified tree.** Red-first proof, with each mutation on disk and restored from the committed
baseline (`git restore --source=HEAD`) each time:

1. A 5-bit latch mask in `window_addr`, via
   `(((*self.bank as u32) & 0x1F) << 15) | (addr as u32 & 0x7FFF)`: the gate is red at bus.rs:470. The
   older `bank_window_reads_and_writes_work_ram` also goes red.
2. A 1 MiB image mirror in `read_window`, via `self.cart_banks.rom_offset(a68k) & 0xF_FFFF`: the gate is
   red at bus.rs:470. The older mapper test also goes red.

Source audit of the cart and bus path:

- `window_addr` is `(bank<<15)|(addr&0x7FFF)`, with the full 9 bits and no mask.
- `CartBanks::rom_offset` is the identity at reset: `bank*$80000 + (a&$7FFFF)` with `banks=[0..7]`, so
  every address below `$400000` maps flat.
- `System::load_rom` keeps the whole `Vec`, so there is no size cap.
- Reads past the image end return `$FF`.

The SRAM overlay is not consulted on the Z80 window path. The clip's SRAM (fallback `$200000+`) is far
from the sound banks anyway.

## 4. Runtime Z80 tap on the real ROM (scratch instrumentation, reverted)

I added a temporary thread-local log to `Z80Bus::read` (the `$8000-$FFFF` arm) and the `$6000` write
arm. It recorded the latch value, Z80 address, byte and Z80 PC for 1200 frames. Both the log and its
harness example were removed afterwards, and the tree was restored from HEAD.

- **163,744 window reads. 0 mismatches** against `rom[(latch<<15)|(addr&$7FFF)]`.
- Only two latch values were ever used for reads: `$020` (113,236 reads, DAC bank at `$100000`) and
  `$021` (50,508 reads, song bank at `$108000`). The latch reaches `$021` on frame 3. From frame ~170
  the driver switches between `$20` and `$21` every frame.
- Reads by ROM region: DAC 162,365, `Song_S2_EHZ` 997, `S2_EHZ_Patches` 382, **`Song_S2_CPZ` 0,
  `S2_CPZ_Patches` 0**. The first EHZ body read is on frame 169, and EHZ reads continue in every second
  from 2 to 19.
- `Music_Want` goes 0 → 2 on frame 165, and `Music_Current` goes 0 → 2 on frame 168.
- No Z80 fault.
- The driver never read `SongTable` (`$10F674`) through the window. It reaches the song by some other
  path, possibly a table copied to Z80 RAM. The body reads show which song it chose: EHZ.

The latch-change list also held a few transient values: `$002` on frame 127 and `$042` on frame 135.
These are artefacts of my 9-write segmentation, because the 68k-side `$A06000` ticks were not tapped.
No read was ever issued while one of those values was in the latch.

## Conclusion and open items

The oracle core is not the cause on this ROM. Its latch, window translation and >1 MiB mapping are
correct at runtime and under the new gate. The music is EHZ, the same song GPGX plays.

Open items and tags:

- **TAG (GUI).** The owner may have heard it in the oracle **player**. Its real-time audio path
  (pacing, resampling and device buffering) is not exercised by this headless render. "Sporadic sounds,
  mostly no music" is also what audio underruns sound like. This needs a live GUI listen on this ROM.
- **Open.** The player persists battery SRAM (`.srm`) between runs, but headless runs boot with fresh
  SRAM. If the debug clip reads SRAM at boot, the owner's saved `.srm` could change the boot path.
  Check whether that clip reads SRAM, and try the owner's `.srm` if one exists.
- **Open.** The d6ce40fb build the owner actually heard is gone. aeon reports it behaved the same on
  their headless check.
- The ~3.5-4 dB level gap between oracle and GPGX is a synth gain difference. It is not a correctness
  finding here.
