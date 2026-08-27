# Disclosing the breakpoint coverage gap — recon

**Date:** 2026-08-27 · **Branch:** `recon/bp-disclose` · **Base:** oracle `d813283` (`main`)
**Contract revision read:** empyrean `d41910e` — `origin/main` at dispatch and still `origin/main` when read
(`git -C ../empyrean rev-parse origin/main` → `d41910e7442e90f5fdfb8b0b49002fa672c3b320`; subject:
*"queue: CR-E (stopPrecision) filed by oracle, queued for adjudication"*). Note empyrean's **local** `main`
is a different ref (`59ff2b7b`); nothing here is cited from it except where §2 says so.
**Nothing was implemented. No CR was drafted. No `cargo` was run** — this parcel is docs-only and another
lane may hold the cargo lane, so the absence of a build gate is the parcel's shape, not a skipped gate.
No emulator was run, no socket dialled, no `mcp__oracle__*` tool touched.

**The question.** The breakpoint surface shipped with one registered coverage gap
(`docs/2026-08-27-breakpoints.md` §9). The server has no declared way to say so on the wire, and this lane
has just filed an undeclared `note` key as a conformance defect against the legacy server (CR-E §1), so
inventing an undeclared key is not available. This document prices the honest options and recommends one.

---

## 0. Headline findings

**The headline, above all others:**

> **Our own breakpoints doc recommends the change that arms this gap.** `docs/2026-08-27-breakpoints.md` §7
> tells the `aeon` lane that `evict_witness.py` sends `timeout_ms` where the contract says `timeoutMs`, and
> asks for the fix. That tool **attaches to a hardcoded `/run/user/1000/oracle.sock`** — the exact path
> `oracle-frontend --aether` binds by default — arms a breakpoint, and waits. Today it is saved *by the
> very bug we asked them to fix*: the snake_case key is refused `-32602` before the handler runs
> (`engine.rs:369`, refusal `:216-220`), so the tool errors **loudly**. Fix the spelling, change nothing
> else, and the same tool falls silently into the coverage gap and reports a false negative about the ROM.
> **The gap is not hypothetical, and the trigger is a one-line change we requested in writing.**

1. **Two of the three facts in my brief are wrong** (§2.1). aeon's breakpoint consumer does **not** spawn
   its own server — it attaches to the well-known socket unconditionally. Aurora's shipping Aether bridge
   does **not** spawn a rebuilt binary — it is pure attach; the "rebuilt binary" it spawns is the *ROM
   build script*. Only the MCP-shim fact held.

2. **§9's cost estimate for closing the gap is wrong on both halves.** §9 says a halt on the player's path
   *"needs the player to pause itself and the engine to emit the `stopped`, neither of which exists"*.
   **Both exist.** The player already follows the bus's pause state every iteration (`main.rs:1827-1836`,
   reading `Host::is_paused`), and `Engine::emit_stopped` already exists and is already what
   `free_run_step` calls on a halt (`engine.rs:1710`, called `:1243`). The engine's own doc comment on the
   `breakpoints` field states it correctly — *"a halt it does not currently look for"* (`engine.rs:711-717`)
   — so **the doc comment and §9 disagree, and the doc comment is the accurate one.**

3. **The real cost is a third thing §9 never names.** A `stopped` emitted outside a pump window would be
   stamped `frame 0, mclk 0` from the placeholder `System`, because `Engine::emit` stamps whatever machine
   the engine is holding (`engine.rs:1702-1705`). That is a D11 lie. It has an exact in-tree precedent and
   remedy: `Host::pending_free_run` defers for this identical reason (`host.rs:179-186`, applied `:419-421`).

4. **The gap is sharper than §9 states, in a way §9's wording hides.** All **seven** advancing methods call
   `require_paused` — `run_frames` (`engine.rs:2174`), `run_to` (`:2201`), `run_to_scanline` (`:2319`),
   `step` (`:2421`), `step_over` (`:2450`), `step_out` (`:2470`), `press` (`:4129`) — and hosted, `free_run`
   *is* "the player is not paused" (`host.rs:258-274`). **There is no hosted state in which a breakpoint
   can fire while anyone is playing**, and the documented consumer idiom `resume` → `wait_for_break` is
   exactly and only the broken path.

5. **The failure a client experiences is the worst class this suite ranks.** The reply is
   `{"timeoutReached": true}` with envelope `running: true` (`engine.rs:5444-5450`) and `breakpoint_list`
   showing `hits: 0` — **indistinguishable** from "the ROM never reached that address", with `hits: 0`
   actively corroborating the wrong reading. A believable wrong answer about the **program under test**.

6. **§9's path anchors are imprecise.** The advance calls in `Host` (`host.rs:780,782`) and `bus.rs`
   (`:330,332`) are inside `#[cfg(test)]` (boundaries `host.rs:498`, `bus.rs:250`). In production exactly
   one advance site misses the breakpoint: `main.rs:1734/1746/1753`.

7. **No declared mechanism exists** (§3) — but the contract already recognises this *class* of statement
   in `capabilities.watchpoints.spaces`, and it is unavailable to breakpoints only because
   `capabilities.breakpoints` is a frozen boolean.

---

## 1. The mechanism, from source at `d813283`

### 1.1 The sink — two construction sites, both bare

`BreakStop` is constructed in exactly two places, found by enumerating the constructor rather than by
trusting §9:

```
crates/oracle-aether/src/engine.rs:1211-1213   in free_run_step
crates/oracle-aether/src/engine.rs:1429-1431   in advance_with
```

Both attach it **unwrapped**. That is load-bearing: `Observe` overrides exactly one hook —
`fn stop_requested(&self) -> bool { false }` (`oracle-core/src/bus.rs:506-509`, comment *"The whole point:
every capability query forwards, this one does not"*) — so an `Observe`-wrapped `BreakStop` would still
latch `fired` from `on_step_boundary` while never ending the run. §9's *"would count hits without halting"*
is directionally right; precisely, `fired` latches and `record_halt` would then count a halt that never
happened — the same believable-wrong-answer class as the gap itself.

### 1.2 Every production path that advances the machine

Enumerated by grepping the **advance primitives** (`run_frames_with_sink`, `run_frames`, `run_until_stop`,
`step_instruction`, `boot_with_sink`, `reset_with_sink`) across `*.rs`, then classifying each hit against
its file's `#[cfg(test)]` boundary — not by grepping for the breakpoint.

> Method note, recorded because it nearly produced a confidently wrong enumeration: my first attempt used
> the pathspec `crates/*/src`, which returned **empty with exit status 1**. `engine.rs` obviously does call
> `run_frames_with_sink`, so the emptiness was a broken pathspec, not an empty world. Re-asked as
> `-- '*.rs'` (exit 0).

| # | production advance site | who drives it | breakpoint rides? |
|---|---|---|---|
| 1 | `engine.rs:1218` (`free_run_step`) | standalone server loop's `None` branch, `server.rs:469` | **YES**, bare |
| 2 | `engine.rs:1439` (`advance_with`) | every bounded advance, **both** arrangements | **YES**, bare |
| 3 | `main.rs:1734 / 1746 / 1753` | the windowed player's own loop | **NO** |
| 4 | `oracle-replay/src/runner.rs:565, 656` | the headless `replay_runner` binary | **NO** — ruled out below |
| 5 | `engine.rs:4538` (`reset`), `:4570` (`reload_rom`) → `System::reset` → `reset_with_sink(&mut ())` (`system.rs:458-459, 511`) | `emulator/reset`, `emulator/reload_rom` | **NO** — benign, below |

Test-only advance sites, named so the exclusion is checkable: `host.rs:535, 780, 782`; `bus.rs:328-332`;
`lens/mod.rs:1583`; `lens/profile.rs:264`; all of `save_state.rs`; `main.rs:2369` and below. `cfg(test)`
boundaries: `host.rs:498`, `bus.rs:250`, `lens/mod.rs:449`, `lens/profile.rs:242`, `save_state.rs:284`,
`main.rs:2090`, `runner.rs:826`, `engine.rs:6605`.

**Row 4 is not a surface gap.** `oracle-replay`'s `[dependencies]` is one line — `oracle-core` only — so
there is no `Engine`, no bus and no breakpoint set in that process. It advances a machine no breakpoint
client can address. Enumerated because the bar was "what touches the data"; ruled out on its dependency
graph, not on intent.

**Row 5 is benign, and is what makes a documented idiom safe.** `System::reset` runs the power-on recipe
over the bus through a null sink, so a breakpoint cannot trip on it. That is *desirable*: the `breakpoints`
field doc cites the `aeon` `evict_witness` idiom — *"arm a breakpoint, then `reload_rom`, then wait"*
(`engine.rs:709-711`) — which a spurious fire during the reset recipe would break.

**So §9 is right in substance for row 3 and wrong in its anchors.** The one production advance that misses
the breakpoint is the player's loop, and it misses it because `Engine::run_sinks` returns a 2-tuple of
`(watch, profiler)` (`engine.rs:1297-1309`); the player calls it at `main.rs:1726` and fans out those two.

### 1.3 Why the hosted gap has no window in which it does not bite

`Host::set_paused`'s doc states the identity plainly: *"an un-paused player **is** a free-running bus"*
(`host.rs:258-266`). `Engine::require_paused` refuses on `self.free_run` (`engine.rs:2116-2124`). All seven
advancing methods call it. Therefore, hosted:

* **window running** → `free_run == true` → every advancing method refused `-32005 machineRunning`; the only
  thing moving the machine is `main.rs:1734`, which carries no breakpoint. **Breakpoints are inert.**
* **window paused** → `free_run == false` → bounded advances succeed and carry the breakpoint (row 2).

There is no third state. `emulator/resume` hosted does not start a bus-driven free-run — it clears the
*player's* pause (`main.rs:1827-1836`, which prints *"aether: resumed by a client"*), handing the machine
to the one loop that cannot see a breakpoint. `free_run_step` (row 1) is **never called hosted**: its only
caller is `server.rs:469`, on the standalone engine thread. The exposure sweep corroborated this
independently — `git grep -nE 'breakpoint' -- crates/oracle-frontend/src/` returns **exit 1, zero hits**.

### 1.4 What closing it would actually require

| # | what must change | exists today? | anchor |
|---|---|---|---|
| i | the player's run carries a `BreakStop` | no | `run_sinks` returns 2 elements (`engine.rs:1297`) |
| ii | the sink needs the run's starting PC | **problem** | `BreakStop::new(set, resume_pc)`; outside the pump the engine holds the placeholder `System`, so it **cannot read the real PC itself** — the caller must supply it |
| iii | the fired observation handed back and counted | no | `record_halt` is engine-side (`engine.rs:1223, 1479`) |
| iv | both run flags clear on the halt | **mechanism exists** | `free_run = false; running = false` (`engine.rs:1235-1236`) |
| v | the player pauses itself | **EXISTS ALREADY** | `Host::is_paused` (`host.rs:279-281`) → `main.rs:1827-1836` |
| vi | the engine emits `stopped` | **EXISTS ALREADY** | `emit_stopped` (`engine.rs:1710`) |
| vii | the `stopped` is stamped truthfully | **problem §9 omits** | `emit` stamps the held machine (`engine.rs:1702-1705`); outside pump that is `frame 0, mclk 0` |
| viii | `set_paused` must not resurrect `free_run` after a halt | **problem §9 omits** | the loop calls `bus.set_paused(paused)` **before** `pump` (`main.rs:1795-1796`); with the player un-paused, `set_paused` computes `want = true`, sees `free_run` now false, and queues `pending_free_run = Some(true)` (`host.rs:271-274`) — undoing the halt |

(v) and (vi) are the two §9 said do not exist. (vii) and (viii) are the genuine costs, and (vii) has an
exact precedent in the same file: `pending_free_run` exists *precisely* because `set_free_run` emits events
and *"outside the window the engine holds the placeholder, so an event emitted there would be stamped
`frame 0, mclk 0` — a lie about the exact instant a client most needs the truth about"* (`host.rs:180-186`).
The remedy is the same shape — latch the halt, apply it at the top of the next `pump` right after
`swap_system` (`host.rs:417-421`) — which also resolves (viii) by ordering the halt *after*
`pending_free_run` so the halt wins.

(ii) is the one API change with no precedent to lean on: `run_sinks` gains the caller's PC, touching the
served build (`engine.rs`, `host.rs`, `bus.rs`), the stub build (`bus_stub.rs:123`, whose doc insists *"the
one place the two builds must not differ is the shape of what the loop attaches to its run"*) and
`main.rs`. Borrow-wise a 3-tuple is sound: `BreakStop` borrows `self.breakpoints` **shared** while the
other two borrow their own fields mutably, and the fields are disjoint.

**Verdict on §9(b): the claim is wrong and the work is bounded** — one signature change carrying a
parameter, one latch field, one apply-at-pump-top branch on an existing precedent, one ordering rule, plus
tests. ⚠ Not compile-verified; this parcel ran no `cargo`. **TAGGED for foreground follow-up.**

---

## 2. The exposure

Swept as `git grep -nE … HEAD -- . ':(exclude)vendor/**' ':(exclude)node_modules/**' ':(exclude)target/**'
':(exclude).claude/worktrees/**'` — searching `HEAD` rather than the worktree, so only tracked files at a
committed revision are read. Two independent sweeps were run (identifiers, and quoted namespaced wire
keys); the quoted sweep added nothing the identifier sweep had not caught. Exit status was checked on every
empty result.

Revisions swept: aeon `7511a440`, aurora `7743a12b`, seraph `1920083c`, sigil `e9ef00b5`, empyrean
`59ff2b7b` (local `main`), oracle-old `58b6f81f`.
⚠ **sigil's HEAD advanced three times during the sweep** (another session committing live); its citations
are a snapshot at `e9ef00b5`.

### 2.1 Two of the three supplied facts are wrong

| supplied fact | verdict |
|---|---|
| *"aeon's tools reach the emulator via `tools/aether_instance.py` → `BusClient`, spawning the Rust server directly"* | **WRONG for the breakpoint consumer.** `aeon/tools/evict_witness.py:49` hardcodes `SOCK = "/run/user/1000/oracle.sock"` and connects at `:76` with no spawn, no env var, no flag, and no `aether_instance` import. Its own docstring (`:37`) says *"Requires one running oracle_gui (socket /run/user/1000/oracle.sock)."* `aether_instance.py` is spawn-only and correct — but it is **not the path the breakpoint consumers take**. |
| *"aurora spawns its own rebuilt binary"* | **WRONG.** `aurora/src/main/aether/bridge.ts:108,118` resolves `$ORACLE_SOCKET` → `$EXODUS_SOCKET` → `$XDG_RUNTIME_DIR/oracle.sock` → `/tmp/oracle.sock` and `net.connect`s it. It **never spawns an emulator**. What it spawns (`build-run.ts:123,232`) is the **ROM build script**, then `reload_rom`s over the attached client. |
| *"the MCP shim spawns its own private `oracle-aether` by default and only ATTACHes when `$ORACLE_SOCKET`/`$EXODUS_SOCKET` is set"* | **HOLDS.** `oracle-old/linux-port/mcp/oracle_mcp.py:131-138` reads only those two vars; `:297` spawns into `mkdtemp("oracle-mcp-")` otherwise. Its docstring `:29-32` names the intended attach target: *"e.g. the windowed player launched with `--aether`."* |

### 2.2 Per-consumer verdicts

| consumer | calls | connects how | reaches a hosted player? |
|---|---|---|---|
| `aeon/tools/evict_witness.py` | `breakpoint_add` `:85`, `wait_for_break` `:101`, `breakpoint_clear` `:121` | **ATTACH**, hardcoded `/run/user/1000/oracle.sock` `:49,76` | **YES — unconditionally.** See §2.3 |
| `aeon/tools/parallax_hscroll_probe.py` | bp_add `:584`, wait `:592`, bp_clear `:575/594/598/600` | SPAWN, `headless_emulator` `:1013` | No |
| `aeon/tools/raster_frame_epoch_probe.py` | bp_add `:220/221`, wait `:228`, bp_clear `:219/258` | SPAWN, `headless_emulator` `:420` | No |
| 16 files under `sigil/crates/sigil-harness/golden/ab/{a3,g9,waveb,wavec}/` | `breakpoint_add` / `wait_for_break` / `breakpoint_clear` | **ATTACH**, `BusClient(...)` with **no `socket_path`** → `resolve_socket_path()` default | **Path yes, vocabulary no** — they send **bare** method names (`"breakpoint_clear"`), and `BusClient.call` does no prefixing, so against our server they die `-32601`. Against the **legacy C++ `oracle_gui`** windowed player they are a live default-path breakpoint attach. |
| `aurora/src/main/aether/bridge.ts` | **no breakpoint methods** — recorded as unimplemented in `aurora/docs/reviews/2026-08-22-oracle-instrument-gaps.md:134` | ATTACH, default resolution | Reaches a hosted player; not yet a breakpoint consumer |
| `oracle-old/linux-port/mcp/oracle_mcp.py` | bp_add `:613`, bp_list `:622`, bp_clear `:628`, wait `:436` | SPAWN by default; ATTACH iff `$ORACLE_SOCKET`/`$EXODUS_SOCKET` | **YES, on the env var** — and its docstring names that as the intended use |
| `seraph` | **zero hits**, sweep exit 1 | — | No |
| `empyrean` | 9 hit files, **all** spec/schema/vectors/docs — no executable consumer | — | No |

### 2.3 Does anything hit the gap today?

**One consumer is aimed straight at it and is saved only by an unrelated bug we asked to have fixed.**

`evict_witness.py` attaches to the hosted player's default path and arms a fully-qualified
`emulator/breakpoint_add`, which our server accepts. It then sends `{"timeout_ms": 60000}` (`:101`). Our
params set is closed (`engine.rs:369 params: &["timeoutMs"]`), so the call is refused `-32602` **before
the handler runs** (`:216-220`), and `server.rs:681-706` deliberately gives the snake_case spelling a zero
sleep so the refusal is not preceded by a wait. **It errors loudly today.** The file's own comment
(`:93-97`) says the spelling is pinned to the legacy server on purpose and *"the migration moves both
halves together"* — and `docs/2026-08-27-breakpoints.md` §7 asks that lane to move it. **The day the
spelling is fixed, this tool falls into the gap with nothing else changed**, and `evict_witness.py:97`
reads `r.get("timeout_reached")` — also snake_case — so it will read `None` from a timed-out wait and
**print no failure at all**. Silent false negative, on a witness tool.

**This configuration is routine on this machine, not hypothetical.** `aurora/docs/OVERSEER.md:805` records
`/run/user/1000/oracle.sock` held by `oracle-frontend` pid 1542676 (`--aether --x11`, the owner's player)
with seven MCP shims live-attached; `empyrean/docs/lane-log.jsonl:42` corroborates independently;
`aurora/docs/reviews/2026-08-27-band-lens.md:20` records another (pid 2768705). Launch is documented in at
least five places, including `oracle/README.md:96-100`, `empyrean/clients/typescript/scripts/smoke.mjs:1`,
`empyrean/docs/superpowers/plans/2026-08-20-scribe-v1.md:21`, `aurora/docs/OVERSEER.md:818-819`, and —
dated **yesterday** — `aeon/docs/research/2026-08-27-fg-left-edge-reproduction.md:93`, which *proposes*
attaching a probe to the owner's live `oracle-frontend` socket.

**Conclusion: the gap is reachable, reached by a documented and recorded configuration, and one requested
one-line change away from producing a silent false negative in a live tool.**

---

## 3. What the contract already offers

Read at empyrean `d41910e`. Our vendored schema
(`crates/oracle-aether/tests/contract/bus-protocol.schema.json`) is **byte-identical** to
`d41910e:contract/schema/bus-protocol.schema.json` (`diff -q`, exit 0), so quotations from either are
quotations of `d41910e`. Verified firsthand: `stopPrecision` appears **0 times** in both contract files —
CR-E is filed and queued, not landed.

### 3.1 `caveat` — **OUT**, declared absent on all five fragments

§2.4 defines it as *"A human-readable statement that the reply is less trustworthy, less complete or less
direct than its shape suggests"*, narrowed by §11.20 to *"It may be emitted only on a result whose schema
fragment declares it… emitting one there is a conformance failure, not a courtesy."* Rule 3 independently
disqualifies it as a carrier: *"Clients MUST NOT parse it… Any consequence a client must act on needs its
own typed key."*

All five fragments declare it absent. ⚑ **But my brief was wrong that each cites §11.20 — only four do.**
The four breakpoint rows say *"caveat is declared ABSENT (the sprites / write_memory / read_cram
precedent, §11.20)"* / *"(the family's own precedent, §11.20)"*. `wait_for_break` cites **§2.4 rule 3** and
gives a different reason:

> *"caveat is declared ABSENT: `timeoutReached` is the typed key §2.4 rule 3 asks for, so the weak answer
> here already has a machine-readable discriminant."*

**That sentence is the most important one in the contract for this problem, and it cuts both ways.** The
contract's position is that a weak `wait_for_break` answer is already honestly discriminated. True *for the
case the key was designed for* — the wait expired. **False for a coverage gap:** `timeoutReached: true`
cannot distinguish "the code never ran" from "this arrangement cannot see it run". The contract considered
the weak answer on this exact method and concluded no further disclosure was needed — without the coverage
case in view.

(Enforcement note: none of the five result nodes carries `additionalProperties`/`unevaluatedProperties`.
The prohibition is enforced **at test time only**, by §8 item 20's `unevaluatedProperties: false` applied
by the harness and *"deliberately NOT written into the published schema"*.)

### 3.2 `capabilities.breakpoints` — **OUT**, frozen boolean

Schema: `{"type": "boolean", "description": "Whether the breakpoint family (§6) is served."}`. §11.21
design choice 3: *"the breakpoint capability is already a **boolean** that shipping clients read, and
§11.18 says an emitted shape cannot be widened under a client that already parses it."* §6 repeats it
normatively. The controller's reading holds.

⚑ **One nuance worth recording:** §11.18's literal text is about widening an **enum's value set**, not
about retyping a published key. The general rule is real and thrice-stated (here, in the schema's
`objectDecoders` description, and in §11.25's D4) — but §11.18 is its *cited home* rather than its literal
source. This does not change the outcome.

Note also that `breakpoints: true` remains **truthful** under the gap: the family *is* served and every
method answers. The boolean is not lying; it is answering a different question.

### 3.3 `limits` — **admits new keys, but is the wrong category**

Structurally open: `additionalProperties` and `unevaluatedProperties` are both **absent**, `required` is
only `["maxRunFrames","maxReadLen","maxLineBytes"]`, and §8 item 20's closure is **top-level only** —
*"Objects nested in a result are closed only where their own published subschema closes them"* — so
`limits`, being nested in the `initialize` result, is not closed even at test time. And `maxBreakpoints` is
an explicit precedent for *displacing a fact out of a frozen boolean capability into `limits`*.

**But** all eleven keys (`maxRunFrames`, `maxReadLen`, `maxLineBytes`, `maxInputRows`, `maxWriteLen`,
`maxHashLen`, `maxBreakpoints`, `maxProfilerRoutines`, `maxProfilerFrames`, `maxProfilerCallers`,
`maxObjectSlots`) are `{"type": "integer", ...}`, and §2.1's prose says *"Three fields, all required, **all
JSON numbers** (D9 category 2)"*. A coverage scope is not a ceiling and has no number. CR-E §7(d) rejected
`limits` on this identical ground; my independent enumeration confirms its premise.

### 3.4 The `stopped` event and its `reason` enum — **OUT, structurally**

The enum is closed and complete: `breakpoint`, `watchpoint`, `step`, `runTo`, `runToScanline`, `runFrames`,
`pause`, `entry`. But the enumeration is beside the point: **under the gap no `stopped` event is ever
emitted**, so there is no message for a qualifier to ride. The event cannot carry this fact even in
principle. This is also the cleanest statement of how the gap differs from CR-E (§3.6).

### 3.5 `-32005` refusal — **OUT for disclosure**

The declared vocabulary is `machineRunning`, `checkpointCapReached`, `unknownCheckpoint`,
`watchCapReached`, `breakpointCapReached`, `unknownBreakpoint`, plus `callersNotArmed` (§11.18). The type
is an open `{"type":"string"}`, but every member was registered by amendment.

It cannot carry this. `-32005` answers *a call*, and the gap has no call at the moment it bites:
`breakpoint_add` succeeds and the failure is that nothing fires later. §6 additionally pins arming as legal
during a run — *"Not subject to the run-control state rule: arming, toggling and clearing mutate an
observer, not the timeline, and are legal while running"* — so refusing `breakpoint_add` because a host
loop is driving would contradict that pin. Refusing `wait_for_break` instead would break the standalone
arrangement, where `resume` → `wait_for_break` is correct; the refusal would have to be conditional on the
arrangement, which is the disclosure problem restated one level down.

Worth noting that a related refusal **already fires**: hosted with the window running, every advancing
method is refused `-32005 machineRunning` (`engine.rs:2118-2122`). A client that tries to *drive* the
machine gets a loud typed answer today. Only `wait_for_break` — which has no `require_paused` — returns a
successful weak reply.

### 3.6 `stopPrecision` (CR-E) — a **sibling**, not the same shape

CR-E proposes an ordered enum `exact | afterCommit | approximate`, declared per `reason` as a top-level
handshake key **and** REQUIRED on every `emulator/stopped`, with a binding rule that an event may exceed
but never fall short of the declaration (§4.1-4.4).

**Same:** the failure class (*"a believable wrong answer rather than an error"*); the placement analysis —
both are barred from `caveat` by §2.4 rule 3, from `capabilities.breakpoints` by §11.21 choice 3, and from
`limits` by the all-integers property; and the presence-is-the-discriminator device. CR-E has already done
that entire elimination and its reasoning transfers wholesale.

**Different, decisively — four ways:**

1. **Accuracy vs occurrence.** Every `stopPrecision` value, including `approximate`, describes a stop that
   **occurred**. This gap is about whether a stop happens at all. They are orthogonal: a server can be
   `exact` on every stop it produces and produce none during a host free-run.
2. **A carrier problem CR-E does not have.** CR-E's Level 2 works because the fact coincides with a
   message. This fact's entire content is the **absence** of a message, so a handshake declaration for
   coverage would be **unfalsifiable by observation** in a way CR-E's is not — a materially different
   adjudication problem.
3. **Wrong key axis.** CR-E's map is keyed by `reason` — *what condition ended the run*. This gap is keyed
   by *who was driving the machine*, an axis the contract does not have: §3/§11.7 pin that *"`reason` names
   the condition that ended the run, never the method that drove it."*
4. **It cannot be a fourth enum member.** §4.1 makes the enum **totally ordered** and §4.4's binding rule
   depends on the ordering. "Never fires under condition X" is not a weaker stop; it is not a stop.

So: **a sibling sharing a home, a discriminator device and an adversary — but answering coverage where
CR-E answers precision.** They compose, and if both ever land they belong in one amendment. Note the
sharpest consequence: a client reading `stopPrecision: {"breakpoint": "exact"}` and getting no event at all
has been misled **by CR-E's own key** — an exactness promise about a stop the server cannot produce there.

### 3.7 What *does* exist, and is the nearest precedent: `capabilities.watchpoints.spaces`

The one genuinely scope-shaped declaration on this bus:

> *"Which address spaces this server can watch. **Advertised rather than assumed**: a server with no
> VDP-internal write capture supports only 'bus', and **a client must not have to arm a watch to find
> out**."*

That is exactly the statement class wanted — *the scope over which an instrument works, declared so a
client need not discover it by silence*. It is unavailable here only because it rides an **object-valued**
capability, and `capabilities.breakpoints` is frozen as a boolean (§3.2). **The contract already believes
in this kind of statement; the breakpoint family just cannot express it.**

Two further near-misses: `timingBasis` shows the handshake is the registered home for declarative
statements about how a server operates (*"A top-level key rather than a capability flag: it is not
something a server may or may not support, it is what that server's stamps are expressed in"*), and §8
item 23's `methods`-as-warranty is explicitly binary — *"Item 23 governs name resolution only. It does NOT
require the call to succeed."*

**Conclusion: no declared mechanism exists.** The bus can say *whether* a family is served, *how much* of
it, *which sub-domains* an instrument covers (only where the capability is an object), and *what a reply
assumed* — but it has no way to say **under which run conditions a stop-shaped guarantee holds.**

---

## 4. The options, priced

**The failure mode, stated once, because options (a), (c) and (d) all leave it intact.** Client arms a
breakpoint against a hosted player, calls `resume`, calls `wait_for_break`, receives
`{"timeoutReached": true, "running": true, …}` with `breakpoint_list` showing `hits: 0`. Every field is
true; the composite is a lie. It is the exact reply for "the ROM never reached that address", and `hits: 0`
corroborates it. A client acting on it deletes a test, files a bug against the ROM, or declares a code path
dead. **A believable wrong answer about the program under test** — worse than the class CR-E addresses,
which misdescribes the emulator's own stop rather than the user's software.

### (a) A CR to empyrean for a declared handshake disclosure

**Cost.** A full CR drafted, filed, adjudicated, then implemented. CR-E is 941 lines and **still queued
unadjudicated** at `d41910e`. A second stop-shaped CR lands on the same desk, overlapping the first in
placement, discriminator device and rationale.
**Buys.** The only option that reaches a client which never reads our repo, and the only one that helps a
second implementation. §3.7 shows the contract already endorses the statement class.
**Binds.** Every server implementing the amendment, to a vocabulary describing a hosted/standalone split
that is **our architecture**; `oracle-cpp` has no such split. Weakest point: standardising a distinction
one implementation currently has.
**Client experience if this alone ships.** Gap intact; a client that reads the handshake avoids it, one
that does not gets the wrong answer unchanged. **Crucially, `evict_witness.py` does not read the
handshake for this** — nothing would make it check.

### (a′) Make the silence legible on the instrument — a counter, not a declaration

Surfaced by the contract sweep and worth recording, because it is what the contract *actually did* for the
analogous pathology. §11.8's named disease is *"a silently-dropped watch produces a `seen`-positive,
`matched`-zero reading that reads exactly like a negative finding"* — and the cure was **counters that make
silence legible on the instrument** (`seen`, `dropped`, `matched`, `first`/`last`), not a handshake word.
The breakpoint analogue would be a per-breakpoint "runs this instrument actually rode" counter, so
`hits: 0` with a zero observation count is distinguishable from `hits: 0` with a positive one.
**Cost:** still a new key, so still a CR — but a smaller one, on a stronger and directly analogous
precedent, and it is *falsifiable by observation* in the way §3.6(2) says a handshake declaration is not.
**Still leaves the gap**, and still requires the client to look.

### (b) Close the gap — wire the sink into the player's loop

**Cost.** §1.4: one signature change carrying `resume_pc` (propagated to `bus_stub` for shape parity); a
latch field on `Host`; an apply-at-pump-top branch on the `pending_free_run` precedent; one ordering rule;
tests including a hosted fixture that arms, resumes, and asserts the window actually pauses. **Materially
cheaper than §9 claims**, because the two prerequisites §9 called nonexistent already exist.
**Buys.** The gap stops existing. No CR, no vocabulary, no binding of any peer. `resume` →
`wait_for_break` then works identically in both arrangements — which is what `capabilities.breakpoints:
true` already implies to every client reading it today.
**Binds.** Nobody outside this repo. Zero contract surface movement.
**Client experience.** Correct: the breakpoint fires, the window pauses with the existing *"aether: paused
by a client"* notification, `stopped` arrives with `reason: "breakpoint"`, and `pc` is exact (breakpoints
doc §8).
**Risk.** The D11 stamping and the `set_paused` ordering must be got right; a halt applied in the wrong
order is a machine that pauses and instantly resumes — a *new* believable wrong answer. Mitigated by both
having an in-file precedent to copy.

### (c) Documentation only

**Cost.** Near zero — already written (§9 and `engine.rs:711`).
**Buys.** Nothing a client can act on. To reach a client that never reads this repo it would have to live
in empyrean's contract prose, which **is** option (a) minus the schema, and the contract does not carry
per-implementation errata.
**Client experience.** Unchanged, and for the one consumer actually aimed at the gap, **silent** — because
`evict_witness.py:97` reads snake_case `timeout_reached` and would print no failure. **Fails the
failure-mode test outright.**

### (d) Refuse instead of disclosing

Have `wait_for_break` refuse `-32005` when free-running *and* this server cannot observe breakpoints there.
**Rejected** — §3.5: the same reply must stay a *success* on the standalone server, so the refusal is
conditional on the arrangement, which restates the disclosure problem; and it would break
`wait_for_break`'s contractual role as the retained polling path for clients without `events`.

### (e) Advertise less — `capabilities.breakpoints: false` when hosted

**Rejected**, recorded because it is superficially the cheapest honest move using only declared vocabulary.
It is a lie in the other direction: the family *is* served, four of five methods work perfectly hosted, and
§11.21 design choice 4 makes `breakpoint_set_enabled`'s **presence in `methods`** the discriminator for the
handle surface. It would strip a working surface from every client to describe one broken path.

---

## 5. Recommendation

**Do (b) — close the gap — and do not file a CR. Treat it as urgent rather than as a follow-up.**

1. **The gap is a defect, not a property.** All five methods are served and answer; the surface's stated
   semantics (`capabilities.breakpoints: true`, §6's unqualified halt prose) already promise what the
   hosted player fails to deliver. A disclosure key would formalise a promise gap the code can simply stop
   having — and the contract's own instinct runs this way: `wait_for_break`'s fragment declines `caveat`
   because the honest typed answer was already available.
2. **§9's cost estimate — the sole basis for scoping this out — does not survive contact with the source.**
   Two of its three prerequisites already exist and are wired end to end. The deferral was priced wrong.
3. **The exposure is live and the trigger is ours.** §2.3: a real `aeon` tool attaches to the hosted
   player's default socket, arms a breakpoint, and is saved today only by the `timeout_ms` spelling bug
   that *our own §7 asked them to fix*. That makes (b) time-sensitive in a way a disclosure key would not
   address — nothing in (a) would make `evict_witness.py` check anything.
4. **(a) would standardise our architecture into a shared contract** while its sibling CR-E is still
   unadjudicated. Two overlapping stop-shaped CRs is a real cost to the adjudicator and a real risk of two
   keys that should have been one.
5. **(c) leaves the worst failure intact and, for the concrete consumer, silent.**

**Sequencing.** (b) is additive and binds nobody, so it need not wait for CR-E. **Interim, at zero cost:**
`docs/2026-08-27-breakpoints.md` §7's request to the aeon lane should carry a warning that fixing
`timeout_ms` → `timeoutMs` *while the gap is open* converts a loud refusal into a silent false negative —
so the two changes must be sequenced, gap first. If (b) proves harder than §1.4 prices it — specifically if
the `set_paused` ordering cannot be made safe without a larger rework — then **(a′) is the better fallback
than (a)**: it rides §11.8's directly analogous precedent, is falsifiable by observation, and is a smaller
amendment. If a declaration is ultimately wanted, it should be folded into CR-E as a second key in one
amendment (§3.6), not filed separately.

### The weakest point, named

**I could not compile or run anything, so §1.4's price is a reading of the source, not a measurement.** The
item most likely to be wrong is (ii): `run_sinks` must gain a `resume_pc` parameter because the engine
holds the placeholder `System` outside the pump window. I verified the placeholder from `Host::new`
(`host.rs:194-196`, *"an inert placeholder `System`"*) and from `pending_free_run`'s doc, but did not
verify that no other route lets the engine see the real PC at that moment. If one does, (b) gets cheaper;
if the borrow-checker rejects the 3-tuple for a reason my disjoint-fields reasoning missed, (b) gets dearer
and the recommendation weakens toward (a′).

**Second weakness, and the honest one:** declining (a) rests on the judgement that this is *our* defect
rather than a general property of hosted emulators. §3.7 cuts against me — `capabilities.watchpoints.spaces`
shows the contract already thinks scope-of-an-instrument is a first-class thing to declare, which is
evidence that a coverage declaration is legitimate rather than parochial. And sigil's 16 AB runners target
the **legacy C++ windowed player**, which may have the same gap (unsettled, §6) — if it does, the
distinction is already general and declining to declare it is the wrong call. I still recommend (b) first,
because a defect fixed needs no vocabulary and a vocabulary can be added later by whoever needs it; but
this is the argument that would change my mind.

---

## 6. What I could not settle, and why

* **Nothing was compiled, run, or dialled**, per this parcel's standing invariant. **TAGGED for the
  controller's foreground follow-up:** (i) that a 3-element `run_sinks` borrow-checks; (ii) that the
  halt-applied-at-pump-top ordering produces exactly one `stopped` and leaves the window paused; (iii) a
  live reproduction of the gap against a hosted player, which is the one thing that would turn §1.3's
  argument into a measurement.
* **Whether any player is listening on `/run/user/1000/oracle.sock` right now.** `ORACLE_SOCKET` is unset
  and `XDG_RUNTIME_DIR=/run/user/1000`, so the resolver *selects* that path — but presence is a
  live-process fact and no socket was stat'd or dialled. **UNSETTLED by instruction.**
* **Whether the legacy C++ `oracle_gui` has the same hosted-free-run gap.** This decides whether sigil's 16
  AB runners are exposed and, per §5's second weakness, whether the distinction is general enough to
  deserve a contract key. Would need `oracle-old/linux-port/gui/ControlSocket.cpp` traced. **UNSETTLED —
  and it is the single question most likely to change the recommendation.**
* **§11.20's own prose** was read via the sweep's quotation rather than independently by me; the four
  fragments' citations of it were confirmed verbatim.
* **sigil's HEAD moved three times during the sweep**; its citations are a snapshot at `e9ef00b5`.
