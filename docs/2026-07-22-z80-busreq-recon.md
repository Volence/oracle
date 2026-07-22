# Z80 bus-arbitration recon + design — the `$A11100`/`$A11200` handshake (first slice of the Z80 rock)

**Status: RECON + DESIGN, 2026-07-22. Docs only — no code this slice.** Grounds the first correctness
work after the s4-boot milestone. The first differential ROM sweep
(`docs/2026-07-22-differential-rom-findings.md`) surfaced **DR-1 (Gunstar Heroes)** as the top correctness
gap with hard evidence: a permanent freeze spinning on the Z80 BUSREQ register `$A11100`, which oracle-next's
"always-granted" stub never drives the way the game's handshake expects. This doc (1) pins the exact hardware
semantics of `$A11100`/`$A11200` from clean-room reference **and from two disassembled commercial games**,
(2) reproduces DR-1 and re-classifies DR-2 (Thunder Force IV) against the same root, (3) surfaces — does
**not** default — the load-bearing sequencing decision (bus-level arbitration vs. a full Z80 core), and (4)
gives a gated, gate-respecting implementation plan with Gunstar + TF4 as the acceptance set.

**Permitted sources only** (audit policy 3, identical to the VDP/IO recon docs): official Sega documentation,
**Plutiedev** (plutiedev.com), and **SpritesMind** hardware threads for corroboration. **No emulator source was
opened** (BlastEm / jgenesis / Genesis Plus GX). In addition — and this is the strongest evidence class here —
the **in-situ assembly of two real games** (Gunstar Heroes, Thunder Force IV, disassembled from the user's own
ROMs on disk, never committed, skip-if-absent per D5) is used as the primary ground truth. Where a paraphrased
secondary source disagrees with the games, **the games win** and the discrepancy is called out.

Items are numbered **Z1–Z7**. Each states the pin, the evidence, confidence, the behavioral-vs-timing class
(per `docs/reference/README.md` and the timing-vs-behavior rule), and the open remainder with its
pin-vs-defer disposition.

---

## Part 1 — Pinned hardware semantics

### Z1 — The two registers and their write polarity

**PINNED.** Two word-addressed control registers gate the Z80 subsystem from the 68000 side:

| Address | Register | Write `$0100` (bit0 = 1) | Write `$0000` (bit0 = 0) |
|---|---|---|---|
| `$A11100` | **Z80 BUSREQ** | **assert** BUSREQ — 68000 requests the Z80 bus | **release** BUSREQ — bus returns to the Z80 |
| `$A11200` | **Z80 RESET** | **release** reset — Z80 runs | **assert** reset — Z80 held in reset |

Note the **opposite** conventions: on BUSREQ, `1` = "I want the bus"; on RESET, `1` = "let it run" (`0` =
"hold it in reset"). The load-bearing bit is **bit 0 of the byte at the even address** (`$A11100` / `$A11200`).
Games write the whole word `$0100` because bit 0 of the byte at the even address is where the high byte of the
word lands (`move.w #$100,$A11100` puts `$01` at `$A11100`, `$00` at `$A11101`).

**Evidence**: Plutiedev "Using the Z80": *"Request Z80 bus by writing `$100` to `Z80BusReq`… Release Z80 bus by
writing `$000` to `Z80BusReq`"* and *"Assert Z80 reset by writing `$000` to `Z80Reset`… Release Z80 reset by
writing `$100` to `Z80Reset`."* In-situ: Gunstar `move.w #$100,$a11100` (assert) / `move.w #$0,$a11100`
(release) in the same routine pair (`sub_000D36` / `loc_000F28`); TF4 `move.w #$0100,($00A11100)` at `$00082C`.
**Confidence**: high. **Classification**: behavioral. **Open remainder**: none.

### Z2 — Read model of `$A11100` bit 0 — the load-bearing pin

**PINNED.** Reading `$A11100` bit 0 reports **who owns the Z80 bus**:

- **bit 0 == 1** → the **Z80 owns the bus** (it is running / the 68000's request is not yet acknowledged).
  The 68000 must **not** touch Z80 space (`$A00000–$A0FFFF`, and the `$A11xxx`/VDP path some code guards this
  way) yet.
- **bit 0 == 0** → the **68000 owns the bus** (BUSREQ acknowledged, Z80 halted). Z80 space is safe to access.

So the two canonical poll loops are:

```
; TAKE the bus: assert, then wait until GRANTED (bit0 -> 0)
    move.w  #$100, ($A11100)
@wait: btst  #0, ($A11100)
    bne.s   @wait                 ; loop WHILE bit0 == 1  (exit on 0 = granted)
    ; ... now safe to access Z80 RAM / do VDP DMA ...

; RELEASE the bus: deassert, then wait until the Z80 has it back (bit0 -> 1)
    move.w  #$0, ($A11100)
@rel:  btst  #0, ($A11100)
    beq.s   @rel                  ; loop WHILE bit0 == 0  (exit on 1 = Z80 running)
```

**Evidence (primary, dispositive)**: **Gunstar Heroes** uses *both* polarities in one routine pair —
`loc_000D44: btst #0,$a11100 / bne.b $d44` (take: wait while 1) and `loc_000F3A: btst #0,$a11100 / beq.b $f3a`
(release: wait while 0). The take-loop is immediately followed by a VDP control access (`lea $c00004,a0 …`),
proving "bit0 == 0 ⇒ safe to proceed." **Thunder Force IV** uses the take form at `$000834: btst #0,($A11100) /
bne` after asserting at `$00082C`. **Plutiedev** "Using the Z80" shows the identical wait loop
(`btst #0,(Z80BusReq) / bne.s @WaitZ80`). Three independent witnesses, two of them shipping games, agree.

**Discrepancy resolved**: a paraphrased SpritesMind forum snippet reads *"the ZBUSREQ bit returns 1 if z80 bus
can be accessed."* Taken literally that inverts the polarity above — but it conflates the **raw BUSACK pin**
(active-low, opposite sense) with the **register bit**, and it contradicts every wait loop in real code. The
game assembly is ground truth: **bit0 == 0 = 68000 granted.** The snippet is not used as a pin.

**Confidence**: high (three witnesses incl. two games). **Classification**: behavioral (the exact value a poll
loop reads for a given ownership state — a wrong polarity hangs or corrupts). **Open remainder**: none.

### Z3 — Grant timing: not instant on hardware, but "instant-grant" is safe to model

**PINNED (behavioral) + timing deferred.** On real hardware the grant is **not** immediate: the bus arbiter
lets the Z80 finish its current bus cycle before acknowledging, so the 68000 sees bit0 stay `1` for a few
reads, then flip to `0` (SpritesMind "Genesis 68K bus timing": the arbiter *"finishes its own access cycle
first before releasing the bus"*; Z80↔68k contention delays cited at ~3.3 Z80 / ~11 68k cycles average). **But
every real consumer polls in a loop** (Z2) precisely because the delay is unpredictable. A model that grants on
the **first** read (bit0 flips the instant BUSREQ is asserted) is therefore **observationally safe**: the poll
loop simply iterates once instead of a few times, and no game can distinguish "granted immediately" from
"granted after 3 cycles" — it only ever branches on the final value. The multi-cycle latency is a **timing**
property (BlastEm-differential class, deferrable), not a behavioral one.

**Confidence**: high (behavioral safety of instant-grant). **Classification**: the *value* is behavioral; the
*latency* is timing. **Open remainder**: exact grant/contention cycle cost — **deferred** (no acceptance-set
consumer observes it; pin source when needed: SpritesMind bus-timing threads + BlastEm-over-the-bus).

### Z4 — `$A11200` RESET and the RESET↔BUSREQ interaction

**PINNED (behavioral).** `$A11200` bit 0: `1` (write `$100`) releases the Z80 from reset → it runs; `0` (write
`$000`) asserts reset → the Z80 is held. The documented ordering to touch Z80 RAM from cold is: assert reset →
request the bus → **release reset** before the RAM access completes, because *"we can't access Z80 RAM while
Z80 is reset"* (Plutiedev "Using the Z80"). For the read model of Z2, the composite hardware condition for
"68000 may access" is **(BUSREQ acknowledged) AND (Z80 not mid-cycle)**; when the Z80 is **held in reset** the
bus is likewise free. For the **acceptance set** the reset line only ever gates *Z80 execution*, which does not
exist yet (Z7), so RESET is modeled as a plain latch that stores the last written bit and, optionally, also
reports the bus as grantable while asserted (a cheap hardware-faithful refinement, not required to unblock
Gunstar). **Confidence**: high (write semantics + ordering); medium (the "reset ⇒ auto-grant" refinement, from
one paraphrased source). **Classification**: behavioral (write semantics); the refinement is behavioral but
**deferred** as it has no acceptance-set consumer. **Open remainder**: whether real silicon reports bit0 == 0
purely from reset with BUSREQ deasserted — deferred (unobserved by Gunstar/TF4; pin from a hardware-test
thread before relying on it).

### Z5 — What a poll loop is actually waiting for (why the stub hangs)

**PINNED.** The current oracle-next stub (`bus.rs:287`) returns a **constant `0x00`** for `$A11100` reads =
"68000 always owns the bus." Map that onto Z2's two idioms:

- **Take-bus** (`bne`, loop while bit0 == 1): reads `0` → exits on the **first** poll. **Satisfied** by the
  stub. This is why every ROM tested so far boots — the aeon `s4.bin` handshake (`stopZ80`), Sonic 2, and S&K
  all use *only* the take form.
- **Release-and-wait** (`beq`, loop while bit0 == 0): reads `0` **forever** → **never exits**. **Hangs.** No
  ROM before the differential sweep exercised this form, so the gap was invisible.

The stub is thus correct for *acquisition* and wrong for *release*. A game that releases the bus and waits for
the Z80 to resume (bit0 → 1) spins forever. **Confidence**: high (mechanism traced to the exact line + the
game loop). **Classification**: behavioral. **Open remainder**: none — this is the DR-1 mechanism.

---

## Part 2 — DR-1 reproduced, DR-2 re-classified

### DR-1 (Gunstar Heroes) — **CONFIRMED**: `$A11100` release-spin

Gunstar's Z80-hold routine pair (disassembled in-situ; ~6 copies across the ROM, e.g. `$000D36`/`$000F28` and
the copy at `$003258`/`$003265` that matches the frozen PC from the sweep):

```
sub_000D36:  move.w #$100,$a11100   ; assert
loc_000D44:  btst #0,$a11100 / bne  ; take — waits for grant (bit0->0)  [stub OK]
             ... VDP access under Z80 hold ...
loc_000F28:  move.w #$0,$a11100      ; release
loc_000F3A:  btst #0,$a11100 / beq  ; release — waits for Z80 (bit0->1) [stub HANGS]
```

The differential sweep froze Gunstar at PC `$3090` with display **off** — inside the `beq` release-spin of the
`$3258` copy. The always-`0` stub never lets bit0 reach `1`, so the `beq` loops forever, the boot sequence
never completes, and the display is never enabled. **This is exactly Z5's hang, on real game code.** Fixing Z2
(read reflects the BUSREQ latch: `1` after release) resolves it directly.

### DR-2 (Thunder Force IV) — **RE-CLASSIFIED**: same handshake *family*, not confirmed same *root*

The original findings hypothesized TF4 was "likely the same Z80/handshake family." Investigation **refines**
that:

- **TF4 does use the `$A11100` handshake** — 70 access sites in the ROM binary; the boot stop/start routines at
  `$00082C` (assert + `btst #0 / bne` take-loop) and `$000840` (release, no poll) are the standard idiom. The
  static disasm left these regions as data, which is why a source grep found nothing; the **binary** does not.
- **But TF4's observed hang is not a raw `$A11100` release-spin.** At 1200 frames it sits at PC `$0FF3E8`
  (ROM, display **on**, `r01 = $44`), inside a **boot init-script / VDP-upload loop**: `$0FF3E8` is a
  `lea data,a0 / bsr.w $0FF490` step; `$0FF490` is a **command-dispatch interpreter** (scans a byte table for
  a match and dispatches); `$0FF30A` **copies a routine into `$FF0000` and programs the VDP**; the enclosing
  `$0FF354`/`$0FF388` loops are `dbf`-counted **VDP data uploads** (`move.w (a0,d0),(a6)` with `a6 = $C00000`).
  The sweep's `$FF388↔$FF354` oscillation is this upload/dispatch machinery cycling — **not** a `btst $A11100`
  spin.

**Disposition**: TF4's take-bus form is *already* satisfied by the stub (like everyone else), so its blank
screen is a **downstream** dependency, one of: (a) a release-spin at a code site the sampled PC didn't land on;
(b) a **Z80-RAM mailbox** the running sound driver would set — which needs actual **Z80 execution** (Z7), not
just arbitration; or (c) a render/DMA data-flow bug in the same class as DR-3 (Batman). **TF4 is therefore
kept in the acceptance set as a *re-test-after-fix* case, not a *predicted pass*.** The bus-level fix is
necessary groundwork for TF4 but may not be sufficient — and that is an honest, evidence-backed correction to
the punch-list, not a regression.

---

## Part 3 — The sequencing decision (surfaced, not defaulted)

**The question.** Can correct BUSREQ/RESET arbitration be modeled at the **bus level** — a grant/ack/release
state machine with **no Z80 instruction execution** — as a bounded fix that unblocks Gunstar (and re-tests
TF4)? Or does it genuinely require standing up the **full Z80 core** first?

**Finding.** The arbitration handshake is **purely bus-ownership signaling**. Everything the 68000 observes
(Z2, Z4) is a function of *which processor owns the bus* and *the latched BUSREQ/RESET bits* — never of *what
the Z80 computes*. The 68000's poll loops branch only on bit0 = "granted / not granted." Modeling that requires
one boolean latch (BUSREQ) plus one boolean latch (RESET); it does **not** require decoding a single Z80
opcode. The full Z80 core is a much larger rock (opcode decode, its own scheduler slice at mclk/15, FM/PSG
wiring, sound-driver semantics) and is only needed when *what the Z80 does with the bus* becomes observable —
i.e. **sound**, and any **Z80-RAM mailbox** a game's 68000 side reads back expecting the driver to have updated
it.

**Recommendation — do the bounded bus-level arbitration first.**

| | Bus-level arbitration state machine (**recommended, first**) | Full Z80 core first |
|---|---|---|
| Scope | ~2 latches + read/write decode at `$A11100`/`$A11200`; `bus.rs` only | Z80 CPU, scheduler slice, FM/PSG, sound driver semantics |
| Unblocks | **Gunstar (DR-1) — confirmed.** Necessary step for TF4 (re-test). | Same, *plus* sound + Z80-RAM mailboxes |
| Risk to frozen gates | **None** (Z6 proof below): touches no instruction semantics, no exported state | Large surface; export-layout v2, new state_hash bytes, scheduler changes |
| Effort | Small, one bounded slice | The whole Z80 rock (multi-slice) |
| Leaves open | Sound; any true Z80-execution mailbox dependency (possibly TF4) | — |

The bus-level fix is **strictly on the path** to the full core (the same latches are what the eventual Z80
core will read to decide whether it's allowed to run), captures the **confirmed** win now, and cannot regress
the frozen currencies. If TF4 remains blocked after it, that is the **evidence** that promotes the full Z80
core (or the DR-3 render thread) to next — a decision better made *with* that datum than speculatively before
it. Recommendation: **ship arbitration first; let TF4's post-fix behavior choose the next rock.**

---

## Part 4 — Gated implementation plan: bus-level Z80 arbitration

A single bounded slice in `crates/oracle-core/src/bus.rs`. **No `m68000/*` changes, no exported-state changes,
no SST changes.**

### Z6 — Gate-safety proof (why this respects every frozen currency)

- **SST (1,000,058-case invariant)** — pure m68000 instruction semantics; bus arbitration touches no opcode.
  **Unaffected.**
- **Export golden `0x22F80ECF29ED3AD4` + state_hash layout** — the BUSREQ/RESET latches are **bus-internal**
  and are **not** added to `export_state`; the frozen layout (…Z80 RAM `0x2000` + zeroed `0x40` Z80-reg
  placeholder…) is unchanged. The latches live beside the VDP/IO state, which is likewise not exported.
  **Unaffected by construction.**
- **7 golden_frames** — captured from aeon `s4.bin`, whose only `$A11100` use is `stopZ80` (take-bus) with a
  **poll-free `startZ80`** (`macros.asm:226–235`: `startZ80` writes `$0` and does **not** read back). So
  s4.bin reads `$A11100` **only inside the asserted window**, where an instant-grant model returns `0` —
  **identical** to today's constant-`0` stub. The differing value (bit0 = `1` while *unasserted*) is **never
  read** by s4.bin. Therefore the 7 golden_frames render **byte-identical** after the change. This is made an
  **explicit regression gate** (below), not an assumption.

### Design (instant-grant, hardware-faithful value)

State added to `MegaDriveBus` (or its owned bus-register block):

```
z80_busreq: bool   // last bit0 written to $A11100 (true = asserted/requested)
z80_reset:  bool   // last bit0 written to $A11200 (true = running; false = held in reset)
```

- **Write `$A11100`** (byte/word, bit 0): `z80_busreq = (byte & 1) != 0`. (Word `$0100` → byte `$01` at the
  even address → `true`; `$0000` → `false`.)
- **Write `$A11200`** (bit 0): `z80_reset = (byte & 1) != 0`.
- **Read `$A11100`** bit 0 = `0` **iff** `z80_busreq` (instant grant; optionally `|| !z80_reset` as the Z4
  refinement) — i.e. `read_bit0 = if z80_busreq { 0 } else { 1 }`; other bits `0`. **Read `$A11200`** returns
  the latched reset bit (or `0`; no acceptance-set consumer reads it — keep minimal, mirror the latch).
- Replace the constant at `bus.rs:287` (`0xA1_1100..=0xA1_1101 | 0xA1_1200..=0xA1_1201 => Some(0x00)`) with the
  latch-driven read; add the write cases in `store_byte` (currently these addresses fall through and drop).

### Gates (ordered; each must pass before the next)

1. **Semantics unit tests** (new, in `bus.rs`): assert `$A11100` → read bit0 = 0 (granted); release → read
   bit0 = 1 (Z80 owns); the take-bus idiom exits, the release idiom exits. Replaces/extends the existing
   `z80_busreq_reports_the_bus_granted` test (`bus.rs:833`), which currently asserts the *old* constant-0
   behavior and **must be updated** as part of the slice.
2. **Frozen-currency regression gate**: re-run the export-golden test, the 7 golden_frames, and the full SST.
   All three MUST be **unchanged** (Z6 predicts byte-identical). Any diff is a stop-and-investigate, not a
   re-baseline.
3. **Acceptance — Gunstar (DR-1)**: `boot_rom "…/Gunstar Heroes (USA).md" 1800 /tmp/gun` must boot **past** the
   `$3090` release-spin (PC advances beyond the loop), **enable the display** (`r01` bit6 set), and render a
   non-black frame (`tail -c +16 … | tr -d '\000' | wc -c` > 0). This is the confirming acceptance for the
   whole slice.
4. **Acceptance — TF4 (DR-2), diagnostic (not pass/fail-gating)**: re-run TF4 and record the new stuck PC /
   render. Three outcomes, each actionable: **renders** → TF4 was a second release-spin, done; **still blank at
   `$0FF3xx`** → confirms a Z80-execution mailbox or render dependency → routes TF4 to the **full Z80 core** or
   the **DR-3 render thread** with evidence; **new PC** → re-triage. TF4's result **chooses the next rock** (Part 3).

### Out of scope for this slice (named, not silent)

- **Z7 — the full Z80 core** (opcode execution, mclk/15 scheduler slice, FM/PSG, sound-driver semantics). This
  is the rock the arbitration latches lead into: the eventual core reads exactly `z80_busreq`/`z80_reset` to
  decide when it may run. Sound and any true Z80-RAM mailbox dependency (possibly TF4) live here.
- **Grant/contention cycle-accurate timing** (Z3) — deferred; no acceptance-set consumer observes it.
- **6-button / serial / TH-interrupt** — unrelated (already deferred in `docs/2026-07-17-io-recon.md` IO6).
- **DR-3 (Batman & Robin)** — separate VDP/DMA render bug; explicitly not folded in.

---

---

## Part 5 — Implementation outcome (2026-07-22, slice shipped for review)

The bus-level `z80_busreq` slice was implemented per Part 4 (overseer steer: minimal
`read_bit0 = if z80_busreq { 0 } else { 1 }`; the `z80_reset` refinement dropped as out-of-scope — `$A11200`
read unchanged, its write still drops; latch threaded like `last_bus_word`, not in `export_state`).

**Gates:**
- **#1 semantics** — `z80_busreq_reflects_the_request_latch` (bus.rs): power-on → bit0 = 1; assert (word/byte)
  → 0; release → 1; even-byte-only latching. Watched it RED (power-on read 0, wanted 1) then GREEN. Full
  lib suite **595/595**.
- **#2 frozen currencies — byte-identical (the Z6 proof, confirmed empirically):** export golden
  `v1_golden_hash_is_frozen` ✓, all **7 golden_frames** ✓, determinism gate ✓, **SST 112/112** anchor cases
  match SingleStepTests (845 s), 0 diffs. (SST runs on `FlatBus`, structurally isolated from the bus change.)
- **#3 Gunstar (DR-1):** **the confirmed BUSREQ freeze is RESOLVED** — PC advanced from the frozen `$3090`
  release-spin to `$66360`, executing freely. **But full render is not yet reached**: Gunstar then hangs on a
  **second, independent blocker** — a **YM2612 FM busy-flag poll** at `$A04000` (`lea $A04000,a0; btst #7,(a0);
  bne`). oracle-next maps `$A04000–$A04003` (the FM chip) as **Z80 RAM**, so the game's write of an FM register
  address ≥ `$80` (observed: `$F3` in `z80_ram[0]`) leaves bit7 set and the busy-poll spins forever. A
  **throwaway** experiment (FM status → not-busy for `$A04000–$A04003`, immediately reverted) proved the point:
  with the BUSREQ fix **plus** a not-busy FM read, Gunstar boots to its main loop (`$450`), **enables the
  display** (`r01 = $64`), and renders a **fully non-black** frame (71 680/71 680 bytes). So gate #3's
  render half is gated behind a bounded **FM-status bus stub**, not the full Z80/FM core.
- **#4 TF4 (DR-2) diagnostic:** **unchanged** by the fix (PC `$0FF354`, display on, blank) — confirms TF4 is
  *not* Gunstar's root; its proximate loop is the VDP-upload region. Routes to the next rock (FM-status,
  Z80-mailbox, or the DR-3 render thread) per Part 3.

**What Gunstar taught us (the "learn from it" the overseer asked for): the next bounded correctness win is an
FM-status bus stub** — reads of `$A04000–$A04003` should report the YM2612 as **not busy** (bit7 = 0) instead
of aliasing Z80 RAM. It is the same *shape* of fix as this slice (a small, gate-safe bus-mapping correction,
**not** the full Z80/FM core) and, combined with this slice, is **proven** to render Gunstar. Recommended as
the immediate follow-up recon+slice; whether TF4 also clears with it is the first thing to check.

## Appendix — primary byte evidence (from the user's ROMs, on disk only)

Gunstar Heroes (`Gunstar Heroes (USA).md`), `gunstar_disasm/code/disasm.asm`:
- `L495 move.w #$100,$a11100` / `L498 btst #0,$a11100` / `bne` (take) ; `L627 move.w #$0,$a11100` /
  `L630 btst #0,$a11100` / `beq` (release-spin — the DR-1 hang). ~6 copies incl. `$3258`/`$3265`.

Thunder Force IV (`Thunder Force IV (U).bin`), decoded from the binary (disasm leaves these as data):
- `$00082C: 33FC 0100 00A1 1100` `move.w #$100,$A11100` ; `$000834: 0839 0000 00A1 1100` `btst #0,$A11100` ;
  `$00083C: 66F6` `bne $834` (take) ; `$000840: 33FC 0000 00A1 1100` `move.w #$0,$A11100` (release, no poll).
  70 `$A11100` access sites total. Stuck PC `$0FF3E8` is a `bsr.w $0FF490` dispatch step, not a `btst $A11100`.

aeon `s4.bin`, `engine/macros.asm:226`: `stopZ80` = assert + `btst #0 / bne` (take) ; `startZ80` = write `$0`,
**no poll** — the basis of the Z6 golden-frames proof.

## Sources

- [Plutiedev — Using the Z80](https://plutiedev.com/using-the-z80) (register writes, wait loop, reset ordering)
- [Plutiedev — Mega Drive hardware notes (Kabuto)](https://plutiedev.com/mirror/kabuto-hardware-notes)
- [SpritesMind — Genesis 68K bus timing](https://gendev.spritesmind.net/forum/viewtopic.php?t=2943)
- [SpritesMind — DEFINITIVE info about Z80 BUSREQ, RESET?](https://gendev.spritesmind.net/forum/viewtopic.php?t=2195)
- Primary: in-situ disassembly of Gunstar Heroes and Thunder Force IV (user's ROMs, never committed, D5).
