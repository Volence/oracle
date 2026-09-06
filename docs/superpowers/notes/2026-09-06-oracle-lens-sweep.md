# Oracle full-repo lens sweep — PACKET (in progress)

Charter, corpus, roster deviation and seat rules:
`2026-09-06-oracle-lens-sweep-CHARTER.md`. Pin `d3ca871` / review revision `e6bc191` (identical code).

**STATUS: IN PROGRESS.** 22 seats launched, **15 returned**. Still out: P2, A2, Va, Vb, B2a, B2b, B1. This file is
written incrementally so no finding lives only in a session's context. **Nothing here is fixed** — the
sweep is read-only by design; landing a packet makes findings discoverable, it does not close them.

**Controller verification status is recorded per finding.** ✔ = re-derived firsthand by the overseer;
◻ = seat's derivation accepted as written, not yet re-run.

---

## CRITICAL / HIGH — reachable today

### H1 ✔ Three guard tests and the guards they test both vanish under `--release` (seat TEST)

`crates/oracle-core/src/testrom.rs` — `short_disp` (`:100`) and `disp16` (`:76`) are **`debug_assert!`**, and
their three `#[should_panic]` rows (`:1482`, `:1493`, `:1513`) carry `#[cfg(debug_assertions)]`.

**Re-measured by the controller, both profiles:** `cargo test -p oracle-core --lib testrom:: -- --list`
→ **11 tests** debug, **8 tests** release; the three `rejects` rows are present in one and absent in the
other. They do not report as `ignored` — **the count drops silently by three.**

Why it matters: release test runs are a wired path here (`crates/oracle-replay/tests/replay_real_artifacts.rs:41-52`
routes playthroughs through `cargo test --release` precisely so they cannot be skipped). In that run the
displacement guards are off in the fixture builder **and** their tests are gone. The failure mode is the
one `testrom.rs:88-92` calls invisible at every layer: an out-of-range `as i8` does not fail to assemble,
it **silently emits a different valid branch**, so the ROM boots and measures the wrong thing.

Evidence this is oversight not policy: `oracle-aether` treats the same profile split as load-bearing and
pins it (`tests/server_build.rs:355-366`). `testrom.rs` has the shape with none of the machinery.

### H2 ◻ A reset can strand or ROLL BACK a battery save — LIVE, and wider than its booking (seat STATE)

Booked as `F-HOSTED-RESET-SRM` (`docs/OVERSEER.md:98`) and **confirmed live with a sharper mechanism**;
critically it is **not hosted-only**, so the booked mitigation ("warn clients off hosted reset") does not
cover it. The window's own F1/palette reset reaches it too.

Controller re-derived the three load-bearing steps ✔:
- `System::reset_with_sink` (`system.rs:481-503`) preserves SRAM bytes but **clears `sram_dirty`** — its
  own comment says so (*"`sram_dirty` also clears (it is only a persistence throttle)"*).
- `Battery::after_replacement` (`battery.rs:188-201`) writes **only if `c.bytes != sys.sram()`**. A soft
  reset preserves the bytes → equal → **no write**, and `countdown` is cleared.
- The contrast proving intent: `oracle-frontend/src/main.rs:1946-1952` flushes **before** reset, with a
  comment naming this exact hazard. **The player has no equivalent on this path.**

**Worse variant:** on the iteration answering a window gesture, `drain` skips the carry (`bus.rs:1064`), so
`after_replacement` can write the **previous** image over the `.srm` — disk rolled back *and* newer bytes
lost. Loss window ≈ one iteration (~16 ms).

**The existing guard cannot fail on this**: `bus::one_door::a_reset_does_not_rewind_the_live_battery_to_the_file`
(`bus.rs:4049`) asserts on the **machine**, never the file, and its helper never calls `Battery::tick`.
Green and blind.

### H3 ◻ The request side of the wire has a machine-readable contract and zero differential (seat FUZZ)

70 `params` fragments carry **57 integer bounds** plus patterns and enums; `jsonschema` is a
**dev-dependency only** (`oracle-aether/Cargo.toml:25`) and `grep 'schema' crates/oracle-aether/src/*.rs`
is empty. Every bound is re-implemented by hand in 62 handlers, and **nothing asserts the two agree**:
`params_closure.rs` compares key *names* only; `schema_conformance.rs` validates *replies*.

**21 of 62 methods have no negative-value test at any severity** (list in the seat report), verified
against a control pass for negative assertions written in other styles.

Blast radius: one `Engine` owns one `System` on one thread serving every connection
(`server.rs:409-434`), and **`catch_unwind` appears nowhere in production**. A handler panic takes the
loaded ROM, every checkpoint and every client with it.

### H4 ◻ Two of four machine-replacing doors in `oracle-frontend` tell the bus nothing (seat STATE)

`bus.machine_replaced` appears **exactly once** in `oracle-frontend/src/main.rs` (`:1927`, the F4 state
load). The human's **reset** (`:1954`) and the **F5 / ROM-browser swap** (`:2124-2160`) never call it, and
`set_rom_path` does not bump `rom_generation`. A client that paused the frontend then sees a latched
framebuffer of the outgoing cartridge, hits stamped in a dead epoch, a profiler stack describing returns
that will never come — and **no event at all**, while `emulator/status` reports the *new* `romPath`.

⚑ The charter's lead asked whether a door runs **no** repair. For the **audio** repair the lead is
**REFUTED** — every door runs it, structurally. For the **bus-notification** repair it is **confirmed**.

### H5 ◻ `schema_conformance.rs`'s "loud refusal" is silent — libtest eats it (seat GATE)

`:509-522` prints its SKIPPED banner with `eprintln!`, swallowed under default capture (seat measured both
directions plus an independent control). **This repo already found and fixed exactly this** at `c301f89`,
adding an fd-2 helper to four files — and `mcp_tool_sweep.rs:63-65` cites `schema_conformance.rs` as the
exemplar it is following. **The sweep missed the file it named as the model.**

Worse here than elsewhere: no runner sets `$AETHER_CONTRACT_SCHEMA`/`$AETHER_CONTRACT_REPO`, so step 3 is
the branch that **always** runs, on every landing, invisibly. Nothing has ever confirmed the pinned
revision exists upstream.

### H6 ◻ The symbol-file acceptance policy is written out FOUR times; three survive retirement (seat ARCH)

Four independent `match RomBinding` blocks implementing one four-arm table including the fail-open guard:
`oracle-aether/src/engine.rs:7100-7145`, `oracle-replay/src/policy.rs:53-75`,
`oracle-player/src/symbols.rs:136-173`, `oracle-frontend/src/symbol_file.rs:72-110`.

They agree today. **No test could assert it**: `oracle-replay` depends on `oracle-core` alone, so no crate
sees the replay copy and any other at once. `policy.rs:15-18` says extracting it *"is the immediately-
following slice, not a follow-up ticket"* — the slice did not happen and a **fourth** copy landed instead.
Its cross-crate citation has drifted ~6,100 lines.

---

## MEDIUM

- **M1 ◻ `land.sh` is the third place the suite runs and does not name the aeon pin** (GATE). `aeon_pin.rs:33-40`
  states the rule in terms: *"If you add a third place the suite runs, name the pin there too, or that
  run's green says nothing about which build it passed against."* `grep nocapture\|aeon_pin tools/land.sh`
  → no matches. Cost to fix: one line, ~0.06 s.
- **M2 ◻ Event fragments are printed, never pinned** (GATE). The methods axis is pinned both ways
  (`UNCOVERED_METHODS` empty + enumerated `SCHEMATIZED_NOT_ADVERTISED`); the events axis is only reported,
  and `$defs/notification.method` is an unconstrained string. A new event kind would be checked by the
  envelope alone, silently. Not exploitable today (all four events have fragments).
- **M3 ◻ The contract-vector runner has no floor** (GATE). Its anti-vacuity is `passed > 0 && refused > 0`
  plus one per-method count. Measured: 295 cases / 176 fail / 119 pass. **A re-vendor dropping 175 of the
  176 fail-vectors would still pass**, and the blob pin moves with every re-vendor by design. The numbers
  to assert already exist, derived, in `PROVENANCE.md:195`.
- **M4 ◻ `replay_playthroughs.sh` discards the pin gate's verdict** (GATE). `set -uo pipefail` (no `-e`),
  and the `cargo test … | sed` pipeline's status is never read, so a **red** `aeon_pin` prints nothing and
  the script proceeds. The one CI job whose stated purpose is "this green is about THESE bytes" cannot
  fail on a broken pin.
- **M5 ◻ `export_state_hash` is blind to a live chip** (STATE). `system.rs:791-792` still writes FM/PSG
  zero placeholders commented *"they fill when those chips land"* — **the FM chip has landed** and holds
  behavioural state (latch, timers) the guest reads back. The determinism gate's sole currency will call
  two machines identical when they differ in FM, bus arbitration or I/O. Contrast, and the right pattern:
  the wire's `state_hash` carries an unconditional caveat naming exactly what it omits.
- **M6 ◻ `EngineConfig` limits and the schema's constants are two copies of one number** (FUZZ). `max_read_len`,
  `max_hash_len`, `max_run_frames`, `MAX_WAIT_TIMEOUT_MS` are `pub` fields; the schema hardcodes the matching
  maxima. They agree at the defaults and nothing asserts it.
- **M7 ◻ `oracle-aether` has become the player's model layer, not only its control surface** (ARCH).
  `ScreenSurfaceKind` (five names of one window's chrome as a closed contract enum), `target_fps` documented
  by a consumer's **CLI flag**, and `MAX_SCREEN_SURFACES` justified by a constant defined **upward** in
  `oracle-frontend`. Each is argued in its own doc comment — the question is whether `oracle-aether` is
  still a control surface. **Negative controls clean:** no toolkit symbols in `oracle-core`/`oracle-aether`.
- **M8 ◻ `oracle-panels-spike` is dead, with evidence** (ARCH). Excluded from the workspace, zero tests,
  zero code dependents; already rotted two ways (a false "no lib target" justification; `stats.rs` is a
  **diverged fork** that still flattens an empty distribution to `0.0`, which §11.42 M3 forbids). **Caveat:
  its `run.sh` is load-bearing to the migration docs and the retirement gate's "same form as the spike
  doc".** Recommendation: delete the crate, keep `run.sh` beside the doc. Owner call.
- **M9 ◻ Eight zero-assertion decode tests with names promising classification they never check** (TEST).
  `m68000/decode.rs` ×8, each ending `let _ = decode(&regs);`. Subsumed by a totality sweep over all 65536
  opcodes that runs in 0.02 s. `adda_decode_classifies_and_sizes` checks neither classification nor size.
- **M10 ◻ The safety-valve test is fed the constant the valve checks** (TEST). `oracle-frontend/src/audio.rs:640`:
  loop bound *and* boundary case are both `MAX_CONSECUTIVE_SKIPS`, so it is green for **every** value.
  Nothing pins the number 4. Same shape mirrored at `oracle-player/src/pacing.rs:1133` — and `pacing.rs:1090`
  does it **right** one test earlier, so the house instrument exists and was not applied.
- **M11 ◻ `F-THREE-MASKED-RENDERERS` advanced** (ARCH): the single owner **already exists** and none of the
  three uses it (`Vdp::active_display()`, `render.rs:1098`); `machine.rs:287-289` **names the hazard and
  then walks into it** twelve lines later; the trigger is scheduled in-tree (*"When V30 lands, it lands
  here"*). Post-retirement the count is **two, not zero**. A comparison test is writable today.
- **M12 ◻ `Resolution::name()`'s documented round-trip is false for same-address aliases** (FUZZ).
  `address_of` requires uniqueness; `build()` deliberately leaves same-address aliases non-ambiguous.
  **Measured over all four real listings (8,209 symbols): 0 occurrences** → latent, not live. The guarding
  test checks exactly two hand-picked addresses.

## LOW / structural

- **L1 ◻ `System::restore` performs no post-decode shape validation** (FUZZ) — a short `ram` decodes cleanly
  then panics on first read. Not reachable from the wire; only from a hand-forged local file.
- **L2 ◻ `debug_read`'s `len == 0` underflows** (FUZZ) — a `pub`, cross-crate, total-looking API whose domain
  restriction lives only in five callers. All five currently bound it; the class is the finding.
- **L3 ◻ `parse_body_line` lacks the hex-digit check its twin documents and performs** (FUZZ) —
  `symbols.rs:1470` vs `:1494`; `+FF` parses as `$FF` in the fallback dialect.
- **L4 ◻ `CramLanding.addr`'s evenness/range is documented and consumed unmasked** (FUZZ) — safe today by
  a single production caller; two test-only callers pass through unchecked.
- **L5 ◻ Two god-modules by CODE lines** (ARCH): `engine.rs` 10,108 code / `ui.rs` 5,922 code. The headline
  `decode.rs` is 67% tests and is fine. `engine.rs:9432-10006` is a cross-crate library surface (six
  functions imported by the player) filed under "the engine".
- **L6 ◻ Stale derivations that no longer redo** (GATE, ARCH): `land.sh:142`'s own grep now returns 10 hits;
  `DIMENSIONS.tsv:20,23` line cites off by ~100 and ~250 lines (substance verified sound);
  `aeon_dimensions.rs:6` says "three sightings" where its manifest lists four; `PROVENANCE.md:42` says
  276 cases where the file has 295; `policy.rs`'s citation drifted ~6,100 lines.
- **L7 ◻ `MAX_SYMBOL_DISPLACEMENT` has three definitions**, two of which die with the frontend (ARCH).
  `engine.rs:5734` uses a literal `0x2000` where `Z80_RAM_SIZE` is imported and used correctly two sites
  away. `WORK_RAM_LO` is a **name collision across crates with different values**, both correct in place.
- **L8 ◻ Three `#[ignore]`d tests, none with a booking row** (TEST). Two are the schema dry-run pair whose
  premise has moved **23 landings** underneath them (38 fragments at authorship, 71 now); the third needs a
  ROM the repo deliberately does not carry. They read as "pending work"; they are "an answered question
  with no owner".

---

## Refuted / downgraded — recorded so they are not re-found

- **The audio-repair "door that runs none" lead: REFUTED** (STATE). Every machine-replacing door runs it,
  structurally, in both windows.
- **`land.sh` G1 is NOT the only thing between a fresh worktree and a false green** (GATE) — refuted, and
  the real backstop is stronger: `export CI=1` arms six in-suite refusals that check **named manifests**,
  not path presence. **Residual, and it is why `land.sh` exists:** a human typing
  `cargo test --workspace --release` by hand gets `CI` unset, every SST row skips, and the skip messages
  are `eprintln!` — invisible.
- **`DIMENSIONS.tsv` is DERIVED, not transcribed** (GATE) — re-measured through `SymbolTable` every run
  against independently-pinned bytes, with a positive control that panics on an uncovered probe kind.
- **The SAT-stride duplication is a CLOSED row**, not an open finding (ARCH) — correctly not re-reported.
- **`palette.rs`'s `METHODS` lead: confirmed but not reachable in the shipped player** (ARCH), because
  `bus.rs:420` sets `presents_frames = true`. One degree worse than booked: the refusal text quotes a
  **count** that would be wrong, in the sentence whose job is to be authoritative.
- **The 68000/Z80 cores are corpus-driven, not example-driven** (FUZZ) — ~1M + 708k vendored cases with
  anti-vacuity guards. A property test buys nothing there; **do not file one**.

## Notable verified-clean (re-derived, reported as results)

`save_state::decode` (magic, version, layout fingerprint, ROM fingerprint, exact length, checksum, plus
negative tests) is the strongest decoder in the repo · checkpoint capture/restore decodes fully before
applying, so a half-restore is not expressible · every `System` field is snapshot-encoded except one
documented exception · `oracle-core` has zero dead public functions and exactly two test-only ones ·
RPC framing capped with drain-and-discard · VRAM indexing masked at every site · both hand-rolled hashers
are proved against published vectors **including the empty-input case** · `MCLK_PER_FRAME` carries a
compile-time cross-module agreement assertion — **the fix shape M11/H6 lack, already in the tree**.

## TAGGED for the controller's foreground (no seat may do these)

1. Confirm H2 with a live run: reset with a same-iteration guest SRAM write, then read the `.srm`.
2. Confirm H4: attach a client, pause the frontend, press F5, then `emulator/screenshot` + `watchpoint_hits`.
3. `AETHER_CONTRACT_REPO=… cargo test -p oracle-aether --test schema_conformance the_pin_is_confirmed -- --nocapture`
   — the check H5 says nobody runs.
4. Whether CI's `build-test-lint` job is currently green (it fetches only the 68000 corpus but `CI=true`
   arms guards needing three). **Either that job is permanently red — the shape where a permanently-red
   check camouflages a vacuous one — or something supplies those dirs.** The GitHub MCP server failed to
   connect this session, so no seat could read it.
5. Whether `oracle-panels-spike` still compiles (~250 crates; deliberately not run on a shared machine).

---

# Second tranche — seats SAFE, CACHE, P1a, VDP, CPU-A, ERR, TIMING, CPU-B

**13 of 22 seats returned.** Still out: PROTO, P1b, P2, A2, Va, Vb, B2a, B2b, B1.

## CRITICAL — new

### C1 ✔ CI HAS BEEN RED FOR 46 DAYS, so `cargo test --workspace`, clippy and fmt are NOT gates (seat CACHE)

**Independently re-measured by the controller** via `gh api` over all 554 runs: **529 failure, 23 success,
2 cancelled. Last success `2026-07-22T22:53:02Z`.** Zero successes since, on any branch.

Two stacked, unrelated causes, **neither booked anywhere**:
- **Original (07-23→):** `ci.yml:44` runs only `fetch-tests.sh` (68000 corpus). The z80 and TestRoms
  fetches are **never run in CI**, so `vendor_data_present_when_running_in_ci` fires — **correctly**.
- **Current (~08-13→):** the Clippy step dies on `libudev-sys` (via `gilrs`, landed `a68e23a`), *before*
  fetch and *before* test. So `Fetch`, `Name the frozen aeon pin` and `Test` all show skipped.

⚑ **The consequence is the sharpest thing in this sweep.** `cargo test --workspace` has not executed in
CI since 2026-07-22, and with it **all five `vendor_data_present_when_running_in_ci` controls — written
precisely so a missing corpus could not pass vacuously — are now controls that cannot run.** This lane's
own booked lesson (*"a permanently-red check camouflages a vacuous one"*) turned on itself: the red guard
camouflages the vacuity it exists to detect, and a second breakage buried the first for three weeks.

**Ordering trap for whoever fixes it:** fixing libudev alone returns CI to the 07-23 red, not to green.
And `fetch-testroms.sh:63` pulls from Google Drive, which is likely why it was never wired.

*(`land.sh` and the determinism/replay jobs are unaffected and green — local landings were never blind.)*

### C2 ◻ A second VDP data-port write inside one instruction re-charges the first write's stall (seat TIMING)

`vdp.rs:1337-1347`. `now_mclk` is set **once per instruction** (`bus.rs:926`) and never advances within
it, while `fifo_slot_clock` is absolute and keeps moving — so the *k*-th stalling write re-charges every
earlier write's wait. Seat built a replica of the arithmetic and **validated it against the crate's own
documented figures** before using it. Measured: six stalling writes charge **967 cycles against a true
281** — **+40 % on the stall half**. `move.l dN,(a6)` with `a6 = $C00000` is two data-port writes and is
*the* canonical Genesis VRAM idiom, so this is reachable, not exotic.

`data_read_at` does the same job **correctly** one function away (assigns rather than accumulates).
Every existing test stops at the **fifth** write, so the bug is green by construction.

### C3 ◻ `$A11200` is a hold, not a reset — the Z80 keeps PC, HALT, IFF and IM across a reset pulse (seat CPU-B)

`bus.rs:1101` is the *entire* effect of a Z80 `/RESET` write: it flips a boolean that stops the clock.
`Z80` exposes **no reset method at all**; the core is constructed once and never re-initialised. Real
`/RESET` forces `PC=0, I=0, R=0, IFF1=IFF2=0, IM 0` and un-halts.

**Measured out-of-tree with two non-trivial controls** (a positive control proving the probe observes Z80
execution, and a capability control proving the counter *can* reach the expected value): after a reset
pulse the counter reads **1 where hardware gives 2**.

⚑ **This repo already wrote the rule down** (`docs/2026-07-22-sound-stack-recon.md:224`) and closed with
*"pin it; do not let it drift."* It was never implemented — a rule that lives in prose and not in code,
which is this seat's exact remit. Worst symptom is silent and permanent: a Z80 halted with `IFF1=0` when
reset is asserted **stays halted forever**, so a reloaded sound driver never starts.

Why nothing caught it: every Z80-live test sets the private field directly; **not one drives
reset-release through the bus**, and none re-asserts.

### C4 ◻ 560 Z80 encodings `unimplemented!()`, uncontained, and a commercial ROM has already reached the class (seats SAFE + CPU-B, independent convergence)

`z80/mod.rs:1309` (ED mirrors), `:1795` (IXH/IXL), `:1866` (DDCB/FDCB). CPU-B **measured the exact set by
executing every prefixed encoding under `catch_unwind`**: ED 20, DD 46, FD 46, DDCB 224, FDCB 224 = **560**,
with both a positive and a detector control. These are *real instructions*, not illegal opcodes, and the
IXH/IXL set is common in hand-optimised sound drivers.

**Containment: none.** `catch_unwind` appears twice in the whole repo, both in a test and an example; no
`panic = "abort"`. One byte in a guest's Z80 program kills the thread — in the player the window dies
mid-session; in the server the engine thread dies while the socket stays bound (see M13).

Precedent is in-tree and demonstrated: Vectorman's `FD FF` boot panic pinned the emulator at 26 frames
until the prefix rule landed. Two of five panic classes were closed then; **the ledger prose reads as if
the class were finished. Three classes are not.**

## HIGH — new

- **H7 ◻ An address-error abort reports `executed: true`** (CPU-A). `install_address_error`
  (`microop.rs:1470-1499`) never sets `suppresses_trace`, though **three doc sites beside it say it does**.
  Consequence: a faulting `JSR` is classified as a Call → a phantom frame nothing pushed → **`step_over`
  waits for a return that cannot come**. The guard that looks like it covers this exercises only the
  decode-time path — *it validated the search, not the question*.
- **H8 ◻ The machine runs 0.129 % fast: "60 Hz" is the label, 59.9227 Hz is the value** (TIMING). The mclk
  frequency is stated **nowhere in `crates/`**, though it is derivable from the tree's own chip constants.
  `audio_sink.rs:100` `samples_per_frame = sample_rate / 60` **means samples per 1/60 s** and is *named*
  per frame. Closed loop with the audio-master pacing ⇒ **278 extra emulated frames per hour**, growing
  linearly. Player-only; headless is unaffected.
- **H9 ◻ Two copies of the completed-frame reader; only one is tested, and the untested one answers the
  wire** (VDP). `main.rs:676 blit_capture` vs `engine.rs:9967 store_from_capture` — the latter's own doc
  says they are the same algorithm. The frontend copy has two dedicated unit tests; the aether copy has
  none and is what `screenshot`, `scanlines` and `state_hash{includeFramebuffer}` read. **This subsystem
  enforces one-derivation-two-consumers by name everywhere else** (four cited instances).
- **H10 ◻ `rings.rs`'s bounds guard checks 4 of 9 listing offsets, and the check it does perform overflows
  in release** (SAFE). Every number comes from a `.lst` equate, which `symbols.rs:1324` explicitly does
  **not** range-check. `off + width > entry_size` in `u64` with `overflow-checks = false` ⇒ a `-1`-spelled
  equate passes the guard and slices out of range. **The module's stated contract is defeated by the
  arithmetic in the sentence that states it.** The newer player-side consumer of the same equate *does*
  narrow and zero-check (`objects.rs:645-649`) — one namespace, two consumers, one careful.
- **H11 ◻ The "never measured" sentinel is printed as the worst real reading** (ERR). `report.rs:262-269`
  maps `u64::MAX` ("no steady-state callback yet") onto **`0 samples (0.0 ms)`** — "the ring hit rock
  bottom", the most alarming value the stat can produce. Every neighbouring figure guards loudly
  (`FRAME PERIOD NOT MEASURED`). **The reference doctrine inverted in the crate that states it, two files
  apart.** Any run under ~1.5 s ends here.

## MEDIUM — new (abbreviated; full derivations in the seat reports)

- **M13 ◻** A panicked engine thread leaves a bound socket and a server answering nothing — indistinguishable
  from a hang, and the socket file keeps a restart's incumbent check happy (SAFE).
- **M14 ◻** `serverBuild.dirty`'s scope excludes **both presenting crates**; the player half is documented
  and caveated, **the frontend half is documented nowhere**, and neither is caveated on the wire (CACHE).
- **M15 ◻** `dirty_scope_covers_this_window()` is a **substring match on an absolute path** — build from a
  worktree whose path contains `oracle-player` and the caveat silently retires early (CACHE).
- **M16 ◻** `state_hash`'s mask-divergence warning has **no typed key**; its only carrier is a caveat every
  reply already contains, and the contract forbids parsing caveats (ERR).
- **M17 ◻** `object_list`/`object_slot` never emit the caveat their fragment declares for an
  **indeterminate** symbol binding — a reachable, live condition (ERR).
- **M18 ◻** Three methods emit an **unconditional** caveat, which §11.27 makes a MUST NOT; the tree names
  one of them as *the* in-tree example of the forbidden shape (ERR).
- **M19 ◻** "No save yet" and "the save could not be read" are collapsed at `sram_file.rs:38`, and the F5
  path has **no else arm at all** (ERR).
- **M20 ◻** `cram_divergence_caveat`'s `last_completed` is off by one frame from lines 224-261, so the
  caveat fires on a whole frame of writes exactly in the mid-frame case it exists for. The test helper is a
  **transcription of the implementation**, and every call site parks the clock at line 0 (TIMING).
- **M21 ◻** `catch_up_z80` rolls the Z80 frontier **backward** on a gate close, refunding ≤345 mclk per
  BUSREQ. **Measured**: +60 loop iterations over 200 gate-closes (CPU-B).
- **M22 ◻** VRAM **copy** DMA omits the `address ^ 1` lane swap that fill performs — *against the very
  source the fill arm cites*, which names copy explicitly (VDP).
- **M23 ◻** `play_input.rows.items` is the **only nested object on the whole bus** and carries no closed
  key set, so a misspelled key is silently applied to port 0; and its per-row refusals drop the row index
  the caller already has (ERR).
- **M24 ◻** `EI` has no one-instruction delay and `/INT` is held for the whole vblank rather than one line
  (CPU-B) — both stated in comments, both invisible to the SST gate by construction.

## The lag question — P1a's answer is better than the standing hypothesis

⚑ **The precondition is confirmed live, from the owner's own saved layout** (`app.ron`, autosaved
17:49:22Z): the leaf `[Pacing, Spawn, Planes]` has **`active:(2)` = Planes**, drawn at 613×911; two other
leaves are collapsed. He moved it there — it is not the default.

**Two corrections to my standing hypothesis, both raising confidence in the fix while changing its shape:**
1. **It is not effects-specific.** `gather` mixes the nametable cells, every referenced tile's bytes and
   all of CRAM as well as the per-line scroll — so the cache has **never hit while the camera moves, on
   any level**. A diagnosis predicting lag only on the effects scene is therefore not what this code does;
   the effects scene must add something on top, and **plane size is the one term it can enlarge**.
2. **The dominant cost is `covered_edges`, not the pixel lookups** — 2 hardware divisions and 4 scattered
   mask loads *per plane pixel*, over the whole plane, plus a `pw*ph` bool allocation and a 0.5-2 MB
   texture upload. At 64×64 cells that is ~524k divisions for the outline alone.
   ⚑ **So "drop the scroll from the fingerprint" is the WRONG FIX**: with the outline on (the default,
   and not persisted) the outline genuinely moves every frame, and a stale outline over a live plane is a
   picture that lies. The right shape is to key the *texture* on map+tiles+CRAM and paint the outline as
   an egui shape over it.

**Two zero-cost instruments already on his screen**, which settle this with no build and no risk:
- The Planes tab's own line: *"{rasters} times in {repaints} repaints"* (`ui.rs:1322`). **Equal numbers =
  the cache never hits, measured by the panel itself.**
- The **"viewport" checkbox** (`ui.rs:1266`) — unticking it skips `covered_edges` and *nothing else*. If
  the lag halves, C-edges is the term; if it does not move, it is the dot loop or the upload.

**And the instrument gap this exposes:** `Loop::iterate` fills `Buckets::{emulate, audio, convert, upload,
ui, bus, cpu_total}` **every frame**, and the only reader runs on the bench deadline. **In the mode people
use, the window knows exactly where its 16 ms went and never says.**

---

# Third tranche — seats PROTO, P1b

## HIGH — new

- **H12 ◻ The per-line scroll fetch is done per PIXEL, and the value is already in a variable one frame up
  the stack** (P1b). `render.rs:1358-1368`: `plane_sample` calls `plane_hscroll(plane, line)` — which
  **takes no `x`** and does two VRAM reads — once per dot per plane. **640 calls per line against a
  necessary 3: 214× more often than the value can change**; 8.6 M/s at 60 fps.
  ⚑ **The proof the author knew it is per-line sits three lines away**: `resolve_line_masked:1644` computes
  `a_hscroll` **once per line** and stores it in `ctx` — and then calls the helper that re-reads it per dot.
  **Confirmed in the shipped binary** by `objdump`: six out-of-line calls per composited dot.
  This is the class the reverse walk exists to catch — four call sites, three correct, and the wrong one
  looks entirely reasonable at its own site.
- **H13 ◻ One hardware integer division per plane per dot, from a `Vec::len()` used as a modulus** (P1b).
  `render.rs:1298-1301` `% self.vsram().len()` — a runtime load, so a real `div`. **The repo already has
  the cheap spelling twice** (`vdp.rs:801`, `:865`, `% VSRAM_SIZE`); this one site diverges. Confirmed in
  the binary: LLVM folded the full-scroll arm, so **only the 2-cell/parallax arm pays — the case this lane
  is actively working on.** One token to fix.
- **H14 ◻ `emulator/z80_read` emits an off-contract `bytes` for a request the contract permits** (PROTO).
  `len: 0` is legal params (`minimum: 0`, the only such on the surface — every sibling declares 1), the
  handler accepts it, and the reply is `"0x"`, which fails the result's own `^0x[0-9A-Fa-f]+$`. **The
  fragment contradicts itself**: result `len` allows 0 while result `bytes` forbids the only spelling of a
  zero-length payload. Validated with `jsonschema` plus a passing control. Two-sided CR, not a patch.

## MEDIUM — new

- **M25 ◻ `emulator/z80_write` accepts an empty payload its two siblings refuse AND test** (PROTO).
  `write_memory` and `write_vram` both refuse with the same sentence and both have a test; `z80_write` has
  neither, and reports a no-op as a write. Its own `$comment` claims the shape "is enforced mechanically" —
  the *alternation* is, the *payload* is not.
- **M26 ◻ 68 fragment-declared param constraints, exactly ONE machine-tied to the fragment** (PROTO). This
  is the class that produced the `step` defect the charter cites: ~40 numeric ceilings are hand-transcribed
  copies. `params_closure.rs` pins param **names** both ways; nothing does that for **values**.
- **M27 ◻ `emulator/read`'s advertised summary understates the row by 4095 bytes** (PROTO) — "one byte
  read" for a row that has taken `len` 1..4096 since the day it landed. **21 days.** Same defect as
  `lookup_symbol`'s, and the summary is what `initialize`, the MCP surface and the palette all display.
- **M28 ◻ `lookup_equate` declares `minLength: 1` and enforces it nowhere** (PROTO) — `{"prefix": ""}`, a
  request the contract refuses, returns the first 256 equates of the whole table. The test named for this
  case uses `"ZZZ"` — an empty *result*, not an empty *prefix*. **The name reads as coverage it does not
  have.**
- **M29 ◻ `checkpoint_list.limit` has a handler ceiling the contract declares nowhere** (PROTO) — and the
  ceiling is a *capacity* (8), not a page size, so an ordinary `limit: 100` is refused.
- **M30 ◻ VRAM/CRAM/VSRAM are `Vec<u8>`, so every already-masked renderer index still bounds-checks**
  (P1b) — ~10 per dot. The second-order cost is larger: the panic paths are part of why the hot helpers
  never inline.
- **M31 ◻ With a mask set, every active line is composited TWICE per frame** (P1b), and the same loop is
  written in three places — each individually justified, and **none of the three says the first render was
  paid for anyway**.
- **M32 ◻ 68000 `decode` is a 113-test linear if-chain with the hottest opcodes near the end** (P1b) —
  MOVEQ is test #100, the shift family #101. The arms are documented as mutually disjoint, so a prefilter
  is a pure reordering. (The Z80 core dispatches on a jump table — the 68k chain is the outlier.)

## Instrument note carried from P1b

Its ranking is derived from **call counts and machine code, not a measured profile**: `perf`, `valgrind`
and `ptrace` are all unavailable in the sandbox, and it said so rather than implying a profile. Its
baseline (459/461 fps headless, two runs agreeing to 0.5 %) is stamped UTC **with the load average
beside it** and explicitly labelled *a floor, not an uncontended number*.

---

# Fourth tranche — seat A2 (comment truth)

## HIGH — new

- **H15 ◻ The SRAM field doc denies a currency it is in** (`system.rs:235`). Claims SRAM is *"NOT in
  `export_state` (that go-live is S3)"*. **S3 shipped**: `export_state` writes SRAM at `:796-804` and the
  same function's own comment two lines away says *"SRAM (v2 go-live)"*. `export_state_hash` is **the
  determinism gate's currency**, so a reader deciding "can an SRAM write move the gate?" reads this field
  doc and answers *no*. ⚑ **The neighbouring field's doc makes the same claim and is CORRECT** — the two
  lines read identically and only one is wrong. Textbook condition-since-met.
- **H16 ◻ Three comments say the Z80 "steps zero instructions"** (`system.rs:259`, `:1108`, `:1345`) —
  including **the top of the production run loop**. Region 4 is live, `catch_up_z80` has a live branch, and
  the module's own tests reach `Z80::step`. Anyone reasoning about run-loop cost or export currency from
  `run_frames` starts from a wrong premise. *Nuance kept:* the parenthetical `(z80_running == false in
  every fixture)` is the accurate fixture-scoped version sitting beside the absolute that overstates it —
  the `pictures()` shape again.
- **H17 ◻ A seven-step staircase of "never reaches decode" claims, all superseded** (`decode.rs:1222`
  through `:1312`). Each shift arm froze its own scope note; every arm now exists and the corpus loads all
  eight. Even the last step is one behind. Nothing is broken — **the hazard is a future edit made on a
  false disjointness premise**, which is exactly why reassurance-of-unreachability rates HIGH.
- **H18 ◻ `render_scanline` documented as "not wired into `System::run`"** (`render.rs:2125`) with the
  currency parenthetical *"so the export golden is untouched"*. It is called from `system.rs:1220` on every
  active line, and `system.rs:1766` states the corrected fact outright.
- **H19 ◻ The panel count: a correction applied to two files out of four** (`oracle-player`). `Tab` has
  **11** variants and `initial_dock` builds 4 leaves, so **7 bodies do not run**. `nav.rs` and `screen.rs`
  were amended from "six" to "four"; **`ui.rs:40` and `main.rs:1120` still say six**, so the repo now
  states both numbers authoritatively. ⚑ The amending comment itself wrote *"a wrong number in a header is
  a wrong number, and this one had been copied twice before it was checked"* — and the fix missed two of
  the four copies. **Good news recorded:** the guarding *test* derives from the leaf count rather than
  restating a figure, so the mechanism built to stop this worked and only the prose drifted.

## MEDIUM — new (comment-truth cluster)

- **M33 ◻ Seven in-repo `file:line` citations in `oracle-frontend` all resolve to unrelated code.** Worst
  is `main.rs:1776-1817`, which lands on plausible-looking run-loop code, so a reader may not notice the
  misdirection. Most name a symbol alongside the number, which is what rescues them.
- **M34 ◻ Four "NOT this commit / NOT decoded this push" notes for things decoded in the same file**
  (`decode.rs:990`, `:457`, `:547`, `:749`). The disjointness arguments still hold; only the framing lies.
- **M35 ◻ A correction written in the other file and never applied here** (`decode.rs:1045` vs
  `singlestep_m68000.rs:3241`, which says *"(correcting the earlier `*Q skipped` note)"* — and the note
  survived the correction).
- **M36 ◻ `host.rs:1618` says three of **seven** `CensusKey` variants; there are eight.** The kind of
  figure that gets transcribed into a protocol brief.
- **M37 ◻ `mcp_tool_sweep.rs:567` cites two wrong addresses for the param choke** — inside a block that
  explicitly warns *"do not restore the earlier wording from a stale doc"*. A paragraph guarding against
  staleness, carrying stale pointers.
- **M38 ◻ Two cross-repo citations into aeon have drifted** (`BUGS.md:494` → `:1183`;
  `DEFERRED_WORK.md:113` → `:4471`), and one quotes in the present tense a gap **this very crate closed**.
- **M39 ◻ `vdp.rs:184` "Read-only this push"** for latches the render pipeline now ORs every active line.
- **M40 ◻ `ui.rs:4153`/`:4165` say "all ten"/"ten leaves"** where `Tab::ALL` is 11.

## Verified clean, and worth keeping

`theme.rs`'s four *"Held, not yet wired"* notes are **still true** (every call site passes
`DEFAULT_FAMILY`) — the seat's strongest condition-since-met candidate, and it is not one. `effects.rs`'s
`NUDGE_BLOCKED` note is **current**, checked against aeon HEAD. Five aeon citations verified line-exact.
**Only two `TODO`/`FIXME` comments exist in the whole corpus**, and both describe a dependency's TODO.

⚑ **The seat's own methodological catch, in this repo's signature shape:** its first pass flagged five aeon
citations as missing files, because its grep had stripped the `aeon/` prefix — *a tool's failure
indistinguishable from its result*. It caught this by re-running with the prefix intact and reported both
the false alarm and the control.

---

# ⚑ A structural fact about this sweep, recorded because it bounds every "clean" above

**The 20-agent concurrency cap did not only delay the five owed seats — it degraded seats that ran.**
Seat A2 attempted two parallel helpers to close a line-by-line pass over `engine.rs` (~9,400 lines) and
`ui.rs` (~6,000); **both launches were refused because other seats had saturated the cap.** Seat ERR
reports the same refusal and covered `oracle-core`/`oracle-replay` by pattern sweep rather than a full
read, naming `render.rs`, `vdp.rs`, `watchpoints.rs`, `testrom.rs` and `synth/` as **unexamined rather
than cleared**.

So the panel's coverage is **narrower than its seat count suggests**, and the narrowing is invisible from
the seat list alone. Both seats disclosed it unprompted, which is the behaviour the charter's
"say what you could NOT check" rule exists to get. Any later reader treating this packet as exhaustive
should start from those two disclosures.

---

# Fifth tranche — seat P2 (algorithmic altitude)

## ⚑ CONVERGENCE — the panel's strongest signal, and it fired

**P1b (reverse walk) and P2 (algorithmic altitude) landed independently on `plane_hscroll`.** Two seats,
different hunts, told not to coordinate, both arriving at: *a value that is constant for the whole line,
fetched once per pixel per plane, 320× more often than it can change, while the line-hoisted copy already
sits in `ASlotCtx` three lines away.* Neither knew the other existed.

Per the protocol, convergence outranks any single seat's confidence. **Treat H12/A2 as the highest-
confidence performance finding in this sweep.**

## CRITICAL — new

### C5 ◻ Every frame of every replay playthrough renders a full 320-px attributed scanline and discards it

`system.rs:1220` renders every active line unconditionally; the result is *stashed* only when the sink
wants scanlines, and `BusEventSink::wants_scanlines` **defaults to false**. `render_scanline`'s only
mutation is three status booleans. So on an unarmed run the entire plane/priority/attribution pipeline
is computed and dropped **to produce three bits**.

⚑ **The seat checked who actually runs unarmed rather than assuming, which is where this finding could
have died — and found the worst possible consumer:** `oracle-replay`, *the tool whose entire pitch is
"one playthrough instead of one per stale checkpoint"*, uses a `Fanout` of two sinks that neither
override the flag. **It pays the full discarded raster for every frame of every playthrough.**

Residue that survives even when armed: `flush_pending_row` reads `line` and `pixels` only, while
`line_report_from` builds `sprites` (up to 80 entries) and two `PlaneScroll`s per line — and under 2-cell
vscroll, *what Sonic titles use*, each is a heap `Vec`. **≈27,000 allocations/second for fields nothing
reads.**

**Root cause as shape:** `resolve_line` offers one granularity; three consumers want three different
things (3 bits, RGB, one dot) and all three pay the union. `pixel_attribution_masked` is the third
witness — it resolves 320 pixels, then **re-samples** the one dot it wanted.

**Honest limit, stated by the seat and worth keeping:** it cannot rule out LLVM eliding part of the
unarmed loop, judges it very unlikely (three heap allocations across a non-inlined `pub fn` boundary),
and **TAGGED the decisive instrument** rather than asserting. It ran no timing at all, deliberately,
because load was 3.4→5.7 with the owner's window live — *"I produced none rather than produce a
misleading one."*

## HIGH — new

- **H20 ◻ `Drained::symbols` is published and has ZERO consumers — the player copied the frontend's
  signal but not its repair** (`oracle-player/src/bus.rs:930`). The frontend disarms spawn mode when the
  listing changes, with the measurement in its comment: *"Of the symbols `s4.lst` and `s4.debug.lst`
  share, **92.6% name a different address**."* The player publishes the same flag and nothing reads it,
  and nothing disarms on a listing change. **The click does not fail — it succeeds at a different
  address.** Silent-corruption shape, fixed and documented in one crate, reintroduced one crate over.
  ⚑ The irony is on the record: the frontend's own `drain.rs` calls a report field with no consumer
  *"the precise shape of the defect this file exists because of."*
- **H21 ◻ `spawn::Mode`'s doc states an invariant the frontend's own F5 swap violates** — *"disarmed
  after every reset / ROM swap"*, while the only disarm site is the explicit toggle. And the engine
  explains in words why the drain cannot cover it: *"A window that swaps its own cartridge (the
  frontend's F5) therefore does not get told about its own listing."* **Shape remedy the seat proposes is
  the right one:** derive armed-ness from the generation it was armed against, so no swap path can forget.
- **H22 ◻ 68000 decode is a 104-arm cascade over a provably 2¹⁶ input space** — and the decisive fact is
  that the whole cascade reads **only the opcode and one supervisor bit**, so it is a pure function of a
  16-bit word, fully memoizable. ⚑ **The seat argued the counter-case against itself** (each arm carries
  its recon citation; a generated table would lose the audit trail) **and then dissolved it**: keep the
  cascade as the builder, memoize in front of it, and the readable derivation survives untouched.

## MEDIUM — new

- **M41 ◻ A `String` allocated per sort comparison** (`engine.rs:6947`), Θ(m log m) heap allocations for a
  user-controlled prefix — ~24,000 allocations for one keystroke-driven lookup. **The only instance in the
  repo**, and the seat proved the cost from `sort_by_key`'s signature rather than from std's call count.
  It also cleared the symbol cap the brief asked about: the cap governs the reply, not the search.
- **M42 ◻ A full scanline is rendered to learn a width that is `if h40 {320} else {256}`** — in both GUI
  crates, every frame a mask is set. `Vdp::active_display()` is public and returns exactly that. **Bonus
  vacuity:** the `if width == 0` guard beside it *"loud-on-unmeasurable's floor"* is unreachable, because
  width ∈ {256, 320} always.
- **M43 ◻ The VDP FIFO is three fields and one index expression written out verbatim three times**
  (`vdp.rs:740`, `:1339`, `:1403`) — compared character by character, agreeing today. A fourth drain site
  that gets it wrong reads the wrong entry silently.

## Verified clean — the seat's own list is unusually valuable

Ten targets re-derived and cleared with reasons, including several I would have bet against: symbol
resolution is `partition_point` binary search over a pre-sorted index, not a scan; `self.symbols.clone()`
at four sites is an `Arc` refcount bump, not an O(n) copy; per-object symbol lookup is O(slots · log
symbols); the SAT cache's apparent two-copies-of-one-truth is **correct by design, because the staleness
being modelled is the hardware's** (the Bloodlines stale-cache behaviour) — *recomputation would be the
wrong answer*; and RPC dispatch's linear scan buys the property that *"the advertised list and the
implemented set are the same set by construction"* — **right trade, leave it.**

⚑ **Third seat degraded by the concurrency cap:** P2's breadth explorer never launched, so its coverage of
the ~700 remaining GUI loop sites is *"thinner than the forward-direction findings above"* — its words,
volunteered.
