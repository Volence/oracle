# `fault_run` — the emulator half of Aeon's replay net (2026-08-16)

Ranked item 3, after CR-19 established that the item had been ranked for the wrong reason. The owner chose
to chase the runner rather than build the pad timeline.

## The finding that made this cheap

Aeon's replay net is fully built and completely dead: `engine/system/replay.emp` plays an `ARP0` input
stream, recomputes an address-free hash of the player's state every 64 ticks, and raises `REPLAY DESYNC` on
mismatch — while Aeon's own `DEFERRED_WORK.md` records that *"it cannot detect a desync — that needs the
emulator."*

The recon bundled this with "deterministic scripted input" and ranked the pair as the highest-leverage
engine-facing work. **The two halves are unrelated.** In `INPUT_PLAYBACK` the **engine plays its own
stream** from a blob in the ROM: `Input_Tick` fetches `(buttons, hold−1)` RLE pairs and overwrites
`Ctrl_1_Held`/`Ctrl_1_Press` itself, deriving presses from *stream* history rather than the live pad —
deliberately, to kill an input-bleed desync class structurally.

**So the emulator injects nothing, and a pad timeline would be inert during a replay run.** The whole job
is noticing that the machine reached its fault handler and reporting the registers the trap carries. That
needed **no new emulator capability** — only a runner around `System::run_until_stop`, which the P0 slice
already shipped.

## What landed

`crates/oracle-core/examples/fault_run.rs`:

```text
cargo run --release --example fault_run -- <rom> [--symbols P] [--symbol NAME] [--addr HEX] [--max-frames N]
```

- Resolves the fault handler **before running**, from the `.lst` beside the ROM, refusing a listing that
  does not bind to the image — the server's policy, for the server's reason.
- `run_until_stop(max_frames, |pc, _| pc == target)`; the predicate sees each PC *before* it commits, so
  the stop lands on the handler's first instruction with the fault state intact.
- Prints `d0`/`d1`/`d2` on a fault (Aeon's desync convention: actual hash, `Logic_Tick`, expected).
- **Exit codes are the gate:** `0` clean, `1` faulted, `2` setup error. The third is not a nicety — a
  runner that exits 0 because it could not resolve its target is a green gate that tests nothing.

Deliberately **not** an Aether client. The same recon records that *"a hang in the debug transport
destroyed irreplaceable evidence"* — a frozen repro frame lost to a control-socket hang and impossible to
re-freeze. A CI gate is the last place that may happen, so this drives `oracle-core` directly: no socket,
no server, no second process. `motion_run.rs` is the precedent.

### The default symbol is the one the spike got wrong

`raise_exception` does **not** route through `ErrorTrap`. `ErrorTrap` is a proc
(`engine/debug/error_handler.emp:186`) that handles the TRAP 0–15 and reserved *vectors*
(`vectors.emp:135–140`) and raises `"ERROR TRAP"` of its own; `raise_exception` reaches the vendored MD
Debugger blob, *"reached only via `jsr (MDDBG__ErrorHandler).l`"* — i.e. **`ErrorHandlerBlob`**, `$A217A`,
exact in `s4.debug.lst`. The first draft of the CR named `ErrorTrap`, which would have produced a runner
that never fires for a desync. That is why the default is what it is, and why it carries a comment saying
so.

## ★ The positive control, and why it exists

**The first version of the fixture passed its own test while proving nothing, twice.**

A fault-watching runner tested only against ROMs that do not fault demonstrates that the watch is *silent*,
not that it *works*. So `testrom.rs` gained `build_trap_on_frame(n)`: the stock ROM plus a VInt handler
that counts frames and executes an `ILLEGAL` on the nth, vectoring to the illegal handler exactly as an
engine's `raise_exception` reaches its own. `build()` stays byte-identical — the golden fixture depends on
it — as `build_vint_counter` already established.

It took two goes to make a ROM that really faults, and both failures were invisible without the control:

1. **Work RAM comes up seeded, not zeroed** (`System::new` takes a fill seed), so an increment-and-compare
   counter started from garbage and never reached `n`. Fixed by zeroing it in an init stub.
2. **`build()` never enables the VDP's VInt** — its own VInt test arms IE0 from *outside*. The handler the
   fixture depended on was never called. Fixed by having the fixture arm IE0 itself.

Both are now the test's mutation checks, and each kills it:

| mutation | result |
|---|---|
| IE0 enable removed (VInt never fires) | 1 failed |
| counter zeroing removed (RAM is seeded) | 1 failed |

Measured end to end, all three exit paths:

| case | output | exit |
|---|---|---|
| ROM built to fault on frame 3 | `FAULT at frame 2`, `d0 0x00000003` | **1** |
| stock ROM, same watch | `CLEAN — 60 frames` | **0** |
| unresolvable symbol | `no symbol named NoSuchSymbol` | **2** |

(Frame 2 is correct: frame indices are 0-based and the third VInt lands in frame index 2. `d0` carries the
counter that tripped it — the register a real handler would report.)

## What is still open, and it is not ours

**No real desync has been observed.** `s4.debug.bin` arms no `ARP0` stream, and with no register-write op
(deliberately dead) there is no way from here to force one. What is established: the target resolves
exactly, a clean 900-frame boot reaches neither handler symbol and stays live in `Render_Sprites`, and a
ROM that genuinely faults is caught, located, and reported with its registers.

**The remaining link is an Aeon-side ask:** a build with a stream armed, run under `fault_run`. At that
point the dead regression net is a CI gate, and the gate is one command.

## Suggested Aeon usage

```sh
cargo run --release --example fault_run -- s4.debug.bin --max-frames 3600 || exit 1
```

`--symbols` defaults to the `.lst` beside the ROM; `--symbol` defaults to `ErrorHandlerBlob`. A non-zero
exit is the failure, and the stdout block is the evidence.
