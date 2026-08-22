# The short-routine residual — measured, hand-adjudicated, and split into three parts (2026-08-22)

Open item §11.5 of `docs/2026-08-20-profiler-corpus-ab.md`: five *ungated* rows disagree between our
profiler and Aeon's (oracle-old-based) instrument by 11–40% while their W4 straddle exposure is under
4%, and the honest word for it there is **unexplained**. This document is the settling run.

Every number below came off a run executed for this document, on the corpus ROM, on branch
`profiler-shortrow-residual` cut from `main` `a27e4d2`. Where a figure is theirs it is quoted from
`git show bc048e2a:<path>` with its section. **No `mcp__oracle__*` tooling was used at any point**, and
no file under `crates/` was modified — this slice's diff is this document.

---

## 0. Verdict, one page

**The residual splits into three parts. Two are now closed. One remains open and is smaller and much
better characterised than it was.** Nothing in any of the three is on our side of the comparison.

| part | what it is | status |
|---|---|---|
| **A** | **A denominator artifact in their DERIVED `cyc/logic-tick` column.** Compared the honest way — total cycles against total cycles over the same 31 video frames — **13 of the 28 row-state cells fall inside one invocation** of each other, which is the window-phase slack two independently-started samples must have. `VSync_Wait` goes 2.5% → **0.9%**; `Camera_Update` 2.4% → **~1.0%**; `EntityWindow_Scan` idle, published as −0.11%, is really **+1.02…+1.04 invocations**. Two rows the corpus A/B's §6.2 omitted, `Raster_VBlank` and `Enqueue_Dirty_Buffers`, are **+101%** and **+95%** wrong at maxdiag by pure construction — they are frame-driven and were multiplied by 2.067 anyway. | **CLOSED** (§3, §6.1) |
| **B** | **`BgAnim_Update` at idle — the one row in the whole set that reads HIGH — is a near-conserved TRANSFER across one call boundary.** Its excess (**+975.5 … +1006.5**) is matched by `Parallax_Update`'s deficit (**−1054.5 … −1023.5**): each row alone is off by 21% and 0.17%, the pair by **0.003–0.013%**. The two are called back-to-back with **one instruction between them** (`jsr $941C.l`, `$0A194A`). Near-conservation independently *derives* the invocation count as **N = 29.998 ≈ 30**, the tick count, which was not assumed. **It is not exact: the pair is short by 17–79 cycles**, and that shortfall is the same quantity as the transfer's ~1–3-cycle gap to the structural boundary — one open number, measured two ways (§5.2). | **CLOSED, with one named residual** (§5.2) |
| **C** | **Four rows still read LOW by far more than the W4 straddle ceiling allows** — `Palette_Compose` (**242×** the ceiling), `Section_UpdateColumns` (**28×**), `EntityWindow_Scan` maxdiag (**17×**), `Tile_Cache_Fill` idle (**13×**). Sign-consistent with a positional-pop defect closing victims early; **not quantitatively pinned.** | **OPEN, characterised** (§8) |

> **A correction landed after first commit and is recorded rather than quietly folded in.** The first
> version of this document read their published integer as a **floor**. It is **round-to-nearest** —
> re-derived here from their own two columns, which check their formatter: **26/26 for round against
> 15/26 for truncation, with 11 discriminating rows going 11–0** (§3.1a). Every band is now `31c ± 15.5`.
> The consequences are itemised where they land: part B no longer conserves exactly (above, §5.2), one
> row in §3.3 moved from inside to marginally outside ±1 invocation (§3.4), and **parts A, C, the §5.1
> impossibility bound, and every H1/H2/H3 verdict survive unchanged** — stated explicitly so a reader
> need not re-derive them to find out.

**And the arc's central question is answered by a third party.** Hand-derived from the ROM and the Yacht
v1.1 tables, on a bracket pinned from our source *before* counting: `Palette_Compose` costs **exactly
180** ideal 68000 cycles and `BgAnim_Update` **exactly 154**. Ours reads 180.0 and 154.0. **Theirs reads
~150 and ~187.** The hand count adjudicates for us on both rows, to the cycle, and matches theirs on
neither (§4).

Four further results, each from a measurement neither side had:

- **`BgAnim_Update` at idle is not merely wrong on their instrument — it is ARITHMETICALLY IMPOSSIBLE.**
  Tick-driven, 30 ticks in the window, hand-derived constant cost 154 → ceiling `31 × 154 = 4774`. Their
  band is **[5595.5, 5626.5)**, implying **36.33–36.54 invocations of a routine that ran 30 times**. At
  least **821.5 cycles that are not `BgAnim_Update`'s** are inside that row. No instrument comparison is
  involved. (§5.1)
- **The end-of-frame-FLUSH route is REFUTED on magnitude.** The flush charges `endCycle − start`, so it
  can only deliver ~1000 cycles if the victim opened ~1000 cycles before the frame end. Measured by
  `run_to` clock timestamps: `BgAnim_Update` enters at **19.3% into the video frame, with 103,286 CPU
  cycles still to run**, and the phase is locked by `VSync_Wait`. Wrong by **~3,000× per event**. (§5.3)
- **H2 is REJECTED and its partition comes out INVERTED.** A complete census of every 68000 write to
  `$C00000-$C0001F` (2050 hits at idle, **0 dropped**) puts the heaviest port-toucher —
  `HBlank_Vector_Slot`, 384 writes via `Raster_HInt` — at **0.000%** delta, while **all five §11.5 rows
  write to no VDP port at all**. (§6.2)
- **Controller test 1 lands and refutes the relayed model on the reference row.** `$FFB452` is **the HInt
  autovector target** (ROM `$000070 = $FFFFB452`; our caller lens reports `entryKind: hint`, 124/124
  calls; **no `jsr`/`bsr` to it exists anywhere in the image**), not a `JSR` target. The relayed model
  predicts **+96 cyc/frame** there against a measured **0**. (§6.4)

**Nothing STOPPED.** No BLOCKED item. The ROM rebuild reproduced the corpus image byte-identically, so
the listing is provably the corpus listing (§1.3) — load-bearing for the second consumer that was told
not to build one.

---

## 1. Provenance

### 1.1 The two instruments

| | revision | note |
|---|---|---|
| **Ours** (`oracle`, Rust) | branch `profiler-shortrow-residual`, cut from **`a27e4d2`** | binary `target/release/oracle-aether` built from that tree (`cargo build --release -p oracle-aether`); no `crates/` file modified |
| **Theirs** (`oracle-old`) | **not run here** | every "theirs" figure is quoted from the corpus documents at `bc048e2a` |
| **The corpus** (`aeon`) | **`bc048e2a`** — "Merge measure/scanline-p2-phase0" | read via `git show bc048e2a:<path>`; the live aeon tree has moved past it and was never checked out |

### 1.2 The ROM, re-verified from scratch

| | value |
|---|---|
| file | `s4.debug.bin` (sonic4, DEBUG shape) |
| **CRC32** | **`d22dda85`** |
| length | **713295** bytes |
| SHA-256 (first 16) | `ad289eae947b2dd4` |
| corpus pin | `INSTRUMENT-PARITY.md:5-6`, `ENGINE-BASELINE.md:6-7`, `WALKER-MODEL.md:6` |
| listing | `s4.debug.lst`, **5162 lines**, **2578 symbols** as counted by `emulator/status` |
| binding | `initialize` → `capabilities.symbolsLoaded: true`; a listing from a different build is refused |

> The live `aeon/s4.debug.bin` is a **different ROM** (crc `f8a1c567`, len 715010) and so is the
> `s4.debug.lst` beside it. Neither was used. The server's listing↔ROM binding check is the safety net
> and was not defeated or worked around.

### 1.3 How the ROM and the listing were recovered — and the binding proof

Two provenance chains, one image; the second is what makes the **listing** trustworthy.

1. **Chain A (extraction).** sigil's committed golden at `7b46f075`:
   `git -C sigil cat-file blob 633f5f88c56430377d105d3c5f5def705e666ec5` →
   `crates/sigil-harness/golden/s4.debug.bin`. Measured: **crc `d22dda85`, len 713295** — the corpus pin.
   **But a golden blob carries no listing.**
2. **Chain B (rebuild).** Throwaway worktrees — aeon at `bc048e2a`, sigil at `7b46f075` — built to a
   scratch `CARGO_TARGET_DIR` outside both repos:

```
SIGIL_BUILD=…/release/sigil SIGIL_EMIT=…/release/emit_sound_blob DEBUG=1 ./build.sh sonic4 --no-lint
  → built: sonic4 debug native ROM — crc=d22dda85 len=713295
```

3. **The binding proof.** The rebuilt image is **byte-identical** to the golden blob (same SHA-256,
   `ad289eae947b2dd4`). Therefore the `s4.debug.lst` the same invocation emitted beside it **is** the
   corpus listing — not "a listing that looks right", but the listing whose emitter produced the corpus's
   exact bytes. Its 5162 lines / 2578 symbols reproduce `docs/2026-08-20-profiler-corpus-ab.md` §1.2
   exactly.
4. **Both known failure modes were hit and are recorded as known, not as tree rot.** The lint stage's
   pytest suite dies with `ModuleNotFoundError: No module named 'launcher'` (their tools hardcode a
   `HARNESS` path that is now the Rust repo) — resolved by `--no-lint`, which emits no bytes. Behind it
   waits `[map.undeclared-island] ROM section at 0x99F0`, a genuine toolchain-version mismatch, which is
   why sigil is pinned to `7b46f075` rather than to its current head.
5. Both throwaway worktrees were removed at the end of this run. Neither checkout, branch, nor daemon in
   `aeon` or `sigil` was touched.

**The pair is staged for reuse, outside any git tree, so nobody has to repeat step 2:**

```
/home/volence/sonic_hacks/corpus-rom-d22dda85/
    s4.debug.bin     713295 B, crc d22dda85, sha256 ad289eae947b2dd4…
    s4.debug.lst     286414 B, 5162 lines, 2578 symbols
    PROVENANCE.md    the binding proof above, the rebuild recipe, the two build snags,
                     and the addresses this document names
```

It is a plain staging directory, not anyone's checkout — copy it, do not work in it.

### 1.4 Wall clock

All measurement runs on 2026-08-22 between **10:21:52** (`up 4 days, 10:45`, load 4.35) and **10:39:00**
(`up 4 days, 11:02`, load 2.35) — about **17 minutes** for 11 server launches plus the analysis. The
sigil build and the ROM build are separate commands and are not inside that figure.

---

## 2. The reproduction rituals

All states are reached through the Aether socket with a hand-rolled NDJSON JSON-RPC 2.0 client (the
`tools/aether_smoke.py` pattern). **One server process per boot** — an independent boot is a new machine,
never a `reset` on an old one. Server: `oracle-aether <rom> --socket <path> --no-pace`; the listing is
auto-bound from `<rom>.lst` beside the ROM.

Constants transcribed from `aeon/tools/engine_baseline_probe.py:106-116` at `bc048e2a`:
`SST_X_POS = 0x02`, `SST_Y_POS = 0x06`, `DIAG_AHEAD_X = 2000`, `DIAG_AHEAD_Y = 1400`.

### 2.1 The Stage-A sample (§3)

```
initialize {clientId, protocolVersion:1, clientCapabilities:{events:true}} ; initialized
  -> assert capabilities.symbolsLoaded == true          (the listing<->ROM binding gate)
emulator/status                                          -> romBytes 713295, symbolCount 2578
emulator/reset
emulator/run_frames {frames:180}                         -- SETTLE
<per-state ritual, below>
<read witnesses: Camera_X/Y, Camera_Art_Hold, Frame_Counter, Logic_Tick, Lag_Frame_Count,
 Dbg_Cam_Clamp_Frames>
emulator/set_profiler        {enabled:true, perFrame:true, callers:true}
emulator/run_frames          {frames:32}                 -- yields frameCount == 31
emulator/get_profiler        {}
emulator/get_profiler_frames {frames:32, top:512, topCallers:32}
emulator/set_profiler        {enabled:false}
<re-read every witness>
```

`idle` — no poke at all. `maxdiag` — resolve the leader through `Camera_Target`, add `(2000,1400)` px to
its `Sst.x_pos`/`y_pos` in 16.16, then `run_frames 24`. Both exactly as
`docs/2026-08-20-profiler-corpus-ab.md` §2.2–2.3.

**Reproduced witnesses, this run:** `frameCount` **31**, `sampleCycles` **3968178**, rows **52** (idle) /
**79** (maxdiag), `Camera_Art_Hold` 0, `Dbg_Cam_Clamp_Frames` 0 → 0, maxdiag camera
(320,368) → (560,608), `Logic_Tick` +30 (idle) / +15 (maxdiag). Every one matches the corpus A/B.
**Two boots per state: the `get_profiler_frames` replies are byte-identical.**

### 2.2 The VDP-port census (§6.2)

```
…settle + per-state ritual as above…
emulator/watchpoint_add {addr:"0xC00000", len:0x20, space:"bus", write:true, read:false,
                         mode:"record", label:"vdp-ports"}
emulator/run_frames     {frames:32}          -- idle;  {frames:6} at maxdiag, see the LOUD note
emulator/watchpoint_hits {limit:512}         -- paged on `cursor` until truncated == false
   -> attribute each hit by the server's own `symbol` field
```

> ⚠ **LOUD, not smoothed.** The first maxdiag census over the full 32-frame window **dropped 1997 of
> 6093 hits** (record cap). That census is **incomplete and is not reported as a partition.** It was
> re-run over a 6-frame window, which completed at **1194 hits, `dropped: 0`**, and that is the maxdiag
> row in §6.2. The idle census completed over the full window at **2050 hits, `dropped: 0`**.

### 2.3 The frame-phase measurement (§5.3)

```
…settle…
emulator/run_frames {frames:1} x2                  -- two boundaries -> frame length in mclk
emulator/run_to     {addr:"0x941C", maxFrames:8}   -- reply envelope carries `frame` and `mclk`
   offset_into_frame = (hit_mclk - boundary_mclk) mod frame_length
```

**This touches no profiler figure at all** — it is the machine's own clock, read off the run-to reply
envelope. Frame length measured: **896028 mclk = 128004 CPU cycles** at 7 mclk/cycle.

### 2.4 The disassembly

`capstone 5.0.7` (`CS_ARCH_M68K | CS_MODE_M68K_000`) over the ROM bytes — **an independent decoder, not
oracle's own `m68000::decode` path**, so the instruction stream Stage B costs is not supplied by the
instrument under test.

---

## 3. Stage A — count or cost?

### 3.1 What their two columns actually are

`ENGINE-BASELINE.md` §3 publishes `routine | calls | cyc/video-frame | %frame | cyc/logic-tick`. Two
facts govern everything below, and the corpus's own author has since confirmed both in writing:

- **`cyc/logic-tick` is DERIVED, not measured** — `cyc/video-frame × frames-per-tick`, rounded, with
  frames-per-tick `1.033` at idle (31/30) and `2.067` at maxdiag (31/15). Their words: *"Treat my
  `cyc/logic-tick` as a reconstruction, not a measurement."*
- **`calls` is per-video-frame and integer**, `max(1, floor(total/frames))`
  (`ControlSocket.cpp:2042-2043`), and reads `1` for every routine here. For a tick-driven routine at a
  non-integer frames-per-tick it *cannot* represent the true rate: the true idle rate is 0.968
  invocations per video frame and the column reads `1`; at maxdiag it is 0.484 and the column still reads
  `1`.

So their only measured quantity is `cyc/video-frame`, and it is an average over frames **including frames
in which the routine never ran**. Ours is a true per-invocation figure. The commensurable comparison is
neither: it is **TOTAL cycles over the same 31 video frames**, which is what both instruments watched.

### 3.1a The rounding band — DERIVED from their own two columns, not assumed

Their `cyc/video-frame` column is an integer, so every comparison below turns on what that integer
means. The two candidates give different bands, and the difference is not cosmetic — it decides whether
§5.2's pair conserves exactly or not:

| convention | band on the true total |
|---|---|
| truncation (floor) | `[31c, 31c + 31)` |
| **round-to-nearest** | **`[31c − 15.5, 31c + 15.5)`** |

**Their own data settles it, because `cyc/logic-tick` is derived from `cyc/video-frame` by a known
factor** (31/30 idle, 31/15 maxdiag) — so the published pair is a test of their formatter. Applying both
conventions to the 26 published row-states:

```
round-to-nearest reproduces the published cyc/logic-tick:  26 / 26
truncation       reproduces it:                            15 / 26

11 discriminating rows (the two conventions disagree), 11 for ROUND, 0 for TRUNC:
  idle    GameState_OJZScroll_Update  36295.83 -> published  36296   (trunc 36295)
  idle    VInt_Lag                      171.53 -> published    172   (trunc   171)
  idle    Palette_Compose               149.83 -> published    150   (trunc   149)
  idle    Camera_Update                 583.83 -> published    584   (trunc   583)
  idle    EntityWindow_Scan            1855.87 -> published   1856   (trunc  1855)
  maxdiag VInt_Level                  12641.80 -> published  12642   (trunc 12641)
  maxdiag VInt_Lag                     6001.60 -> published   6002   (trunc  6001)
  maxdiag Parallax_Update             25190.60 -> published  25191   (trunc 25190)
  maxdiag BgAnim_Update                 152.93 -> published    153   (trunc   152)
  maxdiag EntityWindow_Scan            1969.53 -> published   1970   (trunc  1969)
  maxdiag Tile_Cache_Fill            106162.60 -> published 106163   (trunc 106162)
```

**Eleven discriminating rows, eleven for ROUND, zero for TRUNC.** Every band in this document is
therefore `31c ± 15.5`.

> **This corrects an earlier draft of this document**, which used the truncation band and consequently
> reported §5.2's pair as conserving to *"an interval containing zero"*. It does not. Under the correct,
> narrower band the pair is **short by 17–79 cycles**, and §5.2 now says so — which turns out to make the
> finding sharper rather than weaker (§5.2's closing paragraphs).

### 3.2 Frame-driven or tick-driven — measured, not assumed

This decides, per row, whether their number is even the same kind of thing as ours. Read straight off
`callsTotal`:

| driver | rows | `callsTotal` idle / maxdiag | ticks in window |
|---|---|---|---|
| **display** (per HInt fire) | `HBlank_Vector_Slot` | 124 / 124 | — (4 fires × 31 frames) |
| **video-frame** | `Raster_VBlank`, `Enqueue_Dirty_Buffers` | **31 / 31** | — |
| **logic-tick** | `GameState_OJZScroll_Update`, `Parallax_Update`, `Palette_Compose`, `BgAnim_Update` | 29 / **14** | 30 / 15 |
| **logic-tick** | `VSync_Wait`, `VInt_Level`, `Camera_Update`, `EntityWindow_Scan`, `Section_UpdateColumns`, `Tile_Cache_Fill` | 29 / **15** | 30 / 15 |
| **lag** | `VInt_Lag` | 2 / 16 | — |

The 14/15 split at maxdiag is not noise — it is **call order**, and it reproduces the source exactly
(`games/sonic4/test/ojz_scroll_test.emp`): `Camera_Update`(:330) → `Tile_Cache_Fill`(:345) →
`EntityWindow_Scan`(:358) → `Section_UpdateColumns`(:364) → … → `Parallax_Update`(:512) →
`BgAnim_Update`(:514) → *[state returns]* → `Palette_Compose` (`engine/system/game_loop.emp:49`). The
rows that got 15 are the ones the in-flight tick had already finished when the sample closed; the ones
that got 14 are the ones it had not reached. **A free correctness check on the counts, and it passes.**

> **Two rows the corpus A/B's §6.2 omitted are the purest demonstration of the artifact.**
> `Raster_VBlank` and `Enqueue_Dirty_Buffers` run **once per video frame**, so multiplying them by 2.067
> invents work. At maxdiag their derived per-tick figures are **3075** and **2554** against our **1527.8**
> and **1307.3** — **+101%** and **+95%**, pure denominator artifact, on rows where nothing else is
> happening.

### 3.3 The reconciliation — total against total, with the rounding band shown

Their total over 31 frames is the interval `[31c − 15.5, 31c + 15.5)` (§3.1a). `Δ TOTAL` is carried as
that whole interval. `Δ / ourInv` expresses it in **our** invocations of that routine — the unit that
answers count-or-cost, because a window-phase difference between two independently-started samples can
move a row by **at most one invocation**. A row is marked ✓ only if the **entire interval** lies within
±1.00.

#### idle — 31 video frames, 30 logic ticks

| routine | ourN | our/inv | our TOTAL | thr c/vf | thr TOTAL (band) | Δ TOTAL | **Δ / ourInv** | ≤1 inv | verdict |
|---|---:|---:|---:|---:|---:|---:|---:|:-:|:--|
| `HBlank_Vector_Slot` | 124 | 469.5 | 58218 | 1878 | 58202.5–58233.5 | −15.5 … +15.5 | **−0.03 … +0.03** | ✓ | exact |
| `VInt_Level` | 29 | 8780.8 | 254642 | 8280 | 256664.5–256695.5 | +2022.5 … +2053.5 | +0.23 | ✓ | count/phase |
| `VSync_Wait` | 29 | 84301.4 | 2444740 | 79595 | 2467429.5–2467460.5 | +22689.5 … +22720.5 | +0.27 | ✓ | count/phase |
| `Camera_Update` | 29 | 598.0 | 17342 | 565 | 17499.5–17530.5 | +157.5 … +188.5 | +0.26 … +0.32 | ✓ | count/phase |
| `Raster_VBlank` | 31 | 1521.8 | 47176 | 1482 | 45926.5–45957.5 | −1249.5 … −1218.5 | −0.82 … −0.80 | ✓ | count/phase |
| `Parallax_Update` | 29 | 20196.0 | 585684 | 19511 | 604825.5–604856.5 | +19141.5 … +19172.5 | +0.95 | ✓ | **see §5.2 — this is Part B's other half** |
| `EntityWindow_Scan` | 29 | 1854.0 | 53766 | 1796 | 55660.5–55691.5 | +1894.5 … +1925.5 | **+1.02 … +1.04** | **✗ marginal** | ≈30 invocations **plus ~62 cycles** |
| `Enqueue_Dirty_Buffers` | 31 | 1416.6 | 43916 | 1356 | 42020.5–42051.5 | −1895.5 … −1864.5 | −1.34 … −1.32 | ✗ | **cost** |
| `GameState_OJZScroll_Update` | 29 | 40175.6 | 1165092 | 35125 | 1088859.5–1088890.5 | −76232.5 … −76201.5 | −1.90 | ✗ | **cost** (W4 covers) |
| **`Section_UpdateColumns`** | 29 | 978.0 | 28362 | 847 | 26241.5–26272.5 | **−2120.5 … −2089.5** | **−2.17 … −2.14** | ✗ | **COST — part C** |
| **`Palette_Compose`** | 29 | 180.0 | 5220 | 145 | 4479.5–4510.5 | **−740.5 … −709.5** | **−4.11 … −3.94** | ✗ | **COST — part C** |
| **`BgAnim_Update`** | 29 | 154.0 | 4466 | 181 | 5595.5–5626.5 | **+1129.5 … +1160.5** | **+7.33 … +7.54** | ✗ | **COST — part B** |
| **`Tile_Cache_Fill`** | 29 | 7952.5 | 230622 | 4629 | 143483.5–143514.5 | **−87138.5 … −87107.5** | **−10.96 … −10.95** | ✗ | **COST — part C** |
| `VInt_Lag` | 2 | 7520.0 | 15040 | 166 | 5130.5–5161.5 | −9909.5 … −9878.5 | −1.32 … −1.31 | ✗ | not the same quantity |

**6 of 14 entirely within ±1 invocation**, plus `EntityWindow_Scan` marginally outside.

#### max-diagonal — 31 video frames, 15 logic ticks

| routine | ourN | our/inv | our TOTAL | thr c/vf | thr TOTAL (band) | Δ TOTAL | **Δ / ourInv** | ≤1 inv | verdict |
|---|---:|---:|---:|---:|---:|---:|---:|:-:|:--|
| `HBlank_Vector_Slot` | 124 | 469.5 | 58218 | 1878 | 58202.5–58233.5 | −15.5 … +15.5 | **−0.03 … +0.03** | ✓ | exact |
| `Camera_Update` | 15 | 674.0 | 10110 | 319 | 9873.5–9904.5 | −236.5 … −205.5 | −0.35 … −0.30 | ✓ | count/phase |
| `Parallax_Update` | 14 | 26269.6 | 367774 | 12189 | 377843.5–377874.5 | +10069.5 … +10100.5 | +0.38 | ✓ | count/phase |
| `BgAnim_Update` | 14 | 154.0 | 2156 | 74 | 2278.5–2309.5 | +122.5 … +153.5 | **+0.80 … +1.00** | ✓ | **reconciles at 15 invocations: 15 × 154 = 2310, at the band's top edge** |
| `VSync_Wait` | 15 | 67550.1 | 1013252 | 34371 | 1065485.5–1065516.5 | +52233.5 … +52264.5 | +0.77 | ✓ | count/phase |
| `VInt_Level` | 15 | 13484.0 | 202260 | 6117 | 189611.5–189642.5 | −12648.5 … −12617.5 | −0.94 | ✓ | count/phase |
| `Raster_VBlank` | 31 | 1527.8 | 47362 | 1488 | 46112.5–46143.5 | −1249.5 … −1218.5 | −0.82 … −0.80 | ✓ | count/phase |
| `Enqueue_Dirty_Buffers` | 31 | 1307.3 | 40526 | 1236 | 38300.5–38331.5 | −2225.5 … −2194.5 | −1.70 … −1.68 | ✗ | **cost** |
| **`EntityWindow_Scan`** | 15 | 2326.0 | 34890 | 953 | 29527.5–29558.5 | **−5362.5 … −5331.5** | **−2.31 … −2.29** | ✗ | **COST — part C** |
| `Tile_Cache_Fill` | 15 | 125928.7 | 1888930 | 51369 | 1592423.5–1592454.5 | −296506.5 … −296475.5 | −2.35 | ✗ | **cost** (W4 covers) |
| **`Palette_Compose`** | 14 | 180.0 | 2520 | 67 | 2061.5–2092.5 | **−458.5 … −427.5** | **−2.55 … −2.38** | ✗ | **COST — part C** |
| **`Section_UpdateColumns`** | 15 | 9499.2 | 142488 | 3621 | 112235.5–112266.5 | **−30252.5 … −30221.5** | **−3.18** | ✗ | **COST — part C** |
| `VInt_Lag` | 16 | 7449.2 | 119188 | 2904 | 90008.5–90039.5 | −29179.5 … −29148.5 | −3.92 … −3.91 | ✗ | not the same quantity |
| `GameState_OJZScroll_Update` | 14 | 182033.3 | 2548466 | 55144 | 1709448.5–1709479.5 | −839017.5 … −838986.5 | −4.61 | ✗ | **cost** (W4 covers) |

**7 of 14 entirely within ±1 invocation. 13 of the 28 row-state cells across both tables.**

### 3.4 Stage A's answer

**For all five §11.5 rows the disagreement is in COST/ATTRIBUTION, not in COUNT.** Every one exceeds one
invocation — `Section_UpdateColumns` −2.17…−2.14, `Palette_Compose` −4.11…−3.94, `BgAnim_Update`
+7.33…+7.54, `Tile_Cache_Fill` −10.96…−10.95, `EntityWindow_Scan` (maxdiag) −2.31…−2.29 — so no
window-phase story reaches any of them. Meanwhile **13 of the 28 row-state cells the per-tick basis made
look like 0.1–5% disagreements are inside one invocation** on the honest basis, i.e. they are not
disagreements at all.

**And the sharpest lead resolves, both halves.** `BgAnim_Update` reconciles at maxdiag — their band
`[2278.5, 2309.5)` reaches `15 × 154 = 2310`, the tick count, at its top edge — and is **irreconcilable
at idle**, where their band `[5595.5, 5626.5)` implies **36.33–36.54 invocations of a routine that ran 30
times**. The asymmetry is not the routine and not the state: one of the two samples has extra cycles in
that row.

> **One row moved when the band was corrected, and it is flagged rather than left silent.**
> `EntityWindow_Scan` at idle sat at +1.03 invocations under the (wrong) truncation band and was counted
> as window phase. Under the correct band it is **+1.02 … +1.04 — marginally OUTSIDE ±1**. Read
> concretely: their row is ≈30 invocations **plus about 62 cycles** (2 cyc/video-frame). It is 60×
> smaller than any part-C row and 20× smaller than part B's transfer, so it changes no conclusion, but it
> is no longer clean window phase and is not reported as such.

**One direction fact worth naming, because §5 rests on it.** Re-scored against the tick count (`N = 30`)
rather than against our own 29, **`BgAnim_Update` is the only significant GAINER in the entire idle set**:

```
theirs_total − 30 × ours_per_invocation, idle   (band CENTRES, i.e. 31c; each carries ±15.5):
  BgAnim_Update          +991        <- the only significant positive
  EntityWindow_Scan       +56
  Camera_Update          −425
  Parallax_Update       −1039
  Section_UpdateColumns −3083
  VInt_Level            −6744
  VSync_Wait           −61597
  Tile_Cache_Fill      −95076
  GameState_OJZScroll  −116393
  Palette_Compose         −905
```

Everything else loses, and the large losses grow with invocation length — the W4 straddle signature.

---

## 4. Stage B — the instrument-independent third party

Neither emulator adjudicates the other. A cycle count derived by hand from the ROM and the 68000 timing
tables adjudicates both.

### 4.1 The bracket, pinned from OUR SOURCE before any counting

From `crates/oracle-core/src/profiler.rs`, `BusEventSink::on_step_retire` (:892-947) — the order of its
four steps is the whole answer:

1. `pending_call`, armed by the *previous* retirement, pushes a frame keyed by **this step's `pc`** — the
   callee's first instruction (:894-900).
2. …
3. **`charge(...)` runs AFTER the push** (:926-932): *"The entry cost belongs to the frame it opened, so
   charge after pushing."*
4. **Classification runs LAST** (:941-946): `ControlFlow::Call` arms `pending_call`;
   `ControlFlow::Return` calls `close_routine`.

Therefore:

| | charged to |
|---|---|
| the `JSR`/`BSR` itself | **the CALLER** — it retires before `pending_call` is consumed, so step 3 charges it to the frame still on top |
| the callee's first instruction | **the CALLEE** — pushed at step 1, charged at step 3 |
| the callee's `RTS` | **the CALLEE** — step 3 charges it, step 4 then closes the frame |

**Our bracket is `[first instruction of the routine … its RTS, inclusive]`, excluding the call
instruction.** Every count below is on that bracket, stated before the arithmetic because the bracket
convention is itself a candidate mechanism.

Both routines are **leaves in the sample** — `cyclesTotal == cyclesSelfTotal` exactly (5220 and 4466 at
idle) — which independently proves no `bsr`/`jsr` on either path was taken.

### 4.2 `BgAnim_Update` — the executed path

**Path determination.** `lea.l $27CD0.l, a3` then `move.w (a3)+, d7` loads the band count from
`BgAnim_Table`. **ROM `$027CD0` = `0000`: band_count is 0.** It is a ROM constant, so the path is the
same at both camera states — which is *why* the routine is a constant on our instrument, and why the
corpus A/B's "state phase" rejection was right for the right reason.

The `move.w sr,-(sp)` / `cmp.w` / `bls.w` / `move.w (sp)+,sr` sandwich is the DEBUG
`assert.w d7, ls, #BGANIM_MAX_BANDS`, and it is flag-transparent by construction: the `beq.w` at `$948E`
therefore tests the Z flag left by `move.w (a3)+, d7`, i.e. `band_count == 0` → taken → `.exit`.

| # | addr | bytes | instruction | Yacht v1.1 row | cycles |
|--:|---|---|---|---|---:|
| 1 | `$00941C` | `48E70018` | `movem.l a3-a4,-(sp)` | MOVEM `R→M .L -(An)` = `8+8m`, m=2 (mask `$0018` = A3,A4) | **24** |
| 2 | `$009420` | `47F900027CD0` | `lea.l $27CD0.l,a3` | LEA `(xxx).L` | **12** |
| 3 | `$009426` | `49F88CAA` | `lea.l $8CAA.w,a4` | LEA `(xxx).W` | **8** |
| 4 | `$00942A` | `3E1B` | `move.w (a3)+,d7` | MOVE `<ea>,Dn .W (An)+` | **8** |
| 5 | `$00942C` | `40E7` | `move.w sr,-(sp)` | MOVE from SR `-(An)` = INSTR 8 + EA 6 | **14** |
| 6 | `$00942E` | `BE7C0004` | `cmp.w #4,d7` | CMP `<ea>,Dn .W #<data>` = INSTR 4 + EA 4 | **8** |
| 7 | `$009432` | `63000058` | `bls.w $948C` *(taken)* | Bcc `.W branch taken` | **10** |
| 8 | `$00948C` | `46DF` | `move.w (sp)+,sr` | MOVE to SR `(An)+` = INSTR 12 + EA 4 | **16** |
| 9 | `$00948E` | `670000D2` | `beq.w $9562` *(taken)* | Bcc `.W branch taken` | **10** |
| 10 | `$009562` | `4CDF1800` | `movem.l (sp)+,a3-a4` | MOVEM `M→R .L (An)+` = `12+8m`, m=2 (mask `$1800` = A3,A4) | **28** |
| 11 | `$009566` | `4E75` | `rts` | RTS | **16** |
| | | | | **TOTAL** | **154** |

**Ours: 154.0. Hand-derived: 154. Theirs (idle): ~187.**

### 4.3 `Palette_Compose` — the executed path

**Path determination, from live memory at the sampled state.** Read at +0/+8/+16/+24/+32 frames across
the idle window and **constant throughout**: `Pal_Active` (`$FF8CA6`) = **`$10`**, `Pal_Base_Dirty`
(`$FF8CA7`) = `$00`, `Pal_Fade_Frames` (`$FF8CA2`) = `$00`, `Pal_Op` (`$FF8CA3`) = `$00`. `$10` is bit 4
set (`PAL_ACT_VARIANT`), bits 0/1/5 clear.

| # | addr | bytes | instruction | flag → branch | Yacht v1.1 row | cycles |
|--:|---|---|---|---|---|---:|
| 1 | `$007DF6` | `4A388CA6` | `tst.b $8CA6.w` | `$10` ≠ 0 → Z=0 | TST `.B (xxx).W` = 4+8 | **12** |
| 2 | `$007DFA` | `6602` | `bne.b $7DFE` | **taken** | Bcc `.B taken` | **10** |
| 3 | `$007DFE` | `4A388CA7` | `tst.b $8CA7.w` | `$00` → Z=1 | TST `.B (xxx).W` | **12** |
| 4 | `$007E02` | `6728` | `beq.b $7E2C` | **taken** | Bcc `.B taken` | **10** |
| 5 | `$007E2C` | `083800018CA6` | `btst.b #1,$8CA6.w` | bit1 of `$10` = 0 → Z=1 | BTST `#<data>,<ea> .B (xxx).W` = 8+8 | **16** |
| 6 | `$007E32` | `6702` | `beq.b $7E36` | **taken** | Bcc `.B taken` | **10** |
| 7 | `$007E36` | `4A388CA2` | `tst.b $8CA2.w` | `$00` → Z=1 | TST `.B (xxx).W` | **12** |
| 8 | `$007E3A` | `6704` | `beq.b $7E40` | **taken** | Bcc `.B taken` | **10** |
| 9 | `$007E40` | `4A388CA3` | `tst.b $8CA3.w` | `$00` → Z=1 | TST `.B (xxx).W` | **12** |
| 10 | `$007E44` | `6704` | `beq.b $7E4A` | **taken** | Bcc `.B taken` | **10** |
| 11 | `$007E4A` | `083800048CA6` | `btst.b #4,$8CA6.w` | bit4 of `$10` = **1** → Z=0 | BTST `#<data>,<ea> .B (xxx).W` | **16** |
| 12 | `$007E50` | `6712` | `beq.b $7E64` | **NOT taken** | Bcc `.B not taken` | **8** |
| 13 | `$007E52` | `083800058CA6` | `btst.b #5,$8CA6.w` | bit5 of `$10` = 0 → Z=1 | BTST `#<data>,<ea> .B (xxx).W` | **16** |
| 14 | `$007E58` | `670A` | `beq.b $7E64` | **taken** | Bcc `.B taken` | **10** |
| 15 | `$007E64` | `4E75` | `rts` | | RTS | **16** |
| | | | | | **TOTAL** | **180** |

Grouped: 4 × `tst.b (xxx).W` (48) + 3 × `btst #n,(xxx).W` (48) + 6 × `Bcc.b` taken (60) + 1 × `Bcc.b`
not-taken (8) + `rts` (16) = **180**.

**Ours: 180.0. Hand-derived: 180. Theirs (idle): ~150 per tick, ~155 per invocation.**

### 4.4 Yacht v1.1 citations, verbatim

All from `docs/reference/Yacht.txt` (flamewing, v1.1) — the repo's tracked permissive-documentation
timing source, per `docs/reference/README.md` (*"Timing / bus-stream idle structure → Yacht.txt"*). Line
numbers are that file's.

| instruction class | Yacht section | row | INSTR + EA | total |
|---|---|---|---:|---:|
| `TST .B (xxx).W` | `TST` (:757) | `(xxx).W` | 4 + 8 | 12 |
| `BTST #<data>,<ea> .B (xxx).W` | `BTST` (:337) | `#<data>,<ea> .B (xxx).W` | 8 + 8 | 16 |
| `Bcc .B` taken / not taken | `Bcc` (:1073) | `.B or .S` | — | 10 / 8 |
| `Bcc .W` taken | `Bcc` (:1073) | `.W branch taken` | — | 10 |
| `BSR .W` | `BSR` (:1066) | `.B .S or .W` | — | 18 |
| `RTS` | `RTS` (:926) | — | — | 16 |
| `JSR (xxx).W` / `(xxx).L` | `JSR` (:855) | `(xxx).W` / `(xxx).L` | — | 18 / 20 |
| `LEA (xxx).L,An` / `(xxx).W,An` | `LEA` (:844) | `(xxx).L` / `(xxx).W` | — | 12 / 8 |
| `MOVE.W <ea>,Dn (An)+` | `MOVE` (:389) | `<ea>,Dn .B or .W (An)+` | — | 8 |
| `MOVE.W SR,-(An)` | `MOVE from SR` (:638) | `-(An)` | 8 + 6 | 14 |
| `MOVE.W (An)+,SR` | `MOVE to CCR, to SR` (:653) | `<ea>,SR .W (An)+` | 12 + 4 | 16 |
| `CMP.W #<data>,Dn` | `CMP` (:1407) | `<ea>,Dn .B or .W #<data>` | 4 + 4 | 8 |
| `MOVEM.L regs,-(An)` | `MOVEM` (:704) | `R→M .L -(An)` | `8 + 8m` | 24 (m=2) |
| `MOVEM.L (An)+,regs` | `MOVEM` (:704) | `M→R .L (An)+` | `12 + 8m` | 28 (m=2) |

### 4.5 A third, independent confirmation of the 154 — from the machine's clock, not the profiler

`run_to` timestamps (§2.3) put `BgAnim_Update`'s entry at **173026 mclk** into its frame and
`Palette_Compose`'s at **174342 mclk** — a gap of **1316 mclk = 188 CPU cycles**. Disassembling what runs
in that gap:

```
$0A1946  4EB87406       jsr $7406.w          <- the call into Parallax_Update  (18)
$0A194A  4EB90000941C   jsr $941C.l          <- the call into BgAnim_Update    (20)
$0A1950  4E75           rts                  <- the game state's own return    (16)
$0026C4  61005730       bsr.w $7DF6          <- GameLoop's call into Palette_Compose (18)
$0026C8  60E6           bra.b $26B0          <- jbra GameLoop
```

`BgAnim_Update` (154, hand-derived) + `rts` (16, Yacht `RTS`) + `bsr.w` (18, Yacht `BSR .W`) = **188**.
**Measured 188.** The hand-derived 154 is confirmed by a clock reading that never touches the profiler.

### 4.6 Stage B's verdict, stated plainly

**The hand count matches OURS on both rows, exactly — 180 and 154 — and matches theirs on neither.**
There is no nearer-one to pick: 180 against their ~155, and 154 against their ~187, fall on opposite
sides.

This also **confirms, from the other direction, a documented invariant in our own source**
(`crates/oracle-core/src/bus.rs`, the `stall_cycles` field docs): that our per-instruction figure with
`stallCycles: 0` **is an ideal 68000 cycle count**, directly comparable to their ideal-cycle instrument.
Two hand-derived ideal counts landing exactly on two of our measured rows is what that invariant
predicts, and it held. **Had it not, that would have been the larger finding**, and it is recorded here
that it was tested rather than assumed.

---

## 5. The mechanism behind part B

### 5.1 A hard bound: their idle `BgAnim_Update` is impossible, not merely different

`BgAnim_Update` is tick-driven (§3.2). The idle window holds **30 logic ticks** (`Logic_Tick` +30,
reproduced). Its cost is the hand-derived **constant 154** (§4.2 — a ROM constant drives the path, so it
cannot vary). Therefore:

```
even allowing a 31st invocation:    31 × 154 = 4774 cycles
the honest tick ceiling:            30 × 154 = 4620 cycles
they publish (181 cyc/video-frame): band [5595.5, 5626.5)        (§3.1a)
```

**Excess over the generous ceiling: ≥ 821.5 cycles. Over the honest one: 975.5–1006.5.** Their row
implies **36.33–36.54 invocations of a routine that ran 30 times.** No instrument comparison is involved.
At least that many cycles inside their `BgAnim_Update` row belong to something that is not
`BgAnim_Update`. *(The conclusion is insensitive to §3.1a: under the truncation band the excess was
≥837, and the row is impossible either way.)*

### 5.2 Where they came from: a conserved transfer across one call boundary

`BgAnim_Update` is the only significant gainer in the idle set (§3.4). The routine called **immediately
before it**, with **exactly one instruction in between** (`jsr $941C.l` at `$0A194A`, §4.5), is
`Parallax_Update` — and `Parallax_Update` loses very nearly the same amount.

| | true, at N=30 | theirs (band, §3.1a) | Δ |
|---|---:|---:|---:|
| `BgAnim_Update` | 4620 | 5595.5 … 5626.5 | **excess +975.5 … +1006.5** |
| `Parallax_Update` | 605880 | 604825.5 … 604856.5 | **deficit −1054.5 … −1023.5** |
| **the pair** | **610500** | **610421 … 610483** | **short by 17 … 79** |

**The pair very nearly conserves — but not exactly, and the shortfall is itself the finding.** Each row
alone is off by 21% and 0.17%; together they are off by **0.003–0.013%**. A mechanism that *adds* cycles cannot
produce that; one that *moves a boundary between two adjacent brackets* does exactly that, and the 17–79
cycles it fails to conserve are quantified below rather than absorbed.

**N was not fitted — near-conservation derives it.** Setting `BgAnim_excess == Parallax_deficit` and
solving for the invocation count:

```
(theirs_BgAnim + theirs_Parallax) / N == 154 + 20196 == 20350
N == 610452 / 20350 == 29.9976        band [29.9961, 29.9992)
```

**29.998** — the tick count, to four significant figures, recovered from their published numbers and our
two per-invocation figures with no free parameter. That is the check that keeps this from being a
coincidence fitted after the fact. (The next plausible integers are refuted by orders of magnitude: at
N=29 the pair is short by 20,271–20,333 and at N=31 by 20,367–20,429.)

#### The size of the transfer, and the ONE quantity that is left over

Measured on each side separately, per invocation at N=30:

| | measured transfer | vs the structural boundary `rts`(16) + `jsr (xxx).L`(20) = **36** |
|---|---|---|
| gaining side, `BgAnim_Update` = `theirs/30 − 154` | **+32.52 … +33.55** | short by 2.45 … 3.48 |
| losing side, `Parallax_Update` = `20196 − theirs/30` | **+34.12 … +35.15** | short by 0.85 … 1.88 |
| **the two sides disagree by** | **0.57 … 2.63 cyc/inv** | |

**Those are not two loose ends — they are one, measured twice.** The two sides' disagreement of
0.57–2.63 cycles per invocation, over 30 invocations, is **17–79 cycles — exactly the pair's shortfall in
the table above.** The non-conservation and the "not quite 36" are the same quantity seen from two
directions, and stating it once is both more honest and more falsifiable than the earlier draft's
"conserves exactly, plus an unexplained 2–3 cycles beside it".

**So the single open quantity in part B is ~1–3 cycles per invocation**, and this run cannot name where
it goes. Two things are worth recording about it for whoever does:

- **Both sides fall short of 36, and both straddle 34 without containing it** — 34 being
  `rts`(16) + `jsr (xxx).W`(18), the call form used one instruction earlier, at `$0A1946` into
  `Parallax_Update`. The losing side misses 34 by only 0.12 cyc/inv. That is suggestive of which
  instruction the boundary actually lands between, and it is **not** claimed as the answer: neither
  structural value lies inside either measured interval.
- It is ~0.01% of `Parallax_Update`'s invocation and ~2% of `BgAnim_Update`'s, so it is invisible on the
  long row and visible only because the short row magnifies it — the same asymmetry that hid the whole
  transfer from the corpus A/B (§9.5).

**Two independent routes, one conclusion.** A parallel source audit read
`oracle-old/…/ControlSocket.cpp` and reports a shadow stack rebuilt empty each frame whose Exit arm pops
`stack.back()` and **never compares the address** — i.e. it identifies the closing event *by position in
a stream, not by identity*, so one desync mis-pairs every subsequent Exit in that frame. A boundary
shifted by one position between two adjacent brackets is precisely what that produces. **I did not read
their source**; this is measurement converging on their reading, not a confirmation of it.

**It is state-dependent, and the data says so plainly.** At maxdiag `BgAnim_Update` reconciles at exactly
15 × 154 with no transfer at all (§3.3). A *fixed bracket convention* could not do that; a *desync* —
state-dependent by nature — can. This is evidence for the desync reading and against a bracket-convention
reading, and it is the same evidence that rejects H3 (§6.3).

### 5.3 The end-of-frame-FLUSH route: REFUTED on magnitude

The flush charges `snap.endCycle − top.startCycle` — the entire remainder of the frame. For one flush
event to deliver ~1000 cycles, the spurious entry must open within ~1000 cycles of the frame end.
**Measured, by clock timestamps that touch no profiler figure (§2.3):**

| routine | entry offset into the video frame | **CPU cycles still to run before the frame end** |
|---|---:|---:|
| `Parallax_Update` | 3.4% | 123,592 |
| **`BgAnim_Update`** | **19.3%** | **103,286** |
| `Palette_Compose` | 19.5% | 103,098 |
| `VSync_Wait` | 19.6% | 102,890 |
| `VBlank_Handler` (V-INT entry) | 85.5% | 18,506 |

`BgAnim_Update` runs at **19.3% into the frame**, and the loop is frame-synchronous — `VSync_Wait` spins
the remaining ~66% of every idle frame — so that phase is **locked, not drifting**. A flush charging from
`BgAnim_Update`'s start would deliver **~103,000 cycles, not ~34**: wrong by a factor of ~3,000 per
event. The observed excess is **30 small transfers of ~34**, not one large flush.

**Per review bar 5, this is the check that keeps §5.2 from being a confound.** The transfer's magnitude
was not fitted: it is a single call boundary, read out of the ROM and costed from Yacht, and the
near-conservation independently recovers the tick count. Where the model does *not* predict exactly —
the ~1–3 cycles per invocation, which is simultaneously the pair's shortfall and the gap to 36 — that is
stated as one open quantity rather than tuned away.

### 5.4 The `calls` fingerprint: unavailable in the published form

Their flush also runs `routineMap[top.address].calls++`, so a desync inflates the victim's invocation
count and not only its cost — a cheaper refutation, if the data existed. **It does not, in the published
table.** `calls` is `max(1, floor(total/31))`: a clean tick-driven row (30 raw calls) prints `1`, and a
row inflated to 36–37 also prints `1`. The column only reaches `2` at **≥ 62** raw calls. So the
published `1` bounds their raw count at ≤ 61 and **carries no information about this question**. Reading
the rounded `1` as evidence either way would be reading past the rounding.

**A live ask for aeon: the RAW `calls` for `Parallax_Update` and `BgAnim_Update` at idle.** §5.2 predicts
an excess on `BgAnim_Update` and none on `Parallax_Update`; a raw 30/30 would weaken it, a raw ~36/30
would confirm it from their own instrument.

> **Postscript (2026-08-22, after the ask went out — the ask is WITHDRAWN and the `max(1, …)` above is
> the reason.** Recorded because the rescue was well-argued and nearly landed.)
>
> aeon replied that the published `1` *does* carry information: the normalization is integer division
> in the consumer (`ControlSocket.cpp:2042`, `int avgCalls = st.calls / numFrames;`) with `numFrames`
> = 31, so a displayed `1` would require `st.calls >= 31`, whereas a healthy tick-driven routine at 30
> invocations gives `30 / 31 == 0`. Their table reads `1`, so — the argument runs — the row shows an
> excess invocation count from their own instrument, exactly the flush-victim signature §5.2 predicts.
>
> **It does not, and the refutation is the very next line:**
>
> ```cpp
> int avgCalls = st.calls / numFrames;   // :2042  <- cited
> if (avgCalls < 1) avgCalls = 1;        // :2043  <- the clamp
> ```
>
> A computed `0` is forced to `1` before serialisation, so the table **cannot** print `0` and a
> displayed `1` means `st.calls` ∈ **[0, 61]** — `[31, 61]` by the division, `[0, 30]` by the clamp.
> No lower bound survives. Their dependency (30 ticks, once-per-tick) is correct and was verified
> independently; it simply never becomes load-bearing, because the clamp destroys the low end first.
> This is the `max(1, …)` already stated above, arrived at from the other direction and refuted by it.
>
> **The consequence is worse than "coarse", and it is the durable finding here:** the one value that
> would have been diagnostic — `0`, the normal reading for *every* tick-driven routine at idle, meaning
> "fewer invocations than frames" — is the exact value the code refuses to emit. The column is not
> merely uninformative about this question; it is structurally unable to represent the true state. If
> that consumer is opened for the identity-pairing fix, the clamp is worth removing in the same pass.
>
> **The ask is withdrawn rather than left standing.** Raw `st.calls` is unreachable from the aeon side
> by construction — the division happens in the consumer before the response is built, so their probe
> never sees the total; it lives in `routineMap[…].calls` (`:1991`, `:2012`), inside the very code
> carrying the defect. And with the clamp there is no longer a predicted excess for it to confirm, so
> §5.2 stands on the conservation derivation alone, which is where it always did its work.

---

## 6. The hypotheses — verdicts, each with its killing or supporting test

### 6.1 H1 — denominator / lag-scaling artifact: **SUPPORTED, and it does about half the work**

**Supporting test.** Recomputing every row on TOTAL cycles over the same 31 frames rather than on their
derived per-tick column (§3.3) moves **13 of the 28 row-state cells** to inside **one invocation** — the
maximum a window-phase difference can produce. Rows the corpus A/B published as disagreements which are
not: `VSync_Wait` (+2.5% → +0.9%), `Camera_Update` (+2.4% → ~+1.0%), `VInt_Level` (+2.6% → +0.8%),
`Parallax_Update` (+0.17% → +0.95 invocations — though see §5.2, that one is Part B's other half). The
purest cases are `Raster_VBlank` and `Enqueue_Dirty_Buffers` at maxdiag: **+101%** and **+95%** wrong by
construction (§3.2).

**But H1 cannot reach the class.** All five §11.5 rows exceed one invocation on the total basis (§3.4),
and the sharpest of them, `BgAnim_Update` at idle, is 21% high **in the measured per-video-frame number,
before any derivation touches it** (154/1.033 = 149.1 predicted against 181 measured). The artifact is
downstream of the divergence and cannot be its cause.

**This verdict is unchanged by the §3.1a band correction** — the narrower band moves the individual
figures by ≤0.03 invocations and moves exactly one cell across the ±1 line (`EntityWindow_Scan` idle,
§3.4), which is called out there rather than absorbed here.

### 6.2 H2 — cycle-attribution / port-cost asymmetry: **REJECTED, by an inverted partition**

**Killing test.** A complete census of every 68000 write to `$C00000-$C0001F` during the window,
attributed by the server's own symbol resolution. **idle: 2050 hits, `dropped: 0`.**

| writes (idle, 31 frames) | writing code | profiled row it belongs to | that row's Δ (total basis) |
|---:|---|---|---:|
| 608 | `Flush_VDP_Shadow.loop` | VBlank bracket → `VInt_Level` | +0.8% |
| 868 | `Process_DMA_Critical.drain_1..4` | VBlank bracket → `VInt_Level` | +0.8% |
| **384** | **`Raster_HInt` + `.region_loop` `.op_region` `.op_cram` `.op_reg` `.cram_loop`** | **`HBlank_Vector_Slot`** | **+0.000%** |
| 128 | `Vscroll_Write` + `.whole_plane` | VBlank path (`requires(vblank)`, `engine/level/parallax.emp:426`) | — |
| 30 | `VInt_DrawLevel.done` | VBlank bracket | — |
| **0** | — | **`Palette_Compose`** | **−13.9%** |
| **0** | — | **`BgAnim_Update`** | **+25.6%** |
| **0** | — | **`Tile_Cache_Fill`** | **−37.8%** |
| **0** | — | **`Section_UpdateColumns`** | **−7.4%** |
| **0** | — | **`EntityWindow_Scan`** | +3.6% idle / **−15.3%** maxdiag |
| **0** | — | **`Parallax_Update`** | +3.3% (0.95 invocations) |

maxdiag, complete over a 6-frame window (**1194 hits, `dropped: 0`**): the same writer set —
`VInt_DrawLevel` (804), `Process_DMA_Critical` (147), `Flush_VDP_Shadow` (114), `Raster_HInt` (72),
`Vscroll_Write` (24), `Enqueue_Dirty_Buffers.ship_reg` (6). **Still zero for all five §11.5 rows.**

**The partition is exactly inverted from H2's prediction.** The single heaviest port-toucher —
`HBlank_Vector_Slot`, whose entire job is VDP register and CRAM/VSRAM port writes — is the **most exact
agreement in the whole set: 0.000% at both states, `stallCycles` 0.** Every row that disagrees touches no
port at all. H2 predicted inflation where ports are touched and none where they are not; the measurement
shows the opposite, on the same standard the corpus A/B used to reject W2's `4EF9` partition.

**Corroborated from the other direction.** Stage B's two hand-derived paths contain **no port access
whatsoever** — every effective address is work-RAM absolute-short or ROM — and both land exactly on our
figures. If our core folded port cost into instruction cycles, these are the last two rows it could
affect, and they are two of the five that disagree. (See also §4.6: the `bus.rs` `stall_cycles` invariant
says our figure with `stallCycles: 0` *is* an ideal count, and the hand derivations confirm it.)

### 6.3 H3 — bracket convention: **REJECTED as an explanation; our bracket independently VALIDATED**

**Validating test.** Our bracket is pinned from source (§4.1) *before* the counting, and both hand
derivations, computed on that bracket, land exactly (180, 154). A wrong bracket would have shown as a
constant offset in both.

**Killing test.** A bracket-convention difference is **bounded**: the largest call form here is
`jsr (xxx).L` (20) and the return is `rts` (16), so no bracket disagreement can exceed **±36
cycles/invocation**. Per-invocation deltas at the tick count:

| row | Δ per invocation | inside ±36? |
|---|---:|:--|
| `Palette_Compose` idle | −30.68 … −29.65 | yes |
| `BgAnim_Update` idle | +32.52 … +33.55 | yes |
| `Camera_Update` idle | −14.68 … −13.65 | yes |
| `Parallax_Update` idle | −35.15 … −34.12 | yes (barely) |
| `Palette_Compose` maxdiag | −42.57 … −40.50 | **no** |
| `Section_UpdateColumns` idle | −103.28 … −102.25 | **no** |
| `EntityWindow_Scan` maxdiag | −357.50 … −355.43 | **no** |
| `Section_UpdateColumns` maxdiag | −2016.83 … −2014.77 | **no** |
| `Tile_Cache_Fill` idle | −3169.70 … −3168.67 | **no** |

H3's sharp prediction — a fixed offset, hence a delta vanishing in percentage terms on long rows — is
also contradicted: `Parallax_Update` (20,196 cyc/inv) carries −34 while `Tile_Cache_Fill` (7,952) carries
−3,169, and `Camera_Update` (598) carries −14. **And the consistency check the brief demanded is
satisfied**: no fixed convention can put `BgAnim_Update` at +33 and `Palette_Compose` at −30 at idle and
both within a few cycles of 0 at maxdiag.

**Unchanged by the §3.1a band correction** — the band moves each interval by about ±1 cycle, and the four
rows that break the ±36 ceiling break it by 3× to 88×.

### 6.4 Controller test 1 — the `$FFB452` entry form: **the relayed model is REFUTED on that row**

The relayed source-side model predicts the corpus's PHASE-0 reference row can only match to the cycle if
it is entered by `jsr (An)` **and is a `JSR` target rather than the HInt vector**, because the vector path
would predict **+96 cyc/frame** against a measured **0**.

**Measured, three independent ways:**

- **The ROM.** The level-4 autovector at `$000070` reads **`$FFFFB452`**. The listing spells
  `$FFFFB452 : HBlank_Vector_Slot`. It **is** the HInt vector target.
- **Static scan.** **No `jsr (xxx).L` to `$FFFFB452` or `$00FFB452` exists anywhere in the 713295-byte
  image.** Nothing calls it.
- **Our caller lens.** `HBlank_Vector_Slot`'s row has **exactly one caller edge**, and it is
  `entryKind: hint` with `calls: 124` of 124 — the HInt interrupt bucket, not a routine.

The antecedent fails, so the model's own prediction for this row is +96 cyc/frame; the measurement is
**+0, at both states**. Reported as a refutation of the model *as relayed to me*, not of the source
reading it came from — I did not read `oracle-old`.

### 6.5 Controller test 2 — the desync census: **fuel present; the mechanism is NOT killed**

Zero desync opportunities would kill the mechanism outright. There are not zero.

- **Calls open at their frame seam: never zero.** The V-INT lands at **85.5% into the video frame**
  (§5.3), inside `VSync_Wait` — a routine 2+ frames deep in the call chain consuming 84,301 of 128,004
  cycles per idle frame (65.9%). Every one of the 31 frames has a live call chain across their seam.
- **Static fuel census** over the corpus image (word-aligned, **code and data undifferentiated — an
  upper bound on fuel, not an execution count**): `RTR` 7, `TRAP` 78, `JMP (xxx).L` 97, `JMP (An)` 24,
  other `JMP` forms 226, `RTE` 6, against `RTS` 426, `JSR (xxx).L` 104, `JSR (An)` 25.

**W2 (unhooked `JMP`) is therefore UNTESTED here, not dead.** The corpus A/B's §6.3 rejection asked which
routines *contain* a `4EF9`; a positional desync is not local — it mis-pairs subsequent Exits anywhere in
the frame — so the partition that was tested was the wrong partition. This run neither restores nor
refutes it; a `JMP` **execution** count within the window would, and that needs an instruction trace this
server has no method for (§8, TAG-1).

---

## 7. What this document does NOT establish

- **Their instrument was not re-run.** Every "theirs" figure is a quotation from `bc048e2a`. Their side
  carries their spread and their caveats, and this document cannot detect an error in their transcription.
- **The §5.2 mechanism is inferred from our side of the pair.** What is *measured* is that
  `Parallax_Update`'s deficit and `BgAnim_Update`'s excess conserve to within 0.003–0.013% of the pair total, that the
  near-conservation recovers the tick count with no free parameter, that the transfer is one call
  boundary in size, and that the flush route is magnitude-wrong. **It is not exact** — the residual
  ~1–3 cycles per invocation is reported as part B's one open number (§5.2, §8.2). Which line of their
  consumer produces any of it is **not pinned here**, and §5.2 says so rather than picking one.
- **Part C is not adjudicated** (§8). Four rows still read low by more than any mechanism measured here
  accounts for.
- **Two states, one act, one section.** OJZ act 1 section 0. `dense` was not run — it is not in §11.5's
  row set.
- **The static fuel census is static.** It bounds opportunity; it does not count executions.
- **The maxdiag port census covers 6 frames, not 31** — the 31-frame attempt dropped 1997 hits and is
  discarded rather than reported (§2.2).

---

## 8. Open items

1. **★ Part C — four rows still read LOW by far more than W4 allows.** Per invocation at the tick count,
   with the W4 straddle ceiling `L/256000` for comparison:

   | row | Δ/inv | Δ % | W4 ceiling | outside by |
   |---|---:|---:|---:|---:|
   | `Palette_Compose` idle | −30.68 … −29.65 | −17.0% | 0.07% | **242×** |
   | `Palette_Compose` maxdiag | −42.57 … −40.50 | −23.6% | 0.07% | **336×** |
   | `Section_UpdateColumns` idle | −103.28 … −102.25 | −10.6% | 0.38% | **28×** |
   | `Section_UpdateColumns` maxdiag | −2016.83 … −2014.77 | −21.2% | 3.71% | **6×** |
   | `EntityWindow_Scan` maxdiag | −357.50 … −355.43 | −15.4% | 0.91% | **17×** |
   | `Tile_Cache_Fill` idle | −3169.70 … −3168.67 | −39.9% | 3.11% | **13×** |

   All are **losses**, which is the sign a positionally-matched pop produces when it closes a victim
   early, and none has an identified counterpart gainer in the published set. **Sign-consistent,
   magnitude-unpinned. This is the honest remainder of §11.5**, and it is **unchanged by the §3.1a band
   correction** — the narrower band moves each figure by ~1 cycle against overruns of 6× to 336×.
2. **The ~1–3 cycles per invocation part B does not predict** (§5.2) — **one quantity, not two**: the
   pair's 17–79-cycle shortfall and the transfer's 0.85–3.48-cycle gap to the structural
   `rts`+`jsr (xxx).L` = 36 boundary are the same number seen from two directions. Both sides straddle
   `rts`+`jsr (xxx).W` = 34 without containing it, the losing side by only 0.12 cyc/inv.
3. **W2 (unhooked `JMP`) is reopened and untested** (§6.5).
4. **The raw `calls` ask** (§5.4) — cheap and decisive, available from aeon's side alone.
5. **TAG-1 — no instruction trace on this server.** `emulator/step` and breakpoints do not exist in this
   Aether surface (only `run_to`), so a `JMP`-execution count and an event-level trace of our own side are
   not reachable from a client. Recorded as a surface limit; not approximated by a proxy and labelled with
   the original question's name.

### 8.1 The paired trace, specified concretely enough to dispatch

If part C must be closed, this is the experiment. It needs the reference running.

| | |
|---|---|
| **ROM** | `s4.debug.bin`, crc `d22dda85`, len 713295, with the `s4.debug.lst` this run rebuilt (binding proof §1.3) |
| **State** | `idle` — boot, `run_frames 180`, no poke |
| **The invocation** | `BgAnim_Update`, entered at **`$00941C`**, called by `jsr $941C.l` at **`$0A194A`**. Any invocation after settle; the path is a ROM constant so all are identical. Take the 5th–10th |
| **Our expected event list** | `$941C` → `$9420` → `$9426` → `$942A` → `$942C` → `$942E` → `$9432` (`bls.w`, taken) → `$948C` → `$948E` (`beq.w`, taken) → `$9562` → `$9566` (`rts`). **11 instructions, 154 cycles**, per §4.2 |
| **The two events that decide it** | their `SubroutineEntry` for `$941C` and the `SubroutineExit` their consumer pops against it. **Prediction: the Exit paired with that Entry is not the `rts` at `$9566`**, and correspondingly their `Parallax_Update` bracket (`$007406`, called by `jsr $7406.w` at `$0A1946`) closes ~34 cycles early |
| **The falsifier** | if their Entry/Exit pair brackets exactly `$941C … $9566` *and* `Parallax_Update`'s brackets exactly `$7406 … its rts`, §5.2 is wrong and the excess is elsewhere |
| **The part-C leg** | the same trace on `Palette_Compose`, entry **`$007DF6`**, called by `bsr.w` at **`$0026C4`**, expected **15 instructions, 180 cycles** (§4.3); and on `Section_UpdateColumns` (`$006E78`) and `Tile_Cache_Fill` (`$005D60`), where the loss is 6×–336× the straddle ceiling and no gainer is identified |
| **The two builds** | theirs at aeon `bc048e2a` / oracle-old; ours at `oracle` `profiler-shortrow-residual` (this branch) |

### 8.2 A note on the fix, with its caveat

If §5.2 holds, their Exit event **already carries an address** and their consumer discards it — which
would make this a **consumer-side defect, fixable without touching their emulator core.** State the
caveat with the option: the Exit carries the *return* address while the stack entry holds the *entry*
address, so a verification check cannot compare the two directly — it would have to pair against the
recorded return location, which their core already tracks. **Do not read this as a specified fix**; it is
an observation, made by a session that did not read their source, that the information needed to verify a
pop is present in the stream.

---

## 9. Consequences for the corpus A/B

Offered as edits to `docs/2026-08-20-profiler-corpus-ab.md`, not applied here.

1. **§11.5 can be rewritten as three parts — two closed, one open and characterised** (§0).
2. **§6.1 and §6.2's tables should carry the total-cycle basis beside the per-tick one.** As they stand
   they publish denominator artifact as measurement on at least five rows, and they omit `Raster_VBlank`
   and `Enqueue_Dirty_Buffers` at maxdiag, which are the two clearest demonstrations of the artifact
   (+101%, +95%).
3. **§6.3's W2 rejection should be downgraded to UNTESTED** — the `4EF9`-containment partition is the
   wrong partition for a non-local desync (§6.5).
4. **§6.3's W4 straddle ceiling is sound but was applied to the wrong class.** It covers the long rows
   correctly; the short rows are not straddle at all, and part C is 6×–336× outside it.
5. **★ §6.3's "the one row where the two instruments essentially agree is the walker" must be footnoted,
   and this is the most load-bearing edit in the list.** That line is currently cited as *"the best
   evidence here that our nominal 68000 timings and theirs are the same timings — which is what makes the
   §5.1 exact matches meaningful rather than lucky."* It cannot carry that weight. On the total basis
   `Parallax_Update`'s 0.17% is **not agreement**: it is a **−1054.5 … −1023.5-cycle deficit that is the
   counterpart of `BgAnim_Update`'s +975.5 … +1006.5 excess** (§5.2). It *reads* as agreement only because
   it is a 20,000-cycle routine — which is precisely why a ~34-cycle-per-invocation transfer hides in it.
   The agreement was an artifact of the divisor, not evidence about timings.

   **What still carries that evidential weight, and carries it far better:** §4's hand derivations. Two
   routines, costed instruction by instruction from the ROM bytes (capstone, an independent decoder) and
   the Yacht v1.1 tables, landing **exactly** on our 180.0 and 154.0 — with a third, independent
   confirmation from `run_to` clock timestamps (§4.5) that touches no profiler figure at all. That is a
   direct check of our nominal timings against the published 68000 tables, where the walker was only ever
   an indirect check of ours against theirs. And the `HBlank_Vector_Slot` row — 1878 against 1878 at both
   states, `stallCycles` 0, on the one row with no straddle and no preemption exposure (§6.2) — remains a
   genuine instrument-to-instrument agreement, because it is display-driven and so has no lag denominator
   to hide anything in.
6. **A new claim for their side:** their published idle `BgAnim_Update` row (181 cyc/video-frame) is
   **arithmetically impossible** for that routine (§5.1) — worth a retraction rather than a tolerance. The
   corpus author has said in writing they will retract rather than defend if the hand-derivation rules
   against it. It does.

---

## 10. Verification note

Run 2026-08-22 on branch `profiler-shortrow-residual`, cut from `a27e4d2`. `cargo build --release -p
oracle-aether`; **`cargo test` was not run in this repo at all** (no Rust source was touched and no
aggregate was needed). Eleven server launches. **No `mcp__oracle__*` tooling was used at any point, and
no file under `crates/` was modified** — this slice's diff is this document.

The `aeon` and `sigil` trees were read-only except for two **throwaway worktrees** (`bc048e2a` and
`7b46f075`) created in scratch space under this worktree and **removed after the ROM was built**; neither
checkout, branch, nor daemon was touched. Every corpus figure quoted above was read from
`git show bc048e2a:<path>` rather than from a working tree that can move.

Disassembly is `capstone 5.0.7` — an independent decoder. Cycle costs are `docs/reference/Yacht.txt` v1.1,
cited row by row in §4.4. Neither is oracle's own timing path, which is the point of Stage B.
