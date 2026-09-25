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
