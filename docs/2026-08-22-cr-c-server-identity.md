# CR-C — server identity: which implementation answered, and which build of it

**Raised by:** the oracle lane (the ground-up Rust core + Aether server, `oracle/`).
**Against:** `empyrean` `contract/protocol.md` §2.1 (*the handshake*), D4, §8 (*conformance checklist*), and
`contract/schema/bus-protocol.schema.json` fragment `handshake.initialize`, read at `origin/main`
`cc88d38` (blobs `1e832b1` and `9d8cc3c`).
**Closes:** no numbered audit defect. Raised from a live incident: a consumer could not determine which of
the two servers answered it, nor which build of that server, and the check that finally settled it was an
accident of a launch argument.
**Adjacent:** audit **D-01**, whose reading (c) — *"a server-chosen build number"* on `emulator/ping` —
is the same want with no home; §5 makes a home for it here instead.
**Date:** 2026-08-22.

---

## 0. How to read this document

This CR proposes changes to a **contract**, not to a server. It is written to be adjudicated by a reader
with no prior exposure to this repo, so every claim below is either quoted from a cited source at a named
revision or marked as a judgement.

§1 is the summary. §2 is the evidence base — what was read, at what revision, what was *not* checked, and
the six discriminators that exist today with the reason each is scheduled for deletion. §3 states the
three **properties** this CR asks the contract to adopt; they are the substance, and §4–§8 are mechanisms
serving them. §9 gives the exact textual deltas. §10 states what this CR does not bind. §11 names where
this CR is weakest. §12 separates the questions handed over undecided from what this CR considers settled,
so an adjudicator can object to the settling. §13 is provenance.

**Two implementers, and this is a CR where that matters.** The Aether bus has two servers: the Rust
`oracle-aether` in this repo, and the legacy C++ `ControlSocket.cpp` in `oracle-old/`. Every behavioural
statement below is attributed to one of them by name, at a named revision. Where they disagree, the
disagreement is reported rather than resolved by preferring the lane that wrote this document. Nothing
here asks the legacy implementer to do work it has not agreed to; §10 states separately what each
implementation would owe.

**A note on cost and on runtime.** No emulator was driven, no `cargo` command was run, and no
`mcp__oracle__*` tool was touched. Nothing here is a runtime observation. Items wanting runtime
confirmation are tagged **⟨RUNTIME⟩** and collected in §11.6.

---

## 1. Summary

A client connects to the Aether socket, completes `initialize`, and gets back — among other things — a
`serverName` and a `serverVersion`. Neither answers the two questions the client actually has:

1. **Which implementation am I talking to?**
2. **Which build of it?**

Both questions are live, not theoretical. Both servers resolve the **same socket chain** (§2.3), so which
one answers is decided by **whoever launched first**, with no config change on either side and no signal
to the client (§2.3, verified from the consumer's own source). And an installed binary can serve a
strictly smaller surface than the source tree it was built from: the release binary on this machine
carries **37** of the **41** method names present in source at `082e6ce`, so it cannot advertise more
than 37. The four missing names are enumerated in §2.6, and both builds would report identical values
in both existing identity fields.

The contract's existing identity fields cannot be repaired into an answer:

| Field | Rust server (`082e6ce`) | Legacy C++ (`d629771`) | Why it fails |
|---|---|---|---|
| `serverName` | `"oracle-next"`, an `EngineConfig` field (`engine.rs:150`) with a hardcoded default (`:190`) shipped at `:1344` | `"oracle"`, a file-scope `static const char*` (`ControlSocket.cpp:2693`) | **Config-overridable on one side.** Discriminates perfectly today only because nobody has set it — and the rename that inverts it is already in progress (§3.2). |
| `serverVersion` | `env!("CARGO_PKG_VERSION")` as an `EngineConfig` default (`:191`), and `crates/oracle-aether/Cargo.toml:3` pins `version = "0.0.0"` | `"2.1-linux"`, a `static const char*` (`:2694`) | **Carries no information on either side.** Ours is the constant string `0.0.0` for every commit ever made; theirs has not moved since the port. Not REQUIRED by the schema, either. |

This CR proposes:

| # | Change | Property served |
|---|---|---|
| **C1** | A new REQUIRED top-level `implementation` string in the `initialize` result, drawn from a registry in §2.1, immutable at run time | P1, P2 |
| **C2** | A new REQUIRED top-level `serverBuild` object — `{id, source, dirty?}` — fixed at build time, never read at start-up | P1, P3 |
| **C3** | `serverVersion` becomes REQUIRED and is **defused**: a human-facing label clients MUST NOT branch on (D-01's own remedy applied here) | P1 |
| **C4** | §8 gains item **23**: a name in `methods` MUST dispatch. A `-32601` for an advertised name is a server defect, not a discovery mechanism | the consumer pre-check |
| **C5** | `methodCount` is **refused** as a wire field, and made unnecessary by C2 — with one stated condition that would reverse this | — |

**C4 is a clarification, not an invention.** D4 already says the response lists *"the **exact set of
supported** methods"* (`protocol.md:73-74`). What is missing is that this is nowhere on §8's checklist and
no consequence is stated for violating it. Both implementations already satisfy it structurally (§2.4), so
C4 costs neither server anything today and converts a consumer's expensive discovery route into a defect
detector.

**What this costs consumers.** One consumer exists and it is `aurora`, read at `638df0a`. C1–C3 are purely
additive: nothing aurora reads today changes meaning. C4 costs it nothing and lets it demote a code path
it already carries. C5 declines to add a field aurora **does not receive from the server anyway** — it
computes `methodCount` itself as `methods.length` (`client.ts:149`), which §8 shows is the right answer
and which C2 makes non-load-bearing.

---

## 2. Evidence base

### 2.1 Sources read, and at what revision

| Source | Read as | Revision |
|---|---|---|
| `empyrean/contract/protocol.md` | committed blob, `git -C ../empyrean show origin/main:contract/protocol.md` | `origin/main` `cc88d38`, blob `1e832b1` |
| `empyrean/docs/2026-08-22-protocol-schema-audit.md` | committed blob, same route | `origin/main` `cc88d38` |
| `empyrean/CLAUDE.md` | committed blob, same route | `origin/main` `cc88d38` |
| `oracle/crates/oracle-aether/tests/contract/bus-protocol.schema.json` | vendored copy, parsed with `json.load` | worktree at `082e6ce`, blob `9d8cc3c` |
| `oracle/crates/oracle-aether/src/{engine,main,server,host}.rs` | worktree files | `082e6ce` |
| `oracle/crates/oracle-aether/Cargo.toml` | worktree file | `082e6ce` |
| `oracle-old/linux-port/gui/ControlSocket.cpp` | committed blob, `git -C ../oracle-old show d629771:…` | `oracle-old` `d629771` |
| `aurora/src/main/aether/{client,unserved}.ts` | committed blobs, `git -C ../aurora show 638df0a:…` | `aurora` `638df0a` |
| `oracle/target/release/oracle-aether` | binary, `strings -a -n 4` | built 2026-08-21 22:11:04 −0400 |

Four provenance facts, stated rather than assumed:

- **The vendored schema in this repo is byte-identical to upstream's.** `git rev-parse
  HEAD:crates/oracle-aether/tests/contract/bus-protocol.schema.json` at `082e6ce` and `git -C ../empyrean
  rev-parse origin/main:contract/schema/bus-protocol.schema.json` both give blob
  `9d8cc3c36cf2d77fdb9a4aed124f31c95f2de028`. Fragment text quoted below is the upstream artifact.
- **CR-A and CR-B read `protocol.md` at `f172e61`; this CR reads it at `origin/main` `cc88d38`.**
  `git log f172e61..origin/main -- contract/protocol.md contract/schema/bus-protocol.schema.json` returns
  **no commits**, and `git rev-parse` gives blob `1e832b1` for `contract/protocol.md` at both revisions.
  All three CRs are reading identical contract text.
- **The schema holds 58 method fragments, not 59.** `d['methods']` has 59 keys; exactly one of them is
  `$comment`. Both figures were derived by parsing, and the 59 is the miscount a naive key count
  produces. The 58 are enumerated in §2.6's difference set together with the 41 our server serves.
- **`../empyrean` and `../oracle-old` and `../aurora` are peers' live working trees.** Every file from
  them was read through `git show` at a named revision, never through the path. Nothing below quotes an
  uncommitted mid-edit file.

### 2.2 What was not checked, and is therefore taken as given

- **No `cargo` command was run** — another lane holds that resource in this repo. The 41-method figure is
  a source count (`grep -c 'name: "emulator/' crates/oracle-aether/src/engine.rs` → 41), not a served
  count observed on a wire. **⟨RUNTIME⟩**
- **No emulator was driven and no MCP tool was called.** Every behavioural claim is a claim about source
  text or about a binary's string table.
- **The 37-method release-binary figure is derived from `strings`, not from a handshake.** The method is
  stated in §2.6 and its one weakness is named there. **⟨RUNTIME⟩**
- **The legacy C++ server was read, not run.** Statements about what it puts on the wire are read off
  `ControlSocket.cpp` at `d629771`; a running instance may be an older or newer build, which is the very
  hazard this CR exists to make visible and which I therefore cannot exclude for my own evidence.
- **`aurora` was read at `638df0a` on branch `master`; it has no `origin/main`.** That is its default
  branch, not a stale pointer.

### 2.3 Which server answers is decided by launch order — verified from both sides

The consumer states it in its own source, unprompted:

> The suite is cutting over from the legacy C++ Aether server to the Rust core, and both resolve the
> **SAME socket chain** (`$ORACLE_SOCKET` -> `$EXODUS_SOCKET` -> `$XDG_RUNTIME_DIR/oracle.sock` ->
> `/tmp/oracle.sock`, see `socket-path.ts`). Aurora can therefore change which implementation it is
> talking to **with nothing in this codebase changing**, and the new server serves a SUBSET of the old
> surface.
> — `aurora/src/main/aether/unserved.ts:4-10`, at `638df0a`

And again in the client:

> WHAT ANSWERED `initialize`. Recorded because the socket chain does not identify the implementation: the
> legacy C++ server and the Rust core resolve the same path, so Aurora can be swapped between them with
> nothing in this codebase changing.
> — `aurora/src/main/aether/client.ts:55-58`

The MCP surface compounds it. `empyrean/CLAUDE.md:37` at `origin/main` records that the live
`mcp__oracle__*` tools bind to `oracle-old/linux-port/mcp/oracle-mcp` — *"verified at the registration
2026-08-22."* That shim is a **client**: it ships from the legacy repo and connects over the chain above,
so the server it reaches is whichever process holds the socket. A tool whose registration says
`oracle-old` can be driving the Rust core, and the client cannot tell.

**This is the whole problem in one sentence: a session can silently change which implementation it is
talking to, with no config change on either side and no signal on the wire.**

### 2.4 What each `initialize` result carries today — attributed separately

**The Rust server** (`crates/oracle-aether/src/engine.rs:1320-1415`, `082e6ce`) emits eight top-level
keys: `serverName`, `serverVersion`, `protocolVersion`, `capabilities`, `methods`, `methodSummaries`,
`limits`, `timingBasis` — plus the stamp and `droppedEvents` injected per §2.2/§2.3. `serverName` and
`serverVersion` both read from `self.config` (`:1344-1345`); `EngineConfig` declares them at `:150-151`
and defaults them at `:190-191`.

**The legacy C++ server** (`ControlSocket.cpp:2876-2882`, `d629771`) emits **five**:

```cpp
json result = {
    {"serverName",      kServerName},        // "oracle"        (:2693)
    {"serverVersion",   kServerVersion},     // "2.1-linux"     (:2694)
    {"protocolVersion", kProtocolVersion},
    {"capabilities",    Capabilities(ctx)},
    {"methods",         AdvertisedMethods()},
};
```

No `timingBasis`, no `limits`, no `methodSummaries`. `timingBasis` appears **nowhere in `oracle-old` at
`d629771`** — `git grep -n timingBasis d629771` over the whole repo exits 1, while a control grep for
`serverName` in the same file exits 0 with one hit, so this is an absence and not a failed command.
`timingBasis` is REQUIRED by D16 (`protocol.md:249`), by §2.1 (`:360`), by §8 item 17 (`:1966`), and by
the published fragment's `required` array.

**Both satisfy C4 already.** The Rust side by construction — *"[`METHODS`] holds the function pointers,
dispatch is a lookup in it, and `initialize` reports its names. There is no second list to fall out of
sync with, in either direction"* (`engine.rs:8-10`); `dispatch` is `METHODS.iter().find(…)` at `:1291`.
The legacy side likewise — `AdvertisedMethods()` (`ControlSocket.cpp:2715-2722`) iterates `Handlers()`,
the same map `RunMethod` looks up at `:2799-2800`.

### 2.5 Every discriminator available today, enumerated — and why each is scheduled for deletion

This is the core of the evidence. Six things currently distinguish the two servers. **Not one of them is
identity.** Each is either an accident, a defect, or a name.

| # | Discriminator | Status |
|---|---|---|
| 1 | `serverName`: `"oracle-next"` vs `"oracle"` | **A name, and one side's is config-overridable.** The rename that inverts it is in flight — §3.2. |
| 2 | `timingBasis` present vs absent | The legacy server's **non-conformance** with D16 / §8 item 17. Dies the day it conforms. |
| 3 | `limits` present vs absent | Same: `limits` is in the fragment's `required` array. Dies on conformance. |
| 4 | `methodSummaries` present vs absent | Registered by CR-13 (`protocol.md:2443`), not required. A server may add it at any time. |
| 5 | `emulator/status` key spellings: `frameToken` / `symbolCount` vs `frame_token` / `symbol_count`, and `romBytes` present vs absent | The legacy `OpStatus` (`ControlSocket.cpp:410-435`) emits snake_case and no `romBytes`; `RunMethod` (`:2790-2825`) performs no key renaming. All three names are in the fragment's `required` array. Dies on conformance. |
| 6 | `status.romPath` relative vs absolute | **Our own SHOULD-violation** — see below. Dies when we fix our bug. |

**On #6, and this corrects the framing this CR was commissioned with.** The check that actually settled
which server was answering used `status.romPath`: our binary, launched with a relative ROM path, echoed
it back relative. That echo is verbatim and uncanonicalised — `main.rs:38` takes the bare argv string,
`:92` puts it on `MachineInfo`, `server.rs:402` hands it to `Engine::set_rom_path` (`engine.rs:1006`), and
`:1756` ships `self.rom_path` unchanged. But two things about the other half do not hold up:

- `protocol.md:1799` says, normatively, **"Absolute paths SHOULD be reported"** — precisely because *"a
  client that cannot see the path cannot tell which build it is looking at."* So the discriminating
  signal was **our server failing a SHOULD**, not a property of either implementation.
- The legacy server's `emulator/status` emits **no `romPath` at all**. `OpStatus`
  (`ControlSocket.cpp:410-435`) writes `ok`, `running`, `rom_loading?`, `pc`, `sp`, `sr`,
  `symbol_at_pc?`, `symbol_disp?`, `frame_token`, `symbol_count` — and nothing else. A repo-wide
  case-insensitive `git grep rompath d629771` returns twenty hits, none in `ControlSocket.cpp`. So the
  claim that "the C++ process carries an absolute one" is **not supported by the legacy source at
  `d629771`**, and either the running process was a different build or the observation was
  misattributed. **⟨RUNTIME⟩**

**The property survives the correction, and is strengthened by it.** A server can neither fake nor be
wrong about a value it never chose — that is exactly why the accidental discriminator worked. But the
value it worked on was a bug on one side and an absence on the other, and it will disappear the moment
either is fixed. **The suite is currently identifying its servers by the set of ways they are broken.**

### 2.6 The measured harm: a binary carrying 37 method names while source serves 41

The release binary at `oracle/target/release/oracle-aether`, built **2026-08-21 22:11:04 −0400**, contains
**37** of the 41 `emulator/*` method-name literals present in `engine.rs` at `082e6ce`. The four absent:

1. `emulator/step`
2. `emulator/step_over`
3. `emulator/step_out`
4. `emulator/run_to_scanline`

And the commit log says exactly why: `a05e34c` *"serve emulator/step, step_over and step_out"*
(2026-08-22 17:08) and `6fc3bd5` *"serve emulator/run_to_scanline"* (2026-08-22 20:15) both post-date the
binary.

**Method, and its weakness.** The 41 names come from `grep -c 'name: "emulator/'` on `engine.rs`; the
binary's set from `strings -a -n 4` with a substring test for each of the 41. String literals are packed
contiguously in `rodata`, so a naive `^emulator/…$` match returns junk (`emulator/checkpointcapture`) —
the substring test is what makes the comparison sound, and it is one-directional: it proves a name is
**present**, so a *missing* name is strong evidence and a *present* one is merely consistent with the
method being served. Confirming served-ness needs a handshake. **⟨RUNTIME⟩**

**What the two builds would say about themselves.** Both would answer `serverName: "oracle-next"` and
`serverVersion: "0.0.0"` — the crate pins `version = "0.0.0"` at `Cargo.toml:3`, so `CARGO_PKG_VERSION`
is that literal string for every commit this repo has ever had. **Two builds differing by four served
methods are byte-identical under both existing identity fields.** That is the incident this CR exists to
close.

For completeness, the 58 schema fragments split against the 41 served names as **41 served + 17
schematized-but-unserved**, difference-set computed by parsing rather than counted by hand. The 17:
`audio_spectrum`, `breakpoint_add`, `breakpoint_clear`, `breakpoint_list`, `get_channel_states`,
`get_layer_states`, `log_clear`, `ping`, `set_channel_enabled`, `set_layer_enabled`, `vgm_start`,
`vgm_status`, `vgm_stop`, `wait_for_break`, `write_vram`, `z80_read`, `z80_write`. The served-not-in-schema
set is **empty**.

### 2.7 The count-drift is live, and it happened today, in the contract repo

`empyrean/CLAUDE.md:38` at `origin/main` `cc88d38` says the Rust server *"[s]erves **40** of the 58
documented methods over Aether; **18 pinned-unserved are the acceptance delta** (`58 == 40 + 18`)."*

My parse says **41 + 17**. The discrepancy is one method, and it is datable to the minute:
`run_to_scanline` was served at `6fc3bd5`, **2026-08-22 20:15:20**. `cc88d38` was authored **20:38:48** —
**23 minutes later.** The contract repo's own hand-carried count was already stale when it was written.

This is not a criticism of that row; it is the fourth independent instance of the same failure in this
document, and the reason §8 refuses `methodCount`:

1. The original `list_ops` advertised **34 of 47** before anyone noticed (`protocol.md:75`, D4's own
   motivating story).
2. §6 carried *"Oracle now advertises 53 ops"* until it was deleted for this reason (`:2260`).
3. The schema's top-level `description` carried a hand-maintained fragment count **across six recounts and
   was still wrong** — it read `37` while the file held `58`. The number is now deleted rather than
   repaired, because *"repairing the number would only reset that clock."*
4. `empyrean/CLAUDE.md:38`, stale by one within 23 minutes, today.

### 2.8 The consumer, and what this CR costs it

`aurora` is the only programmatic consumer of the handshake. Read at `638df0a`:

- It stores `capabilities` **raw** — `capabilities?: Record<string, unknown>` (`client.ts:78`) — rather
  than branching on it. That is a deliberate hold: it does not yet know which flags are trustworthy.
- It computes `methodCount: methods.length` at the wire (`client.ts:149`, and again as
  `servedMethodCount` at `:314`). **This is not a server field.** It is aurora's own measurement, and its
  comment says exactly what it is standing in for:

  > `serverName` **IS** a real discriminator today and not an invented one … **but it is a name, and names
  > get aligned.** `methodCount` is the load-bearing field: an installed `oracle-aether` binary can banner
  > a different count from the source tree it was built from, and a consumer measuring against the binary
  > gets the older answer with nothing announcing it.
  > — `client.ts:60-67`

  Read that against §2.6: the consumer described the 37-vs-41 incident **before it was measured**, and
  reached for a count because nothing better existed. The want is build identity. The count is a proxy.
- It logs the handshake once, *"because this is the only moment the answer is free. Two different servers
  answer this socket; the count is how a swap becomes visible in a log rather than as a feature that
  stopped working"* (`client.ts:153-155`).
- It keeps `handshake` after teardown deliberately, while clearing `methods` — *"a stale capability is a
  lie, but 'which server answered' is a record of something that happened"* (`client.ts:105-110`). That
  distinction is correct and this CR preserves it: `implementation` and `serverBuild` are records, not
  capabilities.
- Its fixtures already disagree with reality: `serverVersion: '0.1.0'` (`__tests__/client.test.ts:31`)
  and `'0.4.1'` (`__tests__/unserved.test.ts:42`). Neither is `0.0.0`. Nobody noticed, because nothing
  depends on the value — which is C3's argument in miniature.

---

## 3. The three properties

These are the substance of this CR. §4–§8 are mechanisms, and an adjudicator who takes the properties and
rejects every mechanism has given this CR what it came for.

### 3.1 P1 — implementation identity and build identity are two facts, and they fail independently

> **The handshake MUST carry, as separate fields, (a) which implementation of this contract is answering
> and (b) which build of that implementation.**

They are not two spellings of one thing. Each fails without the other:

- **Same implementation, different builds** — §2.6, measured. Two `oracle-aether` binaries agreeing on
  every implementation-level fact and disagreeing about four methods.
- **Different implementations, same build identity** — not merely possible but *default*: `serverVersion`
  is `"0.0.0"` on ours and `"2.1-linux"` on theirs, and both are constants that would remain equal to
  themselves across any imaginable change to either server.

A single "identity" field collapses these, and whichever one it is made to track, the other question
becomes unanswerable. The contract already applies this reasoning elsewhere: `capabilities.checkpoints`
is *"an object rather than a bare flag, because the cap must be discoverable before a client plans around
it"* (schema, `capabilities.checkpoints.description`). Same move, one level up.

### 3.2 P2 — neither field may be forgeable by configuration

> **Neither `implementation` nor `serverBuild` may be settable by configuration — not by a config struct,
> a config file, an environment variable, a command-line flag, or any bus method. A server that makes
> either settable is non-conformant, whether or not anyone sets it.**

**Be precise about what a wire contract can and cannot do here.** A contract cannot make a value
unforgeable; a server can put any bytes it likes on a socket. What a contract *can* do is move forging
from **a supported configuration** to **a contract violation** — and that is exactly the change being
asked for. Today, setting `EngineConfig::server_name` is a supported thing to do. Under P2 it would not
be. The property protects against an honest server that is *wrong*, and against an *unremarkable* config
change made for an unrelated reason. It does not protect against a server that lies, and this CR does not
claim it does.

**The worked demonstration, and it is not hypothetical.** `serverName` discriminates the two servers
perfectly today. That is precisely what makes it dangerous.

Consider a consumer guard of the form `if (serverName === 'oracle') { /* legacy path */ }`. It is green
today and correct today. Now:

- `empyrean/CLAUDE.md:38` at `origin/main` names the Rust successor **"Oracle" (was `oracle-next`)**, and
  `:37` records that the legacy row *"used to say `oracle/`, which now means the Rust repo below."* The
  repo rename has already happened.
- The commit that wrote those rows, `cc88d38`, is titled *"the keystone was false in this file for weeks,
  and the two emulator rows were **inverted by the rename**."*
- D3 (`protocol.md:66-70`) explicitly blesses renaming tools: *"you can rename any tool … forever without
  touching the wire."*
- `serverName` is one line of config away from `"oracle"` on our side (`engine.rs:190`).

So the day the successor's `server_name` default catches up with the product name it has already been
given, that guard **silently takes the legacy branch against the Rust core**. Not red. Green, and wrong.
No test fails, no log line changes, nothing announces it.

**That is the shape of the hazard: `serverName` is not theoretically forgeable, it is forgeable and
currently unforged.** A lane that tests `serverName === 'oracle-next'` to mean "I am on the Rust core"
stays green for months and inverts the day someone makes an unrelated config change. It is a
discriminator with an expiry date nobody has written down.

### 3.3 P3 ⚑ — build identity is STRUCTURALLY EMITTED, never SELF-REPORTED

> **`serverBuild.id` MUST be fixed when the binary is produced and MUST NOT be obtained, at run time,
> from anything the running process could have gotten wrong — not a file, not an environment variable,
> not a generated config, not a flag, not a sibling process.**

The reason is not accuracy. A build-generated config file is usually accurate. The reason is that **a
process which *reads* its identity has an opinion about it, and an opinion can be stale, mismatched, or
copied from a neighbour.** A process that has it compiled in cannot be wrong about it, because it never
chose the value.

This is the same property that made the accidental `romPath` check work (§2.5): it settled the question
precisely because neither server had an opinion about the answer. The lesson generalises, but the
accident does not — which is why the property must be written down and the accident must not be relied on.

**A *reported* build identity is a self-report again, merely better-sourced.** A binary that reads
`build.json` at start-up will happily report the build of whatever `build.json` is on disk: the one from
the last `cargo build`, the one from a colleague's tree, the one from the release that was installed over
it. That is the 37-vs-41 failure wearing a tidier hat.

**The regression clause, stated so a future refactor cannot pass as a tidy-up:**

> A change that moves `serverBuild.id` from a compile-time constant to any value read at start-up —
> **including a generated file written by the same build that produced the binary** — is a **regression
> against this property**, not a refactor, and MUST be adjudicated as an amendment to this section.

**What P3 does not claim.** It does not make the value unforgeable (see P2), it does not detect a
tampered binary, and it does not help if the build system itself computes the wrong id. It closes exactly
one failure: *an honest server confidently reporting an identity that belongs to something else.* That is
the failure that actually happened here.

### 3.4 What the three properties do not claim

- They do not make a lying server detectable.
- They do not replace capability negotiation. A client still branches on `capabilities` and `methods`,
  never on `implementation` (D5's rule, unchanged — see §10).
- They do not make the two implementations interchangeable, and they are not a step toward that. They
  make the *difference* legible, which is the opposite move.

---

## 4. C1 — `implementation`: which server this is

### 4.1 The proposal

A new key in the `initialize` result, top-level, **REQUIRED**:

```json
"implementation": "oracle-rs"
```

- **Type:** string, from the registry in §2.1 (below). A value outside the registry is non-conformant.
- **Immutable:** MUST NOT be settable by configuration, environment, flag, or method (P2).
- **Not a display name:** it names an implementation lineage, not a product, not a repo, not a release.

The registry, added to §2.1 and extended only by amendment:

| Value | Implementation |
|---|---|
| `oracle-rs` | The Rust `oracle-aether` server (`oracle/crates/oracle-aether`). |
| `oracle-cpp` | The legacy C++ `ControlSocket` server (`oracle-old/linux-port/gui/ControlSocket.cpp`), a Linux port of Exodus. |

### 4.2 Why a registry rather than a free string

A free string is `serverName` again with a stricter comment attached. The registry is what makes a
consumer's check **stable**: `implementation === 'oracle-rs'` has one meaning that only an amendment can
change, and a value nobody registered is a bug the consumer can name rather than a branch it silently
falls out of.

It also gives the contract a place to *record* that there are two implementers — which is a fact the
audit already leans on (*"'Which implementation has this been built against?' has two different
answers"*) but which appears nowhere a client can read.

### 4.3 Why not reuse or repair `serverName`

Three reasons, and the third is the one that decides it:

1. **It is shipped and consumed.** aurora logs it and stores it (`client.ts:145`, `:157`), and
   `MethodNotServedError` puts it in its message (`unserved.ts:68-71`). Changing what it means breaks a
   consumer for no gain.
2. **It is genuinely useful as a deployment label.** Two `oracle-rs` processes on one machine — a hosted
   embedding and a standalone — want distinguishable names, and a config field is the right tool for
   that. Making it immutable would delete a capability to fix a different problem.
3. **A field that is both a display name and a machine discriminator is the defect.** Splitting them is
   the fix. `serverName` keeps the job it is good at; `implementation` takes the job it was never
   designed for.

### 4.4 Why top-level rather than inside `capabilities`

D16's argument, verbatim and transposed: `timingBasis` is *"a top-level key of the `initialize` result
rather than a capability flag — it is not a thing a server may or may not support, it is what that
server's stamps mean"* (`protocol.md:360-362`). Identity is not a capability either. Every server has one;
`capabilities` is for things a server may lack.

There is a second reason specific to this CR: aurora keeps `handshake` after the link dies precisely
because it is *"a record of something that happened"* while clearing capabilities as lies
(`client.ts:105-110`). Putting identity in `capabilities` would place it on the wrong side of a
distinction a consumer has already drawn correctly.

### 4.5 Alternatives rejected

- **Derive it from `methods`.** A fingerprint of the advertised set would discriminate the two servers
  today. It fails P1 outright: it changes when the *build* changes, so it cannot answer the
  implementation question, and it would have reported the 2026-08-21 binary and the `082e6ce` source as
  different implementations.
- **A new `emulator/whoami` method.** A method can be unserved (that is this bus's whole discovery
  story), so identity would be discoverable only on servers that chose to be discoverable. The handshake
  is the one exchange every conformant server performs.
- **Put it in `capabilities.implementation`.** §4.4.
- **Leave it to `serverName` plus documentation.** This is the status quo with a comment. §3.2 is the
  answer.

---

## 5. C2 — `serverBuild`: which build of it

### 5.1 The proposal

A new key in the `initialize` result, top-level, **REQUIRED**:

```json
"serverBuild": { "id": "6fc3bd5a…", "source": "vcs", "dirty": false }
```

| Field | Type | Required | Meaning |
|---|---|---|---|
| `id` | string | yes | Opaque. Two servers reporting equal `id` **and** equal `implementation` MUST be the same build. Clients MUST NOT parse it, order it, or compare it for anything but equality. |
| `source` | string enum | yes | How `id` was derived: `"vcs"`, `"content"`, or `"declared"`. |
| `dirty` | boolean | when `source` is `"vcs"` | Whether the working tree had uncommitted changes to the sources that went into this binary. |

`source` values:

- **`"vcs"`** — a revision identifier from version control (a git commit hash). Requires `dirty`.
- **`"content"`** — a digest computed over the built artifact or its inputs. Self-consistent by
  construction; no `dirty` is meaningful.
- **`"declared"`** — the build system was told a string and embedded it. **A self-report, and typed as
  one on purpose.**

### 5.2 Why `source` is REQUIRED and not a nicety

This is P3 made checkable. An opaque `id` alone cannot tell a consumer whether the id is *trustworthy*: a
`"declared"` id is exactly the self-report P3 exists to reject, and a consumer that treats it like a
`"vcs"` id has been fooled by a field that was supposed to prevent being fooled. Typing the weak case
means a consumer can decide what to do with it — and means a build pipeline that quietly degrades from
`"vcs"` to `"declared"` announces the degradation instead of hiding it.

`"declared"` is admitted rather than banned because packagers exist: a distribution rebuilding from a
tarball has no VCS and would otherwise be forced to either lie or be non-conformant. Naming it beats
either.

### 5.3 Why `dirty` is REQUIRED under `"vcs"`

A commit hash from a dirty tree names a commit whose content is **not what was built**. That is the
37-vs-41 failure with a more convincing-looking field: a consumer reads a real hash, resolves it, and
gets source that does not describe the binary answering it. `dirty: true` costs one boolean and turns a
confident wrong answer into a stated uncertainty — which is the same trade §2.4's `caveat` machinery
makes bus-wide.

### 5.4 The normative rule on when `id` must change

> **`serverBuild.id` MUST differ between any two builds whose observable behaviour on this bus can
> differ.** In practice: any change to the served surface, to a served method's behaviour, or to an
> advertised limit or capability. A build system that satisfies this by changing `id` on *every* build is
> conformant and is the recommended implementation; a build system that reuses an `id` across a change in
> served methods is not.

Stated as a floor rather than a recipe because the recipe belongs to each implementer. A VCS hash
satisfies it whenever the change was committed, which is why `dirty` is the escape valve for when it
was not.

### 5.5 Why not `buildId`

**`buildId` is already taken, in this contract, for a different thing.** The `emulator/romReloaded` event
carries `buildId?` — *"reserved for the future build manifest; emit if known"* (`protocol.md:639`) — and
`:779` anticipates checking *"a `buildId` against the loaded ROM."* That is the **ROM's** build, not the
server's. Two `buildId` keys on one bus meaning two different artifacts is a name collision a consumer
will get wrong, and `serverBuild` reads unambiguously beside `serverName` and `serverVersion`.

The audit already counts `buildId` among the camelCase keys it tracks (`2026-08-22-protocol-schema-audit.md:532`),
so the name is live and not vestigial.

### 5.6 Alternatives rejected

- **Make `serverVersion` carry it.** It is already shipped by both implementations with a different
  meaning, is not REQUIRED by the schema, and reads as a release label to every human who sees it.
  Redefining a shipped field is more disruptive than adding one, and C3 does the cheaper thing.
- **A `X-Build` style header or an out-of-band file.** There is no header; the bus is NDJSON. A file is
  the P3 violation by name.
- **`emulator/ping`'s `version`, per audit D-01 reading (c).** D-01 documents that this reading exists
  and that *"all three are shippable today and a client cannot tell them apart."* Building on a field
  whose meaning is actively undecided, on a method our server does not serve at all (§2.6), and which
  D-01's own recommendation proposes to pin as a constant, would be building on sand. This CR takes
  D-01's recommendation as read and gives the build number the home it wanted.
- **Embed the whole `methods` set hash.** That is C5's proxy again, and it conflates surface with build
  (§4.5).

---

## 6. C3 — `serverVersion`: defused, and made REQUIRED

### 6.1 The finding

`serverVersion` is in the §2.1 example (`protocol.md:339`) and in the schema's `properties`, but **not in
its `required` array** — which lists `serverName`, `protocolVersion`, `capabilities`, `methods`,
`timingBasis`, `limits`. A conformant server may omit it entirely.

Its shipped values carry no information: `"0.0.0"` for every commit this repo has ever had
(`Cargo.toml:3`), and `"2.1-linux"` unchanged since the port. And a consumer's fixtures already disagree
with both (`0.1.0`, `0.4.1` — §2.8), which nobody noticed because nothing depends on the value.

### 6.2 The proposal

Keep it, make it **REQUIRED**, and defuse it:

> `serverVersion` is a **human-facing release label**. Clients MUST NOT branch on it, parse it, or order
> it. It has no defined format. A client that wants to know which build is answering reads `serverBuild`
> (§2.1); a client that wants to know which implementation reads `implementation`.

That is D-01's own remedy applied to a second field — *"pin `version` … and say in the row that clients
MUST NOT branch on it."* A shipped field with no rule attached is where a client's wrong assumption goes
to live undetected; a shipped field with a MUST NOT is a field that cannot hurt anyone.

### 6.3 The case for striking it instead

Genuinely strong, and handed to the adjudicator in §12. Once `serverBuild` exists, `serverVersion` has no
job that another field does not do better, and every key kept alive is a key someone eventually branches
on despite the prose. §2.8's fixture drift is a small live example of a field nobody is maintaining.

**Recommended: keep and defuse.** Striking a key both implementations currently emit is a wire change for
zero functional gain, and §2.8 shows the consumer surfaces it in a log line where a human release label
is exactly the right thing.

---

## 7. C4 — determining servedness before acting

### 7.1 The consumer hazard, in the consumer's words

> A client that only checks the list trusts an advertisement it has no way to audit; a client that only
> reads `-32601` **pays a round trip and a pause window** to learn something the handshake already said.
> — `aurora/src/main/aether/unserved.ts:31-33`

The pause window is the harm. A consumer whose flow is **pause → write → resume** and which discovers
unserved-ness only from the `-32601` on the write has **already stopped the machine** — a state change
caused by a call that was always going to fail. If the *resume* is the unserved method, the machine stays
stopped: aurora anticipates exactly this, noting that *"a `resume` the server cannot serve leaves the
machine stopped, and 'the warp hung' is what the user sees"* (`unserved.ts:100-103`).

This is the failure mode the whole `-32005` rule exists to prevent, arriving by a different road. §5's
rule *"never pause the machine to service a request"* forbids the server from doing it; nothing stops the
*discovery protocol* from making a client do it to itself.

### 7.2 What the contract must expose — and mostly already does

**Precedent first.** D4 already says the response lists *"the **exact set of supported methods**"*
(`protocol.md:73-74`), and the schema already says `methods` is the *"[a]uthoritative live method set."*
The pre-check material is there. Two things are missing:

1. **No consequence is stated.** Nothing says what it means if an advertised name answers `-32601`.
   aurora reads that gap correctly and conservatively: *"a method can be ADVERTISED AND UNIMPLEMENTED,
   and only the reply proves it"* (`unserved.ts:27-29`). Under the current text that reading is right,
   and it is why aurora must keep both detection routes.
2. **It is not on §8's checklist**, so no server's conformance suite is asked to test it.

### 7.3 The proposal — §8 item 23

> **23. Every advertised method dispatches** (D4, §2.1). A name present in `initialize`'s `methods` MUST
> be dispatchable: a server MUST NOT answer `-32601` for a name it advertised. `-32601` for an advertised
> name is a **server defect**, not a discovery mechanism. Correspondingly, a name absent from `methods`
> MUST answer `-32601` if called. `methods` is therefore a **warranty**, not an advertisement, and a
> client MAY treat membership as sufficient to decide servedness **without issuing a call** — which is
> the point: a client that must call to find out has already changed the machine's state to learn
> something the handshake told it.

**This costs both implementations nothing today** — §2.4 shows each derives its advertised list from its
dispatch table, so both satisfy it structurally. The cost is a conformance test each side already has the
material to write.

### 7.4 What this CR deliberately does **not** do

**It does not mandate client behaviour.** It does not say a client MUST pre-check, MUST NOT call an
unadvertised method, or MUST route through any particular error type. Clients own their flows, and a
client with a good reason to call optimistically (a hot path where the round trip is cheaper than the
lookup) is not doing anything wrong.

What the contract owes a client is that **the cheap check is sound** — that `methods` is a warranty it
can rely on, so pre-checking is a real option rather than a guess. Whether to take that option is the
client's call. This boundary is drawn on purpose: over-reaching into client conduct would be this
document doing to aurora what §8 forbids the emulator side doing to the contract.

### 7.5 Alternatives rejected

- **Require servers to answer a distinct error code for "advertised but unimplemented."** This
  *legitimises* the state item 23 makes a defect. A method that cannot run should not be advertised.
- **Require a `dryRun` or `supports` param on every method.** A second discovery mechanism next to
  `methods`, which is D4's original sin (`list_ops` beside the truth).
- **Leave it to consumers.** That is the status quo, and it costs the consumer a permanent second code
  path plus a pause window per discovery. C4 lets aurora keep its `'rpc-error'` route as a *defect
  detector* — which is a better use for it, and one its own comment already gestures at.

---

## 8. C5 — `methodCount`: refused, and made unnecessary

### 8.1 The ruling

**Do not add `methodCount` to the wire.** C2 removes the need for it.

### 8.2 The argument

**First, the factual correction.** `methodCount` is **not** a field aurora receives from a server. It is
computed at the wire from what did arrive: `methodCount: methods.length` (`client.ts:149`). The consumer
is not asking the contract for it and would gain nothing from being sent it.

**Second, precedent — and it is on the nose, twice.**

> **No count is maintained here.** … The size of this surface is **the table below**, and the size of any
> given server's surface is **the list `initialize` advertises** (D4). **A number in prose is a third
> answer that must be kept equal to the other two by hand**, and D4 exists precisely to retire
> hand-maintained op inventories — the original `list_ops` drifted to advertising 34 of 47 before anyone
> noticed. … That is why the count was never load-bearing.
> — `protocol.md:830-839`

And in the schema, on the very field in question:

> **This array IS the count of the server's surface; no number in prose tracks it.**
> — `handshake.initialize.result.properties.methods.description`

A `methodCount` field is not a *prose* count, so it escapes the letter of both. It does not escape the
reason. It would be a **second machine-readable answer to a question `methods` already answers exactly**,
and the only thing a second answer can add is the possibility of disagreeing. §2.7 enumerates four live
instances of exactly that failure, one of them 23 minutes old at the time of writing.

**Third, it is the wrong instrument for the job the consumer is using it for.** aurora's comment
(`client.ts:60-67`) says the load-bearing question is whether *"an installed `oracle-aether` binary can
banner a different count from the source tree it was built from."* That is a **build-identity** question,
and a count is a lossy proxy for it: two builds serving the same 41 methods but differing in a method's
*behaviour*, a limit, or a capability flag are equal under the count and different in every way that
matters. §2.6's incident happened to move the count; the next one need not.

`serverBuild` answers that question exactly, which is what "made unnecessary" means here.

### 8.3 The strongest case for blessing it, and the condition that would reverse this

Stated properly, because it is not weak:

`methods` is an array, and §2.4 makes containers on this bus **policy-bounded** — bounded, refused or
truncated with the truncation reported. Today `methods` is unbounded on both servers. If it ever became
policy-bounded — a server with a very large surface paging it, or a transport clipping it — then
`methods.length` would become a **floor**, not a count, and every consumer computing `methodCount` from
it would silently start under-reporting. At that moment the `checkpoint_list` precedent applies exactly:
that row was **registered** with `total` / `returned` / `limit` (`protocol.md:2443`) for this reason.

**So the ruling is conditional, and the condition is worth writing into the amendment:**

> If `methods` ever becomes a policy-bounded container, an explicit total becomes REQUIRED alongside it,
> on the `checkpoint_list.total/returned/limit` precedent. While `methods` is complete, the array is the
> count.

A second, weaker case: a count survives log truncation where a 41-element array does not. That is real
but is a *client* logging concern — aurora already solves it by logging `methods.length` itself
(`client.ts:160`) — and does not justify a wire field.

### 8.4 What this asks of aurora, plainly

Nothing breaks. Keep computing `methods.length`; keep logging it. What changes is what it is *for*: once
`serverBuild` lands, the count stops being the answer to "which build is this" and goes back to being a
log line. The comment at `client.ts:60-67` — which is a correct diagnosis of a real gap — can be replaced
by a read of `serverBuild.id`, and `capabilities` can stop being stored raw as a hedge against not
knowing which server wrote it.

---

## 9. The exact deltas requested

### 9.1 `contract/protocol.md:338-350` — the §2.1 server-response example

Add two keys to the example object, immediately after `serverVersion`:

```json
{"jsonrpc":"2.0","id":1,"result":{
  "serverName":"oracle","serverVersion":"…","protocolVersion":1,
  "implementation":"oracle-cpp",
  "serverBuild":{"id":"d629771…","source":"vcs","dirty":false},
  "capabilities":{ … },
  …
}}
```

### 9.2 `contract/protocol.md`, new prose in §2.1, after line 358

> **`implementation` and `serverBuild` (REQUIRED) — which server answered, and which build of it.** They
> are top-level keys of the `initialize` result rather than capability flags, for D16's reason applied to
> identity: every server has one, and `capabilities` is for what a server may lack.
>
> **`implementation`** names the implementation lineage, from this registry, extended only by amendment:
>
> | Value | Implementation |
> |---|---|
> | `oracle-rs` | the Rust `oracle-aether` server |
> | `oracle-cpp` | the legacy C++ `ControlSocket` server (Linux port of Exodus) |
>
> **`serverBuild`** is an object: `id` (string, opaque — compare for equality only), `source`
> (`"vcs"` \| `"content"` \| `"declared"`), and `dirty` (boolean, REQUIRED when `source` is `"vcs"`).
> `id` MUST differ between any two builds whose observable behaviour on this bus can differ.
>
> **Neither is forgeable by configuration.** A server MUST NOT make either settable by config file,
> config struct, environment variable, command-line flag or bus method. A contract cannot stop a server
> from putting arbitrary bytes on a socket; what it can do is make forging these a **violation** rather
> than a supported configuration, and that is what this clause does.
>
> ⚑ **`serverBuild.id` is structurally emitted, never self-reported.** It MUST be fixed when the binary
> is produced and MUST NOT be read at run time from a file, an environment variable, a generated config,
> a flag, or a sibling process. The reason is not accuracy — a build-generated file is usually accurate.
> The reason is that a process which *reads* its identity has an **opinion** about it, and an opinion can
> be stale, mismatched, or copied from a neighbour; a process that has it compiled in cannot be wrong
> about it, because it never chose the value. **A change that moves this value from a compile-time
> constant to anything read at start-up — including a generated file written by the same build that
> produced the binary — is a regression against this clause, not a refactor, and MUST be adjudicated as
> an amendment to it.** `source: "declared"` exists so a build with no version control can say so rather
> than lie; a client that treats a `"declared"` id like a `"vcs"` id has been fooled by the field that
> was meant to prevent it.
>
> **`serverVersion` is a human-facing release label** and is now REQUIRED. Clients MUST NOT branch on it,
> parse it, or order it; it has no defined format. Which build is answering is `serverBuild`; which
> implementation is `implementation`. `serverName` remains a **deployment** label a config may set — two
> processes of the same implementation on one machine want distinguishable names — and MUST NOT be used
> to discriminate implementations.

### 9.3 `contract/protocol.md:72-77` — D4, one added sentence

> D4 also fixes **who** is answering, not only what they serve: the response carries `implementation` and
> `serverBuild` (§2.1). Discovery that says what is served but not what is serving it leaves a client
> unable to tell one implementation's answer from another's, which is the gap D4's own `list_ops` story
> is a special case of.

### 9.4 `contract/protocol.md` §8, new item 23 (after line 2007)

Text as given in §7.3 above, **plus the dispatch-not-succeed clause added at consumer review
(§9.4.1)**, which is not optional decoration: without it the item is likely to be read backwards.

#### 9.4.1 Added at consumer review — **item 23 requires a method to DISPATCH, not to SUCCEED**

*Contributed by the aurora lane reviewing as the consumer; adopted by the overseer and folded in
before adjudication so the adjudicator rules on the version that survives contact.*

**Normative text to accompany item 23:**

> Item 23 governs **name resolution only**. A method named in `methods` MUST resolve to a handler and
> be dispatched to it; `-32601` in reply to an advertised name is a server defect. **It does NOT
> require the call to succeed.** A handler that dispatches and then refuses on its own domain terms —
> no ROM loaded, machine running where the row requires paused, no symbol table, a bound exceeded —
> is **conformant and is answering truthfully**. `-32601` means *"I do not have this name"*; a domain
> refusal means *"I have this name and here is why I will not do it now"*. These are different
> answers and only the first is barred.

**Why the clause is load-bearing rather than clarifying.** A future implementer reading *"every
advertised method MUST dispatch"* without it will hear *"you may only advertise what you can always
do"*, and will then take one of two bad paths: **drop a legitimate method from `methods`** whenever it
can conditionally refuse, or **invent the gated-discovery mechanism §7.5 rejects**. The first is the
dangerous one — **a server that under-advertises to stay conformant breaks the warranty by satisfying
it**, and it does so silently, because a shrunken `methods` array looks exactly like a smaller server.

**Precedent already in the contract, which is why this costs nothing:** `require_paused` is this
pattern today — those rows dispatch and refuse `-32005` on machine state, are advertised
unconditionally, and no one reads that as a violation. The clause names an existing practice rather
than creating an exception.

### 9.5 `contract/protocol.md` §6, `emulator/ping` row and D-01

No change requested here, but flagged for the adjudicator: if D-01 is settled by pinning `version` as a
constant with a MUST-NOT-branch, that closes reading (c) — *"a server-chosen build number"* — and
`serverBuild` is where that want should be pointed. The two rulings should be taken in the same sitting
or the second will re-open the first.

### 9.6 Schema — `contract/schema/bus-protocol.schema.json`

In `handshake.initialize.result`:

- `required`: add `implementation`, `serverBuild`, `serverVersion` → `["serverName", "serverVersion",
  "implementation", "serverBuild", "protocolVersion", "capabilities", "methods", "timingBasis",
  "limits"]`.
- `properties.implementation`: `{"type":"string","enum":["oracle-rs","oracle-cpp"],"description":"Which
  implementation of this contract answered. Registry lives in protocol.md §2.1 and is extended only by
  amendment. NOT config-settable, and distinct from `serverName`, which is a deployment label."}`
- `properties.serverBuild`:
  ```json
  {
    "type": "object",
    "required": ["id", "source"],
    "properties": {
      "id":     {"type": "string", "minLength": 1,
                 "description": "Opaque build identifier, fixed when the binary was produced. Compare for equality only — never parse, order, or range-check. Differs between any two builds whose observable bus behaviour can differ."},
      "source": {"enum": ["vcs", "content", "declared"],
                 "description": "How `id` was derived. 'declared' is a self-report and is typed as one on purpose (protocol.md §2.1)."},
      "dirty":  {"type": "boolean",
                 "description": "Uncommitted changes were present in the built sources. REQUIRED when source is 'vcs': a commit hash from a dirty tree names a commit whose content is not what was built."}
    },
    "allOf": [{"if":   {"properties": {"source": {"const": "vcs"}}, "required": ["source"]},
               "then": {"required": ["dirty"]}}],
    "description": "protocol.md §2.1. Which BUILD is answering, as distinct from which implementation. Structurally emitted, never read at run time."
  }
  ```
- `properties.serverVersion.description`: *"Human-facing release label. Clients MUST NOT branch on it,
  parse it, or order it; no format is defined. Build identity is `serverBuild`; implementation identity
  is `implementation`."*
- `properties.serverName.description`: *"Deployment label. MAY be config-set, so it MUST NOT be used to
  discriminate implementations — use `implementation`."*

### 9.7 ⚠ Three obligations this CR creates that **no fragment can express**

Item 20's closure validates *shapes*. These three are behavioural and must be tested in each server's own
harness, not the schema:

1. **P2 — non-forgeability.** A schema cannot see whether a value came from a config field. The check is
   a source-level one on each implementation: `implementation` and `serverBuild.id` must not appear in
   any config type, env lookup, or argument parser.
2. **P3 — structural emission.** Likewise invisible on the wire. The check is that `serverBuild.id` is
   produced by the build (a `build.rs` / preprocessor constant) and that nothing reads it from the
   filesystem or environment at start-up. A test asserting the value is *correct* is not this test; the
   test is that the value has no run-time source.
3. **Item 23 — advertised implies dispatchable.** A conformance test iterating `methods` and asserting
   no `-32601`. Both servers get it structurally today (§2.4), so it is a regression guard rather than a
   fix.

---

## 10. What this CR does and does not bind

**It binds the contract.** If adopted, both implementations owe `implementation`, `serverBuild` and a
REQUIRED `serverVersion`, and both owe §8 item 23's test.

**What each implementation would owe, separately:**

| | `oracle-rs` (this repo) | `oracle-cpp` (`oracle-old/`) |
|---|---|---|
| `implementation` | a compile-time constant, removed from `EngineConfig` reachability | a `static const char*` beside `kServerName` |
| `serverBuild` | a `build.rs` embedding the commit + dirty flag | a preprocessor define set by the build |
| `serverVersion` REQUIRED | already emitted; value is `"0.0.0"` and remains conformant (no format is defined) | already emitted |
| item 23 | a test; the property already holds (`engine.rs:8-10`) | a test; the property already holds (`ControlSocket.cpp:2715-2722`) |

**It does not bind either implementation to a schedule.** In particular this CR takes no view on when —
or whether — the legacy server adopts it, and nothing here should be read as the successor's lane
speaking for the legacy implementer. If the legacy server is retired before it adopts these fields, the
registry simply never sees `oracle-cpp` on a wire, and the contract has lost nothing.

**It does not change D5.** Clients still branch on capabilities and on `methods`, never on a version
integer — **and, this CR adds, never on `implementation` either.** `implementation` is for logs,
provenance, bug reports and test-fixture selection. A client that branches its *behaviour* on which
implementation answered has reintroduced the two-vocabularies problem D5 exists to prevent, and this CR
does not license it.

**It does not touch `emulator/status`, `romPath`, or the path rule.** §2.5's finding that our
`status.romPath` violates §6's *"Absolute paths SHOULD be reported"* is a **defect in this repo's server**
and is reported here as evidence, not proposed as a contract change. It is a separate fix.

**It does not resolve the 41-vs-40 disagreement with `empyrean/CLAUDE.md:38`.** §2.7 reports it with both
derivations and dates; deciding whose number governs is not this CR's business, and the contract's own
rule (§6's no-count blockquote) already says neither prose number does.

---

## 11. Where this CR is weakest

### 11.1 It proposes three new REQUIRED keys, and §8 forbids the emulator side inventing

This is the procedural objection and it is the serious one. §8's ban on unilateral invention is what CR-7
(`timingBasis`) and CR-8 (`droppedEvents`) were both adjudicated under, and both were shipped-then-raised
— which is worse than this CR, which raises before shipping. But "raised first" is not the same as
"permitted": if the adjudicator holds that identity belongs to the *contract's* authors rather than to an
implementer, the correct outcome is that §3's properties are adopted and §4–§6's spellings are
redesigned upstream. That outcome would be a success for this document.

### 11.2 The registry in C1 is a maintenance burden with a failure mode

Every new implementation needs an amendment before it can conform. For a bus with two implementers that
is cheap; for a bus with ten it is friction, and the pressure would be to allow unregistered values,
which converts `implementation` back into `serverName`. A reasonable adjudicator could take the free
string plus a MUST-NOT-be-config-settable rule and accept the weaker check.

### 11.3 P2 cannot be enforced by the artifact that would carry it

§3.2 says this outright, but it bears repeating as a weakness: the schema cannot see a config field, so
P2 is honour-system-plus-source-review. Its real work is making a future config field a *violation*
rather than a *feature*, which changes what a review objects to but does not stop a determined
implementer.

### 11.4 `serverBuild` as an object may be one field too many

`dirty` and `source` are both defensible individually (§5.2, §5.3), and both were argued at the point of
use — but a bare REQUIRED string plus a rule ("it must differ when behaviour differs") would close the
37-vs-41 incident on its own. The object is the stronger design; the string is the cheaper one, and an
adjudicator preferring the string would not be wrong. §12 hands this over.

### 11.5 The commissioning mechanism did not check out

§2.5 corrects it: the legacy server emits no `romPath`, and our relative echo is a SHOULD-violation
rather than a property. The *property* P3 draws from that incident stands on its own reasoning, but this
CR is one exhibit lighter than it was commissioned to be, and an adjudicator should weigh P3 on §3.3's
argument rather than on the anecdote.

### 11.6 Nothing here was confirmed at runtime ⟨RUNTIME⟩

Collected: **(a)** the 41 served methods are a source grep, not a handshake; **(b)** the 37 in the release
binary are a `strings` substring test, sound for *absence* and merely consistent for *presence*;
**(c)** the running legacy process may be a different build from `d629771`, which is the hazard this CR
describes applied to its own evidence; **(d)** no `initialize` result from either server was observed on a
wire. A foreground session with the emulator MCP could settle all four in one handshake each — which is
itself an argument for this CR, since today the only way to know which server answered is to look.

### 11.7 One consumer is not a sample

aurora is the only programmatic consumer read. `seraph`, `sigil` and the MCP shim were not swept for
handshake use. A consumer that branches on `serverVersion` today would be broken by C3's MUST NOT — not
by a wire change, but by being told it was always wrong.

---

## 12. Questions for the adjudicator

### 12.1 Handed over undecided

1. **Are the three properties in §3 adopted?** They are the request. Every mechanism below them is
   negotiable.
2. **C1's registry, or a free string?** §11.2 states the case for the weaker form.
3. **`serverBuild` as an object `{id, source, dirty?}`, or a bare REQUIRED string?** §5.2/§5.3 argue the
   object; §11.4 argues the string.
4. **`serverVersion`: defused-and-REQUIRED (§6.2) or struck (§6.3)?** This CR recommends the former and
   the case against is real.
5. **Item 23's spelling.** *"A `-32601` for an advertised name is a server defect"* is the version here.
   An adjudicator may prefer the softer *"SHOULD dispatch"*, which would leave aurora's dual-route client
   permanently justified — that is the cost of the softer form and it should be paid knowingly.
6. **Does `implementation` belong in the schema's `enum`, or only in `protocol.md`'s registry?** An
   `enum` in the published schema makes an unregistered value a validation failure, which is stronger —
   and also means every new implementation needs a schema release before it can pass its own suite.
7. **Should D-01 and this CR be adjudicated together?** §9.5 argues yes.
8. **Is `oracle-rs` / `oracle-cpp` the right vocabulary?** They are language-flavoured, and D3 warns
   against brand names on the wire. These are not brands, but an adjudicator may prefer
   `oracle-successor` / `oracle-exodus` or similar.

### 12.2 Considered settled, and not asked

Listed so an adjudicator can object to the settling rather than have it pass silently.

1. **Implementation identity and build identity are two facts.** §3.1, and §2.6 is a measurement, not an
   argument.
2. **Neither existing field answers either question.** §1's table; both halves verified from source at
   named revisions on both implementations.
3. **Which server answers is decided by launch order.** §2.3, verified from the consumer's own source and
   from `empyrean/CLAUDE.md:37`. Not asked because nobody disputes it.
4. **`buildId` is unavailable as a name.** §5.5 — it is reserved for the ROM's build manifest at
   `protocol.md:639` and `:779`.
5. **Identity goes top-level, not in `capabilities`.** §4.4, on D16's stated reasoning.
6. **`methodCount` is not added.** §8, on §6's no-count blockquote, the schema's own `methods`
   description, and the fact that aurora derives it locally rather than receiving it — with the one
   condition in §8.3 that would reverse it.
7. **This CR does not mandate client behaviour.** §7.4. The contract owes a sound cheap check; taking it
   is the client's call.
8. **The schema holds 58 method fragments plus one `$comment`.** Derived by parsing, not counted.
9. **Our `status.romPath` violates §6's absolute-path SHOULD.** §2.5. Reported as a defect in this repo,
   not proposed as a contract change.

---

## 13. Provenance

| | |
|---|---|
| Written in | `oracle` worktree at `082e6ce`, branch `cr-c-identity` |
| Contract read at | `empyrean` `origin/main` `cc88d38` — `contract/protocol.md` blob `1e832b1`, `contract/schema/bus-protocol.schema.json` blob `9d8cc3c` |
| Legacy implementer read at | `oracle-old` `d629771` |
| Consumer read at | `aurora` `638df0a` (branch `master`; the repo has no `origin/main`) |
| Binary inspected | `oracle/target/release/oracle-aether`, mtime 2026-08-21 22:11:04 −0400, 1,925,824 bytes |
| Runtime | none — no `cargo`, no emulator, no MCP tool |
| Siblings | every sibling file read through `git show` at a named revision, never through the working-tree path |

**Models.** `docs/2026-08-22-cr-a-breakpoints.md` and `docs/2026-08-22-cr-b-z80.md`, for the shape: argue
each departure at the point of use, name where the document is weakest, and separate what is handed over
from what is settled so the settling can be objected to.
