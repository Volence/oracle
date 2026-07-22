# Differential ROM findings — 2026-07-22 (first multi-game boot sweep)

**What this is.** With the s4-boot milestone complete, the next correctness work is *differential hardening*:
run oracle-next on real, independent games (built from the in-workspace disassemblies, or commercial dumps
already on the user's disk — **never downloaded, never committed**, skip-if-absent per D5) and compare the
render against what the game should show. oracle-next had only ever run the aeon `s4.bin` engine; this is the
first exposure to other engines.

**Method.** `cargo run --release --example boot_rom -- <rom> <frames> <prefix>` → render + `.regs.txt` (PC, SR,
D/A, VDP regs). Six diverse engines, 300-frame first pass, then a 120/600/1000/1800 spread + PC/display probe
on the failures. Renders eyeballed (the games are well-known; the human is the reference). ROMs live at the
paths below (user's disk).

## Result: 6/6 boot clean, 3/6 render correctly, 3/6 real gaps

**Robustness win:** all six ran 300 frames with **no panic, no hang-on-opcode, no unimplemented instruction** —
oracle-next generalizes well past the engine it was tuned on.

| Game | ROM (user disk) | Boot | Render | Notes |
|---|---|---|---|---|
| Sonic 2 (rev01) | `AP Backups/Awesome Project/s2rev01.bin` | clean | ✓ | SEGA logo pixel-clean |
| Sonic & Knuckles | `skdisasm/sonic3k.bin` | clean | ✓ | intro animation |
| aeon demo | `aeon/demo.bin` | clean | ✓ | blue field + placeholder box (aeon engine) |
| **Thunder Force IV** | `The Adventures of Batman and Robin/Thunder Force IV (U).bin` | clean | ✗ | **permanently blank** |
| **Gunstar Heroes** | `The Adventures of Batman and Robin/Gunstar Heroes (USA).md` | clean | ✗ | **permanently black** |
| **Batman & Robin** | `The Adventures of Batman and Robin/Adventures of Batman & Robin, The (USA).md` | clean | ✗ | **garbled tiles** |

(Also present for later batches: Alien Soldier, Vectorman, Ristar (with disasms), Sonic 3, Sonic 3 Complete,
S.C.E. The ~15 Sonic-2 hack variants collapse to one engine — diversity, not count, is what matters.)

## Differential punch-list (living)

| # | Game | PC f600→f1800 | Display (r01 b6) | Symptom | Root cause / hypothesis | Status |
|---|---|---|---|---|---|---|
| **DR-1** | Gunstar | ~~`$3090` frozen~~ → `$450` main loop | ~~OFF~~ → ON (0x64) | ~~hard-stuck, display never enabled~~ → **RENDERS** | **FULLY RESOLVED.** Two bounded bus-level slices, no Z80/FM core: **DR-1a** = `$A11100` BUSREQ latch (`docs/2026-07-22-z80-busreq-recon.md`); **DR-1b** = YM2612 FM-status carve-out at `$A04000–3` (`docs/2026-07-22-fm-status-recon.md`, Option B — reads report not-busy, writes drop, no longer alias `z80_ram[0..3]`). Gunstar boots to main loop, display on, 71 680/71 680 non-black. | **DONE (a + b)** |
| **DR-2** | Thunder Force IV | `$0FF3E8` (init-script loop) | ON (0x44) | display on, permanently blank; cycles a boot init-script/VDP-upload loop, never loads art | **RE-CLASSIFIED** (`docs/2026-07-22-z80-busreq-recon.md`, Part 2): same handshake *family* — TF4 uses `$A11100` (70 sites, take-bus at `$82C`) — but its hang is a `bsr.w`/VDP-upload loop at `$0FF3E8`, **not** a raw `$A11100` release-spin. So it is **not** confirmed same *root* as DR-1; downstream cause = a release-spin elsewhere, a Z80-execution mailbox (needs the full Z80 core), or a render/DMA bug (DR-3 class). Kept as a **re-test-after-fix** case, not a predicted pass. **TRIAGED** (`docs/2026-07-22-tf4-triage.md`): **NOT a Z80 mailbox** (0 Z80 reads) → does NOT need the Z80 core. Boot stalls on a **68k→VRAM DMA at `$0FF350`** that oracle-next runs with len=0 → 65536 words → **3.56M-cycle (~28-frame) CPU halt** (step count frozen at 531 for any budget). DMA-enable (reg1 bit4) reads 0 + length regs 0 → spurious/mis-decoded DMA trigger or length-capture bug. **Same family as DR-3.** **HALT FIXED** by the CD5 clear-on-consume fix (`docs/2026-07-22-vdp-dma-cd5-recon.md`, `b05786e`): the spurious len=0 DMA is gone, TF4 runs at normal step rate and renders (~128k px). **But a further init layer remains** — PC still cycles `$0FF35x` across f120–f1200; TF4 does not reach gameplay. Next-layer triage queued (`docs/2026-07-22-tf4-next-layer-triage.md`, pending). | **DR-2 halt DONE**; next init layer OPEN |
| **DR-3** | Batman & Robin | ~~garbled~~ → coherent, playable | ON (0x64) | ~~garbled tiles~~ → **renders correctly** | **RESOLVED by the CD5 fix** (`b05786e`) — NOT a separate bug. Re-triaged fresh (`docs/2026-07-22-batman-dr3-triage.md`): the garble does not reproduce post-fix; the pre-fix stale-CD5 phantom-DMA was corrupting Batman's heavy per-frame VRAM streaming. Overseer Oracle A/B: SEGA-logo CRAM 64/64 + VRAM art `$A000` **byte-identical** to the C++ reference. Human-confirmed **playable through level 1**. | **DONE (by CD5 fix)** |

## What this tells the roadmap

- **Z80 bus arbitration is the right next slice, now designed.** DR-1 is a *confirmed* release-spin on the Z80
  BUSREQ register `$A11100`; the recon+design doc `docs/2026-07-22-z80-busreq-recon.md` pins the register
  semantics (bit0 == 0 = 68000 granted; the stub's constant-0 satisfies *take-bus* but hangs *release*), proves
  it needs **only a bus-level arbitration state machine, not the full Z80 core**, and shows the fix cannot
  regress the frozen currencies (aeon's `startZ80` is poll-free). The disasm builds (Sonic 2, S&K) and s4.bin
  pass because they use only the take-bus form the stub already satisfies. **TF4's post-fix behavior chooses
  the next rock** (full Z80 core vs. the DR-3 render thread).
- **Batman (DR-3) is a separate VDP/DMA render thread** — do not fold it into the Z80 work. **Update
  (2026-07-22): DR-2 TF4 triaged into this same VDP/DMA rock** — TF4's blocker is a runaway 68k→VRAM DMA
  (`docs/2026-07-22-tf4-triage.md`), not Z80. So the **next rock = the VDP/DMA thread (DR-2 + DR-3 together)**;
  the full Z80 core stays deferred (no triaged game needs Z80 *execution*).
- **Cadence for widening the ROM set:** don't. Six diverse engines already surfaced the top root cause; more
  ROMs now mostly re-hit DR-1/DR-3. Fix the roots, *then* widen to confirm and find the next layer.

## Repro (any session)

```
BR=target/release/examples/boot_rom          # cargo build --release --example boot_rom -p oracle-core
"$BR" "<rom>" 1800 /tmp/probe                 # render + /tmp/probe.regs.txt (PC, VDP regs)
tail -c +16 /tmp/probe.ppm | tr -d '\000' | wc -c   # nonzero pixel bytes (0 = all black)
grep -E '^pc |^vdp r01 ' /tmp/probe.regs.txt  # PC (stuck loop?) + display-enable (r01 bit6 = 0x40)
```
