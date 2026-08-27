# Symbol watch — the player says what a named RAM byte became

**Parcel:** `parcel/symbol-watch`
**Files:** `crates/oracle-frontend/src/symbol_watch.rs` (new), `crates/oracle-frontend/src/config.rs`,
`crates/oracle-frontend/src/main.rs`

---

## What it does

Name a RAM symbol and its values in `player.conf`. Whenever that byte changes while the game runs, the
player puts a line on the glass:

```
8/20 — Haze
```

Nothing has to be armed. No client has to connect. No bus call is made. It is configured once, it
persists across runs, and it serves the person sitting at the window.

### Where the ask came from

Aeon has a debug hotkey that cycles twenty background-effect scenes live. It works, and the owner
immediately hit the wall that nothing tells him *which* scene is on screen. His words:

> *"do you think we could add something to either the emulator or somewhere else to say in text what kind
> of scene we're switching to? just so I know? Like 1/20 - Fire BG or something."*

The game side deliberately will not do this: there is no debug-text path in a running frame, so a ROM-side
readout means building a font renderer, which is far more work than the feature deserves. The player
already owns a font, an overlay, and a `.lst` reader.

---

## Paste this (the twenty scenes)

Add to `$XDG_CONFIG_HOME/oracle/player.conf` (or `~/.config/oracle/player.conf`). One line, wrapped here
for reading — **it must be a single line in the file**:

```
symbol_watch = Debug_Scene_Index: OJZ Default, OJZ Underwater, OJZ Windy, Shimmer Slow, Shimmer, Shimmer Fast, Haze Slow, Haze, Haze Fast, Haze Uniform, Rocking Slow, Rocking, Rocking Fast, Perspective Subtle, Perspective, Perspective Dramatic, Windy Haze, Sky Haze, OJZ Caves, OJZ Locked Clouds
```

The labels are in `SCENES[]` order, transcribed from the `.scene_table` in
`aeon/games/sonic4/test/ojz_scroll_test.emp` (indices 0–19). They are **aeon's to change** — rename a
scene there, edit this line, and nothing in oracle needs rebuilding. That is the whole reason they live in
a config file: see "The scoping call" below.

Press the hotkey and the corner reads `1/20 — OJZ Default`, `2/20 — OJZ Underwater`, and so on.

### ⚠ It will complain until `s4.debug.bin` is rebuilt

Checked on disk at the time of writing:

| | |
|---|---|
| `aeon/s4.debug.bin` / `s4.debug.lst` built | 2026-08-26 **19:06** |
| `lab(effects): a DEBUG hotkey cycles the twenty scenes live` (`96be3d7f`) | 2026-08-26 **19:49** |

The built artefacts predate the feature by 43 minutes, and `grep Debug_Scene_Index s4.debug.lst` finds
nothing (the only `Debug_Scene_*` symbol in it is `Debug_Scene_Freeze`). So with today's binaries the
player will say, once, in red:

```
symbol watch: cannot watch `Debug_Scene_Index` — this ROM's listing has no such symbol
(nothing will be reported for it)
```

That is the feature working correctly. Rebuild `s4.debug.bin` from current `main` and it resolves.

Aeon's own `docs/DEFERRED_WORK.md` records the symbol as `0xFFFFE50D` in the debug shape. **That address
is quoted, not verified** — no listing available here contains the symbol yet. It does not matter to the
config, which names the *symbol* and lets the listing supply the address; it is noted only so nobody
hardcodes it.

---

## The config key

`symbol_watch = <Symbol>: <label 0>, <label 1>, <label 2>, …`

* **The key may repeat.** Each line is one watch and they accumulate in file order. That is the natural
  spelling in a flat `key = value` file; there is no nesting to invent.
* **The colon is optional.** `symbol_watch = Level_Id` is a legitimate "just tell me the number" watch.
* **Labels are positional.** `a, , c` means index 1 has no name while index 2 is still `c`. A blank entry
  is a gap, not a shorter list, and it survives a round trip.
* **A label may contain a colon** (only the first colon splits). **A label may not contain a comma** —
  that is the one thing this grammar cannot spell.
* **A blank value is no watch.** The key is always written to the file, blank if nothing is configured, so
  a user who has never heard of it can find it by reading their own settings.
* Whitespace around the symbol and around each label is trimmed.

The file's existing rules all still apply: an unparseable value warns per-key and keeps going, and a
hand-edited file survives every in-app autosave byte-for-byte.

---

## What it prints

Four shapes, because there are four honestly different things to say. Only the first is the format the
owner asked for; the other three name the symbol, because without a label there is nothing else in the
line to say which watch spoke.

| situation | line |
|---|---|
| value has a label | `8/20 — Haze` |
| value's label is blank | `13/20 — <no label> (Debug_Scene_Index)` |
| value past the end of the list | `Debug_Scene_Index = 23 — outside the 20 labels configured` |
| no labels configured at all | `Level_Id = 12` |

**It never invents a name, and it never goes quiet.** House rule: refuse rather than guess, but a feature
that silently does nothing is indistinguishable from one that is working and has never fired. `13/20 —
<no label>` beats showing nothing and beats making a name up. `24/20` would be a lie about the list's
length, so out-of-range prints the raw value and states the shortfall instead.

### Failures are loud, once, in red

Every watch that cannot be armed produces one `ERROR` toast **and** a stderr line, at startup and again
after a ROM reload:

* no `.lst` loaded at all (or the player refused one that described a different build)
* the listing has no such symbol
* the symbol is not in work RAM (a ROM symbol never changes, so watching one looks identical to a broken
  watch, forever)

One bad entry never disarms the good ones.

---

## Design calls

### The scoping call — labels are DATA, and this is an argued departure

Aeon asked for the *specific* version: hardcode the symbol and the twenty names in Rust, on the reasonable
grounds that a smaller parcel is better. The overseer overruled that in one direction, and the ruling is
right:

Those twenty names are **aeon's vocabulary for aeon's feature**. Baking them into this emulator's source
ships another tool's model inside oracle — the array goes stale the day aeon renames a scene, nothing in
this repo's gates would notice, and the next lane that wants the same readout gets nothing. This repo
already refuses to name another tool's tile-blob slots for exactly this reason.

Everything else stayed as dumb as aeon asked. There is **no** expression evaluation, no second watch kind,
no format template, no scripting surface. One symbol, a list of labels, a toast on change. Allowing the
key to repeat cost one `push` instead of one `=` and one loop in the serializer, which is inside "nearly
free".

### ⚑ It reads a BYTE. The brief said "word", and the brief was wrong.

The parcel brief specified "a RAM word". The addressed symbol is not one:

```
aeon/games/sonic4/config/ram.emp:230:   Debug_Scene_Index:  u8,   // effects-lab cursor: index into SCENES[]
aeon/games/sonic4/test/ojz_scroll_test.emp:1372:   move.b  Debug_Scene_Index, d0
aeon/games/sonic4/test/ojz_scroll_test.emp:1385:   move.b  d0, Debug_Scene_Index
```

It is a `u8`, the game touches it with `move.b`, and aeon's own note puts it at `0xFFFFE50D` — an **odd**
address. A word read there would splice in the neighbouring byte and report a number the game never held.
Twenty labels cannot overflow a byte either.

Pinned by `the_read_is_one_byte_and_ignores_its_neighbours`, which moves both neighbours and asserts
silence — so a later "make it a word" edit fails there rather than quietly reporting nonsense.

**Follow-up, not built:** there is no width knob. A word-sized counter cannot be watched today, and adding
one silently (rather than declaring the width in the config) would be the same confidently-wrong failure in
the other direction. If a lane needs it, the honest shape is an explicit suffix in the config value.

### The first-poll ruling

The brief asked for two things that look contradictory: don't fire on the first read (there is no
"change" yet), but don't swallow a genuine change that happens on frame 1.

**Both hold, because the baseline is seeded at arm time from the machine's actual RAM** — power-on zeros
at startup, or whatever a reloaded ROM left behind. There is therefore no "first read" that fires with
nothing to compare against, *and* a change during the very first emulated frame is still a change against
a real prior value and still fires. "Skip the first poll" cannot manage both; it would swallow frame 1.

Pinned from both sides: `an_unchanged_value_is_silent_on_the_first_poll` and
`a_change_on_the_very_first_poll_still_fires`.

### It cannot perturb the machine

`SymbolWatch::poll` takes `&[u8]` — the borrow `oracle_core::system::System::ram` hands out. No bus cycle
is issued, no clock advances, no VDP port is read, and the argument is immutable at the type level, so no
future edit can change that without changing the signature. The RAM index is computed with the same
`& (RAM_SIZE - 1)` mirror the core bus applies to `$E00000-$FFFFFF`, so it reads the byte the 68000 would
read at that address.

### Re-armed on ROM reload

The reload path already re-reads and re-validates the `.lst`, because a rebuild moves symbols and a cached
table names the previous build's addresses while looking perfectly healthy (the suite contract's D7
incident). A watch holding a stale RAM index is that same failure one address wide — it would keep
reporting confidently wrong scene names from whatever now lives there. So the watches are re-armed
against the new listing after the reset, which also re-seeds the baseline from the machine that is
actually running, and complains out loud about any symbol the new listing lost.

### Where the poll sits

Once per loop iteration, after **both** things that can advance the machine (the local frame loop and
`bus.pump`), so a client-driven `run_frames` is seen exactly like a locally-run frame. An iteration that
ran two emulated frames reports only where the value ended up — the honest answer for a readout whose
whole job is "what am I looking at now".

---

## What is NOT verified

* **Nobody has seen this on a window.** The implementing session has no display and neither does the
  overseer. The overlay wiring in `main()` — arming at startup, polling per iteration, re-arming on
  reload, `ACCENT` for a change and `ERROR` for a failure — is verified by **compilation and code review
  only**. There is no test around the render loop, and I have not claimed one.
* **No end-to-end run against a real ROM.** `s4.debug.bin` does not currently contain the symbol (see
  above), so the only thing a run today could demonstrate is the complaint path.

**Eyes needed on a window:** rebuild `s4.debug.bin` from current aeon `main`, paste the config line above,
run `cargo run --release -p oracle-frontend -- s4.debug.bin`, and press the scene hotkey. Expect
`1/20 — OJZ Default` … `20/20 — OJZ Locked Clouds` in the bottom-left corner, one toast per press.

---

## Gates

| gate | result |
|---|---|
| `cargo test --workspace` | LEGS=56 PASSED=1900 FAILED=0 IGNORED=6 (baseline 1880; **+20** = the new tests) |
| `cargo test -p oracle-frontend --no-default-features` | 247 passed, 0 failed, 1 ignored (baseline 227; **+20**, same tests) |
| `cargo clippy --workspace --all-targets` | exit 0, zero warnings |
| `cargo clippy -p oracle-frontend --no-default-features --all-targets` | exit 0, zero warnings |
| `cargo fmt --all -- --check` | exit 0 |
| `crates/oracle-core/tests/` diff | **zero files touched** — no currency movement |

Nothing is feature-gated, so both build variants get the same twenty tests and the same delta.

### Red-first

Every new test was proved red by a named mutation of the code it guards (23 poisons, run one at a time,
each checked to actually compile — a mutation that fails to build proves nothing). Two of them are
**controls against the opposite failure**, because a "loud" assertion can be satisfied by shouting at
everything and a "silent" assertion by never speaking at all:

* `P14 poll-always-empty` — proves the silence assertions are not passing because nothing ever fires
  (it kills five reporting tests).
* `P15 always-complain` — proves the complaint assertions are not passing because arm complains about
  everything (it kills `a_resolvable_ram_symbol_arms_and_says_nothing`).

The harness itself needed one correction mid-run: its compile-error detector was matching cargo's
`error: test failed` line and reporting all 21 poisons as invalid. Fixed to key off `could not compile`.
