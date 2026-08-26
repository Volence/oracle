# CR-C landed in the contract: what was applied, what the serve parcel still owes (2026-08-26)

**Pairing.** This is the oracle half of a two-repo parcel. The contract text and schema live on empyrean
branch **`cr-c-amendment`**, commit **`4b17949`** (*protocol §11.23 (CR-C): the handshake says which
implementation answered, and which build of it*), cut from empyrean `origin/main` **`be23578`**
(protocol.md blob `add2efc`, schema blob `7d1ea5b` at that base). This doc lives on oracle branch
**`cr-c-apply`**. Neither branch is merged; neither is pushed by this parcel.

**Sources.** The CR is `docs/2026-08-22-cr-c-server-identity.md` (blob `a8882d0` at oracle `main`
`d10a821`); the ruling is `docs/2026-08-26-ruling-cr-c.md` (landed in oracle `f22d128`), verdict **ADOPT
WITH CHANGES**. Where the two disagree the ruling wins, and that is how this was applied: the ruling's
M1-M5 amend the CR's §9 deltas, and its §12.1/§12.2 answers settle the CR's open questions. No emulator
tool was called, no server was started, no `cargo` was run.

**This parcel is contract text and schema only.** It ships no server code. Everything the reference server
must do to become conformant again is listed under *The serve parcel* below.

---

## What landed, delta by delta

All line numbers are on the amended empyrean file at `4b17949`.

| Delta | Where | Driven by |
|---|---|---|
| §2.1 example gains `implementation` and `serverBuild` after `serverVersion` | `contract/protocol.md:343-344` | CR §9.1, with S2's full-identifier SHOULD applied to the value |
| §2.1 prose block: the two keys, the registry, the `source` table, non-forgeability, the ⚑ structural-emission clause, `serverVersion` REQUIRED and defused | `contract/protocol.md:366-426` | CR §9.2 as amended by M1, M2, S2 and §12.1 answers 2, 3, 4, 6, 8 |
| D4 gains the who-is-answering sentence | `contract/protocol.md:76-79` | CR §9.3 |
| §8 item 23, with the dispatch-not-succeed clause and the per-build-warranty clause as sub-bullets | `contract/protocol.md:2173-2203` | CR §9.4 + §9.4.1 + §9.4.2, re-anchored per M5; MUST per §12.1 answer 5 |
| §11.23 amendment entry | `contract/protocol.md:4088-4225` (end of file) | M4 (renumbered from the ruling's §11.21 - see *Numbering* below) |
| Schema: three keys appended to `initialize.result.required` (`:367-377`); `implementation` (enum, `:387-394`); `serverBuild` (object, `if`/`then` on `dirty`, `:395-440`); descriptions on `serverName` (`:379`) and `serverVersion` (`:383`) | `contract/schema/bus-protocol.schema.json` | CR §9.6 as amended by M1's `source` description and §12.1 answer 6 |
| 11 new wire vectors (3 accepting, 8 refusing), group `handshake` | `contract/schema/tests/vectors.json:1503-1917` | Invariant 8 (vectors are gates) and the entry's adoption condition |
| `specExamples` §2.1 initialize result updated with the two keys | `contract/schema/tests/vectors.json:2041-2046` | S3 |

**Numbering.** The ruling's M4 says "add a §11.21 entry", because at ruling time (empyrean `fa7f2b6`) that
was the next free number. Two amendments landed since - §11.21 (breakpoints, CR-BP) and §11.22
(`z80_write` and the setter enums) - so the entry is **§11.23**. Every cross-reference written into
protocol.md and the schema says §11.23.

**One deviation from the CR's literal text, recorded rather than absorbed.** CR §9.6 spells the amended
`required` array with the three new names interleaved (`serverName, serverVersion, implementation,
serverBuild, protocolVersion, ...`). They are **appended** instead. `required` is a set in JSON Schema, so
the two forms are semantically identical, and appending means no existing array index moves - which is what
makes the leaf-level additivity proof below come back with *zero* changed values instead of six.

---

## Every ruling item: applied, or not, and why

### MUST

**M1 - a `"vcs"` id must fold in surface-changing build configuration. APPLIED**, in all three places the
ruling names:

- `protocol.md:386-390` - the §2.1 sentence after the differ-rule: *"Under `"vcs"` the id is the revision
  identifier extended by whatever build-time selection changes the served surface ... `dirty` covers
  uncommitted source; it does not cover configuration."*
- `protocol.md:396` - the `source` table's `"vcs"` row now reads *"Derived from a version-control revision
  identifier, plus any build configuration the rule above requires."*
- `bus-protocol.schema.json:413` - `serverBuild.properties.source.description` ends with *"'vcs' ids are
  derived from, not equal to, a revision: build configuration that changes the served surface is part of
  the id (protocol.md §2.1)."*

**M1's example is corrected, per the overseer's instruction, and the correction is in the §11.23 entry
rather than left implicit.** The CR's §9.4.2 and the ruling's M1 both describe the `synth`-gated audio
surface as **live** in this tree. It is not: `grep -c 'cfg(feature' crates/oracle-aether/src` returns **0**
(run in this worktree), so `oracle-aether` has no audio handlers under either feature state, and no build
of it advertises or serves those rows today. The **rule** M1 states is unaffected and stands as written -
`oracle-core` really does declare a default-off `synth` feature that `oracle-aether` depends on without
enabling - so §11.23 phrases it as a compile-time-optional surface the tree is *shaped for*, not one it
serves. §8 item 23's contract text states the general case and names no repo.

**M2 - P3's central sentence is false for the mechanism P3 recommends. APPLIED**, both deltas, in the ⚑
paragraph at `protocol.md:405-419`:

- *"cannot be wrong about it, because it never chose the value"* is replaced by *"has no opinion to be
  wrong about, provided the value is recomputed whenever its inputs change"*.
- The ⚑ paragraph gains **"The constant must be invalidated by what it names"** with the Cargo spelling
  (`rerun-if-changed` on `.git/HEAD` and the ref it resolves to), and states the line explicitly as
  **compile time versus run time**, with a build-generated file that is `include!`d named as conformant so
  the clause cannot be read as banning `OUT_DIR`.

**M3 - §11.9's review-scope note is stale and self-referential. NOT APPLIED HERE; it is a correction to
the CR document, not to the contract.** This parcel is contract text and schema only, and the brief's
oracle deliverable is this doc alone, so `docs/2026-08-22-cr-c-server-identity.md` was read and not
edited. The debt is real and is recorded here: §11.9 cites blob `d364edd` as "the current text" (it is
`f67e8eb`'s blob, which does not contain §11.9) and states the delta from the reviewed blob as +43/-1 when
the ruling measured **+94/-1** across §9.4.1, §9.4.2, §11.9 and the §12.1 item-5 verdict paragraph. Whoever
next touches the CR record should state the delta by section and cite the reviewed *commit* (`e93571b`).
Nothing in the contract depends on it.

**M4 - no amendment-log entry was proposed. APPLIED** as §11.23 (`protocol.md:4088-4225`). It carries all
six things M4 asks for: (a) the 37-vs-41 incident and the `status.romPath` finding; (b) the three
properties as adopted, with M2's correction folded into P3's wording; (c) the registry's two initial
values; (d) the M1 and M2 precisions; (e) that the schema adds three `required` keys to a fragment
`initialize` is exempt from closing, so the change is additive for every client that does not ask and a
conformance obligation for both servers; and (f) that neither server emits either key today, on the
`read_cram` precedent - fragment before handler, §8 item 20.

**M5 - re-anchor to the tip and absorb §11.19. APPLIED in the half this parcel owns.** Item 23 is anchored
after item 22's text at the current tip (the CR's "after line 2007" is long gone; item 22 now ends at
`:2171`), under its own *Added by the 2026-08-26 amendment (§11.23)* heading, matching how items 10, 15,
20, 21 and 22 are introduced. §11.19's narrowing is absorbed in §11.23's own ★ paragraph: launch order
decides only the **attached** arrangement now, and under own-instance a client knows the binary path and
still not the build, which moves §2.6's stale-binary hazard from the socket to the filesystem and makes
`serverBuild` more load-bearing rather than less. **The other half of M5 - adding a tip-revision row to the
CR's §2.1 table, a paragraph to its §2.3, and narrowing its §12.2 item 3 - is CR-document work and was not
done here**, for the same reason as M3.

### SHOULD

- **S1 (quote `capabilities.checkpoints.description` verbatim) - NOT APPLIED**: a CR-document correction.
  The schema text it misquotes is untouched by this amendment.
- **S2 (`id` SHOULD be the full revision identifier) - APPLIED** at `protocol.md:390-392`, and the §2.1
  example now carries a full 40-character hash (`6fc3bd57b5d50a0fcdb39c088ddf546d896a5151`, a real oracle
  commit - the one that took the served surface to 41) instead of an abbreviation with an ellipsis.
- **S3 (update the gate's §2.1 vector in the same commit) - APPLIED**, and it was load-bearing rather than
  tidy: the un-updated example is **refused** by the amended schema (proven, see below), so leaving it
  would have turned the gate red. Note that the ruling expected this vector to be `knownDefect` for D-25;
  at the tip it is `expect: "pass"` (D-25 was resolved), so it stays a pass vector and now passes with the
  two new keys.
- **S4 (mark the binary exhibit perishable) - NOT APPLIED**: a CR-document correction. §11.23 states the
  fact the contract needs - the exhibit is gone and the claim stands on the commit dates and the constant
  version.
- **S5 (name the reference suite's own `serverName` pin) - CARRIED into the serve list below**, and into
  §11.23's closing paragraph.

### The §12.1 answers

| # | Answer | Where it landed |
|---|---|---|
| 1 | All three properties adopted, with M1/M2 folded into P3 | `protocol.md:366-419`; §11.23's three-properties paragraph |
| 2 | Registry, not a free string | `protocol.md:371-381` (the table plus *extended only by amendment*) |
| 3 | The object `{id, source, dirty?}`, not a bare string | `protocol.md:383-398`; schema `:395-440` |
| 4 | `serverVersion`: keep, defuse, REQUIRED | `protocol.md:421-426`; schema `required` `:374` and `serverVersion.description` `:385` |
| 5 | Item 23 is a **MUST**, with the dispatch-not-succeed clause and the per-build reading | `protocol.md:2175-2203` |
| 6 | Enum in the schema **and** registry in the prose, with the prose governing | schema `:393` (*"THE REGISTRY IN protocol.md §2.1 GOVERNS ... this enum mirrors it"*); prose `:371-381` |
| 7 | D-01 in the same sitting | **Not ruled here** - D-01 belongs to `docs/2026-08-22-protocol-schema-audit.md` and the ruling explicitly declined to pin it. §11.23's closing paragraph records the pointer (reading (c) is closed by `serverBuild`) so the audit ruling does not reopen this one. ⟨owner⟩ |
| 8 | `oracle-rs` / `oracle-cpp` | `protocol.md:371-381`; schema enum `:389-392` |

The §12.2 settlings needed no contract text of their own except item 3, which M5 narrows (applied, above)
and item 9, which is a defect in this repo rather than a contract change (see the serve list).

---

## Additivity, proven the way the CR-28 ruling proved it

Script: `additivity.py` (written for this parcel; session scratchpad at
`/tmp/claude-1000/-home-volence-sonic-hacks-oracle/f47c1d71-ecc5-4890-9dac-88b4a1980950/scratchpad/`, so
re-derive rather than assume it survives). It walks both documents to scalar leaves keyed by JSON pointer,
and was run from the empyrean worktree root against `git show HEAD:contract/schema/bus-protocol.schema.json`
and the amended file. Output, verbatim in substance:

```
fragment count (schema.methods, $comment excluded): before 59, after 59
  fragments removed: none
  fragments added:   none
leaf count: before 2394, after 2419 (+25)
removed leaves (0):
changed leaf values (0):
added leaves (25): ...
prune test (delete `implementation`, `serverBuild`, the three added `required` entries,
 and the two added descriptions): deep-equal to pre-amendment = True
```

- **59 fragments before, 59 after**, re-derived by parsing both revisions (§11.17 clause 7). No method is
  added, renamed or removed.
- **Zero leaves removed. Zero existing leaf values changed.** The amendment names no changed value, and
  there are none - appending the three `required` entries instead of interleaving them is what buys that.
- **25 leaves added**: the `implementation` subtree (4), the `serverBuild` subtree (16), two descriptions
  on `serverName`/`serverVersion` that previously had none, and three `required` entries at indices 6, 7,
  8.
- **Prune test deep-equal = True**: deleting exactly the names this amendment adds yields a document
  byte-equal in structure to the pre-amendment one.

Additivity of the *change* to clients: `initialize.result` declares no `additionalProperties` /
`unevaluatedProperties`, and §8 item 22 exempts `initialize` from closure by name, so a client that never
asks for these keys is unaffected. In the asking direction the change makes both servers non-conformant
until they emit - the `timingBasis`/CR-7 path, taken deliberately this time.

---

## The gate

Command, run from the empyrean worktree root (`.../scratchpad/empyrean-cr-c`):

```
python3 contract/schema/tests/validate_contract_schema.py
```

**Before the amendment** (at `be23578`):

```
GREEN: schema well-formed; 59 params fragments closed; 61 pass-vectors, 86 red-vectors, 27 closure checks.
EXIT=0
```

**After** (at `4b17949`), full output preserved:

```
== G1  schema is well-formed against https://json-schema.org/draft/2020-12/schema
  ok  (whole document + 124 fragments, each checked on its own ...)
== G2  §2.5 / §8 item 22 — request params are closed
  ok  (59 method params fragments, all closed)
== G3/G4  wire vectors
  64 pass-vectors validated, 30 of them also under §8 item 20 closure
  94 fail-vectors proven red
== G5  §6 coverage
  §6 catalogues 67 methods; the schema fragments 59 of them.
  UNSCHEMATIZED (§9's open item ...): z80_registers, read_vdp_registers, read_vsram, object_slot,
  object_list, player_state, call_stack, log_tail   [8 rows, unchanged by this amendment]
== G6  the spec's own example payloads
  ok  x9   [all nine, including §2.1:337 initialize result]

GREEN: schema well-formed; 59 params fragments closed; 64 pass-vectors, 94 red-vectors, 30 closure checks.
EXIT=0
```

Deltas: **+3 pass-vectors, +8 red-vectors, +3 closure checks**. G2's count is unmoved (this amendment
touches no method params object), G5's eight unschematized rows are unmoved, and all nine spec examples
still validate.

### Red-first, 8 for 8

Invariant 8 requires a refusing vector proven red before the amendment. Script: `redfirst.py` (same
scratchpad), validating each new vector against **both** the pre-amendment schema and the amended one. Every one of the eight
refusing vectors was **accepted** by the pre-amendment schema and is **refused** by this one:

| Vector | Pre-amendment | Amended - message |
|---|---|---|
| `implementation` missing | accepted | REFUSED - `'implementation' is a required property` |
| `serverBuild` missing | accepted | REFUSED - `'serverBuild' is a required property` |
| `serverVersion` missing | accepted | REFUSED - `'serverVersion' is a required property` |
| `implementation: "oracle-next"` | accepted | REFUSED - `'oracle-next' is not one of ['oracle-rs', 'oracle-cpp']` |
| `source: "vcs"` without `dirty` | accepted | REFUSED - `'dirty' is a required property` |
| `id: ""` | accepted | REFUSED - `'' should be non-empty` |
| `source: "git"` | accepted | REFUSED - `'git' is not one of ['vcs', 'content', 'declared']` |
| `serverBuild` as a bare string | accepted | REFUSED - `... is not of type 'object'` |

The three accepting vectors cover the shape as written and both non-`vcs` sources: `"content"` and
`"declared"` are accepted **without** `dirty`, which is what makes the `if`/`then` conditional rather than
universal. And the §2.1 spec example, stripped of the two new keys, is **REFUSED** by the amended schema -
which is why S3's "same commit" is not a nicety.

Nothing was weakened to reach green. No vector expectation was copied from a server; each is derived from
the amendment text and cites §2.1 (§11.23) in its `why`.

---

## The serve parcel: what oracle owes before it is conformant again

None of this is done here, and all of it needs `cargo`, which this parcel did not run.

> **Served on branch `cr-c-serve`, 2026-08-26.** Items 1, 2, 3, 4, 6 and 7 are done; item 5 is the legacy
> shim's and is not this branch's. Ticks, commits and what moved are in *The serve parcel, served* at the
> end of this document — including two things the list below could not have anticipated, because the
> contract tip moved twice while the serve parcel was running: §11.24's D-03 (served here, `66ee253`) and
> §11.21's MCP tool row (**BLOCKED** — the file is in `oracle-old`).

1. **Emit `implementation` from `engine.rs`.** The `initialize` result is built at
   `crates/oracle-aether/src/engine.rs:1353-1355` (`serverName` / `serverVersion` come off
   `self.config`). `implementation` must be a **compile-time constant** with no path through
   `EngineConfig` - P2 makes a config-settable value a violation, and the check is source-level: the value
   must not appear in a config type, an env lookup or an argument parser. Value: `"oracle-rs"`.
2. **Emit `serverBuild`.** There is no `build.rs` anywhere under `crates/` today. It needs one that embeds
   `{id, source: "vcs", dirty}` **and declares `cargo:rerun-if-changed` on `.git/HEAD` and on the ref it
   resolves to** - M2 makes a cached build-script product non-conformant, so this is not optional
   hygiene. Under M1 the `id` must fold in any surface-changing build configuration; today
   `oracle-aether` compiles one surface under either `synth` state (0 `cfg(feature` hits in its `src`), so
   a plain revision id is conformant **now** and stops being conformant the moment a feature gates a
   method. S2: use the full revision, not an abbreviation.
3. **Move the identity pin off the display name.** `crates/oracle-aether/tests/handshake.rs:33` asserts
   `r["serverName"] == "oracle-next"` - the exact pattern §2.1 now bars (`serverName` is a deployment
   label a config may set). It becomes `implementation == "oracle-rs"`. Note the value drift too: the
   default is still the pre-rename string `"oracle-next"` (`engine.rs:190-191`).
4. **Refresh the vendored schema copy** at `crates/oracle-aether/tests/contract/bus-protocol.schema.json`.
   It is pinned to empyrean `9b46a235` (2026-08-22) at **58 fragments**; the tip is already at 59 (§11.21
   added `breakpoint_set_enabled`), so the refresh pulls in §11.21, §11.22 **and** §11.23 together.
   `tests/contract/PROVENANCE.md:16-17` records the source revision, the blob and the fragment count and
   must be updated with all three re-derived by parsing, per its own rule. **Expect the refresh alone to
   turn the handshake tests red**: `tests/common/schema.rs:226-228` compiles
   `handshake.initialize.result` **closed** and validates every handshake reply against it, so the three
   newly-required keys fail until items 1 and 2 are done. Sequence 4 after 1 and 2, or in the same change.
5. **The legacy shim's own emission.** `oracle-old/linux-port/gui/ControlSocket.cpp` emits five keys at
   `:2876-2882` and owes `implementation: "oracle-cpp"` beside `kServerName` (`:2693`) plus a
   preprocessor-defined `serverBuild`. §11.23 asks it for **no schedule**; if it retires before it adopts
   them, the registry entry retires with it. It also still owes `timingBasis`, which it has never emitted.
6. **Item 23's regression test.** Iterate the advertised `methods` and assert none answers `-32601`. Both
   servers satisfy it structurally today (dispatch table is the source of the list), so this is a guard,
   not a fix. Two behavioural obligations sit beside it and no fragment can express either: the
   non-forgeability check (item 1's source-level assertion) and the structural-emission check (that
   `serverBuild.id` has **no run-time source** - asserting the value is *correct* is a different test).
7. **`status.romPath`** (§12.2 item 9, this repo's own defect): the successor echoes the launch argument
   verbatim (`engine.rs:1016` sets it, `:1766` ships it, `main.rs:38` takes bare argv; the ruling's `:1006`/`:1756` were at oracle `082e6ce`), so it is relative
   whenever the launcher's was, against §6's absolute-path SHOULD. Not a contract change; a fix here.

## ⟨RUNTIME⟩ - for the controller's foreground follow-up

Nothing in this parcel depends on any of these; they are carried forward from the ruling and from the
serve list.

1. One `initialize` against a freshly built `oracle-aether`: confirm 41 method names on the wire and
   `serverName`/`serverVersion` = `"oracle-next"` / `"0.0.0"` (source says so; the wire has not been
   checked).
2. One `initialize` against whatever legacy process is running: confirm the five keys and the absence of
   `timingBasis`, and thereby whether the running build is `d629771`'s.
3. `emulator/status` on the Rust server launched with a **relative** ROM path: confirm `romPath` echoes
   relative, which is the wire half of item 7 above.
4. After the serve parcel: one `initialize` validated against the refreshed vendored schema **closed**,
   which is §11.23's adoption clause 1 executed against a real server rather than a vector.

## BLOCKED

Nothing was blocked. Four ruling items (M3, S1, S4, and M5's CR-document half) were **out of scope**
rather than blocked - they correct the CR record, not the contract - and each is named above with what it
owes. No MUST was degraded to reach green, and no vector was weakened.

---

# The serve parcel, served (branch `cr-c-serve`, 2026-08-26)

| # | Owed | Done | Commit |
|---|---|---|---|
| 1 | Emit `implementation` from `engine.rs`, compile-time, no `EngineConfig` path | ✅ | `0a4313e` |
| 2 | Emit `serverBuild` from a `build.rs`, with M2's `rerun-if-changed` and M1's fold-in | ✅ | `0a4313e` |
| 3 | Move the identity pin off `serverName` | ✅ | `5c2d94d` |
| 4 | Refresh the vendored schema + PROVENANCE + the pins | ✅ | `0a4313e` (with 1 and 2 — see below) |
| 5 | The legacy shim's own emission | — | **not this branch.** Unchanged and unscheduled, per §11.23. |
| 6 | Item 23's regression test | ✅ | `9fed864` |
| 7 | `status.romPath` | ✅ | `02045fe` |
| + | §11.24 D-03 — `step_over`/`step_out` return `pc` (arrived with the re-vendor; closed on the controller's ruling) | ✅ | `66ee253` |
| + | §11.21 — the MCP `breakpoint_clear` tool row | **BLOCKED** | in another repository; see the deviations section |

*(`0522e40` removes an import the item-23 split orphaned; fmt and clippy are commit gates here.)*

**Item 4 could not be its own commit, and the reason is structural rather than tidiness.** The suite
compiles `handshake.initialize.result` **closed** (`tests/common/schema.rs`), so the old vendored schema
refuses the two new keys and the new one refuses their absence: *neither* half is green alone. Items 1, 2
and 4 are therefore one commit, and the split the serve list imagined would have shipped a red commit
whichever order it was taken in.

**The contract tip moved twice while this ran.** The serve list was written against empyrean
`4b17949`/`be23578`; by the time the bytes were taken `origin/main` was **`fc7d7a5`**, carrying §11.21
(CR-BP), §11.22, §11.23 (merged as `45969af`) and §11.24 (batch B1). So the refresh adopted four
amendments, not one. **Fragments 58 → 59**, re-derived by parsing both copies: §11.21 added
`emulator/breakpoint_set_enabled` and **nothing else added a row** — §11.22/§11.23/§11.24 changed fourteen
existing fragments and the handshake. `SCHEMATIZED_NOT_ADVERTISED` therefore moves **17 → 18** by exactly
that one name; `UNCOVERED_METHODS` stays pinned empty. The arithmetic and the per-entry attribution are in
`tests/contract/PROVENANCE.md`.

## What `serverBuild.id` folds in

`<40-char commit>+profile=<debug|release>+target=<triple>+features=<sorted,comma-joined>`, under
`source: "vcs"` with `dirty` beside it. The revision is first and whole (§2.1's SHOULD, so a consumer can
resolve it back to a commit); the rest is M1's *"extended by whatever build-time selection changes the
served surface"*, derived from what this crate actually keys off:

- **profile** — `engine.rs`'s `checkpoint_restore` carries a `#[cfg(debug_assertions)] debug_assert!`, so
  two builds of one clean commit answer `emulator/restore` differently. That is the differ-rule's own
  case. `PROFILE` is a proxy for `debug_assertions`, not the flag; a custom profile that flips it away
  from its `PROFILE` default is undistinguished, and `build.rs` says so rather than hiding it.
- **target** — the crate is `#![cfg(unix)]`, so the triple gates the whole surface.
- **features** — this crate's own `CARGO_FEATURE_*`. **Empty today, and correct rather than vestigial**:
  `oracle-aether/src` reads zero `cfg(feature = …)`, and `tests/server_build.rs` re-derives that instead of
  trusting the comment. A dependency's feature turned on by unification is invisible to
  `CARGO_FEATURE_*` — that is only a gap if this crate reads such a cfg without declaring a mirror
  feature, and the test fails, by name, if it ever does.

**No git, no lie.** No usable revision → `source: "declared"`, an id beginning `no-vcs+` that cannot be
mistaken for a hash, no `dirty` (the schema requires it only under `"vcs"`), and a `cargo:warning` so the
fallback is on the build log rather than silent.

**M2's invalidation.** `.git/HEAD`, the ref it resolves to, `packed-refs` and the index are all declared —
every path resolved with `git rev-parse --git-path`, never spelled `.git/…` by hand, because this repo is
routinely built from a linked worktree where `HEAD` is not at `.git/HEAD` and a hand-spelled path would be
a `rerun-if-changed` on a file that does not exist (which Cargo accepts silently). The index covers the
staged half of `dirty` and the declared source paths cover the working-tree half; neither covers the
other. `dirty` is measured over exactly the declared source scope, so flag and invalidation agree by
construction — and a modified doc, which cannot change this binary's behaviour, correctly leaves it
`false`.

## The two deviations: one closed here, one BLOCKED on another repository

Both were first reported as *named and not fixed*. The controller ruled that `main` does not go red and
that both were small enough to close in this parcel. One was; one turned out to live outside this
repository, which is the stated stop condition.

### 1. §11.24 D-03 — CLOSED, `66ee253`

§11.24 gave `emulator/step_over` and `emulator/step_out` `emulator/step`'s result (`pc` required,
`symbol`?, `symbolDisp`?). This server returned `{}` from both, transcribed from the *pre*-amendment
fragment, and §11.24 said outright: *"Both are non-conformant until they emit `pc`."* It now emits it,
through a single `halt_result(pc)` builder the three rows share, reusing the `symbol_at` lookup
`emit_stopped` already uses for the `stopped` event — deliberately one lookup, because D-03 *was* the
drift between the reply and the event.

**Red-first is the record at `0522e40`**, where eight tests failed as
`methods.emulator/step_over.result: $: "pc" is a required property` against
`{"id":8,"jsonrpc":"2.0","result":{"droppedEvents":0,"frame":0,"mclk":196,"running":false}}` — the six in
`step.rs`, plus `handshake.rs`'s dispatch sweep and `methods.rs`'s stamp sweep. All eight are green now.
`step_over_and_step_out_return_the_envelope_and_nothing_else` became
`…_return_the_halt_pc_and_the_envelope`; it asserted the opposite before, and the flip **is** the
amendment. Its PC is cross-checked against `emulator/registers` rather than pinned, so a handler returning
a plausible address it never read would still fail.

**Scope held.** §11.24's other asks — `step.count`'s refusal outside `1…1000000`, `wait_for_break`'s
`timeoutMs` ceiling, `ping.version`, `z80_read.len`, `vgm_status.path` — are untouched and stay with
CONF-11.24.

### 2. §11.21 `breakpoint_clear` — **BLOCKED**: the row is in another repository

The fix is agreed and is not this repo's to make. The tool row lives at

```
/home/volence/sonic_hacks/oracle-old/linux-port/mcp/oracle_mcp.py:626-635
```

which `git rev-parse --show-toplevel` confirms is the separate **`oracle-old`** repository (this parcel's
brief stops at that boundary by name). The row declares `addr`, `symbol` and `all`; since CR-BP the
fragment declares `breakpoint` (string) and `all` (boolean), so `addr` and `symbol` are params no fragment
declares — and since §2.5 the bus refuses an undeclared top-level param **by name**, so it is a call the
client will compose and the server will refuse, at runtime, on someone else's machine.

**What the fix is**, for whoever owns that file: replace `addr`/`symbol` with
`"breakpoint": {"type": "string", …}`, keep `all`, and reword the description, which still says
*"`addr`/`symbol` to remove those matching a specific location"*. The Rust server serves none of the five
breakpoint rows (`capabilities.breakpoints: false`), so nothing here changes with it.

**Registering it instead was tried and the repo correctly refused.** `mcp_tool_sweep`'s D-33 registry
re-derives that every entry is a *respelling* whose camelCase partner the fragment declares; these are
not, and the assertion says so: *"if it is a genuinely undeclared param it is a client bug and does not
belong in this registry"*. The pin therefore stands **red and un-weakened**, and
`every_mcp_tool_property_is_declared_by_its_contract_fragment` is the one remaining failure in the
workspace suite. It skips loudly rather than failing on a machine without the `oracle-old` checkout
(`ORACLE_MCP_PY` overrides the path), so it is red **here** because the sibling is present, not everywhere.

## Verification

`cargo fmt --all -- --check` exit 0. `cargo clippy -p oracle-aether --all-targets -- -D warnings` exit 0
(this crate has no `[features]`, so there is no `--no-default-features` variant of it to run).
`cargo clippy --workspace --all-targets -- -D warnings` exits 101 on **two pre-existing** lints in
`oracle-core` (`chunks_exact_to_as_chunks` at `examples/ab_compare.rs:180` and
`src/synth/audio_sink.rs:415`) — reproduced identically with this branch's changes stashed, and neither
file is touched here.

`cargo test --workspace --no-fail-fast`: **54 legs, 1819 passed, 1 failed, 6 ignored.**

| run | legs | passed | failed | ignored |
|---|---|---|---|---|
| before this branch | 51 | 1811 | 1 (stale vendored schema) | 6 |
| after the serve commits | 54 | 1811 | 9 | 6 |
| after D-03 (`66ee253`) | 54 | **1819** | **1** | 6 |

The arithmetic closes at every step: 8 tests added and green, the freshness check resolved, the 8 D-03
failures closed by `66ee253`, and the **single remaining failure** is
`every_mcp_tool_property_is_declared_by_its_contract_fragment` — the BLOCKED item above, whose fix is a
file in `oracle-old`.

**Currency unmoved, run and checked rather than assumed** — `conformance_roms` 3/3 (42.60s),
`scanline_goldens` 5/5 (42.65s), `golden_frames` 7/7, `export_state_v1` 3/3, `determinism_gate` 2/2,
`singlestep_m68000` 113/113 (367.67s). No hash was regenerated and no golden was touched. `vendor` is a
symlink to the main tree's in this worktree — `vendor_data_present_when_running_in_ci` passes in both
scorecards, so no row silently skipped.

## ⟨RUNTIME⟩ added by this parcel

The four above stand. Item 1 is now sharper and item 3 is now the *negative*:

5. One `initialize` against a freshly built `oracle-aether`, validated against the refreshed vendored
   schema **closed**: confirm `implementation: "oracle-rs"` and a `serverBuild` whose `id` starts with the
   40-character hash of the built revision. No server was started by this parcel.
6. `emulator/status` on the Rust server launched with a **relative** ROM path: `romPath` must now come
   back **absolute**. `tests/rom_path.rs` proves it in-process; the wire has not been checked.
