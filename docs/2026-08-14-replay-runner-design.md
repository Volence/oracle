# The headless replay runner — design (P3, 2026-08-14)

**Status:** design, commissioned from `docs/2026-08-14-tooling-frontier-recon.md` §7 P3 and §1c after two
recon passes (one per repo). Every address, byte layout and ROM-shape claim below was re-verified
firsthand by the overseer against the built artifacts before this document was written.

**What it is.** A headless binary in this repo that boots Aeon's DEBUG ROM, arms its embedded input-replay
stream, runs it to completion, and reports PASS / DESYNC / FAULT / TIMEOUT with an exit code. It turns a
regression net that is *fully built and completely dead* into something a CI gate can run.

Aeon's own ledger states the gap (`aeon/docs/DEFERRED_WORK.md:113-125`):

> **The replay net has NO automated runner — it is invisible to every gate we own.** … Verified: it is not
> a pytest, not a cargo test in sigil, not in `test.sh`, and there is no CI. … it cannot detect a desync —
> **that needs the emulator.** Two candidate fixes, neither scoped: (a) a headless oracle runner invoked
> from `test.sh`, (b) a committed re-stamp tool that makes the manual loop cheap enough to run routinely.

This design delivers (a), and shapes it so that (b) becomes a flag on it rather than a second tool.

---

## 1. Why this is smaller than the handoff implied

The handoff sized P3 as "deterministic scripted input + the headless replay runner". Recon collapsed most
of it:

- **Run-until-PC already exists.** `System::run_until_stop(max_frames, |pc, frame| …)`
  (`crates/oracle-core/src/system.rs:924`) *is* a PC predicate, and `crates/oracle-aether/src/engine.rs:641`
  already drives it from a **symbol-resolved** target. Nothing to build.
- **Atomic arm-at-power-on already exists.** `System::boot_with_sink` (Fable ruling F, item 1) makes
  load-then-reset-then-arm inexpressible. The sibling's "arm the breakpoint BEFORE `reload_rom`" ordering
  hazard (`aeon/docs/superpowers/notes/2026-08-13-replay-net-restamp-ab.md:172`) cannot occur here.
- **Set-not-add pad semantics already exist.** `io.rs:127` is `self.pad[port] = pad;` — a total
  replacement. The sibling's "`hold` ADDS, it does not replace" defect has no analogue in this tree.
- **Scripted input is barely needed for this gate at all.** During `INPUT_PLAYBACK` the seam *overwrites*
  `Ctrl_1_Held`/`Ctrl_1_Press` from the stream, and press edges derive from the stream's previous byte,
  never the live pad (`aeon/engine/system/replay.emp:194-201`). The runner's only pad obligation is
  **negative**: never press Start, because `replay.emp:157-159` reads live `Ctrl_1_Press` before the
  overwrite and sets `Replay_Exit_Request`.

So the two halves of "P3" are genuinely separable, and the runner is the half that pays. The shared
pad-timeline type — which would retire **six** independent re-implementations, not the four the recon
claimed — is real work with its own justification, and it is **not** in this slice.

---

## 2. The contract, re-verified

### 2.1 Resolve every symbol by name. The documented addresses are wrong, and one is a landmine.

Measured against the `s4.debug.lst` in the tree, versus the values in
`aeon/docs/superpowers/plans/2026-08-13-replay-net-restamp.md:89-93`:

| symbol | space | doc value | **current build** |
|---|---|---|---|
| `Input_Source` | RAM u8 | `$FF803A` | `$FF8036` |
| `Replay_Done` | RAM u8 | `$FF803C` | `$FF8038` |
| `Replay_Ptr` | RAM u32 | `$FF8040` | **`$FF803C`** |
| `Logic_Tick` | RAM u32 | `$FF8004` | `$FF8004` |
| `GameState_OJZScroll_Init` | ROM | `$A1734` | `$A1724` |
| `Replay_OJZ_Fixture` | ROM | `$A1DA0` | `$A1D80` |
| `Replay_OJZ_Slide_Fixture` | ROM | `$A1EB0` | `$A1E90` |
| `ErrorHandlerBlob` | ROM | (unaddressed) | `$A213A` |
| `EndOfRom` | ROM | — | `$A3090` |

The cells shifted −4 and re-packed. **The docs' `Replay_Done` address is now `Replay_Ptr`.** A runner that
hardcoded the documented values would poll the top byte of a stream cursor for `$FF`, never see it, and
report a hang on a green run. Addresses appear in this document only as sanity anchors; the runner
resolves all of them by name at runtime and fails loudly on any unresolved name.

> Aeon's own derivation recipe is also broken: `plans/…-restamp.md:175` greps `"^Name "`, but sigil's
> listing rows are ` Name : ADDR C |` — leading space, ` : ` separator. That grep matches zero lines, which
> is plausibly how the stale table survived. Our `symbols.rs` parses the real format.

### 2.2 The fixture, read from the ROM — not from the `.bin` on disk

The fixture is `embed()`ed into the ROM (`aeon/games/sonic4/test/replay_fixture.emp:16`), so **the ROM is
the fixture**. Reading the stream from the resolved `Replay_OJZ_Fixture` symbol makes a fixture-vs-ROM
mismatch structurally impossible, and it costs nothing.

This matters because the header's `core_hash` field **cannot** serve as the build-identity guard its own
docstring advertises. `aeon/docs/superpowers/notes/2026-08-05-objtest-gate-ab.md:59-66` records it as
having **zero consumers**, and the committed value (`0x7054d28b`) is byte-identical in both fixtures
despite their being recorded on different days against different builds. It is stale metadata. Do not
build a guard on it; do not "helpfully" refresh it either — that note's ruling is *"do not hand-refresh a
field nothing reads."*

Header, `REPLAY_HEADER_LEN = 20`, big-endian, confirmed at `$A1D80` in `s4.debug.bin`:

```
41 52 50 30 | 00 | 00 | 00 00 06 b9 | 70 54 d2 8b | 00 00 00 00 | 00 00 | ff 01 1d 37 50 66 | 00 3f
"ARP0"      flags pad   ticks=1721    core_hash     seed=0        resv    ring-0 checkpoint    RLE pair
```

Offsets: magic 0, flags 4, pad 5, tick_count 6, core_hash 10, rng_seed 14, reserved 18, body 20. `pad` and
`reserved` must be 0 (`aeon/tools/replay_pack.py:123-124`). Body opcodes
(`aeon/engine/system/constants.emp:488-492`): `REPLAY_ESCAPE = $FF`, `REPLAY_OP_END = $00`,
`REPLAY_OP_CHECK = $01` (+ BE u32 expected hash). An ordinary pair is `buttons u8 (≠$FF), hold_minus_1 u8`;
run length is `hold_minus_1 + 1`. RLE runs are **split at every checkpoint**, and the packer zero-pads
after the terminator — a decoder must tolerate trailing zeros.

Two fixtures exist, and only two: `ojz_fixture.bin` (272 B, 1721 ticks, 27 checkpoints) and
`ojz_slide_fixture.bin` (336 B, 2350 ticks, 37 checkpoints). They arm identically. The `1496 ticks / 24
checkpoints` in `replay_fixture.emp:27` is a stale source comment; `aeon/tools/test_replay_fixture.py:28-30`
pins the true figures.

### 2.3 Refuse a release ROM — it produces a false green

`replay.emp:174-186` gates the checkpoint compare on `if DEBUG == 1`; the release shape *steps over the
payload without comparing*. A release ROM therefore runs the stream to completion and sets
`Replay_Done = $FF` **having verified nothing**. That is worse than no gate.

Two independent refusals, both cheap:

1. **Shape binding** — `SymbolTable::validate_against_rom` (`symbols.rs:622`), the `deb2` appendix probe.
   Fatal on `Mismatch`; fatal on `Indeterminate` when `!is_intact()` (the truncation fail-open, already
   closed in `symbol_file.rs:26-34`). Note the documented residual: a `Match` means "not obviously wrong",
   never "proven right".
2. **A positive assertion that the compare path exists** — the literal bytes `REPLAY DESYNC` must be
   present in the ROM image. Verified firsthand: `s4.bin` contains neither trap string, `s4.debug.bin`
   contains both. This one is decisive where the shape binding is only suggestive.

### 2.4 The arm

The requirement is exact: **`Input_Source` and `Replay_Ptr` must be written after the Work-RAM clear and
before the first `Input_Tick` runs with `Input_Source ≠ 0`.** `GameLoop` is
`VSync_Wait → addq.l #1,Logic_Tick → jbsr Input_Tick → dispatch` (`aeon/engine/system/game_loop.emp:29-31`),
and `GameState_OJZScroll_Init` is the game's `Game.entry` (`games/sonic4/config/game.emp:24`), i.e. the
one-shot boot init dispatched out of that loop.

**Arm at `PC == GameState_OJZScroll_Init`.** This is the anchor the recorded-green runs used, and the
fixtures were recorded against it — checkpoint alignment is a property of the arm point, so this is not a
free choice. `Input_Tick`'s first hit is a defensible alternative and is deliberately **not** taken: the
arm is the single point of failure for the whole net (all three recorded near-misses were arm failures),
and this is not the place to innovate.

Do **not** clear `Cheat_Flags`. The DEBUG scene arms `CHEAT_DEBUG_FLY` itself
(`games/sonic4/test/ojz_scroll_test.emp:89-92`) and the fixture opens with 1024 idle ticks then a B tap to
drop out of free flight — the starting mode is part of the fixture's contract.

Poke `Replay_Ptr = Replay_OJZ_Fixture + 20` and `Input_Source = 1` via `System::mega_bus(&mut ())`
(`system.rs:634`; `&mut ()` suppresses the bus events). Then **read back and assert**: `Replay_Ptr` holds
the expected value, and the ROM at the fixture base starts `ARP0` with `FF 01` at +20. Computing the
pointer in typed code from a resolved symbol makes the recorded hex-vs-decimal class of bug — three
recorded instances, each a silently-accepted wrong pointer replaying garbage from inside the header —
inexpressible; the read-back is belt-and-braces.

### 2.5 PASS

`Replay_Done == $FF` (68000 `st` writes `$FF`, not 1 — `replay.emp:204`), with `Input_Source` self-cleared
to 0 on the same path as an independent corroboration. Both live inside one 4-byte read at `Input_Source`.

`Logic_Tick` **overshoots** the header's tick count, because after end-of-stream the game keeps running on
live input (green runs recorded 2423 ticks for a 1721-tick stream). So `Logic_Tick == tick_count` is not
the completion test; `Replay_Done == $FF` is.

### 2.6 FAIL — one predicate catches everything

There are exactly two raise sites on the replay path, both DEBUG-only (`replay.emp:210-218`):
`REPLAY DESYNC` (`d0` = actual hash, `d1` = `Logic_Tick`, `d2` = expected hash) and `REPLAY BAD OPCODE`.
But **89 other raise sites exist across `engine/` + `games/`**, plus all 12 CPU exception vectors, and
several are reachable mid-replay — the engine's own duplicate tripwire has fired inside a replay before
(`replay_fixture.emp:31-33`). All of them land in the same place.

**Stop on `PC == ErrorHandlerBlob`, exact equality.** Not a range: other blob entry points
(`MDDBG__KDebug_Write` at blob+`$D0E`, `MDDBG__Console_Write` at blob+`$B92`) are called during *normal*
operation, so a range predicate would fire on every debug print.

**Decode the fault from the stack, not from a screenshot.** `raise_exception` lowers to
`jsr (MDDBG__ErrorHandler).l` immediately followed by the inline NUL-terminated message
(`sigil/crates/sigil-frontend-emp/src/eval/diag.rs:727-736`). Confirmed byte-for-byte at `$26A2`:

```
22 38 80 04                move.l ($8004).w,d1      ; Logic_Tick -> d1
4e b9 00 0a 21 3a          jsr    $000A213A.l       ; ErrorHandlerBlob + 0
52 45 50 4c 41 59 20 44 45 53 59 4e 43 00           ; "REPLAY DESYNC\0"
a0 00                      dc.b   $A0,$00
4e f9 00 0a 2f 00          jmp    $000A2F00.l       ; PagesController
```

So at blob+0: `(A7).l` is a ROM pointer to the message text, and `(A7).l - 6` is the `jsr` — the **raise
site**, resolvable through the `.lst` to the exact `$module$Proc$label` (e.g.
`$engine.replay$Input_Tick$desync` at `$26A2`). Registers are **pre-clobber** at that instant.

This is the whole point. The current procedure reads `d0`/`d1`/`d2` **off a screenshot of the MD Debugger**,
because by the time a human can query, the handler is ~3630 bytes in and has clobbered `d0`-`d2` drawing
its own screen (`notes/…-restamp-ab.md:152-157`). Stopping at blob+0 eliminates that, and it delivers
recon §6 item 5 (*break-on-fault with a pre-clobber register snapshot*) as a side effect — with **no
engine-side mailbox**, because the return address *is* the mailbox.

> Caveat found in the bytes, recorded for the aeon side: `raise_exception` deliberately omits the
> `pea self(pc)` / `move.w sr` frame because it is the exception-vector counterpart, but `replay.emp:212,217`
> use it at an ordinary call site inside `Input_Tick`. The crash screen's SR/Offset/Caller lines at a
> replay trap are therefore decoded from a malformed frame. The register dump and the `(A7)` message are
> unaffected. Worth raising with aeon as a possibly mis-selected construct (`raise_error` frames correctly)
> — but it does not affect this runner, which reads neither.

### 2.7 TIMEOUT — a progress watchdog, not a fixed cap

`Logic_Tick` is not the frame clock. It increments once per `GameLoop` iteration, after `VSync_Wait`
(`game_loop.emp:29-30`, explicitly *"lag-immune, unlike `Frame_Counter`"*), so `ticks ≤ frames`, never the
reverse — and the boot phase burns frames with **zero** ticks, because `Level_LoadArt` spins `VSync_Wait`
inside a single dispatch (`aeon/engine/level/load_art.emp:124`).

Primary bound: **fail if `Logic_Tick` has not advanced for N frames while armed and not done.** That
distinguishes wedged from slow, and it catches an arm failure at frame ~10 rather than at the end of a
budget. A generous absolute frame cap backstops it.

On timeout, report `{Logic_Tick, Replay_Ptr − fixture_base, Replay_Hold, Replay_Done, Input_Source, PC +
symbol}`. `Replay_Ptr` still pointing inside the header is the unmistakable signature of a bad arm.

**The runner must check for the trap before ever reporting a timeout.** A desync presents *exactly* as a
hang (`running=true`, `Logic_Tick` frozen) — a timeout report that has not checked `PC == ErrorHandlerBlob`
is the single most likely wrong answer this tool can produce.

Wall-clock is not a budget under any circumstances: recorded sibling runs varied between ~30 fps and
~0.9 fps purely with competing desktop load, one playback taking 10–20 minutes. Load-independence is a
main argument for this runner.

---

## 3. Shape

A **new workspace crate, `oracle-replay`**: a library (all the logic) plus one `[[bin]]` named
`replay_runner`. It depends on `oracle-core` and nothing else.

Rationale: `oracle-core`'s charter is "deterministic, no-I/O" with `bincode` as its single dependency, so
file loading cannot live there — `oracle-aether` is the standing precedent for "this needs I/O, so it
lives outside core". The 14 `examples/` are the other precedent, but their own stated convention is *"a
dev tool, not a gate artifact — nothing in CI depends on the ROM existing"*, and 13 of 14 have exactly one
commit. This deliverable is the opposite of a disposable instrument: it is a gate another repo invokes by
path, so it wants a real binary name and ordinary `tests/`.

**Zero changes to `oracle-core`.** Every primitive is already public: `boot_with_sink`, `run_until_stop`,
`mega_bus` (poke), `ram()`, `rom()`, `cpu_regs()`, and the whole of `symbols.rs`. Currency-neutrality is
therefore structural, not merely tested — the same property that made several recent slices safe.

The one genuinely missing primitive, a memory-value stop condition, is **not needed**: the loop runs a
frame at a time under a PC predicate and reads work RAM between chunks, which is simpler than a custom
sink and gives the watchdog its natural granularity.

### Known duplication, accepted for exactly one slice

The `.lst` accept/refuse policy will exist in **three** places once this lands
(`oracle-frontend/src/symbol_file.rs:50`, `oracle-aether/src/engine.rs:970`, and here). That is precisely
the failure mode this project has criticised in its own archaeology — *"the abstraction was invented,
named, and not shared."* It is accepted here only because the extraction is a separate, mechanical,
no-behaviour-change edit across three crates, and bundling it would blur the review of the runner itself.
It is queued as the immediately-following slice, not as a follow-up ticket that drifts.

---

## 4. Slices

**Slice 1 — the runner.** New crate; load ROM + `.lst`; both refusals; resolve symbols by name; parse the
header from ROM; boot-and-arm at `GameState_OJZScroll_Init`; run under the trap predicate with the
progress watchdog; classify PASS / DESYNC / FAULT / TIMEOUT; decode faults via `(A7)` (message text, raise
site symbol, pre-clobber registers); exit 0 / non-zero with a structured diagnosis. Both fixtures
selectable. Tests: unit tests on the pure parts (header parse, classification, fault decode) plus a
real-artifact test gated on the Aeon build outputs being present, following the
`tests/symbols_real_lst.rs` idiom (`ORACLE_AEON_DIR`, graceful skip with a printed note).

**Slice 1n — the negative control, in the same slice.** `--negative-control` patches one checkpoint
payload to `DEADBEEF` in the loaded image and asserts the trap fires with message `REPLAY DESYNC` and
`d2 == 0xDEADBEEF`. This is not polish. *"A gate you have never seen fail is not a gate"*
(`plans/…-restamp.md:576`), and the failure mode it guards against — a silently mis-armed runner reporting
green forever — is the most likely way this tool ends up worse than useless. The proven expected result on
the sibling was a trap at `Logic_Tick 2` with `d2 = DEADBEEF` and `d0 = 1D375066`.

**Slice 2 — de-triplicate the `.lst` policy.** Extract to `oracle-core::symbols` as a pure function; wire
all three call sites; no behaviour change.

**Slice 3 — `--restamp`.** Aeon's candidate fix (b) collapses into a flag on (a): the run loop already
reports `{ring, tick, expected, actual}`, so one pass can patch each stale checkpoint in memory and
continue, replacing ~7 sequential 10–20-minute playthroughs. Must hard-refuse any length change — the
fixture sits before the fault-handler island, which *must* be the last byte-emitting section, so a size
change moves `EndOfRom` and requires a sigil repin.

**Slice 4 — the shared pad timeline.** The other half of P3, retiring six independent re-implementations
(`motion_run.rs:50-143` is the reference design: per-port `[start,end)` rows, union semantics). Independent
justification, independent slice.

**Not in scope, and deliberately:** `--rerecord`. The sibling driver's inability to press `c` made
re-recording impossible and re-stamping the only option; the moment we make it possible, someone will
re-record where they should re-stamp and forfeit the fixture's coverage —
`aeon/tools/test_replay_fixture.py:53-72` exists to catch exactly that. If it is ever built it must be a
separate, deliberate command, never a fallback.

**Cross-repo, not ours to land:** wiring `test.sh` section 8 to invoke the runner. Aeon's harness is
hand-rolled bash (it runs no pytest at all) and its own hard-won rule is that *a named gate whose inputs
are absent is a RED gate, not a silent one*. Raised as an ask, with a sketch, once the runner exists.

---

## 5. Traps this design has already absorbed

| trap | source | handled by |
|---|---|---|
| Stale hardcoded addresses; `Replay_Done`'s old address is now `Replay_Ptr` | measured, §2.1 | resolve by name, fail on unresolved |
| Release ROM = false green | `replay.emp:174-186` | shape binding + `REPLAY DESYNC` string assertion |
| `core_hash` has zero consumers and is stale | `notes/2026-08-05-objtest-gate-ab.md:59-66` | read the stream from ROM instead |
| A trap is indistinguishable from a hang | `plans/…-restamp.md:98` | check `PC == ErrorHandlerBlob` before reporting timeout |
| Registers clobbered before they can be read | `notes/…-restamp-ab.md:152` | stop at blob+0, pre-clobber |
| Hex-vs-decimal poke, silently accepted (3 instances) | `notes/…-restamp-ab.md:158` | typed computation from a resolved symbol + read-back |
| Wrong-shape `.lst` is a silent wrong answer | `plans/…-restamp.md:81` | `validate_against_rom`, fatal |
| Watchpointing `Replay_Done` wedges the sibling | `notes/…-restamp-ab.md:164` | poll between frame chunks; no watchpoint |
| `reload_rom` ordering | `notes/…-restamp-ab.md:172` | `boot_with_sink` — inexpressible here |
| `Logic_Tick = ring + 2` | `notes/…-restamp-ab.md:22` | fixture-specific, **not** a format invariant; never hardcode |
| Wall-clock as a budget | `notes/…-restamp-ab.md:167` | emulated-time watchdog only |
| `.worktrees/` holds ~20 stale copies of the sources | measured | anchor on absolute repo-root paths |
| Pressing Start sets `Replay_Exit_Request` | `replay.emp:157-159` | hold the pad at `$00` for the whole run |
