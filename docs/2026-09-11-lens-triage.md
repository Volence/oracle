# Lens triage — 2026-09-11

Re-verification of every still-open MEDIUM/LOW row from the 2026-09-06 lens sweep
(`docs/superpowers/notes/2026-09-06-oracle-lens-sweep.md`, pin `d3ca871`) against HEAD `3a85bf2`, so fix
parcels are spent only on findings that are still real. Read-and-reason only: **no cargo was run, no
emulator was touched.** Where a verdict needs a run, the row is **TAG** with the question and the command.

*(In progress — rows are added in batches; the parcel plan is written last.)*

## Population

`docs/lens-findings.jsonl` at `3a85bf2`: 234 lines, 140 ids, resolved last-line-wins per id.
Latest-state tally: 51 `medium` open, 8 `low` open, 1 `high` open (H22, a design question, out of
scope), 2 `tagged` open (T1, T2, skipped), 6 `clean/holds` (K*, skipped), plus 5 critical / 33 high /
25 medium / 4 low `fixed` and 5 `refuted`. **In scope: 59 − L2 = 58 rows** (L2 is folded into the
running `debug_read` parcel and not judged here). The brief's count (51 + 8) reproduces exactly.

Most in-scope rows carry no `detail` on the ledger (only a compressed `title`), so every row was judged
from its packet paragraph, not its title.

## Verdicts

Coordinates are HEAD `3a85bf2`. Size is the fix, not the finding.

| id | verdict | where (HEAD) | evidence | size |
|---|---|---|---|---|
| M5 | HOLDS | `crates/oracle-core/src/system.rs:876-878` | `// FM / PSG remain fixed all-zero placeholders (they fill when those chips land).` then two `repeat_n(0u8, …)` — while `system.rs:310` documents the live YM2612 timers. `export_state_hash` still omits the FM chip's behavioural state. | M (moves the export golden — measure-first) |
| M7 | HOLDS (design question) | `crates/oracle-aether/src/engine.rs:1649`, `:1742-1745`, `:772-779` | `ScreenSurfaceKind` still names five pieces of one window's chrome; `target_fps`'s doc is written in terms of a consumer's CLI flag (`--target-fps 0`); `MAX_SCREEN_SURFACES`'s doc justifies itself by `MAX_TOASTS`, defined upward in `oracle-frontend/src/overlay.rs:40`. Architecture call, not a fix. | — (owner) |
| M8 | HOLDS | `crates/oracle-panels-spike/` (still `exclude`d, `Cargo.toml:24`) | 0 `#[test]` in `src/main.rs`/`src/stats.rs`; `stats.rs:31`/`:41` still `return 0.0` on an empty distribution; `main.rs:41` still says oracle-frontend is "a binary crate over there (no lib target)" while `oracle-frontend/Cargo.toml:11` documents `src/lib.rs`. Only commit since the pin is `2c04414` (H25 moved its frame reader onto the shared one). | S (owner call: delete crate, keep `run.sh`) |
| M11 | HOLDS | `engine.rs:3663`, `crates/oracle-player/src/machine.rs:355`, `crates/oracle-frontend/src/main.rs:730` | Three independent masked-frame loops, each learning width by rendering line 0; none calls `Vdp::active_display()` (`crates/oracle-core/src/render.rs:1109`, whose doc still says "When V30 lands, it lands here"). `machine.rs:351-353` still names the width-probe off-by-one hazard. No cross-crate comparison test exists (the two masked tests, `oracle-frontend/src/drain.rs:942` and `main.rs:3030`, compare within one crate). | M |
| M12 | HOLDS (latent) | `crates/oracle-core/src/symbols.rs:397-416`, `:866-874`, `:1234-1243` | `name()`'s doc promises a round-trip through `address_of`; `build()` deliberately leaves same-address aliases non-ambiguous ("Aliases at the *same* address are not ambiguous"), so `name()` returns the shared demangled spelling, and `address_of` answers `None` for any demangled name with `v.len() != 1`. | S |
| L1 | HOLDS | `crates/oracle-core/src/system.rs:797-800` | `restore` is `bincode::decode_from_slice(...)?; Ok(system)` — no post-decode shape check. | S |
| L3 | HOLDS | `crates/oracle-core/src/symbols.rs:1613` vs `:1586` | `parse_body_line` does `u64::from_str_radix(hex, 16)` with no digit check (`from_str_radix` accepts a leading `+`); its twin `parse_table_entry` guards `!v.bytes().all(\|b\| b.is_ascii_hexdigit())` at `:1586`. | S |
| L4 | HOLDS | `crates/oracle-core/src/render.rs:552` (doc: "even, `0..CRAM_SIZE`"), consumed unmasked at `:899-900` | `cram[l.addr]` / `cram[l.addr \| 1]` with no check in `journal_cram` (`:641`); one production caller (`system.rs:1196`), two test callers (`render.rs:2675`, `:2823`). | S |
| L5 | HOLDS (grew) | `crates/oracle-aether/src/engine.rs`, `crates/oracle-player/src/ui.rs` | engine.rs 10,801 → **11,132** lines since the pin, ui.rs 6,036 → **8,354**. The row's "10,108 / 5,922 code" does not reproduce under any spelling tried (non-blank: 10,367 / 5,797 at the pin; non-blank non-comment: 5,944 / 4,067), so treat the figures as the seat's own measure; the direction is unambiguous. The player now imports ~13 names from `oracle_aether::engine` (METHODS, MethodSpec, ScreenSurface/Kind, symbol_at, breakpoint_wire_id, watch_wire_id, LastBreak, FrameTimes, PacingAudio, PacingFacts, absolutise, EngineConfig, Rgb) — wider than the row's six. | L (owner/arch) |
| L6 | CHANGED (4 of 5 hold, 1 fixed) | see evidence | **Fixed:** `oracle-replay/src/policy.rs`'s drifted citation — now cites by symbol (`390e020`, a code-file commit touching `policy.rs`). **Hold:** (a) `tools/land.sh:146` still says `git grep lane-status -- '*.rs' '*.py' '*.sh'` "is empty"; the same command today returns 15 lines (tools/land.sh ×11, tools/lane-check.py ×4; the seat counted 10 at the pin) — none `.rs`, so the *conclusion* "nothing compiled reads it" survives, the stated *measurement* does not); (b) `fixtures/aeon/DIMENSIONS.tsv:19-20,22-23` cite `engine.rs:6813`/`:6702` for `equates_with_prefix`/`with_prefix`, now at `engine.rs:7312`/`:7188` (drift grew to ~500 lines); (c) `crates/oracle-core/tests/aeon_dimensions.rs:6` "Three sightings" while `DIMENSIONS.tsv:16` says "Four"; (d) `crates/oracle-aether/tests/contract/PROVENANCE.md:42` "276 cases" where `:226` records 295. | S |
| L7 | CHANGED | `engine.rs:10306`, `oracle-frontend/src/main.rs:751`, `oracle-frontend/src/lens/profile.rs:40`; `engine.rs:5978` | Three `MAX_SYMBOL_DISPLACEMENT = 0x1000` definitions still stand (two in the frontend). `z80_read` still bounds `len` with a literal `0x2000` (`engine.rs:5978`) while `Z80_RAM_SIZE` is imported (`:39`) and used at `:6023`/`:9789`. **No longer true:** "`WORK_RAM_LO` is a name collision with different values" — `65124d1` (H28, code, `oracle-replay/src/lib.rs`) made replay's `0x00E0_0000`, equal to `engine.rs:78`, with a guard test at `lib.rs:138`. What remains of that half is two same-valued copies. | S |
| L8 | HOLDS | `crates/oracle-aether/tests/schema_dryrun.rs:221`, `:286`; `crates/oracle-frontend/src/main.rs:4060` | Exactly three plain `#[ignore]` (the replay playthroughs use `cfg_attr(debug_assertions, ignore)` deliberately and are not this row). No booking row: `docs/OVERSEER.md` and `docs/lane-status.json` have no `dryrun`/`SHOT_ROM`/`#[ignore]` hit; the doc hits are design notes and logs, not register rows. | S |
| M52 | **FIXED-ELSEWHERE** `0c39449` | `crates/oracle-core/src/symbols.rs:1159` | At the pin: three `fn integrity_note` (frontend `symbol_file.rs:125`, player `symbols.rs:194`, replay `policy.rs:85`). At HEAD: one, in oracle-core, and all three crates call it. `0c39449` is a code commit over those files and says it closes M52. Ledger line appended. | — |
