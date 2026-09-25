**Kind:** investigation

# F-BANKED-ADDR-AMBIGUITY: recon, and a DRAFT contract change for the hub

**Date:** 2026-09-25 · **Branch:** `parcel/banked-addr-recon` · **Base (oracle HEAD):** `094f847daf59a9a4c0d8e180303306e1aff35761`
**Contract read:** empyrean `origin/main` = `0ebc923f1d582e33df2ec60a4266d074afdbd4a6` (after `git fetch`), via
`git show`, never the working tree. The vendored schema `crates/oracle-aether/tests/contract/bus-protocol.schema.json`
is byte-identical to that commit's `contract/schema/bus-protocol.schema.json` (`cmp` exit 0, measured).
**Nothing under `crates/` changed. Nothing was applied to `../empyrean`. No emulator was run.** Every claim is
marked **(measured)** (a command was run and its output read) or **(reasoned)** (read from code or text, not run).

## What's going on, in plain words

Oracle now copies a trick some big cartridges use: part of the cartridge's address range can be switched to show a
different slice of the ROM. So an address such as `$088000` can mean different bytes at different moments, and a
debugger that reports only the address (where the game stopped, which code the profiler timed, what a watch
caught) leaves out which slice it was. Only the three memory-read replies report the slice today, as a phrase in
text. No suite game switches slices, so today nothing is wrong in practice. My recommendation is one small, typed
field, `romOffset`, which says exactly which ROM byte an address meant at that moment. It goes on the memory reads
and on "where the CPU is now" replies, together with the current switch table on `status`. The harder historical
cases (profiler rows, watch hits) get stated honestly in the contract and wait until a game that switches slices
actually arrives.

## 1. The premise, re-derived at HEAD `094f847`

The booking (`docs/2026-09-11-cart-mapper-design.md` §8, last bullet) said: *"A 24-bit address inside a banked
window no longer names one ROM byte across time … Nothing in-tree reports the bank alongside such an address yet."*
It was written before `F-DEBUGREAD-BANKED` merged. **Verdict: PARTLY TRUE.** The ambiguity is real and unchanged.
The "nothing reports the bank" half went false for exactly three methods on 2026-09-11, and it is still true for
every other address on the bus.

**What went false (measured).** `emulator/read {space:"bus"}`, `emulator/read_memory` and `emulator/memory_hash`
carry `region`. Under a re-pointed window `region` is `"cartridge ROM bank N"`. That is contract text since §11.48
(empyrean `0ebc923` `contract/protocol.md` l.1237-1243 and l.5812-5901), served by
`crates/oracle-aether/src/engine.rs:9943` (`cart_region`) and `:9984` (`debug_read`). The player's Memory panel
shows the same label (`crates/oracle-player/src/memory.rs:1249`). The bank travels **as a string**. §11.48's own
SHOULD **S1** (l.5881) says it should become a typed key *"once, with oracle's F-BANKED-ADDR-AMBIGUITY"*, so this row
also owns that discharge.

**What is still true (measured).** Outside the core, exactly one line reads the bank table:

    git -C <worktree> grep -n -E 'cart_banks\(\)' HEAD -- crates/oracle-aether/src crates/oracle-player/src \
        crates/oracle-frontend/src crates/oracle-replay/src crates/oracle-panels-spike
    -> HEAD:crates/oracle-aether/src/engine.rs:10013   (inside debug_read; nothing else)

No typed bank or offset key exists anywhere in the tree or in the vendored schema:

    git -C <worktree> grep -c -E '"(bank|banks|cartBanks|romOffset|romCrc32|imageOffset)"' HEAD -- crates   -> 0
    git -C <worktree> grep -c -E '"(bank|banks|cartBanks|romOffset|romCrc32|imageOffset)"' HEAD -- \
        crates/oracle-aether/tests/contract/bus-protocol.schema.json                                        -> 0

The core consumers that hold history are bank-blind in their data structures. This is **(reasoned)**, read from
the types, not run:

- Watch hits: `WatchHit { addr: u32, pc: u32, frame, mclk, … }` (`crates/oracle-core/src/watchpoints.rs:515-544`) and
  `Stamp { pc, frame, mclk, seq }` (`:294-302`). Neither has a bank.
- Profiler: rows are `BTreeMap<u32, Counts>` keyed by the bare entry address (`crates/oracle-core/src/profiler.rs:427`,
  `:467`, `:470`), with caller edges `BTreeMap<(u32, CallerKey), EdgeCounts>` (`:437`). **Two routines at one bus
  address in two banks become ONE row.** That is conflation, not just an ambiguous label: no bank added to the reply
  afterwards can split it.
- Breakpoints: `first_enabled_at(addr: u32)` / `record_halt(addr: u32)` (`crates/oracle-aether/src/breakpoints.rs:145`,
  `:164`). A breakpoint fires at its bus address in **every** bank.
- Symbols: `SymbolTable::resolve(addr)` masks to 24 bits and searches one address-sorted list
  (`crates/oracle-core/src/symbols.rs:1322-1336`). The listing's `Phase Table` does carry both halves, `vma` and `lma`
  (`:657-669`), but only for phased blocks, and those rows never enter `resolve` (`:681-682`).

**The machine does bank, on a real image (measured by the overseer on 2026-09-11, not by me):** *Sonic Delta Origins*
(5,242,880 B) re-points windows 6 and 7 at banks 8 and 9 at its title screen (`docs/2026-09-11-debugread-banked.md`
§9). **No suite ROM banks (measured):** aeon's whole ROM source (206 `.emp`/`.asm` files in `engine/` + `games/`, plus
`tools/`, at aeon `origin/master` `63013da`) contains no `$A130xx` reference at all:

    git -C ../aeon grep -c -i -E 'A130' 63013da -- engine games tools          -> 0 (exit 1)
    git -C <worktree> grep -c -i -E 'A130F[3579BDF]' HEAD -- crates/oracle-core/src/bus.rs   -> 11  (the control fires)

The hub has already ruled on demand. empyrean `0ebc923` `docs/OVERSEER-LOG.md:13565` reads: *"no lane wants it
(aeon does not bank, sigil builds S1/S2) — declining is right under 'grow only against a real consumer'; its trigger
is a banked ROM entering the suite."* The row was then pushed as **recon + draft CR** under the owner's 2026-09-02
words (`:15238`), not as a build. This doc is written to that scope. §4's recommendation is sized so that most of it
needs no consumer to justify it, and the part that does need one is gated on a named trigger.

**Contradictions of the brief, with evidence (brief rule 4):**

1. *"`vendor/` if present … Oracle's vendored copy: `contract/` in this repo."* Neither directory exists at HEAD
   (measured: `ls contract vendor` gives no such directory). The only vendored contract artifact is the schema at
   `crates/oracle-aether/tests/contract/bus-protocol.schema.json`. `protocol.md` is not vendored.
2. *"§5 says three seams are ONE and should ship once."* This is only partly accurate. `docs/2026-09-11-debugread-banked.md`
   §5 ties the fingerprint (*"it belongs with F-BANKED-ADDR-AMBIGUITY"*) and the typed key (*"should ship there,
   once"*) to this row. Its SRAM-space bullet carries **no** bundling sentence. The hub has also already **split** the
   fingerprint off: §11.48 **S2** reads *"as its own CR when a consumer needs one"* (protocol l.5882-5883). §3.6 below
   rules on the bundling afresh.
3. *"trace records"* in the brief's list of ambiguous carriers. No trace method is served. 62 `emulator/*` methods are
   in `engine::METHODS`, and `grep -E '^\s+name: "emulator/' crates/oracle-aether/src/engine.rs | grep -c -i trace`
   gives `0`, where the same pipe with `step` gives `3` (measured). Nothing on the wire is a trace record, so that
   class is empty today.

Re-run everything in this section:
`git -C <worktree> grep -n -E 'cart_banks\(\)' HEAD -- crates/oracle-aether/src crates/oracle-player/src crates/oracle-frontend/src crates/oracle-replay/src crates/oracle-panels-spike`.

## 2. Consumer enumeration: every served field that can carry a cart-space 68000 address

### 2.1 How the list was derived, and the enumeration's own controls

Two independent walks, then a cross-check. The scripts are in Appendix A, verbatim, so a reader can re-run them.

- **Schema walk (the authority).** `walk.py` recurses every `methods`, `events` and `$defs` fragment of the vendored
  schema and prints each leaf typed `$ref: #/$defs/hex`, which is D9 category 1, *"Addresses and byte payloads are hex
  strings"*. Result (measured): **94 hex leaves** over **31 methods + 1 event (`emulator/stopped`) + 5 `$defs`**.
  Every one of the 31 methods is in `engine::METHODS` (`comm` of the two sorted lists, measured; the only name
  left over is the event). The 9 schema methods that `engine::METHODS` does not serve (`ping`, `vgm_*`,
  `audio_spectrum`, `*_channel_*`, `log_clear`) carry no hex leaf.
- **Code walk (does the server actually emit it).** `attrib.py` finds every line in `engine.rs`, `decoders.rs` and
  `objreq.rs` (above each file's `#[cfg(test)]`) that writes a JSON key through `hex::addr(`, and names the
  enclosing `fn`. Result (measured): **56 emit sites**.
- **Positive controls.** Both walks return the two fields known to exist: `emulator/stopped.pc` (schema, and
  `engine.rs:3435` `emit_stopped`) and `emulator/read.addr` (schema, and `engine.rs:4690` `read`). **Each walk also
  missed something, and the cross-check caught it:**
  - The schema walk's hex filter misses **`watchpoint_list.watches[].census[].key`**. It is `type: integer`, and
    under `censusKey: "addr"` it *is* an address (its own description: *"an address, a written value, or 0/1/2 for
    'via'"*). It was found by reading all **286 integer leaves** the same walk prints with `int`. It is the only
    integer-typed address.
  - The code walk misses **`registers.d0-d7` / `a0-a7`**. Their keys are built with `format!("d{i}")`
    (`engine.rs:4073-4077`), not literals. They were found because the schema walk lists them and the code walk does
    not.

  So neither walk is complete alone. The table is the union, and every row names both its schema leaf and its emit
  site.

### 2.2 The table

**Classes.** **NOW** means sampled from the live machine in the same handler call that replies, or at the stop that
emits. **HIST** means captured at an access or step and reported later, possibly aggregated. **CONFIG** means an
address the client armed, echoed back, which then *matches* bus addresses. **READ** means a debug read whose
provenance `region` already discloses (§11.48). **SYM** means a listing address. "Ambiguous today" means that under
a re-pointed window the field alone does not identify one ROM byte, and nothing typed in the same reply does either.

| # | Field (schema leaf) | Emit site | Class | Ambiguous today? |
|---|---|---|---|---|
| 1 | event `emulator/stopped.pc` (+ `symbol`/`symbolDisp` derived from it) | `engine.rs:3435` `emit_stopped` | NOW | **yes** |
| 2 | `status.pc` (+ `symbolAtPc`) | `engine.rs:3850` | NOW | **yes** |
| 3 | `registers.pc` | `engine.rs:4079` | NOW | **yes** |
| 4 | `run_frames.pc` | `engine.rs:4110` | NOW | **yes** |
| 5 | `run_to.pc` | `engine.rs:4164` (the stop record) | NOW | **yes** |
| 6 | `step.pc`, `step_over.pc`, `step_out.pc` | `engine.rs:4457` `halt_result` (called at `:4355`, `:4416`, `:4439`) | NOW | **yes** |
| 7 | `play_input.pc` | `engine.rs:7200` | NOW | **yes** |
| 8 | `wait_for_break.pc` | `engine.rs:9073` (reads `cpu_regs().pc` of a paused machine) | NOW | **yes** |
| 9 | `read.addr` (space `bus`) | `engine.rs:4690` | READ | disclosed, **as a string** (`region`) |
| 10 | `read_memory.addr` | `engine.rs:4538` | READ | disclosed, as a string |
| 11 | `memory_hash.addr` | `engine.rs:5292` | READ | disclosed, as a string |
| 12 | `watchpoint_hits.hits[].addr` | `engine.rs:8775` `watch_hit_json` | HIST | **yes** (`value`/`old`, `:8776`/`:8780`, are the true bytes and not addresses) |
| 13 | `watchpoint_hits.hits[].pc` | `engine.rs:8785` | HIST | **yes** |
| 14 | `watchpoint_list.watches[].first/.last` → `$defs/watchStamp.pc` | `engine.rs:9696` `watch_stamp_json` | HIST | **yes** |
| 15 | `watchpoint_list.watches[].census[].key` (censusKey `addr`) | `engine.rs:9681` | HIST, aggregated | **yes, and conflated**: one key counts accesses in every bank |
| 16 | `get_profiler_frames.routines.items[].addr` | `engine.rs:5664` `profiler_row` | HIST, aggregated | **yes, and conflated** (`profiler.rs:427`) |
| 17 | `get_profiler_frames.routines.items[].callers[].callerAddr` | `engine.rs:5727` `profiler_caller_edge` | HIST, aggregated | **yes, and conflated** (`profiler.rs:437`) |
| 18 | `run_to.target` | `engine.rs:4142`, `:4162` | CONFIG | **yes**: matches in every bank |
| 19 | `watchpoint_add.addr`, `watchpoint_list.watches[].addr` | `engine.rs:8576`, `:9652` | CONFIG | **yes**: a bus watch range matches in every bank (`watchpoint_add` refuses only a range past `BUS_ADDR_MAX`) |
| 20 | `breakpoint_add.addr`, `breakpoint_list.breakpoints[].addr` (+ `hits`) | `engine.rs:8853`, `:8943` | CONFIG, `hits` aggregated | **yes**: fires in every bank (`breakpoints.rs:145`), and `hits` sums across banks |
| 21 | `lookup_symbol.addr`, `.rawAddr`, `.otherMatches.items[].addr` | `engine.rs:7271`, `:7310-7311`, `:7349`, `:9709` | SYM | **yes, for a banked listing**: a VMA in window *k* names a different ROM byte per bank (`symbols.rs:1322`) |

That is **21 rows covering 27 result/event fields, over 19 methods and 1 event.** **24 fields are ambiguous today and
3 disclose the bank only as a string.** The two-surface cross-check above is the enumeration's positive control.

**Enumerated and excluded, with the reason for each (the negative class):**

- **Register values.** `registers.a0-a7`, `d0-d7`, `sp`, `usp`, `ssp` (`engine.rs:4073-4082`) and `status.sp`
  (`:3851`) are **values**. One may hold a cart pointer, but the bus never asserts that it is an address. A client
  that dereferences one does so through `read`, which discloses. **Decision: not annotated.**
- **RAM by construction.** `write_memory.addr` (`:4639`) is refused outside `$E00000-$FFFFFF` (`:4629-4633`), and so
  are the object mailbox's `addr` / `handle` / `baseAddr` (`engine.rs:6520`, `:6880`, `:6910`; `decoders.rs:330`,
  `:684`; `objreq.rs:243`), which are SST records in work RAM.
- **Not 68000 space (19 hex leaves).** `read_vram` / `write_vram.addr`, `read_cram` / `write_cram.cramAddr`,
  `pixel_attribution.cramAddr/tileAddr/satAddr`, `sprites.satBase`, `z80_read` / `z80_write.addr`. `z80_read` serves
  only the Z80's own 16 KB (`engine.rs:6100-6130`), never its `$8000` bank window.
- **Error echoes.** `error.data.addr` (`engine.rs:10326`, `:7258`) repeats a request's address back. It inherits the
  request's meaning and asserts nothing new.
- **`object_spawn.def`** is a **param** (a client-supplied `ObjDef` pointer; pre-flighted at `engine.rs:6612` to
  lie in `$000000-$3FFFFF`). The game dereferences it through the bus, so on a banked game it would mean whatever
  the window shows at the dereference. It is a CONFIG input with no reply-side field. Aeon places no ObjDef in a
  banked window, because aeon does not bank (§1). **Recorded, not annotated.**

### 2.3 The third surface (the player GUI), in-process

These are not wire fields, but the three-surface directive asks that each gap be a decision. Every one prints a
bare address (measured by grep, read not run): the CPU lens `PC $XXXXXX` (`crates/oracle-frontend/src/lens/cpu.rs:127-129`),
the watch lens hit PC (`lens/watch.rs:54`), the profile lens rows (`lens/profile.rs`), and the player's stop and
breakpoint lines (`crates/oracle-player/src/stopping.rs:541`, `:1226-1228`, `:2520`). The Memory panel alone shows
the bank (`memory.rs:1249`).

### 2.4 Sibling consumers (read through git objects at each lane's `origin` default branch)

- **aurora** `origin/master` `44ba3d7` (measured): its `src/` calls `emulator/status` (for `romPath`),
  `lookup_symbol` **by name** (`client.ts:462`), `run_to` **by symbol** (`boot-restore.ts:184`), and `read_memory`
  bytes. `git grep -c -E 'watchpoint_hits|get_profiler_frames|breakpoint_add|breakpoint_list|callerAddr|wait_for_break' 44ba3d7 -- src`
  gives **0**. No history-bearing result has a reader there. Results are cast (`as { romPath?: string }`), not
  validated. The only `z.object` in `src/main/aether` (`adapter.ts:25`) validates aurora's own `editor/*` **params**.
  **An additive result key breaks nothing in aurora (reasoned from those sites).**
- **aeon** `origin/master` `63013da` (measured): `get_profiler_frames` in three cost probes
  (`tools/parallax_cost_probe.py:748`, `:853`, `:1083`; `tools/raster_cost_probe.py:608`), and `memory_hash` in
  `tools/evict_witness.py`, which is §11.48's named reviewer. Aeon does not bank (§1), so every row it reads is the
  identity case. `tools/test_legacy_seam_keys.py` pins the **params** aeon sends, not result keys. **An additive result
  key breaks nothing in aeon (reasoned).**
- **The MCP shim**, which is live: `~/.claude.json` registers `oracle-old/linux-port/mcp/oracle-mcp`. At its `HEAD`
  `1eb09a9` (which is `mcp_tool_sweep.rs`'s `PIN_REV`), it returns results through `json.dumps(result)`
  (`oracle_mcp.py:1261`, `:1747`). So a new key reaches the model unchanged. It never reads `region`: `git grep -c -E
  'get\("region"\)|\["region"\]'` gives 0, with `get\("crc32"\)` giving 1 as the control. **Two findings in passing,
  both (reasoned):**
  - Its ROM-freshness check (`:1361-1555`) hashes `$000000..romBytes` in `maxHashLen` chunks, which are 4 MiB
    (`engine.rs:230`, fallback `oracle_mcp.py:1296`). On any image over 4 MiB, and on any image while a window is
    re-pointed, the first refused chunk makes it answer `"unmeasurable"` (`:1510-1520`). That is loud, never a false
    `"stale"`, because §11.48 refuses a straddle. **This is the first concrete consumer of §11.48 S2** (an
    address-free fingerprint), but only once a banked ROM is driven through MCP.
  - Its `memory_hash` tool description (`:478-487`) still says *"one of two regions"* and that *"a cart-window hash
    matches CRC32 over the ROM-file slice"*. That is a §11.48 paraphrase left standing in a live client. It is not
    this row's to fix, and it is booked in §8.

## 3. Options

Every option is scored on the same four questions: **what a client gains**, **what breaks** for aurora, aeon's tools
and the MCP shim (§2.4), **the schema and vendoring cost**, and **whether it is right for the HIST rows**. The last
question is the axis the booking did not name, and it decides most of the options.

**The axis the listed options miss: WHEN the bank is read.** For a NOW field, the mapping in force at reply time is the
mapping in force when the fact was true. The machine is paused, or the handler samples PC and table in one call (§2.2
rows 1-8). For a HIST field it is not. A watch hit, a stamp, a profiler row and a census key were captured at some
earlier access. The window may have been re-pointed since, and on *Sonic Delta Origins* it is re-pointed at the title
screen (§1). So **any bank read at reply time and attached to a HIST record is a plausible wrong answer**. That is
exactly the shape §11.48 removed from `read_memory`. Aggregated HIST rows (16, 17, 15, and 20's `hits`) are worse: they
have already merged two banks' events into one number, and no field added afterwards can split them.

### (i) A bank alongside the address: a typed `cartBanks` snapshot per reply or per event

- **Gains:** a NOW reply becomes self-describing. The client computes `bank*$80000 + (addr & $7FFFF)` itself.
- **Breaks:** nothing, since it is additive and all three consumers tolerate extra keys (§2.4).
- **Cost:** the same 8-integer array on roughly 12 fragments, including every `stopped` event and every `step`. That
  is noise on the hottest replies, and it carries meaning only when a PC or address is in `$080000+`.
- **HIST:** **wrong.** Attached to `watchpoint_hits`, `get_profiler_frames` or `watchpoint_list`, it describes the
  table now, not at capture. Capturing it per record instead means 8 bytes per hit in the ring, taken in the watch
  sink's `on_event`, which is the bus hot path (`BusEvent`, `bus.rs:55-61`, carries no bank). It cannot split a
  profiler row at all.
- **Verdict:** keep the *table* (it belongs on `status`, once). **Reject it as the per-reply carrier.**

### (ii) A "banked address" type

- **(ii-a) `{addr, bank}` objects in place of the hex string.** This breaks every consumer. aurora's cast `pc` string,
  aeon's dict reads and the shim's pretty-printing all change type, and D9 category 1 (*"Addresses … are hex
  strings"*) is violated. It needs a version bump. **Reject.**
- **(ii-b) A 32-bit form: the bank in bits 24-31, or the image offset itself in place of `addr`.** The 32-bit spelling
  is **already taken**. The listing's sign-extended RAM spelling is `0xFFFF_8CFA` (`symbols.rs:370-377`, `raw_addr`),
  and the contract tells clients to **mask to the 24 address lines** (protocol l.1974-1976, the profiler `addr`). A
  client that masks, as told, silently drops the bank, and bank `$FF` collides with the RAM spelling. Putting the
  image offset *in place of* `addr` is §11.48's rejected option (B), *"two vocabularies wearing one name"*
  (protocol l.5863). **Reject.**

### (iii) Status-only window table, plus a normative "addresses are the bus view at the time of the event"

- **Gains:** almost nothing to build: `status.cartBanks` from `System::cart_banks()` (`system.rs:923`), plus prose. The
  ambiguity becomes a **documented property instead of a surprise**. History can be reconstructed with existing
  surfaces: arm a `write` watch on `$A130F3-$A130FF` before the first re-point, and each hit's `value` is a bank with
  an `mclk` to order it against a record's. That is **(reasoned)**, not run. The overseer measured on 2026-09-11 that
  a watch on the neighbouring `$A130F1` recorded the ROM's own write (debugread doc §9). `$A130F3+` writes take the
  same bus write arm (`bus.rs:1330-1345`).
- **Breaks:** nothing.
- **Cost:** one key on `status` and one paragraph.
- **HIST:** documented, not solved. The join recipe works for hits and stamps and **cannot un-merge an aggregate**.
- **Leaves open:** §11.48 **S1**. Clients still parse `N` out of `"cartridge ROM bank N"`, against §2.4 rule 3 (*"Any
  consequence a client must act on needs its own typed key"*).

### (iv) Do nothing until a game needs it

- **Gains:** zero cost. It honours the hub's own standing answer (`OVERSEER-LOG.md:13565`: *"its trigger is a banked
  ROM entering the suite"*).
- **Breaks:** nothing today.
- **Costs that are not zero:** **the contract says something false about banked carts, and nothing flags it.**
  Protocol l.1974-1976 promises that a profiler `addr` is masked *"so a row key and a listing address compare
  directly"*. On a banked cart a row key is two routines. The acceptance ROM already banks, so a human profiling it in
  oracle today gets merged rows with no warning. S1 stays an open SHOULD indefinitely.

### Better-approach pass: what the listed options are the floor of

**(v) `romOffset`: one typed companion field, defined once as a §2.4 shared convention.** It is a hex string (D9
category 1): **the offset into the loaded ROM image of the byte the bus resolved the sibling address to, at the
moment that address was captured**. It is **present exactly when that address resolved to a cartridge-ROM byte**, so
absence has one meaning: work RAM, the SRAM overlay's lane, I/O or open bus. Why an *offset* rather than a *bank*:

1. **It is the comparison clients actually make.** A file slice, a CRC, a listing's `lma` (`symbols.rs:667`: *"where
   the code is stored — the offset in the assembled image"*). Each is one field read, with no
   `N × $80000 + (addr & $7FFFF)` formula (which §11.48 had to write into prose, l.1173).
2. **It is mapper-agnostic.** A `bank` number presumes the SSF2 geometry of 512 KiB windows. The mapper note §7
   already names other mappers (Codemasters, Pier Solar). Under this mapper, bank = `romOffset >> 19`. The reverse
   derivation needs the geometry.
3. **Under the identity mapping it equals the address.** Every unbanked client (all of them today) can ignore it, and
   nothing it already does changes.
4. **It keeps `addr` a bus address on every row**, which is §11.48 (C)'s principle, and it is additive.

**(vi) The capture discipline as a contract rule, not an implementation detail.** *"Computed from the mapping in
force when the fact was captured; a server MUST NOT derive it from a later mapping."* This one sentence is what makes
(v) safe to extend to HIST later. It is also what makes it wrong to put on HIST fields *now* without per-record capture.

**(vii) For HIST, when it is needed: a mapper-write journal instead of per-access capture.** Bank registers are
*expected* to be written rarely. On *Sonic Delta Origins* the only measurement is that windows 6 and 7 were identity
at frame 120 and re-pointed by frame ~1,545 (debugread §9). **How often it writes them was not measured (TAG-B2,
§8)**, and the journal's bound depends on it. A bounded journal of `(mclk, window, bank)` kept by `System` (snapshot state, like `cart_banks`, so
restore rewinds it) lets the server resolve a hit's or stamp's `romOffset` **at reply time from the mapping at the
record's `mclk`**, with **no change to the watch sink or the bus hot path**. Profiler rows are the exception. They
aggregate at step time, so they must be keyed `(addr, romOffset-or-bank)` in `profiler.rs`, and that is a hot-path
key change (`on_step_retire`) with a measurable cost. All of this is **wave B**, gated (§4).

**(viii) Bank-qualified CONFIG** (a breakpoint or watch that fires only when its address resolves to a given
`romOffset`) is a new **param**, and §2.5 makes params closed, so it is a separate addition. **Wave B.** Until then the
contract says plainly that matching is bank-blind.

**(ix) Symbol-by-offset.** Resolve a banked PC through the listing's LMA rather than its VMA. This needs every banked
symbol to carry an LMA. Sigil's `Phase Table` does that for phased blocks only (`symbols.rs:72-126`), and an AS listing
does not. It is the eventual consumer of (v) on `symbol`/`symbolAtPc`. **Named, not scheduled.**

### 3.6 The bundling in debugread §5: is it right?

| Seam | Verdict | Why |
|---|---|---|
| (a) typed bank key | **Belongs here.** In this design it IS `romOffset` on the three READ fields | Same derivation as `region` (`cart_region`, `engine.rs:9943`), same question ("which ROM byte"), and §11.48 S1 assigns it to this row by name |
| (b) address-free image fingerprint (`status.romCrc32`) | **Split, as §11.48 S2 already ruled** | Different question: *which image is loaded*, not *which byte an address meant*. It has no address, no bank and no capture time. Its first concrete consumer is the MCP shim's freshness check on a >4 MiB or re-pointed image (§2.4, reasoned), which is real only once a banked ROM is driven through MCP. Bundling it would hold a trivial, independently useful key hostage to this CR's adoption, or the reverse |
| (c) readable SRAM space (`read {space:"sram"}`) | **Split.** Its own CR when a save-tooling consumer asks | Unrelated to address ambiguity: it adds a new *space* to `read`'s closed enum. Its consumer would be save-file tooling (`oracle-frontend/src/sram_file.rs` already owns the file side), not debugging. `write_memory` refuses SRAM too (`engine.rs:4631`), so a readable SRAM space invites a matching write question this row has no evidence for |

## 4. Recommendation: (v) `romOffset`, wave A now, wave B gated

**One CR, "wave A", in four parts.** Parts A1 and A4 need no consumer to justify them. A2 and A3 are small, and they
are the contract's own debt (§11.48 S1).

- **A1. The convention.** §2.4 gains `romOffset`, defined once, with its presence rule and the capture rule (vi).
- **A2. Where it is emitted in wave A:** the three READ fields (`read`, `read_memory`, `memory_hash`), which discharges
  S1, and the NOW `pc` fields where the reply *is* a stop or a paused look: `stopped`, `status`, `run_frames`,
  `run_to`, `step`, `step_over`, `step_out`, `play_input` and `wait_for_break`. **Not on `registers`**, which stays a
  pure register file (decision: `status.pc` already answers the same question).
- **A3. `status.cartBanks`:** the window table now, 8 integers, where index *k* is window *k* and index 0 is always
  `0`. This gives a client the mapping without an address to hang it on, for example before arming a breakpoint.
- **A4. The bus-view paragraph.** It makes the **HIST and CONFIG fields' bank-blindness normative, including the
  aggregation**, and it amends the profiler sentence that is false on banked carts today.

**Wave B**, booked and not proposed here, covers `romOffset` on HIST records via the journal (vii), per-bank profiler
and census keys, bank-qualified breakpoints and watches (viii), and symbol-by-offset (ix). **Its trigger is the hub's
own, verbatim: a banked ROM entering the suite**, or a client asking for historical attribution. Wave B adds fields to
the registered list and does not add a key. So *"the typed key ships once"* (S1) holds: one name, one definition, one
presence rule.

**Why this and not the others.** (i) is wrong on the HIST rows while looking right. (ii) breaks every consumer or
collides with the listing's 32-bit spelling. (iii) is the right *floor* and is contained in wave A (A3 + A4), but alone
it leaves S1 open. (iv) leaves a normative sentence false on the one banked ROM oracle is known to run. (v) is the only
option that gives a typed answer where the answer is cheap and certainly right (NOW and READ) and says out loud where it
is not (HIST). Wave A needs **no core change and no hot-path change**. `System::cart_peek(a)` (`system.rs`, already
serving `debug_read`) yields `CartByte::Rom(i)` for a ROM-backed address, and `cart_banks()` exists. It changes **no
frozen currency**: `export_state` and `state_hash` are untouched (reasoned).

**If the hub holds "grow only against a real consumer" strictly:** adopt **A4 alone** now, since it is prose and fixes
a false normative sentence, and hold A1-A3 behind wave B's trigger. That is option (iv) with the false sentence
corrected, and it is the one fallback I would defend.

**The player GUI (third surface), as a decision:** the implementing parcel shows `romOffset` beside the PC in the CPU
lens and the stop line **only when it differs from the address**. That is a display rule and not a wire rule; the wire
presence rule is A1's. The profile and watch lenses get the same bank-blind note the contract gets, as a panel
caption. **MCP:** no shim change is needed, because results pass through `json.dumps`. The shim's stale
`memory_hash` description is §8's booking, not this CR's.

## 5. Drafted contract wording: **DRAFT, NOT APPLIED**

Everything in this section is a proposal for the hub. None of it is in `../empyrean`, and none of it is served. Line
numbers are empyrean `0ebc923`'s `contract/protocol.md`. The proposed amendment number is **§11.51**, the next free
one after §11.50 (l.6011).

### 5.1 §2.4: a new shared convention, placed after `caveat` and before *The bounded-list rule* (l.636)

> #### `romOffset` — which ROM byte a cartridge address named *(added §11.51)*
>
> | Field | Type | Meaning |
> |---|---|---|
> | `romOffset` | hex string (D9 category 1) | OPTIONAL, handler-emitted, and only where the method's schema fragment declares it. The offset into the loaded ROM image of the byte that the 68000 bus resolved the reply's address to, **at the moment that address was captured**. |
>
> Five rules:
>
> 1. **Presence has one meaning.** `romOffset` is present **exactly** when the address resolved, at capture, to a
>    byte of the cartridge ROM image, through any cartridge bank mapper. It is absent for work RAM, for the
>    cartridge SRAM overlay's byte lane, for I/O and for open bus (which includes a window pointed past the image's
>    end). A server MUST NOT omit it for a ROM-backed address, **including one where it equals the address**.
>    Absence therefore never means "same as `addr`".
> 2. **Capture, not reply.** It is computed from the mapping in force when the fact was captured: for a stop, the
>    mapping at the stop; for a read, the mapping at the read. A server MUST NOT derive it from a later mapping. This
>    is why it appears only on the fields §6 registers for it, not on every address (see *Addresses are bus
>    addresses*, below).
> 3. **A range names its first byte.** On `read`, `read_memory` and `memory_hash`, whose range lies within one region
>    (§11.48), the range's bytes are the image's `romOffset … romOffset + len − 1`.
> 4. **Under the identity mapping it equals the address.** That is every unbanked cartridge, and every cartridge at
>    reset. A client that never meets a mapper may ignore it.
> 5. **It is the typed form of the bank.** A client that acts on which ROM byte an address meant MUST read
>    `romOffset` and MUST NOT parse `N` out of `region`'s `"cartridge ROM bank N"`. `region` stays the human-readable
>    provenance, together with §11.48 M1's exact-equality test for `"cartridge ROM"`. *(This discharges §11.48 S1.)*
>
> **Addresses are bus addresses, and a cartridge address without `romOffset` is bank-blind** *(added §11.51)*. Every
> 68000 address field on this bus names what the CPU drives on its 24 address lines. It is never an image offset.
> Where a cartridge bank mapper re-points a window (`$080000–$3FFFFF`), one address names different image bytes over
> time. A field whose reply carries `romOffset` is disambiguated by it. **Every other cart-space address is
> bank-blind, and that is normative, not an omission:** `watchpoint_hits.hits[].addr` / `.pc`, `$defs/watchStamp.pc`,
> `census[].key` under `censusKey: "addr"`, `get_profiler_frames`' `routines.items[].addr` and `callerAddr`, the
> armed addresses of `breakpoint_*`, `watchpoint_*` and `run_to.target`, and `lookup_symbol`'s addresses. In
> particular:
>
> - a breakpoint, a bus watch and a `run_to` target **match their bus address in every bank**;
> - profiler rows, caller edges, `addr` census keys and breakpoint `hits` **aggregate across banks**. Two routines at
>   one bus address in two banks are one row.
>
> A client that needs the bank behind a historical record can reconstruct it. It arms a `write` watch on
> `$A130F3–$A130FF` before the first re-point: each hit's register (`addr`) names the window (`($addr − $A130F1) / 2`),
> its `value` is the bank, and its `mclk` orders it against the record's.

### 5.2 §6 catalog rows (each a replacement of the current row's result cell, additions in **bold**)

- l.1117 `emulator/read`: `space`, `addr`, `len`, `bytes`, `region`?, **`romOffset`?**, `symbol`?, `symbolDisp`?, `caveat`?
- l.1118 `emulator/memory_hash`: `addr`, `len`, `region`, **`romOffset`?**, `fnv1a64`, `crc32`
- l.1119 `emulator/read_memory`: `addr`, `len`, `bytes`, `region`, **`romOffset`?**, `symbol`?, `symbolDisp`?
- l.1028-1030 `emulator/step` / `step_over` / `step_out`: `pc`, **`romOffset`?**, `symbol`?, `symbolDisp`?, …(unchanged)
- l.1031 `emulator/run_to`: `target`, **`reached`**, `pc`, **`romOffset`?** *(of `pc`, never of `target`)*, `maxFrames`, …
- l.1033 `emulator/run_frames` and l.1749 `emulator/play_input`: …, `pc`, **`romOffset`?**
- l.1034 `emulator/wait_for_break`: `pc`?, **`romOffset`?** *(absent whenever `pc` is)*, `symbol`?, …
- l.2110 `emulator/status`: `running`, `pc`, **`romOffset`?**, `sp`, `sr`, …, `romBytes`, **`cartBanks`**, …
- §3 `emulator/stopped` `params`: …, `pc`, **`romOffset`?**, …

**`status.cartBanks`, the added sentence (after the status table, l.2115):**

> **`cartBanks`** *(added §11.51)* is the cartridge bank mapper's window table **now**: an array of exactly 8 JSON
> numbers (D9 category 2), where index *k* is the image bank that 512 KiB window *k* (`$080000·k …`) shows. Index 0 is
> always `0`, because window 0 is fixed. At power-on and after a reset every entry is its own index. A server with no
> mapper answers the identity table. It is always present, because *"no mapper"* and *"identity"* are the same answer to
> the question the key asks.

**The profiler sentence (l.1974-1976), replacing** *"`addr` is a hex string (D9 category 1) of the routine's entry
address **masked to the 24 address lines the 68000 drives**, so a row key and a listing address compare directly
instead of through a client-side mask."* **with:**

> `addr` is a hex string (D9 category 1) of the routine's entry address **masked to the 24 address lines the 68000
> drives**, so a row key and a listing address compare directly instead of through a client-side mask. **Under a
> cartridge bank mapper this key is bank-blind** (§2.4, *Addresses are bus addresses*): routines entered at one bus
> address in different banks share one row, and a row key names an image byte only while that window is unmapped.

### 5.3 Schema diff sketch (DRAFT; against the vendored schema = empyrean `0ebc923`)

```diff
 "$defs": {
+  "romOffset": {
+    "allOf": [{ "$ref": "#/$defs/hex" }],
+    "description": "protocol.md §2.4 (§11.51). Offset into the loaded ROM image of the byte the bus resolved the sibling address to, AT CAPTURE. Present exactly when that address was ROM-backed; absence means RAM / SRAM lane / I/O / open bus, never 'same as addr'. Equals the address under the identity mapping."
+  },
   ...
 }
 "methods": {
   "emulator/read":        { "result": { "properties": { ...,
+      "romOffset": { "$ref": "#/$defs/romOffset" } } } },
   "emulator/read_memory": { ... same addition ... },
   "emulator/memory_hash": { ... same addition ... },
   "emulator/step" | "step_over" | "step_out" | "run_to" | "run_frames" | "play_input" | "wait_for_break":
+      "romOffset": { "$ref": "#/$defs/romOffset" },
   "emulator/status": { "result": {
+      "required": [ ..., "cartBanks" ],
       "properties": { ...,
+        "romOffset": { "$ref": "#/$defs/romOffset" },
+        "cartBanks": { "type": "array", "minItems": 8, "maxItems": 8,
+                       "prefixItems": [{ "const": 0 }],
+                       "items": { "type": "integer", "minimum": 0, "maximum": 255 },
+                       "description": "protocol.md §6 status (§11.51): the mapper window table NOW; index k = bank shown by window k; index 0 is fixed at 0." } } } }
 }
 "events": {
   "emulator/stopped": { "params": { "properties": { ...,
+      "romOffset": { "$ref": "#/$defs/romOffset" } } } }
 }
+ read / read_memory / memory_hash: in the existing `if space == bus then require region, else forbid region/symbol`
+ conditional, add `romOffset` to the forbidden set of the ELSE branch (a VDP-space read has no ROM offset).
+ wait_for_break: `"dependentRequired": { "romOffset": ["pc"] }`, since romOffset without pc is meaningless.
```

**Additivity, in §11.18's form:** 12 leaves are added (11 `romOffset` + `cartBanks`) and 1 `$def`. Leaves removed:
none. One REQUIRED key is added (`status.cartBanks`). That is additive for every client (§2.4: none validates result
key sets) and REQUIRED of every server; the reference server is the only one serving `status`. One description string
is reworded (profiler `addr`). Vendoring cost: one re-vendor of the schema in the implementing parcel's merge window,
the §11.48 procedure.

### 5.4 Conformance vectors (DRAFT)

**Fixture.** This is §11.48's own fixture, unchanged: ten 512 KiB banks, with the byte at image offset `i` equal to
`((i>>19) ^ 0x5A) ^ mix(i & $7FFFF)`, and window *k* re-pointed by an odd-byte write of `N` to `$A130F1 + 2k`. **Added
for the NOW vectors:** `bra.s *` (`$60FE`) at image offsets `$180010` (bank 3) and `$480010` (bank 9), and a copy of
it placed in work RAM at `$FF0000` by the boot code.

**Normative:**

1. Unbanked, `read {addr:$000100, len:4}` → `region "cartridge ROM"`, `romOffset "0x00000100"`.
2. Window 1 → bank 9: `read {addr:$080040, len:32}` → `region "cartridge ROM bank 9"`, `romOffset "0x00480040"`,
   `bytes` = file[`$480040`..+32]. `read_memory` gives the identical result. `memory_hash` over the same range gives
   the same `romOffset`, and `crc32` = CRC32(file[`$480040`..+32]).
3. `read {addr:$FF0000}` → `region "work RAM"`, **no** `romOffset`.
4. SRAM mapped in (fallback `$200001-$20FFFF`, odd), window 4 → bank 9: `read {addr:$200001}` → `region "cartridge
   SRAM"`, **no** `romOffset`. `read {addr:$200000}` → `region "cartridge ROM bank 9"`, `romOffset "0x00480000"`.
5. Windows *k* → *k*+2 for *k* = 1..7: `status.cartBanks` = `[0,3,4,5,6,7,8,9]` (eight entries; window 0 is fixed at
   0). After `emulator/reset`: `[0,1,2,3,4,5,6,7]`.
6. Window 1 → bank 9, `run_to {addr:$080010}` → `reached: true`, `pc "0x00080010"`, `romOffset "0x00480010"`. The
   `stopped` event carries the same `pc` and `romOffset`. A following `step` reply carries `pc "0x00080010"`,
   `romOffset "0x00480010"`.
7. Window 1 → bank 3, same `run_to {addr:$080010}` → `pc "0x00080010"`, `romOffset "0x00180010"`. **Same `pc` as
   vector 6, different `romOffset`. This pair is the whole point of the change.**
8. PC in work RAM (the machine parked on the `$FF0000` copy): `status.pc "0x00FF0000"`, **no** `romOffset`. The
   `stopped` event for that halt has none either.

**Informative (the reference server's behaviour, pinned so that wave B changing it is a visible decision):**

9. `breakpoint_add {addr:$080010}`, then run code reaching `$080010` once with window 1 → bank 3 and once with
   window 1 → bank 9: the breakpoint fires both times, and `breakpoint_list.hits` = 2.
10. Profiling the same two entries gives **one** `routines` row at `addr "0x00080010"`, with `calls` = 2.

## 6. Test plan for the implementing parcel (described, NOT written)

These are all hermetic: the fixture is built in-process like `crates/oracle-aether/tests/debug_read_banked.rs`, and no
cart outside the repo is used. Each expectation is **derived by hand** (`N * 0x80000 + (addr & 0x7FFFF)`, and `addr`
itself for window 0), never read back through `CartBanks::rom_offset` and never compared with `CartBanks::IDENTITY`.
That is the mapper note's M3 lesson (*"an assertion against the constant under test is circular"*) and debugread's
M1/M2/M5 lesson (a parity pair between two surfaces sharing one function cannot see that function's defect). Each
fixture first asserts that its banks differ.

| Gate | What it proves | Red-first: how it is shown failing before the fix |
|---|---|---|
| **T1 READ carries `romOffset`** | vectors 1, 2, 3, 4 on `read`, `read_memory` and `memory_hash` | Run against the unchanged server. Predicted, and written down before the run: vectors 1 and 2 and the `$200000` leg of 4 go red on the missing key, while vector 3 and the SRAM leg of 4 pass, because absence is today's behaviour. Mutation M-a: emit `romOffset = addr` (identity). Vector 2 goes red and vector 1 stays green, which is why the vector set must include both |
| **T2 presence rule, one meaning** | present with `romOffset == addr` in window 0; absent on RAM, SRAM lane and I/O | M-b: "emit only when it differs from `addr`" → vector 1 red. M-c: "always emit" → vectors 3 and 4 red, **and §8 item 20's schema closure stays green** (the key is declared), which is why the test asserts absence by name |
| **T3 `status.cartBanks`** | vector 5; reset returns it to per-index identity | Unchanged server: `cartBanks` is missing, so it goes red. M-d: a window off-by-one in the emitter (`banks[k+1]`) goes red **only because all seven windows are re-pointed k→k+2 at once**, the debugread fixture discipline. A single re-pointed window can let an off-by-one pass |
| **T4 NOW `pc` carries `romOffset`** | vectors 6, 7, 8 across `run_to`, `step`, `stopped`, `status` and `wait_for_break` | Unchanged server: absent, so red. Mutation M-e, "derive from the table as of the reply rather than the stop", is **not observable in wave A**, and the test plan says so instead of faking a gate. A paused machine cannot re-point a window: `write_memory` refuses `$A130Fx` (`engine.rs:4631`), and the stop, the `stopped` event and the reply sit in one handler call. Rule 2 binds wave B. Its gate is a journal test there: a hit captured before a re-point must keep the old bank |
| **T5 vector 7 vs 6** | same `pc`, different `romOffset` | Included in T4, and called out on its own because it is the only assertion that fails if the implementation emits a constant bank |
| **T6 schema closure and re-vendor** | every emitted `romOffset` / `cartBanks` validates against the re-vendored fragments (§8 item 20) | Emit the keys **before** re-vendoring: the harness's closure goes red with a surplus key on every NOW method. That is the proof the closure sees the new keys at all. Then re-vendor, and it goes green |
| **T7 informative pins** | vectors 9 and 10 (bank-blind breakpoints; one merged profiler row) | These are characterization tests. They pass on today's code, and their red-first is a **mutation that makes them bank-aware** (key the breakpoint on `(addr, bank)`): they then fail. They exist so wave B's change shows up as a deliberate edit to them |
| **T8 MCP sweep untouched** | `mcp_tool_sweep.rs` stays green (no **param** added) | Nothing to show red. Stated so nobody reads its green as coverage of the new keys |
| **T9 GUI parity** | the CPU lens and stop line show `@ $offset` only when it differs from the PC (the §4 display rule) | A player unit test on the formatter, red before the formatter exists. Screen text through `screen_text` for the foreground check (TAG-B1) |

**Runner:** `cargo test -p oracle-aether --test <new file>`, the player's `cargo test -p oracle-player`, and
`tools/land.sh` for the full leg count. A new test target adds one leg. Each red run is checked for `Compiling
oracle-…` before its result is read. The mutations are applied, quoted back from disk and restored with `git checkout
--`, per this lane's standing record format.

## 7. What would have to be true for this recommendation to be wrong

1. **A banked ROM enters the suite and runs code from windows 1-7.** Then wave B is needed at once. Shipping wave A
   first costs a second schema pass, although the key's definition does not change. *Check:* TAG-B1, and sigil's or
   aeon's queues for a mapper-using build. None exists at aeon `63013da` (§1).
2. **The hub reads S1's "ships once" as "every field in one adoption".** Then wave A and wave B merge into one CR, and
   the gating argument in §4 falls to the fallback (A4 alone now).
3. **A client validates result key sets strictly.** Then `romOffset` and `cartBanks` break it. *Checked for aurora,
   aeon and the shim (§2.4): none does.* A consumer outside those three was not searched (seraph, sigil and
   `empyrean/clients/python` were not read). Sigil and seraph do not drive `emulator/*` addresses as far as the §11.48
   consumer set records (protocol l.5885-5891). That record is the hub's, dated 2026-09-11, and was not re-run here.
4. **Some NOW field is not actually sampled at one instant with the mapping.** If a handler read `pc` from an earlier
   record while the table moved in between, rule 2 would make the server wrong. Read here: every NOW row samples a
   paused machine or its own stop record (§2.2). A checkpoint `restore` or a `reset` moves the PC and the table
   together, so a later `status` stays consistent. *T4's vectors 6 and 7 would catch a handler that mixed them.*
5. **A mapper other than SSF2 arrives with windows that do not map linearly onto image offsets** (for example SRAM-
   or register-backed windows). Then "offset into the loaded ROM image" can be undefined for a cart byte that is
   nevertheless not RAM. Rule 1's absence already covers it as "not ROM-backed". But if such bytes need provenance,
   `romOffset` alone would not carry it, and `region` would be the place.
6. **`bank` turns out to be what humans want to read.** `region` already says `"cartridge ROM bank N"` for humans,
   and the GUI display rule shows the offset. If owner looks show that people want `bank 9` rather than `$480010`, the
   GUI changes its formatting, not the wire.
7. **The acceptance ROM's mapper writes are per-frame** (TAG-B2). Then (vii)'s journal is unbounded in practice, and
   wave B needs per-record capture in the sink after all. That is a hot-path cost to measure, but it does not change
   wave A.

## 8. Tags for the foreground, bookings, and observations (not fixed)

- **TAG-B1 (runtime look, emulator required, so NOT done here).** On *Sonic Delta Origins*, does the PC ever sit in
  `$080000-$3FFFFF` while that window is re-pointed? This decides whether any real image makes the NOW rows ambiguous
  *in practice*, and whether profiler conflation (§2.2 rows 16-17) is observable on it. Probe: profile a few hundred
  frames past the title and list any `routines.items[].addr >= 0x080000`. Or arm a `write` watch on
  `$A130F3-$A130FF` and correlate it with `stopped.pc`.
- **TAG-B2 (runtime look).** How often does that ROM write the bank registers, once per scene or per frame? Arm a
  `write` watch on `$A130F3-$A130FF` in `census` mode keyed by `addr`, over a long play segment. It sizes (vii)'s
  journal.
- **TAG-B3 (runtime look, MCP).** Expected, from reasoning only: the shim's ROM freshness reports `"unmeasurable"` on
  *Sonic Delta Origins* (5 MiB > the 4 MiB chunk). If it ever reports `"stale"` for a fresh file, §11.48's straddle
  refusal is not what reaches the shim.
- **Booking, suggested `F-MCP-HASH-DESC-STALE`.** `oracle-old` `1eb09a9` `linux-port/mcp/oracle_mcp.py:478-487`
  still describes `memory_hash` as *"one of two regions"* with an unconditional file-slice promise. §11.48 made both
  false for banked images. It is the live MCP (`~/.claude.json`), and the fix is a text edit in a frozen reference
  repo, which is an owner-level question about who edits `oracle-old`.
- **Observation:** `watchpoints.rs:508` describes `seq` as *"a monotonic id assigned to every matched access in
  order"*. I read that as one counter across all watches, which is what §5.1's reconstruction recipe needs (it uses
  `mclk`, which is monotonic by construction, so the recipe does not depend on it). Not measured.

**Claims in this doc that were NOT measured (reasoned only):**
- the HIST structures being bank-blind (read from the types, `watchpoints.rs` / `profiler.rs` / `breakpoints.rs`);
- the profiler really merging two banks' routines into one row (read from the key type; no banked run was made);
- a `write` watch on `$A130F3-$A130FF` recording mapper writes (inferred from the measured `$A130F1` watch and the
  shared write arm);
- the MCP shim answering `"unmeasurable"` on a >4 MiB or re-pointed image (read from its code; not run);
- "an additive result key breaks nothing" for aurora, aeon and the shim (read from their call sites; seraph, sigil and
  `empyrean/clients/python` were not read);
- wave A needing no core or hot-path change and moving no frozen currency (read, not built);
- `seq` being one counter across watches (read from a doc comment).

```detectors
ABSENCE: outside oracle-core, nothing but debug_read reads the cartridge bank table (so no other surface can report a bank)
  instrument: git -C <worktree> grep -n -E 'cart_banks\(\)' HEAD -- crates/oracle-aether/src crates/oracle-player/src crates/oracle-frontend/src crates/oracle-replay/src crates/oracle-panels-spike  (1 line: engine.rs:10013, inside debug_read)
  positive:   git -C <worktree> grep -c -E 'cart_banks\(\)' HEAD -- crates/oracle-core/src/system.rs -> 6
  negative:   git -C <worktree> grep -c -E 'cart_banks\(\)' HEAD -- crates/oracle-aether/src/breakpoints.rs -> 0
  scope:      every tracked file under the five non-core crate source roots at oracle HEAD 094f847 (git grep over the commit, not the working tree)
  contains:   engine.rs (the whole Aether server, every emit site in §2.2) is inside crates/oracle-aether/src; the player's memory.rs and the frontend lenses of §2.3 are inside the other two roots

ABSENCE: no typed bank or ROM-offset key exists in any crate or in the vendored schema
  instrument: git -C <worktree> grep -c -E '"(bank|banks|cartBanks|romOffset|romCrc32|imageOffset)"' HEAD -- crates -> 0
  positive:   git -C <worktree> grep -c -E '"(region|romBytes)"' HEAD -- crates/oracle-aether/tests/contract/bus-protocol.schema.json -> 11
  negative:   git -C <worktree> grep -c -E '"(bank|banks|cartBanks|romOffset|romCrc32|imageOffset)"' HEAD -- crates/oracle-aether/tests/contract/bus-protocol.schema.json -> 0
  scope:      every tracked file under crates/ at oracle HEAD 094f847, which includes the vendored schema
  contains:   the positive runs the same quoted-key syntax on the vendored schema, whose region/romBytes keys are the §11.48 neighbours a bank key would sit beside

ABSENCE: no suite ROM source writes or names a cartridge mapper register (aeon does not bank)
  instrument: git -C ../aeon grep -c -i -E 'A130' 63013da22ab69da6c72c1ea3409bbd14d5398c7a -- engine games tools -> 0
  positive:   git -C <worktree> grep -c -i -E 'A130F[3579BDF]' HEAD -- crates/oracle-core/src/bus.rs -> 11
  negative:   git -C <worktree> grep -c -i -E 'A130F[3579BDF]' HEAD -- crates/oracle-aether/src/breakpoints.rs -> 0
  scope:      aeon origin/master 63013da, engine/ + games/ + tools/: 210 .emp/.asm files (73 + 133 + 4, git ls-tree), which is every .emp/.asm in the tree
  contains:   the git ls-tree count over the whole commit found .emp/.asm only under those three roots, so the ROM's whole source is inside scope

ABSENCE: aurora's source reads no history-bearing result (watch hits, profiler rows, breakpoints, wait_for_break)
  instrument: git -C ../aurora grep -c -E 'watchpoint_hits|get_profiler_frames|breakpoint_add|breakpoint_list|callerAddr|wait_for_break' 44ba3d7f6d17bf481d47aa15cd35274fe6416a71 -- src -> 0
  positive:   git -C ../aeon grep -c -E 'watchpoint_hits|get_profiler_frames|breakpoint_add|breakpoint_list|callerAddr|wait_for_break' 63013da22ab69da6c72c1ea3409bbd14d5398c7a -- tools/parallax_cost_probe.py -> 4
  negative:   git -C ../aurora grep -c -E 'watchpoint_hits|get_profiler_frames' 44ba3d7f6d17bf481d47aa15cd35274fe6416a71 -- src/renderer -> 0
  scope:      aurora origin/master 44ba3d7, all of src/ (main, core, renderer); scratchpad/ and test/live are excluded as non-shipping
  contains:   aurora's Aether client (src/main/aether/client.ts) and every caller of it are under src/

ABSENCE: the live MCP shim never reads `region` from a reply
  instrument: git -C ../oracle-old grep -c -E 'get\("region"\)|\["region"\]' 1eb09a989effad1ea42839e877a1dbf2b418b68d -- linux-port/mcp/oracle_mcp.py -> 0
  positive:   git -C ../oracle-old grep -c -E 'get\("crc32"\)' 1eb09a989effad1ea42839e877a1dbf2b418b68d -- linux-port/mcp/oracle_mcp.py -> 1
  negative:   git -C ../oracle-old grep -c -E 'get\("region"\)|\["region"\]' 1eb09a989effad1ea42839e877a1dbf2b418b68d -- linux-port/gui/ControlSocket.cpp -> 0
  scope:      the one file the shim is, at oracle-old HEAD 1eb09a9 (= mcp_tool_sweep.rs PIN_REV), read from git objects
  contains:   ~/.claude.json registers oracle-old/linux-port/mcp/oracle-mcp, the wrapper for this file; the positive hits the reply-key reader its freshness check uses (:1521)

ABSENCE: no served method is a trace record
  instrument: grep -E '^\s+name: "emulator/' crates/oracle-aether/src/engine.rs | grep -c -i trace -> 0
  positive:   grep -E '^\s+name: "emulator/' crates/oracle-aether/src/engine.rs | grep -c -i step -> 3
  negative:   grep -E '^\s+name: "emulator/' crates/oracle-aether/src/engine.rs | grep -c -i scanline_trace -> 0
  scope:      the 62 MethodSpec names in engine::METHODS at HEAD 094f847, the server's whole served list
  contains:   the positive's three step rows (step, step_over, step_out) are enumerated from the same list

HEURISTIC: a parity assertion between two surfaces that share one derivation cannot see a defect in that derivation, so T1-T5 assert against the hand-written formula
  from:    docs/2026-09-11-debugread-banked.md §7 (mutations M1/M2/M5: parity passed and the derived assertion failed)
  assumes: the implementing parcel derives romOffset through the same cart_peek/cart_decode path that region and read's bytes already use, so any read-vs-status parity pair shares that path
  checked: 094f847:docs/2026-09-11-debugread-banked.md:340 "A parity pair between surfaces that share one"

CANNOT-TEST: whether any real image runs code from a re-pointed window, and how often it writes the bank registers (TAG-B1, TAG-B2)
  attempted: none — the brief's invariant 1 forbids any emulator or MCP call from a background agent, and both need a runtime look
  cost:      not measured — the probe is a foreground profiler run plus one census watch on the acceptance ROM, TAGged in §8 for the overseer
  prior:     docs/2026-09-11-cart-mapper-design.md
```

## Appendix A: the two enumeration scripts, verbatim (re-run from the repo root)

`python3 walk.py crates/oracle-aether/tests/contract/bus-protocol.schema.json hex` (then `int`, `str`):

```python
import json, sys
s = json.load(open(sys.argv[1]))
out = []
HEX = '#/$defs/hex'
def is_hex(n):
    if not isinstance(n, dict): return False
    if n.get('$ref') == HEX: return True
    for k in ('allOf', 'anyOf', 'oneOf'):
        if any(isinstance(x, dict) and x.get('$ref') == HEX for x in n.get(k, [])): return True
    it = n.get('items')
    return isinstance(it, dict) and it.get('$ref') == HEX
def walk(node, path, name=None):
    if isinstance(node, dict):
        if name is not None:
            if is_hex(node): out.append(('hex', path))
            elif node.get('type') == 'integer': out.append(('int', path))
            elif node.get('type') == 'string': out.append(('str', path))
        for k, v in node.items():
            if k == 'properties' and isinstance(v, dict):
                for pn, pv in v.items(): walk(pv, path + '.' + pn, pn)
            elif k == 'items' and isinstance(v, dict):
                walk(v, path + '[]', None)
            elif k in ('params', 'result') and isinstance(v, dict):
                walk(v, path + ' ' + k + ':', None)
            elif isinstance(v, (dict, list)):
                walk(v, path, None)
    elif isinstance(node, list):
        for x in node: walk(x, path, None)
for sec in ('methods', 'events', '$defs'):
    for k, v in s[sec].items():
        if k == '$comment': continue
        walk(v, sec + ':' + k)
seen = []
for e in out:
    if e not in seen: seen.append(e)
kind = sys.argv[2] if len(sys.argv) > 2 else None
for k, p in seen:
    if kind is None or k == kind: print(k, p)
```

`python3 attrib.py crates/oracle-aether/src/{engine,decoders,objreq}.rs` (one file per call):

```python
import re, sys
path = sys.argv[1]
tests = '--tests' in sys.argv
fn_re = re.compile(r'^\s*(?:pub(?:\([a-z]+\))?\s+)?fn\s+([a-zA-Z0-9_]+)')
emit = re.compile(r'"([A-Za-z0-9_]+)"(?:\.into\(\))?\s*(?:[:,]|\]\s*=)\s*(?:json!\()?\s*(?:crate::)?hex::addr\(')
census = re.compile(r'json!\(\{"key":')
cur = '?'
for n, line in enumerate(open(path), 1):
    if not tests and line.startswith('#[cfg(test)]'):
        break
    m = fn_re.match(line)
    if m: cur = m.group(1)
    s = line.strip()
    if s.startswith('//'): continue
    for k in emit.findall(line):
        print(f'{path}:{n}\t{cur}\t{k}')
    if census.search(line):
        print(f'{path}:{n}\t{cur}\tkey(census,int)')
```

