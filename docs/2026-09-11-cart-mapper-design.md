# The Sega ("SSF2-style") cartridge bank mapper — design, evidence, and what it deliberately does not do

Landed on `parcel/cart-bank-mapper`. Sites: `crates/oracle-core/src/bus.rs` (`CartBanks`, the
`$A130F3-$A130FF` write arm, the `mapped_byte` indirection), `crates/oracle-core/src/system.rs`
(`System::cart_banks` + the getter), `crates/oracle-core/src/testrom.rs` (one construction site).

**Read this before re-deriving any of it.** The map below was pinned from primary evidence; the point of
this page is that the next session transcribes it instead of guessing it from how other emulators do it.

## 1. The defect it closes

Cartridge space `$000000-$3FFFFF` was resolved as a flat `rom[a]`, and `$A130F1` (SRAM enable /
write-protect) was the **only** `$A130xx` register decoded — `$A130F3-$A130FF` fell through `write_byte`'s
catch-all `_ => {}` and were silently dropped. Consequences: a ROM larger than 4 MiB could not reach its
upper banks at all, and any ROM that re-points a window read the wrong data with no diagnostic.

## 2. The register / window map (pinned)

Eight 512 KiB windows tile cartridge space. The register at `$A130F1 + 2k` controls window `k`:

| Register | Window | Address span |
|---|---|---|
| `$A130F1` | — | **NOT a bank register** — SRAM enable (bit0) / write-protect (bit1). See §3 |
| `$A130F3` | 1 | `$080000-$0FFFFF` |
| `$A130F5` | 2 | `$100000-$17FFFF` |
| `$A130F7` | 3 | `$180000-$1FFFFF` |
| `$A130F9` | 4 | `$200000-$27FFFF` |
| `$A130FB` | 5 | `$280000-$2FFFFF` |
| `$A130FD` | 6 | `$300000-$37FFFF` |
| `$A130FF` | 7 | `$380000-$3FFFFF` |

Window 0 (`$000000-$07FFFF`) is **fixed** and has no register — it carries the vector table and the header.

- The value written is a 512 KiB **bank number**; a byte resolves to `bank * $80000 + (addr & $7FFFF)`
  (`CartBanks::rom_offset`).
- **Power-on and reset = the identity mapping** (window `k` → bank `k`, `CartBanks::IDENTITY`). That
  mapping resolves to exactly the pre-mapper flat decode, which is why every frozen currency and every
  golden is byte-identical **by construction** rather than by measurement.
- The registers are the **odd** bytes of their words, **write-only**, with no read arm (a read stays open
  bus). A word write to the even neighbour lands its low byte in the register — the mechanism by which a
  `move.w` reaches it, identical to how `$A130F1` is fed from `$A130F0`.

## 3. `$A130F1` is the SRAM latch and must not become window 0's bank register

The acceptance ROM (`Sonic Delta Origins`, 5,242,880 bytes = exactly ten 512 KiB banks) **documents its own
contract in plain text**, and it is the SRAM latch, not a bank register. Verified byte-exact in this repo's
lane on 2026-09-11 (`LC_ALL=C`, Python, not `grep -P`): the Portuguese technical-information block runs from
about `$17D4`, with the word `registro` at `$17DC`, `mapeador` at `$17EB`, and the two literal instruction
lines at `$186F` and `$1894`:

> "O registro #0 do mapeador deve ser usado para alternar entre a ROM/RAM após o endereço $200000"
> — followed by `move.b #1, ($A130F1)  ; ativa SRAM` and `move.b #0, ($A130F1)  ; desativa SRAM`.

Register #0 of the mapper is the ROM/RAM switch at `$200000+`, which is precisely the enable /
write-protect latch already implemented. **The existing SRAM code therefore needed zero behavioural
change**, and `a130f1_stays_the_sram_latch_and_is_not_a_bank_register` is the guard that keeps a later
session from folding it into the bank table.

Two more things that same block gives us, both useful and neither previously recorded here:

- The surrounding screen is an **error** screen: `Exceção $41: SRAM não disponível`, telling the player to
  ask their cartridge vendor for *"suporte ao Mapeador SEGA com SRAM via registro #0"*. So the ROM
  **self-tests** this contract and renders a visible verdict — see the runtime follow-ups in §8.
- `A130F3` does **not** appear as ASCII anywhere in the image (0 hits over all 5 MiB). The higher registers
  are exercised by assembled code, not documented in the text; only register #0 is described. The window
  table in §2 is therefore pinned from the mapper convention plus this ROM's own use of register #0, and
  §8 names the runtime check that closes the loop on the rest.

**If a later session finds evidence contradicting §3, that is a BLOCKED item** — record it and stop on it,
do not quietly redesign around it.

## 4. SRAM precedence, and why window 4 makes it load-bearing

`mapped_byte` checks the SRAM overlay **first**, then resolves ROM through the bank table — the same order
the flat decode had. This is not incidental: **window 4 is `$200000-$27FFFF`, exactly the span SRAM maps
into**, so an enabled overlay and a re-pointed window are two live claimants on one address. SRAM wins.

A finding from the red-first work, worth more than the fix: **nothing pinned this precedence before.**
Flipping the order (mutation M4 below) turns exactly **one** test red — the new one. Every pre-existing
SRAM test loads a ROM too short to cover `$200001`, so a ROM-first decode would still have fallen through
to SRAM and answered correctly. It took a 5 MiB image to make both claimants real.

## 5. Decisions recorded rather than left silent

1. **The write path resolves nothing through the table.** Cartridge space has no writable backing store but
   SRAM, and SRAM's mapping is address-keyed (base / end / parity from the header), not bank-keyed — so a
   `rom_offset` call in `store_byte` would compute an index into a read-only image purely to discard it.
   Re-pointing a window cannot make a cart-space write land anywhere new;
   `a_write_into_a_rebanked_window_still_does_not_touch_the_rom_image` asserts that rather than assuming it.
2. **A bank number wider than the ROM image is not masked.** It resolves past the image end and reads
   **open bus** — the same answer a short ROM already gives past `$3FFFFF`. How many bank lines a real cart
   decodes is per-cart wiring, and open bus is the conservative answer that needs no per-cart claim.
3. **The table is snapshot-only, in neither frozen currency.** It rides `System`'s bincode snapshot for
   determinism and is **not** in `export_state` and **not** in `state_hash` — the same class as
   `sram_enabled` / `sram_write_protect` / `z80_bank` (cartridge and bus-control registers). Consequence:
   **no version bump**, because the frozen `export_state` layout is untouched. (Had it gone into the image,
   `docs/export-state-v1.md`'s layout-only rule would have applied.)
4. **Window 0 is protected structurally, not by a runtime check.** `banks[0]` is `0` at construction and
   the only writer derives its window from `CartBanks::window_of_register`, which never yields 0;
   `CartBanks::set` carries a `debug_assert!` for the case that can't arise.

## 6. Tests (hermetic) and the red-first proof

13 tests, all building their ROM images in-process: each 512 KiB bank is filled with a label **derived**
from its bank index (`(bank as u8) ^ 0x5A` — XOR is a bijection, so distinct banks always read back
distinctly), and the standard image is ten banks, so banks 8 and 9 exist **only** past `$3FFFFF` and are
reachable solely through a re-pointed window. Nothing depends on a cart outside this repo.

In `bus::tests`: `identity_mapping_at_reset_is_the_flat_rom_decode`,
`writing_a_bank_register_repoints_exactly_that_window`, `window_zero_is_fixed_under_every_a130xx_write`,
`a_word_write_to_the_even_neighbour_reaches_the_bank_register`,
`bank_registers_are_write_only_a_read_is_open_bus`,
`sram_overlay_still_wins_over_a_banked_rom_read_in_window_4`,
`a_write_into_a_rebanked_window_still_does_not_touch_the_rom_image`,
`a130f1_stays_the_sram_latch_and_is_not_a_bank_register`, `a_mem_dma_sources_through_the_bank_table`.
In `system::tests`: `the_bank_table_powers_on_as_the_identity_mapping`,
`the_cart_bank_table_survives_snapshot_and_restore`, `a_soft_reset_restores_the_identity_bank_mapping`,
`loading_a_cartridge_reseeds_the_identity_bank_mapping`.

Runner: they are `oracle-core` lib tests, so `cargo test --workspace --release` (G7 of `tools/land.sh`)
executes them; `cargo test -p oracle-core --lib` is the fast loop.

Four mutations against the committed baseline, each applied and quoted back **from disk** before its red
run (predictions made before each run; `cargo test -p oracle-core --lib`, 906 tests):

| # | Mutation | Result |
|---|---|---|
| M1 | read-path indirection reverted to `let i = a as usize;` | 8 red — exactly the predicted set, by name |
| M2 | the `$A130F3-$A130FF` write arm made a no-op | 10 red — exactly the predicted set, by name |
| M3 | `IDENTITY` mutated to `[0; 8]` | 8 red, and the membership disagreed with the prediction (see below) |
| M4 | SRAM-vs-ROM precedence flipped | 1 red — see §4 |

**M3 is the one that paid.** `a_soft_reset_restores_the_identity_bank_mapping` stayed **green** under a
mutated `IDENTITY`, because it asserted `s.cart_banks() == CartBanks::IDENTITY` — a comparison against the
very constant that would be wrong, so both halves move together and the assertion cannot fail. The
companion test failed only through a second assertion it happened to have. Both now assert
`bank(k) == k as u8` per window, derived from the index, and re-running M3 after the repair gives **9** red
including the reset test. The transferable shape: *an assertion against the constant under test is
circular, and it looks exactly like a strong assertion until something mutates the constant.*

## 7. What this does NOT cover

- Other mappers: no Codemasters / Pier Solar / EEPROM-serial support (EEPROM was already a named deferral
  in `docs/2026-07-23-sram-design-recon.md`, open question 4).
- `$400000-$7FFFFF` stays open bus and was deliberately **not** widened: a large ROM is reached *through*
  the low windows, which is what the mapper is for.
- No read arm for the bank registers (they are write-only on hardware); a debugger that wants the live
  mapping reads `System::cart_banks()`, which is why that getter exists.

## 8. Follow-ups — the runtime half is TAGGED, not done

Nothing in this parcel was run against a live emulator (no-emulator standing invariant). For the
foreground:

- **TAG-1 — boot the acceptance ROM and look for the error screen.** `Sonic Delta Origins` renders
  `Exceção $41: SRAM não disponível` when it decides the mapper/SRAM contract is unsupported (§3). With
  this parcel the expectation is that it does **not**. That screen is a far better acceptance signal than
  "it boots", and it is the ROM's own verdict rather than ours.
- **TAG-2 — confirm the upper banks are reached at all.** The image is exactly ten 512 KiB banks; before
  this parcel banks 8 and 9 were unreachable by construction.
- **F-DEBUGREAD-BANKED (new, open).** `emulator/read_memory` → `engine.rs::debug_read` indexes
  `sys.rom()[addr]` directly and carries the documented caveat *"taken straight from the region, bypassing
  the bus"*. That caveat was previously harmless in cart space, where flat and bus agreed; with a live
  mapper a debug read at `$080000` reports the **image's** bank 1 whatever window 1 currently shows, and it
  also accepts addresses up to `rom.len()` that are not in cart space on the bus at all. Options: resolve
  the debug read through `CartBanks` (and say so in the caveat), or sharpen the caveat to name the
  divergence. Not folded in here because it is an Aether-surface contract change, not a bus fix.
- **F-BANKED-ADDR-AMBIGUITY (new, open).** A 24-bit address inside a banked window no longer names one ROM
  byte across time — symbols, watchpoint hits, and profiler routine addresses in `$080000+` are ambiguous
  without the window's bank. Nothing in-tree reports the bank alongside such an address yet.
