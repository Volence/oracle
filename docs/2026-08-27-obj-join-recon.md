# OBJ-JOIN — settling "which game object is that sprite?", on paper

**Paper recon.** Branch `recon/obj-join`, off `main` at `6568ca0`. **No `cargo` command was run** (a peer agent
holds this repo's cargo lane), **no emulator MCP tool was touched**, no socket was dialled, and **nothing was
written, committed or branched in `/home/volence/sonic_hacks/aeon`** — that tree was read only.

Predecessor: `docs/2026-08-27-gui-layers.md` **§D**, which returned this feature BLOCKED on two grounds. This
document settles ground 1 and re-prices ground 2. Where §D or the dispatching brief is wrong, it is corrected
here with evidence — five material corrections, listed in §5.

Revisions read: oracle worktree at `6568ca0`; aeon `master` at `353aaa49` (read-only).

**aeon moved under this recon** — it advanced to `f898ca2` while I was writing, which is the perishability
`docs/2026-08-27-gui-layers.md`'s own corrections section warns about. I re-checked:
`git diff 353aaa49 f898ca2 -- engine/objects/sprites.emp engine/objects/sst.emp engine/ram.emp
engine/objects/rings.emp` is **empty**, so every line number cited below — including every one in the sendable
ask in §3 — is still correct at aeon's current HEAD. Anyone sending §3 later should re-run that diff first.

---

## VERDICT (Q1), in one sentence

**No non-heuristic derivation exists: aeon's sprite builder writes the SAT and three scalars and leaves no
ownership record of any kind — no per-object SAT index, no SAT→object side table, and no positional
correspondence that survives its own skip paths — so the SAT-index → object-slot relation is not recorded
anywhere in the running machine, and the only thing that could recover it is a version-locked
re-implementation of ~350 lines of engine code that cannot detect its own divergence.**

**Recommendation (Q2): ship (a) — no object name — but make the refusal *informative* rather than silent, and
file the engine ask.** Reject (b) outright; §2 gives the killing argument, which is not the one §D anticipated:
**rings are the most-clicked sprite class in a Sonic act and rings are not objects at all.**

**Q3 is where the feature actually lives.** One `[u16; 80]` array under `DEBUG`, written once per emitted
piece from a register the emit loop already documents as free, turns the whole thing from a guess into an
array index — *and* carries its own consistency check. Sendable text in §3.

**Q4: the crate split is priced (§4) and is NOT on the critical path.** The Q2 recommendation needs no object
decode, so the split is earned only once the engine ask lands. Its two headline facts: the JSON/RPC skin is
**42 of 771 lines** and the move adds **zero dependencies** to `oracle-core` — and §D's `--no-default-features`
asymmetry **does not exist** (§5.3).

---

## 1. Q1 — what the engine records, and what it does not

### 1.1 The mechanism, read firsthand

`aeon/engine/objects/sprites.emp` (780 lines) is the whole subsystem. Two phases per frame:

**Phase 1 — registration.** Each object's own routine tail-calls `Draw_Sprite` (`sprites.emp:56-170`). It culls
against the current mapping frame's ROM bbox, then files **the object's own SST address, as a RAM word**, into
a priority bucket:

```
        lea     Sprite_Bands, a1
        move.w  a0, (a1,d2.w)         // store SST address (RAM = .w addressable)
        lea     Sprite_Band_Counts, a1
        addq.b  #1, (a1,d0.w)
```
(`sprites.emp:157-162`.) Band index is `render_flags >> 5` (`:125-127`). A full band **cascades downward**
rather than dropping (`:136-145`), so the band an object is filed under is not necessarily the band its
`render_flags` name.

**Phase 2 — the SAT build.** `Render_Sprites` (`sprites.emp:198-524`) walks bands **7 → 0**, and within each
band **alternates direction every frame** for flicker fairness:

```
        btst    #0, Sprite_Cycle_Counter+1    // odd byte = frame parity
        bne   .reverse_band
        move.w  #2, -(sp)                     // even frame: forward step
```
(`:247-255`.) `d5` is the running SAT index; it is written into each entry's **link byte** as it goes
(`size_link`, `sprites.emp:587-592`), and at the end it becomes `Sprites_Rendered` and patches the DMA length
(`:484-491`).

### 1.2 The three things the builder leaves behind — and none of them is an ownership record

At `sprites.emp:484-491` and `:494-513`, the *only* state written outside the SAT bytes themselves is:
`Sprites_Rendered` (a count), `Sprite_Table_Dirty` (a flag), and the patched DMA length. That is all.

I confirmed the absence two ways. My own read of every write site in `Render_Sprites`; and an exhaustive grep
by the recon agent over `engine/` + `games/` for `sat_owner|sprite_owner|sat_index|sprite_index|sprite_slot|
first_sprite|sat_slot|owner_table`, which returns **zero code hits**. The one thing that *looks* like a
back-reference — the `if DEBUG == 1` chain walk at `sprites.emp:463-476` — re-reads the link bytes to assert
the chain length equals `d5`, then discards.

**`sprite_piece_count` ($25) is not the field the brief hoped for, and it is worse than "a count, not a
range".** `aeon/engine/objects/sst.emp:59` calls it *"current frame's piece count (overflow prediction)"*. It
is written in exactly three places, all of them "recompute the current animation frame's piece count"
(`engine/objects/frames.emp:66`, spliced into `animate.emp` and `children.emp`; plus the spawn seed at
`load_object.emp:83`). It is read in exactly one place — the overflow pre-check at `sprites.emp:276`. So it is
a **prediction made before the walk**, not a record of what was emitted; an object skipped by any of the four
cut-offs below still carries a nonzero value.

### 1.3 Why positional correspondence dies — six independent breaks

Even granting a reader all of `Sprite_Bands`, `Sprite_Band_Counts`, `Sprite_Cycle_Counter` and every object's
`sprite_piece_count`, the mapping from band-list position to SAT index is broken by:

| # | break | where | why it is not replayable from an end-of-frame snapshot |
|---|---|---|---|
| 1 | per-object overflow skip (`d5 + count > 80` ⇒ skip whole object, **walk continues**) | `sprites.emp:270-279` | a later *smaller* object still slots in ahead of a skipped bigger one |
| 2 | outer cap ⇒ abandon all remaining bands **and `DrawRings`** | `:259-260`, `:515-521` | truncation point depends on 1/3/4 |
| 3 | **mid-object** truncation inside the unrolled piece loop (`cmpi.b #MAX_VDP_SPRITES,d5 / dbeq`) | `emit_piece_loop`, `:673-684` | an object emits *some* of its pieces |
| 4 | soft per-32-scanline budget skip | `:326-350` | keys on `Scanline_Band_Sprites` **as it stood when that object was visited**; only the final value survives |
| 5 | multi-sprite children emit via the parent's sibling walk, indexed by the **parent's** `mapping_frame` | `:364-428`, `:377-389` | the child's own cached `$25` is the wrong number; requires re-reading ROM mapping data |
| 6 | mask sprites and **ring sprites** consume SAT entries owned by no object | `:435-442`, `:449`; `rings.emp:165` | there is no object slot to name |

Break 4 is the decisive one for "is a replay sound?": it is a function of mid-loop mutable state that the
end-of-frame snapshot has overwritten. A replay must re-simulate the whole walk including camera bias and ROM
mapping reads to reproduce it.

### 1.4 The thing §D got wrong, and why it does not change the verdict

§D says recovering the join "means replaying the engine's own sprite-building order … which is game code we do
not model", implying replay is infeasible. **It is feasible.** Every input is in RAM and every one of them is
in the listing — I confirmed all four resolve in `aeon/s4.debug.lst:5257-5260`:

```
 Sprite_Bands : FFFFA3EA C |
 Sprite_Band_Counts : FFFFA5EA C |
 Sprites_Rendered : FFFFA5F2 C |
 Sprite_Cycle_Counter : FFFFA5F4 C |
```

(Note they sit at *different* addresses in `s4.lst` — `$FFFFA35C` etc. — because the debug build carries more
RAM. Any consumer must resolve them from the loaded listing, never hardcode. `decoders.rs` already works this
way by construction; see `decoders.rs:367-496`, where every quantity is a symbol difference.)

So the honest statement is stronger and more specific than §D's: **a byte-exact reconstruction is buildable,
and we should not build it.** It would be a second implementation of `Render_Sprites` + `Emit_ObjectPieces` +
`DrawRings` + `InsertSpriteMasks` living in Rust, version-locked to one engine build, re-deriving ROM mapping
data, and reproducing six branch conditions including one (break 4) that depends on state the engine has since
overwritten. When it diverges — and an engine under active development will diverge — it produces a confident
wrong object name **and has no signal that it did**. That is the same failure class as the guessed answer,
arrived at by more expensive means.

**Verdict: no derivation. Not "we haven't found it" — we read every write site and it is not there.**

---

## 2. Q2 — what to ship

### 2.1 Reject (b), the labelled nearest-object inference

Three grounds, in descending order of how badly each one hurts.

**(i) The most-clicked sprite in a Sonic act is a ring, and rings are not objects.** `DrawRings`
(`aeon/engine/objects/rings.emp:165`) is called from inside `Render_Sprites` (`sprites.emp:449`) and emits one
SAT entry per visible ring straight from `Ring_Buffer` — a flat buffer, *not* the SST pool. So for the single
most numerous sprite class on screen, **every possible nearest-object answer is wrong**, and there is no
signal distinguishing "nearest, and right" from "nearest, and the thing you clicked has no object at all". The
same holds for the X=0 mask sprites (`sprites.emp:435-442`) and the empty-table hidden terminator (`:494-513`).
§D's ground-1 argument was about *precision*; this is about **the answer not existing for the common case**.

**(ii) The cheap version measures the wrong box.** An object's `x_pos`/`y_pos` is its **origin**; the clicked
sprite is one *piece*, offset from that origin by the mapping frame's piece offsets. And `width_pixels`/
`height_pixels` ($16/$17) are **collision** dims — `sst.emp:45-46` says so verbatim — not display extent. The
display extent lives in the ROM frame header's `FRAME_BBOX_*` bytes. So a bounding-box variant needs ROM
mapping reads before it is even *approximately* right, and the naive field-based version is wrong about which
rectangle it is testing.

**(iii) It has no "I don't know" state.** There is always a nearest object. The label is therefore the only
thing between the reader and a name, and a label is read once while the name is what gets carried away and
repeated. This lane's bar — *loud-on-unmeasurable beats a plausible answer* — is precisely a rule against
shipping an answer whose only defence is a modifier.

### 2.2 Ship (a), with the refusal made informative

Keep the sentence's shape and add one clause that states the absence **and its reason**, so a reader stops
looking rather than assuming the feature is merely unfinished. Concretely, extend `pick.rs`'s sprite branch
(`crates/oracle-frontend/src/pick.rs:245-262`) so the terminal line reads:

> `That dot is sprite 12, drawn from tile $1A3 — which game object drew it is not recorded: this engine's`
> `sprite builder emits SAT entries without an ownership record, and rings and mask sprites are emitted`
> `outside the object list entirely. sprite 12 at (144,96) 2x2 cells, base $1A0, pal 1 — SAT entry @ VRAM`
> `$B060-$B067`

How a reader tells this apart from a derived answer: **it names no object.** There is no slot number, no
`code` value, no name — the sentence's subject stays *sprite 12*, which is the hardware's own name for the
thing and is derived. That is the same discipline `TILE_SPACE` already enforces one level down
(`pick.rs:139-150`: *name the space we do own, and only that*).

**Cost: a string. No crate split, no symbol read, no RAM read, no new plumbing, and it works identically in
both frontend builds.** This is why §4's split is not on the critical path.

### 2.3 The third option I considered and am not recommending

A **candidate-set** answer — read `Sprite_Bands`/`Sprite_Band_Counts` by symbol and say *"the engine queued 23
objects to draw this frame; which one drew this sprite is not recorded"* — is fully derived and refuses to
pick. I am not recommending it because the number is not actionable (a reader cannot narrow 23 to 1 with it),
it needs the whole symbol→RAM plumbing that the real answer needs anyway, and it would land the plumbing
against a throwaway consumer. **Hold it as the fallback if and only if the §3 ask is declined**; then the
plumbing has a permanent job and the sentence can at least be quantitative.

---

## 3. Q3 — the ask, in sendable form

> ⚑ **AMENDED 2026-08-27 AFTER aeon ANSWERED — DO NOT SEND THE CURSOR VERSION BELOW AS WRITTEN.**
> The ask was **accepted in principle**; the register allocation carried a defect that aeon found, and
> their proposed fix carried one of its own. Both confirmed firsthand here in their tree at `97d3ca69`.
>
> **(1) The `(a6)+` cursor desynchronises at the first ring.** `size_link` is a genuine choke point *for
> object pieces* — one call site, `sprites.emp:678` — but it is **not the only writer of the SAT stream**.
> `rings.emp:231-240` writes SAT entries on its own `(a4)+` run and **never calls `size_link`** (`grep
> size_link engine/objects/rings.emp` → zero hits). A post-increment owner cursor advances only for object
> pieces while `a4` advances for rings too, so it falls one entry behind at the first ring and stays behind
> cumulatively. **This is worse than a hole and our own consistency check cannot catch it:** the SAT is
> correct, so `Sprite_Table_Buffer[i]` still matches VRAM — only the ownership array is skewed, so the
> proof survives intact and certifies a **wrong** object. That is the same undetectable-divergence disease
> §2 used to disqualify the replay, arriving from the other direction. It was outside our six break paths
> because all six concern the object walk and this one is outside it.
>
> **(2) The fix is an INDEXED write, not a cursor** — inherently in lockstep with the SAT index whichever
> path emitted the entry. **But `(a6,d5.w*2)` as first proposed is not a 68000 addressing mode**: scale
> factors on the indexed mode are 68020+. Corroborated in aeon's own tree — no scaled index exists anywhere
> in `engine/`, while unscaled `(An,Dn.w)` is used throughout (`raster.emp:1505,1508,1670,1870`). Correct
> form, doubling into a scratch register:
> ```
> move.w  d5, d0
> add.w   d0, d0            // ×2 — no scale factor on 68000
> move.w  a1, (a6,d0.w)
> ```
> **`d0` specifically, not `d1`:** `Emit_ObjectPieces` declares `clobbers(d0-d1/d4/a0/a3)` (`:709`) so both
> are free by contract, but `size_link`'s unflipped branch is holding `size<<8|pad` in `d1` at the
> insertion point (`:588-591`). `InsertSpriteMasks` also declares `clobbers(d0-d1)`.
>
> ⚑ **`d0`'s freedom is a LIVE-RANGE fact, not a procedure-wide one — and this seat's first stated reason
> for it was WRONG.** The claim relayed to aeon was *"`d0` is the flip variant, dead once the four-variant
> dispatch has branched."* Half right and misleading: `d0` is dead **as the parameter** (the four variants
> are `emit_piece_loop(xflip, yflip)` with **comptime** ints, so flip is resolved at assembly time and
> never re-read from `d0`; each variant ends in `rts` at `:683`, no fall-through) — **but `d0` is then
> reused as scratch throughout the loop body**: `y_term` builds the Y word in it (`:566-568`) and
> `tile_term` builds the tile-attrs word in it (`:601-604`, and the other three variants likewise).
> **What actually makes the insert safe is the local dataflow.** Loop order is `y_term` → `size_link` →
> `tile_term` → `x_term` (`:675-679`); `y_term` **ends** by consuming `d0` into the SAT, `tile_term`
> **begins** by reloading it from `(a3)+`. So `d0`'s live range does not span `size_link`, and the insert
> sits in a genuine dead window — in all four variants.
> **Therefore the register choice is bound to the PLACEMENT, not to the procedure.** Relocating the owner
> write into `tile_term`, `x_term`, or past `tile_term`'s load makes `d0` a live-range collision that
> **assembles cleanly and corrupts the Y or tile-attrs word** — the same shape as the `d1` defect, which
> aeon correctly named as the dangerous kind precisely because the illegal addressing mode would have been
> refused loudly while this one would not.
> **Flags are safe, checked in the same pass:** `cmpi.b #MAX_VDP_SPRITES,d5` sits after `x_term` and
> immediately before `dbeq` (`:680-681`), so it sets the condition codes the loop exit depends on *after*
> the insert; neither `size_link` branch reads flags in between.
> **Method note:** twice in one exchange a correct answer arrived with a wrong reason attached, and both
> times the wrong reason was the *more general* one — "dead after dispatch" instead of "dead in this
> window", "infeasible" instead of "undetectably divergent". **An over-general reason is the failure mode
> to watch here: it licenses moves the narrow reason forbids.**
>
> **(3) Placement: at index `d5`, BEFORE the `addq.w #1,d5`.** All three writers agree — `size_link` yflip
> (`:582-584`), `size_link` unflipped (`:588-590`), and the ring path (`rings.emp:233-234`) each increment
> `d5` then stamp it as the link, so link = next index and **pre-increment `d5` = the current entry's
> index on every path**. That agreement is what makes one indexed write correct across all of them.
>
> **(4) Price, restated honestly: ~22 cycles/piece, worst case ≈2,000 cycles ≈1.6% of a DEBUG frame** —
> roughly **double** the 8-cycle/≈1,200 figure below, which was priced against an instruction that does not
> exist on this CPU. Zero in release, unchanged.
>
> **(5) Staleness — RULED HERE, at aeon's invitation, since this lane is the only consumer: take the clear,
> and clear the WHOLE array at `Render_Sprites` entry.** We enforce `index < Sprites_Rendered` reader-side
> as well, which is free and ours. **The clear is not about the bound.** Every in-range index is written
> this frame *only if every emit path remembers to write an owner* — and a future third path that forgets
> is precisely the class aeon just caught. A cleared array turns that mistake into a visible `$0000` and an
> honest "unknown"; an uncleared one leaves a stale **valid** address and names the wrong object. Full-array
> rather than `[0..Sprites_Rendered)` because that bound at frame start is last frame's value, and the
> correctness of the clear should not depend on a stale number. ~200 cycles, DEBUG only.
>
> **(6) `DEBUG`-only confirmed by aeon and not to be widened.** Zero release cost is what made it cheap to
> accept. Sequencing: queued behind their parallax byte-mover; they will signal when it lands.
>
> **Method note worth keeping:** the ask was checked by its recipient against the one step it did not cite,
> and that is where the defect was. **A choke-point claim is only as strong as the enumeration of writers
> that reaches it** — we proved `size_link` had one caller and never asked who else writes the stream.

Send as-is. Priced against `sprites.emp` at aeon `353aaa49`.

---

> **To: the aeon lane — one debug-only field would turn a blocked oracle feature into an array index**
>
> **The feature.** Clicking a dot in oracle's player window resolves it to a hardware sprite (a SAT index).
> We want it to also say *"that is the object at slot 27, `Obj_Motobug`"*. We have the SAT index and we decode
> your SST pool already (`emulator/object_list`). **We cannot join them**, and we established why by reading
> your source rather than guessing: `Render_Sprites` writes the SAT bytes plus `Sprites_Rendered` /
> `Sprite_Table_Dirty` / the DMA length, and nothing else. `sprite_piece_count` ($25) is a *pre-walk
> prediction*, not an emitted count, and six independent paths break positional correspondence anyway — the
> per-object overflow skip that continues the walk (`sprites.emp:270-279`), the mid-object `dbeq` truncation
> in `emit_piece_loop` (`:673-684`), the soft scanline budget whose state is overwritten by the end of the
> frame (`:326-350`), the sibling walk's parent-indexed child frames (`:377-389`), and the mask/ring entries
> that belong to no object at all (`:435-442`, `:449`).
>
> We are **not** going to re-implement your builder in Rust to recover it. A second implementation of that
> walk would be version-locked to one build, would re-derive your ROM mapping data, and — the disqualifying
> part — would produce a *confident wrong object name* on divergence with no signal that it had diverged.
>
> **The ask: an owner array, `DEBUG`-only.**
>
> ```
> // ram.emp, beside Sprites_Rendered — DEBUG builds only
> Sprite_Owner: [u16; MAX_VDP_SPRITES],   // 160 bytes; entry i = SST address word
>                                          // that emitted SAT entry i.
>                                          // $0000 = no object, $0001 = ring, $0002 = mask sprite
> ```
>
> Written **once per emitted piece**, at the one place every piece passes through — `size_link`
> (`sprites.emp:578-593`), alongside the `addq.w #1, d5` that already stamps the link byte.
>
> **A candidate register allocation, yours to change.** `Emit_ObjectPieces` documents `a1` and `a6` as
> *preserved* and untouched by the H2 stream-order restructure (`sprites.emp:671`, `:707`). So:
> `lea Sprite_Owner, a6` once at `Render_Sprites` entry; `movea.w <current SST>, a1` at each of the three
> `jbsr Emit_ObjectPieces` sites — single `:361`, parent `:368`, child `:420`; `move.w a1, (a6)+` inside
> `size_link`.
> `InsertSpriteMasks` and `DrawRings` write their sentinel the same way.
>
> **Price.**
> * **RAM:** 160 bytes, DEBUG builds only. Release: **zero**.
> * **Cycles:** `move.w a1,(a6)+` = 8 cycles per emitted piece; ≤80 pieces ⇒ **≤640 cycles/frame**, plus one
>   `movea.w` per object (~4 cyc × ≤66). Worst case **≈1,200 cycles**, ≈0.9% of an NTSC frame's ~127,800 —
>   and only in DEBUG. Release: **zero**. The `if DEBUG == 1` idiom is already in this exact file
>   (`sprites.emp:284-291`, `:463-476`), and the `comptime fn … -> Code` splices take a `DEBUG` branch as
>   naturally as they take the flip branches.
> * **Symbol:** it appears in `s4.debug.lst` and not in `s4.lst`, which is the correct shape for us — our
>   decoders answer "symbol resolves ⇒ decode; symbol absent ⇒ say so, never guess", and the player already
>   runs `s4.debug.bin` for exactly this class of work.
>
> **What it buys.** The join becomes `Sprite_Owner[sat_index]` — one word read. It is correct through every
> one of the six breaks above **by construction**, because it records what happened rather than predicting it;
> it needs no ROM mapping reads; and it survives any future change to your walk order, cap policy or budget
> without us touching a line.
>
> **The part we like most: it checks itself.** `Sprite_Table_Buffer` is the RAM source of the SAT DMA
> (`ram.emp:325`). Our picker resolves against the **VRAM** SAT, which is the *last shipped* frame, while the
> owner array describes the *last built* one — a ≤1-frame skew if a click lands between `Render_Sprites` and
> the VBlank enqueue (`buffers.emp:454-463`). We can close that ourselves with no help from you: compare the
> 8 bytes of `Sprite_Table_Buffer[i]` against the VRAM SAT entry we clicked. Match ⇒ the owner word describes
> *this* entry, answer it. Mismatch ⇒ *"the sprite table has been rebuilt since this frame was drawn"* and
> refuse. So the join ships with its own proof and a loud failure mode, which is the only shape we were
> willing to build.
>
> **Alternative we considered and rejected**, in case it looks cheaper from your side: a per-object
> `first_sprite_index` byte in the SST. It is 1 byte instead of 160, but it costs us an O(slots) scan, it goes
> stale for every object that was queued and then skipped (it would falsely claim a range), and it cannot
> express the mask/ring entries. The array is the right direction because it is indexed by the thing we
> actually hold.
>
> **Not urgent.** Declining is a fine answer; we then ship *"which game object drew this is not recorded"* and
> the feature stays closed. We are asking because one write in your inner loop is cheaper than anything we can
> do alone, and because this is the arrangement — you name gaps, we build instruments.

---

## 4. Q4 — pricing the crate split (parked, not on the critical path)

**Where the seam goes: `oracle-core::objects` takes everything that is not JSON; `oracle-aether` keeps the
skin. No second copy of anything.**

### 4.1 The measurement

`crates/oracle-aether/src/decoders.rs` is **771 lines** (§D's figure, confirmed), of which ~57 are
`#[cfg(test)]`. Lines that mention `RpcError`, `json!`, `Value`, `Map::` or `crate::hex`: **42**. That is the
whole JSON/RPC surface — **~6% of the file.** The rest is byte arithmetic over a `SymbolTable`.

| stays in `oracle-aether` (the skin) | moves to `oracle-core::objects` |
|---|---|
| `ObjectLayout::to_json` (`:287-317`) | `Repr`, `FieldSpec`, `f()` (`:42-79`) |
| `DecodedRecord::to_json` (`:628-661`) | `AEON_SST_FIELDS` (`:94-133`) |
| `field_value`'s `Value` mapping (`:683-689`) | `EngineLayout` + `AEON_SST` (`:136-200`) |
| `hex_of` / `hex::addr` spellings (`:692-700`) | `Pool`, `ObjectLayout`, accessors (`:206-266`) |
| one `From<LayoutError> for RpcError` (**new**, ~30 lines) | `derive`, `derive_pools` (`:367-546`) |
| | `be`, `signed` (`:570-588`) |
| | `DecodedRecord` + `active` + `code_target` (`:590-627`, `:662-681`) |
| | `resolve_fields`, `required_symbols`, `slot_role` (`:318-365`, `:707-714`) |

**≈575 lines move; ≈100 lines of skin remain or are newly written.**

### 4.2 What it costs `oracle-core` — nothing it charter-cares about

**Zero new dependencies.** `oracle-core`'s entire runtime dependency list is `bincode`
(`crates/oracle-core/Cargo.toml`). The moved code needs `SymbolTable` — **already `oracle_core::symbols`** —
plus `String`/`Vec`. Nothing else. The charter phrase §D worried about is *"deterministic, no-I/O"*, and this
code is both.

**Precedent, twice over.** `oracle_core::symbols` is a listing *parser* — equally "not emulation", already
there. And `oracle_core::render::sprite_tile_at` was moved into core for **precisely this reason**: `pick.rs`
had a local copy, and the copy was the problem (`pick.rs:27-33`). The same argument reaches the same answer
here, which is why "no second copy" is not a constraint to work around but the thing the split exists to
honour.

### 4.3 The two real costs

1. **`FieldSpec`'s opacity must be preserved deliberately.** `decoders.rs:59-62` defends every member being
   private: *"a caller can hold a resolved field and hand it back to be decoded but cannot read an offset out
   of it and go read the bus itself."* If aether keeps `field_value`, it needs `Repr` and the offsets public —
   which throws that property away. **The fix costs one small enum**: core returns
   `FieldValue { U(u32), I(i64), Hex(u32, width) }` and aether maps it to `Value`. Offsets stay private, the
   one-place-fields-are-interpreted rule survives, and the skin does only what a skin should.
2. **The error type.** Every refusal in `derive` funnels through `no_layout(spec, table, why)` with a plain
   `&str` reason (`:547-563`), plus one `NO_SYMBOLS_LOADED`. So core's error is a two-variant enum
   — `NoSymbols` and `NoLayout { engine, missing: Vec<&'static str>, why: String }` — and aether needs exactly
   one `From` impl to reproduce today's codes and `error.data` payloads byte-for-byte. **The wire does not
   move.** Two accessors also need adding (`base_addr`, `detected_from`) for `to_json` to keep working.

### 4.4 Why it is parked

§2.2's recommendation is a sentence in `pick.rs` that reads no memory. The split buys a shared decoder, and
until §3's ask lands there is nothing to ask it — §D's own words, and they are right. **Do the split as the
first half of the parcel that consumes `Sprite_Owner`, not before.**

---

## 5. Corrections — to §D and to the brief

**5.1 — The object record is `$50` (80) bytes, not 64.** The brief says *"the engine's own 64-byte object
records"*; 64 is the legacy Sonic-2 shape. aeon's `Sst` is `$50` (`aeon/engine/objects/sst.emp`, fields
`$00-$4F`), and our own decoder already knows it: `decoders.rs:172` `table_slot_bytes: 0x50`, cross-checked
against a stride **measured** from `Player_2 − Player_1` (`:174`, `:394-424`). Worth flagging because the
record was `$52` three weeks ago (`decoders.rs:392`) — a hardcoded size here would already have been wrong
once.

**5.2 — aeon is not hand-written 68000 assembly any more.** The brief says *"68000 assembly"*. The engine is
written in **`.emp`**, Sigil's language, which lowers to 68000 — `sprites.emp` is `proc`/`comptime fn`/`asm{}`
blocks, and the emit loop is a comptime-spliced skeleton with four flip instantiations. This matters
practically: a grep for `--include=*.asm` finds almost nothing, and the emit path is *generated*, so an ask
that says "add a `move` to the inner loop" must name the splice (`size_link`), not a line of assembly.

**5.3 — §D's `--no-default-features` asymmetry does not exist, and this is the correction that changes a
decision.** §D says *"`oracle-aether` is an **optional** dependency of `oracle-frontend`, so a
`--no-default-features` player could not identify objects at all — a feature-gated panel answer is a second
behaviour, not a second build."* The optionality is real (`crates/oracle-frontend/Cargo.toml`,
`oracle-aether = { … optional = true }`, `default = ["audio","gamepad","aether"]`). **The consequence is not.**
Everything an object decode needs is already present in *both* builds, with no `#[cfg]` anywhere near it:

* `mod symbol_file` (`main.rs:259`), `mod symbol_watch` (`:262`) and `mod pick` (`:289`) are **ungated** —
  only `mod bus` is `#[cfg]`-switched (`:280-285`).
* the symbol table is loaded unconditionally at `main.rs:887` and is a live local at the click site
  (`main.rs:1166`);
* `System::ram()` is core (`crates/oracle-core/src/system.rs:833`) and is already read from the frontend in
  both builds (`main.rs:1127`, `SymbolWatch::arm(…, sys.ram())`);
* `SymbolTable` is core (`crates/oracle-core/src/symbols.rs`).

So once the decode is in `oracle-core` — which is where the split puts it anyway — **`pick.rs` calls it
directly and `oracle-aether` is not involved at all**. There is no feature gate, no second behaviour, and no
second build. §D's ground 2 is a *refactor cost*, priced in §4; it was never an availability problem.

**5.4 — §D's ground 1 is right in its conclusion and wrong in its reason** (§1.4). "Replaying the walk … is
game code we do not model" implies it cannot be done. It can; every input is in RAM and in the listing. The
correct reason to refuse is that a replay is an undetectably-divergent second implementation, which is a
sharper argument and the one that also rules out the *cheaper* partial replays someone will propose next.

**5.5 — `sprite_piece_count` is not just "a count, not a range"** (both §D and the brief). It is a **pre-walk
prediction** (`sst.emp:59`, written only by `frames.emp:66` / `load_object.emp:83`, read only at
`sprites.emp:276`), so it is nonzero for objects that emitted nothing. Anyone tempted by a prefix-sum
reconstruction should know that before they start.

---

## 6. Firsthand vs carried, and what is open

**Verified firsthand (I read the file):** every `oracle` citation in §4 and §5.3, including the 42-line JSON
touchpoint count, the manifests, and the ungated module list. In `aeon`: `sprites.emp` lines 1-470 and 660-730
(`Draw_Sprite`, `Render_Sprites`'s band walk / parity / cap / budget / sibling walk / mask+ring calls / commit,
`emit_piece_loop`'s mid-object `dbeq`, `Emit_ObjectPieces`'s preserved-`a1`/`a6` contract); `sst.emp:30-90` in
full; `ram.emp:318-330` and `:625-660`; `rings.emp:1-40` and `:140-200`; the three `sprite_piece_count` write
sites by grep; and the four symbol rows in `s4.debug.lst:5257-5260`.

**Carried on the recon agent's word** (`aeon` read-only sub-agent, whose findings agreed with mine everywhere
they overlapped): the exhaustive negative grep for owner-table-shaped names across `engine/` + `games/`; the
`s4.lst` addresses and `Player_1`/`Player_2` stride rows; the frame-ordering call sites in
`ojz_scroll_test.emp` / `object_test_state.emp` / `demo_state.emp`; the `buffers.emp` VBlank enqueue lines;
and the `sprites.emp` line numbers outside the windows I read myself.

**Open / not settled here:**

* ⟨RUNTIME⟩ **Nothing in this document required a running machine, and none was used.** The one thing a
  foreground pass would add is measuring how often the ≤1-frame skew of §3 actually bites — i.e. how often a
  click lands between `Render_Sprites` and the VBlank enqueue. It does not change the design (the
  `Sprite_Table_Buffer` byte-compare closes it either way), only how often the picker says "rebuilt since
  drawn".
* **Ownership call, not mine:** whether `oracle-core` is the right home for a *game-engine* field catalogue.
  §4.2 argues yes on two precedents and zero dependency cost, but the charter is the owner's to read.
* **Not priced:** what `Sprite_Owner` would cost aeon in *their* review/verification budget (their byte-identity
  gates, replay hashes — `replay.emp` hashes an SST byte range that includes `$25`). The cycle and RAM figures
  in §3 are mine and are firsthand; the process cost is theirs to state.
* **Deliberately not done:** no code was written, no split was performed, `pick.rs` is untouched.
