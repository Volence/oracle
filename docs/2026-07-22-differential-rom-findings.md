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
| **DR-1** | Gunstar | `$3090`→`$3090` (frozen) | OFF (0x24) | hard-stuck, display never enabled | **CONFIRMED: Z80 BUSREQ spin-wait.** Loop at `$3088`: `move.w #0,($A11100)` / `btst #0,($A11100)` / `beq $3088` — waits for `$A11100` bit 0, which oracle-next's "always-granted" Z80 stub never drives to the expected value. **The Z80 bus-arbitration gap** (`recon-io`: "BUSREQ/RESET arbitration lands with the Z80 core"). | OPEN — Z80 rock |
| **DR-2** | Thunder Force IV | `$FF388`→`$FF354` (looping) | ON (0x44) | display on, permanently blank; loops near `$FF3xx`, never loads art | Likely the same Z80/handshake family (confirm: disassemble the `$FF3xx` loop, check for an `$A11100`/`$A11200`/Z80-status poll). | OPEN — confirm vs DR-1 |
| **DR-3** | Batman & Robin | `$9C10`→`$9C14` (advancing) | ON (0x64) | executing + drawing, but garbled tiles (intermittent non-black) | Separate **render/DMA correctness bug** — game runs, VRAM/mapping content wrong. Not Z80. Needs an Oracle A/B at a matched frame + VRAM diff. | OPEN — render thread |

## What this tells the roadmap

- **The Z80 core is the right next rock, now evidence-backed.** DR-1 is a *confirmed* spin-wait on the Z80
  BUSREQ register; proper `$A11100`/`$A11200` BUSREQ/RESET arbitration would very likely unblock Gunstar and
  (pending confirmation) TF4 — a real game exposes the handshake the aeon ROM's looser use never did. The
  disasm builds (Sonic 2, S&K) pass because their handshake happens to be satisfied by the stub.
- **Batman (DR-3) is a separate VDP/DMA render thread** — do not fold it into the Z80 work.
- **Cadence for widening the ROM set:** don't. Six diverse engines already surfaced the top root cause; more
  ROMs now mostly re-hit DR-1/DR-3. Fix the roots, *then* widen to confirm and find the next layer.

## Repro (any session)

```
BR=target/release/examples/boot_rom          # cargo build --release --example boot_rom -p oracle-core
"$BR" "<rom>" 1800 /tmp/probe                 # render + /tmp/probe.regs.txt (PC, VDP regs)
tail -c +16 /tmp/probe.ppm | tr -d '\000' | wc -c   # nonzero pixel bytes (0 = all black)
grep -E '^pc |^vdp r01 ' /tmp/probe.regs.txt  # PC (stuck loop?) + display-enable (r01 bit6 = 0x40)
```
