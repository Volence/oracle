# LIVE-EFFECTS — the nudge control, and the struct the forecast was about

**Parcel:** `LIVE-EFFECTS-NUDGES`, part of the declared cross-lane project `LIVE-EFFECTS`
(empyrean `contract/projects.json`, `declaredAt` 2026-09-06T15:47:55Z).
**Branch:** `parcel/live-effects-nudges`, base `f52d226`.
**What shipped:** the Effects tab's nudge row, which was a **disabled control with a one-line reason** for
a fortnight, is now a working one. It arms aeon's RAM scratch parallax config and turns ten of its numbers
while the game runs.

**Sources, at commits rather than tips (aeon's default branch is `master`, not `main`):**

* `HOOK` = aeon **`935c33cf`** — the commit that landed `Parallax_Scratch_Config`, `Parallax_Scratch_Arm`
  and `Parallax_InstallScratch`. Read out of `engine/ram.emp:1811` and `engine/level/parallax.emp:4208`
  **firsthand**, because the doc's own §4 banner says *"LANDED"* in one sentence and *"are on branch
  `parcel/live-effects-hook`"* in the next, which are different claims.
* `NOTE` = aeon `docs/2026-09-06-live-effects-ram-surface.md` at `origin/master` `1cabfa68`, §6 — the
  scratch's contract, written for this lane.
* `s4.debug.lst` (built 2026-09-18 19:26) and `demo.debug.lst` (2026-09-17 17:04), on this box. Every
  address and stride below is read out of one of those, not out of a doc.

---

## 1. ⚑ THE ONE CORRECTION THAT SHAPES THE PARCEL: the hook reaches a different struct than the forecast

The switchboard design's §5.2 and this parcel's brief both said the control would be **two numbers,
`driver` and `rate_shift`**, citing `NOTE` §2. **Those two fields are not in this hook and cannot be
reached by it.**

* `driver` and `rate_shift` are fields of the **BgAnim band record** — the 44-byte struct in the table
  `BgAnim_Table_Ptr` selects (`NOTE` §2).
* The hook that landed is the **parallax** channel's. Its buffer holds a `parallax_config`: a 30-byte
  header plus band records whose fields are **scroll-factor shifts**, deform shifts, phase offsets and a
  drift rate. There is no `driver` and no `rate_shift` anywhere in it.

aeon's own note noticed this from the other side and said so in §6.5, which is why it is a correction and
not a discovery: *"The parallel with §2's band-record advice holds, and the answer is **not** the same one:
there, `driver`/`rate_shift` were nudgeable and `step_mask`/`col_shift` were art geometry. Here the
division is **three-way**."*

So §5.2's *"two numbers"* was a forecast made about one channel's fields from the other channel's note, and
it did not survive the hook actually landing. §5.2 is corrected where it stands.

**The refusal reason matters as much as the refusal.** `driver` is **not** geometry — it is perfectly
nudgeable in principle — so filing it under the geometry reason would be a correct conclusion under a wrong
premise, and the next lane to ask would be told the wrong thing. It is filed as **wrong channel**, with the
engine-side gap named (§4 below).

---

## 2. What was built

### 2.1 The two conditions, and the trap they exist for

The precedent is §5.3 of the switchboard design, and its lesson is one sentence: **a name resolving is not
storage existing.** For bands-off, `BgAnim_Table_Empty`'s *name* enters a release listing with an address
while its array emits nothing behind it, so a gate keyed on that one symbol would have written a real
address into a cell that was not the destination.

**This hook has the identical shape, and `NOTE` §6.6 measured it:**

> `Parallax_InstallScratch` **APPEARS IN THE RELEASE LISTING, with an address**, while
> `Parallax_Scratch_Config` and `Parallax_Scratch_Arm` do NOT.

Its body — the `rts` included — is inside `if DEBUG == 1`, so the proc emits zero bytes there and its empty
label collapses onto its neighbour's address. A panel that decided *"the hook is available"* by resolving
the proc would offer knobs on a release build and then write into whatever now occupies the buffer's
address, with no fault to show for it.

So the gate is **two conditions with the destination resolved FIRST**:

1. **`Parallax_Scratch_Config` resolves** — the buffer a nudge writes into. Genuinely absent from a release
   listing (`engine/ram.emp:1811`, `if DEBUG == 1 @shape_divergent`).
2. **`Parallax_Scratch_Arm` resolves** — the request cell. Asked separately: one of the two resolving is
   not both.

**`Parallax_InstallScratch` is never resolved at all.** `the_gate_ignores_the_proc_because_its_name_ships_in_a_release_listing`
asserts that no lookup for it is even issued, because whatever is done with the answer, the answer is *yes*
on a build with no buffer.

### 2.2 The refusal on a build with no scratch

The control is drawn **disabled with the reason above it**, which is the shape the panel already chose over
an absent one, for the owner's stated reason: a panel that simply has no control reads as *we forgot*, which
sends him to ask. The line says, in this order:

* both symbols are missing, so **nothing was written**;
* **this is not a fault** — the scratch is `if DEBUG == 1`, so a RELEASE build emits zero bytes for it;
* **selecting a scene still works here**, because `Parallax_Current_Config` is in both shapes and only
  editing a scene's numbers needs the buffer;
* ⚠ `Parallax_InstallScratch` *does* appear in a release listing with an address, and this panel
  deliberately does not resolve it — so a reader who finds the name and wonders why the feature is off has
  the answer;
* remedy: run `s4.debug.bin` and load the listing that build produced; if you are already on a debug build,
  your listing predates `935c33cf` — rebuild and reload.

It never uses the words *broken*, *failed* or *error*, and
`the_no_scratch_refusal_names_the_shape_and_the_symbol_and_never_reads_as_broken` holds all of that. §5.3
paid for this wording once already: the bands-off target landed at 16:48Z and the owner's window held a
14:53Z listing, so a *correct* refusal looked like a broken feature.

### 2.3 "Did the install take" — the fact, not a status byte

`NOTE` §6.2 is explicit that the arm cell is a **request** byte and not a status byte (the engine clears it
as it services it, whether the install took or was refused), and that the success test is reading
`Parallax_Current_Config` and comparing it against `Parallax_Scratch_Config` — *"the fact itself rather
than a report of it"*. That comparison is the whole of the answer.

**The arm byte is read back too, and for exactly one job**: separating two failures the comparison alone
renders identical.

| `Parallax_Current_Config` | arm byte | what it is |
|---|---|---|
| == the scratch | (either) | **installed.** A nudge lands on the next frame |
| != the scratch | cleared | the engine **serviced and REFUSED**: no active config, or `pcfg_band_count` > `MAX_PARALLAX_BANDS`. `Parallax_InstallScratch`'s own `Out:` — nothing written, *"a refusal never clamps"* |
| != the scratch | still set | the arm was **never serviced**: `Parallax_Update` did not reach its head poll. `HOOK`'s banner names `games/demo` as exactly this case — *"arming on demo leaves the arm cell SET for ever, which reads like a dirty refusal and is nothing of the kind"* |

Two facts read off the machine, three states, no byte invented and no byte read as a status.
`Parallax_Snap_Pending` is a second available witness (`NOTE` §6.2: a 0 after the arm frame is the pass) and
is deliberately **not** read — the comparison already settles it, and a second source that can disagree is a
second thing to explain.

⚑ **The comparison is masked to 24 bits on both sides.** `Parallax_Current_Config` holds the full
sign-extended long and a listing may resolve the symbol either way; `NOTE` §6.2 warns *"a raw compare is a
false mismatch, and it was the first thing this lane's own probe got wrong."* The gate is written over the
**space** of values rather than the pair that was measured: there are four spelling combinations, masking is
right in all four and a raw compare is right in one.

### 2.4 The pause-write-resume flow, unchanged

Every write goes through `screen_pick::paused_for`, which is **called and not re-implemented**, so *"the
machine really was paused while the body ran"* has one implementation in the crate. The arm gesture also
runs **exactly one frame** while paused — that is the only way in, because `NOTE` §6.2 says an Aether client
*"can write a byte and run a frame and cannot force a `jsr`"* — and one rather than two because
`Parallax_Update` polls the arm at its head, **ahead of its own config select**, so the install lands on
that frame. The poke-before-frame order is asserted, not only the counts.

**The torn-frame caveat is not reintroduced and does not apply**: a paused write cannot land mid-frame, and
aeon accepted that correction in `NOTE` §5.

### 2.5 Editing a scratch the engine is not reading is refused

The scratch is ordinary work RAM: a write into it always succeeds and means **nothing** unless
`Parallax_Current_Config` points at it. Two ordinary events leave it that way — nobody armed yet, and
`NOTE` §6.6's consequence 1, **crossing a section boundary evicts the scratch** because
`Parallax_CheckBoundary` installs the new section's own ROM preset exactly as it always did. So the install
is re-checked before **every** write, and the refusal's remedy names the boundary crossing and says to arm
again. Without it, a person who armed, walked right, and turned a knob would watch nothing happen — which is
`NOTE` §0's whole subject.

---

## 3. ⚑ THE OFFSETS COME OUT OF THE LISTING, AND TWO MEASUREMENTS SAY THEY MUST

`NOTE` §6.3/§6.4 tabulate every field's offset and transcribing them was the obvious move. Two things
measured at this seat say not to:

**1. The scratch has already MOVED.** `NOTE` §6.1 records `Parallax_Scratch_Config` at **`$FFFFEA26`**;
`s4.debug.lst` as built 2026-09-18 puts it at **`$FFFFEA46`** — $20 along. It is at the RAM tail inside a
`@shape_divergent` group, so it moves whenever any *other* debug-RAM group changes size, which is an
ordinary aeon commit. A `Channel::drift`-style positional refusal here would refuse the whole feature on a
healthy build. So the noted address is kept as a **witness only** and the panel states the difference in
its own shape line with the reason it is benign — an unexplained disagreement, on a surface whose sibling
refuses for exactly that, is a reader's hour.

**2. The band-record stride is PER GAME.**

| build | `Scratch_Config` | `Scratch_Config_End` | span | header | ceiling | **stride** |
|---|---|---|---|---|---|---|
| `s4.debug` | `$FFFFEA46` | `$FFFFEC64` | 542 | 30 | 16 | **32** |
| `demo.debug` | `$FFFFE550` | `$FFFFE60E` | 190 | 30 | 16 | **10** |

A transcribed 32 would have addressed demo's band 1 inside its band 3. `NOTE` §6.4's *"`sizeof(band_record)`
is 32 **for this game**"* says so; this is what reading past that clause costs.

**What the listing publishes instead is the struct layout itself, as equates**, and this was the parcel's
best find:

```
EQU parallax_config_len                   = $0000001E
EQU parallax_config_pcfg_layer_mask       = $00000002
EQU band_entry_band_factor_a_s1           = $00000002
EQU MAX_PARALLAX_BANDS                    = $00000010
```

Those reach a client through **`emulator/lookup_equate`** (protocol §11.36 / CR-M) — already served, already
vendored, **no contract change was needed for this parcel**. So every offset a nudge writes at is resolved
per gesture from the listing the machine is running with, exactly as every address already is, and the
stride is **derived** rather than believed:

```
span   = Parallax_Scratch_Config_End - Parallax_Scratch_Config
stride = (span - parallax_config_len) / MAX_PARALLAX_BANDS
```

which is `engine/ram.emp`'s own sizing arithmetic run backwards. **A non-exact division is a refusal, never
a rounded stride** — a rounded stride is right for band 0 and wrong for every band after it, forever, with
no fault, because writes land in the middle of fields.

One caveat, stated rather than hidden: the equates published are `band_entry_*` (the 10-byte legacy struct),
not `band_record_*` (the strided one). Every field this panel offers is inside the `band_entry` half, which
is why they all resolve; the capability tails publish no equates at all. That is why the stride comes from
the span and not from `band_entry_len`.

---

## 4. ⚑ FOR AEON AND AURORA — which of these fields are live at runtime

*Written to be read without this parcel's context. This is what the parcel established about the parallax
scratch and about the BgAnim table; it is owed in the same turn rather than discovered later.*

### 4.1 A runtime reader exists for these, and oracle's panel now writes them

All are in `Parallax_Scratch_Config` (DEBUG shapes), and all are in `NOTE` §6.5's **free knobs** class:
nothing else in the config depends on them, so turning one produces a picture whose parts still agree.

**In the header, once per scene:** `pcfg_layer_mask` (u16 — `NOTE` calls it the best knob here),
`pcfg_deform_speed_fg` (u8), `pcfg_deform_speed_bg` (u8), `pcfg_bob` (u8, packed — the **whole byte** 0 is
the no-bob sentinel).

**In each band record:** `band_factor_a_s1`, `band_factor_a_s2`, `band_factor_b_s1`, `band_factor_b_s2`
(u8, `0..=14` a shift and `15` a sentinel), `band_factor_ops` (u8, bits 0-1 only), `band_phase_offset` (u8).

### 4.2 Geometry and coupling — refused, with the reason

* **Geometry (the art's shape decides it).** BgAnim's `step_mask` (a period **minus one**) and `col_shift`
  (a **log2** of a byte unit): moving either without moving the art gives *"a picture rather than an
  effect"* (`NOTE` §2). `brm_hshift` likewise — `H = 1 << brm_hshift` is the remap ladder's own geometry, and
  changing it without the ladder walks off the table.
* **Coupled to something no runtime pass re-derives.** `pcfg_v_factor_bg`, `pcfg_v_center_y`,
  `pcfg_v_offset`: every `band_top_plane` was computed through those three **at build time** and nothing
  recomputes it, so turning one slides the camera's idea of the plane against the art the layers were
  registered on. `band_top_plane` itself is coupled to its neighbours — the fill reads band *i+1*'s top as
  band *i*'s end, so the records must stay strictly ascending. `pcfg_band_count` upward is coupled to what
  the install actually copied.
* **Inert — a control on any of these is a slider that does nothing.** `pcfg_v_factor_fg` (RESERVED, **no
  runtime reader at all**: the v1 pipeline always sets `fg_vscroll = camY`), `bc_step`/`bc_rem`/`bc_span`
  (derived every frame into the engine's shadow copy), `bc_pad` (alignment, read by nothing),
  `pcfg_transition` (read at install time only, and forced to 1 by the install on purpose).
* **Pointer fields** (`pcfg_deform_table_*`, `brm_ladder`) — their one safe written value is `0`, which is
  an on/off rather than a nudge.

**⚑ For aurora specifically:** an editor that offers a field on *this* list must either move the coupled
partner with it or say what it will not look like. The three vertical-mapping bytes are the sharp case: they
are a real and useful knob (*"how fast does the BG plane climb"*) and they are also the one place a slider
produces a picture whose parts disagree. `pcfg_v_factor_bg = 15` (lock) is the clean case, and eighteen of
the twenty shipped scenes already author it.

### 4.3 What a RELEASE build does not have

**`Parallax_Scratch_Config`, `Parallax_Scratch_Config_End` and `Parallax_Scratch_Arm` do not exist in a
release build** — zero bytes emitted, no name in the listing (`engine/ram.emp:1811`,
`if DEBUG == 1 @shape_divergent`). So **no numeric nudging of any kind is possible on a release ROM**, and a
tool that offers it there is offering a write into unrelated RAM.

⚠ **`Parallax_InstallScratch` DOES appear in a release listing, with an address.** Its body is DEBUG-gated,
so it emits nothing and its label collapses onto its neighbour's, but the *name* ships. **Never gate a
feature on it.** This is the second instance of the same trap (`BgAnim_Table_Empty` was the first): a name
resolving is not storage existing. Gate on the RAM symbol, and gate on two of them.

`Parallax_Current_Config` is in **both** shapes, so scene *selection* works everywhere; only editing a
scene's numbers needs the debug buffer.

### 4.4 ⚑ AN OPEN AEON ASK — BgAnim has no scratch, so its two nudgeable fields are unreachable

`NOTE` §2 names `driver` and `rate_shift` as the two fields of a BgAnim band record a panel could usefully
nudge, and they are still the right two. **They cannot be written by anything.** An act's `BgAnim_Table` is
ROM (`engine/ram.emp:1715`: *"An act's `BgAnim_Table` is ROM"*), `BgAnim_Table_Ptr` selects between ROM
tables, and **no RAM copy of a band table exists in any build shape.** There is nothing to edit in place.

Closing it is the same two-part shape this hook already is, applied one struct over: a
`BgAnim_Scratch_Table` in the DEBUG RAM tail sized `2 + BGANIM_MAX_BANDS * sizeof(bganim_band)`, plus a
`BgAnim_InstallScratch` that copies the active table into it and calls `BgAnim_SetTable` (so the
`BgAnim_LastStep` poisoning comes from one authority rather than a second copy), armed by a request byte the
way this one is. **oracle is not asking for it, only reporting that it is what the gap is**; the panel says
so on screen rather than leaving the two field names looking forgotten. A second ask, much smaller, would
make the derivation in §3 unnecessary: publish a `band_record_len` equate (or `PARALLAX_SCRATCH_BYTES`)
alongside the `band_entry_*` rows, and a client could resolve the stride instead of deriving it.

---

## 5. Gates

Nineteen mutations. Each was **written to disk and read back off disk** before its run (the harness asserts
the new text is present and the old text is gone, and prints the region), the suite was run, the failing
guards were recorded **by name**, and the file was restored with `git checkout` from the **committed**
baseline. **A compile error is not a red** — every mutation below compiles and changes behaviour, and the
harness flags a compile error separately so it can never be counted as one.

The parameter was varied deliberately rather than repeated: which symbol the gate resolves, the order it
resolves them in, a dropped condition, a reworded refusal, a transcribed constant, a rounded division, a
transcribed offset, a raw comparison, a collapsed state, a dropped precondition, a bound taken from the
wrong number, a clip instead of a refusal, a dropped guard, a swapped call order, an over-long read, a
misfiled reason, an offered inert field, and a defaulted zero.

Baseline for M1–M17: `8d8e1bb`. For M18 and M19: `01498d6`.

| # | mutation, quoted from disk | the guard that fired |
|---|---|---|
| M1 | `let scratch_raw = match resolve(c, SCRATCH_PROC) {` (was `SCRATCH`) | `the_gate_ignores_the_proc_because_its_name_ships_in_a_release_listing` (+12 others — a broad mutation; the named row failed on its own assertion, printing the two lookups the gate issued) |
| M2 | the arm-cell block moved **above** the destination block | `the_destination_is_resolved_before_the_arm_cell` |
| M3 | the `resolve(c, SCRATCH_ARM)` check replaced by `let _ = SCRATCH_ARM;` | `a_build_with_the_buffer_and_no_arm_cell_is_refused` |
| M4 | the refusal reworded to *"numeric nudging failed: it is broken in this build"*, keeping every symbol name and phrase the row looks for | `the_no_scratch_refusal_names_the_shape_and_the_symbol_and_never_reads_as_broken` |
| M5 | `stride: 32, // NOTE 6.4's figure, transcribed` | `the_band_stride_is_derived_from_the_span_and_is_32_on_s4_and_10_on_demo` |
| M6 | the `% max_bands == 0` clause dropped from `coherent` | `a_span_that_does_not_factor_is_refused_rather_than_rounded` |
| M7 | `equate(c, &field.equate_name())?` replaced by a `match field.key` of NOTE §6.3/§6.4's transcribed offsets | `every_offered_fields_offset_comes_from_an_equate_and_never_from_this_file`, `a_field_whose_offset_equate_is_absent_is_refused_and_not_written_at_zero` |
| M8 | `self.current == self.scratch` (was both sides masked) | `the_install_compare_masks_both_sides_so_either_listing_spelling_matches` |
| M9 | `if self.arm != 0 {` → `if false {` — the two failure states share one sentence | `the_three_install_states_are_told_apart_by_the_arm_byte_and_named_differently` |
| M10 | `if !state.took() {` → `if false {` — writes into an evicted scratch | `a_nudge_into_a_scratch_that_is_not_the_current_config_writes_nothing` |
| M11 | `band >= h.max_bands` (was `band >= s.band_count`) | `a_band_at_or_above_the_installed_count_is_refused_and_not_clamped` |
| M12 | `let value = value.clamp(lo, hi);` and the refusal disabled | `a_value_outside_the_fields_coherent_range_is_refused_rather_than_clipped` |
| M13 | the `forbidden(SCRATCH, …)` guard replaced by `let _ = &forbidden;` | `the_lab_index_guard_fires_on_a_scratch_write_that_resolves_to_the_cursor` |
| M14 | `run_frames` moved **before** the request-byte poke | `arming_writes_the_request_byte_and_runs_exactly_one_frame` |
| M15 | `let want = h.span as usize;` (was bounded by the installed band count) | `the_scratch_read_stops_at_the_installed_band_count` (+5 — the over-long read also breaks the fixture's served length) |
| M16 | `driver`'s `class` changed from `"wrong channel"` to `"geometry"` | `the_refused_list_names_driver_and_rate_shift_as_the_wrong_channel_and_not_as_geometry` |
| M17 | the `bob` field's equate changed to `pcfg_v_factor_fg` — an offered field with no runtime reader | `no_offered_field_is_one_the_note_calls_inert_or_coupled` |
| M18 | `resolved_offsets` defaults an unpublished equate to `0` instead of dropping the field | ⚑ **GREEN on the first run.** See §5.1 |
| M19 | the moved-address clause dropped from `shape_line` | `the_shape_line_names_the_notes_address_when_the_listing_has_moved_past_it` |

### 5.1 ⚑ M18 READ GREEN, AND IT IS THE MOST USEFUL ROW HERE

`resolved_offsets` was changed from *drop a field whose offset equate the listing does not publish* to
*default it to zero*, and **the suite did not notice.** The reason is worth writing down because it
generalises: the existing row covered the **write** path (`nudge` resolves the equate itself and refuses
with the server's own `-32013`), and **nothing covered the draw path**, which reads its offsets out of
`Nudging::offsets`. Two paths, one of them gated, and the gated one's refusal was *hiding* the other.

The consequence is not cosmetic. **Offset 0 is `pcfg_band_count`**, so a defaulted zero makes the control
for an unpublished field **display the band count as that field's value** — a readout that lies, on the one
surface whose entire thesis is that it does not, with a correct refusal on the write ensuring nobody ever
found out.

Two gates were added (`01498d6`) and M18 and M19 were then re-run against that commit: both red, each on its
own guard. **This is the retroactive tightening the method requires** — the campaign's method was
*mutate the decision*, and M18 showed a decision can live in two places with only one of them measured.

---

## 5.2 Aggregates

**The DEBUG profile, which is what CI runs**, at `1f90d41`, wall clock uptime 3 days 6:36 at launch:

```
cargo test --workspace     91 legs   3046 passed   0 failed   10 ignored   exit 0
cargo fmt --all --check                                                    exit 0
cargo clippy --workspace --all-targets -- -D warnings                      exit 0   (exit captured OUTSIDE a pipe)
```

The four frozen-currency suites, **individually**, all from that same run:

| suite | result |
|---|---|
| `determinism_gate` | ok. **2 passed**, 0 failed, in 1.77s |
| `export_state_v1` | ok. **3 passed**, 0 failed, in 0.41s |
| `golden_frames` | ok. **9 passed**, 0 failed, in 0.03s |
| `scanline_goldens` | ok. **5 passed**, 0 failed, in 36.19s |

No golden or fixture file is in the diff (`git diff --name-only f52d226..HEAD` is four files, two of them docs),
so the currencies came back byte-identical rather than re-pinned.

### ⚑ Two failures the DEBUG suite caught that a targeted run did not

1. **`p10_no_dashes_in_shipped_text`: 14 em dashes reached a person through shipped strings.** The rule
   exempts doc comments and `#[cfg(test)]`, and this parcel's prose habits went into the strings the panel
   *draws*. Fixed at `1f90d41` with the rule's own remedies (a colon, a full stop, a comma pair,
   parentheses); no sentence lost a clause. **This is the invariant about verifying in the debug profile
   rather than only in a targeted run, paying for itself in the same session it was written down.**
2. **`the_compiled_in_build_id_still_names_this_tree` failed once, transiently and correctly.** It fired on
   a run that was launched *before* the parcel's commit and read back afterwards: `build.rs` baked
   `f52d226` and HEAD had become `8d8e1bb`. That is the guard working as designed (it exists to catch a
   cached build-script product) and is not a defect in this parcel. It is green on the clean run.
   **Recorded rather than dropped**, because a reader of the earlier log would otherwise find a red with no
   account of it.
3. A third, worth naming because it is an environment fact rather than a result: a fresh worktree has no
   `vendor/` (it is gitignored), so `color_1536_gradient_guard` blocked on a missing test ROM until the
   directory was symlinked to the main tree's. Nothing in the repo changed for it.

## 6. Owed, and not done here

* **⚑ NO RUNTIME CONFIRMATION, AND THIS PARCEL IS THE KIND THAT NEEDS IT.** Nothing here has been exercised
  against a running game: this lane is barred from the emulator MCP tools (`mcp__oracle__*` deadlocks from a
  background seat, with no interrupt path) and from opening a second window while the owner's is live. The
  gate, the three install states, the offsets, the refusals and the range bounds are checked against a fake
  bus whose numbers are read out of the two real listings — and **not** against a machine. **Tagged for
  foreground follow-up, and the final confirmation genuinely needs the owner's eyes on a moving picture:**
  the whole claim is *"turn this number and the parallax changes"*, and no fixture can see that. aeon's own
  `tools/parallax_scratch_probe.py` already drove the engine half (§6.7 of `NOTE`), so what is unproven is
  specifically the panel's half.
* **The `never serviced` install state has never been observed**, only derived — from `HOOK`'s own banner
  about `games/demo`. aeon says in the same place that its refusal arm is *"DEFENSIVE AND UNEXERCISED"*
  because neither shipped fixture can produce its trigger, so two of the three states this panel names are
  argued from source rather than measured. That is stated on the sentences themselves.
* **The equate namespace is assumed to be populated.** `emulator/lookup_equate` is served and vendored, and
  the `parallax_config_*` / `band_entry_*` rows are in `s4.debug.lst` today; a build tool that stopped
  emitting the `Equate Table` would make every control draw unavailable with a truthful hover and no other
  symptom. That is the right failure but it has not been provoked end to end.
