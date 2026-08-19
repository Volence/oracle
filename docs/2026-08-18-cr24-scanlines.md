# CR-24 — `emulator/scanlines`: the readback the sweep is waiting on

**Status: PROPOSED 2026-08-18.** Raised against `empyrean/contract/protocol.md` §6 (VRAM / CRAM /
layers) and `contract/schema/bus-protocol.schema.json`; would land as amendment **§11.14** and take
the vendored schema's method fragments **32 → 33**. The demand source is
`docs/2026-08-18-aeon-scanline-readback-demand.md` (Aeon's second demand-side statement, Ask 1 with
its two follow-ups and closing addition); the acceptance fixture is
`aeon/docs/benchmarks/scanline-p2/HBLANK-WINDOW-SWEEP-SPEC.md` (aeon commit `1fb982f7`, branch
`parcel/raster-substrate-byte-moving`), read firsthand for this CR.

**Sequencing.** Contract-first, per the 2026-08-17 owner ruling CR-21/22/23 cite: §6 row and schema
fragment before any handler. §8's first must-not is *"invent new ops not in this spec"*
(`protocol.md:1440`), and no row for any pixel readback exists anywhere in `empyrean` — this document
is the raising. Every line anchor below was read in the file it names on 2026-08-18; every quoted
sentence exists verbatim at its anchor.

**The contract has already named this gap, in its own voice.** `emulator/pixel_attribution`'s prose —
the paragraph §6 itself calls *"the single most misreadable property of the method"* — closes its
first normative bullet with (`protocol.md:1066-1067`):

> *"a client that needs the two to agree needs a per-scanline capability, which this catalog does not
> yet have."*

CR-24 is that capability. The amendment must also edit that sentence to name `emulator/scanlines` as
the reconciliation path — leaving it reading "does not yet have" beside a row that exists would be a
live prose/catalog contradiction of exactly the kind D14 files as a spec bug. (The §11.13 amendment
log's own echo of the sentence at `protocol.md:1775-1777` is a historical record and stays untouched.)

---

## What exists today

### In the contract: nothing

No §6 row is keyed by a scanline. The *VRAM / CRAM / layers* table (`protocol.md:1039-1051`) holds
nine rows; the only coordinate-shaped one is `pixel_attribution` (one winning dot, resolved from
**current** VDP state — its prose at `protocol.md:1059-1067` is explicit that it *"MUST NOT read a
framebuffer"* and may therefore legitimately disagree with the drawn picture). `emulator/screenshot`
returns a PNG file path, not bytes a gate can assert on. `run_to_scanline` (`protocol.md:788`) is a
*stop*, not a readback — and the demand doc's own assessment (Ask 1, "This repo's position") records
that what Aeon needs first is *"the readback, not the stop."*

### In the legacy MCP: nothing

`grep -in scanline /home/volence/sonic_hacks/oracle/linux-port/mcp/oracle_mcp.py` (run 2026-08-18)
matches only the `run_to_scanline` tool (`oracle_mcp.py:660-665`). No legacy tool reads a row of
pixels; unlike CR-21/22/23 there is no de-facto shape to inherit or correct. The MCP side of this CR
is therefore a **new tool row**, written against the contract row below rather than against history.

### In this engine: the entire mechanism, already running — only the bus surface is missing

This is the CR's central claim, and it was verified by recon rather than transcribed, because it
decides whether the handler is a pure read or a plumbing project. **It is a pure read of existing
engine state. Zero core changes; zero new capture plumbing.** The anchors, each read at its line:

1. **The engine already owns a live per-line capture, attached to every run.**
   `crates/oracle-aether/src/engine.rs:447` — `screen: ScanlineCapture` — constructed at `:510` as
   `ScanlineCapture::new(Retain::LastFrame)`. The field's own doc (`:445-446`): *"Attached to every
   run this engine performs, so the picture a client asks for is the one the raster actually drew."*
2. **Every advancing path rides through it.** `free_run_step` (`engine.rs:640-642`), `advance`
   (`:673-676`) and `advance_until` (`:715-718`) all build a `Fanout` with `&mut self.screen` and end
   in `latch_screen()` (`:741-746`), which copies the completed frame into `last_frame`
   (`Option<CapturedFrame>`, `:451`) and bumps `screen_generation`. In the **hosted** arrangement the
   player's run loop hands its own capture in through `publish_capture` (`:611-613`) — *"the same
   input `latch_screen` consumes, run through the same reader, so a published frame and a
   client-driven one cannot disagree about geometry."* There is no run driver in this tree whose
   frames the capture does not see.
3. **The raster-vs-state-render split is already normative on this server's own wire.**
   `Engine::framebuffer` (`engine.rs:1061-1073`) prefers the latched raster frame
   (`from_raster = true`) and falls back to a post-hoc `render_line` sweep of current VDP state only
   when no whole frame has ever completed. Its doc comment (`:1050-1055`) records the measured reason:
   the fallback renders from end-of-frame state, *"by which point a game has already rewritten CRAM
   for the next frame"* — S3K's underwater split came out *"bright red instead of slate blue"*, and
   the window hit the same bug over 6 of 17 conformance ROMs before being fixed the same way.
   `emulator/screenshot` (`:1705-1744`) already publishes the provenance:

   ```rust
   "source": if from_raster { "raster" } else { "stateRender" },
   ```
   (`engine.rs:1731`), with a `caveat` on the fallback only (`:1733-1742`), and `emulator/status`
   carries the same fact as `framebufferSource` (`:1617`).
4. **The capture holds exactly the demanded bytes.** `crates/oracle-core/src/scanline_capture.rs`:
   `on_scanline(&mut self, line: u16, rgb: &[(u8, u8, u8)])` (`:141`) receives the **live rendered
   line, S/H applied** — the demand doc's feasibility section verified the same fact against the same
   file. `lines()` (`:106`) is `&[(u16, usize)]`, per-delivery `(line, width)`; `pixels()` (`:119`)
   is the retained frame line-major, one `(r,g,b)` per pixel. Per-row width is recoverable, and H40
   (320) vs H32 (256) with it.
5. **Heterogeneous widths exist in the stream but are NOT representable in the retained frame** —
   the one recon answer that bends a pin, recorded in "Pins" below. `store_from_capture`
   (`engine.rs:3462-3491`, doc at `:3445-3461`) is the single reader both the bus and the player's
   `blit_capture` (`crates/oracle-frontend/src/main.rs:503`) use, and its doc states: *"A frame is
   not guaranteed rectangular. A game can switch H32↔H40 part-way down (S3K does exactly that on the
   first frame after a soft reset), so the width is the width the frame **ended** on — what the VDP
   is actually scanning out by V-Blank — and short lines are padded with black to reach it."* The
   retained frame is rectangular **by construction**: one width, frame-end, black-padded.

So the handler is: slice `framebuffer()`'s rows `[startLine .. startLine+count]`, spell them per D9,
attach the `source` the tuple already carries. The determinism the demand calls *"the whole
requirement"* is likewise already this core's construction (seeded machine, determinism gate) — the
demand doc's assessment says so and nothing in recon contradicts it.

---

## The proposed row and prose (verbatim, as they would land)

In §6's *VRAM / CRAM / layers* table, directly under `emulator/sprites` (`protocol.md:1051`):

```markdown
| `emulator/scanlines` | `startLine`? (0–223, def 0), `count`? (≥1; `startLine`+`count` ≤ 224, def: through line 223) | `startLine`, `mode`, `source`, `rows[]{line,width,rgb}`, `caveat`? |
```

and, as its **own blockquote block** after the `sprites` blockquote (the CR-21 R4 placement rule —
never a new bullet inside a sibling's closed enumeration):

```markdown
> **`emulator/scanlines` — the raster, row by row** *(added 2026-08-18, §11.14)*. Returns the
> rendered RGB of a row range of the most recently **completed** frame's active display — the rows
> the raster actually drew, mid-frame CRAM/scroll/shadow-highlight effects included, S/H applied —
> not a render of end-of-frame state. This is the per-scanline capability `pixel_attribution`'s
> first bullet names as the reconciliation path. Five behaviours are normative:
>
> - **The content is the retained last completed frame, and there is no frame parameter.** "As of
>   frame F" is achieved by driving the machine to F (`run_frames`, `run_to`) and reading; the
>   envelope's §2.2 stamp identifies the frame the reply describes. This is the bus's whole-frame
>   read model, not a shortcut: sub-frame addressing was expressly declined by the capability's own
>   demand side ("the deterministic frame counter is fine").
> - **`rows[].rgb` is the measurement.** A hex byte string (D9 category 1) of exactly `width` × 3
>   bytes — pixels left to right, `r`,`g`,`b` per pixel, the shadow/highlight-applied output values,
>   the same bytes the presented picture is built from. Rows are line-ascending and contiguous from
>   `startLine`.
> - **`source` names which instrument answered**, with `emulator/screenshot`'s spellings and
>   semantics: `"raster"` — the live per-line capture, the normative content; `"stateRender"` — the
>   pre-first-frame fallback, rendered from VDP state as it stands now. A stateRender reply is still
>   an answer (a machine that has never completed a frame has no raster to show) and carries
>   `caveat`; but a post-hoc render is structurally blind to mid-frame effects — the exact blindness
>   this method exists to see past — so **a gate that depends on mid-frame liveness MUST check
>   `source == "raster"`**. A row whose provenance is unstated is worse than a wrong one.
> - **A pure read**: §6's run-control state rule does not apply and a server MUST NOT refuse it on a
>   free-running machine, exactly as `read`, `pixel_attribution` and `sprites` are. The envelope's
>   `running: true` is the whole answer to a torn sample.
> - **Bounds are structural, and refused.** Active display is lines 0–223 (NTSC V28). `startLine`
>   past 223, `count` under 1, or `startLine`+`count` past 224 is `-32602` — refused, never clipped.
>   The row list takes neither a truncation flag nor a cursor: §2.4's dichotomy, structural bound →
>   neither. `mode` names the answering frame's width class — `"h40"` (320) or `"h32"` (256). A
>   frame is not guaranteed rectangular (a game can switch width mid-frame); the answer is
>   normalized to the width the frame **ended** on — what the VDP was scanning out by V-Blank —
>   short lines black-padded, so every `rows[].width` in one reply equals `mode`'s width. `width` is
>   carried per row anyway so each `rgb` is mechanically checkable (`width` × 6 hex digits) without
>   consulting `mode`.
```

## The proposed schema fragment (verbatim, as it would land)

```json
"emulator/scanlines": {
  "$comment": "protocol.md §6 (VRAM/CRAM/layers), added 2026-08-18 by §11.14 (CR-24). Row-range readback of the most recently completed frame's rendered active display — the live raster, S/H applied, mid-frame effects included — never a post-hoc state render when a completed frame exists. A pure read: a server MUST NOT refuse it on a free-running machine, exactly as read and sprites. No frame param: drive the machine to F and read; the envelope stamp is the frame identity. Bounds are structural (active lines 0-223, NTSC V28): startLine+count past 224 is -32602, refused never clipped, and the list takes neither truncated nor cursor (§2.4: structural bound -> neither). source carries screenshot's spellings ('raster'|'stateRender'); a liveness-dependent gate MUST check source=='raster' — a stateRender row passes every shape check and is structurally blind to mid-frame effects. caveat is emitted on stateRender only, declared here so its absence elsewhere is a decision (the sprites precedent).",
  "params": {
    "type": "object",
    "properties": {
      "startLine": { "type": "integer", "minimum": 0, "maximum": 223, "description": "First active-display line to return. Default 0. D9 category 2." },
      "count": { "type": "integer", "minimum": 1, "maximum": 224, "description": "Rows to return. Default: through line 223. startLine+count must not exceed 224 — refused (-32602), never clipped." }
    }
  },
  "result": {
    "allOf": [{ "$ref": "#/$defs/replyFields" }],
    "required": ["startLine", "mode", "source", "rows"],
    "properties": {
      "startLine": { "type": "integer", "minimum": 0, "maximum": 223, "description": "Echoed (or defaulted) first line." },
      "mode": { "enum": ["h40", "h32"], "description": "The answering frame's width class: h40 = 320 px, h32 = 256 px. A mid-frame width switch is normalized to the width the frame ended on, short lines black-padded." },
      "source": { "enum": ["raster", "stateRender"], "description": "Which instrument answered — screenshot's spellings. raster: the retained live per-line capture (the normative content). stateRender: the pre-first-frame fallback, rendered from current VDP state; structurally blind to mid-frame effects, so liveness gates MUST check for raster." },
      "rows": {
        "type": "array",
        "minItems": 1,
        "maxItems": 224,
        "items": {
          "type": "object",
          "required": ["line", "width", "rgb"],
          "properties": {
            "line": { "type": "integer", "minimum": 0, "maximum": 223, "description": "Active-display line number (D9 category 2)." },
            "width": { "type": "integer", "enum": [256, 320], "description": "Pixels in this row. Equals mode's width for every row of one reply; carried per row so rgb is checkable in place." },
            "rgb": { "type": "string", "pattern": "^0x([0-9A-Fa-f]{6})+$", "description": "width x 3 bytes, pixels left to right, r,g,b per pixel — the S/H-applied output values (D9 category 1). The pattern enforces whole-pixel granularity." }
          }
        },
        "description": "The requested rows, line-ascending, contiguous from startLine."
      },
      "caveat": { "type": "string", "description": "Present only when source is stateRender: no frame has completed, and the rows are rendered from VDP state as it stands now — mid-frame effects a real raster would show are NOT reproduced." }
    }
  }
},
```

A sizing note, recorded rather than discovered later: a maximal reply (224 H40 rows) is ~430 KB of
hex digits. That is structurally bounded, the demand's actual use is 2–8 rows around a boundary
(the sweep asserts rows 98–101), and §2.4 forbids both the cursor and the flag here — the number is
stated so nobody re-derives it as an objection.

---

## The demand evidence

- **The problem, in the demand side's words** (`docs/2026-08-18-aeon-scanline-readback-demand.md`,
  Ask 1): Aeon's raster layer writes CRAM mid-scanline from an HBlank handler; whether a write lands
  in blanking or in active display is a **pixel** question (*"row 99 tinted from x~170 of 320"*),
  and *"nothing else can see it: CRAM reads report the final value never the landing time;
  screenshots are press-frame non-deterministic; the replay net is pixel-blind by construction."*
  Three separate Aeon capture protocols failed their own controls; every landing measurement today
  is *"a hand ritual (pause → poke → screenshot → count pixels in a PNG)."* Their ranking:
  **"asks 1 and 2 are worth more to Aeon than stepping is."**
- **What it unblocks now**: a confirmed in-flight Aeon defect (CRAM burst landing mid-active-display)
  whose fix needs the HBlank window located in cycle space — a delay-value sweep that is ~20 manual
  screenshot analyses today and becomes an automated, permanently-protective gate with this method.
- **The acceptance fixture already exists and was read for this CR**:
  `aeon/docs/benchmarks/scanline-p2/HBLANK-WINDOW-SWEEP-SPEC.md` (`1fb982f7`) — prediction
  (clean N ∈ [15, 19], centre 17), one-poke fixture (`write_memory` at `Raster_Buf_A + 20`, under
  our own `-32005` paused-poke gate, *"enforced rather than remembered"*), classification, and §8's
  acceptance criteria A1–A5. Its A3–A5 are satisfied by this row's shape directly: row range in one
  call, rendered RGB with S/H applied (*"Field 1 alone satisfies this sweep"*), active-only 320
  columns (*"A correctly-landed write is invisible by definition, and shows up here as the clean
  signal"*).
- **The structural-blindness rationale, verbatim** (the demand doc's shape ruling, item 1, and the
  reason RGB is the normative content rather than a rendering courtesy): rendered RGB with S/H
  applied is *"not a preference"* — the defect class is a mid-scanline CRAM write, so
  **"pre-palette indices are identical either side of the landing point and are structurally blind
  to the bug"** — exactly as this repo's post-hoc frame dumps were blind to mid-frame raster
  effects. *"The boundary x in rendered RGB **is** the measurement."*

---

## Pins, each with its reasoning

1. **Name and group: `emulator/scanlines`, in §6's *VRAM / CRAM / layers* table.** It is keyed by a
   line, not by an address, so it does not belong in the memory table or in `read`'s `space` enum —
   the same shape argument that keeps `read_cram` (keyed by palette line, `protocol.md:1044`)
   outside `read`'s address spaces. Its nearest relatives (`pixel_attribution`, `sprites`,
   `screenshot`-adjacent provenance) are all in or around this group.
2. **Semantics: the retained last completed frame; no frame parameter.** The bus's read model is
   whole-frame — a client drives the machine to the frame it cares about (`run_frames`, `run_to`)
   and reads, and the §2.2 stamp on the reply is the frame identity. The demand side declined
   sub-frame addressing in as many words (*"no sub-frame addressing needed; the deterministic frame
   counter is fine"*). A `frame` parameter would imply a frame archive no engine keeps
   (`Retain::LastFrame` holds exactly one completed frame — `scanline_capture.rs:29-35`) and would
   be the D12 silent-wrong-answer trap the moment a client asked for a frame the server no longer
   had.
3. **Params `startLine` (def 0) + `count` (def: through line 223); bounds structural; `-32602`
   refused never clipped.** The bound is the core's own constant — `ACTIVE_LINES: u16 = 224`
   (`engine.rs:62`), NTSC V28; this core's region is NTSC-hardcoded, so 0–223 is a constant of the
   catalog, not a server policy. A constant bound is schema-expressible (`maximum: 223` /
   `maximum: 224`), and a violation of a schema-mechanical constraint is invalid params, `-32602` —
   the same code CR-21 pinned for `write_memory`'s payload cap ("refused, never truncated"). The
   deliberate contrast is `pixel_attribution`'s `-32004`: its refusal is coordinate-shaped and
   mode-dependent (H40 vs H32 width, carried back as `width`/`height` in `error.data` —
   `protocol.md:1073-1080`), which a static schema cannot express; a line bound of 224 has no such
   escape hatch to need. Row RANGE in one call, per the demand ruling ("assertions are always about
   a boundary; a single-row call would just be called N times") and the fixture's A3.
4. **Result `startLine`, `mode`, `source`, `rows[]{line,width,rgb}`, `caveat`?; neither `truncated`
   nor `cursor`.** §2.4(d)'s dichotomy verbatim: *"policy bound → flag it, and cursor it only where
   continuation is supported; structural bound → neither"* (`protocol.md:540-546`). 224 is the
   video hardware's bound, `pixel_attribution.candidates`' side of the dichotomy. `rgb` is a hex
   byte string per D9 category 1 (*"Addresses and byte payloads are hex strings"*,
   `protocol.md:110-112`). **`mode` is a single top-level value, not per-row — the pin that bent to
   tree-truth**: the dispatch pin allowed per-row width "if recon shows heterogeneity is
   representable", and recon showed the opposite — heterogeneity exists in the raw line stream but
   is *normalized away* by the one shared frame reader (`store_from_capture`, `engine.rs:3454-3456`:
   frame-end width, black-padded), identically for the bus, the player, and a hosted publish. A
   per-row `mode` would promise a distinction no reply can ever carry. Per-row `width` is kept
   anyway — every row's `rgb` is mechanically checkable in place (`width` × 6 hex digits), the
   §2.4 "checkable against each other rather than individually plausible" spirit — with the prose
   pinning it equal to `mode`'s width.
5. **A pure read — MUST NOT refuse on a free-running machine.** Exactly as `read`
   (`protocol.md:871-872`), `pixel_attribution` (`:1071-1073`) and `sprites` (`:1107-1109`): it
   advances nothing and mutates nothing, so §6's run-control state rule (`:793-801`) gives no
   ground to gate it, and the envelope's `running: true` plus the stamp is the contract's whole
   answer to a torn sample (D11). It is deliberately NOT added to the state rule's named list.
6. **RGB with S/H applied is the normative content; index and S/H state are registered follow-ups,
   not fields in this row.** The rationale is quoted in the demand evidence above — pre-palette
   indices are structurally blind to the defect class. The follow-ups are registered as
   **F-SCANLINE-INDEX** (per-pixel CRAM index — for attribution: *"pixels at row 99, x>170 use
   index $4A and that entry changed mid-row"*, a gate that detects *their* change, not *a* change)
   and **F-SCANLINE-SH** (per-pixel shadow/normal/highlight state — to split the palette-write op
   from the S/H-register op: Aeon shipped a recorded bug carrying the palette half without reg $0C
   bit 3, *"tinted but visibly lighter"*, invisible as a missing op in RGB alone). They are out of
   this row because they are **not** free the way field 1 is: recon confirms the renderer resolves
   indices and S/H internally and hands the sink only RGB (`on_scanline(line, &[(r,g,b)])`,
   `scanline_capture.rs:141`; the S/H-aware conversion is private — the demand doc's feasibility
   section, verified against the same file). Extending the renderer→sink interface is a core change
   with its own currency-neutrality scrutiny; landing it inside a bus CR would smuggle a core
   change under a contract row. Adding the fields later is additive, D5's direction. The demand
   side confirmed the split: *"Field 1 ALONE unblocks the sweep completely… fields 2–3 must NOT
   hold it up."*
7. **`source` is required, with `screenshot`'s spellings — and a stateRender reply is still
   answered.** The precedent is on this server's wire today (`engine.rs:1731`,
   `"source": if from_raster { "raster" } else { "stateRender" }`; `status.framebufferSource`,
   `:1617`) and the reason is the demand's own: a row whose provenance is unstated is worse than a
   wrong one, because A2 depends on knowing which instrument answered. Refusing before the first
   completed frame would make the method unusable on a freshly-reset machine for no client benefit
   (the screenshot doc's *"a post-hoc render of the reset state is better than a black rectangle"*,
   `engine.rs:1057-1059`); answering with `source: "stateRender"` + `caveat` keeps the reply honest
   instead. The prose therefore carries the MUST: **A2-based gates MUST check
   `source == "raster"`** — a stateRender row passes every shape check and fails A2, which is not a
   corner case but the exact structural blindness the demand names.
8. **The A2 acceptance condition becomes the adoption condition** — see the section below. A shape
   can be conformant and still be the post-frame-state implementation the demand explicitly warns
   against; the adoption condition is what closes that door.
9. **MCP: one NEW tool row; player GUI: no new surface — a decision, not an omission.** The
   three-surface parity rule requires the gap to be decided: the MCP gains a `scanlines` tool
   written against this row (no legacy row exists — grep-verified above); the player already
   renders every line of every frame through the very capture this method reads (`blit_capture`,
   `crates/oracle-frontend/src/main.rs:503`, consuming the same `store_from_capture` reader), so a
   GUI scanline panel would re-present what the window *is*. Recorded here per D15's discipline.
10. **The handler reads `framebuffer()`'s source — no new engine state.** Implementation placement,
    recorded so the adjudicator sees the whole design (it changes no wire shape): slice the same
    `(width, frame, from_raster)` tuple `screenshot` consumes (`engine.rs:1061-1073`), rows
    `[startLine .. startLine+count]`, spell per D9. Nothing new is captured, latched, cleared, or
    invalidated; every existing invariant (`latch_screen`'s release discipline, `invalidate_screen`
    on machine replacement, hosted `publish_capture`) covers this method for free because it owns
    no state of its own.

---

## The adoption condition

In the §11.6 / §11.8 / §11.10 / §11.11 lineage, with the third clause this capability specifically
requires. **Registered when:**

1. **a conformant reply passes the fragment closed** — happy path plus one refusal per catalogued
   bound (`startLine` past 223; `startLine`+`count` past 224; `count` under 1);
2. **A1 — determinism — holds**, quoted verbatim from the sweep spec (§8):

   > *"**A1 — Determinism.** Same ROM, same N, ≥3 runs → byte-identical rows. This is the one that
   > matters most: three prior capture protocols in Aeon failed their own controls on exactly this,
   > which is why raster landing has never been gateable here."*

3. **A2 — liveness — holds**, quoted verbatim from the same section:

   > *"**A2 — Liveness (the non-vacuity check).** **N = 0 and N = 17 MUST produce different content
   > on row 99.** A capture that reports post-frame state would show them identical, because by end
   > of frame the CRAM value is the same either way — the whole question is *when* it changed. If
   > A2 fails, the surface is structurally blind to this defect class in the same way a post-hoc
   > frame dump is, regardless of how good the pixels look."*

A post-frame-state implementation passes clause 1 in full and fails clause 3 — **that is the
point**. The sweep spec's own gloss earns A2 its permanence: *"It is a poison in the Aeon sense —
it perturbs the subject (the spin value) and requires the instrument to notice, rather than
asserting that the instrument returned something."* A2 lives in this repo's suite permanently, and
per pin 7 the harness asserts it against replies whose `source == "raster"`.

One inherited constraint for any synthetic A1/A2 fixture, from the demand doc's content trap: the
tinted CRAM entry must be one the art at the measured rows actually references (Aeon's R1 got a
null result from near-unused entries; the sweep spec pins line 2 with Camera_Y frozen at 144). A
synthetic test ROM inherits this constraint or its A1 passes while meaning nothing.

---

## Verification protocol — the disagreement discriminator (run unconditionally)

Transcribed from the demand doc's closing addition, as this CR's verification section rather than
an appendix. If the capture disagrees with the predicted clean range, two already-measured anchors
assign the fault — both observed CLEAN on oracle, both buildable by the same
`aeon/tools/raster_cost_probe.py` encoder as the sweep fixture (two extra poke-and-capture runs on
the same harness):

- the **row-119 fixture** (`reg_set` + `stream_cram`, CRAM op second) — measured 1 px spill → 0,
  boundary on the authored line;
- **R1 §7.3** (`pal_restore` alone, dispatch depth 4) — row 139 fully tinted, 140+ fully base, OFF
  edge exactly on the authored line.

The anchors **bracket** the disagreement — same handler, same burst, same window, differing only in
preamble cycles ahead of the write:

| Anchors | Sweep vs [15,19] | Verdict |
|---|---|---|
| both CLEAN | disagrees | our raster timing agrees with oracle's → **Aeon's §3 arithmetic**; they own it and re-derive |
| either DIRTY | — | the capture disagrees with a landing oracle measured clean on an untouched shape → **our raster timing or the capture's sampling point** |
| both DIRTY | — | almost certainly fixture/harness — check the **content trap** first (is the art actually sampling the tinted entry) |

**Run the two anchors unconditionally as the sweep's first two data points**, not only on
disagreement — they give the sweep two known-good calibration rows before it ventures into the
unmeasured shape.

---

## The close — fragment count, limits, and the two side-edits

**Fragment delta: 32 → 33.** Counted 2026-08-18, not transcribed: the vendored
`bus-protocol.schema.json` `methods` object holds 32 method fragments (33 keys, one of them
`$comment`) after §11.13. This CR adds one, removes none. The schema's top-level `description`
carries the count (*"32 of §6's ~60 … recounted 2026-08-18, §11.13"*) and the amendment recounts it
again.

**`initialize.limits` gains nothing.** Every bound on this method is structural (224 active lines,
widths 256/320 fixed by the video hardware). There is no policy number a client would otherwise
discover by being refused, so advertising one would misfile a hardware constant as server taste.

**The `pixel_attribution` prose edit** (`protocol.md:1066-1067`): *"needs a per-scanline
capability, which this catalog does not yet have"* → names `emulator/scanlines` as that capability.
The §11.13 log's echo of the old sentence stays — amendment logs are records, not live text.

**Two follow-ups registered**: **F-SCANLINE-INDEX** and **F-SCANLINE-SH** (pin 6, with the
attribution rationale attached there). Both are additive per D5, both require the renderer→sink
interface extension that is a core change with its own scrutiny, and neither holds up field 1 —
the demand side's own ruling.
