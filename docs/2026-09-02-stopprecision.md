# `stopPrecision` — serving §11.31, and the §8 item 24 proof under it

**Parcel** `parcel/stopprecision` (CR-STOPPREC). **Contract** `empyrean` `82982b7`, protocol §2.1 (the
`stopPrecision` block and its four rules), §3 (the `emulator/stopped` row), §6 (the normative prose),
§8 item 24, and §11.31 (the adjudication of our own CR-E). **Suite rule for the vendor gate**
`empyrean/contract/SUITE_PATHS.md` at `38f6df4`, *"What a resolver owes its reader"*.

We are the only conformant emitter, so nothing about this key was inherited. Everything below was
measured here or is named as unmeasured.

---

## 1. The measured precision, per reason, and how each was measured

The instrument is `oracle_core::testrom::build_stop_precision` — a seven-instruction loop with
interrupts masked, so the PC cannot leave a table of known boundaries:

```text
$000200 main:  move.w #$2700, SR          ; supervisor, interrupts MASKED
$000204 loop:  moveq  #0, D0              ; D0 <- 0
$000206        addq.w #1, D0              ; D0 <- 1     [PROBE]
$000208        addq.w #2, D0              ; D0 <- 3
$00020A        addq.w #4, D0              ; D0 <- 7
$00020C        addq.w #1, D1              ; D1 += 1     [TICK]
$00020E        move.w D1, ($00FF8000).L   ; mem <- D1   [STORE]
$000214        bra.s  loop
```

`SP_PROBE` is item 24's *"instruction with a single observable register effect"*: `addq.w #1, D0` has
no memory traffic and one register, and `D0.w` is `0` **only** at that boundary. `SP_STORE` is the
access-armed probe, whose commit is observable in memory (`mem16 == D1.w - 1` before, `== D1.w` after).
`SP_BOUNDARIES` gives the state that characterises "the instruction at this PC has not executed" for
every boundary, and it is **not a comment**: `testrom.rs`'s own unit test drives the real CPU one
instruction at a time and asserts the machine satisfies the row for whatever PC it is at, so the
Aether-side tests inherit a proven table rather than an assumption.

| `reason` | declared | how it was measured | trials |
|---|---|---|---|
| `breakpoint` | `exact` | `breakpoint_add {addr: SP_PROBE}`, `resume`, halt. The event's `pc` **is** `SP_PROBE` and `D0.w == 0` — the probe has not run. A one-instruction-late server would report `SP_PROBE+2` with `D0.w == 1`. | 8 |
| `runTo` | `exact` | `run_to {addr: SP_PROBE}`. Same discriminator, checked on **both** the event's `pc` and the reply's — the reply is what §2.1 rule 4's readers act on. | 8 |
| `step` | `exact` | `run_to SP_LOOP`, then `step`. Lands at `SP_PROBE` with `D0.w == 0`, which is reachable no other way: it proves the stepped `moveq #0, D0` committed **and** the instruction at the reported `pc` has not. | 1 |
| `watchpoint` | `afterCommit` | `watchpoint_add {addr: SP_STORE_ADDR, stopAfter: 1}`, then `run_frames`. The halt is at the boundary **after** the store, with `mem16 == D1.w` — the write is in memory. §6's *"with the triggering instruction fully committed"*, confirmed rather than quoted. | 1 |
| `runToScanline` | `exact` | No triggering address. §11.31 rules it exact because *"the definition binds `pc`, not the line"*. What was measured is the non-definitional half: the reported `pc` is a real boundary and the machine is in the state that PC implies. | 1 |
| `runFrames` | `exact` | Same; §3 says *"no triggering address, so their value is `exact` by definition"*. Boundary-checked. | 1 |
| `pause` | `exact` | Same. `resume` then `pause` mid-free-run; boundary-checked. | 1 |
| `entry` | **not declared** | This server has **no path that emits it**. See §3 below. | — |

Nothing here rests on a doc comment. `Engine::attribute` carries one that says a breakpoint *"halts at
an instruction boundary before the instruction runs"* and a watch *"halts after a triggering instruction
has committed"*, and both turn out to be true — but item 24 names *"an `exact` declared on the strength
of"* such a comment as **"this amendment's own failure mode committed by its implementer"**, so the
declaration was set from `tests/stop_precision.rs`'s Part A and the comments were treated as a
hypothesis the measurement happened to confirm.

### On repetition, and what it is worth

§2.1 rule 3 is explicit that *"a precision that varies cannot be characterised by sampling"* — the
legacy server's `[0, 0, -98, 0]` over four trials at one address is the measured case. The two
repeatable reasons get 8 trials each, which is enough to catch an *intermittently* early stop of that
kind and is **not** a proof of invariance. What supports the declaration beyond sampling is structural,
and is worth stating because it is the actual argument:

* `BreakStop` raises its flag from `on_step_boundary(pc)` and compares `pc` against the armed set. There
  is no cycle budget, no deadline and no approximation in that path; a stop either happens at a boundary
  whose PC is in the set or does not happen.
* The watch's stop is `stop_requested()` becoming true **during** a step and being honoured at the next
  boundary, which is `afterCommit` by construction and cannot become `exact`.
* Every reason is emitted through one `emit_stopped`, which reads the precision from one registry, so a
  per-stop value that differs from the declaration cannot be produced at all.

---

## 2. The design call: one registry, generated

§2.1 rule 1 requires the handshake map's key set to be *"the set of `reason` values this server can emit
— no more, no fewer — derived from the same registry that produces `methods` and `capabilities`"*.

Before this parcel the reasons were **string literals at fourteen `emit_stopped("…")` call sites**. A
map written beside them would have been a second list, and two lists drift — which is the failure
`methodSummaries`' derivation clause exists to prevent and which this key would have reproduced.

The registry is a `stop_reasons!` macro invocation in `engine.rs`. From one row per reason it generates:
the `StopReason` variant, `StopReason::ALL`, the wire spelling, and the declared `StopPrecision`. Three
consequences, each of which replaces a test that would otherwise have to be remembered:

1. **`emit_stopped` takes a `StopReason`, not a `&str`.** A reason outside the registry cannot reach the
   wire. That is rule 1's "no fewer" half enforced by the compiler.
2. **The handshake map is `StopReason::ALL.iter().map(…)`.** It cannot be edited apart from the emitter.
3. **`precision()` is an exhaustive match.** Adding a reason forces a precision decision at the point of
   adding it, rather than leaving a map entry to be forgotten.

`StopPrecision` is a type with `wire()`/`from_wire()`/`at_least_as_strong_as()` rather than three string
literals, because §2.1 makes the ordering normative (`exact` > `afterCommit` > `approximate`) and an
ordering cannot live in a `&str`. The derived `Ord` has *stronger compares less*, which is exactly the
polarity a reader gets backwards, so every use goes through the named method.

**Why the map is top-level.** Beside `timingBasis` and `limits`, and for their reason: every server that
halts a machine has a stop precision, so it is not something a server may or may not "support". Not under
`capabilities.breakpoints`, which §11.21 deliberately kept a boolean and whose scope was always too
narrow for a key covering `runTo`, `step` and `watchpoint`. Not under `limits`, where every key is a JSON
number by rule.

**This server emits exactly what it declares**, never a stronger value it would also be entitled to emit
under rule 3. A per-stop upgrade would be a second source of truth for one fact.

---

## 3. `entry` is absent, and that is the rule working

§3's enum has eight members; this server has an emitting path for seven. Rule 1 is *"no more, no fewer"*
in **both** directions — *"a server that serves no breakpoints has no `breakpoint` entry, exactly as it
has no `breakpoint_add`"* — and §11.31 kept key-set equality over the CR's own doubt because *"an
over-declared entry is an advertisement for a stop the server cannot produce, the class D4 abolished"*.

The enumeration was **not** taken from a grep over string literals, which is what the brief warned could
miss a computed reason. Two independent closures:

* **Source-level, and now type-level:** `emit_stopped` takes a `StopReason`; there is no `entry` value to
  pass.
* **Runtime:** `the_reasons_this_server_emits_are_the_seven_it_names` drives every method that can halt
  the machine, collects the reasons that actually came out, and asserts the set. That is the "no more"
  direction, which no type can close.

---

## 4. What §8 item 24 covers here

`crates/oracle-aether/tests/stop_precision.rs`, run by the ordinary `cargo test --workspace`.
`item24_every_declared_reason_is_proven_against_the_machine` produces **all seven** declared reasons,
and for each one:

* asserts the event carried `stopPrecision` at all (§3's REQUIRED);
* asserts the value is **not weaker** than the declaration (rider 2);
* asserts the reason **has a map entry** (rider 1, the key-set rule's checkable half);
* runs the register check against **the value the event carried** — which rule 3 permits to be stronger
  than the declaration, so it is the stronger obligation, and checking the declaration alone would let a
  server that declares `approximate` and emits `exact` past.

It then closes the set of reasons it proved against the set the handshake declares, so a version of this
test that quietly proved four of seven goes red rather than green. Item 24 is an anti-vacuity item; a
vacuous implementation of it would have been the worst possible outcome of this parcel.

### What it provably cannot cover

Said plainly so nobody reads the green as more than it is.

1. **`bra.s` has no observable effect.** The boundary table therefore cannot separate a stop reported at
   `SP_BRANCH` from one reported at `SP_LOOP` — they are adjacent through the branch and carry the same
   `(D0.w, mem16 − D1.w)` pair. Every **other** adjacent pair in the loop is separated, so a
   one-instruction error anywhere else is caught. It is only removable by giving the branch a register
   effect, which means a `dbra` and an outer reload, and the residual case affects only the three
   addressless reasons — which have no triggering address to be off by one *from*.
2. **The addressless reasons get half the definition.** `runFrames`, `runToScanline` and `pause` land on
   an arbitrary PC, so item 24's "arm a stop at an instruction" is not available for them. What is
   checked is that the reported `pc` is an instruction boundary and that the machine state is the one
   that PC implies — a real, falsifiable assertion, and less than the probe gives the other four.
3. **Rider 2 is structurally unreachable in this server.** The event's value comes from the same registry
   as the declaration, so an event weaker than its declaration cannot be produced. The assertion is a
   guard against a future divergence, not a live check; it would only start doing work the day someone
   introduces a second source for the value, which is precisely when it should.
4. **The hosted breakpoint halt is not on the wire.** `Engine::halt_on_breakpoint` is reached from two
   run drivers — the engine's own free-run step, which a socket `resume` uses and which these tests
   measure, and the player window's loop through `Host::pump`. Both read the stopping `pc` from the same
   `self.sys` at the same point, but only one is reachable from a socket client, so only one is measured.
   Registered below.
5. **A precision that varies cannot be characterised by sampling.** Section 1 says what carries the
   declaration instead.

### Red-first, both directions

| probe | failure text |
|---|---|
| declare `watchpoint` as `exact` (a server **lying stronger**) | ``` `watchpoint` says `exact`: the store must NOT yet have committed — mem16 == D1.w - 1 at pc 0x0000020E / left: AfterCommit right: Exact ``` |
| drop the `stopPrecision` insert in `emit_stopped` (a server **omitting the key**) | the vendored schema fires first: `events.emulator/stopped.params: $: "stopPrecision" is a required property`. Stripping the key test-side instead shows the assertion bites on its own: ``§3: `stopPrecision` is REQUIRED on every emulator/stopped; this one has none`` |
| filter one reason out of the handshake map (an **under-declared map**) | ``rider 1: `pause` was observed on emulator/stopped and has NO entry in the handshake map``, plus the key-set test's `left`/`right` diff. The schema **cannot** catch this: the map is `minProperties: 1`. |
| arm the breakpoint one instruction past the probe (a **late-stopping server**) | `left: AfterCommit, right: Exact` |
| corrupt one `SP_BOUNDARIES` row | core: `step 0 at 0x00000206: D0.w left: 0 right: 5`; wire: `at pc 0x00000206 the instruction there has not executed, so D0.w must be 0x0005; it is 0x0000` |

---

## 5. A harness race, found because the test repeats

The obvious spelling — `c.ok("emulator/resume")` then `next_stopped(c)` — **races**. On a
seven-instruction loop the breakpoint fires before the `resume` reply is written, the two messages are
produced by different threads, and `Client::ok` reads through to the reply **discarding the events it
passes**. When the halt wins, the event is thrown away and the test blocks until its 20 s socket timeout.

It survived three trials and failed on the fourth. **A single-shot test would have called it green**, and
the only reason it was seen at all is that rule 3's "cannot be characterised by sampling" argued for
repeating the measurement. `resume_and_wait_for_stop` reads both lines before acting on either.

The same spelling appears in `tests/breakpoints.rs` and `tests/watchpoints.rs`. Those are not in this
parcel's scope and their fixtures loop more slowly, but the race is latent there too — registered below.

---

## 6. The vendor gate, reshaped (`F-SCHEMA-READS-LIVE-EMPYREAN`)

The old `upstream_schema_path` walked up from `CARGO_MANIFEST_DIR` and byte-compared
`empyrean/contract/schema/bus-protocol.schema.json` — **a peer's live working tree**. It went red when
the hub saved mid-edit, and would have gone **green against a change no other lane could see**.

The suite ratified the shape for exactly this while the parcel was running, citing the finding by name
(`SUITE_PATHS.md` at `38f6df4`): *"A gate that proves a vendored copy of a peer's CONTENT is fresh reads
the peer through git objects at a named revision, never through the peer's working tree."*

| step | source | proves |
|---|---|---|
| 0 | the vendored bytes, hashed as a git blob against `pin.blob` in `PROVENANCE.md` | the copy is the artifact the pin names. **Never skipped** — needs no peer. |
| 1 | `$AETHER_CONTRACT_SCHEMA`, a file | the pinned bytes equal that file |
| 2 | `$AETHER_CONTRACT_REPO`, a git checkout read only through `cat-file`/`rev-parse`/`merge-base` | the blob is in that repo, the pinned revision carries it at the contract path, and that revision is merged into the committed default branch |
| — | neither set | **nothing about the peer**, printed as a banner naming both variables and both halves. No walk. |

The resolver prints which step answered before checking anything against it, per the same section.

**What was given up.** A default local run no longer notices upstream moving on its own; the automatic
alarm is now the re-vendor discipline plus step 2 under a variable. What it gains is that it never again
reports a verdict about somebody's desk, and it now notices a vendored copy being *edited* — which the
byte-compare against a live tree could not distinguish from upstream having changed.

**A finding from the red-first probe.** Pointing `$AETHER_CONTRACT_REPO` at *this* repository makes
`git cat-file blob <pin>` **succeed**, because vendoring the schema here put the same blob in this repo's
object store. Content-addressing alone says only "some repo has these bytes". The revision and
ancestry checks are what make step 2 mean something, and the probe failed on the second of them. Written
into the code beside the check.

SHA-1 is implemented in `schema_conformance.rs` rather than added as a dependency — this crate's runtime
dep list is deliberately two crates — and is closed against constants from outside this repo before it is
used: FIPS 180-1's `SHA-1("abc")`, git's empty blob, and `git hash-object` on a literal.

---

## 7. Registered, open, or for the foreground

| id | what | why it is not closed here |
|---|---|---|
| `F-STOPPREC-HOSTED-HALT` | The player window's breakpoint halt (`Host::pump` → `halt_on_breakpoint`) is not reachable from a socket client, so its precision is inferred from sharing one function with the measured path rather than measured. | Needs a host-side test, like `host.rs`'s existing `the_bus_and_the_panel_read_one_instrument`. Bounded; not in this parcel. |
| `F-RESUME-STOP-RACE` | `ok("emulator/resume")` + `next_stopped` is latently flaky in `tests/breakpoints.rs` and `tests/watchpoints.rs` for the reason in §5. Not observed failing there — their fixtures take longer to reach the breakpoint — but the mechanism is identical. | Out of this parcel's scope; the fix is to lift `resume_and_wait_for_stop` into `tests/common`. |
| **Upstream-drift alarm** | Step 2 runs only when `$AETHER_CONTRACT_REPO` is set, so no default run notices the contract moving. | A deliberate trade (§6). If the lane wants the alarm back it belongs in a runner or a nightly that sets the variable, never in a walk. |
| `LIVE-TREE-RESIDUE` | `crates/oracle-core/examples/common/rom_source.rs:44` (`LIVE_AEON_DIR`) and `tools/aeon_pin_report.py:145` are the same shape in this repo. | **Explicitly out of scope** for this parcel per the hub. Nothing here makes them worse. |
