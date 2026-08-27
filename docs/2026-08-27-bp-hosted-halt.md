# The hosted breakpoint halt — closing the gap the surface shipped with

**Date:** 2026-08-27 · **Branch:** `parcel/bp-hosted-halt` · **Base:** `4fd3cdd` (`main` at dispatch)

**Acceptance delta: none. Contract surface moved: none.** No new wire key, no new capability, no new
`reason` value, no CR. `emulator/stopped` with `reason: "breakpoint"` already existed and is what this
emits; `wait_for_break` already existed and is what a client already calls. What changed is that against
the **windowed player** those two now do what they have always claimed to do.

**⟨RUNTIME⟩ ban honoured.** No emulator was launched, no `oracle-aether` spawned, no socket outside the
test harness dialled, no `mcp__oracle__*` tool touched. Every claim below is from an in-process fixture or
a command whose output is quoted.

---

## 0. What was served

`docs/2026-08-27-breakpoints.md` §9 registered one gap: *a breakpoint does not fire while the hosted
player's own free-run loop drives the machine.* `docs/2026-08-27-bp-disclose-recon.md` measured that gap
as **total** in the arrangement the owner actually uses, and its OVERSEER REVIEW ratified closing it. This
parcel closes it.

The two halves that composed the gap, restated because the fix has to answer both:

* the player's 60 Hz loop reached the machine through `Engine::run_sinks`, which returned **two**
  instruments and no breakpoint sink;
* every bounded advance that *does* carry a breakpoint is refused `-32005 machineRunning` while the player
  plays, because hosted, *"an un-paused player **is** a free-running bus"* (`host.rs`'s own words).

So `resume` → `wait_for_break` — the idiom the surface documents and `aeon/tools/evict_witness.py`
implements — was exactly and only the broken path, and it failed by answering `{"timeoutReached": true}`
with `breakpoint_list` showing `hits: 0`. Every field true; the composite indistinguishable from *"the ROM
never reached that address"*, with `hits: 0` corroborating the wrong reading. A believable wrong answer
about the **program under test**.

---

## 1. The design, and the four calls that make it

### 1.1 `run_sinks` returns three halves, and the third is bare

`Engine::run_sinks(resume_pc) -> (Option<Observe<&mut Watchpoints>>, Option<Observe<&mut Profiler>>,
Option<BreakStop<'_>>)`, forwarded verbatim through `Host` and `oracle-frontend`'s `bus.rs`, with a
same-shaped twin in `bus_stub.rs` so the run loop has no `#[cfg]` of its own.

**The asymmetry is the parcel.** The two instruments stay wrapped in `Observe`; the breakpoint is bare.
Not a style choice on either side:

* a `stopAfter` watch raises `stop_requested` on a **level** (`matched >= n` stays true forever), so
  attached bare it would end every one of the loop's 1-frame runs before it began — a client's stop
  condition turned into a frozen window;
* a breakpoint's condition is an **edge**, re-evaluated per step boundary against the current PC, which
  cannot latch on;
* and an `Observe` around the breakpoint is **worse than not attaching it at all**. `Observe` overrides
  exactly one hook — `stop_requested -> false` — so the sink still latches and the halt still lands. What
  changes is that the frame ran to completion first, so the machine is reported as stopped somewhere it
  is not. Measured under that mutation: `pc 0x00000210` for a breakpoint at `0x0000020E`.

### 1.2 `resume_pc` comes from the caller, because it cannot come from anywhere else

`BreakStop` suppresses a fire at the PC the run *started* on until one instruction has retired. Without
it, a machine halted at a breakpoint re-fires on that same address before anything executes, the run
advances nothing, and the window is **unresumable** — `resume` appears to work and the clock never moves.

The engine cannot read that PC for itself here: outside a `Host::pump` drain window it holds an inert
placeholder `System` whose PC is `0`. The run driver owns the real machine, so the run driver supplies it.
This was the recon's named weakest point (*"the one API change with no precedent to lean on"*, and *"if
the borrow-checker rejects the 3-tuple … (b) gets dearer"*). **It borrow-checks.** `BreakStop` borrows
`self.breakpoints` shared while the other two borrow their own fields mutably; the fields are disjoint and
the three-element split compiled first time.

### 1.3 The halt is ONE function, extracted rather than copied

`Engine::halt_on_breakpoint(addr)`, lifted out of `free_run_step` and now called by both drivers.

This is not tidiness. The surface has already produced the two-flag bug once: `free_run` is the *mode* and
`running` is *is-it-advancing-now*, a halt ends both, and a halt that cleared only `running` left the loop
free-running and re-broke once per frame — **374,011 stops where the contract says 1**
(`docs/2026-08-27-breakpoints.md` §5). There are now two run drivers that halt. A second hand-written
halt is a second chance to clear one flag, so there is only one halt.

### 1.4 The observation is latched, and applied at the top of the next drain

`Host::record_break(addr)` sets `pending_break`; `Host::pump` takes it right after `pending_free_run`.
Two reasons, and the second is the one §9 never named:

* **D11.** `emit_stopped` stamps whatever machine the engine is holding, and reads the stopping `pc` off
  it. Applied where the observation is made, that is the placeholder. Measured under that mutation:
  `{"frame":0,"mclk":0,"pc":"0x00000000","reason":"breakpoint"}` — the recon predicted this exactly.
  `pending_free_run` exists for the identical reason and is the precedent copied.
* **Ordering.** See §1.5.

`record_break` uses `get_or_insert`, not assignment. Today exactly one frame runs between a latch and the
drain that takes it, so a second cannot arrive — but the *earlier* halt is the one that stopped the
machine, and silently replacing it would report the wrong address for the stop.

### 1.5 The ordering rule, and the collision that makes it real

`pending_break` is applied **after** `pending_free_run`. Both are deferred changes to the same pair of run
flags and they can be queued in one iteration, in exactly one reachable way:

> A human un-pauses the window on the very iteration whose frame hits a breakpoint. The frame runs and
> halts; then `set_paused(false)` sees the engine still paused, and queues `pending_free_run = Some(true)`
> **behind** the latched halt.

Applied the other way round, the un-pause lands second and puts `free_run` back: **a machine that pauses
and instantly resumes** — a new believable wrong answer rather than a missing one. A halt is the later
fact and it wins.

Note what this collision needs: a *local* un-pause. A client-driven `resume` cannot produce it, because by
the time the player is running again the engine's flag already agrees and `set_paused` queues nothing. So
the wire suite structurally cannot reach this case, and §3 records that it does not.

### 1.6 What the run loop owes, and what it does not have to get right

`main.rs`'s obligation is three lines: read `sys.cpu_regs().pc` before the run, put the bare sink in the
**outer** `Fanout` beside the capture, and hand the observation back. The sink rides outside
`AudioAndWatch` deliberately — that alias is itself a `Fanout` and would compose the stop correctly, but
placing the halt where a plain `Fanout` carries it in every build variant means no future combinator can
quietly swallow it.

**Order of `record_break` against `set_paused` does not matter, and that is by construction**: the former
only latches and the latter reads the *engine's* flag, which neither has moved. What matters is that both
land before `pump`, which applies them in its own fixed order. One fewer ordering for a caller to get
wrong.

---

## 2. Failure modes pinned, with the mutation each was proved against

Every row was run: edit → `touch` → *"Compiling"* observed → the **named** assertion failed → revert →
green. No mutation is recorded from reasoning.

| # | Mutation | Result | Named failure |
|---|---|---|---|
| M1 | `Observe`-wrap the hosted sink (`brk` → `Observe<Option<BreakStop>>` through all four call sites) | **RED** — `hosted` 8/2, frontend 1/1 | *"the stop is not AT the breakpoint … it drops only `stop_requested`"* — `left 0x00000210`, `right 0x0000020E` |
| M2 | Swap the two applies in `Host::pump` so the halt lands first | **RED** — `host::tests` 12/1 | *"the un-pause landed after the halt and put `free_run` back — the window paused and instantly resumed"* |
| M3 | `halt_on_breakpoint` clears only `running`, not `free_run` | **RED** — `hosted` 8/2 | the `timeoutReached` assertion (`left true`, `right false`) |
| M3b | `pending_break` **peeked** instead of taken | **RED** | *"one halt must announce itself once"* — `left 25`, `right 1` |
| M4 | Apply the halt eagerly inside `record_break` | **RED** | the `timeoutReached` assertion — see §4, this breaks *two* things at once |
| M4b | Keep the latch, apply it **before** `swap_system` | **RED** | *"stamped from the placeholder System"* — `left 0`, `right 2688252`, event `{"frame":0,"mclk":0,"pc":"0x00000000"}` |
| M5 | Pass `0` instead of the real PC to `run_sinks` | **RED** | *"re-broke at its own resume PC without retiring an instruction"* — `1792182 -> 1792182` |
| M6 | The loop never calls `record_break` | **RED** — `hosted` 8/2 | the `timeoutReached` assertion |

**Two findings from the mutation pass that changed the tests**, recorded because they are the point of
running them rather than reasoning about them:

1. **M3 and M4 both failed on `timeoutReached`, whose message named only one of the three causes that
   produce that reply.** A future reader hitting M3 would have been told the halt did not ride the loop,
   when in fact it rode and cleared the wrong flag. The message now names both.
2. **The exactly-one-stop assertion was very nearly a bystander.** On M3 the wait fails first, so the
   count never runs; on M6 likewise. It is **not** hollow — M3b reaches it and it fires as the named
   assertion at 25 vs 1 — but that was checked rather than assumed, and the message was rewritten to
   describe the cause it actually guards. The D11 stamp assertion was also moved **before** `pc`, because
   M1 and M4b otherwise both land on `pc` and the placeholder defect gets reported as an `Observe` defect.

**One assertion I wrote was wrong and the fixture caught it.** The ordering test originally asserted that
a *second* `set_paused(false)` must leave the machine paused. It must not — that is a human pressing
un-pause again, which is a legitimate resume. Corrected to what the player really does: mirror
`is_paused()` back.

### The fixtures

* `crates/oracle-aether/tests/hosted.rs` — three tests over a **real socket** against the threaded
  miniature player, which now runs `main.rs`'s sink expression verbatim in shape. This is the consumer's
  view: the same `breakpoint_add` → `wait_for_break` an `aeon` tool sends.
* `crates/oracle-aether/src/host.rs` — the ordering collision and the latch-not-overwritten unit, both
  white-box because neither is reachable from the wire.
* `crates/oracle-frontend/src/bus.rs` — the frontend's own seam, so `bus.rs`/`bus_stub.rs` parity is
  exercised rather than asserted.

**`Client` in `hosted.rs` no longer discards notifications.** It did, which is fine when the tests are
about replies — but a stop is an *event*, it can arrive before the reply that provoked it, and discarding
it makes "announced once" and "announced 374,011 times" look identical.

### Derivation of the fixture addresses

`HOT_PC = 0x0000020E` is **not** copied from `tests/breakpoints.rs`. Every test that uses it first reads
the opcode back out of the ROM image the fixture actually loads and asserts it is `0x3010`
(`move.w (A0),D0`, `testrom.rs`'s documented *"$00020E inner:"*), so the address is anchored to the
instruction and a ROM change breaks the fixture loudly instead of silently arming a dead address.
`COLD_PC` is `oracle_core::testrom::TRAP_HANDLER_ADDR`, the core's own public constant.

---

## 3. Gates

| gate | command | result |
|---|---|---|
| format | `cargo fmt --all --check` | **exit 0**, no diff |
| lint (cached) | `cargo clippy --workspace --all-targets` | **exit 0**, 0 warnings, 0 errors |
| lint (genuine rebuild) | `cargo clean -p oracle-core -p oracle-aether` (*"Removed 716 files, 161.8MiB"*) then the same clippy | **exit 0**, 0 warnings; `Checking oracle-core` + `Compiling oracle-aether` both present in the log, so it really rebuilt |
| tests | `cargo test --workspace --no-fail-fast` | **LEGS 58 · PASSED 1940 · FAILED 0 · IGNORED 6 · exit 0** |
| stub build | `cargo build -p oracle-frontend --no-default-features --all-targets` | **exit 0** |
| stub lint | `cargo clippy -p oracle-frontend --no-default-features --all-targets` | **exit 0**, 0 warnings. Run because the workspace clippy above covers only the default feature set, so a `pub fn` with no caller in `bus_stub.rs` — which this repo treats as a hard error in a bin-only crate — would not have shown up there. |
| stub tests | `cargo test -p oracle-frontend --no-default-features --bin oracle-frontend` | **247 passed, 0 failed, 1 ignored, exit 0** |
| currency | `git diff main...HEAD --name-only -- crates/oracle-core/` | **0 files.** Not one file under `crates/oracle-core/` was touched, let alone `crates/oracle-core/tests/`. No golden was regenerated. |

### The baseline, and a worktree trap worth writing down

The dispatch's baseline was **LEGS 58 · PASSED 1934 · FAILED 0 · IGNORED 6**, and re-deriving it on the
merge base with a clean tree first produced **LEGS 58 · PASSED 1926 · FAILED 8 · exit 101**.

The eight failures were real and were not mine — they were `save_state::tests::*` panicking with

> *vendored test ROM …/vendor/TestRoms/m68k_memory_test.bin is missing … (or symlink vendor/ into this
> worktree); these tests must not skip silently*

**A fresh worktree has no `vendor/`.** `/vendor` is gitignored, so `ln -s <repo>/vendor <worktree>/vendor`
fixes it and leaves the tree clean. With the symlink in place the merge base re-derives **exactly** the
dispatch's number, exit 0. The controller's figure was right; the worktree was not.

Worth keeping because the shape recurs: this repo's memory records the same class as *"fresh worktrees
need a `vendor` symlink or conformance rows silently **SKIP**"*. Here they did **not** skip — `save_state`
panics with a message naming the fix, which is why this took one command to diagnose instead of being
mistaken for a regression. That is the good design working.

### The delta, leg by leg

**+6 passed, 0 failed, legs unchanged at 58.**

Accounted by diffing the two runs leg for leg (`Running <target>` line paired with its `test result:`
row), not by reading the totals:

| leg | baseline | now | Δ | what |
|---|---|---|---|---|
| `oracle-aether` `unittests src/lib.rs` | 56 | 58 | **+2** | `host::tests::a_halt_outranks_an_unpause_queued_in_the_same_iteration`, `host::tests::an_unapplied_halt_is_not_overwritten_by_a_later_one` |
| `oracle-aether` `tests/hosted.rs` | 7 | 10 | **+3** | `a_breakpoint_halts_the_playing_window_exactly_once`, `a_halted_window_resumes_past_its_own_breakpoint`, `a_breakpoint_the_rom_never_reaches_does_not_halt_the_window` |
| `oracle-frontend` `unittests src/main.rs` | 279 | 280 | **+1** | `bus::tests::a_breakpoint_a_client_armed_halts_the_players_loop` |
| every other leg | — | — | **0** | not one other leg's count moved in either direction |

---

## 4. What is open, and what I got wrong

**M4 does not isolate what it was built to isolate, and the reason is instructive.** Applying the halt
eagerly inside `record_break` fails on `timeoutReached`, not on the stamp — because the eager apply also
loses the ordering: it clears `free_run` outside the window, then the player's still-un-paused
`set_paused(false)` queues `pending_free_run = Some(true)` and the next drain resurrects it. **The eager
apply breaks the stamp and the ordering at once.** M4b (apply before `swap_system`) is the mutation that
isolates the stamp, and it is the one the D11 evidence rests on.

**M2 is guarded by exactly one test, and I checked that rather than assuming it.** Under the swapped
ordering the whole `hosted` wire suite stays **green, 10/10** — the collision needs a local un-pause,
which no client can drive. If `host::tests::a_halt_outranks_an_unpause_queued_in_the_same_iteration` is
ever deleted as redundant, the ordering rule becomes unguarded.

**M1 is not caught by the ordering test either** (`host::tests` stayed 13/13 green under it). The two
tests guard different things and neither is a superset of the other.

### TAGGED ⟨RUNTIME⟩ for foreground follow-up

1. **A live reproduction against the owner's actual player window.** Everything here is an in-process
   fixture whose loop *mirrors* `main.rs`; nothing executed `main.rs`'s own loop. The mirror is faithful
   in shape and ordering and is checked by the frontend seam test, but the real loop is a `while` in a
   binary with a window, and no test drives it.
2. **The `--no-default-features` player.** It builds (gate above) and its `run_sinks` can only answer
   `None`, so there is nothing to halt — but nothing exercises that at runtime either.
3. **The audio build's sink chain.** The `#[cfg(feature = "audio")]` branch is compiled by the gates and
   the sink sits in a plain `Fanout` outside `AudioAndWatch`, but no test runs a frame through the audio
   composite with a breakpoint armed (there is no audio device here).

### Still open from the recon, unchanged by this parcel

* **Whether the legacy C++ `oracle_gui` has the same gap.** The recon named it as the single question
  most likely to change the framing, and it is unsettleable without a running machine. Closing our gap
  does not settle it — but it does make it cheaper to care about, since we no longer have anything to
  declare.
* **The `timeout_ms` → `timeoutMs` sequencing warning in `docs/2026-08-27-breakpoints.md` §7.** That
  warning was about the window in which fixing the spelling would convert a loud refusal into a fall into
  this gap. **That window is now shut** — the gap is gone, so `aeon`'s deferred rename is safe whenever
  they take it. §7 has not been edited to say so; a follow-up should either amend it or let this document
  stand as the answer.

### Nothing was BLOCKED

The zero-contract-surface constraint never bound: the halt reuses `emulator/stopped` and
`reason: "breakpoint"`, both of which already existed, and no point in the design wanted a new key. The
`Observe` prohibition and the ordering constraint both had in-tree precedents to copy, and the ordering
was made safe rather than deferred.
