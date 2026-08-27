# The breakpoint surface, served

**Date:** 2026-08-27 · **Branch:** `parcel/breakpoints` · **Base:** `a3917fe` (two commits past the
`parcel/write-vram` merge, `0517955`)

**Contract revision read: `empyrean` `origin/main` `82eb397`.** Not the `5625683` the dispatch named — it
had moved again by the time this lane opened, which is the fourth time this document's predecessors have
had to say so. Everything below is quoted from `git show origin/main:contract/protocol.md` and
`…:contract/schema/bus-protocol.schema.json` at that revision, never from the peer's working tree. The
vendored schema at `crates/oracle-aether/tests/contract/bus-protocol.schema.json` is **byte-identical** to
`82eb397`'s (verified by `diff`), so the prose moved between `5625683` and `82eb397` and the schema did
not — no re-vendor was needed.

**Acceptance delta: 15 → 10.** Five methods, all previously unserved:
`emulator/breakpoint_add`, `emulator/breakpoint_set_enabled`, `emulator/breakpoint_list`,
`emulator/breakpoint_clear`, `emulator/wait_for_break`.

---

## 1. What the contract requires, quoted

### The four breakpoint rows (§6, amended by §11.21 on 2026-08-26)

| Method | params | result |
|---|---|---|
| `breakpoint_add` | `addr`\|`symbol`, `enabled`? (def `true`), `label`? | `breakpoint` (str), `addr`, `symbol`?, `symbolDisp`?, `enabled`, `label`? |
| `breakpoint_set_enabled` | `breakpoint` (str), `enabled` | `breakpoint`, `enabled`, `hits` |
| `breakpoint_list` | `cursor`? (str), `limit`? (1–4096) | `breakpoints[]{breakpoint,label?,addr,symbol?,symbolDisp?,enabled,hits}`, `total`, `returned`, `limit`, `truncated`, `cursor`? |
| `breakpoint_clear` | `breakpoint` (str)\|`all` | `removed` |

The five normative behaviours, verbatim from §6's prose block:

1. **Handle, not address.** *"A breakpoint is an opaque handle … a server-assigned string, **never
   reused**, so a stale handle resolves to nothing rather than to someone else's breakpoint. **One address
   may carry several breakpoints**, each its own handle with its own `enabled` and `hits`; a second
   `breakpoint_add` at an address that already has one is **not** a duplicate error and **not** an
   idempotent echo — it is a second breakpoint."* And on firing: *"the machine halts once,
   `emulator/stopped` carries `reason: "breakpoint"` and the additive **`breakpoint`** param naming
   **one** handle — the earliest-added enabled breakpoint at that address — and **every** enabled
   breakpoint at that address increments its `hits`."*
2. **One writer of `enabled`.** *"A disabled breakpoint keeps its handle, its label and its `hits`, and
   does not halt. `hits` counts firings while enabled and is never reset by this surface; a client wanting
   a fresh count clears and re-adds."*
3. **`symbol` resolves at add time**, `-32012` with no symbols loaded, `-32013` when unknown, and *"a
   breakpoint does not move when symbols are reloaded."*
4. **The cap.** *"advertised as `limits.maxBreakpoints` … At the cap `breakpoint_add` MUST fail with
   `-32005` carrying `{"reason":"breakpointCapReached","cap":n,"count":n}` and MUST NOT silently grow past
   the advertised number."* And the deliberate asymmetry: `set_enabled` on an unheld handle *"refuses with
   `-32005 {"reason":"unknownBreakpoint"}` (a client that thinks it is toggling something must learn it is
   toggling nothing)"*; `clear` *"succeeds with `removed: 0`"*. `clear {all:true}` *"removes every
   breakpoint on the server, **including other clients'** — that is the one deliberately shared verb."*
5. **Not subject to the run-control state rule.** *"arming, toggling and clearing mutate an observer, not
   the timeline, and are legal while running."*

Plus §11.21's M2 clarifications: `caveat` is declared **absent** on all four fragments, and `breakpoint`
on `emulator/stopped` is *"REQUIRED on the handle shape: a handle-shape server MUST emit it whenever
`reason` is `breakpoint` and MUST NOT otherwise"*.

Discovery, §11.21 design choice 4: *"A client tells the two apart by the `methods` list in the
`initialize` result: **`emulator/breakpoint_set_enabled` present means the handle shape**, absent means
the address shape. `capabilities.breakpoints` stays a boolean."* We advertise the method and publish the
boolean `true`; the cap rides `limits.maxBreakpoints` because §11.18 forbids widening a boolean a client
already parses into an object.

### `wait_for_break` and its named exemption

The row (§6, as amended by §11.24):

> `emulator/wait_for_break` *(deprecated by `stopped`)* | `timeoutMs`? (≥0, def 30000, ≤300000; refused
> above) | `pc`?, `symbol`?, `symbolDisp`?, `timeoutReached`?, `waitedMs`? *(`running` is the stamp's, see
> `status`; every handler key optional — §11.24)*

The exemption, ruled 2026-08-27 and read at `82eb397` §D12:

> *One named exemption (hub ruling under overnight delegation, 2026-08-27, oracle CR-A review finding):*
> the retained, deprecated `emulator/wait_for_break` keeps its legacy spelling, `timeoutMs` as the bound
> and `timeoutReached` as the gave-up signal (`true` = surrendered; `false` or absent = the break fired),
> because D-07 rules that a retained deprecated method preserves the legacy server's behaviour rather than
> reinventing it, and it dies with the transition window (D6). The exemption is that method's alone: **no
> new op may take it**.

Served exactly as written: `timeoutMs`/`timeoutReached`, no `maxFrames`, no `reached`. `running` is
**not** declared by the handler — it is the envelope stamp's (D-05).

---

## 2. The transport answer for `wait_for_break`

**Question asked of this lane: what happens to a concurrent request while a wait is outstanding?**
**Answer: it is served immediately, on the engine thread, which stays free for the whole wait.**

### Why the obvious implementation is not merely rude here, but self-defeating

`server.rs`'s engine thread is the only owner of `System`, and `engine_loop` serialises: it takes one
`EngineMsg`, dispatches it to completion, and only then looks at the channel again. Its `None` branch —
the one that calls `Engine::free_run_step` — **is what advances a free-running machine**. So a handler
that slept until a break would:

1. stall every other client, *including the one that would call `emulator/pause` to end the wait*; and
2. **guarantee its own timeout**, because the frames that would have carried the machine to the
   breakpoint are exactly the frames the sleeping thread is not running.

Worse in the hosted arrangement: `Host::pump` checks its wall-clock budget *between* commands and *"one
that has started always finishes"*, so a 300-second handler would freeze the player's window for 300
seconds — and that window is the owner's.

### What was built instead

Waiting is a **transport** concern, not a machine concern, so it happens on the thread that is already
allowed to block. `server::wait_for_break_delay` delays the *forward* of the call on the calling
connection's own reader thread, polling the `SharedStamp` the engine already publishes after every
dispatch and every free-run frame. By the time `Engine::wait_for_break` runs, the machine is either
stopped or the deadline has passed; the handler itself neither sleeps nor runs the machine.

Consequences, all of them intended:

* **Another connection is unaffected** — different thread, engine thread free. Two clients can wait at
  once. Asserted by `a_wait_does_not_stall_another_client`, and proven red against a poison that moved
  the sleep onto the engine thread: the concurrent client was then served only after **2.015 s** of a
  2 s wait.
* **The machine keeps running throughout**, which is what lets the awaited breakpoint actually fire.
* A second request *pipelined on the same connection* queues behind the wait, because one connection is
  one reader thread reading NDJSON in order. That is the client's own pipelining choice and is unchanged.
* `Engine::dispatch` remains the only dispatch path. The transport delays a forward; it never answers.

Two details that keep this honest:

* **`timeoutMs` is parsed leniently by the transport and strictly by the engine.** Anything the transport
  does not recognise — missing, wrong type, past the ceiling, or the snake_case `timeout_ms` — yields a
  **zero** delay, so a request that is going to be refused is refused *at once* rather than after a
  five-minute sleep. The engine is the authority on legality; the transport is only the authority on how
  long to sleep.
* **`waitedMs` has exactly one writer** — the transport, which is the only layer that knows. The fragment
  itself says the field measures *"the WAIT, which is a host-side fact, not a machine coordinate"*.
* The wait also watches the server-wide shutdown flag, so it cannot outlive the server it waits on.

Not blocked. No stall was shipped.

---

## 3. The two failures this parcel was told to avoid, and what happened

### "A breakpoint that silently never fires"

The breakpoint sink rides **`free_run_step` bare** — not wrapped in `Observe`. That is the whole point:
free-run is the only path a `resume` → `wait_for_break` loop executes on, and an `Observe` there would
have been a surface that reports success and arms nothing. The `stopAfter` watch beside it *stays*
wrapped, and the asymmetry is principled rather than an oversight: a watch's stop is a **level**
(`matched >= n` is true forever, so honouring it in free-run is a permanent freeze), a breakpoint's is an
**edge** evaluated per step boundary against the current PC, which cannot latch on.

It also rides every bounded advance, through the shared `advance_with`/`attribute` pair. `Engine::advance`
— the `run_frames`/`press` path — was a *separate* Fanout and has been folded onto the shared one, because
a separate Fanout is precisely how an instrument comes to ride four of the five advancing shapes and
silently miss the fifth.

**A real bug was found here by the tests, before it could ship.** See §5.

### The re-trigger hazard (the legacy server's defect 1)

`BusEventSink::on_step_boundary`'s own doc: *"on the stopping iteration `on_step_boundary` is called for an
instruction that does not run, and it is called again for that same PC when the caller resumes."* A sink
that fired on that repeat halts at the same instruction forever. Three `aeon` tools carry a hand-written
`emulator/step`-before-`resume` workaround for exactly this, with the measurement in the comment: *"the
sweep arm ran 24 iterations against ONE frozen tick"*.

`BreakStop` latches the run's starting PC and suppresses a fire there until at least one instruction has
retired — GDB's rule. Asserted on the **emulated clock**, not the hit count, by
`resuming_from_a_breakpoint_address_makes_progress`: a server that never advanced would still report a
rising `hits`, which is how the defect survived as long as it did.

The cost is named rather than hidden: a breakpoint armed at the exact address the machine is *already*
sitting at does not fire on the next resume; it fires the next time execution *arrives* there.

---

## 4. `hits` means "times this breakpoint halted the machine"

The sink observes; the **engine** counts, in `attribute`, once precedence between the run's three possible
stop causes has settled. Counting inside the sink would make `hits` mean "stops that happened to land at
this address", which inflates under exactly the `step`-then-`resume` idiom every consumer uses.

Precedence, when more than one condition ends one run:

1. **the caller's own predicate** (a `run_to` whose target also carries a breakpoint *reached its target*);
2. **a breakpoint** (halts *before* the instruction runs — the more precise of the two remaining);
3. **a `stopAfter` watch** (halts *after* a triggering instruction commits, so it is the later cause).

Every existing stop emitter grew a breakpoint branch ahead of its watch branch, with the same
`deadlineReached: false` and the same caveat discipline `run_to` and `run_to_scanline` already used for
watches.

---

## 5. The bug the tests found

`Engine` carries **two** run flags, deliberately split:

* `free_run` — the *mode*. `emulator/resume` sets it, `pause` clears it, `is_running()` returns it, and
  the server loop reads it to decide whether to advance the machine.
* `running` — *is the machine advancing right now*, free-run **or** inside a bounded run. This is what
  every reply's and event's `running` reports.

The doc comment on the pair says why they are separate: collapsing them *"makes the event stream lie"*.

The breakpoint halt cleared only `running`. The engine loop therefore kept free-running, the machine
re-broke once per frame forever, and every reply told the client it was stopped. Measured through
`Engine::dispatch`: **374,011 hits where the contract says 1**, on a machine a client had just been told
was halted. A halt is the one event that ends *both* conditions, so it now clears both — and it does so
directly rather than through `set_free_run`, which is `pause`'s path and would have emitted
`reason: "pause"` for a stop the client armed and is waiting to be told about by name.

The same confusion sat in `wait_for_break`'s poll, which read `self.running` — false at every dispatch
boundary, so it would have reported a halt on every running machine. It now reads `is_running()`, which is
also exactly the flag the transport polls, so the two halves of the method agree by construction.

**A test gap the poisoning found**, worth recording because it is the failure mode invariant 6 exists for:
`the_list_pages_and_the_cursor_walks_the_whole_set` passed against a server that ignored `limit` entirely
and returned all five rows on page one. It still visited each handle exactly once, which was all the
fixture checked. It now asserts the page ceiling and the page count.

---

## 6. Better-approach pass — where we would serve this better than the contract asks

The standing rule for this lane is that a fragment is the compatibility **floor**, never the design
ceiling. Four findings; **all four are recommendations, none was unilaterally shipped**, and the surface
served is the contract's.

1. **`hits` cannot be read without also being unable to reset it.** §6: *"never reset by this surface; a
   client wanting a fresh count clears and re-adds."* But clearing and re-adding **changes the handle**,
   which every other rule on this surface treats as identity — so the one supported way to re-zero a
   counter is the one operation guaranteed to invalidate every reference a peer holds to that breakpoint.
   On a shared bus that is the 1,691,410-hit incident's shape wearing a different hat. *Recommendation:*
   a `resetHits`? boolean on `breakpoint_set_enabled`, defaulting false. *Cost to a consumer:* zero — an
   additive optional param on a method that already exists, and a server without it refuses the key by
   name under §2.5's closure, so a client learns immediately rather than silently keeping its old count.

2. **`breakpoint_list` cannot be filtered by address.** A client that knows an address and has lost its
   handle must page the whole set and match `addr` itself. Harmless at a cap of 32; the asymmetry with
   `watchpoint_hits`, which *does* take a `watch`? filter, is the tell. *Recommendation:* an `addr`?
   filter param. *Cost:* zero, same reasoning. **Deliberately not** a clear-by-address — that is the exact
   hazard §11.21 abolished, and reading is not deleting.

3. **The re-trigger suppression is behaviour no contract text describes.** It is the difference between a
   surface that works and one three consumers have to route around, and we implemented it — but a second
   conformant server could reasonably not, and then the same client code works against one and freezes
   against the other. *Recommendation:* pin it in §6's prose, in the same place the halt semantics are
   pinned. *Cost:* none to us; it describes what we already do.

4. **`wait_for_break`'s exemption is right, and the transport cost it implies is not written down.**
   D-07 preserves the legacy `timeoutMs`, and on any server whose emulator is single-threaded that is a
   wall-clock bound on a thread that must not block. We solved it below the method; the contract says
   nothing about where the wait may happen, so a naive server can conform to every word of the row and
   still wedge its own bus. *Recommendation:* one sentence in §6 saying a wait must not prevent the server
   answering another client. *Cost:* none — it forbids only the broken implementation.

---

## 7. Live-consumer finding: `timeout_ms` vs `timeoutMs`

Three tools in the sibling `aeon` repo call these methods today and **all three send `timeout_ms`**
(snake_case) where the row says `timeoutMs`:

| tool | calls |
|---|---|
| `aeon/tools/evict_witness.py` | `breakpoint_add`:85, `wait_for_break`:96 (`{"timeout_ms": 60000}`), `breakpoint_clear`:100 |
| `aeon/tools/parallax_hscroll_probe.py` | `breakpoint_add`:584, `wait_for_break`:592 (`timeout_ms=120000`), `breakpoint_clear`:575/594/598/600 |
| `aeon/tools/raster_frame_epoch_probe.py` | `breakpoint_add`:220/221, `wait_for_break`:228 (`{"timeout_ms": 6000}`), `breakpoint_clear`:219/258 |

(`aeon` was read-only to this lane; nothing there was written, committed or branched.)

**We serve the contract spelling and added no alias.** Two spellings for one parameter is how a
vocabulary rots, and §2.5's params closure makes the mismatch a `-32602` **naming the key** before the
handler runs — the loud outcome, and the one §11.24's own migration ruling asks for: *"when two
implementations of one contract disagree about unknown keys, sequence the cutover onto the strict one."*
Pinned as a test (`the_snake_case_spelling_is_refused_rather_than_aliased`) so it stays a property.

**Two further things the lane that owns those tools needs to know:**

* `evict_witness.py:97` reads `r.get("timeout_reached")` — also snake_case. §11.24's D-06 rules camelCase
  the one true spelling. Even once the param is fixed, that line will read `None` from a timed-out wait
  and print no failure, which is the *silent* half of the same defect. The other two tools read the
  envelope's `running`, which we serve correctly, so only `evict_witness` has this second problem.
* The same divergence is already registered in this repo for the owner's **MCP client**, at
  `crates/oracle-aether/tests/mcp_tool_sweep.rs`'s `D33_WIRE_SPELLING` (`wait_for_break.timeout_ms`,
  alongside `audio_spectrum.fft_size`/`max_hz`). So the client and the tools diverge the same way, and
  fixing the tools does not fix the MCP path.

---

## 7b. Three in-tree method sweeps had to be told about the one blocking row

Serving `wait_for_break` turned three existing tests red, and the failure was a **read timeout**, not an
assertion: `handshake::initialize_advertises_a_generated_method_list_that_is_the_dispatch_table`,
`item23_dispatch::every_advertised_method_dispatches_and_every_unadvertised_one_does_not`, and
`methods::every_reply_from_every_method_carries_frame_mclk_and_running`. All three sweep the whole
advertised table calling each name with `{}`. `emulator/resume` is in that table; once it has been called
the machine is free-running, and `wait_for_break` with no params then waits its contractual 30-second
default. The sweeps' own 20-second read timeouts fired.

Fixed at the call site, in one shared helper (`common::sweep_params`), by passing the contract's own
non-blocking spelling `{"timeoutMs": 0}` — §11.24: *"`0` polls once and returns."* Nothing is weakened:
the call still dispatches, still runs the handler, still returns a stamped reply, which is the whole
subject of all three sweeps. What it removes is their hidden dependence on the order of the table.

**Worth naming as a property of the surface rather than a test wart:** `wait_for_break` is now the only
row in the catalog whose *default* behaviour is to block the caller for thirty seconds. That is the
contract's, deliberately (D-07 preserves the legacy default), and it is safe for other clients — the wait
is on the caller's own thread — but any generic "call every method" tooling will need the same
`timeoutMs: 0`. This is the fourth better-approach observation in §6 seen from the consumer's side.

## 8. Stop precision — reported, not keyed

Per instruction, **no `stopPrecision` key was invented**; that question is live in
`docs/2026-08-27-cr-e-stop-precision.md` (merged to `main` at `884a6be` while this lane ran). The finding
this lane can contribute: **our stops are exact.**

`BreakStop` raises its flag from `on_step_boundary`, which the core's own doc pins as *"the machine always
stops at an instruction boundary, never mid-instruction, with `pc` pointing at the instruction that has
not yet executed"*. So the reported `pc` **is** the breakpoint address, and
`a_breakpoint_actually_halts_a_free_running_machine` asserts that exactly (`r["pc"] == HOT_PC`), not
approximately. The legacy defect CR-E documents — *"the reported stop PC occasionally lands a few
instructions before the breakpoint address"*, disclosed through a `note` key — has no analogue here.
Whatever CR-E rules, this server has nothing to disclose on this surface.

---

## 9. Known gap, registered rather than hidden

**Breakpoints do not fire while the hosted player's own free-run loop drives the machine.**

They ride every run *this engine* drives: the standalone server's `free_run_step`, and every bounded
advance (`run_to`, `run_to_scanline`, `run_frames`, `press`, all three `step*`) in **both** arrangements.
They do not ride the player's loop, which reaches the machine through `Engine::run_sinks` — a public
2-tuple threaded through `Host`, `oracle-frontend`'s `bus.rs`, `bus_stub.rs` and `main.rs`.

Wiring it is not a signature change alone: a breakpoint halt there needs the *player* to pause itself and
the engine to emit the `stopped`, neither of which exists, and `Observe`-wrapping it (the shape the watch
uses there) would count hits without halting — worse than not wiring it. Scoped out deliberately, recorded
here and in a doc comment on `Engine::breakpoints`, and the narrow statement is: *a breakpoint armed over
the socket against the windowed player does not fire while the player is free-running; it fires on every
bounded run this bus drives, in both arrangements.*

---

## 10. Gates

| gate | result |
|---|---|
| `cargo fmt --all` | clean (hard commit gate) |
| `cargo clippy --workspace --all-targets` | **0 warnings, 0 errors** |
| `cargo test --workspace --no-fail-fast` | **LEGS 58 · PASSED 1934 · FAILED 0 · IGNORED 6 · exit 0** |
| `git diff main -- crates/oracle-core/tests/` | **empty**. No golden was regenerated; none was touched. |
| `SCHEMATIZED_NOT_ADVERTISED` | forced red by serving five, and edited in the commit that shipped the handlers — the direction the pin's second bullet exists for. All five breakpoint rows plus `wait_for_break` left the set together. |

**The baseline, re-derived rather than trusted**, on a detached checkout of the merge base `a3917fe`:
**LEGS 57 · PASSED 1912 · FAILED 0 · IGNORED 6 · exit 0**. That is exactly the figure the dispatch
carried — the hint was accurate, and is now verified rather than assumed.

The *first* attempt at re-deriving it was **contaminated** and is worth recording: it was launched in this
worktree while edits were in flight, so cargo compiled the half-written tree into the doctest leg. It
reported `LEGS 56 · PASSED 1911 · FAILED 0 · IGNORED 6` beside **`EXIT=101`** — zero reported failures
next to a non-zero exit, which is precisely the shape invariant 5 warns about, and precisely why totals
and exit codes are both reported here rather than a total alone.

**The delta, accounted leg by leg:**

| | baseline | branch | delta |
|---|---|---|---|
| `oracle-aether` lib unittests | 50 | 56 | **+6** — the `breakpoints.rs` module tests |
| `tests/breakpoints.rs` (new leg) | — | 16 | **+16**, and LEGS 57 → 58 |
| every other leg | — | — | unchanged |
| **total passed** | 1912 | 1934 | **+22** = 6 + 16, exactly |

`FAILED` 0 → 0 and `IGNORED` 6 → 6. Three legs (`handshake`, `item23_dispatch`, `methods`) went red
mid-parcel and were fixed at the call site — see §7b; their pass counts are unchanged from baseline.

---

## 11. The 16 fixtures, and the poison each was proven red against

`crates/oracle-aether/tests/breakpoints.rs`, wired into `cargo test --workspace` as the
`oracle-aether` integration leg `breakpoints`. Every line the fixtures receive is schema-validated and
closed against its fragment by `common::schema`, including the `stopped` events.

| fixture | proven red against |
|---|---|
| `one_address_carries_several_breakpoints` | add returns the existing handle at that address (the idempotent echo §11.21 abolished) |
| `a_handle_is_never_reused` | `Breakpoints::clear` rewinds `next_id`; and `clear{all}` returning `removed: 0` |
| `set_enabled_carries_hits_across_the_toggle` | `set_enabled` resets `hits` to 0 |
| `a_toggle_refuses_what_a_clear_forgives` | `set_enabled` forgives an unknown handle (the asymmetry collapsed) |
| `the_advertised_cap_is_the_cap_that_is_enforced` | cap check disabled; and `capabilities.breakpoints: false` with `limits.maxBreakpoints` dropped |
| `clear_all_reaches_another_clients_breakpoints` | `clear{all}` removes nothing |
| `a_breakpoint_actually_halts_a_free_running_machine` | the free-run halt clears no flag (an `Observe` would do this); and `record_halt` bumping only the first breakpoint at the address |
| `a_disabled_breakpoint_does_not_halt` | `breakpoint_add` ignores `enabled` and always arms |
| `resuming_from_a_breakpoint_address_makes_progress` | the resume-PC suppression removed (the legacy defect 1, reintroduced) |
| `a_wait_does_not_stall_another_client` | the wait moved onto the engine thread — concurrent client served only after 2.015 s of a 2 s wait |
| `a_timeout_past_the_ceiling_is_refused_and_refused_at_once` | `timeoutMs` accepted unbounded |
| `a_zero_timeout_polls_once_and_returns` | `timeoutMs: 0` takes the default instead of polling once |
| `the_snake_case_spelling_is_refused_rather_than_aliased` | `timeout_ms` added to the method's params as an alias |
| `the_list_pages_and_the_cursor_walks_the_whole_set` | `breakpoint_list` ignores `limit` — **this one initially stayed green, and the fixture was strengthened rather than the poison weakened** |
| `add_refuses_the_shapes_the_fragment_forbids` | `addr` + `symbol` together silently resolved to one |
| `the_whole_surface_is_legal_while_the_machine_runs` | `breakpoint_add` gated on `require_paused` |

Plus 6 unit tests in `crates/oracle-aether/src/breakpoints.rs` covering the instrument in isolation: the
per-handle state split, the earliest-added-names-the-stop rule, never-reused ids, the resume-PC
suppression, the latch that stops a repeated boundary being double-counted, and the disabled negative
control.
