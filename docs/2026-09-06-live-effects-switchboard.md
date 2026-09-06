# LIVE-EFFECTS — the switchboard, and the three cells the card did not know about

**Project:** `LIVE-EFFECTS`, declared by the owner at empyrean `contract/projects.json`
(`declaredAt` 2026-09-06T15:47:55Z, `declaredBy` *"go for it"*).

**What shipped:** an `Effects` dock tab in `oracle-player` that picks a parallax scene, a raster program
or a background band table by symbol and switches the running game to it, replacing the START chord in
the debug build. Every gesture is pause, write, resume.

**Sources, at commits rather than tips:**

* `NOTE` = aeon `c4c5c3d8` `docs/2026-09-06-live-effects-ram-surface.md` — the RAM surface and the band
  record layout, written for this lane.
* `ENGINE` = aeon `c4c5c3d8` `engine/` — what the engine actually does, read where the two differ.

Both are transcribed into **one place**, `crates/oracle-player/src/effects.rs`'s `CHANNELS`. Nothing in
`ui.rs` spells a symbol or an address.

---

## 1. THE CORRECTION: writing one pointer is not a selection

The card says the three selectors are RAM longwords re-read every frame, *"so a write takes effect on the
next frame with no engine change"*. **The first half is true and the conclusion does not follow.** Each
channel has a companion cell whose stale value either reverts the write or hides it. All three were read
firsthand out of `ENGINE`; none of them needs an engine change, because every cell is RAM.

### 1.1 Raster — `Raster_Program` is an OUTPUT of the install, not its input

`Raster_Install`'s entire body is one instruction:

```
pub proc Raster_Install (a0: u32) clobbers() {
        move.l  a0, Raster_Pending          // raster.emp:946
        rts
}
```

`Raster_VBlank` is what copies the program into `Raster_Buf_A`, points `Raster_Active_Buf` at it, and
clears `Raster_Patch_Tab` and `Effects_Offscreen_Entry` (`raster.emp:1023-1039`). **The HInt walker reads
`Raster_Active_Buf`** (`raster.emp:1052`), never `Raster_Program`.

So a panel that wrote `Raster_Program` would change the cell a readout looks at and none of the state the
screen is drawn from: **it would name the new program while the old one kept running.** A leftover
`Raster_Patch_Tab` makes it worse than inert — `Raster_BuildSchedule` keeps re-recording the *outgoing*
patched program over the buffer every VBlank, so *"the static program would never be walked at all"*
(`raster.emp:1025-1028`).

**The panel writes `Raster_Pending` (`$FFFF8BDE`).** Still one longword; a different one. `Raster_Program`
is kept as the channel's **live cell** — what the panel reads back to say what actually took.

Two further facts from the same proc, both used:

* `Raster_Pending` = 0 means *keep whatever is live* (`raster.emp:934-942`), so **zero is never an off
  route** on this channel. Off is the empty program `Raster_Program_None`, which `Raster_VBlank`
  recognises by its first record already being the terminator and answers by tail-calling
  `HBlank_Uninstall`.
* Poking `Raster_Program` to the empty program directly would leave the HInt armed, measured by aeon at
  *"512 cycles per frame across TWO HInt entries, forever, to accomplish nothing"*.

### 1.2 Parallax — a lone poke is ignored, then silently reverted

While `Parallax_Transition_Frames` is non-zero, `Parallax_Update` drives from `Parallax_Target_Config` and
ignores `Parallax_Current_Config` entirely; when the counter expires it **promotes the staged target over
the write**:

```
        tst.b   Parallax_Transition_Frames          // parallax.emp:1647
        beq     .use_current
        subq.b  #1, Parallax_Transition_Frames
        bne     .use_target
        move.l  Parallax_Target_Config, d0          // :1655
        move.l  #0, Parallax_Target_Config
        move.l  d0, Parallax_Current_Config         // :1657  <- your poke, gone
```

The panel writes `Parallax_StartTransition`'s **instant arm verbatim** (`parallax.emp:1279-1287`):
`Parallax_Current_Config` = target, `Parallax_Target_Config` = 0, `Parallax_Transition_Frames` = 0,
`Parallax_Snap_Pending` = 1.

**Two deliberate differences from the engine's proc, both stated on screen:**

* **Always the instant arm.** The proc picks between instant and a smooth lerp on the new config's
  `pcfg_transition` byte. On the smooth arm `Parallax_Current_Config` still holds the *old* scene for
  several frames, so the panel's own readback would disagree with what the panel just did — the exact
  class of lying readout this surface exists against. A person turning a knob also wants the picture to
  change when they click.
* **The VDP $0B (Mode Set 3) shadow is NOT written**, and that is measured rather than assumed.
  `Parallax_Update` re-asserts $0B every frame from the resolved active config (`parallax.emp:1668-1690`)
  and its comment gives the reason: *"Parallax_StartTransition writes the mode only on a section-boundary
  crossing … without a per-frame re-assert the register goes stale."* It also names itself the **sole
  writer** of that byte, which is a second reason not to write it from outside.

⚑ **`Parallax_Active_Config` is a PROC, not a cell** (`parallax.emp:1358`; release `$648C`, debug
`$7C54` — ROM either way). Anything that reached for it as "the other config variable" would be writing
into code. Named here because the name invites the mistake.

### 1.3 Bands — the eight poison bytes are half of the switch

`BgAnim_SetTable` is the pointer **and** `BgAnim_LastStep` set to the init sentinel:

```
pub proc BgAnim_SetTable (a0: u32) clobbers(d0) {
        if DEBUG == 1 {
                move.l  a0, BgAnim_Table_Ptr    // bg_anim.emp:183
                moveq   #-1, d0
                move.l  d0, BgAnim_LastStep     // :185
                move.l  d0, BgAnim_LastStep + 4 // :186
                rts
        }
}
```

Its header states the requirement rather than the tidiness: `BgAnim_LastStep` is per **band index**, not
per band identity, so two tables whose band 0 sits on the same step take the `.skip_band` arm forever and
*"the new table would never paint"*. Poisoning *"makes the switch atomic — there is no way to call this
and get the stale picture."* A lone pointer write is exactly that way.

### 1.4 And the write-set is atomic BECAUSE the machine is paused

Four cells written to a stopped machine are seen by the next frame together or not at all. On a running
machine they would not be. This is a second thing pause-write-resume buys that nobody asked for, and it
sits alongside the caveat it dissolves (§4).

---

## 2. Everything is addressed by NAME; the transcribed addresses are witnesses

Every write goes out as `emulator/write_memory {symbol, disp, value, width}`, so **the server resolves the
destination from its own listing** and no write address exists anywhere in this crate. The reason is a
defect this workspace has already paid for: `Camera_X` is `$FFFFA576` in the release listing and
`$FFFFA604` in the debug one, and **a stale address does not fault — it returns a number**.

`Channel::noted_addr` is used for exactly one thing: `Channel::drift` **refuses** when the loaded listing
and `NOTE` disagree about where a channel's live cell is. A refusal rather than a caveat, because the
obvious reading (*the note is stale, trust the listing*) is right about half the time and the other half
is *the listing loaded does not describe the ROM running* — and the panel cannot tell which.

⚑ **The spelling matters and it cost a bug.** `NOTE`'s addresses are the listing's 32-bit `rawAddr`
(`$FFFF88EC`), not the 24-bit form the memory doors take (`$FF88EC`). `forbidden`'s address route was
handed the door form and was therefore **dead against every real input**, while its own unit row passed
because that row fed it the constant — the one input that cannot expose it. Found by the gates before any
mutation was applied; fixed by comparing on the raw spelling throughout, and pinned by
`the_address_route_fires_on_a_resolved_symbol_and_not_only_on_the_constant`, which goes through the real
`run` with a real listing and a real resolve.

---

## 3. `Debug_Lab_Index` is guarded twice, on every cell

`$FFFFEE0D` is the START chord's **cursor**. Writing it moves the label on screen and changes nothing that
runs. `NOTE` prices the mistake: *"This cost this lane an hour today — the label said one row while the
machine ran another."*

`forbidden` refuses it **by name and by resolved address independently** — a write-set edited to name the
cursor, and a cell whose symbol resolves there, are different mistakes and neither guard subsumes the
other — and it runs on **every cell of a write-set**, not only the channel's own live cell, because a
write-set is data and data gets edited by somebody who has not read the header.

---

## 4. The torn-frame caveat does not apply here, and aeon agrees

aeon's first note warned that writing these pointers races the once-per-frame read and yields one torn
frame. A paused write cannot land between a read and a buffer fill because nothing is reading. aeon
accepted the correction in the same note that raised it (`NOTE` §5): *"Oracle is right that a paused write
cannot land mid-frame … the caveat stands only for a write to a running machine, which is the
scripted-sweep case and not the panel's."* It is written into the code comment so a later reader does not
re-derive the worry.

Pause-write-resume is also what the served contract permits: `emulator/write_memory` is paused-machine
only, *refused never clipped*. The owner accepted the cost in advance: *"a small pause to pause and
unpause for the change is fine."* `crate::screen_pick::paused_for` is **called, never re-implemented**, so
*already-paused stays paused* has one implementation in this crate.

---

## 5. The two BLOCKED items, resolved differently

### 5.1 Band read-back — UNBLOCKED, and built

It was blocked only while the record layout was undocumented. aeon committed it at `c4c5c3d8` §2, so the
readout is transcribed field for field: `u16 count` then `count` records of **44 bytes**,
`BGANIM_MAX_BANDS` = 4. **The 44 is the note's stated `ensure`, never a sum of the fields** — a struct
with tail padding sums to 44 just as readily while the walk strides differently — and the field sum is
asserted only to *fit*.

The two derivations the note spells out are done for the reader rather than left to them: `step_mask` is a
period **minus one** (63 means 64 px) and `col_shift` is a **log2** (7 means 128 bytes). A count past the
ceiling is **reported, not trusted**. The gate is the note's own worked example decoding to the note's own
sentence.

### 5.2 Numeric nudging — GENUINELY BLOCKED, and shown disabled

`Parallax_Current_Config` points at ROM, so a factor cannot be edited in place. aeon's hook (a RAM scratch
config plus a copy-and-repoint entry, the shape `Raster_Buf_A`/`Raster_Buf_B` already use) is sized and
not started, and the hub ruled *"nudge controls do not ship until it lands."*

**Choice made: draw the control DISABLED with a one-line reason, rather than omit it.** The argument for
omitting is that an absent control makes no promise. The argument against is stronger and it is this
panel's own thesis applied to itself: the owner asked for nudges **by name**, and a panel that simply has
none reads as *we forgot*, which sends him to ask. A disabled control with a readable line answers the
question where it is asked, and it retires itself the day the hook lands.

It is **two** numbers when it arrives, not four: `NOTE` §2 says only `driver` and `rate_shift` are
meaningful — `step_mask` and `col_shift` are geometry derived from the art's shape, and moving either
without moving the art gives *"a picture rather than an effect"*. The disabled line is gated on not
promising them.

### 5.3 Bands-off — refused, and the trap in the refusal

There is **no canonical off target**. Pointing at the act's own `BgAnim_Table` works only because the
shipped act happens to hold a zero count there — `NOTE` §3, *"by coincidence of content, not by
contract"*. An act with live bands holds a real count in that same word. `BgAnim_Table_Ptr` = 0 is never
valid either (`BgAnim_Init` seeds it). So bands-off is **refused with the reason on screen** until
`BgAnim_Table_Empty` resolves in the loaded listing.

**The constant has since LANDED**, at aeon `41c845fa`, so the panel's target now exists in the engine.
`NOTE` §3's *"there is no `BgAnim_Table_Empty` symbol in this tree"* is superseded by it; both are cited,
because the panel has to behave correctly on a listing built before it as well as after.

⚑ **The trap, and it is why the gate is TWO conditions rather than one.** The declaration is:

```
const BGANIM_EMPTY_EMIT = if DEBUG == 1 { 1 } else { 0 }
pub data BgAnim_Table_Empty: [u16; BGANIM_EMPTY_EMIT] = if DEBUG == 1 { [0] } else { [] }
```

So in a **release** build the symbol's NAME enters the listing with an address while the array emits
**nothing** — there is no zero word behind it there. `BgAnim_Table_Ptr`, the destination, is genuinely
absent from a release listing. A gate keyed on the target alone would therefore offer a bands-off button
on a release build that writes **an address with no zero behind it** into **a cell that is not the band
pointer**: two wrongs in one gesture, each of which looks fine on its own.

`turn_off` resolves the destination **first**, which is what makes the target's release visibility
harmless rather than dangerous. That ordering is a correctness property with a gate on it
(`bands_off_needs_the_destination_pointer_too_not_only_the_empty_table`, M7 below).

⚑ **And the refusal has to read as *your listing is old*, not as *this is broken*.** The symbol landed at
16:48Z; the listings on this machine were built at 06:42Z (release) and 14:53Z (debug), so neither
contains it yet and the owner's running window loaded the 14:53Z one. **Bands-off will correctly refuse on
his machine until aeon rebuilds and he reloads symbols.** That is the gate working, so the message names
the missing symbol, says the feature exists in the engine, says why the obvious substitute is wrong, and
carries a remedy naming the symbol and the commit. Three assertions hold each of those, because a refusal
a person misreads as a broken feature costs more than no refusal.

---

## 6. Gates

Nine mutations, applied to the committed baseline `cfcf117`, each read back from disk before the run and
restored from that same commit afterwards. **All nine red**, and two more follow below. The parameter was
varied deliberately —
which cell, which symbol, which guard branch, which constant, which order, which persistence door — rather
than repeating one shape.

| # | mutation, quoted from disk | red rows |
|---|---|---|
| M1 | `symbol: "Parallax_Current_Config",` (was `Parallax_Target_Config`) | `every_channels_write_set_is_the_installers_own_cells`, `a_cell_refused_mid_set_names_how_far_it_got` |
| M2 | `symbol: "Raster_Program",` (was `Raster_Pending`) | `every_channels_write_set_…`, `selecting_a_raster_program_stages_pending_and_never_writes_the_program_cell` |
| M3 | `symbol: "Debug_Lab_Index",` (was `BgAnim_Table_Ptr`) | `no_shipped_write_set_names_or_resolves_to_the_cursor`, `a_selection_writes_the_installers_cells_by_symbol_and_nothing_else`, `bands_off_is_refused_…`, `every_channels_write_set_…` |
| M4 | `} else if false && raw_addr == LAB_INDEX_ADDR {` | `the_address_route_fires_on_a_resolved_symbol_and_not_only_on_the_constant`, `the_lab_index_is_refused_by_name_and_by_address_independently` |
| M5 | `(false && total > shown).then(\|\| {` | `a_cut_short_search_says_so_and_a_whole_one_says_nothing` |
| M6 | `pub const BAND_RECORD_BYTES: usize = 40;` | `the_notes_worked_example_decodes_to_the_notes_own_sentence`, `the_record_size_is_the_notes_figure_…`, `a_count_the_ceiling_forbids_…` |
| M7 | `let sel_addr = available(c, channel).unwrap_or(channel.noted_addr);` | `bands_off_needs_the_destination_pointer_too_not_only_the_empty_table` |
| M8 | `let _ = &changes;` (was the `retain` that replaces a channel's change) | `the_statement_stands_only_while_something_is_overridden_and_names_it` |
| M9 | `storage.set_string("oracle_player_effects", "parallax".to_string());` | `nothing_the_effects_panel_selects_can_reach_the_saved_layout` |

M1 and M2 are the row that matters most: the **write-set transcription table** is a literal beside the
line of `ENGINE` each cell came from, because every other gate derives its expectation from `CHANNELS` and
is blind by construction to the table itself being wrong. That blindness is the expensive one — the card's
one-cell raster set passes every derived assertion.

Three controls are load-bearing and named as such:

* `nothing_the_effects_panel_selects_can_reach_the_saved_layout` asserts the blob **contains** `"Effects"`
  before asserting it contains none of the panel's vocabulary. A not-contains assertion passes trivially
  against an empty blob; the positive control validates the **search**, not just the question.
* `a_cut_short_search_says_so_and_a_whole_one_says_nothing` reads the cap from
  `EngineConfig::default().max_symbol_matches` rather than typing 256, and asserts the whole list says
  nothing.
* `bands_off_needs_the_destination_pointer_too_not_only_the_empty_table` has a control proving it is about
  the **order** rather than about refusing whenever anything is missing.

Two more landed with the readout fixes, on the same harness against baseline `4c66e79`:

| # | mutation, quoted from disk | red row |
|---|---|---|
| M10 | `available(c, channel)?;` (the drift check deleted from the READ path) | `a_drifted_selector_refuses_the_readback_as_well_as_the_write` |
| M11 | `if false && self.value == 0 {` | `a_zero_live_cell_reads_as_that_channels_own_documented_meaning` |

**Eleven mutations, eleven red.**

### 6.1 Two defects the gates found in my own work before any mutation ran

* **`forbidden`'s address route was dead.** See §2. Found because `Channel::drift` had the same
  raw-versus-door confusion with the sign flipped and refused every legitimate selection, which is what
  actually went red. The lab-index row went on passing throughout, because it fed the guard the constant.
* **A zero live cell read as *"the listing names nothing there"* on all three channels.** It is a
  different fact on each and `NOTE` documents two of them: a zero raster program is a documented off
  state, and a zero band pointer is a machine that has not initialised. The readout was reporting one as
  unreadable and the other as ordinary. `Channel::zero` now carries each.

### 6.2 Aggregates, and one that is NOT a pass

| suite | result |
|---|---|
| `cargo test -p oracle-player` | **346 passed, 0 failed** (+ 4 in `tests/p10_no_dashes_in_shipped_text.rs`) |
| `cargo test -p oracle-frontend` | **389 passed, 0 failed, 1 ignored** over 5 legs |
| `cargo test -p oracle-aether` | **587 passed, 0 failed, 2 ignored** over 45 legs |
| `cargo test -p oracle-replay` | 0 tests |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -p oracle-player --all-targets` | zero warnings |

⚑ **`cargo test --workspace` was NOT run to completion and is not claimed green.** Two attempts were
reaped by the harness's time cap, the second at 54 of ~65 legs with 0 failures so far. A reaped run is not
a pass, so the suites above were run per crate instead. `oracle-core` is reported separately for the same
reason: it carries the multi-minute SST sweep.

⚑ **And an environmental failure that looks exactly like a regression.** A worktree without
`vendor/TestRoms/` fails 8 `save_state::tests::*` rows in `oracle-frontend` — *"vendored test ROM … is
missing … these tests must not skip silently"*, which is the right design and not a flake. Symlinking the
main checkout's `vendor/` in makes them 12 passed / 0 failed; `vendor` is gitignored, so the symlink does
not dirty the tree. Worth knowing before anyone reads those eight names as this branch's doing.

⚑ **A background run's completion notice reported "exit code 0" while cargo had exited 101.** The notice
carries the *wrapper's* status. The real code was visible only because the wrapper wrote it into the log.
Anything reading a background suite's verdict off the notification alone is reading the wrong number.

---

## 7. Owed, and not done here

* **No runtime confirmation.** Nothing on this branch has been exercised against a running game: this lane
  is barred from the emulator MCP tools and from opening a second window while the owner's is live. The
  panel's write-sets, refusals and readbacks are checked against a fake bus and against aeon's committed
  source, and **not** against a machine. Tagged for foreground follow-up.
* **`BgAnim_Table_Empty` is not in any listing yet**, so the bands-off path has never resolved its target.
  Its two-condition gate is tested with the constant injected into a fake listing.
* **The default prefixes are a starting point, not a claim.** `ParallaxConfig_` (21 in both listings),
  `EditorRaster_` (6 in both), `BgAnim_Table` (2 in debug). The prefix box is editable for that reason.
