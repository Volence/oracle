# The corpus A/B — our profiler against Aeon's Phase-0 parity measurements (2026-08-20)

Slice 7 of the profiler arc (`docs/2026-08-19-profiler-recon.md:973`, protocol at `:981`). **Not
code: this file is the acceptance artifact.** Every number below came off a run executed for this
document; nothing is reconstructed, carried from a previous session, or averaged out of two sources.
Where a figure is theirs it is quoted with its file and section; where it is ours it carries the ROM
CRC and the boot spread.

The governing rules are the controller's (`docs/2026-08-19-ruling-profiler-recon.md`, "Acceptance
protocol"), K.1–K.4 of the recon, and — decisively for the *cycle* comparison — §2.4 of
`docs/2026-08-19-streaming-asks-recon.md`, which forbids comparing the inclusive figure because our
two mechanism differences from the reference have opposite signs and can cancel into a false
agreement.

---

## 0. Verdict, one page

| Rule (ruling, "Acceptance protocol") | Result |
|---|---|
| **Our spread is exactly 0 across three boots** | **PASS.** Three independent boots per state (a fresh server process each, not a reset): the `emulator/get_profiler_frames` replies are **byte-identical**, including the `perFrame[]` ring and the reply envelope's `frame`/`mclk`. 9 boots, 3 states, zero differing bytes. |
| **`calls` matches exactly** | **PASS, 6 cells for 6**, on every row where the corpus publishes a count that is not W16's fabricated floor: `HBlank_Vector_Slot` (4.000 vs 4), `Raster_VBlank` (1.000 vs 1) and `Enqueue_Dirty_Buffers` (1.000 vs 1), at both camera states. **One disagreement, at the dense state: theirs 100, ours 101** — and 101 is *their own model's* derived count (`lines + 5`), on the row they themselves booked as an anomaly. See §5.2 and §7. |
| **`cycles` exact on stall-free paths** | **PASS, to the cycle.** The corpus's own designated "PHASE-0 REFERENCE ROW" — the HBlank trampoline `$FFB452` — reads **1878 cyc/video-frame on ours and 1878 on theirs, at *both* camera states**, with our `stallCycles` = **0**. At the dense state ours reads **33000**, which is Aeon's shipped `.emp` model exactly (`1512 + 328 × 96`). |
| **Stall rows reconcile through `stallCycles`, or the arc stops** | **PARTIAL, reported not smoothed.** The only stall-bearing rows in the corpus set are the VBlank bracket. `VInt_Level` does **not** reconcile by subtraction alone (§6.3); the residual has a named mechanism (W4/W7 — their frame seam *is* the V-INT, so their VBlank rows are measured across it by construction) but is not pinned to the cycle. Recorded as an open item, not as a tolerance. |
| **`interrupts.hint` must DISAGREE by the falsifiable equation** | **PASS, itemised to the cycle.** §8: our `hint + vint` = 11176 cyc/frame at idle against their two published component rows' 10324; the whole 852-cycle gap decomposes into three named terms, one of them (the HInt exception entry, **176 = 44 × 4 fires**) exact. |
| **K.3 — the measured claim** | **Delivered, and larger than the design argument predicted.** Their own five published top-level rows tile **97.69%** of the idle frame and **78.45%** of the max-diagonal frame. Ours tile 99.24% / 99.33% of the same five spans, and our reconciliation identity closes at **exactly 0 remainder** at all three states. **21.55 points of the max-diagonal frame are unattributed on the old instrument** — the 20.6%-class witness, measured on the corpus. §9. |

**Bonus anomalies.** The dense one-fire / 242-cycle gap **does not reproduce** — our instrument reads
the model's 101 fires / 33000 cycles, so the anomaly was instrument-side, not program-side (§7). The
walker's two-regime anchor was **not re-driven** (their 17-fixture installer is out of this slice's
scope) — stated as a limit, with the out-of-sample leg that *was* measurable reported in §10.

**Nothing STOPPED.** No STOP rule fired: the spread gate passed, the ROM CRC matched before a single
measurement, and every state witness the corpus publishes was reproduced. Two BLOCKED-class incidents
in ROM recovery were resolved by pinning the toolchain rather than by working around it (§1.3).

**One thing is unexplained and is labelled that way.** Five *ungated* rows disagree by 11–40% with a
W4 exposure under 4%, and neither candidate mechanism survived testing (§6.3, §11.5). It is outside
the gate set by the governing documents' own comparability rules, so it does not stop the arc — but
it is not written up as a tolerance either.

---

## 1. Provenance

### 1.1 The two instruments

| | revision | note |
|---|---|---|
| **Ours** (`oracle`, Rust) | branch `profiler-corpus-ab`, cut from **`018612a`** ("Merge profiler-closeout — the C1 witness and the fourth surface") | binary `target/release/oracle-aether` built from that tree; `emulator/set_profiler` / `get_profiler` / `get_profiler_frames` per §11.16 / CR-26 |
| **Theirs** (`oracle-old`, C++ Exodus port) | not run here | every "theirs" figure is quoted from the corpus documents, which record it |
| **The corpus** (`aeon`) | **`bc048e2a`** — "Merge measure/scanline-p2-phase0 — P2 Phase 0: every budget denominator measured" | read via `git show bc048e2a:<path>`; the live aeon tree has moved past it and was never checked out |

Corpus documents read in full at that revision:
`docs/benchmarks/scanline-p2/INSTRUMENT-PARITY.md`, `ENGINE-BASELINE.md`, `WALKER-MODEL.md`.

### 1.2 The ROM — and a correction to the dispatch brief

**Every row in this document was measured on one image:**

| | value |
|---|---|
| file | `s4.debug.bin` (sonic4, DEBUG shape) |
| **CRC32** | **`d22dda85`** |
| length | **713295** bytes |
| corpus pin | `INSTRUMENT-PARITY.md:5-6`, `ENGINE-BASELINE.md:6-7`, `WALKER-MODEL.md:6` — *"crc `d22dda85`, len 713295"* |
| listing | `s4.debug.lst`, same build, 5162 lines, **2578 symbols** as counted by `emulator/status` |
| binding | the server's own listing↔ROM binding check passed (`initialize` → `capabilities.symbolsLoaded: true`); a listing from a different build is refused, not silently accepted |

> **The brief's CRC was wrong and this is the correction.** The dispatch brief pinned `06af0010`.
> That value is real but is a *different* ROM: it is the **pre-parcel identity from the streaming
> diagnosis packet** at aeon `3469c920` — *"`crc=06af0010` (debug, **713863 B**)"*
> (`docs/2026-08-19-aeon-streaming-demand.md:236-237`), i.e. the image their *fix ladder* starts
> from, ~568 bytes larger than the corpus's. The corpus pins `d22dda85`/713295 in all three of its
> files, and the corpus is the authority on which ROM its own measurements were taken on. This
> document uses `d22dda85`. Building to `06af0010` would have produced a table that still looked
> fine and compared nothing.

### 1.3 How the ROM was recovered (and the two things that went wrong)

1. `git -C aeon worktree add <scratch>/aeon-corpus bc048e2a` — a throwaway worktree, never their
   checkout, **removed at the end of this run**.
2. First build attempt with the *current* sigil binaries (`sigil af2a4429`) **failed twice**:
   - the lint stage's pytest suite died with `ModuleNotFoundError: No module named 'launcher'`, because
     their tools hardcode `HARNESS = "/home/volence/sonic_hacks/oracle/linux-port/harness"` and that
     path is now the Rust repo (the old C++ reference moved to `oracle-old`). **Not a ROM problem** —
     the lint stage touches no emitted byte — so the build was re-run with `--no-lint`.
   - the native build then refused: `[map.undeclared-island] ROM section at 0x99F0 …`. That is a real
     toolchain-version mismatch: sigil has moved past what aeon `bc048e2a` declares.
3. **Resolved by pinning the toolchain, not by working around it.** Sigil's own history names the
   pairing: `7b46f075` — *"refreeze: raster-cram-anchor-366 … PAIRED with aeon d2974510 … s4.debug
   b7960905 -> **d22dda85**"*, i.e. the corpus's exact ROM identity, committed on the sigil side. A
   throwaway sigil worktree at `7b46f075`, built to a scratch `CARGO_TARGET_DIR`, produced
   `sigil` + `emit_sound_blob`, and:

```
SIGIL_BUILD=… SIGIL_EMIT=… DEBUG=1 ./build.sh sonic4 --no-lint
  → built: sonic4 debug native ROM — crc=d22dda85 len=713295
```

4. **Verified twice, independently.** `zlib.crc32` over the built file = `d22dda85`, length 713295;
   and the built file is **byte-identical** to sigil's committed golden blob
   `crates/sigil-harness/golden/s4.debug.bin` at `7b46f075` (713295 bytes, same CRC). Two provenance
   chains, one image.

---

## 2. The reproduction rituals, transcribed so they re-run

All three states are reached through the Aether socket with a hand-rolled NDJSON JSON-RPC 2.0 client
(the `tools/aether_smoke.py` pattern; **no `mcp__oracle__*` tooling was used at any point**). One
**server process per boot** — an independent boot is a new machine, not a `reset` on an old one.

Server: `oracle-aether <rom> --socket <path> --no-pace`; the listing is auto-bound from
`<rom>.lst` beside the ROM.

Constants transcribed from `aeon/tools/engine_baseline_probe.py:106-116` at `bc048e2a`:
`SST_X_POS = 0x02`, `SST_Y_POS = 0x06` (both 16.16 Coord), `DIAG_AHEAD_X = 2000`,
`DIAG_AHEAD_Y = 1400`, `CAM_MAX_STEP = 16`.

### 2.1 Common preamble and sample

```
initialize {clientId, protocolVersion:1, clientCapabilities:{events:true}} ; initialized
emulator/status                                   -> romBytes 713295, symbolCount 2578
emulator/reset
emulator/run_frames {frames:180}                  -- SETTLE
<per-state ritual, below>
<read the state witnesses: Camera_X/Y, Camera_Art_Hold, Frame_Counter, Logic_Tick,
 Lag_Frame_Count, Dbg_Cam_Clamp_Frames, Raster_Active_Buf + 96 bytes of live program>
emulator/set_profiler      {enabled:true, perFrame:true}
emulator/run_frames        {frames:32}
emulator/get_profiler      {}
emulator/get_profiler_frames {frames:32, top:512}
emulator/set_profiler      {enabled:false}
<re-read every state witness, plus Raster_Dense_Lines>
```

> **Why `run_frames: 32` for a 31-frame sample.** Our sample is delimited by frame boundaries at
> *both* ends, so `run_frames(N)` yields `frameCount == N − 1`. Running 32 gives
> **`frameCount == 31`, the corpus's own window length, exactly** — verified in every one of the 9
> boots. This is the honest counterpart of the legacy `frames: sample − 1` folklore the arc deleted
> (§F.3): we do not ask for one frame fewer, we run one frame more, and the instrument reports the
> number of complete frames it actually measured.

### 2.2 `idle` (`ENGINE-BASELINE.md` §1)

No poke at all. Settle 180, sample.

### 2.3 `maxdiag` (`ENGINE-BASELINE.md` §1)

```
tgt    = read_memory(Camera_Target, 2)              -- a SHORT pointer into work RAM
leader = 0xFF0000 | tgt   if tgt & 0x8000 else tgt  -- resolved to 0xFF8DB0 on this ROM
cam_x  = read_memory(Camera_X, 4) >> 16 ; cam_y = read_memory(Camera_Y, 4) >> 16
write_memory {addr: leader+0x02, value: (cam_x + 2000) << 16, width: 4}
write_memory {addr: leader+0x06, value: (cam_y + 1400) << 16, width: 4}
run_frames   {frames: 24}                           -- LEAD, reach the steady follow ceiling
```

### 2.4 `dense` (`ENGINE-BASELINE.md` §4 "the dense tier"; state from `tools/scenes/effects_raster_dense.json`)

```
write_memory {symbol Debug_Scene_Freeze, value:1,    width:1}
write_memory {symbol Camera_Y,           value:144,  width:2}
write_memory {symbol Camera_X,           value:4960, width:2}
run_frames   {frames: 12}
```

### 2.5 The state witnesses — every one of the corpus's own reproducibility claims, re-checked

| witness | corpus says | ours (all 3 boots) | ✓ |
|---|---|---|---|
| idle camera | `Camera_X` 96, `Camera_Y` 144, stationary | 96 / 144, unchanged start→end | ✓ |
| maxdiag camera | **(320,368) → (560,608)** across the sample | **(320,368) → (560,608)** | ✓ exact |
| idle logic ticks | 30 ticks | **30** | ✓ |
| maxdiag logic ticks | 15 ticks, `Logic_Tick` and `camera_dx/16` agreeing | **15**, `dx` = 240 px = 15 × 16 | ✓ exact |
| `Camera_Art_Hold` | 0 throughout, both states | 0 / 0, both states | ✓ |
| `Dbg_Cam_Clamp_Frames` | 0 for the entire run | 0 → 0, all states | ✓ |
| lag | idle +1, maxdiag +16 (over 31 frames) | idle **+2**, maxdiag **+17** (over 32 frames) | consistent |
| raster program stable within the sample | asserted start and end | pre == post, all 3 states, all 3 boots | ✓ |
| idle live program wire image | decoded in `ENGINE-BASELINE.md` §4 | **byte-identical** (below) | ✓ |

The idle program read off `Raster_Active_Buf` (= `0xFFFF8A22`) is exactly the image they decoded:

```
0004  8A4D 0000  8A8D 0000  8AFF 0002  0000 8C89  0004 C0480000 000B 0002 0048
      8AFF 0001  0002 40020010 0016 0000 0043     8AFF FFFF
```

> **One corpus claim our reading contradicts, stated plainly.** `ENGINE-BASELINE.md` §1 says the
> max-diagonal raster program is *"the same OJZ section-0 sparse preset, 4 records, **byte-identical
> to idle**"*. It is not byte-identical on this ROM: the two **priming** records' arm words read
> `8A4D`/`8A8D` at idle and **`8A00`/`8ADA`** at max-diagonal — they track the camera, as
> position-clamped records should. Every *costed* op word is identical, which is why the HInt total
> is identical (1878) at both states and why their conclusion is unaffected. The within-state
> stability they assert (start == end, all boots) reproduces exactly.

---

## 3. The spread gate — exactly 0, and it is a gate

`Slice 4`'s three-boot bar, applied to real content. Three independent boots per state, each a fresh
server process on the same ROM.

| state | ROM CRC | boots | `get_profiler_frames` replies byte-identical | envelope `frame` / `mclk` |
|---|---|---|---|---|
| idle | `d22dda85` | 3 | **yes** | 212 / 189960526, all three |
| maxdiag | `d22dda85` | 3 | **yes** | 236 / 211465457, all three |
| dense | `d22dda85` | 3 | **yes** | 224 / 200713044, all three |

Not "spread 0 on the rows we looked at" — **spread 0 on the serialized reply**, routine rows,
interrupt buckets, the 31-row `perFrame[]` ring and the envelope included. Their bar is five boots
with a *reported* spread because the old instrument has a noise floor; ours is 0 by construction and
is checked, not assumed.

**A second determinism witness, free:** `sampleCycles` is **3968178** in all nine boots across all
three states — 31 frames of machine time is the same 31 frames of machine time whatever the game is
doing in them.

---

## 4. What reconciles to what, and why

Stated precisely because it is the part a careful reader will attack.

| their column | our field | why |
|---|---|---|
| per-routine `cycles` (cyc/video-frame) | `routines[].cyclesTotal / frameCount` | theirs is **inclusive of callees** (`WALKER-MODEL.md` §1 verifies this on this ROM) and divided by the requested window; ours is the undivided partner divided by the number of frames actually measured. We divide ourselves rather than read the truncated `cycles` key, because their division truncates (W13/W14) and ours would too |
| their cyc/**logic-tick** column | `cyclesTotal / callsTotal` | for a once-per-tick routine these are the same quantity: **cost per invocation**. This is the lag-independent unit and the only one that survives the two machines ticking at slightly different rates |
| per-routine `calls` | `callsTotal / frameCount` | ours is a real count; theirs is `max(1, total/frames)` — floored, then **fabricated up to 1** at `ControlSocket.cpp:2043` (W16). So their `calls: 1` on a once-per-tick routine carries no information and is not a comparison |
| `interrupts.hint` (conflated) | `interrupts.hint.cyclesTotal + interrupts.vint.cyclesTotal` | K.1's falsifiable equation; theirs sums both causes (W8) |
| — (no counterpart) | `cyclesSelf*`, `stallCycles*` | K.1: *"New information; report, do not compare"* |

**Caveat 0 and what it does to the arithmetic.** Every corpus cycle figure is an **ideal cycle**
count by construction: their clock adds only `cyclesExecuted` to `_currentCycle`
(`M68000.cpp:1029-1031`) and stall lands in `_currentTime`. Ours is stall-inclusive. **So the
reconciliation is OUR `(cyclesTotal − stallCyclesTotal)` against THEIR `cycles`** — and our
`stallCycles` is printed beside every comparison below, never absorbed into one.

**And the inclusive figure is not the compared quantity, on the streaming recon's §2.4 ground.** Our
inclusive `cycles` deliberately excludes preemption (a routine's row does not grow when a VBlank
interrupts it) while theirs folds it in (W5, `ControlSocket.cpp:1981-1982`); and theirs *drops* the
post-boundary segment of a straddling invocation (W4, `:1972` + the flush at `:2007-2022`) while
ours does not. **Two mechanisms, opposite signs.** An inclusive row that agrees is therefore not
evidence, and one that disagrees is not attributable to either mechanism without more. This is why
§5 gates on the rows where both mechanisms are structurally absent, and §6 *reports* the rest
without gating them.

---

## 5. The gated rows — where both mechanisms are absent

### 5.1 The corpus's own PHASE-0 REFERENCE ROW: the HBlank trampoline `$FFB452`

`INSTRUMENT-PARITY.md` §"THE PHASE-0 REFERENCE ROW" and `ENGINE-BASELINE.md` §4 both key on this
address; it is the one row the corpus designates as the thing later tasks cite. It is also the one
row in the set with **no** preemption exposure (an HInt fires and returns inside a display line),
**no** frame-boundary straddle, and **zero** stall on our clock.

| state | ROM CRC | their fires | our `callsTotal`/frame | their cyc/frame | **our cyc/frame** | our `stallCycles`/frame | delta | our spread |
|---|---|---|---|---|---|---|---|---|
| idle | `d22dda85` | 4 | **4.000** (124/31) | **1878** | **1878.0** | 0 | **0** | **0** |
| maxdiag | `d22dda85` | 4 | **4.000** (124/31) | **1878** | **1878.0** | 0 | **0** | **0** |
| dense | `d22dda85` | 100 | **101.000** (3131/31) | 32758 | **33000.0** | 0 | +242 / +1 fire | **0** |

At both camera states this is an **exact match, to the cycle, on both the cycle figure and the call
count**, with our stall term identically zero — the ruling's *"`cycles` matches exactly on stall-free
paths"* discharged on the sharpest row available. The dense row is §7.

**And the same row cross-checks against Aeon's shipped cost model, not just against their
instrument.** `ENGINE-BASELINE.md` §4 sums the live records at 294 + 294 + 666 + 624 = **1878**; we
read 1878. At the dense state their model is `1512 + 328 × lines` with `lines + 5` fires, which at
96 lines is **101 fires / 33000 cycles**; we read **101 / 33000.0**. Our instrument reproduces the
`.emp` model to the cycle at 4 fires/frame and at 101 fires/frame.

### 5.2 The `calls` exactness table — pass/fail, no tolerance column

A cell is comparable only where their published count is a real quotient rather than W16's floor —
i.e. where the routine runs a whole number of times per **video frame**. Three rows qualify.

| row | state | theirs | ours (`callsTotal`/31) | verdict |
|---|---|---:|---:|---|
| `HBlank_Vector_Slot` | idle | 4 | **4.000** (124) | **PASS** |
| `HBlank_Vector_Slot` | maxdiag | 4 | **4.000** (124) | **PASS** |
| `Raster_VBlank` | idle | 1 | **1.000** (31) | **PASS** |
| `Raster_VBlank` | maxdiag | 1 | **1.000** (31) | **PASS** |
| `Enqueue_Dirty_Buffers` | idle | 1 | **1.000** (31) | **PASS** |
| `Enqueue_Dirty_Buffers` | maxdiag | 1 | **1.000** (31) | **PASS** |
| `HBlank_Vector_Slot` | dense | 100 | **101.000** (3131) | **FAIL vs their measurement; matches their model** (§7) |
| the ten once-per-**tick** rows | all | `1` | 0.935 (29/31), 0.452 (14/31), 0.484 (15/31) | **not comparable** — their `1` is W16's fabricated floor, not a count |

**Six exact cells, one disagreement, and the disagreement is the row they booked as an anomaly.**

That last line is not a dodge, it is the finding: on a once-per-logic-tick routine sampled over 31
video frames the honest count is 29 (idle) or 14–15 (max-diagonal), the old instrument's arithmetic
produces `max(1, floor(29/31)) = 1` (`ControlSocket.cpp:2042-2043`), and **a client cannot tell that
1 from a real one**. Ours emits `callsTotal` beside `calls` precisely so the divided figure can never
be the only number in the room.

---

## 6. The ungated rows — reported, with signs and mechanisms

Per **invocation** (`cyclesTotal / callsTotal` on ours; their published cyc/logic-tick column), which
is the lag-independent unit. ROM `d22dda85`, our spread 0 on every cell.

### 6.1 idle — 30 logic ticks, our `callsTotal` 29 over 31 frames

| routine | their cyc/tick | our cyc/call | our stall/call | our ideal/call | delta (ours − theirs) | sign |
|---|---:|---:|---:|---:|---:|:--:|
| `Parallax_Update` | 20161 | **20196.0** | 0 | 20196.0 | **+35 (+0.17%)** | + |
| `EntityWindow_Scan` | 1856 | 1854.0 | 0 | 1854.0 | −2 (−0.11%) | − |
| `Raster_VBlank` | 1531 | 1521.8 | 0 | 1521.8 | −9 (−0.60%) | − |
| `Enqueue_Dirty_Buffers` | 1401 | 1416.6 | 0 | 1416.6 | +16 (+1.1%) | + |
| `VSync_Wait` | 82248 | 84301.4 | 0 | 84301.4 | +2053 (+2.5%) | + |
| `Camera_Update` | 584 | 598.0 | 0 | 598.0 | +14 (+2.4%) | + |
| `VInt_Level` | 8556 | 8780.8 | 2266.7 | **6514.1** | +225 raw / **−2042 ideal** | ± |
| `Section_UpdateColumns` | 875 | 978.0 | 0 | 978.0 | +103 (+12%) | + |
| `GameState_OJZScroll_Update` | 36296 | 40175.6 | 0 | 40175.6 | +3880 (+11%) | + |
| `Palette_Compose` | 150 | 180.0 | 0 | 180.0 | +30 (+20%) | + |
| `BgAnim_Update` | 187 | 154.0 | 0 | 154.0 | −33 (−18%) | − |
| `Tile_Cache_Fill` | 4783 | 7952.5 | 0 | 7952.5 | +3170 (+66%) | + |
| `VInt_Lag` | 172 | 7520.0 | 2222.0 | 5298.0 | not the same quantity — see §6.3 | — |

### 6.2 max-diagonal — 15 logic ticks, 2.13 video frames per tick

| routine | their cyc/tick | our cyc/call | our stall/call | our ideal/call | delta |
|---|---:|---:|---:|---:|---:|
| `BgAnim_Update` | 153 | 154.0 | 0 | 154.0 | **+1 (+0.7%)** |
| `Palette_Compose` | 138 | 180.0 | 0 | 180.0 | +42 (+30%) |
| `Camera_Update` | 659 | 674.0 | 0 | 674.0 | +15 (+2.3%) |
| `Parallax_Update` | 25191 | 26269.6 | 0 | 26269.6 | +1079 (+4.3%) |
| `VSync_Wait` | 71033 | 67550.1 | 0 | 67550.1 | −3483 (−4.9%) |
| `EntityWindow_Scan` | 1970 | 2326.0 | 0 | 2326.0 | +356 (+18%) |
| `Tile_Cache_Fill` | 106163 | 125928.7 | 0 | 125928.7 | **+19766 (+18.6%)** |
| `Section_UpdateColumns` | 7483 | 9499.2 | 0 | 9499.2 | +2016 (+27%) |
| `VInt_Level` | 12642 | 13484.0 | 2220.0 | 11264.0 | +842 raw / −1378 ideal |
| `GameState_OJZScroll_Update` | 113964 | 182033.3 | 0 | 182033.3 | **+68069 (+60%)** |

### 6.3 Reading these honestly

**These rows are not gate cells and this document does not treat them as such.** By K.1 our
`cyclesSelf` has no counterpart at all, and by streaming-recon §2.4 the inclusive figure must not be
the compared quantity. But "not gated" is not "not examined", so the deltas are sorted below into
what the enumerated mechanisms cover and what they do not.

**The straddle model, with numbers.** W4 (`ControlSocket.cpp:1972` + the flush at `:2007-2022`) is
the dominant candidate: their shadow stack is re-initialised empty at every frame boundary, a
straddling call is closed with a truncated duration, and *"its real RTS in the next frame pops an
unrelated entry"*. For an invocation of length `L` cycles the exposure is bounded: it straddles with
probability ≈ `L / 128000` and loses on average half of itself when it does, so the expected relative
loss is ≈ `L / 256000`. That is a *ceiling*, and it is checkable per row:

Percentages in this table are relative to **ours** (`(ours − theirs) / ours`), so they read directly
as "how much of the invocation their instrument did not see".

| routine (state) | our `L` (cyc/call) | W4 ceiling | observed | covered? |
|---|---:|---:|---:|:--:|
| `GameState_OJZScroll_Update` (maxdiag) | 182033 | 71% (spans 1.42 frames) | theirs 37% low | **yes** |
| `Tile_Cache_Fill` (maxdiag) | 125929 | 49% | theirs 16% low | **yes** |
| `VSync_Wait` (idle) | 84301 | 33% | theirs 2.4% low | **yes** |
| `GameState_OJZScroll_Update` (idle) | 40176 | 16% | theirs 9.7% low | **yes** |
| `Parallax_Update` (maxdiag) | 26270 | 10% | theirs 4.1% low | **yes** |
| `Tile_Cache_Fill` (**idle**) | 7952 | **3.1%** | theirs **40% low** | **NO** |
| `EntityWindow_Scan` (maxdiag) | 2326 | 0.9% | theirs 15% low | **NO** |
| `Section_UpdateColumns` (idle) | 978 | 0.4% | theirs 11% low | **NO** |
| `Palette_Compose` (idle) | 180 | 0.07% | theirs 17% low | **NO** |
| `BgAnim_Update` (idle) | 154 | 0.06% | theirs 21% **high** | **NO** |

**So the story splits cleanly in two, and only half of it is closed.**

- **The large deltas are covered, and their ordering is the mechanism's own signature.** Every row
  whose invocation approaches or exceeds a frame reads lower on their instrument, by an amount inside
  the W4 ceiling, and the effect grows monotonically with `L`. `GameState_OJZScroll_Update` at
  max-diagonal — 182,033 cycles, 1.42 frame-lengths — reads **60% higher on ours**. That is the whole
  point of a sample-lifetime stack, measured on shipped content.
- **A residual class on SHORT routines is NOT covered and is registered as a finding, not smoothed**
  (§11.5). Five rows in the table above disagree by 11–40% while their W4 exposure is under 4%. Two
  candidate mechanisms were examined and neither explains it:
  - **W2 (JMP/tail calls never hooked).** Checked directly against the ROM: `JMP (xxx).L` (`4EF9`)
    sites exist inside `Tile_Cache_Fill`, `BgAnim_Update`, `EntityWindow_Scan`, `Camera_Update` and
    `VSync_Wait`, and are **absent** from `Palette_Compose`, `Section_UpdateColumns`, `Raster_VBlank`
    and `Parallax_Update`. That partition does **not** track the delta — `Palette_Compose` has no
    `JMP` and disagrees by 17%; `Camera_Update` has one and agrees to 2.4%. **Rejected.**
  - **State phase.** A data-dependent routine could cost different amounts in two samples. Rejected
    for these rows on our side: `Palette_Compose` reads **exactly 180.0** and `BgAnim_Update`
    **exactly 154.0** cycles per invocation at *both* camera states, i.e. they are constants on our
    instrument, so a state-phase explanation would have to live entirely on theirs.
  - What would settle it is a **paired trace** — the same invocation, both instruments, event by
    event — which needs the reference running and is out of this slice's scope. Registered as such.
- **The one row where the two instruments essentially agree is the walker.** `Parallax_Update` at
  idle: theirs 20161, ours 20196 — **35 cycles in 20,000, 0.17%**, with our `stallCycles` = 0. That
  is what agreement looks like with no stall term and low straddle exposure, and it is the best
  evidence here that our nominal 68000 timings and theirs are the same timings — which is what makes
  the §5.1 exact matches meaningful rather than lucky.
- **`VInt_Level` does not reconcile through `stallCycles`.** Subtracting our stall takes it *further*
  from theirs (idle: 8781 raw → 6514 ideal against their 8556), which is the wrong direction for
  caveat 0 alone to explain. The named mechanism is **W7 + W4 together**: their frame is
  *first-event → V-INT*, not V-INT → V-INT (`main_gui.cpp:2008`), so the VBlank bracket is the
  routine their frame seam runs through **by construction**, and W5 nests interrupts on the
  subroutine stack. Recorded as an open item (§11.1), **not** widened into a bound.
- **`VInt_Lag`'s rows are not the same quantity on the two sides** and the table says so: their 172
  cyc/tick is a per-frame average over a state that lags once in 31 frames; ours is a real
  per-invocation cost over 2 invocations. Comparing them would be comparing an average to a cost.

**One confound was tested and eliminated.** The idle state might have differed between the two
machines through settle history (a streaming backlog would inflate `Tile_Cache_Fill`). It does not:
re-running idle at **settle 300** instead of 180 produces **byte-identical rows** — same
`callsTotal`, same `cyclesTotal`, same per-call figures on all 14 routines. The idle state is a
genuine fixed point on our machine, so nothing in §6.1 is settle drift.

---

## 7. Bonus anomaly 1 — the dense one-fire / 242-cycle gap: **does not reproduce**

Their booked finding (`ENGINE-BASELINE.md` §4 "The dense tier — measured, and its model DISAGREES"):
the shipped `OJZ_TestGradient` program measured **100 fires / 32758 cyc/frame** where the model and a
byte-identical *poked* fixture both give **101 / 33000**; booked as
`hint_total_dense_model_gap = -242`, `"measured, model DISAGREES"`, **UNEXPLAINED**. Three hypotheses
(sample-window artifact, `Effects_Offscreen_Entry`, `Raster_Patch_Tab`) were tested and refused.

**Our reading of the same shipped program, on the same ROM, at their state:**

| | fires | cyc/video-frame | stall | spread (3 boots) | ROM CRC |
|---|---:|---:|---:|---:|---|
| theirs (shipped program) | 100 | 32758 | n/a | not stated for this row | `d22dda85` |
| their model / their poked fixture | 101 | 33000 | n/a | — | `d22dda85` |
| **ours (shipped program)** | **101** | **33000.0** | **0** | **0** | `d22dda85` |

**The anomaly is instrument-side, not program-side.** The gap they could not close is the difference
between the shipped program and a byte-identical poked one *as their instrument sees them*; our
instrument sees no difference, because it reads the model's value for both. Two secondary symptoms
also fail to reproduce:

- `Raster_Dense_Lines` reads **0** at the frame boundary on ours — the value their document says is
  *expected* — where theirs read 1 and was taken as evidence the run finished a line short. It did
  not finish a line short.
- The 242 cycles are not a whole fire (a dense fire is 328). **A partial loss is exactly W4's
  signature**: the flush closes a straddling invocation *with a truncated duration*, so what is lost
  is the pre-boundary fraction of one fire, not the fire. 242 of 328 is a fraction; 328 would have
  been a missing fire. Their own number was telling them which defect it was.

Named mechanism, from the enumerated list: **W4** (per-frame shadow-stack reset, `:1972`, and the
frame-end flush `:2007-2022`) compounded by **W7** (a "frame" is first-event → V-INT,
`main_gui.cpp:2008`). We do not claim to have pinned which of the two lines produced the missing pop
— our data cannot separate them — and that is stated rather than smoothed.

**Consequence for their toml, and it is a deletion not an edit:** `hint_total_dense_model_gap = -242`
and `hint_total_dense_status = "measured, model DISAGREES"` describe their instrument, not the
engine. On this instrument the model and the measurement agree exactly, and the row can be retaken
as `measured == model`.

---

## 8. The interrupt equation, evaluated

K.1's prediction is not a tolerance: **their `interrupts.hint` ≈ our `hint.cycles + vint.cycles`**,
because theirs sums both causes (W8: the classifier tests the handler *PC* against a *vector-table*
constant, so on any ROM whose handler is not at `$000078` the `else` fires and `vint` is structurally
0).

**The corpus deliberately never read `interrupts.hint` on this ROM** (`INSTRUMENT-PARITY.md`
caveat 1: *"It is NEVER a valid source"*), so the equation is evaluated against the sum of their two
published **component** rows, which is the best available and is labelled as such.

| term | idle, cyc/video-frame | ROM CRC | spread |
|---|---:|---|---:|
| theirs: `$FFB452` row + `VInt_Level` + `VInt_Lag` | 1878 + 8280 + 166 = **10324** | `d22dda85` | 0 (5 boots) |
| **ours: `interrupts.hint` + `interrupts.vint`** | 2054.0 + 9122.0 = **11176.0** | `d22dda85` | **0** |
| gap | **+852.0** | | |

**The gap itemises, and one of its three terms is exact:**

| term | value | what it is |
|---|---:|---|
| HInt exception entry | **+176.0** | our `hint` bucket's own `cyclesSelf` = **44 cycles × 4 fires**, exactly. A row keyed at the trampoline *symbol* structurally cannot see the cycles between the IACK and the first instruction of the handler; a bucket opened at the IACK can. |
| VInt exception entry + handler prologue/epilogue | **+422.6** | our `vint` bucket self (44.0) + `VBlank_Handler`'s own self (378.6). Their published rows name `VInt_Level` and `VInt_Lag`, which are *callees* of `VBlank_Handler`; the bracket around them is unnamed in their table. |
| the two VBlank rows themselves | **+253.5** | ours 8214.3 + 485.2 = 8699.5 against their 8280 + 166 = 8446 — the §6.3 W4/W7 residual |

Our side decomposes **exactly**, which is the check that the itemisation is not fitted:

```
vint.cycles 9122.0 = vint.cyclesSelf 44.0 + VBlank_Handler 9078.0
VBlank_Handler 9078.0 = self 378.6 + VInt_Level 8214.3 + VInt_Lag 485.2      (= 9078.1, rounding)
hint.cycles 2054.0 = hint.cyclesSelf 176.0 + HBlank_Vector_Slot 1878.0
```

**And the disagreement K.1 wanted is present in the strongest form.** Their `interrupts.vint` is
structurally **0** on this ROM. Ours reads **9122.0 cyc/frame at idle, 10781.9 at max-diagonal,
8673.3 at dense**, with `calls` exactly 1 per video frame at all three states. A non-zero `vint`
against their structural 0 is the expected result, not a discrepancy — and it is the number their
whole caveat-1 rule existed to work around.

**Corroboration from their own subtraction.** `ENGINE-BASELINE.md` §4 supersedes
`dense_run_cycles_per_frame` 41579 (a 2026-08-13 `interrupts.hint` reading) with 32758, and notes
41579 − 32758 = 8821 ≈ `VInt_Level` 8280 — *"precisely what `interrupts.hint` is in this ROM"*. Our
dense `hint + vint` = **46117.3**. The two are not comparable at the cycle (41579 was measured on the
pre-parcel ROM, nine byte-moving parcels earlier — a named confound, not a residual), but the shape
they inferred by subtraction is the shape our two buckets report directly.

**Q6, answered from a running machine.** The recon left open whether any corpus ROM nests HInt inside
VInt (`§C.3`, TAGged for the acceptance run). On this ROM, at all three states, it does not: `vint`
calls are exactly 1/frame and the `hint` bucket's cycles are wholly accounted for by
`HBlank_Vector_Slot` + the entry, with `depthExceeded` = 0 and `abandonedFrames` = 0 throughout. **No
nesting fixture is required by this corpus.**

---

## 9. K.3 — the measured claim

> *"If exact per-invocation attribution beats the old instrument's floor, this corpus is where that
> stops being a design argument and becomes a measured one."*

### 9.1 Our spread, by construction, against their measured floor

| | their instrument | ours |
|---|---|---|
| boots per state | 5 | 3 |
| reported spread | **0** on every row (`ENGINE-BASELINE.md` §3) | **0** — byte-identical replies (§3) |

Their 0 is a *measured* floor on a deterministic fixture, and their own documents say why it holds:
*"the 'exact to within 1 cycle' property is therefore a property of **their determinism**, not of the
instrument"* (recon §A.5) — truncation is one-sided and up to `numFrames − 1` low per row, invisible
only because every profiled frame is identical. Ours is 0 for a different reason: exact attribution
and a deterministic machine, checked on the serialized reply rather than on selected rows. **Same
number, different warrant** — and the warrant is what a client relies on the moment a frame stops
being identical to its neighbour.

Max-diagonal is exactly that case, and this is where the two warrants separate visibly: our
`perFrame[]` ring shows `vintCycles` taking two values, **13908 on exactly 15 of the 31 frames — the
15 logic ticks — and ~7852 on the other 16** (§12). Their instrument has no per-frame rows at all
(W29), so a sample whose frames differ by 77% reports one average and no way to see it.

### 9.2 The 20.6%-class witness, cross-referenced and measured on the corpus

The C1 witness (`crates/oracle-core/tests/profiler.rs`, landed at `f102072` on this branch's base)
asserts that a routine's own cycles do not move under preemption — *"exactly the property old oracle
violated by 20.6%"*. That was a fixture. Here is the same defect on shipped content.

Take the five **disjoint top-level spans** the corpus publishes — the HInt total, the main loop, the
VBlank bracket, the lag handler and the idle spin. Between them they are the whole CPU frame: there
is nowhere else for a cycle to be. Sum them against the 128000-cycle frame both sides use:

| state | ROM CRC | **their** five rows | % of frame | **our** same five rows | % of frame | **unattributed on theirs** | on ours |
|---|---|---:|---:|---:|---:|---:|---:|
| idle | `d22dda85` | 125044 | 97.69% | 127023.6 | 99.24% | **2.31 pts** | 0.76 pts |
| maxdiag | `d22dda85` | 100414 | **78.45%** | 127141.4 | **99.33%** | **21.55 pts** | 0.67 pts |

Row by row, so the sum is auditable:

| span | idle theirs | idle ours | maxdiag theirs | maxdiag ours |
|---|---:|---:|---:|---:|
| `HBlank_Vector_Slot` | 1878 | 1878.0 | 1878 | 1878.0 |
| `GameState_OJZScroll_Update` | 35125 | 37583.6 | 55144 | 82208.6 |
| `VInt_Level` | 8280 | 8214.3 | 6117 | 6524.5 |
| `VInt_Lag` | 166 | 485.2 | 2904 | 3844.8 |
| `VSync_Wait` | 79595 | 78862.6 | 34371 | 32685.5 |

**21.55 points of every max-diagonal frame are unattributed on the old instrument, against 0.67 on
ours** — and our 0.67 points is mostly *itemised* rather than residual. At max-diagonal it is 864.3
cyc/frame, of which:

```
hint bucket self       176.0   (the HInt exception entry, 44 × 4)
vint bucket self        44.0   (the VInt exception entry)
VBlank_Handler self    368.6   (the bracket around VInt_Level / VInt_Lag)
root frame self         65.3
                     -------
                       653.9   leaving ~210 in the main loop's dispatch between the five spans
```

Their 21.55 points has no such itemisation available, because there is no self figure to sum (W1).
And the loss appears exactly where invocations grow past one frame — W4's signature and nobody
else's: at idle, where a tick is 1.03 frames, their hole is 2.31 points; at max-diagonal, where it is
2.13 frames, it is **21.55**.

### 9.3 The identity — the claim ours can make that theirs cannot make at all

At **all three states**, on the undivided sample:

```
Σ routines[].cyclesSelfTotal + Σ interrupts[].cyclesSelfTotal + unattributedCycles == sampleCycles
        3968178                                                       + 0            == 3968178
```

Exact, remainder 0, with `unattributedCycles = 0`, `abandonedFrames = 0`, `depthExceeded = 0`,
`routines.truncated = false` (52 / 79 / 55 rows, all returned) — at idle, at max-diagonal and at
dense. There is no equivalent statement on the old instrument, because there is no self figure to sum
(W1) and no escape-hatch term for what is missing.

### 9.4 Task 5 — the row they recorded as unmeasurable is now a number

`ENGINE-BASELINE.md` §4b: `max_contiguous_dma_stall_cycles` is
`"UNMEASURABLE-ON-THIS-INSTRUMENT"`, with oracle-next's `stallCycles` named as the migration path.
Taking it here:

| state | stall cyc/video-frame | where it is | min/max per frame | spread |
|---|---:|---|---|---:|
| idle | **2263.8** | 100% inside `VBlank_Handler`, all of it in `Process_DMA_Critical` | 2174 / 2270 | 0 |
| maxdiag | **2200.4** | as above | 2182 / 2220 | 0 |
| dense | **2180.1** | as above | 2174 / 2212 | 0 |

Two things worth stating with it. First, the whole machine's stall is the VBlank DMA drain: the
per-frame ring's `stallCycles` equals `VBlank_Handler`'s equals `Process_DMA_Critical`'s, to the
cycle, at every state — nothing else on this ROM stalls measurably. Second, it sits beside their
*derived* indirect bound of **2745.5 cycles** for the largest single transfer (576 words), computed
from documented hardware rates rather than measured. A measured per-frame total of ~2200–2265 against
a derived single-transfer ceiling of 2745.5 is the right relationship, and it is the first time the
two can be put in one table.

**What this is not.** This is the per-frame *total*, not `maxContiguousStallCycles` — that field is
deliberately later than v1 (ruling, "The stall correction"). Their row asked for the contiguous
maximum and this does not answer it; it answers the quantity their instrument could not see at all.

---

## 10. Bonus anomaly 2 — the walker's two cost regimes: **not re-driven, and why**

`WALKER-MODEL.md` §5(c) books `anchor` as **named but not fitted**: the overlay costs +456.7 (W10),
+455.7 (W12) and **+1204.7** (W16), the third being 749 dearer because the anchor switches the
filler's *loop type* at the split rather than merely re-writing shifts. Two regimes,
`anchor_cycles_reglue_only` and `anchor_cycles_shipped_shape`, deliberately not collapsed into one
column.

**We did not reproduce it, and that is a scope statement, not a result.** The 17 fixtures exist only
as RAM-resident `ParallaxConfig` images built by `tools/parallax_cost_probe.py` — four pokes plus a
28-byte header and a `band_entry` array constructed by mutating the shipped config's bytes. Driving
them is a second measurement harness, not a second query, and it is outside this slice. **Stated
plainly: the two-regime curve is neither confirmed nor refuted by this document.**

**What *was* measurable, and it does reproduce.** `WALKER-MODEL.md` §6 is the model's out-of-sample
check against the config actually live at the idle baseline (`ParallaxConfig_OJZ_Underwater`, 4
authored bands, anchor split at screen line 80, BG sampling 144 lines) — i.e. the **shipped-shape
regime**, the one that costs +1204.7:

| | cycles | basis |
|---|---:|---|
| their model (un-anchored terms + `anchor_cycles_shipped_shape`) | 19288.7 | per invocation |
| their measured `Parallax_Update` | 19511 | per **video frame** |
| their measured, converted to per invocation (× 1.033 f/t) | 20161 | per invocation |
| **our measured `Parallax_Update`** | **20196.0** | per invocation, stall 0, spread 0, ROM `d22dda85` |

Two readings fall out. **(a) The two instruments agree on the walker to 35 cycles in 20,000 (0.17%)**
— among the closest agreements in this A/B (only `EntityWindow_Scan` at idle, −0.11%, is tighter),
and on precisely the routine the model is about. **(b) The
model's out-of-sample gap reproduces and is slightly larger than published**: against our exact
per-invocation figure it is **+907.3 (4.7%)**, against their own per-invocation figure **+872
(4.5%)** — not the **+222.3 (1.1%)** their §6 records, because that comparison divided a per-frame
average by nothing and their own probe comments flag the same 3% denominator trap elsewhere
(`engine_baseline_probe.py`, the `frames: sample` note). The gap is real, it is in the direction they
say (the shipped config's own band tops and scroll-factor shifts, which the fixtures hold constant),
and it is about four times the size their document states.

---

## 11. Open items — carried, not closed

1. **`VInt_Level` does not reconcile through `stallCycles`** (§6.3). Mechanism named (W4/W7 — their
   frame seam is the V-INT), residual not pinned to the cycle. This is the one place the ruling's
   *"stall-heavy rows reconcile through `stallCycles` or the arc stops"* is answered with "reconciles
   in mechanism, not in arithmetic". It is recorded here rather than bounded away, and it does not
   touch any gated row.
2. **The walker's two regimes** are unmeasured on our instrument (§10).
3. **`maxContiguousStallCycles`** is not in v1, so Task 5's literal question is still open (§9.4).
4. **Their `ENGINE-BASELINE.md` §1 byte-identity claim** for the max-diagonal raster program is
   contradicted by our read (§2.5) — two priming arm words, no cost consequence. Worth a one-line fix
   on their side.
5. **★ The short-routine residual (§6.3).** Five ungated rows — `Tile_Cache_Fill` at idle,
   `EntityWindow_Scan` at max-diagonal, `Section_UpdateColumns`, `Palette_Compose`, `BgAnim_Update` —
   disagree by 11–40% while their W4 straddle exposure is under 4%. W2 (unhooked `JMP`) was tested
   against the ROM and **rejected** (the `4EF9` partition does not track the delta); state phase was
   tested and rejected on our side (`Palette_Compose` and `BgAnim_Update` are exact constants at both
   states). **This delta has no named mechanism yet.** It is outside the gate set — the governing
   documents declare the inclusive figure non-comparable — so it does not stop the arc, but it is a
   real open question and the honest word for it is *unexplained*. The settling experiment is a
   paired event-level trace of one invocation on both instruments, which needs the reference running.

---

## 12. Per-frame material — the pull shape

Presented as machine-liftable tables per the demand side's ask. Each is 31 rows, the full sample, one
boot; **identical across all three boots** at each state (checked as part of §3). Fields are exactly
`get_profiler_frames`' `perFrame[]` rows.

**`maxdiag` is the max-diagonal state** and is the one their streaming residual (5 of 31 ticks
spiking past the budget line, cause unmeasured) will want first.

**The ring partitions tick-frames from lag-frames exactly, with no engine counter.** At max-diagonal
`vintCycles` takes two values and the histogram is decisive:

| state | `vintCycles` histogram over 31 frames | `Logic_Tick` delta | partition |
|---|---|---:|---|
| maxdiag | **13908 × 15**, 7852 × 15, 7840 × 1 | **15** | 15 high frames == 15 ticks, **exact** |
| dense | **21472 × 5**, 6212 × 26 | **5** | 5 high frames == 5 ticks, **exact** |
| idle | 9230 × 15, 9242 × 13, 8342/8310/7534 × 1 each | 30 | not separable — 30 ticks in 31 frames leaves nothing to separate |

So at any lagging state a client can read *which video frames carried a logic tick* straight off
`perFrame[].vintCycles`, then bucket by tick. Two consequences for the spike hunt: a per-tick
distribution is **two interleaved per-frame series**, not one — averaging them finds nothing — and
the high/low ratio is itself large (3.5× at dense, 1.77× at max-diagonal). `hintCycles` is flat
(2054 sparse, 37444 dense) and `stallCycles` follows the same partition (2220/2182 at max-diagonal,
2212/2174 at dense), so the tick's extra cost is VBlank work and DMA drain, not raster.

One irregularity is left in the data rather than smoothed: max-diagonal frame **211** reads 7840
where its neighbours read 7852, breaking the otherwise strict alternation across frames 210–212.
Twelve cycles, reproducible across all three boots.

> The dense state's *5 of 31* is **not** their streaming residual's *5 of 31* — different state,
> different cause, and the coincidence is stated only so nobody reads it as a match.

### idle
| frame | cycles | stallCycles | hintCycles | vintCycles |
|---:|---:|---:|---:|---:|
| 181 | 128002 | 2270 | 2054 | 9230 |
| 182 | 128008 | 2270 | 2054 | 9242 |
| 183 | 128002 | 2270 | 2054 | 9230 |
| 184 | 128012 | 2270 | 2054 | 9242 |
| 185 | 128002 | 2270 | 2054 | 9230 |
| 186 | 128010 | 2270 | 2054 | 9242 |
| 187 | 128002 | 2270 | 2054 | 9230 |
| 188 | 128012 | 2270 | 2054 | 9242 |
| 189 | 128002 | 2270 | 2054 | 9230 |
| 190 | 127998 | 2270 | 2054 | 9242 |
| 191 | 128014 | 2270 | 2054 | 9230 |
| 192 | 128000 | 2270 | 2054 | 9242 |
| 193 | 128012 | 2270 | 2054 | 9230 |
| 194 | 128000 | 2270 | 2054 | 9242 |
| 195 | 128014 | 2270 | 2054 | 9230 |
| 196 | 128000 | 2270 | 2054 | 9242 |
| 197 | 128000 | 2270 | 2054 | 9230 |
| 198 | 128012 | 2174 | 2054 | 7534 |
| 199 | 128010 | 2270 | 2054 | 8310 |
| 200 | 128002 | 2174 | 2054 | 8342 |
| 201 | 128002 | 2270 | 2054 | 9230 |
| 202 | 128012 | 2270 | 2054 | 9242 |
| 203 | 128002 | 2270 | 2054 | 9230 |
| 204 | 128000 | 2270 | 2054 | 9242 |
| 205 | 128012 | 2270 | 2054 | 9230 |
| 206 | 128000 | 2270 | 2054 | 9242 |
| 207 | 128014 | 2270 | 2054 | 9230 |
| 208 | 128008 | 2270 | 2054 | 9242 |
| 209 | 128002 | 2270 | 2054 | 9230 |
| 210 | 128000 | 2270 | 2054 | 9242 |
| 211 | 128012 | 2270 | 2054 | 9230 |

### maxdiag
| frame | cycles | stallCycles | hintCycles | vintCycles |
|---:|---:|---:|---:|---:|
| 205 | 128004 | 2220 | 2054 | 13908 |
| 206 | 128012 | 2182 | 2054 | 7852 |
| 207 | 128000 | 2220 | 2054 | 13908 |
| 208 | 128006 | 2182 | 2054 | 7852 |
| 209 | 128004 | 2220 | 2054 | 13908 |
| 210 | 128008 | 2182 | 2054 | 7852 |
| 211 | 128004 | 2182 | 2054 | 7840 |
| 212 | 128006 | 2182 | 2054 | 7852 |
| 213 | 128006 | 2220 | 2054 | 13908 |
| 214 | 128012 | 2182 | 2054 | 7852 |
| 215 | 128004 | 2220 | 2054 | 13908 |
| 216 | 128004 | 2182 | 2054 | 7852 |
| 217 | 128000 | 2220 | 2054 | 13908 |
| 218 | 128012 | 2182 | 2054 | 7852 |
| 219 | 128002 | 2220 | 2054 | 13908 |
| 220 | 128004 | 2182 | 2054 | 7852 |
| 221 | 128014 | 2220 | 2054 | 13908 |
| 222 | 128002 | 2182 | 2054 | 7852 |
| 223 | 128000 | 2220 | 2054 | 13908 |
| 224 | 128014 | 2182 | 2054 | 7852 |
| 225 | 128000 | 2220 | 2054 | 13908 |
| 226 | 128004 | 2182 | 2054 | 7852 |
| 227 | 128006 | 2220 | 2054 | 13908 |
| 228 | 128014 | 2182 | 2054 | 7852 |
| 229 | 127998 | 2220 | 2054 | 13908 |
| 230 | 128012 | 2182 | 2054 | 7852 |
| 231 | 128000 | 2220 | 2054 | 13908 |
| 232 | 128004 | 2182 | 2054 | 7852 |
| 233 | 128014 | 2220 | 2054 | 13908 |
| 234 | 128002 | 2182 | 2054 | 7852 |
| 235 | 128006 | 2220 | 2054 | 13908 |

### dense
| frame | cycles | stallCycles | hintCycles | vintCycles |
|---:|---:|---:|---:|---:|
| 193 | 128010 | 2174 | 37444 | 6212 |
| 194 | 128004 | 2212 | 37444 | 21472 |
| 195 | 128006 | 2174 | 37444 | 6212 |
| 196 | 128002 | 2174 | 37444 | 6212 |
| 197 | 128012 | 2174 | 37444 | 6212 |
| 198 | 128008 | 2174 | 37444 | 6212 |
| 199 | 128006 | 2174 | 37444 | 6212 |
| 200 | 127998 | 2212 | 37444 | 21472 |
| 201 | 128014 | 2174 | 37444 | 6212 |
| 202 | 128000 | 2174 | 37444 | 6212 |
| 203 | 128002 | 2174 | 37444 | 6212 |
| 204 | 128010 | 2174 | 37444 | 6212 |
| 205 | 128018 | 2174 | 37444 | 6212 |
| 206 | 127996 | 2174 | 37444 | 6212 |
| 207 | 128000 | 2174 | 37444 | 6212 |
| 208 | 128010 | 2212 | 37444 | 21472 |
| 209 | 128002 | 2174 | 37444 | 6212 |
| 210 | 128006 | 2174 | 37444 | 6212 |
| 211 | 128010 | 2174 | 37444 | 6212 |
| 212 | 128000 | 2212 | 37444 | 21472 |
| 213 | 128006 | 2174 | 37444 | 6212 |
| 214 | 128006 | 2174 | 37444 | 6212 |
| 215 | 128006 | 2174 | 37444 | 6212 |
| 216 | 128008 | 2174 | 37444 | 6212 |
| 217 | 128004 | 2174 | 37444 | 6212 |
| 218 | 128004 | 2174 | 37444 | 6212 |
| 219 | 128014 | 2174 | 37444 | 6212 |
| 220 | 128000 | 2212 | 37444 | 21472 |
| 221 | 128010 | 2174 | 37444 | 6212 |
| 222 | 128002 | 2174 | 37444 | 6212 |
| 223 | 128004 | 2174 | 37444 | 6212 |


---

## 13. What this A/B does NOT establish

Stated so nobody reads more into it than it says.

- **It is not a cycle-for-cycle validation of two emulators.** It compares one instrument's rows to
  another's on one ROM at three states. Exactly two quantities are gated: the HInt row on stall-free
  display-time paths, and `calls`. Everything in §6 is reported, not gated, because the governing
  documents say the inclusive figure is not comparable.
- **It says nothing about wall time on hardware.** Their figures are ideal cycles by construction;
  ours add a stall model that is itself a model. That our numbers match theirs exactly where stall is
  zero is evidence about the *nominal instruction timings*, and that is all.
- **Three states, one act, one section.** OJZ act 1 section 0 (plus section 2 for dense), one game
  state, a near-empty scene. Their §5 limits carry over verbatim and are not repeated here.
- **The `06af0010` ROM was never measured.** Every row is `d22dda85`. The moment their fix ladder
  lands, none of these rows compares to a re-measurement without a rebuild — which is why the CRC is
  in every table.
- **Their instrument was not re-run.** Every "theirs" figure is a quotation from `bc048e2a`, not a
  fresh measurement, so their side carries their spread and their caveats and this document cannot
  detect an error in their transcription.
- **The F0–F8 fixture leg of K.3 was not taken.** Those eleven fixtures are installed by
  `raster_cost_probe.py`'s encoder into RAM; like the walker fixtures they are a second harness. The
  dense pair's *derived* checks (`lines + 5` fires, `1512 + 328 × lines`) were verified instead, at
  96 lines, exactly — but the eight per-fire shapes were not re-driven.
- **No mechanism is claimed to a specific source line** where our data cannot separate two candidate
  lines; §7 says so explicitly rather than picking one.

---

## 14. Verification note

Run 2026-08-20 on branch `profiler-corpus-ab`, cut from `018612a`. `cargo build --release -p
oracle-aether` (the serialized lock was held for it) and nine server launches; **no
`mcp__oracle__*` tooling was used at any point, and no file under `crates/` was modified** — this
slice's diff is this document.

The `aeon` and `sigil` trees were read-only except for two **throwaway worktrees** (`bc048e2a` and
`7b46f075`) created in scratch space and removed after the ROM was built; neither checkout, branch,
nor daemon was touched.

Every corpus figure quoted above was read from `git show bc048e2a:<path>` rather than from a working
tree that can move. Every one of our figures came from `ab_results.json`, the raw capture of the nine
boots, and the derivations in §6 and §9 are arithmetic over that file and nothing else. The
sensitivity run in §6.3 (idle at settle 300) is a tenth boot, executed for that paragraph.
