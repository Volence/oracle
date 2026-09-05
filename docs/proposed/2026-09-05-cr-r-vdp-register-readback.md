# CR-R, the VDP register file becomes readable, and the row that already promised it stops carrying an ellipsis

**Raised by:** oracle lane, 2026-09-05. Grounded in `docs/2026-09-05-toystory-floor-recon.md`, a live
measurement pass that answered its question without register access and recorded what that cost.
**Target:** `contract/protocol.md` §11.41 (next free). One §6 row **amended** (not added), one schema
fragment written against it, one new §8 conformance item (29, next free).
**Reviewer:** to be named by the adjudicator in the ruling itself, per the 2026-08-27 substituted-reviewer rule.

**Revisions everything below was read at.** Contract: `empyrean` **`origin/main` `33c293b`**, read as
`git show origin/main:contract/protocol.md`, never through the sibling working tree. The schema audit is
the same tree's `origin/main:docs/2026-08-22-protocol-schema-audit.md`. Server: `oracle` **`ce68501`**.
Aurora's ask is read at its own tree's working copy of
`docs/reviews/2026-08-22-oracle-instrument-gaps.md`, and is quoted rather than summarized because the
proposal partly disagrees with it.

---

## ⚑ 0. The first thing to know: this is not a new method

**`emulator/read_vdp_registers` is already a catalogued §6 row**, at `contract/protocol.md:1388`:

> `emulator/read_vdp_registers` | — | `raw[]` (per-reg hex), `decoded{hintEnabled,displayEnabled,planeANametable,h40Mode,dmaLength,…}`, `status{raw,vintPending,vblank,dmaBusy,…}`

So the contract has already agreed that VDP register readback belongs on this bus. What it has not done is
make the row **servable**, and the reason is precise and already written down. The row is one of five
deliberately unschematized §6 rows, and audit **D-20** says why and says what would fix it:

> Row: `— | raw[] (per-reg hex), decoded{...}, status{...}`. **Two literal ellipses.** The key sets of
> `decoded` and `status` are not enumerable from the document, and their member types are not stated (is
> `planeANametable` a hex address or a register-field integer? is `dmaLength` a count?). `raw[]`'s length is
> also unstated, 24 registers on a Mega Drive VDP, but the row does not say. **To unblock:** enumerate both
> objects exhaustively, with a type per key, and state `raw[]`'s length. **This is the largest of the eight
> and is worth its own change request**; a VDP register decode is the kind of surface that grows, so its
> enumeration should also say what an unknown-to-this-server field does.

This document is that change request. It is filed against the audit's own words, three weeks after they
were written, because a real investigation has now paid the cost of the row not being served.

**Why the ellipsis is a hard blocker and not a formality.** §8 item 20 closes a result against its fragment
in the harness, so a fragment that declares half a row's keys does not under-specify, it **actively refuses**
the conformant server that emits the other half, while looking complete. §2.5 reads an absent fragment
correctly as "not yet transcribed". So the row cannot be half-transcribed, and under §8 item 20 the fragment
is the **precondition** for the handler rather than its record. **The ellipsis is what makes the method
unimplementable, not merely undocumented.**

There is precedent for exactly this repair. **§11.25 is "the first amendment to remove a row from the
schema's BLOCKED set rather than add one to the catalog"**, closing audit D-27 for the three decoder rows.
This CR is the second, closing D-20. It adds no method and removes none.

---

## 1. The evidence: what a real investigation could not read

`docs/2026-09-05-toystory-floor-recon.md` measured Toy Story's perspective floor on the owner's own running
window, at two camera positions, entirely through pure reads. **It succeeded.** The honest framing is not
that the lane was blocked, because it was not. The framing is what the absence cost.

Its closing section names the registers:

> Registers this pass wanted and could not read: **$04** (plane B nametable base), **$0D** (horizontal
> scroll table base), **$10** (plane size), **$0B** (scroll modes).

Everything the pass needed was derived instead:

| the fact | how it was actually obtained | the register that states it |
|---|---|---|
| plane B nametable base | a unique tile-sequence match against VRAM, plus nametable row spacing | `$04`, one byte |
| plane width, 64 cells / 512 px | rows measured 128 bytes apart, then divided | `$10`, two bits |
| no per-line horizontal scroll fan | 68 cell-phase probes across two camera positions, showing a constant phase | `$0B`, two bits |
| whole-plane vertical scroll | VSRAM's own shape, only two entries non-zero | `$0B`, one bit |

### The near miss, which is the strongest thing in this file

A structural search of VRAM for a per-line scroll table found a candidate at `0xCC00` with exactly the right
shape: constant for the top 64 lines, then a 32-line repeating sawtooth of 2 per line on a baseline rising
38 every 32 lines. In the recon's own words, *"It looks exactly like a floor table, and it would have made a
convincing curve."*

**It was the nametable.** The smooth ramp was consecutive tile indices and the period-38 was the 38 tiles
per cell-row. It was refuted by a cell-phase test costing four probes.

> **The structural search alone would have produced a fabricated curve that fitted the data it was derived
> from. The phase test is what refuted it, and it cost four probes.**

Register `$0D` names the horizontal scroll table's base directly. **One read would have said `0xCC00` is not
it, before any curve was fitted.** This is the argument for the row, and it is not "we were blocked": it is
that a derivation which fits its own evidence is **indistinguishable from a correct one** until something
independent contradicts it, and a register read is that independent thing. The pass caught its own error.
The next one may not, and it will have the same tools.

### The contrast that makes the case specific rather than general

Audit **D-21** pairs `read_vdp_registers` with `emulator/read_vsram` and suggests the latter *"may want
retiring outright, since `emulator/read {space:"vsram"}` already covers it."* The recon settles that pairing
empirically: **it read VSRAM without difficulty** (`0x0320, 0x0320, 0x0000 …`, which is how it pinned
whole-plane vertical scroll) because `emulator/read {space: "vsram"}` is served, **and it could not read a
single register** because nothing is. Two rows the audit treated as one backlog item are not alike: one has
a served substitute that a live pass actually used, the other has no route at all.

**This CR deliberately does not touch `read_vsram`.** D-21 is a retirement decision, not a specification
decision, and folding a removal into an unblocking would make one ruling do two unrelated jobs. Recorded as
decided rather than omitted, per CR-P's rule that something found must be decided.

---

## 2. Exactly which registers, and why each

**The recon names four. A second, independent source names four, overlapping in three. The union is five,
and I recommend serving all twenty-four. The rest of this section is why that is not padding.**

| reg | what it holds | who needs it, and for what |
|---|---|---|
| **`$04`** | plane B nametable base, bits 0 to 2, shifted left 13 | recon: derived structurally, by a unique tile-sequence match. Aurora: asked for by name. |
| **`$0B`** | scroll modes: HSCR/LSCR (horizontal, per-line vs per-row vs whole-plane) and VSCR (vertical, per-column vs whole-plane) | recon: the pass's actual question. 68 phase probes and a VSRAM shape argument stand in for three bits. |
| **`$0D`** | horizontal scroll table base, bits 0 to 5, shifted left 10 (this server's own mask, `render.rs:1274`) | recon: the register that would have refuted the `0xCC00` candidate in one call. Aurora: asked for by name, because the per-line scroll table lives in VRAM and is unreachable until you know where it starts. |
| **`$10`** | plane size, two bits horizontal and two vertical | recon: derived from 128-byte row spacing. Aurora: asked for by name. |
| **`$02`** | plane **A** nametable base, bits 3 to 5, shifted left 10 | **not named by the recon, and warranted anyway.** Toy Story's floor is on plane B, so the recon only ever needed B. That is an accident of one ROM. `$02` and `$04` are the same fact for the two planes, and Aurora's harness hardcodes `const PLANE_A = 0xC000` precisely because `$02` has no route. Serving B and withholding A would be a distinction with no reason behind it. |

**No other register is claimed.** `$05` (sprite table base) is already decoded onto the wire as
`emulator/sprites.satBase`; `$07` (background colour), `$0A` (HINT counter) and `$13`/`$14`/`$15`/`$16`/`$17`
(DMA) have no asker in either source and are not argued for here.

### Why the proposal serves 24 anyway, and why that is cheaper than serving 5

The evidence names five. A five-register method is **more** contract surface, not less:

1. It needs a **selection rule**: either five named result keys, or a param naming which registers, with its
   own bounds and its own `-32602` refusal.
2. Every future asker needs an **amendment**. Two independent parties, who did not coordinate, arrived at
   overlapping-but-different sets three weeks apart. The third will differ again.
3. Under **§11.18** a published key is unwidenable. Five names published today is five names owned forever,
   each of which must keep meaning what it meant.

Twenty-four bytes is forty-eight characters on the wire, needs no selection rule, no param, no bounds
refusal, and no name the contract has to own. **The set is closed by the hardware**: a Mega Drive VDP has
exactly 24 registers, so `raw[]` has a fixed length that can never grow, which is the one property that made
the neighbouring rows schematizable. From the audit's D-20 note on `get_layer_states`' sibling reasoning:
a row is transcribable when *"the row ENUMERATES its four keys with no ellipsis"*.

---

## 3. The shape, recommended, with the alternatives and why each is rejected

### Recommendation

Amend the §6 row to:

| Method | params | result |
|---|---|---|
| `emulator/read_vdp_registers` | — | `raw[]` (24 entries, per-reg hex), `status{raw}` |

That is: **keep `raw[]` and pin its length at 24. Keep `status` as an object and reduce it to its `raw`
word. Strike `decoded{}` from the row entirely.** Both ellipses are removed by **striking**, not by
enumerating, and the fragment becomes writable with nothing invented.

**`raw[]` is an array of 24 two-hex-digit strings, index-ordered 0 to 23** (D9 category 1: byte payloads are
hex strings, which is what the row already says with "per-reg hex"). An array rather than one 48-character
string, because the access pattern is `raw[0x0B]`: a client that wants register 11 should index, not do
substring arithmetic. `emulator/read`'s single `bytes` string is the right shape for a *range* whose length
the caller chose, and the wrong shape for a *file* whose members are individually named by number.

**`status` stays an object carrying one REQUIRED key, `raw`, a four-hex-digit word.** Not a bare string,
because an object leaves room to add named bits additively later under §11.18 without a type change, and a
string does not. Today it closes with exactly one key.

### Why `decoded{}` is struck rather than enumerated

This is the load-bearing call in the proposal, and the audit anticipated it: *"a VDP register decode is the
kind of surface that grows."*

1. **§11.25 already ruled on decoded names, and its reasoning transfers directly.** It refused decoded bit
   names on every decoder row, on the ground that *"a set-bits list carries strictly less information than
   the `raw` beside it, because it cannot express a clear bit"* and that *"a client asks for the raw field by
   name and applies the bit names it already has, from the source that defines them."* A `decoded.planeBNametable` carries strictly less than `raw[0x04]`, which yields it by one shift.
2. **It is the only part of the row that requires invention.** `raw[]` needs one number, 24, fixed by the
   hardware. `status.raw` needs the chip's own status word. `decoded` needs a name and a type per derived
   quantity, chosen by us, and owned forever.
3. **The recon's four registers need no decode at all.** `$04`, `$0B`, `$0D` and `$10` are each one byte, and
   every fact wanted from them is a shift and a mask that the hardware documentation specifies.
4. **The asymmetry settles it: adding a decode later is additive under §11.18, and removing a wrong one is
   not.** If a consumer asks for `planeBNametable` by name, with a stated type, that is a small amendment. If
   we publish a decode vocabulary now and get a type wrong, we own it.

⚑ **The honest counterargument, stated because it is real.** Striking `decoded` moves the decode into every
client, and independent clients can decode differently. Two things blunt it, but not to nothing. The decode
is specified by the hardware, not by us, so clients cannot disagree the way §11.25's object flags could
(where *"two branches disagree on the spelling of the same concept"*, `in_air` versus `air`). And the shift
that turns `raw[0x04]` into a base address is three lines. **What is genuinely lost is that the contract does
not state those three lines**, so five clients write them five times and a mistake in one is invisible to the
others. I judge that cheaper than owning a growing name set, but it is a trade and the adjudicator owns it.

### Rejected: a `space` on `emulator/read`

This is the alternative the brief raised, and the contract **already forecloses it in as many words**, at
`protocol.md:1105`:

> **The enum is `watchpoint_add`'s, and gains nothing.** In particular the Z80's space is **not** a value
> here: `emulator/z80_read` keeps its own row and its own bounds. **A read enum holding a value the watch
> enum refuses would be two vocabularies wearing one name**, which is the drift sharing the enum exists to
> prevent.

So a `vdpregs` space on `read` would oblige `watchpoint_add` to take it too, which means a **watch on a VDP
register**: a strictly larger feature, with its own capture point, its own hit shape, and no asker. There is
a second, independent reason: `read`'s spaces are byte-addressed arrays with hardware address ranges (VRAM
`$FFFF`, CRAM `$7F`, VSRAM `$4F`, per the same block). The register file is **not addressable**. Its members
are selected by a 5-bit field inside a control word, never by an address, so a `space` would require
inventing an address space that no hardware defines. That is the kind of invention §8 bars.

### Rejected: folding the fields into an existing served result

Aurora's first suggested shape, *"add the plane fields to an existing served result."* Rejected on three
counts. There is no natural host: `emulator/status` is run-control, `emulator/registers` is the 68000 file
(`z80_registers` being the standing precedent that each device gets its own row rather than sharing one).
It would publish named decoded keys, which is exactly what §3's argument above declines. And it would make
every reply from the host method carry register state that most callers did not ask for, which §2.4's
advisory reasoning about caveats applies to just as well: a payload every reply carries is one nobody reads.

### Rejected: a new narrow row with named geometry fields

Aurora's second suggested shape, and its preferred one, citing `emulator/sprites.satBase` as precedent for a
decoded register surfaced as a named scalar. **This is the alternative with the strongest case and it is
still the wrong one.**

The `satBase` precedent cuts the other way on inspection. `sprites.satBase` sits on a row **whose subject is
sprites**: the register is incidental context for an answer about something else, and the row would exist
without it. A row **whose subject is the register file** is a different object, and putting geometry names on
it makes the contract the owner of a VDP decode vocabulary, on the one row where that vocabulary would look
authoritative.

More decisively: a named-field row would need **five** names on the day it shipped, from two askers who did
not coordinate and whose sets differ (the recon needs `$0B`, which Aurora's four do not include; Aurora needs
`$02`, which the recon's four do not). That is the growth the audit warned about, visible before the row
exists. And it would be a **new catalog row** while a catalogued row for the same job sits blocked, which
leaves the BLOCKED set meaning something its own stated reason does not say, the objection §11.25 raised
against splitting the decoder family.

⚑ **Aurora's stated objection to the catalogued row is removed by this proposal, not overridden by it.**
Their words: *"Do **not** ask for `read_vdp_registers` as catalogued; its `decoded{…}` is why the row is
unschematizable and the ask would stall on that."* **The `decoded{…}` is precisely what this CR strikes.**
Their own fallback condition is *"a new narrow row whose result keys are fully enumerated"*, and `raw[]` at a
fixed 24 plus `status{raw}` is fully enumerated with no ellipsis. The disagreement is narrower than it looks:
it is about whether the enumerated keys should be five decoded names or one fixed-length byte array, and
Aurora reached for names only after ruling the catalogued row unusable on a ground this CR removes.

### Adopted from Aurora's conditions, and one declined with a reason

Their four conditions, ruled on individually:

* **"Must work headless."** Adopted, and free: the handler reads `System` state and touches no window.
* **"Must not require pausing."** Adopted and made normative. It is a **pure read**: §6's run-control state
  rule does not apply and a server MUST NOT refuse it on a free-running machine, exactly as `read`,
  `sprites`, `pixel_attribution` and `scanlines` are not refused. D11's stamp is the whole answer to a torn
  sample, as it is for those four.
* **"Must survive `reload_rom` in the sense that it re-reads the machine."** Adopted, and it needs no text:
  the handler holds no cache, so there is nothing to invalidate. Worth one sentence in the prose anyway, so
  a server is not free to add one.
* **"Say whether the answer is the retained frame's or right-now's"**, with a `source` discriminant like
  `scanlines` and `screenshot` carry. ⚑ **The substance is adopted and the field is declined.** A register
  read is **always live state and never the retained frame's**, because the retained frame is pixels and
  registers are not in it, so a discriminant would be a constant. But the hazard Aurora names is real and is
  not about retention: *"a scroll register read in V-Blank has already been rewritten for the next frame."*
  That is true, and **the contract has already ruled on exactly it**, on `pixel_attribution`: *"It describes
  the VDP's state **now**, not the frame anybody has seen."* The answer is that sentence restated on this
  row, not a new field. It must be **stated** rather than left out, because after an explicit ask the absence
  of a discriminant would otherwise read as an oversight.

---

## 4. ⚑ The hazard: these registers are write-only on real hardware

**Stated plainly, because it is the one thing a consumer can build a wrong gate on.** Of the 24 VDP
registers, a real Mega Drive lets a cartridge read back **none**. They are write-only: a program writes them
through the control port and can never read them again. A client reading them over this bus is reading
**emulator-internal state that no cartridge and no real console could observe.**

That is entirely legitimate for a debugger, and this bus is a debugger (D8: *"trusted local-developer API"*).
It is not legitimate as a model of anything the hardware can expose, and the difference has to be said where
a consumer will actually meet it.

### The bisection the row hides, which is worth having

The row's two halves are **not** alike on this point:

* **`raw[]` is write-only state.** No hardware route exists, at all.
* **`status` is genuinely hardware-readable**, at `$C00004`. But reading it there **has side effects**: on
  this server `Vdp::control_read_status` (`crates/oracle-core/src/vdp.rs:1166`) clears the control-port
  pending toggle, advances the FIFO drain, and clears the sprite-overflow and collision latches, the last
  two per the Sega Genesis Software Manual. So the value is reachable in principle and **not** reachable
  without perturbing the machine.

Two normative consequences follow, and the second is the one an implementer will get wrong:

1. **A server MUST answer from the non-mutating accessors.** This server already has both:
   `Vdp::regs(&self)` (`vdp.rs:399`) and `Vdp::status_word(&self, mclk)` (`vdp.rs:510`), against the
   side-effecting `control_read_status(&mut self, …)`. **The method is a peek, not a port read.**
2. **The reply MUST NOT be read as what a status-port read would have returned**, because a real one clears
   flags this one leaves standing. A client modelling a game's own status-read behaviour must not substitute
   this method for it.

### How the proposal keeps that honest

Three mechanisms, deliberately not a `caveat`. §2.4 rules that *"a caveat every reply carries is one nobody
reads"*, and a caveat that fires on **every** reply of this method is exactly that.

1. **The fragment's own `description` string says it**, which is where the brief asked for it and where a
   client generating code from the schema will actually encounter it. Proposed text: *"The 24 VDP registers
   are write-only on real hardware: no cartridge can read them back. This method reports the emulator's
   record of what was written. It is valid for comparing an emulator against itself or against another
   emulator, and it is not a model of anything a real console can observe. `status` is the exception, being
   hardware-readable at `$C00004`, but a real read there clears flags that this method does not clear."*
2. **The §6 prose states the peek rule and the no-substitution rule** as the two normative behaviours above.
3. **A new §8 conformance item, 29, makes the peek mechanically checkable.** Proposed: *a server advertising
   `emulator/read_vdp_registers` MUST answer it without moving the machine. Calling it any number of times on
   a paused machine leaves `emulator/state_hash.combined` byte-identical, and leaves the control-port
   write-pending toggle and the sprite-overflow and collision latches exactly as they were.*

⚑ **§11.25 declined a conformance item on the ground that its five obligations were per-engine and *"not
mechanically checkable by a generic harness"*, and that *"an unverifiable conformance item is worse than
prose, because it looks like a gate."* That test is met here in the other direction: this obligation is
checkable by any harness that can call the method twice and hash the machine, with no game knowledge at all.
The item is proposed on that distinction rather than by analogy.

### The register file is already a frozen currency, which bounds the hazard precisely

`emulator/state_hash` already returns **`regs`**, a 16-hex FNV-1a over the 24 register bytes
(`crates/oracle-core/src/state_hash.rs:36,44`), and the register file is one of the four frozen currencies
(`vram`, `cram`, `vsram`, `regs`). **So this bus already publishes register state, and already lets a client
gate on it, in a form that cannot be decoded.**

That gives the honest boundary a sharp edge instead of a vague warning:

* **Comparing register state is already ruled legitimate on this bus.** It is a frozen currency, and a live
  differential against the legacy emulator is pinned in this repo's own tests
  (`crates/oracle-core/tests/oracle_differential.rs`, 24 captured bytes and
  `ORACLE_REGS_HASH = 0x40E9_6BAB_1A5B_F5BC`). What is missing is not permission, it is the ability to read
  the bytes the gate already hashes.
* **What remains illegitimate is comparing against hardware.** A gate asserting *"the game set `$0B` to X"*
  tests the emulator's record of a write and is fine. A gate claiming a **cartridge** could detect that is
  wrong, and no amount of readback makes it right.

*(One provenance wrinkle, recorded because it would otherwise mislead a later reader. `oracle_differential.rs:16`
says the bytes came from *"Oracle's `read_vdp_registers`"*, and no such method exists in `oracle-old`: measured
with `/usr/bin/grep -r` over that whole tree, zero hits, against a positive control for `read_vram` that
returns `linux-port/gui/ControlSocket.cpp` among others. The legacy has a **GUI panel** for VDP registers
(`linux-port/gui/main_gui.cpp:4089-4150`) and no socket method. **This lane had already established the same
thing and then lost track of it**: `docs/2026-08-22-peer-schema-defect-answers.md:791` says
*"`emulator/read_vdp_registers` and `emulator/read_vsram` are **absent from the legacy server too**. A
repo-wide search for either literal returns zero hits in any file type. Neither is in `Handlers()`."* The
audit agrees in its own words: *"§6:1137-1138 describe methods NO implementation has ever served."* The 24
bytes are still pinned currency; only the test comment's attribution is loose, and the most likely reading is
that they were transcribed from the GUI panel.)*

---

## 5. Vectors

Written so they can be turned into tests. **None has been run**: this lane cannot launch a windowed binary
(the owner's live window is running) and changed no code, so every vector below is a proposal, and the two
that name specific values derive them from the recon's measurements rather than from a fresh read.

### Positive

* **V1, the shape.** `emulator/read_vdp_registers` with `{}` returns `raw` of **exactly 24** entries, each a
  two-hex-digit string, index-ordered 0 to 23, and `status.raw` as a four-hex-digit word. Both keys REQUIRED.
* **V2, the join against a frozen currency, and the strongest positive available.** Decode the 24 entries of
  `raw[]` to bytes, hash them with FNV-1a in `state_hash`'s byte order, and the result equals
  `emulator/state_hash`'s `regs` on the same machine point. This ties the new read to a currency that is
  already frozen, so the method cannot drift from what the bus already hashes without a red. It has a ready
  fixture: `oracle_differential.rs`'s captured `REGS` array and `ORACLE_REGS_HASH`.
* **V3, the question the row was raised for, answered in one call.** At the Toy Story machine point the recon
  measured, `raw[0x04] & 0x07` shifted left 13 equals **`0xC000`**, the plane B nametable base the recon
  derived structurally from a unique tile-sequence match; and `raw[0x10] & 0x03` decodes to **64 cells**, the
  512-pixel plane width the recon derived from 128-byte row spacing. **The register-derived and
  structure-derived answers must agree.** This is the vector that shows the method answers the thing it was
  raised for, and it is checkable because the recon published both numbers.
* **V4, the refutation that cost four probes.** At the same machine point, `raw[0x0D] & 0x3F` shifted left 10
  is **not** `0xCC00`, and `raw[0x0B] & 0x03` decodes to whole-plane horizontal scroll, not per-line.
  Together these refute the rejected `0xCC00` candidate in one call, which the recon needed a four-probe
  cell-phase test to do. **Note that `0xCC00` is expressible in `$0D`** (`0xCC00` shifted right 10 is `0x33`,
  which fits the 6-bit field), so this vector is a real discrimination and not a range check that could not
  have failed.
* **V5, purity of a different kind.** The method is not refused on a **free-running** machine, matching
  `read`, `sprites`, `pixel_attribution` and `scanlines`, and its reply carries the D11 stamp.

### Negative, the half that can fail

* **V6, the sharpest one: the toggle is not cleared.** Arm a first control word so the control-port
  write-pending toggle is set, call `read_vdp_registers`, then send the second control word. It MUST still
  complete as a two-word command. **This catches the single most likely implementation mistake**, calling the
  `&mut self` `control_read_status` instead of the `&self` `status_word`, and that mistake is silent,
  timeline-corrupting and would make the game's next control write parse as a fresh first word. A vector set
  without this one is satisfied by a handler that quietly breaks the machine it reports on.
* **V7, the status latches are not cleared.** With sprite overflow and collision both set, call the method
  twice; both bits MUST still read set in `status.raw` on the second call. A real `$C00004` read clears them.
  This is the same rule as V6 at a second site, and it fails independently.
* **V8, the whole machine does not move.** `emulator/state_hash.combined` is byte-identical across N calls on
  a paused machine, for N greater than 1. This is §8 item 29 asserted directly and is game-agnostic.
* **V9, the params closure.** The row takes no params, so under §2.5 and §8 item 22 the fragment closes
  `params` with `unevaluatedProperties: false` and `{"reg": 4}` or `{"space": "vdp"}` is **`-32602`, refused
  before any read**. Without this a client that guesses a filter param gets the full file back and believes
  its filter worked, which is the confidently-wrong answer §4 of the contract exists to prevent.
* **V10, the result closure, which keeps the ellipsis from growing back.** A reply carrying any key beyond
  `raw` and `status` is refused by §8 item 20's harness closure, and a `status` carrying any key beyond `raw`
  is refused by its own published subschema. The row is being unblocked **by removing** an open key set; a
  vector set that does not assert the closure has not actually removed it.
* **V11, the length is pinned.** A reply whose `raw` has 23 or 25 entries is refused by the fragment's
  `minItems` and `maxItems` of 24. D-20's complaint is that *"`raw[]`'s length is also unstated"*, so a
  fragment that does not pin the length **has not unblocked the row**, it has only moved the ellipsis into a
  schema.
* **V12, no write counterpart exists.** `emulator/write_vdp_registers` answers `-32601 no such method`. This
  CR proposes a **read only**, and the standing keep-dead entry *"a register-write op"*, whose scope the
  owner ruled on 2026-08-17 as covering **register** writes and not memory writes
  (`docs/2026-08-17-aeon-switchover-gap-list.md:79-88`), is the reason. No asker has requested one. ⚑ I do
  **not** claim that entry was written with VDP registers in mind: the 2026-08-17 ruling contrasts register
  writes with memory writes in a 68000 context, so its reach here is arguable. It is cited as a reason to
  stay read-only, not as a settled prohibition.

**V6 through V12 are the half that can fail.** V1 through V5 alone are satisfied by a handler that returns
the right numbers while clearing the machine's status latches on every call.

---

## 6. What it costs us, checked rather than assumed

### The core already holds the register file in a readable form

Checked, not assumed:

* **`crates/oracle-core/src/vdp.rs:144`**: `regs: [u8; REG_COUNT]`, a private field, documented *"The 24 VDP
  registers."*
* **`crates/oracle-core/src/vdp.rs:399`**: `pub fn regs(&self) -> &[u8; REG_COUNT]`. **A public,
  non-mutating accessor already exists.**
* **`crates/oracle-core/src/vdp.rs:510`**: `pub fn status_word(&self, mclk: u64) -> u16`. Also public, also
  non-mutating, and distinct from the side-effecting `control_read_status(&mut self, …)` at `:1165`.
* `REG_COUNT` is imported from `state_hash` (`vdp.rs:14`) and is **24**, the same constant the frozen
  currency hashes, so the length in the fragment and the length in the currency cannot drift apart.

**No core change is required.** Both halves of the proposed reply are already reachable through public
`&self` accessors, and the purity the §8 item demands is available by construction rather than by discipline.

### Server-side

One handler that formats 25 numbers, one `METHODS` row, one schema fragment. The fragment moves the
BLOCKED set from **5 to 4** (`z80_registers`, `read_vsram`, `call_stack`, `log_tail` remain) and the method
fragment count up by one; both figures are derived by the gate at
`contract/schema/tests/validate_contract_schema.py`, which parses the file, and neither should be copied
from this document into a commit message. There is no new `$defs`, no `limits` key, and no `capabilities`
change: per §8 item 23, servedness is `methods` membership, and this row needs no flag of its own.

### Wire-visible change for existing clients

**None.** No served reply changes shape, no existing key changes meaning, and no client can break on a
method that answered `-32601` yesterday. The one edit to an existing artifact is the §6 row itself, and
**because the row has never been served by any implementation, in either tree, that edit cannot break a
client either**: there is nothing to be compatible with. That is unusual and it is the cheapest moment this
change will ever be available.

### Verification actually performed for this document

No behaviour was changed and no `.rs` file was touched, so no build was run and none is claimed. Every code
citation above was read at `ce68501`. The zero-hit result for `read_vdp_registers` in `oracle-old` was taken
with `/usr/bin/grep -r` against a positive control, because `grep` is a shell function on this machine and
under-reports over ignored paths; a bare empty result would not have been evidence.

---

## 7. Who else wants it

**One named consumer beyond this lane, evidenced and quoted, and it is not the one the framing expected.**

**Aurora wants it, and filed the ask.** `aurora/docs/reviews/2026-08-22-oracle-instrument-gaps.md`, section
**GN-1**, *"VDP plane geometry readback: plane bases, plane size, and the scroll registers"*, the **only**
genuinely-new item in a survey of the whole bus. It names `$02`, `$04`, `$0D` and `$10` by number and it
names its own workaround: `scratchpad/warp-tearing-harness.mjs:110-113` hardcodes
`const PLANE_A = 0xC000` with the comment *"VRAM $C000 is aeon's plane A base for this build"*, which their
own document calls *"a literal that silently becomes a different plane the day aeon moves its VRAM layout.
This is precisely what contract D7 exists to prevent, and Aurora observes D7 everywhere else. It cannot
here."*

Their second, weaker consequence is worth quoting because it is the same failure mode as the recon's near
miss: the harness's cleanliness verdict *"is a fine tearing detector (tearing is broad) and weak evidence of
cleanliness"*, because its 40x28 sample is the true view only when plane A's scroll is at the origin. Given
the plane base, the plane size and the scroll, *"the 0-of-1120 result stops being '0 in a fixed sample' and
becomes '0 in what the player sees'."* **Two independent parties arrived at overlapping register sets by
working on different problems**, which is the closest thing to demand evidence a pre-release bus produces.

⚑ **Aeon is not a consumer, and the framing this CR was written from said it was.** Aeon's active
raster-and-parallax roadmap item does not ask for register readback, because aeon **derives the same facts
from build-time symbols it owns**: sigil emits the layout constants into the `.lst`'s third `EQU` section.
Aeon knows where it put plane A because aeon put it there. The need is specifically an **external observer's**
need, which is why Aurora has it and aeon does not, and why this lane had it while measuring a **commercial**
ROM whose build it does not own. That distinction is the real shape of the demand and it should not be
blurred into "the raster project wants it".

**Two things that are not demand, listed so they are not mistaken for it.** `aeon/docs/research/phase_harness/wedge_repro.py:18`
calls `b.call("emulator/read_vdp_registers")` for `status.dma_busy` and `status.fifo_full`; it was last
touched 2026-07-02, is in no suite or gate, and is a script that assumed a catalogued method existed. It is
evidence that the `status` half has had a reader, and it is not an active ask. And **seraph has nothing**:
zero hits on this topic, against a positive control that found real `emulator/` content in that tree.

**Aurora's own priority, in their words:** *"Not urgent. Nothing Aurora has shipped is blocked. It makes an
existing measurement trustworthy and removes a hardcoded address; it does not gate a feature."* Filed
2026-08-22 and unactioned since. **This CR does not claim urgency it does not have.** It claims that the ask
is now two-sourced, that the audit already priced it as *"worth its own change request"*, and that a live
pass has since paid for its absence in a way that is documented rather than asserted.

---

## 8. What would have to be true for this to be wrong

* **If the adjudicator prefers Aurora's named-geometry shape.** It is the shape the asking consumer asked
  for, which is a real argument, and §3 above may be over-weighting a growth risk on a surface that might
  never grow past six names. If so, the right form is a `decoded` object enumerated exhaustively with a type
  per key, on the same row, and the audit's extra condition should ride with it: *"its enumeration should
  also say what an unknown-to-this-server field does."* This CR would then be adopted with `decoded`
  restored rather than rejected, and §4, §5 and §6 stand unchanged either way.
* **If the write-only nature is read as disqualifying rather than as a caveat.** The position would be that a
  debugger bus should expose only what the modelled hardware can expose, so that no gate can be built on a
  fiction. I think the frozen currency already refutes it (`state_hash.regs` hashes precisely these bytes and
  is gated on today), but it is a coherent reading of what this bus is for, and if it wins then §8 item 29 is
  not enough and the row should be struck from §6 rather than served.
* **If `raw[]` without a decode is judged to move a real cost onto clients rather than a nominal one.** The
  three-line shift is only nominal until five clients write it five times. A middle answer exists and is
  worth naming: serve `raw[]` now and let the **first** consumer that asks for a named field bring the name
  and the type with it, which is additive under §11.18 and costs nothing to defer.
* **If no one ever calls it.** Aurora's ask is three weeks old, marked not urgent, and has had no follow-up.
  If the adjudicator wants a consumer commitment before spending contract surface, that is defensible, and
  the honest counter is only that the surface is unusually cheap here: a row that already exists, a core
  accessor that already exists, and no client that can break.
