# Scanline readback shipped — the ritual becomes a gate (2026-08-19)

Branch `scanline-readback`, 5 commits `ce36ff4..056f07f` (2 code/test, 1 schema re-vendor, 2
CR/ruling docs, plus this handoff), cut on top of the Tier 1 merge
(`docs/2026-08-18-tier1-bus-methods.md`).
Demand statement: `docs/2026-08-18-aeon-scanline-readback-demand.md` — Ask 1, their top priority,
ranked above stepping by the demand side itself.
Change request: `docs/2026-08-18-cr24-scanlines.md`. Ruling: `docs/2026-08-18-ruling-cr24.md`.

The measurement this replaces was a **hand ritual**: pause → poke → screenshot → count pixels in a
PNG, ~20 times per delay sweep, with three separate Aeon capture protocols having already failed
their own controls. It is now a call that returns bytes a test can assert on.

## What shipped

One §6 method, **contract-first** — CR written, ruled, applied to the CR document, the amendment
written into `empyrean`, the schema re-vendored, and the tests written against the vendored
fragment *before* a line of handler existed. The order is visible in the history: `ce36ff4` (CR) →
`e6164df` (ruling applied) → `2500372` (re-vendor) → `50ef55a` (handler + tests) → `056f07f`
(review fixes).

- **`emulator/scanlines`** (`50ef55a`, review follow-up `056f07f`) — reads the **drawn rows** of the
  last completed frame back: `startLine` + `count`, one row range per call, each row carrying
  `line`, `width` and `rgb` (a hex byte string of exactly `width` × 3 bytes, shadow/highlight
  already applied by the renderer that produced the values). The content is the **live per-line
  raster**, so mid-frame CRAM / scroll / S-H effects are *in* it — the exact blindness that
  `pixel_attribution`'s own first normative bullet names, and that `emulator/screenshot`'s PNG path
  cannot be asserted on.

  Four decisions worth carrying forward:

  - **`source` is required and names which instrument answered** — `"raster"` (the live capture,
    the normative content) or `"stateRender"` (the post-hoc render, carrying `caveat`). A
    stateRender reply is still an *answer*: a machine that has never completed a frame has no
    raster to show, and refusing would make the method unusable on a freshly-reset machine for no
    client benefit. But a post-hoc render is structurally blind to precisely the effects this row
    exists to see, so **a gate that depends on mid-frame liveness MUST check `source == "raster"`**.
    `reset`, `reload_rom` and `restore` drop the retained frame — the machine under it was
    replaced — and the first call after any of them answers stateRender until a new frame
    completes. A row whose provenance is unstated is worse than a wrong one.
  - **Bounds are refused, never clipped** (`-32602`). `startLine` past 223, `count` under 1, or the
    **sum** past 224 — the sum checked in the handler because no static schema can see it. A
    clipped range hands back fewer rows than were asked for with nothing on the wire saying so.
  - **Deliberately not `require_paused`.** A pure read, exactly as `read`, `pixel_attribution` and
    `sprites` are: §6's run-control state rule gives no ground to gate it, and the envelope's
    `running: true` plus the §2.2 stamp is the contract's whole answer to a torn sample.
  - **`mode` is derived from the answering frame's own width**, not from a VDP register read. The
    frame readers normalize a mid-frame width switch to the width the frame ended on (shorter lines
    black-padded, longer cropped), so a `mode` taken from the register could name a width the rows
    do not have. The fragment ties `mode` ↔ `rows[].width` ↔ `rgb` length with an `if`/`then` in
    `result.allOf` rather than leaving it to prose, so a disagreement is a **rejected reply**, not a
    prose violation nobody runs.

  There is **no frame parameter**. "As of frame F" is achieved by driving the machine to F
  (`run_frames`, `run_to`) and reading — the bus's whole-frame read model, and sub-frame addressing
  was expressly declined by the capability's own demand side ("the deterministic frame counter is
  fine").

Contract side: **§11.14** in `empyrean` — `46e0567`, on branch `scanline-amendment`. It also
**edits** the standing `pixel_attribution` sentence at `protocol.md:1066-1067`, because leaving
*"a per-scanline capability, which this catalog does not yet have"* beside a row that now exists is
the live prose/catalog contradiction D14 files as a spec bug; §11.3's log echo of the old sentence
stays untouched, since amendment logs are records, not live text. Schema fragments **32 → 33**, the
top-level `description`'s count recounted again. Re-vendored into
`crates/oracle-aether/tests/contract/` by `2500372`.

### Four-surface accounting (the parity rule)

| Surface | This slice |
|---|---|
| **§6 row + fragment** | new — `emulator/scanlines`, §11.14, fragment 33 |
| **Aether handler** | new — `Engine::scanlines`, `crates/oracle-aether/src/engine.rs:1721` |
| **MCP tool row** | **new this time** — no legacy row existed (grep-verified in the CR and again before the edit) |
| **Player GUI** | **none, by decision** — the window *is* the presentation |

- **Bus ✓** — one handler, this branch.
- **MCP ✓** — unlike Tier 1, where the rows pre-existed, this row had to be **written**. `oracle`
  commit `81e41bb` adds the `("scanlines", …)` tuple beside the read-shaped video tools and teaches
  the coverage sweep to exercise it (`EXERCISE_ARGS["scanlines"] = {"startLine": 96, "count": 4}` —
  a pure read, so the generic loop, no `PAUSE_FIRST`, no `ORDERED_LAST`). **That repo has no
  remote**, so the commit exists locally and cannot be pushed.
- **Player GUI — decided, not omitted.** The player already renders every line of every frame
  through the very capture this method reads (`blit_capture`,
  `crates/oracle-frontend/src/main.rs:503` — `store_from_capture`'s twinned implementation, not its
  caller). A GUI scanline panel would re-present what the window *is*. Recorded per **D15**'s
  discipline (grounding verified by the adjudicator at `protocol.md:237-242`), which is the rule
  that the gap must be a decision someone made rather than an omission nobody noticed.

## The shape of the diff: zero core MACHINE changes, one testrom builder

The whole capability is a **bus surface over a seam that already existed**.
`oracle_core::scanline_capture::ScanlineCapture` has captured the live per-line raster since the S3
lens work; the handler slices the same `(width, frame, from_raster)` tuple `screenshot` already
consumes. Nothing new is captured, latched, cleared or stored.

```
$ git diff --stat m68000-microop-framework...scanline-readback -- crates/oracle-core/
 crates/oracle-core/src/testrom.rs | 141 ++++++++++++++++++++++++++++++++++++++
 1 file changed, 141 insertions(+)
```

**One file, additive only, and it is the fixture builder — not the machine.** `crates/oracle-core/tests/`
is a **zero-file diff**: not one currency test, golden, or conformance row moved, because nothing
they measure moved.

The builder is `testrom::build_cram_midframe(line)`, which the adoption condition's suite gate (i)
required and which nothing in this tree had: it polls the HV counter at `$C00008` until the beam
reaches `line`, repaints the backdrop CRAM entry, and **re-arms in each vblank**. `build()` and
every other builder are byte-identical.

## Live validation (controller-run, against the real server)

Server: this worktree's `oracle-aether` (release) on `aeon/s4.debug.bin`, socket
`/run/user/1000/sr5-check.sock`, PID 219795, killed by PID afterwards.

- The MCP coverage sweep ran **32-for-32** through the real `call_tool` path — both passes
  `EXIT=0`:

  ```
  server advertises 32 methods; tool table has 63 rows; 32 shared
  MISSING TOOL ROWS (0) — every advertised method is reachable.
  list_tools returned 32 tools for this server
    ...and every one of them is a method this server advertises.
  RESULT: PASS — every advertised method has a tool row, and every exercised tool answered.
  ```

  with the new row exercised in the generic loop:

  ```
  ok   emulator_scanlines {'startLine': 96, 'count': 4} -> {   "droppedEvents": 0,   "frame": 603,
       "mclk": 540312136,   "mode": "h40",   "rows": [     {       "line": …
  ```

- One targeted call through `call_tool` (`{"startLine": 98, "count": 2}`, after 30 frames) rendered
  `source: "raster"`, `mode: "h40"`, two rows of `width: 320` with **`rgb` exactly 1922 chars**
  (`0x` + 320 × 3 × 2) — the fragment's `^0x[0-9A-Fa-f]{1920}$` length, checked against the wire
  rather than against the prose.

- **Standing record holds: 5-for-5 that sweep failures have been the harness's, not the server's** —
  there were none this round.

## Gates (run firsthand on `056f07f`, this worktree)

```
FMT=0
C1=0             # cargo clippy --all-targets --workspace -- -D warnings
C2=0             # ... --no-default-features -- -D warnings
EXIT=0           # cargo test --workspace
LEGS=40 PASSED=1588 FAILED=0 IGNORED=4
NO-FAILING-LEGS  # every `test result` line reports " 0 failed"
```

Tier 1 merged at **LEGS=39 PASSED=1580 IGNORED=4**. The one new leg is
`crates/oracle-aether/tests/scanlines.rs` — **8 tests, and they are the whole +8**: the fixture
builder ships with no unit tests of its own, because its evidence is the two-timings poison
assertion in the integration leg (mutation 4 below) rather than a self-check.

## Mutation ledger

Every evidence-bearing test in the slice carries a mutation line in its commit body. Transcribed
here, attributed. Each was applied, the file touched so the next run recompiled rather than reusing
a stale binary, the named test confirmed FAILING, then reverted.

**`50ef55a` — the handler and the fixture:**

1. handler slices rows one line high → `a2_two_timings_differ` FAILS at the boundary ("line 50 still
   draws colour A": got white, wanted black).
2. hex encoder drops each row's last pixel → **the fragment's `if`/`then` rgb-length pattern rejects
   the reply on the wire**; 6 of 7 tests fail, first `the_happy_path_returns_the_requested_rows`
   ("does not match `^0x[0-9A-Fa-f]{1536}$`"). The contract caught it before an assertion could.
3. `source` hardcoded `"raster"` → `restore_drops_the_frame_and_the_same_machine_answers_staterender`
   FAILS ("a server MUST NOT serve a frame drawn by a machine that is no longer there").
4. builder skips the HV poll and writes CRAM immediately → `a2` FAILS ("one frame must carry both
   backdrops"): both halves come back the new colour. **This is the mutation that proves the fixture
   is a fixture and not a constant.**
5. bounds clipped instead of refused → `bounds_are_refused_never_clipped` FAILS (`startLine` 200 +
   `count` 25 succeeds with 24 rows).

**`056f07f` — the pure-read pin made observable:** adding `require_paused` to `Engine::scanlines` →
`it_answers_a_free_running_machine` is the **sole** failure, 7 passed / 1 failed, on
`-32005 reason=machineRunning`. Reverted; `engine.rs` is byte-identical to `50ef55a` in that commit.
The test additionally asserts the envelope's `running: true`, because a server that answered by
quietly pausing first would satisfy the call while violating §8 item 12 — `running` is what tells
those apart.

Gate (ii) of the adoption condition is `color_1536`, the corpus's pinned LIVE-DIFFERS ROM, read
twice at one machine point: once with a frame retained, once after `restore` dropped it. That
doubles as the R4 invalidation pin. (`cram_flicker` is pinned IDENTICAL-TO-POST-HOC and is **not** a
liveness discriminator — do not substitute it.)

> **Re-checked 2026-08-19 against `F-SCANLINE-SUBLINE` slice 4, and the sentence stands.** That slice was
> the obvious candidate to overturn it: `cram_flicker` writes CRAM 16× per active line, so once landings
> resolve to a pixel its rows really do split. Measured — 2,692 value-changing in-active-window writes in
> the hashed frame — and it **stayed IDENTICAL-TO-POST-HOC**, because every one of those writes is to index
> 4 or 36 and the picture samples only index 0. So each segment decodes the same colour and the ROM remains
> a non-discriminator, now for a proven reason rather than a predicted one. `color_1536` remains the gate;
> its hash moved to `0x9ae4acc58d2a382d` in that slice.

## What the process caught this slice

- **The adjudication caught an unexecutable adoption condition.** The CR gated registration on the
  sweep-spec's A2 *verbatim* — which requires a ROM from an unmerged Aeon branch plus §5's
  game-state ritual (Camera_Y = 144, three live-pointer pokes), none of which exists in this tree,
  where tests boot `testrom::build()` and no builder wrote CRAM mid-frame at all. **Registration
  gated on an unexecutable clause is a condition that gets waived silently.** R1 replaced it with
  two gates that run here (the two-timings poison fixture; the `color_1536` liveness diff) and
  demoted the verbatim sweep to an *acceptance protocol* owned by the demand side.
- **The implementer caught the write-once-fixture trap.** A fixture that writes CRAM once marks
  exactly one frame, so the assertion would silently depend on which frame the reader happened to
  stop at — green or red by luck. `build_cram_midframe` re-arms every vblank instead, so every
  completed frame after the first carries the split.
- **The quality review caught a rustdoc off by one frame.** The builder's `///` claimed "every
  completed frame carries the split". Frame 0 does not — the poll begins after the frame-0 vblank
  arm, so the first colour-B write lands in frame 1 — and that truth lived only in a body comment
  fifty lines below, where a caller reading the docs never sees it. Now stated in the `///` block
  with the usable precondition spelled out (read at any frame ≥ 1). Doc-only: not one ROM byte
  moved.
- **The §11.14 claim audit came back CLEAN — the first amendment round with zero wrong claims.**
  Tier 1's §11.13 needed a provenance correction (`c3b408d`); CR-22's central evidence claim was
  *reversed* by its adjudication. This round's ~40 checked anchors all held. Worth recording
  because the audit's value has, until now, always been the count of things it found.

## Registered follow-ups

- **F-SCANLINE-INDEX** — per-pixel CRAM index, for *attribution*: "pixels at row 99, x > 170 use
  index `$4A` and that entry changed mid-row" — a gate that detects **their** change, not **a**
  change. Demand-side ranked second, explicitly not zero.
- **F-SCANLINE-SH** — per-pixel shadow/normal/highlight state, to split the palette-write op from
  the S-H-register op. Aeon shipped a recorded bug carrying the palette half without reg `$0C`
  bit 3 ("tinted but visibly lighter", found in play); in RGB alone that reads as slightly-wrong
  colour, not a missing op.

  Both are out of this row for the same reason: the renderer resolves indices and S/H internally and
  hands the sink only RGB (`on_scanline(line, &[(r,g,b)])`,
  `crates/oracle-core/src/scanline_capture.rs:141`; the S/H-aware conversion is private). Extending
  the renderer→sink interface **is a core change with its own currency-neutrality scrutiny**, and
  landing it inside a bus CR would smuggle a core change under a contract row. Adding the fields
  later is additive, D5's direction. The demand side confirmed the split: *"Field 1 ALONE unblocks
  the sweep completely… fields 2–3 must NOT hold it up."* Full rationale: CR-24 pin 6.

- **F-TESTROM-DISP-GUARD** — branch-displacement truncation is unguarded **file-wide** in
  `crates/oracle-core/src/testrom.rs`: `(loop_top as i32 - (bra_at as i32 + 2)) as i8 as u8` at
  `:193`, `:396`, `:481`, `:537`. A builder edit that pushes a loop body past ±127 bytes silently
  produces a *different valid branch* rather than failing to assemble — a fixture that boots and
  measures the wrong thing. Wants one `debug_assert!`-and-message pass over all four sites, not a
  local patch at the new one.
- **F-HEX-BYTES-PERF** — the row encoder is `hex::bytes` over a per-row `Vec<u8>`, i.e. a `format!`
  per byte, and `self.framebuffer()` hands back a **clone of the whole frame** even when two rows
  were asked for. A full-frame call is ~430 KB of hex built one `format!` at a time. Correct today
  and nothing measures it yet; registered as future perf, not a defect.
- **Carry-forwards** from `docs/2026-08-18-tier1-bus-methods.md`, unchanged by this slice and still
  owed: **F-WALK-ROW**, **F-HOSTED-RESET-SRM**, **F-RENAME-ROM-CHANGED**, **F-ADDR-SYMBOL-BOTH**,
  **F-RESET-SURVIVE-PINS**, **F-STATE-HASH-PROBE**. **F-WM-ECHO** is **deprioritized by its own
  beneficiary** — Aeon read back to verify and `memory_hash` makes it cheap: *"do not prioritise it
  on Aeon's behalf."* It stays registered, not scheduled.

## Acceptance protocol — handed to Aeon

R1 demoted the sweep-spec's verbatim A1/A2 from a suite gate to an **acceptance protocol on the
demand side's own harness**, because it depends on a ROM that does not exist in this repo (the spin
word at `Raster_Buf_A + 20` exists only post-item-1a on aeon branch
`parcel/raster-substrate-byte-moving`, unmerged) and on §5's game-state ritual (Camera_Y frozen at
144, three live-pointer pokes).

**Their side runs it when their branch's ROM is in hand:**

- **A1 — determinism**: ≥ 3 runs byte-identical. This is the criterion three prior Aeon capture
  protocols failed on their own controls.
- **A2 — non-vacuity**: N = 0 and N = 17 MUST produce different content on row 99. By end of frame
  the CRAM value is identical either way; only the mid-frame landing time differs, so a
  post-frame-state capture — however clean and deterministic — returns identical rows for both. The
  shape of A2 is already permanently in **our** suite as `a2_two_timings_differ`; what runs on their
  side is A2 against **their** fixture.
- **The two discriminator anchors run unconditionally as the sweep's first two data points**, not
  only on disagreement — they give the sweep two known-good calibration rows before it ventures into
  the unmeasured shape: the **row-119 fixture** (`reg_set` + `stream_cram`, CRAM op second; measured
  1 px spill → 0, boundary on the authored line) and **R1 §7.3** (`pal_restore` alone, dispatch
  depth 4; row 139 fully tinted, 140+ fully base, OFF edge exactly on the authored line). Both were
  observed CLEAN on oracle and are buildable by the same `raster_cost_probe.py` encoder. Their
  verdict table (`docs/2026-08-18-aeon-scanline-readback-demand.md`) assigns the fault: both
  CLEAN + sweep disagreeing with [15, 19] → Aeon's §3 arithmetic; either DIRTY → our raster timing
  or the capture's sampling point; both DIRTY → fixture/harness, check the **content trap** first.
- **Every gate on their side must assert `source == "raster"`.** A stateRender reply is a legal
  answer to the call and a vacuous answer to the sweep.

Sequencing note: their ROM lives on an unmerged branch, so the earliest this can run is when that
branch is buildable on their side. Nothing on our side blocks it.

## Push state (at doc time)

- **oracle-next** — branch `scanline-readback` is **unmerged and unpushed**.
- **empyrean** — branch `scanline-amendment` (`46e0567`) is **unmerged and unpushed**.
- **oracle** — `81e41bb` is committed and **cannot be pushed: that repo has no remote.**

## Owner-owed

Unchanged by this slice and still owed: the **full smoke checklist** from S1/S2/S3, the **gamepad**
(nobody has plugged one in), and **SY-7 mix levels**.

Carried forward from Tier 1, and now the switchover's live finish line: **nobody has run the
Aeon-side sweep against this method yet.** Our sweep proves the server answers and our suite proves
the answer is *live*; the A1/A2 run against their fixture is what proves the ~20-screenshot ritual
is actually gone. That is on the far side of the socket.
