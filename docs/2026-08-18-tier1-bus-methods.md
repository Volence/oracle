# Tier 1 shipped — the pixel-gate blockers are on the bus (2026-08-18)

Branch `aeon-tier1`, 9 commits `bdecbf6..9e60c6f` (4 code/test, 1 schema re-vendor, 4 CR/ruling/plan
docs), cut on top of the S3 lens merge (`docs/2026-08-17-player-s3-lenses.md`).
Demand statement: `docs/2026-08-17-aeon-switchover-gap-list.md` — Tier 1, items 1-3.
Change requests: `docs/2026-08-18-cr21-23-tier1-rows.md`. Ruling: `docs/2026-08-18-ruling-cr21-23.md`.

## What shipped

Three §6 methods, all **contract-first** — the ruling was applied to the CR document, the amendment
written into `empyrean`, the schema re-vendored, and the tests written against the vendored fragment
*before* a line of handler existed. That is the discipline the 2026-08-17 owner ruling asked for, and
the order is visible in the history: `a8e82cb` (CRs) → `4072069` (ruling applied) → `560ee85`
(re-vendor) → `aafcd0e`/`d1239fe`/`162debc` (handlers).

- **`emulator/write_memory`** (`aafcd0e`, hardened `fdc149e`) — the poke primitive. Work-RAM window
  `$E00000-$FFFFFF` only; a base outside it, a range whose end runs past it, or any ROM/IO target is
  `-32004` — **refused, never clipped**. Exactly one payload spelling (`bytes` XOR `value`+`width`),
  with both / neither / `width`-with-`bytes` / `value`-sans-`width` / an overflowing value / an odd
  digit count / an empty payload all `-32602`, checked **before anything is written**. Over
  `limits.maxWriteLen` is `-32602`, refused not truncated. Values land big-endian. Requires a paused
  machine (`-32005`), which is why §11.13 added the row to §6's run-control state rule.
  Two decisions worth carrying forward: the bytes travel `Bus68k::write8` rather than a `ram_mut`
  back door, so the hardware mirror masking is **the machine's** (`$E00300` and `$FF0300` are
  provably the same cell); and the sink is `()` on purpose — a poke is a debugger access, not a guest
  access, so it is never offered to the watch surface, because a hit's `pc` names the instruction
  that drove the access and a poke has none to name (ruling CR-21 R5).
- **`emulator/reset`** (`d1239fe`, review follow-up `6a47c50`) — the row has been in §6 since the
  protocol shipped and the handler never was: a client could reload the ROM from disk but not press
  the reset button on the one already loaded. Deliberately **not** `require_paused` — the contract
  forbids `-32005` here explicitly (CR-22 R3), and gating it would force the pause-call-resume dance
  §5 refuses to resolve implicitly. It **MUST NOT** change the run state (CR-22 R2): a paused machine
  sits at the reset vector, a free-running one keeps running from it. `deferred: false` is a fact
  about this server, not a constant — the reset is applied before the reply is composed. Symbols
  survive (the image is unchanged, unlike `reload_rom`), SRAM survives inside `System::reset`,
  checkpoints and watchpoints survive, held pads clear (a cold start has nobody holding anything, and
  a `hold` left armed across a reset would silently steer a scene preamble). The generation bump
  follows `restore`'s precedent, so a hosted player resyncs off `PumpReport::rom_changed`.
- **`emulator/memory_hash`** (`162debc`, symbol arm `9e60c6f`) — the gap `state_hash` leaves: its
  five fingerprints cover VDP state only, and nothing else on this bus could hash 68000 memory. A
  pure read — no `require_paused`, answered at the engine thread's single coherent point, exactly as
  `emulator/read` is. Routing is `debug_read`'s, so the two-region rule and its "work RAM" /
  "cartridge ROM" spellings are the ones `read_memory` already answers with, and a range crossing out
  of its region is `-32004`. `crc32.rs` is IEEE/zlib so a cartridge-window hash equals CRC32 over the
  same slice of the ROM *file* — the property Aeon's gates compare against. Dependency-free, table
  built in a `const fn`, `pub` for the same reason `oracle_core::state_hash` exports its primitives:
  the tests recompute the expectation themselves, and a hash only its own implementation can vouch
  for is not checked. FNV-1a-64 is now **pinned by parameter** in the contract (offset basis
  `0xCBF29CE484222325`, prime `0x100000001B3`, ascending address order) — CR-20's condition, which
  `$defs/hash64`'s "a 64-bit FNV-1a fingerprint" had not discharged.

Contract side: §11.13 in `empyrean` — `37f547c` (the amendment) + `c3b408d` (a provenance
correction, see below), on branch `tier1-amendment`. Schema re-vendored into
`crates/oracle-aether/tests/contract/` by `560ee85`, which also un-rotted the `PROVENANCE.md` table.

### Four-surface accounting (the parity rule, gap list §"Three-surface parity")

The gap list plans each Tier 1 method as **§6 row → Aether handler → MCP tool row → player surface
(if one makes sense)**, and its own table already recorded what "makes sense" meant here:

| Method | Bus | MCP | Player GUI |
|---|---|---|---|
| `write_memory` | new row + handler | tool row | later — a memory editor is its own design |
| `reset` | new row + handler | tool row | **already exists** (`Tab`/`F1`) — the bus is the gap |
| `memory_hash` | new row + handler | tool row | no natural surface; record the decision |

- **Bus ✓** — three handlers, this branch.
- **MCP ✓** — the tool rows pre-existed (`d5c5cc4` gave every advertised method a row and taught
  `list_tools` to negotiate on the handshake). This slice fixed `reset`'s mangled description and
  taught the coverage sweep to poke and to reset last: `oracle` commit `78f0157`. **That repo has no
  remote**, so the commit exists locally and cannot be pushed.
- **Player GUI** — reset already exists on `Tab`/`F1`; `write_memory`'s editor is **deferred by
  design**; `memory_hash` has **no natural surface, by decision**. Per the owner rule, the gap is a
  decision someone made, not an omission nobody noticed — and the decisions are the gap list's own
  table, cited above rather than re-invented here.

## Live validation (controller-run, against the real server)

- The MCP coverage sweep ran **31-for-31** through the real `call_tool` path against
  `aeon/s4.debug.bin` — both runs `EXIT=0`.
- A `value`+`width` write was independently **read back as `0xABCD`** — the round trip, not the
  reply.
- The server's CRC-32 was confirmed against Python's `zlib` over the same bytes: **`0xE9FFC9D0`**.
- A post-reset read returned the **boot-time** value: the reset is proven by a value, not by a claim.
- One probe errored, and it was the **probe's** bug (`space:"m68k"`, not a space this bus serves) —
  keeping the standing record **4-for-4 that sweep failures have been the harness's**, not the
  server's.

## Gates (run firsthand on `9e60c6f`, this worktree)

```
FMT=0
CLIPPY1=0        # cargo clippy --all-targets --workspace -- -D warnings
CLIPPY2=0        # ... --no-default-features -- -D warnings
EXIT=0           # cargo test --workspace
LEGS=39 PASSED=1580 FAILED=0 IGNORED=4
NO-FAILING-LEGS  # every `test result` line reports " 0 failed"
```

**Zero core diff.** `git diff --stat m68000-microop-framework -- crates/oracle-core/` is **empty** —
this slice touched no `oracle-core` file at all, so the two-dot form that misled S3 (main advancing
underneath it) cannot mislead here: there is nothing on either side of it.

S3 merged at 36 legs; the three new integration test files (`write_memory.rs`, `reset.rs`,
`memory_hash.rs`) are the three new legs.

## Mutation ledger

Every evidence-bearing test in the slice carries a mutation line in its commit body. Transcribed
here, attributed:

**`aafcd0e` — write_memory** (each applied, named test confirmed FAILING, reverted, file touched so
the next run recompiled rather than reusing a stale binary):

1. dropped the `end > WORK_RAM_HI` clause -> `rom_and_out_of_window_are_refused` FAILED (a 2-byte
   write at `$FFFFFF` succeeded instead of `-32004`).
2. `to_be_bytes` -> `to_le_bytes` -> `value_width_is_big_endian` FAILED at the width-2 read-back.
3. emptied the `bus.write8` loop body (reply success, write nothing) ->
   `bytes_land_in_ram_and_read_back` FAILED on the independent read path.
4. removed the `require_paused` call -> `a_free_running_machine_refuses_the_poke` FAILED (the poke
   succeeded on a free-running machine).
5. loosened the cap to `> max_write_len + 1` -> `an_over_cap_payload_is_refused_not_truncated`
   FAILED — and it failed **in the schema validator**, on `len: 4097` exceeding the fragment's own
   maximum, which is the contract catching it before the assertion could.

**`fdc149e` — the refusal probe made load-bearing:** made the both-spellings arm write its `bytes`
payload before returning `-32602` -> `payload_spelling_is_exactly_one_of_two` FAILED with
`left: "0x00", right: "0xA5"` — which is exactly the leak the old assertion would have passed
straight through, since `0x00 != 0x5A`.

**`d1239fe` — reset** (six tests, each mutation-checked):

- drop `self.sys.reset()` -> `reset_restores_the_power_on_anchor` FAILS
- drop `self.rom_generation += 1` -> `a_client_reset_reaches_the_player_as_a_rom_change` FAILS
- hardcode `deferred: true` -> schema still passes (it is a boolean); `reset_restores_the_power_on_anchor` FAILS
- `set_free_run(false)` in the handler -> `run_state_is_preserved_both_ways` FAILS on the
  free-running half

**`6a47c50` — the survival restore made observable:** deleting the `restore` call from the test ->
`watch_and_checkpoint_surfaces_survive` FAILS on "the pre-reset machine came back byte-for-byte".
Recorded honestly for what it is — that proves the new assertion *depends* on the restore, not that
restore is correct; restore correctness is `tests/checkpoints.rs`'s job.

**`162debc` — memory_hash** (each applied alone, recompiled, reverted):

1. handler hashes `&data[..data.len()-1]` -> `the_hash_matches_the_bytes_read_back` and
   `a_cart_window_hash_matches_the_rom_slice` FAIL (5 passed, 2 failed).
2. CRC init `0xFFFF_FFFF` -> `0` -> `the_check_vector_holds` FAILS (0 passed, 2 failed) against the
   outside-world ITU/zlib vector.
3. `parse_count` max arg -> `u64::MAX` -> `bounds_and_the_required_len` FAILS: the cap case returns
   `-32004` instead of `-32602` (6 passed, 1 failed).
4. CRC final XOR removed -> `the_check_vector_holds` FAILS, proving both halves of the convention are
   load-bearing (0 passed, 2 failed).

**`9e60c6f` — the memory_hash symbol spelling:** handler takes `addr` only, dropping `resolve_target`'s
symbol arm — the realistic form of this bug — and the new test is its **sole** guard (7 passed,
1 failed).

## What the process caught this slice

- **The adjudication reversed CR-22's central evidence claim.** The CR said Oracle's threading forces
  `deferred: true` "because the reset drains on the GUI thread after the reply is composed"; the code
  says the opposite — `ControlSocket.cpp` waits on `pendingReset->done` (`:479-486`), set only after
  `doFullReset` has run, and *then* composes the `true` reply. Oracle's `true` is a **mechanism**
  report, not a **when** report, and the CR's "only definition under which both shipping behaviours
  are legal" was therefore false.
- **The plan's own reset anchor test was vacuous.** Its first draft used `state_hash` as the "the
  machine really moved" sanity check — but `state_hash` covers the five VDP fingerprints only, and
  the fixture ROM stirs work RAM without touching the VDP. Work RAM at `$FF0000` is the instrument;
  `state_hash` now rides along as the VDP half of the claim.
- **The `write_memory` refusal probe was near-vacuous** (`assert_ne!(back["bytes"], "0x5A")` — no
  refused payload carries `0x5A`, and a real leak is a `0x00`-shaped byte indistinguishable from
  reset RAM). Its strengthened sentinel form's mutation check **proved the old one passed a leaking
  handler**.
- **The sweep's `ORDERED_LAST`-alone edit would have silently dropped both new tools from the sweep
  while reporting PASS** — a coverage instrument that under-reports its own coverage and calls it
  green.
- **A false provenance clause in §11.13 was caught by review** and corrected in `c3b408d` (the key is
  four days younger than the socket).

## Registered follow-ups

- **F-WALK-ROW** — the sprite link walk still owes a §6 row. Its defer-trigger ("it gets a contract
  row when something renders it") **fired** when S3's outline lens started rendering
  `render_line_report().sprites`. Needs its own CR; owed regardless of this slice. Already recorded
  in `docs/2026-08-17-aeon-switchover-gap-list.md`; anchor is `lens/mod.rs` `models()`.
- **F-HOSTED-RESET-SRM** — a **bus-initiated** reset in the hosted player bypasses the player's
  flush-before-reset (`crates/oracle-frontend/src/main.rs:1360-1369`, the `commands::Cmd::Reset` arm
  — `flush_pending_srm` then `sys.reset()`), so an unsaved SRAM delta sitting inside the autosave
  debounce window loses its dirty signal. The window is small and the standalone server is
  unaffected; the fix belongs to the player's pump loop, not to the handler.
- **F-RENAME-ROM-CHANGED** — `PumpReport::rom_changed` now fires on **three** producers, two of which
  do not change the ROM. The doc comment is fixed (`6a47c50`, `host.rs:138-149` — "read it as
  *resynchronise*, not *re-read the ROM*"); the rename (`machine_replaced`) is recorded as a decision
  for later rather than done now.
- **F-WM-ECHO** — `write_memory`'s reply does not echo the written bytes, so a model using the
  `value`+`width` spelling cannot confirm the round trip without a second read. An additive result
  key is a **contract change request**, not a patch: §8 item 20 makes an unknown key on the wire a
  change request, never a shipment. Candidate **CR-24**.
- **F-ADDR-SYMBOL-BOTH** — the new fragments make passing **both** `addr` and `symbol` schema-invalid
  (`oneOf: [{required:[addr]},{required:[symbol]}]`), but the shared `resolve_target`
  (`crates/oracle-aether/src/engine.rs:981-995`) silently **prefers** `symbol` instead of refusing
  with `-32602`. This is engine-wide — `read_memory`, `run_to`, `lookup_symbol`, `watchpoint_add`,
  and now both new rows share the helper — so it wants one consistency pass with its own tests, not a
  local patch here.
- **F-STATE-HASH-PROBE** — record the measured lesson as a house rule: **`state_hash` alone is not a
  "the machine moved" probe against RAM-only activity.** It fingerprints VDP state; a fixture that
  stirs work RAM and never touches the VDP leaves it constant, so any test using it that way asserts
  nothing. Caught here in the plan's own authored test (see above); the shape is general.

## Push state

- **oracle-next** — branch `aeon-tier1` is **unmerged and unpushed** at the time of writing; Task 10
  merges it.
- **empyrean** — branch `tier1-amendment` (`37f547c`, `c3b408d`) is **unmerged and unpushed**.
- **oracle** — `78f0157` is committed and **cannot be pushed: that repo has no remote.**

## Owner-owed

Unchanged by this slice, and still owed: the **full smoke checklist** from S1/S2/S3
(`docs/2026-08-17-player-s3-lenses.md` carries the extended list — nobody has walked it), the
**gamepad** (still nobody has plugged one in), and **SY-7 mix levels**.

New, and it is the one that matters for the switchover: **nobody has run the Aeon-side `ab_runner`
against these three methods yet.** The sweep proves our server answers; the `ab_runner` re-point is
what proves the pixel gate actually closes "without the C++ surgery". That is the switchover's actual
finish line, and it is on the far side of the socket.
