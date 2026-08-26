# CR-D — the object decoders: a closed envelope over an engine-shaped payload

**Raised by:** the oracle lane (the ground-up Rust core + Aether server, `oracle/`).
**Against:** `empyrean` `contract/protocol.md` §6 (*object / player decoders ⚙*, lines 1492–1503), §9's
Phase-5 deferral (line 2277), §8 item 20 (line 2142), and
`contract/schema/bus-protocol.schema.json` — read at `origin/main` **`39cfaa27c293510d583581b5b07d07709691508a`**
(2026-08-26 06:12:23 −0400), blobs `b4776ce90000a89ee50755892f999c03e5130e99` (protocol.md) and
`7b24bcedc24f0a6aa7dd4504f4e2f9bf63e4cda7` (schema).
**Closes:** audit defect **D-27** (`docs/2026-08-22-protocol-schema-audit.md` §3, at the same revision) —
two of the three rows it names. `object_slot`, the third, is offered severably in §8.
**Raised from:** a consumer demand filed by aeon on 2026-08-26 and recorded in this repo at
`docs/2026-08-26-obj-decode-demand.md` (commit `0d7c5c21c2e92458d55de5d4a062e08d2532d610`).
**Date:** 2026-08-26.

---

## 0. How to read this document

This CR proposes changes to a **contract**, not to a server. It is written to be adjudicated by a reader
with no prior exposure to this repo, so every claim below is either quoted from a cited source at a named
revision or marked as a judgement.

§1 is the summary. §2 is the evidence base — what was read, at what revision, what was *not* checked, the
SST layout re-derived from the consumer's own source, and the consumer sweep with its enumeration
parameters named. §3 states the **central tension** this CR exists to resolve and why routing around it
was refused. §4 states the three **properties** the proposal serves. §5–§8 are the mechanisms: D1 the
layout descriptor, D2 `object_list`, D3 `player_state`, D4 the capability flag, D5 `object_slot`
(severable). §9 is the visible better-approach pass — every place this CR departs from what the consumer
asked for, with the cost to them stated. §10 gives the exact textual and schema deltas. §11 states what
this CR does not bind. §12 names where it is weakest, including a peer consumer's recorded argument
*against* the whole idea. §13 separates the questions handed over undecided from what this CR considers
settled, so an adjudicator can object to the settling as well as to the openings. §14 is provenance.

**Two implementers, and this is a CR where that matters.** The Aether bus has two servers: the Rust
`oracle-aether` in this repo, and the legacy C++ `ControlSocket.cpp` in `oracle-old/`. The legacy server
**serves both methods today**; the successor serves neither. Every behavioural statement below is
attributed to one of them by name, at a named revision.

**The consumer's reference policy, honoured throughout.** The filed demand states: *"No captured legacy
sample exists on aeon's side. The Rust output is the first and only reference; do NOT A/B against the
legacy bridge's shape. This is the consumer's explicit instruction, not a shortcut."*
(`docs/2026-08-26-obj-decode-demand.md`.) Accordingly the legacy implementation appears below **only as
evidence about what a real decoder found it needed** — never as a shape to preserve. Four places where
this CR deliberately departs from it are named in §9, and one place where the legacy is simply better than
the demand is named there too.

**A note on cost and on runtime.** No emulator was driven, no `cargo` command was run, and no
`mcp__oracle__*` tool was touched. Nothing here is a runtime observation. Items wanting runtime
confirmation are tagged **⟨RUNTIME⟩** and collected in §12.7.

---

## 1. Summary

Two catalogued methods — `emulator/object_list` and `emulator/player_state` — have **no schema fragment**,
and are therefore unimplementable by a conformant successor: §8 item 20 makes the fragment the
precondition for the handler, not its record. They are two of eight §6 rows in that position, and their
absence is **deliberate**. The schema's own `$comment` (blob `7b24bce…`, line 5) records why:

> EIGHT §6 ROWS REMAIN UNSCHEMATIZED AND DELIBERATELY SO … because each states its result too loosely to
> transcribe without inventing … A PARTIAL fragment would be worse than none: item 20's closure would then
> refuse the conformant server that emits the keys the fragment omitted, while §2.5 already reads an
> ABSENT fragment correctly as 'not yet transcribed'.

So a CR proposing fragments for two of the eight has to answer that argument, not step around it. **§3 is
that answer**, and it is the spine of this document. The short form: item 20's closure is a rule about a
*key set*, and the reason these two rows cannot be transcribed is that part of their key set is **not
knowable to the contract** — it is a function of which game is loaded. Those are separable. A fragment can
close the part the contract owns and **explicitly declare the other part open**, and the openness is then
a *published property of the shape* rather than a gap the harness happens not to reach.

That mechanism is not this CR's invention. It is what the audit that BLOCKED these rows proposed as their
unblock condition, in its own words (`docs/2026-08-22-protocol-schema-audit.md` §3, D-27):

> **To unblock:** either Phase 5 (a config/symbol-driven decode whose *envelope* is fixed even though its
> fields are not — e.g. a declared `fields` map plus a `layout` discriminant), or, much cheaper and
> available now, the flag §6 already asks for: surface `capabilities.objectDecoders` and the engine-detect
> result so a client can branch instead of assuming.

This CR takes **both** halves of that sentence, and argues they are one design rather than two options.

| # | Change | Serves |
|---|---|---|
| **D1** | A `layout` descriptor, REQUIRED on every decoder reply: what was detected, how, and the pool table it decoded against | P1, P2 |
| **D2** | `emulator/object_list` — flat bounded list of active slots; closed core per item (`slot`, `addr`, `x`, `y`, `code`), optional resolved `name`/`nameDisp`, and a **typed-open `fields` map** for engine-declared decodes | P1, P3 |
| **D3** | `emulator/player_state` — the same item shape under a `players[]` **array**, killing the legacy's engine-dependent top-level key set; **no invented bit names** | P1, P2 |
| **D4** | `capabilities.objectDecoders` keeps its published boolean type and gains a pinned meaning: *this build has the handlers*, never *a layout was detected* | P2 |
| **D5** | `emulator/object_slot` — schematized as the single-slot projection of D2. **Severable; no consumer asked for it.** | — |

**What this costs consumers.** aeon asked for this and gets everything it asked for except two keys it
does not need to be handed (§9). aurora is the only in-tree bus client, and it has **recorded an argument
against this whole surface** (§2.6, §12.1) — that argument is reproduced in full and answered rather than
omitted. The legacy server is asked for nothing: §11 states that explicitly.

**The pre-release window, and what this CR spends it on.** Neither method ships on the successor, and the
legacy is frozen (§11.23's treatment). So every key here can be REQUIRED at zero migration cost, and will
be expensive to make REQUIRED later. This CR spends that window on exactly one thing: **`layout`**. A
decoder reply that does not say what it decoded against is not degraded information, it is *confidently
wrong* information — §4's `binding` argument, transferred (§5.2). Everything else it could have made
REQUIRED, it deliberately did not.

---

## 2. Evidence base

### 2.1 Sources read, and at what revision

| Source | Read as | Revision |
|---|---|---|
| `empyrean/contract/protocol.md` | committed blob, `git -C ../empyrean show origin/main:contract/protocol.md` | `origin/main` **`39cfaa27`**, blob `b4776ce9` |
| `empyrean/contract/schema/bus-protocol.schema.json` | blob id only, `git rev-parse` | `origin/main` `39cfaa27`, blob `7b24bced` |
| `empyrean/docs/2026-08-22-protocol-schema-audit.md` | committed blob, same route | `origin/main` `39cfaa27` |
| `empyrean/docs/{ASSEMBLER_VISION,STUDIO_VISION,ROADMAP}.md`, `CLAUDE.md` | committed blobs, same route | `origin/main` `39cfaa27` |
| `oracle/crates/oracle-aether/tests/contract/bus-protocol.schema.json` | vendored copy, parsed with `json.load` | worktree at `0d7c5c2`, blob `7b24bced` |
| `oracle/crates/oracle-aether/src/engine.rs` | worktree file | `0d7c5c2` |
| `oracle/crates/oracle-aether/tests/common/schema.rs` | worktree file | `0d7c5c2` |
| `oracle/docs/2026-08-22-peer-schema-defect-answers.md` | worktree file | `0d7c5c2` |
| `oracle/docs/OVERSEER.md` | worktree file | `0d7c5c2` |
| `oracle/docs/2026-08-26-obj-decode-demand.md` | worktree file | `0d7c5c2` |
| `aeon/engine/objects/sst.emp`, `engine/objects/core.emp`, `engine/ram.emp`, `engine/system/constants.emp`, `engine/objects/objcodebase.emp`, `games/sonic4/player/player_common.emp` | committed blobs, `git -C ../aeon show f4896139:…` | aeon **`f4896139`** |
| `aeon` tree (consumer sweep) | committed tree, `git grep <sha>` | aeon `b87e6e5a53d1e4ec45f2bdf614c663d5025e0eb7` |
| `aurora/docs/reviews/2026-08-22-oracle-instrument-gaps.md` | committed blob | aurora `e5dd32a56d4b4e11b5c28b614d14bba47bfdfd86` |
| `oracle-old/linux-port/gui/ControlSocket.cpp` | committed blob | oracle-old `9dc67c5bb4e85b4c70e80e8a5b198f00d824877e` |
| `sonic_hack/S4.constants.asm` | committed blob | sonic_hack `858af72c50083fa9e721ac1ecd69095022d3659e` |

Five provenance facts, stated rather than assumed:

- **The vendored schema in this repo is byte-identical to upstream's, re-verified for this CR.**
  `git hash-object crates/oracle-aether/tests/contract/bus-protocol.schema.json` → `7b24bcedc24f0a6aa7dd4504f4e2f9bf63e4cda7`;
  `git -C ../empyrean rev-parse 39cfaa27:contract/schema/bus-protocol.schema.json` → the same blob.
  Fragment text quoted below is the upstream artifact.
- **The contract path is `contract/protocol.md`, not `docs/protocol.md`.** Confirmed by
  `git -C ../empyrean ls-tree -r 39cfaa27 --name-only | grep -i schema`, which lists
  `contract/schema/bus-protocol.schema.json` and no `docs/` counterpart.
- **The filed demand cites empyrean `fc7d7a5`; this CR reads `39cfaa27`.** The demand's contract claims
  were re-checked at `39cfaa27` and all hold (§2.3). This repo's vendored schema `PROVENANCE.md` records
  its bytes as taken at `fc7d7a58ac2ab413f1a13e1ef7229a2e4702b016` (2026-08-25) — so the contract *prose*
  moved between `fc7d7a5` and `39cfaa27` while the schema blob did not.
- **`f4896139` is a DOCS commit** (`git show f4896139 --stat` → one file, `docs/lane-log.jsonl`, +1). It is
  cited here **as a tree revision** — the source files read at it are whatever the last code commit left —
  and never as a commit that vouches for code. `git merge-base --is-ancestor f4896139 origin/master`
  succeeded, so it is on aeon's published history.
- **`../aeon`, `../aurora`, `../empyrean`, `../oracle-old`, `../sonic_hack` are peers' live working
  trees.** Every file from them was read through `git show <rev>:<path>` or `git grep <rev>`, never through
  the path. Nothing below quotes an uncommitted mid-edit file.

### 2.2 What was not checked, and is therefore taken as given

- **No `cargo` command was run** — another lane holds that resource in this repo, and a docs parcel needs
  none. Every count of this repo's own surface below is a *source* count with its command shown.
- **No emulator was driven.** Nothing about what either server actually *replies* at runtime is asserted;
  every legacy shape below is read from C++ source. §12.7 collects the items that want a runtime check.
- **aeon's `.lst` was not read.** The demand names `s4.debug.lst` as the symbol authority; this CR asserts
  only that the symbols `Object_RAM`, `Player_1`, `Player_2`, `Dynamic_Slots`, `System_Slots`,
  `Effect_Slots`, `Object_RAM_End` and `ObjCodeBase` are *declared* (§2.4), never what addresses they
  resolve to in any particular build. §2.4 shows why that distinction is load-bearing.
- **empyrean's own gate** (`contract/schema/tests/validate_contract_schema.py`) was read only through the
  audit's and the schema `$comment`'s description of it. Its check G5 is asserted on their word.

### 2.3 The brief's contract claims, re-checked at `39cfaa27` — four correct, two adjusted

The dispatch that produced this CR supplied six statements as hypotheses to check. All six were checked
against command output; **two need correction**, and both corrections are small and in the same direction.

| Statement checked | Verdict | Evidence |
|---|---|---|
| §6 catalogues both as ⚙ rows around line 1496, `object_list` → `objects[]{slot,…,x,y,class}`, `player_state` → "engine-dependent decoded player struct(s)" | **CORRECT, exactly** | lines 1496 and 1497 verbatim; group heading `### object / player decoders ⚙` at 1492 |
| Neither has a fragment at empyrean tip nor in our vendored copy | **CORRECT** | `d['methods']` at the vendored blob contains neither key |
| "…(60 fragments)" | **ADJUSTED: 59 fragments, 60 keys** | `d['methods']` has **60** keys; exactly one, `$comment`, is not a method. This is the same off-by-one CR-C recorded at the 58/59 boundary — *"the 59 is the miscount a naive key count produces"* — reproduced one fragment later. The parsing command and its output are in §2.5. |
| `capabilities.objectDecoders` is `false` at `engine.rs:1393`, and §6's note says the flag and the engine-detect result SHOULD be surfaced | **CORRECT on both halves** | `grep -n objectDecoders crates/oracle-aether/src/engine.rs` → `1393:                "objectDecoders": false,`; protocol.md:1500-1502 |
| `Engine::symbol_at` ~`:1559`, plus `lookup_symbol` / `load_symbols` | **CORRECT, exact line** | `grep -n 'fn symbol_at' …/engine.rs` → `1559`; both method literals present in the 44-literal set (§2.5) |
| Next free amendment section is §11.25 | **CORRECT** | `grep -nE '^### 11\.'` → last is `4227:### 11.24 — 2026-08-26: batch B1…`; file is 4265 lines |

**A hypothesis of this CR's own, formed and then retracted.** While reading the legacy decoder it appeared
to contain a stride defect: `slotAddr = base + slot * 0x50` is computed **before** the engine branch
(`ControlSocket.cpp:987`, `:1103`) and so applies `$50` to the `sonic_hack` layout as well as to aeon's.
That would misaddress every slot after 0 **if** `sonic_hack`'s object record were the 64 bytes this
workspace's `CLAUDE.md` describes. It is not:
`git -C ../sonic_hack grep -n 'object_size' 858af72:S4.constants.asm` → `172:object_size = $50`. The
legacy code is **correct**, `CLAUDE.md`'s "64 bytes" is the stale statement (it describes stock Sonic 2),
and its slot count checks out too — `S4.constants.asm:1084-1119` gives 12 reserved slots plus
`$60*object_size` dynamic = 108, which is exactly the legacy's `maxSlots` for that branch (`:976`,
`:1095`). Recorded because a retracted finding is cheaper to publish than to re-derive, and because it is
the second-order form of the error this CR is otherwise about: a plausible layout fact, taken from a
document rather than from the layout's own source.

### 2.4 The SST layout, re-derived from `sst.emp` at `f4896139`

Read with `git -C ../aeon show f4896139:engine/objects/sst.emp`. The struct declares its size in-file and
carries an `@` literal offset on every field past the first, and the module's own comment says the layout
engine verifies both — *"a drifted field fails the module's own lowering."* So these offsets are checked
by aeon's build, not merely written down.

`pub struct Sst (size: $50)`:

| field | offset | declared type | note from source |
|---|---|---|---|
| `code_addr` | `$00` | `ObjRoutine` (word) | *"The first word IS the dispatch … ObjCodeBase + code_addr each frame (0 = empty slot, the bank's safety rts)"* |
| `x_pos` | `$02` | `Coord` | 16.16 subpixel |
| `y_pos` | `$06` | `Coord` | 16.16 subpixel |
| `x_vel` | `$0A` | `Velocity` | 8.8 fixed-point |
| `y_vel` | `$0C` | `Velocity` | 8.8 fixed-point |
| `render_flags` | `$0E` | `u8` | bit0 on-screen, 1 xflip, 2 yflip, 3 coord mode, 4 multi-sprite, **bits 5–7 priority band** |
| `collision_resp` | `$0F` | `u8` | 0 = none |
| `mappings` | `$10` | `u32` | ROM pointer |
| `art_tile` | `$14` | `VramArtTile` | |
| `width_pixels` | `$16` | `HitboxDim` | full, not half |
| `height_pixels` | `$17` | `HitboxDim` | |
| `anim` | `$18` | `AnimId` | |
| `subtype` | `$19` | `u8` | |
| `anim_table` | `$1A` | `u32` | ROM pointer |
| `status` | `$1E` | `u8` | `ST_*` constants |
| `angle` | `$1F` | `Angle` | |
| `prev_anim` | `$20` | `AnimId` | |
| `anim_frame` | `$21` | `AnimFrame` | |
| `anim_timer` | `$22` | `u8` | |
| `mapping_frame` | `$23` | `MappingFrame` | |
| `prev_frame` | `$24` | `MappingFrame` | |
| `sprite_piece_count` | `$25` | `u8` | |
| `parent_ptr` | `$26` | `u16` | |
| `sibling_ptr` | `$28` | `u16` | |
| `slot_tag` | `$2A` | `TagRef` | `$FF` = untagged |
| `entity_section_id` | `$2B` | `u8` | |
| `entity_list_index` | `$2C` | `u8` | |
| `layer` | `$2D` | `u8` | 0 = path A, 1 = path B |
| `frame_off` | `$2E` | `u16` | engine render cache |
| **`sst_custom`** | **`$30`** | **`[u8; 32]`** | **the per-object overlay window, `$30–$4F`** |

**Agreement with the demand doc's table: complete for every row it lists — same offsets, same order, same
size `$50`.** One omission, and it is the important one:

> **The demand's table stops at `frame_off @ $2E` and does not carry `sst_custom: [u8; 32] @ $30`.**

That is 32 of the record's 80 bytes — 40% of it — and it is precisely the part that is **not** a fixed
layout (§3.2, §7.1). The demand table also does not mention the engine-owned tail word `SST_interact` at
`$4E`, which `sst.emp` defines as `sizeof(Sst) - 2` via `pub comptime fn interact_off()` and describes as
*"NOT a Sst struct field, so it can't be reached by field name"* — i.e. the custom window has **30**
game-usable bytes, not 32. Neither omission contradicts the demand; both matter to §7.

Two further facts read from the same file, both load-bearing below:

- **The record's size is not historically stable.** `sst.emp`'s own comment dates the current `$50` to a
  2026-08-05 "sst-fold" that *"shrink[ed] the record `$52` -> `$50`"*, and `core.emp:52` still carries a
  stale sentence reading *"Since the SST grew to `$52`"*. A decoder that hardcodes `$50` was wrong three
  weeks ago and will be wrong again.
- **`code_addr` is an offset, not an address.** `objcodebase.emp:4-6`: *"The object code bank starts at
  `$10000` (ObjCodeBase) … Every object routine's code_addr is `label - ObjCodeBase`."* `objroutine(0)`
  resolves to `ObjCodeBase` itself, whose `rts` returns immediately — so `0` is simultaneously the
  empty-slot sentinel and a legal routine offset. Nothing outside aeon can turn a `code_addr` into an
  address without `ObjCodeBase`.

**Pool bounds, from `core.emp` at the same SHA.** The demand cites `core.emp:207-280`; the pool table is at
**`:207-215`**, inside `DeleteObject`'s header comment, and it is an algorithm rather than a table:

```
// Pool detection (RAM order: Player | Dynamic | System | Effect):
//   addr <  Dynamic_Slots                              → player slot (no stack push)
//   addr >= Dynamic_Slots AND addr < System_Slots      → dynamic pool
//   addr >= System_Slots  AND addr < Effect_Slots      → system slot (no stack push)
//   addr >= Effect_Slots  AND addr < Effect_Slots + SST_len*NUM_EFFECTS → effect pool
```

The counts come from `engine/system/constants.emp:78-90`:
`NUM_PLAYERS = 2`, `NUM_DYNAMIC = 40`, `NUM_SYSTEM = 8`, `NUM_EFFECTS = 16`,
`NUM_TOTAL_SLOTS = NUM_PLAYERS + NUM_DYNAMIC + NUM_SYSTEM + NUM_EFFECTS` — **66**, which is exactly the
legacy server's hardcoded `maxSlots` for this branch. The RAM order is declared once, in
`engine/ram.emp:612-618`:

```
    mark Object_RAM,
    Player_1:               Sst,
    Player_2:               Sst,
    Dynamic_Slots:          [Sst; NUM_DYNAMIC],
    System_Slots:           [Sst; NUM_SYSTEM],
    Effect_Slots:           [Sst; NUM_EFFECTS],
    mark Object_RAM_End,
```

and `core.emp:35` pins the adjacency with a link-time `ensure`. So every quantity a decoder needs —
base, stride, per-pool first-slot and count — is derivable from **seven declared symbols** plus one
constant, and none of it needs a number typed into a server.

**Why "derivable from symbols" is not a stylistic preference here.** A committed fixture in aeon's own
tree, `tools/fixtures/s4_listing_excerpt.lst`, gives `Dynamic_Slots : FFFF8DC2`. The demand doc gives
`Player_1` as `$FF8DB0`. Those two cannot both describe one build: `Player_1 + 2 × $50` is `…8E50`, not
`…8DC2`. One of them is from an older ROM. **That is the whole argument for D1 in one pair of numbers** —
any address in this family is a fact about a build, and a decoder that carries one is carrying a fact
about a build it is not looking at.

### 2.5 The schema and catalog, enumerated — with the parameter named each time

Every completeness claim below names what it enumerated over. Nothing here is asserted as complete that
was not produced by a command whose output is shown.

**(a) Schema fragments.** Enumeration parameter: **the literal key set of the JSON object at
`$.methods`** in the vendored schema blob `7b24bced`, via `json.load`, no regex involved.

```
methods container keys: 60
  of which method names (prefix "emulator/"): 59
  of which non-method:  1   ($comment)
```

`emulator/object_list` and `emulator/player_state` are **not among the 59**. Neither are
`emulator/object_slot`, `emulator/call_stack`, `emulator/log_tail`, `emulator/z80_registers`,
`emulator/read_vdp_registers`, `emulator/read_vsram`.

**(b) The unschematized set, derived rather than transcribed.** Enumeration parameter: the regex
`` `(emulator/[A-Za-z0-9_]+)` `` applied to **table rows only** (lines whose first non-space character is
`|`) in `protocol.md` lines **896–1927** — the span from `## 6. Method catalog` to `### 6.1 Checkpoints`.
The character class is `[A-Za-z0-9_]` deliberately: this repo lost a count once to a `[a-z_]` class that
hid a capital letter, and camelCase is the contract's own spelling rule.

```
catalogued names:                  67
schema fragment names:             59
CATALOGUED WITHOUT FRAGMENT (8):
    emulator/call_stack
    emulator/log_tail
    emulator/object_list
    emulator/object_slot
    emulator/player_state
    emulator/read_vdp_registers
    emulator/read_vsram
    emulator/z80_registers
FRAGMENT NOT IN THAT RANGE (0):
```

The difference set matches the eight the schema `$comment` names, **and the reverse difference is empty**,
which is the check that makes the 67/59/8 arithmetic mean something rather than merely add up. This is a
re-derivation, not a transcription of the `$comment`'s list.

**(c) This server's advertised surface.** Enumeration parameter: unique matches of
`"emulator/[A-Za-z0-9_]+"` — quoted string literals — in `crates/oracle-aether/src/engine.rs` at `0d7c5c2`.

```
44 unique literals = 41 methods + 3 events (stopped, resumed, romReloaded)
```

41 matches the demand's measured figure (*"Measured against `oracle-frontend` at `1000472` (41 methods)"*).
Neither decoder name appears. `emulator/lookup_symbol` and `emulator/load_symbols` both do, which is the
machinery §5 builds on.

**(d) Where item 20's closure actually reaches.** Enumeration parameter: the single call site that applies
it, `crates/oracle-aether/tests/common/schema.rs::closed` (lines 129–136), read in full. Its doc comment
states the scope explicitly:

> Closure is applied at the **top level of the result object**. That is item 20's literal subject ("any
> result key"), and it is where the whole measured surplus lived. Nested objects are closed only where the
> contract closes them itself — `otherMatches.items[]` carries its own `additionalProperties: false` in the
> published schema, which is legal there because that subschema has no `allOf` to see past.

**This is the reference server's *reading* of item 20, not item 20's text**, and §3.3 refuses to lean on
it. It is recorded because an adjudicator should know that the reading exists and is shipping.

### 2.6 Consumer sweep — commands, enumeration parameters, and every hit classified

**What was swept.** Enumeration parameter for the *repository* set: the six sibling repos named by the
suite's own `CLAUDE.md` project list, minus this one — `aeon`, `aurora`, `seraph`, `sigil`, `empyrean`,
`oracle-old` — plus `oracle` itself, each at the committed revision in §2.1. Every sweep ran
`git grep <sha>`, which searches the **committed tree**; that is also what excludes live worktree copies
(`.claude/worktrees/…` is untracked in every one of them) without needing a path filter.

**Three greps per repo, because no one of them is a superset of the others.** A variable named
`object_list` may read a wire key spelled `objectList`; and the MCP tool spelling
`emulator_object_list` is **invisible to the bare-identifier pattern**, because the character before
`object_list` there is `_`, which the boundary class treats as a word character. That is not a
hypothetical: it is where a first pass at this sweep lost eleven hits in aeon alone.

| # | pattern | catches |
|---|---|---|
| **A** | `(^\|[^A-Za-z0-9_])(object_list\|player_state)([^A-Za-z0-9_]\|$)` | the bare token, and the `emulator/…` bus spelling |
| **B** | `emulator[/_](object_list\|player_state)` | the bus spelling **and** the MCP tool spelling `emulator_…` |
| **C** | `(^\|[^A-Za-z0-9_])(objectList\|playerState)([^A-Za-z0-9_]\|$)` | a camelCase wire key |

A and B overlap **only** on the `emulator/` slash form, which is counted separately below so the two
columns can be added without double-counting. Every figure is a **matching-line count**, produced by
`git grep -cE <pat> <sha>` summed over files; the sigil row was cross-checked against
`git grep -nE … | wc -l` and the two agree at 10 (an earlier by-eye count of the same listing said 8 — the
`-c` sum is the authority, and the by-eye figure is why this note exists).

| repo | rev | **A** | **B** | **C** | slash-form (A∩B) | files (A) |
|---|---|---:|---:|---:|---:|---:|
| aeon | `b87e6e5` | 45 | 11 | **0** | 0 | 22 |
| aurora | `e5dd32a` | 1 | 0 | **0** | 0 | 1 |
| seraph | `2c8dc88` | 0 | 0 | **0** | 0 | 0 |
| sigil | `e7f596e` | 10 | 2 | **0** | 0 | 9 |
| empyrean | `39cfaa27` | 18 | 2 | **0** | 2 | 10 |
| oracle-old | `9dc67c5` | 14 | 2 | **0** | 0 | 5 |
| oracle (this) | `0d7c5c2` | 24 | 4 | **0** | 4 | 13 |
| **total** | | **112** | **21** | **0** | **6** | **60** |

**camelCase form: zero hits in all seven trees.** That is a real finding, not an absence of effort: it
means **no wire key named `objectList`/`playerState` exists anywhere in the suite**, so nothing constrains
the spelling this CR proposes, and there is no second vocabulary to reconcile with.

**A mention is not an invocation, and here the distinction has a clean answer.** Of the 127 distinct
mentions (112 in A plus the 15 B-hits that A cannot see), **exactly six are executable**, and all six are
in `oracle-old`:

```
oracle-old 9dc67c5:linux-port/gui/ControlSocket.cpp:2657  {"object_list",       OpObjectList},
oracle-old 9dc67c5:linux-port/gui/ControlSocket.cpp:2658  {"player_state",      OpPlayerState},
oracle-old 9dc67c5:linux-port/gui/ControlSocket.cpp:2835  … || op == "object_list" || op == "object_slot"
oracle-old 9dc67c5:linux-port/gui/ControlSocket.cpp:2836  || op == "player_state" || …
oracle-old 9dc67c5:linux-port/mcp/oracle_mcp.py:701       "object_list",
oracle-old 9dc67c5:linux-port/mcp/oracle_mcp.py:709       "player_state",
```

Four are name registrations — the server's dispatch table and the MCP shim's tool table. The other two are
a log-rate-limiting "noisy op" list, which is executable but is not a call. **There is no call site in any
consumer tree** — and that is not because nobody calls them. It is because **every consumer of these two
methods calls them as MCP tools, from an agent session, and an MCP tool call leaves no artifact in a source
tree.** The evidence for that is the *form* the B-hits take in aeon:

```
aeon b87e6e5:docs/BUGS.md:1119                       (`emulator_object_list` confirms), fly ~80px above it…
aeon b87e6e5:docs/benchmarks/effects-p2/GATE-EVIDENCE.md:119   address from `emulator_player_state`, never from a doc.
aeon b87e6e5:docs/superpowers/2026-08-12-next-session-handoff.md:247   `emulator_object_list` showed the platform…
aeon b87e6e5:docs/superpowers/plans/2026-04-25-particle-effect-pool-test.md:625  Use Exodus MCP `emulator_object_list` to confirm those slots are empty (code_addr = 0)
aeon b87e6e5:docs/superpowers/plans/2026-06-15-sonic-animations.md:752  Confirm via `mcp__exodus__emulator_player_state` that the player is frozen…
```

Eleven such mentions in aeon, two more in sigil, two in `oracle-old`'s own docs. Every one names the **MCP
tool spelling** (`emulator_object_list`, `mcp__exodus__emulator_player_state`) rather than the bus method
(`emulator/object_list`) — the slash form's count is **0** in all three of those trees — and every one is a
*record of having used it*: "confirms", "showed", "take it from", "confirm via". So the honest statement is:

> **We cannot produce a call count, and no count would be meaningful.** What we can assert is the *form*:
> both methods are reached through the MCP shim, by name, by agent sessions in at least two consumer repos
> (aeon and sigil), and fifteen separate written records exist of a session having relied on a reply
> (11 in aeon, 2 in sigil, 2 in `oracle-old`'s own docs).
> `GATE-EVIDENCE.md:119` — *"address from `emulator_player_state`, never from a doc"* — is a consumer
> instructing itself to prefer this surface over its own documentation.

**The name collision, and it is not incidental.** aeon's 45 A-hits split **20 in source** (`engine/**`,
`games/**`) and **25 in docs**; every one of the 20 is the token `player_state` used for something else
entirely — a **field of aeon's own player overlay** — and **zero** of them are `object_list`
(`git grep -cE '…object_list…' <sha> -- 'engine/**' 'games/**'` → `0`). sigil carries the same collision in
its porting notes.

```
aeon b87e6e5:games/sonic4/player/player_common.emp:91    player_state:     u8,   // PSTATE_* (jump-table byte offset)
aeon b87e6e5:games/sonic4/player/player_common.emp:195   equ _pl_state = offsetof(Sst, sst_custom) + offsetof(PlayerV, player_state)
aeon b87e6e5:games/sonic4/objects/dust_spindash.emp:85   cmpi.b  #PSTATE_SLIDE, PlayerV.player_state(a0)
```

So on the one engine this method exists to serve, `player_state` names **a single byte at `sst_custom+2`**,
while the bus method named `player_state` returns a whole decoded player. This CR does **not** propose a
rename (§11.4), but the collision is why §7 is careful to say what the method's result *is* in one
sentence, and it is a standing hazard for anyone reading a consumer's prose.

**Where this sweep is weaker than it looks.** It measures *tracked files at one revision per repo*. It
cannot see: an agent session's tool calls (the actual invocations, as argued above); an untracked local
script; or a consumer outside `/home/volence/sonic_hacks/`. And each sibling revision is that peer's
current `HEAD`, which is a live tree's tip and will move.

### 2.7 What the legacy server actually does — evidence, never the target

Read from `oracle-old` `9dc67c5:linux-port/gui/ControlSocket.cpp`. This repo already transcribed these
shapes in `docs/2026-08-22-peer-schema-defect-answers.md` §5–§6 (at `0d7c5c2`), explicitly labelled
*transcription material, NOT proposed fragments*; what follows is re-read from the C++ and agrees with it.

**`OpObjectList` (`:1088-1142`).** No params. Top level: **one key**, `objects` — array, always present,
possibly empty. **No count, no total, no engine, no truncation flag.** Per item, 5 keys:
`{slot, pool, x, y, class}` on the aeon branch, `{slot, id, x, y, class}` on the other. Empty slots are
skipped (`continue` on `codeAddr == 0`), so **presence is activity** and slot numbers are sparse.

**`OpPlayerState` (`:1150-1286`).** No params. Top level: **`{engine, player_1, player_2}` on one branch,
`{main, sidekick}` on the other** — a key set that changes with the ROM, and an `engine` discriminant
present on only one of the two branches. Each nested player carries 12 keys when active, 2 when not, and
a `status` object of the form `{raw, bits:[…]}` whose bit names come from a hardcoded C table
(`stBits`, `:1187-1190`): `b0, xflip, yflip, in_air, rolling, on_object, pushing, underwater`.

**The supporting machinery (`:933-963`).** `DetectSST` branches on whether the symbol `Player_1` resolves.
`S4PoolName` hardcodes the boundaries **2 / 40 / 8** as C integer literals. `S4ClassName` looks up
`ObjCodeBase` (falling back to `0x10000`), forms `romAddr = (codeBase & 0xFFFF0000u) | codeAddr`, resolves
the nearest symbol within `$100`, and then **strips a `_Main` suffix**. `Object_RAM` is looked up with a
`0xFFB000` fallback; the stride `0x50` and the slot counts `66`/`108` are C literals.

**What this is evidence *for*, and it is worth stating plainly.** A real implementation, serving real
consumers for months, found it needed: a per-slot address, world-pixel `x`/`y`, an activity test, a pool
label, a symbolic identity, and an engine discriminant. Those six wants are the skeleton of §6 and §7, and
they were *measured* by someone shipping rather than reasoned by someone drafting. Two of them are things
this CR would not have thought to require: the per-slot `addr`, and the fact that `engine` needs to be on
the *reply*.

**What it is evidence *against*.** Five properties are non-conformant or unsafe on the contract's own
rules, and the consumer's reference policy means none of them needs defending here — they are simply not
inherited:

1. **Wire keys are snake_case** (`code_addr`, `mapping_ptr`, `anim_frame`, `symbol_disp`). §10 decision 4
   and §3 make camelCase normative for fields (protocol.md:712-713, 2332).
2. **The top-level key set varies by ROM** (`player_1` vs `main`). Our own transcription calls this
   *"the biggest shape hazard in the eight"*; a client branching on `engine` mis-handles every reply from
   the branch that omits it.
3. **`class` is a name that cannot round-trip.** `S4ClassName` strips `_Main`, so `Foo_Main` is reported as
   `Foo`, which resolves to nothing. §4 is unambiguous: *"`name` is the identifying spelling, and it MUST
   round-trip."* And an absent class is spelled `""` on `object_list` while the same fact is an *omitted
   key* on `object_slot` — one datum, two absence conventions, two methods, one family.
4. **The bit names are invented.** `b0`, and `in_air` here against `air` on the other engine. §11.18's rule
   that an emitted enum cannot be widened later makes freezing these expensive, and `status2`'s
   placeholders (`s2b0`…`s2b4`) are not names at all.
5. **The layout constants are C literals.** `0x50`, `66`, `2/40/8`, `0xFFB000`, `0x10000`. §2.4 showed the
   record was `$52` three weeks ago; §2.4 also showed two published aeon artifacts disagreeing about
   `Player_1`'s address. Every one of those literals is a bet on a build.

---

## 3. The central tension, stated before it is resolved

### 3.1 The rule that says these two rows should stay unschematized

§8 item 20 (protocol.md:2142) requires a server's harness to **fail on any result key absent from that
method's fragment**, implemented as `unevaluatedProperties: false` applied at test time. The consequence,
in the audit's words (`docs/2026-08-22-protocol-schema-audit.md` §3):

> A fragment that declares half a row's keys therefore does not under-specify, it **actively refuses** the
> conformant server that emits the other half, and it does so while looking complete. §2.5 confirms the
> other direction reads absence correctly … So a missing fragment means "not yet transcribed", which is
> true, while a partial fragment asserts something false.

And the specific reason these two are in that set (D-27):

> These are the game-specific decoders §9 defers to **Phase 5** … their result shapes are *by construction*
> not fixed by this contract today. Transcribing them would mean **freezing one engine's layout into the
> bus**, which is the coupling Phase 5 exists to remove.

Both sentences are correct, and this CR does not dispute either. **A fragment that enumerated aeon's SST
fields as bus keys would be exactly the defect D-27 describes** — and it would be worse than the audit
says, because §2.4 shows aeon's own record changed size three weeks ago. A contract that had frozen it in
July would today be refusing aeon.

### 3.2 Why the tension is real for `player_state` in a way it is not for `object_list`

`object_list`'s catalogued result is `objects[]{slot,…,x,y,class}` — a shape with a **literal ellipsis** in
its key set, which is a transcription problem.

`player_state`'s catalogued result is *"engine-dependent decoded player struct(s)"* — which is **not a
shape at all**. There is no ellipsis to fill in; there is no key set to be partial about.

And the difficulty is deeper than "two engines disagree". Reading aeon's own player overlay
(`games/sonic4/player/player_common.emp:83-170` at `f4896139`), the decoded player struct is not stable
**within one engine, one ROM, one frame**:

```
pub vars PlayerV: Sst.sst_custom {
        ground_speed:     i16,
        player_state:     u8,
        …
        // --- ABILITY SCRATCH: … These two bytes are a UNION, not Tails' private property:
        // exactly one character is resident per slot … so Knuckles' glide/climb and any later
        // ability re-use the same bytes under their own names …
        fly_fuel:         u8,
        fly_thrust:       u8,
        glide_angle:      u8,
        knux_step:        u8,
        …
}
```

The same bytes are `fly_fuel`/`fly_thrust` when Tails is resident and `glide_angle`/`knux_step` when
Knuckles is. The module says so in terms: *"The language cannot express the union as byte-SHARING … so the
fields are declared in place and this reads as a budget principle."*

**So there is no such thing as "the decoded player struct" even for aeon alone.** A fragment that
enumerated `flyFuel` and `glideAngle` as sibling keys would be describing a record that never exists. This
is the strongest single fact in the CR and it points *away* from the shape the legacy server ships and the
demand implies.

### 3.3 The resolution, and why it is not a loophole

**Separate the part of the reply the contract owns from the part it cannot know, and publish the boundary.**

- The contract **owns**: that a reply carries a list, that the list obeys §2.4, that each entry has a slot
  index and a bus address and a world position and an identity datum, that a symbolic name obeys §4's
  round-trip rule, and that the reply says what layout produced it.
- The contract **cannot know**: which named fields a particular build's record has, what they mean, or
  whether two of them share bytes.

A fragment can state the first exhaustively and declare the second **as a typed-open map** — an object
whose key set is unbounded by construction and whose *value* shape is pinned. That is not a hole; it is a
declared property of the shape, and item 20's closure passes over it because the subschema says so, not
because the harness stops early.

**Three reasons this is a resolution and not a way around the rule:**

1. **It is the audit's own unblock condition, quoted verbatim in §1**: *"a config/symbol-driven decode whose
   envelope is fixed even though its fields are not — e.g. a declared `fields` map plus a `layout`
   discriminant."* This CR is proposing the thing D-27 said would unblock D-27.
2. **The mechanism already ships in this schema.** `handshake.initialize.result.capabilities` is an open
   object (no `additionalProperties`, no `unevaluatedProperties`), and `methodSummaries` is a *typed*-open
   map — `{"type":"object","additionalProperties":{"type":"string"}}` — with a normative prose clause
   binding its key set to `methods`. That is exactly the pattern: schema pins the value type, prose pins
   where the keys come from. Precedent found, not invented.
3. **It does not depend on the harness's depth.** §2.5(d) records that this repo's harness applies closure
   only at the top level of a result. This CR deliberately does **not** rest on that reading. The `fields`
   map carries its own `additionalProperties` subschema, so it is legal under either reading of item 20's
   scope — and §13.1 Q6 asks the adjudicator to *pin* the scope either way, because a rule this load-bearing
   should not be settled by one implementation's code comment.

### 3.4 What the resolution does not rescue

It does not rescue **decoded semantics**. A `fields` map can carry `{"anim": 3}` because "the byte at the
offset the layout calls `anim`" is a fact about memory. It cannot carry `{"status": {"bits": ["in_air"]}}`,
because "bit 3 of `status` means in-air" is a fact about a *game*, the server would be inventing the
spelling, and §11.18 says an emitted enum cannot be widened afterwards. §7.3 declines that half explicitly
and says what a client does instead.

---

## 4. The three properties

Everything in §5–§8 is a mechanism serving one of these. An adjudicator who rejects a mechanism but keeps
a property has said something useful; the reverse has not.

### P1 — A decoder reply is self-describing or it is not trustworthy

A client receiving `{"slot": 7, "x": 1024, "y": 320}` cannot tell whether the server read the right
addresses. The reply must carry enough to make it **checkable against another instrument on the same bus**
— which means a per-item bus `addr` (checkable with `emulator/read`), and a `layout` block naming what was
detected and how (checkable against `emulator/lookup_symbol`).

### P2 — What the server *assumed* is part of the answer, never a silent premise

§4 already establishes this for symbols, and states the reason better than a paraphrase can:

> a mismatched listing is not degraded information, it is **confidently wrong information** — and a client
> that cannot see it has no way to know how much to trust every address it resolves afterwards.

`load_symbols.binding` is that rule made a field. A decoder has the identical hazard one level up: it
assumes a *layout*. P2 says the assumption ships with the answer.

### P3 — Engine-specific detail is carried, never enumerated

The contract must be able to convey `anim = 3` without the contract knowing what `anim` is. A fixed
envelope carrying an open payload does that; a fragment listing engine field names does not, and would
freeze the coupling Phase 5 exists to remove.

---

## 5. D1 — the `layout` descriptor

### 5.1 The proposal

Every reply from an object-decoder method carries a REQUIRED `layout` object:

```json
"layout": {
  "engine": "aeon-sst",
  "detectedBy": "symbol",
  "detectedFrom": "Player_1",
  "slotBytes": 80,
  "slotCount": 66,
  "baseAddr": "0x00FF8DB0",
  "pools": [
    {"name": "player",  "firstSlot": 0,  "slotCount": 2},
    {"name": "dynamic", "firstSlot": 2,  "slotCount": 40},
    {"name": "system",  "firstSlot": 42, "slotCount": 8},
    {"name": "effect",  "firstSlot": 50, "slotCount": 16}
  ]
}
```

| key | type | required | meaning |
|---|---|---|---|
| `engine` | string | **yes** | The server's name for the layout it decoded against. **A free string, not an enum** — §11.18's rule that an emitted enum cannot be widened later applies with full force to a value whose whole job is to grow one entry per supported game. Clients compare for equality; they do not switch on a closed set. |
| `detectedBy` | string | **yes** | How the layout was chosen. Registered values: `"symbol"` (a symbol table resolved the discriminant), `"configured"` (an operator named it), `"fallback"` (nothing resolved and the server guessed). Also a free string for the same reason. |
| `detectedFrom` | string | no | The symbol whose resolution decided it, when `detectedBy` is `"symbol"`. `$defs/symbolName`, so it round-trips through `lookup_symbol`. |
| `slotBytes` | integer | **yes** | Stride of one record, in bytes. D9 category 2. |
| `slotCount` | integer | **yes** | Total slots the pool spans — what a full scan would cover. |
| `baseAddr` | `$defs/hex` | **yes** | Bus address of slot 0. Lets a client verify every item's `addr` with one multiplication. |
| `pools` | array | no | The pool vocabulary **as data**: `{name, firstSlot, slotCount}` per entry, in ascending `firstSlot` order, contiguous and covering `[0, slotCount)` where present at all. Absent on an engine with no pool structure. |

### 5.2 The argument — and why `layout` is the one thing made REQUIRED

The pre-release window for REQUIRED keys shuts at first ship. This CR spends it on `layout` and on nothing
else, for P2's reason: **an unstated layout assumption is the `binding: indeterminate` hazard with no
`binding` field.** §4 pays a whole paragraph to the difference between "degraded" and "confidently wrong",
and a decoder that silently fell back to `0xFFB000` (as the legacy does, `:985` and `:1097`) hands back numbers that
are wrong in exactly the way a client cannot detect.

Making it optional later is additive; making it required later is not. It costs a conformant server four
scalars it already knows in order to answer at all.

### 5.3 Why on the reply, not in `capabilities`

§6's own note asks for both the flag and the detect result to be surfaced (protocol.md:1500-1502), and the
natural-looking home for the second is the handshake. **It is the wrong home, and the reason is measured
rather than aesthetic:** `emulator/load_symbols` may be called at any point in a session, and the detect
branches on whether a symbol resolves (`DetectSST`, `:935-941`). So the detected layout **changes after the
handshake**, and a handshake-time value is stale by construction — for the whole session, silently, on a
key whose only job is to be trusted.

Hence the split, which answers §6's note exactly:
- **`capabilities.objectDecoders`** — static, handshake-time: *does this build have the handlers* (D4).
- **`layout`** — dynamic, per-reply: *what did it detect, just now* (D1).

### 5.4 Why `pools` is data rather than an enum

The demand asks for a per-slot `pool` of `player|dynamic|system|effect`. Making that a schema enum would
freeze aeon's four-pool structure into the bus for every future engine, which is D-27's stated objection
verbatim. Carrying the same information as a **table in the reply** gives the client strictly more (the
bounds, not just the label), costs nothing, and is widenable. §9.1 states what that costs the consumer:
one comparison.

### 5.5 Alternatives rejected

- **A separate `debug/struct_layout` method.** empyrean's `ASSEMBLER_VISION.md:154` already plans one
  (*"Add an Aether `debug/struct_layout` op"*), and a full field catalogue belongs there, not here. But a
  second round trip to learn what the first reply assumed reintroduces the skew D11's stamp exists to
  prevent — the layout could change between the two calls. `layout` is deliberately the **minimum** that
  makes *this* reply checkable; the catalogue stays future work.
- **A `layoutId` handle the client resolves separately.** Adds a D9 category-4 handle and a round trip to
  save ~120 bytes on a reply that already carries an array.
- **Reusing `engine` as a top-level key** (the legacy spelling). Rejected: the legacy emits it on one
  branch and not the other, and a bare string cannot carry `detectedBy`, which is the half P2 is actually
  about.

---

## 6. D2 — `emulator/object_list`

### 6.1 The proposal

**Params** (closed by §2.5, `unevaluatedProperties: false`):

| param | type | required | meaning |
|---|---|---|---|
| `limit` | integer ≥1 | no | Max entries, from the lowest slot upward. Default: all active slots. |
| `fields` | array of strings | no | Layout field names to decode into each item's `fields` map. Unknown name → `-32602`, with `error.data.unknownFields` carrying the offending names. |
| `includeBytes` | boolean | no | Default `false`. When true, each item carries `bytes`: its whole record verbatim. **Severable — §13.1 Q3.** |

**Result** (top level, closed):

| key | type | required | note |
|---|---|---|---|
| `objects` | array | **yes** | §2.4's *flat* spelling — the list IS the result, so no container object. |
| `total` | integer | **yes** | How many **active** slots exist. This is the demand's `count`. |
| `returned` | integer | **yes** | §2.4 clause (a). |
| `limit` | integer | **yes** | The ceiling actually applied, which may differ from the one asked for. |
| `truncated` | boolean | **yes** | §2.4 clause (a) — **required even when false**. |
| `layout` | object | **yes** | D1. |
| `caveat` | string | no | §2.4. Declared because the method **can** emit one (§6.5); emitted conditionally, never on every reply. |

**No `cursor`, and none may be emitted** — §2.4 clause (b): the method accepts no continuation param, so a
token it issued could never be handed back. `lookup_symbol`'s ruling, unchanged.

**Per item** (`objects[]`):

| key | type | required | note |
|---|---|---|---|
| `slot` | integer | **yes** | Index into the pool. D9 category 2. Sparse: empty slots are omitted, so slot numbers skip. |
| `addr` | `$defs/hex` | **yes** | Bus address of the record. P1: the key that makes the reply checkable with `emulator/read`. |
| `x` | integer | **yes** | World **pixels**, signed. The integer half of the record's position; the sub-pixel half is not carried (§9.3). |
| `y` | integer | **yes** | Same. |
| `code` | `$defs/hex` | **yes** | **The engine's identity datum for the slot, exactly as read**, at the offset and width the layout declares. For aeon: the `code_addr` word at `$00`. A hex string per D9 category 1 — it is a raw payload, not something to count with. |
| `name` | `$defs/symbolName` | no | The **bare** label the server resolved for `code`. §4's identifying spelling: it MUST round-trip through `lookup_symbol`. Omitted when nothing resolved — never `""`. |
| `nameDisp` | integer ≥0 | no | Bytes past `name`. Present with `name`. §4: *"A displacement is never inside a name string."* |
| `fields` | object | no | **Typed-open map.** Keys are layout field names; values are `number \| string \| boolean`. Present iff the request asked for `fields`. |
| `bytes` | `$defs/hex` | no | The whole record, `slotBytes` long. Present iff `includeBytes`. |

**Not present, deliberately:** `pool` (derivable from `slot` + `layout.pools`, §9.1), `active` (presence is
activity, §9.2), and `codeAddr` as a separate resolved address (§6.3).

### 6.2 Why the flat bounded-list spelling with all four companions

§2.4's two spellings resolve this cleanly: *"A list that **is** the whole result keeps the flat spelling …
the array under its own name beside `total`, `returned`, `limit`, `truncated` and `cursor?` as siblings."*
`emulator/sprites` is the shipping precedent for exactly this on a per-entity decoded array, and its
fragment's own `$comment` says so: *"§2.4's flat bounded-list spelling, as `watchpoint_hits` uses it, with
`satBase` and `parsedMax` as scalars beside the list."*

**One semantic divergence from `sprites`, argued because it is real.** `sprites` pins `total` as
`{"const": 80}` — the table's size — because *every slot is an item* there. Here an empty slot is **not**
an item, so `total` is the count of active objects and the table's size lives in `layout.slotCount`. Those
are two different facts and this CR gives them two homes rather than one ambiguous key. This also answers
the demand's stated want directly: *"a `count` so 'zero effects' is a stated fact, not an empty list"* —
`total: 0` beside `truncated: false` is that fact, stated twice over.

### 6.3 Why `code` is the raw datum and the resolved address is not carried

`emulator/get_profiler_frames`' `routines.items[]` sets the precedent for identifying a decoded row, and it
picks the address: *"The row's identity: rows are keyed by entry address, never by symbol."* That argues
for emitting a resolved 24-bit `codeAddr` per item.

It was considered and rejected, for two reasons:

1. **It is derivable, and CR-13 struck derivable keys.** `emulator/sprites` makes the ruling concrete:
   *"Emitted ONCE here rather than as a per-entry `satAddr`: each entry's address is `satBase + index*8`,
   which D9 category 2 explicitly permits a client to compute."* Here `codeAddr = ObjCodeBase + code`, and
   `ObjCodeBase` is one symbol the client can resolve — or, if it prefers, one it already has, since
   `name`/`nameDisp` are the resolution done for it.
2. **It is not universally meaningful.** On a layout whose identity datum is an object *id* rather than a
   code offset (the legacy's other branch reads a byte `id` at `$00`), there is no address to resolve. A
   REQUIRED key that is meaningless on half the layouts is the freezing D-27 forbids. `code` — *the
   identity datum, verbatim* — is true on both.

The profiler precedent is honoured where it applies: the *symbolic* identity is `name`, and it obeys §4's
round-trip rule rather than the legacy's `_Main`-stripping (§9.4).

### 6.4 Why `fields` and not a fixed field list

P3, and D-27's own sketch. Three properties make the map safe:

- **Its keys are the layout's field names**, not names this contract chose. Prose binds them:
  *a server MUST NOT emit a key in `fields` that its `layout.engine` does not name.* That is the same
  shape as `methodSummaries`' normative clause (*"its key set MUST equal `methods`"*) — schema pins the
  value type, prose pins the key provenance.
- **Values are scalars only** (`number | string | boolean`). No nested objects, no arrays: a nested decode
  is a semantic claim, and §7.3 declines those.
- **The client asks.** `fields` is empty unless requested, so the default reply's key set is fully
  enumerated by the fragment, and a client that wants closure simply never sends `fields`.

The demand asks for *"the raw `sst` bytes, or at least `anim` and `mapping_frame`"*. Both halves are
served: `fields: ["anim","mapping_frame"]` for the second, `includeBytes` for the first.

### 6.5 Refusals, and one hardening

| condition | code | note |
|---|---|---|
| No symbol table loaded | `-32012` | The existing code, exact fit. **This is a hardening** — see below. |
| `fields` names something the layout does not have | `-32602` | `message` names the offending key and lists the accepted ones; `error.data.unknownFields` is an array of strings. §2.5's `unknownParams` shape, one level down. |
| `limit` below 1 or above `limits.maxObjectSlots` | `-32602` | Refused, never clamped. |
| Machine free-running | *none* | **A pure read.** §6's run-control state rule does not apply, exactly as for `read`, `sprites`, `pixel_attribution` and `scanlines`. The envelope's `running` is the whole answer to a torn sample. |

**The hardening, stated as such.** The legacy falls back to `0xFFB000` when `Object_RAM` does not resolve
(`:985` for `object_slot`, `:1097` for `object_list`). This CR proposes **refusing instead**: without symbols there is no layout, and a decode from a
guessed base is P2's confidently-wrong answer. `write_memory`'s precedent is the argument and the escape
hatch both: *"strict by design: relaxing a refusal later is additive (D5); introducing one is not."* A
server that later wants a configured fallback declares `detectedBy: "configured"` and answers. §13.1 Q4
hands this to the adjudicator because it is a deliberate divergence from the only shipping implementation.

`caveat` is declared and emitted **conditionally**: when `layout.detectedBy` is `"fallback"`, or when the
symbol table that produced the layout was accepted with `binding: "indeterminate"` (§4). Never
unconditionally — §2.4's advisory is explicit that a caveat on every reply is one nobody reads.

---

## 7. D3 — `emulator/player_state`

### 7.1 The finding this row turns on

§3.2 established it and it bears restating in one line, because it decides the whole design:

> **There is no fixed "decoded player struct", not even for one engine.** aeon's `PlayerV` overlays
> `Sst.sst_custom` and its ability-scratch bytes are a *union* over the resident character. A fragment
> enumerating `flyFuel` and `glideAngle` as siblings would describe a record that never exists.

### 7.2 The proposal

`player_state` is **`object_list` restricted to the player pool, with roles attached.** Params: `fields`?
and `includeBytes`?, identical in meaning to D2's. No `limit` — the player pool is structurally bounded
(§2.4 clause (d): structural bound → neither flag nor cursor).

**Result** (top level, closed):

| key | type | required | note |
|---|---|---|---|
| `players` | array | **yes** | **An ARRAY, not per-role keys.** One entry per player slot, ascending. Includes inactive slots (see below). |
| `layout` | object | **yes** | D1, identical. |
| `caveat` | string | no | §2.4, same conditions as D2. |

No `total`/`returned`/`truncated`: nothing is bounded by policy, so §2.4 clause (d) says none of them.

**Per item** (`players[]`): D2's item shape **exactly**, plus two keys:

| key | type | required | note |
|---|---|---|---|
| `active` | boolean | **yes** | Unlike `object_list`, inactive players are **returned, not omitted** — "player 2 is not present" is the answer to the question, and a client must not have to distinguish it from a short array. |
| `role` | string | no | The server's label for this slot, from the layout — `"player"`, `"sidekick"`, … A free string, §11.18's reason. |

Everything else — `slot`, `addr`, `x`, `y`, `code`, `name?`, `nameDisp?`, `fields?`, `bytes?` — carries D2's
meanings unchanged. When `active` is `false`, `slot` and `addr` are still present (they are facts about the
slot, not the object) and the rest are omitted.

### 7.3 What is deliberately not served, and what a client does instead

**The `status` bit-name decode is declined.** The legacy emits `{"raw": n, "bits": ["in_air", …]}` from a
hardcoded C table. This CR proposes that no server emit decoded bit names on this row, for three reasons:

1. **The names are invented.** `b0` is not a name. And the two branches disagree on spelling for the same
   concept — `in_air` against `air`, `on_object` against `onobject` — so cross-engine comparison of the
   field is unsafe *today*, in the only implementation that emits it.
2. **§11.18 makes emitted enums unwidenable.** Freezing eight strings per status byte, per engine, into
   the contract buys a display convenience and sells the ability to correct it.
3. **The semantics are the game's, not the bus's.** aeon's own `sst.emp` documents `render_flags`' bit
   meanings in a comment, and its own `constants.emp` owns the `ST_*` names. A server-side copy is a second
   copy that can drift while both halves look right.

**What a client does instead:** `fields: ["status", "render_flags"]` returns the raw values, and the client
applies the bit names it already has — from the same source that defines them. This is not a loss: the
legacy's `bits` array carries strictly less information than the `raw` beside it, because a set-bits list
cannot express a clear bit.

### 7.4 Why an array, when the legacy uses named keys

Because the named keys are the defect. This repo's own transcription
(`docs/2026-08-22-peer-schema-defect-answers.md` §6, at `0d7c5c2`) puts it plainly:

> `engine` is present on one branch and absent on the other, and a client that branches on `engine` will
> mis-handle every sonic_hack reply. The only reliable discriminator is `player_1` vs `main`. We would flag
> this as a defect rather than a shape to preserve.

An array has a key set that does not vary; a `role` string carries the label without buying a key; and
`layout` carries the discriminant on **every** reply rather than on one branch. Three defects, one shape
change. And it is what makes D2's item shape reusable, which is the property §7.5 argues from.

### 7.5 The alternative that was seriously considered: decline this method

**The case for declining.** On aeon, `player_state` ⊆ `object_list`: the players are slots
`[0, NUM_PLAYERS)`, which `layout.pools[0]` states. Everything D3 returns is obtainable from D2 plus one
bound the same reply already carries. The suite has a standing rule against derivable keys (CR-13) and a
standing dislike of two vocabularies wearing one name. §6's catalogued result for this row is not a shape.
And the only in-tree bus client has written that it should not ask for this surface at all (§12.1). By
every rule this contract usually applies, the parsimonious answer is: **serve `object_list`, decline
`player_state`, and tell a client the player pool is `pools[0]`.**

**Why it is nonetheless recommended for adoption.** One fact outweighs the parsimony: **the legacy server
serves `player_state` today and a real consumer relies on it** (§2.6 — eleven separate written records in
aeon of a session having used the decoder pair, including one instructing itself to prefer it over its own
docs). The
successor is scheduled to replace that server. Declining the row does not leave a gap in a contract; it
**removes a working surface at cutover**, which is the harm D5 exists to prevent, dressed as tidiness.
Serving it costs one fragment that reuses D2's item shape and adds two keys.

**The condition that would reverse this.** If the adjudicator finds that the cutover plan does not in fact
require the successor to cover the legacy's decoder surface — the acceptance contract in this repo's
`docs/OVERSEER.md` item 8 is built from the **schematized-and-unserved** set, and these eight rows are by
construction *not* in it — then the D5 argument weakens sharply and declining becomes correct. **This CR
cannot settle that; it is §13.1 Q1.**

---

## 8. D4 and D5

### 8.1 D4 — `capabilities.objectDecoders`, kept as a boolean with a pinned meaning

The key is published as `{"type": "boolean"}` and this server emits `false` (`engine.rs:1393`). The
tempting change is to promote it to an object, as `checkpoints` and `watchpoints` are
(`{"supported": …, "cap": …}`). **Rejected.** Changing a published key's JSON type is not additive, and D5
protects clients from exactly that; the information an object would carry is the detect result, which §5.3
showed must be per-reply anyway.

What it gains instead is a normative sentence, because today it has none:

> **`objectDecoders` reports whether this BUILD has the decoder handlers — never whether a layout was
> detected.** A server MUST advertise `true` iff the object-decoder method names appear in `methods`
> (§8 item 23's per-build warranty). The detect result is `layout` on each reply (§5.3); a client that
> branched on this flag to decide whether a decode would succeed would be reading a build-time constant as
> a run-time fact.

That is §6's *"the capability flag and the engine-detect result SHOULD be surfaced"* discharged in full,
with the two halves put where each is true.

### 8.2 D5 — `emulator/object_slot`, offered severably

`object_slot` is the third row in the same ⚙ group and the audit says its `slot` param *"is transcribable
on its own … and was withheld only under the no-half-fragment rule."* Once D2's item shape exists, its
fragment is:

- **Params:** `slot` (integer ≥0, REQUIRED), `fields`?, `includeBytes`? — D2's meanings.
- **Result:** `layout` (REQUIRED), `caveat`?, and **D2's item keys hoisted to the top level**, plus
  `active` (REQUIRED, D3's meaning — this row addresses a slot, so emptiness is an answer).
- **Refusals:** `slot` ≥ `layout.slotCount` → **`-32602`**. Note the divergence: the legacy answers
  `-32004` (*"slot out of range"*, `:978-983`). `-32004` is *"address out of range"*, and a slot index is a
  parameter, not an address; the fragment cannot bound it because the bound is a property of the loaded
  game. §13.1 Q5.

**No consumer asked for this.** It is offered because leaving one row of a three-row family unschematized
means the next CR reopens the same file for a fragment that is now nearly free, and because a family with
one absence convention is better than a family with two (§2.7 finding 3). **Sever it without prejudice.**

**A tidier alternative, offered and not pushed:** `object_list` gains an optional `slot` param and
`object_slot` becomes an alias deprecated by it, the way `read_memory` is deprecated by `read` and
retained. Declined here only because D5 requires retaining it anyway, so the deprecation buys nothing but
a second name for one behaviour.

---

## 9. The better-approach pass — every departure from the demand, with its cost

The standing directive is that a consumer's demand is the compatibility floor, never the design ceiling.
Below is every place this CR gives the consumer something other than what they asked for. Two things they
asked for are affirmed as already-right and the reason is given, because "we kept it" is also an outcome
of this pass.

### 9.1 `pool` moves from the item to the layout — DEPARTURE

**Asked for:** per slot, `pool: one of player|dynamic|system|effect`.
**Proposed:** no per-item `pool`; `layout.pools[]` carries `{name, firstSlot, slotCount}`.
**Precedent:** `emulator/sprites` struck a per-entry `parsed` flag for exactly this reason —
*"it is `index < parsedMax`, the derivable key CR-13 struck"* — and emitted `satBase` once instead of a
per-entry `satAddr`.
**Cost to the consumer:** one comparison, against a table the same reply hands them.
**What they gain:** the pool *bounds*, not just the label — so a gate asserting "the effect pool is empty"
can be written as a range check rather than a string filter, and the vocabulary can widen without a
contract amendment. The demand's own authority note asks that pool bounds be *"read … from the `.lst`,
never hard-code[d]"*; carrying the bounds on the wire is that instruction served rather than restated.

### 9.2 No `active` on `object_list`; `active` REQUIRED on `player_state` — DEPARTURE (split)

**Asked for:** *"Empty slots (`code_addr == 0`) omitted."*
**Proposed:** omitted on `object_list` (agreed), but **returned with `active: false`** on `player_state`.
**Reason:** on a list of *what exists*, presence is activity and a flag would always be `true`. On a fixed
two-slot roster, "player 2 is absent" is the answer to the question asked, and a client should not have to
infer it from an array's length. The legacy makes the same split and it is the one place its two decoder
rows are more careful than their catalog entry.
**Cost:** none; it is one extra entry on a two-entry array.

### 9.3 `x`/`y` stay world pixels; the sub-pixel half is reachable, not carried — AFFIRMED

**Asked for:** *"`x`, `y` in WORLD PIXELS: the integer half of the 16.16 … The gates compare against
ring/entity coordinates, which are pixel words."*
**Proposed:** exactly that, as REQUIRED signed integers.
**Why it is right and not merely conceded:** it is the one value that is comparable **across layouts** —
aeon stores 16.16 at `$02`, the other branch stores a whole-pixel word at `$10`, and "world pixels" is true
of both. Carrying a 16.16 raw would push the fixed-point convention into the contract, where it would be
aeon's convention wearing a bus key's name. A client that wants sub-pixels asks
`fields: ["x_pos"]` and gets the record's own value, or reads the four bytes at `addr + 2`.

### 9.4 `class` becomes `name` + `nameDisp`, and stops being mangled — DEPARTURE

**Asked for:** *"A symbolic name via the `.lst` is a nice-to-have, never required."*
**Proposed:** OPTIONAL, as the demand asks — but as `$defs/symbolName`, **bare and round-trippable**, with
the displacement in its own numeric key.
**What changes against the legacy:** `S4ClassName` (`:951-963`) strips a `_Main` suffix, so `Foo_Main`
arrives as `Foo`, which resolves to nothing. §4 is categorical: *"`name` is the identifying spelling, and
it MUST round-trip."* And absence is an **omitted key**, never `""` — the legacy uses `""` on
`object_list` and an omitted key on `object_slot` for the same fact.
**Cost:** a consumer that wants the pretty name displays `name` minus a suffix locally; a consumer that
wants to *act* on the name (set a breakpoint on that routine, look it up) can now do so, which it could not
before. This is the one place where the demand asked for less than it should have.

### 9.5 A per-item `addr` the demand did not ask for — ADDITION

**Not asked for.** Proposed as REQUIRED. The legacy emits it on `object_slot` and `player_state` but
**not** on `object_list`, and its absence there is the reason an `object_list` reply is unverifiable: there
is nothing in it to check against another instrument. With `addr`, any entry can be confirmed by
`emulator/read {addr, len}` and any field can be poked by `write_memory {addr, disp}` — which is P1, and
which is what turns this from a viewer into a debugger surface. Cost: 12 bytes per entry.

### 9.6 The raw bytes become a request, not a default — DEPARTURE

**Asked for:** *"the raw `sst` bytes, or at least `anim` and `mapping_frame`."*
**Proposed:** `includeBytes` (default off) for the first, `fields` for the second.
**Reason:** 66 records × `$50` is 5,280 bytes, which is 10,562 hex digits on **every** call for a caller
who wanted two coordinates. The `write_memory` precedent for one-payload-spelling-per-intent applies:
`bytes` XOR `value`+`width`, never both.
**Cost:** one param. And the `fields` route returns something better than raw bytes — a value the *server*
read at the offset the *layout* declares, which is the decode the client wanted rather than a byte string
it must re-offset itself.

### 9.7 What the legacy got right that the demand did not ask for — ADOPTED FROM EVIDENCE

Two, named because the reference policy forbids A/B-ing against the legacy's *shape* but says nothing about
learning from what a shipping implementation found it needed:

- **An engine discriminant on the reply.** The legacy emits `engine` (on one branch). D1 generalises it and
  fixes the branch asymmetry. Without a shipping implementation to look at, this CR would probably have put
  it in `capabilities` and been wrong for §5.3's reason.
- **The activity split** (§9.2). Reasoned independently, then found already made.

---

## 10. The exact deltas requested

### 10.1 `contract/protocol.md:1492-1503` — replace the group

Replace the `### object / player decoders ⚙` block (rows and the ⚙ note) with:

```markdown
### object / player decoders ⚙
| Method | params | result |
|---|---|---|
| `emulator/object_slot` ⚙ | `slot`, `fields`?, `includeBytes`? | `layout`, item keys hoisted, `active`, `caveat`? |
| `emulator/object_list` ⚙ | `limit`?, `fields`?, `includeBytes`? | `objects[]`, `total`, `returned`, `limit`, `truncated`, `layout`, `caveat`? |
| `emulator/player_state` ⚙ | `fields`?, `includeBytes`? | `players[]`, `layout`, `caveat`? |
| `emulator/call_stack` | `maxBytes`?,`maxFrames`? | `pc`,`sp`,`frames[]` |

> ⚙ These decode a game's object records, so **part of each reply is engine-shaped**. The contract fixes
> the envelope and leaves the payload open, deliberately: see §11.25. Two rules follow and both are
> normative. **(1)** Every reply carries `layout` — what the server decoded against, and how — because an
> unstated layout assumption is §4's *confidently wrong information* one level up. **(2)** Engine-specific
> values travel in the per-item `fields` map, whose keys are the layout's own field names and whose values
> are scalars; a server MUST NOT emit a `fields` key its `layout.engine` does not name, and MUST NOT emit
> decoded bit-name enums for any field. `capabilities.objectDecoders` reports whether this **build** has
> the handlers and never whether a layout was detected; the detect result is on the reply, because symbols
> may load after the handshake.
```

*(`call_stack` is unchanged and shown only for position.)*

### 10.2 `contract/protocol.md` §9, line 2277 — amend the deferral

The Phase-5 entry currently reads:

> - **Config/symbol-driven object decoders** — making `object_slot`/`player_state` not hardcode the
>   aeon vs sonic_hack layouts (Phase 5).

Append:

> *Partially lifted 2026-08-26 (§11.25):* the **wire shape** of the three ⚙ decoder rows is no longer
> deferred — it is fixed as a closed envelope over an open `fields` payload, with a `layout` descriptor
> making the server's assumption part of the answer. What remains deferred is the *implementation* side: a
> server may still detect its layout however it likes, and the declared field **catalogue** (offsets,
> widths, types) belongs to the planned `debug/struct_layout` op rather than to these rows. Deferring the
> shape was costing the successor a served surface the legacy has, on rows whose result the catalog
> described with a literal ellipsis and a phrase.

### 10.3 `contract/protocol.md` §8 — no new item

**This CR asks for no new conformance item, and says so deliberately.** Everything it requires is already
carried by items 15 (validate against the schema), 20 (close results), 22 (close params) and 23 (advertised
methods dispatch). Adding an item for a two-row surface would be item inflation; CR-A and CR-C each earned
theirs by creating an obligation no fragment could express, and this one does not.

### 10.4 `contract/protocol.md` §11 — new amendment section **§11.25**

Appended after §11.24 (which ends the file at line 4265). Title: *"the decoder rows get a shape: a closed
envelope, an open payload, and a server that says what it assumed."* Content: §3's argument, D1–D5, the
§9 departure table, and the adoption condition.

### 10.5 Schema — `contract/schema/bus-protocol.schema.json`

**New `$defs`:**

```json
"decoderLayout": {
  "type": "object",
  "required": ["engine", "detectedBy", "slotBytes", "slotCount", "baseAddr"],
  "properties": {
    "engine":       {"type": "string", "minLength": 1},
    "detectedBy":   {"type": "string", "minLength": 1},
    "detectedFrom": {"$ref": "#/$defs/symbolName"},
    "slotBytes":    {"type": "integer", "minimum": 1},
    "slotCount":    {"type": "integer", "minimum": 0},
    "baseAddr":     {"$ref": "#/$defs/hex"},
    "pools": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["name", "firstSlot", "slotCount"],
        "properties": {
          "name":      {"type": "string", "minLength": 1},
          "firstSlot": {"type": "integer", "minimum": 0},
          "slotCount": {"type": "integer", "minimum": 0}
        },
        "additionalProperties": false
      }
    }
  },
  "additionalProperties": false
},
"decodedSlot": {
  "type": "object",
  "required": ["slot", "addr", "x", "y", "code"],
  "properties": {
    "slot":     {"type": "integer", "minimum": 0},
    "addr":     {"$ref": "#/$defs/hex"},
    "x":        {"type": "integer"},
    "y":        {"type": "integer"},
    "code":     {"$ref": "#/$defs/hex"},
    "name":     {"$ref": "#/$defs/symbolName"},
    "nameDisp": {"type": "integer", "minimum": 0},
    "fields": {
      "type": "object",
      "additionalProperties": {"type": ["number", "string", "boolean"]}
    },
    "bytes":    {"$ref": "#/$defs/hex"}
  },
  "additionalProperties": false
}
```

Both item objects close themselves with `additionalProperties: false`, which is legal at these depths for
the reason `otherMatches.items[]` already does it: neither has an `allOf` for the keyword to be blind past.
`fields` is the one place left open, and it is open **by declaration**, with its value type pinned — the
`methodSummaries` shape.

**New fragments** (`methods["emulator/object_list"]`, `["emulator/player_state"]`, and — severably —
`["emulator/object_slot"]`), each with `params` carrying `unevaluatedProperties: false` per §8 item 22, and
each `result` an `allOf: [{"$ref": "#/$defs/replyFields"}]` with the keys and `required` sets of §6.1/§7.2/§8.2.
`player_state`'s item is `{"allOf": [{"$ref": "#/$defs/decodedSlot"}], "properties": {"active": …, "role": …}}` —
which, note, means the composed item is **not** closed by `decodedSlot`'s own
`additionalProperties: false`; the two extra keys must be declared in a schema that also re-states the
closure, or `decodedSlot` must be factored as an unclosed base. **§13.1 Q7 asks which.** This is exactly the
`additionalProperties`-is-blind-past-`allOf` trap §11.5 reproduced, arriving from a new direction, and it
is flagged rather than quietly resolved.

**One capability description amended:** `objectDecoders` gains §8.1's normative sentence as its
`description`. Its `{"type": "boolean"}` does not change.

**One new OPTIONAL `limits` key**, and it is new — flagged because it is easy to read §6.5's reference to
it as a citation of something that exists:

```json
"maxObjectSlots": {"type": "integer", "minimum": 1}
```

`handshake.initialize.result.limits` currently declares ten keys and requires three
(`maxRunFrames`, `maxReadLen`, `maxLineBytes`); this adds an eleventh, **optional**, on the
`maxProfilerRoutines` / `maxBreakpoints` precedent — *"THIS SERVER's ceilings, not the catalog's"*. It is
the ceiling `object_list.limit` is refused against. A server that applies no ceiling omits it, and
`object_list.limit` is then bounded only by `layout.slotCount`. Optional rather than required because a
decoder-less build has no such number and must not be made to invent one.

### 10.6 ⚠ Obligations this CR creates that **no fragment can express**

Four, listed because a schema green does not mean a server conforms:

1. **`fields` key provenance.** *A server MUST NOT emit a `fields` key its `layout.engine` does not name.*
   A typed-open map cannot express this; only prose plus a server's own test can.
2. **No decoded bit-name enums.** §7.3's rule is a rule about what a value may *mean*. A schema that
   allows a string value cannot forbid it from being a bit name.
3. **`layout` must describe the decode that produced *this* reply**, not a cached one. A server that
   computed `layout` at `load_symbols` time and stapled it on would validate perfectly and be wrong the
   moment the layout changed.
4. **The refusal ordering.** An unknown `fields` name must be refused **before** any decode, so a refused
   request has read nothing — §2.5's *"the refusal precedes any effect"*, applied one level down.

---

## 11. What this CR does and does not bind

- **It binds the wire shape of three rows** (two, if D5 is severed) and nothing else.
- **It does not bind the legacy C++ server.** No schedule is asked of it, per §11.23's treatment. Its
  current replies are non-conformant with these fragments and this CR **freezes rather than migrates**
  them, exactly as §11.21 did for the breakpoint family. If it is ever pointed at these fragments, §2.7's
  five findings are the work list.
- **It does not bind how a server detects a layout.** Symbol probe, config file, ROM hash — the contract
  cares only that the answer says which was used.
- **It does not create a field catalogue.** `layout` carries geometry (stride, counts, pools), not a list
  of field names with offsets. That is `debug/struct_layout`'s job when it lands
  (empyrean `ASSEMBLER_VISION.md:154`), and until then a client learns the legal `fields` names from its
  own knowledge of the engine — the same way it learns them today.
- **It does not rename `player_state`**, despite §2.6's name collision with a field in the very engine it
  serves. A rename would break the MCP shim's registered tool name (`oracle-old`'s
  `linux-port/mcp/oracle_mcp.py:709`) for a cosmetic gain, and D5 covers method names.
- **It does not touch the other five unschematized rows.** `call_stack`, `log_tail`, `z80_registers`,
  `read_vdp_registers`, `read_vsram` keep their BLOCKED status and their D-2x entries.
- **What this server would owe on adoption:** implement all three (or two), set
  `capabilities.objectDecoders: true`, and add them to `methods` (item 23's warranty makes those two moves
  a single act). The symbol machinery it builds on already exists — `Engine::symbol_at`
  (`engine.rs:1559`), `emulator/lookup_symbol` and `emulator/load_symbols` are all served today (§2.5(c)).

---

## 12. Where this CR is weakest

### 12.1 A peer consumer has already argued against this entire surface, and the argument is good

`aurora` — the only in-tree bus client — reviewed exactly this question a week ago and reached the opposite
conclusion (`aurora` `e5dd32a:docs/reviews/2026-08-22-oracle-instrument-gaps.md:307-315`, under its own
heading *"2.6 Live object/entity inspection — `read_memory` + Aurora's own decoder"* at `:305`), quoted in full
because summarising it would be advocacy:

> `capabilities.objectDecoders` is `false`, and `object_list` / `object_slot` / `player_state` /
> `call_stack` have **no contract fragment at all** (they are among the eight §6 rows left unschematized
> because their results are too loosely stated to transcribe without inventing).
>
> Aurora should not ask for them. It already decodes the SST itself — `build-run.ts` reads
> `Player_1 + $02` / `+ $06` and knows they are 16.16 fixed point. The object format is **Aurora's domain
> knowledge**; a server-side decoder would be a second copy of it that can drift while both halves look
> right, and Aurora would still have to keep its own for the editor. Read the bytes, decode locally.

**The drift objection is correct and this CR does not defeat it.** It narrows it. A server that carries
*offsets* (the legacy: `$02`, `$06`, `$18`, `$23` as C literals) holds a second copy of the layout that can
drift silently — aurora's exact complaint, and §2.4's `$52`→`$50` fold is the proof it drifts. What D1
changes is that the copy is now **declared on the wire**: a client that disagrees with `layout.slotBytes`
sees the disagreement in the reply instead of in wrong coordinates. That is drift made *visible*, not drift
eliminated, and an adjudicator is entitled to judge that insufficient.

**Where the two consumers genuinely differ.** aurora is an editor that must own the layout regardless.
aeon is a build-and-test lane whose gates want a witness *other than* their own arithmetic — the demand's
words are that the gates need *"'what is on screen' as a witness instead of raw `read_memory`"*. A second
independent decode is worth exactly nothing to aurora and is the entire point for aeon. This CR serves the
second and costs the first nothing, since aurora simply does not call the methods. But it is one consumer
for and one against, which is a thin mandate.

### 12.2 The open `fields` map is a hole, and calling it "declared" does not close it

§3.3 argues the openness is a published property rather than a gap. An adjudicator may reasonably answer
that item 20's purpose is to make *an unknown key on the wire a change request*, and that a map whose keys
are unbounded defeats that purpose wherever it appears — that the distinction between "a hole" and "a
declared hole" is one this contract has not previously drawn, and that `methodSummaries` is a weak
precedent because its key set is pinned to `methods` by a rule the server can be checked against, whereas
`fields`' key set is pinned to a layout nothing outside the server can enumerate.

That last clause is the sharpest form of the objection and this CR concedes it: **`layout` does not carry a
field catalogue**, so `fields`' key provenance rule (§10.6 item 1) is unverifiable from the wire. It
becomes verifiable the day `debug/struct_layout` exists. Until then it is prose backed by a server's own
tests, which is a weaker guarantee than this contract usually accepts.

### 12.3 It proposes six REQUIRED keys on rows that could have carried one

`objects`/`players`, `total`, `returned`, `limit`, `truncated`, `layout`, and five per-item keys are all
REQUIRED. §8 forbids the emulator side inventing, and this is a lot of invention arriving as MUST. The
defence is the pre-release window and §2.4 clause (a)'s *"required even when false"*, but the honest
statement is that a smaller REQUIRED set — say `objects`, `total`, `layout` — would also have worked and
would have left more room to be wrong.

### 12.4 `code` as a hex string is arguable

It is a word, and D9 category 2 covers *"counts, lengths, slot indices"*. A `code_addr` is none of those —
it is a raw datum whose width varies by layout (word here, byte on the other branch) — so category 1's
*"byte payloads are hex strings"* is the better fit, and a fixed spelling across layouts of different width
is worth something. But a reader could call it a number and not be obviously wrong, and the legacy emits it
as `addHex(…, 4)` on one row while emitting `render_flags` as a decimal `%u` on another, so the shipping
implementation is not a guide. §13.1 Q2.

### 12.5 One consumer, and its demand is explicitly non-blocking

The demand says *"Not blocking; wanted within the week if cheap."* This CR is not cheap: it is three
fragments, two `$defs`, a §6 rewrite, a §9 amendment and a §11 section. An adjudicator may reasonably rule
that a non-blocking want does not justify unblocking two of eight deliberately-blocked rows, and that the
right answer is the audit's *second* clause alone — surface `capabilities.objectDecoders` and a detect
result, and leave the rows blocked.

**That minimal ruling is coherent and this CR would not argue with it**, with one caveat: `objectDecoders`
cannot honestly be `true` on a server that serves no decoder, so the flag alone is not servable — it would
have to be `layout`-on-some-other-method, and there is no other method to put it on.

### 12.6 The SST layout was read from one commit of one engine

§2.4's table is `sst.emp` at `f4896139`. It is aeon's `master` lineage and the file pins its own offsets at
comptime, so it is about as trustworthy as a source read gets — but it is one engine, and the second
"engine" this CR keeps referring to (`sonic_hack`) was read only through the legacy C++ decoder's
constants plus one `S4.constants.asm` grep. A third engine could break assumptions this CR believes are
general — most likely `layout.pools`' contiguity, which nothing forces.

### 12.7 ⟨RUNTIME⟩ — nothing here was confirmed against a running server

Tagged for the controller's foreground follow-up. None was attempted; background agents must not touch the
emulator MCP.

1. ⟨RUNTIME⟩ **What the legacy server actually replies** for `object_list` and `player_state` on a live
   aeon ROM. §2.7 is read from C++ source only. In particular: whether `class` is ever non-empty in
   practice, and what `player_2` looks like in a one-player level.
2. ⟨RUNTIME⟩ **Whether `Object_RAM`, `ObjCodeBase`, `Dynamic_Slots`, `System_Slots`, `Effect_Slots` and
   `Object_RAM_End` all resolve** in `s4.debug.lst`. §2.4 shows they are *declared*; D1 assumes they
   *resolve*. If any does not, `layout.pools` is unbuildable for aeon and §5.1's optionality on `pools`
   becomes load-bearing rather than defensive.
3. ⟨RUNTIME⟩ **Whether `Player_1`'s address in a current build matches the demand's `$FF8DB0` or the
   committed fixture's neighbourhood** (§2.4). Either answer is fine; the discrepancy is the point, and
   confirming it live would make §2.4's argument measured rather than documentary.
4. ⟨RUNTIME⟩ **Whether any consumer's agent session actually calls these tools today**, which no source
   sweep can establish (§2.6). A transcript or MCP log would convert §2.6's *form* assertion into a count.

### 12.8 An unrelated finding, reported and not acted on

While enumerating `capabilities`, this CR found that this server emits **`romLoaded`**
(`crates/oracle-aether/src/engine.rs:1420`) and that the key appears **nowhere** in either artifact:
`grep -c romLoaded` on the vendored schema → `0`; on `protocol.md` at `39cfaa27` → no matches. It validates
only because `capabilities` is an open object. That is the same class as §11.11's *"three capabilities the
schema never knew we advertised"*, one instance later. **Out of scope here and deliberately not folded in**
— it belongs to whoever next opens the handshake — but it should not go quiet.

---

## 13. Questions for the adjudicator

### 13.1 Handed over undecided

**Q1 — Does the successor owe the legacy's decoder surface at cutover?** §7.5's recommendation to serve
`player_state` rests entirely on "declining removes a working surface a consumer uses". But this repo's
acceptance contract (`docs/OVERSEER.md` item 8) is built from the **schematized-and-unserved** set, and
these rows are by construction not in it. If cutover does not owe them, D3 should be **declined** and
clients pointed at `object_list` + `layout.pools[0]`. *This is the CR's most consequential open question
and it turns on a fact outside this CR's reach.*

**Q2 — Is `code` a hex string (D9 category 1) or a number (category 2)?** §12.4 states both readings. This
CR proposes category 1 because the datum's width varies by layout and it is a raw record value rather than
something to count with.

**Q3 — Keep `includeBytes`, or strike it and let clients use `emulator/read`?** Striking it is cleaner and
`read` already serves the need one slot at a time; keeping it saves 66 round trips for the "capture the
whole pool" case the demand asks for. Severable either way.

**Q4 — Is refusing `-32012` without symbols right, when the only shipping implementation falls back?**
§6.5 argues yes on `write_memory`'s "strict by design" precedent. Reversing it costs a `caveat` and a
`detectedBy: "fallback"` and nothing else.

**Q5 — `-32602` or `-32004` for a slot index past the pool?** §8.2 proposes `-32602` (it is a param, and
the fragment cannot bound it); the legacy answers `-32004`. Only reachable if D5 is adopted.

**Q6 — Pin item 20's closure scope.** §2.5(d) shows the reference harness applies `unevaluatedProperties`
only at the **top level of a result**, on a reading recorded in a code comment and nowhere in the contract.
This CR does not depend on that reading (§3.3 point 3), but a rule this load-bearing should be stated in
§8 item 20 rather than left to an implementation. Either reading works for these fragments; the ambiguity
is the defect.

**Q7 — How should `player_state`'s item compose `decodedSlot` with `active`/`role`?** §10.5 flags the trap:
`decodedSlot` closes itself with `additionalProperties: false`, which under an `allOf` composition does not
see the two added keys — the exact blindness §11.5 reproduced. Either (a) factor `decodedSlot` unclosed and
have each user restate the closure, or (b) duplicate the five core keys into the `player_state` item. (a)
is DRY and one refactor of a `$def` this CR is also introducing; (b) is dumber and cannot go wrong. This
CR leans (a) and would not argue with (b).

**Q8 — Should D5 (`object_slot`) travel with this CR at all?** No consumer asked. §8.2 argues family
consistency; the counter is scope discipline and that a CR should not schematize what nobody requested.

### 13.2 Considered settled, and not asked

Listed so the adjudicator can object to what was closed as well as to what was left open.

1. **A `fields` map beats an enumerated field list.** D-27 proposed it, §3.2's union finding makes an
   enumerated list describe a record that does not exist, and `methodSummaries` is the shipping precedent
   for a typed-open map. Settled.
2. **`layout` is REQUIRED and per-reply.** §5.2 (P2, the `binding` argument) and §5.3 (symbols load after
   the handshake, so a handshake value is stale by construction). Settled — and it is the one REQUIRED key
   this CR would defend at any cost.
3. **`objectDecoders` stays a boolean.** §8.1: changing a published key's JSON type is not additive, and
   the information an object would carry belongs on the reply anyway. Settled.
4. **No decoded bit-name enums, on any row.** §7.3: the names are invented, they already disagree across
   the legacy's two branches, and §11.18 makes them unwidenable. Settled — this is the one place this CR
   refuses a capability the legacy ships.
5. **`players` is an array, not `player_1`/`player_2` keys.** §7.4, and this repo's own transcription calls
   the alternative *"the biggest shape hazard in the eight"*. Settled.
6. **`name` is bare and round-trippable; absence is an omitted key, never `""`.** §4 is categorical and the
   legacy violates it two ways. Settled.
7. **No `cursor` on either row.** §2.4 clause (b): no continuation param, so no token. Settled.
8. **Both rows are pure reads and are not subject to §6's run-control state rule.** §6.5, on `read` /
   `sprites` / `pixel_attribution` / `scanlines`. Settled.
9. **`caveat` is declared on both and emitted conditionally.** §2.4 rule 4 requires the declaration; §2.4's
   advisory requires the conditionality. Settled.
10. **No new §8 conformance item.** §10.3. Settled.
11. **`player_state` is not renamed** despite colliding with a field name in the engine it serves (§2.6).
    Settled — D5, and a live MCP tool name.
12. **The legacy server is asked for nothing.** §11, on §11.23's precedent. Settled.

---

## 14. Provenance

**Written by:** the oracle lane, 2026-08-26, as a docs-only parcel on branch `cr-d-object-decoders`, based
on `oracle` `main` at `0d7c5c21c2e92458d55de5d4a062e08d2532d610`.

**Anchors, with the class of each stated:**

| anchor | class | what it vouches for |
|---|---|---|
| empyrean `39cfaa27c293510d583581b5b07d07709691508a` | repo tip (docs commit: *"hub: rebooted after /clear…"*) | the **tree** at which `contract/protocol.md` (blob `b4776ce9`) and the schema (blob `7b24bced`) were read |
| oracle `0d7c5c21c2e92458d55de5d4a062e08d2532d610` | code+docs merge lineage | this repo's source, vendored schema and prior docs |
| aeon `f48961396848de666a737971bbb7b1c627a90f78` | **docs commit** (`docs/lane-log.jsonl`, +1) | the **tree** at which `sst.emp`/`core.emp`/`ram.emp`/`constants.emp`/`player_common.emp` were read; verified an ancestor of `origin/master` |
| aeon `b87e6e5a53d1e4ec45f2bdf614c663d5025e0eb7` | working-tree HEAD | the consumer sweep only |
| aurora `e5dd32a56d4b4e11b5c28b614d14bba47bfdfd86` | working-tree HEAD | §12.1's quoted review |
| oracle-old `9dc67c5bb4e85b4c70e80e8a5b198f00d824877e` | working-tree HEAD | §2.7's legacy source |
| sigil `e7f596eb436c537c7cd27e9b3120b38fed31c4c6` | working-tree HEAD | consumer sweep only |
| seraph `2c8dc882aaddd4cf13618e1351f8c15bda002585` | working-tree HEAD | consumer sweep only (zero hits) |
| sonic_hack `858af72c50083fa9e721ac1ecd69095022d3659e` | working-tree HEAD | §2.3's retracted-hypothesis check |

Every sibling file was read via `git show <rev>:<path>` or `git grep <rev>`, never through the sibling
directory path — each of those is a peer's live working tree, and a path read measures somebody's
uncommitted mid-edit state while returning a clean confident answer.

**Method:** documentary and source-reading only. No `cargo`, no emulator, no MCP tool. Every count in §2.5
and §2.6 was produced by a command whose enumeration parameter is named beside it, and whose output is
reproduced rather than summarised.
