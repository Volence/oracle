**Kind:** investigation

# §11.51 wire check: "Addresses are bus addresses, and a cartridge address is bank-blind"

**Date:** 2026-09-25 · **Branch:** `parcel/banked-wire-check` · **Oracle base:** `fb6ca8a` (no `crates/` change
since the recon's `094f847`, measured: `git diff --stat 094f847 HEAD -- crates` is empty).
**Text under test:** empyrean `7040319f:contract/protocol.md` l.636-656 (§2.4), l.1996-2001 (§6 profiler `addr`)
and the §11.51 entry, read with `git show` only. The §6 word was then amended at `93740f63`, which was also read.
**Nothing was applied to empyrean. No emulator or MCP was used.** Measurements are in-process `cargo test`
runs against a server spawned in-test. Each verdict below says whether it was **measured** or **reasoned**.

## What's going on, in plain words

The hub added a paragraph to the shared contract. It says every address the debugger reports is a plain CPU
address that ignores which ROM slice was switched in, and it tells a client how to work out the slice
afterwards: watch the slice-switch registers. I checked each sentence against what oracle actually sends.
Every sentence is true except one part of the recipe. It works when a game switches slices with a one-byte
write, which is what the one slice-switching ROM we have does. When the switch is a two-byte write, the recipe
either misses the switch entirely or names the wrong window, and oracle honours two-byte writes. A short CR
below fixes the recipe wording. Nothing in oracle needs to change.

## Claims

| # | Claim (§2.4 at `7040319f` unless noted) | Verdict | Evidence |
|---|---|---|---|
| 1a | Each **listed** field is on the wire as named: `stopped`/`status`/`step*`/`run_to`/`run_frames`/`play_input`/`wait_for_break` `pc`; `watchpoint_hits.hits[].addr`/`.pc`; `$defs/watchStamp.pc`; `census[].key` under `censusKey:"addr"`; `routines.items[].addr` and `callerAddr`; the armed addresses of `breakpoint_*`, `watchpoint_*`, `run_to.target`; `lookup_symbol`'s addresses | **TRUE** (measured, schema and code) | A walk of the vendored schema's result hex leaves finds every one of them. `step*` = `step`, `step_over`, `step_out`, and all three return `pc` through one `halt_result` (`engine.rs:4456`, called at `:4355/:4416/:4439`). `census[].key` is the one integer-typed address, and `censusKey` is `addr`/`value`/`via` only (`engine.rs:9523-9531`, schema enum). `callerAddr` sits at `routines.items[].callers[].callerAddr` (`engine.rs:5727`). |
| 1b | Each listed field is bank-blind | **TRUE** (measured for `stopped.pc`, `breakpoint` `addr`, `run_to`, `hits[].addr/.pc`; reasoned for the rest) | Every one is a bare `u32` bus address: `WatchHit{addr,pc}` (`watchpoints.rs:515-544`), `Stamp.pc` (`:294-302`), census `key_of` Addr → `hit.addr` (`:247`), profiler `BTreeMap<u32,_>` (`profiler.rs:427,437`). `cart_banks()` has exactly one reader outside the core, `debug_read` (`engine.rs:10013`). |
| 1c | The list is complete | **n/a: the list illustrates the rule and does not bound it** (hub ruling). Omissions are notes, not defects. | See *Notes* §1. |
| 2 | "The only replies that say which image bytes an address meant are `read`, `read_memory` and `memory_hash`, through `region`" | **TRUE** (measured) | `"region"` is emitted at exactly three sites: `engine.rs:4541` (`read_memory`), `:4696` (`read`), `:5294` (`memory_hash`). Only those three result fragments in the schema carry `region`. No `bank`/`cartBanks`/`romOffset` key exists anywhere in `crates/oracle-aether/src` (detectors). Qualifier: `read` carries `region` only for `space:"bus"` (`:4693`). `read` and `read_memory` also carry a `caveat` naming the image offset, but those are the same replies, so the claim stands. |
| 3 | "a breakpoint, a bus watch and a `run_to` target **match their bus address in every bank**" | **TRUE** (measured) | Code: `first_enabled_at` compares `b.addr == addr` (`breakpoints.rs:145`), `run_to` tests `pc == target` (`engine.rs:4126`), and a watch tests `(lo..=hi).contains(&hit.addr)` against the bus event's address (`watchpoints.rs:458-465`, `:1005`). Measured (`bank_blind_wire.rs`): with window 1 re-pointed to bank 8, a breakpoint at `$080010` stops with `pc 0x00080010`, and `run_to` reaches it. A **read** watch on `$080000` fires with `addr 0x00080000` and bank 8's byte (`$B8`). A read watch on `$080010` catches the `fc 6` **fetch**, with bank 8's `$4E75` rather than bank 1's `$4E71`. So a watch on ROM space does see banked reads and fetches, keyed by the bus address. |
| 4 | "profiler rows, caller edges, `addr` census keys and breakpoint `hits` **aggregate across banks**" | **TRUE** (reasoned) | Rows `BTreeMap<u32, Counts>` and edges `BTreeMap<(u32, CallerKey), _>` (`profiler.rs:427,437,467-470`). Census key = `hit.addr as u64` (`watchpoints.rs:247`). `hits` is counted per breakpoint whose `addr` equals the pc (`breakpoints.rs:164`). None of these types has a bank. |
| 5a | The recipe's watch can be armed on `$A130F3–$A130FF`, and a hit has `addr` = the register, `value` = the bank, and `mclk` | **TRUE for byte writes** (measured) | `watchpoint_add` refuses a bus range only past `$FFFFFF` (`engine.rs:8521`). `move.b #8,($A130F3).l` hits with `addr 0x00A130F3`, `size 1`, `value 0x08`, `mclk 252`. The hits come in `mclk` order and precede the record's `mclk` (980). |
| 5b | Formula: window `n`'s register is `$A130F1 + 2n`, so `($A130F3 − $A130F1)/2 = 1` | **TRUE** (reasoned; the gate asserts it against `region`) | `CartBanks::window_of_register` (`bus.rs:860-865`) uses the same formula. |
| 5c | The recipe as a whole: "each hit's register (`addr`) names the window … its `value` is the bank" | **FALSE for word and long writes** (measured) | The bus honours a word write to the even neighbour: its low byte lands on the register (`bus.rs:1339-1347`, `write16` `:1616-1623`). The bus emits **one** event at the **even** address with the whole word (`:1623`). Measured: `move.w #7,($A130F2).l` re-points window 1 but lies outside `$A130F3–$A130FF`, so the watch **never sees it**. `move.w #$0109,($A130F4).l` hits with `addr 0x00A130F4`, which gives window **1.5** (truncated, 1, but the true window is 2), and `value 0x0109`, whose bank is only the low byte (9). `move.l #$00050006,($A130F6).l` gives **two** word hits (`$A130F6`, `$A130F8`) with the **same** `mclk` (532), so only `seq` orders them. |
| 5d | Do games write these registers as bytes or words? | **Bytes, in the one banked image available** (measured) | *Sonic Delta Origins* has 20 absolute-long `move.b`/`clr.b` writes to odd `$A130F3–FF` and 0 word/long writes to even `$A130F2–FE` (detectors). Its only word writes to `$A130Fx` target `$A130F0` (the SRAM latch). So the recipe works on today's acceptance ROM. It fails on any cart whose code uses `move.w`/`move.l`, which oracle models. |
| 6 | "Under the identity mapping … none of this changes anything" | **TRUE** (reasoned) | Power-on and `reset()` set `CartBanks::IDENTITY` (`system.rs:719,884`). Under it `rom_offset(a) = a` for all of `$000000-$3FFFFF` (`bus.rs:845-847`). The SRAM overlay is a separate cart-space ambiguity, and §11.51 already books it apart. |
| 7 | §6: "a row key names an image byte only while that window is **unmapped**" | **Was wrong. Fixed upstream; verified** | "Unmapped" contradicted the paragraph's own term. empyrean `93740f63` is a one-line diff to *"only while that window is at its identity mapping"*, matching §2.4's *"Under the identity mapping"* (l.654 at `93740f63`). No CR. |

## Notes (omissions and edges, recorded, not CR'd)

1. **Cart-capable fields the list does not name.** The first sentence covers all of these:
   - `registers.pc` (`engine.rs:4079`, the same NOW value as `status.pc`);
   - the derived `symbol`/`symbolDisp`/`symbolAtPc` on `stopped`, `step*`, `run_to`, `wait_for_break`, `status` and on hits;
   - `breakpoint_set_enabled.hits`;
   - `object_list`/`player_state`/`object_slot`'s `decodedSlot.name`/`nameDisp`, a symbol resolved from `ObjCodeBase + code`, which is a code pointer (`decoders.rs:699`).

   Values that may *hold* a cart pointer are excluded as the recon excluded them: `registers.a0-a7`/`d*` and
   `decodedSlot.code` (*"not a resolved address"*). There is **no** Z80-side 68k-window address on the wire:
   `z80_read` serves only Z80 RAM (`engine.rs:10124-10128`). No trace method is served. `sprites.satBase` is VRAM.
2. **`lookup_symbol.rawAddr` is a listed field that is not a 24-line bus address.** It is the listing's unmasked
   spelling (e.g. `0xFFFF8CFA`), present only where it differs from `addr`, so it is never a cart-space value.
   It is bank-blind trivially. The first sentence's *"names what the CPU drives on its 24 address lines"* is
   literally false of it. This is harmless, and it is outside the narrowed CR scope.
3. **An overlapping watch can take the recipe's hits.** An access matched by several watches is recorded once,
   under the earliest-armed one (`watchpoints.rs:516-517`). Measured in the probe: a `$A130F2–FF` watch armed
   after a `$A130F3–FF` watch reported `matched 5` but listed only 1 hit under its own handle. A client
   following the recipe must read hits unfiltered, or arm no earlier overlapping watch. The recipe also needs
   `dropped == 0` over the span it reconstructs. This is a watch-surface property and not specific to §11.51.
   It is noted, and not put in the CR.

## DRAFT CR: NOT APPLIED. For the hub, against §2.4 at `93740f63` l.652-654

**Replace** the sentence

> A client that needs the bank behind a record can reconstruct it: arm a `write` watch on `$A130F3–$A130FF`
> before the first re-point; each hit's register (`addr`) names the window (`($addr − $A130F1) / 2`), its `value`
> is the bank, and its `mclk` orders it against the record's.

**with**

> A client that needs the bank behind a record can reconstruct it: arm a `write` watch on `$A130F2–$A130FF`
> before the first re-point. A hit re-points a window only if it reaches an odd register byte. A byte hit
> (`size` 1) at an odd `addr` reaches register `r = addr`. A word hit (`size` 2) at an even `addr` puts its low
> byte on `r = addr + 1`, which is how `move.w` and each half of a `move.l` arrive. A byte hit at an even address
> re-points nothing. The window is `(r − $A130F1) / 2`, the bank is the **low byte** of `value`, and `mclk`, then
> `seq` (the two halves of a long write share one `mclk`), orders it against the record's.

**Ground:** measured by `bank_blind_wire.rs` at this branch (claim 5c). The published range misses a word
write to `$A130F2` (window 1), and its formula gives a half-integer window and a whole-word "bank" for every
other word write. **Additivity:** prose only. No schema leaf changes, and no server behaviour changes. Oracle
already emits every fact the corrected recipe reads.

## The gate kept: `crates/oracle-aether/tests/bank_blind_wire.rs`

Three tests run over one in-memory 5 MiB image. The program re-points windows 1-4 with a `move.w` to `$A130F2`,
a `move.b` to `$A130F3`, a `move.w` to `$A130F4` and a `move.l` to `$A130F6`, then reads `$080000` and `jsr`s to
`$080010`. The expected banks come from the server's own `region` at `k * CART_BANK_SIZE`, never from a typed
constant.

- `the_register_watch_reconstructs_every_window_when_word_hits_are_read_at_addr_plus_one`: the corrected recipe
  rebuilds all 8 windows, and the hits come in `mclk` order before the record.
- `the_recipe_as_first_published_mis_reads_word_writes`: pins claim 5c. This is server behaviour, so it stays
  true after the CR lands.
- `breakpoints_and_bus_watches_match_the_bus_address_in_a_re_pointed_window`: pins claim 3.

**Red-first**, each mutation applied on disk (`git diff --numstat` showed `1 1`), run, then restored from
committed `HEAD` with `git checkout HEAD -- <file>`:

- **A.** `watchpoints.rs:1005` `addr: event.addr & !1`: recipe test and v1 test **FAILED** (window 1 rebuilt 7,
  truth 8), bank-blind test ok. 1 passed, 2 failed.
- **B.** `bus.rs:1346` bank write dropped: recipe test **FAILED** (vacuity guard: truth is identity) and bank-blind
  test **FAILED** (`window 1 must be re-pointed`). 1 passed, 2 failed.
- **C.** `engine.rs:9898` region label `bank + 1`: recipe test **FAILED** (`[0,8,9,5,6,5,6,7]` vs
  `[0,9,10,6,7,5,6,7]`) and bank-blind test **FAILED** (fetched `$4E75`, derived 0). 1 passed, 2 failed.

**Totals.**
- Green at baseline: 3 passed, 0 failed.
- Full `cargo test -p oracle-aether`, with `vendor/` symlinked in (and removed afterwards): exit 0, 669 passed,
  0 failed, 2 ignored, over 52 test binaries.
- `cargo fmt --check`: exit 0. `cargo clippy -p oracle-aether --all-targets -- -D warnings`: exit 0. Both exit
  codes were captured outside a pipe.

```detectors
ABSENCE: no reply outside read/read_memory/memory_hash carries a bank-bearing key (region's siblings bank/cartBanks/romOffset do not exist in the server)
  instrument: git grep -c -E '"(bank|banks|cartBanks|romOffset|imageOffset)"' HEAD -- crates/oracle-aether/src -> 0
  positive:   git grep -c -E '"region"' HEAD -- crates/oracle-aether/src/engine.rs -> 3
  negative:   git grep -c -E '"(bank|banks|cartBanks|romOffset|imageOffset)"' HEAD -- crates/oracle-aether/src/breakpoints.rs -> 0
  scope:      every tracked file under crates/oracle-aether/src at oracle fb6ca8a (git grep over the commit)
  contains:   engine.rs, the whole Aether server and every emit site in the claim table, is inside that root; the three region emits the positive counts are engine.rs:4541/4696/5294

ABSENCE: the acceptance ROM never writes a bank register with a word or long write (so the published recipe works on it)
  instrument: python3 mapw.py word 'Sonic Delta Origins.bin' -> 0
  positive:   python3 mapw.py word pos.bin -> 1
  negative:   python3 mapw.py word vendor/TestRoms/m68k_memory_test.bin -> 0
  scope:      absolute-long move.w/move.l #imm, move.w/move.l Dn and clr.w/clr.l to an even $A130F2-$A130FE, over all 5,242,880 bytes; pos.bin is the 8 bytes 33FC 0007 00A1 30F2 (the gate's own word write); mapw.py lives in this session's scratchpad
  contains:   the same scan with `byte` finds the ROM's 20 odd-register byte writes (move.b #n,($A130F3..FF)), and a raw scan for 00A130Fx operands finds 30 sites, all of which are those byte writes, $A130F0/F1 latch writes, or one data table at $2298; register writes through an address register are NOT enumerated

HEURISTIC: none — no heuristic was carried in; every verdict is read from this tree's code at fb6ca8a or measured by the gate on this branch

CANNOT-TEST: none — every claim was measured or read; the one live look (the acceptance ROM's runtime register writes) is optional and TAGGED, not claimed impossible
```

**Detector check:** `python3 tools/detector-check.py` returned exit 0 with
`detector-check: clean — 228 doc(s), 24 declaration(s), 9 note(s)`.

**TAGGED for the overseer (live look, not done here):** none are needed for the verdicts. Optionally, boot
*Sonic Delta Origins*, arm `write` on `$A130F2–$A130FF`, and confirm that every hit is `size 1` at an odd
address. That would close the "register writes through an address register" gap left in the second detector's
`contains:` field.
