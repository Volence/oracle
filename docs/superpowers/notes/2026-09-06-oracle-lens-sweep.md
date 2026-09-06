# Oracle full-repo lens sweep — PACKET (in progress)

Charter, corpus, roster deviation and seat rules:
`2026-09-06-oracle-lens-sweep-CHARTER.md`. Pin `d3ca871` / review revision `e6bc191` (identical code).

**STATUS: IN PROGRESS.** 22 seats launched, **5 returned** (TEST, GATE, STATE, FUZZ, ARCH). This file is
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
