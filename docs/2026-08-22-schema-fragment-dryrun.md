# Dry run: empyrean's 58-fragment bus schema against the replies this server emits today

**Date:** 2026-08-22 · **Branch:** `schema-dryrun` · **Nothing was vendored.**
`crates/oracle-aether/tests/contract/bus-protocol.schema.json` is untouched by this work and still holds
the 37-fragment revision (`f038672daf6eb2b8…`). Adopting the candidate is the controller's call, not this
document's.

## The inputs, pinned by content and not only by pointer

**A revision is a pointer to the artifact; the pointer keeps moving. The blob id and the content hash name
the bytes actually validated against, which is what every verdict below is a statement about.** Both are
given for both sides.

| artifact | sha256 | git blob | provenance |
|---|---|---|---|
| **candidate** (subject) | `82dde99ef8c62d41bbe1d9808e783b452330c1d743989203cd24325e496b1b12` | `bb252a4d1381e1cd9f20f93d6a5f2a160f9796dc` | empyrean `contract/schema/bus-protocol.schema.json` at `ceef822` (merge `fe5a238`, fragments authored `8d89098`) |
| **vendored** (baseline) | `f038672daf6eb2b844abce7e7d0196c9aff3354ca230afe09e3abb2d7a745516` | `251fef0a8c4323edc7696fe2fd78e5865cd96267` | this repo, `crates/oracle-aether/tests/contract/bus-protocol.schema.json` |
| **this run's oracle revision** | — | — | `3ca8521df7a8787d86044bbb42e17d02004eaf72`, branch `schema-dryrun` |

**The candidate commits are PUSHED and reachable.** Verified firsthand at the end of the run:
`git merge-base --is-ancestor fe5a238 origin/main` → yes; same for `ceef822`. The brief that opened this
task described them as unpushed; they were published while it was running, and that caveat is withdrawn.

**The pointer moved twice during this task, and the blob did not — which is the reason both columns
exist.** empyrean's `origin/main` was `20a8e81` when the task was written, `02cac0bd…` when the controller
verified reachability, and `baf15c28540866dcc41c51e4d4f065c63a06dbaf` when this document was finalised. At
every one of those tips, `origin/main:contract/schema/bus-protocol.schema.json` resolves to blob
`bb252a4d…` — byte-identical to the copy measured here. A later reader re-resolving the *branch* will get
whatever is current; re-resolving the *blob* gets what was measured.

**The scratch copy was re-hashed before any of this was written down**, because it was taken from a
working-tree path before the push, and that check is the only thing making it the same object as the
published artifact. It matched: `82dde99e…` / `bb252a4d…`, derived independently in this worktree.

## Executive answer

> *Which of the ~21 newly-covered methods would the new fragments accept or refuse when validated against
> the replies our server actually emits today?*

**Neither — for all 21.** This server emits no reply for any of them. Every one answers `-32601 no such
method`. The 21 newly-covered methods are, exactly and without exception, methods `oracle-next` does not
advertise and does not serve.

That is the whole of the direct answer, and it is a **class (c)** result 21 times over. It is reported as
a failure, not as a clean sheet, and the scaffolding is built so it cannot be read any other way.

The candidate is also, on the surface this server *does* serve, **purely additive**: all 37 pre-existing
fragments, plus `$defs`, `anyMessage`, `events` and `handshake`, are structurally identical between the
two revisions (compared by parsed-JSON equality, not by diff hunks). No fragment lost coverage.

## Baseline, measured first

`AETHER_CONTRACT_SCHEMA=<f038672 copy> cargo test -p oracle-aether`
— wall clock `13:48:24 → 13:48:55` (uptime 4 days 14:12, load 8.71).

```
LEGS=23 PASSED=294 FAILED=0 IGNORED=0
```

Green, with the freshness check pinned against a copy of the `f038672…` bytes rather than the sibling
checkout's working tree (which already holds the candidate). Nothing skipped: the `vendor`
symlink was created at the worktree root and verified to carry all 17 `TestRoms` entries before any
cargo ran, because a missing symlink makes conformance rows skip silently and a silent skip read as a
pass would have poisoned every number in this document.

## Dry-run aggregate

The candidate was copied over the vendored path (the harness compiles its schema in via `include_str!`,
so a file swap is the only faithful instrument), the suite was run, and the file was restored and its
sha re-verified as `f038672…`.

`cargo test -p oracle-aether --no-fail-fast` — wall clock `13:49:53 → 13:50:17` (uptime 4 days 14:13,
load 17.86).

```
LEGS=23 PASSED=291 FAILED=3 IGNORED=0
```

`--no-fail-fast` is load-bearing: the first run without it reported `LEGS=8 PASSED=110 FAILED=1`, because
cargo stops launching test binaries after one target fails. That truncated aggregate is not a result and
is recorded here only so nobody re-derives it and thinks the suite shrank.

**Same 23 legs, same 294 tests, three moved from pass to fail.** All three are named below. **None of
them is a reply being refused.**

## Proof the runs happened, separate from what they returned

Corollary to bar 4: **on a byte-neutral parcel a matching hash cannot witness that the run executed.** It
bites here harder than usual, because "nothing moved" is this dry run's *expected* result — so an
unchanged vendored sha is equally consistent with a clean run and with a run that never validated against
what I think it did. Three separate witnesses, none of which is a hash.

### W1 — each leg emitted a report only a binary compiled against *that* schema could produce

The harness compiles its schema in via `include_str!`, so the bytes in
`crates/oracle-aether/tests/contract/bus-protocol.schema.json` **at compile time** are what every reply is
validated against. The coverage test prints a report derived from those compiled-in bytes, and the two
legs print different reports:

Baseline leg (`sha256 f038672…` in place), verbatim:

```
advertised methods: 37   result schema present: 37   absent: 0
  schematized but not advertised (0):
```

Dry-run leg (`sha256 82dde99…` in place), verbatim:

```
advertised methods: 37   result schema present: 37   absent: 0
  schematized but not advertised (21): emulator/audio_spectrum, … emulator/z80_write
```

`(0)` versus `(21)` is a fact about the compiled-in document. It cannot be produced by a stale binary, a
skipped leg, or a path that silently resolved to no fragments — and it distinguishes the two legs from
each other, which a hash of an unchanged file cannot.

### W2 — the fragment count is derived by parse and was *wrong* on the candidate

`params_closure::every_params_object_in_the_vendored_schema_is_closed` re-derives the fragment count by
walking the compiled-in document and comparing it to the document's own self-description. On the dry-run
leg it reported `left: 37, right: 58`. A run that had not actually loaded the 58-fragment document could
not have counted 58. The failure is reported as finding (b) below; it doubles as proof of execution.

### W3 — the harness was pointed at a knowingly-wrong fragment and went red, naming the mismatch

The concern the two witnesses above do not answer: *can this harness be green while validating nothing?*
Answered by making it fail on purpose. `emulator/status`'s result fragment was given one extra `required`
key in a scratch copy of the **baseline** schema, that copy was installed, and one leg was run:

```
contract schema violation on the wire (1 failure(s)); the schema is normative for wire shapes
  answering method: emulator/status
  line: {"id":3,"jsonrpc":"2.0","result":{"droppedEvents":0,"frame":0,"frameToken":0,"mclk":0,
         "pc":"0x00000200","romBytes":768,"romLoading":false,"romPath":null,"running":false,
         "sp":"0x00FFFFFE","sr":"0x2700","symbolCount":0,"symbolsPath":null}}
  methods.emulator/status.result: $: "aKeyNoServerEmits" is a required property
```

```
test result: FAILED. 19 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out
```

The mismatch is named exactly, on a **real live reply** rather than a fixture, and one planted key turned
6 tests red across a single leg. So the green baseline of 294 is not a validator accepting everything: a
fragment demanding a key this server does not emit is caught, loudly, wherever the reply is produced. The
corrupted copy was then discarded and the vendored file restored to `f038672…` / blob `251fef0a…`.

The same question is answered for the dry-run *scaffolding* separately, and also by deliberate red — see
"The scaffolding" below, where `probe` was neutered to return `Pass` unconditionally and its control
failed.

## Per-method verdict table

### A. Covered before, and still covered (37) — all MEASURED, all PASS

Every method this server advertises. Their fragments are byte-for-byte the same in both revisions, and the
suite exercised them through `Client::recv`, the single funnel where every server→client line is validated
against `methods.<name>.result` closed with `unevaluatedProperties: false` (§8 item 20). With the candidate
vendored in, **not one `assert_incoming` violation fired** across all 23 legs.

`emulator/checkpoint`, `checkpoint_drop`, `checkpoint_list`, `get_profiler`, `get_profiler_frames`,
`hold`, `load_symbols`, `lookup_symbol`, `memory_hash`, `pause`, `pixel_attribution`, `play_input`,
`press`, `read`, `read_cram`, `read_memory`, `read_vram`, `registers`, `release_all`, `reload_rom`,
`reset`, `restore`, `resume`, `run_frames`, `run_to`, `scanlines`, `screenshot`, `set_profiler`,
`sprites`, `state_hash`, `status`, `watchpoint_add`, `watchpoint_clear`, `watchpoint_hits`,
`watchpoint_list`, `write_cram`, `write_memory` — **37/37 PASS.**

### B. Newly covered (21) — all UNMEASURED, class (c)

**Every one of the 21 fragments compiled cleanly** (`FRAGMENT-BROKEN: 0`), so nothing here is blocked on a
malformed schema. They are blocked on there being no reply in existence to judge.

| method | advertised here | governing capability flag | verdict | server's own words |
|---|---|---|---|---|
| `emulator/breakpoint_add` | no | `"breakpoints": false` | **UNMEASURED** | `-32601 no such method: emulator/breakpoint_add` |
| `emulator/breakpoint_clear` | no | `"breakpoints": false` | **UNMEASURED** | `-32601 no such method: emulator/breakpoint_clear` |
| `emulator/breakpoint_list` | no | `"breakpoints": false` | **UNMEASURED** | `-32601 no such method: emulator/breakpoint_list` |
| `emulator/z80_read` | no | `"z80": false` | **UNMEASURED** | `-32601 no such method: emulator/z80_read` |
| `emulator/z80_write` | no | `"z80": false` | **UNMEASURED** | `-32601 no such method: emulator/z80_write` |
| `emulator/vgm_start` | no | `"vgm": false` | **UNMEASURED** | `-32601 no such method: emulator/vgm_start` |
| `emulator/vgm_status` | no | `"vgm": false` | **UNMEASURED** | `-32601 no such method: emulator/vgm_status` |
| `emulator/vgm_stop` | no | `"vgm": false` | **UNMEASURED** | `-32601 no such method: emulator/vgm_stop` |
| `emulator/step` | no | *none — plain absence* | **UNMEASURED** | `-32601 no such method: emulator/step` |
| `emulator/step_over` | no | *none — plain absence* | **UNMEASURED** | `-32601 no such method: emulator/step_over` |
| `emulator/step_out` | no | *none — plain absence* | **UNMEASURED** | `-32601 no such method: emulator/step_out` |
| `emulator/run_to_scanline` | no | *none — plain absence* | **UNMEASURED** | `-32601 no such method: emulator/run_to_scanline` |
| `emulator/wait_for_break` | no | *none — plain absence* | **UNMEASURED** | `-32601 no such method: emulator/wait_for_break` |
| `emulator/write_vram` | no | *none — plain absence* | **UNMEASURED** | `-32601 no such method: emulator/write_vram` |
| `emulator/ping` | no | *none — plain absence* | **UNMEASURED** | `-32601 no such method: emulator/ping` |
| `emulator/log_clear` | no | *none — plain absence* | **UNMEASURED** | `-32601 no such method: emulator/log_clear` |
| `emulator/audio_spectrum` | no | *none — plain absence* | **UNMEASURED** | `-32601 no such method: emulator/audio_spectrum` |
| `emulator/set_layer_enabled` | no | *none — plain absence* | **UNMEASURED** | `-32601 no such method: emulator/set_layer_enabled` |
| `emulator/get_layer_states` | no | *none — plain absence* | **UNMEASURED** | `-32601 no such method: emulator/get_layer_states` |
| `emulator/set_channel_enabled` | no | *none — plain absence* | **UNMEASURED** | `-32601 no such method: emulator/set_channel_enabled` |
| `emulator/get_channel_states` | no | *none — plain absence* | **UNMEASURED** | `-32601 no such method: emulator/get_channel_states` |

The capability column is the *contract-level* reason, quoted from this server's own `initialize` reply
(`crates/oracle-aether/src/engine.rs`, the `capabilities` object). Eight of the 21 are governed by a flag
this server already publishes as `false`, so a conformant client is told in advance not to call them. The
other thirteen have no flag: they are simply not implemented, and a client learns that only from the
`methods` array `initialize` returns.

### B.1 — Reachability, enumerated per route rather than argued

Protocol bar 13: *"this can never be live when that fires" is a claim about EVERY caller.* "The harness
could not reach it" is exactly such a claim, so it is discharged by enumerating every route that could
produce a reply in this process, not by describing the one the dry run happened to take.

The routes were found by grepping for every call site of `Engine::dispatch`, every use of `METHODS`, every
`fn` matching the 21 names, and every `cfg(feature)` and `[features]` declaration in `oracle-aether`.

| # | route to a reply | who takes it | blocker, specifically |
|---|---|---|---|
| 1 | socket server → `Engine::dispatch` | `crates/oracle-aether/src/server.rs:456` — what `Client::call` drives, i.e. the path the dry run used | `METHODS.iter().find(\|m\| m.name == method)` returns `None`; refused by name **before** any handler exists to run |
| 2 | in-process host → `Engine::dispatch` | `crates/oracle-aether/src/host.rs:409` (`EngineMsg::Call`) | identical — same function, same `const` table |
| 3 | frontend/player → `Engine::dispatch` | `crates/oracle-frontend/src/pick.rs:539`, `src/bus.rs:362/376/392` | identical. The frontend **calls** dispatch by method name; it registers no methods of its own |
| 4 | session pre-dispatch (bypasses `METHODS`) | `crates/oracle-aether/src/session.rs:54`, `:79` | reaches exactly two names, `initialize` and `initialized`; every other method falls to `_ => Ok(Action::Dispatch)`, i.e. back to route 1. None of the 21 can be intercepted here |
| 5 | direct handler call, bypassing the bus entirely (what a test *could* do) | any test with an `Engine` | **no handler exists to call.** `grep` for `fn step`, `fn step_over`, `fn step_out`, `fn ping`, `fn z80_read`, `fn z80_write`, `fn write_vram`, `fn vgm_start`, `fn vgm_stop`, `fn vgm_status`, `fn breakpoint_*`, `fn audio_spectrum`, `fn log_clear`, `fn wait_for_break`, `fn run_to_scanline`, `fn set_layer_enabled`, `fn get_layer_states`, `fn set_channel_enabled`, `fn get_channel_states` over `crates/oracle-aether/src/` returns **zero hits** |
| 6 | a build configuration that adds methods | cargo | `crates/oracle-aether/Cargo.toml` has **no `[features]` section at all**, and `grep 'cfg(feature'` over `crates/oracle-aether/src/` returns nothing. There is no build in which `METHODS` differs |
| 7 | a runtime toggle that adds methods | `EngineConfig` | `METHODS` is a `pub const` — a compile-time literal array with no conditional entries. `capabilities.z80/vgm/breakpoints` are *declarations of absence*, not switches; nothing reads them to enable a row |

`engine.rs:981` states the same thing in the source's own words: *"Look a method up in `METHODS`. This is
the **only** dispatch path."* The enumeration above is the check on that claim rather than a restatement
of it — routes 4, 5, 6 and 7 are precisely the ones that would falsify it, and each was searched for and
came back empty.

**What would have to become true** for any of the 21 to be measurable here: a `MethodSpec` row in
`METHODS` with a handler behind it — i.e. implementing the method. Not a loaded ROM (the dry run ran on a
booted test ROM and got `-32601` regardless), not a live VDP or Z80 state, not a capability negotiation,
not an MCP session. The refusal happens on a name lookup before any machine state is consulted.

**One honest residual.** This enumeration is scoped to *this process*. A server that does serve these
methods exists — `oracle-old` — and validating the 21 fragments against **its** replies is a real and
un-taken measurement. It is a different repository and a different exercise, and it is named here so that
"unreachable" is not read as "unmeasurable anywhere".

### C. Uncovered by both (8 §6 rows)

`z80_registers`, `read_vdp_registers`, `read_vsram`, `object_slot`, `object_list`, `player_state`,
`call_stack`, `log_tail` — the candidate's own `description` names these eight and states the reason
(each states its result too loosely to transcribe without inventing). Out of scope here: no fragment, so
nothing to accept or refuse. This server does not serve them either.

## The three failure classes, kept apart

### (a) Our server is wrong — **0**

No reply this server emits is refused by any fragment in the candidate. This is **measured, not inferred**:
the candidate was actually vendored in and the whole 23-leg suite run against it, so every reply produced
by 294 tests passed through the closed per-method validator. Zero `contract schema violation on the wire`
panics.

### (b) The fragment is wrong — **1**

**The candidate's own `description` contradicts the document it describes.**

Ours, quoted from `crates/oracle-aether/tests/params_closure.rs:144`:

```
assertion `left == right` failed: the description claims 37 fragments and the document holds 58
 — a count is parsed or it is wrong (§11.17 clause 7)
  left: 37
 right: 58
```

Theirs, quoted from the candidate's `description`, leading sentence:

> "…the symbol primitive, and **37 of §6's ~60 catalogued methods are schematized here**…"

and, 1,200 characters later in the same string:

> "…recounted again 2026-08-22 by the §9 mechanical-completion pass, which **took it to 58 of §6's 66** by
> adding 21 fragments…"

The document holds 58. The leading claim says 37. Both spellings match the shape `N of §6's …` that
`§11.17 clause 7` requires be **parsed rather than trusted**, and our derivation lands on the first — which
is also the sentence a human reads first. The parenthetical correction does not repair the headline; it
sits inside it.

**Honest qualification:** a parser taking the *last* such triple would read 58 and go green, so part of
this is a parse-position choice on our side. It is still classed (b) because the artifact contains a
literally false statement about itself either way, and because empyrean's own gate
(`contract/schema/tests/validate_contract_schema.py`, G5) derives its count by diffing §6 against the
fragments and **never inspects the `description` at all** — so nothing upstream is watching this string.
Cheapest fix: amend the leading sentence to 58 of §6's 66.

### (c) The harness cannot reach the method — **21**

All of section B. The reason is uniform and structural: `oracle-next` advertises 37 methods; the candidate
schematizes 58 §6 rows; the 21 in the gap are contract catalog entries this server has never implemented.
There is no ROM state, no timing window and no capability flag that would make them reachable — the
dispatch table has no entry, so `Engine::dispatch` refuses by name before any handler exists to run.

**These 21 are not evidence that the fragments are good.** Nothing this server does was ever compared
against them. If any of the 21 fragments would refuse a conformant server, this exercise could not have
found out, and the scaffolding is written to fail rather than let that read as green.

## Two further findings that are not reply-shape classes

Both are real, both were surfaced by the candidate, and neither fits (a)/(b)/(c) — so they are reported
separately rather than folded into a bucket where they would be miscounted.

### F1 — three legacy-MCP param names the new fragments do not declare (client-side collision)

`tests/mcp_tool_sweep.rs` sweeps the legacy MCP client's tool table against the fragments. Its numbers
move a long way, mostly for the better:

| | tools parsed | with a fragment | without | properties no fragment declares |
|---|---|---|---|---|
| baseline (37) | 63 | 37 | 26 | **0** |
| candidate (58) | 63 | **57** | 6 | **3** |

The three:

```
properties the fragment does NOT declare:
  ["audio_spectrum.fft_size", "audio_spectrum.max_hz", "wait_for_break.timeout_ms"]
```

Both sides, quoted. `oracle-old/linux-port/mcp/oracle_mcp.py`:

```python
# line 202
{"timeout_ms": {"type": "integer", "description": "Milliseconds to wait (default 30000)"}},
# lines 590-591
"fft_size": {"type": "integer", "description": "Power of two 256..32768 (default 4096). ..."},
"max_hz":   {"type": "integer", "description": "Optional: only return bins up to this frequency."},
```

Candidate fragments:

```
emulator/audio_spectrum  params.properties = ["fftSize", "maxHz", "source"]
emulator/wait_for_break  params.properties = ["timeoutMs"]
```

**The fragments are right and the client is wrong.** `protocol.md` §6 spells both rows camelCase —
line 1388 `fftSize`? (256–32768, def 4096, →pow2), `maxHz`?; line 857 `timeoutMs`? — so the fragments were
transcribed faithfully. `call_tool` passes its arguments through verbatim (`args = dict(arguments or {})`,
`oracle_mcp.py:1014`; no rename anywhere in the path), so the MCP client puts `fft_size` on the wire. Since
§2.5 closed request params, a server enforcing these fragments answers `-32602` and the call fails at
runtime on the user's machine.

This is **not** a defect the candidate introduces — the collision already existed; the fragment is the
first artifact able to see it. It is a live client bug against the contract, and it is only latent here
because this server does not serve either method. Worth relaying to whoever owns `oracle_mcp.py`.

### F2 — 21 fragments for methods this server does not advertise (a decision owed at vendor time)

```
thread 'the_schema_covers_every_method_we_advertise_and_the_uncovered_list_is_pinned_empty' panicked
at crates/oracle-aether/tests/schema_conformance.rs:350:5:
the schema has fragments for methods this server does not advertise: [... all 21 ...].
Not a conformance failure — but advertising a method IS shipping it, so either serve them in this
cycle or record the deferral deliberately by relaxing this assertion with the reason.
```

The assertion is doing exactly what it was written for, and its own comment predicted this case: fragments
landing ahead of handlers is the order §8 item 20 wants, and the rule is only that it must be *a decision
taken in the re-vendor commit* rather than something noticed later. **Whoever vendors this schema owes
that decision** — serve the 21, or relax the assertion with the reason written down. It is not a bug in
the candidate.

Coverage report at that point, verbatim:

```
advertised methods: 37   result schema present: 37   absent: 0
  UNCOVERED (0):
  schematized but not advertised (21): emulator/audio_spectrum, … emulator/z80_write
  events with a params schema (3): emulator/resumed, emulator/romReloaded, emulator/stopped
  KNOWN CONTRACT DIVERGENCES (0)
```

## Coverage and honesty accounting

Of the candidate's **58** fragments:

* **37 exercised against live replies, all PASS.** These are the methods this server advertises; the
  suite drove them through the real harness with the candidate vendored in.
* **21 NOT exercised.** Named in full in section B, with the blocker enumerated per route in B.1 rather
  than asserted. Reason, identical for all 21 and quoted from the server: `-32601 no such method`. Not
  sampled, not partially checked, not assumed fine — **unmeasured.**
* **0 fragments failed to compile.**
* **0 fragments lost coverage** relative to the vendored revision.

So the exercised fraction is **37/58 = 64%**, and the unreached 36% is exactly the newly-added surface
the question was about. That is the honest shape of this result: the dry run confirms the candidate is
safe for everything we serve, and says **nothing at all** about the 21 rows it was primarily added for.

**What would make the 21 measurable:** implementing them here — a `MethodSpec` row with a handler behind
it. B.1 enumerates the seven routes that could otherwise have produced a reply and names the blocker on
each; none is a ROM, a machine state, a feature flag or a negotiation. The one measurement that remains
genuinely available and un-taken is against a server that *does* serve them (`oracle-old`), which is a
different repository and a different exercise.

## The scaffolding

`crates/oracle-aether/tests/schema_dryrun.rs`, committed separately and marked dry-run in its own module
doc. It is **not in the default suite**: every test is `#[ignore]`d.

Named runner:

```
AETHER_DRYRUN_SCHEMA=/path/to/candidate.json \
  cargo test -p oracle-aether --test schema_dryrun -- --ignored --nocapture
```

Properties it was built to have, and how each was proven:

* **Derived, not transcribed.** The newly-covered set is computed as *candidate fragments − vendored
  fragments*, by parse. Nothing in the file lists the 21 by name; the number `21` in this document came
  out of the run.
* **No default candidate.** `AETHER_DRYRUN_SCHEMA` is mandatory. A fallback to the vendored copy would
  validate it against itself, report a clean sweep and mean nothing.
* **Loud on unmeasurable.** `dry_run` **fails** while anything is UNMEASURED, printing the whole table and
  then a banner naming every unreached method with the server's own refusal text. There is deliberately no
  configuration that turns 21 unmeasured rows into a green result.
* **Anti-vacuity, two layers, both proven red.**
  * *Reachability control.* `POSITIVE_CONTROLS` (`status`, `registers`, `read_cram`) must come back
    MEASURED, or the UNMEASURED rows describe the harness rather than the server. **This fired for real
    on the first run** — `read_cram` was probed with `{"index":0,"count":1}` and the server answered
    `-32602 emulator/read_cram does not accept 'count', 'index'; accepted params: line`, so the control
    stopped the run instead of letting a mis-parameterised row sit in the table looking like evidence.
    Fixed to `{"line":0}`; 3/3 MEASURED/PASS since.
  * *Validation control.* `the_probe_rejects_a_reply_the_candidate_forbids` plants two defects in the
    candidate **in memory only** — a `required` key no server emits, and the removal of a `properties`
    entry (`pc`) the server does emit, to exercise item 20's `unevaluatedProperties` — and requires the
    probe to reject the same live reply and name the planted key. **Proven red on purpose:** `probe` was
    temporarily edited to return `Verdict::Pass` unconditionally and the control failed with
    `THE PROBE ACCEPTED A REPLY THE FRAGMENT FORBIDS. Every MEASURED/PASS in the table is worthless if
    this can happen: MEASURED/PASS`. The edit was reverted.
* **Reads a path, never the vendored copy**, so running it cannot change what the real suite validates
  against.

## Currency

Nothing under `crates/oracle-core/tests/` moved. No golden was regenerated. The vendored schema was
restored bit-for-bit after the swap and its sha re-verified (`f038672daf6eb2b8…`).

## Recommendation

1. **The candidate is safe to vendor as far as this server can tell** — it refuses nothing we emit, and
   it costs nothing on the 37-method surface.
2. **Vendoring it owes two deliberate acts**, neither of which is optional and neither of which this
   dry run may take on the controller's behalf:
   * a decision on F2 — serve the 21 in that cycle, or relax `schema_only.is_empty()` with the reason;
   * either a contract amendment fixing the `description`'s leading count (F/(b)), or a note that our
     parse-position is the thing to change.
3. **Report F/(b) and F1 back to empyrean.** The count is a one-line amendment. The three MCP param
   names are a client fix that the fragments have just made findable, and that will break real calls the
   moment either method is served by a §2.5-closing server.
4. **Do not read this document as a review of the 21 fragments.** It is a review of the 37 it did not
   change. The 21 remain unexamined by any instrument in this repo.
