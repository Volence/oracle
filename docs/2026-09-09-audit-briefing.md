# Audit briefing for the oracle lane — 2026-09-09 evening

**From:** the owner's audit session in aeon, at the owner's request: *"Fix the cards. Answer any you
can. Send info on any cleanups or things the current running agents should do from your audit."*
Every claim cites where it was read. No cargo was run here; the code findings come from a read-only
reviewer sampling 14 code commits since 2026-09-06.

## 1. Owner cards — what changed in `docs/decisions.jsonl` and what the lane owes

Six of eight blocking rows failed the Dominion contract (`options[]` without `key`/`name`,
`recommend` as a string). All are normalised; six are answered with the lane's own recommendation,
recorded `by: "hub"` with the owner's delegation quoted. **Drop these six from `blockedOnOwner` in
the next board write** (the main checkout's `docs/lane-status.json` is dirty and was not touched).

| card | chose | note |
|---|---|---|
| d-40 | `leave-frozen` | refresh when aeon's build manifest exists |
| d-44 | `split` | after the remaining critical fixes; measure before/after |
| d-47 | `structural` | investigate first; fall back to rewording; no new machinery |
| d-35 | `temporary-ring` | already shipped at `c91208e`; the card was stale |
| d-38 | `leave` | the measurement stands |
| d-31 | `leave-it` | lane-log 13:56: the collapse arrow is its own undo |
| d-39 | **open** | look-and-feel; `recommend.key` set to `a-shared-card` only because A is what the tab shows today (the lane declined to recommend, and the contract requires a key) |
| d-33 | **open** | owner's muscle memory; already well-formed |

## 2. Cleanups and risks, in priority order

1. **Copy-pasted tails reintroduce the drift class H25 fixed the same day.** `a5e18cb`,
   `engine.rs:2396-2403` (`note_reset`) is a verbatim copy of `reset` (`:7406-7414`), and
   `note_rom_reloaded` (`:2435-2475`) copies `reload_rom`'s tail (`:7830-7853`). Refactor the handlers
   onto the new helpers.
2. **`fnv1a_rgb` exists in three test files** (`color_1536_gradient_guard.rs:113`,
   `scanline_goldens.rs:231`, `conformance_roms.rs:291`) plus inlined constants in `render_perf.rs`,
   while `state_hash::fnv1a_bytes` exists. Dedupe.
3. **`render.rs` `vsram_word`** (`63fb29c`) indexes behind a `debug_assert_eq!` on `len()`; a release
   build with a short Vec panics on index. Low risk (only `Vdp::new` builds it); make the guard live
   in release or make the type carry the length.
4. **M48 guard is only meaningful under `--release`** (`a003801`) and `ci.yml:174` never runs release;
   `land.sh` G7 does, manually. The same landing skipped clippy and broke main (`74c74cf`). Either run
   the release gate in CI or state that the landing script is the only runner.
5. **Source-grepping gates are brittle** (`a5e18cb` `every_machine_replacing_door…`, `bc59bf3`,
   `900e520`): a door spelled `sys = System::new(..)` escapes the hard-coded `MUTATIONS` strings.
6. **`3a468c5` landed "WIP, UNVERIFIED" on `main`**; `ff37443` found a vacuous assertion inside it 20
   minutes later. Honest label, wrong branch.
7. **`docs/lane-status.json` is dirty and uncommitted** in the main checkout.

## 3. Quality verdict for the record

Sampled grades: correctness A-, Rust B, tests A-, cleanliness B+, message honesty A. Every core change
cites a reference or derives from source constants and the derivations check (H8's 59.9227 Hz; C2's
FIFO anchor). Red-first is quoted with numbers. Completeness is handled structurally (`land.sh`
G6/G8). Decisions: five of six sampled ran the discriminating experiment; `74c74cf` accepted a
"toolchain skew" story first and retracted it in 15 minutes with a positive control. 49% of commits
since 09-06 touch code.
