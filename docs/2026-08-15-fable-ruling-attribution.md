# Ruling — `emulator/pixel_attribution`, and the order the item-19 work goes in (2026-08-15)

An un-framed adjudication pass on `docs/2026-08-15-pixel-attribution-bus-method.md`, run because contract
§8 forbids a server inventing a method: *"Deviations are raised as change requests against this file, not
implemented unilaterally — the contract leads."* The precedent is `docs/2026-08-14-fable-rulings.md`, where
the same mechanism adopted six change requests and **changed two of them on the way in**.

The brief was deliberately un-framed: the artifacts, the contract, the prior CRs, both handoffs and the
wire probe, with no indication of what I wanted the answer to be, and an explicit instruction to say so if
the framing of the brief itself was wrong.

## Verdict

**Adopt, with changes.** Four conditions, all now applied to the design doc.

## What it verified rather than accepted

- Re-executed the schema fragment against the real `bus-protocol.schema.json` independently (9 → 10
  methods), confirming the whole stays valid draft-2020-12, that the method name passes D3's request
  pattern, and that the fragment accepts a sprite reply, a plane reply and a blanked-dot reply while
  rejecting all five malformed cases the design claims — plus empty `candidates`.
- Spot-checked the anchors. Of everything sampled — `pick.rs`, `main.rs`, `render.rs`, `Cargo.toml`, the
  design doc, the schema's line 271, and the eight-row / nine-method / twenty-row counts — **one** was
  wrong: `engine.rs:1449-1521` cites `restore`'s span, not `checkpoint_list`'s.
- Confirmed the violation on all three counts (no coordinate-shaped row in §6, none in the schema, none in
  `METHODS`) against a live consumer at `pick.rs:111` and `main.rs:2082`.

## The four conditions

**1. Renumber to CR-10.** `CR-9` was taken by the `press`-reason change request, committed **one minute
after** this document's first commit. A same-day race neither agent could see. Applied.

**2. ★ The reconciliation sentence was wrong, and this project's own data disproves it.** The design said a
disagreement between `screenshot` and `pixel_attribution` is a free-running artifact — *"pause first, or
read the stamp."* It is not. Attribution is a whole-frame-state read by construction, and the overnight
per-scanline work measured **6 of 17 ROMs diverging post-hoc vs live**, one of which makes *zero*
active-display writes. On any ROM whose state moved mid-frame the two disagree **paused or not**; the
reconciliation path is the per-scanline capability, not `pause`. This landed in the one paragraph the
design labels *"the single most misreadable thing about the method"* — the invalid-yardstick failure mode,
inside the warning about it. Applied.

**3. Two false provenance claims, struck before they could reach the contract's amendment log.** Both
verified from the two repos' git history rather than argued:

| claim | measured |
|---|---|
| *"the panel shipped one day after item 19 became binding"* | `pick.rs` landed **06:28**; item 19 / D15's prescriptive half landed **12:14–12:28** the same day. The panel **predates the rule by ~6 hours.** No day, and no defiance. |
| *"designed bus-first thirteen months ago"* | `docs/2026-07-01-vdp-design.md` landed 2026-07-01 — **six weeks.** |

Neither touches the violation as a *current state*, which is all the CR rests on. Both would have entered
the contract's permanent record as history. **The first of the two is inherited from
`docs/2026-08-15-handoff-capability-layer.md` §2 item 8, which says the same thing and is also wrong** —
corrected there too. Also applied: the five-vs-four violation count, which the intro and the sweep table
disagreed about.

**4. Sequencing: validator → this CR → the watchpoint surface.** Landing an eleventh method before the §8
item 15 instrument exists repeats exactly the pattern §11.2 documents, and the design's own test 9 assumes
the validator. The handler must emit **exactly** the schematized keys — no surplus of the kind the wire
probe's F4 found on ten existing methods.

## Where it disagreed with the design, and with me

**The "fix the watchpoint log first" recommendation is half-sound and was not adopted.** Right that
violation D is larger in scope — three capabilities the catalogued `watchpoint_add` row structurally cannot
express. But the design cited the handoff as having *"independently ranked it 4th on executed usage"*, and
the handoff actually calls it *"the most-**requested** missing instrument"* — request evidence, which is the
exact yardstick the same handoff quantified as confidently wrong (201 of 225 mentions proposed, never
executed). On executed evidence A and D are peers: each has one in-tree panel and no remote client. On
readiness A is finished and D carries an unsettled shape question. **Do not invert 2 and 3** — blocking a
finished, executable-validated CR behind an undrafted design-laden one buys nothing, which the design's own
§5.4 already conceded.

It also declined to defer on the ground that the ranked capability-collapse plan (37 methods → ~11) might
subsume this: that redesign is itself unexecuted intent, and a coordinate-shaped semantic query would not
fall under `read{space, addr, len}` anyway. Refusing a finished row pending an undrafted one would be
ranking by how good an argument sounds.

## Directed next

**The watchpoint surface is the next CR pair to draft** — a hits-reading method, plus a `space` parameter on
`watchpoint_add` — with the *"measure value changes, not write counts"* shape ruling settled **in that
pass**, not inherited by default.

## Kept as drafted

One method not two; no pause gate; `-32004` with `width`/`height` in `error.data` *and* on success; `tile`
as a JSON number with `tileAddr` as hex (D9 category 4's test applies inverted — clients demonstrably
compute on tile ids); all seven fields kept; `candidates` uncursored at `maxItems: 4`; the panel keeps
calling core with a parity test as the drift guard; `sprite_tile_at` moves to `oracle-core`.
