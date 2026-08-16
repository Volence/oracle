# CR-19 — a pad timeline, and two claims in its ranking that do not survive checking

**Status: proposed, unruled.** Ranked item 3 of `docs/2026-08-15-handoff-conformance-and-item19.md` §7,
inherited from `docs/2026-08-14-tooling-frontier-recon.md` §6 item 3 and §1c.

The capability is worth building. **Two of the three claims that rank it are wrong**, and both change what
gets built — so they come first, before any design that would inherit them.

## ☠ Correction 1: the ARP0 replay runner needs no pad capability at all

The recon bundles this as one item: *"Deterministic scripted input → headless `ARP0` replay runner — NEW
glue over two built halves"*, and calls it *"the highest-leverage engine-facing item we found."*

**MEASURED, by reading `aeon/engine/system/replay.emp`:** in `INPUT_PLAYBACK` mode the **engine plays its
own stream**, from an `ARP0` blob embedded in the ROM. `Input_Tick` fetches `(buttons_byte, hold_minus_1)`
RLE pairs from `Replay_Ptr` and overwrites `Ctrl_1_Held` / `Ctrl_1_Press` itself, deriving presses from
**stream** history rather than the live pad — deliberately, to kill the S1-REV00/S2 input-bleed desync
class structurally.

So the emulator injects nothing. **A pad timeline would be inert during a replay run**, and any design that
justifies itself by "it drives the ARP0 net" is justifying the wrong instrument.

What the runner actually needs is to notice that the engine raised `REPLAY DESYNC` and to read the three
registers the trap carries. **MEASURED:** the desync path is `raise_exception "REPLAY DESYNC"` with `d0` =
actual hash, `d2` = expected, `d1` = `Logic_Tick` (`replay.emp`, the `.desync` label, DEBUG builds only).

### The spike, run — and the symbol this document first named was the wrong one

**`ErrorTrap` is not the common entry.** It is a `proc` in `engine/debug/error_handler.emp:186` that
handles the TRAP 0–15 and reserved *vectors* (`vectors.emp:135–140`) and raises `"ERROR TRAP"` itself.
`raise_exception` routes to the vendored MD Debugger blob, *"reached only via `jsr
(MDDBG__ErrorHandler).l`"* — i.e. **`ErrorHandlerBlob`**. An earlier draft of this correction named
`ErrorTrap`, which would have watched the wrong address and produced a runner that never fires.

Run firsthand against `s4.debug.bin` + `s4.debug.lst` (2,540 symbols, bound to the image), on the server
as it ships today:

| check | result |
|---|---|
| `lookup_symbol ErrorHandlerBlob` | `0x000A217A`, exact |
| `lookup_symbol ErrorTrap` | `0x000A2162`, exact |
| **negative control** — clean 900-frame boot, `run_to ErrorHandlerBlob` | `reached: false`, machine live in `Render_Sprites.band_loop` |

**So the mechanism is `load_symbols` + `run_to {symbol: "ErrorHandlerBlob"}` + `registers`: three methods
we already ship, zero new surface, for the half the recon called highest-leverage.**

**What the spike did NOT prove, stated so nobody cites it as more than it is:** no desync was observed.
This build arms no `ARP0` stream, and with no register-write op (deliberately dead) there is no way from
the bus to force the trap. What is established is that the target resolves, that a clean run does not
reach it, and that `run_to` stops exactly on a symbol when the PC arrives (`EntryPoint`, `reached: true`,
`symbolDisp: 0`). The remaining link — that a real desync lands there — needs an engine-side build with a
stream, which is an Aeon-side ask, not a bus capability.

## ☠ Correction 2: one in-tree re-implementation, not six

The handoff says the capability *"retires six in-tree re-implementations."* **MEASURED**, by reading every
`set_pad` site in the tree:

| site | what it does | a timeline? |
|---|---|---|
| `examples/motion_run.rs` | per-frame scripted pad from a parsed script, both ports, `run_frames(1)` in a loop | **YES** |
| `examples/s3k_sram_probe.rs` | one pad, then N frames | no — `hold` + `run_frames` |
| `examples/pad_probe.rs` | one pad, 3 frames | no |
| `examples/k4_openbus_probe.rs`, `examples/testrom_probe.rs` | fixture state | no |
| `tests/io_controllers.rs`, `tests/conformance_roms.rs` | fixture state | no |
| `src/io.rs`, `src/system.rs`, `src/bus.rs` | the API itself and its tests | no |

**One** site hand-rolls a timeline. The rest inject a single pad state, which the existing `hold` +
`run_frames` already expresses on the wire. The capability's real executed evidence is *narrower and
better* than the claim: it is `motion_run.rs`, a dev tool that had to parse its own script format because
the bus has no way to say "hold right from frame 60 to 360."

**What survives, and is enough:** pad input is the largest executed-usage signal in the corpus (52 of ~90
real calls), and the one in-tree timeline exists because the bus cannot express one.

## The gap, stated precisely

`press{buttons, frames}` holds a set and releases it. `hold{buttons, down}` sets named buttons and leaves
the rest alone. Between them a client can express any sequence — **in one call per change**, with the
machine's pad state carried across calls as accumulated mutable state.

Three things that costs:

1. **No artifact.** A reproduction is a sequence of calls in someone's scrollback, not a file. `motion_run`
   has a checkable script; the bus has nothing to check in.
2. **Accumulation, not declaration.** `hold` mutates a set that persists. Two clients, or one client and a
   forgotten `hold`, and the pad at frame N depends on call history rather than on the script. This is the
   sibling's measured failure — *"`hold` ADDS, it does not replace"*, *"re-recording is impossible"*, a
   re-stamp costing ~7 manual playthroughs.
3. **No overlap.** "Hold right for 300 frames and tap A at frame 120" needs four calls and cannot be
   expressed as one intention at all.

## Proposed: `emulator/play_input`

**One row.** §6's *run control* table, beside `press`:

| Method | params | result |
|---|---|---|
| `emulator/play_input` | `rows[]{start,end,buttons,port?}`, `maxFrames`? | `frames`, `stoppedAt`, `reason`, `rowsApplied`, `ports[]` |

### ★ The one normative property, from which the rest follows

**The pad at frame N is a pure function of `rows`, and of nothing else.** Not of previous `hold` calls, not
of the pad's state when the call began, not of call order. A server MUST compute each frame's pad from the
timeline alone and MUST NOT union it with the client's held set.

That is the property that kills the desync class, and it is why this is a *timeline* rather than a
convenience wrapper over `hold`. It is also the property a second implementation is most likely to get
wrong, because "apply the rows on top of what's already held" is the easier implementation.

*Interaction with `hold`, pinned:* the client's held set is **suspended** for the duration and **restored**
after, unchanged. Not cleared — clearing would make `play_input` a destructive operation on state it does
not own, which is the `release_all` reasoning (*"a button the human is physically holding is not the bus's
to release"*) applied one method over.

### Intervals, not steps — and this is the executed shape

`rows` are half-open `[start, end)` frame intervals relative to the call, each naming a **complete** button
set for one port. This is `motion_run.rs`'s format (`START END BUTTONS [PORT]`, `end > start` enforced),
which is the only such format in the tree that has actually been used.

**Overlapping rows on one port UNION.** `pad_for` does exactly this (`|=` per button), and it is what makes
"hold right, tap A at 120" a two-row script instead of a hand-computed sequence of disjoint states. The
alternative — later row wins — is defensible and MUST be ruled explicitly either way, because it is a place
two conformant servers would silently differ.

*Why not RLE steps* (`[{buttons, frames}, …]`, ARP0's own shape): consecutive runs cannot express overlap
without pre-flattening, which pushes the composition work onto every client. And the ARP0 argument for
matching that shape is **Correction 1** — it does not apply.

### Bounds, and what a client gets back

- `rows` is bounded (proposed 256) and each `port` is 0 or 1. Rows need not be sorted or disjoint.
- `maxFrames` bounds the run; absent, it is the largest `end`. It shares `press`'s ceiling for the reason
  `press` has one: hosted, a long tap freezes the player's window and its OS event pump.
- `frames` is **exact, including zero** (CR-17), because a watch with `stopAfter` can end the run inside
  its first frame.
- `reason` names the **stop condition**, not the method (§11.7 / CR-9): `runFrames` when the timeline ran
  to its end, `watchpoint` when a watch stopped it.
- `stoppedAt` is the frame index within the call where it ended — the field that tells a client which row
  was in effect. **Open:** whether this duplicates `frames` and is therefore CR-13 bait. It does when the
  run completes and does not when a watch cuts it short; that may still be one field too many.

### Behaviours to pin

1. **Run control**, unlike CR-18: it advances the machine, so §6's run-control state rule applies and it is
   `-32005` on a free-running machine, exactly as `press` is.
2. **Determinism is the promise.** The same `rows` from the same machine state MUST produce the same frames.
   This is a statement the schema cannot express and the prose must therefore make.
3. **A row naming a port with no pad is `-32602`**, not silently dropped (ports 0/1 only; EXP has none).
4. **3-button only.** Core's `Pad` has exactly `up/down/left/right/a/b/c/start`; a 6-button pad is on the
   accuracy backlog and unbuilt. `buttons` MUST refuse `x`/`y`/`z`/`mode` rather than accept and ignore
   them — the MCP's tool description currently *promises* them ("plus x, y, z, mode on a 6-button pad"),
   which is a client documenting a capability no server has.

## Cost

Schema 27 → 28 fragments; advertised 26 → 27. The handler is a loop over `run_frames(1)` with a computed
pad, which is `motion_run.rs`'s body. No core change: `System::set_pad` is the sole input path and already
public.

## ☐ Unruled questions

1. **Should this exist at all before the spike in Correction 1?** If `run_to ErrorTrap` closes the replay
   runner, the strongest stated motivation for this capability evaporates and what remains is
   `motion_run.rs` plus ergonomics. That is a real case but a smaller one, and the honest ranking may drop.
2. **Union vs later-row-wins** on overlap (above).
3. **`stoppedAt`** — worth its key, or CR-13 bait?
4. **Does `press` survive?** `play_input` subsumes it (`[{start:0, end:n, buttons}]`). Keeping both is a
   two-spellings-one-meaning smell; removing `press` breaks the single most-executed call in the corpus.
   Recommended: keep `press`, and say in its row that it is the one-row spelling — but this is exactly the
   kind of overlap the register should rule rather than let accumulate.
