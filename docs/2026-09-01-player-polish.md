# PLAYER-POLISH — six fixes in the player's own window (2026-09-01/02)

Branch `parcel/player-polish`, off `2fd5bb0`. Four registered defects in the frontend, one label ruling
from the decision ledger, and one owner ruling relayed from the suite. Nothing in this parcel moves a byte
of emulation: no core file is touched, no golden is regenerated, and every code change is in
`crates/oracle-frontend`.

| Item | What | Commit |
|---|---|---|
| A | `F-FONT-BACKTICK` / `F-FONT-EMDASH` — glyphs for the characters the player prints | `d845e1e` |
| B | `F-TOAST-TRUNCATES` — a cut toast says so, and reasons come before paths | `5d2978d` |
| C | `F-PICKER-FILTER-MARKER` — the picker filters on the label, not the `[loaded]` marker | `f336658` |
| D | `F-ROMOPEN-C-DOC` — an unreadable folder leaves the previous listing up | `e5f57c4` |
| E | ledger `L-08` — the status line's `F n` becomes `DRAWS n` | `6853a17` |
| F | owner ruling d-20 `remember-choice` — the console audio filter is a remembered setting | `d13e6c9`, `35e3cda` |

Alongside those seven: `e7b81b9`, a review fix to item E's control (described in that section), and the
commits carrying this document. `git log 2fd5bb0..` is the count; it is deliberately not restated here,
because a self-referential total in a file that is itself a commit is stale the moment it is written —
which is how it was wrong the first time.

Every test below was proven red before it was made green: the code was put back to the behaviour it
replaces, the failing assertion was read, and the change restored. The failure texts are quoted in the
commit messages so the proof survives without this file.

---

## A — the font could not draw what the player prints (`d845e1e`)

**What changed.** `font.rs` gained four glyphs: backtick, tilde, em dash and the horizontal ellipsis.
`screen_text.rs` gained `every_string_literal_the_frontend_can_show_is_drawable`, which walks the module
tree from `main.rs`, lexes the string literals out of each module's production region, and asserts every
character has a 5x7 glyph.

**How verified.** Red-first, the new test reported **62 undrawable literals**, the first being
`main.rs: ": — cannot read " lacks glyphs for ["—"]`. The two premise tests that used to measure
themselves against the em dash were re-measured against characters the font still cannot draw
(`→`, `·`), so they state a true premise rather than a stale one.

**Open.** The lexer covers string literals, not `format!` output assembled from runtime values; a path
or a symbol name containing an exotic character still renders as a hollow box. That is the correct
behaviour (the box is honest), and the fixed strings are what this test exists to hold.

Deviation from the brief, recorded: **four glyphs, not two.** The tilde came out of the lexer sweep —
the usage text prints one — and the ellipsis is the character item B needed for its truncation mark.
Adding it here rather than in B keeps "the font can draw it" in one commit.

---

## B — a truncated toast now says it was truncated (`5d2978d`)

**What changed.** `overlay::fit_marked` returns the whole string when it fits and a head plus
`TRUNCATION_MARK` (`…`) when it does not, reserving the mark's own width from the budget.
`visible_toasts` composes through it, so `draw_toasts` and `text_surfaces` cannot disagree about what
reached the glass. The cannot-read toasts were re-ordered to put the **reason before the path**
(`cannot_read_toast`), because the path is what a cut removes and the reason is what the reader needs.
`io_reason` strips the trailing ` (os error N)` — errno is redundant next to the text and cost the
reason its place in the budget.

**How verified.** Red-first three ways: `left: "ABCD" right: "ABC…"` for the mark itself; *"the rendered
toast must be 46-of-48 glyphs plus the mark"* for the budget arithmetic; and with the path first,
*"the reason must come before the path"*. Capacity is derived from `toast_text_avail` and the font's own
advance, never transcribed: at the 320x224 floor it is 47 glyphs.

**Open — TAG for the controller.** `crates/oracle-aether/tests/contract/bus-protocol.schema.json`
describes `rendered` as "a prefix of `text` today". With the mark appended that is no longer literally
true for toasts (it is a prefix plus one character). The protocol file was **not** touched, per the
parcel's rules; this is a description-only re-vendor candidate. `screen_text.rs`'s own doc for
`Surface.rendered` has been corrected in-tree.

---

## C — the picker filtered on text it was only painting (`f336658`)

**What changed.** A `PickerItem { label, marker, cmd }` replaces the `(String, Cmd)` pair: the label is
what the filter matches, the marker is painted after it. `rom_browser::picker_marker` supplies the
`   [loaded]` suffix. Filtering, drawing and selection now read the same structure.

**How verified.** Red-first: *"the loaded ROM survived a filter that only matches its marker
left: ["s4.bin"] right: []"*. The filter string in the test is derived — the letters of the marker that
do not occur in the label — so it cannot accidentally be a label match.

---

## D — an unreadable folder now leaves the previous listing up (`e5f57c4`)

**What changed.** The ROM browser's listing moved into a `RomBrowser { dir, entries }` that outlives a
failed scan. `open_rom_picker` re-opens the picker on the retained entries (the folder is **not** read
again) alongside the toast. With no prior listing there is nothing to restore and the toast is all there
is, as before.

**How verified.** Red-first, with the old early return: *"the palette must come back up, not stay
closed"*. The test drives a real temp directory and a real `io::Error`, and asserts the restored item
list, the title and the toast list whole.

**Why this direction.** `docs/2026-08-28-rom-open.md` §5(c) already promised this behaviour. Doc and code
disagreed; the doc was the older, more considered statement, so the code moved.

---

## E — `F n` becomes `DRAWS n` (`6853a17`), and what it cost

**What changed.** `Status.frame` → `Status.draws`; the status line reads `DRAWS 1234`; the CPU chip reads
`DRAWS 42`; the window title reads `Oracle — draws N`. Per ledger **L-08** this is a relabel and
explicitly *not* a sync: the window's number counts run-loop iterations (it keeps moving at a breakpoint
halt, which is the whole reason it is useful), while the bus's `frame` is derived from the emulated clock
and correctly freezes there. The hazard L-08 names is a reader joining the two.

**How verified.** Red-first, with `status_text` still composing `F{}`:
`left: "…320X224 F1234"  right: "…320X224 DRAWS 1234"`. The new test asserts the **whole** composed line
and then, as a control, that no word on it is `FRAME` or the old `F<digits>` shape.

**The cost, stated with numbers.** The status line's budget in *glyphs* is not monotonic in window size,
because `status_font_scale` steps 2→3 at exactly 896 while the picture grows only 4:3:

| window height | status font scale | text budget | glyphs |
|---|---|---|---|
| 224 (floor) | 1 | 204 px | 34 |
| 448 | 1 | 503 px | 84 |
| 672 | 2 | 712 px | 59 |
| **896** | **3** | **920 px** | **51** |
| 1080 | 3 | 1166 px | 64 |
| 1440 | 3 | 1646 px | 91 |

The fixture line is 46 characters plus the tally: `F1234` made 51 — fitting the 896 dip **to the byte** —
and `DRAWS 1234` makes 56. Three tests claimed the whole line survives at 896. Each now states what
actually renders there, asserted whole: the tally's digits are cut, both honesty fields (`AETHER OFF`,
`AUDIO VA0-VA2`) survive, which is exactly what the field order exists to arrange.

**That row was pinning the fixture, not the player.** The control beside it *constructs the old label's
line* — the same five leading fields, taken from `status_text`'s own output, then `F` and the digits — and
sweeps the digit count against the same 896 budget. It fits at four digits (so the row was not wrong when
it was written, only narrow) and stops fitting at **five**: from the 10,000th draw on, i.e. under three
minutes at 60 fps. The claim "the whole line survives at 2x and above" was true of a four-digit test
value, not of the running window.

The first version of that control measured the **`DRAWS`** line instead and asserted it overflowed. That
was a true statement resting on a measurement that could not fail for the reason it named: `DRAWS n` is
strictly the longer string, so its overflow implies nothing about `F n`, and the control would have stayed
green at a budget where the old label comfortably fit. Caught in review, fixed in `e7b81b9`, and proven in
both directions: widening the budget by three glyph cells makes the claim false and the control now says
so (`left: 6  right: 5`), and building the wrong string trips the four-digit premise instead. The
corrected sweep is also what moved the figure — the earlier prose said six digits and seventeen minutes,
which was wrong twice over (six digits is 27.8 minutes; the real threshold is five digits and 2.8).

**The alternative, so the controller can overrule.** The only field that fits 51 glyphs is five characters
including the digits — i.e. a bare letter and a number (`D1234`). That fits every existing row without
changing a test, and it is unreadable: a reader who has to ask "what is D?" is no better served than one
who joins `F` to `frameToken`. `DRAWS n` was taken as the controller's stated default; if the 896 row's
old claim is worth more than the word, `D1234` is the swap and it is a one-line change to
`status_text` plus the three rows.

---

## F — the console audio filter is a remembered setting (`d13e6c9`, `35e3cda`)

**This is a per-user preference, not an accuracy deviation.** The default is unchanged and is still the
hardware filter (`Model1Va0Va2`, picked by ear 2026-08-15); nothing about the emulated output moved.
Owner ruling **d-20 `remember-choice`**, relayed as empyrean
`4e8e865b7c6e821cc23cb3683776aa71243cac0b`: *"his own rule is accuracy stays and taste deviations do not,
so the default stays the hardware filter; his setting persists between runs the way volume does."*

A future report of "the sound changed" is therefore diagnosable from the setting, and deliberately so:
the startup line names both the stage in use **and where the choice came from**.

```
audio: console output stage = model1-va3-va6 (low-pass 2842 Hz) — remembered in player.conf; F cycles it (remembered), ORACLE_CONSOLE_FILTER=off|va0|va3 overrides for one run
```

**What changed.**

* `config.rs` — a `console_filter` key taking `va0` / `va3` / `off`, the short spellings
  `ConsoleModel::from_name` already accepts. Stored as text, not as a `ConsoleModel`, so a
  `--no-default-features` build with no synth still round-trips the file untouched. A **blank** value means
  "nothing chosen yet", not "off" (the same always-present-key convention `symbol_watch` uses), which is
  what lets the player honestly print `default`. `KNOWN_KEYS` is 8 → 9.
* `main.rs` — `resolve_console_filter(env, conf) -> (model, provenance)`, a pure function: the one-run
  `ORACLE_CONSOLE_FILTER` override beats the remembered choice beats the built-in.
* `commands.rs` — `Cmd::CycleConsoleFilter` under `#[cfg(feature = "audio")]`, in the palette's SETTINGS
  group as *"Audio filter: VA0-VA2 / VA3-VA6 / raw"*, bound to **`F`**.
* On a change: a toast naming the new stage and what it does (`audio: VA3-VA6, low-pass 2842 Hz —
  remembered`), the status line's `AUDIO …` field follows, `AudioSink::set_console_model` applies it live,
  and `cfg.console_filter` rides the existing two-second autosave debounce.

**One behaviour deliberately changed.** An unparseable `ORACLE_CONSOLE_FILTER` used to fall back to the
built-in default. It now warns and falls through to the **next source** — the remembered choice, then the
built-in. Before this ruling there was nothing to fall past; now the old behaviour would silently discard
the listener's own pick on a typo.

**The hotkey.** `F` was free: the game keys are the arrows, A/S/D and Enter, and the frontend's letters
were W, C, M and O. `key_name` names it, and `hotkeys_unique` (which already existed) is what proves there
is no collision.

**With no output device** the cycle is refused with a red toast rather than recorded blind — a setting
nobody could hear is not a choice, and writing it would leave a remembered preference the listener never
actually evaluated.

**How verified.** Six tests, each red-first against the behaviour it replaces (failure texts in
`35e3cda`): precedence over all six (env, conf) combinations including both typo cases; the startup line
whole, for each of the three provenances, with a control that the three words differ; the file spellings
and `ConsoleModel::ALL` proven to be the same three in both directions; the cycle proven to visit every
revision and wrap; the toast proven to fit a toast *whole* at the 320x224 floor, against the overlay's own
budget arithmetic; and a source-level control that `ORACLE_CONSOLE_FILTER` is read in exactly one place
and `cfg.console_filter` written in exactly one, from the **sink's** model rather than from the
environment — which is what makes "the override is never written back" a checked claim rather than an
intention.

**Open.** Nothing hears the filter in CI. Everything above is register- and string-level; the audible
half is the owner's, as it was for SY-5.

---

## What is left open, in one place

1. **The vendored protocol schema is behind upstream, and that is not this parcel's to fix.** Empyrean
   adopted §11.31 (`82982b7`, 2026-09-02 00:43) while this branch was in flight, so
   `the_vendored_schema_is_byte_identical_to_the_upstream_contract` fails — on untouched `main` as well,
   confirmed as a control. The re-vendor has to land **together with serving the new required field**, or
   every `emulator/stopped` fixture under `crates/oracle-aether/tests/` goes red with it. Owner's lane.
   The item B description staleness below folds into that same re-vendor.
2. **Contract description staleness (item B)** — `bus-protocol.schema.json` still says `rendered` is "a
   prefix of `text` today", which the truncation mark makes a prefix *plus one character*.
   Description-only; the file was not touched.
3. **The 896 dip (item E) — settled, keep `DRAWS`.** Three rows now state a truncation they used to
   forbid. The alternative (`D1234`, the only field that fits 51 glyphs) was declined on review: the tally
   is deliberately last in truncation order, the dip is one intermediate window height, and stating what
   actually renders beats gaming the budget. Recorded so the question is not reopened for free.
4. **The audible half of item F** — unverifiable here (no `/dev/snd`). Owner's lane.
5. **`filter_effect` prints a rounded cutoff** — `low-pass 3386 Hz` for 3386.3. Deliberate: the status
   line and the toast are for reading, and the exact constants live in `console_filter.rs`.
