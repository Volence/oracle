# CR-25 — the row stops being atomic: sub-line CRAM landings in `emulator/scanlines`

**Status: DRAFT, unadjudicated (raised 2026-08-19).** Raised against `empyrean/contract/protocol.md`
§6 (VRAM / CRAM / layers) — specifically the `emulator/scanlines` blockquote's row-content
clarification at `protocol.md:1165-1189`, merged **this morning** as `112d683` — and lands as
amendment **§11.15**. It changes **no schema fragment**: the vendored `bus-protocol.schema.json`
`methods` object stays at **33** fragments (counted mechanically today, see "What does not change").

This CR is the vehicle the controller ruled for the F-SCANLINE-SUBLINE arc. It does **not** redesign
anything: the design is `docs/2026-08-19-subline-recon.md` (adopted as written) and the ruling is
`docs/2026-08-19-ruling-subline-recon.md` §Q1 — *"a numbered CR and a §11.15 amendment entry, not
plain prose"* (`:15-24`). Q6's one-sentence `pixel_attribution` divergence note rides this CR, per the
same ruling (`:59-63`).

> **What is unusual about this CR, said up front.** It is not the first amendment in this catalog to
> make an existing, *deliberately merged* passage of live contract prose **false**: CR-24/§11.14
> edited the standing `pixel_attribution` sentence (*"…which this catalog does not yet have"*) for
> exactly that reason, and recorded the edit rather than leave *"the live prose/catalog contradiction
> D14 files as a spec bug"* (`protocol.md:2519`). What is distinctive here is the **speed** — the
> passage this CR supersedes was merged *seven hours* (7h09m) before the recon that superseded it, by
> the same controller, in the same arc. That is not a defect in `112d683`: the clarification stated the convention the reference server
> then had, and it stated it because the demand side had just measured it. The right reading is that
> the arc moved faster than the prose, and the amendment log is where that gets recorded rather than
> quietly overwritten. §11.15's own entry says so.

**Sequencing.** Contract-first, per the 2026-08-17 owner ruling CR-21/22/23 cite and CR-24's own
sequencing note (`docs/2026-08-18-cr24-scanlines.md:22-27`). The ruling refines it for this arc
(`docs/2026-08-19-ruling-subline-recon.md:20-24`, `:65-74`): the CR is drafted and adjudicated **while
slices 1–3 (all behaviour-neutral) proceed**, and the empyrean prose correction merges **in the same
window as the arc's oracle-next merge**, so `contract/protocol.md` is never wrong on `main`. Slice 4 —
the slice that actually changes row content — lands **after** adjudication.

Every line anchor below was read in the file it names on 2026-08-19. Where an anchor cited by an
upstream document has since drifted, the drift is recorded rather than repeated (see "Anchors that
moved").

---

## 1. Why this is being raised

### The prose is about to be false, in three sentences

`protocol.md:1165-1189` is a clarification block appended to the `emulator/scanlines` normative
blockquote. Its own header calls it *"a clarification"* that *"states what the row already did …
adds no behaviour and changes no wire shape"* (`:1165-1167`). Three of its sentences stop being true
the moment the F-SCANLINE-SUBLINE arc's slice 4 ships:

| `protocol.md` | today's text | after |
|---|---|---|
| `:1167-1170` | *"A row is a sample of VDP state taken at that row's own line-start, and it is **atomic** … nothing re-samples CRAM, scroll or the mode registers part-way across a row."* | Scroll and the mode registers still don't. **CRAM does.** |
| `:1172-1177` | *"A write that lands during line N **cannot change row N** … the change first appears in row N+1."* | A CRAM write inside line N's active window changes row N **from its landing pixel onward**; the first *wholly* changed row is N+1. Non-CRAM writes keep the old rule. |
| `:1178-1181` | *"A mid-row landing is **not expressible** … this surface resolves a landing time to one scanline, never to a pixel within one."* | Expressible for CRAM, to instruction granularity. |

(That table is `docs/2026-08-19-subline-recon.md:565-581` §E.6, re-verified against
`protocol.md` line by line for this CR.)

The fourth paragraph — `:1183-1189`, *"This catalog does **not** pin the intra-line sampling point
for all servers"* — **stays true, and is why this is cheap.** The catalog deliberately left the
sampling point unpinned and told clients not to depend on the atomic reading; a client that obeyed
it is not broken by this amendment. What breaks is a client that read the *reference server's stated
convention* as a promise. That client deserves an amendment-log entry, which is exactly the ruling's
reasoning for the vehicle.

### Why the reference server is changing at all

The demand is Aeon's, and it is the same demand CR-24 answered one layer coarser. Their first
acceptance sweep against `emulator/scanlines` passed A1 (determinism) and the restated A2
(liveness) — recorded in `docs/2026-08-19-aeon-acceptance-results.md:24-70` — and then hit the wall
this CR removes:

- **`flipX` was `{0}` at all 201 sampled N** — the set of distinct flip-x values across the whole
  sweep is exactly `{0}` and `atLeft` is `True` at every N, because in their own words *"the
  partial-row landing … is not expressible here"*
  (`aeon/docs/benchmarks/scanline-p2/HBLANK-WINDOW-SWEEP-RESULTS.md:314-316`, at aeon `d113e088`;
  the block spans `:311-320`). Their own heading: *"`flipX` is 0 at every single N — that is the
  finding"* (`:311`). The shorter gloss *"because a partial row cannot exist"* is **this repo's**
  registration prose, not theirs (`docs/2026-08-19-aeon-acceptance-results.md:187`).
- **The HBlank window could be bracketed from one side only.** Their §6 procedure **measured** the
  upper edge (N = 21.5) and had to **derive** the lower one (15.21) from blanking width
  (`HBLANK-WINDOW-SWEEP-RESULTS.md:364-372`).
- Registered on this side as **F-SCANLINE-SUBLINE**, *"the gap that blocks their sweep's own
  purpose"* — *"this surface can measure a landing to ±1 scanline, which is enough to bracket a
  window from one side and not enough to close it"* (`docs/2026-08-19-aeon-acceptance-results.md:181-190`).

The owner scheduled the item; the recon found the change an order of magnitude cheaper than its
registration assumed (below); the controller adopted the design. This CR is the contract half.

### The finding that made it cheap — recorded because it is the CR's load-bearing claim

`Vdp::resolve_line` (`crates/oracle-core/src/render.rs:988-1035`) **never reads CRAM**: every input
it takes works in the **index** domain, and CRAM enters in exactly one place — `pixels_rgb`
(`render.rs:1045-1052`) mapping each resolved `PixelResolution` through `cram_rgb_state`
(`render.rs:767-777`), the **decode** stage (`docs/2026-08-19-subline-recon.md:14-44`). Therefore:

> A CRAM write landing inside line N cannot change which **index** any pixel of line N resolved to.
> It can only change the **colour** those already-decided indices decode to.

So sub-line CRAM is not "resolve the line in segments" — it is "**decode** the already-resolved line
in segments". The plane fetch, sprite pipeline, priority, S/H and the per-line sprite-latch commit do
not move in time at all. That is the whole reason this amendment can promise a sub-line answer
without re-timing anything a ROM can observe, and the controller spot-verified it firsthand
(`docs/2026-08-19-ruling-subline-recon.md:6-8`).

---

## 2. What exists today

### In the contract

- The `emulator/scanlines` row and its five-behaviour normative blockquote,
  `protocol.md:1121-1163` (CR-24, §11.14, added 2026-08-18). **Untouched by this CR.**
- The row-content clarification, `protocol.md:1165-1189` (merged `112d683`, prose-only, **no §11
  entry** — grep for `11.14` in `protocol.md` returns `:1068`, `:1121`, `:2503`, `:2519` only, i.e.
  no log entry mentions the clarification). **This CR rewrites this block.**
- `pixel_attribution`'s first normative bullet, `protocol.md:1054-1072`, whose closing clause CR-24
  already edited to name `emulator/scanlines` (`:1067-1068`). **This CR appends one sentence to it**
  (ruling Q6).
- `§11.14`, `protocol.md:2503-2597` — the last amendment entry; the file is 2597 lines, so §11.15
  appends at end of file.

### In the reference server

The line-atomic convention, with its three in-tree statements — conformance Limitation L1
(`crates/oracle-core/src/bus.rs:91-95`), the `build_cram_midframe` rustdoc
(`crates/oracle-core/src/testrom.rs:425-427`), and the suite gate's own doc comment
(`crates/oracle-aether/tests/scanlines.rs:298-300`) — all of which the arc restates in slice 4
(`docs/2026-08-19-subline-recon.md:688`). The mechanism it becomes is §A option (ii),
*eager resolve at line start + segmented decode at line close*
(`docs/2026-08-19-subline-recon.md:87-134`), adopted as designed.

### On the demand side

Their results document **already cites the current convention as settled contract**: *"Pinned in the
contract at empyrean main `112d683` (§6 blockquote)"*
(`HBLANK-WINDOW-SWEEP-RESULTS.md:218-226`). That is a second reason the amendment log matters here —
the superseded prose has already been read, quoted and built on downstream, inside a day.

---

## 3. The proposed §6 text (verbatim, as it would land)

Replacing `protocol.md:1165-1189` **in full** — same position, same blockquote, same section; the
five-behaviour normative list above it is not touched and its count is unchanged.

```markdown
> **What a row's *content* is pinned to** *(rewritten 2026-08-19 by §11.15; supersedes the
> clarification of the same name added earlier that day, which described the line-atomic convention
> this amendment changed).* **A row is one complete RGB row per line, and its content is segmented
> by mid-line CRAM landings.** The reference server resolves line N exactly once, at the scheduler
> event that opens line N — that single atomic resolution is what fixes every pixel's palette
> *index* — and then decodes those indices to RGB in segments, so a CRAM write that lands part-way
> across line N is visible from the pixel it landed on. Three consequences a client must plan for:
>
> - **A CRAM write landing inside line N's active display changes row N from its landing pixel
>   onward.** The landing pixel is `x = floor(d / p)`: `d` is the write's offset in master clocks
>   into line N's **2560-mclk** active window, and `p` is **8** mclk per pixel at H40 (320 px) or
>   **10** at H32 (256 px). Pixels `0 .. x` keep the pre-write colour. A write in the blanking
>   *before* the active window recolours the **whole** row; a write at or past `d = 2560` — the
>   trailing blanking — leaves row N untouched and first appears in row N+1. A fixture that spins on
>   the HV counter until V reads N and then writes CRAM therefore produces a **split** row N, and
>   the first **wholly** recoloured row is still N+1: "one row below the line the writing code was
>   aiming at" remains the right reading for the first full row, and the row indices are the
>   renderer's own line numbers throughout, as they always were.
> - **Only CRAM is sub-line.** Scroll, VSRAM, VRAM, the mode registers and the sprite latches are
>   sampled once at line-start and still change a row only from the following line — a row is
>   line-start state everywhere except the palette lookup. The CRAM *dot* artefact (a write painting
>   the beam position irrespective of the index a pixel resolved to) is a different mechanism and is
>   not modelled.
> - **Landing resolution is instruction-granular, not exact.** The write is stamped with the master
>   clock at the **start** of the instruction that performed it, so `x` can read early by up to one
>   instruction's worth of pixels (≈18 px for a 20-cycle `move.w` at H40), and every word of a
>   multi-word burst — a `movem`, a 68k→CRAM DMA — shares one landing pixel instead of spreading
>   across the slots it really occupies. A client measuring a landing column must read it as
>   bounded, not exact.
>
> **The wire shape does not change**: one `rows[]` entry per line, `width` × 3 bytes of `rgb` per
> row, no new field, no fragment change. What changed is what those bytes contain.
>
> This catalog still does **not** pin the intra-line sampling point for all servers, and this
> amendment does not start. A **line-atomic** server — one that resolves and decodes each row once
> at line-start and shows any mid-line write as a whole-row change on line N+1 — remains fully
> conformant, as does one whose renderer advances per pixel clock (the Exodus-derived reference
> emulator's does). The reference server is now the middle case: line-atomic for resolution,
> sub-line for the CRAM decode. Two conformant servers can therefore disagree by one row, or
> **within** a row, on where a mid-line write's boundary sits, and a client comparing them must
> expect that; what is stated here is the reference server's own convention, so a gate written
> against it knows what to assert. Pinning the sampling point normatively — and the per-pixel index
> attribution that would say *which* palette entry a pixel used (**F-SCANLINE-INDEX**) — is left to
> a future amendment.
```

### The `pixel_attribution` sentence (ruling Q6, verbatim)

Appended to the first normative bullet of the `pixel_attribution` blockquote, immediately after
CR-24's *"…which reads the drawn rows back."* (`protocol.md:1067-1068`) and before that bullet's
closing parenthetical:

```markdown
> After §11.15 the two can also disagree **within** a row and not merely between rows —
> `pixel_attribution` answers from a whole-line post-hoc resolve of current state, while an
> `emulator/scanlines` row is segmented by mid-line CRAM landings — and closing that divergence is a
> registered follow-up (**F-SCANLINE-INDEX**), not a defect in either method.
```

One sentence, as ruled (`docs/2026-08-19-ruling-subline-recon.md:59-63`: *"document only"*,
*"one sentence, folded into the Q1 CR"*). It is placed on `pixel_attribution` rather than on
`scanlines` because that bullet is where the catalog already owns the disagreement claim — it is the
paragraph §6 itself calls *"the single most misreadable property of the method"* (`:1069`), and
leaving a now-narrower disagreement described only in its older, coarser form is the same D14
prose/catalog contradiction CR-24 fixed in the other direction.

---

## 4. The proposed §11.15 entry (verbatim, as it would land)

Appended at end of `protocol.md` (currently 2597 lines; §11.14 spans `:2503-2597`).

```markdown
### 11.15 — 2026-08-19: the row stops being atomic, for CRAM only

**CR-25**, raised in `oracle-next/docs/2026-08-19-cr25-subline-convention.md` off the design in
`oracle-next/docs/2026-08-19-subline-recon.md` and the controller ruling in
`oracle-next/docs/2026-08-19-ruling-subline-recon.md`. This amendment corrects prose merged **the
same morning** (`112d683`): a clarification that stated, accurately, the line-atomic row convention
the reference server then had — written because the demand side had just measured it, and made false
within hours by the change it prompted. Nothing about that clarification was wrong when it landed;
what was wrong was to assume the convention would outlive the sweep that exposed it. The catalog
records the supersession rather than overwriting it, which is what an amendment log is for.

| Item | The defect | What this amendment changed |
|---|---|---|
| **CR-25 — sub-line CRAM landings** | The row-content clarification at `protocol.md:1165-1189` pinned three statements about the reference server: a row is an **atomic** line-start sample; *"a write that lands during line N cannot change row N"*; *"a mid-row landing is not expressible."* The last is the one that mattered to a client — Aeon's acceptance sweep found `flipX` was the constant `{0}` across all 201 sampled N — the partial-row landing their procedure exists to detect being, in their own words, *"not expressible here"* (`HBLANK-WINDOW-SWEEP-RESULTS.md:314-316`, aeon `d113e088`) — so their HBlank window could be bracketed from one side and never closed. The gap was registered as **F-SCANLINE-SUBLINE** and priced as a core renderer change; recon found it is not one, because `resolve_line` never reads CRAM — CRAM enters only at the RGB **decode** stage, so a mid-line landing re-decodes already-resolved indices and re-times nothing. | The clarification block is **rewritten**. A row remains one complete RGB row per line, but its content is **segmented by mid-line CRAM landings**: a CRAM write at offset `d` master clocks into line N's 2560-mclk active window is visible from pixel `floor(d / p)` of row N onward (`p` = 8 mclk/px at H40, 10 at H32); a pre-active-blanking write recolours the whole row; a write at or past `d = 2560` first appears in row N+1, as before. Three limits are pinned with it: **only CRAM is sub-line** (scroll, VSRAM, VRAM, mode registers and sprite latches stay line-start samples; the CRAM *dot* artefact is not modelled), landing resolution is **instruction-granular** (the write carries its instruction's start clock, so a multi-word burst shares one landing pixel), and the first **wholly** recoloured row is still N+1. `pixel_attribution`'s first bullet gains **one sentence**: it can now disagree with `emulator/scanlines` within a row as well as between rows, and closing that is **F-SCANLINE-INDEX**, not a defect. **No wire shape, no fragment, no count moves — the schema's `methods` object stays at 33.** |

**★ Nothing about conformance changed, and that is the reason this is an amendment and not a break.**
The clarification's final paragraph — *"This catalog does **not** pin the intra-line sampling point
for all servers"* — was written deliberately, and it survives verbatim in substance. A **line-atomic**
server remains fully conformant after this amendment, as does a per-pixel-clock one; the reference
server has simply moved from the first camp to a middle one (line-atomic resolution, sub-line CRAM
decode). What the amendment owes a client is not a new rule but an accurate statement of the
reference server's own convention, because a gate written against a reference server whose documented
convention is stale is worse off than one told nothing.

**★ Why a numbered CR when the prose it replaces did not need one.** `112d683` merged as prose only,
and its merge message gave the reason: *"It adds no behaviour, changes no wire shape, touches no
schema fragment … §11 logs amendments, and this is not one."* Every clause of that reasoning holds
here except the first, and the first is the one that decides. This edit **does** add behaviour to the
reference server. A client written against the stated convention — and the demand side had already
quoted it into their own results document within the day — is entitled to find the change in the log
rather than in a diff.

**★ What the wire looks like: unchanged.** No new field, no new fragment, no changed cardinality: one
`rows[]` entry per line, `width` × 3 bytes of `rgb`, `mode`/`source`/`caveat` exactly as §11.14 pinned
them. This is a **server-behaviour** amendment, not a protocol one — the bytes stay the same shape and
start telling the truth at finer resolution. A conformance suite written against §11.14's fragment
passes unchanged, before and after.

**★ What it does not carry.** Four limits ship as named follow-ups rather than as quiet
approximations: **F-SUBLINE-ACCESSMCLK** (stamp a write with the access instant inside the
instruction, not the instruction's start), **F-SUBLINE-DMASPREAD** (a DMA burst lands at one pixel
instead of smearing across the slots it occupies), **F-SUBLINE-HGRID** (the readable H counter's
uniform 422-position H40 grid disagrees with the 8-mclk pixel axis by ~33 mclk at active-end; moving
it would change `$C00008` for every ROM, which is a different currency conversation from an opt-in
capture) and **F-VCOUNT-PHASE** (the V counter increments at the line boundary here and mid-line on
hardware, so HV-polled fixtures still disagree with a per-pixel-clock oracle — now over a row's first
`x` pixels instead of a whole row). **F-CRAMDOT** gets cheaper and stays open: this amendment gives a
CRAM write an h-position, which is half of what it asks for, and deliberately not the other half.
**F-SCANLINE-INDEX** and **F-SCANLINE-SH** are likewise unmoved as contract rows, though the
implementation this amendment describes retains per-pixel index and shadow/highlight state to the
sink boundary, which makes both a sink-interface extension rather than a renderer change.

*Adoption condition, per §11.6 / §11.8 / §11.10 / §11.11 / §11.14, in CR-24's two-part structure —
suite gates executable in the reference repo, plus a demand-side acceptance protocol:* registered
when **(1)** the rewritten two-timings suite gate holds — for `build_cram_midframe(L)`, rows below
`L` uniformly colour A and rows above `L` uniformly colour B, **row `L` split** with a colour-A first
pixel, a colour-B last pixel and exactly one transition, the transition column inside a band
**derived in the test from source constants** (poll-loop cycle cost × mclk per CPU cycle ÷ mclk per
pixel) rather than copied from any measurement, and the two-ROM band swap retained — asserted against
replies whose `source == "raster"`; **(2)** the zero-mid-line-CRAM-write case stays **byte-identical**
to the pre-amendment rows — checkable by a third party without a pre-amendment binary, because those
rows are frozen inline goldens: every `scanline_goldens` pin for a ROM that makes no active-display
CRAM write stays unchanged, and only the two named `color_1536` literals (ruling Q2) plus any
per-row-justified flips (ruling Q3) may move — which is the amendment's neutrality claim and the
reason a line-atomic client sees no change where no such write exists; and **(3)** the demand side
re-runs its sweep unchanged in form, reporting **`flipX` as a measurement rather than the constant
`{0}`** (predicted ≈ 222 on their row-100 fixture — a prediction bounded by the
instruction-granularity limit, roughly [205, 225], not a pin; `0` or `319` falsifies the model) and a
**restated-A2 distinct-picture count of ≥ ~30 over N ∈ 0..57 step 1**, against 4 over N ∈ 0..57
step 3 measured before the change — a different step, and at step 1 the pre-amendment count would be
at most a few more than 4, since the surface's quantum is a whole line. Clause 3 is an acceptance
protocol, not a suite gate — the fixture and the driver are the demand side's — exactly as §11.14's
verbatim A1/A2 sweep is.
```

---

## 5. What does **not** change

Stated explicitly, because the cheapness of this CR is entirely a function of this list.

1. **No wire shape.** One `rows[]` entry per line, `width` × 3 bytes of `rgb`, line-ascending and
   contiguous from `startLine`. Segments are **internal to the emitter and never reach the sink** —
   a hard interface constraint of the adopted design, not an incidental property
   (`docs/2026-08-19-subline-recon.md:546-550`; ruling `:3-6` adopts it as one of the two hard
   implementation constraints).
2. **No schema fragment, and no count.** `contract/schema/bus-protocol.schema.json`'s `methods`
   object holds **33** fragments and contains `emulator/scanlines` — counted mechanically today by
   parsing the file, not transcribed. This CR adds none and removes none. §11.14's `mode` ↔
   `rows[].width` ↔ `rgb`-length `if`/`then` tie is unaffected: widths and lengths are unchanged.
3. **No `initialize.limits` entry.** Same reasoning §11.14 recorded: every bound here is structural.
   The new numbers (2560 mclk, 8/10 mclk per pixel) are video-hardware constants, not server policy;
   advertising one would misfile a hardware constant as taste.
4. **No change to §11.14's five normative behaviours** — retained frame with no frame parameter,
   `rows[].rgb` as the measurement, `source` with the liveness MUST, pure read, structural bounds
   refused with `-32602`. This CR edits only the clarification block beneath them.
5. **No conformance rule.** The catalog's non-pinning of the intra-line sampling point
   (`protocol.md:1183-1189`) is preserved in substance: a line-atomic server stays conformant. No
   server, existing or hypothetical, becomes non-conformant by this amendment.
6. **No non-CRAM state becomes sub-line.** Registers, VSRAM, the HSCROLL table, VRAM, sprite latches
   and shadow/highlight state remain line-start samples (decisions C-2, C-3, C-5 below).
7. **No frozen currency in the reference repo moves as a consequence of the contract text.** The
   arc's currency movement is real but lives in slices 4–5 and is named there — the two
   `color_1536` hash literals and four assertion sites in the suite gate
   (`docs/2026-08-19-subline-recon.md:684-688`); no rendered pixel enters `state_hash` or
   `export_state` (`:419-442`, spot-verified by the controller, ruling `:6-8`).

---

## 6. Pins and decisions

Each is carried from the adopted design or the ruling; none is invented here.

1. **Segmented decode, not segmented resolve.** §A option (ii) — eager resolve at line start,
   deferred segmented decode at line close — is the adopted model
   (`docs/2026-08-19-subline-recon.md:87-134`; ruling `:3-6`). Option (i) (resolve at line end) was
   rejected because it moves the per-line sprite-latch commit, which games poll, for **every** ROM
   armed or not; option (iii) (per-pixel-clock beam FSM, Exodus's architecture) was rejected on five
   grounds including the clean-room policy at `crates/oracle-core/src/vdp.rs:9-11`
   (`:69-161`). The contract text states only the *observable* convention, which is common to (ii)
   and (iii) for CRAM — a server may implement it either way.
2. **8 mclk/px (H40) and 10 mclk/px (H32); a 2560-mclk active window** — decision **B-1**
   (`:232-238`). Chosen over the readable H counter's uniform 422-position H40 grid, which is a
   ~8.104 mclk/position approximation that would put active-end at `d = 2593` and misplace the right
   edge by ~4 px. Two independent cross-checks: the demand side's own blanking arithmetic
   (`3420/7 − 320*8/7 = 122.9` cycles, `HBLANK-WINDOW-SWEEP-RESULTS.md:368`), and their measured
   landing — 253.6 cycles into line 100 ⇒ *"pixel ≈ 222 of 320"* (`:413`), which the 8-mclk mapping
   reproduces at `x = 221` where the 422-grid gives 219. The disagreement with `h_counter` is
   knowing and registered as **F-SUBLINE-HGRID**, out of arc because it would move `$C00008` for
   every ROM.
3. **`floor`, and `p` taken from the answering row's own width class** — decision **B-2**
   (`:240-243`). A mid-line H40→H32 switch would otherwise place `x` on a grid the row was not drawn
   on; this mirrors §11.14's existing rule that `mode` is the answering frame's width, not a live
   register read.
4. **Instruction-granular landing, stated as a limitation** (`:245-265`). The bus takes `now_mclk`
   by value, constructed once per CPU step, so the stamp is the *instruction's start*: `x` can read
   early by up to one instruction (≈17.5 px for a 20-cycle `move.w` at H40 — the contract text says
   "≈18 px"). Still a ~15–40× improvement on a 320-px quantum, and finer than the effect the demand
   side is measuring: their three burst words are **30.0 cycles apart, measured over 8 intervals**
   (`HBLANK-WINDOW-SWEEP-RESULTS.md:339`, `:346-349`) = 210 mclk = ~26 px, so each word gets its own
   boundary. Refinement registered as **F-SUBLINE-ACCESSMCLK**.
5. **CRAM only** — decisions **C-1** (in: every path reaching the VDP's CRAM write choke, including
   CPU data-port writes, the CD5 fill trigger, fill bodies and 68k→VDP DMA words), **C-2**
   (registers out), **C-3** (VSRAM / HSCROLL table / VRAM out — these are resolve-stage inputs, so
   including them is option (i)'s cost and blast radius), **C-4** (the CRAM **dot** artefact out,
   left to **F-CRAMDOT**), **C-5** (shadow/highlight out — resolved in the same pass as the index),
   **C-6** (one DMA burst = one boundary, registered as **F-SUBLINE-DMASPREAD**), **C-7** (Z80 CRAM
   writes are in by construction, untested) — `docs/2026-08-19-subline-recon.md:307-315`. The narrow
   scope is the demand's own scope: *"a landing inside a row, i.e. the row rendered from the CRAM
   state as it evolves across the row"* (`docs/2026-08-19-aeon-acceptance-results.md:183-185`).
6. **The `+1` survives, restated.** The clarification's headline consequence — a fixture aiming at
   line N gets its boundary at N+1 — is not withdrawn; it is narrowed to *the first **wholly**
   recoloured row*. That restatement is deliberate and load-bearing, because the same `+1` is
   asserted in three places in the reference tree (`bus.rs:91-95`, `testrom.rs:425-427`,
   `crates/oracle-aether/tests/scanlines.rs:298-300`), all of which the arc restates the same way
   (`docs/2026-08-19-subline-recon.md:536-542`).
7. **A line-atomic server stays conformant** — the catalog's non-pinning
   (`protocol.md:1183-1189`) is preserved, and the contract text now says so in words rather than
   leaving it to inference. This is the one place the new prose is *stronger* than the old: the old
   text named the per-pixel-clock case as the permitted difference, and after this amendment the
   reference server is no longer the line-atomic pole, so the line-atomic case needs naming too.
8. **`pixel_attribution`: documented, not closed** — ruling Q6 (`:59-63`). One sentence; the
   divergence is real, bounded and already half-stated by that bullet's existing text. Its in-tree
   test (`crates/oracle-aether/tests/pixel_attribution.rs:296`) ties the reply to post-hoc
   `render_line` on **both** sides and is immune; only its *meaning* narrows
   (`docs/2026-08-19-subline-recon.md:553-557`).
9. **Three surfaces, and the gap is decided.** No MCP tool changes: `emulator/scanlines`'s tool
   surface is unchanged because the wire shape is unchanged, and a tool that returns rows returns
   better rows without a signature change. No player GUI surface: §11.14 already recorded that the
   window *is* the capture rendered (`blit_capture`), so the player shows the finer content
   automatically. Recorded per D15's discipline as a decision, not an omission.

---

## 7. The adoption condition

In §11.14's two-part structure — suite gates executable in this repo, plus the demand-side
acceptance protocol — and mirroring CR-24's ruling R1 split, which exists precisely because
*"registration gated on an unexecutable clause is a condition that gets waived silently"*
(`protocol.md:2553-2554`). **Registered when:**

1. **Suite gate — the rewritten two-timings gate** (`crates/oracle-aether/tests/scanlines.rs:302`,
   `a2_two_timings_differ_and_the_boundary_moves`, rewritten in slice 4). For
   `build_cram_midframe(L)`:
   - rows `< L` uniformly colour A, rows `> L` uniformly colour B (the existing timing claim,
     unchanged in substance);
   - **row `L` is split**: first pixel A, last pixel B, exactly **one** A→B transition. This is the
     clause a line-atomic renderer cannot pass — the gate now discriminates sub-line liveness the
     way today's discriminates line liveness;
   - the transition column falls inside a **band derived in the test from source constants**
     (poll-loop cycle cost × mclk per CPU cycle ÷ mclk per pixel), with the derivation in a comment
     — **not** copied from the recon's estimate or from any single measurement. Ruling Q5
     (`docs/2026-08-19-ruling-subline-recon.md:49-57`) declines an exact column: a frame is
     128,005.71 CPU cycles, so the loop's phase relative to the line drifts frame to frame and an
     exact pin is a flake. Determinism of the column *within* one boot stays covered by A1;
   - the band/outside-band structure retained, with line `L` removed from the flat-equality list;
   - all of it asserted against replies whose `source == "raster"` (§11.14's MUST).
2. **Suite gate — neutrality.** With no CRAM write inside a row's active window, rows are
   **byte-identical** to the pre-amendment rows. This is the arc's currency-neutrality claim, made
   and tested in slice 3 in isolation from any behaviour change
   (`docs/2026-08-19-subline-recon.md:645-665`), and it is what entitles the amendment to say a
   line-atomic client sees no change where no such write exists.
3. **Acceptance protocol, not a suite gate** — the demand side re-runs `hblank_window_sweep.py`
   unchanged in form (`docs/2026-08-19-subline-recon.md:590-602`), reporting two numbers:
   - **`flipX` becomes a measurement** instead of the constant `{0}` it was at all 201 sampled N
     (`HBLANK-WINDOW-SWEEP-RESULTS.md:311-320`). Predicted **≈ 222** on their row-100 fixture — the
     §B cross-check reproduces their own independently derived figure, arriving at `x = 221` by
     floor division against their *"pixel ≈ 222 of 320"* (`:413`). **It is a prediction with a
     tolerance, not a pin**: the instruction-granular limit (decision 4 above) bounds the error at
     roughly one instruction, so an answer inside ~[205, 225] confirms the model and an answer of
     `0` or `319` falsifies it.
   - **Restated-A2 distinct pictures ≥ ~30 over N ∈ 0..57 step 1.** Each spin iteration is ~10 CPU
     cycles = 70 mclk = 8.75 px at H40, so consecutive N should be distinguishable until the landing
     walks out of active display. The pre-amendment baseline the demand side measured is **4
     distinct pictures over N ∈ 0..57 step 3** (`HBLANK-WINDOW-SWEEP-RESULTS.md:99`) — note the
     different step: at step 1 the pre-amendment count would be at most a few more, since the
     surface's quantum is a whole line. Anything near 4 at step 1 means the arc did not land.

Clause 3 is deliberately external, exactly as §11.14's verbatim A1/A2 sweep is: the fixture ROM, the
driver and the game-state ritual are the demand side's, and the same-session re-run was offered
(`docs/2026-08-19-ruling-subline-recon.md:73-74`).

---

## 8. Claims, anchors, and what could not be verified

**Verified firsthand for this CR** (read in the file named, today):

| Claim | Anchor |
|---|---|
| The clarification block spans exactly `:1165-1189`, and its three superseded sentences read as quoted | `empyrean/contract/protocol.md:1165-1189` |
| The non-pinning paragraph is `:1183-1189` and survives | same file |
| §11.14 is the last entry; the file is 2597 lines; §11.15 appends at EOF | `protocol.md:2503-2597`; `wc -l` |
| No §11 entry mentions the clarification | grep for `11.14` in `protocol.md` → `:1068`, `:1121`, `:2503`, `:2519` only |
| `112d683`'s merge message gives *"adds no behaviour"* as the prose-only rationale; `ba6ca8e` is prose-only, 26 insertions, one file | `git log -1 112d683`; `git show --stat ba6ca8e` |
| The schema `methods` object holds 33 fragments and contains `emulator/scanlines` | `contract/schema/bus-protocol.schema.json`, parsed |
| `pixel_attribution`'s first bullet, and CR-24's edit naming `emulator/scanlines` | `protocol.md:1054-1072`, `:1067-1068` |
| `flipX` = `{0}` at all 201 sampled N; the demand side's own reason is *"the partial-row landing … is not expressible here"* at `:314-316` — the *"because a partial row cannot exist"* gloss is **this repo's** prose, not theirs | `aeon/…/HBLANK-WINDOW-SWEEP-RESULTS.md:311-320` (aeon `d113e088`); `docs/2026-08-19-aeon-acceptance-results.md:187` |
| Blanking window 122.9 cycles from `3420/7 − 320*8/7`; upper edge measured 21.5, lower edge derived 15.21 | same file `:364-372` |
| Landing at 253.6 cycles into line 100, *"pixel ≈ 222 of 320"* | same file `:413` |
| 30.0 cycles per burst word over 8 intervals, three `move.w (a1)+, VDP_DATA` | same file `:339`, `:346-349` |
| Restated-A2 baseline: 4 distinct pictures over N ∈ 0..57 step 3 | same file `:99` |
| The demand side already cites `112d683` as the settled convention | same file `:218-226` |
| Adopted design, every decision cited above | `oracle-next/docs/2026-08-19-subline-recon.md` (branch merged at `5ca74df`) |
| Ruling Q1 (vehicle), Q5 (band), Q6 (one sentence), execution order | `oracle-next/docs/2026-08-19-ruling-subline-recon.md:15-24`, `:49-57`, `:59-63`, `:65-74` |

**Anchors that moved, recorded rather than repeated.** The demand-side results file has grown across
**three commits** since the acceptance-results doc cited it — `8f629a69` (475→491), `deaff54a`
(491→558), `d113e088` (558→640): that doc cites aeon `810b6d90`
(`docs/2026-08-19-aeon-acceptance-results.md:17`), and the file is now at `d113e088` and 640 lines
against 475. Every quoted claim survives, at shifted anchors — e.g. the `flipX` heading is `:295` at
`810b6d90` and `:311` today, the 122.9-cycle figure `:352` → `:368`, the ≈222 landing `:397` →
`:413`. **All line anchors in this CR are against `d113e088`, the commit read today.** The recon
doc's own citations (`:311-332`, `:368`, `:410-413`) already match `d113e088`; its `:205-207`
citation for the 30-cycles-per-word measurement does **not** — that content is at `:339`/`:346-349`
today, and this CR anchors it there.

**Claims this CR does not make.**

- It does not claim the reference server already behaves as described. It does not: slice 4 has not
  been written. The contract text is drafted to land **with** that slice, per the ruling's sequencing
  (`docs/2026-08-19-ruling-subline-recon.md:20-24`), and merging this prose before the behaviour
  would make `main` wrong in the opposite direction.
- It does not claim `x` is exact. Decision 4 bounds the error at one instruction and registers the
  refinement.
- It does not claim agreement with the GUI oracle. The V-counter phase difference
  (**F-VCOUNT-PHASE**) is untouched by this arc, so for HV-polled fixtures a whole-row disagreement
  becomes a partial-row disagreement over the row's first `x` pixels rather than disappearing
  (`docs/2026-08-19-subline-recon.md:280-298`, decision **B-3**). For the demand side's
  **HInt-dispatched** fixtures, sub-line rendering was the *only* difference, so those do converge.
- It does not claim the three TAGged corpus measurements
  (`docs/2026-08-19-subline-recon.md:795-803`). Whether `direct_color_dma`'s live hash moves, whether
  any of the 9 low-risk rows moves, and the measured split column are slice-5 questions, resolved by
  running the suite, and none is a contract question.

**Nothing is BLOCKED.** Every claim this CR must anchor was verifiable in the tree it names; the one
drift found (the demand-side file's line numbers) is recorded above with both anchor sets.

---

## 9. Verification note

**Docs-only.** In `oracle-next`, one new file under `docs/`, nothing under `crates/` — so per the
standing rule no `cargo test --workspace` run was required and **none is claimed**. In `empyrean`,
one file edited (`contract/protocol.md`) on branch `subline-amendment`, cut from `main` at
`112d683`: the §6 replacement of §3, the `pixel_attribution` sentence of §3, and the §11.15 entry of
§4 above, committed as a DRAFT pending adjudication and **not merged**. No schema artifact was
regenerated, because none changes.

**No emulator MCP tooling was used, at any point, for any purpose.** No live boot was required and
none was performed. Every measurement quoted from the demand side is theirs; every contract and
source statement is from a file read in this session at the commit named beside it.
