d-51 LISTENING KIT: the sound chip's once-a-frame interrupt, "held until taken" vs "one scanline"
====================================================================================================

WHAT CHANGED (plain words)
  Once per frame the console pokes the sound CPU (the Z80) with an interrupt. Aeon's sound driver uses
  that poke to check its mailbox for new sound commands from the game.

  BEFORE (today's oracle): the poke waits for the sound CPU as long as it takes. If the game's main CPU
  is borrowing the sound CPU's bus at that moment, the poke is still there when the bus is handed back,
  and the driver takes it a little late (1 to 10 scanlines).

  AFTER (the change on branch side/d51-int-level-listen): the poke lasts exactly one scanline, as the
  hardware notes say it does. If the sound CPU is locked out of its bus for that whole scanline, it never
  sees that frame's poke; it catches the next frame's instead.

  So AFTER, Aeon's driver skips its mailbox check on some frames. The measurements below say how many and
  when. Nothing else about the sound chip changed.

HOW TO PLAY EACH (about five minutes)
  ./play-before.sh      today's oracle
  ./play-after.sh       the change
  Both play the SAME ROM: aeon's s4.debug.bin as built 2026-09-25 01:53, sha256 e29fa028...183c (a copy
  sits in before/ and in after/, so each side keeps its own save slots). Extra player flags pass through,
  e.g. ./play-after.sh --audio off.
  A save state made in one player will NOT load in the other: the change adds one number to the saved
  machine, so the other build refuses the file by name. That is expected, not a bug.

WHAT TO LISTEN FOR
  - A sound effect or music change that starts a frame later (about 1/60 s) than it should, or a
    command that seems to get "swallowed" and then happens a moment later.
  - It clusters: in every headless run the skipped pokes come in bursts of about 12 frames in a row,
    roughly every 7.6 seconds of play (at about 3 s, 10 s, 18 s, 26 s, 33 s, 41 s from power-on). Those
    bursts are the moments the game holds the sound CPU's bus across the frame's poke. If there is an
    audible difference while you play, it is most likely right around such a burst: a jingle or effect
    triggered in that 0.2 s window.
  - Continuous music in the sound-test ROM (below) differs throughout, from 0.4 s in, but its loudness
    per second is the same both ways: listen for timing/rhythm/"flam" differences, not volume.

HEADLESS RENDERS (renders/, all 44.1 kHz stereo 16-bit, same length and same inputs both ways)
  file                  what it is                                              before vs after
  boot30s-*.wav         the ROM you play, power-on, no buttons, 30 s             IDENTICAL, but SILENT
  legA-*.wav            oracle's frozen copy of the debug ROM, no buttons, 20 s   IDENTICAL, but SILENT
  ojz-*.wav             recorded gameplay replay (OJZ), 31.7 s                    differ 21.79 s - 24 s only
  slide-*.wav           recorded gameplay replay (OJZ slide), 45 s                IDENTICAL, but SILENT
  st-*.wav              aeon's sound-test ROM (built 2026-07-22), 30 s of music   differ from 0.42 s on
  liveojz-*.wav         the replay stream on the ROM you play (it is stale for     IDENTICAL, but SILENT
                        this build and wanders; census only)

  ** The debug ROM makes no sound headless unless the game is played. ** Every debug-ROM render is
  silent except about 3 seconds of effects in the ojz replay (21-24 s), so "IDENTICAL" on a silent file
  says nothing about how it sounds. The two renders worth listening to are:
     renders/st-before.wav   vs renders/st-after.wav     (music; the real A/B)
     renders/ojz-before.wav  vs renders/ojz-after.wav    (skip to 21 s)

MEASURED DIFFERENCE (renders/diff-report.txt, from ./diff-all.sh)
  st (sound test): first different sample at 0.4159 s; 70.6% of samples differ; RMS of the difference
      1282 of 32768, i.e. 12.2 dB below the music itself; per-second loudness equal within 0.3%;
      no second is silent in one and not the other.
  ojz replay: first different sample at 21.7856 s; differs only in seconds 21-23 (the only seconds with
      sound); per-second loudness equal both ways, so it is the same effect placed slightly differently.
  Everything else: sample-identical (and silent).
  The differ was checked on itself: a file against itself reads IDENTICAL, and a copy with one sample
  moved by 1 step at exactly 5.0000 s is reported at exactly sample 441000, 5.0000 s.

MISSED / LATE SOUND-CPU INTERRUPTS OVER THE SAME STRETCHES (renders/*.census.tsv lists each one, with time)
  "late" = taken more than one scanline after the poke; "missed" = never taken that frame.
  stretch      frames   BEFORE: taken / late / missed    AFTER: taken / late / missed    lost
  boot30s       1800        1798 /  47 /   2                  1751 /  0 /  49              47
  legA          1200        1198 /  33 /   2                  1161 /  0 /  39              37
  ojz           1900        1898 /  45 /   2                  1851 /  0 /  49              47
  slide         2700        2698 /  68 /   2                  2627 /  0 /  73              71
  st (music)    1800        1701 / 167 /  99                  1534 /  0 / 266             167
  liveojz       1900        1898 /  47 /   2                  1848 /  0 /  52              50
  Every late or missed poke in the fixture-ROM runs happened while the main CPU held the sound CPU's bus.
  On the ROM you play (liveojz), 12 of them did NOT: the sound CPU simply had interrupts switched off for
  the whole scanline. That case rests on the medium-confidence half of the hardware notes (Eke: "cleared
  on next line ... regardless of interrupts being masked"; his own words: "needs to be confirmed").
  Replays: both still finish on the same frame with the same game tick (1824/1798 and 2449/2599), so the
  game itself plays the same.

HOW TO RE-RUN
  FIXTURES=<oracle checkout>/fixtures/aeon ./render-all.sh before
  FIXTURES=<oracle checkout>/fixtures/aeon ./render-all.sh after
  ./diff-all.sh > renders/diff-report.txt
  python3 loudness.py renders/*.wav            (which seconds have any sound at all)
  How the binaries were built (commits, commands): docs/2026-09-25-d51-listen-kit.md on the branch.

NEEDS-OWNER-EAR: nobody has listened to any of this. The numbers above say where a difference exists,
not whether it is audible or which one sounds right.
