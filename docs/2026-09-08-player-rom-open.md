# Opening a ROM from inside the toolkit player — three routes, one sequence

**Date:** 2026-09-08 · **Parcel:** `PLAYER-ROM-OPEN` · **Branch:** `feat/player-rom-open`
**Predecessor:** [`2026-08-28-rom-open.md`](2026-08-28-rom-open.md) — the same feature for the minifb window.

## 0. What was asked, and what was approved

The owner asked *"how much would it take to add a file browser or something that I don't need params like
that with?"*, was given three options, and approved **option 1 + option 2 together**: drag-and-drop **and**
an in-window browser, both funnelling into one shared `open(path)`, landing as one change rather than two
half-correct paths. He also approved the recommendation on the parcel's single open design question (§3).

## 1. What was true before

`crates/oracle-player` took its cartridge from `--rom` and from nowhere else.

* `F5` re-reads **the same** path (`input::MachineKey::ReloadRom` → `ui::RELOAD_ROM` with no `path`).
* The only route to a *different* game was the `⌨ commands` palette: pause, type `emulator/reload_rom`,
  type `{"path": "/…/s4.bin"}`.
* A client on the bus, meanwhile, could always swap the cartridge.

So the asymmetry the 08-28 page recorded for the minifb window — *the tools could change his ROM and he
could not* — was still true for the window that replaced it.

## 2. What shipped

### 2.1 `rom_browser` moved into `oracle-frontend`'s lib

`crates/oracle-frontend/src/rom_browser.rs` is now `pub mod rom_browser` in that crate's `lib.rs`, and the
binary reaches it through the lib the way `icon` already does (`pub(crate) use oracle_frontend::rom_browser;`),
so every existing `rom_browser::` call site in `main.rs` is unchanged. The migration precedent is
`save_state`/`sram_file`/`spawn`.

**Both windows must offer one listing, one ordering, one `[loaded]` rule.** A second implementation would be
a second answer to *which cartridge is running*, and the marker's whole job is to answer that.

**No feature gate, no new dependency — measured, not assumed.** The module is std-only (`std::path` plus one
`read_dir`):

| check | result |
|---|---|
| `git diff 8ca3056..HEAD` over `Cargo.lock` and every touched crate's `Cargo.toml` | **empty** |
| `cargo tree -p oracle-frontend`, base vs. tip, `diff -q` | **identical** |
| `cargo tree -p oracle-core`, base vs. tip, `diff -q` | **identical** |
| `cargo tree -p oracle-core \| grep -icE 'egui\|eframe\|wgpu\|winit'` | **0** |
| `cargo tree -p oracle-frontend \| grep -icE 'egui\|eframe\|wgpu\|winit'` | **0** |

**Two things the move would have taken out silently, and did not:**

1. **`picker_label` was `#[cfg(test)]`.** A `cfg(test)` item in a **lib** is invisible to the **bin**'s tests
   — the bin links the lib as a dependency, with `test` off — so the two assertions that read it would have
   *vanished* rather than failed. It is now `pub` and documented as the one written-down spelling of the
   marked row, which each window's tests check their own composition against.
2. **`the_loaded_marker_is_painted_but_not_filtered_on` reached `crate::palette` and `crate::commands`** —
   this binary's modules. It moved to `main.rs`'s test module, where the seam it tests actually is, with
   nothing changed about what it asserts. Confirmed by name in the bin leg's log, while the four model tests
   now appear in the **lib** leg's.

### 2.2 The control — `crates/oracle-player/src/rom_open.rs`

**A CONTROL, not a `ui::Tab`**, per the owner's amended rule in `palette.rs`'s header (*a gesture you hit and
forget is a control; a standing surface you read is a tab*). `Ctrl+O` through `consume_shortcut`, a button
beside `⌨ commands`, no `Tab` variant (which would owe `layout::LAYOUT_VERSION` a bump and discard the
owner's stored layout), and **nothing persisted across launches**.

**The model is egui-free and that is the parcel's central structural property.** `folder_of`, `RomOpen::rows`,
`RomOpen::activate`, `decide_drop`, `reload_params`, `RomOpen::rescan`, `RomOpen::typed` and
`RomOpen::act_on_drop` are all callable with no window; `show` draws them and decides nothing — a keystroke
or a click sets a flag and `activate`/`run_action` resolve it *after* the draw closure has closed. That is
not tidiness: §5 of the 08-28 page is the frontend's own confession that its swap block *"is not covered at
all"*, precisely because its deciding happens inside a run loop nothing can call.

### 2.3 The three routes and the one sequence

| route | where it enters | what it produces |
|---|---|---|
| a row's Enter or click | `show` → `activate` | `Action::Open` / `Action::Descend` / `Action::None` |
| a pasted path in the box | `typed` → `rows` (first row) → `activate` | the same three |
| a dropped file | `dropped_paths` → `decide_drop` → `act_on_drop` | `Open` / `Browse` / `Refused` / `Nothing` |

All three end in **`RomOpen::open`**, which is exactly:

1. `screen_pick::paused_for(...)` — **called, never re-implemented.** It is this crate's one implementation of
   *"the machine really was paused while the body ran, and it is where it was afterwards"*, and it is a
   closure-taker precisely so that is directly assertable.
2. `Bus::call("emulator/reload_rom", {"path": …})`. The registry declares that key set **closed**
   (`METHODS`' row: `params: &["path"]`) and `Engine::dispatch` refuses any other top-level key `-32602`
   *before the handler runs*, so there is no second key to add.
3. The run state put back by `paused_for`, reported through `spawn_picker::RunState::sentence_of` with this
   module's own `Deed` (`OPENING`) — *"paused the machine to place the object"* would be a false account of
   a cartridge swap.
4. On a swap that **landed**, the listing re-taken on `folder_of(bus.rom_path())` — the bus's own *new*
   answer. On a swap that was **refused**, nothing.

The reply, success or refusal, is echoed **verbatim** through `ui::Echo` (code + message + bracketed
`error.data.reason`), and `remedy` is keyed on `reason` and never on prose.

#### 2.3.1 ⚑ Step 4 was got wrong, and the way it was wrong is the point

**Shipped defect, found by the controller's verification pass and fixed on this branch
(`500b23b`).** `open` re-listed **`self.dir`**, and `act_on_drop`'s `Open` arm set `attempted = false`
meaning to re-derive the folder instead — with the comment *"the folder the dropped image came from, so the
listing describes what was just loaded"*. **That line was then overwritten**: `open` succeeds, reaches the
`rescan(&self.dir)`, and `rescan` sets `attempted = true`, so `ensure_listing` never re-derived. A file
dropped from a folder other than the one being browsed left the window on the **old** folder. Measured with
a probe:

```
cartridge now   = .../elsewhere/there.bin
listing dir now = .../browsed
attempted       = true
rows            = ["../", "here.bin"]
```

Every row's `[loaded]` marker is a claim about **which cartridge is running**. On that picture **nothing is
marked** and the image that *is* running **is not on the list** — a believable wrong answer rather than a
missing one. The two comments in `open` and `act_on_drop` asserted opposite intents, so only one could ever
have been the design.

**The fix is `folder_of(bus.rom_path())`, and it unifies the routes rather than special-casing the drop:**

| route | before | after |
|---|---|---|
| a row's Enter | correct by coincidence — `self.dir` *is* the new cartridge's folder | unchanged; nothing visibly moves |
| a pasted path from elsewhere | **stale listing, marker on nothing** | repoints; marker on the image just loaded |
| a drop from elsewhere | **stale listing, marker on nothing** | repoints; marker on the image just loaded |

It is also the rule this feature already states everywhere else — *the folder the running image came from*
(`folder_of`, `ensure_listing`, `oracle-frontend`'s `Cmd::RomPicker`). The dead `attempted = false` is gone
and both comments now say what the code does.

**Refusals — previously uncovered and undocumented, now decided.** A refused swap re-lists **nothing**: the
cartridge did not move, so the listing and its marker are still correct, and re-taking them would be a folder
read nobody asked for on the one path where nothing changed. `attempted` makes two states of that, and only
one had ever been reasoned about:

1. **A listing is up** → left exactly as it was, marker included.
2. **No listing has ever been taken** (a drop onto a control nobody opened) → `attempted` stays `false`, so
   the next repaint derives one from the **unchanged** `rom_path` and marks the cartridge still running.

A rescan that itself fails leaves `said` carrying the folder error beside a `last` reporting the successful
load. Both are true and both are shown: the swap happened *and* the new folder cannot be listed.

**`battery.rs` is unchanged, and that is not an omission.** `Battery::carry` takes the pending battery image
at the top of *every* `bus::drain` — before the pump and before the `build_ui` this control is drawn in — and
`after_replacement` writes it against the outgoing cartridge once the machine has moved. A swap from this
window is the same event as a socket client's, and that module already answers both. `states::after_replacement`
is likewise driven off `drained.rom_path`. (This is why **`F-BUSSWAP-PATHS`**, registered against the *minifb*
window by the 08-28 parcel, does not apply to the player: `bus::drain` re-derives `rom_path`, the `.srm` and
the slot keys from the engine's own answer.)

### 2.4 Drag-and-drop — §4's blocker was minifb-specific

Read from the crate sources rather than assumed:

* `egui-winit-0.36.1/src/lib.rs:474` — `WindowEvent::DroppedFile(path)` pushes into
  `self.egui_input.dropped_files`.
* `egui-0.36.1/src/data/input/raw_input.rs:81` — `pub dropped_files: Vec<DroppedFileHandle>`.
* `egui-0.36.1/src/data/input/dropped_file.rs` — `DroppedFileHandle = Arc<dyn DroppedFile + Send + Sync>`,
  and `DroppedFile::path(&self) -> &Path`, documented as *"an absolute path on native platforms"*.

So the toolkit hands us the path for free, `dropped_paths` is the whole of the reading, and **`d-18`/`d-19`'s
premise — *"our window library does not model drops"* — does not hold for `oracle-player`.** It held for
minifb 0.28, which models no drop of any kind.

**The four drop rules, each a decision:**

| dropped | answer | why |
|---|---|---|
| one ROM image | `Open` | the point of the gesture |
| one directory | `Browse` | an obvious *show me what is in here*; refusing it answers *no* to something with one right answer |
| one non-image file | **`Refused`**, said on screen, `reload_rom` never called | `Engine::reload_rom` does `fs::read` + `load_rom` with **no header check**, so a silently loaded `.txt` runs as garbage — a believable wrong answer, not a missing one. The gate is `rom_browser::is_rom`, the same predicate the listing filters on, and the refusal names the extensions from `ROM_EXTS` rather than transcribing them. |
| more than one file | **`Refused`**, said on screen | a Genesis holds one cartridge. Picking the first of five is the window choosing a game on the person's behalf and being wrong four times out of five. |

`Browse` on a dropped directory is the one rule not in the brief. Taken deliberately and recorded here as a
design call.

### 2.5 The typed path closes what `d-19` chose and never shipped

`d-19`'s adopted option was *"if what you type looks like a path, it offers that file directly"*. **It was
never built in the minifb window** — confirmed by grep before writing this down: `Picker::visible` filters
through `commands::subseq_match` on `item.label` alone (`palette.rs:100-107`), and nothing in
`crates/oracle-frontend/src/` tests whether the query names a file on disk (`grep -rniE 'is_file|paste|typed.?path'`
over that tree finds only `screen_text.rs`'s `.lst` probe and one doc comment).

The player now has it: an existing image is offered as the **first** row, so paste-then-Enter opens it with
the selection where it starts; an existing directory descends; an existing file that is not an image is **not
offered** and is named as the reason; a path that does not exist is simply a filter. **This does not close
`d-19` for the minifb window.**

## 3. The one open design question, and the owner's answer

*Does opening a ROM on a running machine refuse, or pause?* **It pauses, reloads, and puts the prior run
state back** — the owner's approved answer. Opening a ROM implies playing it, and a refusal for a state the
window could have fixed itself is a worse surface than one that fixes it and says so. `paused_for` already
*is* that sequence, including its two quiet cases: a machine somebody else paused is left paused, and a pause
that cannot be read back leaves the machine stopped rather than starting one a person deliberately stopped.

The palette keeps the plain unpaused path, so `-32005 machineRunning` stays reachable and tested there.

## 4. What was verified firsthand, with real exit codes and aggregate totals

Every figure is `$?` on an unpiped command, and every total is the **aggregate across all legs**, not a tail.
`awk` over every `^test result:` line, so a leg cannot be dropped by a screenful.

| gate | result |
|---|---|
| `cargo fmt --all -- --check` | **exit 0** |
| `cargo clippy --all-targets -- -D warnings` | **exit 0**, 0 warnings |
| `cargo test -p oracle-player` | **exit 0** — `LEGS=2 PASSED=376 FAILED=0 IGNORED=0` |
| `cargo test -p oracle-frontend` | **exit 0** — `LEGS=5 PASSED=389 FAILED=0 IGNORED=1` |
| `cargo test -p oracle-frontend --no-default-features` | **exit 0** — `LEGS=4 PASSED=90 FAILED=0 IGNORED=0` |
| `cargo check -p oracle-frontend --no-default-features` | **exit 0** |
| `cargo build -p oracle-frontend --no-default-features --features audio,aether` | **exit 0** |

**Accounting for the player leg, as arithmetic rather than a second measurement.** The baseline on `8ca3056`
was measured directly (not derived): `LEGS=2 PASSED=362 FAILED=0 IGNORED=0`. 362 + 12 `rom_open` tests from
the first pass + 2 from §2.3.1's fix = **376**, exactly, and all fourteen appear **by name** in their run's
log. (The controller's independent pass confirmed the *frontend* baseline on `8ca3056` is also 389, so the
lib move lost no tests, and counted +12/−0 `#[test]` in the first pass's player diff.)

### 4.0 ⚑ ONE FAILURE OBSERVED ONCE AND NOT REPRODUCED — booked, not written off

During §2.3.1's fix, a single `cargo test -p oracle-player` came back `PASSED=371 FAILED=1` (one leg, 372
rows). **It has not been reproduced in 62 subsequent runs**: 8 plain, 5 with a concurrent
`cargo clippy --all-targets` hammering the CPU, 4 repeating the exact `fmt → clippy → test` compound that
produced it, 30 of `loop_tests` alone (the only wall-clock-sensitive rows), and 15 of the whole suite at
`--test-threads=1`.

**I cannot name the test, and the reason is my own filter.** That run was piped through
`grep -E "^test result:" | awk …` to compute the aggregate, which **discarded the `… FAILED` line and the
panic**. A filter narrow enough to produce a total is narrow enough to destroy the diagnosis. The harness
scripts now print the totals **and** any failing name **and** the panic.

Recorded as an observation with a narrow window rather than as a flake, per this lane's own standing rule
that *a load-only failure is a defect with a narrow window, not a flake* — written off twice before. The
command that produced it is in this section; anyone who sees it again should capture the unfiltered output.

Two things the repo's own gates caught, recorded because in a tail they look like a regression and like
nothing respectively:

* **`p10_no_dashes_in_shipped_text` found four em-dashes in shipped strings** in the first draft of
  `rom_open.rs`. Rewritten as colons and full stops.
* **`cargo test -p oracle-frontend` failed 8 `save_state` rows in this worktree** for a **missing
  `vendor/TestRoms`**, not for anything in the diff — the tests say so in their own panic and refuse to skip
  silently, which is why it was diagnosable. `vendor/` is gitignored and was symlinked in from the main
  checkout; the eight then pass. **"8 failed" and "a real regression" are the same artifact in a tail.**

### 4.1 New tests, by name

`crates/oracle-player/src/rom_open.rs`:

1. `enter_opens_the_row_the_filter_left_on_screen` — ★★ the load-bearing one
2. `descending_and_coming_back_keeps_the_loaded_marker_on_the_running_image`
3. `an_unreadable_folder_keeps_the_previous_listing_and_says_why`
4. `a_pasted_path_is_offered_and_a_pasted_non_image_is_refused`
5. `open_runs_on_a_paused_machine_and_restores_the_prior_run_state` — ★★
6. `the_path_is_sent_verbatim_under_the_registry_s_only_declared_key`
7. `a_bus_refusal_is_echoed_verbatim_and_the_cartridge_did_not_change`
8. `a_dropped_non_rom_is_refused_and_reload_rom_is_never_called` — ★★
9. `the_control_reports_what_it_drew_and_reports_nothing_while_closed`
10. `the_folder_listed_is_the_running_cartridge_s_own`
11. `the_shortcut_is_spelled_once_and_collides_with_nothing`
12. `the_selection_cannot_walk_off_the_visible_rows`
13. `a_swap_from_another_folder_repoints_the_listing_and_the_marker_follows_the_cartridge` — ★★ §2.3.1's
    defect. **Three distinct folders**, which is what the existing drop test could not have: it drops an
    image from the folder it is already listing, where `self.dir` and `folder_of(rom_path)` coincide and the
    bug is invisible. It asserts the repoint, the `[loaded]` marker landing on the dropped image, the running
    image being **present** in the rows, and the previous one being **absent** — and it drives **both** entry
    points (`act_on_drop(Dropped::Open(..))` and `open(..)` direct), because the fix belongs to the shared
    sequence and a fix living in the drop arm would satisfy the first leg only.
14. `a_refused_swap_leaves_the_listing_alone_and_still_describes_the_running_cartridge` — both `attempted`
    states, the second driven through the real `show` since `ensure_listing` is what closes it.

Plus one assertion added to `crates/oracle-player/src/main.rs`'s
`loop_tests::a_client_reads_this_windows_top_bar_and_it_follows_the_run_state` — the only test that drives the
**real** `build_ui` — so that *both* invoked controls being on the bar and reaching `emulator/screen_text` is
a gate rather than an assumption. It closes the same hole for `palette::PALETTE_LABEL`, which had none.

### 4.2 Mutations — 16, every one applied to a COMMITTED tree, quoted from disk, its revert verified

Method, and it matters: **commit, then mutate, then restore.** `git checkout --` on a dirty tree deletes your
own uncommitted work first, after which every later mutation patches a file it has already reverted. Each
mutation below was written to disk by a helper that reads the file back and refuses ambiguous matches; `git
diff --stat` named the file before each red run; `git status --short` was **empty** after each restore.

| # | file | mutation | the assertion that fired |
|---|---|---|---|
| M1 | `rom_open.rs` | `activate` indexes `self.entries` instead of `self.rows()` | `left: Descend("/tmp")` / `right: Open(".../zulu.bin")` — 3 tests red |
| M2 | `rom_open.rs` | `Row::display` drops the marker | `left: ["s4.bin"]` / `right: ["s4.bin   [loaded]"]` |
| M3 | `rom_open.rs` | a failed `rescan` clears the listing | `left: []` / `right: [Entry{../}, Entry{s4.bin}]` |
| M4 | `rom_open.rs` | `typed` treats any existing file as a ROM | `left: Rom(".../readme.txt")` / `right: NotAnImage(...)` |
| M5 | `rom_open.rs` | `open` calls the bus **without** `paused_for`, claiming `Restored` | `-32005 … needs the machine paused` reached the glass — 3 tests red |
| M6 | `rom_open.rs` | `reload_params` canonicalises the path | `left: ".../s4.bin"` / `right: ".../acts/../s4.bin"` |
| M7 | `rom_open.rs` | the refusal's `text` becomes a canned sentence | server said `cannot read …: No such file…`; control showed `-32602 could not open that ROM` |
| M8 | `rom_open.rs` | `decide_drop` accepts any single **file** | ``a `.txt` must be refused, not Open(".../notes.txt")`` |
| M9 | `rom_open.rs` | `show` draws the window's own sentence but does not report it | the run vector came back with the sentence absent |
| M10 | `oracle-frontend/palette.rs` | `Picker::visible` filters on `display()` instead of `label` | `left: ["s4.bin"]` / `right: []` — *the moved seam test, still live in its new home, catching its own historical defect* |
| M11 | `rom_open.rs` | `folder_of` always answers `"."` | `left: "."` / `right: "/home/x/games"` — 2 tests red |
| M12 | `oracle-frontend/rom_browser.rs` | `picker_marker` compares `file_name()` instead of the whole path | lib: `left: "s4.bin   [loaded]"` / `right: "s4.bin"`; player: `a same-named ROM in a different folder was marked` |
| M13 | `oracle-player/main.rs` | the bar draws the control but does not report it | the whole bar quoted back with `📂 open ROM` missing between `⌨ commands` and the status |
| M14 | `rom_open.rs` | **§2.3.1's shipped defect, put back**: `open` re-lists `self.dir` | `left: Some(".../repoint-browsed")` / `right: Some(".../repoint-elsewhere")`, exit **101** — the controller's probe, reproduced as an assertion |
| M15 | `rom_open.rs` | the repoint moved **into the drop arm** (two-part: `open` back to `self.dir`, plus a repoint after `self.open(..)` in `act_on_drop`) | leg 1 **passes**, leg 2 fails: *"a swap through `open` must repoint too — the fix belongs to the shared sequence, not to the drop arm"*. This is what earns the second leg. |
| M16 | `rom_open.rs` | a **refused** swap re-lists anyway | *"a refused swap must leave the listing un-attempted, or the repaint below cannot take one"* |

#### Two mutations that did **not** go to plan, and both changed the work

**M6 was applied and stayed GREEN.** `12 passed; 0 failed`. Applied-and-still-green is a **test** defect, not
a pass. The reason: the fixture path was a made-up `./games/../games/My ROMs/s4 (final).bin`, and
`canonicalize` *fails* on a path that is not on disk — the mutation's `unwrap_or_else` handed back the input
and the mutation was inert. The test was measuring a string round trip and calling it a no-canonicalise
guarantee. It now uses a **real** file reached through a **real** detour (`<tmp>/acts/../s4.bin`) with two
COULD-NOT-MEASURE preconditions asserted first, and M6 is red against it. Committed as its own change
(`98331d9`) before the re-run, per the method.

**M12 was red in the lib and green in the player**, which found a genuine gap rather than a redundancy: the
by-path rule lives on `picker_marker`, and the player only *delegates* to it, so a `Row::marker` rewritten to
compare names locally would keep the lib's test green and lose the property on this side. The round-trip test
gained a decoy leg — a same-named `s4.bin` written into the subfolder must not be marked — and M12 is now red
on both sides (`c81ea85`).

**And M13's first attempt is the method's own trap, sprung.** The bar assertion was **uncommitted** when the
mutation was reverted, so `git checkout --` deleted it: `grep -c "must offer both invoked controls"` came back
`0`. Re-added, committed (`05effe2`), and the mutation re-run against that commit — where it is red, and where
the restore leaves the assertion in place (`grep -c` = 1, `git status --short` empty). Recorded because an
unapplied mutation and a correctly restored baseline print the same word.

## 5. ⟨RUNTIME⟩ — what only a foreground window pass can confirm

**Nothing in this parcel has touched a running window.** There is no display in this session and the runtime
pass is the controller's. Everything above is `cargo test`. Owed, in priority order:

1. ★★★ **Does the owner's Wayland compositor actually deliver a file drop to winit on his desktop?** This is
   the one claim the code cannot make. The API path is confirmed from crate source (§2.4) and the decision is
   covered headless, but *"`WindowEvent::DroppedFile` arrives"* is a fact about a compositor, a portal and a
   file manager. The 08-28 page measured his session as native Wayland (`XDG_SESSION_TYPE=wayland`). If the
   drop never arrives, `decide_drop` is dead code on his machine and the browser + paste routes are the whole
   feature — which is still both halves of what the previous parcel shipped.
2. ★★ **The whole gesture against a real game**, on `aeon/s4.debug.bin` per the owner's standing request:
   `Ctrl+O`, descend through `../` and back, open a different image, and confirm (a) the picture and audio
   follow the new cartridge, (b) the `.srm` and the ten save slots re-key to it (`bus::drain`'s tests cover
   the module; this is the end-to-end), (c) the `[loaded]` marker survives the round trip on real paths, and
   (d) the machine is **running** afterwards if it was running before. **Now also (e): open an image from a
   folder other than the one being browsed and confirm the listing repoints to it with the marker on the new
   cartridge** — §2.3.1's defect, whose fix is covered headless but has not been looked at.
3. ★★ **`Ctrl+O` actually reaches the control** through a real compositor's modifier state, and does **not**
   also reach a focused text field or move the save-state slot. `consume_shortcut` is the mechanism and
   `poll_machine_keys`'s latch is the guard; both are the palette's proven path, but neither has been pressed
   by a hand here.
4. ★ **The window is legible and floats over the dock.** `egui::Window` drawn after the `DockArea` should
   float, and the row list scrolls at 320 px, but no pixel of this has been looked at. Also: does the
   `📂` glyph render, or is it a hollow box? `screen::Glyphs` measures that from the atlas and the top-bar
   readback asserts `unrenderable: []`, so this is expected to be *answered* rather than open — but it has
   been answered by arithmetic, not by eyes.
5. ★ **The arrow-key walk is usable.** Up/Down are read only while the path box has focus (deliberately: the
   arrows are also the D-pad). Whether that reads as natural, or as "the arrows do nothing until I click the
   box", is a judgement only a hand can make.
6. ★ **A drop while the control is closed opens it visibly** — `act_on_drop` sets `open = true` on every
   non-`Nothing` arm, so a refusal cannot be silent. Covered as state; not seen as a window.

## 6. What this parcel deliberately did not do

* **No `rfd` and no native file dialog.** `rom_browser`'s header argues it and the argument stands: a portal
  dependency tree, a runtime portal requirement, and — the reason that decided it — a native dialog is
  untestable by construction.
* **No per-parameter form.** Not this control's business; `palette`'s header owns that refusal.
* **No `d-19` closure for the minifb window.** §2.5.
* **No change to `battery.rs`, `states.rs` or `bus::drain`.** §2.3.
