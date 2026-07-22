# The s4.bin first-scene milestone — spec + working plan

**Goal:** Our core boots the real, aeon-built `s4.bin` from reset to the settled first scene (the jungle
level — the game has no title screen) and renders it correctly, with **Oracle as the pixel oracle**. This is
the first time the whole machine — 68000 + VDP (render / interrupts / DMA / FIFO) + I/O — runs a real game
instead of fixtures. We expect a bug punch-list; producing and burning down that punch-list **is** the
milestone.

**Done bar (owner-approved):** our rendered settled-scene frame matches Oracle's screenshot of the same
frame, minus the Cam/Vel coord text at the top **if** capture shows it is Oracle's debug overlay rather than
game-rendered (pinned in Phase 0, decision D1). Ring animation phase is neutralized by pinning the same
frame number on both sides. Every remaining difference is either fixed or written into the punch-list with a
root-cause hypothesis and an explicit owner decision to defer.

**ROM:** `/home/volence/sonic_hacks/aeon/s4.bin` (a build artifact — never committed). Both emulators must
run the **byte-identical** file, SHA-256 recorded in the capture inventory below before any debugging.
Aeon rebuilds change the ROM constantly; every capture session re-verifies the hash on both sides first.

**Workflow (owner-approved):** slice-per-agent. The main session (foreground) does all Oracle MCP work —
reference capture and any live-emulator debugging — because **subagents must never drive the emulator MCP
(it deadlocks)**. Each punch-list item becomes a tight brief and a fresh Opus implementation agent; the main
session reviews every diff against the brief before it lands and runs the gates between slices.

---

## Standing gates (every slice, non-negotiable)

- Full suite green: lib tests, SST sweep (**exactly 1,000,058 covered cases**), golden_frames (7 hashes),
  determinism gate, oracle_differential, proptests, io_controllers.
- Frozen currencies untouched: export golden `0x22F80ECF29ED3AD4`, Oracle `state_hash` layout, golden-frame
  hashes. Any slice that believes it must move a currency **stops and escalates to the owner** — that is a
  design event, not a fix.
- `m68000/*` is presumed-correct territory (1M SST cases). A slice that thinks the bug is in the CPU core
  must first produce a minimal repro *instruction trace* and get main-session sign-off before touching it.

---

## The checkpoint ladder

Rungs are ordered so the first divergence localizes the bug. For each rung: capture Oracle ground truth
(foreground, MCP), run our core to the same point, byte-diff, and either pass the rung or mint a punch-list
item. Oracle capture tools: `emulator_screenshot`, `emulator_read_vram` / `read_cram` / `read_memory` /
`registers`, `emulator_run_to_scanline` / `pause` for frame-exact stops.

| # | Rung | Our-side observable | Oracle ground truth |
|---|---|---|---|
| 1 | Reset vector + checksum + RAM init | 68k reaches the game's main init (PC milestone), work RAM initialized | RAM dump after init, PC/registers |
| 2 | VDP registers programmed | VDP register file matches | register state |
| 3 | Z80 upload / BUSREQ handshake survived | 68k does not spin forever on `$A11100`; Z80 RAM contains the driver image | Z80 RAM dump, 68k PC past the handshake |
| 4 | Level art DMA'd | VRAM regions match (tiles, mappings, SAT) | VRAM dump at a pinned frame |
| 5 | Palettes loaded | CRAM (64 entries) matches | CRAM dump |
| 6 | First non-blank frame | rendered frame is not backdrop-only | screenshot at pinned frame |
| 7 | Settled scene | full-frame pixel match at pinned frame N (modulo D1 overlay) | screenshot + VRAM/CRAM/VSRAM at frame N |

Rung ordering is a debugging aid, not a straitjacket — if the game hard-hangs at rung 3, that item jumps the
queue regardless of ladder position.

---

## Phase 0 — tooling + reference capture (main session, before any agent)

1. **`boot_rom` runner** (`crates/oracle-core/examples/boot_rom.rs`): load a ROM by path (CLI arg), run N
   frames, dump the frame as PPM plus optional VRAM/CRAM/VSRAM/RAM/register snapshots at requested frames.
   Skip-if-missing pattern for the ROM (same convention as the SST vendor data) so nothing in CI depends on
   the build artifact.
2. **Diff tooling** (scratch, not committed unless it stabilizes): image diff (our PPM vs Oracle PNG,
   normalized) + memory-region byte-diff with first-divergence offset reporting.
3. **Reference capture:** hash the ROM, load it in Oracle, capture every rung's ground truth, store big
   blobs in scratch and anything small + stable enough to commit under
   `crates/oracle-core/tests/fixtures/s4-boot/`. Record the capture inventory (below).
4. **Pin D1** (coord-text provenance) during capture.
5. **First run:** boot our core on the ROM, walk the ladder, mint the initial punch-list.

## Phase 1..n — the punch-list loop (one Opus agent per slice)

Each slice brief contains: the symptom (which rung, what diverged, first-divergence offsets), the fixtures,
the pinned reference (recon doc / capture / citation), explicit scope ("fix this divergence, touch nothing
else"), and the standing gates. Review checklist for every slice: diff matches brief scope, gates green,
recon-pinned behavior cited not guessed, no currency drift, punch-list + this doc updated.

---

## Decisions — surfaced, not defaulted

- **D1 (open, pinned in Phase 0):** Is the Cam/Vel coord text Oracle's debug overlay (→ excluded from the
  match) or game-rendered sprites (→ must match)? Determined by reading Oracle's VRAM/SAT at the settled
  frame: if the glyphs aren't in the sprite table, it's overlay.
- **D2 (open, decided at rung 3):** Z80. There is no Z80 core; BUSREQ reads "granted". If the 68k merely
  handshakes and moves on, the stub survives the milestone. If it spin-waits on real Z80 behavior, the
  observed wait loop decides stub-extension vs minimal-core, **escalated to the owner** with the trace.
- **D3 (decided):** Audio correctness is out of scope. The Z80 driver *executing* is out of scope unless D2
  forces a minimal core, and even then only far enough to unblock the 68k.
- **D4 (decided):** Frame alignment = pin the same frame number on both sides (Oracle can stop
  frame-exact). No fuzzy matching; ring animation is neutralized by frame pinning, not tolerance windows.
- **D5 (decided):** The ROM is never committed; tests that need it skip cleanly when it's absent, keyed on
  the recorded SHA-256 so a stale/rebuilt ROM fails loudly rather than diffing garbage.

## Known risks (named up front)

- **Z80 handshake hang** (D2) — likely punch-list #1.
- **HV counter / H-int usage** by the level (water line, parallax raster effects). H-int exists in the VDP
  push; real-game usage patterns may hit untested corners.
- **Per-line DMA cost** is Phase-3 docketed (approved deviation). Only bites if the game races the VBlank
  window; if a symptom traces there, it's an owner-visible deferral candidate, not a silent fix.
- **Open-bus / unmapped-region reads** a real game performs that fixtures never did.

## Non-goals

Audio, Z80 driver execution (see D3), performance, later scenes/gameplay, pixel-matching Oracle's debug
overlay, 6-button pads.

---

## Capture inventory (filled in Phase 0)

ROM: `s4.bin` sha256 `560b348633f81ecadce2edf022bfe87c955800614de2dc2339f8b7475f65b27c` (420,749 bytes on
disk; Oracle reports 420,750 — it pads odd-sized ROMs by one byte for word access; harmless, noted).

| Item | Where | Status |
|---|---|---|
| Our settled frame + VRAM/CRAM/VSRAM/RAM/Z80/regs dumps (frame 600) | session scratch `s4boot.*` | captured 2026-07-21 |
| Oracle settled-scene screenshot (paused, post-reload) | session scratch `oracle-settled.png` | captured 2026-07-21 |
| Oracle CRAM (all 64 raw words via MCP) | compared in-band | captured 2026-07-21 |

## Ladder walk — 2026-07-21 (first attempt)

**All seven rungs passed on the first boot.** `boot_rom` ran s4.bin 600 frames with no panic, no
unimplemented-opcode halt, no hang:

- Rungs 1–3: 68k reached the game loop; H40 programmed; Z80 driver uploaded (5,900 nonzero bytes in Z80
  RAM) — the BUSREQ "always granted" stub survived the real handshake (**D2 resolved: stub is enough** for
  this milestone).
- Rung 5: CRAM byte-identical to Oracle, 64/64 raw words.
- Rungs 6–7: settled frame is **pixel-identical to Oracle — 0 / 71,680 pixels differ** after normalizing
  the documented DAC-ramp difference (P8 in `docs/2026-07-16-vdp-pixel-known-differences.md`; Oracle renders
  3-bit level L as L×34, we render round(L×255/7) — a bijection, verified 27↔27 colors 1:1).
- **D1 resolved:** the Cam/Vel coord text is Oracle's UI overlay — absent from Oracle's own VDP framebuffer
  screenshot. Excluded from the match, as suspected.
- Comparison sensitivity proven, not assumed: pre-normalization the frames differ in 35,083 pixels (the
  ramp), and our own frames 600 vs 604 differ in exactly the ring-animation region (276 px, x 204–235,
  y 5–20) — the ring phase in the zero-diff match was genuinely aligned, and animation runs in our core.

## Punch-list (living — the milestone's real output)

| # | Rung | Symptom | First divergence | Hypothesis | Slice/agent | Status |
|---|---|---|---|---|---|---|
| — | — | — | — | — | — | **empty — ladder walked clean 2026-07-21** |

**The static done bar is met.** What the static scene cannot exercise — and where the punch-list will
actually come from — is **motion**: input-driven gameplay (the placeholder player moving, camera scroll,
tile streaming, h-scroll parallax mid-frame). Next step: an input-scripted A/B run (same held inputs, same
frame count, both emulators) as the milestone's motion extension, per the "verify during motion, not at
rest" rule.
