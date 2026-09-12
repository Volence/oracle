# Lens triage — 2026-09-11

Re-verification of every still-open MEDIUM/LOW row from the 2026-09-06 lens sweep
(`docs/superpowers/notes/2026-09-06-oracle-lens-sweep.md`, pin `d3ca871`) against HEAD `3a85bf2`, so fix
parcels are spent only on findings that are still real. Read-and-reason only: **no cargo was run, no
emulator was touched.** Where a verdict needs a run, the row is **TAG** with the question and the command.

**Result: 58 rows — 50 HOLDS, 6 CHANGED, 2 FIXED-ELSEWHERE (M26, M52), 0 REFUTED, 0 TAG.** Two
ledger lines appended (M26, M52); nothing else on the ledger moved. The sweep's findings have aged
well: in five days of heavy landings, the fixes aimed at its *highs* closed only two of these
mediums outright and narrowed six.

Method, and its limits. Rows were split across three read-only helpers plus this session, by area
(protocol/aether, oracle-core, duplication/idiom); every FIXED-ELSEWHERE and every CHANGED claim was
re-checked here from the commit and the file, and at least three HOLDS per helper were spot-read.
Most rows carry only a `title` on the ledger, and the packet's own paragraphs for the second, third,
ninth and tenth tranches say *"abbreviated; full derivations in the seat reports"* — **those seat
reports are not in this repo**, so rows such as M13-M24 and M61-M77 were judged from one-paragraph
summaries. A candidate fixing commit was searched by id in every commit body since the pin
(373 commits, `d3ca871..3a85bf2`) as well as by token (`git log -S`/`-G`).

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
| M19 | HOLDS (wider) | `crates/oracle-frontend/src/sram_file.rs:36-38`; F5 at `crates/oracle-frontend/src/main.rs:2147` | `load_srm` is `std::fs::read(path).ok()`; the F5 path is `if let Some(saved) = …` with no else. Floor widened: `main.rs:1221` has an else that says "no save yet" for a read error too; `crates/oracle-player/src/battery.rs:103` routes through the same `load_srm` while its own doc (`:96-97`) says the two are different facts, and `battery.rs:256` is another no-else `if let Some`. | S–M |
| M50 | HOLDS | `crates/oracle-player/src/stopping.rs:768-775` vs `crates/oracle-player/src/memory.rs:529-533` | `target_params` still sends a bare hex word as `{"addr": t}` under a comment claiming it follows `memory::resolve_address`'s precedent; the server's `hex::parse_addr` (`crates/oracle-aether/src/hex.rs:52-55`) refuses a bare word. An all-hex-letter symbol (`beef`) is never looked up. `2e5c062` changed the Memory box (default base + bus mask) but not this. The covering test (`stopping.rs:1571-1590`) tries only `0xFF0000` and `Player_1`. | S (the `stopping.rs` half; any change to the Memory box's rule waits for the running parcel) |
| M51 | HOLDS | `crates/oracle-aether/src/main.rs:168` | `binding => {` is the accept arm after `Mismatch` and `Indeterminate(_) if !is_intact`; `binding_note` twenty lines below is exhaustive with the doc against `_` arms; the four sibling tables (player `symbols.rs`, frontend `symbol_file.rs`, replay `policy.rs`, `engine.rs:7362-7396`) are exhaustive. | S |
| M53 | HOLDS (count exact) | `engine.rs:85` `ACTIVE_LINES`, `oracle-frontend/src/main.rs:341` `HEIGHT`, `oracle-player/src/machine.rs:19` `HEIGHT`, `oracle-panels-spike/src/main.rs:59` | Exactly four production constants equal to the display height, and still no owner in `oracle-core`, which spells it as bare literals (`render.rs:1110` inside `active_display()`, `system.rs:1334`/`:1344`) and as `vdp.rs:72` `VBLANK_START_LINE = 0xE0`. Same finding as M67; M8's spike deletion would make it three. | M (with M67) |
| M67 | CHANGED | as M53, plus 8 test/example copies | Named `= 224` constants: 12 at the pin, **14** at HEAD (new: `oracle-core/tests/color_1536_gradient_guard.rs:87`, `oracle-core/examples/render_perf.rs:40`); the pin search re-run at HEAD finds all 12 pin sites (the control). **Two of the 14 are not the display height** — `oracle-aether/tests/object_mutation.rs:97` and `oracle-frontend/src/bus.rs:972` `SCREEN_HEIGHT` are aeon's player-bound inset that happens to equal 224, and a dedupe must not fold them in. "Ten, three cite a source" does not reproduce; about 5 of 14 cite one. Not seen: arithmetic spellings (`28 * 8`) and >200 unnamed test literals. | M (with M53) |
| M61 | HOLDS (count exact) | `crates/oracle-core/src/testrom.rs:43` (`const INNER = 0x0000_020E`, private) + 7 `HOT_PC` sites | Seven named `HOT_PC`, the row's count exactly: strings at `oracle-aether/tests/breakpoints.rs:31`, `tests/watch_hits_epoch.rs:633`; `u32` at `tests/hosted.rs:689`, `src/host.rs:1764`, `oracle-player/src/bus.rs:1679`, `oracle-player/src/main.rs:1802`, `oracle-frontend/src/bus.rs:753`. The sibling is published (`pub const TRAP_HANDLER_ADDR`), `INNER` is not. Unnamed `"0x0000020E"` copies beyond the seven: `server.rs:920/1018/1061`, `stopping.rs:1691/1713`. | S–M |
| M66 | HOLDS (wider) | `crates/oracle-aether/src/objreq.rs:139,141` vs `crates/oracle-frontend/src/spawn.rs:103,105` | `LEVEL_WIDTH_SYMBOL`/`…HEIGHT…` in both, both restating aeon's measurement; frontend's `oracle-aether` dependency is `optional = true` (`Cargo.toml:40`) while `spawn.rs` is unconditional; no gate compares them. Unbooked second instance: `oracle-frontend/src/rings.rs:82,84` vs `oracle-player/src/objects.rs:427-432` — there the player already links `oracle-frontend` unconditionally, so "import it" *is* the fix. | S–M |
| M68 | HOLDS | `crates/oracle-aether/tests/object_mutation.rs:74-75` vs `crates/oracle-frontend/src/bus.rs:943-947` | `W_PLACE = 0x00FF_971A` in one, `0x00FF_9720` in the other, `W_SLOT` only in the first; values unchanged since the pin; nothing says which divergence is intended. | M |
| M69 | HOLDS | `crates/oracle-player/src/ui.rs:6649` | `const ALL: [&str; 8] = ["up", …, "start"]` then `assert_ne!(shown, ALL.join(", "), …)`; the real list `BUTTONS_3` (`engine.rs:10020`) is private, so the player cannot import it today. | S |
| M74 | HOLDS (latent) | `crates/oracle-player/src/stopping.rs:752`, `:922`; `crates/oracle-player/src/ui.rs:2555`, `:2581` | `pub const WATCH_SPACES: [&str; 4]` + `pub w_space: usize`, indexed unchecked; real enums exist (`memory.rs:64 enum Space`, core `watchpoints.rs:153 enum WatchSpace`). Only the combo box writes the index today. | S |
| M76 | HOLDS | `crates/oracle-core/src/io.rs:125,131` vs `:139-150` | `set_pad`/`pad` `assert!(port < 2, …)` while `read_data` returns `Pad::default()` for port 2; the engine layer (`engine.rs:2174`) folds `held[port & 1]`, a third behaviour, then `host.rs:338`. | M |
| M77 | HOLDS | `crates/oracle-player/src/memory.rs:412` | `payload.trim().trim_start_matches("0x")` strips repeatedly; `0X41`/`$41` fail the `is_ascii_hexdigit` check at `:416`; the blessed `hex::parse_bytes` strips exactly one of `0x`/`0X`/`$` (`hex.rs:114-117`). **Touches `memory.rs` — waits for the running parcel.** | S |
| M20 | HOLDS | `crates/oracle-core/src/render.rs:1066-1067` | `last_completed = now_mclk / MCLK_PER_FRAME - 1`, but the frame boundary is line 224 (`system.rs:1342-1347`: "this instant, and not line 0, is the boundary"), so lines 224-261 compare against the frame before. The tests lock the defect in: `render.rs:2359`/`:2395` call it at `MCLK_PER_FRAME - 1` expecting "no frame has completed", and the helper `line_drew_at` (`:2280`) transcribes the implementation. No change to `last_completed` since the pin. | S (the caveat a client reads changes; emulation bytes do not) |
| M21 | HOLDS | `crates/oracle-core/src/system.rs:1509` | The gate-closed arm is `self.z80_frontier_mclk = now;` unconditionally, after a stepping loop (`:1489-1506`) that can leave the frontier past `now` — so a close rolls it backward. | S (**moves timing bytes — measure first**) |
| M22 | HOLDS (deferred on purpose) | `crates/oracle-core/src/vdp.rs:1333-1344` (`run_copy`) | Copy writes `self.addr as usize & (VRAM_SIZE - 1)` with no `^ 1`; the fill arm (`:1300`) applies it, quoting the source that names copy too, and `:1298` records that `run_copy` was left alone because no vendored test covers it (open question Q2). Hardware sub-question left open, not adjudicated here. | S (**moves emulation bytes — measure first**) |
| M24 | HOLDS | `crates/oracle-core/src/z80/mod.rs:958-968`; `crates/oracle-core/src/system.rs:1359-1362`, `:1377-1381` | EI sets both flip-flops at once ("EI's one-instruction delay is unobserved by the SST-z80 gate"); `/INT` rises at VInt and drops only at line 0 of the next frame. | M (**moves emulation bytes — measure first**) |
| M30 | HOLDS | `crates/oracle-core/src/vdp.rs:160-164` | `vram`/`cram`/`vsram` are still `Vec<u8>`, accessors return `&[u8]` (`:417-427`). The per-dot cost remains the packet's call-count reasoning, not a profile. | M (check the save-state layout fingerprint; perf is measure-first) |
| M31 | HOLDS (narrowed by C5) | player `crates/oracle-player/src/bus.rs:1503`; frontend `crates/oracle-frontend/src/drain.rs:232`, `:276` | Both windows always arm a capture (`drain.rs:276` `ScanlineCapture::new(Retain::LastFrame)`, `wants_scanlines` true at `oracle-core/src/scanline_capture.rs:247`), so every line is composited by `render_scanline`, then re-composited under a mask each frame. C5 (`470091d`) only changed the capture-less path (`advance_scanline`); the engine's masked framebuffer (`engine.rs:3660`) is now a per-request cost, not a per-frame double. The player half rests on `machine.rs:322-324`'s doc ("the player always arms the capture"), not a call trace. | M (or S as doc-only) |
| M32 | HOLDS (count corrected) | `crates/oracle-core/src/m68000/decode.rs:291-1383` (`decode_dispatch`) | 104 top-level `if` arms (H22's figure, not the row's 113); MOVEQ is arm 101 (`:1223`). Rides H22's memoisation decision (open high, out of scope). | M |
| M42 | HOLDS | `engine.rs:3663`, `crates/oracle-player/src/machine.rs:355-356`, `crates/oracle-frontend/src/main.rs:730-731` | All three render a full masked line to learn a width `Vdp::active_display()` (`render.rs:1109`) returns directly; `machine.rs:356`'s `if width == 0` guard is still unreachable. Same three sites as M11. | S |
| M43 | HOLDS (count exact) | `crates/oracle-core/src/vdp.rs:775`, `:1374`, `:1460` | `self.fifo[(self.fifo_write.wrapping_sub(self.fifo_len) & 3) as usize]` three times verbatim (C2's fix, `02d6282`, left it three); the same `fifo\[` search finds the write site `:645` and the snoop `:794` (control). | S (byte-neutral) |
| M62 | HOLDS (wider) | `crates/oracle-aether/src/engine.rs:9107` (`SAT_SLOTS`, the only name) | `oracle-core` has **5** bare `80`s (the row said 4): `render.rs:942`, `:1255`, `:1779`, `vdp.rs:862`, `:1766`; plus `oracle-player/src/preview.rs:336` `[false; 80]`. Not seen: a `0x50` spelling. The row's `320` trap stands. | S–M |
| M63 | HOLDS (count exact) | `crates/oracle-core/src/render.rs:1300`, `crates/oracle-player/src/planes.rs:486` | Both `(tile * 32 + row * 4 + (px >> 1)) & (VRAM_SIZE - 1)`; a cross-crate search for `* 32 + … >> 1` / `/ 2` finds exactly these two. | S |
| M64 | HOLDS | `crates/oracle-frontend/src/pick.rs:157` | `const VRAM_MASK: u32 = 0xFFFF;` beside a doc naming `VRAM_SIZE`, which is public and already imported by `oracle-player/src/planes.rs:108` (control). | S (or dies with the frontend) |
| M65 | HOLDS | `crates/oracle-core/src/synth/sn76489.rs:31` vs `crates/oracle-core/src/vgm.rs:71`; `vgm.rs:73` vs `crates/oracle-core/src/synth/ym2612_synth.rs:99` | PSG clock as `PSG_CLOCK` and `SN76489_CLOCK`; FM clock as `YM2612_CLOCK: u32` and `YM2612_CLOCK: f64`; no `const _: () = assert!` ties either pair (the house form is at `system.rs:87`). `audio_sink.rs:40` now derives `MCLK_HZ` from `PSG_CLOCK`, with FM only a prose cross-check. | S |
| M70 | HOLDS | `crates/oracle-core/src/state_hash.rs:44-48` | `compute(vram: &[u8], cram: &[u8], vsram: &[u8], regs: &[u8])`, guarded by `debug_assert_eq!` only; 5 callers. | S–M |
| M72 | HOLDS (count exact) | `crates/oracle-core/src/m68000/decode.rs:2320` (`xarith_step`), `:2673` (`cmpm_step`) | Both re-implement `ea.rs:222` `step_bytes`; each doc explains the cross-file mirror, neither the in-file twin. `if reg == 7` across `m68000/` finds exactly the three bodies. | S |
| M75 | HOLDS (latent) | `crates/oracle-core/src/render.rs:1161-1166` | `nametable_cell` reads `vram()[a \| 1]` while `vram_word` (now eight lines *above*, `:1153-1156`) uses `(a + 1) & (VRAM_SIZE - 1)`; a third `a \| 1` at `plane_hscroll` (`:1353`). Every address reaching them is even today. | S |
| M2 | HOLDS (latent) | `crates/oracle-aether/tests/schema_conformance.rs:718-722`; schema `$defs/notification.method` | Events are still only printed (`events_with_params()`); the one events pin (`tests/handshake.rs:170`) compares advertised events to `engine::EVENTS`, not to the schema; `notification.method` is still `{"type":"string"}`. Five schema events now (`emulator/clicked` added since the pin), all five with fragments — still latent. | S |
| M6 | CHANGED | `crates/oracle-aether/src/engine.rs:9120` (`MAX_WAIT_TIMEOUT_MS`) | `tests/request_bounds.rs` (`84e14f6`) now probes the schema's maxima at both ends for `len` (4096) and `memory_hash.len` (4194304), tying `max_read_len`/`max_hash_len` to the schema; `max_run_frames` has no schema maximum at all (the fragments point at `limits.maxRunFrames`), so that copy is gone. **Still a pair of untied copies:** `MAX_WAIT_TIMEOUT_MS` = 300000 vs `wait_for_break.timeoutMs` maximum — its in-bounds control is exempted in `MAX_CONTROL_SKIPS` ("REASONED, NOT MEASURED"), so lowering the constant stays green. Not verified: that `spawn_for_sweep` keeps the other limits at defaults. | S |
| M13 | HOLDS | `crates/oracle-aether/src/server.rs:419-422`, `:438` (`engine_loop`) | No containment on the engine thread; `forward` (`:818-829`) returning `None` ends only the connection; accept loop and socket survive, `main.rs:112-130` parks forever, so a restart's incumbent probe is answered. `catch_unwind` appears only in tests/examples (control: finds `tests/handshake.rs:540`, `tests/request_bounds.rs:845`). | M |
| M14 | HOLDS | `crates/oracle-aether/build.rs:138-156` | The scope doc says "`oracle-frontend` and `oracle-replay` are deliberately absent: neither links into this binary", yet oracle-frontend links oracle-aether (feature `aether`, `Cargo.toml:40`) and serves the bus (`--aether`, `main.rs:218`); the player half is caveated only in a hover (`oracle-player/src/identity.rs:231`); `initialize`'s `serverBuild` (`engine.rs:3210-3221`) carries no caveat. | S (doc); a wire caveat is contract-touching |
| M15 | HOLDS | `crates/oracle-player/src/identity.rs:110-114` | `.any(\|p\| p.contains("oracle-player"))` over absolute scope paths (`build.rs:99-103`). | S |
| M16 | HOLDS | `crates/oracle-aether/src/engine.rs:5161-5169`, `:3710` | `masked_hash_caveat()` is appended into the always-present caveat string; the schema's `state_hash` result has no typed mask key. | M (**contract-touching**) |
| M17 | HOLDS | `crates/oracle-aether/src/engine.rs:5842-5898` (`object_list`), `:6148-6222` (`object_slot`) | Neither inserts `caveat`; `load_symbols` accepts Indeterminate-but-intact tables (`:7387-7400`); the fragment's `caveat` description names exactly that binding. (The fallback-detection half is moot: `decoders.rs:322-326` makes `detectedBy` always `"symbol"`.) | S–M |
| M18 | HOLDS | `crates/oracle-aether/src/engine.rs:4465` (`read_memory`), `:4860` (`read_vram`), `:5133` (`state_hash`) | Three constant caveat literals with no condition; `tests/pixel_attribution.rs:733` still calls `read_memory`'s "the in-tree example". Control: the same literal-caveat search found `lookup_symbol`'s (`:7214`), which reads as conditional and was excluded. **`read_memory` is in the memory-read path — that site waits for the running parcel.** | S per site |
| M23 | HOLDS | schema `play_input.rows.items`; `crates/oracle-aether/src/engine.rs:10062-10120` (`parse_input_rows`), `:10173-10177` (`parse_port`) | `items` has no closed key set (the top-level `unevaluatedProperties:false` does not reach into items); `parse_port` returns `Ok(0)` for an absent `port`; `parse_port(row)?`/`parse_buttons(row)?` (`:10106-10107`) drop the row index. | S (index: code-only); closing the key set is **contract-touching** |
| M25 | HOLDS | `crates/oracle-aether/src/engine.rs:6004` | `(Some(b), None) => hex::parse_bytes("bytes", b)?` accepts `"0x"` as an empty Ok (`hex.rs:68-93`), `z80_window(addr, 0)` passes, reply `{"len":0}`; the siblings refuse (`:4499-4501`, `:4922-4924`). Neither new request gate sends `"0x"` (the pattern probe's one candidate is `"zzz+$FF"`). | S |
| M26 | **FIXED-ELSEWHERE** `ec213c5` (+ `84e14f6`) | `crates/oracle-aether/tests/request_shapes.rs`, `tests/request_bounds.rs` | Both files derive every probe from the vendored schema at run time — numeric bounds (`84e14f6`) and required/pattern/minLength/enum/minItems (`ec213c5`); each is a one-file test commit per `git show --stat`. Declared residue: six `object_*` methods UNCOVERED (`F-OBJ-BOUNDS-UNPROBED`), two `MAX_CONTROL_SKIPS`, one illegal value per pattern site (why M25 survives). Ledger line appended. | — |
| M27 | HOLDS | `crates/oracle-aether/src/engine.rs:472` | `summary: "one byte read across the bus/vram/cram/vsram spaces…"` while `emulator/read.len` takes up to 4096. | S (visible text in the palette and `methodSummaries`) |
| M28 | CHANGED | `crates/oracle-aether/src/engine.rs:7306-7311`; `crates/oracle-aether/tests/methods.rs:639`, `:686` | **Fixed half:** `3386902` (code, `engine.rs` only) refuses `prefix: ""` naming the key, and `request_shapes.rs:256`'s minLength probe sends `""`. **Still true:** the test `lookup_equate_serves_the_exact_hit_the_empty_prefix_and_the_two_refusals` still sends `{"prefix":"ZZZ"}` — an empty *result* — and its name now reads as "the empty prefix is served", the opposite of the fixed behaviour. | S (rename) |
| M29 | HOLDS | `crates/oracle-aether/src/engine.rs:8185`; schema `checkpoint_list.limit` | `parse_count("limit", v, 1, cap)` with `cap = max_checkpoints` (a capacity), while the schema declares `minimum: 1` and no maximum — so `request_bounds` cannot see it, and `limit: 100` is refused. | S (treat as a page size: code-only); declaring a maximum instead is contract-touching |
| M41 | HOLDS (count exact) | `crates/oracle-aether/src/engine.rs:7192` | `all.sort_by_key(\|s\| (s.addr, s.name.clone()));` — the only allocating key among the 8 `sort_by_key(` calls in `crates/` (the search's own hit is its control). | S |
| M71 | CHANGED | `crates/oracle-aether/src/outbound.rs:61`; `crates/oracle-aether/src/server.rs:52`, `:512` | Still `assert!(capacity > 0, …)` and an unvalidated `pub usize event_queue_cap`. **Different place:** `Outbound::new` runs inside `connection_loop` on the per-connection `"aether-conn"` thread (`server.rs:295-305`), not the accept thread — a 0 kills every connection while the socket stays bound (M13's shape), rather than the accept thread. | S |
| M73 | HOLDS (count restated) | `crates/oracle-aether/src/engine.rs:5348-5354`, `:5366-5372` | Two `RpcError::new(code::INVALID_STATE, …)` bypasses (`perFrameNotArmed`, `callersNotArmed`) against **14** production helper calls (engine.rs ×10, `objreq.rs` ×4) — 2 of 16 production sites; "18" matches only counting `rpc.rs`'s two test calls. Same shape at the pin. Control: the search finds the helper's own body (`rpc.rs:95`). | S |

## Totals

- **HOLDS (50):** M2, M5, M7, M8, M11, M12, M13, M14, M15, M16, M17, M18, M19, M20, M21, M22, M23, M24,
  M25, M27, M29, M30, M31, M32, M41, M42, M43, M50, M51, M53, M61, M62, M63, M64, M65, M66, M68, M69,
  M70, M72, M73, M74, M75, M76, M77, L1, L3, L4, L5, L8.
- **CHANGED (6):** L6, L7, M6, M28, M67, M71.
- **FIXED-ELSEWHERE (2):** M26 (`ec213c5`, with `84e14f6`), M52 (`0c39449`). Ledger lines appended.
- **REFUTED (0). TAG (0).**

Counts in rows, re-derived: exact for M41, M43, M53, M61, M63, M72; **wider** than booked for M19,
M62 (5 core sites, not 4), M66 (a second pair), L5 (both files grew); **corrected** for M32 (104 arms,
not 113), M73 (2 of 16, not 18), and M67 (14, not 10). Two of M67's 14 are not the display height.

## Proposed fix parcels

Hub files (`engine.rs` alone carries ~15 of these rows) make one flat, file-disjoint set impossible,
so the parcels come in **waves: the parcels inside one wave touch no file in common** and can run
concurrently; a later wave starts after the earlier one lands. A row that crosses hubs is split by
file, and the split is named. Ranked inside each wave by value for effort.

Kinds: **code-only**; **contract-touching** (changes what `empyrean/contract/protocol.md` or the
vendored `bus-protocol.schema.json` describes — a CR first); **measure-first** (moves emulation or
currency bytes); **look-call** (what a person sees — the owner decides the shape).

### Wave 1 — all mutually file-disjoint

| rank | parcel | rows | files | kind | size |
|---|---|---|---|---|---|
| 1 | **AETHER-TEST-GATES** | M2, M6, M28, plus M25's gate half | `crates/oracle-aether/tests/schema_conformance.rs`, `tests/request_bounds.rs`, `tests/methods.rs`, `tests/request_shapes.rs` | code-only | S |
| 2 | **AETHER-SERVER-LIFECYCLE** | M13, M71, M51 | `crates/oracle-aether/src/server.rs`, `src/outbound.rs`, `src/main.rs` | code-only | M |
| 3 | **CORE-RENDER** | M20, M63, M75, L4, M62 (render half) | `crates/oracle-core/src/render.rs`, `crates/oracle-player/src/planes.rs` | code-only (M20 changes when a client's caveat fires, to what §11.27 already says; no emulation bytes) | S–M |
| 4 | **FRONTEND-SMALL** | M19, M64, L8 | `crates/oracle-frontend/src/sram_file.rs`, `src/main.rs`, `src/pick.rs`, `crates/oracle-player/src/battery.rs`, `crates/oracle-aether/tests/schema_dryrun.rs` | code-only + **look-call** (how "the save could not be read" is shown); L8 also needs a booking row the overseer writes | S–M |
| 5 | **PLAYER-STOPPING** | M50 (`stopping.rs` half), M74 | `crates/oracle-player/src/stopping.rs`, `src/ui.rs` | code-only + **look-call** (which rule both address boxes share: hex-first or symbol-first) | S |
| 6 | **BUILD-IDENTITY** | M14 (doc half), M15 | `crates/oracle-aether/build.rs`, `crates/oracle-player/src/identity.rs` | code-only | S |
| 7 | **CORE-SYMBOLS** | M12, L3 | `crates/oracle-core/src/symbols.rs` | code-only | S |
| 8 | **DOC-TRUTH** | L6 (four stale halves) | `tools/land.sh`, `fixtures/aeon/DIMENSIONS.tsv` (comment lines only — `aeon_dimensions.rs` parses the file), `crates/oracle-core/tests/aeon_dimensions.rs`, `crates/oracle-aether/tests/contract/PROVENANCE.md` (prose line 42 only — `schema_conformance.rs:41` `include_str!`s it and parses its markers) | code-only | S |
| 9 | **TEST-FIXTURE-DEDUPE** | M61, M68 | `crates/oracle-core/src/testrom.rs`; `crates/oracle-aether/tests/{breakpoints,watch_hits_epoch,hosted,object_mutation}.rs`, `src/host.rs`; `crates/oracle-player/src/bus.rs`, `src/main.rs`; `crates/oracle-frontend/src/bus.rs` | code-only | S–M |
| 10 | **SYMBOL-NAME-CONTRACT** | M66 | `crates/oracle-aether/src/objreq.rs`, `crates/oracle-frontend/src/spawn.rs`, `src/rings.rs`, `crates/oracle-player/src/objects.rs` | code-only | S–M |
| 11 | **CORE-CLOCKS** | M65 | `crates/oracle-core/src/vgm.rs`, `src/synth/sn76489.rs`, `src/synth/ym2612_synth.rs` | code-only, byte-neutral | S |
| 12 | **M68K-STEP-BYTES** | M72 | `crates/oracle-core/src/m68000/decode.rs`, `src/m68000/ea.rs` | code-only; the SST corpus is the gate | S |
| 13 | **CORE-VDP** | M43, M62 (vdp half), the M53/M67 anchor constant; M22 as its own commit | `crates/oracle-core/src/vdp.rs` | code-only, **except M22: measure-first** (and it needs the hardware sub-question below answered) | S–M |
| 14 | **CORE-EMULATION-TIMING** | M21, M24, M5, L1 | `crates/oracle-core/src/system.rs`, `src/z80/mod.rs`, `tests/export_state_v1.rs`, `docs/export-state-v1.md` | **measure-first**; M5 moves the export golden and needs a layout version bump | M |

### Wave 2

| rank | parcel | rows | files | kind | size |
|---|---|---|---|---|---|
| 1 | **AETHER-HANDLERS** ⚑ | M25, M27, M41, M73, M17, M29 (as a page size), M23 (row-index half), L7 (`engine.rs:5978`'s `0x2000` → `Z80_RAM_SIZE`), M69 (make `BUTTONS_3` `pub`) | `crates/oracle-aether/src/engine.rs` (+ tests in `tests/methods.rs`, after wave 1's AETHER-TEST-GATES) | code-only (M27 changes a summary a person reads in the palette) | M |

⚑ **Touches `engine.rs`, the file the running `debug_read` parcel is rewriting — dispatch after that
lands.** None of these rows touches `debug_read` or the read handlers; M18's `read_memory` site is
deliberately kept out (wave 4).

**Updated 2026-09-11 by the overseer.** The memory-read parcel LANDED (`F-DEBUGREAD-BANKED`, §11.48), so every ⚑ wait on it is released. **Wave 1A landed** (M12, L3, M15, M14 doc half, L6, M2, M6, M28 fixed; M25's gate half). Two items for this wave from it: **(1)** M25's handler fix here MUST delete `KNOWN_ANSWERED`'s `emulator/z80_write` row in `crates/oracle-aether/tests/request_shapes.rs` in the same commit, or that file's anti-rot test stays red. **(2) New finding, not from the sweep:** `Engine::lookup_symbol`'s demangled branch reports `ambiguous: true` and the caveat *"N different addresses answer…"* for a same-address alias group, which is false by `symbols.rs`'s own rule (same-address aliases are not ambiguous). Latent: no frozen listing has such a group. It rides with AETHER-HANDLERS.

**Updated after wave 1B (merge `a3627ca`).** Fixed: M72, M65, M20, M63, M75, L4, M13, M71, M51. Residue this wave left, booked so it does not live only in the agent's report: **(1)** M62 stays open — `vdp.rs`'s two bare `80`s go with CORE-VDP, `engine.rs`'s `SAT_SLOTS` can now import `render::SAT_SLOTS` (AETHER-HANDLERS), and `oracle-player/src/preview.rs` has a `[false; 80]`. **(2)** `render.rs`'s new private `FRAME_END_LINE = 224` is a THIRD spelling of the frame-end line beside `system.rs`'s literal and `vdp.rs`'s `VBLANK_START_LINE`; fold it into CORE-VDP's display-height constant (M53/M67). **(3)** M71's residue: `HostConfig::event_queue_cap` in `host.rs` is unchecked until `Host::serve` binds. **(4)** stale paraphrase: `synth/audio_sink.rs`'s `MCLK_HZ` doc still calls the FM tie a prose cross-check (it is now a compile-time assert). **(5)** M13's binary exit path (`main` exits non-zero on a dead emulator thread) is untested end to end; the library side is.

**Updated after wave 1C (merge `19702f3`).** Fixed: M66, M43, and M68 (refuted as drift by git history and documented in both fixtures). Left open, residue named: **M61** — one copy, `LOAD_PC` in `crates/oracle-core/tests/watchpoints.rs` (fenced in 1C); **M62** — `engine.rs`'s `SAT_SLOTS` (in the wave-2 brief) and `oracle-player/src/preview.rs`'s `[false; 80]`; **M53/M67** — the owner is now `vdp::ACTIVE_LINES`; wave 3's consumer half is `system.rs`'s `line < 224`/`line == 224`, the four production heights (`engine.rs` `ACTIVE_LINES`, frontend `main.rs` `HEIGHT`, player `machine.rs` `HEIGHT`, panels-spike `HEIGHT`), the test/example copies and `testrom.rs`'s `PROF_VBLANK_LINE` — and NOT `SCREEN_HEIGHT` in `object_mutation.rs:97` / frontend `bus.rs:972` (aeon's player-bound inset). **New findings, unbooked until now:** `spawn::Bounds` and `objreq::ActExtent` are one type written twice; no oracle-core test catches a wrong `active_display()` height; the two mailbox test fixtures copy aeon's `Obj_Req_*` layout independently with nothing comparing them. **Method lesson:** the FIFO conformance ROMs and goldens stayed GREEN under a wrong-slot FIFO helper — a gate named for a subsystem is not a gate on each of its functions.

**Updated after wave 2 (merge `b1e12b5`).** Fixed: M25, M27, M29, M41, M73, and the booked `lookup_symbol` same-address alias finding. Engine halves fixed, rows open for their other crates: **L7** (the frontend's two `MAX_SYMBOL_DISPLACEMENT` copies), **M62** (`oracle-player/src/preview.rs`'s `[false; 80]`), **M69** (`ui.rs` importing the now-`pub` `BUTTONS_3`: wave 3's BUTTON-TABLE), **M23** (closing `play_input.rows.items`' key set, a contract change). **M17 REOPENED for one row:** `object_list` and `player_state` carry the conditional caveat; `object_slot` withholds it as a KNOWN GAP (`tests/object_decoders.rs` goes red the day it emits), because `oracle-player/src/objects.rs`'s `the_row_expansion_shows_what_emulator_object_slot_shows` compares that reply key for key, and **what the object panel shows is a look call, parked with DATA-DISPLAY-AUDIT, not carded** (the hub's reading, 2026-09-12). Unbooked observations from the agent: `z80_window()`'s `0x3FFF`/`0x4000` have no named constant; the M73 guard cannot see a code passed through an alias or a variable. **Also landed between wave 1C and wave 2, not a triage row:** CI had been red since wave 1B on M13's socket-ownership check (ext4 inode reuse; local tmpfs cannot see it), fixed at `d0844ce`; its residual **HOST-SHUTDOWN-UNLINK** (`host.rs`'s `Host::shutdown` unlinks unconditionally after its listener closes) and a stale `main.rs:120` comment are booked.

### Wave 3 — file-disjoint from each other, after wave 2

| rank | parcel | rows | files | kind | size |
|---|---|---|---|---|---|
| 1 | **ONE-MASKED-FRAME, ONE-HEIGHT** ⚑ | M11, M42, M31, and the consumer half of M53/M67 | `crates/oracle-aether/src/engine.rs` (`framebuffer`, `ACTIVE_LINES`), `crates/oracle-player/src/machine.rs`, `src/bus.rs`, `crates/oracle-frontend/src/main.rs`, `src/drain.rs`, `crates/oracle-core/src/system.rs` (`:1334`/`:1344` literals) | code-only; picture-neutral by construction, so a picture-parity check is the gate | M |
| 2 | **BUTTON-TABLE** | M69 (`ui.rs` half: import the now-`pub` `BUTTONS_3`) | `crates/oracle-player/src/ui.rs` | code-only | S |

### Wave 4 and later

| parcel | rows | files | kind | size |
|---|---|---|---|---|
| **CORE-TYPES** ⚑ | M70, M76 | `crates/oracle-core/src/state_hash.rs`, `src/io.rs`, `src/system.rs`, `src/vdp.rs` (test at `:3986`), `crates/oracle-aether/src/engine.rs` (`:2174`), `src/host.rs` | code-only; public-API change in `oracle-core` | M |
| **STATE-HASH-TYPED-MASK** ⚑ | M16, M18 (`state_hash` + `read_vram` sites; the `read_memory` site only after the running parcel) | `crates/oracle-aether/src/engine.rs`, the vendored schema on re-vendor | **contract-touching** — M16 needs a typed key, so a CR comes first; M18's three sites are code-only but ride with it because M16's append depends on `state_hash`'s caveat | M |
| **MEMORY-PAYLOAD** ⚑ | M77, plus the `memory.rs` half of M50 if its look-call picks symbol-first | `crates/oracle-player/src/memory.rs` | code-only; refusal text is visible | S |

⚑ = touches `crates/oracle-aether/src/engine.rs` or `crates/oracle-player/src/memory.rs`; each waits
for the running parcel.

**Wave 4 is not file-disjoint, and is stated rather than hidden:** CORE-TYPES and
STATE-HASH-TYPED-MASK both touch `engine.rs`, so they run one after the other (STATE-HASH waits on
its CR anyway). MEMORY-PAYLOAD shares no file with either.

Optional CRs these rows raise, none required for the code-only halves above: closing
`play_input.rows.items`' key set (M23); a caveat for `serverBuild.dirty` on the wire (M14);
constraining `$defs/notification.method` (M2). For M29 the recommended fix is code-only (a page
size); declaring a maximum instead would be a CR.

### Not parcelled — owner or deferred

- **M7** (is `oracle-aether` a model layer?) and **L5** (split the two god-modules): architecture calls.
- **M8** (delete `oracle-panels-spike`, keep `run.sh` beside its doc): owner call. Deleting it removes
  one of M53's four production heights.
- **M32**: rides H22's memoisation decision (open high, out of scope here).
- **M30**: a performance change touching every `vram()` caller; needs a real profile before it is a parcel.
- **L7's other halves:** two of the three `MAX_SYMBOL_DISPLACEMENT` copies live in the frontend and go
  with its retirement; the two `WORK_RAM_LO` copies now agree and are guarded (`65124d1`).

## Open sub-questions a fix will need (not TAGs — the verdicts do not depend on them)

- **M22:** does copy DMA's VRAM write (and its read) land at `addr ^ 1` as fill's does? Settle from
  BlastEm's `vdp.c` copy path, or a copy-DMA test ROM run by the parcel that owns it — not from memory.
- **M13 / M71:** judged from the code, not a live server. The parcel should reproduce each with a
  test that kills the engine thread (M13) or sets `event_queue_cap = 0` (M71) and asserts the socket's
  observable state, e.g. `cargo test -p oracle-aether --test <new_file>`.
- **M6:** whether `spawn_for_sweep` keeps `max_read_len`/`max_hash_len` at their defaults; read
  `crates/oracle-aether/tests/common/` before relying on the two maxima being tied.
