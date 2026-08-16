# CR-19 — a pad timeline, and two claims in its ranking that do not survive checking

**Status: RULED 2026-08-16 — adopt with ten changes, all applied below.** Ruling recorded in
`docs/2026-08-16-ruling-cr19.md`. Ranked item 3 of `docs/2026-08-15-handoff-conformance-and-item19.md` §7,
inherited from `docs/2026-08-14-tooling-frontier-recon.md` §6 item 3 and §1c.

> **What the adjudication found, kept rather than quietly folded in.** Both of this CR's headline
> corrections were **verified and right about what they checked — and each stopped exactly one step
> short.** One examined the replay net's *playback* path while the evidence it cited lives in the
> *recording* path; the other objected to a promise in the MCP while the same promise sat unfixed in this
> contract. It also struck three of five proposed result keys under CR-13, aimed the row at the wrong §6
> table, and found the purity pin one input source short of its own property. **Checking a claim is not
> the same as checking its scope.**

## ☠ Correction 1: the ARP0 replay runner needs no pad capability — *on the playback side*

The recon bundles this as one item: *"Deterministic scripted input → headless `ARP0` replay runner — NEW
glue over two built halves"*, and calls it *"the highest-leverage engine-facing item we found."*

**MEASURED**, by reading `aeon/engine/system/replay.emp`: in `INPUT_PLAYBACK` the **engine plays its own
stream**, from an `ARP0` blob embedded in the ROM. `Input_Tick` fetches `(buttons_byte, hold_minus_1)` RLE
pairs from `Replay_Ptr` and overwrites `Ctrl_1_Held` / `Ctrl_1_Press` itself, deriving presses from
**stream** history rather than the live pad — deliberately, to kill the S1-REV00/S2 input-bleed desync
class structurally.

**But "a pad timeline would be inert during a replay run" is overstated, and the qualifier matters.** The
playback path reads the **live** `Ctrl_1_Press` Start bit *before* overwriting and sets
`Replay_Exit_Request`. Injected Start is not inert — **it aborts the replay.** Injection cannot help a
playback run and can actively break it; that is the true statement.

### ★ And the pain this CR cites as its evidence is in the *recording* half

`replay.emp` has a third mode. **`INPUT_RECORD` taps the *latched live* pad** into the record ring — and
outside playback, that latched pad is exactly what the emulator's `set_pad` drives. The sibling failures
quoted to rank this work — *"`emulator_hold` fails ~50% of the time"*, *"re-recording is impossible"*, a
re-stamp costing ~7 manual playthroughs — are **recording-side**.

So the ARP0 case does not evaporate. It **moves** from the half this correction examined to the half it
did not, and a deterministic timeline is precisely the re-record instrument.

### The playback-verification half, spiked

What that half needs is to notice the engine raised `REPLAY DESYNC` and read the registers it carries:
`d0` = actual hash, `d2` = expected, `d1` = `Logic_Tick` (`replay.emp`, `.desync`, DEBUG builds only).

**`ErrorTrap` is not the common entry.** It is a `proc` (`engine/debug/error_handler.emp:186`) handling the
TRAP 0–15 and reserved *vectors* (`vectors.emp:135–142`), raising `"ERROR TRAP"` itself. `raise_exception`
reaches the vendored MD Debugger blob — `MDDBG__ErrorHandler = extern("ErrorHandlerBlob")`
(`error_handler.emp:84`). An earlier draft named `ErrorTrap` and would have produced a runner that never
fires.

| check (firsthand, against `s4.debug.bin` + its listing) | result |
|---|---|
| `lookup_symbol ErrorHandlerBlob` | `0x000A217A`, exact |
| **negative control** — clean 900-frame boot, `run_to ErrorHandlerBlob` | `reached: false`, live in `Render_Sprites.band_loop` |

**That half is `load_symbols` + `run_to` + `registers`: three shipped methods, zero new surface.** It has
since shipped as `examples/fault_run.rs` — see `docs/2026-08-16-fault-run-replay-gate.md`.

## ☠ Correction 2: one in-tree re-implementation, not six

The handoff says the capability *"retires six in-tree re-implementations."* **MEASURED**, by reading every
`set_pad` site — and verified independently, row for row:

| site | what it does | a timeline? |
|---|---|---|
| `examples/motion_run.rs:266-267` | per-frame scripted pad from a parsed script, both ports | **YES** |
| `examples/s3k_sram_probe.rs:23`, `pad_probe.rs:35` | one pad, then N frames | no — `hold` + `run_frames` |
| `examples/testrom_probe.rs:50-52`, `k4_openbus_probe.rs:245-254` | set at frame N, clear at N+len | no — `press`-shaped |
| `tests/*`, `src/io.rs`, `src/system.rs`, `src/bus.rs` | fixture state and the API itself | no |
| `frontend/main.rs`, `frontend/gamepad.rs` | live host input | no — not a timeline |

**One** site hand-rolls a timeline, and it exists because the bus cannot say "hold right from frame 60 to
360."

## The row: `emulator/play_input`

**§6's *input* table, beside `press`** — *not* run control, where an earlier draft aimed it. `press`'s row
lives in the input table; run control merely *names* the methods its state rule binds. **`play_input` is
added to that named list**, because the rule is an explicit enumeration and an unnamed method in it is the
"one server refuses, another accepts, both conforming" hole the rule's own prose warns about.

| Method | params | result |
|---|---|---|
| `emulator/play_input` | `rows[]{start,end,buttons,port?}`, `maxFrames`? | `frames`, `frameToken`, `pc` |

The result is `run_frames`' own shape, which is what this method is. **Four proposed keys were struck:**

| proposed | why struck |
|---|---|
| `reason` | §11.7 cited backwards — the house rule puts the stop condition on the **`stopped` event**; no catalogued method carries `reason` as a result key |
| `stoppedAt` | CR-17 made `frames` exact *precisely* in the watch-cut case, so the stop position is `frames` in **every** case. This is §11.5's struck `run_to.stoppedAtFrame`, re-proposed |
| `rowsApplied`, `ports[]` | pure functions of `rows` + `frames` — the offence §11.10 struck a per-entry `parsed` flag for |

### ★ The one normative property, from which the rest follows

**The pad at frame N is a pure function of `rows`, and of nothing else.** A server MUST compute each
frame's pad from the timeline alone and **MUST NOT merge any other input source** — the client's held set
and the host's live input alike.

Naming both sources is the correction: the engine merges **two** non-row sources (`held[]`, and `live[]`
for a human's physical pad), and a pin naming only the first would let a hosted server union `live` and
argue conformance from the letter.

- **Both are suspended for the duration and restored afterwards, unchanged** — not cleared. Clearing would
  make this destructive to state it does not own, which is `release_all`'s reasoning (*"a button the human
  is physically holding is not the bus's to release"*) one method over. It hides nothing: the held set is
  client-authored and re-observable via `hold` after the call.
- **A port no row covers is fully released** for every frame of the run. "Pure function of rows" implies
  it; leaving it unsaid is where two servers differ.
- **Application point:** the pad computed for frame *i* is applied at the frame boundary **before** frame
  *i* runs. Indices are 0-based and relative to the call.

This property is also what makes `play_input` a different method from `press` rather than a longer
spelling of it — see below.

### Intervals, and union

`rows` are half-open `[start, end)` frame intervals relative to the call, each naming **the buttons that
row contributes** for one port. This is `motion_run.rs`'s executed format (`START END BUTTONS [PORT]`,
`end > start` enforced, half-open, union via `|=`).

**Overlapping rows on one port UNION**, normatively. It is the executed shape, it is what makes "hold
right, tap A at 120" a two-row script — and, decisive for the contract, it is **order-independent**: the
pad depends on the row *set*, not the row order. That extends the purity property naturally and is why
"rows need not be sorted" costs nothing. *Later-row-wins was rejected* precisely because it would make row
order load-bearing and reintroduce a silent divergence.

*Why not RLE steps* (ARP0's own shape): consecutive runs cannot express overlap without pre-flattening,
which pushes composition onto every client — and Correction 1 removes the argument for matching ARP0's
shape on the playback side.

### Bounds

- `rows` bounded at 256; `port` is 0 or 1. Rows need not be sorted or disjoint. **The bound is
  discoverable** via `initialize.limits.maxInputRows` — a client that must hit a limit to learn it loses
  the work it was doing when it found out.
- **`maxFrames`'s ceiling is `max_run_frames`**, `run_frames`' own — *not* `press`'s legacy 1000. That
  1000 is a compatibility floor press always had; 1000 frames is ≈16.7 s, far too short for the re-record
  workflow that is now this capability's strongest case. Absent, `maxFrames` is the largest `end`, capped
  at the ceiling. **A `maxFrames` below the largest `end` truncates**, and rows starting at or beyond it
  never apply.
- `frames` is **exact, including zero** (CR-17): a watch with `stopAfter` can end the run inside frame 0.

### Behaviours pinned

1. **Run control.** It advances the machine, so §6's run-control state rule applies: `-32005` with
   `data.reason: "machineRunning"` on a free-running machine, exactly as `press` is.
2. **Events.** One `resumed` at the start and one `stopped` at the end — **never one per frame**.
   `reason: "runFrames"` when the timeline ran out (§11.7's redefinition covers a bounded frame advance
   whose stop condition is an exhausted count), or `"watchpoint"` with `watch` when a watch cut it short.
   A timeline-driven stop carries the pad in effect at the stop frame per driven port, on the same
   `dependentRequired` machinery §11.7 gave `buttons`/`port` — ruled explicitly, because silence here is
   where two servers differ.
3. **Determinism is the promise.** The same `rows` from the same machine state MUST produce the same
   frames. The schema cannot express this; the prose must.
4. **D12 does not apply.** This is not wait-shaped — its stop condition is an exhausted count, not a
   predicate — so there is no `reached`. Stated, or a second implementer will add one.
5. **Errors**: `end <= start` in a row, an empty `rows`, `rows` over the bound, and a `port` other than
   0/1 are each `-32602`. An empty timeline is a request to do nothing and is refused rather than treated
   as a no-op.

### ★ `press` survives, on semantics — the "subsumes" claim was false

An earlier draft offered to keep `press` for compatibility and called the overlap a smell. **There is no
overlap.** `press` **unions** its buttons with the live and held sets by design; `play_input` suspends
both. With Right held, `press{buttons:["a"]}` taps Right+A, while the one-row timeline taps A alone. They
are different semantics, not two spellings. Both rows gain one sentence naming the difference: *press
composes with held and live input; play_input replaces it.*

### ★ The 6-button reconciliation, which this CR first aimed at the wrong target

Core's `Pad` is 3-button (`up/down/left/right/a/b/c/start`). An earlier draft pinned only that the **MCP**
promises `x`/`y`/`z`/`mode` it cannot deliver. That is true and is the *smallest* of three sites:

- **The contract's own §6 `press` row and the schema's `press.buttons` enum both list `x`/`y`/`z`/`mode`
  as legal**, while
- **the reference server refuses them with `-32602`**, justified by a `sixButtonPad: false` capability
  that **appears nowhere in the contract or schema**.

The server rejects a parameter its own normative schema accepts, on the strength of a capability key it
invented — a live divergence plus a CR-13-class invention, in a shipped method. **Pin 4 is therefore
adopted only together with:** striking `x/y/z/mode` from `press`'s row and the schema enum (or gating them
on a registered capability), **registering `capabilities.sixButtonPad`**, and fixing the MCP description.
One button vocabulary, defined once, shared by `press` / `hold` / `play_input`.

## Cost, and the adoption condition

Schema **27 → 28** fragments; advertised **26 → 27**. No core change: `System::set_pad` is public and the
sole input path.

**Implementation anchor:** the handler loops `Engine::advance(1)`, **not** bare `run_frames(1)` — the watch
fan-out and CR-17's exact-frames accounting ride `advance`, and a bare-core loop would lose mid-frame
`stopAfter` stops.

**★ ADOPTION IS CONDITIONAL ON THE FRAGMENT BEING EXECUTED**, per §11.6 / §11.8 / §11.10: registered when a
conformant reply passes it **closed** under §8 item 20 — both branches, including a watch-cut reply
carrying `frames: 0` and a completed-timeline reply.
