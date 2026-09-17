# CR-W landing: `emulator/screen_text` reports the debug window's drawn panels as `panel` surfaces

**Branch** `parcel/crw-panel`, base `774b4ba`. **Status: built and green; BLOCKED on the hub's contract
commit** (§11.50, the §6 bullets and the schema). The parcel lands together with that text, never before
it. Nothing here was run against an emulator process or a window: every figure comes from `cargo test`
in-process harnesses over the fixture ROM, and whether the harvest reads the way the owner's window looks
is Q2, his look (last section).

**Read at:** CR `docs/proposed/2026-09-17-cr-w-panel-screen-text.md`; the Q3 spike
`docs/2026-09-17-cr-w-q3-spike.md` (where they disagree, the spike was followed); the hub's ruling at
empyrean `629bf21f` (*CR-W RULED*: option (a); Q7 fold ADOPTED; R1 DROPPED; §8 item 31 ADOPTED; Q4 and
Q5 booked), and its follow-up at `4e5251d3` (*spike PASSED*, lockstep accepted).

## 1. What was built

| Commit | What |
|---|---|
| `bb804ef` | `oracle-aether`: `ScreenSurfaceKind::Panel(PanelName)`. The name lives inside the variant and `PanelName::new` refuses `""`, so a named `statusLine`, an unnamed `panel` and an empty name cannot be built (the `PacingFacts` precedent). The handler writes `panel` exactly when the variant carries one. `ScreenSurfaceKind` loses `Copy`; no caller copied it. |
| `3951a4a` | `oracle-player`: the production harvest, form 1. `TabViewer::ui` notes the body's layer and `PaintList::next_idx` around its `match` (`screen::PanelMark`); `build_ui` returns the spans (a local, so per pass by construction); `Loop::publish_screen_text` reads each span from its own layer inside the pass and pushes `titleBar`, `statusLine`, then one `panel` per drawn body in draw order. `screen.rs`'s module doc rewritten: the three-argument refusal is answered point by point from CR §1.1. The spike module became `panel_attribution.rs`. |
| `481c1d4` | The `is_serving` gate observed through `Loop::iterate` (it was pinned for neither the bar nor the panels). |
| `3fe04ee` | `oracle-aether` tests: a hosted wire row for panel surfaces; the CR-W vectors row, ignored until the re-vendor. |
| `fb0be6c` | W10 harness corrected (blocks, not per-present interleaving; see §4). Docs: `MAX_SCREEN_SURFACES` counts panels; `oracle-frontend`'s module note says the enum has six values and why it never produces `panel`. |
| `c7ad4bd` | An ignored capture that prints real replies for the vector file's cases 1-4. |
| `36c3871` | W5's real-body sweep was vacuous; fixed (see §3). |

**The reading rule as built** (`crates/oracle-player/src/screen.rs`, module doc and `glass_run`/`join`):
- A **run** is one `Shape::Text` in the span with at least one glyph whose logical rectangle meets its clip.
- `text` is `Galley::text()` (the source). `rendered` is the glyphs that meet the clip, the elision mark included.
- A source LF is a row break, not a glyph, so its folded space goes back into `rendered` between visible glyphs.
- TAB and LF are folded to a space in both strings (Q7).
- **Rows:** a run joins the current visual row when the centre of its first visible glyph row lies inside the band of the row's topmost run. Rows go top to bottom joined by LF; runs go left to right joined by TAB, identically in both strings.
- `unrenderable` runs over each run's source, per `LayoutJob` section family, through the existing `Glyphs` probe. Only `Some(false)` counts.
- A drawn panel with no run is present with `""`.
- **Recording and publishing are gated on `is_serving`,** decided once per iteration (`main.rs`, `Loop::iterate`).

**Served-method doc.** `git grep -ln screen_text` finds no oracle document that describes the served method
outside dated notes, the overseer files and the CR itself; the method's contract text is empyrean's
`protocol.md`. So no oracle protocol doc was changed.

**Not built, per the ruling:** R1 `initialize.capabilities.screenTextKinds` (DROPPED, so W9 does not exist);
Q4 `F-PANEL-SCROLL-UNSTATED` and Q5 `F-SCREEN-TEXT-TRANSIENT-AREAS` (booked). Form 2 is not production and
is not re-tested (its record stays at `5a51da1`).

## 2. Conformance rows, and where each is tested

All player rows are in `crates/oracle-player/src/panel_attribution.rs` (`tests`), and each drives the
shipped `Loop::build_ui` plus `Loop::publish_screen_text`, then reads the reply through `Bus::call`.

| Row | Test | What it asserts |
|---|---|---|
| **W1** drawn set | `w1_a_panel_surface_exists_exactly_when_its_body_is_drawn` | Registers/Memory/Objects share a leaf: only the active one has a surface; activating Memory swaps the names; collapsing the leaf removes all three while the reply still succeeds with the other leaves. §8 item 31's anti-vacuity: the first read has non-empty panel text. Also: `every_drawn_tab_body_is_attributed_completely_and_exclusively` (spans equal each non-collapsed leaf's active tab over 13 arrangements) and `a_floating_window_...` (a floating window is drawn and reported after the main surface). |
| **W2** name | `every_drawn_tab_body_is_attributed_completely_and_exclusively` | The reply's panel names equal the drawn tabs' `Tab::title`, in draw order, in every arrangement. |
| **W3** alignment | `assert_aligned`, called by the attribution gate (every arrangement), the scales row, W4 and W5 | Equal LF count; equal TAB count per row. The gate requires more than 100 surfaces checked. |
| **W4** no false truncation | `w4_an_unelided_unclipped_run_renders_exactly_its_source` | Every unelided run whose glyphs all lie inside its clip has `rendered == text`, over real bodies plus a planted wrapped label and a planted TAB. Printed: `W4: 322 whole unelided runs, rendered == text; wrapped label on 5 rows` (`cargo test -p oracle-player --bin oracle-player -- --nocapture w4_`). |
| **W5** elision | `w5_an_elided_run_is_truncated_and_ends_in_the_elision_mark` | A planted `Label::truncate` in 40 pt: `Galley::elided` is true (control), the run ends in `…`, `truncated: true`, and the cut run is on the same row in both strings. Real half: narrow stopping panes, at least one elided unclipped run, each ending in the mark (`W5: 1 real elided, unclipped runs`, same command with `w5_`). |
| **W6** clipping | `w6_a_clipped_run_is_whole_in_text_and_cut_in_rendered_and_an_unseen_run_is_absent` | A run straddling the pane's right edge: whole in `text`, a strict prefix in `rendered`, `truncated: true`. A run wholly outside: painted into the span (control) and in neither string. Warmed up by 12 presents. |
| **W7** boxes | `w7_a_character_the_window_cannot_draw_is_named_and_a_drawable_one_is_not` | The Watchpoints add box holding `A`: `unrenderable: []` (control). Holding U+6F22: `["漢"]`. |
| **W8** blank | `w8_a_drawn_panel_with_no_text_on_the_glass_is_present_and_empty` | Real, not faked. In `every_tab_dock` at 1600x1000, **Profiler's body runs and paints no text shape at all**, and **Watchpoints paints two text shapes with no glyph on the glass**. Both are present with `""`, `truncated: false`, `unrenderable: []`. Found by a scratch sweep of that dock at four window sizes, not committed. |
| W9 | none | R1 was dropped. |
| **W10** cost | `w10_publish_cost_per_present` (ignored) | §4. |
| is_serving | `a_window_no_client_can_reach_publishes_no_panels` | An unserved loop runs `iterate` and `screen_text` still refuses `noDisplay`; control: every body draws. |
| end to end | `loop_tests::a_client_reads_this_windows_top_bar_and_it_follows_the_run_state` (`main.rs`) | A real socket client reads `total == 2 + drawn`, and the served panel names equal the drawn dock's titles in draw order. The pinned `2` it used to carry now derives from the dock. |
| wire | `a_panel_surface_carries_its_name_and_no_other_surface_carries_one`, `a_panel_name_is_never_empty_and_is_otherwise_verbatim` (`oracle-aether` lib); `a_player_that_published_panels_serves_each_by_name_after_the_bar` (`tests/hosted.rs`) | `panel` appears on exactly the panel kind; the whole hosted reply is pinned. |

**Spike gates carried over, run on the production path** (the module doc has the full old→new table):
- the attribution gate: 13 arrangements, every drawn body exact against the independent ground truth;
- the planted interleaving;
- the combo list and the tooltip, each in its own layer and not attributed;
- TextEdit contents and hint, plus spike correction 5: empty galleys are painted and are not runs;
- the floating window;
- the narrow Screen overlay;
- the discarded first pass;
- 1.25 and 2.0 ppp.

Two spike controls read the probe's own view from inside the body (`at_leave`, `layer_at_leave`). They
have no production counterpart and were removed with the probe.

**`screen.rs` unit rows** (hand-laid galleys):
- `a_runs_own_line_breaks_and_tabs_are_folded_identically_in_both_strings`
- `a_clip_decides_which_glyphs_are_on_the_glass`
- `runs_join_into_visual_rows_left_to_right_identically_in_both_strings`
- `a_panel_box_is_attributed_to_the_family_of_its_section`

## 3. Red-first

Every mutation was applied on disk, run with `cargo test`, and restored with `git checkout <committed sha> -- <file>`.
- E1-E3 were against `bb804ef`.
- P1-P13 were against `481c1d4`, driven by a scratch script. Each P row ran `cargo test -p oracle-player --bin oracle-player -- panel_attribution screen::tests a_client_reads_this_windows_top_bar`, a filter that includes the existing screen-text rows.
- The retro P7 was against `36c3871`.

| # | Mutation | Red? | Went red |
|---|---|---|---|
| E1 | the handler never writes `panel` | red | `a_panel_surface_carries_its_name...`; hosted `a_player_that_published_panels...` (re-run at `3fe04ee`). **Existing locks stayed GREEN:** hosted `a_player_that_published_its_screen_serves_every_surface_with_both_strings`, `a_blank_screen_succeeds...`, `the_screen_served_is_the_frame_the_raster_drew`, and `screen_text.rs`'s refusal row. No existing row published a panel. |
| E2 | `panel` written on every surface | red | the new engine row, **and the existing** hosted `a_player_that_published_its_screen_serves_every_surface_with_both_strings` (whole-reply pin) |
| E3 | `PanelName::new` accepts `""` | red | `a_panel_name_is_never_empty...` |
| P1 | span `end` one shape short | red | 9 player rows (attribution gate, planted, combo, window, overlay, scales, W4, W5, W6) |
| P2 | read `LayerId::background()` instead of the span's layer | red | only `a_floating_window...`. It is the only arrangement with a second layer, as the spike found for M3. |
| P3 | span list "not reset per pass" | **stayed GREEN: the mutation was vacuous.** It read a thread-local that nothing ever wrote, so it was equivalent to production. | none |
| P3b | P3 corrected: the list is stored and re-read across passes | red | 14 rows, including `a_discarded_first_pass...` and the existing `a_client_reads_this_windows_top_bar...` |
| P4 | no TAB/LF fold in `text` | red | `a_runs_own_line_breaks...`, W4 |
| P5 | source LF not put back in `rendered` | red | `a_runs_own_line_breaks...` only. **No real body in the swept arrangements carries a multi-line label that W4 checks.** |
| P6 | the whole galley counts as rendered when any glyph is visible | red | `a_clip_decides...`, W6 |
| P7 | elision mark dropped from `rendered` | red | W4 (a real label contains `…` as text), W5. Retro against the rewritten W5 at `36c3871`: red. |
| P8 | monospace sections asked as proportional | red | `a_panel_box_is_attributed_to_the_family_of_its_section` only. W7 stays green, because U+6F22 is a box in both families. |
| P9 | panels published before the bar | red | the existing end-to-end `a_client_reads_this_windows_top_bar...` |
| P10 | no row grouping | red | `runs_join_into_visual_rows...` only. **W3 stays green by design:** the joins are shared, so alignment holds under any grouping. |
| P11 | panels in reverse draw order | red | the attribution gate (W2), `a_floating_window...`, end to end |
| P12 | blank panels omitted | red | W8, the attribution gate |
| P13 | publish when not serving | red | `a_window_no_client_can_reach_publishes_no_panels` |

**Existing `snapshot` locks** (the six `screen::tests` rows about the bar) stayed green under every P
mutation, as expected: none of them touches panels.

**Interleaving planted:** `a_planted_interleaving_is_reported_foreign` paints one extra string into each
recorded body before its span closes. The harvest reports exactly that string as foreign on all four
default-dock tabs, and the served panel text contains it.

**Absence claims audited:**
1. The hosted row was drafted as `#[ignore]` on the belief that its client validates against the schema. Run with `--ignored`, it was GREEN, because `tests/hosted.rs` has its own `Client` with no schema check. It is not ignored, and its doc says so.
2. The spike's "no production body calls `set_clip_rect` or opens its own `Area`" was re-checked at `481c1d4` by `grep -n "set_clip_rect\|Area::new\|egui::Window::new\|set_transform_layer" crates/oracle-player/src/*.rs`. The only hits are the two modal `Window`s (`palette.rs:360`, `rom_open.rs:660`) and test code. The grep can see a hit.
3. W5's real sweep asserted nothing at first: 0 elided runs in the every-tab and default docks at 1600x1000. It now uses narrow panes and asserts at least one.

## 4. Cost (W10)

**Command:** `CRW_ROUNDS=2000 cargo test --release -p oracle-player --bin oracle-player w10_publish_cost -- --ignored --nocapture` at `fb0be6c`.
- `uptime` load average: 3.71 before, 3.48 after.
- Harness: `Loop::build_ui` at 1600x1000, ppp 1.0, fixture ROM armed, no emulation.
- Arms run in blocks of 20 consecutive presents, rotating across arms; the first 5 of every block and two whole warm-up blocks per arm are discarded. n = 1995 per arm.

| Dock | Arm | Present median | p95 | Publish median | p95 | Reply bytes |
|---|---|---|---|---|---|---|
| default | null (not serving) | 0.248 ms | 0.257 | 0 | 0 | |
| default | bar only (pre-CR-W publish) | 0.258 | 0.268 | 0.0105 | 0.0111 | |
| default | bar + panels (production) | **0.294** | 0.308 | **0.0445** | 0.0471 | |
| default | bar + panels + one reply serialised | 0.301 | 0.316 | 0.0510 | 0.0542 | 3364 |
| every-tab | null | 1.022 | 1.048 | 0 | 0 | |
| every-tab | bar only | 1.031 | 1.060 | 0.0125 | 0.0130 | |
| every-tab | bar + panels | **1.090** | 1.118 | **0.0653** | 0.0708 | |
| every-tab | bar + panels + one reply | 1.108 | 1.138 | 0.0799 | 0.0862 | 5253 |

**CR-W's increment over the pre-CR-W publish:** +0.036 ms present median on the default dock and +0.059 ms
on every-tab. Adding one reply per present brings those to +0.043 and +0.077 ms, about 0.3-0.5% of a
16.7 ms frame. A window no client can reach pays none of it (the null arm, and P13's row).

**A measurement finding, recorded because the first harness got it wrong.** The spike's harness rotated
arms present by present. Measured that way first (at `3fe04ee`, `CRW_ROUNDS=2000 cargo test --release -p oracle-player w10_publish_cost -- --ignored --nocapture`, load 5.58 before / 5.18 after),
the publish arm came out **bimodal**: ~0.26 ms after a present that had not published, ~0.06 ms after one
that had. The cause: `Glyphs` lays out every character it probes, and egui keeps a galley cached only while
the previous pass used it. A serving window publishes on every present, so its steady state is the warm
case. Interleaving measured a cold cache the product never has. The spike's 0.012/0.022 ms figures left out
the glyph probe and the push, as its own §4 says, so they are not comparable to the numbers above.

## 5. Totals

The commands were `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `cargo test --workspace --release`, all with `CARGO_BUILD_JOBS=4`. The base ran on the untouched `774b4ba`, the tip on `36c3871`, the last code commit; the commit that adds this note changes only this file. Counts are summed from each run's `test result:` lines, and 91 legs means 91 `Running`/`Doc-tests` headers, each with a result line.

| | fmt | clippy `-D warnings` | `cargo test --workspace` | `cargo test --workspace --release` |
|---|---|---|---|---|
| base `774b4ba` | clean | clean | 91 legs, 2999 passed, 0 failed, 9 ignored (3008 named result lines) | not run at base (the brief asks for release at the tip) |
| tip `36c3871` | clean | clean | 91 legs, **3013 passed, 0 failed, 11 ignored** | 91 legs, **3016 passed, 0 failed, 8 ignored** |

**Debug delta, reconciled by name** (sorted `test <name> ... <status>` lines of the two logs, diffed):
- **Passed: +14** (2999 → 3013).
  - Removed, 9: the spike's `crw_q3_spike::tests::*` gates.
  - Added, 23:
    - `panel_attribution::tests::*`, 16: the 9 carried-over gates, `a_window_no_client_can_reach_publishes_no_panels`, and W1, W4, W5, W6, W7, W8.
    - `screen::tests::*`, 4 new.
    - `engine::tests::*`, 2.
    - hosted `a_player_that_published_panels_serves_each_by_name_after_the_bar`, 1.
  - 2999 − 9 + 23 = 3013.
- **Ignored: +2** (9 → 11).
  - Removed: the spike's `harvest_cost_per_present`.
  - Added: `w10_publish_cost_per_present`, `capture_cr_w_vector_replies`, and `the_cr_w_vectors_validate_the_way_the_file_says_they_do` (the one awaiting the re-vendor).
- One name, `boot_read_exists_and_is_measurable`, appeared in the diff only because that test's own stdout split its result line at the tip. It passed in both runs.

**Release against debug at the tip:** +3 passed and −3 ignored. `one_pass_repairs_four_stale_checkpoints_and_reproduces_the_pristine_image`, `the_slide_fixture_runs_green` and `the_standing_fixture_runs_green` are ignored in debug and run in release. That was true before this parcel.

## 6. Blocked: the schema re-vendor

**No empyrean SHA carrying §11.50 and the schema had been sent when everything else was green.** Stopped
there as instructed. The vendored bytes are content-addressed against `PROVENANCE.md`'s pins, so they cannot
be hand-patched.

**Ignored awaiting the re-vendor (the complete list):**
- `crates/oracle-aether/tests/screen_text.rs::the_cr_w_vectors_validate_the_way_the_file_says_they_do`, `#[ignore = "awaiting CR-W contract re-vendor"]`.
- Measured red today with `cargo test -p oracle-aether --test screen_text -- --ignored cr_w`: *case 1 is declared passing and the schema REFUSED it: ... "panel" is not one of "statusLine", "toast" or 3 other candidates*.
- It skips rider R1's five cases by their `[CR-W rider R1]` tag.

**The re-vendor, ready to run** (the `PROVENANCE.md` recipe; `REV` is the hub's carrying commit):

```sh
REV=<hub SHA>
git -C ../empyrean fetch
git -C ../empyrean merge-base --is-ancestor $REV origin/main && echo ancestor
git -C ../empyrean show $REV:contract/schema/bus-protocol.schema.json > crates/oracle-aether/tests/contract/bus-protocol.schema.json
git -C ../empyrean show $REV:contract/schema/tests/vectors.json      > crates/oracle-aether/tests/contract/vectors.json
git -C ../empyrean rev-parse $REV:contract/schema/bus-protocol.schema.json $REV:contract/schema/tests/vectors.json
git hash-object crates/oracle-aether/tests/contract/bus-protocol.schema.json crates/oracle-aether/tests/contract/vectors.json
wc -c crates/oracle-aether/tests/contract/bus-protocol.schema.json crates/oracle-aether/tests/contract/vectors.json
# then: both pin.* triples in PROVENANCE.md at the SAME revision, both Current-copy tables re-derived by parsing,
# a leaf-path structural diff against the copy replaced, stated in full.
```

**Then, in the same commit:**
1. Remove the `#[ignore]` above and run it.
2. Replace the vector file's cases 1-4 with the lines printed by `cargo test -p oracle-player --bin oracle-player capture_cr_w_vector_replies -- --ignored --nocapture` (wrapped in each case's existing `method`/`kind`/`expect`/`why`, with `why` stating that it is a capture).
3. Mark or strip R1's cases 12-16, since R1 was dropped.
4. Run the whole suite. The hub's text may add vectors to `vectors.json` itself, and `schema_conformance` runs those.

**Premise to re-check at the SHA:** the hub may spell the schema differently from CR §5.1, for example
without the `if`/`then`/`else`. The serve emits exactly `kind`, `panel` (on panel kinds only), `text`,
`rendered`, `truncated` and `unrenderable`. Any other required key is a stop.

## 7. Look calls for the owner (Q2, tagged, not claimed)

No seat can open the window. These are the checks only he can make, each with a reply beside his screen:
1. **A cut cell.** At 1600 wide, with only Registers and Breakpoints drawn, the capture found Registers' `aether` line cut at the pane's right edge: `rendered` ends at *"...nothing can attach to this window"*, with no elision mark, and `text` continues *"(pass --aether, ...)"*. Does the glass cut it there? It is also a plain UX question: a sentence clipped mid-parenthesis in the default arrangement.
2. **Row order of stat cards.** Watchpoints' readout cards serialise as the value row (`268810\t38398\t34302`) before the label row (`seen\tmatched\tdropped`), because the values sit higher on the glass. Is that how it reads?
3. **A scrolled table and a collapsed pane.** Rows scrolled out are in neither string. A collapsed pane has no surface.
4. **A floating window** is reported after the main surface's panels.

**Also for the record, from the build:**
- P5 showed that no real body in the swept arrangements carries a multi-line label W4 checks, so the LF-restore path is proven by the unit row alone.
- P10 showed that W3 cannot see a grouping error by design, so row grouping is proven by the unit row alone.
