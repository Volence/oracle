# The tooling frontier — consolidated recon (2026-08-14)

**Status:** recon + proposed sequencing. Of §7, **only P2 (symbols) has been built** — see the P2 entry and
§9. Everything else remains proposed: the owner has approved nothing further in tracks 2/3 beyond the
investigation itself.

> ## ⚠ ERRATA — five claims in this document are wrong (corrected 2026-08-14)
>
> The follow-up design pass `docs/2026-08-14-trace-recorder-design.md` was commissioned from this
> document and, doing its job properly, **refuted five of its claims**. All five were re-verified
> firsthand by the overseer before acceptance. They are corrected inline below and listed here so
> nobody reads a stale figure out of the tables:
>
> 1. **"6 ad-hoc sinks", with "two near-duplicates written for the same seam on the same day"
>    (§3 item 1)** — WRONG as evidence for a bus trace. `FrameCapture`
>    (`tests/conformance_roms.rs:262`) and `LineCollector` (`tests/scanline_capture.rs:27`) both have
>    a byte-identical **empty** `on_event` stub: they are *scanline* collectors on a different seam
>    and record no bus events at all. **Committed recurrence for a bus trace is 2, not 6.**
> 2. **"`k4_openbus_probe.rs` (26 hand-declared counters)" (§3, §5)** — WRONG. It has **16**
>    counters (18 struct fields, two of which are boolean latch shadows). The 26 was a markdown
>    table's column count.
> 3. **"Do NOT add fields to `BusEvent` … a new field poisons those asserts" (§5, constraint 2)** —
>    the *conclusion* survives but the *stated reason* is folklore. Measured blast radius: **2
>    breaking assertions in 1 file plus 12 mechanical literal edits, zero exhaustive
>    destructurings.** The "~40 sites" figure traces to `docs/2026-07-23-phase-sy4-subframe-timing-design.md:97-100`,
>    which counted helper *call* sites. **The real reason a field is wrong is stronger:** three of the
>    four emission sites do not know `pc`/`frame` (`bus.rs:563` has only `now_mclk`; `bus.rs:190/201`
>    has no clock), so the field would be a sentinel in the 22 unit tests that build a bus with no
>    `System` — a lie exactly where the struct is most used.
> 4. **Function-code master attribution "is what let the sound-silence hunt eliminate the 68k"
>    (§5)** — NOT SUPPORTED. The primary record (`docs/2026-07-22-phase-rt-design.md:145-148, :444`)
>    shows that hunt classified on `addr` and was explicitly `fc`-agnostic; the 68k was never a
>    suspect. Master attribution may still be worth having, but this document's justifying episode
>    does not hold it up. Registered as open question `F-TRACE-MASTER`.
> 5. **C1 (atomic arm-at-power-on) is presented as a constraint to honour (§5)** — it is currently
>    **structurally impossible**: `System::reset()` hardcodes a unit sink (`system.rs:361`), so the
>    reset vector fetches cannot be captured by any caller, and all eight bespoke sinks in the tree
>    attach post-reset. That is a real defect, ~5 lines to fix, and it should be unbundled rather
>    than carried as a constraint everything else must satisfy.
>
> **Method note for future recon:** every one of these came from an agent instructed to be skeptical
> of the brief that commissioned it, and four of the five are cases where *this document restated a
> figure without recounting it*. Recurrence counts and blast radii must be measured at the moment
> they are cited, not inherited. That is the same failure mode as the self-reported `exit 0` in §8.

**Provenance.** Three independent agent surveys, run in parallel, each required to cite `file:line`
or quote primary documents:

1. **Archaeology** — what *we* repeatedly hand-rolled across two years of building and validating
   oracle-next. Evidence: 14 committed examples (2,709 lines), 19 recon/triage/findings docs, 11 test
   files, 2,550 lines of Python + 68k harness ROMs in `tools/`.
2. **Critique** — a design review of the sibling C++ Oracle's ~50-tool MCP surface, read end to end
   (`ControlSocket.cpp` 3,168 lines, `oracle_mcp.py` 736 lines, plus `empyrean/contract/protocol.md`).
3. **Engine-side** — what Aeon/Empyrean development actually needs, read from the suite's own repos.

The owner's framing: *"utilize the information we got from building our engine and testing… review
the ~45 capabilities because I don't know if all of them are useful… this is where we consider the
things we can do for our engine specifically and our tools around it."* All three streams were
commissioned against that framing. Broad ROM validation was dropped by owner ruling the same day and
is not revisited here.

---

## 1. The headline corrections

Three premises this project was carrying turned out to be wrong or stale. They matter because each
one changes scope.

**1a. Track 2 is not a design problem. The protocol already exists and is normative.**
`empyrean/contract/protocol.md` (412 lines) states outright: *"This document is the source of truth;
the emulator conforms to it, not the reverse."* It fixes JSON-RPC 2.0 over NDJSON, an `AF_UNIX`
transport at `$XDG_RUNTIME_DIR/oracle.sock` (mode 0600), an `initialize`/`initialized` handshake
advertising a generated method list plus capability flags, `emulator/<name>` method naming,
server-push events (`emulator/stopped`, `emulator/resumed`, `emulator/romReloaded`), hex-string
addresses vs numeric counts, and error codes including a distinct `-32012` (no symbols loaded) vs
`-32013` (symbol not found). `ROADMAP.md:63` says oracle-next *"inherits Oracle's bus role
unchanged."* **Aurora is already written against this surface and approved.** We implement a spec
with a waiting client; we do not design one.

Independent corroboration worth noting: the critique stream's single strongest architectural
recommendation — *build the API, make MCP one client of it* — is already Aether decisions D1/D10.
Two agents with different evidence reached the same conclusion the contract had already reached.

**1b. The `convsym` premise was stale; symbols are cheaper AND better than assumed.**
As of the 2026-08-08 Spec-5 Stage 2 flip, `aeon/build.sh:261-262` is a single `sigil build … --emit-lst`
call — `asl` and `convsym` are both out of the build script (`convsym` survives, shelled from *inside*
sigil). The emitted `<rom>.lst` is two regexes from a symbol table. Two properties beat the sibling:

- Names are `$module.path$Proc$local` — a **three-level hierarchy for free**. Split on `$` and a PC
  resolves to `EntryPoint.wait_dma` instead of "somewhere after `EntryPoint`", retiring the
  nearest-label-plus-displacement guess for locals inside long routines.
- **Every shipped ROM, release included, already carries a `deb2` symbol appendix** at `EndOfRom`
  (verified: `s4.bin` at `0xA11F0`, 36,884 B; `s4.debug.bin` at `0xA30B0`, 43,474 B). Decoding it makes
  symbols **structurally unstaleable** — the table ships inside the bytes you loaded. `convsym` cannot
  read `deb2`, so this is bounded new work against an open-source reference, and it is optional; the
  `.lst` is the cheap path.

Traps a loader must respect: **no file/line/column information exists anywhere in the build output**
(`grep -c include s4.lst` = 0); RAM addresses are plain 8-hex `FFFFxxxx`, **not** AS's 48-bit
sign-extended form (Aeon's own `tools/s4budget.py` still checks the old form and consequently reports
`RAM: 0 bytes` — a live bug; do not port the regex); the type column is `C` for 100% of entries, so
code and RAM are distinguishable only by address range; and symbols bind **per-game and per-shape**
(`if DEBUG == 1 @shape_divergent` blocks in `ram.emp` shift RAM layout), so `s4.debug.lst` against
`s4.bin` is a silent wrong answer that must be refused.

**1c. There is a large, fully-built, completely dead asset on the engine side.**
`aeon/engine/system/replay.emp` (378 lines) plays an `ARP0` input stream embedded in the ROM and every
64 ticks recomputes an **address-free hash of `Player_1`'s gameplay state**, raising `REPLAY DESYNC`
on mismatch. Aeon's own `DEFERRED_WORK.md`: *"The replay net has NO automated runner — it is invisible
to every gate we own… it cannot detect a desync — **that needs the emulator**."*

Meanwhile the sibling's input driver cannot do the job:
*"`emulator_hold` fails ~50% of the time"* · *"`hold` ADDS, it does not replace"* · *"**the `c` button
never registers**"* · *"Re-recording is impossible."* A re-stamp currently costs ~7 manual playthroughs.

Our core is deterministic by construction and already has `set_pad` + `run_frames`. Correct
set-not-add pad semantics plus a headless runner converts a dead regression net into a working CI
gate. **This is the highest-leverage engine-facing item we found.**

---

## 2. The disagreement, and how to read it

The two streams contradict each other on one point, and it should not be smoothed over.

- **Critique:** `pixel_attribution` — per-pixel "why is this pixel this colour, including which
  candidates lost and why" — is *"the flagship agent tool"*, and combined with a VRAM watch it closes
  the whole visual-glitch loop (click pixel → tile → watch address → writing PC → disassemble). The
  frontend already prototypes exactly this by accident.
- **Archaeology:** `pixel_attribution`, `render_line_report`, `sprites_decoded` and `frame_report`
  were the **charter's named differentiator** (`CHARTER.md:43-45`), shipped July 2026, and across all
  19 root-cause docs and all 14 probes have **zero uses**. Every VDP bug actually caught — CD5,
  DMA-fill placement, VBlank-while-display-off, the ODD flag, FIFO slot phase — was found by bus
  tracing and byte-diffing.

**Overseer reading (a judgement, not a measurement): these sample different populations.** The
archaeology's corpus is *us debugging the emulator*. The engine-side corpus is *Aeon developers
debugging a game* — and there, three separate documents independently rediscovered that the sibling's
`read_cram` is frame-latched and therefore *"NOT a valid instrument for raster work"*, which is
precisely what our per-scanline `LineReport` machinery solves. The zero-use datum is real and valid
for emulator development; it should **not** be generalised to the game-development audience that
tracks 2/3 are actually for.

The cautionary half still stands and should be carried as policy: **a capability being in the charter
is not evidence anyone will use it.** Ship the thin exposure first, watch whether it gets used, and
let usage — not design conviction — justify further investment.

### 2a. Where the archaeology should simply win

Its negative evidence on *interactive* debugging is strong and specific. `grep -rni breakpoint
crates/` returns nothing; all stepping episodes were against reference emulators. Three independent
statements of **harm**:

1. *"stepping distorts Oracle's device scheduling"* → the live run was demoted to secondary evidence.
2. Seven breakpoints from an earlier session were still armed (one with **1,691,410 hits**) and had to
   be cleared before captures were trustworthy — the debugger's own residual state was a contaminant.
3. Oracle's breakpoints/profiler are 68k-only, forcing Z80 PC sampling by single-stepping the 68k.

This survives the availability objection: we had ~50 MCP ops available the whole time, and
`breakpoint_add`, `call_stack`, `step_over`, `step_out`, `run_to`, `load_symbols`, `object_list` and
`get_profiler` appear in **zero** hunt narratives. We used the reference debugger as a ground-truth
*capture device*, never as a step-and-inspect debugger.

**But the conclusion is not "no run control".** What the record wants is **non-blocking
stop-on-condition**: let the run halt itself deterministically at a predicate and hand back a
snapshot — no interactive session, no residual state, no scheduling perturbation. That is a different
feature from a debugger's step/break, and the evidence supports it as strongly as it rejects the
interactive form.

**The cheapest item on the board:** `run_until_with_sink` already calls `sink.on_step_boundary(pc,
frame)` on *every* CPU step (`system.rs:718-720`), but no sink method can return a stop signal
(`bus.rs:52-100`), so the loop cannot be interrupted. **A ~10-line trait change gates roughly half of
the archaeology's Tier 1.**

> **SHIPPED 2026-08-14 (P0).** `BusEventSink::stop_requested` (defaulted `false`), asked once per step
> immediately after the `on_step_boundary` stamp and *before* the instruction commits, plus
> `System::run_until_stop(max_frames, predicate)` and a `StopRecord { reason, pc, frame, mclk }` that
> keeps "the predicate fired" and "I gave up" as distinct variants. The estimate was accurate: the loop
> change is 4 lines. Settled semantics and the currency argument are in `system.rs` /
> `bus.rs` doc comments; the resumability property (`stop → resume ≡ uninterrupted`) is a test.

### 2b. The success precedent to copy

Watchpoints are the **only** Tier-1 capability that stopped being hand-rolled — predicted 2026-07-20,
shipped 2026-07-22 as a 552-line public API, then reused *unmodified* by `watch_probe.rs`, the live
frontend, and a K4 adjudication. Tracing never got promoted and has been re-implemented six times.
The lesson is about **promotion**, not invention: the capability existed either way; making it a real
API is what stopped the duplication.

---

## 3. What the archaeology says we actually do (ranked by recurrence)

Recurrence is the ranking signal. A capability appearing once is an anecdote. Note that *"throwaway"*
appears in **11 independent docs**, so committed code undercounts the true recurrence, and **13 of 14
examples have exactly one commit** — they are disposable single-question instruments.

| # | Capability | Recurrence | Status in our core |
|---|---|---|---|
| 1 | Filtered, attributed, timestamped **bus-event trace** | 11 hunts + ~~6~~ **2** committed bus-trace sinks — see ERRATA 1; the "near-duplicate pair" are scanline collectors with empty `on_event` stubs, a different seam | **SHIPPED 2026-08-14** — as four additive changes to `watchpoints.rs` (`mclk`, watch ids/labels, record/count/census modes, `seen` + an amortized ring), not a new subsystem. `examples/diag_soundqueue.rs` is now pure configuration with byte-identical output. Design + measured corrections: `docs/2026-08-14-trace-recorder-design.md` |
| 2 | **Boot → run to frame N → dump regions → byte-diff** | 6 hunts; `boot_rom` cited in 10 docs; `write_ppm` byte-identical in **4 files** | Copy-paste only |
| 3 | **Frame comparison** w/ normalization + offset alignment | 6 episodes; alignment solved 3 separate ways | Sharpest "unreachable primitive" case (below) |
| 4 | **Guest-structure decoding** (read the game's own data) | 4+; the single biggest unlock of the conformance arc | Doesn't exist |
| 5 | **Disassembler** | 5 external uses; leaked into permanent source at `bus.rs:1976,2063,2153` | Doesn't exist |
| 6 | **Capture with atomic arm-at-power-on** | 11 differential episodes; **the 3 most expensive failures were capture-control failures, not core bugs** | Doesn't exist |
| 7 | **Run-until-condition / stop-on-event** | 9 hand-tuned magic frame budgets + 2 polling loops | **SHIPPED 2026-08-14** — `stop_requested` + `run_until_stop`; 2 of the 9 budgets converted |
| 8 | **Scripted deterministic input** | 4 independent implementations | Nothing above the raw `set_pad` |
| 9 | **Aggregate/statistical probing** | 5 episodes — and this class **disproved two recorded root causes** | Doesn't exist |
| 10 | Fixture-ROM authoring | 8 hand-authored ROMs | Exists but `#[doc(hidden)]`, deliberately unreachable |
| 11 | Instrument without touching `src/` | 3+, all resolved by copying the whole tree | Why `BusEventSink` is load-bearing |

**Item 3 deserves its own note**, because it is the cleanest example of the failure mode this project
should design against: `conformance_roms.rs:231` factored out `fnv1a_rgb` **and named the hazard in a
comment** — *"so they cannot drift into different layouts"* — and then shared it with nobody. Three
sites still hand-roll the identical loop, and `state_hash::fnv1a_bytes` stays unreachable for all of
them because it takes `&[u8]` while frames are `Vec<(u8,u8,u8)>`. *The abstraction was invented,
named, and not shared.*

**Tier 2 — anecdotes, do not generalise:** WAV writing, PPM *parsing*, VGM timeline TSV, the SST perf
harness, the SRAM round-trip.

**Zero-use, despite being wishlisted or promised:** rewind/time-travel (despite `snapshot`/`restore`
existing and being a headline charter promise); call stack, step-over, symbol lookup (one wishlist
mention each); taint tracking; execution coverage; **and the bus-legality detector, which the
2026-07-20 wishlist called "the big one for us… Highest strategic value" and which two years of hunts
never once needed.** That last one is worth pausing on before it is funded again.

---

## 4. Critique of the ~50-tool surface — what to keep, reshape, drop

**Headline: it is a debugger GUI transcribed into RPC.** Nearly every tool maps 1:1 to an Exodus panel
or menu item, so the tools answer *"show me this panel"* where an agent needs *"answer this
question."* Consequences: **nothing conveys causation** (no provenance, no disassembly, no trace, no
diff — you get state, never *why*), and the README's flagship workflow is **four round-trips**
(screenshot → registers → call_stack → player_state) to answer "what's wrong".

**The worst single defect: no reply carries a timestamp.** Reads are taken against a possibly-running
machine and only `status` returns a frame token. An agent stitching four reads into one conclusion may
be reading four different machine states **with no way to detect it**. That is a silent-wrong-answer
generator.

**Verdict:** ~20 well-shaped tools cover strictly more ground than the current 50.

**Preserve deliberately:** bus-first with MCP as a client (already D1/D10); the **capability handshake
generated from the handler map**, which makes doc/code drift structurally impossible; symbol-first
addressing where every reply annotates nearest-symbol + displacement; hash-not-payload discipline;
**refuse loud, never clamp or wrap**; **caveats carried inside the payload** (the sibling's single best
idea — agents over-trust precise-looking numbers); server-side aggregation as seen in
`get_profiler_frames`, the only correctly-shaped tool in the set; cursor-based tailing.

**Reshape (selected):** `audio_spectrum` is a **context bomb** (up to 16,384 floats at `indent=2`) →
return peaks, not bins. `read_memory`/`read_vram` return undifferentiated hex with **inconsistent
parameter types** (hex string vs integer) and inconsistent caps → one `read {space, addr|symbol, len}`
with an annotated dump; formatted output is usually *shorter* than raw hex and vastly more usable.
Collapse `step`/`step_over`/`step_out` into one blocking call returning a stop record. `breakpoint_add`
gains conditions and an **`on_hit: record`** mode — log and keep running, which is what an agent wants
most of the time. `call_stack` is a heuristic stack scan; we control the CPU, so maintain a **shadow
stack** and return the exact one (which also hands us the profiler). `state_hash` should return a
**vector of region hashes** so one call localises a divergence instead of merely proving one exists.

**Drop:** `get_profiler`, `log_clear` (destructive on a multi-client bus — it eats other clients'
unread entries), `get_layer_states`/`get_channel_states` (better: make mutes **scoped** to the call
that takes the screenshot, so they auto-restore and a stale mute cannot leak into a later conclusion),
`release_all`, `object_slot`, `wait_for_break`, `ping`, and `debug_arbiter` — 130 lines of
introspection built for a single deadlock hunt that is now fixed. Textbook vestigial.

**A genuine landmine to fix if ported:** `write_vram` writes straight into the VRAM buffer, bypassing
the VDP port path, autoincrement, FIFO and DMA — and **nothing in its docstring says so**. An agent
will "verify" tile data and conclude the bug is elsewhere having proven nothing about the real path.
Label it `poke_vram` and flag `bypasses_vdp_port: true` in the reply.

**Three rules that should be non-negotiable:** every reply carries `{frame, mclk, running}`; every
array is bounded, cursored, and flags truncation; anything approximate carries a `caveat` string.

---

## 5. Design constraints (from the archaeology's most expensive failures)

> Any capture, trace, or comparison facility must satisfy all four, or it will produce **confidently
> wrong answers rather than no answer**.
>
> **C1 — Atomic arm-at-power-on.** Arming must be indivisible with reset, verifiable from the captured
> state itself, and impossible to express as "reset, then arm". Precedent: a post-boot arm point
> silently skipped the window under investigation and *"diverged at melody index 0, a garbage
> comparison"*, voiding a full investigation; the doc carries a banner retracting its own verdict. The
> correct arm point was confirmed only by observing pristine power-on values (`PC=0xFFFFFFFF,
> SP=0xFFFFFFFF, SR=0xFFFF`) — so the API must **expose** that check, not assume it.
>
> **C2 — Deterministic frame identity.** Every capture stamped with an *emulated* frame/mclk index,
> never wall-clock. Two runs of the same ROM+input must yield identical stamps. Precedent: the
> sibling's `frame_token` is a UI counter, which forced hand-rolled realignment three separate ways.
>
> **C3 — Cheap negative-control mode.** Every comparator must be runnable in a *known-fail*
> configuration via one flag, without editing the instrument. Precedent: the s4 A/B was trustworthy
> only because *"pre-normalization the frames differ in 35,083 pixels"* proved the comparator was not
> trivially passing; and a null result was correctly downgraded to *"instrument blind spot… NOT a
> pin"* only because NOP controls proved the detector fired. **Absent a control, a silent zero is
> indistinguishable from a pass.**
>
> **C4 — No residual instrument state.** Caller-owned and run-scoped, never stored in the machine.
> `BusEventSink` already gets this right; the 1.69-million-hit stale breakpoint shows the cost of the
> alternative.

Two structural notes for whoever builds the control layer:

- **`BusEventSink` is monomorphized and passed per-run** — exactly one sink type per run, no registry.
  A control layer needs a fan-out combinator, and the only existing example is hand-written
  (`oracle-frontend/src/audio.rs`, `AudioAndWatch`). **Build the combinator before the tool surface or
  it will be written five times.**
  > **SHIPPED 2026-08-14.** `bus::Fanout<A, B>` (nest for 3+), plus `BusEventSink` impls for `&mut S` and
  > `Option<S>` so an only-sometimes-attached member needs no bespoke type. Composition rules: deliveries go
  > to both halves in a fixed `a`-then-`b` order; every `wants_*` capability query and the stop signal are
  > **OR**ed. The hand-written `AudioAndWatch` is now a type alias for it — the one remaining hand-rolled
  > fan-out is retired rather than joined.
- **`BusEvent` carries no attribution** (`bus.rs:43-49` is op/fc/addr/size/value only), so every
  consumer relatches PC/frame/mclk itself. ~~Function-code *master* attribution … is what let the
  sound-silence hunt eliminate the 68k, and belongs **in the event**.~~
  **CORRECTED (ERRATA 3 and 4).** Attribution does **not** belong in the event: three of the four
  emission sites do not know `pc`/`frame`, so a field would be a sentinel wherever the struct is most
  used. The design pass settles this as a richer type layered *above* `BusEvent`
  (`docs/2026-08-14-trace-recorder-design.md` §3-§4). And the master-attribution justification above
  is not supported by the primary record — that hunt classified on `addr` and was explicitly
  `fc`-agnostic. Master attribution is now open question `F-TRACE-MASTER`, not a settled requirement.
- ~~**Watchpoints have two gaps to close before exposure:** the per-watch `label` is
  `#[allow(dead_code)]` and never propagated into hits, and there is no watch id / removal /
  enumeration — an agent running three concurrent watches cannot tell which one fired.~~
  > **CLOSED 2026-08-14** (S4/T2). `add`/`add_watch`/`add_vdp_watch` return a `WatchId`; every
  > `WatchHit` carries the id of the watch that recorded it (the id, not the `String` — a hit stays
  > `Copy` and nothing allocates on the instrumented path), resolvable via `label_of`/`watches`.
  > `remove(id)` retires one watch, and ids are never reused, so a stale handle resolves to nothing
  > rather than silently to a different watch. An access matching several watches is recorded once,
  > attributed to the lowest-id matching `Record` watch, while every matching watch counts it.

---

## 6. Engine-facing capabilities, ranked

**CONFORM** = a decision already exists · **CONSUME** = the artifact already exists on disk · **NEW**

1. **Aether socket + handshake + events** — CONFORM. Gates everything else. Implement `protocol.md`
   verbatim; §8 explicitly forbids inventing ops or a second envelope.
2. **`.lst` symbol loader** — CONSUME. Highest single-feature leverage. Plus the three refinements in
   §1b: `$`-scope tree, 8-digit RAM addresses, **refuse a shape mismatch**.
3. **Deterministic scripted input → headless `ARP0` replay runner** — NEW glue over two built halves.
   Converts a dead regression net into a CI gate (§1c). Fastest credibility win with the engine team.
4. **KDebug / Gens-KMod on `$C00004`** — NEW, ~1 day. `aeon/engine/debug/debugger.asm:507-517` *already
   emits* `$9E00` breakline, `$9FC0` starttimer, `$9F00` endtimer, `$9D00` breakpoint plus character
   output — **and we currently discard all of it as no-op VDP register writes.** Honouring it yields
   engine `printf` into the log, real breakpoints, and exact cycle timing, with the engine
   self-instrumenting its own hot loops.
5. **Break-on-fault with a pre-clobber register snapshot** — NEW, small. Aeon ships the MD Debugger in
   *both* shapes, wired to all 12 exception vectors, with 165 `assert`/`raise_exception` sites. Today
   `BUGS.md:152-153` says to *"read `d0`/`d1` off the MD Debugger screen (registers are CLOBBERED at a
   fault, so screenshot the dump)"*. Capture at vector entry instead. **The engine-side mailbox the
   2026-07-20 idea doc assumed we would need is unnecessary** — the vendored handler's convention is
   already a fixed contract.
6. **Expose the per-scanline `LineReport` / `pixel_attribution` machinery** — MOSTLY BUILT. Three
   separate Aeon docs independently rediscovered the frame-latched-CRAM trap. Near-free.
7. **Build-identity validation** — half CONSUME (the `deb2` appendix is a strong fingerprint even
   undecoded), half a suite ask (`s4.build.json`, already blessed in `empyrean/CLAUDE.md:203`).
8. Then: symbol-attributed profiler (+ **DMA-budget/ring-margin telemetry**, a stated blind spot:
   *"we have literally no way to measure how much DMA-survival margin a frame actually used"*), VDP
   watchpoint exposure, scenario ops, Z80 disassembly/breakpoints, per-frame value tracing.

**Object inspection must be descriptor-driven, not compiled in.** The sibling hardcodes struct offsets
in C++ and detects the engine by "does `Player_1` resolve" — and it **already rotted once**. Aeon's
layout lives in `.emp`, a typed compiled DSL with no machine-readable export; the object record is
**$50 bytes, not the classic $40**. Correct seam: a small external versioned descriptor resolving all
base addresses via symbols, proposed into `contract/schema/` — *not* a `.emp` parser in the emulator,
and not hardcoded offsets. `ASSEMBLER_VISION.md:284` already ruled the container format: a compact
versioned Aether-native artifact, explicitly **not** ELF+DWARF.

**Transport robustness is a real requirement, not polish.** `aeon/docs/BUGS.md:494-551`: a frozen repro
frame was *"lost to an emulator control-socket hang before the sprite table could be dumped"* and could
not be re-frozen. **A hang in the debug transport destroyed irreplaceable evidence.** A slow or dead
client must never be able to wedge the emulator.

---

## 7. Proposed sequencing

Reconciling "conform to Aether" with "our record says build recorders, not a debugger": the protocol
fixes the *wire*, not which methods we implement first or how their bodies behave. So we conform on
shape and let the archaeology choose the order.

- **P0 — the ~10-line unlock. DONE 2026-08-14.** A stop signal on the sink trait → `run_until(predicate)`,
  shipped with the P1 fan-out combinator pulled forward alongside it (it is the other half of the same seam).
  2 of the 9 magic frame budgets are converted as proof; the other 7 are a separate reviewable change. Each
  one needs its own measurement rather than a blanket idle detector — `m68k_bcd` turns out to touch the VDP
  on frames 0, 6 and 530 and nowhere in between, so a "the screen went quiet" predicate stops 523 frames
  before its answer exists (recorded in `docs/2026-07-25-testrom-conformance.md` L2).
- **P1 — Aether socket + handshake + events**, verbatim per `protocol.md`. The fan-out sink combinator it
  asked for first is already built (see §5). Thin surface only: `status`, `run`, `read`, `input`,
  `checkpoint`, `screen`.
  Every reply carries `{frame, mclk, running}` from day one — retrofitting C2 later is far more
  expensive than honouring it now.
- **P2 — symbols** from `.lst`, with shape refusal. Everything downstream reads better immediately.
  **SHIPPED** — `crates/oracle-core/src/symbols.rs` (pure parser, both lookup directions, `$`-scope tree,
  `deb2` shape refusal) + `crates/oracle-frontend/src/symbol_file.rs` (the file half) + symbolised watch-hit
  PCs. See §9 for the seven corrections shipping it produced.
- **P3 — deterministic scripted input + the headless replay runner.** First real payback to the engine.
- **P4 — KDebug `$C00004`**, then break-on-fault.
- ~~**P5 — trace/query as a first-class recorder** on the sink seam (attribution in the event, filtering,
  aggregation) — the archaeology's #1, and the thing that has been re-implemented six times.~~
  > **SHIPPED 2026-08-14.** Attribution is *not* in the event (ERRATA 3/4) — it is in `WatchHit`, layered
  > above `BusEvent`. Filtering stayed record-time and gained an `fc` predicate; aggregation shipped as
  > count / bounded census / distinct-cardinality / first-last. "Re-implemented six times" is 2 (ERRATA 1).
- **Later / evidence-gated:** disassembler, object descriptors, profiler, bus-legality lint. Explicitly
  **argue against** interactive step/break, call stacks, and rewind until something demands them.

**Open questions for the owner.** (a) Does the `deb2` decoder earn its cost now, or is the `.lst` path
sufficient until symbols rot again? (b) Do we raise the two cross-repo asks — ship `s4.build.json`, and
resolve the `emulator/rom_reloaded` vs `emulator/romReloaded` drift between Aurora's approved spec and
`protocol.md` §3 — as change requests now, or carry them? (c) How much of the 53-method catalog do we
commit to, given the critique's finding that ~20 well-shaped tools cover more ground?

---

## 8. Method note

Two of the three streams corrected themselves mid-flight: the archaeology retracted a claim it had
asserted without checking and corrected another into a **sharper** finding (the `fnv1a_rgb` case in
§3), and the engine survey overturned the `convsym` premise this project had been carrying in its
own CLAUDE.md. The instruction that produced both was *require file:line or a primary quote for every
claim, and report negative evidence*. Worth repeating on the next recon.

Two findings were reached **independently by two streams**, which is the strongest signal in the set:
the sibling's input driver is broken, and its frame identity is not emulation-derived.

---

## 9. Addendum — what shipping P2 (symbols) corrected (2026-08-14)

The recon above was right about the format and the traps. Implementing it against the real files surfaced
seven things it did not have — three of them (9e–9g) only after an adversarial review of the first working
version. Recorded here rather than edited into §1b, so the original reading stays intact.

**9a. A fourth trap: `FFFFxxxx` is a 32-bit spelling of a 24-bit bus address.** §1b correctly says RAM
addresses are plain 8-hex and not sign-extended, but stops there. The 68000 drives 24 address lines and
`bus.rs` decodes work RAM at `$E00000–$FFFFFF`, so the listing's `FFFF8CFA` **is** the machine's `$FF8CFA`.
Matching a PC or a bus address against the raw listing value finds nothing, every time — a loader that got
traps 1–3 right and missed this would still resolve zero RAM addresses. `Symbol` therefore carries both
`raw_addr` (the file's spelling) and `addr` (masked); all lookups use the masked form and mask the query too.
Corollary: nearest-preceding search must not cross an `AddrSpace` boundary, or a RAM address below the first
RAM symbol resolves to the last ROM symbol with a ~15 MB displacement.

**9b. `deb2` is verified, and it does catch the shape cross — but it is a filter, not a proof.** Verified
firsthand: `s4.bin` carries `de b2 04 02 …` at `$A11F0` (= 659,952; appendix 36,884 bytes, matching §1b),
`s4.debug.bin` at `$A30B0` (43,474 bytes). Both crosses are caught — `s4.bin` at the debug `EndOfRom` reads
`43 0b …`, `s4.debug.bin` at the release `EndOfRom` reads zeros. **But `demo.lst` and `demo.debug.lst` both
declare `EndOfRom : 11224`**, so the probe cannot separate two genuinely different demo builds that share
1,197 symbols at differing addresses. §1b's "structurally unstaleable" is true only of a *decoded* appendix;
the offset+magic probe is a strong filter with a real, reproduced blind spot. `validate_against_rom` returns
three states (`Match` / `Mismatch` / `Indeterminate`) rather than a bool for exactly this reason, and a
`Match` is documented as "not obviously wrong", never "proven right".

**9c. The cost of a real binding guarantee is producer-side, and small.** Investigated and rejected as
insufficient: the ROM header `$100–$18D` is **byte-identical** between `s4.bin` and `s4.debug.bin` (same date,
title, and serial `GM S4-0001-00`), so it separates *games* but never *shapes*; the `.lst` itself carries no
date, version, hash, or ROM size — `EndOfRom` is the only ROM-derivable datum in the whole file; and symbol
names are **not** ASCII-searchable in the appendix (`deb2` packs them), so no zero-decode substring check
exists. `$1A4` == `len - 1` holds 5/5 across shapes, and `$18E` is a genuine whole-image checksum over
`[0x200, len)` — but both validate the ROM against *itself*, not the listing against the ROM. The clean fix
is a **sidecar from sigil**: have `append_deb2_appendix` also emit the built image's `$18E`/`$1A4` (or a hash)
beside the `.lst`. That is a cross-repo ask, not our work.

**9d. Aeon's `s4budget.py` bug is measurable, not just theoretical.** The real `s4.lst` has **279** RAM
symbols; their tool reports `RAM: 0 bytes` because its regex still expects the 48-bit sign-extended form.
Pinned as a regression test here (`tests/symbols_real_lst.rs`), and worth raising with them.

**9e. A fifth trap, found in review: the module is located by the dot, not by position.** §1b reads the
mangling as `$<module.path>$<Proc>$<local>`, and that is true of the release listing — but a label emitted
inside a macro puts the macro instance *outside* the module: `$diag2$engine.bg_anim$raise`. Taking the first
component as the module invents a phantom module per macro instance. Measured on `s4.debug.lst`: **94
"modules", 34 of them phantom `diag1…diag46`, with 125 symbols misfiled**. `s4.lst` contains no
macro-scoped labels at all, which is precisely why the positional rule looks correct until someone opens a
debug build — a reminder that validating a format against one artifact validates one artifact. Across all
six real listings every mangled name has **exactly one** dotted component and it is always the module, in
three arrangements (`$mod$Parent$local`, `$outer$mod$local`, `$outer$mod$Parent$local`), so the dot rule
resolves all of them. Corrected count for `s4.debug.lst`: 64 modules, all real.

**9f. And a consequence of it: some readable names are ambiguous.** Because a macro expands N times, N
labels share one demangled spelling at N *different* addresses — 24 non-synthetic collisions in
`s4.debug.lst` (e.g. `engine.compression_selftest.raise` names five addresses), 130 counting plumbing. That
spelling does not identify a location, so printing it is exactly the failure mode this work exists to
prevent. Such symbols are flagged and displayed by their raw mangled name, which is unique.

**9g. The shape refusal was fail-open, also found in review.** `Mismatch` and `Indeterminate` are not
independent: **any listing that would be refused becomes merely `Indeterminate` if its `EndOfRom` row goes
missing** — and truncation removes rows from the end, where `EndOfRom` sits. Verified: deleting that one row
from `s4.debug.lst` turns a correct refusal against `s4.bin` into "loading it unverified", after which 1,775
of 1,811 shared symbols name the wrong address. Closed by requiring an unverifiable listing to at least be
internally whole (`is_intact`: section present, footer present, count matching, no unparsed rows) — note
that a footer-count comparison alone does *not* catch this, because truncation usually takes the footer too,
yielding `None` rather than a mismatch.

**Also confirmed against the real file:** 2,129 symbols across **54** modules, footer count exact, zero
unparsed rows, zero duplicate names, and the two halves of the file (body lines and symbol-table rows) agree
on all 2,129 addresses. The type column is `C` for 100% of rows — though sigil's emitter *does* support `-`
for equates (`sigil-link/src/listing.rs`), so §1b's "the emitter dumps no EQU/constants" is a property of
Aeon's current source, not of the format. The parser reads both markers and depends on neither.
