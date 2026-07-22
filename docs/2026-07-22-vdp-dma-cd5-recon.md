# VDP DMA recon + design (DR-2) — the spurious CD5 DMA trigger

**Status: RECON + DESIGN, 2026-07-22. Docs only — no code.** The TF4 triage
(`docs/2026-07-22-tf4-triage.md`) localized DR-2 to oracle-next firing a 65536-word 68k→VRAM DMA that halts the
CPU for ~28 frames, from a control write while DMA-enable (reg 1 bit 4 / M1) is off and the length registers
are 0. This doc pins the hardware truth clean-room (the overseer's explicit ask: **do not just flip recon R1**),
surfaces the fix decision, front-loads the gate-safety analysis (the real risk here — this is the
currency-sensitive VDP DMA path), and confirms **Batman (DR-3) is a separate bug**.

**Permitted sources only** (audit policy 3): **SpritesMind** VDP-internals threads (Nemesis — the authoritative
VDP reverse-engineer — and Eke), the MegaDrive Development Wiki, Plutiedev, the Genesis Software/Technical
manuals. **No emulator source opened.** Primary: the confirmed in-tree mechanism + the TF4/Batman ROMs on the
user's disk (D5).

Items **V1–V6**.

---

## Part 1 — The confirmed bug

`vdp.rs` `control_write` completes a command's second word (lines 614–634):

```rust
let cd_hi = ((w >> 4) & 0x0F) as u8;             // CD5..CD2 from the command word
if self.regs[1] & 0x10 != 0 {                    // M1 (DMA enable) set
    self.code = (self.code & 0x03) | (cd_hi << 2);      // full CD5..CD2 update
} else {
    self.code = (self.code & 0x23) | ((cd_hi << 2) & 0x1C);  // CD5 RETAINED (R1), CD4..CD2 updated
}
// ...
if self.code & 0x20 != 0 { self.arm_dma(); }     // <-- trigger on CD5 ALONE, ignores M1
```

The CD5 **retain** (line 622) is correct (recon R1). The bug is that **`self.code` bit 5 (CD5) is never
cleared once a DMA has set it** — the only writers of `self.code` are control-word decodes (612/619/622). So a
real DMA (M1=1) leaves CD5 = 1; the game then clears M1; a subsequent plain VRAM-write command with M1 = 0
**retains** CD5 = 1; and the trigger at 632 fires a spurious Mem DMA with the unset length registers
(`0 → 65536` words, `bus.rs:425`), charged as one ~28-frame CPU halt (`bus.rs:449`). **This is TF4's hang.**

CD5 is consulted at **three** sites, all of which see the stale bit: `632` (Mem/Copy trigger on the control
write), `771` (Fill trigger on the data write), `865` (`data_write_at` routes a DMA-command data write past the
FIFO). A complete fix must neutralize the stale bit for all three, not just 632.

## Part 2 — Clean-room hardware pin (do not flip R1)

### V1 — M1's *only* effect is to gate CD5 *writes* — it is **not** a trigger gate

**PINNED.** Nemesis (SpritesMind VDP-internals): *"CD5 can only ever be modified externally by a control port
write if the DMA enable bit is set (reg 1, bit 4). **The absolute only effect the DMA enable bit ever has is to
enable or disable control port writes being able to modify the current state of CD5.**"* And: *"This bit also
masks CD5 in the VDP control word if cleared."* So **recon R1 stands unchanged** — CD5 is retained (not
writable) while M1 = 0. **The DMA trigger is not M1-gated**; it fires on CD5 alone. This **rules out** the
"gate `arm_dma` on M1" fix as the *mechanism* (it would model a gate hardware does not have).

**Confidence**: high (explicit statement from the authoritative source, corroborated by the MegaDrive Dev Wiki
DMA page). **Classification**: behavioral.

### V2 — CD5 is the "DMA work **pending**" bit — hardware clears it when the DMA completes

**PINNED — the missing behavior.** The command-code bits are (Nemesis): *"CD4 – Work complete, **CD5 – DMA work
pending**."* CD5 is not a static command bit; it is the DMA-engine's pending flag, **cleared by the engine when
the transfer finishes**. That is exactly the behavior oracle-next lacks: it sets CD5 (via the control write)
but never clears it on completion, so the bit goes stale. On hardware, after a DMA, CD5 = 0; a later M1 = 0
command retains CD5 = **0**, so no spurious trigger — which is why TF4 (and every game) works on silicon.

Why oracle-next's *other* ROMs (s4.bin) never hit this: they keep M1 = 1 after a DMA, so their next command's
control-word decode (line 619) overwrites CD5 with the new command's (0) bit — an *incidental* clear. Only the
DMA-then-`M1:=0`-then-command sequence (TF4) exposes the missing engine-side clear.

**Confidence**: high (authoritative, and it is the only model consistent with real games not spuriously
re-triggering). **Classification**: behavioral. **R1/R4 status**: R1 (CD5-write-masking) unchanged; this is a
**gap in the R4 DMA model** (CD5-clear-on-completion was never pinned), not an R1 correction.

### V3 — Length register = 0 means 65536 (this part is correct)

**PINNED (already correct in-tree).** `run_mem_dma`/`run_fill` treat `len == 0` as `0x10000` — the documented
hardware convention (MegaDrive Dev Wiki DMA; Genesis manual). The 65536-word transfer is not the bug; the bug
is that the transfer *fires at all*. No change here. **Confidence**: high.

---

## Part 3 — The fix decision (surfaced, not defaulted)

All three options below fix TF4. They differ in faithfulness and completeness.

| | Mechanism | Faithful to V1/V2? | Fixes all 3 CD5 sites? | Notes |
|---|---|---|---|---|
| **(i)** force-clear CD5 when M1 = 0 | ad-hoc | No (invents an M1→CD5 clear) | Yes (CD5 becomes 0) | Hack; overwrites the retained bit R1 says to keep |
| **(ii)** gate `arm_dma` on M1 | add `&& M1` at 632 | **No** (V1: M1 is not a trigger gate) | **No** — 771/865 still see stale CD5 | Would need parallel M1 gates at 771/865; leaves CD5 permanently stale |
| **(iii)** clear CD5 when the DMA is consumed/completes | matches V2 | **Yes** | **Yes** (roots out the stale bit) | 1 site, hardware-faithful |

**Recommendation: (iii).** Clear CD5 (code bit 5) at the single DMA choke point — `take_dma_request`
(`vdp.rs:661`), which the bus calls for every Mem/Copy/Fill once the request is armed and about to run
(synchronous, so consumed ≈ completed):

```rust
pub fn take_dma_request(&mut self) -> Option<DmaRequest> {
    let req = self.dma_pending.take();
    if req.is_some() {
        self.code &= !0x20; // CD5 = "DMA work pending" — cleared when the engine consumes the transfer (V2)
    }
    req
}
```

This models V2 directly, fixes all three CD5 sites at the root, leaves R1's CD5-retain untouched, and does not
invent the (V1-contradicted) M1 trigger gate. It is guarded on `is_some()` so a non-DMA VDP access (the common
case, `dma_pending == None`) never touches CD5.

## Part 4 — Gate-safety analysis (front-loaded — the real risk)

The change touches the currency-sensitive DMA path, so prove neutrality *before* coding. The one behavior (iii)
changes for an M1 = 1 sequence is: **a data-port write issued immediately after a Mem/Copy/Fill DMA with no
intervening control command.** Today the stale CD5 = 1 mis-routes that write (line 771/865: treated as a
DMA-command data write — no FIFO, no store, no autoincrement). (iii) clears CD5, so the write becomes a normal
data write — *more* correct, but a change. The gate question is whether any frozen fixture does this.

| Currency | DMAs at all? | Does DMA-then-data-write? | Neutral under (iii)? |
|---|---|---|---|
| export golden (`export_state_v1`, testrom) | **No** (Z80/VDP DMA never armed; test asserts Z80 RAM all-zero) | — | Yes (no DMA) |
| golden_frames (7, bus-less `Vdp` scenes) | **No** (poke VRAM directly; never arm a DMA) | — | Yes |
| SST (1,000,058, `FlatBus`) | **No** (`FlatBus` has no VDP) | — | Yes |
| determinism_gate (testrom) | **No** | — | Yes |
| oracle_differential (captured bytes) | **No** (no `System`/bus) | — | Yes |

**No frozen currency arms a DMA at all** → (iii) is currency-neutral by construction. Additional checks the
slice must run (behavioral, not golden-hashed, but part of the bar):

- **push-6 DMA unit tests** (`vdp.rs` `copy_runs_at_half_the_fill_byte_rate`, `cram_fill_uses_the_four_writes_ago_entry`,
  `fifo_and_dma_fields_survive_a_bincode_round_trip`) call `run_fill`/`run_copy` **directly** (bypassing
  `take_dma_request`) and assert on VRAM/CRAM/`dma_busy_until` — never on `code`. Unaffected by (iii).
- **`dma_busy` status (bit 1)** is a separate timed window (`dma_busy_until`, `vdp.rs:166/378`), **not**
  CD5-derived — so clearing CD5 cannot move a status-poll result. This is the load-bearing reason (iii) is safe.
- **s4.bin** (frame_dump / the golden-frame ROM) keeps M1 = 1 across its DMAs, so its next command already
  overwrites CD5 (line 619) — (iii) only moves the clear a few cycles earlier; render output is unchanged.

**The slice's hard gate**: all five frozen currencies byte-identical + the push-6 DMA tests green + a new unit
test proving the TF4 sequence (real DMA → M1:=0 → plain command) no longer arms a DMA.

## Part 5 — Batman (DR-3) is a separate bug — do **not** assume this fixes it

**PINNED (measured).** Batman's PC **advances** (`$7542 → $9C10 → $12D38` across frames) and it **renders**
non-black content (34,144 then 119,556 non-zero pixel bytes) — it is **running and drawing (garbled)**, not
hung. This is categorically unlike TF4's frozen 531-instruction, ~28-frame-stall hang. Batman does **not** hit
the spurious-DMA halt; its bug is **render/VRAM-content correctness** (wrong tiles/mappings), a different
mechanism. **The CD5 fix will very likely not touch Batman.** DR-3 needs its own diagnosis — a matched-frame
VRAM/CRAM/mapping A/B against a reference (Oracle/BlastEm) — as its own recon. The slice must **re-test Batman**
and report it unchanged (expected), not silently fold it in.

## Part 6 — Gated implementation plan

One `vdp.rs` change (the `take_dma_request` CD5 clear, Part 3). Gates, ordered:

1. **Semantics unit test** (new): the TF4 sequence — arm+run a real Mem DMA (M1 = 1), set M1 = 0, complete a
   plain VRAM-write command — must leave `dma_pending == None` (no spurious DMA armed). Plus: a normal M1 = 1
   Mem/Fill/Copy still arms + runs. RED before the fix, GREEN after.
2. **Frozen currencies byte-identical (hard):** export golden, 7 golden_frames, determinism, oracle_differential,
   SST. Plus the push-6 DMA unit tests + `export_state_captures_live_z80_ram`. Any drift = stop-and-investigate.
3. **Acceptance — TF4:** `boot_rom` boots **past** `$0FF350` (no ~28-frame stall; step rate normal), and its
   render/PC progresses into gameplay init instead of freezing. (Whether it fully renders may expose a *next*
   TF4 layer — report it, do not over-promise, same as Gunstar/DR-1b.)
4. **Diagnostic — Batman:** re-run; expected **unchanged** (Part 5). If unchanged, DR-3 stays its own rock (a
   VRAM-content A/B recon). If it changes, re-triage.

### Out of scope (named)

- **DR-3 Batman render-content bug** — separate recon (matched-frame VRAM diff).
- **The broader mis-mirrored Z80 map** (`$A06000`/`$A07Fxx`/`$A08000`) — the Z80-map slice, unrelated.
- **Cycle-exact DMA timing** — the interim `dma_cost` window stands; not this slice.

## Part 7 — Implementation outcome (2026-07-22, slice shipped for review)

Fix (iii) implemented per Part 3: `take_dma_request` clears `code & !0x20` on an actual consume (`is_some`
guard). `vdp.rs` only.

**Gates:**
- **#1 semantics** — two new unit tests: `a_completed_dma_clears_cd5_so_a_later_m1_off_command_does_not_respawn_it`
  (the TF4 sequence — real Mem DMA → M1:=0 → plain command must leave `dma_pending == None`; RED before, GREEN
  after) and `fill_cd5_survives_the_control_write_until_the_data_trigger` (the Fill two-step — CD5 must survive
  the control write; the `is_some` guard means the None-returning take does not clear it prematurely). Lib
  **598/598**; fmt-clean, clippy-clean.
- **#2 frozen currencies — byte-identical:** export golden 3/3, 7 golden_frames, determinism 2/2,
  oracle_differential 3/3, **SST 112/112**, plus all push-6 DMA unit tests (bus `mem_dma`/`fill`/`copy`/SAT/
  frame_report) and `export_state_captures_live_z80_ram`. Currency-neutral held (Part 4).
- **#3 TF4 acceptance — the ~28-frame DMA halt is RESOLVED.** TF4 no longer stalls: it executes at normal step
  rate and **renders ~128k non-zero pixels** (varying frame-to-frame = actively running), where before it froze
  blank at 531 instructions. PC is past `$0FF350`. **Honest next-layer note:** PC still cycles the `$0FF35x`
  VRAM-upload region across 120–1200 frames, so TF4 has not reached full gameplay — the halt is fixed, a
  further init-loop layer remains (exactly the "may expose a next layer" the overseer flagged). Not
  over-promised: DR-2's *blocker* is fixed; TF4 is not fully booting yet.
- **#4 Batman diagnostic — CHANGED, not unchanged.** Batman still runs-and-draws-garbled (DR-3, unfixed), but
  its render shifted (e.g. f1200 PC `$12D38`/0 px → `$9C14`/4682 px). The CD5 fix touched Batman's DMA path too
  (Batman uses DMAs; it was silently relying on / exercising the old stale-CD5 behavior in some sequence). It is
  **still broken, differently** — not a regression of a working state, but a real change. **DR-3 must be
  re-triaged against this new post-fix baseline**, not compared to the pre-fix one.
- **Regression check (working games):** Sonic 2 (207,621 px), S&K (134,589 px), aeon demo (215,040 px) all
  render, display on — **no regression** on the games that already worked.

**Net:** DR-2's spurious-DMA halt is fixed and gate-safe; TF4 renders but has a further init layer; Batman is
unchanged-in-status (still DR-3) though its execution shifted and needs a fresh triage.

## Sources

- [SpritesMind — VDP Internals (Nemesis: CD5 = DMA-pending; M1 only masks CD5 writes)](https://gendev.spritesmind.net/forum/viewtopic.php?t=1291)
- [MegaDrive Development Wiki — VDP DMA](https://wiki.megadrive.org/?title=VDP_DMA)
- [SpritesMind — VDP registers timings (Eke: DMA-busy on the control-port setup write)](http://gendev.spritesmind.net/forum/viewtopic.php?t=291)
- In-tree: `crates/oracle-core/src/vdp.rs` (`control_write` 614–634, CD5 sites 632/771/865, `take_dma_request`
  661, `dma_busy_until` 166/378), `crates/oracle-core/src/bus.rs` (`run_mem_dma` 423). Recon R1/R4:
  `docs/2026-07-16-vdp-recon.md`. Triage: `docs/2026-07-22-tf4-triage.md`.
