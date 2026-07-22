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

**ROM re-pinned 2026-07-22** (aeon rebuilds constantly): current `s4.bin` sha256
`db0eb03d767a751b348f10a87ab0176e1e33adb8b9164c3e1ad5a7f43d080ab2` (420,749 bytes, built 02:19). Both
emulators verified running this byte-identical file before the 2026-07-22 A/B — 3 build-specific offsets
matched Oracle's loaded cart exactly (vectors `0x08` = `0005CAB0…`, code entry `0x5CAB0` = `4EB90005CC0A…`,
deep data `0x40000` = `00000000…2222`). All 2026-07-22 findings below are on this ROM.

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

## Motion A/B — first session, 2026-07-21 (foreground, motion_run 37f4339 + Oracle MCP)

**Headline: when frames align, the emulators are pixel-identical even mid-spawn.** Oracle stepped to
"frame 60" from reset matched our frames 56/57/58 with **0/71,680 differing pixels** (ramp-normalized);
the 966-pixel residue at other offsets is pure ring-animation phase. Motion rendering itself shows no
divergence so far: our deep-scroll frame under held Right reaches the same terrain Oracle reaches under a
live free-run hold.

**Blocker found (Oracle-side, not oracle-next):** `emulator_press` — the deterministic
hold-N-frames-and-pause input tool — advances frames bit-exactly but its held buttons **never reach the
game**: Right held via press for 298/598 frames from reset AND for 120 frames from a settled scene all
produced zero player/camera motion, while the free-run `emulator_hold` path scrolls normally. Frame-exact
input A/B is blocked until Oracle's press path injects into the pad model during stepped frames. Filed
with the owner for an Oracle-side fix.

**Protocol notes (hard-won, keep):**
- `emulator_reset` is **deferred** — it applies when the next frame steps, and the following press
  effectively starts at reset frame 0 (minus a 2–4 frame constant offset; calibrate per session by
  matching a screenshot against our neighbour frames — the zero-diff match is unambiguous).
- Oracle's `frame_token` is a UI/wall-clock counter, **not** an emulated-frame count — never use it for
  frame accounting.
- `emulator_screenshot`'s `path` parameter is ignored; shots land in `~/.config/oracle/screenshots/`.
- Pause → reload does not survive: `emulator_reload_rom` resumes the emulator.

| # | Rung | Symptom | First divergence | Hypothesis | Slice/agent | Status |
|---|---|---|---|---|---|---|
| M-1 | motion | Oracle MCP `emulator_press` produces zero/inconsistent motion during stepped frames | n/a (tooling, Oracle repo) | ~~press injects at a layer the core only samples in free-run~~ **DISPROVEN** → press advances a *non-deterministic* frame count (render-token race) | Oracle-side session | **RESOLVED + overseer-verified live 2026-07-22: 3× `reset→press right 120` byte-identical (Camera_X 0x0060, PC 0x5B4C, all regs); settled press scrolls camera → input reaches gameplay** |

### M-1 UPDATE — 2026-07-22 (Oracle-side forensic result)
Our filed hypothesis ("held buttons never reach the game") is **disproven** on the current Oracle build.
An Oracle agent verified under `ORACLE_DETERMINISTIC=1`: hold-right → the game's `Ctrl_1_Held ($FF802C)`
reads the Right bit; press right 120 frames → `Camera_X` 0x60→0x0BC0 (moves), press left → 0x60→0x00
(opposite), no-input control stays 0x60 — motion is genuinely input-caused. The literal "input doesn't
reach the game" symptom was a **deterministic-mode line-change orphan bug already fixed 2026-07-02
(7f88ce7)**. **The real, still-live defect:** `press` (main_gui.cpp:2094-2102 / 2288-2329) drives async
`RunSystem()` + counts `GetImageLastRenderedFrameToken()` (render-thread frame count) to decide when to
stop → it advances a **non-deterministic** number of emulated frames (3 identical `press right` → 3
different state hashes AND camera 0x0660/0x0650/0x0640), while `run_frames` (ControlSocket.cpp:2247) steps
exact `kNtscFrameNs` quanta via synchronous `ExecuteSystemStep` and is bit-exact. This non-determinism is
the true cause of the "zero motion" WE saw (press likely stepped ~0 frames in our runs) and it blocks
frame-exact A/B. **Fix greenlit (owner/overseer):** route the MCP `emulator_press` path through the same
deterministic `ExecuteSystemStep(kNtscFrameNs)×N` stepping `run_frames` uses; input injection untouched.
**Acceptance bar (unblocks our A/B):** 3 identical `emulator_press right N` from reset → identical state
hash + identical `Camera_X`, matching a deterministic `run_frames`+injected-input reference; plus the exact
env/config for deterministic press over the MCP socket. Overseer to verify the fix live once it lands.

**RESOLVED Oracle-side 2026-07-22.** The Oracle agent rewrote the MCP `emulator_press` path (`OpPress` in
`ControlSocket.cpp`) to run synchronously on the socket thread and step the same
`ExecuteSystemStep(kNtscFrameNs)×N` quantum as `run_frames` (shared `kNtscFrameNs` constant; no more GUI
main-loop marshaling / render-token counting). Agent-verified STRONGEST: 3× identical `press right` from
one reset → byte-identical state hash AND Camera_X, and press == a `hold+run_frames+release` reference
(both `0xE0C827EA14DD4041`, cam `0x0420`); regression guards intact; test
`linux-port/harness/press_determinism_test.py` fails-old/passes-new. **PROTOCOL PIN for A/B:** bit-exact
press requires the *oracle_gui process that owns the socket* to run with `ORACLE_DETERMINISTIC=1` (NOT the
MCP server — it only connects to an existing `oracle.sock`; the harness `launcher.py:42` already sets it).
Without it press still steps exactly N frames but isn't bit-exact (threaded exec). Not a global default
(that's the future `set_deterministic` bus op). **Overseer live re-verify PENDING**: the running instance
(PID 3366703) is the stale pre-fix binary — must restart the GUI on the new build + `ORACLE_DETERMINISTIC=1`,
then reproduce 3×-identical over MCP and run the real frame-exact motion A/B vs our core. Oracle-side
housekeeping flagged: `launcher.py` `headless_emulator` leaks oracle_gui grandchildren + Xvfb (xvfb-run
terminate() misses them) → `start_new_session=True` + `os.killpg` fix (separate from the press commit).

## Frame-exact motion A/B — RUN 2026-07-22 (overseer, foreground MCP). Verdict: OUR CORE CORRECT, ORACLE HAS A NEW BUG

The payoff A/B ran. It did **not** produce the clean pixel-match we expected — it produced a genuine
**motion divergence**, and the milestone's own rule (localize the first divergence, mint the punch-list item)
applies. **Determined rigorously: oracle-next executes this ROM's input-driven motion correctly; Oracle (the
C++ emulator) does not.** This is the milestone's real output — the first bug the whole-machine A/B has
surfaced, and it is Oracle-side.

**Setup (all verified):** both emulators on byte-identical ROM `db0eb0…` (3 offsets, above). Oracle press is
deterministic (2× `reset→press right 120` byte-identical: Camera_X `0x0060`, PC `0x5B4C`, all 16 regs). The
M-1 press-determinism fix holds. **But determinism ≠ correctness**, and that is exactly the trap the earlier
overseer "3×-identical" check fell into (see below).

**The scenario:** the active player is `games/sonic4/objects/test_player.asm` running in **debug free-flight
mode** (the yellow box — art `$A0FA`, mapping `Map_TestObj`; `TestPlayer_Debug`). In that mode the game does,
per frame, `move.b (Ctrl_1_Held).w,d0` → `btst #3,d0` (RIGHT) → `add.l d1,SST_x_pos(a0)` with
`d1 = DEBUG_FLY_SPEED<<16`, `DEBUG_FLY_SPEED = 16`. **Held Right must fly the box right at exactly 16 px/frame.**

**What each core did under held Right (identical injected input):**
- **Input reaches the game on BOTH cores.** `Ctrl_1_Held ($FF802C) = 0x08` (the Right bit) — read live on
  Oracle mid-hold, and present in our `.ram.bin` dump. Input injection is NOT the issue (this retires the
  earlier "press doesn't inject" hypothesis for good — the pad reaches the game variable on both sides).
- **oracle-next (motion_run, `set_pad` Right every frame):** `SST_x_pos` 1648 px (f120) → 10928 px (f700) =
  9280 px / 580 frames = **exactly 16 px/frame**, and the whole trajectory fits `x = 256 + (f−33)×16` (a ~33
  frame boot/settle before free-flight input takes effect) to the pixel at both f120 and f700 — an **exact
  match to the source arithmetic**. Camera follows: `Camera_X` `0x0060`→`0x16C0`. Render visibly deep-scrolled.
- **Oracle (`press right 700` AND free-run `hold right`):** `SST_x_pos` frozen at 256, `Camera_X` `0x0060`,
  `x_vel` 0, player `(256,256)` — **fails to advance x_pos despite `Ctrl_1_Held=0x08`.** Byte-identical regs
  between press-120 and press-700 (a true fixed point: nothing moves).

**Why our core is provably the correct one:** the per-frame delta is exactly `DEBUG_FLY_SPEED` (16) and the
absolute trajectory matches `add.l #(16<<16),SST_x_pos` executed every frame — this is not "plausibly moving,"
it is arithmetically the source. Oracle leaving x_pos at 256 with the same input in RAM is inconsistent with
the (trivial, branchless-after-btst) game code.

**Static A/B still holds on the new ROM (bonus):** our **no-input** f700 vs Oracle's `press right 700` (which
is inert = no-input) match to **124 px / 71,680**, all inside bbox `x[206..233] y[5..19]` — the pinned
ring-animation region (`x[204..235] y[5..20]`), pure sub-frame ring phase (offset-searched 694–706: residual
oscillates 124↔227 px by ring phase, localized to that box only). Modulo the P8 DAC ramp (raw 73,553 → ramp
normalized) and ring phase, the settled scene is pixel-identical. The static done bar is intact on `db0eb0`.

**Why the earlier overseer verify passed but missed this:** the M-1 live check verified `3× press right 120 →
byte-identical` and read that as resolved. But `Camera_X 0x0060` after press-120 is the **un-scrolled** value —
the motion was already absent; deterministic-but-inert is still 3×-identical. Determinism was fixed; the
input-driven-motion path was never actually asserted to *move* the game. Lesson for the acceptance bar below:
assert **motion happens and matches our trajectory**, not merely that repeated presses agree.

### Punch-list item M-2 (Oracle-side) — OPEN
| # | Rung | Symptom | First divergence | Hypothesis | Owner | Status |
|---|---|---|---|---|---|---|
| M-2 | motion | Held Right does not advance `SST_x_pos` on Oracle though `Ctrl_1_Held=0x08` reaches the game; our core advances it exactly 16 px/frame (source-correct) | `SST_x_pos` stays 256 vs our 256→10928 over 700 frames | **Two compounding Oracle-C++ bugs (root-caused by the Oracle session):** (1) a paused reset left a deferred `FlagInitialize` that `press`'s first `ExecuteSystemStep` consumed → the controller `Initialize()` wiped the just-injected `_buttonPressed[]` (`main_gui.cpp doFullReset`); (2) `MDControl6::AssertCurrentOutputLineState` drove only *high* lines, so a button held-from-power-on never re-deltaed and its grounded line stayed stale-high (`0x7F` = not-pressed). Both required fixing (each verified necessary by reverting). NOT oracle-next. | Oracle C++ session | **RESOLVED + overseer-verified live 2026-07-22** |

**Acceptance bar for the Oracle fix (unblocks the frame-exact A/B payoff):** `reset → press right N` (N≥~400)
must **move** the game — specifically `SST_x_pos` (and `Camera_X`) must advance, and match our core's
reference trajectory `x_pos = 256 + (frame−33)×16` (debug free-flight, 16 px/frame) / `Camera_X 0x0060→0x16C0`
at frame ~700. "3× identical" alone is INSUFFICIENT — it must be 3× identical **and non-trivially moved to the
reference value**. Our-side reference dumps for the diff live in session scratch (`ours.f*.{ppm,ram.bin}`,
`ni.f*` no-input neighbors, `oracle-ref-n700.ppm`).

### M-2 RESOLVED + MOTION A/B PIXEL-IDENTICAL — overseer live verify 2026-07-22
Oracle rebuilt (`oracle/linux-port/build/oracle_gui`, both fixes above), overseer relaunched it with
`ORACLE_DETERMINISTIC=1` and drove the acceptance bar live over MCP on the byte-identical `db0eb0` ROM:
- **Moves to the reference (M-2 fixed):** `reset → press right 700` (NO settle) → `Camera_X 0x0060→0x16C0`
  (exact match) and player `x 256 → 10912` = `256+(700−34)×16` — our core's reference is `256+(700−33)×16 =
  10928`, i.e. **within one frame / 16 px of deferred-reset offset, identical 16 px/frame slope**. Pre-fix this
  was frozen at `x=256` / `Camera_X 0x0060`.
- **Determinism intact (M-1 not regressed):** the run repeated byte-identical (`Camera_X 0x16C0`, `x 10912`).
  The acceptance bar is met in full — **deterministic AND non-trivially moved to the reference**, the exact
  gap the old "3×-identical" M-1 check couldn't see.
- **Frame-exact motion pixel A/B — THE PAYOFF, DONE:** Oracle's moved press-700 screenshot vs our core's
  held-Right frames 697–701 through the committed `ab_compare` = **0 / 71,680 differing pixels** (ramp
  normalized) at every offset. Camera settled at `0x16C0` both sides, the free-flight box is off-screen right,
  and the rings have scrolled out of view — so the scrolled scene renders **pixel-identical** with no
  animation-phase residual. Both emulators, identical ROM, identical held input, mid-scroll: bit-for-bit the
  same picture.

**The s4-boot milestone's motion extension is COMPLETE.** Static bar (met 2026-07-21, re-held on `db0eb0`) +
motion bar (input-driven scroll pixel-identical, 0/71,680) both green; the one divergence the whole-machine
A/B surfaced (M-2) was Oracle-side, fixed, and verified. oracle-next validated end-to-end on a real game
under real input.

**oracle-next status: unchanged and correct.** No oracle-next `src/` code touched this session — the A/B
validated our core, it did not implicate it. `motion_run` + a scratch ramp-normalized PPM comparator did the
live job; the **committed `examples/ab_compare.rs` then landed and was overseer-accepted** — it reproduces the
scratch comparator's exact metric (best offset 124 px / 71,680, first divergence (210,5) in the ring region;
raw ~73,553 from the P8 ramp), derives `ours_ramp` from the pinned truncated `intensity(Normal)` in
`src/render.rs` (`[0,36,72,109,145,182,218,255]`, unit-test pinned), carries 7/7 unit tests, and leaves
`src/`+`m68000/` zero-diff (Cargo.toml `[[example]] test=true` opt-in only). Staged-ready in the working tree
(uncommitted, held separate from this doc edit per the two-commit structure).
