# F-DEBUGREAD-BANKED: the bus-space debug read, under a live cartridge mapper

Landed on `parcel/debugread-banked`. It closes the row booked in `docs/2026-09-11-cart-mapper-design.md`
§8. Sites: `crates/oracle-core/src/bus.rs` (`cart_decode`, `CartByte`, `sram_visible_index`,
`CART_SPACE_END`), `crates/oracle-core/src/system.rs` (`System::cart_peek`, `System::sram_window`,
`System::sram_map`), `crates/oracle-aether/src/engine.rs` (`debug_read`, `BusRegion`, and the
`read` / `read_memory` / `memory_hash` handlers), `crates/oracle-player/src/memory.rs` (the Memory panel).

**Contract status: this needs an amendment.** Two normative sentences in `emulator/memory_hash`'s paragraph
are contradicted (§4 quotes them and drafts replacements). The branch merges only after the hub rules on
that amendment. That is the lane's contract-first rule, and it is expected.

Contract read at empyrean `origin/main` = `68b9a2e129c07090e561895fc5c54ea010c5cb2d` (from `git rev-parse`),
`contract/protocol.md`, via `git show`. The vendored schema is
`crates/oracle-aether/tests/contract/bus-protocol.schema.json` in this repo.

## 1. The defect, re-verified

`engine::debug_read` indexed `sys.rom()` flat. The mapper parcel made cartridge space bank-resolved on the
bus (`MegaDriveBus::mapped_byte` via `CartBanks::rom_offset`), so once a game re-points window *k* the
debugger and the CPU disagree about every byte in `$080000*k..+$7FFFF`. The debugger then showed the
**image's** bank *k* under the label `"cartridge ROM"`, which is a plausible wrong answer. Measured red-first:
with window 1 pointed at bank 3, a debug read at `$080000` returned `image[$080000]`, not `image[$180000]`.

Two adjacent divergences. They are established differently, and I say which is which:

- **(a) No SRAM arm, established by reading the code, not by a red run.** The bus consults the SRAM overlay
  first (`mapped_byte`), and the old `debug_read` had no SRAM arm at all. Since S4 **every** loaded ROM has
  an SRAM map (the header-less fallback is `$200001-$20FFFF`, odd lane), so this diverged for *every*
  cartridge while `$A130F1` bit0 was set, not only for carts that declare SRAM. The red-first run of the
  SRAM test did not reach the latch-on assertion. It stopped one assertion earlier, at latch-off, where the
  flat read returned `image[$200001]` (`[95]`) while window 4 showed bank 9. The latch-on half is covered
  since the fix, by `the_sram_overlay_answers_first_and_is_its_own_region` and by mutation M3b (§7).
- **(b) `$400000..rom.len()`, measured.** On an image over 4 MiB (the mapper's acceptance ROM, *Sonic Delta
  Origins*, is ten 512 KiB banks), the old debug read served `$400000` as `([82], "cartridge ROM")`. The bus
  decodes `$400000-$7FFFFF` as open bus. So the debug read gave an answer at an address where the CPU has
  none.

## 2. Who touches cartridge-space bytes: the consumer enumeration

I enumerated every reader of ROM **bytes**, not only callers of the name `debug_read`
(`grep -rn 'rom\[\|\.rom\b\|rom()\|rom\.get\|debug_read' crates/*/src`, then read each site). The dividing
question for each: is it asking about an **address** (so it wants what the CPU sees now), or about the
**image** (so flat is correct)?

| Site | Question it answers | Verdict |
|---|---|---|
| `engine::debug_read` (free fn) | bytes at a bus address | **live view** (fixed here) |
| `emulator/read {space:"bus"}` → `debug_read` | bus address | live view, follows |
| `emulator/read_memory` → `debug_read` | bus address (exact alias of `read`) | live view, follows |
| `emulator/memory_hash` → `debug_read` | bus address | **live view**: the fork, §3 |
| `Engine::read_u8` / `read_u16` (objreq mailbox, camera, sprite table, `Level_Width`/`Height`) | live game state at a symbol's address | live view, follows. All resolve RAM symbols today, so unaffected in practice |
| `Engine::slot_record`, `objreq_position` (`slot_bytes` stride) | live object records | live view, follows (RAM) |
| `oracle-player` `memory.rs` (`Space::Bus`) | bus address, shown to a person | live view, follows. It prints `region` on its `region` line, and it is the only place a person sees that field (see below) |
| `oracle-player` `objects.rs` (records, ring count) | live game state | live view, follows (RAM) |
| `step_over` (`cpu_regs().prefetch[0]`) | the opcode about to run | **unaffected**: the prefetch was fetched through the bus, so it is already live |
| `save_state::rom_fingerprint(sys.rom())` (frontend `drain.rs`, player `states.rs`) | *which image is loaded* | **image**: bank state is snapshot state, not identity |
| `SymbolTable::validate_against_rom` / `rom_declared_end` (`$1A4`) / DEB2 appendix magic (`symbols.rs`, engine `load_symbols` / `reload_rom`) | does this listing belong to this FILE | **image**: the appendix lives past the declared end in the file |
| `parse_sram_header` (`$1B0-$1BB`, `system.rs`) | the cart's declared SRAM map | **image**, and it sits in window 0, which is fixed |
| reset SSP / entry | the reset vectors | **not an image read at all** (the brief listed it as correct-as-flat). The production path is `reset_with_sink`, and its *"six reads — the SSP/PC vector table at `$0`/`$2`/`$4`/`$6`, then the two prefetches"* are **bus** reads (`system.rs:504`), sink-visible and resolved through `mapped_byte` → `cart_decode`. It is unaffected because window 0 is fixed. The only flat `rom[0..8]` in the tree is in the test `boot_with_sink_captures_the_reset_vector_fetches`, where it derives the expectations from the fixture |
| `romBytes` (`status`, `reload_rom`), player strip `rom bytes`, `romLoaded` | size of what is held | **image** |
| `Z80Bus::read_window` | the Z80's bank window into 68k space | already banked (mapper parcel §4b). **It does not see the SRAM overlay.** I noted that and did not change it (§6) |
| 68k DMA source | bytes the VDP pulls | already through `mapped_byte`, so live |
| `oracle-replay` `runner.rs` (`self.rom[i]`) | offsets in a file it owns | unaffected (not the machine) |
| `oracle-frontend` `bus.rs:787`, `oracle-aether` `host.rs:1772` | test fixtures reading `testrom::build()` | unaffected (test-only, image) |
| `oracle-frontend` generally | none: no ROM bytes are displayed | unaffected. It has no `debug_read` caller, and its only ROM reads are the fingerprint and the symbol appendix |

Sibling consumers, read at their `origin` default branch. I checked aurora (`master` `5361cd6`) myself. For
sigil / seraph / aeon I did not re-run the brief's literal sweep, and I rely on it. **aurora reads `bytes`
from `read_memory` and never
reads `region`** (`git grep` for `"region"` / `.region` in its client code: zero hits outside unrelated
editor code). It also does not branch on `caveat`. So the one place a person sees `region` is in this
repo, in the player's Memory panel.

## 3. The fork, and the recommendation

Before the mapper, *"what the CPU sees at A"* and *"what the file holds at offset A"* were one answer. Now
they are two. `read` is a debugger question. `memory_hash` was specified as an image-oriented one, and
that is where the fork is.

### Options for `memory_hash`

- **(A) Follow `read`.** One derivation. Amend the crc32 and `romBytes` sentences to hold "under identity".
  Cost: a whole-image freshness hash of a banking cart stops matching the file while a window is
  re-pointed, and the answer does not say why.
- **(B) Stay on the image.** Freshness checks stay trivially right. Cost: `memory_hash.addr` becomes an image
  offset while `read.addr` stays a bus address. The contract itself names that shape as the defect to
  avoid: *"two vocabularies wearing one name"* (the `emulator/read` enum paragraph). It also leaves hash and
  read disagreeing at `$200001` whenever SRAM is mapped in.
- **(C), recommended: follow `read`, and make `region` the provenance that says whether the image promise
  holds.** Every surface answers the CPU's view through one derivation. Then `region` is spelled so that
  **`"cartridge ROM"` means exactly "the address IS an image offset"**, and a re-pointed window reads
  `"cartridge ROM bank N"`. With that spelling, the crc32 promise survives in sharpened form: *a hash whose
  region is `"cartridge ROM"` equals CRC32 over the same slice of the file; a banked region names the
  bank, so the file slice is `bank*$80000 + (addr & $7FFFF)`*. A freshness checker can then tell "stale
  image" from "window re-pointed" using only a key the reply already carries. That is what (A) cannot do
  and what (B) buys at the cost of a split vocabulary.

**The decisive argument:** §11.5 already made `region` *provenance, not decoration*, and predicted this
exact case: *"the moment a cartridge mapper or a bank register enters the catalog that derivation is wrong
while looking right."* So the contract has already designated the channel that should carry the bank. (C)
is the only option that uses it. (A) has nothing to say about the bank, and (B) splits the address
vocabulary. In (C), `addr` is a bus address on every row and `region` says whether it is also an image
offset.

### 3.1 The region model

A **region is an address range governed by one decode rule**, and a debug read never straddles two.
Crossing is `-32004`, refused and not stitched, so the single label on a reply is true of every byte in it.
The spellings (`engine::BusRegion::label`):

| `region` | Where | Bytes |
|---|---|---|
| `"work RAM"` | `$E00000-$FFFFFF`, mirror-masked | RAM (unchanged) |
| `"cartridge ROM"` | a window that shows its **own** bank: every unbanked cart, and every cart at reset | `image[addr]` |
| `"cartridge ROM bank N"` | a window re-pointed at bank `N ≠ k` | `image[N*$80000 + (addr & $7FFFF)]` |
| `"cartridge SRAM"` | the SRAM overlay's span `base..=end` while `$A130F1` bit0 is set (checked first, because the bus checks it first) | the chip's lane is save RAM. The other lane is exactly what the bus decodes there, which is ROM through window 4 |

Identity windows coalesce into one `"cartridge ROM"` region. So a whole-cartridge-space read or hash of an
unbanked cart, `$000000` for `$400000`, is one region and answers exactly as before. Two adjacent
re-pointed windows are two regions even when their banks happen to be contiguous in the image: the rule
stays simple and the label stays single-valued, and a client issues two calls.

### 3.2 The adjacent decisions

- **`$400000+` is refused, however long the image is.** The CPU reads open bus there. A >4 MiB image's
  upper banks are reached the way the CPU reaches them, through a re-pointed window. Serving image offsets
  at bus addresses the CPU cannot see is the (B) vocabulary split under another name. The image-wide
  question ("is this the file I built?") deserves its own address-free answer. §5 proposes one and does
  not build it.
- **A window pointed past the image, and a short image's tail, are refused** (`-32004`, worded per case).
  The bus reads open bus there, and a debug read has no byte to report.
- **SRAM is in `debug_read`, via the shared decode** (§3.1). Leaving it out would reproduce divergence
  (a). Refusing reads under a mapped-in overlay would hide what the CPU sees.
- **`len == 0` is refused** with `-32602` (lens L2). Every served caller already bounds it: `parse_count(…,
  1, …)` in `read` / `read_memory` / `memory_hash`; fixed 1 / 2 in `read_u8` / `read_u16`; `slot_bytes`
  filtered `> 0` and pinned to `0x50` in `decoders::derive`; `count_width ∈ {1,2,4}` in `objects.rs`; the
  panel's `view` returns before reading when `len == 0`. But the function is `pub` and cross-crate, and
  `end = addr + len - 1` underflowed at `addr 0`. Measured red-first: a debug-profile panic,
  *"attempt to subtract with overflow"*, at `engine.rs:9689`.

### 3.3 The one derivation, and why it is in core

Divergence (a) shows the real risk. A second decode drifts from the first on exactly the rule nobody
re-reads. So I did not teach `debug_read` the bank table and the SRAM precedence a second time. Instead:

- `bus::cart_decode(a, sram_map, sram_enabled, &banks) -> CartByte` is **the** decision (SRAM first, then
  ROM through the table). `MegaDriveBus::mapped_byte` resolves through it, and so does
  `System::cart_peek(a) -> Option<(CartByte, u8)>`, which reads straight from the image / SRAM buffer with
  no latch, no event, no FIFO. `debug_read` serves cartridge space through `cart_peek` and nothing else.
- The CPU side changed by construction, not by measurement: the arm's two decisions moved into a function
  and neither changed. `oracle-core` lib: 910 passed, 0 failed (the 907 that existed before, plus the three
  `cart_peek_*` added here). `crates/oracle-core/tests/` has zero diff against `d62b9ff`.
- Parity is structural, which also makes a parity assertion **blind** to a defect in the shared function.
  Every test here therefore also asserts against the formula written out by hand. §7's M5 shows why.

### 3.4 Caveats

`read_memory` used to emit a **constant** caveat: *"…bypassing the bus … A CPU read at this address can
differ."* After this change that is false for every address the row serves, and §2.4's advisory names this
exact caveat as the shape a server gets wrong. `read` (bus) and `read_memory` now carry **the same
conditional caveat**, present exactly when `region` is banked or SRAM, because those are the answers that
are right but easy to misread as image bytes. `memory_hash` carries none, because its fragment declares
`caveat` absent. There, `region` alone discloses a banked answer. This is a **partial discharge of lens M18**
(read_memory's unconditional caveat). `read_vram`'s constant caveat (`engine.rs`, *"bypassing the VDP port
path"*) is plausibly another of M18's three. It is not chased here.

## 4. The contract sentences this contradicts, and the drafted amendment

Read at empyrean `68b9a2e129c07090e561895fc5c54ea010c5cb2d`.

### 4.1 Contradicted (normative)

1. `emulator/memory_hash` paragraph: *"The base (or resolved symbol) must land in one of two regions: work
   RAM (`$E00000–$FFFFFF`, mirror-masked) or cartridge ROM (`$000000` up to `romBytes`), and the range must
   not run past that region's last byte."* This is contradicted three ways. There are now more than two
   regions. `$400000..romBytes` is refused on a >4 MiB image. And `romBytes..$3FFFFF` can be readable
   when a window points at a bank inside the image.
2. Same paragraph: *"`crc32` is IEEE/zlib CRC-32 … chosen so a cartridge-window hash equals CRC32 over the
   same slice of the ROM file."* This is false for a re-pointed window. It still holds for every
   `"cartridge ROM"` answer.
3. The schema mirrors of both: `emulator/memory_hash.$comment` (*"Two regions: … cartridge ROM ($000000 up
   to romBytes)"*), `…result.properties.region.description` (*"Which of the two regions answered —
   read_memory.region's spellings ('work RAM', 'cartridge ROM')"*), and
   `…result.properties.crc32.description` (*"so a cartridge-window hash equals CRC32 over the same slice of
   the ROM file"*).

### 4.2 Checked and NOT contradicted

- `emulator/read`: *"A base outside its space, or a range whose **end** runs past it, is `-32004` —
  **refused, never clipped** … Space sizes: bus 24-bit"*. This is compatible: the bus space is 24-bit, and
  the server already refused its unmapped holes (`$A10000`) with `-32004`. *"Its `caveat` is emitted
  **conditionally**"*: yes, now for the bus space as well.
- `read_memory.region` (§11.5): *"`"work RAM"`, `"cartridge ROM"`, and whatever a server with a wider map
  adds"*. The new spellings are exactly this. It is not contradicted, but §4.3 asks to register them.
- `read_memory` as an **exact alias** of `read{space:"bus"}`: after this change they emit the same caveat
  under the same condition. Before, one was constant and the other absent, so this change makes the alias
  claim more true.
- §2.4 rule 1 (a caveat only where the fragment declares it): `read` and `read_memory` both declare
  `caveat`; `memory_hash` emits none.
- Schema: `region` is `type: string` with no enum, on all three fragments, so the new spellings validate.
  Every wire test in `tests/debug_read_banked.rs` passes through the harness's schema check.
- No new result keys (§8's surplus-key ban).

### 4.3 Drafted replacement wording (for the hub; NOT applied to `../empyrean`)

**memory_hash, the region sentence.** Replace *"must land in one of two regions: work RAM (…) or cartridge
ROM (`$000000` up to `romBytes`), and the range must not run past that region's last byte"* with:

> must land in a readable region of the 68000 map as the machine decodes it **now**: work RAM
> (`$E00000–$FFFFFF`, mirror-masked) or cartridge space (`$000000–$3FFFFF`), where each byte is what the CPU
> would read there, through any cartridge bank mapper and SRAM overlay. A byte with no answer (open bus: past
> the image end under the current mapping, or at `$400000` and above however long the image is) is outside
> every region. The range must lie within **one** region, meaning one decode rule named by `region`; a
> range whose end crosses out of its region, including into a differently-mapped window, is `-32004`.

**memory_hash, the crc32 sentence.** Replace *"chosen so a cartridge-window hash equals CRC32 over the
same slice of the ROM file"* with:

> chosen so a hash whose `region` is `"cartridge ROM"` equals CRC32 over the same slice of the ROM file (the
> address is then an image offset). Under a re-pointed window `region` names the bank, `"cartridge ROM bank
> N"`, and the equal slice is at image offset `N × $80000 + (addr & $7FFFF)`.

**read_memory.region, append:**

> Registered spellings: `"work RAM"`; `"cartridge ROM"`, meaning the bytes are the image's bytes at the same
> offset; `"cartridge ROM bank N"` (`N` decimal), meaning a bank-mapper window currently shows image bank
> `N` there; `"cartridge SRAM"`, meaning the cartridge SRAM overlay is mapped in over this range, with its
> byte lane reading the save RAM and the other lane reading what the bus decodes there. A client comparing a
> cartridge read against the ROM file MUST check for `"cartridge ROM"` and MUST NOT assume it from the
> address.

**Schema mirrors:** the three strings in §4.1 item 3 are replaced by the same sentences. No structural
change: `region` stays a free string.

### 4.4 Conformance vectors (for the hub to adopt or amend)

Fixture: ten 512 KiB banks, byte at image offset `i` = `((i>>19) ^ 0x5A) ^ mix(i & $7FFFF)`; window
*k* re-pointed by an odd-byte write of `N` to `$A130F1 + 2k`.

1. Unbanked, `memory_hash {addr:0, len:$400000}` → `region "cartridge ROM"`, `crc32` = CRC32(file[0..$400000]).
2. Window 1 → bank 9: `read {addr:$080040, len:32}` → `region "cartridge ROM bank 9"`, `bytes` =
   file[$480040..+32]; `read_memory` identical; `memory_hash` over the same range: same `region`,
   `crc32` = CRC32(file[$480040..+32]).
3. Window 1 → bank 9: `memory_hash {addr:0, len:$400000}` → `-32004` (straddles into a re-pointed window).
4. Ten-bank image, `read {addr:$400000}` → `-32004` (not cartridge space, though the file has bytes there).
5. Window 3 → bank 20 of 10: `read {addr:$180000}` → `-32004` (open bus).
6. SRAM mapped in (fallback page `$200001-$20FFFF`, odd), window 4 → bank 9: `read {addr:$200001}` →
   `region "cartridge SRAM"`, the save byte; `read {addr:$200000}` → `region "cartridge ROM bank 9"`;
   `read {addr:$200000, len:2}` → `-32004`.
7. Unbanked: a bus `read` / `read_memory` reply carries **no** `caveat`.

## 5. Not built, and proposed

- **An address-free image fingerprint.** The whole-image question ("is the held ROM the file I built?") is
  now answerable by `memory_hash` only for images of 4 MiB or less under identity. The better answer has no
  address at all: e.g. `emulator/status.romCrc32`, or `reload_rom`'s reply carrying it. That is a new key,
  so a contract change, and it belongs with **F-BANKED-ADDR-AMBIGUITY**. No suite ROM is over 4 MiB or
  banks, so nothing depends on it today.
- **A typed bank key.** Parsing `N` out of `"cartridge ROM bank N"` is legal but string-shaped. §2.4 rule 3
  prefers a typed key for anything a client acts on. A `bank` key (or the `cartBanks` array the mapper note
  §5.4 floated for `status`) is the same seam as F-BANKED-ADDR-AMBIGUITY, and it should ship there, once.
- **A readable SRAM space** (`read {space:"sram"}`): the save RAM as its own array, readable whatever the
  latch. It is a new enum value and a contract change.

## 6. Observations made in passing (not fixed)

- `Z80Bus::read_window` resolves cartridge space through the bank table but **not** through the SRAM
  overlay, while the 68k side checks SRAM first. A Z80 reading `$200001` through its bank window with SRAM
  mapped in would read ROM where the 68k reads save RAM. The shared `cart_decode` is the obvious fix. I left
  it alone because it is a bus-behaviour change with its own currency question.
- The engine's `read_u16` carried an orphaned doc paragraph (a stale copy of the old `debug_read` rationale,
  claiming `read` carries a caveat, which it never did). I removed it.
- **`system::tests::no_ra_rom_maps_fallback_sram_after_a130f1_enable` cannot tell SRAM from open bus on
  its SRAM lane.** Found by M3b: with the shared decode's SRAM arm removed, the test stayed green. Its
  ROM is `0x4000` bytes, so `$200001` then resolves past the image into open bus. The `write8($200001,
  0xFF)` just before the read drives the open-bus latch. By the test's own comment on the latch (*"a
  work-RAM write drives open bus = 0x1111"*) the read then echoes `0xFF`, which is exactly the value it
  asserts. The latch value itself is inferred from that comment and from the green result, not measured.
  The test's even-lane half already guards this hazard ("drive the open-bus latch to a distinct value
  first"), but its SRAM-lane half does not. The cure is the same move: drive the latch to a distinct value
  before the read. It is a pre-existing test and not this parcel's to change. `cart_peek_answers_sram_first…`
  and the aether SRAM test do cover the SRAM lane with a value distinct from what the latch holds.

## 7. Tests and the red-first record

New: `crates/oracle-aether/tests/debug_read_banked.rs` (9 tests, a **new test target**, so +1 leg in
`land.sh`'s derived count). `oracle-core` `system::tests`: `cart_peek_is_the_cpu_view_under_a_remapped_table`,
`cart_peek_answers_sram_first_exactly_where_the_bus_does`, `cart_peek_is_none_where_the_cpu_reads_open_bus`.
`oracle-player` `memory::bus_parity::the_panel_shows_the_bank_a_remapped_window_shows`. Changed:
`tests/methods.rs` `approximate_answers_carry_a_caveat`. Its `read_memory` leg pinned the constant caveat,
and it now pins its absence on an unbanked read.

Every expectation is derived from the image plus `bank * $80000 + (addr & $7FFFF)` written out. None is
read back through `rom_offset`, and none is compared with `CartBanks::IDENTITY`. Each fixture first asserts
that its banks differ. All seven windows are re-pointed at once, *k → k+2*, so a window off-by-one reads
*k+1* or *k+3* and cannot pass.

**Red-first (pre-fix):** the new aether file against the unchanged `debug_read` gave 8 red and 1 green (the
fixture-premise check). Each red was the defect: flat bytes; the constant caveat on an unbanked read;
`$400000` and bank-20 served; the len-0 overflow panic.

**Mutations**, each applied to the committed baseline `0f0c02d`, quoted back from disk, run, and restored
with `git checkout --` onto a tree `git status` showed clean:

| # | Mutation (line as it read on disk) | Predicted red | Observed red |
|---|---|---|---|
| M1 | `match sys.rom().get(a as usize).map(\|b\| ((), *b)) {` (flat bytes) | 4 aether + panel | exactly those 5: `every_remapped_window…`, `three_wire…`, `the_sram_overlay…`, `a_window_pointed_past_the_image…`, `the_panel_shows_the_bank…`. The panel test's two parity assertions **passed**; only its derived assertion (`memory.rs:1089`) failed |
| M2 | `let window = ((a as usize / CART_BANK_SIZE + 1) & 7) as u8;` (label off-by-one) | 3 aether + panel; the identity test blind | exactly those 4. The unbanked test stayed green: every window is identity, so a shifted label still reads `"cartridge ROM"`. The panel's label-parity assertion **passed** (both sides wrong alike) and the derived-label assertion (`memory.rs:1095`) failed |
| M3a | `if let Some(m) = sram.filter(\|_\| false) {` (the label's SRAM-span arm removed; bytes still from the shared decode) | only `the_sram_overlay…` | exactly that 1, on its label: `"cartridge ROM bank 9"` where `"cartridge SRAM"` was due |
| M3b | `match None::<usize> {` in `bus::cart_decode` (the SHARED decode loses its SRAM arm, so bus and peek lose it together) | at least `sram_overlay_still_wins…`, `cart_peek_answers_sram_first…`, aether `the_sram_overlay…`, plus any core test reading SRAM back through the bus read arm | core 3 (those two plus `sram_saves_within_a_session_with_odd_byte_addressing`), aether 1, at its derived assertion (`:318`, *"the SRAM byte, not the ROM under it"*). The write arm uses `sram_index` directly, so writes still landed. Of the other SRAM tests, `reset_preserves_the_battery_sram_and_its_map` never reads through the bus, so green is right. **`no_ra_rom_maps_fallback_sram_after_a130f1_enable` does read `$200001` through the bus and stayed green anyway.** That is a pre-existing blind spot, recorded in §6 |
| M4 | `if false && here != region {` (the region-crossing refusal removed) | `a_read_may_not_straddle…`, `the_sram_overlay…` | exactly those 2. The straddling read came back `[92, 93, 83, 82]` under the one label `CartRom`: two bytes from window 0 and two from bank 9, so the label was false for half of them |
| M5 | `None => CartByte::Rom(a as usize),` in `bus::cart_decode` (the SHARED decode ignores the bank table) | aether `every_remapped…`, `three_wire…`, `the_sram_overlay…`, `a_window_pointed_past…`; the panel test at its derived assertion; core: the three `cart_peek_*` plus every test reading a re-pointed window through the 68k bus; the Z80-window test green | as predicted. Aether 4, player 1 (**at `memory.rs:1089`, the derived assertion, after both parity assertions passed**: the tool and the panel agreed with each other while both were wrong), core 11 (the three `cart_peek_*` and 8 mapper tests, DMA included). The Z80 window test stayed green because it calls `rom_offset` directly |
| M6 | `if false && len == 0 {` (the L2 guard removed) | only `a_zero_length_read…` | exactly that 1: *"attempt to subtract with overflow"* at `engine.rs:9808` (the `end` line) |

M1/M2/M5 are the reason for the brief's third assertion. A parity pair between surfaces that share one
function cannot see a defect in that function. In all three runs, the parity assertions ran and passed
before the derived one failed. Runners: `cargo test -p oracle-aether --test debug_read_banked`,
`cargo test -p oracle-player memory`, `cargo test -p oracle-core --lib` (debug profile). Each run was
checked for `Compiling oracle-…` before its result was read.
