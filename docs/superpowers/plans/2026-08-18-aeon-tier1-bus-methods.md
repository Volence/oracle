# Aeon Tier 1 Bus Methods Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `emulator/write_memory`, `emulator/reset`, and `emulator/memory_hash` on the Aether
bus, contract-first, closing Tier 1 of `docs/2026-08-17-aeon-switchover-gap-list.md` so the Aeon
side's `ab_runner` can re-point at oracle-next.

**Architecture:** Four surfaces per method, per the owner's three-surface-parity rule — §6 contract
row → Aether handler → MCP tool row → player GUI decision. The contract amendment (§11.13, CR-21/22/23)
lands in `empyrean` FIRST, the schema is re-vendored, and only then do handlers land (the measured
discipline: of 21 methods audited, only the ones specified before implementation had zero
undocumented result keys). **All three MCP tool rows already exist** (legacy Oracle tools) — the MCP
work is verification + two small fixes, not new rows. No new player GUI in this slice: `reset`
already exists on Tab/F1, `write_memory`'s memory editor is deferred by design, `memory_hash` has no
natural surface (decision recorded in the handoff).

**Tech Stack:** Rust (oracle-aether handlers + tests, `jsonschema` validation harness), Markdown +
JSON Schema (empyrean contract), Python (oracle repo MCP, small edits only).

**Repos touched:** `oracle-next` (worktree `.worktrees/player-s1-palette`, branch `aeon-tier1`),
`empyrean` (has remote — push at end), `oracle` (NO remote — commit locally only).

**Out of scope, registered:** the sprite link walk's §6 row (its defer-trigger has fired — the S3
lens renders it — but its shape needs its own CR; register as F-WALK-ROW in the handoff, do not
bolt it into this slice). Also F-HOSTED-RESET-SRM (see Task 6 notes).

---

## Design pins (already ruled or decided; the CR document argues them, the adjudication checks them)

1. **`write_memory` window = work RAM `$E00000–$FFFFFF`** (the mirror window `debug_read` already
   serves), not the row's legacy `$FF0000–$FFFFFF`: an address you can `read` you can write, and
   watch hits can report mirror addresses. Refusal `-32004`, refused-never-clipped, ROM/IO refused.
2. **`write_memory` requires a paused machine** (`-32005`, `data.reason="machineRunning"`), named in
   §6's run-control state rule. Strict-first: relaxing a refusal later is additive (D5); adding one
   breaks clients.
3. **Exactly one payload spelling**: `bytes` (hex string, even digits, `parse_bytes`) XOR
   `value`+`width` (width ∈ {1,2,4}, value must fit, big-endian as the 68000 stores). Both, neither,
   `width` with `bytes`, or `value` without `width` → `-32602`. Cap `len ≤ limits.maxWriteLen` (4096,
   mirroring `maxReadLen`).
4. **Writes go through the bus path** — `self.sys.mega_bus(&mut ()).write8(...)` with FC=5
   (supervisor data), byte at a time. Hardware mirror masking for free; no `ram_mut()` added to core.
5. **`reset` is NOT run-state gated** — the checkpoint-block precedent (`engine.rs:2017-2028`): it
   advances nothing and replaces state wholesale between frames. Result `deferred: bool` defined
   honestly: `false` = applied before the reply (our server, always), `true` = queued for the next
   frame boundary (Oracle's behaviour while free-running). Both conformant.
6. **`reset` semantics on the wire**: master clock restarts at 0 (stamps jump backwards — prose
   note); SRAM contents survive; loaded symbols survive (image unchanged — unlike `reload_rom`);
   checkpoints and watchpoints survive; held pads clear; `rom_generation` bumps (the `restore`
   precedent, so a hosted player resyncs).
7. **`memory_hash` is a pure read** (not gated, answers free-running). Params `addr`|`symbol` +
   **required** `len` (1..=`limits.maxHashLen` = 4194304). Routes exactly like `debug_read` (work-RAM
   window or cart ROM, `-32004` past the end). Result: `addr`, `len`, `region`, `fnv1a64`
   (`$defs/hash64`), `crc32` (new `$defs/hash32`, IEEE/zlib so a cart-window hash matches CRC32 over
   the ROM-file slice — the legacy MCP row promises exactly this).
8. **`memory_hash` ≠ the deferred `frame_hash`** (§9.1): that is a picture hash and
   `state_hash includeFramebuffer` already serves it; `memory_hash` fills the gap `state_hash`
   leaves (its five fingerprints cover VDP state only — nothing today hashes 68000 memory).
9. **CRC-32 lives in oracle-aether** (`src/crc32.rs`, const-table, dep-free), NOT in oracle-core's
   `state_hash.rs` (that module is Oracle-byte-compat FNV territory with a do-not-touch warning).
10. Schema fragment count moves **29 → 32**; `initialize`'s `limits` fragment gains `maxWriteLen` +
    `maxHashLen` (the vendored-schema closure on `initialize` would otherwise reject the handshake).

## House rules (from the S1–S3 record — binding)

- **Mutation-verify every evidence-bearing test at writing time**, one recorded line each.
  Assertions must compare two *independently derived* values — never bind measured output to the
  expected name (`assert_eq!(got, got)` passed a green suite once; clippy caught it, not the test).
- **Never `cargo test | tail`** (hides failures, wrong exit code). Redirect to a log file and grep.
- **Never run `cargo test --workspace` from two trees at once.**
- Gates per task: `cargo fmt --all -- --check`, `cargo clippy --all-targets --workspace -- -D warnings`,
  same with `--no-default-features`, `cargo test --workspace`. fmt is a HARD commit gate.
- `touch` any file after scripted writes/reverts and confirm a "Compiling" line — cargo's
  fingerprint is mtime-based.

---

### Task 0: Base check

**Files:** none (verification only). Working dir for ALL oracle-next tasks:
`/home/volence/sonic_hacks/oracle-next/.worktrees/player-s1-palette` (branch `aeon-tier1`).

- [ ] **Step 1: Verify the base** (every brief opens with this — 6-for-6 agents once got a stale worktree):

```bash
cd /home/volence/sonic_hacks/oracle-next/.worktrees/player-s1-palette
git log -1 --oneline   # MUST contain: "sh_probe grows save-state restore"
test -f docs/2026-08-17-aeon-switchover-gap-list.md && echo BASE-OK
git status --short     # MUST be empty (plan file itself excepted)
ls vendor/TestRoms | head -3   # vendor must resolve
```

Expected: `777eef3 chore(examples): sh_probe grows save-state restore, pad journeys, and the raster-state dump`, `BASE-OK`, vendor listing non-empty. **If any check fails, STOP and report — do not "fix" the worktree silently.**

- [ ] **Step 2: Commit this plan file**

```bash
git add docs/superpowers/plans/2026-08-18-aeon-tier1-bus-methods.md
git commit -m "docs: Tier 1 plan — write_memory, reset, memory_hash, contract-first"
```

---

### Task 1: CR-21/22/23 document

**Files:**
- Create: `docs/2026-08-18-cr21-23-tier1-rows.md` (oracle-next worktree)
- Modify: `docs/2026-08-14-aether-change-requests.md` (append three register entries)

- [ ] **Step 1: Write the CR document.** One document, three CRs (the §11.5 multi-CR-entry
  precedent). Structure per CR: *What exists today* (with the protocol.md line anchors below),
  *The proposed row + prose + fragment* (copy VERBATIM from Task 3's step contents so the
  adjudicator reads exactly what will land), *The demand evidence*, *Open questions: none — pins
  1–10 above, each with its reasoning*. Required content per CR:
  - **CR-21 `write_memory`**: row exists at `protocol.md:825`, legacy vintage — no prose, no
    fragment, untyped `width`, no refusal code, silent on the run-control rule. Owner ruling
    2026-08-17 (gap-list doc §"Assessment") already ADOPTED it scoped: keep-dead covers register
    writes only. Demand: three committed Aeon scenes poke via `mega_bus().write8`; the MCP row
    (`oracle_mcp.py:304-316`) has declared `bytes`|`value`+`width` for years. Argue pins 1–4.
  - **CR-22 `reset`**: row exists at `protocol.md:783` with result `deferred` — never defined.
    Oracle serves it (`ControlSocket.cpp` emits `deferred` true/false); our books call `reset` "the
    conspicuous absence from a 25-method control surface"; the player has had it on Tab/F1 the whole
    time (a live D15 parity gap). Argue pins 5–6, especially why the row must define `deferred` so
    both servers stay conformant.
  - **CR-23 `memory_hash`**: NO row anywhere in empyrean (genuinely new — §8's ban on unadvertised
    ops applies, hence contract-first). De-facto spec = the legacy MCP row (`oracle_mcp.py:238-254`):
    fnv1a64 + crc32, no 4096 cap, RAM/ROM auto-route, refuse-never-clip. Argue pins 7–9, and the
    §9.1 `frame_hash` distinction explicitly (a CR that ignores a standing deferral invites the
    momentum-refund failure).
  - Close with the fragment-count delta (29 → 32) and the adoption condition: *registered when a
    conformant reply passes each fragment closed — happy path plus one refusal per bound per method.*
- [ ] **Step 2: Append register entries.** In `docs/2026-08-14-aether-change-requests.md`, after the
  last entry, add three rows in the file's existing format: `CR-21 write_memory (proposed
  2026-08-18, doc 2026-08-18-cr21-23-tier1-rows.md)`, same for CR-22, CR-23. Note in CR-21's entry
  that it supersedes the file's own `write_memory — read-only for now` deferral (around line 984)
  by the 2026-08-17 owner ruling; leave the `write_vram`/`poke_vram` landmine note intact — it is
  not covered by this slice.
- [ ] **Step 3: Commit**

```bash
git add docs/2026-08-18-cr21-23-tier1-rows.md docs/2026-08-14-aether-change-requests.md
git commit -m "docs: CR-21/22/23 — the Tier 1 rows (write_memory, reset, memory_hash)"
```

---

### Task 2: Fable adjudication of the CRs

**Files:**
- Create: `docs/2026-08-18-ruling-cr21-23.md`

- [ ] **Step 1: Dispatch an un-framed adjudication agent** (the standing mechanism for contract
  rulings — it has caught a fabricated contract quote, a CR-number collision, and wrong provenance
  claims). Agent tool, `model: "fable"`, fresh context. The brief must be NEUTRAL — do not tell it
  the answer you want, do not call the pins "settled". Give it: the CR doc path, `empyrean/contract/protocol.md`,
  `empyrean/contract/schema/bus-protocol.schema.json`, `docs/2026-08-17-aeon-switchover-gap-list.md`,
  and ask: *"Rule on CR-21/22/23: adopt, adopt-with-changes, or reject, each. Verify every factual
  claim against the named files — line anchors, quoted contract text, claimed precedents. List
  required changes with reasons. Pay particular attention to: bound values against the catalog
  (three prior CRs each narrowed or misquoted one), whether each named precedent actually says what
  the CR claims, and whether any result key is unregistered or any registered key unreachable."*
- [ ] **Step 2: Write the ruling document** from the agent's report: verdict per CR, required
  changes, corrections to the CR's evidence (verbatim, so the record shows what was wrong).
- [ ] **Step 3: Apply required changes to the CR doc and Task 3's planned text.** If the ruling
  changes a design pin, update this plan file's pin list in the same commit and say so in the
  commit message. If a ruling REJECTS a CR, STOP — report to the owner rather than proceeding.
- [ ] **Step 4: Commit**

```bash
git add docs/2026-08-18-ruling-cr21-23.md docs/2026-08-18-cr21-23-tier1-rows.md
git commit -m "docs: ruling on CR-21/22/23 — applied"
```

---

### Task 3: The empyrean amendment (§11.13 + schema fragments)

**Files (all in `/home/volence/sonic_hacks/empyrean`):**
- Modify: `contract/protocol.md` (rows :783 and :825, run-control rule :793-806, memory-table
  prose, new memory_hash row, §11.13 at end of §11)
- Modify: `contract/schema/bus-protocol.schema.json` (`$defs/hash32`, three method fragments,
  `initialize.limits` two keys, top-level `description` count)

> Text below is the pre-ruling draft; apply Task 2's required changes before landing.
>
> NOTE: Task 3's draft text below is SUPERSEDED — land the post-ruling text from
> `docs/2026-08-18-cr21-23-tier1-rows.md` as amended, per `docs/2026-08-18-ruling-cr21-23.md`.

- [ ] **Step 1: Amend the `reset` row's surroundings.** The row at `protocol.md:783`
  (`| \`emulator/reset\` | — | \`deferred\` |`) stays as-is. In the run-control table's blockquote
  region (after the run-control state rule, before the checkpoints group), add:

```markdown
> **`emulator/reset`** drives the machine's `/RESET` sequence against the current cartridge.
> Deliberately **not** subject to the state rule above, for the checkpoint methods' reason: it
> advances nothing and replaces state wholesale between frames, so it cannot fight a free-run loop.
> `deferred` reports *when* it landed — `false`: applied before this reply; `true`: queued for the
> next frame boundary. Both are conformant; a server answers with the one its threading makes
> honest, and a client that needs the reset visible waits for a reply with `deferred: false` or one
> subsequent frame. The master clock restarts from 0, so §2.2 stamps on later replies jump
> backwards — that is the reset, not a bug. Battery SRAM contents survive (as on real hardware);
> loaded symbols survive (the image is unchanged — contrast `reload_rom`); checkpoints and
> watchpoints survive; held pads clear. Can time out (`-32010`).
```

- [ ] **Step 2: Amend the `write_memory` row.** Replace `protocol.md:825` with:

```markdown
| `emulator/write_memory` ← `write` | `addr`\|`symbol`; `bytes`\|(`value`+`width` 1\|2\|4); `len` ≤ `limits.maxWriteLen` | `addr`, `len` |
```

  and add to the memory table's blockquote (alongside the `emulator/read` prose):

```markdown
> - **`emulator/write_memory`** is the poke primitive. Scope: the work-RAM window
>   `$E00000–$FFFFFF` (mirror-masked, exactly the window `read`'s `bus` space serves) — a base
>   outside it, a range whose **end** runs past it, or any ROM/IO target is `-32004`, **refused,
>   never clipped**. Exactly one payload spelling: `bytes` (hex string, even digit count) or
>   `value`+`width` (`width` ∈ 1|2|4; `value` must fit `width`); both, neither, `width` with
>   `bytes`, or `value` without `width` is `-32602`. Multi-byte values land **big-endian**, as the
>   68000 stores. Writes travel the bus path (hardware mirror masking), not a buffer poke.
>   Requires a **paused** machine (see the run-control state rule) — strict by design: relaxing a
>   refusal later is additive (D5); introducing one is not.
```

- [ ] **Step 3: Name `write_memory` in the run-control state rule.** In `protocol.md:793-806`,
  extend the named list: `run_to`, `run_to_scanline`, `run_frames`, `step*`, `press`, `play_input`,
  `reload_rom` **and `write_memory`**, and append one sentence to the *why*: *"`write_memory` is
  named for `press`'s reason — a poke mid-free-run mutates the timeline just as surely, and leaving
  it unnamed would let one server refuse and another accept, both conforming."*
- [ ] **Step 4: Add the `memory_hash` row.** In the memory table, directly under `emulator/read`:

```markdown
| `emulator/memory_hash` | `addr`\|`symbol`, `len` (required, ≤ `limits.maxHashLen`) | `addr`, `len`, `region`, `fnv1a64`, `crc32` |
```

  and its blockquote entry:

```markdown
> - **`emulator/memory_hash`** fingerprints a byte range without moving it — no byte payload
>   crosses the wire, so large ranges are cheap where `read` caps at `maxReadLen`. A **pure read**
>   (not subject to the run-control state rule). Routes exactly as `read`'s `bus` space: work-RAM
>   window or cartridge ROM, `-32004` past the end, refused never clipped. `fnv1a64` is
>   `state_hash`'s family (16-hex, `$defs/hash64`); `crc32` is IEEE/zlib CRC-32 (8-hex,
>   `$defs/hash32`), chosen so a cartridge-window hash equals CRC32 over the same slice of the ROM
>   file. Fills the gap `state_hash` leaves — its five fingerprints cover VDP state only; nothing
>   else on this bus hashes 68000 memory. Distinct from §9's deferred `frame_hash`: that is a
>   picture hash, already served by `state_hash`'s `includeFramebuffer`.
```

- [ ] **Step 5: Schema — `$defs/hash32`.** In `bus-protocol.schema.json`'s `$defs`, after `hash64`:

```json
"hash32": {
  "type": "string",
  "pattern": "^0x[0-9A-Fa-f]{8}$",
  "description": "A CRC-32 (IEEE 802.3 / zlib polynomial) fingerprint, spelled as a hex string of exactly 8 digits (D9 category 1). Fixed width for hash64's reason: a fingerprint is compared, never computed on, and a 7-digit answer is a dropped leading zero."
},
```

- [ ] **Step 6: Schema — the three method fragments.** Add under `methods` (alphabetical-adjacent
  placement matching the file's existing grouping):

```json
"emulator/write_memory": {
  "$comment": "protocol.md §6 (memory), specified 2026-08-18 by §11.13 (CR-21). The poke primitive. Work-RAM window $E00000-$FFFFFF only, refused (-32004) never clipped; exactly one payload spelling (bytes XOR value+width, else -32602); requires a paused machine per the §6 run-control state rule (-32005 machineRunning). Values land big-endian. The keep-dead 'register-write op' entry was ruled (2026-08-17) to cover register writes only.",
  "params": {
    "type": "object",
    "properties": {
      "addr": { "$ref": "#/$defs/hex", "description": "First byte to write. D9 category 1." },
      "symbol": { "allOf": [{ "$ref": "#/$defs/symbolName" }], "description": "Resolved to an address (D7); alternative to addr." },
      "bytes": { "$ref": "#/$defs/hex", "description": "Byte payload, even digit count. Alternative to value+width; passing both is -32602." },
      "value": { "type": "integer", "minimum": 0, "maximum": 4294967295, "description": "Numeric payload; must fit width. Requires width." },
      "width": { "enum": [1, 2, 4], "description": "Byte width of value. Valid only with value." }
    }
  },
  "result": {
    "allOf": [{ "$ref": "#/$defs/replyFields" }],
    "required": ["addr", "len"],
    "properties": {
      "addr": { "$ref": "#/$defs/hex", "description": "Echoed resolved base address." },
      "len": { "type": "integer", "minimum": 1, "maximum": 4096, "description": "Bytes written. Bounded by limits.maxWriteLen." },
      "caveat": { "type": "string", "description": "protocol.md §2.4. Conditional, never constant." }
    }
  }
},
"emulator/reset": {
  "$comment": "protocol.md §6 (run-control), result defined 2026-08-18 by §11.13 (CR-22) — the row predates the schema and its `deferred` key was never defined. NOT subject to the run-control state rule (the checkpoint precedent: replaces wholesale, advances nothing). The master clock restarts at 0, so stamps on subsequent replies jump backwards by design.",
  "params": { "type": "object" },
  "result": {
    "allOf": [{ "$ref": "#/$defs/replyFields" }],
    "required": ["deferred"],
    "properties": {
      "deferred": { "type": "boolean", "description": "false: the reset was applied before this reply. true: it is queued for the next frame boundary. Both conformant — a server answers with the one its threading makes honest." }
    }
  }
},
"emulator/memory_hash": {
  "$comment": "protocol.md §6 (memory), added 2026-08-18 by §11.13 (CR-23). Fingerprints a byte range without moving it — the gap state_hash leaves (its five hashes cover VDP state only). A pure read: not run-state gated. Routes as read's bus space; -32004 past the end, refused never clipped. Distinct from §9's deferred frame_hash (a picture hash, served by state_hash includeFramebuffer).",
  "params": {
    "type": "object",
    "required": ["len"],
    "properties": {
      "addr": { "$ref": "#/$defs/hex", "description": "First byte to hash. D9 category 1." },
      "symbol": { "allOf": [{ "$ref": "#/$defs/symbolName" }], "description": "Resolved to an address (D7); alternative to addr." },
      "len": { "type": "integer", "minimum": 1, "maximum": 4194304, "description": "Bytes to hash. REQUIRED — a hash without a length hashes nothing. Bounded by limits.maxHashLen." }
    }
  },
  "result": {
    "allOf": [{ "$ref": "#/$defs/replyFields" }],
    "required": ["addr", "len", "region", "fnv1a64", "crc32"],
    "properties": {
      "addr": { "$ref": "#/$defs/hex" },
      "len": { "type": "integer", "minimum": 1, "maximum": 4194304 },
      "region": { "type": "string", "description": "Which region of the 68000 map the range landed in (work RAM / cartridge ROM)." },
      "fnv1a64": { "$ref": "#/$defs/hash64" },
      "crc32": { "$ref": "#/$defs/hash32", "description": "IEEE/zlib CRC-32, so a cartridge-window hash equals CRC32 over the same slice of the ROM file." },
      "caveat": { "type": "string", "description": "protocol.md §2.4. Conditional, never constant." }
    }
  }
},
```

- [ ] **Step 7: Schema — `initialize.limits` + the count.** In the `initialize` fragment's `limits`
  properties (which today hold `maxRunFrames`, `maxReadLen`, `maxLineBytes`, `maxInputRows`), add:

```json
"maxWriteLen": { "type": "integer", "description": "Byte ceiling for one emulator/write_memory payload." },
"maxHashLen": { "type": "integer", "description": "Byte ceiling for one emulator/memory_hash range." }
```

  Update the schema's top-level `description` fragment count (currently "23 of §6's ~60" — recount
  after adding three; it becomes e.g. "26 of §6's ~60"; verify the real starting number by counting
  `methods` keys with a `result`, do not trust the stale text).
- [ ] **Step 8: Write §11.13.** Append to `protocol.md` §11, in the §11.12 shape (framing paragraph;
  `| Item | The defect | What this amendment changed |` table with one row per CR; `★` paragraphs
  for the normative findings — at minimum: `deferred` was a key with no definition shared by two
  servers, the run-control list gains its first non-advancing-but-gated member and why, and the
  `frame_hash` non-overlap; closing *Adoption condition* line: *registered when a conformant reply
  passes each of the three fragments closed — happy path plus one refusal per catalogued bound.*
  Include the fragment delta (`29 → 32`, corrected to real counts from Step 7).
- [ ] **Step 9: Validate the schema is still valid JSON and self-consistent**

```bash
cd /home/volence/sonic_hacks/empyrean
python3 -c "import json; json.load(open('contract/schema/bus-protocol.schema.json')); print('JSON OK')"
```

- [ ] **Step 10: Commit (empyrean)**

```bash
git -C /home/volence/sonic_hacks/empyrean add contract/protocol.md contract/schema/bus-protocol.schema.json
git -C /home/volence/sonic_hacks/empyrean commit -m "contract: §11.13 — the Tier 1 rows: a poke, a reset with a defined answer, and the hash state_hash cannot give"
```

---

### Task 4: Re-vendor the schema into oracle-next

**Files (oracle-next worktree):**
- Modify: `crates/oracle-aether/tests/contract/bus-protocol.schema.json` (byte-copy)
- Modify: `crates/oracle-aether/tests/contract/PROVENANCE.md` (table — note it is ALREADY stale:
  it records `34a1993`/89562 bytes while the vendored file matches current upstream at 103086
  bytes; fix the record while updating it)

- [ ] **Step 1: Copy + record**

```bash
cd /home/volence/sonic_hacks/oracle-next/.worktrees/player-s1-palette
cp /home/volence/sonic_hacks/empyrean/contract/schema/bus-protocol.schema.json \
   crates/oracle-aether/tests/contract/bus-protocol.schema.json
sha256sum crates/oracle-aether/tests/contract/bus-protocol.schema.json
git -C /home/volence/sonic_hacks/empyrean log -1 --format='%H %s' -- contract/schema/bus-protocol.schema.json
```

- [ ] **Step 2: Update PROVENANCE.md's table** with the new commit hash, sha256, byte count, and
  today's date. Add one line noting the previous table row had rotted (recorded `34a1993` while the
  file tracked later commits) — the record should show the record failed, per house style.
- [ ] **Step 3: Run the conformance suite** — the three new fragments are `schema_only` (schematized,
  not yet advertised), which is the harmless bucket; `UNCOVERED_METHODS` stays `&[]` untouched.

```bash
cargo test -p oracle-aether > /tmp/claude-1000/tier1-t4.log 2>&1; echo "EXIT=$?"; grep -E "test result|FAILED" /tmp/claude-1000/tier1-t4.log
```

Expected: EXIT=0, all legs pass.
- [ ] **Step 4: Commit**

```bash
git add crates/oracle-aether/tests/contract/
git commit -m "chore(aether): re-vendor the contract schema at §11.13 (and un-rot the PROVENANCE table)"
```

---

### Task 5: `emulator/write_memory` handler (TDD)

**Files (oracle-next worktree):**
- Create: `crates/oracle-aether/tests/write_memory.rs`
- Modify: `crates/oracle-aether/src/engine.rs` (METHODS entry after the `reload_rom` entry
  ~`:270`; handler near `read_memory` ~`:1211`; `FC_SUPERVISOR_DATA` const near the other consts
  ~`:49`; `EngineConfig` gains `max_write_len`; `limits` block ~`:827` gains `maxWriteLen`)

- [ ] **Step 1: Write the failing test file**

```rust
//! `emulator/write_memory` — `protocol.md` §6 (memory), adopted as CR-21
//! (`docs/2026-08-18-cr21-23-tier1-rows.md`, ruled in `docs/2026-08-18-ruling-cr21-23.md`, §11.13).
//!
//! Every reply is validated against the vendored schema on the way past. The adoption condition is
//! the shape of the file: closed happy path per payload spelling, plus one refusal per bound.

mod common;

use common::{spawn_system, Client};
use oracle_core::system::System;
use serde_json::json;

fn machine() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    sys
}

fn client(h: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(h);
    c.handshake(false);
    c
}

/// Happy path, `bytes` spelling — and the write is verified through the INDEPENDENT read path.
#[test]
fn bytes_land_in_ram_and_read_back() {
    let h = spawn_system("wm-bytes", machine(), 64);
    let mut c = client(&h);
    let r = c.ok(
        "emulator/write_memory",
        json!({"addr": "0xFF0100", "bytes": "0xDEADBEEF"}),
    );
    assert_eq!(r["addr"], json!("0x00FF0100"));
    assert_eq!(r["len"], json!(4));
    let back = c.ok("emulator/read", json!({"addr": "0xFF0100", "len": 4}));
    assert_eq!(back["bytes"], json!("0xDEADBEEF"), "read back what was written");
}

/// Happy path, `value`+`width` spelling — big-endian, as the 68000 stores.
#[test]
fn value_width_is_big_endian() {
    let h = spawn_system("wm-value", machine(), 64);
    let mut c = client(&h);
    for (width, value, expect) in [
        (1, 0xAB_u32, "0xAB"),
        (2, 0x1234, "0x1234"),
        (4, 0xCAFE_F00D, "0xCAFEF00D"),
    ] {
        let r = c.ok(
            "emulator/write_memory",
            json!({"addr": "0xFF0200", "value": value, "width": width}),
        );
        assert_eq!(r["len"], json!(width));
        let back = c.ok("emulator/read", json!({"addr": "0xFF0200", "len": width}));
        assert_eq!(back["bytes"], json!(expect), "width {width}: big-endian bytes");
    }
}

/// The mirror window: an address in `$E00000` aliases the same RAM cell `$FF0000` sees.
#[test]
fn the_mirror_window_is_writable_and_aliases() {
    let h = spawn_system("wm-mirror", machine(), 64);
    let mut c = client(&h);
    c.ok("emulator/write_memory", json!({"addr": "0xE00300", "bytes": "0x5A"}));
    let back = c.ok("emulator/read", json!({"addr": "0xFF0300", "len": 1}));
    assert_eq!(back["bytes"], json!("0x5A"), "$E00300 and $FF0300 are the same cell");
}

/// Exactly one payload spelling — all four wrong shapes are -32602, before any write happens.
#[test]
fn payload_spelling_is_exactly_one_of_two() {
    let h = spawn_system("wm-spelling", machine(), 64);
    let mut c = client(&h);
    for bad in [
        json!({"addr": "0xFF0000", "bytes": "0x00", "value": 1, "width": 1}), // both
        json!({"addr": "0xFF0000"}),                                          // neither
        json!({"addr": "0xFF0000", "bytes": "0x00", "width": 1}),             // width with bytes
        json!({"addr": "0xFF0000", "value": 5}),                              // value sans width
        json!({"addr": "0xFF0000", "value": 256, "width": 1}),                // value overflows width
        json!({"addr": "0xFF0000", "bytes": "0xABC"}),                        // odd digit count
        json!({"addr": "0xFF0000", "bytes": "0x"}),                           // empty payload
    ] {
        let e = c.err("emulator/write_memory", bad.clone());
        assert_eq!(e["code"], json!(-32602), "refused: {bad}");
    }
    // Nothing above wrote anything:
    let back = c.ok("emulator/read", json!({"addr": "0xFF0000", "len": 1}));
    assert_ne!(back["bytes"], json!("0x5A"), "sanity: probe cell untouched by refusals");
}

/// ROM and out-of-window targets are -32004 — refused, never clipped, and the end bound counts.
#[test]
fn rom_and_out_of_window_are_refused() {
    let h = spawn_system("wm-bounds", machine(), 64);
    let mut c = client(&h);
    for (addr, why) in [
        ("0x00000100", "ROM"),
        ("0x00400000", "unmapped"),
        ("0x00FFFFFF", "end runs past the window"), // len 2 below
    ] {
        let params = if why == "end runs past the window" {
            json!({"addr": addr, "bytes": "0x0102"})
        } else {
            json!({"addr": addr, "bytes": "0x01"})
        };
        let e = c.err("emulator/write_memory", params);
        assert_eq!(e["code"], json!(-32004), "{why} refused");
    }
    // The last legal byte IS writable:
    let r = c.ok("emulator/write_memory", json!({"addr": "0x00FFFFFF", "bytes": "0x7E"}));
    assert_eq!(r["len"], json!(1));
}

/// Run-state gated: -32005 machineRunning while free-running (named in §6's run-control rule).
#[test]
fn a_free_running_machine_refuses_the_poke() {
    let h = spawn_system("wm-gate", machine(), 64);
    let mut c = client(&h);
    c.ok("emulator/resume", json!({}));
    let e = c.err("emulator/write_memory", json!({"addr": "0xFF0000", "bytes": "0x00"}));
    assert_eq!(e["code"], json!(-32005));
    assert_eq!(e["data"]["reason"], json!("machineRunning"));
}

/// Symbol addressing works (D7) and a bad symbol is -32013 / no table is -32012.
#[test]
fn symbol_paths_resolve_and_refuse() {
    let h = spawn_system("wm-sym", machine(), 64);
    let mut c = client(&h);
    let e = c.err("emulator/write_memory", json!({"symbol": "NoSuch", "bytes": "0x00"}));
    assert_eq!(e["code"], json!(-32012), "no table loaded yet");
}

/// §8 item 20 closure, asserted locally: the success key set is exact.
#[test]
fn the_key_set_is_exact() {
    use std::collections::BTreeSet;
    let h = spawn_system("wm-keys", machine(), 64);
    let mut c = client(&h);
    let r = c.ok("emulator/write_memory", json!({"addr": "0xFF0400", "bytes": "0x00"}));
    let got: BTreeSet<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
    let want: BTreeSet<&str> =
        ["addr", "len", "frame", "mclk", "running", "droppedEvents"].into_iter().collect();
    assert_eq!(got, want, "no surplus keys, no constant caveat");
}
```

- [ ] **Step 2: Run it — must fail with `-32601` (method not found)**

```bash
cargo test -p oracle-aether --test write_memory > /tmp/claude-1000/tier1-t5a.log 2>&1; echo "EXIT=$?"; grep -E "test result|panicked" /tmp/claude-1000/tier1-t5a.log | head
```

- [ ] **Step 3: Implement.** In `engine.rs`:

Near the other consts (~line 49):

```rust
/// Function code for the debug poke path: supervisor data, matching what the replay runner arms with.
const FC_SUPERVISOR_DATA: u8 = 5;
```

`EngineConfig` (find it in `engine.rs`; it already carries `max_read_len`): add
`pub max_write_len: u64,` defaulting to `4096`, and `pub max_hash_len: u64,` defaulting to
`4_194_304` (added here once so Task 7 doesn't touch the struct again). In the `limits` JSON block
(~`:827-832`) add `"maxWriteLen": self.config.max_write_len, "maxHashLen": self.config.max_hash_len,`
(spelling per the schema fragment).

The handler, placed after `read_memory`:

```rust
    /// `emulator/write_memory` — the poke primitive (§6 memory, CR-21 / §11.13).
    ///
    /// Work-RAM window only, refused never clipped; exactly one payload spelling; requires a paused
    /// machine (named in §6's run-control state rule for `press`'s reason — a poke mid-free-run
    /// mutates the timeline just as surely). Bytes travel the bus path, so hardware mirror masking
    /// applies and no `ram_mut` debug back door exists on core.
    fn write_memory(&mut self, params: &Value) -> Result<Value, RpcError> {
        self.require_paused("emulator/write_memory")?;
        let addr = self.resolve_target(params)?;
        let data: Vec<u8> = match (params.get("bytes"), params.get("value")) {
            (Some(_), Some(_)) => {
                return Err(RpcError::invalid_params(
                    "`bytes` and `value` are alternatives — pass exactly one",
                ))
            }
            (Some(b), None) => {
                if params.get("width").is_some() {
                    return Err(RpcError::invalid_params(
                        "`width` goes with `value`; a `bytes` payload carries its own length",
                    ));
                }
                let d = hex::parse_bytes("bytes", b)?;
                if d.is_empty() {
                    return Err(RpcError::invalid_params("`bytes` is empty — nothing to write"));
                }
                if d.len() as u64 > self.config.max_write_len {
                    return Err(RpcError::invalid_params(format!(
                        "`bytes` is {} bytes; the ceiling is limits.maxWriteLen = {}",
                        d.len(),
                        self.config.max_write_len
                    )));
                }
                d
            }
            (None, Some(v)) => {
                let Some(value) = v.as_u64() else {
                    return Err(RpcError::invalid_params(
                        "`value` must be a non-negative integer (D9 category 2)",
                    ));
                };
                let width = match params.get("width").and_then(Value::as_u64) {
                    Some(w @ (1 | 2 | 4)) => w as usize,
                    Some(_) => {
                        return Err(RpcError::invalid_params("`width` must be 1, 2 or 4"))
                    }
                    None => {
                        return Err(RpcError::invalid_params(
                            "`value` requires `width` (1, 2 or 4)",
                        ))
                    }
                };
                if width < 8 && value >= 1u64 << (width * 8) {
                    return Err(RpcError::invalid_params(format!(
                        "`value` {value} does not fit width {width}"
                    )));
                }
                // Big-endian, as the 68000 stores.
                value.to_be_bytes()[8 - width..].to_vec()
            }
            (None, None) => {
                return Err(RpcError::invalid_params(
                    "one of `bytes` (hex string) or `value`+`width` is required",
                ))
            }
        };
        let end = u64::from(addr) + data.len() as u64 - 1;
        if !(WORK_RAM_LO..=WORK_RAM_HI).contains(&addr) || end > u64::from(WORK_RAM_HI) {
            return Err(out_of_range(
                addr,
                "only the work-RAM window ($E00000-$FFFFFF) is writable; ROM and I/O writes are refused",
            ));
        }
        let mut sink = ();
        let mut bus = self.sys.mega_bus(&mut sink);
        for (i, b) in data.iter().enumerate() {
            bus.write8(addr + i as u32, FC_SUPERVISOR_DATA, *b);
        }
        Ok(json!({ "addr": hex::addr(addr), "len": data.len() }))
    }
```

METHODS entry (append after the `reload_rom` entry, keeping the §6-verbatim-name rule):

```rust
    MethodSpec {
        name: "emulator/write_memory",
        handler: Engine::write_memory,
        summary: "poke bytes into the work-RAM window (paused machine only; refused never clipped)",
    },
```

- [ ] **Step 4: Run — all write_memory tests pass, and the whole aether suite stays green**
  (`methods.rs`'s every-method-with-`{}` loop now hits the new handler — an empty-params call must
  return a stamped `-32602`, which the implementation above gives via resolve_target).

```bash
cargo test -p oracle-aether > /tmp/claude-1000/tier1-t5b.log 2>&1; echo "EXIT=$?"; grep -E "test result" /tmp/claude-1000/tier1-t5b.log
```

- [ ] **Step 5: Mutation checks — one recorded line each.** Apply each mutation, confirm the named
  test FAILS, revert, `touch` the file, confirm recompile:
  1. Bounds check `end > WORK_RAM_HI` removed → `rom_and_out_of_window_are_refused` fails.
  2. `to_be_bytes` swapped to `to_le_bytes` → `value_width_is_big_endian` fails.
  3. `write8` loop body deleted (write nothing, reply success) → `bytes_land_in_ram_and_read_back` fails.
  4. `require_paused` call removed → `a_free_running_machine_refuses_the_poke` fails.
  Record the four lines in the commit message body.
- [ ] **Step 6: fmt + clippy + commit**

```bash
cargo fmt --all && cargo fmt --all -- --check
cargo clippy --all-targets --workspace -- -D warnings && cargo clippy --all-targets --workspace --no-default-features -- -D warnings
git add crates/oracle-aether/
git commit -m "feat(aether): emulator/write_memory — the poke primitive, contract-first"
```

---

### Task 6: `emulator/reset` handler (TDD)

**Files (oracle-next worktree):**
- Create: `crates/oracle-aether/tests/reset.rs`
- Modify: `crates/oracle-aether/src/engine.rs` (METHODS entry; handler near `reload_rom`)
- Modify (one test): `crates/oracle-aether/tests/hosted.rs` (rom_changed on reset)

- [ ] **Step 1: Write the failing test file**

```rust
//! `emulator/reset` — `protocol.md` §6 (run-control), result defined by CR-22 / §11.13.

mod common;

use common::{spawn_system, Client};
use oracle_core::system::System;
use serde_json::json;

fn machine() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    sys
}

fn client(h: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(h);
    c.handshake(false);
    c
}

/// The reset restores the deterministic power-on anchor: the state hash after run-frames + reset
/// equals the hash captured at first boot — two independently derived values.
#[test]
fn reset_restores_the_power_on_anchor() {
    let h = spawn_system("rs-anchor", machine(), 64);
    let mut c = client(&h);
    let boot = c.ok("emulator/state_hash", json!({}));
    c.ok("emulator/run_frames", json!({"frames": 5}));
    let moved = c.ok("emulator/state_hash", json!({}));
    assert_ne!(
        boot["combined"], moved["combined"],
        "sanity: five frames actually changed the machine"
    );
    let r = c.ok("emulator/reset", json!({}));
    assert_eq!(r["deferred"], json!(false), "this server applies before the reply");
    let after = c.ok("emulator/state_hash", json!({}));
    assert_eq!(boot["combined"], after["combined"], "back at the anchor");
}

/// The master clock restarts: stamps on the reply AFTER the reset report frame 0 again.
#[test]
fn the_clock_restarts_from_zero() {
    let h = spawn_system("rs-clock", machine(), 64);
    let mut c = client(&h);
    c.ok("emulator/run_frames", json!({"frames": 3}));
    c.ok("emulator/reset", json!({}));
    let s = c.ok("emulator/status", json!({}));
    assert_eq!(s["frame"], json!(0), "stamps jump backwards by design — that is the reset");
}

/// NOT run-state gated (the checkpoint precedent): a free-running machine accepts it.
#[test]
fn it_answers_a_free_running_machine() {
    let h = spawn_system("rs-freerun", machine(), 64);
    let mut c = client(&h);
    c.ok("emulator/resume", json!({}));
    let r = c.ok("emulator/reset", json!({}));
    assert_eq!(r["deferred"], json!(false));
}

/// §8 item 20 closure, asserted locally: the key set is exact.
#[test]
fn the_key_set_is_exact() {
    use std::collections::BTreeSet;
    let h = spawn_system("rs-keys", machine(), 64);
    let mut c = client(&h);
    let r = c.ok("emulator/reset", json!({}));
    let got: BTreeSet<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
    let want: BTreeSet<&str> =
        ["deferred", "frame", "mclk", "running", "droppedEvents"].into_iter().collect();
    assert_eq!(got, want);
}
```

Additionally, per the ruling's cross-CR finding 6 (reset's adoption condition asserts run-state
preservation both ways): add a run-state-preservation test — a reset issued on a paused machine
leaves `running: false` on the next `status` reply; a reset issued on a free-running machine leaves
`running: true`.

- [ ] **Step 2: Run — must fail with `-32601`**

```bash
cargo test -p oracle-aether --test reset > /tmp/claude-1000/tier1-t6a.log 2>&1; echo "EXIT=$?"; grep -E "test result|panicked" /tmp/claude-1000/tier1-t6a.log | head
```

- [ ] **Step 3: Implement.** Handler placed next to `reload_rom` (whose body it is a strict subset
  of — `reload_rom` already calls `self.sys.reset()` at `engine.rs:1973`):

```rust
    /// `emulator/reset` — drive the /RESET sequence against the current cartridge (§6 run-control,
    /// result defined by CR-22 / §11.13).
    ///
    /// Deliberately NOT `require_paused`, for the checkpoint block's reason: a reset replaces the
    /// machine wholesale between frames — it advances nothing and cannot fight the free-run loop.
    /// Symbols are KEPT: the image is unchanged, so the binding that survived boot survives this
    /// (contrast `reload_rom`, which re-validates). The generation bump is `restore`'s precedent —
    /// the timeline jumped, and a hosted player resyncs off `PumpReport::rom_changed`.
    fn reset(&mut self, _params: &Value) -> Result<Value, RpcError> {
        self.sys.reset();
        self.held = [Pad::default(); 2];
        self.invalidate_screen();
        self.rom_generation += 1;
        Ok(json!({ "deferred": false }))
    }
```

METHODS entry:

```rust
    MethodSpec {
        name: "emulator/reset",
        handler: Engine::reset,
        summary: "drive the /RESET sequence — back to the power-on anchor, SRAM and symbols kept",
    },
```

**Name check:** `Engine` may already have a private method named `reset` — grep first
(`grep -n "fn reset" crates/oracle-aether/src/engine.rs`); if so, name the handler `reset_machine`
and keep the wire name `emulator/reset` (the MethodSpec decouples them).

- [ ] **Step 4: Add the hosted test.** In `tests/hosted.rs`, copy the file's existing
  pump-and-assert pattern (read the file first; use its established Host setup) to add:

```rust
/// A bus reset in hosted mode reports rom_changed, so the player resyncs its frame counter and audio.
#[test]
fn a_bus_reset_reports_rom_changed_to_the_host() {
    // ... file's established setup: build Host + System, connect a client ...
    // client sends emulator/reset; host.pump(&mut sys) afterwards must return a report with
    // rom_changed == true (the restore precedent).
}
```

- [ ] **Step 5: Run — reset tests + hosted + whole aether suite green**

```bash
cargo test -p oracle-aether > /tmp/claude-1000/tier1-t6b.log 2>&1; echo "EXIT=$?"; grep -E "test result" /tmp/claude-1000/tier1-t6b.log
```

Note: `methods.rs`'s every-method-with-`{}` loop now RESETS the machine mid-sweep. The loop only
asserts stamp presence, so this is safe — but if it newly fails, the failure is real information;
investigate rather than reorder.
- [ ] **Step 6: Mutation checks — one recorded line each:**
  1. `self.sys.reset()` removed (reply success, do nothing) → `reset_restores_the_power_on_anchor` fails.
  2. `rom_generation += 1` removed → the hosted `rom_changed` test fails.
  3. Hardcode `deferred: true` → schema passes (boolean) but `reset_restores_the_power_on_anchor`'s
     `deferred == false` assertion fails — this is the check that `deferred` is a fact, not a constant.
- [ ] **Step 7: fmt + clippy + commit**

```bash
cargo fmt --all && cargo fmt --all -- --check
cargo clippy --all-targets --workspace -- -D warnings && cargo clippy --all-targets --workspace --no-default-features -- -D warnings
git add crates/oracle-aether/
git commit -m "feat(aether): emulator/reset — the conspicuous absence, present"
```

**Registered follow-up (do NOT solve here): F-HOSTED-RESET-SRM.** `System::reset` clears
`sram_dirty` (a persistence throttle); the player's own Tab-reset flushes the `.srm` first
(`main.rs:1364-1368`), but a BUS-initiated reset in hosted mode bypasses that flush, so an unsaved
SRAM delta inside the debounce window loses its dirty signal. Small window (the autosave debounce),
zero impact on the standalone server (it never writes `.srm`). Record in the handoff with this
anchor; the fix belongs to the player's pump loop, not this slice.

---

### Task 7: `emulator/memory_hash` handler + CRC-32 (TDD)

**Files (oracle-next worktree):**
- Create: `crates/oracle-aether/src/crc32.rs`
- Create: `crates/oracle-aether/tests/memory_hash.rs`
- Modify: `crates/oracle-aether/src/lib.rs` (declare `mod crc32;` — match the file's existing
  visibility style for `hex`)
- Modify: `crates/oracle-aether/src/engine.rs` (METHODS entry; handler near `state_hash`)

- [ ] **Step 1: Write the CRC module with its known-answer tests**

```rust
//! CRC-32, IEEE 802.3 polynomial (the zlib/`crc32` one) — so a cartridge-window
//! `emulator/memory_hash` equals CRC32 over the same slice of the ROM file, which is what the
//! Aeon side's gates compare against. Table-driven, dependency-free, built at compile time.
//!
//! NOT in `oracle-core::state_hash`: that module is byte-compatible with Oracle's `OpStateHash`
//! and carries a do-not-touch warning; this is a bus convenience with a different job.

const fn table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

const TABLE: [u32; 256] = table();

/// CRC-32 over a byte slice (init `0xFFFFFFFF`, reflected, final XOR — the zlib convention).
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = TABLE[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::crc32;

    /// The standard check value every CRC-32 implementation must produce (ITU/zlib test vector) —
    /// an expectation from OUTSIDE this codebase, so it cannot be self-confirming.
    #[test]
    fn the_check_vector_holds() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn empty_is_zero() {
        assert_eq!(crc32(b""), 0);
    }
}
```

- [ ] **Step 2: Write the failing contract test file**

```rust
//! `emulator/memory_hash` — `protocol.md` §6 (memory), adopted as CR-23 / §11.13.
//! The gap `state_hash` leaves: nothing else on this bus hashes 68000 memory.

mod common;

use common::{spawn_system, Client};
use oracle_core::system::System;
use serde_json::json;

fn machine() -> System {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    sys
}

fn client(h: &oracle_aether::server::ServerHandle) -> Client {
    let mut c = Client::connect(h);
    c.handshake(false);
    c
}

/// The hash agrees with hashing the SAME range fetched through the independent `read` path —
/// poked to known bytes first so the input is controlled, not incidental.
#[test]
fn the_hash_matches_the_bytes_read_back() {
    let h = spawn_system("mh-agree", machine(), 64);
    let mut c = client(&h);
    c.ok(
        "emulator/write_memory",
        json!({"addr": "0xFF0500", "bytes": "0x0123456789ABCDEF"}),
    );
    let r = c.ok("emulator/memory_hash", json!({"addr": "0xFF0500", "len": 8}));
    assert_eq!(r["region"], json!("work RAM"));
    assert_eq!(r["len"], json!(8));
    // Independent derivations: the algorithm crates' own fns over the known payload.
    let payload = [0x01u8, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF];
    assert_eq!(
        r["fnv1a64"],
        json!(oracle_core::state_hash::hex(oracle_core::state_hash::fnv1a_bytes(&payload)))
    );
    assert_eq!(r["crc32"], json!(format!("0x{:08X}", oracle_aether::crc32::crc32(&payload))));
}

/// A cartridge-window hash equals CRC32 over the same slice of the ROM image — the property the
/// legacy MCP row promises and the Aeon gates rely on.
#[test]
fn a_cart_window_hash_matches_the_rom_slice() {
    let sys = machine();
    let expect = format!("0x{:08X}", oracle_aether::crc32::crc32(&sys.rom()[0x100..0x200]));
    let h = spawn_system("mh-rom", sys, 64);
    let mut c = client(&h);
    let r = c.ok("emulator/memory_hash", json!({"addr": "0x00000100", "len": 256}));
    assert_eq!(r["region"], json!("cartridge ROM"));
    assert_eq!(r["crc32"], json!(expect));
}

/// `len` is required, bounded, and a range past the region end is -32004 refused never clipped.
#[test]
fn bounds_and_the_required_len() {
    let h = spawn_system("mh-bounds", machine(), 64);
    let mut c = client(&h);
    let e = c.err("emulator/memory_hash", json!({"addr": "0xFF0000"}));
    assert_eq!(e["code"], json!(-32602), "len is required");
    let e = c.err("emulator/memory_hash", json!({"addr": "0xFF0000", "len": 4_194_305}));
    assert_eq!(e["code"], json!(-32602), "len above limits.maxHashLen");
    let e = c.err("emulator/memory_hash", json!({"addr": "0x00FFFFFF", "len": 2}));
    assert_eq!(e["code"], json!(-32004), "end past the window: refused, never clipped");
}

/// A pure read: answers a free-running machine (like emulator/read, unlike write_memory).
#[test]
fn it_answers_a_free_running_machine() {
    let h = spawn_system("mh-freerun", machine(), 64);
    let mut c = client(&h);
    c.ok("emulator/resume", json!({}));
    let r = c.ok("emulator/memory_hash", json!({"addr": "0xFF0000", "len": 16}));
    assert!(r.get("fnv1a64").is_some());
}

/// §8 item 20 closure, asserted locally: the key set is exact.
#[test]
fn the_key_set_is_exact() {
    use std::collections::BTreeSet;
    let h = spawn_system("mh-keys", machine(), 64);
    let mut c = client(&h);
    let r = c.ok("emulator/memory_hash", json!({"addr": "0xFF0000", "len": 4}));
    let got: BTreeSet<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
    let want: BTreeSet<&str> =
        ["addr", "len", "region", "fnv1a64", "crc32", "frame", "mclk", "running", "droppedEvents"]
            .into_iter()
            .collect();
    assert_eq!(got, want);
}
```

Note: `oracle_aether::crc32` must be `pub mod` for the test to reach it; if the crate keeps `hex`
private, make `crc32` the exception and say why in a line comment (tests derive expectations from
it — same reason `oracle_core::state_hash` exports its primitives).

- [ ] **Step 3: Run — must fail with `-32601`**

```bash
cargo test -p oracle-aether --test memory_hash > /tmp/claude-1000/tier1-t7a.log 2>&1; echo "EXIT=$?"; grep -E "test result|panicked" /tmp/claude-1000/tier1-t7a.log | head
```

- [ ] **Step 4: Implement.** Handler next to `state_hash` (~`engine.rs:1461`):

```rust
    /// `emulator/memory_hash` — fingerprint a byte range without moving it (§6 memory, CR-23 /
    /// §11.13). A pure read: no `require_paused`. Routes exactly as `read`'s bus space; the FNV is
    /// `state_hash`'s family, the CRC-32 is IEEE/zlib so a cart-window hash matches the ROM file.
    fn memory_hash(&mut self, params: &Value) -> Result<Value, RpcError> {
        let addr = self.resolve_target(params)?;
        let Some(l) = params.get("len") else {
            return Err(RpcError::invalid_params(
                "`len` is required — a hash without a length hashes nothing",
            ));
        };
        let len = hex::parse_count("len", l, 1, self.config.max_hash_len)?;
        let (data, region) = self.debug_read(addr, len as usize)?;
        Ok(json!({
            "addr": hex::addr(addr),
            "len": data.len(),
            "region": region,
            "fnv1a64": oracle_core::state_hash::hex(oracle_core::state_hash::fnv1a_bytes(&data)),
            "crc32": format!("0x{:08X}", crate::crc32::crc32(&data)),
        }))
    }
```

METHODS entry:

```rust
    MethodSpec {
        name: "emulator/memory_hash",
        handler: Engine::memory_hash,
        summary: "fingerprint a byte range (FNV-1a-64 + CRC-32) without moving it — the hash state_hash cannot give",
    },
```

- [ ] **Step 5: Run — memory_hash + whole aether suite green**

```bash
cargo test -p oracle-aether > /tmp/claude-1000/tier1-t7b.log 2>&1; echo "EXIT=$?"; grep -E "test result" /tmp/claude-1000/tier1-t7b.log
```

- [ ] **Step 6: Mutation checks — one recorded line each:**
  1. Handler hashes `&data[..data.len()-1]` (off-by-one slice) → both agreement tests fail.
  2. CRC init constant changed to `0` → `the_check_vector_holds` fails (the outside-world vector).
  3. `parse_count` max changed to `u64::MAX` → `bounds_and_the_required_len`'s cap case fails.
- [ ] **Step 7: fmt + clippy + commit**

```bash
cargo fmt --all && cargo fmt --all -- --check
cargo clippy --all-targets --workspace -- -D warnings && cargo clippy --all-targets --workspace --no-default-features -- -D warnings
git add crates/oracle-aether/
git commit -m "feat(aether): emulator/memory_hash — FNV-1a-64 + CRC-32 over a range, no payload on the wire"
```

---

### Task 8: MCP side — verify rows, fix the mangled description, teach the sweep

**Files (in `/home/volence/sonic_hacks/oracle` — NO REMOTE, commits are local-only):**
- Modify: `linux-port/mcp/oracle_mcp.py` (one string, line ~135)
- Modify: `linux-port/mcp/coverage_check.py` (`EXERCISE_ARGS`, `PAUSE_FIRST`, `ORDERED_LAST`)

The three tool rows already exist (`reset` :133, `memory_hash` :238, `write_memory` :304) and the
filter (`served_methods`, :829) makes them appear automatically once the bus advertises the
methods. The handlers built in Tasks 5–7 already match the rows' schemas by construction
(hex-string `addr`, both `write_memory` payload spellings, `fnv1a64`+`crc32`). Remaining work:

- [ ] **Step 1: Fix the corrupted model-facing description.** `oracle_mcp.py:135`: replace
  `"FlagInitialize the system — full hardware reset, like pressing F2."` with
  `"Reset the system — the /RESET sequence, back to the power-on state. SRAM and loaded symbols survive."`
- [ ] **Step 2: Teach `coverage_check.py` the two mutating tools.** In `EXERCISE_ARGS` add:

```python
    "write_memory": {"addr": "0xFF0000", "bytes": "0x00"},
```

  (`reset` needs no args — absent from `EXERCISE_ARGS` is fine, it gets `{}`). Add
  `"write_memory"` to `PAUSE_FIRST` (it is `-32005`-gated). Change `ORDERED_LAST` to
  `("restore", "checkpoint_drop", "watchpoint_clear", "write_memory", "reset")` — `reset` LAST of
  all so the sweep's machine state survives every measuring tool, with a comment saying so.
- [ ] **Step 3: Live validation against oracle-next.** Start our server with the debug ROM, then run
  the coverage check (per [[feedback-debug-rom-for-testing]] use `aeon/s4.debug.bin`; socket under
  `$XDG_RUNTIME_DIR` — the scratchpad path exceeds `SUN_LEN`):

```bash
cd /home/volence/sonic_hacks/oracle-next/.worktrees/player-s1-palette
cargo run --release -p oracle-aether --bin aether-server -- /home/volence/sonic_hacks/aeon/s4.debug.bin \
  --socket /run/user/1000/tier1-check.sock > /tmp/claude-1000/tier1-server.log 2>&1 &
# (verify the actual binary name + CLI first: `cargo run -p oracle-aether --bin` with no name lists them)
cd /home/volence/sonic_hacks/oracle/linux-port/mcp
ORACLE_SOCKET=/run/user/1000/tier1-check.sock .venv/bin/python coverage_check.py --exercise \
  > /tmp/claude-1000/tier1-coverage.log 2>&1; echo "EXIT=$?"
grep -E "MISSING|FAIL|offered" /tmp/claude-1000/tier1-coverage.log
```

Expected: EXIT=0, `MISSING TOOL ROWS (0)`, 31-for-31 offered and answered (28 + the three new).
Kill the server by PID afterwards — **never `pkill -f`** (it self-matches and kills the harness
shell, exit 144).
- [ ] **Step 4: Commit (oracle repo, local only)**

```bash
git -C /home/volence/sonic_hacks/oracle add linux-port/mcp/oracle_mcp.py linux-port/mcp/coverage_check.py
git -C /home/volence/sonic_hacks/oracle commit -m "mcp: the Tier 1 tools go live — un-mangle reset's description, teach the sweep to poke and to reset last"
```

---

### Task 9: Full gates, handoff doc, gap-list closure

**Files (oracle-next worktree):**
- Create: `docs/2026-08-18-tier1-bus-methods.md` (handoff)
- Modify: `docs/2026-08-17-aeon-switchover-gap-list.md` (mark Tier 1 shipped)

- [ ] **Step 1: Full workspace gates, firsthand** (single tree — confirm no other `cargo test` runs):

```bash
cd /home/volence/sonic_hacks/oracle-next/.worktrees/player-s1-palette
cargo fmt --all -- --check
cargo clippy --all-targets --workspace -- -D warnings
cargo clippy --all-targets --workspace --no-default-features -- -D warnings
cargo test --workspace > /tmp/claude-1000/tier1-gate.log 2>&1; echo "EXIT=$?"
grep -cE "^test result.*FAILED" /tmp/claude-1000/tier1-gate.log; grep -E "test result" /tmp/claude-1000/tier1-gate.log | tail -40
```

Expected: fmt clean, clippy 0 both variants, EXIT=0, 0 failed across all legs (baseline was 1554
passed / 36 legs; expect ~+25).
- [ ] **Step 2: Verify zero core-tests diff** (the standing record: `crates/oracle-core/tests/` has
  been a zero-file diff for four sessions — this slice must keep it, and `crates/oracle-core/src`
  should also be untouched):

```bash
git diff --stat m68000-microop-framework -- crates/oracle-core/ | tail -3
```

Expected: empty (no oracle-core changes at all in this slice).
- [ ] **Step 3: Write the handoff doc.** Must include: what shipped (three methods, four surfaces
  each — bus/MCP done, GUI decisions recorded per the parity table: `reset` already had one,
  `write_memory` memory-editor deferred by design, `memory_hash` none-by-decision); gates output
  verbatim; the mutation-check lines from Tasks 5–7; registered follow-ups **F-WALK-ROW** (sprite
  link walk §6 row — trigger fired) and **F-HOSTED-RESET-SRM** (Task 6's note, with anchors);
  push state of all three repos (`oracle` cannot push — say it again).
- [ ] **Step 4: Update the gap list.** In `docs/2026-08-17-aeon-switchover-gap-list.md`, under the
  Tier 1 heading, add a dated line: Tier 1 shipped (three methods live; `ab_runner` can re-point),
  pointing at the handoff doc. Do not rewrite the historical content.
- [ ] **Step 5: Commit**

```bash
git add docs/
git commit -m "docs: Tier 1 handoff — the pixel-gate blockers are on the bus"
```

---

### Task 10: Whole-branch review, merge, push

- [ ] **Step 1: Whole-branch review** — the S1/S3 record shows it finds what per-task review
  structurally cannot (the Enter→Start leak; the overlay erasing glyphs). Dispatch a code-reviewer
  agent over `git diff m68000-microop-framework...aeon-tier1` plus the empyrean diff, with the CR
  doc + ruling as the spec. Specific standing questions for it: (a) does every result key on the
  wire appear in its fragment and vice versa; (b) is any assertion comparing a value to itself;
  (c) do the two repos' texts agree (the amendment vs the handler comments); (d) did anything
  touch `crates/oracle-core`.
- [ ] **Step 2: Fix what it finds** (each fix gets its own commit with a failing-test-first where
  the finding is behavioural).
- [ ] **Step 3: Merge and push oracle-next**

```bash
cd /home/volence/sonic_hacks/oracle-next
git merge --no-ff aeon-tier1 -m "Merge Tier 1 — write_memory, reset, memory_hash: the Aeon switchover unblocked"
cargo test --workspace > /tmp/claude-1000/tier1-merged.log 2>&1; echo "EXIT=$?"   # gates on the MERGED tree
git push origin m68000-microop-framework
```

- [ ] **Step 4: Push empyrean**

```bash
git -C /home/volence/sonic_hacks/empyrean push
```

- [ ] **Step 5: Report to the owner**: what shipped, the coverage-check result, the two registered
  follow-ups, and the owner-owed items that did NOT move (smoke checklist, gamepad, SY-7 mix).

---

## Self-review notes (run at plan-writing time)

- **Spec coverage:** gap-list Tier 1 items 1–3 → Tasks 5/6/7; contract-first ruling → Tasks 1–4
  strictly precede 5–7; three-surface parity → Task 8 (MCP) + GUI decisions recorded in Task 9;
  the walk-row debt → explicitly registered out-of-scope. ✓
- **Types:** `max_write_len`/`max_hash_len` (config) vs `maxWriteLen`/`maxHashLen` (wire) used
  consistently; `FC_SUPERVISOR_DATA` defined in Task 5 and reused nowhere else; `crc32` public
  because tests derive expectations from it. ✓
- **Known uncertainties an implementer must verify in-tree (named, not hand-waved):**
  `EngineConfig`'s exact field/default syntax; whether `Engine` already has a `fn reset` (Task 6
  Step 3 names the fallback); the aether-server binary name/CLI (Task 8 Step 3 says how to list);
  `hosted.rs`'s setup pattern (Task 6 Step 4 says copy the file's own); whether `tests/handshake.rs`
  pins the `limits` key set — if it does, add the two new keys in Task 5 Step 3's commit.
