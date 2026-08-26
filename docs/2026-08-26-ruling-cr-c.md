# Adjudication of CR-C — server identity: which implementation answered, and which build of it (2026-08-26, independent adjudicator)

Applies to `docs/2026-08-22-cr-c-server-identity.md` at oracle `main` `d10a821` (file blob `a8882d0`,
five commits `afe70ac`..`121b3c8`) and the contract it targets — empyrean `contract/protocol.md` and
`contract/schema/bus-protocol.schema.json` — read at three committed revisions: `cc88d38` (the CR's
anchor; blobs `1e832b1` / `9d8cc3c`), `594f09f` (the dispatch brief's tip; `e585ac5` / `9d8cc3c`) and
`fa7f2b6` (the tip after `git fetch` at ruling time; `b1abc7e` / `9d8cc3c`). **The tip moved during
dispatch** (`594f09f` → `fa7f2b6`, one commit, §11.20, a `caveat`-emission rule that does not touch the
handshake). The two servers were read at the revisions the CR names — oracle `082e6ce` (an ancestor of
`d10a821`) and oracle-old `d629771` — and the consumer at aurora `638df0a`, every sibling file through
`git show`, never through a working-tree path. No emulator MCP tool was used; no `cargo` was run; no
server was started; nothing was committed to any `main`. Everything executed was `git`, `grep`, `sed`,
`strings` and three-line Python I wrote for this ruling. The ruling model is **Claude Fable 5**, and it
was **un-framed**: it received the CR, the contract, the precedent ruling's *form*, and no steer on the
outcome.

## Ruling: **ADOPT WITH CHANGES**

The evidence base is sound to the line. Of the CR's anchors I could reach, every line-number citation
into `protocol.md`, the schema, `engine.rs`, `main.rs`, `server.rs`, `Cargo.toml`, `ControlSocket.cpp`,
`unserved.ts`, `client.ts`, the audit and `empyrean/CLAUDE.md` resolves to the text the CR says is
there (the one exception is a paraphrase presented as a quotation — S1). The structural facts reproduce
by parse: 59 keys under `methods`, one of them `$comment`, 58 fragments; `initialize.result.required`
is exactly `[serverName, protocolVersion, capabilities, methods, timingBasis, limits]` with
`serverVersion` declared but not required; 41 `emulator/*` names in `engine.rs` at `082e6ce` **and still
41 at `d10a821`**; the served-not-in-schema set is empty. The two commits that make the 37-vs-41
story datable exist with the subjects and timestamps claimed (`a05e34c` 17:08:26, `6fc3bd5` 20:15:20,
both 2026-08-22 −0400), `cc88d38` was authored 20:38:48 the same evening with the title the CR quotes,
and `f172e61..cc88d38` really is empty for both contract files. The legacy server really emits five
keys and no `timingBasis` anywhere in the repo; its `OpStatus` really emits no `romPath`; its
`AdvertisedMethods()` really walks the same `Handlers()` map `RunMethod` looks up. Aurora really
computes `methodCount` locally at `client.ts:149`, keeps the handshake after teardown, and carries
fixtures at `0.1.0` and `0.4.1`.

The three properties are right and I adopt all three. The mechanisms are mostly right. But the CR
contains one internal contradiction that is not cosmetic — **its normative differ-rule (§5.4) is
violated by its own recommended `source: "vcs"` encoding in exactly the case its §9.4.2 declares live in
this tree** — and one claim at the heart of P3 (*"a process that has it compiled in cannot be wrong
about it"*) that is false for the mechanism the CR proposes the reference server use. Add a stale,
self-referential review-scope note, a missing amendment-log entry, and contract drift the CR predates
(§11.19 changed the socket arrangement its §2.3 rests on), and this is adopt-with-changes.

---

## Every checkable claim, checked

Legend: **HELD** = reproduced firsthand at the named revision; **FAILED** = the tree or the arithmetic
says otherwise; **UNVERIFIABLE** = the exhibit no longer exists or would need a running process
(⟨RUNTIME⟩ where a foreground handshake would settle it).

### Contract (empyrean `cc88d38`; re-checked at `fa7f2b6` where noted)

| # | Claim | Result |
|---|---|---|
| 1 | Blobs `1e832b1` (protocol.md) and `9d8cc3c` (schema) at `cc88d38` | HELD |
| 2 | `f172e61..cc88d38` has no commits on either contract file; protocol blob identical | HELD (`git log` empty; `f172e61:contract/protocol.md` = `1e832b1`) |
| 3 | Schema `methods` has 59 keys, one `$comment`, 58 fragments | HELD by parse, both revisions |
| 4 | `initialize.result.required` = `[serverName, protocolVersion, capabilities, methods, timingBasis, limits]`; `serverVersion` in `properties` but not required | HELD by parse; unchanged at `fa7f2b6` |
| 5 | Schema `methods.description`: *"This array IS the count of the server's surface; no number in prose tracks it."* | HELD verbatim |
| 6 | `capabilities.checkpoints.description` quoted as *"an object rather than a bare flag, because the cap must be discoverable before a client plans around it"* | **FAILED (minor)** — the text is *"An object, not a bare flag, because the cap must be discoverable before a client plans around it."* Substance identical; the CR presents a paraphrase as a quotation (S1) |
| 7 | `buildId` appears in the schema; `methodCount`/`implementation`/`serverBuild` do not | HELD (1 / 0 / 0 / 0) |
| 8 | D3 at `:66-70`, *"you can rename any tool … forever without touching the wire"* | HELD (elision is honest: the omitted text is the parenthetical *"(Oracle (the emulator; repo `oracle`) → MegaScope/Vantage, etc.)"*) |
| 9 | D4 at `:72-77`, *"the exact set of supported methods"*, `list_ops` 34-of-47 story | HELD (`:73-74`; D4 spells it *"47-vs-34"*, the §6 blockquote *"34 of 47"*) |
| 10 | D5: clients never branch on a version integer | HELD (`:78-82`) |
| 11 | D16 at `:249`; `timingBasis` REQUIRED, top-level, *"it is what that server's stamps mean"* at `:360-362` | HELD |
| 12 | §2.1 example at `:338-350`, `serverVersion` at `:339`, stamp/`droppedEvents` sentence ends at `:358` | HELD; **unchanged at `fa7f2b6`** (same line numbers) |
| 13 | `romReloaded.buildId?` at `:639`; `load_symbols` forward note at `:779` | HELD (`:642` / `:782` at tip) |
| 14 | §6 no-count blockquote at `:830-839` | HELD (`:833` at tip) |
| 15 | *"Absolute paths SHOULD be reported"* at `:1799` with the *"cannot tell which build"* clause | HELD — the sentence spans `:1799-1800` (`:1802-1803` at tip) |
| 16 | §8 item 17 at `:1966`; item 22 is the last item and ends at `:2007` | HELD — **at the tip item 22 spans `:2036-2045`; "after line 2007" must be re-anchored (M5)** |
| 17 | *"Oracle now advertises 53 ops"* deleted, recorded at `:2260` | HELD |
| 18 | CR-13 registered `methodSummaries` and `checkpoint_list.total/returned/limit` at `:2443` | HELD — both in the same table row |
| 19 | `emulator/ping` row exists with `version` (audit says §6 line 847) | HELD (`:847`; `:850` at tip) |
| 20 | §8 bans unilateral invention by the emulator side (§11.1's objection) | HELD (`:2100-2102`; `:2073`) |
| 21 | Audit D-01 readings (a)/(b)/(c), *"all three are shippable today and a client cannot tell them apart"*, recommendation *"MUST NOT branch on it"* | HELD (`audit.md:83-91` at `cc88d38`) |
| 22 | Audit `:532` counts `buildId` among camelCase keys | HELD |
| 23 | Audit: *"'Which implementation has this been built against?' has two different answers"* | HELD (`audit.md:30`) |
| 24 | `empyrean/CLAUDE.md:37` — MCP tools bind to `oracle-old/linux-port/mcp/oracle-mcp`, *"verified at the registration 2026-08-22"*; row *"used to say `oracle/`"* | HELD |
| 25 | `empyrean/CLAUDE.md:38` — *"Serves 40 of the 58 … 18 pinned-unserved … (58 == 40 + 18)"*; *"Oracle (was `oracle-next`)"* | HELD |
| 26 | `cc88d38` title *"the keystone was false in this file for weeks, and the two emulator rows were inverted by the rename"*, authored 20:38:48 | HELD |
| 27 | Contract drift since `cc88d38` | **Moved**: protocol.md `1e832b1` → `e585ac5` (`594f09f`) → `b1abc7e` (`fa7f2b6`), +153 lines; schema byte-identical throughout. Hunks: §2.4 rule 1 narrowed (§11.20), **§7.1 rewritten** (§11.19, own-instance vs attached), §7.3 count un-pinned, two rename notes, §11.19, §11.20. Nothing touches §2.1, D4, §8's items or the `initialize` fragment. §11.19 **does** touch the CR's §2.3 premise — see M5 |

### Rust server (oracle `082e6ce`; drift to `d10a821` checked)

| # | Claim | Result |
|---|---|---|
| 28 | `082e6ce` exists, is an ancestor of `d10a821` | HELD |
| 29 | Vendored schema at `082e6ce` = upstream blob `9d8cc3c` | HELD (and still `9d8cc3c` at `d10a821`) |
| 30 | `engine.rs:8-10` — METHODS holds the pointers, dispatch is a lookup, *"no second list"* | HELD |
| 31 | `EngineConfig.server_name/server_version` at `:150-151`; defaults `"oracle-next"` / `env!("CARGO_PKG_VERSION")` at `:190-191`; shipped at `:1344-1345` | HELD (`:1354-1355` at `d10a821`; the two intervening hunks are summary-string edits, not identity) |
| 32 | `Cargo.toml:3` `version = "0.0.0"` | HELD |
| 33 | `dispatch` = `METHODS.iter().find(…)` at `:1291` | HELD (`:1291`) |
| 34 | `initialize_result` at `:1320-1415` emits eight top-level keys | HELD — parsed the `json!` literal: `serverName serverVersion protocolVersion capabilities methods methodSummaries limits timingBasis` |
| 35 | `set_rom_path` at `:1006`; `romPath` shipped unchanged at `:1756`; `main.rs:38` takes the bare argv; `:92` puts it on the machine; `server.rs:402` hands it over | HELD, all five |
| 36 | `grep -c 'name: "emulator/'` → 41 | HELD (41 at `082e6ce`, 41 at `d10a821`) |
| 37 | 58 fragments split 41 served + 17 unserved (the 17 named); served-not-in-schema empty | HELD by set difference (my 17 = the CR's 17) |
| 38 | `a05e34c` *"serve emulator/step, step_over and step_out"* 2026-08-22 17:08; `6fc3bd5` *"serve emulator/run_to_scanline"* 20:15 | HELD (17:08:26 / 20:15:20 −0400) |
| 39 | `oracle-core/Cargo.toml:8-12` `synth = []` default OFF; `lib.rs:32` `#[cfg(feature = "synth")]`; `oracle-aether` depends on `oracle-core` without features | HELD, all three |
| 40 | `require_paused` rows dispatch and refuse `-32005` on machine state (§9.4.2's precedent) | HELD (`engine.rs:1727`, 24 uses; `rpc.rs:38` `INVALID_STATE = -32005`) |
| 41 | Nothing in this tree already implements `serverBuild`/`build.rs` | HELD (0 hits; no `build.rs` under `crates/`) |
| 42 | Release binary built 2026-08-21 22:11:04, 1,925,824 bytes, carries 37 of 41 names | **UNVERIFIABLE** — the file at that path is now 2,036,784 bytes, mtime **2026-08-25 21:03:18**. It has been rebuilt; the exhibit is gone. By the CR's own method on today's binary: **41 of 41 present** (consistent with a post-`6fc3bd5` build; substring test is one-directional, as the CR says). ⟨RUNTIME⟩ |
| 43 | Both builds would say `serverName: "oracle-next"`, `serverVersion: "0.0.0"` | HELD from source (32, 31); wire values ⟨RUNTIME⟩ |
| 44 | §11.9: consumer reviewed blob `9a64d1e` = `e93571b:docs/…cr-c…`; current text is `d364edd` (+43/−1) | **FAILED in part** — `9a64d1e`/`e93571b` HELD. But `d364edd` is `f67e8eb`'s blob; §11.9 itself landed in `a789008` (blob `4aa07de`) and `121b3c8` then added §9.4.2 (blob `a8882d0`, the file at `d10a821`). The reviewed blob differs from the current one by **+94/−1** across §9.4.1, §9.4.2, §11.9 and the §12.1-item-5 verdict paragraph — not +43/−1 (M3) |

### Legacy server (oracle-old `d629771`)

| # | Claim | Result |
|---|---|---|
| 45 | `kServerName = "oracle"` `:2693`, `kServerVersion = "2.1-linux"` `:2694` | HELD |
| 46 | `initialize` result = five keys at `:2876-2882`, the block quoted verbatim | HELD |
| 47 | `AdvertisedMethods()` `:2715-2722` iterates `Handlers()`; `RunMethod` `:2790-2825` looks up the same map at `:2799-2800` | HELD |
| 48 | `OpStatus` `:410-435` emits `ok, running, rom_loading?, pc, sp, sr, symbol_at_pc?, symbol_disp?, frame_token, symbol_count` and no `romPath`/`romBytes` | HELD (snake_case; nothing else) |
| 49 | `git grep timingBasis d629771` → nothing; control `serverName` in the same file → one hit | HELD (0 / 1) |
| 50 | `git grep -i rompath d629771` → twenty hits, none in `ControlSocket.cpp` | HELD (20 hits across 8 files; 0 in `ControlSocket.cpp`) |
| 51 | A `static const char*`/preprocessor define is the natural home (§10) — i.e. nothing embeds a VCS id today | HELD (the only `GIT_*` in `linux-port/CMakeLists.txt` are FetchContent pins) |
| 52 | The running legacy process is the build at `d629771` | **UNVERIFIABLE** ⟨RUNTIME⟩ (the CR says so itself) |

### Consumer (aurora `638df0a`) and the sweep the CR did not do

| # | Claim | Result |
|---|---|---|
| 53 | aurora has no `origin/main`; `master` is default | HELD (`origin/HEAD -> origin/master`) |
| 54 | `unserved.ts:4-10` same-socket-chain block; `:27-29`; `:31-33`; `:68-71` `serverName` in the error message; `:100-103` *"the warp hung"* | HELD, all five, verbatim |
| 55 | `client.ts:55-58`, `:60-67`, `:78` raw `capabilities`, `:105-110`, `:145`/`:157`, `:149` `methodCount: methods.length`, `:153-155`, `:160` logs `methods.length`, `:314` `servedMethodCount` | HELD, all |
| 56 | Fixtures `serverVersion: '0.1.0'` (`client.test.ts:31`) and `'0.4.1'` (`unserved.test.ts:42`) | HELD |
| 57 | §11.7: seraph, sigil, the MCP shim, the reference clients not swept | **Swept here**: empyrean `clients/typescript` reads `serverName`/`serverVersion` for a log line and types both as **required strings** (`client.ts:15-16`); the Python client has no hit; seraph `src-tauri/src/lib.rs` matches only `initialize`; sigil none; the oracle-old shim at `90f40b8` never reads either; aurora `origin/master` has **no** `serverName ===`-style branch. No consumer branches on either field today — C3's MUST NOT breaks nobody |
| 58 | §2.3: the MCP shim registered from `oracle-old` can be driving the Rust core | HELD and **now by design**: at `07314aa` (the landing §11.19 cites) `oracle_mcp.py:102` spawns `oracle/target/release/oracle-aether` by default (M5) |
| 59 | The reference suite pins identity by display name | Not a CR claim, but found: `crates/oracle-aether/tests/handshake.rs:33` asserts `serverName == "oracle-next"` — the exact pattern §3.2 warns about, in the reference server's own suite (note for the implementer) |

**Tally: 59 rows, of which 57 are CR claims (27 is a drift report and 59 a finding of mine): 53 HELD;
2 FAILED (6, a paraphrase presented as a quotation; 44, the stale scope note); 2 UNVERIFIABLE (42, the
rebuilt binary; 52, the running legacy build).** The two reasoning defects (M1, M2) are argued in the
changes, not tallied as anchors.

---

## §12.1 — the eight questions, answered

**1. Are the three properties adopted? YES, all three**, with two precisions folded into P3's text
(M1, M2). P1 is a measurement, not an argument, and it survives the loss of its exhibit (row 42):
the commit dates alone prove two builds of one implementation differed by four methods while
`serverVersion` was the constant `0.0.0` (rows 32, 38). P2 is correctly scoped — it moves forging from
a supported configuration to a violation and claims nothing more. P3 is right in its aim and wrong in one
sentence (M2).

**2. Registry, not a free string.** §11.2's friction argument is real but priced at ten implementers;
the bus has two, and *extended only by amendment* is the same rule the contract already applies to
`capabilities.events`, `entryKind` and every enum on the wire. A free string is `serverName` with a
comment, which the CR says itself.

**3. The object `{id, source, dirty?}`.** `dirty` is the only thing that makes a VCS id honest, and this
suite builds from dirty trees as a matter of course — the 37-vs-41 binary was a developer build. A bare
string closes the incident and reopens it the first time a dirty build is resolved to a clean commit.
But the object as drafted is under-specified in one case the CR itself names (M1).

**4. `serverVersion`: keep, defuse, REQUIRED.** The reference TypeScript client at empyrean
`origin/main` already types `serverVersion: string` non-optional (row 57); making the schema say what
the reference client already assumes is the schema catching up, not a new burden. Striking it would be a
wire change to a key both servers and three clients surface in a log line — the one job it is fit for.

**5. Item 23: MUST, with §9.4.1's dispatch-not-succeed clause and §9.4.2's per-build reading.** The
consumer's argument is decisive and I verified its premise: both servers derive `methods` from the
dispatch map (rows 30, 33, 47), so MUST costs nothing today, and SHOULD would make the pre-check
permanently unsound. §9.4.1 is load-bearing, as claimed — `require_paused` (row 40) is live precedent
for advertise-and-refuse being conformant.

**6. Enum in the schema AND registry in the prose.** A registry that the published schema does not
enforce is a stale-artifact hazard by construction: item 20's closure would pass a value the prose
forbids. The cost — a schema release per new implementation — is already paid by the amendment the
registry requires. Mirror, and say in the schema description that the prose registry governs.

**7. D-01 in the same sitting: YES.** This ruling cannot pin `ping.version` — D-01 belongs to the audit
— but it records the position: take the audit's own recommendation (constant `2`, MUST NOT branch),
and note in D-01's closing text that reading (c) is closed by `serverBuild`. Ruling one without the
other re-opens it, as §9.5 says.

**8. Vocabulary: `oracle-rs` / `oracle-cpp`, as drafted.** D3 governs *method names*; a field whose
whole purpose is to identify the implementation must name it. The language suffix is the one fact that
did not move in the 2026-08 rename and will not move in the next; `-successor` is a relative term with
an expiry, and `-exodus` names the legacy server's ancestry rather than the thing on the socket.

## §12.2 — the nine settlings, each given a position

1. Two facts, failing independently — **agreed**; proven from commit dates and the constant version.
2. Neither existing field answers either — **agreed** (rows 31, 32, 45).
3. Which server answers is decided by launch order — **objected in part.** True at `cc88d38`; at
   `fa7f2b6`, §11.19 makes *own instance on a private path* the default and *attached* the opt-in. Under
   own-instance the client knows which **binary path** it spawned — but not which build (the shim spawns
   whatever sits at `oracle/target/release/oracle-aether`, row 58, which is exactly the stale-binary
   hazard of §2.6). So the drift narrows the launch-order sentence to the attached arrangement and
   **strengthens** C2. The CR must say so (M5).
4. `buildId` unavailable — **agreed** (rows 13, 22).
5. Top-level, not in `capabilities` — **agreed**, on D16's reasoning (row 11) and aurora's record-vs-lie
   distinction (row 55).
6. `methodCount` not added — **agreed**, including the §8.3 reversal condition.
7. No client behaviour mandated — **agreed**.
8. 58 fragments plus `$comment` — **verified** (row 3).
9. `status.romPath` violates the SHOULD — **agreed with a precision**: the echo is verbatim (row 35), so
   the violation is *conditional on a relative launch argument*, not unconditional; the CR's §2.5
   already says this and §12.2 should not round it up.

---

## MUST (blocks adoption; each names its defect and its fix)

**M1 — §5.4's differ-rule is violated by the CR's own `"vcs"` encoding in the case §9.4.2 declares
live.** §5.4 (and §9.2, §9.6): *"`id` MUST differ between any two builds whose observable behaviour on
this bus can differ."* §5.1: `"vcs"` = *"a revision identifier from version control (a git commit
hash)"*. §9.4.2, verified (row 39): `synth` is a compile-time feature, default OFF, and the audio rows
are served only when it is on. Two binaries built from **one clean commit**, one with `--features synth`
and one without, differ in served methods and carry the **same** `serverBuild` under the drafted rule —
`{id: <hash>, source: "vcs", dirty: false}`. The CR's normative floor is unmet by its recommended
recipe, and `dirty` does not cover it (the tree is clean). Fix — three textual deltas:

- §9.2, the `serverBuild` sentence, after *"`id` MUST differ between any two builds whose observable
  behaviour on this bus can differ."* add: *"Under `"vcs"` the id is the revision identifier **extended
  by whatever build-time selection changes the served surface** — a feature set, a build profile, a
  target — when the implementation has one; a commit hash alone does not satisfy this rule for a source
  tree with compile-time-optional surfaces, and §9.4.2 is the live case. `dirty` covers uncommitted
  source; it does not cover configuration."*
- §5.1's `source` table, `"vcs"` row: *"a revision identifier from version control"* → *"**derived
  from** a version-control revision identifier, plus any build configuration §5.4 requires."*
- §9.6 schema, `serverBuild.properties.source.description`: append *"'vcs' ids are derived from, not
  equal to, a revision: build configuration that changes the served surface is part of the id
  (protocol.md §2.1)."*

**M2 — P3's central sentence is false for the mechanism P3 recommends.** *"A process that has it
compiled in cannot be wrong about it, because it never chose the value"* (§3.3, and §9.2's ⚑ paragraph).
A Cargo build script embeds the hash at compile time — and its output is **cached** unless it declares
`cargo:rerun-if-changed` on `.git/HEAD` and the ref it points at. A binary rebuilt after a commit, whose
build script did not re-run, carries a compile-time constant naming the **previous** commit: structurally
emitted, never read at start-up, and wrong. That is a self-report by another route, and the ⚑ clause as
drafted would call the fix a refactor. Fix — two deltas to §9.2's ⚑ paragraph:

- After *"…because it never chose the value."* add: *"**The constant must be invalidated by what it
  names.** A build MUST recompute `id` whenever the revision, the dirty state, or the configuration §5.4
  names changes; a cached build-script product that survives such a change is a self-report by another
  route and is non-conformant. In Cargo terms: a `build.rs` that embeds the hash MUST declare
  `rerun-if-changed` on `.git/HEAD` and on the ref it resolves to."*
- Replace *"cannot be wrong about it, because it never chose the value"* with *"has no opinion to be
  wrong about, provided the value is recomputed whenever its inputs change"*. The line P3 draws is
  compile-time versus run-time, not file versus constant: a generated file `include!`d at compile time is
  on the right side of it, and the text should say so in the parenthetical rather than leave *"including a
  generated file"* to be read as banning `OUT_DIR`.

**M3 — §11.9's review-scope note is stale and self-referential.** It names `d364edd` as *"the current
text"* and *"+43/−1 — §9.4.1 and this section"*. `d364edd` is `f67e8eb`'s blob, which does not contain
§11.9; the commit that added §11.9 (`a789008`) produced blob `4aa07de`, and `121b3c8` then added §9.4.2
(blob `a8882d0`, the file at `d10a821`). A section cannot name its own blob, and this one was stale on
the commit that introduced it. The reviewed blob `9a64d1e` differs from the current file by **+94/−1**
across §9.4.1, §9.4.2, §11.9 and the §12.1-item-5 verdict paragraph. Fix: state the delta **by section**
and cite the reviewed *commit* (`e93571b`) and the current *commit* rather than the current blob; drop
the +43/−1 arithmetic. The note's own principle — *"a review has a subject, and the subject moves"* —
is right, which is why the note must not be the thing that moved.

**M4 — No amendment-log entry is proposed.** Every adopted CR in this contract lands as a §11.N entry
(§11.18 for CR-28, §11.19 for CR-SOCKET, §11.20 for D-30); the CR's §9 gives §2.1, D4, §8 and schema
deltas and no §11 text. At `fa7f2b6` the next entry is **§11.21**. Fix: add to §9 a §11.21 entry that
records (a) the incident and its correction (§2.5's `romPath` finding), (b) the three properties as
adopted here, (c) the registry's two initial values, (d) the M1/M2 precisions, (e) that the schema
change adds three `required` keys to a fragment whose closure `initialize` is exempt from — so the
change is additive for every client that does not ask, and a conformance item for both servers — and
(f) that the reference server does not yet emit either key, on the `read_cram` precedent (fragment before
handler, §8 item 20).

**M5 — Re-anchor to the tip and absorb §11.19.** The CR's §9.4 says *"after line 2007"*; at `fa7f2b6`
item 22 ends at `:2045` (row 16). §9.1/§9.2/§9.3's anchors are unchanged. More than line numbers: §2.3's
*"both resolve the same socket chain … decided by whoever launched first"* is the attached arrangement
only, now that §7.1 defines two (row 27); and the sentence *"a tool whose registration says `oracle-old`
can be driving the Rust core"* is no longer a hazard but the shipped design at `07314aa` (row 58). Fix:
§2.1's table gains a row for the tip revision read; §2.3 gains one paragraph stating that under §11.19's
own-instance arrangement the client knows the binary path and still not the build — which is §2.6's
stale-binary hazard moved from the socket to the filesystem — and §12.2 item 3 is narrowed to match.

## SHOULD (improves the record; does not block)

**S1 — Quote `capabilities.checkpoints.description` verbatim** (row 6) or mark it a paraphrase.

**S2 — Under `"vcs"`, `id` SHOULD be the full revision identifier.** §9.1's example shows `"d629771…"`
and §5.1's `"6fc3bd5a…"` — abbreviated. The CR's own §5.3 has a consumer *resolving* the hash, so it is
not opaque in practice; abbreviations collide over a repo's life. One sentence in §9.2.

**S3 — Update the gate's §2.1 vector in the same commit.** empyrean `contract/schema/tests/vectors.json`
carries a `specExamples` entry *"§2.1:337 initialize result"* pointed at `handshake/initialize/result`
with `expect: "knownDefect"` (D-25, the missing envelope). Its `doc` must gain `implementation` and
`serverBuild` when the §2.1 example does, or the example and its vector diverge; it stays `knownDefect`
for D-25's reason.

**S4 — Mark the binary exhibit perishable.** §1, §2.6 and §13 rest one figure on a file that has since
been rebuilt (row 42). Say the exhibit is a snapshot and that the argument stands on the commit dates
and the constant version, which it does.

**S5 — Name the reference suite's own `serverName` pin.** `tests/handshake.rs:33` asserts
`serverName == "oracle-next"` (row 59) — a test that will silently pin a config-settable display label as
identity. When C1 lands, that assertion moves to `implementation == "oracle-rs"`. Not a CR defect; a note
the implementer will otherwise miss.

---

## The centerpiece, stressed

**Additivity.** `initialize.result` carries no `additionalProperties`/`unevaluatedProperties`
(row 4), and item 22 exempts `initialize` from closure by name (`:2036-2045` at tip). Adding two keys and
promoting one to `required` therefore breaks no client and no vendored validator in the never-asking
direction; in the asking direction it makes both servers non-conformant until they emit — which is the
`timingBasis`/CR-7 path, raised before shipping this time, and the correct order under §8 item 20.

**The registry's `enum` and the CR's conditional `dirty`.** The drafted `allOf`/`if`/`then` is valid
Draft 2020-12 and enforces `dirty` exactly when `source == "vcs"`; nothing forbids `dirty` under
`"content"`/`"declared"`, which is fine — a meaningless optional key is not a defect. After M1 the
schema needs no structural change: the configuration rides inside the opaque `id`.

**C4 against both trees.** Rows 30/33 and 47 are the structural proof for both implementations; the
per-build reading in §9.4.2 is the only reading under which a default `oracle-aether` build (no `synth`)
is conformant, and M1 is what lets `serverBuild` actually separate the two cases §9.4.2 says have one
wire signature — without M1 it cannot, because two builds of one commit share an id.

**C5.** The §6 blockquote (row 14) and the schema's `methods.description` (row 5) are as quoted; the
four drift instances are each verified (rows 9, 17, 25/26, and the schema description's own text about
its deleted count). The §8.3 reversal condition is correctly stated against §2.4's policy-bounded rule
(`:456-579` at tip).

---

## ⟨RUNTIME⟩ — for the controller's foreground follow-up

1. One `initialize` against a freshly built `oracle-aether` at `d10a821`: confirm 41 names on the wire
   and `serverName`/`serverVersion` = `"oracle-next"`/`"0.0.0"` (rows 36, 43).
2. One `initialize` against whatever legacy process is running: confirm five keys and no `timingBasis`
   (rows 46, 49), and thereby whether the running build is `d629771`'s (row 52).
3. `emulator/status` on the Rust server launched with a relative ROM path: confirm `romPath` echoes
   relative (row 35 proves the code path; the wire fact is the SHOULD violation).
4. Whether today's binary (2026-08-25 21:03, 2,036,784 bytes) serves all 41 — the `strings` test says
   present, which is only consistent, not proof (row 42).

Nothing in this ruling depends on any of the four.

## BLOCKED

Nothing was blocked. One exhibit was **unreproducible** rather than blocked — the 2026-08-21 binary
(row 42) — and is recorded as such; the claim it supported is carried by the commit dates instead.

---

## Provenance

| | |
|---|---|
| Ruling written in | oracle worktree `/home/volence/sonic_hacks/oracle/.claude/worktrees/agent-a78130e322ff71869`, branch `ruling-cr-c`, cut from `main` `d10a821` |
| CR read at | `d10a821:docs/2026-08-22-cr-c-server-identity.md` = blob `a8882d0`; history `afe70ac`, `fb2db06`, `f67e8eb` (`d364edd`), `a789008` (`4aa07de`), `121b3c8` (`a8882d0`); reviewed blob `9a64d1e` = `e93571b` |
| Contract read at | empyrean `cc88d38` (`1e832b1`/`9d8cc3c`), `594f09f` (`e585ac5`/`9d8cc3c`), **`fa7f2b6`** (`b1abc7e`/`9d8cc3c`) — `origin/main` after `git fetch` at ruling time; the tip moved from `594f09f` during dispatch |
| Also read at `cc88d38` | `docs/2026-08-22-protocol-schema-audit.md`, `CLAUDE.md`; at `origin/main`: `contract/schema/tests/validate_contract_schema.py`, `vectors.json`, `clients/` (grep only) |
| Rust server read at | oracle `082e6ce` (`engine.rs`, `main.rs`, `server.rs`, `Cargo.toml`, `oracle-core/Cargo.toml`, `oracle-core/src/lib.rs`) and `d10a821` (`engine.rs` diff, `rpc.rs`, `tests/handshake.rs`) |
| Legacy server read at | oracle-old `d629771` (`linux-port/gui/ControlSocket.cpp` whole; repo-wide greps); `07314aa`/`90f40b8` (`linux-port/mcp/oracle_mcp.py`, grep only) |
| Consumer read at | aurora `638df0a` (`unserved.ts`, `client.ts` whole; two test files by line); `origin/master` (grep only); seraph `origin/main`, sigil `origin/main` (grep only) |
| Binary inspected | `/home/volence/sonic_hacks/oracle/target/release/oracle-aether`, mtime 2026-08-25 21:03:18 −0400, 2,036,784 bytes — **not** the CR's exhibit |
| Runtime | none — no `cargo`, no emulator, no `mcp__oracle__*` tool, no server started |
| Not checked | the four ⟨RUNTIME⟩ items above; the content of CR-A/CR-B (only the git fact that they and CR-C read one blob); the cross-lane consumer review as an event (only its recorded blob); whether the empyrean gate passes with the §9.6 deltas applied (would need running the gate — docs-only ruling, no gate added, per the brief) |
| Scripts | three Python snippets (schema parse; vector inspection; binary-strings membership) and shell `sed`/`grep`/`git` — full outputs read, none tailed |
