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
